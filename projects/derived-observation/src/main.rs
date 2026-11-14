//! Compare native ECS computed reads with retained derived-state publication.
//! No new framework or independent application transaction is implemented.
mod bevy_backend;
mod contract;
mod runen_backend;
mod uncached;

use bevy_backend::BevyBackend;
use contract::{
    Data, Expected, HeaderRow, Key, Packet, TraceCase, changed_packets, packet_digest, propose,
};
use runen_backend::RunenBackend;
use uncached::{BevyReadOnly, RunenReadOnly};
use std::collections::BTreeMap;

/// Rebuilds the retained output after each admitted batch. This is a full-recompute control,
/// not an incremental dependency engine. Repeated reads use the cached publication.
struct FullRecomputePublication {
    packets: BTreeMap<Key, Packet>,
    refreshes: usize,
}

impl FullRecomputePublication {
    fn new(initial: &Data) -> Self {
        Self {
            packets: initial.packets(),
            refreshes: 1,
        }
    }

    fn accept(&mut self, next: &Data) -> (Vec<[u32; 2]>, Vec<[u32; 2]>) {
        let next_packets = next.packets();
        let delta = changed_packets(&self.packets, &next_packets);
        self.packets = next_packets;
        self.refreshes += 1;
        delta
    }

    fn read(&self) -> &BTreeMap<Key, Packet> {
        &self.packets
    }
}

#[derive(Debug)]
struct Summary {
    accepted: usize,
    rejected: usize,
    output_changes: usize,
    removals: usize,
    direct_reads_per_backend: usize,
    full_refreshes: usize,
}

/// The independently generated Lab #9 oracle is deliberately sampled rather than
/// rewritten. The shared propose() gives all backends the same admitted edit plan,
/// but must not be described as an atomic transaction implemented by either ECS.
fn replay() -> Summary {
    let fixture = include_str!("../fixtures/oracle-prefix.jsonl");
    let mut lines = fixture.lines().filter(|line| !line.is_empty());
    let header: HeaderRow =
        serde_json::from_str(lines.next().expect("oracle header")).expect("valid oracle header");

    let mut source = Data::initial();
    let mut previous = source.packets();
    assert_eq!(source.source_digest(), header.header.initial_source_digest);
    assert_eq!(packet_digest(&previous), header.header.initial_digest);

    let mut bevy = BevyBackend::new(&source);
    let mut runen = RunenBackend::new(&source);
    let mut bevy_uncached = BevyReadOnly::new(&source);
    let mut runen_uncached = RunenReadOnly::new(&source);
    let mut retained = FullRecomputePublication::new(&source);

    let mut accepted = 0;
    let mut rejected = 0;
    let mut output_changes = 0;
    let mut removals = 0;
    let mut direct_reads_per_backend = 0;
    let mut epoch = 0;

    for (index, json) in lines.enumerate() {
        let case: TraceCase = serde_json::from_str(json).expect("oracle row");
        let Expected {
            accepted: expected_accepted,
            error: expected_error,
            epoch: expected_epoch,
            digest,
            source_digest,
            changed,
            removed,
        } = case.expect;
        let mut publication = (Vec::new(), Vec::new());

        match propose(&source, &case.ops) {
            Ok(next) => {
                assert!(expected_accepted, "unexpected accepted edit at {index}");
                assert!(expected_error.is_none());
                bevy.apply(&source, &next.data, &next.dirty);
                runen.apply(&source, &next.data, &next.dirty);
                bevy_uncached.apply(&source, &next.data, &next.dirty);
                runen_uncached.apply(&source, &next.data, &next.dirty);
                publication = retained.accept(&next.data);
                source = next.data;
                epoch += 1;
                accepted += 1;
            }
            Err(error) => {
                assert!(!expected_accepted, "unexpected rejected edit at {index}");
                assert_eq!(Some(error), expected_error.as_deref());
                rejected += 1;
            }
        }

        let current = source.packets();
        let delta = changed_packets(&previous, &current);
        assert_eq!(publication, delta, "retained publication at {index}");
        assert_eq!(delta.0, changed, "changed identities at {index}");
        assert_eq!(delta.1, removed, "removed identities at {index}");
        assert_eq!(epoch, expected_epoch, "accepted epoch at {index}");
        assert_eq!(
            source.source_digest(),
            source_digest,
            "source digest at {index}"
        );
        assert_eq!(packet_digest(&current), digest, "packet digest at {index}");

        assert_eq!(bevy.data(), source, "Bevy source at {index}");
        assert_eq!(runen.data(), source, "RunenECS source at {index}");
        assert_eq!(bevy.packets(), current, "Bevy retained packets at {index}");
        assert_eq!(
            runen.packets(),
            current,
            "RunenECS retained packets at {index}"
        );
        assert_eq!(
            retained.read(),
            &current,
            "full-recompute publisher at {index}"
        );

        // Profile A uses separate native Worlds with no derived cache or publisher.
        // Both on-demand reads repeat the entire projection.
        for _ in 0..2 {
            assert_eq!(
                bevy_uncached.packets(),
                current,
                "Bevy no-cache read at {index}"
            );
            assert_eq!(
                runen_uncached.packets(),
                current,
                "RunenECS no-cache read at {index}"
            );
            direct_reads_per_backend += 1;
        }

        // The retained implementation can also compute directly, independently
        // of its Published resource. It is not counted as a no-cache application.
        assert_eq!(bevy.computed_packets(), current);
        assert_eq!(runen.computed_packets(), current);

        output_changes += changed.len();
        removals += removed.len();
        previous = current;
    }

    assert!(accepted > 0 && rejected > 0);
    assert!(output_changes > 0 && removals > 0);
    assert_eq!(retained.refreshes, accepted + 1);

    Summary {
        accepted,
        rejected,
        output_changes,
        removals,
        direct_reads_per_backend,
        full_refreshes: retained.refreshes,
    }
}

fn main() {
    let summary = replay();
    println!(
        "accepted={} rejected={} changed={} removed={} direct_reads_per_backend={} full_refreshes={}",
        summary.accepted,
        summary.rejected,
        summary.output_changes,
        summary.removals,
        summary.direct_reads_per_backend,
        summary.full_refreshes
    );
    println!(
        "Native computed reads recompute; retained publication refreshes once per accepted batch."
    );
    println!("The snapshot deltas are harness observations, NOT native ECS change notifications.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_direct_reads_and_retained_publication_match_independent_oracle() {
        let summary = replay();
        assert!(summary.accepted > summary.rejected / 2);
        assert_eq!(
            summary.direct_reads_per_backend,
            2 * (summary.accepted + summary.rejected)
        );
        assert_eq!(summary.full_refreshes, summary.accepted + 1);
        assert!(summary.output_changes > summary.removals);
    }
}
