# Compatibility obligations

Runtime and SDK evolution are DESIGNED in [ADR-RUN-03](../decisions/ADR-RUN-03.md) and the [SDK contract](../contracts/sdk.md); nothing is IMPLEMENTED or TESTED. This document records behavior required by topic contracts. Advertised support and tested support are separate entries in the conformance matrix.

| Boundary | Required stable behavior | Candidate test |
|---|---|---|
| Manifest and contributions | Stable IDs survive label and app version changes. Local enablement binds actual bytes. Applicability is checked per contribution. | Load one package with a compatible image processor and an unavailable native or time role. Other entries remain usable. |
| Document formats | A newer host format that cannot round-trip uses read-only view or refusal. Supported headers preserve unknown provider payloads. Formats without live state support require explicit compatible export or bake. | `DOC-roundtrip`, `DOC-transfer` and `RET-*` (`DOC-legacy`), including recovery without the provider and exact opaque byte digests. |
| Mount and UI body | Framework changes preserve body/node semantics, scope and authorization. An unavailable leaf uses an independently valid equivalent body. | `UI01-workspace` and `UI02-mixed`. Run the same guest before and after renaming a framework component. |
| Editor commands and snapshots | Stable public IDs and types survive moving engine commands between modules. App-specific equivalents advertise explicit support. | Replay an old request with the same typed outcome, Undo and errors against remapped engine entrypoints. |
| Processor pixels/time | Precision, channels, color, alpha, units and coverage are explicit. Exact output requires complete processing at the requested precision. | Pin an image and time fixture and its outcome. Change the internal buffer format without changing guest behavior. |
| Desktop, browser and headless | Input, window and device capabilities depend on a verified backend. Compatible headless work runs under the same authority. | Disable optional capabilities. Verify reporting, approved fallbacks and denied paths. |
| Privilege and version | A new authority or incompatible package digest requires a fresh admitted context. A project never enrolls or activates a package. | Reopen against a same-ID different-digest package. No inherited grant or implicit code activation. |

[Workspace](../contracts/ui.md) · [UI content](../contracts/ui-content.md) · [Operations](../contracts/operations.md) · [Persistent records](../contracts/document.md)
