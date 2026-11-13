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
        .rule::<NextPosition, _>(|ctx, entity| {
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
        .rule::<WorldPosition, _>(|ctx, entity| {
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
        .rule::<ContactStart, _>(|ctx, entity| {
            Ok(ctx
                .get::<Contact>(entity)?
                .and_then(|contact| contact.0.then_some(ContactStart)))
        })
        .unwrap();

    engine
}

#[test]
fn dense_movement_uses_explicit_feedback_without_instantaneous_cycle() -> Result<(), Error> {
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

    game.promote::<NextPosition, Position, _>(|position| Position(position.0))?;
    for slot in 1..=COUNT {
        let id = Key::new(slot, 1);
        let expected = i64::from(slot) + 2 * (i64::from(slot) % 7);
        assert_eq!(game.get::<Position>(id), Some(Position(expected)));
    }

    let target = Key::new(43, 1);
    game.transact(|tx| tx.put(target, Velocity(99)))?;
    // Only the target's NextPosition computation is invalidated.
    assert_eq!(game.last_evaluations(), 1);
    assert_eq!(
        game.get::<NextPosition>(target),
        Some(NextPosition(243))
    );
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
    game.rule::<Dead, _>(|ctx, key| {
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
        game.rule::<Dead, _>(|_, _| Ok(None)),
        Err(Error::LateRegistration)
    );
    Ok(())
}
