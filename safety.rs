use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use crate::metrics::MetricsLogger;

// ─────────────────────────────────────────────────────────────────────────────
// Safety system — tracks consecutive Thermal sensor misses
// ─────────────────────────────────────────────────────────────────────────────

pub struct SafetySystem {
    missed_thermal: AtomicUsize,
}

impl SafetySystem {
    pub fn new() -> Self {
        Self { missed_thermal: AtomicUsize::new(0) }
    }

    /// Call whenever a Thermal sample is successfully buffered.
    /// Resets the consecutive-miss counter.
    pub fn report_thermal_ok(&self) {
        self.missed_thermal.store(0, Ordering::SeqCst);
    }

    /// Call whenever a Thermal sample is dropped or evicted.
    /// Raises a safety alert after 3 consecutive misses.
    pub fn report_thermal_miss(&self, metrics: &Arc<MetricsLogger>) {
        let misses = self.missed_thermal.fetch_add(1, Ordering::SeqCst) + 1;
        if misses >= 3 {
            metrics.log_safety(&format!(
                "Thermal data missed {} consecutive cycles — SAFETY ALERT RAISED",
                misses
            ));
        } else {
            println!(
                "[SAFETY][WARN] Thermal miss #{} (alert triggers at 3 consecutive)",
                misses
            );
        }
    }
}