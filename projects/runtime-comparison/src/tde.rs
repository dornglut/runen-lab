//! Local restricted differential runtime candidate, not Runen framework code.
use crate::contract::{Data, Dirty, Key, Obj, Packet, packet};
use crate::generated;
use std::collections::{BTreeMap, BTreeSet};

pub struct Tde {
    objects: BTreeMap<u32, Obj>,
    materials: BTreeMap<u32, u32>,
    versions: BTreeMap<u32, u32>,
    inverse: BTreeMap<u32, BTreeSet<Key>>,
    published: BTreeMap<Key, Packet>,
}

impl Tde {
    pub fn new(initial: &Data) -> Self {
        let mut result = Self {
            objects: BTreeMap::new(),
            materials: initial.materials.clone(),
            versions: initial.versions.clone(),
            inverse: BTreeMap::new(),
            published: BTreeMap::new(),
        };
        result.apply(
            &Data {
                versions: BTreeMap::new(),
                objects: BTreeMap::new(),
                materials: initial.materials.clone(),
            },
            initial,
            &Dirty {
                objects: initial.objects.keys().copied().collect(),
                materials: initial.materials.keys().copied().collect(),
            },
        );
        result
    }

    pub fn apply(&mut self, before: &Data, after: &Data, dirty: &Dirty) {
        // Snapshot only reverse-index groups impacted by this edit.
        let previous_groups = generated::snapshot_groups(&self.inverse, &dirty.materials);
        let mut affected_objects = BTreeSet::new();
        for &slot in &dirty.objects {
            let prior = before.objects.get(&slot).map(|&obj| {
                (
                    Key {
                        slot,
                        r#gen: obj.r#gen,
                    },
                    obj,
                )
            });
            let next = after.objects.get(&slot).map(|&obj| {
                (
                    Key {
                        slot,
                        r#gen: obj.r#gen,
                    },
                    obj,
                )
            });
            if let Some((key, _)) = prior {
                affected_objects.insert(key);
            }
            if let Some((key, _)) = next {
                affected_objects.insert(key);
            }
            generated::update_inverse(&mut self.inverse, prior, next);
            match next {
                Some((_, obj)) => {
                    self.objects.insert(slot, obj);
                }
                None => {
                    self.objects.remove(&slot);
                }
            }
        }
        for &material in &dirty.materials {
            if let Some(&color) = after.materials.get(&material) {
                self.materials.insert(material, color);
            }
        }
        self.versions.clone_from(&after.versions);
        let affected = generated::affected(
            &previous_groups,
            &self.inverse,
            &affected_objects,
            &dirty.materials,
        );
        for key in affected {
            match self
                .objects
                .get(&key.slot)
                .filter(|obj| obj.r#gen == key.r#gen)
                .copied()
            {
                Some(obj) => {
                    let value = packet(obj, self.materials[&obj.material]);
                    if self.published.get(&key) != Some(&value) {
                        self.published.insert(key, value);
                    }
                }
                None => {
                    self.published.remove(&key);
                }
            }
        }
    }

    pub fn data(&self) -> Data {
        Data {
            objects: self.objects.clone(),
            materials: self.materials.clone(),
            versions: self.versions.clone(),
        }
    }
    pub fn packets(&self) -> BTreeMap<Key, Packet> {
        self.published.clone()
    }
}
