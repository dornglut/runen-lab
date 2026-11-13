//! Independent downstream consumer: one registration function, three game domains.
use unified_rules::{Engine, Error, Key};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Position(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Velocity(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Delta(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NextPosition(i64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Local(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Parent(Key);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WorldPosition(i64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Contact(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ContactStart;

const CLOCK: Key = Key::new(0, 1);

fn new_game() -> Engine {
    let mut engine = Engine::new();

    // Rule 1: pure next-step state. Feedback becomes authoritative only via
    // the explicit epoch-boundary promote() operation, never same-epoch.
    engine
        .rule::<NextPosition>(|ctx, entity| {
            let Some(Position(position)) = ctx.get::<Position>(entity)? else {
                return Ok(None);
            };
            let Some(Velocity(velocity)) = ctx.get::<Velocity>(entity)? else {
                return Ok(None);
            };
            let Some(Delta(dt)) = ctx.get::<Delta>(CLOCK)? else {
                return Ok(None);
            };
            let step = velocity.checked_mul(dt).ok_or(Error::Overflow)?;
            Ok(Some(NextPosition(
                position.checked_add(step).ok_or(Error::Overflow)?,
            )))
        })
        .unwrap();

    // Rule 2: same registration path, dynamically keyed parent relation.
    // The runtime resolves parent->child dependency paths and detects cycles.
    engine
        .rule::<WorldPosition>(|ctx, entity| {
            let Some(Local(local)) = ctx.get::<Local>(entity)? else {
                return Ok(None);
            };
            let world = if let Some(Parent(parent)) = ctx.get::<Parent>(entity)? {
                if !ctx.exists(parent) {
                    return Err(Error::InvalidRelationship);
                }
                let WorldPosition(ancestor) = ctx
                    .get::<WorldPosition>(parent)?
                    .ok_or(Error::InvalidRelationship)?;
                ancestor.checked_add(local).ok_or(Error::Overflow)?
            } else {
                local
            };
            Ok(Some(WorldPosition(world)))
        })
        .unwrap();

    // Rule 3: another output using exactly the same rule interface.
    // A presence transition None->Some is an observable one-epoch insertion.
    engine
        .rule::<ContactStart>(|ctx, entity| {
            Ok(ctx
                .get::<Contact>(entity)?
                .and_then(|contact| contact.0.then_some(ContactStart)))
        })
        .unwrap();

    engine
}

#[test]
fn dense_movement_derived_output_does_not_mutate_source_state() -> Result<(), Error> {
    let mut game = new_game();
    const COUNT: u32 = 1024;
    game.transact(|tx| {
        tx.spawn(CLOCK)?;
        tx.put(CLOCK, Delta(2))?;
        for slot in 1..=COUNT {
            let key = Key::new(slot, 1);
            tx.spawn(key)?;
            tx.put(key, Position(i64::from(slot)))?;
            tx.put(key, Velocity(i64::from(slot) % 7))?;
        }
        Ok(())
    })?;
    for slot in 1..=COUNT {
        let id = Key::new(slot, 1);
        let expected = i64::from(slot) + 2 * (i64::from(slot) % 7);
        assert_eq!(game.get::<NextPosition>(id), Some(NextPosition(expected)));
        assert_eq!(game.get::<Position>(id), Some(Position(i64::from(slot))));
    }

    let target = Key::new(43, 1);
    game.transact(|tx| tx.put(target, Velocity(99)))?;
    // Only the target's NextPosition computation is invalidated.
    assert_eq!(game.last_evaluations(), 1);
    assert_eq!(game.get::<NextPosition>(target), Some(NextPosition(241)));
    Ok(())
}

#[test]
fn dynamic_hierarchy_reparenting_cycle_rejection_and_generation() -> Result<(), Error> {
    let mut game = new_game();
    let root = Key::new(1, 1);
    let weapon = Key::new(2, 1);
    let camera = Key::new(3, 1);
    game.transact(|tx| {
        for key in [root, weapon, camera] {
            tx.spawn(key)?;
        }
        tx.put(root, Local(10))?;
        tx.put(weapon, Local(4))?;
        tx.put(camera, Local(-2))?;
        tx.put(weapon, Parent(root))?;
        tx.put(camera, Parent(root))?;
        Ok(())
    })?;
    assert_eq!(game.get::<WorldPosition>(weapon), Some(WorldPosition(14)));
    assert_eq!(game.get::<WorldPosition>(camera), Some(WorldPosition(8)));

    game.transact(|tx| tx.put(root, Local(20)))?;
    assert_eq!(game.get::<WorldPosition>(weapon), Some(WorldPosition(24)));
    assert_eq!(game.get::<WorldPosition>(camera), Some(WorldPosition(18)));
    assert_eq!(game.last_evaluations(), 3);

    game.transact(|tx| tx.put(weapon, Parent(camera)))?;
    assert_eq!(game.get::<WorldPosition>(weapon), Some(WorldPosition(22)));
    let snapshot = game.get::<WorldPosition>(weapon);
    assert_eq!(
        game.transact(|tx| tx.put(root, Parent(weapon))),
        Err(Error::RuleCycle)
    );
    assert_eq!(game.get::<WorldPosition>(weapon), snapshot);
    assert_eq!(
        game.transact(|tx| tx.despawn(camera)),
        Err(Error::InvalidRelationship)
    );

    game.transact(|tx| {
        tx.remove::<Parent>(weapon)?;
        tx.despawn(camera)?;
        Ok(())
    })?;
    assert_eq!(game.get::<WorldPosition>(weapon), Some(WorldPosition(4)));
    assert_eq!(game.get::<WorldPosition>(camera), None);
    game.transact(|tx| {
        tx.spawn(Key::new(3, 2))?;
        tx.put(Key::new(3, 2), Local(7))?;
        Ok(())
    })?;
    assert_eq!(
        game.get::<WorldPosition>(Key::new(3, 2)),
        Some(WorldPosition(7))
    );
    assert_eq!(game.get::<WorldPosition>(camera), None);
    assert_eq!(
        game.transact(|tx| tx.put(weapon, Parent(camera))),
        Err(Error::InvalidRelationship)
    );
    Ok(())
}

#[test]
fn ephemeral_contact_events_once_per_presence_transition() -> Result<(), Error> {
    let mut game = new_game();
    let contact = Key::new(8, 1);
    game.transact(|tx| {
        tx.spawn(contact)?;
        tx.put(contact, Contact(true))?;
        Ok(())
    })?;
    assert_eq!(
        game.inserted::<ContactStart>(),
        vec![(contact, ContactStart)]
    );
    game.transact(|_tx| Ok(()))?;
    assert!(game.inserted::<ContactStart>().is_empty());
    game.transact(|tx| tx.put(contact, Contact(true)))?;
    assert!(game.inserted::<ContactStart>().is_empty());
    game.transact(|tx| tx.put(contact, Contact(false)))?;
    assert_eq!(game.get::<ContactStart>(contact), None);
    assert!(game.inserted::<ContactStart>().is_empty());
    game.transact(|tx| tx.put(contact, Contact(true)))?;
    assert_eq!(
        game.inserted::<ContactStart>(),
        vec![(contact, ContactStart)]
    );
    game.transact(|tx| tx.despawn(contact))?;
    assert!(game.inserted::<ContactStart>().is_empty());
    Ok(())
}

// Previously unseen domain: proving no runtime edits are needed to add a rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Health(i32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Dead;
#[test]
fn fourth_domain_requires_only_another_rule_and_source_type() -> Result<(), Error> {
    let mut game = new_game();
    game.rule::<Dead>(|ctx, key| {
        Ok(ctx
            .get::<Health>(key)?
            .and_then(|hp| (hp.0 <= 0).then_some(Dead)))
    })?;
    let id = Key::new(50, 1);
    game.transact(|tx| {
        tx.spawn(id)?;
        tx.put(id, Health(10))?;
        Ok(())
    })?;
    assert!(game.inserted::<Dead>().is_empty());
    game.transact(|tx| tx.put(id, Health(0)))?;
    assert_eq!(game.inserted::<Dead>(), vec![(id, Dead)]);
    Ok(())
}

#[test]
fn invalid_writes_and_overflow_preserve_visible_world() -> Result<(), Error> {
    let mut game = new_game();
    let player = Key::new(1, 1);
    game.transact(|tx| {
        tx.spawn(CLOCK)?;
        tx.put(CLOCK, Delta(1))?;
        tx.spawn(player)?;
        tx.put(player, Position(10))?;
        tx.put(player, Velocity(2))?;
        Ok(())
    })?;
    assert_eq!(game.get::<NextPosition>(player), Some(NextPosition(12)));
    assert_eq!(
        game.transact(|tx| tx.put(player, NextPosition(99))),
        Err(Error::DerivedIsReadOnly)
    );
    game.transact(|tx| tx.put(CLOCK, Delta(i64::MAX)))
        .err()
        .filter(|err| *err == Error::Overflow)
        .ok_or(Error::Overflow)?;
    assert_eq!(game.get::<Delta>(CLOCK), Some(Delta(1)));
    assert_eq!(game.get::<NextPosition>(player), Some(NextPosition(12)));
    assert_eq!(
        game.transact(|tx| tx.spawn(player)),
        Err(Error::InvalidGeneration)
    );
    assert_eq!(
        game.rule::<Dead>(|_, _| Ok(None)),
        Err(Error::LateRegistration)
    );
    Ok(())
}

#[test]
fn dynamic_reparent_replaces_read_dependencies() -> Result<(), Error> {
    let mut game = new_game();
    let a = Key::new(1, 1);
    let b = Key::new(2, 1);
    let child = Key::new(3, 1);
    game.transact(|tx| {
        for key in [a, b, child] {
            tx.spawn(key)?;
        }
        tx.put(a, Local(10))?;
        tx.put(b, Local(100))?;
        tx.put(child, Local(1))?;
        tx.put(child, Parent(a))?;
        Ok(())
    })?;
    assert_eq!(game.get::<WorldPosition>(child), Some(WorldPosition(11)));

    game.transact(|tx| tx.put(child, Parent(b)))?;
    assert_eq!(game.get::<WorldPosition>(child), Some(WorldPosition(101)));

    // Moving the former parent must NOT invalidate the child.
    game.transact(|tx| tx.put(a, Local(20)))?;
    assert_eq!(game.last_evaluations(), 1);
    assert_eq!(game.get::<WorldPosition>(child), Some(WorldPosition(101)));

    // Moving the new parent must invalidate parent and child, exactly.
    game.transact(|tx| tx.put(b, Local(200)))?;
    assert_eq!(game.last_evaluations(), 2);
    assert_eq!(game.get::<WorldPosition>(child), Some(WorldPosition(201)));
    Ok(())
}

#[test]
fn independently_computed_hierarchy_oracle_over_changing_tree() -> Result<(), Error> {
    let mut game = new_game();
    const COUNT: usize = 128;
    let mut local: Vec<i64> = (0..COUNT).map(|i| (i % 7) as i64).collect();
    let mut parents: Vec<Option<usize>> =
        (0..COUNT).map(|i| (i > 0).then(|| (i - 1) / 2)).collect();
    game.transact(|tx| {
        for slot in 0..COUNT {
            tx.spawn(Key::new(slot as u32, 1))?;
        }
        for slot in 0..COUNT {
            let key = Key::new(slot as u32, 1);
            tx.put(key, Local(local[slot]))?;
            if let Some(parent) = parents[slot] {
                tx.put(key, Parent(Key::new(parent as u32, 1)))?;
            }
        }
        Ok(())
    })?;

    for round in 0..128_usize {
        let slot = (round * 37 % (COUNT - 1)) + 1;
        let key = Key::new(slot as u32, 1);
        if round % 2 == 0 {
            local[slot] = round as i64 * 3 - 25;
            game.transact(|tx| tx.put(key, Local(local[slot])))?;
        } else {
            // Parent is always a lower slot, so this reference tree is acyclic.
            let parent = (round * 11) % slot;
            parents[slot] = Some(parent);
            game.transact(|tx| tx.put(key, Parent(Key::new(parent as u32, 1))))?;
        }

        // Independent, deliberately simple full-world oracle in index order.
        let mut expected = vec![0_i64; COUNT];
        for i in 0..COUNT {
            expected[i] = local[i] + parents[i].map_or(0, |p| expected[p]);
        }
        for (i, &world) in expected.iter().enumerate() {
            assert_eq!(
                game.get::<WorldPosition>(Key::new(i as u32, 1)),
                Some(WorldPosition(world)),
                "round {round}, slot {i}"
            );
        }
    }
    Ok(())
}

#[test]
fn derived_overflow_rolls_back_source_and_all_derived_outputs() -> Result<(), Error> {
    let mut game = new_game();
    let parent = Key::new(1, 1);
    let child = Key::new(2, 1);
    game.transact(|tx| {
        tx.spawn(parent)?;
        tx.spawn(child)?;
        tx.put(parent, Local(10))?;
        tx.put(child, Local(5))?;
        tx.put(child, Parent(parent))?;
        Ok(())
    })?;
    assert_eq!(game.get::<WorldPosition>(parent), Some(WorldPosition(10)));
    assert_eq!(game.get::<WorldPosition>(child), Some(WorldPosition(15)));

    assert_eq!(
        game.transact(|tx| tx.put(parent, Local(i64::MAX))),
        Err(Error::Overflow)
    );
    assert_eq!(game.get::<Local>(parent), Some(Local(10)));
    assert_eq!(game.get::<WorldPosition>(parent), Some(WorldPosition(10)));
    assert_eq!(game.get::<WorldPosition>(child), Some(WorldPosition(15)));
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SpawnPosition(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Step {
    epoch: u64,
    dt: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ContactOccurrence(u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ContactPulse {
    epoch: u64,
    pulse: u8,
}

/// Temporal candidate: *same rule API* as the hierarchy and derived-contact
/// consumers, but Position itself is rule-owned and reads the prior commit.
/// An external Step is a typed input change; no promote/copy is needed.
fn temporal_game() -> Engine {
    let mut game = Engine::new();
    game.clock::<Step>(CLOCK, |step| step.epoch).unwrap();
    game.rule::<Position>(|ctx, entity| {
        let Some(SpawnPosition(start)) = ctx.get::<SpawnPosition>(entity)? else {
            return Ok(None);
        };
        let Some(Velocity(velocity)) = ctx.get::<Velocity>(entity)? else {
            return Ok(None);
        };
        let before = ctx.previous::<Position>(entity).unwrap_or(Position(start));
        let Some(step) = ctx.advanced::<Step>(CLOCK)? else {
            return Ok(Some(before));
        };
        // A newly created entity is seeded, not retroactively integrated.
        if ctx.previous::<Position>(entity).is_none() {
            return Ok(Some(before));
        }
        let delta = velocity.checked_mul(step.dt).ok_or(Error::Overflow)?;
        Ok(Some(Position(
            before.0.checked_add(delta).ok_or(Error::Overflow)?,
        )))
    })
    .unwrap();
    game.rule::<ContactOccurrence>(|ctx, entity| {
        let Some(Contact(true)) = ctx.get::<Contact>(entity)? else {
            return Ok(None);
        };
        // Two separate occurrences from the same key and commit; neither
        // becomes externally visible if a later rule fails admission.
        let advanced = ctx.advanced::<Step>(CLOCK)?;
        let Some(step) = ctx.get::<Step>(CLOCK)? else {
            return Ok(None);
        };
        // A new contact during an already-admitted step is also an occurrence.
        if advanced.is_some() || ctx.previous::<Contact>(entity) != Some(Contact(true)) {
            ctx.emit(ContactPulse {
                epoch: step.epoch,
                pulse: 0,
            })?;
            ctx.emit(ContactPulse {
                epoch: step.epoch,
                pulse: 1,
            })?;
        }
        let previous = ctx.previous::<ContactOccurrence>(entity);
        if advanced.is_some() || previous.is_none() {
            Ok(Some(ContactOccurrence(step.epoch)))
        } else {
            Ok(previous)
        }
    })
    .unwrap();
    game
}

#[test]
fn temporal_rule_moves_without_promote_and_emits_repeated_typed_occurrences() -> Result<(), Error> {
    use unified_rules::OutputChange;

    let mut world = temporal_game();
    let player = Key::new(1, 1);
    let enemy = Key::new(2, 1);
    world.transact(|tx| {
        tx.spawn(CLOCK)?;
        tx.spawn(player)?;
        tx.spawn(enemy)?;
        tx.put(player, SpawnPosition(10))?;
        tx.put(player, Velocity(3))?;
        tx.put(enemy, Contact(true))?;
        Ok(())
    })?;
    assert_eq!(world.get::<Position>(player), Some(Position(10)));
    assert!(world.changes::<ContactOccurrence>().is_empty());

    world.transact(|tx| tx.put(CLOCK, Step { epoch: 1, dt: 2 }))?;
    assert_eq!(world.get::<Position>(player), Some(Position(16)));
    assert_eq!(
        world.changes::<ContactOccurrence>(),
        vec![OutputChange::Added(enemy, ContactOccurrence(1))]
    );

    world.transact(|_| Ok(()))?;
    assert!(world.changes::<ContactOccurrence>().is_empty());
    world.transact(|tx| tx.put(player, Velocity(4)))?;
    // Editing velocity without a new step must not move the player.
    assert_eq!(world.get::<Position>(player), Some(Position(16)));
    assert!(world.changes::<Position>().is_empty());

    world.transact(|tx| tx.put(CLOCK, Step { epoch: 2, dt: 2 }))?;
    assert_eq!(world.get::<Position>(player), Some(Position(24)));
    assert_eq!(
        world.changes::<ContactOccurrence>(),
        vec![OutputChange::Updated(enemy, ContactOccurrence(2))]
    );

    world.transact(|tx| tx.put(enemy, Contact(false)))?;
    assert_eq!(
        world.changes::<ContactOccurrence>(),
        vec![OutputChange::Removed(enemy)]
    );

    // A failed timestep must not advance state or expose event changes.
    assert_eq!(
        world.transact(|tx| tx.put(
            CLOCK,
            Step {
                epoch: 3,
                dt: i64::MAX
            }
        )),
        Err(Error::Overflow)
    );
    assert_eq!(world.get::<Position>(player), Some(Position(24)));
    assert_eq!(world.get::<Step>(CLOCK), Some(Step { epoch: 2, dt: 2 }));
    assert_eq!(
        world.changes::<ContactOccurrence>(),
        vec![OutputChange::Removed(enemy)]
    );

    world.transact(|tx| tx.put(CLOCK, Step { epoch: 3, dt: 2 }))?;
    assert_eq!(world.get::<Position>(player), Some(Position(32)));
    world.transact(|tx| tx.put(enemy, Contact(true)))?;
    assert_eq!(
        world.changes::<ContactOccurrence>(),
        vec![OutputChange::Added(enemy, ContactOccurrence(3))]
    );
    assert_eq!(world.events::<ContactPulse>().len(), 2);
    Ok(())
}

#[test]
fn temporal_rules_follow_independent_fixed_step_oracle() -> Result<(), Error> {
    let mut world = temporal_game();
    let mut expected = [0_i64; 64];
    world.transact(|tx| {
        tx.spawn(CLOCK)?;
        for (index, value) in expected.iter_mut().enumerate() {
            let key = Key::new(index as u32 + 1, 1);
            *value = index as i64 * 3;
            tx.spawn(key)?;
            tx.put(key, SpawnPosition(*value))?;
            tx.put(key, Velocity(index as i64 % 5))?;
        }
        Ok(())
    })?;
    for epoch in 1..=32_u64 {
        let dt = i64::try_from(epoch % 3 + 1).map_err(|_| Error::Overflow)?;
        world.transact(|tx| tx.put(CLOCK, Step { epoch, dt }))?;
        for (index, value) in expected.iter_mut().enumerate() {
            *value += (index as i64 % 5) * dt;
            assert_eq!(
                world.get::<Position>(Key::new(index as u32 + 1, 1)),
                Some(Position(*value)),
                "step {epoch}, key {index}"
            );
        }
    }
    Ok(())
}

#[test]
fn despawn_produces_typed_derived_removal_and_reuse_is_new_occurrence() -> Result<(), Error> {
    use unified_rules::OutputChange;

    let mut game = temporal_game();
    let original = Key::new(9, 1);
    game.transact(|tx| {
        tx.spawn(CLOCK)?;
        tx.spawn(original)?;
        tx.put(original, Contact(true))?;
        tx.put(CLOCK, Step { epoch: 1, dt: 1 })?;
        Ok(())
    })?;
    assert_eq!(
        game.changes::<ContactOccurrence>(),
        vec![OutputChange::Added(original, ContactOccurrence(1)),]
    );
    game.transact(|tx| tx.despawn(original))?;
    assert_eq!(game.get::<ContactOccurrence>(original), None);
    assert_eq!(
        game.changes::<ContactOccurrence>(),
        vec![OutputChange::Removed(original),]
    );
    let fresh = Key::new(9, 2);
    game.transact(|tx| {
        tx.spawn(fresh)?;
        tx.put(fresh, Contact(true))?;
        Ok(())
    })?;
    assert_eq!(game.get::<ContactOccurrence>(original), None);
    assert_eq!(
        game.changes::<ContactOccurrence>(),
        vec![OutputChange::Added(fresh, ContactOccurrence(1)),]
    );
    Ok(())
}

#[test]
fn clock_admission_rejects_repeats_regressions_removal_and_zero() -> Result<(), Error> {
    let mut game = temporal_game();
    game.transact(|tx| tx.spawn(CLOCK))?;
    assert_eq!(
        game.transact(|tx| tx.put(CLOCK, Step { epoch: 0, dt: 1 })),
        Err(Error::InvalidClock)
    );
    game.transact(|tx| tx.put(CLOCK, Step { epoch: 2, dt: 1 }))?;
    // Identical replacement is a legal no-op, not a new step.
    game.transact(|tx| tx.put(CLOCK, Step { epoch: 2, dt: 1 }))?;
    assert_eq!(
        game.transact(|tx| tx.put(CLOCK, Step { epoch: 2, dt: 3 })),
        Err(Error::InvalidClock)
    );
    assert_eq!(
        game.transact(|tx| tx.put(CLOCK, Step { epoch: 1, dt: 1 })),
        Err(Error::InvalidClock)
    );
    assert_eq!(
        game.transact(|tx| tx.remove::<Step>(CLOCK)),
        Err(Error::InvalidClock)
    );
    assert_eq!(
        game.transact(|tx| tx.despawn(CLOCK)),
        Err(Error::InvalidClock)
    );
    assert_eq!(game.get::<Step>(CLOCK), Some(Step { epoch: 2, dt: 1 }));
    game.transact(|tx| tx.put(CLOCK, Step { epoch: 3, dt: 1 }))?;
    assert_eq!(game.get::<Step>(CLOCK), Some(Step { epoch: 3, dt: 1 }));
    Ok(())
}

#[test]
fn occurrences_preserve_same_key_multiplicity_and_discard_failed_commit() -> Result<(), Error> {
    use unified_rules::Occurrence;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct FailsOnSecondStep;

    let mut game = temporal_game();
    // Registered last: after contact occurrences have been staged, its
    // failure must still discard all candidate output and event changes.
    game.rule::<FailsOnSecondStep>(|ctx, key| {
        let Some(Contact(true)) = ctx.get::<Contact>(key)? else {
            return Ok(None);
        };
        if ctx
            .advanced::<Step>(CLOCK)?
            .is_some_and(|step| step.epoch == 2)
        {
            return Err(Error::Overflow);
        }
        Ok(None)
    })?;

    let a = Key::new(4, 1);
    let b = Key::new(5, 1);
    game.transact(|tx| {
        tx.spawn(CLOCK)?;
        for key in [b, a] {
            tx.spawn(key)?;
            tx.put(key, Contact(true))?;
        }
        Ok(())
    })?;
    assert!(game.events::<ContactPulse>().is_empty());
    game.transact(|tx| tx.put(CLOCK, Step { epoch: 1, dt: 1 }))?;
    let expected = vec![
        Occurrence {
            source: a,
            ordinal: 0,
            value: ContactPulse { epoch: 1, pulse: 0 },
        },
        Occurrence {
            source: a,
            ordinal: 1,
            value: ContactPulse { epoch: 1, pulse: 1 },
        },
        Occurrence {
            source: b,
            ordinal: 0,
            value: ContactPulse { epoch: 1, pulse: 0 },
        },
        Occurrence {
            source: b,
            ordinal: 1,
            value: ContactPulse { epoch: 1, pulse: 1 },
        },
    ];
    assert_eq!(game.events::<ContactPulse>(), expected);
    assert_eq!(
        game.transact(|tx| tx.put(CLOCK, Step { epoch: 2, dt: 1 })),
        Err(Error::Overflow)
    );
    assert_eq!(game.get::<Step>(CLOCK), Some(Step { epoch: 1, dt: 1 }));
    assert_eq!(game.events::<ContactPulse>(), expected);
    game.transact(|_| Ok(()))?;
    assert!(game.events::<ContactPulse>().is_empty());
    Ok(())
}

#[test]
fn clock_contract_requires_registration_and_cannot_be_registered_twice() -> Result<(), Error> {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Unregistered(i32);

    let mut game = Engine::new();
    game.rule::<Unregistered>(|ctx, key| {
        let _ = ctx.advanced::<Step>(CLOCK)?;
        Ok(Some(Unregistered(key.slot as i32)))
    })?;
    assert_eq!(
        game.transact(|tx| tx.spawn(CLOCK)),
        Err(Error::UnregisteredClock)
    );

    let mut game = temporal_game();
    assert_eq!(
        game.clock::<Step>(CLOCK, |step| step.epoch),
        Err(Error::DuplicateClock)
    );
    Ok(())
}

#[test]
fn multiple_rule_producers_have_unambiguous_per_key_event_ordinals() -> Result<(), Error> {
    use unified_rules::Occurrence;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Trigger;
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct First;
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Second;
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct SharedEvent;

    let mut game = Engine::new();
    game.rule::<First>(|ctx, key| {
        if ctx.get::<Trigger>(key)?.is_some() {
            ctx.emit(SharedEvent)?;
        }
        Ok(None)
    })?;
    game.rule::<Second>(|ctx, key| {
        if ctx.get::<Trigger>(key)?.is_some() {
            ctx.emit(SharedEvent)?;
        }
        Ok(None)
    })?;
    let key = Key::new(7, 1);
    game.transact(|tx| {
        tx.spawn(key)?;
        tx.put(key, Trigger)?;
        Ok(())
    })?;
    assert_eq!(
        game.events::<SharedEvent>(),
        vec![
            Occurrence {
                source: key,
                ordinal: 0,
                value: SharedEvent,
            },
            Occurrence {
                source: key,
                ordinal: 1,
                value: SharedEvent,
            },
        ]
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Contribution(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Weighted(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Total(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ChildCount(usize);

const SUMMARY: Key = Key::new(0, 1);

/// Two independent applications of the *same* rule API: aggregation over a
/// derived typed collection and an inverse relationship count.
fn collection_game() -> Engine {
    let mut game = Engine::new();
    game.rule::<Weighted>(|ctx, key| {
        Ok(ctx
            .get::<Contribution>(key)?
            .map(|value| value.0.checked_mul(2).map(Weighted).ok_or(Error::Overflow))
            .transpose()?)
    })
    .unwrap();
    game.rule::<Total>(|ctx, key| {
        if key != SUMMARY {
            return Ok(None);
        }
        let value = ctx
            .all::<Weighted>()?
            .into_iter()
            .try_fold(0_i64, |sum, (_, contribution)| {
                sum.checked_add(contribution.0).ok_or(Error::Overflow)
            })?;
        Ok(Some(Total(value)))
    })
    .unwrap();
    game.rule::<ChildCount>(|ctx, key| {
        let count = ctx
            .all::<Parent>()?
            .into_iter()
            .filter(|(_, Parent(parent))| *parent == key)
            .count();
        Ok(Some(ChildCount(count)))
    })
    .unwrap();
    game
}

#[test]
fn aggregation_and_relation_collection_follow_independent_oracles() -> Result<(), Error> {
    let mut game = collection_game();
    const COUNT: usize = 32;
    let mut values: Vec<Option<i64>> = (0..COUNT)
        .map(|i| Some(i64::try_from(i).expect("small index")))
        .collect();
    let mut generations = [1_u32; COUNT];
    let mut parents: Vec<Option<Key>> = (0..COUNT).map(|i| (i > 0).then_some(SUMMARY)).collect();

    game.transact(|tx| {
        tx.spawn(SUMMARY)?;
        for (i, &value) in values.iter().enumerate() {
            let key = Key::new(i as u32 + 1, 1);
            tx.spawn(key)?;
            if let Some(value) = value {
                tx.put(key, Contribution(value))?;
            }
            if let Some(parent) = parents[i] {
                tx.put(key, Parent(parent))?;
            }
        }
        Ok(())
    })?;

    for round in 0..96_usize {
        let index = (round * 17) % COUNT;
        let key = Key::new(index as u32 + 1, generations[index]);
        if round % 7 == 0 && values[index].is_some() {
            game.transact(|tx| {
                tx.remove::<Contribution>(key)?;
                tx.remove::<Parent>(key)?;
                Ok(())
            })?;
            values[index] = None;
            parents[index] = None;
        } else if round % 7 == 1 && values[index].is_none() {
            let value = round as i64;
            game.transact(|tx| {
                tx.put(key, Contribution(value))?;
                tx.put(key, Parent(SUMMARY))?;
                Ok(())
            })?;
            values[index] = Some(value);
            parents[index] = Some(SUMMARY);
        } else if round % 7 == 2 && values[index].is_some() {
            // Reparent to another currently live key.
            let destination = Key::new(
                (index + 1) as u32 % COUNT as u32 + 1,
                generations[(index + 1) % COUNT],
            );
            game.transact(|tx| tx.put(key, Parent(destination)))?;
            parents[index] = Some(destination);
        } else if round % 7 == 3 {
            game.transact(|tx| {
                tx.despawn(key)?;
                tx.spawn(Key::new(key.slot, key.generation + 1))?;
                Ok(())
            })?;
            generations[index] += 1;
            values[index] = None;
            parents[index] = None;
        } else {
            let value = round as i64 - 15;
            game.transact(|tx| tx.put(key, Contribution(value)))?;
            values[index] = Some(value);
        }

        let expected_total: i64 = values.iter().flatten().sum::<i64>() * 2;
        assert_eq!(
            game.get::<Total>(SUMMARY),
            Some(Total(expected_total)),
            "round {round}: derived aggregation"
        );
        // A separate full-vector oracle; no runtime-level parent index.
        for target in 0..COUNT {
            let parent = Key::new(target as u32 + 1, generations[target]);
            let count = parents.iter().filter(|&&p| p == Some(parent)).count();
            assert_eq!(
                game.get::<ChildCount>(parent),
                Some(ChildCount(count)),
                "round {round}, target {target}"
            );
        }
        let root_count = parents.iter().filter(|&&p| p == Some(SUMMARY)).count();
        assert_eq!(
            game.get::<ChildCount>(SUMMARY),
            Some(ChildCount(root_count))
        );
    }
    Ok(())
}

#[test]
fn aggregation_tracks_new_members_deleted_members_and_rolls_back_overflow() -> Result<(), Error> {
    let mut game = collection_game();
    let a = Key::new(1, 1);
    let b = Key::new(2, 1);
    game.transact(|tx| {
        tx.spawn(SUMMARY)?;
        tx.spawn(a)?;
        tx.put(a, Contribution(3))?;
        Ok(())
    })?;
    assert_eq!(game.get::<Total>(SUMMARY), Some(Total(6)));
    game.transact(|tx| {
        tx.spawn(b)?;
        tx.put(b, Contribution(5))?;
        Ok(())
    })?;
    assert_eq!(game.get::<Total>(SUMMARY), Some(Total(16)));
    game.transact(|tx| tx.remove::<Contribution>(a))?;
    assert_eq!(game.get::<Total>(SUMMARY), Some(Total(10)));
    game.transact(|tx| tx.despawn(b))?;
    assert_eq!(game.get::<Total>(SUMMARY), Some(Total(0)));
    game.transact(|tx| tx.put(a, Contribution(1)))?;
    assert_eq!(game.get::<Total>(SUMMARY), Some(Total(2)));
    // Overflow inside the derived collection must roll back input and aggregate.
    assert_eq!(
        game.transact(|tx| tx.put(a, Contribution(i64::MAX))),
        Err(Error::Overflow)
    );
    assert_eq!(game.get::<Contribution>(a), Some(Contribution(1)));
    assert_eq!(game.get::<Weighted>(a), Some(Weighted(2)));
    assert_eq!(game.get::<Total>(SUMMARY), Some(Total(2)));
    Ok(())
}

#[test]
fn self_aggregating_rule_rejects_dependency_cycle() -> Result<(), Error> {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct SelfAggregate(i64);
    let mut game = Engine::new();
    game.rule::<SelfAggregate>(|ctx, _key| {
        let _ = ctx.all::<SelfAggregate>()?;
        Ok(Some(SelfAggregate(1)))
    })?;
    assert_eq!(
        game.transact(|tx| tx.spawn(Key::new(1, 1))),
        Err(Error::RuleCycle)
    );
    Ok(())
}
