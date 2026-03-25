use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultKind {
    SafetyAlert,
    DegradedMode,
    CorruptedTelemetry,
    LossOfContact,
    StaleTelemetry,
}

#[derive(Debug, Clone)]
pub struct ActiveFault {
    pub kind: FaultKind,
    pub started_at: Instant,
    pub last_seen_at: Instant,
    pub first_blocked_command_latency_us: Option<u128>,
}

impl ActiveFault {
    pub fn new(kind: FaultKind, now: Instant) -> Self {
        Self {
            kind,
            started_at: now,
            last_seen_at: now,
            first_blocked_command_latency_us: None,
        }
    }

    pub fn touch(&mut self, now: Instant) {
        self.last_seen_at = now;
    }
}