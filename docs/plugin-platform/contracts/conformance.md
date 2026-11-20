# Conformance, measurement and observability contract

Designed in [RUN-05/SDK-02](../decisions/ADR-SDK-02.md). Nothing has run. All numbers are hypotheses until an eligible `RunRecord` exists.

Related: [Evidence rules](../architecture/evidence-and-verification.md) · [Tests](../implementation/conformance-acceptance.md) · [Phases](../implementation/phases.md)

```text
RunRecord = {test_id, run_id, app_pin, fw_rev, os, runner_image, night, class, gate, mode, backend, fixture_hash,
             arm: A0|A|B|AA, block, load, refresh_hz, metrics, work, outcome}
Verdict   = Pass | Fail | Inconclusive(Noisy|Budget|NoRunner)
allow     = max(floor[gate], fraction[gate] × q_A), capped by ceiling[gate]
Δq        = q_B − q_A, seeded 10k block bootstrap
```

## Harness and probes

`pair` lives in CL `testkit`, and both apps take it as a dev-dependency. It owns:
- log-bucket histograms, 1 µs to 60 s, with at most 1% error;
- pairing and the block bootstrap;
- the verdict.

### The pair engine

`craft_testkit::pair` reads these fields of a `RunRecord`: the identity strings `test_id`, `run_id`, `app_pin`, `fw_rev`, `os` and `runner_image`, plus `night`, `block`, `arm`, `gate`, `refresh_hz` and one latency histogram of the gate's metric. One record holds one arm's samples from one block.

- **Histogram.** Log-linear buckets from 1 µs to 60 s inclusive: 64 buckets per power of two, 1,728 buckets in all, each read at its midpoint, so a quantile is at most 0.78% from the sample's own. Samples below 1 µs count as underflow and samples above 60 s as overflow. A quantile is the nearest-rank sample, at rank ⌈p × n⌉.
- **Pool.** Every record of one verdict shares `test_id`, `app_pin`, `fw_rev`, `os`, `gate` and `refresh_hz`: that is the digest. The arms are exactly one contrast: A against B, A0 against A, or A against AA, the first named being the base. Anything else is an error, not a verdict.
- **Units and strata.** Records with the same `night`, `runner_image` and `block` form one unit, normally one ABBA quartet, and a unit needs samples in both arms. The units of one `night` and `runner_image` form a stratum. A stratum with fewer than two units is `Inconclusive(Noisy)`, because one unit cannot show its own noise.
- **Bootstrap.** 10,000 resamples from a SplitMix64 generator seeded by the caller. Each resample draws n − 1 units with replacement from every stratum of n units and takes Δq of the merged histograms. Drawing n − 1 rather than n keeps the bootstrap variance unbiased when a stratum holds only a few units.
- **Interval.** Δ̂ ± t × s, where Δ̂ is Δq of the whole pool, s is the standard deviation of the 10,000 resampled Δq, and t is Student's two-sided 99% point with (units − strata) degrees of freedom. 99% is 1 − 0.05/K with K = 5 looks. A percentile interval was rejected: with three units a night it gave 23 false Passes in 200 trials of `CONF-aa-sequential`.
- **Readings.** A quantile in the underflow reads as 0. A quantile in the overflow, in the pool or in any resample, makes the verdict `Inconclusive(Noisy)`.
- **Looks.** Each distinct `night` in the pool is one look. A pool with a sixth night is `Inconclusive(Budget)`.
- **Allowance.** `allow` comes from the base quantile of the whole pool and the gate's row of the [gate table](#tolerances). The interval never changes it.

The apps own their probes. PC extends `frame_stages.rs`. EC replaces its FPS EMA at `app/frame.rs:318` and stamps synthetic input on its own control queue. Runs are driven by PC `xtask perf --pair` and EC `bench --pair`.

| Metric | Definition |
|---|---|
| C | UI-thread CPU per stage: input, ui_build, plugin_host[principal], command, damage, composite, upload |
| I | Interval between `raw_input_hook`s under continuous drag, animation or playback. A frame is `missed` when I > 1.5 × period, and a `long stall` when I > max(4 × period, 50 ms). |
| L | App-local stamp of synthetic input to the end of the pass that consumes it |
| P | EC playback on-time ratio against the audio clock, and seek-to-viewer-frame latency |
| gpu_ns | Timestamp queries on AppDevice routes and the plugin leaf |

The A and B arms run in one binary under distinct ids until cutover. B ids exist only under a dev cfg, and release `engine.lock` refuses to persist them. Blocks of about 60 s run ABBA. Each arm needs at least 10k frames or 30 render repetitions. Records assert WorkMeter(B) > 0 and WorkMeter(A) = 0. An A0 arm, with the host never started, is compared with 0, 10 and 100 idle packages.

## Eligibility and verdicts

A (class, gate, runner image) is eligible when:
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

| Class | Gate | Floor |
|---|---|---|
| UI-eq | C plugin p95, I missed Δ, long stalls Δ ≤ 1 per 10k frames, L p95, leaf gpu_ns | 1 ms, 0.5 pp, one refresh; extractions have an L floor of 2 ms |
| UI-load (others busy or bombing) | I missed Δ, L p95, PC stroke L p95 | 1 pp, one refresh, 2 ms |
| Overhead (A0 vs A) | C and I with 0, 10 and 100 idle packages | 15% p50 in both apps |
| Playback (EC) | P on-time Δ, seek p95 | −1 pp, 50 ms |
| Command/commit | Wall p50 and p95 | 50 ms, with B p95 ≤ 100 ms whenever A p95 ≤ 100 ms |
| Export/batch/throughput, GPU route | Wall, gpu_ns | 10% |
| Kernel | SDK versus native throughput | Not gated; below 0.6× opens a `DEFECT` |

`pair` implements the latency-quantile gates of this table. Its floors, fractions and ceilings:

| Gate | Row | Quantile | Floor | Fraction of q_A | Ceiling |
|---|---|---|---|---|---|
| `UiEqCpu` | UI-eq, C plugin | p95 | 1 ms | 10% | none |
| `UiEqLatency` | UI-eq, L | p95 | one refresh | 10% | none |
| `UiEqExtractionLatency` | UI-eq, L of an extraction | p95 | 2 ms | 10% | none |
| `UiLoadLatency` | UI-load, L | p95 | one refresh | 10% | none |
| `UiLoadStrokeLatency` | UI-load, PC stroke L | p95 | 2 ms | 10% | none |
| `OverheadCpu` / `OverheadInterval` | Overhead, C and I | p50 | none | 15% | none |
| `PlaybackSeek` | Playback, seek | p95 | 50 ms | 10% | none |
| `CommandWallMedian` | Command/commit, wall | p50 | 50 ms | 10% | none |
| `CommandWallTail` | Command/commit, wall | p95 | 50 ms | 10% | 100 ms |
| `ExportWall` / `ExportGpu` | Export/batch/throughput, GPU route | p50 | none | 10% | none |

One refresh is 1 s divided by `refresh_hz`, rounded down to the nanosecond. A ceiling c applies when q_A ≤ c and lowers `allow` to at most c − q_A, which is the row's rule that B p95 stays at or below 100 ms whenever A p95 does. The rate gates (I missed Δ, long stalls, P on-time Δ) and UI-eq leaf gpu_ns, whose row gives no floor, are not `pair` gates yet.

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
