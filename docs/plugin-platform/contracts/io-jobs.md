# Local file, format, inference and job contract

Designed in [IO-01/RENDER-05](../decisions/ADR-IO-01.md). Runtime, platform and device gates remain pending. All types are proposed. [Invocation](operations.md) · [Document](document.md) · [Render](render.md) · [Tests](../implementation/io-acceptance.md)

```text
fs.request(File|Files|Folder|Output{suggested_name, format}|Sibling(h, name), reason) -> Grant? (host picker or sheet)
fs.open(h, RelPath) -> Handle | Refused(Escape|Link|Mount|Protected|NotLocal|Placeholder|OutsideGrant)
fs.list(h, cursor, budget);  fs.lease(h) -> ReadLease{read_at(off, len <= 16 MiB)}
fs.rename(h, from, to) | fs.delete(h, name, Trash|Permanent) -> Receipt;  fs.watch(h) -> coalesced hints
stage(Output{grant, name}, CreateNew|Replace) -> StagedFile;  publish(set, AllThenPublish|PerArtifact) -> Receipt[]
Receipt = (name, Unchanged(e) | VisibleUnconfirmed(reason) | Durable, losses[], incarnation?, revision?)
FormatContribution = (id, extensions, magic, reader?: bytes -> Plan, writer?: ExactStream|Snapshot -> StagedFile,
                      options: PropsSchema)
doc.open(h, Format?) -> DocTarget::Scratch;  doc.adopt(t);  doc.save(t, Output, Format, LossPolicy) = writer -> stage -> publish
job.start(requested_class <= host_class, memory_hint?) -> Job{progress, preview, cancel, scratch()}
job.unit(key, f) -> T                  (memo keyed by CacheKey, validated by recorded persistent fingerprints)
infer(model: PackageMember|Grant + sha256, inputs: leases, device_pref) -> outputs | UnsupportedOp
Terminal = Committed(Receipt) | Stale(ReadKey[]) | Cancelled | Failed(e) | Revoked
```

## Grants and resolution

The host issues a grant only from a trusted action: a picker, drop admission, the relink sheet, or the app's Open/Save As/Export dialog for a contributed format. A grant is the runtime `Grant` (principal, right, scope, and `IssuedTo` contribution for attribution only) plus OS handle, persistent id and volume class. Handles carry the current `PrincipalEpoch` ([generations](runtime.md#generations)). Scopes are Invocation, Document(incarnation), Session or Persisted. Rights are Read, Create, Replace, Rename and Delete, each relative to the held handle.

Resolution happens beneath the handle: `openat2` with `RESOLVE_BENEATH|NO_SYMLINKS|NO_MAGICLINKS|NO_XDEV` on Linux, falling back to a component walk; `openat` with `O_NOFOLLOW_ANY` on macOS; a relative `NtCreateFile` that refuses reparse points on Windows.

Write-class rights are refused in each OS's protected set:
- autostart and launch-agent locations;
- shell startup files, `.ssh` and `.git/hooks`;
- app and plugin configuration;
- the OS startup folders.

A pick of a home or volume root is read-only. Persisted identity is (filesystem UUID or volume serial, file id), plus a bookmark or portal document id. A lease adds length, mtime and ctime, and a ctime-only change is rechecked by digest. On restart a grant rebinds only on an identity match; otherwise the single relink sheet asks again.

Volume class is decided by I/O, never by path text:
- `Local`, `Local(Portal)`, `Removable` and `Browser` behave normally.
- `Remote` and `Remote(SyncRoot)` require an explicit labelled pick and are Session scope unless the user opts in. Descending into a sync root from a broader grant is refused.
- `Unknown`, such as FUSE, is treated as Remote.
- A placeholder is hydrated only on an explicit pick. A folder pick shows "N files / X GB: Download / Skip" before the grant is issued. Discovery-time placeholders return `Refused(Placeholder)`. Every host thread that serves a plugin disables materialization (`IOPOL_MATERIALIZE_DATALESS_FILES_OFF`, `FILE_FLAG_OPEN_NO_RECALL`).

Watch events carry only beneath-grant relative paths, and nameless `MovedIn/MovedOut` events for anything outside the grant. Watches have per-package and global caps (`WatchLimit`). The web returns `Unavailable`.

## Publication

`craft-files` owns staging, identity, exchange or replace, the receipt journal and handle-relative rename, delete and trash. It absorbs and replaces CL's existing `craft-atomic`, so there is one atomic writer crate; `craft-atomic` callers migrate in FILES-SINK. Temp files are dot-named and hidden.

On Unix, publish exchanges the staged and destination files and verifies the swapped-out identity, swapping back on mismatch. The exchange is journaled, so recovery restores a non-owned inode. Volumes without exchange get `Refused(NoAtomicPublish)` unless the user consents to verify-then-rename. Windows verifies identity and then calls `ReplaceFileW`; the remaining window is an open verification item.

Durability is typed. `VisibleUnconfirmed` names `FullFlushRefused`, `DirOpenDenied`, `DirSyncUnsupported`, `DirSyncFailed` or `Browser`. A directory-sync failure is never ignored: it yields `VisibleUnconfirmed(DirSyncFailed)`. `Receipt` is the only publication outcome type; document writers return it too ([document contract](document.md#open-migrate-and-write)). A batch is `Durable` only after a covering full flush on the same device, issued after the directory flushes.

`AllThenPublish` validates the complete set before the first rename. `PerArtifact` reports each file independently. The web offers only downloads or Chromium handle writes, each reporting `VisibleUnconfirmed(Browser)`, and no `AllThenPublish`.

A published file is never undone by document rollback. Cleanup removes only files whose id matches the journal.

Destruction guards:
- `Permanent` delete is allowed only for files in the plugin's own receipt journal; any other permanent delete needs host confirmation.
- A job pauses on a host sheet after replacing or deleting more than 50 files that existed before the job.
- A missing trash returns `TrashUnavailable` and never falls back to permanent delete.
- Recursive delete stays beneath the grant without crossing mounts or links.
- A per-grant lock serialises rename, delete and publish.

Revoke stops further renames and returns partial receipts. Cancel after the point of no return yields `Finishing`.

## Formats and scratch documents

A plugin reader runs in a `job` Store and returns a typed plan plus content-addressed blobs. The host builds app objects from that plan into a scratch session (PC `file_cmds/batch_run.rs:367–372`, EC equivalent) or into the current document through one DOC-01 edit. Native app readers run in HostNative whenever a plugin triggers the open or supplies the bytes, and return their plan to the `job` Store through rings. External references in SVG, playlists, PDF, Lottie or a model's external data resolve only through the broker, and URLs are inert.

A writer receives the render owner's exact stream or a snapshot and writes into a `StagedFile`. `options` is a typed `PropsSchema` that the host renders in its Export and Save As sheets; named presets live in the principal's data folder, and the chosen values reach the writer as typed arguments. `LossPolicy` (Refuse, Bake, Flatten, Drop) applies per batch, and every receipt lists the losses.

A document opened by a plugin reader remembers its source format. Ctrl+S writes that format only after the writer's lossless claim has passed a reader round-trip check for that release and content; otherwise Ctrl+S becomes Save As with a loss report. A scratch document is discarded unless it is adopted.

## Jobs

The host assigns each job's class:
- Interactive: a visible host preview.
- UserWaiting: started by a gesture, with foreground progress.
- Background: everything else.

A plugin may request only a lower class. Only Background uses EcoQoS or utility QoS. Each plugin job runs in its own `job` Store inside the principal process, never the `ui` Store, so a kill drops only that Store ([runtime contract](runtime.md), superseding the earlier job-scoped process). Native app decoders and CPU inference triggered by plugins run in HostNative ([process glossary](runtime.md#processes-and-turns)).

The ledger gives each package at most half of every contended pool, applies deficit round robin by measured CPU and bytes within a class, and serves classes in strict priority. Memory and disk use blocking credits: beyond its share a job waits or fails with a typed error. The pool is available memory minus app headroom. Enforcement uses the Wasm limiter, Job Objects, rlimit or cgroups, and on macOS a footprint poll followed by a kill. The disk ledger keeps at least max(2× autosave, 5%) free. Broker I/O runs at the job's class priority.

The read set includes:
- every brokered read;
- every write target, except operations the host declares commutative;
- every `render.read` `ReadKey` (EC `input_key`, PC Smart Filter prefix) ([render contract](render.md#guest-reads));
- query leases as (projection, filter, collection fingerprint);
- cache reads;
- the invoking context ticket's reads.

Publication compares only O(1) tokens: a generation, an `Arc` pinned by the snapshot, or a precomputed digest. Deep comparisons run off the owner thread, and sets over 1,024 keys coalesce to their parent. Cancel and commit are ordered by one atomic. A terminal state and its receipts are journaled before the guest is notified. A lost acknowledgement is retrieved idempotently by job id. A closed target document cancels commit outputs, but publish-only outputs continue.

A memo is keyed by the unit key, the principal, the model and the one `CacheKey` ([render contract](render.md#composition-and-export)), which carries release, backend and `ExecProfile`. It maps to its recorded reads with persistent fingerprints:
- PC: per-tile BLAKE3 cached on the immutable tile.
- EC: `frames_key`.
- Files: identity plus digest.

A hit revalidates every fingerprint. Sequential units chain the previous output digest. Memos and the journal live in app data under a quota with LRU collection.

A `cache.put` slot is keyed by `CacheKey` and stores its read-set digest and provenance. It is consumed only on a key match, does not dirty the document, and never certifies exact export unless it is reproducible under `BakeKey` ([document contract](document.md#records-and-participation)).

## Inference

The CPU backend runs `tract` in HostNative, and results reach the requesting `job` Store through rings. The GPU backend runs on the RENDER-02 provider device, admitted against the adapter budget minus the app's reservation, in chunks of at most 4 ms of measured GPU time. The first reset attributed to a package quarantines its GPU route, and the job reruns on CPU with its backend pinned. Operator coverage is advertised per backend. An unsupported operator in exact mode returns `UnsupportedOp`, with no silent fallback.

Models are parsed only in HostNative, with operators and dimensions bounded before allocation. Memos are keyed by backend, because results are not bit-identical across devices. The host never downloads a model for a plugin.

## App adapters

| Adapter function | PhotoCraft | EffectCraft |
|---|---|---|
| `fingerprint(snapshot, ReadKey)` | `raster/src/lib.rs:612–643` `Surface::generation` and tile digests; `PartialEq` on small fields | `analysis/frames_key.rs`, layer/group/footage stamps, handle identity for media |
| `scratch_session()` | `Session::new()` as in `file_cmds/batch_run.rs` | Hidden project session |
| Native job registration | `engine/src/jobs.rs` state machine and slot cap move to the shared service. `edit_job` and `lock_document` stay for native jobs only. | `background/jobs.rs:118–197` admission and epoch cancel, no UI-thread joins. `analysis/roto.rs` cache moves to `cache.put`. |
| Sink adoption | Save, autosave, export through `craft-files` | Config, export pipeline and Render Queue output modules through `craft-files` |
| `craft-atomic` callers | Migrate to `craft-files` in FILES-SINK | Migrate to `craft-files` in FILES-SINK |
| Picker and drop | `craft::host::Files` returns grants | Footage import and drop return grants |

Platform limits that cannot be guaranteed:
- provider behaviour on every kernel and filesystem;
- portal inode stability;
- the verify-to-`ReplaceFileW` window and the path-based trash race;
- honest flushes on SMB, NFS or FUSE;
- Linux plain sync folders;
- the Windows dehydrate-then-read window;
- data already read before a revoke;
- browser durability;
- decoder memory safety (contained, not proven);
- GPU resets.
