//! System-tray indicator + control surface via the freedesktop
//! StatusNotifierItem spec (`ksni`). On Wayland/Hyprland the icon and its menu
//! are drawn by the bar's tray host (e.g. waybar); we only publish state over
//! D-Bus, so no GUI toolkit is pulled in.
//!
//! The menu *is* the config surface: mute toggle, mic selection, set-default,
//! and hotkey rebind. Live actions (mute, restore) act on the running daemon;
//! config-changing actions write `config.toml` and ask the daemon to re-exec
//! (`Lifecycle::Restart`) so the change applies through the startup path.

use crate::config::Config;
use crate::daemon::{Daemon, Lifecycle};
use crate::{input, notify, pipewire};
use anyhow::Result;
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{MenuItem, Tray};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

const ICON_ACTIVE: &str = "audio-input-microphone";
const ICON_MUTED: &str = "microphone-sensitivity-muted";

pub struct SmrTray {
    daemon: Arc<Daemon>,
    tx: Sender<Lifecycle>,
}

impl Tray for SmrTray {
    fn id(&self) -> String {
        "smr".into()
    }

    fn title(&self) -> String {
        "SMR".into()
    }

    fn icon_name(&self) -> String {
        if self.daemon.is_muted() { ICON_MUTED } else { ICON_ACTIVE }.into()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let state = if self.daemon.is_muted() {
            "Muted"
        } else {
            "Routing Active"
        };
        ksni::ToolTip {
            icon_name: self.icon_name(),
            icon_pixmap: Vec::new(),
            title: format!("SMR — {state}"),
            description: format!(
                "Mic: {}\nHotkey: {}",
                self.daemon.physical(),
                self.daemon.keys_display()
            ),
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let muted = self.daemon.is_muted();
        let mic = self.daemon.physical().to_string();
        let cfg = Config::load().unwrap_or_default();
        let devices = pipewire::list_capture_devices().unwrap_or_default();
        let names: Vec<String> = devices.iter().map(|d| d.name.clone()).collect();
        let selected = names.iter().position(|n| *n == mic).unwrap_or(usize::MAX);

        // Disabled status line: state dot + current mic.
        let dot = if muted { "● Muted" } else { "● Routing Active" };
        let header = StandardItem {
            label: format!("{dot} — {mic}"),
            enabled: false,
            ..Default::default()
        };

        let mute_item = CheckmarkItem {
            label: "Muted".into(),
            checked: muted,
            activate: Box::new(|t: &mut Self| {
                if let Err(e) = t.daemon.toggle() {
                    notify::error("Mute failed", &e.to_string());
                }
            }),
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
            label: "Microphone".into(),
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
                ..Default::default()
            })],
            ..Default::default()
        };

        let cur_default = cfg.set_default;
        let tx_def = self.tx.clone();
        let default_item = CheckmarkItem {
            label: "Set as default source".into(),
            checked: cur_default,
            activate: Box::new(move |_t: &mut Self| match set_default_flag(!cur_default) {
                Ok(()) => {
                    let _ = tx_def.send(Lifecycle::Restart);
                }
                Err(e) => notify::error("Config error", &e.to_string()),
            }),
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

        let restore_item = StandardItem {
            label: "Restore default source".into(),
            activate: Box::new(|_t: &mut Self| match restore_default() {
                Ok(Some(prev)) => notify::info("Default source", &format!("Restored → {prev}")),
                Ok(None) => notify::info("Default source", "No previous default recorded"),
                Err(e) => notify::error("Restore failed", &e.to_string()),
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
            mute_item.into(),
            MenuItem::Separator,
            mic_menu.into(),
            default_item.into(),
            rebind_item.into(),
            MenuItem::Separator,
            restore_item.into(),
            quit_item.into(),
        ]
    }
}

/// Publish the tray. Returns `None` (and logs) if no StatusNotifier host is
/// reachable, so the daemon can still run CLI-only.
pub fn spawn(daemon: Arc<Daemon>, tx: Sender<Lifecycle>) -> Option<Handle<SmrTray>> {
    match (SmrTray { daemon, tx }).spawn() {
        Ok(handle) => Some(handle),
        Err(e) => {
            eprintln!("smr: tray unavailable: {e}");
            None
        }
    }
}

/// Repaint the icon when the mute state changes from outside the menu (hotkey, the
/// CLI socket). ksni already re-renders after a menu click; this covers the rest
/// by polling the atomic — a cheap load every 60 ms, a sustained hotkey is caught
/// well within human perception.
pub fn watch_mute(daemon: Arc<Daemon>, handle: Handle<SmrTray>) {
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

fn restore_default() -> Result<Option<String>> {
    let cfg = Config::load()?;
    match cfg.previous_default {
        Some(prev) => {
            pipewire::set_default_source(&prev)?;
            Ok(Some(prev))
        }
        None => Ok(None),
    }
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
    let shown = keys.iter().map(u16::to_string).collect::<Vec<_>>().join("+");
    let result = (|| -> Result<()> {
        let mut cfg = Config::load()?;
        cfg.hotkey_keys = keys;
        cfg.hotkey_device = device;
        cfg.save()
    })();
    match result {
        Ok(()) => {
            notify::info("Hotkey bound", &format!("evdev {shown}"));
            let _ = tx.send(Lifecycle::Restart);
        }
        Err(e) => notify::error("Rebind failed", &e.to_string()),
    }
}
