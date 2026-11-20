# Runtime, identity and authority acceptance

Planned cases for [SYS-01/02 and RUN-01/02](../decisions/ADR-RUN-01.md). Run them on macOS, Windows, Linux, Flatpak and Docker CI where named, and on web and headless where advertised. None of them has run. Every mode stays `Unavailable` until its gate passes. Record the verification fields from the [evidence rules](../architecture/evidence-and-verification.md).

| ID | Scenario | Expected result |
|---|---|---|
| RUN-lint / RUN-classify-all / RUN-policy-diff | Build with the `authorize`, `automation_gate` and `automation_input` fields deleted. Run the clippy `disallowed-methods` list and enumerate every registry command. Replay the old automation policies. | The compiler lists every former site. Every command has (command, parameter role) sink classes. Each `#[allow]` appears in the CI-diffed allowlist. The old policy decisions are reproduced bit for bit. |
| RUN-sink | Through UI, CLI, MCP, scripts, headless, a plugin and a deferred job, invoke each sink class on a target the chain lacks. | Identical denial for every caller. No sink runs without an `Admitted<K>` token. |
| RUN-sink-launder | A plugin writes an action step, a linked smart object to `/Volumes/smb`, autofs or UNC, a script reference and a zipped `.atn`. The user later replays, exports or reveals. | Replay runs under `[User, P]`. Remote locators need a user relink. Spawn and Net are denied with an attributed message. Provenance is recovered from the journal digest after a zip round trip. |
| RUN-sink-binding | A plugin tries to set `allowScriptsWriteFiles`, bind `file.runScript` to a shortcut, or add a startup binding. | Security-relevant settings are denied. Bindings carry provenance, and pressing the key replays under the author chain. |
| RUN-replay-scope / RUN-adopt / RUN-paste-prov | Replay a plugin-authored action after uninstalling its plugin. Paste plugin-authored layers into another document. | Document edits on the user's target run. Other sinks open the adopt sheet. Provenance survives paste and import. |
| RUN-service | Alias of `RUN-service-xflow`; budget per conformance tolerance table. | |
| RUN-idle-call / T-hol | Run 1,000 sequential queries with the window idle, then with one plugin holding a long edit. | Under 100 ms in total. One plugin's edit never holds another's calls. |
| RUN-identity | Change one byte of a release. Relink a developer build. Install a second lineage with the same id. Downgrade. | New digest and `PrincipalEpoch`. Session and Persisted grants are re-issued under the new epoch; Invocation and Document grants end. Grants expand only with consent. The adopt sheet appears. Data is snapshotted on downgrade. A document reference never activates code. |
| RUN-lease-ro / RUN-late-write / RUN-handle-stash | Write into an input ring. Write an output slot after `Done`. Stash a handle and use it after its request ends. | The write is refused by the mapping. The host validates its own copy. Stale handles are typed errors. |
| RUN-bomb | Trap at start, loop forever, allocate past the limit, flood progress, emit giant or deep CBOR. | Store recycle or a typed error. The app UI p99 is unchanged. Documents are untouched. No deadline extension from progress. |
| T-cpu-vs-wall / T-sleep / T-trap-param / T-recycle-restore | Run honest turns under load, after sleep and in debug builds. Trap on one input value. Recycle a panel Store. | No false strikes. Negative cache for the trapping input. Panel state restored from `cx.state`. |
| T-hook-behind-command / T-service-grief / RUN-service-xflow / T-credit-deadlock | A floods B's service while B's hooks run. A tries to read C's data through B. Two chains reserve credits crosswise. | B's hooks stay within budget, and A receives `Budget` and strikes for its faults. Partitions prevent cross-head flow. No deadlock. |
| RUN-oversub | Eight principals render while EC plays back and PC paints. | UI-load and Export/throughput rows of the conformance tolerance table hold. Total runnable threads at most cores plus reserve. |
| NET-matrix / NET-seccomp-arch / NET-proc-mem / NET-dns-deputy | In each worker kind and its descendants, attempt TCP, UDP, loopback, DNS, mDNS, the framework control channel, `/proc`, other processes, exec, bus, URL open and foreign-arch syscalls. | Every attempt is denied, and the wire capture shows zero bytes. If a probe fails, that mode is `Unavailable(reason)`, with Wasm still available and the degraded OS layer disclosed. |
| NET-xprincipal / GPU-xprincipal | Principal A opens, signals or reads B's process, and reads B's GPU allocations. | Denied by SID, signal scope and per-principal device. Externally caused deaths are not strikes. |
| NET-cef-webrtc / CEF-no-windowserver / CEF-cost | Run WebRTC, prefetch, DNS and a window-server connection inside the CEF tree. Measure cold start and RSS. | Everything is denied. Software raster runs until the GPU probe passes. Cost is recorded against the hypothesis. |
| GPU-software-parity | Run the same WGSL on hardware and on the advertised software adapter. | Output within the declared tolerance. Software is advertised only where an ICD ships. |
| RUN-first-party | Alias of `PAR-*`; budget per conformance tolerance table. | |
| RUN-cost | Alias of `CONF-cold`; budget per conformance tolerance table. | |
| RUN-cost-js | Alias of `CONF-cold`; budget per conformance tolerance table. | |
| T-crash-commit / T-revoke-commit | Kill or revoke before and after document publication. | Before: `Failed(WorkerLost)` or `Revoked` with no Undo. After: the result is fetched idempotently. |
| PKG-macos-jit / PKG-spi | Per release: check the signed worker's entitlement and the seatbelt SPI. | Release is blocked if either check fails. |

Cost hypotheses, measured as paired same-machine baselines. Where they overlap the conformance absolute limits, those limits govern:

| Measure | Target |
|---|---|
| Spare bind | ≤ 2 ms |
| Cold confined spawn plus cached instantiate, p50 | ≤ 15 ms (≤ 30 ms on Windows) |
| Idle principal RSS | ≤ 15 MB plus guest memory |
| Fresh Store | ≤ 50 µs |
| Broker round trip, p50 | ≤ 30 µs |

For QuickJS-Wasm, measured under `CONF-cold`:

| Measure | Target |
|---|---|
| Snapshot start | ≤ 2 ms |
| 5k-node UI tree | ≤ 8 ms of CPU |
| 10 MB of JSON | ≤ 150 ms |
