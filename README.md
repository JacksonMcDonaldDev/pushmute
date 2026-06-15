# PushMute

*A push-to-mute virtual microphone for your whole system.*

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/JacksonMcDonaldDev/pushmute#license)
![Platform: Linux](https://img.shields.io/badge/platform-Linux%20%2F%20PipeWire-informational)

**Hold one key to mute your mic from every other app while you use voice to text tools** — Discord, Teams,
Zoom, your browser, all of it. Let go and they hear you again.

PushMute creates a virtual PipeWire microphone ("PushMute") and makes it your system
default source, so every app captures through it. Your hotkey mutes that virtual mic
instantly: hold it and everything reading the default mic goes silent.

You can use this to coordinate **push-to-talk for one app while push-muting open comms**: bind the *same* key as both PushMute's hotkey and your target app's push-to-talk while pointing
that app at your physical mic directly (not the system default). Now one key transmits
there while silencing everything else. 

I find this most useful for continuing to use my voice transcription app (shoutout [Handy](https://github.com/cjpais/Handy)!) while sharing an open comms space with co-workers or friends, and keeping the channels separate and convenient.

## Requirements

- **PipeWire + WirePlumber** — the runtime dependency. PushMute drives the stock
  `pw-loopback`, `pw-dump`, and `wpctl` tools that ship with them.
  (Arch: `pipewire` + `wireplumber`; Debian/Ubuntu: `pipewire-bin` + `wireplumber`.)
- **Membership in the `input` group** — the hotkey is read straight from `evdev`
  (`/dev/input/event*`). No root needed, but your user must be in `input`:
  ```sh
  sudo usermod -aG input "$USER"   # log out and back in to apply
  ```
- **An SNI-capable tray** — for the icon and its menu. Native on waybar (Hyprland/Sway)
  and KDE Plasma; stock GNOME and Pop!_OS need the AppIndicator extension. See
  [Desktop support](#desktop-support).

## Install

### Recommended: `install.sh`

One command sets up the whole desktop integration — daemon, systemd user unit, and app
launcher — under your per-user XDG directories. It downloads the prebuilt static binary
(x86-64), so there's **nothing to compile and no toolchain to install**:

```sh
curl -sSfL https://raw.githubusercontent.com/JacksonMcDonaldDev/pushmute/main/install.sh | bash
```

Rather read it first? Clone and run the same script:

```sh
git clone https://github.com/JacksonMcDonaldDev/pushmute
cd pushmute
./install.sh             # add --build to compile from source instead (needs Rust)
```

Either way it installs to `~/.local`, starts the service, and runs `pushmute doctor` to
check your environment. Running on login stays **off** until you opt in (see
[Auto-start](#auto-start)). Then jump to [Setup](#setup) to pick your mic and hotkey from
the tray.

To remove it:

```sh
./install.sh --uninstall          # remove files, keep ~/.config/pushmute
./install.sh --uninstall --purge  # also remove the config directory
```

### Just the binary

If you only want the `pushmute` command without the systemd unit or launcher entry, grab
the static binary yourself. This still gives you the daemon and the tray icon; add the
unit later via [Auto-start](#auto-start) if you want it.

```sh
curl -sSfL https://github.com/JacksonMcDonaldDev/pushmute/releases/latest/download/pushmute-x86_64-linux -o pushmute
chmod +x pushmute
install -Dm755 pushmute ~/.local/bin/pushmute   # ensure ~/.local/bin is on your PATH
pushmute doctor                                  # verify PipeWire, input group, tray
```

A matching `.sha256` is attached to each release if you want to verify the download.

### Build from source

For a non-x86-64 machine or local development, build it yourself. From a clone,
`./install.sh --build` does this and wires up the full desktop integration; or just build
the binary:

```sh
cargo build --release
install -Dm755 target/release/pushmute ~/.local/bin/pushmute
```

Make sure `~/.local/bin` is on your `PATH`, then run `pushmute doctor`.

> **Arch Linux:** an AUR package is planned but not yet published. For now use
> `install.sh` or one of the paths above.

## Setup

Once PushMute is running, click its **tray icon** — the menu is the whole control surface:

- **Microphone Input Source** — pick the physical mic to route from.
- **Rebind hotkey…** — press the key or chord you want to mute with.
- **Run on startup** — start PushMute on every login (off by default).
- **Enabled** — toggle routing on or off.

Settings are saved to `~/.config/pushmute/config.toml`.

> No tray icon? See [Desktop support](#desktop-support).

## Auto-start

Running on login is **off by default**. The easiest way to turn it on is the tray menu's
**Run on startup** checkbox, which enables/disables the systemd user unit for you.
Equivalently, from a shell:

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

> **Hyprland/sway users:** the unit is `WantedBy=default.target`, so it autostarts in any
> `systemd --user` session — including bare `exec-once` setups that never activate
> `graphical-session.target`. The tray waits for your bar's tray host (e.g. waybar) to
> appear and registers when it does, so it doesn't matter if pushmute starts first.

> **Non-systemd sessions:** if you don't run a `systemd --user` instance at all, the unit
> won't fire on login. Add your compositor's own autostart line instead — e.g. in
> `hyprland.conf`:
> ```
> exec-once = ~/.local/bin/pushmute run
> ```

## Command-line control

The tray covers everyday use, but everything is scriptable too. Configure:

```sh
pushmute devices                 # list capture + input devices
pushmute set-mic <node.name>     # choose the physical mic to route from
pushmute set-key                 # press the key you want as your hotkey
```

Run and control (when not using the systemd service):

```sh
pushmute run                     # foreground; Ctrl-C restores the default source and tears down
pushmute status
pushmute mute / pushmute unmute / pushmute toggle
pushmute restore                 # reset the default source to its pre-PushMute value
pushmute doctor                  # environment check
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
