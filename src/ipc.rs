//! Control socket so the `smr` CLI can drive a running daemon.
//!
//! Line protocol over a Unix socket at `$XDG_RUNTIME_DIR/smr.sock`: the client
//! writes one command line, the daemon writes one response line. The socket also
//! serves as the single-instance guard.

use crate::daemon::Daemon;
use anyhow::{anyhow, Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

pub fn socket_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(dir).join("smr.sock")
}

/// Bind the control socket, failing if another daemon already owns it. Removes a
/// stale socket left by a crashed daemon.
pub fn bind() -> Result<UnixListener> {
    let path = socket_path();
    if path.exists() {
        // If something answers, another daemon is live.
        if UnixStream::connect(&path).is_ok() {
            return Err(anyhow!("another smr daemon is already running"));
        }
        let _ = std::fs::remove_file(&path);
    }
    UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))
}

/// Serve control commands on `listener` until the process exits.
pub fn serve(listener: UnixListener, daemon: Arc<Daemon>) {
    thread::spawn(move || {
        for stream in listener.incoming() {
            let stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };
            let daemon = daemon.clone();
            thread::spawn(move || handle(stream, daemon));
        }
    });
}

fn handle(stream: UnixStream, daemon: Arc<Daemon>) {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let reply = dispatch(line.trim(), &daemon);
    let mut w = &stream;
    let _ = writeln!(w, "{reply}");
}

fn dispatch(cmd: &str, daemon: &Daemon) -> String {
    match cmd {
        "status" => daemon.status_line(),
        "mute" => result(daemon.set_mute(true)),
        "unmute" => result(daemon.set_mute(false)),
        "toggle" => result(daemon.toggle()),
        other => format!("error: unknown command `{other}`"),
    }
}

fn result(r: Result<()>) -> String {
    match r {
        Ok(()) => "ok".into(),
        Err(e) => format!("error: {e}"),
    }
}

/// Client side: send one command to the daemon and return its reply.
pub fn send(cmd: &str) -> Result<String> {
    let path = socket_path();
    let stream = UnixStream::connect(&path)
        .map_err(|_| anyhow!("daemon not running (no socket at {})", path.display()))?;
    let mut w = &stream;
    writeln!(w, "{cmd}")?;
    let mut reader = BufReader::new(&stream);
    let mut resp = String::new();
    reader.read_line(&mut resp)?;
    Ok(resp.trim().to_string())
}
