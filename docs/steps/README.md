# Steps

One file per step — the live plan since 2026-09-27, when the plan of 2026-09-25 closed into
[`../completed/2026-09-25-STEPS.md`](../completed/2026-09-25-STEPS.md).

`<S-number>-<slug>-<status>.md`: the slug is a few words and the **status is last**, one of `todo`,
`doing`, `blocked`, `done` — so a directory listing is the plan. A step changes status by renaming its
file (`git mv S32-…-doing.md S32-…-done.md`) in the same commit that rewrites its `**Progress**` line.

Each file carries the step's own goal, work, machine-checkable exit, `Human` line, rulings, `Result` and
that one `**Progress**` line, which is the authority on where the step stands. The binding rule is
`AGENTS.md`, "Step discipline"; the S-numbering continues across plans, and `docs/CONTRACT.md` stays the
authority for every shape.
