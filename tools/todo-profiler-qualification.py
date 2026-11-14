#!/usr/bin/env python3
"""Test-only validation of an independent Todo recording, never an Engine exporter."""
import json
import os
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
expected = sys.argv[2]
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
    "cpu_events", "stage_statistics", "render", "unavailable",
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
receipt_keys = {"sequence", "finality", "ownership_revision"}
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
all_render = 0
seen_ids = set()
for path in outputs:
    if path.stat().st_size > 8 * 1024 * 1024:
        raise SystemExit("privacy/capacity HOLD: oversized profiler file")
    raw = path.read_bytes()
    if any(token in raw.lower() for token in (b"bulk 1", b'"x11"', b'"enter"', b"runen lab - todo")):
        raise SystemExit("privacy HOLD: controlled Todo task value or title in raw capture")
    data = json.loads(raw)
    if set(data) != top or data["schema_version"] != 1:
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
                if receipt["finality"] == "committed":
                    committed_receipts += 1
            if event["domain"] == "runenui_m13" and event["stage"] == "canonical_input_pump":
                all_m13 += 1
                if m13["processed_envelopes"] > 0:
                    productive_m13_pumps += 1
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
        "render_observations": len(data["render"]["observations"]),
    })

resize_status = (root / "resize-exit.txt").read_text().strip()
interaction_status = (root / "interaction-exit.txt").read_text().strip()
completed = all(s["status"] == "completed" for s in sessions)
qualified = (
    resize_status == "0" and interaction_status == "0" and completed
    and all_m13 > 0 and productive_m13_pumps > 0
    and committed_receipts > 0 and all_render > 0
    and any(e["cpu_events"] > 0 for e in sessions)
    and any(e["capture"] == "resize.json" and e["status"] == "completed"
            and e["render_observations"] > 0 for e in sessions)
)
summary = {
    "classification": "pass" if qualified else "hold",
    "kind": "runen_lab_independent_todo_native_profiler_candidate",
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
