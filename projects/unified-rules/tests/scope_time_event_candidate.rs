//! Experimental typed time and event origins across keyed and global computations.
//! Not a general effect scheduler, a persistent event log, or an accepted API.
use unified_rules::{Each, Engine, Error, EventOrigin, Key, Occurrence, One, ScopedOccurrence};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Step { epoch: u64, dt: i64 }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Seed(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Velocity(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Position(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Contact(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ContactAt(u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GlobalEpoch(u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct KeyEpoch(u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Rejected;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Pulse { epoch: u64, kind: u8 }

const CLOCK_ROW: Key = Key::new(0, 1);
const FIRST: Key = Key::new(1, 1);
const SECOND: Key = Key::new(2, 1);

fn scoped_game() -> Result<Engine, Error> {
    let mut app = Engine::new();
    // The same Rust clock type is independently valid in two scopes.
    app.clock_in::<One<Step>>((), |step| step.epoch)?;
    app.clock_in::<Each<Step>>(CLOCK_ROW, |step| step.epoch)?;

    // Rule 0: keyless event producer, no fabricated source Key.
    app.compute::<One<GlobalEpoch>>(|cx, ()| {
        let Some(step) = cx.advanced_in::<One<Step>>(())? else {
            return Ok(cx.previous_singleton::<GlobalEpoch>());
        };
        cx.emit(Pulse { epoch: step.epoch, kind: 0 })?;
        cx.emit(Pulse { epoch: step.epoch, kind: 1 })?;
        Ok(Some(GlobalEpoch(step.epoch)))
    })?;

    // Rule 1: ordinary per-record temporal feedback, driven by the same
    // keyless typed clock without requiring an application scheduler.
    app.compute::<Each<Position>>(|cx, key| {
        let Some(Seed(seed)) = cx.get::<Seed>(key)? else {
            return Ok(None);
        };
        let Some(Velocity(velocity)) = cx.get::<Velocity>(key)? else {
            return Ok(None);
        };
        let previous = cx.previous::<Position>(key).unwrap_or(Position(seed));
        let Some(step) = cx.advanced_in::<One<Step>>(())? else {
            return Ok(Some(previous));
        };
        if cx.previous::<Position>(key).is_none() {
            return Ok(Some(previous));
        }
        let movement = velocity.checked_mul(step.dt).ok_or(Error::Overflow)?;
        Ok(Some(Position(previous.0.checked_add(movement).ok_or(Error::Overflow)?)))
    })?;

    // Rule 2: keyed event producer, sharing one occurrence type with Rule 0.
    app.compute::<Each<ContactAt>>(|cx, key| {
        if cx.get::<Contact>(key)? != Some(Contact(true)) {
            return Ok(None);
        }
        let Some(step) = cx.advanced_in::<One<Step>>(())? else {
            return Ok(cx.previous::<ContactAt>(key));
        };
        cx.emit(Pulse { epoch: step.epoch, kind: 2 })?;
        Ok(Some(ContactAt(step.epoch)))
    })?;

    // Rule 3: independent keyed clock, observed by a keyless computation.
    app.compute::<One<KeyEpoch>>(|cx, ()| {
        if let Some(step) = cx.advanced_in::<Each<Step>>(CLOCK_ROW)? {
            return Ok(Some(KeyEpoch(step.epoch)));
        }
        Ok(Some(cx.previous_singleton::<KeyEpoch>().unwrap_or(KeyEpoch(0))))
    })?;

    // Rule 4: failure after both event producers, to test atomic publication.
    app.compute::<One<Rejected>>(|cx, ()| {
        if cx.advanced_in::<One<Step>>(())?.is_some_and(|step| step.epoch == 3) {
            return Err(Error::Overflow);
        }
        Ok(None)
    })?;
    Ok(app)
}

fn pulse_events(epoch: u64) -> Vec<ScopedOccurrence<Pulse>> {
    vec![
        ScopedOccurrence {
            source: EventOrigin::Global,
            ordinal: 0,
            value: Pulse { epoch, kind: 0 },
        },
        ScopedOccurrence {
            source: EventOrigin::Global,
            ordinal: 1,
            value: Pulse { epoch, kind: 1 },
        },
        ScopedOccurrence {
            source: EventOrigin::Key(FIRST),
            ordinal: 0,
            value: Pulse { epoch, kind: 2 },
        },
    ]
}

#[test]
fn keyless_clock_moves_keyed_state_and_preserves_typed_event_origins() -> Result<(), Error> {
    let mut app = scoped_game()?;
    app.transact(|tx| {
        tx.spawn(CLOCK_ROW)?;
        tx.spawn(FIRST)?;
        tx.spawn(SECOND)?;
        tx.put(FIRST, Seed(10))?;
        tx.put(FIRST, Velocity(3))?;
        tx.put(FIRST, Contact(true))?;
        tx.put(SECOND, Seed(100))?;
        tx.put(SECOND, Velocity(-2))?;
        Ok(())
    })?;
    assert_eq!(app.get::<Position>(FIRST), Some(Position(10)));
    assert_eq!(app.get::<Position>(SECOND), Some(Position(100)));
    assert_eq!(app.singleton::<KeyEpoch>(), Some(KeyEpoch(0)));
    assert!(app.occurrences::<Pulse>().is_empty());

    for (epoch, dt, first, second) in [(1, 2, 16, 96), (2, -1, 13, 98)] {
        app.transact(|tx| tx.put_singleton(Step { epoch, dt }))?;
        assert_eq!(app.get::<Position>(FIRST), Some(Position(first)));
        assert_eq!(app.get::<Position>(SECOND), Some(Position(second)));
        assert_eq!(app.singleton::<GlobalEpoch>(), Some(GlobalEpoch(epoch)));
        assert_eq!(app.occurrences::<Pulse>(), pulse_events(epoch));
        assert_eq!(
            app.events::<Pulse>(),
            vec![Occurrence {
                source: FIRST,
                ordinal: 0,
                value: Pulse { epoch, kind: 2 },
            }]
        );
    }

    // An invalid step must preserve both the committed values and event snapshot.
    assert_eq!(
        app.transact(|tx| tx.put_singleton(Step { epoch: 3, dt: 1 })),
        Err(Error::Overflow)
    );
    assert_eq!(app.singleton::<Step>(), Some(Step { epoch: 2, dt: -1 }));
    assert_eq!(app.get::<Position>(FIRST), Some(Position(13)));
    assert_eq!(app.occurrences::<Pulse>(), pulse_events(2));

    // Same-value clock replacement is not a tick, and clears the old snapshot.
    app.transact(|tx| tx.put_singleton(Step { epoch: 2, dt: -1 }))?;
    assert_eq!(app.last_evaluations(), 0);
    assert!(app.occurrences::<Pulse>().is_empty());

    // Keyed clock with the *same Rust Step type* advances independently.
    app.transact(|tx| tx.put(CLOCK_ROW, Step { epoch: 1, dt: 9 }))?;
    assert_eq!(app.singleton::<KeyEpoch>(), Some(KeyEpoch(1)));
    assert_eq!(app.singleton::<GlobalEpoch>(), Some(GlobalEpoch(2)));
    assert_eq!(app.get::<Position>(FIRST), Some(Position(13)));
    assert!(app.occurrences::<Pulse>().is_empty());
    Ok(())
}

#[test]
fn multiple_scoped_clock_admission_and_negative_cases() -> Result<(), Error> {
    let mut app = scoped_game()?;
    assert_eq!(app.clock_in::<One<Step>>((), |v| v.epoch), Err(Error::DuplicateClock));
    assert_eq!(
        app.clock_in::<Each<Step>>(CLOCK_ROW, |v| v.epoch),
        Err(Error::DuplicateClock)
    );
    app.transact(|tx| tx.spawn(CLOCK_ROW))?;
    assert_eq!(
        app.transact(|tx| tx.put_singleton(Step { epoch: 0, dt: 1 })),
        Err(Error::InvalidClock)
    );
    app.transact(|tx| {
        tx.put_singleton(Step { epoch: 2, dt: 1 })?;
        tx.put(CLOCK_ROW, Step { epoch: 5, dt: 1 })?;
        Ok(())
    })?;
    assert_eq!(
        app.transact(|tx| tx.put_singleton(Step { epoch: 2, dt: 2 })),
        Err(Error::InvalidClock)
    );
    assert_eq!(
        app.transact(|tx| tx.put_singleton(Step { epoch: 1, dt: 1 })),
        Err(Error::InvalidClock)
    );
    assert_eq!(
        app.transact(|tx| tx.remove_singleton::<Step>()),
        Err(Error::InvalidClock)
    );
    assert_eq!(
        app.transact(|tx| tx.put(CLOCK_ROW, Step { epoch: 4, dt: 1 })),
        Err(Error::InvalidClock)
    );
    assert_eq!(
        app.transact(|tx| tx.remove::<Step>(CLOCK_ROW)),
        Err(Error::InvalidClock)
    );
    assert_eq!(
        app.transact(|tx| tx.despawn(CLOCK_ROW)),
        Err(Error::InvalidClock)
    );
    assert_eq!(app.singleton::<Step>(), Some(Step { epoch: 2, dt: 1 }));
    assert_eq!(app.get::<Step>(CLOCK_ROW), Some(Step { epoch: 5, dt: 1 }));
    assert_eq!(app.singleton::<KeyEpoch>(), Some(KeyEpoch(5)));
    Ok(())
}

#[test]
fn missing_clock_registration_fails_a_keyless_computation_without_commit() -> Result<(), Error> {
    let mut app = Engine::new();
    app.compute::<One<GlobalEpoch>>(|cx, ()| {
        Ok(cx.advanced_in::<One<Step>>(())?.map(|v| GlobalEpoch(v.epoch)))
    })?;
    assert_eq!(app.transact(|_| Ok(())), Err(Error::UnregisteredClock));
    assert_eq!(app.singleton::<GlobalEpoch>(), None);
    Ok(())
}

#[test]
fn independent_time_oracle_over_many_epochs_and_edits() -> Result<(), Error> {
    let mut app = Engine::new();
    app.clock_in::<One<Step>>((), |step| step.epoch)?;
    app.compute::<Each<Position>>(|cx, key| {
        let Some(Seed(start)) = cx.get::<Seed>(key)? else { return Ok(None) };
        let Some(Velocity(speed)) = cx.get::<Velocity>(key)? else { return Ok(None) };
        let before = cx.previous::<Position>(key).unwrap_or(Position(start));
        let Some(step) = cx.advanced_in::<One<Step>>(())? else {
            return Ok(Some(before));
        };
        if cx.previous::<Position>(key).is_none() {
            return Ok(Some(before));
        }
        Ok(Some(Position(
            before.0.checked_add(
                speed.checked_mul(step.dt).ok_or(Error::Overflow)?
            ).ok_or(Error::Overflow)?
        )))
    })?;

    let keys: Vec<_> = (0_u32..24).map(|slot| Key::new(slot, 1)).collect();
    let mut expected: Vec<i64> = (0..24_i64).map(|i| i * 4).collect();
    let mut speeds: Vec<i64> = (0..24_i64).map(|i| i % 7 - 3).collect();
    app.transact(|tx| {
        for (i, &key) in keys.iter().enumerate() {
            tx.spawn(key)?;
            tx.put(key, Seed(expected[i]))?;
            tx.put(key, Velocity(speeds[i]))?;
        }
        Ok(())
    })?;
    for epoch in 1_u64..=64 {
        let dt = i64::try_from(epoch % 5).map_err(|_| Error::Overflow)? - 2;
        let changed = usize::try_from(epoch % 24).map_err(|_| Error::Overflow)?;
        speeds[changed] = i64::try_from(epoch % 9).map_err(|_| Error::Overflow)? - 4;
        app.transact(|tx| {
            tx.put(keys[changed], Velocity(speeds[changed]))?;
            tx.put_singleton(Step { epoch, dt })?;
            Ok(())
        })?;
        for (i, &key) in keys.iter().enumerate() {
            expected[i] += speeds[i] * dt;
            assert_eq!(
                app.get::<Position>(key),
                Some(Position(expected[i])),
                "step {epoch} record {i}"
            );
        }
    }
    Ok(())
}
