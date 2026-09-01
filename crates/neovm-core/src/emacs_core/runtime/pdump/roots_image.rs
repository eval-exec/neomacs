//! Fixed-layout pdump section for top-level Lisp roots.
//!
//! GNU pdumper records root objects in the mapped image and fixes global
//! runtime pointers through relocation/copy records.  This section moves the
//! Neomacs evaluator's simple Lisp roots out of RuntimeState bincode while the
//! remaining manager state is migrated section by section.

use bytemuck::{Pod, Zeroable};

use super::DumpError;
use super::object_value_codec::{Cursor, write_u32, write_u64, write_value};
use super::types::{
    DumpLispString, DumpOrderedSymMap, DumpRuntimeBindingValue, DumpSymId, DumpValue,
};

const ROOTS_MAGIC: [u8; 16] = *b"NEOROOTS\0\0\0\0\0\0\0\0";
const ROOTS_FORMAT_VERSION: u32 = 3;

#[derive(Debug, Clone)]
pub(crate) struct DumpRootState {
    pub dynamic: Vec<DumpOrderedSymMap>,
    pub lexenv: DumpValue,
    pub features: Vec<DumpSymId>,
    pub require_stack: Vec<DumpSymId>,
    pub loads_in_progress: Vec<DumpLispString>,
    pub standard_syntax_table: DumpValue,
    pub syntax_code_objects: DumpValue,
    pub standard_category_table: DumpValue,
    pub current_local_map: DumpValue,
    pub current_global_map: DumpValue,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct RootsHeader {
    magic: [u8; 16],
    version: u32,
    header_size: u32,
    dynamic_count: u64,
    feature_count: u64,
    require_stack_count: u64,
    loads_in_progress_count: u64,
    payload_offset: u64,
    payload_len: u64,
}

const HEADER_SIZE: usize = std::mem::size_of::<RootsHeader>();

pub(crate) fn roots_section_bytes(roots: &DumpRootState) -> Result<Vec<u8>, DumpError> {
    let dynamic_count = u64::try_from(roots.dynamic.len()).map_err(|_| {
        DumpError::SerializationError("pdump dynamic root count overflows u64".into())
    })?;
    let feature_count = u64::try_from(roots.features.len())
        .map_err(|_| DumpError::SerializationError("pdump feature count overflows u64".into()))?;
    let require_stack_count = u64::try_from(roots.require_stack.len()).map_err(|_| {
        DumpError::SerializationError("pdump require-stack count overflows u64".into())
    })?;
    let loads_in_progress_count = u64::try_from(roots.loads_in_progress.len()).map_err(|_| {
        DumpError::SerializationError("pdump loads-in-progress count overflows u64".into())
    })?;

    let mut bytes = vec![0; HEADER_SIZE];
    for frame in &roots.dynamic {
        write_ordered_sym_map(&mut bytes, frame)?;
    }
    write_value(&mut bytes, &roots.lexenv)?;
    write_sym_ids(&mut bytes, &roots.features);
    write_sym_ids(&mut bytes, &roots.require_stack);
    for load in &roots.loads_in_progress {
        write_lisp_string(&mut bytes, load)?;
    }
    write_value(&mut bytes, &roots.standard_syntax_table)?;
    write_value(&mut bytes, &roots.syntax_code_objects)?;
    write_value(&mut bytes, &roots.standard_category_table)?;
    write_value(&mut bytes, &roots.current_local_map)?;
    write_value(&mut bytes, &roots.current_global_map)?;

    let payload_len = bytes.len() - HEADER_SIZE;
    let header = RootsHeader {
        magic: ROOTS_MAGIC,
        version: ROOTS_FORMAT_VERSION,
        header_size: HEADER_SIZE as u32,
        dynamic_count,
        feature_count,
        require_stack_count,
        loads_in_progress_count,
        payload_offset: HEADER_SIZE as u64,
        payload_len: payload_len as u64,
    };
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    Ok(bytes)
}

pub(crate) fn load_roots_section(section: &[u8]) -> Result<DumpRootState, DumpError> {
    let header = read_header(section)?;
    let payload_offset = usize::try_from(header.payload_offset)
        .map_err(|_| DumpError::ImageFormatError("roots payload offset overflows usize".into()))?;
    let payload_len = usize::try_from(header.payload_len)
        .map_err(|_| DumpError::ImageFormatError("roots payload length overflows usize".into()))?;
    let end = payload_offset
        .checked_add(payload_len)
        .ok_or_else(|| DumpError::ImageFormatError("roots payload range overflows".into()))?;
    if payload_offset < HEADER_SIZE || end > section.len() {
        return Err(DumpError::ImageFormatError(
            "roots payload range is outside section".into(),
        ));
    }

    let dynamic_count = usize::try_from(header.dynamic_count)
        .map_err(|_| DumpError::ImageFormatError("dynamic root count overflows usize".into()))?;
    let feature_count = usize::try_from(header.feature_count)
        .map_err(|_| DumpError::ImageFormatError("feature count overflows usize".into()))?;
    let require_stack_count = usize::try_from(header.require_stack_count)
        .map_err(|_| DumpError::ImageFormatError("require-stack count overflows usize".into()))?;
    let loads_in_progress_count =
        usize::try_from(header.loads_in_progress_count).map_err(|_| {
            DumpError::ImageFormatError("loads-in-progress count overflows usize".into())
        })?;

    let mut cursor = Cursor::new(&section[payload_offset..end]);
    let mut dynamic = Vec::with_capacity(dynamic_count);
    for _ in 0..dynamic_count {
        dynamic.push(read_ordered_sym_map(&mut cursor)?);
    }
    let lexenv = cursor.read_value()?;
    let features = read_sym_ids(&mut cursor, feature_count, "feature symbol id")?;
    let require_stack = read_sym_ids(&mut cursor, require_stack_count, "require-stack symbol id")?;
    let mut loads_in_progress = Vec::with_capacity(loads_in_progress_count);
    for _ in 0..loads_in_progress_count {
        loads_in_progress.push(read_lisp_string(&mut cursor)?);
    }
    let standard_syntax_table = cursor.read_value()?;
    let syntax_code_objects = cursor.read_value()?;
    let standard_category_table = cursor.read_value()?;
    let current_local_map = cursor.read_value()?;
    let current_global_map = cursor.read_value()?;

    if !cursor.is_empty() {
        return Err(DumpError::ImageFormatError(format!(
            "roots section has {} trailing payload bytes",
            cursor.remaining()
        )));
    }

    Ok(DumpRootState {
        dynamic,
        lexenv,
        features,
        require_stack,
        loads_in_progress,
        standard_syntax_table,
        syntax_code_objects,
        standard_category_table,
        current_local_map,
        current_global_map,
    })
}

fn read_header(section: &[u8]) -> Result<RootsHeader, DumpError> {
    if section.len() < HEADER_SIZE {
        return Err(DumpError::ImageFormatError(format!(
            "roots section shorter than header: {} < {HEADER_SIZE}",
            section.len()
        )));
    }
    let header = *bytemuck::from_bytes::<RootsHeader>(&section[..HEADER_SIZE]);
    if header.magic != ROOTS_MAGIC {
        return Err(DumpError::ImageFormatError(
            "roots section has bad magic".into(),
        ));
    }
    if header.version != ROOTS_FORMAT_VERSION {
        return Err(DumpError::UnsupportedVersion(header.version));
    }
    if header.header_size != HEADER_SIZE as u32 {
        return Err(DumpError::ImageFormatError(format!(
            "roots header size {} does not match runtime header size {HEADER_SIZE}",
            header.header_size
        )));
    }
    Ok(header)
}

fn write_ordered_sym_map(out: &mut Vec<u8>, frame: &DumpOrderedSymMap) -> Result<(), DumpError> {
    write_len(out, frame.entries.len(), "dynamic binding count")?;
    for (sym, value) in &frame.entries {
        write_u32(out, sym.0);
        write_runtime_binding_value(out, value)?;
    }
    Ok(())
}

fn read_ordered_sym_map(cursor: &mut Cursor<'_>) -> Result<DumpOrderedSymMap, DumpError> {
    let len = read_len(cursor, "dynamic binding count")?;
    let mut entries = Vec::with_capacity(len);
    for _ in 0..len {
        entries.push((
            DumpSymId(cursor.read_u32("dynamic binding symbol")?),
            read_runtime_binding_value(cursor)?,
        ));
    }
    Ok(DumpOrderedSymMap { entries })
}

const RUNTIME_BINDING_BOUND: u8 = 0;
const RUNTIME_BINDING_VOID: u8 = 1;

fn write_runtime_binding_value(
    out: &mut Vec<u8>,
    value: &DumpRuntimeBindingValue,
) -> Result<(), DumpError> {
    match value {
        DumpRuntimeBindingValue::Bound(value) => {
            super::object_value_codec::write_u8(out, RUNTIME_BINDING_BOUND);
            write_value(out, value)?;
        }
        DumpRuntimeBindingValue::Void => {
            super::object_value_codec::write_u8(out, RUNTIME_BINDING_VOID);
        }
    }
    Ok(())
}

fn read_runtime_binding_value(
    cursor: &mut Cursor<'_>,
) -> Result<DumpRuntimeBindingValue, DumpError> {
    match cursor.read_u8("runtime binding value tag")? {
        RUNTIME_BINDING_BOUND => Ok(DumpRuntimeBindingValue::Bound(cursor.read_value()?)),
        RUNTIME_BINDING_VOID => Ok(DumpRuntimeBindingValue::Void),
        other => Err(DumpError::ImageFormatError(format!(
            "unknown runtime binding value tag {other}"
        ))),
    }
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

fn write_lisp_string(out: &mut Vec<u8>, string: &DumpLispString) -> Result<(), DumpError> {
    write_bytes(out, &string.data)?;
    write_usize(out, string.size)?;
    write_i64(out, string.size_byte);
    Ok(())
}

fn read_lisp_string(cursor: &mut Cursor<'_>) -> Result<DumpLispString, DumpError> {
    Ok(DumpLispString {
        data: read_bytes(cursor)?,
        size: read_usize(cursor, "lisp string char size")?,
        size_byte: read_i64(cursor, "lisp string byte size")?,
    })
}

fn write_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), DumpError> {
    write_len(out, bytes.len(), "byte payload length")?;
    out.extend_from_slice(bytes);
    Ok(())
}

fn read_bytes(cursor: &mut Cursor<'_>) -> Result<Vec<u8>, DumpError> {
    let len = read_len(cursor, "byte payload length")?;
    let mut bytes = Vec::with_capacity(len);
    for _ in 0..len {
        bytes.push(cursor.read_u8("byte payload")?);
    }
    Ok(bytes)
}

fn write_len(out: &mut Vec<u8>, len: usize, what: &str) -> Result<(), DumpError> {
    let len = u64::try_from(len)
        .map_err(|_| DumpError::SerializationError(format!("{what} overflows u64")))?;
    write_u64(out, len);
    Ok(())
}

fn read_len(cursor: &mut Cursor<'_>, what: &str) -> Result<usize, DumpError> {
    let len = cursor.read_u64(what)?;
    usize::try_from(len).map_err(|_| DumpError::ImageFormatError(format!("{what} overflows usize")))
}

fn write_usize(out: &mut Vec<u8>, value: usize) -> Result<(), DumpError> {
    let value = u64::try_from(value)
        .map_err(|_| DumpError::SerializationError("usize value overflows u64".into()))?;
    write_u64(out, value);
    Ok(())
}

fn read_usize(cursor: &mut Cursor<'_>, what: &str) -> Result<usize, DumpError> {
    let value = cursor.read_u64(what)?;
    usize::try_from(value)
        .map_err(|_| DumpError::ImageFormatError(format!("{what} overflows usize")))
}

fn write_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_ne_bytes());
}

fn read_i64(cursor: &mut Cursor<'_>, what: &str) -> Result<i64, DumpError> {
    let raw = cursor.read_u64(what)?;
    Ok(i64::from_ne_bytes(raw.to_ne_bytes()))
}

#[cfg(test)]
mod tests {
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
}
