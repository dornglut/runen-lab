//! Independent consumer for Lab #15 / Engineering #146.
//! A simulated executor owns external submission; Engine owns typed state.
//! No I/O, durability, asynchronous concurrency or exactly-once is claimed.
use std::collections::BTreeSet;
use unified_rules::{Each, EffectId, Engine, Error, EventOrigin, Key, One};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Intent {
    revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CancelRequested {
    revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Queued,
    Submitted,
    CancelRequested,
    Completed,
    Failed,
    Cancelled,
}
impl Phase {
    fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Execution {
    revision: u64,
    phase: Phase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct View {
    revision: u64,
    phase: Phase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RejectCompleted(bool);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AdmissionGate;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Noise(u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandKind {
    Start,
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WorkTicket {
    key: Key,
    revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WorkCommand {
    ticket: WorkTicket,
    kind: CommandKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalReport {
    Completed,
    Failed,
    Cancelled,
    CancelRejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Decision {
    Applied,
    Stale,
    AlreadyTerminal,
    AlreadyRequested,
    Rejected,
    NotPending,
}

/// External I/O boundary. These functions intentionally sit OUTSIDE
/// Engine::compute. They are application/test glue, not a second engine
/// scheduler or an accepted generic public frontend.
#[derive(Default)]
struct FakeExecutor {
    in_flight: BTreeSet<EffectId>,
}

fn app() -> Result<Engine, Error> {
    let mut engine = Engine::new();
    engine.compute::<Each<View>>(|cx, key| {
        let Some(intent) = cx.get::<Intent>(key)? else {
            return Ok(None);
        };
        if cx.changed::<Intent>(key)?.is_some() {
            cx.request(WorkCommand {
                ticket: WorkTicket {
                    key,
                    revision: intent.revision,
                },
                kind: CommandKind::Start,
            })?;
        }
        if cx.changed::<CancelRequested>(key)?
            == Some(CancelRequested {
                revision: intent.revision,
            })
        {
            cx.request(WorkCommand {
                ticket: WorkTicket {
                    key,
                    revision: intent.revision,
                },
                kind: CommandKind::Cancel,
            })?;
        }
        // Force a tracked but semantically irrelevant dependency to detect
        // repeated request generation on unrelated invalidation.
        let _noise = cx.get::<Noise>(key)?;
        let current = cx.get::<Execution>(key)?;
        Ok(current.map(|state| View {
            revision: state.revision,
            phase: state.phase,
        }))
    })?;
    engine.compute::<One<AdmissionGate>>(|cx, ()| {
        let reject = cx.singleton::<RejectCompleted>()? == Some(RejectCompleted(true));
        if reject
            && cx
                .all::<Execution>()?
                .iter()
                .any(|(_, state)| state.phase == Phase::Completed)
        {
            return Err(Error::Overflow);
        }
        Ok(Some(AdmissionGate))
    })?;
    Ok(engine)
}

fn create(engine: &mut Engine, key: Key, revision: u64) -> Result<(), Error> {
    engine.transact(|tx| {
        tx.spawn(key)?;
        tx.put(key, Intent { revision })?;
        tx.put(
            key,
            Execution {
                revision,
                phase: Phase::Queued,
            },
        )
    })
}

fn replace(engine: &mut Engine, key: Key, revision: u64) -> Result<(), Error> {
    engine.transact(|tx| {
        tx.put(key, Intent { revision })?;
        tx.put(
            key,
            Execution {
                revision,
                phase: Phase::Queued,
            },
        )?;
        tx.remove::<CancelRequested>(key)
    })
}

fn cancel(engine: &mut Engine, key: Key, revision: u64) -> Result<Decision, Error> {
    let Some(intent) = engine.get::<Intent>(key) else {
        return Ok(Decision::Stale);
    };
    let Some(state) = engine.get::<Execution>(key) else {
        return Ok(Decision::Stale);
    };
    if intent.revision != revision || state.revision != revision {
        return Ok(Decision::Stale);
    }
    if state.phase.terminal() {
        return Ok(Decision::AlreadyTerminal);
    }
    if state.phase == Phase::CancelRequested {
        return Ok(Decision::AlreadyRequested);
    }
    if state.phase == Phase::Queued {
        // No worker has been submitted by this single-threaded test driver.
        // Cancel the queued work without generating a fake cancel request.
        // The terminal state and the outbox acknowledgment are still separate
        // operations; a real concurrent executor needs an atomic claim fence.
        engine.transact(|tx| {
            tx.put(
                key,
                Execution {
                    revision,
                    phase: Phase::Cancelled,
                },
            )
        })?;
        let expected = WorkTicket { key, revision };
        for request in engine.pending_effects::<WorkCommand>() {
            if request.value.ticket == expected && request.value.kind == CommandKind::Start {
                engine.acknowledge_effect(request.id);
            }
        }
        return Ok(Decision::Applied);
    }
    engine.transact(|tx| {
        tx.put(key, CancelRequested { revision })?;
        tx.put(
            key,
            Execution {
                revision,
                phase: Phase::CancelRequested,
            },
        )
    })?;
    Ok(Decision::Applied)
}

fn pending(engine: &Engine, ticket: WorkTicket, kind: CommandKind) -> EffectId {
    engine
        .pending_effects::<WorkCommand>()
        .into_iter()
        .find(|effect| effect.value == WorkCommand { ticket, kind })
        .expect("expected committed typed effect request")
        .id
}

impl FakeExecutor {
    fn submit(&mut self, engine: &mut Engine, id: EffectId) -> Result<Decision, Error> {
        let Some(effect) = engine
            .pending_effects::<WorkCommand>()
            .into_iter()
            .find(|item| item.id == id)
        else {
            return Ok(Decision::NotPending);
        };
        let request = effect.value;
        if effect.origin != EventOrigin::Key(request.ticket.key) {
            return Ok(Decision::Rejected);
        }
        let Some(intent) = engine.get::<Intent>(request.ticket.key) else {
            return Ok(Decision::Stale);
        };
        let Some(state) = engine.get::<Execution>(request.ticket.key) else {
            return Ok(Decision::Stale);
        };
        if intent.revision != request.ticket.revision
            || state.revision != request.ticket.revision
            || state.phase.terminal()
        {
            return Ok(Decision::Stale);
        }
        match request.kind {
            CommandKind::Start if state.phase == Phase::Queued => {
                engine.transact(|tx| {
                    tx.put(
                        request.ticket.key,
                        Execution {
                            revision: request.ticket.revision,
                            phase: Phase::Submitted,
                        },
                    )
                })?;
            }
            CommandKind::Cancel if state.phase == Phase::CancelRequested => {}
            _ => return Ok(Decision::Rejected),
        }
        self.in_flight.insert(id);
        Ok(Decision::Applied)
    }

    fn report(
        &mut self,
        engine: &mut Engine,
        id: EffectId,
        ticket: WorkTicket,
        report: TerminalReport,
    ) -> Result<Decision, Error> {
        if !self.in_flight.contains(&id) {
            return Ok(Decision::NotPending);
        }
        let Some(effect) = engine
            .pending_effects::<WorkCommand>()
            .into_iter()
            .find(|item| item.id == id)
        else {
            self.in_flight.remove(&id);
            return Ok(Decision::NotPending);
        };
        if effect.value.ticket != ticket || effect.origin != EventOrigin::Key(ticket.key) {
            return Ok(Decision::Rejected);
        }
        let kind = effect.value.kind;
        let intent = engine.get::<Intent>(ticket.key);
        let state = engine.get::<Execution>(ticket.key);
        let Some(current) = state else {
            engine.acknowledge_effect(id);
            self.in_flight.remove(&id);
            return Ok(Decision::Stale);
        };
        if intent.map(|v| v.revision) != Some(ticket.revision)
            || current.revision != ticket.revision
        {
            engine.acknowledge_effect(id);
            self.in_flight.remove(&id);
            return Ok(Decision::Stale);
        }
        if current.phase.terminal() {
            engine.acknowledge_effect(id);
            self.in_flight.remove(&id);
            return Ok(Decision::AlreadyTerminal);
        }
        let next = match (kind, report, current.phase) {
            (
                CommandKind::Start,
                TerminalReport::Completed,
                Phase::Submitted | Phase::CancelRequested,
            ) => Phase::Completed,
            (
                CommandKind::Start,
                TerminalReport::Failed,
                Phase::Submitted | Phase::CancelRequested,
            ) => Phase::Failed,
            (CommandKind::Cancel, TerminalReport::Cancelled, Phase::CancelRequested) => {
                Phase::Cancelled
            }
            (CommandKind::Cancel, TerminalReport::CancelRejected, Phase::CancelRequested) => {
                Phase::Submitted
            }
            _ => return Ok(Decision::Rejected),
        };
        // The terminal result is admitted as an ordinary typed input fact.
        // Only afterward do we acknowledge the pending external command.
        // This is NOT atomic across the two calls.
        engine.transact(|tx| {
            tx.put(
                ticket.key,
                Execution {
                    revision: ticket.revision,
                    phase: next,
                },
            )
        })?;
        engine.acknowledge_effect(id);
        self.in_flight.remove(&id);
        Ok(Decision::Applied)
    }
}

const A: Key = Key::new(0, 1);
const B: Key = Key::new(1, 1);

fn ticket(key: Key, revision: u64) -> WorkTicket {
    WorkTicket { key, revision }
}

fn phase(engine: &Engine, key: Key) -> Option<Phase> {
    engine.get::<View>(key).map(|v| v.phase)
}

#[test]
fn completion_failure_and_duplicate_report_are_typed_state() -> Result<(), Error> {
    let mut engine = app()?;
    let mut executor = FakeExecutor::default();
    create(&mut engine, A, 1)?;
    create(&mut engine, B, 3)?;
    let a = pending(&engine, ticket(A, 1), CommandKind::Start);
    let b = pending(&engine, ticket(B, 3), CommandKind::Start);
    assert_eq!(
        executor.report(&mut engine, a, ticket(A, 1), TerminalReport::Completed)?,
        Decision::NotPending
    );
    assert_eq!(executor.submit(&mut engine, a)?, Decision::Applied);
    assert_eq!(executor.submit(&mut engine, b)?, Decision::Applied);
    assert_eq!(phase(&engine, A), Some(Phase::Submitted));
    assert_eq!(
        executor.report(&mut engine, a, ticket(B, 3), TerminalReport::Completed)?,
        Decision::Rejected
    );
    assert_eq!(
        executor.report(&mut engine, a, ticket(A, 1), TerminalReport::Completed)?,
        Decision::Applied
    );
    assert_eq!(phase(&engine, A), Some(Phase::Completed));
    assert_eq!(
        executor.report(&mut engine, a, ticket(A, 1), TerminalReport::Completed)?,
        Decision::NotPending
    );
    assert_eq!(
        executor.report(&mut engine, b, ticket(B, 3), TerminalReport::Failed)?,
        Decision::Applied
    );
    assert_eq!(phase(&engine, B), Some(Phase::Failed));
    assert!(engine.pending_effects::<WorkCommand>().is_empty());
    assert_eq!(
        engine.get::<View>(A),
        Some(View {
            revision: 1,
            phase: Phase::Completed
        })
    );
    Ok(())
}

#[test]
fn cancellation_race_completion_wins_or_cancel_wins() -> Result<(), Error> {
    let mut engine = app()?;
    let mut executor = FakeExecutor::default();
    create(&mut engine, A, 1)?;
    create(&mut engine, B, 1)?;
    let start_a = pending(&engine, ticket(A, 1), CommandKind::Start);
    let start_b = pending(&engine, ticket(B, 1), CommandKind::Start);
    executor.submit(&mut engine, start_a)?;
    executor.submit(&mut engine, start_b)?;
    assert_eq!(cancel(&mut engine, A, 1)?, Decision::Applied);
    assert_eq!(cancel(&mut engine, B, 1)?, Decision::Applied);
    let cancel_a = pending(&engine, ticket(A, 1), CommandKind::Cancel);
    let cancel_b = pending(&engine, ticket(B, 1), CommandKind::Cancel);
    executor.submit(&mut engine, cancel_a)?;
    executor.submit(&mut engine, cancel_b)?;
    assert_eq!(
        executor.report(
            &mut engine,
            start_a,
            ticket(A, 1),
            TerminalReport::Completed
        )?,
        Decision::Applied
    );
    assert_eq!(
        executor.report(
            &mut engine,
            cancel_a,
            ticket(A, 1),
            TerminalReport::Cancelled
        )?,
        Decision::AlreadyTerminal
    );
    assert_eq!(phase(&engine, A), Some(Phase::Completed));
    assert_eq!(
        executor.report(
            &mut engine,
            cancel_b,
            ticket(B, 1),
            TerminalReport::Cancelled
        )?,
        Decision::Applied
    );
    assert_eq!(
        executor.report(
            &mut engine,
            start_b,
            ticket(B, 1),
            TerminalReport::Completed
        )?,
        Decision::AlreadyTerminal
    );
    assert_eq!(phase(&engine, B), Some(Phase::Cancelled));
    assert!(engine.pending_effects::<WorkCommand>().is_empty());
    Ok(())
}

#[test]
fn cancel_rejected_then_original_operation_completes() -> Result<(), Error> {
    let mut engine = app()?;
    let mut executor = FakeExecutor::default();
    create(&mut engine, A, 5)?;
    let id = pending(&engine, ticket(A, 5), CommandKind::Start);
    executor.submit(&mut engine, id)?;
    cancel(&mut engine, A, 5)?;
    let cancel_id = pending(&engine, ticket(A, 5), CommandKind::Cancel);
    executor.submit(&mut engine, cancel_id)?;
    assert_eq!(
        executor.report(
            &mut engine,
            cancel_id,
            ticket(A, 5),
            TerminalReport::CancelRejected
        )?,
        Decision::Applied
    );
    assert_eq!(phase(&engine, A), Some(Phase::Submitted));
    assert_eq!(
        executor.report(&mut engine, id, ticket(A, 5), TerminalReport::Completed)?,
        Decision::Applied
    );
    assert_eq!(phase(&engine, A), Some(Phase::Completed));
    Ok(())
}

#[test]
fn obsolete_revision_and_reused_entity_key_cannot_receive_old_completion() -> Result<(), Error> {
    let mut engine = app()?;
    let mut executor = FakeExecutor::default();
    create(&mut engine, A, 1)?;
    let old = pending(&engine, ticket(A, 1), CommandKind::Start);
    executor.submit(&mut engine, old)?;
    replace(&mut engine, A, 2)?;
    let current = pending(&engine, ticket(A, 2), CommandKind::Start);
    assert_eq!(
        executor.report(&mut engine, old, ticket(A, 1), TerminalReport::Completed)?,
        Decision::Stale
    );
    assert_eq!(phase(&engine, A), Some(Phase::Queued));
    executor.submit(&mut engine, current)?;
    assert_eq!(
        executor.report(&mut engine, current, ticket(A, 2), TerminalReport::Failed)?,
        Decision::Applied
    );
    assert_eq!(phase(&engine, A), Some(Phase::Failed));

    engine.transact(|tx| tx.despawn(A))?;
    let reused = Key::new(A.slot, A.generation + 1);
    create(&mut engine, reused, 1)?;
    let new_id = pending(&engine, ticket(reused, 1), CommandKind::Start);
    assert_ne!(new_id, old);
    assert_eq!(engine.get::<View>(A), None);
    assert_eq!(phase(&engine, reused), Some(Phase::Queued));
    executor.submit(&mut engine, new_id)?;
    assert_eq!(
        executor.report(
            &mut engine,
            new_id,
            ticket(reused, 1),
            TerminalReport::Completed
        )?,
        Decision::Applied
    );
    assert_eq!(phase(&engine, reused), Some(Phase::Completed));
    Ok(())
}

#[test]
fn unrelated_change_does_not_resubmit_unchanged_intent_or_cancel() -> Result<(), Error> {
    let mut engine = app()?;
    let mut executor = FakeExecutor::default();
    create(&mut engine, A, 1)?;
    let start = pending(&engine, ticket(A, 1), CommandKind::Start);
    assert_eq!(executor.submit(&mut engine, start)?, Decision::Applied);
    assert_eq!(engine.pending_effects::<WorkCommand>().len(), 1);
    engine.transact(|tx| tx.put(A, Noise(1)))?;
    assert_eq!(engine.pending_effects::<WorkCommand>().len(), 1);
    assert_eq!(cancel(&mut engine, A, 1)?, Decision::Applied);
    assert_eq!(cancel(&mut engine, A, 1)?, Decision::AlreadyRequested);
    assert_eq!(engine.pending_effects::<WorkCommand>().len(), 2);
    engine.transact(|tx| tx.put(A, Noise(2)))?;
    assert_eq!(engine.pending_effects::<WorkCommand>().len(), 2);
    Ok(())
}

#[test]
fn terminal_admission_failure_keeps_pending_request_and_original_state() -> Result<(), Error> {
    let mut engine = app()?;
    let mut executor = FakeExecutor::default();
    create(&mut engine, A, 1)?;
    let id = pending(&engine, ticket(A, 1), CommandKind::Start);
    executor.submit(&mut engine, id)?;
    engine.transact(|tx| tx.put_singleton(RejectCompleted(true)))?;
    assert_eq!(
        executor.report(&mut engine, id, ticket(A, 1), TerminalReport::Completed),
        Err(Error::Overflow)
    );
    assert_eq!(phase(&engine, A), Some(Phase::Submitted));
    assert_eq!(engine.pending_effects::<WorkCommand>().len(), 1);
    engine.transact(|tx| tx.put_singleton(RejectCompleted(false)))?;
    assert_eq!(
        executor.report(&mut engine, id, ticket(A, 1), TerminalReport::Completed)?,
        Decision::Applied
    );
    assert_eq!(phase(&engine, A), Some(Phase::Completed));
    assert!(engine.pending_effects::<WorkCommand>().is_empty());
    Ok(())
}

#[test]
fn admission_commit_to_ack_gap_is_visible_not_claimed_atomic() -> Result<(), Error> {
    let mut engine = app()?;
    let mut executor = FakeExecutor::default();
    create(&mut engine, A, 8)?;
    let id = pending(&engine, ticket(A, 8), CommandKind::Start);
    executor.submit(&mut engine, id)?;

    // Simulate interruption after committing a typed terminal fact but
    // before engine.acknowledge_effect() removes the outbox entry.
    engine.transact(|tx| {
        tx.put(
            A,
            Execution {
                revision: 8,
                phase: Phase::Completed,
            },
        )
    })?;
    assert_eq!(phase(&engine, A), Some(Phase::Completed));
    assert_eq!(engine.pending_effects::<WorkCommand>().len(), 1);
    assert_eq!(
        executor.report(&mut engine, id, ticket(A, 8), TerminalReport::Completed)?,
        Decision::AlreadyTerminal
    );
    assert!(engine.pending_effects::<WorkCommand>().is_empty());
    Ok(())
}

#[test]
fn cancelling_before_submission_has_no_fictitious_executor_work() -> Result<(), Error> {
    let mut engine = app()?;
    let mut executor = FakeExecutor::default();
    create(&mut engine, A, 1)?;
    let queued = pending(&engine, ticket(A, 1), CommandKind::Start);
    assert_eq!(cancel(&mut engine, A, 1)?, Decision::Applied);
    assert_eq!(phase(&engine, A), Some(Phase::Cancelled));
    assert!(engine.pending_effects::<WorkCommand>().is_empty());
    assert_eq!(executor.submit(&mut engine, queued)?, Decision::NotPending);
    assert_eq!(cancel(&mut engine, A, 1)?, Decision::AlreadyTerminal);
    Ok(())
}

#[test]
fn despawned_generational_key_cannot_receive_a_late_completion() -> Result<(), Error> {
    let mut engine = app()?;
    let mut executor = FakeExecutor::default();
    create(&mut engine, A, 1)?;
    let old = pending(&engine, ticket(A, 1), CommandKind::Start);
    executor.submit(&mut engine, old)?;
    engine.transact(|tx| tx.despawn(A))?;
    let reused = Key::new(A.slot, A.generation + 1);
    create(&mut engine, reused, 1)?;
    let current = pending(&engine, ticket(reused, 1), CommandKind::Start);
    assert_eq!(
        executor.report(&mut engine, old, ticket(A, 1), TerminalReport::Completed)?,
        Decision::Stale
    );
    assert_eq!(phase(&engine, reused), Some(Phase::Queued));
    assert_eq!(engine.pending_effects::<WorkCommand>().len(), 1);
    executor.submit(&mut engine, current)?;
    assert_eq!(
        executor.report(
            &mut engine,
            current,
            ticket(reused, 1),
            TerminalReport::Completed
        )?,
        Decision::Applied
    );
    assert_eq!(phase(&engine, reused), Some(Phase::Completed));
    Ok(())
}
