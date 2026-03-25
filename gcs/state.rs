use std::time::Instant;

use crate::gcs::config::{
    FAULT_RESPONSE_ALERT_THRESHOLD_US, LOSS_OF_CONTACT_CONSECUTIVE_FAILURES,
};
use crate::gcs::fault::{ActiveFault, FaultKind};
use crate::gcs::interlock::RejectReason;
use crate::gcs::packet::SensorKind;

#[derive(Debug, Clone, Default)]
pub struct RunningMetric {
    pub count: u64,
    pub total: f64,
    pub max: f64,
}

impl RunningMetric {
    pub fn record(&mut self, value: f64) {
        self.count += 1;
        self.total += value;
        if value > self.max {
            self.max = value;
        }
    }

    pub fn avg(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total / self.count as f64
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RejectionCounts {
    pub safety_alert_active: u64,
    pub degraded_mode_active: u64,
    pub corrupted_telemetry_active: u64,
    pub loss_of_contact_active: u64,
    pub stale_telemetry_active: u64,
}

impl RejectionCounts {
    pub fn record(&mut self, reason: RejectReason) {
        match reason {
            RejectReason::SafetyAlertActive => self.safety_alert_active += 1,
            RejectReason::DegradedModeActive => self.degraded_mode_active += 1,
            RejectReason::CorruptedTelemetryActive => self.corrupted_telemetry_active += 1,
            RejectReason::LossOfContactActive => self.loss_of_contact_active += 1,
            RejectReason::StaleTelemetryActive => self.stale_telemetry_active += 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MonitorState {
    pub session_start: Instant,
    pub session_finished: bool,
    pub total_received: u64,
    pub total_decode_errors: u64,
    pub total_missing_packets: u64,

    pub total_decode_time_us: u128,
    pub max_decode_time_us: u128,

    pub last_sequence: Option<u32>,
    pub consecutive_failures: u32,
    pub loss_of_contact_active: bool,
    pub loss_of_contact_count: u64,
    pub contact_restored_count: u64,

    pub telemetry_started: bool,
    pub last_receive_time: Option<Instant>,

    pub thermal_count: u64,
    pub power_count: u64,
    pub payload_count: u64,
    pub status_count: u64,
    pub unknown_count: u64,

    pub degraded_count: u64,
    pub safety_alert_count: u64,
    pub nan_corruption_count: u64,

    pub last_thermal_time: Option<Instant>,
    pub last_power_time: Option<Instant>,
    pub last_payload_time: Option<Instant>,
    pub last_status_time: Option<Instant>,

    pub active_faults: Vec<ActiveFault>,
    pub total_fault_activations: u64,
    pub total_fault_clears: u64,

    pub total_commands_dispatched: u64,
    pub total_commands_rejected: u64,
    pub total_urgent_commands: u64,
    pub total_urgent_commands_dispatched: u64,
    pub total_urgent_deadline_misses: u64,
    pub total_command_deadline_misses: u64,

    pub total_interlock_activations: u64,
    pub total_interlock_activation_latency_us: u128,
    pub max_interlock_activation_latency_us: u128,

    pub total_first_blocked_command_events: u64,
    pub total_first_blocked_command_latency_us: u128,
    pub max_first_blocked_command_latency_us: u128,

    pub total_critical_ground_alerts: u64,

    pub reception_latency_us: RunningMetric,
    pub reception_drift_us: RunningMetric,
    pub urgent_dispatch_latency_us: RunningMetric,
    pub fault_recovery_ms: RunningMetric,
    pub rejection_counts: RejectionCounts,

    // Real telemetry backlog: number of telemetry events waiting in the in-process queue.
    pub telemetry_backlog_depth: RunningMetric,
    pub pending_telemetry_events: u64,

    pub scheduler_drift_us: RunningMetric,
    pub scheduler_jitter_us: RunningMetric,
    pub gcs_task_exec_time_us: RunningMetric,

    pub gcs_reception_to_processing_us: RunningMetric,

    pub scheduler_tick_count: u64,
    pub scheduler_active_time_us: u128,
    pub scheduler_last_actual_at: Option<Instant>,
}

impl MonitorState {
    pub fn new() -> Self {
        Self {
            session_start: Instant::now(),
            session_finished: false,
            total_received: 0,
            total_decode_errors: 0,
            total_missing_packets: 0,

            total_decode_time_us: 0,
            max_decode_time_us: 0,

            last_sequence: None,
            consecutive_failures: 0,
            loss_of_contact_active: false,
            loss_of_contact_count: 0,
            contact_restored_count: 0,

            telemetry_started: false,
            last_receive_time: None,

            thermal_count: 0,
            power_count: 0,
            payload_count: 0,
            status_count: 0,
            unknown_count: 0,

            degraded_count: 0,
            safety_alert_count: 0,
            nan_corruption_count: 0,

            last_thermal_time: None,
            last_power_time: None,
            last_payload_time: None,
            last_status_time: None,

            active_faults: Vec::new(),
            total_fault_activations: 0,
            total_fault_clears: 0,

            total_commands_dispatched: 0,
            total_commands_rejected: 0,
            total_urgent_commands: 0,
            total_urgent_commands_dispatched: 0,
            total_urgent_deadline_misses: 0,
            total_command_deadline_misses: 0,

            total_interlock_activations: 0,
            total_interlock_activation_latency_us: 0,
            max_interlock_activation_latency_us: 0,

            total_first_blocked_command_events: 0,
            total_first_blocked_command_latency_us: 0,
            max_first_blocked_command_latency_us: 0,

            total_critical_ground_alerts: 0,

            reception_latency_us: RunningMetric::default(),
            reception_drift_us: RunningMetric::default(),
            urgent_dispatch_latency_us: RunningMetric::default(),
            fault_recovery_ms: RunningMetric::default(),
            rejection_counts: RejectionCounts::default(),

            telemetry_backlog_depth: RunningMetric::default(),
            pending_telemetry_events: 0,

            scheduler_drift_us: RunningMetric::default(),
            scheduler_jitter_us: RunningMetric::default(),
            gcs_task_exec_time_us: RunningMetric::default(),

            gcs_reception_to_processing_us: RunningMetric::default(),

            scheduler_tick_count: 0,
            scheduler_active_time_us: 0,
            scheduler_last_actual_at: None,
        }
    }

    pub fn record_decode_time(&mut self, decode_time_us: u128) {
        self.total_decode_time_us += decode_time_us;
        if decode_time_us > self.max_decode_time_us {
            self.max_decode_time_us = decode_time_us;
        }
    }

    pub fn record_reception_timing(&mut self, latency_us: f64, drift_us: f64) {
        self.reception_latency_us.record(latency_us);
        self.reception_drift_us.record(drift_us.abs());
    }

    pub fn record_gcs_reception_to_processing_latency(&mut self, latency_us: f64) {
        self.gcs_reception_to_processing_us.record(latency_us);
    }

    pub fn record_urgent_dispatch_latency(&mut self, latency_us: f64, deadline_missed: bool) {
        self.total_urgent_commands += 1;
        self.total_urgent_commands_dispatched += 1;
        self.urgent_dispatch_latency_us.record(latency_us);

        if deadline_missed {
            self.total_urgent_deadline_misses += 1;
        }
    }

    pub fn record_telemetry_backlog(&mut self, backlog_depth: f64) {
        self.telemetry_backlog_depth.record(backlog_depth);
    }

    pub fn mark_telemetry_enqueued(&mut self) {
        self.pending_telemetry_events += 1;
        self.record_telemetry_backlog(self.pending_telemetry_events as f64);
    }

    pub fn mark_telemetry_dequeued(&mut self) {
        // Record the queue depth seen at dequeue time, then decrement.
        self.record_telemetry_backlog(self.pending_telemetry_events as f64);
        if self.pending_telemetry_events > 0 {
            self.pending_telemetry_events -= 1;
        }
        self.record_telemetry_backlog(self.pending_telemetry_events as f64);
    }

    pub fn record_scheduler_tick(
        &mut self,
        expected_at: Instant,
        actual_at: Instant,
        nominal_period_us: f64,
        exec_time_us: u128,
    ) {
        self.scheduler_tick_count += 1;
        self.scheduler_active_time_us += exec_time_us;

        let drift_us = if actual_at >= expected_at {
            actual_at.duration_since(expected_at).as_micros() as f64
        } else {
            expected_at.duration_since(actual_at).as_micros() as f64
        };
        self.scheduler_drift_us.record(drift_us);

        if let Some(last_actual) = self.scheduler_last_actual_at {
            let actual_interval_us = actual_at.duration_since(last_actual).as_micros() as f64;
            let jitter_us = (actual_interval_us - nominal_period_us).abs();
            self.scheduler_jitter_us.record(jitter_us);
        }

        self.gcs_task_exec_time_us.record(exec_time_us as f64);
        self.scheduler_last_actual_at = Some(actual_at);
    }

    pub fn gcs_scheduler_load_percent(&self) -> f64 {
        let elapsed_us = self.session_start.elapsed().as_micros() as f64;
        if elapsed_us <= 0.0 {
            0.0
        } else {
            (self.scheduler_active_time_us as f64 / elapsed_us) * 100.0
        }
    }

    pub fn avg_decode_time_us(&self) -> f64 {
        if self.total_received == 0 {
            0.0
        } else {
            self.total_decode_time_us as f64 / self.total_received as f64
        }
    }

    pub fn avg_interlock_activation_latency_us(&self) -> f64 {
        if self.total_interlock_activations == 0 {
            0.0
        } else {
            self.total_interlock_activation_latency_us as f64
                / self.total_interlock_activations as f64
        }
    }

    pub fn avg_first_blocked_command_latency_us(&self) -> f64 {
        if self.total_first_blocked_command_events == 0 {
            0.0
        } else {
            self.total_first_blocked_command_latency_us as f64
                / self.total_first_blocked_command_events as f64
        }
    }

    pub fn urgent_deadline_adherence_percent(&self) -> f64 {
        if self.total_urgent_commands_dispatched == 0 {
            100.0
        } else {
            let good =
                self.total_urgent_commands_dispatched - self.total_urgent_deadline_misses;
            (good as f64 / self.total_urgent_commands_dispatched as f64) * 100.0
        }
    }

    pub fn mark_decode_error(&mut self) {
        self.total_decode_errors += 1;
        self.consecutive_failures += 1;
        self.update_loss_of_contact();
    }

    pub fn mark_receive_success(&mut self, now: Instant) {
        self.telemetry_started = true;
        self.total_received += 1;
        self.last_receive_time = Some(now);
        self.consecutive_failures = 0;

        if self.loss_of_contact_active {
            println!("[GCS][INFO] Contact restored.");
            self.loss_of_contact_active = false;
            self.contact_restored_count += 1;
            self.clear_fault(FaultKind::LossOfContact, now);
        }
    }

    pub fn mark_missing_packets(&mut self, count: u32, now: Instant) {
        if count == 0 {
            return;
        }

        self.total_missing_packets += count as u64;
        self.consecutive_failures += count;
        self.update_loss_of_contact();

        if self.loss_of_contact_active {
            self.raise_or_touch_fault(FaultKind::LossOfContact, now);
        }
    }

    fn update_loss_of_contact(&mut self) {
        if !self.loss_of_contact_active
            && self.consecutive_failures >= LOSS_OF_CONTACT_CONSECUTIVE_FAILURES
        {
            self.loss_of_contact_active = true;
            self.loss_of_contact_count += 1;
            println!(
                "[GCS][ALERT] Loss of contact triggered after {} consecutive failures.",
                self.consecutive_failures
            );
        }
    }

    pub fn update_sensor_timestamp(&mut self, sensor: SensorKind, now: Instant) {
        match sensor {
            SensorKind::Thermal => self.last_thermal_time = Some(now),
            SensorKind::Power => self.last_power_time = Some(now),
            SensorKind::Payload => self.last_payload_time = Some(now),
            SensorKind::Status => self.last_status_time = Some(now),
            SensorKind::Unknown(_) => {}
        }
    }

    pub fn raise_or_touch_fault(&mut self, kind: FaultKind, now: Instant) -> bool {
        if let Some(existing) = self.active_faults.iter_mut().find(|fault| fault.kind == kind) {
            existing.touch(now);
            return false;
        }

        self.total_fault_activations += 1;
        println!("[GCS][FAULT] Activated {:?}", kind);
        self.active_faults.push(ActiveFault::new(kind, now));
        true
    }

    pub fn clear_fault(&mut self, kind: FaultKind, now: Instant) {
        if let Some(index) = self.active_faults.iter().position(|fault| fault.kind == kind) {
            let cleared = self.active_faults.remove(index);

            self.total_fault_clears += 1;
            let recovery_ms = now.duration_since(cleared.started_at).as_secs_f64() * 1000.0;
            self.fault_recovery_ms.record(recovery_ms);

            println!("[GCS][FAULT] Cleared {:?} after {} ms", kind, recovery_ms.round());
        }
    }

    pub fn has_fault(&self, kind: FaultKind) -> bool {
        self.active_faults.iter().any(|fault| fault.kind == kind)
    }

    pub fn record_interlock_activation_latency(
        &mut self,
        kind: FaultKind,
        latency_us: u128,
    ) {
        self.total_interlock_activations += 1;
        self.total_interlock_activation_latency_us += latency_us;

        if latency_us > self.max_interlock_activation_latency_us {
            self.max_interlock_activation_latency_us = latency_us;
        }

        if latency_us > FAULT_RESPONSE_ALERT_THRESHOLD_US {
            self.total_critical_ground_alerts += 1;
            println!(
                "[GCS][CRITICAL] Ground alert: {:?} activation latency {}us exceeded {}us",
                kind,
                latency_us,
                FAULT_RESPONSE_ALERT_THRESHOLD_US
            );
        }
    }

    pub fn record_first_blocked_command(
        &mut self,
        reason: RejectReason,
        now: Instant,
    ) -> Option<u128> {
        self.rejection_counts.record(reason);

        let fault_kind = match reason {
            RejectReason::SafetyAlertActive => FaultKind::SafetyAlert,
            RejectReason::DegradedModeActive => FaultKind::DegradedMode,
            RejectReason::CorruptedTelemetryActive => FaultKind::CorruptedTelemetry,
            RejectReason::LossOfContactActive => FaultKind::LossOfContact,
            RejectReason::StaleTelemetryActive => FaultKind::StaleTelemetry,
        };

        let Some(fault) = self
            .active_faults
            .iter_mut()
            .find(|fault| fault.kind == fault_kind)
        else {
            return None;
        };

        if let Some(existing_latency) = fault.first_blocked_command_latency_us {
            return Some(existing_latency);
        }

        let latency_us = now.duration_since(fault.started_at).as_micros();
        fault.first_blocked_command_latency_us = Some(latency_us);

        self.total_first_blocked_command_events += 1;
        self.total_first_blocked_command_latency_us += latency_us;

        if latency_us > self.max_first_blocked_command_latency_us {
            self.max_first_blocked_command_latency_us = latency_us;
        }

        Some(latency_us)
    }

    pub fn print_summary(&self) {
        println!();
        println!("================ GCS SESSION SUMMARY ================");
        println!("Telemetry received      : {}", self.total_received);
        println!("Decode errors           : {}", self.total_decode_errors);
        println!("Missing packets         : {}", self.total_missing_packets);
        println!("Loss of contact count   : {}", self.loss_of_contact_count);
        println!("Fault activations       : {}", self.total_fault_activations);
        println!("Commands dispatched     : {}", self.total_commands_dispatched);
        println!("Commands rejected       : {}", self.total_commands_rejected);
        println!("Deadline misses         : {}", self.total_command_deadline_misses);
        println!(
            "Decode avg/max          : {:.2}us / {}us",
            self.avg_decode_time_us(),
            self.max_decode_time_us
        );
        println!(
            "Interlock act avg/max   : {:.2}us / {}us",
            self.avg_interlock_activation_latency_us(),
            self.max_interlock_activation_latency_us
        );
        println!(
            "First block avg/max     : {:.2}us / {}us",
            self.avg_first_blocked_command_latency_us(),
            self.max_first_blocked_command_latency_us
        );
        println!(
            "Telemetry backlog avg/max: {:.2} / {:.2}",
            self.telemetry_backlog_depth.avg(),
            self.telemetry_backlog_depth.max
        );
        println!(
            "Scheduler drift avg/max : {:.2}us / {:.2}us",
            self.scheduler_drift_us.avg(),
            self.scheduler_drift_us.max
        );
        println!(
            "Scheduler jitter avg/max: {:.2}us / {:.2}us",
            self.scheduler_jitter_us.avg(),
            self.scheduler_jitter_us.max
        );
        println!("Critical alerts         : {}", self.total_critical_ground_alerts);
        println!("=====================================================");
    }
}