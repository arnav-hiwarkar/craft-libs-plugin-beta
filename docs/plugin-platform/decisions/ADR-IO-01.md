# IO-01 and RENDER-05 · Local files, formats, inference and jobs

Accepted with verification. The [file and job contract](../contracts/io-jobs.md) is designed. Platform, crash, fairness and device tests have not run.

## Decision

Plugins never see paths, URLs, sockets or processes. They work through four host mechanisms, each with exactly one implementation shared by plugins and the apps' own code:

1. **Handle broker.** A trusted host action (picker, drop, relink sheet or a contributed format in Open/Save As/Export) issues an opaque grant. Every later open resolves beneath the held OS handle and refuses `..`, links, mount crossing, protected locations and remote classes the user did not pick. Grants record a persistent volume and file identity, so they rebind across restarts only on an identity match.
2. **Publication sink.** A `craft-libs/craft-files` crate is lifted from PhotoCraft's existing `atomic.rs` and `noreplace.rs`, and absorbs and replaces CL's existing `craft-atomic`, so there is one atomic writer. Typed durability forbids ignoring a directory-sync failure, which `craft-atomic` does today. It stages every external write privately, rechecks destination identity and policy, publishes with an exchange or replace step, and journals a typed per-file receipt. The apps' own Save, autosave, config and export use the same sink, so durability semantics never fork.
3. **Job service.** One state machine, resource ledger and memo store serve plugin jobs. App-native jobs join the ledger and state machine but keep their existing commit paths. A plugin job reads through brokered queries, `render.read`, leases and inference. The host records each read as a `ReadKey`. At the owner's publication point the adapter compares O(1) fingerprint tokens and either commits one Undo through DOC-01 or returns `Stale` naming the changed keys. Plugin jobs never lock the document, so Save stays available.
4. **Format and inference services.** A `FormatContribution` reader is the importer and a writer is the exporter. Both are listed in the app's normal Open, Save As, Export and EffectCraft Render Queue. Plugin-triggered native decoding runs only in HostNative (confined), which returns a typed plan and blobs to the `job` Store through rings. One `infer` service runs on CPU (`tract`) in HostNative or on the RENDER-02 GPU provider device. Models are parsed only in HostNative. (Final reconciliation: this replaces the earlier codec-worker and job-worker wording, per ADR-RUN-01 and the [ledger](../cross-topic-conflicts.md#final-reconciliation-2026-11-20).) A model is a package member or a granted file pinned by digest.

### Alternatives

| Alternative | Strongest argument | Why it lost |
|---|---|---|
| Per-object generation counters (A1) | Cheap compare and simple to explain | Both apps edit through whole-document closures (PC `session/execute.rs`, EC `session/edit.rs`). A counter would either mark every job stale on any stroke or allow undo ABA. Read-set fingerprints use identities both apps already have. |
| Checkpoints with a declared footprint and reservation (A1) | Predictable admission | The footprint is not knowable before the guest runs. Unit memoization keyed by recorded reads gives resume without a second declaration, and the ledger meters actual use. |
| Plugin-only file path, with native saves left as they are | Smaller first change | PC's atomic writer over-reports durability at three sites and EC truncates export output in place. Two sinks would mean two answers about durability. |
| Hard refusal of every cloud placeholder | No hydration traffic at all | It breaks the user's explicit pick of iCloud or OneDrive files, which native Open accepts. Hydration is an OS action on explicit user intent with no plugin-chosen remote. Discovery-time hydration stays refused. |

## Amendments applied from C

All 12 of C's required amendments are in the contract, with tests in the [acceptance cases](../implementation/io-acceptance.md):

1. Protected write locations, and read-only home/root picks.
2. Confined decode for every plugin-triggered open.
3. Write targets, `render.read` keys, query leases and inherited tickets join the read set. Publication compares only O(1) tokens.
4. Memo hits revalidate persistent fingerprints (PC per-tile BLAKE3 on immutable tiles, EC `frames_key`, file identity plus digest).
5. Typed `VisibleUnconfirmed` reasons and an exchange journal.
6. Folder-pick hydration sheet; sync roots classed `Remote(SyncRoot)`.
7. Mass-destruction pause, no permanent-delete fallback, beneath-grant watch events.
8. Provenance-keyed cache slots; a lossless claim is verified by reader round-trip.
9. Host-assigned job classes and blocking credits for memory and disk.
10. GPU admission against the adapter budget, chunked submission, and quarantine after a reset.
11. Revoke stops further renames; commit receipts name incarnation and revision.
12. Merged requests (`fs.request`), one `Receipt` type, and `doc.save` defined as writer → stage → publish.

The lead adds one dependency edge. Under Flatpak, EffectCraft's manifest grants `--share=network` and host spawn, so RUN-02 must confine Principal and HostNative processes with no network and no host-spawn D-Bus. ADR-RUN-01 meets this with its own seccomp and Landlock launcher inside the Flatpak sandbox ([ledger](../cross-topic-conflicts.md)).

## Integration slices

| Slice | Owner and codepaths | Exit proof |
|---|---|---|
| FILES-SINK | `craft-libs/craft-files` from PC `format/src/{atomic,noreplace}.rs`, `io/src/lib.rs:213–231`, absorbing CL `craft-atomic`; its callers migrate here. PC Save and autosave first. Then EC `settings/config.rs:215–236` and `export/src/{out.rs:19–29}`, `pipeline.rs:884–905` with owned-file cleanup. | `IO-crash`, `IO-crash-exchange`, `IO-exfat` |
| FILES-GRANT | Grant table and beneath-resolution. PC `craft::host::Files` picker and drop, `automation/src/workspace.rs` `RootCapability`. EC footage open via `FileReader::from_file`. Replaces BETA `ext-runtime/src/files`. | `IO-pick`, `IO-link`, `IO-swap-mtime`, `IO-protected`, `IO-remote`, `IO-restart` |
| JOB-CORE | Shared state machine, ledger and memo store. PC `engine/src/jobs.rs` and EC `background/jobs.rs` register native jobs. Plugin jobs commit through `edit.commit` with adapter fingerprints (PC `Surface::generation` plus tile digests, EC `frames_key`). | `JOB-aba`, `JOB-lost-update`, `JOB-readset-capture`, `JOB-cancel-race`, `JOB-fair`, `JOB-oom` |
| FORMATS | Format registry and scratch sessions (PC `file_cmds/batch_run.rs`). Writer and reader entries, with typed `options` and presets, in Open, Save As, Export and the EC Render Queue. HostNative decode. | `IO-parser-confine`, `IO-format-open-save`, `IO-format-roundtrip`, `JOB-scratch-batch`, `FORMAT-options` |
| INFER | CPU backend in HostNative, reusing BETA `ModelStore` digest addressing and tract. GPU backend on the RENDER-02 GPU provider. EC `curl` model install has no plugin entry. | `JOB-infer`, `JOB-gpu-fair`, `JOB-gpu-tdr`, `JOB-model-bomb` |

## Question resolution

RENDER-05.1: one inference service with digest-pinned local models and host-owned CPU/GPU admission. RENDER-05.2: read-set fingerprints with an atomic commit-or-stale result. RENDER-05.3: units, previews, scratch documents and receipts in a single job service. RENDER-05.4: host-assigned classes, deficit round robin and blocking credits.

IO-01.1: grants and leases, with no raw paths. IO-01.2: beneath-resolution, identity and the shared sink. IO-01.3: format contributions through confined decode and an ordinary edit. IO-01.4: `LossPolicy` with per-receipt losses and verified lossless writers. IO-01.5: Principal and HostNative confinement, broker-only reference resolution, and hydration on explicit pick only.

The Q, A1, B, A2 and C review roles ran in order. Q was restarted after the Sol usage limit. Every role, and the lead, ran on Claude Opus 5.5 at high effort in separate contexts. A1 and A2 are the same agent, and B and C each ran in their own context. No role ran on the GPT family in this round.
