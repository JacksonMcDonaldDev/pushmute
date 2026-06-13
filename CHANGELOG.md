# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

### Added
- Selective mic routing for PipeWire: route your microphone to a single
  application and silence it everywhere else with a global push-to-mute hotkey.
- System-tray (StatusNotifierItem) indicator with live mute state.
- `pushmute` CLI subcommands: `run`, `devices`, `set-mic`, `set-key`,
  `restore`, and `doctor`.
- `pushmute doctor` environment preflight (PipeWire/WirePlumber tooling, graph
  parseability, `input` group, tray watcher); critical checks also auto-run at
  `pushmute run` startup.
- systemd user unit ordered after `graphical-session.target`, restoring the
  original default source on stop.
- Single-instance guard: safe to launch from a desktop launcher, the systemd
  user unit, or a terminal without spawning duplicate daemons.
- Packaging: AUR `PKGBUILD` + `.SRCINFO`, a `.desktop` launcher, and a scalable
  hicolor icon.
- `install.sh` for non-Arch users: downloads the prebuilt static binary by
  default (no toolchain) and can be piped straight from the web
  (`curl -sSfL .../install.sh | bash`); `--build` compiles from a source
  checkout instead.

<!-- next-url -->
[Unreleased]: https://github.com/JacksonMcDonaldDev/pushmute/compare/v0.1.0...HEAD
