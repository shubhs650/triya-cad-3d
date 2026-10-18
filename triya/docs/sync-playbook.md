# Upstream sync playbook

Policy: ADR-0002. Frozen on v0.14.0 until `triya-v0.1.0-beta`. Then daily.

## 1. Manual merge (what the bot does, by hand)
```bash
git fetch upstream --tags
git checkout -b sync/upstream main
git merge --no-ff upstream/main          # zdiff3 markers; rerere replays known resolutions
# conflicts? see §2
bash triya/ci/check-patch-whitelist.sh upstream/main
export CARGO_TARGET_DIR="$PWD/target"; cargo xtask ci
cargo test -p triya-stdlib -p triya-vessel -p triya-drawing -p triya-ui
cd triya/apps/triya-web && trunk build --release && cd -
git push -u origin sync/upstream && gh pr create --fill --label upstream-sync
```
Record in `PROGRESS.md` Log: upstream range merged, seams re-placed, anything that moved.

## 2. Resolving conflicts (agent recipe)
Conflicts can only occur in: (a) seam files (`Cargo.toml`, `engine/cmd/mod.rs`,
`ui-egui/toolbar.rs`, the three locale JSONs, `xtask/layers.rs`, `xtask/web_offline.rs`),
(b) `Cargo.lock`, (c) root files with `merge=ours` (should not conflict), (d) a Triya crate that
depends on an upstream API that changed (compile error, not a git conflict).

- **Seam file**: take **theirs**, then re-insert our marked one-liner at the new anchor:
  `git checkout --theirs <file>` → edit → `git add <file>`. `triya_extra.rs` is a new file and
  never conflicts; its two call sites in `mod.rs` do. Update the anchor column in `PATCHES.md`
  and regenerate `0001-seams.patch` (`git diff upstream/main -- <seam files> > triya/patches/0001-seams.patch`).
- **Locale JSON**: take theirs, re-add the `ui.heading_vessel` line (keep catalogs sorted —
  `cargo xtask lists --fix`).
- **`Cargo.lock`**: `git checkout --theirs Cargo.lock && cargo update -w --offline || cargo generate-lockfile`; verify the triya crates are still in it.
- **API change breaking a Triya crate**: fix in `triya/`; if upstream renamed a command id or
  param our generator uses, update `triya-vessel::emit` and the `docs/upstream-map.md` table in
  the same PR. `cargo semver-checks --baseline-rev <last-merged-sha> -p solidcraft-engine` gives
  an early list (misses type/generic changes).
- **Format bump upstream**: nothing to do — we store nothing in the format (ADR-0005), but run the
  metadata round-trip test and open a pre-merge design to confirm the vessel JSON survived.
- Finish: `git commit` (rerere records the resolution; the bot commits `triya/rr-cache`).

## 3. The first post-beta merge (dedicated task, PLAN P1.1)
Expect several hundred upstream commits (format 25 → N, ribbon redesign, kernel FFI work).
1. Read upstream `CHANGELOG`/release notes for v0.15–v0.x: list renamed commands, moved files,
   changed `CommandSpec` fields.
2. Merge **tag by tag** (`v0.15.0`, `v0.16.0`, …) rather than straight to `main` — smaller
   conflict sets, rerere learns each seam move once.
3. After each tag: whitelist check, `cargo xtask ci`, regenerate reference vessels A/B/C, compare
   volumes and the three golden sheets; capture any kernel regression in `kernel-cases/`.
4. When done: arm the cron in `upstream-sync.yml`, post the two upstream issues if not yet done.

## 4. Upstream-first loop for a mechanism we need
1. Implement in `triya/` behind the smallest seam.
2. Open an upstream issue (owner posts) describing the hook; if welcomed, open a PR to upstream
   from a branch cut from `upstream/main` (not from our `main`), following upstream's claim rules.
3. When merged upstream and pulled in, delete the seam row and the marker.
