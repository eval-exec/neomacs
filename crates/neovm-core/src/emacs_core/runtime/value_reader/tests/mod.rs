use super::*;
use crate::emacs_core::intern::resolve_sym;
use crate::emacs_core::value::{Value, ValueKind};

/// Helper: read one form from a string, panic on error.
fn read1(input: &str) -> Value {
    let ob = crate::emacs_core::symbol::Obarray::new();
    let result = read_one(input, 0, &ob).expect("read_one failed");
    result.expect("no form found").0
}

/// Helper: read all forms from a string, panic on error.
fn read_all_ok(input: &str) -> Vec<Value> {
    let ob = crate::emacs_core::symbol::Obarray::new();
    read_all(input, &ob).expect("read_all failed")
}

// ---------------------------------------------------------------------------
// Integers
// ---------------------------------------------------------------------------

#[test]
fn integer_zero() {
    crate::test_utils::init_test_tracing();
    let v = read1("0");
    assert_eq!(v.as_fixnum(), Some(0));
}

#[test]
fn integer_positive() {
    crate::test_utils::init_test_tracing();
    let v = read1("42");
    assert_eq!(v.as_fixnum(), Some(42));
}

#[test]
fn integer_negative() {
    crate::test_utils::init_test_tracing();
    let v = read1("-17");
    assert_eq!(v.as_fixnum(), Some(-17));
}

#[test]
fn integer_hex() {
    crate::test_utils::init_test_tracing();
    let v = read1("0xFF");
    assert!(v.is_symbol_named("0xFF"));
}

#[test]
fn integer_hex_hash() {
    crate::test_utils::init_test_tracing();
    let v = read1("#xFF");
    assert_eq!(v.as_fixnum(), Some(255));
}

#[test]
fn integer_octal_hash() {
    crate::test_utils::init_test_tracing();
    let v = read1("#o77");
    assert_eq!(v.as_fixnum(), Some(63));
}

#[test]
fn integer_binary_hash() {
    crate::test_utils::init_test_tracing();
    let v = read1("#b1010");
    assert_eq!(v.as_fixnum(), Some(10));
}

// ---------------------------------------------------------------------------
// Floats
// ---------------------------------------------------------------------------

#[test]
fn float_simple() {
    crate::test_utils::init_test_tracing();
    let v = read1("3.125");
    assert_eq!(v.as_number_f64(), Some(3.125));
}

#[test]
fn float_exponent() {
    crate::test_utils::init_test_tracing();
    let v = read1("1e10");
    assert_eq!(v.as_number_f64(), Some(1e10));
}

/// GNU `read0` (`src/lread.c`) treats every character code <= 32, plus
/// NO-BREAK SPACE, as reader whitespace.  This matters for fixed-width wire
/// fields such as BERT's OLD_FLOAT_EXT: the decimal token is followed by NUL
/// padding, and `read` must stop before that padding rather than intern a
/// float-looking symbol containing control characters.
#[test]
fn c0_controls_and_no_break_space_separate_atoms_like_gnu() {
    crate::test_utils::init_test_tracing();

    let forms = read_all_ok("19.875\0\u{1}\u{1f} \u{a0}tail");

    assert_eq!(forms.len(), 2);
    assert_eq!(forms[0].as_number_f64(), Some(19.875));
    assert!(forms[0].is_float());
    assert!(forms[1].is_symbol_named("tail"));
}

#[test]
fn float_negative() {
    crate::test_utils::init_test_tracing();
    let v = read1("-2.5");
    assert_eq!(v.as_number_f64(), Some(-2.5));
}

#[test]
fn float_infinity() {
    crate::test_utils::init_test_tracing();
    let v = read1("1.0e+INF");
    assert_eq!(v.as_number_f64(), Some(f64::INFINITY));
}

#[test]
fn float_neg_infinity() {
    crate::test_utils::init_test_tracing();
    let v = read1("-1.0e+INF");
    assert_eq!(v.as_number_f64(), Some(f64::NEG_INFINITY));
}

#[test]
fn float_nan() {
    crate::test_utils::init_test_tracing();
    let v = read1("0.0e+NaN");
    assert!(v.as_number_f64().unwrap().is_nan());
}

/// GNU `string_to_number` (`src/lread.c`) lexes a digit run followed by a
/// single trailing "." (no fractional digits, no exponent) as an INTEGER —
/// the dot is a terminator. Only "5.0"/"5e0"/"5.e0"/".5" are floats.
///   GNU oracle: (read "5.") => 5 (integer); (read "5.0") => 5.0 (float)
///               (read "1000000000000000000000.") => 1000000000000000000000
#[test]
fn integer_trailing_dot_is_not_a_float() {
    crate::test_utils::init_test_tracing();

    // "5." => integer 5, not the float 5.0.
    let v = read1("5.");
    assert_eq!(v.as_fixnum(), Some(5));
    assert!(!v.is_float(), "\"5.\" must not be a float, got {v:?}");

    // Signed trailing-dot integers.
    assert_eq!(read1("-5.").as_fixnum(), Some(-5));
    assert_eq!(read1("+5.").as_fixnum(), Some(5));
    assert_eq!(read1("100.").as_fixnum(), Some(100));

    // A magnitude wider than a fixnum becomes a bignum integer, not 1e+21.
    let big = read1("1000000000000000000000.");
    assert!(
        big.is_integer() && !big.is_float(),
        "\"1000000000000000000000.\" must be a bignum integer, got {big:?}"
    );
    assert_eq!(
        crate::emacs_core::print_value(&big),
        "1000000000000000000000"
    );

    // Sanity: genuine floats still read as floats.
    assert_eq!(read1("5.0").as_number_f64(), Some(5.0));
    assert!(read1("5.0").is_float());
    assert_eq!(read1("5e0").as_number_f64(), Some(5.0));
    assert!(read1("5e0").is_float());
    assert_eq!(read1("5.e0").as_number_f64(), Some(5.0));
    assert!(read1("5.e0").is_float());
    assert_eq!(read1(".5").as_number_f64(), Some(0.5));
    assert!(read1(".5").is_float());
}

// ---------------------------------------------------------------------------
// Symbols
// ---------------------------------------------------------------------------

#[test]
fn symbol_simple() {
    crate::test_utils::init_test_tracing();
    let v = read1("foo");
    assert!(v.is_symbol_named("foo"));
}

#[test]
fn symbol_with_dashes() {
    crate::test_utils::init_test_tracing();
    let v = read1("some-symbol-name");
    assert!(v.is_symbol_named("some-symbol-name"));
}

#[test]
fn symbol_t() {
    crate::test_utils::init_test_tracing();
    let v = read1("t");
    assert_eq!(v, Value::T);
}

#[test]
fn symbol_nil() {
    crate::test_utils::init_test_tracing();
    let v = read1("nil");
    assert_eq!(v, Value::NIL);
}

#[test]
fn symbol_escaped() {
    crate::test_utils::init_test_tracing();
    let v = read1(r"a\ b");
    assert!(v.is_symbol_named("a b"));
}

#[test]
fn symbol_escaped_decimal_digits_are_not_numbers() {
    crate::test_utils::init_test_tracing();
    let v = read1(r"\12345678");
    assert!(v.is_symbol_named("12345678"));
}

#[test]
fn symbol_escaped_hex_digits_are_not_numbers() {
    crate::test_utils::init_test_tracing();
    let v = read1(r"\0xFF");
    assert!(v.is_symbol_named("0xFF"));
}

// ---------------------------------------------------------------------------
// Keywords
// ---------------------------------------------------------------------------

#[test]
fn keyword_simple() {
    crate::test_utils::init_test_tracing();
    let v = read1(":foo");
    assert!(v.is_keyword());
    let id = v.as_keyword_id().unwrap();
    assert_eq!(resolve_sym(id), ":foo");
}

#[test]
fn keyword_bare_colon() {
    crate::test_utils::init_test_tracing();
    let v = read1(":");
    assert!(v.is_keyword());
}

// ---------------------------------------------------------------------------
// Strings
// ---------------------------------------------------------------------------

#[test]
fn string_simple() {
    crate::test_utils::init_test_tracing();
    let v = read1(r#""hello""#);
    assert_eq!(v.as_utf8_str().unwrap(), "hello");
}

#[test]
fn ascii_string_literal_is_read_as_unibyte() {
    crate::test_utils::init_test_tracing();
    let v = read1(r#""hello""#);
    let ls = v.as_lisp_string().expect("string literal");
    assert_eq!(ls.as_bytes(), b"hello");
    assert!(!ls.is_multibyte());
}

#[test]
fn empty_string_literal_is_read_as_unibyte() {
    crate::test_utils::init_test_tracing();
    let v = read1(r#""""#);
    let ls = v.as_lisp_string().expect("string literal");
    assert!(ls.as_bytes().is_empty());
    assert!(!ls.is_multibyte());
}

#[test]
fn string_escapes() {
    crate::test_utils::init_test_tracing();
    let v = read1(r#""a\nb\t""#);
    assert_eq!(v.as_utf8_str().unwrap(), "a\nb\t");
}

/// In a string literal GNU drops `\<SPC>` and `\<LF>` entirely (whitespace /
/// line continuation): `read_string_literal` in `src/lread.c` has
///   case ' ': case '\n': ... continue;
/// `\<TAB>` and `\<CR>` are NOT dropped (they keep the literal char).
///   GNU oracle: (length (read "\"a\\ b\"")) => 2   ;; backslash-space
///               (length (read "\"a\\\nb\"")) => 2  ;; backslash-newline
///               (length (read "\"a\\\tb\"")) => 3  ;; backslash-tab kept
#[test]
fn string_backslash_space_is_dropped_like_backslash_newline() {
    crate::test_utils::init_test_tracing();

    // Backslash-space is dropped: "a\ b" => "ab".
    let space = read1("\"a\\ b\"");
    assert_eq!(space.as_utf8_str(), Some("ab"));

    // Backslash-newline is also dropped (pre-existing behavior).
    let newline = read1("\"a\\\nb\"");
    assert_eq!(newline.as_utf8_str(), Some("ab"));

    // Backslash-tab is NOT dropped by GNU: the literal tab is kept.
    let tab = read1("\"a\\\tb\"");
    assert_eq!(tab.as_utf8_str(), Some("a\tb"));
}

#[test]
fn string_space_escape_wins_over_super_modifier_prefix() {
    crate::test_utils::init_test_tracing();
    let v = read1(r#""^[\s-]*$""#);
    let ls = v.as_lisp_string().expect("string literal");
    assert_eq!(ls.as_bytes(), b"^[ -]*$");
    assert!(!ls.is_multibyte());
}

#[test]
fn string_old_control_escape_matches_gnu_reader() {
    crate::test_utils::init_test_tracing();

    let assert_string_bytes = |source: &str, expected: &[u8]| {
        let v = read1(source);
        let ls = v.as_lisp_string().expect("string literal");
        assert_eq!(ls.as_bytes(), expected);
        assert!(!ls.is_multibyte());
    };

    assert_string_bytes(r#""\^l""#, &[0x0C]);
    assert_string_bytes(r#""\^?""#, &[0x7F]);
    assert_string_bytes(r#""\^@""#, &[0]);
    assert_string_bytes(r#""\^ ""#, &[0]);
}

#[test]
fn string_literal_preserves_literal_carriage_return() {
    crate::test_utils::init_test_tracing();
    let v = read1("\"a\rb\"");
    let ls = v.as_lisp_string().expect("string literal");
    assert_eq!(ls.as_bytes(), b"a\rb");
    assert!(!ls.is_multibyte());
}

#[test]
fn string_hex_escape() {
    crate::test_utils::init_test_tracing();
    let v = read1(r#""\x41""#);
    assert_eq!(v.as_utf8_str().unwrap(), "A");
}

#[test]
fn string_short_hex_escape_uses_gnu_raw_byte_character() {
    crate::test_utils::init_test_tracing();
    let v = read1(r#""\xc3\xa9B""#);
    let string = v.as_lisp_string().expect("string literal");
    assert!(string.is_multibyte());
    assert_eq!(
        crate::emacs_core::builtins::lisp_string_char_codes(string),
        vec![crate::emacs_core::emacs_char::byte8_to_char(0xC3), 0xA9B]
    );
}

#[test]
fn string_short_octal_escape_uses_gnu_raw_byte_character() {
    crate::test_utils::init_test_tracing();
    let v = read1(r#""\303\u0100""#);
    let string = v.as_lisp_string().expect("string literal");
    assert!(string.is_multibyte());
    assert_eq!(
        crate::emacs_core::builtins::lisp_string_char_codes(string),
        vec![crate::emacs_core::emacs_char::byte8_to_char(0xC3), 0x100]
    );
}

#[test]
fn string_unicode_escape() {
    crate::test_utils::init_test_tracing();
    let v = read1(r#""\u0041""#);
    assert_eq!(v.as_utf8_str().unwrap(), "A");
}

#[test]
fn string_named_unicode_escape_matches_gnu_reader() {
    crate::test_utils::init_test_tracing();
    let v = read1(r#""\N{COLON}\N{fullwidth   colon}""#);
    assert_eq!(v.as_utf8_str().unwrap(), ":：");
}

#[test]
fn string_octal_escape() {
    crate::test_utils::init_test_tracing();
    let v = read1(r#""\101""#);
    assert_eq!(v.as_utf8_str().unwrap(), "A");
}

// ---------------------------------------------------------------------------
// Character literals
// ---------------------------------------------------------------------------

#[test]
fn char_literal_simple() {
    crate::test_utils::init_test_tracing();
    let v = read1("?A");
    assert_eq!(v.as_fixnum(), Some(65));
}

#[test]
fn char_literal_hex_byte8_is_canonicalized_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(read1(r"?\xE6").as_fixnum(), Some(0xE6));
    assert_eq!(read1(r"?\346").as_fixnum(), Some(0xE6));
    assert_eq!(
        read1(r"?\M-\xE6").as_fixnum(),
        Some((CHAR_META_MODIFIER | 0xE6) as i64)
    );
    assert_eq!(
        read1(r"?\C-\xE6").as_fixnum(),
        Some((CHAR_CTRL_MODIFIER | 0xE6) as i64)
    );
}

#[test]
fn char_literal_space() {
    crate::test_utils::init_test_tracing();
    let v = read1("? ");
    assert_eq!(v.as_fixnum(), Some(32));
}

#[test]
fn char_literal_newline() {
    crate::test_utils::init_test_tracing();
    let v = read1("?\\n");
    assert_eq!(v.as_fixnum(), Some(10));
}

#[test]
fn char_literal_tab() {
    crate::test_utils::init_test_tracing();
    let v = read1("?\\t");
    assert_eq!(v.as_fixnum(), Some(9));
}

#[test]
fn char_literal_control() {
    crate::test_utils::init_test_tracing();
    // \C-a should be 1
    let v = read1("?\\C-a");
    assert_eq!(v.as_fixnum(), Some(1));
}

#[test]
fn char_literal_old_control_escape_matches_gnu_reader() {
    crate::test_utils::init_test_tracing();
    assert_eq!(read1("?\\^l").as_fixnum(), Some(12));
    assert_eq!(read1("?\\^?").as_fixnum(), Some(127));
    assert_eq!(read1("?\\^@").as_fixnum(), Some(0));
}

#[test]
fn char_literal_named_unicode_escape_matches_gnu_reader() {
    crate::test_utils::init_test_tracing();
    assert_eq!(read1(r"?\N{COLON}").as_fixnum(), Some(58));
    assert_eq!(read1(r"?\N{fullwidth   colon}").as_fixnum(), Some(65306));
    assert_eq!(read1(r"?\N{U+003A}").as_fixnum(), Some(58));
}

#[test]
fn char_literal_named_unicode_escape_keeps_gnu_exact_name_rules() {
    crate::test_utils::init_test_tracing();
    assert_eq!(read1(r"?\N{BACKSPACE}").as_fixnum(), Some(8));
    assert_eq!(read1(r"?\N{BELL}").as_fixnum(), Some(128276));
    assert_eq!(read1(r"?\N{BELL (BEL)}").as_fixnum(), Some(7));
    assert_eq!(
        read1(r"?\N{GREEK SMALL LETTER LAMBDA}").as_fixnum(),
        Some(955)
    );

    for input in [
        r"?\N{latinsmalllettera}",
        r"?\N{LATIN_SMALL_LETTER_A}",
        r"?\N{u+003A}",
        r"?\N{}",
        r"?\N{Aé}",
    ] {
        assert!(
            read_one(input, 0, &crate::emacs_core::symbol::Obarray::new()).is_err(),
            "reader should reject GNU-incompatible named escape {input:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Quote syntax
// ---------------------------------------------------------------------------

#[test]
fn quote_form() {
    crate::test_utils::init_test_tracing();
    let v = read1("'foo");
    // Should be (quote foo)
    assert!(v.is_cons());
    let car = v.cons_car();
    assert!(car.is_symbol_named("quote"));
    let cadr = v.cons_cdr().cons_car();
    assert!(cadr.is_symbol_named("foo"));
}

#[test]
fn backquote_form() {
    crate::test_utils::init_test_tracing();
    let v = read1("`foo");
    assert!(v.is_cons());
    let car = v.cons_car();
    assert!(car.is_symbol_named("`"));
}

#[test]
fn unquote_form() {
    crate::test_utils::init_test_tracing();
    let v = read1(",foo");
    assert!(v.is_cons());
    let car = v.cons_car();
    assert!(car.is_symbol_named(","));
}

#[test]
fn splice_form() {
    crate::test_utils::init_test_tracing();
    let v = read1(",@foo");
    assert!(v.is_cons());
    let car = v.cons_car();
    assert!(car.is_symbol_named(",@"));
}

#[test]
fn function_quote() {
    crate::test_utils::init_test_tracing();
    let v = read1("#'foo");
    assert!(v.is_cons());
    let car = v.cons_car();
    assert!(car.is_symbol_named("function"));
    let cadr = v.cons_cdr().cons_car();
    assert!(cadr.is_symbol_named("foo"));
}

// ---------------------------------------------------------------------------
// Lists
// ---------------------------------------------------------------------------

#[test]
fn empty_list() {
    crate::test_utils::init_test_tracing();
    let v = read1("()");
    assert!(v.is_nil());
}

#[test]
fn proper_list() {
    crate::test_utils::init_test_tracing();
    let v = read1("(1 2 3)");
    // Should be a cons chain: (1 . (2 . (3 . nil)))
    assert!(v.is_cons());
    assert_eq!(v.cons_car().as_fixnum(), Some(1));
    let cdr1 = v.cons_cdr();
    assert_eq!(cdr1.cons_car().as_fixnum(), Some(2));
    let cdr2 = cdr1.cons_cdr();
    assert_eq!(cdr2.cons_car().as_fixnum(), Some(3));
    assert!(cdr2.cons_cdr().is_nil());
}

#[test]
fn dotted_pair() {
    crate::test_utils::init_test_tracing();
    let v = read1("(1 . 2)");
    assert!(v.is_cons());
    assert_eq!(v.cons_car().as_fixnum(), Some(1));
    assert_eq!(v.cons_cdr().as_fixnum(), Some(2));
}

#[test]
fn dotted_list() {
    crate::test_utils::init_test_tracing();
    let v = read1("(1 2 . 3)");
    assert!(v.is_cons());
    assert_eq!(v.cons_car().as_fixnum(), Some(1));
    let cdr1 = v.cons_cdr();
    assert_eq!(cdr1.cons_car().as_fixnum(), Some(2));
    assert_eq!(cdr1.cons_cdr().as_fixnum(), Some(3));
}

#[test]
fn dotted_tail_before_unquote_without_whitespace() {
    crate::test_utils::init_test_tracing();
    let v = read1("(a .,_ )");
    assert!(v.is_cons());
    assert!(v.cons_car().is_symbol_named("a"));
    let tail = v.cons_cdr();
    assert!(tail.is_cons());
    assert!(tail.cons_car().is_symbol_named(","));
    assert!(tail.cons_cdr().cons_car().is_symbol_named("_"));
}

#[test]
fn dotted_tail_before_reader_prefix_without_whitespace() {
    crate::test_utils::init_test_tracing();
    let v = read1("(a .`b)");
    assert!(v.is_cons());
    assert!(v.cons_car().is_symbol_named("a"));
    let tail = v.cons_cdr();
    assert!(tail.is_cons());
    assert!(tail.cons_car().is_symbol_named("`"));
    assert!(tail.cons_cdr().cons_car().is_symbol_named("b"));
}

#[test]
fn leading_dot_in_list_is_invalid() {
    crate::test_utils::init_test_tracing();
    assert!(read_one("(. a)", 0, &crate::emacs_core::symbol::Obarray::new()).is_err());
}

#[test]
fn dot_before_close_paren_is_symbol() {
    crate::test_utils::init_test_tracing();
    let v = read1("(a .)");
    assert!(v.is_cons());
    assert!(v.cons_car().is_symbol_named("a"));
    let second = v.cons_cdr().cons_car();
    assert!(second.is_symbol_named("."));
    assert!(v.cons_cdr().cons_cdr().is_nil());
}

#[test]
fn nested_list() {
    crate::test_utils::init_test_tracing();
    let v = read1("(a (b c))");
    assert!(v.is_cons());
    let car = v.cons_car();
    assert!(car.is_symbol_named("a"));
    let inner = v.cons_cdr().cons_car();
    assert!(inner.is_cons());
    assert!(inner.cons_car().is_symbol_named("b"));
}

// ---------------------------------------------------------------------------
// Vectors
// ---------------------------------------------------------------------------

#[test]
fn empty_vector() {
    crate::test_utils::init_test_tracing();
    let v = read1("[]");
    assert!(v.is_vector());
    let data = v.as_vector_data().unwrap();
    assert_eq!(data.len(), 0);
}

#[test]
fn vector_with_elements() {
    crate::test_utils::init_test_tracing();
    let v = read1("[1 2 3]");
    assert!(v.is_vector());
    let data = v.as_vector_data().unwrap();
    assert_eq!(data.len(), 3);
    assert_eq!(data[0].as_fixnum(), Some(1));
    assert_eq!(data[1].as_fixnum(), Some(2));
    assert_eq!(data[2].as_fixnum(), Some(3));
}

// ---------------------------------------------------------------------------
// Hash syntax
// ---------------------------------------------------------------------------

#[test]
fn uninterned_symbol() {
    crate::test_utils::init_test_tracing();
    let v = read1("#:foo");
    // Should be a symbol (uninterned)
    let name = v.as_symbol_name().unwrap();
    assert_eq!(name, "foo");
}

#[test]
fn empty_symbol() {
    crate::test_utils::init_test_tracing();
    let v = read1("##");
    let name = v.as_symbol_name().unwrap();
    assert_eq!(name, "");
}

#[test]
fn byte_code_literal_short_vector() {
    crate::test_utils::init_test_tracing();
    let err = super::read_one("#[1 2 3]", 0, &crate::emacs_core::symbol::Obarray::new())
        .expect_err("GNU signals invalid byte-code object");
    assert!(err.message.contains("Invalid byte-code object"));
}

#[test]
fn byte_code_literal_produces_bytecode() {
    crate::test_utils::init_test_tracing();
    // A valid byte-code literal: [arglist bytecode-string constants max-depth]
    // arglist=0 means (&rest _), bytecode="" means empty, constants=[], maxdepth=0
    let v = read1("#[0 \"\" [] 0]");
    assert!(v.is_bytecode(), "expected ByteCode, got {:?}", v.kind());
}

#[test]
fn byte_code_literal_can_produce_interpreted_closure() {
    crate::test_utils::init_test_tracing();
    let v = read1("#[(x) ((+ x 1)) nil]");
    assert!(v.is_lambda(), "expected Lambda, got {:?}", v.kind());
}

#[test]
fn ordinary_vector_with_closure_like_shape_stays_vector() {
    crate::test_utils::init_test_tracing();
    let v = read1("[(x) ((+ x 1)) nil]");
    assert!(v.is_vector(), "expected plain vector, got {:?}", v.kind());
}

#[test]
fn read_label_define_and_ref() {
    crate::test_utils::init_test_tracing();
    // #1=(a b) #1# should return the same list for both positions
    let forms = read_all_ok("#1=(1 2) #1#");
    assert_eq!(forms.len(), 2);
    // Both should be the same (1 2) list
    assert!(forms[0].is_cons());
    assert!(forms[1].is_cons());
    assert_eq!(forms[0].cons_car().as_fixnum(), Some(1));
    assert_eq!(forms[1].cons_car().as_fixnum(), Some(1));
}

// ---------------------------------------------------------------------------
// Propertized strings
// ---------------------------------------------------------------------------

#[test]
fn propertized_string() {
    crate::test_utils::init_test_tracing();
    let v = read1(r#"#("hello" 0 5 (face bold))"#);
    assert_eq!(v.as_utf8_str().unwrap(), "hello");
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

#[test]
fn line_comment() {
    crate::test_utils::init_test_tracing();
    let v = read1("; this is a comment\n42");
    assert_eq!(v.as_fixnum(), Some(42));
}

#[test]
fn block_comment() {
    crate::test_utils::init_test_tracing();
    let v = read1("#| block comment |# 42");
    assert_eq!(v.as_fixnum(), Some(42));
}

// ---------------------------------------------------------------------------
// read_all and read_one
// ---------------------------------------------------------------------------

#[test]
fn read_all_multiple_forms() {
    crate::test_utils::init_test_tracing();
    let forms = read_all_ok("1 2 3");
    assert_eq!(forms.len(), 3);
    assert_eq!(forms[0].as_fixnum(), Some(1));
    assert_eq!(forms[1].as_fixnum(), Some(2));
    assert_eq!(forms[2].as_fixnum(), Some(3));
}

#[test]
fn read_all_skips_autoload_cookie_comment_before_defun() {
    crate::test_utils::init_test_tracing();
    let forms = read_all_ok(
        r#"(define-minor-mode tool-bar-mode "doc")
;;;###autoload
(defun toggle-tool-bar-mode-from-frame (&optional arg)
  "doc"
  (interactive (list (or current-prefix-arg 'toggle)))
  (if (eq arg 'toggle)
      (tool-bar-mode 1)
    (tool-bar-mode arg)))"#,
    );
    assert_eq!(forms.len(), 2, "autoload cookie should be skipped");
    assert!(
        forms[1].is_cons(),
        "expected defun form after autoload cookie, got {:?}",
        forms[1]
    );
    assert!(
        forms[1].cons_car().is_symbol_named("defun"),
        "expected second top-level form to be defun, got {:?}",
        forms[1].cons_car()
    );
}

#[test]
fn read_one_returns_position() {
    crate::test_utils::init_test_tracing();
    let (val, pos) = read_one("42 rest", 0, &crate::emacs_core::symbol::Obarray::new())
        .unwrap()
        .unwrap();
    assert_eq!(val.as_fixnum(), Some(42));
    assert_eq!(pos, 2);
}

#[test]
fn read_one_empty() {
    crate::test_utils::init_test_tracing();
    let result = read_one("   ", 0, &crate::emacs_core::symbol::Obarray::new()).unwrap();
    assert!(result.is_none());
}

#[test]
fn read_one_with_offset() {
    crate::test_utils::init_test_tracing();
    let (val, pos) = read_one("42 99", 3, &crate::emacs_core::symbol::Obarray::new())
        .unwrap()
        .unwrap();
    assert_eq!(val.as_fixnum(), Some(99));
    assert_eq!(pos, 5);
}

// ---------------------------------------------------------------------------
// Complex forms
// ---------------------------------------------------------------------------

#[test]
fn defun_form() {
    crate::test_utils::init_test_tracing();
    let v = read1("(defun my-fn (x) (+ x 1))");
    assert!(v.is_cons());
    assert!(v.cons_car().is_symbol_named("defun"));
}

#[test]
fn mixed_types() {
    crate::test_utils::init_test_tracing();
    let v = read1(r#"(42 3.14 "hello" :key nil t foo)"#);
    assert!(v.is_cons());
    // First: 42
    assert_eq!(v.cons_car().as_fixnum(), Some(42));
}

#[test]
fn dollar_hash_load_file_name() {
    crate::test_utils::init_test_tracing();
    // GNU `Fread_from_string` (`src/lread.c`) expands `#$` to the
    // *value* of `load-file-name`, not the symbol. With no load
    // context active and no obarray binding bridged in (the bare
    // `read_one` path doesn't see an obarray), the result is nil.
    let v = read1("#$");
    assert!(v.is_nil(), "expected nil, got {v:?}");
}

/// Read forms from window.elc through the `.elc` unibyte reader path
/// until form 22 (the form that loadup fails on) and assert no
/// docstring fragment leaks out.
#[test]
fn read_window_elc_does_not_leak_docstring_fragments() {
    crate::test_utils::init_test_tracing();
    let path = "../lisp/window.elc";
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => {
            eprintln!("skipping: {} not present", path);
            return;
        }
    };
    // `.elc` loading wraps raw bytes in a Latin-1 envelope and selects the
    // explicit encoded-file reader state, so it remains distinct from a
    // genuine unibyte Lisp string or buffer.
    let content: String = bytes.iter().map(|&b| b as char).collect();

    // Skip the .elc preamble: header lines starting with `;` until the
    // first newline-then-paren.
    let mut pos = 0;
    while pos < content.len() {
        if content[pos..].starts_with("\n(") {
            pos += 1;
            break;
        }
        pos += 1;
    }

    let mut form_idx = 0;
    while form_idx < 50 {
        let res = read_one_from_encoded_file_bytes(
            &content,
            pos,
            &crate::emacs_core::symbol::Obarray::new(),
            None,
        );
        match res {
            Ok(Some((form, next_pos))) => {
                // Make sure we never produce a symbol whose name appears
                // in window.elc only inside docstrings (e.g. `NORE`,
                // a fragment of `IGNORE`).
                if let ValueKind::Symbol(sid) = form.kind() {
                    let name = resolve_sym(sid);
                    assert_ne!(
                        name, "NORE",
                        "form {} parsed as symbol NORE — docstring leak \
                         at byte {}",
                        form_idx, pos
                    );
                }
                pos = next_pos;
                form_idx += 1;
            }
            Ok(None) => break,
            Err(e) => panic!("read error at byte {}: {}", pos, e.message),
        }
    }
}

/// Reproduce the .elc reader bug where the `#@LEN` doc-string skip
/// drifts and a docstring fragment ends up parsed as a symbol.
///
/// Format from `bytecomp.el byte-compile-output-as-comment`:
///   #@LENGTH<space><docstring><US (\037)>
/// where LENGTH = 1 (leading space) + len(docstring) + 1 (US) bytes.
///
/// After the skip, the reader should land at the next form, NOT at any
/// byte inside the docstring.
#[test]
fn hash_skip_doc_string_lands_after_us_terminator() {
    crate::test_utils::init_test_tracing();
    // Docstring contents: "with a non-nil\nIGNORE arg"  (25 bytes)
    // Wrapped form: " <docstring>\037" — 27 bytes
    // So #@27 must skip past the leading space, the docstring, and the
    // \037 terminator, landing at the next form `(next-form)`.
    let docstring = "with a non-nil\nIGNORE arg";
    assert_eq!(docstring.len(), 25);
    let length = 1 + docstring.len() + 1; // 27
    let input = format!("#@{length} {docstring}\u{1f}\n(next-form)");
    let forms = read_all_ok(&input);
    assert_eq!(forms.len(), 1, "expected exactly one top-level form");
    let form = forms[0];
    assert!(form.is_cons(), "form should be a cons, got {:?}", form);
    assert!(
        form.cons_car().is_symbol_named("next-form"),
        "expected (next-form ...), got car {:?}",
        form.cons_car()
    );
}

/// `.elc` files store text bytes literally, including the raw bytes
/// (0xe2 0x80 0x99) for U+2019 ('right single quotation mark') that
/// `byte-compile-output-as-comment` writes when a docstring contains
/// `‘` or `’`. Those bytes are Latin-1-decoded into Rust `chars` whose
/// codepoints sit above 0x7F and therefore re-encode as 2-byte UTF-8
/// sequences in our internal `String`. The `#@LEN` skip must count
/// source bytes (= chars in the decoded String), not UTF-8 bytes — a
/// byte-wise advance under-skips by 1 per high-bit source byte and
/// strands the reader inside the docstring.
#[test]
fn hash_skip_doc_string_handles_high_bit_source_bytes() {
    crate::test_utils::init_test_tracing();
    // Build the same docstring layout that bytecomp produces, then
    // re-encode it as Latin-1 (each source byte → one char) before
    // handing to the reader, exactly like load.rs does for .elc files.
    //
    // Source docstring: "use ‘window-state-get’ first" with the curly
    // quotes stored as raw bytes 0xe2 0x80 0x98 / 0xe2 0x80 0x99
    // (the literal UTF-8 encoding of U+2018 / U+2019).
    let mut source: Vec<u8> = Vec::new();
    source.extend_from_slice(b"use ");
    source.extend_from_slice(&[0xe2, 0x80, 0x98]); // ‘
    source.extend_from_slice(b"window-state-get");
    source.extend_from_slice(&[0xe2, 0x80, 0x99]); // ’
    source.extend_from_slice(b" first");
    let doc_byte_len = source.len();
    let length = 1 + doc_byte_len + 1;

    let mut elc: Vec<u8> = Vec::new();
    elc.extend_from_slice(format!("#@{length} ").as_bytes());
    elc.extend_from_slice(&source);
    elc.push(0x1f); // US terminator
    elc.push(b'\n');
    elc.extend_from_slice(b"(window-state-put)");

    let content: String = elc.iter().map(|&b| b as char).collect();
    let forms = read_all(&content, &crate::emacs_core::symbol::Obarray::new())
        .expect("read should succeed");
    assert_eq!(forms.len(), 1, "expected exactly one top-level form");
    assert!(
        forms[0].cons_car().is_symbol_named("window-state-put"),
        "expected (window-state-put), got car {:?}",
        forms[0].cons_car()
    );
}

#[test]
fn hash_caret_reads_char_table_literal() {
    crate::test_utils::init_test_tracing();
    let contents = std::iter::repeat("nil")
        .take(64)
        .collect::<Vec<_>>()
        .join(" ");
    let input = format!("#^[nil nil test ascii {contents}]");
    let table = read1(&input);

    assert!(crate::emacs_core::chartable::is_char_table(&table));
    let ascii = crate::emacs_core::builtins::builtin_aref(vec![table, Value::fixnum(42)])
        .expect("aref should read char-table ASCII slot");
    assert!(ascii.is_symbol_named("ascii"));
}

#[test]
fn hash_caret_reads_nested_sub_char_table_literal() {
    crate::test_utils::init_test_tracing();
    let mut sub_items = vec!["nil"; 128];
    sub_items[65] = "letter-a";
    let sub_table = format!("#^^[3 0 {}]", sub_items.join(" "));
    let contents = std::iter::repeat("nil")
        .take(64)
        .collect::<Vec<_>>()
        .join(" ");
    let input = format!("#^[nil nil test {sub_table} {contents}]");
    let table = read1(&input);

    let letter_a = crate::emacs_core::builtins::builtin_aref(vec![table, Value::fixnum(65)])
        .expect("aref should read nested sub-char-table value");
    assert!(letter_a.is_symbol_named("letter-a"));
}

#[test]
fn unibyte_source_preserves_direct_latin1_string_bytes() {
    crate::test_utils::init_test_tracing();
    let input: String = [b'"', 0xFF, b'"'].into_iter().map(char::from).collect();
    let result = read_one_with_source_multibyte(
        &input,
        false,
        0,
        &crate::emacs_core::symbol::Obarray::new(),
    )
    .expect("read_one_with_source_multibyte should succeed")
    .expect("reader should produce one form")
    .0;
    let text = result
        .as_lisp_string()
        .expect("reader should return a LispString");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF]);
}

#[test]
fn lisp_read_source_tracks_logical_offsets_for_unibyte_input() {
    crate::test_utils::init_test_tracing();
    let input =
        crate::heap_types::LispString::from_unibyte(vec![b'"', 0xFF, b'"', b' ', b'4', b'2']);
    let source = LispReadSource::new(&input);

    let (first, first_end) = source
        .read_one(0, &crate::emacs_core::symbol::Obarray::new())
        .expect("first read should succeed")
        .expect("first form should exist");
    let first_text = first
        .as_lisp_string()
        .expect("first form should be a string");
    assert_eq!(first_text.as_bytes(), &[0xFF]);
    assert_eq!(first_end, 3);

    let (second, second_end) = source
        .read_one(first_end, &crate::emacs_core::symbol::Obarray::new())
        .expect("second read should succeed")
        .expect("second form should exist");
    assert_eq!(second.as_fixnum(), Some(42));
    assert_eq!(second_end, 6);
}

#[test]
fn lisp_read_source_reads_late_multibyte_forms_directly() {
    crate::test_utils::init_test_tracing();
    let mut text = String::new();
    for _ in 0..1024 {
        text.push_str("\"é\"\n");
    }
    text.push_str("42");

    let input = crate::heap_types::LispString::from_utf8(&text);
    let source = LispReadSource::new(&input);
    let mut pos = 0;

    for _ in 0..1024 {
        let (form, next_pos) = source
            .read_one(pos, &crate::emacs_core::symbol::Obarray::new())
            .expect("multibyte read should succeed")
            .expect("multibyte form should exist");
        assert!(form.is_string(), "expected string form, got {form:?}");
        pos = next_pos;
    }

    let (last, end_pos) = source
        .read_one(pos, &crate::emacs_core::symbol::Obarray::new())
        .expect("final read should succeed")
        .expect("final form should exist");
    assert_eq!(last.as_fixnum(), Some(42));
    assert_eq!(end_pos, input.sbytes());
}

#[test]
fn lisp_read_source_preserves_extended_emacs_chars_in_string_literals() {
    crate::test_utils::init_test_tracing();
    let extended_code = 0x1A_01C1;
    let mut extended = [0; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let extended_len = crate::emacs_core::emacs_char::char_string(extended_code, &mut extended);

    let mut bytes = Vec::new();
    bytes.push(b'"');
    bytes.extend_from_slice(&extended[..extended_len]);
    bytes.push(b'"');

    let input = crate::heap_types::LispString::from_emacs_bytes(bytes);
    let source = LispReadSource::new(&input);
    let (form, end) = source
        .read_one(0, &crate::emacs_core::symbol::Obarray::new())
        .expect("read should not panic on extended Emacs chars")
        .expect("reader should produce a form");
    let text = form
        .as_lisp_string()
        .expect("reader should return a string");

    assert_eq!(end, input.sbytes());
    assert!(text.is_multibyte());
    assert_eq!(
        crate::emacs_core::builtins::lisp_string_char_codes(text),
        vec![extended_code]
    );
}

#[test]
fn lisp_read_source_preserves_extended_emacs_chars_in_char_literals() {
    crate::test_utils::init_test_tracing();
    let extended_code = 0x1A_01C1;
    let mut extended = [0; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let extended_len = crate::emacs_core::emacs_char::char_string(extended_code, &mut extended);

    let mut bytes = Vec::new();
    bytes.push(b'?');
    bytes.extend_from_slice(&extended[..extended_len]);

    let input = crate::heap_types::LispString::from_emacs_bytes(bytes);
    let source = LispReadSource::new(&input);
    let (form, end) = source
        .read_one(0, &crate::emacs_core::symbol::Obarray::new())
        .expect("read should not panic on extended Emacs char literals")
        .expect("reader should produce a form");

    assert_eq!(end, input.sbytes());
    assert_eq!(form.as_fixnum(), Some(extended_code as i64));
}

#[test]
fn runtime_reader_reads_utf8_emacs_extended_char_literal() {
    crate::test_utils::init_test_tracing();
    let code = 0x1A_01CA;
    // Faithful Emacs-bytes LispString source (issue #131): the non-Unicode
    // ethiopic literal keeps its real code instead of round-tripping through
    // the lossy storage-string form.
    let source = crate::emacs_core::load::decode_emacs_utf8_source_lisp(
        b"?\xF6\xA0\x87\x8A",
        crate::emacs_core::coding::EolConversion::Enabled,
    );
    let read_source = crate::emacs_core::value_reader::LispReadSource::new(&source);

    let (form, end_pos) = read_source
        .read_one(0, &crate::emacs_core::symbol::Obarray::new())
        .expect("read should accept extended utf-8-emacs character literals")
        .expect("form should exist");

    assert_eq!(form.as_fixnum(), Some(code));
    assert_eq!(end_pos, source.sbytes());
}

#[test]
fn runtime_reader_steps_utf8_emacs_extended_chars_as_single_source_chars() {
    crate::test_utils::init_test_tracing();
    let code = 0x1A_01CA;
    let source = crate::emacs_core::load::decode_emacs_utf8_source_lisp(
        b"(list ?\xF6\xA0\x87\x8A 1)",
        crate::emacs_core::coding::EolConversion::Enabled,
    );
    let read_source = crate::emacs_core::value_reader::LispReadSource::new(&source);

    let (form, _) = read_source
        .read_one(0, &crate::emacs_core::symbol::Obarray::new())
        .expect("read should accept extended utf-8-emacs character literals")
        .expect("form should exist");
    let items =
        crate::emacs_core::value::list_to_vec(&form).expect("reader should return a proper list");

    assert_eq!(items.len(), 3);
    assert!(items[0].is_symbol_named("list"));
    assert_eq!(items[1].as_fixnum(), Some(code));
    assert_eq!(items[2].as_fixnum(), Some(1));
}

#[test]
fn runtime_reader_reads_utf8_emacs_extended_string_literal() {
    crate::test_utils::init_test_tracing();
    let code = 0x1A_01CA;
    let source = crate::emacs_core::load::decode_emacs_utf8_source_lisp(
        b"\"\xF6\xA0\x87\x8A\"",
        crate::emacs_core::coding::EolConversion::Enabled,
    );
    let read_source = crate::emacs_core::value_reader::LispReadSource::new(&source);

    let (form, _) = read_source
        .read_one(0, &crate::emacs_core::symbol::Obarray::new())
        .expect("read should accept extended utf-8-emacs string literals")
        .expect("form should exist");
    let text = form.as_lisp_string().expect("form should be a string");

    assert!(text.is_multibyte());
    assert_eq!(
        crate::emacs_core::builtins::lisp_string_char_codes(text),
        vec![code as u32]
    );
}

#[test]
fn lisp_read_source_reads_nonunicode_emacs_char_literal() {
    crate::test_utils::init_test_tracing();
    let code = 0x1A_01C1;
    let mut encoded = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let encoded_len = crate::emacs_core::emacs_char::char_string(code, &mut encoded);

    let mut source_bytes = vec![b'?'];
    source_bytes.extend_from_slice(&encoded[..encoded_len]);
    let input = crate::heap_types::LispString::from_emacs_bytes(source_bytes);
    let source = LispReadSource::new(&input);

    let (form, end_pos) = source
        .read_one(0, &crate::emacs_core::symbol::Obarray::new())
        .expect("read should not panic on non-Unicode Emacs chars")
        .expect("form should exist");

    assert_eq!(form.as_fixnum(), Some(code as i64));
    assert_eq!(end_pos, input.sbytes());
}

#[test]
fn lisp_read_source_reads_nonunicode_emacs_string_literal() {
    crate::test_utils::init_test_tracing();
    let code = 0x1A_01C1;
    let mut encoded = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let encoded_len = crate::emacs_core::emacs_char::char_string(code, &mut encoded);

    let mut source_bytes = vec![b'"'];
    source_bytes.extend_from_slice(&encoded[..encoded_len]);
    source_bytes.push(b'"');
    let input = crate::heap_types::LispString::from_emacs_bytes(source_bytes);
    let source = LispReadSource::new(&input);

    let (form, end_pos) = source
        .read_one(0, &crate::emacs_core::symbol::Obarray::new())
        .expect("read should not panic on non-Unicode Emacs chars")
        .expect("form should exist");
    let text = form.as_lisp_string().expect("form should be a string");

    assert!(text.is_multibyte());
    assert_eq!(text.as_bytes(), &encoded[..encoded_len]);
    assert_eq!(end_pos, input.sbytes());
}

#[test]
fn bool_vector_literal_returns_bool_vector_object() {
    crate::test_utils::init_test_tracing();
    let value = read1("#&3\"\x05\"");

    assert!(crate::emacs_core::chartable::is_bool_vector(&value));
    assert_eq!(
        crate::emacs_core::chartable::bool_vector_length(&value),
        Some(3)
    );
    assert_eq!(
        crate::emacs_core::chartable::bool_vector_ref_value(&value, 0),
        Some(Value::T)
    );
    assert_eq!(
        crate::emacs_core::chartable::bool_vector_ref_value(&value, 1),
        Some(Value::NIL)
    );
    assert_eq!(
        crate::emacs_core::chartable::bool_vector_ref_value(&value, 2),
        Some(Value::T)
    );
}

#[test]
fn bool_vector_literal_rejects_wrong_payload_size_like_gnu() {
    crate::test_utils::init_test_tracing();
    let ob = crate::emacs_core::symbol::Obarray::new();
    let err = read_one("#&3\"\"", 0, &ob).expect_err("short payload should be invalid");

    assert_eq!(err.message, "#&...");
}

#[test]
fn bool_vector_literal_rejects_missing_decimal_size_like_gnu() {
    crate::test_utils::init_test_tracing();
    let ob = crate::emacs_core::symbol::Obarray::new();
    let err = read_one("#&x\"a\"", 0, &ob).expect_err("missing size should be invalid");

    assert_eq!(err.message, "#&");
}
