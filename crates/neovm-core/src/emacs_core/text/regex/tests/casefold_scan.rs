//! Case-folded candidate scans in `re_search`.
//!
//! A case-folded search may skip a position only where the per-character loop
//! would have skipped it.  These tests pin down the facts that make the fast
//! scans exact, and compare them with the exhaustive scan.

use super::*;

/// `AsciiPreimage::AsciiOnly` is a claim about every character code: the
/// standard translation keeps ASCII in ASCII and non-ASCII out of it.  The
/// folded candidate table and the folded literal prefilter both rest on it.
#[test]
fn standard_translation_keeps_non_ascii_out_of_ascii() {
    crate::test_utils::init_test_tracing();
    let table = CaseTranslation::standard();
    assert_eq!(table.ascii_preimage(), AsciiPreimage::AsciiOnly);
    for c in 0..0x80u32 {
        let translated = table.translate(c);
        assert!(
            translated < 0x80,
            "ASCII {c:#x} translates out of ASCII to {translated:#x}"
        );
    }
    let mut into_ascii = Vec::new();
    for c in 0x80..=emacs_char::MAX_CHAR {
        let translated = table.translate(c);
        if translated < 0x80 {
            into_ascii.push((c, translated));
        }
    }
    assert!(
        into_ascii.is_empty(),
        "non-ASCII characters translate into ASCII: {into_ascii:#x?}"
    );
}

/// A case-canon char-table can fold anything into ASCII, so it makes no
/// claim.
#[test]
fn char_table_translation_makes_no_ascii_preimage_claim() {
    crate::test_utils::init_test_tracing();
    let table = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
    let translation = CaseTranslation::from_char_table(table);
    assert_eq!(translation.ascii_preimage(), AsciiPreimage::Unknown);
}
