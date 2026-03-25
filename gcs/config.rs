use std::time::Duration;

pub const GCS_BIND_ADDR: &str = "127.0.0.1:9002";
pub const GCS_UPLINK_BIND_ADDR: &str = "127.0.0.1:0";
pub const UPLINK_TARGET_ADDR: &str = "127.0.0.1:9000";

pub const EXPECTED_PACKET_SIZE: usize = 27;

pub const MAGIC_0: u8 = 0x4F; // 'O'
pub const MAGIC_1: u8 = 0x43; // 'C'

// Assignment-related thresholds
pub const TELEMETRY_DECODE_DEADLINE_US: u128 = 3_000; // 3 ms
pub const URGENT_COMMAND_DISPATCH_DEADLINE_US: u128 = 2_000; // 2 ms
pub const FAULT_RESPONSE_ALERT_THRESHOLD_US: u128 = 100_000; // 100 ms

// Monitoring defaults
pub const LOSS_OF_CONTACT_CONSECUTIVE_FAILURES: u32 = 3;
pub const CONTACT_IDLE_TIMEOUT: Duration = Duration::from_millis(350);
pub const STALE_TELEMETRY_THRESHOLD: Duration = Duration::from_millis(250);

// Real periodic scheduler tick for Student B metrics
pub const GCS_SCHEDULER_PERIOD: Duration = Duration::from_millis(100);