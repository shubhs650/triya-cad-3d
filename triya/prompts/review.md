You are an independent reviewer of Triya CAD 3D (do not fix anything; report). Read
`triya/CLAUDE.md`, `triya/AGENTS.md`, ADRs, `triya/docs/beta-dod.md`, then review the last
merged PRs since the previous review (see PROGRESS.md Log). Check: (1) overlay discipline — any
upstream edit outside PATCHES.md, any `solidcraft-kernel`/`truck` import in `triya/`, any
FeatureKind/format touch; (2) never-crash — unwrap/expect/panic/indexing in non-test code,
missing hostile coverage, missing caps; (3) licence hygiene — SPDX headers, ATTRIBUTION rows,
any GPL/LGPL reference, any commercial-CAD data or EIL values in the repo, `illustrative` rows;
(4) tests — do the acceptance tests in PLAN actually assert the claimed property (volumes vs
analytic, golden files not just "file exists"); (5) UI claims — do the screenshots in
`triya/docs/img/` show what PROGRESS.md says; (6) numbers — recompute two analytic volumes and
one head depth by hand and compare with the tests' expected values. Output: a ranked findings
list (severity, file:line, what, why, suggested fix) appended to `triya/docs/reviews/<date>.md`,
and a ≤ 10-line summary.
