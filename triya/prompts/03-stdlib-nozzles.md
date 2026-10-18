Phase 3 (PLAN T3.1–T3.5): standards library and nozzles. Run T3.1 and T3.2 (tables) as a
parallel subagent in its own worktree on `triya-stdlib` only, following
`triya/docs/standards-library.md` and ADR-0007: delete the skeleton's `illustrative` rows, seed
B16.5 Cl 150/300 + B36.10 from bd_warehouse (Apache-2.0; add the ATTRIBUTION row; re-key, don't
copy code), hand-key Cl 400–2500 and B16.47 A/B from two independent public vendor tables, every
row with `source[]` and `verified`, validator tests green, disagreements logged in §5 of the
doc. Never use PV Elite/COMPRESS/AutoPIPE data or any copyrighted standard PDF. Meanwhile in the
main session implement nozzle frames (T3.3: radial, hillside, tangential, tilted,
parallel-to-axis, head-radial-at-R), neck from the pipe table, set-in/set-on, pad, projections;
flange on nozzle (T3.4); boolean validation + fallback with `kernel-cases/` capture (T3.5 —
deliberately include an equal-radius crossing-cylinder case). Engine tests: volume after
cut+join for six configurations on cylinder and head within tolerance; the fallback test must
show regenerate still succeeds with `flags.boolean_failed = true`. Record every kernel case in
PROGRESS.md.
