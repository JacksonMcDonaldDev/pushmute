//! Environment preflight: the `pushmute doctor` subcommand and the critical-check
//! gate that `daemon::run` calls before provisioning anything.
//!
//! Checks split by severity (mirroring `DEPLOYMENT_PLAN.md`):
//! - **Critical** — `pw-dump`/`wpctl`/`pw-loopback` missing from PATH, an
//!   unparseable graph, or the user not in the `input` group. These abort `run`.
//! - **Warning** — no `StatusNotifierWatcher` on the session bus: the tray won't
//!   appear but routing still works, so we proceed with a notice at every startup.

use crate::pipewire;
use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;

enum Severity {
    Critical,
    Warning,
}

/// One environment check. `outcome` is `Ok(note)` on pass (the optional note is
/// shown alongside PASS) or `Err(reason)` on failure.
struct Check {
    name: String,
    severity: Severity,
    outcome: Result<Option<String>, String>,
}

/// `pushmute doctor` — run every check, print PASS/WARN/FAIL, and fail the command
/// if any *critical* check did, so the exit code is usable from scripts.
pub fn doctor() -> Result<()> {
    println!("pushmute: checking environment…\n");
    let mut critical_failed = false;
    for check in run_checks() {
        let (tag, detail) = match (&check.severity, &check.outcome) {
            (_, Ok(note)) => ("PASS", note.clone()),
            (Severity::Warning, Err(reason)) => ("WARN", Some(reason.clone())),
            (Severity::Critical, Err(reason)) => {
                critical_failed = true;
                ("FAIL", Some(reason.clone()))
            }
        };
        match detail {
            Some(d) => println!("  [{tag}] {} — {d}", check.name),
            None => println!("  [{tag}] {}", check.name),
        }
    }
    if critical_failed {
        Err(anyhow!(
            "environment has critical problems — fix the [FAIL] items above"
        ))
    } else {
        println!("\npushmute: environment looks good.");
        Ok(())
    }
}

/// The startup gate for `daemon::run`. Aborts on the first critical failure;
/// emits any warnings (e.g. a missing tray watcher) to stderr and proceeds.
pub fn preflight() -> Result<()> {
    for check in run_checks() {
        match (&check.severity, check.outcome) {
            (Severity::Critical, Err(reason)) => return Err(anyhow!("{}: {reason}", check.name)),
            (Severity::Warning, Err(reason)) => eprintln!("pushmute: warning: {reason}"),
            _ => {}
        }
    }
    Ok(())
}

fn run_checks() -> Vec<Check> {
    let mut checks = vec![
        path_check("pw-dump"),
        path_check("wpctl"),
        path_check("pw-loopback"),
    ];
    // Only probe the live graph once the tools that drive it are present —
    // otherwise the probe failure is just noise stacked on the PATH failure.
    let tools_present = checks.iter().all(|c| c.outcome.is_ok());
    checks.push(probe_check(
        "pw-dump output parses",
        tools_present,
        pipewire::probe_pw_dump(),
    ));
    checks.push(probe_check(
        "wpctl responds",
        tools_present,
        pipewire::probe_wpctl(),
    ));
    checks.push(input_group_check());
    checks.push(sni_check());
    checks
}

fn path_check(bin: &str) -> Check {
    let outcome = if which(bin) {
        Ok(None)
    } else {
        Err(format!(
            "`{bin}` not found in PATH — install pipewire / wireplumber"
        ))
    };
    Check {
        name: format!("`{bin}` on PATH"),
        severity: Severity::Critical,
        outcome,
    }
}

/// Wrap a graph probe (`probe_pw_dump`/`probe_wpctl`) as a critical check, skipped
/// when its underlying tools are absent (already reported by the PATH checks).
fn probe_check(name: &str, tools_present: bool, probe: Result<()>) -> Check {
    let outcome = if !tools_present {
        Ok(Some("skipped (tool missing)".into()))
    } else {
        probe.map(|_| None).map_err(|e| format!("{e:#}"))
    };
    Check {
        name: name.to_string(),
        severity: Severity::Critical,
        outcome,
    }
}

fn input_group_check() -> Check {
    let outcome = if in_input_group() {
        Ok(None)
    } else {
        Err("user is not in the `input` group — \
             run `sudo usermod -aG input $USER`, then log out and back in"
            .to_string())
    };
    Check {
        name: "`input` group membership".to_string(),
        severity: Severity::Critical,
        outcome,
    }
}

fn sni_check() -> Check {
    let outcome = match sni_watcher_present() {
        None => Ok(Some("skipped (busctl unavailable)".into())),
        Some(true) => Ok(None),
        Some(false) => Err(sni_absent_reason(is_gnome())),
    };
    Check {
        name: "system tray (StatusNotifierWatcher)".to_string(),
        severity: Severity::Warning,
        outcome,
    }
}

/// The warning shown when no tray watcher is on the bus. On GNOME the watcher
/// ships only with the AppIndicator extension, so point there specifically.
fn sni_absent_reason(is_gnome: bool) -> String {
    let mut reason = "no StatusNotifierWatcher on the session bus — the tray icon \
                      won't appear (audio routing still works)"
        .to_string();
    if is_gnome {
        reason.push_str(
            "; on GNOME, install the AppIndicator support extension \
             (https://extensions.gnome.org/extension/615/appindicator-support/)",
        );
    }
    reason
}

fn is_gnome() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|d| d.to_uppercase().contains("GNOME"))
        .unwrap_or(false)
}

/// Is `bin` an executable file on `$PATH`? A small `which`, kept here so doctor
/// has no extra dependency.
fn which(bin: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    path.split(':')
        .filter(|d| !d.is_empty())
        .any(|dir| is_executable(&Path::new(dir).join(bin)))
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn in_input_group() -> bool {
    Command::new("id")
        .arg("-nG")
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .any(|g| g == "input")
        })
        .unwrap_or(false)
}

/// `Some(present)` from the session bus, or `None` when we can't tell — `busctl`
/// absent (non-systemd) shouldn't surface as a failed tray check.
fn sni_watcher_present() -> Option<bool> {
    if !which("busctl") {
        return None;
    }
    let out = Command::new("busctl")
        .args(["--user", "list"])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).contains("org.kde.StatusNotifierWatcher"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_finds_a_real_binary_and_misses_a_fake_one() {
        assert!(which("sh"), "`sh` should resolve on any PATH");
        assert!(!which("pushmute-not-a-real-binary-xyz"));
    }

    #[test]
    fn gnome_reason_points_at_the_appindicator_extension() {
        assert!(sni_absent_reason(true).contains("AppIndicator"));
        assert!(!sni_absent_reason(false).contains("AppIndicator"));
        // The base message is present either way.
        assert!(sni_absent_reason(false).contains("tray icon"));
    }

    #[test]
    fn probe_check_skips_when_tools_absent() {
        let c = probe_check("x", false, Err(anyhow!("should be ignored")));
        assert!(
            c.outcome.is_ok(),
            "absent tools skip rather than double-fail"
        );
    }

    #[test]
    fn probe_check_reports_failure_when_tools_present() {
        let c = probe_check("x", true, Err(anyhow!("boom")));
        assert_eq!(c.outcome.unwrap_err(), "boom");
    }
}
