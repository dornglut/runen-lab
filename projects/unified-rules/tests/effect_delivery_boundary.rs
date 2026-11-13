//! Research-only fault model for Engineering #146 / Lab #15.
//! This DOES NOT use a real disk, queue, transactional database or network.
//! "Durable" below means a simulated recoverable snapshot, not proven fsync.
use std::collections::{BTreeMap, BTreeSet};
use unified_rules::{Engine, Error, One};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct IntentId {
    operation: u64,
    occurrence: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Command {
    id: IntentId,
    payload: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Snapshot {
    business: BTreeMap<u64, u32>,
    // Keep successful intent identities even after acknowledgment, so a
    // repeated evaluation cannot silently requeue an acknowledged action.
    known: BTreeMap<IntentId, Command>,
    pending: BTreeMap<IntentId, Command>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ModelError {
    ConflictingIntent,
    RejectedTransaction,
}

/// Pure test abstraction of a storage adapter providing one atomic commit
/// for the domain state AND the request outbox. It assumes atomic persistence;
/// it intentionally does not pretend to implement or test real persistence.
#[derive(Default)]
struct ModeledAtomicStore {
    checkpoint: Snapshot,
}

impl ModeledAtomicStore {
    fn restore(checkpoint: Snapshot) -> Self {
        Self { checkpoint }
    }

    fn transact(
        &mut self,
        f: impl FnOnce(&mut Snapshot) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
        let mut staged = self.checkpoint.clone();
        f(&mut staged)?;
        self.checkpoint = staged;
        Ok(())
    }

    fn acknowledge(&mut self, id: IntentId) -> bool {
        self.checkpoint.pending.remove(&id).is_some()
    }
}

impl Snapshot {
    fn stage(&mut self, request: Command) -> Result<(), ModelError> {
        match self.known.get(&request.id) {
            Some(existing) if existing == &request => return Ok(()),
            Some(_) => return Err(ModelError::ConflictingIntent),
            None => {}
        }
        self.known.insert(request.id, request);
        self.pending.insert(request.id, request);
        Ok(())
    }
}

#[derive(Default)]
struct MockExternalSink {
    // This simulates separately retained sink-side identity information.
    seen: BTreeSet<IntentId>,
    physically_executed: Vec<Command>,
    attempts: usize,
}

impl MockExternalSink {
    fn deliver_unprotected(&mut self, request: Command) {
        self.attempts += 1;
        self.physically_executed.push(request);
    }

    fn deliver_idempotent(&mut self, request: Command) {
        self.attempts += 1;
        if self.seen.insert(request.id) {
            self.physically_executed.push(request);
        }
    }
}

const FIRST: IntentId = IntentId {
    operation: 7,
    occurrence: 1,
};

fn command(id: IntentId, payload: u32) -> Command {
    Command { id, payload }
}

#[test]
fn uncoordinated_external_handoff_loses_work_in_commit_to_enqueue_gap() {
    let mut committed_business = BTreeMap::new();
    let external_queue: Vec<Command> = Vec::new();

    // Strategy B: state commit succeeds, then the process dies before its
    // separate "send to executor queue" step. No durable handoff record.
    committed_business.insert(7_u64, 20_u32);
    assert_eq!(committed_business.get(&7), Some(&20));
    assert!(external_queue.is_empty());

    // No amount of executor-side polling can recover a command that was
    // never recorded, unless a replayable source log/CDC is provided.
}

#[test]
fn modeled_atomic_state_and_outbox_cover_crash_before_and_after_commit() -> Result<(), ModelError> {
    let mut store = ModeledAtomicStore::default();

    // Crash before atomic commit: staging is abandoned as a unit.
    let mut abandoned = store.checkpoint.clone();
    abandoned.business.insert(7, 20);
    abandoned.stage(command(FIRST, 20))?;
    drop(abandoned);
    assert!(store.checkpoint.business.is_empty());
    assert!(store.checkpoint.pending.is_empty());

    // Accepted commit publishes state and request together.
    store.transact(|next| {
        next.business.insert(7, 20);
        next.stage(command(FIRST, 20))
    })?;
    let snapshot = store.checkpoint.clone();

    // Restart after commit, before external dispatch: the pending request
    // survives because it shares the modeled atomic checkpoint.
    let mut recovered = ModeledAtomicStore::restore(snapshot);
    assert_eq!(recovered.checkpoint.business.get(&7), Some(&20));
    assert_eq!(recovered.checkpoint.pending.get(&FIRST), Some(&command(FIRST, 20)));

    // A rejected transaction cannot publish either domain state or intent.
    let before = recovered.checkpoint.clone();
    assert_eq!(
        recovered.transact(|next| {
            next.business.insert(7, 99);
            next.stage(command(IntentId { operation: 7, occurrence: 2 }, 99))?;
            Err(ModelError::RejectedTransaction)
        }),
        Err(ModelError::RejectedTransaction)
    );
    assert_eq!(recovered.checkpoint, before);
    Ok(())
}

#[test]
fn dispatch_before_ack_can_repeat_without_sink_idempotency() -> Result<(), ModelError> {
    let mut store = ModeledAtomicStore::default();
    store.transact(|next| next.stage(command(FIRST, 20)))?;
    let saved = store.checkpoint.clone();
    let request = *saved.pending.get(&FIRST).expect("committed request");

    let mut unsafe_sink = MockExternalSink::default();
    unsafe_sink.deliver_unprotected(request);
    // Crash occurs after external execution but before persistent ack.
    let recovered = ModeledAtomicStore::restore(saved.clone());
    let replay = *recovered.checkpoint.pending.get(&FIRST).expect("still pending");
    unsafe_sink.deliver_unprotected(replay);
    assert_eq!(unsafe_sink.attempts, 2);
    assert_eq!(unsafe_sink.physically_executed.len(), 2);

    // A separately idempotent external sink can suppress the duplicate;
    // this requires sink-side stable identity, not just an engine counter.
    let mut protected_sink = MockExternalSink::default();
    protected_sink.deliver_idempotent(request);
    protected_sink.deliver_idempotent(replay);
    assert_eq!(protected_sink.attempts, 2);
    assert_eq!(protected_sink.physically_executed.len(), 1);

    let mut after_ack = ModeledAtomicStore::restore(saved);
    assert!(after_ack.acknowledge(FIRST));
    let restored = ModeledAtomicStore::restore(after_ack.checkpoint);
    assert!(restored.checkpoint.pending.is_empty());
    assert!(restored.checkpoint.known.contains_key(&FIRST));
    Ok(())
}

#[test]
fn stable_business_occurrence_keys_dedupe_retries_but_preserve_new_occurrences() -> Result<(), ModelError> {
    let mut store = ModeledAtomicStore::default();
    store.transact(|next| next.stage(command(FIRST, 20)))?;

    // Unrelated invalidation or retry requests the same logical occurrence.
    store.transact(|next| next.stage(command(FIRST, 20)))?;
    assert_eq!(store.checkpoint.pending.len(), 1);
    assert!(store.acknowledge(FIRST));

    // Acknowledged actions must not be repeated under the same key.
    store.transact(|next| next.stage(command(FIRST, 20)))?;
    assert!(store.checkpoint.pending.is_empty());

    // Reusing an ID for different work is rejected, not silently dropped.
    let before = store.checkpoint.clone();
    assert_eq!(
        store.transact(|next| {
            next.business.insert(7, 99);
            next.stage(command(FIRST, 99))
        }),
        Err(ModelError::ConflictingIntent)
    );
    assert_eq!(store.checkpoint, before);

    // A genuinely later business occurrence needs a different identity.
    let later = IntentId { operation: 7, occurrence: 2 };
    store.transact(|next| next.stage(command(later, 20)))?;
    assert_eq!(store.checkpoint.pending.len(), 1);
    assert_eq!(store.checkpoint.pending.get(&later), Some(&command(later, 20)));
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Source(bool);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Producer;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Email(u32);

fn actual_lab_engine() -> Result<Engine, Error> {
    let mut app = Engine::new();
    app.compute::<One<Producer>>(|cx, ()| {
        if cx.singleton::<Source>()? == Some(Source(true)) {
            cx.request(Email(42))?;
        }
        Ok(Some(Producer))
    })?;
    Ok(app)
}

#[test]
fn real_lab_engine_uses_volatile_request_ids_and_loses_work_on_recreation() -> Result<(), Error> {
    let mut first = actual_lab_engine()?;
    first.transact(|tx| tx.put_singleton(Source(true)))?;
    let requests = first.pending_effects::<Email>();
    assert_eq!(requests.len(), 1);
    let first_id = requests[0].id;
    drop(first);

    // This is a real kernel API counterexample, not the model above:
    // Engine::new() cannot reconstruct the pending outbox or the prior state.
    let mut recreated = actual_lab_engine()?;
    assert!(recreated.pending_effects::<Email>().is_empty());
    assert_eq!(recreated.singleton::<Source>(), None);

    // A new process-local sequence may reuse an earlier numeric EffectId.
    recreated.transact(|tx| tx.put_singleton(Source(true)))?;
    assert_eq!(recreated.pending_effects::<Email>()[0].id, first_id);
    Ok(())
}
