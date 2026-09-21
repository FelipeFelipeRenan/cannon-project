#!/usr/bin/env bash
set -euo pipefail

COUNT=1000000
URL=127.0.0.1:3001
RUNS=3

RESULTS_DIR="benchmark-results/tcp-long"

mkdir -p "$RESULTS_DIR"
rm -f "$RESULTS_DIR"/workers-*-run-*.json
rm -f "$RESULTS_DIR"/combined.json
rm -f "$RESULTS_DIR"/summary.tsv

for WORKERS in 32 64 128 250; do
    for RUN in $(seq 1 "$RUNS"); do
        OUTPUT="$RESULTS_DIR/workers-${WORKERS}-run-${RUN}.json"

        echo
        echo "========================================"
        echo "Workers: $WORKERS"
        echo "Run:     $RUN/$RUNS"
        echo "Requests: $COUNT"
        echo "========================================"

        cargo run --release -- \
            --mode tcp \
            -u "$URL" \
            -c "$COUNT" \
            -w "$WORKERS" \
            --body "{{value:1:u8}}" \
            -o "$OUTPUT"
    done
done

jq -s 'sort_by(.concurrency)' \
    "$RESULTS_DIR"/workers-*-run-*.json \
    > "$RESULTS_DIR/combined.json"

{
    printf 'workers\truns\tavg_rps\trps_stddev\tmin_rps\tmax_rps\tavg_p50_ms\tavg_p95_ms\tavg_p99_ms\n'

    jq -r '
        def avg:
            add / length;

        def stdev:
            (avg) as $mean |
            map((. - $mean) * (. - $mean)) |
            avg |
            sqrt;

        group_by(.concurrency)[] |
        {
            workers: .[0].concurrency,
            runs: length,
            avg_rps: (map(.actual_rps) | avg),
            rps_stddev: (map(.actual_rps) | stdev),
            min_rps: (map(.actual_rps) | min),
            max_rps: (map(.actual_rps) | max),
            avg_p50: (map(.p50_ms) | avg),
            avg_p95: (map(.p95_ms) | avg),
            avg_p99: (map(.p99_ms) | avg)
        } |

        [
            .workers,
            .runs,
            .avg_rps,
            .rps_stddev,
            .min_rps,
            .max_rps,
            .avg_p50,
            .avg_p95,
            .avg_p99
        ] |

        @tsv
    ' "$RESULTS_DIR/combined.json"
} > "$RESULTS_DIR/summary.tsv"

echo
echo "========================================"
echo "Benchmark summary"
echo "========================================"

column -t -s $'\t' "$RESULTS_DIR/summary.tsv"
