# Triya CAD 3D — instructions for agents (overlay rules)

You are working in **Triya CAD 3D**, a rebranded, open-source fork of SolidCraft maintained by
Open Engineering Tools (OET). Upstream's `CLAUDE.md` (the file that imported this one) stays in
force: never crash, everything is a command, layering, thin UI, Rust only, no GPL/LGPL, no
commercial-CAD data, assets in ATTRIBUTION.md, `cargo xtask ci` before every commit. This file
adds Triya's rules and **overrides** the upstream sections named in §9.

## 1. Start every session here
1. `triya/PROGRESS.md` — current phase, next task, open blockers, decisions taken.
2. `triya/PLAN.md` — the dated plan; find the task you are on and its acceptance test.
3. `triya/ARCHITECTURE.md` — where code goes and how data flows; `triya/docs/upstream-map.md`
   for the exact upstream types, command ids and seams; `triya/SKELETON-REPORT.md` for what the
   kickoff skeleton already does.
4. `triya/AGENTS.md` — the autonomous operation protocol (what you decide alone, what waits for
   the owner, how to report).
5. ADRs in `triya/adr/` — decisions already made; do not reopen them in a task, propose a new ADR.

`triya/` is **committed** (unlike upstream's gitignored `plan/`). Keep PROGRESS.md current.

## 2. The overlay rule (most important rule in this fork)
- All Triya code lives under `triya/` (`triya/crates/*`, `triya/apps/*`). Upstream directories
  (`crates/`, `apps/`, `xtask/`, `docs/`, `assets/`, `packaging/`, upstream workflows) are
  **read-only** except at the seams listed in `triya/patches/PATCHES.md`.
- A seam is a one-line (or near one-line) change, marked in the file with
  `Modified by Triya CAD 3D: <why>` in a comment (JSON files: recorded in PATCHES.md only), with
  a row in PATCHES.md and a refreshed `triya/patches/0001-seams.patch`.
  `triya/ci/check-patch-whitelist.sh` fails CI otherwise.
- If a task seems to need a bigger upstream edit, stop and write the alternative that keeps the
  code in `triya/` (wrapper, adapter, copy of a *small* upstream file into `triya/apps/`). If no
  alternative exists, it is an owner decision (AGENTS.md §4) and a candidate upstream PR.
- Never add a `FeatureKind` variant, never touch `crates/doc/src/format/`, never edit
  `crates/kernel`. Triya reaches geometry **only through commands** (ADR-0003).

## 3. Product name and identifiers
Display name **Triya CAD 3D**; organisation **Open Engineering Tools**; constants come from
`triya-brand` (`APP_NAME`, `APP_ID = "in.openengtools.triya"`, URLs, `about_text()`). Rename
**only** what users see: window title, About, PWA manifest, icons, og tags, web `data-bin`,
binary names under `triya/apps/`, data folders, MCP `serverInfo.name`.
**Keep unchanged**: the `.solidcraft` extension, the `"solidcraft/N"` format tag, `solid.*` /
`sketch.*` command ids, `solidcraft-*` crate names, `SOLIDCRAFT_*` env vars. Triya files must
open in unmodified upstream and vice versa (ADR-0006). Never put "SolidCraft" in Triya UI chrome
except the attribution sentence; never put "Triya" into upstream crates (the `triya_extra.rs`
extension slot is the one exception and is named for greppability).

## 4. Geometry: commands, not kernel
- `triya-vessel` turns a `triya.vessel/0.1` JSON document into a **plan of native commands**
  (sketch → revolve → extrude cut/join → circular pattern → coil → parameters) and runs them
  through the engine's normal execution path inside **one undo step**. It never imports `truck`
  or `solidcraft-kernel`.
- Every emitted dimension is a **named user parameter** (`V<elem>_<name>`, e.g. `V101_shell_ID`),
  so users can also edit in upstream's parameter table.
- **Beta kernel rules** (truck is frozen upstream; curved booleans/fillets are its weak spots):
  no fillets on curved edges (knuckles are true arcs in the revolve profile; welds are drawn on
  the 2D sheet, not modelled); after every nozzle cut/join, `measure` the body and compare with
  the analytic volume (tolerance in `triya-vessel::validate`); on failure, keep the nozzle as a
  separate body in a component and set `attachment.flags.boolean_failed = true`. Never let a
  kernel failure fail the whole regenerate.
- Units: millimetres and radians internally (upstream convention); the vessel JSON is mm/deg and
  converted at the boundary. Z is up; vertical vessels stand along +Z; horizontal vessels lie
  along +X with the datum at the left tangent line.

## 5. Drawings: data-driven, native Rust
- `triya-drawing` builds the GA sheet from the vessel model (elevation, plan, section, rule-based
  dimensions, nozzle schedule, BOM, design-data block, notes, title block, revisions), not from a
  hidden-line projection of the solid (ADR-0004). Export via the `Plotter` trait: SVG (also the
  in-app preview, painted with egui/epaint — no DOM JavaScript, upstream's Rust-only rule), DXF
  (upstream's public `solidcraft_io::write_dxf_drawing` first; the `dxf` crate only for missing
  entities; hatch as lines on a `HATCH` layer; dimensions exploded to lines+text in the beta),
  PDF (`printpdf`; if font embedding fails in wasm, fall back to browser print of the SVG via
  `web-sys` and record it in PROGRESS.md).
- Generic mesh-based HLR of arbitrary parts is **post-beta**. Do not start it before the vessel
  sheet is accepted (PLAN.md P2).
- OpenCASCADE/`cadrum` is **not** an option here (LGPL; upstream's and our rule). FreeCAD
  TechDraw and SolveSpace are *design/algorithm references by description only*; never open
  their source.

## 6. Standards data
- Flange/pipe/head tables live in `triya-stdlib/data/*.json` with a `source` and `verified`
  field per row (`docs/standards-library.md`). Seed from `bd_warehouse` (Apache-2.0) for B16.5
  Cl 150/300; hand-key the rest from **two** independent public vendor tables and mark
  `verified: "two-source"`; anything else is `verified: "unverified"` and the UI shows it. The
  skeleton ships `illustrative` rows — replace them; CI forbids `illustrative` outside `tests/`.
- EIL standard sheets are **not public**: the repo ships only `standardRef {org, number, rev}`
  + `sizeKey` fields, open preset tables, and the private-table import. Never commit EIL
  contents or any copyrighted standard table. Never scrape PV Elite/COMPRESS/AutoPIPE data.

## 7. Quality gates (in addition to upstream's)
- Tests first per task: the acceptance test named in PLAN.md exists and fails before the
  implementation lands.
- `cargo xtask ci` green; `cargo test -p triya-stdlib -p triya-vessel -p triya-drawing -p triya-ui`
  green; `bash triya/ci/check-patch-whitelist.sh v0.22.0` green; wasm builds
  (`cargo xtask wasm`); the web app serves (`trunk serve` in `triya/apps/triya-web`) and the
  vessel regenerates in the browser for UI tasks (take a screenshot via `solidcraft-cli snapshot`
  or the control channel and look at it).
- Every vessel command appears in `triya-vessel`'s own hostile-params test (upstream's
  `hostile_params_never_panic` cannot see runtime-registered commands); every element type has an
  analytic-volume test; every drawing export has a golden-file test
  (`triya/crates/triya-drawing/tests/golden/`).
- Add `// SPDX-License-Identifier: MIT OR Apache-2.0` and
  `// Copyright (c) 2026 Triya CAD 3D contributors` to every new Rust file; every new data table
  gets an ATTRIBUTION.md row (source, licence, what was derived); every file with an asset
  extension (png/svg/…) anywhere in the tree needs an ATTRIBUTION row (`cargo xtask assets`).

## 8. Workflow for this fork
- `origin` is `github.com/<you>/triya-cad-3d`; `upstream` is `tangency-labs/solidcraft`.
  Work on branches named `triya/<phase>-<topic>`, open a PR, let `CI` + `Triya CI` run, then
  `gh pr merge --squash --delete-branch`. Never push to `main` directly; never force-push `main`.
- Commit messages: `triya(<crate>): <what>` for overlay code, `seam: <file> — <why>` for the
  rare upstream touch, `plan: …` / `docs: …` for documents. Reference PLAN task ids (`T2.3`).
- **Upstream base: v0.22.0** (merged 2026-10-25, ADR-0002 amendment). Until the tag
  `triya-v0.1.0-beta` exists, merge an upstream release tag only when the owner asks; never
  `upstream/main` between releases. The `upstream-sync` workflow is manual-only until then.
  Recipe: `docs/sync-playbook.md` (incl. §5, re-merging the copied web shell).
- **Cloud sessions** (Claude Code Projects): keep one thread per phase so `target/` survives; run
  `cargo check` + Triya tests while working and the full `cargo xtask ci` once before the PR; GitHub
  Actions is the gate. Tags via `gh api repos/<owner>/<repo>/git/refs -f ref=refs/tags/<tag> -f sha=<sha>`
  (the GitHub proxy rejects tag pushes). See `CLOUD-SETUP.md` at the repo root.
- The upstream "Claim = draft PR" protocol does not apply inside this fork (single owner); it
  does apply when you open a PR on `tangency-labs/solidcraft`.
- Builds: `export CARGO_TARGET_DIR="$PWD/target"`; web: `cd triya/apps/triya-web && trunk serve`
  (port 8782; upstream's web shell uses 8781) or `trunk build --release`; headless: `cargo run -p triya-cli -- run <script.json> --out part.step`
  (the Triya CLI wrapper registers `vessel.*` then delegates to upstream's CLI; plain
  `solidcraft-cli` does not know the vessel commands); MCP: `cargo run -p triya-cli -- mcp`.

## 8a. Upstream facts you will trip over (verified in v0.22.0; details in `docs/upstream-map.md`, v0.22.0 delta first)
- `command_specs()` builds a `Vec` from static per-module arrays; Triya commands enter through
  the extension slot `crates/engine/src/cmd/triya_extra.rs` (`register(&'static [CommandSpec])`,
  idempotent, refuses duplicate ids; `extend()` is called at the end of `command_specs()`). The
  **Triya app shells call `register(triya_vessel::COMMANDS)` at startup** before the first
  `Session` (a static reference would be a Cargo cycle). Consequence: upstream's
  `hostile_params_never_panic` and the ui-egui registry tests never see Triya commands —
  `triya-vessel` has its own sibling tests.
- Composite commands: upstream's `selection.delete::sub` pattern — call `(spec.enabled)(s)` and
  `(spec.run)(s, params)` of other specs from inside your `run`; the outer `Session::execute`
  makes it one undo step (asserted in the skeleton's test). `run_nested` is `pub(crate)`.
  Alternative for scratch-and-accept: `s.scratch()` → `scratch.execute(step)` →
  `scratch.undo.clear()` → `s.accept_computed(scratch)` (`sketch_dimensioned.rs`).
- The ribbon is `ui-egui/src/workspace.rs`: `TABS`, then `SECTIONS` (a tab's panels) and
  `PLACEMENTS` (`.at(tab, panel)` → section). The `VESSEL` tab is a seam there (TABS + four empty
  `SECTIONS` rows + four `PLACEMENTS` rows) plus `ui.heading_vessel` in
  `assets/locales/{en,ja,zh}.json`. Dropping it = move the commands to an existing tab via `.at(…)`.
- Panels and windows go through `ui_kit` (`Container::new(ctx, Surface::…)`, `ui_kit::Window`);
  `frame::window`/`docked_on` no longer exist. Canvases with a camera use `navigation::handle`.
- `Tool::for_command`/`Dialog::for_command` are closed in-crate matches: no external dialog hook.
  Triya panels are drawn by the Triya app shell around `SolveApp::ui` (pub API: `run`, `start`,
  `set_status`, `session`, `ui_kit`, `theme::Tokens`). Programmatic calls never open
  dialogs — our panels call `app.run`, never `app.start`.
- Layers: `xtask/src/layers.rs` `TABLE` is hard-coded with a strictly-lower rule; the skeleton
  registers `triya-brand` (standalone), `triya-stdlib` L0, `triya-vessel` L4 and `triya-drawing`
  L4 beside engine, `triya-ui` L5 beside ui-egui, apps Exempt, with `INTRA_LAYER_ORDER` chains
  `engine → triya-vessel → triya-drawing` and `ui-egui → triya-ui`. Every workspace member must be
  listed or `cargo xtask layers` fails.
- `Document.metadata` is the only sanctioned free-form slot (reverse-DNS key, ≤ 64 KiB/value,
  ≤ 256 KiB total) → `in.openengtools.triya.vessel` (ADR-0005). No `format/pending/` files ever:
  a fork pending file would write `solidcraft/26` and collide with upstream's next format.
- Sketch planes: upstream's named XZ plane has sketch *y* = **−Z** (`geom/plane.rs`); the
  skeleton therefore passes an explicit plane `{origin, x_dir:[1,0,0], y_dir:[0,0,1]}` to
  `sketch.create`. Face/edge references: `[x,y,z]` points or stable names
  `F<feature>:<side>:<curve>` (`doc/src/naming.rs`). Emitted steps may reference earlier results
  with `$ref:<step>.<field>` (skeleton's `emit.rs`).
- `xtask web-offline` read `apps/solidcraft-web/sw.js` unconditionally; the skeleton's seam
  prefers `$TRUNK_SOURCE_DIR/sw.js`, so `triya/apps/triya-web/sw.js` gets its own asset list.
  Upstream `pages.yml` hard-codes `/solidcraft/app/`; ours is `triya-pages.yml`.
- `deny.toml` has no licence allowlist (policy is prose) but `cargo deny` denies **git
  dependencies** — add crates from crates.io only. Upstream pins Rust 1.95; clippy on a newer
  toolchain may flag upstream code (`kernel/prism_local.rs` under 1.97) — not ours to fix; use
  the pinned toolchain in CI.
- The desktop update checker polls `bherbruck/solidcraft` releases unless `Services::http_get`
  is `None`; the agent-chat panel looks for a sibling `solidcraft-cli` binary. Triya shells pass
  `None` and ship the CLI alongside. The web shell keeps upstream's element ids in `index.html`
  because the shared `loading.css` styles them.
- `solidcraft_io::write_dxf_drawing` (+ `DxfLayer/DxfEntity/DxfShape`) is public: evaluate it
  before adding the `dxf` crate.
- The `skills/` folder: CI `include_str!`s exactly seven upstream files, so `skills/triya-*` can
  be added freely; `plan/` and `dist/` are gitignored upstream — never name a Triya folder `plan`.

## 9. Upstream sections this file overrides
| Upstream `CLAUDE.md` section | In this fork |
|---|---|
| Product name | §3 above (display name Triya; identifiers kept) |
| Start every session here (`plan/…`) | §1 above (`triya/PROGRESS.md`, `PLAN.md`, `ARCHITECTURE.md`) |
| Publishing (`origin` = bherbruck) | §8 above |
| Claim = a draft PR | Not used inside the fork |
| Roadmap (`ROADMAP.md`) | `triya/PLAN.md` + `PROGRESS.md`; never edit upstream `ROADMAP.md` |
| Application releases | Tags `triya-vX.Y.Z[-beta]`; the Pages deploy is `triya-pages.yml`; upstream release workflows are disabled in the fork |
| Design file format | Unchanged rule, plus: Triya never adds a pending format file (ADR-0005) |

## 10. When to stop and ask the owner
Only for the items in `triya/AGENTS.md` §4 (irreversible or product-defining). Everything else:
decide, record the decision in PROGRESS.md "Decisions", continue.
