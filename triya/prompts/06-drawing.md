Phase 6 (PLAN T6.1–T6.7): the fabrication drawing generator, per `triya/docs/drawing-model.md`
and ADR-0004. Rust only; no OCCT. The skeleton has the model types, the `Plotter` trait and an
`SvgPlotter`. DXF: evaluate upstream's public `solidcraft_io::write_dxf_drawing` first and add
the `dxf` crate only for missing entities (record the decision); PDF via `printpdf` (verify font
embedding in wasm first — if it fails within 2 h, switch to the browser-print fallback through
`web-sys` and record it). Run T6.1 (epaint preview) and T6.6 (DXF/PDF backends) as parallel
subagents in worktrees on `triya-drawing` only; main session does T6.2–T6.5 (elevation, plan,
section with hatch, rule-based dimensions, tables, title block). Golden-file tests for vessels A,
B, C (golden `.svg` fixtures need ATTRIBUTION rows); every dimension carries its `source` field
id. Visual review: render each sheet to PNG (resvg or the egui preview snapshot), look at it, fix
overlaps. Owner checks the DXF in a viewer — write the exact file paths in the summary.
