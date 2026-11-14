# Invoice aggregation and observation (Lab #25)

**Research-only independent downstream application.** Its source inputs reside in one native Bevy App World or one accepted RunenECS World, respectively. Identity maps store only logical-to-native handles. It does not copy framework source, share runtime machinery across Lab projects, or install the experimental C World.

Two native backends consume one explicit, independently hand-authored 12-step fixture. The complete business case includes checked line multiplication, per-invoice grouping, open balance, clock-dependent overdue state, reassignment, deletion, no-op, overflow rejection, invalid invoice, time regression, payment, and downstream result observation.

**Distinct contracts:**

- Direct read rebuilds a current projection from actual native ECS source components/resources on **each read**, without using a publication cache.
- Retained publication saves one computed value snapshot **after each admitted edit**, compares it with the previously published value, and retains it across no-op/rejected edits. The application owns refresh and value-delta calculation. The fixture checks named changed fields and exact refresh counts.
- Application-level `preflight()` clones the small source snapshot to validate a **closed set of typed edits** before native mutation. It does not demonstrate transactional arbitrary ECS writes, rollback of callbacks, or durability. No cross-system concurrency is tested.

The two backends intentionally share a domain Rust `project()` implementation and admission function to isolate the **developer model and native storage boundary** from different arithmetic implementations. The fixture's explicit expected values and deltas are authored separately, but shared projection logic means this is **not** independent correctness proof of two unrelated aggregation algorithms. This is a normal domain-function + ECS alternative, not a generic incremental programming framework.

The Bevy path uses `App::world()` and native components/resources; the RunenECS path uses native `World::require` / `insert` / `spawn` / `despawn`. Neither needs an application scheduler for synchronous read and explicit publication. Do not infer that read-while-writing, subscriber cursors, native change notifications, transactions, entity reuse, performance at scale or rich diagnostics have been established.

The existing [Lab C invoice consumer](https://github.com/dornglut/runen-lab/blob/9f424138e06dd51a1ec89d38b35bd637ad607ff1/projects/unified-rules/tests/end_to_end_invoice_candidate.rs) is a separately compiled, **non-equivalent broader contract** with its own keyless clock, emitted reminders and staged effect requests. No claims about effect/external I/O parity or comparison of raw source LOC follow. No C source is modified.

Run `cargo test --manifest-path projects/invoice-authoring/Cargo.toml --all-targets --locked` and `cargo run --manifest-path projects/invoice-authoring/Cargo.toml --release --locked`. All outcomes require exact-head CI and canonical repository validation before any acceptance claim.
