# Question closure

Each row is a requirement from the question bank. Record exact contract and acceptance IDs on closure. Tests are planned unless marked with an execution record.

## SYS-01 · What belongs in a lean core versus an extension?

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| SYS-01.1 | 8 | Which functions must live in the host to make plugins fast and safe (document ownership, renderer scheduler, undo, UI framework, I/O broker)? Which high-level first-party features can truly move into plugins without a privileged API shortcut? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`RUN-first-party`, `RUN-oversub`](implementation/runtime-acceptance.md) |
| SYS-01.2 | 9 | Do startup, offline and missing-plugin workflows still work when an optional first-party feature is absent? Can users uninstall/disable a formerly built-in feature safely? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`RUN-identity`, `RUN-replay-scope`](implementation/runtime-acceptance.md) |
| SYS-01.3 | 10 | What *minimal* common extension primitives unlock maximum classes of features, without exposing private Rust fields and leaking internal version churn? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`RUN-sink`, `RUN-classify-all`](implementation/runtime-acceptance.md) |
| SYS-01.4 | 11 | Which existing beta/app systems can be preserved as generic infrastructure; which plugin-specific code must be replaced or deleted? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`RUN-policy-diff`, `RUN-first-party`](implementation/runtime-acceptance.md) |

## SYS-02 · Extensibility semantics and discovery

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| SYS-02.1 | 14 | What is a package, contribution, type, instance, runtime worker, resource, document dependency and permission principal? Are identities stable across app instances, save/load, import/clone and SDK updates? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`RUN-identity`, `RUN-paste-prov`](implementation/runtime-acceptance.md) |
| SYS-02.2 | 15 | Can plugins register a new feature class without recompiling the app? Which classes require host contracts to be implemented ahead of time? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`RUN-identity`, `RUN-first-party`](implementation/runtime-acceptance.md) |
| SYS-02.3 | 16 | Can plugin A depend on or communicate with plugin B? How avoid cross-plugin authority escalation, cycles and version conflicts? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`T-service-grief`, `RUN-service-xflow`, `T-credit-deadlock`](implementation/runtime-acceptance.md) |
| SYS-02.4 | 17 | How does the host advertise capabilities honestly, including CPU/GPU features, 2D/3D/time, platform constraints, headless support, limits and optional contract extensions? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`NET-matrix`, `GPU-software-parity`](implementation/runtime-acceptance.md) |

## UI-01 · Docked/floating/native windows

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| UI-01.1 | 22 | Trace current PhotoCraft craft-framework docks/windows and EffectCraft `craft_ui::docking`: how does one dynamically registered panel join the same workspace, tab group, resize, multi-monitor OS window, restore layout, command palette and focus cycle as built-ins? | [Decision](decisions/ADR-UI-01.md) · [Contract](contracts/ui.md) · [`UI01-workspace`, platform](implementation/ui-01-acceptance.md) |
| UI-01.2 | 23 | Who owns panel IDs, lifecycle, persistence, tab titles/icons, close events, modal/tooltip/popup focus, keyboard accelerators, accessibility roles and localization? What happens if plugin worker dies with panels open? | [Decision](decisions/ADR-UI-01.md) · [Contract](contracts/ui.md) · [`UI01-workspace`, modal, a11y, recovery, save](implementation/ui-01-acceptance.md) |
| UI-01.3 | 24 | How do panels survive host UI framework changes without SDK recompile: shared semantic retained-tree IR versus callback rendering, versioned fallbacks, renderer independence? | [Decision](decisions/ADR-UI-01.md) · [Contract](contracts/ui.md) · [`UI01-a11y`, frame-cost](implementation/ui-01-acceptance.md) |
| UI-01.4 | 25 | Does separate-window implementation work on macOS, Windows, Wayland and web? What are honest fallback behaviors? | [Decision](decisions/ADR-UI-01.md) · [Contract](contracts/ui.md) · [`UI01-platform`](implementation/ui-01-acceptance.md) |
| UI-01.5 | 26 | Can a plugin supply 2+ panels, a workspace, utility palette, modal dialog, custom inspector and detachable full-screen view, all without introducing another docking engine? | [Decision](decisions/ADR-UI-01.md) · [Contract](contracts/ui.md) · [`UI01-workspace`, a11y, platform](implementation/ui-01-acceptance.md) |
| UI-01.6 | 27 | How to register/replace a built-in panel through safe extension slots while protecting permissions, file dialogs and trusted system UI? | [Decision](decisions/ADR-UI-01.md) · [Contract](contracts/ui.md) · [`UI01-trust`, modal](implementation/ui-01-acceptance.md) |

## UI-02 · Rich native-looking components and web/custom surface

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| UI-02.1 | 30 | Native semantic components versus arbitrary custom canvas versus GPU view versus HTML webview: choose boundaries, ownership, fallback and accessibility. Do not make HTML/webview mandatory for everyday UI. | [Decision](decisions/ADR-UI-02.md) · [Contract](contracts/ui-content.md) · [`UI02-mixed`, html, device](implementation/ui-02-acceptance.md) |
| UI-02.2 | 31 | How are stable widget IDs, tree revisions, focus, IME, undo gesture state, high-DPI scaling, theme tokens, translations, complex tables/tree views and virtualized thousands-row lists represented? | [Decision](decisions/ADR-UI-02.md) · [Contract](contracts/ui-content.md) · [`UI02-update`, invalidation, collections](implementation/ui-02-acceptance.md) |
| UI-02.3 | 32 | How do UI updates avoid full reserialization and full tree rebuild? Compare retained keyed patches, property updates, host-local state and cache correctness. What is the frame-time impact? | [Decision](decisions/ADR-UI-02.md) · [Contract](contracts/ui-content.md) · [`UI02-update`, cost](implementation/ui-02-acceptance.md) |
| UI-02.4 | 33 | Can authors design meaningful dashboards/node editors/3D viewports, not merely buttons and sliders? What is the escape hatch for bespoke visual content without arbitrary code on UI thread? | [Decision](decisions/ADR-UI-02.md) · [Contract](contracts/ui-content.md) · [`UI02-canvas`, a11y via mixed](implementation/ui-02-acceptance.md) |
| UI-02.5 | 34 | Does the plugin UI observe theme/appearance/contrast changes automatically, including PhotoCraft's framework and future EffectCraft port? | [Decision](decisions/ADR-UI-02.md) · [Contract](contracts/ui-content.md) · [`UI02-mixed`, invalidation](implementation/ui-02-acceptance.md) |

## UI-03 · Input / custom tools / canvas overlays

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| UI-03.1 | 37 | How do plugins register paint/selection/move/vertex/3D-gizmo tools, custom cursors, hover/hit-testing, snapping, stylus pressure/tilt and gesture capture? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-capture`](implementation/edit-acceptance.md) |
| UI-03.2 | 38 | What state stays inside host on the hot path, what messages are async, and what user interaction must complete within a frame? How do slow workers avoid pointer lag? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-capture`, cancel](implementation/edit-acceptance.md) |
| UI-03.3 | 39 | How do drag previews, interaction transactions, captured revisions, Escape/cancel, modifier keys, one-pass events and multi-pass rerenders work correctly? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-cancel`, atomic](implementation/edit-acceptance.md) |
| UI-03.4 | 40 | How can a plugin draw overlays in the composition viewer/PhotoCraft canvas without modifying compositor source pixels or cross-document leaking? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-overlay`](implementation/edit-acceptance.md) |
| UI-03.5 | 41 | How do tools behave across multiple documents/viewports/native windows, magnification, DPI, focus loss and headless mode? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-capture`, target](implementation/edit-acceptance.md) |

## UI-04 · Commands, menus, shortcuts, hooks and replacement

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| UI-04.1 | 44 | Can a plugin contribute menu groups/context menus, toolbar slots, effect control widgets, shortcuts, inspector pages and searchable commands with stable identity? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-hook`](implementation/edit-acceptance.md) |
| UI-04.2 | 45 | Can a plugin *replace*, wrap or intercept default behavior? How do priorities, user opt-in, conflict resolution, deterministic ordering, auditing, and rollback work without infinite loops or security spoofing? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-hook`](implementation/edit-acceptance.md) |
| UI-04.3 | 46 | What event types exist: selection changes, document open/save, layer edits, playback tick, frame invalidated, active tool changed, render started/completed? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-events`](implementation/edit-acceptance.md) |
| UI-04.4 | 47 | Which events are ordered/transactional, and which are advisory/coalesced? How prevent reentrant edits and event storms? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-events`, hook](implementation/edit-acceptance.md) |
| UI-04.5 | 48 | Can first-party plugin features use identical hooks and permissions instead of private bypasses? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-hook`, atomic](implementation/edit-acceptance.md) |

## DOC-01 · Full document query/edit API

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| DOC-01.1 | 53 | Inventory CLI/MCP/engine commands, but also examine missing *typed high-volume queries* and capability-specific operations. Should reads be snapshot, indexed query, projections, stream, handle or object proxy? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-bulk`](implementation/edit-acceptance.md) |
| DOC-01.2 | 54 | How are target authorization and capture scope evaluated after resolving explicit IDs/paths? What happens when another document becomes active mid-call? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-target`, bulk](implementation/edit-acceptance.md) |
| DOC-01.3 | 55 | What guarantees are needed for batched edits, revision/optimistic checks, undo labels, rollback, preview edits, atomic visibility, nested commands and post-commit events? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-atomic`, editor](implementation/edit-acceptance.md) |
| DOC-01.4 | 56 | How do huge operations (10k layers, million vector points) avoid per-element IPC, serializing whole documents and blocking UI threads? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-bulk`](implementation/edit-acceptance.md) |
| DOC-01.5 | 57 | What contract remains stable when app internal Rust structs change next month? Can registry-derived SDK bindings be generated and compatibility-checked automatically? | [Decision](decisions/ADR-UI-03.md) · [Contract](contracts/operations.md) · [`EDIT-bulk`, hook](implementation/edit-acceptance.md) |

## DOC-02 · New persistent objects and non-destructive features

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| DOC-02.1 | 60 | Preserve the accepted decision: plugins can extend existing built-ins **and** register entirely new first-class persistent object types. What lives in host vs plugin state, and how are versions/unknown bytes handled? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-object`, roundtrip](implementation/document-acceptance.md) |
| DOC-02.2 | 61 | EffectCraft `LayerSource` versus PhotoCraft `LayerContent`: where does generic extension node fit without massive branching, duplicate document trees or runtime-dependent project loading? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-object`, invalidate](implementation/document-acceptance.md) |
| DOC-02.3 | 62 | Are generic slots enough for virtual children, custom effect stacks, procedural geometry, nested scenes, custom timeline tracks? How are render graph and object dependencies declared? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-object`, invalidate](implementation/document-acceptance.md) |
| DOC-02.4 | 63 | How do copy/paste, duplicate, import/export, undo, selection, transform, serialization, missing plugin, rollback, and exact baked output work? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-transfer`, missing](implementation/document-acceptance.md) |
| DOC-02.5 | 64 | What happens if the plugin is missing or newer/older, state migration fails, or the author deletes a type? Can the host save the opaque state untouched? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-roundtrip`, missing](implementation/document-acceptance.md) |

## DOC-03 · State, assets, project portability

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| DOC-03.1 | 67 | What is per-project persistent plugin state vs per-user app settings vs ephemeral worker runtime state vs cached results? How are identities and revisions tied to each? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-roundtrip`](implementation/document-acceptance.md) |
| DOC-03.2 | 68 | Where can large binary assets live: content-addressed data, deduplication, copy-on-write, external file references, temporary cache? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-assets`](implementation/document-acceptance.md) |
| DOC-03.3 | 69 | How does a reopened project resolve plugin-dependent features without making the app crash/block? Which exact final outputs can be rendered without the plugin, and which must fail explicitly? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-missing`](implementation/document-acceptance.md) |
| DOC-03.4 | 70 | What are project migration and downgrade rules; how do you prevent an older host from destroying unknown data? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-roundtrip`](implementation/document-acceptance.md) |
| DOC-03.5 | 71 | How do document histories, collaboration (future), autosave, background export and multi-document context handle extension objects? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-transfer`, assets](implementation/document-acceptance.md) |

## DOC-04 · Property/animation/expressions

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| DOC-04.1 | 74 | Can plugin-defined properties have stable IDs, typed units, ranges, keyframes, interpolation, expressions, links, dependencies and inspector bindings? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-properties`](implementation/document-acceptance.md) |
| DOC-04.2 | 75 | Can an EffectCraft extension animate full new structures without being constrained to a list of `f64` slider parameters? Which values need special serialization and interpolation? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-properties`](implementation/document-acceptance.md) |
| DOC-04.3 | 76 | Can PhotoCraft plugins expose live editable parameter groups and adjustment/Smart Filter stacks rather than destructive replacements? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-properties`, legacy](implementation/document-acceptance.md) |
| DOC-04.4 | 77 | How do property changes invalidate only affected frames/tiles, and how are expression dependency cycles detected? | [Decision](decisions/ADR-DOC-02.md) · [Contract](contracts/document.md) · [`DOC-invalidate`](implementation/document-acceptance.md) |

## RENDER-01 · CPU effects and compositing

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| RENDER-01.1 | 82 | What are host-owned image/audio/buffer handles, image formats/color spaces, alpha conventions, ICC/linear/HDR semantics, mipmaps and signed/float pixels? | [Decision](decisions/ADR-RENDER-01.md) · [Contract](contracts/render.md) · [`RND-format`](implementation/render-acceptance.md) |
| RENDER-01.2 | 83 | How do plugins request regions-of-interest, halo/padding, tiled/banded processing, multi-input masks and metadata? Can transforms be tile-invariant or explicitly full-frame? | [Decision](decisions/ADR-RENDER-01.md) · [Contract](contracts/render.md) · [`RND-tile`, full](implementation/render-acceptance.md) |
| RENDER-01.3 | 84 | When can host copy avoidance be proven for process IPC, WASM memory, shared memory and GPU buffer transfer? Who owns buffers and leases under cancel/crash? | [Decision](decisions/ADR-RENDER-01.md) · [Contract](contracts/render.md) · [`RND-authority`, cost](implementation/render-acceptance.md) |
| RENDER-01.4 | 85 | What is the render graph dependency model, cache invalidation and output quality policy: live preview versus exact export? | [Decision](decisions/ADR-RENDER-01.md) · [Contract](contracts/render.md) · [`RND-cache`, authority](implementation/render-acceptance.md) |
| RENDER-01.5 | 86 | How are deterministic results and cross-platform parity checked without forbidding legitimate stochastic-but-seeded effects? | [Decision](decisions/ADR-RENDER-01.md) · [Contract](contracts/render.md) · [`RND-tile`, cache](implementation/render-acceptance.md) |

## RENDER-02 · GPU, geometry and shaders

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| RENDER-02.1 | 89 | Which plugin features require textures, custom GPU compute, procedural geometry, programmable shaders, render passes, 3D depth/light information or access to advanced backend features? | [Decision](decisions/ADR-RENDER-01.md) · [Contract](contracts/render.md) · [`RND-gpu`, 3d](implementation/render-acceptance.md) |
| RENDER-02.2 | 90 | Should host schedule validated WGSL, trusted native compute helpers, hybrid CPU/GPU jobs, or all? What exact privileges and synchronization/copy costs exist? | [Decision](decisions/ADR-RENDER-01.md) · [Contract](contracts/render.md) · [`RND-gpu`](implementation/render-acceptance.md) |
| RENDER-02.3 | 91 | Can plugins request resource pools, intermediate surfaces, kernels and passes without obtaining raw wgpu device pointers? How handle GPU loss, shader hangs, OOM, invalid binding and cache reuse? | [Decision](decisions/ADR-RENDER-01.md) · [Contract](contracts/render.md) · [`RND-gpu`, authority](implementation/render-acceptance.md) |
| RENDER-02.4 | 92 | How do render timelines, geometry/3D scene bounds, depth buffers, color management and downstream compositing compose with the apps' existing pipelines? | [Decision](decisions/ADR-RENDER-01.md) · [Contract](contracts/render.md) · [`RND-scene-3d`, format](implementation/render-acceptance.md) |
| RENDER-02.5 | 93 | Specify honest latency/resource benchmarks for 1080p, 4K, multi-layer composites and large photos. Don't claim GPU execution is completely isolateable by shader validation. | [Decision](decisions/ADR-RENDER-01.md) · [Contract](contracts/render.md) · [`RND-cost`](implementation/render-acceptance.md) |

## RENDER-03 · Temporal, simulation and timeline

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| RENDER-03.1 | 96 | What time domain is used (composition ticks, source time, frames, subframes, speed maps, reversed layers), and what is immutable input at each requested time? | [Decision](decisions/ADR-RENDER-03.md) · [Contract](contracts/time.md) · [`TIME-map`](implementation/time-acceptance.md) |
| RENDER-03.2 | 97 | How do plugins sample past/future frames, optical flow, motion blur, temporal neighborhoods, cache/stateful simulation and deterministic re-render after seeking? | [Decision](decisions/ADR-RENDER-03.md) · [Contract](contracts/time.md) · [`TIME-neighbors`, `TIME-state`](implementation/time-acceptance.md) |
| RENDER-03.3 | 98 | How is dependency reach declared so EffectCraft invalidates correct frame intervals without invalidating the whole project unnecessarily? | [Decision](decisions/ADR-RENDER-03.md) · [Contract](contracts/time.md) · [`TIME-invalidate`](implementation/time-acceptance.md) |
| RENDER-03.4 | 99 | How to prevent recursive rendering/dependency cycles and unbounded frame requests? What is the deadline/cancel strategy during scrub versus final export? | [Decision](decisions/ADR-RENDER-03.md) · [Contract](contracts/time.md) · [`TIME-invalidate`, `AUD-exact`](implementation/time-acceptance.md) |
| RENDER-03.5 | 100 | Can a plugin add custom timeline tracks, procedural clips, transition types or effect controls without duplicating the editor's timeline logic? | [Decision](decisions/ADR-RENDER-03.md) · [Contract](contracts/time.md) · [`TIME-edit`](implementation/time-acceptance.md) |

## RENDER-04 · Audio and real-time constraints

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| RENDER-04.1 | 103 | If audio is in near-term scope: what contracts for buffers, sample rate, format, latency, lookahead, automation and clip access are supported by EffectCraft? | [Decision](decisions/ADR-RENDER-03.md) · [Contract](contracts/time.md) · [`AUD-format`](implementation/time-acceptance.md) |
| RENDER-04.2 | 104 | Are realtime DSP callbacks allowed? How avoid allocation, locks, runtime traps and IPC on the realtime thread; can plugins use compiled prevalidated graphs plus bounded buffers? | [Decision](decisions/ADR-RENDER-03.md) · [Contract](contracts/time.md) · [`AUD-device`](implementation/time-acceptance.md) |
| RENDER-04.3 | 105 | How are plugin crashes/glitches isolated and how is silence/last-good fallback selected for playback versus offline export? | [Decision](decisions/ADR-RENDER-03.md) · [Contract](contracts/time.md) · [`AUD-device`, `AUD-exact`](implementation/time-acceptance.md) |
| RENDER-04.4 | 106 | Is an app without audio capabilities able to reject this contribution cleanly while accepting the same package's image contribution? | [Decision](decisions/ADR-RENDER-03.md) · [Contract](contracts/time.md) · [`TIME-edit`](implementation/time-acceptance.md) |

## RENDER-05 · Local AI and background compute

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| RENDER-05.1 | 109 | Can a plugin request local model inference/segmentation/denoising/tracking without network or filesystem escape? Who controls model assets, CPU/GPU resources, cancellation and priority? | [Decision](decisions/ADR-IO-01.md) · [Contract](contracts/io-jobs.md) · [`JOB-infer`, gpu-fair, gpu-tdr, model-bomb](implementation/io-acceptance.md) |
| RENDER-05.2 | 110 | How do large jobs use immutable snapshots and revision-checked result commits; what happens if document changes during computation? | [Decision](decisions/ADR-IO-01.md) · [Contract](contracts/io-jobs.md) · [`JOB-aba`, lost-update, readset-capture, save-during](implementation/io-acceptance.md) |
| RENDER-05.3 | 111 | Can a plugin register a new worker/job graph that streams progress, intermediate previews, batch processes files and resumes or aborts safely? | [Decision](decisions/ADR-IO-01.md) · [Contract](contracts/io-jobs.md) · [`JOB-resume`, scratch-batch, cancel-race, revoke](implementation/io-acceptance.md) |
| RENDER-05.4 | 112 | How do memory/resource reservations prevent 5 heavy plugins from starving the host, and can the app remain interactive? | [Decision](decisions/ADR-IO-01.md) · [Contract](contracts/io-jobs.md) · [`JOB-fair`, oom, `IO-disk-full-autosave`](implementation/io-acceptance.md) |

## IO-01 · Local files, importers/exporters and media

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| IO-01.1 | 115 | How can plugins import/export codecs, LUTs, brush assets, vector/scene/3D data, generate files and read user-selected folders with **no raw path privileges**? | [Decision](decisions/ADR-IO-01.md) · [Contract](contracts/io-jobs.md) · [`IO-pick`, protected, format-open-save](implementation/io-acceptance.md) |
| IO-01.2 | 116 | How are native OS file pickers, opaque scoped handles, read/write policies, symlink races, size/zipbomb limits, temporary staging and atomic write/publish modeled? | [Decision](decisions/ADR-IO-01.md) · [Contract](contracts/io-jobs.md) · [`IO-link`, swap-mtime, crash, crash-exchange, placeholder-pick](implementation/io-acceptance.md) |
| IO-01.3 | 117 | How can plugins add custom file formats without bypassing the app's undo/history/import validation or running native parsers in-process? | [Decision](decisions/ADR-IO-01.md) · [Contract](contracts/io-jobs.md) · [`IO-parser-confine`, format-open-save, format-roundtrip](implementation/io-acceptance.md) |
| IO-01.4 | 118 | What is the policy for export that cannot preserve new plugin object types? Include explicit baking/flattening/loss reports, not silent data loss. | [Decision](decisions/ADR-IO-01.md) · [Contract](contracts/io-jobs.md) · [`IO-export-loss`, format-roundtrip](implementation/io-acceptance.md) |
| IO-01.5 | 119 | Can importing asset files indirectly trigger networking via URL references, nested media loaders, shell hooks or decoder processes? Threat model. | [Decision](decisions/ADR-IO-01.md) · [Contract](contracts/io-jobs.md) · [`IO-parser-confine`, remote, placeholder-pick](implementation/io-acceptance.md) |

## RUN-01 · Runtime execution model and isolation

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| RUN-01.1 | 124 | Evaluate WASM runtime/ABI/component options, JS host, optional native helpers and browser worker differences using actual supported features/benchmarks. What module lifecycle/cold-start and worker-per-package vs worker-per-contribution costs exist? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`RUN-cost`, `RUN-cost-js`](implementation/runtime-acceptance.md) |
| RUN-01.2 | 125 | Does a sandbox support async, events, UI, rendering and state management without synchronous re-entry? Define timeout/cancellation/trap handling. | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`T-hol`, `T-cpu-vs-wall`, `T-hook-behind-command`](implementation/runtime-acceptance.md) |
| RUN-01.3 | 126 | Which operations need cross-process shared memory, copy operations, handles, immutable snapshots and generation numbers? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`RUN-lease-ro`, `RUN-late-write`, `RUN-handle-stash`](implementation/runtime-acceptance.md) |
| RUN-01.4 | 127 | Can a plugin freeze/alloc-bomb/trap on startup without hanging or corrupting any application document? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`RUN-bomb`, `T-recycle-restore`](implementation/runtime-acceptance.md) |
| RUN-01.5 | 128 | Does the design avoid a second codepath for native calls, scripts, agent commands and headless CLI? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`RUN-sink`, `RUN-lint`](implementation/runtime-acceptance.md) |

## RUN-02 · No-network security and permissions

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| RUN-02.1 | 131 | Enumerate all ways a plugin can reach network: host imports, WASI, browser JS/webview, OS sockets, subprocesses, `localhost` proxies, device APIs, raw paths, inherited environment, IPC to host, native linking. Which are categorically denied? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`NET-matrix`, `NET-dns-deputy`, `RUN-sink-binding`](implementation/runtime-acceptance.md) |
| RUN-02.2 | 132 | What is the exact security authority of plugin principal, runtime instance, package, contribution, document scope, release and app? Do permissions attach per package or contribution with process-memory implications? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`RUN-identity`, `NET-xprincipal`](implementation/runtime-acceptance.md) |
| RUN-02.3 | 133 | How does every call pass target-resolved authorization, revision/grant generation, implicit/default document context and cancellation? How does revoke/disable poison an in-flight edit? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`RUN-sink-launder`, `T-revoke-commit`](implementation/runtime-acceptance.md) |
| RUN-02.4 | 134 | Can a privileged native helper be confined to *no networking* on all supported OSes? If not, must it be excluded or separately sandboxed with a credible complete network-denial boundary? | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`NET-matrix`, `PKG-spi`](implementation/runtime-acceptance.md) |
| RUN-02.5 | 135 | How are UI phishing, forged dialogs, synthetic input, clipboard, exfiltration via local file access, malicious shaders and native escapes considered? Distinguish mitigation from unattainable promise. | [Decision](decisions/ADR-RUN-01.md) · [Contract](contracts/runtime.md) · [`RUN-sink`, `NET-cef-webrtc`, `GPU-xprincipal`](implementation/runtime-acceptance.md) |

## RUN-03 · SDK/IDL/versioning/adapters

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| RUN-03.1 | 138 | What precise stable boundary survives changes to EffectCraft/PhotoCraft internals: capability IDs, typed schemas, generated Rust/TS bindings, semantic version, optional negotiated extensions, legacy contracts? | [Decision](decisions/ADR-RUN-03.md) · [Contract](contracts/sdk.md) · [`SDK-major-forever`, `SDK-alias-persisted`, `IDMAP-props`](implementation/sdk-acceptance.md) |
| RUN-03.2 | 139 | Does adapter own operation mapping/document semantics while shared platform owns permission, dispatch, lifecycle, auth, UI IR? Are there any duplicate per-app validators or enum wrappers that will drift? | [Decision](decisions/ADR-RUN-03.md) · [Contract](contracts/sdk.md) · [`SCHEMA-equiv`, `SDK-no-private`](implementation/sdk-acceptance.md) |
| RUN-03.3 | 140 | How does app CI detect breaking API changes and prove every public feature in advertised contracts? Design contract diff/export and independent host conformance. | [Decision](decisions/ADR-RUN-03.md) · [Contract](contracts/sdk.md) · [`SDK-idl-spike`, `SDK-major-forever`](implementation/sdk-acceptance.md) |
| RUN-03.4 | 141 | Can a plugin ask what the host supports in a stable manner without switching on private app versions or renderer implementation? | [Decision](decisions/ADR-RUN-03.md) · [Contract](contracts/sdk.md) · [`CAP-truth`](implementation/sdk-acceptance.md) |
| RUN-03.5 | 142 | If next month PhotoCraft changes its UI framework, command table, color pipeline or composition model, what code changes *exactly*? Simulate one such refactor and show that old guest binaries still run. | [Decision](decisions/ADR-RUN-03.md) · [Contract](contracts/sdk.md) · [`SDK-save-refactor-reopen`](implementation/sdk-acceptance.md) |

## RUN-04 · Compatibility, headless and platforms

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| RUN-04.1 | 145 | Can the same guest/package run on desktop macOS, Windows and Linux, as well as browsers when feasible? What exact capabilities are supported per target? | [Decision](decisions/ADR-RUN-03.md) · [Contract](contracts/sdk.md) · [`CAP-truth`, `WASM-profile`](implementation/sdk-acceptance.md) |
| RUN-04.2 | 146 | What runs in headless CLI tests and exports with UI contributions disabled? Are workers spawned differently on web? What is the fallback on Wayland/no OS subwindows? | [Decision](decisions/ADR-RUN-03.md) · [Contract](contracts/sdk.md) · [`CAP-truth`, `WEB-broker`, `SDK-script-sync`](implementation/sdk-acceptance.md) |
| RUN-04.3 | 147 | Does webview need special privilege or same-origin policy? No network means disable navigation/fetch/remote resources in the host and test that assumption. | [Decision](decisions/ADR-RUN-03.md) · [Contract](contracts/sdk.md) · [`CAP-truth`; `NET-cef-webrtc` in runtime acceptance](implementation/sdk-acceptance.md) |
| RUN-04.4 | 148 | How do architecture-specific native helpers differ and how is strict permission/no-network enforced on each OS? Document unsupported platforms honestly. | [Decision](decisions/ADR-RUN-03.md) · [Contract](contracts/sdk.md) · [`CAP-truth`; native `NET-matrix` in runtime acceptance](implementation/sdk-acceptance.md) |

## RUN-05 · Observability, tests and performance

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| RUN-05.1 | 151 | Define budgets on actual work: pixels×taps, render tiles, CPU time, fuel, memory reservations, GPU allocations, job queues, worker concurrency, message sizes and event fanout. | [Decision](decisions/ADR-SDK-02.md) · [Contract](contracts/conformance.md) · [`PAR-fuel-map`, `FAULT-quota`, `CONF-arm-identity`](implementation/conformance-acceptance.md) |
| RUN-05.2 | 152 | How to measure built-in feature parity in p50/p95/p99 GUI frame times, effect render times, CPU/RSS, GPU use, cold activation and memory leaks? Use *paired* same-machine baselines. | [Decision](decisions/ADR-SDK-02.md) · [Contract](contracts/conformance.md) · [`CONF-calibrate`, `CONF-cadence`, `CONF-hitch`, `CONF-overhead`, `LEAK-cycle`](implementation/conformance-acceptance.md) |
| RUN-05.3 | 153 | What catastrophic faults are tested: malformed payloads, panic, trap, cancellation, stale grants, blocked network, invalid geometry, GPU loss, script infinite loop, two plugins contending, autosave mid-crash? | [Decision](decisions/ADR-SDK-02.md) · [Contract](contracts/conformance.md) · [fault grid, `FAULT-adapter-panic`, `FAULT-autosave-kill`](implementation/conformance-acceptance.md) |
| RUN-05.4 | 154 | What logging/profiling tools can plugin authors use safely to debug without internal private knowledge or sensitive document exfiltration? | [Decision](decisions/ADR-SDK-02.md) · [Contract](contracts/conformance.md) · [`LOG-redact`, `LOG-covert`, `LOG-author`](implementation/conformance-acceptance.md) |

## SDK-01 · Developer ergonomic contract

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| SDK-01.1 | 159 | What is the shortest path to a new panel, native edit command, non-destructive effect, render kernel, procedural layer, brush/tool, importer? Each should have a working 20–50 line reference example where possible. | [Decision](decisions/ADR-RUN-03.md) · [Contract](contracts/sdk.md) · [`SDK-examples-7`](implementation/sdk-acceptance.md) |
| SDK-01.2 | 160 | Rust and TS/JS parity: which APIs can be pure guest code and which require generation? What IDE types, docs, stubs, errors, sandbox tests and local dev tooling are necessary? | [Decision](decisions/ADR-RUN-03.md) · [Contract](contracts/sdk.md) · [`SDK-examples-7`, `DEV-loop`, `DEV-debug`](implementation/sdk-acceptance.md) |
| SDK-01.3 | 161 | Can existing code be ported with a small translation layer when ABI evolves? Is host API version negotiation explicit? | [Decision](decisions/ADR-RUN-03.md) · [Contract](contracts/sdk.md) · [`SDK-legacy-retire`, `SDK-source-compat`, `SDK-downlevel-rmw`](implementation/sdk-acceptance.md) |
| SDK-01.4 | 162 | How should plugins declare permissions and supported capabilities so authoring errors are compile/test-time rather than user-facing failures? | [Decision](decisions/ADR-RUN-03.md) · [Contract](contracts/sdk.md) · [`SDK-permission-compile`, `CAP-truth`](implementation/sdk-acceptance.md) |

## SDK-02 · Feature parity proof and cleanup

| ID | Bank line | Question | Closure |
|---|---:|---|---|
| SDK-02.1 | 165 | Take at least 12 challenging *functional* plugin scenarios, not proprietary plugin binaries. For each trace every host API and verify public SDK sufficiency on EC and PC. | [Decision](decisions/ADR-SDK-02.md) · [Contract](contracts/conformance.md) · [REF-01–20 per app, `VIEW-pick`, `TEXT-shape`, `AUD-port`, `GEN-fill`](implementation/conformance-acceptance.md) |
| SDK-02.2 | 166 | Can first-party teams build one real professional feature strictly as a plugin, with no private hacks? If not, what missing general host contract is the actual root cause? | [Decision](decisions/ADR-SDK-02.md) · [Contract](contracts/conformance.md) · [`PAR-pc-stylize`, `PAR-ec-echo`, `PAR-pc-liquify`, `PAR-ec-track`, `PAR-ec-primitive`](implementation/conformance-acceptance.md) |
| SDK-02.3 | 167 | Which old plugin features and saved projects must survive removal? What old app loaders, duplicate UIs and settings may be deleted only once migration/conformance pass? | [Decision](decisions/ADR-SDK-02.md) · [Contract](contracts/conformance.md) · [`RET-*`, `RET-corpus-coverage`, `RET-ec-library`](implementation/conformance-acceptance.md) |
| SDK-02.4 | 168 | How will an agent know the architecture is complete? Require a coverage matrix with every question, ADR, contract, app integration, test and proof plugin mapped—no category marked 'covered' by a vague SDK facade. | [Decision](decisions/ADR-SDK-02.md) · [Contract](contracts/conformance.md) · [`COV-matrix`, `SDK-typed-ratchet`](implementation/conformance-acceptance.md) |

