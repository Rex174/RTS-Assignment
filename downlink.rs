use std::{
    io::Write,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use flate2::{write::GzEncoder, Compression};
use crate::{
    buffer::BoundedBuffer,
    metrics::MetricsLogger,
    network::{OcsTelemetryPacket, UdpSender,
              SENSOR_THERMAL, SENSOR_POWER, SENSOR_PAYLOAD,
              FLAG_DEGRADED, FLAG_SAFETY_ALERT, FLAG_NAN},
    util::{SensorData, ShutdownFlag},
};

// ─────────────────────────────────────────────────────────────────────────────
// Internal packet representation (gzip compression step)
// ─────────────────────────────────────────────────────────────────────────────

struct TelemetryPacket {
    sensor_name:     &'static str,
    sensor_id:       u8,
    sequence:        u32,
    original_size:   usize,
    compressed_size: usize,
    raw_value:       f64,
}

fn compress_and_packetize(data: &SensorData, seq: u32) -> TelemetryPacket {
    let raw = format!(
        "SEQ={} SENSOR={} VALUE={:.4} AGE_US={}",
        seq, data.name, data.value,
        data.timestamp.elapsed().as_micros()
    );
    let orig = raw.len();
    let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
    enc.write_all(raw.as_bytes()).unwrap_or(());
    let comp = enc.finish().unwrap_or_default().len();

    let sensor_id = match data.name {
        "Thermal" => SENSOR_THERMAL,
        "Power"   => SENSOR_POWER,
        _         => SENSOR_PAYLOAD,
    };

    TelemetryPacket {
        sensor_name: data.name,
        sensor_id,
        sequence: seq,
        original_size: orig,
        compressed_size: comp,
        raw_value: data.value,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Downlink thread — drains buffer every 100ms, sends UDP to GCS
// ─────────────────────────────────────────────────────────────────────────────

pub fn start_downlink(
    buffer:        Arc<BoundedBuffer>,
    metrics:       Arc<MetricsLogger>,
    shutdown:      Arc<ShutdownFlag>,
    ocs_bind_addr: &str,   // e.g. "0.0.0.0:9001"
    gcs_udp_addr:  &str,   // e.g. "127.0.0.1:9002"
) {
    metrics.downlink_loop_start();

    // Create the UDP sender — bind once, reuse for every packet
    let sender = match UdpSender::new(ocs_bind_addr, gcs_udp_addr) {
        Ok(s)  => Arc::new(s),
        Err(e) => {
            eprintln!("[NET][ERROR] Cannot bind UDP sender: {} — downlink disabled", e);
            return;
        }
    };

    thread::spawn(move || {
        let mut sequence:   u32  = 0;
        let mut degraded:   bool = false;
        let mut total_sent: u64  = 0;

        loop {
            // ── Shutdown check ────────────────────────────────────────────
            if shutdown.is_set() {
                break;
            }

            // ── 5ms init deadline timer (starts before any work) ─────────
            let loop_start  = Instant::now();
            let fill        = buffer.fill_ratio();
            let init_lat_ms = loop_start.elapsed().as_secs_f64() * 1000.0;
            if init_lat_ms > 5.0 {
                metrics.downlink_miss("INIT_DEADLINE", init_lat_ms - 5.0);
            }

            // ── Degraded mode gate ────────────────────────────────────────
            if fill > 0.8 && !degraded {
                degraded = true;
                metrics.downlink_status(fill * 100.0, total_sent, true);
            } else if fill <= 0.8 && degraded {
                degraded = false;
            }

            metrics.downlink_visibility();
            metrics.downlink_status(fill * 100.0, total_sent, degraded);

            // Build flags byte from current system state
            let safety_active = {
                let agg = metrics.agg.lock().unwrap();
                agg.safety_alerts > 0
            };
            let base_flags: u8 =
                if degraded       { FLAG_DEGRADED }      else { 0 } |
                    if safety_active  { FLAG_SAFETY_ALERT }  else { 0 };

            // ── Drain buffer within 30ms visibility window ────────────────
            let window_start = Instant::now();
            let mut pkts: Vec<TelemetryPacket> = Vec::new();

            while let Some(data) = buffer.pop() {
                let pkt = compress_and_packetize(&data, sequence);
                let tx_start = Instant::now();

                // Determine per-packet flags
                let mut flags = base_flags;
                if data.value.is_nan() {
                    flags |= FLAG_NAN;
                }

                // ── Send over UDP to GCS ──────────────────────────────────
                let udp_pkt = OcsTelemetryPacket {
                    sequence:     pkt.sequence,
                    timestamp_us: sender.elapsed_us(),
                    sensor_id:    pkt.sensor_id,
                    value:        if data.value.is_nan() { 0.0 } else { data.value },
                    orig_size:    pkt.original_size as u16,
                    flags,
                };
                sender.send(&udp_pkt);

                metrics.downlink_tx(
                    pkt.sequence,
                    pkt.sensor_name,
                    pkt.original_size,
                    pkt.compressed_size,
                    tx_start.elapsed().as_secs_f64() * 1000.0,
                );
                sequence    = sequence.wrapping_add(1);
                total_sent += 1;
                pkts.push(pkt);
            }

            let elapsed_ms = window_start.elapsed().as_secs_f64() * 1000.0;
            if !pkts.is_empty() {
                if elapsed_ms > 30.0 {
                    metrics.downlink_miss("30ms-window", elapsed_ms - 30.0);
                } else {
                    metrics.downlink_window_result(pkts.len(), elapsed_ms, total_sent);
                }
            }

            // ── Wait for next 100ms cycle (interruptible by shutdown) ─────
            let mut remaining = Duration::from_millis(100);
            while remaining > Duration::ZERO && !shutdown.is_set() {
                let step = remaining.min(Duration::from_millis(10));
                thread::sleep(step);
                remaining = remaining.saturating_sub(step);
            }
        }
    });
}