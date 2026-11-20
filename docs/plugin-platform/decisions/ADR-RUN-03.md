# RUN-03/04 and SDK-01 · Public SDK, evolution, targets and author workflow

Accepted with verification. The [SDK contract](../contracts/sdk.md) and the [seven reference examples](../contracts/sdk-examples.md) are designed. Spikes, fuzzing, conformance and per-target probes have not run.

## Decision

**One ABI.** Every guest is a core Wasm module speaking `craft@2`, with a pinned feature profile:

| Element | Content |
|---|---|
| Imports | `memory` (imported), `submit(op, args) -> ticket`, `read`, `write`, `log` |
| Exports | `craft_abi`, `craft_alloc`, `craft_turn -> Idle \| Pending` |

Every payload is one CBOR envelope under a strict profile. The host decodes it once on the broker thread. Anything the host hashes or persists, it re-encodes deterministically itself.

There are no WIT components. Component evolution would need a new function or major for every added field or error case, and on the web it would add JS glue running outside Wasm. Our own generator stays small because the schema subset is restricted.

**Rust types are the source of truth.** Shared domain types (`doc`, `render`, `time`, `io`, `job`, `ui`, `host`) live in the dependency-free crate `craft-libs/crates/plugin-api`. App-specific `pc.*` and `ec.*` types live in dependency-free crates in each app.

A single `#[craft::api]` macro emits schemars JSON Schema 2020-12 with `x-craft-*` annotations, and the patch, downlevel, mapper and builder code. It writes checked-in locks: `api.lock`, `<app>.lock`, which includes a `provides` section, and `engine.lock`. From those locks one generator produces:
- TypeScript bindings;
- docs;
- MCP and CLI descriptions;
- the fake test host;
- conformance cases.

Every validator, including the PC `param_check.rs`, the EC `cmd/schema.rs` and the handwritten beta `.d.ts`, is replaced by the one host validator.

Cross-app code uses Domain operations by default. An App operation needs a written `why_not_domain`, and guests detect it with `cx.contract::<pc::Filters>()` or `cx.has("pc.filter")`.

**Evolution.**
- Contract levels are `area@1.minor`, and each operation has its own major. Every published major is served forever through pure `up`/`down` mappers.
- A retired major is `Unavailable(Retired)` for that one operation only. Retiring needs CODEOWNERS approval plus a fixture showing the major cannot be mapped.
- Live calls are strict. Persisted decode is tolerant, migrated and diagnosed.
- Writes are key-addressed patches. Every guest commit is checked against the host-recorded read set, reusing IO-01's fingerprint mechanism at DOC-01 publication.
- Newer enum values arrive as `Other{raw}`. That value carries no authority, and the host validates and re-encodes it on write.
- A hidden, non-neutral semantic field blocks whole-object replacement, and blocks patches to the fields it `governs`.

**Persisted identity.**
- Every engine id ever shipped stays in `engine.lock` forever. Each entry records its first-shipped persisted id, flattened aliases, tombstones and an args-schema hash. Changing the hash requires `migrate_args` or `persistable=false`.
- One CL crate, `craft-idmap`, resolves all four alias tables: engine ids, public operation aliases, contribution aliases, and `legacy.<app>.<id>` keys.
- Saved PC Smart Filters, Actions, keymaps and toolbars, and EC effect ids, groups and journals, resolve through it. An unresolvable reference stays inert, with a diagnostic.

**Typed handlers are the access condition.** Guests may call an operation only when its handler is `fn(&mut Session, Args) -> Result<Out>`, where `Args` derives the schema. A codemod with a key-access trace drafts the `Args` types. The order is:
1. operations behind the examples;
2. operations behind the bundled EC `.jsx` scripts;
3. no-argument commands.

A CI ratchet only moves up. SDK-02 parity needs 100% of Domain and App leaves typed.

**Legacy families run through bundled shims over the one ABI.**

| Legacy family | Shim |
|---|---|
| PC `pc_abi` 1 | Pinned document-encoded `ImageDesc` |
| EC `ec_api` 1/2 | v2 neighbours become a timed plan |
| Beta `craft_abi` 1 | Converted onto the one ABI |
| EC `.jsx` | `@craft/ec-script` plus ScriptUI-to-Nodes, on an asyncify-instrumented `quickjs-script.wasm`. Each synchronous getter or modal `show()` unwinds to `Pending` and resumes on `Completion`, and "next UI event for window W" is a ticket kind. |

Script segments are ordinary turns, so budgets, cancel, revoke and `cx.state` stay intact. The same build runs on desktop, on the web without JSPI, and headless.

**Determinism.** `ExecProfile` carries `DeterminismClass`:
- `Canonical`: Cranelift or Pulley with NaN canonicalisation and no relaxed-SIMD;
- `Engine(id)`: browser or preview.

Caches and bakes never cross classes. Exact fork-join partitions depend only on the request. Exact Stores have no clock or entropy.

**Author workflow.** `craft new|types|dev|test|check|pack` covers the cycle.
- Permission mistakes fail at compile time through `Has<Grant>` marker impls generated from `craft.toml`. TypeScript uses `Cx<Granted>`.
- Dev re-admit keeps the epoch for user-linked folders.
- Debugging is deterministic replay. `craft dev` records turn transcripts, and `craft replay` runs the same module under lldb or QuickJS DevTools against a fake host, with no authority.
- Symbolication runs outside the host.

### Alternatives

| Alternative | Strongest argument | Why it lost |
|---|---|---|
| WIT components with wit-bindgen, jco and componentize-qjs | Standard IDL with generated bindings in many languages | Every added field or error case needs a new function or major, which is churn against "every major forever". jco glue runs outside Wasm on the web, and the toolchain is still moving. A restricted schemars profile gives typed bindings with one validator. |
| A home-grown derive and IDL (A1) | Full control | schemars already ships in PC automation and MCP. The single `#[craft::api]` macro keeps ownership of the patch, mapper and downlevel code without a second schema language. |
| "Engine renames create no alias" (A1) | Smaller lock | Broke saved Smart Filters, Actions and EC effects. That was B's BLOCKER. |
| Public ids stored in new records (B) | One vocabulary for guests | It gives two vocabularies per record, and older builds can't read them. Engine records keep their first-shipped engine ids. |
| A fiber `craft.wait` in a separate script Store (A2) | Low wait cost | It breaks turn budgets, the wall-clock backstop and the `cx.state` commit. It also opens re-entry through EC's nested modal loop and needs JSPI on the web. |

## Amendments applied from C

All twelve are in the contract and tests:
1. The compile run stays guest-free. `pc_manifest` and TS pre-init run in the Principal process, and the snapshot is only a memory image on one shared `quickjs.wasm`.
2. Asyncify script waits.
3. The strict CBOR profile, ticket cap and broker-thread decode.
4. `Other` mints no authority.
5. The read-set check on every commit, key-addressed list patches and `governs`.
6. The full `engine.lock` and `craft-idmap`.
7. The schema profile, `#[craft::api]` and `#[non_exhaustive]` types.
8. A measured typed-handler migration.
9. Determinism class.
10. Replay debugging.
11. CODEOWNERS retirement.
12. Honest Windows arm64 and web cells.

## Integration slices

| Slice | Owner and codepaths | Exit proof |
|---|---|---|
| IDS-NOW (phase 0, no plugin code) | `engine.lock` export and CI check in both apps. `resolve_persisted` on PC `doc/src/lib.rs:633–641` Smart Filters, `actions_record.rs`, keymaps and toolbars, and on EC `project/src/layer.rs:199–203` effect ids and journals. No engine-id rename merges before this lands. | `IDMAP-props`, `SDK-save-refactor-reopen` (args rename) |
| API-CORE | CL `plugin-api`, `#[craft::api]`, `sdk-gen`, R3 prescan, `craft-idmap` | `SDK-idl-spike`, `SCHEMA-equiv`, `ENV-fuzz`, `SDK-source-compat` |
| TYPED | PC `registry.rs` handlers to typed functions; EC `cmd!`/`query!` typed arm (`cmd.rs:122–157`) | `SDK-typed-ratchet`, descriptor cases per operation |
| HOST-DESKTOP | Host validator, mappers, conformance kit, author CLI | `SDK-examples-7`, `EDIT-lost-update`, `SDK-downlevel-rmw`, `SDK-major-forever`, `WASM-profile`, `RUN-cache-isolation` |
| LEGACY | Shim packages. Delete the PC `crates/plugins` and EC `plugin/src/wasm.rs` loaders afterwards. | `SDK-legacy-retire`, `SDK-save-refactor-reopen` |
| SCRIPT | Asyncify `.jsx` shim, then delete boa | `SCRIPT-ec-suite`, bundled ScriptUI panels, `SCRIPT-reentry`, `SCRIPT-perf` on desktop, web and headless |
| WEB, WIN-ARM64 | Broker Worker backend. PC keeps in-page wasmi until `WEB-broker` passes. Pulley versus x64-under-Prism measurement. | `WEB-broker`, `WASM-winarm64`, `NET-matrix` |

## Question resolution

| Question | Resolution | Section |
|---|---|---|
| RUN-03.1 | Locks, levels, majors forever, `craft-idmap` | Evolution and identity |
| RUN-03.2 | One host validator. The adapter binds typed handlers and owns semantics only. | One validator |
| RUN-03.3 | `sdk-gen diff`, the ratchet and the conformance kit | |
| RUN-03.4 | Static `requires` against the `provides` section, plus run-time `host.capabilities()` facts with no app, OS or vendor names | |
| RUN-03.5 | The refactor simulation | [contract](../contracts/sdk.md#refactor-simulation) |
| RUN-04.1 | The target matrix | |
| RUN-04.2 | Headless has the same workers and no UI turns. On the web, a broker Worker. Wayland floats inside the app. | |
| RUN-04.3 | HTML is CEF-only and `Unavailable` on the web | ADR-RUN-01 |
| RUN-04.4 | Exact-triple native members behind probes | |
| SDK-01.1 | The seven examples | |
| SDK-01.2 | Rust and TS are generated from one lock; a TS kernel is supported but slow | |
| SDK-01.3 | Shims and mappers | |
| SDK-01.4 | Compile-time grant markers and `craft check` | |

The Q, A1, B, A2 and C review roles ran in order. Every role and the lead ran Claude Opus 5.5 at high effort, each in a separate context. A1 and A2 are the same agent. No role ran on the GPT family.
