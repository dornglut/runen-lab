//! Public RunenECS scheduling with application-owned event queues, not a native ECS event API.
use crate::contract::{Case, Receipt, Snapshot, Target};
use runen_ecs::prelude::*;

#[derive(Component)]
struct Health(u32);

#[derive(Clone, Copy)]
struct Hit {
    target: Entity,
    damage: u32,
    tick: u32,
}

#[derive(Resource, Default)]
struct Incoming(Vec<Hit>);

#[derive(Resource, Default)]
struct PendingDeaths(Vec<u32>);

#[derive(Resource, Default)]
struct Results {
    receipts: Vec<Receipt>,
    deaths: Vec<u32>,
    second_deaths: Vec<u32>,
}

#[derive(ScheduleLabel)]
struct LogicalTick;

#[derive(SystemSet)]
struct ApplyStage;

fn apply_hits(
    mut hits: ResMut<Incoming>,
    mut health: Query<&mut Health>,
    mut pending: ResMut<PendingDeaths>,
    mut results: ResMut<Results>,
) {
    for hit in std::mem::take(&mut hits.0) {
        match health.get(hit.target) {
            Some(state) => {
                let before = state.0;
                state.0 = state.0.saturating_sub(hit.damage);
                results.receipts.push(Receipt::Applied(state.0));
                if before > 0 && state.0 == 0 {
                    pending.0.push(hit.tick);
                }
            }
            None => results.receipts.push(Receipt::Rejected),
        }
    }
}

// Explicit, application-owned fanout. Two consumers are not native RunenECS subscribers.
fn publish_deaths(mut pending: ResMut<PendingDeaths>, mut results: ResMut<Results>) {
    for death in std::mem::take(&mut pending.0) {
        results.deaths.push(death);
        results.second_deaths.push(death);
    }
}

pub struct Backend {
    world: World,
    runtime: Runtime,
    live: Entity,
    stale: Entity,
}

impl Backend {
    pub fn new() -> Self {
        let mut world = World::new();
        world.insert_resource(Incoming::default());
        world.insert_resource(PendingDeaths::default());
        world.insert_resource(Results::default());
        let live = world.spawn(Health(10)).expect("valid live entity");
        let stale = world.spawn(Health(99)).expect("valid stale entity");
        world.despawn(stale).expect("despawn stale entity");
        let mut runtime = Runtime::new();
        runtime
            .add_systems(LogicalTick, (
                apply_hits.in_set(ApplyStage),
                publish_deaths.after(ApplyStage),
            ))
            .expect("valid sequential publication schedule");
        Self { world, runtime, live, stale }
    }

    pub fn step(&mut self, case: &Case) -> Snapshot {
        for hit in &case.hits {
            let target = match hit.target {
                Target::Live => self.live,
                Target::Stale => self.stale,
            };
            self.world.resource_mut::<Incoming>().expect("incoming").0.push(Hit {
                target,
                damage: hit.damage,
                tick: case.tick,
            });
        }
        self.runtime.run_schedule::<LogicalTick>(&mut self.world).expect("execute logical tick");
        let health = self.world.require::<Health>(self.live).expect("live target").0;
        let results = self.world.resource_mut::<Results>().expect("results");
        Snapshot {
            health,
            receipts: std::mem::take(&mut results.receipts),
            deaths: std::mem::take(&mut results.deaths),
            second_consumer_deaths: std::mem::take(&mut results.second_deaths),
        }
    }
}
