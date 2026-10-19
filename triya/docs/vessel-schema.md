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
| `skirt` | `height_mm` (from the underside of the base ring to the skirt's top), `base_ring {od_mm, id_mm, thickness_mm}`, `top_offset_mm?` (distance below the bottom tangent line where the skirt meets the head, default 0 → attaches at TL); first element only; without `diameter_mm` its OD is the bottom head's OD | cylinder + base annulus |
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

### 3.2 Resolution (`triya-vessel::eval`, T2.2)
Stations run in document order; a head's station spans its seam to the inside of its crown, with
the tangent line (TL) where the straight flange meets the knuckle (bottom head: `z_to − SF`; top
head: `z_from + SF`). Datums: `bottom_tl`/`left_tl` = bottom (left) TL, `bottom_seam` = end of the
bottom head's straight flange, `skirt_base` = underside of the base ring, `plant_elevation` =
grade with the bottom TL at `plant_elevation_mm`. A cone's end nearer the inherited diameter comes
first and the next element inherits its far end; a near-end mismatch > 0.5 mm and a half-apex
angle > 30° (measured along the knuckle-tangent line) are warnings. Cone knuckles are inside radii;
a small-end knuckle must exceed the wall thickness. ASME F&D's knuckle is `max(0.06 D, 3 t)`;
default straight flanges are 3.5 s (Klöpper) and 3 t (others).

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

How the frames are read (T3.3, `triya-vessel/src/nozzle.rs`):
- Azimuth: vertical vessels have 0° at plant north (+Y; `plus_x`: +X), clockwise looking down,
  so 90° is +X. Horizontal vessels have 0° at the top (+Z; `plus_y`: +Y), clockwise looking from
  the left end, so 90° is −Y. `counterclockwise_from_above` reverses both.
- `hillside`/`tangential` offsets run along the circumferential tangent `axis × radial`.
- `tilted` turns the radial about the tangent through the point where the axis meets the shell's
  outside at the elevation; positive tilt points toward +axis.
- Head frames ignore `axial`. `head_radial_at_r` is normal to the head's outside surface at
  `radius_mm` from the axis.
- `projection.from = cl` is measured along the nozzle axis from the vessel centreline (cylinder
  frames) or from the head's tangent-line plane (head frames). `od` is measured from where the
  axis leaves the host's outside surface.
- `set_in` and `set_on` build the same joined solid (host + neck outside it − bore); the weld
  details differ only on the drawing. `inside_projection_mm` is a square cut at that distance
  inside where the axis meets the inside surface.
- Pads are built on cylinders only in the beta (on a head: a warning; the pad is drawn on the
  sheet).

- Flanges (T3.4) are their own body (`Vessel <id> flange`), the B16.5/B16.47 row's section
  revolved about the nozzle axis with its bolt holes (straddling the centrelines). `projection`
  ends at the gasket face: the top of the raised face (RF; RTJ is modelled as RF, no groove) or
  the flat face (FF). The neck ends at a weld neck's weld end, one wall inside a slip-on
  (or lap-joint/socket-weld) ring face, or at a blind's back. LWN uses the WN row in the beta.
  A manway's `davit` is a reference for the drawing and BOM; no davit geometry in the beta.

### 4.1a Supports and details (Phase 4, `triya-vessel/src/support.rs`)
Every support or detail is built from separate bodies, one per plate, pipe or ring, named
`Vessel <id> <part>` (`Vessel S1 web`, `Vessel S1 rib 2a`, `Vessel S1 wear plate`). They touch the
shell's outside surface (or a wear plate's) and never overlap it or each other; nothing is joined
to the shell (welds are drawn on the sheet). Each body's measured volume is checked against its
closed form (0.5 %); a part the kernel cannot build is left out with a warning and
`flags.boolean_failed`, and the rest of the regeneration goes on. `vessel.regenerate` reports
`supports: [{id, kind, built: built|partly|none, parts: [{name, what, expected_mm3, measured_mm3}]}]`;
`vessel.inspect` lists each support's parts and volumes. Parameters: `V<vessel>_<id>_<field>`.

- `saddle` (T4.1): on a cylinder of a horizontal vessel, centred at `axial`; always under the
  vessel (−Z), `angle_deg` is not used. `height_mm` runs from the base plate's underside to the
  shell's bottom (outside). Contact radius = shell OD/2 (+ wear plate thickness); horns at
  ±`contact_angle_deg`/2 from the bottom. Parts: the web (across the vessel, `web_thk_mm`, from
  the base plate up to the contact arc between the horns); `rib_count` ribs along the axis,
  horn to horn (one rib: at the bottom), `width_mm` long, each in two halves either side of the
  web (`rib Na`, `rib Nb`), up to the contact arc at the rib face nearer the bottom; the base
  plate (`len_mm` across, `wid_mm` along the axis) with `anchor_bolts.count` holes of bolt
  diameter + 6 mm in a row across, midway between the web and the plate's +axis edge; the wear
  plate (a cylinder sector on the shell, inside radius = shell OD/2, `width_mm` along the axis,
  `angle_deg` about the bottom). Two saddles are two attachments (`axial` from `left_tl`).
- `leg` (T4.2): vertical vessels; `count` legs evenly from `start_angle_deg` (or `angle_deg` when
  that is 0). `section.pipe` from B36.10 (a tube, bored); `section.angle` read from its size
  (`ISA 100x100x10` → 100 × 100 × 10), heel toward the vessel at the shell OD, flanges at ±45°
  from the radial; `section.channel` is not built in the beta (no open channel table).
  `attach: shell`: the leg's top at `axial`, a pipe tangent to the shell's outside;
  `attach: head` (host = a head): the leg's outer face on the head's inside radius, its top square
  at the head's outside under the leg's inner edge (a head-moulded top builds — the pipe cut by
  the crown's sphere — but took the kernel ~70 s a leg, so it is drawn on the sheet instead).
  `length_mm` runs down from the top; `base_plate` is centred under each leg, its edges along the
  radial and the tangent. Bodies `Vessel <id> leg <n>`, `… leg <n> base plate`.
- `bracket` (T4.2): vertical vessels; `count` brackets evenly from `angle_deg`, `axial` = the top
  of the top plate. Top plate `len_mm` out from the shell OD × `wid_mm` across; two gussets under
  it flush with its sides, `width_mm` out at the top, `height_mm` down, narrowing to the bottom
  plate's `wid_mm` (a triangle without one); bottom plate `wid_mm` out × `len_mm` across under the
  gussets; `bolt_holes` in the top plate, outboard of the gussets when the plate overhangs them
  by a hole + 10 mm, else between them. Bodies `Vessel <id> <n> gusset a|b`, `… top plate`,
  `… bottom plate`.
- `anchor_chair` (T4.2): on a skirt with a base ring; `count` chairs evenly from `angle_deg` on
  the base ring's top: two gussets `gusset_thk_mm` at ±(`width_mm` − gusset)/2 from the skirt OD
  to the ring OD, a top plate `top_plate_thk_mm` at `height_mm` above the ring with a hole of
  `bolt_dia_mm` + 6 on the bolt circle midway between the skirt OD and the ring OD.

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

### 6.1 Storage and regeneration (T2.5)
- The document is kept in the design's metadata under `in.openengtools.triya.vessel` as
  `{schema, doc, generated: {features, group}}`. When its compact JSON is over 60 KiB, `doc` is
  replaced by `chunks: n` and the JSON text is split over `in.openengtools.triya.vessel.chunk-1…n`
  (each `{"text": …}`, at most 4 chunks: upstream limits one value to 64 KiB and all metadata to
  256 KiB). Upstream opens and saves such a design unchanged; it just doesn't read it.
- `vessel.regenerate` puts what it builds in the timeline folder `Vessel <id>` and records the
  feature ids and the folder id in `generated`; the next regeneration deletes exactly those
  features (a feature the user adds, even inside the folder, stays) and reuses the folder.
  The whole regeneration is one undo step.
- `vessel.inspect` reports the resolved stations (profile, tangent line, half-apex, flange or base
  ring detail), the extent and TL–TL, warnings (cone angle, joint mismatch, derived bolt holes,
  parameters changed in the table), attachment flags, the parameters it would write and the plan.

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
