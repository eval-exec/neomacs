//! Mapped heap payload extraction for pdump images.
//!
//! GNU pdumper keeps object headers in the mapped image and writes cold string
//! data later, then fixes string data pointers to the mapped cold region during
//! load.  Neomacs is migrating heap classes onto that same shape: mapped object
//! headers, mapped string bytes, mapped vectorlike slot arrays, and external GC
//! mark bits.

use super::DumpError;
use super::mmap_image::ImageRelocation;
use super::types::{
    DumpByteData, DumpConsSpan, DumpContextState, DumpFloatSpan, DumpHeapObject, DumpSlotSpan,
    DumpStringSpan, DumpTaggedHeap, DumpValue, DumpVecLikeSpan,
};
use super::value_fixups::RawValueFixup;
use crate::heap_types::LispString;
use crate::tagged::header::{
    ByteCodeObj, CharTableObj, ConsCell, FloatObj, GcHeader, HeapObjectKind, LambdaObj, MacroObj,
    MarkerObj, OverlayObj, RecordObj, StringObj, SubCharTableObj, VecLikeHeader, VecLikeType,
    VectorObj,
};
use crate::tagged::value::TaggedValue;
use bytemuck::{Pod, Zeroable};

const HEAP_PAYLOAD_ALIGN: usize = 8;
const TAG_CONS: u64 = 0b011;
const TAG_STRING: u64 = 0b100;
const TAG_VECLIKE: u64 = 0b101;
const TAG_FLOAT: u64 = 0b111;
const GC_HEADER_PADDING: usize = std::mem::size_of::<usize>() - 2;
const VECLIKE_HEADER_PADDING: usize = std::mem::size_of::<usize>() - 1;
const STRING_I64_PADDING: usize = 8 - std::mem::size_of::<usize>();
const STRING_TRAILING_PADDING: usize = 8 - std::mem::size_of::<usize>();

#[derive(Default)]
pub(crate) struct MappedHeapPayload {
    pub bytes: Vec<u8>,
    pub relocations: Vec<ImageRelocation>,
    pub value_fixups: Vec<RawValueFixup>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RawGcHeader {
    marked: u8,
    kind: u8,
    padding: [u8; GC_HEADER_PADDING],
    next: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RawFloatObj {
    header: RawGcHeader,
    value: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RawVecLikeHeader {
    header: RawGcHeader,
    type_tag: u8,
    padding: [u8; VECLIKE_HEADER_PADDING],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RawStringObj {
    header: RawGcHeader,
    size: usize,
    size_padding: [u8; STRING_I64_PADDING],
    size_byte: i64,
    intervals: usize,
    data: usize,
    storage: usize,
    trailing_padding: [u8; STRING_TRAILING_PADDING],
}

// The raw mirrors must reproduce the runtime object layouts byte for byte on
// EVERY target, including wasm32's 4-byte words: a mapped string's data word
// is relocated by offset, so a drifted field lands the pointer in the wrong
// word. Proving it here makes the wasm32/Android cross-checks part of the
// evidence instead of a dump-time debug assertion nobody runs on those hosts.
const _: () = {
    use std::mem::{align_of, offset_of, size_of};
    assert!(size_of::<RawGcHeader>() == size_of::<GcHeader>());
    assert!(align_of::<RawGcHeader>() == align_of::<GcHeader>());
    assert!(offset_of!(RawGcHeader, kind) == offset_of!(GcHeader, kind));
    assert!(offset_of!(RawGcHeader, next) == offset_of!(GcHeader, next));

    assert!(size_of::<RawFloatObj>() == size_of::<FloatObj>());
    assert!(align_of::<RawFloatObj>() == align_of::<FloatObj>());
    assert!(offset_of!(RawFloatObj, value) == offset_of!(FloatObj, value));

    assert!(size_of::<RawVecLikeHeader>() == size_of::<VecLikeHeader>());
    assert!(align_of::<RawVecLikeHeader>() == align_of::<VecLikeHeader>());
    assert!(offset_of!(RawVecLikeHeader, type_tag) == offset_of!(VecLikeHeader, type_tag));

    assert!(size_of::<RawStringObj>() == size_of::<StringObj>());
    assert!(align_of::<RawStringObj>() == align_of::<StringObj>());
    assert!(offset_of!(RawStringObj, size) == offset_of!(StringObj, data));
    assert!(
        offset_of!(RawStringObj, data)
            == offset_of!(StringObj, data) + LispString::data_field_offset()
    );
};

#[derive(Clone, Copy)]
pub(crate) struct MappedHeapView {
    ptr: *mut u8,
    len: usize,
    writable: bool,
}

/// See [`MappedHeapView::value_word_batch`].
pub(crate) struct ValueWordBatch {
    ptr: *mut u8,
    max: usize,
}

impl ValueWordBatch {
    #[inline]
    pub(crate) fn word_ptr(&self, offset: u64) -> Result<*mut usize, DumpError> {
        let start = usize::try_from(offset).map_err(|_| {
            DumpError::ImageFormatError("mapped value fixup offset overflows usize".into())
        })?;
        if start > self.max {
            return Err(DumpError::ImageFormatError(format!(
                "mapped value fixup at {start} exceeds heap word limit {}",
                self.max
            )));
        }
        if start % std::mem::align_of::<TaggedValue>() != 0 {
            return Err(DumpError::ImageFormatError(format!(
                "mapped value fixup offset {start} is not {}-byte aligned",
                std::mem::align_of::<TaggedValue>()
            )));
        }
        Ok(unsafe { self.ptr.add(start).cast::<usize>() })
    }
}

pub(crate) struct MappedBytes {
    pub ptr: *const u8,
    pub len: usize,
}

impl MappedHeapView {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn from_slice(bytes: &[u8]) -> Self {
        Self {
            ptr: bytes.as_ptr().cast_mut(),
            len: bytes.len(),
            writable: false,
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn from_mut_slice(bytes: &mut [u8]) -> Self {
        Self {
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
            writable: true,
        }
    }

    pub(crate) unsafe fn from_raw_parts(ptr: *mut u8, len: usize, writable: bool) -> Self {
        Self { ptr, len, writable }
    }

    pub(crate) fn bytes(self, data: &DumpByteData) -> Result<MappedBytes, DumpError> {
        match data {
            DumpByteData::Owned(_) | DumpByteData::StaticRoData { .. } => {
                Err(DumpError::ImageFormatError(
                    "owned byte payload requested from mapped heap view".to_string(),
                ))
            }
            DumpByteData::Mapped(span) => {
                let start = usize::try_from(span.offset).map_err(|_| {
                    DumpError::ImageFormatError("mapped heap offset overflows usize".into())
                })?;
                let len = usize::try_from(span.len).map_err(|_| {
                    DumpError::ImageFormatError("mapped heap length overflows usize".into())
                })?;
                let end = start.checked_add(len).ok_or_else(|| {
                    DumpError::ImageFormatError("mapped heap range overflow".into())
                })?;
                let terminator_end = end.checked_add(1).ok_or_else(|| {
                    DumpError::ImageFormatError("mapped heap terminator range overflow".into())
                })?;
                if terminator_end > self.len {
                    return Err(DumpError::ImageFormatError(format!(
                        "mapped heap range {start}..{terminator_end} exceeds heap section length {}",
                        self.len
                    )));
                }
                let terminator = unsafe { *self.ptr.add(end) };
                if terminator != 0 {
                    return Err(DumpError::ImageFormatError(format!(
                        "mapped heap string data at {start}..{end} is missing GNU trailing NUL"
                    )));
                }
                let ptr = if start < self.len {
                    unsafe { self.ptr.add(start).cast_const() }
                } else {
                    std::ptr::NonNull::<u8>::dangling().as_ptr()
                };
                Ok(MappedBytes { ptr, len })
            }
        }
    }

    /// Like [`Self::bytes`] but WITHOUT the GNU trailing-NUL contract:
    /// for spans that are interior slices of an object (the bytecode
    /// extras region), where the byte after the span belongs to the next
    /// object and can hold anything.
    pub(crate) fn bytes_unterminated(
        self,
        span: super::types::DumpByteSpan,
    ) -> Result<MappedBytes, DumpError> {
        let start = usize::try_from(span.offset).map_err(|_| {
            DumpError::ImageFormatError("mapped heap offset overflows usize".into())
        })?;
        let len = usize::try_from(span.len).map_err(|_| {
            DumpError::ImageFormatError("mapped heap length overflows usize".into())
        })?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| DumpError::ImageFormatError("mapped heap range overflow".into()))?;
        if end > self.len {
            return Err(DumpError::ImageFormatError(format!(
                "mapped heap range {start}..{end} exceeds heap section length {}",
                self.len
            )));
        }
        let ptr = if start < self.len {
            unsafe { self.ptr.add(start).cast_const() }
        } else {
            std::ptr::NonNull::<u8>::dangling().as_ptr()
        };
        Ok(MappedBytes { ptr, len })
    }

    pub(crate) fn slots_mut(
        self,
        span: DumpSlotSpan,
        expected_len: usize,
    ) -> Result<*mut TaggedValue, DumpError> {
        if !self.writable {
            return Err(DumpError::ImageFormatError(
                "mapped heap view is not writable".to_string(),
            ));
        }
        let slot_len = usize::try_from(span.len).map_err(|_| {
            DumpError::ImageFormatError("mapped slot span length overflows usize".into())
        })?;
        if slot_len != expected_len {
            return Err(DumpError::ImageFormatError(format!(
                "mapped slot span length {slot_len} does not match vector length {expected_len}"
            )));
        }
        let start = usize::try_from(span.offset).map_err(|_| {
            DumpError::ImageFormatError("mapped slot span offset overflows usize".into())
        })?;
        let byte_len = slot_len
            .checked_mul(std::mem::size_of::<TaggedValue>())
            .ok_or_else(|| {
                DumpError::ImageFormatError("mapped slot byte length overflow".into())
            })?;
        let end = start
            .checked_add(byte_len)
            .ok_or_else(|| DumpError::ImageFormatError("mapped slot span range overflow".into()))?;
        if end > self.len {
            return Err(DumpError::ImageFormatError(format!(
                "mapped slot span {start}..{end} exceeds heap section length {}",
                self.len
            )));
        }
        if start % std::mem::align_of::<TaggedValue>() != 0 {
            return Err(DumpError::ImageFormatError(format!(
                "mapped slot span offset {start} is not {}-byte aligned",
                std::mem::align_of::<TaggedValue>()
            )));
        }
        if slot_len == 0 {
            Ok(std::ptr::NonNull::<TaggedValue>::dangling().as_ptr())
        } else {
            Ok(unsafe { self.ptr.add(start).cast::<TaggedValue>() })
        }
    }

    pub(crate) fn cons_cell_mut(self, span: DumpConsSpan) -> Result<*mut ConsCell, DumpError> {
        if !self.writable {
            return Err(DumpError::ImageFormatError(
                "mapped heap view is not writable".to_string(),
            ));
        }
        let start = usize::try_from(span.offset).map_err(|_| {
            DumpError::ImageFormatError("mapped cons span offset overflows usize".into())
        })?;
        let end = start
            .checked_add(std::mem::size_of::<ConsCell>())
            .ok_or_else(|| DumpError::ImageFormatError("mapped cons span range overflow".into()))?;
        if end > self.len {
            return Err(DumpError::ImageFormatError(format!(
                "mapped cons span {start}..{end} exceeds heap section length {}",
                self.len
            )));
        }
        if start % std::mem::align_of::<ConsCell>() != 0 {
            return Err(DumpError::ImageFormatError(format!(
                "mapped cons span offset {start} is not {}-byte aligned",
                std::mem::align_of::<ConsCell>()
            )));
        }
        Ok(unsafe { self.ptr.add(start).cast::<ConsCell>() })
    }

    pub(crate) fn float_obj_mut(self, span: DumpFloatSpan) -> Result<*mut FloatObj, DumpError> {
        if !self.writable {
            return Err(DumpError::ImageFormatError(
                "mapped heap view is not writable".to_string(),
            ));
        }
        let start = usize::try_from(span.offset).map_err(|_| {
            DumpError::ImageFormatError("mapped float span offset overflows usize".into())
        })?;
        let end = start
            .checked_add(std::mem::size_of::<FloatObj>())
            .ok_or_else(|| {
                DumpError::ImageFormatError("mapped float span range overflow".into())
            })?;
        if end > self.len {
            return Err(DumpError::ImageFormatError(format!(
                "mapped float span {start}..{end} exceeds heap section length {}",
                self.len
            )));
        }
        if start % std::mem::align_of::<FloatObj>() != 0 {
            return Err(DumpError::ImageFormatError(format!(
                "mapped float span offset {start} is not {}-byte aligned",
                std::mem::align_of::<FloatObj>()
            )));
        }
        Ok(unsafe { self.ptr.add(start).cast::<FloatObj>() })
    }

    #[inline]
    pub(crate) fn typed_object_mut<T: 'static>(
        self,
        span: DumpVecLikeSpan,
        label: &'static str,
    ) -> Result<*mut T, DumpError> {
        if !self.writable {
            return Err(DumpError::ImageFormatError(
                "mapped heap view is not writable".to_string(),
            ));
        }
        let start = usize::try_from(span.offset).map_err(|_| {
            DumpError::ImageFormatError(format!("mapped {label} span offset overflows usize"))
        })?;
        let len = usize::try_from(span.len).map_err(|_| {
            DumpError::ImageFormatError(format!("mapped {label} span length overflows usize"))
        })?;
        let expected = std::mem::size_of::<T>();
        // Bytecode spans may carry a trailing extras region (see
        // `BytecodeExtras`): the typed object still sits at the span start
        // and the bounds/alignment checks below cover the full span.
        let extras_allowed = std::any::TypeId::of::<T>()
            == std::any::TypeId::of::<crate::tagged::header::ByteCodeObj>();
        if len != expected && !(extras_allowed && len > expected) {
            return Err(DumpError::ImageFormatError(format!(
                "mapped {label} span length {len} does not match object size {expected}"
            )));
        }
        let end = start.checked_add(len).ok_or_else(|| {
            DumpError::ImageFormatError(format!("mapped {label} span range overflow"))
        })?;
        if end > self.len {
            return Err(DumpError::ImageFormatError(format!(
                "mapped {label} span {start}..{end} exceeds heap section length {}",
                self.len
            )));
        }
        if start % std::mem::align_of::<T>() != 0 {
            return Err(DumpError::ImageFormatError(format!(
                "mapped {label} span offset {start} is not {}-byte aligned",
                std::mem::align_of::<T>()
            )));
        }
        Ok(unsafe { self.ptr.add(start).cast::<T>() })
    }

    pub(crate) fn veclike_header_mut(
        self,
        span: DumpVecLikeSpan,
    ) -> Result<*mut VecLikeHeader, DumpError> {
        if !self.writable {
            return Err(DumpError::ImageFormatError(
                "mapped heap view is not writable".to_string(),
            ));
        }
        let start = usize::try_from(span.offset).map_err(|_| {
            DumpError::ImageFormatError("mapped vectorlike span offset overflows usize".into())
        })?;
        let len = usize::try_from(span.len).map_err(|_| {
            DumpError::ImageFormatError("mapped vectorlike span length overflows usize".into())
        })?;
        if len < std::mem::size_of::<VecLikeHeader>() {
            return Err(DumpError::ImageFormatError(format!(
                "mapped vectorlike span length {len} is smaller than header size {}",
                std::mem::size_of::<VecLikeHeader>()
            )));
        }
        let end = start.checked_add(len).ok_or_else(|| {
            DumpError::ImageFormatError("mapped vectorlike span range overflow".into())
        })?;
        if end > self.len {
            return Err(DumpError::ImageFormatError(format!(
                "mapped vectorlike span {start}..{end} exceeds heap section length {}",
                self.len
            )));
        }
        if start % std::mem::align_of::<VecLikeHeader>() != 0 {
            return Err(DumpError::ImageFormatError(format!(
                "mapped vectorlike span offset {start} is not {}-byte aligned",
                std::mem::align_of::<VecLikeHeader>()
            )));
        }
        Ok(unsafe { self.ptr.add(start).cast::<VecLikeHeader>() })
    }

    pub(crate) fn veclike_type(self, span: DumpVecLikeSpan) -> Result<VecLikeType, DumpError> {
        let start = usize::try_from(span.offset).map_err(|_| {
            DumpError::ImageFormatError("mapped vectorlike span offset overflows usize".into())
        })?;
        let len = usize::try_from(span.len).map_err(|_| {
            DumpError::ImageFormatError("mapped vectorlike span length overflows usize".into())
        })?;
        if len < std::mem::size_of::<VecLikeHeader>() {
            return Err(DumpError::ImageFormatError(format!(
                "mapped vectorlike span length {len} is smaller than header size {}",
                std::mem::size_of::<VecLikeHeader>()
            )));
        }
        let end = start.checked_add(len).ok_or_else(|| {
            DumpError::ImageFormatError("mapped vectorlike span range overflow".into())
        })?;
        if end > self.len {
            return Err(DumpError::ImageFormatError(format!(
                "mapped vectorlike span {start}..{end} exceeds heap section length {}",
                self.len
            )));
        }
        let tag_offset = start + std::mem::size_of::<GcHeader>();
        let tag = unsafe { *self.ptr.add(tag_offset) };
        veclike_type_from_tag(tag)
    }

    #[inline]
    pub(crate) fn string_obj_mut(self, span: DumpStringSpan) -> Result<*mut StringObj, DumpError> {
        self.typed_object_mut::<StringObj>(
            DumpVecLikeSpan {
                offset: span.offset,
                len: span.len,
            },
            "string",
        )
    }

    pub(crate) fn write_value_word(self, offset: u64, value: TaggedValue) -> Result<(), DumpError> {
        if !self.writable {
            return Err(DumpError::ImageFormatError(
                "mapped heap view is not writable".to_string(),
            ));
        }
        let start = usize::try_from(offset).map_err(|_| {
            DumpError::ImageFormatError("mapped value fixup offset overflows usize".into())
        })?;
        let end = start
            .checked_add(std::mem::size_of::<TaggedValue>())
            .ok_or_else(|| {
                DumpError::ImageFormatError("mapped value fixup range overflow".into())
            })?;
        if end > self.len {
            return Err(DumpError::ImageFormatError(format!(
                "mapped value fixup {start}..{end} exceeds heap section length {}",
                self.len
            )));
        }
        if start % std::mem::align_of::<TaggedValue>() != 0 {
            return Err(DumpError::ImageFormatError(format!(
                "mapped value fixup offset {start} is not {}-byte aligned",
                std::mem::align_of::<TaggedValue>()
            )));
        }
        unsafe {
            self.ptr
                .add(start)
                .cast::<usize>()
                .write_unaligned(value.bits());
        }
        Ok(())
    }

    /// Validate a value-word offset ONCE and hand back the raw word pointer
    /// for a read-modify-write. The value-fixup loop used to pay the full
    /// validation twice per fixup (read_value_word then write_value_word on
    /// the same offset, ~130K fixups per load); the bounds/alignment check is
    /// still the memory-safety boundary (the load path skips the body
    /// checksum), it just runs once.
    /// Hoisted validation state for a batch of value-word fixups: the
    /// writable check and limit arithmetic run once, each entry pays two
    /// compares. Same safety boundary as [`Self::value_word_ptr`].
    pub(crate) fn value_word_batch(self) -> Result<ValueWordBatch, DumpError> {
        if !self.writable {
            return Err(DumpError::ImageFormatError(
                "mapped heap view is not writable".to_string(),
            ));
        }
        Ok(ValueWordBatch {
            ptr: self.ptr,
            max: self.len.saturating_sub(std::mem::size_of::<TaggedValue>()),
        })
    }

    pub(crate) fn value_word_ptr(self, offset: u64) -> Result<*mut usize, DumpError> {
        if !self.writable {
            return Err(DumpError::ImageFormatError(
                "mapped heap view is not writable".to_string(),
            ));
        }
        let start = usize::try_from(offset).map_err(|_| {
            DumpError::ImageFormatError("mapped value fixup offset overflows usize".into())
        })?;
        if start > self.len.saturating_sub(std::mem::size_of::<TaggedValue>()) {
            return Err(DumpError::ImageFormatError(format!(
                "mapped value fixup at {start} exceeds heap section length {}",
                self.len
            )));
        }
        if start % std::mem::align_of::<TaggedValue>() != 0 {
            return Err(DumpError::ImageFormatError(format!(
                "mapped value fixup offset {start} is not {}-byte aligned",
                std::mem::align_of::<TaggedValue>()
            )));
        }
        Ok(unsafe { self.ptr.add(start).cast::<usize>() })
    }

    #[inline]
    pub(crate) fn read_value_word(self, offset: u64) -> Result<usize, DumpError> {
        let start = usize::try_from(offset).map_err(|_| {
            DumpError::ImageFormatError("mapped value fixup offset overflows usize".into())
        })?;
        let end = start
            .checked_add(std::mem::size_of::<TaggedValue>())
            .ok_or_else(|| {
                DumpError::ImageFormatError("mapped value fixup range overflow".into())
            })?;
        if end > self.len {
            return Err(DumpError::ImageFormatError(format!(
                "mapped value fixup {start}..{end} exceeds heap section length {}",
                self.len
            )));
        }
        if start % std::mem::align_of::<TaggedValue>() != 0 {
            return Err(DumpError::ImageFormatError(format!(
                "mapped value fixup offset {start} is not {}-byte aligned",
                std::mem::align_of::<TaggedValue>()
            )));
        }
        Ok(unsafe { self.ptr.add(start).cast::<usize>().read_unaligned() })
    }
}

pub(crate) fn extract_mapped_heap_payloads(state: &mut DumpContextState) -> MappedHeapPayload {
    extract_tagged_heap_payloads(&mut state.tagged_heap, &mut state.obarray)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn clear_heap_metadata(heap: &mut DumpTaggedHeap) {
    heap.mapped_cons.clear();
    heap.mapped_floats.clear();
    heap.mapped_strings.clear();
    heap.mapped_veclikes.clear();
    heap.mapped_slots.clear();
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn rebuild_heap_metadata(heap: &mut DumpTaggedHeap) -> Result<(), DumpError> {
    let mut layout = HeapLayoutCursor::default();

    heap.mapped_cons.clear();
    heap.mapped_cons.resize(heap.objects.len(), None);
    heap.mapped_floats.clear();
    heap.mapped_floats.resize(heap.objects.len(), None);
    heap.mapped_strings.clear();
    heap.mapped_strings.resize(heap.objects.len(), None);
    heap.mapped_veclikes.clear();
    heap.mapped_veclikes.resize(heap.objects.len(), None);
    heap.mapped_slots.clear();
    heap.mapped_slots.resize(heap.objects.len(), None);

    let cons_count = heap
        .objects
        .iter()
        .filter(|object| matches!(object, DumpHeapObject::Cons { .. }))
        .count();
    let cons_base = layout.reserve_cons_cells(cons_count);
    let mut cons_index = 0usize;
    let float_count = heap
        .objects
        .iter()
        .filter(|object| matches!(object, DumpHeapObject::Float(_)))
        .count();
    let float_base = layout.reserve_float_objects(float_count);
    let mut float_index = 0usize;

    // Mirror of extract_tagged_heap_payloads' segregated struct-span run
    // (see the comment there); the lockstep test compares the two layouts.
    for (index, object) in heap.objects.iter().enumerate() {
        match object {
            DumpHeapObject::Vector(_) => {
                heap.mapped_veclikes[index] = Some(layout.reserve_typed_object::<VectorObj>());
            }
            DumpHeapObject::Lambda(_) => {
                heap.mapped_veclikes[index] = Some(layout.reserve_typed_object::<LambdaObj>());
            }
            DumpHeapObject::Macro(_) => {
                heap.mapped_veclikes[index] = Some(layout.reserve_typed_object::<MacroObj>());
            }
            DumpHeapObject::Record(_) => {
                heap.mapped_veclikes[index] = Some(layout.reserve_typed_object::<RecordObj>());
            }
            DumpHeapObject::Marker(_) => {
                heap.mapped_veclikes[index] = Some(layout.reserve_typed_object::<MarkerObj>());
            }
            DumpHeapObject::Overlay(_) => {
                heap.mapped_veclikes[index] = Some(layout.reserve_typed_object::<OverlayObj>());
            }
            _ => {}
        }
    }

    for (index, object) in heap.objects.iter().enumerate() {
        if matches!(object, DumpHeapObject::Cons { .. }) {
            let offset = cons_base.expect("non-zero cons count should reserve a mapped cons arena")
                + cons_index * std::mem::size_of::<ConsCell>();
            heap.mapped_cons[index] = Some(DumpConsSpan {
                offset: offset as u64,
            });
            cons_index += 1;
        }

        if matches!(object, DumpHeapObject::Float(_)) {
            let offset = float_base.expect("non-zero float count should reserve mapped floats")
                + float_index * std::mem::size_of::<FloatObj>();
            heap.mapped_floats[index] = Some(DumpFloatSpan {
                offset: offset as u64,
            });
            float_index += 1;
        }

        if let DumpHeapObject::ByteCode(function) = object {
            let extras = bytecode_extras_len(function);
            heap.mapped_veclikes[index] =
                Some(layout.reserve_typed_object_with_extras::<ByteCodeObj>(extras));
        }

        if let DumpHeapObject::Str { data, .. } = object {
            let span = layout.reserve_typed_object::<StringObj>();
            heap.mapped_strings[index] = Some(DumpStringSpan {
                offset: span.offset,
                len: span.len,
            });
            match data {
                DumpByteData::Owned(bytes) => {
                    layout.push_bytes_len(bytes.len());
                }
                DumpByteData::Mapped(span) => {
                    let rebuilt = layout.push_bytes_len(span.len as usize);
                    if rebuilt != *span {
                        return Err(DumpError::ImageFormatError(format!(
                            "mapped string data span mismatch for heap object {index}: dump has {span:?}, rebuilt {rebuilt:?}"
                        )));
                    }
                }
                DumpByteData::StaticRoData { .. } => {}
            }
        }

        let slot_count = match object {
            DumpHeapObject::Vector(slots)
            | DumpHeapObject::Lambda(slots)
            | DumpHeapObject::Macro(slots)
            | DumpHeapObject::Record(slots) => Some(slots.len()),
            _ => None,
        };
        if let Some(slot_count) = slot_count {
            heap.mapped_slots[index] = Some(layout.reserve_slots(slot_count));
        }
    }

    Ok(())
}

/// Fixed header of the self-describing "bytecode extras" region that sits
/// immediately after a mapped `ByteCodeObj` inside its (extended) veclike
/// span. Everything the object-extra descriptor used to carry rides here
/// instead: scalars in this header, then `n_required + n_optional` u32 dump
/// symbol ids, padding to 8, `n_extra_slots` raw value words (fixup-patched
/// like slot spans), then the docstring bytes. Only Gnu-instruction
/// bytecode uses the region; `Decoded` instruction vectors (test-only in
/// practice) stay descriptor-driven.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct BytecodeExtras {
    pub max_stack: u16,
    pub n_required: u16,
    pub n_optional: u16,
    pub flags: u16,
    pub rest_sym: u32,
    pub closure_slot_count: u32,
    pub n_extra_slots: u32,
    pub docstring_size: u32,
    pub docstring_size_byte: i64,
    /// v14: GNU byte-span offset RELATIVE to the owning object's veclike
    /// span offset (0 when absent — presence is BC_FLAG_HAS_GNU, not this
    /// field). Relative so a stub walker can locate the bytes from the
    /// object's own address without any load-time side table.
    pub gnu_rel: i64,
    pub gnu_len: u64,
    pub arglist_word: u64,
    pub env_word: u64,
    pub doc_form_word: u64,
    pub interactive_word: u64,
    /// v14: constants slot-span offset relative to the object span offset.
    pub const_rel: i64,
    /// v14: constants slot count (DumpSlotSpan::len semantics).
    pub const_count: u32,
    pub _pad: u32,
}

const _: () = assert!(std::mem::size_of::<BytecodeExtras>() == 96);

/// Walk every heap-reference word a LAZY bytecode stub's mapped regions
/// carry, WITHOUT materializing and without allocating: the GC's stub legs
/// (trace, collect, seed, verify) call this to reach children that live
/// only in the patched image — the four metadata value words, the
/// extra-slot words, and the constants slot span. Param SymId words are
/// deliberately not emitted (symbols are not heap-traced children), and
/// docstring/GNU bytes are not Values.
///
/// # Safety
/// `obj` must point at a live mapped `ByteCodeObj` whose extras region of
/// `extras_len` bytes is span-tail-adjacent (the dump reserves
/// object+extras as one span; `bytecode_extras_span` proves the same
/// arithmetic on the load side). The image words must already be PATCHED
/// (relocations + value fixups applied): the GC only runs after load
/// completes, which the caller doctrine in gc.rs guarantees.
pub(crate) unsafe fn for_each_stub_bytecode_child(
    obj: *const crate::tagged::header::ByteCodeObj,
    extras_len: usize,
    mut f: impl FnMut(TaggedValue),
) {
    if extras_len < std::mem::size_of::<BytecodeExtras>() {
        return;
    }
    let base = obj as *const u8;
    let extras_ptr = unsafe { base.add(std::mem::size_of::<crate::tagged::header::ByteCodeObj>()) };
    // Spans are 8-aligned (alignment-checked at map time), so the header
    // reads are plain unaligned-safe loads of a Pod struct.
    let header: BytecodeExtras =
        unsafe { std::ptr::read_unaligned(extras_ptr.cast::<BytecodeExtras>()) };
    let word_at = |ptr: *const u8| -> TaggedValue {
        TaggedValue::from_bits(unsafe { std::ptr::read_unaligned(ptr.cast::<usize>()) })
    };
    if header.flags & BC_FLAG_HAS_ARGLIST != 0 {
        f(TaggedValue::from_bits(header.arglist_word as usize));
    }
    if header.flags & BC_FLAG_HAS_ENV != 0 {
        f(TaggedValue::from_bits(header.env_word as usize));
    }
    if header.flags & BC_FLAG_HAS_DOC_FORM != 0 {
        f(TaggedValue::from_bits(header.doc_form_word as usize));
    }
    if header.flags & BC_FLAG_HAS_INTERACTIVE != 0 {
        f(TaggedValue::from_bits(header.interactive_word as usize));
    }
    let ids = header.n_required as usize + header.n_optional as usize;
    let extra_off = (std::mem::size_of::<BytecodeExtras>() + ids * 4 + 7) & !7;
    let mut cursor = unsafe { extras_ptr.add(extra_off) };
    for _ in 0..header.n_extra_slots {
        f(word_at(cursor));
        cursor = unsafe { cursor.add(8) };
    }
    if header.const_count > 0 {
        let mut slot = unsafe { base.offset(header.const_rel as isize) };
        for _ in 0..header.const_count {
            f(word_at(slot));
            slot = unsafe { slot.add(std::mem::size_of::<TaggedValue>()) };
        }
    }
}

pub(crate) const BC_FLAG_LEXICAL: u16 = 1;
pub(crate) const BC_FLAG_OPS_SEALED: u16 = 2;
pub(crate) const BC_FLAG_HAS_REST: u16 = 4;
pub(crate) const BC_FLAG_HAS_DOCSTRING: u16 = 8;
pub(crate) const BC_FLAG_HAS_ARGLIST: u16 = 16;
pub(crate) const BC_FLAG_HAS_ENV: u16 = 32;
pub(crate) const BC_FLAG_HAS_DOC_FORM: u16 = 64;
pub(crate) const BC_FLAG_HAS_INTERACTIVE: u16 = 128;
/// v14: the function carries a GNU byte region (`gnu_rel`/`gnu_len` valid).
/// Replaces the ambiguous `gnu_len > 0 || gnu_offset > 0` presence test — a
/// zero-length region at offset zero was indistinguishable from absence.
pub(crate) const BC_FLAG_HAS_GNU: u16 = 256;

/// Byte length of the extras region for one dump bytecode function
/// (0 when the function stays descriptor-driven).
pub(crate) fn bytecode_extras_len(function: &super::types::DumpByteCodeFunction) -> usize {
    if !matches!(
        function.instructions,
        super::types::DumpByteCodeInstructions::Gnu(_)
    ) {
        return 0;
    }
    let ids = function.params.required.len() + function.params.optional.len();
    let ids_bytes = (ids * 4 + 7) & !7;
    let doc_bytes = function.docstring.as_ref().map_or(0, |doc| doc.data.len());
    std::mem::size_of::<BytecodeExtras>() + ids_bytes + function.extra_slots.len() * 8 + doc_bytes
}

/// One fixed obarray symbol row (see `DumpObarray::plain_rows`).
pub(crate) const OBARRAY_ROW_SIZE: usize = 32;

/// The canonical bytes of `ByteCodeFunction::pdump_stub(extras_len)`, built
/// field by field in a ZEROED template — never a whole-struct copy, which
/// would memcpy the stack temporary's uninitialized padding into the image
/// (nondeterministic bytes, and a leak of dumper memory into a distributable
/// artifact). Per-leaf writes keep inter-field padding at the template's
/// concrete zeros. Every leaf value is process-independent: empty `Vec`s are
/// (dangling=align, 0, 0) build constants, `None`s are niche or
/// discriminant patterns, `Value::NIL` is 0, and `runtime` is `None` (the
/// point of `Option<Runtime>`). Cross-binary layout validity is enforced by
/// [`stub_layout_witness`] in the image header.
pub(crate) fn baked_stub_template(extras_len: usize) -> Box<[u8]> {
    use crate::emacs_core::bytecode::chunk::ByteCodeFunction;
    // Write one `None` into a pre-zeroed field, then zero back every byte
    // that `is_none` does not depend on. A plain whole-value write memcpys a
    // stack temporary whose None-payload bytes are UNDEFINED — measured to
    // vary by call site (the dump-time and load-time witness hashes
    // disagreed inside one process) — while "leave zeros" is wrong for
    // niched Options whose None encoding is nonzero (Option<Vec>'s niche
    // lives in cap's validity range). Greedy per-byte minimization keeps
    // exactly the compiler-written niche bytes (deterministic constants)
    // and canonicalizes everything else to zero, for ANY layout rustc
    // picks.
    unsafe fn write_canonical_none<T>(dst: *mut T, none: T, is_none: impl Fn(&T) -> bool) {
        unsafe {
            std::ptr::write(dst, none);
            let bytes = dst.cast::<u8>();
            for i in 0..std::mem::size_of::<T>() {
                let saved = bytes.add(i).read();
                if saved == 0 {
                    continue;
                }
                bytes.add(i).write(0);
                if !is_none(&*dst) {
                    bytes.add(i).write(saved);
                }
            }
            assert!(is_none(&*dst), "canonicalized None must still read as None");
        }
    }

    let mut slot = std::mem::MaybeUninit::<ByteCodeFunction>::zeroed();
    let p = slot.as_mut_ptr();
    unsafe {
        use std::ptr::addr_of_mut;
        // Direct writes only for fields whose representation is fully
        // defined: Vecs (three words, no padding), the niched LispValueVec
        // (one Vec), bools and usize. Zero scalars (source_id,
        // stack_verified, max_stack, arglist = Value::NIL = 0, lexical) and
        // the pointer-niched Nones (runtime, lazy_gnu_code: None = null
        // word) stay at the template's zeros.
        addr_of_mut!((*p).ops).write(Vec::new());
        addr_of_mut!((*p).ops_sealed).write(true);
        addr_of_mut!((*p).constants).write(Vec::new().into());
        addr_of_mut!((*p).params.required).write(Vec::new());
        addr_of_mut!((*p).params.optional).write(Vec::new());
        addr_of_mut!((*p).closure_slot_count).write(extras_len);
        addr_of_mut!((*p).extra_slots).write(Vec::new());
        write_canonical_none(addr_of_mut!((*p).params.rest), None, Option::is_none);
        write_canonical_none(addr_of_mut!((*p).env), None, Option::is_none);
        write_canonical_none(
            addr_of_mut!((*p).gnu_byte_offset_map),
            None,
            Option::is_none,
        );
        write_canonical_none(addr_of_mut!((*p).gnu_bytecode_bytes), None, Option::is_none);
        write_canonical_none(addr_of_mut!((*p).docstring), None, Option::is_none);
        write_canonical_none(addr_of_mut!((*p).doc_form), None, Option::is_none);
        write_canonical_none(addr_of_mut!((*p).interactive), None, Option::is_none);

        // Full semantic readback: the template bytes must reconstruct the
        // exact stub `pdump_stub(extras_len)` would build, field by field.
        // This is the runtime proof behind every zeros-are-None assumption
        // above, and it runs on every bake (trivial next to the image
        // checksum), so no compiler/layout change can bake invalid bytes.
        let stub = &*p;
        assert!(
            stub.is_pdump_stub(),
            "baked template must read back as a stub"
        );
        assert_eq!(stub.source_id, 0);
        assert!(!stub.stack_verified);
        assert!(stub.constants.as_slice().is_empty());
        assert_eq!(stub.max_stack, 0);
        assert!(stub.params.required.is_empty() && stub.params.optional.is_empty());
        assert!(stub.params.rest.is_none());
        assert_eq!(
            stub.arglist.bits(),
            crate::emacs_core::value::Value::NIL.bits()
        );
        assert!(!stub.lexical);
        assert!(stub.env.is_none());
        assert!(stub.gnu_byte_offset_map.is_none());
        assert!(stub.gnu_bytecode_bytes.is_none());
        assert!(stub.docstring.is_none());
        assert!(stub.doc_form.is_none());
        assert!(stub.interactive.is_none());
        assert_eq!(stub.closure_slot_count, extras_len);
        assert!(stub.extra_slots.is_empty());
        #[cfg(feature = "jit")]
        assert!(stub.runtime.is_none());
        assert!(stub.lazy_gnu_code.is_none());

        // All fields own nothing, so the template needs no drop. The bytes
        // are copied out because the image buffer is not 8-aligned at dump
        // time (the mapped OFFSET is aligned, not the builder Vec).
        std::slice::from_raw_parts(p.cast::<u8>(), std::mem::size_of::<ByteCodeFunction>()).into()
    }
}

/// TOTAL layout witness for baked stubs: an FNV-1a hash of the canonical
/// template bytes, stored in the image header and recomputed at load. Any
/// repr(Rust) layout drift between the dumping and loading binaries — field
/// order, Vec ptr/len/cap order, Option niches, alignment-derived dangling
/// pointers — changes some byte of the template, so a mismatched image is
/// REJECTED cleanly instead of wild-freeing at the publish-site drop. This
/// matters for the one binding hole: unstamped dev builds share a
/// placeholder fingerprint, so the header fingerprint check cannot tell two
/// dev binaries apart. Building the template twice and asserting equality
/// turns any bake nondeterminism into a loud dump-time failure instead of a
/// silent never-validating cache.
pub(crate) fn stub_layout_witness() -> u64 {
    const SENTINEL_EXTRAS_LEN: usize = 0xC0DE;
    let a = baked_stub_template(SENTINEL_EXTRAS_LEN);
    let b = baked_stub_template(SENTINEL_EXTRAS_LEN);
    assert_eq!(
        a, b,
        "stub template bake must be byte-deterministic (an uninitialized \
         byte reached the template)"
    );
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in a.iter() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

fn extract_tagged_heap_payloads(
    heap: &mut DumpTaggedHeap,
    obarray: &mut super::types::DumpObarray,
) -> MappedHeapPayload {
    let mut builder = MappedHeapBuilder::default();

    heap.mapped_cons.clear();
    heap.mapped_cons.resize(heap.objects.len(), None);
    heap.mapped_floats.clear();
    heap.mapped_floats.resize(heap.objects.len(), None);
    heap.mapped_strings.clear();
    heap.mapped_strings.resize(heap.objects.len(), None);
    heap.mapped_veclikes.clear();
    heap.mapped_veclikes.resize(heap.objects.len(), None);
    heap.mapped_slots.clear();
    heap.mapped_slots.resize(heap.objects.len(), None);

    let cons_count = heap
        .objects
        .iter()
        .filter(|object| matches!(object, DumpHeapObject::Cons { .. }))
        .count();
    let cons_base = builder.reserve_cons_cells(cons_count);
    let mut cons_index = 0usize;
    let float_count = heap
        .objects
        .iter()
        .filter(|object| matches!(object, DumpHeapObject::Float(_)))
        .count();
    let float_base = builder.reserve_float_objects(float_count);
    let mut float_index = 0usize;

    // Segregate the placeholder-written struct spans into ONE contiguous
    // run before everything else, like the cons and float arenas above.
    // The loader ptr::writes each of these structs at every startup;
    // interleaved with read-only slot/string payloads those writes COW'd
    // ~1,251 scattered image pages, clustered they touch ~85. Nothing
    // requires struct/slot adjacency for these types (spans are
    // self-describing per-object ObjectStarts data). ByteCode is NOT here:
    // its span must stay adjacent to its extras tail, and since pdump v15
    // its stub bytes are baked — the loader writes nothing there anyway.
    for (index, object) in heap.objects.iter().enumerate() {
        match object {
            DumpHeapObject::Vector(_) => {
                heap.mapped_veclikes[index] = Some(builder.reserve_typed_object::<VectorObj>());
            }
            DumpHeapObject::Lambda(_) => {
                heap.mapped_veclikes[index] = Some(builder.reserve_typed_object::<LambdaObj>());
            }
            DumpHeapObject::Macro(_) => {
                heap.mapped_veclikes[index] = Some(builder.reserve_typed_object::<MacroObj>());
            }
            DumpHeapObject::Record(_) => {
                heap.mapped_veclikes[index] = Some(builder.reserve_typed_object::<RecordObj>());
            }
            DumpHeapObject::Marker(_) => {
                heap.mapped_veclikes[index] = Some(builder.reserve_typed_object::<MarkerObj>());
            }
            DumpHeapObject::Overlay(_) => {
                heap.mapped_veclikes[index] = Some(builder.reserve_typed_object::<OverlayObj>());
            }
            DumpHeapObject::CharTable { .. } => {
                heap.mapped_veclikes[index] = Some(builder.reserve_typed_object::<CharTableObj>());
            }
            DumpHeapObject::SubCharTable { .. } => {
                heap.mapped_veclikes[index] =
                    Some(builder.reserve_typed_object::<SubCharTableObj>());
            }
            _ => {}
        }
    }

    for (index, object) in heap.objects.iter_mut().enumerate() {
        if matches!(object, DumpHeapObject::Cons { .. }) {
            let offset = cons_base.expect("non-zero cons count should reserve a mapped cons arena")
                + cons_index * std::mem::size_of::<ConsCell>();
            heap.mapped_cons[index] = Some(DumpConsSpan {
                offset: offset as u64,
            });
            cons_index += 1;
        }

        if matches!(object, DumpHeapObject::Float(_)) {
            let offset = float_base.expect("non-zero float count should reserve mapped floats")
                + float_index * std::mem::size_of::<FloatObj>();
            heap.mapped_floats[index] = Some(DumpFloatSpan {
                offset: offset as u64,
            });
            float_index += 1;
        }

        if let DumpHeapObject::ByteCode(function) = object {
            let extras = bytecode_extras_len(function);
            heap.mapped_veclikes[index] =
                Some(builder.reserve_typed_object_with_extras::<ByteCodeObj>(extras));
        }

        if let DumpHeapObject::Str { data, .. } = object {
            let span = builder.reserve_typed_object::<StringObj>();
            heap.mapped_strings[index] = Some(DumpStringSpan {
                offset: span.offset,
                len: span.len,
            });
            match data {
                DumpByteData::Owned(bytes) => {
                    let owned = std::mem::take(bytes);
                    let span = builder.push_bytes(&owned);
                    *data = DumpByteData::mapped(span.offset, span.len);
                }
                DumpByteData::Mapped(_) | DumpByteData::StaticRoData { .. } => {}
            }
        }

        // GNU bytecode bytes ride the image the same way string payloads do;
        // the loader aliases the span instead of copying a Vec per function.
        if let DumpHeapObject::ByteCode(function) = object
            && let super::types::DumpByteCodeInstructions::Gnu(data) = &mut function.instructions
            && let DumpByteData::Owned(bytes) = data
        {
            let owned = std::mem::take(bytes);
            let span = builder.push_bytes(&owned);
            *data = DumpByteData::mapped(span.offset, span.len);
        }

        let slot_count = match object {
            DumpHeapObject::Vector(slots)
            | DumpHeapObject::Lambda(slots)
            | DumpHeapObject::Macro(slots)
            | DumpHeapObject::Record(slots) => Some(slots.len()),
            // Char-table trailing storage: the four fixed slots and the
            // 64 top-level contents live INLINE in the CharTableObj span;
            // only `extras` needs external slot storage. Sub-char-table
            // contents are the external storage.
            DumpHeapObject::CharTable { extras, .. } => Some(extras.len()),
            DumpHeapObject::SubCharTable { contents, .. } => Some(contents.len()),
            // Bytecode constant pools are mapped as bare slot spans (no
            // veclike header — the object stays descriptor-driven); the
            // loader aliases them as LispValueVec::mapped instead of
            // decoding an owned Vec per function.
            DumpHeapObject::ByteCode(function) => Some(function.constants.len()),
            _ => None,
        };
        if let Some(slot_count) = slot_count {
            heap.mapped_slots[index] = Some(builder.reserve_slots(slot_count));
        }
    }

    // Obarray symbol rows: Plain and Varalias symbols become fixed rows in
    // the heap image whose value words go through the standard fixup
    // classes; Localized/Forwarded stay in the residual per-symbol path.
    let (row_symbols, residual): (Vec<_>, Vec<_>) = std::mem::take(&mut obarray.symbols)
        .into_iter()
        .partition(|(_, data)| {
            matches!(
                data.val,
                super::types::DumpSymbolVal::Plain(_) | super::types::DumpSymbolVal::Alias(_)
            )
        });
    obarray.symbols = residual;
    let rows_base = builder.reserve_obarray_rows(row_symbols.len());

    builder.populate_raw_heap_payloads(heap);
    if let Some(base) = rows_base {
        builder.populate_obarray_rows(base, &row_symbols, heap);
        obarray.plain_rows = Some((base as u64, row_symbols.len() as u64));
    } else {
        obarray.plain_rows = None;
    }
    builder.finish()
}

#[derive(Default)]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
struct HeapLayoutCursor {
    offset: usize,
}

impl HeapLayoutCursor {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn push_bytes_len(&mut self, payload_len: usize) -> super::types::DumpByteSpan {
        self.align_to(HEAP_PAYLOAD_ALIGN);
        let offset = self.offset;
        self.offset += payload_len + 1;
        super::types::DumpByteSpan {
            offset: offset as u64,
            len: payload_len as u64,
        }
    }

    fn reserve_slots(&mut self, slot_count: usize) -> DumpSlotSpan {
        let align = std::mem::align_of::<TaggedValue>().max(HEAP_PAYLOAD_ALIGN);
        self.align_to(align);
        let offset = self.offset;
        let byte_len = slot_count.saturating_mul(std::mem::size_of::<TaggedValue>());
        if byte_len == 0 {
            self.offset += std::mem::size_of::<TaggedValue>();
        } else {
            self.offset += byte_len;
        }
        DumpSlotSpan {
            offset: offset as u64,
            len: slot_count as u64,
        }
    }

    fn reserve_cons_cells(&mut self, cons_count: usize) -> Option<usize> {
        if cons_count == 0 {
            return None;
        }
        let align = std::mem::align_of::<ConsCell>().max(HEAP_PAYLOAD_ALIGN);
        self.align_to(align);
        let offset = self.offset;
        self.offset += cons_count * std::mem::size_of::<ConsCell>();
        Some(offset)
    }

    fn reserve_float_objects(&mut self, float_count: usize) -> Option<usize> {
        if float_count == 0 {
            return None;
        }
        let align = std::mem::align_of::<FloatObj>().max(HEAP_PAYLOAD_ALIGN);
        self.align_to(align);
        let offset = self.offset;
        self.offset += float_count * std::mem::size_of::<FloatObj>();
        Some(offset)
    }

    fn reserve_typed_object<T>(&mut self) -> DumpVecLikeSpan {
        self.reserve_typed_object_with_extras::<T>(0)
    }

    /// Reserve a typed object plus `extras` trailing bytes inside ONE span
    /// (the span length above `size_of::<T>()` is the extras region — see
    /// `BytecodeExtras`).
    fn reserve_typed_object_with_extras<T>(&mut self, extras: usize) -> DumpVecLikeSpan {
        let align = std::mem::align_of::<T>().max(HEAP_PAYLOAD_ALIGN);
        self.align_to(align);
        let offset = self.offset;
        let len = std::mem::size_of::<T>() + extras;
        self.offset += len;
        DumpVecLikeSpan {
            offset: offset as u64,
            len: len as u64,
        }
    }

    fn align_to(&mut self, align: usize) {
        self.offset += align_padding(self.offset, align);
    }
}

#[derive(Default)]
struct MappedHeapBuilder {
    bytes: Vec<u8>,
    relocations: Vec<ImageRelocation>,
    value_fixups: Vec<RawValueFixup>,
}

impl MappedHeapBuilder {
    fn reserve_obarray_rows(&mut self, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let align = HEAP_PAYLOAD_ALIGN.max(8);
        let padding = align_padding(self.bytes.len(), align);
        self.bytes.resize(self.bytes.len() + padding, 0);
        let offset = self.bytes.len();
        self.bytes.resize(offset + count * OBARRAY_ROW_SIZE, 0);
        Some(offset)
    }

    /// Write the obarray symbol rows (see `DumpObarray::plain_rows`). Runs
    /// after `populate_raw_heap_payloads` so `self.bytes` is fully sized.
    fn populate_obarray_rows(
        &mut self,
        base: usize,
        rows: &[(super::types::DumpSymId, super::types::DumpSymbolData)],
        heap: &DumpTaggedHeap,
    ) {
        use super::types::{DumpSymbolVal, DumpValue};
        for (i, (sym, data)) in rows.iter().enumerate() {
            let offset = base + i * OBARRAY_ROW_SIZE;
            let mut head = [0u8; 8];
            head[..4].copy_from_slice(&sym.0.to_le_bytes());
            head[4] = data.redirect;
            head[5] = data.trapped_write;
            head[6] = data.interned;
            head[7] = u8::from(data.declared_special);
            self.write_bytes(offset, &head);
            let val = match &data.val {
                DumpSymbolVal::Plain(v) => v.clone(),
                DumpSymbolVal::Alias(target) => DumpValue::Symbol(*target),
                _ => unreachable!("row partition admits only Plain and Alias"),
            };
            self.write_dump_value_word(offset + 8, &val, heap);
            self.write_dump_value_word(offset + 16, &data.function, heap);
            self.write_dump_value_word(offset + 24, &data.plist, heap);
        }
    }

    fn push_bytes(&mut self, payload: &[u8]) -> super::types::DumpByteSpan {
        let padding = align_padding(self.bytes.len(), HEAP_PAYLOAD_ALIGN);
        self.bytes.resize(self.bytes.len() + padding, 0);
        let offset = self.bytes.len();
        self.bytes.extend_from_slice(payload);
        self.bytes.push(0);
        super::types::DumpByteSpan {
            offset: offset as u64,
            len: payload.len() as u64,
        }
    }

    fn finish(self) -> MappedHeapPayload {
        MappedHeapPayload {
            bytes: self.bytes,
            relocations: self.relocations,
            value_fixups: self.value_fixups,
        }
    }

    fn reserve_slots(&mut self, slot_count: usize) -> DumpSlotSpan {
        let align = std::mem::align_of::<TaggedValue>().max(HEAP_PAYLOAD_ALIGN);
        let padding = align_padding(self.bytes.len(), align);
        self.bytes.resize(self.bytes.len() + padding, 0);
        let offset = self.bytes.len();
        let byte_len = slot_count.saturating_mul(std::mem::size_of::<TaggedValue>());
        if byte_len == 0 {
            self.bytes
                .resize(self.bytes.len() + std::mem::size_of::<TaggedValue>(), 0);
        } else {
            self.bytes.resize(self.bytes.len() + byte_len, 0);
        }
        DumpSlotSpan {
            offset: offset as u64,
            len: slot_count as u64,
        }
    }

    fn reserve_cons_cells(&mut self, cons_count: usize) -> Option<usize> {
        if cons_count == 0 {
            return None;
        }
        let align = std::mem::align_of::<ConsCell>().max(HEAP_PAYLOAD_ALIGN);
        let padding = align_padding(self.bytes.len(), align);
        self.bytes.resize(self.bytes.len() + padding, 0);
        let offset = self.bytes.len();
        self.bytes
            .resize(offset + cons_count * std::mem::size_of::<ConsCell>(), 0);
        Some(offset)
    }

    fn reserve_float_objects(&mut self, float_count: usize) -> Option<usize> {
        if float_count == 0 {
            return None;
        }
        let align = std::mem::align_of::<FloatObj>().max(HEAP_PAYLOAD_ALIGN);
        let padding = align_padding(self.bytes.len(), align);
        self.bytes.resize(self.bytes.len() + padding, 0);
        let offset = self.bytes.len();
        self.bytes
            .resize(offset + float_count * std::mem::size_of::<FloatObj>(), 0);
        Some(offset)
    }

    fn reserve_typed_object<T>(&mut self) -> DumpVecLikeSpan {
        self.reserve_typed_object_with_extras::<T>(0)
    }

    /// See the layout cursor's twin: one span covering the object plus
    /// `extras` trailing bytes.
    fn reserve_typed_object_with_extras<T>(&mut self, extras: usize) -> DumpVecLikeSpan {
        let align = std::mem::align_of::<T>().max(HEAP_PAYLOAD_ALIGN);
        let padding = align_padding(self.bytes.len(), align);
        self.bytes.resize(self.bytes.len() + padding, 0);
        let offset = self.bytes.len();
        let len = std::mem::size_of::<T>() + extras;
        self.bytes.resize(offset + len, 0);
        DumpVecLikeSpan {
            offset: offset as u64,
            len: len as u64,
        }
    }

    fn populate_raw_heap_payloads(&mut self, heap: &DumpTaggedHeap) {
        for (index, object) in heap.objects.iter().enumerate() {
            match object {
                DumpHeapObject::Cons { car, cdr } => {
                    if let Some(span) = heap.mapped_cons.get(index).copied().flatten() {
                        let offset = span.offset as usize;
                        self.write_dump_value_word(offset, car, heap);
                        self.write_dump_value_word(
                            offset + std::mem::size_of::<TaggedValue>(),
                            cdr,
                            heap,
                        );
                    }
                }
                DumpHeapObject::Float(value) => {
                    if let Some(span) = heap.mapped_floats.get(index).copied().flatten() {
                        self.write_raw_float_obj(span.offset as usize, *value);
                    }
                }
                DumpHeapObject::Vector(slots)
                | DumpHeapObject::Lambda(slots)
                | DumpHeapObject::Macro(slots)
                | DumpHeapObject::Record(slots) => {
                    if let Some(span) = heap.mapped_veclikes.get(index).copied().flatten() {
                        let type_tag = match object {
                            DumpHeapObject::Vector(_) => VecLikeType::Vector,
                            DumpHeapObject::Lambda(_) => VecLikeType::Lambda,
                            DumpHeapObject::Macro(_) => VecLikeType::Macro,
                            DumpHeapObject::Record(_) => VecLikeType::Record,
                            _ => unreachable!(),
                        };
                        self.write_raw_veclike_header(span.offset as usize, type_tag);
                    }
                    if let Some(span) = heap.mapped_slots.get(index).copied().flatten() {
                        let mut offset = span.offset as usize;
                        for slot in slots {
                            self.write_dump_value_word(offset, slot, heap);
                            offset += std::mem::size_of::<TaggedValue>();
                        }
                    }
                }
                DumpHeapObject::ByteCode(function) => {
                    if let Some(span) = heap.mapped_veclikes.get(index).copied().flatten() {
                        self.write_raw_veclike_header(span.offset as usize, VecLikeType::ByteCode);
                        let base = span.offset as usize + std::mem::size_of::<ByteCodeObj>();
                        if (span.len as usize) > std::mem::size_of::<ByteCodeObj>() {
                            // Extras-bearing function: bake the lazy stub's
                            // bytes into the struct region so the loader
                            // writes NOTHING into this span (the per-object
                            // ptr::write used to COW ~1,187 image pages per
                            // startup). closure_slot_count carries the extras
                            // length, exactly as pdump_stub would set it.
                            let extras_len = span.len as usize - std::mem::size_of::<ByteCodeObj>();
                            self.write_baked_stub(
                                span.offset as usize + std::mem::offset_of!(ByteCodeObj, data),
                                extras_len,
                            );
                            let slots_span = heap.mapped_slots.get(index).copied().flatten();
                            let end = self.write_bytecode_extras(
                                base,
                                span.offset,
                                slots_span,
                                function,
                                heap,
                            );
                            assert_eq!(
                                end,
                                span.offset as usize + span.len as usize,
                                "bytecode extras write must exactly fill the reserved span \
                                 (object {index})"
                            );
                        }
                        // Extras-less spans stay zero-filled: the loader's
                        // descriptor-era placeholder write still covers them.
                    }
                    if let Some(span) = heap.mapped_slots.get(index).copied().flatten() {
                        let mut offset = span.offset as usize;
                        for slot in &function.constants {
                            self.write_dump_value_word(offset, slot, heap);
                            offset += std::mem::size_of::<TaggedValue>();
                        }
                    }
                }
                DumpHeapObject::Marker(_) | DumpHeapObject::Overlay(_) => {
                    if let Some(span) = heap.mapped_veclikes.get(index).copied().flatten() {
                        let type_tag = match object {
                            DumpHeapObject::Marker(_) => VecLikeType::Marker,
                            DumpHeapObject::Overlay(_) => VecLikeType::Overlay,
                            _ => unreachable!(),
                        };
                        self.write_raw_veclike_header(span.offset as usize, type_tag);
                    }
                }
                DumpHeapObject::CharTable {
                    defalt,
                    parent,
                    purpose,
                    ascii,
                    contents,
                    extras,
                } => {
                    if let Some(span) = heap.mapped_veclikes.get(index).copied().flatten() {
                        let base = span.offset as usize;
                        self.write_raw_veclike_header(base, VecLikeType::CharTable);
                        // The four fixed slots and the 64 top-level contents
                        // are INLINE TaggedValue fields of CharTableObj: bake
                        // their value words (immediates directly, symbols and
                        // heap refs via the fixup machinery, exactly like
                        // mapped cons car/cdr words).
                        self.write_dump_value_word(
                            base + std::mem::offset_of!(CharTableObj, defalt),
                            defalt,
                            heap,
                        );
                        self.write_dump_value_word(
                            base + std::mem::offset_of!(CharTableObj, parent),
                            parent,
                            heap,
                        );
                        self.write_dump_value_word(
                            base + std::mem::offset_of!(CharTableObj, purpose),
                            purpose,
                            heap,
                        );
                        self.write_dump_value_word(
                            base + std::mem::offset_of!(CharTableObj, ascii),
                            ascii,
                            heap,
                        );
                        let mut offset = base + std::mem::offset_of!(CharTableObj, contents);
                        for slot in contents {
                            self.write_dump_value_word(offset, slot, heap);
                            offset += std::mem::size_of::<TaggedValue>();
                        }
                    }
                    if let Some(span) = heap.mapped_slots.get(index).copied().flatten() {
                        let mut offset = span.offset as usize;
                        for slot in extras {
                            self.write_dump_value_word(offset, slot, heap);
                            offset += std::mem::size_of::<TaggedValue>();
                        }
                    }
                }
                DumpHeapObject::SubCharTable {
                    depth,
                    min_char,
                    contents,
                } => {
                    if let Some(span) = heap.mapped_veclikes.get(index).copied().flatten() {
                        let base = span.offset as usize;
                        self.write_raw_veclike_header(base, VecLikeType::SubCharTable);
                        // depth/min_char are plain i32 fields; live objects
                        // guarantee the range, so bake them raw.
                        self.write_bytes(
                            base + std::mem::offset_of!(SubCharTableObj, depth),
                            &(*depth as i32).to_ne_bytes(),
                        );
                        self.write_bytes(
                            base + std::mem::offset_of!(SubCharTableObj, min_char),
                            &(*min_char as i32).to_ne_bytes(),
                        );
                    }
                    if let Some(span) = heap.mapped_slots.get(index).copied().flatten() {
                        let mut offset = span.offset as usize;
                        for slot in contents {
                            self.write_dump_value_word(offset, slot, heap);
                            offset += std::mem::size_of::<TaggedValue>();
                        }
                    }
                }
                DumpHeapObject::Str {
                    data,
                    size,
                    size_byte,
                    ..
                } => {
                    if let Some(span) = heap.mapped_strings.get(index).copied().flatten() {
                        self.write_raw_string_obj(span.offset as usize, *size, *size_byte, data);
                    }
                }
                _ => {}
            }
        }
        // The GNU trailing NUL after every mapped string-data span is
        // load-validated; catch any writer overrunning into a neighbor at
        // dump time, where the colliding object index is still known.
        for (index, object) in heap.objects.iter().enumerate() {
            if let DumpHeapObject::Str {
                data: super::types::DumpByteData::Mapped(span),
                ..
            } = object
            {
                let nul = span.offset as usize + span.len as usize;
                assert_eq!(
                    self.bytes.get(nul).copied(),
                    Some(0),
                    "mapped string data for object {index} at {}..{} lost its trailing NUL",
                    span.offset,
                    nul
                );
            }
        }
    }

    fn write_raw_float_obj(&mut self, offset: usize, value: f64) {
        let raw = RawFloatObj {
            header: RawGcHeader {
                marked: 0,
                kind: u8::from(HeapObjectKind::Float),
                padding: [0; GC_HEADER_PADDING],
                next: 0,
            },
            value,
        };
        self.write_bytes(offset, bytemuck::bytes_of(&raw));
    }

    fn write_raw_veclike_header(&mut self, offset: usize, type_tag: VecLikeType) {
        let raw = RawVecLikeHeader {
            header: RawGcHeader {
                marked: 0,
                kind: u8::from(HeapObjectKind::VecLike),
                padding: [0; GC_HEADER_PADDING],
                next: 0,
            },
            type_tag: u8::from(type_tag),
            padding: [0; VECLIKE_HEADER_PADDING],
        };
        self.write_bytes(offset, bytemuck::bytes_of(&raw));
    }

    /// Bake the exact bytes of `ByteCodeFunction::pdump_stub(extras_len)`
    /// into the (pre-zeroed) struct region of a bytecode span, so the loader
    /// writes NOTHING there — the mapped bytes ARE the stub.
    fn write_baked_stub(&mut self, offset: usize, extras_len: usize) {
        self.write_bytes(offset, &baked_stub_template(extras_len));
    }

    fn write_raw_string_obj(
        &mut self,
        offset: usize,
        size: usize,
        size_byte: i64,
        data: &DumpByteData,
    ) {
        let data_word = match data {
            DumpByteData::Mapped(span) => {
                let data_field_offset = offset
                    + std::mem::offset_of!(StringObj, data)
                    + LispString::data_field_offset();
                self.relocations.push(ImageRelocation {
                    location_offset: data_field_offset as u64,
                    addend: 0,
                });
                usize::try_from(span.offset)
                    .expect("mapped string byte offset should fit in a word")
            }
            DumpByteData::StaticRoData { .. } => 0,
            DumpByteData::Owned(_) => {
                unreachable!("owned string bytes should be extracted before raw heap population")
            }
        };
        let raw = RawStringObj {
            header: RawGcHeader {
                marked: 0,
                kind: u8::from(HeapObjectKind::String),
                padding: [0; GC_HEADER_PADDING],
                next: 0,
            },
            size,
            size_padding: [0; STRING_I64_PADDING],
            size_byte,
            intervals: 0,
            data: data_word,
            storage: 0,
            trailing_padding: [0; STRING_TRAILING_PADDING],
        };
        self.write_bytes(offset, bytemuck::bytes_of(&raw));
    }

    /// Fill one bytecode function's extras region (see [`BytecodeExtras`]).
    /// Metadata Values go through `write_dump_value_word`, so heap-ref words
    /// register fixups exactly like slot spans do.
    fn write_bytecode_extras(
        &mut self,
        base: usize,
        obj_offset: u64,
        slots_span: Option<super::types::DumpSlotSpan>,
        function: &super::types::DumpByteCodeFunction,
        heap: &DumpTaggedHeap,
    ) -> usize {
        use super::types::DumpByteCodeInstructions;
        // v14: presence is a FLAG; the offsets are object-relative so a lazy
        // stub can locate its regions from its own address alone.
        let (has_gnu, gnu_rel, gnu_len) = match &function.instructions {
            DumpByteCodeInstructions::Gnu(super::types::DumpByteData::Mapped(span)) => {
                (true, span.offset as i64 - obj_offset as i64, span.len)
            }
            _ => (false, 0, 0),
        };
        let (const_rel, const_count) = slots_span.map_or((0i64, 0u32), |span| {
            (span.offset as i64 - obj_offset as i64, span.len as u32)
        });
        let mut flags = 0u16;
        if has_gnu {
            flags |= BC_FLAG_HAS_GNU;
        }
        if function.lexical {
            flags |= BC_FLAG_LEXICAL;
        }
        if function.ops_sealed {
            flags |= BC_FLAG_OPS_SEALED;
        }
        if function.params.rest.is_some() {
            flags |= BC_FLAG_HAS_REST;
        }
        if function.docstring.is_some() {
            flags |= BC_FLAG_HAS_DOCSTRING;
        }
        if function.arglist.is_some() {
            flags |= BC_FLAG_HAS_ARGLIST;
        }
        if function.env.is_some() {
            flags |= BC_FLAG_HAS_ENV;
        }
        if function.doc_form.is_some() {
            flags |= BC_FLAG_HAS_DOC_FORM;
        }
        if function.interactive.is_some() {
            flags |= BC_FLAG_HAS_INTERACTIVE;
        }
        let header = BytecodeExtras {
            max_stack: function.max_stack,
            n_required: function.params.required.len() as u16,
            n_optional: function.params.optional.len() as u16,
            flags,
            rest_sym: function.params.rest.as_ref().map_or(0, |s| s.0),
            closure_slot_count: function.closure_slot_count as u32,
            n_extra_slots: function.extra_slots.len() as u32,
            docstring_size: function.docstring.as_ref().map_or(0, |doc| doc.size as u32),
            docstring_size_byte: function.docstring.as_ref().map_or(0, |doc| doc.size_byte),
            gnu_rel,
            gnu_len,
            arglist_word: 0,
            env_word: 0,
            doc_form_word: 0,
            interactive_word: 0,
            const_rel,
            const_count,
            _pad: 0,
        };
        self.write_bytes(base, bytemuck::bytes_of(&header));
        // Metadata value words at their header offsets — via the fixup-aware
        // writer so heap refs patch at load.
        let words = [
            (
                std::mem::offset_of!(BytecodeExtras, arglist_word),
                function.arglist.as_ref(),
            ),
            (
                std::mem::offset_of!(BytecodeExtras, env_word),
                function.env.as_ref(),
            ),
            (
                std::mem::offset_of!(BytecodeExtras, doc_form_word),
                function.doc_form.as_ref(),
            ),
            (
                std::mem::offset_of!(BytecodeExtras, interactive_word),
                function.interactive.as_ref(),
            ),
        ];
        for (field_offset, value) in words {
            if let Some(value) = value {
                self.write_dump_value_word(base + field_offset, value, heap);
            }
        }
        let mut cursor = base + std::mem::size_of::<BytecodeExtras>();
        for id in function
            .params
            .required
            .iter()
            .chain(function.params.optional.iter())
        {
            self.write_bytes(cursor, &id.0.to_le_bytes());
            cursor += 4;
        }
        cursor = (cursor + 7) & !7;
        for slot in &function.extra_slots {
            self.write_dump_value_word(cursor, slot, heap);
            cursor += 8;
        }
        if let Some(doc) = &function.docstring {
            self.write_bytes(cursor, &doc.data);
            cursor += doc.data.len();
        }
        cursor
    }

    fn write_dump_value_word(&mut self, offset: usize, value: &DumpValue, heap: &DumpTaggedHeap) {
        let Some(word) = self.dump_value_word(offset as u64, value, heap) else {
            self.value_fixups.push(RawValueFixup::Value {
                location_offset: offset as u64,
                value: value.clone(),
            });
            let word = TaggedValue::NIL.bits();
            self.write_bytes(offset, &word.to_ne_bytes());
            return;
        };
        self.write_bytes(offset, &word.to_ne_bytes());
    }

    fn dump_value_word(
        &mut self,
        location_offset: u64,
        value: &DumpValue,
        heap: &DumpTaggedHeap,
    ) -> Option<usize> {
        match value {
            DumpValue::Nil => Some(TaggedValue::NIL.bits()),
            DumpValue::True => Some(TaggedValue::T.bits()),
            DumpValue::Int(n) => Some(TaggedValue::fixnum(*n).bits()),
            DumpValue::Unbound => Some(TaggedValue::UNBOUND.bits()),
            DumpValue::Symbol(id) => {
                self.value_fixups
                    .push(RawValueFixup::Symbol { location_offset });
                // BAKED (format v12): the word carries Value::symbol bits over
                // the DUMP-LOCAL id. On an identity load (fresh registry —
                // the production path) the word is already final and the
                // 127K-entry symbol fixup walk is skipped wholesale; the
                // fallback (non-fresh registry: bootstrap cache-miss reload,
                // in-process test loads) untags, remaps, and re-tags.
                Some(TaggedValue::from_sym_id(crate::emacs_core::intern::SymId(id.0)).bits())
            }
            _ => {
                let (target_offset, tag) = mapped_heap_ref_target(value, heap)?;
                self.relocations.push(ImageRelocation {
                    location_offset,
                    addend: tag as u8,
                });
                Some(
                    usize::try_from(target_offset)
                        .expect("mapped heap relocation target offset should fit in a word"),
                )
            }
        }
    }

    fn write_bytes(&mut self, offset: usize, payload: &[u8]) {
        let end = offset
            .checked_add(payload.len())
            .expect("mapped heap write range should not overflow");
        self.bytes[offset..end].copy_from_slice(payload);
    }
}

fn mapped_heap_ref_target(value: &DumpValue, heap: &DumpTaggedHeap) -> Option<(u64, u64)> {
    match value {
        DumpValue::Cons(id) => heap
            .mapped_cons
            .get(id.index as usize)
            .copied()
            .flatten()
            .map(|span| (span.offset, TAG_CONS)),
        DumpValue::Float(id) => heap
            .mapped_floats
            .get(id.index as usize)
            .copied()
            .flatten()
            .map(|span| (span.offset, TAG_FLOAT)),
        DumpValue::Str(id) => heap
            .mapped_strings
            .get(id.index as usize)
            .copied()
            .flatten()
            .map(|span| (span.offset, TAG_STRING)),
        DumpValue::Vector(id)
        | DumpValue::CharTable(id)
        | DumpValue::SubCharTable(id)
        | DumpValue::Record(id)
        | DumpValue::Lambda(id)
        | DumpValue::Macro(id)
        | DumpValue::Marker(id)
        | DumpValue::Overlay(id)
        // ByteCode spans ARE in mapped_veclikes; routing references to them
        // through an ordinary TAG_VECLIKE relocation (bit-identical to what
        // the Value-class fixup produced) turns 6,860 of the 8,685 every-load
        // fixups into baked words. Only descriptor-driven bytecodes (no
        // mapped span) still fall through to the fixup path.
        | DumpValue::ByteCode(id) => heap
            .mapped_veclikes
            .get(id.index as usize)
            .copied()
            .flatten()
            .map(|span| (span.offset, TAG_VECLIKE)),
        _ => None,
    }
}

fn veclike_type_from_tag(tag: u8) -> Result<VecLikeType, DumpError> {
    VecLikeType::try_from(tag).map_err(|_| {
        DumpError::ImageFormatError(format!("unknown mapped vectorlike type tag {tag}"))
    })
}

fn align_padding(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (align - (value & (align - 1))) & (align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emacs_core::pdump::types::DumpTaggedHeap;

    #[test]
    fn mapped_veclike_tags_decode_the_full_runtime_domain() {
        let variants = [
            VecLikeType::Vector,
            VecLikeType::HashTable,
            VecLikeType::Lambda,
            VecLikeType::Macro,
            VecLikeType::ByteCode,
            VecLikeType::Record,
            VecLikeType::Overlay,
            VecLikeType::Marker,
            VecLikeType::Buffer,
            VecLikeType::Window,
            VecLikeType::Frame,
            VecLikeType::Timer,
            VecLikeType::Subr,
            VecLikeType::Xwidget,
            VecLikeType::XwidgetView,
            VecLikeType::SurfaceHandle,
            VecLikeType::VideoHandle,
            VecLikeType::Bignum,
            VecLikeType::SymbolWithPos,
            VecLikeType::Finalizer,
            VecLikeType::Sqlite,
            VecLikeType::UserPtr,
            VecLikeType::ModuleFunction,
            VecLikeType::CharTable,
            VecLikeType::SubCharTable,
            VecLikeType::Obarray,
        ];

        for variant in variants {
            assert_eq!(veclike_type_from_tag(u8::from(variant)).unwrap(), variant);
        }

        assert!(matches!(
            veclike_type_from_tag(u8::MAX),
            Err(DumpError::ImageFormatError(_))
        ));
    }

    #[test]
    fn extracts_string_bytes_into_mapped_heap_section() {
        let mut tagged_heap = DumpTaggedHeap {
            objects: vec![DumpHeapObject::Str {
                data: DumpByteData::owned(b"abc".to_vec()),
                size: 3,
                size_byte: 3,
                text_props: Vec::new(),
            }],
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };

        let heap = extract_tagged_heap_payloads(
            &mut tagged_heap,
            &mut crate::emacs_core::pdump::types::DumpObarray {
                symbols: Vec::new(),
                global_members: Vec::new(),
                function_unbound: Vec::new(),
                function_epoch: 0,
                plain_rows: None,
            },
        );
        assert_eq!(tagged_heap.mapped_strings.len(), 1);
        let string_span = tagged_heap.mapped_strings[0].expect("string object span");
        assert_eq!(string_span.offset, 0);
        assert_eq!(string_span.len as usize, std::mem::size_of::<StringObj>());
        let DumpHeapObject::Str { data, .. } = &tagged_heap.objects[0] else {
            panic!("expected string object");
        };

        let view = MappedHeapView::from_slice(&heap.bytes);
        let mapped = view.bytes(data).unwrap();
        let mapped_bytes = unsafe { std::slice::from_raw_parts(mapped.ptr, mapped.len) };
        assert_eq!(mapped_bytes, b"abc");
        assert_eq!(unsafe { *mapped.ptr.add(mapped.len) }, 0);

        let object_offset = string_span.offset as usize;
        let data_field_offset = object_offset + std::mem::offset_of!(RawStringObj, data);
        assert_eq!(
            heap.bytes[object_offset + 1],
            u8::from(HeapObjectKind::String)
        );
        assert_eq!(
            read_usize(
                &heap.bytes,
                object_offset + std::mem::offset_of!(RawStringObj, size)
            ),
            3
        );
        assert_eq!(
            read_i64(
                &heap.bytes,
                object_offset + std::mem::offset_of!(RawStringObj, size_byte)
            ),
            3
        );
        assert_eq!(
            read_usize(&heap.bytes, data_field_offset),
            mapped.ptr as usize - heap.bytes.as_ptr() as usize
        );
        assert!(
            heap.relocations
                .iter()
                .any(
                    |relocation| relocation.location_offset == data_field_offset as u64
                        && relocation.addend == 0
                )
        );
    }

    #[test]
    fn empty_strings_still_create_heap_section_anchor() {
        let mut tagged_heap = DumpTaggedHeap {
            objects: vec![DumpHeapObject::Str {
                data: DumpByteData::owned(Vec::new()),
                size: 0,
                size_byte: 0,
                text_props: Vec::new(),
            }],
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };

        let heap = extract_tagged_heap_payloads(
            &mut tagged_heap,
            &mut crate::emacs_core::pdump::types::DumpObarray {
                symbols: Vec::new(),
                global_members: Vec::new(),
                function_unbound: Vec::new(),
                function_epoch: 0,
                plain_rows: None,
            },
        );
        assert!(heap.bytes.len() > std::mem::size_of::<StringObj>());
        let DumpHeapObject::Str { data, .. } = &tagged_heap.objects[0] else {
            panic!("expected string object");
        };
        let view = MappedHeapView::from_slice(&heap.bytes);
        let mapped = view.bytes(data).unwrap();
        assert_eq!(mapped.len, 0);
        assert!(mapped.ptr as usize >= heap.bytes.as_ptr() as usize);
        assert_eq!(unsafe { *mapped.ptr }, 0);
    }

    #[test]
    fn reserves_aligned_slot_spans_for_vectorlike_objects() {
        let mut tagged_heap = DumpTaggedHeap {
            objects: vec![DumpHeapObject::Vector(vec![
                crate::emacs_core::pdump::types::DumpValue::Int(1),
                crate::emacs_core::pdump::types::DumpValue::Int(2),
            ])],
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };

        let mut heap = extract_tagged_heap_payloads(
            &mut tagged_heap,
            &mut crate::emacs_core::pdump::types::DumpObarray {
                symbols: Vec::new(),
                global_members: Vec::new(),
                function_unbound: Vec::new(),
                function_epoch: 0,
                plain_rows: None,
            },
        );
        assert!(heap.bytes.len() >= std::mem::size_of::<VectorObj>());
        assert_eq!(tagged_heap.mapped_veclikes.len(), 1);
        let object_span = tagged_heap.mapped_veclikes[0].expect("vector object span");
        assert_eq!(object_span.offset, 0);
        assert_eq!(object_span.len as usize, std::mem::size_of::<VectorObj>());
        assert_eq!(tagged_heap.mapped_slots.len(), 1);
        let span = tagged_heap.mapped_slots[0].expect("vector slot span");
        assert!(span.offset as usize >= std::mem::size_of::<VectorObj>());
        assert_eq!(span.len, 2);
        let view = MappedHeapView::from_mut_slice(&mut heap.bytes);
        let header = view.veclike_header_mut(object_span).unwrap();
        assert_eq!(header.cast::<u8>(), heap.bytes.as_mut_ptr());
        assert_eq!(view.veclike_type(object_span).unwrap(), VecLikeType::Vector);
        let ptr = view
            .typed_object_mut::<VectorObj>(object_span, "vector")
            .unwrap();
        assert_eq!(ptr.cast::<u8>(), heap.bytes.as_mut_ptr());
    }

    #[test]
    fn reserves_mapped_cons_cells_as_heap_objects() {
        let mut tagged_heap = DumpTaggedHeap {
            objects: vec![
                DumpHeapObject::Cons {
                    car: crate::emacs_core::pdump::types::DumpValue::Int(1),
                    cdr: crate::emacs_core::pdump::types::DumpValue::Int(2),
                },
                DumpHeapObject::Cons {
                    car: crate::emacs_core::pdump::types::DumpValue::Int(3),
                    cdr: crate::emacs_core::pdump::types::DumpValue::Nil,
                },
            ],
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };

        let mut heap = extract_tagged_heap_payloads(
            &mut tagged_heap,
            &mut crate::emacs_core::pdump::types::DumpObarray {
                symbols: Vec::new(),
                global_members: Vec::new(),
                function_unbound: Vec::new(),
                function_epoch: 0,
                plain_rows: None,
            },
        );
        assert_eq!(heap.bytes.len(), 2 * std::mem::size_of::<ConsCell>());
        assert_eq!(tagged_heap.mapped_cons.len(), 2);
        let first = tagged_heap.mapped_cons[0].expect("first cons span");
        let second = tagged_heap.mapped_cons[1].expect("second cons span");
        assert_eq!(first.offset, 0);
        assert_eq!(second.offset as usize, std::mem::size_of::<ConsCell>());

        let view = MappedHeapView::from_mut_slice(&mut heap.bytes);
        let ptr = view.cons_cell_mut(first).unwrap();
        assert_eq!(ptr.cast::<u8>(), heap.bytes.as_mut_ptr());

        assert_eq!(
            read_usize(&heap.bytes, first.offset as usize),
            TaggedValue::fixnum(1).bits()
        );
        assert_eq!(
            read_usize(
                &heap.bytes,
                first.offset as usize + std::mem::size_of::<TaggedValue>()
            ),
            TaggedValue::fixnum(2).bits()
        );
    }

    #[test]
    fn reserves_mapped_float_objects_as_heap_objects() {
        let mut tagged_heap = DumpTaggedHeap {
            objects: vec![DumpHeapObject::Float(1.0), DumpHeapObject::Float(2.0)],
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };

        let mut heap = extract_tagged_heap_payloads(
            &mut tagged_heap,
            &mut crate::emacs_core::pdump::types::DumpObarray {
                symbols: Vec::new(),
                global_members: Vec::new(),
                function_unbound: Vec::new(),
                function_epoch: 0,
                plain_rows: None,
            },
        );
        assert_eq!(heap.bytes.len(), 2 * std::mem::size_of::<FloatObj>());
        assert_eq!(tagged_heap.mapped_floats.len(), 2);
        let first = tagged_heap.mapped_floats[0].expect("first float span");
        let second = tagged_heap.mapped_floats[1].expect("second float span");
        assert_eq!(first.offset, 0);
        assert_eq!(second.offset as usize, std::mem::size_of::<FloatObj>());

        let view = MappedHeapView::from_mut_slice(&mut heap.bytes);
        let ptr = view.float_obj_mut(first).unwrap();
        assert_eq!(ptr.cast::<u8>(), heap.bytes.as_mut_ptr());

        assert_eq!(
            heap.bytes[first.offset as usize + 1],
            u8::from(HeapObjectKind::Float)
        );
        let value_offset = first.offset as usize + std::mem::size_of::<RawGcHeader>();
        let value = f64::from_ne_bytes(
            heap.bytes[value_offset..value_offset + std::mem::size_of::<f64>()]
                .try_into()
                .unwrap(),
        );
        assert_eq!(value, 1.0);
    }

    #[test]
    fn emits_tagged_relocations_for_heap_values_in_raw_cons_payload() {
        let mut tagged_heap = DumpTaggedHeap {
            objects: vec![
                DumpHeapObject::Str {
                    data: DumpByteData::owned(b"child".to_vec()),
                    size: 5,
                    size_byte: 5,
                    text_props: Vec::new(),
                },
                DumpHeapObject::Cons {
                    car: crate::emacs_core::pdump::types::DumpValue::Str(
                        crate::emacs_core::pdump::types::DumpHeapRef { index: 0 },
                    ),
                    cdr: crate::emacs_core::pdump::types::DumpValue::Nil,
                },
            ],
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };

        let heap = extract_tagged_heap_payloads(
            &mut tagged_heap,
            &mut crate::emacs_core::pdump::types::DumpObarray {
                symbols: Vec::new(),
                global_members: Vec::new(),
                function_unbound: Vec::new(),
                function_epoch: 0,
                plain_rows: None,
            },
        );
        let cons_span = tagged_heap.mapped_cons[1].expect("mapped cons");
        let string_span = tagged_heap.mapped_strings[0].expect("mapped string");

        assert!(
            heap.relocations
                .iter()
                .any(|relocation| relocation.location_offset == cons_span.offset
                    && relocation.addend == TAG_STRING as u8)
        );
        assert_eq!(
            read_usize(&heap.bytes, cons_span.offset as usize),
            string_span.offset as usize
        );
    }

    #[test]
    fn writes_raw_vector_slots_into_mapped_heap_payload() {
        let mut tagged_heap = DumpTaggedHeap {
            objects: vec![DumpHeapObject::Vector(vec![
                crate::emacs_core::pdump::types::DumpValue::Int(11),
                crate::emacs_core::pdump::types::DumpValue::True,
            ])],
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };

        let heap = extract_tagged_heap_payloads(
            &mut tagged_heap,
            &mut crate::emacs_core::pdump::types::DumpObarray {
                symbols: Vec::new(),
                global_members: Vec::new(),
                function_unbound: Vec::new(),
                function_epoch: 0,
                plain_rows: None,
            },
        );
        let slots = tagged_heap.mapped_slots[0].expect("mapped slots");
        let second = slots.offset as usize + std::mem::size_of::<TaggedValue>();

        assert_eq!(
            read_usize(&heap.bytes, slots.offset as usize),
            TaggedValue::fixnum(11).bits()
        );
        assert_eq!(read_usize(&heap.bytes, second), TaggedValue::T.bits());
    }

    #[test]
    fn emits_value_fixups_for_raw_slots_that_need_runtime_remap() {
        let mut tagged_heap = DumpTaggedHeap {
            objects: vec![DumpHeapObject::Vector(vec![
                crate::emacs_core::pdump::types::DumpValue::Symbol(
                    crate::emacs_core::pdump::types::DumpSymId(42),
                ),
                crate::emacs_core::pdump::types::DumpValue::Subr(
                    crate::emacs_core::pdump::types::DumpNameId(7),
                ),
            ])],
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };

        let heap = extract_tagged_heap_payloads(
            &mut tagged_heap,
            &mut crate::emacs_core::pdump::types::DumpObarray {
                symbols: Vec::new(),
                global_members: Vec::new(),
                function_unbound: Vec::new(),
                function_epoch: 0,
                plain_rows: None,
            },
        );
        let slots = tagged_heap.mapped_slots[0].expect("mapped slots");

        assert_eq!(heap.value_fixups.len(), 2);
        assert!(matches!(
            heap.value_fixups[0],
            RawValueFixup::Symbol { location_offset } if location_offset == slots.offset
        ));
        assert!(matches!(
            heap.value_fixups[1],
            RawValueFixup::Value {
                location_offset,
                value: crate::emacs_core::pdump::types::DumpValue::Subr(_),
            } if location_offset == slots.offset + std::mem::size_of::<TaggedValue>() as u64
        ));
        // v12: the symbol word is BAKED as Value::symbol bits over the
        // dump-local id, not the raw id.
        assert_eq!(
            read_usize(&heap.bytes, slots.offset as usize),
            TaggedValue::from_sym_id(crate::emacs_core::intern::SymId(42)).bits()
        );
    }

    #[test]
    fn rebuild_heap_metadata_matches_extracted_layout() {
        let mut tagged_heap = DumpTaggedHeap {
            objects: vec![
                DumpHeapObject::Str {
                    data: DumpByteData::owned(b"abc".to_vec()),
                    size: 3,
                    size_byte: 3,
                    text_props: Vec::new(),
                },
                DumpHeapObject::Vector(vec![
                    crate::emacs_core::pdump::types::DumpValue::Int(1),
                    crate::emacs_core::pdump::types::DumpValue::Nil,
                ]),
                DumpHeapObject::Cons {
                    car: crate::emacs_core::pdump::types::DumpValue::Int(2),
                    cdr: crate::emacs_core::pdump::types::DumpValue::Nil,
                },
            ],
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };
        let _heap = extract_tagged_heap_payloads(
            &mut tagged_heap,
            &mut crate::emacs_core::pdump::types::DumpObarray {
                symbols: Vec::new(),
                global_members: Vec::new(),
                function_unbound: Vec::new(),
                function_epoch: 0,
                plain_rows: None,
            },
        );
        let expected_cons = tagged_heap.mapped_cons.clone();
        let expected_strings = tagged_heap.mapped_strings.clone();
        let expected_veclikes = tagged_heap.mapped_veclikes.clone();
        let expected_slots = tagged_heap.mapped_slots.clone();

        clear_heap_metadata(&mut tagged_heap);
        rebuild_heap_metadata(&mut tagged_heap).expect("rebuild heap metadata");

        assert_eq!(tagged_heap.mapped_cons, expected_cons);
        assert_eq!(tagged_heap.mapped_strings, expected_strings);
        assert_eq!(tagged_heap.mapped_veclikes, expected_veclikes);
        assert_eq!(tagged_heap.mapped_slots, expected_slots);
    }

    #[test]
    fn reserves_mapped_vectorlike_headers_as_heap_objects() {
        let mut tagged_heap = DumpTaggedHeap {
            objects: vec![
                DumpHeapObject::Vector(Vec::new()),
                DumpHeapObject::Record(Vec::new()),
                DumpHeapObject::Lambda(Vec::new()),
                DumpHeapObject::Macro(Vec::new()),
            ],
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };

        let heap = extract_tagged_heap_payloads(
            &mut tagged_heap,
            &mut crate::emacs_core::pdump::types::DumpObarray {
                symbols: Vec::new(),
                global_members: Vec::new(),
                function_unbound: Vec::new(),
                function_epoch: 0,
                plain_rows: None,
            },
        );

        assert_eq!(tagged_heap.mapped_veclikes.len(), 4);
        assert_eq!(
            tagged_heap.mapped_veclikes[0].unwrap().len as usize,
            std::mem::size_of::<VectorObj>()
        );
        assert_eq!(
            tagged_heap.mapped_veclikes[1].unwrap().len as usize,
            std::mem::size_of::<RecordObj>()
        );
        assert_eq!(
            tagged_heap.mapped_veclikes[2].unwrap().len as usize,
            std::mem::size_of::<LambdaObj>()
        );
        assert_eq!(
            tagged_heap.mapped_veclikes[3].unwrap().len as usize,
            std::mem::size_of::<MacroObj>()
        );
        assert!(heap.bytes.len() >= std::mem::size_of::<VectorObj>());
    }

    fn read_usize(bytes: &[u8], offset: usize) -> usize {
        usize::from_ne_bytes(
            bytes[offset..offset + std::mem::size_of::<usize>()]
                .try_into()
                .unwrap(),
        )
    }

    fn read_i64(bytes: &[u8], offset: usize) -> i64 {
        i64::from_ne_bytes(
            bytes[offset..offset + std::mem::size_of::<i64>()]
                .try_into()
                .unwrap(),
        )
    }
}
