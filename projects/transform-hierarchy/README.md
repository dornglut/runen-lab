# Game transform-hierarchy API qualification — Lab #13

This project is an **independent downstream game-domain experiment** in [Runen Lab #13](https://github.com/dornglut/runen-lab/issues/13). It is not a Runenwerk engine API, general TDE engine, ECS substitute, release-conformance suite, or performance ranking. Its source lives only in this independent Cargo project.

## Contract under test

The API presents typed generational `NodeId`, local `Translation {x,y,z}`, optional parent links, and read-only derived world translations. Ordinary consumers supply **batches of typed operations** with `scene.transact(...)` and read `scene.world(node_id)`; they do not maintain reverse indexes or descendant invalidation.

```rust
use transform_hierarchy_qualification::{Edit, NodeId, Scene, Translation};

let mut scene = Scene::new();
let player = NodeId::new(0, 1);
let weapon = NodeId::new(1, 1);
scene.transact(&[
    Edit::Spawn { id: player, parent: None, local: Translation::new(10, 0, 0) },
    Edit::Spawn { id: weapon, parent: Some(player), local: Translation::new(2, 0, 0) },
])?;
assert_eq!(scene.world(weapon), Some(Translation::new(12, 0, 0)));
scene.transact(&[Edit::Move { id: player, local: Translation::new(20, 0, 0) }])?;
assert_eq!(scene.world(weapon), Some(Translation::new(22, 0, 0)));
# Ok::<(), transform_hierarchy_qualification::Error>(())
```

This is a **concrete Rust API in the project**, not proposed builder/macro syntax. It uses *translation only*; rotation, scale, transforms in more than one coordinate space, rendering and GPU resources have not been qualified.

## Semantics

- A live node's `NodeId` is a slot + generation; deleting it retains the last generation for stale-reference rejection and later reuse with incremented generation.
- Each batch targets a given slot at most once. Spawn, move, reparent and remove are supported; a batch may reparent a child and remove its previous parent atomically.
- A transaction is rejected without changing the visible `Scene` when a referenced parent is missing/stale, a cycle is introduced, the target is stale, a slot generation is invalid, two edits target the same slot, or a world translation overflows signed `i64`.
- Deleting a node with surviving children requires the children to be reparented or deleted **in the same batch**.
- This is an exclusively **single-writer, in-memory** API. No durable ACID claim, concurrent reader isolation, lock-free reads, or rollback of external side effects is made.

The first pass deliberately uses **whole-source staging and independent whole-world validation per transaction**, so the transaction path is **O(N)** even for sparse changes. The incremental stage keeps a reverse parent→children index and recomputes only affected descendants, using iterative traversal to avoid deep recursion. It is not yet designed or qualified for one million nodes.

## What is independently checked

Tests compare the incremental world cache with an independently structured topological full-recomputation oracle and use **explicit expected outputs** for player/weapon/camera changes. Coverage includes parent move, reparent, orphan deletion rejection, combined reparent+delete, cycles, duplicate targets, stale generation, identity reuse, overflow, no-op, randomized valid/invalid edits, 4,096-sibling fanout, and 4,096-node chain depth.

The deterministic replay **shares the admission/staging function** between the candidate and its oracle, so it does not independently validate every admission rule. The explicitly expected acceptance/error cases narrow that gap. Neither these tests nor compilation establish a benchmark ranking.

Run locally with the project directory:

```sh
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --release --all-targets --locked
```

A separate exact-feature-head workflow qualifies this project while repository-owned `cargo validate` remains a distinct required gate. The independent Todo PR #8 owns a separate validation integration; this investigation must not overwrite it.

## Subsequent gates

1. Independent Bevy ECS and accepted pinned RunenECS public-API consumers for **the same transform and edit semantics**.
2. A genuinely competitive admission path without whole-world cloning, then shared-host 1,024 / 65,536 / 1,000,000-node experiments measuring setup, propagation, whole transaction, peak/retained memory, and failures.
3. General rotations/scales and concrete game consumers before claiming a real 3D transform API.
4. Critical review of public API ergonomics, stable ownership/borrowing, field/component modeling, parallel reads, and GPU resource lifecycle. Route reusable framework semantics to the owning repository.

Related fixed-join experiment: [Lab #9](https://github.com/dornglut/runen-lab/issues/9).
