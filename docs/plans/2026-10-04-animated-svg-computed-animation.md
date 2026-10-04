# Plan: animated SVG — computed animation

Date: 2026-10-04 · Branch: `feat/svg-animation`
Design doc: `docs/display-engine/ANIMATED_SVG.md`

Research finding (2026-10-04): no Rust open-source project plays animated
SVG. resvg/librsvg/THORVG are static by design; vello makes per-frame
re-rendering cheap but ships no timeline engine. The SMIL engine is
therefore in-repo, in `neomacs-renderer-wgpu/src/svg_animation/`.

## Stages

1. **Protocol contracts** ✅ — `MediaClock`, `AnimatedVisual` +
   `SampleGrid` (exact rational delays, MAX_SLOTS cap), and
   `ImageAnimationPolicy` as the opt-in divergence carrier.
2. **Engine** ✅ — plan/eval/patch/sampler; sequence-cache integration
   (`resolve_svg`, `DecodedImageSequence::from_frames`); 17 engine tests.
3. **Policy plumbing** ✅ — `:animation` spec property parsed by both the
   evaluator (`neovm-core`) and the layout engine; carried on
   `ImageResolveRequest`, `AssetCommand::ImageLoad*`, `DecodeRequest`;
   parser unit test; oracle parity test for the disabled default.
4. **Elisp surface + docs** ✅ — `neomacs-svg-animation` defcustom,
   `neomacs-image-spec-add-animation`, `neomacs-image-animate-svg`.

## Deliberately deferred

- **Render-paced driving** — advancement via per-sequence `MediaClock`
  and a `DemandReason::AnimatedImage` cadence, replacing the elisp-timer
  walk. All vocabulary for it is implemented (`AnimatedVisual` on
  `AnimationPlan`); the wiring is the follow-up. The elisp-driven path
  (GNU's own mechanism, shared with GIF) is the interim driver.
- **Compositor-tier execution** — the plan evaluator's rule
  classification seam exists (attribute-override vs compositor-op);
  routing transform/opacity rules to compositor ops instead of re-raster
  is the performance endgame.
- **Vello rasterization** — the sampler calls `svg::decode` through the
  same seam every static SVG uses; swapping the rasterizer needs no
  timeline changes.

## Verification

- `cargo nextest run -p neomacs-display-protocol -p neomacs-renderer-wgpu
  svg_animation` — engine + contracts.
- `cargo nextest run -p neovm-core image_spec_animation` — property
  parsing.
- Oracle parity (env-gated): `divergence_animated_svg_static_by_default_
  matches_gnu`.
- Full affected-crate suites: only pre-existing environmental failures
  (missing imgmsg fixtures; GUI/daemon tests requiring a display) —
  verified identical on main.

## Review round 2 (codex, gpt-6.1-sol high — full text in the companion file)

4×P1 + 12×P2 found. Fixed in this round:

- P1 budget-after-allocation → admission during sampling
  (`MAX_COMPUTED_SEQUENCE_BYTES`, exact projection after slot 0).
- P1 double realize → sampler decodes at intrinsic extent; bitmap
  realization applies size/rotation/scale exactly once, like GIF frames.
- P1 realization-blind cache key → entries record the baked face colors;
  a color change replaces the entry instead of serving stale pixels.
- P1 file-backed policy ignored → `decode_file` gains the gated computed
  arm with the file's resource context.
- P2 unicode hex panic → ASCII validation before octet slicing.
- P2 plist-get vs plist-member in `neomacs-image-spec-add-animation`.
- P2 vacuous oracle test → document embedded as a quoted Elisp string;
  the `:index`-on-static arm was dropped (its GNU-ignored claim was
  untested commentary, and main already rejects static `:index > 0`).

Review round 3 (coderabbit + Copilot, fixed in 229aeab9e6): dedup key
now includes the target element; loop periods are begin-aware with
steady-state sampling for looping plans; next_event never reports
pre-activation boundaries; neomacs-image-animate-svg mutates the
displayed spec in place; the parity test pins the value, not just
agreement.

Still tracked for the render-paced increment (not v1 blockers, all in
the enabled-policy path): zero-width keyTimes selection, discrete
keyTimes last<1, fractional repeatCount, single-quote/entity-safe value
serialization, SMIL sandwich ordering instead of last-wins, finite
freeze/remove endpoint sampling, SVGZ/namespaced prefilter,
concurrent-miss grid coherence, in_flight RAII on panic.

Section 2 (design) to be weighed against this plan's deferred list —
its MediaAsset/Playback/View/SampleKey identity split is the stronger
long-term shape and supersedes "one clock per source".
