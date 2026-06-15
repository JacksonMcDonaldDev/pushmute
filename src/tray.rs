//! System-tray indicator + control surface via the freedesktop
//! StatusNotifierItem spec (`ksni`). On Wayland/Hyprland the icon and its menu
//! are drawn by the bar's tray host (e.g. waybar); we only publish state over
//! D-Bus, so no GUI toolkit is pulled in.
//!
//! The menu *is* the config surface: enable/disable, mic selection, set-default,
//! and hotkey rebind. Muting itself is driven only by the hotkey, not the menu.
//! Config-changing actions write `config.toml` and ask the daemon to re-exec
//! (`Lifecycle::Restart`) so the change applies through the startup path;
//! unchecking set-default restores the prior device on that teardown.

use crate::config::Config;
use crate::daemon::{Daemon, Lifecycle};
use crate::{autostart, input, notify, pipewire};
use anyhow::Result;
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{MenuItem, Tray};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

const ICON_ACTIVE: &str = "audio-input-microphone";
const ICON_MUTED: &str = "microphone-sensitivity-muted";

pub struct PushMuteTray {
    daemon: Arc<Daemon>,
    tx: Sender<Lifecycle>,
}

impl Tray for PushMuteTray {
    // Left click opens the menu instead of firing an activate action; the menu is
    // our only control surface, so there's no separate primary action to run.
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "pushmute".into()
    }

    fn title(&self) -> String {
        "PushMute".into()
    }

    fn icon_name(&self) -> String {
        if self.daemon.is_muted() {
            ICON_MUTED
        } else {
            ICON_ACTIVE
        }
        .into()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            icon_name: self.icon_name(),
            icon_pixmap: Vec::new(),
            title: format!("PushMute — {}", self.daemon.state_label()),
            description: format!(
                "Mic: {}\nHotkey: {}",
                self.daemon.physical(),
                self.daemon.keys_display()
            ),
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let enabled = self.daemon.is_enabled();
        let mic = self.daemon.physical().to_string();
        let cfg = Config::load().unwrap_or_default();
        let devices = pipewire::list_capture_devices().unwrap_or_default();
        let names: Vec<String> = devices.iter().map(|d| d.name.clone()).collect();
        let selected = names.iter().position(|n| *n == mic).unwrap_or(usize::MAX);

        // Non-clickable status line: state dot + current mic.
        let header = StandardItem {
            label: format!("● {} — {mic}", self.daemon.state_label()),
            enabled: false,
            ..Default::default()
        };

        let enabled_item = CheckmarkItem {
            label: "Enabled".into(),
            checked: enabled,
            activate: Box::new(|t: &mut Self| {
                if let Err(e) = t.daemon.toggle_enabled() {
                    notify::error("Toggle failed", &e.to_string());
                }
            }),
            ..Default::default()
        };

        // Run on startup ↔ the systemd user unit's enabled state. Off by default
        // (install.sh starts the daemon without enabling it); this is how the user
        // opts in. No daemon restart: it only flips the boot symlink.
        let on_startup = autostart::is_enabled();
        let startup_item = CheckmarkItem {
            label: "Run on startup".into(),
            checked: on_startup,
            activate: Box::new(
                move |_t: &mut Self| match autostart::set_enabled(!on_startup) {
                    Ok(()) => notify::info(
                        "Run on startup",
                        if on_startup {
                            "Disabled — won't start on login"
                        } else {
                            "Enabled — starts on login"
                        },
                    ),
                    Err(e) => notify::error("Run on startup failed", &e.to_string()),
                },
            ),
            ..Default::default()
        };

        // Microphone ▸ radio submenu. Selecting writes config + restarts.
        let options: Vec<RadioItem> = devices
            .iter()
            .map(|d| RadioItem {
                label: d.description.clone(),
                ..Default::default()
            })
            .collect();
        let names_for_select = names.clone();
        let tx_mic = self.tx.clone();
        let mic_menu = SubMenu {
            label: "Microphone Input Source".into(),
            submenu: vec![MenuItem::RadioGroup(RadioGroup {
                selected,
                select: Box::new(move |_t: &mut Self, i: usize| {
                    let Some(name) = names_for_select.get(i) else {
                        return;
                    };
                    match set_physical_mic(name) {
                        Ok(()) => {
                            notify::info("Microphone", &format!("Switched to {name}"));
                            let _ = tx_mic.send(Lifecycle::Restart);
                        }
                        Err(e) => notify::error("Mic switch failed", &e.to_string()),
                    }
                }),
                options,
            })],
            ..Default::default()
        };

        let cur_default = cfg.set_default;
        let tx_def = self.tx.clone();
        let default_item = CheckmarkItem {
            label: "Set push-mute as system default source".into(),
            checked: cur_default,
            activate: Box::new(move |_t: &mut Self| match set_default_flag(!cur_default) {
                Ok(()) => {
                    // Unchecking hands the default source back: the daemon restart
                    // restores the prior device on teardown, so tell the user.
                    if cur_default {
                        match Config::load().ok().and_then(|c| c.previous_default) {
                            Some(prev) => {
                                notify::info("Default source", &format!("Restored → {prev}"))
                            }
                            None => notify::info("Default source", "Restored default source"),
                        }
                    }
                    let _ = tx_def.send(Lifecycle::Restart);
                }
                Err(e) => notify::error("Config error", &e.to_string()),
            }),
            ..Default::default()
        };

        // Non-clickable hint showing the currently bound chord by name.
        let hotkey_hint = StandardItem {
            label: format!("Hotkey: {}", self.daemon.keys_display()),
            enabled: false,
            ..Default::default()
        };

        let tx_rebind = self.tx.clone();
        let rebind_device = cfg.hotkey_device.clone();
        let rebind_item = StandardItem {
            label: "Rebind hotkey…".into(),
            activate: Box::new(move |_t: &mut Self| {
                // capture_combo blocks until a chord is released, so it must not
                // run on the tray's D-Bus thread.
                let tx = tx_rebind.clone();
                let device = rebind_device.clone();
                std::thread::spawn(move || rebind_hotkey(device, tx));
            }),
            ..Default::default()
        };

        let tx_quit = self.tx.clone();
        let quit_item = StandardItem {
            label: "Quit".into(),
            activate: Box::new(move |_t: &mut Self| {
                let _ = tx_quit.send(Lifecycle::Quit);
            }),
            ..Default::default()
        };

        vec![
            header.into(),
            MenuItem::Separator,
            enabled_item.into(),
            startup_item.into(),
            MenuItem::Separator,
            mic_menu.into(),
            default_item.into(),
            hotkey_hint.into(),
            rebind_item.into(),
            MenuItem::Separator,
            quit_item.into(),
        ]
    }
}

/// Publish the tray. Returns `None` (and logs) if no StatusNotifier host is
/// reachable, so the daemon can still run CLI-only.
///
/// `assume_sni_available(true)` makes ksni tolerate the `StatusNotifierWatcher`
/// being absent at spawn time: instead of failing, it waits and registers when
/// the watcher (e.g. waybar's tray host) appears. This is what lets the systemd
/// unit start from `default.target` without ordering after the graphical session
/// — pushmute can come up before the bar and still get its icon once the bar is
/// ready. It also means a bar restart/reload re-acquires the icon automatically.
pub fn spawn(daemon: Arc<Daemon>, tx: Sender<Lifecycle>) -> Option<Handle<PushMuteTray>> {
    match (PushMuteTray { daemon, tx })
        .assume_sni_available(true)
        .spawn()
    {
        Ok(handle) => Some(handle),
        Err(e) => {
            eprintln!("pushmute: tray unavailable: {e}");
            None
        }
    }
}

/// Repaint the icon when the mute state changes from outside the menu (hotkey, the
/// CLI socket). ksni already re-renders after a menu click; this covers the rest
/// by polling the atomic — a cheap load every 60 ms, a sustained hotkey is caught
/// well within human perception.
pub fn watch_mute(daemon: Arc<Daemon>, handle: Handle<PushMuteTray>) {
    std::thread::spawn(move || {
        let mut last = daemon.is_muted();
        loop {
            std::thread::sleep(Duration::from_millis(60));
            if handle.is_closed() {
                break;
            }
            let cur = daemon.is_muted();
            if cur != last {
                last = cur;
                handle.update(|_| {});
            }
        }
    });
}

fn set_physical_mic(name: &str) -> Result<()> {
    let mut cfg = Config::load()?;
    cfg.physical_mic = Some(name.to_string());
    cfg.save()
}

fn set_default_flag(value: bool) -> Result<()> {
    let mut cfg = Config::load()?;
    cfg.set_default = value;
    cfg.save()
}

fn rebind_hotkey(device: Option<String>, tx: Sender<Lifecycle>) {
    notify::info("Rebind hotkey", "Press your key or chord, then release…");
    let keys = match input::capture_combo(device.clone()) {
        Ok(k) => k,
        Err(e) => {
            notify::error("Rebind failed", &e.to_string());
            return;
        }
    };
    let shown = crate::daemon::fmt_key_names(&keys);
    let result = (|| -> Result<()> {
        let mut cfg = Config::load()?;
        cfg.hotkey_keys = keys;
        cfg.hotkey_device = device;
        cfg.save()
    })();
    match result {
        Ok(()) => {
            notify::info("Hotkey bound", &shown);
            let _ = tx.send(Lifecycle::Restart);
        }
        Err(e) => notify::error("Rebind failed", &e.to_string()),
    }
}
