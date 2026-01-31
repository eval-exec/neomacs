# Neomacs - Modern Rust Display Engine

## Project Overview

**Neomacs** is a fork of GNU Emacs with a modernized GPU-accelerated display engine written in Rust.

### Project Decisions

| Decision | Choice |
|----------|--------|
| **Project Type** | Fork (maintained separately from GNU Emacs) |
| **Platform** | Linux only (initially) |
| **Display Server** | Wayland-first (via GTK4) |
| **GPU Rendering** | GTK4/GSK (uses Vulkan internally) |
| **Text Rendering** | Pango (via GTK4) |
| **Video Backend** | GStreamer (VA-API hardware decoding) |
| **Browser Embedding** | WPE WebKit (DMA-BUF zero-copy) |
| **Implementation** | Rust with C FFI |
| **Compatibility** | 90% (minor Lisp breakage acceptable) |
| **Backends** | TTY + GTK4 only (remove X11, W32, NS, etc.) |

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Emacs Core (C)                               │
│  Lisp interpreter, buffers, windows, faces, overlays            │
└─────────────────────────────────┬───────────────────────────────┘
                                  │ C FFI
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│              libneomacs_display (Rust)                          │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    C API Layer (ffi.rs)                  │   │
│  └─────────────────────────────────┬───────────────────────┘   │
│                                    │                            │
│  ┌─────────────────────────────────▼───────────────────────┐   │
│  │              Display Engine Core (Rust)                  │   │
│  │  scene.rs, layout.rs, damage.rs, animation.rs           │   │
│  └─────────────────────────────────┬───────────────────────┘   │
│                                    │                            │
│  ┌──────────────┬──────────────────┼──────────────┬────────┐   │
│  │ TTY Backend  │  GTK4 Backend    │ Video        │ WebKit │   │
│  │ (tty.rs)     │  (gtk4-rs/GSK)   │ (gstreamer)  │ (wpe)  │   │
│  └──────────────┴──────────────────┴──────────────┴────────┘   │
│                            │                                    │
│                            ▼                                    │
│           GPU: DMA-BUF → GdkDmabufTexture → GSK → Vulkan       │
└─────────────────────────────────────────────────────────────────┘
```

### GPU Pipeline (Zero-Copy)

```
┌─────────────────────────────────────────────────────────────────┐
│                     GPU Memory (VRAM)                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Video (VA-API)──┐    WebKit (WPE)──┐    Image (large)──┐     │
│         │         │          │        │         │         │     │
│         ▼         │          ▼        │         ▼         │     │
│   ┌─────────┐     │   ┌─────────┐     │   ┌─────────┐     │     │
│   │ DMA-BUF │     │   │ DMA-BUF │     │   │ DMA-BUF │     │     │
│   └────┬────┘     │   └────┬────┘     │   └────┬────┘     │     │
│        │          │        │          │        │          │     │
│        └──────────┴────────┴──────────┴────────┘          │     │
│                           │                                │     │
│                           ▼                                │     │
│                  GdkDmabufTexture                          │     │
│                           │                                │     │
│                           ▼                                │     │
│                    GSK TextureNode                         │     │
│                           │                                │     │
│                           ▼                                │     │
│                  Vulkan/GL Compositor                      │     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Progress Summary

| Phase | Description | Status |
|-------|-------------|--------|
| **Phase 1** | Foundation & Project Setup | ✅ Complete |
| **Phase 2** | TTY Backend | ⏳ Pending |
| **Phase 3** | GTK4 Backend - Basic Rendering | ✅ Complete |
| **Phase 4** | Image Support | ✅ Complete |
| **Phase 5** | Video Support (GStreamer) | ✅ Complete (overlay only) |
| **Phase 6** | WPE WebKit Support | ✅ Complete (overlay only) |
| **Phase 7** | GPU Zero-Copy Pipeline | 🔧 **In Progress** (VA-API decode done) |
| **Phase 8** | Animation System | ⏳ Pending |
| **Phase 9** | Inline Display (`xdisp.c`) | 🔥 **HIGH PRIORITY** |
| **Phase 10** | Emacs Build Integration | 🔧 Partial |
| **Phase 11** | Remove Legacy Backends | ⏳ Pending |
| **Phase 12** | Testing & Documentation | ⏳ Pending |
| **Phase 13** | Polish & Optimization | ⏳ Pending |

### Inline Display Status

| Type | Overlay/Floating | Inline (display property) |
|------|------------------|---------------------------|
| **Image** | ✅ Works | ✅ Works (`(image :file ...)`) |
| **Video** | ✅ Works | ❌ **NEEDS IMPLEMENTATION** |
| **WebKit** | ✅ Works | ❌ **NEEDS IMPLEMENTATION** |

---

## Implementation Tasks

---

## Phase 1: Foundation & Project Setup ✅

### 1.1 Project Structure ✅
- [x] Create `rust/neomacs-display/` crate directory
- [x] Set up `Cargo.toml` with dependencies (gtk4, gsk4, pango, gstreamer)
- [x] Configure `cbindgen.toml` for C header generation
- [x] Create `build.rs` for build-time tasks
- [x] Create `shell.nix` for NixOS development
- [x] Create `Makefile` for build automation
- [ ] Set up CI/CD for Rust crate (cargo test, clippy, fmt)
- [ ] Add LICENSE (GPL3 to match Emacs)
- [ ] Create README.md for the Rust crate

### 1.2 Core Types ✅
- [x] Define `GlyphType` enum (Char, Image, Video, Wpe, etc.)
- [x] Define `Glyph` struct with `#[repr(C)]` for FFI
- [x] Define `Face` struct for text styling
- [x] Define `Color` type (RGBA)
- [x] Define `Rect`, `Point`, `Size` geometry types
- [x] Define `Transform` for 2D transformations
- [x] Write unit tests for core types

### 1.3 Scene Graph ✅
- [x] Define `NodeKind` enum (Container, TextRun, Image, Video, Wpe, ColorRect)
- [x] Define `Node` struct with bounds, opacity, clip, transform
- [x] Implement `Scene` struct as root container
- [x] Implement `WindowScene` struct for window representation
- [x] Implement scene graph builder methods
- [x] Write unit tests for scene graph

### 1.4 C FFI Layer ✅
- [x] Create `ffi.rs` module
- [x] Implement `neomacs_display_init()` / `neomacs_display_shutdown()`
- [x] Implement `neomacs_display_begin_frame()` / `neomacs_display_end_frame()`
- [x] Implement `neomacs_display_add_window()` / `neomacs_display_set_cursor()`
- [x] Implement animation FFI functions
- [ ] Generate C header with cbindgen
- [ ] Test FFI with simple C test program

### 1.5 Animation System ✅
- [x] Implement `Animation` struct with easing functions
- [x] Implement `AnimationManager` for cursor blink, smooth scroll
- [x] Write unit tests for animation

### 1.6 Backend Trait ✅
- [x] Define `DisplayBackend` trait
- [x] Create GTK4 backend skeleton
- [x] Create TTY backend skeleton

---

## Phase 2: TTY Backend ⏳

### 2.1 Terminal Output
- [ ] Create `backend/tty.rs` module
- [ ] Implement `DisplayBackend` trait for TTY
- [ ] Implement terminal capability detection (terminfo)
- [ ] Implement ANSI escape sequence generation
- [ ] Implement cursor positioning
- [ ] Implement color output (16, 256, truecolor)

### 2.2 Character Cell Rendering
- [ ] Convert scene graph to character cell grid
- [ ] Implement text rendering (UTF-8)
- [ ] Implement face/attribute rendering (bold, underline, etc.)
- [ ] Implement box-drawing characters for UI elements
- [ ] Handle wide characters (CJK)

### 2.3 TTY Optimizations
- [ ] Implement dirty region tracking
- [ ] Implement differential updates (only changed cells)
- [ ] Implement scroll optimization (terminal scroll regions)
- [ ] Benchmark TTY rendering performance

---

## Phase 3: GTK4 Backend - Basic Rendering ✅

### 3.1 GTK4 Application Setup ✅
- [x] Create `backend/gtk4/mod.rs` module
- [x] Initialize GTK4 with `gtk4::init()`
- [x] Create `GtkDrawingArea` as rendering canvas
- [x] Set up `set_draw_func` for frame rendering
- [x] Handle window resize events
- [ ] Create full `GtkApplication` integration
- [ ] Handle window close/destroy events
- [ ] Implement graceful shutdown

### 3.2 Cairo Drawing API ✅
- [x] Implement `DisplayBackend` trait for GTK4
- [x] Implement Cairo rectangle rendering (`rectangle()`, `fill()`)
- [x] Implement color rendering (`set_source_rgba()`)
- [x] Implement clipping (`clip()`)
- [x] Implement transforms (`save()`, `restore()`, `translate()`)
- [x] Create renderer module (`backend/gtk4/renderer.rs`)
- [ ] Port to GtkSnapshot API (for newer GTK4)

### 3.3 Text Rendering with Pango ✅
- [x] Get `PangoContext` from DrawingArea
- [x] Create `PangoLayout` for text runs
- [x] Implement `set_font_description()` for fonts
- [x] Implement `set_text()` for content
- [x] Implement text rendering with `pangocairo::show_layout()`
- [ ] Implement text measurement (`get_pixel_size()`)
- [ ] Handle foreground colors per-run
- [ ] Implement underline, strikethrough via Pango attributes
- [ ] Test CJK characters rendering
- [ ] Test emoji rendering

### 3.4 Glyph Atlas (GPU Text Caching)
- [ ] Create `core/atlas.rs` module
- [ ] Research if custom atlas needed (Pango/GSK may handle this)
- [ ] If needed: implement texture atlas allocation (bin packing)
- [ ] If needed: implement glyph rasterization to atlas
- [ ] If needed: implement glyph cache lookup (font + codepoint → coords)
- [ ] If needed: implement atlas texture upload to GPU
- [ ] If needed: implement atlas eviction/regeneration
- [ ] Benchmark text rendering performance vs current Emacs
- [ ] Profile GPU usage during text rendering

### 3.5 Input Handling (GTK4 Event Controllers)
- [ ] Create `backend/gtk4/input.rs` module
- [ ] Create `EventControllerKey` for keyboard input
- [ ] Implement `connect_key_pressed` handler
- [ ] Implement `connect_key_released` handler
- [ ] Convert `gdk4::Key` to Emacs key events
- [ ] Handle modifier keys (Ctrl, Alt, Shift, Super)
- [ ] Create `GestureClick` for mouse clicks
- [ ] Implement `connect_pressed` / `connect_released`
- [ ] Handle single, double, triple clicks
- [ ] Create `EventControllerMotion` for mouse movement
- [ ] Implement `connect_motion` for hover/tracking
- [ ] Create `EventControllerScroll` for scroll wheel
- [ ] Implement `connect_scroll` for wheel events
- [ ] Handle smooth scrolling (touchpad)
- [ ] Forward input events to Emacs event queue
- [ ] Test keyboard input with special keys (F1-F12, arrows, etc.)
- [ ] Test mouse input in different window regions

### 3.6 Frame Clock and Animation Loop
- [ ] Get `GdkFrameClock` from drawing area
- [ ] Implement `connect_update` callback
- [ ] Calculate delta time from `frame_time()`
- [ ] Call `begin_updating()` to start receiving updates
- [ ] Update animations in frame clock callback
- [ ] Call `queue_draw()` when content changes
- [ ] Implement frame rate limiting (if needed)
- [ ] Verify vsync synchronization
- [ ] Test smooth 60fps updates

### 3.7 Cursor Rendering ✅
- [x] Implement box cursor (filled rectangle)
- [x] Implement bar cursor (thin line)
- [x] Implement underline cursor
- [x] Implement hollow cursor (unfocused)
- [x] Implement cursor rendering in glyph rows
- [x] Handle cursor color from face
- [ ] Implement cursor blinking via frame clock
- [ ] Test cursor in different modes

---

## Phase 4: Image Support ✅

### 4.1 Image Loading ✅
- [x] Create `backend/gtk4/image.rs` module
- [x] Implement ImageCache for efficient storage
- [x] Implement image loading from file path (PNG, JPEG, GIF, etc.)
- [x] Implement image loading from raw bytes
- [x] Implement Pixbuf to Cairo surface conversion (RGBA→BGRA)
- [ ] Handle animated GIFs (see 4.4)

### 4.2 Image Rendering ✅
- [x] Integrate ImageCache into Gtk4Renderer
- [x] Implement render_image() method
- [x] Implement image scaling to glyph dimensions
- [x] Implement image placeholder (when not loaded)
- [ ] Implement image clipping (partial display)
- [ ] Implement image transforms (rotation, flip)
- [ ] Support SVG rendering at any scale

### 4.3 Image FFI (Partial)
- [x] Implement `neomacs_display_add_image_glyph()` FFI
- [x] Implement `neomacs_display_load_image()` FFI (stub)
- [ ] Implement `emacs_display_image_size()` FFI
- [ ] Implement `emacs_display_free_image()` FFI

### 4.4 Animated Image Support (GIF, APNG, WebP)
- [x] Define AnimationFrame struct with frame + delay
- [x] Implement GIF frame extraction via PixbufAnimation
- [x] Implement advance_animation() for frame cycling
- [ ] Integrate with GTK4 frame clock (single animation loop)
- [ ] Implement per-frame delay handling
- [ ] Implement loop modes (forever, count, once)
- [ ] Benchmark: 10+ animated GIFs simultaneously

---

## Phase 5: Video Support (GStreamer) ✅

### 5.1 GStreamer Setup ✅
- [x] Create `backend/gtk4/video.rs` module
- [x] Initialize GStreamer
- [x] Create video pipeline (playbin + appsink)
- [x] Use videoconvert + videoscale for format conversion
- [x] Capture frames as BGRA raw bytes
- [x] Handle pipeline state changes
- [x] Handle end-of-stream (via bus messages)
- [x] Handle errors (via bus messages)

### 5.2 Video Playback Control ✅
- [x] Implement play/pause/stop
- [x] Implement seek (nanosecond precision)
- [x] Implement volume control
- [x] Get current position and duration
- [x] Implement loop mode flag
- [ ] Implement playback speed control

### 5.3 Video Rendering ✅
- [x] Create VideoCache for multiple video players
- [x] Convert raw BGRA to Cairo ImageSurface (on main thread)
- [x] Convert raw BGRA to GdkTexture (for GPU rendering)
- [x] Integrate VideoCache into GskRenderer
- [x] Implement render_video() method
- [x] Implement video placeholder rendering
- [x] Handle video glyphs in build_row_nodes()
- [ ] Implement aspect ratio preservation

### 5.4 Video FFI ✅
- [x] Implement `neomacs_display_add_video_glyph()` FFI
- [x] Implement `neomacs_display_load_video()` FFI
- [x] Implement `neomacs_display_video_play()` FFI
- [x] Implement `neomacs_display_video_pause()` FFI
- [x] Implement `neomacs_display_video_stop()` FFI
- [ ] Implement `emacs_display_video_seek()` FFI
- [ ] Implement `emacs_display_video_set_volume()` FFI
- [ ] Implement `emacs_display_video_get_state()` FFI

### 5.5 Video Lisp API ✅
- [x] Define `neomacs-video-load` Lisp function
- [x] Define `neomacs-video-play`, `neomacs-video-pause`, `neomacs-video-stop` functions
- [x] Define `neomacs-video-overlay`, `neomacs-video-overlay-clear` functions
- [x] Create `lisp/neomacs-video.el` helper package
- [ ] Define `neomacs-video-seek`, `neomacs-video-set-volume` functions
- [ ] Define `neomacs-video-duration`, `neomacs-video-current-time` functions

### 5.6 Video Testing ✅
- [x] Test 720p (1280x720) H.264 video - PASSED
- [x] Test 1080p (1920x1080) Theora video - PASSED
- [x] Verify frame decoding (BGRA pixels captured)
- [x] Video overlay API implemented and tested
- [ ] Test 4K video playback
- [ ] Benchmark video CPU/GPU usage
- [ ] Visual verification of video overlay rendering

---

## Phase 6: WPE WebKit Support ✅

### 6.1 WPE Backend Setup ✅
- [x] Create `backend/wpe/` module (backend.rs, view.rs, dmabuf.rs, sys.rs)
- [x] Generate WPE Rust bindings via bindgen (libwpe, wpebackend-fdo, wpe-webkit)
- [x] Initialize WPE WebKit context with EGL display
- [x] Create WPE view backend for offscreen rendering
- [x] Set up DMA-BUF export (EGLImage → GdkDmabufTexture)

### 6.2 WebKit View Management ✅
- [x] Create WebKit web view (WpeWebView wrapper)
- [x] Load URI (`webkit_web_view_load_uri`)
- [x] Handle navigation (go_back, go_forward, reload, stop)
- [x] Get page state (title, url, progress, loading)
- [x] Handle JavaScript execution (`webkit_web_view_evaluate_javascript`)

### 6.3 WPE Rendering ✅
- [x] Receive rendered buffers from WPE (export_fdo_egl_image callback)
- [x] Convert EGLImage to GdkDmabufTexture via DmaBufExporter
- [x] Integrate WebKitCache with GSK renderer
- [x] Render WebKit views as GskTextureNode
- [x] Handle continuous updates (frame_available flag)

### 6.4 Input Forwarding ✅
- [x] Forward keyboard events (`wpe_view_backend_dispatch_keyboard_event`)
- [x] Forward mouse/pointer events (`wpe_view_backend_dispatch_pointer_event`)
- [x] Forward scroll/axis events (`wpe_view_backend_dispatch_axis_event`)
- [x] Convenience methods: click(), scroll()

### 6.5 WPE FFI ✅
- [x] Implement `neomacs_display_webkit_init()` FFI
- [x] Implement `neomacs_display_webkit_create()` FFI
- [x] Implement `neomacs_display_webkit_destroy()` FFI
- [x] Implement `neomacs_display_webkit_load_uri()` FFI
- [x] Implement `neomacs_display_webkit_go_back()` FFI
- [x] Implement `neomacs_display_webkit_go_forward()` FFI
- [x] Implement `neomacs_display_webkit_reload()` FFI
- [x] Implement `neomacs_display_webkit_execute_js()` FFI
- [x] Implement `neomacs_display_webkit_send_key()` FFI
- [x] Implement `neomacs_display_webkit_send_pointer()` FFI
- [x] Implement `neomacs_display_webkit_send_scroll()` FFI
- [x] Implement `neomacs_display_webkit_click()` FFI
- [x] Implement `neomacs_display_webkit_get_title()` FFI
- [x] Implement `neomacs_display_webkit_get_url()` FFI
- [x] Implement `neomacs_display_webkit_get_progress()` FFI
- [x] Implement `neomacs_display_webkit_is_loading()` FFI
- [x] Implement `neomacs_display_set_floating_webkit()` FFI
- [x] Implement `neomacs_display_hide_floating_webkit()` FFI

### 6.6 WPE Lisp API ✅
- [x] Define `neomacs-webkit-init` Lisp function
- [x] Define `neomacs-webkit-create` Lisp function
- [x] Define `neomacs-webkit-destroy` Lisp function
- [x] Define `neomacs-webkit-load-uri` Lisp function
- [x] Define `neomacs-webkit-go-back`, `neomacs-webkit-go-forward` functions
- [x] Define `neomacs-webkit-reload` Lisp function
- [x] Define `neomacs-webkit-execute-js` Lisp function
- [x] Define `neomacs-webkit-floating`, `neomacs-webkit-floating-clear` functions
- [x] Define `neomacs-webkit-send-key`, `neomacs-webkit-send-pointer` functions
- [x] Define `neomacs-webkit-send-scroll`, `neomacs-webkit-click` functions
- [x] Define `neomacs-webkit-get-title`, `neomacs-webkit-get-url` functions
- [x] Define `neomacs-webkit-get-progress`, `neomacs-webkit-loading-p` functions
- [x] Create `lisp/neomacs-webkit.el` helper package with:
  - `neomacs-webkit-browse` interactive command
  - `neomacs-webkit-mode` major mode with keybindings
  - Mode line with title and loading progress
  - Mouse click and scroll handlers

---

## Phase 7: GPU Zero-Copy Pipeline ⏳

**Goal**: Eliminate CPU→GPU memory copies for video and large images.

### Current State (2024-01-31)
| Type | Decode | Texture | Copy? | Status |
|------|--------|---------|-------|--------|
| Image | CPU (GdkPixbuf) | MemoryTexture | ⚠️ CPU→GPU copy | Working |
| Video | **GPU (VA-API)** | **GdkPaintable** | ✅ Zero-copy | **gtk4paintablesink** |
| WebKit | GPU (WPE) | DmabufTexture | ✅ Zero-copy | Working |

### Target State (Full Zero-Copy)
| Type | Decode | Texture | Copy? |
|------|--------|---------|-------|
| Image (small) | CPU | MemoryTexture | CPU→GPU (fast, small data) |
| Image (large) | CPU | DmabufTexture | ✅ Zero-copy upload |
| Video | GPU (VA-API) | **DmabufTexture** | ✅ Zero-copy |
| WebKit | GPU (WPE) | DmabufTexture | ✅ Zero-copy |

### 7.1 Video Hardware Decoding ✅ COMPLETE
- [x] Enable VA-API hardware decoding in GStreamer pipeline
- [x] Use `playbin` (auto-selects `vah264dec`, `vah265dec`, etc.)
- [x] Fallback chain: VA-API → software decode
- [x] Tested with 1920x1080 H.264 video
- [x] `GpuVideoPlayer` struct with VA-API support
- [x] `VideoCache` now uses `GpuVideoPlayer`

### 7.2 Video DMA-BUF Export ✅ COMPLETE
**Solution**: Use `gtk4paintablesink` from gst-plugins-rs instead of appsink

`gtk4paintablesink` handles all DMA-BUF/GL/VideoMeta negotiation internally and provides
a `GdkPaintable` that can be snapshotted directly into our render tree.

**Capabilities**:
- `video/x-raw(memory:DMABuf)` - DMA-BUF zero-copy ✅
- `video/x-raw(memory:GLMemory)` - GL texture ✅
- `video/x-raw(memory:SystemMemory)` - Fallback ✅

- [x] Research gtk4paintablesink as solution
- [x] Add gst-plugins-rs/gtk4 plugin to nix environment (shell.nix)
- [x] Replace appsink with gtk4paintablesink in GpuVideoPlayer
- [x] Get GdkPaintable from sink's `paintable` property
- [x] Remove old VideoPlayer struct and gst_app/gst_allocators deps
- [ ] Snapshot paintable into render tree (needs VideoCache update)
- [ ] Test DMA-BUF zero-copy path works (check logs)
- [ ] Test 4K@60fps performance (target: <5% CPU usage)

### 7.3 Image DMA-BUF Upload (Medium Priority)
- [ ] Use `GdkDmabufTextureBuilder` for large images (>1MP)
- [ ] Create DMA-BUF via `gbm_bo_create()` or `drm` allocator
- [ ] Map buffer, decode image, unmap
- [ ] Threshold: use DMA-BUF only for images > 1 megapixel
- [ ] Keep MemoryTexture for small images (faster for small data)

---

## Phase 8: Animation System ⏳

### 8.1 Animation Core
- [ ] Create `core/animation.rs` module
- [ ] Define animation types (scroll, fade, transform)
- [ ] Implement easing functions (linear, ease-in-out, cubic, etc.)
- [ ] Implement animation timeline/scheduler
- [ ] Handle animation completion callbacks

### 8.2 Smooth Scrolling
- [ ] Implement smooth scroll animation
- [ ] Integrate with GTK4 frame clock
- [ ] Handle scroll velocity/momentum
- [ ] Handle scroll interruption

### 8.3 Cursor Animation
- [ ] Implement cursor blink animation
- [ ] Implement cursor smooth movement (optional)

### 8.4 Transition Effects
- [ ] Implement fade in/out for windows
- [ ] Implement buffer switch transitions (optional)

---

## Phase 9: Inline Display Support ⏳ (HIGH PRIORITY)

**Goal**: Support `(video :id N)` and `(webkit :id N)` display properties in buffer text, just like `(image ...)` works today.

### Current Inline Support Status

| Type | Display Property | xdisp.c | Status |
|------|------------------|---------|--------|
| **Image** | `(image :file "x.png")` | ✅ `Qimage`, `produce_image_glyph()` | ✅ **WORKS** |
| **Video** | `(video :id N)` | ❌ Missing | ❌ **NOT IMPLEMENTED** |
| **WebKit** | `(webkit :id N)` | ❌ Missing | ❌ **NOT IMPLEMENTED** |

### Usage Example (Target)
```elisp
;; Image (already works)
(put-text-property 1 2 'display '(image :file "/path/to/image.png"))

;; Video (need to implement)
(let ((id (neomacs-video-load "/path/to/video.mp4")))
  (put-text-property 1 2 'display `(video :id ,id :width 640 :height 480)))

;; WebKit (need to implement)
(let ((id (neomacs-webkit-create 800 600)))
  (neomacs-webkit-load-uri id "https://example.com")
  (put-text-property 1 2 'display `(webkit :id ,id)))
```

### 9.1 Display Property Infrastructure
- [x] Add WEBKIT_GLYPH to `enum glyph_type` in dispextern.h
- [x] Add IT_WEBKIT to `enum it_method` in dispextern.h
- [x] Add webkit_id to glyph union and iterator struct
- [x] Handle WEBKIT_GLYPH in neomacsterm.c draw functions
- [ ] Add VIDEO_GLYPH to `enum glyph_type` in dispextern.h
- [ ] Add IT_VIDEO to `enum it_method` in dispextern.h
- [ ] Add video_id to glyph union and iterator struct
- [ ] Handle VIDEO_GLYPH in neomacsterm.c draw functions

### 9.2 xdisp.c Symbols and Parsing
- [ ] Add `Qvideo` symbol via DEFSYM
- [ ] Add `Qwebkit` symbol via DEFSYM
- [ ] Parse `(video :id N :width W :height H)` in `handle_single_display_spec()`
- [ ] Parse `(webkit :id N)` in `handle_single_display_spec()`
- [ ] Validate video/webkit IDs exist in cache
- [ ] Extract width/height properties

### 9.3 Glyph Production Functions
- [ ] Implement `produce_video_glyph()` (similar to `produce_image_glyph()`)
- [ ] Implement `produce_webkit_glyph()`
- [ ] Set glyph dimensions from video/webkit size
- [ ] Handle ascent/descent calculation
- [ ] Handle slice (partial display)

### 9.4 Iterator Support
- [ ] Add `GET_FROM_VIDEO` to `enum it_method`
- [ ] Add `GET_FROM_WEBKIT` to `enum it_method`
- [ ] Implement `next_element_from_video()`
- [ ] Implement `next_element_from_webkit()`
- [ ] Handle iterator state transitions
- [ ] Handle `set_iterator_to_next()` for video/webkit

### 9.5 Layout Integration
- [ ] Handle video/webkit glyph dimensions in `x_produce_glyphs()`
- [ ] Handle line height calculation with video/webkit
- [ ] Handle cursor movement over video/webkit glyphs
- [ ] Handle mouse click on video/webkit glyphs

---

## Phase 10: Emacs Build Integration 🔧

### 10.1 Build System Integration
- [ ] Add Rust build to Emacs `configure.ac`
- [ ] Add cargo build to `Makefile.in`
- [ ] Link `libneomacs_display.so` with Emacs
- [ ] Add `--with-neomacs` configure option
- [ ] Handle cross-compilation

### 10.2 Modify dispnew.c
- [ ] Include generated C header
- [ ] Initialize Rust display engine on startup
- [ ] Replace `update_frame` to call Rust FFI
- [ ] Convert `glyph_matrix` to Rust `Glyph` array
- [ ] Handle backend selection (TTY vs GTK4)

### 10.3 Modify keyboard.c
- [ ] Forward input to video/wpe when cursor on those glyphs
- [ ] Handle video playback shortcuts
- [ ] Handle WPE input mode

### 10.4 New Lisp Primitives (Already Done)
- [x] Add video primitives to `src/neomacsterm.c`
- [x] Add WebKit primitives to `src/neomacsterm.c`
- [x] Register primitives in `syms_of_neomacsterm`

---

## Phase 11: Remove Legacy Backends ⏳

### 11.1 Remove X11 Backend
- [ ] Remove `xterm.c`, `xterm.h`
- [ ] Remove `xfns.c`
- [ ] Remove X11-specific code from `gtkutil.c`
- [ ] Update configure.ac
- [ ] Update Makefile.in

### 11.2 Remove Windows Backend
- [ ] Remove `w32term.c`, `w32term.h`
- [ ] Remove `w32fns.c`
- [ ] Remove `w32*.c` files
- [ ] Update configure.ac

### 11.3 Remove macOS Backend
- [ ] Remove `nsterm.m`, `nsterm.h`
- [ ] Remove `nsfns.m`
- [ ] Remove `ns*.m` files
- [ ] Update configure.ac

### 11.4 Remove Other Backends
- [ ] Remove Haiku backend (`haikuterm.c`, etc.)
- [ ] Remove Android backend (`androidterm.c`, etc.)
- [ ] Remove MS-DOS backend (`msdos.c`)
- [ ] Remove old X menu code (`oldXMenu/`)

### 11.5 Cleanup
- [ ] Remove `output_method` enum entries (keep initial, termcap, neomacs)
- [ ] Remove unused conditionals (`HAVE_X_WINDOWS`, `HAVE_NS`, etc.)
- [ ] Remove unused header includes
- [ ] Update documentation

---

## Phase 12: Testing & Documentation ⏳

### 12.1 Rust Unit Tests
- [ ] Test core types serialization
- [ ] Test scene graph construction
- [ ] Test glyph atlas packing
- [ ] Test animation interpolation
- [ ] Test TTY escape sequence generation

### 12.2 Integration Tests
- [ ] Test C FFI from test harness
- [ ] Test GTK4 rendering with screenshot comparison
- [ ] Test video playback
- [ ] Test WPE WebKit loading

### 12.3 Emacs Integration Tests
- [ ] Test basic text display
- [ ] Test face rendering (colors, styles)
- [ ] Test image display
- [ ] Test video insertion and playback
- [ ] Test WPE view insertion and navigation
- [ ] Test smooth scrolling

### 12.4 Performance Benchmarks
- [ ] Benchmark text rendering throughput
- [ ] Benchmark large buffer scrolling
- [ ] Benchmark image loading
- [ ] Benchmark video playback CPU/GPU usage
- [ ] Compare with old display engine

### 12.5 Documentation
- [ ] Write Rust API documentation (rustdoc)
- [ ] Write C FFI documentation
- [ ] Write Lisp API documentation (docstrings)
- [ ] Update Emacs manual for new features
- [ ] Write developer guide for extending display engine

---

## Phase 13: Polish & Optimization ⏳

### 13.1 Performance Optimization
- [ ] Profile and optimize hot paths
- [ ] Reduce memory allocations
- [ ] Optimize scene graph updates (incremental)
- [ ] Optimize glyph atlas usage
- [ ] Enable GPU shader optimizations

### 13.2 Error Handling
- [ ] Graceful fallback on GPU failure
- [ ] Handle video codec errors
- [ ] Handle WPE crash recovery
- [ ] Log errors to Emacs `*Messages*`

### 13.3 Accessibility
- [ ] Ensure screen reader compatibility
- [ ] Support high contrast themes
- [ ] Support reduced motion preferences
- [ ] Support system font scaling

### 13.4 Platform Testing
- [ ] Test on Linux (X11 via XWayland)
- [ ] Test on Linux (Wayland native)
- [ ] Test on macOS (via GTK4)
- [ ] Test on Windows (via GTK4/MSYS2)

---

## Milestones

| Milestone | Phases | Status |
|-----------|--------|--------|
| **M1: Basic Rendering** | 1, 3 | ✅ GTK4 text rendering works |
| **M2: Feature Parity** | 4 | ✅ Images work like current Emacs |
| **M3: Video Support** | 5 | ✅ Video playback in buffers |
| **M4: WebKit Support** | 6 | ✅ WPE WebKit embedding works |
| **M5: GPU Zero-Copy** | 7 | ⏳ Video/Image DMA-BUF pipeline |
| **M6: Smooth UX** | 8 | ⏳ Animations and smooth scrolling |
| **M7: Inline Display** | 9 | ⏳ `(video :id N)` display property |
| **M8: Full Integration** | 10 | 🔧 Emacs builds with new engine |
| **M9: Cleanup** | 11 | ⏳ Legacy backends removed |
| **M10: Release Ready** | 12-13 | ⏳ Tested, documented, optimized |

---

## Dependencies

### Rust Crates
- `gtk4` (0.9+) - GTK4 bindings
- `gdk4` (0.9+) - GDK4 bindings  
- `gsk4` (0.9+) - GSK bindings
- `pango` (0.20+) - Text rendering
- `cairo-rs` (0.20+) - 2D graphics
- `gstreamer` (0.23+) - Video playback
- `gstreamer-video` (0.23+) - Video utilities

### System Libraries
- GTK4 (4.10+)
- GStreamer (1.20+)
- WPE WebKit (2.38+)
- libwpe, wpebackend-fdo
- Pango (1.50+)
- Cairo (1.16+)
- VA-API (for hardware video decoding)
- Mesa (for DMA-BUF)

---

## Open Questions

1. ~~**WPE Rust bindings**: Do mature bindings exist?~~ → Solved: bindgen for C APIs
2. **GTK4 minimum version**: What's the minimum GTK4 version to support?
3. **Fallback rendering**: Should we support software rendering fallback?
4. **Thread model**: How to handle Rust async with Emacs event loop?
5. **Memory sharing**: How to efficiently share buffer text with Rust?

---

## References

- [gtk4-rs Documentation](https://gtk-rs.org/gtk4-rs/stable/latest/docs/gtk4/)
- [gstreamer-rs Documentation](https://gstreamer.freedesktop.org/documentation/rust/)
- [WPE WebKit](https://wpewebkit.org/)
- [VA-API](https://github.com/intel/libva)
- [DMA-BUF](https://docs.kernel.org/driver-api/dma-buf.html)
- [GdkDmabufTexture](https://docs.gtk.org/gdk4/class.DmabufTexture.html)
- [Emacs Internals](https://www.gnu.org/software/emacs/manual/html_node/elisp/Display.html)
