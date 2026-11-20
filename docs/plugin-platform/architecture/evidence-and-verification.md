# Verification rules

Design review artifacts attest to review of a written proposal. SOURCE records an inspected code path. HISTORICAL records evidence at a different revision or a vendor's documented behavior. A design is DESIGNED when its contract, failure semantics, app integration and executable-test specification are present and the lead has reconciled the topic's independent reviews. IMPLEMENTED requires both the provider and its real app callers. TESTED requires the named fixture, exact app and framework pins, platform, command or interaction, and observed outcome. Performance numbers in ADRs are hypotheses.

## Acceptance record

Record `test_id, app_revision, framework_revision, target_os, backend, fixture_hash, action_trace, observable_expected, observable_actual, result`. To reproduce UI behavior, use the app's own automation harness in a disposable profile. Verify real state where screenshots would miss a failure, such as one history entry or a preserved opaque payload. Run hardware-specific checks on the named window system. Test absence of a capability along with its implementation. For UI paths use the port kit's state-by-state production comparison, report changed defaults, test input and compare visible differences.

## Paired measurements

Pair the closest built-in and SDK workflows on the same machine, document, scale, display refresh rate, renderer, binary profile and thermal state. Separate cold initialization from steady state. Record sample sizes, p50/p95/p99, wall time and work done (visible cells, input samples delivered, actual pixels and kernel invocations, reads and writes, copied bytes and GPU fences). Report aborts, disabled paths, fallbacks and drops separately. Establish the native baseline before fixing numeric release thresholds. Attribute extra work to host validation, staging, invocation and commit instead of only timing the guest. Retain raw summaries locally and compare several interleaved runs.

## References used for workflow requirements

The first-party proof and original reference plugins exercise the actual ArtCraft contracts. Photoshop's imaging documentation supports using explicit pixel formats, color profiles and bounded regions in the proof. Neat Video advertises spatial and temporal processing. The original denoising proof uses previous and future frames with reproducible edge policies. Overlord's documented editable transfer informs geometry and text interchange obligations. These precedents establish workflow requirements. Independent implementations and paired ArtCraft results establish actual parity. [Photoshop Imaging](https://developer.adobe.com/photoshop/uxp/2022/ps-reference/media/imaging), [Neat Video features](https://www.neatvideo.com/features), [Overlord](https://battleaxe.co/overlord)
