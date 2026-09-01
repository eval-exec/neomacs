use crate::bidi::{self, BidiDir};
use neomacs_display_protocol::glyph_matrix::{
    Glyph, GlyphArea, GlyphProvenance, GlyphRow, GlyphType,
};
use neomacs_display_protocol::types::FaceId;

#[derive(Clone)]
struct BidiGlyphUnit {
    ch: char,
    cols: Vec<usize>,
    glyphs: Vec<Glyph>,
}

/// Recompute physical terminals from visual adjacency after UAX#9 reordering.
/// Swapping each odd glyph independently is insufficient: reordering can put
/// boxed glyphs from different logical runs next to one another, or separate
/// logical neighbors. GNU decides the physical terminals from visual
/// boxed/unboxed adjacency while preserving independently open viewport/row
/// boundaries. The explicit membership bit makes `Neither` unambiguous.
fn restamp_visual_box_runs(
    row: &mut GlyphRow,
    units: &[BidiGlyphUnit],
    levels: &[u8],
    visual_order: &[usize],
) {
    let available_cols = units
        .iter()
        .flat_map(|unit| unit.cols.iter().copied())
        .collect::<Vec<_>>();
    let mut next_col = 0usize;
    let text = &mut row.glyphs[GlyphArea::Text.index()];
    for (visual_index, &logical_index) in visual_order.iter().enumerate() {
        let unit_len = units[logical_index].glyphs.len();
        let Some(&target_start) = available_cols.get(next_col) else {
            return;
        };
        let membership = units[logical_index]
            .glyphs
            .iter()
            .find(|glyph| !glyph.padding)
            .map(|glyph| glyph.box_vertical_edges.membership())
            .unwrap_or_default();
        for glyph in &mut text[target_start..target_start + units[logical_index].glyphs.len()] {
            glyph.box_vertical_edges = neomacs_display_protocol::face::BoxVerticalEdges::Neither
                .with_membership(membership);
        }

        if !membership.is_boxed() {
            next_col = next_col.saturating_add(unit_len);
            continue;
        }
        let source_edges = units[logical_index]
            .glyphs
            .iter()
            .find(|glyph| !glyph.padding)
            .map(|glyph| glyph.box_vertical_edges)
            .unwrap_or_default();
        let physical_edges = if levels[logical_index] & 1 == 1 {
            source_edges.reversed()
        } else {
            source_edges
        };
        let previous_boxed = visual_index.checked_sub(1).is_some_and(|index| {
            units[visual_order[index]]
                .glyphs
                .iter()
                .any(|glyph| !glyph.padding && glyph.box_vertical_edges.membership().is_boxed())
        });
        let next_boxed = visual_order.get(visual_index + 1).is_some_and(|&index| {
            units[index]
                .glyphs
                .iter()
                .any(|glyph| !glyph.padding && glyph.box_vertical_edges.membership().is_boxed())
        });
        let owns_left = if visual_index == 0 {
            physical_edges.owns_left()
        } else {
            !previous_boxed
        };
        let owns_right = if visual_index + 1 == visual_order.len() {
            physical_edges.owns_right()
        } else {
            !next_boxed
        };
        if let Some(glyph) = text[target_start..target_start + units[logical_index].glyphs.len()]
            .iter_mut()
            .find(|glyph| !glyph.padding)
        {
            glyph.box_vertical_edges =
                neomacs_display_protocol::face::BoxVerticalEdges::from_ownership(
                    owns_left, owns_right,
                );
        }
        next_col = next_col.saturating_add(unit_len);
    }
}

fn bidi_char_for_glyph(glyph: &Glyph) -> Option<char> {
    if glyph.padding {
        return None;
    }

    match &glyph.glyph_type {
        GlyphType::Char { ch } | GlyphType::Glyphless { ch } => Some(*ch),
        GlyphType::Composite { text } => text.chars().next(),
        GlyphType::Stretch { .. } => Some(' '),
        GlyphType::Image { .. }
        | GlyphType::Video { .. }
        | GlyphType::Xwidget { .. }
        | GlyphType::Surface { .. } => None,
    }
}

fn apply_bidi_mirroring(glyph: &mut Glyph, level: u8) {
    if level & 1 == 0 {
        return;
    }

    match &mut glyph.glyph_type {
        GlyphType::Char { ch } | GlyphType::Glyphless { ch } => {
            if let Some(mirrored) = bidi::bidi_mirror(*ch) {
                *ch = mirrored;
            }
        }
        GlyphType::Composite { .. }
        | GlyphType::Stretch { .. }
        | GlyphType::Image { .. }
        | GlyphType::Video { .. }
        | GlyphType::Xwidget { .. }
        | GlyphType::Surface { .. } => {}
    }
}

fn collect_bidi_units(text: &[Glyph]) -> Vec<BidiGlyphUnit> {
    let mut units = Vec::new();
    let mut idx = 0;

    while idx < text.len() {
        let glyph = &text[idx];
        let Some(ch) = bidi_char_for_glyph(glyph) else {
            idx += 1;
            continue;
        };

        let mut cols = vec![idx];
        let mut glyphs = vec![glyph.clone()];
        idx += 1;

        // Absorb this base's trailing padding cells into the same bidi unit so
        // the glyph stays a contiguous, base-first block under visual
        // reordering: a 2-column wide char or a multi-column composed run.
        // Padding is only emitted immediately after its base, so consecutive
        // padding always belongs to this glyph.
        while idx < text.len() && text[idx].padding {
            cols.push(idx);
            glyphs.push(text[idx].clone());
            idx += 1;
        }

        units.push(BidiGlyphUnit { ch, cols, glyphs });
    }

    units
}

fn rewrite_units_into_row(
    row: &mut GlyphRow,
    original_text: &[Glyph],
    units: &[BidiGlyphUnit],
    levels: &[u8],
    visual_order: &[usize],
    cursor_logical_idx: Option<usize>,
    phys_cursor_logical_idx: Option<usize>,
) -> Option<u16> {
    let available_cols: Vec<usize> = units
        .iter()
        .flat_map(|unit| unit.cols.iter().copied())
        .collect();
    let mut next_col = 0usize;
    let mut reordered = original_text.to_vec();
    let mut visual_cursor_col = None;
    let mut remapped_phys_cursor_col = None;

    for &logical_idx in visual_order {
        let unit = &units[logical_idx];
        let unit_len = unit.glyphs.len();
        let target_cols = available_cols.get(next_col..next_col + unit_len)?;
        if !target_cols.windows(2).all(|w| w[1] == w[0] + 1) {
            return None;
        }

        let target_start = target_cols[0];
        let mut placed = unit.glyphs.clone();
        for glyph in &mut placed {
            glyph.bidi_level = levels[logical_idx];
        }
        if let Some(first) = placed.first_mut() {
            apply_bidi_mirroring(first, levels[logical_idx]);
        }
        for (offset, glyph) in placed.into_iter().enumerate() {
            reordered[target_start + offset] = glyph;
        }

        if cursor_logical_idx == Some(logical_idx) {
            visual_cursor_col = Some(target_start as u16);
        }
        if phys_cursor_logical_idx == Some(logical_idx) {
            remapped_phys_cursor_col = Some(target_start as u16);
        }

        next_col += unit_len;
    }

    row.glyphs[GlyphArea::Text.index()] = reordered;
    if let Some(col) = visual_cursor_col {
        row.cursor_col = Some(col);
    }
    remapped_phys_cursor_col
}

/// Attach a cluster-extender char (combining mark / ZWJ / variation
/// selector) to the last non-padding glyph in `area`, upgrading a
/// `Char` glyph into `Composite` or appending to an existing
/// `Composite`. Returns true when the extender was merged; false when
/// there is no preceding base glyph (caller should fall back to
/// emitting a standalone glyph).
fn merge_extender_into_last_glyph(area: &mut [Glyph], ch: char) -> bool {
    // Walk back past padding cells (the right half of a preceding wide
    // char); the combining mark attaches to the wide base, not the
    // padding slot.
    for glyph in area.iter_mut().rev() {
        if glyph.padding {
            continue;
        }
        match &mut glyph.glyph_type {
            GlyphType::Char { ch: base } => {
                let mut s = String::with_capacity(base.len_utf8() + ch.len_utf8());
                s.push(*base);
                s.push(ch);
                glyph.glyph_type = GlyphType::Composite {
                    text: s.into_boxed_str(),
                };
                return true;
            }
            GlyphType::Composite { text } => {
                let mut s = String::with_capacity(text.len() + ch.len_utf8());
                s.push_str(text);
                s.push(ch);
                glyph.glyph_type = GlyphType::Composite {
                    text: s.into_boxed_str(),
                };
                return true;
            }
            GlyphType::Glyphless { .. }
            | GlyphType::Stretch { .. }
            | GlyphType::Image { .. }
            | GlyphType::Video { .. }
            | GlyphType::Xwidget { .. }
            | GlyphType::Surface { .. } => {
                return false;
            }
        }
    }
    false
}

/// Append `ch` to a glyph's character/cluster text (Char -> Composite,
/// Composite grows). Used to extend the per-cell grapheme of a complex run's
/// member.
fn extend_glyph_grapheme(glyph: &mut Glyph, ch: char) {
    match &mut glyph.glyph_type {
        GlyphType::Char { ch: base } => {
            let mut s = String::with_capacity(base.len_utf8() + ch.len_utf8());
            s.push(*base);
            s.push(ch);
            glyph.glyph_type = GlyphType::Composite {
                text: s.into_boxed_str(),
            };
        }
        GlyphType::Composite { text } => {
            let mut s = String::with_capacity(text.len() + ch.len_utf8());
            s.push_str(text);
            s.push(ch);
            glyph.glyph_type = GlyphType::Composite {
                text: s.into_boxed_str(),
            };
        }
        _ => {}
    }
}

/// Whether `glyph` is a complex-run member's padding cell carrying its own
/// per-cell grapheme (a non-blank Char or a Composite), as opposed to a
/// blank wide-char padding slot. Such cells let the TTY decompose the run.
///
/// These cells must contribute 0 columns and 0 pixels to the rendered metric:
/// the run's base `Composite` already carries the whole run's width (GNU's
/// `cmp->width`, set once in `produce_composite_glyph`, src/term.c). See
/// `DisplayRowWriteMetrics::from_glyphs`.
pub(crate) fn is_run_member_padding(glyph: &Glyph) -> bool {
    glyph.padding
        && match &glyph.glyph_type {
            GlyphType::Char { ch } => *ch != ' ',
            GlyphType::Composite { .. } => true,
            _ => false,
        }
}

fn mark_displays_text_if_text_area(row: &mut GlyphRow, area_index: usize) {
    if area_index == GlyphArea::Text.index() {
        row.displays_text = true;
    }
}

#[cfg(test)]
pub(crate) fn push_char_to_row(
    row: &mut GlyphRow,
    ch: char,
    face_id: FaceId,
    charpos: usize,
    pixel_width: f32,
) {
    push_char_to_area(
        row,
        GlyphArea::Text.index(),
        ch,
        face_id,
        GlyphProvenance::buffer(charpos),
        pixel_width,
    );
}

pub(crate) fn push_char_to_area(
    row: &mut GlyphRow,
    area_index: usize,
    ch: char,
    face_id: FaceId,
    provenance: GlyphProvenance,
    pixel_width: f32,
) {
    let area = &mut row.glyphs[area_index];
    if crate::unicode::is_cluster_extender(ch) && merge_extender_into_last_glyph(area, ch) {
        return;
    }
    area.push(Glyph::char_with_provenance(ch, face_id, provenance).with_pixel_width(pixel_width));
    mark_displays_text_if_text_area(row, area_index);
}

#[cfg(test)]
pub(crate) fn push_wide_char_to_row(
    row: &mut GlyphRow,
    ch: char,
    face_id: FaceId,
    charpos: usize,
    pixel_width: f32,
) {
    push_wide_char_to_area(
        row,
        GlyphArea::Text.index(),
        ch,
        face_id,
        GlyphProvenance::buffer(charpos),
        pixel_width,
    );
}

pub(crate) fn push_wide_char_to_area(
    row: &mut GlyphRow,
    area_index: usize,
    ch: char,
    face_id: FaceId,
    provenance: GlyphProvenance,
    pixel_width: f32,
) {
    let area = &mut row.glyphs[area_index];
    if crate::unicode::is_cluster_extender(ch) && merge_extender_into_last_glyph(area, ch) {
        return;
    }
    let mut glyph = Glyph::char_with_provenance(ch, face_id, provenance);
    glyph.wide = true;
    glyph.pixel_width = if pixel_width.is_finite() && pixel_width > 0.0 {
        pixel_width
    } else {
        0.0
    };
    area.push(glyph);
    area.push(Glyph::padding_with_provenance(face_id, provenance));
    mark_displays_text_if_text_area(row, area_index);
}

/// Append a grapheme-cluster continuation character — a ZWJ-joined emoji, the
/// second regional indicator of a flag, a combining mark, a variation selector,
/// etc. — to the last emitted text glyph, upgrading it to a `Composite` so the
/// renderer shapes the whole cluster as one unit. Falls back to a standalone
/// glyph when there is no mergeable base.
#[cfg(test)]
pub(crate) fn push_cluster_continuation_to_row(
    row: &mut GlyphRow,
    ch: char,
    face_id: FaceId,
    charpos: usize,
) {
    push_cluster_continuation_to_area(
        row,
        GlyphArea::Text.index(),
        ch,
        face_id,
        GlyphProvenance::buffer(charpos),
    );
}

pub(crate) fn push_cluster_continuation_to_area(
    row: &mut GlyphRow,
    area_index: usize,
    ch: char,
    face_id: FaceId,
    provenance: GlyphProvenance,
) {
    let area = &mut row.glyphs[area_index];
    if let Some(last) = area.last_mut()
        && is_run_member_padding(last)
    {
        extend_glyph_grapheme(last, ch);
    }
    if merge_extender_into_last_glyph(area, ch) {
        return;
    }
    area.push(Glyph::char_with_provenance(ch, face_id, provenance));
    mark_displays_text_if_text_area(row, area_index);
}

/// Grow a contextual-shaping run by appending `ch` to the last text glyph's
/// composed cluster and pushing a padding cell carrying `ch`'s own buffer
/// position.
#[cfg(test)]
pub(crate) fn push_run_member_to_row(
    row: &mut GlyphRow,
    ch: char,
    face_id: FaceId,
    charpos: usize,
    pixel_width: f32,
) {
    push_run_member_to_area(
        row,
        GlyphArea::Text.index(),
        ch,
        face_id,
        GlyphProvenance::buffer(charpos),
        pixel_width,
    );
}

pub(crate) fn push_run_member_to_area(
    row: &mut GlyphRow,
    area_index: usize,
    ch: char,
    face_id: FaceId,
    provenance: GlyphProvenance,
    pixel_width: f32,
) {
    let area = &mut row.glyphs[area_index];
    if merge_extender_into_last_glyph(area, ch) {
        let member_width = if pixel_width.is_finite() && pixel_width > 0.0 {
            pixel_width
        } else {
            0.0
        };
        if let Some(base) = area.iter_mut().rev().find(|g| !g.padding) {
            base.pixel_width += member_width;
        }
        let mut pad = Glyph::padding_with_provenance(face_id, provenance);
        pad.glyph_type = GlyphType::Char { ch };
        pad.pixel_width = member_width;
        area.push(pad);
        return;
    }
    area.push(Glyph::char_with_provenance(ch, face_id, provenance).with_pixel_width(pixel_width));
    mark_displays_text_if_text_area(row, area_index);
}

#[cfg(test)]
pub(crate) fn push_stretch_to_row(
    row: &mut GlyphRow,
    width_cols: u16,
    face_id: FaceId,
    pixel_width: f32,
    pixel_height: f32,
    pixel_ascent: f32,
    charpos: usize,
) {
    push_stretch_to_area(
        row,
        GlyphArea::Text.index(),
        width_cols,
        face_id,
        pixel_width,
        pixel_height,
        pixel_ascent,
        GlyphProvenance::buffer(charpos),
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn push_stretch_to_area(
    row: &mut GlyphRow,
    area_index: usize,
    width_cols: u16,
    face_id: FaceId,
    pixel_width: f32,
    pixel_height: f32,
    pixel_ascent: f32,
    provenance: GlyphProvenance,
) {
    let glyph = Glyph::stretch_with_provenance(width_cols, face_id, provenance)
        .with_pixel_geometry(pixel_width, pixel_height, pixel_ascent);
    row.glyphs[area_index].push(glyph);
    mark_displays_text_if_text_area(row, area_index);
}

/// Normalize a standalone row built outside the window-matrix walker.
///
/// This keeps the non-reorder bookkeeping (currently `displays_text`) that a
/// freshly built logical-order row needs. Bidi reordering is deliberately NOT
/// done here: it happens exactly once, at row install, via `reorder_row_bidi`
/// (see `display_row_finalizer`). GNU likewise commits a row in visual order a
/// single time when the row is appended to the matrix (src/xdisp.c).
pub(crate) fn normalize_external_row(row: &mut GlyphRow) {
    row.displays_text = !row.glyphs[GlyphArea::Text.index()].is_empty();
}

/// True once a row has already been reordered into visual order. Used only by a
/// debug assertion guarding against a second reorder; reordering is no longer
/// idempotent-by-early-return — every row reorders exactly once at install.
/// (Reachability of the install reorder is enforced at the lifecycle level via
/// `OutputWindowRowGrid::finalized_rows`.)
fn has_bidi_finalization(row: &GlyphRow) -> bool {
    row.reversed_p
        || row.glyphs[GlyphArea::Text.index()]
            .iter()
            .any(|glyph| glyph.bidi_level != 0)
}

pub(crate) fn reorder_row_bidi(row: &mut GlyphRow, phys_cursor_col: Option<u16>) -> Option<u16> {
    debug_assert!(
        !has_bidi_finalization(row),
        "reorder_row_bidi ran twice on the same row: rows must reorder exactly \
         once at install (reversed_p={}, has bidi levels)",
        row.reversed_p,
    );

    let original_text = row.glyphs[GlyphArea::Text.index()].clone();
    if original_text.is_empty() {
        return None;
    }

    let units = collect_bidi_units(&original_text);
    if units.is_empty() {
        return None;
    }

    // Fast path: when every representative char is below U+0590 (the first RTL
    // block) the row has no R/AL/AN and no explicit bidi controls, so UAX#9
    // resolves all chars to even (LTR) levels with paragraph base 0 — visual
    // order is the identity and the row is not reversed. Skip BOTH resolver
    // passes (resolve_levels + paragraph_base_level), which otherwise run per row
    // on EVERY redisplay even for pure-ASCII/code buffers (the common case, and
    // the rank-1 redisplay-layout cost center). Soundness: I1 bumps European
    // digits to level 2, but 2 is even — every consumer tests `bidi_level & 1`,
    // and an all-even row has no odd run for rule L2 to reverse, so the result is
    // observably identical to a full resolve.
    let all_ltr = !units.iter().any(|unit| unit.ch >= '\u{0590}');
    let (levels, reversed_p) = if all_ltr {
        (vec![0u8; units.len()], false)
    } else {
        let chars: String = units.iter().map(|unit| unit.ch).collect();
        let levels = bidi::resolve_levels(&chars, BidiDir::Auto);
        // GNU marks a row whose paragraph base direction is right-to-left as
        // `reversed_p` and displays it flush to the right margin (src/xdisp.c).
        // Row materialization reads the same flag to offset glyphs to the right
        // edge. Determined from logical-order representative chars.
        let reversed_p = bidi::paragraph_base_level(&chars, BidiDir::Auto) & 1 == 1;
        (levels, reversed_p)
    };
    if levels.len() != units.len() {
        return None;
    }
    row.reversed_p = reversed_p;

    let cursor_logical_idx = row.cursor_col.and_then(|col| {
        units
            .iter()
            .position(|unit| unit.cols.contains(&(col as usize)))
    });
    let phys_cursor_logical_idx = phys_cursor_col.and_then(|col| {
        units
            .iter()
            .position(|unit| unit.cols.contains(&(col as usize)))
    });

    let visual_order = if levels.iter().all(|&level| level == 0) {
        (0..units.len()).collect()
    } else {
        bidi::reorder_visual(&levels)
    };

    let remapped_cursor = rewrite_units_into_row(
        row,
        &original_text,
        &units,
        &levels,
        &visual_order,
        cursor_logical_idx,
        phys_cursor_logical_idx,
    );
    restamp_visual_box_runs(row, &units, &levels, &visual_order);
    remapped_cursor
}
