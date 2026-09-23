use super::super::types::DumpHeapRef;
use super::*;

#[test]
fn face_section_round_trips_face_table() {
    let table = DumpFaceTable {
        face_ids: vec![(
            DumpSymId(1),
            DumpFace {
                foreground: Some(DumpColor {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 4,
                }),
                background: None,
                family_value: Some(DumpValue::Str(DumpHeapRef { index: 5 })),
                family: Some("legacy-family".into()),
                foundry_value: Some(DumpValue::Symbol(DumpSymId(6))),
                foundry: None,
                height: Some(DumpFaceHeight::Relative(1.25)),
                weight: Some(700),
                slant: Some(DumpFontSlant::Italic),
                underline_disabled: false,
                underline: Some(DumpUnderline {
                    style: DumpUnderlineStyle::Wave,
                    color: Some(DumpColor {
                        r: 10,
                        g: 11,
                        b: 12,
                        a: 13,
                    }),
                    position: Some(2),
                }),
                overline: Some(true),
                strike_through: Some(false),
                box_disabled: false,
                box_border: Some(DumpBoxBorder {
                    color: None,
                    width: -1,
                    style: DumpBoxStyle::Raised,
                }),
                inverse_video: Some(false),
                stipple_value: Some(DumpValue::Nil),
                stipple: Some("legacy-stipple".into()),
                extend: Some(true),
                inherit_syms: vec![DumpSymId(7), DumpSymId(8)],
                inherit: vec!["legacy-inherit".into()],
                overstrike: true,
                doc_value: Some(DumpValue::True),
                doc: Some("doc".into()),
            },
        )],
        faces: vec![(
            "legacy-face".into(),
            DumpFace {
                foreground: None,
                background: Some(DumpColor {
                    r: 20,
                    g: 21,
                    b: 22,
                    a: 23,
                }),
                family_value: None,
                family: None,
                foundry_value: None,
                foundry: Some("foundry".into()),
                height: Some(DumpFaceHeight::Absolute(120)),
                weight: None,
                slant: Some(DumpFontSlant::ReverseOblique),
                underline_disabled: true,
                underline: None,
                overline: None,
                strike_through: None,
                box_disabled: true,
                box_border: None,
                inverse_video: None,
                stipple_value: None,
                stipple: None,
                extend: None,
                inherit_syms: Vec::new(),
                inherit: Vec::new(),
                overstrike: false,
                doc_value: None,
                doc: None,
            },
        )],
    };

    let bytes = face_table_section_bytes(&table).expect("encode face table");
    let decoded = load_face_table_section(&bytes).expect("decode face table");

    assert_eq!(format!("{decoded:?}"), format!("{table:?}"));
}

#[test]
fn face_section_rejects_bad_magic() {
    let mut bytes = face_table_section_bytes(&empty_face_table()).expect("encode face table");
    bytes[0] ^= 1;
    let err = load_face_table_section(&bytes).expect_err("bad magic should fail");
    assert!(matches!(err, DumpError::ImageFormatError(_)));
}
