# Deployment & Distribution Plan (`pushmute`)

Status: **in progress** — decisions locked via grill session. Rename sweep +
GitHub repo rename done; packaging + CI/CD remain.
Branch: `deployment-pipeline`.

This is the packaging + CI/CD effort that takes `pushmute` from a personal
build to something other users can install. Pick up from the runbook + build
checklist at the bottom.

## App profile (why these choices)

`pushmute` is a niche Linux desktop daemon:

- Rust binary; entire OS-facing surface is **four external binaries** it shells out
  to: `pw-dump` (JSON parsed), `wpctl inspect/set-default/set-mute` (text scraped),
  and `pw-loopback`. See `src/pipewire.rs`.
- Hard runtime deps: **PipeWire + WirePlumber**.
- Needs `/dev/input` access via the **`input` group** (evdev push-to-talk).
- Ships a **systemd user unit** and paints a **StatusNotifierItem tray** (`ksni`).
- Ships a **`.desktop` file** for application launcher discoverability.
- **Single-instance guard:** if `pushmute run` detects a daemon already running
  (via IPC socket), it prints `pushmute: already running` to stderr and exits 0.
  Makes every launch path (launcher click, autostart race, terminal) safe.

Audience skews **Arch / Hyprland / waybar** power users; secondary audience is
KDE, Sway, and Ubuntu/Pop!_OS users.

## Desktop integration shape

Three layers work together to make `pushmute` launchable and autostarting across
desktop environments:

1. **Binary is self-protecting** — single-instance guard via IPC socket check.
   Safe to launch from any mechanism.
2. **Autostart is layered:**
   - Systemd user unit — primary path on all install routes. Restart-on-failure,
     journald logging, proper session ordering.
   - XDG autostart (`~/.config/autostart/pushmute.desktop`) — fallback for DEs
     that don't activate `graphical-session.target` or don't use systemd sessions.
     Installed by `install.sh` only (not AUR — user-specific path).
3. **Launcher entry is dumb** — `Exec=pushmute run`. Works regardless of install
   method or which autostart mechanism is active. Single-instance guard handles
   the rest.

## Arch vs. Ubuntu/Pop reality check

- **Identical on both:** `input` group + evdev udev rules; systemd user units; the
  four PipeWire binaries exist (Arch: `pipewire`+`wireplumber`; Ubuntu:
  `pipewire-bin`+`wireplumber` — different package names, same binaries).
- **Real divergence (worst first):**
  1. **PipeWire/WirePlumber version skew — the actual risk.** Dev box (Pop, Ubuntu
     base) *freezes* PipeWire old; Arch is rolling and *newest*. The fragile surface
     is the `pw-dump` JSON shape + `wpctl inspect` text scraping, which has drifted
     across releases. Arch users run newer PipeWire than was developed against.
  2. **glibc skew — only bites prebuilt binaries.** AUR builds from source on the
     user's machine, so it's immune. Only a GitHub-Releases binary would need an
     old-glibc build container or musl.
  3. **Tray visibility is a desktop, not distro, concern.** Native in waybar
     (Arch/Hyprland); stock GNOME (Pop) needs the AppIndicator extension. Ubuntu
     ships `ubuntu-appindicator@ubuntu.com` pre-installed; Pop!_OS may not. The
     target audience actually has the *better* tray story.

## Cross-DE compatibility

| Desktop | Audio stack | Tray | Verdict |
|---|---|---|---|
| Arch + Hyprland (uwsm) | PipeWire ✓ | waybar SNI ✓ | Primary target |
| Arch + KDE Plasma | PipeWire ✓ | Native SNI ✓ | Works out of the box |
| Arch + Sway | PipeWire ✓ | waybar SNI ✓ | Same as Hyprland |
| Ubuntu 22.04+ / Pop!_OS | PipeWire ✓ | Extension may be needed | Works, some friction |
| Ubuntu 24.04 | PipeWire ✓ | Extension may be needed | Works, newer PipeWire |

Note: Hyprland users not using `uwsm` may not have `graphical-session.target`
activated. README should document `uwsm` as the recommended session manager for
correct systemd unit ordering.

## Locked decisions

- **Name:** `pushmute` — binary + AUR package. Verified free on AUR (2026-06-11).
  Virtual mic shows as **"PushMute"** (node `pushmute`). Clean pre-1.0 break,
  **no migration shim**.
- **Install spine:** **source-built everywhere** (chosen over prebuilt-first). Two
  routes:
  - AUR `makepkg` from the release tarball (primary — Arch audience).
  - A local-compile install script for non-Arch (build → `~/.local/bin` → systemd
    unit → `input`-group check → run `doctor`).
  - Prebuilt binaries deferred to a later "don't want to compile" convenience
    (build in old-glibc container when added).
  - **crates.io / `cargo install` dropped (2026-06-12):** no install path uses it,
    `cargo install` is binary-only (no unit/desktop/icon — a degraded experience for
    a tray daemon), name-squat risk on a niche name is negligible, and reclaiming
    later is trivial. No `cargo binstall` planned. NB: `cargo` stays the **build
    tool** on every route, and dependency crates still come from crates.io — only
    *publishing pushmute itself to crates.io* is removed.
- **Branding — "PushMute" is the proper name; "selective mic router" is the
  subtitle.** Today `pushmute --help` (`src/main.rs:16`) and `systemctl --user
  status` (`pushmute.service:2`) both surface the old "Selective Mic Router" while
  everything else says PushMute. Unify: clap `about = "PushMute — selective mic
  router"`; unit `Description=PushMute (selective mic router)`; README `# PushMute`
  with the old phrase as the tagline beneath; also update the `src/main.rs:1` module
  doc-comment. Keeps the descriptive phrase without leaving the old name in tooling
  output.
- **Desktop integration shape:** see section above. Single-instance guard +
  layered autostart (systemd primary, XDG fallback) + dumb launcher `Exec=`.
- **Single-instance guard (already exists — reshape, don't reimplement):**
  `ipc::bind()` (`src/ipc.rs:22-32`) already probes the socket, but on a live daemon
  it returns a generic `Err` that `main` (`src/main.rs:55-58`) prints as
  `pushmute: error: …` and exits **1**. Spec wants `pushmute: already running` on
  stderr and exit **0**. Reshape:
  - `bind()` returns an outcome enum (`Listener(UnixListener)` | `AlreadyRunning`),
    not an overloaded `Err` — exit-0 is a *success*, so it should not travel the
    error path. `daemon::run` matches: `AlreadyRunning` → print the spec message →
    `return Ok(())`; `Listener` → continue.
  - **Close the autostart race (TOCTOU):** the connect-probe can let two
    simultaneous launches both pass, then one loses `bind()` with `AddrInUse`. Map
    an `AddrInUse` bind error to `AlreadyRunning` too (the race is the whole point
    of the guard). Any *other* bind error (permission, undeletable stale socket)
    stays a real `Err` → exit 1.
- **`.desktop` file:** `packaging/pushmute.desktop`. `Exec=pushmute run`,
  `Icon=pushmute`, `StartupNotify=false`. Install paths:
  - AUR → `/usr/share/applications/pushmute.desktop` (system-wide, launcher only).
  - `install.sh` → two copies: `~/.local/share/applications/pushmute.desktop`
    (launcher) AND `~/.config/autostart/pushmute.desktop` (XDG autostart fallback).
- **Icon:** `assets/pushmute.svg` in repo. Installed to XDG hicolor scalable path
  (`/usr/share/icons/hicolor/scalable/apps/pushmute.svg` for AUR;
  `~/.local/share/icons/hicolor/scalable/apps/pushmute.svg` for `install.sh`).
- **Systemd unit — `graphical-session.target`:** unit must declare
  `After=graphical-session.target`, `PartOf=graphical-session.target`, and
  `WantedBy=graphical-session.target`. Without this the tray silently fails to
  register on cold boot before the Wayland compositor is ready.
- **Systemd unit — `ExecStart` path (one file, PKGBUILD seds it):** the repo ships a
  single canonical `pushmute.service` with `ExecStart=%h/.local/bin/pushmute run`.
  `install.sh` installs it **verbatim** (the `%h` specifier expands to the user's
  home). The AUR `PKGBUILD` installs the binary to `/usr/bin` and **seds** the unit
  at package time:
  `sed -i 's|%h/.local/bin/pushmute|/usr/bin/pushmute|' "$pkgdir/usr/lib/systemd/user/pushmute.service"`.
  One source of truth, one mechanical transform on the diverging (AUR) route only.
  Rejected: two hand-maintained unit files (drift fails silently); PATH-based
  `ExecStart=pushmute run` (systemd *user* manager PATH does not reliably include
  `~/.local/bin`, so install.sh users would silently fail to launch).
- **AUR post-install:** installs files only; prints post-install message:
  `"To start pushmute and enable it on login: systemctl --user enable --now pushmute"`.
  Does not auto-enable (Arch convention).
- **`install.sh` auto-enable:** runs `systemctl --user enable --now pushmute`
  automatically after installing.
- **`install.sh` input group handling:** if user is not in `input` group, print
  warning + `sudo usermod -aG input $USER` command + continue (do not abort).
  Doctor will surface this at every startup until resolved.
- **`install.sh --uninstall [--purge]`** — a flag on the *same* script (reuses the
  install path constants, so it can't drift). Reverses install in order, leading
  with `systemctl --user disable --now pushmute` — which fires the unit's
  `ExecStopPost=pushmute restore` (`pushmute.service:9`) and so **restores the
  user's original default source** before the binary disappears. Then removes
  binary, unit (`daemon-reload`), both `.desktop` copies, icon, and a stale
  `$XDG_RUNTIME_DIR/pushmute.sock` if present. **Leaves `~/.config/pushmute/` by
  default** (`apt remove` semantics); `--purge` also removes the config dir.
  **Does not touch `input` group membership** (install only warned, never added it;
  other tools may rely on it) — prints a note that it was left in place.
- **AUR uninstall is pacman's job.** `pacman -Rns pushmute` removes packaged files.
  No `pre_remove` `systemctl --user disable` hook (a system package can't cleanly
  reach every user session — fragile). Instead the post-install message also notes:
  run `systemctl --user disable --now pushmute` before removal.
- **Robustness (in scope for this branch):**
  - `pushmute doctor` preflight subcommand — checks environment, reports pass/fail.
    Also auto-runs critical checks at `pushmute run` startup.
  - **Doctor severity split:**
    - *Critical (abort `run`):* `pw-dump`/`wpctl`/`pw-loopback` not in PATH;
      `pw-dump` JSON unparseable; `wpctl inspect` output unparseable; user not in
      `input` group.
    - *Warning (proceed with notice):* `StatusNotifierWatcher` absent on D-Bus
      (tray won't appear; audio routing still works). Shown at **every startup**
      if missing, not just via `pushmute doctor`.
  - GNOME doctor message: if `XDG_CURRENT_DESKTOP=GNOME` and SNI watcher absent,
    point to the AppIndicator extension.
  - Pull the scrapers in `src/pipewire.rs` (`list_capture_devices`, `node_id_by_name`,
    `current_default_source`) into pure functions; pin them with **fixtures captured
    from old (Pop) + new (Arch-era) PipeWire** and unit-test in CI.
  - **Not** doing live-PipeWire-in-CI (headless runners make it flaky for little gain
    over fixtures).
- **Pipeline:**
  - **CI (every PR/push to main):** `cargo fmt --check` → `cargo clippy -D warnings`
    → `cargo build` → `cargo test` (incl. fixture tests).
  - **`lint-packaging` job (parallel, Ubuntu, no Rust toolchain):**
    `desktop-file-validate packaging/pushmute.desktop` (catches silent `.desktop`
    errors) + `shellcheck install.sh` (guards the install/uninstall/purge shell).
    Both are one-`apt`-line, millisecond gates. **No `.SRCINFO` CI gate:** it needs
    Arch's `makepkg`, and the AUR deploy action regenerates `.SRCINFO` from the
    PKGBUILD at publish time, so the committed copy is non-authoritative anyway.
  - **Release cut:** `cargo-release` (`cargo release minor`) bumps `Cargo.toml`,
    updates `CHANGELOG.md`, commits, tags `vX.Y.Z`, pushes — atomically.
  - **CD (on `v*` tag):** one **`gate`** job (fmt/clippy/build/test) feeds two
    **parallel, idempotent** publish jobs, both `needs: [gate]`:
    - **GitHub Release** — `softprops/action-gh-release` (updates rather than
      erroring if the release already exists); auto source tarball from the tag.
    - **AUR** — `KSXGNU/github-actions-deploy-aur` (no-ops when nothing changed).
    The two are independent (AUR builds from the GitHub tarball, not from the
    Release object), so order doesn't matter. Recovery is GitHub's **re-run failed
    jobs**: a flaky AUR push re-runs alone without re-firing an already-succeeded
    Release. No crates.io publish (see Install spine), so the irreversible-publish
    failure mode is gone.
- **AUR:** single **`pushmute`** package built **from the GitHub release tarball**
  (the canonical release artifact). `depends=(pipewire wireplumber)`,
  `makedepends=(cargo)`, installs binary + unit + `.desktop` + icon + LICENSE.
  Auto-published via `KSXGNU/github-actions-deploy-aur` on tag. `pushmute-git`
  **deferred**.

## Manual runbook (only the user can do these — accounts/keys/secrets)

Plant the AUR flag + wire secrets so automation can take over.

- [ ] **Step 1 — AUR:** register at aur.archlinux.org (note username + email).
  Generate a CI-only key: `ssh-keygen -t ed25519 -C "aur-pushmute-ci" -f ~/.ssh/aur_pushmute -N ""`.
  Add the **public** key to AUR → My Account. Add the **private** key as GitHub repo
  secret **`AUR_SSH_PRIVATE_KEY`**. (Package name gets claimed by the first automated
  push.)
- [ ] **Step 2 — GitHub Actions perms:** Settings → Actions → General → Workflow
  permissions → **Read and write** (lets the release workflow create Releases).
- [ ] **Step 3 — Local tool:** `cargo install cargo-release`.

Values I'll need referenced in the workflow files: AUR username + email.

## Build checklist (Claude — code side, once runbook flags are planted)

- [ ] Update `pushmute.service` — add `graphical-session.target` ordering
      (`After=`, `PartOf=`, `WantedBy=`). Keep canonical `ExecStart=%h/.local/bin/pushmute run`
      (install.sh uses verbatim; PKGBUILD seds to `/usr/bin/pushmute run`). Note: current
      file still has `WantedBy=default.target` + `After=pipewire/wireplumber` — replace/extend
      `[Install]` with graphical-session ordering rather than leaving both.
- [ ] Single-instance guard — **reshape existing `ipc::bind()`** (already probes the
      socket): return `Listener | AlreadyRunning` enum instead of a generic `Err`;
      `daemon::run` prints `pushmute: already running` to stderr + exits 0 on
      `AlreadyRunning`; map `AddrInUse` bind error to `AlreadyRunning` (autostart
      race); other bind errors stay exit 1.
- [ ] `pushmute doctor` subcommand + auto-run critical checks at `run` startup;
      SNI watcher warning shown at every startup if missing.
      (Dev-box baseline 2026-06-11: `pw-dump`, `wpctl`, `pw-loopback` all present in
      `/usr/bin`; Rust 1.96. Critical failure paths need simulating via doctored PATH.)
- [ ] Extract `src/pipewire.rs` scrapers into pure functions; add old+new PipeWire
      fixtures + unit tests.
- [ ] `.github/workflows/ci.yml` (fmt/clippy/build/test on PR + main) + parallel
      `lint-packaging` job: `desktop-file-validate packaging/pushmute.desktop` +
      `shellcheck install.sh`.
- [ ] `.github/workflows/release.yml` (on `v*`: one `gate` job → two parallel
      idempotent jobs `needs: [gate]` — GitHub Release + AUR deploy action).
- [ ] `cargo-release` config (`Cargo.toml` `[workspace.metadata.release]` or
      `release.toml`) + `CHANGELOG.md`.
- [ ] Add `assets/pushmute.svg` icon (copy from source SVG).
- [ ] `packaging/pushmute.desktop` — `Exec=pushmute run`, `Icon=pushmute`,
      `StartupNotify=false`, `Categories=Audio;AudioVideo;Utility;`.
- [ ] Add a top-level `LICENSE` (MIT) file. **Gap surfaced 2026-06-11:** `Cargo.toml`
      declares `license = "MIT"` but no `LICENSE` file exists, and both the PKGBUILD
      and `install.sh` are specified to install it. Must land before those two items.
- [ ] `packaging/PKGBUILD` + `.SRCINFO` — installs binary + unit + `.desktop` +
      icon + LICENSE; post-install message for `systemctl --user enable --now pushmute`.
- [ ] `install.sh` — compile → `~/.local/bin` → unit → `systemctl --user enable --now`
      → two `.desktop` copies (applications + autostart) → icon → input-group warn
      (print fix command, don't abort) → `pushmute doctor`.
- [ ] `install.sh --uninstall [--purge]` — `disable --now` (restores default source
      via ExecStopPost) → remove binary/unit/desktop×2/icon/stale socket → keep config
      unless `--purge` → leave `input` group, print note. Update AUR post-install
      message to mention `systemctl --user disable --now pushmute` before removal.
- [ ] Branding sweep — clap `about` (`src/main.rs:16`), unit `Description=`
      (`pushmute.service:2`), `src/main.rs:1` doc-comment, and README H1 all to
      "PushMute" with "selective mic router" as subtitle/tagline.
- [ ] Update `README.md` — install/run sections, `uwsm` note for Hyprland users,
      GNOME AppIndicator extension note for Ubuntu/Pop!_OS users.

## Related: repo cleanup before public

See `GIT_CLEANUP.md` for the repo-hygiene pass. **Sequencing matters:** AUR fetches
the GitHub tarball, which needs the repo **public** at first-release time, so the
cleanup must finish before the visibility flip. Two cleanup items are coupled to this
work and shouldn't wait for the end: committing `Cargo.lock` (with CI) and the
secret-scan of history (before going public).

## Pick-up point

Recommended next action: user knocks out runbook Steps 1–3 (AUR registration + CI
key, Actions write perms, `cargo install cargo-release`); Claude starts on the build
checklist items that don't depend on any secret (unit file fix, single-instance
guard, doctor, pipewire fixtures).
