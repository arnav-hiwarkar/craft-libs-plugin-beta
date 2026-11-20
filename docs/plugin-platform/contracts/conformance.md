# Conformance, measurement and observability contract

Designed in [RUN-05/SDK-02](../decisions/ADR-SDK-02.md). Nothing has run. All numbers are hypotheses until an eligible `RunRecord` exists.

Related: [Evidence rules](../architecture/evidence-and-verification.md) · [Tests](../implementation/conformance-acceptance.md) · [Phases](../implementation/phases.md)

```text
RunRecord = {test_id, run_id, app_pin, fw_rev, os, runner_image, night,
             workload: {fixture_hash, mode, backend, load, scale},
             block, arm: A0|A|B|AA, gate, refresh_hz, samples: Hist | Count(events, frames)}
Verdict   = Pass | Fail | Inconclusive(Noisy|Budget|NoRunner)
allow     = latency gate: max(floor[gate], fraction[gate] × q_A), capped by ceiling[gate]
            rate gate:    floor[gate]
Δ         = q_B − q_A or rate_B − rate_A, seeded 10k stratified block bootstrap
```

## Harness and probes

`pair` lives in CL `testkit`, and both apps take it as a dev-dependency. It owns:
- log-bucket histograms, 1 µs to 60 s, with at most 1% error;
- pairing and the block bootstrap;
- the verdict;
- the frame series (`pair::frames`), which every app shares.

### Frame series

An app stamps its own frame loop and hands `pair::frames` one `Frame` per frame: its start (the `raw_input_hook`), the wall time of each of its C stages, the L of an input the frame consumed, the P playback count the frame closed, and the P seek latency the frame ended. The module turns a series into what the gates read:
- C per stage, and the C total as the sum of the stages;
- I from consecutive starts, each interval classed `on time`, `missed` or `long stall` by exact integer comparison with I in nanoseconds: missed when 2 · I · hz > 3 · 10⁹, long stall when I · hz > 4 · 10⁹ and I > 50 · 10⁶;
- one `RunRecord` per gate the workload names and per block, with test id `CONF-calibrate`, for `OverheadCpu`, `OverheadInterval`, `UiEqMissed`, `UiEqLongStalls`, `UiEqLatency`, `PlaybackLate` and `PlaybackSeek`; any other gate is an error;
- ABBA blocks labelled A, AA, AA, A, the A/A calibration of one binary;
- the A/AA verdict per gate and a Markdown summary of each stage's p50, p95 and p99;
- the run driver shared by the apps' benches: options, stale-output clearing, the load label, record identity, and a `Run` that measures each workload in each ABBA block through the app's measure function, pools the blocks, refuses a workload that changes between them, and writes and prints the records and the summary. An app only spawns its bench and reads the bench's report into `Frame`s, rejecting a frame whose start or stage time is not a finite, non-negative number of milliseconds.

The C stage set is the app's. PhotoCraft's is `input`, `ui_build`, `command`, `damage`, `composite`, `upload`. EffectCraft's is `input`, `logic`, `ui_build`, `upload`, where `upload` is putting viewer frames on screen: CPU frames into textures, GPU frames registered as textures, and the display pass. egui's end-of-pass tessellation and eframe's paint and present run inside eframe after the app returns, so no stage holds them; they show only in I.

### The pair engine

`craft_testkit::pair` decides one verdict from a pool of `RunRecord`s. One record holds one arm's samples of one gate's metric from one block. The class is the gate's row of the [gate table](#tolerances), so a record names its gate and not its class.

- **Histogram.** A latency gate's samples are a `Hist`: log-linear buckets from 1 µs to 60 s inclusive, 64 buckets per power of two, 1,728 buckets in all, each read at its midpoint, so a quantile is at most 0.78% from the sample's own. Samples below 1 µs count as underflow and samples above 60 s as overflow. A quantile is the nearest-rank sample, at rank ⌈p × n⌉.
- **Counts.** A rate gate's samples are a count of events in frames, with events ≤ frames. The app counts the events: missed frames (I > 1.5 × period), long stalls (I > max(4 × period, 50 ms)) and late playback frames (frames not on time against the preview's clock). A rate is events ÷ frames, in parts per billion; 1 pp is 10,000,000.
- **Digest.** Every record of a pool shares `test_id`, `app_pin`, `fw_rev`, `os`, the whole `workload` (`fixture_hash`, `mode`, `backend`, `load`, `scale`), `gate` and `refresh_hz`. Records with different workloads are never pooled. A pool holds every record of its digest: the harness passes all of them, never a chosen subset.
- **Pool.** The arms are exactly one contrast: A against B, A0 against A, or A against AA, the first named being the base. A digest mismatch, a repeated `run_id`, another set of arms, samples of the wrong kind for the gate, or a block missing an arm is an error, not a verdict.
- **Units and strata.** Records with the same `night`, `runner_image` and `block` form one unit, normally one ABBA quartet. The units of one `night` and `runner_image` form a stratum. A stratum with fewer than two units cannot show its own noise; it is left out and counted in the measurement's `excluded_strata`. A pool with no stratum left is `Inconclusive(Noisy)`.
- **Seed.** The bootstrap seed is the 64-bit FNV-1a hash of one line per record, sorted and each ended by a newline. The line holds only what affects the verdict: the record's stratum and unit ordinals (strata ordered by night and runner image, units by block), its arm, gate, workload, `refresh_hz` and samples. Labels such as `run_id`, `test_id` and the night's text are left out, so renaming them leaves the decision bit-identical, the same records in any order give the same verdict, and a caller cannot choose a seed.
- **Bootstrap.** 10,000 resamples from SplitMix64. Each resample draws n − 1 units with replacement from every stratum of n units and gives each drawn unit the weight n ÷ (n − 1) per draw (Rao and Wu's rescaling), so every stratum keeps its share of the pool and the variance of a linear statistic is unbiased. A resample's Δ is the base and test quantile of the weighted merged histograms, or the weighted rates.
- **Interval.** Δ̂ ± (t × s + w), where Δ̂ is the whole pool's Δ, s the standard deviation of the 10,000 resampled Δ, and t Student's two-sided 99% point with (units − strata) degrees of freedom. 99% is 1 − 0.05/K with K = 5 looks. For a latency gate, w is the sum of the half-widths of the base and test quantiles' buckets, so constant units still have a nonzero interval; for a rate gate, w is 0. A rate gate also adds the binomial noise of its events: b = z × √(v_A + v_B) with z = 2.576, where each arm's v is the Wilson-adjusted variance p̃(1 − p̃) ÷ (F + z²), p̃ = (E + z²/2) ÷ (F + z²), from the arm's E events in F frames over the whole pool. The interval is then Δ̂ ± (√((t × s)² + b²) + w), so a pool of a few events stays inconclusive even when its units agree. A percentile interval was rejected: with three units a night it gave 23 false Passes in 200 trials of `CONF-aa-sequential`.
- **Out of range.** A quantile in the underflow reads as 0 with a half-width of 1 µs. A test-arm quantile in the overflow reads as 60 s and makes the upper bound unbounded: Δ is at least that much, so Fail can fire and Pass cannot. A base-arm quantile in the overflow is `Inconclusive(Noisy)`.
- **Looks.** Each distinct `night` in the pool is one look. A pool with a sixth night is `Inconclusive(Budget)`.
- **Allowance.** `allow` comes from the base quantile of the whole pool and the gate's row. The interval never changes it.
- **Decision.** `Measured` carries the unit (nanoseconds or parts per billion), `allow`, Δ̂, the bounds and `excluded_strata`; its verdict follows the table in [eligibility and verdicts](#eligibility-and-verdicts). `Withheld` carries `Noisy`, `Budget` or `NoRunner`.

#### Record lines

A nightly pool persists as UTF-8 text, one record per line, each line ended by `\n`. A line is at most 64 KiB: the version `craft-pair/1`, then these `key=value` fields in this order, separated by single spaces:

```text
craft-pair/1 test_id= run_id= app_pin= fw_rev= os= runner_image= night= fixture_hash= mode= backend= load= scale= block= arm= gate= refresh_hz= samples=
```

- Identity values are 1 to 128 characters from `A–Z a–z 0–9 . _ : / + @ -`.
- `block` and `refresh_hz` are decimal without leading zeros; `refresh_hz` is above zero.
- `arm` is `A0`, `A`, `B` or `AA`. `gate` is a name from the gate table.
- `samples` is `hist:` followed by `slot:count` pairs joined by commas, slots ascending, counts above zero and summing to at most 2^64 − 1, or `count:events/frames` with events ≤ frames. Slot 0 is the underflow, slots 1 to 1,728 are the buckets in order, and slot 1,729 is the overflow. An empty histogram is `hist:`.
- The samples' kind matches the gate's kind.

A reader rejects anything else with the line number and the problem, and an empty line inside the text is an error. A pool text holds at most 20,000 records and 64 MiB; a nightly pool of five nights, 32 ABBA units a night and four blocks a unit is 640 records of a few KiB each. Past either cap the reader stops with `TooManyRecords` or `PoolTooLarge` and the line where the cap was passed.

The apps own their probes. PC extends `frame_stages.rs`. EC replaces its FPS EMA at `app/frame.rs:318` and stamps synthetic input on its own control queue. Runs are driven by PC `xtask perf --pair` and EC `bench --pair`.

| Metric | Definition |
|---|---|
| C | UI-thread wall time per stage of the app's stage set (see [Frame series](#frame-series)), plus plugin_host[principal] once the host runs. Stages do not overlap, and the frame-rate cap sleep belongs to no stage. Wall time rather than thread CPU time: blocking waits inside a stage delay the frame as much as computation does, and it needs no per-OS clock code in the apps. The C total is the sum of the stages |
| I | Interval between `raw_input_hook`s under continuous drag, animation or playback. A frame is `missed` when I > 1.5 × period, and a `long stall` when I > max(4 × period, 50 ms). |
| L | App-local stamp of synthetic input to the end of the pass that consumes it |
| P | EC playback on-time ratio against the audio clock, and seek-to-viewer-frame latency. Each comp frame the preview's clock passes during playback counts once, and it is late unless the viewer showed it while the clock was on it. The clock is the audio clock when the preview plays sound, and otherwise the preview's own pacing clock: its frame rate from where it last started, which keeps running while the preview holds on a frame that is not rendered, so a stall's frames count as late. A seek, a restart or an audio start begins a new clock generation and counts nothing for the jump. Within a generation a loop wrap counts the frames to the end and from the start, and any other step back counts nothing. A seek runs from the start of the frame whose input moved the current time while paused to the end of the frame whose viewer shows the new time's frame; a later seek replaces one still waiting |
| gpu_ns | Timestamp queries on AppDevice routes and the plugin leaf |

The A and B arms run in one binary under distinct ids until cutover. B ids exist only under a dev cfg, and release `engine.lock` refuses to persist them. Blocks of about 60 s run ABBA. Each arm needs at least 10k frames or 30 render repetitions. Records assert WorkMeter(B) > 0 and WorkMeter(A) = 0. An A0 arm, with the host never started, is compared with 0, 10 and 100 idle packages.

## Eligibility and verdicts

A (gate, runner image) is eligible when:
- at least 59 in-job A/A verdicts show zero Fail, an upper 95% bound of 5%;
- its positive controls behave: an injected CPU delay of 2× the allowance and a GPU-heavy canvas must Fail, and the null case must Pass.

Any image change requires recalibration. In-job A/A verdicts and control verdicts come from the same engine without the eligibility check. An `Inconclusive` A/A verdict counts toward the 59, because it is not a Fail. A pool is `Inconclusive(NoRunner)` unless every runner image in it has an eligible calibration for the pool's gate.

| Result | Condition |
|---|---|
| Pass | The CI upper bound is within `allow` |
| Fail | The CI lower bound is above `allow` |
| Inconclusive | Anything else |

Pooled nights allow at most five looks per digest, at level 1 − 0.05/K, with the bootstrap stratified by night and image. Noise never widens `allow`.

**Counters.** These gate every slice on hosted CI:
- copy bytes per boundary at or below the plan;
- zero UI-thread fence waits;
- zero wakeups and zero processes while disabled;
- exact leak counts;
- envelope and ticket bounds;
- zero AppDevice cross-device bytes.

**Inconclusive and failing results.** `Inconclusive` blocks only RET and release claims. At a phase exit it becomes a `DEFECT` due the next phase. A Fail on main opens a `DEFECT` automatically.

**Runners.** Laptops never produce verdicts. Hosted runners serve wherever they are eligible. A self-hosted reference machine is optional: owner-authorised, ephemeral, dispatch only, never on `pull_request*`, no secrets. `CI-trigger-lint` enforces this. Schedules carry a per-phase approval reference and stay within a global macOS budget queued by pending RET.

## Tolerances

One table, read by `pair`. A latency gate compares a quantile and allows `max(floor, fraction × q_A)`. A rate gate compares event rates and allows its floor. One refresh is 1 s divided by `refresh_hz`, rounded down to the nanosecond. A ceiling c applies when q_A ≤ c and lowers `allow` to at most c − q_A: B p95 stays at or below 100 ms whenever A p95 does.

| Gate | Class | Metric | Kind | Quantile | Floor | Fraction of q_A | Ceiling |
|---|---|---|---|---|---|---|---|
| `UiEqCpu` | UI-eq | C plugin | latency | p95 | 1 ms | 10% | none |
| `UiEqGpu` | UI-eq | leaf gpu_ns per frame | latency | p95 | 1 ms | 10% | none |
| `UiEqLatency` | UI-eq | L | latency | p95 | one refresh | 10% | none |
| `UiEqExtractionLatency` | UI-eq | L of an extraction | latency | p95 | 2 ms | 10% | none |
| `UiEqMissed` | UI-eq | I missed frames | rate | none | 0.5 pp | none | none |
| `UiEqLongStalls` | UI-eq | I long stalls | rate | none | 0.01 pp (1 per 10k frames) | none | none |
| `UiLoadMissed` | UI-load (others busy or bombing) | I missed frames | rate | none | 1 pp | none | none |
| `UiLoadLatency` | UI-load | L | latency | p95 | one refresh | 10% | none |
| `UiLoadStrokeLatency` | UI-load | PC stroke L | latency | p95 | 2 ms | 10% | none |
| `OverheadCpu` | Overhead (A0 vs A, 0, 10 and 100 idle packages) | C | latency | p50 | none | 15% | none |
| `OverheadInterval` | Overhead | I | latency | p50 | none | 15% | none |
| `PlaybackLate` | Playback (EC) | P frames not on time | rate | none | 1 pp (on-time Δ ≥ −1 pp) | none | none |
| `PlaybackSeek` | Playback (EC) | seek | latency | p95 | 50 ms | 10% | none |
| `CommandWallMedian` | Command/commit | wall | latency | p50 | 50 ms | 10% | none |
| `CommandWallTail` | Command/commit | wall | latency | p95 | 50 ms | 10% | 100 ms |
| `ExportWall` | Export/batch/throughput, GPU route | wall | latency | p50 | none | 10% | none |
| `ExportGpu` | Export/batch/throughput, GPU route | gpu_ns | latency | p50 | none | 10% | none |

The kernel class (SDK versus native throughput) is not gated; below 0.6× opens a `DEFECT`.

Each verdict runs at 6 MP and 1080p. One confirmation pair at 24 MP and 4K must not Fail.

Absolute limits:

| Area | Limit |
|---|---|
| Spare bind | ≤ 2 ms |
| Cold spawn p50 | ≤ 15 ms (≤ 30 ms on Windows) |
| QuickJS image start | p95 ≤ 3 ms |
| Enable to first body | p95 ≤ 150 ms |
| Idle principal | ≤ 15 MB plus guest memory |
| Leaks | Baseline exactly after 1,000 cycles; RSS ≤ 4 KiB per cycle |
| Commit at 50 keys | Owner p99 < 2 ms |
| 1,000 idle queries | < 100 ms |
| Audio callback | Zero guest work; p99 ≤ 25% of the period as the gate, with 10% as the ADR-RENDER-03 target; zero underflows |
| Document open | Δ ≤ 5% |

These supersede the overlapping numbers in earlier acceptance files. The [registry](../implementation/conformance-acceptance.md#registry-and-aliases) records the aliases.

## Work budgets

WorkMeter charges each (principal, request) at choke points the guest cannot bypass:
- broker decode;
- ring grant (`px_staged` = pixels × taps);
- credit ledger;
- limiter;
- GPU provider;
- text shaping (glyphs and bytes);
- UI publish;
- outbox fanout.

The runtime caps apply per host-sized unit:

| Cap | Value |
|---|---|
| Turn CPU | 50 ms / 100 ms / 5 s / 60 s / 30 s |
| Envelope size | 16 MiB |
| Nesting depth | 64 |
| Tickets | 1,024 |
| `cx.state` | 64 KiB |
| Credits | All-or-nothing |

Legacy shims use:
- CPU: `max(contract cap, legacy fuel / R_min)`, where legacy fuel is PC 50M + 4k/sample/band and EC 50M + 20k/pixel, and `R_min` is fixed from the slowest class;
- memory: the larger of the contract and the legacy limit (PC 512 MiB per instance and 2 GiB total, EC 1 GiB).

Interactive preview keeps the 5 s tile cap and returns `Degraded`.

## Fault grid

Cells are fault class × mode {Wasm, Job, Native, HTML, Host} × OS. They are generated from the capability table. A cell is required where the mode is advertised, and otherwise records `NA(reason)`. Every fault must leave:
- the document unchanged, or with exactly one Undo;
- a typed error;
- UI-load holding;
- resources back at baseline.

| Class | Tests |
|---|---|
| Malformed input, invalid geometry | `ENV-fuzz`, `UI02-update`, `RUN-bomb`, `FAULT-geometry` |
| Trap, panic, loop (including mid-asyncify) | `RUN-bomb`, `T-trap-param`, `T-cpu-vs-wall`, `FAULT-script-loop` |
| Memory, GPU, device loss, TDR | `JOB-oom`, `JOB-model-bomb`, `RND-gpu`, `UI02-device`, `JOB-gpu-tdr` |
| Kill, cancel, stale grant | `T-crash-commit`, `UI01-recovery`, `JOB-cancel-race`, `EDIT-cancel`, `T-revoke-commit`, `RND-authority` |
| Network | `NET-matrix`, `NET-dns-deputy`, `NET-cef-webrtc` |
| Contention, collision, flood | `RUN-oversub`, `JOB-fair`, `T-service-grief`, `T-credit-deadlock`, `FAULT-slot-collision`, `FAULT-ui-flood` |
| Host adapter panic | `FAULT-adapter-panic`: `Failed(HostFault)`, no strike |
| Autosave kill mid-publication | `FAULT-autosave-kill` |
| Storage | `IO-disk-full-autosave`, `IO-usb-replug`, `IO-rename-delete` |
| Audio device | `AUD-device` |
| Re-entry and late writes | `SCRIPT-reentry`, `RUN-late-write`, `RUN-handle-stash`, `T-hook-behind-command` |
| Lifecycle | `T-recycle-restore`, `JOB-save-during`, `JOB-revoke`, `DEV-loop` |
| Quota | `FAULT-quota` (64 KiB state, 1,024 tickets, depth 64, 16 MiB) |
| Safe mode, alias spoof | `FAULT-safe-mode`, `DOC-missing`, `FAULT-alias-spoof`, `FAULT-alias-spoof-legacy` |
| Corrupt state, migration, headless | `DOC-roundtrip`, `SDK-downlevel-rmw`, `T-sleep`, `RET-mig-race`, `RET-headless` |

## Author observability

**Logs.**
- Each principal gets a log ring of 64 KiB per turn and 1 MiB in total. Log text is treated as document data.
- A lint keeps host and adapter crates to logging typed codes only, never guest strings. That closes the EC `library.rs:577` → crash tail route.
- Crash reports carry a principal hash, a digest prefix and a fault code.
- Logs leave the app only through the user's Export sheet, which says the logs contain document data.
- Headless stdout needs `--plugin-logs`.

**Profiles.**
- `cx.profile.self()` and `craft profile` return the caller's own WorkMeter at 1 ms granularity.
- Aggregate C and I data appears only in host Diagnostics.
- Span names are interned from the manifest. Perfetto exports contain no document bytes.

**Transcripts.**
- Recorded for Developer links or behind a time-limited toggle.
- Kept in a profile plugins cannot read, at most 64 MiB, expiring after 7 days.
- `craft replay` runs without authority.

## Coverage matrix and registry

The registry and lint live in CL `plugin-conformance`. The canonical design docs move to CL `docs/plugin-platform/` in phase 0a.

Test IDs match `^(T|[A-Z][A-Z0-9]*)-[a-z][a-z0-9]*(-[a-z0-9]+)*$`. Topic IDs such as `UI-01` are a separate namespace. Each ID has one definition and is never reused; a rename keeps `alias_of`. Slash-joined headings expand to separate IDs, and a wildcard expands to at least one ID, all of which must pass.

A test ID is defined by the first cell of a body row in a table whose first header cell is `ID`, in `implementation/*-acceptance.md`. A test ID is referenced by an inline code span holding exactly one ID or one wildcard, such as `RUN-sink` or `PAR-*`. Prose words such as UI-thread are never IDs, because they are not code spans. A bare word outside a code span that equals a defined ID is reported as an unmarked reference, except in its defining cell. Every acceptance file needs at least one `ID` table row.

A question is open when the first word of its `Closure` cell, without punctuation or markup, is `Open` in any case, or when the cell starts with `Still open`. Every question table headed `ID` has a `Closure` column. A closed question links a document under `decisions/` and one under `contracts/`. [Question closure](../question-closure.md) needs at least one `Closure` row.

A proof row is a body row of a table whose first header cell is `ID`, in [the proof inventory](../implementation/proof-inventory.md), whose first cell starts with `REF-`. Its `Target` cell must contain `EC`, `PC` or `Both` as a word. The inventory needs at least one proof row.

A relative link must resolve, after `..` is applied inside `docs/plugin-platform/`, to a Markdown file of that set, with the exact case. Its anchor must match a heading slug of the target by GitHub's rules. Absolute paths and links that leave the set are reported. Links with a URL scheme are not checked.

Each row is generated from [question closure](../question-closure.md) and the registry:

```text
question → adr#anchor → contract#anchor → adapter{ec,pc}: slice-id | Unavailable(reason) | CONTRACT_DEPENDENCY:<id>
         → tests[] → refs[] → state: DESIGNED|IMPLEMENTED|TESTED → run_ids[]
```

The lint fails on:
- an undefined, duplicate or ill-formed ID;
- an empty cell;
- a REF without per-app cells;
- IMPLEMENTED without a slice and `host=app`;
- TESTED without a successful registered workflow run whose head SHA equals the pin and whose OS and fixture match;
- a generated fault cell with neither a run nor a reason;
- a decrease in the typed-handler ratchet;
- a performance ID with no class;
- a `DEFECT` without an owner and an exit phase;
- a new standard UI node with fewer than two REF users.

A `DEFECT` is a body row of a table whose first header cell is `Defect`, in any file of the canonical docs. The table has `Owner` and `Exit phase` columns, and the lint reports a row where either is empty:

```text
| Defect | Owner | Exit phase | Summary |
|---|---|---|---|
| DEFECT-<n> | <person or role> | <phase from the phase plan> | <what fails, and on which ID> |
```
