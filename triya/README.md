# Triya CAD 3D overlay

Triya CAD 3D is a downstream fork of [SolidCraft](https://github.com/bherbruck/solidcraft)
(v0.14.0, commit `2bb9c81`, MIT OR Apache-2.0, © SolidCraft contributors; not affiliated with the
SolidCraft project), published by Open Engineering Tools (https://openengtools.com).

Everything of Triya's lives under `triya/`; upstream is touched only at the seams listed in
`patches/PATCHES.md`, each marked `// Modified by Triya CAD 3D`. Upstream's rules apply
unchanged (`CLAUDE.md`, `AGENTS.md`, `CONTRIBUTING.md`): no panics outside tests, everything is a
command, the UI is thin, `cargo xtask layers` passes, wasm32 builds.

## Crates (all `MIT OR Apache-2.0`)

| crate | layer | what |
|---|---|---|
| `crates/triya-brand` | L0 standalone | product name, ids, upstream attribution (`about_text()`) |
| `crates/triya-stdlib` | L0 | typed flange / pipe-schedule / head-preset rows, `StandardRef`, `TableSet` JSON loader, head depth formulas. `data/*.json` are **illustrative placeholders**, not standard tables |
| `crates/triya-vessel` | L4 (beside engine) | `VesselDoc` model (`triya.vessel/0.1`), `resolve()` → axial stations, `plan()` → engine command calls, the `vessel.*` commands |
| `crates/triya-drawing` | L4 | 2D sheet model, `Plotter` trait, `SvgPlotter`; vessel views stubbed |
| `crates/triya-ui` | L5 (beside ui-egui) | `register()` the commands, the `VesselPanel` window, About text |
| `apps/triya-web` | app | copy of `apps/solidcraft-web` with Triya branding and the overlay wired in |

## Commands

`vessel.new`, `vessel.set`, `vessel.regenerate`, `vessel.inspect` (tab `VESSEL`, panels
`MAKE` / `INSPECT`). The vessel JSON is stored in the design's metadata under
`in.openengtools.triya.vessel`, so it is saved with the `.solidcraft` file and takes part in undo
without a design-format change. `vessel.regenerate` runs the planned upstream commands
(`sketch.create` → `sketch.rectangle.two_point` → `sketch.finish` → `solid.revolve`) inside its
own run, so the whole regeneration is **one undo step**; only cylinders are built so far, every
other element and all attachments return "not supported yet".

## Build

```sh
export CARGO_TARGET_DIR=$PWD/target
cargo check -p triya-brand -p triya-stdlib -p triya-drawing -p triya-vessel -p triya-ui -p triya-web --profile ci
cargo test  -p triya-brand -p triya-stdlib -p triya-drawing -p triya-vessel -p triya-ui --profile ci
cargo xtask layers
cd triya/apps/triya-web && trunk serve --release        # browser build (wasm32), not run here
cargo check -p triya-vessel --target wasm32-unknown-unknown   # the wasm gate for the lower crates
```

## Desktop app

`apps/solidcraft` is ~2 700 lines (control server, crash capture, native frame, Space Mouse,
startup checks) and not a small copy, so there is no `triya/apps/triya` yet. The intended route:
copy `apps/solidcraft` as `triya/apps/triya`, call `triya_ui::register()` before the first
`Session`, draw `triya_ui::VesselPanel` after `app.ui(ui)` (as `triya-web/src/main.rs` does), and
take `APP_NAME`/`APP_ID` from `triya-brand` for the window title, settings and crash folders.
