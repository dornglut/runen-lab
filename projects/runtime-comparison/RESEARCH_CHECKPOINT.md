# Investigation checkpoint — 2026-11-12

## Established

- Lab issue #9 authorizes a bounded three-way downstream experiment.
- Accepted Lab `main`: `a97965144057dbfe0ac751514b5d4a96d0c6638c`; Lab #7/draft PR #8 at `19d283929de30d7759adee05362a00619be12231` owns the first-project validator writer.
- Accepted RunenECS source inspected at `5d6af86bfa7c58f1ad26ec73635590a72e89ef37`; real `Relations<...>::targets`, `RelationsMut::remove` and schedule API checked in source.
- Bevy 0.20.1 and native custom relationship API checked against published documentation.
- Separate local Rust source has been authored for all three candidates and a deterministic fixture runner. **No Rust compilation/CI has occurred.**
- Independent Python model verified the generated join/index algorithm over all 28,022 fixtures; this is algorithm-level evidence only.

## Risks requiring compiler/test qualification

- Check Bevy relationship hooks, prelude imports and `App::update` execution on the pinned release.
- Check RunenECS removed-component observation ordering and conservative change windows; confirm runtime observes a new `PacketComponent` even when its value equals the default.
- Check macro and resource trait bounds against both published crate versions; correct any compiler feedback on the exact candidate.
- Recheck all expected changed/removed keys and accepted noop-epoch semantics using both fixtures.
- Establish semantically equivalent **timed** admission+publication paths before reporting latency, allocation or developer complexity. This source replay is intentionally not timed.
- Generate and commit the project `Cargo.lock` with Cargo, preserve dependency pinning, and run project checks as part of repository-owned `cargo validate` after the first-project validation boundary is coordinated.

## Disposition

**RESEARCH SOURCE PREPARED / EXECUTION HOLD.** No source has been pushed to GitHub and no CI or benchmark claims are warranted.
