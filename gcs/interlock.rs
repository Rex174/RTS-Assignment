use crate::gcs::command::{Command, CommandKind};
use crate::gcs::fault::FaultKind;
use crate::gcs::state::MonitorState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterlockDecision {
    Allow,
    Reject(RejectReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    SafetyAlertActive,
    DegradedModeActive,
    CorruptedTelemetryActive,
    LossOfContactActive,
    StaleTelemetryActive,
}

pub fn evaluate_command(state: &MonitorState, command: &Command) -> InterlockDecision {
    if state.has_fault(FaultKind::LossOfContact) {
        return match command.kind {
            CommandKind::EnterSafeMode | CommandKind::AcknowledgeFault => InterlockDecision::Allow,
            _ => InterlockDecision::Reject(RejectReason::LossOfContactActive),
        };
    }

    if state.has_fault(FaultKind::SafetyAlert) {
        return match command.kind {
            CommandKind::EnterSafeMode | CommandKind::AcknowledgeFault | CommandKind::PayloadHold => {
                InterlockDecision::Allow
            }
            _ => InterlockDecision::Reject(RejectReason::SafetyAlertActive),
        };
    }

    if state.has_fault(FaultKind::CorruptedTelemetry) {
        return match command.kind {
            CommandKind::AcknowledgeFault | CommandKind::RequestStatusRefresh | CommandKind::PayloadHold => {
                InterlockDecision::Allow
            }
            _ => InterlockDecision::Reject(RejectReason::CorruptedTelemetryActive),
        };
    }

    if state.has_fault(FaultKind::DegradedMode) {
        return match command.kind {
            CommandKind::AcknowledgeFault
            | CommandKind::RequestStatusRefresh
            | CommandKind::EnterSafeMode
            | CommandKind::PayloadHold => InterlockDecision::Allow,
            _ => InterlockDecision::Reject(RejectReason::DegradedModeActive),
        };
    }

    if state.has_fault(FaultKind::StaleTelemetry) {
        return match command.kind {
            CommandKind::AcknowledgeFault
            | CommandKind::RequestStatusRefresh
            | CommandKind::EnterSafeMode => InterlockDecision::Allow,
            _ => InterlockDecision::Reject(RejectReason::StaleTelemetryActive),
        };
    }

    InterlockDecision::Allow
}