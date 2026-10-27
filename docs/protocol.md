# Pulsar eS Xlite (3710:9604, wired) — config protocol notes

Captured 2026-10-26 with usbmon while Pulsar's web driver (Bibimbap, bbb.pulsar.gg, Chrome
WebHID) talked to the mouse over its USB cable. Mouse firmware V1.0. Dongle not yet captured.
Each entry says how it was established: **confirmed** (changed the setting and saw the
command), **matched** (value equals what Bibimbap displays), or **inferred** (fits, untested).

## Transport (confirmed)

- Command: HID SET_REPORT, Feature report id `0x0e`, interface 1 (`21 09 030e 0001 0040`),
  64-byte payload.
- Reply: HID GET_REPORT, same report (`a1 01 030e 0001 0040`). The host polls until the reply
  echoes the opcode; until then the device answers `00 00 00 00 88 0f` (inferred: "not ready").
- Payload: `0e a5 <op> <dir> <len> <data…>`, dir `01` = read, `02` = write. After every write
  Bibimbap reads the same op back.
- Motion reports are on interrupt endpoint 2 (report id 1, 16-bit X/Y), unrelated.

## Opcodes

| Op | Len | Meaning | Status |
|---|---|---|---|
| `0x02` | 2 | stage count, active stage (1-based) — `06 02` = 6 stages, stage 2 | matched |
| `0x03` | 4 | per stage: `<stage> R G B` stage colour | matched (6/6) |
| `0x04` | 6 | per stage: `<stage> 01 <X u16-LE> <Y u16-LE>` DPI | matched (6/6); the `01` byte unknown |
| `0x07` | 1 | polling rate: 2 = 1 kHz, 3 = 2 kHz | confirmed; 500 Hz = 1, 4 kHz = 4, 8 kHz = 5 inferred |
| `0x08` | 2 | byte 0 = Turbo Mode, byte 1 = Motion Sync (0/1) | confirmed |
| `0x1c` | 1 | Angle Snap (0/1) | confirmed |
| `0x14` | 1 | active profile: 1 → `01`, 2 → `02` | confirmed |
| `0x09` | 1 | LOD: slider "Default" notch → `02`, one notch higher → `04` | confirmed op; scale (2 per notch?) from 2 points only |
| `0x05` | 1 | Debounce: page range value 0 → `00`, 2 → `01` | confirmed op; ms scale unconfirmed |
| `0x1b` | 1 | Dongle function button: 1 = Profile, 2 = Polling Rate, 3 = Turbo Mode | confirmed (over the dongle) |
| `0x1e` | 1 | Dongle "Function & DPI LED": 1 = on, 0 = off | confirmed (over the dongle) |
| `0x18` | 2 | Rotation: 0° → `00 00`, +5° → `01 05` (sign/direction byte + degrees?) | confirmed op; negative encoding untested |
| `0x1f` | 2 | `00 05` with LED effect Off, brightness 5 | inferred |
| `0x0c` | 2 | per button `b1`…`b5`: `<button> <function>`; all defaults (1–5) | inferred (button map) |
| `0x06` | 1 | `1` (not the profile — that is `0x14`) | unknown |
| `0x0b` | 1 | battery in 5% steps: `0x14` (20) = 100%, `0x0e` (14) = 70% | matched (2 levels) |
| `0x01` | 2 | `56 00` (86) at both 100% and 70% battery | unknown (not battery) |
| `0x0d`, `0x0f`, `0x10` | 2, 1, 1 | `30 00`, `01`, `00`; `0x10` only seen over the dongle | unknown |
| `0x1d` | 5 | polled every ~0.7 s throughout; last reply changed `…00` → `…01` | unknown (live status) |

## Over the dongle (eS LINK Dongle 3710:5602, dongle FW V1.0) — 2026-10-26

- Same report, framing and opcodes as wired; every value matched the wired handshake.
- "Not ready" reply differs: `0e a5 ff` over the dongle (wired: `00 00 00 00 88 0f`).
- Handshake order starts with `0x0d` (`30 00`), `0x0f` (`01`), `0x1e` (`01`, read ×3), and also
  reads `0x10` (`00`, not seen wired) and `0x1b` (`01`). `0x1b` and `0x1e` confirmed by a write
  round (function button, LED); every value restored afterwards.
- `0x01` = `56 00` (86) again while Bibimbap showed **100%, not charging**. Both sightings of
  `0x56` were at (or right next to) 100%, so it is not a plain percentage but could still be a
  raw/scaled battery value — one charge level can't tell. No reply contained 100 (`0x64`), and
  all interrupt reports were motion (report id 1). Test: compare replies at a lower charge.
- 2026-10-27, wireless, Bibimbap showing **70%**: only `0x0b` had changed, `0x14` → `0x0e`
  (20 → 14; × 5 = 100 → 70). `0x01` stayed `56 00`, so it isn't battery. Settings had changed
  between the two readings (8 kHz, Turbo on, 4 stages), but landing exactly on
  the displayed % by coincidence is unlikely. Untested: the value while charging, and over the cable.
- `0x1d` polled ~every 0.5 s, all zeros over the dongle.

## Not yet tested

Debounce ms scale, LOD scale beyond two points, negative rotation, 500 Hz / 4 kHz / 8 kHz
codes, battery while charging / over the cable, `0x1d`, the reads seen only during a profile switch, and the dongle
(wireless) path. Write captures: 2026-10-26 round 1 (Motion Sync, Angle Snap, Turbo, polling)
and round 2 (debounce, LOD, rotation, profile); every setting was restored afterwards.
