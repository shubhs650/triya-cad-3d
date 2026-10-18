# ADR-0005 — Where the vessel document lives inside a `.solidcraft` design

**Status:** accepted 2026-10-18 (confirmed against `crates/doc/src/metadata.rs`; implemented and round-trip-tested in the skeleton)

## Context
`vessel.regenerate` needs the `triya.vessel/0.1` JSON back when a design is reopened. Triya must
not add a `FeatureKind` or a format `pending/` file (ADR-0002/0003): a fork pending file would
write `solidcraft/26` and collide with upstream's next format. The file must still open in
unmodified upstream.

## Decision
Store the vessel record in **`Document.metadata`** under the reverse-DNS key
`in.openengtools.triya.vessel` — upstream's sanctioned slot for other tools' data (set via the
`document.metadata.set` command; saved with the design, undoable, round-trips unvalidated). The
record is `{schema, doc, generated: {features}}`. Limits upstream enforces: ≤ 64 KiB per value,
≤ 256 KiB total → serialise compactly (skip defaults); above 60 KiB split into
`in.openengtools.triya.vessel.<n>` chunks (≤ 3) with a `….chunks` count (T2.5). The GA sheet
definition uses `in.openengtools.triya.sheet`.

## Options considered
1. `Document.metadata` — chosen.
2. A string user parameter `TRIYA_VESSEL_JSON` — fallback only if metadata disappears upstream.
3. Side-car file (OPFS/next to the design) — survives upstream but not file sharing; no.
4. Group-header feature carrying the JSON in its notes — depends on folder notes persisting; no.

## Consequences
- The vessel JSON must stay small and stable: `schema` field with semver; unknown fields
  preserved on read (`#[serde(flatten)]`), so older Triya opens newer files.
- A design may contain **one** vessel in the beta (multi-vessel post-beta: key per vessel id).
- If upstream adds a proper extension block (we propose it), migrate with a one-shot
  `vessel.migrate_storage` command.
