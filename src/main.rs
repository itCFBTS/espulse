//! espulse — read-only status of the Pulsar eS Xlite (wired 3710:9604, eS LINK dongle 3710:5602).
//!
//! Every packet it sends is one of the read queries Pulsar's web driver (Bibimbap) sends when it
//! connects, copied byte for byte from USB captures. The direction byte is always 0x01 (read);
//! there is no code path that builds a write. Field meanings follow Bibimbap's own source
//! (opcode names, lookup tables) and were checked against captures. Protocol notes: docs/protocol.md.
//! Needs the hidraw uaccess udev rule from udev/71-pulsar-es.rules (no sudo).

use std::cell::RefCell;
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
/// (wired: 00 00 00 00 88 0f; dongle: 0e a5 ff) or a "busy" one (our op with direction 03).
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
    /// Every reply received, in order, for --raw.
    log: RefCell<Vec<(Query, Vec<u8>)>>,
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
                    self.log.borrow_mut().push((query, d.clone()));
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

/// Opens the mouse and returns it with the reply to `probe`, the first read sent to each candidate.
fn open_mouse(probe: Query) -> (Mouse, Vec<u8>) {
    let candidates = find_candidates();
    if candidates.is_empty() {
        eprintln!("Pulsar eS Xlite not found: plug in the dongle or the mouse cable.");
        exit(1);
    }
    for (node, wireless) in &candidates {
        match OpenOptions::new().read(true).write(true).open(node) {
            Ok(file) => {
                let m = Mouse { file, node: node.clone(), wireless: *wireless, log: RefCell::default() };
                if let Some(d) = m.read(probe) {
                    return (m, d);
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

/// Polling rate in Hz for opcode 0x07 (Bibimbap's table; its page offers 500 Hz and up).
fn polling_hz(code: u8) -> Option<u32> {
    match code {
        0 => Some(125),
        1 => Some(500),
        2 => Some(1000),
        3 => Some(2000),
        4 => Some(4000),
        5 => Some(8000),
        _ => None,
    }
}

fn polling(code: u8) -> String {
    polling_hz(code).map(|hz| format!("{hz} Hz")).unwrap_or_else(|| format!("unknown code {code}"))
}

/// Battery level from opcode 0x0b, in 5% steps (Bibimbap multiplies by 5). 0xff means the mouse has
/// no reading yet; anything else above 20 is not a level Bibimbap expects.
fn battery_percent(raw: u8) -> Option<u8> {
    (raw <= 20).then(|| raw * 5)
}

/// Lift-off distance in mm for opcode 0x09 (Bibimbap's table; 0.8 mm is its "Default").
fn lod_mm(code: u8) -> Option<&'static str> {
    match code {
        1 => Some("0.7"),
        2 => Some("0.8"),
        4 => Some("1.0"),
        7 => Some("1.2"),
        11 => Some("1.4"),
        _ => None,
    }
}

/// Debounce in ms for opcode 0x05: Bibimbap writes 0, 2 and 4 ms as 0, 1 and 2.
fn debounce_ms(code: u8) -> Option<u8> {
    (code <= 2).then(|| code * 2)
}

/// Button function for opcode 0x0c (Bibimbap's list).
fn button_function(code: u8) -> &'static str {
    match code {
        1 => "Left Click",
        2 => "Right Click",
        3 => "Wheel Click",
        4 => "Backward",
        5 => "Forward",
        6 => "Disable",
        7 => "Page Down",
        8 => "Full Screenshot",
        9 => "Region Capture",
        10 => "Cut",
        11 => "Copy",
        12 => "Paste",
        _ => "unknown",
    }
}

/// Physical button for each 0x0c selector.
const BUTTONS: [(u8, &str); 5] = [(0xb1, "left"), (0xb2, "right"), (0xb3, "wheel"), (0xb4, "back"), (0xb5, "forward")];

/// Firmware build as Bibimbap formats it (high byte then low, in hex): `56 00` → "0056". Pulsar's
/// server maps builds to the version Bibimbap shows (0056 and dongle 0030 both show as V1.0).
fn firmware(d: &[u8]) -> Option<String> {
    (d.len() >= 2).then(|| format!("{:02x}{:02x}", d[1], d[0]))
}

#[derive(Default)]
struct Stage {
    n: u8,
    x: u16,
    y: u16,
    colour: Option<[u8; 3]>,
}

/// Everything espulse reads, gathered once; printed as text or JSON.
#[derive(Default)]
struct Status {
    wireless: bool,
    node: String,
    profile: Option<u8>,
    stage_count: Option<u8>,
    active_stage: Option<u8>,
    stages: Vec<Stage>,
    polling_code: Option<u8>,
    motion_sync: Option<bool>,
    turbo: Option<bool>,
    angle_snap: Option<bool>,
    lod: Option<u8>,
    debounce_raw: Option<u8>,
    rotation: Option<[u8; 2]>,
    xy_linked: Option<bool>,
    led: Option<[u8; 2]>,
    buttons: Vec<(u8, Option<u8>)>,
    dongle_button: Option<u8>,
    dongle_led: Option<bool>,
    linked: Option<bool>,
    charging: Option<bool>,
    battery_raw: Option<u8>,
    mouse_fw: Option<String>,
    dongle_fw: Option<String>,
}

fn collect(m: &Mouse, full: bool) -> Status {
    let read = |query: Query| m.read(query);
    let mut st = Status { wireless: m.wireless, node: m.node.display().to_string(), ..Default::default() };
    st.profile = read(q(0x14, 1)).map(|d| d[0]);
    if let Some(s) = read(q(0x02, 2)) {
        st.stage_count = Some(s[0]);
        st.active_stage = Some(s[1]);
        for n in 1..=s[0].min(8) {
            if let Some(d) = read(qa(0x04, 6, n)).filter(|d| d.len() >= 6) {
                let colour = if full { read(qa(0x03, 4, n)).filter(|c| c.len() >= 4).map(|c| [c[1], c[2], c[3]]) } else { None };
                st.stages.push(Stage { n, x: u16::from_le_bytes([d[2], d[3]]), y: u16::from_le_bytes([d[4], d[5]]), colour });
            }
        }
    }
    st.polling_code = read(q(0x07, 1)).map(|d| d[0]);
    if let Some(d) = read(q(0x08, 2)) {
        st.turbo = Some(d[0] != 0);
        st.motion_sync = Some(d[1] != 0);
    }
    st.angle_snap = read(q(0x1c, 1)).map(|d| d[0] != 0);
    st.lod = read(q(0x09, 1)).map(|d| d[0]);
    st.debounce_raw = read(q(0x05, 1)).map(|d| d[0]);
    st.rotation = read(q(0x18, 2)).filter(|d| d.len() >= 2).map(|d| [d[0], d[1]]);
    st.xy_linked = read(q(0x06, 1)).map(|d| d[0] == 1);
    st.battery_raw = read(q(0x0b, 1)).map(|d| d[0]);
    if m.wireless {
        // Bibimbap reads these only over the dongle.
        st.linked = read(q(0x0f, 1)).map(|d| d[0] == 1);
        st.charging = read(q(0x10, 1)).map(|d| d[0] == 1);
    }
    if full {
        st.led = read(q(0x1f, 2)).filter(|d| d.len() >= 2).map(|d| [d[0], d[1]]);
        st.buttons = BUTTONS.iter().map(|&(b, _)| (b, read(qa(0x0c, 2, b)).and_then(|d| d.get(1).copied()))).collect();
        st.mouse_fw = read(q(0x01, 2)).as_deref().and_then(firmware);
        if m.wireless {
            st.dongle_fw = read(q(0x0d, 2)).as_deref().and_then(firmware);
            st.dongle_button = read(q(0x1b, 1)).map(|d| d[0]);
            st.dongle_led = read(q(0x1e, 1)).map(|d| d[0] != 0);
        }
    }
    st
}

/// Opcode 0x18: byte 0 = rotation on, byte 1 = signed degrees (Bibimbap's range is -30..30).
fn rotation_text(r: [u8; 2]) -> String {
    match (r[0], r[1] as i8) {
        (0, _) | (_, 0) => "0°".to_string(),
        (1, n) => format!("{n:+}°"),
        _ => format!("raw {}", hex(&r)),
    }
}

fn led_text(l: [u8; 2]) -> String {
    match l[0] {
        0 => format!("off (brightness {})", l[1]),
        1 => format!("steady, brightness {}", l[1]),
        e => format!("effect {e}, brightness {}", l[1]),
    }
}

fn dongle_button_name(code: u8) -> &'static str {
    match code {
        1 => "Profile",
        2 => "Polling Rate",
        3 => "Turbo Mode",
        _ => "unknown",
    }
}

/// `show_raw` adds the raw battery byte.
fn print_text(st: &Status, verbose: bool, show_raw: bool) {
    let mode = if st.wireless { "wireless (eS LINK dongle)" } else { "wired" };
    if verbose {
        println!("Pulsar eS Xlite, {mode}   [{}]", st.node);
    } else {
        println!("Pulsar eS Xlite, {mode}");
    }
    if let Some(p) = st.profile {
        println!("profile       : {p}");
    }
    if let (Some(count), Some(active)) = (st.stage_count, st.active_stage)
        && let Some(s) = st.stages.iter().find(|s| s.n == active)
    {
        let dpi = if s.x == s.y { s.x.to_string() } else { format!("{} × {}", s.x, s.y) };
        println!("dpi           : {dpi}  (stage {active} of {count})");
    }
    if st.xy_linked == Some(false) {
        println!("X-Y           : separate");
    }
    if let Some(c) = st.polling_code {
        println!("polling rate  : {}", polling(c));
    }
    if let Some(v) = st.motion_sync {
        println!("motion sync   : {}", on_off(v as u8));
    }
    if let Some(v) = st.turbo {
        println!("turbo mode    : {}", on_off(v as u8));
    }
    if let Some(v) = st.angle_snap {
        println!("angle snap    : {}", on_off(v as u8));
    }
    if let Some(v) = st.lod {
        let t = match lod_mm(v) {
            Some(mm) if v == 2 => format!("{mm} mm  (Default)"),
            Some(mm) => format!("{mm} mm"),
            None => format!("unknown code {v}"),
        };
        println!("LOD           : {t}");
    }
    if let Some(v) = st.debounce_raw {
        let t = debounce_ms(v).map(|ms| format!("{ms} ms")).unwrap_or_else(|| format!("unknown code {v}"));
        println!("debounce      : {t}");
    }
    if let Some(r) = st.rotation {
        println!("rotation      : {}", rotation_text(r));
    }
    match st.battery_raw {
        Some(r) => match battery_percent(r) {
            Some(pct) if show_raw => println!("battery       : {pct}%  (0x0b = {r:02x}, {r} × 5)"),
            Some(pct) => println!("battery       : {pct}%"),
            None => println!("battery       : unexpected reply 0x{r:02x}"),
        },
        None => println!("battery       : no reply"),
    }
    if st.charging == Some(true) {
        println!("charging      : yes");
    }
    if st.linked == Some(false) {
        println!("link          : dongle reports no mouse connected");
    }
    if verbose {
        let list: Vec<String> = st
            .stages
            .iter()
            .map(|s| {
                let c = s.colour.map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])).unwrap_or("?".into());
                if Some(s.n) == st.active_stage { format!("[{} {c}]", s.x) } else { format!("{} {c}", s.x) }
            })
            .collect();
        println!("dpi stages    : {}  (active bracketed)", list.join("  "));
        if let Some(l) = st.led {
            println!("mouse LED     : {}", led_text(l));
        }
        let buttons: Vec<String> = st
            .buttons
            .iter()
            .map(|&(b, f)| {
                let name = BUTTONS.iter().find(|(c, _)| *c == b).map(|(_, n)| *n).unwrap_or("?");
                format!("{name}={}", f.map(button_function).unwrap_or("?"))
            })
            .collect();
        println!("buttons       : {}", buttons.join("  "));
        if let Some(b) = st.dongle_button {
            println!("dongle button : {}", dongle_button_name(b));
        }
        if let Some(v) = st.dongle_led {
            println!("dongle LED    : {}", on_off(v as u8));
        }
        if let Some(f) = &st.mouse_fw {
            let dongle = st.dongle_fw.as_ref().map(|d| format!(", dongle {d}")).unwrap_or_default();
            println!("firmware      : mouse {f}{dongle}  (build numbers; both show as V1.0 in Bibimbap)");
        }
    }
}

fn json_opt<T: ToString>(v: Option<T>) -> String {
    v.map(|x| x.to_string()).unwrap_or("null".into())
}

fn json_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Machine-readable output for the tray widget. Values that are only inferred are flagged.
fn print_json(st: &Status) {
    let stages: Vec<String> = st
        .stages
        .iter()
        .map(|s| {
            let c = s.colour.map(|c| json_str(&format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))).unwrap_or("null".into());
            format!("{{\"n\":{},\"x\":{},\"y\":{},\"colour\":{c}}}", s.n, s.x, s.y)
        })
        .collect();
    let fields = [
        format!("\"mode\":{}", json_str(if st.wireless { "wireless" } else { "wired" })),
        format!("\"profile\":{}", json_opt(st.profile)),
        format!("\"stage_count\":{}", json_opt(st.stage_count)),
        format!("\"active_stage\":{}", json_opt(st.active_stage)),
        format!("\"stages\":[{}]", stages.join(",")),
        format!("\"polling_code\":{}", json_opt(st.polling_code)),
        format!("\"polling_hz\":{}", json_opt(st.polling_code.and_then(polling_hz))),
        format!("\"motion_sync\":{}", json_opt(st.motion_sync)),
        format!("\"turbo\":{}", json_opt(st.turbo)),
        format!("\"angle_snap\":{}", json_opt(st.angle_snap)),
        format!("\"lod\":{}", json_opt(st.lod)),
        format!("\"lod_mm\":{}", json_opt(st.lod.and_then(lod_mm))),
        format!("\"debounce_raw\":{}", json_opt(st.debounce_raw)),
        format!("\"debounce_ms\":{}", json_opt(st.debounce_raw.and_then(debounce_ms))),
        format!("\"rotation\":{}", json_opt(st.rotation.map(|r| json_str(&rotation_text(r))))),
        format!("\"xy_linked\":{}", json_opt(st.xy_linked)),
        format!("\"led\":{}", json_opt(st.led.map(|l| json_str(&led_text(l))))),
        format!(
            "\"buttons\":{{{}}}",
            st.buttons
                .iter()
                .filter_map(|&(b, f)| Some(format!("{}:{}", json_str(BUTTONS.iter().find(|(c, _)| *c == b)?.1), json_str(button_function(f?)))))
                .collect::<Vec<_>>()
                .join(",")
        ),
        format!("\"dongle_button\":{}", json_opt(st.dongle_button.map(|b| json_str(dongle_button_name(b))))),
        format!("\"dongle_led\":{}", json_opt(st.dongle_led)),
        format!("\"battery\":{}", json_opt(st.battery_raw.and_then(battery_percent))),
        format!("\"charging\":{}", json_opt(st.charging)),
        format!("\"linked\":{}", json_opt(st.linked)),
        format!("\"mouse_fw\":{}", json_opt(st.mouse_fw.as_deref().map(json_str))),
        format!("\"dongle_fw\":{}", json_opt(st.dongle_fw.as_deref().map(json_str))),
    ];
    println!("{{{}}}", fields.join(","));
}

fn main() {
    let mut verbose = false;
    let mut raw = false;
    let mut json = false;
    let mut battery_only = false;
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "-v" | "--verbose" => verbose = true,
            "--raw" => raw = true,
            "--json" => json = true,
            "-b" | "--battery" => battery_only = true,
            "-h" | "--help" => {
                println!("espulse — read-only status of the Pulsar eS Xlite\n");
                println!("  -v, --verbose   also show all DPI stages with colours, button codes and dongle settings");
                println!("  --raw           also dump every reply received, as hex");
                println!("  --json          everything -v reads, as one JSON object (for the tray widget)");
                println!("  -b, --battery   battery % only, one read query (for scripts/widgets); exit 3 = unexpected reply");
                return;
            }
            other => {
                eprintln!("unknown option: {other} (try --help)");
                exit(2);
            }
        }
    }
    if battery_only {
        // The battery read doubles as the probe, so this is the only query sent.
        let (_, d) = open_mouse(q(0x0b, 1));
        match battery_percent(d[0]) {
            Some(pct) => println!("{pct}"),
            None => {
                eprintln!("unexpected battery reply 0x{:02x}", d[0]);
                exit(3);
            }
        }
        return;
    }
    // Probe with the first read Bibimbap makes (active profile).
    let (m, _) = open_mouse(q(0x14, 1));
    let st = collect(&m, verbose || json);
    if json {
        print_json(&st);
        return;
    }
    print_text(&st, verbose, raw || verbose);

    if raw {
        println!("\nraw replies:");
        for (query, d) in m.log.borrow().iter() {
            let arg = query.arg.map(|a| format!(" [{a:02x}]")).unwrap_or_default();
            println!("  0x{:02x}{arg}: {}", query.op, hex(d));
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
    fn busy_replies_are_rejected() {
        // Bibimbap keeps polling when the reply echoes the op with direction 03.
        assert_eq!(parse_reply(0x0b, &[0x0e, 0xa5, 0x0b, 0x03, 0x01, 0x12]), None);
    }

    #[test]
    fn decoded_values_from_the_capture() {
        // Writes captured 2026-10-31 alongside the values Bibimbap showed.
        assert_eq!(rotation_text([0x01, 0xfb]), "-5°");
        assert_eq!(rotation_text([0x01, 0xe2]), "-30°");
        assert_eq!(rotation_text([0x01, 0x1e]), "+30°");
        assert_eq!(rotation_text([0x00, 0x00]), "0°");
        assert_eq!(lod_mm(2), Some("0.8"));
        assert_eq!(lod_mm(11), Some("1.4"));
        assert_eq!(debounce_ms(2), Some(4));
        assert_eq!(polling_hz(1), Some(500));
        assert_eq!(polling_hz(4), Some(4000));
        assert_eq!(button_function(4), "Backward");
        assert_eq!(firmware(&[0x56, 0x00]).as_deref(), Some("0056"));
        assert_eq!(firmware(&[0x30, 0x00]).as_deref(), Some("0030"));
        assert_eq!(led_text([0x01, 0x0a]), "steady, brightness 10");
    }

    #[test]
    fn battery_steps() {
        assert_eq!(battery_percent(0x14), Some(100)); // Bibimbap: 100%
        assert_eq!(battery_percent(0x0e), Some(70)); // Bibimbap: 70%
        assert_eq!(battery_percent(0), Some(0));
        assert_eq!(battery_percent(0xff), None);
    }

    #[test]
    fn ioctl_numbers() {
        // HIDIOCSFEATURE(64) and HIDIOCGFEATURE(64) from <linux/hidraw.h>.
        assert_eq!(hid_ioc(6, 64), 0xC040_4806);
        assert_eq!(hid_ioc(7, 64), 0xC040_4807);
    }
}
