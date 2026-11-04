use super::*;
use crate::emacs_core::pdump::types::{DumpNameId, DumpSymbolEntry};

#[test]
fn symbol_table_section_round_trips_exact_names() {
    let table = DumpSymbolTable {
        names: vec![
            LispString::from_unibyte(vec![0xff, b'a']),
            LispString::from_utf8("lambda"),
            LispString::from_emacs_bytes("λ".as_bytes().to_vec()),
        ],
        symbols: vec![
            DumpSymbolEntry {
                name: DumpNameId(0),
                canonical: true,
            },
            DumpSymbolEntry {
                name: DumpNameId(1),
                canonical: false,
            },
            DumpSymbolEntry {
                name: DumpNameId(2),
                canonical: true,
            },
        ],
    };

    let bytes = symbol_table_section_bytes(&table).expect("encode symbol table");
    with_symbol_table_section(&bytes, |names, symbol_names, canonical| {
        assert_eq!(names.len(), 3);
        assert_eq!(names[0].as_bytes(), &[0xff, b'a']);
        assert!(!names[0].is_multibyte());
        assert!(names[0].has_trailing_nul());
        assert_eq!(names[1].as_bytes(), b"lambda");
        assert!(names[1].is_multibyte());
        assert!(names[1].has_trailing_nul());
        assert_eq!(names[2].as_bytes(), "λ".as_bytes());
        assert!(names[2].is_multibyte());
        assert!(names[2].has_trailing_nul());
        assert_eq!(symbol_names, &[0, 1, 2]);
        assert_eq!(canonical, &[true, false, true]);
        Ok(())
    })
    .expect("decode symbol table");
}

#[test]
fn symbol_table_section_rejects_bad_name_id() {
    let table = DumpSymbolTable {
        names: vec![LispString::from_unibyte(b"ok".to_vec())],
        symbols: vec![DumpSymbolEntry {
            name: DumpNameId(0),
            canonical: true,
        }],
    };
    let mut bytes = symbol_table_section_bytes(&table).expect("encode symbol table");
    let header = *bytemuck::from_bytes::<SymbolTableHeader>(&bytes[..HEADER_SIZE]);
    let symbol_start = header.symbol_table_offset as usize;
    let mut symbol = *bytemuck::from_bytes::<SymbolEntry>(
        &bytes[symbol_start..symbol_start + SYMBOL_ENTRY_SIZE],
    );
    symbol.name_id = 99;
    bytes[symbol_start..symbol_start + SYMBOL_ENTRY_SIZE]
        .copy_from_slice(bytemuck::bytes_of(&symbol));

    let err = with_symbol_table_section(&bytes, |_, _, _| Ok(())).unwrap_err();
    assert!(
        matches!(err, DumpError::ImageFormatError(message) if message.contains("out of range"))
    );
}
