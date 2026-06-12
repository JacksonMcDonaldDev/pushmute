# Selective Mic Router (`pushmute`)

Talk to one app without being heard by the rest. `pushmute` exposes a virtual PipeWire
microphone ("PushMute"), routes your physical mic into it, sets it as the system
default source, and silences it while you hold a global hotkey — so when you hold
that key to talk in your comms app, your dictation software and everything else go quiet.

See [`docs/PRD.md`](docs/PRD.md) for the full design and [`docs/TODO.md`](docs/TODO.md) for status.

## Build & install

```sh
cargo build --release
install -Dm755 target/release/pushmute ~/.local/bin/pushmute
```

## First-run setup

```sh
pushmute devices            # list capture devices + input devices
pushmute set-mic <node.name>   # pick the physical mic to route from
pushmute set-key            # press the key you want as your hotkey
```

Config is written to `~/.config/pushmute/config.toml`.

## Run

```sh
pushmute run                # foreground; Ctrl-C restores the default source and tears down
```

While running, from another shell:

```sh
pushmute status
pushmute mute / pushmute unmute / pushmute toggle
pushmute restore            # reset the default source to its pre-PushMute value
```

Bind the **same** physical key as both your comms app's push-to-talk and `pushmute`'s hotkey: holding
it transmits in the comms app and silences everything reading the default source.

## Auto-start (systemd user service)

```sh
install -Dm644 pushmute.service ~/.config/systemd/user/pushmute.service
systemctl --user enable --now pushmute
```

Hyprland alternative — add to `hyprland.conf`:

```
exec-once = ~/.local/bin/pushmute run
```

## How it works

PipeWire models audio as a graph, so routing is native. `pushmute` drives the proven CLI
primitives: `pw-loopback` creates the virtual source and the loopback from the
physical mic, `wpctl` manages the default source and the mute, and the hotkey is read
directly from `evdev` (`/dev/input/event*`) — which works under any compositor and
needs no root because the user is in the `input` group. The comms app is pinned to
the **physical** device inside that app, so it is unaffected by the mute.
