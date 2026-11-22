//! Konsolen finden (SRCH) und aufwecken (WAKEUP) über UDP.
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

pub const PORT_PS4: u16 = 987;
pub const PORT_PS5: u16 = 9302;
const VER_PS4: &str = "00020020";
const VER_PS5: &str = "00030010";

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Ready,
    Standby,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Console {
    pub addr: String,
    pub name: String,
    pub id: String,
    pub is_ps5: bool,
    pub state: State,
    pub system_version: String,
    pub running_app: String,
}

fn parse_response(text: &str, addr: String) -> Option<Console> {
    let mut lines = text.lines();
    let first = lines.next()?;
    let code: u32 = first.split_whitespace().nth(1)?.parse().ok()?;
    let state = match code {
        200 => State::Ready,
        620 => State::Standby,
        _ => State::Unknown,
    };
    let mut h: HashMap<&str, &str> = HashMap::new();
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            h.insert(k.trim(), v.trim());
        }
    }
    let is_ps5 = h.get("device-discovery-protocol-version") == Some(&VER_PS5);
    Some(Console {
        addr,
        name: h.get("host-name").unwrap_or(&"?").to_string(),
        id: h.get("host-id").unwrap_or(&"").to_string(),
        is_ps5,
        state,
        system_version: h.get("system-version").unwrap_or(&"").to_string(),
        running_app: h.get("running-app-name").unwrap_or(&"").to_string(),
    })
}

fn srch(ver: &str) -> String {
    format!("SRCH * HTTP/1.1\ndevice-discovery-protocol-version:{ver}\n")
}

/// Eigene lokale IP des PCs (es wird kein Paket gesendet).
pub fn local_ip() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    match s.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(v) => Some(v),
        _ => None,
    }
}

fn bad_input(e: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string())
}

/// Sucht Konsolen im lokalen Netz (Broadcast) oder gezielt bei `target_ip`.
pub fn discover(target_ip: Option<&str>, timeout: Duration) -> std::io::Result<Vec<Console>> {
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.set_broadcast(true)?;
    sock.set_read_timeout(Some(Duration::from_millis(200)))?;

    let mut dests: Vec<String> = Vec::new();
    if let Some(ip) = target_ip {
        dests.push(ip.to_string());
    } else {
        dests.push(Ipv4Addr::BROADCAST.to_string());
        if let Some(ip) = local_ip() {
            let o = ip.octets();
            dests.push(format!("{}.{}.{}.255", o[0], o[1], o[2]));
        }
    }
    for dest in &dests {
        for (port, ver) in [(PORT_PS4, VER_PS4), (PORT_PS5, VER_PS5)] {
            let addr: SocketAddr = format!("{dest}:{port}").parse().map_err(bad_input)?;
            let _ = sock.send_to(srch(ver).as_bytes(), addr);
        }
    }

    let mut found: HashMap<String, Console> = HashMap::new();
    let start = Instant::now();
    let mut buf = [0u8; 2048];
    while start.elapsed() < timeout {
        if let Ok((n, from)) = sock.recv_from(&mut buf) {
            let text = String::from_utf8_lossy(&buf[..n]);
            if let Some(c) = parse_response(&text, from.ip().to_string()) {
                found.insert(c.addr.clone(), c);
            }
        }
    }
    Ok(found.into_values().collect())
}

/// Weckt eine Konsole aus dem Ruhemodus. `credential` ist die Remote-Play-User-Credential.
pub fn wakeup(host: &str, credential: u64, ps5: bool) -> std::io::Result<()> {
    let (port, ver) = if ps5 { (PORT_PS5, VER_PS5) } else { (PORT_PS4, VER_PS4) };
    let msg = format!(
        "WAKEUP * HTTP/1.1\nclient-type:vr\nauth-type:R\nmodel:w\napp-type:r\nuser-credential:{credential}\ndevice-discovery-protocol-version:{ver}\n"
    );
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    let addr: SocketAddr = format!("{host}:{port}").parse().map_err(bad_input)?;
    for _ in 0..3 {
        sock.send_to(msg.as_bytes(), addr)?;
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
