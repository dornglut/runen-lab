//! RunenECS public-contract discriminator for Engineering #146 / Lab #15.
//!
//! This directly consumes immutable accepted RunenECS. It does NOT adapt the
//! experimental C evaluator, promise generalized transactions, or duplicate
//! ECS entity/storage/scheduler authority.

use runen_ecs::EntityError;
use runen_ecs::prelude::*;

#[derive(ScheduleLabel)]
struct Derive;

#[derive(Debug, Component)]
struct Position(f32);

#[derive(Debug, Component)]
struct Velocity(f32);

// Deliberately neither Clone nor Eq: a valid native ECS derived component.
#[derive(Debug, Component)]
struct ProjectedX(f32);

#[derive(Resource)]
struct FixedDt(f32);

fn derive_projected_x(mut rows: Query<(&mut ProjectedX, &Position, &Velocity)>, dt: Res<FixedDt>) {
    for (projected, position, velocity) in rows.iter() {
        projected.0 = position.0 + velocity.0 * dt.0;
    }
}

#[test]
fn native_schedule_computes_non_eq_float_outputs_in_one_authoritative_world()
-> Result<(), Box<dyn std::error::Error>> {
    let mut world = World::new();
    world.insert_resource(FixedDt(0.5));
    let finite = world.spawn((Position(1.0), Velocity(3.0), ProjectedX(0.0)))?;
    let nan = world.spawn((Position(f32::NAN), Velocity(2.0), ProjectedX(0.0)))?;

    let mut runtime = Runtime::new();
    runtime.add_systems(Derive, derive_projected_x)?;
    runtime.run_schedule::<Derive>(&mut world)?;

    assert_eq!(world.require::<ProjectedX>(finite)?.0, 2.5);
    assert!(world.require::<ProjectedX>(nan)?.0.is_nan());

    // The ECS supports this type, but does not impose a domain-specific
    // validation or equality policy on the computed value.
    assert_eq!(world.query::<&ProjectedX>().iter(&world).count(), 2);
    Ok(())
}

// Two independent eager writes are not a transaction: the first is observable
// even when a later access fails. This is a counterexample to assuming that
// ordinary fallible World mutations provide C's all-or-nothing semantics.
fn eager_two_entity_update(
    world: &mut World,
    first: Entity,
    second: Entity,
) -> Result<(), EntityError> {
    world.entity_mut(first)?.require_mut::<Position>()?.0 = 42.0;
    world.entity_mut(second)?.require_mut::<Position>()?.0 = 43.0;
    Ok(())
}

// Narrow positive alternative: validate a fixed, known write set before
// modifying the World. This can prevent *these* ordinary admission errors,
// but is NOT a public arbitrary-operation transaction/rollback mechanism.
fn preflight_two_entity_update(
    world: &mut World,
    first: Entity,
    second: Entity,
) -> Result<(), EntityError> {
    world.require::<Position>(first)?;
    world.require::<Position>(second)?;
    world.entity_mut(first)?.require_mut::<Position>()?.0 = 42.0;
    world.entity_mut(second)?.require_mut::<Position>()?.0 = 43.0;
    Ok(())
}

#[test]
fn failed_eager_world_writes_retain_the_committed_prefix_but_preflight_can_reject_early()
-> Result<(), Box<dyn std::error::Error>> {
    let mut eager = World::new();
    let first = eager.spawn(Position(1.0))?;
    let lacking_position = eager.spawn(Velocity(3.0))?;
    assert!(eager_two_entity_update(&mut eager, first, lacking_position).is_err());
    assert_eq!(eager.require::<Position>(first)?.0, 42.0);

    let mut preflighted = World::new();
    let first = preflighted.spawn(Position(1.0))?;
    let lacking_position = preflighted.spawn(Velocity(3.0))?;
    let cursor = preflighted.current_change_cursor();
    assert!(preflight_two_entity_update(&mut preflighted, first, lacking_position).is_err());
    assert_eq!(preflighted.require::<Position>(first)?.0, 1.0);
    assert_eq!(preflighted.current_change_cursor(), cursor);

    let second = preflighted.spawn(Position(2.0))?;
    preflight_two_entity_update(&mut preflighted, first, second)?;
    assert_eq!(preflighted.require::<Position>(first)?.0, 42.0);
    assert_eq!(preflighted.require::<Position>(second)?.0, 43.0);
    Ok(())
}

struct Parent;
impl Relation for Parent {
    type Kind = Directed;
    const CONSTRAINTS: RelationConstraints = RelationConstraints::new()
        .source_cardinality(SourceCardinality::One)
        .cycles(CyclePolicy::Forbid);
}

#[test]
fn native_relation_identity_cycles_and_despawn_remain_world_owned()
-> Result<(), Box<dyn std::error::Error>> {
    let mut world = World::new();
    let root = world.spawn(Position(0.0))?;
    let middle = world.spawn(Position(1.0))?;
    let leaf = world.spawn(Position(2.0))?;

    world.relations_mut::<Parent>().insert(middle, root)?;
    world.relations_mut::<Parent>().insert(leaf, middle)?;
    assert!(world.relations_mut::<Parent>().insert(root, leaf).is_err());
    assert!(world.relations::<Parent>().contains(middle, root));
    assert!(world.relations::<Parent>().contains(leaf, middle));

    world.despawn(middle)?;
    assert!(!world.relations::<Parent>().contains(leaf, middle));
    assert!(!world.relations::<Parent>().contains(middle, root));
    assert!(world.entity(middle).is_err());
    Ok(())
}
