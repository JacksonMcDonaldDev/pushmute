#!/usr/bin/env bash
#
# PushMute installer — installs the daemon, systemd user unit, launcher entry, and
# icon into the per-user XDG locations. By default it downloads the prebuilt static
# binary from the latest GitHub release, so no Rust toolchain is needed and the
# script can be piped straight from the web:
#
#   curl -sSfL https://raw.githubusercontent.com/JacksonMcDonaldDev/pushmute/main/install.sh | bash
#
# Run from a cloned checkout it behaves the same, but `--build` then compiles from
# source instead of downloading. The same script uninstalls (`--uninstall`), reusing
# the path constants below so install and uninstall can never drift.
#
#   ./install.sh                  install (download → files → start → doctor)
#   ./install.sh --build          build from source instead of downloading (needs a clone + Rust)
#   ./install.sh --uninstall      remove files, keep ~/.config/pushmute
#   ./install.sh --uninstall --purge   also remove ~/.config/pushmute
#
set -euo pipefail

# --- Where things come from -----------------------------------------------------
REPO="JacksonMcDonaldDev/pushmute"
BINARY_ASSET="pushmute-x86_64-linux"
BINARY_URL="https://github.com/$REPO/releases/latest/download/$BINARY_ASSET"
RAW_BASE="https://raw.githubusercontent.com/$REPO/main"
ICON_URL="$RAW_BASE/assets/pushmute.svg"

# Locate our own directory when run from a checkout; empty when piped via curl. The
# repo copies of the unit/desktop/icon are then preferred over the embedded ones so
# a developer's edits stay authoritative during local installs.
SOURCE="${BASH_SOURCE[0]:-}"
if [ -n "$SOURCE" ] && [ -f "$SOURCE" ]; then
	SCRIPT_DIR="$(cd "$(dirname "$SOURCE")" && pwd)"
else
	SCRIPT_DIR=""
fi
IN_CLONE=false
[ -n "$SCRIPT_DIR" ] && [ -f "$SCRIPT_DIR/Cargo.toml" ] && IN_CLONE=true

# --- Install locations (the single source of truth for both directions) --------
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"

BIN="$HOME/.local/bin/pushmute"
UNIT="$CONFIG_HOME/systemd/user/pushmute.service"
ICON="$DATA_HOME/icons/hicolor/scalable/apps/pushmute.svg"
DESKTOP_LAUNCHER="$DATA_HOME/applications/pushmute.desktop"
CONFIG_DIR="$CONFIG_HOME/pushmute"
SOCKET="$RUNTIME_DIR/pushmute.sock"

say()  { printf 'pushmute: %s\n' "$1"; }
warn() { printf 'pushmute: warning: %s\n' "$1" >&2; }
die()  { printf 'pushmute: error: %s\n' "$1" >&2; exit 1; }

usage() {
	cat <<'EOF'
Usage:
  ./install.sh                       Install PushMute (downloads the prebuilt binary).
  ./install.sh --build               Build from source instead (needs a clone + Rust).
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

# --- Acquire the binary ---------------------------------------------------------
download_binary() {
	command -v curl >/dev/null 2>&1 || die "curl is required to download the prebuilt binary."
	local arch
	arch="$(uname -m)"
	[ "$arch" = "x86_64" ] || die "no prebuilt binary for '$arch' — clone the repo and run './install.sh --build'."

	local tmp
	tmp="$(mktemp)"
	say "downloading prebuilt binary ($BINARY_ASSET)…"
	if ! curl -sSfL "$BINARY_URL" -o "$tmp"; then
		rm -f "$tmp"
		die "download failed — check your connection or build from source ('--build')."
	fi
	install -Dm755 "$tmp" "$BIN"
	rm -f "$tmp"
	say "installed binary → $BIN"
}

build_binary() {
	$IN_CLONE || die "'--build' must be run from a cloned checkout (no Cargo.toml found)."
	command -v cargo >/dev/null 2>&1 || die "cargo not found — install a Rust toolchain or drop '--build' to download the prebuilt binary."
	say "building (cargo build --release)…"
	cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"
	install -Dm755 "$SCRIPT_DIR/target/release/pushmute" "$BIN"
	say "installed binary → $BIN"
}

# --- Supporting files -----------------------------------------------------------
# The unit and launcher are embedded so a curl-piped install needs no checkout; when
# run from a clone the repo copies are used instead. Keep these in sync with the
# files at the repo root and packaging/ — the clone path is what gets exercised in dev.
write_unit() {
	if $IN_CLONE && [ -f "$SCRIPT_DIR/pushmute.service" ]; then
		install -Dm644 "$SCRIPT_DIR/pushmute.service" "$UNIT"
	else
		mkdir -p "$(dirname "$UNIT")"
		cat >"$UNIT" <<'EOF'
[Unit]
Description=PushMute (selective mic router)
# Order after the audio stack and the graphical session so the tray (SNI) has a
# StatusNotifierWatcher to register against on cold boot.
After=pipewire.service wireplumber.service graphical-session.target
Wants=pipewire.service
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=%h/.local/bin/pushmute run
# Best-effort restore of the default source if the daemon is stopped/killed.
ExecStopPost=%h/.local/bin/pushmute restore
Restart=on-failure
RestartSec=2

[Install]
WantedBy=graphical-session.target
EOF
	fi
}

write_desktop() {
	if $IN_CLONE && [ -f "$SCRIPT_DIR/packaging/pushmute.desktop" ]; then
		install -Dm644 "$SCRIPT_DIR/packaging/pushmute.desktop" "$DESKTOP_LAUNCHER"
	else
		mkdir -p "$(dirname "$DESKTOP_LAUNCHER")"
		cat >"$DESKTOP_LAUNCHER" <<'EOF'
[Desktop Entry]
Type=Application
Name=PushMute
GenericName=Selective Mic Router
Comment=Talk to one app without being heard by the rest
Exec=pushmute run
Icon=pushmute
Terminal=false
# Clicking this launcher while the daemon is already running (e.g. started by the
# systemd user unit) is safe — the single-instance guard makes the second
# invocation exit cleanly.
StartupNotify=false
Categories=Audio;AudioVideo;Utility;
Keywords=microphone;mic;mute;push-to-talk;pipewire;
EOF
	fi
}

write_icon() {
	# Launcher icon only — the tray uses stock theme icon names regardless, so a
	# missing icon here is cosmetic. Fetch it when there's no local copy.
	if $IN_CLONE && [ -f "$SCRIPT_DIR/assets/pushmute.svg" ]; then
		install -Dm644 "$SCRIPT_DIR/assets/pushmute.svg" "$ICON"
		return
	fi
	mkdir -p "$(dirname "$ICON")"
	if command -v curl >/dev/null 2>&1 && curl -sSfL "$ICON_URL" -o "$ICON.tmp" 2>/dev/null; then
		mv "$ICON.tmp" "$ICON"
		chmod 644 "$ICON"
	else
		rm -f "$ICON.tmp"
		warn "could not fetch the launcher icon — the tray icon is unaffected."
	fi
}

install_pushmute() {
	local mode="$1"

	if [ "$mode" = "build" ]; then
		build_binary
	else
		download_binary
	fi

	write_unit
	write_desktop
	write_icon
	say "installed unit, launcher entry, and icon"

	systemctl --user daemon-reload
	# Start now so the tray is available immediately, but do NOT enable on login —
	# run-on-startup is opt-in via the tray's "Run on startup" checkbox (off by
	# default). A fresh install with no mic configured yet will fail to *start* —
	# that's expected, so tolerate it and tell the user.
	if systemctl --user start pushmute 2>/dev/null; then
		say "started the user service (run-on-startup is off — enable it from the tray)"
	else
		say "not running yet — pick a mic from the tray, then: systemctl --user start pushmute"
	fi

	check_input_group

	say "running environment check…"
	"$BIN" doctor || true

	say "done. Click the PushMute tray icon to pick your mic and hotkey."
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

	rm -f "$BIN" "$ICON" "$DESKTOP_LAUNCHER" "$SOCKET"
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
	local action="install" purge="keep" mode="download"
	for arg in "$@"; do
		case "$arg" in
			--build)     mode="build" ;;
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
		install_pushmute "$mode"
	fi
}

main "$@"
