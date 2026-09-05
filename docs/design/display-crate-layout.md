# Display-crate layout — where display code lives

`docs/design/neovm-core-layout.md` governs placement inside `neovm-core`. This
is the equivalent authority for the four display crates. It exists because its
absence has a measurable cost: `render_thread/` grew to 22k lines in one flat
directory with no stated seam, and a shared type's home had to be re-derived
from the dependency graph every time one was added.

## The dependency direction, which is the whole rule

```
neomacs-display-protocol      (depends on nothing in the workspace)
        ^            ^
        |            |
neomacs-layout-engine         neomacs-renderer-wgpu
        ^                             ^
        |                             |
        +------ neomacs-display-runtime ------+
```

`neomacs-renderer-wgpu` and `neomacs-layout-engine` are **dependencies of**
`neomacs-display-runtime`, not siblings of it. Two consequences that are easy
to get wrong:

1. A type shared between the runtime and the renderer **must** live in
   `neomacs-display-protocol`. The renderer cannot see runtime types. This is
   not a style preference — it is the only place both can name.
2. The renderer cannot call back into the runtime. Anything the renderer needs
   to know arrives as a parameter.

## What each crate owns

### `neomacs-display-protocol` — the vocabulary

Immutable wire and domain types that more than one crate names. No GPU
resources, no threads, no I/O, no mutable animation state.

Lives here: geometry and its phantom spaces, `FrameGlyphBuffer` /
`FrameDisplayState` / `GlyphRow`, the presented-pointer index, `PresentMapping`,
`InteractionProjection`, `MotionSpec`, `EventTime` / `FrameSample`,
`VisualConfig` and the effect registry, `PresentationOrigin`, id newtypes.

The test of belonging: **could two crates disagree about this if each had its
own copy?** If yes, it belongs here.

This crate has zero `#[allow(dead_code)]`. Keep it that way — it is the one
crate where an unused type means nobody agreed on it.

### `neomacs-layout-engine` — the producer

Turns buffer text, faces, overlays and window structure into a sealed
`FrameDisplayState`. Knows about fonts, wrapping, bidi, display properties. Does
not know that a GPU exists.

Emits **facts about what is displayed**, never instructions about how to draw
it. A producer that names an effect is a design error: see the deletion of
`WindowEffectHint`, which declared animations from numbers nobody measured.

### `neomacs-renderer-wgpu` — the drawing adapter

Owns every GPU resource: pipelines, shaders, the glyph atlas, offscreen
textures, effect state. Draws what it is told to draw, at the time it is told.

It must not decide **whether** an effect happens or **how far along** it is.
Both are the compositor's job; the renderer receives the answer. A trigger that
mints its own `observe_platform_now()` instead of taking the frame's
`EventTime` is the recurring version of this mistake.

### `neomacs-display-runtime` — the render thread

Owns the event loop, window lifecycle, input, frame scheduling, and the
compositor. This is the only crate that may hold mutable temporal state.

Submodule charters — a new file needs one of these to belong:

- `render_thread/frame_compositor/` — **retained presentation state and every
  decision derived from it.** What is currently displayed, what changed since,
  what that means for motion. Its `continuity/` submodule holds one file per
  *fact* measurable by diffing two presentations (scroll, reflow, selection,
  theme, pane layout). A new derived effect adds a file there, not a branch
  elsewhere.
- `render_thread/frame_sched.rs` — when to draw next, and nothing else.
- `render_thread/pointer_events.rs`, `window_events.rs` — platform input in,
  evaluator events out.
- `render_thread/render_pass/` — the draw order for one frame, and one
  submodule per phase of it. `mod.rs` owns the sequence and nothing else:
  acquire, sample the pane motion, pick a composition strategy, draw through
  it, hand the result to `present`. Each phase owns how it draws — `surface`
  (getting the swapchain texture and naming how that fails),
  `composition_targets` (the offscreen textures a frame composes through),
  `retained_static` and `full_render` (the three composition strategies),
  `scene` (glyphs, child frames, content overlays), `chrome` (the window-level
  overlays drawn over them), `present` (handing the result to the platform and
  publishing the projection). When a body in `mod.rs` grows past "call the
  phase, check the outcome", it belongs in a phase.
- `render_thread/frame_ingest.rs` — presentations in from the evaluator.
- `backend/tty/` — the terminal backend, which shares the protocol and nothing
  else.

## Rules that have already caught mistakes

1. **Time is a parameter, never a field.** `EventTime` is a stored observation
   of when something happened; `FrameSample` is passed to the code drawing one
   frame and must not be stored or fabricated. Production code may not call
   `Instant::now()` or `.elapsed()`;
   `render_thread/time_discipline_test.rs` fails the build on a raw clock read
   and requires a written justification per allowlisted file.
2. **Never `#[allow(dead_code)]`.** Wire it, delete it, or mark it
   `#[cfg(test)]` if only tests need it. An unreachable helper is a design that
   was abandoned without being removed.
3. **Facts, not instructions, cross a crate boundary.** The producer says what
   is on screen; the compositor decides what that means; the renderer draws the
   result.
4. **Make the illegal state unrepresentable before adding a check.**
   `PresentedHitQuery` takes a witnessed point rather than two `f32`s because a
   validating constructor can be bypassed and a missing constructor cannot.
5. **A config value that Lisp can set must be scalar.** The effect registry is
   serde reflection over `VisualConfig` and carries only scalars plus
   `Duration`; a field that serializes to an object breaks
   `neomacs-effects-apply` for every effect, not just its own.

## What this document does not govern

Crate boundaries themselves. They are load-bearing and stable; changing one is
a design decision, not a cleanup. `neovm-core` placement is governed by
`neovm-core-layout.md` and by GNU's own file structure — do not reorganize it
on aesthetic grounds.
