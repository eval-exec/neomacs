# WPE WebKit + wgpu Integration Design

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Import WPE WebKit frames as wgpu textures without any GTK4 dependency.

**Architecture:** Linux DMA-BUF zero-copy import via wgpu's Vulkan HAL.

**Tech Stack:** WPE WebKit, EGL, Vulkan (via ash crate), wgpu HAL

---

## 1. Architecture Overview

**Data flow:**
```
WPE WebKit → EGLImage → DmaBufExporter → DmaBufBuffer → wgpu HAL → wgpu::Texture
```

**Key components:**

1. **DmaBufExporter (modified)** - Returns `DmaBufBuffer` instead of `gdk4::Texture`
2. **DmaBufBuffer::to_wgpu_texture() (implemented)** - Uses wgpu's Vulkan HAL to import DMA-BUF
3. **WgpuWebKitCache (new)** - Caches imported textures by view ID
4. **WebKitBufferProvider trait (new)** - Abstraction for getting frames

---

## 2. DMA-BUF Import to wgpu

Implementation in `external_buffer.rs`:

```rust
#[cfg(target_os = "linux")]
impl DmaBufBuffer {
    pub fn to_wgpu_texture(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) -> Option<wgpu::Texture> {
        // 1. Get the Vulkan device from wgpu HAL
        // 2. Check VK_EXT_external_memory_dma_buf support
        // 3. Create VkImage with VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT
        // 4. Import DMA-BUF fd via vkAllocateMemory + VkImportMemoryFdInfoKHR
        // 5. Bind memory to image
        // 6. Wrap VkImage as wgpu::Texture via HAL
    }
}
```

**Required Vulkan extensions:**
- `VK_KHR_external_memory`
- `VK_KHR_external_memory_fd`
- `VK_EXT_external_memory_dma_buf`
- `VK_EXT_image_drm_format_modifier`

**Dependencies:** `ash` crate for raw Vulkan calls

---

## 3. WPE Module Changes

**New `ExportedDmaBuf` struct (dmabuf.rs):**

```rust
/// Exported DMA-BUF data (no GTK4 dependency)
pub struct ExportedDmaBuf {
    pub fds: [i32; 4],        // File descriptors per plane
    pub strides: [u32; 4],    // Stride per plane
    pub offsets: [u32; 4],    // Offset per plane
    pub num_planes: u32,
    pub fourcc: u32,          // DRM format code
    pub modifier: u64,        // DRM modifier
    pub width: u32,
    pub height: u32,
}

impl DmaBufExporter {
    /// Export EGLImage to DMA-BUF (no GTK4)
    pub fn export_egl_image(&self, egl_image: *mut c_void, width: u32, height: u32)
        -> DisplayResult<ExportedDmaBuf>

    /// Convert to DmaBufBuffer for wgpu import
    pub fn to_dmabuf_buffer(&self, exported: ExportedDmaBuf) -> DmaBufBuffer
}
```

**Changes to `WpeWebView` (view.rs):**

```rust
impl WpeWebView {
    /// Get current frame for wgpu
    pub fn get_frame_dmabuf(&self) -> Option<DmaBufBuffer>
}
```

---

## 4. wgpu Integration & Caching

**New `WgpuWebKitCache` (backend/wgpu/webkit_cache.rs):**

```rust
pub struct WgpuWebKitCache {
    views: HashMap<u32, CachedWebKitView>,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

struct CachedWebKitView {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    last_updated: Instant,
}

impl WgpuWebKitCache {
    pub fn new(device: &wgpu::Device) -> Self
    pub fn update_view(&mut self, view_id: u32, buffer: DmaBufBuffer,
                       device: &wgpu::Device, queue: &wgpu::Queue)
    pub fn get_bind_group(&self, view_id: u32) -> Option<&wgpu::BindGroup>
    pub fn remove(&mut self, view_id: u32)
}
```

**Integration with WgpuRenderer:**

```rust
impl WgpuRenderer {
    pub fn set_webkit_cache(&mut self, cache: Arc<Mutex<WgpuWebKitCache>>)
    pub fn render_webkit_view(&mut self, view_id: u32, bounds: Rect)
}
```

---

## 5. Feature Gating & Dependencies

**Cargo.toml:**

```toml
[dependencies]
ash = { version = "0.38", optional = true }
# DELETE all gtk4/gdk4/gsk4/pango/pangocairo dependencies

[features]
default = ["winit-backend"]
winit-backend = ["winit", "wgpu", "raw-window-handle", "arboard", "bytemuck", "pollster"]
webview = ["winit-backend", "ash"]
video = ["gstreamer", "gstreamer-video"]
# DELETE gtk4-backend feature
```

---

## 6. Implementation Tasks

**Files to delete:**
- `src/backend/gtk4/` (entire directory)

**Files to heavily modify:**
- `Cargo.toml` - remove GTK4 deps, add ash
- `src/backend/mod.rs` - remove gtk4 module, BackendType::Gtk4
- `src/ffi.rs` - remove all gtk4 code paths
- `src/backend/wpe/dmabuf.rs` - remove gdk4, return DmaBufBuffer
- `src/backend/wpe/view.rs` - remove gdk4::Texture path
- `src/backend/wpe/platform.rs` - remove gdk4 imports

**Files to create:**
- `src/backend/wgpu/webkit_cache.rs` - texture caching for wgpu

**Files to implement:**
- `src/backend/wgpu/external_buffer.rs` - actual DMA-BUF → wgpu import

---

## 7. Future Work (Cross-Platform)

For non-Linux platforms, add shared memory fallback:
1. WPE exports frame to shared memory instead of DMA-BUF
2. Copy to wgpu texture via `queue.write_texture()`
3. Slower but works on macOS/Windows

This is deferred - Linux DMA-BUF path first.
