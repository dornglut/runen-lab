//! Project-local typed-rule research kernel for Lab #15.
//! This is a deliberately serial, in-memory experiment, NOT framework authority.
use std::any::{Any, TypeId};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Key {
    pub slot: u32,
    pub generation: u32,
}
impl Key {
    pub const fn new(slot: u32, generation: u32) -> Self {
        Self { slot, generation }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    MissingEntity,
    InvalidGeneration,
    DerivedIsReadOnly,
    DuplicateProducer,
    LateRegistration,
    RuleCycle,
    InvalidRelationship,
    Overflow,
}

trait Column {
    fn clone_box(&self) -> Box<dyn Column>;
    fn erase(&mut self, key: Key) -> bool;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
impl Clone for Box<dyn Column> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
#[derive(Clone)]
struct Values<T: Clone + Eq + 'static>(BTreeMap<Key, T>);
impl<T: Clone + Eq + 'static> Column for Values<T> {
    fn clone_box(&self) -> Box<dyn Column> {
        Box::new(self.clone())
    }
    fn erase(&mut self, key: Key) -> bool {
        self.0.remove(&key).is_some()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Clone, Default)]
struct World {
    live: BTreeMap<u32, Key>,
    generation: BTreeMap<u32, u32>,
    columns: HashMap<TypeId, Box<dyn Column>>,
}
impl World {
    fn contains(&self, key: Key) -> bool {
        self.live.get(&key.slot) == Some(&key)
    }
    fn get<T: Clone + Eq + 'static>(&self, key: Key) -> Option<T> {
        if !self.contains(key) {
            return None;
        }
        self.columns
            .get(&TypeId::of::<T>())
            .and_then(|c| c.as_any().downcast_ref::<Values<T>>())
            .and_then(|c| c.0.get(&key).cloned())
    }
    fn set<T: Clone + Eq + 'static>(&mut self, key: Key, value: Option<T>) -> bool {
        let ty = TypeId::of::<T>();
        match value {
            Some(value) => {
                let col = self
                    .columns
                    .entry(ty)
                    .or_insert_with(|| Box::new(Values::<T>(BTreeMap::new())));
                let values = &mut col
                    .as_any_mut()
                    .downcast_mut::<Values<T>>()
                    .expect("type-indexed column")
                    .0;
                if values.get(&key) == Some(&value) {
                    return false;
                }
                values.insert(key, value);
                true
            }
            None => self.columns.get_mut(&ty).is_some_and(|col| {
                col.as_any_mut()
                    .downcast_mut::<Values<T>>()
                    .expect("type-indexed column")
                    .0
                    .remove(&key)
                    .is_some()
            }),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
struct Fact {
    ty: TypeId,
    key: Key,
}
impl Fact {
    fn of<T: 'static>(key: Key) -> Self {
        Self {
            ty: TypeId::of::<T>(),
            key,
        }
    }
}
struct Exists;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChangeKind {
    Added,
    Updated,
    Removed,
}

/// Change to a cached typed output in the latest successful transaction.
/// A change is not a durable, acknowledged external event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutputChange<T> {
    Added(Key, T),
    Updated(Key, T),
    Removed(Key),
}

pub struct Transaction<'a> {
    world: &'a mut World,
    changed: &'a mut HashSet<Fact>,
    outputs: &'a HashMap<TypeId, usize>,
}
impl Transaction<'_> {
    pub fn spawn(&mut self, key: Key) -> Result<(), Error> {
        if self.world.live.contains_key(&key.slot) {
            return Err(Error::InvalidGeneration);
        }
        let expected = self
            .world
            .generation
            .get(&key.slot)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(Error::InvalidGeneration)?;
        if key.generation != expected {
            return Err(Error::InvalidGeneration);
        }
        self.world.live.insert(key.slot, key);
        self.world.generation.insert(key.slot, key.generation);
        self.changed.insert(Fact::of::<Exists>(key));
        Ok(())
    }
    pub fn despawn(&mut self, key: Key) -> Result<(), Error> {
        if !self.world.contains(key) {
            return Err(Error::MissingEntity);
        }
        self.world.live.remove(&key.slot);
        self.changed.insert(Fact::of::<Exists>(key));
        for (&ty, col) in &mut self.world.columns {
            if col.erase(key) {
                self.changed.insert(Fact { ty, key });
            }
        }
        Ok(())
    }
    pub fn put<T: Clone + Eq + 'static>(&mut self, key: Key, value: T) -> Result<(), Error> {
        self.replace::<T>(key, Some(value))
    }
    pub fn remove<T: Clone + Eq + 'static>(&mut self, key: Key) -> Result<(), Error> {
        self.replace::<T>(key, None)
    }
    fn replace<T: Clone + Eq + 'static>(
        &mut self,
        key: Key,
        value: Option<T>,
    ) -> Result<(), Error> {
        if !self.world.contains(key) {
            return Err(Error::MissingEntity);
        }
        if self.outputs.contains_key(&TypeId::of::<T>()) {
            return Err(Error::DerivedIsReadOnly);
        }
        if self.world.set::<T>(key, value) {
            self.changed.insert(Fact::of::<T>(key));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
struct Task {
    rule: usize,
    key: Key,
}
type RuleFn = dyn for<'e, 'r> Fn(&mut RuleCx<'e, 'r>, Key) -> Result<(), Error>;
struct Rule {
    output: TypeId,
    evaluate: Rc<RuleFn>,
}

struct Evaluator<'r> {
    world: World,
    previous: &'r World,
    rules: &'r [Rule],
    by_type: &'r HashMap<TypeId, usize>,
    invalidated: BTreeSet<Task>,
    done: BTreeSet<Task>,
    visiting: BTreeSet<Task>,
    dependencies: HashMap<Task, HashSet<Fact>>,
    insertions: HashSet<Fact>,
    output_changes: HashMap<Fact, ChangeKind>,
}
impl Evaluator<'_> {
    fn evaluate(&mut self, task: Task) -> Result<(), Error> {
        if !self.world.contains(task.key)
            || !self.invalidated.contains(&task)
            || self.done.contains(&task)
        {
            return Ok(());
        }
        if !self.visiting.insert(task) {
            return Err(Error::RuleCycle);
        }
        let function = Rc::clone(&self.rules[task.rule].evaluate);
        let mut reads = HashSet::new();
        let outcome = function(
            &mut RuleCx {
                evaluator: self,
                reads: &mut reads,
            },
            task.key,
        );
        self.visiting.remove(&task);
        outcome?;
        self.dependencies.insert(task, reads);
        self.done.insert(task);
        Ok(())
    }
}

pub struct RuleCx<'e, 'r> {
    evaluator: &'e mut Evaluator<'r>,
    reads: &'e mut HashSet<Fact>,
}
impl RuleCx<'_, '_> {
    pub fn exists(&mut self, key: Key) -> bool {
        self.reads.insert(Fact::of::<Exists>(key));
        self.evaluator.world.contains(key)
    }
    pub fn get<T: Clone + Eq + 'static>(&mut self, key: Key) -> Result<Option<T>, Error> {
        let fact = Fact::of::<T>(key);
        self.reads.insert(fact);
        if let Some(&rule) = self.evaluator.by_type.get(&fact.ty) {
            self.evaluator.evaluate(Task { rule, key })?;
        }
        Ok(self.evaluator.world.get::<T>(key))
    }

    /// The previous committed value. This is a temporal edge, not a
    /// same-transaction dependency: it cannot create an immediate rule cycle.
    pub fn previous<T: Clone + Eq + 'static>(&self, key: Key) -> Option<T> {
        self.evaluator.previous.get::<T>(key)
    }

    /// Read a current value (tracking it) only when it differs from the
    /// previous committed value. Removal returns None, not a value.
    pub fn changed<T: Clone + Eq + 'static>(&mut self, key: Key) -> Result<Option<T>, Error> {
        let current = self.get::<T>(key)?;
        Ok(if current != self.previous::<T>(key) {
            current
        } else {
            None
        })
    }
}

#[derive(Default)]
pub struct Engine {
    world: World,
    rules: Vec<Rule>,
    by_type: HashMap<TypeId, usize>,
    dependencies: HashMap<Task, HashSet<Fact>>,
    reverse: HashMap<Fact, BTreeSet<Task>>,
    insertions: HashSet<Fact>,
    output_changes: HashMap<Fact, ChangeKind>,
    last_evaluations: usize,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }
    /// Register a typed, per-key computation. A given output type has one producer.
    /// This first candidate requires registration before the first spawned key.
    pub fn rule<T>(
        &mut self,
        f: impl for<'e, 'r> Fn(&mut RuleCx<'e, 'r>, Key) -> Result<Option<T>, Error> + 'static,
    ) -> Result<(), Error>
    where
        T: Clone + Eq + 'static,
    {
        if !self.world.live.is_empty() {
            return Err(Error::LateRegistration);
        }
        let ty = TypeId::of::<T>();
        if self.by_type.contains_key(&ty) {
            return Err(Error::DuplicateProducer);
        }
        let callback = move |ctx: &mut RuleCx<'_, '_>, key: Key| {
            let value = f(ctx, key)?;
            let previously = ctx.evaluator.world.get::<T>(key);
            if previously != value {
                let kind = match (previously.is_some(), value.is_some()) {
                    (false, true) => ChangeKind::Added,
                    (true, true) => ChangeKind::Updated,
                    (true, false) => ChangeKind::Removed,
                    (false, false) => unreachable!("unequal missing values"),
                };
                let fact = Fact::of::<T>(key);
                if kind == ChangeKind::Added {
                    ctx.evaluator.insertions.insert(fact);
                }
                ctx.evaluator.output_changes.insert(fact, kind);
            }
            ctx.evaluator.world.set::<T>(key, value);
            Ok(())
        };
        self.by_type.insert(ty, self.rules.len());
        self.rules.push(Rule {
            output: ty,
            evaluate: Rc::new(callback),
        });
        Ok(())
    }

    pub fn get<T: Clone + Eq + 'static>(&self, key: Key) -> Option<T> {
        self.world.get::<T>(key)
    }
    pub fn keys(&self) -> Vec<Key> {
        self.world.live.values().copied().collect()
    }
    pub fn inserted<T: Clone + Eq + 'static>(&self) -> Vec<(Key, T)> {
        let mut items: Vec<_> = self
            .insertions
            .iter()
            .filter(|fact| fact.ty == TypeId::of::<T>())
            .filter_map(|fact| self.get::<T>(fact.key).map(|value| (fact.key, value)))
            .collect();
        items.sort_by_key(|(key, _)| *key);
        items
    }
    /// Added, modified and removed output values from the last successful
    /// transaction, ordered by generational key. Only one delta per key.
    pub fn changes<T: Clone + Eq + 'static>(&self) -> Vec<OutputChange<T>> {
        let mut result: Vec<_> = self
            .output_changes
            .iter()
            .filter(|(fact, _)| fact.ty == TypeId::of::<T>())
            .filter_map(|(fact, kind)| match kind {
                ChangeKind::Added => self
                    .get::<T>(fact.key)
                    .map(|value| OutputChange::Added(fact.key, value)),
                ChangeKind::Updated => self
                    .get::<T>(fact.key)
                    .map(|value| OutputChange::Updated(fact.key, value)),
                ChangeKind::Removed => Some(OutputChange::Removed(fact.key)),
            })
            .collect();
        result.sort_by_key(|change| match change {
            OutputChange::Added(key, _)
            | OutputChange::Updated(key, _)
            | OutputChange::Removed(key) => *key,
        });
        result
    }
    pub fn last_evaluations(&self) -> usize {
        self.last_evaluations
    }

    pub fn transact<F>(&mut self, f: F) -> Result<(), Error>
    where
        F: FnOnce(&mut Transaction<'_>) -> Result<(), Error>,
    {
        let mut proposed = self.world.clone();
        let mut changes = HashSet::new();
        f(&mut Transaction {
            world: &mut proposed,
            changed: &mut changes,
            outputs: &self.by_type,
        })?;

        // The old dependency graph determines which tasks must be invalidated.
        // A derived output's invalidation is propagated transitively, before
        // evaluating any rule, even when its next value proves unchanged.
        let mut invalidated = BTreeSet::new();
        let mut queue: VecDeque<Fact> = changes.iter().copied().collect();
        let mut observed = HashSet::new();
        while let Some(fact) = queue.pop_front() {
            if !observed.insert(fact) {
                continue;
            }
            if let Some(dependents) = self.reverse.get(&fact) {
                for &task in dependents {
                    if invalidated.insert(task) {
                        queue.push_back(Fact {
                            ty: self.rules[task.rule].output,
                            key: task.key,
                        });
                    }
                }
            }
        }
        for &key in proposed.live.values() {
            if !self.world.contains(key) {
                for rule in 0..self.rules.len() {
                    invalidated.insert(Task { rule, key });
                }
            }
        }
        // Re-evaluated rules may discover new dependencies. They use the
        // current staged world, with other invalidated derivations resolved
        // recursively, and reject dynamic cycles before commit.
        let mut evaluator = Evaluator {
            world: proposed,
            previous: &self.world,
            rules: &self.rules,
            by_type: &self.by_type,
            invalidated,
            done: BTreeSet::new(),
            visiting: BTreeSet::new(),
            dependencies: HashMap::new(),
            insertions: HashSet::new(),
            output_changes: HashMap::new(),
        };
        let tasks: Vec<_> = evaluator.invalidated.iter().copied().collect();
        for task in tasks {
            evaluator.evaluate(task)?;
        }
        let evaluations = evaluator.done.len();
        let mut dependencies = self.dependencies.clone();
        dependencies.retain(|task, _| evaluator.world.contains(task.key));
        for task in evaluator.invalidated {
            dependencies.remove(&task);
        }
        dependencies.extend(evaluator.dependencies);

        // Correct but intentionally expensive: whole graph re-indexed per
        // successful transaction. Not a scalable O(changes) implementation.
        let mut reverse: HashMap<Fact, BTreeSet<Task>> = HashMap::new();
        for (&task, reads) in &dependencies {
            for &fact in reads {
                reverse.entry(fact).or_default().insert(task);
            }
        }
        self.world = evaluator.world;
        self.dependencies = dependencies;
        self.reverse = reverse;
        self.insertions = evaluator.insertions;
        self.output_changes = evaluator.output_changes;
        self.last_evaluations = evaluations;
        Ok(())
    }

    /// Explicit epoch-boundary promotion of derived next-state into input
    /// state. Prevents same-epoch feedback loops without a second rule API.
    pub fn promote<From, To, F>(&mut self, map: F) -> Result<(), Error>
    where
        From: Clone + Eq + 'static,
        To: Clone + Eq + 'static,
        F: Fn(From) -> To,
    {
        let updates: Vec<_> = self
            .keys()
            .into_iter()
            .filter_map(|key| self.get::<From>(key).map(|value| (key, map(value))))
            .collect();
        self.transact(|tx| {
            for (key, value) in updates {
                tx.put::<To>(key, value)?;
            }
            Ok(())
        })
    }
}
