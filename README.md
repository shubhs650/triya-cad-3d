# Triya CAD 3D

Parametric 3D CAD in the browser for pressure-vessel and static-equipment engineers, with a
parametric vessel builder (shell courses, heads, cones, skirts, nozzles with ASME B16.5/B16.47
flanges, saddles, legs, brackets, rings, limpet coils, lugs) and automatic fabrication (GA)
drawings. Part of [Open Engineering Tools](https://openengtools.com). Installable PWA, works
offline, no login, no cookies.

> **Based on [SolidCraft](https://github.com/tangency-labs/solidcraft) v0.14.0**, © SolidCraft
> contributors, MIT OR Apache-2.0. Triya CAD 3D is an independent project and is not affiliated
> with or endorsed by the SolidCraft project. See [NOTICE](NOTICE).

**Status:** pre-beta (beta target 8 Nov 2026). See [`triya/PROGRESS.md`](triya/PROGRESS.md).

## What you get
- Everything SolidCraft 0.14 does: sketcher with constraints, parametric timeline, extrude /
  revolve / sweep / loft / shell / fillet / patterns / assemblies, STEP AP242 & IGES I/O, MCP
  server for AI agents, desktop and web.
- **Vessel tab** (being built, Phases 2–5): build a vessel as an ordered element stack plus
  attachments (from/to nodes, a selectable datum), all dimensions as editable parameters,
  standards tables with provenance, private company-standard table import.
- **Drawing tab** (Phase 6): GA sheet generated from the vessel model — elevation, plan, section, stacked
  tangent-line dimensions, nozzle schedule, BOM with weights, design-data block — exported as
  SVG, DXF and PDF.
- Agent-friendly: every vessel operation is a command reachable from the UI, JSON scripts, the
  CLI and MCP (`vessel.new`, `vessel.set`, `vessel.regenerate`, `drawing.generate_ga`, …).

## Quick start
Web: open <https://shubhs650.github.io/triya-cad-3d/app/> (once GitHub Pages is enabled, see
`triya/PROGRESS.md`), then *Install app* from the File menu.
Local:
```bash
rustup target add wasm32-unknown-unknown && cargo install trunk --version 0.21.14 --locked
cd triya/apps/triya-web && trunk serve        # http://127.0.0.1:8782
cargo run -p triya -- --sample                # desktop app
```
Headless: `cargo run -p triya-cli -- run triya/crates/triya-vessel/examples/cylinder.json --out cylinder.step`
(`triya-cli` is upstream's CLI with the `vessel.*` commands registered). For AI agents:
`cargo run -p triya-cli -- mcp` (MCP server on stdio, reports itself as `triya-cad-3d`).

## Repository layout
Upstream SolidCraft lives in `crates/`, `apps/`, `xtask/`, … and is modified only at the one-line
seams listed in [`triya/patches/PATCHES.md`](triya/patches/PATCHES.md). Everything Triya adds is
under [`triya/`](triya/): crates `triya-brand`, `triya-stdlib`, `triya-vessel`, `triya-drawing`,
`triya-ui`; apps `triya-web` (PWA), `triya` (desktop), `triya-cli` (headless + MCP); plan, ADRs and
docs. Read
[`triya/ARCHITECTURE.md`](triya/ARCHITECTURE.md) first.

## Limitations (beta)
No fillets on curved edges (knuckles are exact profile arcs; welds are drawn, not modelled);
DXF dimensions are exploded lines + text; heat exchangers and hex meshing are post-beta; EIL and
other proprietary standard sheets are not included — import your own tables (see
`triya/docs/standards-library.md`). Files are saved as `.solidcraft` on purpose: they open in
SolidCraft too.

## Brand assets
The Triya mark is drawn in code (`triya/crates/triya-brand/src/mark.rs`); the web icons are exports
of it, each with a row in [`ATTRIBUTION.md`](ATTRIBUTION.md). No SolidCraft or third-party artwork is
used for Triya's branding.

## Licence
MIT OR Apache-2.0, at your option — both for the upstream code and for Triya's additions. Data
tables carry per-row provenance; verify against the current standard edition before use. This
software performs no pressure-vessel code calculations and prints no compliance verdicts.
