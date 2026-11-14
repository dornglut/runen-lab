//! Test-local, function-first frontend candidate for the existing C kernel.
//!
//! One named-function registration method infers required per-record inputs,
//! keyed advanced computation, or keyless advanced computation from the Rust
//! function signature. There is NO alternate evaluator or scheduler here.
//!
//! This experiment does NOT prove a universal frontend: simple injected reads
//! still clone values; advanced functions still require RuleCx and a key;
//! mutation inside rules, arbitrary fallible/optional injected results, typed
//! event-handler invocation and ownership of real I/O remain unresolved.
use std::marker::PhantomData;
use unified_rules::{Each, Engine, Error, Key, On, One, OutputChange, RuleCx, Transaction};

struct Required1<A, O>(PhantomData<(A, O)>);
struct Required2<A, B, O>(PhantomData<(A, B, O)>);
struct Keyed<O>(PhantomData<O>);
struct Keyless<O>(PhantomData<O>);
struct Fallible2<A, B, O>(PhantomData<(A, B, O)>);
struct Optional1<A, O>(PhantomData<(A, O)>);
struct TargetMutation<E, T>(PhantomData<(E, T)>);
struct OccurrenceHandler<E>(PhantomData<E>);

/// Explicit event-to-entity routing. The runtime does not infer a target
/// entity from a Rust struct field name.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Targeted<E> {
    entity: Key,
    value: E,
}

trait EmitTo {
    fn emit_to<E: Clone + Eq + 'static>(&mut self, entity: Key, value: E) -> Result<(), Error>;
}
impl EmitTo for Transaction<'_> {
    fn emit_to<E: Clone + Eq + 'static>(
        &mut self,
        entity: Key,
        value: E,
    ) -> Result<(), Error> {
        self.emit(Targeted { entity, value })
    }
}

trait Install<S> {
    fn install(self, app: &mut Engine) -> Result<(), Error>;
}

impl<F, A, O> Install<Required1<A, O>> for F
where
    A: Clone + Eq + 'static,
    O: Clone + Eq + 'static,
    F: Fn(&A) -> O + 'static,
{
    fn install(self, app: &mut Engine) -> Result<(), Error> {
        app.compute::<Each<O>>(move |cx, key| Ok(cx.get::<A>(key)?.map(|value| self(&value))))
    }
}

impl<F, A, B, O> Install<Required2<A, B, O>> for F
where
    A: Clone + Eq + 'static,
    B: Clone + Eq + 'static,
    O: Clone + Eq + 'static,
    F: Fn(&A, &B) -> O + 'static,
{
    fn install(self, app: &mut Engine) -> Result<(), Error> {
        app.compute::<Each<O>>(move |cx, key| {
            let Some(a) = cx.get::<A>(key)? else {
                return Ok(None);
            };
            let Some(b) = cx.get::<B>(key)? else {
                return Ok(None);
            };
            Ok(Some(self(&a, &b)))
        })
    }
}

impl<F, O> Install<Keyed<O>> for F
where
    O: Clone + Eq + 'static,
    F: for<'e, 'r> Fn(&mut RuleCx<'e, 'r>, Key) -> Result<Option<O>, Error> + 'static,
{
    fn install(self, app: &mut Engine) -> Result<(), Error> {
        app.compute::<Each<O>>(self)
    }
}

impl<F, O> Install<Keyless<O>> for F
where
    O: Clone + Eq + 'static,
    F: for<'e, 'r> Fn(&mut RuleCx<'e, 'r>) -> Result<Option<O>, Error> + 'static,
{
    fn install(self, app: &mut Engine) -> Result<(), Error> {
        app.compute::<One<O>>(move |cx, ()| self(cx))
    }
}


impl<F, A, B, O> Install<Fallible2<A, B, O>> for F
where
    A: Clone + Eq + 'static,
    B: Clone + Eq + 'static,
    O: Clone + Eq + 'static,
    F: Fn(&A, &B) -> Result<O, Error> + 'static,
{
    fn install(self, app: &mut Engine) -> Result<(), Error> {
        app.compute::<Each<O>>(move |cx, key| {
            let Some(a) = cx.get::<A>(key)? else {
                return Ok(None);
            };
            let Some(b) = cx.get::<B>(key)? else {
                return Ok(None);
            };
            Ok(Some(self(&a, &b)?))
        })
    }
}

impl<F, A, O> Install<Optional1<A, O>> for F
where
    A: Clone + Eq + 'static,
    O: Clone + Eq + 'static,
    F: Fn(&A) -> Option<O> + 'static,
{
    fn install(self, app: &mut Engine) -> Result<(), Error> {
        app.compute::<Each<O>>(move |cx, key| {
            Ok(cx.get::<A>(key)?.and_then(|value| self(&value)))
        })
    }
}

impl<F, E, T> Install<TargetMutation<E, T>> for F
where
    E: Clone + Eq + 'static,
    T: Clone + Eq + 'static,
    F: Fn(&E, &mut T) -> Result<(), Error> + 'static,
{
    fn install(self, app: &mut Engine) -> Result<(), Error> {
        app.compute::<On<Targeted<E>>>(move |cx, occurrence| {
            cx.try_update::<T>(occurrence.entity, |source| {
                self(&occurrence.value, source)
            })?;
            Ok(None)
        })
    }
}

impl<F, E> Install<OccurrenceHandler<E>> for F
where
    E: Clone + Eq + 'static,
    F: for<'e, 'r> Fn(&mut RuleCx<'e, 'r>, E) -> Result<(), Error> + 'static,
{
    fn install(self, app: &mut Engine) -> Result<(), Error> {
        app.compute::<On<E>>(move |cx, event| {
            self(cx, event)?;
            Ok(None)
        })
    }
}

trait FunctionFirst {
    fn register<S, F>(&mut self, rule: F) -> Result<(), Error>
    where
        F: Install<S>;
}

impl FunctionFirst for Engine {
    fn register<S, F>(&mut self, rule: F) -> Result<(), Error>
    where
        F: Install<S>,
    {
        rule.install(self)
    }
}

// Normal, separately testable application functions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Health(u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MaxHealth(u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HealthPercent(u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Dead(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Seed(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Velocity(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Position(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Parent(Key);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WorldPosition(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Step {
    epoch: u64,
    dt: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AliveCount(usize);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DeathSeen(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Died(Key);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlaySound(Key);

fn health_percent(health: &Health, max: &MaxHealth) -> HealthPercent {
    if max.0 == 0 {
        return HealthPercent(0);
    }
    let percent = u64::from(health.0.min(max.0)) * 100 / u64::from(max.0);
    HealthPercent(percent as u32)
}

fn is_dead(health: &Health) -> Dead {
    Dead(health.0 == 0)
}

fn movement(cx: &mut RuleCx<'_, '_>, key: Key) -> Result<Option<Position>, Error> {
    let Some(Seed(seed)) = cx.get::<Seed>(key)? else {
        return Ok(None);
    };
    let Some(Velocity(velocity)) = cx.get::<Velocity>(key)? else {
        return Ok(None);
    };
    let previous = cx.previous::<Position>(key);
    let before = previous.unwrap_or(Position(seed));
    let Some(step) = cx.advanced_in::<One<Step>>(())? else {
        return Ok(Some(before));
    };
    if previous.is_none() {
        return Ok(Some(before));
    }
    let displacement = velocity.checked_mul(step.dt).ok_or(Error::Overflow)?;
    Ok(Some(Position(
        before.0.checked_add(displacement).ok_or(Error::Overflow)?,
    )))
}

fn world_position(cx: &mut RuleCx<'_, '_>, key: Key) -> Result<Option<WorldPosition>, Error> {
    let Some(Position(local)) = cx.get::<Position>(key)? else {
        return Ok(None);
    };
    let world = if let Some(Parent(parent)) = cx.get::<Parent>(key)? {
        if !cx.exists(parent) {
            return Err(Error::InvalidRelationship);
        }
        let WorldPosition(parent_world) = cx
            .get::<WorldPosition>(parent)?
            .ok_or(Error::InvalidRelationship)?;
        local.checked_add(parent_world).ok_or(Error::Overflow)?
    } else {
        local
    };
    Ok(Some(WorldPosition(world)))
}

fn alive_count(cx: &mut RuleCx<'_, '_>) -> Result<Option<AliveCount>, Error> {
    let count = cx
        .all::<Health>()?
        .into_iter()
        .filter(|(_, health)| health.0 > 0)
        .count();
    Ok(Some(AliveCount(count)))
}

fn death_transition(cx: &mut RuleCx<'_, '_>, key: Key) -> Result<Option<DeathSeen>, Error> {
    let Some(Dead(dead)) = cx.get::<Dead>(key)? else {
        return Ok(None);
    };
    if dead && cx.previous::<DeathSeen>(key) != Some(DeathSeen(true)) {
        cx.emit(Died(key))?;
        cx.request(PlaySound(key))?;
    }
    Ok(Some(DeathSeen(dead)))
}

#[test]
fn named_functions_share_one_registration_contract_across_game_rules() -> Result<(), Error> {
    let mut app = Engine::new();
    app.clock_in::<One<Step>>((), |step| step.epoch)?;
    app.register(health_percent)?;
    app.register(is_dead)?;
    app.register(movement)?;
    app.register(world_position)?;
    app.register(alive_count)?;
    app.register(death_transition)?;

    let player = Key::new(1, 1);
    let sword = Key::new(2, 1);
    let enemy = Key::new(3, 1);

    app.transact(|tx| {
        for key in [player, sword, enemy] {
            tx.spawn(key)?;
        }
        tx.put_singleton(Step { epoch: 1, dt: 0 })?;
        tx.put(player, Seed(10))?;
        tx.put(player, Velocity(0))?;
        tx.put(player, Health(80))?;
        tx.put(player, MaxHealth(100))?;
        tx.put(sword, Seed(3))?;
        tx.put(sword, Velocity(0))?;
        tx.put(sword, Parent(player))?;
        tx.put(enemy, Seed(20))?;
        tx.put(enemy, Velocity(0))?;
        tx.put(enemy, Health(40))?;
        tx.put(enemy, MaxHealth(40))?;
        Ok(())
    })?;
    assert_eq!(app.get::<HealthPercent>(player), Some(HealthPercent(80)));
    assert_eq!(app.get::<WorldPosition>(sword), Some(WorldPosition(13)));
    assert_eq!(app.singleton::<AliveCount>(), Some(AliveCount(2)));

    app.transact(|tx| {
        tx.put(player, Velocity(2))?;
        tx.put(enemy, Health(0))?;
        tx.put_singleton(Step { epoch: 2, dt: 1 })
    })?;
    assert_eq!(app.get::<Position>(player), Some(Position(12)));
    assert_eq!(app.get::<WorldPosition>(sword), Some(WorldPosition(15)));
    assert_eq!(app.singleton::<AliveCount>(), Some(AliveCount(1)));
    assert_eq!(app.get::<HealthPercent>(enemy), Some(HealthPercent(0)));
    assert_eq!(app.occurrences::<Died>().len(), 1);
    assert_eq!(app.pending_effects::<PlaySound>().len(), 1);

    // No new fixed step: changing an unrelated source must not move the player.
    app.transact(|tx| tx.put(player, MaxHealth(200)))?;
    assert_eq!(app.get::<Position>(player), Some(Position(12)));
    assert!(app.occurrences::<Died>().is_empty());
    assert_eq!(app.pending_effects::<PlaySound>().len(), 1);

    // A rejected dynamic cycle preserves the previously committed world.
    assert_eq!(
        app.transact(|tx| tx.put(player, Parent(sword))),
        Err(Error::RuleCycle)
    );
    assert_eq!(app.get::<WorldPosition>(sword), Some(WorldPosition(15)));

    app.transact(|tx| tx.put(sword, Parent(enemy)))?;
    assert_eq!(app.get::<WorldPosition>(sword), Some(WorldPosition(23)));

    // A second genuine death transition is a new occurrence, not replay.
    app.transact(|tx| tx.put(enemy, Health(30)))?;
    assert_eq!(app.singleton::<AliveCount>(), Some(AliveCount(2)));
    app.transact(|tx| tx.put(enemy, Health(0)))?;
    assert_eq!(app.occurrences::<Died>().len(), 1);
    assert_eq!(app.pending_effects::<PlaySound>().len(), 2);
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Price(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Quantity(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Amount(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Balance(i64);

fn line_amount(price: &Price, quantity: &Quantity) -> Amount {
    Amount(price.0 * quantity.0)
}

fn balance(cx: &mut RuleCx<'_, '_>) -> Result<Option<Balance>, Error> {
    let amount = cx
        .all::<Amount>()?
        .into_iter()
        .try_fold(0_i64, |total, (_, value)| {
            total.checked_add(value.0).ok_or(Error::Overflow)
        })?;
    Ok(Some(Balance(amount)))
}

#[test]
fn same_named_function_contract_works_for_invoice_and_retraction() -> Result<(), Error> {
    let mut app = Engine::new();
    app.register(line_amount)?;
    app.register(balance)?;
    let row = Key::new(9, 1);
    app.transact(|tx| {
        tx.spawn(row)?;
        tx.put(row, Price(12))?;
        tx.put(row, Quantity(3))
    })?;
    assert_eq!(app.get::<Amount>(row), Some(Amount(36)));
    assert_eq!(app.singleton::<Balance>(), Some(Balance(36)));

    app.transact(|tx| tx.remove::<Quantity>(row))?;
    assert_eq!(app.get::<Amount>(row), None);
    assert_eq!(app.singleton::<Balance>(), Some(Balance(0)));
    assert_eq!(app.changes::<Amount>(), vec![OutputChange::Removed(row)]);
    assert_eq!(
        app.transact(|tx| tx.put(row, Amount(5))),
        Err(Error::DerivedIsReadOnly)
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Hit {
    amount: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Loot;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LootCount(usize);

fn damage(hit: &Hit, health: &mut Health) -> Result<(), Error> {
    health.0 = health.0.saturating_sub(hit.amount);
    Ok(())
}

fn spawn_loot(cx: &mut RuleCx<'_, '_>, died: Died) -> Result<(), Error> {
    let loot = Key::new(died.0.slot + 100, 1);
    cx.spawn(loot)?;
    cx.put(loot, Loot)
}

fn loot_count(cx: &mut RuleCx<'_, '_>) -> Result<Option<LootCount>, Error> {
    Ok(Some(LootCount(cx.all::<Loot>()?.len())))
}

#[test]
fn function_first_game_composes_targeted_mutation_derivation_temporal_hierarchy_and_feedback(
) -> Result<(), Error> {
    let mut app = Engine::new();
    app.clock_in::<One<Step>>((), |step| step.epoch)?;

    // Every behavior registers via one method and ordinary Rust function names.
    app.register(health_percent)?;
    app.register(is_dead)?;
    app.register(damage)?;
    app.register(death_transition)?;
    app.register(spawn_loot)?;
    app.register(loot_count)?;
    app.register(movement)?;
    app.register(world_position)?;

    let player = Key::new(1, 1);
    let enemy = Key::new(2, 1);
    let sword = Key::new(3, 1);
    let loot = Key::new(102, 1);
    app.transact(|tx| {
        for entity in [player, enemy, sword] {
            tx.spawn(entity)?;
        }
        tx.put_singleton(Step { epoch: 1, dt: 0 })?;
        tx.put(player, Seed(10))?;
        tx.put(player, Velocity(0))?;
        tx.put(player, Health(100))?;
        tx.put(player, MaxHealth(100))?;
        tx.put(enemy, Seed(20))?;
        tx.put(enemy, Velocity(0))?;
        tx.put(enemy, Health(40))?;
        tx.put(enemy, MaxHealth(40))?;
        tx.put(sword, Seed(3))?;
        tx.put(sword, Velocity(0))?;
        tx.put(sword, Parent(player))
    })?;
    assert_eq!(app.singleton::<LootCount>(), Some(LootCount(0)));
    assert_eq!(app.get::<WorldPosition>(sword), Some(WorldPosition(13)));

    app.transact(|tx| {
        tx.put(player, Velocity(2))?;
        tx.put_singleton(Step { epoch: 2, dt: 1 })?;
        tx.emit_to(enemy, Hit { amount: 10 })?;
        tx.emit_to(enemy, Hit { amount: 30 })
    })?;
    assert_eq!(app.get::<Health>(enemy), Some(Health(0)));
    assert_eq!(app.get::<HealthPercent>(enemy), Some(HealthPercent(0)));
    assert_eq!(app.get::<Dead>(enemy), Some(Dead(true)));
    assert_eq!(app.get::<Position>(player), Some(Position(12)));
    assert_eq!(app.get::<WorldPosition>(sword), Some(WorldPosition(15)));
    assert_eq!(app.get::<Loot>(loot), Some(Loot));
    assert_eq!(app.singleton::<LootCount>(), Some(LootCount(1)));
    assert_eq!(app.occurrences::<Died>().len(), 1);
    assert_eq!(app.pending_effects::<PlaySound>().len(), 1);

    app.transact(|_| Ok(()))?;
    assert!(app.occurrences::<Died>().is_empty());
    assert_eq!(app.pending_effects::<PlaySound>().len(), 1);
    assert_eq!(app.singleton::<LootCount>(), Some(LootCount(1)));

    assert_eq!(
        app.transact(|tx| tx.emit_to(Key::new(99, 1), Hit { amount: 1 })),
        Err(Error::MissingComponent)
    );
    assert_eq!(app.get::<Health>(enemy), Some(Health(0)));
    assert_eq!(
        app.transact(|tx| tx.put(player, Parent(sword))),
        Err(Error::RuleCycle)
    );
    assert_eq!(app.get::<WorldPosition>(sword), Some(WorldPosition(15)));
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Paid(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Payment(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Outstanding(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OutstandingTotal(i64);
#[derive(Clone, Debug, Eq, PartialEq)]
struct Document(String);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Nonempty;

fn pay(payment: &Payment, paid: &mut Paid) -> Result<(), Error> {
    paid.0 = paid.0.checked_add(payment.0).ok_or(Error::Overflow)?;
    Ok(())
}

fn outstanding(amount: &Amount, paid: &Paid) -> Result<Outstanding, Error> {
    Ok(Outstanding(amount.0.checked_sub(paid.0).ok_or(Error::Overflow)?))
}

fn outstanding_total(cx: &mut RuleCx<'_, '_>) -> Result<Option<OutstandingTotal>, Error> {
    let total = cx
        .all::<Outstanding>()?
        .into_iter()
        .try_fold(0_i64, |current, (_, outstanding)| {
            current.checked_add(outstanding.0).ok_or(Error::Overflow)
        })?;
    Ok(Some(OutstandingTotal(total)))
}

fn nonempty(document: &Document) -> Option<Nonempty> {
    (!document.0.trim().is_empty()).then_some(Nonempty)
}

#[test]
fn same_function_contract_supports_fallible_invoice_updates_optional_retractions_and_rollback(
) -> Result<(), Error> {
    let mut app = Engine::new();
    app.register(line_amount)?;
    app.register(outstanding)?;
    app.register(outstanding_total)?;
    app.register(pay)?;
    app.register(nonempty)?;

    let row = Key::new(9, 1);
    app.transact(|tx| {
        tx.spawn(row)?;
        tx.put(row, Price(12))?;
        tx.put(row, Quantity(3))?;
        tx.put(row, Paid(0))?;
        tx.put(row, Document("invoice".into()))
    })?;
    assert_eq!(app.get::<Amount>(row), Some(Amount(36)));
    assert_eq!(app.get::<Outstanding>(row), Some(Outstanding(36)));
    assert_eq!(app.get::<Nonempty>(row), Some(Nonempty));
    assert_eq!(app.singleton::<OutstandingTotal>(), Some(OutstandingTotal(36)));

    app.transact(|tx| {
        tx.emit_to(row, Payment(10))?;
        tx.emit_to(row, Payment(5))
    })?;
    assert_eq!(app.get::<Paid>(row), Some(Paid(15)));
    assert_eq!(app.get::<Outstanding>(row), Some(Outstanding(21)));
    assert_eq!(app.singleton::<OutstandingTotal>(), Some(OutstandingTotal(21)));

    assert_eq!(
        app.transact(|tx| tx.emit_to(row, Payment(i64::MAX))),
        Err(Error::Overflow)
    );
    assert_eq!(app.get::<Paid>(row), Some(Paid(15)));

    app.transact(|tx| tx.put(row, Document(" ".into())))?;
    assert_eq!(app.get::<Nonempty>(row), None);
    assert_eq!(app.changes::<Nonempty>(), vec![OutputChange::Removed(row)]);

    app.transact(|tx| tx.remove::<Quantity>(row))?;
    assert_eq!(app.get::<Amount>(row), None);
    assert_eq!(app.get::<Outstanding>(row), None);
    assert_eq!(app.singleton::<OutstandingTotal>(), Some(OutstandingTotal(0)));
    assert_eq!(
        app.changes::<Outstanding>(),
        vec![OutputChange::Removed(row)]
    );
    Ok(())
}
