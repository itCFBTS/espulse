//! espulse — read-only status of the Pulsar eS Xlite (wired 3710:9604, eS LINK dongle 3710:5602).
//!
//! Every packet it sends is one of the read queries Pulsar's web driver (Bibimbap) sends when it
//! connects, copied byte for byte from USB captures. The direction byte is always 0x01 (read);
//! there is no code path that builds a write. Protocol notes: docs/protocol.md.
//! Needs the hidraw uaccess udev rule from udev/71-pulsar-es.rules (no sudo).

use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::process::exit;
use std::thread::sleep;
use std::time::Duration;

const VENDOR: &str = "00003710";
const PID_WIRED: &str = "00009604";
const PID_DONGLE: &str = "00005602";
/// Feature report carrying the config protocol (interface 1).
const REPORT_ID: u8 = 0x0e;
const MAGIC: u8 = 0xa5;
const DIR_READ: u8 = 0x01;
const REPORT_LEN: usize = 64;

/// One read query exactly as Bibimbap sends it: opcode, reply data length, optional selector.
#[derive(Clone, Copy)]
struct Query {
    op: u8,
    len: u8,
    arg: Option<u8>,
}

const fn q(op: u8, len: u8) -> Query {
    Query { op, len, arg: None }
}
const fn qa(op: u8, len: u8, arg: u8) -> Query {
    Query { op, len, arg: Some(arg) }
}

/// The 64-byte feature report for a read query. Always DIR_READ.
fn packet(query: Query) -> [u8; REPORT_LEN] {
    let mut p = [0u8; REPORT_LEN];
    p[0] = REPORT_ID;
    p[1] = MAGIC;
    p[2] = query.op;
    p[3] = DIR_READ;
    p[4] = query.len;
    if let Some(a) = query.arg {
        p[5] = a;
    }
    p
}

/// Data bytes of a reply that answers `op`, or None if it is a "not ready" reply
/// (wired: 00 00 00 00 88 0f; dongle: 0e a5 ff).
fn parse_reply(op: u8, r: &[u8]) -> Option<Vec<u8>> {
    if r.len() < 5 || r[0] != REPORT_ID || r[1] != MAGIC || r[2] != op || r[3] != DIR_READ {
        return None;
    }
    let n = (r[4] as usize).min(r.len() - 5);
    Some(r[5..5 + n].to_vec())
}

/// _IOC(_IOC_READ | _IOC_WRITE, 'H', nr, len) — HIDIOCSFEATURE is nr 6, HIDIOCGFEATURE nr 7.
fn hid_ioc(nr: libc::c_ulong, len: usize) -> libc::c_ulong {
    (3 << 30) | ((len as libc::c_ulong) << 16) | ((b'H' as libc::c_ulong) << 8) | nr
}

struct Mouse {
    file: File,
    node: PathBuf,
    wireless: bool,
}

impl Mouse {
    /// Send one read query and poll for its reply, the way Bibimbap does (~10 ms apart).
    fn read(&self, query: Query) -> Option<Vec<u8>> {
        let fd = self.file.as_raw_fd();
        for _attempt in 0..3 {
            let out = packet(query);
            // SAFETY: valid fd, buffer of exactly REPORT_LEN bytes matching the ioctl size.
            if unsafe { libc::ioctl(fd, hid_ioc(6, REPORT_LEN), out.as_ptr()) } < 0 {
                return None;
            }
            for _ in 0..60 {
                sleep(Duration::from_millis(10));
                let mut buf = [0u8; REPORT_LEN];
                buf[0] = REPORT_ID;
                // SAFETY: as above; the kernel writes at most REPORT_LEN bytes.
                let n = unsafe { libc::ioctl(fd, hid_ioc(7, REPORT_LEN), buf.as_mut_ptr()) };
                if n < 0 {
                    return None;
                }
                if let Some(d) = parse_reply(query.op, &buf[..n as usize]) {
                    return Some(d);
                }
            }
        }
        None
    }
}

/// hidraw nodes of the mouse or dongle, the config interface (bInterfaceNumber 01) first.
fn find_candidates() -> Vec<(PathBuf, bool)> {
    let mut found: Vec<(u8, PathBuf, bool)> = Vec::new();
    let Ok(rd) = fs::read_dir("/sys/class/hidraw") else { return Vec::new() };
    for entry in rd.flatten() {
        let sys = entry.path();
        let Ok(uevent) = fs::read_to_string(sys.join("device/uevent")) else { continue };
        let wireless = if uevent.contains(&format!("{VENDOR}:{PID_DONGLE}")) {
            true
        } else if uevent.contains(&format!("{VENDOR}:{PID_WIRED}")) {
            false
        } else {
            continue;
        };
        let iface = fs::canonicalize(sys.join("device"))
            .ok()
            .and_then(|hid| fs::read_to_string(hid.parent()?.join("bInterfaceNumber")).ok())
            .and_then(|s| u8::from_str_radix(s.trim(), 16).ok())
            .unwrap_or(0xFF);
        found.push((iface, PathBuf::from("/dev").join(entry.file_name()), wireless));
    }
    found.sort_by_key(|(i, _, _)| if *i == 1 { 0 } else { 1 + *i as u16 });
    found.into_iter().map(|(_, p, w)| (p, w)).collect()
}

fn open_mouse() -> Mouse {
    let candidates = find_candidates();
    if candidates.is_empty() {
        eprintln!("Pulsar eS Xlite not found: plug in the dongle or the mouse cable.");
        exit(1);
    }
    for (node, wireless) in &candidates {
        match OpenOptions::new().read(true).write(true).open(node) {
            Ok(file) => {
                let m = Mouse { file, node: node.clone(), wireless: *wireless };
                // Probe with the first read Bibimbap makes (active profile).
                if m.read(q(0x14, 1)).is_some() {
                    return m;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!(
                    "No permission to open {}. Install udev/71-pulsar-es.rules, then unplug and replug the dongle or cable.",
                    node.display()
                );
                exit(1);
            }
            Err(e) => eprintln!("note: cannot open {} ({e})", node.display()),
        }
    }
    eprintln!(
        "No reply from the mouse. Is Bibimbap open in a browser tab? Close it (only one program can talk to the mouse at a time). If wireless, move the mouse to wake it."
    );
    exit(1);
}

fn on_off(b: u8) -> &'static str {
    if b == 0 { "off" } else { "on" }
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

fn polling(code: u8) -> String {
    // 2 and 3 confirmed by changing the setting; the rest follow the page's order.
    match code {
        2 => "1000 Hz".into(),
        3 => "2000 Hz".into(),
        1 => "500 Hz (code 1, inferred)".into(),
        4 => "4000 Hz (code 4, inferred)".into(),
        5 => "8000 Hz (code 5, inferred)".into(),
        c => format!("unknown code {c}"),
    }
}

fn main() {
    let mut verbose = false;
    let mut raw = false;
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "-v" | "--verbose" => verbose = true,
            "--raw" => raw = true,
            "-h" | "--help" => {
                println!("espulse — read-only status of the Pulsar eS Xlite\n");
                println!("  -v, --verbose   also show all DPI stages with colours, button codes and dongle settings");
                println!("  --raw           also dump the replies we haven't decoded yet (battery research)");
                return;
            }
            other => {
                eprintln!("unknown option: {other} (try --help)");
                exit(2);
            }
        }
    }
    let m = open_mouse();
    let read = |query: Query| m.read(query);
    let mode = if m.wireless { "wireless (eS LINK dongle)" } else { "wired" };
    if verbose {
        println!("Pulsar eS Xlite, {mode}   [{}]", m.node.display());
    } else {
        println!("Pulsar eS Xlite, {mode}");
    }

    if let Some(d) = read(q(0x14, 1)) {
        println!("profile       : {}", d[0]);
    }
    let stages = read(q(0x02, 2));
    let mut dpis: Vec<(u8, u16, u16)> = Vec::new();
    if let Some(s) = &stages {
        let (count, active) = (s[0], s[1]);
        for i in 1..=count.min(8) {
            if let Some(d) = read(qa(0x04, 6, i)) {
                if d.len() >= 6 {
                    dpis.push((i, u16::from_le_bytes([d[2], d[3]]), u16::from_le_bytes([d[4], d[5]])));
                }
            }
        }
        if let Some((_, x, y)) = dpis.iter().find(|(i, _, _)| *i == active) {
            let dpi = if x == y { x.to_string() } else { format!("{x} × {y}") };
            println!("dpi           : {dpi}  (stage {active} of {count})");
        }
    }
    if let Some(d) = read(q(0x07, 1)) {
        println!("polling rate  : {}", polling(d[0]));
    }
    if let Some(d) = read(q(0x08, 2)) {
        println!("motion sync   : {}", on_off(d[1]));
        println!("turbo mode    : {}", on_off(d[0]));
    }
    if let Some(d) = read(q(0x1c, 1)) {
        println!("angle snap    : {}", on_off(d[0]));
    }
    if let Some(d) = read(q(0x09, 1)) {
        let note = if d[0] == 2 { "  (Bibimbap \"Default\")" } else { "" };
        println!("LOD           : {}{note}", d[0]);
    }
    if let Some(d) = read(q(0x05, 1)) {
        let v = if d[0] == 0 { "0 ms".to_string() } else { format!("raw {} (ms scale unconfirmed)", d[0]) };
        println!("debounce      : {v}");
    }
    if let Some(d) = read(q(0x18, 2)) {
        let v = match (d[0], d[1]) {
            (0, 0) => "0°".to_string(),
            (1, n) => format!("+{n}°"),
            _ => format!("raw {}", hex(&d)),
        };
        println!("rotation      : {v}");
    }
    println!("battery       : not decoded yet (see --raw)");

    if verbose {
        let colours: Vec<String> = (1..=dpis.len() as u8)
            .map(|i| read(qa(0x03, 4, i)).filter(|d| d.len() >= 4).map(|d| format!("#{:02x}{:02x}{:02x}", d[1], d[2], d[3])).unwrap_or("?".into()))
            .collect();
        let active = stages.as_ref().map(|s| s[1]).unwrap_or(0);
        let list: Vec<String> = dpis
            .iter()
            .zip(&colours)
            .map(|((i, x, _), c)| if *i == active { format!("[{x} {c}]") } else { format!("{x} {c}") })
            .collect();
        println!("dpi stages    : {}  (active bracketed)", list.join("  "));
        let buttons: Vec<String> = [0xb1u8, 0xb2, 0xb3, 0xb4, 0xb5]
            .iter()
            .map(|&b| read(qa(0x0c, 2, b)).map(|d| format!("{:02x}={}", b, d.get(1).copied().unwrap_or(0))).unwrap_or(format!("{b:02x}=?")))
            .collect();
        println!("button codes  : {}  (1–5 = factory defaults; mapping unconfirmed)", buttons.join(" "));
        if m.wireless {
            if let Some(d) = read(q(0x1b, 1)) {
                let f = match d[0] {
                    1 => "Profile",
                    2 => "Polling Rate",
                    3 => "Turbo Mode",
                    _ => "unknown",
                };
                println!("dongle button : {f}");
            }
            if let Some(d) = read(q(0x1e, 1)) {
                println!("dongle LED    : {}", on_off(d[0]));
            }
        }
    }

    if raw {
        println!("\nundecoded replies (same reads Bibimbap makes):");
        let mut unknown = vec![q(0x01, 2), q(0x06, 1), q(0x0b, 1), q(0x0d, 2), q(0x0f, 1), q(0x1d, 5), q(0x1f, 2)];
        if m.wireless {
            unknown.push(q(0x10, 1));
        }
        for u in unknown {
            match read(u) {
                Some(d) => println!("  0x{:02x}: {}", u.op, hex(&d)),
                None => println!("  0x{:02x}: no reply", u.op),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_are_always_reads() {
        for query in [q(0x14, 1), qa(0x04, 6, 3), qa(0x0c, 2, 0xb5), q(0x1d, 5)] {
            let p = packet(query);
            assert_eq!(p[0], REPORT_ID);
            assert_eq!(p[1], MAGIC);
            assert_eq!(p[3], DIR_READ);
        }
    }

    #[test]
    fn packets_match_the_capture() {
        // From the wired handshake: 0e a5 04 01 06 01 00…  and  0e a5 0c 01 02 b1 00…
        assert_eq!(&packet(qa(0x04, 6, 1))[..6], &[0x0e, 0xa5, 0x04, 0x01, 0x06, 0x01]);
        assert_eq!(&packet(qa(0x0c, 2, 0xb1))[..6], &[0x0e, 0xa5, 0x0c, 0x01, 0x02, 0xb1]);
        assert!(packet(q(0x07, 1))[5..].iter().all(|&b| b == 0));
    }

    #[test]
    fn replies_from_the_capture() {
        assert_eq!(parse_reply(0x01, &[0x0e, 0xa5, 0x01, 0x01, 0x02, 0x56, 0x00]), Some(vec![0x56, 0x00]));
        let stage = [0x0e, 0xa5, 0x04, 0x01, 0x06, 0x02, 0x01, 0x20, 0x03, 0x20, 0x03];
        let d = parse_reply(0x04, &stage).unwrap();
        assert_eq!(u16::from_le_bytes([d[2], d[3]]), 800);
    }

    #[test]
    fn not_ready_replies_are_rejected() {
        assert_eq!(parse_reply(0x14, &[0x00, 0x00, 0x00, 0x00, 0x88, 0x0f]), None); // wired
        assert_eq!(parse_reply(0x14, &[0x0e, 0xa5, 0xff]), None); // dongle
        assert_eq!(parse_reply(0x14, &[0x0e, 0xa5, 0x07, 0x01, 0x01, 0x02]), None); // other op
    }

    #[test]
    fn ioctl_numbers() {
        // HIDIOCSFEATURE(64) and HIDIOCGFEATURE(64) from <linux/hidraw.h>.
        assert_eq!(hid_ioc(6, 64), 0xC040_4806);
        assert_eq!(hid_ioc(7, 64), 0xC040_4807);
    }
}
