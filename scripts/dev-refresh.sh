#!/usr/bin/env bash
#
# Rebuild and relaunch the smr dev daemon, guaranteeing no stale instance
# survives to trip the "another smr daemon is already running" startup guard
# (see src/ipc.rs::bind). Idempotent: safe whether or not a daemon — systemd or
# dev — is currently up.
#
# Run it backgrounded so the caller can read startup logs:
#   scripts/dev-refresh.sh
#
set -euo pipefail

sock="${XDG_RUNTIME_DIR:-/tmp}/smr.sock"

# 1. Build FIRST. If it fails we exit here, loudly, with the existing daemon
#    still serving the mic — a broken edit never leaves you with no input.
cargo build

# 2. Hand control away from the installed systemd instance, if it's running.
systemctl --user stop smr 2>/dev/null || true

# 3. Stop any running dev daemon. SIGTERM (not -9) lets it restore the default
#    source, kill its pw-loopback child, and remove the socket. `smr run`
#    re-execs as target/{debug,release}/smr, so match the binary, not "cargo".
#    The [s] keeps the pattern from matching this script's own command line.
pkill -TERM -f '[s]mr run' 2>/dev/null || true

# 4. Wait for the old daemon to actually exit before relaunching. ipc::bind
#    treats a socket that still answers as a live daemon, so racing ahead of
#    teardown is exactly the failure this script exists to prevent. The process
#    going away is the liveness signal; once it's gone, clear any socket its
#    cleanup didn't remove (e.g. after a crash).
for _ in $(seq 1 50); do
    pgrep -f '[s]mr run' >/dev/null || break
    sleep 0.1
done
rm -f "$sock"

# 5. Launch the fresh binary. exec so the daemon inherits this PID and the
#    caller's background job tracks the daemon directly.
exec cargo run -- run
