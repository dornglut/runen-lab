//! Test-local outbox protocol for Lab #15. The requests represent work to
//! perform *after* a successful commit; the test never executes real I/O.
//! In-memory acknowledgement is NOT durable or an exactly-once guarantee.
use std::cell::Cell;
use std::rc::Rc;
use unified_rules::{Each, EffectRequest, Engine, Error, EventOrigin, Key, One};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Trigger(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GlobalProducer;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct KeyProducer;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Failure(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FailureProducer;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Message {
    number: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Upload {
    number: u32,
}

const KEY: Key = Key::new(1, 1);
const OTHER: Key = Key::new(2, 1);

fn effect_app() -> Result<Engine, Error> {
    let mut app = Engine::new();
    app.compute::<One<GlobalProducer>>(|cx, ()| {
        if cx.singleton::<Trigger>()? == Some(Trigger(true)) {
            cx.request(Message { number: 10 })?;
            cx.request(Message { number: 11 })?;
            cx.request(Upload { number: 12 })?;
        }
        Ok(Some(GlobalProducer))
    })?;
    app.compute::<Each<KeyProducer>>(|cx, key| {
        if cx.get::<Trigger>(key)? == Some(Trigger(true)) {
            cx.request(Message { number: key.slot })?;
        }
        Ok(cx.get::<Trigger>(key)?.map(|_| KeyProducer))
    })?;
    app.compute::<One<FailureProducer>>(|cx, ()| {
        if cx.singleton::<Failure>()? == Some(Failure(true)) {
            Err(Error::Overflow)
        } else {
            Ok(Some(FailureProducer))
        }
    })?;
    Ok(app)
}

#[test]
fn requests_persist_until_acknowledged_with_unique_ids_and_typed_origins() -> Result<(), Error> {
    let mut app = effect_app()?;
    app.transact(|tx| {
        tx.spawn(KEY)?;
        tx.spawn(OTHER)?;
        tx.put_singleton(Trigger(true))?;
        tx.put(KEY, Trigger(true))?;
        tx.put(OTHER, Trigger(false))?;
        Ok(())
    })?;

    let records: Vec<EffectRequest<Message>> = app.pending_effects();
    assert_eq!(records.len(), 3);
    assert_eq!(
        records
            .iter()
            .map(|request| request.value.number)
            .collect::<Vec<_>>(),
        vec![10, 11, 1]
    );
    assert_eq!(
        records
            .iter()
            .map(|request| request.origin)
            .collect::<Vec<_>>(),
        vec![
            EventOrigin::Global,
            EventOrigin::Global,
            EventOrigin::Key(KEY)
        ]
    );
    assert!(records[0].id != records[1].id);
    assert!(records[1].id != records[2].id);
    let uploads: Vec<EffectRequest<Upload>> = app.pending_effects();
    assert_eq!(uploads.len(), 1);
    assert_eq!(uploads[0].value, Upload { number: 12 });
    assert!(records.iter().all(|v| v.id != uploads[0].id));

    // Successful no-op and unrelated edits do not lose pending work.
    app.transact(|_| Ok(()))?;
    assert_eq!(app.pending_effects::<Message>(), records);
    app.transact(|tx| tx.put_singleton(Failure(false)))?;
    assert_eq!(app.pending_effects::<Message>(), records);

    // Unknown/repeated acknowledgements are idempotent.
    assert!(app.acknowledge_effect(records[0].id));
    assert!(!app.acknowledge_effect(records[0].id));
    assert!(app.acknowledge_effect(uploads[0].id));
    assert!(app.pending_effects::<Upload>().is_empty());
    assert_eq!(app.pending_effects::<Message>().len(), 2);

    // A later revision can create more work without reusing prior IDs.
    app.transact(|tx| tx.put_singleton(Trigger(false)))?;
    app.transact(|tx| tx.put_singleton(Trigger(true)))?;
    let after = app.pending_effects::<Message>();
    assert_eq!(after.len(), 4);
    assert_eq!(after[0].id, records[1].id);
    assert_eq!(after[1].id, records[2].id);
    assert!(after[2].id > records[2].id);
    assert!(after[3].id > after[2].id);
    Ok(())
}

#[test]
fn request_rollback_preserves_earlier_committed_work_without_new_ids() -> Result<(), Error> {
    let mut app = effect_app()?;
    app.transact(|tx| {
        tx.spawn(KEY)?;
        tx.put(KEY, Trigger(true))?;
        tx.put_singleton(Trigger(true))?;
        Ok(())
    })?;
    let original = app.pending_effects::<Message>();

    // A successful false trigger first retracts dependencies, then a combined
    // input change stages new effects before a later rule rejects the commit.
    app.transact(|tx| {
        tx.put(KEY, Trigger(false))?;
        tx.put_singleton(Trigger(false))?;
        Ok(())
    })?;
    let earlier = app.pending_effects::<Message>();
    assert_eq!(earlier, original);

    assert_eq!(
        app.transact(|tx| {
            tx.put(KEY, Trigger(true))?;
            tx.put_singleton(Trigger(true))?;
            tx.put_singleton(Failure(true))?;
            Ok(())
        }),
        Err(Error::Overflow)
    );
    assert_eq!(app.pending_effects::<Message>(), earlier);
    assert_eq!(app.singleton::<Trigger>(), Some(Trigger(false)));
    assert_eq!(app.get::<Trigger>(KEY), Some(Trigger(false)));

    // The next *accepted* attempt gets the next IDs, with no gaps from
    // the rejected staging. We do not promise stability across restarts.
    app.transact(|tx| {
        tx.put(KEY, Trigger(true))?;
        tx.put_singleton(Trigger(true))?;
        Ok(())
    })?;
    let after = app.pending_effects::<Message>();
    assert_eq!(after.len(), earlier.len() + 3);
    assert!(after[3].id > after[2].id);
    Ok(())
}

#[test]
fn unacknowledged_requests_survive_deletion_of_producer_record() -> Result<(), Error> {
    let mut app = effect_app()?;
    app.transact(|tx| {
        tx.spawn(KEY)?;
        tx.put(KEY, Trigger(true))?;
        Ok(())
    })?;
    let pending = app.pending_effects::<Message>();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].origin, EventOrigin::Key(KEY));
    app.transact(|tx| tx.despawn(KEY))?;
    assert_eq!(app.pending_effects::<Message>(), pending);
    assert!(app.acknowledge_effect(pending[0].id));
    assert!(app.pending_effects::<Message>().is_empty());
    Ok(())
}

#[test]
fn direct_effect_in_callback_is_not_rollback_safe_negative_oracle() -> Result<(), Error> {
    let counter = Rc::new(Cell::new(0_u32));
    let observed = Rc::clone(&counter);
    let mut app = Engine::new();
    app.compute::<One<GlobalProducer>>(move |cx, ()| {
        if cx.singleton::<Trigger>()? == Some(Trigger(true)) {
            observed.set(observed.get() + 1);
            cx.request(Message { number: 22 })?;
        }
        Ok(Some(GlobalProducer))
    })?;
    app.compute::<One<FailureProducer>>(|cx, ()| {
        if cx.singleton::<Failure>()? == Some(Failure(true)) {
            return Err(Error::Overflow);
        }
        Ok(None)
    })?;
    assert_eq!(
        app.transact(|tx| {
            tx.put_singleton(Trigger(true))?;
            tx.put_singleton(Failure(true))
        }),
        Err(Error::Overflow)
    );
    assert_eq!(counter.get(), 1);
    assert_eq!(app.singleton::<Trigger>(), None);
    assert!(app.pending_effects::<Message>().is_empty());

    // This is a *negative* result: the engine cannot undo arbitrary external
    // effects executed directly inside callbacks. No purity claim is valid.
    Ok(())
}


#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Noise(u32);

#[test]
fn unchanged_business_condition_can_duplicate_requests_after_invalidation() -> Result<(), Error> {
    let mut app = Engine::new();
    app.compute::<One<GlobalProducer>>(|cx, ()| {
        let active = cx.singleton::<Trigger>()? == Some(Trigger(true));
        let _other_dependency = cx.singleton::<Noise>()?;
        if active {
            cx.request(Message { number: 99 })?;
        }
        Ok(Some(GlobalProducer))
    })?;
    app.transact(|tx| {
        tx.put_singleton(Trigger(true))?;
        tx.put_singleton(Noise(1))
    })?;
    let first = app.pending_effects::<Message>();
    assert_eq!(first.len(), 1);

    // The business trigger stays true, but a second read dependency changes.
    app.transact(|tx| tx.put_singleton(Noise(2)))?;
    let after = app.pending_effects::<Message>();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].value, after[1].value);
    assert_ne!(after[0].id, after[1].id);
    Ok(())
}
