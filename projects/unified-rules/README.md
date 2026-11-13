# Unified typed-rule game-domain qualification — Lab #15

Experimental code for [Runen Lab #15](https://github.com/dornglut/runen-lab/issues/15) under the architectural investigation [Engineering #146](https://github.com/dornglut/engineering/issues/146). This **does not define** accepted Runenwerk or RunenECS APIs, a general-purpose Bevy alternative, or "TTD" as a ratified design.

## What is actually being tested

A single, typed per-key rule contract in a standalone Rust project:

```rust
engine.rule::<Derived>(|ctx, key| {
    let input = ctx.get::<Input>(key)?;
    Ok(input.map(|value| Derived::from(value)))
})?;
```

The **same rule registration, resolver, typed store, dynamic dependency tracing, and invalidation machinery** are used in `tests/consumer.rs` to run:
- ordinary next-position derivation and **rule-owned, tick-driven Position state** using committed prior values, without an application `promote()` operation;
- a dynamic parent/child world-position hierarchy with reparenting, cycle rejection and generational identity reuse;
- ephemeral collision/contact-start presence transitions **and typed added/updated/removed output deltas**, including recurring per-step contact occurrences; and
- a fourth unrelated health-to-death rule to challenge application-domain coupling.

The application defines only types and ordinary rule bodies; no domain-specific reverse index or scheduler exists. Temporal rules read prior committed outputs through `ctx.previous::<T>(key)` and the typed clock through `ctx.get::<Step>(CLOCK)`; an ordinary source transaction updates that clock. There is **no imperative copy/promotion call** in the temporal consumer. The independent consumer also checks dynamic dependency replacement, atomic rollback on derived overflow, and 128 rounds of changes against a separately calculated 128-node hierarchy reference. Actual 3D rotation/scale, geometric collision detection, GPU/IO effects, persistent event queues and multiplayer are **not** included.

## Semantic contract and limitations

- Per-key types are `Clone + Eq + 'static`. A rule's output type has one producer. Rules must register **before spawning entities**. Values are cloned on read; borrowed components, non-`'static` closures, non-`Eq` types and concurrent execution are not proven.
- Rules can read other derived types via `ctx.get::<T>(key)`; dynamic read sets include absent lookups and `exists` checks. Recursive derived reads use per-key cycle detection and fail the whole staged transaction.
- An in-memory single-writer transaction stages source edits, invalidates affected cached tasks **transitively** through previously observed dependencies, resolves derived dependencies on demand, and publishes coherent values on success. Failed edits or rule evaluation leave the previous world visible.
- A no-op transaction performs no rule evaluations; typed output deltas are reset per successful transaction.
- A rule may read its own **previous committed** output via `ctx.previous::<T>(key)` without creating a same-transaction recursive dependency. An updated typed `Step { epoch, dt }` input triggers recomputation and a rule-selected next state. There is **no public `promote` method**; the consumer still needs manual epoch-change guards and a typed seed input. Epoch monotonicity and fixed-clock authority are **not enforced by the generic engine**, so temporal ergonomics and correctness beyond the tested contract remain open.
- `inserted::<T>()` reports missing-to-present output transitions. `changes::<T>()` reports one `OutputChange::{Added, Updated, Removed}` per key in the **latest successful transaction only**. A contact occurrence can change its epoch payload at successive steps; this is not a durable event log, does not allow multiple same-key occurrences in one commit, and does not provide exactly-once delivery or external effect acknowledgments.
- The engine clones **the entire typed world** on every attempted transaction and reconstructs **the entire reverse dependency graph** after each successful one. Rule evaluations can be sparse, but the whole transaction is **not** O(changes). This is intentionally a semantics/API proof, **not** a million-entity performance implementation.
- Source and output state share the same internal erased column store, with output writes denied to consumers. `previous` reads do not record a current-epoch invalidation edge. A later production design must assess stronger separation, lifetime/ownership constraints, deterministic temporal clock admission, and cycle diagnostics.
- Event "contact" source is supplied by the consumer; geometric broadphase/narrowphase and actual effects are not modeled.

## Reproduce

```sh
cd projects/unified-rules
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --release --all-targets --locked
```

The project CI workflow runs these steps at the exact PR head; the repository's canonical `cargo validate` is a **separate** gate and does not include this project.

## Required subsequent gates

1. Real source-independent admission/behavior oracles, negative compile cases and temporal clock monotonicity proof; distinguish static declared dependencies from runtime-tracked reads.
2. Deep/stack-safe dynamic hierarchies, runtime control of time/feedback without guards in every rule, aggregation and multi-event occurrence semantics; test multiple independent consuming crates.
3. Full 3D transforms, geometric collision generation, user-defined custom operations, non-`Eq`/borrowed values, external effects, parallelism and cancellation.
4. Replace whole-world cloning/full dependency reindexing, then **fair** 1,024/65,536/1,000,000-entity Bevy/RunenECS comparisons using equivalent semantics.

**Decision:** one generic rule registration path now supports both pure derivation and temporal feedback, but feedback requires explicit `previous`/epoch checks inside authored closures. That design is compile-proved for narrow cases, **not yet accepted as an ergonomic universal rule API**. Scalability and general-purpose engine fitness remain unqualified. Keep the draft PR on HOLD until those gates are resolved.
