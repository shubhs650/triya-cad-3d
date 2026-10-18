# Preparing the Triya CAD 3D repository (step by step)

This is the one-time setup that turns `tangency-labs/solidcraft` into your fork
`<you>/triya-cad-3d`, frozen on **v0.14.0** for the beta window, with the Triya overlay
(`triya/`) committed and the automation armed-but-dormant. Follow it top to bottom once.
Everything after this file is driven by Claude Code using `triya/CLAUDE.md`.

**Developing in Claude Code cloud (Projects)?** Read `CLOUD-SETUP.md` instead of §2–§5 of this
file; §1 and §6–§10 apply to both.

Assumptions (local route): GitHub account with `gh` CLI installed and logged in (`gh auth login`), Rust
stable ≥ 1.95 (`rustup update stable`), `trunk` 0.21.x (`cargo install trunk --version 0.21.14 --locked`),
`wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`), Chrome or
Chromium (upstream's `web-offline-test` drives headless Chrome), ~15 GB free disk for `target/`.

---

## 1. Fork on GitHub (UI, 2 minutes)

1. Open https://github.com/tangency-labs/solidcraft → **Fork**.
2. Owner: your account (or the `openengtools` org if you have made one).
   Repository name: **`triya-cad-3d`**.
   Description: `Triya CAD 3D — parametric 3D CAD PWA for pressure-vessel engineers, part of Open Engineering Tools. Based on SolidCraft.`
   **Untick "Copy the main branch only"** — you want the tags (we freeze on the `v0.14.0` tag).
3. Keep the fork **public** (GitHub Actions minutes are free for public repos; upstream's CI is heavy).

## 2. Clone and wire the remotes

```bash
gh repo clone <you>/triya-cad-3d
cd triya-cad-3d
git remote add upstream https://github.com/tangency-labs/solidcraft.git
git fetch upstream --tags
git remote -v        # origin = your fork, upstream = tangency-labs
```

## 3. Freeze `main` on v0.14.0 (do this before any other commit)

Your fork's `main` currently points at whatever upstream `main` was at fork time (many commits
past v0.14.0, format already bumped). We rebuild it from the tag. This is the **only**
force-push the project ever does, and it happens before anyone else has the repo.

```bash
git checkout -B main v0.14.0
git push --force origin main
git branch -f upstream-frozen v0.14.0 && git push origin upstream-frozen   # bookmark of the freeze point
```

## 4. Git configuration that makes upstream merges survivable

```bash
git config rerere.enabled true          # remember every conflict resolution, replay it next time
git config rerere.autoupdate true
git config merge.conflictStyle zdiff3   # conflict markers agents read reliably
git config pull.rebase false            # we merge, never rebase, main
git config merge.ours.driver true       # enables the "merge=ours" attribute used in .gitattributes
```

The kit's `.gitattributes` pins a handful of root files to **ours** on merge (`README.md`,
`NOTICE`, `.github/workflows/triya-*.yml`): upstream edits to those never produce conflicts.
Every other upstream file merges normally.

## 5. Drop in the Triya kit

Copy the contents of this kit folder into the repo root (it only adds `triya/`,
`.github/workflows/triya-*.yml` and `upstream-sync.yml`, `.gitattributes`, the root `NOTICE`
and `README.md` replacements, plus the handful of one-line seams listed in
`triya/patches/PATCHES.md`):

```bash
# from the directory that contains this REPO-SETUP.md
cp -r ./triya ./.github ./.gitattributes ./NOTICE ./README.md <repo>/
cd <repo>
git apply --3way triya/patches/0001-seams.patch      # the seams in upstream files (+ the new engine/cmd/triya_extra.rs)
git apply --3way triya/patches/0002-cargo-lock.patch # Cargo.lock delta for v0.14.0 (new members only); if it fails: cargo update --workspace
echo "@triya/CLAUDE.md" >> CLAUDE.md                 # agents load Triya rules after upstream's
```

What the seams are (13 rows, 10 files — `triya/patches/PATCHES.md` is the source of truth):
workspace members + deps in `Cargo.toml`; two call sites in `crates/engine/src/cmd/mod.rs` plus
the new extension-slot file `crates/engine/src/cmd/triya_extra.rs`; `VESSEL` in
`crates/ui-egui/src/toolbar.rs` with `ui.heading_vessel` in the en/ja/zh locale catalogs; layer
rows in `xtask/src/layers.rs`; the `sw.js` template path in `xtask/src/web_offline.rs`;
`Cargo.lock`. The skeleton was compiled and tested against v0.14.0 before packaging
(`triya/SKELETON-REPORT.md`; its absolute paths refer to the build sandbox, not your machine).

Check that nothing outside the whitelist was touched:

```bash
bash triya/ci/check-patch-whitelist.sh v0.14.0
```

Then build once to prove the freeze point is healthy on your machine:

```bash
export CARGO_TARGET_DIR="$PWD/target"
cargo xtask ci                     # fmt, clippy -D warnings, tests, assets, layers, wasm — 20–40 min cold
cargo test -p triya-stdlib -p triya-drawing -p triya-vessel
cd triya/apps/triya-web && trunk serve     # http://127.0.0.1:8781 — upstream app under a Triya shell
```

Commit:

```bash
git add -A
git commit -m "triya: kickoff kit — overlay crates, plan, ADRs, workflows, seams

Freeze point: SolidCraft v0.14.0 (2bb9c81). See triya/PLAN.md and triya/ARCHITECTURE.md."
git push origin main
```

## 6. GitHub settings (UI, 5 minutes)

**Actions → General**
- Workflow permissions: **Read and write**; tick **Allow GitHub Actions to create and approve pull requests** (the sync bot opens PRs).

**Actions → Workflows → disable** (click the workflow → ⋯ → *Disable workflow*; this avoids
editing/deleting upstream workflow files, which would conflict on every merge):
- `Release`, `WinGet`, `Snap Store`, `Flatpak Pages`, `Claim`, `Pages` (replaced by `Triya Pages`), `Generated files`, `Lists`.
- Keep **`CI`** enabled (upstream's quality gate; it tests Triya crates too because they are workspace members) and the three `triya-*` workflows.

**Pages** → Source: **GitHub Actions**. The site lands at `https://<you>.github.io/triya-cad-3d/`
(app at `/app/`). When the OET domain is ready, add a `CNAME` in `triya-pages.yml` (see comment there).

**Branches** → protect `main`: require a pull request (0 approvals is fine for a solo project),
require `CI` and `Triya CI` status checks, block force pushes. Agents then always work on
branches and merge with `gh pr merge --squash` once green.

**General** → Features: Issues on, Discussions on (future beta testers), Wiki off.
Topics: `cad`, `rust`, `wasm`, `pwa`, `pressure-vessel`, `asme`, `solidcraft`, `egui`.

**Secrets**: none needed. The sync bot uses the built-in `GITHUB_TOKEN`.

## 7. Issue labels the automation expects

```bash
gh label create upstream-conflict -c B60205 -d "Daily upstream merge hit a conflict"
gh label create upstream-sync     -c 0E8A16 -d "Automated upstream merge PR"
gh label create seam              -c FBCA04 -d "Touches an upstream file (must be in PATCHES.md)"
gh label create vessel -c 1D76DB; gh label create drawing -c 1D76DB; gh label create stdlib -c 1D76DB
gh label create pwa -c 1D76DB;    gh label create beta-blocker -c B60205
```

## 8. Claude Code project

Open the repo in Claude Code (desktop or CLI) and create a Project for it. Claude Code reads
the root `CLAUDE.md` (upstream's rules) which now ends with `@triya/CLAUDE.md` (ours). Start the
first session with the prompt in `triya/prompts/00-kickoff.md`; every later session starts with
`triya/prompts/resume.md`. Agents update `triya/PROGRESS.md` as they go — that file, plus the PR
list, is how you see where things stand without reading code.

## 9. Two upstream issues to open in week one (not after the beta)

Open these on `tangency-labs/solidcraft` as feature requests; if accepted they delete most of
our seams:

1. **"Registration point for out-of-tree command arrays"** — a way for an app crate to append
   `&[CommandSpec]` slices to the registry without editing `crates/engine/src/cmd/mod.rs`.
   Mention it is the smallest viable step toward #1221 (plugins) and that a downstream module
   (parametric pressure vessels) exists that would use it.
2. **"Centralise brand constants"** — one module holding product name, app id, URLs and About
   text, so the renames `docs/compatibility.md` documents become a one-file change.

Be factual, link to your fork, and say "based on SolidCraft, not affiliated" — the same words
our NOTICE and About page use.

## 10. What *not* to do

- Don't rename `.solidcraft`, `solidcraft/N`, `solid.*` command ids or `SOLIDCRAFT_*` env vars. Files must stay interchangeable with upstream (ADR-0006).
- Don't merge `upstream/main` before the beta ships (ADR-0002). The sync workflow is `workflow_dispatch`-only until you flip the cron on.
- Don't edit upstream files outside `triya/patches/PATCHES.md`. If you must, add the row and the `Modified by Triya CAD 3D` marker in the same commit; CI fails otherwise.
- Don't read FreeCAD, OpenCASCADE, CadQuery/OCP, SolveSpace, LibreCAD, QCAD or any GPL/LGPL/AGPL source — upstream's rule, and ours (ADR-0004, ADR-0006).
- Don't extract data, tables or screenshots from PV Elite, COMPRESS or AutoPIPE Vessel. Public documentation and general conventions only (upstream's "no commercial CAD products" rule applies to UX inspiration as well).
