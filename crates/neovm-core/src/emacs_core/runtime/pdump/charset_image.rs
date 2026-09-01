//! Fixed-layout pdump section for charset registry state.
//!
//! Charset data is runtime-global table state in GNU Emacs.  This section
//! moves Neomacs' dump mirror out of RuntimeState bincode and into explicit
//! tables with stable method tags.

use bytemuck::{Pod, Zeroable};

use super::DumpError;
use super::object_value_codec::{Cursor, write_bool, write_u8, write_u32, write_u64, write_value};
use super::types::{
    DumpCharsetInfo, DumpCharsetMethod, DumpCharsetRegistry, DumpCharsetSubsetSpec, DumpSymId,
    DumpValue,
};

const CHARSET_MAGIC: [u8; 16] = *b"NEOCHARSET\0\0\0\0\0\0";
const CHARSET_FORMAT_VERSION: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct CharsetHeader {
    magic: [u8; 16],
    version: u32,
    header_size: u32,
    charset_count: u64,
    priority_sym_count: u64,
    priority_string_count: u64,
    next_id: i64,
    payload_offset: u64,
    payload_len: u64,
}

const HEADER_SIZE: usize = std::mem::size_of::<CharsetHeader>();

pub(crate) fn charset_section_bytes(registry: &DumpCharsetRegistry) -> Result<Vec<u8>, DumpError> {
    let mut bytes = vec![0; HEADER_SIZE];

    for charset in &registry.charsets {
        write_charset_info(&mut bytes, charset)?;
    }
    for sym in &registry.priority_syms {
        write_u32(&mut bytes, sym.0);
    }
    for name in &registry.priority {
        write_string(&mut bytes, name)?;
    }

    let payload_len = bytes.len() - HEADER_SIZE;
    let header = CharsetHeader {
        magic: CHARSET_MAGIC,
        version: CHARSET_FORMAT_VERSION,
        header_size: HEADER_SIZE as u32,
        charset_count: count_u64(registry.charsets.len(), "charset count")?,
        priority_sym_count: count_u64(registry.priority_syms.len(), "charset priority symbols")?,
        priority_string_count: count_u64(registry.priority.len(), "charset priority strings")?,
        next_id: registry.next_id,
        payload_offset: HEADER_SIZE as u64,
        payload_len: payload_len as u64,
    };
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    Ok(bytes)
}

pub(crate) fn load_charset_section(section: &[u8]) -> Result<DumpCharsetRegistry, DumpError> {
    let header = read_header(section)?;
    let payload_offset = usize::try_from(header.payload_offset).map_err(|_| {
        DumpError::ImageFormatError("charset payload offset overflows usize".into())
    })?;
    let payload_len = usize::try_from(header.payload_len).map_err(|_| {
        DumpError::ImageFormatError("charset payload length overflows usize".into())
    })?;
    let end = payload_offset
        .checked_add(payload_len)
        .ok_or_else(|| DumpError::ImageFormatError("charset payload range overflows".into()))?;
    if payload_offset < HEADER_SIZE || end > section.len() {
        return Err(DumpError::ImageFormatError(
            "charset payload range is outside section".into(),
        ));
    }

    let mut cursor = Cursor::new(&section[payload_offset..end]);
    let mut charsets = Vec::with_capacity(to_usize(header.charset_count, "charset count")?);
    for _ in 0..header.charset_count {
        charsets.push(read_charset_info(&mut cursor)?);
    }
    let mut priority_syms = Vec::with_capacity(to_usize(
        header.priority_sym_count,
        "charset priority symbols",
    )?);
    for _ in 0..header.priority_sym_count {
        priority_syms.push(DumpSymId(cursor.read_u32("charset priority symbol")?));
    }
    let mut priority = Vec::with_capacity(to_usize(
        header.priority_string_count,
        "charset priority strings",
    )?);
    for _ in 0..header.priority_string_count {
        priority.push(read_string(&mut cursor)?);
    }

    if !cursor.is_empty() {
        return Err(DumpError::ImageFormatError(format!(
            "charset section has {} trailing payload bytes",
            cursor.remaining()
        )));
    }

    Ok(DumpCharsetRegistry {
        charsets,
        priority_syms,
        priority,
        next_id: header.next_id,
    })
}

fn read_header(section: &[u8]) -> Result<CharsetHeader, DumpError> {
    if section.len() < HEADER_SIZE {
        return Err(DumpError::ImageFormatError(format!(
            "charset section shorter than header: {} < {HEADER_SIZE}",
            section.len()
        )));
    }
    let header = *bytemuck::from_bytes::<CharsetHeader>(&section[..HEADER_SIZE]);
    if header.magic != CHARSET_MAGIC {
        return Err(DumpError::ImageFormatError(
            "charset section has bad magic".into(),
        ));
    }
    if header.version != CHARSET_FORMAT_VERSION {
        return Err(DumpError::UnsupportedVersion(header.version));
    }
    if header.header_size != HEADER_SIZE as u32 {
        return Err(DumpError::ImageFormatError(format!(
            "charset header size {} does not match runtime header size {HEADER_SIZE}",
            header.header_size
        )));
    }
    Ok(header)
}

fn write_charset_info(out: &mut Vec<u8>, info: &DumpCharsetInfo) -> Result<(), DumpError> {
    write_i64(out, info.id);
    write_opt_sym(out, info.name_sym);
    write_opt_string(out, info.name.as_deref())?;
    write_i64(out, info.dimension);
    for value in info.code_space {
        write_i64(out, value);
    }
    write_i64(out, info.min_code);
    write_i64(out, info.max_code);
    write_opt_i64(out, info.iso_final_char);
    write_opt_i64(out, info.iso_revision);
    write_opt_i64(out, info.emacs_mule_id);
    write_bool(out, info.ascii_compatible_p);
    write_bool(out, info.supplementary_p);
    write_bool(out, info.unified_p);
    write_opt_i64(out, info.invalid_code);
    write_value(out, &info.unify_map)?;
    write_charset_method(out, &info.method)?;
    write_sym_value_pairs(out, &info.plist_syms)?;
    write_string_value_pairs(out, &info.plist)?;
    Ok(())
}

fn read_charset_info(cursor: &mut Cursor<'_>) -> Result<DumpCharsetInfo, DumpError> {
    let mut code_space = [0; 8];
    Ok(DumpCharsetInfo {
        id: read_i64(cursor, "charset id")?,
        name_sym: read_opt_sym(cursor)?,
        name: read_opt_string(cursor)?,
        dimension: read_i64(cursor, "charset dimension")?,
        code_space: {
            for value in &mut code_space {
                *value = read_i64(cursor, "charset code-space entry")?;
            }
            code_space
        },
        min_code: read_i64(cursor, "charset min-code")?,
        max_code: read_i64(cursor, "charset max-code")?,
        iso_final_char: read_opt_i64(cursor, "charset iso-final-char")?,
        iso_revision: read_opt_i64(cursor, "charset iso-revision")?,
        emacs_mule_id: read_opt_i64(cursor, "charset emacs-mule-id")?,
        ascii_compatible_p: cursor.read_bool("charset ascii-compatible")?,
        supplementary_p: cursor.read_bool("charset supplementary")?,
        unified_p: cursor.read_bool("charset unified")?,
        invalid_code: read_opt_i64(cursor, "charset invalid-code")?,
        unify_map: cursor.read_value()?,
        method: read_charset_method(cursor)?,
        plist_syms: read_sym_value_pairs(cursor)?,
        plist: read_string_value_pairs(cursor)?,
    })
}

const METHOD_OFFSET: u8 = 0;
const METHOD_MAP: u8 = 1;
const METHOD_SUBSET: u8 = 2;
const METHOD_SUPERSET_SYMS: u8 = 3;
const METHOD_SUPERSET_STRINGS: u8 = 4;

fn write_charset_method(out: &mut Vec<u8>, method: &DumpCharsetMethod) -> Result<(), DumpError> {
    match method {
        DumpCharsetMethod::Offset(offset) => {
            write_u8(out, METHOD_OFFSET);
            write_i64(out, *offset);
        }
        DumpCharsetMethod::Map(name) => {
            write_u8(out, METHOD_MAP);
            write_string(out, name)?;
        }
        DumpCharsetMethod::Subset(spec) => {
            write_u8(out, METHOD_SUBSET);
            write_subset_spec(out, spec)?;
        }
        DumpCharsetMethod::SupersetSyms(entries) => {
            write_u8(out, METHOD_SUPERSET_SYMS);
            write_sym_i64_pairs(out, entries)?;
        }
        DumpCharsetMethod::Superset(entries) => {
            write_u8(out, METHOD_SUPERSET_STRINGS);
            write_string_i64_pairs(out, entries)?;
        }
    }
    Ok(())
}

fn read_charset_method(cursor: &mut Cursor<'_>) -> Result<DumpCharsetMethod, DumpError> {
    match cursor.read_u8("charset method tag")? {
        METHOD_OFFSET => Ok(DumpCharsetMethod::Offset(read_i64(
            cursor,
            "charset offset method",
        )?)),
        METHOD_MAP => Ok(DumpCharsetMethod::Map(read_string(cursor)?)),
        METHOD_SUBSET => Ok(DumpCharsetMethod::Subset(read_subset_spec(cursor)?)),
        METHOD_SUPERSET_SYMS => Ok(DumpCharsetMethod::SupersetSyms(read_sym_i64_pairs(cursor)?)),
        METHOD_SUPERSET_STRINGS => Ok(DumpCharsetMethod::Superset(read_string_i64_pairs(cursor)?)),
        other => Err(DumpError::ImageFormatError(format!(
            "unknown charset method tag {other}"
        ))),
    }
}

fn write_subset_spec(out: &mut Vec<u8>, spec: &DumpCharsetSubsetSpec) -> Result<(), DumpError> {
    write_opt_sym(out, spec.parent_sym);
    write_opt_string(out, spec.parent.as_deref())?;
    write_i64(out, spec.parent_min_code);
    write_i64(out, spec.parent_max_code);
    write_i64(out, spec.offset);
    Ok(())
}

fn read_subset_spec(cursor: &mut Cursor<'_>) -> Result<DumpCharsetSubsetSpec, DumpError> {
    Ok(DumpCharsetSubsetSpec {
        parent_sym: read_opt_sym(cursor)?,
        parent: read_opt_string(cursor)?,
        parent_min_code: read_i64(cursor, "charset subset min code")?,
        parent_max_code: read_i64(cursor, "charset subset max code")?,
        offset: read_i64(cursor, "charset subset offset")?,
    })
}

fn write_sym_value_pairs(
    out: &mut Vec<u8>,
    pairs: &[(DumpSymId, DumpValue)],
) -> Result<(), DumpError> {
    write_len(out, pairs.len(), "symbol/value pair count")?;
    for (sym, value) in pairs {
        write_u32(out, sym.0);
        write_value(out, value)?;
    }
    Ok(())
}

fn read_sym_value_pairs(cursor: &mut Cursor<'_>) -> Result<Vec<(DumpSymId, DumpValue)>, DumpError> {
    let len = read_len(cursor, "symbol/value pair count")?;
    let mut pairs = Vec::with_capacity(len);
    for _ in 0..len {
        pairs.push((
            DumpSymId(cursor.read_u32("symbol/value symbol")?),
            cursor.read_value()?,
        ));
    }
    Ok(pairs)
}

fn write_string_value_pairs(
    out: &mut Vec<u8>,
    pairs: &[(String, DumpValue)],
) -> Result<(), DumpError> {
    write_len(out, pairs.len(), "string/value pair count")?;
    for (key, value) in pairs {
        write_string(out, key)?;
        write_value(out, value)?;
    }
    Ok(())
}

fn read_string_value_pairs(cursor: &mut Cursor<'_>) -> Result<Vec<(String, DumpValue)>, DumpError> {
    let len = read_len(cursor, "string/value pair count")?;
    let mut pairs = Vec::with_capacity(len);
    for _ in 0..len {
        pairs.push((read_string(cursor)?, cursor.read_value()?));
    }
    Ok(pairs)
}

fn write_sym_i64_pairs(out: &mut Vec<u8>, pairs: &[(DumpSymId, i64)]) -> Result<(), DumpError> {
    write_len(out, pairs.len(), "symbol/i64 pair count")?;
    for (sym, value) in pairs {
        write_u32(out, sym.0);
        write_i64(out, *value);
    }
    Ok(())
}

fn read_sym_i64_pairs(cursor: &mut Cursor<'_>) -> Result<Vec<(DumpSymId, i64)>, DumpError> {
    let len = read_len(cursor, "symbol/i64 pair count")?;
    let mut pairs = Vec::with_capacity(len);
    for _ in 0..len {
        pairs.push((
            DumpSymId(cursor.read_u32("symbol/i64 symbol")?),
            read_i64(cursor, "symbol/i64 value")?,
        ));
    }
    Ok(pairs)
}

fn write_string_i64_pairs(out: &mut Vec<u8>, pairs: &[(String, i64)]) -> Result<(), DumpError> {
    write_len(out, pairs.len(), "string/i64 pair count")?;
    for (key, value) in pairs {
        write_string(out, key)?;
        write_i64(out, *value);
    }
    Ok(())
}

fn read_string_i64_pairs(cursor: &mut Cursor<'_>) -> Result<Vec<(String, i64)>, DumpError> {
    let len = read_len(cursor, "string/i64 pair count")?;
    let mut pairs = Vec::with_capacity(len);
    for _ in 0..len {
        pairs.push((read_string(cursor)?, read_i64(cursor, "string/i64 value")?));
    }
    Ok(pairs)
}

fn write_opt_sym(out: &mut Vec<u8>, sym: Option<DumpSymId>) {
    match sym {
        Some(sym) => {
            write_bool(out, true);
            write_u32(out, sym.0);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_sym(cursor: &mut Cursor<'_>) -> Result<Option<DumpSymId>, DumpError> {
    if cursor.read_bool("optional symbol present")? {
        Ok(Some(DumpSymId(cursor.read_u32("optional symbol")?)))
    } else {
        Ok(None)
    }
}

fn write_opt_string(out: &mut Vec<u8>, value: Option<&str>) -> Result<(), DumpError> {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_string(out, value)?;
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn read_opt_string(cursor: &mut Cursor<'_>) -> Result<Option<String>, DumpError> {
    if cursor.read_bool("optional string present")? {
        Ok(Some(read_string(cursor)?))
    } else {
        Ok(None)
    }
}

fn write_opt_i64(out: &mut Vec<u8>, value: Option<i64>) {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_i64(out, value);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_i64(cursor: &mut Cursor<'_>, what: &str) -> Result<Option<i64>, DumpError> {
    if cursor.read_bool("optional i64 present")? {
        Ok(Some(read_i64(cursor, what)?))
    } else {
        Ok(None)
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), DumpError> {
    write_len(out, value.len(), "string byte length")?;
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn read_string(cursor: &mut Cursor<'_>) -> Result<String, DumpError> {
    let len = read_len(cursor, "string byte length")?;
    let mut bytes = Vec::with_capacity(len);
    for _ in 0..len {
        bytes.push(cursor.read_u8("string byte")?);
    }
    String::from_utf8(bytes)
        .map_err(|err| DumpError::ImageFormatError(format!("invalid UTF-8 string: {err}")))
}

fn write_len(out: &mut Vec<u8>, len: usize, what: &str) -> Result<(), DumpError> {
    write_u64(out, count_u64(len, what)?);
    Ok(())
}

fn read_len(cursor: &mut Cursor<'_>, what: &str) -> Result<usize, DumpError> {
    to_usize(cursor.read_u64(what)?, what)
}

fn write_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_ne_bytes());
}

fn read_i64(cursor: &mut Cursor<'_>, what: &str) -> Result<i64, DumpError> {
    Ok(i64::from_ne_bytes(cursor.read_u64(what)?.to_ne_bytes()))
}

fn count_u64(count: usize, what: &str) -> Result<u64, DumpError> {
    u64::try_from(count).map_err(|_| DumpError::SerializationError(format!("{what} overflows u64")))
}

fn to_usize(count: u64, what: &str) -> Result<usize, DumpError> {
    usize::try_from(count)
        .map_err(|_| DumpError::ImageFormatError(format!("{what} overflows usize")))
}

pub(crate) fn empty_charset_registry() -> DumpCharsetRegistry {
    DumpCharsetRegistry {
        charsets: Vec::new(),
        priority_syms: Vec::new(),
        priority: Vec::new(),
        next_id: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::DumpHeapRef;
    use super::*;

    #[test]
    fn charset_section_round_trips_registry_state() {
        let registry = DumpCharsetRegistry {
            charsets: vec![
                DumpCharsetInfo {
                    id: 1,
                    name_sym: Some(DumpSymId(2)),
                    name: Some("charset-one".into()),
                    dimension: 2,
                    code_space: [0, 127, 128, 255, 0, 0, 0, 0],
                    min_code: 0,
                    max_code: 255,
                    iso_final_char: Some(65),
                    iso_revision: Some(1),
                    emacs_mule_id: None,
                    ascii_compatible_p: true,
                    supplementary_p: false,
                    unified_p: true,
                    invalid_code: Some(-1),
                    unify_map: DumpValue::Vector(DumpHeapRef { index: 3 }),
                    method: DumpCharsetMethod::Subset(DumpCharsetSubsetSpec {
                        parent_sym: Some(DumpSymId(4)),
                        parent: Some("parent".into()),
                        parent_min_code: 10,
                        parent_max_code: 20,
                        offset: 30,
                    }),
                    plist_syms: vec![(DumpSymId(5), DumpValue::Int(6))],
                    plist: vec![("prop".into(), DumpValue::True)],
                },
                DumpCharsetInfo {
                    id: 7,
                    name_sym: None,
                    name: Some("charset-two".into()),
                    dimension: 1,
                    code_space: [0, 255, 0, 0, 0, 0, 0, 0],
                    min_code: 0,
                    max_code: 255,
                    iso_final_char: None,
                    iso_revision: None,
                    emacs_mule_id: Some(9),
                    ascii_compatible_p: false,
                    supplementary_p: true,
                    unified_p: false,
                    invalid_code: None,
                    unify_map: DumpValue::Nil,
                    method: DumpCharsetMethod::SupersetSyms(vec![(DumpSymId(8), 9)]),
                    plist_syms: Vec::new(),
                    plist: Vec::new(),
                },
            ],
            priority_syms: vec![DumpSymId(10)],
            priority: vec!["charset-one".into()],
            next_id: 11,
        };

        let bytes = charset_section_bytes(&registry).expect("encode charset registry");
        let decoded = load_charset_section(&bytes).expect("decode charset registry");

        assert_eq!(format!("{decoded:?}"), format!("{registry:?}"));
    }

    #[test]
    fn charset_section_rejects_bad_magic() {
        let mut bytes =
            charset_section_bytes(&empty_charset_registry()).expect("encode charset registry");
        bytes[0] ^= 1;
        let err = load_charset_section(&bytes).expect_err("bad magic should fail");
        assert!(matches!(err, DumpError::ImageFormatError(_)));
    }
}
