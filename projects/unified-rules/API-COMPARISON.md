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
and rebuild the reverse graph, and the runtime is serial. A keyless rule
cannot emit an occurrence because the old event identity requires a
producing entity; the engine explicitly rejects this case rather than
fabricating an origin. Time still uses the prior keyed clock contract, and
effect requests and acknowledgments are unresolved. No performance,
borrowed-data, concurrency or fully unified programming-model claim follows.

**Compiler evidence:** the first method name `Engine::derive` shadowed
candidate A's trait-provided `derive`, breaking six existing call sites.
It was changed to `Engine::compute` and all existing consumers passed.
This is a real Rust method-resolution ergonomics concern, not a runtime
algorithm proof.

**Disposition:** scoped computation identity is **PROMISING AS A RESEARCH
PRIMITIVE**, but candidate C is not an accepted frontend or a universal API.
RULE-07 remains **HOLD** until time, events and effects can fit one coherent
contract and a fair Bevy comparison is compiled and benchmarked.

## Mandatory next API architecture gate

Candidate C now establishes **keyed versus keyless computation identity** in
one experimental evaluator; it does not establish collection-valued outputs,
time coordinates, occurrences or effect requests in the same API. The next
research question is whether a typed computation identity can include these
remaining dimensions **without introducing multiple application-facing
programming paradigms**. Compile-execute both function-first and relational
frontends across the same non-game and movement/hierarchy/event consumers,
including explicit external-effect semantics. Test rustc
inference/diagnostics and complete consumer expressiveness before adding
macros or redesigning the public framework.

Retain findings RULE-01 (full clone/reindex), RULE-02 (time authority),
RULE-03 (event/effects), RULE-04 (generality/borrow), RULE-05 (stack safety),
RULE-06 (purity/isolation), and RULE-07 (frontend/output scope). **HOLD**
the architecture and PR merge gate.
