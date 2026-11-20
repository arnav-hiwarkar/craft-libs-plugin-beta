# Capability coverage

Source of required questions is the question bank. A topic passes design review only after its independent Q, A1, B, A2 and C artifacts exist and the lead resolves the findings. All runtime conformance and performance gates remain unrun. The [final reconciliation](cross-topic-conflicts.md#final-reconciliation-2026-11-20) applied every final-review finding; every topic below is DESIGNED pending owner review, and none is IMPLEMENTED or TESTED.

| Order | Topic | Workshop state |
|---|---|---|
| 1 | UI-01 | DESIGNED. [ADR](decisions/ADR-UI-01.md), [surface contract](contracts/ui.md), [app slices](implementation/ui-01-slices.md), [acceptance](implementation/ui-01-acceptance.md) |
| 2 | UI-02 | DESIGNED. [ADR](decisions/ADR-UI-02.md), [content contract](contracts/ui-content.md), [acceptance](implementation/ui-02-acceptance.md) |
| 3 | UI-03 | DESIGNED together. [Decision](decisions/ADR-UI-03.md), [invocation contract](contracts/operations.md), [acceptance](implementation/edit-acceptance.md) |
| 4 | DOC-01 | DESIGNED together. [Decision](decisions/ADR-UI-03.md), [invocation contract](contracts/operations.md), [acceptance](implementation/edit-acceptance.md) |
| 5 | DOC-02 | DESIGNED together. [Decision](decisions/ADR-DOC-02.md), [document contract](contracts/document.md), [acceptance](implementation/document-acceptance.md) |
| 6 | RENDER-01 | DESIGNED together. [Decision](decisions/ADR-RENDER-01.md), [render contract](contracts/render.md), [acceptance](implementation/render-acceptance.md) |
| 7 | RENDER-02 | DESIGNED together. [Decision](decisions/ADR-RENDER-01.md), [render contract](contracts/render.md), [acceptance](implementation/render-acceptance.md) |
| 8 | RENDER-03 | DESIGNED together. [Decision](decisions/ADR-RENDER-03.md), [timed contract](contracts/time.md), [acceptance](implementation/time-acceptance.md) |
| 9 | RUN-02 | DESIGNED together. [Decision](decisions/ADR-RUN-01.md), [runtime contract](contracts/runtime.md), [acceptance](implementation/runtime-acceptance.md) |
| 10 | RUN-03 | DESIGNED together. [Decision](decisions/ADR-RUN-03.md), [SDK contract](contracts/sdk.md), [examples](contracts/sdk-examples.md), [acceptance](implementation/sdk-acceptance.md) |
| 11 | IO-01 | DESIGNED together. [Decision](decisions/ADR-IO-01.md), [file and job contract](contracts/io-jobs.md), [acceptance](implementation/io-acceptance.md) |
| 12 | RUN-01 | DESIGNED together. [Decision](decisions/ADR-RUN-01.md), [runtime contract](contracts/runtime.md), [acceptance](implementation/runtime-acceptance.md) |
| 13 | SYS-01 | DESIGNED together. [Decision](decisions/ADR-RUN-01.md), [runtime contract](contracts/runtime.md), [acceptance](implementation/runtime-acceptance.md) |
| 14 | SYS-02 | DESIGNED together. [Decision](decisions/ADR-RUN-01.md), [runtime contract](contracts/runtime.md), [acceptance](implementation/runtime-acceptance.md) |
| 15 | UI-04 | DESIGNED together. [Decision](decisions/ADR-UI-03.md), [invocation contract](contracts/operations.md), [acceptance](implementation/edit-acceptance.md) |
| 16 | DOC-03 | DESIGNED together. [Decision](decisions/ADR-DOC-02.md), [document contract](contracts/document.md), [acceptance](implementation/document-acceptance.md) |
| 17 | DOC-04 | DESIGNED together. [Decision](decisions/ADR-DOC-02.md), [document contract](contracts/document.md), [acceptance](implementation/document-acceptance.md) |
| 18 | RENDER-04 | DESIGNED together. [Decision](decisions/ADR-RENDER-03.md), [timed contract](contracts/time.md), [acceptance](implementation/time-acceptance.md) |
| 19 | RENDER-05 | DESIGNED together. [Decision](decisions/ADR-IO-01.md), [file and job contract](contracts/io-jobs.md), [acceptance](implementation/io-acceptance.md) |
| 20 | RUN-04 | DESIGNED together. [Decision](decisions/ADR-RUN-03.md), [SDK contract](contracts/sdk.md), [examples](contracts/sdk-examples.md), [acceptance](implementation/sdk-acceptance.md) |
| 21 | RUN-05 | DESIGNED together. [Decision](decisions/ADR-SDK-02.md), [conformance contract](contracts/conformance.md), [acceptance](implementation/conformance-acceptance.md), [phases](implementation/phases.md) |
| 22 | SDK-01 | DESIGNED together. [Decision](decisions/ADR-RUN-03.md), [SDK contract](contracts/sdk.md), [examples](contracts/sdk-examples.md), [acceptance](implementation/sdk-acceptance.md) |
| 23 | SDK-02 | DESIGNED together. [Decision](decisions/ADR-SDK-02.md), [conformance contract](contracts/conformance.md), [acceptance](implementation/conformance-acceptance.md), [phases](implementation/phases.md) |
