//! Explicit eager/deferred controls. Fixtures own immutable protocol rows and
//! numeric metadata; no new test selector, runtime state, or TLS is needed.
use super::*;
use crate::incremental_layout::edit_sync::{
    EditSyncInstall, EditSyncPlan, EditSyncReached, backward_scroll_plan, dense_index,
};
use crate::incremental_layout::*;
use neomacs_display_protocol::cursor::CursorStyle;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{
    Glyph, GlyphPointerAppearance, GlyphPointerSourceIdentity, GlyphProvenance,
    GlyphStringBufferRange, GlyphStringId, GlyphStringSource,
};
use neovm_core::window::{DisplayPointRole, DisplayPointRows, WindowDisplaySnapshot, WindowId};

fn source_row(pointer: bool) -> MatrixRow {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    row.start_charpos = 100;
    row.end_charpos = 106;
    row.pixel_y = 50.0;
    row.height_px = 10.0;
    row.cursor_col = Some(1);
    row.cursor_type = Some(CursorStyle::FilledBox);
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char('界', FaceId::new(3), 100));
    let source = row
        .push_string_source(GlyphStringSource::replacement(
            GlyphStringId::new(7),
            GlyphStringBufferRange::new(101, 106),
        ))
        .unwrap();
    row.glyphs[GlyphArea::Text.index()].push(
        Glyph::char('é', FaceId::new(7), 101).with_provenance(GlyphProvenance::string(source, 2)),
    );
    if pointer {
        let pointer = row
            .intern_pointer_appearance(GlyphPointerAppearance {
                source: GlyphPointerSourceIdentity {
                    kind: GlyphPointerSourceKind::Buffer,
                    source_id: 1,
                    range_start: 101,
                    range_end: 106,
                    property_owner: 1,
                    occurrence: GlyphPointerOccurrenceIdentity::BufferDisplayReplacement {
                        buffer_id: 1,
                        anchor: 101,
                    },
                },
                face_id: FaceId::new(11),
            })
            .unwrap();
        row.glyphs[GlyphArea::Text.index()][0].pointer_appearance = Some(pointer);
    }
    MatrixRow::new(row)
}

fn prepared(source: &[(usize, MatrixRow)], delta: i64, mode: DeferShiftMode) -> CandidateRows {
    CandidateRows::prepare(
        source.iter().map(|(index, row)| (*index, row)),
        90,
        delta,
        mode,
    )
}

fn plan(source: &[(usize, MatrixRow)], delta: i64, mode: DeferShiftMode) -> EditSyncPlan {
    let snapshots = source
        .iter()
        .map(|(index, row)| DisplayRowSnapshot {
            row: *index as i64,
            y: row.pixel_y as i64,
            height: 10,
            start_buffer_pos: Some(LispCharPos1::from_one_based_usize(
                (row.start_charpos as i64 + delta + 1) as usize,
            )),
            end_buffer_pos: Some(LispCharPos1::from_one_based_usize(
                (row.end_charpos as i64 + delta + 1) as usize,
            )),
            ..DisplayRowSnapshot::default()
        })
        .collect();
    let points: Vec<_> = source
        .iter()
        .map(|(index, row)| DisplayPointSnapshot {
            row: *index as i64,
            buffer_pos: LispCharPos1::from_one_based_usize(
                (row.start_charpos as i64 + delta + 1) as usize,
            ),
            role: DisplayPointRole::Glyph,
            x: 0,
            y: row.pixel_y as i64,
            width: 8,
            height: 10,
            col: 0,
        })
        .collect();
    EditSyncPlan {
        stop_charpos: (source[0].1.start_charpos as i64 + delta) as usize,
        first_unchanged_index: source[0].0,
        first_unchanged_y: source[0].1.pixel_y,
        rows: prepared(source, delta, mode),
        row_snapshots: snapshots,
        point_rows: Some(DisplayPointRows::from_points(points.clone())),
        points,
    }
}

fn assert_installed_equal(left: &EditSyncInstall, right: &EditSyncInstall) {
    assert_eq!(format!("{:?}", left.rows), format!("{:?}", right.rows));
    assert_eq!(left.row_snapshots, right.row_snapshots);
    assert_eq!(left.points, right.points);
    assert_eq!(left.point_rows, right.point_rows);
    assert_eq!(left.dy, right.dy);
}

#[test]
fn defer_shift_selector_is_default_on_and_preserves_explicit_controls() {
    assert_eq!(DeferShiftMode::parse(None), DeferShiftMode::On);
    for value in ["", "off", "0", "false", "junk"] {
        assert_eq!(
            DeferShiftMode::parse(Some(value.as_ref())),
            DeferShiftMode::Off
        );
    }
    for value in ["on", "1", "true", "yes", " ON "] {
        assert_eq!(
            DeferShiftMode::parse(Some(value.as_ref())),
            DeferShiftMode::On
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            DeferShiftMode::parse(Some(std::ffi::OsStr::from_bytes(b"on\xff"))),
            DeferShiftMode::Off
        );
    }
}

#[test]
fn deferred_rows_share_originals_until_materialized_and_preserve_all_source_metadata() {
    for (delta, pointer) in [(4, true), (-3, false)] {
        let source = vec![(5, source_row(pointer))];
        let original = format!("{:?}", source);
        let eager = prepared(&source, delta, DeferShiftMode::Off);
        let deferred = prepared(&source, delta, DeferShiftMode::On);
        assert!(matches!(&deferred.stage, CandidateStage::Deferred { .. }));
        assert!(std::ptr::eq(
            deferred.face_rows().next().unwrap(),
            source[0].1.as_ref()
        ));
        assert_eq!(
            referenced_face_ids(eager.face_rows()),
            referenced_face_ids(deferred.face_rows())
        );
        let positioned = deferred.into_positioned();
        assert_eq!(
            format!("{:?}", eager.into_positioned()),
            format!("{positioned:?}")
        );
        let row = &positioned[0].1;
        assert_eq!(row.start_charpos, (100 + delta) as usize);
        assert_eq!(row.end_charpos, (106 + delta) as usize);
        assert_eq!(
            row.glyphs[GlyphArea::Text.index()][0].provenance,
            GlyphProvenance::buffer((100 + delta) as usize)
        );
        let provenance = source[0].1.glyphs[GlyphArea::Text.index()][1].provenance;
        assert_eq!(
            row.glyphs[GlyphArea::Text.index()][1].provenance,
            provenance,
            "string index is row-local"
        );
        let string_source = source[0].1.string_sources()[0];
        assert_eq!(
            row.string_sources()[0].covered_buffer_range(),
            Some(GlyphStringBufferRange::new(
                (101 + delta) as usize,
                (106 + delta) as usize
            ))
        );
        assert_eq!(source[0].1.string_sources()[0], string_source);
        if pointer {
            assert_eq!(row.pointer_appearances()[0].source.range_start, 105);
            assert_eq!(row.pointer_appearances()[0].source.range_end, 110);
            assert_eq!(
                row.pointer_appearances()[0].source.occurrence,
                GlyphPointerOccurrenceIdentity::BufferDisplayReplacement {
                    buffer_id: 1,
                    anchor: 105
                }
            );
        }
        assert!(row.cursor_col.is_none() && row.cursor_type.is_none());
        assert_eq!(row.appearance_id(), source[0].1.appearance_id());
        assert_eq!(
            format!("{:?}", source),
            original,
            "retained source is immutable"
        );
    }
}

#[test]
fn rejected_or_unsynchronized_attempt_drops_only_shared_handles() {
    let mut source = source_row(true);
    let pointer = source.as_ref() as *const GlyphRow;
    {
        let candidates =
            CandidateRows::prepare(std::iter::once((5, &source)), 90, 4, DeferShiftMode::On);
        let plan = EditSyncPlan {
            stop_charpos: 104,
            first_unchanged_index: 5,
            first_unchanged_y: 50.0,
            rows: candidates,
            row_snapshots: vec![],
            points: vec![],
            point_rows: None,
        };
        assert!(
            plan.stop()
                .reached_at(crate::types::LayoutCharPos0::new(105), 50.0, 5)
                .is_none()
        );
        assert!(std::ptr::eq(
            plan.face_rows().next().unwrap(),
            source.as_ref()
        ));
        // Dropping an attempt after failed synchronization is also the path
        // for a pre-walk freshness rejection; neither consumes candidate rows.
        drop(plan);
    }
    assert_eq!(
        MatrixRow::make_mut(&mut source) as *mut GlyphRow as *const GlyphRow,
        pointer,
        "dropping the rejected plan releases every extra Arc handle"
    );
    assert_eq!(source.start_charpos, 100);
    assert_eq!(source.cursor_col, Some(1));
}

#[test]
fn ready_transfer_preserves_original_vector_and_arc_identity() {
    let mut row = source_row(false);
    MatrixRow::make_mut(&mut row).cursor_col = None;
    MatrixRow::make_mut(&mut row).cursor_type = None;
    let rows = vec![(5, row)];
    let storage = rows.as_ptr();
    let row_identity = rows[0].1.as_ref() as *const GlyphRow;
    let positioned = CandidateRows::ready(rows).into_positioned();
    assert_eq!(positioned.as_ptr(), storage);
    assert_eq!(positioned[0].1.as_ref() as *const GlyphRow, row_identity);
}

#[test]
fn zero_delta_keeps_original_eager_cursor_stripping_and_sharing() {
    let mut undecorated = source_row(false);
    MatrixRow::make_mut(&mut undecorated).cursor_col = None;
    MatrixRow::make_mut(&mut undecorated).cursor_type = None;
    let source = vec![(5, source_row(false)), (6, undecorated)];
    for mode in [DeferShiftMode::Off, DeferShiftMode::On] {
        let rows = prepared(&source, 0, mode);
        assert!(matches!(&rows.stage, CandidateStage::Ready(_)));
        let positioned = rows.into_positioned();
        assert!(!std::ptr::eq(
            positioned[0].1.as_ref(),
            source[0].1.as_ref()
        ));
        assert!(std::ptr::eq(positioned[1].1.as_ref(), source[1].1.as_ref()));
        assert!(
            positioned
                .iter()
                .all(|(_, row)| row.cursor_col.is_none() && row.cursor_type.is_none())
        );
    }
}

#[test]
fn dense_and_legacy_installers_consume_identical_shifted_rows_and_clip_snapshots() {
    let mut second = source_row(false);
    MatrixRow::make_mut(&mut second).pixel_y = 60.0;
    let source = vec![(5, source_row(false)), (6, second)];
    for delta in [-3, 4] {
        for (index, y, bottom, limit) in [
            (5, 50.0, 80.0, 9),
            (6, 60.0, 70.0, 9),
            (5, 50.0, 80.0, 6),
            (6, 60.0, 0.0, 9),
        ] {
            let reached = EditSyncReached {
                display_row_index: index,
                y,
            };
            let expected =
                plan(&source, delta, DeferShiftMode::Off).install(reached, bottom, limit);
            let deferred = plan(&source, delta, DeferShiftMode::On).install(reached, bottom, limit);
            assert_installed_equal(&expected, &deferred);
            let dense_plan = plan(&source, delta, DeferShiftMode::On);
            let dy = y - 50.0;
            let shift_y = dy.abs() >= 0.5;
            let dvpos = index as i64 - 5;
            let kept = dense_index::installation_map(
                dense_plan.rows.placement_rows(),
                dvpos,
                dy,
                shift_y,
                bottom,
                limit,
            )
            .unwrap();
            let dense = dense_index::install(dense_plan, kept, dvpos, dy, shift_y, bottom, limit);
            assert_installed_equal(&expected, &dense);
        }
    }
}

#[test]
fn still_installation_keeps_stage_vector_and_eager_snapshot_storage() {
    // Reuse the existing thread-owned RAII policy scope; no new test TLS.
    let _policy = crate::incremental_layout::edit_sync::dense_index_test_support::Guard::set(true);
    crate::incremental_layout::edit_sync::set_edit_sync_still_for_test(Some(true));
    let source = vec![(5, source_row(true))];
    for mode in [DeferShiftMode::Off, DeferShiftMode::On] {
        let plan = plan(&source, 4, mode);
        let vector_storage = plan.rows.placement_rows().as_ptr();
        let snapshots_storage = plan.row_snapshots.as_ptr();
        let point_storage = plan.points.as_ptr();
        let cell_storage = plan.point_rows.as_ref().unwrap().rows.as_ptr();
        let installed = plan.install_edit(
            EditSyncReached {
                display_row_index: 5,
                y: 50.0,
            },
            80.0,
            9,
        );
        assert_eq!(installed.rows.as_ptr(), vector_storage);
        assert_eq!(installed.row_snapshots.as_ptr(), snapshots_storage);
        assert_eq!(installed.points.as_ptr(), point_storage);
        assert_eq!(
            installed.point_rows.as_ref().unwrap().rows.as_ptr(),
            cell_storage
        );
        assert_eq!(installed.rows[0].1.start_charpos, 104);
        assert_eq!(
            installed.rows[0].1.pointer_appearances()[0]
                .source
                .range_start,
            105
        );
        assert!(installed.rows[0].1.cursor_col.is_none());
    }
}

#[test]
fn install_edit_dispatch_and_sparse_fallback_preserve_deferred_results() {
    for indices in [[5, 6], [5, 7], [5, 5]] {
        let mut second = source_row(false);
        MatrixRow::make_mut(&mut second).pixel_y = 60.0;
        let source = vec![(indices[0], source_row(false)), (indices[1], second)];
        let reached = EditSyncReached {
            display_row_index: 6,
            y: 60.0,
        };
        for dense in [false, true] {
            let _policy =
                crate::incremental_layout::edit_sync::dense_index_test_support::Guard::set(dense);
            let expected = plan(&source, 4, DeferShiftMode::Off).install(reached, 80.0, 9);
            let deferred = plan(&source, 4, DeferShiftMode::On).install_edit(reached, 80.0, 9);
            assert_installed_equal(&expected, &deferred);
        }
    }
}

#[test]
fn both_phase_a_face_collectors_admit_deferred_candidates_without_materializing() {
    let source = vec![(5, source_row(true))];
    let replay = ScrollReplay {
        dvpos: 0.0,
        reused_rows: vec![],
        reused_row_snapshots: vec![],
        reused_points: vec![],
        reused_point_rows: None,
        walk_start: PartialBodyWalkStart::new(90),
        exposed_row_base: 4,
        exposed_row_count: 1,
        exposed_text_y: 40.0,
        new_window_start: 0,
        new_point: 90,
        bound_walk: false,
        expected_walk: None,
        chrome: None,
        chrome_memo: None,
        one_line_contract: None,
        sync: Some(plan(&source, 4, DeferShiftMode::On).into()),
        edit: true,
        face_generation: FrameFaceGeneration::default(),
    };
    let expected = vec![FaceId::new(3), FaceId::new(7), FaceId::new(11)];
    assert_eq!(replay.retained_face_ids(), expected);
    let mut ids = std::collections::BTreeSet::new();
    replay.extend_retained_face_ids(&mut ids);
    assert_eq!(ids.into_iter().collect::<Vec<_>>(), expected);
    let sync = replay.sync.as_ref().unwrap();
    assert!(matches!(
        &sync.union_for_test().rows.stage,
        CandidateStage::Deferred { .. }
    ));
    assert!(std::ptr::eq(
        sync.face_rows().next().unwrap(),
        source[0].1.as_ref()
    ));
}

#[test]
fn character_coordinate_threshold_preserves_existing_negative_clamping() {
    let source = source_row(false);
    for from in [-5, 0, 102] {
        let eager =
            CandidateRows::prepare(std::iter::once((5, &source)), from, 4, DeferShiftMode::Off);
        let deferred =
            CandidateRows::prepare(std::iter::once((5, &source)), from, 4, DeferShiftMode::On);
        assert_eq!(
            format!("{:?}", eager.into_positioned()),
            format!("{:?}", deferred.into_positioned())
        );
    }
}

// Same numeric retained-window shape used by the existing Sync classifier
// controls. No editor, Lisp runtime, or process selector participates here.
fn fixture_key(window_start: i64, point: i64) -> RetainedWindowKey {
    RetainedWindowKey {
        fontset_generation: 1,
        prefixes: Default::default(),
        invisibility: Default::default(),
        char_table_revision: Default::default(),
        symbol_property_revision: Default::default(),
        display_table: Default::default(),
        media_generation: 0,
        body_redisplay: Default::default(),
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
        word_wrap_policy: Default::default(),
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
        face_overlay_coverage: Default::default(),
        face_change_count: 5,
        display_var_change_count: 5,
    }
}

/// A retained matrix with `n_body` 16px Text rows (10 chars each, the first
/// starting at `base`) plus a mode-line row.
fn fixture_matrix(base: i64, n_body: usize) -> RetainedWindowMatrix {
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
        key: fixture_key(base, 0),
        validity: MatrixValidity::Valid,
        display_snapshot: std::sync::Arc::new(WindowDisplaySnapshot {
            point_rows: None,
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
        }),
        presented_cursor: None,
        face_generation: FrameFaceGeneration::default(),
        chrome_uses_column: false,
        chrome_modified_flag: false,
        chrome_fingerprints: None,
    }
}

#[test]
fn dense_plan_materializes_the_same_rows_and_preserves_eager_snapshot_streams() {
    let mut matrix = fixture_matrix(0, 6);
    let body: Vec<_> = matrix
        .matrix
        .rows
        .iter()
        .take(6)
        .cloned()
        .enumerate()
        .collect();
    let snapshot_source = plan(&body, 0, DeferShiftMode::Off);
    let snapshot = std::sync::Arc::make_mut(&mut matrix.display_snapshot);
    snapshot.rows = snapshot_source.row_snapshots;
    snapshot.points = snapshot_source.points;
    snapshot.point_rows = snapshot_source.point_rows;
    let candidates: Vec<_> = matrix
        .matrix
        .rows
        .iter()
        .enumerate()
        .skip(2)
        .take(4)
        .collect();
    for delta in [-1, 0, 1] {
        let stop = (candidates[0].1.start_charpos as i64 + delta) as usize;
        let eager =
            dense_index::plan(&matrix, &candidates, 15, delta, stop, DeferShiftMode::Off).unwrap();
        let deferred =
            dense_index::plan(&matrix, &candidates, 15, delta, stop, DeferShiftMode::On).unwrap();
        assert_eq!(eager.stop_charpos, deferred.stop_charpos);
        assert_eq!(eager.first_unchanged_index, deferred.first_unchanged_index);
        assert_eq!(eager.first_unchanged_y, deferred.first_unchanged_y);
        assert_eq!(eager.row_snapshots, deferred.row_snapshots);
        assert_eq!(eager.points, deferred.points);
        assert_eq!(eager.point_rows, deferred.point_rows);
        if delta != 0 {
            assert!(std::ptr::eq(
                deferred.face_rows().next().unwrap(),
                candidates[0].1.as_ref()
            ));
        }
        assert_eq!(
            format!("{:?}", eager.rows.into_positioned()),
            format!("{:?}", deferred.rows.into_positioned())
        );
    }
}

#[test]
fn backward_scroll_stays_ready_with_original_zero_delta_cursor_semantics() {
    let mut matrix = fixture_matrix(100, 3);
    let first = MatrixRow::make_mut(&mut matrix.matrix.rows[0]);
    first.cursor_col = Some(0);
    first.cursor_type = Some(CursorStyle::FilledBox);
    let body: Vec<_> = matrix.matrix.rows.iter().enumerate().take(3).collect();
    let plan = backward_scroll_plan(&matrix, &body).unwrap();
    assert!(matches!(&plan.rows.stage, CandidateStage::Ready(_)));
    let original_zero: Vec<_> = body
        .iter()
        .map(|&(index, row)| (index, super::super::shift_row_positions(row, 0, 0)))
        .collect();
    let rows = plan.rows.into_positioned();
    assert_eq!(format!("{rows:?}"), format!("{original_zero:?}"));
    assert!(!std::ptr::eq(rows[0].1.as_ref(), body[0].1.as_ref()));
    assert!(std::ptr::eq(rows[1].1.as_ref(), body[1].1.as_ref()));
}

/// Native retained rows with independent buffer provenance and complete
/// snapshot streams. No live producer authorization is constructed here.
fn prepared_suffix_fixture() -> RetainedWindowMatrix {
    let mut matrix = fixture_matrix(0, 6);
    for (index, row) in matrix.matrix.rows.iter_mut().take(6).enumerate() {
        let row = MatrixRow::make_mut(row);
        let mut glyph = Glyph::char('a', FaceId::new(index as u32 + 1), row.start_charpos);
        glyph.pixel_width = 8.0;
        row.glyphs[GlyphArea::Text.index()].push(glyph);
    }
    let source: Vec<_> = matrix
        .matrix
        .rows
        .iter()
        .take(6)
        .cloned()
        .enumerate()
        .collect();
    let snapshots = plan(&source, 0, DeferShiftMode::Off);
    let snapshot = std::sync::Arc::make_mut(&mut matrix.display_snapshot);
    snapshot.rows = snapshots.row_snapshots;
    snapshot.points = snapshots.points;
    snapshot.point_rows = snapshots.point_rows;
    matrix
}

#[test]
fn prepared_edit_suffix_keeps_one_union_for_both_frontiers_and_face_admission() {
    use crate::incremental_layout::edit_sync::{
        EditSyncBodyRowIndex, RestartInterest, RetainedEditBody, prepare_positions,
    };
    let matrix = prepared_suffix_fixture();
    let body: Vec<_> = matrix.matrix.rows.iter().enumerate().take(6).collect();
    let preparation = prepare_positions(
        &matrix,
        RetainedEditBody::new(&body, EditSyncBodyRowIndex::new(1)).unwrap(),
        EditReplayPositions::new(15, 21, 1),
    )
    .unwrap();
    let union = preparation.union_for_test();
    assert_eq!(union.first_unchanged_index, 2);
    assert_eq!(
        union
            .rows
            .placement_rows()
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 5]
    );
    assert_eq!(preparation.conservative_stop().stop_charpos, 31);
    assert_eq!(
        preparation.restart_interest(crate::types::LayoutCharPos0::new(21)),
        RestartInterest::Candidate
    );
    assert_eq!(
        preparation.restart_interest(crate::types::LayoutCharPos0::new(31)),
        RestartInterest::NotApplicable
    );
    assert_eq!(
        referenced_face_ids(preparation.face_rows()),
        vec![
            FaceId::new(3),
            FaceId::new(4),
            FaceId::new(5),
            FaceId::new(6)
        ]
    );
    assert_eq!(
        union
            .row_snapshots
            .iter()
            .map(|row| row.row)
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 5]
    );
    assert_eq!(
        union
            .points
            .iter()
            .map(|point| point.row)
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 5]
    );
    assert_eq!(
        union
            .point_rows
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .map(|row| row.row())
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 5]
    );
}

#[test]
fn prepared_edit_suffix_conservative_selection_keeps_canonical_rows_and_projections() {
    use crate::incremental_layout::edit_sync::{
        EditSyncBodyRowIndex, RetainedEditBody, plan_positions, prepare_positions,
    };
    for delta in [-1, 0, 1] {
        let matrix = prepared_suffix_fixture();
        let body: Vec<_> = matrix.matrix.rows.iter().enumerate().take(6).collect();
        let positions = EditReplayPositions::new(15, 20 + delta, delta);
        let canonical = plan_positions(&matrix, &body, 1, positions).unwrap();
        let preparation = prepare_positions(
            &matrix,
            RetainedEditBody::new(&body, EditSyncBodyRowIndex::new(1)).unwrap(),
            positions,
        )
        .unwrap();
        assert_eq!(preparation.union_for_test().rows.placement_rows().len(), 4);
        let stop = preparation.conservative_stop();
        let reached = stop
            .reached_at(
                crate::types::LayoutCharPos0::new(stop.stop_charpos as i64),
                48.0,
                3,
            )
            .unwrap();
        let selected = preparation.select_conservative(reached);
        assert_eq!(
            selected.source_position(),
            crate::types::LayoutCharPos0::new(30 + delta)
        );
        let installed = selected.install_edit(
            super::super::EditSyncWindowY::new(100.0).unwrap(),
            super::super::EditSyncRowLimit::new(7),
        );
        let expected = canonical.install_edit(reached, 100.0, 7);
        assert_installed_equal(&installed, &expected);
        assert_eq!(
            installed
                .rows
                .iter()
                .map(|(index, _)| *index)
                .collect::<Vec<_>>(),
            vec![3, 4, 5]
        );
        assert_eq!(installed.row_snapshots.len(), 3);
        assert_eq!(installed.points.len(), 3);
        assert_eq!(installed.point_rows.as_ref().unwrap().rows.len(), 3);
    }
}

#[test]
fn prepared_edit_suffix_discards_unselected_row_before_positioning() {
    use crate::incremental_layout::edit_sync::prepared_suffix::{PreparedEditSuffix, SuffixOrigin};
    let mut source = vec![(5, source_row(false)), (6, source_row(false))];
    MatrixRow::make_mut(&mut source[1].1).start_charpos = 110;
    MatrixRow::make_mut(&mut source[1].1).end_charpos = 116;
    MatrixRow::make_mut(&mut source[1].1).pixel_y = 60.0;
    let original = source[0].1.as_ref() as *const GlyphRow;
    let conservative = SuffixOrigin::from_row(
        crate::incremental_layout::edit_sync::EditSyncRowIndex::new(6),
        &source[1].1,
        EditReplayPositions::new(90, 94, 4),
    )
    .unwrap();
    let preparation = PreparedEditSuffix::with_independent_frontier(
        plan(&source, 4, DeferShiftMode::On),
        conservative,
    );
    assert!(matches!(
        &preparation.union_for_test().rows.stage,
        CandidateStage::Deferred { .. }
    ));
    let selected = preparation.select_conservative(EditSyncReached {
        display_row_index: 6,
        y: 60.0,
    });
    assert_eq!(
        MatrixRow::make_mut(&mut source[0].1) as *mut GlyphRow as *const GlyphRow,
        original,
        "the unselected prefix Arc must be released before any suffix installation"
    );
    let installed = selected.install_edit(
        super::super::EditSyncWindowY::new(100.0).unwrap(),
        super::super::EditSyncRowLimit::new(8),
    );
    assert_eq!(installed.rows.len(), 1);
    assert_eq!(installed.rows[0].0, 6);
    assert_eq!(installed.rows[0].1.start_charpos, 114);
}

#[test]
fn prepared_edit_suffix_from_conservative_plan_preserves_scroll_installation() {
    use crate::incremental_layout::edit_sync::{PreparedEditSuffix, RestartInterest};
    let source = vec![(5, source_row(true))];
    let reached = EditSyncReached {
        display_row_index: 6,
        y: 60.0,
    };
    let preparation = PreparedEditSuffix::from(plan(&source, 4, DeferShiftMode::On));
    assert_eq!(
        preparation.restart_interest(crate::types::LayoutCharPos0::new(104)),
        RestartInterest::NotApplicable
    );
    let selected = preparation.select_conservative(reached);
    let installed = selected.install_scroll(
        super::super::EditSyncWindowY::new(100.0).unwrap(),
        super::super::EditSyncRowLimit::new(8),
    );
    let expected = plan(&source, 4, DeferShiftMode::On).install(reached, 100.0, 8);
    assert_installed_equal(&installed, &expected);
}

#[derive(Clone, Copy, Debug)]
enum PreparedSuffixRefusalCase {
    Boxed,
    Continuation,
    StringSource,
    EndOfBuffer,
    Geometry,
}

#[test]
fn prepared_edit_suffix_typed_refusals_retain_only_the_conservative_suffix() {
    use crate::incremental_layout::edit_sync::prepared_suffix::IndependentFrontierRefusal;
    use crate::incremental_layout::edit_sync::{
        EditSyncBodyRowIndex, RestartInterest, RetainedEditBody, prepare_positions,
    };
    for case in [
        PreparedSuffixRefusalCase::Boxed,
        PreparedSuffixRefusalCase::Continuation,
        PreparedSuffixRefusalCase::StringSource,
        PreparedSuffixRefusalCase::EndOfBuffer,
        PreparedSuffixRefusalCase::Geometry,
    ] {
        let mut matrix = prepared_suffix_fixture();
        let expected = match case {
            PreparedSuffixRefusalCase::Boxed => {
                MatrixRow::make_mut(&mut matrix.matrix.rows[1]).glyphs[GlyphArea::Text.index()]
                    [0]
                .box_vertical_edges = neomacs_display_protocol::face::BoxVerticalEdges::Both;
                IndependentFrontierRefusal::BoxDependency
            }
            PreparedSuffixRefusalCase::Continuation => {
                MatrixRow::make_mut(&mut matrix.matrix.rows[1]).continued = true;
                IndependentFrontierRefusal::SourceTopology
            }
            PreparedSuffixRefusalCase::StringSource => {
                MatrixRow::make_mut(&mut matrix.matrix.rows[2])
                    .push_string_source(GlyphStringSource::replacement(
                        GlyphStringId::new(9),
                        GlyphStringBufferRange::new(20, 29),
                    ))
                    .unwrap();
                IndependentFrontierRefusal::SourceProvenance
            }
            PreparedSuffixRefusalCase::EndOfBuffer => {
                MatrixRow::make_mut(&mut matrix.matrix.rows[2]).ends_at_zv = true;
                IndependentFrontierRefusal::EndOfBuffer
            }
            PreparedSuffixRefusalCase::Geometry => {
                MatrixRow::make_mut(&mut matrix.matrix.rows[2]).pixel_y = f32::NAN;
                IndependentFrontierRefusal::InvalidGeometry
            }
        };
        let body: Vec<_> = matrix.matrix.rows.iter().enumerate().take(6).collect();
        let retained = RetainedEditBody::new(&body, EditSyncBodyRowIndex::new(1)).unwrap();
        let positions = EditReplayPositions::new(15, 21, 1);
        assert!(
            matches!(super::super::independent_frontier(&retained, EditSyncBodyRowIndex::new(3), positions), Err(reason) if reason == expected),
            "{case:?}"
        );
        let preparation = prepare_positions(&matrix, retained, positions).unwrap();
        assert_eq!(
            preparation.union_for_test().first_unchanged_index,
            3,
            "{case:?}"
        );
        assert_eq!(
            preparation.union_for_test().rows.placement_rows().len(),
            3,
            "{case:?}"
        );
        assert_eq!(
            preparation.restart_interest(crate::types::LayoutCharPos0::new(21)),
            RestartInterest::NotApplicable,
            "{case:?}"
        );
    }
}
