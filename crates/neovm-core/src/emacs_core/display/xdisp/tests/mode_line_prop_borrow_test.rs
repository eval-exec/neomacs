//! Cost-only mode-line property reads, compared with the copied baseline.
//! Both selectors are forced in every differential; public GNU oracle tests
//! remain the external behavior gate for the formatter.

use super::*;

struct BorrowGuard;

struct SliceGuard;

impl SliceGuard {
    fn set(enabled: bool) -> Self {
        set_mode_line_prop_slice_for_test(Some(enabled));
        Self
    }
}

impl Drop for SliceGuard {
    fn drop(&mut self) {
        set_mode_line_prop_slice_for_test(None);
    }
}

impl BorrowGuard {
    fn set(enabled: bool) -> Self {
        set_mode_line_prop_borrow_for_test(Some(enabled));
        Self
    }
}

impl Drop for BorrowGuard {
    fn drop(&mut self) {
        set_mode_line_prop_borrow_for_test(None);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct StringTrace {
    bytes: Vec<u8>,
    multibyte: bool,
    char_len: usize,
    interval_plists: String,
}

#[derive(Debug, PartialEq, Eq)]
struct SourceTrace {
    output_start: usize,
    output_end: usize,
    source_start: usize,
    source_ordinal: usize,
    is_root: bool,
    is_input_source: bool,
    text: StringTrace,
}

#[derive(Debug, PartialEq, Eq)]
struct OutputTrace {
    text: StringTrace,
    sources: Vec<SourceTrace>,
    eval_count: i64,
    point: usize,
}

#[derive(Clone, Copy)]
enum PropertyOrder {
    Exact,
    Canonical,
}

fn string_trace(eval: &mut Context, value: Value, order: PropertyOrder) -> StringTrace {
    let string = value.as_lisp_string().expect("mode-line string");
    let bytes = string.as_bytes().to_vec();
    let multibyte = string.is_multibyte();
    let char_len = string.schars();
    // No interval borrow crosses these Lisp calls. Literal-only cases keep
    // exact plist order. Percent expansion's existing HashMap overlay order
    // is random even for two copied-baseline walks, so those comparisons sort
    // property pairs within each output run, retaining boundaries and values.
    // Original source strings always use Exact below.
    eval.set_variable("d5-borrow-result", value);
    let inspect = match order {
        PropertyOrder::Exact => "(prin1-to-string (object-intervals d5-borrow-result))",
        PropertyOrder::Canonical => {
            r#"(prin1-to-string
          (mapcar
           (lambda (run)
             (let ((plist (nth 2 run)) (pairs nil))
               (while plist
                 (setq pairs (cons (cons (car plist) (car (cdr plist))) pairs))
                 (setq plist (cdr (cdr plist))))
               (list (car run) (car (cdr run))
                     (apply #'append
                            (mapcar (lambda (pair) (list (car pair) (cdr pair)))
                                    (sort pairs
                                          (lambda (a b)
                                            (string-lessp (prin1-to-string (car a))
                                                          (prin1-to-string (car b))))))))))
           (object-intervals d5-borrow-result)))"#
        }
    };
    let interval_plists = eval
        .eval_str(inspect)
        .expect("object intervals")
        .as_utf8_str()
        .expect("printed intervals")
        .to_owned();
    StringTrace {
        bytes,
        multibyte,
        char_len,
        interval_plists,
    }
}

fn output_trace(setup: &str, enabled: bool, order: PropertyOrder) -> OutputTrace {
    crate::test_utils::init_test_tracing();
    let _guard = BorrowGuard::set(enabled);
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("property-borrow", 800, 600, buffer_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    eval.buffers
        .get_mut(buffer_id)
        .expect("buffer")
        .insert("first\nsecond\n");
    eval.eval_str("(setq d5-borrow-evals 0)")
        .expect("eval counter");
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'A', 0xff, b'B', b'%', b'b', b'Z', 0x80,
    ]));
    eval.set_variable("d5-borrow-raw", raw);
    let format = eval.eval_str(setup).expect("format setup");
    eval.set_variable("d5-borrow-format", format);
    let output = format_mode_line_for_display_with_sources(
        &mut eval,
        format,
        Value::make_window(window_id.0),
        Value::make_buffer(buffer_id),
        24,
    );
    // Fresh :eval strings are owned only by the output sidecar. Root the
    // complete sidecar before introspection calls that could collect.
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(output.value());
    for span in output.source_spans() {
        eval.push_specpdl_root(span.source());
    }
    let input_source = eval.obarray.symbol_value_copied("d5-borrow-source");
    let mut identities = Vec::new();
    let text = string_trace(&mut eval, output.value(), order);
    let sources = output
        .source_spans()
        .iter()
        .map(|span| {
            let source = span.source();
            let source_ordinal =
                if let Some(index) = identities.iter().position(|seen| *seen == source) {
                    index
                } else {
                    identities.push(source);
                    identities.len() - 1
                };
            SourceTrace {
                output_start: span.output_start(),
                output_end: span.output_end(),
                source_start: span.source_start(),
                source_ordinal,
                is_root: source == output.value(),
                is_input_source: input_source == Some(source),
                text: string_trace(&mut eval, source, PropertyOrder::Exact),
            }
        })
        .collect();
    let eval_count = eval
        .obarray
        .symbol_value_copied("d5-borrow-evals")
        .and_then(Value::as_fixnum)
        .expect("eval counter");
    let point = eval
        .buffers
        .get(buffer_id)
        .expect("buffer")
        .point_char_pos()
        .get();
    eval.restore_specpdl_roots(root_scope);
    OutputTrace {
        text,
        sources,
        eval_count,
        point,
    }
}

fn compare(setup: &str) {
    let _slice = SliceGuard::set(false);
    let copied = output_trace(setup, false, PropertyOrder::Exact);
    let borrowed = output_trace(setup, true, PropertyOrder::Exact);
    assert_eq!(borrowed, copied, "format setup: {setup}");
    let _slice = SliceGuard::set(true);
    for borrow in [false, true] {
        assert_eq!(
            output_trace(setup, borrow, PropertyOrder::Exact),
            copied,
            "slice/borrow={borrow}: {setup}"
        );
    }
}

fn compare_percent_fields(setup: &str) {
    let _slice = SliceGuard::set(false);
    let copied = output_trace(setup, false, PropertyOrder::Canonical);
    let copied_again = output_trace(setup, false, PropertyOrder::Canonical);
    assert_eq!(
        copied_again, copied,
        "copied baseline must reproduce: {setup}"
    );
    let borrowed = output_trace(setup, true, PropertyOrder::Canonical);
    assert_eq!(borrowed, copied, "format setup: {setup}");
    let _slice = SliceGuard::set(true);
    for borrow in [false, true] {
        assert_eq!(
            output_trace(setup, borrow, PropertyOrder::Canonical),
            copied,
            "slice/borrow={borrow}: {setup}"
        );
    }
}

#[test]
fn literal_multibyte_properties_and_repeated_source_identity_match_copied() {
    compare(
        r#"(progn
      (setq d5-borrow-source (propertize "中文 été" 'face 'bold 'help-echo "literal"))
      (list d5-borrow-source "|" d5-borrow-source))"#,
    );
}

#[test]
fn literal_raw_unibyte_properties_match_copied() {
    compare(
        r#"(progn
      (setq d5-borrow-source d5-borrow-raw)
      (put-text-property 0 (length d5-borrow-source) 'face 'italic d5-borrow-source)
      (list "" 'd5-borrow-source))"#,
    );
}

#[test]
fn percent_fragments_field_padding_and_raw_bytes_match_copied() {
    compare_percent_fields(
        r#"(progn
      (setq d5-borrow-source (propertize "é:%12b|%l:%c:%Z!" 'face 'bold 'help-echo "field"))
      (list d5-borrow-source d5-borrow-raw))"#,
    );
}

#[test]
fn nested_propertize_min_width_and_precision_match_copied() {
    compare_percent_fields(
        r#"(progn
      (setq d5-borrow-source (propertize "é:%b!" 'face 'bold 'mouse-face 'highlight))
      (list '(:propertize "AB" display (min-width (7.0)) face italic help-echo "first")
            '(:propertize "Y" display (min-width (4.0)) help-echo "second")
            (list -4 d5-borrow-source)
            (list 12 (list :propertize d5-borrow-source 'face 'italic))))"#,
    );
}

#[test]
fn eval_mutating_later_source_properties_preserves_capture_timing() {
    compare_percent_fields(
        r#"(progn
      (setq d5-borrow-source (propertize "A%12bZ" 'face 'bold 'help-echo "before"))
      (list d5-borrow-source
            '(:eval (progn
                      (setq d5-borrow-evals (1+ d5-borrow-evals))
                      (put-text-property 0 (length d5-borrow-source) 'face 'italic d5-borrow-source)
                      (put-text-property 0 (length d5-borrow-source) 'help-echo "after" d5-borrow-source)
                      d5-borrow-source))
            d5-borrow-source))"#,
    );
}

#[test]
fn eval_fresh_mouse_targets_preserve_source_identity_topology() {
    compare_percent_fields(
        r#"(list
      '(:eval (progn
                (setq d5-borrow-evals (1+ d5-borrow-evals))
                (propertize (copy-sequence "E") 'help-echo (number-to-string d5-borrow-evals)
                            'local-map '(keymap (mouse-1 . ignore)))))
      '(:eval (progn
                (setq d5-borrow-evals (1+ d5-borrow-evals))
                (propertize (copy-sequence "E") 'help-echo (number-to-string d5-borrow-evals)
                            'local-map '(keymap (mouse-1 . ignore)))))
      "%-")"#,
    );
}
