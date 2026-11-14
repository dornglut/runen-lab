//! Bevy buffered messages: readers have their own consumption cursors.
use crate::contract::{Case, Receipt, Snapshot, Target};
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::IntoScheduleConfigs;

#[derive(Component)]
struct Health(u32);

#[derive(Message)]
struct Hit {
    target: Entity,
    damage: u32,
    tick: u32,
}

#[derive(Message)]
struct Died(u32);

#[derive(Resource, Default)]
struct Results {
    receipts: Vec<Receipt>,
    deaths: Vec<u32>,
    second_deaths: Vec<u32>,
}

fn apply_hits(
    mut hits: MessageReader<Hit>,
    mut health: Query<&mut Health>,
    mut died: MessageWriter<Died>,
    mut results: ResMut<Results>,
) {
    for hit in hits.read() {
        match health.get_mut(hit.target) {
            Ok(mut state) => {
                let before = state.0;
                state.0 = state.0.saturating_sub(hit.damage);
                results.receipts.push(Receipt::Applied(state.0));
                if before > 0 && state.0 == 0 {
                    died.write(Died(hit.tick));
                }
            }
            Err(_) => results.receipts.push(Receipt::Rejected),
        }
    }
}

fn observe_death(mut events: MessageReader<Died>, mut results: ResMut<Results>) {
    for died in events.read() {
        results.deaths.push(died.0);
    }
}

fn audit_death(mut events: MessageReader<Died>, mut results: ResMut<Results>) {
    for died in events.read() {
        results.second_deaths.push(died.0);
    }
}

pub struct Backend {
    app: App,
    live: Entity,
    stale: Entity,
}

impl Backend {
    pub fn new() -> Self {
        let mut app = App::new();
        app.init_resource::<Results>()
            .add_message::<Hit>()
            .add_message::<Died>()
            .add_systems(Update, (apply_hits, observe_death, audit_death).chain());
        let live = app.world_mut().spawn(Health(10)).id();
        let stale = app.world_mut().spawn(Health(99)).id();
        assert!(app.world_mut().despawn(stale));
        Self { app, live, stale }
    }

    pub fn step(&mut self, case: &Case) -> Snapshot {
        for hit in &case.hits {
            let target = match hit.target {
                Target::Live => self.live,
                Target::Stale => self.stale,
            };
            self.app.world_mut().resource_mut::<Messages<Hit>>().write(Hit {
                target,
                damage: hit.damage,
                tick: case.tick,
            });
        }
        self.app.update();
        let health = self.app.world().get::<Health>(self.live).expect("live target").0;
        let mut results = self.app.world_mut().resource_mut::<Results>();
        Snapshot {
            health,
            receipts: std::mem::take(&mut results.receipts),
            deaths: std::mem::take(&mut results.deaths),
            second_consumer_deaths: std::mem::take(&mut results.second_deaths),
        }
    }
}
