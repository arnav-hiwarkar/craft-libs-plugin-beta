# Conflicts and dependencies

When a later ADR changes an earlier contract, record both ADRs here and update the canonical contract and affected tests. Owner instructions are listed in [source baselines](source-baselines.md).

| Conflict | Resolution |
|---|---|
| Old beta implements or plans marketplace, remote catalogue and Adobe hosts | Current scope permits local developer loading and original functional equivalents. Retain useful fixtures and data contracts where applicable. |
| Old architecture describes networking, full-access native execution and embedded web engines | The current no-network rule applies to the whole execution path, including alternate frontends, file/media resolution, process access and app commands. Runtime authority must be proved per mode and target. |
| Old beta status reports PhotoCraft filters as destructive | Current PhotoCraft source includes smart filter registration and re-evaluation. Migration must cover both historical destructive results and current live dependencies. See PC-RENDER-1 in the inventory. |
| Framework main, EffectCraft's framework pin, PhotoCraft's framework pin, and beta's framework pin differ | Review each source under its actual revision. Phase the EffectCraft UI port on a common verified framework dependency and compile a contract before parallel app work. |
| Existing command tables cover operations beyond editing | Treat CLI/MCP as a parity inventory. Authority is applied to resolved operations and targets, including composite operations with side effects. Route continuous input and processing through their domain contracts. |
| Original owner demands complete capability without deferral, while design kit orders dependency phases | Each capability needs a design and test obligation. A dependency edge names the later contract and implementation gate. It cannot close a question by itself. |

## Dependencies from UI decisions

| Origin | Contract owner and obligation | Proof |
|---|---|---|
| UI-01 captures a document and base revision from a live mount | DOC-01 defines target-aware prepare and publication, history, preview-generation lease, grant and ticket validation at commit. PC's Action and relative path contexts come from the target. | `UI01-edit`, `UI01-preview` |
| UI-01 claims input before built-in or guest dispatch | UI-03 specifies claim ledger, host tool routing, viewport transforms, capture and ordered sample delivery. UI-04 maps honest host shortcuts and modal priority into the same admission. | `UI01-pass`, `UI01-modal`, `UI01-drop` |
| UI-01 declares inspector and replacement roles | UI-04 defines stable command and slot enumeration, ordering and opt-in restoration. Protected UI remains host-owned. | `UI01-trust`, REF-12 |
| UI-01 saves plugin panel and workspace identity | SYS-02 and RUN-03 define package, contribution, scope, incarnation and alias rules. DOC-03 defines opaque plugin state, format versions and lossless treatment. | `UI01-workspace`, `UI01-save` |
| UI-02 presents a leased image and synchronizes invalidation | RENDER-02 supplies format, alpha/color, device generation and fence semantics. Revocation and provider loss withdraw old pixels and accessible state. | `UI02-device`, `UI02-update` |
| UI-02 offers packaged HTML as one compatible leaf | RUN-02/04 own browser isolation and per-backend advertisement. Subresource/file brokers are local only. Denial uses a task-equivalent body. | `UI02-html` |
| UI-02 exposes optional versioned controls and semantic lookup | RUN-03 owns version negotiation and old guest conformance. UI-03 uses the same authority barrier for offscreen reveal and eventual actions. | `UI02-invalidation`, `UI02-collections` |

## Dependencies from input-to-edit

| Origin | Contract owner and obligation | Proof |
|---|---|---|
| DOC-01 prepares engine mutations and explicit side effects | DOC-02/03/04 state/asset/property edits stage under one engine publication. Reject ambient Action or editor-state swaps. | `DOC-object`, `DOC-properties`, `EDIT-target` |
| UI-03 keeps speculative tiles out of committed output | RENDER-01/02 unify lease admission and derived preview generation. Host commit and cancel invalidate late preview outputs. | `EDIT-overlay`, `UI01-preview` |
| UI-04 resolves macro, hook and fallback leaves at sinks | RUN-02 classifies each effective scope and preserves host-issued provenance through deferred effects. IO-01 owns destination checks and durable file publication. | `EDIT-hook`, REF-11, REF-15 |
| DOC-01 projects typed immutable snapshots and versioned ops | RUN-03 defines schema/binary evolution; app command renames preserve authored IDs. RUN-05 measures actual visited and evaluated work. | `EDIT-bulk`, old-guest conformance |

## Persistence and rendering checks queued from source

The following are open until the topic ADRs close them.

- Old file versions can lose unknown data through normal save. The version guard must cover conventional saves, recovery, export copies, named data changes and native folder/ZIP variants. The same payload must reach native open after a failed migration.
- Native formats can retain both authored state and an old rendered surface. Carry render completeness through preview, caches, native save composites and final export. A cached skipped filter remains an incomplete stage.
- Multiple engines independently remap IDs in copy, project import and load. Ensure typed referenced operands and asset closures travel through all of those routes. Locate declarations and payload links without asking the missing plugin to execute.

## Dependencies from persistence

| Origin | Contract owner and obligation | Proof |
|---|---|---|
| DOC-02 exposes first-class layer, geometry and per-app time roles | RENDER-01/02 map source and augmentation stages plus geometry through real compose and export. RENDER-03/04 own typed time/track/audio availability. | `DOC-object`, `RND-scene-3d`, REF-02, REF-07 |
| DOC-03 saves opaque authored state and a complete asset closure | IO-01 defines local handle authority, streamed publication and crash/destination semantics. RUN-02 gates migration and link reads. | `DOC-assets`, `DOC-roundtrip` |
| DOC-03 requires trusted exact bake evidence | RENDER-01/02 mark propagated completeness in graph outputs, stage caches and final encoders. RUN-03 defines verifiable portable provider/version identities. | `DOC-missing`, `RND-cache` |
| DOC-04 uses reference operands and recurrence within a valid time horizon | RENDER-03 proves strict progress toward the base, bounded expanded graph work, exact scheduling-independent results and interval invalidation. | `DOC-invalidate`, REF-04 |

## Processor dependencies

| Origin | Contract owner and obligation | Proof |
|---|---|---|
| RENDER-01/02 accepts actual frame/plane coverage | RENDER-03/04 pins source and comp time, shutter/dependency samples, clock and audio ranges on the same invocation. | `TIME-map`, `TIME-neighbors`, `AUD-format` |
| RENDER-01/02 stages complete exact output | IO-01 owns destination authority, single-file replacement and sequence receipts. Completion before publication does not imply a filesystem group transaction. | `RND-cache`, `AUD-exact`, REF-11 |
| RENDER-02 uses a confined service and optional GPU interop | RUN-02/04 prove process/network effect boundary and device support. A driver reset may share fault across devices. Historical wording; the GPU provider is named in the final reconciliation (M3). | `RND-gpu`, `UI02-device` |
| RENDER-01 uses verified exact profiles and bake provenance | RUN-03 defines stable schemas/digests. RUN-05 measures actual comparable pixel, time and format work. | `RND-format`, `RND-cost`, `DOC-missing` |

## Dependencies from files, formats and jobs

| Origin | Contract owner and obligation | Proof |
|---|---|---|
| IO-01 runs plugin-triggered decode, job and inference work in confined workers | RUN-01/02 provide a job-scoped worker kind and codec worker with no sockets or exec. Historical; superseded by ADR-RUN-01 below and by HostNative in the final reconciliation (H6). Under EffectCraft's Flatpak manifest (`--share=network`, host spawn) workers need a no-network sub-sandbox without host-spawn D-Bus. | `IO-parser-confine`, `JOB-infer`, RUN-02 denial matrix |
| IO-01 adopts one publication sink for app saves and plugin output | Supersedes RENDER-01's "IO-01 publication sink" placeholder with `craft-files`. PC Save and EC export cut over first, with typed durability. | `IO-crash`, `RND-cache` |
| RENDER-05 commits by read-set fingerprints | DOC-01 publication accepts `Stale(ReadKey[])` as a terminal result. Adapters expose O(1) tokens (PC tile digests, EC `frames_key`). | `JOB-aba`, `JOB-lost-update`, `EDIT-target` |
| RENDER-05 cache slots feed render | DOC-03 bake rule decides whether a reproducible slot may certify exact export. RENDER-01 cache keys include slot provenance. | `JOB-cache-key`, `DOC-missing` |
| RENDER-05 GPU inference shares the provider device | RENDER-02 provider admits inference with adapter budget, chunked submission and quarantine. RUN-04 advertises per-backend operator coverage. | `JOB-gpu-fair`, `JOB-gpu-tdr` |
| IO-01 format contributions appear in native dialogs | UI-04 command and slot enumeration places Open, Save As, Export and Render Queue entries. SDK-01 needs a 20–50 line importer example. | `IO-format-open-save` |

## Supersessions and dependencies from runtime and authority

| Origin | Contract owner and obligation | Proof |
|---|---|---|
| ADR-RUN-01 supersedes IO-01's job-scoped worker process | Plugin jobs and plugin readers run as `job` Stores in the principal process. A job kill drops its Store. Native app decoders keep a separate confined codec process, renamed HostNative in the final reconciliation (H6). `io-jobs.md` is updated. | `RUN-bomb`, `JOB-cancel-race`, `IO-parser-confine` |
| ADR-RUN-01 replaces IO-01's Flatpak `flatpak-spawn` sub-sandbox requirement | Our launcher applies seccomp and Landlock inside the Flatpak sandbox; `flatpak-spawn` is optional. Landlock under Flatpak's seccomp is an open verification in `NET-matrix`. | `NET-matrix` (Flatpak) |
| ADR-RUN-01 replaces the earlier separate activation and grant generations | `PrincipalEpoch` is the generation checked by UI-01 tickets, DOC-01 publication, RENDER leases and IO-01 grants. The full generation set is in the final reconciliation (H1). | `T-revoke-commit`, `EDIT-target` |
| ADR-RUN-01 sets HTML panels on a confined CEF component | UI-02's packaged HTML leaf is `Unavailable` until `NET-cef-webrtc` and `CEF-cost` pass, and its native fallback body applies. The web target never offers HTML panels. | `UI02-html` |
| ADR-RUN-01 gives each principal a logical GPU device in the shared provider | RENDER-02 and IO-01 GPU inference admit per principal device. The software adapter is advertised only where an ICD ships. | `GPU-xprincipal`, `JOB-gpu-fair` |
| ADR-RUN-01 provenance on persisted artifacts | DOC-03 persists `Chain` provenance with actions, presets, filter stacks, locators, expressions and bindings. Paste and import remaps carry it. UI-04 bindings carry provenance. | `RUN-sink-launder`, `RUN-paste-prov`, `DOC-roundtrip` |
| ADR-RUN-01 one CPU credit ledger | RENDER-01 schedulers, IO-01 jobs and app-native rayon work draw from it. RUN-05 measures oversubscription. | `RUN-oversub`, `JOB-fair` |

## Dependencies from SDK and evolution

| Origin | Contract owner and obligation | Proof |
|---|---|---|
| ADR-RUN-03 extends IO-01 read-set fingerprints to every guest commit | DOC-01 publication checks the read set recorded under the ticket for UI, tool, script and job commits alike. Operations are key-addressed patches. | `EDIT-lost-update`, `JOB-aba` |
| ADR-RUN-03 adds `ExecProfile` with `DeterminismClass` | RENDER-01 session and bake keys and DOC-03 bake identity include it. Caches never cross classes. Exact fork-join partitions depend only on the request. | `WASM-profile`, `RND-cache`, `DOC-missing` |
| ADR-RUN-03 rejects a separate script Store | `runtime.md` Store kinds are unchanged. `.jsx` runs as an asyncify build in an ordinary `ui` or `job` Store, and each wait is a turn boundary. | `SDK-script-sync`, `SCRIPT-reentry` |
| ADR-RUN-03 keeps the compile run guest-free | Manifest extraction and TS pre-init run in the Principal process (historically "principal worker"). The snapshot is a memory image on one shared `quickjs.wasm`. | `RUN-cache-isolation` |
| ADR-RUN-03 dev re-admit keeps `PrincipalEpoch` for user-linked folders with unchanged grants | Amends the runtime contract's "update bumps the epoch" for developer links only. | `DEV-loop`, `RUN-identity` |
| ADR-RUN-03 web runtime | The web Wasm row is P with a broker Worker. PC keeps in-page wasmi until `WEB-broker` passes. No SharedArrayBuffer is promised. | `WEB-broker` |
| ADR-RUN-03 `engine.lock` and `craft-idmap` | DOC-03 persisted invocations and UI-04 bindings, keymaps and toolbars resolve through `craft-idmap`. Phase 0 lands before any engine-id rename. | `IDMAP-props`, `SDK-save-refactor-reopen` |
| ADR-RUN-03 typed handlers gate guest access | UI-04 and DOC-01 expose only typed Domain and App leaves. SDK-02 requires 100% typed leaves for parity. | Ratchet, SDK-02 parity |

## Supersessions from conformance and parity

| Origin | Contract owner and obligation | Proof |
|---|---|---|
| ADR-SDK-02 replaces ADR-RUN-01 FIRST-PARTY (P2) and its "≥ 0.6× native" gate | `PAR-*` gates with the Kernel `DEFECT` rule. `RUN-first-party` is an alias. Phase numbering is mapped in [phases](implementation/phases.md#numbering-map). | `PAR-pc-stylize`, `PAR-ec-echo` |
| ADR-SDK-02 consolidates budgets | The [tolerance table](contracts/conformance.md#tolerances) supersedes overlapping numbers in `UI01-frame-cost`, `UI02-cost`, `RUN-oversub`, `JOB-fair`, `RND-cost` and the IO-01 and RUN-01 hypotheses. | `CONF-*` |
| ADR-SDK-02 adds three amendments for phase 6 | RENDER-01 and UI-02: VIEW eyedropper parameter annotations. RENDER-02: TEXT-01 brokered text service. RENDER-03: an `AudioWindow` input kind with an invalidation edge. Each needs two REF users before a new standard node. | `VIEW-pick`, `TEXT-shape`, `AUD-port` |
| ADR-SDK-02 moves canonical design docs into CL in phase 0a | The coverage lint runs there. This workspace remains the design history. | `COV-matrix` |
| ADR-SDK-02 merges duplicate test IDs | [Registry aliases](implementation/conformance-acceptance.md#registry-and-aliases) apply across every acceptance file. | `COV-matrix` |

## Final reconciliation (2026-11-20)

The lead accepted all 24 findings of the final review. Each row supersedes the earlier ledger rows and contract text it names. Section-level detail is in the change log. Everything remains DESIGNED; nothing is IMPLEMENTED or TESTED.

| Finding | Supersession or correction | Files changed |
|---|---|---|
| H1 | A Generations table defines `PrincipalEpoch`, `ProcessEpoch` (renames WorkerEpoch), `MountEpoch`, `InteractionEpoch`, `DeviceGeneration`, and `ReleaseDigest` as provider generation. `Grant = (Principal, Right, Scope, IssuedTo: ContributionKey)`. An update without grant expansion re-issues Session/Persisted grants under the new epoch; Invocation/Document grants die. Supersedes "update keeps grants" and every "grant generation", "WorkerEpoch" and "provider generation". | contracts/runtime.md, ui.md, io-jobs.md, render.md, document.md, ui-content.md; decisions/ADR-RUN-01.md; implementation/runtime-acceptance.md |
| H2 | Publication uses the read-set rule (ReadKey → O(1) token over captured targets, query leases, render reads and write targets); mismatch returns `Stale(ReadKey[])` with no Undo; `DocRevision` scopes previews and receipts only. `CommitOutcome` adds `Stale`. Supersedes "recheck document revision/dependencies". | contracts/operations.md, ui.md |
| H3 | `ExtensionRecord` carries `Provenance(Chain)`; `TypeKey = (PackageId, LocalTypeId)` through `craft-idmap`; the record lives in CL `plugin-api::doc`, replacing `ext-core::project`. `LineageKey` is host-local and never persisted; records bind by `PackageId + TypeKey + SchemaVersion`, and adoption governs only data folders and grants. One `BakeKey` in the document contract; render and SDK reference it. | contracts/document.md, runtime.md, render.md, sdk.md; decisions/ADR-DOC-02.md, ADR-RUN-01.md |
| H4 | The three ADR-SDK-02 amendments are in the contracts: `ParamAnnotation::Eyedropper`, `AudioWindow` with an invalidation edge, and §Text (TEXT-01). Font install or download, including `craft-font-store` and OS on-demand fonts, is a `Net` sink never admitted under a plugin chain. | contracts/ui-content.md, render.md, time.md, runtime.md; architecture/threat-model.md |
| H5 | `render.read(ticket\|job, Target, TimePoint, Region, ImageDesc, Purpose) -> Lease + ReadKey` replaces "render ports"; `ReadKey` is the app dependency key. | contracts/render.md, io-jobs.md; decisions/ADR-IO-01.md; implementation/io-acceptance.md |
| H6 | The Codec process is renamed HostNative (confined). It runs native decoders and `tract`, is the only model parser, and returns results to `job` Stores through rings. Supersedes the IO-01 "job worker" and "codec worker" sentences and the earlier "native app decoders keep a separate confined codec process" row. | contracts/runtime.md, io-jobs.md; decisions/ADR-IO-01.md, ADR-RUN-01.md; implementation/io-acceptance.md, phases.md |
| H7 | Web GPU processors are `P (WebGPU in the broker Worker)`, with test `WEB-gpu`. | contracts/sdk.md, runtime.md; implementation/sdk-acceptance.md, phases.md |
| H8 | Slice 4a0 (DOC-RECORD plus the PC Smart Filter attachment, gated by `DOC-roundtrip`) precedes 4a; `PAR-pc-stylize` and the legacy shims depend on it. The phase 3 Liquify spike covers UI body and tool capture only; full `PAR-pc-liquify` stays in phase 6. Phase 1 reads "do not port; the beta remains historical". Slices branch into arnav-hiwarkar/* forks or Forgejo, never a storytold remote; builds run only in approved Actions jobs. | implementation/phases.md, dependencies.md, ui-01-slices.md, conformance-acceptance.md; decisions/ADR-DOC-02.md, ADR-SDK-02.md |
| M1 | `craft-files` absorbs and replaces CL `craft-atomic`; its callers migrate in FILES-SINK; a directory-sync failure is never ignored. | contracts/io-jobs.md; decisions/ADR-IO-01.md |
| M2 | One `Receipt` type: document writers call `craft-files` and return it; `PublishSave` is removed. The web reports `VisibleUnconfirmed(Browser)`, replacing `Delivered(Unverified)`. | contracts/document.md, io-jobs.md; implementation/io-acceptance.md |
| M3 | Process glossary: Principal process, HostNative, HTML, GPU provider, NativeConfined. The GPU uses a per-principal logical device in the shared GPU provider, an accepted residual risk proved by `GPU-xprincipal`. The counter is "zero AppDevice cross-device bytes". | contracts/runtime.md, render.md, ui-content.md, io-jobs.md, sdk.md, conformance.md; architecture/system.md, threat-model.md; decisions/ADR-RENDER-01.md, ADR-RENDER-03.md, ADR-RUN-03.md, ADR-SDK-02.md; implementation/conformance-acceptance.md |
| M4 | Warm-state rule: state crosses requests only as host-held checkpoints keyed by `(instance, RecurrenceGroup \| MixEpoch)`; one warm Store per lane. Hypothesis: 48 kHz/128-frame Store reuse plus checkpoint ≤ 10% of the block budget. | contracts/runtime.md, time.md; decisions/ADR-RUN-01.md; implementation/time-acceptance.md |
| M5 | `doc.list(ticket)` and `doc.ticket(DocRef, reason)` target other open documents; REF-14 gains a two-open-document case; test `DOC-multi`. | contracts/operations.md, ui.md; implementation/proof-inventory.md, edit-acceptance.md |
| M6 | Host resource libraries are typed Domain/App operations with provenance (REF-21, `RES-library`). Brokered `input.device(grant)` (HID/MIDI; OSC excluded as network) and `capture.open(grant)` (camera, screen, microphone) from a host picker, phase 6 (`INPUT-device`, `CAPTURE-open`). `FormatContribution.options: PropsSchema` with presets in the principal data folder (`FORMAT-options`). | contracts/operations.md, io-jobs.md, runtime.md; decisions/ADR-IO-01.md; implementation/proof-inventory.md, edit-acceptance.md, io-acceptance.md, phases.md |
| M7 | A registered `.jsx` runs as its generated package's command contribution, a `Command` sink under chain intersection; package-to-package command calls use the same gate; `Spawn(Script)` covers only OS interpreters. `DocRead`, `DocEdit` and `Editor` join the `Sink` enum. | contracts/runtime.md, sdk.md; implementation/sdk-acceptance.md |
| M8 | Rows defined for `RET-stylize`, `SCRIPT-ec-suite`, `WASM-winarm64`, the eight FAULT IDs, and `RUN-cost`, `RUN-cost-js` and `RUN-service` as aliases. UI02 slices are upper case. Seven aliased duplicates reduce to "Alias of X; budget per conformance tolerance table". Every referenced test ID has one defining row. | implementation/conformance-acceptance.md, sdk-acceptance.md, runtime-acceptance.md, io-acceptance.md, edit-acceptance.md, ui-01-acceptance.md, ui-02-acceptance.md, document-acceptance.md, phases.md; decisions/ADR-UI-02.md; architecture/compatibility.md |
| L1 | The audio callback gate is p99 ≤ 25% of the period (conformance); 10% stays the ADR-RENDER-03 target. | contracts/conformance.md; decisions/ADR-RENDER-03.md |
| L2 | `cx.ui.prompt`, `cx.overlay.publish`, one-shot `cx.query` and `cx.commit` are SDK sugar; error codes keep enum casing (`"Stale"`). | contracts/sdk.md, sdk-examples.md |
| L3 | `Expired` joins `OpError`; `UiOutcome::Stale` is renamed `BodyStale`. | contracts/sdk.md, ui.md, operations.md |
| L4 | CEF comes from a bundled or picked local archive, never downloaded; its install is `Setting(SecurityRelevant)`; the inner sandbox is off, recorded as residual risk. | contracts/runtime.md; architecture/threat-model.md; implementation/phases.md |
| L5 | User-picked remote volumes, explicit cloud hydration and printing are user-directed exceptions a plugin never initiates. | architecture/owner-invariants.md |
| L6 | `Canvas`, image assets and swatches declare a `ColorSpace`. | contracts/ui-content.md |
| L7 | One `CacheKey` for session caches, memos and `cache.put`; one `ExecProfile` replaces `ResultProfile`, the recurrence `Profile`, the memo profile and "precision profile". | contracts/render.md, io-jobs.md, time.md, sdk.md, document.md |
| L8 | Stale status text updated to DESIGNED pending owner review. | README.md; architecture/system.md, threat-model.md, compatibility.md; decisions/ADR-IO-01.md; implementation/dependencies.md; capability-coverage.md |
