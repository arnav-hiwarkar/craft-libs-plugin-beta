# Local files, formats, inference and jobs acceptance

Planned cases for [IO-01/RENDER-05](../decisions/ADR-IO-01.md). They use public SDK packages, the apps' normal Open, Save, Export, Render Queue and job paths, controlled local fixtures, and network observation at the OS level. None of them has run yet. Run each case on EffectCraft and PhotoCraft where the capability is advertised, and record the verification fields from [evidence rules](../architecture/evidence-and-verification.md).

| ID | Scenario | Expected result |
|---|---|---|
| IO-pick | Use the picker, drop, relink sheet and a contributed Open. Try an unpicked sibling, `..`, a symlink, a magic link, a mount crossing, a named device and a URL. | Only the chosen target opens. Each refusal is typed. No path or URL reaches the guest. |
| IO-link / IO-swap-mtime | Replace a leased file with a symlink, a different inode, and an identical-length, `cp -p` copy (same mtime) during read and before publish. | Reads report `Changed`. Publish returns `Unchanged(DestinationChanged)`. No write follows a link. |
| IO-protected | With a home-root pick, attempt to create a LaunchAgent, `.zshrc`, a Startup link, `.git/hooks/pre-commit` and an app config file. | Home and root picks are read-only. Every write-class attempt in a protected set returns `Refused(Protected)`. |
| IO-remote / IO-persist-remote | Pick from SMB, a FUSE mount, a Dropbox/iCloud sync root and a Flatpak portal path. Restart. | `Remote` and `Unknown` need an explicit labelled pick. Sync-root descent is refused. Portal is `Local(Portal)`. Remote grants do not persist without opt-in. |
| IO-placeholder-pick / IO-placeholder-folder | Pick one dataless file, then a folder of 400 placeholders. Afterwards let the plugin discover another placeholder. | Single picks hydrate with progress. The folder sheet shows count and size before granting. Discovery returns `Refused(Placeholder)`. After the grant, wire bytes caused by plugin reads are 0. |
| IO-restart / IO-usb-replug | Persist folder grants, restart, then unplug and replug USB media and replace a folder with an equal-path copy. | Rebind happens only on an identity match. Otherwise the relink sheet asks. |
| IO-crash / IO-crash-exchange / IO-exfat | Kill at every sink step on APFS, ext4, NTFS, exFAT, SMB and FUSE for a single file and a 500-file `AllThenPublish`. | The journal replays. Each file reports `Unchanged`, `VisibleUnconfirmed(reason)` or `Durable` truthfully. Foreign inodes are restored. Volumes without exchange give `NoAtomicPublish` or the consented fallback. |
| IO-rename-delete / IO-mass-destroy / IO-trash-smb | Rename and trash inside a grant. Mass Replace across a picture folder. Permanently delete a pre-existing file. Delete on SMB with no trash. | Receipts per effect. Pause after 50 pre-existing files. `Permanent` delete of a non-journal file needs confirmation. `TrashUnavailable` never falls back. |
| IO-watch-boundary | Watch a grant while files move into and out of it, and with 1M files. | Out-of-grant names never appear. `WatchLimit` is typed. |
| IO-parser-confine | `doc.open` a fuzzed PSD and a plugin-written file through a native reader, and an SVG with remote `href`. | Native decoding runs in HostNative. A crash fails the open only. No in-process parse, no network attempt, references are inert. |
| IO-format-open-save / IO-format-roundtrip | Register a reader and writer. Open, edit, Ctrl+S, Save As, Export and EC Render Queue output. Make the writer lossy on one object type. | Format entries appear in native dialogs. Import is one Undo. Ctrl+S on a lossy writer becomes Save As with a loss report. A failed write leaves the old file hash unchanged. |
| FORMAT-options | Register a writer with a typed `options` schema. Export with defaults, a saved preset and edited values, from the UI and headless. Reopen the app and reuse the preset. | The host renders the options in its sheets. The writer receives typed values. Presets persist in the principal's data folder and are invisible to other principals. |
| IO-export-loss | Export a document with a plugin object under each `LossPolicy`. | Refuse, Bake, Flatten and Drop behave as named. Losses are listed per receipt. Authored state is unchanged. |
| JOB-lost-update | Alias of `EDIT-lost-update`; budget per conformance tolerance table. | |
| JOB-aba / JOB-readset-capture | Edit, then undo back to identical content during a job. Edit a write target. Edit a precomp behind a `render.read`. Insert into an empty filter set. Start from a UI ticket at an old revision. | An identical undo commits. Every other change returns `Stale` with the exact keys. Zero partial edits. |
| JOB-save-during / JOB-cancel-race / JOB-revoke | Save while a plugin job runs. Race cancel against commit over 10k interleavings. Revoke during `Finishing`. | Save is never blocked. Exactly one terminal state. Revoke stops further renames and reports partial receipts. |
| JOB-commit-cost | Commit with 50, 1,024 and 100k read keys. | Owner-thread p99 stays below 2 ms at 50 keys and grows sub-linearly through coalescing. |
| JOB-resume / JOB-memo-cost | Interrupt a segmentation batch, restart, change one input, and resume on a 20k² 16-bit spilled image. | Memo hits revalidate. Changed units recompute. The output equals the uninterrupted bytes on the same backend and profile, and revalidation reads only tile digests. |
| JOB-scratch-batch | Batch-process a folder through scratch documents and a writer. | Scratch documents never appear in the UI unless adopted. Per-file receipts. |
| JOB-fair / JOB-oom / IO-disk-full-autosave | Five plugins request the top class while the user paints on a spilled document. One plugin allocates past its share. Memos fill the disk. | The host downgrades the classes. The UI-load row of the conformance tolerance table holds. Typed memory failure, no swap storm. Autosave headroom is preserved. |
| JOB-infer / JOB-gpu-fair / JOB-gpu-tdr / JOB-model-bomb | Segment a local image with a digest-pinned model on CPU and GPU while scrubbing. Inject a TDR. Load a model with huge dims and external-data paths. | No network or model download. CPU inference and model parsing run only in HostNative. Compositor frames are not delayed by more than 4 ms chunks. The reset quarantines the package GPU route and the job reruns on CPU. The bomb is refused before allocation. |
| JOB-cache-key | Feed a `cache.put` matte to render, then change its input and export exactly. | The slot is used only on a key match. The document is not dirtied. Exact export recomputes or fails, and never consumes an unverified cache. |
| IO-web | Run Chromium and non-Chromium targets. | Capabilities are advertised per browser. Downloads and handle writes report `VisibleUnconfirmed(Browser)`. `AllThenPublish` and watch are unavailable. |

Paired performance hypotheses:
- Beneath-open at or below 20 µs with `openat2`, and at or below 60 µs with the component walk.
- Native leases at or above 1.5 GB/s.
- Admission to first progress under 50 ms.
- Cancel p95 under 150 ms when cooperative, and under 500 ms by kill.
- A 500-file publish within 5% of plain writes.
- A UserWaiting export within 10% of native wall time.
