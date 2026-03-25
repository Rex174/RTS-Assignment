use std::collections::VecDeque;
use std::time::Instant;

use tokio::sync::mpsc::UnboundedSender;

use crate::gcs::command::{Command, CommandKind, CommandPriority};
use crate::gcs::interlock::{evaluate_command, InterlockDecision};
use crate::gcs::logger::CsvLogger;
use crate::gcs::state::MonitorState;

pub struct CommandScheduler {
    urgent_queue: VecDeque<Command>,
    normal_queue: VecDeque<Command>,
    next_command_id: u64,
    logger: CsvLogger,
    uplink_tx: UnboundedSender<Command>,
}

impl CommandScheduler {
    pub fn new(logger: CsvLogger, uplink_tx: UnboundedSender<Command>) -> Self {
        Self {
            urgent_queue: VecDeque::new(),
            normal_queue: VecDeque::new(),
            next_command_id: 1,
            logger,
            uplink_tx,
        }
    }

    pub fn enqueue_demo_commands(&mut self) {
        let request_status_refresh_id = self.take_next_id();
        self.enqueue(Command::new(
            request_status_refresh_id,
            CommandKind::RequestStatusRefresh,
            CommandPriority::Normal,
        ));

        if self.next_command_id % 2 == 0 {
            let resume_nominal_ops_id = self.take_next_id();
            self.enqueue(Command::new(
                resume_nominal_ops_id,
                CommandKind::ResumeNominalOps,
                CommandPriority::Normal,
            ));
        } else {
            let payload_resume_id = self.take_next_id();
            self.enqueue(Command::new(
                payload_resume_id,
                CommandKind::PayloadResume,
                CommandPriority::Normal,
            ));
        }
    }

    pub fn enqueue_fault_response_commands(&mut self) {
        let enter_safe_mode_id = self.take_next_id();
        let acknowledge_fault_id = self.take_next_id();
        let payload_hold_id = self.take_next_id();

        let commands = [
            Command::new(
                enter_safe_mode_id,
                CommandKind::EnterSafeMode,
                CommandPriority::Urgent,
            ),
            Command::new(
                acknowledge_fault_id,
                CommandKind::AcknowledgeFault,
                CommandPriority::Normal,
            ),
            Command::new(
                payload_hold_id,
                CommandKind::PayloadHold,
                CommandPriority::Normal,
            ),
        ];

        for command in commands {
            self.enqueue(command);
        }
    }

    fn take_next_id(&mut self) -> u64 {
        let id = self.next_command_id;
        self.next_command_id += 1;
        id
    }

    pub fn enqueue(&mut self, command: Command) {
        match command.priority {
            CommandPriority::Urgent => self.urgent_queue.push_back(command),
            CommandPriority::Normal => self.normal_queue.push_back(command),
        }
    }

    pub fn dispatch_ready(&mut self, state: &mut MonitorState) {
        while let Some(command) = self.pop_next() {
            self.dispatch_one(state, command);
        }
    }

    fn pop_next(&mut self) -> Option<Command> {
        if let Some(command) = self.urgent_queue.pop_front() {
            return Some(command);
        }

        self.normal_queue.pop_front()
    }

    fn dispatch_one(&mut self, state: &mut MonitorState, command: Command) {
        let dispatch_started_at = Instant::now();
        let queue_wait = dispatch_started_at.duration_since(command.created_at);
        let session_time_us = state.session_start.elapsed().as_micros();

        match evaluate_command(state, &command) {
            InterlockDecision::Allow => {
                let dispatch_latency = dispatch_started_at.duration_since(command.created_at);
                let deadline_missed = dispatch_latency > command.dispatch_deadline;

                state.total_commands_dispatched += 1;

                if matches!(command.priority, CommandPriority::Urgent) {
                    state.record_urgent_dispatch_latency(
                        dispatch_latency.as_micros() as f64,
                        deadline_missed,
                    );
                }

                if deadline_missed {
                    state.total_command_deadline_misses += 1;
                    println!(
                        "[GCS][CMD] DISPATCH id={} kind={} priority={} wait={}us DEADLINE_MISS",
                        command.id,
                        command.kind.as_str(),
                        command.priority.as_str(),
                        dispatch_latency.as_micros()
                    );
                } else {
                    println!(
                        "[GCS][CMD] DISPATCH id={} kind={} priority={} wait={}us",
                        command.id,
                        command.kind.as_str(),
                        command.priority.as_str(),
                        dispatch_latency.as_micros()
                    );
                }

                self.logger.log_line(&format!(
                    "{},{},{},{},dispatch,ALLOW,{},-",
                    session_time_us,
                    command.id,
                    command.kind.as_str(),
                    command.priority.as_str(),
                    queue_wait.as_micros()
                ));

                if self.uplink_tx.send(command.clone()).is_ok() {
                    println!(
                        "[GCS][UPLINK] QUEUED id={} kind={} priority={}",
                        command.id,
                        command.kind.as_str(),
                        command.priority.as_str()
                    );

                    self.logger.log_line(&format!(
                        "{},{},{},{},uplink_queue,QUEUED,0,-",
                        session_time_us,
                        command.id,
                        command.kind.as_str(),
                        command.priority.as_str()
                    ));
                } else {
                    println!(
                        "[GCS][UPLINK][ERROR] queue send failed for id={} kind={} priority={}",
                        command.id,
                        command.kind.as_str(),
                        command.priority.as_str()
                    );

                    self.logger.log_line(&format!(
                        "{},{},{},{},uplink_queue,QUEUE_FAIL,0,-",
                        session_time_us,
                        command.id,
                        command.kind.as_str(),
                        command.priority.as_str()
                    ));
                }
            }
            InterlockDecision::Reject(reason) => {
                state.total_commands_rejected += 1;

                let first_block_latency_us =
                    state.record_first_blocked_command(reason, dispatch_started_at);

                match first_block_latency_us {
                    Some(latency_us) => {
                        println!(
                            "[GCS][CMD] REJECT id={} kind={} priority={} reason={:?} first_block_latency={}us",
                            command.id,
                            command.kind.as_str(),
                            command.priority.as_str(),
                            reason,
                            latency_us
                        );
                    }
                    None => {
                        println!(
                            "[GCS][CMD] REJECT id={} kind={} priority={} reason={:?}",
                            command.id,
                            command.kind.as_str(),
                            command.priority.as_str(),
                            reason
                        );
                    }
                }

                self.logger.log_line(&format!(
                    "{},{},{},{},dispatch,REJECT:{:?},{},-",
                    session_time_us,
                    command.id,
                    command.kind.as_str(),
                    command.priority.as_str(),
                    reason,
                    queue_wait.as_micros()
                ));
            }
        }
    }

    pub fn queue_depth(&self) -> usize {
        self.urgent_queue.len() + self.normal_queue.len()
    }
}