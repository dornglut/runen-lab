//! Phase-feedback qualification: a derived occurrence triggers admitted
//! source mutation without replaying the emitting computation. This is a
//! correctness-first, bounded Lab-only candidate, not a production contract.
use unified_rules::{
    Each, Engine, Error, EventOrigin, Key, On, One, OutputChange, RuleCx, SingletonChange,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Health(u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Dead(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Hit {
    target: Key,
    damage: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Died {
    target: Key,
    loot: Key,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Loot;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LootCount(usize);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Sound(Key);

fn hit(cx: &mut RuleCx<'_, '_>, event: Hit) -> Result<Option<()>, Error> {
    cx.try_update::<Health>(event.target, |health| {
        health.0 = health.0.saturating_sub(event.damage);
        Ok(())
    })?;
    Ok(None)
}

fn death(cx: &mut RuleCx<'_, '_>, target: Key) -> Result<Option<Dead>, Error> {
    let Some(Health(health)) = cx.get::<Health>(target)? else {
        return Ok(None);
    };
    let next = Dead(health == 0);
    if next == Dead(true) && cx.previous::<Dead>(target) != Some(Dead(true)) {
        cx.emit(Died {
            target,
            loot: Key::new(target.slot + 100, 1),
        })?;
    }
    Ok(Some(next))
}

fn loot_count(cx: &mut RuleCx<'_, '_>, (): ()) -> Result<Option<LootCount>, Error> {
    Ok(Some(LootCount(cx.all::<Loot>()?.len())))
}

fn game() -> Result<Engine, Error> {
    let mut app = Engine::new();
    app.compute::<On<Hit>>(hit)?;
    app.compute::<Each<Dead>>(death)?;
    app.compute::<On<Died>>(|cx, died| {
        cx.spawn(died.loot)?;
        cx.put(died.loot, Loot)?;
        cx.request(Sound(died.target))?;
        Ok(None)
    })?;
    app.compute::<One<LootCount>>(loot_count)?;
    Ok(app)
}

fn spawn_player(app: &mut Engine, target: Key) -> Result<(), Error> {
    app.transact(|tx| {
        tx.spawn(target)?;
        tx.put(target, Health(100))
    })
}

#[test]
fn derived_death_occurrence_spawns_loot_and_recomputes_aggregate_once() -> Result<(), Error> {
    let target = Key::new(1, 1);
    let loot = Key::new(101, 1);
    let mut app = game()?;
    spawn_player(&mut app, target)?;
    assert_eq!(app.get::<Dead>(target), Some(Dead(false)));
    assert_eq!(app.singleton::<LootCount>(), Some(LootCount(0)));

    app.transact(|tx| {
        tx.emit(Hit {
            target,
            damage: 100,
        })
    })?;
    assert_eq!(app.get::<Health>(target), Some(Health(0)));
    assert_eq!(app.get::<Dead>(target), Some(Dead(true)));
    assert_eq!(app.get::<Loot>(loot), Some(Loot));
    assert_eq!(app.singleton::<LootCount>(), Some(LootCount(1)));
    assert_eq!(
        app.singleton_change::<LootCount>(),
        Some(SingletonChange::Updated(LootCount(1)))
    );
    assert_eq!(
        app.changes::<Dead>(),
        vec![OutputChange::Updated(target, Dead(true))]
    );
    let deaths = app.occurrences::<Died>();
    assert_eq!(deaths.len(), 1);
    assert_eq!(deaths[0].source, EventOrigin::Key(target));
    assert_eq!(app.pending_effects::<Sound>().len(), 1);

    // A subsequent no-op commit must not replay the death or re-request audio.
    app.transact(|_| Ok(()))?;
    assert!(app.occurrences::<Died>().is_empty());
    assert_eq!(app.pending_effects::<Sound>().len(), 1);
    assert_eq!(app.singleton::<LootCount>(), Some(LootCount(1)));
    Ok(())
}

#[test]
fn derived_source_feedback_that_would_replay_an_event_is_rejected_atomically() -> Result<(), Error>
{
    let target = Key::new(2, 1);
    let mut app = Engine::new();
    app.compute::<On<Hit>>(hit)?;
    app.compute::<Each<Dead>>(death)?;
    app.compute::<On<Died>>(|cx, died| {
        // Reviving the same target invalidates the already-effectful Dead rule.
        cx.try_update::<Health>(died.target, |health| {
            health.0 = 100;
            Ok(())
        })?;
        cx.request(Sound(died.target))?;
        Ok(None)
    })?;
    spawn_player(&mut app, target)?;

    assert_eq!(
        app.transact(|tx| tx.emit(Hit {
            target,
            damage: 100
        })),
        Err(Error::UnstableDerivedFeedback)
    );
    assert_eq!(app.get::<Health>(target), Some(Health(100)));
    assert_eq!(app.get::<Dead>(target), Some(Dead(false)));
    assert!(app.occurrences::<Hit>().is_empty());
    assert!(app.occurrences::<Died>().is_empty());
    assert!(app.pending_effects::<Sound>().is_empty());
    Ok(())
}

#[test]
fn feedback_handler_failure_discards_derived_events_and_all_changes() -> Result<(), Error> {
    let target = Key::new(3, 1);
    let mut app = game()?;
    // The deterministic loot target is occupied; reject the whole batch.
    let occupied = Key::new(103, 1);
    app.transact(|tx| {
        tx.spawn(target)?;
        tx.put(target, Health(100))?;
        tx.spawn(occupied)?;
        tx.put(occupied, Loot)
    })?;
    assert_eq!(app.singleton::<LootCount>(), Some(LootCount(1)));

    assert_eq!(
        app.transact(|tx| tx.emit(Hit {
            target,
            damage: 100
        })),
        Err(Error::InvalidGeneration)
    );
    assert_eq!(app.get::<Health>(target), Some(Health(100)));
    assert_eq!(app.get::<Dead>(target), Some(Dead(false)));
    assert_eq!(app.singleton::<LootCount>(), Some(LootCount(1)));
    assert!(app.occurrences::<Died>().is_empty());
    assert!(app.pending_effects::<Sound>().is_empty());
    Ok(())
}

#[test]
fn derived_event_can_despawn_its_producer_while_preserving_removal_delta() -> Result<(), Error> {
    let target = Key::new(4, 1);
    let mut app = Engine::new();
    app.compute::<On<Hit>>(hit)?;
    app.compute::<Each<Dead>>(death)?;
    app.compute::<On<Died>>(|cx, died| {
        cx.despawn(died.target)?;
        Ok(None)
    })?;
    spawn_player(&mut app, target)?;
    app.transact(|tx| {
        tx.emit(Hit {
            target,
            damage: 100,
        })
    })?;
    assert_eq!(app.get::<Dead>(target), None);
    assert_eq!(app.get::<Health>(target), None);
    assert_eq!(app.changes::<Dead>(), vec![OutputChange::Removed(target)]);
    assert_eq!(app.occurrences::<Died>().len(), 1);
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Invoice(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Overdue(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Reminder(Key);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReminderCount(usize);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReminderIssued;

#[test]
fn non_game_derived_invoice_transition_uses_same_feedback_mechanics() -> Result<(), Error> {
    let mut app = Engine::new();
    app.compute::<Each<Overdue>>(|cx, key| {
        let Some(Invoice(balance)) = cx.get::<Invoice>(key)? else {
            return Ok(None);
        };
        let overdue = balance > 0;
        if overdue && cx.previous::<Overdue>(key) != Some(Overdue(true)) {
            cx.emit(Reminder(key))?;
        }
        Ok(Some(Overdue(overdue)))
    })?;
    app.compute::<On<Reminder>>(|cx, reminder| {
        cx.put(reminder.0, ReminderIssued)?;
        Ok(None)
    })?;
    app.compute::<One<ReminderCount>>(|cx, ()| {
        Ok(Some(ReminderCount(cx.all::<ReminderIssued>()?.len())))
    })?;
    let invoice = Key::new(20, 1);
    app.transact(|tx| {
        tx.spawn(invoice)?;
        tx.put(invoice, Invoice(0))
    })?;
    app.transact(|tx| tx.put(invoice, Invoice(10)))?;
    assert_eq!(app.get::<Overdue>(invoice), Some(Overdue(true)));
    assert_eq!(app.get::<ReminderIssued>(invoice), Some(ReminderIssued));
    assert_eq!(app.singleton::<ReminderCount>(), Some(ReminderCount(1)));
    assert_eq!(app.occurrences::<Reminder>().len(), 1);
    Ok(())
}
