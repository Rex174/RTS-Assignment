use std::time::Instant;

use crate::gcs::packet::TelemetryPacket;

#[derive(Debug, Clone)]
pub struct TelemetryEvent {
    pub packet: TelemetryPacket,
    pub received_at: Instant,
    pub decode_time_us: u128,
    pub remote_timestamp_us: u64,
    pub estimated_age_us: i128,
}

impl TelemetryEvent {
    pub fn from_packet(
        packet: TelemetryPacket,
        received_at: Instant,
        decode_time_us: u128,
        session_start: Instant,
    ) -> Self {
        let local_elapsed_us = session_start.elapsed().as_micros() as i128;
        let remote_timestamp_us = packet.timestamp_us;
        let estimated_age_us = local_elapsed_us - remote_timestamp_us as i128;

        Self {
            remote_timestamp_us,
            packet,
            received_at,
            decode_time_us,
            estimated_age_us,
        }
    }
}