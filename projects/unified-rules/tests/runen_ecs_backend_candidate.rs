//! Accepted RunenECS backend capability and C-compatibility boundary.
/// RESEARCH ONLY: this is a direct accepted RunenECS consumer, not an adapter
/// from the separate experimental C Engine. It never mirrors a World.
use runen_ecs::prelude::*;

#[derive(ScheduleLabel)]
struct Step;

#[derive(SystemSet)]
struct Mutate;

#[derive(SystemSet)]
struct Observe;

#[derive(Debug, Component)]
struct Position {
    x: f32,
    y: f32,
}

// Deliberately neither Clone nor Eq; this is a valid RunenECS component.
#[derive(Debug, Component)]
struct Velocity {
    x: f32,
    y: f32,
}

#[derive(Resource)]
struct DeltaTime(f32);

#[derive(Resource, Default)]
struct ObservedChanges(Vec<usize>);

#[derive(Component)]
struct NonClonePayload(Vec<u8>);

fn integrate(mut bodies: Query<(&mut Position, &Velocity)>, dt: Res<DeltaTime>) {
    for (position, velocity) in bodies.iter() {
        position.x += velocity.x * dt.0;
        position.y += velocity.y * dt.0;
    }
}

fn observe_changes(
    mut positions: Query<&Position, Changed<Position>>,
    mut log: ResMut<ObservedChanges>,
) {
    log.0.push(positions.iter().count());
}

fn make_world() -> Result<(World, Entity, Entity), Box<dyn std::error::Error>> {
    let mut world = World::new();
    world.insert_resource(DeltaTime(0.5));
    world.insert_resource(ObservedChanges::default());
    let moving = world.spawn((Position { x: 1.0, y: 2.0 }, Velocity { x: 3.0, y: -2.0 }))?;
    let static_payload = world.spawn((
        Position { x: 10.0, y: -10.0 },
        NonClonePayload(vec![7; 256]),
    ))?;
    Ok((world, moving, static_payload))
}

fn runen_runtime() -> Result<Runtime, Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.add_systems(
        Step,
        (
            integrate.in_set(Mutate),
            observe_changes.in_set(Observe).after(Mutate),
        ),
    )?;
    Ok(runtime)
}

#[test]
fn accepted_ecs_supports_non_eq_float_and_non_clone_payload_with_borrowed_queries()
-> Result<(), Box<dyn std::error::Error>> {
    let (mut world, moving, static_payload) = make_world()?;
    let mut runtime = runen_runtime()?;

    runtime.run_schedule::<Step>(&mut world)?;
    assert_eq!(world.require::<Position>(moving)?.x, 2.5);
    assert_eq!(world.require::<Position>(moving)?.y, 1.0);
    assert_eq!(world.require::<Position>(static_payload)?.x, 10.0);

    // Query returns references into the authoritative ECS World. No cloned
    // Position or Velocity is required; query selection is structural.
    let query = world.query::<(Entity, &Position)>();
    let found: Vec<_> = query
        .iter(&world)
        .map(|(entity, position)| (entity, position.x))
        .collect();
    assert_eq!(found.len(), 2);
    assert!(found.contains(&(moving, 2.5)));
    assert!(found.contains(&(static_payload, 10.0)));
    assert_eq!(
        world.require::<NonClonePayload>(static_payload)?.0.len(),
        256
    );
    assert_eq!(world.resource::<ObservedChanges>()?.0, vec![2]);

    // The public change filter is a conservative record of mutable access,
    // not a value-equality check. This is deliberately different from C.
    world.resource_mut::<DeltaTime>()?.0 = 0.0;
    runtime.run_schedule::<Step>(&mut world)?;
    assert_eq!(world.require::<Position>(moving)?.x, 2.5);
    assert_eq!(world.resource::<ObservedChanges>()?.0, vec![2, 1]);

    world.despawn(moving)?;
    assert!(world.entity(moving).is_err());
    let next = world.spawn(Position {
        x: f32::NAN,
        y: 0.0,
    })?;
    assert!(world.entity(next).is_ok());
    assert!(world.entity(moving).is_err());
    assert!(world.require::<Position>(next)?.x.is_nan());
    Ok(())
}

#[test]
fn float_input_remains_a_valid_ecs_component_without_equality_or_clone()
-> Result<(), Box<dyn std::error::Error>> {
    let mut world = World::new();
    let entity = world.spawn(Position {
        x: f32::NAN,
        y: f32::INFINITY,
    })?;
    assert!(world.require::<Position>(entity)?.x.is_nan());
    assert!(world.require::<Position>(entity)?.y.is_infinite());
    {
        let mut object = world.entity_mut(entity)?;
        object.require_mut::<Position>()?.y = -1.25;
    }
    assert_eq!(world.require::<Position>(entity)?.y, -1.25);
    Ok(())
}
