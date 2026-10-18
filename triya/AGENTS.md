# Triya CAD 3D — autonomous operation protocol

Upstream's `AGENTS.md` applies (coding conventions, lists many PRs add to, format rules). This
file is about *how a session runs* in the Triya fork: one owner (Shubham), one hard deadline
(beta by Sun 8 Nov 2026, window closes Sat 14 Nov 2026), agents doing almost all of the typing.

## 1. Session loop
1. **Orient** (≤ 5 min): read `triya/PROGRESS.md`, locate the current task in `triya/PLAN.md`,
   read its acceptance test, read the ARCHITECTURE section it touches, skim open PRs
   (`gh pr list`) and the `beta-blocker` issues.
2. **Claim the task** in PROGRESS.md (`▶ in progress — <branch> — <date>`), create the branch
   `triya/<phase>-<topic>` from `origin/main`.
3. **Test first**: write the acceptance test from PLAN.md (engine test, golden file, or
   hostile-params entry). Run it; it must fail.
4. **Implement** in `triya/` only. If you touch an upstream file, follow CLAUDE.md §2 (seam).
5. **Verify**: `cargo xtask ci`, Triya tests, whitelist check, and for anything the user sees,
   *look at it* (snapshot PNG or browser screenshot through the control channel). For geometry,
   check the measured volume against the analytic value and export STEP once.
6. **Record**: update PROGRESS.md (task → done, decisions, surprises, measured numbers such as
   regenerate time and wasm size), update ARCHITECTURE.md if a boundary moved, add an ADR if a
   decision was architectural.
7. **Land**: push, open the PR with the template below, wait for CI, `gh pr merge --squash --delete-branch`.
   One task per PR. Push work in progress at least once per session even if unfinished.
8. **Pick the next task**: the first unblocked task in PLAN.md order. Never jump phases unless
   PROGRESS.md says the current phase is done or the task is explicitly parallel-safe (marked ∥).

## 2. Parallel work
Tasks marked ∥ in PLAN.md may run in parallel subagents, each in its own `git worktree` and
branch, each owning disjoint crates (`triya-stdlib` vs `triya-drawing` vs `triya-ui`). A
subagent never edits a seam file; seams are applied by the coordinating session only. The
coordinator merges PRs in PLAN order and resolves `triya/PROGRESS.md` conflicts by hand
(append-only "Log" section makes this rare). A subagent that copies or moves directories must
never replace an existing folder wholesale (`cp -r`/`rsync --delete` onto `triya/` destroyed
the kickoff docs once) — add files, don't replace trees.

## 3. Decisions you make alone (record them, don't ask)
- Internal API shapes inside `triya/` crates, module layout, naming within the conventions.
- Which upstream command sequence implements a vessel element, as long as it is commands only.
- Test tolerances (document the reasoning), default values that come from public references
  (cite the reference in a comment), presets.
- Dependency additions that are `MIT`, `Apache-2.0`, `MIT OR Apache-2.0`, `BSD-2/3`, `Zlib`,
  `Unicode-3.0` or `CC0` **and** build for wasm32 **and** come from crates.io (no git deps);
  record each in PROGRESS.md "Dependencies".
- Deferring a sub-feature to post-beta when it threatens the phase's end date — but record it in
  PROGRESS.md "Deferred" with the reason, and keep the acceptance test as `#[ignore]` with a note.
- Kernel fallbacks per CLAUDE.md §4 (separate body + flag).

## 4. Decisions that wait for the owner (stop, write it up in PROGRESS.md "Owner decisions", continue with something else)
- Anything irreversible outside the repo: creating/deleting GitHub repos, changing repo
  visibility, publishing to a package registry or app store, registering domains, posting
  upstream issues/PRs in the owner's name (draft the text, the owner posts).
- Any upstream edit beyond the seams in PATCHES.md; any new `FeatureKind`, format change or
  kernel change.
- Adding a dependency with a licence not listed in §3 (incl. any GPL/LGPL/AGPL/MPL/SSPL: reject
  outright, do not ask), or a JavaScript dependency.
- Changing the beta scope or the beta date; dropping a DoD item (`docs/beta-dod.md`).
- Product-defining UX choices with no public reference to lean on (e.g. the default angle
  convention) — ship a configurable default, flag it.
- Anything involving money, accounts, credentials or user data.
- Merging `upstream/main` before `triya-v0.1.0-beta` exists.

## 5. When something breaks
- **Kernel failure** (boolean/fillet returns error or wrong volume): do not retry with random
  tweaks. Reproduce with a minimal `triya-cli run` script, save it under
  `triya/crates/triya-vessel/tests/kernel-cases/<name>.json`, apply the fallback, add a test
  that asserts the fallback engages, note it in PROGRESS.md "Kernel cases". Post-beta, these
  become the regression suite for upstream's kernel swap.
- **Upstream CI red on our PR** for reasons unrelated to our change (flaky shards, nightly-only
  tests, newer-clippy lints in upstream code): re-run once; if still red, note the job name in the
  PR and merge on `Triya CI` green only if the failing job is clearly unrelated — otherwise stop
  and record.
- **wasm build breaks**: it is always a Triya dependency pulling std-only features. Fix with
  `default-features = false` or a `cfg(target_arch)` split; never disable the wasm gate.
- **Time**: if a task is at 150 % of its PLAN estimate, stop, write what is done and what is
  left, and propose a cut in PROGRESS.md. The reduced-beta checkpoint (PLAN.md day 15) is the
  fallback scope.

## 6. PR template (copy into the description)
```
Task: PLAN <id> — <title>
Acceptance test: <path::test_name> (fails before / passes after)
Seams touched: none | <file> (PATCHES.md row <n>)
Looked at it: <snapshot path or "n/a (no UI change)">
Measured: regenerate <x> s native / <y> s wasm; volume Δ <z> %; wasm size <n> MB (when relevant)
Deferred / follow-ups: …
```

## 7. PROGRESS.md discipline
- Top block: phase, day number (day 1 = Sat 17 Oct 2026), next task, blockers — ≤ 10 lines.
- Sections: Done (table), In progress, Decisions (dated, one line each), Owner decisions
  (open/closed), Kernel cases, Deferred, Dependencies, Measurements, Log (append-only, dated).
- Never delete history; condense the Log monthly.

## 8. Tone and output
Write to the owner the way an engineer writes to an engineer: numbers, file paths, what changed,
what to look at. No motivational text. If a result is uncertain (an unverified table value, a
tolerance you chose), say so in the PR and in the code comment.
