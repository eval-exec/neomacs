// External child module of glyph_row_writer. Source-only; applies to v1 or v2.
// The legacy control below is copied literally from frozen ebed source, with
// identifiers renamed. This proves native preservation, not GNU expectations.
use super::*;
use neomacs_display_protocol::face::BoxVerticalEdges;
use neomacs_display_protocol::glyph_matrix::{
    FringeBitmapInfo, GlyphPointerAppearance, GlyphPointerOccurrenceIdentity,
    GlyphPointerSourceIdentity, GlyphPointerSourceKind, GlyphStringBufferRange, GlyphStringId,
    GlyphStringSource, GlyphlessPresentation, TerminalComposition, TerminalCompositionCell,
};
use neomacs_display_protocol::{CursorStyle, GlyphRowRole};

#[derive(Clone, Copy)]
struct LegacyBidiGlyphUnit {
    ch: char,
    start: usize,
    len: usize,
    edges: neomacs_display_protocol::face::BoxVerticalEdges,
}

impl LegacyBidiGlyphUnit {
    fn cols(&self) -> std::ops::Range<usize> {
        self.start..self.start + self.len
    }
}

/// Recompute physical terminals from visual adjacency after UAX#9 reordering.
/// Swapping each odd glyph independently is insufficient: reordering can put
/// boxed glyphs from different logical runs next to one another, or separate
/// logical neighbors. GNU decides the physical terminals from visual
/// boxed/unboxed adjacency while preserving independently open viewport/row
/// boundaries. The explicit membership bit makes `Neither` unambiguous.
fn legacy_restamp_visual_box_runs(
    row: &mut GlyphRow,
    units: &[LegacyBidiGlyphUnit],
    levels: &[u8],
    visual_order: &[usize],
) {
    let available_cols = units
        .iter()
        .flat_map(LegacyBidiGlyphUnit::cols)
        .collect::<Vec<_>>();
    let mut next_col = 0usize;
    let text = &mut row.glyphs[GlyphArea::Text.index()];
    for (visual_index, &logical_index) in visual_order.iter().enumerate() {
        let unit_len = units[logical_index].len;
        let Some(&target_start) = available_cols.get(next_col) else {
            return;
        };
        let membership = units[logical_index].edges.membership();
        for glyph in &mut text[target_start..target_start + units[logical_index].len] {
            glyph.box_vertical_edges = neomacs_display_protocol::face::BoxVerticalEdges::Neither
                .with_membership(membership);
        }

        if !membership.is_boxed() {
            next_col = next_col.saturating_add(unit_len);
            continue;
        }
        let source_edges = units[logical_index].edges;
        let physical_edges = if levels[logical_index] & 1 == 1 {
            source_edges.reversed()
        } else {
            source_edges
        };
        let previous_boxed = visual_index
            .checked_sub(1)
            .is_some_and(|index| units[visual_order[index]].edges.membership().is_boxed());
        let next_boxed = visual_order
            .get(visual_index + 1)
            .is_some_and(|&index| units[index].edges.membership().is_boxed());
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
        if let Some(glyph) = text[target_start..target_start + units[logical_index].len]
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

fn legacy_bidi_char_for_glyph(glyph: &Glyph) -> Option<char> {
    if glyph.padding {
        return None;
    }

    match &glyph.glyph_type {
        GlyphType::Char { ch } | GlyphType::Glyphless { ch, .. } => Some(*ch),
        GlyphType::Composite { text } | GlyphType::AutomaticComposite { text, .. } => {
            text.chars().next()
        }
        GlyphType::Stretch { .. } => Some(' '),
        GlyphType::Image { .. }
        | GlyphType::Video { .. }
        | GlyphType::Xwidget { .. }
        | GlyphType::Surface { .. } => None,
    }
}

fn legacy_apply_bidi_mirroring(glyph: &mut Glyph, level: u8) {
    if level & 1 == 0 {
        return;
    }

    match &mut glyph.glyph_type {
        GlyphType::Char { ch } | GlyphType::Glyphless { ch, .. } => {
            if let Some(mirrored) = bidi::bidi_mirror(*ch) {
                *ch = mirrored;
            }
        }
        GlyphType::Composite { .. }
        | GlyphType::AutomaticComposite { .. }
        | GlyphType::Stretch { .. }
        | GlyphType::Image { .. }
        | GlyphType::Video { .. }
        | GlyphType::Xwidget { .. }
        | GlyphType::Surface { .. } => {}
    }
}

fn legacy_collect_bidi_units(text: &[Glyph]) -> Vec<LegacyBidiGlyphUnit> {
    let mut units = Vec::with_capacity(text.len());
    let mut idx = 0;
    while idx < text.len() {
        let glyph = &text[idx];
        let Some(ch) = legacy_bidi_char_for_glyph(glyph) else {
            idx += 1;
            continue;
        };
        let start = idx;
        let edges = glyph.box_vertical_edges;
        idx += 1;
        // A base and its trailing padding remain one contiguous unit.
        while idx < text.len() && text[idx].padding {
            idx += 1;
        }
        units.push(LegacyBidiGlyphUnit {
            ch,
            start,
            len: idx - start,
            edges,
        });
    }
    units
}

fn legacy_rewrite_units_into_row(
    row: &mut GlyphRow,
    units: &[LegacyBidiGlyphUnit],
    levels: &[u8],
    visual_order: &[usize],
    cursor_logical_idx: Option<usize>,
    phys_cursor_logical_idx: Option<usize>,
) -> Option<u16> {
    let available_cols: Vec<usize> = units.iter().flat_map(LegacyBidiGlyphUnit::cols).collect();
    let text = &mut row.glyphs[GlyphArea::Text.index()];
    // Map each source slot to its destination. Excluded media/orphan padding
    // remain fixed. Validate every destination before changing any glyph.
    let mut destinations: Vec<usize> = (0..text.len()).collect();
    let mut next_col = 0usize;
    let mut visual_cursor_col = None;
    let mut remapped_phys_cursor_col = None;
    for &logical_idx in visual_order {
        let unit = &units[logical_idx];
        let target_cols = available_cols.get(next_col..next_col + unit.len)?;
        if !target_cols.windows(2).all(|w| w[1] == w[0] + 1) {
            return None;
        }
        let target_start = target_cols[0];
        for (offset, source) in unit.cols().enumerate() {
            destinations[source] = target_start + offset;
        }
        if cursor_logical_idx == Some(logical_idx) {
            visual_cursor_col = Some(target_start as u16);
        }
        if phys_cursor_logical_idx == Some(logical_idx) {
            remapped_phys_cursor_col = Some(target_start as u16);
        }
        next_col += unit.len;
    }
    for (unit, &level) in units.iter().zip(levels) {
        for glyph in &mut text[unit.cols()] {
            glyph.bidi_level = level;
        }
        legacy_apply_bidi_mirroring(&mut text[unit.start], level);
    }
    // Cycle swaps move owned payloads and sidecar tokens; no Glyph or string
    // is cloned, and the row keeps its existing allocation.
    for index in 0..destinations.len() {
        while destinations[index] != index {
            let target = destinations[index];
            text.swap(index, target);
            destinations.swap(index, target);
        }
    }
    if let Some(col) = visual_cursor_col {
        row.cursor_col = Some(col);
    }
    remapped_phys_cursor_col
}

fn legacy_has_bidi_finalization(row: &GlyphRow) -> bool {
    row.reversed_p
        || row.glyphs[GlyphArea::Text.index()]
            .iter()
            .any(|glyph| glyph.bidi_level != 0)
}

pub(crate) fn legacy_reorder_row_bidi(
    row: &mut GlyphRow,
    phys_cursor_col: Option<u16>,
) -> Option<u16> {
    debug_assert!(
        !legacy_has_bidi_finalization(row),
        "legacy_reorder_row_bidi ran twice on the same row: rows must reorder exactly \
         once at install (reversed_p={}, has bidi levels)",
        row.reversed_p,
    );

    let text = &row.glyphs[GlyphArea::Text.index()];
    if text.is_empty() {
        return None;
    }

    let units = legacy_collect_bidi_units(text);
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
            .position(|unit| unit.cols().contains(&(col as usize)))
    });
    let phys_cursor_logical_idx = phys_cursor_col.and_then(|col| {
        units
            .iter()
            .position(|unit| unit.cols().contains(&(col as usize)))
    });

    let visual_order = if levels.iter().all(|&level| level == 0) {
        (0..units.len()).collect()
    } else {
        bidi::reorder_visual(&levels)
    };

    let remapped_cursor = legacy_rewrite_units_into_row(
        row,
        &units,
        &levels,
        &visual_order,
        cursor_logical_idx,
        phys_cursor_logical_idx,
    );
    legacy_restamp_visual_box_runs(row, &units, &levels, &visual_order);
    remapped_cursor
}

fn source_row(text: &str) -> GlyphRow {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text.index()] = text
        .chars()
        .enumerate()
        .map(|(index, ch)| Glyph::char(ch, FaceId::new(20 + index as u32), 100 + index))
        .collect();
    decorate(&mut row);
    row
}

fn decorate(row: &mut GlyphRow) {
    row.hash = 0xabcdef;
    row.enabled = false;
    row.cursor_type = Some(CursorStyle::Hbar(3.0));
    row.truncated_left = true;
    row.continued = true;
    row.displays_text = true;
    row.ends_at_zv = true;
    row.mode_line = true;
    row.pixel_x = 13.5;
    row.start_col = 3;
    row.pixel_y = 27.0;
    row.height_px = 19.0;
    row.line_spacing_px = 2.0;
    row.ascent_px = 13.0;
    row.start_charpos = 100;
    row.end_charpos = 900;
    row.left_fringe_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 3,
        face_id: FaceId::new(91),
    });
    row.right_fringe_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 5,
        face_id: FaceId::new(92),
    });
    row.overlay_arrow_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 7,
        face_id: FaceId::new(93),
    });
    let pointer = row
        .intern_pointer_appearance(GlyphPointerAppearance {
            source: GlyphPointerSourceIdentity {
                kind: GlyphPointerSourceKind::Buffer,
                source_id: 19,
                range_start: 100,
                range_end: 900,
                property_owner: 23,
                occurrence: GlyphPointerOccurrenceIdentity::Source,
            },
            face_id: FaceId::new(99),
        })
        .expect("small native pointer side table");
    let string = row
        .push_string_source(GlyphStringSource::replacement(
            GlyphStringId::new(123),
            GlyphStringBufferRange::new(101, 104),
        ))
        .expect("small native string side table");
    row.intern_image_margins(neomacs_display_protocol::ImageMargins::asymmetric(
        1.0, 2.0, 3.0, 4.0,
    ))
    .expect("small native margin side table");
    for (index, glyph) in row.glyphs[GlyphArea::Text.index()].iter_mut().enumerate() {
        glyph.pixel_width = 8.0 + index as f32 / 4.0;
        glyph.pixel_height = 15.0 + index as f32 / 8.0;
        glyph.pixel_ascent = 11.0;
        glyph.vertical_offset_px = -2.0;
        glyph.pointer_appearance = Some(pointer);
        if index == 0 {
            glyph.provenance = GlyphProvenance::string(string, 4);
        }
    }
    // Margins and preexisting pointer runs must not be touched by row bidi.
    let mut left = Glyph::char('L', FaceId::new(201), 1);
    left.bidi_level = 3;
    left.pointer_appearance = Some(pointer);
    row.glyphs[GlyphArea::LeftMargin.index()].push(left);
    row.glyphs[GlyphArea::RightMargin.index()].push(Glyph::char('R', FaceId::new(202), 2));
    row.rebuild_pointer_runs(8.0, 400.0);
}

fn assert_legacy_row(input: &GlyphRow, physical: Option<u16>) -> (GlyphRow, Option<u16>) {
    let untouched = input.clone();
    let source_identity = input.appearance_id();
    let mut expected = input.clone();
    let mut actual = input.clone();
    let allocation = actual.glyphs.each_ref().map(|area| area.as_ptr());
    let appearance = actual.appearance_id();
    let sidecar_allocations = (
        actual.pointer_appearances().as_ptr(),
        actual.image_margins_table().as_ptr(),
        actual.string_sources().as_ptr(),
        actual.pointer_runs().as_ptr(),
    );
    let payloads: Vec<_> = actual
        .glyphs
        .iter()
        .flatten()
        .filter_map(|glyph| match &glyph.glyph_type {
            GlyphType::Composite { text } => Some((glyph.provenance, text.as_ptr(), None)),
            GlyphType::AutomaticComposite { text, terminal } => Some((
                glyph.provenance,
                text.as_ptr(),
                Some(std::sync::Arc::as_ptr(terminal)),
            )),
            _ => None,
        })
        .collect();
    let expected_physical = legacy_reorder_row_bidi(&mut expected, physical);
    let actual_physical = reorder_row_bidi(&mut actual, physical);
    assert_eq!(
        actual_physical, expected_physical,
        "physical cursor differs"
    );
    // GlyphRow's derived PartialEq compares every stored content field,
    // including private side tables/runs. RowAppearance deliberately compares
    // equal as content; its concrete identity is checked independently below.
    assert_eq!(
        actual, expected,
        "complete row differs from literal pre-change control"
    );
    assert_eq!(actual.appearance_id(), appearance);
    assert_eq!(
        actual.glyphs.each_ref().map(|area| area.as_ptr()),
        allocation
    );
    assert_eq!(
        (
            actual.pointer_appearances().as_ptr(),
            actual.image_margins_table().as_ptr(),
            actual.string_sources().as_ptr(),
            actual.pointer_runs().as_ptr(),
        ),
        sidecar_allocations
    );
    for (source, text_pointer, terminal_pointer) in payloads {
        assert!(
            actual.glyphs.iter().flatten().any(|glyph| {
                if glyph.provenance != source {
                    return false;
                }
                match &glyph.glyph_type {
                    GlyphType::Composite { text } => {
                        terminal_pointer.is_none() && text.as_ptr() == text_pointer
                    }
                    GlyphType::AutomaticComposite { text, terminal } => {
                        text.as_ptr() == text_pointer
                            && terminal_pointer == Some(std::sync::Arc::as_ptr(terminal))
                    }
                    _ => false,
                }
            }),
            "owned text/terminal payload was copied or lost"
        );
    }
    assert_eq!(input, &untouched, "source row changed");
    assert_eq!(input.appearance_id(), source_identity);
    (actual, actual_physical)
}

#[test]
fn bidi_identity_matches_literal_legacy_for_existing_ltr_certificate() {
    for text in [
        "a",
        "ASCII 123 ()[] +-$,./",
        "é λ \u{058f}",
        "\0\t\r \u{0301}",
    ] {
        let mut row = source_row(text);
        assert!(
            LtrBidiUnits::certify(&collect_bidi_units(&row.glyphs[GlyphArea::Text.index()]))
                .is_some()
        );
        for col in [None, Some(0), Some(1), Some(9), Some(u16::MAX)] {
            row.cursor_col = col;
            for physical in [None, Some(0), Some(1), Some(9), Some(u16::MAX)] {
                assert_legacy_row(&row, physical);
            }
        }
    }
}

#[test]
fn bidi_identity_snaps_both_cursors_inside_padded_units() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let mut base = Glyph::char('a', FaceId::new(1), 100);
    base.wide = true;
    let mut stretch = Glyph::char(' ', FaceId::new(2), 104);
    stretch.glyph_type = GlyphType::Stretch { width_cols: 3 };
    row.glyphs[GlyphArea::Text.index()] = vec![
        base,
        Glyph::padding_for(FaceId::new(1), 101),
        Glyph::padding_for(FaceId::new(1), 102),
        Glyph::char('b', FaceId::new(3), 103),
        stretch,
        Glyph::padding_for(FaceId::new(2), 105),
        Glyph::padding_for(FaceId::new(2), 106),
    ];
    decorate(&mut row);
    for (col, base_col) in [(0, 0), (1, 0), (2, 0), (3, 3), (4, 4), (5, 4), (6, 4)] {
        row.cursor_col = Some(col);
        let (actual, physical) = assert_legacy_row(&row, Some(col));
        assert_eq!(actual.cursor_col, Some(base_col));
        assert_eq!(physical, Some(base_col));
    }
}

#[test]
fn bidi_identity_preserves_media_gaps_orphan_padding_and_empty_composite() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let mut media = Glyph::char('?', FaceId::new(1), 101);
    let margins = row
        .intern_image_margins(neomacs_display_protocol::ImageMargins::new(2.0, 3.0))
        .expect("native image margin fixture");
    media.glyph_type = GlyphType::Image {
        image_id: 31,
        width_cols: 2,
        source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
        margins,
        opaque_background: neomacs_display_protocol::ImageOpaqueBackground::new(Some(0x123456)),
        presentation: neomacs_display_protocol::ImageGlyphPresentation::Texture,
    };
    let mut empty = Glyph::char('?', FaceId::new(2), 103);
    empty.glyph_type = GlyphType::Composite { text: "".into() };
    row.glyphs[GlyphArea::Text.index()] = vec![
        Glyph::padding_for(FaceId::new(1), 100),
        media,
        Glyph::padding_for(FaceId::new(1), 102),
        empty,
        Glyph::padding_for(FaceId::new(2), 104),
        Glyph::char('a', FaceId::new(3), 105),
        Glyph::padding_for(FaceId::new(3), 106),
        Glyph::char('b', FaceId::new(4), 107),
    ];
    decorate(&mut row);
    for col in [
        None,
        Some(0),
        Some(1),
        Some(2),
        Some(3),
        Some(4),
        Some(5),
        Some(6),
        Some(7),
        Some(8),
    ] {
        row.cursor_col = col;
        let (actual, physical) = assert_legacy_row(&row, col);
        if matches!(col, Some(0..=4) | Some(8)) {
            assert_eq!(
                actual.cursor_col, col,
                "unaddressable row cursor remains unchanged"
            );
            assert_eq!(physical, None);
        }
    }
}

#[test]
fn bidi_identity_restamps_box_adjacency_and_open_boundaries() {
    for mask in 0u8..16 {
        for boundary in [
            BoxVerticalEdges::Neither,
            BoxVerticalEdges::Left,
            BoxVerticalEdges::Right,
            BoxVerticalEdges::Both,
        ] {
            let mut row = source_row("abcd");
            for (index, glyph) in row.glyphs[GlyphArea::Text.index()].iter_mut().enumerate() {
                glyph.box_vertical_edges = if mask & (1 << index) == 0 {
                    BoxVerticalEdges::Unboxed
                } else {
                    boundary
                };
            }
            row.cursor_col = Some(2);
            assert_legacy_row(&row, Some(1));
        }
    }
    // An actual source edge must change even though the permutation is identity.
    let mut row = source_row("abc");
    for glyph in &mut row.glyphs[GlyphArea::Text.index()] {
        glyph.box_vertical_edges = BoxVerticalEdges::Both;
    }
    let (actual, _) = assert_legacy_row(&row, None);
    assert_eq!(
        actual.glyphs[GlyphArea::Text.index()][1].box_vertical_edges,
        BoxVerticalEdges::Neither
    );
}

#[test]
fn bidi_identity_preserves_owned_compositions_and_representative_convention() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let mut composite = Glyph::char('a', FaceId::new(1), 100);
    // Native preservation of the EXISTING first-character representative
    // rule, not a claim that this synthetic mixed cluster matches GNU bidi.
    composite.glyph_type = GlyphType::Composite {
        text: "aאב".into()
    };
    let mut automatic = Glyph::char('e', FaceId::new(2), 103);
    automatic.glyph_type = GlyphType::AutomaticComposite {
        text: "e\u{0301}".into(),
        terminal: std::sync::Arc::new(TerminalComposition {
            cells: vec![TerminalCompositionCell {
                base: 'e',
                extenders: "\u{0301}".into(),
                width_cols: 1,
                source_char_len: 2,
            }]
            .into_boxed_slice(),
            width_cols: 1,
        }),
    };
    let mut glyphless = Glyph::char('\u{058f}', FaceId::new(3), 105);
    glyphless.glyph_type = GlyphType::Glyphless {
        ch: '\u{058f}',
        presentation: GlyphlessPresentation::HexCode,
    };
    row.glyphs[GlyphArea::Text.index()] = vec![
        composite,
        Glyph::padding_for(FaceId::new(1), 101),
        Glyph::padding_for(FaceId::new(1), 102),
        automatic,
        Glyph::padding_for(FaceId::new(2), 104),
        glyphless,
    ];
    decorate(&mut row);
    assert!(
        LtrBidiUnits::certify(&collect_bidi_units(&row.glyphs[GlyphArea::Text.index()])).is_some()
    );
    row.cursor_col = Some(4);
    assert_legacy_row(&row, Some(2));
}

#[test]
fn bidi_identity_refuses_other_unicode_without_changing_resolved_output() {
    for text in [
        "\u{0590}",
        "אבג",
        "abc אב 12",
        "العربية",
        "中",
        "a\u{202e}bc\u{202c}",
        "a\u{2067}אב\u{2069}",
    ] {
        let mut row = source_row(text);
        let before = row.clone();
        assert!(
            LtrBidiUnits::certify(&collect_bidi_units(&row.glyphs[GlyphArea::Text.index()]))
                .is_none()
        );
        assert_eq!(row, before, "refusal mutated the source row");
        for col in [None, Some(0), Some(1), Some(9)] {
            row.cursor_col = col;
            assert_legacy_row(&row, col);
        }
    }
}

#[test]
fn bidi_identity_preserves_empty_and_excluded_only_rows() {
    for excluded in [false, true] {
        let mut row = GlyphRow::new(GlyphRowRole::Text);
        if excluded {
            let mut media = Glyph::char('?', FaceId::new(1), 100);
            media.glyph_type = GlyphType::Surface {
                surface_id: 49,
                width_cols: 1,
            };
            row.glyphs[GlyphArea::Text.index()] =
                vec![media, Glyph::padding_for(FaceId::new(1), 101)];
        }
        decorate(&mut row);
        row.cursor_col = Some(1);
        let (actual, physical) = assert_legacy_row(&row, Some(1));
        assert_eq!(physical, None);
        assert_eq!(actual.cursor_col, Some(1));
    }
}

#[test]
fn bidi_identity_preserves_long_owned_rows() {
    for size in [50, 1000] {
        let mut row = source_row(&"a9 é ".repeat(size));
        row.cursor_col = Some(49);
        assert_legacy_row(&row, Some(51));
    }
}
