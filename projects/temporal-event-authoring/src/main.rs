//! Bounded executable reference, not a framework implementation.
mod bevy_messages;
mod bevy_observers;
mod contract;
mod runen;

use bevy_messages::Backend as BevyMessages;
use bevy_observers::Backend as BevyObservers;
use contract::Case;
use runen::Backend as Runen;
use sha2::{Digest, Sha256};

const ORACLE: &str = include_str!("../fixtures/cases.json");

fn qualify() -> usize {
    let cases: Vec<Case> = serde_json::from_str(ORACLE).expect("independent fixture");
    let mut bevy_messages = BevyMessages::new();
    let mut bevy_observers = BevyObservers::new();
    let mut runen = Runen::new();
    let mut previous_tick = 0;
    for case in &cases {
        assert!(case.tick > previous_tick, "ticks must be strictly ordered");
        previous_tick = case.tick;
        bevy_messages.step(case).check(case, "Bevy messages");
        bevy_observers.step(case).check(case, "Bevy triggered observers");
        runen.step(case).check(case, "RunenECS app-owned queue");
    }
    cases.len()
}

fn main() {
    let count = qualify();
    println!(
        "temporal_event_authoring=pass cases={count} oracle_sha256={:x}",
        Sha256::digest(ORACLE.as_bytes())
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_consumer_contracts_match_independent_trace() {
        assert_eq!(super::qualify(), 3);
    }
}
