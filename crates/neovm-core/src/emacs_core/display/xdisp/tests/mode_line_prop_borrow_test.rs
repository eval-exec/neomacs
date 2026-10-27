//! Raw formatter differentials against explicit test-only property references.
//! Product source-slice grafting has one path; the two-step reference and its
//! typed scoped selector exist only in unit-test binaries.

use super::*;

#[derive(Clone, Copy, Debug)]
enum PropertyReadCase {
    Copied,
    Borrowed,
}

#[derive(Clone, Copy)]
enum PreviousBorrowOverride {
    ProcessDefault,
    Copied,
    Borrowed,
}

#[must_use]
struct BorrowGuard {
    previous: PreviousBorrowOverride,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl BorrowGuard {
    fn enter(case: PropertyReadCase) -> Self {
        let enabled = match case {
            PropertyReadCase::Copied => false,
            PropertyReadCase::Borrowed => true,
        };
        let previous =
            MODE_LINE_PROP_BORROW_OVERRIDE.with(|cell| match cell.replace(Some(enabled)) {
                None => PreviousBorrowOverride::ProcessDefault,
                Some(false) => PreviousBorrowOverride::Copied,
                Some(true) => PreviousBorrowOverride::Borrowed,
            });
        Self {
            previous,
            _thread: std::marker::PhantomData,
        }
    }
}

impl Drop for BorrowGuard {
    fn drop(&mut self) {
        let previous = match self.previous {
            PreviousBorrowOverride::ProcessDefault => None,
            PreviousBorrowOverride::Copied => Some(false),
            PreviousBorrowOverride::Borrowed => Some(true),
        };
        // Preserve the enclosing override, including during an unwinding test.
        // If this thread's TLS has retired, Drop has no live state to restore.
        let _ = MODE_LINE_PROP_BORROW_OVERRIDE.try_with(|cell| cell.set(previous));
    }
}

#[must_use]
struct SliceGuard {
    previous: ModeLineSourceSliceTestMode,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl SliceGuard {
    fn enter(mode: ModeLineSourceSliceTestMode) -> Self {
        Self {
            previous: replace_mode_line_source_slice_test_mode(mode),
            _thread: std::marker::PhantomData,
        }
    }
}

impl Drop for SliceGuard {
    fn drop(&mut self) {
        restore_mode_line_source_slice_test_mode(self.previous);
    }
}

static_assertions::assert_not_impl_any!(BorrowGuard: Send, Sync);
static_assertions::assert_not_impl_any!(SliceGuard: Send, Sync);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StringEncoding {
    Unibyte,
    Multibyte,
}

/// Owned bytes and explicit encoding, valid after the producing heap closes.
#[derive(Debug, PartialEq, Eq)]
struct StringRepresentation {
    bytes: Vec<u8>,
    encoding: StringEncoding,
    chars: CharLen,
}

impl StringRepresentation {
    fn capture(value: Value) -> Self {
        let string = value.as_lisp_string().expect("string representation");
        Self {
            bytes: string.as_bytes().to_vec(),
            encoding: if string.is_multibyte() {
                StringEncoding::Multibyte
            } else {
                StringEncoding::Unibyte
            },
            chars: CharLen::new(string.schars()),
        }
    }
}

impl From<&StringRepresentation> for Value {
    fn from(representation: &StringRepresentation) -> Self {
        let string = match representation.encoding {
            StringEncoding::Unibyte => {
                crate::heap_types::LispString::from_unibyte(representation.bytes.clone())
            }
            StringEncoding::Multibyte => {
                crate::heap_types::LispString::from_emacs_bytes(representation.bytes.clone())
            }
        };
        Value::heap_string(string)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct PropertyPairCount(usize);

#[derive(Debug, PartialEq, Eq)]
struct IntervalShape {
    range: CharRange,
    pairs: PropertyPairCount,
}

#[derive(Debug)]
struct StringTrace {
    representation: StringRepresentation,
    intervals: Vec<IntervalShape>,
    interval_plists: StringRepresentation,
    readable: StringRepresentation,
}

#[derive(Debug, PartialEq, Eq)]
struct SourceOrdinal(usize);

#[derive(Debug, PartialEq, Eq)]
enum SourceRole {
    OutputRoot,
    InputSource,
    OutputRootAndInputSource,
    Other,
}

impl SourceRole {
    fn of(source: Value, output: Value, input: Option<Value>) -> Self {
        match (source == output, input == Some(source)) {
            (true, true) => Self::OutputRootAndInputSource,
            (true, false) => Self::OutputRoot,
            (false, true) => Self::InputSource,
            (false, false) => Self::Other,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct SourceTopology {
    output: CharRange,
    source_start: CharPos0,
    ordinal: SourceOrdinal,
    role: SourceRole,
}

#[derive(Debug)]
struct SourceTrace {
    topology: SourceTopology,
    text: StringTrace,
}

#[derive(Debug, PartialEq, Eq)]
struct CallbackCount(i64);

#[derive(Debug)]
struct OutputTrace {
    text: StringTrace,
    sources: Vec<SourceTrace>,
    eval_count: CallbackCount,
    point: CharPos0,
}

#[derive(Clone, Copy)]
enum OutputPropertyContract {
    LiteralExactOrder,
    RawPropertyEquality,
}

fn read_representation(eval: &mut Context, printed: &StringRepresentation) -> Value {
    eval.set_variable("d5-borrow-readable", Value::from(printed));
    let value = eval
        .eval_str("(read d5-borrow-readable)")
        .expect("read full raw value");
    eval.push_specpdl_root(value);
    value
}

fn string_trace(eval: &mut Context, value: Value) -> StringTrace {
    let representation = StringRepresentation::capture(value);
    let intervals = borrow_string_text_properties_table_for_value(value)
        .map(|table| {
            table
                .interval_plist_runs_for_test()
                .into_iter()
                .map(|(start, end, pairs)| IntervalShape {
                    range: CharRange::new(CharPos0::new(start), CharPos0::new(end)),
                    pairs: PropertyPairCount(pairs.len()),
                })
                .collect()
        })
        .unwrap_or_default();
    // The caller has rooted the original string and every output sidecar.
    // No table borrow crosses any printer/reader/equality call.
    eval.set_variable("d5-borrow-result", value);
    let intervals_value = eval
        .eval_str(
            "(let ((print-circle t) (print-level nil) (print-length nil))
           (prin1-to-string (object-intervals d5-borrow-result)))",
        )
        .expect("raw object intervals");
    let interval_plists = StringRepresentation::capture(intervals_value);
    let printed_value = eval
        .eval_str(
            "(let ((print-circle t) (print-level nil) (print-length nil))
           (prin1-to-string d5-borrow-result))",
        )
        .expect("print full raw string");
    let readable = StringRepresentation::capture(printed_value);
    let roundtrip = read_representation(eval, &readable);
    eval.set_variable("d5-borrow-roundtrip", roundtrip);
    assert!(
        eval.eval_str("(equal-including-properties d5-borrow-result d5-borrow-roundtrip)")
            .expect("raw property roundtrip")
            .is_truthy(),
        "unfiltered string roundtrip"
    );
    assert_eq!(
        StringRepresentation::capture(roundtrip),
        representation,
        "roundtrip must preserve every byte, encoding and character"
    );
    StringTrace {
        representation,
        intervals,
        interval_plists,
        readable,
    }
}

fn output_trace(setup: &str, case: PropertyReadCase) -> OutputTrace {
    crate::test_utils::init_test_tracing();
    let _guard = BorrowGuard::enter(case);
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
    // Fresh :eval source strings are retained only by this sidecar. The RAII
    // extent roots the complete result before any introspection can collect.
    mode_line_walk::with_root_scope(&mut eval, |eval| {
        eval.push_specpdl_root(output.value());
        for span in output.source_spans() {
            eval.push_specpdl_root(span.source());
        }
        let input_source = eval.obarray.symbol_value_copied("d5-borrow-source");
        let mut identities = Vec::new();
        let text = string_trace(eval, output.value());
        let sources = output
            .source_spans()
            .iter()
            .map(|span| {
                let source = span.source();
                let ordinal =
                    if let Some(index) = identities.iter().position(|seen| *seen == source) {
                        SourceOrdinal(index)
                    } else {
                        identities.push(source);
                        SourceOrdinal(identities.len() - 1)
                    };
                SourceTrace {
                    topology: SourceTopology {
                        output: CharRange::new(
                            CharPos0::new(span.output_start()),
                            CharPos0::new(span.output_end()),
                        ),
                        source_start: CharPos0::new(span.source_start()),
                        ordinal,
                        role: SourceRole::of(source, output.value(), input_source),
                    },
                    text: string_trace(eval, source),
                }
            })
            .collect();
        let eval_count = CallbackCount(
            eval.obarray
                .symbol_value_copied("d5-borrow-evals")
                .and_then(Value::as_fixnum)
                .expect("eval counter"),
        );
        let point = eval
            .buffers
            .get(buffer_id)
            .expect("buffer")
            .point_char_pos();
        OutputTrace {
            text,
            sources,
            eval_count,
            point,
        }
    })
}

fn assert_raw_string_equal(eval: &mut Context, actual: &StringTrace, expected: &StringTrace) {
    assert_eq!(
        actual.representation, expected.representation,
        "all string bytes/encoding/characters"
    );
    assert_eq!(
        actual.intervals, expected.intervals,
        "all interval boundaries and plist pair counts"
    );
    let actual_value = read_representation(eval, &actual.readable);
    let expected_value = read_representation(eval, &expected.readable);
    eval.set_variable("d5-borrow-actual", actual_value);
    eval.set_variable("d5-borrow-expected", expected_value);
    assert!(
        eval.eval_str("(equal-including-properties d5-borrow-actual d5-borrow-expected)")
            .expect("raw property comparison")
            .is_truthy(),
        "unfiltered properties differ: actual={actual:?}, expected={expected:?}"
    );
}

fn assert_output_equal(
    actual: &OutputTrace,
    expected: &OutputTrace,
    contract: OutputPropertyContract,
) {
    let mut eval = interactive_context();
    mode_line_walk::with_root_scope(&mut eval, |eval| {
        assert_raw_string_equal(eval, &actual.text, &expected.text);
        match contract {
            OutputPropertyContract::LiteralExactOrder => assert_eq!(
                actual.text.interval_plists, expected.text.interval_plists,
                "exact literal property order"
            ),
            OutputPropertyContract::RawPropertyEquality => {}
        }
        assert_eq!(
            actual.eval_count, expected.eval_count,
            "callback population"
        );
        assert_eq!(actual.point, expected.point, "buffer point");
        assert_eq!(
            actual.sources.len(),
            expected.sources.len(),
            "all source spans"
        );
        for (actual, expected) in actual.sources.iter().zip(&expected.sources) {
            assert_eq!(
                actual.topology, expected.topology,
                "source position/identity topology"
            );
            assert_raw_string_equal(eval, &actual.text, &expected.text);
            assert_eq!(
                actual.text.interval_plists, expected.text.interval_plists,
                "exact source property order and values"
            );
        }
    });
}

fn compare(setup: &str) {
    let _slice = SliceGuard::enter(ModeLineSourceSliceTestMode::TwoStepReference);
    let copied = output_trace(setup, PropertyReadCase::Copied);
    let borrowed = output_trace(setup, PropertyReadCase::Borrowed);
    assert_output_equal(
        &borrowed,
        &copied,
        OutputPropertyContract::LiteralExactOrder,
    );
    let _slice = SliceGuard::enter(ModeLineSourceSliceTestMode::Production);
    for case in [PropertyReadCase::Copied, PropertyReadCase::Borrowed] {
        assert_output_equal(
            &output_trace(setup, case),
            &copied,
            OutputPropertyContract::LiteralExactOrder,
        );
    }
}

fn compare_percent_fields(setup: &str) {
    let _slice = SliceGuard::enter(ModeLineSourceSliceTestMode::TwoStepReference);
    let copied = output_trace(setup, PropertyReadCase::Copied);
    let copied_again = output_trace(setup, PropertyReadCase::Copied);
    assert_output_equal(
        &copied_again,
        &copied,
        OutputPropertyContract::RawPropertyEquality,
    );
    let borrowed = output_trace(setup, PropertyReadCase::Borrowed);
    assert_output_equal(
        &borrowed,
        &copied,
        OutputPropertyContract::RawPropertyEquality,
    );
    let _slice = SliceGuard::enter(ModeLineSourceSliceTestMode::Production);
    for case in [PropertyReadCase::Copied, PropertyReadCase::Borrowed] {
        assert_output_equal(
            &output_trace(setup, case),
            &copied,
            OutputPropertyContract::RawPropertyEquality,
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
