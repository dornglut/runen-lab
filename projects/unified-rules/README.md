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
- tick-driven next-position computation and explicit **epoch-boundary** promotion to source position;
- a dynamic parent/child world-position hierarchy with reparenting, cycle rejection and generational identity reuse;
- ephemeral collision/contact-start **presence transitions**, exposed as newly inserted derived values for one successful transaction; and
- a fourth unrelated health-to-death rule to challenge application-domain coupling.

The application defines only types and ordinary rule bodies; no domain-specific reverse index or scheduler exists. The independent consumer also checks dynamic dependency replacement, atomic rollback on derived overflow, and 128 rounds of changes against a separately calculated 128-node hierarchy reference. Actual 3D rotation/scale, geometric collision detection, GPU/IO effects, persistent event queues and multiplayer are **not** included.

## Semantic contract and limitations

- Per-key types are `Clone + Eq + 'static`. A rule's output type has one producer. Rules must register **before spawning entities**. Values are cloned on read; borrowed components, non-`'static` closures, non-`Eq` types and concurrent execution are not proven.
- Rules can read other derived types via `ctx.get::<T>(key)`; dynamic read sets include absent lookups and `exists` checks. Recursive derived reads use per-key cycle detection and fail the whole staged transaction.
- An in-memory single-writer transaction stages source edits, invalidates affected cached tasks **transitively** through previously observed dependencies, resolves derived dependencies on demand, and publishes coherent values on success. Failed edits or rule evaluation leave the previous world visible.
- A no-op transaction performs no rule evaluations; insertion events are reset per successful transaction.
- Source state does not automatically advance through a self-referential same-epoch rule. `promote::<Next, Current>(...)` is an explicit feedback step. This is currently a **second operation** distinct from rule registration; whether that undermines the proposed single programming model remains an open research finding.
- `inserted::<T>()` reports transitions from a missing derived output to a present derived value **in the latest successful transaction only**. It is not a durable event log, an arbitrary change event, or exactly-once delivery to external systems.
- The engine clones **the entire typed world** on every attempted transaction and reconstructs **the entire reverse dependency graph** after each successful one. Rule evaluations can be sparse, but the whole transaction is **not** O(changes). This is intentionally a semantics/API proof, **not** a million-entity performance implementation.
- Source and output state share the same internal erased column store, with output writes denied to consumers. A later production design must assess stronger separation, lifetime/ownership constraints, cycle diagnostics and temporal rules.
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

1. Real source-independent admission/behavior oracles and negative compile cases; distinguish static declared dependencies from runtime-tracked reads.
2. More demanding dynamic dependency shifts, same-key reentrant rule errors, registration order, deletion, temporal edge cases and events with payload changes; test multiple independent consuming crates.
3. Full 3D transforms, geometric collision generation, user-defined custom operations, non-`Eq`/borrowed values, external effects, parallelism and cancellation.
4. Replace whole-world cloning/full dependency reindexing, then **fair** 1,024/65,536/1,000,000-entity Bevy/RunenECS comparisons using equivalent semantics.

**Decision:** this artifact can prove a first shared Rust rule API, but cannot qualify its scalability or suitability as a complete general-purpose game engine. Keep the draft PR on HOLD until those gates are resolved.
