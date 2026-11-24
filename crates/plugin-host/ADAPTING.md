# Giving an ArtCraft app plugins with this kit

How to connect another app to the Craft Plugin Kit, and what the later step of replacing an app's own plugin
system will need. EffectCraft is the one app adapted so far. Its adapter is the model; copy its shape, not
its code. How the kit itself was built and changed is in `docs/plugin-build/playbook.md` in the EffectCraft
fork.

## What the kit gives an app

| Crate | Path | Gives |
|---|---|---|
| `craft-plugin` | `crates/plugin-sdk` | The SDK authors write against; the manifest, wire types and native ABI the host reads; `math`, curve and gradient samplers. Depends on serde only. |
| `craft-plugin-host` | `crates/plugin-host` | Loading (WebAssembly in Wasmtime, approved native libraries), packages, the installed set and approval (`Registry`, `Consent`), the permission gate, banding, region rendering, host kernels and fusion, tasks, the plugin data cache, the journal, the retained panel state machine (`Retained`), and the test kit (`testing` feature: `TestPlugin`, stored frames, band and time checks, `probe_dir`). |
| `craft-plugin-ui` | `crates/plugin-ui` | egui drawing of panel widget trees, custom control rows and viewer marks, with every app-specific piece passed in through its `Host` and `Theme`. |

None of them may name an app type. An app type that the kit needs is a trait the app implements.

## What the app must provide

| Piece | Where | Needed for |
|---|---|---|
| `craft_plugin_host::Host` | `src/host.rs` | Everything. One implementation per context (a render, a session command, a panel worker). Every method has a default, so an app implements only what it offers: `access` and `command` (the permission decision and running an app command), `notify`, `layer`, `source`, `masks`, `scene`, `scene_at`, `placement`, `frame`, `audio`, `pick_file`, `form` (show the `Form` modal with `craft_plugin_ui::draw_form` on the UI thread while the plugin's thread waits, and answer it from automation too; the kit fits the answer to the fields), `params_at`, `project_closed`, `cancel` (a render's call-off flag: once it is set the kit stops the render at its next band or pass with `PluginError::Cancelled`, a sandboxed band within one 50 ms clock tick, and logs no error; the frame the app passed in is then unspecified, so the app must discard it, and keep nothing cached from it or show it; the kit itself caches nothing from a cancelled render), `finish` (post-process each band, such as quantising), `clipboard`, `set_clipboard`, `effect`, `data` and `set_data` (plugin data in the project and on layers: store `craft_plugin_host::data::Data` per plugin id, write through `data::write` so the limits hold, make each write an undo step, and put a layer's data in its cache key). |
| `ParamValues` | `src/app.rs` | Effects. Converts between the app's property values and wire values (`to_wire`, `from_wire`). |
| `EffectRegistry` | `src/app.rs` | Effects. Registers and unregisters a loaded plugin's effects in the app's own effect registry. `supports` names the parameter kinds the app offers; `admit` then refuses a plugin at load with a message naming the effect, the parameter and its kind. |
| `craft_plugin_ui::Host` | plugin-ui `src/lib.rs` | Any UI. Translation, automation registration, textures, polygon triangulation, drag payloads in and out, file drops, the combo box. |
| `craft_plugin_ui::Theme` | plugin-ui `src/lib.rs` | Any UI. The paint colours and heading font, built from the app's theme tokens. |
| A command gate policy | app side | Any plugin that runs app commands. Which command families a plugin may run at all, which need write, which name files. |

Optional by kind of plugin: panels and commands need only `Host` (with `access`, `command`, `project_closed`)
and the UI crate. Effects that declare `EffectSpec::version` need the app to store that version on each saved
instance and, when a project opens with an older one, send `Call::Migrate` with the instance's values and write
back what comes back; a saved version newer than the plugin's is kept as it is and reported. Tasks add `frame`, `audio` and `pick_file` if offered. Effects add `ParamValues`,
`EffectRegistry` and the render methods of `Host`. Viewer overlays and tools need the UI crate's marks and
the app's own viewer mapping.

## Steps for a new adapter, smallest first

Each step ends with tests that fail if it is wrong. Use the probe plugin (`tests/probe`, found with
`testing::probe_dir()`) for every host call, and keep one test that loads a plugin built against an older
SDK (EffectCraft keeps `crates/plugin/tests/it/legacy/legacy.wasm`).

1. **Install, approve, load.** Point `Registry` at the app's config folder, show the approval with the
   requested permissions, refuse unapproved native code, refuse plugin management from scripts. Test: install
   a bundle, it waits; approve, it runs and still runs after a restart; a script cannot approve it.
   `Registry::grants(bundle)` hashes the bundle and answers a `Refusal` whose text tells the user why
   (changed files, wider permissions, quarantined); show it. Call `Registry::recover` once at start, then
   `mark_running` with the set of unsandboxed plugins before each one starts and after one stops, and
   `end_run` on a clean quit, so a crash turns those plugins off next time. Offer a safe-mode flag that
   starts none. Start an approved plugin with `Plugin::open(bundle, digest, ..)`, passing the bundle and
   digest of `Registry::approved`, whose manifest is parsed from the bytes it hashed; for a bundle with no
   approval (a session copy) use `Bundle::read`. A sandboxed plugin that was described before under the
   same digest registers from its kept description (`.data/.compiled/<name>.described`) and loads its
   module on first use; `Plugin::started` tells whether it has and `Plugin::failure` why that load failed.
   Show a failure as failed. The manifest's entry must name its file in the exact case.
2. **Commands and the gate.** Implement `Host::access` and `Host::command` over the app's command system.
   List each plugin command (`CommandSpec`) in the app's menus, command search and keymap; offer its `shortcut` as a default only when no built-in command holds those keys, and log the refusal otherwise.
   Allow an explicit list of document command families (an allow list, never a deny list), map undoable
   commands to `document.write`, require both file permissions for any command that names a file, and run
   one plugin batch as one undo step. Keep a census of every allowed command with its decision and a test
   that fails when a command is added. Test each rule with a probe call that is refused without it.
3. **Panels.** Dock a panel per contribution. Run every interactive call on one worker thread per plugin;
   the UI thread posts requests, answers host calls that need the document, and applies replies, with a
   time budget per UI frame. Give each request the plugin's load, the project's token and the surface's
   token, and drop it when any is stale. Feed `Retained` with the app's events (`Topic`: selection, time,
   comp, project, edit, mapped to the app's nearest equivalents). Give each custom parameter row its own
   `Retained` on the same worker and pass a `ControlsReply`'s `topics` and `timers` into it, so rows
   subscribe and tick as panels do. Test: zero plugin calls over idle frames;
   each event kind calls once; a reply after the project closed is dropped; kittests with real pointer and
   key events.
4. **Tasks.** Run on their own thread and instance, bound to the project they started in and cancelled when
   it closes. Answer their document requests on the main thread through a pump. Test: progress, cancel,
   and a task that outlives its project.
5. **Effects.** Implement `ParamValues` for each `ParamKind` the app supports and `EffectRegistry` over the
   app's registry; refuse other kinds at load. Pass the layout through (bands, padding, passes) and map the
   dependence flags (`time_dependent`, `reads_scene`, `reads_other_frames`, layer inputs) into the app's
   cache key; hash values, not their `Debug` text. Give the region of interest (`shown`) when the
   compositor knows it, snapped to a grid so small moves keep the key. Test: stored frames against the
   probe, a render through the app's real cache before and after a change of each declared input, and a
   project saved with the plugin missing that keeps its values and renders again when it returns.
6. **Viewer overlays and tools.** Map viewer coordinates to the plugin's spaces, turn a handle drag into
   one undoable parameter change, give tools pointer and key events. Overlays are capped by the SDK. Test
   with kittests that drag a handle and undo it.
7. **Conditional rows and buttons.** Before drawing a parameter row, call `Param::visible` and
   `Param::enabled` with a lookup of the instance's current values (no plugin call); keep hidden values.
   Store a `ParamKind::Button` as a placeholder with no value the effect receives, draw it with
   `craft_plugin_ui::param_button`, and send one `Call::Button` per press on the plugin's interactive queue,
   folding what it does into one undo step. Test: a kittest that changes a popup, sees a row appear, and
   presses the button by pointer and keyboard.

## Where EffectCraft's adapter does each thing

Paths in the EffectCraft fork (`pluginkit/effectcraft`, branch `pluginkit`).

| Concern | File |
|---|---|
| Install, approve, reload, menu commands, the task pump | `crates/plugin/src/manage.rs` (`install`, `pump`) |
| Command gate policy and its census | `crates/plugin/src/gate.rs` (`DOCUMENT_FAMILIES`, `SessionHost`), `crates/plugin/src/gate/census.tsv` |
| Script refusal of plugin management | `crates/engine/src/cmd/plugin.rs` |
| Effects: `EffectRegistry`, `ParamValues`, the render `Host`, margins | `crates/plugin/src/effects.rs` (`impl EffectRegistry for Engine`, `impl ParamValues for Placed`, `RenderHost`, `margin`) |
| One queue per plugin, request identity, the panel worker's `Host` | `crates/plugin/src/panels.rs` (`Panels`, `Remote`) |
| State versions: stamp, migrate on open, keep newer | `crates/plugin/src/effects.rs` (`migrate`), `crates/engine/src/session/edit.rs` (`upgrade_effects`) |
| Overlays, handles, caps | `crates/plugin/src/viewer.rs` |
| UI `Host` and `Theme` | `crates/ui-egui/src/panels/plugin_view.rs` (`AppHost`, `theme`) |
| Effect Controls rows, overlay drawing, handles | `crates/ui-egui/src/panels/effect_controls/{rows,plugin_overlay,viewer}.rs` |
| Tools, manager window | `crates/ui-egui/src/panels/plugin_tools.rs`, `plugin_manager.rs` |
| Region of interest and cache keys | `crates/render/src/pipeline.rs` (`shown_part`), `crates/render/src/cache.rs` (`key_any`, `hash_scene`, `time_dependent`) |
| Adapter tests | `crates/plugin/tests/it/` (gate, render, panels, viewer, params, legacy) |

The adapter crate is about 2,500 lines, of which panels and the gate are two thirds.

## What is still EffectCraft-shaped in the kit

These are app-independent in code but carry a compositing app's model. A raster or vector app needs care
with each.

- **Frames and layers.** A frame is premultiplied `f32` RGBA (`Pixel = [f32; 4]`). `layer`, `source`,
  `masks` and layer parameters assume layers in a stack. A raster app works in a document colour model and
  bit depth (PhotoCraft hands plugins samples in the document's own model); a vector app works on objects,
  not pixels.
- **Scene and camera.** `Scene`, `Camera`, lights and `placement` assume a 3D comp. A 2D app returns `None`.
- **Time.** `cx.time`, `layer_at`, `source_at`, `params_at`, `time_dependent` and the `Time` topic assume a
  timeline. A still-image app has one time; these then return the current state or `None`.
- **Viewer spaces.** `Space` is `Layer` or `Comp`. A vector app needs artboard or document space; a raster
  app needs canvas space.
- **Event topics.** `Comp` and `Edit { layers }` name comp concepts. Map them to the nearest app concept
  before adding topics; add a topic only by addition.
- **Parameter kinds.** `Point`, `Angle`, `Layer`, `Mask` assume layer space and layers. An app that lacks one
  refuses it through `EffectRegistry::supports`.

## Open questions for replacing an app's current system

VectorCraft and PhotoCraft ship plugin systems of their own upstream. The owner holds any second adapter
until coexist or replace is decided (2026-11-24). Read-only notes from their code:

- **VectorCraft** (`crates/plugins`, `docs/plugins.md`, ABI v1, API level 2): modules run in wasmi (an
  interpreter) with a fuel budget per call plus a wall clock, a "heavy" budget the user allows, a fresh
  instance per run, no host imports except progress, log, settings, cache and allow-listed commands, each
  behind a permission. Kinds: **object filters** (selected objects in as JSON, objects to keep, change or
  add out, one edit), **live effects** (rewrite one object's geometry each draw from an appearance stack;
  "painted" ones also paint), **exporters** and **importers** of whole-document formats, menu commands, and
  generated **dialogs** from parameter specs (labels, units, groups, help, conditions). Safe mode loads
  nothing.
- **PhotoCraft** (`crates/plugins` on upstream `main`): wasmi, no host imports at all, fuel and wall clock,
  a fresh instance per band, pixels as normalised `f32` samples in the document's colour model, filters run
  on the active layer, channel, quick mask or smart object with the selection, recorded as smart filters
  that call `plugin.run` with the plugin id. Its `plugin-beta` branch holds an earlier shared-host attempt
  (`docs/plugin-migration.md`) worth reading for what that approach needed.

What a migration would need to answer:

1. **The web build.** Both apps run plugins in the browser through wasmi. This kit's runtime is Wasmtime,
   which does not run on `wasm32`; `unavailable.rs` stubs the host there. Replacing those systems needs a
   web runtime behind the same `Engine` seam (wasmi, or the browser's own WebAssembly), or the apps lose
   plugins on the web.
2. **Object-level plugins.** The kit's effect model is pixels. Object filters, live effects on geometry and
   import/export need new request kinds (by addition): objects in, objects out as one edit; a per-draw
   geometry rewrite with a cache key; a format contribution. Exporters and importers are also the deferred
   "formats" unit for EffectCraft, so one design can serve all three apps.
3. **Old plugins.** Existing modules export their own ABI (`vc_*`, `pc_filter`) and import nothing or a
   small set. Either a compatibility runtime loads them unchanged behind the kit's `Plugin` type, or authors
   rebuild against `craft-plugin`. A shim keeps saved documents working; a rebuild is simpler. Decide per app
   from how many third-party plugins exist.
4. **Saved documents.** Smart filters and appearance stacks store plugin ids and parameter values. Ids and
   values must map to the new manifest, and a document saved with a plugin missing must keep its values.
5. **Budgets.** Fuel gives deterministic limits and works on the web; the kit's epoch limit is cheaper but
   time-based. Pick one rule per runtime and say which in the manifest docs.
6. **Permissions.** VectorCraft's `settings`, `cache`, `progress` and `commands` map to the plugin data
   folder, the data cache, task progress and the command gate. Its command allow list in the manifest is
   stricter than the kit's per-family gate; keep that strictness where the app wants it.
7. **Dialogs.** VectorCraft generates dialogs from parameter specs with units and conditions; the kit's
   parameters have groups, soft ranges and custom controls but no conditions or units. Add them by addition
   if the replacement needs them.

## Repository and dependency rules

- Two repositories. **craft-libs fork** (this repo, branch `pluginkit`; remotes `github`
  arnav-hiwarkar/craft-libs-plugin-beta and `forgejo`) holds the three kit crates and anything else
  app-independent. **EffectCraft fork** (branch `pluginkit`) holds the adapter, the app UI and the reference
  plugins. Another app's adapter lives in that app's repository.
- Put each piece at the lowest shared layer that fits. Move code once, as a pure move, then delete the old
  copy in one commit.
- Apps take the kit by git at one pinned `rev` in the workspace `Cargo.toml`. Reference plugins, templates and
  fixtures pin the same `rev`. A `rev` move is its own change: push craft-libs, set the `rev` everywhere,
  update lockfiles with `cargo metadata`.
- To change the kit and an app together, uncomment the app's `[patch."https://github.com/arnav-hiwarkar/craft-libs-plugin-beta"]`
  block pointing at a local checkout, and comment it again before the `rev` move.
- craft-libs forbids `unsafe` except where its AGENTS.md names it (the SDK ABI and the native runtime), denies
  `type_complexity`, disallows `egui::ComboBox` and `egui::Id::new`, and wraps at 150 columns. Check the kit
  with `-p` (`cargo fmt --check --all` fails on vendored code).
- Kit tests: `cargo test -p craft-plugin -p craft-plugin-host -p craft-plugin-ui`, clippy with
  `--all-targets --all-features -D warnings`, and `cargo check --target wasm32-unknown-unknown -p craft-plugin`.
  The EffectCraft Linux workflow's `kit` job runs them at the pinned `rev`; an app's adapter tests run in that
  app's job.
