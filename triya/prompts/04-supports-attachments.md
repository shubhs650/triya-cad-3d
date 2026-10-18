Phase 4 (PLAN T4.1–T4.5): supports and attachments. Horizontal vessels lie along +X with the
datum at the left TL (`global.datum = left_tl`). Implement saddles (T4.1) on reference vessel B,
legs/brackets/anchor chairs (T4.2) on vessel C and A, rings/limpet coil/lugs/insulation
rings/clips (T4.3) on A, `standardRef` + `sizeKey` + preset tables + `vessel.import_tables`
private import (T4.4 — the repo must contain **no** EIL values; a sample *fictional* private
table lives under `tests/`), BOM and weights with the handbook cross-check column (T4.5).
Analytic tests per PLAN. Commit vessels A, B, C JSON under `tests/reference/` with their
expected volumes. Day 15 checkpoint: at the end of this phase, write the "reduced beta" decision
in PROGRESS.md (continue to Phase 5+6, or Phase 5 only) with the evidence.
