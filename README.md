[![Release Cannon](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/release.yml/badge.svg)](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/release.yml)

[![CI Pipeline](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/ci.yml/badge.svg)](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/ci.yml)

# **🚀 Cannon**

```
      _____          _   _ _   _  ____  _   _
     / ____|   /\   | \ | | \ | |/ __ \| \ | |
    | |       /  \  |  \| |  \| | |  | |  \| |
    | |      / /\ \ | . ` | . ` | |  | | . ` |
    | |____ / ____ \| |\  | |\  | |__| | |\  |
     \_____/_/    \_\_| \_|_| \_|\____/|_| \_|

```

**The High-Velocity Load Tester**

**Cannon** is a lightweight, high-velocity load testing tool written in **Rust**. Built for mission-critical APIs, it leverages the Tokio async runtime to deliver sub-millisecond precision and extreme concurrency with minimal resource consumption.

## **✨ Key Features**

* **⚡ Extremely Fast:** Built in Rust with zero garbage collection. The bottleneck will be your server, not your testing tool.
* **🎯 Constant RPS Mode:** Integrated metronome logic to fire requests at an exact rate (Requests Per Second) for stable load testing.
* **📈 Progressive Ramp-up:** Gradually increase load from 1 RPS to your target, simulating realistic traffic growth scenarios.
* **🧬 Dynamic Payloads:** Real-time data generation engine. Use placeholders like `{{user}}`, `{{email}}`, `{{uuid}}`, `{{timestamp}}`, and `{{number}}` to simulate unique users.
* **📊 Comprehensive Reports:** Interactive CLI with real-time RPS counter, ASCII latency histogram, detailed JSON reports, and **interactive HTML dashboard** with Chart.js graphs.
* **🎯 Apdex Score:** User satisfaction metric based on response times (≤50ms satisfactory, 51-200ms tolerable).
* **🛠️ Full REST Support:** GET, POST, PUT, PATCH, DELETE with custom headers, configurable user-agent, and dynamic JSON bodies.
* **✅ Response Validation:** Assert HTTP response content with the `--expect` flag.
* **📦 Zero-Dependency Binary:** Single static binary. Download and run on Linux, macOS, or Windows without installing runtimes.
* **🔄 Auto-Update:** Automatically update to the latest version with `--update`.

## **📦 Installation**

### **Pre-compiled Binaries (Recommended)**

Download the latest binary for your architecture from the **Releases** page.

#### Install globally on Linux via cURL:

```bash
curl -sSL https://raw.githubusercontent.com/FelipeFelipeRenan/cannon-project/main/install.sh | sh
```

#### Manual example for Linux:

```bash
chmod +x cannon-linux-x64
sudo mv cannon-linux-x64 /usr/local/bin/cannon
```

#### Verify installation:

```bash
cannon --version
```

#### Update to latest version:

```bash
cannon --update
```

## **🛠️ CLI Arguments**

Use these flags to configure your load test.

| Short Flag | Long Flag | Description | Default |
| :---- | :---- | :---- | :---- |
| `-u` | `--url` | **(Required)** Target endpoint URL (e.g., `http://localhost:8080/api`). | - |
| `-c` | `--count` | Total number of requests to fire. | `1` |
| `-w` | `--workers` | Number of concurrent workers (simultaneous connections). | `10` |
| `-r` | `--rps` | Limits throughput to a specific RPS (Constant Load + Ramp-up). | None |
| `-X` | `--method` | HTTP Method: GET, POST, PUT, PATCH, DELETE. | `GET` |
| `-b` | `--body` | JSON payload for the request. Supports dynamic tags. | None |
| `-o` | `--output` | Path to save detailed report in `.json` format. | None |
| `-t` | `--timeout` | Timeout in milliseconds to cancel slow requests. | `30000` |
| `-H` | `--header` | Custom header (e.g., `Authorization: Bearer token`). Repeat for multiple. | None |
| `-A` | `--user-agent` | Request User-Agent. | `Cannon/1.0` |
| - | `--ramp-up` | Time in seconds for progressive warm-up (e.g., `15s`, `2m`). | None |
| - | `--expect` | Expected string in response body for validation (assertion). | None |
| - | `--html` | Path to save interactive HTML report with charts. | None |
| - | `--update` | Checks and installs available update. | - |

## **🧬 Dynamic Payload Tags**

When using the `--body` flag, you can inject dynamic data into the JSON to ensure unique requests and bypass database constraints.

| Tag | Substitution Logic | Example Usage |
| :---- | :---- | :---- |
| `{{user}}` | Generates random 8-character alphanumeric string (lowercase). | `"username": "user_{{user}}"` |
| `{{random}}` | Alias for `{{user}}`. | `"id": "req_{{random}}"` |
| `{{email}}` | Generates random email in format `xxxxxxxx@example.com`. | `"email": "{{email}}"` |
| `{{number}}` | Generates random integer between 10 and 9999. | `"amount": {{number}}` |
| `{{uuid}}` | Generates unique UUID v4. | `"requestId": "{{uuid}}"` |
| `{{timestamp}}` | Generates Unix timestamp in milliseconds. | `"createdAt": {{timestamp}}` |

## **🚀 Quick Start**

### **1. Simple Stress Test (GET)**

Fire 5000 requests with 20 concurrent workers:

```bash
cannon -u http://localhost:8081/api/v1/accounts -c 5000 -w 20
```

### **2. Stable Load Simulation (RPS + Dynamic POST)**

Create 1000 unique accounts at a constant rate of 100 requests per second:

```bash
cannon -u http://localhost:8081/api/v1/accounts \
    -c 1000 \
    -w 10 \
    -X POST \
    --rps 100 \
    --body '{"clientId": "user_{{user}}_test", "currency": "BRL"}'
```

### **3. Stress Test with Progressive Ramp-up**

Fire 5000 requests, gradually increasing to 100 RPS over 15 seconds:

```bash
cannon -u 'http://localhost:8081/api/v1/accounts' \
  -c 5000 \
  -w 20 \
  --rps 100 \
  --ramp-up 15s \
  -H 'Authorization: Bearer your_token'
```

### **4. Test with Response Validation (Assertion)**

Validate if response body contains the string `admin_privileges`:

```bash
cannon -u 'http://api.example.com/v1/user' \
  --expect 'admin_privileges' \
  -c 1000 -w 10
```

### **5. Complete Test with JSON and HTML Export**

```bash
cannon -u 'http://localhost:8081/api/v1/accounts' \
  -c 5000 \
  -w 50 \
  --rps 200 \
  --ramp-up 10s \
  -X POST \
  --body '{"userId": "{{uuid}}", "ts": {{timestamp}}, "email": "{{email}}"}' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer token123' \
  -A 'CannonLoadTester/2.0' \
  -t 5000 \
  --expect '"status":"success"' \
  -o report.json \
  --html dashboard.html
```

## **🔍 Understanding the Report**

At the end of each execution, Cannon provides a surgical analysis of your API health:

### **Terminal Report (CLI)**

* **Percentile Latency:** p50, p95, p99 - Essential for identifying "tail latency" that averages hide.
* **ASCII Histogram:** Visual representation of latency distribution directly in the terminal.
* **Status Codes:** Distribution of HTTP responses (2xx, 4xx, 5xx) with colors.
* **Failure Breakdown:** List of errors with occurrence count.
* **Network Efficiency:** Upload/download throughput in MB/s.
* **Cannon Efficiency:** Comparison between **Target RPS** vs **Actual RPS** to validate test integrity.
* **Apdex Score:** User satisfaction index (0.0 to 1.0).

### **JSON Report (`--output`)**

Exports structured metrics for CI/CD integration, dashboards, or later analysis:

```json
{
  "target": "http://localhost:8081/api/v1/accounts",
  "total_requests": 5000,
  "concurrency": 50,
  "successes": 4987,
  "failures": 13,
  "min_ms": 12.5,
  "avg_ms": 45.3,
  "p50_ms": 38.2,
  "p95_ms": 89.7,
  "p99_ms": 156.4,
  "max_ms": 523.1,
  "actual_rps": 198.6,
  "apdex_score": 0.92,
  "bytes_sent": 1048576,
  "bytes_received": 5242880,
  "duration_secs": 25.1,
  "status_codes": { "200": 4987, "500": 13 },
  "errors": { "Timeout": 5, "Connection Error": 8 }
}
```

### **HTML Dashboard (`--html`)**

Interactive dashboard with:
* Chart.js graphs for latency and status codes
* Cards with key metrics (Apdex, RPS, Successes/Failures)
* Button to export PDF
* Detailed error table
* Responsive dark theme design

## **📊 Apdex System**

Cannon automatically calculates the **Application Performance Index (Apdex)**:

* **Satisfactory (≤50ms):** Full count
* **Tolerable (51ms - 200ms):** Half point
* **Frustrating (>200ms):** Zero points

**Formula:** `Apdex = (Satisfactory + Tolerable/2) / Total`

**Classification:**
* **≥0.94:** Excellent 🟢
* **≥0.85:** Good 🟢
* **≥0.70:** Fair 🟡
* **≥0.50:** Poor 🟡
* **<0.50:** Unacceptable 🔴

## **🛠️ Technical Architecture**

Cannon is built on modern distributed systems principles:

* **Producer-Consumer Pattern:** Request engine (Producer) and metrics aggregator (Consumer) communicate via **MPSC Channels** to eliminate memory contention and ensure thread safety.
* **Backpressure:** Strictly managed concurrency via **Semaphores** to prevent socket exhaustion and memory spikes.
* **High-Precision Metrics:** Uses **HdrHistogram** to record latencies in microseconds, avoiding the "coordinated omission" problem common in legacy load testers.
* **Tokio Async Runtime:** Leverages non-blocking I/O and multiplexing for maximum efficiency.
* **TLS via rustls:** Secure and performant TLS implementation without native dependencies.

## **📋 Use Case Examples**

### **CI/CD Pipeline Integration**

```bash
# In your GitHub Actions / GitLab CI pipeline
cannon -u "$API_URL/health" -c 100 -w 10 --expect '"status":"ok"' -o results.json

# Parse JSON to validate thresholds
jq '.apdex_score >= 0.9 and .failures == 0' results.json
```

### **Soak Test**

```bash
# 100k requests at 50 constant RPS for ~33 minutes
cannon -u http://api.prod.com/users -c 100000 --rps 50 -o soak-test.json
```

### **Spike Test**

```bash
# Sudden spike of 500 RPS for 60 seconds
cannon -u http://api.prod.com/checkout -c 30000 --rps 500 -w 100
```

### **Cache Validation**

```bash
# First request (cache miss) vs subsequent (cache hit)
cannon -u http://cdn.example.com/assets/logo.png -c 1000 --rps 100
```

## **⚠️ Important Considerations**

* **System Limits:** Adjust `ulimit -n` (file descriptors) for high-concurrency tests.
* **Network Bandwidth:** High RPS tests may saturate your local network connection.
* **Target Capacity:** Ensure the target can support the generated load to avoid false positives.
* **Rate Limiting:** APIs with rate limiting may return 429 Too Many Requests during intense tests.

## **🤝 Contributing**

Contributions are welcome! Feel free to:

1. Fork the repository
2. Create a branch for your feature (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## **📄 License**

Distributed under the MIT License. See `LICENSE` for more information.

### **👨‍💻 Author**

**Felipe Fernandes** – [LinkedIn](https://linkedin.com/in/felipefernandesss) | [GitHub](https://github.com/FelipeFelipeRenan)

*"Robust systems require relentless testing."*

---

**Version:** 1.0.5 | **Build:** LTO-optimized Release
