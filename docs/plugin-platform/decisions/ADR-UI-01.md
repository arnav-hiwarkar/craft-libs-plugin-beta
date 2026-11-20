# UI-01 · Plugin panels in the application workspace

**Decision:** Accepted with verification. **Status:** Designed for this topic. Implementation and runtime tests are pending. EffectCraft and PhotoCraft source and actual dependency pins are recorded in the baseline ledger. [Source pins](../source-baselines.md) · [Owner invariants](../architecture/owner-invariants.md)

## Decision and tradeoff

A plugin registers named UI contributions. The shared host owns the mapping from contribution and instance to mounted content and admitted actions. The app supplies that content inside Craft's ordinary tab, floating column, inspector and dialog paths. Native or embedded is a saved preference of a floating group. The host reports the effective result from the current window backend. Panels can contain custom 2D and GPU views. Author-defined workspace presets apply through the same layout validator. An approved replacement slot retains a built-in provider and one-click restore.

A worker-rendered image and hit map can support arbitrary visuals. General use would duplicate IME, local controls, theming, accessibility and rendering state while adding texture transfer and resize synchronization. Host-rendered semantic content with bounded custom drawing and leased images supplies those controls directly. It introduces an obligation to extend the shared framework when professional controls are missing. UI-02 defines the custom content and compatible fallback. A1 alternatives (UI-01 A1 record)

## Host and SDK contract

The [workspace contract](../contracts/ui.md) owns the signatures, identity, layout, app action ordering and failure semantics. In SDK terms a package declares `layer-tools`, the host issues a mount, `ui.publish` sends validated content for it, and `onUiAction` receives an attributed event. One authorized edit ticket can commit once for a captured document instance, revision and actual targets through DOC-01. Retrying after receipt expiry yields Expired. Revocation and owner-thread publication are serialized.

The framework consumes input at its originating viewport. It carries input claim and drop provenance across root, immediate or deferred callbacks. Before signaling the guest or admitting a reply, the app's UI owner applies host intents and resolves protected dialogs, target changes and presentation changes. A consumed key stays consumed after submit-and-blur. Closing, rebinding, modal blocking or fault invalidates the affected interaction. The current draft and preview retire independently of saved layout. Same-pass layout reruns use only their own input.

The renderer sees bounded, immutable content. Runtime admission charges framing bytes, validated preparation, visible work, queued actions and aggregate live and retired lease memory. Each gesture reserves terminal-event capacity. Protected prompt, picker and recovery authority remain with the host. Local web and native execution modes require separate offline evidence on each advertised target. Headless exposes no window presentations. A guest built to this API knows neither `egui` nor an app's panel enum.

## Code impact

- PhotoCraft. Keep `app/framework.rs` shell setup and history/undo callers. Route dynamic descriptors through `panels/registry.rs` and `panels/panel_docking.rs` at its existing `DockPanels<String>`. Correct startup reconciliation, origin-aware drop routing, and the order of input and protected modal admission at `app/frame.rs`. Insert inspector slots through existing `panels/properties.rs`. PhotoCraft's `edit_with` relies on active document, Action capture, relative path and settings. The transaction slice changes these together so no guest action retargets active B when it began on A. Evidence P1–P4 (UI-01 Q record)
- EffectCraft. Port its existing `dock.rs`, `dock_ui.rs`, `workspace_state.rs` and Window commands using the split framework as the dependency target. Preserve current editor body and native/IME ownership while translating layout. Replace focus-after-draw forwarding with claims. Keep stable built-in keys and add persistent extension identity. Treat unmapped legacy ScriptUI IDs as inert. Preserve viewport preview generation in locked and ordinary viewers. Choose to port the common dock/interface before enabling public extension panels in EffectCraft. That avoids implementing and shipping the same group and modal fixes on both framework generations. Existing EffectCraft layout and normal app behavior are a prerequisite of the port's cutover. Evidence E1–E3, F1 (UI-01 Q record)

## Review resolution

| Investigation | Lead disposition | Proof |
|---|---|---|
| Q1, Q7 | Stable instance identity, saved intent including named and closed layouts, fresh live mount. Hydrate missing entries before reconcile and preserve opaque records through downgrade. | `UI01-workspace`, `UI01-save`, `UI01-recovery` |
| Q2 | Independent per-group presentation preference. Advertise native and fullscreen only after backend proof. Use embedded or maximize when supported. | `UI01-platform` |
| Q3 | Single claim plus modality at the UI owner boundary. UI-03 specifies input payload and tool capture. | `UI01-pass`, `UI01-modal`, `UI01-drop` |
| Q4 | Validated versioned body, bounded work and semantic fallback. UI-02 owns content and component changes. | `UI01-a11y`, `UI01-frame-cost` |
| Q5 | Explicit opt-in eligible slots, attributed chrome and offline assets. UI-04 owns hooks and ordering. IO-01 and RUN-02 own resource authority. | `UI01-trust`, `UI01-drop` |
| Q6 | Explicit target, generation, action semantics, single publication and per-app history. DOC-01 must implement engines' real paths. | `UI01-edit`, `UI01-preview` |

All five B findings are accepted. C returned PASS_WITH_REQUIRED_AMENDMENTS. The contract and acceptance cases include all five C rows: enforced mode boundaries, claims before both built-in and guest dispatch, aggregate accounting and terminal capacity, atomic publication with non-reusable tickets and gesture semantics, and safe layout/state migration. EffectCraft's implementation order uses the converged framework before enabling plugin panels. It retains the C gates through that port.

The formative budget is under 8 ms framework time on the port-kit Overview fixture, up to 1 ms p95 additional work versus a matched built-in panel on the same hardware, and local feedback within a 60 Hz refresh under worker stall. These are hypotheses with named paired workloads. [Acceptance cases](../implementation/ui-01-acceptance.md) · [Implementation slices](../implementation/ui-01-slices.md)

**Independent record:** The Q, A1, B, A2 and C review roles ran in order. Every role used GPT-6.2 Sol, high reasoning.
