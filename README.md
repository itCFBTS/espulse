# espulse

A gentle, read-only pulse for your **Pulsar eS Xlite** on Linux: prints the mouse's current
settings over its cable or the eS LINK dongle.

```console
$ espulse
Pulsar eS Xlite, wireless (eS LINK dongle)
profile       : 1
dpi           : 800  (stage 2 of 6)
polling rate  : 1000 Hz
motion sync   : off
turbo mode    : off
angle snap    : off
LOD           : 2  (Bibimbap "Default")
debounce      : 0 ms
rotation      : 0°
battery       : not decoded yet (see --raw)
```

- `-v` adds every DPI stage with its LED colour, button codes, and the dongle's function
  button and LED setting.
- `--json` prints everything `-v` reads as one JSON object (used by the tray widget). Values
  whose encoding is inferred rather than confirmed are flagged.
- `--raw` adds the replies that aren't decoded yet, as hex. Battery percentage is one of the
  open questions: run `espulse --raw` at two different charge levels and compare.

> Not affiliated with or endorsed by Pulsar. Protocol reverse-engineered from USB captures of
> Pulsar's web driver (Bibimbap); tested on mouse and dongle firmware V1.0.

## Read-only by design

espulse sends only the read queries Bibimbap itself sends when it connects, copied byte for
byte from USB captures. Every packet carries the protocol's read flag; there is no code path
that builds a write. Decoded protocol, with what is confirmed versus inferred:
[docs/protocol.md](docs/protocol.md).

Only one program can talk to the mouse at a time, so close any Bibimbap tab first.

## Install

udev rule, so your user can open the mouse without sudo:

```bash
sudo cp udev/71-pulsar-es.rules /etc/udev/rules.d/
sudo udevadm control --reload
```

Then unplug and replug the dongle (or the mouse cable).

Build and install (Rust 1.85+, [rustup](https://rustup.rs)):

```bash
cargo install --path .
```

### Plasma widget (KDE Plasma 6 only)

```bash
kpackagetool6 -t Plasma/Applet -i plasmoid
```

Then enable **espulse** under the system tray's *Configure System Tray → Entries*, or add it
from *Add Widgets*. The tray shows the active DPI with a dot in that stage's LED colour; the
popup shows all DPI stages, polling rate, the toggles, LOD, debounce, rotation and the
dongle's settings. It refreshes every 5 minutes and when opened. To update after pulling
changes: `kpackagetool6 -t Plasma/Applet -u plasmoid`, then
`systemctl --user restart plasma-plasmashell`.

## License

[MIT](LICENSE)
