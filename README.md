# **🚀 Cannon**

      _____          _   _ _   _  ____  _   _ 
     / ____|   /\   | \ | | \ | |/ __ \| \ | |
    | |       /  \  |  \| |  \| | |  | |  \| |
    | |      / /\ \ | . ` | . ` | |  | | . ` |
    | |____ / ____ \| |\  | |\  | |__| | |\  |
     \_____/_/    \_\_| \_|_| \_|\____/|_| \_|
                                                
\--- The High-Velocity Load Tester \---

**Cannon** is a lightweight, high-velocity load testing tool written in **Rust**. Built for performance-critical APIs, it leverages the Tokio async runtime to provide sub-millisecond precision and extreme concurrency with a minimal resource footprint.

## **✨ Key Features**

* **⚡ Blazing Fast:** Engineered in Rust with zero garbage collection pauses. The bottleneck will be your server, not your testing tool.  
* **🎯 Constant RPS Mode:** Built-in metronome logic to fire requests at an exact rate (Requests Per Second) for baseline stability testing.  
* **🧬 Dynamic Payloads:** Real-time data generation engine. Use placeholders like {{user}} and {{number}} to simulate unique users and prevent database collisions.  
* **📊 Real-time Insights:** Interactive CLI with a live RPS counter, progress tracking, and an **ASCII Latency Distribution Histogram**.  
* **🛠️ Full REST Support:** Handles GET, POST, PUT, PATCH, and DELETE with custom headers and dynamic JSON bodies.  
* **📦 Zero-Dependency Binary:** Single static binary. Download and run on Linux, macOS, or Windows without installing any runtime.

## **📦 Installation**

### **Pre-built Binaries (Recommended)**

Download the latest binary for your architecture from the **Releases** page.

## To install globally on Linux:

### install directly via cURL

```bash
curl -sSL https://raw.githubusercontent.com/FelipeFelipeRenan/cannon-project/main/install.sh | sh
```

### Example for Linux

```bash
chmod +x cannon-linux-x64  
sudo mv cannon-linux-x64 /usr/local/bin/cannon
```

## **🛠️ CLI Arguments**

Use these flags to configure your load test.

| Flag | Long Flag | Description | Default |
| :---- | :---- | :---- | :---- |
| \-u | \--url | **(Required)** The target endpoint URL (e.g., http://localhost:8080/api). | \- |
| \-c | \--count | Total number of requests to be fired during the test. | 1 |
| \-w | \--workers | Number of concurrent workers (simultaneous connections). | 10 |
| \-r | \--rps | Limit the throughput to a specific Requests Per Second (Constant Load). | None |
| \-X | \--method | HTTP method to use: GET, POST, PUT, PATCH, DELETE. | GET |
| \-b | \--body | JSON payload for the request. Often used with POST/PATCH. | None |
| \-o | \--output | Path to save the detailed execution report in .json format. | None |
| \-t | \--timeout | Timeout to cancel a long request. | None |
| \-H | \--header | Custom header (e.g., {"Authorization: Bearer token"}). | None |

## **🧬 Dynamic Payload Tags**

When using the \--body flag, you can inject dynamic data into your JSON to ensure unique requests and bypass database constraints.

| Tag | Replacement Logic | Example Usage |  
| :---- | :---- | :---- |  
| {{user}} | Generates a random 8-character alphanumeric string. | "username": "user\_{{user}}" |
| {{email}} | Generates a random user email. | "email": {{email}} |
| {{random}} | Alias for {{user}}. Generates a random alphanumeric string. | "id": "req\_{{random}}" |  
| {{number}} | Generates a random integer between 10 and 9999\. | "amount": {{number}} |

## **🚀 Quick Start**

### **1\. Simple Stress Test (GET)**

Fire 5,000 requests with 20 concurrent workers:

```bash
cannon -u http://localhost:8081/api/v1/accounts -c 5000 -w 20
```

### **2\. Stable Load Simulation (RPS \+ Dynamic POST)**

Create 1,000 unique accounts at a steady rate of 100 requests per second:

```bash
cannon -u http://localhost:8081/api/v1/accounts \  
    -c 1000 \  
    -w 10 \  
    -X POST \  
    --rps 100 \  
    --body '{"clientId": "user\_{{user}}\_test", "currency": "BRL"}'
```

## **🔍 Understanding the Report**

At the end of each run, Cannon provides a surgical breakdown of your API's health:

* **Latency Percentiles:** p50, p95, and p99. Essential for identifying "tail latency" that averages might hide.  
* **Cannon Efficiency:** Comparison between **Target RPS** and **Actual RPS** to validate test integrity.  
* **ASCII Histogram:** A visual representation of latency distribution directly in your terminal.

## **🛠️ Technical Architecture**

Cannon is built on modern distributed systems principles:

* **Producer-Consumer Pattern:** The request engine (Producer) and the metrics aggregator (Consumer) communicate via **MPSC Channels** to eliminate memory contention and ensure thread safety.  
* **Backpressure:** Concurrency is strictly managed via **Semaphores** to prevent OS socket exhaustion and memory spikes.  
* **High-Precision Metrics:** Uses HdrHistogram to record latencies in microseconds, avoiding the "coordinated omission" problem common in many legacy load testers.

## **📄 License**

Distributed under the MIT License. See LICENSE for more information.

### **👨‍💻 Author**

**Felipe** – [LinkedIn](https://linkedin.com/in/felipefernandesss) | [GitHub](https://www.google.com/search?q=https://github.com/FelipeFelipeRenan)

*"Robust systems require merciless testing."*
