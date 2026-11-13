//! Exploratory same-host Release measurements in propagation and shared-transaction modes.
//! Transaction mode includes admission; fixture parsing, setup, and readback are excluded.
use crate::Consumer;
use crate::bevy_backend::BevyBackend;
use crate::contract::{Data, Dirty, Edit, Key, Obj, Packet, propose};
use crate::runen_backend::RunenBackend;
use crate::tde::Tde;
use std::collections::BTreeMap;
use std::time::Instant;

fn linux_memory_kib(field: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let row = status.lines().find(|row| row.starts_with(field))?;
    row.split_whitespace().nth(1)?.parse().ok()
}

fn memory_field(value: Option<u64>) -> String {
    value.map_or_else(|| "NA".to_owned(), |n| n.to_string())
}

fn states(size: u32, workload: &str) -> (Data, Data, Dirty) {
    let materials: BTreeMap<u32, u32> = (0..32)
        .map(|id| (id, 0x0010_1010 + id * 0x0001_0101))
        .collect();
    let versions: BTreeMap<u32, u32> = (0..size).map(|slot| (slot, 1)).collect();
    let objects = (0..size)
        .map(|slot| {
            (
                slot,
                Obj {
                    r#gen: 1,
                    x: i64::from(slot) * 4,
                    y: -(i64::from(slot) * 3),
                    radius: 2,
                    material: if workload == "fanout" { 0 } else { slot % 32 },
                },
            )
        })
        .collect();
    let before = Data {
        versions,
        objects,
        materials,
    };
    let mut after = before.clone();
    let mut dirty = Dirty::default();

    match workload {
        "noop" => {}
        "sparse" => {
            after.objects.get_mut(&0).expect("object zero").x += 1;
            dirty.objects.insert(0);
        }
        "dense" => {
            for (&slot, object) in &mut after.objects {
                object.x += 1;
                dirty.objects.insert(slot);
            }
        }
        "fanout" => {
            *after.materials.get_mut(&0).expect("material zero") ^= 0x0002_0202;
            dirty.materials.insert(0);
        }
        "reassign" => {
            for (&slot, object) in &mut after.objects {
                if slot % 16 == 0 {
                    object.material = (object.material + 1) % 32;
                    dirty.objects.insert(slot);
                }
            }
        }
        _ => panic!("unknown workload: {workload}"),
    }

    before.validate().expect("valid baseline");
    after.validate().expect("valid mutated state");
    if workload != "noop" {
        assert_ne!(before, after, "workload must have an effect");
    }
    (before, after, dirty)
}

/// Deliberately simple full-rebuild reference. Not another ECS and not a TDE backend.
struct FullRecompute {
    source: Data,
    published: BTreeMap<Key, Packet>,
}

impl FullRecompute {
    fn new(source: &Data) -> Self {
        Self {
            source: source.clone(),
            published: source.packets(),
        }
    }
}

impl Consumer for FullRecompute {
    fn label(&self) -> &'static str {
        "full-recompute-reference"
    }

    fn apply(&mut self, _previous: &Data, next: &Data, _dirty: &Dirty) {
        self.source = next.clone();
        self.published = next.packets();
    }

    fn data(&self) -> Data {
        self.source.clone()
    }

    fn packets(&self) -> BTreeMap<Key, Packet> {
        self.published.clone()
    }
}

/// Build the same legal transactions for every backend, outside timed execution.
/// The benchmark workloads change only one field family per object.
fn transition_edits(before: &Data, after: &Data, dirty: &Dirty) -> Vec<Edit> {
    let mut edits = Vec::with_capacity(dirty.objects.len() + dirty.materials.len());
    for &material in &dirty.materials {
        edits.push(Edit::Recolor {
            material,
            color: after.materials[&material],
        });
    }
    for &slot in &dirty.objects {
        let old = before.objects.get(&slot).expect("benchmark object exists");
        let new = after.objects.get(&slot).expect("benchmark object persists");
        let generation = i64::from(old.r#gen);
        assert_eq!(old.r#gen, new.r#gen);
        if old.material != new.material {
            assert_eq!((old.x, old.y, old.radius), (new.x, new.y, new.radius));
            edits.push(Edit::Reassign {
                slot,
                r#gen: generation,
                material: new.material,
            });
        } else if old.x != new.x || old.y != new.y {
            assert_eq!(old.radius, new.radius);
            edits.push(Edit::Move {
                slot,
                r#gen: generation,
                x: new.x,
                y: new.y,
            });
        } else if old.radius != new.radius {
            edits.push(Edit::Radius {
                slot,
                r#gen: generation,
                radius: new.radius,
            });
        } else {
            panic!("benchmark dirty object does not change");
        }
    }
    edits
}

fn apply_transaction(consumer: &mut dyn Consumer, source: &mut Data, ops: &[Edit]) {
    let next = propose(source, ops).expect("valid benchmark transaction");
    consumer.apply(source, &next.data, &next.dirty);
    *source = next.data;
}

pub fn run(args: &[String]) {
    assert!(
        args.len() == 4 || args.len() == 5,
        "usage: --bench BACKEND WORKLOAD OBJECTS ITERATIONS [propagation|transaction]"
    );
    let backend = args[0].as_str();
    let workload = args[1].as_str();
    let size: u32 = args[2].parse().expect("integer object count");
    let iterations: usize = args[3].parse().expect("integer iteration count");
    let mode = args.get(4).map_or("propagation", String::as_str);
    assert!(
        matches!(mode, "propagation" | "transaction"),
        "unknown benchmark mode"
    );
    assert!((32..=100_000).contains(&size), "object count out of range");
    assert!(
        (2..=100_000).contains(&iterations) && iterations.is_multiple_of(2),
        "iteration count must be even and bounded"
    );

    let (before, after, dirty) = states(size, workload);
    let forward = transition_edits(&before, &after, &dirty);
    let reverse = transition_edits(&after, &before, &dirty);
    // Validate the actual authored inputs, including reverse direction,
    // against the shared transaction contract before measuring.
    let verified_forward = propose(&before, &forward).expect("forward transaction");
    assert_eq!(verified_forward.data, after);
    assert_eq!(verified_forward.dirty.objects, dirty.objects);
    assert_eq!(verified_forward.dirty.materials, dirty.materials);
    let verified_reverse = propose(&after, &reverse).expect("reverse transaction");
    assert_eq!(verified_reverse.data, before);
    assert_eq!(verified_reverse.dirty.objects, dirty.objects);
    assert_eq!(verified_reverse.dirty.materials, dirty.materials);

    let baseline_rss = linux_memory_kib("VmRSS:");
    let start_setup = Instant::now();
    let mut consumer: Box<dyn Consumer> = match backend {
        "tde" => Box::new(Tde::new(&before)),
        "bevy" => Box::new(BevyBackend::new(&before)),
        "runen" => Box::new(RunenBackend::new(&before)),
        "recompute" => Box::new(FullRecompute::new(&before)),
        _ => panic!("unknown backend: {backend}"),
    };
    let setup_ns = start_setup.elapsed().as_nanos();
    let after_setup_rss = linux_memory_kib("VmRSS:");

    assert_eq!(consumer.data(), before);
    assert_eq!(consumer.packets(), before.packets());
    consumer.apply(&before, &after, &dirty);
    assert_eq!(consumer.data(), after);
    assert_eq!(consumer.packets(), after.packets());
    consumer.apply(&after, &before, &dirty);
    assert_eq!(consumer.data(), before);
    assert_eq!(consumer.packets(), before.packets());

    let mut source = before.clone();
    for index in 0..20 {
        let (from, to, ops) = if index % 2 == 0 {
            (&before, &after, &forward)
        } else {
            (&after, &before, &reverse)
        };
        if mode == "transaction" {
            apply_transaction(consumer.as_mut(), &mut source, ops);
        } else {
            consumer.apply(from, to, &dirty);
        }
    }

    let started = Instant::now();
    for index in 0..iterations {
        let (from, to, ops) = if index % 2 == 0 {
            (&before, &after, &forward)
        } else {
            (&after, &before, &reverse)
        };
        if mode == "transaction" {
            apply_transaction(consumer.as_mut(), &mut source, ops);
        } else {
            consumer.apply(from, to, &dirty);
        }
    }
    let elapsed_ns = started.elapsed().as_nanos();
    let peak_rss = linux_memory_kib("VmHWM:");

    if mode == "transaction" {
        assert_eq!(source, before, "final admitted source state");
    }
    assert_eq!(consumer.data(), before, "final authoritative source state");
    assert_eq!(
        consumer.packets(),
        before.packets(),
        "final published packets"
    );

    println!(
        "BENCH,{backend},{workload},{size},{iterations},{mode},{setup_ns},{elapsed_ns},{},{},{}",
        memory_field(baseline_rss),
        memory_field(after_setup_rss),
        memory_field(peak_rss)
    );
}

#[cfg(test)]
mod tests {
    use super::{states, transition_edits};
    use crate::contract::propose;

    #[test]
    fn benchmark_transactions_follow_the_shared_contract() {
        for size in [32, 128] {
            for workload in ["noop", "sparse", "dense", "fanout", "reassign"] {
                let (before, after, dirty) = states(size, workload);
                let forward = transition_edits(&before, &after, &dirty);
                let reverse = transition_edits(&after, &before, &dirty);
                let accepted = propose(&before, &forward).expect("forward accepted");
                assert_eq!(accepted.data, after);
                assert_eq!(accepted.dirty.objects, dirty.objects);
                assert_eq!(accepted.dirty.materials, dirty.materials);
                assert_eq!(
                    propose(&after, &reverse).expect("reverse accepted").data,
                    before
                );
            }
        }
    }
}

pub fn diagnostic(args: &[String]) {
    assert_eq!(
        args.len(),
        4,
        "usage: --diagnostic BACKEND WORKLOAD OBJECTS ITERATIONS"
    );
    let backend = args[0].as_str();
    let workload = args[1].as_str();
    let size: u32 = args[2].parse().expect("integer object count");
    let iterations: usize = args[3].parse().expect("integer iterations");
    assert!((32..=100_000).contains(&size), "object count out of range");
    assert!((2..=100_000).contains(&iterations) && iterations.is_multiple_of(2));

    let (before, after, dirty) = states(size, workload);
    match backend {
        "tde" => {
            let mut consumer = Tde::new(&before);
            let mut totals = [0u128; 5];
            for index in 0..iterations {
                let (from, to) = if index % 2 == 0 {
                    (&before, &after)
                } else {
                    (&after, &before)
                };
                let times = consumer.apply_profiled(from, to, &dirty);
                totals[0] += times.snapshot_ns;
                totals[1] += times.dirty_objects_ns;
                totals[2] += times.sources_ns;
                totals[3] += times.affected_ns;
                totals[4] += times.publication_ns;
            }
            assert_eq!(consumer.data(), before);
            assert_eq!(consumer.packets(), before.packets());
            println!(
                "PROFILE,tde,{workload},{size},{iterations},{},{},{},{},{}",
                totals[0], totals[1], totals[2], totals[3], totals[4]
            );
        }
        "runen" => {
            assert_eq!(
                workload, "noop",
                "RunenECS diagnosis is currently no-op only"
            );
            let mut consumer = RunenBackend::new(&before);
            let (resource_ns, schedule_ns) = consumer.profile_noop_schedule(iterations);
            assert_eq!(consumer.data(), before);
            assert_eq!(consumer.packets(), before.packets());
            println!("PROFILE,runen,noop,{size},{iterations},{resource_ns},{schedule_ns}");
        }
        _ => panic!("unknown diagnostic backend: {backend}"),
    }
}
