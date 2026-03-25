use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    AcknowledgeFault,
    RequestStatusRefresh,
    EnterSafeMode,
    ResumeNominalOps,
    PayloadHold,
    PayloadResume,
}

impl CommandKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandKind::AcknowledgeFault => "AcknowledgeFault",
            CommandKind::RequestStatusRefresh => "RequestStatusRefresh",
            CommandKind::EnterSafeMode => "EnterSafeMode",
            CommandKind::ResumeNominalOps => "ResumeNominalOps",
            CommandKind::PayloadHold => "PayloadHold",
            CommandKind::PayloadResume => "PayloadResume",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandPriority {
    Normal,
    Urgent,
}

impl CommandPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandPriority::Normal => "Normal",
            CommandPriority::Urgent => "Urgent",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Command {
    pub id: u64,
    pub kind: CommandKind,
    pub priority: CommandPriority,
    pub created_at: Instant,
    pub dispatch_deadline: Duration,
}

impl Command {
    pub fn new(id: u64, kind: CommandKind, priority: CommandPriority) -> Self {
        let dispatch_deadline = match priority {
            CommandPriority::Urgent => Duration::from_micros(2_000),
            CommandPriority::Normal => Duration::from_millis(50),
        };

        Self {
            id,
            kind,
            priority,
            created_at: Instant::now(),
            dispatch_deadline,
        }
    }

    pub fn to_wire_line(&self) -> String {
        format!(
            "CMD,{},{},{}\n",
            self.id,
            self.kind.as_str(),
            self.priority.as_str()
        )
    }
}