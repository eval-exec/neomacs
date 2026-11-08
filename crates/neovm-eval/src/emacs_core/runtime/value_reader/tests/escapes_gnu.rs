use super::*;

#[test]
fn shared_decoder_preserves_character_and_string_domains() {
    assert_eq!(read1(r"?\200").as_fixnum(), Some(128));
    assert_eq!(read1(r"?\M-\101").as_fixnum(), Some(0x8000041));
    assert_eq!(read1(r#""\x41;b""#).as_str_owned().as_deref(), Some("A;b"));
    let ob = neovm_value::symbol::Obarray::new();
    let error = read_one(
        "?\\\n",
        0,
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &ob,
        ),
    )
    .unwrap_err();
    assert_eq!(error.kind, ReadErrorKind::Error);
    let error = read_one(
        r"?\N{LATIN SMALL LETTER A",
        0,
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &ob,
        ),
    )
    .unwrap_err();
    assert_eq!(error.kind, ReadErrorKind::EndOfFile);
    for source in [r"?\Mz", r"?\Nq"] {
        let error = read_one(
            source,
            0,
            crate::emacs_core::eval::VariableView::canonical(
                neovm_value::symbol::CanonicalAccess::TEST,
                &ob,
            ),
        )
        .unwrap_err();
        assert_eq!(
            error.position,
            source.len(),
            "malformed prefix consumes its next character"
        );
    }
}

#[test]
fn modifier_chains_are_iterative_and_control_is_not_idempotent() {
    assert_eq!(read1(r"?\C-\C-a").as_fixnum(), Some(0x4000001));
    assert_eq!(read1(r"?\M-\200").as_fixnum(), Some(0x8000080));
    let source = format!("?{}a", r"\M-".repeat(100_000));
    assert_eq!(read1(&source).as_fixnum(), Some(0x8000061));
    let source = format!("\"{}a\"", r"\M-".repeat(100_000));
    let value = read1(&source);
    assert_eq!(
        value.as_lisp_string().expect("string literal").as_bytes(),
        &[225]
    );
}
