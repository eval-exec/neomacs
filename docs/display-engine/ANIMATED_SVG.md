# Animated SVG (computed animation)

Status: implemented — elisp-driven increment. Render-paced driving is the
declared follow-up (see "Pacing" below).

GNU Emacs renders SVG through librsvg, which has no document clock: an
animated SVG displays as one static frame and `image-multi-frame-p`
returns nil. Neomacs can instead *compute* the frames an SVG's SMIL
timeline defines, expose them through the same machinery as GIF frames,
and let everything downstream (`image-animate`, `:index`, the sequence
cache budget) work unmodified. Because that changes observable behavior,
it is strictly opt-in per image spec.

## The gate

The `:animation` image-spec property (a Neomacs extension; GNU has no such
key):

```elisp
;; static, GNU-identical (the default, also for absent/nil/zero values)
(list 'image :type 'svg :data svg-text)

;; materialize frames at the default 30 fps sampling ceiling
(list 'image :type 'svg :data svg-text :animation t)

;; materialize with an explicit ceiling — also the frame-count bound
(list 'image :type 'svg :data svg-text :animation 12)
```

`neomacs-svg-animation` is the defcustom default packages can spread in
(`neomacs-image-spec-add-animation`). Oracle parity runs never set it:
the disabled policy is the parity boundary, pinned by
`divergence/image_svg_animation.rs`.

## Architecture

Three concerns, strictly separated:

1. **Timeline model** (`neomacs-renderer-wgpu/src/svg_animation/plan.rs`) —
   the document's animation elements compiled once into pure data: byte
   sites in the source text, parsed keyframe values, timing. Nothing
   re-parses the document per frame.
2. **Evaluation** (`eval.rs`) — pure `(plan, document time) → attribute
   values`. No clocks, no I/O; unit-tested as data in, data out.
3. **Sampling** (`sampler.rs` + `patch.rs`) — a
   `display_protocol::animated_visual::SampleGrid` quantizes the loop;
   each slot is evaluated, byte-spliced into the source, and rasterized
   through the ordinary `svg.rs` path, producing frames shaped exactly
   like decoded GIF frames (dimensions + exact rational delay).

Frames are published through the image sequence cache
(`image_sequence.rs::resolve_svg`), so computed animation shares the
64 MiB sequence budget, LRU eviction, and retirement fencing with authored
animation. `MediaClock` (display-protocol) is the presentation-time ⇄
document-time mapping the pacing increment will drive.

### The SMIL subset

`<animate>`, `<animateTransform>`, `<set>` with `from`/`to`/`values`,
`dur`, `begin` offsets, `repeatCount`, `fill`, `calcMode`
`linear`/`discrete` (spline/paced degrade to linear), `keyTimes`, and
targets by parent or `href`. Anything outside the subset drops its rule —
unsupported animation degrades toward the static frame, never a failed
load.

### Bounds

- `period × fps` slots maximum, hard-capped at `SampleGrid::MAX_SLOTS`
  (256); the fps ceiling is therefore a memory bound as much as a rate.
- Per-frame raster is bounded by the ordinary SVG caps (8 MiB input,
  64 MiB raster).
- Sampling happens on the background decode pool, like every other
  decode.

## Threading

The evaluator (elisp VM) thread participates only at image load and
retirement, exactly as for GIF: a spec resolves, a load command carries
the policy, the decode pool materializes frames. In this increment frame
*advancement* is driven by `image-animate`'s timer walking `:index` —
GNU's own mechanism, and what neomacs uses for GIF today.

**The follow-up increment** moves advancement to the render thread: a per
-sequence `MediaClock` (epoch at first presentation, paused while
unpresented), frame indices derived from presentation time on the
scheduler's phase-anchored grid, and a new `DemandReason` cadence
(`AnimatedImage`) declaring `Cadence::At`/`MaxRate` from the plan's
`AnimatedVisual` answers (already implemented on `AnimationPlan`). The
scheduler and layer design (`frame_sched.rs`, `LayerMask::MEDIA`) needs
no changes for that — the vocabulary exists. Until then, the elisp path
is the driving mechanism, and the VM-thread cost is one load command per
frame change while animating (identical to GIF under `image-animate`).
