# SMR v1 — TODO

Tracks the path from PRD → working v1. The strategy: a **Rust daemon (`smr`)** that
orchestrates the already-proven CLI primitives (`pw-loopback`, `wpctl`, `pw-dump`) and
reads PTT directly from **evdev**. This is the PRD's "faster-prototype path" implemented
in the target language — it gets v1 on its feet without first fighting the libpipewire
bindings, and the orchestration boundary can be swapped for native bindings later.

Legend: `[x]` done · `[~]` partial / v1-good-enough · `[ ]` not started

## Foundation
- [x] Probe system baseline (cargo, pw tools, evdev, input group) — all present
- [x] De-risk the load-bearing path live: virtual source via `pw-loopback`, node-id
      resolution via `pw-dump`, mute via `wpctl set-mute <id>`
- [x] Cargo project + module layout (`config`, `pipewire`, `input`, `daemon`, `ipc`)

## FR-1 Virtual Microphone Provisioning
- [x] Create `smr_mic` Audio/Source via `pw-loopback` child on startup
- [x] Present as a normal mic (verified: `media.class=Audio/Source`, description "SMR Mic")
- [x] Tear down cleanly on graceful exit (kill child)
- [~] Self-heal if the loopback child dies mid-session (v2: registry-event reconnect)

## FR-2 Default Source Management
- [x] Record previous default source before changing it
- [x] Set `smr_mic` as default on startup (config-gated via `set_default`)
- [x] Restore previous default on graceful exit
- [x] `smr restore` subcommand — reset default without a full run
- [~] First-run confirm prompt / "don't ask again" (config flag exists; interactive
      prompt not yet wired — defaults to auto-apply)
- [ ] Detect stale `smr_mic`-as-default from a previous crash and self-heal on startup

## FR-3 Physical Mic Selection and Capture
- [x] Enumerate capture devices (`smr devices`), excluding `smr_mic`
- [x] `smr set-mic <node.name>` persists selection
- [x] Route without exclusive grab (pw-loopback is shared by default)
- [~] Pin route by node.name (survives same-name re-enumeration; serial-based pin is v2)

## FR-4 Audio Routing
- [x] Loopback link physical mic → `smr_mic` inside the graph
- [x] Stability-first: default quantum (no aggressive low-latency tuning)
- [~] Match device sample rate/format (relies on PipeWire negotiation; no forced rate yet)

## FR-5 Global Push-to-Talk Mute
- [x] Read PTT from evdev globally (no `EVIOCGRAB`, so comms app still sees the key)
- [x] Hold → mute `smr_mic`; release → unmute (instant, via `wpctl set-mute <id>`)
- [x] `smr set-key` learns the bind by capturing the next press/release
- [x] **Chord bindings** (e.g. Ctrl+F19): mute only while *all* bound keys are held;
      a single key is just a 1-element chord. (Beyond the PRD's v1 scope — pulled
      forward to mirror a comms-app PTT chord. Verified live, incl. negative test.)
- [x] Persist bind (+ optional specific input device) across sessions
- [ ] Hyprland `bind`/`bindr` fallback path documented (secondary mechanism)

## FR-6 Control Surface and Status
- [x] Daemon + foreground flag
- [x] CLI: `status`, `mute --hold`/`unmute`, `toggle`, `set-mic`, `set-key`, `restore`,
      `devices`, `reload`
- [x] IPC over `$XDG_RUNTIME_DIR/smr.sock` so CLI talks to the running daemon
- [~] `reload` (re-reads config; live mic-change requires daemon restart in v1)
- [ ] waybar StatusNotifierItem / custom module (optional, deferred)
- [ ] `notify-send` desktop notifications on state change / error (optional)

## FR-7 Auto-Start on Login
- [x] `smr.service` systemd user unit shipped
- [ ] Document Hyprland `exec-once` alternative (README)

## Non-Functional
- [x] Single instance (socket-bind guard; stale-socket cleanup)
- [x] Graceful SIGINT/SIGTERM cleanup (restore default + tear down loopback)
- [~] Graceful degradation on device disappearance (logs + exits cleanly; auto-reconnect v2)
- [x] Wayland/X11-agnostic input (evdev)

## Open Questions (from PRD — to validate)
- [ ] Q1 virtual-source primitive: pw-loopback child (current) vs. config drop-in vs.
      native node — revisit once native bindings land
- [ ] Q2 crash-restoration robustness (ExecStopPost / startup self-heal)
- [x] Q3 evdev selection: read all KEY devices without grab (chosen); `--device` narrows it
- [ ] Q4 tray scope: CLI + notify-send likely enough for v1
