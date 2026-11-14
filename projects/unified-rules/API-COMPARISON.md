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

## F — crash-window and effect-lifecycle ownership comparison

This is an independently compiled **fault model and counterexample suite**, not
a real transactional database, disk persistence, networking or GPU execution
test. The test's `ModeledAtomicStore` assumes one atomic recoverable
checkpoint for business state and effect intents; it does **not** implement
`fsync`, log replay, multi-process concurrency or a source-of-truth adapter.

| Candidate ownership boundary | Behavior at commit→handoff crash | Behavior at dispatch→ack crash | Tradeoff |
| --- | --- | --- | --- |
| Engine-owned durable outbox | Can retain request **only** if engine state and outbox are atomically durable | Can duplicate delivery | Forces persistence-specific semantics into a potentially volatile game execution core |
| External executor-owned queue, naive postcommit enqueue | **LOST COMMAND**: business state committed but nothing enqueued | Can duplicate delivery | Unsafe without atomic shared commit, durable source log/CDC or recoverable reconciliation |
| State-owner's atomic state+intent adapter plus external executor | Pending request recoverable in the modeled transaction | Retry can duplicate; idempotent sink can suppress by stable key | Separates computation contract from persistence and transport, but requires explicit adapter/ack authority |
| Volatile GPU/renderer request lifetime | In-process progress only; restart recovery may intentionally mean rebuilding resources | Not a database delivery guarantee; explicit terminal status, generations and cancellation needed | Different durability policy behind a coherent typed request/result vocabulary |

The suite uses two different kinds of evidence:
1. **Real Lab engine**: recreating `Engine` loses pending requests and reuses
   numeric `EffectId` values. Never treat these IDs as persistent identity.
2. **Modeled fault boundaries**: staged failure before commit leaves neither
   state nor request; modeled atomic commit survives simulated restart; a
   postcommit external enqueue gap demonstrably loses work; a crash after
   execution but before acknowledgment can duplicate; sink-side stable-key
   deduplication suppresses the *second execution* in the model. A separate
   business-occurrence key deduplicates re-evaluation/ack retries and rejects
   conflicting payload reuse while admitting a genuinely new occurrence.

**Provisional architecture recommendation (NOT acceptance):** Keep the
**deterministic calculation/request staging contract** in the computation
domain, but let the **authoritative state storage adapter** own any required
atomic persistence of state and pending intents. A separately owned effect
executor handles submission and explicit completion/failure/acknowledgment.
Do not force database durability on volatile GPU or frame operations; their
owners require bounded progress, cancellation/obsolescence and terminal
observation instead. A "single developer-facing rule model" can expose typed
requests and result facts without treating externally executing them as
transactional rules.

**Still blocked:** Real storage implementation, recovery/fsync, crash
injection with actual persistence, concurrent claim/lease, ordered
multi-producer delivery, durable ID scheme, I/O side-effect purity and
type-safe completion feedback are unqualified. Both generic API frontends and
full Bevy/RunenECS equivalence/performance gates remain unproven. The F suite
does not establish at-least-once delivery in a real system, and never claims
external exactly-once effects. Retain RULE-03, RULE-06, RULE-07 on **HOLD**.

## G — typed terminal-result intake and cancellation races (real Lab consumer)

The independent `tests/effect_lifecycle_candidate.rs` uses the actual
`Engine::compute::<Each<View>>` and `Engine::transact` interfaces for
typed intent, operation state, current derived view, and staged
`WorkCommand { WorkTicket { Key, revision }, kind }` requests. A separate,
simulated executor drives the boundary; it is **consumer-local glue**, not
a new engine scheduler, shared Lab framework or approved frontend.

The candidate's admission policy:
- **Requested → submitted → terminal**; `CancelRequested` is an
  intermediate state, *not* proof of cancellation. Completion may win a
  cancellation race, or an acknowledged cancellation may win. Cancel rejection
  returns the still-running operation to `Submitted`. Cancelling a queued,
  unsubmitted request needs no fabricated executor command; the test-only
  driver retracts its pending request after admitting `Cancelled`.
- A terminal report must match the pending `EffectId`, its typed payload's
  exact `WorkTicket` and origin, the current live `Key` generation, and
  the current operation revision. Old-generation or obsolete completions
  do not overwrite new state. An unsubmitted obsolete queued request is
  retired without dispatch. Duplicate/late reports cannot overwrite a
  terminal result.
- Accepted terminal reports enter as **ordinary typed input state**
  via `Engine::transact`; therefore dependent `View` is derived using
  the same rule model, and a later failing validation rule rejects the
  entire terminal state transition. The effect remains unacknowledged
  if admission fails.
- `cx.changed::<Intent>` and
  `cx.changed::<CancelRequested>` avoid generating repeat commands when
  unrelated dependencies change. This is a **consumer-authored transition
  discipline**, not a generic kernel guarantee of logical idempotency.

**Critical counterevidence / HOLD:** The executor is an in-memory simulation,
and its `submit`/`report` methods are necessary external orchestration,
not magically unified into one rule. The terminal state transaction
and `acknowledge_effect` are still **two separate steps**; an interruption
between them leaves a terminal fact and a pending request. The test models
that gap and handles repeated observation within the same Engine instance.
There is no real worker, concurrency, durability, persistence, replay across
Engine reconstruction, claim/lease authority or external exactly-once
proof. Nor is cancellation guaranteed to stop physical execution just
because a cancel command was staged. Rules remain impure and serial.

**Critical red/green regression proof:** At red exact head
`c22c131008cdf9171821d2b312d3e2792a1a8d63`, three independently
added tests failed: retry after a rejected cancel generated no new work;
accepted cancellation leaked its Start request; successful Start completion
left an obsolete Cancel request. The corrected consumer adds explicit
`CancelRequested { revision, attempt }` and
`CommandKind::Cancel { attempt }` identity, admitting a changed fact for
each real retry. After an accepted *terminal* outcome the fake executor
retires all in-memory Start/Cancel requests for that precise generational
operation ticket; `CancelRejected` retires only its own request. Previous
tests that expected late sibling reports to return AlreadyTerminal now
correctly expect NotPending after retirement.

**Still unproven:** An accepted cancellation report in this serial simulation
is not evidence that a real external worker stopped, and retirement is not
resource cleanup or a durable/cross-process acknowledgment. Physical work,
concurrent claim fencing, authenticated terminal reports and state+outbox
atomicity require separately owned guarantees; a caller can still manually
write terminal `Execution` input and bypass the fake executor.

**Disposition:** Bounded typed lifecycle admission and race model may be
a useful semantic primitive, but do **not** adopt it as a final engine API.
The frontend still requires manually authored command orchestration and
transaction/admission glue, so RULE-03 / RULE-06 / RULE-07 remain **HOLD**.

## I — end-to-end invoice contract using the existing scoped C frontend

The new `tests/end_to_end_invoice_candidate.rs` composes four
`Engine::compute` rules: per-line checked amount, per-invoice grouped
total, one keyless outstanding balance, and per-invoice overdue state with
event + effect-request staging. The actual keyless `Day` input is validated
monotonically via `clock_in::<One<Day>>`. The suite covers reparenting,
deletion, payment, repeated no-op edits, retained/acknowledged requests,
missing-parent/overflow transaction rejection and a **48-step independent
per-invoice + global reference model**. No SUMMARY or CLOCK entity is created.

| Requirement | Function injection A | Relational B | Scoped `compute` C in new consumer |
| --- | --- | --- | --- |
| Per-line checked calculation | Requires fallible-function extension | Uses raw callback | Native per-key task |
| Cross-invoice relations and grouping | No frontend expression | Typed algebra, full snapshot | Raw `cx.all`, full snapshot |
| Global open balance | Fallback to artificial key/raw rule | Fallback to artificial key/raw rule | True keyless `One<OpenBalance>` |
| Monotonic keyless time | Unsupported in own frontend | Older keyed clock fallback | Scoped clock admission |
| Once-on-transition reminder + effect request | Unsupported | Old raw callback, no effect lifecycle | In-memory `emit` / `request` in same rule |
| Full independent game consumer in this frontend | **Not proven** | **Not proven** | A separate scoped consumer now exists; check its exact-head CI (J) |
| Indexed/incremental joins, borrowing, parallelism | Not proven | Not proven | Not proven |

**Critical distinction:** This is a *complete vertical invoice application
scenario*, **not** a complete frontend comparison. The C test still relies
on `RuleCx` for every domain, performs O(total lines × invoices) repeated
collection scans on affected invoices, and carries explicit input and
outbox management. The original game consumer still uses the older `rule::<T>`
surface; the separate J consumer applies scoped `compute` to the game
workload. Neither A nor B yet supports this full comparison. The 66-line engine configuration span (lines 47–112) excludes
types, fixtures, assertions and external effect execution; do not compare
that number against Bevy source LOC or call it a productivity win.

**Next discriminating gate:** Compare A/B/C against an identical semantic
workload **without raw fallback paths**; a candidate unable to express
required capabilities should receive a clear FAIL, not a fake wrapper
that hides the missing contract. Then compile the existing game consumer
under the same selected scoped interface, qualify failure semantics,
and create equivalent public Bevy/RunenECS consumers before any performance
or developer-ergonomics GO claim. Retain RULE-01/03/04/05/06/07 **HOLD**.

## J — full game consumer through scoped compute (qualification candidate)

`tests/scoped_game_consumer.rs` independently exercises the same
`Engine::compute::<Each<T> / One<T>>` frontend as the invoice test, without
altering the kernel or deleting legacy compatibility tests. The single application
combines keyless monotonic steps, previous-commit movement, recursive hierarchy,
reparenting and generational identity, keyed contact and keyless tick
occurrences, and a global velocity reduction without a fabricated summary key.
A separate 32-round / 24-record arithmetic and acyclic-tree reference challenges
updates, parent redirection, absence of retroactive movement and global sums.
Negative cases cover cycles, orphaned links, stale generations, rejected time,
failed-computation rollback and preservation of the preceding event snapshot.

**This is an experimental source candidate; consult exact-head CI for
its current qualification status**, not source inspection alone. Even a green result establishes scoped frontend *expressiveness* for
two downstream workloads, not frontend simplicity, borrowed data, selective
indexes, concurrent correctness, complete effects, or Bevy/RunenECS superiority.
`Each<T>` currently delegates to the original `rule<T>` evaluator; no
distinct runtime algorithm is claimed. A/B still lack complete own-frontend
coverage. The architecture remains on HOLD.

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


## K — composition probe: one registration, optional functions and operators

The independent `tests/compute_combinators_candidate.rs` compiles one
test-local **fallible** `required2` typed-function combinator and a small
`Relation` algebra against the **unchanged** scoped
`Engine::compute::<Each<T> / One<T>>` registration.

Both an invoice application and a game consumer intentionally mix:
- `compute::<Each<T>>(required2(|a: &A, b: &B| -> Result<T, Error> { ... }))`
  for required per-record derived values, with absence/retraction and
  checked failure;
- the ordinary `compute::<Each<T>>` callback for dynamic parent lookups,
  prior-commit feedback and reminder occurrences;
- `Relation` joins, grouped sums and reductions *inside* the same
  `compute` callback, including a real `One<T>` global result.

The test exercises invoice grouping on relationship redirection, effect
request staging, non-tick hierarchy propagation, dynamic-cycle rollback,
and failed checked arithmetic. It uses no altered kernel, no second
registration method, no artificial global entity, and no new scheduler.

**Discriminating result, not an architecture GO:** This tests whether the
**ergonomic strengths** of function injection and relational operators can
be composed as ordinary Rust helpers under one rule contract. It does
**not** make A or B independently complete alternative frontends; the
dynamic, temporal and effect cases still explicitly use `RuleCx`.
`required2` clones inputs because the underlying `cx.get` does; the
`Relation` helper materializes full snapshots and joins, without indexes,
incrementality or query planning. No numerical speed or Bevy comparison
is implied. Any claims of successful compilation must be tied to exact-head
CI; this source comparison alone is not execution evidence.

**Architecture implication:** Keep the three questions separate:
(1) semantic output identity and transaction ownership, (2) optional
ergonomic function/query operators, (3) the scaling implementation.
Embedding A/B operators as helpers may prevent an unnecessary hybrid public
registration API, but a single *registration name* does not guarantee
one understandable programming model. A/B true independent universality,
ownership/borrowing, workload equivalence and performance remain
**HOLD** pending separately qualified evidence.

## L — same-workload function-return contract discrimination

**Research checkpoint only; frontend selection remains HOLD.** Test source:
[`named_function_contract_candidate.rs`](tests/named_function_contract_candidate.rs).
Exact behavior and compiler evidence at code head
`4d5d628ffb82d48a4027a535f921bac6de5ed654`:
[project qualification](https://github.com/dornglut/runen-lab/actions/runs/43587263065)
(**92/92 Debug and 92/92 Release**, Rustfmt, strict Clippy) and
[canonical validation](https://github.com/dornglut/runen-lab/actions/runs/43587263341),
both successful. The tests run **four** registration contracts against
**identical** game and invoice fixtures, expected state and failure assertions.
They share the *same* underlying evaluator, staged transaction mechanics,
rule callback implementations and test assertions. This is frontend
expressiveness and compatibility evidence, **not** an independent oracle,
new scheduler, public API or runtime comparison with Bevy/RunenECS.

The earlier unconstrained `register(outstanding)` /
`register(nonempty)` produced a real Rust E0283 ambiguity at
[negative compile head `ffb1892`](https://github.com/dornglut/runen-lab/actions/runs/43584365865):
the blanket pure `Fn(...) -> O` adapters also fit
`O = Result<T, Error>` or `O = Option<T>` because those types satisfy
the persistent-output bounds. The four follow-on variants test different
ways to make that distinction explicit.

| Form | Pure functions | Fallible and optional functions | Per-call marker | Required extra declaration |
| --- | --- | --- | --- | --- |
| P1: unconstrained opportunistic injection | `fn(&A) -> T` | Ordinary `Result<T>` / `Option<T>` | **Yes** for ambiguous functions | Signature-shape generics at registration |
| P2: one canonical derived result | `fn(&A) -> Result<Option<T>, Error>` | Same canonical return | No | Wrappers on otherwise pure functions |
| P3: explicit non-`Eq` tagged outcome | `fn(&A) -> T` | `fn(&A) -> TaggedOutcome<T>` | No | Custom result wrapper and conversion |
| P4: explicit produced-type marker | `fn(&A) -> T` | Ordinary `Result<T>` / `Option<T>` | No | Output type must implement `MarkedOutput` |

For the identical **invoice** registration fixture P1 uses **two explicit
signature-shape markers**, P2 adds **three canonical-return adapter functions**,
P3 adds **two tagged-outcome adapter functions**, and P4 uses **ten test-local
produced-type opt-ins** across the complete game-plus-invoice fixture.
These figures are narrowly scoped ceremony counts, **not developer LOC,
total complexity or productivity measurements**. Each form registers five
invoice behaviors and eight game behaviors plus one scoped clock.
Cross-entity navigation, temporal reads, global aggregations, spawning,
staged effects and failure diagnostics still use the same advanced `RuleCx`
behavior or input-admission interface in every variant.

**P4 is the strongest *provisional frontend contract* for stable Rust
ergonomics in this bounded comparison**, because it preserves ordinary
`?` propagation inside fallible `Result<T, Error>` functions while
avoiding signature-shape markers at the call site. In this experiment,
the marker is implemented once for each eligible produced data type.
Whether this can reuse the existing `runen_ecs::Component` (or another
authoritative type classification) **has not been compile-proven**; forcing
a new per-type derive everywhere could erase the ergonomic advantage.
Output-role typing must also avoid accidentally treating `Option<T>` or
`Result<T, Error>` themselves as stored outputs. P4 is **not selected** as
an accepted Runenwerk/RunenECS contract.

P3 resolves inference but sacrifices native `?` inside a function whose
return type is the custom wrapper: implementing the relevant `std::ops::Try`
trait for an arbitrary application type remains
[nightly-only](https://doc.rust-lang.org/stable/std/ops/trait.Try.html).
P2 retains ordinary `?` but requires `Ok(Some(...))` even for simple
pure derivation. P1 retains the plainest function signatures but can require
long signature generics at registration. These are real developer-level
tradeoffs, not equivalent spellings.

### What these tests do not establish

- **No new general semantics.** P1–P4 use the same serial cloned-world,
  feedback-limited kernel. Passing the same assertions cannot prove full
  transactional isolation, durable effects, arbitrary feedback or safe
  in-rule external I/O.
- **No borrowing/parallel proof.** Injected references borrow cloned
  snapshots; the kernel still requires stored state `Clone + Eq + 'static`,
  rebuilds dependency indexes and scans collections.
- **No complete portability qualification.** The marker is test-local;
  accepted public RunenECS source and actual component derive behavior have
  not been used to compile this candidate. Cross-repo lifetime, relation,
  access and scheduling capabilities are unqualified.
- **No competitive advantage established.** There are no complete equivalent
  public-API Bevy and RunenECS consumers with fair matched semantic
  guarantees and rotated Release performance trials.

### Next discriminating gate — avoid further surface-only experiments

1. **Type-authority proof:** inspect and compile against immutable accepted
   RunenECS `Component`/query interfaces to test whether P4's required type
   distinction reuses existing declarations, preserves output ownership and
   avoids new redundant type hierarchies. If this fails, P4 loses its
   provisional ergonomic advantage.
2. **Backend feasibility:** compare C's transaction/invalidations with real
   RunenECS entity, relation, query, mutation journal and deterministic
   serial/parallel contracts, without copying or moving source into Lab.
   Identify whether a thin higher-level semantic layer is feasible at all.
3. **Independent equivalence/performance:** qualify the actual same
   movement/hierarchy/event and invoice applications on immutable Bevy and
   RunenECS public revisions. Measure developer-authored domain and glue
   code separately, exact results/errors and both full transaction and
   propagation-only release times, including dense and sparse workloads.
4. **GO / PIVOT / REJECT:** recommend adoption only if it improves
   developer comprehension and correctness with a plausible scalable
   execution implementation. Otherwise retain the useful isolated
   combinators or reject the universal API ambition.

**Disposition:** complete PR #16, accepted-base→head assurance,
framework adoption and the Engineering #146 decision remain **HOLD**.
No production Runenwerk/RunenECS semantics are changed by these tests.

## M — accepted RunenECS component and query boundary (RULE-04/RULE-07)

**Downstream executable qualification, not an integration or framework
change.** The independent [`runen_ecs_backend_candidate.rs`](tests/runen_ecs_backend_candidate.rs)
uses a **git-revision-pinned accepted RunenECS**
`5d6af86bfa7c58f1ad26ec73635590a72e89ef37` as a dev dependency,
with an actual Cargo-generated committed lockfile. The source is read through
accepted public `runen_ecs::prelude` APIs; no RunenECS source is copied,
no Runenwerk/RunenECS public API is changed, and no backend adapter or
second synced World is introduced.

**Positive compile-executed evidence at temporary lock-generation
revision `f8f4ee7ca2f1672389391b21721809e35fa67194`:**
[project-specific exact-head CI](https://github.com/dornglut/runen-lab/actions/runs/43589584019)
passed **94/94 Debug and 94/94 Release** tests, Rustfmt and strict Clippy.
The final exact-head run must separately prove the restored strict
`--locked` workflow, not rely on that temporary generation step.

- RunenECS stores `Position { x: f32, y: f32 }` and `Velocity { x: f32,
  y: f32 }` as valid derived-macro `Component` types with **neither
  `Clone` nor `Eq`**, including NaN/infinity values. Ordinary
  `Query<(&mut Position, &Velocity)>` mutates in place and
  `query::<(Entity, &Position)>()` borrows without copies. A
  non-`Clone` `Vec<u8>` component also remains accessible.
- Ordinary fixed-step integration with `Res<DeltaTime>` and explicit
  schedule ordering compiles via public `Runtime::add_systems`. A
  `Changed<Position>` observation deliberately counts a mutably accessed
  value as changed even when `dt=0` leaves its float unchanged. This is
  conservative change observation, **not** a numeric-value comparison.
- Despawn and subsequent spawn reject stale entity access; no proof
  depends on a particular slot-reuse sequence or query iteration order.

**Critical source contract mismatch:**
- Accepted [RunenECS `Component`](https://github.com/dornglut/runen-ecs/blob/5d6af86bfa7c58f1ad26ec73635590a72e89ef37/crates/runen-ecs/src/component.rs)
  requires only `'static`; its
  [`#[derive(Component)]`](https://github.com/dornglut/runen-ecs/blob/5d6af86bfa7c58f1ad26ec73635590a72e89ef37/crates/runen-ecs-macros/src/lib.rs)
  generates that marker directly. The Lab C `OutputShape::Value` and
  stored derived-output APIs require `Clone + Eq + 'static`. Thus a
  valid float-valued RunenECS component is *not* automatically a valid C
  derived output. P4's test-only `MarkedOutput` role opt-in does not
  solve this; equating it with `Component` would silently impose stronger
  requirements on existing component types.
- Accepted RunenECS `World::mutation_journal::MutationJournal` is
  **`pub(crate)`**, and its implementation replays component/resource
  **change-observation metadata**; it does **not** stage arbitrary
  component value updates with C's all-or-nothing rollback.
  `BatchCommands::apply` applies commands sequentially and can fail
  mid-application. No public RunenECS API has been shown here to provide
  atomic speculative value publication, rollback of arbitrary mutation,
  or C's derived-event feedback contract.
- RunenECS's borrowed-query and deterministic schedule semantics are
  authoritative for its own World. The separate Lab C engine still owns
  its **own** cloned World and `Key` identity. This test **does not
  connect their entities, queries, source state or output commits**;
  claiming “C already runs over RunenECS” would be false.

**Architecture disposition: PIVOT FEASIBILITY UNDER INVESTIGATION, HOLD
on adoption.** A legitimate C-over-RunenECS design must preserve a
**single** RunenECS-owned entity/storage/liveness authority, define
distinct *source vs derived* output roles and per-output change policy
(including `f32::NAN`, removals and equality vs changed-by-access),
provide safely validated staged publication/rollback, preserve observer
and relation semantics, and avoid two independently mutating schedulers.
There is no sound adapter, storage-journal or performance proof in
this consumer. Any new reusable ECS mutation capability belongs to an
independently authorized RunenECS issue, not to this Lab experiment.

**Next necessary gate:** determine whether existing accepted RunenECS
public capabilities can support the required C transaction contract,
or precisely identify the smallest missing upstream transaction/source
journal boundary. Compile-prove a **single authoritative World** and
floating-point derived output without cloning the entire World; reject
the architecture rather than adding a mirrored World or app-specific
recompute/scheduler. Then run equivalent complete Bevy/RunenECS
applications and sparse/dense Release benchmarks before GO/PIVOT/REJECT.

## N — one-World native execution and fallible-write discriminator (RULE-04/RULE-07)

**Exact immutable source / proof:** [the independent ECS consumer at
`a80b94c4414c5b9438941968796ce08c2c088310`](https://github.com/dornglut/runen-lab/blob/a80b94c4414c5b9438941968796ce08c2c088310/projects/unified-rules/tests/one_world_execution_boundary.rs).
The accepted RunenECS dependency is still
`5d6af86bfa7c58f1ad26ec73635590a72e89ef37`. Its
[exact-head project workflow](https://github.com/dornglut/runen-lab/actions/runs/43599393865)
passed **97/97 Debug and 97/97 Release tests**, Rustfmt, strict Clippy and
locked builds. [Canonical repository validation](https://github.com/dornglut/runen-lab/actions/runs/43599394241)
passed on the same source SHA. The three added tests pass in both profiles.
Earlier intermediate heads failed first Rustfmt and then a rejected tuple query
shape; their failures were corrected, not suppressed.

### What is now executable

1. One actual RunenECS `World`, one ordinary native `Runtime` system, and
   `Query<(&mut ProjectedX, &Position, &Velocity)>` compute float
   `ProjectedX` in place without requiring `Clone` or `Eq`.
   Both ordinary finite output and NaN propagation are observed. This confirms
   a non-`Eq` **native ECS component**, *not a C-produced derived output*.
2. Two eager fallible `World::entity_mut(...).require_mut(...)` updates
   expose the first successful mutation when the second operation returns an
   error. A closed, known two-record write set can instead preflight both
   components before changing either; the rejected case keeps data and the
   change cursor unchanged. Preflight is a bounded positive alternative for
   this failure, **not an all-or-nothing transaction primitive** for arbitrary
   callbacks, allocation, structural mutation or application effects.
3. Typed `Parent` relations use ECS `Entity` directly, with directed
   source-cardinality-one and cycle-forbidden policy. The consumer verifies a
   rejected cycle, preserved prior edges and cleanup on middle-entity despawn.
   This proves native relation semantics, **not reactive descendant
   invalidation or atomic relation-plus-component publication**.

The first compile attempt used
`Query<(&Position, &Velocity, &mut ProjectedX)>`, which does *not* satisfy
accepted RunenECS's `QueryData` tuple implementations. Reordering the
mutable component to the first position compiled and executed. A future
one-function authoring model must account for these type-shape diagnostics,
not mistake Rust tuple permutations for an established uniform API.

### Direct source/API implications

- `runen_ecs::Component: 'static` permits non-`Eq` and non-`Clone`
  values; C's `OutputShape::Value: Clone + Eq + 'static` does not.
  A C output cannot simply be cast or registered as an arbitrary ECS component.
- The Lab C `Engine::transact` stages with `self.world.clone()` and uses
  C-owned `Key` identities. Native `World` borrowing and C's cached graph
  have **not** been joined; the new test deliberately does not create a
  second World or a sync adapter.
- Native RunenECS `BatchCommands::apply` executes queued commands in
  sequence and can error after an earlier command. Its internal
  `MutationJournal` updates mutation/change observation, not arbitrary value
  rollback. Existing individual-operation failure atomicity cannot be
  generalized into a batch transaction by assertion.
- Native `Changed<T>` can report mutable access without numeric change
  (previous consumer with `dt=0`). A derived-value policy must explicitly
  choose observation-by-access, replacement, or a domain-selected equality
  function. Default `PartialEq` for `f32` is not a universal fix: NaN is
  not equal to itself; approximate tolerance may violate transitivity.
  Removal, relation redirection and epoch-sensitive invalidation require
  their own precise admission/observation rules.
- RunenECS remains the liveness, relation, storage and scheduler authority.
  Even if a rule frontend is chosen, an optional reactive evaluator must not
  introduce independent entity IDs or a concurrently authoritative scheduler.

### Architectural comparison after this discriminator

| Option | Direct evidence | Principal cost / missing proof | Disposition |
| --- | --- | --- | --- |
| Replace ECS execution with the C kernel as-is | Lab C semantics are testable, but it owns a separate cloned World and `Key` | Violates one-World requirement; `Clone + Eq`, O(World) staging, duplicate scheduling | **Reject as-is** |
| Directly make every ECS function a transactional reactive rule | Native ECS borrowing works, but no arbitrary all-or-nothing C commit integration has been shown | Dense per-tick cost, source/derived role conflicts, non-`Eq` change policies, feedback safety | **HOLD** |
| Retain RunenECS execution and qualify selected C mechanisms behind one coherent application authoring contract | Native floating-point systems, relations and explicit scheduling pass | Needs one-World incremental observation, bounded validated publication and complete game/invoice developer comparisons | **Preferred PIVOT investigation**, not adoption |
| Keep accepted RunenECS without C | Fully usable native components/queries/scheduler, and no speculative runtime | Sparse derived ergonomics/transactional composition still require explicit code | **Viable fallback** |

### Minimum next decision-proof, not a framework implementation authorization

The owning architecture investigation must specify **which operations genuinely
require atomic publication**, including the boundary of source edits, derived
values, entity births/deaths, relation edits, occurrence dispatch and effect
requests. Discriminate a **closed, prevalidated application batch** from a
**reusable, typed ECS publication capability**; do not presume a generic
transaction engine is necessary. If the latter is essential and unavailable on
accepted public RunenECS, route its smallest contract to an independently
authorized RunenECS issue. Keep identity, ownership, borrowing, scheduling and
liveness exclusively in RunenECS.

Only then compile **one real ECS-owned World** executing representative game
and independent invoice rules with deterministic failure/retraction cases and
no cloned/shadow World. Compare complete public-API Bevy and RunenECS authored
code, then measure optimized **1,024 / 65,536 / 1,000,000** entity cases,
dense and sparse, including full transaction and retained memory. This
checkpoint has **no C-over-ECS adapter, full equivalent comparison, or
performance result**.

Runenwerk already has a RunenNet-backed replication, delivery-acceptance and
prediction integration under `engine/src/plugins/net/`. Chimera Signal's
existing authoritative-server implementation uses Lightyear. These are
separate downstream requirements: gameplay commit, replication projection,
transport acceptance, and prediction/reconciliation must remain distinct.
No Chimera migration or networking rewrite is authorized here.

**Verdict: PIVOT as research hypothesis; HOLD on C architecture acceptance
and Lab PR #16 merge.** This executable evidence strengthens the case for a
RunenECS-native foundation and against copying the C kernel as a replacement.
It does not prove the proposed unified Rule API preferable to Bevy.

