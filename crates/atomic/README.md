# Craft Atomic

`craft-atomic` is the one crash-safe file writer for the Craft suite. Every write stages a
temporary file beside its destination, publishes it with one rename and returns a `Receipt` that
never claims more durability than the flushes delivered.

## Receipt

| Outcome | Meaning |
|---|---|
| `Unchanged(error)` | Nothing was published. The destination keeps its bytes and the staging file is removed. |
| `Displaced { error, original }` | Windows only: `ReplaceFileW` moved the original to its backup name and could neither finish nor restore it. The original survives at `original`. |
| `VisibleUnconfirmed(Unconfirmed)` | The new file is in place, but a flush or cleanup did not confirm it. `Unconfirmed` carries every reason and its error. |
| `Durable` | The content and the folder entry were fully flushed. |

The reasons are `FullFlushRefused` (Apple `F_FULLFSYNC` refused, plain `fsync` done),
`DirOpenDenied`, `DirSyncUnsupported`, `DirSyncFailed` and `StageRemovalFailed`. `Receipt::published`
returns the `Reasons` set of a visible file, and `Receipt::visible` drops it. `Durability` (durable, the
reasons, or not measured) is the value apps report, with `id()` for automation output. Every unconfirmed receipt is logged at `warn`, and a displaced one at
`error`.

## Writing

```rust
use std::path::Path;

let receipt = craft_atomic::write(Path::new("project.craft"), b"document");
let created = craft_atomic::write_new(Path::new("export.png"), b"image");
```

`WriteOptions` selects:
- private creation (`NewFilePermissions::Private`) for settings and recovery data;
- `Links::Replace` to replace a symbolic link instead of writing through it;
- `Metadata::ModeOnly` to keep only the mode and use a plain rename on Windows;
- an injected `AtomicFs` and `RenameRetry` for failure injection.

`WriteOptions::stage` returns a `Stage` for a writer that streams over time (an export). Dropping
an unpublished stage removes only its own temporary file. A stage is `Write + Seek`, `Stage::replaces`
says whether a file already existed, and `Stage::publish` returns the receipt. Every filesystem step,
including `ReplaceFileW`, goes through `AtomicFs`, so a writer confined to a directory handle supplies
its own implementation.

## Guarantees

- A replacement keeps the replaced file's mode and group. Extended attributes, the macOS creation
  time and ACL are carried on a best-effort basis, and Windows uses `ReplaceFileW` to keep the
  DACL and named streams.
- A read-only destination is refused before anything is written.
- A destination replaced by another program while staging is left alone (`Unchanged`).
- A write with no existing destination, and `write_new`, never replace a file that appears meanwhile: an exclusive rename where the platform has one, then a hard
  link, then an exclusive copy.
- A missing folder is an error; no folder is created.
- Temporary files are named `<name>.<pid>-<n>.craft_tmp`, not dot-named, so an SMB server that
  hides dot files cannot hide the published file. `temp_owner` names the process that wrote one,
  for sweeping leftovers after a crash.
- Renames refused by a transient lock are retried with capped exponential backoff on Windows.
