//! Control socket so the `pushmute` CLI can drive a running daemon.
//!
//! Line protocol over a Unix socket at `$XDG_RUNTIME_DIR/pushmute.sock`: the client
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
    PathBuf::from(dir).join("pushmute.sock")
}

/// Outcome of trying to claim the control socket. `AlreadyRunning` is a *success*
/// (the single-instance guard did its job), not an error — the caller exits 0 — so
/// it travels its own variant rather than an overloaded `Err`.
pub enum Bind {
    Listener(UnixListener),
    AlreadyRunning,
}

/// Try to claim the control socket. Returns `AlreadyRunning` if another daemon owns
/// it; removes a stale socket left by a crashed daemon and binds otherwise.
///
/// Two paths report `AlreadyRunning`: a live daemon answering the connect-probe, and
/// an `AddrInUse` bind error. The latter closes the autostart TOCTOU race — two
/// launches can both pass the connect-probe, then one loses the `bind()`; that loser
/// is a duplicate, which is exactly what the guard exists to catch. Any *other* bind
/// error (permissions, an undeletable stale socket) stays a real `Err`.
pub fn bind() -> Result<Bind> {
    bind_at(&socket_path())
}

fn bind_at(path: &std::path::Path) -> Result<Bind> {
    if path.exists() {
        // If something answers, another daemon is live.
        if UnixStream::connect(path).is_ok() {
            return Ok(Bind::AlreadyRunning);
        }
        let _ = std::fs::remove_file(path);
    }
    match UnixListener::bind(path) {
        Ok(listener) => Ok(Bind::Listener(listener)),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => Ok(Bind::AlreadyRunning),
        Err(e) => Err(e).with_context(|| format!("binding {}", path.display())),
    }
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

#[cfg(test)]
mod bind_tests {
    use super::*;

    /// A unique throwaway socket path under the OS temp dir, so the test doesn't
    /// touch the real `$XDG_RUNTIME_DIR/pushmute.sock` or race other tests.
    fn temp_sock() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("pushmute-test-{}-{n}.sock", std::process::id()))
    }

    #[test]
    fn first_bind_listens_second_sees_already_running() {
        let path = temp_sock();
        let first = bind_at(&path).expect("first bind");
        assert!(
            matches!(first, Bind::Listener(_)),
            "first bind should listen"
        );

        // A live listener answers the connect-probe → AlreadyRunning.
        match bind_at(&path).expect("second bind") {
            Bind::AlreadyRunning => {}
            Bind::Listener(_) => panic!("second bind should see the live daemon"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stale_socket_is_reclaimed() {
        let path = temp_sock();
        // A socket file with nothing listening (crashed daemon) is removed + rebound.
        let listener = UnixListener::bind(&path).unwrap();
        drop(listener); // leaves the file on disk, nothing listening
        assert!(path.exists());
        match bind_at(&path).expect("rebind over stale socket") {
            Bind::Listener(_) => {}
            Bind::AlreadyRunning => panic!("stale socket should be reclaimed, not refused"),
        }
        let _ = std::fs::remove_file(&path);
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
