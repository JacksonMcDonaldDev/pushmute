# PushMute

*selective mic router*

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/JacksonMcDonaldDev/pushmute#license)
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

## Install

### Prebuilt binary (any distro)

Each release ships a single static binary — no toolchain to install, no glibc version
to match. Download it, drop it on your `PATH`, and run the environment check:

```sh
curl -sSfL https://github.com/JacksonMcDonaldDev/pushmute/releases/latest/download/pushmute-x86_64-linux -o pushmute
chmod +x pushmute
install -Dm755 pushmute ~/.local/bin/pushmute   # ensure ~/.local/bin is on your PATH
pushmute doctor          # verify PipeWire tools, input-group membership, tray support
```

A matching `.sha256` is attached to each release if you want to verify the download.
This gives you the `pushmute` command and daemon; to also get the systemd user service,
launcher entry, and tray icon wired up, use `install.sh` below or follow
[Auto-start](#auto-start-systemd-user-service) to drop the unit in manually.

### Full per-user install (`install.sh`)

The repo ships an installer that sets up the complete desktop integration — systemd
user unit, launcher entry, and icon — under your per-user XDG locations. It builds
from source, so a Rust toolchain is required:

```sh
git clone https://github.com/JacksonMcDonaldDev/pushmute
cd pushmute
./install.sh             # build → install to ~/.local → start service → run doctor
```

It starts the service (but does **not** enable it on login — see [Auto-start](#auto-start-systemd-user-service))
and finishes by running `pushmute doctor`. To remove it:

```sh
./install.sh --uninstall          # remove files, keep ~/.config/pushmute
./install.sh --uninstall --purge  # also remove the config directory
```

### Manual build

If you'd rather build and wire it up yourself:

```sh
cargo build --release
install -Dm755 target/release/pushmute ~/.local/bin/pushmute
```

Make sure `~/.local/bin` is on your `PATH`, then run `pushmute doctor` to confirm your
environment is ready.

> **Arch Linux:** an AUR package is planned but not yet published. For now use the
> prebuilt binary or `install.sh` above.

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

Running on login is **off by default**. The easiest way to turn it on is the tray
menu's **Run on startup** checkbox, which enables/disables the systemd user unit for
you. Equivalently, from a shell:

```sh
systemctl --user enable pushmute     # start on every login
systemctl --user disable pushmute    # stop starting on login
```

For the **prebuilt-binary** and **manual** install paths, first drop the unit in place
(the `install.sh` path does this for you):

```sh
install -Dm644 pushmute.service ~/.config/systemd/user/pushmute.service
systemctl --user start pushmute      # run now; add `enable` to also start on login
```

> **Hyprland users:** launch your session via [`uwsm`](https://github.com/Vladimir-csp/uwsm).
> It activates `graphical-session.target`, which the systemd unit orders against — without
> it the tray may start before the graphical session is ready and silently fail to appear.

> **Non-systemd sessions:** if your session doesn't activate `graphical-session.target`,
> the unit won't fire on login. Add your compositor's own autostart line instead, e.g.
> `exec pushmute run` in your WM config.

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

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

© Jackson McDonald

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
