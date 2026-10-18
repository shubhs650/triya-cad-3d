Post-beta task P1.1: first upstream merge. Follow `triya/docs/sync-playbook.md` §3 exactly:
read upstream release notes v0.15.0 → latest, list renamed commands/moved files/changed
CommandSpec fields; merge **tag by tag** on `sync/upstream`; after each tag: whitelist check,
`cargo xtask ci`, regenerate vessels A/B/C, compare volumes and golden sheets, capture kernel
regressions in `kernel-cases/`; re-place seams (`engine/cmd/mod.rs` call sites, `toolbar.rs`
TABS, locale JSONs, `layers.rs` rows, `web_offline.rs`) and refresh `0001-seams.patch` and
PATCHES.md anchors; commit `triya/rr-cache`. Open one PR per tag or one PR for the whole range
if conflicts were trivial. When merged, uncomment the cron in
`.github/workflows/upstream-sync.yml` and write the owner summary: upstream range, seams moved,
regressions, what changed in the kernel plan.
