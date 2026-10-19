# Standards library (`triya-stdlib`)

Policy: ADR-0007. This file defines the table format, sources and verification rules. The
kickoff skeleton's `illustrative` placeholder tables were replaced in T3.1 by
`data/asme-b16.5.json` and `data/asme-b36.10.json`; CI forbids `illustrative` rows outside `tests/`.

## 1. Table set format (`data/*.json` and private imports)
One file per standard. The table carries the standard's provenance; every row carries its own
`source` list and `verified` grade (T3.1 shape, `triya-stdlib::tables`):
```jsonc
{
  "source": "ASME B16.5 (metric tables) Class 150 and 300: values re-keyed from bd_warehouse 0.4.0 …",
  "note": "Lengths in mm. thickness = tf without the raised face; …",
  "standard": { "org": "ASME", "number": "B16.5", "rev": "2020" },
  "rows": [
    { "nps": "4", "class": "150", "kind": "wn", "od": 230.0, "bolt_circle": 190.5, "bolt_count": 8,
      "bolt_size": "5/8", "bolt_hole": 19.1, "thickness": 22.3, "hub_od": 135.0, "hub_weld_d": 114.3,
      "raised_face_d": 157.2, "raised_face_h": 2.0, "length_through_hub": 75.0,
      "source": ["bd_warehouse 0.4.0 flange.py (gumyr, Apache-2.0), …", "wermac.org/flanges/dim_wnflg_b16-5_4.html, …"],
      "verified": "two-source" }
  ]
}
```
Flange rows: `nps` (canonical `1-1/4` form), `class`, `kind` (`wn`·`so`·`blind`·`lj`·`sw`·`lwn`),
`od`, `bolt_circle`, `bolt_count`, `bolt_size` (inch fraction or `M{n}`), `bolt_hole`, `thickness`
(ring `tf` **without** the raised face), `hub_od` (0 for blinds), `hub_weld_d` (WN `A`),
`raised_face_d`, `raised_face_h` (B16.5 metric: 2 mm Cl 150/300, 7 mm Cl 400+), `length_through_hub`
(also without the raised face), `bore` (SO/SW/LJ), `socket_depth` (SW), `note`. Pipe rows: `nps`,
`dn`, `od`, `schedule` (`STD`·`XS`·`XXS`·`40`·`80`…), `wall`.
`verified` ∈ `two-source` · `single-source` · `derived` (computed from other rows/formula) ·
`illustrative` (placeholder, never shipped). Rows may carry `note` (e.g. "sources disagree on
hub_len: 76 vs 76.2; kept 76").

Private sets (EIL etc.) use the same format; `TableSet::with_private` (behind `vessel.import_tables`,
T4.4) loads them at runtime (OPFS/file) and searches them before the shipped tables, so they
override shipped rows of the same key. Private rows without `verified` count as `unverified`.

## 2. Shipped tables (beta)
| name | content | seed source | target `verified` |
|---|---|---|---|
| `asme-b16.5-{wn,so,blind,lj,sw}` Cl 150/300 NPS ½–24 | OD, BC, bolts, thk, hub, RF | bd_warehouse (Apache-2.0) re-keyed | two-source after one vendor cross-check |
| `asme-b16.5-{wn,blind}` Cl 400/600/900/1500/2500 | same | two public vendor tables, hand-keyed | two-source |
| `asme-b16.47a-{wn,blind}`, `asme-b16.47b-{wn,blind}` NPS 26–60 Cl 150–900 | same | two public vendor tables | two-source |
| `asme-b36.10` schedules | OD, wall per NPS/sch | bd_warehouse / `fluids` (MIT) | two-source |
| `asme-b36.19` stainless schedules 5S/10S/40S/80S | OD, wall per NPS/sch | wermac.org + Wikipedia NPS tables | two-source |
| `lwn-generic` | LWN hub lengths by NPS/class | vendor catalogs vary → flagged | single-source |
| `heads` | presets as formulas (code, not table) | public standard definitions | — |
| `saddle-preset` | width/height/web/base plate ranges by OD (600–3000) | public handbook proportions; BS 5276-2 size ranges (described, not copied) | derived |
| `leg-preset`, `bracket-preset`, `lug-preset`, `chair-preset`, `ring-preset`, `clip-preset` | typical proportions by vessel OD/weight class | handbook proportions | derived |
| `materials` | density and BOM names | public | — |

## 3. Keys
- Flange: `size_key = "{nps}/{class}/{kind}"` (e.g. `4/150/wn`, `1-1/4/300/so`), the key the vessel
  model already used in Phase 2; the standard is chosen by the table (`flange_ref.standard` /
  `flange.standard` = `B16.5`, `B16.47A`, `B16.47B`). NPS spellings `1 1/4`, `1.1/4`, `1-1/4` and
  `1¼` all resolve to `1-1/4`.
- Supports/attachments: free `size_key` strings matched against the set's `sizeKey` column
  (e.g. EIL `"7-12-0024/LL-25"`); presets use `"OD1600"` style keys with nearest-lower lookup.

### 3.1 Presets and private tables (T4.4)
Support-preset tables (`data/support-presets.json`, `triya-stdlib::support`) hold rows
`{kind, size_key, fields, source, verified, note?}`: `kind` is the attachment kind (`saddle`,
`leg`, `lifting_lug`, …), `fields` the attachment fields the row sets, in the vessel JSON's own
names (an object such as `base_plate` replaces the attachment's whole). A row may not set `id`,
`kind`, `host`, `size_key`, `standard_ref` or `flags`; at most 64 fields.

An attachment with a `size_key` takes its row's fields at every regenerate (and in
`vessel.inspect`): an exact key in the design's private table first, then the shipped table;
an `OD<mm>` key with no row takes the nearest smaller OD of that kind. A key no table has keeps
the attachment's own dimensions, with a warning when the attachment has a `standard_ref`. A row
that is not `two-source` or `derived` sets `flags.unverified_table` and warns. The applied values
are stored in the vessel, so dropping the table later keeps the last dimensions.

The shipped presets (saddle OD600–3600, legs OD300–1600, lifting lugs OD600–3600) are Triya's
own proportional rules (`derived`), starting points to model with, not design values. They
were written out by a throw-away script (not committed, Rust-only rule); the JSON is the record.

`vessel.import_tables {tables: {flanges?, supports?} | path | clear}` keeps the user's private
tables in the design's metadata under `in.openengtools.triya.tables` (at most 60 KiB; upstream
caps one value at 64 KiB), searched before the shipped ones; each kind replaces the previous
private table of that kind, and the import is one undo step. EIL standard sheets (and any other
company standard) reach a design only this way: the repository ships no EIL values (a test scans
`data/`); the import test uses the fictional `triya-vessel/tests/fixtures/fictional-private-supports.json`.

## 4. Validator (`cargo test -p triya-stdlib`)
- Every row has `source[]` (≥ 1) and `verified`; `two-source` requires ≥ 2 distinct sources.
- Monotonic sanity (B16.5 only): OD, BC and bolt count non-decreasing with NPS within a class and
  kind. B16.47 is not monotonic (Series A Cl 300 NPS 38 has OD 1170, NPS 36 has 1270: its sizes
  come from MSS SP-44 and API 605). Everywhere: BC < OD;
  rf_dia < BC.
- Hub and holes fit: `hub_od < bolt_circle − bolt_hole`, `bolt_circle + bolt_hole < od`.
- Unit sanity: flange lengths in mm with ≤ 1 decimal, pipe walls ≤ 2 decimals (B36.10M lists
  walls to 0.01 mm); bolt sizes as fraction strings or `M{n}`.
- No `illustrative` rows outside `tests/` (also enforced by `Triya CI`).

## 5. Sources log (append as tables are keyed)
| table | source A | source B | disagreements |
|---|---|---|---|
| B16.5 Cl 150/300 WN, SO, blind, LJ, SW (T3.1, 184 rows, 168 two-source) | bd_warehouse 0.4.0 `flange.py` (Tables 7–10, metric) | wermac.org per-NPS pages (`dim_{wn,bl,so,lj,sw}flg_b16-5_*.html`, mm) | wermac's thicknesses and hub lengths include the 1.6 mm (1/16 in) raised face (B16.5's metric tables: tf without the 2 mm RF) and were compared after subtracting it. 14 rows differ beyond 0.15/0.6 mm, each noted in its row, bd_warehouse kept: NPS 2 WN tf (wermac's NPS 2 page omits the RF), NPS 2½ SO hub length (+1.6), NPS 14 Cl 150 SO hole (wermac 31.8 = 1¼ in, B16.5 1⅛ in), hub diameters NPS 3/3½/5/16/20 (±2 mm). NPS 5 blind/SO/LJ and SW above NPS 2½ have no wermac page: `single-source`. RF diameters: wermac only (bd_warehouse has none). OD: B16.5's metric table rounds (NPS 4 Cl 150: 230, inch table 9.00 in = 228.6); Triya ships the metric values |
| B36.10M carbon steel, NPS ⅛–42 (T3.1, 279 rows, 224 two-source) | bd_warehouse 0.4.0 `pipe.csv` (inches × 25.4) | wermac.org `dim_b36.10_pipes_part-{a…k}.html` (mm column) | walls within 0.05 mm take wermac's mm value (the B36.10M column: e.g. NPS ½ Sch 160 4.78, inch 0.187 × 25.4 = 4.75); NPS 34 Sch 10 differs (8.74 vs 7.92), bd_warehouse kept and noted; sizes without a wermac row are `single-source` |
| B16.5 Cl 400/600/900/1500/2500 WN + blind (T3.2, 148 rows, 90 two-source) | bd_warehouse 0.4.0 `data_flange/flange_data_class_*.csv` + `drilling_class_*.csv` | wermac.org per-NPS pages (mm; Cl 400: the inch page × 25.4) | bd_warehouse's CSVs have defects, fixed and noted per row: lost decimal points in tf (175.0 → 17.5, 318 → 31.8, 413 → 41.3, 1778 → 177.8), stray empty cells, a `1,040` thousands separator, a letter `l` for `1` in a bolt size, a missing bolt size (taken from wermac). Rows still missing data are left out: NPS ¾ Cl 600/1500/2500, NPS 18 Cl 900. wermac's thickness and hub length include the 6.4 mm RF on some pages and not on others; both readings were accepted. Rows with any remaining difference (Cl 1500/2500 bolt circles up to 2.5 mm, hub diameters 1–8 mm, Cl 400 NPS 6/16 OD from the inch table) are `single-source` with the difference in `note` |
| B16.47 Series A, Cl 150–900 WN + blind (T3.2, 168 rows, 144 two-source) | wermac.org `dimensions_welding-neck_blind-flanges_class-*_asme-b16-47-a.html` (mm) | blog.projectmaterials.com `asme-b-16-47-flange-dimensions` (inches × 25.4) | od/bolting/RF/hub from wermac, tf and hub length from the inch table (wermac includes the RF on some pages). Differing rows are `single-source`: Cl 400 NPS 30–46 hole 54.0 vs 66.5, a few bolt circles (NPS 52 Cl 300: 1479.6 vs 1428.8) |
| B16.47 Series B, Cl 150–900 WN + blind (T3.2, 168 rows, 82 two-source) | as Series A, `…-b16-47-b.html` | as Series A | Cl 300 hub diameters differ by ~40–50 mm throughout (wermac 756 vs 716 at NPS 28), Cl 400 blind thicknesses by the RF, so those rows are `single-source`; the hub difference is a candidate for an owner check against the standard |

| B36.19M stainless, NPS ⅛–30, 5S/10S/40S/80S (T4.5, 93 rows, 93 two-source) | wermac.org `dim_b36.19_pipes.html` (mm) | en.wikipedia.org Nominal Pipe Size wall tables (inches × 25.4) | none beyond 0.05 mm; wermac lists no 5S wall for NPS ⅛–⅜ ("Note 1"), so those rows are left out. Added when reference vessel C (10S and 40S nozzles) was first regenerated whole |

B16.47 is not monotonic in NPS (Series A sizes 26–36 come from MSS SP-44, 38–60 from API 605), so
the sanity rules check monotonicity on B16.5 only.

The conversion was done once by a throw-away script (bd_warehouse's dict literals and CSV, the
wermac HTML tables) that is not committed (Rust-only rule); the JSON is the record, and its rows
say where each value came from.

## 6. Attribution rows to add to `ATTRIBUTION.md`
- `triya/crates/triya-stdlib/data/asme-b16.5.json`, `asme-b36.10.json` — values re-keyed from
  gumyr/bd_warehouse (Apache-2.0); dimensions are facts of the public standard; cross-checked
  against wermac.org (rows added in T3.1).
- `triya/crates/triya-stdlib/data/asme-b16.47a.json`, `asme-b16.47b.json` — values from wermac.org's
  metric tables and blog.projectmaterials.com's inch tables (facts of the public standard; rows
  added in T3.2).
- OET title-block template SVGs — original work, MIT OR Apache-2.0.
- `triya/crates/triya-stdlib/data/asme-b36.19.json` — wermac.org + Wikipedia, facts of the public standard (T4.5).
- `triya/crates/triya-stdlib/data/support-presets.json` — Triya's own proportional rules (T4.4).
- `triya/crates/triya-vessel/tests/fixtures/fictional-private-supports.json` — invented test data (T4.4).
- Triya icons — drawn in code in `triya-ui::icons`, original; any rasterised icon files, too.
