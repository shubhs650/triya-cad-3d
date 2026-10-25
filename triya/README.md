# Triya CAD 3D overlay

Triya CAD 3D is a downstream fork of [SolidCraft](https://github.com/tangency-labs/solidcraft)
(v0.22.0, commit `0537623`, MIT OR Apache-2.0, © SolidCraft contributors; not affiliated with the
SolidCraft project), published by Open Engineering Tools (https://openengtools.com).

Everything of Triya's lives under `triya/`; upstream is touched only at the seams listed in
`patches/PATCHES.md`, each marked `// Modified by Triya CAD 3D`. Upstream's rules apply
unchanged (`CLAUDE.md`, `AGENTS.md`, `CONTRIBUTING.md`): no panics outside tests, everything is a
command, the UI is thin, `cargo xtask layers` passes, wasm32 builds.

## Crates (all `MIT OR Apache-2.0`)

| crate | layer | what |
|---|---|---|
| `crates/triya-brand` | L0 standalone | product name, ids, upstream attribution (`about_text()`) |
| `crates/triya-stdlib` | L0 | typed flange / pipe-schedule / head-preset rows, `StandardRef`, `TableSet` JSON loader, head depth formulas, support presets; `data/*.json` are ASME B16.5/B16.47/B36.10M/B36.19M tables with per-row `source` and `verified` (`docs/standards-library.md`) |
| `crates/triya-vessel` | L4 (beside engine) | `VesselDoc` model (`triya.vessel/0.1`), `resolve()` → axial stations, `plan()` → engine command calls, the `vessel.*` commands |
| `crates/triya-drawing` | L4 | GA sheet model built from the vessel (views, dimensions, tables, title block), `Plotter` trait with SVG, DXF and PDF writers, the `drawing.*` commands |
| `crates/triya-ui` | L5 (beside ui-egui) | `register()` the commands and the product name, the Vessel panel (stack, attachments, plan, drawing, JSON), the About window |
| `apps/triya-cli` | app | upstream's `solidcraft-cli` source compiled with `vessel.*` registered first (`build.rs` rewrites only `main` → `upstream_main` and module paths); `run`, `eval`, `snapshot`, `commands`, `mcp`, … all know the vessel commands |
| `apps/triya-web` | app | copy of `apps/solidcraft-web` with Triya branding and the overlay wired in |

## Commands

`vessel.*` (new, set, element/attachment/support add, datum, import_tables, regenerate, bom,
bom.export, inspect) and `drawing.*` (generate_ga, export) on the `VESSEL` tab (panels CREATE,
INSERT, MODIFY, INSPECT); `triya-cli commands` lists them with their parameters. The vessel JSON
(`triya.vessel/0.1`, `docs/vessel-schema.md`) and the sheet options live in the design's metadata
(`in.openengtools.triya.vessel`, `in.openengtools.triya.sheet`), so they are saved in the
`.solidcraft` file and take part in undo without a design-format change. `vessel.regenerate`
plans native commands (sketch → revolve → extrude cut/join → pattern) and runs them as **one undo
step**; the sheet is drawn from the vessel model, not from the solid (ADR-0004).

## Build

```sh
export CARGO_TARGET_DIR=$PWD/target
cargo check -p triya-brand -p triya-stdlib -p triya-drawing -p triya-vessel -p triya-ui -p triya-web --profile ci
cargo test  -p triya-brand -p triya-stdlib -p triya-drawing -p triya-vessel -p triya-ui --profile ci
cargo xtask layers
cargo run -p triya-cli -- run triya/crates/triya-vessel/examples/cylinder.json --out /tmp/cyl.step
cd triya/apps/triya-web && trunk serve --release        # browser build (wasm32)
bash triya/ci/upstream-roundtrip.sh                    # a Triya design opens in unmodified v0.22.0
cargo check -p triya-vessel --target wasm32-unknown-unknown   # the wasm gate for the lower crates
```

## Desktop app

`triya/apps/triya` (`cargo run -p triya -- [--sample] [--control PORT] [files…]`) is a thin shell
around upstream's `SolveApp`, not a copy of the ~2,700-line `apps/solidcraft`. It registers the
`vessel.*` commands, draws the Vessel panel and Triya's About window around `app.ui`, uses the
Triya title, icon, app id and settings folder, never checks for SolidCraft updates, and compiles
upstream's control server (`apps/solidcraft/src/control_server.rs`) unchanged. Upstream's
Windows crash filter and resize border, macOS Finder events, Space Mouse and safe-mode counter
are left out for the beta.
