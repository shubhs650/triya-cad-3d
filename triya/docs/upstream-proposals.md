# Upstream proposals (drafts for the owner to post)

Issue texts for `tangency-labs/solidcraft` (REPO-SETUP.md §9, PLAN T1.5). The agent drafts them
and the owner posts them (AGENTS.md §4: nothing is posted upstream in the owner's name by an
agent). Each one, if accepted, removes seams or workarounds listed in `triya/patches/PATCHES.md`.
Line numbers refer to v0.14.0 (`2bb9c81`).

Fill in before posting: nothing. The fork's link is `https://github.com/shubhs650/triya-cad-3d`.
After posting, add the issue URL to the table and to PROGRESS.md "Log".

| # | title | removes | posted |
|---|---|---|---|
| 1 | Registration point for out-of-tree command arrays | seams 3–5 (`cmd/mod.rs` lines, `triya_extra.rs`) | — |
| 2 | Centralise brand constants | `triya-cli` MCP output rewrite, the About takeover, upstream names left in Triya's menus | — |
| 3 | `space_mouse` test fails when `SolveApp::new` is slow | the occasional re-run of upstream CI on fork PRs | — |
| 4 | Let an app add toolbar tabs at runtime | seams 6–9 (`TABS += "VESSEL"`, three catalog lines) | — |

---

## 1. Registration point for out-of-tree command arrays

**Title:** Registration point for out-of-tree command arrays

Hi! I maintain Triya CAD 3D (https://github.com/shubhs650/triya-cad-3d), a downstream fork that
adds a parametric pressure-vessel module on top of SolidCraft v0.14.0. It is based on SolidCraft
and not affiliated with the project.

The module is a set of `CommandSpec`s (`vessel.new`, `vessel.set`, `vessel.regenerate`, …) that
build geometry only by running SolidCraft's own commands. Today there is no way for a crate outside
`crates/engine` to add specs to the registry: `command_specs()` (`crates/engine/src/cmd/mod.rs`)
concatenates the engine's static per-module arrays, and `find_command`/`Session::execute` go
through it. The module's crate depends on `solidcraft-engine` (it needs `Session`, `CommandSpec`
and `params`), so the engine cannot list it either without a dependency cycle.

What we do in the fork: a new file `crates/engine/src/cmd/triya_extra.rs` with
`register(&'static [CommandSpec]) -> Result<(), String>` (idempotent for the same slice, refuses an
id already registered, case-insensitively) and `extend(&mut Vec<CommandSpec>)`, called at the end
of `command_specs()`. The app shells call `register` once before the first `Session`. A poisoned
lock reads as "nothing registered", never a panic.

Proposal: the same thing upstream under a neutral name (e.g. `cmd::extra::register`), or any
mechanism you prefer. It looks like the smallest useful step toward plugins (#1221): out-of-tree
commands get the toolbar, palette, scripts, CLI, control channel and MCP for free, because they all
read the registry. We'd be glad to send the PR, with tests: duplicate ids are refused, register is
idempotent, and a registered command runs through `Session::execute` as one undo step.

One thing to decide: upstream's `hostile_params_never_panic` and the registry tests can't see
runtime-registered commands, so a downstream crate has to carry its own sibling tests. We do that,
and a public test helper that runs the hostile-params sweep over a given slice would make it easier.

---

## 2. Centralise brand constants

**Title:** Centralise brand constants (product name, app id, URLs, About text)

Hi! Triya CAD 3D (https://github.com/shubhs650/triya-cad-3d) is a downstream fork of SolidCraft
v0.14.0. It is based on SolidCraft and not affiliated with the project. We rename only what users
see and keep every identifier (`.solidcraft`, `solidcraft/N`, command ids, `SOLIDCRAFT_*`), so
designs open in both.

The user-facing name is spread over many literals, so a downstream build can't change it without
editing upstream files. Examples in v0.14.0:

- `crates/mcp/src/server.rs:145`: `serverInfo: {"name": "solidcraft", "title": "SolidCraft"}`,
  with no parameter on `Server::new`;
- `crates/ui-egui/src/help.rs`: `"About SolidCraft"` (menu item and window title), the About body,
  `DOCS_URL`, and the first line of `diagnostics()`;
- the update checker's release repository, window titles, and status messages that name the app.

What we do in the fork instead of editing them: `triya-cli` passes the MCP server a writer that
rewrites `serverInfo` in the `initialize` answer, and the Triya shells take over `app.help.about`
to show their own window. Menu labels such as "About SolidCraft" still show the upstream name.

Proposal: one module (e.g. `solidcraft_engine::brand` or a small `solidcraft-brand` crate at L0)
holding `APP_NAME`, `APP_ID`, `DOCS_URL`, `RELEASES_REPO` and `about_text()`, with defaults equal to
today's values. Downstream builds could then override it in one place: a `Brand` value passed to
`SolveApp::new` / `Server::new`, or a one-file patch. `docs/compatibility.md` already lists the
renames this project went through (SolveCraft → SolidCraft), so this would also make any future
rename a one-file change. Happy to send the PR.

---

## 3. `space_mouse` test fails when `SolveApp::new` is slow

**Title:** `each_frame_hands_the_view_its_motion_through_the_drivers_mapping` depends on startup time

`apps/solidcraft/src/space_mouse.rs`, test
`each_frame_hands_the_view_its_motion_through_the_drivers_mapping` (around line 487) builds the
`Reading` with `now = Instant::now()` and only then calls `SolveApp::new(...)`, which takes about
0.5 s on a loaded CI runner. `Reading` values count as stale after `STALE_AFTER` = 150 ms (line 21),
so by the time `mouse.tick` runs, the rotation reads as `[0; 3]` and
`app.viewport.device_motion` is `None`. The test then panics at
`expect("the frame hands the page its motion")`.

We saw it fail on a fork's main branch and in a local run, and pass on a rerun. It only fails when
`SolveApp::new` is slow.

Fix: build the app first, then take `now` and the reading:

```rust
let mut app = SolveApp::new(solidcraft_engine::Session::default(), solidcraft_ui_egui::Services::default());
app.home.open = false;
let now = Instant::now();
let mut reading = Reading::new(now);
// … rest unchanged
```

(Context: Triya CAD 3D, https://github.com/shubhs650/triya-cad-3d, a downstream fork based on
SolidCraft v0.14.0, not affiliated with the project. We run your CI on our PRs.)

---

## 4. Let an app add toolbar tabs at runtime

**Title:** Let an app add toolbar tabs at runtime

Panel contents already come from `CommandSpec.tab`/`.panel` (`toolbar_custom::panel_lists`), but
the tab row is the fixed constant `crates/ui-egui/src/toolbar.rs` `TABS`, and the i18n tests require
every tab in each complete catalog. To show our `VESSEL` tab we add it to `TABS` and add one line to
`assets/locales/{en,ja,zh}.json`.

Proposal: tabs = `TABS` plus any `CommandSpec.tab` that isn't in it (ordered after the built-in
tabs), with the label taken from the catalog when present and from the tab id otherwise. The i18n
test would then cover only the built-in tabs. This depends on #1 (out-of-tree commands); post it
after #1 is accepted, or fold it into that issue.

(Context as above: Triya CAD 3D, https://github.com/shubhs650/triya-cad-3d, based on SolidCraft,
not affiliated.)
