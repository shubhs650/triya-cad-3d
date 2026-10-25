# Upstream map: SolidCraft for the Triya CAD 3D fork

## v0.22.0 delta (base since 2026-10-25)

The fork merged upstream `v0.22.0` (commit `0537623`, 873 commits after v0.14.0). The v0.14.0
map below is still the reference for everything not listed here; where they disagree, this
section wins.

| Area | v0.14.0 (below) | v0.22.0 |
|---|---|---|
| Format | `solidcraft/25` | `solidcraft/35` (`crates/doc/src/format/v35/`); `Document.metadata` unchanged, so the vessel JSON (ADR-0005) is untouched. Upstream migrates older files on open |
| Toolbar tabs | `toolbar.rs` `TABS` = SOLID, SURFACE, MESH, SHEET METAL, PLASTIC, UTILITIES; `panels()` match | Ribbon in `ui-egui/src/workspace.rs`: `TABS` = MODEL, SURFACE, ASSEMBLE, INSERT, INSPECT, MANAGE, VIEW (+ SKETCH, SHEET METAL contextual). A tab's panels are the `SECTIONS` rows naming it; a command reaches a section by id (`SECTIONS.ids`) or by its `.at(tab, panel)` mapped through `PLACEMENTS`. `toolbar::TABS` is a re-export. Seams 6/15 live in `workspace.rs` |
| Panels and windows | `frame::window`, `frame::docked_on`, `frame::dock_frame`, `frame::Width` | Gone. Every outer container goes through `ui_kit` (`Container::new(ctx, Surface::…)`, `ui_kit::Window`, `Panel`, `Confirm`, `Menu`); `cargo xtask ui-containers` enforces it in `crates/` and `apps/` (not `triya/`, but Triya follows it). The Vessel panel is a `Surface::Panel` area; the Triya About window is a `ui_kit::Window` |
| Camera input | — | Every canvas with a camera calls `navigation::handle` (upstream rule) |
| Icons | Code-drawn `icons.rs` | SVG masters in `assets/icons/ui/design/svg/{model,small,flat}/`, compiled by `icon_svg.rs`; every icon name the vessel commands use still resolves |
| Help | `help::FONTS` | Fonts text is the catalog message `help.font_details` |
| Start page | heading painted the product name | `FileView` page; the product name appears only in the `home:about` row (seam 18) |
| Web shell | — | `apps/solidcraft-web` gained `disk.rs` (File System Access API: Save/Open real files in Chrome/Edge), `api.rs` (promise API over the control channel), `SolveApp::with_control`, loading fade |
| New crates | — | `crates/threedx-client` (`solidcraft-3dx-client`, standalone; 3Dconnexion driver client) |
| CI | full suite on every PR | heavy jobs need the `approved-for-ci` label on the PR; merge-queue aware. New bot workflows (`triage`, `claim`, `kernel-paused`, `docker`, `generated`) are disabled in the fork |
| Kernel | truck | still truck (~115 fixes: booleans, revolve to the axis along a curve, hollow tools); upstream is evaluating a new kernel and has paused kernel work |
| Command ids | — | all 20 ids the vessel plan emits exist unchanged |


## v0.14.0 map (original)

Source read: `/home/claude/upstream/solidcraft`, tag `v0.14.0`, commit `2bb9c81` ("Release 0.14.0").
Workspace: `Cargo.toml:3` `members = ["crates/*", "apps/*", "xtask"]`, `exclude = ["vendor"]`,
edition 2024, `rust-version = "1.95"`, `[patch.crates-io]` at `Cargo.toml:135-144` (vendored
`truck-*`, `matext4cgmath`, `egui-wgpu`: any separate downstream workspace must copy this block).
Every file:line below was checked against that tree. "Seam" = an upstream file the fork must edit.

Downstream crates assumed: `triya-vessel` (parametric command generator), `triya-standards`,
`triya-drawing` (2D DXF generator), `triya-ui` (egui panels), apps `triya`, `triya-cli`, `triya-web`.

---

## 1. Command registry (`crates/engine/src/cmd/mod.rs`)

### 1.1 `CommandSpec` (mod.rs:86-116)

```rust
pub type Run = fn(&mut Session, &Value) -> Result<Value>;                       // :86
pub type LiveRun = fn(&mut Session, &Value) -> Result<(Value, Option<crate::LiveEdit>)>; // :90
pub type Enabled = fn(&Session) -> std::result::Result<(), String>;             // :91
pub struct CommandSpec {                                                        // :93
    pub id: &'static str,
    pub label: &'static str,
    pub tab: &'static str,      // Toolbar tab (`SOLID`, `SKETCH`…); empty = not on the toolbar.
    pub panel: &'static str,    // Toolbar panel (`CREATE`, `MODIFY`…).
    pub icon: &'static str,     // Icon name for the UI (drawn in code).
    pub shortcut: Option<&'static str>,
    pub params: &'static str,   // One-line parameter documentation.
    pub enabled: Enabled,
    pub run: Run,
    pub undoable: bool,
    pub live: Option<LiveRun>,  // runs instead of `run` when params carry `live: true`
    pub reads_model: bool,      // false (`doc_only()`): may run on a stale model without evaluating
    pub planned: Option<&'static str>, // GitHub issue number; registered but not built
}
```

All fields are `pub`; `run` is a plain `fn` pointer (no closures, no state). Builder is `const`
(mod.rs:119-175): `new(id,label,run)`, `.planned(issue)`, `.live(run)`, `.doc_only()`,
`.at(tab,panel)`, `.icon(i)`, `.key(k)`, `.params(p)`, `.enabled(e)`, `.noundo()`. Defaults from
`new`: `tab=""`, `panel=""`, `icon=""`, `shortcut=None`, `params=""`, `enabled=always`,
`undoable=true`, `live=None`, `reads_model=true`, `planned=None`.

`CommandInfo` (mod.rs:199-212) is the serialisable view (`engine.commands`, MCP `list_commands`,
`solidcraft-cli commands`): id, label, tab, panel, icon, shortcut, params, `enabled: bool`,
`disabled_reason: Option<String>`, `planned`. Produced by `CommandSpec::info(&Session)` (:182).

Helpers for `enabled`: `always` (:214), `in_sketch` (:218), `not_in_sketch` (:224). Macro
`planned!(id,label,issue)` (:15-21) registers a greyed, refusing command.

How the doc fields are used:
- `tab`/`panel`: `crates/ui-egui/src/toolbar_custom.rs:64-77` (`panel_lists`) filters
  `command_specs()` by `(c.tab, c.panel)`; palette/keymap/shortcut-box list every command
  regardless (`palette.rs:21`, `keymap.rs:103,239`, `shortcut_box.rs:105`). `xtask features`
  prints tab/panel in `docs/features.md`. MCP `list_commands` filter matches id/label/panel/tab
  (`crates/mcp/src/tools.rs:379`).
- `icon`: `crates/ui-egui/src/icons.rs:4` re-exports `paint` from
  `assets/icons/ui/source/icons.rs:135`; unknown names draw a plain square (`icons.rs:1462-1464`).
- `shortcut`: `crates/ui-egui/src/keymap.rs` (user overrides on top).
- `params`: documentation only; the engine never parses it. Tests
  `crates/engine/src/tests.rs:3111` and `:3129` grep it for known keys (`name?`, `body_name?`,
  `targets?`) on the upstream feature commands only.
- `enabled`: checked in `Session::execute` (`lib.rs:641`) and `SolveApp::start` (`lib.rs:696-702`).

### 1.2 `run` contract: what a command receives

`Session::execute(id, params)` (`crates/engine/src/lib.rs:633-721`):

```rust
let spec = find_command(id).ok_or_else(|| EngineError::UnknownCommand(id.to_string()))?; // :634
if live.is_none() && spec.reads_model && self.model.is_stale() { self.refresh(); }         // :638
(spec.enabled)(self).map_err(|why| EngineError::Disabled(spec.id.to_string(), why))?;      // :641
let before = Snapshot { label, doc: self.doc.clone(), active_sketch, pending_moves };      // :642
let (_nested, outermost) = frames::Nested::enter(spec.id);                                 // :653
let mapped = if outermost { frames::to_local(self, spec.id, params) } else { None };       // :654
let r = std::panic::catch_unwind(AssertUnwindSafe(|| (spec.run)(self, params)...));        // :660
// Ok + doc changed: undo.push(before) when spec.undoable; redo.clear(); refresh()        // :689-709
// Err: self.doc = before.doc; active_sketch/pending_moves restored; refresh()            // :711-719
```

- Params are `&serde_json::Value`; `null` becomes `{}` (:648-649). Read them with
  `solidcraft_engine::params::{bad, num, str_, bool_, vec2, vec3, req_vec2, req_vec3,
  vec2_list, expr, req_expr, string_list, finite}` (`crates/engine/src/params.rs:9-74`, all `pub`).
- The command gets `&mut Session` (`lib.rs:196-254`): `doc: Arc<Document>` (mutate through
  `doc_mut()` → `Arc::make_mut`, :456), `model: Model`, `undo/redo`, `active_sketch`, `selection`,
  `active_component`, `pending_moves`, `visibility`, `camera`, `log`. Evaluated geometry:
  `s.model.state()` → `Arc<ModelState>` (bodies, sketches); placed: `s.world_state()`.
- Transaction: the whole `run` is one undo step (`Snapshot`, `lib.rs:152-157`) and is atomic on
  error or panic (`EngineError::Internal`, :667-670). There is no explicit begin/commit API.
- Document mutations via `Document` API (`crates/doc/src/document.rs`): `add_feature(kind, name)`
  (:2645), `insert_feature(f, params)` (:2685), `find_feature(&str)` (:2590, id or name).
- World→component mapping of picks happens only for the outermost command and only for ids listed
  in `crates/engine/src/frames.rs:46` (`table(id)`); a downstream id is passed through unmapped
  (identity placements: no difference).

### 1.3 Assembly of the registry and the downstream seam

```rust
pub fn command_specs() -> Vec<&'static CommandSpec> {     // mod.rs:229
    let mut v: Vec<&'static CommandSpec> = Vec::new();
    v.extend(sketch::COMMANDS.iter());                       // :231  … 42 modules …
    v.extend(render::COMMANDS.iter());                       // :273
    v                                                        // :274
}
pub fn find_command(id: &str) -> Option<&'static CommandSpec> // :277 (exact, then ASCII-case-insensitive)
```

Each module exposes `pub static COMMANDS: &[CommandSpec] = &[ ... ];` (383 ids in v0.14.0,
`registry_is_consistent` at `tests.rs:830` rejects duplicate ids). There is **no** `inventory`/
`linkme` registry, no feature flag, no runtime `Vec` builder: `command_specs()` is a fresh `Vec`
built on every call (every `execute`, `find_command`, toolbar frame).

Cargo forbids a static `v.extend(triya_vessel::COMMANDS)` here: `triya-vessel` must depend on
`solidcraft-engine` for `Session`/`Run`, so engine cannot depend on it (cycle). The minimal seam
is therefore a process-wide extension list in this file, anchored between **mod.rs:273 and :274**:

```rust
// mod.rs, after `v.extend(render::COMMANDS.iter());` (line 273):
    v.extend(EXTRA.get().into_iter().flatten().flat_map(|c| c.iter()));
// plus, near line 228:
static EXTRA: std::sync::OnceLock<Vec<&'static [CommandSpec]>> = std::sync::OnceLock::new();
/// Downstream crates' commands (set once, before the first Session).
pub fn register_extra(specs: Vec<&'static [CommandSpec]>) -> Result<(), String> {
    EXTRA.set(specs).map_err(|_| "commands already registered".into())
}
```

The Triya apps call `solidcraft_engine::cmd::register_extra(vec![triya_vessel::COMMANDS, …])` in
`main()` before `Session::default()`. Everything downstream of `command_specs()` (toolbar,
palette, `engine.commands`, MCP `execute`/`batch`, CLI `run`, control channel) then sees them.
Ids must follow `<area>.<thing>[.<variant>]` lower-case dotted (`CLAUDE.md` "Everything is a
command"); use a Triya area, e.g. `vessel.create`, `vessel.nozzle`, `drawing.generate`.

Alternative with zero engine edits: the Triya app wraps `Session::execute` itself (its own
dispatcher tries `triya_vessel::find(id)` first). It loses toolbar/palette/MCP/control-channel
integration and is not recommended.

---

## 2. Toolbar, ribbon, dialogs, brand strings (`crates/ui-egui`)

### 2.1 Tabs and panels: hard-coded lists, commands from the registry

- Tabs: `toolbar.rs:11` `pub const TABS: &[&str] = &["SOLID", "SURFACE", "MESH", "SHEET METAL",
  "PLASTIC", "UTILITIES"];` drawn at `toolbar.rs:616-650` (`SKETCH` added while sketching,
  `RENDER` replaces them in the render workspace). **A new tab name in a `CommandSpec` does not
  create a tab**; commands on an unknown tab are reachable only through the palette.
- Panels per tab: `toolbar.rs:49-63` (`panels(tab)`), overridden first by
  `workspace.rs:96-102` (`SOLID` adds `CONFIGURE`, `PLASTIC` adds `SETUP`). Fallback arm
  `toolbar.rs:61`: `_ => &["MAKE", "ADD-INS", "UTILITY", "INSPECT", "SELECT"]` (this is what
  `UTILITIES` shows; no upstream command is registered there).
- Panel contents: `toolbar_custom.rs:64-77`:

```rust
pub fn panel_lists(tab: &str) -> Vec<(&'static str, Vec<&'static CommandSpec>, usize)> {
    let specs = command_specs();
    crate::toolbar::panels(tab).iter().map(|panel| {
        let (cmds, promote) = match crate::workspace::layout(tab, panel, &specs) {
            Some(l) => l,
            None => (specs.iter().filter(|c| c.tab == tab && c.panel == *panel).copied().collect(),
                     crate::toolbar::promoted(tab, panel)),
        };
        let cmds = crate::sketch_tools::also_in(tab, panel, cmds, &specs);
        (*panel, cmds, promote)
    }).collect()
}
```

  `workspace::layout` (`workspace.rs:106-135`) fixes explicit orders only for `SKETCH/CREATE`,
  `SOLID/CONFIGURE`, `SKETCH/PATTERN`, `*/ASSEMBLE`, `SHEET METAL/*`, `PLASTIC/*`; everything
  else (notably `SOLID/CREATE`, `SOLID/MODIFY`, `SOLID/INSPECT`, `SOLID/INSERT`,
  `UTILITIES/ADD-INS`) is registry-driven. **A downstream `.at("SOLID", "CREATE")` or
  `.at("UTILITIES", "ADD-INS")` appears with no UI edit**, after upstream commands; `promoted()`
  (`toolbar.rs:67-76`) gives the first N big buttons (4 for SOLID/CREATE, 2 elsewhere), the rest
  go into the panel drop-down.
- A Triya tab needs two edits: `TABS` (`toolbar.rs:11`) and a `"VESSEL" => &[...]` arm in
  `panels()` (`toolbar.rs:53-62`), plus `en.json` entries for the new words (test
  `i18n.rs:259-271` `every_toolbar_tab_and_panel_is_in_the_english_catalog` runs over
  `command_specs()` — only the commands registered in the ui-egui test binary, i.e. upstream's).

### 2.2 Starting a tool or dialog

Toolbar button → `app.start(c.id)` (`toolbar.rs:798, 933, 971, 1224`). Control channel
`ui.start {command}` → `app.start(id)` (`control.rs:370-377`; `params` refused except
`joint.drive`). `SolveApp::start` (`lib.rs:688-847`) in order: refuses disabled commands (status
line), special cases by id (`sketch.construction`, `parameters.change` → `params_dialog`,
`document.metadata*` → `design_properties`, `sketch.finish`, file pickers), then

```rust
if let Some(mut t) = tools::Tool::for_command(id) { ... self.tool = Some(t); return; }     // :783-837
if let Some(d) = dialogs::Dialog::for_command(self, id) { self.dialog = Some(d); return; } // :838-845
let _ = self.run(id, json!({}));                                                            // :846
```

`tools::Tool::for_command` and `dialogs::Dialog::for_command` (`dialogs.rs:441`, a `match id`
over `dialogs::Kind` enum at `dialogs.rs:35`, `Dialog` struct `:307-321`) are closed in-crate
matches; `dialogs_*::start(app, id)` helpers (e.g. `dialogs_sheet.rs:150`) are also in-crate.
There is no trait, registry or hook for external dialogs. **A downstream command clicked on the
ribbon runs with `{}` params.** `app.run(id, params)` (`lib.rs:607`) is the public programmatic
entry (`Result<Value, String>`, updates camera on `view.navigate`, status line on error).

How a fork adds a side panel/dialog without touching ui-egui: the desktop app is a thin wrapper
(`apps/solidcraft/src/main.rs:45-100`: `impl eframe::App for App { fn ui(..) { self.solve.ui(ui) } }`).
A Triya host app draws its own egui UI in the same `ui()` before/after `self.solve.ui(ui)`
(`SolveApp::ui` at `lib.rs:1291`; layout in `cad_ui` `lib.rs:1321-1380`: app bar, toolbar,
timeline, palette bar, browser `SidePanel`, `CentralPanel` viewport at :1358, then dialogs).
Public pieces a host crate may use: `SolveApp { pub session, pub ui: UiState, pub tool, pub dialog,
pub preferences, … }`, `run`, `start`, `set_status` (:954), `fit_view` (:1010),
`solidcraft_ui_egui::frame::{window, docked, Width}` (`frame.rs:67, 154`),
`theme::Tokens::get()` (`theme.rs:294`), `pub mod toolbar/dialogs/i18n/icons/...` (`lib.rs:11-177`).
`frame.rs:328-341` test `windows_come_from_the_frame_helper` scans only `crates/ui-egui/src`, so
a fork crate may call `egui::Window::new` (prefer `frame::window` for width limits).
To open the Triya panel from a ribbon button, a one-line seam at `lib.rs:846` (host hook before
the final `self.run(id, json!({}))`, e.g. `pub start_hook: Option<fn(&mut SolveApp,&str)->bool>`)
is avoidable by giving the Triya panel its own button in the host-drawn UI, or by letting
`vessel.open` be a command whose `run` only sets a flag the host reads (e.g. in `Session::log`).
Live preview for a Triya dialog: `Session::preview(&[(id, params)])` (`lib.rs:870`) evaluates on
a scratch copy and returns `before/after` `ModelState`.

### 2.3 Brand strings (per `docs/compatibility.md`: shims keep the old name; everything else is
"SolidCraft")

**Display-only (safe to rebrand in a fork):**

| File:line | Text |
|---|---|
| `apps/solidcraft/src/main.rs:421` | `.with_title("SolidCraft")` (window title) |
| `apps/solidcraft/src/main.rs:143,163,165` | file dialog filter names "SolidCraft design…", "SolidCraft container" |
| `apps/solidcraft/src/main.rs:250,315` | HTTP user agents `SolidCraft`, `SolidCraft-update-check` |
| `apps/solidcraft/src/main.rs:278` | keyring entry "SolidCraft agent chat" |
| `apps/solidcraft/src/main.rs:546,561,570,672,686,688,698` | log lines / "could not start" dialog title |
| `apps/solidcraft/src/startup.rs:177-185` | no-window message text and docs URL |
| `crates/ui-egui/src/home.rs:359` | start page wordmark `RichText::new("SolidCraft")` |
| `crates/ui-egui/src/home.rs:410-411` | Report issue / What's new links (`github.com/bherbruck/solidcraft`) |
| `crates/ui-egui/src/help.rs:14` | `DOCS_URL = "https://bherbruck.github.io/solidcraft/"` |
| `crates/ui-egui/src/help.rs:21,35,75,106,111` | About text, "About SolidCraft" menu item/window title, About wordmark |
| `crates/ui-egui/src/issue_report.rs:13,301-302,339` | `ISSUE_URL`, crash messages |
| `crates/ui-egui/src/crash_ui.rs:53` | status line after a crash |
| `crates/ui-egui/src/edit_clipboard.rs:64,77,95` | clipboard text descriptions |
| `crates/ui-egui/src/file_load.rs:326,352,703` | timing log lines |
| `crates/ui-egui/src/update.rs:27-33` | `REPO`, `LATEST_API`, `RELEASES_PAGE`, `RELEASE_PREFIX` (update feed; point at the fork or disable `Services::http_get`) |
| `crates/ui-egui/src/update.rs:311,375`, `update/install.rs:114,214-215,307` | release asset names, "SolidCraft's release key" |
| `crates/ui-egui/src/agent_chat/acp.rs:43,60,69-73`, `acp_client.rs:137`, `api_client.rs:182-218` | agent chat clientInfo/system note |
| `crates/mcp/src/server.rs:145` | `"serverInfo": {"name": "solidcraft", "title": "SolidCraft", …}` (title display; name is an identifier, see below) |
| `apps/solidcraft-web/index.html:6-21,38` | `<title>`, description, og:*/twitter:* meta, "Loading SolidCraft…" |
| `apps/solidcraft-web/manifest.webmanifest:2-4` | name, short_name, description |
| `apps/solidcraft-cli/src/main.rs:1-70` | usage text |

**Identifiers to keep (compatibility with upstream designs, settings and tooling):**

| File:line | Identifier | Why keep |
|---|---|---|
| `crates/doc/src/format.rs:331-333` | `format_string(n) = "solidcraft/{n}"`; `version_of` (:340) also reads `solvecraft/N` | opening upstream `.solidcraft` files; a fork tag means a fork format (see §4) |
| `crates/io/src/lib.rs` `DESIGN_EXT`, `LEGACY_DESIGN_EXT`; `crates/io/src/design.rs` container marker `solidcraft-container/1` | file extension and container magic | interchange with upstream |
| `crates/engine/src/crash.rs` `"solidcraft-crash/1"` (seen in `crash_ui.rs:161`) | crash report format | `solidcraft-cli crashes` tooling |
| `crates/ui-egui/src/prefs.rs:17` `STORAGE_KEY = "solidcraft.prefs"`; `apps/solidcraft/src/main.rs:340` `eframe::storage_dir("SolidCraft")`; `home.rs:90` thumbnails dir | settings/recovery folders | a fork that changes them must port `adopt_legacy_dir` logic (`docs/compatibility.md` §4) |
| `crates/ui-egui/src/update.rs:53` `APP_ID = "ai.storyteller.solidcraft"`; `apps/solidcraft/src/main.rs:430` `.with_app_id(APP_ID)`; `assets/platforms/linux/ai.storyteller.solidcraft.*`; `flake.nix:138-143` | desktop/Flatpak app id, icon names | change only together with all packaging files (`assets/platforms/**`, `packaging/**`) |
| `crates/mcp/src/server.rs:145` `"name": "solidcraft"`; `:164-165,174-175` `solidcraft://design`, `solidcraft://commands` (+ `solvecraft://` shim) | MCP server name and resource URIs | the `skills/` folder and agent configs (`mcp__solidcraft__*`, `agent_chat/gate.rs:40`, `agents.rs:225`) name them |
| `solidcraft_geom::env` `SOLIDCRAFT_*` env vars (`docs/compatibility.md` §3), `SOLIDCRAFT_CONTROL_TOKEN`, `SOLIDCRAFT_GPU` | runtime configuration | CI scripts, docs |
| `apps/solidcraft-web/src/main.rs:28-29` `solidcraft_canvas`, `solidcraft_loading`; `:421` `solidcraft.webgl`; `install.rs:11` `solidcraftInstall`; `files.rs:24,93` `recovery.solidcraft`, `solidcraft.file.`; `sw.js:33,36` cache prefixes `solidcraft-app …` | web runtime ids and storage keys | `index.html` and `sw.js` must agree with them; renaming loses users' browser-stored designs |
| crate names `solidcraft-*`, bins `solidcraft`, `solidcraft-cli`, `solvecraft-cli` alias (`apps/solidcraft-cli/Cargo.toml:11-19`) | Cargo identities | `xtask/src/layers.rs:34-56` `short_name` strips `solidcraft-`; `licences.rs:18` `ROOTS` |

`docs/compatibility.md` rule: `git grep -i solvecraft` must list nothing but the shims it
enumerates. A fork introducing "Triya" strings should keep that file's discipline (a new
`docs/compatibility.md` section listing every brand string) rather than a blanket rename.

---

## 3. Programmatic modelling API

All commands take JSON params; results are JSON. Lengths are mm and angles rad internally; param
expressions accept units (`"50 mm"`, `"2 in"`, `"30 deg"`) and parameter names (`"flange_r"`,
`"thickness + 14 mm"`). A bare number in a length param means the design's display unit
(`sketch.dimension` doc, `sketch.rs:114`; `features.rs:397-401` `with_design_unit`). Z is up.

### 3.1 Planes (`crates/geom/src/plane.rs:17-23`)

```rust
pub const XY: Plane = Plane { origin: ZERO, x: X, y: Y };                  // normal +Z
pub const XZ: Plane = Plane { origin: ZERO, x: X, y: Vec3::new(0,0,-1) };  // Front: sketch y = -Z, normal +Y
pub const YZ: Plane = Plane { origin: ZERO, x: Y, y: Z };                   // Right: normal +X
```

**Vessel generator trap:** on `XZ`, sketch +y points to world −Z. For a vertical vessel axis
along +Z with the profile in sketch coordinates, either sketch on `YZ` (x=world Y, y=world Z;
revolve about `"y"` sketch axis or world `"Z"`) or use an explicit plane
`{"origin":[0,0,0],"x_dir":[1,0,0],"y_dir":[0,0,1]}` (normal −Y). `tests/oracle/04-revolve-vase`
draws on XZ with negative sketch y to get +Z.

### 3.2 Sketch commands (`crates/engine/src/cmd/sketch.rs:28-201`)

| Command | Params (from `.params(...)`) | Returns |
|---|---|---|
| `sketch.create` (:29) | `plane: XY\|XZ\|YZ \| plane name/id \| {origin, x_dir, y_dir} \| {face:[x,y,z], body?, index?, path?, fixed?}; offset?: expr; flip?: bool\|"x"\|"y"; project_edges?; name?` | `{sketch: id, name, projected, attachment}` (:716) |
| `sketch.edit` (:33) | `sketch: id or name` | — |
| `sketch.finish` (:34) | none (`enabled: in_sketch`, `noundo`) | — |
| `sketch.line` (:35-40) | `points: [[x,y] \| "l1.end" \| {at, ref}…], closed?, construction?, ids?: [..], infer?: bool, connect?` | `curves`, `points`, + `created: {points, curves, constraints, dimensions}` |
| `sketch.rectangle.two_point` (:44) | `p0, p1; construction?` | `sides {bottom,right,top,left}`, `corners {...}`, `created` |
| `sketch.rectangle.center` (:60) | `center, corner` | same |
| `sketch.circle.center` (:65) | `center: [x,y] or point ref, radius \| diameter` | `curves`, `created` |
| `sketch.arc.center_point` (:82) | `center, start, end (ccw) \| center, start, sweep: signed deg` | `curves`, `created` |
| `sketch.arc.three_point` (:81) | `start, end, through` | |
| `sketch.polygon.*`, `sketch.slot.*`, `sketch.point` (:87-112) | see file | |
| `sketch.dimension` (:113-115) | `entities: [refs], type?: auto\|distance\|horizontal\|vertical\|length\|radius\|diameter\|angle, value?: number or expr, text_at?, if_redundant?: "driven", scale_sketch?` | `measured`, `unit`, `scaled`; creates a model parameter (`d1`, `d2`…, or `param` name) |
| `sketch.constraint.horizontal_vertical` (:116) | `line: id \| points: [a,b]; mode?` | |
| `sketch.constraint.coincident` (:121) | `a, b` (points, point+curve/axis `x`/`y`, two lines) | |
| `sketch.constraint.tangent/equal/parallel/perpendicular/concentric/collinear` (:126-165) | `a, b` | |
| `sketch.constraint.fix` (:146) | `entity \| entities; fixed?` | |
| `sketch.constraint.midpoint` (:151) | `point, line` | |
| `sketch.constraint.symmetry` (:166) | `a, b; line \| point` | |
| `sketch.construction` (:171) | `curves: [ids], value?` | |
| `sketch.delete` (:177) | `entities: [ids]` | |
| `sketch.draw.dimensioned` (:41) | `command, params, dimensions: {role: value}` — one undo step | creator result + `dimensions` |

Point refs: `"<id>.start"`, `"<id>.end"`, `"<id>.center"`, `"origin"`, `"p1"` (point id). Curve
ids are chosen with `ids: [...]` (`sketch.line`) or auto (`l1`, `l2`, `a1`, `c1`); they are stable
and never reused (`created_doc!`, mod.rs:6-10). `sketch` param (id or name) on any sketch command
targets a non-active sketch (`target_sketch`, sketch.rs:207-221).

### 3.3 Solid features (`crates/engine/src/cmd/features.rs:15-115`, `features_more.rs:17-`)

| Command | Params |
|---|---|
| `solid.extrude` (:16-20) | `distance: expr (or through_all: true); taper?; sketch?: id\|name (default active or last); profiles?: all \| [index] \| [{curves:[ids]}] \| [{point:[x,y]}]; face?: [x,y,z]; face_body?; direction?: positive\|negative\|symmetric; convention?: total_distance\|legacy_per_side; distance2?; start_offset?; to?: [x,y,z]; to_offset?; from?: [x,y,z]; operation?: new\|join\|cut\|intersect\|auto; targets?: [body]; name?; body_name?; thin?: true, curves?, thickness, side?: center\|inside\|outside\|two_sides, thickness2?` |
| `solid.revolve` (:21-24) | `axis: sketch line id \| x \| y (sketch axes) \| X\|Y\|Z (world); angle?: expr (default 360 deg); angle2?; direction?: symmetric; convention?: sketch_normal\|legacy_axis_start; to?: [x,y,z]; sketch?, profiles? \| face: [x,y,z], face_body?; operation?, targets?, name?, body_name?` |
| `solid.sweep` (:25-28) | `sketch, profiles?, path_sketch, path: [curve ids], orientation?: perpendicular\|parallel\|[x,y,z], twist?, scale?, operation?, targets?, name?, body_name?` |
| `solid.loft` (:29) | `sections: [{sketch, profiles?} \| {sketch, point: id}], operation?, targets?, name?, body_name?` |
| `solid.box/cylinder/sphere/torus` (:30-33) | `length,width,height` / `radius\|diameter, height, base?, axis?` / … ; `operation?, targets?, name?, body_name?` |
| `solid.fillet` (:34-38), `solid.chamfer` (:39) | `edges: [[x,y,z] \| {body, index}]; radius \| distance; body?; …` |
| `solid.combine` (:40) | `target: body; tools: [body]; operation?: join\|cut\|intersect; keep_tools?; new_component?, component_name?` |
| `solid.pattern.rectangular` (:41-44) | `features: [names] \| bodies \| components; dir1: [x,y,z]; count1; spacing1; dir2?, count2?, spacing2?; alignment?, alignment2?: left\|center\|right; reversed?, reversed2?` |
| `solid.pattern.circular` (:45-48) | `features \| bodies \| components; axis: X\|Y\|Z \| {origin, dir}; count; angle? (default 360 deg); alignment?; reversed?` |
| `solid.pattern.path` (:49-52) | `features \| bodies; path_sketch; path; count; spacing \| distance; orientation?; flip?` |
| `solid.mirror` (:53) | `features: [names] \| bodies: [names] (combine?); plane: XY\|XZ\|YZ\|plane name\|{origin, normal}\|{origin, x_dir, y_dir}` |
| `solid.hole` (:54-58) | `position: [x,y,z] \| sketch + points; diameter \| thread \| clearance; depth? \| to?; type?: simple\|drilled\|counterbore\|countersink; …` |
| `solid.pipe` (:59-62) | `path_sketch, path, diameter, wall?` |
| `solid.shell` (:83-86) | `faces: [[x,y,z] or face names from model.faces]; thickness: expr; direction?: inside\|outside\|both; tangent_chain?; body?` |
| `solid.coil` (`features_more.rs:21-24`) | `diameter; two of revolutions (turns), height, pitch; section_size; section?: circular\|square; section_position?: inside\|center\|outside; base?; axis?: X\|Y\|Z\|[x,y,z]; start_angle?; clockwise?; operation?, targets?, name?, body_name?` |
| `solid.thread` (:79-82) | cosmetic: `face: [x,y,z], designation?: "M8", length?` |
| `construct.plane.offset` (:91-94) | `base: XY\|XZ\|YZ\|plane name \| {face, body?, index?, path?}; offset: expr; name?` |
| `construct.plane.at_angle` (:95-98), others in `construct.rs` (`construct.axis.*`, `construct.point.*`) | |

Every feature command returns (`features.rs:529-530`):
`{"feature": id, "name": fname, "bodies": [...], "created_bodies", "changed_bodies", "removed_bodies", "warning", "ms"}`.
`name?` names the feature; `body_name?`/`body_names?` name the bodies (`features.rs:380-530`
`add_feature`). Rename later with `timeline.rename {feature: id|name, name}` (`edit.rs:29`) and
`body.rename {body, name}` (`browser.rs:16`).

### 3.4 Selecting faces and edges; stable names

- Faces are picked by a world point `[x,y,z]` or by **name** from `model.faces {body}`
  (`inspect.rs:20-23`: per face `index, name, exact, area, centroid, plane_normal, point,
  cylinder?`). Names come from `crates/doc/src/naming.rs:1-16`: a face new to a feature is
  `F<feature>:side:<sketch curve>`, `F<feature>:start|top|bottom|end`, `F<feature>:blend:<edge>`,
  `F<feature>:inner:<face>`; split pieces `F2:top#[F2:side:l4,F4:side:l4]`; edges
  `<faceA>|<faceB>` (`~k` for several). Faces from exact kernel history are `exact: true`; names
  worked out from geometry are marked unverified. Features store the names (`Feature.face_names`,
  `edge_names`, `document.rs:2092-2098`) so they survive upstream edits.
- Edges (fillet/chamfer): `[x,y,z]` on the edge or `{body, index}`; picks far from any edge are
  refused (`tests.rs:41`).
- `model.edges {body}` (`inspect.rs:19`), `select.set {items: [{type: body|edge|face|feature|
  sketch_curve|sketch_point|profile, …}], add?}` / `select.clear` (`edit.rs:67-70`); `Sel` enum
  `lib.rs:86-148` (`Face {body, index, point}`, `Edge {body, index, point}`, `Body {name}`,
  `Feature {id}`, `Plane {name}`, `Axis {name}`…).
- Profiles for extrude/revolve: `[{curves: [ids]}]` is the stable form (re-found each recompute).

### 3.5 Grouping, parameters, inspection, files

- Feature folders: `timeline.group {name?, features?: [id|name], parent?}` (`edit.rs:30-33`),
  `timeline.ungroup`, `timeline.rename_group`, `timeline.move_to_group` (:34-40); stored as
  `Document.timeline_groups: Vec<TimelineGroup {id, name, parent, features}>`
  (`document.rs:2312-2320`). Browser folders: `browser.group {folder: bodies|sketches|construction,
  name?, items?}` (`browser.rs:20-22`).
- User parameters (`parameters.rs:12-47`): `parameters.add {name, expression, unit?: mm|cm|m|in|
  ft|deg|rad|"", comment?, favorite?}`, `parameters.change {name, expression, unit?, comment?,
  favorite?, new_name?} | {name, delete: true}` (returns `recomputed`, `restored`),
  `parameters.list {favorites?, filter?: all|user|sketch|feature|favorites}` → `parameters [{name,
  expression, unit, value, source, feature/input, comment, favorite, uses, used_by, error}]`,
  `parameters.rename/delete/favorite/move/comment/users/graph/export/import`,
  `expr.evaluate {expression, kind?: length|angle|unitless}` → `{ok, value (mm/rad),
  display_value, references, error}`, `parameters.complete {prefix}`. Storage:
  `Document.params: Vec<Param {name, expr, unit, comment, model: bool}>` (`document.rs:21-31`).
- Measure/inspect: `inspect.measure {bodies?: [names] | items: [1-2 selections]}` →
  `bodies[{name, volume_mm3, area_mm2, center_of_mass, bbox, faces, edges, vertices, face_types,
  kernel}]`, `total`, `body_count` (`inspect.rs:11-16, 48-60`); `document.inspect {measure?}`;
  `sketch.inspect {sketch?}`; `timeline.inspect {feature}`; `model.faces`, `model.edges`,
  `model.threads`; `inspect.section {plane, offset?|at?, flip?} | {clear: true}` (`section.rs:12`).
  MCP `body_topology` = `model.edges` + `model.faces` (`tools.rs:406-411`).
- Files (`file.rs:17-67`): `file.new {name?}`; `doc.open {path, up?, names?}` (.solidcraft, .json,
  STEP/IGES/FCStd/3dm/3MF/STL/OBJ/glTF); `file.save {path?, format?: container|json,
  compression?, cache?, preview?}`; `file.save_as {path, …}`; `file.export {path, format?:
  stl|stla|obj|step|3mf|3mf-tagged|urdf|json, bodies?}` (STEP via
  `solidcraft_io::export_with_metadata` / `step_assembly`, `file.rs:430-447`; kernel writer
  `crates/kernel/src/step.rs:98 step_export`, `step_out.rs:1566,1795`); `file.insert_step`,
  `file.insert_mesh`, `file.save_mesh`.
- DXF: there is **no DXF in `file.export`**. `file.export_view_dxf {camera, hidden?: omit|dashed,
  tolerance?, path?}` (`view_dxf.rs:15-19`, 2D LINE/LWPOLYLINE of the current view with hidden-line
  removal in `view_dxf_projection.rs`, whose `drawing()`/`connect()` are `pub(super)` — not usable
  downstream without a visibility seam) and `sheet.export_dxf` (flat patterns, `sheet.rs:70-76`).
  The DXF **writer** is public: `solidcraft_io::{DxfLayer (dxf_out.rs:41), DxfEntity (:109),
  DxfShape::{Line, Polyline, Circle, Arc, Ellipse, Spline, Text} (:60-105), write_dxf_drawing(&[DxfLayer],
  &[DxfEntity]) -> String (:330)}` — `triya-drawing` can emit a full drawing (views, dimensions as
  `Text`, title block) with it and write through `solidcraft_io::vfs::write`. Section geometry:
  `ModelBody::mesh()` (`crates/doc/src/eval.rs:81`) gives the reference `Mesh` per body; the
  kernel B-rep is `ModelBody.body` (truck access only inside `crates/kernel`).
- Metadata: see §4.2 (`document.metadata.set`).

### 3.6 Scripts: a sketch → revolve chain to copy

`tests/action_matrix/designs/revolved-part.json` (stepped knob; shape to copy):

```json
{"commands": [
 {"command": "parameters.change", "params": {"name": "flange_r", "expression": "20 mm"}},
 {"command": "sketch.create", "params": {"plane": "XZ", "name": "Profile"}},
 {"command": "sketch.line", "params": {"points": [[0,0],[20,0],[20,6],[10,6],[10,24],[14,24],[14,34],[0,34]],
   "closed": true, "ids": ["l1","l2","l3","l4","l5","l6","l7","l8"]}},
 {"command": "sketch.dimension", "params": {"entities": ["origin", "l1.end"], "type": "horizontal", "value": "flange_r"}},
 {"command": "sketch.finish", "params": {}},
 {"command": "solid.revolve", "params": {"axis": "Z"}}
]}
```

Others: `crates/engine/src/sample.rs:6-34` (`sample::script()`: parameters → sketch → extrude →
fillet → holes → boss → bore; used by `sample_design_builds`, `tests.rs:787`),
`tests/oracle/04-revolve-vase/recipe.json` (lines + 3-point arc on XZ, revolve about Z),
`tests/oracle/29-two-sided-and-partial-revolve`, `examples/bracket.json`,
`examples/acceptance/sensor_enclosure.json`. Script format accepted by `Session::run_script`
(`lib.rs:922-945`): a JSON array or `{"commands": [...]}` of `{command|id, params}`; ≤100,000
steps; each step is its own `execute` (own undo step) and the first failure aborts with
`step N (id): …`. `solidcraft-cli run script.json --out part.step` / `eval` / `snapshot` take it.

### 3.7 Running commands from a command (one undo step)

Pattern used by `sketch.draw.dimensioned` (`crates/engine/src/cmd/sketch_dimensioned.rs:76-120`):

```rust
let mut scratch = s.scratch();                       // lib.rs:792 — same doc/model, empty history
let mut out = scratch.execute(create, &params)?;     // nested execute: no frame mapping, no journal
…
let result = scratch.execute("sketch.dimension", &dimension)?;
// Session::execute records the wrapper's before snapshot. Nested history must not leak.
scratch.undo.clear();                                // :117
s.accept_computed(scratch);                          // :118 → lib.rs:838 (doc, model, caches, selection)
Ok(out)
```

The outer `execute` then pushes one `Snapshot` (the state before the wrapper) → one undo step,
and restores everything if the wrapper returns `Err`. `Session::scratch`, `accept_computed`,
`execute`, `run_script`, `checkpoint`/`restore_checkpoint` (`lib.rs:745-788`) are all `pub`.
`CommandSpec::run_nested` (`mod.rs:178-181`) is `pub(crate)`, but `spec.run` and `spec.enabled`
are `pub` fields, so `(spec.run)(s, &p)` after `(spec.enabled)(s)?` is the equivalent (skips
`with_created` and `catch_unwind`; the outer `execute` still catches panics). Over a channel, use
`engine.batch {commands, rollback?}` (`control.rs:129-160`, checkpoint-based) or MCP `batch`
(`tools.rs:490-546`); these are N undo steps, rolled back together on failure.

---

## 4. Document model (`crates/doc/src/document.rs`)

### 4.1 `FeatureKind` (document.rs:495-1492; `#[serde(tag = "type", rename_all = "snake_case")]`)

Variants: `BodyTransfer Sketch Extrude Revolve Fillet Chamfer Box Cylinder Sphere Torus Combine
Pattern Mirror Hole Thread Loft Sweep Shell Draft ConstructionPlane Split ConstructionAxis
ConstructionPoint SplitFace Scale OffsetFace DeleteFace BoundingSolid Pipe Emboss Rib ReplaceFace
Align Coil SheetBase SheetContour SheetFlange SheetHem SheetPerforate SheetFold SheetUnfold
SheetConvert Boss Lip SnapFit Rest BoundaryFill Remove Patch Stitch Thicken SurfaceExtrude
Unstitch ReverseNormal SurfaceUntrim SurfaceRevolve SurfaceSweep SurfaceLoft SurfaceOffset
SurfaceRuled SurfaceExtend SurfaceBlend SurfaceBoundary SurfaceMatch SurfaceFair SurfaceTrim Move
Import MeshConvert MeshImport`. There is **no** generic/plugin/opaque variant: `docs/plugins.md`
designs a `FeatureKind::Plugin { plugin_id, params: Value, display, … }` but nothing of it is
implemented in v0.14.0. Adding a variant = a format change (§4.4) + eval support in `eval.rs`.

`Feature` (document.rs:2074-2112): `id: u64, name, suppressed, body_names, param_names, component,
edge_refs, edge_names, face_names, participants, shell_piece_order, #[serde(flatten)] kind`. No
free-form field on features; unknown JSON fields on a feature are not kept.

### 4.2 Free-form extension data: `Document.metadata` (the one sanctioned slot)

`Document` (document.rs:2160-2251) has `pub metadata: crate::Metadata` (:2176, "Design properties
(project, author…) and other tools' data, kept as they are (#1015)"). `crates/doc/src/metadata.rs`:

```rust
#[serde(transparent)] pub struct Metadata(pub Map<String, Value>);        // :48-49
pub const WELL_KNOWN: [&str; 7] = [project, part_number, revision, author, organization, description, license]; // :32
pub fn is_namespaced(key: &str) -> bool  // reverse-DNS: ≥2 labels of [A-Za-z0-9-_], ≤128 bytes  // :58
pub const MAX_VALUE_BYTES: usize = 64 * 1024;  MAX_VALUE_DEPTH = 32;  MAX_KEYS = 256;  MAX_TOTAL_BYTES = 256 * 1024; // :37-44
```

Commands (`cmd/metadata.rs:14-27`): `document.metadata` → `{metadata, properties}`;
`document.metadata.set {key, value} | {values: {…}}` ("another tool's data goes under its
reverse-DNS key such as com.example.tool (an object; null removes)"), one undo step;
`document.metadata.remove {key | keys}`. Unknown keys round-trip through open/edit/save
unvalidated. **Triya should persist the vessel definition as
`metadata["com.triya.vessel"] = {schema: 1, …}`** (≤64 KiB compact JSON) — no format change, no
upstream edit, opens unchanged in upstream SolidCraft. Larger payloads: a sidecar file, or a
format change (§4.4). Also free-form but scoped: `Document.params` comments
(`param_comments`), `favorites`, `named_views`, `browser_groups`, `timeline_groups` (all typed).

### 4.3 Parameters storage

`Document.params: Vec<Param>` (:2177; `Param {name, expr: String, unit: String ("mm"|"deg"|""),
comment, model: bool}` :21-31; `model: true` for dimension/feature-input parameters `d1`…).
Features reference parameters by name inside `Expr` strings (`expr.rs:23 pub struct Expr(String)`);
`Feature.param_names` (:2085) names a feature's inputs so `parameters.change` can drive them.
Expression evaluation: `Document::eval(&Expr, Kind)`, `Kind::{Length, Angle, Unitless}`
(`expr.rs`). Design display units: `Document.units` (:2173, `document.units {units}`).

### 4.4 The format `pending/` mechanism and its cost for a fork

- Files say `"format": "solidcraft/N"` (`format.rs:331`); current `FORMAT = FORMATS.current()`
  (:241) is computed at build time: `crates/doc/build.rs:9 const HAND_WRITTEN: u32 = 22;` + one per
  folded folder `src/format/v23/`, `v24/`, `v25/` (contiguous, asserted :24-27) + 1 while
  `src/format/pending/*.rs` is non-empty (`format.rs:1132` test). v0.14.0: `FORMAT = 25`,
  `pending/` holds only `README.md`.
- A change that saves something new adds ONE file `crates/doc/src/format/pending/<name>.rs`
  (lowercase-dashes name, must start with a `//!` note; optional `pub fn step(v: &mut Value) ->
  crate::Result<()>` to rewrite older JSON; see `pending/README.md`). The build lists it in
  `Document.format_pending` (:2170) so later builds apply only missing steps. At release
  `scripts/fold-format-pending.sh` moves pending files into `v<N>/`.
- **Cost for a fork:** a Triya pending file makes Triya write `solidcraft/26` with
  `format_pending: ["triya-…"]`. Upstream 0.14 refuses such files ("saved by a newer SolidCraft",
  `format.rs:563-565`), and when upstream itself ships format 26 with different pending names, the
  two "26"s collide: upstream 0.15 would read a Triya-26 file as its own 26 (missing Triya fields
  fall back to serde defaults; a Triya `step` never runs there). Fork files are only safe while the
  fork's number stays ahead or the tag differs (`format_string` seam → own tag → loses the ability
  to open upstream designs unless `version_of` reads both). Conclusion: avoid format changes;
  use `metadata` (§4.2). If unavoidable, also change `format_string`/`version_of`
  (`format.rs:331-345`) to a Triya tag that still accepts `solidcraft/N`, mirroring the
  `solvecraft/N` shim.
- `Document` fields are all `#[serde(default)]`-tolerant, but the top-level struct is not
  `deny_unknown_fields`-free everywhere (`document.rs:99,469` deny on sub-types); unknown
  top-level keys are dropped on save (the `metadata.rs:14-15` note explains why only `metadata`
  keeps unknown keys).

---

## 5. Layering and xtask (`xtask/src/*`; alias `.cargo/config.toml` `xtask = "run --package xtask --"`)

Subcommands (`xtask/src/main.rs:20-66, 73-101`): `layers`, `assets`, `app-icons`, `features`,
`i18n [--check|--add-missing]`, `oracle [--cli BIN]`, `action-matrix [--bless] [--results F]
[--from DIR]`, `book [--no-build]`, `licences [--check]`, `lists [--fix]`, `test [CARGO ARGS]
[-- TEST ARGS]`, `ci`, `wasm`, `web-offline [DIR]`, `web-offline-test [--site DIR]`.

`ci` (`main.rs:360-375`): `cargo fmt --all -- --check` → `assets::run` → `licences::run(check)` →
`lists::run` → `cmd_layers` → `cargo clippy --workspace --all-targets --profile ci -- -D warnings`
→ `test::run` (ci profile, all test binaries) in parallel with the wasm check and doc tests →
`web_offline_test::run` (needs headless Chrome; skipped without it). Nothing in `ci` runs
`xtask i18n --check`, `oracle`, `features` or `action-matrix`.

### 5.1 `layers` knows crates from a hard-coded table (`xtask/src/layers.rs:34-56`)

```rust
pub const TABLE: &[(&str, Class)] = &[
    ("geom", Class::Layer(0)), ("derive", Class::Standalone),
    ("sketch", Class::Layer(1)), ("kernel", Class::Layer(1)), ("render", Class::Layer(1)),
    ("doc", Class::Layer(2)), ("io", Class::Layer(3)), ("engine", Class::Layer(4)),
    ("ui-egui", Class::Layer(5)), ("gpu", Class::Layer(5)), ("mcp", Class::Layer(5)),
    ("testkit", Class::Testkit),
    ("solidcraft", Class::Exempt), ("cli", Class::Exempt), ("web", Class::Exempt), ("xtask", Class::Exempt),
];
pub const INTRA_LAYER_ORDER: &[&[&str]] = &[&["gpu", "ui-egui"], &["gpu", "mcp"]];   // :61
pub const UI_CRATES: &[&str] = &["egui", "eframe", "egui-wgpu", "wgpu", "winit", "egui_kittest", "rfd", "bevy*"]; // :74
pub const UI_MIN_LAYER: u8 = 5;                                                       // :77
pub fn short_name(pkg) -> &str { pkg.strip_prefix("solidcraft-").unwrap_or(pkg) }     // :79
```

Rules (`check`, :147-193): every workspace member must be in `TABLE`
(`Violation::Unregistered`); a crate may depend only on **strictly lower** layers unless an
`INTRA_LAYER_ORDER` chain lists the dependency before it; UI crates only at L5+; `Standalone`
has no workspace deps; `Testkit` only as dev-dep; `Exempt` unchecked. Dependencies are read
from `cargo metadata --no-deps` (`from_metadata`, :199-220), so this applies only to crates that
are workspace members (`Cargo.toml:3`).

**What Triya crates must do** (each a `TABLE` row = seam in `layers.rs`, or none if the fork
keeps its crates in a separate workspace, which `layers` never sees):
- `triya-standards` (pure data/maths, no workspace deps): `("triya-standards", Class::Standalone)`
  or `Layer(0)`; `short_name` leaves non-`solidcraft-` names intact, so use the full package name.
- `triya-vessel`, `triya-drawing` (depend on `solidcraft-engine`, L4): cannot be `Layer(4)`
  (`to >= layer` → `Upward`); register as `Layer(5)` with no UI crates, or `Layer(4)` plus
  `INTRA_LAYER_ORDER` entries `&["engine", "triya-vessel"]`. `Layer(5)` is simpler.
- `triya-ui` (depends on `solidcraft-ui-egui`, L5, and egui): `Class::Layer(6)` (any `u8` is
  legal; 6 is also Testkit's rank but Testkit is a separate class) — or `Layer(5)` with
  `&["ui-egui", "triya-ui"]` in `INTRA_LAYER_ORDER`.
- Apps `triya`, `triya-cli`, `triya-web`: `Class::Exempt`.
- `wasm_check` (`main.rs:345-357`) checks every `Class::Layer(_)` crate plus the literal
  `"solidcraft-web"` for `wasm32-unknown-unknown`: Triya `Layer` crates are wasm-checked
  automatically (so no `std::fs`/threads without `cfg`), `Exempt` ones are not; `triya-web` needs
  its own entry (`main.rs:350` `set.push("solidcraft-web".into())` seam) if it should be checked.

### 5.2 `assets` (`xtask/src/assets.rs`)

Walks the repo skipping `.*`, `target*`, `plan`, `dist`, `node_modules` (:38-40). Needs a row in
`ATTRIBUTION.md` (a markdown table row whose first cell is the exact repo-relative path, :63-67)
for: every file under `assets/` except `*.md` (:60-62, including code masters such as
`assets/icons/ui/source/icons.rs`), and any file anywhere with an extension in `ASSET_EXT`
(:5-33: png jpg jpeg svg ico icns car vectorcraft ai psd afdesign fig sketch ttf otf woff woff2
gif bmp webp wav mp3 mp4 stl step stp obj fcstd 3dm). Symlinks under `assets/` are errors.
Icons drawn in Rust outside `assets/` (e.g. `triya/crates/triya-ui/src/icons.rs`) need no row;
a Triya STEP/STL test fixture or a `docs/*.png` screenshot does. Icons added to the upstream
painter live in `assets/icons/ui/source/icons.rs` (already attributed as a file; editing adds no
row). `AGENTS.md` ("Assets — absolutely essential"): no artwork from commercial CAD, ever.

### 5.3 Other gates

- `lists` (`lists.rs`): `assets/locales/*.json` must be `serde_json` pretty-printed, keys sorted,
  no duplicate ids; `--fix` rewrites. Only the locale catalogs are checked.
- `wasm`: see 5.1. `web-offline [DIR]` (`web_offline.rs`): Trunk post-build hook that writes
  `sw.js` + `build.json` (§6). `web-offline-test [--site DIR]` (`web_offline_test.rs`, 820 lines):
  headless Chrome check of offline use and update; skipped when Chrome is absent.
- `oracle [--cli BIN]` (`main.rs:220-333`): discovers every `tests/oracle/*/recipe.json`
  (a new case folder needs no registration), replays via `solidcraft-cli oracle`, compares with
  `expected.json`, writes `docs/oracle.md`; fails on a failing case unless its `expected.json`
  has `known_failure`, and fails when a known failure passes. `NO_ORACLE` table at `main.rs:205`.
- `features` (`features.rs`): runs `cargo run --release -p solidcraft-cli -- commands` (:34) and
  writes `docs/features.md` by id prefix (`AREAS` :13-28; unknown prefixes such as `vessel`
  are appended alphabetically). Regenerated on `main` by `generated.yml`; never in a PR.
- `licences [--check]` (`licences.rs`): writes `THIRD-PARTY-LICENSES.txt` from the normal-dep
  closure of `ROOTS = ["solidcraft", "solidcraft-cli", "solidcraft-web"]` (:18 — a Triya app
  must be added here to be covered). No licence allowlist is enforced by code; policy is in
  CONTRIBUTING §"Rules" 2 (permissive only; no GPL/LGPL/AGPL anywhere, "not even reading it").
- `cargo deny` (`deny.toml`, run by `ci.yml:48-50` nightly/manual only): advisories, bans
  (`multiple-versions`/`wildcards` warn), sources (crates.io only, `unknown-git = "deny"`); **no
  `[licenses]` section** — a git dependency in a Triya crate would fail `cargo deny check sources`.

---

## 6. Web app (`apps/solidcraft-web/`)

- `Trunk.toml` (12 lines): `target = "index.html"`, `dist = "../../dist/web"`, `public_url =
  "./"`, `filehash = true`; serve on `127.0.0.1:8781`; `[[hooks]] stage = "post_build" command =
  "cargo" command_arguments = ["xtask", "web-offline"]`.
- `Cargo.toml`: bin `solidcraft-web` (`src/main.rs`); deps `solidcraft-engine`,
  `solidcraft-ui-egui`, `egui`, `log`; wasm32-only `eframe` (features `default_fonts, wgpu,
  accesskit`), `wasm-bindgen`, `wasm-bindgen-futures`, `js-sys`, `serde_json`, `web-sys` (feature
  list :22-27).
- `index.html` brand points: `:6` `<title>SolidCraft</title>`; `:7` description; `:8-21` og:*
  and twitter:* (site_name, title, description, url `https://bherbruck.github.io/solidcraft/app/`,
  image `…/og.png`, alt); `:22` `<link data-trunk rel="rust" data-bin="solidcraft-web"
  data-wasm-opt="z" data-weak-refs />`; `:23-24` copy-file BIZ UDP Gothic font + OFL into `fonts/`;
  `:25` css `assets/styles/web/loading.css`; `:27` theme-color; `:28-30` manifest + icon links
  (`icons/32x32/ai.storyteller.solidcraft.png`, apple-touch 256); `:31-34` copy-file lines for
  `manifest.webmanifest` and the three Linux icon exports (32/256/512); `:37-38` `<canvas
  id="solidcraft_canvas">`, `<div id="solidcraft_loading">Loading SolidCraft…</div>`; `:39-48`
  inline script: `navigator.serviceWorker.register("sw.js")`, `beforeinstallprompt` →
  `window.solidcraftInstall` (read by `src/install.rs:11`). Ids must match `src/main.rs:28-29`.
- `manifest.webmanifest`: name/short_name "SolidCraft", description, `start_url: "./"`,
  `scope: "./"`, standalone, colours, three icons.
- `sw.js` (JavaScript, allowed exception per `AGENTS.md`): template line `:29`
  `/* @build */ const VERSION = "dev", FILES = [];` replaced by `xtask web-offline`
  (`web_offline.rs:19 MARKER`, `generate` :51-66: lists every file in the dist dir except
  `sw.js`, `build.json` and `ON_DEMAND = ["fonts/"]`, SHA-256 each, version =
  `<workspace version>-<16 hex of manifest digest>`; the same list is written to `build.json`).
  Cache names `:33` CACHE_PREFIX = "solidcraft-app ${PATH} " (a template literal), `:34`
  CACHE = CACHE_PREFIX + VERSION, `:36` PAGES = "solidcraft-app-pages ${PATH}". `VERSION === "dev"` makes the
  worker retire (debug builds; `web_offline.rs:27`).
- `src/main.rs:455-500`: `eframe::WebRunner::new().start(canvas, options, |cc| { files::start();
  let services = Services {…}; SolveApp::new(Session::default(), services) })`; CJK font fetched
  on demand from `./fonts/BIZUDPGothic-Regular.ttf` (`:54`). `files.rs` = OPFS/localStorage
  design store; `space_mouse.rs` = gamepad API.
- Deployment (`.github/workflows/pages.yml`): on push to `main`, `cargo xtask book` → `trunk
  build --release --public-url /solidcraft/app/` (:59) → `cargo xtask web-offline-test --site
  dist/web` → `site/` = book + `site/app` = `dist/web` (:63-66) → `packaging/flatpak-site.sh` →
  `actions/deploy-pages`. Skipped while the repo is private (:30).

**`triya/apps/triya-web` — duplicate vs reference:** duplicate (and rebrand) `Trunk.toml`
(change `dist`, port), `index.html` (every line above; keep the element ids or change
`src/main.rs` constants together), `manifest.webmanifest`, `Cargo.toml` (bin name = `data-bin`),
`src/main.rs` (register Triya commands before `SolveApp::new`, draw Triya panels), and `sw.js`
(only `CACHE_PREFIX`/`PAGES` prefixes need a new name; the `/* @build */` marker must stay).
Reference (reuse unchanged): `assets/styles/web/loading.css`, the font files, `xtask
web-offline`'s template path is hard-coded to `apps/solidcraft-web/sw.js`
(`web_offline.rs:76`) — a Triya web app needs either that path parameterised (seam) or its own
post-build step that calls `web_offline::generate` logic on its own template. `src/files.rs`,
`install.rs`, `space_mouse.rs` are binary-private modules; copy or move them into a shared crate.

---

## 7. Tests and quality conventions

- `hostile_params_never_panic`: `crates/engine/src/tests.rs:674-787` (gated in
  `.config/nextest.toml` as heavy: skipped by PR shards, run on `main`, nightly and `cargo xtask
  test`). It iterates `command_specs()` (:786) minus file-writing ids, and for every id × every
  `hostile_values()` (:647, `pub(crate)`) × every key in the `keys` list (:675-734) runs the
  command on a session with a box, a sketch and a rectangle, collecting `EngineError::Internal`.
  With the §1.3 runtime registry, Triya commands are **not** in the engine's own test binary.
  Add in `triya-vessel`'s tests: a copy of this loop over `triya_vessel::COMMANDS` with Triya's
  own key list (hostile values are easy to re-list: null, bools, NaN-ish numbers, huge numbers,
  empty/huge strings, nested arrays/objects); or (seam) append Triya param names to `keys` and
  register the commands in a `#[cfg(test)]` hook.
- Engine test helpers (`tests.rs:8-23`, private, copy them):

```rust
fn run(s: &mut Session, id: &str, p: Value) -> Value { match s.execute(id, &p) { Ok(v) => v, Err(e) => panic!("{id} {p}: {e}") } }
fn volume(s: &mut Session) -> f64 { run(s, "inspect.measure", json!({}))["total"]["volume_mm3"].as_f64().unwrap() }
fn rel(a: f64, b: f64) -> f64 { (a - b).abs() / b.abs().max(1e-12) }
```

  Per-body: `run(s, "inspect.measure", json!({"bodies":[b]}))["bodies"][0]["volume_mm3"]`
  (`face_name_tests.rs:17-19`). Integration-test template: `crates/engine/tests/atomic_commands.rs`
  (error leaves the design unchanged; undo/redo round trip; determinism). Robustness fuzzer:
  `solidcraft_engine::fuzz::{run, golden, replay, minimise}` (`fuzz.rs:286-410`, `pub`).
- Oracle format (`tests/oracle/README.md`; reader `apps/solidcraft-cli/src/oracle.rs`,
  converter `recipe.rs`): `recipe.json` = `{id, name, description, exercises, units, conventions,
  parameters: [{name, expression, unit}], features: [{op: sketch|extrude|revolve|combine|
  rectangular_pattern|command, …}]}`; a sketch feature holds `plane {ref, origin, x_dir, y_dir,
  normal}`, `entities [{type: line|arc_three_point|…, start {at|ref}, end, id}]`, `constraints`,
  `dimensions`; a `revolve` holds `profile {sketch, loops}`, `axis {origin, dir}`, `angle`,
  `operation`, `result_bodies`. Any step can be a raw `{"op": "command", "command", "params"}`.
  `expected.json` = `{source: "analytic"|"freecad <ver>", method, body_count, volume_mm3,
  area_mm2, center_of_mass, bbox {min,max}, faces?, compare?, tolerance? {rel (1e-3), abs_mm,
  surface_mm}, note?, known_failure?}`; optional `reference.stl`. Expected values never from a
  commercial CAD product. A Triya case = new folder `tests/oracle/NN-vessel-…/` (auto-discovered).
- Crate-root lines to copy (every production crate, e.g. `crates/engine/src/lib.rs:7-8`):

```rust
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]
```

  plus `[lints] workspace = true` in each `Cargo.toml` (`[workspace.lints.rust] unsafe_code =
  "forbid"`; clippy `dbg_macro/todo = warn`, `too_many_arguments/type_complexity/
  neg_cmp_op_on_partial_ord = allow`, `Cargo.toml:67-76`).
- `rustfmt.toml`: `max_width = 150`, `use_small_heuristics = "Max"`. `clippy.toml`:
  `allow-unwrap-in-tests`, `allow-expect-in-tests`, `allow-panic-in-tests`,
  `allow-indexing-slicing-in-tests = true`. Profiles: `[profile.ci]` (opt 1, no debug
  assertions because truck's debug checks reject valid geometry, `Cargo.toml:92-103`).
- `deny.toml`: see §5.3 — no licence allowlist; permitted licences are by policy "permissive"
  (MIT, Apache-2.0, BSD, Zlib, MPL-2.0 is not mentioned — ask before adding), never GPL/LGPL/AGPL.
  `cadmpeg-*` is pinned `=0.6.1` with only open-format features (`Cargo.toml:52-54`).
- UI tests over the registry (run in the ui-egui test binary, so they see only upstream commands
  unless Triya registers in a test hook): `i18n.rs:281` every label in `assets/locales/en.json`
  (lookup is by English text, `i18n.rs:82-88`; keys `cmd.<id with _>`), `i18n.rs:259` every
  tab/panel word, `assets/icons/ui/source/icons.rs:1507` every non-empty icon name draws
  something inside its square, `toolbar.rs:1822-1835` panel accents.

---

## 8. MCP (`crates/mcp`) and CLI (`apps/solidcraft-cli`)

- `serverInfo`: `crates/mcp/src/server.rs:145` `{"name": "solidcraft", "title": "SolidCraft",
  "version": env!("CARGO_PKG_VERSION")}`; resources `solidcraft://design` → `document.inspect
  {measure: true}`, `solidcraft://commands` → `engine.commands` (:163-176; `solvecraft://` shim).
  Protocol constants `PROTOCOL_VERSION`, `SUPPORTED_VERSIONS` (`lib.rs:17`). Line cap
  `solidcraft_engine::MAX_LINE = 16 MiB` (`engine/lib.rs:45`).
- Tools (`tools.rs:61-280`, 15): `list_commands {filter?}`, `execute {command, params?,
  verbose?}`, `batch {commands: [{command, params}], rollback?, verbose?}`, `inspect_design
  {measure?, sketches?}`, `measure {body? | items?}`, `body_topology {body}`, `set_parameter {name,
  value, unit?, comment?}` → `parameters.change`, `screenshot`, `export {path, format, bodies, …}`
  → `file.export`, `crash_reports {id? | limit?}`, `undo`, `redo`, `new_design {name?}`, `open
  {path, up?, names?}`, `save {path?, format?}`. Dispatch `tools.rs:372-434`: `execute` →
  `exec(b, id, params)` → `Backend::call(command, params)` (`backend.rs`: `Headless` runs
  `Session::execute` in-process; `Remote` sends `engine.execute` over the loopback control port).
  `batch` (:490-546): tries one `engine.batch` request (app frame, checkpoint rollback), else
  step-by-step with undo-depth rollback. Triya commands registered per §1.3 are callable through
  `execute`/`batch` with no MCP edit; a dedicated `vessel` tool would be a new arm in
  `tool_definitions` + `dispatch` (avoidable).
- Public API for a `triya-cli mcp`: `solidcraft_mcp::{Server, Backend, Headless, Remote,
  call_tool, tool_definitions, agent_scope, scratch_dir}` (`lib.rs:16-18, 27, 44`).
- `solidcraft-cli` (`apps/solidcraft-cli/src/main.rs:72-123`): `run <script|design|step> [--in F]
  [--out F]... [--save F] [--quiet] [--results] [--json-out F] [--allow-errors]` (fails if any
  timeline feature errors unless `--allow-errors`), `eval|inspect` (measurements JSON), `script
  <design> [--out script.json]` (= `timeline.export_script`), `snapshot <script|design> --out
  shot.png [--width] [--height] [--view NAME | --camera JSON] [--no-grid] [--no-axes]`, `render`,
  `exec <command> [json]`, `gpu`, `commands` (= `engine.commands`), `recipe <recipe.json>`,
  `oracle <case-dir>... [--json]`, `fuzz`, `fuzz-one`, `mcp [--in F] [--connect HOST:PORT
  [--token T]] [--allow-path DIR]... [--allow-any-path]`, `crashes [list|show|replay|dir|clear]`,
  `icon`, `licences`, hidden `save-stress`, `bench-edit`, `bench-open`, `bench-agent`,
  `--version`. `load()` (:145-168) accepts design containers, `format`-tagged JSON, oracle
  recipes (`features` without `commands`) and command scripts. A `triya-cli` can be a copy of
  `main.rs` that registers Triya commands first; `mod app_icon` uses
  `#[path = "../../../assets/icons/app/shared/source/app_icon.rs"]` (:33) — replace with a Triya
  icon source (and an ATTRIBUTION row if it lives under `assets/`).

---

## 9. Agent conventions (`AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `skills/`)

`AGENTS.md` = `CLAUDE.md` plus two paragraphs (camera controls; "Showing and following through
on changes": screenshots as review evidence, keep branches rebased). Constraints that bind a fork:

- Product name discipline (`CLAUDE.md` "Product name", `docs/compatibility.md`): old names only
  in listed shims; `git grep -i solvecraft` must find nothing else. A fork should list its own
  brand strings the same way.
- "Never crash": no `unwrap/expect/panic!/unreachable!/todo!/unimplemented!` in non-test code; no
  `unsafe` except three Windows/macOS files in `apps/solidcraft`; input numbers are hostile
  (`get()` not `[i]`, finite checks, caps); truck only through `solidcraft_kernel::guard`; every
  crash fix lands with a regression test (`hostile_params_never_panic`).
- "Independent": no commercial CAD data, measurements, command ids, files, screenshots or
  artwork (Fusion, SolidWorks, Onshape…); reference values analytic or from open-source tools.
- "Never read, copy or link GPL/LGPL/AGPL code — don't even read their sources": OpenCASCADE,
  FreeCAD (planegcs), SolveSpace, CadQuery/OCP, LibreCAD, QCAD. `tools/freecad-oracle/` runs
  FreeCAD as an external program only.
- Assets: every file asset original/openly licensed with an `ATTRIBUTION.md` row (`xtask
  assets`); UI icons drawn in code (`assets/icons/ui/source/`); no screenshots of commercial CAD.
- "Everything is a command": user-visible behaviour = `CommandSpec` + tests; programmatic calls
  never open dialogs; only `ui.start` starts tools/dialogs.
- Layering enforced by `xtask layers`; nothing below L5 uses egui/eframe/winit/wgpu/rfd; only
  `crates/kernel` uses truck; "The UI crate is swappable"; "The UI is thin" (`app.run`/`app.start`,
  colours from `theme::Tokens`).
- Rust only; JS only in `apps/solidcraft-web/sw.js` and the inline `index.html` scripts.
- Design file format: ONE pending file per change; never edit `FORMAT`, `JSON_STEPS`, history or a
  `v<N>/` folder (§4.4).
- "Lists many PRs add to" (`CONTRIBUTING.md:228-245`): locale catalogs sorted (`xtask lists
  --fix`); `mod` lines in their sorted group; `i18n.rs` language list English first then by id;
  generated files (`THIRD-PARTY-LICENSES.txt`, `docs/features.md`) left to `main`'s workflow.
- Camera input only through `navigation::handle` with `NavCaps` (a test rejects hand-rolled
  camera input) — relevant if `triya-drawing` adds a 2D drawing canvas (`NavCaps` orbit off).
- `plan/` is gitignored and local; `CLAUDE.md` tells agents to read `plan/STATUS.md`,
  `plan/execution-plan.md`, `plan/architecture.md`, `plan/adr/0001-geometry-kernel.md` — none of
  these exist in the public tree (§10).
- Workflow: claim = draft PR; one topic per PR; `cargo xtask ci` before every commit; never
  force-push `main`; never regenerate generated files in a PR; releases only per `docs/releasing.md`.
- Licence of contributions: MIT OR Apache-2.0 (`CONTRIBUTING.md:320-323`); "SolidCraft isn't
  affiliated with Autodesk".

`skills/` (Agent Skills format, `skills/README.md`): `solidcraft-modeling` (SKILL.md +
`reference.md`: plan the part as numbers, build in few `batch` calls, verify with `measure`;
sketch-axis table incl. XZ y = −Z), `solidcraft-editing` (edit via parameters, map the design in
3-4 calls), `solidcraft-checking` (SKILL.md, `reference/porting.md`, `scripts/` Python helpers +
pytest facts, `templates/`: parameter ranges, exact STEP comparison, regression tests, porting
from build123d/CadQuery/OpenSCAD/STEP), `solidcraft-mechanisms` (joints, motion links, closed
loops). CI ties: `crates/mcp/src/tests.rs:1123` `skills_name_only_real_tools_and_commands`
`include_str!`s exactly the seven listed files; `skill_reference_example_builds_as_documented`,
`mechanisms_skill_example_moves_as_documented`, `sketch_plane_axes_match_the_skill_reference`.
A fork can add `skills/triya-vessel/SKILL.md` (+ `reference.md`) alongside with no upstream
edit; to get the same "names only real commands" check, add an `include_str!` line (seam in
`tests.rs:1124-1131`) or a sibling test in a Triya crate.

---

## 10. Everything else a fork must know

- `.gitignore` (9 lines): `/target*/`, **`/plan/`**, `/dist/`, `*~`, `.DS_Store`, `*.swp`,
  `/.claude/worktrees/`, `/target-snapshot`, `__pycache__/`. The fork's planning directory must
  not be named `plan/` (it would be ignored, and `xtask assets` also skips a top-level `plan`
  directory, `assets.rs:40`); `/dist/` is the Trunk output.
- `deny.toml`: §5.3. `flake.nix`: Nix dev shells and a `solidcraft` package (`pname`, bins
  `solidcraft`/`solidcraft-cli`, Linux desktop/mime/icon installs at `:107-143`, Rust version
  assertion against `Cargo.toml`); a fork package needs its own derivation or none.
- Workflows that misfire in a fork and should be disabled or rewritten (`.github/workflows/`):
  - `claim.yml` — `issue_comment` + daily cron `17 5 * * *`: `/claim` bot on the fork's issues.
  - `release.yml` — nightly cron `0 0 * * *` (America/New_York) **publishes nightly pre-releases
    of changed `main` commits to the fork's GitHub Releases**, and `push: branches: [release]`
    builds draft releases named `solidcraft-*`; needs `UPDATE_SIGNING_KEY`,
    `SNAPCRAFT_STORE_CREDENTIALS`.
  - `winget.yml`, `snap-store.yml`, `flatpak-pages.yml` — on `release: published`; skip without
    `WINGET_TOKEN`/`SNAPCRAFT_STORE_CREDENTIALS` but still run and push to Pages.
  - `pages.yml` — push to `main` deploys book + web app to `/<repo>/` with a hard-coded
    `--public-url /solidcraft/app/` (:59) and `packaging/flatpak-site.sh`; wrong path for a fork
    named otherwise.
  - `generated.yml` — commits regenerated `THIRD-PARTY-LICENSES.txt` and `docs/features.md` to
    `main` with `[skip ci]`; harmless but it rewrites `ROOTS`-based licences, so keep or adapt.
  - `ci.yml` (PR/push/nightly) is sound for a fork; it runs `scripts/ci/test_affected.py`
    (package selection from `cargo metadata`), `scripts/claim/*.test.js`, `cargo deny` (nightly),
    the oracle (`cargo xtask oracle --cli target/ci/solidcraft-cli`, :397-399), GPU tests,
    Windows smoke scripts (`tools/windows-*-smoke.ps1`), `cargo xtask wasm` (:506).
  - `freecad-oracle.yml` — manual only. `lists.yml`, `packaging-lint.yml` — fine.
- Update checker (`crates/ui-egui/src/update.rs:27-33`) polls
  `api.github.com/repos/bherbruck/solidcraft/releases/latest` and offers one-click installs of
  upstream packages: a Triya build must repoint `REPO`/`LATEST_API`/`RELEASES_PAGE`/
  `RELEASE_PREFIX` or pass `Services { http_get: None, http_download: None }`.
- Agent chat (`crates/ui-egui/src/agent_chat/`) spawns `solidcraft-cli mcp` found next to the
  executable (`gate.rs`, `discover.rs`); a `triya` binary without a `solidcraft-cli` sibling loses
  the in-app agent unless the CLI name is parameterised.
- `vendor/` (excluded from the workspace, patched via `[patch.crates-io]`): `truck-geometry`,
  `truck-shapeops`, `truck-stepio`, `truck-meshalgo`, `matext4cgmath`, `egui-wgpu` — Apache-2.0 /
  MIT with local fixes (`vendor/README.md`, `ATTRIBUTION.md`). A separate Triya workspace must
  repeat the patch table or it will compile unpatched crates.io versions.
- `docs/features.md`, `THIRD-PARTY-LICENSES.txt`, `docs/oracle.md` are generated; `ROADMAP.md`
  is hand-maintained upstream status; `docs/book/` is the mdBook (`xtask book`).
- `.config/nextest.toml` `default-filter` names heavy tests by exact path (e.g.
  `test(=tests::hostile_params_never_panic)`); a Triya heavy test should be added there or kept
  `#[ignore]` behind an env var to keep PR CI fast.

---

## Seam list

Minimal set of upstream files a fork edits to ship the Triya crates. "Avoidable" = the fork can
reach the goal without touching that file (with the trade-off noted).

| # | Upstream file : anchor | One-line change | Avoidable? |
|---|---|---|---|
| 1 | `Cargo.toml:3` `members = ["crates/*", "apps/*", "xtask"]` | add `"triya/crates/*", "triya/apps/*"` (and Triya workspace deps under `[workspace.dependencies]` :15-25) | Yes — separate `triya/Cargo.toml` workspace with path deps to `../crates/*` and a copy of `[patch.crates-io]` (:135-144); then `xtask layers/wasm/ci` do not cover Triya crates |
| 2 | `crates/engine/src/cmd/mod.rs:273-274` (after `v.extend(render::COMMANDS.iter());`) | append a `OnceLock`-held extra list; add `pub fn register_extra(Vec<&'static [CommandSpec]>)` near :228 | Only by wrapping `Session::execute` in the Triya app (loses toolbar/palette/MCP/control integration) |
| 3 | `xtask/src/layers.rs:34-56` `TABLE` (+ `:61` `INTRA_LAYER_ORDER` if staying at L4/L5) | rows `("triya-standards", Standalone)`, `("triya-vessel", Layer(5))`, `("triya-drawing", Layer(5))`, `("triya-ui", Layer(6))`, `("triya", Exempt)`, `("triya-cli", Exempt)`, `("triya-web", Exempt)` | Yes if #1 uses a separate workspace (then `cargo xtask layers` never sees them) |
| 4 | `xtask/src/main.rs:350` `set.push("solidcraft-web".into())` | also push `"triya-web"` for the wasm check | Yes (check it in a Triya CI step) |
| 5 | `xtask/src/licences.rs:18` `ROOTS` | add `"triya", "triya-cli", "triya-web"` so their deps land in `THIRD-PARTY-LICENSES.txt` | Yes, but then Triya-only deps are unlisted (licence policy) |
| 6 | `crates/ui-egui/src/toolbar.rs:11` `TABS`, `:53-62` `panels()` | add `"VESSEL"` tab and its panel list | Yes — register Triya commands `.at("SOLID", "CREATE")`/`.at("UTILITIES", "ADD-INS")` (registry-driven panels, no UI edit) |
| 7 | `crates/ui-egui/src/lib.rs:846` (`SolveApp::start` fallthrough `let _ = self.run(id, json!({}))`) | insert a host hook (`pub start_hook: Option<fn(&mut SolveApp, &str) -> bool>`) so a ribbon click opens a Triya dialog | Yes — open Triya panels from host-drawn UI (`apps/triya/src/main.rs` around `self.solve.ui(ui)`), or accept `{}`-param runs |
| 8 | `assets/icons/ui/source/icons.rs:144-1464` `match name` | add `"vessel" => {…}` arms for Triya icon names | Yes — reuse existing icon names (`"revolve"`, `"cylinder"`, `"shell"`, `"export"`) or `""` (plain square fallback :1462) |
| 9 | `assets/locales/en.json` (+ `cargo xtask lists --fix`) | `"cmd.vessel_create": "Pressure Vessel"` etc. and any new tab/panel word | Yes — labels fall back to English text (`i18n.rs:87`); required only if Triya commands are registered inside the ui-egui test binary |
| 10 | `crates/engine/src/tests.rs:675-734` `keys` of `hostile_params_never_panic` | add Triya param names | Yes — sibling hostile test in `triya-vessel` over its own `COMMANDS` |
| 11 | `crates/mcp/src/tools.rs:61-280, 372-434` | a dedicated `vessel` MCP tool | Yes — `execute`/`batch` reach any registered command |
| 12 | `crates/mcp/src/tests.rs:1124-1131` `include_str!` list | add `skills/triya-vessel/SKILL.md` | Yes — sibling test in a Triya crate |
| 13 | `crates/engine/src/cmd/view_dxf_projection.rs:10,244` `pub(super) fn connect/drawing` | make `pub` for reuse of hidden-line removal by `triya-drawing` | Yes — call `file.export_view_dxf` as a command (returns `dxf` text without `path`) or write own projection on `ModelBody::mesh()` |
| 14 | `apps/solidcraft-web/sw.js:33,36` + `xtask/src/web_offline.rs:76` template path | fork-named cache prefixes; parameterise the template path for `triya-web` | Partly — own `sw.js` copy in `triya/apps/triya-web`, own post-build hook |
| 15 | `.github/workflows/{release,claim,winget,snap-store,flatpak-pages,pages}.yml` | disable crons/`release:` triggers; fix `--public-url /solidcraft/app/` | No (they fire in any fork with Actions enabled) — delete or add `if: github.repository == 'bherbruck/solidcraft'` |
| 16 | `crates/ui-egui/src/update.rs:27-33,53` | repoint `REPO`/API URLs or leave and disable `Services::http_get` in Triya apps | Yes (`http_get: None`, `http_download: None`) |
| 17 | Brand display strings (§2.3 table) | "Triya CAD 3D" where the fork's apps show them | Yes for strings inside `crates/ui-egui` (home/help/about) only by accepting "SolidCraft" there; app-level strings live in the fork's own `apps/triya` |
| 18 | `crates/doc/src/format/pending/<name>.rs` (new file) | only if a vessel definition cannot live in `metadata["com.triya.vessel"]` | Yes — use `document.metadata.set` (§4.2); no format change |
| 19 | `.config/nextest.toml` `default-filter` | exclude Triya heavy tests from PR shards | Yes (`#[ignore]` + env gate) |
| 20 | `ATTRIBUTION.md` | rows for every Triya file under `assets/` or with an asset extension (icons exported as PNG, STEP/STL fixtures, screenshots) | No, when such files exist (`cargo xtask assets` fails otherwise); yes if Triya ships no file assets and draws icons in code |

Recommended minimum for a first build: seams **1 or (1-alt + patch copy)**, **2**, **3**, **15**,
plus **20** as needed. Everything else can start avoided and be revisited.
