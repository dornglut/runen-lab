//! Bounded mutation-API alternatives against the ACTUAL C transaction kernel.
//!
//! This deliberately compares direct &mut, guarded editing, a bounded update
//! closure, a typed replacement result and a command object. Every candidate
//! writes through the same Engine::transact, without a copied rule evaluator.
//!
//! Critical boundary: all alternatives run in the EXTERNAL transaction closure.
//! RuleCx still cannot write other components, spawn or consume admitted Hit
//! occurrences. These tests prove source-transaction semantics and ergonomic
//! feasibility only, NOT rule-internal command propagation or API superiority.
use unified_rules::{Each, Engine, Error, Key, One, Transaction};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Health(u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MaxHealth(u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HealthPercent(u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Dead(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AliveCount(usize);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Audit;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Reject(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Died(Key);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlaySound(Key);
#[derive(Clone, Copy)]
struct Hit {
    target: Key,
    damage: u32,
}

// Ordinary named Rust behavior has NO dependence on the mutation API.
fn damage_direct(health: &mut Health, hit: Hit) {
    health.0 = health.0.saturating_sub(hit.damage);
}

fn damage_value(health: &Health, hit: Hit) -> Health {
    Health(health.0.saturating_sub(hit.damage))
}

#[derive(Clone, Copy)]
enum Candidate {
    DirectLocal,
    EditGuard,
    UpdateClosure,
    TypedReplacement,
    Command,
}

struct DamageCommand(Hit);
impl DamageCommand {
    fn apply(self, tx: &mut Transaction<'_>) -> Result<(), Error> {
        if !tx.update::<Health>(self.0.target, |hp| damage_direct(hp, self.0))? {
            return Err(Error::MissingEntity);
        }
        Ok(())
    }
}

fn apply(tx: &mut Transaction<'_>, candidate: Candidate, hit: Hit) -> Result<(), Error> {
    match candidate {
        Candidate::DirectLocal => {
            let mut health = tx.get::<Health>(hit.target).ok_or(Error::MissingEntity)?;
            damage_direct(&mut health, hit);
            tx.put(hit.target, health)
        }
        Candidate::EditGuard => {
            let mut health = tx.edit::<Health>(hit.target)?.ok_or(Error::MissingEntity)?;
            damage_direct(&mut health, hit);
            Ok(())
        }
        Candidate::UpdateClosure => {
            if !tx.update::<Health>(hit.target, |hp| damage_direct(hp, hit))? {
                return Err(Error::MissingEntity);
            }
            Ok(())
        }
        Candidate::TypedReplacement => {
            let health = tx.get::<Health>(hit.target).ok_or(Error::MissingEntity)?;
            tx.put(hit.target, damage_value(&health, hit))
        }
        Candidate::Command => DamageCommand(hit).apply(tx),
    }
}

fn game() -> Result<Engine, Error> {
    let mut app = Engine::new();

    app.compute::<Each<HealthPercent>>(|cx, key| {
        let Some(Health(hp)) = cx.get::<Health>(key)? else {
            return Ok(None);
        };
        let Some(MaxHealth(max)) = cx.get::<MaxHealth>(key)? else {
            return Ok(None);
        };
        if max == 0 {
            return Ok(None);
        }
        Ok(Some(HealthPercent(
            (u64::from(hp.min(max)) * 100 / u64::from(max)) as u32,
        )))
    })?;

    app.compute::<Each<Dead>>(|cx, key| {
        let Some(Health(hp)) = cx.get::<Health>(key)? else {
            return Ok(None);
        };
        let dead = hp == 0;
        if dead && cx.previous::<Dead>(key) != Some(Dead(true)) {
            cx.emit(Died(key))?;
            cx.request(PlaySound(key))?;
        }
        Ok(Some(Dead(dead)))
    })?;

    app.compute::<One<AliveCount>>(|cx, ()| {
        Ok(Some(AliveCount(
            cx.all::<Health>()?
                .iter()
                .filter(|(_, hp)| hp.0 > 0)
                .count(),
        )))
    })?;

    // Registered after the death rule: an invalid source edit rejects
    // prior staged death occurrences and pending effect requests.
    app.compute::<Each<Audit>>(|cx, key| {
        if cx.get::<Reject>(key)? == Some(Reject(true)) {
            return Err(Error::Overflow);
        }
        Ok(Some(Audit))
    })?;

    Ok(app)
}

fn seed(app: &mut Engine, key: Key) -> Result<(), Error> {
    app.transact(|tx| {
        tx.spawn(key)?;
        tx.put(key, Health(100))?;
        tx.put(key, MaxHealth(100))
    })
}

#[test]
fn five_mutation_surfaces_reach_the_same_committed_result() -> Result<(), Error> {
    let candidates = [
        Candidate::DirectLocal,
        Candidate::EditGuard,
        Candidate::UpdateClosure,
        Candidate::TypedReplacement,
        Candidate::Command,
    ];
    for candidate in candidates {
        let key = Key::new(1, 1);
        let mut app = game()?;
        seed(&mut app, key)?;
        app.transact(|tx| {
            apply(
                tx,
                candidate,
                Hit {
                    target: key,
                    damage: 25,
                },
            )?;
            // Sequential edits of the SAME component must see staged state.
            assert_eq!(tx.get::<Health>(key), Some(Health(75)));
            apply(
                tx,
                candidate,
                Hit {
                    target: key,
                    damage: 75,
                },
            )?;
            assert_eq!(tx.get::<Health>(key), Some(Health(0)));
            Ok(())
        })?;

        assert_eq!(app.get::<Health>(key), Some(Health(0)));
        assert_eq!(app.get::<HealthPercent>(key), Some(HealthPercent(0)));
        assert_eq!(app.get::<Dead>(key), Some(Dead(true)));
        assert_eq!(app.singleton::<AliveCount>(), Some(AliveCount(0)));
        assert_eq!(app.occurrences::<Died>().len(), 1);
        assert_eq!(app.pending_effects::<PlaySound>().len(), 1);

        // Empty successful admission clears ephemeral occurrences, not
        // outstanding effect requests or already committed derived results.
        app.transact(|_| Ok(()))?;
        assert!(app.occurrences::<Died>().is_empty());
        assert_eq!(app.pending_effects::<PlaySound>().len(), 1);
        assert_eq!(app.get::<Dead>(key), Some(Dead(true)));
    }
    Ok(())
}

#[test]
fn edit_guard_respects_derived_ownership_and_stale_entity_keys() -> Result<(), Error> {
    let mut app = game()?;
    let key = Key::new(2, 1);
    seed(&mut app, key)?;
    app.transact(|tx| {
        {
            let mut health = tx.edit::<Health>(key)?.ok_or(Error::MissingEntity)?;
            health.0 = 70;
        }
        assert_eq!(tx.get::<Health>(key), Some(Health(70)));
        assert!(
            tx.edit::<HealthPercent>(key)
                .is_err_and(|err| err == Error::DerivedIsReadOnly)
        );
        Ok(())
    })?;
    assert_eq!(app.get::<HealthPercent>(key), Some(HealthPercent(70)));

    app.transact(|tx| tx.despawn(key))?;
    assert_eq!(
        app.transact(|tx| tx.edit::<Health>(key).map(|_| ())),
        Err(Error::MissingEntity)
    );
    Ok(())
}

#[test]
fn failed_validation_discards_guard_edits_events_and_effect_requests() -> Result<(), Error> {
    let mut app = game()?;
    let key = Key::new(3, 1);
    seed(&mut app, key)?;

    let rejected = app.transact(|tx| {
        apply(
            tx,
            Candidate::EditGuard,
            Hit {
                target: key,
                damage: 100,
            },
        )?;
        tx.put(key, Reject(true))?;
        Ok(())
    });
    assert_eq!(rejected, Err(Error::Overflow));
    assert_eq!(app.get::<Health>(key), Some(Health(100)));
    assert_eq!(app.get::<HealthPercent>(key), Some(HealthPercent(100)));
    assert_eq!(app.get::<Dead>(key), Some(Dead(false)));
    assert_eq!(app.singleton::<AliveCount>(), Some(AliveCount(1)));
    assert!(app.occurrences::<Died>().is_empty());
    assert!(app.pending_effects::<PlaySound>().is_empty());
    Ok(())
}

#[test]
fn multiple_absolute_puts_currently_have_implicit_last_writer_wins() -> Result<(), Error> {
    let mut app = game()?;
    let key = Key::new(4, 1);
    seed(&mut app, key)?;

    // CONFLICT FINDING: a sequence of absolute puts silently keeps the
    // last writer. This is NOT evidence for deterministic parallel rules,
    // simultaneous updates or a well-defined multi-producer policy.
    app.transact(|tx| {
        tx.put(key, Health(60))?;
        tx.put(key, Health(90))
    })?;
    assert_eq!(app.get::<Health>(key), Some(Health(90)));
    assert_eq!(app.get::<HealthPercent>(key), Some(HealthPercent(90)));
    Ok(())
}
