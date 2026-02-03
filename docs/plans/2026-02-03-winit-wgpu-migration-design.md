# Winit + wgpu Migration Design

Replace GTK4/GSK with a Rust-native graphics stack (winit + wgpu) while preserving WPE WebKit support and adding animation capabilities.

## Goals

- Rust-native window management and rendering
- Zero-copy DMA-BUF on Linux, shared memory fallback elsewhere
- Keep WPE WebKit and GStreamer video support
- Add animation system (including page-flip buffer transitions)
- Cross-platform: Linux primary, macOS/Windows supported

## Non-Goals

- Replacing WPE WebKit with a Rust alternative (none exists)
- Pure Rust video decoding (use GStreamer)

---

## Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                         Emacs (C)                              │
│                    (unchanged, same FFI)                       │
└────────────────────────────┬───────────────────────────────────┘
                             │ FFI
┌────────────────────────────▼───────────────────────────────────┐
│                    neomacs-display (Rust)                      │
├────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │   Scene      │  │ TextEngine   │  │  ExternalBuffer      │  │
│  │  (unchanged) │  │(cosmic-text) │  │  (DMA-BUF/SharedMem) │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
├────────────────────────────────────────────────────────────────┤
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                    WgpuRenderer                          │  │
│  │  - Replaces GskRenderer                                  │  │
│  │  - Renders Scene to wgpu surface                         │  │
│  └──────────────────────────────────────────────────────────┘  │
├────────────────────────────────────────────────────────────────┤
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                    WinitBackend                          │  │
│  │  - Window creation, input events, event loop             │  │
│  │  - Replaces GTK4 window/input handling                   │  │
│  └──────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────┘
```

### Data Flow

Emacs (C) pushes draw commands via FFI → neomacs-display builds Scene → WgpuRenderer renders to screen.

This push model remains unchanged from the current architecture.

---

## WgpuRenderer

```rust
pub struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,

    // Pipelines
    rect_pipeline: wgpu::RenderPipeline,      // solid color rectangles
    texture_pipeline: wgpu::RenderPipeline,   // images, video, webkit
    glyph_pipeline: wgpu::RenderPipeline,     // text glyphs

    // Caches
    glyph_atlas: GlyphAtlas,                  // cached glyph textures
    texture_cache: TextureCache,              // imported external textures
}

impl WgpuRenderer {
    pub fn render(&mut self, scene: &Scene) {
        // 1. Begin render pass
        // 2. Draw background (rect_pipeline)
        // 3. For each window in scene:
        //    - Draw window background
        //    - Draw glyph rows (glyph_pipeline)
        //    - Draw images/videos/webkit (texture_pipeline)
        //    - Draw cursor
        // 4. Draw borders
        // 5. Draw floating elements (videos, images, webkit on top)
        // 6. Submit
    }
}
```

### Shaders

| Shader | Purpose |
|--------|---------|
| `rect.wgsl` | Solid color rectangles (backgrounds, cursor, borders) |
| `texture.wgsl` | Textured quads (images, video frames, webkit) |
| `glyph.wgsl` | Alpha-masked glyph rendering with color |

### Rendering Strategy

- **Batching**: Group draw calls by pipeline type to minimize state changes
- **Glyph atlas**: Cache rasterized glyphs as textures (same as current)
- **Instancing**: Draw many rectangles/glyphs in one draw call using instance buffers

---

## External Buffer Import

Zero-copy texture import for WebKit and video frames.

### Abstraction

```rust
pub trait ExternalBuffer {
    fn to_wgpu_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> wgpu::Texture;
}
```

### Linux (DMA-BUF)

```rust
pub struct DmaBufBuffer {
    fd: RawFd,
    width: u32,
    height: u32,
    format: DrmFormat,
    modifier: u64,
}

impl ExternalBuffer for DmaBufBuffer {
    fn to_wgpu_texture(&self, device: &Device, queue: &Queue) -> Texture {
        // Use wgpu::hal to import via Vulkan's VK_EXT_external_memory_dma_buf
        // Zero-copy: GPU accesses the same memory
    }
}
```

### Fallback (Shared Memory)

```rust
pub struct SharedMemoryBuffer {
    data: Vec<u8>,
    width: u32,
    height: u32,
    stride: u32,
}

impl ExternalBuffer for SharedMemoryBuffer {
    fn to_wgpu_texture(&self, device: &Device, queue: &Queue) -> Texture {
        // Create texture and upload pixels via queue.write_texture()
        // Involves a copy, but works everywhere
    }
}
```

### Platform Selection

```rust
#[cfg(target_os = "linux")]
type PlatformBuffer = DmaBufBuffer;

#[cfg(not(target_os = "linux"))]
type PlatformBuffer = SharedMemoryBuffer;
```

---

## WinitBackend

```rust
pub struct WinitBackend {
    event_loop: EventLoop<UserEvent>,
    window: Window,

    // Input state
    modifiers: ModifiersState,
    ime_enabled: bool,

    // Callbacks to Emacs
    key_callback: Box<dyn Fn(KeyEvent)>,
    mouse_callback: Box<dyn Fn(MouseEvent)>,
    resize_callback: Box<dyn Fn(u32, u32)>,
}

pub enum UserEvent {
    Redraw,              // Emacs requests redraw
    WebKitFrame(u32),    // WebKit view has new frame
    VideoFrame(u32),     // Video has new frame
}
```

### Platform Features

| Feature | Solution |
|---------|----------|
| IME (CJK input) | winit `Ime` events, forward to Emacs |
| Clipboard | `arboard` crate (pure Rust, cross-platform) |
| Drag & drop | winit `DroppedFile` events |
| HiDPI | winit `scale_factor()`, pass to renderer |

---

## WebKit & Video Integration

WPE WebKit and GStreamer export frames as `ExternalBuffer`, imported by wgpu.

### WebKit

```rust
pub struct WebKitView {
    backend: wpe_view_backend,
    current_buffer: Option<PlatformBuffer>,
    texture: Option<wgpu::Texture>,
}

impl WebKitView {
    fn on_frame_exported(&mut self, dmabuf: DmaBufInfo) {
        self.current_buffer = Some(DmaBufBuffer::from(dmabuf));
        event_loop_proxy.send_event(UserEvent::WebKitFrame(self.id));
    }

    pub fn texture(&mut self, device: &Device, queue: &Queue) -> Option<&wgpu::Texture> {
        if let Some(buffer) = &self.current_buffer {
            self.texture = Some(buffer.to_wgpu_texture(device, queue));
        }
        self.texture.as_ref()
    }
}
```

### Video (GStreamer)

Same pattern: appsink receives frames as DMA-BUF or raw memory, converts to `ExternalBuffer`.

### Frame Synchronization

1. WebKit/GStreamer exports frame → stores in buffer
2. Sends `UserEvent` to winit event loop
3. Event loop triggers `RedrawRequested`
4. `WgpuRenderer` imports buffer as texture, renders scene

---

## Animation System

```rust
pub struct AnimationEngine {
    active: Vec<Animation>,
    last_frame: Instant,
}

pub struct Animation {
    id: u64,
    target: AnimationTarget,
    property: AnimatedProperty,
    from: f32,
    to: f32,
    duration: Duration,
    easing: Easing,
    started: Instant,
}

pub enum AnimationTarget {
    Window(u32),
    Glyph { window: u32, row: u32, col: u32 },
    WebKit(u32),
    Cursor,
}

pub enum AnimatedProperty {
    X, Y,
    Width, Height,
    Opacity,
    Scale,
    RotationY,    // for 3D transitions
    RotationX,
    TranslateZ,
}

pub enum Easing {
    Linear,
    EaseInOut,
    EaseOutBounce,
}
```

### Buffer Transitions (Page Flip)

```rust
pub struct BufferTransition {
    from_buffer: wgpu::Texture,   // snapshot of old buffer
    to_buffer: wgpu::Texture,     // snapshot of new buffer
    animation: TransitionType,
    progress: f32,                // 0.0 → 1.0
}

pub enum TransitionType {
    PageFlipLeft,
    PageFlipRight,
    Fade,
    SlideLeft,
    SlideRight,
}
```

Page flip rendering:
- Old page rotates from 0° to -90° (visible first half)
- New page rotates from 90° to 0° (visible second half)

### Example Animations

| Animation | Use case |
|-----------|----------|
| Cursor blink | Opacity 1→0→1 |
| Smooth scroll | Window Y position |
| Window resize | Width/Height with easing |
| Buffer switch | Page flip transition |
| Fade in WebKit | Opacity 0→1 on load |

---

## Migration Strategy

Direct replacement - remove GTK4/GSK entirely.

| Phase | Scope |
|-------|-------|
| 1 | Replace `GskRenderer` with `WgpuRenderer` |
| 2 | Replace GTK4 window/input with `WinitBackend` |
| 3 | Update WebKit/Video to use `ExternalBuffer` trait |
| 4 | Add animation system |
| 5 | Delete all GTK4/GSK code |

No feature flags, no parallel backends. Clean break.

---

## Dependencies

New crates:
- `winit` - window management, input
- `wgpu` - GPU rendering
- `arboard` - clipboard
- `raw-window-handle` - window handle abstraction

Keep:
- `cosmic-text` - text shaping/rasterization
- `image` - image decoding
- WPE WebKit (via FFI)
- GStreamer (via gstreamer-rs)
