You are starting development of Triya CAD 3D, a rebranded fork of SolidCraft v0.14.0 with an
overlay in `triya/`. Read, in order: root `CLAUDE.md` (upstream rules; it imports
`triya/CLAUDE.md`), `triya/AGENTS.md`, `triya/PROGRESS.md`, `triya/PLAN.md` Phase 0,
`triya/ARCHITECTURE.md`, `triya/SKELETON-REPORT.md` (what the kickoff skeleton already does and
its open items), `triya/docs/upstream-map.md`, `triya/patches/PATCHES.md`.

Then execute Phase 0 tasks T0.2–T0.6 in order, one PR each, following the session loop in
AGENTS.md §1. Specifically:

1. T0.2: `export CARGO_TARGET_DIR="$PWD/target"; cargo xtask ci` with the toolchain upstream pins
   (rust 1.95; a newer clippy flags upstream code in `crates/kernel/src/prism_local.rs`, which is
   not ours to fix). Fix nothing in upstream; if something fails because of the environment,
   record it. Confirm `Triya CI` runs on a PR.
2. T0.3: finish the skeleton: run `cargo test -p triya-ui`; add `triya/apps/triya-cli` (a thin
   wrapper that calls `solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS)` and
   then delegates to upstream's CLI main, so `triya-cli run/mcp/commands/eval/snapshot` know the
   vessel commands); install the wasm32 target and run `trunk build --release` in
   `triya/apps/triya-web`, confirming the `web_offline.rs` seam picks up our `sw.js`
   (`cargo xtask web-offline-test --site dist/triya-web`).
3. T0.4: verify the seams; `bash triya/ci/check-patch-whitelist.sh v0.14.0` must pass locally and
   in CI; refresh `triya/patches/0001-seams.patch`.
4. T0.5: `triya-cli run triya/crates/triya-vessel/examples/cylinder.json --out /tmp/cyl.step`
   (create the example from the skeleton's test) and record the STEP size and the measured
   volume in PROGRESS.md.
5. T0.6: confirm `triya-pages.yml` deploys (owner has set Pages → GitHub Actions). Record the
   wasm size.

Decide alone what AGENTS.md §3 allows; write anything in §4 into PROGRESS.md "Owner decisions"
and continue. At the end of the session, PROGRESS.md must show the real state, and you end with
a ≤ 10-line summary for the owner: what landed (PR numbers), measurements, blockers, next task.
