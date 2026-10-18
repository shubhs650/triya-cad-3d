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

## 4. Validator (`cargo test -p triya-stdlib`)
- Every row has `source[]` (≥ 1) and `verified`; `two-source` requires ≥ 2 distinct sources.
- Monotonic sanity: OD, BC and bolt count non-decreasing with NPS within a class; BC < OD;
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

The conversion was done once by a throw-away script (bd_warehouse's dict literals and CSV, the
wermac HTML tables) that is not committed (Rust-only rule); the JSON is the record, and its rows
say where each value came from.

## 6. Attribution rows to add to `ATTRIBUTION.md`
- `triya/crates/triya-stdlib/data/asme-b16.5.json`, `asme-b36.10.json` — values re-keyed from
  gumyr/bd_warehouse (Apache-2.0); dimensions are facts of the public standard; cross-checked
  against wermac.org (rows added in T3.1).
- OET title-block template SVGs — original work, MIT OR Apache-2.0.
- Triya icons — drawn in code in `triya-ui::icons`, original; any rasterised icon files, too.
