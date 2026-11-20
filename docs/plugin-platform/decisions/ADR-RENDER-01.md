# RENDER-01 and RENDER-02 · Processor and resource scheduling

Accepted with verification. The [render contract](../contracts/render.md) is designed. Precision, device containment and performance gates remain unrun.

## Decision

Use typed render plans over immutable, scoped inputs and a host-owned dependency scheduler. CPU packages provide tile, global-analysis or full-frame processing. The app device runs bounded host-owned kernels and supports resident passes. The GPU provider exclusively encodes general guest WGSL on a per-principal logical device in the shared GPU provider, including verified programmable drawing. App adapters select valid render configurations jointly with destination precision and actual input permissions. Standard inter-device transfer uses completed and sealed CPU data. Direct GPU transfer needs independently proven writer exclusion and producer and last-consumer fences. The guest receives typed handles and declared features. It can author geometry, materials, compute and draw passes without depending on a GPU driver or toolkit.

A CPU-only contract would restrict several professional GPU and scene workflows. Sharing the app device with general guest shaders would remove some transfers but could stall the compositor under a bad dispatch. The GPU provider reduces resource and software coupling. Shared hardware contention and reset remain real. Reserve both devices' live and retired memory, compiler/encoder/file staging, and actual runnable dependency work. Keep host input responsive under provider stall. Alternative comparison (review record kept outside this repository) · Source evidence (review record kept outside this repository)

Each port declares data type, color/ICC, alpha, numeric precision, layout and per-plane sample bounds. The host expands ROI into real physical spans before authorization and packing, including transformed masks and wrapped reads. Absolute coordinates and seeded streams preserve tiled equivalence. An exact stage proves complete coverage and verified provenance across all conversions, through the final encoder or native exact composite. Failure closes an unpublished staging artifact. A preview can retain attributed last-good output under live scope. CPU remains the compatible exact route for PhotoCraft high-depth sources until the full GPU path preserves that precision.

## Integration slices

| Slice | Owner and mapping | Exit proof |
|---|---|---|
| RND-SCHEMA | Platform. Version typed image/lease/plan descriptors, region expansion, result quality and provenance. Freeze CPU transfer format and measurable copy accounting. | `RND-format`, `RND-tile`, `RND-full`, `RND-authority` |
| RND-EC | EffectCraft effect/source execution at `effects/src/plugin.rs`, `render/src/{pipeline,accel,three_d}` and existing GPU display. Move exact-ready output and scoped inputs through direct YUV and normal export. Replace public-destination creation in `export/src/out.rs` and path cleanup in pipeline with IO-01 private staging. | `RND-cache`, `RND-gpu`, `RND-scene-3d` through actual display and encoder |
| RND-PC | PhotoCraft Smart Filter prep, prefix key and compositing at `engine/src/cmd/smart_cmds/{rendering,stack_cache}.rs`, `compose`, canvas damage and `io`. Add vector and mask preparation. Keep native ink, high-depth CPU and linear/ICC distinctions. Reconcile exact status for both flat bands and layered encoders. | `RND-format`, `RND-cache` and `RND-tile` through real export and Undo |
| RND-DEVICE | GPU provider and app GPU owners. Extent/feature/DAG/binding validation, off-paint compilation and GPU queue, audited fence leases, bounded retired memory and recovery. EC native geometry or a scoped complete Advanced 3D run for arbitrary custom materials that interleave host translucency. Distinguish GPU provider fault from shell-device loss. | `RND-gpu`, `RND-scene-3d` and `UI02-device`. Offline proof per advertised mode |
| RND-EVAL | Both engines and performance owner. Run paired known-pixel 1080p/4K, EC multilayer matted 3D and 24/96 MP PC cases. Bound work by real dependencies and preservation of region, timing, format and profile. | `RND-cost` and the lease, device, denial and exact publication negative cases |

## Review resolution

Q1 uses typed per-port format and numerical profile. Q2 uses physical footprint plans and explicit partition modes. Q3 uses trusted resource generations, protected mappings, sealed handoff and accountable retirement. Q4 binds exact stage, seeds, coverage and provenance. Q5 permits verified asynchronous compute and draw through the isolated service. Q6 uses native geometry, complete scoped scene batches and typed app composition. Q7 measures actual work under matching profile/coverage. All seven B findings accepted. C passed with amendments. The contract adds joint role/format/authority admission, initialized padding, every consumer's fence, offline service and recovery, end-to-end working-set reservation, and exact private encoding with per-destination outcome reporting.

One-refresh local feedback under stall is a hypothesis. Require matched cold and warm p50/p95/p99, total copies and peak resource residency, including in-flight and retired data. Compare representative algorithms at identical quality before setting parity claims. [Acceptance cases](../implementation/render-acceptance.md)

The Q, A1, B, A2 and C review roles ran in order. All roles used GPT-6.2 Sol at high reasoning.
