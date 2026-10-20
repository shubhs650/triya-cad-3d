# Beta definition of done (`triya-v0.1.0-beta`, target Sun 8 Nov 2026)

Tick every box with evidence (PR, screenshot path, measurement) in `PROGRESS.md`.

## A. Product
- [ ] Installable PWA named **Triya CAD 3D**, served over HTTPS (GitHub Pages or OET domain),
      works offline after first load; update to a new build verified with `web-offline-test`.
      *Open: built and offline-tested in CI (#6, Triya CI web-shell); not deployed — GitHub Pages is not enabled (owner).*
- [x] About shows the attribution sentence (ADR-0006) with upstream version + sha.
      *#8 (Triya About window); app bar/start page/Help menu say Triya since Phase 7 (seams 16–19).*
- [x] `bash triya/ci/check-patch-whitelist.sh v0.14.0` green: upstream diff limited to
      `triya/patches/PATCHES.md` rows and the owned root files; enforced in CI.
      *Triya CI "Seam whitelist" job on every PR.*

## B. Vessel modelling (three reference vessels, JSON in `triya/crates/triya-vessel/tests/reference/`)
- [ ] **Vessel A (vertical)**: skirt + base ring, 2:1 head, two courses (1600 ID), toriconical
      transition, 2200 ID course, ASME F&D top head; nozzles N1 radial w/ pad, N2 hillside,
      N3 tangential, N4 tilted 15°, M1 manway parallel-to-axis on top head, N5 head-radial-at-R
      on bottom head; two longitudinal lifting lugs; one stiffening ring; one limpet coil (12
      turns). Regenerates from JSON in **< 10 s in the browser** (i5-10th gen laptop); STEP AP242
      export opens in another CAD tool with all bodies.
      *Open: geometry complete (#18–#22, Phase 3 exit job). Chrome: shell 6.3 s, separate nozzles 87 s, joined 24 min — the < 10 s target is not met for the full vessel (owner decision 2026-10-20). STEP in another CAD tool: owner check.*
- [x] **Vessel B (horizontal)**: 1200 ID × 5000 TL–TL, two Klöpper heads, two saddles with wear
      plates, 4 nozzles incl. one on a head, two trunnion lugs.
      *Phase 4 exit job (#23–#27).*
- [x] **Vessel C (vertical on legs)**: 900 ID, hemispherical heads, 4 pipe legs attached to the
      head, 3 nozzles, bracket lugs ×2, insulation ring.
      *Phase 4 exit job (#23–#27).*
- [x] Every element/attachment type in `vessel-schema.md` §3–4 generates (or returns a clear
      "not supported in beta" for the ones in the Deferred list).
      *`every-kind.json` tests; deferred kinds return "not built yet" warnings (#23–#26).*
- [x] Changing shell ID on course E2 propagates down the stack (stopping at the cone) and the
      vessel regenerates; element insertion re-stacks attachments per their `axial.ref`.
      *`phase5_exit.rs` (#29).*
- [x] Every edit is possible from the **Vessel panel** and from **MCP** `vessel.set` with identical
      JSON; `vessel.inspect` returns stations and flags.
      *`phase5_exit.rs` (#29).*
- [x] Boolean fallback engages on at least one captured kernel case without failing regenerate.
      *`nozzle_fallback.rs`, kernel-cases/ (#20–#21).*
- [x] Analytic-volume tests exist for every element type (0.1 %) and for 6 nozzle configurations.
      *Phases 2–4 tests (#12–#27).*
- [x] All `vessel.*` and `drawing.*` commands are in the sibling hostile-params test; 10 k fuzz cases, 0 panics.
      *`vessel_commands_never_panic_on_hostile_params`, `drawing` commands test (#30); fuzz 10,000 cases, 0 panics (`fuzz.rs`, Phase 7).*

## C. Standards
- [x] B16.5 Cl 150–2500 WN/blind (Cl 150/300 also SO/LJ/SW) and B16.47 A/B WN/blind tables shipped
      with `source` + `verified`; no `illustrative` rows; validator green.
      *#18–#19, #23 (`triya-stdlib` validator).*
- [x] Private table import works (sample *fictional* private set in `tests/`, EIL values **absent** from repo).
      *#26 (`fictional-private-supports.json`).*
- [x] BOM weights within **2 %** of handbook values for A, B, C.
      *#27, Phase 4 exit job.*

## D. Drawing
- [x] GA sheet for A, B, C: elevation (+ second elevation when needed), plan, section A–A with
      hatch, stacked TL-referenced dimensions, projections, nozzle schedule, BOM, design-data
      block, notes, revision block, OET title block.
      *#30 (golden files, screenshots/phase6).*
- [ ] Exports: SVG (identical to preview), DXF (opens in a DXF viewer with 7 layers; arcs are
      arcs), PDF (or documented browser-print fallback).
      *Done in #30 except the owner's DXF-viewer check of `vessel-a-ga.dxf` (open).*
- [x] Every dimension/table value traceable to a vessel field id (golden-file tests).
      *#30 golden-file tests.*

## E. Engineering hygiene
- [x] `cargo xtask ci` + `cargo xtask layers` + `cargo xtask wasm` green; `Triya CI` green.
      *CI + Triya CI green on every merged PR.*
- [x] The saved `.solidcraft` opens in **unmodified upstream v0.14.0** with all geometry present.
      *`triya/ci/upstream-roundtrip.sh` (CI job "Opens in unmodified upstream v0.14.0"), Phase 7.*
- [ ] OPFS save/open round-trip keeps the vessel JSON and the sheet.
      *Save/open of the same bytes verified natively (`roundtrip.rs`, Phase 7); the in-browser OPFS check waits for the deployed PWA (owner).*
- [x] ATTRIBUTION.md lists every Triya asset and derived table; SPDX headers everywhere in `triya/`;
      no GPL/LGPL/AGPL code read or linked; no commercial-CAD data.
      *`cargo xtask assets` + Triya CI SPDX step.*
- [x] README: quick start, screenshots (ours), limitations, FAQ (why `.solidcraft`), licence.
      *Phase 7.*
- [ ] Measurements recorded: wasm size, regenerate times, time-to-model for a real datasheet.
      *wasm size and regenerate times recorded (PROGRESS.md "Measurements"); time-to-model for a real datasheet waits on the owner.*

## F. Reduced beta (fallback at day 15 if Phase 6 cannot land)
A–C + E without D; the Drawing panel shows "coming in 0.2" and the BOM/nozzle schedule are
exported as CSV instead.
