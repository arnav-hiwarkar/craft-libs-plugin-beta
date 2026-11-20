# Host source inventory

Revisions are in [source baselines](source-baselines.md). These are source observations. Tests cited here are test targets, with execution still pending.

| ID | Source path and line | Observation | Class |
|---|---|---|---|
| PC-UI-1 | `photocraft@359051359:crates/ui-egui/src/app/framework.rs:62,285,682`; `app/frame.rs:231–261` | Installs the shell once, routes input, renders the workspace inside the shell, and runs dialogs over it. | VERIFIED SOURCE |
| PC-UI-2 | `photocraft@359051359:crates/ui-egui/src/panels/panel_docking.rs:62,88,138–292,713–749,835` | A string key enters the canonical workspace. Panel spec and body use a static registry. Restore reconciles against it. Dock output includes native-window file drops, which go to app file-open. | VERIFIED SOURCE |
| PC-FW-1 | `craft-libs@bafb4522:crates/ui-patterns/src/patterns/dock/area.rs:52–84,115,144,352–449`; `crates/behavior/src/dock/workspace.rs:29–43,125–156,292–293` | Framework returns docking actions and focus/window reports. The app applies actions. Reconcile takes a registration predicate and bounds hidden entries. | VERIFIED SOURCE ([dock area](https://raw.githubusercontent.com/storytold/craft-libs/bafb4522899d9f814a07dad636cdefcbf705dead/crates/ui-patterns/src/patterns/dock/area.rs), [workspace](https://raw.githubusercontent.com/storytold/craft-libs/bafb4522899d9f814a07dad636cdefcbf705dead/crates/behavior/src/dock/workspace.rs)) |
| EC-UI-1 | `effectcraft@f32ad3aa:crates/ui-egui/src/dock.rs:17–45`; `dock_ui.rs:197–247,275,388,429–532,825–839`; `panels.rs:294` | Legacy shared docking uses a typed panel key. Script panels have process-local IDs, specialized close behavior and synchronous script events. Native window IME is handled in the app adapter. | VERIFIED SOURCE |
| EC-UI-2 | `effectcraft@f32ad3aa:crates/ui-egui/src/panels/scriptui_view.rs:516–519,544,590` | Script palettes and dialogs use embedded egui windows. Docked bodies follow the workspace callback. | VERIFIED SOURCE |
| FW-UI-1 | `craft-libs@e6dbc97ca:crates/ui/src/docking/render/native.rs:1–8,331–355` | Main-line dock has native windows for eligible panels if embedding and window-system policy permit it. This is separate evidence from the PhotoCraft pin. | VERIFIED SOURCE |
| KIT-UI-1 | `craft-ui-port-kit@27e5ff34:rules/framework-decisions.md`, Panels and docking, The pass model and Shared engines; `rules/porting-rules.md`, Look and Code | One dock, column collapse, pass-local input, shared IME and theme roles. Add framework gaps with contracts and stories. | VERIFIED DOCUMENT |

## Baseline gaps and source routes

- EffectCraft operation and edit proof should follow the real session command and project history path. The render audit starts at `crates/effects/src/plugin.rs:698` and `crates/plugin/src/wasm.rs`, whose module documentation distinguishes both legacy ABIs. Treat older reports of no temporal work as statements about the old shared beta, with EffectCraft current main assessed separately.
- PhotoCraft programmatic entry points are `crates/ui-egui/src/control/dispatch.rs:51,133–162`. RPC has UI and host methods as well as engine methods. Calling the whole RPC dispatcher under a plugin credential would import unrelated authority. Pixel and provenance proof should follow `crates/plugins/src/{apply,runtime,registry}.rs`, `crates/engine/src/session/history.rs` and `crates/compose/src/viewport.rs`.
- Beta code at the pinned HEAD includes both `ext-host` and `ext-runtime`/`ext-worker`. Old status documents describe earlier verification and do not imply that HEAD implements every v3 or lean-plan decision. Audit real callers before reuse. Candidate reuse includes manifest/ID/schema, bounded validator, generated vocabulary, SDK ergonomics, and offline fixture data.
- Beta `ext-ui-fw/src/engine.rs:225` has a framework engine. Historical framework notes describe its egui fallback and renderer selection. Use the node and event fixtures. New apps mount their panel bodies through their single framework dock.

## External functional references

- Region, clip, parameter and render actions are useful precedents for the processor contract. [OpenFX reference 1.5.1](https://openfx.readthedocs.io/en/latest/Reference/)
- WIT describes types, interfaces, worlds and resources. Transport and execution still need separate decisions. [Component Model WIT reference](https://component-model.bytecodealliance.org/design/wit.html)
- egui documents `request_discard` and multiple passes. The pinned fork and port-kit rules control input handling here. [egui Context reference](https://docs.rs/egui/0.37.0/egui/struct.Context.html)

## Corrections to historical assumptions

| ID | Source path and line | Observation |
|---|---|---|
| PC-RENDER-1 | `photocraft@359051359:crates/engine/src/cmd/plugin_cmds.rs:105–159` | `plugin.run` can write target channel/raster pixels or record a `SmartFilter` on a Smart Object. Recorded params contain legacy ID and resolved values. Evaluation skips missing/failing implementations by returning `None`. Preserve both paths on migration and test visible missing/fault state plus exact export. |
| BETA-RUN-1 | `craft-extensions@223e52fe:crates/ext-runtime/src/launch/mod.rs:31,88–101`; `crates/ext-runtime/src/gate/mod.rs:116` | HEAD already has a newer launch/runtime gate beside the previous `ext-host` implementation. Its `Native` path differs from its `Worker` path. A worker process alone proves no denial property. |
| EC-RENDER-1 | `effectcraft@f32ad3aa:crates/plugin/src/wasm.rs:1`; `crates/effects/src/plugin.rs:698`; `crates/project/src/layer.rs:61,196,598` | Candidate routes for preserving ABI 1/2, render stages and first-class extension layer sources. ABI equality across beta/main cannot be inferred from the numbers. |

## Further app integration anchors

| Domain | EffectCraft | PhotoCraft |
|---|---|---|
| Operations and edit | `crates/engine/src/cmd.rs:106,175,281`, `session/dispatch.rs:9–66` and `session/edit.rs:10–105,142–178` | `crates/engine/src/session/execute.rs:17,101,249–375`, `session/history.rs:6–108`, `cmd.rs:169` |
| Render and assets | `crates/engine/src/session/render.rs:9–24,73–89,189–228`, `crates/render/src/audio.rs:50,369–424`, `crates/gpu/src/fx_time.rs:192` | `crates/compose/src/render.rs:27–50,196–263`, `crates/engine/src/cmd/smart_cmds/rendering.rs:132–184,487–570`, `stack_cache.rs:40–88` |
| Jobs and document I/O | `crates/engine/src/render_queue.rs`, `settings/autosave.rs`, `crates/project/src/item.rs` | `crates/engine/src/jobs.rs:1064,1211–1236,1410–1456,1503,2044`, `cmd/file_cmds/open.rs:175–200`, `cmd/file_cmds/save.rs:245–292`, `crates/io/src/smart_map/export.rs:290,364` |

## Document and authority seams to test

- EffectCraft `crates/engine/src/cmd/file.rs:41–102` opens a repaired JSON/XML project, resolves relative footage, and warns that unknown newer settings are lost on save. The new extension envelope requires an older-reader overwrite policy that actually protects bytes. Preserve repair and footage behavior for ordinary built-ins.
- PhotoCraft `crates/engine/src/cmd/file_cmds/save.rs:245–295` passes the document path to the encoder and suppresses linked-file embedding under an untrusted session authorization. Existing save paths write after encode. Carry equivalent protection through brokered file targets and background snapshot export.
- PhotoCraft `crates/engine/src/cmd/actions_cmds/history.rs:20–69` captures active document, target, history and clean state at edit time. It also resets coalescing. A detached stage must neither trigger ambient history capture nor inherit the wrong document's settings.
- Beta `ext-runtime/src/gate/mod.rs:116–178` validates and checks a class-derived permission set for `app.run` and expands nested batch steps. This is evidence for a centralized gate, not evidence that the operation has a bound document incarnation and correctly resolved leaf targets.
- Beta `ext-runtime/src/launch/mod.rs:77–161` uses an OS-specific confinement and self-test path for `Worker` and `Host`, and direct launch for `Native`. The test proves only the specific operations probed. Portable claims need target evidence and confinement of indirect host effects.

## Persistence anchors

- EffectCraft `crates/project/src/item.rs:370–420` checks schema and file values on native write. The literal readback in this path runs in debug builds. Open decodes the closed `Project` before repair. XML open is normalized in `cmd/file.rs`. An unsupported new tagged variant or new schema can be made to refuse safely on old readers. Unknown optional JSON fields can be discarded by old writers.
- PhotoCraft native state uses `crates/format/src/convert.rs:126–194,480–564` to encode and decode known layer variants. It preserves Smart Object source, filter parameters, filter masks and cached appearance. `format/src/store.rs:305–329,384–416` validates manifest limits and verifies content-addressed blobs within load budgets. `format/src/import_conversion.rs:51–93` already carries newer opaque data for a separate conversion field. That precedent does not establish lossless new plugin state in the current format.
- The old beta `ext-core/src/project.rs:19–62` distinguishes a package reference, individual contribution, state version and missing/disabled status. Reuse the concept after reconciling digest lineage, asset closure and opaque payload ownership.
- `FeatureRef` stores params and state as `serde_json::Value`. The comment promises byte-for-byte handling. This representation can preserve JSON value meaning. It cannot preserve the original byte encoding, ordering and formatting without a separate raw field. The new file contract needs an explicit byte payload for opaque state.
- EffectCraft's background autosave at `crates/engine/src/settings/session_settings.rs:361–386` serializes a captured project with `to_json_compact`, a different path from checked native file save. Extend the recovery format path in the same persistence slice so an interrupted operation cannot leave an incomplete saved extension envelope.
- PhotoCraft's exact framework format implementation at `bafb4522` rejects manifest versions higher than the caller's target before model conversion. This is verified pinned source. A real prior-build open/save test remains required. [Reader call](https://github.com/storytold/craft-libs/blob/bafb4522899d9f814a07dad636cdefcbf705dead/crates/pcraft-format/src/read.rs#L26-L39) · [Version comparison](https://github.com/storytold/craft-libs/blob/bafb4522899d9f814a07dad636cdefcbf705dead/crates/pcraft-format/src/migrate.rs#L19-L35)

## Processor data and scheduler facts

- EffectCraft passes top-down, premultiplied RGBA f32 with scale and origin across its legacy effect code. It allocates and copies the Wasm image and parameter arrays. ABI 2 also supplies neighboring frames. The renderer has separate on-device display and CPU/YUV export routing. Exact output needs completion checks on each. Pinned processor trace (RENDER-01 Q record)
- PhotoCraft uses sparse copy-on-write native tiles and converts bands for its legacy Wasm. Existing preview damage controls updates. One current GPU export path rejects U16/F32 sources to preserve native precision, so a nonempty GPU provider list alone is not the capability predicate. Pinned PhotoCraft trace (RENDER-01 Q record)
- Committed beta `ext-gpu/src/{contract,guard,gpu}.rs` validates bindings, inserts loop guards and checks queue waits. A timed wait reports an overdue submission. It does not prove cancellation of the GPU operation. Establish performance and failure boundaries from new provider tests, including exact copy and fence work. Pinned beta evidence (RENDER-01 Q record)

## Timed evaluation facts

EffectCraft current main uses rational frame rates and integer ticks. Legacy Wasm ABI 2 samples neighbors on the layer grid with availability flags. Native render effects can read scoped input prefixes at other times, and current project dirty spans conservatively widen for unknown time reads. Native playback has a lock and scratch growth in the callback path, while immutable-snapshot mixing already happens in a feeder. Offline export requests sample ranges separately. PhotoCraft current main has stored video frames and a frame timeline, with frame navigation updating displayed raster. It has no equivalent track/audio provider at these codepaths. A plugin sampler must resolve the captured editable frame without moving the user's playhead. Pinned cases (RENDER-03 Q record)
