# Legacy migration inventory

This is the input to the migration design. Retire a legacy execution route only after its corresponding workflow and persistence fixture pass through the shared platform. Standalone app functionality with no plugin execution needs a caller audit before deletion.

| Current or historical mechanism | Required data and behavior evidence before removal |
|---|---|
| EffectCraft ScriptUI and session scripts | Preserve the user's source and settings. Inventory ScriptUI panels, palettes, dialog and draw callbacks, path-derived registrations, expression tooling, and automation clients separately. Prove a public-SDK motion dashboard, easing editor, undo, and reopen/restore. A runtime window number cannot prove the old author's stable identity. Only map a historical placement when its source association is unambiguous. |
| EffectCraft Wasm v1/v2 effects | Inventory manifest identity, parameter IDs, serialized effect instances, previous/next-frame policies, load/enable and preview/export behavior. Test preserved bytes and literal pixel/time results against known old fixtures. |
| EffectCraft linked effect modules and external helpers | Inventory linked providers and the external effect runner separately. Preserve effect instance references and user data. Original public-SDK implementations must cover each retained workflow. Direct execution of unconfined third-party helpers conflicts with offline enforcement. |
| PhotoCraft Wasm effects | Preserve destructively written pixels, plugin history, and persistent Smart Filter entries. Test the selection mask, target channel, alpha lock and selected layer. Preserve legacy ID and parameters when the plugin is absent. Test installed, missing, disabled and differing-digest results through the placed pipeline and encoder. |
| Beta ABI 1 and worker runtime | Preserve committed project references and state. Use pinned examples and ABI fixtures. Verify no later beta runtime path or worktree change gets mistaken for shipped behavior. Document exact operation/schema translation or a safe migration report for every supported reference. |
| Beta framework renderer and app-independent float wrapper | Reuse valid fixtures for revisions, fallbacks, canvas events and focus; migrate stored named panel placement where the ID and geometry are recoverable. Preserve only one dock in each target app. |
| Beta native and proprietary format hosts, store, and web modes | Inventory projects that refer to these contributions. Missing dependencies retain their data and report why. Local loader and offline approved providers follow present scope; catalogue delivery and Adobe host execution are excluded. |

Source anchors are in the [inventory](../source-inventory.md) and UI-01 source investigation (UI-01 Q record). Historical scope and test coverage are recorded in the beta status.

## Dispositions (RUN-05/SDK-02)

Decided in [ADR-SDK-02](../decisions/ADR-SDK-02.md). Each path retires in its own PR during phase 7. A retirement needs all of the following:
- the corpus survives;
- zero callers remain;
- ids resolve or are tombstoned, with tombstones naming the local-file install replacement;
- settings are migrated once under a lock;
- `RET-headless`, `RET-mig-race` and `RET-corpus-coverage` pass;
- an eligible timing Pass;
- no open `DEFECT` on the id.

| Path | Disposition | Gate |
|---|---|---|
| PC wasmi `crates/plugins`, `plugin.{list,install,remove,reload}`, Filter › Plug-ins rows | `pc_abi` shim. The `plugin.run` alias is kept, bound to the adoption record. Other commands are tombstoned. | `RET-pc`, `FAULT-alias-spoof-legacy` |
| PC `plug_ins.*folder` prefs, EC `plugins_dir` | One-time consent sheet. The adoption record is honoured headless. | `RET-pc`, `RET-ec`, `RET-headless` |
| EC `plugin/src/wasm.rs`, `effects/src/plugin.rs`, `effect.plugins.*` | `ec_api` shim, with v2 as a timed plan. Commands are tombstoned. | `RET-ec`, `RET-ec-cmds`, `PAR-fuel-map` |
| EC Plug-in Library (`system/library.rs`: 8 commands, `curl`, catalogue, confine list) | Network parts deleted. SHA-verified installs migrate once after consent; a mismatched install gets "origin unverified" adoption. `missing` reports locally. | `RET-ec-library`, `RET-ec-library-mismatch`, `RUN-policy-diff` |
| EC scripts and boa (folders, Startup/Shutdown, `scriptui_panels.json`, `script_settings.json`) | Asyncify `.jsx` shim. Startup and Shutdown scripts run after consent. Settings copied once. `Socket` returns `Unavailable(NoNetwork)`. | `SCRIPT-ec-suite`, `RET-ec-scripts` |
| Native Stylize, Echo, Liquify, Track Motion, Primitive; id-keyed lists | Become packages with declared fields. First-shipped ids and schemas are frozen. | `PAR-*`, `RET-*` (Stylize also needs `WEB-broker`) |
| EC `helper-host`; automation policies | Kept, as NativeConfined and Automation principals | Native `NET-matrix`, `RUN-policy-diff` |
| PC `authorize`/`automation_gate`/`param_check.rs`, EC `cmd/schema.rs`, SlowFrames and the FPS EMA | Deleted. Replaced by the gate, the single validator and probe views. | `RUN-lint`, `SCHEMA-equiv`, `CONF-probe-continuity` |
| Beta `craft_abi` 1, `ext-ui-fw`, `NativeFull`, `Network(host)`, store, Adobe hosts | ABI 1 shim. Everything else deleted. References stay inert. | `RET-beta`, `DOC-missing` |
