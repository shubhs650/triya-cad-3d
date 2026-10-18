# Triya CAD 3D — setup for Claude Code **cloud** development (Projects)

You will not have a local clone. Everything runs in Claude Code cloud sessions (claude.ai/code),
coordinated by a **Project**, pushing to your GitHub fork. This replaces the local steps in
`REPO-SETUP.md` §2–§5; the GitHub-settings steps (§6–§9) still apply unchanged.

Facts about the cloud VM that shape the plan (from Anthropic's docs, Oct 2026): fresh Ubuntu 24.04
VM per session, **4 vCPU / 16 GB RAM / 30 GB disk**; `rustc`/`cargo`/`gh`/`git` pre-installed;
**Trusted** network reaches crates.io, static.rust-lang.org/rustup.rs, github.com (incl. release
assets), ubuntu.com apt mirrors; a **setup script** runs once per environment and is cached as a
filesystem snapshot only if it finishes in **~5 minutes**; a session's `target/` build cache is
**lost when its VM is reclaimed** (inactivity); the GitHub proxy **rejects tag pushes and branch
deletions** (use `gh api`), but allows force-pushes to unprotected branches.

---

## Step 1 — Fork (GitHub UI, 2 min)
1. https://github.com/tangency-labs/solidcraft → **Fork** → owner: you; name **`triya-cad-3d`**;
   **untick "Copy the main branch only"** (you need the `v0.14.0` tag). Public.
2. Install the **Claude GitHub App** on the fork: https://github.com/apps/claude → Configure →
   select `triya-cad-3d`. Projects require the app on every repository a thread clones.

## Step 2 — Freeze `main` on v0.14.0 (GitHub UI, 3 min, no git needed)
1. In the fork, open the branch/tag switcher → **Tags** → `v0.14.0`. In the switcher again, type
   `frozen-v0.14.0` → **Create branch frozen-v0.14.0 from v0.14.0**.
2. **Settings → General → Default branch** → switch to `frozen-v0.14.0`.
3. **Branches** page: rename `main` → `upstream-main-at-fork` (keep it as a bookmark), then rename
   `frozen-v0.14.0` → `main`.
   (Alternative: let the first cloud thread run `git checkout -B main v0.14.0 && git push --force origin main`
   before branch protection exists — the proxy allows it.)

## Step 3 — Get the kit into the repo
**Option A (recommended): tell me the fork name** (`<you>/triya-cad-3d`) once the Claude GitHub
App is installed. From this session I can attach the repo, apply the kit and seams on a branch
`triya/kickoff-kit`, run the whitelist check, and open the PR for you to merge.

**Option B: via the Project.** Create the Project (Step 5), add `triya-cad-3d-kickoff-kit.zip`
to its **Library**, and make the first thread's task:
> Unzip `/mnt/project-files/triya-cad-3d-kickoff-kit.zip` into the repository root (add files,
> never replace existing folders), run `git apply --3way triya/patches/0001-seams.patch` and
> `git apply --3way triya/patches/0002-cargo-lock.patch`, append the line `@triya/CLAUDE.md` to
> the root `CLAUDE.md`, run `bash triya/ci/check-patch-whitelist.sh v0.14.0`, commit as
> `triya: kickoff kit — overlay crates, plan, ADRs, workflows, seams`, push branch
> `triya/kickoff-kit` and open a PR. Do not build yet.

Merge that PR (squash). From here on, the repo is the source of truth.

## Step 4 — Cloud environment (claude.ai/code → Settings → Cloud environments → New)
- **Name**: `triya-cad` · **Network**: Trusted (default is enough; add `Custom` domains only if a
  build needs one — the error "Host not allowed in a cloud session" names it).
- **Environment variables**: `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=4`, `CARGO_TERM_COLOR=always`,
  `CARGO_TARGET_DIR=/root/target`, `TRUNK_VERSION=v0.21.14`.
- **Setup script**: paste `triya/cloud/setup-script.sh` (adds the wasm32 target, clippy/rustfmt,
  prebuilt `trunk`, and Chromium for `web-offline-test`; ~2–3 min; cached). Do **not** build the
  workspace in the setup script — it would exceed the 5-minute cache limit and never be cached.

## Step 5 — Create the Project (claude.ai/code → Projects → New project)
- **Name**: Triya CAD 3D · **Goal**: "Ship the Triya CAD 3D beta (installable PWA with the
  parametric vessel module and GA drawing export) by 8 Nov 2026 per `triya/PLAN.md`."
- **Context**: repository `<you>/triya-cad-3d`.
- **Project settings → Environment**: `triya-cad`.
- **Project settings → Memory → Project instructions**: paste `triya/cloud/project-instructions.md`.
- **Project settings → General**: Thread model Opus, effort high for Phases 0–2 (geometry and seams
  are where mistakes are expensive); consider Sonnet for table-keying threads (T3.1/T3.2).

## Step 6 — First batch (in the Project conversation)
Send, one at a time at first (per the docs' advice to check how the first thread reports back):
1. The text of `triya/prompts/00-kickoff.md` (Phase 0: T0.2–T0.6). One thread.
2. When its PRs are merged, send `triya/prompts/01-rebrand-pwa.md`.
3. From Phase 2 on, send `triya/prompts/resume.md` and let the Project run **one thread per
   phase** (not per task) — a thread keeps its VM and its compiled `target/` for the whole phase,
   which is the only build cache you have. Tasks marked ∥ in PLAN.md may be separate threads.
4. Answer the **Owner decisions** in `triya/PROGRESS.md` when a thread reports them; tell Claude
   "remember: …" for standing corrections so they land in project memory.

## Step 7 — What is different in the cloud (already written into the kit)
- **Builds**: the first `cargo xtask ci` in a fresh VM takes 20–40 min on 4 vCPU and recompiles in
  every new thread. Threads therefore run `cargo check`/targeted `cargo test -p triya-*` while
  working and leave the full `cargo xtask ci` to GitHub Actions on the PR (`CI` + `Triya CI` are
  required checks). A thread that idles long enough loses its VM — keep phase threads busy or
  accept the rebuild.
- **Memory**: 16 GB is enough for the workspace; if a link step is killed, `CARGO_BUILD_JOBS=2`.
- **Tags**: the beta tag `triya-v0.1.0-beta` is created with
  `gh api repos/<you>/triya-cad-3d/git/refs -f ref=refs/tags/triya-v0.1.0-beta -f sha=<sha>`
  (git tag pushes are rejected by the proxy).
- **Branch cleanup**: `gh pr merge --squash --delete-branch` deletes via the API — fine.
- **Screenshots**: `solidcraft-cli snapshot`/`ui.render` work headless (software rendering); the
  desktop app (`cargo run -p triya`) needs `Xvfb :99` in the VM — threads use the snapshot path.
- **Pages**: `triya-pages.yml` deploys on merge to `main`; a thread cannot preview the deployed
  PWA in a browser, so **you** install it on your laptop/phone and report back (DoD A).
- **Owner tasks that stay with you**: GitHub settings (REPO-SETUP §6–§7), posting the two upstream
  issues (§9), opening the DXF/STEP in a viewer, installing the PWA, answering owner decisions.
