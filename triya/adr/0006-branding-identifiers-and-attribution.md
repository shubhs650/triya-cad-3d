# ADR-0006 — Rename the display layer only; keep upstream identifiers; attribution wording

**Status:** accepted 2026-10-18

## Decision
**Rename**: display name "Triya CAD 3D"; organisation "Open Engineering Tools"; app/bundle id
`in.openengtools.triya`; logo/icons (drawn in code, original); URLs; PWA manifest; web
`data-bin`; binaries under `triya/apps/` (`triya`, `triya-cli`); settings/recovery folder names;
MCP `serverInfo.name = "triya-cad-3d"`; README; About.

**Keep exactly as upstream**: `.solidcraft` file extension; `"format": "solidcraft/N"`;
`solidcraft-*` crate names; `solid.*`/`sketch.*`/… command ids; `SOLIDCRAFT_*` environment
variables; upstream's `solidcraft-cli` crate (our `triya-cli` wraps it).

**Attribution** (NOTICE, About dialog, README, Pages footer), always in words, never with the
upstream logo:

> Triya CAD 3D is based on SolidCraft v0.14.0 (commit 2bb9c81), © 2026 SolidCraft contributors,
> licensed MIT OR Apache-2.0. Triya CAD 3D is an independent project of Open Engineering Tools and
> is not affiliated with or endorsed by the SolidCraft project.

Modified upstream files carry `Modified by Triya CAD 3D: <why>` (Apache-2.0 §4(b)); JSON seams are
recorded in PATCHES.md. Both LICENSE files, `NOTICE` and `ATTRIBUTION.md` are retained; Triya
assets and derived tables are added as ATTRIBUTION rows. All new files carry SPDX headers
(`MIT OR Apache-2.0`).

## Reasons
- Files remain interchangeable with upstream; upstream merges never collide on identifiers; the
  kernel swap lands without Triya noticing.
- Apache-2.0 §6 grants no trademark rights; the repo has no trademark policy, which is not proof
  the name is free. Dropping "SolidCraft" from all UI chrome except the attribution sentence
  follows Mozilla/Rust-style norms ("based on" in words, "not affiliated" equally prominent).
- Upstream's `docs/compatibility.md` shows what a *full* rename costs (file ext, format tag,
  env vars, folders, packaging, CLI alias with shims and tests). Not worth it for a fork.

## Consequences
- Users see "Triya CAD 3D" but save `.solidcraft` files — documented in the README FAQ.
- The web shell keeps upstream's `index.html` element ids (shared `loading.css`); only visible
  strings, manifest, icons and the cache prefix change.

## Amendment 2026-10-28 — `.sc` design extension
Upstream v0.25 saves new designs as `.sc` and still opens `.solidcraft` (its compatibility
shim). Triya follows upstream (owner, 2026-10-28, plan D1): the rule was never the spelling, it
was that files stay interchangeable with upstream, and `.sc` is what upstream now writes. Triya
writes `.sc` for new designs, keeps an opened file's own extension on Save, and opens both.
The format tag `solidcraft/N` and every other identifier above are unchanged.
