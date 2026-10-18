# Session prompts

Paste one of these as the first message of a Claude Code session in the repo. `resume.md` is the
default for any session after the first. Phase prompts are entry points when you want to steer a
session to a specific phase; the agent still reads PROGRESS.md and PLAN.md first.

| file | when |
|---|---|
| `00-kickoff.md` | once, right after REPO-SETUP.md §1–7 |
| `resume.md` | every other session (default) |
| `01-rebrand-pwa.md` … `07-hardening.md` | optional phase-specific kick-offs |
| `08-first-upstream-merge.md` | after the beta tag exists |
| `review.md` | an independent review pass (run in a *fresh* session, not the one that wrote the code) |
