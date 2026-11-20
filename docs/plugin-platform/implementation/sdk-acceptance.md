# SDK, evolution and target acceptance

Planned cases for [RUN-03/04 and SDK-01](../decisions/ADR-RUN-03.md). None of these has run. Each target cell stays `Unavailable` until its gate passes. Record the verification fields from [evidence rules](../architecture/evidence-and-verification.md).

| ID | Scenario | Expected result |
|---|---|---|
| SDK-idl-spike / SCHEMA-equiv | Generate locks with `#[craft::api]` for 50 representative operations. Proptest that serde, the JSON Schema, generated TS and the host validator accept and reject the same values. | Identical verdicts. Forbidden shapes fail lock export. |
| ENV-fuzz | Fuzz the prescan with indefinite lengths, unknown tags, duplicate keys, 2^32 headers, deep nesting, non-canonical bytes and 10^6 submits per turn, for 1 CPU-hour per operation type. Run prescan and serde as a differential. | No crash, hang or disagreement. `Busy` past 1,024 tickets. Hashes come from the host re-encoding only. |
| SDK-source-compat | Build the 1.0 examples against 1.N crates and TS types. | They compile unchanged. |
| SDK-downlevel-rmw | A 1.5 guest reads and writes objects carrying 1.6 semantic fields, governed fields, `Other` enum values, hidden list elements and forged `Other.raw` in closed or target types. | Patches keep unseen fields. Whole replace and governed patches return `NeedsLevel`. Forged values are refused. |
| EDIT-lost-update | Read a name, then the user renames, then the guest commits `p + old`. Repeat for tools, scripts and jobs. | `Stale(ReadKey[])`, no Undo. |
| SDK-major-forever / SDK-alias-persisted / IDMAP-props | Call `op@1` after `@2` and `@3` exist. Apply random rename sequences and reuse attempts across all four alias tables. | Results match after up/down mapping. No id is reused. Chains flatten. Older builds still read first-shipped ids. |
| SDK-save-refactor-reopen | The [refactor simulation](../contracts/sdk.md#refactor-simulation), including an argument rename. | Pixel digest within tolerance. The Action replays. Guest transcript, Undo and errors are identical. Only the listed plugin-platform files changed. |
| SDK-legacy-retire | Alias of `RET-*`; budget per conformance tolerance table. | |
| SCRIPT-ec-suite | Run the EC script suite and the bundled ScriptUI panels, including `Ease Presets.jsx`, through the asyncify `.jsx` shim on desktop, web without JSPI, and headless. Launch a registered `.jsx` from a plugin-owned launcher. | Every script passes with the same document result as boa. The launcher runs the script as its generated package's command contribution, under chain intersection. |
| SDK-script-sync / SCRIPT-reentry / SCRIPT-perf | Run the EC script suite and bundled ScriptUI panels, including `Ease Presets.jsx`. Leave a modal open for 10 minutes. Trigger hook-on-self, a double click and modal-in-modal. Run 1,000 `setValueAtTime` calls. Cover desktop, web without JSPI, and headless. | The suite passes. No backstop recycle while waiting. No hang longer than 1 s, and cycles get `Busy`. One coalesced Undo. At most 2× boa time. |
| WASM-profile | Run exact kernels on x64, arm64 and Pulley with 2 and 16 cores. Add a relaxed-SIMD module and browser results. | Identical exact digests in `Canonical`. Relaxed-SIMD runs preview only. No cross-class cache hits. NaN canonicalization costs at most 15%. |
| RUN-cache-isolation | Make `pc_manifest` and TS pre-init attempt to write cache entries, or emit code. | The compile run stays guest-free. The snapshot is only a validated memory image. |
| SDK-examples-7 / SDK-no-private | Compile the [seven examples](../contracts/sdk-examples.md) in Rust and TS, and run them on EC and PC where advertised. Scan public crates for app-private types. | They pass with only `plugin-api` and app API crates as dependencies. |
| SDK-permission-compile | Call an operation that needs a grant missing from `craft.toml`. | Compile error naming the `craft.toml` fix in Rust and TS. `craft test` fails on undeclared grants. |
| DEV-loop / DEV-debug | Edit a TS and a Rust panel while linked. Trap, then replay the transcript under lldb or DevTools. | TS within 1 s; Rust within cargo time plus 300 ms. The last body stays visible. The replay reproduces the trap with no authority. |
| WEB-broker | Page 50k layers on the web with the broker Worker. | Main thread spends at most 1 ms per frame on publication. Cells marked P remain `Unavailable` until this passes. |
| WEB-gpu | Run a WGSL processor through browser WebGPU in the broker Worker, beside the same processor on desktop. Lose the device. | Output within the declared tolerance under `Engine(id)`. Device loss retires the route with a typed reason. The web GPU cell stays P until this passes. |
| WASM-winarm64 | Run the exact kernels, the conformance kit and the `NET-matrix` Wasm column on Windows arm64 under Pulley and x64 under Prism. | The faster mode is chosen by measurement and `Available(Interpreted)` re-budgets. The cell stays P until this passes. |
| CAP-truth | For every A cell in the target matrix, run its workload. Remove the device or probe. | The workload runs. Removal changes `host.capabilities()` with a typed reason. Unmet contributions are `Unavailable` while the rest of the package works. |

Hypotheses:
- Validating, patching and downlevelling 4 KiB takes at most 20 µs at p95.
- About 1.7k operations generate at most 300 KB of `.d.ts`.
- A QuickJS Store starts from its image in at most 3 ms.
- A 1 MiB typed-array page decodes in at most 5 ms.
- The lock diff takes at most 5 s.
- Windows arm64 performance is measured before it is advertised.
