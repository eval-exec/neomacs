use super::{ReadErrorKind, SavedDocBlocks, Value, read_one_from_encoded_file_bytes};
use crate::emacs_core::doc::DocBytePosition;
use crate::emacs_core::eval::{Context, VariableView};
use crate::emacs_core::symbol::{CanonicalAccess, Obarray};

#[test]
fn oversized_file_character_reports_syntax_at_its_unconsumed_tail() {
    crate::test_utils::init_test_tracing();
    let _context = Context::new();
    let ob = Obarray::new();
    // Recovery consumes the oversized sequence's lead byte. Its remaining
    // bytes still reach the character-literal parser, which rejects them.
    let error = read_one_from_encoded_file_bytes(
        b"?\xF8\xB8\x96\xB8\x96",
        0,
        DocBytePosition::from_file_offset(0),
        &mut SavedDocBlocks::new(),
        VariableView::canonical(CanonicalAccess::TEST, &ob),
        None,
    )
    .expect_err("the unconsumed tail is invalid character-literal syntax");

    assert_eq!(error.kind, ReadErrorKind::InvalidReadSyntax);
    assert_eq!(error.position, 2);
}

#[test]
fn oversized_file_string_preserves_the_recovered_lead_and_tail_characters() {
    crate::test_utils::init_test_tracing();
    let _context = Context::new();
    let ob = Obarray::new();
    let input = b"\"\xF8\xB8\x96\xB8\x96\" 23";
    let (value, next_byte) = read_one_from_encoded_file_bytes(
        input,
        0,
        DocBytePosition::from_file_offset(0),
        &mut SavedDocBlocks::new(),
        VariableView::canonical(CanonicalAccess::TEST, &ob),
        None,
    )
    .expect("read quoted malformed file bytes")
    .expect("quoted string");
    let string = value.as_lisp_string().expect("string literal");

    // Recovery retains BYTE8 F8 and B8, then the in-domain GNU raw
    // arithmetic of 96 B8 (0x3FF538), and finally BYTE8 96. Each
    // original input byte therefore contributes to a recovered character.
    assert!(string.is_multibyte());
    assert_eq!(string.schars(), 4);
    for (index, expected) in [0x3F_FFF8, 0x3F_FFB8, 0x3F_F538, 0x3F_FF96]
        .into_iter()
        .enumerate()
    {
        let character =
            crate::emacs_core::builtins::builtin_aref(vec![value, Value::fixnum(index as i64)])
                .expect("read recovered string character");
        assert_eq!(character.as_fixnum(), Some(expected));
    }
    assert_eq!(next_byte, 7);
    let (following, end) = read_one_from_encoded_file_bytes(
        input,
        next_byte,
        DocBytePosition::from_file_offset(0),
        &mut SavedDocBlocks::new(),
        VariableView::canonical(CanonicalAccess::TEST, &ob),
        None,
    )
    .expect("read following form")
    .expect("following integer");
    assert_eq!(following.as_fixnum(), Some(23));
    assert_eq!(end, input.len());
}

#[test]
fn file_character_literals_preserve_in_domain_gnu_arithmetic() {
    crate::test_utils::init_test_tracing();
    let _context = Context::new();
    let ob = Obarray::new();
    // The file reader historically accepts noncanonical encodings. The
    // character-domain guard must not substitute canonical-form validation.
    // Character literals expose byte8 FF as its byte value, 255.
    for (input, expected) in [
        (b"?\xF8\x8F\xBF\xBD\xBF".as_slice(), 0x3F_FF7F),
        (b"?\xF8\x8F\xBF\xBF\xBF".as_slice(), 255),
        (b"?\x80\x80".as_slice(), 0x3F_EF80),
    ] {
        let (value, end) = read_one_from_encoded_file_bytes(
            input,
            0,
            DocBytePosition::from_file_offset(0),
            &mut SavedDocBlocks::new(),
            VariableView::canonical(CanonicalAccess::TEST, &ob),
            None,
        )
        .expect("read in-domain file character")
        .expect("character literal");
        assert_eq!(value.as_fixnum(), Some(expected), "{input:02X?}");
        assert_eq!(end, input.len(), "{input:02X?}");
    }
}
