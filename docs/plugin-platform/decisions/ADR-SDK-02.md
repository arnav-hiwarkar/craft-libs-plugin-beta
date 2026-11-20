# RUN-05 and SDK-02 · Measured conformance, parity proof and legacy retirement

Accepted with verification. The [conformance contract](../contracts/conformance.md), [acceptance cases](../implementation/conformance-acceptance.md), [legacy dispositions](../implementation/legacy-inventory.md) and [reconciled phase plan](../implementation/phases.md) are designed. No harness, probe or run exists yet.

## Decision

**One paired harness, several honest metrics.** `pair` lives in CL `testkit` and is a dev-dependency of both apps. It owns histograms, the `RunRecord`, ABBA blocks, the block bootstrap and the verdict. Each app owns its own probes. The four metrics are:

| Metric | What it measures |
|---|---|
| C | UI-thread CPU per stage, with a plugin-host stage for each principal |
| I | Frame interval under continuous workloads, plus a long-stall count |
| L | Input to the end of the consuming pass, with app-local stamps |
| P | EffectCraft playback on-time ratio against the audio clock |

GPU routes use timestamp queries. None of these needs a change to the egui fork. They replace PC `SlowFrames` and the EC FPS EMA as views.

**Verdicts never block delivery, and noise never passes a regression.**
- Deterministic counters gate every slice on hosted CI: copy bytes, fence waits on the UI thread, wakeups while disabled, leaks, envelope bounds and AppDevice cross-device bytes.
- Timing verdicts need an *eligible* (class, gate, runner image):
  - at least 59 in-job A/A verdicts with no Fail;
  - positive controls that must Fail: an injected 2× CPU delay and a GPU-heavy canvas.
- Pooling across nights allows at most five looks at a Bonferroni-corrected level.
- An A0 arm with the host never started catches regressions shared by both arms.
- `Inconclusive` blocks only deletion (RET) and release claims. An `Inconclusive` at a phase exit becomes a tracked `DEFECT` due the next phase.
- A self-hosted reference Mac is optional. It exists only with owner authorisation, runs ephemeral and dispatch-only, never runs on `pull_request*` triggers, and holds no secrets. Workflow schedules need a per-phase approval reference, which follows the owner's CI rules.

**One tolerance formula.** `allow = max(floor, 10% × A)` on the quantile difference, checked on an isolated workload and on a realistic project.
- Floors are set per interaction class.
- A command has a hard 100 ms p95 ceiling whenever native is under 100 ms.
- An extracted feature has a 2 ms latency floor.
- Kernel throughput is not gated. Below 0.6× native it opens a `DEFECT`. This supersedes the RUN-01 "≥ 0.6×" gate.

**Work budgets at host choke points.** WorkMeter charges each (principal, request) at broker decode, ring grants (pixels × taps), credits, the limiter, the GPU provider, UI publish and outbox fanout. Legacy shims keep the larger of the contract caps and their legacy fuel and memory caps.

**Extractions prove the lean base.** Each extraction becomes a public-SDK package, replacing the id-keyed host lists with declared fields:

| Phase | Extraction | Proves |
|---|---|---|
| 4 | PC Stylize (with Oil Paint) | Tiled CPU, Smart Filters, Actions |
| 4 | EC Echo | Timed plan, GPU combine graph |
| 6 | PC Liquify | Modal body, tools, proxy and full-resolution ROI, session undo |
| 6 | EC Track Motion | Job, overlay, panel, persistent record |
| 6 | EC Primitive LayerSource | A rendered persistent object |

Dev-only spikes run before the contract freezes: the Liquify spike (UI body and tool capture only, no processors) before the UI freeze, and the Track spike before the DOC freeze. The full `PAR-pc-liquify` stays in phase 6. RET is per engine id, so a feature that misses its gate keeps only that id native, with a `DEFECT`.

**Scenarios.** REF-01 to REF-16, plus four new ones:

| Scenario | Built from |
|---|---|
| REF-17 keyer | Host eyedropper parameter annotations (`sample_stage`, `preview_only`); no guest read of displayed pixels |
| REF-18 3D and extruded text | A new brokered text service, TEXT-01 |
| REF-19 audio-reactive visuals | An `AudioWindow` input kind with an invalidation edge |
| REF-20 generative fill | Existing contracts only |

The [final reconciliation](../cross-topic-conflicts.md#final-reconciliation-2026-11-20) adds REF-21 Resource Library (finding M6). Each REF registers a cell for each app, recording either a run or `Unavailable(reason)`.

**Legacy retirement.** One PR per path. It passes only when all of these hold:
- the corpus survives;
- the path has zero callers;
- ids resolve or are tombstoned;
- settings migrate once under a lock;
- headless and downgrade cases pass;
- an eligible timing **Pass** exists;
- no `DEFECT` is open on that id.

The network parts of EC's Plug-in Library are deleted. Its SHA-verified installs migrate once behind consent. A hash mismatch gets an explicit "origin unverified" adoption path. Adoption records bind each legacy id to its (path, sha256, ABI), so no other package can claim it.

**Author observability without exfiltration.**
- Logs live in per-principal memory rings. Host and adapter crates log typed codes only.
- Crash reports carry only a principal hash and a fault code.
- `cx.profile.self()` returns the caller's own WorkMeter only. Aggregates appear in host Diagnostics.
- Span names are interned from the manifest.
- Transcripts are developer-only, capped at 64 MiB and expire after 7 days.
- Export goes through the user's sheet.

**Completeness is machine-checked.** A registry and coverage lint in CL `plugin-conformance` checks the ID regex, unique definitions and aliases, wildcard expansion, per-app REF cells and fault-grid reasons. It also checks TESTED provenance: `run_id`, with head SHA equal to the pin and a registered successful workflow. IMPLEMENTED requires `host=app`. A `DEFECT` needs an owner and an exit phase. The canonical design docs move into CL `docs/plugin-platform/` in phase 0a, after approval.

### Alternatives

| Alternative | Strongest argument | Why it lost |
|---|---|---|
| Extend each app's harness separately | Least new code | Two statistics and two verdicts drift apart. The one `pair` module stays small. |
| OS tracing only (Instruments, ETW, perfetto) | No in-app probes | Not portable to CI. It cannot attribute work to a principal, and no verdict is machine-checkable. |
| One "input to GPU submit" metric F (A1) | One number | Blind to the acquire wait, GPU jank and EC playback drops, and needs fork changes. |
| A required self-hosted `ref-mac` (A2) | Quiet hardware | Not authorised. A self-hosted runner on a public repo executes fork code. Calibrated eligibility works on any runner. |
| Stylize and Echo only (A1) | Small, testable | Processor-only. It doesn't prove that UI, tool or persistent-object features can leave the base. |

## Amendments applied from C

All 13 are applied:
1. Calibration-based eligibility, with an optional `ref-mac`.
2. The 59-run A/A rule plus positive controls.
3. Five looks, stratified by night and image.
4. The A0 arm, `CONF-overhead`, auto-`DEFECT`, and paired 15% overhead gates in both apps.
5. `CONF-hitch`, leaf `gpu_ns`, the 100 ms ceiling, the 2 ms PAR latency floor, and app-local L stamps.
6. Self-only profiles, interned spans, the logging lint, transcript caps and `LOG-covert`.
7. Legacy alias spoofing, the mismatch adoption path, and `RET-corpus-coverage`.
8. `PAR-id-leak`.
9. Lint provenance, docs moved into CL, and one `pair` module.
10. VIEW-01 shrunk to annotations, TEXT-01 fully specified, AUDIO-PORT as an input kind, all in phase 6.
11. Spikes before freezes, the two-REF rule for new nodes, and `PAR-ec-primitive`.
12. Five more fault classes and `FAULT-quota`.
13. Phase consistency, with the phase 4 split and numbering map in the [phase plan](../implementation/phases.md).

**Supersession.** RUN-01's FIRST-PARTY "≥ 0.6× native" gate and its alias `RUN-first-party` are replaced by the `PAR-*` gates and the Kernel `DEFECT` rule ([ledger](../cross-topic-conflicts.md)).

## Question resolution

| Question | Answer |
|---|---|
| RUN-05.1 | WorkMeter and the caps table |
| RUN-05.2 | C/I/L/P, pairing, eligibility and the tolerance table |
| RUN-05.3 | The fault grid |
| RUN-05.4 | Self-only profiles, log rings and replay |
| SDK-02.1 | REF-01 to REF-20, per app |
| SDK-02.2 | Five extractions with declared fields; any gap becomes a host-contract `DEFECT` |
| SDK-02.3 | Legacy dispositions and RET |
| SDK-02.4 | The coverage lint |

The Q, A1, B, A2 and C review roles ran in order. Every role and the lead ran Claude Opus 5.5 at high effort, in separate contexts. A1 and A2 are the same agent. No role ran on the GPT family.
