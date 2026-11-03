# espulse

Read-only status tool for the **Pulsar eS Xlite** on Linux. Prints the mouse's battery and
current settings over its cable or the eS LINK dongle.

```console
$ espulse
Pulsar eS Xlite, wireless (eS LINK dongle)
profile       : 1
dpi           : 800  (stage 2 of 6)
polling rate  : 1000 Hz
motion sync   : off
turbo mode    : off
angle snap    : off
LOD           : 0.8 mm  (Default)
debounce      : 0 ms
rotation      : 0°
battery       : 70%
```

- `-v` adds every DPI stage with its LED colour, the mouse LED, button functions, the dongle's
  function button and LED setting, and firmware build numbers.
- `--battery` prints just the battery percentage, for scripts and the tray widget. It sends
  one read query (two over the dongle, which is first asked whether it has a mouse). The mouse reports battery in 5% steps.
- `--battery-json` prints one line in the same shape as `pulsar-mouse --battery-json`
  from [pulsar-mouse-linux](https://github.com/packerlschupfer/pulsar-mouse-linux), so a widget
  can read both tools the same way: `{"wireless":true,"battery_percent":85,"power_connected":false,"battery_mv":null}`.
  `battery_mv` is always null: no voltage read has been found for this mouse.
- `--json` prints everything `-v` reads as one JSON object (used by the tray widget).
- `--raw` adds every reply received, as hex.

`--battery`, `--battery-json` and `--json` each print one fixed format, so they can't be combined
with each other, `-v` or `--raw`.

> Not affiliated with or endorsed by Pulsar. Protocol reverse-engineered from USB captures of
> Pulsar's web driver (Bibimbap); tested on mouse and dongle firmware V1.0.

## Read-only by design

espulse sends only the read queries Bibimbap itself sends when it connects, copied byte for
byte from USB captures. One difference: Bibimbap asks only the dongle whether power is connected,
and espulse also asks the mouse on its cable. Every packet carries the protocol's read flag; there is no code path
that builds a write. Decoded protocol, with where each meaning comes from:
[docs/protocol.md](docs/protocol.md).

Only one program can talk to the mouse at a time, so close any Bibimbap tab first.

With the mouse on its cable espulse reads it over the cable, where it also reports that it's plugged in.
The battery level can jump soon after the cable goes in, so right after plugging in it may not reflect real charge.

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

This puts `espulse` in `~/.cargo/bin`. The tray widget looks there and in `~/.local/bin`.

### Plasma widget (KDE Plasma 6 only)

```bash
kpackagetool6 -t Plasma/Applet -i plasmoid
install -Dm644 plasmoid/contents/icons/es-xlite.svg ~/.local/share/icons/hicolor/scalable/apps/espulse.svg
```

The second line installs the eS Xlite icon used in the widget list and tray settings (the tray
icon itself comes from inside the widget).

Then enable **espulse** under the system tray's *Configure System Tray → Entries*, or add it
from *Add Widgets*. In the background it only asks for the battery level every 5 minutes
(two or three read queries), shown over a mouse icon, in green while the cable is connected, and in the theme's warning and error colours at 20% and 10%. Every 10 seconds it also lists USB IDs, which
sends the mouse nothing, and reads the battery again if they changed. Opening the popup runs
the full read: all DPI stages, polling rate, the toggles, LOD, debounce, rotation and the
dongle's settings. To update after pulling changes, stop the shell first, which should keep the tray's visibility
setting for the widget:

```bash
systemctl --user stop plasma-plasmashell; kpackagetool6 -t Plasma/Applet -u plasmoid; systemctl --user start plasma-plasmashell
```

## License

[MIT](LICENSE)
