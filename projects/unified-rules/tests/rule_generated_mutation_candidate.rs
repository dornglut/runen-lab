//! Full same-transaction admission-to-derived semantics through ONE compute
//! registration method. Research candidate only: On<T> uses a separate
//! activation *phase*, not a second public registration API or rule scheduler.
//! Advanced handlers still explicitly accept RuleCx and return Option<()>.
use unified_rules::{Each, Engine, Error, Key, On, One, RuleCx};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Health(u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MaxHealth(u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HealthPercent(u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Dead(bool);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AliveCount(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Hit {
    target: Key,
    damage: u32,
    loot: Key,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Died {
    target: Key,
    loot: Key,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Loot;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlaySound(Key);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reject(bool);

fn handle_hit(cx: &mut RuleCx<'_, '_>, hit: Hit) -> Result<Option<()>, Error> {
    let Health(before) = cx
        .get::<Health>(hit.target)?
        .ok_or(Error::MissingComponent)?;
    cx.update::<Health>(hit.target, |health| {
        health.0 = health.0.saturating_sub(hit.damage);
    })?;
    let Health(after) = cx
        .get::<Health>(hit.target)?
        .ok_or(Error::MissingComponent)?;
    if before > 0 && after == 0 {
        cx.emit(Died {
            target: hit.target,
            loot: hit.loot,
        })?;
    }
    Ok(None)
}

fn handle_death(cx: &mut RuleCx<'_, '_>, died: Died) -> Result<Option<()>, Error> {
    cx.spawn(died.loot)?;
    cx.put(died.loot, Loot)?;
    cx.request(PlaySound(died.target))?;
    Ok(None)
}

fn health_percent(cx: &mut RuleCx<'_, '_>, key: Key) -> Result<Option<HealthPercent>, Error> {
    let Some(Health(hp)) = cx.get::<Health>(key)? else {
        return Ok(None);
    };
    let Some(MaxHealth(max)) = cx.get::<MaxHealth>(key)? else {
        return Ok(None);
    };
    Ok((max != 0).then(|| {
        HealthPercent((u64::from(hp.min(max)) * 100 / u64::from(max)) as u32)
    }))
}

fn dead(cx: &mut RuleCx<'_, '_>, key: Key) -> Result<Option<Dead>, Error> {
    Ok(cx.get::<Health>(key)?.map(|h| Dead(h.0 == 0)))
}

fn alive_count(cx: &mut RuleCx<'_, '_>) -> Result<Option<AliveCount>, Error> {
    Ok(Some(AliveCount(
        cx.all::<Health>()?
            .iter()
            .filter(|(_, h)| h.0 > 0)
            .count(),
    )))
}

fn reject(cx: &mut RuleCx<'_, '_>, key: Key) -> Result<Option<Reject>, Error> {
    if cx.get::<RejectInput>(key)? == Some(RejectInput(true)) {
        return Err(Error::Overflow);
    }
    Ok(Some(Reject(false)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RejectInput(bool);

fn game() -> Result<Engine, Error> {
    let mut app = Engine::new();
    app.compute::<On<Hit>>(handle_hit)?;
    app.compute::<On<Died>>(handle_death)?;
    app.compute::<Each<HealthPercent>>(health_percent)?;
    app.compute::<Each<Dead>>(dead)?;
    app.compute::<One<AliveCount>>(|cx, ()| alive_count(cx))?;
    app.compute::<Each<Reject>>(reject)?;
    Ok(app)
}

fn seed(app: &mut Engine, target: Key) -> Result<(), Error> {
    app.transact(|tx| {
        tx.spawn(target)?;
        tx.put(target, Health(100))?;
        tx.put(target, MaxHealth(100))
    })
}

#[test]
fn admitted_hit_mutates_source_and_propagates_all_derived_results_before_commit() -> Result<(), Error> {
    let target = Key::new(1, 1);
    let loot = Key::new(2, 1);
    let mut app = game()?;
    seed(&mut app, target)?;

    app.transact(|tx| {
        tx.emit(Hit {
            target,
            damage: 25,
            loot,
        })?;
        tx.emit(Hit {
            target,
            damage: 75,
            loot,
        })
    })?;
    assert_eq!(app.get::<Health>(target), Some(Health(0)));
    assert_eq!(app.get::<HealthPercent>(target), Some(HealthPercent(0)));
    assert_eq!(app.get::<Dead>(target), Some(Dead(true)));
    assert_eq!(app.singleton::<AliveCount>(), Some(AliveCount(0)));
    assert_eq!(app.get::<Loot>(loot), Some(Loot));
    assert_eq!(app.occurrences::<Hit>().len(), 2);
    assert_eq!(app.occurrences::<Died>().len(), 1);
    assert_eq!(app.pending_effects::<PlaySound>().len(), 1);

    // Events are occurrences, not persistent sources. No replay on no-op.
    app.transact(|_| Ok(()))?;
    assert!(app.occurrences::<Hit>().is_empty());
    assert!(app.occurrences::<Died>().is_empty());
    assert_eq!(app.pending_effects::<PlaySound>().len(), 1);
    Ok(())
}

#[test]
fn failed_derived_evaluation_rolls_back_source_writes_spawn_events_and_effects() -> Result<(), Error> {
    let target = Key::new(4, 1);
    let loot = Key::new(5, 1);
    let mut app = game()?;
    seed(&mut app, target)?;
    let outcome = app.transact(|tx| {
        tx.emit(Hit {
            target,
            damage: 100,
            loot,
        })?;
        tx.put(target, RejectInput(true))
    });
    assert_eq!(outcome, Err(Error::Overflow));
    assert_eq!(app.get::<Health>(target), Some(Health(100)));
    assert_eq!(app.get::<Dead>(target), Some(Dead(false)));
    assert_eq!(app.get::<Loot>(loot), None);
    assert_eq!(app.singleton::<AliveCount>(), Some(AliveCount(1)));
    assert!(app.occurrences::<Hit>().is_empty());
    assert!(app.occurrences::<Died>().is_empty());
    assert!(app.pending_effects::<PlaySound>().is_empty());
    Ok(())
}

#[test]
fn source_write_conflicts_are_rejected_and_sequential_updates_are_ordered() -> Result<(), Error> {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SetHealth(Key, u32);
    let mut app = game()?;
    app.compute::<On<SetHealth>>(|cx, event| {
        cx.put(event.0, Health(event.1))?;
        Ok(None)
    })?;
    let target = Key::new(6, 1);
    seed(&mut app, target)?;

    // Two separate absolute writers must not silently win by order.
    let outcome = app.transact(|tx| {
        tx.emit(SetHealth(target, 90))?;
        tx.emit(SetHealth(target, 80))
    });
    assert_eq!(outcome, Err(Error::ConflictingWrite));
    assert_eq!(app.get::<Health>(target), Some(Health(100)));

    // Multiple relative hit updates do compose in admitted order.
    app.transact(|tx| {
        tx.emit(Hit {
            target,
            damage: 10,
            loot: Key::new(7, 1),
        })?;
        tx.emit(Hit {
            target,
            damage: 20,
            loot: Key::new(7, 1),
        })
    })?;
    assert_eq!(app.get::<Health>(target), Some(Health(70)));
    Ok(())
}

#[test]
fn event_phase_rejects_derived_reads_and_pure_rules_cannot_mutate_sources() -> Result<(), Error> {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct QueryDead(Key);
    let mut app = game()?;
    app.compute::<On<QueryDead>>(|cx, q| {
        let _ = cx.get::<Dead>(q.0)?;
        Ok(None)
    })?;
    let target = Key::new(8, 1);
    seed(&mut app, target)?;
    assert_eq!(
        app.transact(|tx| tx.emit(QueryDead(target))),
        Err(Error::DerivedReadDuringAdmission)
    );
    assert_eq!(app.get::<Dead>(target), Some(Dead(false)));

    let mut invalid = Engine::new();
    invalid.compute::<Each<Dead>>(|cx, key| {
        cx.put(key, Health(0))?;
        Ok(Some(Dead(true)))
    })?;
    assert_eq!(
        invalid.transact(|tx| tx.spawn(Key::new(1, 1))),
        Err(Error::MutationOutsideAdmission)
    );
    Ok(())
}

#[test]
fn invalid_spawn_or_missing_component_rejects_the_entire_input_batch() -> Result<(), Error> {
    let target = Key::new(10, 1);
    let mut app = game()?;
    seed(&mut app, target)?;
    // Died produces the same entity twice, so second spawn rejects all work.
    let loot = Key::new(11, 1);
    let result = app.transact(|tx| {
        tx.emit(Hit {
            target,
            damage: 100,
            loot,
        })?;
        tx.emit(Died { target, loot })
    });
    assert_eq!(result, Err(Error::InvalidGeneration));
    assert_eq!(app.get::<Health>(target), Some(Health(100)));
    assert_eq!(app.get::<Loot>(loot), None);
    assert!(app.pending_effects::<PlaySound>().is_empty());

    assert_eq!(
        app.transact(|tx| {
            tx.emit(Hit {
                target: Key::new(99, 1),
                damage: 5,
                loot,
            })
        }),
        Err(Error::MissingComponent)
    );
    Ok(())
}

#[test]
fn event_cycles_are_bounded_and_rejected_without_publication() -> Result<(), Error> {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Loop;
    let mut app = Engine::new();
    app.compute::<On<Loop>>(|cx, _| {
        cx.emit(Loop)?;
        Ok(None)
    })?;
    assert_eq!(
        app.transact(|tx| tx.emit(Loop)),
        Err(Error::EventStorm)
    );
    assert!(app.occurrences::<Loop>().is_empty());
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Money(i64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DoubleMoney(i64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Charge {
    invoice: Key,
    amount: i64,
}
fn charge(cx: &mut RuleCx<'_, '_>, c: Charge) -> Result<Option<()>, Error> {
    cx.update::<Money>(c.invoice, |money| {
        money.0 += c.amount;
    })?;
    Ok(None)
}

#[test]
fn non_game_consumer_uses_the_same_registration_and_atomic_rule_generated_mutations() -> Result<(), Error> {
    let mut app = Engine::new();
    app.compute::<On<Charge>>(charge)?;
    app.compute::<Each<DoubleMoney>>(|cx, key| {
        let Some(Money(money)) = cx.get::<Money>(key)? else {
            return Ok(None);
        };
        Ok(Some(DoubleMoney(money.checked_mul(2).ok_or(Error::Overflow)?)))
    })?;
    let invoice = Key::new(20, 1);
    app.transact(|tx| {
        tx.spawn(invoice)?;
        tx.put(invoice, Money(10))
    })?;
    app.transact(|tx| {
        tx.emit(Charge { invoice, amount: 2 })?;
        tx.emit(Charge { invoice, amount: 3 })
    })?;
    assert_eq!(app.get::<Money>(invoice), Some(Money(15)));
    assert_eq!(app.get::<DoubleMoney>(invoice), Some(DoubleMoney(30)));
    assert_eq!(
        app.transact(|tx| tx.emit(Charge {
            invoice,
            amount: i64::MAX,
        })),
        Err(Error::Overflow)
    );
    assert_eq!(app.get::<Money>(invoice), Some(Money(15)));
    Ok(())
}
