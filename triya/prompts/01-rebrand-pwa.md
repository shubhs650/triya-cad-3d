Phase 1 (PLAN T1.1–T1.5): make the web app *Triya CAD 3D* without editing upstream's web shell.
Read `triya/docs/upstream-map.md` §6 (every brand point in upstream's `apps/solidcraft-web`) and
ADR-0006. The skeleton's `triya/apps/triya-web` already has the rebranded `index.html`, manifest,
`sw.js` constants and `Trunk.toml`; its icon directories are empty. Finish it: icons drawn in code
in `triya-ui::icons` and rasterised by a Rust build step (every generated file gets an ATTRIBUTION
row — never a hand-made binary asset without one), `manifest.webmanifest` theme/background
colours, `data-bin`, og tags. Wire `triya-brand` in the shells, not via seams: window title and
About dialog drawn around `SolveApp::ui` with `about_text()`, update checker `http_get = None`,
MCP `serverInfo.name = "triya-cad-3d"` through `triya-cli` (only if the server constructor has no
name parameter does this become a new PATCHES.md row — record it as an owner decision first).
Desktop shell `triya/apps/triya`: a thin wrapper around `SolveApp` (register commands, set
title); do not copy upstream's 2,700-line app. Gate: `trunk build --release`,
`cargo xtask web-offline-test --site dist/triya-web`, install the PWA in Chromium and screenshot
the installed window and the About dialog; put the PNGs under `triya/docs/img/` with ATTRIBUTION
rows ("own screenshot"). Draft the two upstream issue texts into
`triya/docs/upstream-proposals.md` (T1.5). PR per task; summary for the owner at the end.
