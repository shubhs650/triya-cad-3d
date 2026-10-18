# ADR-0001 — Platform: fork SolidCraft v0.14.0; do not build on usd-brep/solidcore for the beta

**Status:** accepted 2026-10-17 · **Owner:** Shubham

## Context
Triya CAD 3D needs Fusion-grade parametric modelling, a PWA, MCP, and a pressure-vessel add-on
with auto fabrication drawings, with a beta in 15–29 days. Options: (1) fork SolidCraft
(tangency-labs, Rust, MIT OR Apache-2.0, truck kernel, web/desktop/MCP); (2) build from zero on
NVIDIA usd-brep (SMLib, Apache-2.0) or its Rust port solidcore.

## Decision
Fork SolidCraft at tag **v0.14.0** (2bb9c81, 2026-10-16). Treat it as a *platform to drive*
(commands, scripts, MCP), not a codebase to rewrite.

## Reasons
- usd-brep is a kernel only: no sketcher/constraint solver, no feature history, no STEP/IGES.
  SolidCraft's maintainer estimated 6–8 agent-weeks just to put SMLib behind an *existing* app
  (solidcraft #331). solidcore had 2/9 boolean and 1/22 fillet units ported on 2026-10-17 and
  states "nothing is usable end to end". ClassCAD/buerligons needs a paid key for public WASM.
- SolidCraft ships today: in-house sketch solver, `FeatureKind` timeline, 383-id command
  registry shared by toolbar/scripts/CLI/MCP, STEP AP242 + IGES I/O, a true `wasm32` PWA with
  OPFS and an offline service worker, CI with fmt/clippy/tests/layers/wasm gates and an 82/82
  oracle.
- Licence permits a rebranded fork with attribution; no CLA, no trademark clause in the repo.

## Consequences / known costs
- The truck kernel is **frozen** upstream (to be replaced by solidcore/SMLib FFI, no date) and is
  weak on curved-edge fillets and coincident curved booleans → beta rules in CLAUDE.md §4.
- No plugin API; closed `FeatureKind` enum; format bumped 22→25 across two releases → ADR-0003
  (commands, not FeatureKind) and ADR-0005 (no format changes).
- ~150 commits/day upstream, bus factor ≈ 1 → ADR-0002 (overlay + freeze + merge-based sync).
- Single-threaded wasm (~30 MB per upstream's own comment): heavy booleans block the UI; show
  progress, keep vessels under the DoD size.

## Revisit when
Upstream lands the SMLib kernel (P5 in PLAN.md), or SolidCraft development stops for > 60 days
(then evaluate: continue frozen, or migrate the vessel/drawing crates — which are kernel-agnostic
by design — to another host).
