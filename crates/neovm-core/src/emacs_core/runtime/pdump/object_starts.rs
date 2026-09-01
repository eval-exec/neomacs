//! ObjectStarts section: maps object index → HeapImage offset + span metadata.
//!
//! During dump, the span tables (mapped_cons, mapped_floats, mapped_strings,
//! mapped_veclikes, mapped_slots) are computed and stored directly in this
//! section. During load, they are read back directly, eliminating the need
//! to re-run the layout algorithm via `rebuild_heap_metadata`.

use bytemuck::{Pod, Zeroable};

use super::{DumpError, types::*};

const OBJECT_STARTS_MAGIC: [u8; 16] = *b"NEOOBJSTARTS\0\0\0\0";
const OBJECT_STARTS_FORMAT_VERSION: u32 = 6;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ObjectStartsHeader {
    magic: [u8; 16],
    version: u32,
    header_size: u32,
    object_count: u64,
}

const HEADER_SIZE: usize = std::mem::size_of::<ObjectStartsHeader>();

/// One fixed-width span record (format v6). The section is `object_count`
/// of these after the header; the loader BORROWS them from the mapped image
/// and decodes a row on demand instead of parsing every record into a
/// `Vec<LoadedObjectSpan>` up front (~8.5M Ir of the load at v5). Rows are
/// read with `pod_read_unaligned`, so the payload needs no alignment.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SpanRow {
    tag: u8,
    /// Bit 0: String => self-contained byte data present; Vectorlike =>
    /// slot span present.
    flags: u8,
    reserved: [u8; 2],
    a: u32,
    b: u32,
    c: u32,
    d: u32,
}

const ROW_SIZE: usize = std::mem::size_of::<SpanRow>();
const FLAG_EXTRA_SPAN: u8 = 1;

fn row_off(value: u64, what: &str) -> Result<u32, DumpError> {
    u32::try_from(value).map_err(|_| {
        DumpError::SerializationError(format!("object-starts {what} {value} overflows u32"))
    })
}

/// Build the ObjectStarts section bytes from the dump tagged heap.
///
/// GNU pdumper keeps load metadata in the mapped image and walks it directly.
/// Keep this section compact, but make file pdump load borrow the mapped bytes
/// with a small object-index offset table instead of decoding every span into
/// Rust heap objects.
pub(crate) fn build_object_starts(heap: &DumpTaggedHeap) -> Result<Vec<u8>, DumpError> {
    let count = heap.objects.len();
    let mut bytes = vec![0u8; HEADER_SIZE + count * ROW_SIZE];

    for (i, obj) in heap.objects.iter().enumerate() {
        let row = object_span_row(obj, heap, i)?;
        let start = HEADER_SIZE + i * ROW_SIZE;
        bytes[start..start + ROW_SIZE].copy_from_slice(bytemuck::bytes_of(&row));
    }

    let header = ObjectStartsHeader {
        magic: OBJECT_STARTS_MAGIC,
        version: OBJECT_STARTS_FORMAT_VERSION,
        header_size: HEADER_SIZE as u32,
        object_count: count as u64,
    };
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    Ok(bytes)
}

// Type tags for span records.
const SPAN_NONE: u8 = 0;
const SPAN_CONS: u8 = 1;
const SPAN_FLOAT: u8 = 2;
const SPAN_STRING: u8 = 3;
const SPAN_VECTORLIKE: u8 = 4;
// Category C objects (no span).
const SPAN_UNMAPPED: u8 = 5;
/// A bare slot span with no veclike header: the object itself is
/// descriptor-driven (Category B), but one of its Value arrays lives in the
/// mapped heap (bytecode constant pools).
const SPAN_SLOTS_ONLY: u8 = 6;

fn object_span_row(
    obj: &DumpHeapObject,
    heap: &DumpTaggedHeap,
    index: usize,
) -> Result<SpanRow, DumpError> {
    let mut row = SpanRow::zeroed();
    match obj {
        DumpHeapObject::Cons { .. } => {
            if let Some(span) = heap.mapped_cons.get(index).and_then(|s| *s) {
                row.tag = SPAN_CONS;
                row.a = row_off(span.offset, "cons offset")?;
            }
        }
        DumpHeapObject::Float(_) => {
            if let Some(span) = heap.mapped_floats.get(index).and_then(|s| *s) {
                row.tag = SPAN_FLOAT;
                row.a = row_off(span.offset, "float offset")?;
            }
        }
        DumpHeapObject::Str {
            data, text_props, ..
        } => {
            if let Some(span) = heap.mapped_strings.get(index).and_then(|s| *s) {
                row.tag = SPAN_STRING;
                row.a = row_off(span.offset, "string offset")?;
                row.b = row_off(span.len, "string length")?;
                // A property-free string whose bytes live in the mapped image
                // is self-contained: `write_raw_string_obj` already baked its
                // StringObj header into the image and registered a relocation
                // for the data pointer, so the loader only needs the byte-data
                // span -- no object_extra descriptor.
                if let DumpByteData::Mapped(byte_span) = data
                    && text_props.is_empty()
                {
                    row.flags |= FLAG_EXTRA_SPAN;
                    row.c = row_off(byte_span.offset, "string byte offset")?;
                    row.d = row_off(byte_span.len, "string byte length")?;
                }
            }
        }
        DumpHeapObject::Vector(_)
        | DumpHeapObject::Lambda(_)
        | DumpHeapObject::Macro(_)
        | DumpHeapObject::Record(_)
        | DumpHeapObject::Marker(_)
        | DumpHeapObject::Overlay(_)
        | DumpHeapObject::CharTable { .. }
        | DumpHeapObject::SubCharTable { .. } => {
            let vl = heap.mapped_veclikes.get(index).and_then(|s| *s);
            let sl = heap.mapped_slots.get(index).and_then(|s| *s);
            if let Some(vl) = vl {
                row.tag = SPAN_VECTORLIKE;
                row.a = row_off(vl.offset, "vectorlike offset")?;
                row.b = row_off(vl.len, "vectorlike length")?;
                if let Some(sl) = sl {
                    row.flags |= FLAG_EXTRA_SPAN;
                    row.c = row_off(sl.offset, "slot offset")?;
                    row.d = row_off(sl.len, "slot length")?;
                }
            }
        }
        DumpHeapObject::ByteCode(_) => {
            // Mapped ByteCodeObj (its span length past the struct is the
            // extras region that replaces the object-extra descriptor);
            // constants ride the slots span exactly like vectors.
            let vl = heap.mapped_veclikes.get(index).and_then(|s| *s);
            let sl = heap.mapped_slots.get(index).and_then(|s| *s);
            if let Some(vl) = vl {
                row.tag = SPAN_VECTORLIKE;
                row.a = row_off(vl.offset, "vectorlike offset")?;
                row.b = row_off(vl.len, "vectorlike length")?;
                if let Some(sl) = sl {
                    row.flags |= FLAG_EXTRA_SPAN;
                    row.c = row_off(sl.offset, "slot offset")?;
                    row.d = row_off(sl.len, "slot length")?;
                }
            } else if let Some(sl) = sl {
                row.tag = SPAN_SLOTS_ONLY;
                row.a = row_off(sl.offset, "slot offset")?;
                row.b = row_off(sl.len, "slot length")?;
            } else {
                row.tag = SPAN_UNMAPPED;
            }
        }
        // Category C: no HeapImage representation.
        DumpHeapObject::HashTable(_)
        | DumpHeapObject::Obarray { .. }
        | DumpHeapObject::Subr { .. }
        | DumpHeapObject::Buffer(_)
        | DumpHeapObject::Window(_)
        | DumpHeapObject::Frame(_)
        | DumpHeapObject::Timer(_)
        | DumpHeapObject::Free => {
            row.tag = SPAN_UNMAPPED;
        }
    }
    Ok(row)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum LoadedObjectSpan {
    #[default]
    None,
    Unmapped,
    Cons(DumpConsSpan),
    Float(DumpFloatSpan),
    String {
        /// Location of the mapped `StringObj` header (already baked into the
        /// image at dump time with its `data` pointer relocated).
        object: DumpStringSpan,
        /// Present when the string is self-contained in the heap image: a
        /// property-free string whose bytes are mapped.  The loader uses this
        /// byte-data span to install the storage sidecar directly, skipping the
        /// `object_extra` descriptor.  `None` => descriptor-driven (Category B).
        data: Option<DumpByteSpan>,
    },
    Vectorlike {
        object: DumpVecLikeSpan,
        slots: Option<DumpSlotSpan>,
    },
    /// Bare slot span, no veclike header (bytecode constant pools).
    SlotsOnly(DumpSlotSpan),
}

/// Load-side object span lookup.
///
/// GNU pdumper keeps the mapped dump as the primary object store and walks compact
/// relocation metadata at load time. The file path does the same here: `get`
/// decodes a 20-byte row straight out of the BORROWED mapped section, so the
/// table costs zero anonymous pages — 166K decoded records were 8.0MB of
/// written (fault-per-page, no fault-around) load-time heap, while the mapped
/// rows are 3.3MB of read-only file pages the kernel faults in ~16-page
/// batches. The per-access decode re-pays ~7M Ir across a load (measured when
/// v5 borrowed rows), but at fault economics that trade wins: ~2K anonymous
/// faults bought back for ~1.4ms of arithmetic.
pub(crate) struct LoadedSpans<'a> {
    repr: SpansRepr<'a>,
}

enum SpansRepr<'a> {
    /// File image: the mapped ObjectStarts payload, decoded per access.
    Mapped { payload: &'a [u8], count: usize },
    /// In-memory dump heap (dump side, tests): owned decoded records.
    Owned(Vec<LoadedObjectSpan>),
}

fn decode_span_row(row: SpanRow) -> LoadedObjectSpan {
    let extra = row.flags & FLAG_EXTRA_SPAN != 0;
    match row.tag {
        SPAN_CONS => LoadedObjectSpan::Cons(DumpConsSpan {
            offset: row.a.into(),
        }),
        SPAN_FLOAT => LoadedObjectSpan::Float(DumpFloatSpan {
            offset: row.a.into(),
        }),
        SPAN_STRING => LoadedObjectSpan::String {
            object: DumpStringSpan {
                offset: row.a.into(),
                len: row.b.into(),
            },
            data: extra.then(|| DumpByteSpan {
                offset: row.c.into(),
                len: row.d.into(),
            }),
        },
        SPAN_VECTORLIKE => LoadedObjectSpan::Vectorlike {
            object: DumpVecLikeSpan {
                offset: row.a.into(),
                len: row.b.into(),
            },
            slots: extra.then(|| DumpSlotSpan {
                offset: row.c.into(),
                len: row.d.into(),
            }),
        },
        SPAN_SLOTS_ONLY => LoadedObjectSpan::SlotsOnly(DumpSlotSpan {
            offset: row.a.into(),
            len: row.b.into(),
        }),
        SPAN_UNMAPPED => LoadedObjectSpan::Unmapped,
        // SPAN_NONE and anything unknown: downstream span consumers validate
        // offsets before touching memory, so a corrupt tag degrades to "no
        // span" and errors at its use site rather than being re-validated
        // per object here.
        _ => {
            debug_assert_eq!(row.tag, SPAN_NONE, "unknown object-starts tag");
            LoadedObjectSpan::None
        }
    }
}

pub(crate) struct LoadedSpansIter<'spans, 'data> {
    spans: &'spans LoadedSpans<'data>,
    index: usize,
}

impl<'data> LoadedSpans<'data> {
    pub(crate) fn from_heap(heap: &DumpTaggedHeap) -> Self {
        let mut records = Vec::with_capacity(heap.objects.len());
        for index in 0..heap.objects.len() {
            records.push(span_record_from_heap(heap, index));
        }
        Self {
            repr: SpansRepr::Owned(records),
        }
    }

    pub(crate) fn len(&self) -> usize {
        match &self.repr {
            SpansRepr::Mapped { count, .. } => *count,
            SpansRepr::Owned(records) => records.len(),
        }
    }

    #[inline]
    pub(crate) fn get(&self, index: usize) -> LoadedObjectSpan {
        match &self.repr {
            SpansRepr::Mapped { payload, count } => {
                if index >= *count {
                    return LoadedObjectSpan::default();
                }
                let start = index * ROW_SIZE;
                decode_span_row(bytemuck::pod_read_unaligned(
                    &payload[start..start + ROW_SIZE],
                ))
            }
            SpansRepr::Owned(records) => records.get(index).copied().unwrap_or_default(),
        }
    }

    pub(crate) fn iter(&self) -> LoadedSpansIter<'_, 'data> {
        LoadedSpansIter {
            spans: self,
            index: 0,
        }
    }

    pub(crate) fn cons(&self, index: usize) -> Option<DumpConsSpan> {
        match self.get(index) {
            LoadedObjectSpan::Cons(span) => Some(span),
            _ => None,
        }
    }

    pub(crate) fn float(&self, index: usize) -> Option<DumpFloatSpan> {
        match self.get(index) {
            LoadedObjectSpan::Float(span) => Some(span),
            _ => None,
        }
    }

    pub(crate) fn string(&self, index: usize) -> Option<DumpStringSpan> {
        match self.get(index) {
            LoadedObjectSpan::String { object, .. } => Some(object),
            _ => None,
        }
    }

    /// Byte-data span for a self-contained string (property-free, mapped
    /// bytes).  `None` for descriptor-driven strings or non-strings.
    pub(crate) fn string_self_contained_data(&self, index: usize) -> Option<DumpByteSpan> {
        match self.get(index) {
            LoadedObjectSpan::String { data, .. } => data,
            _ => None,
        }
    }

    pub(crate) fn vectorlike(&self, index: usize) -> Option<DumpVecLikeSpan> {
        match self.get(index) {
            LoadedObjectSpan::Vectorlike { object, .. } => Some(object),
            _ => None,
        }
    }

    pub(crate) fn slots(&self, index: usize) -> Option<DumpSlotSpan> {
        match self.get(index) {
            LoadedObjectSpan::Vectorlike { slots, .. } => slots,
            LoadedObjectSpan::SlotsOnly(slots) => Some(slots),
            _ => None,
        }
    }
}

impl Iterator for LoadedSpansIter<'_, '_> {
    type Item = (usize, LoadedObjectSpan);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.spans.len() {
            return None;
        }
        let index = self.index;
        self.index += 1;
        Some((index, self.spans.get(index)))
    }
}

fn span_record_from_heap(heap: &DumpTaggedHeap, index: usize) -> LoadedObjectSpan {
    if let Some(span) = heap.mapped_cons.get(index).copied().flatten() {
        return LoadedObjectSpan::Cons(span);
    }
    if let Some(span) = heap.mapped_floats.get(index).copied().flatten() {
        return LoadedObjectSpan::Float(span);
    }
    if let Some(span) = heap.mapped_strings.get(index).copied().flatten() {
        // Match the self-containment decision in `write_object_span`: a
        // property-free string with mapped bytes carries its byte-data span so
        // the loader can skip the object_extra descriptor.
        let data = match heap.objects.get(index) {
            Some(DumpHeapObject::Str {
                data: DumpByteData::Mapped(byte_span),
                text_props,
                ..
            }) if text_props.is_empty() => Some(*byte_span),
            _ => None,
        };
        return LoadedObjectSpan::String { object: span, data };
    }
    if let Some(object) = heap.mapped_veclikes.get(index).copied().flatten() {
        return LoadedObjectSpan::Vectorlike {
            object,
            slots: heap.mapped_slots.get(index).copied().flatten(),
        };
    }
    match heap.objects.get(index) {
        Some(
            DumpHeapObject::HashTable(_)
            | DumpHeapObject::Obarray { .. }
            | DumpHeapObject::ByteCode(_)
            | DumpHeapObject::Subr { .. }
            | DumpHeapObject::Buffer(_)
            | DumpHeapObject::Window(_)
            | DumpHeapObject::Frame(_)
            | DumpHeapObject::Timer(_)
            | DumpHeapObject::Free,
        ) => LoadedObjectSpan::Unmapped,
        _ => LoadedObjectSpan::None,
    }
}

pub(crate) fn load_object_starts(section: &[u8]) -> Result<LoadedSpans<'_>, DumpError> {
    if section.len() < HEADER_SIZE {
        return Err(DumpError::ImageFormatError(
            "object-starts section too small for header".into(),
        ));
    }
    let header = *bytemuck::from_bytes::<ObjectStartsHeader>(&section[..HEADER_SIZE]);
    if header.magic != OBJECT_STARTS_MAGIC {
        return Err(DumpError::ImageFormatError(
            "object-starts magic mismatch".into(),
        ));
    }
    if header.version != OBJECT_STARTS_FORMAT_VERSION {
        return Err(DumpError::ImageFormatError(format!(
            "object-starts version mismatch: expected {}, got {}",
            OBJECT_STARTS_FORMAT_VERSION, header.version,
        )));
    }
    let count = usize::try_from(header.object_count).map_err(|_| {
        DumpError::ImageFormatError("object-starts object count overflows usize".into())
    })?;
    let payload = &section[HEADER_SIZE..];
    let expected = count.checked_mul(ROW_SIZE).ok_or_else(|| {
        DumpError::ImageFormatError("object-starts row payload length overflows usize".into())
    })?;
    if payload.len() != expected {
        return Err(DumpError::ImageFormatError(format!(
            "object-starts payload length {} does not match {count} rows of {ROW_SIZE} bytes",
            payload.len()
        )));
    }
    // Borrow the mapped rows and decode per access (see the LoadedSpans doc
    // for the fault economics that reversed the v6 eager decode).
    Ok(LoadedSpans {
        repr: SpansRepr::Mapped { payload, count },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_starts_round_trips() {
        let heap = DumpTaggedHeap {
            objects: vec![
                DumpHeapObject::Cons {
                    car: DumpValue::Int(1),
                    cdr: DumpValue::Nil,
                },
                DumpHeapObject::Float(3.125),
                DumpHeapObject::Free,
                DumpHeapObject::Vector(vec![DumpValue::Nil, DumpValue::True]),
                DumpHeapObject::Str {
                    data: DumpByteData::owned(b"hello".to_vec()),
                    size: 5,
                    size_byte: 5,
                    text_props: vec![],
                },
            ],
            mapped_cons: vec![Some(DumpConsSpan { offset: 0 }), None, None, None, None],
            mapped_floats: vec![None, Some(DumpFloatSpan { offset: 32 }), None, None, None],
            mapped_strings: vec![
                None,
                None,
                None,
                None,
                Some(DumpStringSpan {
                    offset: 48,
                    len: 16,
                }),
            ],
            mapped_veclikes: vec![
                None,
                None,
                None,
                Some(DumpVecLikeSpan {
                    offset: 64,
                    len: 24,
                }),
                None,
            ],
            mapped_slots: vec![
                None,
                None,
                None,
                Some(DumpSlotSpan {
                    offset: 88,
                    len: 16,
                }),
                None,
            ],
        };
        let bytes = build_object_starts(&heap).unwrap();
        let spans = load_object_starts(&bytes).unwrap();
        assert_eq!(spans.len(), 5);
        assert_eq!(spans.cons(0), Some(DumpConsSpan { offset: 0 }));
        assert!(spans.cons(1).is_none());
        assert_eq!(spans.float(1), Some(DumpFloatSpan { offset: 32 }));
        assert_eq!(
            spans.string(4),
            Some(DumpStringSpan {
                offset: 48,
                len: 16
            })
        );
        assert_eq!(
            spans.vectorlike(3),
            Some(DumpVecLikeSpan {
                offset: 64,
                len: 24
            })
        );
        assert_eq!(
            spans.slots(3),
            Some(DumpSlotSpan {
                offset: 88,
                len: 16
            })
        );
        assert_eq!(spans.get(2), LoadedObjectSpan::Unmapped);
    }
}
