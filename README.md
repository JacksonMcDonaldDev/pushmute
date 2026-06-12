# Selective Mic Router (`smr`)

Talk to one app without being heard by the rest. `smr` exposes a virtual PipeWire
microphone ("SMR Mic"), routes your physical mic into it, sets it as the system
default source, and silences it while you hold a global hotkey — so when you hold
that key to talk in your comms app, your dictation software and everything else go quiet.

See [`PRD.md`](PRD.md) for the full design and [`TODO.md`](TODO.md) for status.

## Build & install

```sh
cargo build --release
install -Dm755 target/release/smr ~/.local/bin/smr
```

## First-run setup

```sh
smr devices            # list capture devices + input devices
smr set-mic <node.name>   # pick the physical mic to route from
smr set-key            # press the key you want as your hotkey
```

Config is written to `~/.config/smr/config.toml`.

## Run

```sh
smr run                # foreground; Ctrl-C restores the default source and tears down
```

While running, from another shell:

```sh
smr status
smr mute / smr unmute / smr toggle
smr restore            # reset the default source to its pre-SMR value
```

Bind the **same** physical key as both your comms app's push-to-talk and `smr`'s hotkey: holding
it transmits in the comms app and silences everything reading the default source.

## Auto-start (systemd user service)

```sh
install -Dm644 smr.service ~/.config/systemd/user/smr.service
systemctl --user enable --now smr
```

Hyprland alternative — add to `hyprland.conf`:

```
exec-once = ~/.local/bin/smr run
```

## How it works

PipeWire models audio as a graph, so routing is native. `smr` drives the proven CLI
primitives: `pw-loopback` creates the virtual source and the loopback from the
physical mic, `wpctl` manages the default source and the mute, and the hotkey is read
directly from `evdev` (`/dev/input/event*`) — which works under any compositor and
needs no root because the user is in the `input` group. The comms app is pinned to
the **physical** device inside that app, so it is unaffected by the mute.
