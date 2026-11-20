# RENDER-03 and RENDER-04 · Timed evaluation and audio

Accepted with verification. The [timed contract](../contracts/time.md) is designed. Device, timing and output tests remain pending.

## Decision

Use one resolved time and dependency plan for video, simulation and audio. SDK points use signed rational grids and name their domain. Each app maps to its own underlying clocks, holds, stretch, remap, protected regions, shutter samples and frame boundary rules. Time maps are immutable for an invocation. An explicit recurrence group gives feedback a canonical step independent of source remapping. A valid cycle decreases the step after maps resolve. Host-verified checkpoint lineage and bounded replay preserve output across random seeks and restarts. Invalidate through both versions of a changed dependency graph.

Guest DSP runs ahead of the device in a bounded feeder. A direct callback would provide lower buffering delay but make audio liveness depend on guest execution. Pre-rendered proxies can serve proven identical spans but do not cover input changes or all live workflows. The feeder supports both prepared host graphs and blocks from `render` Stores in the Principal process, with state carried by host-held checkpoints. The device copies and meters preallocated data, reports exact cutover boundaries, and follows an explicit fallback clock when content is late. Headless export evaluates the same mix without a transport. It distinguishes intentional silence from broken decode or a missing authored track. Alternatives (review record kept outside this repository) · Source evidence (review record kept outside this repository)

## Integration slices

| Slice | Owner and mapping | Exit proof |
|---|---|---|
| TIME-RESOLVE | Shared typed timing schema and evaluator dependency descriptors. EC `crates/time`, `render/src/{eval,frames,fx_host}.rs` and current dirty-span propagation. Preserve legacy neighboring frame provenance. PC capture frame stacks and latest committed displayed pixels in `engine/src/cmd/video_cmds.rs`. | `TIME-map`, `TIME-neighbors`, `TIME-state`, `TIME-invalidate` |
| TIME-EDIT | App and document owners. Use native current layers and timeline history. EC split uses explicit time and target. PC adds authored placements for supported image clips/transitions alongside frame stacks. No implicit second timeline or user playhead seeks to query. Encode cumulative timing where representable. | `TIME-edit` through UI and headless, `DOC-object` |
| AUD-MIX | EffectCraft media/mix/export providers. At `media/src/pool.rs`, `render/src/audio.rs` and export `{encode,webm,lib}.rs`, keep authored track discovery, complete source status, exact format/priming/tail and coverage from decode through staged mux. | `AUD-format`, `AUD-exact` |
| AUD-TRANSPORT | EffectCraft playback owners. Update `ui-egui/src/audio.rs`, `ui-egui/src/app/prefetch.rs` and native and web app audio. Allocate ahead, async lifecycle, coherent cutover even with full ring, valid audible clock and bounded late-work reaping. Verify web worklet separately. | `AUD-device` and paired clock/latency fixtures |

## Review resolution

Q1 uses explicit host-mapped coordinates. Q2 uses planned time/ROI plus proven checkpoints. Q3 uses old/new inverse reach, budgets and generation cancellation. Q4 maps typed editor roles to native models with explicit availability. Q5 gives absolute audio sample/phase and layout. Q6 uses callback-owned cutover and no callback waits. Q7 has explicit degradation and staged exact output. B's seven changes are accepted. C passed with amendments for async playback and scrub startup/cleanup, a capacity-independent control channel and audible tail, dynamic authority and lineage, typed authored track discovery and decode outcomes through every encoder, and PC content generations. All amendments are in the contract and tests.

Measure a 48 kHz, 128-frame workload. The callback target is p99 below 10% of one period, with zero allocations, waits or guest work. The gate is the conformance limit of p99 ≤ 25% ([conformance contract](../contracts/conformance.md#tolerances), recorded in the [ledger](../cross-topic-conflicts.md#final-reconciliation-2026-11-20)). Compare clock mapping to one sample over 30 minutes in a deterministic simulation with cuts and underruns. Report hardware latency uncertainty separately. [Acceptance cases](../implementation/time-acceptance.md)

The Q, A1, B, A2 and C review roles ran in order. All roles used GPT-6.2 Sol at high reasoning.
