//! Local restricted differential runtime candidate, not Runen framework code.
use crate::contract::{Data, Dirty, Key, Obj, Packet, packet};
use crate::generated;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

/// Instrumented costs for the exact same TDE update algorithm.
/// These timer readings are diagnostics, not unbiased benchmark latencies.
#[derive(Clone, Copy, Default, Debug)]
pub struct TdeApplyProfile {
    pub snapshot_ns: u128,
    pub dirty_objects_ns: u128,
    pub sources_ns: u128,
    pub affected_ns: u128,
    pub publication_ns: u128,
}

fn record_stage(clock: &mut Option<Instant>) -> u128 {
    let start = clock.as_mut().expect("profile clock exists");
    let elapsed = start.elapsed().as_nanos();
    *start = Instant::now();
    elapsed
}

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
        self.apply_internal::<false>(before, after, dirty);
    }

    pub fn apply_profiled(
        &mut self,
        before: &Data,
        after: &Data,
        dirty: &Dirty,
    ) -> TdeApplyProfile {
        self.apply_internal::<true>(before, after, dirty)
    }

    fn apply_internal<const PROFILE: bool>(
        &mut self,
        before: &Data,
        after: &Data,
        dirty: &Dirty,
    ) -> TdeApplyProfile {
        let mut profile = TdeApplyProfile::default();
        let mut clock = if PROFILE { Some(Instant::now()) } else { None };
        // Snapshot only reverse-index groups impacted by this edit.
        let previous_groups = generated::snapshot_groups(&self.inverse, &dirty.materials);
        if PROFILE {
            profile.snapshot_ns = record_stage(&mut clock);
        }
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
        if PROFILE {
            profile.dirty_objects_ns = record_stage(&mut clock);
        }
        for &material in &dirty.materials {
            if let Some(&color) = after.materials.get(&material) {
                self.materials.insert(material, color);
            }
        }
        for &slot in &dirty.objects {
            if let Some(&generation) = after.versions.get(&slot) {
                self.versions.insert(slot, generation);
            } else {
                self.versions.remove(&slot);
            }
        }
        if PROFILE {
            profile.sources_ns = record_stage(&mut clock);
        }
        let affected = generated::affected(
            &previous_groups,
            &self.inverse,
            &affected_objects,
            &dirty.materials,
        );
        if PROFILE {
            profile.affected_ns = record_stage(&mut clock);
        }
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
        if PROFILE {
            profile.publication_ns = record_stage(&mut clock);
        }
        profile
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
