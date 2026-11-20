# UI-02 · Composable native controls and custom content

Accepted with verification. The [content contract](../contracts/ui-content.md) is designed. App, provider and runtime checks remain unrun. This decision uses the [workspace contract](../contracts/ui.md).

## Decision

Publish a retained, keyed body per mount. The host renders Craft controls and slots for bespoke paint, custom 2D canvas, GPU resource and verified local HTML. A worker-rendered scene with a semantic mirror would broaden toolkit choices. It would also put ordinary controls' resize, feedback, accessibility and IME on a second runtime and tie them to frame transfer. Keep native controls local and use leased visual content for custom rendering. Native control decorations preserve app focus indication, accessible state and ownership. This covers ScriptUI's authored control paint and interaction while fitting inside the shared framework.

UI-02's Questioner found that frame replay and virtualization depend on more than body revision or visible row count. The host prepares an atomic candidate off paint and stamps independent paint/measurement and interaction dependencies. Generation-aware invalidation covers offscreen focus/hit/accessibility. Collections expose prepared extent/rank and semantic lookup with bounded paging. Optional providers supply a task-equivalent fallback. A rejected candidate keeps the last authorized snapshot. An authorization or dependency change removes stale pixels and semantics without waiting for a worker. Source evidence (review record kept outside this repository)

## Engineering slices

| Slice | Owner and change | Proof |
|---|---|---|
| UI02-CONTRACT | Platform. Version standard nodes, keyed full/update message, semantic identity, leaf provider, extent pages and stable actions. Validate the complete candidate, its inactive branches and scope. | `UI02-update`, `UI02-invalidation`, cross-version fixtures |
| UI02-FRAMEWORK | Craft at the converged pin. Fix skipped focus replay in `ui-contract/src/runtime/lazy.rs`, introduce prepared layout and semantic lookup through `ui-widgets/src/list/view.rs` and `ui-contract/src/contract/items`. Add typed control decoration and story per variant. | `UI02-collections`, `UI02-mixed`, `UI02-cost` |
| UI02-MOUNT | PhotoCraft `panels/panel_docking.rs` `DockPanels::body/content` and Properties. EffectCraft's corresponding body and inspector after UI-01 port. Replace old paint callbacks after the equivalent scenes pass. | `UI02-mixed`, `UI02-canvas`, paired baseline |
| UI02-PROVIDER | Platform, app graphics and recovery owners. Tie frames to RENDER-02 descriptors and device generation. Recover on compositor failure separately from shell device loss. Correct EffectCraft recovery at `app/gpu.rs:116–140` to preserve unsaved state on recovery-write failure. | `UI02-device` |
| UI02-HTML | HTML process under RUN-02. Verify immutable bounded assets, offline renderer and descendants, co-composition, modality and versioned bridge admission. Advertise independently by backend. | `UI02-html` and RUN-02 offline gate |

## Review results

Q1/Q2 use mixed-tree, conditional fallback and atomic patch with compatible binding lifetime. Q3 uses external dependency and semantic generation invalidation. Q4 uses a prepared extent index through sizing as well as draw. Q5 uses semantic lookup/reveal and validated actions across locale and DPI. Q6 budgets input, scratch and output before expansion and counts retired resources. Q7 uses a verified offline renderer and generation-bound bridge. All six B findings accepted. C passed with amendments. The contract includes semantic single ownership and post-reveal checks, authority-dependent fallback, preparation work accounting, bidirectional bounded HTML I/O with synchronized deactivation, and distinct producer and shell device-loss behavior.

Test both apps on matched native and public SDK fixtures. Evaluate the 8 ms framework budget and 1 ms p95 increment hypotheses. Count row probes and allocations in both sizing and drawing at 10,000 rows. Include locale/contrast changes, stale actions, native viewport reparenting, unrecoverable shell device loss, busy panels and worker stall. [Acceptance cases](../implementation/ui-02-acceptance.md)

The Q, A1, B, A2 and C review roles ran in order. All roles used GPT-6.2 Sol at high reasoning.
