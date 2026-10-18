# ADR-0004 — Fabrication drawings are data-driven and native Rust; no OpenCASCADE

**Status:** accepted 2026-10-17

## Context
Upstream's 2D drawings milestone (M11) is "deferred until further notice"; the only drafting is a
view-to-DXF export. Commercial vessel tools (COMPRESS via Codeware Interface, PV Elite via
CADWorx/PV Fabricator, AutoPIPE Vessel) generate drawings from the *component list* — views,
schedules, BOM, weld details, templates — not from a generic hidden-line projection. Exact HLR
exists in open source only in OpenCASCADE (`HLRBRep`), which is LGPL; upstream forbids reading or
linking LGPL code, and OCCT HLR is slow and fragile on assemblies. No Rust crate implements HLR.

## Decision
1. `triya-drawing` generates the **GA/fabrication sheet from the vessel model**: analytic
   elevation and plan views for revolved geometry, a section view from the profile with hatch,
   rule-based dimensions emitted by stable field ids, tables (design data, nozzle schedule, BOM,
   notes, revisions), SVG title-block templates.
2. Rendering/export through a `Plotter` trait (our own; KiCad's plotter abstraction is the
   *idea* reference only): SVG (also the egui preview), DXF (upstream's public
   `solidcraft_io::write_dxf_drawing` first, `dxf` crate only for missing entities), PDF
   (`printpdf`, with browser-print fallback). All permissively licensed, Rust only (upstream's
   no-JavaScript rule).
3. **No OCCT/cadrum**, not even as an optional feature, while this fork intends to upstream any
   drawing code. Revisit only as a consciously separate, non-upstreamable module.
4. Generic mesh-based HLR for arbitrary parts (crease + silhouette edges, BVH visibility, arc
   re-fit) is post-beta (P2), written from published algorithm descriptions — SolveSpace and
   three-edge-projection are *algorithm references by description*; their source is not read.
5. `triya-drawing` is behind the Cargo feature `drawing2d` so that, if upstream ships M11, we can
   switch to an adapter that places vessel annotations on upstream sheets.

## Consequences
- The beta sheet is excellent for vessels and useless for a bracket modelled by hand — accepted;
  the DoD is vessel-centric.
- DXF dimensions are exploded (lines + text) in the beta; true DIMENSION entities post-beta.
- Section hatch is lines on a `HATCH` layer.
