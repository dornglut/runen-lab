# Derived-state observation contract — Runen Lab #20

This independent downstream project qualifies the current-state read versus retained-publication distinction for [Engineering #148](https://github.com/dornglut/engineering/issues/148).

## Source provenance

The application contract and native ECS backends are copied and adapted from the **draft, unmerged** [Lab #9 / PR #12 at 8f5dda0](https://github.com/dornglut/runen-lab/tree/8f5dda0350d86e548829f35e48be77cedd1c1e92/projects/runtime-comparison). These are **downstream candidate application files**, not copied framework source or accepted public framework semantics.

Dependencies are Bevy ECS/App **0.20.1** and [RunenECS at accepted exact SHA 5d6af86](https://github.com/dornglut/runen-ecs/tree/5d6af86bfa7c58f1ad26ec73635590a72e89ef37), independently locked. The sample uses the first **300 sequential cases** (including the original header) from Lab #9's independently generated 3,013-case oracle. It preserves original expected digests, accepted epochs, changed/removed identities, invalid edits, fanout and generation reuse.

## Contracts qualified

**A — current-state read:** separate native Bevy and RunenECS source-only Worlds store objects, material components and relationship edges but **no derived PacketComponent, Published resource, publisher system or derived cache**. Two independent queries are checked after *every* edit attempt; repeated reads repeat computation. Retained implementations also expose an independent computed read as a cross-check, but their existing caches are not used as the source-only application.

**B — retained publication:** the existing Bevy and RunenECS systems maintain a Published resource. A deliberately straightforward full-recompute publisher separately retains output once per accepted batch and calculates value-difference deltas; rejected edits do not invoke it. No general incremental model is implemented.

**C — admission/failure:** Lab #9's shared propose() validates and clones the input logical state before invoking any backend. Its all-or-nothing rejection is **not** an intrinsic Bevy/RunenECS transaction. External snapshot differencing by this harness is **not** a native ECS change/event API.

The same independently generated oracle checks exact source and packet digests, accepted epochs, failures, changed/removed identities, and both native current-state reads and retained outputs.

## Reproduce

From the project directory:

- cargo fmt --all -- --check
- cargo test --all-targets --locked
- cargo clippy --all-targets --locked -- -D warnings
- cargo run --release --locked

An exact-head GitHub workflow performs these checks. Repository-level cargo validate is a **separate** acceptance gate.

## Limits and decision

This is a **semantic/read-contract comparison**, not an end-to-end developer-productivity trial or a runtime speed/memory comparison. It does not prove built-in subscriptions, derived dependency tracking, native transaction admission, non-Clone payload support, rollback, external effects or a replacement for Bevy/RunenECS.

A computed read is sufficient when consumers only need the current value and can tolerate per-read work. A retained output with changed/removed notification requires explicit ownership of storage and observation. No-cache reads should not be called equivalent to publication unless a producer/consumer protocol supplies that missing behavior.

Issue: https://github.com/dornglut/runen-lab/issues/20
