#!/usr/bin/env python3
"""Alternating per-process Rust Release samples; raw data, not a ranking."""
import argparse
import csv
import statistics
import subprocess
from pathlib import Path

BACKENDS = ("tde", "bevy", "runen")
WORKLOADS = (
    ("sparse", 512),
    ("dense", 40),
    ("fanout", 40),
    ("reassign", 160),
)
FIELDS = (
    "round", "order", "backend", "workload", "objects", "iterations",
    "setup_ns", "apply_ns", "baseline_rss_kib", "after_setup_rss_kib",
    "peak_rss_kib",
)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--rounds", type=int, default=6)
    parser.add_argument("--objects", type=int, default=2048)
    args = parser.parse_args()
    if not 3 <= args.rounds <= 12:
        parser.error("rounds must be between 3 and 12")
    if not 32 <= args.objects <= 100_000:
        parser.error("object count outside supported range")
    binary = args.binary.resolve(strict=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    rows = []

    with args.output.open("w", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=FIELDS)
        writer.writeheader()
        for workload, iterations in WORKLOADS:
            for repetition in range(args.rounds):
                offset = repetition % len(BACKENDS)
                order = BACKENDS[offset:] + BACKENDS[:offset]
                for ordinal, backend in enumerate(order):
                    command = (
                        str(binary), "--bench", backend, workload,
                        str(args.objects), str(iterations),
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
                        raise RuntimeError(f"unexpected benchmark output: {result.stdout}")
                    values = records[0].split(",")
                    if len(values) != 10 or values[1:5] != [
                        backend, workload, str(args.objects), str(iterations)
                    ]:
                        raise RuntimeError(f"malformed benchmark row: {records[0]}")
                    row = dict(zip(
                        FIELDS,
                        (repetition, ordinal, *values[1:]),
                        strict=True,
                    ))
                    writer.writerow(row)
                    stream.flush()
                    rows.append(row)

    print(f"Raw samples: {args.output}")
    print("Median application latency, microseconds per alternating update:")
    for workload, _ in WORKLOADS:
        for backend in BACKENDS:
            series = [
                int(row["apply_ns"]) / int(row["iterations"]) / 1000
                for row in rows
                if row["workload"] == workload and row["backend"] == backend
            ]
            print(f"{workload:9} {backend:5} {statistics.median(series):.3f} us/op")


if __name__ == "__main__":
    main()
