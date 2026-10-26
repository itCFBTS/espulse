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

## License

[MIT](LICENSE)
