//! Fixed-layout pdump section for the dump-local symbol table.
//!
//! GNU pdumper writes Lisp object state into mapped dump sections instead of
//! deserializing a monolithic runtime blob.  This section moves Neomacs' symbol
//! interner metadata onto that path: fixed headers plus raw name bytes, decoded
//! directly from the mapped image on load.

use bytemuck::{Pod, Zeroable};

use super::DumpError;
use super::types::DumpSymbolTable;
use crate::heap_types::LispString;

const SYMBOL_TABLE_MAGIC: [u8; 16] = *b"NEOSYMTABLE\0\0\0\0\0";
const SYMBOL_TABLE_FORMAT_VERSION: u32 = 1;
const SECTION_ALIGN: usize = 8;
const SYMBOL_CANONICAL_FLAG: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SymbolTableHeader {
    magic: [u8; 16],
    version: u32,
    header_size: u32,
    name_count: u32,
    symbol_count: u32,
    name_table_offset: u64,
    symbol_table_offset: u64,
    byte_data_offset: u64,
    byte_data_len: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SymbolNameEntry {
    byte_offset: u64,
    byte_len: u64,
    size: u64,
    size_byte: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SymbolEntry {
    name_id: u32,
    flags: u32,
}

const HEADER_SIZE: usize = std::mem::size_of::<SymbolTableHeader>();
const NAME_ENTRY_SIZE: usize = std::mem::size_of::<SymbolNameEntry>();
const SYMBOL_ENTRY_SIZE: usize = std::mem::size_of::<SymbolEntry>();

pub(crate) fn symbol_table_section_bytes(table: &DumpSymbolTable) -> Result<Vec<u8>, DumpError> {
    let name_count = u32::try_from(table.names.len()).map_err(|_| {
        DumpError::SerializationError("pdump symbol table has too many names".into())
    })?;
    let symbol_count = u32::try_from(table.symbols.len()).map_err(|_| {
        DumpError::SerializationError("pdump symbol table has too many symbols".into())
    })?;

    let mut name_entries = Vec::with_capacity(table.names.len());
    let mut byte_data = Vec::new();
    for name in &table.names {
        let byte_offset = u64::try_from(byte_data.len()).map_err(|_| {
            DumpError::SerializationError("pdump symbol name byte data is too large".into())
        })?;
        byte_data.extend_from_slice(name.as_bytes());
        byte_data.push(0);
        let byte_len = u64::try_from(name.as_bytes().len()).map_err(|_| {
            DumpError::SerializationError("pdump symbol name byte length overflows u64".into())
        })?;
        let size = u64::try_from(name.schars()).map_err(|_| {
            DumpError::SerializationError("pdump symbol name char length overflows u64".into())
        })?;
        let size_byte = if name.is_multibyte() {
            i64::try_from(name.as_bytes().len()).map_err(|_| {
                DumpError::SerializationError("pdump symbol name byte length overflows i64".into())
            })?
        } else {
            -1
        };
        name_entries.push(SymbolNameEntry {
            byte_offset,
            byte_len,
            size,
            size_byte,
        });
    }

    let mut symbol_entries = Vec::with_capacity(table.symbols.len());
    for symbol in &table.symbols {
        symbol_entries.push(SymbolEntry {
            name_id: symbol.name.0,
            flags: u32::from(symbol.canonical) * SYMBOL_CANONICAL_FLAG,
        });
    }

    let name_table_offset = HEADER_SIZE;
    let symbol_table_offset = align_up(
        checked_add(
            name_table_offset,
            checked_mul(table.names.len(), NAME_ENTRY_SIZE)?,
        )?,
        SECTION_ALIGN,
    );
    let byte_data_offset = align_up(
        checked_add(
            symbol_table_offset,
            checked_mul(table.symbols.len(), SYMBOL_ENTRY_SIZE)?,
        )?,
        SECTION_ALIGN,
    );
    let total_len = checked_add(byte_data_offset, byte_data.len())?;
    let mut bytes = vec![0u8; total_len];

    let header = SymbolTableHeader {
        magic: SYMBOL_TABLE_MAGIC,
        version: SYMBOL_TABLE_FORMAT_VERSION,
        header_size: HEADER_SIZE as u32,
        name_count,
        symbol_count,
        name_table_offset: name_table_offset as u64,
        symbol_table_offset: symbol_table_offset as u64,
        byte_data_offset: byte_data_offset as u64,
        byte_data_len: byte_data.len() as u64,
    };
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));

    for (idx, entry) in name_entries.iter().enumerate() {
        let start = name_table_offset + idx * NAME_ENTRY_SIZE;
        bytes[start..start + NAME_ENTRY_SIZE].copy_from_slice(bytemuck::bytes_of(entry));
    }
    for (idx, entry) in symbol_entries.iter().enumerate() {
        let start = symbol_table_offset + idx * SYMBOL_ENTRY_SIZE;
        bytes[start..start + SYMBOL_ENTRY_SIZE].copy_from_slice(bytemuck::bytes_of(entry));
    }
    bytes[byte_data_offset..byte_data_offset + byte_data.len()].copy_from_slice(&byte_data);

    Ok(bytes)
}

pub(crate) fn load_symbol_table_section(section: &[u8]) -> Result<(), DumpError> {
    with_symbol_table_section(section, |names, symbol_names, canonical| {
        super::convert::load_symbol_table_parts(names, symbol_names, canonical)
    })
}

fn with_symbol_table_section<R>(
    section: &[u8],
    f: impl FnOnce(&[LispString], &[u32], &[bool]) -> Result<R, DumpError>,
) -> Result<R, DumpError> {
    let header = read_pod::<SymbolTableHeader>(section, 0, "symbol table header")?;
    if header.magic != SYMBOL_TABLE_MAGIC {
        return Err(DumpError::ImageFormatError(
            "symbol table section has bad magic".into(),
        ));
    }
    if header.version != SYMBOL_TABLE_FORMAT_VERSION {
        return Err(DumpError::UnsupportedVersion(header.version));
    }
    if header.header_size != HEADER_SIZE as u32 {
        return Err(DumpError::ImageFormatError(format!(
            "symbol table header size {} does not match runtime header size {HEADER_SIZE}",
            header.header_size
        )));
    }

    let name_count = usize::try_from(header.name_count).map_err(|_| {
        DumpError::ImageFormatError("symbol table name count overflows usize".into())
    })?;
    let symbol_count = usize::try_from(header.symbol_count).map_err(|_| {
        DumpError::ImageFormatError("symbol table symbol count overflows usize".into())
    })?;
    let name_table = checked_range(
        header.name_table_offset,
        checked_mul(name_count, NAME_ENTRY_SIZE)?,
        section.len(),
        "symbol name table",
    )?;
    let symbol_table = checked_range(
        header.symbol_table_offset,
        checked_mul(symbol_count, SYMBOL_ENTRY_SIZE)?,
        section.len(),
        "symbol slot table",
    )?;
    let byte_data = checked_range(
        header.byte_data_offset,
        usize::try_from(header.byte_data_len).map_err(|_| {
            DumpError::ImageFormatError("symbol name byte data length overflows usize".into())
        })?,
        section.len(),
        "symbol name byte data",
    )?;

    if name_table.start < HEADER_SIZE {
        return Err(DumpError::ImageFormatError(
            "symbol name table overlaps header".into(),
        ));
    }
    if name_table.end > symbol_table.start || symbol_table.end > byte_data.start {
        return Err(DumpError::ImageFormatError(
            "symbol table section tables overlap".into(),
        ));
    }

    let name_entries: Vec<_> = section[name_table.clone()]
        .chunks_exact(NAME_ENTRY_SIZE)
        .map(|chunk| *bytemuck::from_bytes::<SymbolNameEntry>(chunk))
        .collect();
    let symbol_entries: Vec<_> = section[symbol_table]
        .chunks_exact(SYMBOL_ENTRY_SIZE)
        .map(|chunk| *bytemuck::from_bytes::<SymbolEntry>(chunk))
        .collect();

    let mut names = Vec::with_capacity(name_count);
    for (idx, entry) in name_entries.iter().copied().enumerate() {
        let byte_len = usize::try_from(entry.byte_len).map_err(|_| {
            DumpError::ImageFormatError(format!("symbol name {idx} byte length overflows usize"))
        })?;
        let stored_len = byte_len.checked_add(1).ok_or_else(|| {
            DumpError::ImageFormatError(format!("symbol name {idx} byte length overflows usize"))
        })?;
        let name_range = checked_range(
            entry.byte_offset,
            stored_len,
            byte_data.len(),
            "symbol name bytes",
        )?;
        let size = usize::try_from(entry.size).map_err(|_| {
            DumpError::ImageFormatError(format!("symbol name {idx} char length overflows usize"))
        })?;
        validate_name_sizes(idx, entry.byte_len, entry.size, entry.size_byte)?;

        let start = byte_data.start + name_range.start;
        if section[start + byte_len] != 0 {
            return Err(DumpError::ImageFormatError(format!(
                "symbol name {idx} is missing GNU trailing NUL"
            )));
        }
        let ptr = section[start..start + byte_len].as_ptr();
        let name = unsafe { LispString::from_mapped_bytes(ptr, byte_len, size, entry.size_byte) };
        names.push(name);
    }

    let mut symbol_names = Vec::with_capacity(symbol_count);
    let mut canonical = Vec::with_capacity(symbol_count);
    for (idx, entry) in symbol_entries.iter().copied().enumerate() {
        if entry.flags & !SYMBOL_CANONICAL_FLAG != 0 {
            return Err(DumpError::ImageFormatError(format!(
                "symbol slot {idx} has unknown flags 0x{:x}",
                entry.flags
            )));
        }
        if entry.name_id as usize >= name_count {
            return Err(DumpError::ImageFormatError(format!(
                "symbol slot {idx} name id {} out of range for {name_count} names",
                entry.name_id
            )));
        }
        symbol_names.push(entry.name_id);
        canonical.push(entry.flags & SYMBOL_CANONICAL_FLAG != 0);
    }

    f(&names, &symbol_names, &canonical)
}

fn validate_name_sizes(
    idx: usize,
    byte_len: u64,
    size: u64,
    size_byte: i64,
) -> Result<(), DumpError> {
    if size_byte == -1 {
        if size != byte_len {
            return Err(DumpError::ImageFormatError(format!(
                "unibyte symbol name {idx} has size {size} but byte length {byte_len}"
            )));
        }
        return Ok(());
    }
    if size_byte < -1 {
        return Err(DumpError::ImageFormatError(format!(
            "symbol name {idx} has invalid size_byte {size_byte}"
        )));
    }
    let size_byte = u64::try_from(size_byte).map_err(|_| {
        DumpError::ImageFormatError(format!("symbol name {idx} size_byte overflows u64"))
    })?;
    if size_byte != byte_len {
        return Err(DumpError::ImageFormatError(format!(
            "multibyte symbol name {idx} has size_byte {size_byte} but byte length {byte_len}"
        )));
    }
    if size > byte_len {
        return Err(DumpError::ImageFormatError(format!(
            "multibyte symbol name {idx} has char length {size} greater than byte length {byte_len}"
        )));
    }
    Ok(())
}

fn read_pod<T: Pod>(section: &[u8], offset: usize, label: &str) -> Result<T, DumpError> {
    let range = checked_range(
        offset as u64,
        std::mem::size_of::<T>(),
        section.len(),
        label,
    )?;
    Ok(*bytemuck::from_bytes::<T>(&section[range]))
}

fn checked_range(
    offset: u64,
    len: usize,
    section_len: usize,
    label: &str,
) -> Result<std::ops::Range<usize>, DumpError> {
    let start = usize::try_from(offset).map_err(|_| {
        DumpError::ImageFormatError(format!("{label} offset {offset} overflows usize"))
    })?;
    let end = checked_add(start, len)?;
    if end > section_len {
        return Err(DumpError::ImageFormatError(format!(
            "{label} range {start}..{end} exceeds symbol table section length {section_len}"
        )));
    }
    Ok(start..end)
}

fn checked_add(left: usize, right: usize) -> Result<usize, DumpError> {
    left.checked_add(right)
        .ok_or_else(|| DumpError::ImageFormatError("symbol table section offset overflow".into()))
}

fn checked_mul(left: usize, right: usize) -> Result<usize, DumpError> {
    left.checked_mul(right)
        .ok_or_else(|| DumpError::ImageFormatError("symbol table section length overflow".into()))
}

fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}

#[cfg(test)]
mod tests {
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
}
