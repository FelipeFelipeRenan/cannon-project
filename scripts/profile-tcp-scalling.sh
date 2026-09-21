#!/usr/bin/env bash

set -euo pipefail

COUNT="${COUNT:-1000000}"
URL="${URL:-127.0.0.1:3001}"
OUTPUT_DIR="${OUTPUT_DIR:-/tmp/cannon-profile}"

CANNON="./target/release/cannon"

if [[ ! -x "$CANNON" ]]; then
    echo "Error: $CANNON not found."
    echo "Run: cargo build --release"
    exit 1
fi

mkdir -p "$OUTPUT_DIR"

for workers in 32 64 128 250; do
    json="$OUTPUT_DIR/${workers}.json"
    stats="$OUTPUT_DIR/${workers}.time"

    echo
    echo "========================================"
    echo " Cannon CPU Profile: ${workers} workers"
    echo "========================================"
    echo "Requests: $COUNT"
    echo "Output:   $json"
    echo

    /usr/bin/time -v \
        -o "$stats" \
        "$CANNON" \
        --mode tcp \
        -u "$URL" \
        -c "$COUNT" \
        -w "$workers" \
        --body "{{value:1:u8}}" \
        -o "$json"

    echo
    echo "Saved:"
    echo "  JSON:  $json"
    echo "  stats: $stats"
done

echo
echo "========================================"
echo " Profiles completed"
echo "========================================"
echo
echo "Results:"
ls -lh "$OUTPUT_DIR"
