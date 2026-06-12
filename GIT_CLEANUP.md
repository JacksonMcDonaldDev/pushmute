# Repo Cleanup & Hygiene (before going public)

Status: **backlog** — tackle as a post-step to `DEPLOYMENT_PLAN.md`, with the
exceptions called out under "Sequencing" below. This is the "make the repo
presentable and safe to flip public" pass.

## Why this exists / the sequencing constraint

Going public is **not** a final cosmetic step — the AUR half of the pipeline
fetches `https://github.com/JacksonMcDonaldDev/pushmute/archive/vX.Y.Z.tar.gz`,
which requires the repo to be **public** at first-release time. So:

```
implement deployment code + workflows (private repo)
        │
        ▼
  REPO CLEANUP (this doc)  ← must finish before the repo flips public
        │
        ▼
   flip repo public
        │
        ▼
  plant runbook flags → cut first tagged release (triggers AUR)
```

Two items here are coupled to deployment work and should **not** wait for the
end:

1. **`Cargo.lock` policy** — ideally lands with/before the CI workflow so CI and
   AUR build against pinned deps from day one.
2. **Secret scan of history** — must pass before the visibility flip, no
   exceptions.

Everything else is genuinely post-step.

## Coupling back to DEPLOYMENT_PLAN.md

- If this cleanup relocates planning docs (e.g. into `docs/`), update any in-repo
  links that point at them (`DEPLOYMENT_PLAN.md` ↔ `GIT_CLEANUP.md` cross-refs,
  README links). Low stakes — crates.io was dropped, so there's no published
  `.crate` whose `exclude` list needs to track doc paths anymore.

## Checklist

### Must land before flipping public

- [ ] **Commit `Cargo.lock`.** Remove `Cargo.lock` from `.gitignore` and track
      it — this is a binary crate, so pinned deps are the convention (reproducible
      CI / AUR / contributor builds). Coordinate with the CI workflow.
- [ ] **Secret-scan the full history** (`gitleaks detect` or `trufflehog git`).
      Nothing sensitive is expected (it's a desktop daemon), but the runbook
      discusses tokens/keys, so verify none were ever committed before the repo is
      world-readable. A leak found *after* going public means rotating, not just
      deleting.
- [ ] **Audit `.claude/` tracking.** Keep shareable `settings.json` if desired;
      gitignore `settings.local.json` (local paths, machine-specific permissions).
      Decide whether the `.claude/` dir belongs in a public repo at all.

### Doc organization decision (the original question)

- [ ] **Decide where planning/internal docs live.** Candidates:
      `DEPLOYMENT_PLAN.md`, `PRD.md`, `TODO.md`, `GIT_CLEANUP.md`. Options:
      - **Keep at root, public** — maximal transparency; fine for a personal OSS
        project. Simplest.
      - **Move to `docs/`** — tidier root, still public; update `exclude` + links.
      - **Stop tracking / move out of repo** — if any read as internal-only.
      They're already kept out of the published crate via `exclude`; this decision
      is purely about GitHub visibility + repo tidiness, not the crate.
- [ ] Confirm `scripts/dev-refresh.sh` (dev-only tooling) is fine to ship public,
      or move under a `dev/` / `scripts/` clearly-dev namespace.

### Presentation polish (nice-to-have, not blocking)

- [ ] Add `repository = "https://github.com/JacksonMcDonaldDev/pushmute"` to
      `Cargo.toml` for provenance. (Was a crates.io-publish requirement; now just
      nice-to-have repo hygiene since we don't publish.)
- [ ] README pass for a public first-time reader (badges, install matrix, the
      `uwsm`/AppIndicator notes already tracked in the deployment plan).
- [ ] `CONTRIBUTING.md` + issue/PR templates (only if you want contributions).
- [ ] Decide keep-vs-squash on history. Recommendation: **keep** — the
      `smr → pushmute` rename and PRD-first history are honest and harmless.
- [ ] Confirm the public-facing author email
      (`jacksonmcdonalddev@gmail.com`) is the identity you want on every commit,
      or switch to a GitHub noreply for future commits.
- [ ] Branch hygiene: merge `deployment-pipeline` → `main`, confirm `main` is the
      default branch, prune stale branches.

## Out of scope

Anything that changes shipped behavior or the release pipeline — that's
`DEPLOYMENT_PLAN.md`. This doc is strictly about repository organization,
provenance, and safe-to-publish hygiene.
