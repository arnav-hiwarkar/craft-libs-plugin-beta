# Implementation phase plan

This plan reconciles all topic ADRs, including [RUN-05/SDK-02](../decisions/ADR-SDK-02.md). Implementation starts only after the owner approves the specs. It runs from the [current baselines](../source-baselines.md), one small reviewable PR per slice. Every slice must pass its listed counters and tests to merge. A slice never waits on a timing verdict: `Inconclusive` blocks only retirement (RET) and release claims, and at a phase exit it becomes a tracked `DEFECT`. Design-time hypotheses stay hypotheses until an eligible `RunRecord` exists.

## Numbering map

- This file's phases 0–7 are canonical.
- ADR-RUN-01's P0–P5 map as follows:
  - P0 → 0g
  - P1 → 1
  - P2 → 4 and 7 (superseded by `PAR-*`)
  - P3 → 4d
  - P4 and P5 → tracks N and H
- The workshop kit's phases 0–8 are historical.

## Phases

| Phase | Slices, each one PR or a short series | Exit gate |
|---|---|---|
| **0a** Registry and docs | Move canonical design docs into CL `docs/plugin-platform/`. Create `plugin-conformance` with the registry and coverage lint. | `COV-matrix` fails on today's defects and passes on its fixtures |
| **0b–0d** Harness and probes | CL `testkit::pair`. PC `xtask perf --pair` with p99. EC `bench --pair` and perf-nightly. C/I/L probes in both apps and P in EC, replacing SlowFrames and the EMA. | First counters and A/A `RunRecord`s on hosted runners. Existing tests unchanged. `CONF-probe-continuity` baseline |
| **0e** Declared fields | Turn each id-keyed list into precomputed registration tables, one list per PR: EC `GPU_EFFECTS`, `TIME_DEPENDENT`, `SKIPS_COVER`, `READS_COMP_SCENE`, prefix rules; PC preview/angle/colour lists and `last_filter`. | Behaviour tests identical. An approved PC perf-nightly shows no regression |
| **0f** Legacy corpus | `xtask legacy-corpus` per app, on today's code | `RET-corpus-coverage`. Digests committed |
| **0g** Identity and authority, no plugin code | IDS-NOW: `engine.lock`, `craft-idmap`, `resolve_persisted` on every load path. AUTH-GATE: delete PC `authorize`, `automation_gate`, `automation_input` and the gate thread-locals; admit EC before `enabled`; script `Socket`, model `curl` and prefs become sinks; provenance on persisted artifacts and bindings. FILES-SINK: `craft-files` for the apps' own saves, with typed durability. | `IDMAP-props`, `FAULT-alias-spoof`, `RUN-lint`, `RUN-classify-all`, `RUN-policy-diff`, `RUN-sink`, `RUN-sink-launder`, `RUN-sink-binding`, `RUN-replay-scope`, `IO-crash` on APFS/ext4/NTFS. No engine-id rename merges before IDS-NOW |
| **0h** Framework convergence | Pin a common framework revision and one egui family for both apps. FW-UI-01. | Cargo-resolved facade and egui family verified in both apps |
| **1** Runtime kernel | API-CORE: `plugin-api`, `#[craft::api]`, `sdk-gen`, strict prescan. BROKER: broker thread and `Arc<Snapshot>`. RUNTIME: `craft-plugin-host`, `craft-plugin-ipc`, `craft-plugin-confine`, signed `craft-worker`, rings, credit ledger, CPU caps. WorkMeter, log ring, self profile. Do not port beta `NativeFull`, `Network(host)`, the class gate, the WK filter or the writable compile cache; the beta remains historical. | Nothing loads by default until all of these pass: `NET-matrix` (Wasm column, including Flatpak and Docker), `NET-dns-deputy`, `NET-xprincipal`, `RUN-identity`, `RUN-lease-ro`, `RUN-late-write`, `RUN-bomb`, `T-hol`, `RUN-idle-call`, `T-cpu-vs-wall`, `T-service-grief`, `T-credit-deadlock`, `RUN-oversub`, `CONF-cold`, `CONF-overhead` (counters), `RUN-cache-isolation`, `SDK-idl-spike`, `SCHEMA-equiv`, `ENV-fuzz`, `SDK-source-compat`, `LOG-redact`, `LOG-covert` |
| **2** Operations | ENG-EC-EDIT, ENG-PC-EDIT, EXT-OPS. TYPED: the typed-handler ratchet starts here. FILES-GRANT, with read-only grants first. | No guest edit lands before `T-crash-commit`, `FAULT-adapter-panic` and `EDIT-lost-update` pass. `EDIT-target`. Write-class rights need `IO-protected`, `IO-rename-delete` and `IO-mass-destroy` |
| **3** UI and interaction | EC-UI-PORT, EXT-UI-01, PC-UI-01, EC-UI-01. UI02-CONTRACT, UI02-FRAMEWORK, UI02-MOUNT. EXT-TOOL. Liquify spike, dev only, restricted to the UI body and tool capture with no processors. | UI-01-FREEZE waits for: the Liquify spike (UI body and tool capture); `CONF-cadence`, `CONF-latency` and `CONF-hitch` (an eligible Pass or a `DEFECT`); `FAULT-ui-flood`; `FAULT-slot-collision`; and the UI01 and UI02 acceptance rows. |
| **4a0** Extension record | DOC-RECORD (`plugin-api::doc::ExtensionRecord` with `TypeKey`, `Provenance(Chain)` and `BakeKey`) plus the PC Smart Filter attachment | `DOC-roundtrip`. `PAR-pc-stylize` and the legacy shims (`TypeKey legacy.pc.<id>`) depend on this slice. |
| **4a** Processors | RND-SCHEMA, RND-EC, RND-PC, RND-EVAL. Legacy shims, after 4a0. `PAR-pc-stylize` (after 4a0) and `PAR-ec-echo` under distinct dev ids. | The processor contract freezes only after `PAR-pc-stylize` and `PAR-ec-echo` pass correctness and counters, plus `PAR-id-leak`, `PAR-fuel-map`, `RND-format`, `RND-tile`, `RND-full`, `RND-authority` and `RND-cache`. A timing Fail opens a per-id `DEFECT`. |
| **4b** Files, formats and jobs | JOB-CORE, FORMATS with typed `options`, INFER on CPU in HostNative | `JOB-aba`, `JOB-readset-capture`, `JOB-cancel-race`, `JOB-revoke`, `JOB-oom`, `IO-disk-full-autosave`, `IO-parser-confine`, `IO-format-open-save`, `JOB-scratch-batch`, `FORMAT-options` |
| **4c** Time and audio | TIME-RESOLVE, TIME-EDIT, AUD-MIX, AUD-TRANSPORT | `TIME-*` and `AUD-*` |
| **4d** GPU | RND-DEVICE, GPU provider with per-principal logical devices, UI02-PROVIDER, GPU inference | `GPU-xprincipal`, `GPU-software-parity`, `RND-gpu`, `UI02-device`, `JOB-gpu-fair`, `JOB-gpu-tdr`, `JOB-model-bomb` |
| **4w** Web track | `WEB-broker` backend. WebGPU processors in the broker Worker. PC keeps in-page wasmi until `WEB-broker` passes. | `WEB-broker`, `WEB-gpu`, `IO-web`. `RET-stylize` depends on this. |
| **5** Persistent objects | DOC-EC, DOC-PC (remaining attachments and extension layers; the record and Smart Filter attachment landed in 4a0), DOC-IO, DOC-RENDER. Track spike, dev only. Asyncify `.jsx` shim (SCRIPT). | The DOC freeze waits for the Track spike, the `DOC-*` rows, `FAULT-autosave-kill`, `FAULT-safe-mode` and `RET-downgrade-pixels`. The script shim needs `SCRIPT-ec-suite`, `SDK-script-sync`, `SCRIPT-reentry` and `SCRIPT-perf` on desktop, web without JSPI, and headless. |
| **6** Full workflows | HOST-DESKTOP conformance kit and author CLI. Liquify (full, with processors), Track Motion and Primitive packages. The TEXT-01 service, VIEW annotations and the AudioWindow input as RENDER-01, RENDER-03 and UI-02 amendments. `doc.list` and `doc.ticket`, host resource library operations, `input.device` and `capture.open`. REF-01 to REF-21 on both apps. | Typed-handler ratchet at 100%. `PAR-pc-liquify`, `PAR-ec-track`, `PAR-ec-primitive`. `DOC-multi`, `RES-library`, `INPUT-device`, `CAPTURE-open`. `SDK-examples-7`, `SDK-permission-compile`, `SDK-save-refactor-reopen`, `SDK-downlevel-rmw`, `SDK-major-forever`, `WASM-profile`. `LEAK-cycle`, `LOG-author`, `CAP-truth`. The full fault grid. Every REF cell is Pass, `Unavailable(reason)`, or a landed `CONTRACT_DEPENDENCY`. |
| **7** Cutover and retirement | One PR per path, in this order: Echo, Stylize (after 4w), Liquify, Track, Primitive, PC wasmi, EC Wasm and `effect.plugins`, Plug-in Library network removal and install migration, boa scripts, beta. | Each deletion needs, on an eligible runner: its `RET-*`, `RET-headless`, `RET-mig-race`, `RET-corpus-coverage` and `FAULT-alias-spoof-legacy`; a timing **Pass**; and no open `DEFECT` on that id. Library also needs zero `curl` children and an unopened `catalog.json`. |

## Independent tracks after phase 2

| Track | Content | Gate |
|---|---|---|
| N · NativeConfined | Linux, then macOS, then Windows. EC `helper-host` moves here. | Native `NET-matrix` column (descendants, DNS, signals, TCC), `PKG-macos-jit`, `PKG-spi`. Unavailable until it passes. |
| H · HTML (CEF) | Optional component installed from Settings from a bundled or user-picked local archive, never downloaded. UI02-HTML. | `NET-cef-webrtc`, `CEF-no-windowserver`, `CEF-cost`. Native fallback until it passes. |
| A · Windows arm64 | Pulley versus x64 under Prism | `WASM-winarm64`, `NET-matrix`. The cell stays P until it passes. |

## Rules for every slice

- Record the app and framework pins, OS, fixture and `run_id` for each result.
- Slices branch from the pinned baseline into the owner's arnav-hiwarkar/* forks or Forgejo, never a storytold remote; builds run only in approved Actions jobs.
- Use one designated build owner. Scoped local checks are allowed.
- Deterministic counters gate merges. Timing gates use calibrated eligible runners.
- Owner authorisation is required for any self-hosted runner.

Release claims need every eligible gate to Pass, plus an independent security review.

[Dependencies](dependencies.md) · [UI-01 slices](ui-01-slices.md) · [Legacy dispositions](legacy-inventory.md) · [Proof workflows](proof-inventory.md)
