#!/usr/bin/env python3
"""Test-only validation of an independent Todo recording, never an Engine exporter."""
import json
import os
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
expected = sys.argv[2]
interaction_only = len(sys.argv) == 4 and sys.argv[3] == "--interaction-only"
if len(sys.argv) not in (3, 4) or (len(sys.argv) == 4 and not interaction_only):
    raise SystemExit("invalid qualification invocation")
outputs = sorted(root.glob("todo-*.json")) + sorted(root.glob("resize.json"))
if not re.fullmatch(r"[0-9a-f]{40}", expected):
    raise SystemExit("invalid pinned revision")
if not outputs:
    raise SystemExit("HOLD: ordinary Todo generated no profile files")

# Reject unexpected strings and fields before making any artifact downloadable.
top = {
    "schema_version", "session_id", "kind", "status", "clock",
    "gpu_clock_relation", "process_id", "os", "profile", "source_revision",
    "host_pacing", "native_windows", "event_capacity", "dropped_cpu_events",
    "cpu_events", "stage_statistics", "native_ui_presentations", "render", "unavailable",
}
event_keys = {
    "event_ordinal", "domain", "stage", "start_ns", "duration_ns",
    "host_frame", "window", "ui_slot", "m13",
}
m13_keys = {
    "settlement_status", "pump_outcome", "processed_envelopes",
    "terminal_receipts_total", "sampled_receipts", "omitted_receipts",
    "processed_through", "paused",
}
receipt_keys = {"sequence", "finality", "ownership_revision", "pointer_trace"}
pointer_trace_keys = {
    "sequence", "status", "phase", "displayed_hit_generation",
    "displayed_snapshot", "physical_target_present", "physical_path_depth",
    "matches_previous_down_physical_target", "pointer_default",
    "activation_trace", "trace_evicted_before",
}
pointer_statuses = {
    "unavailable_no_canonical_pointer_trace",
    "unavailable_no_physical_observation",
    "observed_committed_pointer_transaction",
    "physical_observed_without_routed_commit",
}
pointer_phases = {None, "move", "down", "up", "cancel", "wheel", "unknown"}
pointer_snapshots = {None, "current", "retained_historical"}
pointer_defaults = {None, "applied", "suppressed"}
activation_states = {
    "unavailable", "collected", "not_observed_in_retained_trace",
}
native_presentation_keys = {"status", "event_capacity", "dropped_events", "events"}
native_presentation_row_keys = {
    "event_ordinal", "observed_ns", "host_frame", "transition",
    "ui_slot", "native_window", "render_surface", "publication_id",
    "hit_test_generation", "coordinate_revision", "source_ingress_epoch",
}
native_presentation_transitions = {
    "source_staged_for_render",
    "terminal_present_promoted_for_input",
}
stats_keys = {
    "domain", "stage", "window", "ui_slot", "sample_count", "median_ns",
    "p95_ns", "duration_histogram_ns",
}
render_keys = {
    "status", "observations", "omitted_by_export_limit", "evicted",
    "sampled_out", "dropped_correlated", "uncorrelated",
    "host_frame_join_status",
}
render_observation_keys = {
    "render_frame", "render_surface", "prepare_epoch", "target_size_px",
    "acquire_cpu_ms", "prepare_ui_cpu_ms", "preflight_cpu_ms",
    "flow_encode_cpu_ms", "encode_submit_cpu_ms", "composed_gpu_status",
    "composed_gpu_duration_ms", "present_operation_submitted",
    "display_timestamp_status",
}
domains = {"host", "ecs_schedule", "runenui_host", "runenui_m13"}
sessions = []
all_events = 0
all_m13 = 0
committed_receipts = 0
productive_m13_pumps = 0
pointer_physical_observations = 0
pointer_activations_collected = 0
pointer_previous_down_mismatches = 0
pointer_missing_trace = 0
pointer_retained_snapshots = 0
pointer_rejected_input = 0
native_source_staged = 0
native_input_promoted = 0
native_presentation_evictions = 0
native_unpaired_promotions = 0
all_render = 0
seen_ids = set()
for path in outputs:
    if path.stat().st_size > 8 * 1024 * 1024:
        raise SystemExit("privacy/capacity HOLD: oversized profiler file")
    raw = path.read_bytes()
    if (
        re.search(rb"bulk\s+[1-9]", raw.lower())
        or any(token in raw.lower() for token in (b'"x11"', b'"enter"', b"runen lab - todo"))
    ):
        raise SystemExit("privacy HOLD: controlled Todo task value or title in raw capture")
    data = json.loads(raw)
    if set(data) != top or data["schema_version"] != 3:
        raise SystemExit("schema HOLD: unknown capture fields or revision")
    if data["source_revision"] != expected:
        raise SystemExit("provenance HOLD: compiled source revision mismatch")
    session_id = data["session_id"]
    if not isinstance(session_id, str) or not re.fullmatch(r"native-[a-f0-9-]{50,}", session_id):
        raise SystemExit("identity HOLD: invalid session identity")
    if session_id in seen_ids:
        raise SystemExit("identity HOLD: reused native session ID")
    seen_ids.add(session_id)
    events = data["cpu_events"]
    if not isinstance(events, list) or len(events) > 16384:
        raise SystemExit("capacity HOLD: invalid CPU event count")
    previous = 0
    local_activations = 0
    local_physical = 0
    local_rejected = 0
    for event in events:
        if set(event) != event_keys or event["domain"] not in domains:
            raise SystemExit("schema/privacy HOLD: unexpected CPU observation fields")
        ordinal = event["event_ordinal"]
        if not isinstance(ordinal, int) or ordinal <= previous:
            raise SystemExit("ordering HOLD: non-monotonic CPU event completion ordinal")
        previous = ordinal
        m13 = event["m13"]
        if m13 is not None:
            if set(m13) != m13_keys:
                raise SystemExit("schema/privacy HOLD: unexpected M13 fields")
            for receipt in m13["sampled_receipts"]:
                if set(receipt) != receipt_keys:
                    raise SystemExit("schema/privacy HOLD: unexpected M13 receipt fields")
                pointer = receipt["pointer_trace"]
                if not isinstance(pointer, dict) or set(pointer) != pointer_trace_keys:
                    raise SystemExit("schema/privacy HOLD: unexpected canonical pointer fields")
                if (
                    pointer["sequence"] != receipt["sequence"]
                    or pointer["status"] not in pointer_statuses
                    or pointer["phase"] not in pointer_phases
                    or pointer["displayed_snapshot"] not in pointer_snapshots
                    or pointer["pointer_default"] not in pointer_defaults
                    or pointer["activation_trace"] not in activation_states
                ):
                    raise SystemExit("correlation/enum HOLD: invalid source pointer trace")
                for key in ("displayed_hit_generation", "physical_path_depth", "trace_evicted_before"):
                    number = pointer[key]
                    if number is not None and (type(number) is not int or number < 0):
                        raise SystemExit("schema/privacy HOLD: invalid pointer scalar")
                for key in ("physical_target_present", "matches_previous_down_physical_target"):
                    boolean = pointer[key]
                    if boolean is not None and type(boolean) is not bool:
                        raise SystemExit("schema/privacy HOLD: invalid pointer boolean")
                if pointer["displayed_hit_generation"] is not None:
                    pointer_physical_observations += 1
                    local_physical += 1
                if pointer["activation_trace"] == "collected":
                    pointer_activations_collected += 1
                    local_activations += 1
                if pointer["matches_previous_down_physical_target"] is False:
                    pointer_previous_down_mismatches += 1
                if pointer["status"].startswith("unavailable"):
                    pointer_missing_trace += 1
                if pointer["displayed_snapshot"] == "retained_historical":
                    pointer_retained_snapshots += 1
                if receipt["finality"] == "committed":
                    committed_receipts += 1
                elif receipt["finality"] == "processing_rejected":
                    pointer_rejected_input += 1
                    local_rejected += 1
                    if pointer["activation_trace"] == "collected":
                        raise SystemExit("causality HOLD: rejected M13 input claims activation")
                if pointer["activation_trace"] == "collected" and pointer["phase"] != "up":
                    raise SystemExit("causality HOLD: activation without original pointer Up")
            if event["domain"] == "runenui_m13" and event["stage"] == "canonical_input_pump":
                all_m13 += 1
                if m13["processed_envelopes"] > 0:
                    productive_m13_pumps += 1
    presentation = data["native_ui_presentations"]
    if not isinstance(presentation, dict) or set(presentation) != native_presentation_keys:
        raise SystemExit("schema/privacy HOLD: unknown native generation ledger field")
    if (
        presentation["status"] != "source_observed_not_physical_scanout"
        or presentation["event_capacity"] != 1024
        or type(presentation["dropped_events"]) is not int
        or presentation["dropped_events"] < 0
    ):
        raise SystemExit("provenance HOLD: invalid native generation observation limits")
    records = presentation["events"]
    if not isinstance(records, list) or len(records) > 1024:
        raise SystemExit("capacity HOLD: native generation observation overflow")
    native_presentation_evictions += presentation["dropped_events"]
    previous_observation = 0
    staged = {}
    local_staged = 0
    local_promoted = 0
    local_unpaired = 0
    for observation in records:
        if not isinstance(observation, dict) or set(observation) != native_presentation_row_keys:
            raise SystemExit("privacy HOLD: unknown native presentation observation fields")
        for field in (
            "event_ordinal", "observed_ns", "ui_slot", "native_window",
            "render_surface", "publication_id", "hit_test_generation",
            "coordinate_revision",
        ):
            if type(observation[field]) is not int or observation[field] < 0:
                raise SystemExit("schema HOLD: invalid presentation scalar")
        ordinal = observation["event_ordinal"]
        if ordinal <= previous_observation or observation["publication_id"] == 0:
            raise SystemExit("provenance HOLD: nonmonotonic generation observation")
        previous_observation = ordinal
        if observation["host_frame"] is not None and (
            type(observation["host_frame"]) is not int or observation["host_frame"] < 0
        ):
            raise SystemExit("schema HOLD: invalid source Host frame identity")
        if observation["transition"] not in native_presentation_transitions:
            raise SystemExit("schema HOLD: invented publication transition")
        ingress = observation["source_ingress_epoch"]
        key = (
            observation["ui_slot"], observation["native_window"],
            observation["render_surface"], observation["publication_id"],
        )
        if observation["transition"] == "source_staged_for_render":
            if type(ingress) is not int or ingress < 0 or key in staged:
                raise SystemExit("provenance HOLD: duplicate/invalid staged generation")
            staged[key] = observation
            local_staged += 1
        else:
            if ingress is not None:
                raise SystemExit("provenance HOLD: fabricated terminal ingress epoch")
            local_promoted += 1
            old = staged.get(key)
            if old is None:
                local_unpaired += 1  # not evidence of missing stage: ring can evict
            elif (
                old["hit_test_generation"] != observation["hit_test_generation"]
                or old["coordinate_revision"] != observation["coordinate_revision"]
                or old["observed_ns"] > observation["observed_ns"]
            ):
                raise SystemExit("correlation HOLD: terminal source generation mismatch")
    native_source_staged += local_staged
    native_input_promoted += local_promoted
    native_unpaired_promotions += local_unpaired
    if set(data["render"]) != render_keys or any(
        set(observation) != render_observation_keys
        for observation in data["render"]["observations"]
    ):
        raise SystemExit("schema/privacy HOLD: unexpected Render fields")
    for stat in data["stage_statistics"]:
        if set(stat) != stats_keys:
            raise SystemExit("schema/privacy HOLD: unexpected CPU summary fields")
        histogram = stat["duration_histogram_ns"]
        if (set(histogram) != {"upper_bounds_exclusive_ns", "counts"}
                or len(histogram["counts"]) != 11
                or sum(histogram["counts"]) != stat["sample_count"]):
            raise SystemExit("duration HOLD: histogram does not conserve retained samples")
    all_events += len(events)
    all_render += len(data["render"]["observations"])
    sessions.append({
        "capture": path.name,
        "session_id": session_id,
        "status": data["status"],
        "cpu_events": len(events),
        "m13_input_pumps": sum(
            1 for e in events
            if e["domain"] == "runenui_m13" and e["stage"] == "canonical_input_pump"
        ),
        "cpu_evictions": data["dropped_cpu_events"],
        "pointer_physical_observations": local_physical,
        "source_pointer_activations": local_activations,
        "m13_processing_rejected": local_rejected,
        "native_source_staged": local_staged,
        "native_input_promoted": local_promoted,
        "native_presentation_evictions": presentation["dropped_events"],
        "native_presentation_unpaired_promotions": local_unpaired,
        "native_presentation_observations": records,
        "render_observations": len(data["render"]["observations"]),
    })

resize_status = (
    "not_tested_in_this_trial" if interaction_only
    else (root / "resize-exit.txt").read_text().strip()
)
interaction_status = (root / "interaction-exit.txt").read_text().strip()
completed = all(s["status"] == "completed" for s in sessions)
qualified = (
    (interaction_only or resize_status == "0") and interaction_status == "0" and completed
    and all_m13 > 0 and productive_m13_pumps > 0
    and committed_receipts > 0 and pointer_physical_observations > 0
    and pointer_activations_collected > 0 and all_render > 0
    and native_source_staged > 0 and native_input_promoted > 0
    and any(e["cpu_events"] > 0 for e in sessions)
    and (interaction_only or any(
        e["capture"] == "resize.json" and e["status"] == "completed"
        and e["render_observations"] > 0 for e in sessions
    ))
)
summary = {
    "classification": "pass" if qualified else "hold",
    "kind": "runen_lab_independent_todo_native_profiler_candidate",
    "experiment": "controlled_post_reopen_delay" if interaction_only else "unaltered_native_baseline",
    "delay_seconds": 3 if interaction_only else 0,
    "runenwerk_revision": expected,
    "todo_revision": os.environ.get("TODO_SOURCE_REVISION", "unknown"),
    "environment": "ubuntu-24.04_xvfb_lavapipe_release",
    "benchmark_qualification": False,
    "window_input_script_exit": interaction_status,
    "window_resize_script_exit": resize_status,
    "cpu_events": all_events,
    "original_m13_pumps": all_m13,
    "productive_m13_pumps": productive_m13_pumps,
    "resize_capture_present": (root / "resize.json").is_file(),
    "committed_m13_receipts_sampled": committed_receipts,
    "source_pointer_physical_observations": pointer_physical_observations,
    "source_pointer_activations_collected": pointer_activations_collected,
    "source_pointer_previous_down_mismatches": pointer_previous_down_mismatches,
    "source_pointer_unavailable": pointer_missing_trace,
    "source_pointer_retained_historical_snapshots": pointer_retained_snapshots,
    "m13_processing_rejected_receipts": pointer_rejected_input,
    "source_native_presentation_staged": native_source_staged,
    "source_native_input_generations_promoted": native_input_promoted,
    "source_native_presentation_evictions": native_presentation_evictions,
    "source_native_presentation_unpaired_promotions": native_unpaired_promotions,
    "render_observations": all_render,
    "captures": sessions,
    "unproven": [
        "physical_windows_hardware", "profiler_observer_overhead",
        "host_render_frame_join", "physical_display_scanout",
        "statistically_meaningful_performance_baseline",
    ],
}
(root / "validation.json").write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
print("todo_profiler_integration=" + summary["classification"])
print("captures=%d cpu_events=%d m13_pumps=%d committed_receipts=%d render_frames=%d" % (
    len(sessions), all_events, all_m13, committed_receipts, all_render
))
