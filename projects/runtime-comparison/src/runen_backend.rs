//! Actual public RunenECS components, directed relations, queries and scheduler.
use crate::contract::{Data, Dirty, Key, Obj, Packet, packet};
use runen_ecs::prelude::*;
use std::collections::BTreeMap;

#[derive(Component, Clone, Copy, Eq, PartialEq)]
struct ObjectRecord {
    slot: u32,
    obj: Obj,
}
#[derive(Component, Clone, Copy, Eq, PartialEq)]
struct MaterialColor(u32);
#[derive(Component, Clone, Copy, Eq, PartialEq)]
struct PacketComponent(Packet);
#[derive(Resource, Default)]
struct Published(BTreeMap<Entity, (Key, Packet)>);
#[derive(Resource, Default)]
struct MaterialEntities(BTreeMap<u32, Entity>);
#[derive(Resource, Default)]
struct DirtyMaterials(Vec<u32>);

struct BelongsTo;
impl Relation for BelongsTo {
    type Kind = Directed;
}
#[derive(ScheduleLabel)]
struct Update;
#[derive(SystemSet)]
struct ObjectStage;
#[derive(SystemSet)]
struct MaterialStage;

fn changed_objects(
    mut objects: Query<(&ObjectRecord, &mut PacketComponent), Changed<ObjectRecord>>,
    ids: Res<MaterialEntities>,
    mut colors: Query<&MaterialColor>,
) {
    for (object, output) in objects.iter() {
        let mat = ids.0[&object.obj.material];
        let color = colors.get(mat).expect("material exists").0;
        let next = packet(object.obj, color);
        if output.0 != next {
            output.0 = next;
        }
    }
}

fn changed_materials(
    dirty: Res<DirtyMaterials>,
    ids: Res<MaterialEntities>,
    relations: Relations<BelongsTo>,
    mut colors: Query<&MaterialColor>,
    mut objects: Query<(&ObjectRecord, &mut PacketComponent)>,
) {
    for &material in &dirty.0 {
        let entity = ids.0[&material];
        let color = colors.get(entity).expect("material exists").0;
        let users = relations.targets(entity).expect("valid material entity");
        for object_entity in users.iter() {
            if let Some((object, output)) = objects.get(object_entity) {
                let next = packet(object.obj, color);
                if output.0 != next {
                    output.0 = next;
                }
            }
        }
    }
}

fn publish(
    mut changed: Query<(Entity, &PacketComponent), Changed<PacketComponent>>,
    mut objects: Query<&ObjectRecord>,
    mut removed: RemovedQuery<PacketComponent>,
    mut output: ResMut<Published>,
) {
    for removed in removed.iter() {
        output.0.remove(&removed.entity());
    }
    for (entity, packet) in changed.iter() {
        let object = objects.get(entity).expect("changed packet has an object");
        output.0.insert(
            entity,
            (
                Key {
                    slot: object.slot,
                    r#gen: object.obj.r#gen,
                },
                packet.0,
            ),
        );
    }
}

pub struct RunenBackend {
    world: World,
    runtime: Runtime,
    objects: BTreeMap<u32, Entity>,
    materials: BTreeMap<u32, Entity>,
    versions: BTreeMap<u32, u32>,
}

impl RunenBackend {
    pub fn new(initial: &Data) -> Self {
        let mut world = World::new();
        let materials: BTreeMap<_, _> = initial
            .materials
            .iter()
            .map(|(&id, &color)| {
                (
                    id,
                    world.spawn(MaterialColor(color)).expect("spawn material"),
                )
            })
            .collect();
        let objects: BTreeMap<_, _> = initial
            .objects
            .iter()
            .map(|(&slot, &obj)| {
                let entity = world
                    .spawn((
                        ObjectRecord { slot, obj },
                        PacketComponent(Packet::default()),
                    ))
                    .expect("spawn object");
                world
                    .relations_mut::<BelongsTo>()
                    .insert(materials[&obj.material], entity)
                    .expect("valid relation");
                (slot, entity)
            })
            .collect();
        world.insert_resource(Published::default());
        world.insert_resource(MaterialEntities(materials.clone()));
        world.insert_resource(DirtyMaterials(initial.materials.keys().copied().collect()));
        let mut runtime = Runtime::new();
        runtime
            .add_systems(
                Update,
                (
                    changed_objects.in_set(ObjectStage),
                    changed_materials.in_set(MaterialStage).after(ObjectStage),
                    publish.after(MaterialStage),
                ),
            )
            .expect("valid schedule");
        runtime
            .run_schedule::<Update>(&mut world)
            .expect("initial schedule");
        Self {
            world,
            runtime,
            objects,
            materials,
            versions: initial.versions.clone(),
        }
    }

    pub fn apply(&mut self, before: &Data, after: &Data, dirty: &Dirty) {
        for &id in &dirty.materials {
            self.world
                .insert(self.materials[&id], MaterialColor(after.materials[&id]))
                .expect("update material");
        }
        for &slot in &dirty.objects {
            match (before.objects.get(&slot), after.objects.get(&slot)) {
                (None, Some(&obj)) => {
                    let entity = self
                        .world
                        .spawn((
                            ObjectRecord { slot, obj },
                            PacketComponent(Packet::default()),
                        ))
                        .expect("spawn object");
                    self.world
                        .relations_mut::<BelongsTo>()
                        .insert(self.materials[&obj.material], entity)
                        .expect("insert relation");
                    self.objects.insert(slot, entity);
                }
                (Some(_), None) => {
                    let entity = self.objects.remove(&slot).expect("mapped live object");
                    self.world.despawn(entity).expect("despawn object");
                }
                (Some(&old), Some(&obj)) => {
                    let entity = self.objects[&slot];
                    if old.material != obj.material {
                        self.world
                            .relations_mut::<BelongsTo>()
                            .remove(self.materials[&old.material], entity)
                            .expect("remove relation");
                        self.world
                            .relations_mut::<BelongsTo>()
                            .insert(self.materials[&obj.material], entity)
                            .expect("insert relation");
                    }
                    if old != obj {
                        self.world
                            .insert(entity, ObjectRecord { slot, obj })
                            .expect("update object");
                    }
                }
                (None, None) => {}
            }
        }
        self.world
            .insert_resource(DirtyMaterials(dirty.materials.iter().copied().collect()));
        for &slot in &dirty.objects {
            if let Some(&generation) = after.versions.get(&slot) {
                self.versions.insert(slot, generation);
            } else {
                self.versions.remove(&slot);
            }
        }
        self.runtime
            .run_schedule::<Update>(&mut self.world)
            .expect("schedule");
    }

    pub fn data(&self) -> Data {
        let materials = self
            .materials
            .iter()
            .map(|(&id, &entity)| {
                (
                    id,
                    self.world
                        .require::<MaterialColor>(entity)
                        .expect("material exists")
                        .0,
                )
            })
            .collect();
        let objects = self
            .objects
            .iter()
            .map(|(&slot, &entity)| {
                (
                    slot,
                    self.world
                        .require::<ObjectRecord>(entity)
                        .expect("object exists")
                        .obj,
                )
            })
            .collect();
        Data {
            versions: self.versions.clone(),
            objects,
            materials,
        }
    }
    pub fn packets(&self) -> BTreeMap<Key, Packet> {
        self.world
            .resource::<Published>()
            .expect("publisher")
            .0
            .values()
            .map(|&(key, packet)| (key, packet))
            .collect()
    }
}
