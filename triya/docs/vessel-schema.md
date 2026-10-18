# `triya.vessel/0.1` — vessel document schema

The vessel document is the source of truth for the vessel module (ADR-0003). It is a compact JSON
object; `triya-vessel::model` holds the serde structs (every kind and field below, T2.1) and
generates `triya/crates/triya-vessel/vessel-schema.json` (`schemars`); the test
`schema_file_is_current` fails CI when the file is stale (`TRIYA_BLESS=1 cargo test -p triya-vessel
schema_file_is_current` rewrites it). Units: **mm, degrees, kg** in the document; converted to
mm/radians at the engine boundary. Unknown **top-level** fields are preserved
(`#[serde(flatten)]`) for forward compatibility, and `extra` is free-form (both together ≤ 16 KiB);
inside elements and attachments unknown fields are dropped (their `kind` is a flattened tag, and
serde cannot flatten a second map beside it). Reference documents:
`triya/crates/triya-vessel/tests/reference/` (`vessel-a/b/c.json` per `beta-dod.md` §B, plus
`every-kind.json` covering the kinds A–C leave out).

Validation caps: ≤ 200 elements, ≤ 500 attachments, lengths finite and ≤ 1 km, counts 1 … 1000,
ids ≤ 64 characters and unique across elements and attachments, text ≤ 4 KiB. A cone states
both ends and no `diameter_mm`; a body flange takes exactly one of `flange_ref` / `explicit`; a
limpet coil exactly one of `turns` / `end_axial_mm`; `left_tl` is the datum of horizontal
vessels and only of them.

## 1. Top level
```jsonc
{
  "schema": "triya.vessel/0.1",
  "id": "V-101",                      // vessel tag; prefixes all generated parameter names
  "title": "Knock-out drum",
  "global": { … },                    // §2
  "elements": [ … ],                  // §3, ordered bottom→top (vertical) or left→right (horizontal)
  "attachments": [ … ],               // §4 nozzles, supports, details
  "design_data": { … },               // §5 drawing/BOM data only; no code calculations
  "notes": ["…"],
  "revisions": [{ "rev": "0", "date": "2026-11-08", "by": "SSP", "desc": "Issued for beta" }],
  "extra": { }                        // free-form, preserved
}
```

## 2. `global`
| field | type | values / default | meaning |
|---|---|---|---|
| `orientation` | enum | `vertical` (default) · `horizontal` | vertical: axis +Z, stack bottom→top; horizontal: axis +X, stack left→right |
| `datum` | enum | `bottom_tl` (default) · `bottom_seam` · `skirt_base` · `plant_elevation` · `left_tl` (horizontal) | the 0.0 of all `axial` references and of drawing dimensions |
| `plant_elevation_mm` | number | 0 | elevation of the datum above plant grade (drawing only) |
| `diameter_basis` | enum | `inside` (default) · `outside` | how `diameter_mm` on elements is read |
| `angle_zero` | enum | `plant_north` (default) · `plus_x` · `plus_y` | what 0° means on the plan view |
| `angle_sense` | enum | `clockwise_from_above` (default) · `counterclockwise_from_above` | configurable (PV Elite-style setting); shown in the UI header |
| `default_material` | string | `"SA-516 Gr.70"` | BOM default |
| `corrosion_allowance_mm` | number | 0 | drawing note only in the beta |
| `insulation_thickness_mm` | number | 0 | nozzle projection reference when `projection.from = insulation` |

## 3. `elements[]` — the axisymmetric stack
Common fields: `id` (e.g. `"E1"`), `kind`, `thickness_mm`, `material?`, `description?`,
`diameter_mm?` (inherited from the previous element when omitted; inheritance stops at a cone,
which must state both ends).

| `kind` | specific fields | geometry (profile revolved 360° about the axis) |
|---|---|---|
| `skirt` | `height_mm`, `base_ring {od_mm, id_mm, thickness_mm}`, `top_offset_mm?` (distance below the bottom tangent line where the skirt meets the head, default 0 → attaches at TL) | cylinder + base annulus |
| `cylinder` | `length_mm` | rectangle (ID and OD lines) |
| `head` | `preset` (§3.1), `straight_flange_mm` (default per preset: ≥ 3t), `crown_radius_mm?`, `knuckle_radius_mm?` (override), `orientation` = `up`·`down` (auto from position) | SF line → knuckle arc → crown arc, inside and outside curves |
| `cone` | `length_mm` (axial), `diameter_small_mm`, `diameter_large_mm`, `knuckle_large_mm?`, `knuckle_small_mm?` (toriconical when > 0) | trapezoid ± knuckle arcs; warning when half-apex > 30° |
| `flat_cover` | `thickness_mm` (plate), `bolted: bool` | revolved rectangle |
| `body_flange` | `flange_ref {standard, nps_or_dn, class, type}` or explicit `{od_mm, bolt_circle_mm, bolt_count, bolt_hole_mm, thickness_mm, hub_od_mm, hub_len_mm}`, `gasket_gap_mm` (0) | WN profile revolved + bolt holes (circular pattern, extrude cut) |

### 3.1 Head presets (`preset`)
| preset | definition (D = inside diameter unless `diameter_basis = outside`) | depth (inside, excl. SF) |
|---|---|---|
| `asme_fd` | torispherical, L = D, r = 0.06·D (min 3t) | h = L − √[(L−r)² − (D/2−r)²] (≈ 0.1693·D) |
| `ellipsoidal_2_1` | true semi-ellipse a = D/2, b = D/4 | D/4 |
| `ellipsoidal_2_1_ug32_approx` | torispherical L = 0.9·D, r = 0.17·D | same formula |
| `kloepper_din28011` | L = Da, r = 0.1·Da (Da = outside) | exact geometry; DIN's fit h2 = 0.1935·Da − 0.455·s (Da 1000/s 10: 188.95 vs exact 188.89); h1 ≥ 3.5·s |
| `korbbogen_din28013` | L = 0.8·Da, r = 0.154·Da | exact geometry; DIN's fit h2 = 0.255·Da − 0.635·s (248.65 vs exact 248.26); h1 ≥ 3·s |
| `torispherical_80_10` | L = 0.8·D, r = 0.10·D | formula |
| `hemispherical` | sphere R = D/2 | D/2 |
| `custom_torispherical` | explicit `crown_radius_mm`, `knuckle_radius_mm` | formula |

Inside curve: SF of length `straight_flange_mm` at radius D/2; knuckle arc centre
(D/2 − r, z_sf), radius r; crown arc centre on the axis, tangent to the knuckle (the evaluator
computes both centres and the tangency point; tests cover the DIN worked examples within 0.5 mm
and state which convention — exact geometry — is used). Outside curve: same construction with
D + 2t, r + t, L + t.

## 4. `attachments[]`
Common fields:
```jsonc
{
  "id": "N1", "kind": "nozzle", "host": "E3",
  "axial": { "ref": "datum" | "host_from", "mm": 1250 },     // along the vessel axis
  "angle_deg": 90,                                           // azimuth per global.angle_zero/sense
  "frame": { "type": "radial" },                             // §4.1
  "standard_ref": { "org": "EIL", "number": "7-12-0013", "rev": "6" },  // optional
  "size_key": "NPS4-150-wn",                                 // optional, keys into a TableSet
  "material": "SA-106 Gr.B",
  "flags": { "boolean_failed": false, "unverified_table": false }   // set by the generator
}
```
`axial.ref = datum` keeps the elevation fixed when elements are re-stacked (COMPRESS-like);
`host_from` keeps it relative to the host's start node (PV Elite-like). The UI offers both.

### 4.1 `frame.type`
| type | extra fields | meaning |
|---|---|---|
| `radial` | — | axis through the vessel axis, perpendicular to it |
| `hillside` | `offset_mm` | axis parallel to a radial, offset laterally |
| `tangential` | `offset_mm` (default = R − neck_r) | axis offset so the neck OD is flush with the shell |
| `tilted` | `tilt_deg` | radial rotated about the circumferential tangent |
| `parallel_to_axis` | `radius_mm` | on heads: axis parallel to the vessel axis at radius `radius_mm` |
| `head_radial_at_r` | `radius_mm` | on heads: normal to the head surface at radius `radius_mm` |

### 4.2 Kinds
| kind | fields |
|---|---|
| `nozzle` | `nps` (e.g. `"4"`), `schedule` (`"STD"`, `"XS"`, `"80"`…) or `neck_thk_mm`, `neck_type` = `pipe`·`lwn`·`forged`, `flange {standard: "B16.5"|"B16.47A"|"B16.47B", class, type: wn|so|lwn|blind|lj, facing: rf|ff|rtj}`, `projection {from: cl|od|insulation, mm}`, `inside_projection_mm` (0), `attach` = `set_in`·`set_on`, `pad {width_mm, thickness_mm}?`, `manway {davit: bool}?`, `service` (text for the schedule) |
| `saddle` | `width_mm`, `height_mm` (base to vessel bottom), `contact_angle_deg` (120), `web_thk_mm`, `rib_count`, `rib_thk_mm`, `base_plate {len_mm, wid_mm, thk_mm}`, `wear_plate {thk_mm, width_mm, angle_deg}?`, `anchor_bolts {count, dia_mm}?` — host must be a cylinder on a horizontal vessel |
| `leg` | `count` (pattern), `section` = `pipe {nps, schedule}` · `angle {size}` · `channel {size}`, `length_mm`, `attach` = `head`·`shell`, `base_plate?`, `start_angle_deg` |
| `bracket` | `count`, `gusset {height_mm, width_mm, thk_mm}`, `top_plate {len_mm, wid_mm, thk_mm}`, `bottom_plate?`, `bolt_holes?` |
| `anchor_chair` | `count`, `height_mm`, `width_mm`, `gusset_thk_mm`, `top_plate_thk_mm`, `bolt_dia_mm` — host must be a skirt |
| `stiffening_ring` | `section` = `bar {height_mm, thk_mm}` · `angle {size}`, `inside: bool` (false) |
| `limpet_coil` | `pipe_nps`, `schedule`, `pitch_mm`, `turns` or `end_axial_mm`, `start_angle_deg`, `hand` = `rh`·`lh`, `half_pipe: true` |
| `lifting_lug` | `style` = `longitudinal`·`circumferential`·`trunnion`·`tailing`, `plate {len_mm, height_mm, thk_mm}`, `hole_dia_mm`, `pad?` |
| `insulation_ring` | `width_mm`, `thk_mm`, `segments` |
| `clip` | `plate {len_mm, wid_mm, thk_mm}`, `count`, `purpose` (`ladder`·`platform`·`pipe_support`·`earthing`·`nameplate`) |

## 5. `design_data` (drawing/BOM only — Triya does no code calculations in the beta)
`code` ("ASME VIII-1 2025"), `mawp_int {kpa, at_c}`, `mawp_ext?`, `design_temp_c`, `mdmt_c`,
`hydrotest_kpa`, `pwht: bool`, `rt` ("Spot"/"Full"/"None"), `joint_eff`, `wind_code`,
`seismic_code`, `empty_weight_kg?`, `operating_weight_kg?`, `test_weight_kg?` (computed values
fill these when absent), `capacity_m3?`, `nameplate {serial, year}`.

## 6. Generated parameter names
Every emitted dimension becomes a user parameter `V<vessel-id>_<element-or-attachment-id>_<field>`
in mm/deg, e.g. `V101_E3_length`, `V101_N1_projection`. Users may edit them in upstream's
parameter table; `vessel.inspect` reports a drift warning when a parameter no longer matches the
document (the next regenerate rewrites it; `vessel.adopt_parameters` copies parameter values back
into the document — post-beta).

## 7. Analytic checks used by tests (inside dimensions)
- Cylinder metal volume: π/4·(OD² − ID²)·L (skeleton test: 1000 ID × 10 t × 2000 L → 63.46e6 mm³).
- Head volume (inside, excl. SF): hemi π·D³/12; 2:1 ellipsoidal π·D³/24; torispherical via the
  closed-form sum of the knuckle torus segment and the spherical cap (implemented in
  `triya-stdlib::head`; cross-checked against the C-coefficient table in tests with 1 %
  tolerance, exact formula with 0.1 %).
- Head blank/area: F&D curved area ≈ 0.9286·D² (handbook); Klöpper Aa ≈ 0.99·Da² + π·h1·Da.
- Coil length: π·D_coil·n / cos(atan(pitch/(π·D_coil))).

## 8. Example (abridged reference vessel A, see `beta-dod.md`)
```jsonc
{
  "schema": "triya.vessel/0.1", "id": "V-101", "title": "Reference vessel A",
  "global": { "orientation": "vertical", "datum": "bottom_tl", "diameter_basis": "inside" },
  "elements": [
    { "id": "E0", "kind": "skirt", "height_mm": 2500, "thickness_mm": 10, "diameter_mm": 1600,
      "base_ring": { "od_mm": 1900, "id_mm": 1500, "thickness_mm": 20 } },
    { "id": "E1", "kind": "head", "preset": "ellipsoidal_2_1", "diameter_mm": 1600, "thickness_mm": 12, "straight_flange_mm": 40 },
    { "id": "E2", "kind": "cylinder", "length_mm": 3000, "thickness_mm": 12 },
    { "id": "E3", "kind": "cylinder", "length_mm": 2500, "thickness_mm": 12 },
    { "id": "E4", "kind": "cone", "length_mm": 800, "diameter_small_mm": 1600, "diameter_large_mm": 2200, "thickness_mm": 14, "knuckle_large_mm": 100 },
    { "id": "E5", "kind": "cylinder", "diameter_mm": 2200, "length_mm": 2000, "thickness_mm": 14 },
    { "id": "E6", "kind": "head", "preset": "asme_fd", "thickness_mm": 14, "straight_flange_mm": 50 }
  ],
  "attachments": [
    { "id": "N1", "kind": "nozzle", "host": "E2", "axial": { "ref": "datum", "mm": 600 }, "angle_deg": 0,
      "frame": { "type": "radial" }, "nps": "6", "schedule": "STD",
      "flange": { "standard": "B16.5", "class": 150, "type": "wn", "facing": "rf" },
      "projection": { "from": "cl", "mm": 1100 }, "attach": "set_in", "pad": { "width_mm": 50, "thickness_mm": 12 }, "service": "Inlet" },
    { "id": "N2", "kind": "nozzle", "host": "E3", "axial": { "ref": "datum", "mm": 4500 }, "angle_deg": 90,
      "frame": { "type": "hillside", "offset_mm": 300 }, "nps": "3", "schedule": "XS",
      "flange": { "standard": "B16.5", "class": 300, "type": "wn", "facing": "rf" }, "projection": { "from": "od", "mm": 200 } },
    { "id": "M1", "kind": "nozzle", "host": "E6", "axial": { "ref": "host_from", "mm": 0 }, "angle_deg": 180,
      "frame": { "type": "parallel_to_axis", "radius_mm": 600 }, "nps": "24", "schedule": "STD",
      "flange": { "standard": "B16.5", "class": 150, "type": "wn", "facing": "rf" }, "projection": { "from": "cl", "mm": 1600 },
      "manway": { "davit": true }, "service": "Manway" },
    { "id": "L1", "kind": "lifting_lug", "host": "E5", "axial": { "ref": "host_from", "mm": 1000 }, "angle_deg": 90,
      "frame": { "type": "radial" }, "style": "longitudinal", "plate": { "len_mm": 300, "height_mm": 200, "thk_mm": 25 }, "hole_dia_mm": 60,
      "standard_ref": { "org": "EIL", "number": "7-12-0024", "rev": "5" }, "size_key": "LL-25" },
    { "id": "R1", "kind": "stiffening_ring", "host": "E3", "axial": { "ref": "host_from", "mm": 1250 }, "angle_deg": 0,
      "frame": { "type": "radial" }, "section": { "bar": { "height_mm": 100, "thk_mm": 12 } } },
    { "id": "C1", "kind": "limpet_coil", "host": "E2", "axial": { "ref": "host_from", "mm": 200 }, "angle_deg": 0,
      "frame": { "type": "radial" }, "pipe_nps": "2", "schedule": "40", "pitch_mm": 150, "turns": 12, "hand": "rh", "half_pipe": true }
  ]
}
```
