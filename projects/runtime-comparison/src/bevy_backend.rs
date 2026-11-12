//! Actual Bevy 0.20.1 ECS/app implementation using native relationships and change ticks.
use crate::contract::{Data, Dirty, Key, Obj, Packet, packet};
use bevy_app::prelude::*;
use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::IntoScheduleConfigs;
use std::collections::BTreeMap;

#[derive(Component, Clone, Copy, Eq, PartialEq)]
struct ObjectRecord {
    slot: u32,
    obj: Obj,
}
#[derive(Component, Clone, Copy, Eq, PartialEq)]
struct MaterialColor(u32);
#[derive(Component, Debug)]
#[relationship(relationship_target = MaterialUsers)]
struct MaterialOf(Entity);
#[derive(Component, Debug)]
#[relationship_target(relationship = MaterialOf)]
struct MaterialUsers(Vec<Entity>);
#[derive(Component, Clone, Copy, Eq, PartialEq)]
struct PacketComponent(Packet);
#[derive(Resource, Default)]
struct Published(BTreeMap<Entity, (Key, Packet)>);

fn changed_objects(
    materials: Query<&MaterialColor>,
    mut objects: Query<
        (&ObjectRecord, &MaterialOf, &mut PacketComponent),
        Or<(Changed<ObjectRecord>, Changed<MaterialOf>)>,
    >,
) {
    for (obj, relation, mut output) in &mut objects {
        let color = materials.get(relation.0).expect("validated material").0;
        output.set_if_neq(PacketComponent(packet(obj.obj, color)));
    }
}

fn changed_materials(
    materials: Query<(&MaterialColor, Option<&MaterialUsers>), Changed<MaterialColor>>,
    mut objects: Query<(&ObjectRecord, &mut PacketComponent)>,
) {
    for (material, users) in &materials {
        if let Some(users) = users {
            for &entity in &users.0 {
                if let Ok((obj, mut output)) = objects.get_mut(entity) {
                    output.set_if_neq(PacketComponent(packet(obj.obj, material.0)));
                }
            }
        }
    }
}

fn publish(
    changed: Query<(Entity, &ObjectRecord, &PacketComponent), Changed<PacketComponent>>,
    mut removed: RemovedComponents<PacketComponent>,
    mut output: ResMut<Published>,
) {
    for entity in removed.read() {
        output.0.remove(&entity);
    }
    for (entity, object, packet) in &changed {
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

pub struct BevyBackend {
    app: App,
    objects: BTreeMap<u32, Entity>,
    materials: BTreeMap<u32, Entity>,
    versions: BTreeMap<u32, u32>,
}

impl BevyBackend {
    pub fn new(initial: &Data) -> Self {
        let mut app = App::new();
        app.init_resource::<Published>().add_systems(
            Update,
            (changed_objects, changed_materials, publish).chain(),
        );
        let materials: BTreeMap<_, _> = initial
            .materials
            .iter()
            .map(|(&id, &color)| (id, app.world_mut().spawn(MaterialColor(color)).id()))
            .collect();
        let objects = initial
            .objects
            .iter()
            .map(|(&slot, &obj)| {
                let entity = app
                    .world_mut()
                    .spawn((
                        ObjectRecord { slot, obj },
                        MaterialOf(materials[&obj.material]),
                        PacketComponent(Packet::default()),
                    ))
                    .id();
                (slot, entity)
            })
            .collect();
        app.update();
        Self {
            app,
            objects,
            materials,
            versions: initial.versions.clone(),
        }
    }

    pub fn apply(&mut self, before: &Data, after: &Data, dirty: &Dirty) {
        for &id in &dirty.materials {
            let entity = self.materials[&id];
            self.app
                .world_mut()
                .entity_mut(entity)
                .get_mut::<MaterialColor>()
                .expect("material exists")
                .set_if_neq(MaterialColor(after.materials[&id]));
        }
        for &slot in &dirty.objects {
            match (before.objects.get(&slot), after.objects.get(&slot)) {
                (None, Some(&obj)) => {
                    let entity = self
                        .app
                        .world_mut()
                        .spawn((
                            ObjectRecord { slot, obj },
                            MaterialOf(self.materials[&obj.material]),
                            PacketComponent(Packet::default()),
                        ))
                        .id();
                    self.objects.insert(slot, entity);
                }
                (Some(_), None) => {
                    let entity = self.objects.remove(&slot).expect("mapped live object");
                    self.app.world_mut().entity_mut(entity).despawn();
                }
                (Some(&old), Some(&obj)) => {
                    let entity = self.objects[&slot];
                    self.app
                        .world_mut()
                        .entity_mut(entity)
                        .get_mut::<ObjectRecord>()
                        .expect("object component")
                        .set_if_neq(ObjectRecord { slot, obj });
                    if old.material != obj.material {
                        self.app
                            .world_mut()
                            .entity_mut(entity)
                            .insert(MaterialOf(self.materials[&obj.material]));
                    }
                }
                (None, None) => {}
            }
        }
        self.versions.clone_from(&after.versions);
        self.app.update();
    }

    pub fn data(&self) -> Data {
        let world = self.app.world();
        let materials = self
            .materials
            .iter()
            .map(|(&id, &entity)| {
                (
                    id,
                    world
                        .get::<MaterialColor>(entity)
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
                    world
                        .get::<ObjectRecord>(entity)
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
        self.app
            .world()
            .resource::<Published>()
            .0
            .values()
            .map(|&(key, packet)| (key, packet))
            .collect()
    }
}
