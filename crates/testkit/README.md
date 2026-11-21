# craft-testkit

Source-level platform regression guards and verification utilities for the Craft application suite.

## Overview

`craft-testkit` provides lightweight, non-panicking source guards that prevent applications in the Craft ecosystem from repeating platform defects that were previously diagnosed and resolved in peer applications:

1. **`predefined_quit`**: Flags direct calls to `muda`'s `PredefinedMenuItem::quit` outside comments, strings and character literals, also with whitespace or a line break around `::`, in the qualified form `<muda::PredefinedMenuItem>::quit`, and through a `use muda::PredefinedMenuItem as ...` rename. A type of the same name reached through another crate or module, such as `other::PredefinedMenuItem::quit`, is not flagged. On macOS, this predefined menu item routes directly to `NSApplication terminate:`, completely bypassing application dirty document checks and unsaved changes save prompts. Applications should instead wire Quit through `craft-quit` lifecycle interceptors.
2. **`wgsl_float_literals` / `wgsl_float_literals_in`**: Scans WGSL shader sources and flags decimal and hexadecimal float literals whose value, rounded to `f64`, exceeds `f32::MAX` (such as `3.40282347e38` or `0x1p+500f`), or 65504, the largest finite `f16`, for a literal with an `h` suffix. A literal that exceeds the maximum by less than `f64` precision rounds to the maximum and is not reported. Integer literals are not checked. Some native compilers round such literals down, but browser WebGPU compilers reject them with pipeline compilation errors, forcing applications into CPU fallback rendering. `f32::MAX` can be written as the hex float `0x1.fffffep+127f`, which equals it, or as the decimal `3.4028234663852886e38`, which rounds to it.
3. **`std_instant`**: Flags `std::time::Instant` outside comments and strings: the full path, `<m>::Instant` where a `use` binds `m` to `std::time`, and each `use` that imports `std::time::Instant` or `std::time::*`, under any name. The parts of a path may be split by whitespace, comments or line breaks. Other crates' `Instant` (such as `web_time::Instant` or `tokio::time::Instant`) is not flagged, and neither is an item whose `cfg` attribute is false on `wasm32-unknown-unknown`; code before that attribute or after its item on the same line is still checked. A `mod name;` inside such an item also skips the module's file (`name.rs` or `name/mod.rs`, or the file a `#[path]` attribute names) and the files of the modules declared in it, when they are among the files passed in and no declaration outside such items also reaches them. On that target, `std::time::Instant::now()` panics at runtime with `"time not implemented on this platform"`. Code targeting the web must use `web_time::Instant`.
4. **`rust_sources`**: Recursively discovers a crate's `.rs` source files while excluding integration tests (`tests/`), test modules (`tests.rs`, `tests_*.rs`, `*_tests.rs`, `*_test.rs`), build artifacts (`target/`, any folder holding a `CACHEDIR.TAG` file that starts with the line `Signature: 8a477f597d28d172789f06886806bc55`, `.git/` and other folders whose name starts with `.`) and symbolic links, which it does not follow. A file over 8 MiB, more than 256 MiB in all or more than 50,000 files returns an error; `rust_sources_with_limits` takes other limits. It is not built for `wasm32`.

## Epistemic Scope: Syntactic Guards vs Runtime Validation

Source-level guards provide deterministic syntactic evidence about codebase structure:

### What They Establish
- Checked source files do not contain forbidden symbol invocations, known out-of-range literal representations, or platform-hazardous imports.
- Reusable, deterministic checks that run in milliseconds as regular Cargo tests without external runners.

### What They Cannot Establish
- **Runtime execution safety**: Syntactic checks do not verify whether code branches actually execute at runtime.
- **Semantic type resolution**: The scanners follow `use` renames within one file only. They do not resolve paths through other modules or re-exports, a rename of `std` itself, or expand macros.
- **Complex target predicates**: `std_instant` evaluates `cfg` predicates (`all`, `any`, `not`, `unix`, `windows` and the `target_*` keys) for `wasm32-unknown-unknown`, on outer attributes for the item that follows and on inner attributes (`#![cfg(...)]`) for the rest of the module, and on `mod name;` declarations for the module's file and its descendants. It compares file paths as text, so the files must come from one `rust_sources` call or be named the same way. A predicate that depends on a feature or on `test` is not known from the source, so its item is checked. Applications should scope `std_instant` tests to the specific crates that compile into their web distribution.
- **Driver and host OS behavior**: Syntactic checks do not verify WebGPU driver compatibility or host window manager event dispatch.

Source guards serve as fast, zero-dependency regression gates in application test suites, complementing platform integration tests and CI workflows.

## Consumer Adoption Examples

### 1. PhotoCraft: Adopt `wgsl_float_literals` and `predefined_quit`

In `photocraft/crates/gpu/src/lib.rs`, replace the in-tree `decimal_float_literals` helper with `craft_testkit::wgsl_float_literals`:

```rust
#[test]
fn shader_float_literals_fit_f32() {
    let bad = craft_testkit::wgsl_float_literals(SHADER);
    assert!(
        bad.is_empty(),
        "float literals outside f32 finite range reject on WebGPU (Tint):\n{}",
        bad.iter().map(|f| f.to_string()).collect::<Vec<_>>().join("\n")
    );
}
```

In `photocraft/apps/photocraft/tests/native_menu_tests.rs`:

```rust
#[test]
fn no_unintercepted_predefined_quit_in_menu() {
    let sources = craft_testkit::rust_sources("src").expect("scan sources");
    let findings = craft_testkit::predefined_quit(&sources);
    assert!(
        findings.is_empty(),
        "predefined muda Quit bypasses dirty document save prompts:\n{}",
        findings.iter().map(|f| f.to_string()).collect::<Vec<_>>().join("\n")
    );
}
```

### 2. FilmCraft: Adopt `std_instant` and `wgsl_float_literals`

In `filmcraft/crates/engine/src/wasm_clock_tests.rs`, replace in-tree textual search with `craft_testkit::std_instant`:

```rust
#[test]
fn no_std_instant_in_code_the_web_build_runs() {
    let sources = craft_testkit::rust_sources(Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
        .expect("scan engine sources");
    let findings = craft_testkit::std_instant(&sources);
    assert!(
        findings.is_empty(),
        "std::time::Instant panics on wasm32; use web_time::Instant:\n{}",
        findings.iter().map(|f| f.to_string()).collect::<Vec<_>>().join("\n")
    );
}
```

## deadline

The `deadline` module is not built for `wasm32`.

`with_deadline(limit, label, f)` runs `f` on its own thread and returns its value. When `f` does not
finish within the limit, the test fails with a message that names `label`, the limit and the variable
that sets it. A panic in `f` is raised again in the test with its original payload. The limit counts
from the call, including the time spent starting the thread. When `f` returns or panics after the
limit has passed, the test fails with the label and the limit, and with the panic's message if it has
one.

`wait_until(limit, what, cond)` calls `cond` on the test's thread until it returns `true`. It calls
`cond` at once, then after pauses that start at 1 ms and double up to 50 ms, and does not call it
again once the limit has passed. When the limit passes first, or `cond` returns `true` only after the
limit has passed, the test fails with a message that names `what`.

```rust
use std::time::Duration;
use craft_testkit::deadline::{DeadlineLimit, wait_until, with_deadline};

// The default of 30 s; CRAFT_TEST_TIMEOUT_SECS replaces it.
let pixels = with_deadline(Duration::from_secs(30), "decode seed 7", move || decode(&bytes));

// The app's own variable replaces the default instead.
let limit = DeadlineLimit::new(Duration::from_secs(30)).with_variable("FILMCRAFT_HOSTILE_TIMEOUT_SECS");
with_deadline(limit, "hostile input 12", move || parse(&input));

wait_until(Duration::from_secs(10), "the export to finish", || job.is_done());
```

### The limit variable

A `Duration` reads `CRAFT_TEST_TIMEOUT_SECS`. `DeadlineLimit::with_variable` names another variable.
The variable is read on every call; neither function sets or caches it.

| Value | Result |
|---|---|
| Not set | The default limit applies. |
| Decimal digits above zero, such as `30` or `007` | That many seconds replace the default. |
| Empty, signed (`+5`, `-5`), fractional (`1.5`), with spaces or units (`5s`), zero, or larger than a `u64` | The test fails with the variable's name and value, before `f` runs or `cond` is called. |
| Not Unicode | The test fails with the variable's name, before `f` runs or `cond` is called. |

A zero default, a variable name that is empty or holds `=` or a NUL character, and a limit that ends past
the latest time the platform's clock can represent also fail the test before any work starts.

FilmCraft's `filmcraft-testkit` uses the default when the variable holds something other than a
number or holds zero. This module fails the test instead. `craft-media-testkit`'s `with_deadline`
calls this module's.

### Limits

- A body that runs past its limit fails the test, but its thread is left running until the test
  process exits. A body that never returns can keep the test binary alive after its tests finish.
- `wait_until` checks its limit only between calls of `cond`. A `cond` that blocks is not interrupted,
  so it must return promptly.
- Neither function returns elapsed time. Tests of this crate use channel handshakes, and the only time
  bounds in them are the limits under test.

## hostile

`mutations(seed, rounds)` yields `rounds` damaged copies of a valid seed. Each round makes one to four
edits: a bit flip, a random byte, a 1, 2, 4 or 8-byte overwrite with an extreme value
(0, 1, 0x7F, 0x80, 0xFF, 0xFFFF, 0x8000_0000 or `u64::MAX`) in either byte order, a deletion of up to
16 bytes, or a truncation. Each copy carries a label that lists its edits, such as
`flip[3].5 del[7..9]`. The xorshift64* generator behind it is seeded from the seed's length, so the
same seed length and round count give the same edits on every run and every platform. An empty seed
yields no copies. `mutations` and `Rng` build for `wasm32`.

`hammer(&config, seed, parse)` runs `parse` on the unmodified seed, on every truncation of it
(lengths 0 to one less than its length), and on `mutations(seed, rounds)`. The cases run one after
another on one worker thread, and the test's thread fails the test when a case runs past its limit.
A case that finishes after its limit also fails the test. The test fails on the first case that
panics or does not finish, with a message that names the config's label, the case
(`the unmodified seed`, `truncation to N` or the edits), the input's length and the panic's message.
`parse` must be `Fn(&[u8]) + Send + Sync + 'static`, since a case that does not finish keeps running
on the worker thread after the test fails. `hammer` is not built for `wasm32`.

```rust
use std::time::Duration;
use craft_testkit::DeadlineLimit;
use craft_testkit::hostile::{HammerConfig, hammer};

// 2,000 rounds times CRAFT_HOSTILE_MULT; 5 s per case, or CRAFT_TEST_TIMEOUT_SECS.
hammer(&HammerConfig::new("png seed 3", 2_000), &seed, |data| {
    let _ = decode(data);
});

// The app's own variables instead.
let config = HammerConfig::new("mxf primer", 5_000)
    .with_multiplier_variable("CRAFT_MEDIA_HOSTILE_MULT")
    .with_timeout(DeadlineLimit::new(Duration::from_secs(5)).with_variable("FILMCRAFT_HOSTILE_TIMEOUT_SECS"));
hammer(&config, &primer, |data| {
    let _ = parse_primer(data);
});
```

### The rounds

The rounds are the config's iterations times the multiplier variable, `CRAFT_HOSTILE_MULT` unless
`with_multiplier_variable` names another. The limit for each case is 5 s, replaced by
`CRAFT_TEST_TIMEOUT_SECS` or the variable that `with_timeout` names, as in `deadline`. Both variables
are read once, before any case runs. `HammerConfig::rounds` returns the rounds or the error that `hammer` would fail with.

| Multiplier value | Result |
|---|---|
| Not set | The multiplier is 1. |
| Decimal digits above zero, such as `10` | The rounds are multiplied by that number. |
| Empty, signed, fractional, with spaces, zero, or larger than a `usize` | The test fails with the variable's name and value before `parse` is called. |
| Not Unicode | The test fails with the variable's name before `parse` is called. |

A product of the iterations and the multiplier that does not fit in a `usize`, a call that would run
more than `MAX_CASES` (10,000,000) cases in all, and a limit variable that `deadline` would reject
also fail the test before `parse` is called.

### Quick runs

`CRAFT_HOSTILE_DIVISOR=N` runs a fraction of every sweep: the unmodified seed, every N-th truncation
(lengths 0, N, 2N, ...) and the rounds divided by N, rounded up. It takes the same values as the
multiplier. The Workspace gate sets it on every run and the nightly run leaves it unset, so every
case still runs once a day. A sweep written without `hammer` reads it with `hostile::divisor()` and
runs the cases whose index plus an offset is a multiple of it, with a different offset for each input
so that the inputs together still reach every index. A failing case is found again with the same
value.

Changes from `craft-media-testkit`'s `hammer`:

- A case that does not finish fails the test after its limit with the case's label. Before, a case
  was timed after it returned and failed only when it took longer than 5 s.
- A panic's message is part of the failure.
- Every truncation runs. Before, a seed of 8,192 bytes or more ran every `len / 4096`th truncation.
- The caller names the multiplier variable. Before, it was always `CRAFT_MEDIA_HOSTILE_MULT`, and a
  value that was not a number counted as 1.
- An empty seed runs once. Before, it ran once more for each round.
- A mutation's label joins its edits with single spaces. Before, every edit ended with a space, so
  each label ended with one.
- `parse` is owned and runs on a worker thread. A closure that borrows from the test needs its data
  moved or shared with an `Arc`.

## pair

`pair` decides paired performance verdicts: a native arm against a plugin arm of the same workload.
The rules, the gate table and the record line format are in
`docs/plugin-platform/contracts/conformance.md`. It has no dependencies and builds for `wasm32`.

- `Hist` is a latency histogram from 1 µs to 60 s with 64 log-linear buckets per power of two. A
  quantile is at most 0.78% from the sample's own. `underflow()` and `overflow()` count the samples
  outside the range. A `Count` is events in frames, for the rate gates.
- A `RunRecord` holds one arm's `Samples` (a `Hist` or a `Count`) for one block, with its identity,
  `Workload`, `night`, `runner_image`, `block`, `arm` and `gate`. Identity values are `Ident`s.
- `Gate::kind()` gives a gate's `Kind`: a latency quantile with its floor, fraction and ceiling, or a
  rate with its floor in parts per billion. `Kind::allow(q_A, refresh_hz)` is the allowance.
- `decide(records, calibrations)` returns `Decision::Measured(Measurement)` or
  `Decision::Withheld(Doubt)`, and `Decision::verdict()` the `Verdict`. The bootstrap seed comes
  from the records. `calibration_verdict(records)` decides an in-job A/A or control pool without the
  eligibility check, and `Calibration::eligibility()` judges a (gate, runner image).
- `print_records` and `parse_records` write and read a pool, one line per record. `RunRecord`,
  `Samples` and `Hist` also implement `Display` and `FromStr`.

```rust
use craft_testkit::pair::{decide, parse_records, Verdict};

let records = parse_records(&pool_text)?;
let verdict = decide(&records, &calibrations)?.verdict();
assert_ne!(verdict, Verdict::Fail);
```

The `CONF-aa-sequential` simulation runs up to 4,000 verdicts and is ignored by default; the CI job
`testkit-simulation` runs it with `cargo test --release -p craft-testkit -- --ignored conf_aa_sequential`.

## corpus

`corpus`, behind the `corpus` feature and not built for `wasm32`, keeps a legacy corpus: files an
app generates from code and commits, each listed with its sha256 so that later code can be proven
against the exact bytes. The plugin platform's slice 0f uses it for each app's
`tests/legacy-corpus/`.

- `Corpus::new(app)` and `Corpus::add(name, kind, expect, files)` build the corpus in memory. A
  member has a name and a kind (1 to 64 of `a-z`, `0-9` and `-`), a one-line expected behaviour
  of at most 240 characters, and 1 to 64 files. A file path is up to 8 `/`-separated parts of
  letters, digits, spaces, `.`, `_` and `-`, none starting with `.` and none starting or ending
  with a space; a file is at most 16 MiB. A corpus has at most 256 members.
- `Corpus::write(root)` writes `corpus.toml`, a `.gitattributes` of `* -text` (so checkouts keep
  the bytes) and every file under `root/<member>/`, and never deletes a file. A root without
  `corpus.toml` must be absent or empty, so a mistyped root is refused with `NotACorpus` before
  anything is written. Over an existing listing, committed members are append-only: a member whose
  listing row would change in any field (kind, app, expected behaviour, files or digests) is
  refused with `AppendOnly`, new members are added, and a member
  the generator no longer makes stays frozen in the listing with its committed digests, so a
  writer that changes keeps its old output under the old name and adds the new one under a new
  name. Files that already hold their bytes are not rewritten.
- `Corpus::check(root)` compares the folder with the corpus and returns each `Drift`: a missing
  file (`corpus.toml` and `.gitattributes` included), a file whose bytes differ (with both
  digests), a stray file, a `corpus.toml` that is not the generated listing with the frozen members,
  and a committed member the generator would `Rewrite`. A frozen member's files are checked against
  their committed digests. An empty or absent folder reports every file missing.
- `open(root)` reads `corpus.toml`, refuses unknown fields, bad names and paths, duplicates and
  digests that are not lower-case hex, and reads every listed file, failing on one that is missing
  or differs from its digest. `Committed::bytes`, `path` and `dir` then give a member's files to a
  test, and `bytes` checks the digest again. `Committed::copy_member(member, to)` writes a member's
  verified files into a scratch folder, for code that writes next to the files it reads. `Committed::unread()`
  names the members no `bytes`, `path`, `dir` or `copy_member` call has read, so a parity suite
  can prove it read every committed member, frozen ones included.
- `Manifest::coverage(kinds)` returns a `Gap` for each kind without a member and each member whose
  kind is not in `kinds`. An empty `kinds` is an error, so a coverage test cannot pass by listing
  nothing.

- `run_cli(args, default_root, generate)` is an app's `legacy-corpus` binary: no argument writes,
  `--check` lists the members and fails on any drift, saying whether the fix is to regenerate (new
  members) or to add a new member (an append-only breach), and `--root <dir>` names another root.
- `legacy_paths!` declares an app's enum of legacy path kinds with `ALL` and `name()`, so the
  coverage test and an exhaustive parity `match` both come from one list.

`corpus.toml` lists the members sorted by name, and each member's files sorted by path:

```toml
[[member]]
name = "fill-module"
kind = "pc-abi1-module"
app = "photocraft"
expect = "plugin.run sets every colour sample to the level parameter and keeps alpha"
files = [
    { path = "fill.wasm", sha256 = "..." },
    { path = "fill.wat", sha256 = "..." },
]
```

## Origin

### guards

| Module | Source | Path | Commit |
|---|---|---|---|
| `guards` | storytold/craft-libs#163 (zachshallbetter), with the rules of PhotoCraft's shader float test and FilmCraft's web clock test | `crates/testkit/src/guards.rs` | `e92825e427` |

### deadline

| Code | Repository | Path | SHA |
|---|---|---|---|
| `with_deadline`, its panic and timeout tests (`a_slow_body_passes`, `a_panic_in_the_body_is_re_raised`, `a_hang_fails_and_names_the_input`) | storytold/filmcraft | `crates/testkit/src/deadline.rs` | `cff7e144` |
| `with_deadline` under `CRAFT_TEST_TIMEOUT_SECS`, tests `with_deadline_completes` and `with_deadline_times_out` | storytold/craft-libs | `crates/media-testkit/src/lib.rs` | `75a654f4` |
| `within`, a deadline that returns the body's value | storytold/lightcraft | `crates/engine/src/demo/tests.rs` | `487833ad` |
| `within`, a deadline that returns the body's value | storytold/gridcraft | `crates/math/src/lib.rs` | `c2aba7a3` |
| `wait_until` | storytold/photocraft | `crates/ui-egui/src/canvas/paint_thread_tests.rs` | `0d3b1e85` |
| `wait_until` | storytold/lightcraft | `crates/engine/src/availability.rs` | `7ebbb17a` |
| `wait_until` | storytold/effectcraft | `crates/engine/src/footage/footage_reload.rs` | `53576d22` |
| `wait_until` | storytold/filmcraft | `crates/ui-egui/tests/it/export_ui.rs` | `cff7e144` |

### hostile

| Code | Repository | Path | SHA |
|---|---|---|---|
| `Rng`, the edits of `mutations` and `hammer` | storytold/craft-libs | `crates/media-testkit/src/hostile.rs` | `75a654f4` |

## Migration

### guards

| Consumer | Guard | Replaces | State |
|---|---|---|---|
| PhotoCraft | `wgsl_float_literals` | `crates/gpu/src/tests.rs` scanner | Not started |
| FilmCraft | `std_instant` | `crates/engine/src/wasm_clock_tests.rs` search | Not started |

### deadline

| Consumer | Code it replaces | State |
|---|---|---|
| FilmCraft | `filmcraft-testkit`'s `with_deadline` (`FILMCRAFT_HOSTILE_TIMEOUT_SECS`) and the export UI tests' `wait_until` | Pending |
| LightCraft | `within` in the demo tests and `wait_until` in the availability tests | Pending |
| GridCraft | `within` in the tests of its `crates/math` | Pending |
| PhotoCraft | `wait_until` in the paint thread tests | Pending |
| EffectCraft | `wait_until` in the footage reload tests | Pending |
| `craft-media-testkit` | `with_deadline` (`CRAFT_TEST_TIMEOUT_SECS`) | Done |

An adoption keeps the variable name the app uses today.

### hostile

| Consumer | Code it replaces | State |
|---|---|---|
| `craft-media-testkit` | `hostile::hammer` (`CRAFT_MEDIA_HOSTILE_MULT`), called by the ISO BMFF, AVI, Matroska, MXF and MPEG TS hostile tests | Done |
| `craft-cfb` | its `hostile_input_never_panics` test, through `craft-media-testkit` | Done |
| CADCraft | the mutation loop of `hostile_files_never_panic` in `crates/fonts/src/shx/tests.rs` | Pending |
| TextureCraft | the `hammer` of `malformed_meshes_are_errors` in `crates/formats/src/mesh_import/tests.rs` | Pending |

CADCraft's and TextureCraft's loops make other edits than `mutations`. Their adoptions say which
cases change, and keep the test names.
