> Historical review of fc11c3db88. The correctness follow-up and current
> validation are recorded in [2026-10-08-pr474-correctness.md](2026-10-08-pr474-correctness.md).

## SECTION 1 — CODE REVIEW

**Request changes.** The policy gate is substantially improved, but enabled animation has several correctness failures and an allocation path capable of exhausting process memory.

Reviewed `main...feat/svg-animation` through `fc11c3db88`. Findings below are ordered by severity.

1. **P1 — The sequence budget applies after potentially allocating 16 GiB.**
   [svg_animation/sampler.rs:64](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation/sampler.rs:64) accumulates every sampled raster; [image_sequence.rs:382](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/image_sequence.rs:382) checks the 64 MiB sequence budget only afterward.

   A small 4096×4096 SVG with a ten-second indefinite animation produces 256 slots × 64 MiB = **16 GiB per decode**, before rejection. Concurrent misses can multiply that across four workers. Because the rejected sequence never becomes resident, subsequent frame requests repeat the work. Allocation aborts escape `catch_unwind`. Enforce aggregate byte admission during sampling, including concurrent work.

2. **P1 — Computed frames are scaled and rotated twice.**
   [svg_animation/sampler.rs:72](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation/sampler.rs:72) calls `svg::decode` with size, rotation, and realization already applied. [image_cache.rs:1267](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/image_cache.rs:1267) then labels those raster dimensions as native dimensions and calls `realize_bitmap` with the same recipe.

   A native-size 100×50 SVG requested with a quarter-turn becomes 50×100 in the sampler, then 100×50 with a half-turn after bitmap realization. A native-size scale of two becomes scale four. Preserve `ResolvedImageGeometry` for computed frames and apply masking without repeating realization.

3. **P1 — Computed cache hits reuse pixels from another realization.**
   [image_sequence.rs:323](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/image_sequence.rs:323) checks only source sequence identity and producer kind. The source-only identity explicitly excludes realization fields at [image_catalog.rs:223](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs/src/image_catalog.rs:223).

   Warm an animated `currentColor` SVG with foreground red, then request identical bytes with foreground blue: the second request gets red pixels. Size, rotation, and device scale also affect the cached frames. Authored raster frames can be source-native; these computed frames are already realization-specific. Their cache keys must reflect that difference.

4. **P1 — File-backed SVGs ignore the animation policy.**
   [image_cache.rs:1054](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/image_cache.rs:1054) receives `animation`, but `decode_file` never uses it.

   The same document animates through `:data ... :animation t`, but remains static through `:file ... :animation t`; nonzero indices fail before reaching the SVG fallback. Route both sources through the same encoded-data pipeline, retaining the file’s resource context.

5. **P2 — Concurrent misses can expose a losing grid.**
   [image_sequence.rs:345](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/image_sequence.rs:345) selects the returned frame before publication. [image_sequence.rs:369](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/image_sequence.rs:369) discards later same-kind publications without replacing their already-selected return values.

   Concurrent 30-fps and 4-fps requests for a two-second document can return 60-frame metadata to one caller while the resident winner has eight frames. That caller’s subsequent `:index 30` fails. The documented “first requester owns the grid” rule does not justify exposing both grids. Publication/coalescing must return the winning sequence consistently.

6. **P2 — Attribute deduplication deletes animations on unrelated elements.**
   [svg_animation/plan.rs:254](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation/plan.rs:254) compares attribute name and `value_range`, omitting target identity.

   Two rectangles without `opacity`, each carrying an opacity animation, both have `value_range=None`. Compiling the second deletes the first, so only one rectangle animates. Include the target or insertion position in the site identity.

7. **P2 — Compile-time deduplication destroys temporal priority on a shared attribute.**
   The same [plan.rs:254](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation/plan.rs:254) permanently removes earlier rules.

   For a black rectangle with a red `<set begin="0s" dur="1s">` followed by a blue `<set begin="1s" dur="1s">`, the rectangle is black at 0.5 seconds: the red rule was deleted, and blue has not begun. Earlier animation also cannot reappear after a later rule removes its effect. Keep rules grouped by target attribute and resolve active/frozen contributions during evaluation.

8. **P2 — The sampled period loses begin offsets and invents incorrect composite loops.**
   [svg_animation/plan.rs:152](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation/plan.rs:152) takes the maximum active duration, excluding `begin`.

   A rule with `begin="2s" dur="1s" repeatCount="indefinite"` is sampled only below one second, so every frame is the base state. With independent two-second and three-second indefinite rules, the sequence wraps after three seconds although their joint period is six seconds; the shorter rule jumps phase. Represent the introductory interval separately, and advertise a repeatable period only when it is exact.

9. **P2 — Finite animations never display their terminal freeze/remove state.**
   [svg_animation/sampler.rs:65](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation/sampler.rs:65) samples a half-open interval ending before the active duration.

   `from="0" to="100" dur="1s" fill="freeze"` at two fps produces `[0, 50]`. Default `image-animate` plays once and leaves 50, never the frozen 100. With `fill="remove"`, playback likewise leaves an active sample instead of restoring the base value. Evaluator-only freeze/remove tests miss this integration failure.

10. **P2 — Valid equal keyTimes can permanently stall interpolation.**
    [svg_animation/eval.rs:107](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation/eval.rs:107) selects any zero-width interval regardless of whether its time has passed.

    With `values="0;10;20;30"` and `keyTimes="0;0.5;0.5;1"`, evaluation at cycle fraction 0.75 selects the middle interval and returns 10 instead of 25. Equal successive times are permitted; expired zero-width intervals must be skipped. [SVG value semantics](https://www.w3.org/TR/SVG11/animate.html#ValueAttributes)

11. **P2 — Discrete keyTimes and fractional repeat counts are evaluated incorrectly.**
    [svg_animation/plan.rs:478](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation/plan.rs:478) requires the final keyTime to equal one, including discrete animation. Valid `keyTimes="0;0.2;0.8"` therefore becomes uniform timing: green starts at one-third instead of 0.2. [SVG value semantics](https://www.w3.org/TR/SVG11/animate.html#ValueAttributes)

    [svg_animation/plan.rs:313](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation/plan.rs:313) truncates fractional `repeatCount`. A valid half-cycle becomes zero cycles and falls back to static; `1.5` becomes one cycle and freezes at the wrong value. Represent fractional active duration rather than an integer repeat count. [SVG timing semantics](https://www.w3.org/TR/SVG11/animate.html#TimingAttributes)

12. **P2 — Patching corrupts valid single-quoted attributes.**
    [svg_animation/patch.rs:73](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation/patch.rs:73) replaces the value while retaining its original delimiters; [eval.rs:191](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation/eval.rs:191) strips only double quotes.

    Given gradient `g`, `fill='red'` animated to `url('#g')` becomes `fill='url('#g')'`, invalid XML. Sampling silently falls back to static red. XML-decoded ampersands can similarly invalidate replacements or insertions. Track delimiters and escape XML values; stripping characters is not serialization.

    The descending-offset strategy itself looks sound: each sample starts from the exact bytes used for compilation, before downstream SVG rewrites. I found no independent stale-range defect there.

13. **P2 — Unicode values can panic and leak sequence lifecycle state.**
    [svg_animation/plan.rs:437](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation/plan.rs:437) slices strings at assumed ASCII byte boundaries.

    `values="#éx;#fff"` produces a three-byte “hex” string, then slicing `0..1` splits `é` and panics. The worker catches the panic and reports a failed load, but [image_sequence.rs:326](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/image_sequence.rs:326) has already incremented `in_flight` without an unwind guard. Retirement can retain that sequence’s tombstone indefinitely. Validate ASCII hex and finish decode accounting through RAII.

14. **P2 — The raw-byte prefilter misses supported SVG forms.**
    [svg_animation.rs:38](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation.rs:38) searches only literal `<animate` and `<set`, before decompression.

    SVGZ documents and namespace-prefixed `<s:animate>` elements remain static under enabled policy, even though the static decoder supports gzip and the compiler recognizes local element names. Perform detection on normalized document bytes or during parsing.

15. **P2 — The Elisp surface overrides explicit opt-outs and animates an undisplayed copy.**
    [neomacs-image.el:182](/home/exec/Projects/github.com/eval-exec/neomacs/lisp/neomacs-image.el:182) uses `plist-get` to test property presence. With the default set to `t`, explicit `:animation nil` gains a duplicate `:animation t`; the Rust parsers take the last value and enable animation. Use `plist-member`.

    Separately, [neomacs-image.el:193](/home/exec/Projects/github.com/eval-exec/neomacs/lisp/neomacs-image.el:193) copies the spec before passing it to `image-animate`. That function advances by destructively changing its argument. An already-displayed original spec never advances, although the helper returns a timer. Under the default nil customization, the helper also adds no enabling property.

16. **P2 — The oracle test can pass without evaluating an image.**
    [image_svg_animation.rs:35](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neovm-oracle-tests/src/divergence/image_svg_animation.rs:35) interpolates `ANIMATED_SVG` directly into Elisp without string quoting.

    The resulting `:data <svg xmlns="..." ...` evaluates an unbound `<svg` symbol. I reproduced `(void-variable <svg)` in GNU Emacs. The parity helper accepts equal captured evaluations, including matching errors. Escape the document as an Elisp string and assert successful, expected results.

**Policy-off and threading assessment.** The direct policy gate plus producer-kind checks close the warmed-computed-cache leak for directly disabled requests. Cache publication and retirement are mutex-protected; I found no additional new publication/retirement race beyond inconsistent concurrent results and missing unwind cleanup. However, static SVG `:index > 0` rejection already exists on `main`, despite the new test’s claim that it is GNU-ignored. This PR has not established its stronger “byte-for-byte GNU” guarantee.

**Coverage worth adding.** Prioritize end-to-end file/data decoding, rotation/HiDPI, same-source face and size changes, barrier-controlled concurrent publication/retirement, sampled finite endpoints, delayed starts, stacked rules, multiple insertion targets, quote/entity preservation, and budget admission. Add scheduler-contract tests before using `AnimatedVisual`: [plan.rs:179](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-renderer-wgpu/src/svg_animation/plan.rs:179) predicts discrete boundaries using `N−1` intervals while evaluation uses `N`, excludes finite end events, and can report events before activation.

I ran existing prebuilt binaries: 17 SVG engine tests, the warmed-cache disabled-policy regression, and 14 protocol tests passed. No fresh build was performed, and nothing was modified.

## SECTION 2 — IDEAL LONG-TERM DESIGN/ABSTRACTION

**Unify playback, scheduling, readiness, and presentation lifetime. Keep format semantics and execution backends separate.** A precomputed frame sequence should be one backend capability, not the universal representation of temporal media.

The crucial identities should be distinct:

| Type | Responsibility |
|---|---|
| `MediaAssetId` + generation | Immutable source content and compiled/decoded source state |
| `PlaybackId` + epoch | Clock, play/pause, seek, rate, repeat policy |
| `MediaViewId` | Placement, clipping, visibility, realization and retained-layer dependency |
| `SampleKey` | Asset generation, sample identity and complete materialization recipe |

Two views may explicitly share a `PlaybackId` for synchronized playback. Two uses of the same asset should also be able to start independently. The assertion in [media_clock.rs:10](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-display-protocol/src/media_clock.rs:10) that identical sources must share a timeline is too restrictive.

**Put the boundaries here:**

- **Protocol:** transportable identifiers, playback commands, source descriptors, view bindings, geometry, policy and status events. Keep GNU image-spec identity and explicit `:index` behavior as compatibility contracts.
- **A backend-independent media crate:** authored frame timing, typed media time, playback mapping, timeline capabilities, and pure SVG/Lottie compilation and evaluation. XML parsing and curve evaluation should not require `wgpu`.
- **Render-thread runtime:** playback instances, visibility policy, sample requests, completion reconciliation, generation checks and scheduler aggregation.
- **Renderer/backend adapters:** rasterization, GPU upload/import, shader execution, retained surfaces and damage. Native video retains its demux, decoder, audio synchronization and surface ownership.

The temporal interface should answer a **time-dependent** scheduling question, roughly:

```rust
struct TemporalHint {
    next_change: Option<MediaTime>,
    continuous_until: Option<MediaTime>,
    ended: bool,
}
```

A global `is_continuous()` cannot describe a delayed start, a continuous interval followed by a frozen state, or alternating discrete and continuous segments. Represent finite endings explicitly. Expose an exact period only when proven; a mixed SMIL document can have an introductory interval and no practical common period.

Sampling should return ready content, pending work, or a terminal state, with sample validity and geometry. Payloads can differ: CPU pixels, an imported video surface, a retained vector scene, or shader parameters. Avoid sending GPU objects across the VM protocol.

**Preserve the existing separation between decoder service and repaint authorization.** [frame_sched.rs:604](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-display-runtime/src/render_thread/frame_sched.rs:604) already distinguishes a video service deadline from frame demand. Generalize that model: reaching a decode deadline services work; publishing changed ready content authorizes damage. Hold the last ready sample while work is pending.

Aggregate scheduling across visible playback instances before submitting `DemandReason` demands. [frame_sched.rs:585](/home/exec/Projects/github.com/eval-exec/neomacs/crates/neomacs-display-runtime/src/render_thread/frame_sched.rs:585) stores one deadline per reason per window, so independent producers must not overwrite each other. Keep playback phase in playback state; use reason-level cadence to coordinate presentation opportunities.

**Separate rate limits from cache bounds.** A 30-fps ceiling should limit work per second. It should not imply eagerly storing an entire loop or slowing a long animation to fit 256 slots. Use lazy samples with a byte-bounded cache, bounded prefetch, admission before allocation, and coalesced in-flight jobs keyed by the complete sample request. Cancellation and retirement should invalidate generations; RAII should release every reservation and decode registration.

Authored GIF/APNG/WebP can retain delay tables and decode checkpoints. SVG/Lottie can retain compiled plans and materialize only requested states. Video keeps timestamped frames and decoder queues. Shader surfaces often need no CPU frame cache at all.

**Keep geometry and pixel stages explicit.** Distinguish native pixels from realized pixels in types. Preserve geometry with realized output. For vectors, rasterize directly at the desired realization; for authored rasters, realize decoded native pixels once. That distinction would prevent this PR’s double-transform bug.

Animated images also need stable view bindings in retained content: text layout places the media view, while frame advancement updates its surface without rebuilding VM-side image specs. Route resulting damage to the layer containing that view. Whole-document transform/opacity animation can sometimes be composited cheaply; subtree animation requires correctly isolated subtrees, clips, filters and ordering. Moving every transform rule to compositor operations is not automatically equivalent to SVG rendering.

**Starting clean, I would retain** the opt-in policy, pure compile/evaluate split, typed requests, and generation fencing. **I would replace or undo**:

- Eager whole-document conversion into GIF-shaped frame arrays.
- Source-only ownership of realization-specific pixels and sampling grids.
- A clock intrinsically owned by source identity.
- The fixed `next_event`/global `is_continuous`/single-period contract.
- Compile-time deletion of competing animation rules.
- XML string patching as the permanent execution representation.
- VM timer/index mutation as the native playback mechanism.

GNU’s `image-animate` and `:index` should remain supported through a compatibility adapter. Native temporal playback should have its own playback instance and render-thread advancement contract.