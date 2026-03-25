use crate::gcs::config::UPLINK_TARGET_ADDR;
use crate::gcs::state::MonitorState;

fn overall_status(state: &MonitorState) -> &'static str {
    if state.total_decode_errors > 0 || state.total_critical_ground_alerts > 0 {
        return "WARNING";
    }

    if state.loss_of_contact_active || !state.active_faults.is_empty() {
        return "DEGRADED";
    }

    "NOMINAL"
}

fn performance_compliance(state: &MonitorState) -> &'static str {
    let decode_pass = state.max_decode_time_us <= 3_000;
    let urgent_dispatch_pass = state.total_urgent_deadline_misses == 0;
    let interlock_pass = state.max_interlock_activation_latency_us <= 100_000;

    if decode_pass && urgent_dispatch_pass && interlock_pass {
        "✓ PASS"
    } else {
        "✗ CHECK"
    }
}

fn pass_fail(pass: bool) -> &'static str {
    if pass {
        "✓ PASS"
    } else {
        "✗ CHECK"
    }
}

fn us_to_ms(value_us: f64) -> f64 {
    value_us / 1000.0
}

pub fn print_final_report(state: &MonitorState) {
    let session_secs = state.session_start.elapsed().as_secs_f64();

    let decode_pass = state.max_decode_time_us <= 3_000;
    let urgent_dispatch_pass = state.total_urgent_deadline_misses == 0;
    let interlock_pass = state.max_interlock_activation_latency_us <= 100_000;

    let total_commands_processed =
        state.total_commands_dispatched + state.total_commands_rejected;

    println!();
    println!("┌────────────────────────────────────────────────────────────┐");
    println!("│               STUDENT B — GCS FINAL REPORT                │");
    println!("└────────────────────────────────────────────────────────────┘");
    println!("Session duration        : {:.1}s", session_secs);
    println!("End-of-session state    : {}", overall_status(state));
    println!("Performance compliance  : {}", performance_compliance(state));

    println!();
    println!("┌────────────────────────────────────────────────────────────┐");
    println!("│         TASK 1 — TELEMETRY RECEPTION & DECODING           │");
    println!("└────────────────────────────────────────────────────────────┘");
    println!("Telemetry received      : {}", state.total_received);
    println!("Decode errors           : {}", state.total_decode_errors);
    println!("Missing packets         : {}", state.total_missing_packets);
    println!("Loss of contact count   : {}", state.loss_of_contact_count);
    println!("Contact restored count  : {}", state.contact_restored_count);

    println!();
    println!("— Sensor Packet Counts ————————————————————————————————————");
    println!("THERMAL                 : {}", state.thermal_count);
    println!("POWER                   : {}", state.power_count);
    println!("PAYLOAD                 : {}", state.payload_count);
    println!("STATUS                  : {}", state.status_count);
    println!("UNKNOWN                 : {}", state.unknown_count);

    println!();
    println!("— Telemetry Performance ——————————————————————————————————");
    println!(
        "Decode avg              : {:.3}ms ({:.2}us)",
        us_to_ms(state.avg_decode_time_us()),
        state.avg_decode_time_us()
    );
    println!(
        "Decode max              : {:.3}ms ({}us)",
        us_to_ms(state.max_decode_time_us as f64),
        state.max_decode_time_us
    );
    println!(
        "GCS recv→process avg    : {:.3}ms ({:.2}us)",
        us_to_ms(state.gcs_reception_to_processing_us.avg()),
        state.gcs_reception_to_processing_us.avg()
    );
    println!(
        "GCS recv→process max    : {:.3}ms ({:.2}us)",
        us_to_ms(state.gcs_reception_to_processing_us.max),
        state.gcs_reception_to_processing_us.max
    );
    println!(
        "Reception drift avg     : {:.3}ms ({:.2}us)",
        us_to_ms(state.reception_drift_us.avg()),
        state.reception_drift_us.avg()
    );
    println!(
        "Reception drift max     : {:.3}ms ({:.2}us)",
        us_to_ms(state.reception_drift_us.max),
        state.reception_drift_us.max
    );
    println!(
        "Telemetry backlog avg   : {:.2}",
        state.telemetry_backlog_depth.avg()
    );
    println!(
        "Telemetry backlog max   : {:.2}",
        state.telemetry_backlog_depth.max
    );

    println!();
    println!("— Telemetry Compliance ———————————————————————————————————");
    println!("Decode target           : ≤3ms");
    println!("Decode compliance       : {}", pass_fail(decode_pass));

    println!();
    println!("┌────────────────────────────────────────────────────────────┐");
    println!("│      TASK 2 — COMMAND SCHEDULING & SAFETY INTERLOCKS      │");
    println!("└────────────────────────────────────────────────────────────┘");
    println!("Commands dispatched     : {}", state.total_commands_dispatched);
    println!("Commands rejected       : {}", state.total_commands_rejected);
    println!("Total commands handled  : {}", total_commands_processed);
    println!("Urgent commands         : {}", state.total_urgent_commands);
    println!("Urgent dispatched       : {}", state.total_urgent_commands_dispatched);
    println!("Deadline misses         : {}", state.total_command_deadline_misses);
    println!("Urgent deadline misses  : {}", state.total_urgent_deadline_misses);
    println!(
        "Urgent adherence        : {:.3}%",
        state.urgent_deadline_adherence_percent()
    );

    println!();
    println!("— Scheduler / Interlock Timing ———————————————————————————");
    println!(
        "Urgent dispatch avg     : {:.3}ms ({:.2}us)",
        us_to_ms(state.urgent_dispatch_latency_us.avg()),
        state.urgent_dispatch_latency_us.avg()
    );
    println!(
        "Urgent dispatch max     : {:.3}ms ({:.2}us)",
        us_to_ms(state.urgent_dispatch_latency_us.max),
        state.urgent_dispatch_latency_us.max
    );
    println!(
        "Interlock activation avg: {:.3}ms ({:.2}us)",
        us_to_ms(state.avg_interlock_activation_latency_us()),
        state.avg_interlock_activation_latency_us()
    );
    println!(
        "Interlock activation max: {:.3}ms ({}us)",
        us_to_ms(state.max_interlock_activation_latency_us as f64),
        state.max_interlock_activation_latency_us
    );
    println!(
        "First blocked cmd avg   : {:.3}ms ({:.2}us)",
        us_to_ms(state.avg_first_blocked_command_latency_us()),
        state.avg_first_blocked_command_latency_us()
    );
    println!(
        "First blocked cmd max   : {:.3}ms ({}us)",
        us_to_ms(state.max_first_blocked_command_latency_us as f64),
        state.max_first_blocked_command_latency_us
    );

    println!();
    println!("— Command Rejection Reasons —————————————————————————————");
    println!(
        "SafetyAlertActive       : {}",
        state.rejection_counts.safety_alert_active
    );
    println!(
        "DegradedModeActive      : {}",
        state.rejection_counts.degraded_mode_active
    );
    println!(
        "CorruptedTelemetryActive: {}",
        state.rejection_counts.corrupted_telemetry_active
    );
    println!(
        "LossOfContactActive     : {}",
        state.rejection_counts.loss_of_contact_active
    );
    println!(
        "StaleTelemetryActive    : {}",
        state.rejection_counts.stale_telemetry_active
    );

    println!();
    println!("— Command Scheduler Compliance ——————————————————————————");
    println!("Urgent dispatch target  : ≤2ms");
    println!("Urgent dispatch status  : {}", pass_fail(urgent_dispatch_pass));

    println!();
    println!("┌────────────────────────────────────────────────────────────┐");
    println!("│        TASK 3 — FAULT HANDLING & SAFETY RESPONSE          │");
    println!("└────────────────────────────────────────────────────────────┘");
    println!("Fault activations       : {}", state.total_fault_activations);
    println!("Fault clear events      : {}", state.total_fault_clears);
    println!("Critical ground alerts  : {}", state.total_critical_ground_alerts);

    println!();
    println!("— Fault / Flag Observations ——————————————————————————————");
    println!("Degraded flags          : {}", state.degraded_count);
    println!("Safety alert flags      : {}", state.safety_alert_count);
    println!("NaN/corruption flags    : {}", state.nan_corruption_count);

    println!();
    println!("— GCS-Observed Fault Clear Summary ——————————————————————");
    println!(
        "Fault clear avg         : {:.2}ms",
        state.fault_recovery_ms.avg()
    );
    println!(
        "Fault clear max         : {:.2}ms",
        state.fault_recovery_ms.max
    );
    println!(
        "Clear events counted    : {}",
        state.fault_recovery_ms.count
    );

    println!();
    println!("— Fault Response Compliance —————————————————————————————");
    println!("Interlock response limit: ≤100ms");
    println!("Critical ground alerts  : {}", state.total_critical_ground_alerts);
    println!("Interlock status        : {}", pass_fail(interlock_pass));

    println!();
    println!("— End-State Faults ———————————————————————————————————————");
    println!("Loss of contact active  : {}", state.loss_of_contact_active);
    println!("Active faults           : {}", state.active_faults.len());
    for fault in &state.active_faults {
        println!("  - {:?}", fault.kind);
    }

    println!();
    println!("┌────────────────────────────────────────────────────────────┐");
    println!("│       TASK 4 — BENCHMARKING, LOGGING & INTEGRATION        │");
    println!("└────────────────────────────────────────────────────────────┘");

    println!("— GCS Runtime Configuration —————————————————————————————");
    println!("Protocol                : UDP telemetry receiver");
    println!("Bind address            : 127.0.0.1:9002");
    println!("Uplink transport        : UDP command sender");
    println!("Uplink target           : {}", UPLINK_TARGET_ADDR);
    println!("Expected packet size    : 27 bytes");
    println!("Telemetry queue         : {} events", 1024);
    println!("Decode deadline         : 3ms");
    println!("Urgent dispatch limit   : 2ms");
    println!("Critical alert limit    : 100ms");
    println!("Loss-of-contact rule    : 3 missed intervals");
    println!("Session end grace       : 700ms silence after telemetry start");

    println!();
    println!("— GCS Runtime Performance ———————————————————————————————");
    println!(
        "Scheduler drift avg     : {:.3}ms ({:.2}us)",
        us_to_ms(state.scheduler_drift_us.avg()),
        state.scheduler_drift_us.avg()
    );
    println!(
        "Scheduler drift max     : {:.3}ms ({:.2}us)",
        us_to_ms(state.scheduler_drift_us.max),
        state.scheduler_drift_us.max
    );
    println!(
        "Scheduler jitter avg    : {:.3}ms ({:.2}us)",
        us_to_ms(state.scheduler_jitter_us.avg()),
        state.scheduler_jitter_us.avg()
    );
    println!(
        "Scheduler jitter max    : {:.3}ms ({:.2}us)",
        us_to_ms(state.scheduler_jitter_us.max),
        state.scheduler_jitter_us.max
    );
    println!(
        "GCS task exec avg       : {:.3}ms ({:.2}us)",
        us_to_ms(state.gcs_task_exec_time_us.avg()),
        state.gcs_task_exec_time_us.avg()
    );
    println!(
        "GCS task exec max       : {:.3}ms ({:.2}us)",
        us_to_ms(state.gcs_task_exec_time_us.max),
        state.gcs_task_exec_time_us.max
    );
    println!(
        "Estimated GCS load      : {:.2}%",
        state.gcs_scheduler_load_percent()
    );

    println!();
    println!("— Benchmark Sample Counts ———————————————————————————————");
    println!(
        "Telemetry drift tracked : {} samples",
        state.reception_drift_us.count
    );
    println!(
        "GCS recv→process tracked: {} samples",
        state.gcs_reception_to_processing_us.count
    );
    println!(
        "Telemetry backlog tracked: {} samples",
        state.telemetry_backlog_depth.count
    );
    println!(
        "Urgent dispatch tracked : {} samples",
        state.urgent_dispatch_latency_us.count
    );
    println!(
        "Scheduler drift tracked : {} samples",
        state.scheduler_drift_us.count
    );
    println!(
        "Scheduler jitter tracked: {} samples",
        state.scheduler_jitter_us.count
    );
    println!(
        "Fault clear tracked     : {} samples",
        state.fault_recovery_ms.count
    );

    println!();
    println!("— CSV Logs Generated —————————————————————————————————————");
    println!("  - telemetry_log.csv");
    println!("  - command_log.csv");
    println!("  - fault_log.csv");

    println!();
    println!("— Interpretation Notes ———————————————————————————————————");
    println!("Interlock activation latency measures GCS safety response");
    println!("time from fault detection to interlock activation.");
    println!("First blocked command latency measures when an unsafe");
    println!("command was first encountered after fault activation.");
    println!("Reception drift is reported as the absolute difference");
    println!("between observed and expected inter-arrival timing.");
    println!("GCS recv→process latency measures packet handling time");
    println!("from socket reception to end of telemetry processing on");
    println!("the ground station side.");
    println!("Telemetry backlog measures the depth of the bounded");
    println!("in-process telemetry event queue on the GCS side.");
    println!("System load is estimated from scheduler active time over");
    println!("total GCS runtime, not OS-level CPU measurement.");
    println!("Scheduler drift reflects periodic task lateness under");
    println!("live asynchronous telemetry load and shared-state contention.");
    println!("Fault clear timing is reported from the GCS observation");
    println!("perspective, not as onboard injected-fault recovery timing.");

    println!();
    println!("— Integration Note ———————————————————————————————————————");
    println!("Telemetry reception/monitoring is integrated with Student A's");
    println!("real OCS output. Ground-side command scheduling, validation,");
    println!("and safety interlocks are implemented in Student B.");
    println!("Student B now performs real outbound UDP uplink sends for");
    println!("accepted commands.");
    println!("End-to-end command receipt/execution is only fully proven");
    println!("once Student A exposes a matching inbound command receiver");
    println!("and logs received commands.");

    println!();
    println!("— Overall System Verdict ———————————————————————————————————");
    println!("End-of-session state    : {}", overall_status(state));
    println!("Performance compliance  : {}", performance_compliance(state));
    println!("Logs written            : telemetry_log.csv, command_log.csv, fault_log.csv");
    println!("────────────────────────────────────────────────────────────");
}