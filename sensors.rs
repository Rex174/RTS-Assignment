use std::{
    sync::{atomic::Ordering, Arc},
    thread,
    time::{Duration, Instant},
};
use crate::{
    buffer::{BoundedBuffer, PushResult},
    fault::FaultFlags,
    metrics::MetricsLogger,
    safety::SafetySystem,
    util::{SensorData, ShutdownFlag},
};

// Jitter budget for the critical Thermal sensor (1 ms expressed in µs)
const THERMAL_JITTER_BUDGET_US: f64 = 1000.0;

// ─────────────────────────────────────────────────────────────────────────────
// Sensor thread — one spawned per sensor
// ─────────────────────────────────────────────────────────────────────────────

pub fn start_sensor(
    name:        &'static str,
    priority:    u8,
    period:      Duration,
    buffer:      Arc<BoundedBuffer>,
    metrics:     Arc<MetricsLogger>,
    safety:      Arc<SafetySystem>,
    fault_flags: Arc<FaultFlags>,
    shutdown:    Arc<ShutdownFlag>,
) {
    thread::spawn(move || {
        let mut next_release      = Instant::now() + period;
        let mut prev_actual:
            Option<Instant>       = None;
        let mut cycle:       u64  = 0;
        let period_us              = period.as_micros() as f64;

        // ── Consecutive-miss tracking for the Thermal safety alert ────────
        // Records the last cycle where Thermal was buffered with valid data.
        // When the gap since the last successful cycle reaches 3, the safety
        // system is notified via report_thermal_miss() for each missed cycle.
        let mut thermal_last_ok: u64 = 0;

        loop {
            // ── Shutdown check ────────────────────────────────────────────
            if shutdown.is_set() {
                break;
            }

            // ── Sleep until next scheduled release ────────────────────────
            let now = Instant::now();
            if next_release > now {
                let nap = next_release
                    .duration_since(now)
                    .min(Duration::from_millis(5));
                thread::sleep(nap);
                if shutdown.is_set() {
                    break;
                }
            }

            // ── Fault: simulated sensor delay ─────────────────────────────
            if fault_flags.delay_sensors.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(20));
            }

            let actual   = Instant::now();
            let drift_us = actual
                .saturating_duration_since(next_release)
                .as_micros() as f64;
            cycle += 1;

            // ── Jitter (deviation from expected period) ───────────────────
            let jitter_us: Option<f64> = prev_actual.map(|prev| {
                let interval_us = actual.duration_since(prev).as_micros() as f64;
                (interval_us - period_us).abs()
            });
            prev_actual = Some(actual);

            // Warn if Thermal jitter exceeds 1 ms budget
            if name == "Thermal" {
                let j = jitter_us.unwrap_or(0.0);
                if j > THERMAL_JITTER_BUDGET_US {
                    // Bug fix: pass j (actual jitter), not drift_us
                    metrics.sensor_jitter_warn(name, cycle, j, THERMAL_JITTER_BUDGET_US);
                }
                if let Some(j_val) = jitter_us {
                    metrics.update_critical_jitter(
                        Duration::from_micros(j_val as u64)
                    );
                }
            }

            // ── Sensor value (simulated) ──────────────────────────────────
            let mut value: f64 = match name {
                "Thermal" => 20.0 + rand::random::<f64>() * 80.0,
                "Power"   => 3.3  + rand::random::<f64>() * 1.7,
                _         => rand::random::<f64>() * 100.0,
            };
            if fault_flags.corrupt_data.load(Ordering::SeqCst) && name == "Thermal" {
                value = f64::NAN;
            }

            metrics.sensor_read(name, cycle, value, drift_us);

            // ── Buffer push with latency measurement ──────────────────────
            let pre_push = Instant::now();
            let data = SensorData { name, timestamp: actual, value, priority };

            let lat_us = match buffer.push(data) {

                // ── Accepted: item entered the buffer ─────────────────────
                PushResult::Accepted => {
                    let l   = pre_push.elapsed().as_micros() as f64;
                    let len = buffer.len();
                    let cap = buffer.capacity();
                    metrics.sensor_buffered(name, cycle, l, len, cap);

                    if name == "Thermal" {
                        if value.is_nan() {
                            // Corrupted value counts as a miss — the slot was
                            // used but the data is invalid, so we do NOT reset
                            // the ok-counter and instead report the miss.
                            safety.report_thermal_miss(&metrics);
                        } else {
                            // Check whether any cycles were skipped since the
                            // last valid Thermal buffer insertion.
                            // thermal_last_ok == 0 means this is the very first
                            // cycle, so skip the gap check on startup.
                            if thermal_last_ok > 0 {
                                let missed = cycle.saturating_sub(thermal_last_ok + 1);
                                // Fire one report_thermal_miss per missed cycle
                                // so the counter inside SafetySystem increments
                                // correctly and the alert fires at exactly 3.
                                for _ in 0..missed {
                                    safety.report_thermal_miss(&metrics);
                                }
                            }
                            thermal_last_ok = cycle;
                            safety.report_thermal_ok();
                        }
                    }
                    l
                }

                // ── Evicted: incoming accepted, lower-priority item removed ──
                PushResult::Evicted(ev) => {
                    let l = pre_push.elapsed().as_micros() as f64;
                    metrics.sensor_drop(
                        ev.name,
                        ev.timestamp.elapsed().as_secs_f64() * 1000.0,
                    );

                    if ev.name == "Thermal" {
                        // A Thermal item was evicted from the buffer — counts
                        // as a miss for that earlier reading.
                        safety.report_thermal_miss(&metrics);
                    }

                    if name == "Thermal" {
                        // The incoming Thermal reading did make it in.
                        if value.is_nan() {
                            safety.report_thermal_miss(&metrics);
                        } else {
                            if thermal_last_ok > 0 {
                                let missed = cycle.saturating_sub(thermal_last_ok + 1);
                                for _ in 0..missed {
                                    safety.report_thermal_miss(&metrics);
                                }
                            }
                            thermal_last_ok = cycle;
                            safety.report_thermal_ok();
                        }
                    }
                    l
                }

                // ── Dropped: incoming rejected (lowest priority) ───────────
                PushResult::Dropped(d) => {
                    let l = pre_push.elapsed().as_micros() as f64;
                    metrics.sensor_drop(
                        d.name,
                        d.timestamp.elapsed().as_secs_f64() * 1000.0,
                    );

                    // Thermal has priority 0 (highest), so it should never
                    // reach this arm in normal operation. If it does, it is a
                    // genuine miss and must be counted.
                    if name == "Thermal" {
                        safety.report_thermal_miss(&metrics);
                        // Do NOT update thermal_last_ok — the data never entered.
                    }
                    l
                }
            };

            metrics.update_sensor_stats(name, drift_us, jitter_us, lat_us);
            next_release += period;
        }
    });
}