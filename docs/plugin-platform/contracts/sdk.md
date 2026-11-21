# Public SDK, ABI and evolution contract

Designed in [RUN-03/04 and SDK-01](../decisions/ADR-RUN-03.md). All types are proposed. Spikes, fuzzing and per-target probes have not run.

Related: [Runtime](runtime.md) · [Examples](sdk-examples.md) · [Tests](../implementation/sdk-acceptance.md)

```text
profile  MVP + simd128 + bulk-memory + multi-value + sign-ext + sat-conv + mutable-globals
         relaxed-simd => Preview only; no Wasm threads (host fork-join)
imports  craft.memory (imported, host max), craft.submit(op, args) -> ticket,
         craft.read(h, off, dst, len), craft.write(h, off, src, len), craft.log
exports  craft_abi() -> 2, craft_alloc(len), craft_turn(ptr, len) -> Idle | Pending
Turn     Hello{levels} | Invoke{contribution, req} | Event | Completion{ticket, Ok | OpError}
OpError  {code, op, index?, path?, detail?, fix?}
code     Invalid | Denied | NotFound | Stale(ReadKey[]) | Expired | Cancelled | Budget | Busy
         | Unsupported | Unavailable(reason) | Revoked | Failed | Other{raw}
```

## Ownership

| Owner | Owns |
|---|---|
| CL `crates/plugin-api`, dependency-free | Domain types, including `plugin-api::doc::ExtensionRecord`, `TypeKey` and `BakeKey`; the error registry, `up`/`down` mappers, `api.lock` |
| CL `plugin-sdk`, `sdk-gen`, `plugin-conformance`, `craft-idmap` | Rust `Cx`, executor and `#[craft::api]`. TS, docs, MCP/CLI descriptions and the fake host, all generated from the locks. Contract diff, conformance kit, alias resolver |
| CL `plugin-host` | One validator, patch apply, mappers, alias resolution, downlevel, dispatch, the capability table, desktop and web backends |
| `<app>/crates/plugin-api-<app>`, dependency-free | `pc.*` and `ec.*` types, written to `<app>.lock` with a `provides` section |
| `<app>/crates/plugin-adapter` | Typed handler binding, target resolution, `errors.rs`, `render.rs`, `color.rs`, snapshot export |
| App engine | Typed `CommandSpec`, `engine.lock`, `resolve_persisted` |

The guest sees neither `Chain` nor engine ids. The worker forwards `(op, bytes)` without parsing.

## SDK sugar

These helpers are generated SDK code over the operations above. They add no host operation and no authority.

| Helper | Expands to |
|---|---|
| `cx.ui.prompt({title, value})` | `ui.dialog` with a host-standard text prompt body; resolves to the value or `null` on cancel |
| `cx.overlay.publish(gesture, node)` | Publishes an overlay at the gesture's declared `OverlayRoles` stage on its owning viewport ([invocation contract](operations.md#capture-and-preview)) |
| `cx.query(op)` | One-shot query: `query.open`, `query.page` to completion within the turn budget, then close; returns the projection or an `OpError` |
| `cx.commit(label, ops)` | `edit.commit` on the invoking ticket |

Error codes keep the casing of the `code` enum in every language: TypeScript compares `e.code === "Stale"`.

## Envelope

The envelope is strict CBOR:

| Element | Rule |
|---|---|
| Lengths | Definite only |
| Tags | RFC 8746 little-endian typed arrays and the handle tag |
| Map keys | Text, no duplicates, at most 4,096 entries |
| Floats | Schema width |
| Simple values | `true`, `false` and `null` only |
| Strings | Valid UTF-8 |
| Size | At most 16 MiB per frame |
| Depth | At most 64 |
| Item count | At most the bytes remaining |

The broker decodes each envelope once and debits the cost to the principal. Hashed or persisted values are the host's deterministic re-encoding: sorted keys, shortest integers, canonical NaN. Integers above 2^53 become TS `bigint` or opaque strings. A Store may hold at most 1,024 outstanding tickets; further calls get `Busy`. Numeric arrays of more than 256 elements use typed arrays, and large pages use ring handles.

## Schema and levels

The source is Rust types with `#[craft::api]` and `#[op(id = "doc.layer.rename", major = 1, since = "1.0", alias = [...])]`. Fields are tagged `#[info]` or `#[semantic(neutral = ..., governs = [...])]`. Units are `#[unit(px|ticks|deg|s)]`. Authority- and target-bearing structs are `#[closed]`.

Allowed shapes:
- closed objects;
- `kind`-tagged unions with `Other`;
- arrays;
- numbers with a unit;
- handles.

Forbidden:
- `allOf`;
- `anyOf` beyond `Option`;
- `patternProperties`;
- regex patterns;
- serde `flatten`, `untagged` and `deserialize_with`;
- defaults that are not neutral values.

Public types are `#[non_exhaustive]` with constructors. The runtime never interprets schema at run time.

Operation ids follow `ns.objectPath.verb@major`. The operation classes are:
- Domain, the default;
- App, which needs `why_not_domain`;
- Query;
- Tool;
- Workspace;
- Protected and Internal, which are never callable.

A guest calls an operation only once its handler is typed as `fn(&mut Session, Args) -> Result<Out>`.

At enable time the host negotiates `level = min(host, declared)` for each area. A new minor may add:
- operations;
- optional arguments with a neutral default;
- `#[info]` result fields;
- `#[semantic]` fields that declare a neutral value.

| Layer | Older guest on a newer host | Newer data seen by an older guest | Authority fields |
|---|---|---|---|
| Manifest | Unknown key warns | — | An unknown key in a critical section or `critical=[…]` refuses the contribution or package. An unknown class is `Unavailable(UnknownClass)`. |
| Live calls | Strict: `Invalid{op, path, fix}`. Writes are patches, and an absent field means unchanged. | `#[info]` fields beyond the guest's level are dropped. Hidden non-neutral semantic state marks the object `NeedsLevel`, and a whole replace or a patch to a governed field is refused. Enums yield `Other{raw}`, which the host validates, re-encodes and gates on write. | `#[closed]` types and target-bearing types never accept unknown keys or `Other`. |
| Host and worker | postcard in lockstep with `PROTOCOL` | Same | — |
| Persisted | Tolerant decode, then `craft-idmap`, migration and a diagnostic | Unknown keys are preserved | An unknown provenance or locator key makes the document read-only |

Lists are patched by stable key: insert, remove, move or set. A whole-list replace is refused if the list holds hidden or `Other` elements. Every guest `edit.commit` is checked against the read set recorded under its ticket, and a mismatch returns `Stale` at DOC-01 publication.

Every published operation major is served forever through composed pure mappers on patch types. Retiring a major needs CODEOWNERS approval plus an unmappability fixture, and makes only that operation `Unavailable(Retired)`.

## Persisted identity

`engine.lock` lists every shipped engine id with:
- the first-shipped id, which is what gets persisted;
- flattened aliases;
- tombstones;
- class;
- the args-schema hash.

A hash change requires `migrate_args(from_hash)` or `persistable = false`.

`craft-idmap` resolves engine ids, public operation aliases, contribution `aliases` and `legacy.<app>.<id>` keys. Engine records keep engine ids, and extension records use `TypeKey = (PackageId, LocalTypeId)`.

### The lock and the resolver (IDS-NOW)

Each app commits `plugin-contract/engine.lock`, format 1, TOML, written only by `cargo xtask engine-lock --update`:

```text
format = 1
tombstones = ["<retired name>", …]          omitted when empty
[[entry]]                                    sorted by id
id         = "<first-shipped id>"            what saved files hold
current    = "<id the registry has now>"     omitted while equal to id
class      = "<app class>"                   PC: command, query, shell, tool, hold. EC: command, query, effect
args       = "<16 hex digits>"               omitted for classes saved without arguments
aliases    = ["<every intermediate id>"]     flattened: no chains, at most 64
migrations = [{ from = "<hash>", name = "<app migration>" }, …]   at most 64, applied in order
```

- **Args hash.** The first 8 bytes of SHA-256 over the sorted, deduplicated argument keys, one per line. PC reads the keys from each command's params text, EC from its parsed params schema, and EC effects use their parameter ids. Queries are never saved with arguments, so they carry no hash; `persistable = false` is that class.
- **Bounds.** A lock is at most 8 MiB, 65,536 entries and 65,536 tombstones. A name is 1 to 256 bytes of printable ASCII without quotes, backslashes or spaces. A class is 1 to 32 lowercase letters, digits or dashes.
- **Edits.** `--rename OLD=NEW` keeps the first-shipped id and moves the old current id into `aliases`. `--retire ID` moves every name of the entry to `tombstones`. `--migrate ID=NAME` records a migration from the locked hash. No edit takes a name another entry or a tombstone holds.
- **Check.** `craft_idmap::check(lock, registry, migrations)` is pure. Each finding is blocking unless marked *update*:

| Finding | Meaning |
|---|---|
| `EmptyRegistry`, `EmptyLock` (*update* seeds) | Nothing to compare: the check never passes vacuously |
| `Unlisted` (*update*) | A registered id has no entry |
| `Stale` (*update*) | A class change, an argument hash appearing or disappearing, or a change a recorded migration covers |
| `Missing` | An entry's current id is no longer registered: an unaliased removal |
| `Reused` | A registered id is a tombstone or an earlier name of another entry |
| `ArgsChanged` | The argument hash changed with no migration from the locked hash |
| `UnknownMigration` | The lock names a migration the app does not register |
| `Duplicate` | The registry lists an id twice |
| `Conflict`, `Cycle` | A name is claimed twice, or aliases point at each other's current ids |

- **Resolve.** `Resolver::new(lock)` refuses a lock with a `Conflict` or `Cycle`. `resolve(persisted, args, migrations)` returns `Current { id, args }` after running every recorded migration in order, or `Unknown`, `Tombstoned` or `MigrationFailed`. Saved arguments carry no hash, so every migration is idempotent on arguments already in the new shape. `persisted(current)` gives the first-shipped id that saves write.
- **Apps.** Each app passes one `Site` per load path to `resolve_persisted(site, id, args)`; one test matches on every `Site` and drives that real path with an old id and argument under a simulated rename. An unresolved reference is inert, keeps its saved bytes, and reports a typed `Unresolved` naming the site.
  - PhotoCraft resolves at use, so saved data is never rewritten: Smart Filters when they render and are authorized, Action steps when engine, panel, Batch, droplet or Record Again playback runs them, and keyboard shortcuts as preferences and shortcut sets load. Layer details show an unresolved filter's reason. A filter or step created now stores the id it ran.
  - EffectCraft resolves effect ids when a project opens or is imported or a template loads, and when a preset applies; it resolves replay steps and script commands when they run. Project and preset saves write each effect's first-shipped id. An open reports unresolved effects with its toast, and an import with its warnings.
- **Gate.** A `cargo test` in each app builds the registry list, checks it against the committed lock and requires the file to be the lock's own text. `cargo xtask engine-lock [--check | --update …]` runs that test, with the edits passed in `CRAFT_ENGINE_LOCK_ARGS`.

## Capabilities

Static needs are checked at enable time, against the app's `provides` section:
- package `needs` levels;
- each contribution's `requires` and `prefers`.

When a need is unmet, only that contribution becomes `Unavailable(reason)`.

`host.capabilities()` and `host.capabilitiesChanged` report run-time facts only:
- `render.gpu`;
- `ui.window.native`;
- `ui.html`;
- `time.audio`;
- `native.helper`;
- `input.tool`;
- `ui.display`.

Each fact carries a reason when unavailable:
- `Headless`;
- `WebTarget`;
- `ProbeFailed(id)`;
- `NotInstalled`;
- `ArchMismatch`;
- `DeviceLost`;
- `WasmFeature`;
- `Retired`.

No app name, version, OS, renderer or vendor is exposed.

## Legacy families

The loader classifies a module by its exports, and wraps it with a first-party shim in a generated package. Both link inside one Store. Manifests are extracted in the Principal process, and the compile run stays guest-free.

| Family | Shim |
|---|---|
| PC `pc_abi` 1 | A process plan wanting `ImageDesc{DocumentEncoded@pc-359051359, f32 interleaved}`. Selection, alpha lock and target come from the adapter. `TypeKey legacy.pc.<id>` |
| EC `ec_api` 1/2 | v2 neighbours map to a timed plan of ±1 frame. `legacy.ec.<id>` |
| Beta `craft_abi` 1 | JSON `call` maps to `submit`, and raw ids resolve through `engine.lock` |
| EC `.jsx` | The `@craft/ec-script` object model and ScriptUI-to-Nodes run on asyncify `quickjs-script.wasm`. Synchronous calls and modal waits unwind to `Pending` and resume on `Completion`. Other events queue while the realm is suspended, and a causal cycle into it gets `Busy`. Headless `show()` returns cancel. `beginUndoGroup` maps to history coalescing. `Socket` is denied. A registered `.jsx` runs as its generated package's command contribution, a `Command` sink under chain intersection ([runtime contract](runtime.md#authority)). |

Shims stay while their fixtures pass. At `craft@3`, `craft@2` itself becomes a shim.

## Refactor simulation

Pinned guest binaries and one saved `.pcd` containing a guest Smart Filter, a recorded Action and a `pc_abi` 1 filter must survive three simultaneous PC changes:

| Change | App files | Plugin-platform files touched |
|---|---|---|
| Split command tables into owner crates and rename `filters_ext` ids, including one argument rename | `engine/src/install.rs`, `filters_ext.rs`, new owner crates with `renamed_from` | Regenerated `plugin-contract/engine.lock` with `migrate_args`; `plugin-adapter/src/ops/filter.rs` (driven by compile errors) |
| Move to a linear-f32 working space | `crates/color`, `engine/src/pixels*` | `plugin-adapter/src/{render,color}.rs` |
| Replace facade widgets | `crates/ui-egui` | None in PC; CL's Node renderer for the affected control kind |

`api.lock`, `pc.lock`, guests, SDK and fixtures stay unchanged. The test saves, refactors, reopens, re-renders against the pixel digest within tolerance, replays the Action and replays the guest transcript. Results, Undo and errors must match.

## Targets

**A** means available once its probe passes. **P** means pending: it ships as `Unavailable` until its test passes. **U** means unavailable.

| Capability | macOS | Win x64 | Win arm64 | Linux | Wayland | Web | Headless |
|---|---|---|---|---|---|---|---|
| Wasm guests, CPU kernels, formats, jobs | A | A | P (Pulley or x64 under Prism, whichever measures faster; `Available(Interpreted)` re-budgets) | A | A | P (broker Worker; PC keeps in-page wasmi until `WEB-broker`) | A |
| GPU processors | A | A | P | A | A | P (WebGPU in the broker Worker) | A with device |
| Panels, canvas, tools | A | A | P | A | A | P | U(Headless) |
| Native floating windows | A | A | P | A | `Presented(FloatInApp)` when positions are unreported | In window | U |
| HTML (CEF) | P | P | P | P, U without Landlock | P | U(WebTarget) | U |
| NativeConfined, exact triple | P | P | P | P | P | U | P |
| `.jsx` scripts | A | A | P | A | A | P until SCRIPT tests pass | A |

## Author workflow

| Command | Use |
|---|---|
| `craft new <panel\|edit\|effect\|kernel\|layer\|tool\|importer> --rust\|--ts` | Scaffold |
| `craft types` | Types from the installed app's locks or versioned lock packages |
| `craft dev --app pc` | Developer badge; epoch kept for user-linked folders on unchanged grants; the host keeps the last body; source-mapped traces from the worker |
| `craft test` | Fake host plus app-exported snapshots; fails on undeclared grants |
| `craft check` | Exact profile, imports, critical keys and levels |
| `craft replay` | Recorded transcripts under lldb or QuickJS DevTools, with no authority |
| `craft pack` | Canonical tree and digest |

Compile-time guards:
- `Has<Grant>` impls exist only for granted permissions, with `#[diagnostic::on_unimplemented]` naming the `craft.toml` fix. TypeScript gets `Cx<Granted>`.
- Level marker traits.
- Unit newtypes and branded TypeScript units.
- `#[craft::props]` and `defineProps(... as const)`.
