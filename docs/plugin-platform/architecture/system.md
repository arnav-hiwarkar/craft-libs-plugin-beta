# Platform ownership and integration summary

All 23 topics are DESIGNED, pending owner review, after the [final reconciliation](../cross-topic-conflicts.md#final-reconciliation-2026-11-20) of the final review. Nothing is IMPLEMENTED or TESTED: no slice has landed and no acceptance case has run. Generation and process names follow the [runtime contract](../contracts/runtime.md#generations).

```mermaid
flowchart LR
    Guest[Guest Wasm in a Principal process] <-->|craft@2 envelopes, rings and leases| Host[craft-plugin-host broker]
    Loader[Local package loader] --> Host
    Host --> Gate[gate.admit: Chain, Sink, Target]
    Gate --> Adapter[App adapter]
    Adapter --> Editor[App engine: edits, history, documents]
    Adapter --> Render[App render, time and job schedulers]
    Adapter --> Craft[Craft workspace and input]
    Craft -->|Claimed input and context tickets| Host
    Render --> GPU[GPU provider: per-principal logical devices]
    Host --> HostNative[HostNative: native decoders and tract]
    Adapter --> Files[craft-files: stage, publish, Receipt]
```

## Ownership

| Owner | Responsibilities | Contracts |
|---|---|---|
| CL `craft-plugin-host`, `craft-plugin-ipc`, `craft-plugin-confine` | Package admission by `ReleaseDigest`, principals and generations, grants, `Chain` and `gate.admit`, broker thread, rings, WorkMeter, credit ledger, validator and mappers, capabilities | [Runtime](../contracts/runtime.md), [SDK](../contracts/sdk.md) |
| CL `plugin-api` (incl. `plugin-api::doc`), `plugin-sdk`, `sdk-gen`, `craft-idmap` | Domain types, `ExtensionRecord`, `TypeKey`, `BakeKey`, error registry, locks, generated SDKs and fake host, alias resolution | [SDK](../contracts/sdk.md), [Document](../contracts/document.md) |
| CL `craft-files` (absorbs `craft-atomic`) | Grants beneath OS handles, staging, exchange or replace, receipt journal, the one `Receipt` type | [Files and jobs](../contracts/io-jobs.md) |
| CL `plugin-conformance`, `testkit::pair` | Test registry, coverage lint, paired verdicts | [Conformance](../contracts/conformance.md) |
| Craft framework | Window and dock, gestures, controls, IME, focus, accessibility, theme, modal layering | [Workspace](../contracts/ui.md), [UI content](../contracts/ui-content.md) |
| Each app adapter | Typed handler binding, target resolution, O(1) read-set tokens, `render.read` keys, snapshot export, availability reasons | [Invocation](../contracts/operations.md), [Render](../contracts/render.md), [Time](../contracts/time.md) |
| App engines | Edit, history and publication, document persistence, preview and invalidation, compositor, exact export | [Invocation](../contracts/operations.md), [Document](../contracts/document.md) |
| Principal process | One per active principal: `ui`, `service`, `render` and `job` Stores; no network imports | [Runtime](../contracts/runtime.md#processes-and-turns) |
| HostNative (confined) | Native app decoders and `tract` inference for plugin-triggered work; the only model parser | [Runtime](../contracts/runtime.md#processes-and-turns), [Files and jobs](../contracts/io-jobs.md#inference) |
| GPU provider | One per app per device, a per-principal logical device for each principal; an accepted residual risk | [Render](../contracts/render.md#providers-and-leases), [Threat model](threat-model.md#accepted-residual-risks) |
| HTML, NativeConfined | Optional CEF tree per principal; native SDK shim per OS; both `Unavailable` until their probes pass | [Runtime](../contracts/runtime.md#network-denial) |
| Guest | Owned state and algorithms, versioned contributions, optional fallbacks, requests under its grants | [SDK](../contracts/sdk.md), [Examples](../contracts/sdk-examples.md) |

First-party features ship as packages through the same public contracts. The app opens, edits and saves without optional guest code, and a document reference never activates code.

## Four normal flows

1. **Open a panel.** The manifest declares a contribution with a stable `ContributionKey`. The user opens it through the normal Window command; the app applies an ordinary dock action and issues a `MountRef` bound to the `ProcessEpoch` and `MountEpoch`. The guest publishes a validated body with `ui.publish`. The originating viewport claims input once, and the UI owner barrier resolves modals before a `UiAction` reaches the guest with a context ticket. [ADR-UI-01](../decisions/ADR-UI-01.md), [ADR-UI-02](../decisions/ADR-UI-02.md)
2. **Modify the document.** A context ticket captures the document incarnation, targets, `PrincipalEpoch` and a read set (ReadKey → O(1) token) of captured targets, query leases, render reads and write targets. The guest commits key-addressed patches with `edit.commit`. At the owner publication point the host compares tokens only and rechecks grants; a mismatch returns `Stale(ReadKey[])` with no Undo, and success publishes one Undo, receipt and outbox record. `doc.ticket` targets another open document. [ADR-UI-03](../decisions/ADR-UI-03.md), [ADR-RUN-03](../decisions/ADR-RUN-03.md)
3. **Render or export.** The app's render owner sends a `ProcessRequest` with an `ExecProfile`; the guest returns a plan, and `render.read` serves guest-initiated reads under the same footprint and authority rules. CPU work runs in `render` Stores, and GPU work on the guest's logical device in the GPU provider. One `CacheKey` covers session caches, memos and `cache.put`. Exact export needs complete coverage or a verified `BakeKey` match, and publishes through `craft-files` with a `Receipt`. [ADR-RENDER-01](../decisions/ADR-RENDER-01.md), [ADR-RENDER-03](../decisions/ADR-RENDER-03.md), [ADR-DOC-02](../decisions/ADR-DOC-02.md)
4. **Import and publish.** A host picker or sheet issues a `Grant` through `fs.request`. A plugin reader runs in a `job` Store, and native decoders run in HostNative; the host builds app objects through one ordinary edit. Jobs record read sets and commit or return `Stale`. Output is staged and published by `craft-files`, which returns a typed `Receipt` per file (`VisibleUnconfirmed(Browser)` on the web). [ADR-IO-01](../decisions/ADR-IO-01.md)

Every effect passes `gate.admit(Chain, Sink, Target)`, and replayed artifacts carry their provenance `Chain`. [ADR-RUN-01](../decisions/ADR-RUN-01.md), [ADR-SDK-02](../decisions/ADR-SDK-02.md)

## Decisions and contracts

| Decision | Topics | Contracts |
|---|---|---|
| [ADR-UI-01](../decisions/ADR-UI-01.md) | UI-01 | [ui.md](../contracts/ui.md) |
| [ADR-UI-02](../decisions/ADR-UI-02.md) | UI-02 | [ui-content.md](../contracts/ui-content.md) |
| [ADR-UI-03](../decisions/ADR-UI-03.md) | UI-03, UI-04, DOC-01 | [operations.md](../contracts/operations.md) |
| [ADR-DOC-02](../decisions/ADR-DOC-02.md) | DOC-02, DOC-03, DOC-04 | [document.md](../contracts/document.md) |
| [ADR-RENDER-01](../decisions/ADR-RENDER-01.md) | RENDER-01, RENDER-02 | [render.md](../contracts/render.md) |
| [ADR-RENDER-03](../decisions/ADR-RENDER-03.md) | RENDER-03, RENDER-04 | [time.md](../contracts/time.md) |
| [ADR-IO-01](../decisions/ADR-IO-01.md) | IO-01, RENDER-05 | [io-jobs.md](../contracts/io-jobs.md) |
| [ADR-RUN-01](../decisions/ADR-RUN-01.md) | SYS-01, SYS-02, RUN-01, RUN-02 | [runtime.md](../contracts/runtime.md) |
| [ADR-RUN-03](../decisions/ADR-RUN-03.md) | RUN-03, RUN-04, SDK-01 | [sdk.md](../contracts/sdk.md), [sdk-examples.md](../contracts/sdk-examples.md) |
| [ADR-SDK-02](../decisions/ADR-SDK-02.md) | RUN-05, SDK-02 | [conformance.md](../contracts/conformance.md) |

## Status

| Topics | State |
|---|---|
| UI-01, UI-02, UI-03, UI-04, DOC-01, DOC-02, DOC-03, DOC-04, RENDER-01, RENDER-02, RENDER-03, RENDER-04, RENDER-05, IO-01, SYS-01, SYS-02, RUN-01, RUN-02, RUN-03, RUN-04, RUN-05, SDK-01, SDK-02 | DESIGNED, pending owner review. None IMPLEMENTED or TESTED. |

Implementation follows the [phase plan](../implementation/phases.md) and [dependency graph](../implementation/dependencies.md) once the owner approves. Per-topic closure is in [capability coverage](../capability-coverage.md) and the [question register](../question-closure.md); supersessions are in the [conflict ledger](../cross-topic-conflicts.md).
