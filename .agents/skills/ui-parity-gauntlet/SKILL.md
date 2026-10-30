---
name: ui-parity-gauntlet
description: Match the Sloppy Tanks egui shell (the desktop app and, since PR 125, the web page) to the frozen pre-migration web page through repeated capture, implementation and independent visual critique. Use for UI parity with the former web interface, faithful UI ports or a visual gauntlet loop in this repository; not open-ended redesign or generic code review.
---

# UI Parity Gauntlet

Run the work in the current session. Default to the frozen pre-migration web page
(the TypeScript/DOM shell's production build of `90423be0`, built locally; see
[the project workflow](references/sloppy-tanks.md)) and the egui shell in
`crates/desktop`. The live site is no reference any more: it serves the egui shell
itself. Use the selected interface as the acceptance
reference: **faithful reproduction**, not a contest to make a different design
look better. Default to execution unless the user asks only for a plan or prompt.

## Set up the comparison

- Resolve the reference URL/build, target app and owning source from the user's
  request and repository. Read applicable project instructions and check Git status.
  Ask only for genuinely missing targets; continue independent discovery meanwhile.
- Before editing, reuse a suitable isolated branch and worktree or create them.
  Keep the primary checkout on its original branch and preserve user changes.
  Use managed-worktree tools when available, or Git's branch/worktree commands.
  Do not automatically pull,
  reset, or switch a dirty checkout to obtain a newer reference.
- Inventory meaningful UI states, including menus, Settings, pause/controls, HUD,
  diagnostics, results, loading/empty/error states and role-dependent dialogs.
  Choose matched viewport dimensions, content size, pixel ratio, theme and fixture
  data. For desktop ports, a practical minimum size and a few desktop layouts may
  be sufficient; do not expand into phone/responsive support without a reason.
- Read [the project workflow](references/sloppy-tanks.md) before choosing
  capture/build commands. Reuse the repository's existing harness.

## Capture and build

Capture the actual reference and the actual target before broad changes. Await
fonts, images, readiness and settled layout. Pin scores, names, roles, ammo, timer
history, connection state and animation phase so content differences do not
masquerade as layout defects. Test fixtures must represent the same semantic state
in both apps. Verify suspicious mismatches against real interaction or owning code.

Freeze the reference for a run. Save its URL/build identity and asset hashes where
available, fixture identity, source fingerprint, screenshots and commands. Use new
round directories; preserve older evidence. Fixture repairs must retain the same
reference build and asset identity. If the harness cannot replay that build,
record the blocker; switch builds only when the user's request authorizes it and
start a new campaign. Deliberate reference updates must not clear failed reviews.
Reject source edits during capture and invalidate evidence after relevant edits.

Fix the largest supported discrepancies first: geometry, typography, content,
controls and states, then colors, icons, borders and effects. Reuse existing assets
and design values. Keep gameplay/business rules and platform behavior intact.
Add focused deterministic regressions for changed behavior; exercise pointer
routing when click targets or dialogs change. Visual captures complement tests.

## Independent critique loop

Use separate builder and read-only critic sub-agents when available. The lead owns
integration; builders may own disjoint surfaces. Give a fresh critic real captures,
the comparison scope and allowed differences, without the builder's self-assessment.
Start with identity-scrubbed A/B images when practical, then reveal the mapping for
source tracing. Do not replace inspection with a numerical similarity score.

Critic task:

> Inspect every covered state against the fixed reference. Judge fidelity, not
> taste. Return **FIX** or **ACCEPT WITHIN REASON**. For each fix, give the scenario,
> observable discrepancy, priority, evidence path and owning code when established.
> Distinguish an app defect from a fixture/capture error. Identify missing coverage
> and disclose accepted platform differences. Do not implement fixes or certify
> screenshots you have not viewed.

Address supported findings, run appropriate checks, capture a new round and rejudge.
Keep the reference fixed between those rounds. A completed capture means
`needs-independent-review`, not a pass. If independent agents are unavailable,
disclose that limitation instead of presenting self-review as independent approval.

## Finish or resume

Stop successfully only when the critic accepts within the agreed scope, relevant
checks pass, and the accepted evidence matches the final source. Record the verdict,
remaining differences and validation beside the run ledger. Do not lower the bar
just because a round count is reached. Honor user interruption or explicit budgets;
record a concrete blocker and next action when completion is impossible.

On resume, read the existing ledger and verdict, verify source/reference identities,
and continue from the recorded gaps. Do not restart accepted work or reuse stale
approval for changed code. Finish with the branch/worktree, comparison report, checks,
remaining limits and a command to reproduce the loop. Be explicit about the limits
of fixture screenshots versus a live app play-through.

This workflow does not require publication, packaging uploads or changes to the
reference service. Treat any requested external release as a separate action.
