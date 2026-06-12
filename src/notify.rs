//! Desktop notifications via `notify-send` (best-effort; absence is non-fatal).
//!
//! Policy: notify only on the hotkey-rebind flow, applied config changes, and
//! errors — never on routine mute/unmute, which is the tray icon's job.

use std::process::Command;

fn send(urgency: &str, summary: &str, body: &str) {
    // `.status()` reaps the child (avoids zombies); notify-send returns promptly.
    let _ = Command::new("notify-send")
        .args([
            "-a",
            "PushMute",
            "-i",
            "audio-input-microphone",
            "-u",
            urgency,
        ])
        .arg(summary)
        .arg(body)
        .status();
}

/// A normal-priority notification (rebind prompts, applied config changes).
pub fn info(summary: &str, body: &str) {
    send("normal", summary, body);
}

/// A critical notification for failures the user should see.
pub fn error(summary: &str, body: &str) {
    send("critical", summary, body);
}
