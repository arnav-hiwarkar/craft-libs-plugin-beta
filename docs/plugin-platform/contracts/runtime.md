# Runtime, identity and authority contract

Designed in [SYS-01/02 and RUN-01/02](../decisions/ADR-RUN-01.md). Platform probes and cost gates remain pending. All types are proposed. [Files and jobs](io-jobs.md) · [Invocation](operations.md) · [Threats](../architecture/threat-model.md) · [Tests](../implementation/runtime-acceptance.md)

```text
ReleaseDigest = BLAKE3(canonical package tree)       host-owned read-only store; workers receive sealed bytes
Principal     = (PackageId, LineageKey)              one enabled lineage per PackageId; LineageKey is host-local, never persisted
Incarnation   = (Principal, ReleaseDigest, PrincipalEpoch)    revoke, disable or update bumps the epoch and kills processes
Grant         = (Principal, Right, Scope: Invocation|Document(inc)|Session|Persisted, IssuedTo: ContributionKey)
                IssuedTo is attribution only; handles carry the current PrincipalEpoch
Chain         = [User|Automation(kind)|Plugin(Incarnation)…] + cause    serialised as provenance
Sink          = Command(id, ParamRoles) | DocRead(Target) | DocEdit(Target) | Editor(Target)
              | LinkedRead(Locator) | FilterReplay | File(Grant, Rights)
              | Spawn(Reveal|OpenUrl|Exec|Script) | Setting(SecurityRelevant|Ordinary)
              | Binding | Print(EventReceipt) | Clipboard(EventReceipt) | Net
gate.admit(&Chain, Sink, Target) -> Admitted<K> | Denied(reason)     private constructor; sinks require the token
host.capabilities() -> per app, OS, backend, document kind and device; Available(mode) | Unavailable(reason)
```

## Generations

These are the only authority and lifecycle generations in the contracts. Domain counters such as `MixEpoch`, the audio `DeviceEpoch`, source epochs and transform, order and preview generations stay local to their own contracts.

| Generation | Changes on | Checked by |
|---|---|---|
| `PrincipalEpoch` | Revoke, disable, update or lineage change | Tickets, publication, leases and handles |
| `ProcessEpoch` (formerly WorkerEpoch) | Crash or restart of a process | Mounts, rings and replies |
| `MountEpoch` | Close, reparent, rebind or reinstall of a mount | Body publication and UI actions |
| `InteractionEpoch` | Capture, gesture or binding change, modal block | Samples, gesture terminals and previews |
| `DeviceGeneration` | GPU device loss or reset | GPU leases, frames and provider results |
| `ReleaseDigest` | A new release; it is the provider generation | Caches, bakes, migrations and provider binding |

An update without grant expansion re-issues Session and Persisted grants under the new `PrincipalEpoch`. Invocation and Document grants die with the old epoch. `DocRevision` scopes previews and receipts only; publication checks read sets ([invocation contract](operations.md#query-and-publication)).

## Identity and lifecycle

A package is a local folder or archive, admitted by digest. The compile cache is host-only, keyed by (digest, engine), and written by a guest-free compile run. Each principal has its own data folder.

- `LineageKey` is host-random and host-local, and is never persisted. Document records bind by `PackageId + TypeKey + SchemaVersion` ([document contract](document.md#records-and-participation)), so a project opens live on any machine with a compatible enabled provider. Lineage adoption governs only data folders and grants.
- Update within a lineage follows the generation rule above, and any grant expansion needs consent.
- A developer link re-admits every rebuild with a Developer badge. For user-linked folders with unchanged grants it keeps the `PrincipalEpoch` and recycles Stores ([SDK contract](sdk.md#author-workflow)).
- Downgrade is explicit and snapshots the data folder.
- `PluginRef` values in documents are inert names. A document can never activate code.

Contribution classes are host contracts:
- panels and slots;
- commands, hooks and tools;
- processors and object types;
- properties;
- formats, jobs and services.

A package can add any number of types and instances inside these classes without an app rebuild. A new class needs a host contract and a versioned SDK release. Resource library entries, brokered devices and capture use typed operations, not new classes ([invocation contract](operations.md#documents-resources-and-devices)).

Services are declared as `provides X@major` and `requires X@^major`. A call carries the caller's `Chain`, so the provider never gains authority. Requests run in warm `service` Stores partitioned by chain head. Memory, data, cache and ring slots belong to (provider, head), and the provider's base data is read-only to those partitions. Faults strike the executing code, while CPU and memory are debited to the head. Cycles, ambiguity and version mismatch return typed errors. Nothing is looked up by name.

## Processes and turns

**Process glossary.** These five names are the only process names in the contracts.

| Process | Count | Contents |
|---|---|---|
| Principal process | One per active principal, bound from a pre-sandboxed spare where the OS allows | `ui` Store; warm `service` Stores; `render` and `job` Stores from `InstancePre`, fresh per request except under the warm-state rule |
| HostNative (confined) | Per plugin-triggered native decode or CPU inference | Host code only: native app decoders and `tract` inference. The only place models are parsed. Results reach the requesting `job` Store through rings (IO-01) |
| HTML | One CEF tree per principal using HTML | Optional component, installed from Settings only, from a bundled or user-picked local archive and never downloaded. The install is a `Setting(SecurityRelevant)` sink |
| GPU provider | One per app per device, shared by all principals | Host code; a per-principal logical device in the shared GPU provider; hardware or advertised software adapter. An accepted residual risk ([threat model](../architecture/threat-model.md)) |
| NativeConfined | One per native principal, per OS only after probes pass | Native SDK shim |

Older names map onto these: "isolated plugin worker", "confined worker" and "principal worker" are the Principal process; "Codec", "codec worker" and "job/codec worker" are HostNative; "separate provider process", "trusted service code" and "confined service" are the GPU provider.

Every guest call is an async turn with an explicit `Cx`, and there are no blocking host calls or re-entrant callbacks.

Turn budgets are in thread CPU time, checked by epoch interruption, with a 10× wall-clock backstop:
- hooks: 50 ms, measured from turn start, with a queue wait triggering the hook's fallback;
- other `ui` turns: 100 ms;
- interactive tiles: 5 s;
- exact tiles: 60 s;
- decode: 30 s.

Longer work uses `cx.job`. Progress never extends a deadline.

Overrun or a trap recycles the Store and returns `Failed` or `Budget`. A per-input trap is negatively cached. Host-held `cx.state` is committed at turn end and restored after a recycle.

**Warm state.** State crosses `render` and `job` requests only as host-held checkpoints keyed by `(instance, RecurrenceGroup | MixEpoch)` and validated under the [timed contract](time.md#time-mapping-and-evaluation). The host may keep one warm Store per lane. It recycles that Store on a checkpoint mismatch, a generation change or a fault.

A strike needs a crash, an OS kill or more than 10× overrun. Three strikes in 60 s quarantine the release, and deaths caused from outside the process are not strikes.

A crash restarts the process with a new `ProcessEpoch` and shows mount placeholders. Renders retry once. An unpublished commit returns `Failed(WorkerLost)` or `Revoked` with no Undo, and a published commit is fetched idempotently.

Plugin render, service and job Stores and app-native rayon work share one CPU credit ledger: credits equal cores minus a UI reserve. Threads spawn lazily at sub-UI QoS. Credits are reserved all-or-nothing, with no hold-and-wait. Full-frame stages use host fork-join: the guest declares partitions, and N Stores read one read-only lease and write disjoint outputs.

## Data movement

The broker runs on its own thread. Adapters publish an `Arc<Snapshot>` per revision, so queries never touch the app thread. Only publication goes to the owner, coalesced per frame.

All bulk host→guest bytes travel in rings created once per (process, size class):
- query pages;
- `fs.lease` reads;
- tile inputs.

On Linux the rings are memfd with `F_SEAL_FUTURE_WRITE`; elsewhere, a read-only mapped duplicate. A slot is reused only after its request terminates. Output slots are copied into host memory before any hash, validation or cache key, and handles die with their causal request.

Limits:
- frames ≤ 16 MiB;
- CBOR depth ≤ 64;
- logs ≤ 64 KiB per turn in a 1 MiB ring.

## Authority

The gate admits the intersection of every principal in the chain.

- `Spawn` and `Net` are denied whenever any plugin is in the chain. The one exception is Reveal of a local grant held by every plugin in the chain.
- `Spawn(Script)` covers only OS interpreters. A registered `.jsx` is not a spawn: running it invokes its generated package's command contribution ([legacy families](sdk.md#legacy-families)) as a `Command` sink under chain intersection, so script launchers work under a plugin chain. Package-to-package command calls go through the same `Command` sink and gate.
- `Setting(SecurityRelevant)` covers network, script, automation, agent, control, file-trust and update preferences. It is never admitted under a plugin chain.
- `Print` and `Clipboard` need a host sheet or event receipt.

Provenance is a persisted `Chain` on every artifact that the host later executes or dereferences. Replay runs under `[User, …authors]`:
- DocRead, DocEdit and Editor sinks on the user's target are admitted.
- Every other sink needs the live grant of every author.
- A missing author opens a host "Review and adopt" sheet, and adopting clears authorship.

Provenance survives copy, paste, place and import remaps. Files published by plugins are matched to the IO-01 receipt journal by content digest. Opening such a file stamps the publisher on its locators and executable artifacts.

Plugins write locator fields only as grant handles. Every locator is dereferenced through the IO-01 resolver, so a remote or unknown volume needs a user relink. Old documents carry no provenance and are treated as User.

## Network denial

Wasm and QuickJS-Wasm get no socket, process or path imports. OS confinement is checked at launch, per process kind and for its descendants:

| Target | Network and process confinement | Name and deputy lookup |
|---|---|---|
| macOS | Seatbelt profile with no `network*`; signals only to self; mach names only under our bundle prefix; deny windowserver, launchservicesd, pasteboard, tccd and AV devices. Separately signed `craft-worker` gets `allow-jit`. | No libinfo, dnssd, nsurlsessiond or SystemConfiguration |
| Windows | One LPAC SID per principal, Job Object and an alternate desktop. Firewall and BFE state are recorded, and a change kills OS-primary workers. | If a DNS probe resolves, native and HTML stay `Unavailable` |
| Linux and Flatpak | Our launcher applies seccomp: no `socket` except `socketpair(AF_UNIX)` for the CEF tree; foreign arch and x32 killed; no ptrace or `process_vm_*`; signals scoped. Landlock grants no `/proc`. Without Landlock, native and HTML are `Unavailable`. | No bus |
| Web | Workers with CSP `default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'none'; connect-src 'none'`. HTML panels are `Unavailable`; GPU processors use browser WebGPU in the broker Worker (`P` until `WEB-gpu` passes). | n/a |
| Headless | Same workers as desktop, with no UI turns | Same as OS |

Host deputies are gated `Sink`s under any plugin chain, replayed provenance included. That covers EC script `Socket`, model `curl`, `helper-host`, Reveal, print and scripts.

Font install or download, including `craft-font-store` and OS on-demand fonts, is a `Net` sink and is never admitted under a plugin chain. `text.fonts()` lists installed fonts only, and missing-font substitution never triggers a download ([text](render.md#text)).

EC expressions are document data run by a host interpreter with no I/O, and a lint keeps the `expr` crate free of I/O dependencies.

Not promised:
- local timing channels;
- exfiltration the user carries out (export, clipboard);
- data read before a revoke;
- engine-bug isolation within one principal;
- driver isolation beyond per-context address spaces.

## Disposition of existing systems

| Existing | Disposition |
|---|---|
| BETA `ext-runtime`, `ext-ipc`, `ext-core` manifest | Become `craft-plugin-host` and `craft-plugin-ipc`. Retire the class gate, frame pump, Progress deadline re-arm, raw-path grants and the guest-writable compile cache. |
| BETA `NativeFull`, `Network(host)`, the WK filter, V8/rquickjs | Delete in the RUNTIME slice. |
| PC `crates/plugins` (wasmi), EC `plugin/src/wasm.rs` | Become bundled packages after the parity gate, then the in-app loaders are deleted. |
| PC `automation/src/workspace.rs`, EC `automation/src/{backend,confine}.rs` | Become Automation principal policies, preserved bit for bit by a differential test. |
| EC `helper-host` | Moves to NativeConfined. |
| App native decoders reached from plugins, BETA `ModelStore` and tract | Run in HostNative. |
