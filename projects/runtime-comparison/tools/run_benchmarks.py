#!/usr/bin/env python3
"""Alternating per-process Release evidence, not a general performance ranking."""
import argparse
import csv
import statistics
import subprocess
from pathlib import Path

BACKENDS = ("tde", "bevy", "runen", "recompute")
MODES = ("propagation", "transaction")
WORKLOADS = (
    ("noop", 512),
    ("sparse", 512),
    ("dense", 40),
    ("fanout", 40),
    ("reassign", 160),
)
FIELDS = (
    "round", "order", "mode", "backend", "workload", "objects", "iterations",
    "setup_ns", "elapsed_ns", "baseline_rss_kib", "after_setup_rss_kib",
    "peak_rss_kib",
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--rounds", type=int, default=6)
    parser.add_argument("--objects", type=int, nargs="+", default=[128, 2048])
    args = parser.parse_args()
    if not 3 <= args.rounds <= 12:
        parser.error("rounds must be between 3 and 12")
    if not args.objects or any(not 32 <= n <= 100_000 for n in args.objects):
        parser.error("object counts outside supported range")

    binary = args.binary.resolve(strict=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    rows = []

    with args.output.open("w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=FIELDS)
        writer.writeheader()
        for objects in args.objects:
            for mode in MODES:
                for workload, iterations in WORKLOADS:
                    for repetition in range(args.rounds):
                        offset = repetition % len(BACKENDS)
                        order = BACKENDS[offset:] + BACKENDS[:offset]
                        for ordinal, backend in enumerate(order):
                            command = (
                                str(binary), "--bench", backend, workload,
                                str(objects), str(iterations), mode,
                            )
                            result = subprocess.run(
                                command, check=True, text=True, capture_output=True,
                                timeout=120,
                            )
                            records = [
                                line for line in result.stdout.splitlines()
                                if line.startswith("BENCH,")
                            ]
                            if len(records) != 1:
                                raise RuntimeError(
                                    f"unexpected benchmark output: {result.stdout}"
                                )
                            values = records[0].split(",")
                            if len(values) != 11 or values[1:6] != [
                                backend, workload, str(objects), str(iterations), mode
                            ]:
                                raise RuntimeError(f"malformed benchmark row: {records[0]}")
                            row = dict(zip(
                                FIELDS,
                                (repetition, ordinal, mode, *values[1:5], *values[6:]),
                                strict=True,
                            ))
                            writer.writerow(row)
                            stream.flush()
                            rows.append(row)

    print(f"Raw samples: {args.output}")
    print("Median microseconds per update; same-host, alternating separate processes:")
    for objects in args.objects:
        for mode in MODES:
            print(f"OBJECTS={objects} MODE={mode}")
            for workload, _ in WORKLOADS:
                summary = []
                for backend in BACKENDS:
                    series = [
                        int(row["elapsed_ns"]) / int(row["iterations"]) / 1000
                        for row in rows
                        if row["objects"] == str(objects)
                        and row["mode"] == mode
                        and row["workload"] == workload
                        and row["backend"] == backend
                    ]
                    summary.append(f"{backend}={statistics.median(series):.3f}")
                print(f"  {workload:9} " + " ".join(summary))


if __name__ == "__main__":
    main()
