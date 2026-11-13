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

The application defines only types and ordinary rule bodies; no domain-specific reverse index or scheduler exists. Temporal rules read prior committed outputs through `ctx.previous::<T>(key)` and the registered typed clock through `ctx.advanced::<Step>(CLOCK)`; an ordinary source transaction updates that clock. There is **no imperative copy/promotion call** in the temporal consumer. The independent consumer also checks dynamic dependency replacement, atomic rollback on derived overflow, and 128 rounds of changes against a separately calculated 128-node hierarchy reference. Actual 3D rotation/scale, geometric collision detection, GPU/IO effects, persistent event queues and multiplayer are **not** included.

## Semantic contract and limitations

- Per-key types are `Clone + Eq + 'static`. A rule's output type has one producer. Rules must register **before spawning entities**. Values are cloned on read; borrowed components, non-`'static` closures, non-`Eq` types and concurrent execution are not proven.
- Rules can read other derived types via `ctx.get::<T>(key)`; dynamic read sets include absent lookups and `exists` checks. Recursive derived reads use per-key cycle detection and fail the whole staged transaction.
- A trial `ctx.all::<T>()` reads a **typed collection snapshot**, ordered by generational key. The dependency graph now distinguishes a typed collection from an individual typed fact (no fake sentinel key). Source writes/removals and derived-task invalidations propagate collection invalidation, including membership changes on key creation/deletion. It scans and clones all live-key values, and a change to **one member** re-evaluates **every aggregate** observing that type; neither a selective index nor a scalable query plan is proven.
- An in-memory single-writer transaction stages source edits, invalidates affected cached tasks **transitively** through previously observed dependencies, resolves derived dependencies on demand, and publishes coherent values on success. Failed edits or rule evaluation leave the previous world visible.
- A no-op transaction performs no rule evaluations; typed output deltas are reset per successful transaction.
- A rule may read its own **previous committed** output via `ctx.previous::<T>(key)` without creating a same-transaction recursive dependency. A clock is registered once through `engine.clock::<Step>(CLOCK, |s| s.epoch)`. Before any rule runs, admission rejects zero initial epochs, repeat-epoch value changes, regressing epochs and removal of an initialized clock; an identical clock write is a no-op. `ctx.advanced::<Step>(CLOCK)` provides only a newly admitted step and tracks its dependency. There is **no public `promote` method** and no authored epoch comparison. Initialization remains explicit in the rule; clocks are intentionally persistent for this bounded in-memory proof. The clock registration is an input contract, not a second system-execution API.
- `inserted::<T>()` reports missing-to-present output transitions. `changes::<T>()` still reports **one cached output delta per key** in the latest successful transaction. Separately, the same `rule::<T>` callback can stage multiple typed `ctx.emit(...)` occurrences per key on clock advancement or contact activation; `events::<T>()` presents them in deterministic producer-rule/key/emission order, numbered per source and event type within that snapshot **only after a successful commit**. Ordinals are not durable event IDs. Failure preserves the preceding committed event snapshot, and the next successful transaction replaces it. These are volatile in-memory observations, **not** a durable event stream or exactly-once GPU/I/O effect delivery.
- The engine clones **the entire typed world** on every attempted transaction and reconstructs **the entire reverse dependency graph** after each successful one. Rule evaluations can be sparse, but the whole transaction is **not** O(changes). This is intentionally a semantics/API proof, **not** a million-entity performance implementation.
- Source and output state share the same internal erased column store, with output writes denied to consumers. `previous` reads do not record a current-epoch invalidation edge. A later production design must assess stronger separation, lifetime/ownership constraints, deterministic temporal clock admission, and cycle diagnostics.
- Event "contact" source is supplied by the consumer; geometric broadphase/narrowphase and actual effects are not modeled.

## API alternatives under qualification

| Candidate | Developer interface | Result / trade-off |
| --- | --- | --- |
| Application-side `engine.keys()` + `ctx.get::<T>(key)` loop | No runtime addition | **Rejected for reactive aggregation:** it cannot track future typed members and would require application-owned invalidation/scheduling. |
| `ctx.all::<T>()` inside existing `engine.rule::<Out>(...)` | One rule API; typed value+membership observation | **Selected for a bounded executable proof:** no query DSL or domain-specific index; deliberately O(all keys) per read and broad collection invalidation. |
| General tuple-query builder or procedural macro | Compact advanced queries | **Deferred:** join/filter semantics, access validation, borrowing and index ownership require separate evidence; syntax alone does not establish correctness. |

Example of the experimental collection contract:

```rust
engine.rule::<TotalHealth>(|ctx, key| {
    if key != ROOT { return Ok(None); }
    let total = ctx.all::<Health>()?.into_iter().try_fold(0_i64, |acc, (_, hp)| {
        acc.checked_add(hp.0).ok_or(Error::Overflow)
    })?;
    Ok(Some(TotalHealth(total)))
})?;
```

This API direction is not a ratified public contract. It demonstrates a correctness-first collection read for independent consumers, not a replacement for a production query/aggregation system.

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

1. Independent admission/temporal oracles, negative compile cases, multi-clock and invalid/overflow/failure fault injection; distinguish source clock admission, dynamic read tracking and externally owned fixed-step progression.
2. Deep/stack-safe dynamic hierarchies, **selective and indexed** aggregation/many-to-many rules, and independent crates; test more than the collection-wide scan and broad invalidation candidate. Event delivery retention, effects and acknowledgment remain separate open contracts.
3. Full 3D transforms, geometric collision generation, user-defined custom operations, non-`Eq`/borrowed values, external effects, parallelism and cancellation.
4. Replace whole-world cloning/full dependency reindexing, then **fair** 1,024/65,536/1,000,000-entity Bevy/RunenECS comparisons using equivalent semantics.

**Decision:** one generic rule registration path supports derivation, previous-state feedback and staged in-memory occurrences. Clock monotonicity is checked before evaluation, while user-owned initialization and external event delivery still require an architectural decision. This remains a narrow experiment, **not an accepted universal rule API**. Scalability and general-purpose engine fitness remain unqualified. Keep the draft PR on HOLD until those gates are resolved.
