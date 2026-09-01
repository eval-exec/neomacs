//! Fixed-layout pdump section for buffer manager state.
//!
//! GNU Emacs dumps buffer pseudovectors and their text into the pdump image,
//! with runtime caches cleared and marker/window state normalized.  This
//! section moves Neomacs' buffer manager mirror out of RuntimeState bincode
//! while preserving the current logical dump shape exactly.

use bytemuck::{Pod, Zeroable};

use super::DumpError;
use super::object_value_codec::{Cursor, write_bool, write_u8, write_u32, write_u64, write_value};
use super::types::{
    DumpBuffer, DumpBufferId, DumpBufferManager, DumpBufferText, DumpBufferTextBackendKind,
    DumpLispString, DumpMarker, DumpOverlay, DumpOverlayList, DumpPropertyInterval,
    DumpRuntimeBindingValue, DumpSymId, DumpTextPropertyTable, DumpUndoList, DumpUndoRecord,
    DumpValue,
};

const BUFFER_MAGIC: [u8; 16] = *b"NEOBUFFER\0\0\0\0\0\0\0";
const BUFFER_FORMAT_VERSION: u32 = 7;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct BufferHeader {
    magic: [u8; 16],
    version: u32,
    header_size: u32,
    buffer_count: u64,
    default_count: u64,
    next_id: u64,
    next_marker_id: u64,
    payload_offset: u64,
    payload_len: u64,
}

const HEADER_SIZE: usize = std::mem::size_of::<BufferHeader>();

pub(crate) fn buffer_manager_section_bytes(
    manager: &DumpBufferManager,
) -> Result<Vec<u8>, DumpError> {
    let mut bytes = vec![0; HEADER_SIZE];

    write_opt_buffer_id(&mut bytes, manager.current);
    write_len(&mut bytes, manager.buffer_order.len(), "buffer order count")?;
    for id in &manager.buffer_order {
        write_buffer_id(&mut bytes, *id);
    }
    for value in &manager.buffer_defaults {
        write_value(&mut bytes, value)?;
    }
    write_u8(&mut bytes, manager.default_text_backend_kind.into());
    for (id, buffer) in &manager.buffers {
        write_buffer_id(&mut bytes, *id);
        write_buffer(&mut bytes, buffer)?;
    }

    let payload_len = bytes.len() - HEADER_SIZE;
    let header = BufferHeader {
        magic: BUFFER_MAGIC,
        version: BUFFER_FORMAT_VERSION,
        header_size: HEADER_SIZE as u32,
        buffer_count: count_u64(manager.buffers.len(), "buffer count")?,
        default_count: count_u64(manager.buffer_defaults.len(), "buffer default count")?,
        next_id: manager.next_id,
        next_marker_id: manager.next_marker_id,
        payload_offset: HEADER_SIZE as u64,
        payload_len: payload_len as u64,
    };
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    Ok(bytes)
}

pub(crate) fn load_buffer_manager_section(section: &[u8]) -> Result<DumpBufferManager, DumpError> {
    let header = read_header(section)?;
    let payload_offset = usize::try_from(header.payload_offset)
        .map_err(|_| DumpError::ImageFormatError("buffer payload offset overflows usize".into()))?;
    let payload_len = usize::try_from(header.payload_len)
        .map_err(|_| DumpError::ImageFormatError("buffer payload length overflows usize".into()))?;
    let end = payload_offset
        .checked_add(payload_len)
        .ok_or_else(|| DumpError::ImageFormatError("buffer payload range overflows".into()))?;
    if payload_offset < HEADER_SIZE || end > section.len() {
        return Err(DumpError::ImageFormatError(
            "buffer payload range is outside section".into(),
        ));
    }

    let mut cursor = Cursor::new(&section[payload_offset..end]);
    let current = read_opt_buffer_id(&mut cursor)?;
    let order_count = read_len(&mut cursor, "buffer order count")?;
    let mut buffer_order = Vec::with_capacity(order_count);
    for _ in 0..order_count {
        buffer_order.push(read_buffer_id(&mut cursor)?);
    }
    let mut buffer_defaults =
        Vec::with_capacity(to_usize(header.default_count, "buffer default count")?);
    for _ in 0..header.default_count {
        buffer_defaults.push(cursor.read_value()?);
    }
    let default_text_backend_kind = read_buffer_text_backend_kind(&mut cursor)?;
    let mut buffers = Vec::with_capacity(to_usize(header.buffer_count, "buffer count")?);
    for _ in 0..header.buffer_count {
        buffers.push((read_buffer_id(&mut cursor)?, read_buffer(&mut cursor)?));
    }

    if !cursor.is_empty() {
        return Err(DumpError::ImageFormatError(format!(
            "buffer section has {} trailing payload bytes",
            cursor.remaining()
        )));
    }

    Ok(DumpBufferManager {
        buffers,
        buffer_order,
        current,
        next_id: header.next_id,
        next_marker_id: header.next_marker_id,
        buffer_defaults,
        default_text_backend_kind,
    })
}

fn read_header(section: &[u8]) -> Result<BufferHeader, DumpError> {
    if section.len() < HEADER_SIZE {
        return Err(DumpError::ImageFormatError(format!(
            "buffer section shorter than header: {} < {HEADER_SIZE}",
            section.len()
        )));
    }
    let header = *bytemuck::from_bytes::<BufferHeader>(&section[..HEADER_SIZE]);
    if header.magic != BUFFER_MAGIC {
        return Err(DumpError::ImageFormatError(
            "buffer section has bad magic".into(),
        ));
    }
    if header.version != BUFFER_FORMAT_VERSION {
        return Err(DumpError::UnsupportedVersion(header.version));
    }
    if header.header_size != HEADER_SIZE as u32 {
        return Err(DumpError::ImageFormatError(format!(
            "buffer header size {} does not match runtime header size {HEADER_SIZE}",
            header.header_size
        )));
    }
    Ok(header)
}

fn write_buffer(out: &mut Vec<u8>, buffer: &DumpBuffer) -> Result<(), DumpError> {
    write_buffer_id(out, buffer.id);
    write_opt_lisp_string(out, buffer.name_lisp.as_ref())?;
    write_opt_string(out, buffer.name.as_deref())?;
    write_opt_lisp_string(out, buffer.last_name_lisp.as_ref())?;
    write_opt_string(out, buffer.last_name.as_deref())?;
    write_opt_buffer_id(out, buffer.base_buffer);
    write_buffer_text(out, &buffer.text)?;
    write_usize(out, buffer.pt)?;
    write_opt_usize(out, buffer.pt_char)?;
    write_opt_usize(out, buffer.mark)?;
    write_opt_usize(out, buffer.mark_char)?;
    write_usize(out, buffer.begv)?;
    write_opt_usize(out, buffer.begv_char)?;
    write_usize(out, buffer.zv)?;
    write_opt_usize(out, buffer.zv_char)?;
    write_bool(out, buffer.modified);
    write_i64(out, buffer.modified_tick);
    write_i64(out, buffer.chars_modified_tick);
    write_opt_i64(out, buffer.save_modified_tick);
    write_opt_i64(out, buffer.autosave_modified_tick);
    write_opt_usize(out, buffer.last_window_start)?;
    write_bool(out, buffer.read_only);
    write_bool(out, buffer.multibyte);
    write_opt_lisp_string(out, buffer.file_name_lisp.as_ref())?;
    write_opt_string(out, buffer.file_name.as_deref())?;
    write_opt_lisp_string(out, buffer.auto_save_file_name_lisp.as_ref())?;
    write_opt_string(out, buffer.auto_save_file_name.as_deref())?;
    write_markers(out, &buffer.markers)?;
    write_opt_u64(out, buffer.state_pt_marker);
    write_opt_u64(out, buffer.state_begv_marker);
    write_opt_u64(out, buffer.state_zv_marker);
    write_symbol_runtime_pairs(out, &buffer.properties_syms)?;
    write_string_runtime_pairs(out, &buffer.properties)?;
    write_sym_vec(out, &buffer.local_binding_syms)?;
    write_string_vec(out, &buffer.local_binding_names)?;
    write_value(out, &buffer.local_map)?;
    write_text_property_table(out, &buffer.text_props)?;
    write_overlay_list(out, &buffer.overlays)?;
    write_opt_undo_list(out, buffer.undo_list.as_ref())?;
    write_values(out, &buffer.slots)?;
    write_u64(out, buffer.local_flags);
    write_value(out, &buffer.local_var_alist)?;
    write_opt_i64(out, buffer.modtime_sec);
    write_opt_i32(out, buffer.modtime_nsec);
    write_opt_i64(out, buffer.modtime_size);
    Ok(())
}

fn read_buffer(cursor: &mut Cursor<'_>) -> Result<DumpBuffer, DumpError> {
    Ok(DumpBuffer {
        id: read_buffer_id(cursor)?,
        name_lisp: read_opt_lisp_string(cursor)?,
        name: read_opt_string(cursor)?,
        last_name_lisp: read_opt_lisp_string(cursor)?,
        last_name: read_opt_string(cursor)?,
        base_buffer: read_opt_buffer_id(cursor)?,
        text: read_buffer_text(cursor)?,
        pt: read_usize(cursor, "buffer point")?,
        pt_char: read_opt_usize(cursor, "buffer point char")?,
        mark: read_opt_usize(cursor, "buffer mark")?,
        mark_char: read_opt_usize(cursor, "buffer mark char")?,
        begv: read_usize(cursor, "buffer begv")?,
        begv_char: read_opt_usize(cursor, "buffer begv char")?,
        zv: read_usize(cursor, "buffer zv")?,
        zv_char: read_opt_usize(cursor, "buffer zv char")?,
        modified: cursor.read_bool("buffer modified")?,
        modified_tick: read_i64(cursor, "buffer modified tick")?,
        chars_modified_tick: read_i64(cursor, "buffer chars modified tick")?,
        save_modified_tick: read_opt_i64(cursor, "buffer save modified tick")?,
        autosave_modified_tick: read_opt_i64(cursor, "buffer autosave modified tick")?,
        last_window_start: read_opt_usize(cursor, "buffer last window start")?,
        read_only: cursor.read_bool("buffer read-only")?,
        multibyte: cursor.read_bool("buffer multibyte")?,
        file_name_lisp: read_opt_lisp_string(cursor)?,
        file_name: read_opt_string(cursor)?,
        auto_save_file_name_lisp: read_opt_lisp_string(cursor)?,
        auto_save_file_name: read_opt_string(cursor)?,
        markers: read_markers(cursor)?,
        state_pt_marker: read_opt_u64(cursor)?,
        state_begv_marker: read_opt_u64(cursor)?,
        state_zv_marker: read_opt_u64(cursor)?,
        properties_syms: read_symbol_runtime_pairs(cursor)?,
        properties: read_string_runtime_pairs(cursor)?,
        local_binding_syms: read_sym_vec(cursor)?,
        local_binding_names: read_string_vec(cursor)?,
        local_map: cursor.read_value()?,
        text_props: read_text_property_table(cursor)?,
        overlays: read_overlay_list(cursor)?,
        undo_list: read_opt_undo_list(cursor)?,
        slots: read_values(cursor)?,
        local_flags: cursor.read_u64("buffer local flags")?,
        local_var_alist: cursor.read_value()?,
        modtime_sec: read_opt_i64(cursor, "buffer modtime sec")?,
        modtime_nsec: read_opt_i32(cursor, "buffer modtime nsec")?,
        modtime_size: read_opt_i64(cursor, "buffer modtime size")?,
    })
}

fn write_buffer_text(out: &mut Vec<u8>, buffer: &DumpBufferText) -> Result<(), DumpError> {
    write_u8(out, buffer.backend_kind.into());
    write_bytes(out, &buffer.text)
}

fn read_buffer_text(cursor: &mut Cursor<'_>) -> Result<DumpBufferText, DumpError> {
    let backend_kind = read_buffer_text_backend_kind(cursor)?;
    Ok(DumpBufferText {
        backend_kind,
        text: read_bytes(cursor)?,
    })
}

fn read_buffer_text_backend_kind(
    cursor: &mut Cursor<'_>,
) -> Result<DumpBufferTextBackendKind, DumpError> {
    let tag = cursor.read_u8("buffer text backend kind")?;
    DumpBufferTextBackendKind::try_from(tag).map_err(|_| {
        DumpError::ImageFormatError(format!("invalid buffer text backend kind tag: {tag}"))
    })
}

fn write_text_property_table(
    out: &mut Vec<u8>,
    table: &DumpTextPropertyTable,
) -> Result<(), DumpError> {
    write_len(out, table.intervals.len(), "text property interval count")?;
    for interval in &table.intervals {
        write_usize(out, interval.start)?;
        write_usize(out, interval.end)?;
        write_len(out, interval.properties.len(), "text property pair count")?;
        for (key, value) in &interval.properties {
            write_value(out, key)?;
            write_value(out, value)?;
        }
    }
    Ok(())
}

fn read_text_property_table(cursor: &mut Cursor<'_>) -> Result<DumpTextPropertyTable, DumpError> {
    let len = read_len(cursor, "text property interval count")?;
    let mut intervals = Vec::with_capacity(len);
    for _ in 0..len {
        let start = read_usize(cursor, "text property interval start")?;
        let end = read_usize(cursor, "text property interval end")?;
        let pair_count = read_len(cursor, "text property pair count")?;
        let mut properties = Vec::with_capacity(pair_count);
        for _ in 0..pair_count {
            properties.push((cursor.read_value()?, cursor.read_value()?));
        }
        intervals.push(DumpPropertyInterval {
            start,
            end,
            properties,
        });
    }
    Ok(DumpTextPropertyTable { intervals })
}

fn write_marker(out: &mut Vec<u8>, marker: &DumpMarker) -> Result<(), DumpError> {
    write_opt_buffer_id(out, marker.buffer);
    write_bool(out, marker.insertion_type);
    write_opt_u64(out, marker.marker_id);
    write_usize(out, marker.bytepos)?;
    write_usize(out, marker.charpos)?;
    write_bool(out, marker.last_position_valid);
    Ok(())
}

fn read_marker(cursor: &mut Cursor<'_>) -> Result<DumpMarker, DumpError> {
    Ok(DumpMarker {
        buffer: read_opt_buffer_id(cursor)?,
        insertion_type: cursor.read_bool("marker insertion type")?,
        marker_id: read_opt_u64(cursor)?,
        bytepos: read_usize(cursor, "marker byte position")?,
        charpos: read_usize(cursor, "marker char position")?,
        last_position_valid: cursor.read_bool("marker last_position_valid")?,
    })
}

fn write_markers(out: &mut Vec<u8>, markers: &[DumpMarker]) -> Result<(), DumpError> {
    write_len(out, markers.len(), "marker count")?;
    for marker in markers {
        write_marker(out, marker)?;
    }
    Ok(())
}

fn read_markers(cursor: &mut Cursor<'_>) -> Result<Vec<DumpMarker>, DumpError> {
    let len = read_len(cursor, "marker count")?;
    let mut markers = Vec::with_capacity(len);
    for _ in 0..len {
        markers.push(read_marker(cursor)?);
    }
    Ok(markers)
}

fn write_overlay(out: &mut Vec<u8>, overlay: &DumpOverlay) -> Result<(), DumpError> {
    write_u64(out, overlay.serial);
    write_value(out, &overlay.plist)?;
    write_opt_buffer_id(out, overlay.buffer);
    write_usize(out, overlay.start)?;
    write_usize(out, overlay.end)?;
    write_bool(out, overlay.front_advance);
    write_bool(out, overlay.rear_advance);
    Ok(())
}

fn read_overlay(cursor: &mut Cursor<'_>) -> Result<DumpOverlay, DumpError> {
    Ok(DumpOverlay {
        serial: cursor.read_u64("overlay serial")?,
        plist: cursor.read_value()?,
        buffer: read_opt_buffer_id(cursor)?,
        start: read_usize(cursor, "overlay start")?,
        end: read_usize(cursor, "overlay end")?,
        front_advance: cursor.read_bool("overlay front advance")?,
        rear_advance: cursor.read_bool("overlay rear advance")?,
    })
}

fn write_overlay_list(out: &mut Vec<u8>, overlays: &DumpOverlayList) -> Result<(), DumpError> {
    write_len(out, overlays.overlays.len(), "overlay count")?;
    for overlay in &overlays.overlays {
        write_overlay(out, overlay)?;
    }
    Ok(())
}

fn read_overlay_list(cursor: &mut Cursor<'_>) -> Result<DumpOverlayList, DumpError> {
    let len = read_len(cursor, "overlay count")?;
    let mut overlays = Vec::with_capacity(len);
    for _ in 0..len {
        overlays.push(read_overlay(cursor)?);
    }
    Ok(DumpOverlayList { overlays })
}

const RUNTIME_BOUND: u8 = 0;
const RUNTIME_VOID: u8 = 1;

fn write_runtime_binding_value(
    out: &mut Vec<u8>,
    value: &DumpRuntimeBindingValue,
) -> Result<(), DumpError> {
    match value {
        DumpRuntimeBindingValue::Bound(value) => {
            write_u8(out, RUNTIME_BOUND);
            write_value(out, value)?;
        }
        DumpRuntimeBindingValue::Void => write_u8(out, RUNTIME_VOID),
    }
    Ok(())
}

fn read_runtime_binding_value(
    cursor: &mut Cursor<'_>,
) -> Result<DumpRuntimeBindingValue, DumpError> {
    match cursor.read_u8("runtime binding tag")? {
        RUNTIME_BOUND => Ok(DumpRuntimeBindingValue::Bound(cursor.read_value()?)),
        RUNTIME_VOID => Ok(DumpRuntimeBindingValue::Void),
        other => Err(DumpError::ImageFormatError(format!(
            "unknown runtime binding tag {other}"
        ))),
    }
}

fn write_symbol_runtime_pairs(
    out: &mut Vec<u8>,
    pairs: &[(DumpSymId, DumpRuntimeBindingValue)],
) -> Result<(), DumpError> {
    write_len(out, pairs.len(), "symbol/runtime pair count")?;
    for (sym, value) in pairs {
        write_u32(out, sym.0);
        write_runtime_binding_value(out, value)?;
    }
    Ok(())
}

fn read_symbol_runtime_pairs(
    cursor: &mut Cursor<'_>,
) -> Result<Vec<(DumpSymId, DumpRuntimeBindingValue)>, DumpError> {
    let len = read_len(cursor, "symbol/runtime pair count")?;
    let mut pairs = Vec::with_capacity(len);
    for _ in 0..len {
        pairs.push((
            DumpSymId(cursor.read_u32("symbol/runtime symbol")?),
            read_runtime_binding_value(cursor)?,
        ));
    }
    Ok(pairs)
}

fn write_string_runtime_pairs(
    out: &mut Vec<u8>,
    pairs: &[(String, DumpRuntimeBindingValue)],
) -> Result<(), DumpError> {
    write_len(out, pairs.len(), "string/runtime pair count")?;
    for (name, value) in pairs {
        write_string(out, name)?;
        write_runtime_binding_value(out, value)?;
    }
    Ok(())
}

fn read_string_runtime_pairs(
    cursor: &mut Cursor<'_>,
) -> Result<Vec<(String, DumpRuntimeBindingValue)>, DumpError> {
    let len = read_len(cursor, "string/runtime pair count")?;
    let mut pairs = Vec::with_capacity(len);
    for _ in 0..len {
        pairs.push((read_string(cursor)?, read_runtime_binding_value(cursor)?));
    }
    Ok(pairs)
}

const UNDO_INSERT: u8 = 0;
const UNDO_DELETE: u8 = 1;
const UNDO_PROPERTY_CHANGE: u8 = 2;
const UNDO_CURSOR_MOVE: u8 = 3;
const UNDO_FIRST_CHANGE: u8 = 4;
const UNDO_BOUNDARY: u8 = 5;

fn write_undo_record(out: &mut Vec<u8>, record: &DumpUndoRecord) -> Result<(), DumpError> {
    match record {
        DumpUndoRecord::Insert { pos, len } => {
            write_u8(out, UNDO_INSERT);
            write_usize(out, *pos)?;
            write_usize(out, *len)?;
        }
        DumpUndoRecord::Delete { pos, text } => {
            write_u8(out, UNDO_DELETE);
            write_usize(out, *pos)?;
            write_string(out, text)?;
        }
        DumpUndoRecord::PropertyChange {
            pos,
            len,
            old_props,
        } => {
            write_u8(out, UNDO_PROPERTY_CHANGE);
            write_usize(out, *pos)?;
            write_usize(out, *len)?;
            write_string_value_pairs(out, old_props)?;
        }
        DumpUndoRecord::CursorMove { pos } => {
            write_u8(out, UNDO_CURSOR_MOVE);
            write_usize(out, *pos)?;
        }
        DumpUndoRecord::FirstChange {
            visited_file_modtime,
        } => {
            write_u8(out, UNDO_FIRST_CHANGE);
            write_i64(out, *visited_file_modtime);
        }
        DumpUndoRecord::Boundary => write_u8(out, UNDO_BOUNDARY),
    }
    Ok(())
}

fn read_undo_record(cursor: &mut Cursor<'_>) -> Result<DumpUndoRecord, DumpError> {
    match cursor.read_u8("undo record tag")? {
        UNDO_INSERT => Ok(DumpUndoRecord::Insert {
            pos: read_usize(cursor, "undo insert position")?,
            len: read_usize(cursor, "undo insert length")?,
        }),
        UNDO_DELETE => Ok(DumpUndoRecord::Delete {
            pos: read_usize(cursor, "undo delete position")?,
            text: read_string(cursor)?,
        }),
        UNDO_PROPERTY_CHANGE => Ok(DumpUndoRecord::PropertyChange {
            pos: read_usize(cursor, "undo property position")?,
            len: read_usize(cursor, "undo property length")?,
            old_props: read_string_value_pairs(cursor)?,
        }),
        UNDO_CURSOR_MOVE => Ok(DumpUndoRecord::CursorMove {
            pos: read_usize(cursor, "undo cursor position")?,
        }),
        UNDO_FIRST_CHANGE => Ok(DumpUndoRecord::FirstChange {
            visited_file_modtime: read_i64(cursor, "undo first-change modtime")?,
        }),
        UNDO_BOUNDARY => Ok(DumpUndoRecord::Boundary),
        other => Err(DumpError::ImageFormatError(format!(
            "unknown undo record tag {other}"
        ))),
    }
}

fn write_undo_list(out: &mut Vec<u8>, undo: &DumpUndoList) -> Result<(), DumpError> {
    write_len(out, undo.records.len(), "undo record count")?;
    for record in &undo.records {
        write_undo_record(out, record)?;
    }
    write_usize(out, undo.limit)?;
    write_bool(out, undo.enabled);
    Ok(())
}

fn read_undo_list(cursor: &mut Cursor<'_>) -> Result<DumpUndoList, DumpError> {
    let len = read_len(cursor, "undo record count")?;
    let mut records = Vec::with_capacity(len);
    for _ in 0..len {
        records.push(read_undo_record(cursor)?);
    }
    Ok(DumpUndoList {
        records,
        limit: read_usize(cursor, "undo limit")?,
        enabled: cursor.read_bool("undo enabled")?,
    })
}

fn write_opt_undo_list(out: &mut Vec<u8>, undo: Option<&DumpUndoList>) -> Result<(), DumpError> {
    match undo {
        Some(undo) => {
            write_bool(out, true);
            write_undo_list(out, undo)?;
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn read_opt_undo_list(cursor: &mut Cursor<'_>) -> Result<Option<DumpUndoList>, DumpError> {
    if cursor.read_bool("optional undo list present")? {
        Ok(Some(read_undo_list(cursor)?))
    } else {
        Ok(None)
    }
}

fn write_string_value_pairs(
    out: &mut Vec<u8>,
    pairs: &[(String, DumpValue)],
) -> Result<(), DumpError> {
    write_len(out, pairs.len(), "string/value pair count")?;
    for (name, value) in pairs {
        write_string(out, name)?;
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

fn write_values(out: &mut Vec<u8>, values: &[DumpValue]) -> Result<(), DumpError> {
    write_len(out, values.len(), "value count")?;
    for value in values {
        write_value(out, value)?;
    }
    Ok(())
}

fn read_values(cursor: &mut Cursor<'_>) -> Result<Vec<DumpValue>, DumpError> {
    let len = read_len(cursor, "value count")?;
    let mut values = Vec::with_capacity(len);
    for _ in 0..len {
        values.push(cursor.read_value()?);
    }
    Ok(values)
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
    if cursor.read_bool("optional lisp string present")? {
        Ok(Some(read_lisp_string(cursor)?))
    } else {
        Ok(None)
    }
}

fn write_buffer_id(out: &mut Vec<u8>, id: DumpBufferId) {
    write_u64(out, id.0);
}

fn read_buffer_id(cursor: &mut Cursor<'_>) -> Result<DumpBufferId, DumpError> {
    Ok(DumpBufferId(cursor.read_u64("buffer id")?))
}

fn write_opt_buffer_id(out: &mut Vec<u8>, id: Option<DumpBufferId>) {
    match id {
        Some(id) => {
            write_bool(out, true);
            write_buffer_id(out, id);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_buffer_id(cursor: &mut Cursor<'_>) -> Result<Option<DumpBufferId>, DumpError> {
    if cursor.read_bool("optional buffer id present")? {
        Ok(Some(read_buffer_id(cursor)?))
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

fn write_opt_usize(out: &mut Vec<u8>, value: Option<usize>) -> Result<(), DumpError> {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_usize(out, value)?;
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn read_opt_usize(cursor: &mut Cursor<'_>, what: &str) -> Result<Option<usize>, DumpError> {
    if cursor.read_bool("optional usize present")? {
        Ok(Some(read_usize(cursor, what)?))
    } else {
        Ok(None)
    }
}

fn write_opt_u64(out: &mut Vec<u8>, value: Option<u64>) {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_u64(out, value);
        }
        None => write_bool(out, false),
    }
}

fn read_opt_u64(cursor: &mut Cursor<'_>) -> Result<Option<u64>, DumpError> {
    if cursor.read_bool("optional u64 present")? {
        Ok(Some(cursor.read_u64("optional u64")?))
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

fn write_opt_i32(out: &mut Vec<u8>, value: Option<i32>) {
    match value {
        Some(value) => {
            write_bool(out, true);
            out.extend_from_slice(&value.to_le_bytes());
        }
        None => write_bool(out, false),
    }
}

fn read_opt_i32(cursor: &mut Cursor<'_>, what: &str) -> Result<Option<i32>, DumpError> {
    if cursor.read_bool(what)? {
        let bytes_slice = cursor.read_exact(4, what)?;
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(bytes_slice);
        Ok(Some(i32::from_le_bytes(bytes)))
    } else {
        Ok(None)
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), DumpError> {
    write_bytes(out, value.as_bytes())
}

fn read_string(cursor: &mut Cursor<'_>) -> Result<String, DumpError> {
    String::from_utf8(read_bytes(cursor)?)
        .map_err(|err| DumpError::ImageFormatError(format!("invalid UTF-8 string: {err}")))
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
        bytes.push(cursor.read_u8("byte payload byte")?);
    }
    Ok(bytes)
}

fn write_len(out: &mut Vec<u8>, len: usize, what: &str) -> Result<(), DumpError> {
    write_u64(out, count_u64(len, what)?);
    Ok(())
}

fn read_len(cursor: &mut Cursor<'_>, what: &str) -> Result<usize, DumpError> {
    to_usize(cursor.read_u64(what)?, what)
}

fn write_usize(out: &mut Vec<u8>, value: usize) -> Result<(), DumpError> {
    write_u64(
        out,
        u64::try_from(value)
            .map_err(|_| DumpError::SerializationError("usize overflows u64".into()))?,
    );
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

pub(crate) fn empty_buffer_manager() -> DumpBufferManager {
    DumpBufferManager {
        buffers: Vec::new(),
        buffer_order: Vec::new(),
        current: None,
        next_id: 0,
        next_marker_id: 0,
        buffer_defaults: Vec::new(),
        default_text_backend_kind: DumpBufferTextBackendKind::GapBuffer,
    }
}

#[cfg(test)]
mod tests {
    use crate::buffer::BufferTextBackendKind;

    use super::super::types::DumpHeapRef;
    use super::*;

    #[test]
    fn buffer_text_backend_kind_tags_match_runtime_backend_kind() {
        for kind in BufferTextBackendKind::implemented_variants() {
            let dump_kind = DumpBufferTextBackendKind::from(kind);
            assert_eq!(BufferTextBackendKind::from(dump_kind), kind);
            assert_eq!(u8::from(dump_kind), u8::from(kind));
        }
    }

    #[test]
    fn buffer_section_round_trips_manager_state() {
        let buffer = DumpBuffer {
            id: DumpBufferId(1),
            name_lisp: Some(DumpLispString {
                data: b"*scratch*".to_vec(),
                size: 9,
                size_byte: 9,
            }),
            name: Some("*scratch*".into()),
            last_name_lisp: None,
            last_name: Some("*old-scratch*".into()),
            base_buffer: Some(DumpBufferId(2)),
            text: DumpBufferText {
                backend_kind: DumpBufferTextBackendKind::GapBuffer,
                text: b"hello buffer".to_vec(),
            },
            pt: 1,
            pt_char: Some(1),
            mark: Some(2),
            mark_char: Some(2),
            begv: 0,
            begv_char: Some(0),
            zv: 12,
            zv_char: Some(12),
            modified: true,
            modified_tick: 3,
            chars_modified_tick: 4,
            save_modified_tick: Some(5),
            autosave_modified_tick: Some(6),
            modtime_sec: Some(1234567890),
            modtime_nsec: Some(500000000),
            modtime_size: Some(1024),
            last_window_start: Some(7),
            read_only: false,
            multibyte: true,
            file_name_lisp: None,
            file_name: Some("/tmp/file".into()),
            auto_save_file_name_lisp: None,
            auto_save_file_name: Some("#file#".into()),
            markers: vec![DumpMarker {
                buffer: Some(DumpBufferId(1)),
                insertion_type: true,
                marker_id: Some(8),
                bytepos: 9,
                charpos: 10,
                last_position_valid: true,
            }],
            state_pt_marker: Some(11),
            state_begv_marker: Some(12),
            state_zv_marker: Some(13),
            properties_syms: vec![(
                DumpSymId(14),
                DumpRuntimeBindingValue::Bound(DumpValue::Int(15)),
            )],
            properties: vec![("prop".into(), DumpRuntimeBindingValue::Void)],
            local_binding_syms: vec![DumpSymId(16)],
            local_binding_names: vec!["legacy-local".into()],
            local_map: DumpValue::Symbol(DumpSymId(17)),
            text_props: DumpTextPropertyTable {
                intervals: vec![DumpPropertyInterval {
                    start: 0,
                    end: 5,
                    properties: vec![(DumpValue::Symbol(DumpSymId(18)), DumpValue::True)],
                }],
            },
            overlays: DumpOverlayList {
                overlays: vec![DumpOverlay {
                    serial: 27,
                    plist: DumpValue::Nil,
                    buffer: Some(DumpBufferId(1)),
                    start: 0,
                    end: 1,
                    front_advance: true,
                    rear_advance: false,
                }],
            },
            undo_list: Some(DumpUndoList {
                records: vec![
                    DumpUndoRecord::Insert { pos: 1, len: 2 },
                    DumpUndoRecord::Delete {
                        pos: 3,
                        text: "gone".into(),
                    },
                    DumpUndoRecord::PropertyChange {
                        pos: 4,
                        len: 5,
                        old_props: vec![("face".into(), DumpValue::Symbol(DumpSymId(19)))],
                    },
                    DumpUndoRecord::CursorMove { pos: 6 },
                    DumpUndoRecord::FirstChange {
                        visited_file_modtime: 7,
                    },
                    DumpUndoRecord::Boundary,
                ],
                limit: 20,
                enabled: true,
            }),
            slots: vec![DumpValue::Vector(DumpHeapRef { index: 21 })],
            local_flags: 22,
            local_var_alist: DumpValue::Cons(DumpHeapRef { index: 23 }),
        };
        let manager = DumpBufferManager {
            buffers: vec![(DumpBufferId(1), buffer)],
            buffer_order: vec![DumpBufferId(1)],
            current: Some(DumpBufferId(1)),
            next_id: 24,
            next_marker_id: 25,
            buffer_defaults: vec![DumpValue::Int(26)],
            default_text_backend_kind: DumpBufferTextBackendKind::PieceTree,
        };

        let bytes = buffer_manager_section_bytes(&manager).expect("encode buffers");
        let decoded = load_buffer_manager_section(&bytes).expect("decode buffers");

        assert_eq!(format!("{decoded:?}"), format!("{manager:?}"));
    }

    #[test]
    fn buffer_section_rejects_bad_magic() {
        let mut bytes =
            buffer_manager_section_bytes(&empty_buffer_manager()).expect("encode buffers");
        bytes[0] ^= 1;
        let err = load_buffer_manager_section(&bytes).expect_err("bad magic should fail");
        assert!(matches!(err, DumpError::ImageFormatError(_)));
    }
}
