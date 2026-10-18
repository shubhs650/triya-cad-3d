# Triya CAD 3D — architecture

Triya = SolidCraft v0.14.0 (unchanged, frozen for the beta) + an **overlay** of Triya crates that
reach the CAD only through its command layer. Exact upstream types, ids and anchors are in
`docs/upstream-map.md`; what the kickoff skeleton already implements is in `SKELETON-REPORT.md`;
this file explains the shape and the rules.

## 1. The two halves

```
┌──────────────────────────── repository root ─────────────────────────────────────┐
│ UPSTREAM (read-only except seams)          TRIYA OVERLAY (triya/)                │
│ crates/geom     L0                         triya/crates/triya-brand   standalone  │
│ crates/sketch kernel render  L1            triya/crates/triya-stdlib  L0          │
│ crates/doc      L2                                                               │
│ crates/io       L3                                                               │
│ crates/engine   L4  ◀── triya_extra slot ── triya/crates/triya-vessel L4 (after engine) │
│                                             triya/crates/triya-drawing L4 (after vessel) │
│ crates/ui-egui  L5  ◀── panels by (tab,panel) ── triya/crates/triya-ui L5 (after ui-egui) │
│ crates/mcp      L5                                                               │
│ apps/solidcraft, solidcraft-cli, solidcraft-web   triya/apps/triya, triya-web, triya-cli (Exempt) │
│ xtask (ci, layers, assets, wasm, web-offline)     triya/ci, .github/workflows/triya-*      │
└──────────────────────────────────────────────────────────────────────────────────┘
```
Layering is enforced by `cargo xtask layers` through a hard-coded table (`xtask/src/layers.rs`
`TABLE`) with a *strictly-lower* rule plus explicit `INTRA_LAYER_ORDER` chains. The skeleton
registers `engine → triya-vessel → triya-drawing` and `ui-egui → triya-ui` as intra-layer
chains, so each Triya crate sits beside the upstream crate it extends. Nothing in `triya/`
imports `solidcraft-kernel` or `truck`; `triya-vessel` and `triya-drawing` import nothing from
egui; `triya-ui` is the only egui crate.

## 2. Seams (the only upstream edits) — source of truth: `patches/PATCHES.md`
Thirteen rows over ten files at kickoff, each marked `Modified by Triya CAD 3D` where the
syntax allows, enforced by `ci/check-patch-whitelist.sh`:
1. root `Cargo.toml` — workspace `members` += `triya/crates/*`, `triya/apps/*`; five
   `[workspace.dependencies]` entries.
2. `crates/engine/src/cmd/mod.rs` — `pub mod triya_extra;` and `triya_extra::extend(&mut v);`
   at the end of `command_specs()`; new file `crates/engine/src/cmd/triya_extra.rs` = the
   **runtime extension slot** (`register(&'static [CommandSpec])`, idempotent, duplicate ids
   refused). A static `triya_vessel::COMMANDS` reference is impossible (Cargo cycle: `triya-vessel`
   needs `engine::Session`), so the **Triya app shells** (`triya/apps/triya`, `triya-web`,
   `triya-cli`) call `register` at startup before the first `Session`.
3. `crates/ui-egui/src/toolbar.rs` — `TABS += "VESSEL"`, plus `ui.heading_vessel` in
   `assets/locales/{en,ja,zh}.json` (upstream's i18n tests require every tab in every complete
   catalog). *Avoidable*: place the commands in an existing tab via `CommandSpec.at(tab, panel)`
   — panel contents are registry-driven. Owner decision at T5.1.
4. `xtask/src/layers.rs` — `TABLE` rows and `INTRA_LAYER_ORDER` chains for the Triya crates.
5. `xtask/src/web_offline.rs` — prefer `$TRUNK_SOURCE_DIR/sw.js` so `triya/apps/triya-web/sw.js`
   (own cache prefix) gets this build's asset list.
6. `Cargo.lock` — new members, no new third-party crates.
7. root `CLAUDE.md` — the single import line `@triya/CLAUDE.md` (added by REPO-SETUP.md §5).

**Not seams** (handled in the overlay): brand strings — upstream's desktop app is a wrapper
around `SolveApp`, so `triya/apps/triya` sets the window title, draws About and the Vessel/Drawing
panels **around** `SolveApp::ui` with the pub API (`frame::window`, `theme::Tokens`,
`app.run/start/set_status/session`); the web shell is a copy (`index.html`, manifest, `sw.js`,
icons, `Trunk.toml`); MCP `serverInfo.name` (`crates/mcp/src/server.rs:145`) is set by the
`triya-cli` wrapper if the constructor allows, otherwise it becomes one more seam row. The desktop
update checker polls `bherbruck/solidcraft` releases unless `Services::http_get` is `None` — the
Triya shells pass `None`.

## 3. Vessel module data flow (ADR-0003)
```
 triya.vessel/0.1 JSON ──► model::VesselDoc ──► eval::resolve ──► Resolved {stations, radii(z), frames,
      ▲                                                             tables resolved, warnings}
      │ vessel.set (JSON-pointer patch)                                   │
      │ UI panel / MCP / script                                           ▼
      │                                                      emit::plan ──► Vec<CommandCall>  ($ref:<step>.<field>
      │                                                                     substitution between steps)
      │                                                                     sketch.create {explicit plane}
      │                                                                     sketch.rectangle.two_point / lines / arcs
      │                                                                     sketch.finish → solid.revolve {axis:"Z", angle:"360 deg"}
      │                                                                     solid.extrude cut/join, pattern.circular,
      │                                                                     solid.coil, parameters.set, selection.delete
      │                                                                     │
 persist: Document.metadata["in.openengtools.triya.vessel"]                 ▼
      ◀── vessel.regenerate: one undo step (nested (spec.run)(s,p) calls inside our run)
                                                   │
                                                   ▼
                                     validate: measure each body, compare analytic volume,
                                     on failure → separate body + flags.boolean_failed
```
- **Element stack** → each element is one revolved profile (inside + outside curves) → one body;
  adjacent bodies are joined (combine) when the kernel accepts it, otherwise kept as a component
  of bodies — drawings and BOM do not care.
- **Attachments** → frame (position + axis from host, axial ref, angle) → cut host (set-in: neck
  OD; set-on: neck ID) → join neck, pad (annulus conforming to host via offset-cut), flange
  (revolved WN profile + bolt-hole pattern) → validate.
- **Parameters**: every dimension is `parameters.set V<id>_<field>`; sketches reference the
  parameter names so upstream's parameter table can drive them too.
- **One undo step**: the skeleton calls other specs' `(spec.enabled)(s)` / `(spec.run)(s, p)`
  from inside `vessel.regenerate` (upstream's `selection.delete::sub` pattern); the outer
  `Session::execute` makes the whole thing one undo entry (asserted by test). Alternative
  scratch-and-accept pattern (`s.scratch()` … `s.accept_computed(scratch)`) is available if a
  partially failing plan must never touch the live design.
- **Stable references**: faces/edges by upstream's naming (`F<feature>:<side>:<curve>`,
  `doc/src/naming.rs`) or by `[x,y,z]` points computed from the resolved stations.
- **Sketch planes**: upstream's named XZ plane has sketch *y* = **−Z**; the emitter passes an
  explicit plane `{origin:[0,0,0], x_dir:[1,0,0], y_dir:[0,0,1]}` so sketch *y* = +Z, with tests.
- **Persistence** (ADR-0005): `document.metadata.set` under `in.openengtools.triya.vessel`
  (≤ 64 KiB per key → chunked above 60 KiB). The regenerated geometry is always present as plain
  features, so upstream opens the file.

## 4. Drawing module data flow (ADR-0004)
```
Resolved + measured weights ──► vessel_views (analytic elevation/plan/section) ──► Sheet
                               dims (rule-based, FieldId-sourced)                   │
                               tables (design data, nozzle schedule, BOM, notes)    ▼
                                                              Plotter trait ──► SVG | DXF | PDF
                                                                   │
                                                  egui preview paints Edges directly (epaint)
```
Sheet stored next to the vessel JSON (`in.openengtools.triya.sheet`); `drawing.export` downloads
in the browser (Rust `web-sys`), writes a file on desktop. **DXF**: upstream exposes
`solidcraft_io::{DxfLayer, DxfEntity, DxfShape, write_dxf_drawing}` (`crates/io/src/dxf_out.rs`);
the DXF `Plotter` targets it first and adds the `dxf` crate only for missing entities
(TEXT/MTEXT, dashed linetypes, ELLIPSE) — decide in T6.6.

## 5. UI (thin, hosted by the app shells)
`SolveApp` exposes `ui`, `run`, `start`, `set_status`, `session`, `frame::window`,
`theme::Tokens` as pub API. The Triya shells call `SolveApp::ui` and draw the Vessel/Drawing side
panels and the About dialog *around* it; the code lives in `triya-ui` (`VesselPanel::show(app, ctx)`
in the skeleton). Panels read engine state, build a `vessel.set` patch, and call
`app.run("vessel.set", …)` then `app.run("vessel.regenerate", {})`; they never compute geometry
and never call `app.start`. Colours from `theme::Tokens`; icons drawn in code. Everything a
panel can do, MCP can do with the same JSON — the UI is a convenience, the commands are the API.

## 6. Web/PWA
`triya/apps/triya-web` is upstream's web shell with Triya branding: trunk build to
`dist/triya-web`, `wasm32-unknown-unknown`, single-threaded (upstream limitation), OPFS for
designs and private table sets, service worker with the `triya-app` cache prefix and SHA-verified
updates (upstream's `xtask web-offline` hook via seam 5), manifest `id` distinct from upstream so
both can be installed side by side; `index.html` keeps upstream's element ids because the shared
`loading.css` styles them. Heavy regenerates block the UI thread: show a progress overlay, keep
the beta vessel size within the DoD.

## 7. Agent/MCP surface
`triya-cli mcp` = upstream MCP with Triya commands registered and `serverInfo.name =
"triya-cad-3d"`; vessel and drawing commands appear in `list_commands` automatically because they
are registered `CommandSpec`s. Typical agent flow: `execute vessel.new` → `execute vessel.set
{doc}` → `execute vessel.regenerate` → `inspect_design`/`measure` → `execute drawing.generate_ga`
→ `export`.

## 8. Testing strategy
- Unit: evaluator (stations, head depths vs DIN/ASME worked values), frames, table validator.
- Engine tests (in `triya-vessel`, because upstream's test binaries cannot see runtime-registered
  commands): analytic volume per element and per nozzle configuration; regenerate idempotence;
  undo; persistence round-trip; sibling hostile-params test for every `vessel.*`/`drawing.*`.
- Golden files: SVG/DXF sheets for reference vessels A, B, C.
- `kernel-cases/`: captured kernel failures with the expected fallback → regression suite for the
  upstream kernel swap.
- Upstream gates unchanged: `cargo xtask ci` (fmt, assets, licences, lists, layers, clippy, tests,
  wasm, web-offline-test).

## 9. Non-goals (beta)
Code calculations (ASME/EN) and compliance verdicts; heat exchangers; generic 2D drafting of
arbitrary parts; fillets on curved edges; multi-user/cloud; hex meshing (post-beta P3/P4, in
the ClearFEA worker, consuming the vessel feature tree).

## 10. Future convergence
If upstream accepts a brand-constants module and the command-registration slot, seams 2–3
disappear. If upstream ships 2D drawings (M11), `triya-drawing` (feature `drawing2d`) becomes an
adapter placing vessel annotations on upstream sheets. If upstream swaps the kernel, Triya
re-runs `kernel-cases/` and enables curved fillets — no code change in the generator.
