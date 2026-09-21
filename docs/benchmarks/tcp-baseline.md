# TCP Performance Baseline

This document records the baseline established during the Cannon v2.1.0 performance audit.

## Environment

- OS/runtime environment: WSL2
- Kernel: Linux 6.6.87.2-microsoft-standard-WSL2
- Host CPUs available: 16
- Cannon CPU isolation: CPUs 0-7
- Benchmark target: local TCP echo server on `127.0.0.1:3001`
- Server CPU isolation during the scaling analysis: CPUs 8-15

## Workload

The final baseline used:

```bash
taskset -c 0-7 ./target/release/cannon \
  --mode tcp \
  -u 127.0.0.1:3001 \
  -c 1000000 \
  -w 250 \
  --body "{{value:1:u8}}" \
  -o /tmp/cannon-final-benchmark.json
```

The TCP echo server reads one byte and responds with one byte. Cannon maintains a pool of 250 persistent TCP connections.

## Final baseline

| Metric | Result |
|---|---:|
| Requests | 1,000,000 |
| Workers | 250 |
| Cannon CPUs | 8 |
| Successes | 1,000,000 |
| Failures | 0 |
| Throughput | 579,324.96 req/s |
| p50 | 0.39 ms |
| p95 | 0.77 ms |
| p99 | 1.10 ms |
| Maximum latency | 3.30 ms |
| Reported duration | 1.7261 s |
| CLI total time | 2.0895 s |

The reported duration and CLI wall-clock time differ because the latter also includes execution/reporting overhead outside the measured request phase.

## Scaling observations

With the TCP server isolated to the other eight CPUs, Cannon showed increasing throughput as additional client CPUs were made available:

| Cannon CPUs | Approx. throughput |
|---:|---:|
| 2 | 265k req/s |
| 4 | 472k req/s |
| 8 | 612k req/s |

The final 1M-request run measured approximately 579k req/s, demonstrating repeatability in the same general range while avoiding syscall tracing overhead.

## Syscall profile

A `strace -f -c` profile was used only to identify the dominant execution path; its throughput and latency numbers are not benchmark results.

Network-only tracing showed:

- `sendto`: 56.29% of observed syscall time
- `recvfrom`: 43.64%
- connection setup syscalls: negligible

The full syscall profile also showed futex synchronization as a secondary contributor.

The results indicate that this local TCP workload is dominated by network/kernel I/O, with synchronization/scheduling as a secondary cost. There was no evidence from this audit that replacing Tokio, the event loop, or the worker architecture would provide a sufficiently justified optimization for the current Cannon roadmap.

## Scope and limitations

This is a **microbenchmark baseline**, not a general claim about Cannon's maximum throughput.

The workload uses:

- localhost networking;
- a one-byte request;
- a one-byte response;
- persistent TCP connections;
- 250 workers;
- an isolated 8-CPU client workload.

Real-world throughput and latency will vary significantly with network distance, protocol behavior, payload size, server processing time, TLS, HTTP semantics, and workload configuration.

The baseline exists primarily to provide a reproducible reference point for future Cannon performance changes.

## Conclusion

The performance audit established a stable baseline and found no immediate hot-path optimization that justifies architectural changes.

Future performance work should be evaluated against this baseline rather than optimized against the synthetic localhost workload indefinitely.
