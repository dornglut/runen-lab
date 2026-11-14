//! Source-only native Worlds for genuine no-derived-cache application comparisons.
//! Domain objects and relations are stored in Bevy or RunenECS; derived packets
//! are computed on read and are never stored as components or resources.
use crate::contract::{Data, Dirty, Key, Obj, Packet, packet};
use std::collections::BTreeMap;

mod bevy {
    use super::*;
    use bevy_ecs::prelude::*;

    #[derive(Component)]
    struct Object(Obj);

    #[derive(Component)]
    struct MaterialColor(u32);

    #[derive(Component, Debug)]
    #[relationship(relationship_target = MaterialUsers)]
    struct MaterialOf(Entity);

    #[derive(Component, Debug)]
    #[relationship_target(relationship = MaterialOf)]
    struct MaterialUsers(Vec<Entity>);

    pub struct ReadOnly {
        world: World,
        objects: BTreeMap<u32, Entity>,
        materials: BTreeMap<u32, Entity>,
    }

    impl ReadOnly {
        pub fn new(data: &Data) -> Self {
            let mut world = World::new();
            let materials: BTreeMap<_, _> = data
                .materials
                .iter()
                .map(|(&id, &color)| (id, world.spawn(MaterialColor(color)).id()))
                .collect();
            let objects = data
                .objects
                .iter()
                .map(|(&slot, &obj)| {
                    let entity = world
                        .spawn((Object(obj), MaterialOf(materials[&obj.material])))
                        .id();
                    (slot, entity)
                })
                .collect();

            Self {
                world,
                objects,
                materials,
            }
        }

        pub fn apply(&mut self, before: &Data, after: &Data, dirty: &Dirty) {
            for &id in &dirty.materials {
                let entity = self.materials[&id];
                self.world
                    .entity_mut(entity)
                    .insert(MaterialColor(after.materials[&id]));
            }
            for &slot in &dirty.objects {
                match (before.objects.get(&slot), after.objects.get(&slot)) {
                    (None, Some(&obj)) => {
                        let entity = self
                            .world
                            .spawn((Object(obj), MaterialOf(self.materials[&obj.material])))
                            .id();
                        self.objects.insert(slot, entity);
                    }
                    (Some(_), None) => {
                        let entity = self.objects.remove(&slot).expect("live object");
                        self.world.entity_mut(entity).despawn();
                    }
                    (Some(&prior), Some(&next)) => {
                        let entity = self.objects[&slot];
                        if prior != next {
                            self.world.entity_mut(entity).insert(Object(next));
                        }
                        if prior.material != next.material {
                            self.world
                                .entity_mut(entity)
                                .insert(MaterialOf(self.materials[&next.material]));
                        }
                    }
                    (None, None) => {}
                }
            }
        }

        pub fn packets(&self) -> BTreeMap<Key, Packet> {
            self.objects
                .iter()
                .map(|(&slot, &entity)| {
                    let object = self.world.get::<Object>(entity).expect("object").0;
                    let material = self.world.get::<MaterialOf>(entity).expect("relation").0;
                    let color = self
                        .world
                        .get::<MaterialColor>(material)
                        .expect("material")
                        .0;
                    (
                        Key {
                            slot,
                            r#gen: object.r#gen,
                        },
                        packet(object, color),
                    )
                })
                .collect()
        }
    }
}

mod runen {
    use super::*;
    use runen_ecs::prelude::*;

    #[derive(Component)]
    struct Object(Obj);

    #[derive(Component)]
    struct MaterialColor(u32);

    struct MaterialOf;
    impl Relation for MaterialOf {
        type Kind = Directed;
    }

    pub struct ReadOnly {
        world: World,
        objects: BTreeMap<u32, Entity>,
        materials: BTreeMap<u32, Entity>,
    }

    impl ReadOnly {
        pub fn new(data: &Data) -> Self {
            let mut world = World::new();
            let materials: BTreeMap<_, _> = data
                .materials
                .iter()
                .map(|(&id, &color)| {
                    (id, world.spawn(MaterialColor(color)).expect("material"))
                })
                .collect();
            let objects = data
                .objects
                .iter()
                .map(|(&slot, &obj)| {
                    let entity = world.spawn(Object(obj)).expect("object");
                    world
                        .relations_mut::<MaterialOf>()
                        .insert(materials[&obj.material], entity)
                        .expect("relation");
                    (slot, entity)
                })
                .collect();
            Self {
                world,
                objects,
                materials,
            }
        }

        pub fn apply(&mut self, before: &Data, after: &Data, dirty: &Dirty) {
            for &id in &dirty.materials {
                self.world
                    .insert(self.materials[&id], MaterialColor(after.materials[&id]))
                    .expect("recolor");
            }
            for &slot in &dirty.objects {
                match (before.objects.get(&slot), after.objects.get(&slot)) {
                    (None, Some(&obj)) => {
                        let entity = self.world.spawn(Object(obj)).expect("object");
                        self.world
                            .relations_mut::<MaterialOf>()
                            .insert(self.materials[&obj.material], entity)
                            .expect("relation");
                        self.objects.insert(slot, entity);
                    }
                    (Some(_), None) => {
                        let entity = self.objects.remove(&slot).expect("live object");
                        self.world.despawn(entity).expect("despawn");
                    }
                    (Some(&prior), Some(&next)) => {
                        let entity = self.objects[&slot];
                        if prior.material != next.material {
                            let mut relations = self.world.relations_mut::<MaterialOf>();
                            relations
                                .remove(self.materials[&prior.material], entity)
                                .expect("unlink");
                            relations
                                .insert(self.materials[&next.material], entity)
                                .expect("relink");
                        }
                        if prior != next {
                            self.world.insert(entity, Object(next)).expect("update object");
                        }
                    }
                    (None, None) => {}
                }
            }
        }

        pub fn packets(&self) -> BTreeMap<Key, Packet> {
            let relations = self.world.relations::<MaterialOf>();
            self.objects
                .iter()
                .map(|(&slot, &entity)| {
                    let object = self.world.require::<Object>(entity).expect("object").0;
                    let material = relations
                        .sources(entity)
                        .expect("relation")
                        .iter()
                        .next()
                        .expect("source material");
                    let color = self
                        .world
                        .require::<MaterialColor>(material)
                        .expect("material")
                        .0;
                    (
                        Key {
                            slot,
                            r#gen: object.r#gen,
                        },
                        packet(object, color),
                    )
                })
                .collect()
        }
    }
}

pub use bevy::ReadOnly as BevyReadOnly;
pub use runen::ReadOnly as RunenReadOnly;
