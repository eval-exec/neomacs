use super::*;
use neovm_core::emacs_core::value::list_to_vec;

use super::super::{
    ChromeRowInputs, DisplayRowFallbackMetrics, DisplayTabPolicy, ImageScaleEnvironment, Rect,
    ResolvedFace, WindowChromeKind,
};

fn inputs() -> ChromeRowInputs {
    ChromeRowInputs {
        kind: WindowChromeKind::ModeLine,
        display_row_index: 39,
        bounds: Rect::new(0.0, 624.0, 800.0, 16.0),
        text_area_left_px: 0.0,
        selected: true,
        metrics: DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        tab_policy: DisplayTabPolicy::from_tab_width_and_stops(0.0, 8, &[]),
        base_face: ResolvedFace::default(),
        image_scale_environment: ImageScaleEnvironment::default(),
        glyphless_table_bits: None,
        automatic_composition: false,
        face_change_count: 0,
        char_table_revision: neovm_core::window::CharTableLayoutRevision::default(),
        media_generation: 0,
        symbol_values: Box::new([]),
    }
}

type InputSpan = (Value, usize, usize, usize);

/// The two independent pre-optimization consumers, kept only as an equivalence
/// oracle. Expected transport representatives are compared by bits, not equal.
fn legacy(
    root: Value,
    spans: &[InputSpan],
) -> (
    Vec<ChromeSpanShape>,
    Option<usize>,
    Vec<usize>,
    Vec<GlyphStringId>,
    Vec<ChromeSourceSpan>,
) {
    let mut sources = Vec::new();
    let fingerprint = spans
        .iter()
        .map(|&(source, output_start, output_end, source_start)| {
            let index = match sources.iter().position(|seen| *seen == source) {
                Some(index) => index,
                None => {
                    sources.push(source);
                    sources.len() - 1
                }
            };
            ChromeSpanShape {
                output_start,
                output_end,
                source_start,
                source_index: index as u32,
            }
        })
        .collect();
    let root_index = sources.iter().position(|source| *source == root);
    let mut values = vec![root.bits()];
    let root_id = crate::display_row::root_lisp_string_id();
    let mut ids = vec![root_id];
    let mut source_ids: Vec<(Value, GlyphStringId)> = Vec::new();
    let transport = spans
        .iter()
        .map(|&(source, start, end, source_start)| {
            let id = if source == root {
                root_id
            } else if let Some((_, id)) = source_ids.iter().find(|(seen, _)| *seen == source) {
                *id
            } else {
                let id = GlyphStringId::new(source_ids.len() as u64 + 2);
                source_ids.push((source, id));
                values.push(source.bits());
                ids.push(id);
                id
            };
            ChromeSourceSpan::new(start, end, source_start, id)
        })
        .collect();
    (fingerprint, root_index, values, ids, transport)
}

fn compare(root: Value, spans: &[InputSpan]) -> ChromeSourceProjection {
    let owner = [root];
    let scope = MemoKeyScope::for_values(&owner);
    let actual = ChromeSourceProjection::classify(
        &scope,
        root,
        spans
            .iter()
            .map(|&(source, start, end, source_start)| ProjectionSourceSpan {
                source,
                output_start: CharPos0::new(start),
                output_end: CharPos0::new(end),
                source_start: CharPos0::new(source_start),
            }),
    );
    let expected = legacy(root, spans);
    assert_eq!(&*actual.fingerprint_spans, expected.0);
    assert_eq!(actual.root_index, expected.1);
    assert_eq!(
        actual
            .bindings
            .values
            .iter()
            .map(|value| value.bits())
            .collect::<Vec<_>>(),
        expected.2
    );
    assert_eq!(actual.bindings.string_ids, expected.3);
    assert_eq!(actual.bindings.spans, expected.4);
    actual
}

fn values(eval: &mut Context, form: &str) -> Vec<Value> {
    // Keep all fixture objects reachable through an evaluator variable. There
    // are no Lisp calls while the resulting unrooted plans are inspected.
    let value = eval.eval_str(form).expect("projection values");
    eval.set_variable("chrome-projection-values", value);
    list_to_vec(&value).expect("proper values list")
}

#[test]
fn source_projection_policy_is_explicit_and_invalid_values_opt_out() {
    use std::ffi::OsStr;
    assert_eq!(
        ChromeSourceProjectionMode::parse(None),
        ChromeSourceProjectionMode::On
    );
    for text in ["on", "1", " TRUE ", "yes"] {
        assert_eq!(
            ChromeSourceProjectionMode::parse(Some(OsStr::new(text))),
            ChromeSourceProjectionMode::On
        );
    }
    for text in ["off", "0", "false", "", "verify", "unexpected"] {
        assert_eq!(
            ChromeSourceProjectionMode::parse(Some(OsStr::new(text))),
            ChromeSourceProjectionMode::Off
        );
    }
}

#[test]
fn source_projection_empty_still_binds_the_root() {
    let mut eval = Context::new();
    let v = values(&mut eval, r#"(list "root")"#);
    let projection = compare(v[0], &[]);
    assert_eq!(projection.root_index, None);
    assert!(projection.fingerprint_spans.is_empty());
    assert_eq!(projection.bindings.values.len(), 1);
    assert_eq!(projection.bindings.string_ids[0].get(), 1);
}

#[test]
fn source_projection_root_late_preserves_both_numberings_and_span_offsets() {
    let mut eval = Context::new();
    let v = values(&mut eval, r#"(list "root" "a" "b")"#);
    let spans = [
        (v[1], 0, 2, 3),
        (v[2], 2, 4, 7),
        (v[0], 4, 8, 0),
        (v[1], 8, 9, 20),
    ];
    let projection = compare(v[0], &spans);
    assert_eq!(projection.root_index, Some(2));
    assert_eq!(
        projection
            .fingerprint_spans
            .iter()
            .map(|span| span.source_index)
            .collect::<Vec<_>>(),
        [0, 1, 2, 0]
    );
    assert_eq!(
        projection
            .bindings
            .spans
            .iter()
            .map(|span| span.string_id.get())
            .collect::<Vec<_>>(),
        [2, 3, 1, 2]
    );
    assert_eq!(
        projection.bindings.spans[0].source_position(1),
        Some((GlyphStringId::new(2), 4))
    );
    assert_eq!(projection.bindings.spans[0].source_position(2), None);
    assert_eq!(
        projection.bindings.spans[3].source_position(8),
        Some((GlyphStringId::new(2), 20))
    );
}

#[test]
fn source_projection_structural_equal_sources_keep_first_objects_despite_properties() {
    let mut eval = Context::new();
    let v = values(
        &mut eval,
        r#"(list (copy-sequence "root")
      (propertize "same" 'face 'bold 'help-echo "first" 'local-map (list 'keymap) 'keymap (list 'keymap))
      (propertize "same" 'face 'italic 'help-echo "second" 'local-map (list 'keymap (cons 'mouse-1 'ignore)) 'keymap (list 'keymap))
      (propertize "root" 'help-echo "distinct root alias"))"#,
    );
    assert_ne!(v[1].bits(), v[2].bits());
    assert_ne!(v[0].bits(), v[3].bits());
    let projection = compare(
        v[0],
        &[
            (v[1], 0, 4, 0),
            (v[2], 4, 8, 0),
            (v[3], 8, 12, 0),
            (v[1], 12, 16, 1),
        ],
    );
    assert_eq!(
        projection
            .bindings
            .values
            .iter()
            .map(|value| value.bits())
            .collect::<Vec<_>>(),
        [v[0].bits(), v[1].bits()]
    );
    assert_eq!(projection.root_index, Some(1));
    assert_eq!(
        projection
            .bindings
            .spans
            .iter()
            .map(|span| span.string_id.get())
            .collect::<Vec<_>>(),
        [2, 2, 1, 2]
    );
}

#[test]
fn source_projection_root_present_absent_and_repetition_match_existing_consumers() {
    let mut eval = Context::new();
    let v = values(&mut eval, r#"(list "root" "a" "b")"#);
    for spans in [
        vec![(v[0], 0, 4, 0)],
        vec![(v[1], 0, 1, 0), (v[2], 1, 2, 0)],
        vec![(v[1], 0, 1, 0); 1000],
    ] {
        compare(v[0], &spans);
    }
}

#[test]
fn source_projection_encoding_shapes_match_structural_equal_without_eq_shortcuts() {
    let mut eval = Context::new();
    let v = values(
        &mut eval,
        r#"(list "root" (unibyte-string 65) (string-to-multibyte "A")
      (unibyte-string 233) (string-to-multibyte "é")
      (propertize (unibyte-string 65) 'face 'bold)
      (propertize (string-to-multibyte "A") 'face 'italic)
      (propertize (unibyte-string 233) 'face 'bold)
      (propertize (string-to-multibyte "é") 'face 'italic)
      (unibyte-string 195 169))"#,
    );
    for (index, multibyte) in [
        (1, false),
        (2, true),
        (3, false),
        (4, true),
        (5, false),
        (6, true),
        (7, false),
        (8, true),
    ] {
        assert_eq!(
            v[index]
                .as_lisp_string()
                .expect("string fixture")
                .is_multibyte(),
            multibyte
        );
    }
    let spans: Vec<_> = v
        .iter()
        .copied()
        .enumerate()
        .skip(1)
        .map(|(index, value)| (value, index - 1, index, 0))
        .collect();
    let projection = compare(v[0], &spans);
    // ASCII byte/char contents equal across representations; non-ASCII raw
    // unibyte and UTF-8 bytes remain separate, exactly as ordinary equal.
    assert_eq!(
        projection
            .fingerprint_spans
            .iter()
            .map(|span| span.source_index)
            .collect::<Vec<_>>(),
        [0, 0, 1, 2, 0, 0, 1, 2, 3]
    );
}

#[test]
fn source_projection_fingerprint_modes_preserve_plain_and_refused_rows() {
    let mut eval = Context::new();
    for form in [
        r#""plain""#,
        r#"(propertize "x" 'face 'bold 'help-echo "fresh")"#,
        r#"(propertize "x" 'display '(space :align-to 10))"#,
        r#"(propertize "x" 'face (make-marker))"#,
    ] {
        let root = eval.eval_str(form).expect("chrome root");
        eval.set_variable("chrome-projection-root", root);
        let formatted = ModeLineDisplayOutput::from_root_string(root);
        let off = super::super::ChromeRowFingerprint::capture_with_sources(
            inputs(),
            &formatted,
            ChromeSourceProjectionMode::Off,
        );
        let on = super::super::ChromeRowFingerprint::capture_with_sources(
            inputs(),
            &formatted,
            ChromeSourceProjectionMode::On,
        );
        match (off, on) {
            (Some((off, None)), Some((on, Some(bindings)))) => {
                assert_eq!(off, on);
                assert_eq!(bindings.values[0].bits(), root.bits());
            }
            (None, None) => {}
            _ => panic!("projection changed eligibility or explicit policy"),
        }
    }
}

#[test]
fn source_projection_installs_fresh_first_representatives_before_collection() {
    let mut eval = Box::new(Context::new());
    eval.setup_thread_locals();
    let v = values(
        &mut eval,
        r#"(list (copy-sequence "root")
        (propertize "same" 'help-echo "first")
        (propertize "same" 'help-echo "second"))"#,
    );
    let root_bits = v[0].bits();
    let source_bits = v[1].bits();
    let projection = compare(v[0], &[(v[1], 0, 4, 0), (v[2], 4, 8, 0)]);
    let (presented, spans) = projection
        .bindings
        .into_rooted(PresentedWindowChromeArea::ModeLine, &eval)
        .expect("fresh roots");
    assert_eq!(presented.len(), 2);
    assert_eq!(spans.len(), 2);
    eval.set_variable("chrome-projection-values", Value::NIL);
    eval.gc_collect_exact();
    assert_eq!(
        eval.materialize(presented[0].object())
            .expect("root retained")
            .value()
            .bits(),
        root_bits
    );
    assert_eq!(
        eval.materialize(presented[1].object())
            .expect("first source retained")
            .value()
            .bits(),
        source_bits
    );
    assert_eq!(
        presented
            .iter()
            .map(|source| source.string_id().get())
            .collect::<Vec<_>>(),
        [1, 2]
    );
}

#[test]
fn source_projection_formatted_sidecar_preserves_eval_count_and_exact_bindings() {
    let mut eval = Box::new(Context::new());
    eval.setup_thread_locals();
    eval.set_variable("noninteractive", Value::NIL);
    let buffer_id = eval.buffer_manager().current_buffer().expect("buffer").id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("source-projection", 800, 600, buffer_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let format = eval.eval_str(r#"(progn
      (setq chrome-projection-evals 0)
      (setq chrome-projection-format
        (let ((a (propertize "same" 'help-echo "first" 'local-map (list 'keymap)))
              (b (propertize "same" 'help-echo "second" 'local-map (list 'keymap (cons 'mouse-1 'ignore)))))
          (list a b "é:%b" a
            '(:eval (progn (setq chrome-projection-evals (1+ chrome-projection-evals))
                           (propertize "fresh" 'help-echo "current")))))))"#).expect("format fixture");
    let formatted = neovm_core::emacs_core::xdisp::format_mode_line_for_display_with_sources(
        &mut eval,
        format,
        Value::make_window(window_id.0),
        Value::make_buffer(buffer_id),
        40,
    );
    // No Lisp calls or collection until the sidecar has fresh shared roots.
    let spans: Vec<_> = formatted
        .source_spans()
        .iter()
        .map(|span| {
            (
                span.source(),
                span.output_start(),
                span.output_end(),
                span.source_start(),
            )
        })
        .collect();
    assert!(
        spans.len() > 2,
        "actual formatter provenance, not just a root span"
    );
    let expected = legacy(formatted.value(), &spans);
    let (off, absent) = super::super::ChromeRowFingerprint::capture_with_sources(
        inputs(),
        &formatted,
        ChromeSourceProjectionMode::Off,
    )
    .expect("plain off");
    assert!(absent.is_none());
    let (on, bindings) = super::super::ChromeRowFingerprint::capture_with_sources(
        inputs(),
        &formatted,
        ChromeSourceProjectionMode::On,
    )
    .expect("plain shared");
    assert_eq!(off, on);
    let bindings = bindings.expect("shared bindings");
    assert_eq!(
        bindings
            .values
            .iter()
            .map(|value| value.bits())
            .collect::<Vec<_>>(),
        expected.2
    );
    assert_eq!(bindings.string_ids, expected.3);
    assert_eq!(bindings.spans, expected.4);
    let (_roots, _) = bindings
        .into_rooted(PresentedWindowChromeArea::ModeLine, &eval)
        .expect("root formatter output");
    assert_eq!(
        eval.eval_str("chrome-projection-evals")
            .expect("counter remains bound")
            .as_fixnum(),
        Some(1)
    );
}
