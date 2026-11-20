# Proof workflow inventory

Each REF registers a cell per app, for example `REF-05-ec` and `REF-05-pc`, holding either a run or `Unavailable(reason)`. These workflows come from the reference-plugin matrix and are inputs to the design reviews. Each proof must use the public SDK, be original, and exercise real app providers. The phase plan will bind exact assertions and acceptance IDs. No proof has run in this workshop.

| ID | Target | Required end-to-end evidence |
|---|---|---|
| REF-01 Dockable Control Studio | Both | Open two panels, override an eligible inspector, detach a custom editor, use IME and accessibility, restore after worker death, perform one undoable edit. |
| REF-02 Procedural Particle Layer | EC | Create and persist the object, interpolate typed params, seek and render in random order, use a checkpoint only when its dependency key matches, require explicit outcome for missing exact export. |
| REF-03 Wet Pigment Canvas | PC | Capture pressure and tilt through the normal tools path, preview locally, composite across tile seams, undo the whole stroke and resume the saved editable object. |
| REF-04 Temporal Denoiser | EC | Compare arbitrary request order, previous/future sampling, time remaps, cache invalidation, cancellation and exact output. |
| REF-05 Offline Segmentation | Both for compatible still sources | Use a local model with brokered data, update progress, cancel with zero edits, commit a mask only to the revision and layer that supplied pixels. |
| REF-06 Color Grader | Both | Run the same SDK package and negotiated profile conversion on high-depth sources, round-trip live state and compare CPU/GPU semantics. |
| REF-07 3D Workspace | EC and compatible custom view in PC | Manipulate a model in a plugin-owned panel and app viewport. Persist geometry and authored property state. Measure redraw, reset device and preserve document state. |
| REF-08 Shape/Text Generator | Both on advertised domains | Query target, batch thousands of elements into one Undo, preserve text editability and supported vector data, report cross-format losses. |
| REF-09 Tracking and Rotoscope | EC | Read timed frames in a cancellable job, transform overlay coordinates after zoom/reparent, apply masks/keyframes once against captured revision. |
| REF-10 Layer and Effect Explorer | Both | Virtualize a large tree, query visible ranges with stale cursor rejection, preserve selection and keyboard focus while rows reorder. |
| REF-11 Offline Format Importer | Both | Choose source and destination through host, process malformed and large files with bounded reads, reject remote references, publish a complete export or report exact partial effects. |
| REF-12 Timeline and Workspace Tools | EC, common slots PC | Register command and shortcut, test hook ordering and fallback, restore the built-in view after disable, preserve protected commands. |
| REF-13 Audio Analysis | EC where audio advertised | Read exact sample/time windows, calculate a beat map and commit markers; use bounded DSP offline and outside the device thread for interactive playback. |
| REF-14 Batch Composer | Both | Capture multiple document snapshots, cancel during a multi-file batch, enumerate published outputs and validate one atomic edit per affected document. Includes a two-open-document case through `doc.list` and `doc.ticket`. `DOC-multi`. |
| REF-15 Inter-plugin Services | Both | Invoke across two distinct caller scopes, refuse authority escalation, bounded re-entry and cyclic service dependencies. |
| REF-16 First-party Feature Extraction | Both | Move a mature workflow behind the same public contract, verify normal and safe mode, compare paired performance, remove old executable path only after passing. Extractions: PC Stylize and Liquify, EC Echo, Track Motion and Primitive ([`PAR-*`](conformance-acceptance.md#parity-extractions)). |
| REF-17 Keyer | Both | Eyedropper parameters sampled by the host with `sample_stage`, a preview-only matte view, a mask input, and an exact export of the final output. `VIEW-pick`, `VIEW-export-final`. |
| REF-18 3D and Extruded Text | Both | Brokered font listing and shaping into a `GlyphRun` with provenance, editable text property, extrusion through the 3D route, and missing-font substitution. `TEXT-shape`, `TEXT-edit`, `TEXT-missing-font`, `RND-scene-3d`. |
| REF-19 Audio-reactive Visuals | EC (PC `Unavailable(NoAudioRole)`) | A visual processor with an `AudioWindow` input. An audio edit invalidates only the affected frames. `AUD-port`, `TIME-invalidate`. |
| REF-20 Generative Fill | Both (EC on a still frame) | Selection mask, local-model job, one-Undo new layer, no network. `GEN-fill`, `JOB-infer`, `JOB-readset-capture`. |
| REF-21 Resource Library | Both | A package adds brushes, patterns, gradients, swatches, LUTs and tool presets through typed operations with provenance, the user applies them in native tools, and the entries survive uninstall as attributed data. `RES-library`. |

Historical workflow lessons include FlowCraft's easing curves and presets, PrimeCraft's broad command coverage, BOUNCraft's expressions and selected-property editing, and Auto Beat Marker's audio analysis. Reproduce the user tasks in original proof plugins. The current PhotoCraft Smart Filter path adds a migration test beyond the old beta destructive filter. Historical PluginCraft investigation · Historical BOUNCraft investigation · [Current evidence](../source-inventory.md#corrections-to-historical-assumptions)
