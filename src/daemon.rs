//! The long-running router: provisions the virtual source, wires PTT to mute,
//! and restores the graph on exit.

use crate::config::{Config, SMR_DESCRIPTION, SMR_NODE_NAME};
use crate::{input, ipc, pipewire};
use anyhow::{anyhow, Result};
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

/// Shared daemon state. Mute is the hot path so the node id and muted flag are
/// lock-free atomics.
pub struct Daemon {
    node_id: AtomicU32,
    muted: AtomicBool,
    physical: String,
    keycode: Option<u16>,
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

    pub fn status_line(&self) -> String {
        let state = if self.muted.load(Ordering::Relaxed) {
            "Muted"
        } else {
            "Routing Active"
        };
        let key = self
            .keycode
            .map(|k| k.to_string())
            .unwrap_or_else(|| "unset".into());
        format!(
            "state={state} mic={} virtual={SMR_DESCRIPTION} ptt_keycode={key}",
            self.physical
        )
    }
}

/// Run the daemon to completion (blocks until SIGINT/SIGTERM).
pub fn run(mut config: Config) -> Result<()> {
    let physical = config
        .physical_mic
        .clone()
        .ok_or_else(|| anyhow!("no physical mic set — run `smr set-mic <name>` (`smr devices` to list)"))?;

    // Single-instance guard + control socket.
    let listener = ipc::bind()?;

    // Record the pre-existing default source so we can restore it on exit.
    if config.set_default {
        if let Some(cur) = pipewire::current_default_source()? {
            if cur != SMR_NODE_NAME {
                config.previous_default = Some(cur);
                config.save()?;
            }
        }
    }

    // 1. Provision the virtual source.
    println!("smr: creating virtual source `{SMR_DESCRIPTION}` ← {physical}");
    let mut child: Child = pipewire::spawn_loopback(&physical)?;
    let node_id = match pipewire::wait_for_node(SMR_NODE_NAME, 30) {
        Ok(id) => id,
        Err(e) => {
            let _ = child.kill();
            return Err(e);
        }
    };

    // 2. Set default source.
    if config.set_default {
        match pipewire::set_default_source(SMR_NODE_NAME) {
            Ok(()) => println!("smr: default source → {SMR_DESCRIPTION}"),
            Err(e) => eprintln!("smr: could not set default source: {e}"),
        }
    }

    let daemon = Arc::new(Daemon {
        node_id: AtomicU32::new(node_id),
        muted: AtomicBool::new(false),
        physical: physical.clone(),
        keycode: config.ptt_keycode,
    });

    // 3. Control socket.
    ipc::serve(listener, daemon.clone());

    // 4. PTT listeners.
    match config.ptt_keycode {
        Some(code) => {
            let d = daemon.clone();
            input::spawn_listeners(code, config.ptt_device.clone(), move |pressed| {
                if let Err(e) = d.set_mute(pressed) {
                    eprintln!("smr: mute toggle failed: {e}");
                }
            })?;
            println!("smr: push-to-talk armed on keycode {code}");
        }
        None => eprintln!("smr: no PTT key set — routing only (run `smr set-key`)"),
    }

    println!("smr: ready. {}", daemon.status_line());

    // 5. Block until a termination signal, then clean up.
    let (tx, rx) = mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })?;
    let _ = rx.recv();

    println!("\nsmr: shutting down…");
    cleanup(&mut child, &config);
    Ok(())
}

fn cleanup(child: &mut Child, config: &Config) {
    // Restore the previous default source, best effort.
    if let Some(prev) = &config.previous_default {
        match pipewire::set_default_source(prev) {
            Ok(()) => println!("smr: default source restored → {prev}"),
            Err(e) => eprintln!("smr: could not restore default source: {e}"),
        }
    }
    // Tear down the virtual source.
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(ipc::socket_path());
    println!("smr: virtual source removed. bye.");
}

/// `smr restore` — reset the default source to the recorded prior device,
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
