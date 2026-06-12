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
- Single-instance guard: safe to launch from a desktop launcher, XDG autostart,
  or a terminal without spawning duplicate daemons.
- Packaging: AUR `PKGBUILD` + `.SRCINFO`, an `install.sh` for non-Arch users,
  a `.desktop` launcher plus XDG autostart fallback, and a scalable hicolor icon.

<!-- next-url -->
[Unreleased]: https://github.com/JacksonMcDonaldDev/pushmute/compare/v0.1.0...HEAD
