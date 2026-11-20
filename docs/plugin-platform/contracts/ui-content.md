# Plugin UI content contract

Designed in [UI-02](../decisions/ADR-UI-02.md), with app and provider tests pending. It uses the existing [mount and input contract](ui.md).

```text
Node = Container(key, layout, props, children, decor?)
     | Control(key, kind, version, binding, props, actions, decor?)
     | Collection(key, order_source, columns, actions)
     | Canvas(key, logical_scene, hit_map, semantic_tree, ColorSpace)
     | GpuView(key, output_ref, semantics)
     | LocalHtml(key, bundled_entry, bridge_version)
     | Choice(key, required_contract, primary, fallback, action_map)
ContentUpdate = Full(expected_revision, root) | Patch(base_revision, KeyedChange[])
PublishResult = Applied(new_revision) | Rejected(current_revision, reason)
RangeRequest = (mount, collection_key, order_generation, span, request_id)
ParamAnnotation = Eyedropper{sample_stage: input|display, preview_only} | …
```

Use host stacks, grids, splits, scrollers, tabs, sections, labels, icons, images, buttons, choices, inputs, sliders, menus, and indexed lists, tables and tree tables. Containers mix controls, canvas and GPU or HTML leaves. Give interactive controls and hits stable bounded keys. Translate labels from package messages, with host locale and application text direction. Use framework theme roles and named variants. It owns arrangement and geometry in logical points. Compose one transform and clip for drawing, hit regions and accessibility bounds. Authors may supply custom decoration or replacement paint for eligible controls. The actual control retains its semantics, bounds, focus indicator, checked state and action. Security chrome has no decoration slot.

## Prepare, publish and invalidate

1. Frame validators charge byte and message capacity before allocation. Parse a bounded schema. Stage against the expected mount and published revision. Check unique keys, node type and field semantics, both branches of a choice, asset and capability references, resource and work limits. Fuzz malformed change paths and over-budget trees. Resolve component and provider availability, preserving an accessible fallback for every optional provider.
2. Reserve time, scratch, output vertices, decoded images and live memory before expansion. Tessellate curves and prepare indexes off paint. A view can have one active transform preparation and one superseding target. Cancel obsolete prep and discard stale results. Charge candidate, active and retired resources together per package and in aggregate, including text shaping, clipping, ancestor replays, late uploads and fence-delayed reclamation. Preflight dimensions and sizes with checked arithmetic.
3. Apply prepared state atomically after rechecking current base revision, mount and grant. A wrong base returns the current revision for resync without mutating it. Associate prepared visuals, hit and semantic maps with matching content, `ReleaseDigest`, `ProcessEpoch` and `DeviceGeneration` ([generations](runtime.md#generations)). A bad candidate keeps the last valid body only while every dependency remains authorized. If the target, grant or provider becomes invalid, withdraw affected pixels, accessible labels and actions, end drafts and capture, and show an independently valid fallback or attributed inert status. Swap to the prepared fallback if a previously accepted leaf becomes unusable.
4. Use a host interaction generation for each continuous compatible node binding. Stable author keys permit reorder and relocation within a mount without recreating a text draft, selection, focus or capture. Removal, replacement, semantic/schema or scope changes end incompatible transient state and advance generation. Explicit correspondence may preserve state through semantic-compatible fallback. Event outcomes correlate authoritative control values and local drafts. An old action receipt cannot run after patch or resync.
5. Publish new region generations to the canonical measurement and paint mechanisms. Include relevant descendant data, assets, component resolution, scoped snapshots, stage/texture generation and control/semantic changes that Craft cannot observe. Use `DockPanels::content` only with full dependency coverage. Independently invalidate removed or incompatible hits, focus records and accessible actions, including offscreen `Lazy` transcripts. A frozen paint replay cannot retain old interaction authority.

## Parameter annotations

`ParamAnnotation::Eyedropper{sample_stage, preview_only}` marks a colour or value parameter as host-sampled. The host draws the picker, samples at `input` (the processor's input) or `display` (the displayed result), converts to the parameter's `ColorSpace` and writes the value through a normal edit. The guest never reads displayed pixels. A `preview_only` view (for example a keyer matte) is shown only in the viewer, and export always renders the final output ([render contract](render.md#guest-reads)). Tests: `VIEW-pick`, `VIEW-export-final`.

## Indexed content and custom areas

A collection order describes stable row keys, parent and depth, rank, bounded extent estimates and fixed or prepared column metrics. Build order and size indexes outside paint. A viewport obtains visible and overscan ranks from the extent index, then reads resident pages by key. An asynchronous bounded page request carries the order generation. Stale pages are ignored. Selection, lead, range anchor and scroll position are keyed, with scroll offset relative to its row. Width and locale changes may prepare a new index while the old one remains published. Measure real row calls and index probes during sizing and paint. Include an indexed semantic lookup and reveal for offscreen entries. A missing row has loading semantics and zero mutation authority until its binding is materialized and revalidated. Resolve the current key, generation, enabled state and target after reveal. Accessibility actions pass the same modal and authority barrier. Keep a single semantic owner for each subtree.

A node editor publishes logical draw objects, hit targets, accessible roles, names and states, and keyboard actions such as inspect, connect and disconnect. Stable ids identify ports and nodes. A searchable semantic representation permits operation even when zoom hides them. Paint and hit maps share the same frame transform. Canvas interaction uses host capture and staging. A document mutation must use the normal authorized owner-thread commit. GPU views come from the GPU provider as a host-owned leased frame with matching semantics; reject a stale `DeviceGeneration`. Canvas scenes, image assets and swatch values declare a `ColorSpace`, and the host converts to the display. Provider/compositor loss can replace the leaf while the shell paints. Actual window-device loss follows app recovery. EffectCraft needs to retain unsaved state and expose a recovery path when saving fails, as PhotoCraft already retries. RENDER-02 defines lease dimensions, format, alpha, color and synchronization. Hosted HTML must be a clipped, attributed leaf with verified local-only process and broker behavior, bridge provenance, and a fresh browser-document generation after reload. Bound both IPC directions and async immutable asset reads before trusted parsing or copies. Atomically withdraw clip, hit and accessibility participation when a protected prompt takes priority. Only advertise a provider with successful cross-window composition and confinement proof. Retire bridge, visibility, focus and accessible records before fallback. An unauthorized or unsupported provider keeps equivalent controls and custom canvas available.

## App integration

PhotoCraft supplies mount body and dependency stamp from its `DockPanels::body/content` hooks and Properties inspector. The pinned framework needs generation-aware skipped focus records and a prepared list layout provider. EffectCraft reuses this body renderer once its dock port meets UI-01. Test migration of mixed custom ScriptUI control paint against direct SDK decoration and Canvas. [Acceptance cases](../implementation/ui-02-acceptance.md) · [UI-01 app slices](../implementation/ui-01-slices.md)
