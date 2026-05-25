# Selective Mic Router (SMR) - v1 PRD

## Overview

A lightweight Windows background utility that gives users a "talk to one app without being heard by the rest" capability. It captures the physical microphone, routes audio through a virtual cable, and silences the virtual output on demand via a global hotkey — so when you push-to-talk in your comms app, your dictation software, meeting apps, and everything else go silent.

## Problem Statement

Users who run voice-to-text / dictation software alongside a comms app (Discord, Teams, Zoom, etc.) have a conflict: when they push-to-talk in the comms app, the dictation software also hears them and generates unwanted transcriptions. There is no native Windows mechanism to selectively mute mic input per-app.

## Core Concept

```
Physical Mic
    |
    ├──(direct)──► Comms App (Discord, etc.) — uses physical mic, has its own PTT
    |
    └──► SMR App ──► "CABLE Input" ──[VB-CABLE loopback]──► "CABLE Output" (virtual mic, set as system default)
                         |                                         |
                    [hotkey held → silence]                   All other apps read from here
                    [hotkey released → real audio]
```

- The comms app is configured to use the **physical mic** directly.
- All other apps use the **system default mic**, which SMR sets to "CABLE Output" (the VB-CABLE virtual mic).
- The user sets the same key as both: (a) their comms app's PTT and (b) SMR's mute hotkey.
- Result: pressing PTT activates comms AND silences everything else simultaneously.

## User Setup Flow (First Run)

1. App detects whether VB-CABLE is installed.
   - If not: downloads and launches the VB-CABLE installer, guiding the user through it.
2. App prompts the user to confirm changing the system default microphone to "CABLE Output".
   - Includes a "Don't ask again" checkbox for future startups.
3. App presents a config window where the user:
   - Selects their physical microphone from a dropdown of detected devices.
   - Sets their mute hotkey (any keyboard key or mouse button).
4. App minimizes to the system tray and begins routing.

## Functional Requirements

### FR-1: VB-CABLE Detection and Setup

- On startup, check whether "CABLE Input" (render) and "CABLE Output" (capture) endpoints exist in the system audio device list.
- If missing: prompt the user to install VB-CABLE. Provide a download link or bundled installer (respecting VB-Audio's distribution terms for v1 POC).
- Do not proceed with routing until VB-CABLE is confirmed present.

### FR-2: Default Microphone Management

- On startup (after VB-CABLE is confirmed), set "CABLE Output" as the Windows default microphone.
- First time: prompt the user with a confirmation dialog including a "Don't ask again" checkbox.
- Subsequent startups: apply automatically if the user checked "Don't ask again."
- On graceful exit: restore the original default microphone that was active before SMR changed it.
- Best-effort restoration only — if the app crashes, the virtual mic stays as default until the user changes it manually or restarts the app.

### FR-3: Physical Mic Selection and Capture

- Enumerate all active WASAPI capture devices and present them in a dropdown.
- Exclude the VB-CABLE virtual mic ("CABLE Output") from the physical mic list to avoid feedback loops.
- Capture audio from the selected physical mic using WASAPI shared mode (so the comms app can also use the physical mic simultaneously).
- Persist the user's mic selection across sessions.

### FR-4: Audio Routing

- Continuously read PCM audio from the physical mic via `WasapiCapture`.
- Write that audio to "CABLE Input" via `WasapiOut`.
- All apps using the system default mic ("CABLE Output") receive the routed audio.
- Prioritize stability over latency. Acceptable latency target: under 50ms. Use buffer sizes that avoid glitches and dropouts over low-latency tuning.

### FR-5: Global Mute Hotkey

- Allow the user to bind any single keyboard key or mouse button as the mute hotkey.
- The hotkey must work globally — even when a fullscreen game or other app has focus.
- While the hotkey is **held**: write silence (zero-filled buffers) to "CABLE Input" instead of real mic audio.
- While the hotkey is **released**: write real mic audio to "CABLE Input."
- Hotkey response should feel instant (no perceptible delay between keypress and mute).
- Persist the hotkey binding across sessions.

### FR-6: System Tray and Minimal UI

**System Tray:**
- App runs as a system tray icon.
- Tray icon or tooltip indicates current state: "Routing Active" vs. "Muted."
- Right-click context menu: "Open Settings", "Exit."

**Settings Window:**
- Physical microphone dropdown (FR-3).
- Hotkey binding control — click a button, press the desired key to bind.
- Visual mute status indicator (e.g., label or color change showing "Active" / "Muted").
- "Start with Windows" checkbox.
- Close button minimizes to tray rather than exiting the app.

### FR-7: Auto-Start with Windows

- Optional: user can enable "Start with Windows" from the settings window.
- Implemented via the current user's Startup folder or registry `Run` key.
- When auto-started, the app should start minimized to the tray and begin routing immediately (using saved settings).

## Non-Functional Requirements

- **Stability over latency**: The audio pipeline must not produce glitches, dropouts, or crashes. Buffer sizes should be conservative. Latency under 50ms is acceptable.
- **Resource usage**: Minimal CPU and memory footprint. The app is a background utility, not a DAW.
- **Single instance**: Only one instance of the app should run at a time.
- **Graceful degradation**: If "CABLE Input" or "CABLE Output" disappears mid-session (e.g., driver uninstalled), stop routing and notify the user via a tray notification. Do not crash.

## Technical Stack

| Component | Choice | Notes |
|---|---|---|
| Runtime | .NET 8 | LTS, single-file publish for distribution |
| UI | WPF | Modern look, better than WinForms for future polish |
| Audio | NAudio | WASAPI capture/render, device enumeration |
| Global hotkey | Win32 `SetWindowsHookEx` (low-level keyboard hook) or `RegisterHotKey` | Raw input for mouse buttons |
| Settings persistence | JSON file in `%AppData%\SMR\` | Simple, human-readable |
| Installer (v1) | None — run from build output | Revisit for v2 |

## Out of Scope for v1

- Per-app audio routing (routing different audio to different apps beyond the default mic split).
- Physical mic hot-plug detection and automatic reconnection (v2).
- Bundled VB-CABLE installer (v1 will link to the download; distribution agreement needed for bundling).
- Noise suppression, gain control, or audio processing.
- Multi-monitor / multi-hotkey support.
- Installer / auto-updater.

## Open Questions

1. **VB-CABLE redistribution**: For a future distributable release, should we pursue a distribution agreement with VB-Audio, or explore alternatives (e.g., Thesycon TVirtAudio SDK for a self-contained solution)?
2. **Crash recovery for default mic**: If the app crashes, "CABLE Output" stays as the system default mic. Should v2 include a watchdog or startup recovery that detects this state?
