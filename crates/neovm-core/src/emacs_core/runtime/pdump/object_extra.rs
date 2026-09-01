//! Compact ObjectExtra section: sparse extra data for objects not fully mapped.
//!
//! Category A objects (cons, float, vector, lambda, macro, record) are fully
//! in HeapImage/ObjectStarts after relocation and need no extra data.
//!
//! Category B objects (string, overlay, marker) have mapped HeapImage spans
//! but need a small descriptor for fields that can't be raw bytes.
//!
//! Category C objects (hash-table, obarray, bytecode, subr, buffer, window,
//! frame, timer, free) have no HeapImage representation and need a full descriptor.
//!
//! Serialization strategy: each sparse record starts with the object index, then
//! the extra tag byte identifies the variant. Complex payloads use the same
//! encoding as `object_value_codec::write_heap_object`; on read, we delegate to
//! `Cursor::read_heap_object` and extract the relevant fields from the returned
//! `DumpHeapObject`.

use bytemuck::{Pod, Zeroable};
use std::num::NonZeroU32;

use super::mapped_heap::MappedHeapView;
use super::object_starts::{LoadedObjectSpan, LoadedSpans};
use super::object_value_codec;
use super::{DumpError, types::*};
use crate::tagged::header::VecLikeType;

const OBJECT_EXTRA_MAGIC: [u8; 16] = *b"NEOOBJEXTRA\0\0\0\0\0";
const OBJECT_EXTRA_FORMAT_VERSION: u32 = 7;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ObjectExtraHeader {
    magic: [u8; 16],
    version: u32,
    header_size: u32,
    object_count: u64,
    payload_offset: u64,
    payload_len: u64,
}

const HEADER_SIZE: usize = std::mem::size_of::<ObjectExtraHeader>();

// Variant tags — kept distinct from HEAP_* tags in object_value_codec.rs.
const EXTRA_STRING: u8 = 101;
const EXTRA_HASH_TABLE: u8 = 102;
const EXTRA_BYTE_CODE: u8 = 103;
const EXTRA_SUBR: u8 = 104;
const EXTRA_BUFFER: u8 = 105;
const EXTRA_WINDOW: u8 = 106;
const EXTRA_FRAME: u8 = 107;
const EXTRA_TIMER: u8 = 108;
const EXTRA_OVERLAY: u8 = 109;
const EXTRA_MARKER: u8 = 110;
const EXTRA_FREE: u8 = 111;
const EXTRA_CHAR_TABLE: u8 = 112;
const EXTRA_SUB_CHAR_TABLE: u8 = 113;
const EXTRA_OBARRAY: u8 = 114;

/// Per-object extra data needed during load.
#[derive(Debug, Clone)]
pub(crate) enum ObjectExtra {
    /// Category B: string needs size, size_byte, byte data span, and text_props.
    String {
        size: usize,
        size_byte: i64,
        byte_data: DumpByteData,
        text_props: Vec<DumpStringTextPropertyRun>,
    },
    /// Category C: hash table (no HeapImage bytes).
    HashTable(DumpLispHashTable),
    /// Category C: obarray (no HeapImage bytes).
    Obarray { buckets: Vec<DumpValue>, count: u32 },
    /// Category C: bytecode function (no HeapImage bytes).
    ByteCode(DumpByteCodeFunction),
    /// Category C: char-table (no HeapImage bytes).
    CharTable {
        defalt: DumpValue,
        parent: DumpValue,
        purpose: DumpValue,
        ascii: DumpValue,
        contents: Vec<DumpValue>,
        extras: Vec<DumpValue>,
    },
    /// Category C: sub-char-table (no HeapImage bytes).
    SubCharTable {
        depth: i64,
        min_char: i64,
        contents: Vec<DumpValue>,
    },
    /// Category C: subr (no HeapImage bytes).
    Subr {
        name: DumpNameId,
        min_args: u16,
        max_args: Option<u16>,
    },
    /// Category C: buffer ID (no HeapImage bytes).
    Buffer(DumpBufferId),
    /// Category C: window ID (no HeapImage bytes).
    Window(u64),
    /// Category C: frame ID (no HeapImage bytes).
    Frame(u64),
    /// Category C: timer ID (no HeapImage bytes).
    Timer(u64),
    /// Category B: overlay (has veclike span but needs plist).
    Overlay(DumpOverlay),
    /// Category B: marker (has veclike span but needs fields).
    Marker(DumpMarker),
    /// Free slot.
    Free,
}

/// Index into the dense descriptor payload for one object that is not already
/// self-contained in `HeapImage`.
///
/// Encoding the index as one-based makes `Option<ObjectDescriptorId>` exactly
/// one `u32`: zero means that the object needs no descriptor, while every other
/// value points into [`FileObjectDescriptors::descriptors`].
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ObjectDescriptorId(NonZeroU32);

const _: () =
    assert!(std::mem::size_of::<Option<ObjectDescriptorId>>() == std::mem::size_of::<u32>());

impl ObjectDescriptorId {
    fn from_index(index: usize) -> Result<Self, DumpError> {
        let one_based = index
            .checked_add(1)
            .and_then(|value| u32::try_from(value).ok())
            .and_then(NonZeroU32::new)
            .ok_or_else(|| {
                DumpError::ImageFormatError("object descriptor index overflows u32".into())
            })?;
        Ok(Self(one_based))
    }

    fn index(self) -> usize {
        self.0.get() as usize - 1
    }
}

/// Sparse file-pdump descriptors keyed by the stable dump object index.
///
/// Most file-pdump objects are already complete in the mapped heap image.  A
/// one-word lookup slot per dump object keeps that absence explicit without
/// materializing a full-sized `DumpHeapObject::Free` sentinel for every mapped
/// cons, float, vector, closure, record, and property-free string.
pub(crate) struct FileObjectDescriptors {
    /// Sparse object-index -> descriptor map. Only Category-B/C objects
    /// (descriptor-driven strings, unmapped residents, bytecode extras) have
    /// entries — a few thousand of the 166K dump objects — so a dense
    /// `Vec<Option<_>>` was 1.3MB of written pages carrying mostly `None`.
    by_object_index: rustc_hash::FxHashMap<u32, ObjectDescriptorId>,
    object_count: usize,
    descriptors: Vec<DumpHeapObject>,
}

impl FileObjectDescriptors {
    fn new(object_count: usize, encoded_payload_len: usize) -> Self {
        let estimated_descriptor_count =
            encoded_payload_len / std::mem::size_of::<DumpHeapObject>().max(1);
        Self {
            by_object_index: rustc_hash::FxHashMap::default(),
            object_count,
            descriptors: Vec::with_capacity(estimated_descriptor_count),
        }
    }

    fn insert(&mut self, object_index: usize, descriptor: DumpHeapObject) -> Result<(), DumpError> {
        if object_index >= self.object_count {
            return Err(DumpError::ImageFormatError(format!(
                "object-extra index {object_index} is outside object count {}",
                self.object_count
            )));
        }
        let key = u32::try_from(object_index).map_err(|_| {
            DumpError::ImageFormatError(format!(
                "object-extra index {object_index} overflows the descriptor key"
            ))
        })?;
        let descriptor_id = ObjectDescriptorId::from_index(self.descriptors.len())?;
        if self.by_object_index.insert(key, descriptor_id).is_some() {
            return Err(DumpError::ImageFormatError(format!(
                "object-extra has duplicate record for object {object_index}"
            )));
        }
        self.descriptors.push(descriptor);
        Ok(())
    }

    pub(crate) fn len(&self) -> usize {
        self.object_count
    }

    #[cfg(test)]
    pub(crate) fn descriptor_count(&self) -> usize {
        self.descriptors.len()
    }

    pub(crate) fn get(&self, object_index: usize) -> Option<&DumpHeapObject> {
        let descriptor_id = *self
            .by_object_index
            .get(&u32::try_from(object_index).ok()?)?;
        self.descriptors.get(descriptor_id.index())
    }

    pub(crate) fn take(&mut self, object_index: usize) -> Option<DumpHeapObject> {
        let descriptor_id = *self
            .by_object_index
            .get(&u32::try_from(object_index).ok()?)?;
        Some(std::mem::replace(
            &mut self.descriptors[descriptor_id.index()],
            DumpHeapObject::Free,
        ))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &DumpHeapObject> {
        self.descriptors.iter()
    }

    pub(crate) unsafe fn discard_without_drop(&mut self) {
        // SAFETY: callers may use this only after all live payloads have moved
        // into the restored evaluator, leaving discardable sentinel records.
        unsafe {
            self.descriptors.set_len(0);
        }
    }
}

// ---------------------------------------------------------------------------
// Build (dump path)
// ---------------------------------------------------------------------------

/// Build the ObjectExtra section bytes from dump heap objects.
pub(crate) fn build_object_extra(
    objects: &[DumpHeapObject],
    mapped_slots: &[Option<crate::emacs_core::pdump::types::DumpSlotSpan>],
) -> Result<Vec<u8>, DumpError> {
    let mut bytes = vec![0u8; HEADER_SIZE];
    for (index, obj) in objects.iter().enumerate() {
        if !object_needs_extra(obj) {
            continue;
        }
        write_dump_usize(&mut bytes, index, "object-extra object index")?;
        let has_mapped_slots = mapped_slots.get(index).copied().flatten().is_some();
        write_object_extra(&mut bytes, obj, has_mapped_slots)?;
    }
    let payload_len = bytes.len() - HEADER_SIZE;
    let header = ObjectExtraHeader {
        magic: OBJECT_EXTRA_MAGIC,
        version: OBJECT_EXTRA_FORMAT_VERSION,
        header_size: HEADER_SIZE as u32,
        object_count: objects.len() as u64,
        payload_offset: HEADER_SIZE as u64,
        payload_len: payload_len as u64,
    };
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    Ok(bytes)
}

fn write_object_extra(
    out: &mut Vec<u8>,
    obj: &DumpHeapObject,
    has_mapped_slots: bool,
) -> Result<(), DumpError> {
    match obj {
        DumpHeapObject::Cons { .. }
        | DumpHeapObject::Float(_)
        | DumpHeapObject::Vector(_)
        | DumpHeapObject::Lambda(_)
        | DumpHeapObject::Macro(_)
        | DumpHeapObject::Record(_) => {}
        // Category B: partial extra data.
        DumpHeapObject::Str {
            data,
            size,
            size_byte,
            text_props,
        } => {
            object_value_codec::write_u8(out, EXTRA_STRING);
            write_dump_usize(out, *size, "string size")?;
            write_dump_i32(out, *size_byte, "string size_byte")?;
            // Write byte data (Owned or Mapped)
            match data {
                DumpByteData::Owned(bytes) => {
                    object_value_codec::write_u8(out, 0);
                    write_dump_usize(out, bytes.len(), "string owned byte length")?;
                    out.extend_from_slice(bytes);
                }
                DumpByteData::Mapped(span) => {
                    object_value_codec::write_u8(out, 1);
                    write_dump_u64(out, span.offset, "string mapped offset")?;
                    write_dump_u64(out, span.len, "string mapped length")?;
                }
                DumpByteData::StaticRoData { key, len } => {
                    object_value_codec::write_u8(out, 2);
                    object_value_codec::write_u64(out, *key);
                    object_value_codec::write_u64(out, *len);
                }
            }
            write_text_property_runs(out, text_props)?;
        }
        DumpHeapObject::Overlay(overlay) => {
            object_value_codec::write_u8(out, EXTRA_OVERLAY);
            object_value_codec::write_heap_object(out, &DumpHeapObject::Overlay(overlay.clone()))?;
        }
        DumpHeapObject::Marker(marker) => {
            object_value_codec::write_u8(out, EXTRA_MARKER);
            object_value_codec::write_heap_object(out, &DumpHeapObject::Marker(marker.clone()))?;
        }
        // Category C: full descriptor (no HeapImage bytes).
        DumpHeapObject::HashTable(table) => {
            object_value_codec::write_u8(out, EXTRA_HASH_TABLE);
            object_value_codec::write_heap_object(out, &DumpHeapObject::HashTable(table.clone()))?;
        }
        DumpHeapObject::Obarray { .. } => {
            object_value_codec::write_u8(out, EXTRA_OBARRAY);
            object_value_codec::write_heap_object(out, obj)?;
        }
        DumpHeapObject::ByteCode(function) => {
            object_value_codec::write_u8(out, EXTRA_BYTE_CODE);
            let mut function = function.clone();
            if has_mapped_slots {
                // The constants pool lives in the mapped heap image (a
                // SPAN_SLOTS_ONLY span); writing it into the descriptor too
                // would make the loader PARSE it a second time — the parse,
                // not the conversion, was the measured cost. The loader
                // aliases the span and ignores this (now empty) field.
                function.constants = Vec::new();
            }
            object_value_codec::write_heap_object(out, &DumpHeapObject::ByteCode(function))?;
        }
        DumpHeapObject::CharTable { .. } => {
            object_value_codec::write_u8(out, EXTRA_CHAR_TABLE);
            object_value_codec::write_heap_object(out, obj)?;
        }
        DumpHeapObject::SubCharTable { .. } => {
            object_value_codec::write_u8(out, EXTRA_SUB_CHAR_TABLE);
            object_value_codec::write_heap_object(out, obj)?;
        }
        DumpHeapObject::Subr {
            name,
            min_args,
            max_args,
        } => {
            object_value_codec::write_u8(out, EXTRA_SUBR);
            object_value_codec::write_u32(out, name.0);
            object_value_codec::write_u16(out, *min_args);
            write_opt_u16(out, *max_args);
        }
        DumpHeapObject::Buffer(id) => {
            object_value_codec::write_u8(out, EXTRA_BUFFER);
            object_value_codec::write_u64(out, id.0);
        }
        DumpHeapObject::Window(id) => {
            object_value_codec::write_u8(out, EXTRA_WINDOW);
            object_value_codec::write_u64(out, *id);
        }
        DumpHeapObject::Frame(id) => {
            object_value_codec::write_u8(out, EXTRA_FRAME);
            object_value_codec::write_u64(out, *id);
        }
        DumpHeapObject::Timer(id) => {
            object_value_codec::write_u8(out, EXTRA_TIMER);
            object_value_codec::write_u64(out, *id);
        }
        DumpHeapObject::Free => {
            object_value_codec::write_u8(out, EXTRA_FREE);
        }
    }
    Ok(())
}

fn object_needs_extra(obj: &DumpHeapObject) -> bool {
    // Property-free strings whose bytes are mapped are self-contained in the
    // heap image: `write_raw_string_obj` bakes the StringObj header in and
    // relocates the data pointer, and the loader reconstructs them from the
    // object-starts span (installing only the storage sidecar).  They need no
    // object_extra descriptor.  This must stay in lockstep with the
    // self-containment decision in `object_starts::write_object_span`.
    if let DumpHeapObject::Str {
        data: DumpByteData::Mapped(_),
        text_props,
        ..
    } = obj
        && text_props.is_empty()
    {
        return false;
    }
    // Gnu-instruction bytecode is self-contained: everything the descriptor
    // carried rides the extras region after the mapped ByteCodeObj (see
    // `mapped_heap::BytecodeExtras`); its span length exceeding
    // `size_of::<ByteCodeObj>()` is the load-side signal. Decoded-instruction
    // functions (hand-assembled tests) stay descriptor-driven.
    if let DumpHeapObject::ByteCode(function) = obj
        && matches!(
            function.instructions,
            crate::emacs_core::pdump::types::DumpByteCodeInstructions::Gnu(_)
        )
    {
        return false;
    }
    !matches!(
        obj,
        DumpHeapObject::Cons { .. }
            | DumpHeapObject::Float(_)
            | DumpHeapObject::Vector(_)
            | DumpHeapObject::Lambda(_)
            | DumpHeapObject::Macro(_)
            | DumpHeapObject::Record(_)
            | DumpHeapObject::CharTable { .. }
            | DumpHeapObject::SubCharTable { .. }
    )
}

// ---------------------------------------------------------------------------
// Load (load path)
// ---------------------------------------------------------------------------

/// Load the sparse ObjectExtra section into the present extra records.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn load_object_extra(section: &[u8]) -> Result<Vec<ObjectExtra>, DumpError> {
    let (_count, payload) = object_extra_payload(section)?;
    let mut cursor = object_value_codec::Cursor::new_at(payload, 0);
    let mut extras = Vec::new();
    while !cursor.is_empty() {
        let _index = read_dump_usize(&mut cursor, "object-extra object index")?;
        extras.push(read_object_extra(&mut cursor)?);
    }
    Ok(extras)
}

/// Load ObjectExtra for the file pdump path without expanding mapped
/// vectorlike objects into large nil-filled placeholder slot vectors.
///
/// GNU's pdumper does not serialize semantic descriptors for objects already in
/// the mapped image. Neomacs still needs a per-object descriptor vector while
/// the loader is transitional, but mapped vectorlike objects are now treated as
/// self-contained heap-image records instead of semantic ObjectExtra entries.
pub(crate) fn load_file_object_descriptors(
    section: &[u8],
    spans: &LoadedSpans<'_>,
    mapped_heap: Option<MappedHeapView>,
) -> Result<FileObjectDescriptors, DumpError> {
    let (count, payload) = object_extra_payload(section)?;
    if spans.len() != count {
        return Err(DumpError::ImageFormatError(format!(
            "object-extra count {count} does not match object-starts count {}",
            spans.len()
        )));
    }
    let mut descriptors = FileObjectDescriptors::new(count, payload.len());

    let mut cursor = object_value_codec::Cursor::new_at(payload, 0);
    while !cursor.is_empty() {
        let index = read_dump_usize(&mut cursor, "object-extra object index")?;
        if index >= count {
            return Err(DumpError::ImageFormatError(format!(
                "object-extra index {index} is outside object count {count}"
            )));
        }
        if descriptors.get(index).is_some()
            || mapped_object_is_self_contained(spans.get(index), mapped_heap)?
        {
            return Err(DumpError::ImageFormatError(format!(
                "object-extra has duplicate or unnecessary record for mapped object {index}"
            )));
        }
        let extra = read_object_extra(&mut cursor)?;
        descriptors.insert(index, object_extra_into_heap_object(extra))?;
    }

    // Completeness - every object is either descriptor-driven or
    // self-contained - is a property of the WRITER: it emits a descriptor for
    // exactly the non-self-contained set. Re-deriving self-containment for
    // all ~40K mapped objects on every load cost ~2.5M Ir and can only catch
    // a dumper bug, so it validates in debug builds (where every round-trip
    // test runs) and trusts the dump in release, like GNU's pdumper does.
    #[cfg(debug_assertions)]
    for index in 0..count {
        if descriptors.get(index).is_none()
            && !mapped_object_is_self_contained(spans.get(index), mapped_heap)?
        {
            return Err(DumpError::ImageFormatError(format!(
                "object-extra has no descriptor for object {index}"
            )));
        }
    }

    Ok(descriptors)
}

fn object_extra_payload(section: &[u8]) -> Result<(usize, &[u8]), DumpError> {
    if section.len() < HEADER_SIZE {
        return Err(DumpError::ImageFormatError(
            "object-extra section too small for header".into(),
        ));
    }
    let header = *bytemuck::from_bytes::<ObjectExtraHeader>(&section[..HEADER_SIZE]);
    if header.magic != OBJECT_EXTRA_MAGIC {
        return Err(DumpError::ImageFormatError(
            "object-extra magic mismatch".into(),
        ));
    }
    if header.version != OBJECT_EXTRA_FORMAT_VERSION {
        return Err(DumpError::ImageFormatError(format!(
            "object-extra version mismatch: expected {}, got {}",
            OBJECT_EXTRA_FORMAT_VERSION, header.version,
        )));
    }
    let count = header.object_count as usize;
    let payload_start = header.payload_offset as usize;
    let payload_end = payload_start + header.payload_len as usize;
    if payload_end > section.len() {
        return Err(DumpError::ImageFormatError(
            "object-extra payload extends past section".into(),
        ));
    }

    Ok((count, &section[payload_start..payload_end]))
}

fn object_extra_into_heap_object(extra: ObjectExtra) -> DumpHeapObject {
    match extra {
        ObjectExtra::String {
            size,
            size_byte,
            byte_data,
            text_props,
        } => DumpHeapObject::Str {
            data: byte_data,
            size,
            size_byte,
            text_props,
        },
        ObjectExtra::HashTable(table) => DumpHeapObject::HashTable(table),
        ObjectExtra::Obarray { buckets, count } => DumpHeapObject::Obarray { buckets, count },
        ObjectExtra::ByteCode(function) => DumpHeapObject::ByteCode(function),
        ObjectExtra::CharTable {
            defalt,
            parent,
            purpose,
            ascii,
            contents,
            extras,
        } => DumpHeapObject::CharTable {
            defalt,
            parent,
            purpose,
            ascii,
            contents,
            extras,
        },
        ObjectExtra::SubCharTable {
            depth,
            min_char,
            contents,
        } => DumpHeapObject::SubCharTable {
            depth,
            min_char,
            contents,
        },
        ObjectExtra::Subr {
            name,
            min_args,
            max_args,
        } => DumpHeapObject::Subr {
            name,
            min_args,
            max_args,
        },
        ObjectExtra::Buffer(id) => DumpHeapObject::Buffer(id),
        ObjectExtra::Window(id) => DumpHeapObject::Window(id),
        ObjectExtra::Frame(id) => DumpHeapObject::Frame(id),
        ObjectExtra::Timer(id) => DumpHeapObject::Timer(id),
        ObjectExtra::Overlay(overlay) => DumpHeapObject::Overlay(overlay),
        ObjectExtra::Marker(marker) => DumpHeapObject::Marker(marker),
        ObjectExtra::Free => DumpHeapObject::Free,
    }
}

fn span_len_for_self_containment(span: LoadedObjectSpan) -> usize {
    match span {
        LoadedObjectSpan::Vectorlike { object, .. } => object.len as usize,
        _ => 0,
    }
}

fn mapped_object_is_self_contained(
    span: LoadedObjectSpan,
    mapped_heap: Option<MappedHeapView>,
) -> Result<bool, DumpError> {
    match span {
        LoadedObjectSpan::Cons(_) | LoadedObjectSpan::Float(_) => Ok(true),
        // A bare slot span holds only the constants array; the object itself
        // is still descriptor-driven.
        LoadedObjectSpan::SlotsOnly(_) => Ok(false),
        LoadedObjectSpan::Vectorlike { object, .. } => {
            let mapped_heap = mapped_heap.ok_or_else(|| {
                DumpError::ImageFormatError(
                    "mapped vectorlike span requires a heap image section".into(),
                )
            })?;
            match mapped_heap.veclike_type(object)? {
                VecLikeType::Vector
                | VecLikeType::Lambda
                | VecLikeType::Macro
                | VecLikeType::Record
                | VecLikeType::CharTable
                | VecLikeType::SubCharTable => Ok(true),
                VecLikeType::Marker | VecLikeType::Overlay => Ok(false),
                VecLikeType::ByteCode => Ok(span_len_for_self_containment(span)
                    > std::mem::size_of::<crate::tagged::header::ByteCodeObj>()),
                other => Err(DumpError::ImageFormatError(format!(
                    "unexpected mapped vectorlike type {other:?} in object-starts"
                ))),
            }
        }
        // A string span carrying a byte-data span is self-contained (the loader
        // reconstructs it from the heap image); without one it is Category B
        // and still needs an object_extra descriptor.
        LoadedObjectSpan::String { data: Some(_), .. } => Ok(true),
        LoadedObjectSpan::None
        | LoadedObjectSpan::String { data: None, .. }
        | LoadedObjectSpan::Unmapped => Ok(false),
    }
}

fn read_object_extra(cursor: &mut object_value_codec::Cursor) -> Result<ObjectExtra, DumpError> {
    let tag = cursor.read_u8("object extra tag")?;
    match tag {
        EXTRA_STRING => {
            let size = read_dump_usize(cursor, "string size")?;
            let size_byte = read_dump_i32(cursor, "string size_byte")?;
            let byte_data_tag = cursor.read_u8("string byte data tag")?;
            let byte_data = match byte_data_tag {
                0 => {
                    let len = read_dump_usize(cursor, "string owned len")?;
                    let bytes = cursor.read_bytes_fixed(len)?;
                    DumpByteData::owned(bytes)
                }
                1 => {
                    let offset = read_dump_u64(cursor, "string mapped offset")?;
                    let len = read_dump_u64(cursor, "string mapped len")?;
                    DumpByteData::mapped(offset, len)
                }
                2 => {
                    let key = cursor.read_u64("string static rodata key")?;
                    let len = cursor.read_u64("string static rodata len")?;
                    DumpByteData::static_rodata(key, len)
                }
                other => {
                    return Err(DumpError::ImageFormatError(format!(
                        "unknown string byte data tag {other}"
                    )));
                }
            };
            let text_props = read_text_property_runs(cursor)?;
            Ok(ObjectExtra::String {
                size,
                size_byte,
                byte_data,
                text_props,
            })
        }
        EXTRA_HASH_TABLE => {
            // Skip the HEAP_HASH_TABLE tag written by write_heap_object
            let obj = cursor.read_heap_object()?;
            match obj {
                DumpHeapObject::HashTable(table) => Ok(ObjectExtra::HashTable(table)),
                other => Err(DumpError::ImageFormatError(format!(
                    "expected HashTable in ObjectExtra, got {:?}",
                    other.variant_name()
                ))),
            }
        }
        EXTRA_OBARRAY => {
            let obj = cursor.read_heap_object()?;
            match obj {
                DumpHeapObject::Obarray { buckets, count } => {
                    Ok(ObjectExtra::Obarray { buckets, count })
                }
                other => Err(DumpError::ImageFormatError(format!(
                    "expected Obarray in ObjectExtra, got {:?}",
                    other.variant_name()
                ))),
            }
        }
        EXTRA_BYTE_CODE => {
            let obj = cursor.read_heap_object()?;
            match obj {
                DumpHeapObject::ByteCode(function) => Ok(ObjectExtra::ByteCode(function)),
                other => Err(DumpError::ImageFormatError(format!(
                    "expected ByteCode in ObjectExtra, got {:?}",
                    other.variant_name()
                ))),
            }
        }
        EXTRA_CHAR_TABLE => {
            let obj = cursor.read_heap_object()?;
            match obj {
                DumpHeapObject::CharTable {
                    defalt,
                    parent,
                    purpose,
                    ascii,
                    contents,
                    extras,
                } => Ok(ObjectExtra::CharTable {
                    defalt,
                    parent,
                    purpose,
                    ascii,
                    contents,
                    extras,
                }),
                other => Err(DumpError::ImageFormatError(format!(
                    "expected CharTable in ObjectExtra, got {:?}",
                    other.variant_name()
                ))),
            }
        }
        EXTRA_SUB_CHAR_TABLE => {
            let obj = cursor.read_heap_object()?;
            match obj {
                DumpHeapObject::SubCharTable {
                    depth,
                    min_char,
                    contents,
                } => Ok(ObjectExtra::SubCharTable {
                    depth,
                    min_char,
                    contents,
                }),
                other => Err(DumpError::ImageFormatError(format!(
                    "expected SubCharTable in ObjectExtra, got {:?}",
                    other.variant_name()
                ))),
            }
        }
        EXTRA_SUBR => {
            let name = DumpNameId(cursor.read_u32("subr name id")?);
            let min_args = cursor.read_u16("subr min args")?;
            let max_args = cursor.read_opt_u16()?;
            Ok(ObjectExtra::Subr {
                name,
                min_args,
                max_args,
            })
        }
        EXTRA_BUFFER => {
            let id = DumpBufferId(cursor.read_u64("buffer id")?);
            Ok(ObjectExtra::Buffer(id))
        }
        EXTRA_WINDOW => {
            let id = cursor.read_u64("window id")?;
            Ok(ObjectExtra::Window(id))
        }
        EXTRA_FRAME => {
            let id = cursor.read_u64("frame id")?;
            Ok(ObjectExtra::Frame(id))
        }
        EXTRA_TIMER => {
            let id = cursor.read_u64("timer id")?;
            Ok(ObjectExtra::Timer(id))
        }
        EXTRA_OVERLAY => {
            let obj = cursor.read_heap_object()?;
            match obj {
                DumpHeapObject::Overlay(overlay) => Ok(ObjectExtra::Overlay(overlay)),
                other => Err(DumpError::ImageFormatError(format!(
                    "expected Overlay in ObjectExtra, got {:?}",
                    other.variant_name()
                ))),
            }
        }
        EXTRA_MARKER => {
            let obj = cursor.read_heap_object()?;
            match obj {
                DumpHeapObject::Marker(marker) => Ok(ObjectExtra::Marker(marker)),
                other => Err(DumpError::ImageFormatError(format!(
                    "expected Marker in ObjectExtra, got {:?}",
                    other.variant_name()
                ))),
            }
        }
        EXTRA_FREE => Ok(ObjectExtra::Free),
        other => Err(DumpError::ImageFormatError(format!(
            "unknown object-extra tag {other}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Write helpers
// ---------------------------------------------------------------------------

fn write_opt_u16(out: &mut Vec<u8>, value: Option<u16>) {
    match value {
        Some(v) => {
            object_value_codec::write_u8(out, 1);
            object_value_codec::write_u16(out, v);
        }
        None => object_value_codec::write_u8(out, 0),
    }
}

fn write_text_property_runs(
    out: &mut Vec<u8>,
    runs: &[DumpStringTextPropertyRun],
) -> Result<(), DumpError> {
    write_dump_usize(out, runs.len(), "string text property run count")?;
    for run in runs {
        write_dump_usize(out, run.start, "string text property start")?;
        write_dump_usize(out, run.end, "string text property end")?;
        object_value_codec::write_value(out, &run.plist)?;
    }
    Ok(())
}

fn read_text_property_runs(
    cursor: &mut object_value_codec::Cursor,
) -> Result<Vec<DumpStringTextPropertyRun>, DumpError> {
    let len = read_dump_usize(cursor, "string text property run count")?;
    let mut runs = Vec::with_capacity(len);
    for _ in 0..len {
        runs.push(DumpStringTextPropertyRun {
            start: read_dump_usize(cursor, "string text property start")?,
            end: read_dump_usize(cursor, "string text property end")?,
            plist: cursor.read_value()?,
        });
    }
    Ok(runs)
}

fn write_dump_usize(out: &mut Vec<u8>, value: usize, what: &str) -> Result<(), DumpError> {
    let value = u32::try_from(value)
        .map_err(|_| DumpError::SerializationError(format!("{what} overflows dump_off")))?;
    object_value_codec::write_u32(out, value);
    Ok(())
}

fn write_dump_u64(out: &mut Vec<u8>, value: u64, what: &str) -> Result<(), DumpError> {
    let value = u32::try_from(value)
        .map_err(|_| DumpError::SerializationError(format!("{what} overflows dump_off")))?;
    object_value_codec::write_u32(out, value);
    Ok(())
}

fn write_dump_i32(out: &mut Vec<u8>, value: i64, what: &str) -> Result<(), DumpError> {
    let value = i32::try_from(value)
        .map_err(|_| DumpError::SerializationError(format!("{what} overflows dump_off")))?;
    out.extend_from_slice(&value.to_ne_bytes());
    Ok(())
}

fn read_dump_usize(
    cursor: &mut object_value_codec::Cursor,
    what: &str,
) -> Result<usize, DumpError> {
    Ok(cursor.read_u32(what)? as usize)
}

fn read_dump_u64(cursor: &mut object_value_codec::Cursor, what: &str) -> Result<u64, DumpError> {
    Ok(u64::from(cursor.read_u32(what)?))
}

fn read_dump_i32(cursor: &mut object_value_codec::Cursor, what: &str) -> Result<i64, DumpError> {
    let raw = cursor.read_u32(what)?;
    Ok(i64::from(i32::from_ne_bytes(raw.to_ne_bytes())))
}

// ---------------------------------------------------------------------------
// DumpHeapObject helper
// ---------------------------------------------------------------------------

impl DumpHeapObject {
    fn variant_name(&self) -> &'static str {
        match self {
            DumpHeapObject::Cons { .. } => "Cons",
            DumpHeapObject::Vector(_) => "Vector",
            DumpHeapObject::HashTable(_) => "HashTable",
            DumpHeapObject::Obarray { .. } => "Obarray",
            DumpHeapObject::Str { .. } => "Str",
            DumpHeapObject::Float(_) => "Float",
            DumpHeapObject::Lambda(_) => "Lambda",
            DumpHeapObject::Macro(_) => "Macro",
            DumpHeapObject::ByteCode(_) => "ByteCode",
            DumpHeapObject::CharTable { .. } => "CharTable",
            DumpHeapObject::SubCharTable { .. } => "SubCharTable",
            DumpHeapObject::Record(_) => "Record",
            DumpHeapObject::Marker(_) => "Marker",
            DumpHeapObject::Overlay(_) => "Overlay",
            DumpHeapObject::Buffer(_) => "Buffer",
            DumpHeapObject::Window(_) => "Window",
            DumpHeapObject::Frame(_) => "Frame",
            DumpHeapObject::Timer(_) => "Timer",
            DumpHeapObject::Subr { .. } => "Subr",
            DumpHeapObject::Free => "Free",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tagged::header::{GcHeader, LambdaObj, MacroObj, RecordObj, VectorObj};

    #[test]
    fn object_extra_is_sparse_for_category_a_descriptors() {
        let bytes = build_object_extra(
            &[
                DumpHeapObject::Cons {
                    car: DumpValue::Nil,
                    cdr: DumpValue::True,
                },
                DumpHeapObject::Vector(vec![DumpValue::Nil, DumpValue::True]),
                DumpHeapObject::Free,
            ],
            &[],
        )
        .expect("build object extra");

        let extras = load_object_extra(&bytes).expect("load object extra");
        assert_eq!(extras.len(), 1);
        assert!(matches!(extras[0], ObjectExtra::Free));
    }

    #[test]
    fn object_extra_loads_sparse_heap_objects_from_spans() {
        let objects = vec![
            DumpHeapObject::Cons {
                car: DumpValue::True,
                cdr: DumpValue::Nil,
            },
            DumpHeapObject::Free,
        ];
        let bytes = build_object_extra(&objects, &[]).expect("build object extra");
        let heap = DumpTaggedHeap {
            objects,
            mapped_cons: vec![Some(DumpConsSpan { offset: 0 }), None],
            mapped_floats: vec![None, None],
            mapped_strings: vec![None, None],
            mapped_veclikes: vec![None, None],
            mapped_slots: vec![None, None],
        };
        let spans = LoadedSpans::from_heap(&heap);

        let objects = load_file_object_descriptors(&bytes, &spans, None)
            .expect("load heap objects from sparse extra");

        assert!(objects.get(0).is_none());
        assert!(matches!(objects.get(1), Some(DumpHeapObject::Free)));
    }

    #[test]
    fn object_extra_round_trips_static_rodata_string_descriptor() {
        let objects = vec![DumpHeapObject::Str {
            data: DumpByteData::static_rodata(0x1234_5678, 7),
            size: 7,
            size_byte: -2,
            text_props: Vec::new(),
        }];
        let bytes = build_object_extra(&objects, &[]).expect("build object extra");
        let extras = load_object_extra(&bytes).expect("load object extra");

        assert!(matches!(
            &extras[0],
            ObjectExtra::String {
                size: 7,
                size_byte: -2,
                byte_data: DumpByteData::StaticRoData { key: 0x1234_5678, len: 7 },
                text_props,
            } if text_props.is_empty()
        ));
    }

    #[test]
    fn compact_object_extra_leaves_mapped_vectorlike_objects_self_contained() {
        let objects = vec![
            DumpHeapObject::Vector(vec![DumpValue::Nil, DumpValue::True]),
            DumpHeapObject::Lambda(vec![DumpValue::Nil, DumpValue::True]),
            DumpHeapObject::Macro(vec![DumpValue::Nil, DumpValue::True]),
            DumpHeapObject::Record(vec![DumpValue::Nil, DumpValue::True]),
        ];
        let bytes = build_object_extra(&objects, &[]).expect("build object extra");
        assert_eq!(bytes.len(), HEADER_SIZE);

        let mut offset = 0u64;
        let vector_span = reserve_test_object::<VectorObj>(&mut offset);
        let lambda_span = reserve_test_object::<LambdaObj>(&mut offset);
        let macro_span = reserve_test_object::<MacroObj>(&mut offset);
        let record_span = reserve_test_object::<RecordObj>(&mut offset);
        let mut heap_bytes = vec![0u8; offset as usize];
        write_test_veclike_type(&mut heap_bytes, vector_span, VecLikeType::Vector);
        write_test_veclike_type(&mut heap_bytes, lambda_span, VecLikeType::Lambda);
        write_test_veclike_type(&mut heap_bytes, macro_span, VecLikeType::Macro);
        write_test_veclike_type(&mut heap_bytes, record_span, VecLikeType::Record);

        let heap = DumpTaggedHeap {
            objects,
            mapped_cons: vec![None; 4],
            mapped_floats: vec![None; 4],
            mapped_strings: vec![None; 4],
            mapped_veclikes: vec![
                Some(vector_span),
                Some(lambda_span),
                Some(macro_span),
                Some(record_span),
            ],
            mapped_slots: vec![None; 4],
        };
        let spans = LoadedSpans::from_heap(&heap);
        let mapped_heap = MappedHeapView::from_mut_slice(&mut heap_bytes);

        let objects = load_file_object_descriptors(&bytes, &spans, Some(mapped_heap))
            .expect("load compact heap objects from extra");

        assert_eq!(objects.len(), 4);
        assert_eq!(objects.descriptor_count(), 0);
        assert!((0..objects.len()).all(|index| objects.get(index).is_none()));
    }

    #[test]
    fn object_extra_rejects_removed_none_tag() {
        let mut bytes =
            build_object_extra(&[DumpHeapObject::Free], &[]).expect("build object extra");
        bytes[HEADER_SIZE + 4] = 100;

        let err = load_object_extra(&bytes).expect_err("removed NONE tag should be rejected");
        assert!(matches!(err, DumpError::ImageFormatError(_)));
    }

    fn reserve_test_object<T>(offset: &mut u64) -> DumpVecLikeSpan {
        let span = DumpVecLikeSpan {
            offset: *offset,
            len: std::mem::size_of::<T>() as u64,
        };
        *offset += span.len;
        span
    }

    fn write_test_veclike_type(bytes: &mut [u8], span: DumpVecLikeSpan, type_tag: VecLikeType) {
        bytes[span.offset as usize + std::mem::size_of::<GcHeader>()] = u8::from(type_tag);
    }
}
