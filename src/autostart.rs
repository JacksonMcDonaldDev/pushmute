//! Run-on-startup, expressed as the enabled state of the `pushmute` systemd user
//! unit. We deliberately support only systemd (not an XDG `~/.config/autostart`
//! entry): the unit is the documented primary path, and the DEs where a `.desktop`
//! fallback would actually fire (XFCE/MATE) can't render our SNI tray anyway. The
//! unit is `WantedBy=default.target` so it autostarts in *any* systemd --user
//! session — including bare Hyprland/sway, which never reach
//! `graphical-session.target`. Non-systemd sessions add their own
//! `exec pushmute run` instead.

use anyhow::{bail, Context, Result};
use std::process::Command;

const UNIT: &str = "pushmute.service";

/// True if the unit is enabled to start on login. Treats any failure (no systemd,
/// unit absent) as "not on startup" so the tray checkbox renders unchecked rather
/// than vanishing.
pub fn is_enabled() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-enabled", UNIT])
        .output()
        .map(|o| o.stdout.trim_ascii_end() == b"enabled")
        .unwrap_or(false)
}

/// Enable or disable start-on-login. Uses no `--now`: the daemon making this call
/// is already running, so we only touch the boot symlink, never the live unit.
pub fn set_enabled(value: bool) -> Result<()> {
    let verb = if value { "enable" } else { "disable" };
    let out = Command::new("systemctl")
        .args(["--user", verb, UNIT])
        .output()
        .with_context(|| format!("running systemctl --user {verb} {UNIT}"))?;
    if !out.status.success() {
        bail!(
            "systemctl --user {verb} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}
