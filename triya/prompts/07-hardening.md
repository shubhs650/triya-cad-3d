Phase 7 (PLAN T7.1–T7.5): beta hardening against `triya/docs/beta-dod.md`. Fuzz vessel JSON
(10 k cases, 0 panics), measure regenerate time for vessel A in the browser and wasm size, run
the OPFS round trip, open the saved `.solidcraft` in an unmodified upstream v0.14.0 build
(`git worktree add /tmp/upstream v0.14.0 && cd /tmp/upstream && cargo run -p solidcraft-cli -- eval <file>`),
export STEP for the owner to open elsewhere. Write the README (quick start, own screenshots,
limitations, FAQ "why .solidcraft", licence/attribution). Tick every DoD box in PROGRESS.md with
evidence or write why it is not ticked. Then tag `triya-v0.1.0-beta`, confirm Pages deployed,
and produce the owner summary with the live URL.
