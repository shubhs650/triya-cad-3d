# Triya CAD 3D — progress

**Phase:** 0 — Fork, freeze, skeleton · **Day:** 2 of 29 (Sun 18 Oct 2026) · **Beta:** Sun 8 Nov
**Next task:** T0.2 (CI baseline) → T0.3 (triya-cli, web build) → T0.4 → T0.5 → T0.6
**Blockers:** none
**Freeze point:** SolidCraft v0.14.0 @ 2bb9c8106920d2e2ac92e9f9fa90af0de0bfcaae (2026-10-16)

## Done
| id | task | PR | date | notes |
|---|---|---|---|---|
| — | Research + platform decision (report "Triya CAD platform choice and plan") | — | 17 Oct | ADR-0001..0007 record the outcome |
| — | Kickoff kit: docs, workflows, prompts | — | 18 Oct | this folder |
| — | Kickoff skeleton: 6 crates compile; tests triya-brand 2/2, triya-stdlib 9/9, triya-drawing 5/5, triya-vessel 7/7; `cargo xtask layers` OK; clippy/fmt clean; cylinder regenerates as one undo step, volume within 0.1 %; metadata persistence round-trips | — | 18 Oct | `SKELETON-REPORT.md`; not yet run: triya-ui tests, wasm/trunk build, upstream test suites |

## In progress
| id | task | branch | started | owner |
|---|---|---|---|---|
| T0.2 | CI baseline: `cargo xtask ci` + Actions green | ▶ in progress — triya/p0-ci-baseline — 18 Oct | 18 Oct | agent |

## Decisions (dated; architectural ones also get an ADR)
- 2026-10-17 — Fork SolidCraft at v0.14.0; usd-brep/solidcore rejected for the beta window (ADR-0001).
- 2026-10-17 — Overlay architecture with whitelisted seams; merge-based sync; freeze until beta (ADR-0002).
- 2026-10-17 — Vessel module = command generator, not a FeatureKind (ADR-0003).
- 2026-10-17 — Drawings data-driven, native Rust, no OCCT (ADR-0004).
- 2026-10-18 — Vessel JSON persisted in `Document.metadata["in.openengtools.triya.vessel"]` (ADR-0005, confirmed in code).
- 2026-10-18 — Keep all upstream identifiers; rename display only (ADR-0006).
- 2026-10-18 — EIL tables never in the repo; private import (ADR-0007).
- 2026-10-18 — Command registration via a runtime extension slot (`engine/cmd/triya_extra.rs`) called by Triya app shells; static reference impossible (Cargo cycle).
- 2026-10-18 — Skeleton adds a `VESSEL` ribbon tab (seam + 3 locale lines); revisit at T5.1.
- 2026-10-18 — CI toolchain = current stable (1.99.1 on Actions, `dtolnay/rust-toolchain@stable`), not 1.95: `rust-version = "1.95"` is upstream's MSRV, and upstream v0.14.0's own `clippy -D warnings` fails on 1.95/1.96 (`geom/fill_general.rs:166` nonminimal_bool) and 1.97 (`kernel/prism_local.rs:242` manual_range_contains) but passes on 1.99.1. Local gate: `cargo +1.99.1 xtask ci`.
- 2026-10-18 — Desktop app is a thin wrapper around `SolveApp`, not a copy of upstream's 2,700-line app.

## Owner decisions
| opened | question | status | answer |
|---|---|---|---|
| 2026-10-18 | Default angle convention: 0° = Plant North, clockwise looking down (vertical) / 0° at top, clockwise looking from the left end (horizontal)? Ship configurable; needs a default. | open | |
| 2026-10-18 | Default sheet size & title block (A1 landscape, OET block) for the beta? | open | |
| 2026-10-18 | Pages host: `<you>.github.io/triya-cad-3d` for the beta, custom domain later? | open | |
| 2026-10-18 | Disable upstream's `Pages` workflow (Settings → Actions → Pages → Disable): it fails on every push to main and would compete with `triya-pages.yml` for the Pages site. | open | |
| 2026-10-18 | Ribbon: dedicated `VESSEL` tab (keeps 4 seam rows) or commands inside an existing tab (zero UI seams)? Decide by T5.1. | open | |

## Kernel cases (`triya/crates/triya-vessel/tests/kernel-cases/`)
| case | symptom | fallback engaged | upstream issue |
|---|---|---|---|

## Deferred (post-beta, with reason)
| item | reason | target phase |
|---|---|---|
| Curved-edge fillets (knuckle, pad welds) | truck weak on curved fillets; upstream kernel frozen | P5 |
| Generic HLR drafting of arbitrary parts | not needed for vessel GA; 1–2 weeks | P2 |
| True DXF DIMENSION entities | needs anonymous blocks | P1 |
| Davit geometry on manways | reference only in beta | P1 |
| Heat exchangers | model stability first | P4 |
| Hex meshing | post-beta by plan | P3/P4 |
| Triya icon artwork (web icons dirs are empty placeholders) | drawn-in-code icons first (T1.1) | Phase 1 |

## Dependencies added (crate — licence — why — wasm ok)
| crate | licence | why | wasm |
|---|---|---|---|
| (skeleton adds no third-party crates beyond upstream's set) | | | |

## Measurements
| what | value | date | notes |
|---|---|---|---|
| cylinder 1000 ID × 10 t × 2000 L metal volume | π(510²−500²)·2000 ≈ 63.46e6 mm³, measured within 0.1 % | 18 Oct | skeleton engine test |
| Klöpper h2 (Da 1000, s 10) exact geometry vs DIN fit | 188.89 vs 188.95 | 18 Oct | DIN uses a linear fit; tolerance 0.5 mm |
| Korbbogen h2 (Da 1000, s 10) | 248.26 vs 248.65 | 18 Oct | same |
| ASME F&D depth / D (L = D, r = 0.06 D) | 0.16934 | 18 Oct | |
| upstream wasm size (release, `wasm-opt z`) | TBD | | measure at T0.6 |
| reference vessel A regenerate (native / wasm) | TBD | | target < 10 s wasm |

## Log (append-only)
- 2026-10-17 — Deep research completed; decision taken.
- 2026-10-18 — Kickoff kit written; upstream cloned at v0.14.0; skeleton built and tested natively (see SKELETON-REPORT.md).
- 2026-10-18 — T0.2: main @ 3fa507f had red `Triya CI` (web-shell: `web-offline-test` regenerates sw.js from upstream's template), red `Triya Pages` (same), red `Packaging lint` (shellcheck SC2059/SC2015 in `upstream-sync.yml`, fixed in T0.2), red upstream `Pages` (owner decision above). Upstream `CI` green on Actions.
