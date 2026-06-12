# Selective Mic Router (`pushmute`)

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platform: Linux](https://img.shields.io/badge/platform-Linux%20%2F%20PipeWire-informational)
![Status: pre-release](https://img.shields.io/badge/status-v0.1%20pre--release-orange)

**Talk to one app without being heard by the rest.**

`pushmute` exposes a virtual PipeWire microphone ("PushMute"), routes your physical mic
into it, sets it as the system default source, and silences it while you hold a global
hotkey. Bind that same key as your comms app's push-to-talk, and holding it transmits in
the comms app while your dictation software — and everything else reading the default
mic — goes quiet.

See [`docs/design.md`](docs/design.md) for how it works and why it's built this way.

## Requirements

- **PipeWire + WirePlumber** — the hard runtime dependency. `pushmute` drives the stock
  CLI tools `pw-loopback`, `pw-dump`, and `wpctl`, which ship with these.
  (Arch: `pipewire` + `wireplumber`; Debian/Ubuntu: `pipewire-bin` + `wireplumber`.)
- **Membership in the `input` group** — the hotkey is read directly from `evdev`
  (`/dev/input/event*`). This needs no root, but your user must be in `input`:
  ```sh
  sudo usermod -aG input "$USER"   # log out and back in to take effect
  ```
- **A tray that speaks StatusNotifierItem (SNI)** — for the system-tray icon. Native in
  waybar (Hyprland/Sway) and KDE Plasma. On stock GNOME (incl. Pop!_OS) you need the
  AppIndicator extension; see [Desktop support](#desktop-support) below.

## Build & install

`pushmute` is built from source with `cargo` (Rust toolchain required):

```sh
cargo build --release
install -Dm755 target/release/pushmute ~/.local/bin/pushmute
```

Make sure `~/.local/bin` is on your `PATH`.

## First-run setup

```sh
pushmute devices                 # list capture devices + input devices
pushmute set-mic <node.name>     # pick the physical mic to route from
pushmute set-key                 # press the key you want as your hotkey
```

Config is written to `~/.config/pushmute/config.toml`.

## Run

```sh
pushmute run                     # foreground; Ctrl-C restores the default source and tears down
```

While running, from another shell:

```sh
pushmute status
pushmute mute / pushmute unmute / pushmute toggle
pushmute restore                 # reset the default source to its pre-PushMute value
```

Bind the **same** physical key as both your comms app's push-to-talk and `pushmute`'s
hotkey: holding it transmits in the comms app and silences everything reading the default
source.

## Auto-start (systemd user service)

```sh
install -Dm644 pushmute.service ~/.config/systemd/user/pushmute.service
systemctl --user enable --now pushmute
```

> **Hyprland users:** launch your session via [`uwsm`](https://github.com/Vladimir-csp/uwsm).
> It activates `graphical-session.target`, which the systemd unit orders against — without
> it the tray may start before the graphical session is ready and silently fail to appear.

Hyprland alternative without systemd — add to `hyprland.conf`:

```
exec-once = ~/.local/bin/pushmute run
```

## Desktop support

| Desktop | Audio stack | Tray | Notes |
|---|---|---|---|
| Arch + Hyprland / Sway (`uwsm`) | PipeWire ✓ | waybar SNI ✓ | Primary target |
| Arch + KDE Plasma | PipeWire ✓ | Native SNI ✓ | Works out of the box |
| Ubuntu 22.04+ / Pop!_OS | PipeWire ✓ | Extension may be needed | Works, some friction |

On stock GNOME the tray needs the **AppIndicator and KStatusNotifierItem Support**
extension. Ubuntu ships it (`ubuntu-appindicator@ubuntu.com`) pre-enabled; Pop!_OS and
vanilla GNOME may require installing/enabling it before the icon shows up.

## How it works

PipeWire models audio as a graph, so routing is native. `pushmute` drives the proven CLI
primitives: `pw-loopback` creates the virtual source and the loopback from the physical
mic, `wpctl` manages the default source and the mute, and the hotkey is read directly from
`evdev` (`/dev/input/event*`) — which works under any compositor and needs no root because
the user is in the `input` group. The comms app is pinned to the **physical** device inside
that app, so it is unaffected by the mute.

## License

[MIT](LICENSE) © Jackson McDonald
