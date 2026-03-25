// network.rs — OCS ↔ GCS UDP telemetry protocol
//
// Packet wire format (27 bytes fixed, no external crates):
//
//  Offset  Len  Field
//  ──────  ───  ─────────────────────────────────────────────────────
//    0      2   Magic bytes: 0x4F 0x43  ("OC" = OCS packet)
//    2      4   Sequence number (u32, big-endian)
//    6      8   Timestamp µs since OCS start (u64, big-endian)
//   14      1   Sensor ID  0=Thermal 1=Power 2=Payload 0xFF=status
//   15      8   Sensor value (f64, big-endian IEEE-754)
//   23      2   Original payload size in bytes (u16, big-endian)
//   25      1   Flags  bit0=degraded  bit1=safety_alert  bit2=nan
//   26      1   XOR checksum of bytes 0..25
//  ──────  ───
//   Total: 27 bytes per packet

use std::net::UdpSocket;
use std::time::Instant;

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

pub const PACKET_SIZE:   usize = 27;
pub const MAGIC:         [u8; 2] = [0x4F, 0x43]; // "OC"

pub const SENSOR_THERMAL: u8 = 0;
pub const SENSOR_POWER:   u8 = 1;
pub const SENSOR_PAYLOAD: u8 = 2;
pub const SENSOR_STATUS:  u8 = 0xFF;

pub const FLAG_DEGRADED:      u8 = 0b0000_0001;
pub const FLAG_SAFETY_ALERT:  u8 = 0b0000_0010;
pub const FLAG_NAN:           u8 = 0b0000_0100;

// ─────────────────────────────────────────────────────────────────────────────
// OcsTelemetryPacket — decoded representation used by both sides
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OcsTelemetryPacket {
    pub sequence:     u32,
    pub timestamp_us: u64,
    pub sensor_id:    u8,
    pub value:        f64,
    pub orig_size:    u16,
    pub flags:        u8,
}

impl OcsTelemetryPacket {
    /// Encode the packet into 27 bytes ready for UDP transmission.
    pub fn encode(&self) -> [u8; PACKET_SIZE] {
        let mut buf = [0u8; PACKET_SIZE];

        // Magic
        buf[0] = MAGIC[0];
        buf[1] = MAGIC[1];

        // Sequence (u32 big-endian)
        buf[2..6].copy_from_slice(&self.sequence.to_be_bytes());

        // Timestamp µs (u64 big-endian)
        buf[6..14].copy_from_slice(&self.timestamp_us.to_be_bytes());

        // Sensor ID
        buf[14] = self.sensor_id;

        // Value (f64 big-endian)
        buf[15..23].copy_from_slice(&self.value.to_be_bytes());

        // Original size (u16 big-endian)
        buf[23..25].copy_from_slice(&self.orig_size.to_be_bytes());

        // Flags
        buf[25] = self.flags;

        // XOR checksum over bytes 0..26
        buf[26] = buf[0..26].iter().fold(0u8, |acc, &b| acc ^ b);

        buf
    }

    /// Decode 27 bytes received over UDP into a packet.
    /// Returns None if magic bytes are wrong or checksum fails.
    pub fn decode(buf: &[u8; PACKET_SIZE]) -> Option<Self> {
        // Magic check
        if buf[0] != MAGIC[0] || buf[1] != MAGIC[1] {
            return None;
        }

        // Checksum check — XOR of bytes 0..26 must equal byte 26
        let expected = buf[0..26].iter().fold(0u8, |acc, &b| acc ^ b);
        if expected != buf[26] {
            return None;
        }

        let sequence     = u32::from_be_bytes(buf[2..6].try_into().unwrap());
        let timestamp_us = u64::from_be_bytes(buf[6..14].try_into().unwrap());
        let sensor_id    = buf[14];
        let value        = f64::from_be_bytes(buf[15..23].try_into().unwrap());
        let orig_size    = u16::from_be_bytes(buf[23..25].try_into().unwrap());
        let flags        = buf[25];

        Some(Self { sequence, timestamp_us, sensor_id, value, orig_size, flags })
    }

    /// Human-readable sensor name from the ID byte.
    pub fn sensor_name(&self) -> &'static str {
        match self.sensor_id {
            SENSOR_THERMAL => "THERMAL",
            SENSOR_POWER   => "POWER",
            SENSOR_PAYLOAD => "PAYLOAD",
            SENSOR_STATUS  => "STATUS",
            _              => "UNKNOWN",
        }
    }

    pub fn is_degraded(&self)     -> bool { self.flags & FLAG_DEGRADED     != 0 }
    pub fn is_safety_alert(&self) -> bool { self.flags & FLAG_SAFETY_ALERT != 0 }
    pub fn is_nan(&self)          -> bool { self.flags & FLAG_NAN          != 0 }
}

// ─────────────────────────────────────────────────────────────────────────────
// UdpSender — OCS side: wraps a bound UDP socket for fire-and-forget sending
// ─────────────────────────────────────────────────────────────────────────────

pub struct UdpSender {
    socket:   UdpSocket,
    gcs_addr: String,
    pub start: Instant,
}

impl UdpSender {
    /// Bind to `bind_addr` and target GCS at `gcs_addr`.
    /// Example: UdpSender::new("0.0.0.0:9001", "127.0.0.1:9002")
    pub fn new(bind_addr: &str, gcs_addr: &str) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(bind_addr)?;
        // Non-blocking: if GCS is not listening, send() fails silently
        // rather than blocking the downlink thread.
        socket.set_nonblocking(true)?;
        println!("[NET][INIT] UDP sender bound {} → {}", bind_addr, gcs_addr);
        Ok(Self {
            socket,
            gcs_addr: gcs_addr.to_string(),
            start: Instant::now(),
        })
    }

    /// Send a telemetry packet to the GCS.
    /// Errors are logged but never propagate — fire and forget.
    pub fn send(&self, pkt: &OcsTelemetryPacket) {
        let buf = pkt.encode();
        match self.socket.send_to(&buf, &self.gcs_addr) {
            Ok(_)  => {}
            Err(e) => eprintln!("[NET][WARN] UDP send failed: {}", e),
        }
    }

    /// Elapsed microseconds since the sender was created (used for timestamps).
    pub fn elapsed_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }
}