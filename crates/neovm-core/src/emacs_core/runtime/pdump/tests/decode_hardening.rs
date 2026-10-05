use super::super::{convert, types};
use crate::emacs_core::intern::SymId;
use crate::heap_types::LispString;

const DUMP_SYMBOL: types::DumpSymId = types::DumpSymId(0);

/// Installs the one-symbol dump-local remap for this test thread and clears
/// it on every exit path. The marker keeps the guard on its creating thread.
#[must_use = "keep the symbol remap alive until decoding finishes"]
struct SymbolRemap {
    runtime_symbol: SymId,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

static_assertions::assert_not_impl_any!(SymbolRemap: Send, Sync);

impl SymbolRemap {
    fn new(name: &str) -> Self {
        let names = [LispString::from_unibyte(name.as_bytes().to_vec())];
        convert::load_symbol_table_parts(&names, &[0], &[true]).unwrap();
        Self {
            runtime_symbol: convert::load_sym_id(&DUMP_SYMBOL),
            _thread: std::marker::PhantomData,
        }
    }
}

impl Drop for SymbolRemap {
    fn drop(&mut self) {
        convert::finish_load_interner();
    }
}

fn empty_heap() -> types::DumpTaggedHeap {
    types::DumpTaggedHeap {
        objects: Vec::new(),
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    }
}

fn symbol_data() -> types::DumpSymbolData {
    types::DumpSymbolData {
        redirect: 0,
        trapped_write: 0,
        interned: 0,
        declared_special: false,
        val: types::DumpSymbolVal::Plain(types::DumpValue::Nil),
        function: types::DumpValue::Unbound,
        plist: types::DumpValue::Nil,
    }
}

fn residual_obarray(data: types::DumpSymbolData) -> types::DumpObarray {
    types::DumpObarray {
        symbols: vec![(DUMP_SYMBOL, data)],
        global_members: Vec::new(),
        function_unbound: Vec::new(),
        function_epoch: 0,
        plain_rows: None,
    }
}

#[test]
fn pdump_rejects_masked_trapped_write_corruption() {
    use super::super::DumpError;
    let _symbols = SymbolRemap::new("pdump-invalid-trapped-write");
    for code in [3, 0x80, u8::MAX] {
        let heap = empty_heap();
        let mut decoder = convert::LoadDecoder::new(&heap);
        let mut data = symbol_data();
        data.trapped_write = code;
        let error = convert::load_obarray(&mut decoder, &residual_obarray(data)).unwrap_err();
        assert!(
            matches!(error, DumpError::InvalidSymbolTrappedWrite(source) if source.number == code)
        );
    }
}

#[test]
fn pdump_rejects_masked_interned_corruption() {
    use super::super::DumpError;
    let _symbols = SymbolRemap::new("pdump-invalid-interned");
    for code in [3, 0x80, u8::MAX] {
        let heap = empty_heap();
        let mut decoder = convert::LoadDecoder::new(&heap);
        let mut data = symbol_data();
        data.interned = code;
        let error = convert::load_obarray(&mut decoder, &residual_obarray(data)).unwrap_err();
        assert!(matches!(error, DumpError::InvalidSymbolInterned(source) if source.number == code));
    }
}

fn row_obarray() -> types::DumpObarray {
    types::DumpObarray {
        symbols: Vec::new(),
        global_members: Vec::new(),
        function_unbound: Vec::new(),
        function_epoch: 0,
        plain_rows: Some((0, 1)),
    }
}

fn row_words() -> [usize; 4] {
    let mut head = [0; 8];
    head[..4].copy_from_slice(&DUMP_SYMBOL.0.to_le_bytes());
    [
        usize::from_ne_bytes(head),
        crate::emacs_core::value::Value::NIL.bits(),
        crate::emacs_core::value::Value::UNBOUND.bits(),
        crate::emacs_core::value::Value::NIL.bits(),
    ]
}

#[test]
fn pdump_rejects_masked_row_flag_corruption() {
    use super::super::DumpError;
    let _symbols = SymbolRemap::new("pdump-invalid-row-trapped-write");
    for code in [3, 0x80, u8::MAX] {
        for field in [5, 6] {
            let heap = empty_heap();
            let mut words = row_words();
            let mut head = words[0].to_ne_bytes();
            head[field] = code;
            words[0] = usize::from_ne_bytes(head);
            let bytes = bytemuck::cast_slice_mut(&mut words);
            let view = super::super::mapped_heap::MappedHeapView::from_mut_slice(bytes);
            let mut decoder = convert::LoadDecoder::new_with_mapped_heap(&heap, Some(view));
            let error = convert::load_obarray(&mut decoder, &row_obarray()).unwrap_err();
            match field {
                5 => assert!(
                    matches!(error, DumpError::InvalidSymbolTrappedWrite(source) if source.number == code)
                ),
                6 => assert!(
                    matches!(error, DumpError::InvalidSymbolInterned(source) if source.number == code)
                ),
                _ => unreachable!(),
            }
        }
    }
}

// Root cause 1: apply after existing masked-byte tests have demonstrated red.
#[test]
fn pdump_preserves_typed_enum_decode_sources() {
    use super::super::DumpError;
    use crate::emacs_core::symbol::{SymbolInterned, SymbolTrappedWrite};
    use std::error::Error;

    let error = DumpError::from(SymbolTrappedWrite::try_from(3).unwrap_err());
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<num_enum::TryFromPrimitiveError<SymbolTrappedWrite>>()
            .unwrap()
            .number,
        3
    );
    let error = DumpError::from(SymbolInterned::try_from(3).unwrap_err());
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<num_enum::TryFromPrimitiveError<SymbolInterned>>()
            .unwrap()
            .number,
        3
    );
    let error = DumpError::from(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .kind(),
        std::io::ErrorKind::UnexpectedEof
    );
}

#[test]
fn pdump_localized_rebuild_preserves_validated_symbol_flags() {
    use crate::emacs_core::symbol::{SymbolInterned, SymbolRedirect, SymbolTrappedWrite};
    let _symbols = SymbolRemap::new("pdump-localized-validated-flags");
    for code in u8::MIN..=u8::MAX {
        let (Ok(trapped_write), Ok(interned)) = (
            SymbolTrappedWrite::try_from(code),
            SymbolInterned::try_from(code),
        ) else {
            continue;
        };
        let heap = empty_heap();
        let mut decoder = convert::LoadDecoder::new(&heap);
        let mut data = symbol_data();
        data.redirect = SymbolRedirect::Localized.into();
        data.trapped_write = trapped_write.into();
        data.interned = interned.into();
        data.declared_special = true;
        data.val = types::DumpSymbolVal::Localized {
            default: types::DumpValue::Nil,
            local_if_set: false,
            forwarder: None,
        };
        let obarray = convert::load_obarray(&mut decoder, &residual_obarray(data)).unwrap();
        let symbol = obarray.get_by_id(_symbols.runtime_symbol).unwrap();
        assert_eq!(symbol.flags.redirect(), SymbolRedirect::Localized);
        assert_eq!(symbol.flags.trapped_write(), trapped_write);
        assert_eq!(symbol.flags.interned(), interned);
        assert!(symbol.flags.declared_special());
    }
}
