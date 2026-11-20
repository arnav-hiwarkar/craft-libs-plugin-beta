# SYS-01/02 and RUN-01/02 · Platform core, identity, execution and authority

Accepted with verification. The [runtime and authority contract](../contracts/runtime.md) is designed. Each execution mode stays `Unavailable` until its network-denial and isolation probes pass on the target. None of them has run.

## Decision

**Lean core.** The host owns the parts that make plugins fast and safe:
- documents, history and publication (app engines);
- render and job scheduling, through one CPU credit ledger shared with app-native work;
- workspace, input and accessibility (the craft framework);
- files and jobs (IO-01);
- the plugin host itself.

Everything above that, first-party features included, ships as packages through the same public contracts. A missing or disabled package leaves inert, attributed state, and the app still starts, opens and saves.

New contribution classes need a host contract. New types, instances, tools, effects, formats, panels and services within those classes need no app rebuild.

**Identity.**
- A release is a content digest of the canonical package tree in a host-owned read-only store.
- A principal is `(PackageId, LineageKey)`, with one enabled lineage per package id. `LineageKey` is host-local and never persisted. Document records bind by `PackageId + TypeKey + SchemaVersion`, so projects stay live across machines. Moving to a new lineage needs an explicit "adopt state and data" sheet, which governs only data folders and grants.
- An incarnation is `(Principal, ReleaseDigest, PrincipalEpoch)`.
- One `PrincipalEpoch` replaces separate activation and grant generations, because every revoke kills the principal's processes. The [generations table](../contracts/runtime.md#generations) names the rest: `ProcessEpoch`, `MountEpoch`, `InteractionEpoch`, `DeviceGeneration`, and `ReleaseDigest` as provider generation.
- Grants attach to the principal, as `(Principal, Right, Scope, IssuedTo: ContributionKey)` where `IssuedTo` is attribution only. An update without grant expansion re-issues Session and Persisted grants under the new epoch; Invocation and Document grants die. Code sharing a process can impersonate any contribution of its package. Narrower authority comes from the request-bound handles the host passes in.

**Execution.** All guest code is Wasm, JavaScript included: QuickJS-ng is compiled to Wasm and runs in the same Wasmtime engine. Each active principal gets one confined OS process holding three kinds of Wasm instance (Store):
- a `ui` Store for panels, hooks, tools and commands;
- warm `service` Stores, partitioned by the head of the calling chain;
- `render` and `job` Stores, created fresh per request from a pre-instantiated module, scaled by host credits, with host fork-join for full-frame stages. State crosses requests only as host-held checkpoints, and the host may keep one warm Store per lane.

Separate OS processes exist only where the code is not guest Wasm ([process glossary](../contracts/runtime.md#processes-and-turns)):
- native (NativeConfined) code;
- HostNative (confined): native app decoders and `tract` inference triggered by plugins, and the only place models are parsed;
- one confined CEF tree per principal for HTML panels;
- the GPU provider, which gives each principal its own logical device. It is the one process shared across principals, recorded as an accepted residual risk in the [threat model](../architecture/threat-model.md).

Wasm workers are offline by construction because they get no network imports. The OS layer is defence in depth for Wasm and the only boundary for native, HTML and GPU code.

**Authority.** Every caller builds one serialisable `Chain`: UI, CLI, MCP/agent, scripts, headless, plugins, deferred jobs and replayed artifacts. Each effect passes `gate.admit(chain, Sink, target)`. The result is the intersection of every principal in the chain, and local user input is just a principal with full rights. Sinks take an `Admitted<K>` token that has a private constructor, so no codepath can skip the gate.

`Chain` persists as provenance on everything the host later executes or dereferences:
- actions and steps, presets and filter stacks;
- linked locators, script references and expressions;
- shortcut, menu, toolbar and startup bindings.

Security-relevant settings are never admitted under a plugin chain. Every persisted locator is dereferenced through the IO-01 resolver, so a network path needs a user relink whatever its provenance says.

### Alternatives

| Alternative | Strongest argument | Why it lost |
|---|---|---|
| In-process Wasm in the app (today's wasmi in both apps) | No IPC, smallest latency | Engine bugs or Spectre gadgets would expose the document and the networked app process. Wall-clock stalls land on the UI thread. Per-principal OS memory limits and kill are impossible. |
| One shared kernel process for all principals (B) | Fewer processes, warm pool | An engine escape would cross principals' document grants. It also blurs strike attribution and memory accounting. Per-principal processes add about one spawn per session and are reused from a pre-sandboxed spare. |
| Up to four processes per package by role (A1) | Hook stalls isolated from commands | Process and spawn cost without extra safety. Store recycling with CPU-time turn caps isolates turns inside one process. |
| System webview for HTML | Small, auto-updated | WKWebView and WebView2 helpers are not our descendants, so offline confinement cannot be proven. CEF ships as an optional component installed from host Settings only. The native UI-02 fallback applies until its probes pass. |
| V8 for JS | JIT speed | It adds a second engine, limiter and attack surface. QuickJS-Wasm must meet the fixed `RUN-cost-js` fixture. V8 returns only as a separately confined mode if the fixture fails. |

## Amendments applied from C

All ten are in the contract and tests:
1. The locator resolver and journal-digest provenance.
2. A `Setting(SecurityRelevant)` sink and provenance on bindings.
3. Replay scope by sink class, an adopt sheet, and provenance that survives paste and import.
4. One shared CPU credit ledger with a UI reserve and lower QoS.
5. CPU-time turn caps, hook deadlines measured from turn start, a narrowed strike rule, negative caching, and host-held `cx.state`.
6. Warm service Stores partitioned by chain head, all-or-nothing credits, and no hold-and-wait.
7. A per-principal AppContainer SID, signal confinement, and a per-principal GPU logical device.
8. Honest advertisement: software GPU only where an ICD ships, the CEF deny list and Settings-only install, the name-lookup deputies removed, and workers killed on a firewall change.
9. Type-driven migration: the PC `authorize`, `automation_gate` and `automation_input` fields and gate thread-locals are deleted in the engine and ui-egui, plus a clippy allowlist diff, the classification and policy-diff tests, copy-before-validate, one crash/revoke commit rule, and fork-join.
10. Merges: `PrincipalEpoch`, one `Chain`, ring-only bulk input, and no xattr.

The lead also applies C's recommended merge. Plugin Wasm jobs and decoders run as Stores inside the principal process, which **supersedes** the IO-01 sentence on job-scoped workers ([ledger](../cross-topic-conflicts.md)). A job kill drops its Store, and other turns survive. HostNative processes remain for native app decoders and CPU inference that plugins trigger.

## Integration slices

| Slice | Owner and codepaths | Exit proof |
|---|---|---|
| AUTH-GATE (P0, no plugin code) | PC: delete `Session.authorize` and `automation_gate` (`engine/src/lib.rs:282–292`), the job thread-locals (`jobs.rs:45–55`) and `automation_input` in ui-egui. Admit in `Session::dispatch` and at the side sinks and trust flags named in A2 §7 (review record kept outside this repository). Replace the Action boolean (`actions_cmds.rs:43–76`) with provenance. EC: admit in `Session::execute` before `enabled` (`session/dispatch.rs:9–21`). Script `Socket`, model `curl` and prefs become sinks. | `RUN-lint`, `RUN-classify-all`, `RUN-policy-diff`, `RUN-sink`, `RUN-sink-launder`, `RUN-sink-binding`, `RUN-replay-scope` |
| BROKER | Broker thread with `Arc<Snapshot>` published per revision. Coalesced owner publication calls `request_repaint`. Replaces the BETA frame pump (`runtime/mod.rs:272–295`). | `RUN-idle-call`, `T-hol` |
| RUNTIME (P1) | `craft-plugin-host` from BETA `ext-runtime`, `craft-plugin-ipc` from `ext-ipc`, `craft-plugin-confine` launcher and probes, signed `craft-worker`. Delete BETA `NativeFull`, `Network(host)`, the class gate, the WK filter and the guest-writable compile cache. | `RUN-identity`, `RUN-lease-ro`, `RUN-bomb`, `T-cpu-vs-wall`, `NET-matrix` (Wasm), `NET-xprincipal`, `NET-dns-deputy`, `RUN-oversub`, `RUN-cost` |
| FIRST-PARTY (P2) | PC `crates/plugins` (wasmi) and EC `plugin/src/wasm.rs` become bundled packages. EC `helper-host` moves to NativeConfined. | `PAR-*` (supersedes `RUN-first-party`), `CONF-cold`, `RUN-service-xflow`, `T-crash-commit` |
| GPU (P3), NATIVE (P4), HTML (P5) | Provider per-principal devices, then NativeConfined per OS (Linux, macOS, Windows), then the CEF component | `GPU-xprincipal`, `GPU-software-parity`, native NET column, `PKG-macos-jit`, `NET-cef-webrtc`, `CEF-cost` |

## Question resolution

| Question | Resolution |
|---|---|
| SYS-01.1 | Host core list above; first-party features are packages |
| SYS-01.2 | Inert attributed state and app startup without packages |
| SYS-01.3 | Contribution classes, typed contracts, `Chain` and leases |
| SYS-01.4 | Disposition table in the contract |
| SYS-02.1 | Identity model |
| SYS-02.2 | Class versus instance rule |
| SYS-02.3 | Versioned services with chain intersection and per-head partitions |
| SYS-02.4 | Probe-backed `host.capabilities()` |
| RUN-01.1 | Wasmtime with QuickJS-Wasm and per-principal processes |
| RUN-01.2 | Async turns with no re-entry |
| RUN-01.3 | Read-only rings, request-bound handles and snapshots |
| RUN-01.4 | Limiter, CPU caps, recycle and strikes |
| RUN-01.5 | One `Chain` and gate for every caller |
| RUN-02.1 | Network matrix |
| RUN-02.2 | Principal-level grants |
| RUN-02.3 | `Admitted<K>` and `PrincipalEpoch` poisoning |
| RUN-02.4 | NativeConfined only behind per-OS probes |
| RUN-02.5 | Host-owned trust UI, receipts for print and clipboard, and stated limits |

The Q, A1, B, A2 and C review roles ran in order. Every role and the lead ran on Claude Opus 5.5 at high effort, in separate contexts. A1 and A2 are the same agent. No role ran on the GPT family.
