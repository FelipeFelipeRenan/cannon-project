#!/usr/bin/env python3
"""Measure Cannon's own CPU and memory use (Linux/macOS).

Usage: scripts/footprint.py URL [BINARY] [RUNS]

Runs a closed-loop test for several worker counts and prints, per scenario, the
median wall time, CPU time per request and peak resident memory of the Cannon
process alone (the target is not included). Point it at a server you run
yourself; results depend on that server and on how many cores are free.
"""
import os
import statistics
import subprocess
import sys
import time

URL = sys.argv[1] if len(sys.argv) > 1 else sys.exit(__doc__)
BINARY = sys.argv[2] if len(sys.argv) > 2 else "./target/release/cannon"
RUNS = int(sys.argv[3]) if len(sys.argv) > 3 else 3
REQUESTS = 200_000

SCENARIOS = [
    ("closed, 200 workers", ["-w", "200"]),
    ("closed, 2000 workers", ["-w", "2000"]),
    ("closed, 5000 workers", ["-w", "5000"]),
    ("closed, 200 workers, --csv", ["-w", "200", "--csv", os.devnull]),
    ("closed, 200 workers, --threads 1", ["-w", "200", "--threads", "1"]),
]

print(f"{'scenario':36} {'wall':>7} {'cpu/req':>9} {'peak RSS':>9}")
for name, extra in SCENARIOS:
    runs = []
    for _ in range(RUNS):
        started = time.time()
        child = subprocess.Popen(
            [BINARY, "-u", URL, "-c", str(REQUESTS), "--no-color", *extra],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        _, _, usage = os.wait4(child.pid, 0)
        runs.append(
            (time.time() - started, usage.ru_utime + usage.ru_stime, usage.ru_maxrss)
        )

    wall = statistics.median(r[0] for r in runs)
    cpu = statistics.median(r[1] for r in runs)
    rss = statistics.median(r[2] for r in runs)
    rss_mb = rss / (1024 * 1024) if sys.platform == "darwin" else rss / 1024
    print(f"{name:36} {wall:6.2f}s {cpu / REQUESTS * 1e6:7.1f}us {rss_mb:7.0f}MB")
