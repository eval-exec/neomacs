use super::*;
use neovm_core::emacs_core::{Context, Value};

fn viewport() -> (Context, PreparedViewport) {
    let mut context = Context::new();
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
            retained,
            faces,
            rows: 0,
            glyphs: 0,
            computed: false,
            complete_viewport: true,
            cursor_sources: None,
            query_freshness: None,
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
            entry.retained,
            entry.faces,
            reads.unwrap(),
            vec![context.retain_gc_roots([a])],
            true,
            None,
        ),
        "admission cannot rebase changed original producer reads"
    );
}

fn capture(context: &Context, value: Value) -> CursorSourceDependencies {
    let capture = crate::cursor_source_dependencies::Capture::begin(context);
    let _ = crate::cursor_source_dependencies::string_properties(value);
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
        let mut context = Context::new();
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
        [(entry.window, entry.retained)].into_iter().collect(),
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
    let allocated = || neovm_core::tagged::gc::with_tagged_heap(|heap| heap.allocated_count());
    let baseline = allocated();
    entry.cursor_sources = Some(capture(
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
    entry.cursor_sources = Some(capture(&context, Value::string("owned source")));
    let dead_frame = entry.frame;
    let window = entry.window;
    let export = |frame| ExportedCoverage {
        frame,
        window,
        key: entry.retained.key.clone(),
        epoch: 0,
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
