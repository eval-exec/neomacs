# Child-frame lifecycle animation

Date: 2026-09-30
Status: implemented (open/close with fade+slide; movement/resize wired, reserved)

## What this is

Child frames — posframe popups, Corfu, company — get the same four-slot
lifecycle animation vocabulary the window animations use (niri's). One slot
per lifecycle event:

| Slot | Default | Animates |
|------|---------|----------|
| `child-frame-open` | **on**, 150ms ease-out-expo | opacity 0→1, plus an 8px rise from below the placement |
| `child-frame-close` | **on**, 150ms ease-out-quad | opacity 1→0, no displacement by default |
| `child-frame-movement` | **off** | placement drift toward a re-anchored frame |
| `child-frame-resize` | **off**, reserved | wired for a future content crossfade |

Plus two globals: `child-frame-animations.off` (master switch) and
`child-frame-animations.slowdown` (shared multiplier, 0.05–20.0).

The slots ship `open`/`close` **on** and `movement`/`resize` **off**:

* Open: a popup fading in costs only the frames it animates. Nothing needs
  the previous picture, so there is no offscreen composition demand.
* Close: a deleted child frame's ground is the parent frame that was always
  underneath — unlike a deleted *window*, whose fade shows its successor
  half-drawn underneath (the documented reason `window-close` ships off).
* Movement/resize: both animate toward where Emacs *will* anchor the popup,
  and a tooltip pointing at one character reads a few pixels behind as a
  positioning bug. Exactness is the stock default.

## Design decisions

**Sampler, not stepper.** Lifecycle animations reuse the pane-motion
`sampler/Motion` kernel (`frame_compositor/motion.rs`): a `MotionSpec` bound
to an origin `EventTime`, sampled against the draw frame. The same instant
always draws the same picture, so coalescing, dropped frames and scheduler
cadence cannot change where a fade lands, and the finish condition is a fact
about time.

**Displacement shares the slot's curve.** `slide_pixels` (logical px) rides
on the slot, and the offset is `(1 - progress) * slide` for open, `progress *
slide` for close. Raw (unclamped) progress drives displacement, so a spring
slot overshoots through its placement and settles — the pop effect — while
opacity reads `content_mix`, which never overshoots.

**Retention for close.** `ChildFrameManager` keeps a `dying: Vec<DyingChildFrame>`.
`remove_child_frame` retires the subtree into it when the close slot is
non-`Instant`, instead of dropping the entries. Dying frames:

* draw first, beneath living frames, in their own z-order — a dismissed
  popup is normally replaced by the popup that took its place, and the old
  picture receding underneath reads as the new one arriving;
* are unconsultable for interaction: hit-testing, cursor resolution,
  presentations, toolbar and media all read the live map only, so a dying
  popup cannot be clicked, focused, or resolved as a cursor target;
* are pruned on the same draw sample that saw them finish;
* are resurrected by a re-delivery: `update_frame` cancels a dying entry for
  the same id before installing, so Emacs re-showing a popup mid-fade brings
  it back rather than double-animating.

**Open fires on install, never on refresh.** `update_child_frame` starts the
open animation only when the id was absent from the live map. A completion
popup refreshes per keystroke; each refresh carries the previous animation
forward (`update_frame` preserves `entry.animation` across the payload
replace) so the fade cannot restart mid-flight and read as flicker.

**Global alpha is a uniform, not a vertex change.** The renderer's shared
`Uniforms` gained `content_alpha` (the `_padding` slot, same 16 bytes), the
`DrawParameterCache` key grew to four bits-words, and `parameters_with_alpha`
is the only new way to make draw parameters. `render_child_frame` takes one
`alpha` and multiplies it into background alpha and shadow opacity directly;
glyph, coverage, rect, rounded-rect, image, texture and video shaders
multiply their output alpha by `uniforms.content_alpha`. No vertex builder
changes shape, and alpha 1.0 produces the identical cache key as before, so
a settled popup draws through the same immutable snapshot as ever.

**Scheduler demand.** Lifecycle animation has a standing demand reason of
its own (`DemandReason::ChildFrameMotion`), evaluated like the pane-motion
demand in `declare_frame_demands`. Without it, a fading popup on an idle
frame freezes the moment nothing else schedules. The demand check samples at
the observed now with a zero interval — sampling is monotonic in `now`, so an
early sample can at worst retract one poll early.

**Quality policy.** A software adapter (`QualityMode::SoftwareCompatibility`)
does **not** decline this family, unlike pane morphs. A morph animates the
whole tiling and needs the previous picture composed offscreen; a child-frame
fade draws the popup's own pixels onto the retained root scene — the same
work the popup itself already costs every frame — and the software-mode GUI
suites (close confirmation, this verification) already sustain display-rate
demand for exactly that kind of redraw. Turning slots off remains the user's
`enabled` setting. `software_compat_visual_config` therefore leaves
`child_frame_animations` untouched.

**Policy publication.** `RenderQualityPolicy::child_frame_motion()` resolves
the four specs plus slide distances from the effective visual config; they
are published to each top-level window's `FrameCompositor::child_frame_motion`
alongside `pane_motion`, so a fade in flight finishes on the spec it started
with when the policy changes.

## Configuration surface

Elisp, one defcustom per property, as `lisp/neomacs-effects.el` does for
windows: a new `neomacs-child-frame` group, generated through
`neomacs-effects--defslot` (extended with `:group`, `:master`, `:enabled-doc`
and `:slide` keys — all optional, the window slots unchanged), the master
switch `neomacs-child-frame-animations-off`, and
`neomacs-child-frame-animations-slowdown`. Everything pushes through the
existing `neomacs-effects--set` → `neomacs--effect-set` → `VisualConfig`
registry path — no new plumbing — and `neomacs-effects--check-schema`
verifies the options still match the Rust schema at load time.

```elisp
(setq neomacs-child-frame-open-easing 'ease-out-expo)   ; shipped default
(setq neomacs-child-frame-open-slide-pixels 8.0)        ; shipped default
(setq neomacs-child-frame-close-enabled nil)            ; instant dismissal
(setq neomacs-child-frame-animations-slowdown 10.0)     ; watch mode
(setq neomacs-child-frame-animations-off t)             ; reclaim it all
```

## Shipped after the first pass

* **Scale/pop-in** (`scale_from`, default 1.0 — off). The shared uniform
  snapshot grew to 32 bytes carrying `content_scale` and `content_pivot`;
  seven shaders scale positions away from the anchor, the rounded-rect SDF
  scales its rect bounds and thins its border width and radius by the same
  factor, and the CPU-built chrome geometry scales identically. The identity
  normalizes to (1.0, origin) so every settled frame reuses the exact
  draw-parameters entry the pre-scale pipeline produced.
* **Resize content crossfade**: a size-changing update leases the previous
  presentation's picture from the snapshot pool, paints it, and crossfades —
  the old picture fading out beneath the new frame, whose alpha rides the
  mix — over the resize slot's curve. Ships off (`resize` slot disabled by
  default); any failure (slot off, size unchanged, pool refusing the lease)
  degrades to GNU's instant swap. Leases return on finish and on device
  loss.
* **Dying frames interleave** into the z-ordered draw (`merged_render_order`),
  a corpse drawing before a living frame at equal z — the dismissed popup
  recedes beneath the popup that replaced it.

**The retained-static texture must not freeze a fade.** The retained
cursorless scene is rebuilt only on a scene-generation change, and a
lifecycle event changes the scene *without* an ingest: the texture would
otherwise hold the departed popup at whatever alpha the last build saw, and
every composite-only present after the fade would blit it back at full
opacity. Three pieces keep it honest: the retained path is ineligible while
`has_animation_activity()` reports a running animation or a retained corpse;
the scene generation is bumped when a subtree is retired; and finished open
animations are cleared after their pass so the ineligibility ends.

## Verification

End to end, over composited pixels: `crates/neomacs-gui-tests/
tests/child_frame_animation.rs` runs a popup through its whole lifecycle on
a headless sway session (`grim` captures the compositor's output — the
render thread's own readback deliberately sees only the root scene). With
`slowdown 20` (the clamp ceiling, 3s fades) it asserts:

* the open fade: captures at 1s cadence go background → mid-ramp → settled;
  the render thread's own alpha log shows the living popup's alpha rising
  monotonically from < 0.5 to > 0.9;
* the settled popup clearly present over the background;
* the close fade: four captures strictly brightening from the settled
  popup, reaching at least halfway; the dying alpha log falling
  monotonically from ~0.99 to ~0.00002 over the full 3s tween;
* after the prune, the popup's region equals the pre-popup background —
  the corpse's pixels are actually cleared, which is what the
  retained-texture fixes above exist to guarantee;
* the resize content crossfade: the popup grows mid-life, the strip beyond
  its old size fills with red as the new picture fades in (strictly falling
  captures, the crossfade's own mix series rising from < 0.5 in the log);
* `compositor_remove animated=true` and the retire/prune lifecycle in the
  log.

Stable across seven consecutive runs (~20s each). Unit coverage: protocol
(motion conversion, clamp semantics, registry round-trip), manager
(sampling, carry-over, instant no-op, retire-to-dying subtree semantics,
instant close, resurrection on re-delivery, prune timing, dying z-order),
time discipline (wall-clock guard), renderer pixel tests.
