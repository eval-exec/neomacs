# Neomacs Display Engine — Architecture

## Overview

Neomacs replaces Emacs's entire display subsystem with a Rust + wgpu GPU-accelerated
renderer. Emacs C code builds glyph rows as before, but instead of drawing to X11/GTK,
it sends glyph data across an FFI boundary to a Rust render thread that runs a winit
event loop and renders via wgpu (Vulkan/Metal/DX12/OpenGL).

```
┌─────────────────────────────────────────────────────────────────────┐
│                       Emacs Core (C/Lisp)                           │
│                                                                     │
│  xdisp.c ─── glyph rows ───► neomacsterm.c ─── C FFI ───►         │
│                                                                     │
│  neomacs-win.el (Lisp config, animation defaults)                   │
└────────────────────────────────────┬────────────────────────────────┘
                                     │  neomacs_display.h (C header)
                                     │  ffi.rs (Rust extern "C")
┌────────────────────────────────────▼────────────────────────────────┐
│                Rust Display Engine (neomacs-display crate)           │
│                                                                     │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                      Render Thread                             │ │
│  │  render_thread.rs — winit event loop, input dispatch,         │ │
│  │                     frame management, animation state          │ │
│  │  thread_comm.rs   — typed command/event channels               │ │
│  └──────────────────────────────┬─────────────────────────────────┘ │
│                                 │                                    │
│  ┌─────────────┐  ┌────────────▼───────────┐  ┌──────────────────┐ │
│  │   Core      │  │    wgpu Backend        │  │  Media           │ │
│  │             │  │                        │  │                  │ │
│  │ scene.rs    │  │ renderer.rs            │  │ video_cache.rs   │ │
│  │ face.rs     │  │  glyph rendering       │  │  GStreamer       │ │
│  │ glyph.rs    │  │  image compositing     │  │  VA-API decode   │ │
│  │ grid.rs     │  │  animation rendering   │  │  DMA-BUF import  │ │
│  │ types.rs    │  │  scroll effects        │  │                  │ │
│  │ frame_      │  │  buffer transitions    │  │ webkit_cache.rs  │ │
│  │  glyphs.rs  │  │  cursor particles      │  │  WPE WebKit      │ │
│  │             │  │                        │  │  offscreen views │ │
│  │ animation   │  │ glyph_atlas.rs         │  │                  │ │
│  │  _config.rs │  │  cosmic-text rasterize │  │ vulkan_dmabuf.rs │ │
│  │ cursor_     │  │                        │  │  zero-copy GPU   │ │
│  │  animation  │  │ image_cache.rs         │  │  texture import  │ │
│  │  .rs        │  │  GPU texture cache     │  │                  │ │
│  │ scroll_     │  │                        │  │                  │ │
│  │  animation  │  │ 4 WGSL shaders:        │  │                  │ │
│  │  .rs        │  │  glyph, image,         │  │                  │ │
│  │ buffer_     │  │  rect, texture         │  │                  │ │
│  │  transition │  │                        │  │                  │ │
│  │  .rs        │  │ vertex.rs              │  │                  │ │
│  └─────────────┘  └────────────┬───────────┘  └──────────────────┘ │
│                                │                                    │
│                     ┌──────────▼───────────┐                       │
│                     │  winit (Windowing)   │                       │
│                     └──────────────────────┘                       │
└─────────────────────────────────────────────────────────────────────┘
                              │
                 ┌────────────┼────────────┐
                 ▼            ▼            ▼
           ┌─────────┐  ┌─────────┐  ┌─────────┐
           │ Vulkan  │  │  Metal  │  │DX12/GL  │
           │ (Linux) │  │ (macOS) │  │(Windows)│
           └─────────┘  └─────────┘  └─────────┘
```

---

## Threading Model

```
Emacs Main Thread                    Rust Render Thread
─────────────────                    ──────────────────
xdisp.c produces glyphs             winit event loop (60-144 Hz)
  │                                    │
  ├─► neomacsterm.c                    ├─► poll wakeup fd
  │   calls ffi.rs functions           │   process RenderCommands
  │     via C FFI                      │
  │                                    ├─► render frame
  ├─► RenderCommand ──────────────►    │   run animations
  │   (mpsc channel)                   │   submit to wgpu
  │                                    │
  │                              ◄──── ├─► InputEvent
  │   InputEvent                       │   (mpsc channel)
  │   (keyboard, mouse, resize)        │
  │                                    ├─► request_redraw()
  ├─► reads wakeup fd                  │   on animation tick
  │   processes input events           │
```

**Key design decisions:**
- Emacs stays single-threaded — no changes to its threading model
- Render thread owns the window and GPU resources
- Communication is via two lock-free mpsc channels
- A Unix pipe wakeup fd notifies Emacs of pending input events
- Animations run independently of Emacs redisplay cadence

---

## Data Flow: Glyph to Pixel

1. **Emacs** (xdisp.c) computes glyph rows for each window
2. **neomacsterm.c** calls `neomacs_display_render_frame_glyphs()` with raw glyph data
3. **ffi.rs** parses glyph data into `FrameGlyphBuffer` and sends `RenderCommand::RenderFrame`
4. **render_thread.rs** receives command, passes glyph buffer to renderer
5. **renderer.rs** iterates glyph rows:
   - Character glyphs → look up in `glyph_atlas.rs` (cosmic-text rasterized) → textured quads
   - Image glyphs → look up in `image_cache.rs` → textured quads
   - Stretch glyphs → solid color rectangles
   - Video glyphs → look up in `video_cache.rs` → textured quads from GStreamer frames
   - WebKit glyphs → look up in `webkit_cache.rs` → textured quads from WPE offscreen
6. **wgpu** submits GPU commands, presents to surface

All coordinates are in **logical pixels** (Emacs pixel units). The shader's `screen_size`
uniform is logical. The wgpu surface is physical resolution. Glyph textures are rasterized
at physical resolution for HiDPI crispness.

---

## Animation System

Animations run on the render thread independently of Emacs redisplay. The centralized
frame scheduler drives interactive animation up to the display rate and lets ambient
effects declare lower cadences (the cursor color cycle defaults to 24 Hz). Emacs sends
configuration once; the render thread manages state.

### Cursor Animations

**File:** `core/cursor_animation.rs`

The cursor has two independent animation layers:

1. **Movement interpolation** — how the cursor rectangle moves from old to new position
2. **Particle effects** — visual effects triggered on cursor movement

#### Movement Styles (`CursorAnimStyle` in `types.rs`)

| Style | Enum Value | Behavior |
|-------|-----------|----------|
| Exponential | 0 | `pos += (target - pos) * speed * dt` — no fixed duration |
| Spring | 1 | Critically-damped spring: `x(t) = target - (c1 + c2*t) * e^(-ω*t)` |
| EaseOutQuad | 2 | `1 - (1-t)²` over fixed duration |
| EaseOutCubic | 3 | `1 - (1-t)³` over fixed duration |
| EaseOutExpo | 4 | `1 - 2^(-10t)` over fixed duration |
| EaseInOutCubic | 5 | S-curve over fixed duration |
| Linear | 6 | Constant speed over fixed duration |

The **spring style** also supports a 4-corner trail effect where leading corners snap
to the target while trailing corners stretch behind, creating a Neovide-like elastic
rectangle. Controlled by `trail_size` (0.0 = rigid, 1.0 = maximum stretch).

#### Particle Modes (`CursorAnimationMode`)

| Mode | Description |
|------|-------------|
| None | No animation |
| Smooth | Smooth interpolation only (default) |
| Railgun | Particles shoot backward along movement vector |
| Torpedo | Comet trail follows cursor path |
| Pixiedust | Sparkly particles scatter around cursor position |
| Sonicboom | Shockwave ring expands from cursor |
| Ripple | Concentric rings emanate outward |
| Wireframe | Animated outline glow |

Particles are GPU-rendered as colored quads with alpha, velocity, and lifetime.

#### Cursor Blink

Cursor blinking is delegated to the render thread. `neomacs-win.el` advises
`blink-cursor-mode` and suppresses Emacs's blink timer. The render thread handles
the blink interval and alpha animation.

### Buffer Switch Transitions

**Files:** `crates/neomacs-display-protocol/src/transition_policy.rs`,
`crates/neomacs-display-runtime/src/render_thread/transitions.rs`, and
`crates/neomacs-renderer-wgpu/src/renderer/transitions.rs`

When the visible buffer in a window changes, the render thread detects the change
by comparing buffer IDs between frames. It snapshots the old frame to a texture and
applies the configured `buffer-transition` plan to the new frame.

#### Detection (`render_thread.rs:detect_transitions()`)

Each frame, the render thread receives per-window metadata (buffer ID, window-start
position). If buffer ID changes → buffer transition. If window-start changes →
scroll transition.

#### Typed transition plans

The control plane selects a `TransitionEffect`, `TransitionAxisPreference`, and
`TransitionDirection`. `TransitionPolicy` resolves those values with the window
bounds into a `ResolvedTransitionEffect`. That enum makes unsupported states
unrepresentable: directionless effects carry no direction, axis-aligned effects
carry a resolved axis and distance, page curls carry an edge, and intrinsic
vertical/horizontal effects cannot be dispatched on the wrong axis.

#### Rendering

The old frame is stored as a GPU texture (`transition_snapshot`). During the transition,
both old and new frames are composited with effect-specific vertex/alpha manipulation.
Transitions use double-buffered offscreen textures (`offscreen_a`/`offscreen_b`).

### Scroll Animations

**Files:** `crates/neomacs-display-protocol/src/transition_policy.rs`,
`crates/neomacs-display-runtime/src/render_thread/transitions.rs`, and
`crates/neomacs-renderer-wgpu/src/renderer/transitions.rs`

When the window-start position changes within the same buffer, a scroll transition
is triggered. The old frame is snapshotted and the new frame is rendered to the
current offscreen texture. Both are composited with the selected scroll effect.

#### Transition Effects (`TransitionEffect` — 21 variants)

**2D Transitions:**

| # | Effect | Description |
|---|--------|-------------|
| 0 | Slide | Content slides in scroll direction (default) |
| 1 | Crossfade | Alpha blend between old and new positions |
| 2 | ScaleZoom | New frame zooms from 95% to 100% |
| 3 | FadeEdges | Lines near viewport edges fade out (16 strips) |
| 4 | Cascade | Lines drop in with 60ms stagger delay (20 strips) |
| 5 | Parallax | Old scrolls at 0.7x speed, new at 1.0x |

**3D Effects:**

| # | Effect | Description |
|---|--------|-------------|
| 6 | Tilt | Horizontal squeeze simulates perspective tilt (12 strips) |
| 7 | PageCurl | Cylindrical deformation with backside darkening (24 strips) |
| 8 | CardFlip | Height scaling simulates X-axis rotation |
| 9 | CylinderRoll | Horizontal squeeze + brightness from cylinder angle (16 strips) |

**Deformations:**

| # | Effect | Description |
|---|--------|-------------|
| 10 | Wobbly | Sine-wave x-offset with damping, jelly feel (20 strips) |
| 11 | Wave | Sine distortion with animated phase (20 strips) |
| 12 | PerLineSpring | Independent spring physics per line (20 strips) |
| 13 | Liquid | Noise-based xy distortion (20 strips) |

**Post-Processing:**

| # | Effect | Description |
|---|--------|-------------|
| 14 | MotionBlur | Vertical blur via vertex alpha gradient |
| 15 | ChromaticAberration | Slight y-offset on new quad simulates RGB separation |
| 16 | GhostTrails | Semi-transparent copies at offset positions |
| 17 | ColorTemperature | Warm/cool tint based on scroll direction |
| 18 | CRTScanlines | Alternating line alpha for retro scanline effect |
| 19 | DepthOfField | Edge lines dimmed, center lines bright |

**Creative:**

| # | Effect | Description |
|---|--------|-------------|
| 20 | TypewriterReveal | Lines appear left-to-right with stagger |

#### Transition Easing (`TransitionEasing` — 5 variants)

| # | Easing | Formula |
|---|--------|---------|
| 0 | EaseOutQuad | `1 - (1-t)²` |
| 1 | EaseOutCubic | `1 - (1-t)³` |
| 2 | Spring | `1 - (1 + 5.5t) * e^(-5.5t)` (critically damped) |
| 3 | Linear | `t` |
| 4 | EaseInOutCubic | S-curve: `4t³` / `1 - (-2t+2)³/2` |

#### Tessellation

Complex effects (deformations, 3D) use tessellated quads — the content area is divided
into horizontal strips (12-24 depending on effect). Each strip's vertices are displaced
independently, enabling per-line deformation. Helper: `tessellate_quad_strips()` in
`scroll_animation.rs`.

#### Scissor Rects

Scroll effects only apply to the content area of each window. Scissor rects are set in
physical pixels (converted from logical bounds) to clip rendering to the appropriate
region, excluding mode-lines and fringes.

---

## Configuration Pipeline

```
Elisp                    Evaluator / protocol              Render thread
─────                    ────────────────────              ─────────────
(neomacs-effect-set      validate named properties        ConfigCommand::
 'cursor-glow              against VisualConfig             SetVisualConfig(snapshot)
 :enabled t                apply to cloned snapshot              ↓
 :radius 48)               commit atomically              effect/cursor/transition state
```

Each effect keeps its own typed property schema.  Profiles validate completely
before one authoritative snapshot is published, and `VisualConfig` supplies
the defaults and registry names.  See [EFFECTS.md](EFFECTS.md) for the Elisp
surface, ownership rules, and extension contract.

---

## Crate Structure

The display engine is split by ownership instead of living in one renderer
crate:

```
crates/
├── neomacs-display-protocol/        # Shared wire types and transition policy
│   ├── src/visual_config.rs         # User-facing buffer-transition settings
│   ├── src/frame_glyphs.rs          # Semantic window transition hints
│   └── src/transition_policy.rs     # Renderer-ready typed transition plans

├── neomacs-layout-engine/           # Redisplay/layout and semantic hint emission
├── neomacs-display-runtime/         # Window/event loop and transition lifetimes
│   └── src/render_thread/transitions.rs # Snapshot ownership and active plans

└── neomacs-renderer-wgpu/           # GPU resources and rendering
    ├── src/renderer/transitions.rs  # Typed effect dispatch and geometry
    ├── src/glyph_atlas/             # Glyph rasterization and atlas pages
    └── src/shaders/                  # WGPU shaders
```

---

## C Integration

### Files

| File | Purpose | Size |
|------|---------|------|
| `src/neomacsterm.c` | Terminal hooks (redisplay, input, frames) + Lisp DEFUNs | ~156KB |
| `src/neomacsfns.c` | Frame creation, font selection, color handling | ~60KB |
| `src/neomacs_display.h` | C header declaring Rust FFI functions | ~30KB |
| `lisp/term/neomacs-win.el` | Window system initialization, animation defaults | ~8KB |

### Key C→Rust FFI Functions

```c
// Lifecycle
NeomacsDisplay* neomacs_display_init_threaded(const char *title, ...);
void neomacs_display_shutdown_threaded(void);

// Frame rendering — called from neomacsterm.c redisplay hooks
void neomacs_display_render_frame_glyphs(NeomacsDisplay *handle,
    const uint8_t *data, size_t len,
    uint32_t width, uint32_t height, ...);

// Input polling
int neomacs_display_get_threaded_wakeup_fd(void);
int neomacs_display_poll_input(NeomacsDisplay *handle,
    uint8_t *buf, size_t buf_len, size_t *out_len);

// Video
uint32_t neomacs_display_create_video(NeomacsDisplay *handle,
    const char *uri, int width, int height);
void neomacs_display_video_play(NeomacsDisplay *handle, uint32_t video_id);

// WebKit
uint32_t neomacs_display_create_webkit_view(NeomacsDisplay *handle,
    const char *uri, int width, int height);
```

### Key Rust→Emacs Events

The render thread sends `InputEvent` variants back to Emacs via the event channel:

- `KeyPress` — keyboard input (translated via winit key mapping)
- `MouseClick` / `MouseMove` / `MouseWheel` — mouse input (in logical pixels)
- `WindowResize` — window size changed (in logical pixels)
- `WindowClose` — user closed the window
- `FocusChange` — window gained/lost focus

---

## GPU Rendering Pipeline

### Per-Frame Render

```
1. Clear offscreen texture (background color)
2. For each Emacs window:
   a. Set scissor rect to window bounds (physical pixels)
   b. Draw background rectangles (rect.wgsl)
   c. Draw glyph quads (glyph.wgsl) — textured from glyph atlas
   d. Draw image quads (image.wgsl) — from image cache
   e. Draw video frames (image.wgsl) — from video cache
   f. Draw WebKit views (image.wgsl) — from webkit cache
   g. Draw cursor (rect.wgsl or glyph.wgsl with animation)
3. Detect transitions (buffer switch / scroll)
4. If transition active:
   - Render scroll/crossfade effect compositing old + new textures
5. Render cursor particles (if active)
6. Blit final result to window surface (texture.wgsl)
7. Present
```

### Glyph Atlas (`glyph_atlas.rs`)

Uses **cosmic-text** for text shaping and rasterization:

1. Emacs sends font face + character code
2. cosmic-text shapes the glyph using the specified font
3. Glyph bitmap is rasterized at physical pixel resolution (scale_factor applied)
4. Bitmap is packed into a 2D GPU texture atlas
5. Renderer draws a textured quad using atlas UV coordinates
6. At HiDPI, glyphs are crisp because they're rasterized at physical resolution
   but positioned using logical-pixel quads

### Shaders

Four WGSL shaders, all using the same `Uniforms` struct:

```wgsl
struct Uniforms {
    screen_size: vec2<f32>,  // Logical pixels
    _padding: vec2<f32>,
};
```

Vertex positions are in logical pixels. The shader converts to NDC:
```wgsl
let ndc_x = (pos.x / uniforms.screen_size.x) * 2.0 - 1.0;
let ndc_y = 1.0 - (pos.y / uniforms.screen_size.y) * 2.0;
```

---

## Video Pipeline

```
first media command
  └─► initialize platform decoder once
       ├─► Linux: typed Rust GStreamer backend
       ├─► macOS: AVFoundation
       └─► Windows: Media Foundation

Linux GStreamer backend
  ├─► VA-API hardware decode
  ├─► DMA-BUF file descriptor
  │
  ├─► Vulkan import (ash)
  │     VkImportMemoryFdInfoKHR
  │     VkImage from DMA-BUF
  │
  ├─► wgpu texture from Vulkan image
  │     (hal::vulkan → wgpu::Texture)
  │
  └─► Rendered as textured quad
```

On Linux, the full product implements its GStreamer platform backend directly
inside `neomacs-video` through the official Rust bindings. Owned frames and
exhaustive enums cross only private Rust module seams; there is no private C
ABI, runtime loader, second adapter crate, or Neomacs-owned shared object.
Cargo and rustc therefore check every command, event, format, and lifetime
boundary together. GStreamer initialization remains lazy at the first media
command, and the dedicated missing-plugin message protocol becomes a typed,
session-scoped video failure.

The Linux product targets the GStreamer 1.20 API/ABI shipped by the oldest
supported release environment. On that baseline it negotiates legacy linear
DMA-BUF video caps; on newer runtimes it can additionally negotiate `DMA_DRM`
caps with explicit DRM modifiers. The newer caps are parsed into Neomacs-owned
Rust types, so supporting them does not introduce references to GStreamer
1.24-only symbols or raise the executable's loader requirement.

One product ships. It includes video and declares GStreamer in its
loader/package closure, and `verify_built_product` fails the build if the Linux
executable does not link it. The `.deb` and `.rpm` declare the dependency and
the AppImage bundles the closure, so every Linux artifact carries the same
capability; codec plugin families still come from the host. The separate
GStreamer-free `--minimal` product that once backed the AppImage is retired.

DMA-BUF import keeps decoded pixels on the GPU where the decoder and compositor
topology is compatible. The typed transfer policy distinguishes direct external
surfaces, GPU interop copies, and packed CPU upload; the default renderer policy
permits the CPU path as a correctness fallback instead of claiming every pipeline
is zero-copy.

---

## WebKit Integration

WPE WebKit renders web content offscreen. The render thread receives texture data
from WPE and uploads it to GPU textures, which are then rendered as quads at the
WebKit glyph positions in the Emacs buffer.

Mouse/keyboard events for WebKit views are forwarded from the render thread based
on glyph hit-testing.

---

## Build System

```bash
# Build Rust display engine (produces libneomacs_display.a)
cargo build --release --manifest-path rust/neomacs-display/Cargo.toml

# Build Emacs (links against Rust static library)
./autogen.sh
./configure --with-neomacs --with-native-compilation
make -j$(nproc)
```

Feature flags in Cargo.toml:
- `winit-backend` — wgpu + winit GPU rendering (default)
- `video` — cross-platform video playback (direct GStreamer linkage on Linux)
- `webkit` — WPE WebKit browser embedding

---

## Design Principles

1. **Emacs stays in logical pixels** — all coordinates from Emacs are logical.
   Scale factor is handled entirely in the Rust render thread.

2. **Render thread is autonomous** — once configured, animations run independently.
   Emacs does not need to redisplay for animations to progress.

3. **Snapshot-based transitions** — old frame state is captured as a GPU texture.
   New frame is rendered normally. Both are composited for transition effects.

4. **Content-only effects** — scroll/transition effects apply only to window content
   areas (excluding mode-lines), using scissor rects and UV coordinate mapping.

5. **No Emacs C code changes for animations** — animation state lives entirely in
   Rust. Emacs only sends configuration once via the Lisp DEFUN.

6. **Modular effects** — each scroll effect is an independent render method in
   `renderer.rs`. Adding a new effect means adding one new method and one enum variant.
