//! Project-local candidate storage/propagation algorithms.
//! These are not framework ECS implementations nor reusable Runen APIs.
use crate::contract::{Data, Dirty, Key, Obj, Packet, packet};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Strategy {
    /// Only visit modified objects and objects whose material changed.
    Indexed,
    /// Rebuild all packet values if estimated affected work is sufficiently dense.
    Adaptive,
}

impl Strategy {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Indexed => "compact-indexed",
            Self::Adaptive => "compact-adaptive",
        }
    }
}

#[derive(Clone, Copy)]
struct Entry {
    slot: u32,
    object: Obj,
    group_position: usize,
    packet: Packet,
}

/// Dense storage with a slot -> index directory and material -> slots groups.
/// A slot may be arbitrarily large: memory scales with live objects rather
/// than the maximum slot ID. All identity generations are retained separately.
pub struct CompactTde {
    strategy: Strategy,
    entries: Vec<Entry>,
    slots: HashMap<u32, usize>,
    users: HashMap<u32, Vec<u32>>,
    materials: BTreeMap<u32, u32>,
    versions: BTreeMap<u32, u32>,
}

impl CompactTde {
    pub fn new(initial: &Data, strategy: Strategy) -> Self {
        let mut result = Self {
            strategy,
            entries: Vec::with_capacity(initial.objects.len()),
            slots: HashMap::with_capacity(initial.objects.len()),
            users: HashMap::new(),
            materials: initial.materials.clone(),
            versions: initial.versions.clone(),
        };
        for (&slot, &object) in &initial.objects {
            result.insert(slot, object);
        }
        result
    }

    pub const fn label(&self) -> &'static str {
        self.strategy.label()
    }

    fn insert(&mut self, slot: u32, object: Obj) {
        let users = self.users.entry(object.material).or_default();
        let group_position = users.len();
        users.push(slot);
        let index = self.entries.len();
        let old = self.slots.insert(slot, index);
        assert!(old.is_none(), "unique live slot");
        self.entries.push(Entry {
            slot,
            object,
            group_position,
            packet: packet(object, self.materials[&object.material]),
        });
    }

    fn detach(&mut self, material: u32, position: usize) {
        let users = self.users.get_mut(&material).expect("material group");
        users.swap_remove(position);
        if let Some(&moved_slot) = users.get(position) {
            let index = self.slots[&moved_slot];
            self.entries[index].group_position = position;
        }
        if users.is_empty() {
            self.users.remove(&material);
        }
    }

    fn delete(&mut self, slot: u32) {
        let index = self.slots.remove(&slot).expect("live slot");
        let removed = self.entries.swap_remove(index);
        if let Some(moved) = self.entries.get(index) {
            self.slots.insert(moved.slot, index);
        }
        self.detach(removed.object.material, removed.group_position);
    }

    fn replace(&mut self, slot: u32, object: Obj, refresh: bool) {
        let index = self.slots[&slot];
        let old = self.entries[index];
        if old.object.material != object.material {
            self.detach(old.object.material, old.group_position);
            let new_group = self.users.entry(object.material).or_default();
            self.entries[index].group_position = new_group.len();
            new_group.push(slot);
        }
        let entry = &mut self.entries[index];
        entry.object = object;
        if refresh {
            entry.packet = packet(object, self.materials[&object.material]);
        }
    }

    fn estimated_affected(&self, dirty: &Dirty) -> usize {
        let fanout = dirty.materials.iter().fold(0usize, |count, material| {
            count.saturating_add(self.users.get(material).map_or(0, Vec::len))
        });
        dirty.objects.len().saturating_add(fanout)
    }

    pub fn apply(&mut self, _before: &Data, after: &Data, dirty: &Dirty) {
        // This is an explicit initial heuristic, not a tuned or optimal cutoff.
        let bulk = self.strategy == Strategy::Adaptive
            && !self.entries.is_empty()
            && self.estimated_affected(dirty).saturating_mul(4) >= self.entries.len();

        for &material in &dirty.materials {
            self.materials.insert(material, after.materials[&material]);
        }

        for &slot in &dirty.objects {
            match (
                self.slots.get(&slot).copied(),
                after.objects.get(&slot).copied(),
            ) {
                (Some(_), Some(object)) => self.replace(slot, object, !bulk),
                (None, Some(object)) => self.insert(slot, object),
                (Some(_), None) => self.delete(slot),
                (None, None) => {}
            }
            if let Some(&version) = after.versions.get(&slot) {
                self.versions.insert(slot, version);
            } else {
                self.versions.remove(&slot);
            }
        }

        if bulk {
            for entry in &mut self.entries {
                entry.packet = packet(entry.object, self.materials[&entry.object.material]);
            }
        } else {
            // Traverse current reverse groups directly: no snapshots or
            // per-material copies or ordered affected-key set construction.
            for &material in &dirty.materials {
                if let Some(users) = self.users.get(&material) {
                    let color = self.materials[&material];
                    for &slot in users {
                        let index = self.slots[&slot];
                        let entry = &mut self.entries[index];
                        entry.packet = packet(entry.object, color);
                    }
                }
            }
        }
    }

    pub fn data(&self) -> Data {
        Data {
            versions: self.versions.clone(),
            materials: self.materials.clone(),
            objects: self
                .entries
                .iter()
                .map(|entry| (entry.slot, entry.object))
                .collect(),
        }
    }

    pub fn packets(&self) -> BTreeMap<Key, Packet> {
        self.entries
            .iter()
            .map(|entry| {
                (
                    Key {
                        slot: entry.slot,
                        r#gen: entry.object.r#gen,
                    },
                    entry.packet,
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{CompactTde, Strategy};
    use crate::contract::{Data, Dirty, Edit, propose};

    #[test]
    fn sparse_slot_and_identity_reuse() {
        for strategy in [Strategy::Indexed, Strategy::Adaptive] {
            let before = Data::initial();
            let mut engine = CompactTde::new(&before, strategy);
            let first = propose(
                &before,
                &[Edit::Spawn {
                    slot: 4_000_000_000,
                    r#gen: 1,
                    x: 9,
                    y: 7,
                    radius: 2,
                    material: 0,
                }],
            )
            .expect("large sparse slot is allowed");
            engine.apply(&before, &first.data, &first.dirty);
            assert_eq!(engine.data(), first.data);
            assert_eq!(engine.packets(), first.data.packets());

            let next = propose(
                &first.data,
                &[Edit::Delete {
                    slot: 4_000_000_000,
                    r#gen: 1,
                }],
            )
            .expect("delete");
            engine.apply(&first.data, &next.data, &next.dirty);
            assert_eq!(engine.data(), next.data);
            assert_eq!(engine.packets(), next.data.packets());
            let reused = propose(
                &next.data,
                &[Edit::Spawn {
                    slot: 4_000_000_000,
                    r#gen: 2,
                    x: 9,
                    y: 7,
                    radius: 2,
                    material: 1,
                }],
            )
            .expect("new identity");
            engine.apply(&next.data, &reused.data, &reused.dirty);
            assert_eq!(engine.data(), reused.data);
            assert_eq!(engine.packets(), reused.data.packets());
        }
    }

    #[test]
    fn mixed_material_and_reassignment() {
        for strategy in [Strategy::Indexed, Strategy::Adaptive] {
            let before = Data::initial();
            let mut engine = CompactTde::new(&before, strategy);
            let first = propose(
                &before,
                &[
                    Edit::Reassign {
                        slot: 1,
                        r#gen: 1,
                        material: 2,
                    },
                    Edit::Recolor {
                        material: 2,
                        color: 0x0055_5599,
                    },
                ],
            )
            .expect("valid mixed transaction");
            engine.apply(&before, &first.data, &first.dirty);
            assert_eq!(engine.data(), first.data);
            assert_eq!(engine.packets(), first.data.packets());
            let dirty = Dirty::default();
            engine.apply(&first.data, &first.data, &dirty);
            assert_eq!(engine.packets(), first.data.packets());
        }
    }
}
