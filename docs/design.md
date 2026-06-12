# Design

How `pushmute` works and why it's built this way. For usage, see the
[README](../README.md).

## The problem

If you run dictation / voice-to-text software alongside a comms app (Discord,
Teams, Zoom), they fight over your microphone. PipeWire routes every app to the
**default source**, so when you push-to-talk in the comms app, the dictation
software hears you too and generates unwanted transcriptions. There's no native
"mute this app's mic input while I talk to that one" mechanism.

## Core concept

PipeWire models audio as a graph of nodes and links, so routing is native —
there's no need for a third-party virtual-cable driver. `pushmute` creates one
virtual source and controls a single mute on the link feeding it.

```
Physical Mic (e.g. RME Babyface / Anker PowerConf)
    |
    ├──(linked directly)──► Comms App  — pinned to the PHYSICAL device, has its own PTT
    |
    └──► PushMute virtual source ("PushMute", set as the default source)
              ▲
              │  loopback link from physical mic
              │  [PTT held → link muted → silence]
              │  [PTT released → real audio]
              │
         All other apps read from the default source and get this
```

- The comms app is pinned to the **physical capture device** directly.
- All other apps use the **default source**, which `pushmute` points at its
  virtual mic ("PushMute").
- The user binds the **same** physical key as both (a) the comms app's PTT and
  (b) `pushmute`'s mute key. The key is read directly from evdev, so it fires
  regardless of which app has focus.
- Result: holding the key activates the comms app's transmit **and** silences
  everything else simultaneously.

## Key design decisions

### Routing is graph linking, not a PCM copy loop

`pushmute` links the physical mic into the virtual source within the PipeWire
graph (via `pw-loopback`). The audio data path stays inside PipeWire — the
daemon's job is to create nodes/links and toggle a mute, not to shuttle PCM
frames in userspace. This keeps it near-idle and avoids a class of latency and
xrun problems. Muting is a node mute / volume-0 on the loopback link: instant
and glitch-free, with no need to synthesize silence buffers.

The trade-off is stability over latency. Target added latency is under ~50 ms,
with a conservative quantum chosen to avoid xruns rather than chase aggressive
low-latency tuning.

### Input via evdev, not a compositor hook

Wayland deliberately blocks X11-style global keyboard hooks. `pushmute`
therefore reads input directly from **evdev** (`/dev/input/event*`), which works
globally — including when a fullscreen game or any other client has focus — and
needs no root because the user is in the `input` group. Because it's
compositor-agnostic, the same binary works under Hyprland, other Wayland
sessions, and X11.

Input is read **without** `EVIOCGRAB`, so the keystroke still reaches the comms
app for its own PTT — the same key press drives both.

### Driving the stock CLI primitives

Rather than binding libpipewire directly, v1 orchestrates the already-proven
stock tools: `pw-loopback` (create the virtual source + loopback), `wpctl`
(default-source management and mute), and `pw-dump` (node-id resolution). This
got v1 on its feet without first fighting the native bindings, and the
orchestration boundary can be swapped for native bindings later without changing
the model above.

### Default-source management and restoration

On startup (after the virtual source exists), `pushmute` records the current
default source, then sets "PushMute" as the default. On graceful exit it
**restores** the previous default. Restoration is best-effort: if the daemon is
killed rather than exited cleanly, "PushMute" stays default until the user runs
`pushmute restore` or restarts. A first-run prompt (with a persisted "don't ask
again") guards the initial switch.

## Technical stack

| Component | Choice | Notes |
|---|---|---|
| Language/runtime | **Rust** | Single small binary, tiny footprint; ideal for a long-running daemon. |
| Audio | `pw-loopback` / `wpctl` / `pw-dump` | Create virtual source, link nodes, toggle mute, resolve node IDs. |
| Global PTT input | `evdev` reading `/dev/input/event*` | Works without root via the `input` group; Wayland-safe. |
| Config persistence | TOML under `$XDG_CONFIG_HOME/pushmute/` | XDG-compliant, human-readable. |
| Autostart | systemd **user** service (`pushmute.service`) | Native Linux autostart; Hyprland `exec-once` is an alternative. |
| Tray (optional) | StatusNotifierItem (SNI) | Hyprland has no native tray; consumed by waybar. |

## Non-goals (v1)

- Per-app routing beyond the single default-source split.
- Physical-mic hot-plug auto-reconnect (route-by-name groundwork exists; full
  handling is later work).
- Noise suppression, gain control, or other DSP.
- A polished GUI / settings app — CLI + config file ship first.
- `.deb` / Flatpak packaging and auto-update.
- Multi-key / chorded bindings; multiple simultaneous PTT profiles.
