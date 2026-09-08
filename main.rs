use std::sync::Arc;
use std::time::Duration;

mod buffer;
mod downlink;
mod fault;
mod metrics;
mod network;
mod safety;
mod scheduler;
mod sensors;
mod uplink;
mod util;

use buffer::BoundedBuffer;
use fault::FaultFlags;
use metrics::MetricsLogger;
use safety::SafetySystem;
use uplink::RecentPacketStore;
use util::ShutdownFlag;

const GCS_TCP:        &str   = "0.0.0.0:9000";
const GCS_UDP:        &str   = "0.0.0.0:9001 → 127.0.0.1:9002";
const OCS_BIND_ADDR:  &str   = "0.0.0.0:9001";
const GCS_UDP_ADDR:   &str   = "127.0.0.1:9002";
const SIM_SECS:       u64    = 300;
const BUF_CAP:        usize  = 32;
const RECENT_PACKET_CAP: usize = 256;

fn main() {
    let metrics  = Arc::new(MetricsLogger::new("performance_log.txt", "fault_log.txt"));
    let buffer   = Arc::new(BoundedBuffer::new(BUF_CAP));
    let safety   = Arc::new(SafetySystem::new());
    let faults   = Arc::new(FaultFlags::new());
    let shutdown = Arc::new(ShutdownFlag::new());
    let recent_packets = Arc::new(RecentPacketStore::new(RECENT_PACKET_CAP));

    // Sensors
    metrics.sensor_init("Thermal", 10, 0);
    sensors::start_sensor("Thermal", 0, Duration::from_millis(10),
                          buffer.clone(), metrics.clone(), safety.clone(), faults.clone(), shutdown.clone());

    metrics.sensor_init("Power", 50, 1);
    sensors::start_sensor("Power", 1, Duration::from_millis(50),
                          buffer.clone(), metrics.clone(), safety.clone(), faults.clone(), shutdown.clone());

    metrics.sensor_init("Payload", 100, 2);
    sensors::start_sensor("Payload", 2, Duration::from_millis(100),
                          buffer.clone(), metrics.clone(), safety.clone(), faults.clone(), shutdown.clone());

    // Scheduler
    scheduler::start_scheduler(buffer.clone(), metrics.clone(), shutdown.clone());

    // Downlink
    downlink::start_downlink(
        buffer.clone(),
        metrics.clone(),
        shutdown.clone(),
        faults.clone(),
        recent_packets.clone(),
        OCS_BIND_ADDR,
        GCS_UDP_ADDR,
    );

    // Uplink command server
    uplink::start_uplink_server(
        GCS_TCP,
        GCS_UDP_ADDR,
        metrics.clone(),
        faults.clone(),
        shutdown.clone(),
        recent_packets.clone(),
    );

    // Fault injector
    fault::start_fault_injector(metrics.clone(), faults.clone(), shutdown.clone());

    std::thread::sleep(Duration::from_secs(SIM_SECS));
    shutdown.signal();
    std::thread::sleep(Duration::from_millis(150));

    println!();
    println!("┌─────────────────────────────────────────────────────────┐");
    println!("│    Satellite OCS — Real-Time Simulation (Tasks 1–4)     │");
    println!("├─────────────────────────────────────────────────────────┤");
    println!("│  Sensors  : Thermal (10ms) | Power (50ms) |             │");
    println!("│             Payload (100ms)                             │");
    println!("│  Tasks    : ThermalCtrl  | DataCompress |               │");
    println!("│             HealthMon    | AntennaAlign                 │");
    println!("│  Downlink : UDP :9001 → :9002 (telemetry to GCS)        │");
    println!("│  Uplink   : TCP :9000 (active: GCS commands enabled)    │");
    println!("│  Faults   : injected every 60s | limit 200ms            │");
    println!("│  Buffer   : capacity = {}                               │", BUF_CAP);
    println!("│  Resend   : recent packet cache = {}                    │", RECENT_PACKET_CAP);
    println!("│  Duration : {} seconds                                  │", SIM_SECS);
    println!("└─────────────────────────────────────────────────────────┘");
    println!();

    println!("[MAIN] Simulation complete.");
    println!("[MAIN] Student B GCS : TCP {}  |  UDP {}", GCS_TCP, GCS_UDP);
    println!("[MAIN] Fault log      : fault_log.txt");
    println!("[MAIN] Performance log: performance_log.txt");
    metrics.final_report(GCS_TCP, GCS_UDP);
    println!("[MAIN] Simulation ended.");
}
