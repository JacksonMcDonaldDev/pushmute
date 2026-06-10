# Selective Mic Router (SMR) - v1 PRD

> **Platform:** Linux (Pop!_OS 24.04 LTS), PipeWire audio, Wayland/Hyprland session.

## Overview

A lightweight Linux background daemon that gives users a "talk to one app without being
heard by the rest" capability. It exposes a **virtual microphone** backed by PipeWire,
routes the physical mic into it, and silences that virtual mic on demand via a global
push-to-talk key — so when you push-to-talk in your comms app, your dictation software,
meeting apps, and everything else go silent.

## Problem Statement

Users who run voice-to-text / dictation software alongside a comms app (Discord, Teams,
Zoom, etc.) have a conflict: when they push-to-talk in the comms app, the dictation
software also hears them and generates unwanted transcriptions. PipeWire routes every
app to the default source by default, with no native per-app "mute this app's mic input
while I talk to that one" mechanism.

## Core Concept

PipeWire models audio as a graph of nodes and links, so routing is native — there is no
need for a third-party virtual-cable driver. SMR creates one virtual source and controls
a single mute on the link feeding it.

```
Physical Mic (e.g. RME Babyface / Anker PowerConf)
    |
    ├──(linked directly)──► Comms App  — configured to use the PHYSICAL device, has its own PTT
    |
    └──► SMR virtual source ("SMR Mic", set as the default source)
              ▲
              │  loopback link from physical mic
              │  [PTT held → link muted → silence]
              │  [PTT released → real audio]
              │
         All other apps read from the default source and get this
```

- The comms app is pinned to the **physical capture device** directly.
- All other apps use the **default source**, which SMR sets to its virtual mic ("SMR Mic").
- The user binds the same physical key as both (a) their comms app's PTT and (b) SMR's
  mute key (read directly from evdev, so it fires regardless of which app has focus).
- Result: pressing PTT activates the comms app's transmit AND silences everything else
  simultaneously.

## Detected System Baseline (verified 2026-06-09)

| Aspect | Value |
|---|---|
| Distro | Pop!_OS 24.04 LTS |
| Audio server | PipeWire 1.5.85 + WirePlumber + pipewire-pulse |
| Compatibility shims | `pactl`/`pacmd` (libpulse 16.1) present |
| Session | Wayland, Hyprland compositor |
| Current default source | `alsa_input.usb-RME_Babyface_Pro_…pro-input-0` |
| Other capture devices | Anker PowerConf C200, internal analog |
| `input` group | **`jack` is a member** → evdev `/dev/input/event*` readable without root |
| Toolchains present | Rust (cargo), Go, Python3, Node, gcc |
| Autostart | `systemd --user` available |

## User Setup Flow (First Run)

1. SMR creates its virtual source ("SMR Mic") in the PipeWire graph. No driver install,
   no download — PipeWire supports virtual devices natively.
2. SMR prompts (or, on a configured non-interactive start, silently applies) setting the
   **default source** to "SMR Mic". A `--no-prompt` / config flag suppresses the prompt
   on subsequent starts.
3. The user configures (via the config file or a small settings UI):
   - The **physical microphone** to route from (selected from detected capture devices,
     excluding SMR's own virtual mic to prevent feedback loops).
   - The **PTT key** (any key or mouse button, identified by evdev keycode).
   - The **comms app** is pinned to the physical device by the user inside that app — SMR
     does not need to manage the comms app.
4. SMR backgrounds itself (systemd user service or `--daemon`) and begins routing.

## Functional Requirements

### FR-1: Virtual Microphone Provisioning

- On startup, create a virtual source named "SMR Mic" in the PipeWire graph (e.g. a
  null sink whose monitor is exposed as a capture source, or an equivalent loopback /
  virtual-source node).
- The node must present as a normal microphone to all PipeWire and pipewire-pulse clients
  (Discord, browsers, dictation apps).
- On graceful exit, tear the virtual source down cleanly so the graph returns to its
  prior state.
- **No third-party driver is required** — PipeWire creates virtual devices in-process,
  so there is no driver to detect, download, install, or redistribute.

### FR-2: Default Source Management

- On startup (after the virtual source exists), set "SMR Mic" as the PipeWire **default
  source** (via WirePlumber metadata / `wpctl set-default`, with `pactl` as a fallback).
- First time: prompt the user to confirm; offer a "don't ask again" setting persisted to
  config.
- Subsequent startups: apply automatically when the user has opted in.
- On graceful exit: **restore the previous default source** (record it before changing it
  — on this machine that is the RME Babyface pro-input).
- Best-effort restoration only — if the daemon is killed, "SMR Mic" remains default until
  the user resets it or restarts SMR. A `--restore` subcommand should reset the default
  to the recorded prior device without a full run.

### FR-3: Physical Mic Selection and Capture

- Enumerate active PipeWire capture devices and present them for selection.
- **Exclude SMR's own virtual source** from the list to avoid feedback loops.
- Route from the selected physical device without taking it exclusively — PipeWire is
  shared by default, so the comms app can read the same physical mic simultaneously.
- Persist the selection across sessions.
- Optionally pin the route by device **name/serial** rather than node ID so it survives
  reconnects (full hot-plug handling is v2 — see Out of Scope).

### FR-4: Audio Routing

- Link the selected physical mic into the "SMR Mic" virtual source within the PipeWire
  graph (loopback). All apps on the default source thereby receive the physical mic's
  audio.
- Because routing is graph linking rather than a manual capture→render copy loop, the
  data path stays inside PipeWire; SMR's job is to create nodes/links and toggle mute,
  not to shuttle PCM frames in userspace.
- Prioritize stability over latency. Target added latency under ~50 ms; choose a quantum
  / buffer that avoids xruns over aggressive low-latency tuning.
- Match the physical device's sample rate/format where practical to avoid resampling
  surprises (devices here run 48 kHz).

### FR-5: Global Push-to-Talk Mute

- The user binds any single key or mouse button as the PTT/mute control.
- **Wayland note:** Wayland deliberately blocks X11-style global keyboard hooks. SMR
  therefore reads input directly from **evdev** (`/dev/input/event*`), which works
  globally — including when a fullscreen game or any other client has focus — and is
  available without root because the user is in the `input` group.
  - SMR should let the user pick which input device(s) to listen on, or grab all keyboard/
    mouse devices, and identify the bind by evdev keycode.
- While the key is **held**: mute the loopback link into "SMR Mic" (set node mute / volume
  0). This is instant and glitch-free — no need to synthesize silence buffers.
- While the key is **released**: unmute, restoring real mic audio.
- Mute/unmute must feel instant (no perceptible delay).
- Optional secondary binding path: a Hyprland `bind`/`bindr` (release) pair that calls an
  SMR CLI (`smr mute --hold` / `smr unmute`). This is compositor-specific and a fallback;
  evdev is the primary mechanism because it is compositor-agnostic and gives true
  press/hold/release semantics.
- Persist the binding across sessions.

### FR-6: Control Surface and Status

**Daemon + CLI (primary):**
- SMR runs as a background daemon (foreground `--daemon` flag and/or a systemd user
  service).
- A CLI exposes: `status`, `mute --hold` / `unmute`, `toggle`, `set-mic <device>`,
  `set-key <key>`, `restore`, `reload`.
- State is observable: current state ("Routing Active" / "Muted"), selected mic, bound key.

**Optional tray / status indicator:**
- Hyprland has no built-in system tray. Provide status via a **StatusNotifierItem** tray
  icon (consumed by waybar, which is the common Hyprland status bar) and/or a waybar
  custom module that polls `smr status`.
- Optional desktop notifications (via `notify-send` / the freedesktop notification spec)
  for state changes and errors.

**Optional settings UI:**
- A minimal GTK/Qt or web-based settings panel may wrap the same operations (mic dropdown,
  key-capture button, status indicator, "start on login" toggle). For v1 the config file +
  CLI is sufficient; a GUI is a nice-to-have.

### FR-7: Auto-Start on Login

- Provide a **systemd user service** (`smr.service`) the user can enable with
  `systemctl --user enable --now smr`.
- On auto-start, SMR comes up in the background, provisions the virtual source, applies
  saved settings, and begins routing immediately.
- Document the alternative of a Hyprland `exec-once` line for users who prefer compositor-
  managed startup.

## Non-Functional Requirements

- **Stability over latency:** the pipeline must not produce xruns, dropouts, or crashes.
  Conservative quantum sizing; latency under ~50 ms is acceptable.
- **Resource usage:** minimal CPU/memory. It is a background utility, not a DAW; routing
  lives in the PipeWire graph, so SMR itself should be near-idle.
- **Single instance:** only one SMR daemon at a time (lock file under
  `$XDG_RUNTIME_DIR`, or rely on the systemd unit).
- **Graceful degradation:** if the physical mic or the virtual source disappears mid-
  session (device unplugged, PipeWire restarted), stop routing cleanly, surface a
  notification, and reconnect when possible — do not crash. Subscribe to PipeWire registry
  events rather than polling.
- **Wayland/X11 agnostic:** input capture via evdev does not depend on the compositor, so
  the same binary works under Hyprland today and other Wayland or X11 sessions later.

## Technical Stack

| Component | Choice | Notes |
|---|---|---|
| Language/runtime | **Rust** (cargo present) | Single static binary, tiny footprint, first-class PipeWire & evdev bindings; ideal for a long-running daemon. |
| Audio | `pipewire` crate (libpipewire bindings) | Create virtual source, link nodes, toggle mute. `pw-loopback`/`pw-cli`/`wpctl` usable for a faster prototype. |
| Default-source control | WirePlumber metadata via `wpctl`; `pactl` fallback | Both present on this machine. |
| Global PTT input | `evdev` crate reading `/dev/input/event*` | Works because `jack` ∈ `input` group; Wayland-safe. |
| Config persistence | TOML/JSON under `$XDG_CONFIG_HOME/smr/` (`~/.config/smr/`) | XDG-compliant, human-readable. |
| Autostart | systemd **user** service (`smr.service`) | Native Linux autostart. Hyprland `exec-once` documented as alternative. |
| Tray (optional) | StatusNotifierItem for waybar | Hyprland has no native tray. |
| Packaging (v1) | Run from `cargo build --release` output | Revisit `.deb` / Flatpak for v2. |

> **Faster-prototype path:** a Python or shell POC can drive `pw-loopback` (create the
> virtual source + loopback), `wpctl set-default` (default source), and `python-evdev`
> (PTT), muting via `wpctl set-mute`. Good for validating the routing + PTT model before
> committing to the Rust daemon.

## Out of Scope for v1

- Per-app routing beyond the single default-source split (e.g. different audio to
  different apps).
- Physical-mic hot-plug auto-reconnect (route-by-serial groundwork in FR-3, full handling
  is v2).
- Noise suppression, gain control, or other DSP (PipeWire `filter-chain` could host this
  later).
- A polished GUI / settings app (CLI + config file ship first).
- `.deb` / Flatpak packaging and auto-update.
- Multi-key / chorded bindings; multiple simultaneous PTT profiles.

## Open Questions

1. **Virtual source primitive:** null-sink-monitor vs. a dedicated loopback/virtual-source
   node vs. a declarative `~/.config/pipewire/pipewire.conf.d/` drop-in — which gives the
   cleanest lifecycle (created/destroyed by the daemon) and the most natural device name
   to clients? Prototype both.
2. **Default-source restoration robustness:** if the daemon is killed (not gracefully
   exited), "SMR Mic" stays default. Should the systemd unit add an `ExecStopPost` /
   watchdog that runs `smr restore`, and should startup detect "default is a stale SMR Mic
   from a previous crash" and self-heal?
3. **evdev device selection:** grab all input devices and filter, or have the user pick the
   keyboard/mouse explicitly? Grabbing-all is more convenient but must avoid consuming the
   keystroke (read without `EVIOCGRAB` so the comms app still sees the same key for its own
   PTT).
4. **Tray scope:** is a waybar SNI/module worth building for v1, or is CLI + `notify-send`
   enough until a GUI exists?
