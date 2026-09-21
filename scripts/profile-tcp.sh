#!/usr/bin/env bash

set -euo pipefail

COUNT="${COUNT:-1000000}"
WORKERS="${WORKERS:-250}"
URL="${URL:-127.0.0.1:3001}"
OUTPUT="${OUTPUT:-/tmp/cannon-perf.json}"

CANNON="./target/release/cannon"

if [[ ! -x "$CANNON" ]]; then
    echo "Error: $CANNON not found."
    echo "Run: cargo build --release"
    exit 1
fi

if ! command -v perf >/dev/null 2>&1; then
    echo "Error: perf is not installed."
    exit 1
fi

echo "========================================"
echo " Cannon CPU Profile"
echo "========================================"
echo "Target:   $URL"
echo "Requests: $COUNT"
echo "Workers:  $WORKERS"
echo "Report:   $OUTPUT"
echo "========================================"
echo

echo "Running perf stat..."
echo

perf stat \
    "$CANNON" \
    --mode tcp \
    -u "$URL" \
    -c "$COUNT" \
    -w "$WORKERS" \
    --body "{{value:1:u8}}" \
    -o "$OUTPUT"

echo
echo "========================================"
echo " Benchmark report"
echo "========================================"

if command -v jq >/dev/null 2>&1; then
    jq '{
        requests: .total_requests,
        successes: .successes,
        failures: .failures,
        duration_secs: .duration_secs,
        actual_rps: .actual_rps,
        p50_ms: .p50_ms,
        p95_ms: .p95_ms,
        p99_ms: .p99_ms,
        max_ms: .max_ms
    }' "$OUTPUT"
else
    cat "$OUTPUT"
fi
