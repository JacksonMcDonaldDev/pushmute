//! The long-running router: provisions the virtual source, wires the hotkey to mute,
//! and restores the graph on exit.

use crate::config::{Config, PUSHMUTE_DESCRIPTION, PUSHMUTE_NODE_NAME};
use crate::{doctor, input, ipc, pipewire, tray};
use anyhow::{anyhow, Result};
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

/// How the daemon should stop once the main thread is unblocked. Config-changing
/// tray actions write `config.toml` and ask for a `Restart`, which re-execs the
/// process so the change is applied through the normal startup path.
pub enum Lifecycle {
    Quit,
    Restart,
}

/// Shared daemon state. Mute is the hot path so the node id and muted flag are
/// lock-free atomics. `enabled` gates the whole hotkey mechanism: when off, the mic
/// is forced open and key events are ignored.
pub struct Daemon {
    node_id: AtomicU32,
    muted: AtomicBool,
    enabled: AtomicBool,
    physical: String,
    keys: Vec<u16>,
}

/// Raw, machine-friendly rendering of a chord (e.g. `"56+183"`). Used by the
/// status line, which is parsed/grepped rather than read by a person.
fn fmt_keys(keys: &[u16]) -> String {
    if keys.is_empty() {
        "unset".into()
    } else {
        keys.iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join("+")
    }
}

/// Human-readable rendering of a chord (e.g. `"Left Ctrl + F13"`). Used by the
/// tray and notifications, where a bare keycode means nothing to the user.
pub fn fmt_key_names(keys: &[u16]) -> String {
    if keys.is_empty() {
        "unset".into()
    } else {
        keys.iter()
            .map(|&c| key_name(c))
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

/// Translate a single evdev keycode into a friendly name, falling back to the
/// raw number for codes the `evdev` crate doesn't recognise.
fn key_name(code: u16) -> String {
    let raw = format!("{:?}", evdev::Key::new(code));
    let Some(rest) = raw.strip_prefix("KEY_") else {
        return code.to_string();
    };
    // Side-prefixed modifiers (LEFTCTRL → "Left Ctrl"). Guard on the suffix so
    // we don't mangle the arrow keys (KEY_LEFT/KEY_RIGHT) or KEY_RIGHTBRACE.
    let is_mod = |s: &str| matches!(s, "CTRL" | "SHIFT" | "ALT" | "META");
    let (side, token) = if rest.strip_prefix("LEFT").is_some_and(is_mod) {
        ("Left ", &rest[4..])
    } else if rest.strip_prefix("RIGHT").is_some_and(is_mod) {
        ("Right ", &rest[5..])
    } else {
        ("", rest)
    };
    let name = if token == "META" {
        "Super".to_string()
    } else {
        title_case(token)
    };
    format!("{side}{name}")
}

/// Uppercase the first character and lowercase the rest (`"SPACE"` → `"Space"`,
/// `"F13"` → `"F13"`, `"A"` → `"A"`).
fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect(),
        None => String::new(),
    }
}

impl Daemon {
    pub fn set_mute(&self, mute: bool) -> Result<()> {
        let id = self.node_id.load(Ordering::Relaxed);
        if id == 0 {
            return Err(anyhow!("virtual source not ready"));
        }
        pipewire::set_mute_id(id, mute)?;
        self.muted.store(mute, Ordering::Relaxed);
        Ok(())
    }

    pub fn toggle(&self) -> Result<()> {
        let next = !self.muted.load(Ordering::Relaxed);
        self.set_mute(next)
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Turn hotkey routing on or off. While disabled the mic stays open and key
    /// events are ignored, so disabling first flips the gate, then forces the
    /// source unmuted (covering the case where a key was held at the time).
    pub fn set_enabled(&self, enabled: bool) -> Result<()> {
        self.enabled.store(enabled, Ordering::Relaxed);
        if !enabled {
            self.set_mute(false)?;
        }
        Ok(())
    }

    pub fn toggle_enabled(&self) -> Result<()> {
        let next = !self.enabled.load(Ordering::Relaxed);
        self.set_enabled(next)
    }

    /// The `node.name` of the physical mic being routed (for the tray surface).
    pub fn physical(&self) -> &str {
        &self.physical
    }

    /// The bound hotkey chord, rendered for display (e.g. `"Left Ctrl + F13"`
    /// or `"unset"`).
    pub fn keys_display(&self) -> String {
        fmt_key_names(&self.keys)
    }

    /// Human-readable routing state for the tray surface and status line.
    pub fn state_label(&self) -> &'static str {
        if !self.enabled.load(Ordering::Relaxed) {
            "Disabled"
        } else if self.muted.load(Ordering::Relaxed) {
            "Muted"
        } else {
            "Routing Active"
        }
    }

    pub fn status_line(&self) -> String {
        format!(
            "state={} mic={} virtual={PUSHMUTE_DESCRIPTION} hotkey_keys={}",
            self.state_label(),
            self.physical,
            fmt_keys(&self.keys)
        )
    }
}

/// Run the daemon to completion (blocks until SIGINT/SIGTERM).
pub fn run(mut config: Config) -> Result<()> {
    // Single-instance guard + control socket. Checked first so a duplicate launch
    // (launcher click, autostart race, terminal) exits 0 cleanly even before we'd
    // complain about an unset mic.
    let listener = match ipc::bind()? {
        ipc::Bind::Listener(l) => l,
        ipc::Bind::AlreadyRunning => {
            eprintln!("pushmute: already running");
            return Ok(());
        }
    };

    // Critical environment checks (PATH tools, parseable graph, `input` group)
    // abort here before we provision anything; tray-watcher warnings are printed.
    doctor::preflight()?;

    let physical = config.physical_mic.clone().ok_or_else(|| {
        anyhow!("no physical mic set — run `pushmute set-mic <name>` (`pushmute devices` to list)")
    })?;

    // Record the pre-existing default source so we can restore it on exit.
    if config.set_default {
        if let Some(cur) = pipewire::current_default_source()? {
            if cur != PUSHMUTE_NODE_NAME {
                config.previous_default = Some(cur);
                config.save()?;
            }
        }
    }

    // 1. Provision the virtual source.
    println!("pushmute: creating virtual source `{PUSHMUTE_DESCRIPTION}` ← {physical}");
    let mut child: Child = pipewire::spawn_loopback(&physical)?;
    let node_id = match pipewire::wait_for_node(PUSHMUTE_NODE_NAME, 30) {
        Ok(id) => id,
        Err(e) => {
            let _ = child.kill();
            return Err(e);
        }
    };

    // 2. Set default source.
    if config.set_default {
        match pipewire::set_default_source(PUSHMUTE_NODE_NAME) {
            Ok(()) => println!("pushmute: default source → {PUSHMUTE_DESCRIPTION}"),
            Err(e) => eprintln!("pushmute: could not set default source: {e}"),
        }
    }

    let daemon = Arc::new(Daemon {
        node_id: AtomicU32::new(node_id),
        muted: AtomicBool::new(false),
        enabled: AtomicBool::new(true),
        physical: physical.clone(),
        keys: config.hotkey_keys.clone(),
    });

    // 3. Control socket.
    ipc::serve(listener, daemon.clone());

    // 4. Hotkey listeners.
    if config.hotkey_keys.is_empty() {
        eprintln!("pushmute: no hotkey set — routing only (run `pushmute set-key`)");
    } else {
        let d = daemon.clone();
        input::spawn_listeners(
            config.hotkey_keys.clone(),
            config.hotkey_device.clone(),
            move |active| {
                // While the app is disabled the mic stays open: ignore hotkey edges.
                if !d.is_enabled() {
                    return;
                }
                if let Err(e) = d.set_mute(active) {
                    eprintln!("pushmute: mute toggle failed: {e}");
                }
            },
        )?;
        println!(
            "pushmute: hotkey armed on {}",
            fmt_key_names(&config.hotkey_keys)
        );
    }

    // 5. Lifecycle channel — Ctrl-C, the tray's "Quit", and config-change
    //    "Restart" all funnel here so the main thread owns teardown.
    let (life_tx, life_rx) = mpsc::channel::<Lifecycle>();
    let ctrlc_tx = life_tx.clone();
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.send(Lifecycle::Quit);
    })?;

    // 6. System-tray indicator. Optional: if no StatusNotifier host is present
    //    the daemon runs headless and the CLI still drives it.
    if let Some(handle) = tray::spawn(daemon.clone(), life_tx.clone()) {
        tray::watch_mute(daemon.clone(), handle);
        println!("pushmute: tray indicator active");
    } else {
        eprintln!("pushmute: no system tray available — running CLI-only");
    }

    println!("pushmute: ready. {}", daemon.status_line());

    // 7. Block until told to quit or restart, then clean up.
    let event = life_rx.recv().unwrap_or(Lifecycle::Quit);
    println!("\npushmute: shutting down…");
    cleanup(&mut child, &config);
    match event {
        Lifecycle::Quit => Ok(()),
        Lifecycle::Restart => reexec(),
    }
}

/// Replace this process with a fresh `pushmute run`, applying config written to disk
/// by a tray action. Teardown (default-source restore, loopback kill, socket
/// removal) must already have run via `cleanup`.
fn reexec() -> ! {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe().unwrap_or_else(|_| "pushmute".into());
    let err = std::process::Command::new(exe).arg("run").exec();
    eprintln!("pushmute: re-exec failed: {err}");
    std::process::exit(1);
}

fn cleanup(child: &mut Child, config: &Config) {
    // Restore the previous default source, best effort.
    if let Some(prev) = &config.previous_default {
        match pipewire::set_default_source(prev) {
            Ok(()) => println!("pushmute: default source restored → {prev}"),
            Err(e) => eprintln!("pushmute: could not restore default source: {e}"),
        }
    }
    // Tear down the virtual source.
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(ipc::socket_path());
    println!("pushmute: virtual source removed. bye.");
}

/// `pushmute restore` — reset the default source to the recorded prior device,
/// without a full daemon run.
pub fn restore(config: &Config) -> Result<()> {
    match &config.previous_default {
        Some(prev) => {
            pipewire::set_default_source(prev)?;
            println!("default source restored → {prev}");
            Ok(())
        }
        None => Err(anyhow!("no previous default source recorded")),
    }
}

#[cfg(test)]
mod key_name_tests {
    use super::*;

    #[test]
    fn renders_friendly_names() {
        assert_eq!(key_name(29), "Left Ctrl"); // KEY_LEFTCTRL
        assert_eq!(key_name(183), "F13"); // KEY_F13
        assert_eq!(key_name(30), "A"); // KEY_A
        assert_eq!(key_name(57), "Space"); // KEY_SPACE
        assert_eq!(key_name(125), "Left Super"); // KEY_LEFTMETA
        assert_eq!(key_name(105), "Left"); // KEY_LEFT (arrow), not mistaken for a modifier
    }

    #[test]
    fn renders_chords_and_empty() {
        assert_eq!(fmt_key_names(&[29, 183]), "Left Ctrl + F13");
        assert_eq!(fmt_key_names(&[]), "unset");
    }
}
