//! Unit tests for the shared documentation-record unquoting definition
//! (`src/doc.c:266-291` strict file rules, `src/lread.c:3520-3545` saved-block
//! rules) and the typed absolute position.

use super::*;

#[test]
fn unquote_doc_bytes_applies_the_two_gnu_reader_rules() {
    // ^A^A -> ^A, ^A0 -> NUL, ^A_ -> ^_, terminated by ^_.
    let record = b"Alpha\x01\x01Beta\x010Nul\x01_End\x1f";
    let expected = b"Alpha\x01Beta\x00Nul\x1fEnd";
    assert_eq!(
        unquote_doc_bytes(record, DocUnquoteRules::StrictFile),
        Ok(expected.to_vec())
    );
    assert_eq!(
        unquote_doc_bytes(record, DocUnquoteRules::SavedBlock),
        Ok(expected.to_vec())
    );

    // An unknown escape is invalid data for the file reader ...
    let invalid = b"Bad\x01xEscape\x1f";
    assert_eq!(
        unquote_doc_bytes(invalid, DocUnquoteRules::StrictFile),
        Err(DocUnquoteError::InvalidEscape(b'x'))
    );
    // ... and is kept as itself by `get_lazy_string`'s saved-block arm.
    assert_eq!(
        unquote_doc_bytes(invalid, DocUnquoteRules::SavedBlock),
        Ok(b"BadxEscape".to_vec())
    );
}

#[test]
fn unquote_doc_bytes_reports_dangling_escapes_and_missing_terminators() {
    assert_eq!(
        unquote_doc_bytes(b"abc\x01", DocUnquoteRules::StrictFile),
        Err(DocUnquoteError::DanglingEscape)
    );
    assert_eq!(
        unquote_doc_bytes(b"abc\x01", DocUnquoteRules::SavedBlock),
        Err(DocUnquoteError::DanglingEscape)
    );
    assert_eq!(
        unquote_doc_bytes(b"abc", DocUnquoteRules::StrictFile),
        Err(DocUnquoteError::MissingTerminator)
    );
    assert_eq!(unquote_saved_doc_block(b"\x01x\x1f"), Ok(b"x".to_vec()));
}

#[test]
fn doc_byte_position_absolutizes_legacy_negative_positions() {
    assert_eq!(DocBytePosition::from_fixnum(0).get(), 0);
    assert_eq!(DocBytePosition::from_fixnum(83).get(), 83);
    assert_eq!(DocBytePosition::from_fixnum(-83).get(), 83);
    // GNU's `eabs` on the most negative fixnum must not overflow or panic.
    assert_eq!(DocBytePosition::from_fixnum(i64::MIN).get(), 1 << 63);
    assert_eq!(
        DocBytePosition::from_file_offset(usize::MAX).checked_advance(1),
        None
    );
}
