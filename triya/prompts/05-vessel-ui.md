Phase 5 (PLAN T5.1–T5.5): the vessel UI. Read `triya/docs/upstream-map.md` §2 for how
tabs/panels/dialogs are built, ADR-0003 for the UI contract (thin UI; every edit is a
`vessel.set` JSON-pointer patch followed by regenerate; the same JSON must be reachable from MCP),
and the skeleton's `triya-ui::VesselPanel` (JSON box + buttons) as the starting point. T5.1 is
an owner decision already logged in PROGRESS.md: keep the `VESSEL` ribbon tab seam or move the
commands into an existing tab via `CommandSpec.at(…)` — implement whichever the owner chose (if
unanswered, keep the tab). Build in `triya-ui`, drawn by the Triya shells around `SolveApp::ui`:
stack editor with inheritance and datum selector (T5.2), attachment panel with host picker synced
to 3D selection, axial-ref choice, angle snapping toggle (22.5° shell / 5° head), frame type,
nozzle fields, standardRef/sizeKey pickers, flags (T5.3), plan-view orientation widget with the
configurable 0° reference and sense (T5.4). Colours from `theme::Tokens`; icons drawn in code;
panels call `app.run`, never `app.start`. For every panel: take a screenshot via the control
channel or `triya-cli snapshot`, look at it, iterate; commit PNGs under `triya/docs/img/` with
ATTRIBUTION rows. T5.5: model the owner-provided anonymised datasheet end to end and record
time-to-model and friction points in PROGRESS.md; propose the three highest-value UI fixes and
do them if within estimate.
