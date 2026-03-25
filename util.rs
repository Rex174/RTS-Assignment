
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, Ordering};

// ─────────────────────────────────────────────────────────────────────────────
// Core data types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct SensorData {
    pub name:      &'static str,
    pub timestamp: Instant,
    pub value:     f64,
    /// 0 = highest priority. Thermal=0, Power=1, Payload=2.
    pub priority:  u8,
}

// ─────────────────────────────────────────────────────────────────────────────
// Online statistics — mean, max, σ without storing all samples
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct RunningStats {
    pub count:  u64,
    pub sum:    f64,
    pub sum_sq: f64,
    pub max:    f64,
}

impl RunningStats {
    pub fn push(&mut self, v: f64) {
        self.count  += 1;
        self.sum    += v;
        self.sum_sq += v * v;
        if v > self.max {
            self.max = v;
        }
    }

    pub fn mean(&self) -> f64 {
        if self.count == 0 { 0.0 } else { self.sum / self.count as f64 }
    }

    pub fn sigma(&self) -> f64 {
        if self.count < 2 {
            return 0.0;
        }
        let m = self.mean();
        ((self.sum_sq / self.count as f64) - m * m).max(0.0).sqrt()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-sensor aggregated performance data
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct SensorPerfStats {
    pub drift:   RunningStats,
    pub jitter:  RunningStats,
    pub latency: RunningStats,
    pub cycles:  u64,
    pub dropped: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Fault record — one entry per injected fault
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct FaultRecord {
    pub index:       usize,
    pub kind:        &'static str,
    pub sensor:      &'static str,
    pub recovery_ms: u64,
    pub passed:      bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Global aggregates used by MetricsLogger
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct Aggregates {
    // Jitter tracking for critical (Thermal) sensor
    pub critical_jitter_sum:   Duration,
    pub critical_jitter_count: u64,

    // Deadline and fault counters
    pub deadline_misses:  u64,
    pub fault_injections: u64,
    pub safety_alerts:    u64,
    pub data_loss:        u64,

    // Recovery timing
    pub recovery_sum:   Duration,
    pub recovery_count: u64,

    // CPU utilisation
    pub cpu_active_ns: u64,
    pub cpu_total_ns:  u64,

    // Per-sensor performance
    pub thermal: SensorPerfStats,
    pub power:   SensorPerfStats,
    pub payload: SensorPerfStats,

    // Scheduler job tracking
    pub jobs_total:    u64,
    pub jobs_violated: u64,

    // Fault history (one entry per injected fault)
    pub fault_records: Vec<FaultRecord>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared shutdown flag — set by main(), checked by all worker threads
// ─────────────────────────────────────────────────────────────────────────────

pub struct ShutdownFlag(AtomicBool);

impl ShutdownFlag {
    pub fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    /// Called once by main() when the simulation time is up.
    pub fn signal(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Called at the top of every worker loop iteration.
    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}