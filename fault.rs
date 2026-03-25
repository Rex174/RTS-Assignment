use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use crate::{metrics::MetricsLogger, util::ShutdownFlag};

// ─────────────────────────────────────────────────────────────────────────────
// Shared atomic fault flags — read by sensor threads every cycle
// ─────────────────────────────────────────────────────────────────────────────

pub struct FaultFlags {
    /// When true, every sensor thread adds an extra 20 ms delay.
    pub delay_sensors: AtomicBool,
    /// When true, the Thermal sensor produces a NaN (corrupted) value.
    pub corrupt_data:  AtomicBool,
}

impl FaultFlags {
    pub fn new() -> Self {
        Self {
            delay_sensors: AtomicBool::new(false),
            corrupt_data:  AtomicBool::new(false),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fault injector thread — fires two faults every 60 seconds
// ─────────────────────────────────────────────────────────────────────────────

pub fn start_fault_injector(
    metrics:  Arc<MetricsLogger>,
    flags:    Arc<FaultFlags>,
    shutdown: Arc<ShutdownFlag>,
) {
    metrics.bench_init();

    thread::spawn(move || {
        let mut idx: usize = 0;

        loop {
            // Wait 60 s in short steps so shutdown is detected quickly
            let mut remaining = Duration::from_secs(60);
            while remaining > Duration::ZERO {
                if shutdown.is_set() {
                    return;
                }
                let step = remaining.min(Duration::from_millis(50));
                thread::sleep(step);
                remaining = remaining.saturating_sub(step);
            }
            if shutdown.is_set() {
                return;
            }

            // ── Fault 1: Sensor delay ─────────────────────────────────────
            idx += 1;
            metrics.fault_inject(idx, "DELAYED_SENSOR", "THERMAL");
            flags.delay_sensors.store(true, Ordering::SeqCst);
            let t0 = Instant::now();
            thread::sleep(Duration::from_millis(50));
            flags.delay_sensors.store(false, Ordering::SeqCst);
            metrics.fault_recovery(idx, "DELAYED_SENSOR", "THERMAL", t0.elapsed());

            if shutdown.is_set() {
                return;
            }
            thread::sleep(Duration::from_millis(500));

            // ── Fault 2: Data corruption ──────────────────────────────────
            idx += 1;
            // FIX: Changed "POWER" to "THERMAL" — the corrupt_data flag
            // only affects the Thermal sensor in sensors.rs, so the log
            // must reflect the actual target sensor.
            metrics.fault_inject(idx, "CORRUPTED_DATA", "THERMAL");
            flags.corrupt_data.store(true, Ordering::SeqCst);
            let t0 = Instant::now();
            thread::sleep(Duration::from_millis(30));
            flags.corrupt_data.store(false, Ordering::SeqCst);
            metrics.fault_recovery(idx, "CORRUPTED_DATA", "THERMAL", t0.elapsed());
        }
    });
}