//! Bevy triggered observers: this runner invokes triggers before any App schedule.
use crate::contract::{Case, Receipt, Snapshot, Target};
use bevy_ecs::prelude::*;

#[derive(Component)]
struct Health(u32);

#[derive(Event)]
struct Hit {
    target: Entity,
    damage: u32,
    tick: u32,
}

#[derive(Event)]
struct Died(u32);

#[derive(Resource, Default)]
struct Results {
    receipts: Vec<Receipt>,
    deaths: Vec<u32>,
    second_deaths: Vec<u32>,
}

fn react_to_hit(
    hit: On<Hit>,
    mut health: Query<&mut Health>,
    mut commands: Commands,
    mut results: ResMut<Results>,
) {
    match health.get_mut(hit.target) {
        Ok(mut state) => {
            let before = state.0;
            state.0 = state.0.saturating_sub(hit.damage);
            results.receipts.push(Receipt::Applied(state.0));
            if before > 0 && state.0 == 0 {
                commands.trigger(Died(hit.tick));
            }
        }
        Err(_) => results.receipts.push(Receipt::Rejected),
    }
}

fn observe_death(event: On<Died>, mut results: ResMut<Results>) {
    results.deaths.push(event.0);
}

fn audit_death(event: On<Died>, mut results: ResMut<Results>) {
    results.second_deaths.push(event.0);
}

pub struct Backend {
    world: World,
    live: Entity,
    stale: Entity,
}

impl Backend {
    pub fn new() -> Self {
        let mut world = World::new();
        world.insert_resource(Results::default());
        world.add_observer(react_to_hit);
        world.add_observer(observe_death);
        world.add_observer(audit_death);
        let live = world.spawn(Health(10)).id();
        let stale = world.spawn(Health(99)).id();
        assert!(world.despawn(stale));
        Self { world, live, stale }
    }

    pub fn step(&mut self, case: &Case) -> Snapshot {
        for hit in &case.hits {
            let target = match hit.target {
                Target::Live => self.live,
                Target::Stale => self.stale,
            };
            self.world.trigger(Hit {
                target,
                damage: hit.damage,
                tick: case.tick,
            });
        }
        let health = self.world.get::<Health>(self.live).expect("live target").0;
        let mut results = self.world.resource_mut::<Results>();
        Snapshot {
            health,
            receipts: std::mem::take(&mut results.receipts),
            deaths: std::mem::take(&mut results.deaths),
            second_consumer_deaths: std::mem::take(&mut results.second_deaths),
        }
    }
}
