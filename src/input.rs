//! Global hotkey via evdev.
//!
//! Reads `/dev/input/event*` directly (works under any compositor, no root
//! needed because the user is in the `input` group). We never `EVIOCGRAB`, so
//! the comms app still sees the same key for its own push-to-talk.

use anyhow::{anyhow, Context, Result};
use evdev::{Device, EventType};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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

/// Spawn background listeners that invoke `on_edge(active)` whenever the chord
/// state changes — `active` is true only while *every* key in `keys` is held.
/// A single-element `keys` is a plain single-key bind. Threads are detached and
/// die with the process.
pub fn spawn_listeners<F>(keys: Vec<u16>, device: Option<String>, on_edge: F) -> Result<()>
where
    F: Fn(bool) + Send + Sync + 'static,
{
    let keys = Arc::new(keys);
    let down: Arc<Vec<AtomicBool>> =
        Arc::new(keys.iter().map(|_| AtomicBool::new(false)).collect());
    let active = Arc::new(Mutex::new(false));
    let cb = Arc::new(on_edge);

    for (path, mut dev) in open_devices(&device)? {
        let keys = keys.clone();
        let down = down.clone();
        let active = active.clone();
        let cb = cb.clone();
        thread::spawn(move || loop {
            match dev.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        if ev.event_type() != EventType::KEY {
                            continue;
                        }
                        if let Some(i) = keys.iter().position(|k| *k == ev.code()) {
                            // value: 1 press, 2 autorepeat (still down), 0 release.
                            down[i].store(ev.value() != 0, Ordering::Relaxed);
                            let all = down.iter().all(|b| b.load(Ordering::Relaxed));
                            let mut a = active.lock().unwrap();
                            if *a != all {
                                *a = all;
                                drop(a);
                                cb(all);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("pushmute: input device {} closed: {e}", path.display());
                    break;
                }
            }
        });
    }
    Ok(())
}

/// Block until the user presses a key (or chord) and releases it, returning the
/// full set of keys that were held simultaneously. Used by `pushmute set-key`.
pub fn capture_combo(device: Option<String>) -> Result<Vec<u16>> {
    use std::sync::mpsc::channel;
    let (tx, rx) = channel::<(u16, i32)>();
    for (_path, mut dev) in open_devices(&device)? {
        let tx = tx.clone();
        thread::spawn(move || loop {
            match dev.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        if ev.event_type() == EventType::KEY
                            && tx.send((ev.code(), ev.value())).is_err()
                        {
                            return;
                        }
                    }
                }
                Err(_) => return,
            }
        });
    }
    drop(tx);

    let mut held = BTreeSet::new();
    let mut high: BTreeSet<u16> = BTreeSet::new();
    loop {
        let (code, value) = rx.recv().context("waiting for a key press")?;
        if value != 0 {
            held.insert(code);
            if held.len() > high.len() {
                high = held.clone(); // high-water mark of simultaneously-held keys
            }
        } else {
            held.remove(&code);
            if held.is_empty() && !high.is_empty() {
                return Ok(high.into_iter().collect());
            }
        }
    }
}
