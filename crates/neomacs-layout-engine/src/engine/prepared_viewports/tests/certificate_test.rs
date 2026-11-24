use super::*;
use crate::string_property_snapshot::StringSourceRootScope;
use neovm_core::emacs_core::{Context, Value};

fn viewport() -> (Context, PreparedViewport) {
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer = context.buffer_manager().current_buffer().unwrap().id();
    context
        .buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert(&"source row\n".repeat(200));
    let frame = context
        .frame_manager_mut()
        .create_frame("prepared-certificate", 800, 600, buffer);
    context
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let window = context.frame_manager().get(frame).unwrap().selected_window;
    let window = DisplayWindowId::new(window.0 as i64);
    context
        .eval_str("(goto-char 1) (set-window-start nil 1 t)")
        .unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut context, frame);
    let retained = engine.retained_window_matrices[&window].clone();
    let faces = engine.frame_face_arenas[&frame].prepared_snapshot();
    (
        context,
        PreparedViewport {
            frame,
            window,
            payload: std::rc::Rc::new(PreparedViewportPayload {
                retained,
                faces,
                cursor_sources: None,
                query_freshness: None,
            }),
            rows: 0,
            glyphs: 0,
            computed: false,
            complete_viewport: true,
        },
    )
}

#[test]
fn missing_original_certificate_does_not_authorize_prepared_rows() {
    let (_context, entry) = viewport();
    assert!(
        !entry.dependencies_valid(),
        "absence of observations is not an empty source certificate"
    );
}

#[test]
fn computed_rows_keep_original_reads_instead_of_recapturing_at_admission() {
    use neovm_core::tagged::collection_reads;
    let (mut context, entry) = viewport();
    let a = context
        .eval_str("(setq prepared-computed-a (copy-sequence \"original worker A\"))")
        .unwrap();
    let roots = context.retain_gc_roots([a]);
    let (_, reads) = collection_reads::capture(|| {
        let _ = a.as_lisp_string().unwrap().as_bytes();
    });
    let mut prepared = PreparedViewports::default();
    assert!(prepared.insert_computed(
        entry.frame,
        entry.window,
        entry.retained.clone(),
        entry.faces.clone(),
        reads.unwrap(),
        vec![roots],
        true,
        None,
    ));
    let b = context
        .eval_str("(copy-sequence \"newer worker B\")")
        .unwrap();
    let latest = capture(&context, b);
    context.eval_str("(aset prepared-computed-a 0 ?Z)").unwrap();
    assert!(latest.unchanged());
    assert!(!prepared.entries[0].dependencies_valid());
    assert!(
        prepared
            .replay(entry.frame, entry.window, &entry.retained.key, true)
            .is_none()
    );

    let (_, reads) = collection_reads::capture(|| {
        let _ = a.as_lisp_string().unwrap().as_bytes();
    });
    context.eval_str("(aset prepared-computed-a 0 ?Y)").unwrap();
    assert!(
        !prepared.insert_computed(
            entry.frame,
            entry.window,
            entry.retained.clone(),
            entry.faces.clone(),
            reads.unwrap(),
            vec![context.retain_gc_roots([a])],
            true,
            None,
        ),
        "admission cannot rebase changed original producer reads"
    );
}

/// A fractional pixel offset can shrink the destination by one row while the
/// retained prefix still names its old bottom row. That prefix fills the
/// shorter viewport; the join must keep it instead of abandoning the page.
#[test]
fn shorter_destination_completes_a_prefix_that_fills_the_viewport() {
    use crate::frame_face_arena::{FrameFaceArena, FrameFaceGeneration};
    use crate::incremental_layout::{
        MatrixValidity, RetainedWindowKey, RetainedWindowMatrix, ScrollReplay,
    };
    use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
    use neomacs_display_protocol::glyph_matrix::{GlyphMatrix, GlyphRow, MatrixRow};
    use neomacs_display_protocol::types::Rect;
    use neovm_core::window::{PresentedWindowRegions, WindowDisplaySnapshot, WindowId};

    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer = context.buffer_manager().current_buffer().unwrap().id();
    let frame = context
        .frame_manager_mut()
        .create_frame("shorter-capacity", 800, 48, buffer);
    let window = context.frame_manager().get(frame).unwrap().selected_window;
    let window = DisplayWindowId::new(window.0 as i64);
    let observed = context.eval_str("(copy-sequence \"capacity\")").unwrap();
    let roots = context.retain_gc_roots([observed]);
    let (_, reads) = neovm_core::tagged::collection_reads::capture(|| {
        let _ = observed.as_lisp_string().unwrap().as_bytes();
    });

    let regions = PresentedWindowRegions {
        outer: Rect::new(0.0, 0.0, 800.0, 48.0),
        text_body: Rect::new(0.0, 0.0, 800.0, 48.0),
        ..Default::default()
    };
    let mut key = capacity_key(0, 1);
    key.fontset_generation = neovm_core::emacs_core::fontset::fontset_generation();
    key.partition = crate::window_layout::WindowPartitionSignature::from_regions(regions);
    key.viewport_rows = 4;
    let mut prepared_key = key.clone();
    prepared_key.window_start = 40;
    prepared_key.point = 41;
    let mut current = key.clone();
    current.vscroll = -4;
    current.viewport_rows = 3;

    let prefix_rows = (0..4)
        .map(|index| {
            let mut row = GlyphRow::new(GlyphRowRole::Text);
            row.enabled = true;
            row.displays_text = true;
            row.start_charpos = index * 10;
            row.end_charpos = index * 10 + 9;
            row.pixel_y = (index as f32) * 16.0;
            row.height_px = 16.0;
            row.ascent_px = 12.0;
            (index, MatrixRow::new(row))
        })
        .collect::<Vec<_>>();
    let mut prepared_matrix = GlyphMatrix::new(2, 80);
    for (offset, start) in [(0usize, 40usize), (1, 50)] {
        let row = MatrixRow::make_mut(&mut prepared_matrix.rows[offset]);
        row.enabled = true;
        row.role = GlyphRowRole::Text;
        row.displays_text = true;
        row.start_charpos = start;
        row.end_charpos = start + 9;
        row.pixel_y = (offset as f32) * 16.0;
        row.height_px = 16.0;
    }
    let mut retained = RetainedWindowMatrix {
        matrix: prepared_matrix,
        key: prepared_key,
        gnu_accuracy: None,
        validity: MatrixValidity::Valid,
        display_snapshot: std::sync::Arc::new(WindowDisplaySnapshot {
            window_id: WindowId(u64::try_from(window.get()).expect("window id")),
            regions,
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
    };
    // The joined matrix is judged as the current window, including its point.
    retained.key.point = current.point;
    let arena = FrameFaceArena::default();
    let faces = arena.prepared_snapshot();
    let mut prepared = PreparedViewports::default();
    assert!(prepared.insert_computed(
        frame,
        window,
        retained,
        faces,
        reads.expect("observe the capacity string"),
        vec![roots],
        true,
        None,
    ));
    let replay = ScrollReplay {
        dvpos: 0.0,
        reused_rows: prefix_rows,
        reused_row_snapshots: Vec::new(),
        reused_points: Vec::new(),
        reused_point_rows: None,
        walk_start: crate::types::PartialBodyWalkStart::new(0),
        exposed_row_base: 4,
        exposed_row_count: 0,
        exposed_text_y: 64.0,
        new_window_start: current.window_start,
        new_point: current.point,
        bound_walk: false,
        expected_walk: None,
        chrome: None,
        chrome_memo: None,
        one_line_contract: None,
        sync: None,
        edit: false,
        face_generation: arena.generation(),
    };
    let joined = prepared.complete_forward_scroll(
        frame,
        window,
        &current,
        &replay,
        crate::buffer_source::window_geometry::BufferWindowGeometry::for_capacity_test(3, 48.0),
        &arena,
        true,
        prepared.entries[0].cursor_sources.clone().unwrap(),
    );
    assert!(
        joined.is_some(),
        "a prefix that fills the shorter viewport must complete"
    );
}

fn capacity_key(window_start: i64, point: i64) -> RetainedWindowKey {
    use crate::incremental_layout::RetainedWindowKey;
    use crate::types::{DisplayLineNumbersMode, LineWrapMode, NobreakDisplayMode};
    use neomacs_display_protocol::types::Rect;
    use neovm_core::window::PresentedWindowRegions;

    RetainedWindowKey {
        fontset_generation: 0,
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
        partition: crate::window_layout::WindowPartitionSignature::from_regions(
            PresentedWindowRegions {
                outer: Rect::new(0.0, 0.0, 800.0, 48.0),
                text_body: Rect::new(0.0, 0.0, 800.0, 48.0),
                ..Default::default()
            },
        ),
        hscroll: 0,
        vscroll: 0,
        wrap_mode: LineWrapMode::Truncate,
        word_wrap: false,
        word_wrap_policy: Default::default(),
        bidi_paragraph_directions: Default::default(),
        tab_width: 8,
        char_width: 8.0,
        char_height: 16.0,
        smallest_opened_font_height: Some(16),
        glyph_matrix_row_scale: std::num::NonZeroU32::MIN,
        glyph_matrix_rows: None,
        font_pixel_size: 16.0,
        tab_stop_list: Vec::new(),
        extra_line_spacing: 0.0,
        selective_display: 0,
        selected: true,
        scroll_margin: Default::default(),
        viewport_rows: 3,
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

fn capture(context: &Context, value: Value) -> CursorSourceDependencies {
    let capture = crate::cursor_source_dependencies::Capture::begin(context);
    let _ = crate::cursor_source_dependencies::string_properties(
        value,
        &StringSourceRootScope::Rooted(context.owned_root_retainer()),
    );
    capture.finish().unwrap()
}

#[test]
fn historical_rows_keep_original_string_authority_after_a_newer_capture() {
    let (mut context, entry) = viewport();
    let a = context
        .eval_str("(setq prepared-source-a (copy-sequence \"original A\"))")
        .unwrap();
    let b = context
        .eval_str("(setq prepared-source-b (copy-sequence \"current B\"))")
        .unwrap();
    let original = capture(&context, a);
    let history = FrameVisualHistory::default()
        .with_cursor_sources([(entry.window, original.clone())].into_iter().collect());
    let mut next = entry.retained.clone();
    next.key.window_start += 100;
    let mut prepared = PreparedViewports::default();
    prepared.accept(
        entry.frame,
        [(entry.window, entry.retained.clone())]
            .into_iter()
            .collect(),
        &[(entry.window, next)].into_iter().collect(),
        Some(&FrameFaceArena::default()),
        Some(&history),
    );
    assert_eq!(prepared.entries.len(), 1);
    let (_, _, returned_sources) = prepared
        .replay(entry.frame, entry.window, &entry.retained.key, true)
        .unwrap();
    let returned_sources = returned_sources.unwrap();
    let mut shifted_key = entry.retained.key.clone();
    shifted_key.window_start = entry
        .retained
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .nth(1)
        .unwrap()
        .start_charpos as i64;
    let (_, _, shifted_sources) = prepared
        .scroll_replay(entry.frame, entry.window, &shifted_key, 0)
        .unwrap();
    let latest = capture(&context, b);
    context.eval_str("(aset prepared-source-a 0 ?Z)").unwrap();
    assert!(
        latest.unchanged(),
        "B's certificate must not stand in for A"
    );
    assert!(!original.unchanged());
    assert!(
        !returned_sources.unchanged(),
        "exact replay must return A's original authority"
    );
    assert!(
        !shifted_sources.unwrap().unchanged(),
        "shifted replay must return A's original authority"
    );
    assert!(
        prepared
            .replay(entry.frame, entry.window, &entry.retained.key, true)
            .is_none()
    );
    assert!(
        prepared
            .scroll_replay(entry.frame, entry.window, &shifted_key, 0)
            .is_none()
    );
    prepared.retire_invalid_dependencies();
    assert!(prepared.entries.is_empty());
    assert_eq!(prepared.invalidated, vec![(entry.frame, entry.window)]);
}

#[test]
fn forward_completion_certificate_keeps_prefix_and_multiple_original_pages() {
    for changed in ["prepared-prefix", "prepared-page-a", "prepared-page-b"] {
        let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
        let originals: Vec<_> = ["prepared-prefix", "prepared-page-a", "prepared-page-b"]
            .into_iter()
            .map(|name| {
                let value = context
                    .eval_str(&format!("(setq {name} (copy-sequence \"original\"))"))
                    .unwrap();
                capture(&context, value)
            })
            .collect();
        let combined = CursorSourceDependencies::combine_originals(originals).unwrap();
        assert!(combined.can_replay());
        assert!(combined.unchanged());
        let latest = capture(&context, Value::string("latest B is not an authority"));
        context.eval_str(&format!("(aset {changed} 0 ?Z)")).unwrap();
        assert!(latest.unchanged());
        assert!(
            !combined.unchanged(),
            "completion must retain the original authority of {changed}"
        );
    }
}

#[test]
fn history_without_an_original_certificate_is_not_cached() {
    let (_context, entry) = viewport();
    let mut next = entry.retained.clone();
    next.key.window_start += 100;
    let mut prepared = PreparedViewports::default();
    for history in [None, Some(FrameVisualHistory::default())] {
        prepared.accept(
            entry.frame,
            [(entry.window, entry.retained.clone())]
                .into_iter()
                .collect(),
            &[(entry.window, next.clone())].into_iter().collect(),
            Some(&FrameFaceArena::default()),
            history.as_ref(),
        );
        assert!(prepared.entries.is_empty());
    }
}

#[test]
fn frame_only_original_certificate_is_not_cached_as_history() {
    let (context, entry) = viewport();
    let original = capture(&context, Value::string("original"));
    let fresh = capture(&context, Value::string("expanded"));
    let partial = original.after_partial_walk(fresh).unwrap();
    assert!(partial.unchanged());
    assert!(!partial.can_replay());
    let history = FrameVisualHistory::default()
        .with_cursor_sources([(entry.window, partial)].into_iter().collect());
    let mut next = entry.retained.clone();
    next.key.window_start += 100;
    let mut prepared = PreparedViewports::default();
    prepared.accept(
        entry.frame,
        [(entry.window, entry.retained.clone())]
            .into_iter()
            .collect(),
        &[(entry.window, next)].into_iter().collect(),
        Some(&FrameFaceArena::default()),
        Some(&history),
    );
    assert!(prepared.entries.is_empty());
}

#[test]
fn dead_frame_retirement_releases_original_source_roots() {
    let (mut context, mut entry) = viewport();
    context.gc_collect_exact();
    let allocated =
        || unsafe { neovm_core::tagged::gc::with_tagged_heap(|heap| heap.allocated_count()) };
    let baseline = allocated();
    std::rc::Rc::make_mut(&mut entry.payload).cursor_sources = Some(capture(
        &context,
        Value::string("only prepared rows own this source"),
    ));
    let mut prepared = PreparedViewports::default();
    prepared.entries.push_back(entry);
    context.gc_collect_exact();
    assert_eq!(
        allocated(),
        baseline + 1,
        "original source must remain rooted with its rows"
    );
    prepared.retain_live_frames(|_| false);
    context.gc_collect_exact();
    assert_eq!(
        allocated(),
        baseline,
        "retiring the dead frame must release its source lease"
    );
}

#[test]
fn retiring_dead_frames_purges_entries_exports_and_invalidations() {
    let (context, mut entry) = viewport();
    let live_frame = neovm_core::window::FrameId(entry.frame.0 + 1);
    std::rc::Rc::make_mut(&mut entry.payload).cursor_sources =
        Some(capture(&context, Value::string("owned source")));
    let dead_frame = entry.frame;
    let window = entry.window;
    let export = |frame| ExportedCoverage {
        frame,
        window,
        key: entry.retained.key.clone(),
        epoch: 0,
        certified: false,
        last_export: None,
        backward_start: None,
        visible_source: 0..10,
    };
    let exports = vec![export(dead_frame), export(live_frame)];
    let mut prepared = PreparedViewports {
        entries: [entry].into_iter().collect(),
        exports,
        invalidated: vec![(dead_frame, window), (live_frame, window)],
    };
    prepared.retain_live_frames(|frame| frame == live_frame);
    assert!(prepared.entries.is_empty());
    assert_eq!(prepared.exports.len(), 1);
    assert_eq!(prepared.exports[0].frame, live_frame);
    assert_eq!(prepared.invalidated, vec![(live_frame, window)]);
}
