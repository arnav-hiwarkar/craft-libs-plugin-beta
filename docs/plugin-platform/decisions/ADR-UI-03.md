# UI-03, UI-04 and DOC-01 · Input-to-edit invocation

Accepted with verification. These bank topics share the same target, authority and publication boundary. The [invocation contract](../contracts/operations.md) is designed. Runtime proofs remain pending.

## Decision

Tools and commands make proposals against explicit, captured targets. The owning app resolves document and editor dependencies and stages typed edits in one candidate off paint. On publication it checks live authority and dependencies, makes one Undo, and publishes the receipt, journal and outbox together. This uses ordinary engine algorithms and requires extracting explicit target resolution and bookkeeping from some current commands. A shadow Session can run more of the old registry initially. Ambient selection, Action recording, revision updates, generated jobs, inline observers and partial rollback would have to be emulated and then reconciled. Use engine-owned plans for one consistent publication path. Cover every command with a mapped descriptor, a narrowly defined unavailable reason, or a professional workflow that needs a new provider. A1 options (review record kept outside this repository) · Source evidence (review record kept outside this repository)

Host input arbitration, hit testing and immediate feedback remain local. Accurate pen and motion samples use an ordered, bounded window, with sealed terminal sequence. After End, pending publication can still cancel. Hooks run async on an admitted effect envelope, with per-recipient disclosure and lineage of surviving changes. Each effective leaf re-enters admission at the real sink after substitutions. Projected snapshots keep frozen time and explicit scope and charge work even on empty pages. Observer recovery reuses an authorized snapshot/watermark cut, separate from edit receipts.

## Implementation slices

| Slice | Owner and mapping | Exit proof |
|---|---|---|
| ENG-EC-EDIT | EffectCraft engine. Extract a target-resolved pure plan from `cmd/puppet.rs`, `session/edit.rs` and `session/dispatch.rs`. Use independent scrub and selection dependencies from `session/comps.rs`, with view-local speculative time. Publish exact project delta; optional stale editor effects skip. Stage analysis invalidation after the commit record. | `EDIT-editor` and `EDIT-atomic`, including current built-in history behavior |
| ENG-PC-EDIT | PhotoCraft engine. Refactor `session/execute.rs`, `jobs.rs` and `cmd/actions_cmds/history.rs` to prepare and publish explicit target history, relative asset path, render context, selection, coalescing and damage with no live tab swap. Explicit parent Action owns publication. | `EDIT-target` and `EDIT-atomic`, both with Action recording and inactive A |
| EXT-OPS | Platform. Generate stable descriptor schemas and resolution metadata with app owners. Common admission covers all effective dispatch leaves, hooks, nested callbacks and post-commit subscriptions. Add typed work-metered snapshot slices in both engine query paths. | `EDIT-hook`, `EDIT-events`, `EDIT-bulk` and RUN-02 permission/denial evidence |
| EXT-TOOL | Craft and both app view owners. Build one capture contract over PhotoCraft ToolHost/paint thread and EffectCraft viewer/Puppet tool. Use local stroke footprint and path feedback, async prepared result, damage or revision-driven redraw and exact Undo/redo. Use shared UI-01 claims and UI-02 canvas data. | `EDIT-capture`, `EDIT-cancel`, `EDIT-overlay` and the matching UI and headless paths |

[Acceptance cases](../implementation/edit-acceptance.md). Compare matched native and public SDK gesture latency on both apps. Hypotheses are next-refresh local feedback under worker stall, no worker wait in paint/release, and zero per-element IPC for a bulk projection. Count delivered samples, UI passes, frame and completion latency, preparation steps, slices and bytes.

## Review resolution

Q1–Q3 use one view/tool capture, explicit transforms, terminals and preview isolation. Q4–Q5 use the app command catalogue, approved scoped replacement, causal hooks and post-commit notifications. Q6 uses frozen, bounded bulk query leases. Q7 uses a single target-aware owner publication. All six B findings accepted. C passed with amendments. The contract includes resolved leaf authority after macros and frontend effects, approved hit arbitration and contiguous samples, bound watermark and delivery authorization, measured snapshot work and a reserved atomic owner commit with one parent Undo.

The Q, A1, B, A2 and C review roles ran in order. All roles used GPT-6.2 Sol at high reasoning.
