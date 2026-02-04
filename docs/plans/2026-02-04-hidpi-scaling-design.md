# HiDPI Scaling Support Design

## Overview

Add full HiDPI awareness to neomacs so Emacs works in logical coordinates with crisp 2x glyph rendering on high-DPI displays.

**Goals:**
- Emacs sees logical pixels (800x600 at 200% scale)
- Glyphs rasterized at 2x for sharp text
- Mouse coordinates in logical space
- Support integer scale factors (1x, 2x, 3x)

## Architecture

### Scale Factor Ownership

Rust (winit) owns the scale factor:
```
winit window.scale_factor() = 2.0
         ↓
   RenderThread stores scale_factor
         ↓
   Resize event to Emacs: {width: 800, height: 600, scale: 2.0}
         ↓
   Emacs stores in dpyinfo->scale_factor
```

### Coordinate Systems

**Emacs (logical coordinates):**
- Frame dimensions: 800x600
- Mouse positions: 0-800, 0-600
- Glyph positions: logical pixels

**Rust rendering (physical coordinates):**
- Surface size: 1600x1200
- Glyph atlas: 2x resolution textures
- All rendering in physical pixels

**Conversion at FFI boundary:**
```
Emacs → Rust (glyph positions):
  physical_x = logical_x * scale_factor
  physical_y = logical_y * scale_factor

Rust → Emacs (mouse events):
  logical_x = physical_x / scale_factor
  logical_y = physical_y / scale_factor
```

## Rust Changes

### RenderThread State

```rust
// In RenderApp (render_thread.rs)
struct RenderApp {
    scale_factor: f64,  // From winit, e.g., 2.0
    // ... existing fields
}
```

### Winit Event Handling

```rust
// Handle scale factor changes
WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
    self.scale_factor = scale_factor;
    // Trigger resize event to Emacs with new scale
}

// Resize events use logical size
WindowEvent::Resized(physical_size) => {
    let logical = physical_size.to_logical::<u32>(self.scale_factor);
    // Send logical size + scale_factor to Emacs
}
```

### Mouse Events

```rust
WindowEvent::CursorMoved { position, .. } => {
    let logical_x = (position.x / self.scale_factor) as i32;
    let logical_y = (position.y / self.scale_factor) as i32;
    // Send logical coordinates to Emacs
}
```

### FFI Glyph Rendering

```rust
// In neomacs_display_draw_glyph_string()
let physical_x = (x as f64 * scale_factor) as f32;
let physical_y = (y as f64 * scale_factor) as f32;
let physical_font_size = font_size as f32 * scale_factor as f32;
```

### Glyph Atlas

```rust
struct GlyphCacheKey {
    character: char,
    font_id: u32,
    font_size_bits: u32,
    scale_factor_bits: u32,  // Distinguish 1x vs 2x glyphs
}

// Rasterize at physical size
let physical_size = (font_size as f32 * scale_factor) as u32;
```

### Surface Configuration

```rust
// Surface uses physical size for full resolution
let physical_size = window.inner_size();  // 1600x1200
surface.configure(&device, &SurfaceConfiguration {
    width: physical_size.width,
    height: physical_size.height,
    // ...
});
```

## C/Emacs Changes

### Display Info Structure

```c
// neomacsterm.h
struct neomacs_display_info {
    // ... existing fields
    double scale_factor;  // HiDPI scale (1.0, 2.0, etc.)
};
```

### Event Structure

Add scale_factor to resize event data:
```c
// Width/height are logical, scale_factor for DPI
uint32_t width;
uint32_t height;
float scale_factor;
```

### Resize Handler

```c
// neomacsterm.c
case NEOMACS_EVENT_RESIZE:
  {
    dpyinfo->scale_factor = ev->scale_factor;
    dpyinfo->width = ev->width;   // Already logical
    dpyinfo->height = ev->height;

    // Update DPI based on scale
    dpyinfo->resx = 96.0 * dpyinfo->scale_factor;
    dpyinfo->resy = 96.0 * dpyinfo->scale_factor;

    // Frame size uses logical dimensions
    change_frame_size(f, ev->width, ev->height, false, true, false);
  }
  break;
```

## Files to Modify

| File | Changes |
|------|---------|
| `rust/neomacs-display/src/render_thread.rs` | Store scale_factor, convert coordinates, handle ScaleFactorChanged |
| `rust/neomacs-display/src/ffi.rs` | Scale glyph positions, pass scale to rasterizer |
| `rust/neomacs-display/src/backend/wgpu/glyph_atlas.rs` | Include scale in cache key |
| `rust/neomacs-display/src/backend/wgpu/events.rs` | Add scale_factor field to resize event |
| `src/neomacsterm.h` | Add scale_factor to dpyinfo |
| `src/neomacsterm.c` | Store scale_factor from resize events |

## What Stays the Same

- Emacs frame/window calculations (all logical)
- Font selection code (requests logical sizes)
- Buffer/text positioning (all logical)
- Most of neomacsterm.c event handling

## Fractional Scales

Only integer scale factors (1x, 2x, 3x) are supported. If the system reports a fractional scale like 1.5x, round to the nearest integer (preferring up for better quality).

## Testing

1. Start Emacs on 200% scaled display
2. Verify window reports logical size (800x600)
3. Verify text is crisp (not blurry)
4. Verify mouse clicks map to correct buffer positions
5. Move window to different-DPI monitor, verify re-scaling
