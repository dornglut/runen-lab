//! Exploratory, propagation-only same-host Release measurements.
//! Admission/fixture parsing, setup, and verification are excluded from the timed loop.
use crate::Consumer;
use crate::bevy_backend::BevyBackend;
use crate::contract::{Data, Dirty, Obj};
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
    assert_ne!(before, after, "workload must have an effect");
    (before, after, dirty)
}

pub fn run(args: &[String]) {
    assert_eq!(
        args.len(),
        4,
        "usage: --bench BACKEND WORKLOAD OBJECTS ITERATIONS"
    );
    let backend = args[0].as_str();
    let workload = args[1].as_str();
    let size: u32 = args[2].parse().expect("integer object count");
    let iterations: usize = args[3].parse().expect("integer iteration count");
    assert!((32..=100_000).contains(&size), "object count out of range");
    assert!(
        (2..=100_000).contains(&iterations) && iterations.is_multiple_of(2),
        "iteration count must be even and bounded"
    );

    let (before, after, dirty) = states(size, workload);
    let baseline_rss = linux_memory_kib("VmRSS:");
    let start_setup = Instant::now();
    let mut consumer: Box<dyn Consumer> = match backend {
        "tde" => Box::new(Tde::new(&before)),
        "bevy" => Box::new(BevyBackend::new(&before)),
        "runen" => Box::new(RunenBackend::new(&before)),
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

    for index in 0..20 {
        if index % 2 == 0 {
            consumer.apply(&before, &after, &dirty);
        } else {
            consumer.apply(&after, &before, &dirty);
        }
    }

    let started = Instant::now();
    for index in 0..iterations {
        if index % 2 == 0 {
            consumer.apply(&before, &after, &dirty);
        } else {
            consumer.apply(&after, &before, &dirty);
        }
    }
    let apply_ns = started.elapsed().as_nanos();
    let peak_rss = linux_memory_kib("VmHWM:");

    assert_eq!(consumer.data(), before, "final authoritative source state");
    assert_eq!(consumer.packets(), before.packets(), "final published packets");

    println!(
        "BENCH,{backend},{workload},{size},{iterations},{setup_ns},{apply_ns},{},{},{}",
        memory_field(baseline_rss),
        memory_field(after_setup_rss),
        memory_field(peak_rss)
    );
}
