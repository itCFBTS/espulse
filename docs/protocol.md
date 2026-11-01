# Pulsar eS Xlite (3710:9604, wired) — config protocol notes

Captured with usbmon while Pulsar's web driver (Bibimbap, bbb.pulsar.gg, Chrome WebHID) talked to
the mouse: over its USB cable on 2026-10-26, over the eS LINK dongle (3710:5602) on 2026-10-26/27
and 2026-10-31. Mouse and dongle firmware V1.0.

On 2026-10-31 the meanings were checked against Bibimbap's own JavaScript
(`bbb.pulsar.gg/vMouse/assets/index-*.js`), which names each opcode (`readBatteryLevel`,
`readDpiXyBinding`, …) and has the lookup tables. Each entry says how it was established:
**confirmed** (we changed the setting and saw the command), **source** (Bibimbap's code says so),
**matched** (the value equals what Bibimbap displays).

## Transport (confirmed)

- Command: HID SET_REPORT, Feature report id `0x0e`, interface 1 (`21 09 030e 0001 0040`).
  Bibimbap finds the interface by its vendor usage page (0xff05) or by feature report 14.
- Reply: HID GET_REPORT, same report (`a1 01 030e 0001 0040`).
- Payload: `0e a5 <op> <dir> <len> <data…>`. `dir` is `01` = read, `02` = write. Bibimbap
  pads read requests with a zero byte where the reply has one (e.g. a stage read is
  `0e a5 04 01 06 <stage> 00`).
- Bibimbap polls GET_REPORT every ~10 ms (up to 30 tries) until the reply echoes the opcode. It
  skips three kinds of reply (source):
  - `0e a5 ff`: not ready (seen over the dongle);
  - our opcode with direction `03`: busy, poll again;
  - wired only: `00 00 00 00 88 0f` before the mouse answers (confirmed in captures).
- After a write it waits 80 ms (400 ms for `0x14`/`0x15`, 1500 ms for `0x17`) before the next
  command (source).
- Motion reports are on interrupt endpoint 2 (report id 1, 16-bit X/Y), unrelated.

## Opcodes

Names are Bibimbap's function names. Byte offsets are into `<data…>`.

| Op | Len | Bibimbap name | Meaning | Status |
|---|---|---|---|---|
| `0x01` | 2 | `readFirmwareVersion` | Mouse firmware **build**, u16 LE: `56 00` = 0056. Pulsar's server maps it to the shown version ("V 1.0") | source + matched |
| `0x02` | 2 | `readDpiSelection` | stage count, active stage (1-based). Editing a stage makes it active | confirmed |
| `0x03` | 4 | `readDpiLed` | per stage: `<stage> R G B` stage colour | matched |
| `0x04` | 6 | `readDpiValue` | per stage: `<stage> <bound> <X u16-LE> <Y u16-LE>`. `bound` 1 = X and Y linked, 0 = separate | confirmed |
| `0x05` | 1 | `readDebounce` | 0 / 2 / 4 ms = `00` / `01` / `02` (the page's slider runs 0–4 ms) | confirmed |
| `0x06` | 1 | `readDpiXyBinding` | "Enable X-Y": `01` = linked (off), `00` = separate (on) | source + matched |
| `0x07` | 1 | `readReportRate` | 125 / 500 / 1k / 2k / 4k / 8k Hz = 0 / 1 / 2 / 3 / 4 / 5. The page offers 500 Hz and up | confirmed (1–5) |
| `0x08` | 2 | `readSensorSetting` | byte 0 = "gaming" mode, shown as Turbo Mode; byte 1 = Motion Sync | confirmed |
| `0x09` | 1 | `readLod` | 0.7 / 0.8 / 1.0 / 1.2 / 1.4 mm = 1 / 2 / 4 / 7 / 11. 0.8 mm is "Default" | confirmed |
| `0x0a` | 2 | `readMousePid` | mouse USB PID, u16 LE. Bibimbap never calls it | source only |
| `0x0b` | 1 | `readBatteryLevel` | battery = value × 5 %. `ff` = no reading yet | source + matched |
| `0x0c` | 2 | `readMacroSetting` | per button `<button> <function>`, see below | confirmed |
| `0x0d` | 2 | `readDongleFirmwareVersion` | dongle firmware build: `30 00` = 0030, same as its USB bcdDevice | source + matched |
| `0x0f` | 1 | `readWirelessStatus` | `01` = mouse connected to the dongle | source + matched |
| `0x10` | 1 | `readBatteryChargingStatus` | `01` = charging. Read over the dongle only | source |
| `0x14` | 1 | `readCurrentProfile` | active profile 1–4 | confirmed |
| `0x15` | 4 | `readProfileOnStatus` | which of profiles 1–4 are enabled. Bibimbap never reads it | source only |
| `0x18` | 2 | `readAngleTune` | rotation: byte 0 = on, byte 1 = signed degrees (−30…30). −5° = `01 fb`, +30° = `01 1e`, 0° = `00 00` | confirmed |
| `0x19` | 1 | `readColorId` | colourway ID. Bibimbap never reads it | source only |
| `0x1b` | 1 | `readDongleLeftButton` | dongle function button: 1 = Profile, 2 = Polling Rate, 3 = Turbo Mode | confirmed |
| `0x1c` | 1 | `readAngleSnap` | Angle Snap (0/1) | confirmed |
| `0x1d` | 5 | `readStatusChangeFlags` | "changed" flags: profile, DPI, turbo, polling rate, battery level. Bibimbap polls it every 500 ms and rereads whatever changed | source |
| `0x1e` | 1 | `readDongleLed` | dongle "Function & DPI LED": 1 = on | confirmed |
| `0x1f` | 2 | `readMouseLedControl` | byte 0 = LED effect (0 off, 1 steady), byte 1 = brightness 1–10 | confirmed |

**Write-only commands espulse must never send:** `0x17` = factory reset, `0x1a` = start dongle
pairing.

### Buttons (`0x0c`)

Selectors `b1`–`b5` = left, right, wheel, back, forward. Function codes: 1 Left Click, 2 Right
Click, 3 Wheel Click, 4 Backward, 5 Forward, 6 Disable, 7 Page Down, 8 Full Screenshot, 9 Region
Capture, 10 Cut, 11 Copy, 12 Paste. Setting forward to Backward wrote `b5 04`. Every remap is
followed by a `0x14` write of the current profile, apparently to save it.

## Over the dongle (eS LINK Dongle 3710:5602)

- Same report, framing and opcodes as wired.
- Bibimbap reads `0x0d`, `0x0f`, `0x10`, `0x1b` and `0x1e` only over the dongle.
- 2026-10-27: battery 100% → 70% changed only `0x0b` (`14` → `0e`); `0x01` stayed `56 00`.
- 2026-10-31: `0x1d` stayed all zeros through 921 polls while settings were changed from the
  page (it flags changes made on the mouse itself). Nothing unprompted arrived on the interrupt
  endpoint except motion.

## Other devices

Bibimbap's device list routes these to the same page: VID `3710` PIDs `9501`, `9502`, `9503`,
`9600`, `9604`, `9605`, and VID `3057` PIDs `2002`, `4000`, `8000`, `1014`–`1017`.

## Not yet tested

- **Charging:** `0x10` (and whether `0x0b` changes) with the cable plugged in while connected
  over the dongle.
- **Wired reads:** `0x0b` over the cable, and the mouse's own bcdDevice.
- **No link:** what the dongle answers with the mouse switched off (`0x0f` = 0? `0x0b` = `ff`?).

Write captures: 2026-10-26 rounds 1–2 and 2026-10-31 round A. Every setting was restored
afterwards, except stage 4 (3200 × 3600) and Enable X-Y, which the user chose to keep.
