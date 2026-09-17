#!/usr/bin/env bash

set -euo pipefail

COUNT=100000
URL=127.0.0.1:3001
RESULTS_DIR="benchmark-results/tcp-scaling"

mkdir -p "$RESULTS_DIR"

for WORKERS in 1 2 4 8 16 32 50 64 100 128 192 250; do
    echo
    echo "========================================"
    echo " Workers: $WORKERS"
    echo "========================================"

    cargo run --release -- \
        --mode tcp \
        -u "$URL" \
        -c "$COUNT" \
        -w "$WORKERS" \
        --body "{{value:1:u8}}" \
        -o "$RESULTS_DIR/workers-${WORKERS}.json"
done

jq -s 'sort_by(.concurrency)' \
    "$RESULTS_DIR"/workers-*.json \
    > "$RESULTS_DIR/combined.json"

echo
echo "========================================"
echo " Benchmark completed"
echo "========================================"
echo
echo "Combined results:"
echo "$RESULTS_DIR/combined.json"
