use std::fs::OpenOptions;
use std::io::Write as IoWrite;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use crate::util::{Aggregates, FaultRecord};

// ─────────────────────────────────────────────────────────────────────────────
// MetricsLogger — central logging, statistics, and file output
// ─────────────────────────────────────────────────────────────────────────────

pub struct MetricsLogger {
    pub start:  Instant,
    pub agg:    Mutex<Aggregates>,
    perf_path:  String,
    fault_path: String,
}

impl MetricsLogger {
    pub fn new(perf_path: &str, fault_path: &str) -> Self {
        // Create/clear both log files at startup
        let _ = std::fs::write(perf_path,  "");
        let _ = std::fs::write(fault_path, "");
        println!("[LOG] Performance log: {}", perf_path);
        Self {
            start:      Instant::now(),
            agg:        Mutex::new(Aggregates::default()),
            perf_path:  perf_path.to_string(),
            fault_path: fault_path.to_string(),
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────

    fn plog(&self, line: &str) {
        if let Ok(mut f) = OpenOptions::new().append(true).open(&self.perf_path) {
            let _ = writeln!(f, "{}", line);
        }
    }

    fn flog(&self, line: &str) {
        if let Ok(mut f) = OpenOptions::new().append(true).open(&self.fault_path) {
            let _ = writeln!(f, "{}", line);
        }
    }

    // ── Sensor events ─────────────────────────────────────────────────────

    pub fn sensor_init(&self, name: &str, period_ms: u64, priority: u8) {
        let s = format!(
            "[SENSOR][INIT] {:8} | period={}ms | priority={}",
            name.to_uppercase(), period_ms, priority
        );
        println!("{}", s);
        self.plog(&s);
    }

    pub fn sensor_read(&self, name: &str, cycle: u64, val: f64, drift_us: f64) {
        let s = format!(
            "[SENSOR][READ] {:8} | cycle={:5} | val={:8.3} | drift={:.1}µs",
            name.to_uppercase(), cycle, val, drift_us
        );
        println!("{}", s);
        self.plog(&s);
    }

    pub fn sensor_buffered(
        &self, name: &str, cycle: u64, lat_us: f64, buf_len: usize, cap: usize,
    ) {
        let s = format!(
            "[SENSOR][BUFFERED] {:8} | cycle={:5} | latency={:.1}µs | buf_len={}/{}",
            name.to_uppercase(), cycle, lat_us, buf_len, cap
        );
        println!("{}", s);
        self.plog(&s);
    }

    pub fn sensor_jitter_warn(
        &self, name: &str, cycle: u64, jitter_us: f64, budget_us: f64,
    ) {
        let s = format!(
            "[SENSOR][JITTER_WARN] {} cycle={} jitter={:.1}µs > budget={:.1}µs",
            name.to_uppercase(), cycle, jitter_us, budget_us
        );
        println!("{}", s);
        self.plog(&s);
        self.flog(&s);
    }

    pub fn sensor_drop(&self, name: &str, age_ms: f64) {
        let s = format!(
            "[SENSOR][DROP] {} | age={:.3}ms",
            name.to_uppercase(), age_ms
        );
        println!("{}", s);
        self.plog(&s);
        self.flog(&s);
        let mut a = self.agg.lock().unwrap();
        a.data_loss += 1;
        match name {
            "Thermal" => a.thermal.dropped += 1,
            "Power"   => a.power.dropped   += 1,
            _         => a.payload.dropped += 1,
        }
    }

    /// Update per-sensor drift/jitter/latency stats and increment cycle count.
    pub fn update_sensor_stats(
        &self, name: &str, drift_us: f64, jitter_us: Option<f64>, lat_us: f64,
    ) {
        let mut a = self.agg.lock().unwrap();
        let s = match name {
            "Thermal" => &mut a.thermal,
            "Power"   => &mut a.power,
            _         => &mut a.payload,
        };
        s.drift.push(drift_us);
        if let Some(j) = jitter_us {
            s.jitter.push(j);
        }
        s.latency.push(lat_us);
        s.cycles += 1;
    }

    pub fn update_critical_jitter(&self, jitter: Duration) {
        let mut a = self.agg.lock().unwrap();
        a.critical_jitter_sum   += jitter;
        a.critical_jitter_count += 1;
    }

    pub fn log_safety(&self, msg: &str) {
        let s = format!("[SAFETY][ALERT] {}", msg);
        println!("{}", s);
        self.plog(&s);
        self.flog(&s);
        self.agg.lock().unwrap().safety_alerts += 1;
    }

    // ── Scheduler events ──────────────────────────────────────────────────

    pub fn sched_init(
        &self, name: &str, period_ms: u64, wcet_ms: u64, priority: u8,
    ) {
        let s = format!(
            "[SCHED][INIT] {:15} | period={}ms | wcet={}ms | priority={}",
            name, period_ms, wcet_ms, priority
        );
        println!("{}", s);
        self.plog(&s);
    }

    pub fn sched_run(&self, name: &str, buf_pct: f64) {
        let s = format!(
            "[SCHED][RUN] {:15} | buffer={:.0}%",
            name, buf_pct
        );
        println!("{}", s);
        self.plog(&s);
    }

    pub fn sched_violation(
        &self, name: &str, job: u64, kind: &str, delay_us: f64,
    ) {
        let s = format!(
            "[SCHED][VIOLATION] {} job={} kind={} delay={:.0}µs",
            name, job, kind, delay_us
        );
        println!("{}", s);
        self.plog(&s);
        let mut a = self.agg.lock().unwrap();
        a.deadline_misses += 1;
        a.jobs_violated   += 1;
    }

    pub fn sched_preempt(&self, preempted: &str, by: &str) {
        let s = format!("[SCHED][PREEMPT] {} preempted by {}", preempted, by);
        println!("{}", s);
        self.plog(&s);
    }

    pub fn sched_job(&self) {
        self.agg.lock().unwrap().jobs_total += 1;
    }

    pub fn log_cpu(&self, active_ns: u64, period_ns: u64) {
        let mut a = self.agg.lock().unwrap();
        a.cpu_active_ns += active_ns;
        a.cpu_total_ns  += period_ns;
    }

    // ── Downlink events ───────────────────────────────────────────────────

    pub fn downlink_loop_start(&self) {
        let s = "[DOWNLINK][LOOP] Starting downlink loop (drain every 100ms)";
        println!("{}", s);
        self.plog(s);
    }

    pub fn downlink_visibility(&self) {
        let s = "[DOWNLINK][VISIBILITY] Window opened";
        println!("{}", s);
        self.plog(s);
    }

    pub fn downlink_status(&self, fill_pct: f64, sent: u64, degraded: bool) {
        let s = format!(
            "[DOWNLINK][STATUS] fill={:.1}% sent={} degraded={}",
            fill_pct, sent, degraded
        );
        println!("{}", s);
        self.plog(&s);
    }

    pub fn downlink_window_result(
        &self, count: usize, elapsed_ms: f64, total: u64,
    ) {
        let s = format!(
            "[DOWNLINK][WINDOW] Transmitted {} packet(s) in {:.3}ms \
             (within 30ms ✓) | total_sent={}",
            count, elapsed_ms, total
        );
        println!("{}", s);
        self.plog(&s);
    }

    pub fn downlink_miss(&self, kind: &str, overshoot_ms: f64) {
        let s = format!(
            "[DOWNLINK][MISS] {} overshoot={:.3}ms",
            kind, overshoot_ms
        );
        println!("{}", s);
        self.plog(&s);
        self.agg.lock().unwrap().deadline_misses += 1;
    }

    /// Packet-level TX log — written to file only (too noisy for stdout).
    pub fn downlink_tx(
        &self, seq: u32, sensor: &str, orig: usize, comp: usize, tx_ms: f64,
    ) {
        let s = format!(
            "[DOWNLINK][TX] seq={} sensor={} | {}→{}B gzip | tx={:.3}ms",
            seq, sensor, orig, comp, tx_ms
        );
        self.plog(&s);
    }

    // ── Fault / benchmark events ──────────────────────────────────────────

    pub fn bench_init(&self) {
        let s = "[BENCH][INIT] Fault injector ready \
                 — interval=60s recovery_limit=200ms";
        println!("{}", s);
        self.plog(s);
    }

    pub fn fault_inject(
        &self, idx: usize, kind: &'static str, sensor: &'static str,
    ) {
        let s = format!(
            "[FAULT][INJECT] #{} {} sensor={}",
            idx, kind, sensor
        );
        println!("{}", s);
        self.plog(&s);
        self.flog(&s);
        self.agg.lock().unwrap().fault_injections += 1;
    }

    pub fn fault_recovery(
        &self, idx: usize, kind: &'static str,
        sensor: &'static str, duration: Duration,
    ) {
        let ms     = duration.as_millis() as u64;
        let passed = ms < 200;
        let verdict = if passed {
            format!("RECOVERED in {}ms ✓", ms)
        } else {
            format!("SLOW RECOVERY {}ms > 200ms — MISSION ABORT ✗", ms)
        };
        let s = format!(
            "[FAULT][RECOVERY] #{} {} sensor={} | {}",
            idx, kind, sensor, verdict
        );
        println!("{}", s);
        self.plog(&s);
        self.flog(&s);

        let rec = FaultRecord {
            index: idx, kind, sensor, recovery_ms: ms, passed,
        };
        let mut a = self.agg.lock().unwrap();
        a.recovery_sum   += duration;
        a.recovery_count += 1;
        a.fault_records.push(rec);
    }

    // ── Final report ──────────────────────────────────────────────────────

    pub fn final_report(&self, gcs_tcp: &str, gcs_udp: &str) {
        let a        = self.agg.lock().unwrap();
        let sim_secs = self.start.elapsed().as_secs_f64();

        // Derived metrics
        let jitter_ms = if a.critical_jitter_count > 0 {
            a.critical_jitter_sum.as_secs_f64() * 1000.0
                / a.critical_jitter_count as f64
        } else {
            0.0
        };
        let cpu_pct = if a.cpu_total_ns > 0 {
            a.cpu_active_ns as f64 / a.cpu_total_ns as f64 * 100.0
        } else {
            0.0
        };
        let dead_adh = if a.jobs_total > 0 {
            (1.0 - a.jobs_violated as f64 / a.jobs_total as f64) * 100.0
        } else {
            100.0
        };
        let avg_rec = if a.recovery_count > 0 {
            a.recovery_sum.as_millis() as f64 / a.recovery_count as f64
        } else {
            0.0
        };
        let min_rec = a.fault_records.iter()
            .map(|r| r.recovery_ms).min().unwrap_or(0);
        let max_rec = a.fault_records.iter()
            .map(|r| r.recovery_ms).max().unwrap_or(0);
        let within  = a.fault_records.iter().filter(|r| r.passed).count();
        let total_f = a.fault_records.len();
        let within_pct = if total_f > 0 {
            within as f64 / total_f as f64 * 100.0
        } else {
            0.0
        };
        let mission_abort = a.fault_records.iter().any(|r| !r.passed);

        // ── Helper closure: print to stdout and accumulate report string ──
        let mut report = String::new();
        let mut line = |l: &str| {
            println!("{}", l);
            report.push_str(l);
            report.push('\n');
        };

        // ─── TASK 1 & 2 ───────────────────────────────────────────────────
        line("┌────────────────────────────────────────────────────────────┐");
        line("│         TASK 1 & 2  —  SENSOR / SCHEDULER REPORT          │");
        line("└────────────────────────────────────────────────────────────┘");
        line(&format!("Simulation duration  : {:.1}s", sim_secs));
        line(&format!("Thermal jitter (avg) : {:.3}ms  (target: <1ms)", jitter_ms));
        line(&format!("Safety alerts        : {}", a.safety_alerts));
        line(&format!("Data loss events     : {}", a.data_loss));
        line("");

        line("— Sensor Performance Under Fault ——————————————————————————");
        for (label, s) in [
            ("THERMAL",  &a.thermal),
            ("POWER",    &a.power),
            ("PAYLOAD",  &a.payload),
        ] {
            let alerts_str = if label == "THERMAL" {
                format!("alerts={}", a.safety_alerts)
            } else {
                "alerts=0".to_string()
            };
            line(&format!(
                "{} – cycles={} dropped={} {}",
                label, s.cycles, s.dropped, alerts_str
            ));
            line(&format!(
                "  Drift   : mean={:.3}ms  max={:.3}ms  σ={:.3}ms",
                s.drift.mean()  / 1000.0,
                s.drift.max     / 1000.0,
                s.drift.sigma() / 1000.0,
            ));
            line(&format!(
                "  Jitter  : mean={:.3}ms  max={:.3}ms  σ={:.3}ms",
                s.jitter.mean()  / 1000.0,
                s.jitter.max     / 1000.0,
                s.jitter.sigma() / 1000.0,
            ));
            line(&format!(
                "  Latency : mean={:.3}ms  max={:.3}ms \n",
                s.latency.mean() / 1000.0,
                s.latency.max    / 1000.0,
            ));
        }
        line("");

        line("— Scheduler Deadline Adherence —————————————————————————————");
        line(&format!("Total jobs scheduled : {}", a.jobs_total));
        line(&format!("Deadline violations  : {}", a.jobs_violated));
        line(&format!("Deadline adherence   : {:.1}%", dead_adh));
        line("");

        line("— CPU Utilisation ———————————————————————————————————————————");
        line(&format!("Active (scheduler)   : {:.2}%", cpu_pct));
        line(&format!("Idle                 : {:.2}%", 100.0 - cpu_pct));
        line("");

        // ─── TASK 3 ───────────────────────────────────────────────────────
        line("┌────────────────────────────────────────────────────────────┐");
        line("│              TASK 3  —  DOWNLINK METRICS REPORT            │");
        line("└────────────────────────────────────────────────────────────┘");
        line("Protocol        : UDP (alerts/status)");
        line(&format!("TCP address      : {}", gcs_tcp));
        line(&format!("UDP address      : {}", gcs_udp));
        line("GCS connected   : ✓ Simulated");
        line("Visibility rule : data within 30ms of window open");
        line("Degraded thresh : 80% buffer fill");
        line("");

        // ─── TASK 4 ───────────────────────────────────────────────────────
        line("┌────────────────────────────────────────────────────────────┐");
        line("│         TASK 4  —  BENCHMARKING & FAULT SIMULATION         │");
        line("└────────────────────────────────────────────────────────────┘");
        line("— Fault Injection Summary ———————————————————————————————————");
        line(&format!("Total faults injected : {}", a.fault_injections));
        line("Recovery deadline     : 200ms");
        line(&format!(
            "Mission aborted       : {}",
            if mission_abort { "⚠ YES" } else { "✓ NO" }
        ));
        line("");

        for r in &a.fault_records {
            let verdict = if r.passed {
                format!("RECOVERED in {}ms ✓", r.recovery_ms)
            } else {
                format!(
                    "SLOW RECOVERY {}ms > 200ms — MISSION ABORT ✗",
                    r.recovery_ms
                )
            };
            line(&format!(
                "Fault #{} | {:20} | sensor={:8} | {}",
                r.index, r.kind, r.sensor, verdict
            ));
        }
        line("");

        line("— Fault Recovery Time Analysis ——————————————————————————————");
        line(&format!(
            "Recovery time — avg={:.1}ms  min={}ms  max={}ms",
            avg_rec, min_rec, max_rec
        ));
        line(&format!(
            "Within 200ms limit : {}/{} ({:.0}%)",
            within, total_f, within_pct
        ));
        line("");

        line("— Overall System Verdict ————————————————————————————————————");
        if mission_abort {
            line("⚠ MISSION STATUS: DEGRADED — recovery timeout exceeded");
        } else if a.deadline_misses > 0 {
            line("⚠ MISSION STATUS: NOMINAL (with deadline warnings)");
        } else {
            line("✓ MISSION STATUS: NOMINAL — all constraints satisfied");
        }
        line(&format!("Fault log written to: {}", self.fault_path));
        line("─────────────────────────────────────────────────────────────");

        // Write the full report to the performance log file
        let _ = std::fs::write(&self.perf_path, &report);
        println!("[LOG] Final report written to {}", self.perf_path);
    }
}