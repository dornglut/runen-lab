# API candidate comparison — executable checkpoint (Engineering #146 / Lab #15)

This document is **research evidence, not framework authority**. The accepted
public Runenwerk/RunenECS APIs are unchanged. All candidates live under
`projects/unified-rules/`; no new shared Lab framework or dependency exists.

## Comparison contract

Evaluate the developer-facing expression of a common domain-neutral scenario:
an invoice line contributes checked arithmetic, lines join by record identity,
invoice totals group by a relationship, a global outstanding balance reduces
unpaid invoices, a monotonic clock advances overdue status, a transition emits
one reminder, edits/deletions update derived state, and overflow rejects the
transaction without publishing partial data.

Also retain the **same** already-qualified movement/hierarchy/contact consumer
(`tests/consumer.rs`) as a mandatory acceptance cross-check; a candidate that
cannot express those cases in its *own surface* does not pass a complete
unified-programming-model gate.

## Implemented contrasting frontends

**A — ordinary typed function injection** (`tests/function_api_candidate.rs`):
```rust
application.derive(|price: &UnitPrice, qty: &Quantity| {
    LineSubtotal(price.0 * qty.0)
})?;
```
No manual query, scheduler, or `RuleCx` for infallible same-record
1–3-input derivations. Tests cover invoices, document counts and form
validation, source retraction, and rejected writes. **Hard limitation**:
global invoice total needs an artificial summary record and raw `ctx.all`
callback; cross-record relations, time and occurrences remain unsupported by
this frontend.

**B — typed relational algebra** (`tests/dataflow_candidate.rs`):
```rust
let joined = Relation::from_rows(cx.all::<Line>()?)
    .join(Relation::from_rows(cx.all::<LineTotal>()?));
let totals = joined.group_sum(|_, (line, _)| line.invoice,
                               |(_, total)| total.0)?;
```
The operators `join`, `group_sum`, `sum_by` and `filter` are
typed, generic and independently testable. The invoice consumer exercises
membership redirection, retractions, overflow rejection, monotonic clock,
ordered in-memory occurrences and no-op behavior.

**Important falsification:** Candidate B is **not** incremental dataflow:
it builds whole snapshots from `ctx.all`, performs full joins/grouping and
then relies on the existing callback for temporal and occurrence semantics.
The global outstanding output still needs `SUMMARY: Key`; runtime
collection invalidation remains broad. Candidate B therefore **fails the
proposed universal programming-model API target** despite semantic correctness.
Calling these operators 'dataflow' does not make the computation incremental.

## Decision table — no winner established

| Requirement | Function injection (A) | Typed relation (B) |
| --- | --- | --- |
| Required per-record computation | Concise ordinary Rust | Works, more machinery |
| Dynamic collection membership | Not expressible with its signature | Query algebra in old callback |
| Cross-record join and grouped reduction | Not expressible with its signature | Expressible; whole-snapshot cost |
| Global/unkeyed output | Fake key plus old callback | Fake key plus old callback |
| Temporal feedback and multiple occurrences | Falls back to old callback | Falls back to old callback |
| Borrowed store / non-Eq / parallelism | Unproven; cloned values | Unproven; cloned values |
| Same fully coherent frontend for all workloads | FAIL | FAIL |
| Bevy-level functionality/performance superiority | Not established | Not established |

No fair equivalent Bevy and RunenECS complete consumers and benchmark suite
exist for this **exact** invoice/clock/transaction contract. Prior Lab #9
join benchmarks do not measure either of these frontends. Numeric lines of
code or microbenchmark rankings must not be invented.

## C — typed computation scope (new kernel-level counterexample)

Candidate C is **not** another wrapper around a user-visible summary record.
It changes the experimental engine itself: a task/fact now has an explicit
`Each<Key>` or `One` coordinate, and a keyless value has its **own storage
and dependency identity**, distinct from a typed collection membership fact.
A producer is addressed by **(Rust output type, output scope)**. Two
computations may therefore produce `Temp` independently: one per record,
one global; similarly, source values of the same type coexist in both shapes.

The one experimental registration form is:

```rust
app.compute::<Each<LineTotal>>(|ctx, key| {
    // ... derive a typed value for this key ...
})?;
app.compute::<One<InvoiceTotal>>(|ctx, ()| {
    // ... aggregate cx.all::<LineTotal>()? ...
})?;
```

The independently compiled `tests/scope_identity_candidate.rs` verifies
collection→global→keyed dependencies, global→global dependencies, absence,
retraction, deletion, read-only derived values, rollback on arithmetic
overflow, cycles, no-op transactions and type identity not aliasing across
scopes. An independently maintained map is the oracle for **96 rounds of
keyed value edits, membership removal and generational identity reuse**;
the two global results must match that reference after each mutation.

**What it solves:** no fabricated user-level entity key is required for a
global derived output; global state and per-record state really are
different execution coordinates in the kernel. Existing 18 game/temporal/
hierarchy consumers and earlier A/B tests remain unchanged.

**What it does NOT solve:** the rule still exposes `RuleCx`; the developer
must explicitly choose `Each<T>` or `One<T>`. Collection aggregation
still scans/clones all values, staged transactions clone the whole world
and rebuild the reverse graph, and the runtime is serial. At the earlier
**C checkpoint**, a keyless rule could not emit an occurrence without
inventing a record origin; the later **D experiment** introduces an explicit
global event origin and a keyless clock. Effects and acknowledgments remain
unresolved. No performance, borrowed-data, concurrency or fully unified
programming-model claim follows.

**Compiler evidence:** the first method name `Engine::derive` shadowed
candidate A's trait-provided `derive`, breaking six existing call sites.
It was changed to `Engine::compute` and all existing consumers passed.
This is a real Rust method-resolution ergonomics concern, not a runtime
algorithm proof.

**Disposition:** scoped computation identity is **PROMISING AS A RESEARCH
PRIMITIVE**, but candidate C is not an accepted frontend or a universal API.
D adds narrower keyless clock and occurrence proofs; RULE-07 remains **HOLD**
until a complete, consistent time/effects model and fair Bevy comparison are
compiled and benchmarked.

## D — scoped time and in-memory occurrence origin

The next bounded experiment retains one `compute::<Each<T>|One<T>>`
registration and adds scoped clock *input admission* (keyed or keyless) and
an explicit typed occurrence origin (`Key(Key)` or `Global`). No fake
clock entity exists for keyless time. A global rule can emit occurrences
without claiming to originate at an entity, and an event type can appear
from both keyed and keyless producers in one deterministic commit snapshot.

The independently compiled consumer proves clock admission and invalid-step
rejection for both scopes, isolation of two clocks with the same Rust type,
keyless clock-driven movement and events, duplicate same-event emissions,
registration freeze after the first successful commit (including keyless-only
and empty commits),
failed transaction preserving the prior event snapshot, successful no-op
clearing it, unregistered clock rejection, and a 64-epoch/24-object independent
arithmetic oracle.

**Limit, not GO:** `Engine::occurrences` is commit-local observation, not an
effect queue with acknowledgment or retry. The older `Engine::events`
remains keyed-only for compatibility; the final developer contract must
choose one consistent event interface instead of retaining two. Clock input
registration still has separate metadata from rules, tasks do not carry
explicit epoch coordinates, callbacks remain impure, and effects cannot be
made exactly-once by the transactional calculation model alone. No fair
Bevy/RunenECS benchmark or full API selection follows from these tests.

## E — retained post-commit effect requests, NOT execution or durability

`RuleCx::request(T)` stages a typed request during evaluation; the runtime
does **not** execute it. The request is assigned a unique **process-local**
`EffectId` only after all rules succeed, and retained in a memory-only
outbox across later commits. A consumer can read `pending_effects::<T>()`,
execute I/O outside the transaction and explicitly call
`acknowledge_effect(id)`. A rejected transaction stages no new committed
work and consumes no IDs. Acknowledging twice returns false on the second
attempt, while deleting a source entity does not silently discard work.

**Critical falsification:** rules remain ordinary `Fn` closures and can do
uncontrolled I/O themselves. The negative oracle intentionally mutates
`Rc<Cell<u32>>` in a rule, then rejects the transaction later: the
observable external counter has already changed. The engine cannot claim
pure callbacks, transactionally isolated I/O, or external exactly-once
execution. This is a **request-protocol experiment**, not a reliability
or concurrency implementation.

**Additional counterexample:** A rule that requests work while a predicate
is true creates another identical logical request when any other tracked input
invalidates it—even though the business predicate stayed true. IDs differ,
but that does not establish transition-only emission or external idempotency.

**Remaining delivery gap:** IDs and pending entries are lost at process
restart; there is no stable cross-run identity, persisted queue, transport
or retries/leases. The consumer might submit a command and crash before
the acknowledgment, so duplicate external operations are possible.
The no-op `Engine::acknowledge_effect` status flag has no transaction
or durable correlation semantics, and attempts/cancellation still need a
separate authority decision. One coherent rule *computation* registration
is retained, but authoring commands remains an explicit state/effect
distinction. This distinction is necessary, not automatically a hybrid
scheduler API.

## Mandatory next API architecture gate

Candidate C establishes **keyed versus keyless computation identity** in
one evaluator; D adds scoped monotonic clocks and commit-local occurrences;
E proves memory-only retained requests and explicit in-memory acknowledgments.
None establishes collection-valued outputs, temporal coordinates, guaranteed
callback purity or crash-safe effects through one complete API. Investigate these remaining dimensions **without introducing
multiple application-facing programming paradigms**. Compile-execute both function-first and relational
frontends across the same non-game and movement/hierarchy/event consumers,
including explicit external-effect semantics. Test rustc
inference/diagnostics and complete consumer expressiveness before adding
macros or redesigning the public framework.

Retain findings RULE-01 (full clone/reindex), RULE-02 (time authority),
RULE-03 (event/effects), RULE-04 (generality/borrow), RULE-05 (stack safety),
RULE-06 (purity/isolation), and RULE-07 (frontend/output scope). **HOLD**
the architecture and PR merge gate.
