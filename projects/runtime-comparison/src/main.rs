//! Three-way deterministic Rust semantic replay using independent reference fixtures.
//! Not a performance comparison; see README.md for limitations.
mod bench;
mod bevy_backend;
mod contract;
mod generated;
mod runen_backend;
mod tde;

use bevy_backend::BevyBackend;
use contract::{
    Data, Dirty, Expected, HeaderRow, Key, Packet, TraceCase, changed_packets, packet_digest,
    propose,
};
use runen_backend::RunenBackend;
use std::collections::BTreeMap;
use tde::Tde;

trait Consumer {
    fn label(&self) -> &'static str;
    fn apply(&mut self, previous: &Data, next: &Data, dirty: &Dirty);
    fn data(&self) -> Data;
    fn packets(&self) -> BTreeMap<Key, Packet>;
}

impl Consumer for Tde {
    fn label(&self) -> &'static str {
        "generated-tde"
    }
    fn apply(&mut self, previous: &Data, next: &Data, dirty: &Dirty) {
        Tde::apply(self, previous, next, dirty);
    }
    fn data(&self) -> Data {
        Tde::data(self)
    }
    fn packets(&self) -> BTreeMap<Key, Packet> {
        Tde::packets(self)
    }
}
impl Consumer for BevyBackend {
    fn label(&self) -> &'static str {
        "bevy-0.20.1"
    }
    fn apply(&mut self, previous: &Data, next: &Data, dirty: &Dirty) {
        BevyBackend::apply(self, previous, next, dirty);
    }
    fn data(&self) -> Data {
        BevyBackend::data(self)
    }
    fn packets(&self) -> BTreeMap<Key, Packet> {
        BevyBackend::packets(self)
    }
}
impl Consumer for RunenBackend {
    fn label(&self) -> &'static str {
        "runen-ecs-pinned"
    }
    fn apply(&mut self, previous: &Data, next: &Data, dirty: &Dirty) {
        RunenBackend::apply(self, previous, next, dirty);
    }
    fn data(&self) -> Data {
        RunenBackend::data(self)
    }
    fn packets(&self) -> BTreeMap<Key, Packet> {
        RunenBackend::packets(self)
    }
}

struct Outcome<'a> {
    accepted: bool,
    epoch: u64,
    error: Option<&'a str>,
    lineno: usize,
}

fn check_result(
    backend: &dyn Consumer,
    initial: &Data,
    expected: &Expected,
    prior_packets: &BTreeMap<Key, Packet>,
    outcome: Outcome<'_>,
) {
    let Outcome {
        accepted,
        epoch,
        error,
        lineno,
    } = outcome;
    let state = backend.data();
    let packets = backend.packets();
    assert_eq!(state, *initial, "{} state at {lineno}", backend.label());
    assert_eq!(
        packets,
        state.packets(),
        "{} derived state at {lineno}",
        backend.label()
    );
    let (changed, removed) = changed_packets(prior_packets, &packets);
    assert_eq!(
        accepted,
        expected.accepted,
        "{} acceptance at {lineno}",
        backend.label()
    );
    assert_eq!(
        error,
        expected.error.as_deref(),
        "{} error at {lineno}",
        backend.label()
    );
    assert_eq!(
        epoch,
        expected.epoch,
        "{} epoch at {lineno}",
        backend.label()
    );
    assert_eq!(
        state.source_digest(),
        expected.source_digest,
        "{} source digest at {lineno}",
        backend.label()
    );
    assert_eq!(
        packet_digest(&packets),
        expected.digest,
        "{} packet digest at {lineno}",
        backend.label()
    );
    assert_eq!(
        changed,
        expected.changed,
        "{} changed packets at {lineno}",
        backend.label()
    );
    assert_eq!(
        removed,
        expected.removed,
        "{} removed packets at {lineno}",
        backend.label()
    );
}

pub fn replay(text: &str) -> usize {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let header: HeaderRow =
        serde_json::from_str(lines.next().expect("oracle header")).expect("valid oracle header");
    let mut state = Data::initial();
    assert_eq!(state.source_digest(), header.header.initial_source_digest);
    assert_eq!(
        packet_digest(&state.packets()),
        header.header.initial_digest
    );
    let mut consumers: Vec<Box<dyn Consumer>> = vec![
        Box::new(Tde::new(&state)),
        Box::new(BevyBackend::new(&state)),
        Box::new(RunenBackend::new(&state)),
    ];
    let mut epoch = 0u64;
    let mut count = 0;
    for (i, line) in lines.enumerate() {
        let lineno = i + 2;
        let case: TraceCase = serde_json::from_str(line).expect("oracle JSON row");
        let result = propose(&state, &case.ops);
        let (accepted, error) = match result {
            Ok(plan) => {
                for backend in &mut consumers {
                    let before = backend.packets();
                    backend.apply(&state, &plan.data, &plan.dirty);
                    check_result(
                        backend.as_ref(),
                        &plan.data,
                        &case.expect,
                        &before,
                        Outcome {
                            accepted: true,
                            epoch: epoch + 1,
                            error: None,
                            lineno,
                        },
                    );
                }
                state = plan.data;
                epoch += 1;
                (true, None)
            }
            Err(cause) => {
                for backend in &consumers {
                    let before = backend.packets();
                    check_result(
                        backend.as_ref(),
                        &state,
                        &case.expect,
                        &before,
                        Outcome {
                            accepted: false,
                            epoch,
                            error: Some(cause),
                            lineno,
                        },
                    );
                }
                (false, Some(cause))
            }
        };
        assert_eq!(accepted, case.expect.accepted, "oracle acceptance {lineno}");
        assert_eq!(error, case.expect.error.as_deref(), "oracle error {lineno}");
        count += 1;
    }
    count
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--bench") {
        bench::run(&args[1..]);
        return;
    }
    assert!(
        args.is_empty(),
        "usage: runtime-comparison [--bench BACKEND WORKLOAD OBJECTS ITERATIONS [MODE]]"
    );
    let original = include_str!("../fixtures/oracle.jsonl");
    let extended = include_str!("../fixtures/extended.jsonl");
    println!("original cases: {}", replay(original));
    println!("extended cases: {}", replay(extended));
    println!("all three independently maintained states matched their oracles");
}

#[cfg(test)]
mod tests {
    use super::replay;

    #[test]
    fn original_oracle() {
        assert_eq!(replay(include_str!("../fixtures/oracle.jsonl")), 3_012);
    }
    #[test]
    fn adversarial_oracle() {
        assert_eq!(replay(include_str!("../fixtures/extended.jsonl")), 25_010);
    }
}
