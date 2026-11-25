//! A canonical rendering of a materialized frame, for comparing what two
//! frames draw.
//!
//! Glyphs, cursors and the drawing parameters are written in `Debug` form,
//! which writes every float so that it round-trips (so `-0.0` differs from
//! `0.0`); hash maps are written sorted by key. Two frames that draw the same
//! thing have the same canonical text.

use std::fmt::Write as _;

use neomacs_display_protocol::FrameGlyphBuffer;

fn sorted_lines<K: Ord + std::fmt::Debug, V: std::fmt::Debug>(
    out: &mut String,
    label: &str,
    entries: impl IntoIterator<Item = (K, V)>,
) {
    let mut entries: Vec<(K, V)> = entries.into_iter().collect();
    entries.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (key, value) in entries {
        let _ = writeln!(out, "{label} {key:?} {value:?}");
    }
}

/// The canonical text of a materialized frame.
#[must_use]
pub fn materialized(buffer: &FrameGlyphBuffer) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "frame {:?} {:?} presentation {:?} origin {:?} placement {:?}",
        buffer.width, buffer.height, buffer.presentation_id, buffer.origin, buffer.frame_placement
    );
    let _ = writeln!(
        out,
        "cell {:?} {:?} font {:?} background {:?} alpha {:?} {:?}",
        buffer.char_width,
        buffer.char_height,
        buffer.font_pixel_size,
        buffer.background,
        buffer.background_alpha,
        buffer.frame_alpha
    );
    let _ = writeln!(
        out,
        "decorations {:?} border {:?} {:?} outer {:?} {:?} no-focus {:?}",
        buffer.undecorated,
        buffer.border_width,
        buffer.border_color,
        buffer.outer_border_width,
        buffer.outer_border_color,
        buffer.no_accept_focus
    );
    let _ = writeln!(
        out,
        "input {:?} {:?} scroll {:?} renewals {:?}",
        buffer.input_checkpoint,
        buffer.scroll_input_policy,
        buffer.scroll_surfaces.len(),
        buffer.scroll_coverage_renewals
    );
    let _ = writeln!(
        out,
        "fonts generation {:?} options {:?}",
        buffer.font_catalog_generation, buffer.desktop_font_render_options
    );
    for glyph in &buffer.glyphs {
        let _ = writeln!(out, "glyph {glyph:?}");
    }
    for cursor in &buffer.window_cursors {
        let _ = writeln!(out, "cursor {cursor:?}");
    }
    for surface in &buffer.scroll_surfaces {
        for glyph in surface.coverage_glyphs() {
            let _ = writeln!(out, "coverage {glyph:?}");
        }
    }
    for info in &buffer.window_infos {
        let _ = writeln!(out, "window {info:?}");
    }
    for hint in &buffer.transition_hints {
        let _ = writeln!(out, "transition {hint:?}");
    }
    let hits = buffer.presented_hit_index();
    let _ = writeln!(
        out,
        "hits {:?} {:?} {:?} text {:?} strings {:?}",
        hits.presentation(),
        hits.regions(),
        hits.resize_handles(),
        hits.text_positions(),
        hits.string_positions()
    );
    sorted_lines(&mut out, "face", buffer.faces.iter());
    sorted_lines(&mut out, "font", buffer.fonts.iter());
    sorted_lines(
        &mut out,
        "char-fonts",
        buffer.char_fonts.iter().map(|(face, entries)| {
            let mut entries: Vec<_> = entries.iter().collect();
            entries.sort_by_key(|(ch, _)| **ch);
            (face, format!("{entries:?}"))
        }),
    );
    sorted_lines(
        &mut out,
        "clusters",
        buffer.shaped_clusters.iter().map(|(face, entries)| {
            let mut entries: Vec<_> = entries.iter().collect();
            entries.sort_by_key(|(text, _)| *text);
            (face, format!("{entries:?}"))
        }),
    );
    sorted_lines(&mut out, "fringe", buffer.fringe_bitmaps.iter());
    sorted_lines(&mut out, "effects", buffer.cursor_effects_by_window.iter());
    out
}
