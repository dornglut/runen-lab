#!/usr/bin/env python3
"""Three-scale, same-host per-process Rust benchmark with raw failure accounting."""
import argparse
import csv
import statistics
import subprocess
import sys
from pathlib import Path

BACKENDS = ("tde", "bevy", "runen", "recompute", "compact", "adaptive")
WORKLOADS = ("noop", "sparse", "dense", "fanout", "reassign")
MODES = ("propagation", "transaction")
SCALES = (1024, 65536, 1000000)
FIELDS = (
    "round", "order", "mode", "backend", "workload", "objects",
    "iterations", "status", "setup_ns", "elapsed_ns",
    "baseline_rss_kib", "after_setup_rss_kib", "peak_rss_kib", "detail",
)


def iterations_for(objects, workload, backend):
    if objects >= 1_000_000:
        if workload in ("noop", "sparse"):
            return {
                "tde": 512, "compact": 512, "adaptive": 512,
                "bevy": 32, "runen": 8, "recompute": 4,
            }[backend]
        return 4
    if objects >= 65_536:
        if workload in ("noop", "sparse"):
            return {
                "tde": 512, "compact": 512, "adaptive": 512,
                "bevy": 128, "runen": 16, "recompute": 16,
            }[backend]
        return 8
    return {
        "noop": 512, "sparse": 512, "dense": 40,
        "fanout": 40, "reassign": 160,
    }[workload]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--objects", type=int, nargs="+", default=list(SCALES))
    parser.add_argument("--timeout", type=int, default=240)
    parser.add_argument("--allow-incomplete", action="store_true")
    args = parser.parse_args()
    if not 3 <= args.rounds <= 12:
        parser.error("rounds must be between 3 and 12")
    if not args.objects or any(n not in SCALES for n in args.objects):
        parser.error(f"object counts must be among {SCALES}")
    binary = args.binary.resolve(strict=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    rows = []
    failures = 0

    with args.output.open("w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=FIELDS)
        writer.writeheader()
        for objects in args.objects:
            for mode in MODES:
                for workload in WORKLOADS:
                    for repetition in range(args.rounds):
                        offset = repetition % len(BACKENDS)
                        order = BACKENDS[offset:] + BACKENDS[:offset]
                        for ordinal, backend in enumerate(order):
                            iterations = iterations_for(objects, workload, backend)
                            row = {
                                "round": repetition, "order": ordinal,
                                "mode": mode, "backend": backend,
                                "workload": workload, "objects": objects,
                                "iterations": iterations,
                            }
                            command = (
                                str(binary), "--bench", backend, workload,
                                str(objects), str(iterations), mode,
                            )
                            try:
                                result = subprocess.run(
                                    command, check=False, text=True,
                                    capture_output=True, timeout=args.timeout,
                                )
                                if result.returncode != 0:
                                    raise RuntimeError(
                                        f"exit={result.returncode}: {result.stderr[-400:]}"
                                    )
                                records = [
                                    line for line in result.stdout.splitlines()
                                    if line.startswith("BENCH,")
                                ]
                                if len(records) != 1:
                                    raise RuntimeError("expected exactly one BENCH line")
                                values = records[0].split(",")
                                if len(values) != 11 or values[1:6] != [
                                    backend, workload, str(objects), str(iterations), mode
                                ]:
                                    raise RuntimeError(f"unexpected BENCH output: {records[0]}")
                                row.update(dict(zip(
                                    FIELDS[8:13], values[6:], strict=True,
                                )))
                                row["status"] = "ok"
                            except (subprocess.TimeoutExpired, RuntimeError) as error:
                                failures += 1
                                row["status"] = (
                                    "timeout" if isinstance(error, subprocess.TimeoutExpired)
                                    else "failed"
                                )
                                row["detail"] = str(error)[-400:]
                            writer.writerow(row)
                            stream.flush()
                            rows.append(row)

    print(f"Raw results: {args.output}; failed/timeout samples: {failures}")
    print("Median microseconds/update, from successful samples only:")
    for objects in args.objects:
        for mode in MODES:
            print(f"OBJECTS={objects} MODE={mode}")
            for workload in WORKLOADS:
                results = []
                for backend in BACKENDS:
                    subset = [
                        row for row in rows
                        if row["objects"] == objects and row["mode"] == mode
                        and row["workload"] == workload and row["backend"] == backend
                        and row["status"] == "ok"
                    ]
                    times = [
                        int(row["elapsed_ns"]) / int(row["iterations"]) / 1000
                        for row in subset
                    ]
                    median = f"{statistics.median(times):.3f}" if times else "MISSING"
                    results.append(f"{backend}={median}[{len(times)}/{args.rounds}]")
                print(f"  {workload:9} " + " ".join(results))
    if failures and not args.allow_incomplete:
        sys.exit(f"{failures} failed samples; inspect raw CSV")


if __name__ == "__main__":
    main()
