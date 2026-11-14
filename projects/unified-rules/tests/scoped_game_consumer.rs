//! Full game-domain consumer of the same scoped compute contract as the invoice.
//! This is a correctness probe, not a performance or engine-adoption claim.
use unified_rules::{Each, Engine, Error, EventOrigin, Key, One};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Step {
    epoch: u64,
    dt: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Seed(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Velocity(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Position(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Local(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Parent(Key);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WorldPosition(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Contact(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ContactEpoch(u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SpeedSum(i64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TickMarker(u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Pulse {
    epoch: u64,
    global: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RejectSentinelTick;

fn scoped_game() -> Result<Engine, Error> {
    let mut app = Engine::new();
    app.clock_in::<One<Step>>((), |step| step.epoch)?;

    app.compute::<Each<Position>>(|cx, key| {
        let Some(Seed(seed)) = cx.get::<Seed>(key)? else {
            return Ok(None);
        };
        let Some(Velocity(velocity)) = cx.get::<Velocity>(key)? else {
            return Ok(None);
        };
        let before = cx.previous::<Position>(key).unwrap_or(Position(seed));
        let Some(step) = cx.advanced_in::<One<Step>>(())? else {
            return Ok(Some(before));
        };
        if cx.previous::<Position>(key).is_none() {
            return Ok(Some(before));
        }
        let amount = velocity.checked_mul(step.dt).ok_or(Error::Overflow)?;
        Ok(Some(Position(
            before.0.checked_add(amount).ok_or(Error::Overflow)?,
        )))
    })?;

    app.compute::<Each<WorldPosition>>(|cx, key| {
        let Some(Local(local)) = cx.get::<Local>(key)? else {
            return Ok(None);
        };
        let position = if let Some(Parent(parent)) = cx.get::<Parent>(key)? {
            if !cx.exists(parent) {
                return Err(Error::InvalidRelationship);
            }
            let WorldPosition(parent_world) = cx
                .get::<WorldPosition>(parent)?
                .ok_or(Error::InvalidRelationship)?;
            parent_world.checked_add(local).ok_or(Error::Overflow)?
        } else {
            local
        };
        Ok(Some(WorldPosition(position)))
    })?;

    app.compute::<Each<ContactEpoch>>(|cx, key| {
        if cx.get::<Contact>(key)? != Some(Contact(true)) {
            return Ok(None);
        }
        let advanced = cx.advanced_in::<One<Step>>(())?;
        let Some(step) = cx.singleton::<Step>()? else {
            return Ok(None);
        };
        if advanced.is_some() || cx.previous::<Contact>(key) != Some(Contact(true)) {
            cx.emit(Pulse {
                epoch: step.epoch,
                global: false,
            })?;
        }
        Ok(Some(ContactEpoch(step.epoch)))
    })?;

    app.compute::<One<SpeedSum>>(|cx, ()| {
        let total = cx
            .all::<Velocity>()?
            .into_iter()
            .try_fold(0_i64, |sum, (_, velocity)| {
                sum.checked_add(velocity.0).ok_or(Error::Overflow)
            })?;
        Ok(Some(SpeedSum(total)))
    })?;

    app.compute::<One<TickMarker>>(|cx, ()| {
        if let Some(step) = cx.advanced_in::<One<Step>>(())? {
            cx.emit(Pulse {
                epoch: step.epoch,
                global: true,
            })?;
            return Ok(Some(TickMarker(step.epoch)));
        }
        Ok(cx.previous_singleton::<TickMarker>())
    })?;

    // Registered last so failure follows staged events and state calculations.
    app.compute::<One<RejectSentinelTick>>(|cx, ()| {
        if cx
            .advanced_in::<One<Step>>(())?
            .is_some_and(|step| step.epoch == u64::MAX)
        {
            return Err(Error::Overflow);
        }
        Ok(None)
    })?;
    Ok(app)
}

#[test]
fn scoped_game_combines_hierarchy_temporal_state_events_and_global_values() -> Result<(), Error> {
    let mut app = scoped_game()?;
    let root = Key::new(1, 1);
    let child = Key::new(2, 1);
    let other = Key::new(3, 1);

    app.transact(|tx| {
        for (key, seed, velocity, local) in [(root, 10, 1, 5), (child, 20, 2, 2), (other, 30, 3, 9)]
        {
            tx.spawn(key)?;
            tx.put(key, Seed(seed))?;
            tx.put(key, Velocity(velocity))?;
            tx.put(key, Local(local))?;
        }
        tx.put(child, Parent(root))?;
        tx.put(child, Contact(true))?;
        Ok(())
    })?;
    assert_eq!(app.get::<Position>(child), Some(Position(20)));
    assert_eq!(app.get::<WorldPosition>(child), Some(WorldPosition(7)));
    assert_eq!(app.singleton::<SpeedSum>(), Some(SpeedSum(6)));
    assert!(app.occurrences::<Pulse>().is_empty());

    app.transact(|tx| tx.put_singleton(Step { epoch: 1, dt: 2 }))?;
    assert_eq!(app.get::<Position>(child), Some(Position(24)));
    assert_eq!(app.singleton::<TickMarker>(), Some(TickMarker(1)));
    let pulses = app.occurrences::<Pulse>();
    assert_eq!(pulses.len(), 2);
    assert!(pulses.iter().any(|p| {
        p.source == EventOrigin::Key(child)
            && p.value
                == Pulse {
                    epoch: 1,
                    global: false,
                }
    }));
    assert!(pulses.iter().any(|p| {
        p.source == EventOrigin::Global
            && p.value
                == Pulse {
                    epoch: 1,
                    global: true,
                }
    }));

    app.transact(|_| Ok(()))?;
    assert!(app.occurrences::<Pulse>().is_empty());
    app.transact(|tx| {
        tx.put(child, Velocity(4))?;
        tx.put(child, Parent(other))?;
        Ok(())
    })?;
    assert_eq!(app.get::<Position>(child), Some(Position(24)));
    assert_eq!(app.get::<WorldPosition>(child), Some(WorldPosition(11)));
    assert_eq!(app.singleton::<SpeedSum>(), Some(SpeedSum(8)));
    assert!(app.occurrences::<Pulse>().is_empty());

    app.transact(|tx| tx.put_singleton(Step { epoch: 2, dt: 3 }))?;
    assert_eq!(app.get::<Position>(child), Some(Position(36)));
    assert_eq!(app.get::<ContactEpoch>(child), Some(ContactEpoch(2)));
    assert_eq!(app.occurrences::<Pulse>().len(), 2);

    // Rejected cycle and orphaning must retain committed state and relationship.
    assert_eq!(
        app.transact(|tx| {
            tx.put(root, Parent(child))?;
            tx.put(other, Parent(root))?;
            Ok(())
        }),
        Err(Error::RuleCycle)
    );
    assert_eq!(app.get::<WorldPosition>(child), Some(WorldPosition(11)));
    assert_eq!(
        app.transact(|tx| tx.despawn(other)),
        Err(Error::InvalidRelationship)
    );
    assert_eq!(app.get::<WorldPosition>(other), Some(WorldPosition(9)));

    app.transact(|tx| {
        tx.remove::<Parent>(child)?;
        tx.despawn(other)?;
        Ok(())
    })?;
    assert_eq!(app.get::<WorldPosition>(child), Some(WorldPosition(2)));
    assert_eq!(app.singleton::<SpeedSum>(), Some(SpeedSum(5)));
    let fresh = Key::new(3, 2);
    app.transact(|tx| {
        tx.spawn(fresh)?;
        tx.put(fresh, Local(12))?;
        Ok(())
    })?;
    assert_eq!(
        app.transact(|tx| tx.put(child, Parent(other))),
        Err(Error::InvalidRelationship)
    );
    app.transact(|tx| tx.put(child, Parent(fresh)))?;
    assert_eq!(app.get::<WorldPosition>(child), Some(WorldPosition(14)));

    app.transact(|tx| tx.put_singleton(Step { epoch: 3, dt: 1 }))?;
    assert_eq!(app.get::<Position>(child), Some(Position(40)));
    let prior_events = app.occurrences::<Pulse>();
    assert_eq!(
        app.transact(|tx| tx.put_singleton(Step { epoch: u64::MAX, dt: 1 })),
        Err(Error::Overflow)
    );
    assert_eq!(app.singleton::<Step>(), Some(Step { epoch: 3, dt: 1 }));
    assert_eq!(app.get::<Position>(child), Some(Position(40)));
    assert_eq!(app.occurrences::<Pulse>(), prior_events);
    assert_eq!(
        app.transact(|tx| tx.put_singleton(Step { epoch: 2, dt: 5 })),
        Err(Error::InvalidClock)
    );
    Ok(())
}

#[test]
fn scoped_game_matches_independent_movement_hierarchy_and_reduction_oracle() -> Result<(), Error> {
    const COUNT: usize = 24;
    const ROUNDS: usize = 32;
    let mut app = scoped_game()?;
    let mut positions: Vec<i64> = (0..COUNT).map(|i| i as i64 * 5).collect();
    let mut velocities: Vec<i64> = (0..COUNT).map(|i| (i % 7) as i64).collect();
    let mut locals: Vec<i64> = (0..COUNT).map(|i| (i % 5) as i64).collect();
    let mut parents: Vec<Option<usize>> =
        (0..COUNT).map(|i| (i > 0).then(|| (i - 1) / 2)).collect();

    app.transact(|tx| {
        for i in 0..COUNT {
            tx.spawn(Key::new(i as u32 + 1, 1))?;
        }
        for i in 0..COUNT {
            let key = Key::new(i as u32 + 1, 1);
            tx.put(key, Seed(positions[i]))?;
            tx.put(key, Velocity(velocities[i]))?;
            tx.put(key, Local(locals[i]))?;
            if let Some(parent) = parents[i] {
                tx.put(key, Parent(Key::new(parent as u32 + 1, 1)))?;
            }
            if i % 7 == 0 {
                tx.put(key, Contact(true))?;
            }
        }
        Ok(())
    })?;

    for round in 0..ROUNDS {
        let i = (round * 11) % COUNT;
        let key = Key::new(i as u32 + 1, 1);
        velocities[i] = (round % 9) as i64 - 3;
        locals[i] = round as i64 - 10;
        if i > 0 {
            parents[i] = Some((round * 3) % i);
        }
        app.transact(|tx| {
            tx.put(key, Velocity(velocities[i]))?;
            tx.put(key, Local(locals[i]))?;
            if let Some(parent) = parents[i] {
                tx.put(key, Parent(Key::new(parent as u32 + 1, 1)))?;
            }
            Ok(())
        })?;
        // Non-clock changes must not integrate position.
        assert_eq!(app.get::<Position>(key), Some(Position(positions[i])));

        let dt = (round % 3) as i64 + 1;
        let epoch = round as u64 + 1;
        app.transact(|tx| tx.put_singleton(Step { epoch, dt }))?;
        let mut world = vec![0_i64; COUNT];
        for index in 0..COUNT {
            positions[index] += velocities[index] * dt;
            world[index] = locals[index] + parents[index].map_or(0, |p| world[p]);
            let id = Key::new(index as u32 + 1, 1);
            assert_eq!(
                app.get::<Position>(id),
                Some(Position(positions[index])),
                "round {round}, position {index}"
            );
            assert_eq!(
                app.get::<WorldPosition>(id),
                Some(WorldPosition(world[index])),
                "round {round}, hierarchy {index}"
            );
        }
        assert_eq!(
            app.singleton::<SpeedSum>(),
            Some(SpeedSum(velocities.iter().sum())),
            "round {round}, global sum"
        );
        let pulses = app.occurrences::<Pulse>();
        assert_eq!(pulses.len(), 1 + (0..COUNT).filter(|i| i % 7 == 0).count());
        assert!(pulses.iter().all(|event| event.value.epoch == epoch));
    }
    Ok(())
}
