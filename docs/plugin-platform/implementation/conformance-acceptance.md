# Conformance, parity and retirement acceptance

Planned cases for [RUN-05/SDK-02](../decisions/ADR-SDK-02.md). None has run. Every timing gate needs an eligible runner, as defined in the [conformance contract](../contracts/conformance.md#eligibility-and-verdicts).

## Harness

| ID | Scenario | Expected result |
|---|---|---|
| CONF-calibrate / CONF-power / CONF-arm-identity | Run ≥59 A/A verdicts per (class, gate, image). Inject a CPU delay of 2× the allowance and a GPU-heavy canvas. Assert WorkMeter per arm. | Zero A/A Fail. Positive controls Fail and null Passes, or the gate is `Inconclusive(NoRunner)`. Arm identity holds. |
| CONF-aa-sequential | Simulate pooled nightly verdicts with true Δ equal to 0 and to 1.2× the allowance, three units a night with 25% block noise, and with Δ equal to 2× the allowance, four units a night with 10% block noise, up to five nights each. | False Fail and false Pass each have a Clopper–Pearson 95% upper bound ≤ 5% within five looks, and at least 80% Fail at 2×. On 2026-11-21, in a local debug run: false Fail 0 of 300, false Pass 2 of 300, Fail at 2× 200 of 200. |
| CONF-overhead | Compare A0 (host never started) with A at 0, 10 and 100 idle packages in both apps. | 15% p50 C and I gate. Zero wakeups and zero processes while disabled. |
| CONF-cadence / CONF-latency / CONF-hitch / CONF-playback | Drag, animate, paint, scrub and play back with an SDK panel, tool and effect versus built-in equivalents. | Meet the UI-eq and Playback rows, including ≤1 long stall per 10k frames. |
| CONF-cold | Spare bind, cold spawn, QuickJS image start, enable to first body. | Absolute limits in the contract. |
| CONF-probe-continuity | Run the EC UI port and the PC framework updates. | Stage names and counts unchanged. |
| CI-trigger-lint | Scan every workflow that can reach a self-hosted runner. | No `pull_request*` triggers, no secrets, and an approval reference on each schedule. |
| COV-matrix | Run the lint on today's docs, then on fixtures. | It fails on today's defects, including undefined IDs and duplicates. It passes on fixed fixtures. Negative fixtures cover forged records, wildcards and facade cells. |
| SDK-typed-ratchet | Count typed Domain and App leaves per PR. | The count never decreases. It reaches 100% at the phase 6 exit. |
| LEAK-cycle | Run 1,000 enable/mount/render/disable cycles. | Counts return exactly to baseline. RSS grows ≤ 4 KiB per cycle. |

## Parity extractions

| ID | Scenario | Expected result |
|---|---|---|
| PAR-pc-stylize | After slice 4a0, the nine Stylize filters, including Oil Paint, become a package. Exercise Smart Filters, Last Filter and Actions, and replace the id-keyed lists with declared fields. | Pixels within tolerance (`round_i32` bit-exact on wasm32). Command class for cheap filters and Export class for Oil Paint. Counters pass. A timing miss keeps only that id native and opens a `DEFECT`. |
| PAR-ec-echo | Echo becomes a package with a timed plan and an AppDevice combine graph. | Oracle digests identical. Same random-seek reach. Zero AppDevice cross-device bytes. GPU-route and Playback classes pass. |
| PAR-pc-liquify | With a spike. Phase 6: Liquify becomes a modal body and tool, with proxy and full-resolution ROI processors and session undo. The phase 3 spike, before UI-01-FREEZE, covers only the UI body and tool capture, with no processors. | 120 Hz dabs pass UI-eq. OK yields one Undo with the same field digest. Replay and Action pixels are identical. Only public UI leaves are used. |
| PAR-ec-track | With a spike. Track Motion becomes a package: a job over frames, an overlay, a panel, a persistent record and a keyframe commit. The spike runs before the DOC freeze. | Track digests identical. Cancel or a stale read makes zero edits. Apply is one Undo. `tracked-0.22.ecproj` round-trips and opens in the last native release. |
| PAR-ec-primitive | The `Primitive` LayerSource becomes a package object type with properties, keys, GPU render and save/reopen. | Pixel and persistence parity. Opens in the last native release. |
| PAR-fuel-map | Legacy shims under contract caps and legacy-derived caps. | Legacy fixtures complete within their old fuel and memory. Both verdicts are recorded. |
| PAR-id-leak | Save a Smart Filter or effect under a B arm id in a release build. | Persistence is refused. |

## Scenarios added to the proof inventory

| ID | Scenario | Expected result |
|---|---|---|
| VIEW-pick / VIEW-export-final | A keyer declares eyedropper parameters with `sample_stage` and a `preview_only` matte view. | The host samples and writes parameter values, and the guest never reads display pixels. Export always renders the final output. |
| TEXT-shape / TEXT-edit / TEXT-missing-font | Use brokered `text.fonts()` and `text.shape(font, str, features)` to get a `GlyphRun` with provenance (`engine_id`, font digest), within glyph and byte caps. Cover variable axes and colour glyphs. Persist the font reference and reopen without the font. | No font bytes reach the guest. Editable text survives save. Missing-font substitution is attributed. Extrusion works through `RND-scene-3d`. |
| AUD-port | A visual processor takes an `AudioWindow(source, range, layout)` input. Edit the audio. | Only the affected frames are invalidated. PC reports `Unavailable(NoAudioRole)`. |
| GEN-fill | Use a selection mask input, a local-model job and `edit.commit(newLayer)` on a still frame. | Behaves like `JOB-infer` and `JOB-readset-capture`. No network. One Undo. |

## Retirement

| ID | Scenario | Expected result |
|---|---|---|
| RET-corpus-coverage | Enumerate legacy ABI kinds, PC apply branches (channel, locked alpha, selection, Smart Filter, Action), EC ABI 1/2, Library layouts, script folders and settings files. | Each appears in at least one corpus member produced by phase 0f `legacy-corpus`. |
| RET-pc / RET-ec / RET-ec-cmds / RET-beta / RET-ec-scripts | Retire each legacy path in its own PR. | Corpus survives, zero callers, ids resolve or are tombstoned, settings are migrated once, and an eligible timing Pass. |
| RET-ec-library / RET-ec-library-mismatch | Migrate SHA-verified Library installs after consent, then a hash-mismatched one. | Zero `curl` children and `catalog.json` is never opened. The mismatch case gets an "origin unverified" adoption. `RUN-policy-diff` updated. |
| RET-headless / RET-mig-race | Open migrated documents through CLI and MCP. Race two app instances on migration. | Adoption records are honoured, or a typed refusal names the fix. A lock and an atomic marker produce one migration. |
| RET-stylize | Delete the native Stylize ids after `PAR-pc-stylize` passes on desktop and `WEB-broker` passes on the web. | Corpus survives, zero callers, ids resolve to the package, an eligible timing Pass, and the web build runs Stylize through the broker Worker. |
| RET-downgrade-pixels | Open package-written files in the last native release. | Same pixels and records. |
| FAULT-alias-spoof / FAULT-alias-spoof-legacy | A local package claims a bundled id, an adopted `plugin.run` id or a Library effect id. | Refused unless the user chooses it per id. Adoption binds (path, sha256, ABI). |
| FAULT-geometry | A processor, canvas or GPU pass returns invalid geometry: NaN or infinite coordinates, out-of-range indices and an oversized mesh. | Rejected before use with a typed error. The prior valid scene stays. Plus the [fault-grid](../contracts/conformance.md#fault-grid) invariants. |
| FAULT-script-loop | A `.jsx` script loops forever, once in a normal turn and once mid-asyncify unwind. | The Store recycles at the CPU cap with `Budget`. No hang, no backstop leak, and `cx.state` restores. Plus the fault-grid invariants. |
| FAULT-slot-collision | Two packages claim the same replaceable inspector slot and the same contribution key. | The user's selection owns the slot. The other contribution is attributed and inert. No crash or identity theft. Plus the fault-grid invariants. |
| FAULT-ui-flood | A panel publishes bodies, patches and actions far faster than the frame rate and past its byte budget. | Replaceable data coalesces and excess returns `Busy`. Terminal-event capacity holds and UI-load holds. Plus the fault-grid invariants. |
| FAULT-adapter-panic | An app adapter handler panics during an operation and during a query. | `Failed(HostFault)`, no strike, document unchanged. Plus the fault-grid invariants. |
| FAULT-autosave-kill | Kill the app during autosave publication through `craft-files`. | The journal replays. The previous or new autosave is intact, and its `Receipt` is truthful. Plus the fault-grid invariants. |
| FAULT-safe-mode | Start in safe mode with packages that own open document records and workspace panels. | No guest code runs. Records and panels are inert and attributed. The document saves losslessly. Plus the fault-grid invariants. |
| FAULT-quota | Exceed 64 KiB `cx.state`, 1,024 tickets, depth 64 and a 16 MiB envelope. | Typed `Budget`, `Busy` or `Invalid` errors before host allocation. Plus the fault-grid invariants. |
| LOG-redact / LOG-covert / LOG-author | Plant canary text in a document. A plugin with document read tries to signal a plugin with file write through profiles or timing aggregates. An author finds a slow tile. | The canary is absent from every host file, stdio, crash report and trace. No aggregate is readable by a guest. The self profile alone finds the slow tile. |

## Registry and aliases

Every test ID referenced in the canonical docs has exactly one defining row. An alias keeps one row, in its original acceptance file, that reads "Alias of X; budget per conformance tolerance table". Aliases:
- `UI01-frame-cost` → `CONF-cadence` (C gate);
- `UI02-cost` → `CONF-cadence`;
- `RUN-first-party` → `PAR-*`;
- `RUN-cost` and `RUN-cost-js` → `CONF-cold`;
- `RUN-service` → `RUN-service-xflow`;
- `JOB-lost-update` → `EDIT-lost-update`;
- the kill and revoke cases of `EDIT-atomic` → `T-crash-commit` (`EDIT-atomic` keeps its staging cases);
- `SDK-legacy-retire` and `DOC-legacy` → `RET-*`.

Former undefined references now have rows: `WASM-winarm64` (formerly `H7`) and `SCRIPT-ec-suite` in the [SDK acceptance](sdk-acceptance.md), `SDK-typed-ratchet` (formerly `Ratchet`), `RET-stylize`, and the eight FAULT rows above. Slice names are upper case (for example `UI02-CONTRACT`), so they never match the test-ID pattern. Overlapping budgets defer to the [tolerance table](../contracts/conformance.md#tolerances).
