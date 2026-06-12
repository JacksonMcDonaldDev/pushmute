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

Audience skews **Arch / Hyprland / waybar** power users.

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
     (Arch/Hyprland); stock GNOME (Pop) needs the AppIndicator extension. The target
     audience actually has the *better* tray story.

## Locked decisions

- **Name:** `pushmute` — binary + crate + AUR package. Verified free on crates.io
  and AUR (2026-06-11). Virtual mic shows as **"PushMute"** (node `pushmute`).
  Clean pre-1.0 break, **no migration shim**.
- **Install spine:** **source-built everywhere** (chosen over prebuilt-first).
  - AUR `makepkg` from the release tarball.
  - `cargo install pushmute`.
  - A local-compile install script for non-Arch (build → `~/.local/bin` → systemd
    unit → `input`-group check → run `doctor`).
  - Prebuilt binaries deferred to a later "don't want to compile" convenience
    (build in old-glibc container when added).
- **Robustness (in scope for this branch):**
  - `pushmute doctor` preflight subcommand, also auto-run at `pushmute run` startup:
    checks the four binaries exist AND that `wpctl inspect` / `pw-dump` produce
    parseable output; fails with one actionable message instead of a silent
    misroute.
  - Pull the scrapers in `src/pipewire.rs` (`list_capture_devices`, `node_id_by_name`,
    `current_default_source`) into pure functions; pin them with **fixtures captured
    from old (Pop) + new (Arch-era) PipeWire** and unit-test in CI.
  - **Not** doing live-PipeWire-in-CI (headless runners make it flaky for little gain
    over fixtures).
- **Pipeline:**
  - **CI (every PR/push to main):** `cargo fmt --check` → `cargo clippy -D warnings`
    → `cargo build` → `cargo test` (incl. fixture tests).
  - **Release cut:** `cargo-release` (`cargo release minor`) bumps `Cargo.toml`,
    updates `CHANGELOG.md`, commits, tags `vX.Y.Z`, pushes — atomically.
  - **CD (on `v*` tag):** GitHub Actions re-runs the gate → publishes to **crates.io**
    → creates a **GitHub Release** (auto source tarball) → auto-pushes the AUR update.
- **AUR:** single **`pushmute`** package built **from the GitHub release tarball**
  (not the crates.io `.crate`, which lacks the systemd unit + LICENSE).
  `depends=(pipewire wireplumber)`, `makedepends=(cargo)`, installs binary + unit +
  LICENSE. Auto-published via `KSXGNU/github-actions-deploy-aur` on tag. `pushmute-git`
  **deferred**.

## Manual runbook (only the user can do these — accounts/keys/secrets)

Order matters: plant flags on both registries, then wire secrets so automation takes
over.

- [ ] **Step 1 — crates.io:** log in with GitHub → API Tokens → new token
  (`publish-new` + `publish-update`). First publish done **locally** to claim the
  name (`cargo login <token>` → `cargo publish`, after the rename). Add the same
  token as GitHub repo secret **`CARGO_REGISTRY_TOKEN`**.
- [ ] **Step 2 — AUR:** register at aur.archlinux.org (note username + email).
  Generate a CI-only key: `ssh-keygen -t ed25519 -C "aur-pushmute-ci" -f ~/.ssh/aur_pushmute -N ""`.
  Add the **public** key to AUR → My Account. Add the **private** key as GitHub repo
  secret **`AUR_SSH_PRIVATE_KEY`**. (Package name gets claimed by the first automated
  push.)
- [ ] **Step 3 — GitHub Actions perms:** Settings → Actions → General → Workflow
  permissions → **Read and write** (lets the release workflow create Releases).
- [ ] **Step 4 — Local tool:** `cargo install cargo-release`.

Values I'll need referenced in the workflow files: crates.io token (secret), AUR
username + email.

## Build checklist (Claude — code side, once runbook flags are planted)

- [ ] `pushmute doctor` subcommand + auto-run at `run` startup.
  (Dev-box baseline 2026-06-11: `pw-dump`, `wpctl`, `pw-loopback` all present in
  `/usr/bin`; Rust 1.96. So doctor's happy path is locally verifiable — the
  missing-binary failure path will need simulating, e.g. a doctored `PATH`.)
- [ ] Extract `src/pipewire.rs` scrapers into pure functions; add old+new PipeWire
  fixtures + unit tests.
- [ ] `.github/workflows/ci.yml` (fmt/clippy/build/test on PR + main).
- [ ] `.github/workflows/release.yml` (on `v*`: gate → crates.io → GitHub Release →
  AUR deploy action).
- [ ] `cargo-release` config (`Cargo.toml` `[workspace.metadata.release]` or
  `release.toml`) + `CHANGELOG.md`.
- [ ] Add a top-level `LICENSE` (MIT) file. **Gap surfaced 2026-06-11:** `Cargo.toml`
  declares `license = "MIT"` but no `LICENSE` file exists, and both the PKGBUILD and
  `install.sh` are specified to install it. Must land before those two items.
- [ ] `packaging/PKGBUILD` + `.SRCINFO` (source = GitHub release tarball).
- [ ] `install.sh` (non-Arch: compile → `~/.local/bin` → systemd unit → input-group
  check → `doctor`).
- [ ] Update `README.md` install/run sections to the new name + channels.

## Pick-up point

Recommended next action: user knocks out runbook Steps 1–4 (crates.io token, AUR
registration + CI key, Actions write perms, `cargo install cargo-release`); Claude
starts `doctor` + pipewire fixtures in parallel (those don't depend on any secret).
