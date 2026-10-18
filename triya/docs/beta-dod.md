# Beta definition of done (`triya-v0.1.0-beta`, target Sun 8 Nov 2026)

Tick every box with evidence (PR, screenshot path, measurement) in `PROGRESS.md`.

## A. Product
- [ ] Installable PWA named **Triya CAD 3D**, served over HTTPS (GitHub Pages or OET domain),
      works offline after first load; update to a new build verified with `web-offline-test`.
- [ ] About shows the attribution sentence (ADR-0006) with upstream version + sha.
- [ ] `bash triya/ci/check-patch-whitelist.sh v0.14.0` green: upstream diff limited to
      `triya/patches/PATCHES.md` rows and the owned root files; enforced in CI.

## B. Vessel modelling (three reference vessels, JSON in `triya/crates/triya-vessel/tests/reference/`)
- [ ] **Vessel A (vertical)**: skirt + base ring, 2:1 head, two courses (1600 ID), toriconical
      transition, 2200 ID course, ASME F&D top head; nozzles N1 radial w/ pad, N2 hillside,
      N3 tangential, N4 tilted 15°, M1 manway parallel-to-axis on top head, N5 head-radial-at-R
      on bottom head; two longitudinal lifting lugs; one stiffening ring; one limpet coil (12
      turns). Regenerates from JSON in **< 10 s in the browser** (i5-10th gen laptop); STEP AP242
      export opens in another CAD tool with all bodies.
- [ ] **Vessel B (horizontal)**: 1200 ID × 5000 TL–TL, two Klöpper heads, two saddles with wear
      plates, 4 nozzles incl. one on a head, two trunnion lugs.
- [ ] **Vessel C (vertical on legs)**: 900 ID, hemispherical heads, 4 pipe legs attached to the
      head, 3 nozzles, bracket lugs ×2, insulation ring.
- [ ] Every element/attachment type in `vessel-schema.md` §3–4 generates (or returns a clear
      "not supported in beta" for the ones in the Deferred list).
- [ ] Changing shell ID on course E2 propagates down the stack (stopping at the cone) and the
      vessel regenerates; element insertion re-stacks attachments per their `axial.ref`.
- [ ] Every edit is possible from the **Vessel panel** and from **MCP** `vessel.set` with identical
      JSON; `vessel.inspect` returns stations and flags.
- [ ] Boolean fallback engages on at least one captured kernel case without failing regenerate.
- [ ] Analytic-volume tests exist for every element type (0.1 %) and for 6 nozzle configurations.
- [ ] All `vessel.*` and `drawing.*` commands are in the sibling hostile-params test; 10 k fuzz cases, 0 panics.

## C. Standards
- [ ] B16.5 Cl 150–2500 WN/blind (Cl 150/300 also SO/LJ/SW) and B16.47 A/B WN/blind tables shipped
      with `source` + `verified`; no `illustrative` rows; validator green.
- [ ] Private table import works (sample *fictional* private set in `tests/`, EIL values **absent** from repo).
- [ ] BOM weights within **2 %** of handbook values for A, B, C.

## D. Drawing
- [ ] GA sheet for A, B, C: elevation (+ second elevation when needed), plan, section A–A with
      hatch, stacked TL-referenced dimensions, projections, nozzle schedule, BOM, design-data
      block, notes, revision block, OET title block.
- [ ] Exports: SVG (identical to preview), DXF (opens in a DXF viewer with 7 layers; arcs are
      arcs), PDF (or documented browser-print fallback).
- [ ] Every dimension/table value traceable to a vessel field id (golden-file tests).

## E. Engineering hygiene
- [ ] `cargo xtask ci` + `cargo xtask layers` + `cargo xtask wasm` green; `Triya CI` green.
- [ ] The saved `.solidcraft` opens in **unmodified upstream v0.14.0** with all geometry present.
- [ ] OPFS save/open round-trip keeps the vessel JSON and the sheet.
- [ ] ATTRIBUTION.md lists every Triya asset and derived table; SPDX headers everywhere in `triya/`;
      no GPL/LGPL/AGPL code read or linked; no commercial-CAD data.
- [ ] README: quick start, screenshots (ours), limitations, FAQ (why `.solidcraft`), licence.
- [ ] Measurements recorded: wasm size, regenerate times, time-to-model for a real datasheet.

## F. Reduced beta (fallback at day 15 if Phase 6 cannot land)
A–C + E without D; the Drawing panel shows "coming in 0.2" and the BOM/nozzle schedule are
exported as CSV instead.
