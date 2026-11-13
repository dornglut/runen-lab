//! Project-local game-domain typed incremental execution experiment.
//! Translation-only hierarchy: not a general 3D transform or a framework API.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct NodeId {
    pub slot: u32,
    pub generation: u32,
}
impl NodeId {
    pub const fn new(slot: u32, generation: u32) -> Self {
        Self { slot, generation }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Translation {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}
impl Translation {
    pub const fn new(x: i64, y: i64, z: i64) -> Self {
        Self { x, y, z }
    }
    fn checked_add(self, other: Self) -> Result<Self, Error> {
        Ok(Self {
            x: self.x.checked_add(other.x).ok_or(Error::Overflow)?,
            y: self.y.checked_add(other.y).ok_or(Error::Overflow)?,
            z: self.z.checked_add(other.z).ok_or(Error::Overflow)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub local: Translation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Edit {
    Spawn { id: NodeId, parent: Option<NodeId>, local: Translation },
    Move { id: NodeId, local: Translation },
    Reparent { id: NodeId, parent: Option<NodeId> },
    Remove { id: NodeId },
}
impl Edit {
    fn target(self) -> NodeId {
        match self {
            Self::Spawn { id, .. } | Self::Move { id, .. }
            | Self::Reparent { id, .. } | Self::Remove { id } => id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    DuplicateTarget,
    InvalidGeneration,
    StaleOrMissing,
    MissingParent,
    Cycle,
    Overflow,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Source {
    nodes: BTreeMap<u32, Node>,
    versions: BTreeMap<u32, u32>,
}

/// An independent full-world oracle based on Kahn topological traversal.
/// It also validates parents, cycles, and checked world-coordinate arithmetic.
fn full_recompute(source: &Source) -> Result<BTreeMap<u32, Translation>, Error> {
    let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut queue = VecDeque::new();
    for (&slot, node) in &source.nodes {
        if let Some(parent) = node.parent {
            let known = source.nodes.get(&parent.slot);
            if known.is_none_or(|record| record.id != parent) {
                return Err(Error::MissingParent);
            }
            children.entry(parent.slot).or_default().push(slot);
        } else {
            queue.push_back(slot);
        }
    }
    let mut world = BTreeMap::new();
    while let Some(slot) = queue.pop_front() {
        let node = &source.nodes[&slot];
        let parent_world = node.parent.map_or(
            Translation::default(),
            |parent| world[&parent.slot],
        );
        world.insert(slot, parent_world.checked_add(node.local)?);
        if let Some(descendants) = children.get(&slot) {
            queue.extend(descendants.iter().copied());
        }
    }
    if world.len() != source.nodes.len() {
        return Err(Error::Cycle);
    }
    Ok(world)
}

/// Stages an entire batch without mutating the source. Batch input ordering
/// does not affect parent-link validity; every node may be targeted once.
fn stage(source: &Source, edits: &[Edit]) -> Result<Source, Error> {
    let mut next = source.clone();
    let mut seen = BTreeSet::new();
    for &edit in edits {
        let id = edit.target();
        if !seen.insert(id.slot) {
            return Err(Error::DuplicateTarget);
        }
        match edit {
            Edit::Spawn { id, parent, local } => {
                if next.nodes.contains_key(&id.slot) {
                    return Err(Error::InvalidGeneration);
                }
                let expected = next.versions.get(&id.slot).copied().unwrap_or(0)
                    .checked_add(1)
                    .ok_or(Error::InvalidGeneration)?;
                if id.generation != expected {
                    return Err(Error::InvalidGeneration);
                }
                next.nodes.insert(id.slot, Node { id, parent, local });
                next.versions.insert(id.slot, id.generation);
            }
            Edit::Move { id, local } => {
                let node = next.nodes.get_mut(&id.slot)
                    .filter(|record| record.id == id)
                    .ok_or(Error::StaleOrMissing)?;
                node.local = local;
            }
            Edit::Reparent { id, parent } => {
                let node = next.nodes.get_mut(&id.slot)
                    .filter(|record| record.id == id)
                    .ok_or(Error::StaleOrMissing)?;
                node.parent = parent;
            }
            Edit::Remove { id } => {
                if next.nodes.get(&id.slot).is_none_or(|record| record.id != id) {
                    return Err(Error::StaleOrMissing);
                }
                next.nodes.remove(&id.slot);
            }
        }
    }
    // Validate the complete candidate *before* the incremental engine can
    // commit. This O(N) validation is a limitation, not a benchmark result.
    full_recompute(&next)?;
    Ok(next)
}

/// Typed downstream scene API. No propagators, dirty sets or reverse indices
/// appear in the consuming code.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Scene {
    source: Source,
    children: BTreeMap<u32, BTreeSet<u32>>,
    world: BTreeMap<u32, Translation>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    /// All-or-nothing sequential batch; readers see only committed snapshots.
    /// This does not claim concurrent isolation or durable ACID semantics.
    pub fn transact(&mut self, edits: &[Edit]) -> Result<(), Error> {
        let next_source = stage(&self.source, edits)?;
        let mut candidate = self.clone();
        let touched: BTreeSet<u32> = edits.iter().map(|edit| edit.target().slot).collect();

        for &slot in &touched {
            let old_parent = self.source.nodes.get(&slot).and_then(|node| node.parent);
            let new_parent = next_source.nodes.get(&slot).and_then(|node| node.parent);
            if old_parent != new_parent {
                if let Some(parent) = old_parent {
                    let members = candidate.children.get_mut(&parent.slot)
                        .expect("existing parent index");
                    members.remove(&slot);
                    if members.is_empty() {
                        candidate.children.remove(&parent.slot);
                    }
                }
                if let Some(parent) = new_parent {
                    candidate.children.entry(parent.slot).or_default().insert(slot);
                }
            }
        }
        candidate.source = next_source;

        let mut affected = BTreeSet::new();
        let mut stack: Vec<u32> = touched.iter().copied().collect();
        while let Some(slot) = stack.pop() {
            if affected.insert(slot) {
                if let Some(children) = candidate.children.get(&slot) {
                    stack.extend(children.iter().copied());
                }
            }
        }
        for &slot in &affected {
            candidate.world.remove(&slot);
        }

        // Recompute missing paths iteratively, even for very deep hierarchies.
        // Untouched ancestors are read from the prior committed cache.
        for &slot in &affected {
            if !candidate.source.nodes.contains_key(&slot) {
                continue;
            }
            let mut chain = Vec::new();
            let mut current = slot;
            while !candidate.world.contains_key(&current) {
                let node = candidate.source.nodes.get(&current)
                    .expect("validated parent chain");
                chain.push(current);
                match node.parent {
                    Some(parent) => current = parent.slot,
                    None => break,
                }
            }
            for id in chain.into_iter().rev() {
                let node = &candidate.source.nodes[&id];
                let base = node.parent.map_or(
                    Translation::default(),
                    |parent| candidate.world[&parent.slot],
                );
                candidate.world.insert(id, base.checked_add(node.local)?);
            }
        }
        // Independent readback is outside the algorithmic update contract:
        // correctness tests compare against the topological oracle.
        *self = candidate;
        Ok(())
    }

    pub fn world(&self, id: NodeId) -> Option<Translation> {
        self.source.nodes.get(&id.slot)
            .filter(|node| node.id == id)
            .and_then(|_| self.world.get(&id.slot).copied())
    }

    pub fn len(&self) -> usize {
        self.source.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.source.nodes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Reference {
        source: Source,
        world: BTreeMap<u32, Translation>,
    }
    impl Reference {
        fn transact(&mut self, edits: &[Edit]) -> Result<(), Error> {
            let next = stage(&self.source, edits)?;
            let world = full_recompute(&next)?;
            self.source = next;
            self.world = world;
            Ok(())
        }
    }
    fn assert_equivalent(scene: &Scene, reference: &Reference) {
        assert_eq!(scene.source, reference.source);
        assert_eq!(scene.world, reference.world);
        assert_eq!(scene.world.len(), scene.len());
    }

    fn spawn(slot: u32, parent: Option<NodeId>, xyz: (i64, i64, i64)) -> Edit {
        Edit::Spawn {
            id: NodeId::new(slot, 1),
            parent,
            local: Translation::new(xyz.0, xyz.1, xyz.2),
        }
    }

    #[test]
    fn game_consumer_move_reparent_and_atomic_rejection() {
        let root = NodeId::new(0, 1);
        let weapon = NodeId::new(1, 1);
        let camera = NodeId::new(2, 1);
        let muzzle = NodeId::new(3, 1);
        let mut scene = Scene::new();
        scene.transact(&[
            spawn(0, None, (10, 0, 0)),
            spawn(1, Some(root), (4, 0, 0)),
            spawn(2, Some(root), (-3, 0, 0)),
            spawn(3, Some(weapon), (2, 0, 0)),
        ]).unwrap();
        assert_eq!(scene.world(muzzle), Some(Translation::new(16, 0, 0)));
        scene.transact(&[Edit::Move {
            id: root,
            local: Translation::new(20, 2, 3),
        }]).unwrap();
        assert_eq!(scene.world(camera), Some(Translation::new(17, 2, 3)));
        assert_eq!(scene.world(muzzle), Some(Translation::new(26, 2, 3)));

        scene.transact(&[Edit::Reparent {
            id: weapon,
            parent: Some(camera),
        }]).unwrap();
        assert_eq!(scene.world(muzzle), Some(Translation::new(23, 2, 3)));

        let before = scene.clone();
        assert_eq!(scene.transact(&[Edit::Reparent {
            id: root,
            parent: Some(muzzle),
        }]), Err(Error::Cycle));
        assert_eq!(scene, before);

        assert_eq!(scene.transact(&[Edit::Remove { id: camera }]),
            Err(Error::MissingParent));
        assert_eq!(scene, before);

        // Reparent dependents and remove parent as one atomic batch.
        scene.transact(&[
            Edit::Reparent { id: weapon, parent: Some(root) },
            Edit::Remove { id: camera },
        ]).unwrap();
        assert_eq!(scene.world(camera), None);
        assert_eq!(scene.world(muzzle), Some(Translation::new(26, 2, 3)));
    }

    #[test]
    fn generation_reuse_and_old_identity_rejection() {
        let old = NodeId::new(7, 1);
        let fresh = NodeId::new(7, 2);
        let mut scene = Scene::new();
        scene.transact(&[spawn(7, None, (1, 2, 3))]).unwrap();
        scene.transact(&[Edit::Remove { id: old }]).unwrap();
        assert_eq!(scene.world(old), None);
        assert_eq!(scene.transact(&[spawn(7, None, (1, 2, 3))]),
            Err(Error::InvalidGeneration));
        scene.transact(&[Edit::Spawn {
            id: fresh, parent: None, local: Translation::new(4, 5, 6)
        }]).unwrap();
        assert_eq!(scene.world(old), None);
        assert_eq!(scene.world(fresh), Some(Translation::new(4, 5, 6)));
        assert_eq!(scene.transact(&[Edit::Move {
            id: old, local: Translation::new(0, 0, 0)
        }]), Err(Error::StaleOrMissing));
    }

    #[test]
    fn missing_parent_duplicates_overflow_and_noop() {
        let id = NodeId::new(0, 1);
        let mut scene = Scene::new();
        let original = scene.clone();
        assert_eq!(scene.transact(&[spawn(0, Some(NodeId::new(9, 1)), (0,0,0))]),
            Err(Error::MissingParent));
        assert_eq!(scene, original);
        scene.transact(&[spawn(0, None, (i64::MAX, 0, 0))]).unwrap();
        assert_eq!(scene.transact(&[spawn(1, Some(id), (1, 0, 0))]),
            Err(Error::Overflow));
        assert_eq!(scene.len(), 1);
        assert_eq!(scene.transact(&[Edit::Move { id, local: Translation::default() },
            Edit::Move { id, local: Translation::default() }]),
            Err(Error::DuplicateTarget));
        let snapshot = scene.clone();
        scene.transact(&[]).unwrap();
        assert_eq!(scene, snapshot);
    }

    #[test]
    fn wide_and_deep_scene_without_recursive_stack() {
        let mut scene = Scene::new();
        let mut reference = Reference::default();
        let root = NodeId::new(0, 1);
        let mut edits = vec![spawn(0, None, (1, 2, 3))];
        for slot in 1..=4096 {
            edits.push(spawn(slot, Some(root), (i64::from(slot), 0, 0)));
        }
        scene.transact(&edits).unwrap();
        reference.transact(&edits).unwrap();
        let move_root = [Edit::Move {
            id: root, local: Translation::new(7, 8, 9)
        }];
        scene.transact(&move_root).unwrap();
        reference.transact(&move_root).unwrap();
        assert_equivalent(&scene, &reference);

        // Separate 4k-deep chain; no recursive evaluation.
        let mut chain = Scene::new();
        let chain_edits: Vec<_> = (0..4096).map(|slot| {
            let parent = if slot == 0 {
                None
            } else {
                Some(NodeId::new(slot - 1, 1))
            };
            spawn(slot, parent, (1, 1, 1))
        }).collect();
        chain.transact(&chain_edits).unwrap();
        chain.transact(&[Edit::Move {
            id: root, local: Translation::new(2, 2, 2)
        }]).unwrap();
        assert_eq!(chain.world(NodeId::new(4095, 1)),
            Some(Translation::new(4097, 4097, 4097)));
    }

    #[test]
    fn independent_oracle_replays_mixed_valid_and_invalid_edits() {
        let mut scene = Scene::new();
        let mut reference = Reference::default();
        let mut initial = vec![spawn(0, None, (0, 0, 0))];
        for slot in 1..128 {
            let parent = NodeId::new((slot - 1) / 2, 1);
            initial.push(spawn(slot, Some(parent),
                (i64::from(slot), -(i64::from(slot)), 1)));
        }
        scene.transact(&initial).unwrap();
        reference.transact(&initial).unwrap();
        assert_equivalent(&scene, &reference);

        let mut seed: u64 = 0x1234_5678_9abc_def0;
        for i in 0..1000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let slot = (seed % 128) as u32;
            let id = NodeId::new(slot, 1);
            let edits = match i % 5 {
                0 => vec![Edit::Move { id,
                    local: Translation::new(i64::from(i), i64::from(slot), 1) }],
                1 => vec![Edit::Reparent {
                    id,
                    parent: Some(NodeId::new(((seed >> 12) % 128) as u32, 1))
                }],
                2 => vec![Edit::Remove { id }],
                3 => vec![Edit::Reparent { id, parent: None }],
                _ => vec![Edit::Move { id,
                    local: Translation::new(i64::from(slot), 5, 7) }],
            };
            let snapshot = scene.clone();
            let got = scene.transact(&edits);
            let expected = reference.transact(&edits);
            assert_eq!(got, expected, "transaction {i}");
            if got.is_err() {
                assert_eq!(scene, snapshot, "rejected transaction {i} mutated scene");
            }
            assert_equivalent(&scene, &reference);
        }
    }
}
