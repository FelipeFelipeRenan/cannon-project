<div align="center">

# 🚀 Cannon

### High-performance load testing for HTTP & TCP services

**Fast. Predictable. Lightweight.**

[![Release](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/release.yml/badge.svg)](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/release.yml)
[![CI](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/ci.yml/badge.svg)](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/ci.yml)

</div>

---

Cannon is a **high-performance load testing tool written in Rust** for HTTP APIs and raw TCP services.

It is designed around a simple idea:

> **The load generator should introduce as little overhead as possible so you can focus on the system you're actually testing.**

Cannon provides concurrent request generation, constant-RPS workloads, warm-up phases, dynamic payloads, latency histograms, assertions, and structured reports — all delivered as a single command-line tool.

---

## ⚡ Why Cannon?

Load testing is more than sending a lot of requests.

A useful load generator needs to answer questions like:

* How much traffic did I actually generate?
* What happened to tail latency?
* Did the service remain stable under sustained load?
* How many requests actually succeeded?
* What errors occurred?
* Did the workload behave as expected?
* How did this run compare with a previous baseline?

Cannon is built to make those questions measurable.

---

# 📦 Installation

You **don't need Rust** to use Cannon.

Pre-built binaries are published with each release.

### One-line installation

**Linux / macOS**

```bash
curl -sSL https://raw.githubusercontent.com/FelipeFelipeRenan/cannon-project/main/install.sh | sh
```

The installer downloads the latest compatible release and places `cannon` in `/usr/local/bin`.

Verify:

```bash
cannon --version
```

### 🔄 Update

Already have Cannon installed?

```bash
cannon --update
```

Cannon can check for and install the latest available release.

### 📥 Manual installation

Pre-built binaries are available on the **[Releases](https://github.com/FelipeFelipeRenan/cannon-project/releases)** page.

For example, on Linux:

```bash
chmod +x cannon-linux-x64
sudo mv cannon-linux-x64 /usr/local/bin/cannon
```

### 🖥️ Supported platforms

| OS      | x64                      | arm64                      |
| ------- | ------------------------ | -------------------------- |
| Linux   | `cannon-linux-x64`       | `cannon-linux-arm64`       |
| macOS   | `cannon-macos-x64`       | `cannon-macos-arm64`       |
| Windows | `cannon-windows-x64.exe` | —                          |

The installer script picks the right binary for your OS and CPU and verifies
its SHA-256 checksum. On Windows, download the `.exe` from the Releases page.

### 🦀 Build from source

For development or unreleased changes:

```bash
git clone https://github.com/FelipeFelipeRenan/cannon-project.git
cd cannon-project
cargo build --release
```

The binary will be available at:

```text
target/release/cannon
```

---

# 🚀 Quick Start

## Simple HTTP load test

Send 5,000 requests using 20 workers:

```bash
cannon \
  -u http://localhost:8080/api \
  -c 5000 \
  -w 20
```

That's it.

---

## 🎯 Constant-RPS workload

Generate **100 requests per second**:

```bash
cannon \
  -u http://localhost:8080/api \
  -c 5000 \
  -w 20 \
  --rps 100
```

Cannon separates the **requested load** from the **measured results**, allowing you to compare the target rate against the actual throughput achieved during the test.

---

## 🔥 Warm-up + measurement

Warm up the service for 10 seconds, then measure 5,000 requests:

```bash
cannon \
  -u http://localhost:8080/api \
  -c 5000 \
  -w 20 \
  --rps 100 \
  --warmup 10
```

The warm-up generates real traffic, but its requests are **excluded from the reported measurements**.

This avoids mixing cold-start behavior with the actual benchmark.

---

## 📈 Ramp-up

Increase the load gradually instead of starting at full speed:

```bash
cannon \
  -u http://localhost:8080/api \
  -c 20000 \
  -w 50 \
  --rps 500 \
  --ramp-up 30s
```

The request rate grows linearly from zero to `--rps` over the ramp-up
duration (`500ms`, `30s` or `2m`) and then stays constant. `--ramp-up`
requires `--rps`.

---

## 🧬 Dynamic payloads

Generate different data for every request:

```bash
cannon \
  -u http://localhost:8080/users \
  -X POST \
  -c 1000 \
  --body '{"id":"{{uuid}}","email":"{{email}}"}' \
  -H 'Content-Type: application/json'
```

Available dynamic values include:

| Tag             | Description            |
| --------------- | ---------------------- |
| `{{uuid}}`      | UUID v4                |
| `{{email}}`     | Random email           |
| `{{username}}`  | Random username        |
| `{{number}}`    | Random number          |
| `{{timestamp}}` | Current Unix timestamp |

---

# 🗂️ YAML configuration

Keep a scenario in a file and run it with `-f`:

```yaml
# scenario.yml
url: "http://localhost:8080/api/v1/accounts"
method: "GET"
workers: 150
count: 5000
rps: 1000
ramp_up: 10s
apdex_t: 150
headers:
  - "Authorization: Bearer <token>"
  - "Content-Type: application/json"
```

```bash
cannon -f scenario.yml
```

Command-line flags always take precedence over the file, so you can reuse one
scenario and tweak a single value: `cannon -f scenario.yml -c 100`.
See [`examples/scenario.yml`](examples/scenario.yml).

Supported keys: `url`, `mode`, `method`, `headers`, `body`, `expect`, `workers`,
`count`, `rps`, `ramp_up`, `warmup`, `timeout`, `connect_timeout`, `user_agent`,
`insecure`, `http2`, `percentiles`, `apdex_t`, `output`, `html`, `csv`,
`save_baseline`, `compare_baseline`, `tolerance`, `pin_threads`.

---

# 🚦 Performance regression gate

Save a baseline from a known-good run, then fail the build when latency gets
worse:

```bash
# once, on main
cannon -u "$API_URL/health" -c 5000 --save-baseline baseline.json

# on every pull request
cannon -u "$API_URL/health" -c 5000 \
  --compare-baseline baseline.json \
  --tolerance 10
```

Cannon compares the **p99 latency of successful requests** with the baseline.
If it is slower by more than `--tolerance` percent (default `5`), it prints the
regression and exits with status **2**, which fails the CI job.

The **error rate** is checked too: a service that is down answers (with errors)
very quickly, so latency alone would make it look faster. By default the limit
is the error rate stored in the baseline; set it explicitly with
`--max-error-rate <PERCENT>`. A run without any successful request always
fails the gate, and a baseline cannot be saved from such a run.

---

# 📊 What Cannon Measures

Cannon collects latency information using **HdrHistogram** and reports configurable percentiles.

By default:

```text
p50   p95   p99
```

You can choose your own:

```bash
cannon \
  -u http://localhost:8080 \
  -c 10000 \
  --percentiles 50,90,95,99,99.9
```

### Terminal output

The final report includes information such as:

* total requests
* successful requests
* failed requests
* actual RPS
* latency percentiles
* minimum / average / maximum latency
* status-code distribution
* failure breakdown
* bytes sent / received
* Apdex
* target vs actual RPS

The goal is not simply to tell you **"it handled 10k requests."**

The goal is to show **how the system behaved while handling them**.

---

# 📈 Reports

Cannon can export results in multiple formats.

### JSON

```bash
cannon \
  -u http://localhost:8080 \
  -c 5000 \
  -o report.json
```

Useful for automation, CI/CD pipelines and post-processing.

### CSV

```bash
cannon \
  -u http://localhost:8080 \
  -c 5000 \
  --csv results.csv
```

Exports per-request data for external analysis.

### HTML

```bash
cannon \
  -u http://localhost:8080 \
  -c 5000 \
  --html dashboard.html
```

Generates an interactive report with charts and summarized metrics.

---

# 🧪 Response Assertions

You can validate that responses contain an expected value:

```bash
cannon \
  -u http://localhost:8080/api/user \
  -c 1000 \
  --expect '"status":"success"'
```

A request can therefore fail not only because the connection failed, but also because the response did not satisfy the expected assertion.

This is particularly useful for automated performance checks.

---

# 🔌 Raw TCP

Cannon is not limited to HTTP.

Use TCP mode for custom protocols and binary payloads:

```bash
cannon \
  --mode tcp \
  -u 127.0.0.1:9000 \
  -c 1000 \
  -w 20 \
  --body '{{value:1:u8}}'
```

TCP mode writes the payload on a persistent connection and then waits for the
response, so a payload (`--body`) is required. TCP has no message boundaries,
so tell Cannon where a response ends (the default is **one byte**):

| Flag                      | Meaning                                                    |
| ------------------------- | ---------------------------------------------------------- |
| `--tcp-read-bytes <N>`    | Read exactly `N` bytes per response (`0` = do not wait)    |
| `--tcp-read-until <DELIM>`| Read until the response ends with `DELIM` (e.g. `'\r\n'`)  |

```bash
cannon --mode tcp -u 127.0.0.1:6379 --body 'PING\r\n' --tcp-read-until '\r\n'
```

If the server sends more data than Cannon read, the run reports a
`Response Framing` error instead of producing meaningless latencies. When a
connection breaks it is replaced by the next request, so a server that goes
away produces fast failures rather than a hung run. `--timeout` and
`--connect-timeout` apply to TCP just like HTTP; requests that get no answer
are reported as `Timeout` failures.

Binary payloads can be generated directly:

```text
{{number:u8}}
{{number:u16be}}
{{number:u16le}}
{{number:u32be}}
{{number:u32le}}
{{number:u64be}}
{{number:u64le}}
```

Fixed binary values are also supported:

```text
{{value:42:u8}}
{{value:1000:u16be}}
```

This makes Cannon useful for testing services that don't speak HTTP at all.

---

# 🛠️ CLI

Cannon exposes a deliberately small command-line interface.

| Option              | Description                          |
| ------------------- | ------------------------------------ |
| `-u, --url`         | Target endpoint                      |
| `-c, --count`       | Number of measured requests          |
| `-w, --workers`     | Number of concurrent workers         |
| `-r, --rps`         | Target request rate                  |
| `--warmup`          | Warm-up duration                     |
| `--ramp-up`         | Ramp the rate up to `--rps` over a duration |
| `-f, --config`      | YAML scenario file                   |
| `--mode`            | `http` or `tcp`                      |
| `-X, --method`      | HTTP method                          |
| `-b, --body`        | Request payload                      |
| `-H, --header`      | HTTP header                          |
| `-A, --user-agent`  | User-Agent                           |
| `-t, --timeout`     | Request timeout                      |
| `--connect-timeout` | TCP connection timeout               |
| `--max-redirects`   | Follow up to N redirects (default 0: the 3xx is reported; each followed redirect is an extra hit on the server) |
| `--expect-status`   | Status codes that count as success, e.g. `200,404` or `2xx,3xx` (default: any 2xx) |
| `--cacert`          | PEM file with extra CA certificates to trust |
| `--cert`, `--key`   | PEM client certificate and key (mutual TLS) |
| `--no-proxy`        | Ignore `HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY` |
| `--no-color`        | Disable colored output               |
| `--expect`          | Response assertion                   |
| `--percentiles`     | Custom latency percentiles           |
| `--apdex-t`         | Apdex threshold                      |
| `-o, --output`      | JSON report                          |
| `--csv`             | CSV report                           |
| `--html`            | HTML report                          |
| `--save-baseline`   | Save p99 latency and RPS as a baseline |
| `--compare-baseline`| Compare against a saved baseline     |
| `--tolerance`       | Allowed p99 regression in percent (default 5) |
| `--pin-threads`     | Pin runtime threads to CPU cores     |
| `--http2`           | Force HTTP/2 prior knowledge         |
| `-k, --insecure`    | Disable TLS certificate verification |
| `--update`          | Update Cannon                        |

For the complete list:

```bash
cannon --help
```

### Interrupting a run

Press **Ctrl-C** once to stop gracefully: no new requests are scheduled,
in-flight requests finish, and the report for what was measured so far is still
printed and exported. Press **Ctrl-C** again to quit immediately.

### Exit codes

| Code | Meaning                                                              |
| ---- | -------------------------------------------------------------------- |
| `0`  | The run finished (request failures are reported, not fatal)          |
| `1`  | Invalid configuration or an internal/IO error                        |
| `2`  | Regression gate failed (p99 regression or error rate too high)      |
| `130`| Interrupted with Ctrl-C (SIGINT); the partial report is still written|
| `143`| Terminated (SIGTERM); the partial report is still written            |

---

# 🧠 Measurement Model

One important design decision in Cannon is the separation between **warm-up traffic** and **measured traffic**.

With:

```bash
cannon -c 250000 --warmup 4
```

the execution is conceptually:

```text
┌──────────────────────┐
│      Prepare         │
└──────────┬───────────┘
           │
           ▼
┌──────────────────────┐
│      Warm-up         │
│       4 seconds      │
│                      │
│   Metrics ignored    │
└──────────┬───────────┘
           │
           ▼
┌──────────────────────┐
│     Measurement      │
│                      │
│   250,000 requests   │
│      measured        │
└──────────┬───────────┘
           │
           ▼
┌──────────────────────┐
│        Report        │
└──────────────────────┘
```

Without warm-up, the requested count represents the measured workload directly.

This makes benchmark runs easier to reason about and compare.

---

# 🏗️ Architecture

Cannon is built around a small set of focused components:

```text
                         ┌───────────────┐
                         │   CLI / Args  │
                         └───────┬───────┘
                                 │
                                 ▼
                         ┌───────────────┐
                         │     Engine    │
                         └───────┬───────┘
                                 │
                    ┌────────────┴────────────┐
                    │                         │
                    ▼                         ▼
             ┌──────────────┐         ┌──────────────┐
             │    Workers   │         │    Payload   │
             │              │         │   Generator  │
             └──────┬───────┘         └──────────────┘
                    │
                    ▼
             ┌──────────────┐
             │    Target    │
             ├──────────────┤
             │ HTTP │ TCP   │
             └──────┬───────┘
                    │
                    ▼
                 Service
                    │
                    ▼
             ┌──────────────┐
             │   Metrics    │
             │  Histogram   │
             └──────┬───────┘
                    │
                    ▼
             ┌──────────────┐
             │    Report    │
             └──────────────┘
```

### Core technologies

* **Rust** — systems-level control and predictable runtime characteristics
* **Tokio** — asynchronous networking and task scheduling
* **Reqwest** — HTTP client
* **HdrHistogram** — latency distribution
* **Clap** — CLI parsing
* **MiMalloc** — alternative allocator
* **rustls** — TLS without native OpenSSL dependencies

---

# ⚙️ Design Goals

Cannon is intentionally opinionated.

### Small runtime overhead

The generator itself should not become the bottleneck of the benchmark.

### Predictable workloads

Constant-RPS mode and explicit warm-up/measurement phases make test behavior easier to reason about.

### Reusable connections

HTTP clients and TCP connections are reused instead of creating a new connection for every request.

### Low-level control

Rust makes it possible to control allocation, concurrency, networking and binary payload generation without requiring a large abstraction stack.

### One executable

The end-user experience should be simple:

```text
install
  ↓
cannon
  ↓
measure
  ↓
report
```

---

# 🔬 Performance

Cannon's design goal is a load generator that stays out of the way, and the
numbers should be reproducible rather than marketing.

A reference microbenchmark is documented in
[`docs/benchmarks/tcp-baseline.md`](docs/benchmarks/tcp-baseline.md): about
**580k req/s** over persistent localhost TCP connections (one-byte request and
response, 250 workers, 8 isolated client CPUs, WSL2), with p99 around 1.1 ms.

This is a **microbenchmark**, not a promise. Real throughput depends on network
distance, payload size, server work, TLS and protocol. Use
[`scripts/bench.sh`](scripts/bench.sh) to measure on your own hardware, and
always compare tools on the same machine and workload.

---

# 🤖 CI/CD

Cannon can be used as part of automated performance checks.

For example:

```bash
cannon \
  -u "$API_URL/health" \
  -c 1000 \
  -w 20 \
  --expect '"status":"ok"' \
  -o results.json
```

The generated JSON can then be consumed by scripts or CI pipelines.

Example:

```bash
jq -e '.failures == 0' results.json
```

`jq -e` sets the exit status from the result, so the pipeline step fails when
there were failures.

---

# 🧰 Development

Run the test suite (unit tests plus end-to-end tests that run the real binary
against local HTTP and TCP servers):

```bash
cargo test
```

Run documentation tests:

```bash
cargo test --doc
```

Run Clippy:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Build the optimized binary:

```bash
cargo build --release
```

Generate API documentation:

```bash
cargo doc --no-deps
```

---

# 🗺️ Roadmap

Cannon is actively evolving.

### Completed

* [x] HTTP load testing
* [x] Raw TCP mode
* [x] Concurrent workers
* [x] Constant-RPS scheduling
* [x] Warm-up phases
* [x] Dynamic payload generation
* [x] Binary payload generation
* [x] Response assertions
* [x] Latency histograms
* [x] Custom percentiles
* [x] JSON reports
* [x] CSV export
* [x] HTML reports
* [x] Apdex
* [x] Baseline comparison
* [x] CPU affinity
* [x] Automatic updates

### Next

* [ ] Publish to crates.io and package managers (Homebrew, Docker image)
* [ ] GitHub Action for the performance regression gate
* [ ] Multi-step scenarios

### Future direction

These are long-term goals, not part of the current release:

* [ ] Chaos engineering workloads (fault and latency injection)
* [ ] Distributed load generation across multiple machines

See [`CHANGELOG.md`](CHANGELOG.md) for what changed in each release.

---

# 📜 License

Cannon is dual-licensed under the [MIT License](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE), at your option.

---

<div align="center">

### Built in Rust 🦀

**Cannon — generate the load. Measure the system.**

</div>



### **👨‍💻 Author**

**Felipe Fernandes** – [LinkedIn](https://linkedin.com/in/felipefernandesss) | [GitHub](https://github.com/FelipeFelipeRenan)

*"Robust systems require relentless testing."*

