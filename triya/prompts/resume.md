Continue Triya CAD 3D development. Read `triya/PROGRESS.md`, then the current task(s) in
`triya/PLAN.md` and their acceptance tests, then the ARCHITECTURE/docs sections they touch. Run the
AGENTS.md §1 session loop until the session budget is nearly used: claim → test first → implement
in `triya/` → verify (`cargo xtask ci`, Triya tests, whitelist, look at UI output) → record → PR →
merge when green → next task. Use parallel subagents in worktrees only for tasks marked ∥, each
on disjoint crates; subagents add files and never replace existing folders. Never merge upstream
before the beta tag; never edit upstream files outside PATCHES.md. If a task hits 150 % of its
estimate, stop it, write up state and a cut proposal in PROGRESS.md, and move to the next
unblocked task. Finish with a ≤ 10-line owner summary: PRs merged, measurements, kernel cases
captured, owner decisions awaiting an answer, next task.
