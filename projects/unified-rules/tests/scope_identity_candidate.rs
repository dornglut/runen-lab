//! Compile-qualified typed output scopes, not a new accepted framework API.
//! Every rule uses Engine::derive; no fake key, app-maintained index, or
//! separate scheduling path for keyless computation is introduced.
use unified_rules::{Each, Engine, Error, Key, One, SingletonChange};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Line { price: i64, qty: i64 }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LineTotal(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InvoiceTotal(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UpperLimit(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OverLimit(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GlobalFlag(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Temp(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Link(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Pulse(u32);
const FIRST: Key = Key::new(0, 1);
const SECOND: Key = Key::new(1, 1);

fn invoice() -> Result<Engine, Error> {
    let mut engine = Engine::new();
    engine.derive::<Each<LineTotal>>(|cx, key| {
        cx.get::<Line>(key)?
            .map(|line| {
                line.price
                    .checked_mul(line.qty)
                    .map(LineTotal)
                    .ok_or(Error::Overflow)
            })
            .transpose()
    })?;
    engine.derive::<One<InvoiceTotal>>(|cx, ()| {
        let sum = cx.all::<LineTotal>()?.into_iter().try_fold(
            0_i64,
            |total, (_, line)| total.checked_add(line.0).ok_or(Error::Overflow),
        )?;
        Ok(Some(InvoiceTotal(sum)))
    })?;
    engine.derive::<One<OverLimit>>(|cx, ()| {
        let Some(InvoiceTotal(total)) = cx.singleton::<InvoiceTotal>()? else {
            return Ok(None);
        };
        let Some(UpperLimit(limit)) = cx.singleton::<UpperLimit>()? else {
            return Ok(None);
        };
        Ok(Some(OverLimit(total > limit)))
    })?;
    engine.derive::<Each<GlobalFlag>>(|cx, key| {
        let Some(OverLimit(over)) = cx.singleton::<OverLimit>()? else {
            return Ok(None);
        };
        if cx.get::<Line>(key)?.is_none() {
            return Ok(None);
        }
        Ok(Some(GlobalFlag(over)))
    })?;
    Ok(engine)
}

#[test]
fn cross_scope_dependencies_and_atomic_rollback() -> Result<(), Error> {
    let mut app = invoice()?;
    assert!(app.keys().is_empty());
    app.transact(|tx| {
        tx.put_singleton(UpperLimit(35))?;
        tx.spawn(FIRST)?;
        tx.spawn(SECOND)?;
        tx.put(FIRST, Line { price: 10, qty: 2 })?;
        tx.put(SECOND, Line { price: 5, qty: 3 })?;
        Ok(())
    })?;
    assert_eq!(app.keys(), vec![FIRST, SECOND]);
    assert_eq!(app.singleton::<InvoiceTotal>(), Some(InvoiceTotal(35)));
    assert_eq!(app.singleton::<OverLimit>(), Some(OverLimit(false)));
    assert_eq!(app.get::<GlobalFlag>(FIRST), Some(GlobalFlag(false)));
    assert_eq!(app.get::<GlobalFlag>(SECOND), Some(GlobalFlag(false)));
    assert_eq!(
        app.singleton_change::<InvoiceTotal>(),
        Some(SingletonChange::Added(InvoiceTotal(35)))
    );

    app.transact(|tx| tx.put(FIRST, Line { price: 10, qty: 3 }))?;
    assert_eq!(app.singleton::<InvoiceTotal>(), Some(InvoiceTotal(45)));
    assert_eq!(app.singleton::<OverLimit>(), Some(OverLimit(true)));
    assert_eq!(app.get::<GlobalFlag>(FIRST), Some(GlobalFlag(true)));
    assert_eq!(app.get::<GlobalFlag>(SECOND), Some(GlobalFlag(true)));
    assert_eq!(
        app.singleton_change::<InvoiceTotal>(),
        Some(SingletonChange::Updated(InvoiceTotal(45)))
    );

    app.transact(|tx| tx.put_singleton(UpperLimit(60)))?;
    assert_eq!(app.singleton::<InvoiceTotal>(), Some(InvoiceTotal(45)));
    assert_eq!(app.singleton::<OverLimit>(), Some(OverLimit(false)));
    assert_eq!(app.get::<GlobalFlag>(FIRST), Some(GlobalFlag(false)));

    app.transact(|_| Ok(()))?;
    assert_eq!(app.last_evaluations(), 0);
    assert_eq!(app.singleton_change::<InvoiceTotal>(), None);

    assert_eq!(
        app.transact(|tx| tx.put_singleton(InvoiceTotal(999))),
        Err(Error::DerivedIsReadOnly)
    );
    assert_eq!(
        app.transact(|tx| tx.put(FIRST, Line { price: i64::MAX, qty: 2 })),
        Err(Error::Overflow)
    );
    assert_eq!(app.singleton::<InvoiceTotal>(), Some(InvoiceTotal(45)));
    assert_eq!(app.get::<Line>(FIRST), Some(Line { price: 10, qty: 3 }));

    app.transact(|tx| tx.despawn(SECOND))?;
    assert_eq!(app.singleton::<InvoiceTotal>(), Some(InvoiceTotal(30)));
    assert_eq!(app.keys(), vec![FIRST]);
    Ok(())
}

#[test]
fn missing_keyless_input_retracts_global_and_keyed_outputs() -> Result<(), Error> {
    let mut app = invoice()?;
    app.transact(|tx| {
        tx.spawn(FIRST)?;
        tx.put(FIRST, Line { price: 5, qty: 2 })?;
        tx.put_singleton(UpperLimit(5))?;
        Ok(())
    })?;
    assert_eq!(app.singleton::<OverLimit>(), Some(OverLimit(true)));
    app.transact(|tx| tx.remove_singleton::<UpperLimit>())?;
    assert_eq!(app.singleton::<InvoiceTotal>(), Some(InvoiceTotal(10)));
    assert_eq!(app.singleton::<OverLimit>(), None);
    assert_eq!(app.get::<GlobalFlag>(FIRST), None);
    assert_eq!(app.singleton_change::<OverLimit>(), Some(SingletonChange::Removed));
    Ok(())
}

#[test]
fn cross_scope_self_cycle_rejects_initial_commit() -> Result<(), Error> {
    let mut app = Engine::new();
    app.derive::<One<Temp>>(|cx, ()| {
        Ok(cx.singleton::<Link>()?.map(|value| Temp(value.0)))
    })?;
    app.derive::<One<Link>>(|cx, ()| {
        Ok(cx.singleton::<Temp>()?.map(|value| Link(value.0)))
    })?;
    assert_eq!(app.transact(|_| Ok(())), Err(Error::RuleCycle));
    assert_eq!(app.singleton::<Temp>(), None);
    assert_eq!(app.singleton::<Link>(), None);
    Ok(())
}

#[test]
fn global_occurrences_cannot_claim_a_fictitious_entity_origin() -> Result<(), Error> {
    let mut app = Engine::new();
    app.derive::<One<Temp>>(|cx, ()| {
        cx.emit(Pulse(1))?;
        Ok(Some(Temp(1)))
    })?;
    assert_eq!(app.transact(|_| Ok(())), Err(Error::GlobalOccurrenceUnsupported));
    assert_eq!(app.singleton::<Temp>(), None);
    assert!(app.events::<Pulse>().is_empty());
    Ok(())
}
