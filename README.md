# RTS Assignment — Satellite Onboard Control System (OCS)

**Module:** CT087-3-3 Real-Time Systems ||
**Institution:** Asia Pacific University of Technology & Innovation (APU) ||
**Language:** Rust (edition 2021) ||
**Role:** Student A — Satellite Onboard Control System (OCS)

---

## Overview

This repository contains a real-time satellite control simulation written in Rust. The assignment models a two-part satellite communication system:

| Role | Subsystem | Responsibility |
|---|---|---|
| **Student A (this repo)** | Satellite Onboard Control System (OCS) | Sensor acquisition, task scheduling, downlink transmission, fault injection |
| Student B | Ground Control Station (GCS) | Receives OCS telemetry, monitors link health, issues uplink commands |

The two subsystems communicate over UDP using a fixed 27-byte binary telemetry packet format. This repository implements the **OCS side only**.

---

## My Responsibility — Student A (OCS)

I was responsible for the complete onboard satellite subsystem, covering four required task areas:

### Task 1 — Sensor Data Acquisition and Prioritisation
- Three concurrent sensor threads at distinct periods and priorities:
  - **Thermal** — 10ms period, priority 0 (highest, safety-critical)
  - **Power** — 50ms period, priority 1
  - **Payload** — 100ms period, priority 2 (lowest)
- Jitter kept **below 1ms** for the critical Thermal sensor using a hybrid sleep + spin-wait timing strategy
- Priority-aware bounded buffer (32 slots) that evicts the lowest-priority item when full
- Every dropped sample logged with an age timestamp
- Scheduling drift measured as expected vs. actual thread release time
- Read-to-insert latency tracked per cycle
- Autonomous safety alert raised after **3 or more consecutive** Thermal misses

### Task 2 — Real-Time Task Scheduling
- **Rate Monotonic Scheduling (RMS)** across four periodic control tasks:

  | Task | Period | WCET | Priority |
  |---|---|---|---|
  | `THERMAL_CTRL` | 200ms | 20ms | 0 (highest) |
  | `DATA_COMPRESS` | 300ms | 30ms | 1 |
  | `HEALTH_MON` | 500ms | 25ms | 2 |
  | `ANTENNA_ALIGN` | 1000ms | 15ms | 3 (lowest) |

- Preemption enforced via a shared `AtomicI32` priority register
- Deadline violations logged for both `START_LATE` and `FINISH_LATE` conditions
- CPU utilisation measured as % active vs. % idle

### Task 3 — Downlink Data Management
- Gzip compression and binary packetisation of all buffered telemetry
- **27-byte fixed wire format** with magic bytes, sequence number, timestamp, sensor ID, value, flags, and XOR checksum
- UDP transmission (fire-and-forget) from `0.0.0.0:9001` to the GCS at `127.0.0.1:9002`
- 30ms visibility window deadline monitoring
- 5ms downlink initialisation deadline monitoring
- Degraded mode triggered automatically when buffer fill exceeds 80%
- Queue latency, buffer fill rate, and packet throughput tracked

### Task 4 — Benchmarking and Fault Simulation
- Two fault types injected every 60 seconds:
  - `DELAYED_SENSOR` — adds 20ms artificial delay to all sensor threads for 50ms
  - `CORRUPTED_DATA` — injects `NaN` values into the Thermal sensor for 30ms
- Recovery time measured with a monotonic clock for every fault
- Mission abort declared if any recovery exceeds **200ms**
- Full structured benchmarking report generated at simulation end

---

## Module Structure

```
src/
├── main.rs        Entry point — wires all modules, owns shared Arc handles
├── util.rs        Core types: SensorData, RunningStats, Aggregates, ShutdownFlag
├── metrics.rs     MetricsLogger — central logging, statistics, final report
├── sensors.rs     Three sensor threads with drift/jitter/latency measurement
├── buffer.rs      BoundedBuffer — priority-aware bounded queue with eviction
├── safety.rs      SafetySystem — consecutive Thermal miss detection
├── scheduler.rs   RMS scheduler — four task threads with preemption
├── downlink.rs    Downlink thread — compression, packetisation, UDP send
├── fault.rs       Fault injector thread — periodic fault injection
└── network.rs     OcsTelemetryPacket wire format + UdpSender
```

---

## Dependencies

```toml
[dependencies]
rand   = "0.8"    # simulated sensor value generation
flate2 = "1.0"    # gzip compression for downlink packets
```

No async runtime is used. All concurrency is built on `std::thread`, `Arc<Mutex<T>>`, and atomic primitives, keeping timing behaviour fully deterministic and analysable.

---

## Build and Run

```bash
# Build in release mode (recommended — debug builds affect timing accuracy)
cargo build --release

# Run the OCS simulation (300 seconds)
cargo run --release
```

### Configuration

Key constants are defined at the top of `src/main.rs`:

```rust
const OCS_BIND_ADDR: &str  = "0.0.0.0:9001";     // OCS sends from this port
const GCS_UDP_ADDR:  &str  = "127.0.0.1:9002";   // GCS listens here
const SIM_SECS:      u64   = 300;                // simulation duration
const BUF_CAP:       usize = 32;                 // bounded buffer capacity
```

To run against a GCS on a different machine, change `GCS_UDP_ADDR` to that machine's IP address.

---

## Telemetry Packet Format

Both OCS and GCS must agree on this 27-byte layout:

| Offset | Length | Field |
|---|---|---|
| 0 | 2 | Magic bytes `0x4F 0x43` ("OC") |
| 2 | 4 | Sequence number (u32, big-endian) |
| 6 | 8 | Timestamp µs since OCS start (u64, big-endian) |
| 14 | 1 | Sensor ID — 0=Thermal, 1=Power, 2=Payload |
| 15 | 8 | Sensor value (f64, big-endian IEEE-754) |
| 23 | 2 | Original payload size in bytes (u16, big-endian) |
| 25 | 1 | Flags — bit0=degraded, bit1=safety_alert, bit2=nan |
| 26 | 1 | XOR checksum of bytes 0–25 |

A fixed-width binary format was chosen over JSON to guarantee deterministic parse time, which is a requirement for hard real-time communication.

---

## Output Files

Two log files are generated in the project root after each run:

| File | Contents |
|---|---|
| `performance_log.txt` | Complete final structured report — all sensor, scheduler, downlink, and benchmarking metrics |
| `fault_log.txt` | Every fault injection, recovery event, jitter warning, data drop, and safety alert |

---

## Results Summary (300-second run)

| Metric | Target | Result | Status |
|---|---|---|---|
| Thermal jitter (avg) | < 1ms | **0.175ms** | Pass |
| Thermal data dropped | 0 | **0** | Pass |
| Safety alerts raised | Active | **16** | Pass |
| Total scheduler jobs | — | **88,887** | — |
| Deadline violations | Minimal | **2** (100.0% adherence) | Pass |
| CPU utilisation | < 69.3% (RMS bound) | **2.47% active** | Pass |
| Packets transmitted | — | **94,933** | — |
| Faults injected | 8 (every 60s) | **8** | Pass |
| Fault recovery (avg) | < 200ms | **40.2ms** | Pass |
| Mission aborted | No | **No** | Pass |

**Overall verdict:** `MISSION STATUS: NOMINAL`

---

## Design Notes

**Why UDP and not TCP for telemetry?**
TCP's retransmission and head-of-line blocking introduce unpredictable latency spikes. In a hard real-time system, a retransmitted packet arriving hundreds of milliseconds late is operationally worthless. UDP's fire-and-forget model matches the requirement, and packet loss is detected at the application layer through sequence number gaps.

**Why a hybrid sleep + spin-wait for the Thermal sensor?**
Windows `thread::sleep` has approximately 1ms timer granularity, which alone makes the sub-1ms jitter requirement unachievable. The Thermal thread sleeps until 1.5ms before its scheduled release, then spin-waits with `std::hint::spin_loop()` for the remainder, achieving sub-200µs precision at the cost of a small amount of CPU.

**Why Rust?**
Deterministic memory management with no garbage collector pauses, compile-time data race prevention through the ownership model, and zero-cost abstractions — all prerequisites for a system where timing analysis must hold.

---

## Ramaneiss 

**Student A — Satellite Onboard Control System (OCS)**
CT087-3-3 Real-Time Systems, Asia Pacific University
