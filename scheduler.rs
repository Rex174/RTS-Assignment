use std::{
    sync::{
        atomic::{AtomicI32, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use crate::{buffer::BoundedBuffer, metrics::MetricsLogger, util::ShutdownFlag};

static RUNNING_PRIORITY: AtomicI32 = AtomicI32::new(-1);

// ─────────────────────────────────────────────────────────────────────────────
// Task definition
// ─────────────────────────────────────────────────────────────────────────────

struct Task {
    name:     &'static str,
    priority: i32,
    period:   Duration,
    wcet_ms:  u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// RMS scheduler — spawns one thread per task
// ─────────────────────────────────────────────────────────────────────────────

pub fn start_scheduler(
    buffer:   Arc<BoundedBuffer>,
    metrics:  Arc<MetricsLogger>,
    shutdown: Arc<ShutdownFlag>,
) {
    // Rate Monotonic: shorter period → higher priority (lower number).
    let tasks = [
        Task { name: "THERMAL_CTRL",  priority: 0, period: Duration::from_millis(200), wcet_ms: 20 },
        Task { name: "DATA_COMPRESS", priority: 1, period: Duration::from_millis(300), wcet_ms: 30 },
        Task { name: "HEALTH_MON",    priority: 2, period: Duration::from_millis(500), wcet_ms: 25 },
        Task { name: "ANTENNA_ALIGN", priority: 3, period: Duration::from_millis(1000),wcet_ms: 15 },
    ];

    for task in tasks {
        let metrics  = metrics.clone();
        let buffer   = buffer.clone();
        let shutdown = shutdown.clone();

        // Log the task's configuration before spawning
        metrics.sched_init(
            task.name,
            task.period.as_millis() as u64,
            task.wcet_ms,
            task.priority as u8,
        );

        thread::spawn(move || {
            let mut next_release = Instant::now() + task.period;
            let mut job: u64 = 0;

            loop {
                // ── Shutdown check ────────────────────────────────────────
                if shutdown.is_set() {
                    break;
                }

                // ── Sleep until next period boundary ──────────────────────
                let now = Instant::now();
                if next_release > now {
                    let nap = next_release
                        .duration_since(now)
                        .min(Duration::from_millis(10));
                    thread::sleep(nap);
                    if shutdown.is_set() {
                        break;
                    }
                }

                let actual_start = Instant::now();
                let drift_us = actual_start
                    .saturating_duration_since(next_release)
                    .as_micros() as f64;
                job += 1;
                metrics.sched_job();

                // START_LATE violation — drift > 500 µs is flagged
                if drift_us > 500.0 {
                    metrics.sched_violation(
                        task.name, job, "START_LATE", drift_us,
                    );
                }

                // ── Preemption check ──────────────────────────────────────
                let running = RUNNING_PRIORITY.load(Ordering::SeqCst);
                if running != -1 && running < task.priority {
                    metrics.sched_preempt(task.name, "higher-priority task");
                    next_release += task.period;
                    continue;
                }

                // ── Claim the CPU slot ────────────────────────────────────
                if RUNNING_PRIORITY
                    .compare_exchange(
                        -1, task.priority, Ordering::SeqCst, Ordering::SeqCst,
                    )
                    .is_err()
                {
                    next_release += task.period;
                    continue;
                }

                // ── Execute task body ─────────────────────────────────────
                let exec_start = Instant::now();
                metrics.sched_run(task.name, buffer.fill_ratio() * 100.0);
                run_task_body(task.name, &buffer, &metrics);
                let exec_ns = exec_start.elapsed().as_nanos() as u64;

                // Release CPU slot
                RUNNING_PRIORITY.store(-1, Ordering::SeqCst);
                metrics.log_cpu(exec_ns, task.period.as_nanos() as u64);

                // FINISH_LATE violation — total elapsed exceeds period
                let total_us = actual_start.elapsed().as_micros() as f64;
                let period_us = task.period.as_micros() as f64;
                if total_us > period_us {
                    metrics.sched_violation(
                        task.name, job, "FINISH_LATE", total_us - period_us,
                    );
                }

                next_release += task.period;
            }
        });
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Simulated task bodies — realistic sleep durations
// ─────────────────────────────────────────────────────────────────────────────

fn run_task_body(
    name:    &str,
    buffer:  &Arc<BoundedBuffer>,
    metrics: &Arc<MetricsLogger>,
) {
    match name {
        "THERMAL_CTRL" => {
            if buffer.fill_ratio() > 0.8 {
                metrics.downlink_miss("THERMAL_CTRL: buffer critical", 0.0);
            }
            thread::sleep(Duration::from_millis(5));
        }
        "DATA_COMPRESS" => {
            thread::sleep(Duration::from_millis(10));
        }
        "HEALTH_MON" => {
            thread::sleep(Duration::from_millis(15));
        }
        "ANTENNA_ALIGN" => {
            thread::sleep(Duration::from_millis(10));
        }
        _ => {}
    }
}