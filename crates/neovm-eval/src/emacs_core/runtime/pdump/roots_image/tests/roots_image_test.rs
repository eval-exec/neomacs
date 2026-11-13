use super::*;

#[test]
fn roots_section_round_trips_lisp_roots() {
    let roots = DumpRootState {
        dynamic: vec![DumpOrderedSymMap {
            entries: vec![
                (
                    DumpSymId(1),
                    DumpRuntimeBindingValue::Bound(DumpValue::Int(42)),
                ),
                (DumpSymId(2), DumpRuntimeBindingValue::Void),
            ],
        }],
        lexenv: DumpValue::Symbol(DumpSymId(3)),
        features: vec![DumpSymId(4), DumpSymId(5)],
        require_stack: vec![DumpSymId(6)],
        loads_in_progress: vec![DumpLispString {
            data: b"load-file".to_vec(),
            size: 9,
            size_byte: 9,
        }],
        standard_syntax_table: DumpValue::Vector(super::super::types::DumpHeapRef { index: 7 }),
        syntax_code_objects: DumpValue::Vector(super::super::types::DumpHeapRef { index: 9 }),
        standard_category_table: DumpValue::Nil,
        current_local_map: DumpValue::Cons(super::super::types::DumpHeapRef { index: 8 }),
        current_global_map: DumpValue::Cons(super::super::types::DumpHeapRef { index: 10 }),
    };

    let bytes = roots_section_bytes(&roots).expect("encode roots");
    let decoded = load_roots_section(&bytes).expect("decode roots");

    assert_eq!(format!("{decoded:?}"), format!("{roots:?}"));
}

#[test]
fn roots_section_rejects_bad_magic() {
    let mut bytes = roots_section_bytes(&DumpRootState {
        dynamic: Vec::new(),
        lexenv: DumpValue::Nil,
        features: Vec::new(),
        require_stack: Vec::new(),
        loads_in_progress: Vec::new(),
        standard_syntax_table: DumpValue::Nil,
        syntax_code_objects: DumpValue::Nil,
        standard_category_table: DumpValue::Nil,
        current_local_map: DumpValue::Nil,
        current_global_map: DumpValue::Nil,
    })
    .expect("encode roots");
    bytes[0] ^= 1;
    let err = load_roots_section(&bytes).expect_err("bad magic should fail");
    assert!(matches!(err, DumpError::ImageFormatError(_)));
}
