# ADR-0002 — Overlay architecture, merge-based upstream sync, freeze until beta

**Status:** accepted 2026-10-17

## Context
Upstream ships several releases a day. A fork that edits upstream files broadly becomes
un-mergeable within days. We must add large modules, a rebrand, our own CI, and still pull
upstream fixes and the eventual kernel swap.

## Decision
1. **Overlay**: all Triya code lives in `triya/crates/*` and `triya/apps/*`, added to the
   workspace as ordinary members. Upstream files are edited only at **whitelisted seams**
   (`triya/patches/PATCHES.md`), each marked `Modified by Triya CAD 3D` where the syntax allows.
   CI (`triya/ci/check-patch-whitelist.sh`) fails on any other upstream diff.
2. **Sync model**: a true GitHub fork; `git merge upstream/main` (never rebase `main`) with
   `rerere` + `zdiff3`; root files we own outright (`README.md`, `NOTICE`, `triya-*.yml`) use
   `merge=ours` via `.gitattributes`. A daily workflow (`upstream-sync.yml`) merges into
   `sync/upstream`, runs the full gates and opens a PR, or opens one labelled conflict issue.
3. **Freeze**: no upstream merge until the tag `triya-v0.1.0-beta` exists. The sync workflow is
   `workflow_dispatch`-only until the cron line is uncommented after the beta.
4. **Upstream-first for mechanisms**: brand constants, the command-registration slot, and (later)
   a metadata extension block are proposed upstream early; when accepted, the corresponding seams
   are deleted.

## Alternatives rejected
- Rebase/patch-stack (VSCodium, ungoogled-chromium style): re-applies patches on every bump —
  a daily chore at this velocity.
- Submodule/subtree of upstream inside a Triya workspace: Cargo forbids nested workspaces and we
  would have to replicate upstream's root `[patch.crates-io]` → `vendor/` entries.
- Cargo git-dependencies on upstream crates with `[patch]`: good end-state for untouched
  layers, but cannot inject UI/branding, still needs the vendor patches, and `cargo deny` denies
  git dependencies in this workspace.

## Consequences
- Branding is a copy of the web shell (`triya/apps/triya-web`), not edits to upstream's.
- Identifiers stay upstream's (ADR-0006) so merges never collide on format or command ids.
- The first post-beta merge (~hundreds of upstream commits) is a dedicated two-day task
  (`prompts/08-first-upstream-merge.md`); expect the toolbar and layers seams to move.
