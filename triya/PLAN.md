# Triya CAD 3D — development plan (beta window 17 Oct → 14 Nov 2026)

Day 1 = Sat 17 Oct 2026. **Beta target: Sun 8 Nov (day 23).** Reduced-beta checkpoint: Sat 31 Oct
(day 15). Hard end of window: Sat 14 Nov (day 29). One owner, Claude Code agents; one task = one
PR; tasks marked ∥ may run in parallel subagents (AGENTS.md §2). Every task names its
acceptance test; a task is done when that test passes in CI and PROGRESS.md says so.

Legend: **AT** = acceptance test. Estimates are agent-hours (wall-clock, including CI).
Starting point: the kickoff skeleton (`SKELETON-REPORT.md`) — six crates compile, 23 tests pass,
`vessel.new/set/regenerate/inspect` exist, cylinders regenerate as one undo step, persistence in
`Document.metadata` works, 13 seam rows in PATCHES.md.

---

## Phase 0 — Fork, freeze, skeleton (Sun 18 – Tue 20 Oct, days 2–4)

| id | task | AT | est |
|---|---|---|---|
| T0.1 | Repo prepared per `REPO-SETUP.md` §1–7 (owner) | `git log --oneline -1 main` shows the kit commit on top of v0.14.0; whitelist script green | 2 h |
| T0.2 | Workspace builds on the owner's laptop (i5-10th gen, 16 GB) and in CI with the pinned toolchain | `cargo xtask ci` green locally and in Actions | 3 h |
| T0.3 | Skeleton completed: `triya-ui` tests run; `triya/apps/triya-cli` wrapper (registers `vessel.*`, delegates to upstream CLI incl. `mcp`); wasm32 target installed and `trunk build --release` of `triya/apps/triya-web` succeeds with the `web-offline` seam producing `sw.js` | `cargo test -p triya-ui` green; `triya-cli commands` lists `vessel.*`; `cargo xtask web-offline-test --site dist/triya-web` green | 5 h |
| T0.4 | Seams verified against PATCHES.md; markers present; `0001-seams.patch` refreshed; whitelist script wired into `Triya CI` | `check-patch-whitelist.sh v0.14.0` green in CI | 2 h |
| T0.5 | Headless smoke: `triya-cli run triya/crates/triya-vessel/examples/cylinder.json --out cyl.step` exports STEP; the skeleton's analytic-volume test kept as the template for all element tests | STEP file produced; test `cylinder_volume_matches_analytic` green | 2 h |
| T0.6 | `triya-pages.yml` deploys the (still mostly upstream-looking) web app to GitHub Pages | URL loads, installs as PWA, works offline after reload; wasm size recorded | 2 h |

Exit: an installable PWA, built from our fork, with one vessel command working headlessly.

## Phase 1 — Rebrand + PWA shell (Wed 21 – Thu 22 Oct, days 5–6)

| id | task | AT | est |
|---|---|---|---|
| T1.1 | `triya/apps/triya-web` finished: `index.html` (title, og, `data-bin`), `manifest.webmanifest` (name, short_name, id, theme), icons **drawn in code** (`triya-ui::icons`, rasterised at build time with ATTRIBUTION rows for any generated files), `sw.js` cache prefix `triya-`, `Trunk.toml` dist `dist/triya-web` | `cargo xtask web-offline-test --site dist/triya-web` green; Lighthouse "installable" | 5 h |
| T1.2 | `triya-brand` wired in the Triya shells (no new seam): window title, About dialog drawn around `SolveApp::ui` with `about_text()` (upstream name, version, sha, licence, "not affiliated"), update checker `http_get = None`, MCP `serverInfo.name = "triya-cad-3d"` via `triya-cli` (seam only if the server constructor has no name parameter) | snapshot of About shows the sentence; `triya-cli mcp` handshake reports the name | 3 h |
| T1.3 | Desktop shell `triya/apps/triya`: thin wrapper around `SolveApp` that registers commands and sets the title — **not** a copy of upstream's 2,700-line app (native crash/frame code stays upstream's); if a thin wrapper is impossible, document and skip for beta | `cargo run -p triya` opens with the Triya title | 3 h |
| T1.4 | Root `README.md` (Triya), `NOTICE`, ATTRIBUTION rows for icons/screenshots, SPDX headers everywhere in `triya/` | `cargo xtask assets` green; `reuse lint` green if installed | 2 h |
| T1.5 ∥ | Issue texts drafted for upstream (REPO-SETUP §9) in `triya/docs/upstream-proposals.md` | file exists; owner posts | 1 h |

Exit (milestone **M1**): an installable "Triya CAD 3D" PWA that is upstream v0.14.0 under the hood.

## Phase 2 — Vessel core (Fri 23 – Sun 25 Oct, days 7–9)

| id | task | AT | est |
|---|---|---|---|
| T2.1 | `triya.vessel/0.1` model completed per `docs/vessel-schema.md` (skeleton has the shape; fill every kind/field); `schemars` JSON schema generated and checked in | round-trip test on the three reference vessels in `docs/beta-dod.md` | 4 h |
| T2.2 | Evaluator: datum handling (bottom TL / bottom seam / skirt base / plant elevation; horizontal: left TL), stations `z_from/z_to`, `radius_at(z)`, ID/OD basis, straight-flange placement, inheritance of diameter down the stack "stopping at cones" | unit tests: stations for a 7-element vertical vessel match hand calc; head depths match DIN worked examples within 0.5 mm (Klöpper Da 1000/s 10 → 188.95; Korbbogen → 248.65; skeleton currently 188.89 / 248.26 from the exact geometry vs DIN's linear fits — document the difference) | 5 h |
| T2.3 | Emitter: 2:1 ellipsoidal (true ellipse + UG-32 0.9D/0.17D approximation), torispherical (ASME F&D, Klöpper, Korbbogen, 80:10), hemispherical, toriconical (knuckle arc), conical (half-apex warning > 30°), welded flat cover — each as a half-profile sketch revolved 360°, inside and outside curves, all dims as `V<id>_*` parameters (cylinder already done) | engine tests: volume of each element within 0.1 % of analytic (`docs/vessel-schema.md` §7) | 10 h |
| T2.4 | Body flange (WN profile from stdlib + bolt-hole circular pattern) and skirt (cylinder + base ring annulus) | volume tests; bolt count equals table | 4 h |
| T2.5 | Regenerate hardening: `triya:vessel-group` tagging so regenerate replaces only its own features (skeleton deletes previous features via `selection.delete`); chunked metadata above 60 KiB; `vessel.inspect` returns resolved stations and flags | test: regenerate twice → feature count unchanged; undo once → vessel gone; save/open round-trip keeps the JSON; 70 KiB document round-trips | 5 h |
| T2.6 | Hostile-params coverage for all `vessel.*` commands; caps on element count (≤ 200) and attachment count (≤ 500) | sibling hostile test includes every vessel command | 2 h |

Exit: a vertical vessel (skirt, 2:1 head, two courses, cone, torispherical head) regenerates from JSON and exports STEP.

## Phase 3 — Standards library + nozzles (Mon 26 – Wed 28 Oct, days 10–12)

| id | task | AT | est |
|---|---|---|---|
| T3.1 ∥ | `triya-stdlib` tables: B16.5 Cl 150/300 WN/SO/BL/LJ/SW from bd_warehouse (re-keyed, Apache-2.0 row in ATTRIBUTION), B36.10 schedules; delete the skeleton's `illustrative` rows | table tests: row counts; spot values (e.g. 4" Cl 150 WN OD 228.6, 8 × ⅝" bolts); every row has `source` + `verified`; CI's illustrative check green | 5 h |
| T3.2 ∥ | B16.5 Cl 400–2500 and B16.47 A/B WN + blind, hand-keyed from two public vendor tables, `verified: "two-source"`; mismatches logged in `docs/standards-library.md` | cross-check test: rows marked two-source have both sources recorded | 6 h |
| T3.3 | Nozzle frames: radial, hillside (offset), tangential, tilted, parallel-to-axis on heads, head-radial-at-R; neck from pipe table; set-in/set-on; projection from vessel CL or from OD / insulation OD; pad (annulus conforming to host) | unit tests of frame transforms; engine tests: volume after cut+join within tolerance for 6 configurations on cylinder and head | 10 h |
| T3.4 | Flange on nozzle (WN/SO/LWN/blind) from stdlib; manway with davit *reference* (no davit geometry in beta) | volume test; bolt pattern count | 3 h |
| T3.5 | Boolean validation + fallback (separate body + flag) and `kernel-cases/` capture | test that a deliberately degenerate case (equal-radius crossing cylinders) engages the fallback without failing regenerate | 3 h |

Exit: six nozzle configurations on the reference vessel; failures degrade gracefully.

## Phase 4 — Supports + attachments (Thu 29 – Sat 31 Oct, days 13–15)

| id | task | AT | est |
|---|---|---|---|
| T4.1 | Saddle (web, ribs, base plate, wear plate sector) as a component at a station on a horizontal vessel; two saddles with spacing/`distance_from_TL` | volume test; wear plate conforms (gap test ≤ 0.01 mm) | 5 h |
| T4.2 | Legs (pipe, angle) with circular pattern and optional head-moulded top; bracket lugs (gusset + plates); anchor chairs on skirt base ring (pattern) | volume tests; pattern count equals input | 6 h |
| T4.3 | Stiffening ring (bar/angle section at station), limpet coil (`solid.coil`/sweep of half-pipe at `Ro + pipe_r`, pitch, start/end angles), lifting lugs (longitudinal/circumferential plate with hole), insulation support rings, clips | volume tests; coil length within 1 % of π·D·n/cos(helix) | 7 h |
| T4.4 | `standardRef` + `sizeKey` on every support/attachment; open preset tables (saddle, legs, lugs ranges); **private table import** (`vessel.import_tables` from a user JSON file, OPFS in browser) | test: importing a sample *fictional* private table changes the generated saddle dimensions; repo contains no EIL values | 4 h |
| T4.5 | BOM and weights: per-element metal weight from `measure` with handbook cross-check column (F&D 0.9286·D² area; Klöpper 0.99·da² + π·h1·da), nozzle/flange weights from tables, totals (empty/operating/test placeholder) | weights within 2 % of handbook for the reference vessels | 4 h |

Exit: reference vessels B and C regenerate whole with every attachment built, vessel A builds its supports, and every vessel's BOM is within 2 % of the handbook (CI jobs "Phase 3 exit" and "Phase 4 exit").

**Day 15 checkpoint (Sat 31 Oct)**: if Phases 2–4 are done, continue. If not, the *reduced beta*
is Phases 0–5 without Phase 6 (drawing); decide and record in PROGRESS.md.

## Phase 5 — Vessel UI (Sun 1 – Tue 3 Nov, days 16–18)

| id | task | AT | est |
|---|---|---|---|
| T5.1 | Ribbon placement decision: **keep** the skeleton's `VESSEL` tab seam (+ 3 locale lines) **or** drop it and place commands in an existing tab via `CommandSpec.at(tab, panel)` (zero UI edits) — owner picks; panels: New vessel, Elements, Attachments, Supports, Regenerate, Export BOM | snapshot shows the commands; all buttons call `app.run("vessel.*", …)` only | 4 h |
| T5.2 | Stack editor side panel (drawn by the Triya app shells around `SolveApp::ui`, code in `triya-ui`): ordered elements, add/insert/delete/move, inheritance (only type/diameter/thickness/length mandatory), datum selector that rewrites elevations, live station readout | screenshot review; `vessel.set` JSON-pointer patches behind every edit | 10 h |
| T5.3 | Attachment panel: host picker synced with 3D selection, axial ref (datum vs host-from-node), angle with 22.5° (shell) / 5° (head) snapping toggle, frame type, nozzle fields (NPS, class, type, projection, pad), standardRef/sizeKey pickers, flags display (boolean_failed, unverified table) | screenshot review; adding a nozzle through the UI equals adding it via MCP (same JSON) | 10 h |
| T5.4 | Plan-view orientation widget (circle with azimuths, 0° reference and rotation sense settings) | screenshot review | 4 h |
| T5.5 | Usability pass with one real datasheet (owner provides, anonymised): time-to-model recorded | PROGRESS.md "Measurements": < 20 min for a 10-nozzle vessel | 3 h |

Exit (milestone **M2 — reduced beta**): everything editable from the panel and from MCP; shell ID change propagates downstream.

## Phase 6 — Fabrication drawing generator (Wed 4 – Sat 7 Nov, days 19–22)

| id | task | AT | est |
|---|---|---|---|
| T6.1 ∥ | `triya-drawing` model finished per `docs/drawing-model.md` (skeleton has model + `Plotter` + `SvgPlotter`); egui preview (epaint) of the sheet | golden SVG of a rectangle+dimension+table | 5 h |
| T6.2 | Vessel elevation view: shell silhouettes + tangent lines, head profile arcs, nozzles (neck silhouettes + flange rectangle, dashed where inside shell), saddles/skirt/legs as outlines, seams | golden SVG for the three reference vessels; visual review | 8 h |
| T6.3 | Plan view (circles + nozzle stubs at azimuth, support footprint) and one section view (profile loop + 45° hatch clipped even-odd) | golden SVG | 5 h |
| T6.4 | Rule-based dimensions: TL–TL, OAL, nozzle CLs chained from datum TL in stacked rows with collision nudge, projections, ID/OD + thickness notes, support positions, azimuth angles on plan | golden SVG; every driving dimension traceable to a vessel field id | 6 h |
| T6.5 | Tables: design-data block, nozzle schedule, BOM (from T4.5), notes, revisions; SVG title-block template with placeholders (OET default template) | golden SVG; table values equal model values | 4 h |
| T6.6 ∥ | DXF backend (upstream `solidcraft_io::write_dxf_drawing` first; `dxf` crate only if entities are missing; layers `OUTLINE/HIDDEN/CENTER/DIM/HATCH/TEXT/TITLE`, dims exploded) and PDF (`printpdf`; fallback browser print via `web-sys`) | DXF opens in a viewer with layers (owner check); PDF has 1 page A1/A3 as configured | 6 h |
| T6.7 | `drawing.generate_ga`, `drawing.export {svg,dxf,pdf}` commands + Drawing panel; browser download path | commands in the sibling hostile test; snapshot | 4 h |

Exit: GA sheet for the reference vessels in SVG, DXF and PDF (or printed SVG).

## Phase 7 — Beta hardening (Sat 7 – Sun 8 Nov, days 22–23)

| id | task | AT | est |
|---|---|---|---|
| T7.1 | Fuzz vessel JSON through the hostile harness (random/degenerate values); caps enforced | 0 panics over 10 k cases | 3 h |
| T7.2 | Performance: regenerate time for reference vessel A in browser (target < 10 s), wasm size recorded, UI freeze acceptable with a progress indicator | numbers in PROGRESS.md | 3 h |
| T7.3 | Round trips: OPFS save/open; `.solidcraft` opens in unmodified upstream v0.14.0 with geometry intact; STEP opens in another CAD tool (owner) | checklist in `docs/beta-dod.md` ticked | 3 h |
| T7.4 | Docs: README with screenshots (our own), quick-start, limitations list (no curved fillets, dims exploded in DXF, EIL tables private), licence/attribution section | review | 3 h |
| T7.5 | Tag `triya-v0.1.0-beta`, deploy Pages, announce internally | tag + live URL | 1 h |

**Beta (day 23, Sun 8 Nov 2026).**

## Buffer (Mon 9 – Sat 14 Nov, days 24–29)
Absorbs kernel surprises, UI rework, PDF failure. If unused: arm the daily sync (`upstream-sync.yml`
cron), run `prompts/08-first-upstream-merge.md`, post the two upstream issues.

---

## Post-beta phases

| phase | when | scope |
|---|---|---|
| P1 | mid-Nov – Dec 2026 | First upstream merge (dedicated 2-day task), then daily sync live; upstream PRs for brand constants + registry hook; DXF true DIMENSION entities; nozzle weld-detail sheet library keyed to public weld-joint categories; shell plate/seam layout with stagger rules (≥ 5t or 300 mm) and nozzle-cutout clearance; EIL tables completed privately; invite 5–10 engineers (beta feedback via GitHub issues) |
| P2 | Jan 2027 | Generic mesh-based HLR in Rust (crease + silhouette edges from tessellation, BVH visibility, arc re-fit) for arbitrary parts; section/detail views; decide `drawing2d` adapter once upstream M11 status is known |
| P3 | Feb 2027 | Phase-1 hex mesher: quad-mesh the revolve profile, revolve to C3D20R/C3D8I hexes, pole handling, own `.inp` writer → hand-off to ClearFEA's CalculiX worker |
| P4 | Mar 2027 | Nozzle O-grid block templates; Gmsh as a server-side subprocess for quads/tets; heat-exchanger elements (tubesheet, shell/tube nozzles, baffles, channel) on the stable vessel model |
| P5 | when upstream lands the SMLib/solidcore kernel | Re-run the vessel oracle + `kernel-cases/` against the new kernel; enable curved-edge fillets and pad fillets; remove fallbacks that are no longer needed |

## Milestones at a glance
- **M1** Thu 22 Oct — branded PWA on Pages, one vessel command headless.
- **M2** Tue 3 Nov — reduced beta: full vessel modelling with UI + MCP.
- **Beta** Sun 8 Nov — M2 + GA drawing export.
- **Window end** Sat 14 Nov.
