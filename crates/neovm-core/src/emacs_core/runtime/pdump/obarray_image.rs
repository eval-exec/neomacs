//! Fixed-layout pdump section for obarray symbol state.
//!
//! GNU pdumper dumps symbols and obarray buckets as mapped objects with
//! relocations.  Neomacs still reconstructs its Rust obarray manager, but the
//! file image now carries the symbol value/function/plist state outside the
//! RuntimeState bincode blob.

use bytemuck::{Pod, Zeroable};

use super::DumpError;
use super::object_value_codec::{Cursor, write_bool, write_u8, write_u32, write_value};
use super::types::{DumpLocalizedForwarder, DumpObarray, DumpSymId, DumpSymbolData, DumpSymbolVal};

const OBARRAY_MAGIC: [u8; 16] = *b"NEOOBARRAY\0\0\0\0\0\0";
const OBARRAY_FORMAT_VERSION: u32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ObarrayHeader {
    magic: [u8; 16],
    version: u32,
    header_size: u32,
    symbol_count: u64,
    global_member_count: u64,
    function_unbound_count: u64,
    function_epoch: u64,
    payload_offset: u64,
    payload_len: u64,
    /// Heap-image offset of the fixed symbol-row region (see
    /// `DumpObarray::plain_rows`); meaningful only when `rows_count > 0`.
    rows_offset: u64,
    rows_count: u64,
}

const HEADER_SIZE: usize = std::mem::size_of::<ObarrayHeader>();

pub(crate) fn obarray_section_bytes(obarray: &DumpObarray) -> Result<Vec<u8>, DumpError> {
    let symbol_count = u64::try_from(obarray.symbols.len()).map_err(|_| {
        DumpError::SerializationError("pdump obarray symbol count overflows u64".into())
    })?;
    let global_member_count = u64::try_from(obarray.global_members.len()).map_err(|_| {
        DumpError::SerializationError("pdump obarray global member count overflows u64".into())
    })?;
    let function_unbound_count = u64::try_from(obarray.function_unbound.len()).map_err(|_| {
        DumpError::SerializationError("pdump obarray function-unbound count overflows u64".into())
    })?;

    let mut bytes = vec![0; HEADER_SIZE];
    for (sym, data) in &obarray.symbols {
        write_u32(&mut bytes, sym.0);
        write_symbol_data(&mut bytes, data)?;
    }
    write_sym_ids(&mut bytes, &obarray.global_members);
    write_sym_ids(&mut bytes, &obarray.function_unbound);

    let payload_len = bytes.len() - HEADER_SIZE;
    let header = ObarrayHeader {
        magic: OBARRAY_MAGIC,
        version: OBARRAY_FORMAT_VERSION,
        header_size: HEADER_SIZE as u32,
        symbol_count,
        global_member_count,
        function_unbound_count,
        function_epoch: obarray.function_epoch,
        payload_offset: HEADER_SIZE as u64,
        payload_len: payload_len as u64,
        rows_offset: obarray.plain_rows.map_or(0, |(offset, _)| offset),
        rows_count: obarray.plain_rows.map_or(0, |(_, count)| count),
    };
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    Ok(bytes)
}

pub(crate) fn load_obarray_section(section: &[u8]) -> Result<DumpObarray, DumpError> {
    let header = read_header(section)?;
    let payload_offset = usize::try_from(header.payload_offset).map_err(|_| {
        DumpError::ImageFormatError("obarray payload offset overflows usize".into())
    })?;
    let payload_len = usize::try_from(header.payload_len).map_err(|_| {
        DumpError::ImageFormatError("obarray payload length overflows usize".into())
    })?;
    let end = payload_offset
        .checked_add(payload_len)
        .ok_or_else(|| DumpError::ImageFormatError("obarray payload range overflows".into()))?;
    if payload_offset < HEADER_SIZE || end > section.len() {
        return Err(DumpError::ImageFormatError(
            "obarray payload range is outside section".into(),
        ));
    }

    let symbol_count = usize::try_from(header.symbol_count)
        .map_err(|_| DumpError::ImageFormatError("obarray symbol count overflows usize".into()))?;
    let global_member_count = usize::try_from(header.global_member_count).map_err(|_| {
        DumpError::ImageFormatError("obarray global member count overflows usize".into())
    })?;
    let function_unbound_count = usize::try_from(header.function_unbound_count).map_err(|_| {
        DumpError::ImageFormatError("obarray function-unbound count overflows usize".into())
    })?;

    let mut cursor = Cursor::new(&section[payload_offset..end]);
    let mut symbols = Vec::with_capacity(symbol_count);
    for _ in 0..symbol_count {
        symbols.push((
            DumpSymId(cursor.read_u32("obarray symbol id")?),
            read_symbol_data(&mut cursor)?,
        ));
    }
    let global_members = read_sym_ids(&mut cursor, global_member_count, "obarray global member")?;
    let function_unbound = read_sym_ids(
        &mut cursor,
        function_unbound_count,
        "obarray function-unbound",
    )?;
    if !cursor.is_empty() {
        return Err(DumpError::ImageFormatError(format!(
            "obarray section has {} trailing payload bytes",
            cursor.remaining()
        )));
    }

    Ok(DumpObarray {
        symbols,
        global_members,
        function_unbound,
        function_epoch: header.function_epoch,
        plain_rows: (header.rows_count > 0).then_some((header.rows_offset, header.rows_count)),
    })
}

fn read_header(section: &[u8]) -> Result<ObarrayHeader, DumpError> {
    if section.len() < HEADER_SIZE {
        return Err(DumpError::ImageFormatError(format!(
            "obarray section shorter than header: {} < {HEADER_SIZE}",
            section.len()
        )));
    }
    let header = *bytemuck::from_bytes::<ObarrayHeader>(&section[..HEADER_SIZE]);
    if header.magic != OBARRAY_MAGIC {
        return Err(DumpError::ImageFormatError(
            "obarray section has bad magic".into(),
        ));
    }
    if header.version != OBARRAY_FORMAT_VERSION {
        return Err(DumpError::UnsupportedVersion(header.version));
    }
    if header.header_size != HEADER_SIZE as u32 {
        return Err(DumpError::ImageFormatError(format!(
            "obarray header size {} does not match runtime header size {HEADER_SIZE}",
            header.header_size
        )));
    }
    Ok(header)
}

fn write_sym_ids(out: &mut Vec<u8>, syms: &[DumpSymId]) {
    for sym in syms {
        write_u32(out, sym.0);
    }
}

fn read_sym_ids(
    cursor: &mut Cursor<'_>,
    count: usize,
    label: &str,
) -> Result<Vec<DumpSymId>, DumpError> {
    let mut syms = Vec::with_capacity(count);
    for _ in 0..count {
        syms.push(DumpSymId(cursor.read_u32(label)?));
    }
    Ok(syms)
}

const SYMBOL_VAL_PLAIN: u8 = 0;
const SYMBOL_VAL_ALIAS: u8 = 1;
const SYMBOL_VAL_LOCALIZED: u8 = 2;
const SYMBOL_VAL_FORWARDED: u8 = 3;
const SYMBOL_VAL_BOOL_FORWARDED: u8 = 4;
const SYMBOL_VAL_INT_FORWARDED: u8 = 5;
const SYMBOL_VAL_OBJ_FORWARDED: u8 = 6;
const SYMBOL_VAL_KBOARD_FORWARDED: u8 = 7;

fn write_symbol_data(out: &mut Vec<u8>, data: &DumpSymbolData) -> Result<(), DumpError> {
    write_u8(out, data.redirect);
    write_u8(out, data.trapped_write);
    write_u8(out, data.interned);
    write_bool(out, data.declared_special);
    write_symbol_val(out, &data.val)?;
    write_value(out, &data.function)?;
    write_value(out, &data.plist)?;
    Ok(())
}

fn read_symbol_data(cursor: &mut Cursor<'_>) -> Result<DumpSymbolData, DumpError> {
    Ok(DumpSymbolData {
        redirect: cursor.read_u8("symbol redirect")?,
        trapped_write: cursor.read_u8("symbol trapped-write")?,
        interned: cursor.read_u8("symbol interned")?,
        declared_special: cursor.read_bool("symbol declared-special")?,
        val: read_symbol_val(cursor)?,
        function: cursor.read_value()?,
        plist: cursor.read_value()?,
    })
}

/// Tag byte for the forwarder a `Localized` symbol's BLV carries (pdump v58).
///
/// `make_blv` copies the declaration's descriptor into the BLV
/// (`src/data.c:2112-2140`), and the descriptor is a process-lifetime pointer,
/// so the image records the KIND and `load_obarray` rebuilds an equivalent one
/// from the default it also carries.
const LOCALIZED_FWD_NONE: u8 = 0;
const LOCALIZED_FWD_BOOL: u8 = 1;
const LOCALIZED_FWD_INT: u8 = 2;
const LOCALIZED_FWD_OBJ: u8 = 3;
const LOCALIZED_FWD_KBOARD: u8 = 4;

fn encode_localized_forwarder(kind: Option<DumpLocalizedForwarder>) -> u8 {
    match kind {
        None => LOCALIZED_FWD_NONE,
        Some(DumpLocalizedForwarder::Bool) => LOCALIZED_FWD_BOOL,
        Some(DumpLocalizedForwarder::Int) => LOCALIZED_FWD_INT,
        Some(DumpLocalizedForwarder::Obj) => LOCALIZED_FWD_OBJ,
        Some(DumpLocalizedForwarder::Kboard) => LOCALIZED_FWD_KBOARD,
    }
}

fn decode_localized_forwarder(tag: u8) -> Result<Option<DumpLocalizedForwarder>, DumpError> {
    match tag {
        LOCALIZED_FWD_NONE => Ok(None),
        LOCALIZED_FWD_BOOL => Ok(Some(DumpLocalizedForwarder::Bool)),
        LOCALIZED_FWD_INT => Ok(Some(DumpLocalizedForwarder::Int)),
        LOCALIZED_FWD_OBJ => Ok(Some(DumpLocalizedForwarder::Obj)),
        LOCALIZED_FWD_KBOARD => Ok(Some(DumpLocalizedForwarder::Kboard)),
        other => Err(DumpError::ImageFormatError(format!(
            "unknown localized forwarder tag {other}"
        ))),
    }
}

fn write_symbol_val(out: &mut Vec<u8>, val: &DumpSymbolVal) -> Result<(), DumpError> {
    match val {
        DumpSymbolVal::Plain(value) => {
            write_u8(out, SYMBOL_VAL_PLAIN);
            write_value(out, value)?;
        }
        DumpSymbolVal::Alias(sym) => {
            write_u8(out, SYMBOL_VAL_ALIAS);
            write_u32(out, sym.0);
        }
        DumpSymbolVal::Localized {
            default,
            local_if_set,
            forwarder,
        } => {
            write_u8(out, SYMBOL_VAL_LOCALIZED);
            write_value(out, default)?;
            write_bool(out, *local_if_set);
            write_u8(out, encode_localized_forwarder(*forwarder));
        }
        DumpSymbolVal::Forwarded => write_u8(out, SYMBOL_VAL_FORWARDED),
        DumpSymbolVal::BoolForwarded(value) => {
            write_u8(out, SYMBOL_VAL_BOOL_FORWARDED);
            write_bool(out, *value);
        }
        DumpSymbolVal::IntForwarded(value) => {
            write_u8(out, SYMBOL_VAL_INT_FORWARDED);
            write_value(out, value)?;
        }
        DumpSymbolVal::ObjForwarded(value) => {
            write_u8(out, SYMBOL_VAL_OBJ_FORWARDED);
            write_value(out, value)?;
        }
        DumpSymbolVal::KboardForwarded(value) => {
            write_u8(out, SYMBOL_VAL_KBOARD_FORWARDED);
            write_value(out, value)?;
        }
    }
    Ok(())
}

fn read_symbol_val(cursor: &mut Cursor<'_>) -> Result<DumpSymbolVal, DumpError> {
    match cursor.read_u8("symbol value-cell tag")? {
        SYMBOL_VAL_PLAIN => Ok(DumpSymbolVal::Plain(cursor.read_value()?)),
        SYMBOL_VAL_ALIAS => Ok(DumpSymbolVal::Alias(DumpSymId(
            cursor.read_u32("symbol alias target")?,
        ))),
        SYMBOL_VAL_LOCALIZED => Ok(DumpSymbolVal::Localized {
            default: cursor.read_value()?,
            local_if_set: cursor.read_bool("localized local-if-set")?,
            forwarder: decode_localized_forwarder(cursor.read_u8("localized forwarder kind")?)?,
        }),
        SYMBOL_VAL_FORWARDED => Ok(DumpSymbolVal::Forwarded),
        SYMBOL_VAL_BOOL_FORWARDED => Ok(DumpSymbolVal::BoolForwarded(
            cursor.read_bool("Boolean forwarder value")?,
        )),
        SYMBOL_VAL_INT_FORWARDED => Ok(DumpSymbolVal::IntForwarded(cursor.read_value()?)),
        SYMBOL_VAL_OBJ_FORWARDED => Ok(DumpSymbolVal::ObjForwarded(cursor.read_value()?)),
        SYMBOL_VAL_KBOARD_FORWARDED => Ok(DumpSymbolVal::KboardForwarded(cursor.read_value()?)),
        other => Err(DumpError::ImageFormatError(format!(
            "unknown symbol value-cell tag {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
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
}
