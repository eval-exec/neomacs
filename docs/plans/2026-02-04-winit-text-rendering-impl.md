# Winit Text Rendering Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Render text in winit windows using the frame_glyphs buffer and WgpuGlyphAtlas.

**Architecture:** Use existing frame_glyphs collection path for all windows. Add glyph rendering pipeline to WgpuRenderer. Render frame_glyphs to wgpu surface in end_frame_for_window.

**Tech Stack:** Rust, wgpu, cosmic-text (via WgpuGlyphAtlas)

---

### Task 1: Add glyph pipeline to WgpuRenderer

**Files:**
- Modify: `rust/neomacs-display/src/backend/wgpu/renderer.rs`

**Step 1: Add glyph_pipeline field to struct**

In `WgpuRenderer` struct (around line 13), add after `rect_pipeline`:

```rust
pub struct WgpuRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    rect_pipeline: wgpu::RenderPipeline,
    glyph_pipeline: wgpu::RenderPipeline,  // ADD THIS
    glyph_bind_group_layout: wgpu::BindGroupLayout,  // ADD THIS
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}
```

**Step 2: Add imports**

At top of file, add:

```rust
use super::vertex::{RectVertex, GlyphVertex, Uniforms};
```

**Step 3: Create glyph pipeline in create_renderer_internal**

After the rect_pipeline creation (around line 156), add:

```rust
        // Load glyph shader
        let glyph_shader_source = include_str!("shaders/glyph.wgsl");
        let glyph_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Glyph Shader"),
            source: wgpu::ShaderSource::Wgsl(glyph_shader_source.into()),
        });

        // Glyph bind group layout (for per-glyph texture)
        let glyph_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Glyph Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Glyph pipeline layout (uniform + glyph texture)
        let glyph_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Glyph Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout, &glyph_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create glyph pipeline
        let glyph_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Glyph Pipeline"),
            layout: Some(&glyph_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &glyph_shader,
                entry_point: Some("vs_main"),
                buffers: &[GlyphVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &glyph_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });
```

**Step 4: Update Self construction**

Update the `Self { ... }` block to include the new fields:

```rust
        Self {
            device,
            queue,
            surface,
            surface_config,
            rect_pipeline,
            glyph_pipeline,
            glyph_bind_group_layout,
            uniform_buffer,
            uniform_bind_group,
            width,
            height,
        }
```

**Step 5: Add accessor for glyph_bind_group_layout**

Add method after `resize`:

```rust
    /// Get the glyph bind group layout for creating glyph bind groups
    pub fn glyph_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.glyph_bind_group_layout
    }
```

**Step 6: Build and verify**

Run: `cd ~/.config/superpowers/worktrees/neomacs/text-rendering/rust/neomacs-display && cargo build`

Expected: Compiles without errors

**Step 7: Commit**

```bash
git add rust/neomacs-display/src/backend/wgpu/renderer.rs
git commit -m "feat: add glyph pipeline to WgpuRenderer"
```

---

### Task 2: Add WgpuGlyphAtlas to WgpuBackend

**Files:**
- Modify: `rust/neomacs-display/src/backend/wgpu/backend.rs`

**Step 1: Add import**

At top of file with other imports, add:

```rust
use super::glyph_atlas::WgpuGlyphAtlas;
```

**Step 2: Add field to WgpuBackend struct**

Find `pub struct WgpuBackend` and add field:

```rust
    glyph_atlas: Option<WgpuGlyphAtlas>,
```

**Step 3: Initialize in new()**

In the `new()` function, initialize the field:

```rust
            glyph_atlas: None,
```

**Step 4: Initialize atlas in init_wgpu_headless**

After `self.renderer = Some(renderer);` add:

```rust
        // Create glyph atlas for text rendering
        let glyph_atlas = WgpuGlyphAtlas::new(&device);
        self.glyph_atlas = Some(glyph_atlas);
```

**Step 5: Build and verify**

Run: `cd ~/.config/superpowers/worktrees/neomacs/text-rendering/rust/neomacs-display && cargo build`

Expected: Compiles without errors

**Step 6: Commit**

```bash
git add rust/neomacs-display/src/backend/wgpu/backend.rs
git commit -m "feat: add WgpuGlyphAtlas to WgpuBackend"
```

---

### Task 3: Add faces HashMap to NeomacsDisplay

**Files:**
- Modify: `rust/neomacs-display/src/ffi.rs`

**Step 1: Add import**

Add to imports at top:

```rust
use std::collections::HashMap;
use crate::core::face::Face;
```

**Step 2: Add field to NeomacsDisplay struct**

Find `pub struct NeomacsDisplay` and add:

```rust
    faces: HashMap<u32, Face>,
```

**Step 3: Initialize in neomacs_display_init**

In the `NeomacsDisplay` construction (around line 119), add:

```rust
        faces: HashMap::new(),
```

**Step 4: Store face in neomacs_display_set_face**

Find `neomacs_display_set_face` function. After creating the `face` struct, add before the hybrid path check:

```rust
    // Store face for later lookup during rendering
    display.faces.insert(face_id, face.clone());
```

**Step 5: Build and verify**

Run: `cd ~/.config/superpowers/worktrees/neomacs/text-rendering/rust/neomacs-display && cargo build`

Expected: Compiles without errors

**Step 6: Commit**

```bash
git add rust/neomacs-display/src/ffi.rs
git commit -m "feat: add faces HashMap to NeomacsDisplay"
```

---

### Task 4: Revert hybrid bypass conditions

**Files:**
- Modify: `rust/neomacs-display/src/ffi.rs`

**Step 1: Revert all hybrid conditions**

Replace all occurrences of:
```rust
if display.use_hybrid && display.current_render_window_id == 0 {
```

With:
```rust
if display.use_hybrid {
```

There are 13 occurrences at these lines: 248, 284, 356, 410, 461, 556, 637, 717, 909, 977, 1297, 1361, 2364

**Step 2: Build and verify**

Run: `cd ~/.config/superpowers/worktrees/neomacs/text-rendering/rust/neomacs-display && cargo build`

Expected: Compiles without errors

**Step 3: Commit**

```bash
git add rust/neomacs-display/src/ffi.rs
git commit -m "fix: revert hybrid bypass - use frame_glyphs for all windows"
```

---

### Task 5: Implement render_frame_glyphs in WgpuRenderer

**Files:**
- Modify: `rust/neomacs-display/src/backend/wgpu/renderer.rs`

**Step 1: Add imports**

Add at top of file:

```rust
use crate::core::frame_glyphs::{FrameGlyphBuffer, FrameGlyph};
use crate::core::face::Face;
use super::glyph_atlas::{WgpuGlyphAtlas, GlyphKey};
use std::collections::HashMap;
```

**Step 2: Add render_frame_glyphs method**

Add this method to `impl WgpuRenderer`:

```rust
    /// Render frame glyphs to a texture view
    pub fn render_frame_glyphs(
        &self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        faces: &HashMap<u32, Face>,
    ) {
        // Update uniforms for current frame size
        let uniforms = Uniforms {
            screen_size: [frame_glyphs.width, frame_glyphs.height],
            _padding: [0.0, 0.0],
        };
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Collect rectangles (backgrounds, stretches, cursors, borders)
        let mut rect_vertices: Vec<RectVertex> = Vec::new();

        // 1. Draw frame background
        self.add_rect(
            &mut rect_vertices,
            0.0,
            0.0,
            frame_glyphs.width,
            frame_glyphs.height,
            &frame_glyphs.background,
        );

        // 2. Process glyphs in order: backgrounds first, then stretches
        for glyph in &frame_glyphs.glyphs {
            match glyph {
                FrameGlyph::Background { bounds, color } => {
                    self.add_rect(
                        &mut rect_vertices,
                        bounds.x,
                        bounds.y,
                        bounds.width,
                        bounds.height,
                        color,
                    );
                }
                FrameGlyph::Stretch { x, y, width, height, bg, .. } => {
                    self.add_rect(&mut rect_vertices, *x, *y, *width, *height, bg);
                }
                FrameGlyph::Border { x, y, width, height, color } => {
                    self.add_rect(&mut rect_vertices, *x, *y, *width, *height, color);
                }
                FrameGlyph::Cursor { x, y, width, height, color, .. } => {
                    self.add_rect(&mut rect_vertices, *x, *y, *width, *height, color);
                }
                _ => {} // Chars handled separately
            }
        }

        // Create command encoder
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Frame Glyphs Encoder"),
        });

        // Render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Frame Glyphs Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: frame_glyphs.background.r as f64,
                            g: frame_glyphs.background.g as f64,
                            b: frame_glyphs.background.b as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Draw rectangles (backgrounds, stretches, borders, cursors)
            if !rect_vertices.is_empty() {
                let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Rect Vertex Buffer"),
                    contents: bytemuck::cast_slice(&rect_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                render_pass.set_pipeline(&self.rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, rect_buffer.slice(..));
                render_pass.draw(0..rect_vertices.len() as u32, 0..1);
            }

            // Draw character glyphs
            render_pass.set_pipeline(&self.glyph_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Char { char, x, y, width, height, ascent, fg, face_id, .. } = glyph {
                    let key = GlyphKey {
                        charcode: *char as u32,
                        face_id: *face_id,
                    };

                    let face = faces.get(face_id);

                    if let Some(cached) = glyph_atlas.get_or_create(&self.device, &self.queue, &key, face) {
                        // Calculate position with bearing
                        let glyph_x = *x + cached.bearing_x;
                        let glyph_y = *y + *ascent - cached.bearing_y;
                        let glyph_w = cached.width as f32;
                        let glyph_h = cached.height as f32;

                        // Create quad vertices
                        let vertices = [
                            GlyphVertex { position: [glyph_x, glyph_y], tex_coords: [0.0, 0.0], color: [fg.r, fg.g, fg.b, fg.a] },
                            GlyphVertex { position: [glyph_x + glyph_w, glyph_y], tex_coords: [1.0, 0.0], color: [fg.r, fg.g, fg.b, fg.a] },
                            GlyphVertex { position: [glyph_x + glyph_w, glyph_y + glyph_h], tex_coords: [1.0, 1.0], color: [fg.r, fg.g, fg.b, fg.a] },
                            GlyphVertex { position: [glyph_x, glyph_y], tex_coords: [0.0, 0.0], color: [fg.r, fg.g, fg.b, fg.a] },
                            GlyphVertex { position: [glyph_x + glyph_w, glyph_y + glyph_h], tex_coords: [1.0, 1.0], color: [fg.r, fg.g, fg.b, fg.a] },
                            GlyphVertex { position: [glyph_x, glyph_y + glyph_h], tex_coords: [0.0, 1.0], color: [fg.r, fg.g, fg.b, fg.a] },
                        ];

                        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Glyph Vertex Buffer"),
                            contents: bytemuck::cast_slice(&vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                        render_pass.set_bind_group(1, &cached.bind_group, &[]);
                        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                        render_pass.draw(0..6, 0..1);
                    }
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }
```

**Step 3: Build and verify**

Run: `cd ~/.config/superpowers/worktrees/neomacs/text-rendering/rust/neomacs-display && cargo build`

Expected: Compiles without errors

**Step 4: Commit**

```bash
git add rust/neomacs-display/src/backend/wgpu/renderer.rs
git commit -m "feat: implement render_frame_glyphs for text rendering"
```

---

### Task 6: Wire up end_frame_for_window to render frame_glyphs

**Files:**
- Modify: `rust/neomacs-display/src/backend/wgpu/backend.rs`
- Modify: `rust/neomacs-display/src/ffi.rs`

**Step 1: Add imports to backend.rs**

Add at top:

```rust
use crate::core::frame_glyphs::FrameGlyphBuffer;
use crate::core::face::Face;
use std::collections::HashMap;
```

**Step 2: Update end_frame_for_window signature in backend.rs**

Change the method signature and implementation:

```rust
    /// End a frame for a specific window and present it.
    ///
    /// Renders the frame glyphs to the window's surface and presents it.
    pub fn end_frame_for_window(
        &mut self,
        window_id: u32,
        frame_glyphs: &FrameGlyphBuffer,
        faces: &HashMap<u32, Face>,
    ) {
        let renderer = match &self.renderer {
            Some(r) => r,
            None => return,
        };

        let state = match self.windows.get_mut(&window_id) {
            Some(s) => s,
            None => return,
        };

        let output = match state.surface.get_current_texture() {
            Ok(t) => t,
            Err(e) => {
                log::warn!("Failed to get surface texture: {:?}", e);
                return;
            }
        };

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Get mutable reference to glyph atlas
        if let Some(ref mut glyph_atlas) = self.glyph_atlas {
            renderer.render_frame_glyphs(&view, frame_glyphs, glyph_atlas, faces);
        }

        output.present();
        state.window.request_redraw();
    }
```

**Step 3: Update FFI call in ffi.rs**

Find `neomacs_display_end_frame_window` function and update:

```rust
#[no_mangle]
pub extern "C" fn neomacs_display_end_frame_window(
    handle: *mut NeomacsDisplay,
    window_id: u32,
) {
    let display = unsafe { &mut *handle };

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        // Render frame_glyphs to the winit window
        backend.end_frame_for_window(
            window_id,
            &display.frame_glyphs,
            &display.faces,
        );
    }

    // Reset current window tracking after rendering is complete
    display.current_render_window_id = 0;
}
```

**Step 4: Build and verify**

Run: `cd ~/.config/superpowers/worktrees/neomacs/text-rendering/rust/neomacs-display && cargo build`

Expected: Compiles without errors

**Step 5: Run tests**

Run: `cd ~/.config/superpowers/worktrees/neomacs/text-rendering/rust/neomacs-display && cargo test --lib`

Expected: All tests pass

**Step 6: Commit**

```bash
git add rust/neomacs-display/src/backend/wgpu/backend.rs rust/neomacs-display/src/ffi.rs
git commit -m "feat: wire up end_frame_for_window to render frame_glyphs"
```

---

### Task 7: Integration test

**Step 1: Build full Emacs**

```bash
cd ~/.config/superpowers/worktrees/neomacs/text-rendering
./autogen.sh
./configure --with-neomacs
make -j$(nproc)
```

**Step 2: Run and verify**

```bash
RUST_LOG=info src/emacs -Q
```

Expected:
- Window appears with text visible
- Scratch buffer shows welcome message
- Modeline visible at bottom
- Echo area visible
- Cursor visible

**Step 3: Take screenshot to verify**

Use screenshot tool to capture the window for visual verification.

**Step 4: Commit any fixes if needed**

If issues found, debug and fix before final commit.
