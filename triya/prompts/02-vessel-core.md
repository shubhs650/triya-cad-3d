Phase 2 (PLAN T2.1–T2.6): the vessel core. Source of truth: `triya/docs/vessel-schema.md`;
architecture: ADR-0003, `triya/ARCHITECTURE.md` §3; upstream command shapes:
`triya/docs/upstream-map.md` §3; what exists: `triya/SKELETON-REPORT.md` (cylinder emission via
explicit-plane `sketch.create` → `sketch.rectangle.two_point` → `sketch.finish` → `solid.revolve`,
`$ref:<step>.<field>` substitution, one-undo-step nested execution, metadata persistence).
Order: model completion (T2.1, schemars JSON schema checked in) → evaluator with the DIN
worked-example tests (T2.2; exact geometry vs DIN's linear fits — document the 0.06/0.4 mm
differences) → emitter per element with analytic-volume engine tests (T2.3, one PR per 2–3
element kinds; profiles with arcs need `sketch.arc.*` — read the spec params first) → body flange
+ skirt (T2.4) → regenerate hardening: group tagging, chunked metadata above 60 KiB,
`vessel.inspect` (T2.5) → hostile coverage (T2.6).
Rules: commands only (no `solidcraft-kernel`/`truck` imports), knuckles as profile arcs, every
dimension a `V<id>_*` parameter, never panic, caps on counts. For each element, write the
analytic volume test first, run it (fails), implement, run (passes), then export STEP once with
`triya-cli run` and record the measured volume vs analytic in PROGRESS.md "Measurements".
Finish the phase by regenerating reference vessel A without attachments and committing its
JSON under `triya/crates/triya-vessel/tests/reference/vessel_a.json`.
