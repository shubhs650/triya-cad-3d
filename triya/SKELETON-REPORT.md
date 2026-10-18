# Triya CAD 3D — overlay skeleton report

Base: SolidCraft v0.14.0, commit `2bb9c81` (read-only clone at `/home/claude/upstream/solidcraft`).
Work tree: `/home/claude/work/triya-cad-3d` (copy; `CARGO_TARGET_DIR=/home/claude/work/target`).
Kit copy: `/home/claude/triya-kit/triya/` (the whole overlay), `/home/claude/triya-kit/patches/`
(`0001-seams.patch`, `0002-cargo-lock.patch`, `root-Cargo.toml.diff`, `PATCHES.md`),
`/home/claude/triya-kit/upstream-new-files/crates/engine/src/cmd/triya_extra.rs` (the one new file
inside the upstream tree; it is also inside `0001-seams.patch`).

Toolchain here: cargo 1.97.0, 2 cores, 7 GB RAM. Upstream pins `rust-version = "1.95"`.

## 1. Files created

Overlay (`triya/`), every Rust file with `// SPDX-License-Identifier: MIT OR Apache-2.0` and
`// Copyright (c) 2026 Triya CAD 3D contributors` headers:

| file | purpose |
|---|---|
| `triya/README.md` | overlay overview, crate table, build commands, desktop-app note |
| `triya/patches/PATCHES.md` | every upstream file touched: file, anchor, change, reason |
| `triya/patches/0001-seams.patch` | `git diff v0.14.0` of the 9 upstream files (incl. the new `triya_extra.rs`); verified with `git apply --check -R` |
| `triya/patches/0002-cargo-lock.patch` | the Cargo.lock delta (new members only; no new third-party crates) |
| `triya/crates/triya-brand/{Cargo.toml,src/lib.rs}` | `APP_NAME`, `APP_ID = "in.openengtools.triya"`, `ORG`, `HOMEPAGE`, `REPO`, `UPSTREAM_NAME/VERSION/SHA/REPO`, `LICENSE`, `about_text()`; 2 tests |
| `triya/crates/triya-stdlib/Cargo.toml` | deps: serde, serde_json, thiserror |
| `triya/crates/triya-stdlib/src/lib.rs` | `StandardRef {org, number, rev}`, `StdlibError`, re-exports |
| `triya/crates/triya-stdlib/src/flange.rs` | `FlangeKind {Wn, So, Blind, Lj, Sw, Lwn}`, `FlangeRow {nps, class, kind, od, bolt_circle, bolt_count, bolt_size, thickness, hub_od, raised_face_d, length_through_hub, bore_schedule}`, `validate()`, `size_key()` |
| `triya/crates/triya-stdlib/src/pipe.rs` | `PipeScheduleRow {nps, dn, od, schedule, wall}` |
| `triya/crates/triya-stdlib/src/head.rs` | `HeadPreset {Torispherical{crown_ratio, knuckle_ratio, basis}, Ellipsoidal{ratio}, Hemispherical, Toriconical{..}}`, `RatioBasis {Inside, Outside}`, `dish_depth(d_inside, t)`, `depth(d_inside, sf, t)`, `torispherical_depth(D, L, r)`; 6 tests |
| `triya/crates/triya-stdlib/src/tables.rs` | `Table<R> {source, note, standard, rows}`, `TableSet {flanges, pipe_schedules, head_presets}` with `embedded()`, `from_json()`, lookups; size caps; 3 tests |
| `triya/crates/triya-stdlib/data/{flanges,pipe_schedules,head_presets}.json` | 3 + 2 + 5 rows, each file `"source": "ILLUSTRATIVE — replace with verified values"` (no standard tables reproduced) |
| `triya/crates/triya-vessel/Cargo.toml` | deps: solidcraft-engine, solidcraft-doc, triya-stdlib, serde, serde_json, thiserror |
| `triya/crates/triya-vessel/src/lib.rs` | crate docs (where the JSON is persisted), `VesselError {NotSupportedYet, Invalid, Stdlib, Json}`, `From<VesselError> for EngineError` |
| `triya/crates/triya-vessel/src/model.rs` | `VesselDoc {schema "triya.vessel/0.1", name, global, elements, attachments}`, `Global {orientation, datum, diameter_basis, angle_zero_ref, angle_sense, default_material}`, `Element {id, kind: Cylinder/Head{preset}/Cone{small_diameter_mm}/FlatCover/BodyFlange/Skirt, id_or_od_mm, thickness_mm, length_mm, straight_flange_mm, corrosion_allowance_mm, material, note}`, `Attachment {id, host, axial: AxialRef, angle_deg, frame: Radial/Hillside{offset_mm}/Tangential/Tilted{deg}/ParallelToAxis/HeadRadialAtR{r_mm}, kind: Nozzle{nps, class, flange, projection_mm, pad}/Saddle/Leg/Bracket/AnchorChair/StiffeningRing/LimpetCoil/LiftingLug, standard_ref, size_key}`, `validate()` with caps (256 elements, 2048 attachments, 1e6 mm) |
| `triya/crates/triya-vessel/src/eval.rs` | `resolve(&VesselDoc) -> Result<Resolved>`: stations with `z_from/z_to/r_from/r_to/thickness/profile`, datum shift, `radius_at(z)` for straight, conical, ellipsoidal/hemispherical and torispherical (knuckle + crown) profiles |
| `triya/crates/triya-vessel/src/emit.rs` | `CommandCall {id, params}`, `plan(&Resolved)`, `plan_station`, `substitute()` for `"$ref:<step>.<field>"`; cylinder only, else `NotSupportedYet` |
| `triya/crates/triya-vessel/src/commands.rs` | `pub static COMMANDS: &[CommandSpec]` = `vessel.new`, `vessel.set`, `vessel.regenerate`, `vessel.inspect`; `METADATA_KEY`, `TAB` |
| `triya/crates/triya-vessel/src/tests.rs` | 7 tests (plan sequence, resolve of heads, invalid docs, hostile params, registry, real regeneration measured, attachment refusal) |
| `triya/crates/triya-drawing/{Cargo.toml,src/lib.rs}` | deps: serde, serde_json, thiserror, triya-vessel (model/eval only); `DrawingError` |
| `triya/crates/triya-drawing/src/model.rs` | `P2`, `Layer`, `LineStyle`, `Edge {Line, Arc, Circle, Ellipse, Polyline}`, `StyledEdge`, `Dimension {Linear, Diameter, Radius, Angular}`, `View`, `Table`, `Revision`, `TitleBlock`, `SheetSize`, `Sheet` |
| `triya/crates/triya-drawing/src/plot.rs` | `Plotter` trait (`set_layer/set_line_style/set_width/move_to/line_to/arc/circle/text`), `plot_edge/plot_dimension/plot_view/plot_sheet`, `SvgPlotter`; 4 tests (rectangle → 4 `<line>`s, etc.) |
| `triya/crates/triya-drawing/src/vessel_views.rs` | `elevation`, `orientation_plan`, `general_arrangement` → `NotSupportedYet` after resolving; 1 test |
| `triya/crates/triya-ui/{Cargo.toml,src/lib.rs}` | `register()`, `about_text()`, `VesselPanel::show(app, ctx)` (JSON text box, New/Apply/Regenerate/Inspect via `app.run`, About); 1 test |
| `triya/apps/triya-web/Cargo.toml` | copy of `apps/solidcraft-web` + `triya-ui`, `triya-brand`; bin `triya-web` |
| `triya/apps/triya-web/index.html` | title/description/og/twitter → Triya, `data-bin="triya-web"`, asset paths `../../../assets/…`, manifest + `copy-dir icons`; `window.triyaInstall`; element ids kept upstream's (shared `loading.css` styles them by id) |
| `triya/apps/triya-web/manifest.webmanifest` | `name "Triya CAD 3D"`, `short_name "Triya CAD"`, `id "in.openengtools.triya"`, icons `icons/<size>/in.openengtools.triya.png` |
| `triya/apps/triya-web/sw.js` | copy; constants only: `CACHE_PREFIX = \`triya-app ${PATH} \``, `PAGES = \`triya-app-pages ${PATH}\`` (+ header comment) |
| `triya/apps/triya-web/Trunk.toml` | dist `../../../dist/triya-web`, port 8782, same `web-offline` hook |
| `triya/apps/triya-web/src/main.rs` | copy; `triya_ui::register()` before `SolveApp::new`, `vessel: triya_ui::VesselPanel` in `Shell`, `self.vessel.show(&mut self.app, &ctx)` after `app.ui(ui)`, brand in the failure message, `WEBGL_KEY = "triya.webgl"` |
| `triya/apps/triya-web/src/{files,install,space_mouse}.rs` | copies with a provenance header; `LOCAL_PREFIX = "triya.file."`, `OFFER = "triyaInstall"` |
| `triya/apps/triya-web/icons/README.md` + empty `32x32/ 256x256/ 512x512/` | icon placeholders: Triya's own mark goes here; upstream's SolidCraft icon artwork is not copied |

Desktop app `triya/apps/triya`: **not created**. `apps/solidcraft` is ~2 700 lines (control server,
Windows native crash capture/frame, macOS open-documents, Space Mouse HID, startup checks, update
check); not a small copy. The route is written in `triya/README.md`.

## 2. Upstream files modified (seams) and why

All in `triya/patches/PATCHES.md` and `0001-seams.patch`; summary:

1. `Cargo.toml` — `members += "triya/crates/*", "triya/apps/*"`; 5 `[workspace.dependencies]` entries.
2. `crates/engine/src/cmd/mod.rs` — two one-liners: `pub mod triya_extra;` (sorted mod group) and
   `triya_extra::extend(&mut v);` at the end of `command_specs()`.
3. `crates/engine/src/cmd/triya_extra.rs` — **new** file: `register(&'static [CommandSpec]) -> Result<(), String>`
   (idempotent for the same slice; refuses an id already registered, case-insensitively, and
   duplicates inside the slice) and `extend()`, over a `static RwLock<Vec<&'static [CommandSpec]>>`.
   A poisoned lock means "nothing registered", never a panic.
   **Why this and not `command_specs()` listing `triya_vessel::COMMANDS`:** `triya-vessel` depends on
   `solidcraft-engine` (it runs engine commands and uses `Session`, `CommandSpec`, `params`), so the
   engine cannot depend on it — a cycle. `Session` has no constructor hook for extra specs and
   `find_command`/`execute` go through the free function `command_specs()`, so an engine-level slot is
   the least invasive real option; the app registers at startup (`triya_ui::register()`), exactly as
   the task's alternative (a) describes.
4. `crates/ui-egui/src/toolbar.rs` — `TABS += "VESSEL"`. Panels per tab come from `CommandSpec.tab/.panel`
   automatically (`toolbar_custom::panel_lists`), but the tab row is this fixed constant; an unlisted
   tab gets panels `MAKE, ADD-INS, UTILITY, INSPECT, SELECT` (`toolbar::panels`), which the vessel
   commands use (`MAKE`, `INSPECT`). Upstream tab names are upper-case and translated by `tr_label`, so
   the tab is `"VESSEL"` rather than `"Vessel"`.
5. `assets/locales/{en,ja,zh}.json` — one line each, `"ui.heading_vessel"`, sorted after
   `ui.heading_utility`. Required by upstream's own tests
   (`every_toolbar_tab_and_panel_is_in_the_english_catalog`, `every_complete_catalog_carries_every_english_message`);
   JSON cannot carry a marker comment, so these are recorded in PATCHES.md only.
6. `xtask/src/layers.rs` — 6 `TABLE` rows (`triya-brand` Standalone, `triya-stdlib` L0, `triya-vessel` L4,
   `triya-drawing` L4, `triya-ui` L5, `triya-web` Exempt) and 2 `INTRA_LAYER_ORDER` chains
   (`engine → triya-vessel → triya-drawing`, `ui-egui → triya-ui`). The rule is "strictly lower layer";
   sitting *beside* engine at L4 (as the task asks) needs the same explicit intra-layer order upstream
   already uses for `gpu → ui-egui`. `triya-drawing` had to be L4 too (it depends on `triya-vessel`).
7. `xtask/src/web_offline.rs` — `template()` prefers `$TRUNK_SOURCE_DIR/sw.js` when it exists, so the
   Trunk `post_build` hook writes `triya-web`'s own `sw.js` (otherwise it always read
   `apps/solidcraft-web/sw.js`, and the Triya build would ship SolidCraft's cache prefix).
8. `Cargo.lock` — regenerated (members only).

## 3. Upstream command ids / params relied on for the cylinder emission (all verified in source and by a passing engine test)

From `crates/engine/src/cmd/sketch.rs` (`create_sketch`, `rect_two_point`, `finish_sketch`, `target_sketch`)
and `features.rs` (`revolve`, `axis_param`, `feature_sketch`, `add_feature_in`), same sequence as
`revolve_convention_tests.rs`:

| step | id | params | reply used |
|---|---|---|---|
| 0 | `sketch.create` | `{"name": "Vessel S1 profile", "plane": {"origin": [0,0,0], "x_dir": [1,0,0], "y_dir": [0,0,1]}}` | `sketch` (feature id) |
| 1 | `sketch.rectangle.two_point` | `{"p0": [r_i, z_from], "p1": [r_o, z_to], "sketch": "$ref:0.sketch"}` (needs `in_sketch`: satisfied right after step 0) | — |
| 2 | `sketch.finish` | `{}` | — |
| 3 | `solid.revolve` | `{"sketch": "$ref:0.sketch", "axis": "Z", "angle": "360 deg", "operation": "new", "name": "Vessel S1", "body_name": "Vessel S1"}` | `feature` |

Notes: the plane is given explicitly, not as `"XZ"`, because `solidcraft_geom::Plane::XZ` has sketch
y = **−Z** (`Plane { x: X, y: (0,0,-1) }`); with `y_dir = +Z` the rectangle reads as (radius, height).
`axis: "Z"` is the world axis (`AxisRef::World`), `"360 deg"` an angle expression (`check_expr` Kind::Angle).
`sketch` accepts a numeric feature id (`target_sketch`/`feature_sketch` match `Value::Number`), which
is what the `$ref` substitution produces. No `// TODO(verify)` was needed: every shape is exercised by
`regenerate_builds_the_cylinder_in_one_undo_step`, which measures the result.

Nested execution: `vessel.regenerate` runs each step as upstream's `selection.delete::sub` does —
`(spec.enabled)(s)?; (spec.run)(s, &params)?; s.refresh()` through the public `CommandSpec` fields
(`run_nested` is `pub(crate)`) — so the outer command owns the single undo step and `Session::execute`'s
restore-on-error; `Session::execute` is *not* used for the nested steps (it would push one undo step
per step; `component.insert_design` does that upstream, which is why its undo is two steps).
Verified: `s.undo.len()` grows by exactly 1 per `vessel.regenerate`, and `edit.undo` brings the
previous cylinder back in one go.

## 4. Where the vessel JSON is persisted

`Document::metadata` (`crates/doc/src/metadata.rs`), key **`in.openengtools.triya.vessel`**, value
`{"schema": "triya.vessel/0.1", "doc": <VesselDoc>, "generated": {"features": [ids]}}`. Upstream
reserves reverse-DNS keys for other tools' objects ("another tool's data goes under its reverse-DNS
key such as `com.example.tool`"), keeps them through load/save and older/newer versions, and a
metadata change is a document change (undoable, marks the design dirty). No format change
(`crates/doc/src/format/pending/`) is needed. Limits are upstream's (`MAX_VALUE_BYTES` = 64 KiB per
key): a larger vessel is refused by `vessel.set`/`vessel.new` with an error. Documented in the `//!` of
`triya/crates/triya-vessel/src/lib.rs`. Round-trip through `Document::to_json`/`from_json` is tested.

## 5. Compile and test results (verbatim tails)

Total build wall time ≈ 21 min (engine `cargo check` 3.5 min, engine full `--profile ci` build 13.6 min
in the background while writing, then checks/tests/clippy/layers ≈ 4 min).

`cargo check -p triya-brand -p triya-stdlib --profile ci`
```
    Checking triya-brand v0.14.0 (/home/claude/work/triya-cad-3d/triya/crates/triya-brand)
    Checking serde v1.0.229
    Checking triya-stdlib v0.14.0 (/home/claude/work/triya-cad-3d/triya/crates/triya-stdlib)
    Finished `ci` profile [optimized + debuginfo] target(s) in 32.67s
```
`cargo check -p triya-drawing -p triya-vessel --profile ci`
```
    Finished `ci` profile [optimized + debuginfo] target(s) in 1m 01s
```
`cargo check -p triya-ui -p triya-web --profile ci`
```
    Finished `ci` profile [optimized + debuginfo] target(s) in 15.64s
```
`cargo test -p triya-brand -p triya-stdlib -p triya-drawing --profile ci` (after one fix: a missing `Revision` import in a test)
```
     Running unittests src/lib.rs (/home/claude/work/target/ci/deps/triya_brand-442a90d6d324c4e0)

running 2 tests
test tests::about_text_names_both_products_and_the_licence ... ok
test tests::app_id_is_reverse_dns ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (/home/claude/work/target/ci/deps/triya_drawing-010e89c8a54284ae)

running 5 tests
test plot::tests::a_polyline_rectangle_also_gives_four_lines_and_arcs_circles_and_text_are_written ... ok
test plot::tests::a_rectangle_plots_as_four_lines ... ok
test plot::tests::a_sheet_plots_its_frame_and_title_block_and_refuses_bad_views ... ok
test plot::tests::model_round_trips_through_json ... ok
test vessel_views::tests::views_are_not_supported_yet_and_invalid_vessels_are_reported_first ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (/home/claude/work/target/ci/deps/triya_stdlib-e9a195ab0465bc6a)

running 9 tests
test head::tests::asme_fd_depth_is_0_1693_d ... ok
test head::tests::hostile_inputs_are_errors_not_panics ... ok
test head::tests::ellipsoidal_2_1_is_a_quarter_of_the_diameter_and_hemispherical_a_half ... ok
test head::tests::klopper_din_28011_depth_matches_the_published_approximation ... ok
test head::tests::korbbogen_din_28013_depth_matches_the_published_approximation ... ok
test head::tests::presets_round_trip_through_json ... ok
test tables::tests::bad_tables_are_errors ... ok
test tables::tests::embedded_tables_load_and_are_marked_illustrative ... ok
test tables::tests::lookups_find_rows_by_key ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
`cargo test -p triya-vessel --profile ci`
```
     Running unittests src/lib.rs (/home/claude/work/target/ci/deps/triya_vessel-42b8b60181bda18c)

running 7 tests
test tests::heads_cones_and_attachments_are_not_supported_yet_but_resolve ... ok
test tests::invalid_documents_are_errors ... ok
test tests::plan_for_a_cylinder_is_sketch_rectangle_finish_revolve ... ok
test tests::regenerate_refuses_attachments_and_leaves_the_design_as_it_was ... ok
test tests::registry_sees_the_vessel_commands_once ... ok
test tests::regenerate_builds_the_cylinder_in_one_undo_step ... ok
test tests::vessel_commands_never_panic_on_hostile_params ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.69s
```
`cargo xtask layers`
```
crate                        layer          workspace deps
…
triya-brand                  L0 standalone
triya-drawing                L4             triya-vessel
triya-stdlib                 L0
triya-ui                     L5             engine, ui-egui, triya-brand, triya-vessel
triya-vessel                 L4             doc, engine, triya-stdlib
triya-web                    exempt         engine, ui-egui, triya-brand, triya-ui
xtask                        exempt

OK: 21 crates, no layering violations.
```
`cargo clippy --no-deps -p triya-* -p triya-web --profile ci --all-targets -- -D warnings`
```
    Finished `ci` profile [optimized + debuginfo] target(s) in 2.25s
```
(after one fix: `manual_range_contains` in `model.rs`). Note: `cargo clippy` *with* deps fails in
upstream's own `crates/kernel/src/prism_local.rs:242` under clippy 1.97 (`manual_range_contains`);
upstream pins Rust 1.95, where this lint does not fire — not an overlay problem.

`cargo fmt --all -- --check`: clean (the overlay crates were formatted with upstream's `rustfmt.toml`).

`cargo test -p triya-ui --profile ci`: see the addendum at the end of this file (it needs the full
ui-egui + wgpu build; started in the background against the 25-minute budget).

Not run: the wasm build (`cd triya/apps/triya-web && trunk build --release`, or the gate
`cargo check -p triya-vessel -p triya-drawing -p triya-stdlib -p triya-brand --target wasm32-unknown-unknown`);
the `wasm32-unknown-unknown` target is not installed here. Nothing in the new crates below L5 uses
anything upstream's wasm-building crates do not (std `RwLock`, serde, f64 math).

## 6. Numerical results of the head-depth checks

Formula `h = L − sqrt((L−r)² − (D/2−r)²)` with inside radii; DIN presets apply their ratios to
`Da = Di + 2s` (`RatioBasis::Outside`), ASME to `Di`:

| case | inputs | computed | published approximation | test tolerance |
|---|---|---|---|---|
| Klöpper DIN 28011 | Da=1000, s=10 → Di=980, R=1000, r=100 | **188.8896** | 0.1935·Da − 0.455·s = 188.95 | ±0.5 (and ±0.01 of 188.889) |
| Korbbogen DIN 28013 | Da=1000, s=10 → Di=980, R=800, r=154 | **248.2573** | 0.255·Da − 0.635·s = 248.65 | ±0.5 (and ±0.01 of 248.26) |
| ASME F&D | L=D, r=0.06D | **0.169338·D** | ≈ 0.1693·D | 5e-5 on h/D (and 1e-6 of 0.169338) |
| 2:1 ellipsoidal | D=1000 | 250.0 (= D/4) | — | exact |

The DIN published formulas are linear fits; the exact geometry differs from them by 0.06 mm
(Klöpper) and 0.39 mm (Korbbogen) at Da = 1000, hence the ±0.5 mm tolerance.

Regenerated cylinder (1000 ID × 10 t × 2000 L): one body `Vessel S1`, volume measured by
`solidcraft_kernel::measure` within 0.1 % of π(510² − 500²)·2000 = 63 460 171.6 mm³, bbox z ∈ [0, 2000],
x_max = 510.

## 7. Open items / TODO(verify)

No `// TODO(verify)` comments remain in the code (every upstream param shape was read in source and
exercised by the engine test). Open items for the next steps:

- **Desktop app** `triya/apps/triya`: not copied (size); route documented in `triya/README.md`.
- **wasm build** of `triya-web` not run (no target installed); the `web-offline` xtask seam
  (`TRUNK_SOURCE_DIR`) is untested against a real Trunk run.
- **triya-ui test** depends on the full ui-egui build (see addendum).
- **Icons**: `triya/apps/triya-web/icons/*` are empty; Triya's own mark must be supplied (and a row in
  ATTRIBUTION.md if the file is not original). No SolidCraft artwork was copied.
- **Catalog labels** for the four vessel commands (`New Vessel`, `Change Vessel`, `Regenerate Vessel`,
  `Inspect Vessel`) are not in `en.json`; upstream's `every_command_label_is_in_the_english_catalog`
  only sees registered commands (the ui-egui tests do not register the overlay), so they stay English
  in every locale until added (4 lines in en/ja/zh).
- **Elements beyond the cylinder** (heads, cones, flat covers, body flanges, skirts) resolve (stations,
  radii, torispherical profile) but are not emitted; attachments are refused. Emission of a head will
  want `sketch.arc.*`/`sketch.line` profiles revolved with `operation: "join"`.
- **Nested `selection.delete`** of the previous regeneration's features relies on upstream deleting
  dependent features with their sketch (documented behaviour; covered by the second regeneration in
  the test, which leaves exactly one body).
- **Tab name** is `"VESSEL"` (upstream convention: upper-case, translated), not `"Vessel"`.
- Upstream `cargo clippy` (with deps) fails under clippy 1.97 on `crates/kernel`; use Rust 1.95 as
  upstream pins, or lint with `--no-deps`.

## Addendum: `cargo test -p triya-ui --profile ci`

Stopped after 6.5 minutes at the 25-minute total build budget, still compiling dependencies
(`solidcraft-sketch`, `vello_common`, `cadmpeg-codec-rhino` were the last lines; wgpu-hal, glow
and naga had just been built). The test binary needs the full `solidcraft-ui-egui` + `solidcraft-gpu`
+ wgpu/egui build, and cargo rebuilt the engine tree because ui-egui's dev-dependencies change the
unified feature set. `cargo check -p triya-ui` and `cargo clippy --no-deps -p triya-ui -D warnings`
both pass, so the crate compiles; its single test (`register_is_idempotent_and_about_names_upstream`,
which also asserts `toolbar::TABS` contains `VESSEL`) has not been executed. Run on a faster machine:

    CARGO_TARGET_DIR=$PWD/target cargo test -p triya-ui --profile ci

Upstream's own `solidcraft-ui-egui` and `solidcraft-engine` test suites were likewise not run here
(same build cost); the seams were written so that `i18n::every_toolbar_tab_and_panel_is_in_the_english_catalog`,
`every_complete_catalog_carries_every_english_message` and `tests::registry_is_consistent` keep
holding (catalog lines added; registration refuses duplicate ids and nothing is registered in those
test binaries).
