//! Fixed-layout pdump section for coding-system manager state.
//!
//! GNU Emacs keeps coding-system definitions in global Lisp/C tables and
//! preserves those objects through pdumper roots.  This section moves Neomacs'
//! coding-system manager mirror out of RuntimeState bincode and into explicit
//! count-prefixed tables.

use bytemuck::{Pod, Zeroable};

use super::DumpError;
use super::object_value_codec::{Cursor, write_bool, write_u8, write_u32, write_u64, write_value};
use super::types::{
    DumpCodingSystemInfo, DumpCodingSystemManager, DumpEolType, DumpSymId, DumpValue,
};

const CODING_MAGIC: [u8; 16] = *b"NEOCODING\0\0\0\0\0\0\0";
const CODING_FORMAT_VERSION: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct CodingHeader {
    magic: [u8; 16],
    version: u32,
    header_size: u32,
    system_sym_count: u64,
    system_string_count: u64,
    alias_sym_count: u64,
    alias_string_count: u64,
    alias_order_sym_count: u64,
    alias_order_string_count: u64,
    priority_sym_count: u64,
    priority_string_count: u64,
    payload_offset: u64,
    payload_len: u64,
}

const HEADER_SIZE: usize = std::mem::size_of::<CodingHeader>();

pub(crate) fn coding_system_section_bytes(
    manager: &DumpCodingSystemManager,
) -> Result<Vec<u8>, DumpError> {
    let mut bytes = vec![0; HEADER_SIZE];

    for (sym, info) in &manager.systems_syms {
        write_u32(&mut bytes, sym.0);
        write_coding_system_info(&mut bytes, info)?;
    }
    for (name, info) in &manager.systems {
        write_string(&mut bytes, name)?;
        write_coding_system_info(&mut bytes, info)?;
    }
    for (alias, base) in &manager.aliases_syms {
        write_u32(&mut bytes, alias.0);
        write_u32(&mut bytes, base.0);
    }
    for (alias, base) in &manager.aliases {
        write_string(&mut bytes, alias)?;
        write_string(&mut bytes, base)?;
    }
    for (base, aliases) in &manager.alias_order_syms {
        write_u32(&mut bytes, base.0);
        write_sym_vec(&mut bytes, aliases)?;
    }
    for (base, aliases) in &manager.alias_order {
        write_string(&mut bytes, base)?;
        write_string_vec(&mut bytes, aliases)?;
    }
    for sym in &manager.priority_syms {
        write_u32(&mut bytes, sym.0);
    }
    for name in &manager.priority {
        write_string(&mut bytes, name)?;
    }
    write_opt_sym(&mut bytes, manager.keyboard_coding_sym);
    write_opt_string(&mut bytes, manager.keyboard_coding.as_deref())?;
    write_opt_sym(&mut bytes, manager.terminal_coding_sym);
    write_opt_string(&mut bytes, manager.terminal_coding.as_deref())?;

    let payload_len = bytes.len() - HEADER_SIZE;
    let header = CodingHeader {
        magic: CODING_MAGIC,
        version: CODING_FORMAT_VERSION,
        header_size: HEADER_SIZE as u32,
        system_sym_count: count_u64(manager.systems_syms.len(), "coding system symbol count")?,
        system_string_count: count_u64(manager.systems.len(), "coding system string count")?,
        alias_sym_count: count_u64(manager.aliases_syms.len(), "coding alias symbol count")?,
        alias_string_count: count_u64(manager.aliases.len(), "coding alias string count")?,
        alias_order_sym_count: count_u64(
            manager.alias_order_syms.len(),
            "coding alias order symbol count",
        )?,
        alias_order_string_count: count_u64(
            manager.alias_order.len(),
            "coding alias order string count",
        )?,
        priority_sym_count: count_u64(manager.priority_syms.len(), "coding priority symbol count")?,
        priority_string_count: count_u64(manager.priority.len(), "coding priority string count")?,
        payload_offset: HEADER_SIZE as u64,
        payload_len: payload_len as u64,
    };
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    Ok(bytes)
}

pub(crate) fn load_coding_system_section(
    section: &[u8],
) -> Result<DumpCodingSystemManager, DumpError> {
    let header = read_header(section)?;
    let payload_offset = usize::try_from(header.payload_offset)
        .map_err(|_| DumpError::ImageFormatError("coding payload offset overflows usize".into()))?;
    let payload_len = usize::try_from(header.payload_len)
        .map_err(|_| DumpError::ImageFormatError("coding payload length overflows usize".into()))?;
    let end = payload_offset
        .checked_add(payload_len)
        .ok_or_else(|| DumpError::ImageFormatError("coding payload range overflows".into()))?;
    if payload_offset < HEADER_SIZE || end > section.len() {
        return Err(DumpError::ImageFormatError(
            "coding payload range is outside section".into(),
        ));
    }

    let mut cursor = Cursor::new(&section[payload_offset..end]);
    let mut systems_syms = Vec::with_capacity(to_usize(
        header.system_sym_count,
        "coding system symbol count",
    )?);
    for _ in 0..header.system_sym_count {
        systems_syms.push((
            DumpSymId(cursor.read_u32("coding system symbol")?),
            read_coding_system_info(&mut cursor)?,
        ));
    }
    let mut systems = Vec::with_capacity(to_usize(
        header.system_string_count,
        "coding system string count",
    )?);
    for _ in 0..header.system_string_count {
        systems.push((
            read_string(&mut cursor)?,
            read_coding_system_info(&mut cursor)?,
        ));
    }
    let mut aliases_syms = Vec::with_capacity(to_usize(
        header.alias_sym_count,
        "coding alias symbol count",
    )?);
    for _ in 0..header.alias_sym_count {
        aliases_syms.push((
            DumpSymId(cursor.read_u32("coding alias symbol")?),
            DumpSymId(cursor.read_u32("coding alias base symbol")?),
        ));
    }
    let mut aliases = Vec::with_capacity(to_usize(
        header.alias_string_count,
        "coding alias string count",
    )?);
    for _ in 0..header.alias_string_count {
        aliases.push((read_string(&mut cursor)?, read_string(&mut cursor)?));
    }
    let mut alias_order_syms = Vec::with_capacity(to_usize(
        header.alias_order_sym_count,
        "coding alias order symbol count",
    )?);
    for _ in 0..header.alias_order_sym_count {
        alias_order_syms.push((
            DumpSymId(cursor.read_u32("coding alias order base symbol")?),
            read_sym_vec(&mut cursor)?,
        ));
    }
    let mut alias_order = Vec::with_capacity(to_usize(
        header.alias_order_string_count,
        "coding alias order string count",
    )?);
    for _ in 0..header.alias_order_string_count {
        alias_order.push((read_string(&mut cursor)?, read_string_vec(&mut cursor)?));
    }
    let mut priority_syms = Vec::with_capacity(to_usize(
        header.priority_sym_count,
        "coding priority symbol count",
    )?);
    for _ in 0..header.priority_sym_count {
        priority_syms.push(DumpSymId(cursor.read_u32("coding priority symbol")?));
    }
    let mut priority = Vec::with_capacity(to_usize(
        header.priority_string_count,
        "coding priority string count",
    )?);
    for _ in 0..header.priority_string_count {
        priority.push(read_string(&mut cursor)?);
    }

    let keyboard_coding_sym = read_opt_sym(&mut cursor)?;
    let keyboard_coding = read_opt_string(&mut cursor)?;
    let terminal_coding_sym = read_opt_sym(&mut cursor)?;
    let terminal_coding = read_opt_string(&mut cursor)?;

    if !cursor.is_empty() {
        return Err(DumpError::ImageFormatError(format!(
            "coding section has {} trailing payload bytes",
            cursor.remaining()
        )));
    }

    Ok(DumpCodingSystemManager {
        systems_syms,
        systems,
        aliases_syms,
        aliases,
        alias_order_syms,
        alias_order,
        priority_syms,
        priority,
        keyboard_coding_sym,
        keyboard_coding,
        terminal_coding_sym,
        terminal_coding,
    })
}

fn read_header(section: &[u8]) -> Result<CodingHeader, DumpError> {
    if section.len() < HEADER_SIZE {
        return Err(DumpError::ImageFormatError(format!(
            "coding section shorter than header: {} < {HEADER_SIZE}",
            section.len()
        )));
    }
    let header = *bytemuck::from_bytes::<CodingHeader>(&section[..HEADER_SIZE]);
    if header.magic != CODING_MAGIC {
        return Err(DumpError::ImageFormatError(
            "coding section has bad magic".into(),
        ));
    }
    if header.version != CODING_FORMAT_VERSION {
        return Err(DumpError::UnsupportedVersion(header.version));
    }
    if header.header_size != HEADER_SIZE as u32 {
        return Err(DumpError::ImageFormatError(format!(
            "coding header size {} does not match runtime header size {HEADER_SIZE}",
            header.header_size
        )));
    }
    Ok(header)
}

fn write_coding_system_info(
    out: &mut Vec<u8>,
    info: &DumpCodingSystemInfo,
) -> Result<(), DumpError> {
    write_opt_sym(out, info.name_sym);
    write_opt_string(out, info.name.as_deref())?;
    write_opt_sym(out, info.coding_type_sym);
    write_opt_string(out, info.coding_type.as_deref())?;
    write_char(out, info.mnemonic);
    write_eol_type(out, &info.eol_type);
    write_bool(out, info.ascii_compatible_p);
    write_sym_vec(out, &info.charset_list_syms)?;
    write_string_vec(out, &info.charset_list)?;
    write_opt_sym(out, info.post_read_conversion_sym);
    write_opt_string(out, info.post_read_conversion.as_deref())?;
    write_opt_sym(out, info.pre_write_conversion_sym);
    write_opt_string(out, info.pre_write_conversion.as_deref())?;
    write_opt_char(out, info.default_char);
    write_bool(out, info.for_unibyte);
    write_sym_value_pairs(out, &info.properties_syms)?;
    write_string_value_pairs(out, &info.properties)?;
    write_i64_value_pairs(out, &info.int_properties)?;
    Ok(())
}

fn read_coding_system_info(cursor: &mut Cursor<'_>) -> Result<DumpCodingSystemInfo, DumpError> {
    Ok(DumpCodingSystemInfo {
        name_sym: read_opt_sym(cursor)?,
        name: read_opt_string(cursor)?,
        coding_type_sym: read_opt_sym(cursor)?,
        coding_type: read_opt_string(cursor)?,
        mnemonic: read_char(cursor, "coding mnemonic")?,
        eol_type: read_eol_type(cursor)?,
        ascii_compatible_p: cursor.read_bool("coding ascii-compatible")?,
        charset_list_syms: read_sym_vec(cursor)?,
        charset_list: read_string_vec(cursor)?,
        post_read_conversion_sym: read_opt_sym(cursor)?,
        post_read_conversion: read_opt_string(cursor)?,
        pre_write_conversion_sym: read_opt_sym(cursor)?,
        pre_write_conversion: read_opt_string(cursor)?,
        default_char: read_opt_char(cursor, "coding default char")?,
        for_unibyte: cursor.read_bool("coding for-unibyte")?,
        properties_syms: read_sym_value_pairs(cursor)?,
        properties: read_string_value_pairs(cursor)?,
        int_properties: read_i64_value_pairs(cursor)?,
    })
}

const EOL_UNIX: u8 = 0;
const EOL_DOS: u8 = 1;
const EOL_MAC: u8 = 2;
const EOL_UNDECIDED: u8 = 3;

fn write_eol_type(out: &mut Vec<u8>, eol: &DumpEolType) {
    write_u8(
        out,
        match eol {
            DumpEolType::Unix => EOL_UNIX,
            DumpEolType::Dos => EOL_DOS,
            DumpEolType::Mac => EOL_MAC,
            DumpEolType::Undecided => EOL_UNDECIDED,
        },
    );
}

fn read_eol_type(cursor: &mut Cursor<'_>) -> Result<DumpEolType, DumpError> {
    match cursor.read_u8("coding eol type")? {
        EOL_UNIX => Ok(DumpEolType::Unix),
        EOL_DOS => Ok(DumpEolType::Dos),
        EOL_MAC => Ok(DumpEolType::Mac),
        EOL_UNDECIDED => Ok(DumpEolType::Undecided),
        other => Err(DumpError::ImageFormatError(format!(
            "unknown coding eol type tag {other}"
        ))),
    }
}

fn write_sym_vec(out: &mut Vec<u8>, syms: &[DumpSymId]) -> Result<(), DumpError> {
    write_len(out, syms.len(), "symbol vector count")?;
    for sym in syms {
        write_u32(out, sym.0);
    }
    Ok(())
}

fn read_sym_vec(cursor: &mut Cursor<'_>) -> Result<Vec<DumpSymId>, DumpError> {
    let len = read_len(cursor, "symbol vector count")?;
    let mut syms = Vec::with_capacity(len);
    for _ in 0..len {
        syms.push(DumpSymId(cursor.read_u32("symbol vector element")?));
    }
    Ok(syms)
}

fn write_string_vec(out: &mut Vec<u8>, values: &[String]) -> Result<(), DumpError> {
    write_len(out, values.len(), "string vector count")?;
    for value in values {
        write_string(out, value)?;
    }
    Ok(())
}

fn read_string_vec(cursor: &mut Cursor<'_>) -> Result<Vec<String>, DumpError> {
    let len = read_len(cursor, "string vector count")?;
    let mut values = Vec::with_capacity(len);
    for _ in 0..len {
        values.push(read_string(cursor)?);
    }
    Ok(values)
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

fn write_i64_value_pairs(out: &mut Vec<u8>, pairs: &[(i64, DumpValue)]) -> Result<(), DumpError> {
    write_len(out, pairs.len(), "i64/value pair count")?;
    for (key, value) in pairs {
        write_i64(out, *key);
        write_value(out, value)?;
    }
    Ok(())
}

fn read_i64_value_pairs(cursor: &mut Cursor<'_>) -> Result<Vec<(i64, DumpValue)>, DumpError> {
    let len = read_len(cursor, "i64/value pair count")?;
    let mut pairs = Vec::with_capacity(len);
    for _ in 0..len {
        pairs.push((read_i64(cursor, "i64/value key")?, cursor.read_value()?));
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

fn write_opt_char(out: &mut Vec<u8>, value: Option<char>) {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_char(out, value);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_char(cursor: &mut Cursor<'_>, what: &str) -> Result<Option<char>, DumpError> {
    if cursor.read_bool("optional char present")? {
        Ok(Some(read_char(cursor, what)?))
    } else {
        Ok(None)
    }
}

fn write_char(out: &mut Vec<u8>, value: char) {
    write_u32(out, value as u32);
}

fn read_char(cursor: &mut Cursor<'_>, what: &str) -> Result<char, DumpError> {
    let value = cursor.read_u32(what)?;
    char::from_u32(value).ok_or_else(|| {
        DumpError::ImageFormatError(format!("{what} has invalid char scalar {value}"))
    })
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

pub(crate) fn empty_coding_system_manager() -> DumpCodingSystemManager {
    DumpCodingSystemManager {
        systems_syms: Vec::new(),
        systems: Vec::new(),
        aliases_syms: Vec::new(),
        aliases: Vec::new(),
        alias_order_syms: Vec::new(),
        alias_order: Vec::new(),
        priority_syms: Vec::new(),
        priority: Vec::new(),
        keyboard_coding_sym: None,
        keyboard_coding: None,
        terminal_coding_sym: None,
        terminal_coding: None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::DumpHeapRef;
    use super::*;

    #[test]
    fn coding_system_section_round_trips_manager_state() {
        let manager = DumpCodingSystemManager {
            systems_syms: vec![(
                DumpSymId(1),
                DumpCodingSystemInfo {
                    name_sym: Some(DumpSymId(2)),
                    name: None,
                    coding_type_sym: Some(DumpSymId(3)),
                    coding_type: None,
                    mnemonic: 'U',
                    eol_type: DumpEolType::Unix,
                    ascii_compatible_p: true,
                    charset_list_syms: vec![DumpSymId(4), DumpSymId(5)],
                    charset_list: Vec::new(),
                    post_read_conversion_sym: Some(DumpSymId(6)),
                    post_read_conversion: None,
                    pre_write_conversion_sym: None,
                    pre_write_conversion: Some("legacy-pre-write".into()),
                    default_char: Some('?'),
                    for_unibyte: false,
                    properties_syms: vec![(DumpSymId(7), DumpValue::Int(8))],
                    properties: vec![("legacy-prop".into(), DumpValue::True)],
                    int_properties: vec![(9, DumpValue::Vector(DumpHeapRef { index: 10 }))],
                },
            )],
            systems: vec![(
                "legacy-system".into(),
                DumpCodingSystemInfo {
                    name_sym: None,
                    name: Some("legacy-system".into()),
                    coding_type_sym: None,
                    coding_type: Some("utf-8".into()),
                    mnemonic: 'L',
                    eol_type: DumpEolType::Dos,
                    ascii_compatible_p: false,
                    charset_list_syms: Vec::new(),
                    charset_list: vec!["charset".into()],
                    post_read_conversion_sym: None,
                    post_read_conversion: None,
                    pre_write_conversion_sym: None,
                    pre_write_conversion: None,
                    default_char: None,
                    for_unibyte: true,
                    properties_syms: Vec::new(),
                    properties: Vec::new(),
                    int_properties: Vec::new(),
                },
            )],
            aliases_syms: vec![(DumpSymId(11), DumpSymId(12))],
            aliases: vec![("legacy-alias".into(), "legacy-base".into())],
            alias_order_syms: vec![(DumpSymId(16), vec![DumpSymId(16), DumpSymId(17)])],
            alias_order: vec![(
                "legacy-base".into(),
                vec!["legacy-base".into(), "legacy-alias".into()],
            )],
            priority_syms: vec![DumpSymId(13)],
            priority: vec!["legacy-priority".into()],
            keyboard_coding_sym: Some(DumpSymId(14)),
            keyboard_coding: Some("legacy-keyboard".into()),
            terminal_coding_sym: Some(DumpSymId(15)),
            terminal_coding: Some("legacy-terminal".into()),
        };

        let bytes = coding_system_section_bytes(&manager).expect("encode coding systems");
        let decoded = load_coding_system_section(&bytes).expect("decode coding systems");

        assert_eq!(format!("{decoded:?}"), format!("{manager:?}"));
    }

    #[test]
    fn coding_system_section_rejects_bad_magic() {
        let mut bytes =
            coding_system_section_bytes(&empty_coding_system_manager()).expect("encode coding");
        bytes[0] ^= 1;
        let err = load_coding_system_section(&bytes).expect_err("bad magic should fail");
        assert!(matches!(err, DumpError::ImageFormatError(_)));
    }
}
