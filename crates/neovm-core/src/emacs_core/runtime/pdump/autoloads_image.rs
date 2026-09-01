//! Fixed-layout pdump section for autoload manager state.
//!
//! Autoload metadata is the largest remaining RuntimeState payload.  This
//! section keeps it in explicit count-prefixed tables with stable tags instead
//! of relying on serde/bincode layout.

use bytemuck::{Pod, Zeroable};

use super::DumpError;
use super::object_value_codec::{Cursor, write_bool, write_u8, write_u32, write_u64, write_value};
use super::types::{
    DumpAutoloadEntry, DumpAutoloadManager, DumpAutoloadType, DumpLispString, DumpSymId, DumpValue,
};

const AUTOLOADS_MAGIC: [u8; 16] = *b"NEOAUTOLOADS\0\0\0\0";
const AUTOLOADS_FORMAT_VERSION: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct AutoloadsHeader {
    magic: [u8; 16],
    version: u32,
    header_size: u32,
    entries_sym_count: u64,
    entries_string_count: u64,
    after_load_lisp_count: u64,
    after_load_string_count: u64,
    loaded_file_count: u64,
    obsolete_function_sym_count: u64,
    obsolete_function_string_count: u64,
    obsolete_variable_sym_count: u64,
    obsolete_variable_string_count: u64,
    payload_offset: u64,
    payload_len: u64,
}

const HEADER_SIZE: usize = std::mem::size_of::<AutoloadsHeader>();

pub(crate) fn autoloads_section_bytes(
    autoloads: &DumpAutoloadManager,
) -> Result<Vec<u8>, DumpError> {
    let mut bytes = vec![0; HEADER_SIZE];

    for (sym, entry) in &autoloads.entries_syms {
        write_u32(&mut bytes, sym.0);
        write_autoload_entry(&mut bytes, entry)?;
    }
    for (name, entry) in &autoloads.entries {
        write_string(&mut bytes, name)?;
        write_autoload_entry(&mut bytes, entry)?;
    }
    for (file, actions) in &autoloads.after_load_lisp {
        write_lisp_string(&mut bytes, file)?;
        write_values(&mut bytes, actions)?;
    }
    for (file, actions) in &autoloads.after_load {
        write_string(&mut bytes, file)?;
        write_values(&mut bytes, actions)?;
    }
    for file in &autoloads.loaded_files {
        write_lisp_string(&mut bytes, file)?;
    }
    for (sym, (when, kind)) in &autoloads.obsolete_functions_syms {
        write_u32(&mut bytes, sym.0);
        write_lisp_string(&mut bytes, when)?;
        write_lisp_string(&mut bytes, kind)?;
    }
    for (name, (when, kind)) in &autoloads.obsolete_functions {
        write_string(&mut bytes, name)?;
        write_string(&mut bytes, when)?;
        write_string(&mut bytes, kind)?;
    }
    for (sym, (when, kind)) in &autoloads.obsolete_variables_syms {
        write_u32(&mut bytes, sym.0);
        write_lisp_string(&mut bytes, when)?;
        write_lisp_string(&mut bytes, kind)?;
    }
    for (name, (when, kind)) in &autoloads.obsolete_variables {
        write_string(&mut bytes, name)?;
        write_string(&mut bytes, when)?;
        write_string(&mut bytes, kind)?;
    }

    let payload_len = bytes.len() - HEADER_SIZE;
    let header = AutoloadsHeader {
        magic: AUTOLOADS_MAGIC,
        version: AUTOLOADS_FORMAT_VERSION,
        header_size: HEADER_SIZE as u32,
        entries_sym_count: count_u64(autoloads.entries_syms.len(), "autoload symbol entries")?,
        entries_string_count: count_u64(autoloads.entries.len(), "autoload string entries")?,
        after_load_lisp_count: count_u64(autoloads.after_load_lisp.len(), "after-load lisp")?,
        after_load_string_count: count_u64(autoloads.after_load.len(), "after-load string")?,
        loaded_file_count: count_u64(autoloads.loaded_files.len(), "loaded files")?,
        obsolete_function_sym_count: count_u64(
            autoloads.obsolete_functions_syms.len(),
            "obsolete function symbols",
        )?,
        obsolete_function_string_count: count_u64(
            autoloads.obsolete_functions.len(),
            "obsolete function strings",
        )?,
        obsolete_variable_sym_count: count_u64(
            autoloads.obsolete_variables_syms.len(),
            "obsolete variable symbols",
        )?,
        obsolete_variable_string_count: count_u64(
            autoloads.obsolete_variables.len(),
            "obsolete variable strings",
        )?,
        payload_offset: HEADER_SIZE as u64,
        payload_len: payload_len as u64,
    };
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    Ok(bytes)
}

pub(crate) fn load_autoloads_section(section: &[u8]) -> Result<DumpAutoloadManager, DumpError> {
    let header = read_header(section)?;
    let payload_offset = usize::try_from(header.payload_offset).map_err(|_| {
        DumpError::ImageFormatError("autoloads payload offset overflows usize".into())
    })?;
    let payload_len = usize::try_from(header.payload_len).map_err(|_| {
        DumpError::ImageFormatError("autoloads payload length overflows usize".into())
    })?;
    let end = payload_offset
        .checked_add(payload_len)
        .ok_or_else(|| DumpError::ImageFormatError("autoloads payload range overflows".into()))?;
    if payload_offset < HEADER_SIZE || end > section.len() {
        return Err(DumpError::ImageFormatError(
            "autoloads payload range is outside section".into(),
        ));
    }

    let mut cursor = Cursor::new(&section[payload_offset..end]);
    let mut entries_syms = Vec::with_capacity(to_usize(header.entries_sym_count, "entries_syms")?);
    for _ in 0..header.entries_sym_count {
        entries_syms.push((
            DumpSymId(cursor.read_u32("autoload symbol id")?),
            read_autoload_entry(&mut cursor)?,
        ));
    }
    let mut entries = Vec::with_capacity(to_usize(header.entries_string_count, "entries")?);
    for _ in 0..header.entries_string_count {
        entries.push((read_string(&mut cursor)?, read_autoload_entry(&mut cursor)?));
    }
    let mut after_load_lisp =
        Vec::with_capacity(to_usize(header.after_load_lisp_count, "after_load_lisp")?);
    for _ in 0..header.after_load_lisp_count {
        after_load_lisp.push((read_lisp_string(&mut cursor)?, read_values(&mut cursor)?));
    }
    let mut after_load =
        Vec::with_capacity(to_usize(header.after_load_string_count, "after_load")?);
    for _ in 0..header.after_load_string_count {
        after_load.push((read_string(&mut cursor)?, read_values(&mut cursor)?));
    }
    let mut loaded_files = Vec::with_capacity(to_usize(header.loaded_file_count, "loaded_files")?);
    for _ in 0..header.loaded_file_count {
        loaded_files.push(read_lisp_string(&mut cursor)?);
    }
    let mut obsolete_functions_syms = Vec::with_capacity(to_usize(
        header.obsolete_function_sym_count,
        "obsolete_functions_syms",
    )?);
    for _ in 0..header.obsolete_function_sym_count {
        obsolete_functions_syms.push((
            DumpSymId(cursor.read_u32("obsolete function symbol id")?),
            (
                read_lisp_string(&mut cursor)?,
                read_lisp_string(&mut cursor)?,
            ),
        ));
    }
    let mut obsolete_functions = Vec::with_capacity(to_usize(
        header.obsolete_function_string_count,
        "obsolete_functions",
    )?);
    for _ in 0..header.obsolete_function_string_count {
        obsolete_functions.push((
            read_string(&mut cursor)?,
            (read_string(&mut cursor)?, read_string(&mut cursor)?),
        ));
    }
    let mut obsolete_variables_syms = Vec::with_capacity(to_usize(
        header.obsolete_variable_sym_count,
        "obsolete_variables_syms",
    )?);
    for _ in 0..header.obsolete_variable_sym_count {
        obsolete_variables_syms.push((
            DumpSymId(cursor.read_u32("obsolete variable symbol id")?),
            (
                read_lisp_string(&mut cursor)?,
                read_lisp_string(&mut cursor)?,
            ),
        ));
    }
    let mut obsolete_variables = Vec::with_capacity(to_usize(
        header.obsolete_variable_string_count,
        "obsolete_variables",
    )?);
    for _ in 0..header.obsolete_variable_string_count {
        obsolete_variables.push((
            read_string(&mut cursor)?,
            (read_string(&mut cursor)?, read_string(&mut cursor)?),
        ));
    }

    if !cursor.is_empty() {
        return Err(DumpError::ImageFormatError(format!(
            "autoloads section has {} trailing payload bytes",
            cursor.remaining()
        )));
    }

    Ok(DumpAutoloadManager {
        entries_syms,
        entries,
        after_load_lisp,
        after_load,
        loaded_files,
        obsolete_functions_syms,
        obsolete_functions,
        obsolete_variables_syms,
        obsolete_variables,
    })
}

fn read_header(section: &[u8]) -> Result<AutoloadsHeader, DumpError> {
    if section.len() < HEADER_SIZE {
        return Err(DumpError::ImageFormatError(format!(
            "autoloads section shorter than header: {} < {HEADER_SIZE}",
            section.len()
        )));
    }
    let header = *bytemuck::from_bytes::<AutoloadsHeader>(&section[..HEADER_SIZE]);
    if header.magic != AUTOLOADS_MAGIC {
        return Err(DumpError::ImageFormatError(
            "autoloads section has bad magic".into(),
        ));
    }
    if header.version != AUTOLOADS_FORMAT_VERSION {
        return Err(DumpError::UnsupportedVersion(header.version));
    }
    if header.header_size != HEADER_SIZE as u32 {
        return Err(DumpError::ImageFormatError(format!(
            "autoloads header size {} does not match runtime header size {HEADER_SIZE}",
            header.header_size
        )));
    }
    Ok(header)
}

const AUTOLOAD_FUNCTION: u8 = 0;
const AUTOLOAD_MACRO: u8 = 1;
const AUTOLOAD_KEYMAP: u8 = 2;

fn write_autoload_entry(out: &mut Vec<u8>, entry: &DumpAutoloadEntry) -> Result<(), DumpError> {
    write_lisp_string(out, &entry.file)?;
    write_opt_lisp_string(out, entry.docstring.as_ref())?;
    write_bool(out, entry.interactive);
    write_u8(
        out,
        match entry.autoload_type {
            DumpAutoloadType::Function => AUTOLOAD_FUNCTION,
            DumpAutoloadType::Macro => AUTOLOAD_MACRO,
            DumpAutoloadType::Keymap => AUTOLOAD_KEYMAP,
        },
    );
    Ok(())
}

fn read_autoload_entry(cursor: &mut Cursor<'_>) -> Result<DumpAutoloadEntry, DumpError> {
    Ok(DumpAutoloadEntry {
        file: read_lisp_string(cursor)?,
        docstring: read_opt_lisp_string(cursor)?,
        interactive: cursor.read_bool("autoload interactive flag")?,
        autoload_type: match cursor.read_u8("autoload type")? {
            AUTOLOAD_FUNCTION => DumpAutoloadType::Function,
            AUTOLOAD_MACRO => DumpAutoloadType::Macro,
            AUTOLOAD_KEYMAP => DumpAutoloadType::Keymap,
            other => {
                return Err(DumpError::ImageFormatError(format!(
                    "unknown autoload type tag {other}"
                )));
            }
        },
    })
}

fn write_values(out: &mut Vec<u8>, values: &[DumpValue]) -> Result<(), DumpError> {
    write_len(out, values.len(), "autoload value count")?;
    for value in values {
        write_value(out, value)?;
    }
    Ok(())
}

fn read_values(cursor: &mut Cursor<'_>) -> Result<Vec<DumpValue>, DumpError> {
    let len = read_len(cursor, "autoload value count")?;
    let mut values = Vec::with_capacity(len);
    for _ in 0..len {
        values.push(cursor.read_value()?);
    }
    Ok(values)
}

fn write_opt_lisp_string(
    out: &mut Vec<u8>,
    value: Option<&DumpLispString>,
) -> Result<(), DumpError> {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_lisp_string(out, value)?;
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn read_opt_lisp_string(cursor: &mut Cursor<'_>) -> Result<Option<DumpLispString>, DumpError> {
    if cursor.read_bool("lisp string present")? {
        Ok(Some(read_lisp_string(cursor)?))
    } else {
        Ok(None)
    }
}

fn write_lisp_string(out: &mut Vec<u8>, value: &DumpLispString) -> Result<(), DumpError> {
    write_bytes(out, &value.data)?;
    write_usize(out, value.size)?;
    write_i64(out, value.size_byte);
    Ok(())
}

fn read_lisp_string(cursor: &mut Cursor<'_>) -> Result<DumpLispString, DumpError> {
    Ok(DumpLispString {
        data: read_bytes(cursor)?,
        size: read_usize(cursor, "lisp string char size")?,
        size_byte: read_i64(cursor, "lisp string byte size")?,
    })
}

fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), DumpError> {
    write_bytes(out, value.as_bytes())
}

fn read_string(cursor: &mut Cursor<'_>) -> Result<String, DumpError> {
    let bytes = read_bytes(cursor)?;
    String::from_utf8(bytes)
        .map_err(|err| DumpError::ImageFormatError(format!("invalid UTF-8 string: {err}")))
}

fn write_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), DumpError> {
    write_len(out, bytes.len(), "byte payload length")?;
    out.extend_from_slice(bytes);
    Ok(())
}

fn read_bytes(cursor: &mut Cursor<'_>) -> Result<Vec<u8>, DumpError> {
    let len = read_len(cursor, "byte payload length")?;
    cursor.read_bytes_fixed(len)
}

fn write_len(out: &mut Vec<u8>, len: usize, what: &str) -> Result<(), DumpError> {
    write_u64(out, count_u64(len, what)?);
    Ok(())
}

fn read_len(cursor: &mut Cursor<'_>, what: &str) -> Result<usize, DumpError> {
    to_usize(cursor.read_u64(what)?, what)
}

fn write_usize(out: &mut Vec<u8>, value: usize) -> Result<(), DumpError> {
    write_u64(out, count_u64(value, "usize value")?);
    Ok(())
}

fn read_usize(cursor: &mut Cursor<'_>, what: &str) -> Result<usize, DumpError> {
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

pub(crate) fn empty_autoloads() -> DumpAutoloadManager {
    DumpAutoloadManager {
        entries_syms: Vec::new(),
        entries: Vec::new(),
        after_load_lisp: Vec::new(),
        after_load: Vec::new(),
        loaded_files: Vec::new(),
        obsolete_functions_syms: Vec::new(),
        obsolete_functions: Vec::new(),
        obsolete_variables_syms: Vec::new(),
        obsolete_variables: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
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
}
