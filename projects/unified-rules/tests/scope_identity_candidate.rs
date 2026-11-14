//! Compile-qualified typed output scopes, not a new accepted framework API.
//! Every rule uses Engine::compute; no fake key, app-maintained index, or
//! separate scheduling path for keyless computation is introduced.
use unified_rules::{Each, Engine, Error, Key, One, SingletonChange};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Line {
    price: i64,
    qty: i64,
}
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
    engine.compute::<Each<LineTotal>>(|cx, key| {
        cx.get::<Line>(key)?
            .map(|line| {
                line.price
                    .checked_mul(line.qty)
                    .map(LineTotal)
                    .ok_or(Error::Overflow)
            })
            .transpose()
    })?;
    engine.compute::<One<InvoiceTotal>>(|cx, ()| {
        let sum = cx
            .all::<LineTotal>()?
            .into_iter()
            .try_fold(0_i64, |total, (_, line)| {
                total.checked_add(line.0).ok_or(Error::Overflow)
            })?;
        Ok(Some(InvoiceTotal(sum)))
    })?;
    engine.compute::<One<OverLimit>>(|cx, ()| {
        let Some(InvoiceTotal(total)) = cx.singleton::<InvoiceTotal>()? else {
            return Ok(None);
        };
        let Some(UpperLimit(limit)) = cx.singleton::<UpperLimit>()? else {
            return Ok(None);
        };
        Ok(Some(OverLimit(total > limit)))
    })?;
    engine.compute::<Each<GlobalFlag>>(|cx, key| {
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
        app.transact(|tx| tx.put(
            FIRST,
            Line {
                price: i64::MAX,
                qty: 2
            }
        )),
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
    assert_eq!(
        app.singleton_change::<OverLimit>(),
        Some(SingletonChange::Removed)
    );
    Ok(())
}

#[test]
fn cross_scope_self_cycle_rejects_initial_commit() -> Result<(), Error> {
    let mut app = Engine::new();
    app.compute::<One<Temp>>(|cx, ()| Ok(cx.singleton::<Link>()?.map(|value| Temp(value.0))))?;
    app.compute::<One<Link>>(|cx, ()| Ok(cx.singleton::<Temp>()?.map(|value| Link(value.0))))?;
    assert_eq!(app.transact(|_| Ok(())), Err(Error::RuleCycle));
    assert_eq!(app.singleton::<Temp>(), None);
    assert_eq!(app.singleton::<Link>(), None);
    Ok(())
}

#[test]
fn global_occurrences_use_explicit_origin_without_fabricating_a_key() -> Result<(), Error> {
    use unified_rules::{EventOrigin, ScopedOccurrence};

    let mut app = Engine::new();
    app.compute::<One<Temp>>(|cx, ()| {
        cx.emit(Pulse(1))?;
        Ok(Some(Temp(1)))
    })?;
    app.transact(|_| Ok(()))?;
    assert_eq!(app.singleton::<Temp>(), Some(Temp(1)));
    assert!(app.keys().is_empty());
    assert!(app.events::<Pulse>().is_empty()); // Older keyed-only reader.
    assert_eq!(
        app.occurrences::<Pulse>(),
        vec![ScopedOccurrence {
            source: EventOrigin::Global,
            ordinal: 0,
            value: Pulse(1),
        }]
    );
    app.transact(|_| Ok(()))?;
    assert!(app.occurrences::<Pulse>().is_empty());
    Ok(())
}

#[test]
fn same_rust_type_has_independent_keyed_and_global_output_producers() -> Result<(), Error> {
    let mut app = Engine::new();
    app.compute::<Each<Temp>>(|cx, key| Ok(cx.get::<Line>(key)?.map(|line| Temp(line.qty))))?;
    app.compute::<One<Temp>>(|cx, ()| {
        let mut sum = 0_i64;
        for (_, Temp(qty)) in cx.all::<Temp>()? {
            sum = sum.checked_add(qty).ok_or(Error::Overflow)?;
        }
        Ok(Some(Temp(sum)))
    })?;
    app.transact(|tx| {
        tx.spawn(FIRST)?;
        tx.spawn(SECOND)?;
        tx.put(FIRST, Line { price: 9, qty: 2 })?;
        tx.put(SECOND, Line { price: 4, qty: 3 })?;
        Ok(())
    })?;
    assert_eq!(app.get::<Temp>(FIRST), Some(Temp(2)));
    assert_eq!(app.get::<Temp>(SECOND), Some(Temp(3)));
    assert_eq!(app.singleton::<Temp>(), Some(Temp(5)));
    assert_eq!(
        app.transact(|tx| tx.put(FIRST, Temp(300))),
        Err(Error::DerivedIsReadOnly)
    );
    assert_eq!(
        app.transact(|tx| tx.put_singleton(Temp(300))),
        Err(Error::DerivedIsReadOnly)
    );
    app.transact(|tx| tx.remove::<Line>(FIRST))?;
    assert_eq!(app.get::<Temp>(FIRST), None);
    assert_eq!(app.singleton::<Temp>(), Some(Temp(3)));
    Ok(())
}

#[test]
fn same_input_type_in_two_scopes_is_not_aliased() -> Result<(), Error> {
    let mut app = Engine::new();
    app.transact(|tx| {
        tx.spawn(FIRST)?;
        tx.put(FIRST, Temp(2))?;
        tx.put_singleton(Temp(99))?;
        Ok(())
    })?;
    assert_eq!(app.keys(), vec![FIRST]);
    assert_eq!(app.get::<Temp>(FIRST), Some(Temp(2)));
    assert_eq!(app.singleton::<Temp>(), Some(Temp(99)));
    app.transact(|tx| tx.remove_singleton::<Temp>())?;
    assert_eq!(app.get::<Temp>(FIRST), Some(Temp(2)));
    assert_eq!(app.singleton::<Temp>(), None);
    Ok(())
}

#[test]
fn global_sum_matches_independent_mutation_oracle() -> Result<(), Error> {
    use std::collections::BTreeMap;

    let mut app = invoice()?;
    let mut generations = [1_u32; 24];
    let mut independent = BTreeMap::<Key, Line>::new();
    app.transact(|tx| {
        tx.put_singleton(UpperLimit(200))?;
        for slot in 0..24 {
            let key = Key::new(slot, 1);
            tx.spawn(key)?;
            let line = Line {
                price: i64::from(slot + 1),
                qty: i64::from((slot % 5) + 1),
            };
            tx.put(key, line)?;
            independent.insert(key, line);
        }
        Ok(())
    })?;
    for step in 0_u32..96 {
        let slot = step as usize % generations.len();
        let old_key = Key::new(slot as u32, generations[slot]);
        if step % 7 == 0 {
            let next = generations[slot] + 1;
            let new_key = Key::new(slot as u32, next);
            let line = Line {
                price: i64::from((step % 17) + 1),
                qty: i64::from((step % 7) + 1),
            };
            app.transact(|tx| {
                tx.despawn(old_key)?;
                tx.spawn(new_key)?;
                tx.put(new_key, line)
            })?;
            independent.remove(&old_key);
            independent.insert(new_key, line);
            generations[slot] = next;
        } else if step % 11 == 0 {
            app.transact(|tx| tx.remove::<Line>(old_key))?;
            independent.remove(&old_key);
        } else {
            let line = Line {
                price: i64::from((step % 29) + 1),
                qty: i64::from((step % 13) + 1),
            };
            app.transact(|tx| tx.put(old_key, line))?;
            independent.insert(old_key, line);
        }
        let expected: i64 = independent.values().map(|line| line.price * line.qty).sum();
        assert_eq!(
            app.singleton::<InvoiceTotal>(),
            Some(InvoiceTotal(expected)),
            "oracle failed after step {step}"
        );
        assert_eq!(
            app.singleton::<OverLimit>(),
            Some(OverLimit(expected > 200)),
            "derived singleton failed after step {step}"
        );
    }
    Ok(())
}
