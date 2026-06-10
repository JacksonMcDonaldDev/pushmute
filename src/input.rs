//! Global push-to-talk via evdev.
//!
//! Reads `/dev/input/event*` directly (works under any compositor, no root
//! needed because the user is in the `input` group). We never `EVIOCGRAB`, so
//! the comms app still sees the same key for its own PTT.

use anyhow::{anyhow, Context, Result};
use evdev::{Device, EventType};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

/// Open the keyboard-capable input devices to listen on. If `only` is set, just
/// that device; otherwise every device advertising KEY events.
fn open_devices(only: &Option<String>) -> Result<Vec<(PathBuf, Device)>> {
    if let Some(path) = only {
        let dev = Device::open(path).with_context(|| format!("opening {path}"))?;
        return Ok(vec![(PathBuf::from(path), dev)]);
    }
    let mut out = Vec::new();
    for (path, dev) in evdev::enumerate() {
        if dev.supported_events().contains(EventType::KEY) {
            out.push((path, dev));
        }
    }
    if out.is_empty() {
        return Err(anyhow!("no readable input devices with KEY events found"));
    }
    Ok(out)
}

/// Spawn background listeners that invoke `on_edge(pressed)` on each press/release
/// of `keycode`. Threads are detached and die with the process.
pub fn spawn_listeners<F>(keycode: u16, device: Option<String>, on_edge: F) -> Result<()>
where
    F: Fn(bool) + Send + Sync + 'static,
{
    let cb = Arc::new(on_edge);
    for (path, mut dev) in open_devices(&device)? {
        let cb = cb.clone();
        thread::spawn(move || loop {
            match dev.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        if ev.event_type() == EventType::KEY && ev.code() == keycode {
                            match ev.value() {
                                1 => cb(true),  // press
                                0 => cb(false), // release
                                _ => {}         // 2 = autorepeat, ignore
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("smr: input device {} closed: {e}", path.display());
                    break;
                }
            }
        });
    }
    Ok(())
}

/// Block until the next key press and return its keycode. Used by `smr set-key`.
pub fn capture_keycode(device: Option<String>) -> Result<u16> {
    use std::sync::mpsc::channel;
    let (tx, rx) = channel::<u16>();
    for (_path, mut dev) in open_devices(&device)? {
        let tx = tx.clone();
        thread::spawn(move || loop {
            match dev.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        if ev.event_type() == EventType::KEY && ev.value() == 1 {
                            let _ = tx.send(ev.code());
                            return;
                        }
                    }
                }
                Err(_) => return,
            }
        });
    }
    rx.recv().context("waiting for a key press")
}
