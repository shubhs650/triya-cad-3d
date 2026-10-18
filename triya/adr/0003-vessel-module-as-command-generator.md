# ADR-0003 — The vessel module is a command generator, not a new feature kind

**Status:** accepted 2026-10-17 (implemented for cylinders in the kickoff skeleton)

## Context
SolidCraft has no plugin or custom-feature API. Three ways to add parametric vessels:
(a) external scripts/MCP only; (b) a new `FeatureKind::Vessel` in `crates/doc` with its own
evaluator, format migration and dialogs; (c) an out-of-tree crate that registers commands which
*emit ordinary native features*.

## Decision
(c). `triya-vessel` owns a `triya.vessel/0.1` JSON document (source of truth), resolves it
(stations, radii, frames), and emits a **plan of native commands** — sketch profile → revolve,
extrude cut/join for nozzles, circular pattern for legs/chairs/bolts, coil for limpet coils,
`parameters.set` for every dimension — executed by calling the other specs' `run` from inside
`vessel.regenerate` so the whole regenerate is one undo step. Generated features are tracked
(ids in the metadata record; `triya:vessel-group` tag) so `vessel.regenerate` replaces only its
own features. Commands: `vessel.new`, `vessel.set`, `vessel.regenerate`, `vessel.inspect`,
`vessel.add_element`, `vessel.add_attachment`, `vessel.import_tables`, `vessel.export_bom`.
Registration happens at runtime through the engine's `triya_extra` slot, called by the Triya app
shells (a static reference would be a Cargo cycle).

## Reasons
- Zero edits to `doc`/`engine` evaluators or the file format; the `.solidcraft` file opens in
  unmodified upstream with the geometry present as plain features.
- Kernel-agnostic: when upstream swaps truck for SMLib, the commands survive ("everything is a
  command" is upstream's own invariant); only our `kernel-cases/` need re-running.
- Same commands serve the egui panels, the CLI, JSON scripts and MCP → agent-driven vessel
  modelling from datasheets comes for free.
- This is the shape upstream itself sketched for plugins (`docs/plugins.md`, "v2a group header":
  a header feature + ordinary native children) and the pattern proven by the 3,323-step SELENE
  model built purely via MCP.

## Consequences
- Editability is at the level of the vessel document (edit JSON/panel → regenerate) plus
  upstream's parameter table (named `V<id>_*` parameters). Direct manipulation of a generated
  feature is possible but is overwritten by the next regenerate — the UI says so.
- Regenerate cost is the sum of the native features; the beta DoD vessel must regenerate in
  < 10 s in the browser; above 200 elements / 500 attachments we refuse (hostile caps).
- Upstream's engine/ui test binaries never see Triya commands; `triya-vessel` carries its own
  hostile-params and registry tests.

## Rejected
- (a) external only: no in-window UX, not PV-Elite-like.
- (b) `FeatureKind::Vessel`: touches the three most-churned crates and the format on a fork of a
  project releasing several times a day; un-mergeable.
