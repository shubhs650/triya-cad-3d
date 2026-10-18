# Standards library (`triya-stdlib`)

Policy: ADR-0007. This file defines the table format, sources and verification rules. The
kickoff skeleton ships three tiny tables marked `illustrative` (`data/flanges.json`,
`data/pipe_schedules.json`, `data/head_presets.json`) — they must be replaced in T3.1/T3.2; CI
forbids `illustrative` rows outside `tests/`.

## 1. Table set format (`data/*.json` and private imports)
```jsonc
{
  "schema": "triya.tables/0.1",
  "name": "asme-b16.5-wn",                 // unique key
  "org": "ASME", "standard": "B16.5", "edition": "2020",   // provenance of the *standard*
  "licence_note": "Dimensions re-keyed from public sources listed per row; verify against the current edition.",
  "columns": ["nps","class","type","od_mm","bolt_circle_mm","bolt_count","bolt_size","thickness_mm","hub_od_mm","hub_len_mm","rf_dia_mm","bore_mm"],
  "rows": [
    { "nps": "4", "class": 150, "type": "wn", "od_mm": 228.6, "bolt_circle_mm": 190.5, "bolt_count": 8, "bolt_size": "5/8", "thickness_mm": 23.9, "hub_od_mm": 135.0, "hub_len_mm": 76.0, "rf_dia_mm": 157.2, "bore_mm": null,
      "source": ["bd_warehouse flange.py (Apache-2.0), 2026-10", "wermac.org B16.5 class 150 WN, 2026-10"], "verified": "two-source" }
  ]
}
```
`verified` ∈ `two-source` · `single-source` · `derived` (computed from other rows/formula) ·
`illustrative` (placeholder, never shipped). Rows may carry `note` (e.g. "sources disagree on
hub_len: 76 vs 76.2; kept 76").

Private sets (EIL etc.) use the same format with `"private": true`; `vessel.import_tables`
loads them at runtime (OPFS/file) and they override shipped presets by `name` + `sizeKey`.

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
- Flange: `size_key = "NPS{nps}-{class}-{type}"` (e.g. `NPS4-150-wn`). B16.47: `"NPS{nps}-{class}-{A|B}-{type}"`.
- Supports/attachments: free `size_key` strings matched against the set's `sizeKey` column
  (e.g. EIL `"7-12-0024/LL-25"`); presets use `"OD1600"` style keys with nearest-lower lookup.

## 4. Validator (`cargo test -p triya-stdlib`)
- Every row has `source[]` (≥ 1) and `verified`; `two-source` requires ≥ 2 distinct sources.
- Monotonic sanity: OD, BC and bolt count non-decreasing with NPS within a class; BC < OD;
  rf_dia < BC.
- Unit sanity: all lengths in mm with ≤ 1 decimal; bolt sizes as fraction strings or `M{n}`.
- No `illustrative` rows outside `tests/` (also enforced by `Triya CI`).

## 5. Sources log (append as tables are keyed)
| table | source A | source B | disagreements |
|---|---|---|---|
| b16.5 cl150 wn | bd_warehouse | wermac.org | — |

## 6. Attribution rows to add to `ATTRIBUTION.md`
- `triya/crates/triya-stdlib/data/asme-b16.5-*.json` — values re-keyed from gumyr/bd_warehouse
  (Apache-2.0); dimensions are facts of the public standard; verified against vendor tables.
- OET title-block template SVGs — original work, MIT OR Apache-2.0.
- Triya icons — drawn in code in `triya-ui::icons`, original; any rasterised icon files, too.
