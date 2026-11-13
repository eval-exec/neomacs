use super::super::types::{DumpHeapRef, DumpNameId};
use super::*;

#[test]
fn autoloads_section_round_trips_manager_state() {
    let manager = DumpAutoloadManager {
        entries_syms: vec![(
            DumpSymId(1),
            DumpAutoloadEntry {
                file: lisp_string("files"),
                docstring: Some(lisp_string("doc")),
                interactive: true,
                autoload_type: DumpAutoloadType::Macro,
            },
        )],
        entries: vec![(
            "name".into(),
            DumpAutoloadEntry {
                file: lisp_string("named"),
                docstring: None,
                interactive: false,
                autoload_type: DumpAutoloadType::Function,
            },
        )],
        after_load_lisp: vec![(
            lisp_string("feature"),
            vec![
                DumpValue::Subr(DumpNameId(2)),
                DumpValue::Cons(DumpHeapRef { index: 3 }),
            ],
        )],
        after_load: vec![("plain-feature".into(), vec![DumpValue::Int(4)])],
        loaded_files: vec![lisp_string("loaded")],
        obsolete_functions_syms: vec![(DumpSymId(5), (lisp_string("when"), lisp_string("fn")))],
        obsolete_functions: vec![("old-fn".into(), ("1.0".into(), "new-fn".into()))],
        obsolete_variables_syms: vec![(
            DumpSymId(6),
            (lisp_string("when-var"), lisp_string("var")),
        )],
        obsolete_variables: vec![("old-var".into(), ("2.0".into(), "new-var".into()))],
    };

    let bytes = autoloads_section_bytes(&manager).expect("encode autoloads");
    let decoded = load_autoloads_section(&bytes).expect("decode autoloads");

    assert_eq!(format!("{decoded:?}"), format!("{manager:?}"));
}

#[test]
fn autoloads_section_rejects_bad_magic() {
    let mut bytes = autoloads_section_bytes(&empty_autoloads()).expect("encode autoloads");
    bytes[0] ^= 1;
    let err = load_autoloads_section(&bytes).expect_err("bad magic should fail");
    assert!(matches!(err, DumpError::ImageFormatError(_)));
}

fn lisp_string(text: &str) -> DumpLispString {
    DumpLispString {
        data: text.as_bytes().to_vec(),
        size: text.chars().count(),
        size_byte: text.len() as i64,
    }
}
