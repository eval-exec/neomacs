//! A pooled raster of certified body coverage. Root composition still draws
//! chrome/cursors normally and child frames remain above this layer. This owns
//! no editor state and never publishes a projection or retires an input.
use crate::render_thread::frame_windows::GuiFrameRenderState;
use neomacs_display_protocol::{
    Color, DeviceScale, FrameGlyph, FrameGlyphBuffer, FrameRect, GeometrySize, GlyphRowRole,
    LogicalPixels, PresentMapping, PresentationExtent, Rect, SurfaceState,
    scroll_coverage::ScrollSurface,
};
use neomacs_renderer_wgpu::{SnapshotLease, SnapshotSize, WgpuRenderer};
use std::sync::Arc;

// Inactive parameters do not affect body pixels. In particular, startup's
// Lisp gallery rounds colors through sRGB strings even for disabled effects.
// Check activation rather than comparing complete configuration snapshots.
fn static_body_effects(effects: &neomacs_display_protocol::EffectsConfig) -> bool {
    !effects.has_enabled_effects_other_than_cursor_color_cycle()
        && effects.bg_pattern.style == 0
        && effects.mode_line_separator.style == 0
}

struct RasterPaint {
    surface: Arc<ScrollSurface>,
    background: Color,
    defaults: [f32; 3],
    font_catalog: neomacs_display_protocol::font::FontCatalogGeneration,
    scale: DeviceScale,
    size: SnapshotSize,
    origin: (f32, f32),
}

impl RasterPaint {
    fn matches(&self, next: &Self) -> bool {
        let previous = self.surface.coverage();
        let coverage = next.surface.coverage();
        self.scale == next.scale
            && self.size == next.size
            && self.origin == next.origin
            && self.background == next.background
            && self.defaults == next.defaults
            && self.font_catalog == next.font_catalog
            && previous.epoch == coverage.epoch
            && previous.content.window_id == coverage.content.window_id
            && previous.content.text_clip_bounds == coverage.content.text_clip_bounds
            && (Arc::ptr_eq(&self.surface, &next.surface)
                || (self.surface.coverage_glyphs() == next.surface.coverage_glyphs()
                    && previous.faces == coverage.faces
                    && previous.fonts == coverage.fonts
                    && previous.char_fonts == coverage.char_fonts
                    && previous.shaped_clusters == coverage.shaped_clusters))
    }
}

pub(in crate::render_thread) struct RetainedScroll {
    paint: RasterPaint,
    texture: SnapshotLease,
    // A new coverage picture often lasts only one authoritative scene. Keep
    // its immutable inputs and require proportionally more evidence of
    // reuse before building larger rasters. This never delays presentation.
    pending: Option<(RasterPaint, u32)>,
}

pub(super) struct PreparedScrollRaster {
    pub frame: FrameGlyphBuffer,
    pub texture: SnapshotLease,
    pub source_pixels: Rect,
    pub destination: FrameRect,
}

/// Rasterize at the original device-pixel phase, even if the text area's
/// logical origin is fractional. The texture includes the rounding padding.
fn raster_geometry(
    bounds: Rect,
    scale: DeviceScale,
    limit: u32,
) -> Option<(SnapshotSize, (f32, f32))> {
    let scale = scale.get();
    let x = (bounds.x * scale).floor();
    let y = (bounds.y * scale).floor();
    let width = (bounds.right() * scale).ceil() - x;
    let height = (bounds.bottom() * scale).ceil() - y;
    if !width.is_finite()
        || !height.is_finite()
        || width < 1.0
        || height < 1.0
        || width > limit as f32
        || height > limit as f32
    {
        return None;
    }
    Some((
        SnapshotSize::new(width as u32, height as u32)?,
        (x / scale, y / scale),
    ))
}

fn body_background(frame: &FrameGlyphBuffer, viewport: Rect) -> Option<Color> {
    let mut background = frame.background;
    if frame.background_alpha != 1.0 {
        return None;
    }
    for glyph in &frame.glyphs {
        if let FrameGlyph::Background { bounds, color } = glyph {
            let intersects = bounds.x < viewport.right()
                && bounds.right() > viewport.x
                && bounds.y < viewport.bottom()
                && bounds.bottom() > viewport.y;
            if intersects {
                // A partial or translucent background needs the full layer
                // compositor; don't turn it into an opaque scrolling layer.
                if bounds.x > viewport.x
                    || bounds.y > viewport.y
                    || bounds.right() < viewport.right()
                    || bounds.bottom() < viewport.bottom()
                {
                    return None;
                }
                background = *color;
            }
        }
    }
    (background.a == 1.0).then_some(background)
}

fn raster_frame(
    surface: &ScrollSurface,
    frame: &FrameGlyphBuffer,
    size: SnapshotSize,
    origin: (f32, f32),
    scale: DeviceScale,
    background: Color,
) -> FrameGlyphBuffer {
    let coverage = surface.coverage();
    let mut raster = FrameGlyphBuffer::with_size(
        size.width() as f32 / scale.get(),
        size.height() as f32 / scale.get(),
    );
    raster.presentation_id = frame.presentation_id;
    // Coverage owns every binding used by its picture. Do not retain hidden
    // dependencies on unrelated font tables from the current frame.
    raster.font_catalog_generation = frame.font_catalog_generation;
    raster.faces = coverage.faces.clone();
    raster.fonts = coverage.fonts.clone();
    raster.char_fonts = coverage.char_fonts.clone();
    raster.shaped_clusters = coverage.shaped_clusters.clone();
    raster.background = background;
    raster.char_width = frame.char_width;
    raster.char_height = frame.char_height;
    raster.font_pixel_size = frame.font_pixel_size;
    let bounds = coverage
        .content
        .text_clip_bounds
        .expect("certified coverage");
    let clip = Rect::new(
        bounds.x - origin.0,
        bounds.y - origin.1,
        bounds.width,
        bounds.height,
    );
    // Fringe arrows move with the rows but occupy columns outside the text
    // raster. The root frame paints them using their own vertical clip.
    raster.glyphs = surface
        .coverage_glyphs()
        .iter()
        .filter(|glyph| !matches!(glyph, FrameGlyph::FringeBitmap { .. }))
        .cloned()
        .collect();
    for glyph in &mut raster.glyphs {
        match glyph {
            FrameGlyph::Char {
                x,
                y,
                baseline,
                clip_rect,
                ..
            } => {
                *x -= origin.0;
                *y -= origin.1;
                *baseline -= origin.1;
                *clip_rect = Some(clip);
            }
            FrameGlyph::Stretch {
                x, y, clip_rect, ..
            } => {
                *x -= origin.0;
                *y -= origin.1;
                *clip_rect = Some(clip);
            }
            _ => unreachable!("certified text coverage"),
        }
    }
    raster
}

fn retention_allowed(
    renderer: &WgpuRenderer,
    render: &GuiFrameRenderState,
    frame: &FrameGlyphBuffer,
    has_gradient: bool,
) -> bool {
    !has_gradient
        && static_body_effects(&renderer.effects)
        && !render.compositor.renderer_effects.needs_redraw()
        && !super::retained_static::window_has_active_overlays(render)
        && render.pointer_selection_for(frame).is_none()
        && std::env::var_os("NEOMACS_DISABLE_RETAINED_SCROLL").is_none()
}

fn coverage_live(cache: &RetainedScroll, frame: &FrameGlyphBuffer) -> bool {
    frame.scroll_surfaces.iter().any(|next| {
        next.coverage().epoch == cache.paint.surface.coverage().epoch
            && next.coverage().content.window_id == cache.paint.surface.coverage().content.window_id
    })
}

/// Borrow-only retirement: never sample, acknowledge or publish input scroll.
pub(super) fn retire_unusable(
    renderer: &WgpuRenderer,
    render: &mut GuiFrameRenderState,
    has_gradient: bool,
    scale: DeviceScale,
) {
    let obsolete = render
        .compositor
        .retained_scroll
        .as_ref()
        .is_some_and(|cache| {
            render
                .compositor
                .current_frame
                .as_ref()
                .is_none_or(|frame| {
                    !coverage_live(cache, frame)
                        || !retention_allowed(renderer, render, frame, has_gradient)
                        || cache.paint.scale != scale
                        || body_background(frame, cache.paint.surface.coverage().viewport).is_none()
                })
        });
    if obsolete {
        render.compositor.retained_scroll = None;
    }
}

/// Prepare before the root draw, so every rejection takes the unchanged full
/// glyph path. The pool applies its global memory ceiling; an oversized page
/// or refused lease is an ordinary fallback, never an untracked allocation.
pub(super) fn prepare(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    frame: &FrameGlyphBuffer,
    mapping: PresentMapping,
    has_gradient: bool,
) -> Option<PreparedScrollRaster> {
    let Some((surface, offset)) = render.compositor.input_scroll.staged_projection() else {
        // Keep one bounded lease through authoritative-only frames while
        // its coverage epoch remains live. Recheck exact paint before a crop.
        if render
            .compositor
            .retained_scroll
            .as_ref()
            .is_some_and(|cache| !coverage_live(cache, frame))
        {
            render.compositor.retained_scroll = None;
        }
        return None;
    };
    // Effects which alter body pixels need their own raster dependencies.
    // Enabled body effects and live overlays conservatively retain the
    // established full-render path. Disabled parameters are not dependencies.
    if !retention_allowed(renderer, render, frame, has_gradient) {
        render.compositor.retained_scroll = None;
        return None;
    }
    let coverage = surface.coverage();
    let background = body_background(frame, coverage.viewport)?;
    let scale = mapping.surface().device_scale();
    let defaults = [frame.char_width, frame.char_height, frame.font_pixel_size];
    let bounds = coverage.content.text_clip_bounds?;
    let (size, origin) = raster_geometry(
        bounds,
        scale,
        renderer.device().limits().max_texture_dimension_2d,
    )?;
    let paint = RasterPaint {
        surface: surface.clone(),
        background,
        defaults,
        font_catalog: frame.font_catalog_generation,
        scale,
        size,
        origin,
    };
    let valid = render
        .compositor
        .retained_scroll
        .as_ref()
        .is_some_and(|cache| cache.paint.matches(&paint));
    if let Some(cache) = render.compositor.retained_scroll.as_mut() {
        if valid {
            cache.pending = None;
        } else {
            let observations = cache
                .pending
                .as_ref()
                .filter(|(pending, _)| pending.matches(&paint))
                .map_or(1, |(_, count)| count.saturating_add(1));
            let required = (bounds.height / coverage.viewport.height)
                .ceil()
                .clamp(2.0, 192.0) as u32;
            if observations < required {
                cache.pending = Some((paint, observations));
                return None;
            }
        }
    }
    if !valid {
        // Release old coverage before admission, allowing its pool slot to be
        // reclaimed rather than charging both generations against the budget.
        render.compositor.retained_scroll = None;
        let texture = match renderer.acquire_snapshot(size) {
            Ok(texture) => texture,
            Err(_) => {
                crate::render_thread::frame_stats::count(
                    &crate::render_thread::frame_stats::FULL_FRAME_TEXTURE_REFUSALS,
                );
                return None;
            }
        };
        let raster = raster_frame(&surface, frame, size, origin, scale, background);
        let SurfaceState::Drawable(target) =
            SurfaceState::from_device_size(size.width(), size.height(), scale).ok()?
        else {
            return None;
        };
        let raster_mapping = PresentMapping::top_left_clip(
            target,
            PresentationExtent::new(
                raster.presentation_id,
                GeometrySize::<LogicalPixels>::from_px(raster.width, raster.height).ok()?,
            ),
        );
        let atlas = render.compositor.glyph_atlas.as_mut()?;
        atlas.set_current_frame_fonts(raster.font_bindings());
        // Rendering an auxiliary picture must not advance or replace the live
        // frame's effect queues. Its fonts and immutable default metrics are
        // nevertheless the same bindings used by the ordinary root draw.
        renderer.with_frame_effects(&mut Default::default(), |renderer| {
            renderer.render_frame_glyphs(
                texture.view(),
                &raster,
                atlas,
                raster_mapping,
                false,
                None,
                None,
                None,
                None,
                None,
            );
        });
        crate::render_thread::frame_stats::count(
            &crate::render_thread::frame_stats::SCROLL_RASTER_BUILDS,
        );
        tracing::debug!(target: "neomacs_display_runtime::retained_scroll", window = coverage.content.window_id.get(), "rasterized scroll coverage");
        render.compositor.retained_scroll = Some(RetainedScroll {
            paint,
            texture,
            pending: None,
        });
    }
    let cache = render.compositor.retained_scroll.as_ref()?;
    let viewport = coverage.viewport;
    let source_pixels = Rect::new(
        (viewport.x - cache.paint.origin.0) * scale.get(),
        (viewport.y + coverage.origin + surface.clamp_offset(offset) - cache.paint.origin.1)
            * scale.get(),
        viewport.width * scale.get(),
        viewport.height * scale.get(),
    );
    let destination =
        FrameRect::new(viewport.x, viewport.y, viewport.width, viewport.height).ok()?;
    neomacs_renderer_wgpu::renderer::SnapshotRegion::new(
        &cache.texture,
        source_pixels,
        destination,
    )?;
    let mut base = frame.clone();
    let window = coverage.content.window_id;
    base.glyphs.retain(|glyph| {
        glyph.window_id() != Some(window)
            || glyph.row_role() != Some(GlyphRowRole::Text)
            || matches!(
                glyph,
                FrameGlyph::FringeBitmap { .. }
                    | FrameGlyph::ScrollBar { .. }
                    | FrameGlyph::Border { .. }
            )
    });
    Some(PreparedScrollRaster {
        frame: base,
        texture: cache.texture.clone(),
        source_pixels,
        destination,
    })
}

#[cfg(test)]
pub(super) mod tests;

#[cfg(test)]
mod geometry_tests {
    use super::*;
    #[test]
    fn raster_extent_preserves_device_phase_and_rejects_oversized_coverage() {
        let scale = DeviceScale::new(1.5).unwrap();
        let (size, origin) =
            raster_geometry(Rect::new(2.25, 3.75, 20.0, 40.0), scale, 1024).unwrap();
        assert_eq!(origin, (2.0, 10.0 / 3.0));
        assert_eq!((size.width(), size.height()), (31, 61));
        assert!(raster_geometry(Rect::new(0.0, 0.0, 10.0, 10000.0), scale, 1024).is_none());
    }
}
