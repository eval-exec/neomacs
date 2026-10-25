//! Clone row storage without per-field reconstruction of non-owning glyphs.
//!
//! This proof is variant-dependent: Glyph itself must not implement Copy.
//! Keep both exhaustive matches and the Copy checks in sync with the types.
//! A custom Clone or Drop for Glyph/GlyphType requires a new ownership audit.

use super::*;

#[inline(always)]
fn assert_copy<T: Copy>(_: &T) {}

/// Every field outside GlyphType is Copy, and every accepted active variant
/// has exclusively Copy fields. No wildcard/remaining-fields pattern is used:
/// additions must be classified before they can enter the unsafe copy.
#[inline]
fn glyph_has_only_copy_payloads(glyph: &Glyph) -> bool {
    let Glyph {
        glyph_type,
        face_id,
        provenance,
        bidi_level,
        wide,
        pixel_width,
        pixel_height,
        pixel_ascent,
        vertical_offset_px,
        padding,
        box_vertical_edges,
        pointer_appearance,
    } = glyph;
    assert_copy(face_id);
    assert_copy(provenance);
    assert_copy(bidi_level);
    assert_copy(wide);
    assert_copy(pixel_width);
    assert_copy(pixel_height);
    assert_copy(pixel_ascent);
    assert_copy(vertical_offset_px);
    assert_copy(padding);
    assert_copy(box_vertical_edges);
    assert_copy(pointer_appearance);
    match glyph_type {
        GlyphType::Char { ch } => {
            assert_copy(ch);
            true
        }
        GlyphType::Composite { text: _ } => false,
        GlyphType::AutomaticComposite {
            text: _,
            terminal: _,
        } => false,
        GlyphType::Stretch { width_cols } => {
            assert_copy(width_cols);
            true
        }
        GlyphType::Image {
            image_id,
            width_cols,
            source_rect,
            margins,
            opaque_background,
        } => {
            assert_copy(image_id);
            assert_copy(width_cols);
            assert_copy(source_rect);
            assert_copy(margins);
            assert_copy(opaque_background);
            true
        }
        GlyphType::Video {
            video_id,
            width_cols,
            opacity,
        } => {
            assert_copy(video_id);
            assert_copy(width_cols);
            assert_copy(opacity);
            true
        }
        GlyphType::Xwidget {
            xwidget_id,
            webview_id,
            width_cols,
            content,
        } => {
            assert_copy(xwidget_id);
            assert_copy(webview_id);
            assert_copy(width_cols);
            assert_copy(content);
            true
        }
        GlyphType::Surface {
            surface_id,
            width_cols,
        } => {
            assert_copy(surface_id);
            assert_copy(width_cols);
            true
        }
        GlyphType::Glyphless { ch, presentation } => {
            assert_copy(ch);
            assert_copy(presentation);
            true
        }
    }
}

/// Compile-only ownership witness. Moving the non-Copy fields out makes a
/// future custom Drop on Glyph or GlyphType a compile error, even when its
/// borrowed fields would still satisfy the predicate above.
const _: fn(Glyph) = |glyph| {
    let Glyph {
        glyph_type,
        face_id: _,
        provenance: _,
        bidi_level: _,
        wide: _,
        pixel_width: _,
        pixel_height: _,
        pixel_ascent: _,
        vertical_offset_px: _,
        padding: _,
        box_vertical_edges: _,
        pointer_appearance: _,
    } = glyph;
    match glyph_type {
        GlyphType::Char { ch: _ } => {}
        GlyphType::Composite { text } => drop(text),
        GlyphType::AutomaticComposite { text, terminal } => {
            drop(text);
            drop(terminal);
        }
        GlyphType::Stretch { width_cols: _ } => {}
        GlyphType::Image {
            image_id: _,
            width_cols: _,
            source_rect: _,
            margins: _,
            opaque_background: _,
        } => {}
        GlyphType::Video {
            video_id: _,
            width_cols: _,
            opacity: _,
        } => {}
        GlyphType::Xwidget {
            xwidget_id: _,
            webview_id: _,
            width_cols: _,
            content: _,
        } => {}
        GlyphType::Surface {
            surface_id: _,
            width_cols: _,
        } => {}
        GlyphType::Glyphless {
            ch: _,
            presentation: _,
        } => {}
    }
};

fn clone_area(source: &[Glyph]) -> Vec<Glyph> {
    if source.is_empty() {
        return Vec::new();
    }
    if !source.iter().all(glyph_has_only_copy_payloads) {
        return source.to_vec();
    }
    let mut result = Vec::<Glyph>::with_capacity(source.len());
    // SAFETY: the exhaustive predicate proves that every active source
    // variant owns only Copy payloads, and the remaining Glyph fields are
    // Copy. In particular neither Box/Arc-owning composition variant can
    // enter here. The immutable slice cannot change between proof and copy.
    // The fresh allocation is aligned, disjoint, and large enough for len
    // initialized Glyph values. This typed copy also preserves padding
    // without reading it as initialized bytes. No element is scheduled for
    // drop until the whole infallible copy completes. Both copies then drop
    // only their non-owning active fields; the original remains untouched.
    unsafe {
        core::ptr::copy_nonoverlapping(source.as_ptr(), result.as_mut_ptr(), source.len());
        result.set_len(source.len());
    }
    result
}

impl Clone for GlyphRow {
    fn clone(&self) -> Self {
        let Self {
            glyphs,
            pointer_appearances,
            image_margins,
            string_sources,
            pointer_runs,
            hash,
            enabled,
            role,
            cursor_col,
            cursor_type,
            truncated_left,
            continued,
            reversed_p,
            displays_text,
            ends_at_zv,
            mode_line,
            pixel_x,
            start_col,
            pixel_y,
            height_px,
            line_spacing_px,
            ascent_px,
            start_charpos,
            end_charpos,
            left_fringe_bitmap,
            right_fringe_bitmap,
            overlay_arrow_bitmap,
            appearance,
        } = self;
        Self {
            glyphs: std::array::from_fn(|area| clone_area(&glyphs[area])),
            pointer_appearances: pointer_appearances.clone(),
            image_margins: image_margins.clone(),
            string_sources: string_sources.clone(),
            pointer_runs: pointer_runs.clone(),
            hash: *hash,
            enabled: *enabled,
            role: *role,
            cursor_col: *cursor_col,
            cursor_type: *cursor_type,
            truncated_left: *truncated_left,
            continued: *continued,
            reversed_p: *reversed_p,
            displays_text: *displays_text,
            ends_at_zv: *ends_at_zv,
            mode_line: *mode_line,
            pixel_x: *pixel_x,
            start_col: *start_col,
            pixel_y: *pixel_y,
            height_px: *height_px,
            line_spacing_px: *line_spacing_px,
            ascent_px: *ascent_px,
            start_charpos: *start_charpos,
            end_charpos: *end_charpos,
            left_fringe_bitmap: *left_fringe_bitmap,
            right_fringe_bitmap: *right_fringe_bitmap,
            overlay_arrow_bitmap: *overlay_arrow_bitmap,
            // RowAppearance::clone deliberately issues a new identity. Keep
            // it last, just as in the former derived Clone implementation.
            appearance: appearance.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
