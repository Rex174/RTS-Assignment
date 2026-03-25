use std::time::Instant;

use crate::gcs::config::STALE_TELEMETRY_THRESHOLD;
use crate::gcs::fault::FaultKind;
use crate::gcs::logger::LogBundle;
use crate::gcs::packet::SensorKind;
use crate::gcs::scheduler::CommandScheduler;
use crate::gcs::state::MonitorState;
use crate::gcs::telemetry::TelemetryEvent;

pub fn process_telemetry_event(
    state: &mut MonitorState,
    scheduler: &mut CommandScheduler,
    logs: &LogBundle,
    event: &TelemetryEvent,
) {
    state.mark_receive_success(event.received_at);
    state.update_sensor_timestamp(event.packet.sensor, event.received_at);
    state.record_decode_time(event.decode_time_us);

    let reception_latency_us = event.estimated_age_us.max(0) as f64;

    let expected_interval_us = match event.packet.sensor {
        SensorKind::Thermal => 10_000.0,
        SensorKind::Power => 50_000.0,
        SensorKind::Payload => 100_000.0,
        SensorKind::Status => 100_000.0,
        SensorKind::Unknown(_) => 100_000.0,
    };

    let reception_drift_us = if let Some(last_rx) = sensor_last_time(state, event.packet.sensor) {
        let actual_interval_us =
            event.received_at.duration_since(last_rx).as_micros() as f64;
        actual_interval_us - expected_interval_us
    } else {
        0.0
    };

    state.record_reception_timing(reception_latency_us, reception_drift_us);

    match event.packet.sensor {
        SensorKind::Thermal => state.thermal_count += 1,
        SensorKind::Power => state.power_count += 1,
        SensorKind::Payload => state.payload_count += 1,
        SensorKind::Status => state.status_count += 1,
        SensorKind::Unknown(_) => state.unknown_count += 1,
    }

    if event.packet.flags.degraded {
        state.degraded_count += 1;
    }
    if event.packet.flags.safety_alert {
        state.safety_alert_count += 1;
    }
    if event.packet.flags.nan_corruption {
        state.nan_corruption_count += 1;
    }

    let degraded_activated = activate_or_clear_flag_fault(
        state,
        logs,
        FaultKind::DegradedMode,
        event.packet.flags.degraded,
        event.received_at,
        event.packet.sequence,
    );

    let safety_activated = activate_or_clear_flag_fault(
        state,
        logs,
        FaultKind::SafetyAlert,
        event.packet.flags.safety_alert,
        event.received_at,
        event.packet.sequence,
    );

    let corrupted_activated = activate_or_clear_flag_fault(
        state,
        logs,
        FaultKind::CorruptedTelemetry,
        event.packet.flags.nan_corruption,
        event.received_at,
        event.packet.sequence,
    );

    handle_sequence_tracking(state, logs, event);
    let stale_activated = check_stale_telemetry(state, logs, event.received_at);

    if degraded_activated || safety_activated || corrupted_activated || stale_activated {
        scheduler.enqueue_fault_response_commands();
        scheduler.dispatch_ready(state);
    }

    let gcs_reception_to_processing_us =
        std::time::Instant::now().duration_since(event.received_at).as_micros() as f64;
    state.record_gcs_reception_to_processing_latency(gcs_reception_to_processing_us);

    if state.total_received % 200 == 0 {
        println!(
            "[GCS][TEL] rx={} decode_avg={:.2}us decode_max={}us active_faults={}",
            state.total_received,
            state.avg_decode_time_us(),
            state.max_decode_time_us,
            state.active_faults.len()
        );
    }

    let session_time_us = state.session_start.elapsed().as_micros();

    logs.telemetry.log_line(&format!(
        "{},{},{},{},{:.6},{},{},{},{},{}",
        session_time_us,
        event.packet.sequence,
        event.packet.sensor.as_str(),
        event.remote_timestamp_us,
        event.packet.value,
        event.decode_time_us,
        event.estimated_age_us,
        event.packet.flags.degraded,
        event.packet.flags.safety_alert,
        event.packet.flags.nan_corruption
    ));
}

fn sensor_last_time(state: &MonitorState, sensor: SensorKind) -> Option<Instant> {
    match sensor {
        SensorKind::Thermal => state.last_thermal_time,
        SensorKind::Power => state.last_power_time,
        SensorKind::Payload => state.last_payload_time,
        SensorKind::Status => state.last_status_time,
        SensorKind::Unknown(_) => None,
    }
}

fn activate_or_clear_flag_fault(
    state: &mut MonitorState,
    logs: &LogBundle,
    kind: FaultKind,
    active: bool,
    received_at: Instant,
    sequence: u32,
) -> bool {
    if active {
        let activated = state.raise_or_touch_fault(kind, received_at);
        if activated {
            let activation_latency_us = Instant::now().duration_since(received_at).as_micros();
            state.record_interlock_activation_latency(kind, activation_latency_us);

            let session_time_us = state.session_start.elapsed().as_micros();
            logs.fault.log_line(&format!(
                "{},{},FaultActivated,{:?},activation_latency_us={}",
                session_time_us, sequence, kind, activation_latency_us
            ));
        }
        activated
    } else {
        state.clear_fault(kind, received_at);
        false
    }
}

fn handle_sequence_tracking(state: &mut MonitorState, logs: &LogBundle, event: &TelemetryEvent) {
    let current_seq = event.packet.sequence;

    match state.last_sequence {
        None => {
            state.last_sequence = Some(current_seq);
        }
        Some(last_seq) => {
            let expected_next = last_seq.wrapping_add(1);

            if current_seq == expected_next {
                state.last_sequence = Some(current_seq);
                return;
            }

            if current_seq > expected_next {
                let missing = current_seq - expected_next;
                state.mark_missing_packets(missing, event.received_at);

                println!(
                    "[GCS][WARN] Packet gap detected: expected seq {}, got {} (missing {})",
                    expected_next,
                    current_seq,
                    missing
                );

                println!(
                    "[GCS][INFO] Re-request should be triggered for {} missing packet(s).",
                    missing
                );

                let session_time_us = state.session_start.elapsed().as_micros();
                logs.fault.log_line(&format!(
                    "{},{},PacketGap,expected_seq={},got_seq={},missing={}",
                    session_time_us,
                    event.packet.sequence,
                    expected_next,
                    current_seq,
                    missing
                ));

                state.last_sequence = Some(current_seq);
                return;
            }

            if current_seq <= last_seq {
                println!(
                    "[GCS][WARN] Out-of-order or duplicate packet detected: last_seq={}, current_seq={}",
                    last_seq,
                    current_seq
                );

                let session_time_us = state.session_start.elapsed().as_micros();
                logs.fault.log_line(&format!(
                    "{},{},OutOfOrder,last_seq={},current_seq={}",
                    session_time_us,
                    event.packet.sequence,
                    last_seq,
                    current_seq
                ));
            }
        }
    }
}

fn check_stale_telemetry(state: &mut MonitorState, logs: &LogBundle, now: Instant) -> bool {
    if let Some(last_rx) = state.last_receive_time {
        if now.duration_since(last_rx) > STALE_TELEMETRY_THRESHOLD {
            let activated = state.raise_or_touch_fault(FaultKind::StaleTelemetry, now);
            if activated {
                let activation_latency_us = 0u128;
                state.record_interlock_activation_latency(
                    FaultKind::StaleTelemetry,
                    activation_latency_us,
                );

                let session_time_us = state.session_start.elapsed().as_micros();
                logs.fault.log_line(&format!(
                    "{},0,FaultActivated,StaleTelemetry,activation_latency_us=0",
                    session_time_us
                ));
            }
            return activated;
        }
    }

    state.clear_fault(FaultKind::StaleTelemetry, now);
    false
}