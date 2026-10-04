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
