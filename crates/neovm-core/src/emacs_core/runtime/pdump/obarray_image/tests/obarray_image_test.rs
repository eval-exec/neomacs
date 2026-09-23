use super::super::types::{DumpHeapRef, DumpValue};
use super::*;

#[test]
fn obarray_section_round_trips_symbol_state() {
    let obarray = DumpObarray {
        symbols: vec![
            (
                DumpSymId(1),
                DumpSymbolData {
                    redirect: 0,
                    trapped_write: 1,
                    interned: 2,
                    declared_special: true,
                    val: DumpSymbolVal::Plain(DumpValue::Int(42)),
                    function: DumpValue::Subr(super::super::types::DumpNameId(9)),
                    plist: DumpValue::Cons(DumpHeapRef { index: 3 }),
                },
            ),
            (
                DumpSymId(2),
                DumpSymbolData {
                    redirect: 1,
                    trapped_write: 0,
                    interned: 1,
                    declared_special: false,
                    val: DumpSymbolVal::Localized {
                        default: DumpValue::Symbol(DumpSymId(1)),
                        local_if_set: true,
                        forwarder: Some(DumpLocalizedForwarder::Int),
                    },
                    function: DumpValue::Nil,
                    plist: DumpValue::Unbound,
                },
            ),
            (
                DumpSymId(4),
                DumpSymbolData {
                    redirect: 3,
                    trapped_write: 0,
                    interned: 1,
                    declared_special: true,
                    val: DumpSymbolVal::BoolForwarded(true),
                    function: DumpValue::Nil,
                    plist: DumpValue::Nil,
                },
            ),
        ],
        global_members: vec![DumpSymId(1), DumpSymId(2), DumpSymId(4)],
        function_unbound: vec![DumpSymId(3)],
        function_epoch: 77,
        plain_rows: None,
    };

    let bytes = obarray_section_bytes(&obarray).expect("encode obarray");
    let decoded = load_obarray_section(&bytes).expect("decode obarray");

    assert_eq!(format!("{decoded:?}"), format!("{obarray:?}"));
}

#[test]
fn obarray_section_rejects_bad_magic() {
    let mut bytes = obarray_section_bytes(&DumpObarray {
        symbols: Vec::new(),
        global_members: Vec::new(),
        function_unbound: Vec::new(),
        function_epoch: 0,
        plain_rows: None,
    })
    .expect("encode obarray");
    bytes[0] ^= 1;
    let err = load_obarray_section(&bytes).expect_err("bad magic should fail");
    assert!(matches!(err, DumpError::ImageFormatError(_)));
}
