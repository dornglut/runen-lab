# Temporal event-authoring comparison (Lab #23)

Research-only **source-equivalent behavior** comparison, not an accepted framework API.

Each runner uses native Bevy ECS 0.20.1 or pinned accepted RunenECS:
- Bevy buffered `MessageReader`/`MessageWriter` systems, with two independent death readers.
- Bevy `World::trigger` observers, with two independent death observers.
- RunenECS scheduled systems, with an **application-owned** input queue, two independently scheduled death consumers, and explicit queue retirement. RunenECS supplies schedules and ECS storage, **not** this event API or its retention policy.

All variants consume the same manually authored JSON oracle. Two ordered valid hits (4, 6) cross health 10 -> 0 in tick 1; an interleaved stale-entity hit is rejected individually. Tick 2 includes further valid/no-op and invalid hits; tick 3 is empty. Only the first health crossing emits a death occurrence. Both consumers must see the same death record exactly once and must not receive a replay.

**Critical semantic difference:** Bevy's direct `World::trigger(Hit)` invokes Hit observers immediately, but the nested `Commands::trigger(Died)` is deferred until an explicit `World::flush()` after the hit batch. Bevy messages and application-owned RunenECS queues instead deliver through ordered scheduled systems. Matching end-of-step values does **not** establish interchangeable intra-step ordering, event-retention windows, or observer atomicity.

**Integration boundary:** Runenwerk's accepted Engine already owns `FixedStepBegin -> FixedUpdate` and `SimulationTick`; the project tests one explicitly invoked logical tick with the ECS runtimes, **not** Engine's frame accumulator, network rollback, platform delivery, or host scheduler. The native [Arena consumer](https://github.com/dornglut/runenwerk/blob/2198a1f67f7f921daa3e614272cc5f4ab6b1f14f/apps/runenwerk_arena/src/plugin.rs) is source-inspected integration evidence. A separate Engine-App consumer is needed before claiming complete Engine authoring parity.

Run `cargo test --locked` and `cargo run --release --locked` in this project. A passing result is a bounded correctness observation, not a performance or productivity ranking. No shared Lab runtime, production changes, or arbitrary transaction guarantee.
