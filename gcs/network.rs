use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{self, MissedTickBehavior};

use crate::gcs::command::Command;
use crate::gcs::config::{
    CONTACT_IDLE_TIMEOUT, EXPECTED_PACKET_SIZE, GCS_BIND_ADDR, GCS_SCHEDULER_PERIOD,
    GCS_UPLINK_BIND_ADDR, LOSS_OF_CONTACT_CONSECUTIVE_FAILURES,
    TELEMETRY_DECODE_DEADLINE_US, UPLINK_TARGET_ADDR,
};
use crate::gcs::fault::FaultKind;
use crate::gcs::logger::{CsvLogger, LogBundle};
use crate::gcs::monitor::process_telemetry_event;
use crate::gcs::packet::TelemetryPacket;
use crate::gcs::report::print_final_report;
use crate::gcs::scheduler::CommandScheduler;
use crate::gcs::state::MonitorState;
use crate::gcs::telemetry::TelemetryEvent;

const TELEMETRY_QUEUE_CAPACITY: usize = 1024;

fn post_stream_end_grace() -> Duration {
    CONTACT_IDLE_TIMEOUT * 2
}

pub async fn run_receiver() -> Result<()> {
    let telemetry_socket = UdpSocket::bind(GCS_BIND_ADDR).await?;
    println!("[GCS] Listening on {}", GCS_BIND_ADDR);

    let uplink_socket = UdpSocket::bind(GCS_UPLINK_BIND_ADDR).await?;
    let uplink_local_addr = uplink_socket.local_addr()?;
    println!(
        "[GCS][UPLINK] Outbound UDP socket {} -> {}",
        uplink_local_addr, UPLINK_TARGET_ADDR
    );

    let logs = LogBundle {
        telemetry: CsvLogger::new(
            "telemetry_log.csv",
            "session_time_us,sequence,sensor,remote_timestamp_us,value,decode_time_us,estimated_age_us,degraded,safety_alert,nan_corruption",
        )?,
        command: CsvLogger::new(
            "command_log.csv",
            "session_time_us,command_id,command_kind,priority,stage,result,metric_us,destination",
        )?,
        fault: CsvLogger::new("fault_log.csv", "session_time_us,sequence,event,details")?,
    };

    let state = Arc::new(Mutex::new(MonitorState::new()));

    let (uplink_tx, mut uplink_rx) = mpsc::unbounded_channel::<Command>();
    let scheduler = Arc::new(Mutex::new(CommandScheduler::new(
        logs.command.clone(),
        uplink_tx,
    )));

    let (telemetry_tx, mut telemetry_rx) =
        mpsc::channel::<TelemetryEvent>(TELEMETRY_QUEUE_CAPACITY);

    {
        let command_log = logs.command.clone();
        let state_for_uplink = Arc::clone(&state);

        tokio::spawn(async move {
            while let Some(command) = uplink_rx.recv().await {
                let session_time_us = {
                    let state_guard = state_for_uplink.lock().await;
                    if state_guard.session_finished {
                        break;
                    }
                    state_guard.session_start.elapsed().as_micros()
                };

                let payload = command.to_wire_line();
                let send_started_at = std::time::Instant::now();

                match uplink_socket.send_to(payload.as_bytes(), UPLINK_TARGET_ADDR).await {
                    Ok(bytes_sent) => {
                        let send_latency_us = send_started_at.elapsed().as_micros();

                        println!(
                            "[GCS][UPLINK] SENT id={} kind={} priority={} bytes={} dest={} latency={}us",
                            command.id,
                            command.kind.as_str(),
                            command.priority.as_str(),
                            bytes_sent,
                            UPLINK_TARGET_ADDR,
                            send_latency_us
                        );

                        command_log.log_line(&format!(
                            "{},{},{},{},uplink,SENT_OK,{},{}",
                            session_time_us,
                            command.id,
                            command.kind.as_str(),
                            command.priority.as_str(),
                            send_latency_us,
                            UPLINK_TARGET_ADDR
                        ));
                    }
                    Err(err) => {
                        println!(
                            "[GCS][UPLINK][ERROR] send failed id={} kind={} priority={} dest={} err={}",
                            command.id,
                            command.kind.as_str(),
                            command.priority.as_str(),
                            UPLINK_TARGET_ADDR,
                            err
                        );

                        command_log.log_line(&format!(
                            "{},{},{},{},uplink,SENT_FAIL,0,{}",
                            session_time_us,
                            command.id,
                            command.kind.as_str(),
                            command.priority.as_str(),
                            UPLINK_TARGET_ADDR
                        ));
                    }
                }
            }
        });
    }

    {
        let state_for_scheduler = Arc::clone(&state);
        let scheduler_for_task = Arc::clone(&scheduler);

        tokio::spawn(async move {
            let mut tick = time::interval(GCS_SCHEDULER_PERIOD);
            tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
            let _ = tick.tick().await;

            let nominal_period_us = GCS_SCHEDULER_PERIOD.as_micros() as f64;

            loop {
                let scheduled_at: std::time::Instant = tick.tick().await.into();
                let actual_at = std::time::Instant::now();

                let mut state_guard = state_for_scheduler.lock().await;

                if state_guard.session_finished {
                    break;
                }

                let mut scheduler_guard = scheduler_for_task.lock().await;
                let exec_start = std::time::Instant::now();

                if state_guard.telemetry_started {
                    scheduler_guard.enqueue_demo_commands();
                }

                scheduler_guard.dispatch_ready(&mut state_guard);
                let exec_time_us = exec_start.elapsed().as_micros();

                state_guard.record_scheduler_tick(
                    scheduled_at,
                    actual_at,
                    nominal_period_us,
                    exec_time_us,
                );

                if state_guard.scheduler_tick_count % 50 == 0 && state_guard.telemetry_started {
                    println!(
                        "[GCS][SCHED] dispatched={} rejected={} queue_depth={} interlock_act_avg={:.2}us critical_alerts={}",
                        state_guard.total_commands_dispatched,
                        state_guard.total_commands_rejected,
                        scheduler_guard.queue_depth(),
                        state_guard.avg_interlock_activation_latency_us(),
                        state_guard.total_critical_ground_alerts,
                    );
                }

                if state_guard.scheduler_tick_count % 100 == 0 && state_guard.telemetry_started {
                    state_guard.print_summary();
                }
            }
        });
    }

    {
        let state_for_processor = Arc::clone(&state);
        let scheduler_for_processor = Arc::clone(&scheduler);
        let logs_for_processor = logs.clone();

        tokio::spawn(async move {
            while let Some(event) = telemetry_rx.recv().await {
                let mut state_guard = state_for_processor.lock().await;
                if state_guard.session_finished {
                    break;
                }

                state_guard.mark_telemetry_dequeued();

                let mut scheduler_guard = scheduler_for_processor.lock().await;
                process_telemetry_event(
                    &mut state_guard,
                    &mut scheduler_guard,
                    &logs_for_processor,
                    &event,
                );
            }
        });
    }

    let mut buf = [0u8; EXPECTED_PACKET_SIZE];
    let mut idle_tick = time::interval(CONTACT_IDLE_TIMEOUT);
    idle_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let _ = idle_tick.tick().await;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!();
                println!("[GCS] Ctrl+C received. Stopping receiver...");
                let mut state_guard = state.lock().await;
                state_guard.session_finished = true;
                print_final_report(&state_guard);
                return Ok(());
            }

            _ = idle_tick.tick() => {
                let now = std::time::Instant::now();
                let mut state_guard = state.lock().await;

                if state_guard.session_finished {
                    return Ok(());
                }

                if state_guard.telemetry_started {
                    if let Some(last_rx) = state_guard.last_receive_time {
                        let silent_for = now.duration_since(last_rx);
                        let end_grace = post_stream_end_grace();

                        if silent_for >= end_grace {
                            println!();
                            println!(
                                "[GCS] Telemetry session ended after {:?} of silence. Finalizing report...",
                                end_grace
                            );
                            state_guard.session_finished = true;
                            print_final_report(&state_guard);
                            return Ok(());
                        }

                        if silent_for >= CONTACT_IDLE_TIMEOUT {
                            state_guard.consecutive_failures += 1;

                            if !state_guard.loss_of_contact_active {
                                println!(
                                    "[GCS][WARN] No telemetry received within {:?} (consecutive_failures={})",
                                    CONTACT_IDLE_TIMEOUT,
                                    state_guard.consecutive_failures
                                );
                            }

                            if state_guard.consecutive_failures >= LOSS_OF_CONTACT_CONSECUTIVE_FAILURES
                                && silent_for < end_grace
                            {
                                if !state_guard.loss_of_contact_active {
                                    state_guard.loss_of_contact_active = true;
                                    state_guard.loss_of_contact_count += 1;
                                    println!("[GCS][ALERT] Loss of contact due to idle timeout.");

                                    let activated =
                                        state_guard.raise_or_touch_fault(FaultKind::LossOfContact, now);

                                    if activated {
                                        let activation_latency_us = 0u128;
                                        state_guard.record_interlock_activation_latency(
                                            FaultKind::LossOfContact,
                                            activation_latency_us,
                                        );
                                        let session_time_us =
                                            state_guard.session_start.elapsed().as_micros();
                                        logs.fault.log_line(&format!(
                                            "{},0,FaultActivated,LossOfContact,activation_latency_us=0",
                                            session_time_us
                                        ));
                                    }

                                    drop(state_guard);

                                    let mut state_guard = state.lock().await;
                                    if state_guard.session_finished {
                                        return Ok(());
                                    }

                                    let mut scheduler_guard = scheduler.lock().await;
                                    scheduler_guard.enqueue_fault_response_commands();
                                    scheduler_guard.dispatch_ready(&mut state_guard);
                                } else {
                                    state_guard.raise_or_touch_fault(FaultKind::LossOfContact, now);
                                }
                            }
                        }
                    }
                }
            }

            recv_result = telemetry_socket.recv_from(&mut buf) => {
                match recv_result {
                    Ok((len, from_addr)) => {
                        let received_at = std::time::Instant::now();

                        let decode_start = std::time::Instant::now();
                        match TelemetryPacket::decode(&buf[..len]) {
                            Ok(packet) => {
                                let decode_time_us = decode_start.elapsed().as_micros();

                                if decode_time_us > TELEMETRY_DECODE_DEADLINE_US {
                                    println!(
                                        "[GCS][WARN] Telemetry decode deadline exceeded: {}us > {}us",
                                        decode_time_us,
                                        TELEMETRY_DECODE_DEADLINE_US
                                    );
                                }

                                let session_start = {
                                    let state_guard = state.lock().await;
                                    if state_guard.session_finished {
                                        return Ok(());
                                    }
                                    state_guard.session_start
                                };

                                let event = TelemetryEvent::from_packet(
                                    packet,
                                    received_at,
                                    decode_time_us,
                                    session_start,
                                );

                                {
                                    let mut state_guard = state.lock().await;
                                    if state_guard.session_finished {
                                        return Ok(());
                                    }
                                    state_guard.mark_telemetry_enqueued();
                                }

                                if telemetry_tx.send(event).await.is_err() {
                                    let mut state_guard = state.lock().await;
                                    if !state_guard.session_finished {
                                        state_guard.mark_telemetry_dequeued();
                                    }
                                    return Ok(());
                                }

                                {
                                    let state_guard = state.lock().await;
                                    if state_guard.session_finished {
                                        return Ok(());
                                    }
                                    if state_guard.total_received % 200 == 0 {
                                        println!(
                                            "[GCS][RX] from={} telemetry_queue_depth={}",
                                            from_addr,
                                            state_guard.pending_telemetry_events
                                        );
                                    }
                                }
                            }
                            Err(err) => {
                                let mut state_guard = state.lock().await;
                                if state_guard.session_finished {
                                    return Ok(());
                                }
                                state_guard.mark_decode_error();
                                println!(
                                    "[GCS][ERROR] Failed to decode packet from {}: {}",
                                    from_addr, err
                                );
                                let session_time_us = state_guard.session_start.elapsed().as_micros();
                                logs.fault.log_line(&format!(
                                    "{},0,DecodeError,{}",
                                    session_time_us, err
                                ));
                            }
                        }
                    }
                    Err(err) => {
                        println!("[GCS][ERROR] UDP receive failed: {}", err);
                        let session_time_us = {
                            let state_guard = state.lock().await;
                            if state_guard.session_finished {
                                return Ok(());
                            }
                            state_guard.session_start.elapsed().as_micros()
                        };
                        logs.fault.log_line(&format!(
                            "{},0,UdpReceiveError,{}",
                            session_time_us, err
                        ));
                    }
                }
            }
        }
    }
}