//! Shared input semantics and independent expected-state digest generation.
//! This module is used for semantic replay ONLY. Never use it to time a backend.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Key {
    pub slot: u32,
    pub r#gen: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Obj {
    pub r#gen: u32,
    pub x: i64,
    pub y: i64,
    pub radius: i64,
    pub material: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Packet {
    pub l: i32,
    pub b: i32,
    pub r: i32,
    pub t: i32,
    pub color: u32,
}

pub fn packet(obj: Obj, color: u32) -> Packet {
    let x = i128::from(obj.x);
    let y = i128::from(obj.y);
    let r = i128::from(obj.radius);
    Packet {
        l: (x - r).try_into().expect("validated x-min"),
        b: (y - r).try_into().expect("validated y-min"),
        r: (x + r).try_into().expect("validated x-max"),
        t: (y + r).try_into().expect("validated y-max"),
        color,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Data {
    pub versions: BTreeMap<u32, u32>,
    pub objects: BTreeMap<u32, Obj>,
    pub materials: BTreeMap<u32, u32>,
}

impl Data {
    pub fn initial() -> Self {
        let versions = (0..5).map(|slot| (slot, 1)).collect();
        let objects = (0..5)
            .map(|slot| {
                (
                    slot,
                    Obj {
                        r#gen: 1,
                        x: slot as i64 * 10,
                        y: -(slot as i64 * 7),
                        radius: (slot % 4 + 1) as i64,
                        material: slot % 4,
                    },
                )
            })
            .collect();
        let materials = (0..4).map(|m| (m, 0x101010 + m * 0x222222)).collect();
        Self {
            versions,
            objects,
            materials,
        }
    }

    pub fn packets(&self) -> BTreeMap<Key, Packet> {
        self.objects
            .iter()
            .map(|(&slot, &obj)| {
                (
                    Key {
                        slot,
                        r#gen: obj.r#gen,
                    },
                    packet(obj, self.materials[&obj.material]),
                )
            })
            .collect()
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        for obj in self.objects.values() {
            if obj.radius < 0 || !self.materials.contains_key(&obj.material) {
                return Err("INVALID_OBJECT");
            }
            let x = i128::from(obj.x);
            let y = i128::from(obj.y);
            let radius = i128::from(obj.radius);
            for value in [x - radius, x + radius, y - radius, y + radius] {
                if i32::try_from(value).is_err() {
                    return Err("BOUNDS_OVERFLOW");
                }
            }
        }
        Ok(())
    }

    pub fn source_digest(&self) -> String {
        let mut hash = Sha256::new();
        for (&material, &color) in &self.materials {
            hash.update(material.to_be_bytes());
            hash.update(color.to_be_bytes());
        }
        for (&slot, &r#gen) in &self.versions {
            hash.update(slot.to_be_bytes());
            hash.update(r#gen.to_be_bytes());
        }
        for (&slot, &obj) in &self.objects {
            hash.update(slot.to_be_bytes());
            hash.update(obj.r#gen.to_be_bytes());
            hash.update(i32::try_from(obj.x).expect("validated x").to_be_bytes());
            hash.update(i32::try_from(obj.y).expect("validated y").to_be_bytes());
            hash.update(
                i32::try_from(obj.radius)
                    .expect("validated radius")
                    .to_be_bytes(),
            );
            hash.update(obj.material.to_be_bytes());
        }
        format!("{:x}", hash.finalize())
    }
}

pub fn packet_digest(packets: &BTreeMap<Key, Packet>) -> String {
    let mut hash = Sha256::new();
    for (key, p) in packets {
        hash.update(key.slot.to_be_bytes());
        hash.update(key.r#gen.to_be_bytes());
        for v in [p.l, p.b, p.r, p.t] {
            hash.update(v.to_be_bytes());
        }
        hash.update(p.color.to_be_bytes());
    }
    format!("{:x}", hash.finalize())
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Edit {
    Recolor {
        material: u32,
        color: u32,
    },
    Spawn {
        slot: u32,
        r#gen: u32,
        x: i64,
        y: i64,
        radius: i64,
        material: u32,
    },
    Delete {
        slot: u32,
        r#gen: u32,
    },
    Move {
        slot: u32,
        r#gen: u32,
        x: i64,
        y: i64,
    },
    Radius {
        slot: u32,
        r#gen: u32,
        radius: i64,
    },
    Reassign {
        slot: u32,
        r#gen: u32,
        material: u32,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Dirty {
    pub objects: BTreeSet<u32>,
    pub materials: BTreeSet<u32>,
}

pub struct Proposed {
    pub data: Data,
    pub dirty: Dirty,
}

/// The independent oracle's exact batch semantics. This staging path is not
/// part of performance measurement; all three backends receive the same plan.
pub fn propose(before: &Data, edits: &[Edit]) -> Result<Proposed, &'static str> {
    let mut data = before.clone();
    let mut dirty = Dirty::default();
    let mut seen = BTreeSet::new();
    for edit in edits {
        let target = match *edit {
            Edit::Recolor { material, .. } => (0u8, material),
            Edit::Spawn { slot, .. }
            | Edit::Delete { slot, .. }
            | Edit::Move { slot, .. }
            | Edit::Radius { slot, .. }
            | Edit::Reassign { slot, .. } => (1u8, slot),
        };
        if !seen.insert(target) {
            return Err("DUPLICATE_TARGET");
        }
        match *edit {
            Edit::Recolor { material, color } => {
                if !data.materials.contains_key(&material) {
                    return Err("UNKNOWN_MATERIAL");
                }
                if color > 0x00ff_ffff {
                    return Err("INVALID_COLOR");
                }
                data.materials.insert(material, color);
                dirty.materials.insert(material);
            }
            Edit::Spawn {
                slot,
                r#gen,
                x,
                y,
                radius,
                material,
            } => {
                if data.objects.contains_key(&slot)
                    || r#gen
                        != data
                            .versions
                            .get(&slot)
                            .copied()
                            .unwrap_or(0)
                            .saturating_add(1)
                {
                    return Err("INVALID_SPAWN");
                }
                data.objects.insert(
                    slot,
                    Obj {
                        r#gen,
                        x,
                        y,
                        radius,
                        material,
                    },
                );
                data.versions.insert(slot, r#gen);
                dirty.objects.insert(slot);
            }
            Edit::Delete { slot, r#gen } => {
                if data.objects.get(&slot).is_none_or(|obj| obj.r#gen != r#gen) {
                    return Err("STALE_OR_UNKNOWN_OBJECT");
                }
                data.objects.remove(&slot);
                dirty.objects.insert(slot);
            }
            Edit::Move { slot, r#gen, x, y } => {
                let obj = data
                    .objects
                    .get_mut(&slot)
                    .filter(|obj| obj.r#gen == r#gen)
                    .ok_or("STALE_OR_UNKNOWN_OBJECT")?;
                obj.x = x;
                obj.y = y;
                dirty.objects.insert(slot);
            }
            Edit::Radius {
                slot,
                r#gen,
                radius,
            } => {
                let obj = data
                    .objects
                    .get_mut(&slot)
                    .filter(|obj| obj.r#gen == r#gen)
                    .ok_or("STALE_OR_UNKNOWN_OBJECT")?;
                obj.radius = radius;
                dirty.objects.insert(slot);
            }
            Edit::Reassign {
                slot,
                r#gen,
                material,
            } => {
                let obj = data
                    .objects
                    .get_mut(&slot)
                    .filter(|obj| obj.r#gen == r#gen)
                    .ok_or("STALE_OR_UNKNOWN_OBJECT")?;
                obj.material = material;
                dirty.objects.insert(slot);
            }
        }
    }
    data.validate()?;
    Ok(Proposed { data, dirty })
}

#[derive(Deserialize)]
pub struct TraceCase {
    pub ops: Vec<Edit>,
    pub expect: Expected,
}

#[derive(Deserialize)]
pub struct Expected {
    pub accepted: bool,
    pub error: Option<String>,
    pub epoch: u64,
    pub digest: String,
    pub source_digest: String,
    pub changed: Vec<[u32; 2]>,
    pub removed: Vec<[u32; 2]>,
}

#[derive(Deserialize)]
pub struct HeaderRow {
    pub header: Header,
}
#[derive(Deserialize)]
pub struct Header {
    pub initial_digest: String,
    pub initial_source_digest: String,
}

pub fn changed_packets(
    before: &BTreeMap<Key, Packet>,
    after: &BTreeMap<Key, Packet>,
) -> (Vec<[u32; 2]>, Vec<[u32; 2]>) {
    let changed = after
        .iter()
        .filter(|(key, packet)| before.get(key) != Some(*packet))
        .map(|(key, _)| [key.slot, key.r#gen])
        .collect();
    let removed = before
        .keys()
        .filter(|key| !after.contains_key(key))
        .map(|key| [key.slot, key.r#gen])
        .collect();
    (changed, removed)
}
