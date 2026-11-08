//! String tests whose harness is the runtime-startup image.
//!
//! The canonical hook proof is minted in core, so eval cannot construct
//! `runtime_startup_context`. These five tests only call that harness and
//! the public eval and value surfaces. The rest of `strings_test.rs` stays
//! in eval. Assertions are unchanged.

use neovm_value::heap_types::LispString;
use neovm_value::value::Value;

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

    neovm_value::test_support::authority::reset_position_conversion_scan_steps_for_test();
    let result = eval
        .funcall_general(
            Value::symbol("split-string"),
            vec![input, Value::string("\0"), Value::T],
        )
        .expect("split-string should succeed");
    let scan_steps = neovm_value::emacs_char::position_conversion_scan_steps_for_test();
    let fields =
        neovm_value::value::list_to_vec(&result).expect("split-string should return a proper list");

    assert_eq!(fields.len(), field_count);
    assert_eq!(
        scan_steps, 0,
        "GNU treats ASCII multibyte offsets as identity conversions; \
         split-string scanned {scan_steps} characters for {input_len} input bytes"
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

/// `string-lessp` over two genuinely multibyte strings, pinned to GNU 31.1.
///
/// GNU `string_cmp` (src/fns.c) skips the common byte prefix and then decodes
/// only the ONE character where the strings diverge.  It spells this case out
/// rather than reusing its `memcmp` arm for a reason: Emacs stores a raw
/// eight-bit byte as an overlong two-byte sequence, so byte order would sort
/// those between U+007F and U+0080 instead of above every real character.
///
/// The eight-bit rows are therefore the ones that matter -- they are what a
/// naive `memcmp` shortcut gets wrong, and they answer the same way in both
/// engines only because the divergent character is decoded.
#[test]
fn string_lessp_orders_multibyte_strings_like_gnu() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(let* ((raw (string-to-multibyte (unibyte-string #xC3)))
                 (u80 (string (decode-char 'ucs #x80)))
                 (u7f (string (decode-char 'ucs #x7f)))
                 (e "é") (u "ü"))
             (list
              ;; A raw eight-bit char sorts ABOVE every real character.
              (list (string-lessp raw u80) (string-lessp u80 raw))
              (list (string-lessp raw u7f) (string-lessp u7f raw))
              (list (string-lessp e u) (string-lessp u e))
              ;; Long shared prefix, difference in the last character.
              (let ((a (concat (make-string 50 ?é) "a"))
                    (b (concat (make-string 50 ?é) "b")))
                (list (string-lessp a b) (string-lessp b a)))
              ;; One string is a byte-prefix of the other.
              (let ((a (make-string 50 ?é))
                    (b (concat (make-string 50 ?é) "z")))
                (list (string-lessp a b) (string-lessp b a) (string-lessp a a)))
              ;; The difference falls INSIDE a character, not on a byte boundary.
              (let ((a (concat "xx" (string (decode-char 'ucs #x4e00))))
                    (b (concat "xx" (string (decode-char 'ucs #x4e01)))))
                (list (string-lessp a b) (string-lessp b a)))
              ;; A raw eight-bit char after a shared multibyte prefix: the
              ;; prefix skip must not carry the wrong comparison past it.
              (let ((a (concat (make-string 20 ?é) raw))
                    (b (concat (make-string 20 ?é) u80)))
                (list (string-lessp a b) (string-lessp b a)))
              ;; `value<' reads the same order.
              (list (value< raw u80) (value< e u))))"#,
    );
    assert_eq!(
        observed,
        "OK ((nil t) (nil t) (t nil) (t nil) (t nil nil) (t nil) (nil t) (nil t))"
    );
}
