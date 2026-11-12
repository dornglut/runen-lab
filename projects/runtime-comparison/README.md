# Runen Lab #9 — Rust three-way semantic replay (uncompiled research candidate)

This **research-only source snapshot** compares a restricted generated Typed Differential Execution (TDE) engine with the **real public APIs** of Bevy 0.20.1 and RunenECS at immutable revision `5d6af86bfa7c58f1ad26ec73635590a72e89ef37`.

**Status: source-authored, NOT Cargo-compiled or executed. No GitHub branch/PR or CI execution exists for this source.** Do not infer compile success, benchmark results, or ECS superiority.

## What is included

- `src/contract.rs`: identical staged transaction admission, error categories, generation reuse, atomic rejection, authoritative packet and source SHA-256 formats.
- `src/bevy_backend.rs`: Bevy `App`, ECS components, native custom relationships/reverse links, change-detection filters, scheduled packet maintenance and publication.
- `src/runen_backend.rs`: RunenECS `World` and `Runtime`, components, typed directed relationships, change-filtered systems and removed-component publication.
- `tools/compile_rule.py` + `tools/rule.json` + `src/generated.rs`: deterministic **fixed-schema code generation** for the single `render_packets` foreign-key join, reverse-index maintenance, and affected-key discovery. Other schemas are rejected.
- `src/tde.rs`: local TDE candidate using generated rule functions and a keyed packet cache.
- `src/main.rs`: independent three-way replay harness, comparing actual backend source state, published values, transaction errors, epochs, changed/removed keys, and both SHA-256 digests with **28,022** independent oracle expectations.
- `fixtures/*.jsonl`: exact original and adversarial oracle fixtures, copied byte-for-byte from prior independent research.
- `tools/oracle_reference.py` and `tools/check_join_algorithm.py`: a reproducible Python oracle and an independent algorithm-level check of keyed dependency updates across the fixtures.

The input oracle is a **correctness reference only**. It stages updates by cloning logical state, which would dominate a performance comparison. All three backends then apply the identical plan. This avoids timing incomparable admission semantics but does **not** establish framework performance.

## To execute in a properly checked-out Rust environment

Prerequisites: Rust 1.93 or newer as required by RunenECS, a network-enabled dependency resolver, and the accepted versions shown in `Cargo.toml`.

1. Generate the per-project lockfile using Cargo **in a checked-out executor**: `cargo generate-lockfile`. Do not hand-author or fabricate a lockfile.
2. Run `python3 tools/compile_rule.py --check` and `python3 tools/check_join_algorithm.py`; then `cargo fmt --all -- --check` (format source where needed).
3. Run `cargo test --all-targets --locked` followed by `cargo clippy --all-targets --locked -- -D warnings`.
4. Run `cargo run --locked --release` only after exact dependency locking and source review; this command **verifies semantics, not speed**.
5. Record exact source/head SHA, toolchain, dependency lockfile, output, and any corrections before claiming executable proof.

**Not yet done:** compile, format, Cargo.lock resolution, GitHub exact-head CI, and same-host repeated release benchmarks. This candidate must not be published as complete Lab acceptance or as a benchmark against Bevy/RunenECS.

## Important boundaries and findings

- The 25,010-case adversarial fixture includes `radius = -2^40` and values outside signed 32-bit bounds. The Rust transaction **input** deliberately uses `i64` and rejects invalid accepted-state values during staging. Parsing these inputs as `i32` would incorrectly fail before the declared validation contract.
- Bevy and RunenECS are allowed their native relationship/reverse-index mechanisms, actual scheduler, and change filters; they are not forced into full-world scans.
- Publication is checked for **value differences**, not merely conservative `Changed<T>` flags.
- Generated TDE code covers only one explicitly recognized keyed join and dependency maintenance. The transaction processor, numerical packet calculation, and runtime remain manually implemented. This is **not** an arbitrary Rust differential compiler.
- Snapshot retention, bounded memory, parallel differential execution, async effects, and a no-copy mutation path are not provided by this trial.
- No Lab main or draft Todo PR #8 paths have been changed. That PR still owns the first-project validator in `tools/xtask/src/main.rs`. Coordinate before introducing the trial as a maintained project.

## Local evidence (only what actually ran)

- Original fixture independently re-verified in Python: **3,012** rows, exact equality.
- Extended fixture independently re-verified in Python: **25,010** rows, exact equality.
- Restricted generated-rule freshness check: **PASS**.
- Python model of the generated foreign-key/index algorithm: **28,022** cases matched the oracle exactly. This **does not execute the Rust implementation**.
- Rust compiler, Rust test suite, Rust Clippy, exact-head CI: **NOT RUN**.

These fixture checks verify the research **inputs**, not the behavior of the new Rust backends.
