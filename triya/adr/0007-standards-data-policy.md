# ADR-0007 — Standards data: open tables with provenance; EIL and other proprietary sheets stay private

**Status:** accepted 2026-10-18

## Context
The vessel module needs ASME B16.5 / B16.47 flange dimensions, B36.10 pipe schedules, head
preset geometry (ASME F&D, 2:1, Klöpper DIN 28011, Korbbogen DIN 28013, 80:10, hemi,
toriconical) and support/attachment dimension tables. EIL standard sheets (7-12-00xx series:
saddles 0002, skirt base 0004, legs 0006/0007, brackets 0008, nozzle projection/pads
0013/0014, lifting lug 0024, insulation supports 0025/0033, nameplate 0027–0029, …) are issued
with tenders and are not public. ASME standards are copyrighted; vendor web tables reproduce
dimensions with occasional typos. Upstream forbids extracting data from commercial CAD products.

## Decision
1. **Repo ships only open or self-derived data**, each row carrying `source` (URL or document
   title + date) and `verified` ∈ {`two-source`, `single-source`, `derived`, `illustrative`}.
   Seed B16.5 Cl 150/300 and B36.10/B36.19 from `bd_warehouse` (Apache-2.0; ATTRIBUTION row).
   Hand-key Cl 400–2500 and B16.47 A/B from two independent public vendor tables; disagreements
   are logged in `docs/standards-library.md` and the row marked `single-source` until resolved.
   Head presets are formulas (public standards' geometric definitions), not tables. The
   skeleton's `illustrative` rows are placeholders; CI forbids them outside `tests/`.
2. **EIL (and client) sheets are never committed.** Every support/attachment carries
   `standardRef {org, number, rev}` + `sizeKey`; dimensions come from a `TableSet`. The repo
   ships open **preset tables** (ranges from public references, general handbook proportions) so
   the module works out of the box; `vessel.import_tables` loads a private JSON table set (OPFS in
   the browser, a file on desktop) that overrides presets. Private tables are never written into
   the design file — only `standardRef`/`sizeKey` are, so a design opened without the private
   tables regenerates with presets and shows a flag.
3. **No data from PV Elite, COMPRESS, AutoPIPE Vessel or any commercial CAD** (their public
   documentation may inform UX; their numbers, catalogs and files may not be used).
4. The UI shows provenance: rows marked `single-source` or `illustrative` display a warning
   badge; BOM/drawing notes state "flange dimensions per ASME B16.5/B16.47 — verify against the
   current edition".

## Consequences
- The open repo is useful to anyone; EIL-specific fidelity is a private add-on for EPC users
  (and a possible paid service later).
- `triya-stdlib` needs a table-format document and a validator (`docs/standards-library.md`).
- Attribution rows and licence review are part of every table PR.
