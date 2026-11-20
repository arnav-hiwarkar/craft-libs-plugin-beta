# ArtCraft plugin platform design

**Start here: [PLATFORM-SPEC.md](PLATFORM-SPEC.md)**, the single specification for owner review.

All 23 topics are DESIGNED, pending owner review. No topic is IMPLEMENTED or TESTED: there is no implementation and no runtime test has run. The final review returned READY_WITH_CORRECTIONS, and all 24 findings are applied (change log, [final reconciliation](cross-topic-conflicts.md#final-reconciliation-2026-11-20)). The source of truth is the pinned current-main app revisions and PhotoCraft's actual framework dependency.

- Workshop brief and owner addendum (workshop records, not in this repository)
- [Source pins](source-baselines.md) and [host source inventory](source-inventory.md)
- [Owner invariants](architecture/owner-invariants.md), [threat inventory](architecture/threat-model.md) and [verification rules](architecture/evidence-and-verification.md)
- [Question closure](question-closure.md), [capability coverage](capability-coverage.md) and [cross-topic conflicts](cross-topic-conflicts.md)
- [Platform ownership and integration summary](architecture/system.md), [phase plan](implementation/phases.md) and [dependency graph](implementation/dependencies.md)
- [Legacy migration inventory](implementation/legacy-inventory.md) and [proof workflows](implementation/proof-inventory.md)
- Workshop and models (workshop records, not in this repository)
- [UI-01 decision](decisions/ADR-UI-01.md), [workspace contract](contracts/ui.md), [implementation slices](implementation/ui-01-slices.md) and [acceptance cases](implementation/ui-01-acceptance.md)
- [UI-02 decision](decisions/ADR-UI-02.md), [content contract](contracts/ui-content.md) and [acceptance cases](implementation/ui-02-acceptance.md)
- [Input, commands and edits decision](decisions/ADR-UI-03.md), [invocation contract](contracts/operations.md) and [acceptance cases](implementation/edit-acceptance.md)
- [Persistent object decision](decisions/ADR-DOC-02.md), [document contract](contracts/document.md) and [acceptance cases](implementation/document-acceptance.md)
- [Processor decision](decisions/ADR-RENDER-01.md), [render contract](contracts/render.md) and [acceptance cases](implementation/render-acceptance.md)
- [Timed evaluation and audio decision](decisions/ADR-RENDER-03.md), [contract](contracts/time.md) and [acceptance cases](implementation/time-acceptance.md)
- [File, format, inference and job decision](decisions/ADR-IO-01.md), [contract](contracts/io-jobs.md) and [acceptance cases](implementation/io-acceptance.md)
- [Platform core, runtime and authority decision](decisions/ADR-RUN-01.md), [contract](contracts/runtime.md) and [acceptance cases](implementation/runtime-acceptance.md)
- [SDK, evolution and targets decision](decisions/ADR-RUN-03.md), [SDK contract](contracts/sdk.md), [reference examples](contracts/sdk-examples.md) and [acceptance cases](implementation/sdk-acceptance.md)
- [Conformance, parity and retirement decision](decisions/ADR-SDK-02.md), [conformance contract](contracts/conformance.md), [acceptance cases](implementation/conformance-acceptance.md) and the [reconciled phase plan](implementation/phases.md)
