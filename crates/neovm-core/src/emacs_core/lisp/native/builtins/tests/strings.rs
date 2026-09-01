use super::*;
use crate::buffer::CharRange;
use crate::heap_types::LispString;

fn put_string_property(
    table: &mut crate::buffer::text_props::TextPropertyTable,
    start: usize,
    end: usize,
    name: Value,
    value: Value,
) -> bool {
    table.put_property_in_char_range(CharRange::from_usize(start, end), name, value)
}

/// GNU `string_to_number` (`src/lread.c`) scans a numeric prefix directly;
/// it does not rebuild parser machinery for every obsolete-command version
/// inspected by M-x completion.  Neomacs keeps the established `regex` parser,
/// but the two immutable patterns must have process-wide ownership so repeated
/// calls cannot recompile their DFAs.
#[test]
fn string_to_number_reuses_process_wide_prefix_regexes() {
    let first = decimal_number_prefix_regexes();
    let second = decimal_number_prefix_regexes();

    assert!(std::ptr::eq(first, second));
    assert!(first.special_float.is_match("1.0e+INF"));
    assert!(first.number.is_match("-12.5tail"));
}

/// GNU `string_cmp` (`src/fns.c`) compares borrowed string storage in place.
/// M-x completion sorts thousands of command names with `string-lessp`, so the
/// comparison views must borrow both one-byte and multibyte representations.
#[test]
fn string_ordering_views_borrow_string_storage() {
    let one_byte = LispString::from_unibyte(b"alpha".to_vec());
    let multibyte = LispString::from_utf8("α");

    match StringOrderingView::new(&one_byte) {
        StringOrderingView::OneByteChars(bytes) => {
            assert!(std::ptr::eq(bytes, one_byte.as_bytes()));
        }
        StringOrderingView::MultibyteChars(_) => panic!("unibyte storage must stay one-byte"),
    }
    match StringOrderingView::new(&multibyte) {
        StringOrderingView::MultibyteChars(chars) => {
            assert!(std::ptr::eq(chars.bytes, multibyte.as_bytes()));
            assert_eq!(chars.position, 0);
        }
        StringOrderingView::OneByteChars(_) => panic!("non-ASCII storage must be decoded lazily"),
    }
}

/// GNU compares one-byte strings by byte value, but decodes genuinely
/// multibyte strings before comparing.  In particular, unibyte 0x80 sorts
/// before the multibyte raw-byte character made from 0x80 (character
/// 0x3FFF80), while ordinary Unicode U+00E9 sorts before unibyte 0xFF.
#[test]
fn string_ordering_preserves_gnu_mixed_encoding_order() {
    use std::cmp::Ordering;

    let ascii_unibyte = LispString::from_unibyte(b"a".to_vec());
    let ascii_multibyte = LispString::from_utf8("a");
    let unibyte_80 = LispString::from_unibyte(vec![0x80]);
    let multibyte_raw_80 = LispString::from_emacs_bytes(vec![0xc0, 0x80]);
    let unicode_e_acute = LispString::from_utf8("é");
    let unibyte_ff = LispString::from_unibyte(vec![0xff]);

    assert_eq!(
        string_ordering(&ascii_unibyte, &ascii_multibyte),
        Ordering::Equal
    );
    assert_eq!(
        string_ordering(&unibyte_80, &multibyte_raw_80),
        Ordering::Less
    );
    assert_eq!(
        string_ordering(&multibyte_raw_80, &unibyte_80),
        Ordering::Greater
    );
    assert_eq!(
        string_ordering(&unicode_e_acute, &unibyte_ff),
        Ordering::Less
    );
}

/// GNU's `SYMBOL_NAME` supplies the original Lisp string object even when its
/// bytes are not UTF-8.  Comparison must not round-trip a symbol name through
/// Rust `str` while resolving the string designator.
#[test]
fn string_ordering_accepts_raw_unibyte_symbol_names_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = eval
        .eval_str(
            r#"(let* ((name (unibyte-string 255))
                      (sym (intern name)))
                 (list (eq name (symbol-name sym))
                       (string-lessp sym (unibyte-string 255))
                       (string-lessp (unibyte-string 254) sym)
                       ;; `string-greaterp' is lisp/subr.el:6283 and reads
                       ;; `(string-lessp string2 string1)'; DIVERGENCES.md 152.
                       ;; (`not' is subr.el's too -- 148 -- so spell it out.)
                       (if (string-lessp sym (unibyte-string 254)) nil t)))"#,
        )
        .expect("raw unibyte symbol names remain valid string designators");

    assert_eq!(
        crate::emacs_core::print::print_value(&result),
        "(t nil t t)"
    );
}

/// GNU `styled_format` (editfns.c) replaces a SYMBOL argument with
/// `SYMBOL_NAME (arg)` -- the symbol's actual name STRING object -- before it
/// decides anything else, so from that point the argument IS a string and its
/// text properties propagate into the result like any string argument's.
///
/// A symbol interned from propertized text keeps those properties on its name
/// (GNU `Fintern` stores the string it was given), which is how an error message
/// built with `(error "..." SYM)` carries the `fontified` property the symbol was
/// read from -- the elisp-slime-nav divergence. Printing the symbol afresh
/// instead loses them.
#[test]
fn format_percent_s_takes_a_symbols_properties_from_its_name_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let rendered = eval
        .eval_str(
            r#"(prin1-to-string
                 (let* ((name (propertize "p15-fmt-symbol" 'fontified nil))
                        (sym (intern name)))
                   (list (text-properties-at 0 (format "%s" sym))
                         ;; `%S' prints a fresh representation in GNU too, so it
                         ;; carries nothing -- the two conversions must differ.
                         (text-properties-at 0 (format "%S" sym))
                         ;; The offset case the package hits: the symbol lands in
                         ;; the middle of a longer message.
                         (text-properties-at 6 (format "find: %s" sym)))))"#,
        )
        .unwrap();
    // One property only: with several, GNU's own plist ORDER differs between
    // these two calls, so ordering is not a contract worth pinning.
    assert_eq!(
        rendered.as_str_owned().as_deref(),
        Some("((fontified nil) nil (fontified nil))"),
    );
}

#[test]
fn substring_preserves_raw_unibyte_storage_semantics() {
    crate::test_utils::init_test_tracing();

    let source = Value::heap_string(LispString::from_unibyte(vec![0xff, b'a', b'b']));
    let result = builtin_substring(vec![source, Value::fixnum(1), Value::fixnum(3)])
        .expect("substring should accept raw unibyte storage");
    let string = result
        .as_lisp_string()
        .expect("substring should return a string");

    assert!(!string.is_multibyte());
    assert_eq!(string.as_bytes(), b"ab");
}

#[test]
fn substring_can_return_raw_non_utf8_unibyte_bytes() {
    crate::test_utils::init_test_tracing();

    let source = Value::heap_string(LispString::from_unibyte(vec![0xff, 0xfe, b'x']));
    let result = builtin_substring(vec![source, Value::fixnum(0), Value::fixnum(2)])
        .expect("substring should slice raw unibyte bytes");
    let string = result
        .as_lisp_string()
        .expect("substring should return a string");

    assert!(!string.is_multibyte());
    assert_eq!(string.as_bytes(), &[0xff, 0xfe]);
}

#[test]
fn substring_copies_text_properties_through_gnu_add_properties_order() {
    crate::test_utils::init_test_tracing();

    let mut eval = crate::emacs_core::eval::Context::new();
    let result = eval
        .eval_str(
            r#"(let* ((s (propertize "abcdef" 'face 'bold 'tag 'source))
                      (sub (substring s 1 5)))
                 (text-properties-at 0 sub))"#,
        )
        .expect("evaluation succeeds");

    assert_eq!(
        crate::emacs_core::print::print_value(&result),
        "(tag source face bold)"
    );
}

#[test]
fn split_ascii_multibyte_string_uses_gnu_identity_position_conversion() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let field_count = 256;
    let input = (0..field_count)
        .map(|index| format!("src/module-{index:04}/file.rs\0"))
        .collect::<String>();
    let input_len = input.len();
    let input = Value::heap_string(LispString::from_utf8(&input));

    crate::emacs_core::emacs_char::reset_position_conversion_scan_steps_for_test();
    let result = eval
        .funcall_general(
            Value::symbol("split-string"),
            vec![input, Value::string("\0"), Value::T],
        )
        .expect("split-string should succeed");
    let scan_steps = crate::emacs_core::emacs_char::position_conversion_scan_steps_for_test();
    let fields = crate::emacs_core::value::list_to_vec(&result)
        .expect("split-string should return a proper list");

    assert_eq!(fields.len(), field_count);
    assert_eq!(
        scan_steps, 0,
        "GNU treats ASCII multibyte offsets as identity conversions; \
         split-string scanned {scan_steps} characters for {input_len} input bytes"
    );
}

#[test]
fn concat_preserves_multibyte_text_properties_as_char_intervals() {
    crate::test_utils::init_test_tracing();

    let source = Value::string("é");
    let mut table = crate::buffer::text_props::TextPropertyTable::new();
    put_string_property(
        &mut table,
        0,
        1,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(source, table);

    let result = builtin_concat(vec![Value::string("x"), source, Value::string("z")])
        .expect("concat should preserve string properties");
    let props = crate::emacs_core::value::get_string_text_properties_table_for_value(result)
        .expect("result should carry text properties");
    let intervals = props.intervals_snapshot();

    assert_eq!(intervals.len(), 1);
    assert_eq!((intervals[0].start, intervals[0].end), (1, 2));
    assert_eq!(
        intervals[0].properties.get(&Value::symbol("face")),
        Some(&Value::symbol("bold"))
    );
}

#[test]
fn concat_applies_source_plists_in_gnu_add_properties_order() {
    crate::test_utils::init_test_tracing();

    let mut eval = crate::emacs_core::eval::Context::new();
    let result = eval
        .eval_str(
            r#"(let* ((source (propertize "a" 'face 'bold 'tag 'source))
                      (result (concat source)))
                 (text-properties-at 0 result))"#,
        )
        .expect("evaluation succeeds");

    assert_eq!(
        crate::emacs_core::print::print_value(&result),
        "(tag source face bold)"
    );
}

#[test]
fn concat_builds_many_disjoint_property_runs_without_incremental_tree_balancing() {
    crate::test_utils::init_test_tracing();

    let sources = (0..128)
        .map(|source_index| {
            let source = Value::string("abcd");
            let mut table = crate::buffer::text_props::TextPropertyTable::new();
            for char_index in 0..4 {
                put_string_property(
                    &mut table,
                    char_index,
                    char_index + 1,
                    Value::symbol("face"),
                    Value::fixnum((source_index * 4 + char_index) as i64),
                );
            }
            crate::emacs_core::value::set_string_text_properties_table_for_value(source, table);
            source
        })
        .collect::<Vec<_>>();

    crate::buffer::text_props::reset_interval_balance_calls_for_test();
    let result = builtin_concat(sources).expect("concat should preserve all property runs");
    let balance_calls = crate::buffer::text_props::interval_balance_calls_for_test();

    assert_eq!(
        result
            .as_lisp_string()
            .expect("concat returns a string")
            .schars(),
        512
    );
    assert_eq!(
        balance_calls, 0,
        "concat owns all shifted runs up front and should build one balanced tree"
    );
}

#[test]
fn concat_promotes_unibyte_high_bytes_when_result_is_multibyte_like_gnu() {
    crate::test_utils::init_test_tracing();

    let raw = Value::heap_string(LispString::from_unibyte(vec![0xff]));
    let result = builtin_concat(vec![Value::string("é"), raw])
        .expect("concat should promote raw unibyte bytes");
    let string = result.as_lisp_string().expect("concat returns a string");

    assert!(string.is_multibyte());
    assert_eq!(string.schars(), 2);
    assert_eq!(string.sbytes(), 4);
    assert_eq!(string.as_bytes(), &[0xc3, 0xa9, 0xc1, 0xbf]);
}

#[test]
fn concat_all_unibyte_strings_with_properties_stays_unibyte_like_gnu() {
    crate::test_utils::init_test_tracing();

    let raw = Value::heap_string(LispString::from_unibyte(vec![0xff]));
    let suffix = Value::heap_string(LispString::from_unibyte(vec![b'a']));
    let mut table = crate::buffer::text_props::TextPropertyTable::new();
    put_string_property(
        &mut table,
        0,
        1,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(raw, table);

    let result = builtin_concat(vec![raw, suffix]).expect("concat should preserve unibyte storage");
    let string = result.as_lisp_string().expect("concat returns a string");
    let props = crate::emacs_core::value::get_string_text_properties_table_for_value(result)
        .expect("result should carry text properties");
    let intervals = props.intervals_snapshot();

    assert!(!string.is_multibyte());
    assert_eq!(string.as_bytes(), &[0xff, b'a']);
    assert_eq!(intervals.len(), 1);
    assert_eq!((intervals[0].start, intervals[0].end), (0, 1));
    assert_eq!(
        intervals[0].properties.get(&Value::symbol("face")),
        Some(&Value::symbol("bold"))
    );
}

#[test]
fn format_preserves_multibyte_text_properties_as_char_intervals() {
    crate::test_utils::init_test_tracing();

    let source = Value::string("éz");
    let mut table = crate::buffer::text_props::TextPropertyTable::new();
    put_string_property(
        &mut table,
        0,
        1,
        Value::symbol("face"),
        Value::symbol("bold"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(source, table);

    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = builtin_format_wrapper_strict_slice(&mut ctx, &[Value::string("%4s"), source])
        .expect("format should preserve string properties");
    let props = crate::emacs_core::value::get_string_text_properties_table_for_value(result)
        .expect("result should carry text properties");
    let intervals = props.intervals_snapshot();

    assert_eq!(result.as_utf8_str(), Some("  éz"));
    assert_eq!(intervals.len(), 1);
    assert_eq!((intervals[0].start, intervals[0].end), (2, 3));
    assert_eq!(
        intervals[0].properties.get(&Value::symbol("face")),
        Some(&Value::symbol("bold"))
    );
}

/// GNU `styled_format` returns the format string ITSELF when nothing was
/// formatted — `if (! new_result) { val = args[0]; goto return_val; }`
/// (editfns.c) — so the property-copy block never runs and the plist keeps its
/// supplied order. `new_result` is set only by a `%` conversion, `%%`, curly
/// quote translation, or a raw-byte conversion.
///
/// Org reaches this path with `(user-error (substitute-command-keys "…"))`:
/// the propertized string is the FORMAT STRING and there are no directives.
/// Neomacs rebuilt the string and applied the (correct) additive transfer, so
/// the help-key plist came out reversed and 14 oracle tests went red.
///
/// Verified against GNU Emacs 31.0.90 in `--batch -Q`:
///
///   (format (propertize "no directives here" 'f1 1 'f2 2 'f3 3))
///     => properties (f1 1 f2 2 f3 3), and `eq` to the input string
#[test]
fn format_without_directives_returns_the_format_string_unchanged_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();

    let preserved = ctx
        .eval_str(
            r#"(format "%S" (text-properties-at
                 0 (format (propertize "no directives here" 'f1 1 'f2 2 'f3 3))))"#,
        )
        .expect("format of a directive-free string should succeed");
    assert_eq!(
        preserved.as_utf8_str(),
        Some("(f1 1 f2 2 f3 3)"),
        "nothing was formatted, so GNU copies no properties and the supplied \
         order survives"
    );

    // GNU returns the very same object in that case.
    let identical = ctx
        .eval_str(r#"(let ((s (propertize "no directives here" 'f1 1))) (eq s (format s)))"#)
        .expect("eq check should succeed");
    assert!(!identical.is_nil(), "GNU returns args[0] itself");

    // The additive transfer that `4b6132cea` fixed must still reverse when the
    // format actually formats something (ac-helm / ac-php parity).
    let reversed = ctx
        .eval_str(
            r#"(format "%S" (text-properties-at
                 0 (format (propertize "X%sY" 'f1 1 'f2 2 'f3 3) "a")))"#,
        )
        .expect("format with a directive should succeed");
    assert_eq!(
        reversed.as_utf8_str(),
        Some("(f3 3 f2 2 f1 1)"),
        "a real conversion still copies the format plist with GNU's \
         additive-prepend order"
    );
}

#[test]
fn format_reverses_percent_s_text_property_plist_order_like_gnu_add_properties() {
    crate::test_utils::init_test_tracing();

    let source = Value::string("key");
    let mut table = crate::buffer::text_props::TextPropertyTable::new();
    put_string_property(
        &mut table,
        0,
        3,
        Value::symbol("face"),
        Value::symbol("help-key-binding"),
    );
    put_string_property(
        &mut table,
        0,
        3,
        Value::symbol("font-lock-face"),
        Value::symbol("help-key-binding"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(source, table);

    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = builtin_format_wrapper_strict_slice(&mut ctx, &[Value::string("%s ok"), source])
        .expect("format should preserve string properties");
    let props = crate::emacs_core::value::get_string_text_properties_table_for_value(result)
        .expect("result should carry text properties");
    let intervals = props.intervals_snapshot();
    let ordered_keys: Vec<_> = intervals[0]
        .ordered_properties()
        .map(|(name, _)| name.as_symbol_name().unwrap().to_string())
        .collect();

    assert_eq!(result.as_utf8_str(), Some("key ok"));
    assert_eq!(
        ordered_keys,
        vec!["face".to_string(), "font-lock-face".to_string()]
    );
}

#[test]
fn format_exact_percent_s_reuses_the_string_argument_like_gnu() {
    crate::test_utils::init_test_tracing();

    let source = Value::string("key");
    let mut table = crate::buffer::text_props::TextPropertyTable::new();
    put_string_property(
        &mut table,
        0,
        3,
        Value::symbol("face"),
        Value::symbol("help-key-binding"),
    );
    put_string_property(
        &mut table,
        0,
        3,
        Value::symbol("font-lock-face"),
        Value::symbol("help-key-binding"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(source, table);

    let mut ctx = crate::emacs_core::eval::Context::new();
    let formatted =
        builtin_format_wrapper_strict_slice(&mut ctx, &[Value::string("%s"), source]).unwrap();
    let message = builtin_format_message_slice(&mut ctx, &[Value::string("%s"), source]).unwrap();

    assert_eq!(formatted, source);
    assert_eq!(message, source);
}

#[test]
fn format_percent_s_prints_interpreted_closure_slots_without_string_quotes() {
    crate::test_utils::init_test_tracing();

    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = ctx
        .eval_str(
            r##"(list
  (format "%s"
          (lambda ()
            (interactive)
            (message "Rollback the current deployment")))
  (format "%S"
          (lambda ()
            (interactive)
            (message "Rollback the current deployment"))))"##,
        )
        .expect("format should print interpreted functions");

    assert_eq!(
        crate::emacs_core::print::print_value(&result),
        r##"("#[nil ((message Rollback the current deployment)) nil nil nil nil]" "#[nil ((message \"Rollback the current deployment\")) nil nil nil nil]")"##,
    );
}

#[test]
fn format_percent_s_promotes_result_when_printer_outputs_non_ascii_text() {
    crate::test_utils::init_test_tracing();

    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = builtin_format_wrapper_strict_slice(
        &mut ctx,
        &[
            Value::string("%S"),
            Value::list(vec![
                Value::string("é"),
                Value::fixnum(233),
                Value::string("é"),
            ]),
        ],
    )
    .expect("format should evaluate");
    let string = result.as_lisp_string().expect("format returns a string");

    assert!(string.is_multibyte());
    assert_eq!(result.as_utf8_str(), Some("(\"é\" 233 \"é\")"));
    assert_eq!(string.as_bytes(), b"(\"\xc3\xa9\" 233 \"\xc3\xa9\")");
}

#[test]
fn format_percent_c_matches_gnu_non_ascii_multibyte_width_and_precision() {
    crate::test_utils::init_test_tracing();

    fn run_format(fmt: &str, arg: Value) -> LispString {
        let mut ctx = crate::emacs_core::eval::Context::new();
        let result = builtin_format_wrapper_strict_slice(&mut ctx, &[Value::string(fmt), arg])
            .expect("format should evaluate");
        result
            .as_lisp_string()
            .expect("format should return a string")
            .clone()
    }

    fn codes(string: &LispString) -> Vec<u32> {
        if !string.is_multibyte() {
            return string.as_bytes().iter().map(|byte| *byte as u32).collect();
        }
        let mut out = Vec::new();
        let mut pos = 0usize;
        while pos < string.as_bytes().len() {
            let (code, len) = crate::emacs_core::emacs_char::string_char(&string.as_bytes()[pos..]);
            out.push(code);
            pos += len;
        }
        out
    }

    let ascii = run_format("%05c", Value::fixnum(b'x' as i64));
    assert!(!ascii.is_multibyte());
    assert_eq!(ascii.as_bytes(), b"    x");

    let latin1 = run_format("%c", Value::fixnum(0x80));
    assert!(latin1.is_multibyte());
    assert_eq!(latin1.as_bytes(), &[0xc2, 0x80]);
    assert_eq!(codes(&latin1), vec![0x80]);

    let nonunicode = run_format("%2c", Value::fixnum(0x11_0000));
    assert!(nonunicode.is_multibyte());
    assert_eq!(nonunicode.as_bytes(), &[b' ', 0xf4, 0x90, 0x80, 0x80]);
    assert_eq!(codes(&nonunicode), vec![b' ' as u32, 0x11_0000]);

    let empty_nonunicode = run_format("%.0c", Value::fixnum(0x11_0000));
    assert!(empty_nonunicode.is_multibyte());
    assert!(empty_nonunicode.as_bytes().is_empty());

    let padded_empty_nonunicode = run_format("%2.0c", Value::fixnum(0x11_0000));
    assert!(padded_empty_nonunicode.is_multibyte());
    assert_eq!(padded_empty_nonunicode.as_bytes(), b"  ");
}

#[test]
fn format_string_width_and_precision_use_gnu_display_width() {
    crate::test_utils::init_test_tracing();

    let mut ctx = crate::emacs_core::eval::Context::new();
    let wide_padded =
        builtin_format_wrapper_strict_slice(&mut ctx, &[Value::string("%3s"), Value::string("界")])
            .expect("format should evaluate");
    assert_eq!(wide_padded.as_utf8_str(), Some(" 界"));

    let wide_truncated = builtin_format_wrapper_strict_slice(
        &mut ctx,
        &[Value::string("%.1s"), Value::string("界")],
    )
    .expect("format should evaluate");
    let wide_truncated = wide_truncated
        .as_lisp_string()
        .expect("format should return a string");
    assert!(wide_truncated.is_multibyte());
    assert!(wide_truncated.as_bytes().is_empty());
}

#[test]
fn format_percent_g_uses_gnu_fixed_precision_for_negative_exponents() {
    crate::test_utils::init_test_tracing();

    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = builtin_format_wrapper_strict_slice(
        &mut ctx,
        &[
            Value::string("%.2g %.2g %.2g"),
            Value::make_float(0.00042),
            Value::make_float(0.0042),
            Value::make_float(42.0),
        ],
    )
    .expect("format should evaluate");

    assert_eq!(result.as_utf8_str(), Some("0.00042 0.0042 42"));
}

#[test]
fn format_rejects_uppercase_float_conversions() {
    // GNU `Fformat`/`doprnt` only treats lowercase e/f/g as float conversions
    // (`float_conversion` in editfns.c); uppercase E/G (like F/A) are invalid
    // and must signal `(error "Invalid format operation %X")`.
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();

    fn invalid_format_message(ctx: &mut crate::emacs_core::eval::Context, fmt: &str) -> String {
        let err = builtin_format_wrapper_strict_slice(
            ctx,
            &[Value::string(fmt), Value::make_float(1.5e-20)],
        )
        .expect_err("uppercase float conversion must signal an error");
        match err {
            crate::emacs_core::error::Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "error", "fmt: {fmt}");
                sig.data
                    .first()
                    .and_then(|v| v.as_utf8_str())
                    .expect("error message string")
                    .to_string()
            }
            other => panic!("expected a signal for {fmt}, got {other:?}"),
        }
    }

    assert_eq!(
        invalid_format_message(&mut ctx, "%E"),
        "Invalid format operation %E"
    );
    assert_eq!(
        invalid_format_message(&mut ctx, "%G"),
        "Invalid format operation %G"
    );
    assert_eq!(
        invalid_format_message(&mut ctx, "%.3G"),
        "Invalid format operation %G"
    );
    assert_eq!(
        invalid_format_message(&mut ctx, "%010.2E"),
        "Invalid format operation %E"
    );

    // Lowercase float conversions still work, byte-identical to before.
    let ok = |ctx: &mut crate::emacs_core::eval::Context, fmt: &str| {
        builtin_format_wrapper_strict_slice(ctx, &[Value::string(fmt), Value::make_float(1.5e-20)])
            .expect("lowercase float conversion should succeed")
            .as_utf8_str()
            .map(str::to_string)
    };
    assert_eq!(ok(&mut ctx, "%e").as_deref(), Some("1.500000e-20"));
    assert_eq!(ok(&mut ctx, "%g").as_deref(), Some("1.5e-20"));
    assert_eq!(
        builtin_format_wrapper_strict_slice(
            &mut ctx,
            &[Value::string("%f"), Value::make_float(1.5)]
        )
        .expect("eval")
        .as_utf8_str(),
        Some("1.500000")
    );
}

#[test]
fn downcase_greek_final_sigma() {
    // GNU `casefiddle.c` `case_character`: a down-cased capital sigma becomes
    // the final form ς at the end of a word (preceding char is a word
    // constituent, following one is not), σ otherwise.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let cases = [
        (r#"(downcase "ΑΣ")"#, "ας"),       // Σ ends the word → ς
        (r#"(downcase "ΣΑ")"#, "σα"),       // Σ starts the word → σ
        (r#"(downcase "ΑΣΑ")"#, "ασα"),     // medial Σ → σ
        (r#"(downcase "Σ")"#, "σ"),         // lone Σ (no preceding word) → σ
        (r#"(downcase "ΟΔΟΣ")"#, "οδος"),   // word ends Σ → ς
        (r#"(downcase "ΑΣ ΒΣ")"#, "ας βς"), // both at word ends → ς
        (r#"(downcase "ΑΣ_")"#, "ας_"),     // _ is a non-word boundary → ς
        // case-symbols-as-words makes _ a word constituent → not at word end → σ
        (
            r#"(let ((case-symbols-as-words t)) (downcase "ΑΣ_"))"#,
            "ασ_",
        ),
        // upcase is unaffected.
        (r#"(upcase "ας")"#, "ΑΣ"),
    ];
    for (form, expected) in cases {
        let result = ev.eval_str(form).expect("eval");
        assert_eq!(result.as_utf8_str(), Some(expected), "form: {form}");
    }
}

/// `string-width' must measure a TAB using the dynamically-bound `tab-width',
/// not a hardcoded 8 -- mirroring GNU `lisp_string_width' -> `char_width' ->
/// `CHARACTER_WIDTH' returning `SANE_TAB_WIDTH(current_buffer)'.
#[test]
fn string_width_honors_dynamic_tab_width() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();

    let cases = [
        // (form, expected) -- verified against GNU `emacs --batch'.
        (r#"(let ((tab-width 2)) (string-width "\t"))"#, 2),
        (r#"(let ((tab-width 4)) (string-width "a\tb"))"#, 6),
        (r#"(let ((tab-width 8)) (string-width "\t"))"#, 8),
        // Default (unbound/8) tab and `char-width' parity controls.
        (r#"(string-width "\t")"#, 8),
        (r#"(let ((tab-width 2)) (char-width ?\t))"#, 2),
        // No-tab strings must be unaffected by tab-width.
        (r#"(let ((tab-width 2)) (string-width "abc"))"#, 3),
        (r#"(let ((tab-width 2)) (string-width "漢字"))"#, 4),
    ];
    for (form, expected) in cases {
        let result = ev.eval_str(form).expect("eval");
        assert_eq!(result.as_fixnum(), Some(expected), "form: {form}");
    }
}

/// GNU `print_prepare' (print.c) binds `print-escape-nonascii' only when the
/// destination buffer is multibyte, so a unibyte string's high bytes print raw
/// into a unibyte buffer but octal-escaped into a multibyte buffer.
#[test]
fn prin1_unibyte_string_into_unibyte_buffer_prints_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();

    // Unibyte string (#xc3 #xa9) into a UNIBYTE buffer: raw `"<c3><a9>"' = 4 bytes.
    let unibyte = ev
        .eval_str(
            r#"(with-temp-buffer
                 (set-buffer-multibyte nil)
                 (prin1 (unibyte-string 195 169) (current-buffer))
                 (buffer-size))"#,
        )
        .expect("eval");
    assert_eq!(
        unibyte.as_fixnum(),
        Some(4),
        "unibyte target prints raw bytes"
    );

    // Same string into a MULTIBYTE buffer: octal-escaped `"\303\251"' = 10 bytes.
    let multibyte = ev
        .eval_str(
            r#"(with-temp-buffer
                 (prin1 (unibyte-string 195 169) (current-buffer))
                 (buffer-size))"#,
        )
        .expect("eval");
    assert_eq!(
        multibyte.as_fixnum(),
        Some(10),
        "multibyte target octal-escapes high bytes"
    );

    // Multibyte-result printer paths (GNU prints into the multibyte
    // `Vprin1_to_string_buffer', binding `print-escape-nonascii') stay escaped.
    // (form, expected printed result) -- verified against GNU `emacs --batch'.
    let escaped_cases: &[(&str, &str)] = &[
        (
            r#"(prin1-to-string (unibyte-string 195 169))"#,
            r#""\"\\303\\251\"""#,
        ),
        (
            r#"(format "%S" (unibyte-string 195 169))"#,
            r#""\"\\303\\251\"""#,
        ),
        (
            r#"(condition-case e (error "%S" (unibyte-string 195 169))
                 (error (error-message-string e)))"#,
            r#""\"\\303\\251\"""#,
        ),
        // Normal (multibyte / ASCII) content is unaffected by the fix.
        (r#"(prin1-to-string "café")"#, r#""\"café\"""#),
        (r#"(format "%S" "abc")"#, r#""\"abc\"""#),
    ];
    for (form, expected) in escaped_cases {
        let result = ev.eval_str(form).expect("eval");
        assert_eq!(
            crate::emacs_core::print::print_value(&result),
            *expected,
            "form: {form}"
        );
    }
}

/// GNU `styled_format` carries the FORMAT STRING's own text properties into
/// the result (`src/editfns.c:4303-4377`).  Its `discarded[]` scan gives one
/// character of a conversion specification a special role: the spec's first
/// character is `discarded[] == 1`, and walking past it jumps `translated`
/// over the whole converted field.  A property boundary that falls inside the
/// spec therefore lands at the field's END, not its start.
///
/// `xref--insert-xrefs` depends on exactly that.  It formats a propertized
/// format string twice —
/// `(format (format #("%%%dd:" 0 4 (face xref-line-number) 5 6 (face shadow)) W) LINE)`
/// — and the second `format`'s `xref-line-number` range covers `"%1d"`, whose
/// end is interior to the spec.  Mapping that end to the field START collapses
/// the range to nothing and the line number renders unfaced, which is how the
/// `("3" xref-line-number)` run went missing from ggtags' `*xref*` buffer.
///
/// Every expected value here was produced by running the form under GNU Emacs
/// 31.0.90.
#[test]
fn format_maps_format_string_properties_across_conversion_fields_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::emacs_core::eval::Context::new();
    let cases: &[(&str, &str)] = &[
        // xref's first pass: build the width-specific line format.
        (
            r#"(format #("%%%dd:" 0 4 (face xref-line-number) 5 6 (face shadow)) 1)"#,
            r##"#("%1d:" 0 2 (face xref-line-number) 3 4 (face shadow))"##,
        ),
        // xref's second pass: this is the one that used to lose the face.
        (
            r#"(format #("%1d:" 0 2 (face xref-line-number) 3 4 (face shadow)) 3)"#,
            r##"#("3:" 0 1 (face xref-line-number) 1 2 (face shadow))"##,
        ),
        (
            r#"(format #("%%%dd:" 0 4 (face xref-line-number) 5 6 (face shadow)) 2)"#,
            r##"#("%2d:" 0 2 (face xref-line-number) 3 4 (face shadow))"##,
        ),
        // Padding belongs to the field, so the property covers it too.
        (
            r#"(format #("%2d:" 0 3 (face xref-line-number) 3 4 (face shadow)) 7)"#,
            r##"#(" 7:" 0 2 (face xref-line-number) 2 3 (face shadow))"##,
        ),
        (
            r#"(format #("%5dX" 0 2 (face f2)) 7)"#,
            r##"#("    7X" 0 5 (face f2))"##,
        ),
        // A boundary that STARTS inside the spec also lands at the field end.
        (
            r#"(format #("a%dbb" 2 4 (face f6)) 9)"#,
            r##"#("a9bb" 2 3 (face f6))"##,
        ),
        // `%%` is not a conversion field: its discarded `%` has nothing to
        // jump over, so a boundary between the two `%`s stays put and the
        // property collapses to nothing.
        (r#"(format #("%%x" 0 1 (face f1)))"#, r#""%x""#),
    ];
    for (form, expected) in cases {
        let result = ev.eval_str(form).expect("eval");
        assert_eq!(
            crate::emacs_core::print::print_value(&result),
            *expected,
            "form: {form}"
        );
    }
}
