use super::*;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neovm_core::window::{WindowDisplaySnapshot, WindowId};

fn synthetic_key(window_start: i64, point: i64) -> RetainedWindowKey {
    RetainedWindowKey {
        prefixes: Default::default(),
        invisibility: Default::default(),
        char_table_revision: Default::default(),
        display_table: Default::default(),
        media_generation: 0,
        buffer_id: 1,
        window_start,
        point,
        display_line_numbers: DisplayLineNumbersMode::Off,
        buffer_begv: 0,
        buffer_size: 1000,
        partition: WindowPartitionSignature::from_regions(
            neovm_core::window::PresentedWindowRegions {
                outer: Rect::new(0.0, 0.0, 800.0, 616.0),
                text_body: Rect::new(0.0, 0.0, 800.0, 600.0),
                mode_line: Some(Rect::new(0.0, 600.0, 800.0, 16.0)),
                ..Default::default()
            },
        ),
        hscroll: 0,
        vscroll: 0,
        wrap_mode: LineWrapMode::Truncate,
        word_wrap: false,
        tab_width: 8,
        char_width: 8.0,
        char_height: 16.0,
        font_pixel_size: 16.0,
        tab_stop_list: Vec::new(),
        extra_line_spacing: 0.0,
        selective_display: 0,
        selected: true,
        cursor_role: crate::types::WindowCursorRole::Active,
        show_trailing_whitespace: false,
        trailing_ws_bg: 0,
        nobreak_char_display: NobreakDisplayMode::Literal,
        glyphless_char_fg: 0,
        indicate_empty_lines: 0,
        line_prefix: Vec::new(),
        wrap_prefix: Vec::new(),
        is_multibyte: true,
        chars_modified_tick: 5,
        props_modified_tick: 5,
        overlay_modified_tick: 5,
        overlay_digest: 0x51_9e_5a_11,
        face_change_count: 5,
        display_var_change_count: 5,
    }
}

/// A retained matrix with `n_body` 16px Text rows (10 chars each, the first
/// starting at `base`) plus a mode-line row.
fn synthetic_matrix(base: i64, n_body: usize) -> RetainedWindowMatrix {
    let mut matrix = GlyphMatrix::new(n_body + 1, 100);
    for i in 0..n_body {
        let row = MatrixRow::make_mut(&mut matrix.rows[i]);
        row.enabled = true;
        row.role = GlyphRowRole::Text;
        row.displays_text = true;
        row.start_charpos = (base + (i as i64) * 10) as usize;
        row.end_charpos = (base + (i as i64) * 10 + 9) as usize;
        row.pixel_y = (i as f32) * 16.0;
        row.height_px = 16.0;
        row.ascent_px = 16.0;
    }
    let ml = MatrixRow::make_mut(&mut matrix.rows[n_body]);
    ml.enabled = true;
    ml.role = GlyphRowRole::ModeLine;
    ml.mode_line = true;
    ml.pixel_y = (n_body as f32) * 16.0;
    ml.height_px = 16.0;
    RetainedWindowMatrix {
        matrix,
        key: synthetic_key(base, 0),
        validity: MatrixValidity::Valid,
        display_snapshot: WindowDisplaySnapshot {
            window_id: WindowId(1),
            text_area_left_offset: 0,
            mode_line_height: 16,
            header_line_height: 0,
            tab_line_height: 0,
            logical_cursor: None,
            phys_cursor: None,
            points: Vec::new(),
            rows: Vec::new(),
            ..WindowDisplaySnapshot::default()
        },
        presented_cursor: None,
        face_generation: FrameFaceGeneration::default(),
        chrome_uses_column: false,
        chrome_modified_flag: false,
        chrome_fingerprints: None,
    }
}

/// A window nothing touched reuses its body verbatim even when its point
/// sits on the LAST visible row with more buffer below it. That geometry
/// is a scroll hazard only for a point MOVE -- the full pass that produced
/// this matrix already made whatever scroll decision these inputs call
/// for. Ungated, this rejected 200 consecutive frames of the
/// rust-lsp-typing fixture with `differing=[]`, rebuilding a 7-row window
/// in full every time for a window nothing had touched.
#[test]
fn cursor_only_reuses_an_untouched_window_whose_point_sits_on_the_last_row() {
    let mut m = synthetic_matrix(0, 5); // rows span 0-9, 10-19, ... 40-49
    // Point on the last body row, and the buffer does NOT end there.
    m.key.point = 45;
    for row in m.matrix.rows.iter_mut() {
        MatrixRow::make_mut(row).ends_at_zv = false;
    }
    let unchanged = m.key.clone();
    assert_eq!(
        m.key.differing_fields(&unchanged),
        Vec::<&str>::new(),
        "the fixture must present an IDENTICAL key, or it pins nothing"
    );
    assert!(
        m.cursor_only_replay(&unchanged).is_ok(),
        "an untouched window must reuse its body verbatim"
    );

    // The same geometry with point MOVED onto that row keeps declining:
    // a full pass might answer the move by scrolling.
    let mut moved = m.key.clone();
    moved.point = 46;
    assert_eq!(
        m.cursor_only_replay(&moved).err(),
        Some(CursorOnlyDecline::PointMoveMayScrollDown),
        "a point move onto the last row with buffer below still declines"
    );
}

#[test]
fn scroll_replay_detects_whole_row_scroll_down() {
    let m = synthetic_matrix(0, 5); // rows start at 0,10,20,30,40
    let curr = synthetic_key(20, 25); // scrolled to row 2, point followed
    let r = m
        .scroll_replay(&curr)
        .expect("whole-row scroll-down is eligible");
    assert_eq!(r.dvpos, -32.0, "removed two 16px rows");
    // Rows [2,3,4] reused into matrix indices [0,1,2] with shifted pixel_y.
    assert_eq!(
        r.reused_rows.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(r.reused_rows[0].1.pixel_y, 0.0);
    assert_eq!(r.reused_rows[2].1.pixel_y, 32.0);
    assert_eq!(
        r.exposed_row_count, 2,
        "two newly-exposed rows at the bottom"
    );
    assert_eq!(r.exposed_row_base, 3);
    assert_eq!(r.walk_start.get(), 50); // after row 4 (chars 40..49)
    assert_eq!(r.exposed_text_y, 48.0); // 3rd visual row top
    assert_eq!(r.new_point, 25);
}

#[test]
fn replay_face_references_come_from_the_rows_being_reused() {
    use neomacs_display_protocol::glyph_matrix::Glyph;

    let mut retained = synthetic_matrix(0, 3);
    MatrixRow::make_mut(&mut retained.matrix.rows[0]).glyphs[GlyphArea::Text.index()]
        .push(Glyph::char('a', FaceId::new(27), 0));
    MatrixRow::make_mut(&mut retained.matrix.rows[1]).glyphs[GlyphArea::Text.index()]
        .push(Glyph::char('b', FaceId::new(31), 10));

    let replay = retained
        .cursor_only_replay(&synthetic_key(0, 15))
        .expect("pure point movement reuses retained body rows");
    assert_eq!(
        replay.retained_face_ids(),
        vec![FaceId::new(27), FaceId::new(31)]
    );
}

#[test]
fn scroll_replay_bails_on_partial_row_scroll() {
    let m = synthetic_matrix(0, 5);
    let curr = synthetic_key(15, 15); // not a row boundary
    assert!(m.scroll_replay(&curr).is_none());
}

#[test]
fn scroll_replay_bails_on_scroll_up() {
    let m = synthetic_matrix(20, 5); // rows start at 20,30,40,50,60
    let curr = synthetic_key(0, 5); // above the retained top
    assert!(m.scroll_replay(&curr).is_none());
}

#[test]
fn scroll_replay_bails_on_tick_change() {
    let m = synthetic_matrix(0, 5);
    let mut curr = synthetic_key(20, 25);
    curr.props_modified_tick += 1; // a text-property write co-occurred
    assert!(m.scroll_replay(&curr).is_none());
}

#[test]
fn scroll_replay_bails_on_vscroll() {
    let m = synthetic_matrix(0, 5);
    let mut curr = synthetic_key(20, 25);
    curr.vscroll = 1; // pixel-level scroll offset
    assert!(m.scroll_replay(&curr).is_none());
}

#[test]
fn scroll_replay_bails_when_unchanged() {
    let m = synthetic_matrix(0, 5);
    let curr = synthetic_key(0, 0); // window_start did not move
    assert!(m.scroll_replay(&curr).is_none());
}

/// One named change to a [`RetainedWindowKey`] under test.
type KeyMutation = (&'static str, fn(&mut RetainedWindowKey));

/// Adversarial-review fix: each layout-affecting input that does NOT bump a
/// modification tick (tab-stop-list, line-spacing, selective-display, window
/// selection, trailing-whitespace, special-char display, fringes, line/wrap
/// prefixes, multibyteness) must, when changed alone, force a FULL rebuild —
/// otherwise the fast paths would reuse rows shaped under the old setting.
#[test]
fn fast_paths_bail_when_a_non_tick_layout_param_changes() {
    let prev = synthetic_key(0, 0);
    // Baseline: a pure point move IS cursor-only.
    assert!(RetainedWindowKey::cursor_only_eligible(
        &prev,
        &synthetic_key(0, 7)
    ));

    let mutations: &[KeyMutation] = &[
        ("display_line_numbers", |k| {
            k.display_line_numbers = DisplayLineNumbersMode::Relative
        }),
        ("tab_stop_list", |k| k.tab_stop_list = vec![4, 12]),
        ("extra_line_spacing", |k| k.extra_line_spacing = 2.0),
        ("selective_display", |k| k.selective_display = 4),
        ("selected", |k| k.selected = false),
        ("show_trailing_whitespace", |k| {
            k.show_trailing_whitespace = true
        }),
        ("trailing_ws_bg", |k| k.trailing_ws_bg = 0x00ff_00ff),
        ("nobreak_char_display", |k| {
            k.nobreak_char_display = NobreakDisplayMode::HighlightOriginal
        }),
        ("glyphless_char_fg", |k| k.glyphless_char_fg = 0x00ff_ffff),
        ("indicate_empty_lines", |k| k.indicate_empty_lines = 1),
        ("left_fringe", |k| {
            k.partition.regions_mut().left_fringe = Some(Rect::new(0.0, 0.0, 8.0, 600.0))
        }),
        ("right_fringe", |k| {
            k.partition.regions_mut().right_fringe = Some(Rect::new(792.0, 0.0, 8.0, 600.0))
        }),
        ("left_margin", |k| {
            k.partition.regions_mut().left_margin = Some(Rect::new(0.0, 0.0, 16.0, 600.0))
        }),
        ("horizontal_scroll_bar", |k| {
            k.partition.regions_mut().horizontal_scroll_bar =
                Some(Rect::new(0.0, 592.0, 800.0, 8.0))
        }),
        ("text_body_origin", |k| {
            k.partition.regions_mut().text_body.x += 8.0
        }),
        ("line_prefix", |k| k.line_prefix = vec![b'>', b' ']),
        ("wrap_prefix", |k| k.wrap_prefix = vec![b' ', b' ']),
        ("is_multibyte", |k| k.is_multibyte = false),
    ];
    for (name, mutate) in mutations {
        // Cursor-only (point also moved): must bail.
        let mut curr = synthetic_key(0, 7);
        mutate(&mut curr);
        assert!(
            !RetainedWindowKey::cursor_only_eligible(&prev, &curr),
            "{name} change must block the cursor-only fast path"
        );
        // Scroll (window_start also moved): must bail.
        let mut scrolled = synthetic_key(20, 7);
        mutate(&mut scrolled);
        assert!(
            !RetainedWindowKey::scroll_eligible(&prev, &scrolled),
            "{name} change must block the scroll fast path"
        );
        // Edit (chars tick + buffer_size also moved): must bail.
        let mut edited = synthetic_key(0, 7);
        edited.chars_modified_tick = prev.chars_modified_tick + 1;
        edited.buffer_size = prev.buffer_size + 1;
        mutate(&mut edited);
        assert!(
            !RetainedWindowKey::edit_eligible(&prev, &edited),
            "{name} change must block the edit fast path"
        );
    }
}

/// Phase 3 below-reuse classifier: a simple insert into a monospace edited
/// row reuses the rows BELOW the edit (charpos shifted by the inserted count,
/// pixel_y unchanged) and bounds the walk to the edited line. Gated on
/// `allow_below_reuse`; with it off the plan is the above-only edit replay.
#[test]
fn edit_replay_below_reuse_shifts_rows_below_by_inserted_count() {
    use neomacs_display_protocol::glyph_matrix::{
        Glyph, GlyphProvenance, GlyphStringBufferRange, GlyphStringId, GlyphStringSource,
    };
    let mut m = synthetic_matrix(0, 5); // rows start at 0,10,20,30,40
    // Give the edited row (row 2, chars [20,29]) 10 monospace (8px) glyphs.
    for c in 0..10 {
        let mut g = Glyph::char('a', FaceId::new(0), (20 + c) as usize);
        g.pixel_width = 8.0;
        MatrixRow::make_mut(&mut m.matrix.rows[2]).glyphs[GlyphArea::Text.index()].push(g);
    }
    let row3 = MatrixRow::make_mut(&mut m.matrix.rows[3]);
    let string_source = row3
        .push_string_source(GlyphStringSource::replacement(
            GlyphStringId::new(7),
            GlyphStringBufferRange::new(30, 35),
        ))
        .expect("row-local string source");
    row3.glyphs[GlyphArea::Text.index()].push(
        Glyph::char('S', FaceId::new(0), 30)
            .with_provenance(GlyphProvenance::string(string_source, 4)),
    );
    // A 1-char insert at charpos 25 (inside row 2): chars tick moved, point +
    // buffer_size grew by 1, everything else equal.
    let mut curr = synthetic_key(0, 25);
    curr.chars_modified_tick = 6;
    curr.buffer_size = 1001;

    // allow_below_reuse = true → reuse above (0,1) AND below (3,4); below rows
    // are charpos-shifted by +1; the walk is bounded to the one edited row.
    let r = m
        .edit_replay(&curr, EditDamage::new(25, 26, 1, 0), true)
        .expect("below-reuse is eligible");
    assert!(r.bound_walk, "walk bounded to the edited line");
    assert_eq!(r.exposed_row_count, 1, "only the edited line is walked");
    assert_eq!(r.exposed_row_base, 2);
    assert_eq!(
        r.reused_rows.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![0, 1, 3, 4],
        "above (0,1) reused verbatim + below (3,4) reused shifted"
    );
    let below3 = r.reused_rows.iter().find(|(i, _)| *i == 3).unwrap();
    assert_eq!(below3.1.start_charpos, 31, "row 3 start 30 -> 31");
    assert_eq!(below3.1.end_charpos, 40, "row 3 end 39 -> 40");
    assert_eq!(
        below3.1.glyphs[GlyphArea::Text.index()][0].provenance,
        GlyphProvenance::string(string_source, 4),
        "reuse shifts the covered buffer range but never the string index"
    );
    assert_eq!(
        below3
            .1
            .string_source(string_source)
            .and_then(|source| source.covered_buffer_range()),
        Some(GlyphStringBufferRange::new(31, 36)),
        "reuse shifts the occurrence-wide range exactly once"
    );
    let below4 = r.reused_rows.iter().find(|(i, _)| *i == 4).unwrap();
    assert_eq!(below4.1.start_charpos, 41, "row 4 start 40 -> 41");

    // allow_below_reuse = false → the above-only edit replay (no below reuse,
    // walk runs to the bottom).
    let above_only = m
        .edit_replay(&curr, EditDamage::new(25, 26, 1, 0), false)
        .expect("above-only edit replay");
    assert!(!above_only.bound_walk);
    assert_eq!(
        above_only
            .reused_rows
            .iter()
            .map(|(i, _)| *i)
            .collect::<Vec<_>>(),
        vec![0, 1],
        "only the rows above the edit are reused"
    );
    assert_eq!(
        above_only.exposed_row_count, 3,
        "edited line + 2 rows below"
    );
}

/// An edit on the window's first row has no rows above to reuse, but the
/// rows below still reuse shifted; without below-reuse there is nothing
/// to replay and the plain rebuild owns the frame.
#[test]
fn edit_replay_on_the_first_row_reuses_the_rows_below() {
    use neomacs_display_protocol::glyph_matrix::Glyph;
    let mut m = synthetic_matrix(0, 5); // rows start at 0,10,20,30,40
    for c in 0..10 {
        let mut g = Glyph::char('a', FaceId::new(0), c as usize);
        g.pixel_width = 8.0;
        MatrixRow::make_mut(&mut m.matrix.rows[0]).glyphs[GlyphArea::Text.index()].push(g);
    }
    let mut curr = synthetic_key(0, 5);
    curr.chars_modified_tick = 6;
    curr.buffer_size = 1001;
    let r = m
        .edit_replay(&curr, EditDamage::new(5, 6, 1, 0), true)
        .expect("a first-row edit reuses the rows below");
    assert_eq!(r.exposed_row_base, 0, "the walk starts at the first row");
    assert_eq!(r.exposed_row_count, 1, "only the edited row is walked");
    assert!(r.bound_walk);
    assert_eq!(
        r.reused_rows.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "every row below the edit is reused shifted"
    );
    let below1 = r.reused_rows.iter().find(|(i, _)| *i == 1).unwrap();
    assert_eq!(below1.1.start_charpos, 11, "row 1 start 10 -> 11");
    assert!(
        m.edit_replay(&curr, EditDamage::new(5, 6, 1, 0), false)
            .is_none(),
        "with no row above and no below-reuse there is nothing to replay"
    );
}

/// A props-only refontification frame (font-lock after-the-fact pass, or
/// the props part of a keystroke): chars tick unchanged, props tick moved,
/// dirty span covering two rows. The span rows are relaid; the rows below
/// the span are reused UNSHIFTED (delta = 0); expected_walk carries the
/// span's continuity contract.
#[test]
fn edit_replay_props_only_span_relays_span_rows_and_reuses_below_unshifted() {
    use neomacs_display_protocol::glyph_matrix::Glyph;
    let mut m = synthetic_matrix(0, 5); // rows start at 0,10,20,30,40
    for row_idx in [2usize, 3] {
        let base = row_idx * 10;
        for c in 0..10 {
            let mut g = Glyph::char('a', FaceId::new(0), base + c);
            g.pixel_width = 8.0;
            MatrixRow::make_mut(&mut m.matrix.rows[row_idx]).glyphs[GlyphArea::Text.index()]
                .push(g);
        }
    }
    // Font-lock rewrote faces over chars [22, 35): props tick moved, size
    // unchanged.
    let mut curr = synthetic_key(0, 25);
    curr.props_modified_tick = 6;

    let r = m
        .edit_replay(&curr, EditDamage::new(22, 35, 0, 1), true)
        .expect("props-only span replay is eligible");
    assert!(r.bound_walk);
    assert_eq!(r.exposed_row_base, 2, "span starts at row 2 (chars 20..)");
    assert_eq!(r.exposed_row_count, 2, "rows 2 and 3 intersect [22,35)");
    assert_eq!(
        r.reused_rows.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![0, 1, 4],
        "above (0,1) verbatim + below-span (4) reused"
    );
    let below = r.reused_rows.iter().find(|(i, _)| *i == 4).unwrap();
    assert_eq!(below.1.start_charpos, 40, "delta 0: no shift");
    let expected = r.expected_walk.expect("bound walk carries the contract");
    assert_eq!(expected.row_count, 2);
    assert_eq!(
        expected.last_row_end_charpos, 39,
        "old row-3 end 39 + delta 0"
    );
    assert!((expected.total_height_px - 32.0).abs() < 0.01);
}

#[test]
fn edit_replay_face_change_at_row_start_invalidates_predecessor_box_terminal() {
    use neomacs_display_protocol::glyph_matrix::Glyph;

    let mut m = synthetic_matrix(0, 5); // old row ends: 9, 19, 29, 39, 49
    // Populate the two replayed span rows with fixed-width text so the
    // classifier can prove that unchanged rows below remain aligned.  The
    // assertion is then specifically about widening the property damage
    // to row 1 for row 2's source-face lookahead, not about the unrelated
    // conservative fallback for empty synthetic span rows.
    for row_idx in [1usize, 2] {
        let base = row_idx * 10;
        for charpos in base..base + 10 {
            let mut glyph = Glyph::char('a', FaceId::new(0), charpos);
            glyph.pixel_width = 8.0;
            MatrixRow::make_mut(&mut m.matrix.rows[row_idx]).glyphs[GlyphArea::Text.index()]
                .push(glyph);
        }
    }
    let mut curr = synthetic_key(0, 20);
    curr.props_modified_tick += 1;

    let replay = m
        .edit_replay(&curr, EditDamage::new(20, 21, 0, 0), true)
        .expect("property-only replay is eligible");

    assert_eq!(
        replay.exposed_row_base, 1,
        "row 1 owns the box terminal whose lookahead is source position 20"
    );
    assert_eq!(
        replay
            .reused_rows
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>(),
        vec![0, 3, 4],
        "only rows strictly before the predecessor dependency and below the damage reuse"
    );
}

/// An insert whose accumulated dirty span (edit + refontification) covers
/// two rows: both span rows are relaid, rows below the span shift by the
/// insert delta.
#[test]
fn edit_replay_insert_with_multi_row_span_shifts_only_below_span() {
    use neomacs_display_protocol::glyph_matrix::Glyph;
    let mut m = synthetic_matrix(0, 5);
    for row_idx in [2usize, 3] {
        let base = row_idx * 10;
        for c in 0..10 {
            let mut g = Glyph::char('a', FaceId::new(0), base + c);
            g.pixel_width = 8.0;
            MatrixRow::make_mut(&mut m.matrix.rows[row_idx]).glyphs[GlyphArea::Text.index()]
                .push(g);
        }
    }
    // Insert 1 char at 25, font-lock refontified [20, 36) (NEW coords).
    // Old-coordinate span end = 36 - 1 = 35.
    let mut curr = synthetic_key(0, 26);
    curr.chars_modified_tick = 6;
    curr.props_modified_tick = 7;
    curr.buffer_size = 1001;

    let r = m
        .edit_replay(&curr, EditDamage::new(20, 36, 1, 1), true)
        .expect("multi-row span insert replay is eligible");
    assert!(r.bound_walk);
    assert_eq!(
        r.exposed_row_base, 1,
        "row 1 owns the source lookahead into the edited span"
    );
    assert_eq!(
        r.exposed_row_count, 3,
        "topology predecessor plus rows 2 and 3 intersecting [20,35)"
    );
    assert_eq!(
        r.reused_rows.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![0, 4]
    );
    let below = r.reused_rows.iter().find(|(i, _)| *i == 4).unwrap();
    assert_eq!(below.1.start_charpos, 41, "row 4 start 40 -> 41");
    let expected = r.expected_walk.unwrap();
    assert_eq!(expected.row_count, 3);
    assert_eq!(
        expected.last_row_end_charpos, 40,
        "old row-3 end 39 + delta 1"
    );
}

/// The two fast paths agreed on what an overlay CHANGE is only after the
/// delta was named. `cursor_only_eligible` compared the raw
/// `overlay_modified_tick` while `edit_eligible` compared the digest, so
/// an identical rebuild -- LSP diagnostics tearing down and recreating the
/// same overlays -- was a change to one path and a non-change to the
/// other. Nothing pointed at the inconsistency while each predicate was
/// its own private recipe of field alignments.
#[test]
fn an_identical_overlay_rebuild_is_not_a_change_to_either_fast_path() {
    let prev = synthetic_key(0, 10);

    // Tick moved, digest did not: the set was rebuilt exactly as it was.
    let mut rebuilt = prev.clone();
    rebuilt.overlay_modified_tick = prev.overlay_modified_tick + 1;
    let delta = WindowDelta::between(&prev, &rebuilt);
    assert!(
        !delta.overlays_changed,
        "an identical rebuild is not a change"
    );
    assert!(delta.is_still(), "nothing moved at all");
    assert!(
        RetainedWindowKey::cursor_only_eligible(&prev, &rebuilt),
        "the cursor-only path must reuse the body across an identical rebuild"
    );

    // A set that really changed still escalates on both paths.
    let mut changed = rebuilt.clone();
    changed.overlay_digest = prev.overlay_digest ^ 0x5bf0_3635;
    assert!(WindowDelta::between(&prev, &changed).overlays_changed);
    assert!(!RetainedWindowKey::cursor_only_eligible(&prev, &changed));
    let mut edited = changed.clone();
    edited.chars_modified_tick += 1;
    assert!(!RetainedWindowKey::edit_eligible(&prev, &edited));
}
/// Props tick movement alone now qualifies for the edit path (GNU
/// try_window_id proceeds through property changes); overlay/face moves
/// still escalate.
#[test]
fn edit_eligible_accepts_props_and_rebuilt_overlays_but_not_changed_ones() {
    let prev = synthetic_key(0, 10);
    let mut props_only = synthetic_key(0, 12);
    props_only.props_modified_tick = 9;
    assert!(RetainedWindowKey::edit_eligible(&prev, &props_only));

    // An overlay was touched but the set is byte-for-byte what it was:
    // the tick moved, the digest did not. This is the LSP-diagnostic
    // rebuild, and it must NOT cost a full relayout.
    let mut overlays_rebuilt = props_only.clone();
    overlays_rebuilt.overlay_modified_tick = 6;
    assert!(RetainedWindowKey::edit_eligible(&prev, &overlays_rebuilt));

    // The set genuinely differs: escalate, exactly as GNU's
    // `OVERLAY_MODIFF` give-up does.
    let mut overlays_changed = overlays_rebuilt.clone();
    overlays_changed.overlay_digest = prev.overlay_digest ^ 0x9e37_79b9;
    assert!(!RetainedWindowKey::edit_eligible(&prev, &overlays_changed));

    // A digest change with NO tick movement still escalates: the digest is
    // the authority, the tick is only the cheap hint.
    let mut digest_only = props_only.clone();
    digest_only.overlay_digest = prev.overlay_digest ^ 0x1234_5678;
    assert!(!RetainedWindowKey::edit_eligible(&prev, &digest_only));

    let mut face_moved = props_only.clone();
    face_moved.face_change_count = 6;
    assert!(!RetainedWindowKey::edit_eligible(&prev, &face_moved));
}

/// A simple in-line delete (delta < 0) also reuses the rows below the
/// span, shifted DOWN by the deleted count; the deleted-newline hazard is
/// owned by the post-walk expected_walk validation, so the plan builds
/// optimistically.
#[test]
fn edit_replay_delete_reuses_below_rows_with_negative_shift() {
    use neomacs_display_protocol::glyph_matrix::Glyph;
    let mut m = synthetic_matrix(0, 5); // rows start at 0,10,20,30,40
    for c in 0..10 {
        let mut g = Glyph::char('a', FaceId::new(0), 20 + c);
        g.pixel_width = 8.0;
        MatrixRow::make_mut(&mut m.matrix.rows[2]).glyphs[GlyphArea::Text.index()].push(g);
    }
    // 1 char deleted at 25: old span [25, 26), new span empty; size -1.
    let mut curr = synthetic_key(0, 25);
    curr.chars_modified_tick = 6;
    curr.buffer_size = 999;

    let r = m
        .edit_replay(&curr, EditDamage::new(25, 25, -1, 0), true)
        .expect("delete below-reuse is eligible");
    assert!(r.bound_walk);
    assert_eq!(r.exposed_row_base, 2);
    assert_eq!(r.exposed_row_count, 1);
    assert_eq!(
        r.reused_rows.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![0, 1, 3, 4]
    );
    let below3 = r.reused_rows.iter().find(|(i, _)| *i == 3).unwrap();
    assert_eq!(below3.1.start_charpos, 29, "row 3 start 30 -> 29");
    let expected = r.expected_walk.unwrap();
    assert_eq!(expected.row_count, 1);
    assert_eq!(
        expected.last_row_end_charpos, 28,
        "old row-2 end 29 + delta -1"
    );
}
