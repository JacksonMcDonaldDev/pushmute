#!/usr/bin/env bash
#
# PushMute installer — builds and installs the daemon, systemd user unit, launcher
# entry, and icon into the per-user XDG locations. The same
# script uninstalls (`--uninstall`), reusing the path constants below so install and
# uninstall can never drift.
#
#   ./install.sh                  install (build → files → enable → doctor)
#   ./install.sh --uninstall      remove files, keep ~/.config/pushmute
#   ./install.sh --uninstall --purge   also remove ~/.config/pushmute
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# --- Install locations (the single source of truth for both directions) --------
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"

BIN="$HOME/.local/bin/pushmute"
UNIT="$CONFIG_HOME/systemd/user/pushmute.service"
ICON="$DATA_HOME/icons/hicolor/scalable/apps/pushmute.svg"
DESKTOP_LAUNCHER="$DATA_HOME/applications/pushmute.desktop"
DESKTOP_AUTOSTART="$CONFIG_HOME/autostart/pushmute.desktop"
CONFIG_DIR="$CONFIG_HOME/pushmute"
SOCKET="$RUNTIME_DIR/pushmute.sock"

say()  { printf 'pushmute: %s\n' "$1"; }
warn() { printf 'pushmute: warning: %s\n' "$1" >&2; }

usage() {
	cat <<'EOF'
Usage:
  ./install.sh                       Build and install PushMute for the current user.
  ./install.sh --uninstall           Remove PushMute (keeps ~/.config/pushmute).
  ./install.sh --uninstall --purge   Remove PushMute and its config directory.
EOF
}

check_input_group() {
	# Not in the `input` group → evdev hotkey capture won't work. Warn and point at
	# the fix, but never abort and never modify group membership (other tools may
	# rely on it, and adding the user needs a re-login to take effect anyway).
	if id -nG 2>/dev/null | tr ' ' '\n' | grep -qx input; then
		return 0
	fi
	warn "you are not in the 'input' group — the hotkey needs it."
	say  "  fix: sudo usermod -aG input \"\$USER\"   (then log out and back in)"
}

install_pushmute() {
	say "building (cargo build --release)…"
	cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"

	install -Dm755 "$SCRIPT_DIR/target/release/pushmute" "$BIN"
	say "installed binary → $BIN"

	# The unit ships with ExecStart=%h/.local/bin/pushmute run; %h expands to $HOME
	# at runtime, so it is installed verbatim (the AUR route seds it to /usr/bin).
	install -Dm644 "$SCRIPT_DIR/pushmute.service" "$UNIT"
	install -Dm644 "$SCRIPT_DIR/assets/pushmute.svg" "$ICON"
	install -Dm644 "$SCRIPT_DIR/packaging/pushmute.desktop" "$DESKTOP_LAUNCHER"
	say "installed unit, icon, and launcher entry"

	systemctl --user daemon-reload
	# Start now so the tray is available immediately, but do NOT enable on login —
	# run-on-startup is opt-in via the tray's "Run on startup" checkbox (off by
	# default). A fresh install with no mic configured yet will fail to *start* —
	# that's expected, so tolerate it and tell the user.
	if systemctl --user start pushmute 2>/dev/null; then
		say "started the user service (run-on-startup is off — enable it from the tray)"
	else
		say "not running yet — set a mic, then: systemctl --user start pushmute"
	fi

	check_input_group

	say "running environment check…"
	"$BIN" doctor || true

	say "done. Next: 'pushmute set-mic <name>' (see 'pushmute devices') and 'pushmute set-key'."
}

uninstall_pushmute() {
	local purge="$1"

	# Disable+stop first: this fires the unit's ExecStopPost=pushmute restore, which
	# resets the default source to what it was before PushMute, while the binary is
	# still present to do it.
	if systemctl --user disable --now pushmute 2>/dev/null; then
		say "disabled the user service (default source restored via ExecStopPost)"
	else
		warn "could not disable the service (was it installed?) — continuing"
	fi

	# $DESKTOP_AUTOSTART is no longer installed, but rm it anyway to clean up the
	# entry that older versions used to drop.
	rm -f "$BIN" "$ICON" "$DESKTOP_LAUNCHER" "$DESKTOP_AUTOSTART" "$SOCKET"
	if [ -f "$UNIT" ]; then
		rm -f "$UNIT"
		systemctl --user daemon-reload
	fi
	say "removed binary, unit, icon, launcher entry, and stale socket"

	if [ "$purge" = "purge" ]; then
		rm -rf "$CONFIG_DIR"
		say "purged config directory → $CONFIG_DIR"
	else
		say "left config directory in place → $CONFIG_DIR (use --purge to remove)"
	fi

	# Group membership is the user's to manage — install only ever warned about it.
	say "left 'input' group membership unchanged."
}

main() {
	local action="install" purge="keep"
	for arg in "$@"; do
		case "$arg" in
			--uninstall) action="uninstall" ;;
			--purge)     purge="purge" ;;
			-h|--help)   usage; exit 0 ;;
			*) warn "unknown argument: $arg"; usage; exit 2 ;;
		esac
	done

	if [ "$action" = "uninstall" ]; then
		uninstall_pushmute "$purge"
	else
		if [ "$purge" = "purge" ]; then
			warn "--purge only applies to --uninstall; ignoring."
		fi
		install_pushmute
	fi
}

main "$@"
