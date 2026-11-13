//! Project-local typed-rule research kernel for Lab #15.
//! This is a deliberately serial, in-memory experiment, NOT framework authority.
use std::any::{Any, TypeId};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::marker::PhantomData;
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
    DuplicateClock,
    UnregisteredClock,
    InvalidClock,
    ScopeMismatch,
    GlobalOccurrenceUnsupported,
}

trait Column {
    fn clone_box(&self) -> Box<dyn Column>;
    fn erase(&mut self, key: Key) -> bool;
    fn contains(&self, key: Key) -> bool;
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
    fn contains(&self, key: Key) -> bool {
        self.0.contains_key(&key)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// Keyless values have their own typed storage: they are not disguised rows.
trait SingletonColumn {
    fn clone_box(&self) -> Box<dyn SingletonColumn>;
    fn as_any(&self) -> &dyn Any;
}
impl Clone for Box<dyn SingletonColumn> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
#[derive(Clone)]
struct SingletonValue<T: Clone + Eq + 'static>(T);
impl<T: Clone + Eq + 'static> SingletonColumn for SingletonValue<T> {
    fn clone_box(&self) -> Box<dyn SingletonColumn> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Default)]
struct World {
    live: BTreeMap<u32, Key>,
    generation: BTreeMap<u32, u32>,
    columns: HashMap<TypeId, Box<dyn Column>>,
    singletons: HashMap<TypeId, Box<dyn SingletonColumn>>,
}
impl World {
    fn singleton<T: Clone + Eq + 'static>(&self) -> Option<T> {
        self.singletons
            .get(&TypeId::of::<T>())
            .and_then(|value| value.as_any().downcast_ref::<SingletonValue<T>>())
            .map(|value| value.0.clone())
    }
    fn set_singleton<T: Clone + Eq + 'static>(&mut self, value: Option<T>) -> bool {
        if self.singleton::<T>() == value {
            return false;
        }
        match value {
            Some(value) => {
                self.singletons
                    .insert(TypeId::of::<T>(), Box::new(SingletonValue(value)));
            }
            None => {
                self.singletons.remove(&TypeId::of::<T>());
            }
        }
        true
    }
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
enum FactScope {
    Key(Key),
    Collection,
    Singleton,
}
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
struct Fact {
    ty: TypeId,
    scope: FactScope,
}
impl Fact {
    fn of<T: 'static>(key: Key) -> Self {
        Self::keyed(TypeId::of::<T>(), key)
    }
    fn keyed(ty: TypeId, key: Key) -> Self {
        Self {
            ty,
            scope: FactScope::Key(key),
        }
    }
    fn collection<T: 'static>() -> Self {
        Self::collection_type(TypeId::of::<T>())
    }
    fn collection_type(ty: TypeId) -> Self {
        Self {
            ty,
            scope: FactScope::Collection,
        }
    }
    fn singleton<T: 'static>() -> Self {
        Self::singleton_type(TypeId::of::<T>())
    }
    fn singleton_type(ty: TypeId) -> Self {
        Self {
            ty,
            scope: FactScope::Singleton,
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

/// Last committed change of a derived keyless value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SingletonChange<T> {
    Added(T),
    Updated(T),
    Removed,
}

/// In-memory occurrence from the latest successful transaction only.
/// This does not imply delivery or acknowledgment of an external effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Occurrence<T> {
    pub source: Key,
    pub ordinal: u32,
    pub value: T,
}

struct StagedEvent {
    source: Task,
    ordinal: u32,
    value: Box<dyn Any>,
}

type ClockValidator = dyn Fn(&World, &World) -> Result<(), Error>;
struct ClockSpec {
    fact: Fact,
    validate: Rc<ClockValidator>,
}

pub struct Transaction<'a> {
    world: &'a mut World,
    changed: &'a mut HashSet<Fact>,
    outputs: &'a HashMap<(TypeId, ProducerScope), usize>,
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
                self.changed.insert(Fact::keyed(ty, key));
                self.changed.insert(Fact::collection_type(ty));
            }
        }
        Ok(())
    }
    pub fn put_singleton<T: Clone + Eq + 'static>(&mut self, value: T) -> Result<(), Error> {
        self.replace_singleton(Some(value))
    }
    pub fn remove_singleton<T: Clone + Eq + 'static>(&mut self) -> Result<(), Error> {
        self.replace_singleton::<T>(None)
    }
    fn replace_singleton<T: Clone + Eq + 'static>(
        &mut self,
        value: Option<T>,
    ) -> Result<(), Error> {
        if self
            .outputs
            .contains_key(&(TypeId::of::<T>(), ProducerScope::One))
        {
            return Err(Error::DerivedIsReadOnly);
        }
        if self.world.set_singleton(value) {
            self.changed.insert(Fact::singleton::<T>());
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
        if self
            .outputs
            .contains_key(&(TypeId::of::<T>(), ProducerScope::Each))
        {
            return Err(Error::DerivedIsReadOnly);
        }
        if self.world.set::<T>(key, value) {
            self.changed.insert(Fact::of::<T>(key));
            self.changed.insert(Fact::collection::<T>());
        }
        Ok(())
    }
}

// A producer is identified by output type *and* output scope.
// This permits both a keyed and keyless computation of the same Rust type.
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
enum ProducerScope {
    Each,
    One,
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
enum TaskAt {
    Key(Key),
    Singleton,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
struct Task {
    rule: usize,
    at: TaskAt,
}
impl Task {
    fn key(rule: usize, key: Key) -> Self {
        Self {
            rule,
            at: TaskAt::Key(key),
        }
    }
    fn singleton(rule: usize) -> Self {
        Self {
            rule,
            at: TaskAt::Singleton,
        }
    }
}
type RuleFn = dyn for<'e, 'r> Fn(&mut RuleCx<'e, 'r>, TaskAt) -> Result<(), Error>;
struct Rule {
    output: TypeId,
    singleton: bool,
    evaluate: Rc<RuleFn>,
}

/// A typed scope is part of the output identity, not a special-purpose system.
pub struct Each<T>(PhantomData<T>);
pub struct One<T>(PhantomData<T>);

/// Experimental single registration contract; keyed and keyless results share
/// the same evaluator, dependency graph and transaction.
pub trait OutputShape {
    type At: Copy + 'static;
    type Value: Clone + Eq + 'static;
    fn install(
        engine: &mut Engine,
        f: impl for<'e, 'r> Fn(&mut RuleCx<'e, 'r>, Self::At) -> Result<Option<Self::Value>, Error>
        + 'static,
    ) -> Result<(), Error>;
}
impl<T: Clone + Eq + 'static> OutputShape for Each<T> {
    type At = Key;
    type Value = T;
    fn install(
        engine: &mut Engine,
        f: impl for<'e, 'r> Fn(&mut RuleCx<'e, 'r>, Key) -> Result<Option<T>, Error> + 'static,
    ) -> Result<(), Error> {
        engine.rule(f)
    }
}
impl<T: Clone + Eq + 'static> OutputShape for One<T> {
    type At = ();
    type Value = T;
    fn install(
        engine: &mut Engine,
        f: impl for<'e, 'r> Fn(&mut RuleCx<'e, 'r>, ()) -> Result<Option<T>, Error> + 'static,
    ) -> Result<(), Error> {
        engine.singleton_rule(move |cx| f(cx, ()))
    }
}

struct Evaluator<'r> {
    world: World,
    previous: &'r World,
    rules: &'r [Rule],
    by_type: &'r HashMap<(TypeId, ProducerScope), usize>,
    clocks: &'r [ClockSpec],
    events: Vec<StagedEvent>,
    invalidated: BTreeSet<Task>,
    done: BTreeSet<Task>,
    visiting: BTreeSet<Task>,
    dependencies: HashMap<Task, HashSet<Fact>>,
    insertions: HashSet<Fact>,
    output_changes: HashMap<Fact, ChangeKind>,
}
impl Evaluator<'_> {
    fn evaluate(&mut self, task: Task) -> Result<(), Error> {
        if matches!(task.at, TaskAt::Key(key) if !self.world.contains(key))
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
                task,
                next_ordinal: 0,
            },
            task.at,
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
    task: Task,
    next_ordinal: u32,
}
impl RuleCx<'_, '_> {
    pub fn exists(&mut self, key: Key) -> bool {
        self.reads.insert(Fact::of::<Exists>(key));
        self.evaluator.world.contains(key)
    }
    pub fn get<T: Clone + Eq + 'static>(&mut self, key: Key) -> Result<Option<T>, Error> {
        let fact = Fact::of::<T>(key);
        self.reads.insert(fact);
        if let Some(&rule) = self.evaluator.by_type.get(&(fact.ty, ProducerScope::Each)) {
            if self.evaluator.rules[rule].singleton {
                return Err(Error::ScopeMismatch);
            }
            self.evaluator.evaluate(Task::key(rule, key))?;
        }
        Ok(self.evaluator.world.get::<T>(key))
    }

    /// Depend on a genuinely keyless typed source or derived value.
    pub fn singleton<T: Clone + Eq + 'static>(&mut self) -> Result<Option<T>, Error> {
        let fact = Fact::singleton::<T>();
        self.reads.insert(fact);
        if let Some(&rule) = self.evaluator.by_type.get(&(fact.ty, ProducerScope::One)) {
            if !self.evaluator.rules[rule].singleton {
                return Err(Error::ScopeMismatch);
            }
            self.evaluator.evaluate(Task::singleton(rule))?;
        }
        Ok(self.evaluator.world.singleton::<T>())
    }

    /// The preceding committed keyless value; not a present-revision edge.
    pub fn previous_singleton<T: Clone + Eq + 'static>(&self) -> Option<T> {
        self.evaluator.previous.singleton::<T>()
    }

    /// An ordered snapshot of every current value of a type, including rule-owned
    /// derived values. The entire typed collection is tracked as a dependency:
    /// insertions, retractions and changed members all invalidate this reader.
    /// This first probe scans every live key and clones each present value.
    pub fn all<T: Clone + Eq + 'static>(&mut self) -> Result<Vec<(Key, T)>, Error> {
        let ty = TypeId::of::<T>();
        self.reads.insert(Fact::collection::<T>());
        let keys: Vec<_> = self.evaluator.world.live.values().copied().collect();
        let mut result = Vec::new();
        for key in keys {
            if let Some(&rule) = self.evaluator.by_type.get(&(ty, ProducerScope::Each)) {
                if self.evaluator.rules[rule].singleton {
                    return Err(Error::ScopeMismatch);
                }
                self.evaluator.evaluate(Task::key(rule, key))?;
            }
            if let Some(value) = self.evaluator.world.get::<T>(key) {
                result.push((key, value));
            }
        }
        Ok(result)
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

    /// The newly admitted clock step, or None for a non-step edit.
    /// Unlike a general `changed` read, this requires registered monotonic admission.
    pub fn advanced<T: Clone + Eq + 'static>(&mut self, key: Key) -> Result<Option<T>, Error> {
        if !self
            .evaluator
            .clocks
            .iter()
            .any(|clock| clock.fact == Fact::of::<T>(key))
        {
            return Err(Error::UnregisteredClock);
        }
        self.changed::<T>(key)
    }

    /// Stage a typed occurrence; it becomes visible only after a successful commit.
    /// This does not execute I/O or guarantee delivery to external consumers.
    pub fn emit<T: Clone + 'static>(&mut self, value: T) -> Result<(), Error> {
        // Existing occurrence consumers have per-entity source/ordinal semantics.
        // Do not manufacture an entity key for a keyless producer.
        if matches!(self.task.at, TaskAt::Singleton) {
            return Err(Error::GlobalOccurrenceUnsupported);
        }
        let ordinal = self.next_ordinal;
        self.next_ordinal = self.next_ordinal.checked_add(1).ok_or(Error::Overflow)?;
        self.evaluator.events.push(StagedEvent {
            source: self.task,
            ordinal,
            value: Box::new(value),
        });
        Ok(())
    }
}

#[derive(Default)]
pub struct Engine {
    world: World,
    rules: Vec<Rule>,
    by_type: HashMap<(TypeId, ProducerScope), usize>,
    clocks: Vec<ClockSpec>,
    events: Vec<StagedEvent>,
    dependencies: HashMap<Task, HashSet<Fact>>,
    reverse: HashMap<Fact, BTreeSet<Task>>,
    insertions: HashSet<Fact>,
    output_changes: HashMap<Fact, ChangeKind>,
    last_evaluations: usize,
    initialized_singletons: HashSet<usize>,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Single registration form with an explicit typed output scope.
    pub fn compute<S: OutputShape>(
        &mut self,
        f: impl for<'e, 'r> Fn(&mut RuleCx<'e, 'r>, S::At) -> Result<Option<S::Value>, Error> + 'static,
    ) -> Result<(), Error> {
        S::install(self, f)
    }

    fn singleton_rule<T: Clone + Eq + 'static>(
        &mut self,
        f: impl for<'e, 'r> Fn(&mut RuleCx<'e, 'r>) -> Result<Option<T>, Error> + 'static,
    ) -> Result<(), Error> {
        if !self.world.live.is_empty() || !self.initialized_singletons.is_empty() {
            return Err(Error::LateRegistration);
        }
        let ty = TypeId::of::<T>();
        if self.by_type.contains_key(&(ty, ProducerScope::One)) {
            return Err(Error::DuplicateProducer);
        }
        let callback = move |cx: &mut RuleCx<'_, '_>, at: TaskAt| {
            if at != TaskAt::Singleton {
                return Err(Error::ScopeMismatch);
            }
            let value = f(cx)?;
            let previous = cx.evaluator.world.singleton::<T>();
            if previous != value {
                let kind = match (previous.is_some(), value.is_some()) {
                    (false, true) => ChangeKind::Added,
                    (true, true) => ChangeKind::Updated,
                    (true, false) => ChangeKind::Removed,
                    (false, false) => unreachable!("unequal missing values"),
                };
                let fact = Fact::singleton::<T>();
                cx.evaluator.output_changes.insert(fact, kind);
            }
            cx.evaluator.world.set_singleton::<T>(value);
            Ok(())
        };
        self.by_type
            .insert((ty, ProducerScope::One), self.rules.len());
        self.rules.push(Rule {
            output: ty,
            singleton: true,
            evaluate: Rc::new(callback),
        });
        Ok(())
    }

    /// The current value of a keyless source or derived computation.
    pub fn singleton<T: Clone + Eq + 'static>(&self) -> Option<T> {
        self.world.singleton::<T>()
    }

    /// The last committed change of a derived keyless output.
    pub fn singleton_change<T: Clone + Eq + 'static>(&self) -> Option<SingletonChange<T>> {
        let kind = self.output_changes.get(&Fact::singleton::<T>())?;
        match kind {
            ChangeKind::Added => self.singleton::<T>().map(SingletonChange::Added),
            ChangeKind::Updated => self.singleton::<T>().map(SingletonChange::Updated),
            ChangeKind::Removed => Some(SingletonChange::Removed),
        }
    }

    /// Register a typed clock before spawning keys. Each changed clock value
    /// must advance its positive epoch; removal after initialization is rejected.
    /// This is input admission, not a separate rule/scheduler programming model.
    pub fn clock<T: Clone + Eq + 'static>(
        &mut self,
        key: Key,
        epoch: impl Fn(&T) -> u64 + 'static,
    ) -> Result<(), Error> {
        if !self.world.live.is_empty() {
            return Err(Error::LateRegistration);
        }
        let fact = Fact::of::<T>(key);
        if self.clocks.iter().any(|clock| clock.fact == fact) {
            return Err(Error::DuplicateClock);
        }
        self.clocks.push(ClockSpec {
            fact,
            validate: Rc::new(move |before, after| {
                let old = before.get::<T>(key);
                let new = after.get::<T>(key);
                match (old, new) {
                    (None, None) => Ok(()),
                    (Some(previous), Some(current)) if previous == current => Ok(()),
                    (Some(previous), Some(current)) if epoch(&current) > epoch(&previous) => Ok(()),
                    (None, Some(current)) if epoch(&current) > 0 => Ok(()),
                    _ => Err(Error::InvalidClock),
                }
            }),
        });
        Ok(())
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
        if self.by_type.contains_key(&(ty, ProducerScope::Each)) {
            return Err(Error::DuplicateProducer);
        }
        let callback = move |ctx: &mut RuleCx<'_, '_>, at: TaskAt| {
            let TaskAt::Key(key) = at else {
                return Err(Error::ScopeMismatch);
            };
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
        self.by_type
            .insert((ty, ProducerScope::Each), self.rules.len());
        self.rules.push(Rule {
            output: ty,
            singleton: false,
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
            .filter_map(|fact| match fact.scope {
                FactScope::Key(key) => self.get::<T>(key).map(|value| (key, value)),
                FactScope::Collection | FactScope::Singleton => None,
            })
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
            .filter_map(|(fact, kind)| {
                let FactScope::Key(key) = fact.scope else {
                    return None;
                };
                match kind {
                    ChangeKind::Added => self
                        .get::<T>(key)
                        .map(|value| OutputChange::Added(key, value)),
                    ChangeKind::Updated => self
                        .get::<T>(key)
                        .map(|value| OutputChange::Updated(key, value)),
                    ChangeKind::Removed => Some(OutputChange::Removed(key)),
                }
            })
            .collect();
        result.sort_by_key(|change| match change {
            OutputChange::Added(key, _)
            | OutputChange::Updated(key, _)
            | OutputChange::Removed(key) => *key,
        });
        result
    }
    /// Ordered occurrences from the last successful commit, including repeated
    /// values and multiple producers per key. Ordinals are per (key, event type)
    /// within this snapshot only; a later successful commit clears the snapshot.
    pub fn events<T: Clone + 'static>(&self) -> Vec<Occurrence<T>> {
        let mut ordinals = HashMap::<Key, u32>::new();
        self.events
            .iter()
            .filter_map(|event| {
                event.value.downcast_ref::<T>().map(|value| {
                    let TaskAt::Key(source) = event.source.at else {
                        unreachable!("singleton occurrence rejected at admission");
                    };
                    let ordinal = ordinals.entry(source).or_insert(0);
                    let item = Occurrence {
                        source,
                        ordinal: *ordinal,
                        value: value.clone(),
                    };
                    *ordinal = ordinal.checked_add(1).expect("event ordinal overflow");
                    item
                })
            })
            .collect()
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
        // Validate all clocks before any (potentially failing) rule executes.
        for clock in &self.clocks {
            (clock.validate)(&self.world, &proposed)?;
        }

        // Despawned nodes have no tasks to evaluate in the staged world.
        // Preserve their prior derived values' removal facts explicitly.
        let removed_keys: Vec<_> = self
            .world
            .live
            .values()
            .copied()
            .filter(|&key| !proposed.contains(key))
            .collect();

        // New or removed keys can change every derived typed collection.
        // This is deliberately broad: exact member-level incremental admission
        // and selective aggregate invalidation are not established here.
        if !removed_keys.is_empty() || proposed.live.values().any(|&key| !self.world.contains(key))
        {
            for rule in &self.rules {
                changes.insert(Fact::collection_type(rule.output));
            }
        }

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
                        let output = self.rules[task.rule].output;
                        match task.at {
                            TaskAt::Key(key) => {
                                queue.push_back(Fact::keyed(output, key));
                                queue.push_back(Fact::collection_type(output));
                            }
                            TaskAt::Singleton => {
                                queue.push_back(Fact::singleton_type(output));
                            }
                        }
                    }
                }
            }
        }
        for &key in proposed.live.values() {
            if !self.world.contains(key) {
                for (rule, spec) in self.rules.iter().enumerate() {
                    if !spec.singleton {
                        invalidated.insert(Task::key(rule, key));
                    }
                }
            }
        }
        // Evaluate each keyless producer once, even if it initially returns None.
        for (rule, spec) in self.rules.iter().enumerate() {
            if spec.singleton && !self.initialized_singletons.contains(&rule) {
                invalidated.insert(Task::singleton(rule));
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
            clocks: &self.clocks,
            events: Vec::new(),
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
        for key in removed_keys {
            for rule in &self.rules {
                if self
                    .world
                    .columns
                    .get(&rule.output)
                    .is_some_and(|column| column.contains(key))
                {
                    evaluator
                        .output_changes
                        .insert(Fact::keyed(rule.output, key), ChangeKind::Removed);
                }
            }
        }
        let evaluations = evaluator.done.len();
        let mut dependencies = self.dependencies.clone();
        dependencies.retain(|task, _| match task.at {
            TaskAt::Key(key) => evaluator.world.contains(key),
            TaskAt::Singleton => true,
        });
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
        // Move the staged results out of the evaluator before replacing
        // the prior World it borrowed for temporal reads.
        let Evaluator {
            world: next_world,
            insertions,
            output_changes,
            mut events,
            ..
        } = evaluator;
        events.sort_by_key(|event| (event.source, event.ordinal));
        self.world = next_world;
        self.dependencies = dependencies;
        self.reverse = reverse;
        self.insertions = insertions;
        self.output_changes = output_changes;
        self.events = events;
        self.last_evaluations = evaluations;
        for (rule, spec) in self.rules.iter().enumerate() {
            if spec.singleton {
                self.initialized_singletons.insert(rule);
            }
        }
        Ok(())
    }
}
