use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use crate::{metrics::MetricsLogger, util::ShutdownFlag};


// Shared atomic fault / mode flags


pub struct FaultFlags {
    // When true, every sensor thread adds an extra 20 ms delay.
    pub delay_sensors: AtomicBool,
    // When true, the Thermal sensor produces a NaN (corrupted) value.
    pub corrupt_data:  AtomicBool,
    // Manual safe mode entered via GCS uplink command.
    pub manual_safe_mode: AtomicBool,
    // Set true when GCS acknowledges a fault.
    pub fault_acknowledged: AtomicBool,
}

impl FaultFlags {
    pub fn new() -> Self {
        Self {
            delay_sensors: AtomicBool::new(false),
            corrupt_data:  AtomicBool::new(false),
            manual_safe_mode: AtomicBool::new(false),
            fault_acknowledged: AtomicBool::new(false),
        }
    }

    pub fn any_fault_active(&self) -> bool {
        self.delay_sensors.load(Ordering::SeqCst)
            || self.corrupt_data.load(Ordering::SeqCst)
    }
}


// Fault injector thread (fire two faults every 60 seconds)


pub fn start_fault_injector(
    metrics:  Arc<MetricsLogger>,
    flags:    Arc<FaultFlags>,
    shutdown: Arc<ShutdownFlag>,
) {
    metrics.bench_init();

    thread::spawn(move || {
        let mut idx: usize = 0;

        loop {
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

            flags.fault_acknowledged.store(false, Ordering::SeqCst);

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

            flags.fault_acknowledged.store(false, Ordering::SeqCst);

            idx += 1;
            metrics.fault_inject(idx, "CORRUPTED_DATA", "THERMAL");
            flags.corrupt_data.store(true, Ordering::SeqCst);
            let t0 = Instant::now();
            thread::sleep(Duration::from_millis(30));
            flags.corrupt_data.store(false, Ordering::SeqCst);
            metrics.fault_recovery(idx, "CORRUPTED_DATA", "THERMAL", t0.elapsed());
        }
    });
}
