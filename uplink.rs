use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::fault::FaultFlags;
use crate::metrics::MetricsLogger;
use crate::network::{OcsTelemetryPacket, UdpSender};
use crate::util::ShutdownFlag;

#[derive(Debug)]
pub struct RecentPacketStore {
    capacity: usize,
    order: Mutex<VecDeque<u32>>,
    packets: Mutex<HashMap<u32, OcsTelemetryPacket>>,
}

impl RecentPacketStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            order: Mutex::new(VecDeque::with_capacity(capacity)),
            packets: Mutex::new(HashMap::with_capacity(capacity)),
        }
    }

    pub fn insert(&self, packet: OcsTelemetryPacket) {
        let mut order = self.order.lock().unwrap();
        let mut packets = self.packets.lock().unwrap();

        if packets.contains_key(&packet.sequence) {
            packets.insert(packet.sequence, packet);
            return;
        }

        if order.len() >= self.capacity {
            if let Some(oldest) = order.pop_front() {
                packets.remove(&oldest);
            }
        }

        order.push_back(packet.sequence);
        packets.insert(packet.sequence, packet);
    }

    pub fn get(&self, sequence: u32) -> Option<OcsTelemetryPacket> {
        self.packets.lock().unwrap().get(&sequence).cloned()
    }
}

pub fn start_uplink_server(
    bind_addr: &str,
    gcs_udp_addr: &str,
    _metrics: Arc<MetricsLogger>,
    faults: Arc<FaultFlags>,
    shutdown: Arc<ShutdownFlag>,
    recent_packets: Arc<RecentPacketStore>,
) {
    let bind_addr = bind_addr.to_string();
    let gcs_udp_addr = gcs_udp_addr.to_string();

    std::thread::spawn(move || {
        let listener = match TcpListener::bind(&bind_addr) {
            Ok(listener) => listener,
            Err(err) => {
                eprintln!("[UPLINK][ERROR] Failed to bind {}: {}", bind_addr, err);
                return;
            }
        };

        if let Err(err) = listener.set_nonblocking(true) {
            eprintln!("[UPLINK][ERROR] Failed to set nonblocking listener: {}", err);
            return;
        }

        println!("[UPLINK][READY] Listening for GCS commands on {}", bind_addr);

        while !shutdown.is_set() {
            match listener.accept() {
                Ok((stream, peer)) => {
                    println!("[UPLINK][CONNECT] GCS connected from {}", peer);
                    let gcs_udp_addr = gcs_udp_addr.clone();
                    let faults = faults.clone();
                    let shutdown = shutdown.clone();
                    let recent_packets = recent_packets.clone();

                    std::thread::spawn(move || {
                        if let Err(err) = handle_client(
                            stream,
                            &gcs_udp_addr,
                            faults,
                            shutdown,
                            recent_packets,
                        ) {
                            eprintln!("[UPLINK][ERROR] Client session ended: {}", err);
                        }
                    });
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(err) => {
                    eprintln!("[UPLINK][ERROR] Accept failed: {}", err);
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }

        println!("[UPLINK][STOP] Uplink server shutting down");
    });
}

fn handle_client(
    mut stream: TcpStream,
    gcs_udp_addr: &str,
    faults: Arc<FaultFlags>,
    shutdown: Arc<ShutdownFlag>,
    recent_packets: Arc<RecentPacketStore>,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;

    let reader_stream = stream.try_clone()?;
    let mut reader = BufReader::new(reader_stream);
    let resend_sender = UdpSender::new("0.0.0.0:0", gcs_udp_addr)?;

    loop {
        if shutdown.is_set() {
            let _ = stream.write_all(b"SHUTDOWN\n");
            return Ok(());
        }

        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => return Ok(()),
            Ok(_) => {
                let request = line.trim();
                if request.is_empty() {
                    continue;
                }

                let response = handle_command(request, &resend_sender, &faults, &recent_packets);
                stream.write_all(response.as_bytes())?;
                stream.write_all(b"\n")?;
                stream.flush()?;
            }
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(err) => return Err(err),
        }
    }
}

fn handle_command(
    request: &str,
    resend_sender: &UdpSender,
    faults: &Arc<FaultFlags>,
    recent_packets: &Arc<RecentPacketStore>,
) -> String {
    let mut parts = request.split_whitespace();
    let cmd = parts.next().unwrap_or_default().to_ascii_uppercase();

    match cmd.as_str() {
        "PING" => "OK PONG".to_string(),
        "GET_STATUS" => format!(
            "OK STATUS safe_mode={} fault_active={} fault_ack={}",
            faults.manual_safe_mode.load(std::sync::atomic::Ordering::SeqCst),
            faults.any_fault_active(),
            faults.fault_acknowledged.load(std::sync::atomic::Ordering::SeqCst),
        ),
        "ENTER_SAFE_MODE" => {
            faults.manual_safe_mode.store(true, std::sync::atomic::Ordering::SeqCst);
            println!("[UPLINK][CMD] ENTER_SAFE_MODE");
            "OK ENTER_SAFE_MODE".to_string()
        }
        "EXIT_SAFE_MODE" => {
            if faults.any_fault_active() {
                "REJECT FAULT_ACTIVE".to_string()
            } else {
                faults.manual_safe_mode.store(false, std::sync::atomic::Ordering::SeqCst);
                println!("[UPLINK][CMD] EXIT_SAFE_MODE");
                "OK EXIT_SAFE_MODE".to_string()
            }
        }
        "ACK_FAULT" => {
            faults.fault_acknowledged.store(true, std::sync::atomic::Ordering::SeqCst);
            println!("[UPLINK][CMD] ACK_FAULT");
            "OK ACK_FAULT".to_string()
        }
        "REQUEST_RESEND" => {
            let Some(seq_str) = parts.next() else {
                return "REJECT MISSING_SEQUENCE".to_string();
            };

            let Ok(seq) = seq_str.parse::<u32>() else {
                return "REJECT BAD_SEQUENCE".to_string();
            };

            match recent_packets.get(seq) {
                Some(packet) => {
                    resend_sender.send(&packet);
                    println!("[UPLINK][CMD] REQUEST_RESEND {} -> resent", seq);
                    format!("OK RESENT {}", seq)
                }
                None => {
                    println!("[UPLINK][CMD] REQUEST_RESEND {} -> unavailable", seq);
                    format!("REJECT UNKNOWN_SEQUENCE {}", seq)
                }
            }
        }
        "TRIGGER_DELAY_FAULT" => {
            faults.fault_acknowledged.store(false, std::sync::atomic::Ordering::SeqCst);
            faults.delay_sensors.store(true, std::sync::atomic::Ordering::SeqCst);
            println!("[UPLINK][CMD] TRIGGER_DELAY_FAULT");
            "OK TRIGGER_DELAY_FAULT".to_string()
        }
        "CLEAR_DELAY_FAULT" => {
            faults.delay_sensors.store(false, std::sync::atomic::Ordering::SeqCst);
            println!("[UPLINK][CMD] CLEAR_DELAY_FAULT");
            "OK CLEAR_DELAY_FAULT".to_string()
        }
        "TRIGGER_CORRUPT_FAULT" => {
            faults.fault_acknowledged.store(false, std::sync::atomic::Ordering::SeqCst);
            faults.corrupt_data.store(true, std::sync::atomic::Ordering::SeqCst);
            println!("[UPLINK][CMD] TRIGGER_CORRUPT_FAULT");
            "OK TRIGGER_CORRUPT_FAULT".to_string()
        }
        "CLEAR_CORRUPT_FAULT" => {
            faults.corrupt_data.store(false, std::sync::atomic::Ordering::SeqCst);
            println!("[UPLINK][CMD] CLEAR_CORRUPT_FAULT");
            "OK CLEAR_CORRUPT_FAULT".to_string()
        }
        _ => format!("REJECT UNKNOWN_COMMAND {}", request),
    }
}
