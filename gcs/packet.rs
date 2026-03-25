use anyhow::{anyhow, bail, Result};

use crate::gcs::config::{EXPECTED_PACKET_SIZE, MAGIC_0, MAGIC_1};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorKind {
    Thermal,
    Power,
    Payload,
    Status,
    Unknown(u8),
}

impl SensorKind {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Thermal,
            1 => Self::Power,
            2 => Self::Payload,
            0xFF => Self::Status,
            other => Self::Unknown(other),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Thermal => "THERMAL",
            Self::Power => "POWER",
            Self::Payload => "PAYLOAD",
            Self::Status => "STATUS",
            Self::Unknown(_) => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PacketFlags {
    pub degraded: bool,
    pub safety_alert: bool,
    pub nan_corruption: bool,
    pub raw: u8,
}

impl PacketFlags {
    pub fn from_byte(value: u8) -> Self {
        Self {
            degraded: value & 0b0000_0001 != 0,
            safety_alert: value & 0b0000_0010 != 0,
            nan_corruption: value & 0b0000_0100 != 0,
            raw: value,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TelemetryPacket {
    pub sequence: u32,
    pub timestamp_us: u64,
    pub sensor: SensorKind,
    pub value: f64,
    pub original_size: u16,
    pub flags: PacketFlags,
    pub checksum: u8,
}

impl TelemetryPacket {
    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() != EXPECTED_PACKET_SIZE {
            bail!(
                "invalid packet size: expected {}, got {}",
                EXPECTED_PACKET_SIZE,
                buf.len()
            );
        }

        if buf[0] != MAGIC_0 || buf[1] != MAGIC_1 {
            bail!(
                "invalid magic: got [0x{:02X}, 0x{:02X}]",
                buf[0],
                buf[1]
            );
        }

        let expected_checksum = xor_checksum(&buf[..26]);
        let received_checksum = buf[26];

        if expected_checksum != received_checksum {
            bail!(
                "checksum mismatch: expected 0x{:02X}, got 0x{:02X}",
                expected_checksum,
                received_checksum
            );
        }

        let sequence = u32::from_be_bytes(
            buf[2..6]
                .try_into()
                .map_err(|_| anyhow!("failed to read sequence"))?,
        );

        let timestamp_us = u64::from_be_bytes(
            buf[6..14]
                .try_into()
                .map_err(|_| anyhow!("failed to read timestamp_us"))?,
        );

        let sensor_id = buf[14];
        let sensor = SensorKind::from_u8(sensor_id);

        let value = f64::from_be_bytes(
            buf[15..23]
                .try_into()
                .map_err(|_| anyhow!("failed to read value"))?,
        );

        let original_size = u16::from_be_bytes(
            buf[23..25]
                .try_into()
                .map_err(|_| anyhow!("failed to read original_size"))?,
        );

        let flags = PacketFlags::from_byte(buf[25]);

        Ok(Self {
            sequence,
            timestamp_us,
            sensor,
            value,
            original_size,
            flags,
            checksum: received_checksum,
        })
    }
}

pub fn xor_checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |acc, b| acc ^ b)
}