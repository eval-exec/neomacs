//! Conversions between runtime types and pdump snapshot types.

use std::cell::RefCell;
use std::collections::HashMap;

use malachite::integer::Integer;
use rustc_hash::{FxHashMap, FxHashSet};

use super::DumpError;
use super::mapped_heap::MappedHeapView;
use super::object_extra::FileObjectDescriptors;
use super::object_starts::{LoadedObjectSpan, LoadedSpans};
use super::types::*;
use super::value_fixups::{self, RawValueFixup};
use crate::buffer::buffer::{Buffer, BufferDumpParts, BufferId, BufferManager};
use crate::buffer::buffer_text::BufferText;
use crate::buffer::overlay::{Overlay, OverlayList};
use crate::buffer::shared::{SavedPointBeforeCommand, SharedUndoState};
use crate::buffer::text::{BufferTextBytesSnapshot, ImplementedBufferTextBackendKind};
use crate::buffer::text_props::{PropertyInterval, TextPropertyTable};
use crate::buffer::{CharPos0, EmacsBytePos, LispCharPos1, TextPositionAnchor};
// Undo state is now stored directly as a Lisp Value in buffer-local properties.
use crate::emacs_core::abbrev::{Abbrev, AbbrevManager, AbbrevTable};
use crate::emacs_core::advice::{VariableWatcher, VariableWatcherList};
use crate::emacs_core::autoload::{AutoloadEntry, AutoloadManager, AutoloadType};
use crate::emacs_core::bookmark::{Bookmark, BookmarkManager};
use crate::emacs_core::bytecode::chunk::ByteCodeFunction;
use crate::emacs_core::charset::{
    CharsetInfoSnapshot, CharsetMethodSnapshot, CharsetRegistrySnapshot, restore_charset_registry,
    snapshot_charset_registry,
};
use crate::emacs_core::chartable::{
    char_table_external_slots, make_char_table_from_external_slots,
    make_sub_char_table_from_external_slots, sub_char_table_external_slots,
};
use crate::emacs_core::coding::{CodingSystemInfo, CodingSystemManager, EolType};
use crate::emacs_core::custom::CustomManager;
use crate::emacs_core::eval::Context;
use crate::emacs_core::fontset::{
    FontRepertory, FontSpecEntry, FontsetDataSnapshot, FontsetRangeEntrySnapshot,
    FontsetRegistrySnapshot, StoredFontSpec, restore_fontset_registry, snapshot_fontset_registry,
};
use crate::emacs_core::interactive::{InteractiveRegistry, InteractiveSpec};
use crate::emacs_core::intern::{self, NameId, SymId};
use crate::emacs_core::kmacro::KmacroManager;
use crate::emacs_core::mode::{
    CustomGroup as ModeCustomGroup, CustomType as ModeCustomType,
    CustomVariable as ModeCustomVariable, FontLockDefaults, FontLockKeyword, MajorMode, MinorMode,
    ModeRegistry,
};
use crate::emacs_core::rect::RectangleState;
use crate::emacs_core::register::{RegisterContent, RegisterManager};
use crate::emacs_core::symbol::{LispSymbol, Obarray, SymbolTrappedWrite};
use crate::emacs_core::value::{
    ByteCodeKeyPart, HashKey, HashTableTest, HashTableWeakness, LambdaParams, LispHashTable,
    RuntimeBindingValue, StringTextPropertyRun, Value,
};
use crate::emacs_core::value::{ValueKind, VecLikeType};
use crate::emacs_core::value::{
    get_string_text_properties_for_value, set_string_text_properties_for_value,
};
use crate::face::{
    BoxBorder, BoxStyle, Color, Face, FaceDecoration, FaceHeight, FaceTable, FontSlant, FontWeight,
    FontWidth, Underline, UnderlinePosition, UnderlineStyle,
};
use crate::heap_types::LispString;
use crate::tagged::gc::with_tagged_heap;
use crate::tagged::header::{
    ByteCodeObj, CLOSURE_MIN_SLOTS, CharTableObj, ConsCell, FloatObj, HeapObjectKind, LambdaObj,
    LispValueVec, MacroObj, MarkerObj, OverlayObj, RecordObj, StringObj, SubCharTableObj, SubrObj,
    VecLikeHeader, VectorObj,
};
use crate::tagged::value::TaggedValue;

thread_local! {
    static PDUMP_LOAD_NAME_REMAP: RefCell<Option<Vec<NameId>>> = const { RefCell::new(None) };
    static PDUMP_LOAD_SYM_REMAP: RefCell<Option<Vec<SymId>>> = const { RefCell::new(None) };
    /// True when the installed remap is the identity map — derived from a
    /// linear scan of the RETURNED remap (race-free by construction; a
    /// freshness heuristic outside the registry lock is not, and a
    /// seed-prefix match alone is not sufficient: the bootstrap cache-miss
    /// path reloads in-process with a seed-matching but shifted table).
    static PDUMP_LOAD_SYM_IDENTITY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TaggedHeapRef {
    index: u32,
}

struct MappedOffsetRun {
    start: Option<u64>,
    len: usize,
    expected_next: Option<u64>,
    object_size: u64,
    label: &'static str,
}

impl MappedOffsetRun {
    fn new(object_size: u64, label: &'static str) -> Self {
        Self {
            start: None,
            len: 0,
            expected_next: None,
            object_size,
            label,
        }
    }

    fn push(
        &mut self,
        offset: u64,
        mut finish: impl FnMut(u64, usize) -> Result<(), DumpError>,
    ) -> Result<(), DumpError> {
        if let Some(next) = self.expected_next {
            if offset < next {
                return Err(DumpError::ImageFormatError(format!(
                    "mapped {} offsets are not monotonic: got {offset}, expected at least {next}",
                    self.label
                )));
            }
            if offset != next {
                finish(self.start.unwrap(), self.len)?;
                self.start = Some(offset);
                self.len = 1;
            } else {
                self.len += 1;
            }
        } else {
            self.start = Some(offset);
            self.len = 1;
        }
        self.expected_next = Some(offset.checked_add(self.object_size).ok_or_else(|| {
            DumpError::ImageFormatError(format!("mapped {} offset range overflows", self.label))
        })?);
        Ok(())
    }

    fn finish(
        &mut self,
        mut finish: impl FnMut(u64, usize) -> Result<(), DumpError>,
    ) -> Result<(), DumpError> {
        if let Some(start) = self.start.take() {
            finish(start, self.len)?;
        }
        self.len = 0;
        self.expected_next = None;
        Ok(())
    }
}

struct TaggedDumpState {
    objects: Vec<Option<DumpHeapObject>>,
    object_ids: HashMap<usize, TaggedHeapRef>,
}

impl TaggedDumpState {
    fn new() -> Self {
        Self {
            objects: Vec::new(),
            object_ids: HashMap::new(),
        }
    }

    fn finalize(self) -> DumpTaggedHeap {
        DumpTaggedHeap {
            objects: self
                .objects
                .into_iter()
                .map(|obj| obj.unwrap_or(DumpHeapObject::Free))
                .collect(),
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        }
    }
}

pub(crate) struct DumpEncoder {
    state: TaggedDumpState,
}

impl DumpEncoder {
    fn new() -> Self {
        Self {
            state: TaggedDumpState::new(),
        }
    }

    fn finalize(self) -> DumpTaggedHeap {
        self.state.finalize()
    }

    fn value_to_heap_ref(&mut self, v: &Value) -> TaggedHeapRef {
        debug_assert!(v.is_heap_object());
        let bits = v.bits();
        if let Some(id) = self.state.object_ids.get(&bits).copied() {
            return id;
        }

        let id = TaggedHeapRef {
            index: self.state.objects.len() as u32,
        };
        self.state.object_ids.insert(bits, id);
        self.state.objects.push(None);

        let dumped = dump_heap_object_from_value(self, *v);
        self.state.objects[id.index as usize] = Some(dumped);
        id
    }

    fn dump_value(&mut self, v: &Value) -> DumpValue {
        match v.kind() {
            ValueKind::Nil => DumpValue::Nil,
            ValueKind::T => DumpValue::True,
            ValueKind::Fixnum(n) => DumpValue::Int(n),
            ValueKind::Float => DumpValue::Float(dump_heap_ref(self.value_to_heap_ref(v))),
            ValueKind::Symbol(s) => DumpValue::Symbol(dump_sym_id(s)),
            ValueKind::String => DumpValue::Str(dump_heap_ref(self.value_to_heap_ref(v))),
            ValueKind::Cons => DumpValue::Cons(dump_heap_ref(self.value_to_heap_ref(v))),
            ValueKind::Veclike(VecLikeType::Vector) => {
                DumpValue::Vector(dump_heap_ref(self.value_to_heap_ref(v)))
            }
            ValueKind::Veclike(VecLikeType::CharTable) => {
                DumpValue::CharTable(dump_heap_ref(self.value_to_heap_ref(v)))
            }
            ValueKind::Veclike(VecLikeType::SubCharTable) => {
                DumpValue::SubCharTable(dump_heap_ref(self.value_to_heap_ref(v)))
            }
            ValueKind::Veclike(VecLikeType::Record)
            | ValueKind::Veclike(VecLikeType::WindowConfiguration) => {
                DumpValue::Record(dump_heap_ref(self.value_to_heap_ref(v)))
            }
            ValueKind::Veclike(VecLikeType::Font) => {
                panic!("pdump: opened font objects are runtime display resources")
            }
            ValueKind::Veclike(VecLikeType::HashTable) => {
                DumpValue::HashTable(dump_heap_ref(self.value_to_heap_ref(v)))
            }
            ValueKind::Veclike(VecLikeType::Obarray) => {
                DumpValue::Obarray(dump_heap_ref(self.value_to_heap_ref(v)))
            }
            ValueKind::Veclike(VecLikeType::Lambda) => {
                DumpValue::Lambda(dump_heap_ref(self.value_to_heap_ref(v)))
            }
            ValueKind::Veclike(VecLikeType::Macro) => {
                DumpValue::Macro(dump_heap_ref(self.value_to_heap_ref(v)))
            }
            ValueKind::Subr(s) => DumpValue::Subr(dump_name_id(intern::symbol_name_id(s))),
            ValueKind::Veclike(VecLikeType::Subr) => {
                let s = v.as_subr_id().unwrap();
                DumpValue::Subr(dump_name_id(intern::symbol_name_id(s)))
            }
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                DumpValue::ByteCode(dump_heap_ref(self.value_to_heap_ref(v)))
            }
            ValueKind::Veclike(VecLikeType::Marker) => {
                DumpValue::Marker(dump_heap_ref(self.value_to_heap_ref(v)))
            }
            ValueKind::Veclike(VecLikeType::Overlay) => {
                DumpValue::Overlay(dump_heap_ref(self.value_to_heap_ref(v)))
            }
            ValueKind::Veclike(VecLikeType::Buffer) => {
                DumpValue::Buffer(DumpBufferId(v.as_buffer_id().unwrap().0))
            }
            ValueKind::Veclike(VecLikeType::Window) => DumpValue::Window(v.as_window_id().unwrap()),
            ValueKind::Veclike(VecLikeType::Frame) => DumpValue::Frame(v.as_frame_id().unwrap()),
            ValueKind::Veclike(VecLikeType::Timer) => DumpValue::Timer(v.as_timer_id().unwrap()),
            ValueKind::Veclike(VecLikeType::Process) => {
                // Processes are live runtime objects (OS children, sockets);
                // they can never appear in a portable dump image.
                panic!("pdump: process objects are not portable")
            }
            ValueKind::Veclike(VecLikeType::Terminal) => {
                // Terminals are live runtime display objects; dump images must
                // rebuild the initial terminal for the host process.
                panic!("pdump: terminal objects are not portable")
            }
            ValueKind::Veclike(VecLikeType::Xwidget)
            | ValueKind::Veclike(VecLikeType::XwidgetView) => {
                panic!("pdump: xwidget objects are not portable")
            }
            ValueKind::Veclike(VecLikeType::SurfaceHandle) => {
                // Surface handles wrap live GPU objects owned by the host
                // process's render thread; they can never appear in a
                // portable dump image.
                panic!("pdump: shader-surface handles are not portable")
            }
            ValueKind::Veclike(VecLikeType::VideoHandle) => {
                panic!("pdump: video-session handles are not portable")
            }
            ValueKind::Veclike(VecLikeType::Bignum) => {
                DumpValue::Bignum(v.as_bignum().unwrap().to_string())
            }
            ValueKind::Veclike(VecLikeType::SymbolWithPos) => {
                // SymbolWithPos cannot be portably serialized in a pdump yet.
                // Signal an error so callers know this case is unimplemented.
                panic!("pdump: symbol-with-pos is not yet supported in portable dumps")
            }
            ValueKind::Veclike(VecLikeType::Finalizer) => {
                // A live finalizer must never be silently dropped from — or
                // inertly revived in — an image (GNU pdumper does the latter:
                // `dump_finalizer` writes the object but the child never runs
                // it). `dump-emacs-portable` pre-scans the heap's finalizer
                // registry — a superset of anything this walk can reach — and
                // signals an elisp error before writing, so this arm is an
                // unreachable backstop for non-builtin dump entry points.
                panic!("pdump: cannot dump finalizer objects")
            }
            ValueKind::Veclike(VecLikeType::Sqlite) => {
                panic!("pdump: sqlite objects are not portable")
            }
            ValueKind::Veclike(VecLikeType::UserPtr) => {
                panic!("pdump: user-ptr objects are not portable")
            }
            ValueKind::Veclike(VecLikeType::ModuleFunction) => {
                panic!("pdump: module-function objects are not portable")
            }
            ValueKind::Unbound => DumpValue::Unbound,
            ValueKind::Unknown => DumpValue::Nil,
        }
    }

    fn dump_opt_value(&mut self, v: &Option<Value>) -> Option<DumpValue> {
        v.as_ref().map(|value| self.dump_value(value))
    }
}

enum LoadObjectDescriptors {
    /// In-memory snapshots carry one semantic descriptor for every object.
    Snapshot(Vec<DumpHeapObject>),
    /// File dumps omit descriptors for objects already complete in HeapImage.
    File(FileObjectDescriptors),
}

impl LoadObjectDescriptors {
    fn len(&self) -> usize {
        match self {
            Self::Snapshot(objects) => objects.len(),
            Self::File(objects) => objects.len(),
        }
    }

    fn get(&self, index: usize) -> Option<&DumpHeapObject> {
        match self {
            Self::Snapshot(objects) => objects.get(index),
            Self::File(objects) => objects.get(index),
        }
    }

    fn take_or_free(&mut self, index: usize) -> DumpHeapObject {
        match self {
            Self::Snapshot(objects) => std::mem::replace(&mut objects[index], DumpHeapObject::Free),
            Self::File(objects) => objects.take(index).unwrap_or(DumpHeapObject::Free),
        }
    }
}

pub(crate) struct TaggedLoadState<'a> {
    objects: LoadObjectDescriptors,
    spans: LoadedSpans<'a>,
    value_fixups: Vec<RawValueFixup>,
    /// Per-object cached Value, stored as raw bits with a presence bitmap.
    /// `vec![0u64; n]` is calloc-backed, so pages materialize only as
    /// objects actually populate them — `Vec<Option<Value>>` was 2.7MB of
    /// per-page write faults for the same information.
    values: Vec<u64>,
    has_value: Vec<u64>,
    populated: Vec<u64>,
    mapped_heap: Option<MappedHeapView>,
    buffers: FxHashMap<u64, Value>,
    windows: FxHashMap<u64, Value>,
    frames: FxHashMap<u64, Value>,
    timers: FxHashMap<u64, Value>,
    /// O(1) `marker_id` → `MarkerObj*` index built during
    /// `preload_tagged_heap`. Replaces the post-T8
    /// `find_marker_by_id_during_load` heap scan that was used while
    /// reconstructing per-buffer marker chains and resolving state-marker
    /// pointers (`pt`/`begv`/`zv`). The pointer is only valid for the
    /// duration of the load — it points at a `MarkerObj` allocated in the
    /// tagged heap during `preload_tagged_heap` and reachable via the
    /// load-side `Value` cache, so the GC will not free it underneath us.
    pub(crate) markers_by_id: FxHashMap<u64, *mut crate::tagged::header::MarkerObj>,
}

impl<'a> TaggedLoadState<'a> {
    fn new(
        heap: &DumpTaggedHeap,
        mapped_heap: Option<MappedHeapView>,
        value_fixups: Vec<RawValueFixup>,
    ) -> Self {
        let len = heap.objects.len();
        Self {
            objects: LoadObjectDescriptors::Snapshot(heap.objects.clone()),
            spans: LoadedSpans::from_heap(heap),
            value_fixups,
            values: vec![0u64; len],
            has_value: vec![0u64; len.div_ceil(64)],
            populated: vec![0u64; len.div_ceil(64)],
            mapped_heap,
            buffers: FxHashMap::default(),
            windows: FxHashMap::default(),
            frames: FxHashMap::default(),
            timers: FxHashMap::default(),
            markers_by_id: FxHashMap::default(),
        }
    }

    fn from_file_descriptors_and_spans(
        objects: FileObjectDescriptors,
        spans: LoadedSpans<'a>,
        mapped_heap: Option<MappedHeapView>,
        value_fixups: Vec<RawValueFixup>,
    ) -> Self {
        let len = objects.len();
        debug_assert_eq!(spans.len(), len);
        Self {
            objects: LoadObjectDescriptors::File(objects),
            spans,
            value_fixups,
            values: vec![0u64; len],
            has_value: vec![0u64; len.div_ceil(64)],
            populated: vec![0u64; len.div_ceil(64)],
            mapped_heap,
            buffers: FxHashMap::default(),
            windows: FxHashMap::default(),
            frames: FxHashMap::default(),
            timers: FxHashMap::default(),
            markers_by_id: FxHashMap::default(),
        }
    }

    #[inline]
    fn cached_value(&self, index: usize) -> Option<Value> {
        (self.has_value[index >> 6] & (1u64 << (index & 63)) != 0)
            .then(|| Value::from_bits(self.values[index] as usize))
    }

    #[inline]
    fn set_cached_value(&mut self, index: usize, value: Value) {
        self.has_value[index >> 6] |= 1u64 << (index & 63);
        self.values[index] = value.bits() as u64;
    }

    #[inline]
    fn is_populated(&self, index: usize) -> bool {
        self.populated[index >> 6] & (1u64 << (index & 63)) != 0
    }

    #[inline]
    fn mark_populated(&mut self, index: usize) {
        self.populated[index >> 6] |= 1u64 << (index & 63);
    }
}

pub(crate) struct LoadDecoder<'a> {
    state: TaggedLoadState<'a>,
}

impl LoadDecoder<'_> {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn new(heap: &DumpTaggedHeap) -> Self {
        Self::new_with_mapped_heap(heap, None)
    }

    pub(crate) fn new_with_mapped_heap(
        heap: &DumpTaggedHeap,
        mapped_heap: Option<MappedHeapView>,
    ) -> Self {
        Self::new_with_mapped_heap_and_fixups(heap, mapped_heap, Vec::new())
    }

    pub(crate) fn new_with_mapped_heap_and_fixups(
        heap: &DumpTaggedHeap,
        mapped_heap: Option<MappedHeapView>,
        value_fixups: Vec<RawValueFixup>,
    ) -> Self {
        Self {
            state: TaggedLoadState::new(heap, mapped_heap, value_fixups),
        }
    }
}

impl<'a> LoadDecoder<'a> {
    pub(crate) fn from_file_descriptors_and_spans_with_mapped_heap_and_fixups(
        objects: FileObjectDescriptors,
        spans: LoadedSpans<'a>,
        mapped_heap: Option<MappedHeapView>,
        value_fixups: Vec<RawValueFixup>,
    ) -> Self {
        Self {
            state: TaggedLoadState::from_file_descriptors_and_spans(
                objects,
                spans,
                mapped_heap,
                value_fixups,
            ),
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn preload_tagged_heap(&mut self) -> Result<(), DumpError> {
        self.preload_tagged_heap_with_value_fixup_section(None)
    }

    pub(crate) fn preload_tagged_heap_with_value_fixup_section(
        &mut self,
        value_fixups_section: Option<&[u8]>,
    ) -> Result<(), DumpError> {
        self.register_mapped_objects()?;
        for index in 0..self.state.objects.len() {
            if self.object_is_fully_mapped_without_load_work(index) {
                continue;
            }
            self.allocate_tagged_placeholder(TaggedHeapRef {
                index: index as u32,
            })?;
        }
        if let Some(section) = value_fixups_section {
            self.apply_mapped_value_fixup_section(section)?;
        } else {
            self.apply_mapped_value_fixups()?;
        }
        for index in 0..self.state.objects.len() {
            if self.object_needs_no_post_fixup_population(index) {
                continue;
            }
            self.populate_tagged_object(TaggedHeapRef {
                index: index as u32,
            })?;
        }
        Ok(())
    }

    pub(crate) fn discard_restored_file_object_descriptors(&mut self) {
        if self.state.mapped_heap.is_none() {
            return;
        }
        let LoadObjectDescriptors::File(objects) = &mut self.state.objects else {
            return;
        };
        if !objects
            .iter()
            .all(restored_file_object_descriptor_is_discardable)
        {
            return;
        }

        // File-pdump restore has already installed every live value into
        // `state.values` and the returned Context.  At this point the
        // remaining descriptors are only no-payload sentinels or mapped
        // descriptors whose bytes live in the mmap image, so running their
        // enum destructors is pure startup overhead.
        unsafe {
            objects.discard_without_drop();
        }
    }

    fn object_is_fully_mapped_without_load_work(&self, index: usize) -> bool {
        matches!(
            self.state.spans.get(index),
            LoadedObjectSpan::Cons(_) | LoadedObjectSpan::Float(_)
        )
    }

    fn object_needs_no_post_fixup_population(&self, index: usize) -> bool {
        if self.object_is_fully_mapped_without_load_work(index) {
            return true;
        }
        let Some(object) = self.state.objects.get(index) else {
            // No descriptor record: self-contained. Extended bytecode spans
            // still need their extras-driven population pass.
            return !self
                .bytecode_extras_span(TaggedHeapRef {
                    index: index as u32,
                })
                .is_some();
        };
        match object {
            DumpHeapObject::Vector(_)
            | DumpHeapObject::Lambda(_)
            | DumpHeapObject::Macro(_)
            | DumpHeapObject::Record(_) => self.mapped_slots_exist(TaggedHeapRef {
                index: index as u32,
            }),
            DumpHeapObject::Str { text_props, .. } => {
                text_props.is_empty() && self.state.spans.string(index).is_some()
            }
            DumpHeapObject::Float(_) => true,
            _ => false,
        }
    }

    fn apply_mapped_value_fixup_section(&mut self, section: &[u8]) -> Result<(), DumpError> {
        let mapped_heap = self.state.mapped_heap.ok_or_else(|| {
            DumpError::ImageFormatError("value fixups require a writable mapped heap image".into())
        })?;
        let parts = value_fixups::section_parts(section)?;
        let batch = mapped_heap.value_word_batch()?;
        // Identity load: the baked words are already final — skip the
        // 127K-entry offset walk (no per-entry section reads, zero word
        // writes). The Value-class entries below are dumped as NIL
        // placeholders and must be applied on EVERY load: they carry Subr
        // function cells and unresolvable heap refs, and skipping them is
        // total startup failure, not a perf choice.
        let identity = PDUMP_LOAD_SYM_IDENTITY.with(|flag| flag.get());
        if !identity {
            self.apply_symbol_offset_fixups(&batch, parts.symbol_offsets)?;
        }
        value_fixups::for_each_value_entry(&parts, |location_offset, value| {
            let word = batch.word_ptr(location_offset)?;
            let value = self.load_value(&value);
            unsafe { word.cast::<Value>().write_unaligned(value) };
            Ok(())
        })
    }

    /// Apply the Symbol-class fixups: each entry is just a heap-word offset;
    /// the word already holds the dump-local SymId, so the whole class is
    /// read -> remap -> re-tag. The remap is borrowed ONCE for the loop --
    /// `load_sym_id`'s per-call thread-local borrow was a large share of the
    /// ~50 Ir/fixup this class used to cost across ~113K entries.
    fn apply_symbol_offset_fixups(
        &mut self,
        batch: &crate::emacs_core::pdump::mapped_heap::ValueWordBatch,
        offsets_le: &[u8],
    ) -> Result<(), DumpError> {
        if offsets_le.is_empty() {
            return Ok(());
        }
        PDUMP_LOAD_SYM_REMAP.with(|slot| {
            let slot = slot.borrow();
            let remap = slot.as_deref().ok_or_else(|| {
                DumpError::ImageFormatError(
                    "symbol fixups applied before the dump symbol table was restored".into(),
                )
            })?;
            for chunk in offsets_le.chunks_exact(4) {
                let offset = u64::from(u32::from_le_bytes(chunk.try_into().expect("4-byte chunk")));
                let word = batch.word_ptr(offset)?;
                // Format v12 bakes the word as Value::symbol(dump_local_id)
                // bits; untag, remap, re-tag.
                let baked = Value::from_bits(unsafe { word.read_unaligned() });
                let dump_id = baked.as_symbol_id().ok_or_else(|| {
                    DumpError::ImageFormatError(format!(
                        "symbol fixup word at offset {offset} does not hold baked symbol bits"
                    ))
                })?;
                let sym = remap.get(dump_id.0 as usize).copied().ok_or_else(|| {
                    DumpError::ImageFormatError(format!(
                        "symbol fixup id {} is outside the remap of {} slots",
                        dump_id.0,
                        remap.len()
                    ))
                })?;
                unsafe { word.cast::<Value>().write_unaligned(Value::symbol(sym)) };
            }
            Ok(())
        })
    }

    fn apply_mapped_value_fixups(&mut self) -> Result<(), DumpError> {
        if self.state.value_fixups.is_empty() {
            return Ok(());
        }
        let mapped_heap = self.state.mapped_heap.ok_or_else(|| {
            DumpError::ImageFormatError("value fixups require a writable mapped heap image".into())
        })?;
        let fixups = std::mem::take(&mut self.state.value_fixups);
        for fixup in fixups {
            self.apply_mapped_value_fixup(mapped_heap, fixup)?;
        }
        Ok(())
    }

    fn apply_mapped_value_fixup(
        &mut self,
        mapped_heap: MappedHeapView,
        fixup: RawValueFixup,
    ) -> Result<(), DumpError> {
        match fixup {
            RawValueFixup::Symbol { location_offset } => {
                // One validation for the read-modify-write pair. Format v12:
                // the word holds BAKED Value::symbol(dump_local_id) bits —
                // reading it as a raw id would silently resolve symbol
                // 8*dump_id. Untag first, like the section walk.
                let word = mapped_heap.value_word_ptr(location_offset)?;
                let baked = Value::from_bits(unsafe { word.read_unaligned() });
                let dump_id = baked.as_symbol_id().ok_or_else(|| {
                    DumpError::ImageFormatError(format!(
                        "symbol value-fixup word at offset {location_offset} does not hold baked symbol bits"
                    ))
                })?;
                let value = Value::symbol(load_sym_id(&DumpSymId(dump_id.0)));
                unsafe { word.cast::<Value>().write_unaligned(value) };
                Ok(())
            }
            RawValueFixup::Value {
                location_offset,
                value,
            } => {
                let word = mapped_heap.value_word_ptr(location_offset)?;
                let value = self.load_value(&value);
                unsafe { word.cast::<Value>().write_unaligned(value) };
                Ok(())
            }
        }
    }

    fn register_mapped_objects(&self) -> Result<(), DumpError> {
        let mapped_heap = self.state.mapped_heap;
        let mut cons_run = MappedOffsetRun::new(std::mem::size_of::<ConsCell>() as u64, "cons");
        let mut float_run = MappedOffsetRun::new(std::mem::size_of::<FloatObj>() as u64, "float");

        // Pre-size the heap-side registries: one counting pass over the span
        // table is far cheaper than growing a 12K-entry FxHashMap by
        // rehashing during registration.
        let (mut veclikes, mut strings) = (0usize, 0usize);
        for (_index, record) in self.state.spans.iter() {
            match record {
                LoadedObjectSpan::Vectorlike { .. } => veclikes += 1,
                LoadedObjectSpan::String { .. } => strings += 1,
                _ => {}
            }
        }
        with_tagged_heap(|heap| heap.reserve_mapped_object_capacity(veclikes, strings));

        for (_index, record) in self.state.spans.iter() {
            match record {
                // Bare slot spans (bytecode constant pools) need no cell-range
                // registration: the span is reached only through the owning
                // function's LispValueVec and traced through its GC arm.
                LoadedObjectSpan::SlotsOnly(_) => {}
                LoadedObjectSpan::Cons(span) => {
                    cons_run.push(span.offset, |run_start, run_len| {
                        let mapped_heap = mapped_heap.ok_or_else(|| {
                            DumpError::ImageFormatError(
                                "dump reserves mapped cons objects but image has no heap section"
                                    .into(),
                            )
                        })?;
                        let ptr = mapped_heap.cons_cell_mut(DumpConsSpan { offset: run_start })?;
                        with_tagged_heap(|heap| unsafe {
                            heap.register_mapped_cons_range(ptr, run_len);
                        });
                        Ok(())
                    })?;
                }
                LoadedObjectSpan::Float(span) => {
                    float_run.push(span.offset, |run_start, run_len| {
                        let mapped_heap = mapped_heap.ok_or_else(|| {
                            DumpError::ImageFormatError(
                                "dump reserves mapped float objects but image has no heap section"
                                    .into(),
                            )
                        })?;
                        let ptr = mapped_heap.float_obj_mut(DumpFloatSpan { offset: run_start })?;
                        with_tagged_heap(|heap| unsafe {
                            heap.register_mapped_float_range(ptr, run_len);
                        });
                        Ok(())
                    })?;
                }
                LoadedObjectSpan::String { object: span, .. } => {
                    let mapped_heap = mapped_heap.ok_or_else(|| {
                        DumpError::ImageFormatError(
                            "dump reserves mapped string objects but image has no heap section"
                                .into(),
                        )
                    })?;
                    let ptr = mapped_heap.string_obj_mut(span)?;
                    with_tagged_heap(|heap| unsafe {
                        heap.register_mapped_string_object(ptr, std::mem::size_of::<StringObj>())
                    });
                }
                LoadedObjectSpan::Vectorlike { object: span, .. } => {
                    let mapped_heap = mapped_heap.ok_or_else(|| {
                        DumpError::ImageFormatError(
                            "dump reserves mapped vectorlike objects but image has no heap section"
                                .into(),
                        )
                    })?;
                    let ptr = mapped_heap.veclike_header_mut(span)?;
                    let byte_len = usize::try_from(span.len).map_err(|_| {
                        DumpError::ImageFormatError(
                            "mapped vectorlike span length overflows usize".into(),
                        )
                    })?;
                    with_tagged_heap(|heap| unsafe {
                        heap.register_mapped_veclike_object(ptr, byte_len)
                    });
                }
                LoadedObjectSpan::None | LoadedObjectSpan::Unmapped => {}
            }
        }

        cons_run.finish(|run_start, run_len| {
            let mapped_heap = mapped_heap.ok_or_else(|| {
                DumpError::ImageFormatError(
                    "dump reserves mapped cons objects but image has no heap section".into(),
                )
            })?;
            let ptr = mapped_heap.cons_cell_mut(DumpConsSpan { offset: run_start })?;
            with_tagged_heap(|heap| unsafe {
                heap.register_mapped_cons_range(ptr, run_len);
            });
            Ok(())
        })?;
        float_run.finish(|run_start, run_len| {
            let mapped_heap = mapped_heap.ok_or_else(|| {
                DumpError::ImageFormatError(
                    "dump reserves mapped float objects but image has no heap section".into(),
                )
            })?;
            let ptr = mapped_heap.float_obj_mut(DumpFloatSpan { offset: run_start })?;
            with_tagged_heap(|heap| unsafe {
                heap.register_mapped_float_range(ptr, run_len);
            });
            Ok(())
        })?;
        Ok(())
    }

    fn allocate_mapped_self_contained_veclike(
        &mut self,
        id: TaggedHeapRef,
    ) -> Result<Option<Value>, DumpError> {
        let index = id.index as usize;
        if self.state.objects.get(index).is_some() {
            return Ok(None);
        }
        let Some(span) = self.state.spans.vectorlike(index) else {
            return Ok(None);
        };
        let mapped_heap = self.state.mapped_heap.ok_or_else(|| {
            DumpError::ImageFormatError(
                "dump reserves mapped vectorlike objects but image has no heap section".into(),
            )
        })?;
        let slot_count = self.mapped_slot_count_or(id, 0)?;
        let value = match mapped_heap.veclike_type(span)? {
            VecLikeType::Vector => {
                let ptr = self
                    .mapped_typed_object_for_object::<VectorObj>(id, "vector")?
                    .ok_or_else(|| {
                        DumpError::ImageFormatError(
                            "mapped vector span disappeared during restore".into(),
                        )
                    })?;
                let data = self
                    .mapped_slots_for_object_without_copy(id, slot_count)?
                    .unwrap_or_else(|| LispValueVec::owned(vec![Value::NIL; slot_count]));
                unsafe {
                    std::ptr::write(
                        ptr,
                        VectorObj {
                            header: VecLikeHeader::new(VecLikeType::Vector),
                            data,
                        },
                    );
                    Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                }
            }
            VecLikeType::Lambda => {
                let len = slot_count.max(CLOSURE_MIN_SLOTS);
                let ptr = self
                    .mapped_typed_object_for_object::<LambdaObj>(id, "lambda")?
                    .ok_or_else(|| {
                        DumpError::ImageFormatError(
                            "mapped lambda span disappeared during restore".into(),
                        )
                    })?;
                let data = self
                    .mapped_slots_for_object_without_copy(id, slot_count)?
                    .unwrap_or_else(|| LispValueVec::owned(vec![Value::NIL; len]));
                unsafe {
                    std::ptr::write(
                        ptr,
                        LambdaObj {
                            header: VecLikeHeader::new(VecLikeType::Lambda),
                            data,
                            parsed_params: std::sync::OnceLock::new(),
                        },
                    );
                    Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                }
            }
            VecLikeType::Macro => {
                let len = slot_count.max(CLOSURE_MIN_SLOTS);
                let ptr = self
                    .mapped_typed_object_for_object::<MacroObj>(id, "macro")?
                    .ok_or_else(|| {
                        DumpError::ImageFormatError(
                            "mapped macro span disappeared during restore".into(),
                        )
                    })?;
                let data = self
                    .mapped_slots_for_object_without_copy(id, slot_count)?
                    .unwrap_or_else(|| LispValueVec::owned(vec![Value::NIL; len]));
                unsafe {
                    std::ptr::write(
                        ptr,
                        MacroObj {
                            header: VecLikeHeader::new(VecLikeType::Macro),
                            data,
                            parsed_params: std::sync::OnceLock::new(),
                        },
                    );
                    Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                }
            }
            VecLikeType::Record => {
                let ptr = self
                    .mapped_typed_object_for_object::<RecordObj>(id, "record")?
                    .ok_or_else(|| {
                        DumpError::ImageFormatError(
                            "mapped record span disappeared during restore".into(),
                        )
                    })?;
                let data = self
                    .mapped_slots_for_object_without_copy(id, slot_count)?
                    .unwrap_or_else(|| LispValueVec::owned(vec![Value::NIL; slot_count]));
                unsafe {
                    std::ptr::write(
                        ptr,
                        RecordObj {
                            header: VecLikeHeader::new(VecLikeType::Record),
                            data,
                        },
                    );
                    Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                }
            }
            VecLikeType::CharTable => {
                let ptr = self
                    .mapped_typed_object_for_object::<CharTableObj>(id, "char-table")?
                    .ok_or_else(|| {
                        DumpError::ImageFormatError(
                            "mapped char-table span disappeared during restore".into(),
                        )
                    })?;
                let extras = self
                    .mapped_slots_for_object_without_copy(id, slot_count)?
                    .unwrap_or_else(|| LispValueVec::owned(vec![Value::NIL; slot_count]));
                // The four fixed slots and the 64 top-level contents were
                // baked into the image span at dump time and patched by the
                // value fixups — write ONLY the runtime header and the
                // extras storage, never the inline value words.
                unsafe {
                    std::ptr::addr_of_mut!((*ptr).header)
                        .write(VecLikeHeader::new(VecLikeType::CharTable));
                    std::ptr::addr_of_mut!((*ptr).extras).write(extras);
                    Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                }
            }
            VecLikeType::SubCharTable => {
                let ptr = self
                    .mapped_typed_object_for_object::<SubCharTableObj>(id, "sub-char-table")?
                    .ok_or_else(|| {
                        DumpError::ImageFormatError(
                            "mapped sub-char-table span disappeared during restore".into(),
                        )
                    })?;
                let contents = self
                    .mapped_slots_for_object_without_copy(id, slot_count)?
                    .unwrap_or_else(|| LispValueVec::owned(vec![Value::NIL; slot_count]));
                // depth/min_char were baked raw at dump time; write only the
                // runtime header and the contents storage.
                unsafe {
                    std::ptr::addr_of_mut!((*ptr).header)
                        .write(VecLikeHeader::new(VecLikeType::SubCharTable));
                    std::ptr::addr_of_mut!((*ptr).contents).write(contents);
                    Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                }
            }
            VecLikeType::ByteCode => {
                // Self-contained bytecode: the extras region after the
                // struct carries what the descriptor used to (see
                // `mapped_heap::BytecodeExtras`).
                //
                // Extras present => the LAZY STUB was BAKED into the image
                // at dump time (v15): the mapped bytes already ARE
                // `pdump_stub(extras_len)` — header included — so this arm
                // writes NOTHING (the per-object full-struct ptr::write used
                // to COW ~1,187 image pages every startup). The stub-finalize
                // pass validates the baked struct + extras and, on
                // non-identity loads, rewrites param ids; first access
                // through get_bytecode_data materializes. The baked bytes
                // are trusted only after the header's stub layout witness
                // matched this binary (validate_image).
                // Descriptor-driven (no extras) => the span is zero-filled
                // in the image; write today's unsealed placeholder verbatim,
                // so is_pdump_stub is never even transiently true there.
                let ptr = self
                    .mapped_typed_object_for_object::<ByteCodeObj>(id, "bytecode")?
                    .ok_or_else(|| {
                        DumpError::ImageFormatError(
                            "mapped bytecode span disappeared during restore".into(),
                        )
                    })?;
                let extras_len = self
                    .bytecode_extras_span(id)
                    .map_or(0, |span| span.len as usize);
                if extras_len > 0 {
                    unsafe { Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>()) }
                } else {
                    let function = ByteCodeFunction {
                        source_id: crate::emacs_core::bytecode::fresh_bytecode_source_id(),
                        ops: Vec::new(),
                        ops_sealed: false,
                        stack_verified: false,
                        constants: Vec::new().into(),
                        max_stack: 0,
                        params: LambdaParams::simple(Vec::new()),
                        arglist: Value::NIL,
                        lexical: false,
                        env: None,
                        gnu_byte_offset_map: None,
                        gnu_bytecode_bytes: None,
                        docstring: None,
                        doc_form: None,
                        interactive: None,
                        closure_slot_count: 4,
                        extra_slots: Vec::new(),
                        #[cfg(feature = "jit")]
                        runtime: Some(crate::emacs_core::jit::Runtime::new()),
                        lazy_gnu_code: None,
                    };
                    unsafe {
                        std::ptr::write(
                            ptr,
                            ByteCodeObj {
                                header: VecLikeHeader::new(VecLikeType::ByteCode),
                                data: function,
                            },
                        );
                        Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                    }
                }
            }
            VecLikeType::Marker | VecLikeType::Overlay => {
                return Err(DumpError::ImageFormatError(
                    "mapped marker/overlay is missing ObjectExtra descriptor".into(),
                ));
            }
            other => {
                return Err(DumpError::ImageFormatError(format!(
                    "unexpected self-contained mapped vectorlike type {other:?}"
                )));
            }
        };
        self.state.set_cached_value(index, value);
        Ok(Some(value))
    }

    fn heap_ref_to_value(&mut self, id: TaggedHeapRef) -> Value {
        self.allocate_tagged_placeholder(id)
            .expect("pdump placeholder allocation should succeed")
    }

    fn load_cached_buffer(&mut self, id: u64) -> Value {
        *self
            .state
            .buffers
            .entry(id)
            .or_insert_with(|| Value::make_buffer(BufferId(id)))
    }

    fn load_cached_window(&mut self, id: u64) -> Value {
        *self
            .state
            .windows
            .entry(id)
            .or_insert_with(|| Value::make_window(id))
    }

    fn load_cached_frame(&mut self, id: u64) -> Value {
        *self
            .state
            .frames
            .entry(id)
            .or_insert_with(|| Value::make_frame(id))
    }

    fn load_cached_timer(&mut self, id: u64) -> Value {
        *self
            .state
            .timers
            .entry(id)
            .or_insert_with(|| Value::make_timer(id))
    }

    fn load_dump_string(
        &self,
        data: &DumpByteData,
        size: usize,
        size_byte: i64,
    ) -> Result<LispString, DumpError> {
        match data {
            DumpByteData::Owned(bytes) => Ok(LispString::from_dump(bytes.clone(), size, size_byte)),
            DumpByteData::Mapped(_) => {
                let mapped_heap = self.state.mapped_heap.ok_or_else(|| {
                    DumpError::ImageFormatError(
                        "dump references mapped heap bytes but image has no heap section".into(),
                    )
                })?;
                let bytes = mapped_heap.bytes(data)?;
                Ok(unsafe { LispString::from_mapped_bytes(bytes.ptr, bytes.len, size, size_byte) })
            }
            DumpByteData::StaticRoData { key, len } => {
                if size_byte != -2 {
                    return Err(DumpError::ImageFormatError(format!(
                        "static rodata string has non-rodata size_byte {size_byte}"
                    )));
                }
                let len = usize::try_from(*len).map_err(|_| {
                    DumpError::ImageFormatError(
                        "static rodata string length overflows usize".into(),
                    )
                })?;
                LispString::from_registered_rodata_unibyte(*key, len, size).ok_or_else(|| {
                    DumpError::ImageFormatError(format!(
                        "static rodata string key {key:#x} length {len} is not registered"
                    ))
                })
            }
        }
    }

    fn install_mapped_string_sidecars(
        &self,
        ptr: *mut StringObj,
        data: &DumpByteData,
    ) -> Result<(), DumpError> {
        match data {
            DumpByteData::Mapped(_) => {
                // The dump writer bakes the byte-span offset into the
                // StringObj data field and emits an ImageRelocation for it
                // (write_raw_string_obj), so the sidecar pointer is already
                // installed before reconstruction runs.
                // validate_storage_install's pointer-match arm proved exactly
                // that on every load: the pre-set pointer had to equal the
                // re-derived one. Verification-only, so debug builds (where
                // every round-trip test runs) keep it and release trusts the
                // relocation - ~30K strings' worth of span re-derivation,
                // NUL probes, and redundant re-stores per load.
                #[cfg(debug_assertions)]
                {
                    let mapped_heap = self.state.mapped_heap.ok_or_else(|| {
                        DumpError::ImageFormatError(
                            "dump references mapped heap bytes but image has no heap section"
                                .into(),
                        )
                    })?;
                    let bytes = mapped_heap.bytes(data)?;
                    unsafe {
                        (*ptr)
                            .data
                            .install_mapped_storage_sidecar(bytes.ptr, bytes.len)
                            .map_err(DumpError::ImageFormatError)?;
                    }
                }
                Ok(())
            }
            DumpByteData::StaticRoData { key, len } => {
                let len = usize::try_from(*len).map_err(|_| {
                    DumpError::ImageFormatError(
                        "static rodata string length overflows usize".into(),
                    )
                })?;
                unsafe {
                    (*ptr)
                        .data
                        .install_registered_rodata_sidecar(*key, len)
                        .map_err(DumpError::ImageFormatError)?;
                }
                Ok(())
            }
            DumpByteData::Owned(_) => Err(DumpError::ImageFormatError(
                "mapped string object still references owned byte data".into(),
            )),
        }
    }

    fn mapped_slots_for_object(
        &self,
        id: TaggedHeapRef,
        slots: &[Value],
    ) -> Result<Option<LispValueVec>, DumpError> {
        let Some(ptr) = self.mapped_slots_ptr_for_object(id, slots.len())? else {
            return Ok(None);
        };
        if !slots.is_empty() {
            unsafe {
                std::ptr::copy_nonoverlapping(slots.as_ptr(), ptr, slots.len());
            }
        }
        Ok(Some(unsafe {
            LispValueVec::mapped(ptr.cast_const(), slots.len())
        }))
    }

    fn mapped_slots_for_object_without_copy(
        &self,
        id: TaggedHeapRef,
        expected_len: usize,
    ) -> Result<Option<LispValueVec>, DumpError> {
        let Some(ptr) = self.mapped_slots_ptr_for_object(id, expected_len)? else {
            return Ok(None);
        };
        Ok(Some(unsafe {
            LispValueVec::mapped(ptr.cast_const(), expected_len)
        }))
    }

    fn mapped_slot_count_or(
        &self,
        id: TaggedHeapRef,
        fallback_len: usize,
    ) -> Result<usize, DumpError> {
        let Some(span) = self.state.spans.slots(id.index as usize) else {
            return Ok(fallback_len);
        };
        usize::try_from(span.len).map_err(|_| {
            DumpError::ImageFormatError("mapped slot span length overflows usize".into())
        })
    }

    fn mapped_slots_ptr_for_object(
        &self,
        id: TaggedHeapRef,
        expected_len: usize,
    ) -> Result<Option<*mut TaggedValue>, DumpError> {
        let Some(span) = self.state.spans.slots(id.index as usize) else {
            return Ok(None);
        };
        let mapped_heap = self.state.mapped_heap.ok_or_else(|| {
            DumpError::ImageFormatError(
                "dump reserves mapped vector slots but image has no heap section".into(),
            )
        })?;
        mapped_heap.slots_mut(span, expected_len).map(Some)
    }

    fn mapped_cons_cell_for_object(
        &self,
        id: TaggedHeapRef,
    ) -> Result<Option<*mut ConsCell>, DumpError> {
        let Some(span) = self.state.spans.cons(id.index as usize) else {
            return Ok(None);
        };
        let mapped_heap = self.state.mapped_heap.ok_or_else(|| {
            DumpError::ImageFormatError(
                "dump reserves mapped cons cells but image has no heap section".into(),
            )
        })?;
        mapped_heap.cons_cell_mut(span).map(Some)
    }

    fn mapped_float_obj_for_object(
        &self,
        id: TaggedHeapRef,
    ) -> Result<Option<*mut FloatObj>, DumpError> {
        let Some(span) = self.state.spans.float(id.index as usize) else {
            return Ok(None);
        };
        let mapped_heap = self.state.mapped_heap.ok_or_else(|| {
            DumpError::ImageFormatError(
                "dump reserves mapped float objects but image has no heap section".into(),
            )
        })?;
        mapped_heap.float_obj_mut(span).map(Some)
    }

    /// Reconstruct a self-contained string directly from the heap image.
    ///
    /// Mirrors `allocate_mapped_self_contained_veclike`: property-free strings
    /// with mapped bytes have no object-extra descriptor; the object-starts
    /// span carries the byte-data span instead.  The
    /// StringObj header is already baked into the image with its data pointer
    /// relocated (`write_raw_string_obj`); the only load-time work is allocating
    /// the storage-sidecar box that marks the bytes as mapped.
    fn allocate_mapped_self_contained_string(
        &mut self,
        id: TaggedHeapRef,
    ) -> Result<Option<Value>, DumpError> {
        let index = id.index as usize;
        if self.state.objects.get(index).is_some() {
            return Ok(None);
        }
        let Some(byte_span) = self.state.spans.string_self_contained_data(index) else {
            return Ok(None);
        };
        let ptr = self.mapped_string_obj_for_object(id)?.ok_or_else(|| {
            DumpError::ImageFormatError(
                "self-contained string span has no mapped string object".into(),
            )
        })?;
        unsafe {
            debug_assert!(matches!((*ptr).header.kind, HeapObjectKind::String));
        }
        // Self-contained strings are always DumpByteData::Mapped, whose
        // sidecar the relocation pass already installed; the call is
        // verification-only (see install_mapped_string_sidecars) so release
        // skips even the empty-arm dispatch here.
        #[cfg(debug_assertions)]
        {
            let data = DumpByteData::Mapped(byte_span);
            self.install_mapped_string_sidecars(ptr, &data)?;
        }
        #[cfg(not(debug_assertions))]
        let _ = byte_span;
        Ok(Some(unsafe { Value::from_string_ptr(ptr) }))
    }

    fn mapped_string_obj_for_object(
        &self,
        id: TaggedHeapRef,
    ) -> Result<Option<*mut StringObj>, DumpError> {
        let Some(span) = self.state.spans.string(id.index as usize) else {
            return Ok(None);
        };
        let mapped_heap = self.state.mapped_heap.ok_or_else(|| {
            DumpError::ImageFormatError(
                "dump reserves mapped string objects but image has no heap section".into(),
            )
        })?;
        mapped_heap.string_obj_mut(span).map(Some)
    }

    fn mapped_typed_object_for_object<T: 'static>(
        &self,
        id: TaggedHeapRef,
        label: &'static str,
    ) -> Result<Option<*mut T>, DumpError> {
        let Some(span) = self.state.spans.vectorlike(id.index as usize) else {
            return Ok(None);
        };
        let mapped_heap = self.state.mapped_heap.ok_or_else(|| {
            DumpError::ImageFormatError(
                "dump reserves mapped vectorlike objects but image has no heap section".into(),
            )
        })?;
        mapped_heap.typed_object_mut::<T>(span, label).map(Some)
    }

    fn install_mapped_vector_slots(value: Value, storage: LispValueVec) -> bool {
        if value.veclike_type() != Some(VecLikeType::Vector) {
            return false;
        }
        let ptr = value.as_veclike_ptr().unwrap() as *mut VectorObj;
        unsafe {
            (*ptr).data = storage;
        }
        true
    }

    fn install_mapped_record_slots(value: Value, storage: LispValueVec) -> bool {
        if value.veclike_type() != Some(VecLikeType::Record) {
            return false;
        }
        let ptr = value.as_veclike_ptr().unwrap() as *mut RecordObj;
        unsafe {
            (*ptr).data = storage;
        }
        true
    }

    fn install_mapped_closure_slots(value: Value, storage: LispValueVec) -> bool {
        match value.veclike_type() {
            Some(VecLikeType::Lambda) => {
                let ptr = value.as_veclike_ptr().unwrap() as *mut LambdaObj;
                unsafe {
                    let obj = &mut *ptr;
                    let _ = obj.parsed_params.take();
                    obj.data = storage;
                }
                true
            }
            Some(VecLikeType::Macro) => {
                let ptr = value.as_veclike_ptr().unwrap() as *mut MacroObj;
                unsafe {
                    let obj = &mut *ptr;
                    let _ = obj.parsed_params.take();
                    obj.data = storage;
                }
                true
            }
            _ => false,
        }
    }

    fn install_restored_bytecode_data(
        value: Value,
        data: ByteCodeFunction,
    ) -> Result<(), DumpError> {
        if value.veclike_type() != Some(VecLikeType::ByteCode) {
            return Err(DumpError::ImageFormatError(
                "pdump bytecode descriptor resolved to non-bytecode object".into(),
            ));
        }
        let ptr = value.as_veclike_ptr().unwrap() as *mut ByteCodeObj;
        // This is pdump restore-time initialization of a freshly allocated
        // placeholder.  GNU applies dump relocations directly into restored
        // objects here; no user-observable heap mutation has happened yet.
        unsafe {
            (*ptr).data = data;
        }
        Ok(())
    }

    fn mapped_cons_has_raw_words(
        &self,
        id: TaggedHeapRef,
        _car: &DumpValue,
        _cdr: &DumpValue,
    ) -> bool {
        // If a mapped cons span exists, the HeapImage bytes are the
        // source of truth (set by relocation).  The placeholder car/cdr
        // DumpValue is irrelevant for Category A objects.
        self.state.spans.cons(id.index as usize).is_some()
    }

    fn mapped_slots_exist(&self, id: TaggedHeapRef) -> bool {
        self.state.spans.slots(id.index as usize).is_some()
    }

    fn populate_from_mapped_heap_without_descriptor_clone(
        &mut self,
        id: TaggedHeapRef,
        value: Value,
        object: &DumpHeapObject,
    ) -> Result<bool, DumpError> {
        match object {
            DumpHeapObject::Cons { .. } => {
                if self.state.spans.cons(id.index as usize).is_some() {
                    return Ok(true);
                }
            }
            DumpHeapObject::Vector(items) => {
                if self.mapped_slots_exist(id) {
                    let len = self.mapped_slot_count_or(id, items.len())?;
                    let storage = self
                        .mapped_slots_for_object_without_copy(id, len)?
                        .ok_or_else(|| {
                            DumpError::ImageFormatError(
                                "mapped vector object has no mapped slot storage".into(),
                            )
                        })?;
                    let _ = Self::install_mapped_vector_slots(value, storage);
                    return Ok(true);
                }
            }
            DumpHeapObject::Lambda(slots) | DumpHeapObject::Macro(slots) => {
                if self.mapped_slots_exist(id) {
                    let len = self.mapped_slot_count_or(id, slots.len())?;
                    let storage = self
                        .mapped_slots_for_object_without_copy(id, len)?
                        .ok_or_else(|| {
                            DumpError::ImageFormatError(
                                "mapped closure object has no mapped slot storage".into(),
                            )
                        })?;
                    let _ = Self::install_mapped_closure_slots(value, storage);
                    return Ok(true);
                }
            }
            DumpHeapObject::Record(items) => {
                if self.mapped_slots_exist(id) {
                    let len = self.mapped_slot_count_or(id, items.len())?;
                    let storage = self
                        .mapped_slots_for_object_without_copy(id, len)?
                        .ok_or_else(|| {
                            DumpError::ImageFormatError(
                                "mapped record object has no mapped slot storage".into(),
                            )
                        })?;
                    let _ = Self::install_mapped_record_slots(value, storage);
                    return Ok(true);
                }
            }
            DumpHeapObject::Str { text_props, .. } if text_props.is_empty() => {
                return Ok(true);
            }
            DumpHeapObject::Float(_) => {
                return Ok(true);
            }
            _ => {}
        }

        Ok(false)
    }

    fn allocate_tagged_placeholder(&mut self, id: TaggedHeapRef) -> Result<Value, DumpError> {
        if let Some(value) = self.state.cached_value(id.index as usize) {
            return Ok(value);
        }
        // One span-table dispatch instead of four sequential mapped probes:
        // this runs once per dump object (70K objects), and the old chain
        // re-indexed and re-matched the same record per probe.
        match self.state.spans.get(id.index as usize) {
            crate::emacs_core::pdump::object_starts::LoadedObjectSpan::Cons(span) => {
                let mapped_heap = self.state.mapped_heap.ok_or_else(|| {
                    DumpError::ImageFormatError(
                        "dump reserves mapped cons cells but image has no heap section".into(),
                    )
                })?;
                let cell = mapped_heap.cons_cell_mut(span)?;
                let value = unsafe { Value::from_cons_ptr(cell) };
                self.state.set_cached_value(id.index as usize, value);
                return Ok(value);
            }
            crate::emacs_core::pdump::object_starts::LoadedObjectSpan::Float(_) => {
                if let Some(ptr) = self.mapped_float_obj_for_object(id)? {
                    let value = unsafe { Value::from_float_ptr(ptr) };
                    self.state.set_cached_value(id.index as usize, value);
                    return Ok(value);
                }
            }
            crate::emacs_core::pdump::object_starts::LoadedObjectSpan::Vectorlike { .. } => {
                if let Some(value) = self.allocate_mapped_self_contained_veclike(id)? {
                    return Ok(value);
                }
            }
            crate::emacs_core::pdump::object_starts::LoadedObjectSpan::String { .. } => {
                if let Some(value) = self.allocate_mapped_self_contained_string(id)? {
                    return Ok(value);
                }
            }
            _ => {}
        }
        let object = self.state.objects.get(id.index as usize).ok_or_else(|| {
            DumpError::ImageFormatError(format!(
                "dump object {} has neither a mapped representation nor a descriptor",
                id.index
            ))
        })?;
        let value = match object {
            DumpHeapObject::Cons { .. } => Value::cons(Value::NIL, Value::NIL),
            DumpHeapObject::Vector(items) => {
                let len = self.mapped_slot_count_or(id, items.len())?;
                if let Some(ptr) = self.mapped_typed_object_for_object::<VectorObj>(id, "vector")? {
                    let data = self
                        .mapped_slots_for_object_without_copy(id, len)?
                        .unwrap_or_else(|| LispValueVec::owned(vec![Value::NIL; len]));
                    unsafe {
                        std::ptr::write(
                            ptr,
                            VectorObj {
                                header: VecLikeHeader::new(VecLikeType::Vector),
                                data,
                            },
                        );
                        Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                    }
                } else {
                    Value::make_vector(vec![Value::NIL; len])
                }
            }
            DumpHeapObject::CharTable { extras, .. } => {
                let extra_len = extras.len();
                Value::make_char_table(Value::NIL, Value::NIL, extra_len)
            }
            DumpHeapObject::SubCharTable {
                depth,
                min_char,
                contents,
            } => {
                let depth = i32::try_from(*depth).map_err(|_| {
                    DumpError::ImageFormatError("sub-char-table depth overflows i32".into())
                })?;
                let min_char = i32::try_from(*min_char).map_err(|_| {
                    DumpError::ImageFormatError("sub-char-table min-char overflows i32".into())
                })?;
                Value::make_sub_char_table(depth, min_char, vec![Value::NIL; contents.len()])
            }
            DumpHeapObject::HashTable(ht) => with_tagged_heap(|heap| {
                // GNU pdumper restores the hash table header first and wires
                // the key/value arrays via relocation.  This placeholder is
                // only for identity during graph fixups, so avoid allocating
                // maps that population immediately replaces.
                heap.alloc_hash_table(LispHashTable::new_unpopulated_with_options(
                    load_hash_table_test(&ht.test),
                    ht.size,
                    ht.weakness.as_ref().map(load_hash_table_weakness),
                    ht.rehash_size,
                    ht.rehash_threshold,
                ))
            }),
            DumpHeapObject::Obarray { buckets, .. } => Value::obarray(buckets.len()),
            DumpHeapObject::Str {
                data,
                size,
                size_byte,
                ..
            } => {
                if let Some(ptr) = self.mapped_string_obj_for_object(id)? {
                    unsafe {
                        debug_assert!(matches!((*ptr).header.kind, HeapObjectKind::String));
                        debug_assert_eq!((*ptr).data.schars(), *size);
                        debug_assert_eq!((*ptr).data.size_byte(), *size_byte);
                    }
                    self.install_mapped_string_sidecars(ptr, data)?;
                    unsafe { Value::from_string_ptr(ptr) }
                } else {
                    let string = self.load_dump_string(data, *size, *size_byte)?;
                    Value::heap_string(string)
                }
            }
            DumpHeapObject::Float(value) => Value::make_float(*value),
            DumpHeapObject::Lambda(slots) => {
                let slot_count = self.mapped_slot_count_or(id, slots.len())?;
                let len = slot_count.max(CLOSURE_MIN_SLOTS);
                if let Some(ptr) = self.mapped_typed_object_for_object::<LambdaObj>(id, "lambda")? {
                    let data = self
                        .mapped_slots_for_object_without_copy(id, slot_count)?
                        .unwrap_or_else(|| LispValueVec::owned(vec![Value::NIL; len]));
                    unsafe {
                        std::ptr::write(
                            ptr,
                            LambdaObj {
                                header: VecLikeHeader::new(VecLikeType::Lambda),
                                data,
                                parsed_params: std::sync::OnceLock::new(),
                            },
                        );
                        Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                    }
                } else {
                    with_tagged_heap(|heap| heap.alloc_lambda(vec![Value::NIL; len]))
                }
            }
            DumpHeapObject::Macro(slots) => {
                let slot_count = self.mapped_slot_count_or(id, slots.len())?;
                let len = slot_count.max(CLOSURE_MIN_SLOTS);
                if let Some(ptr) = self.mapped_typed_object_for_object::<MacroObj>(id, "macro")? {
                    let data = self
                        .mapped_slots_for_object_without_copy(id, slot_count)?
                        .unwrap_or_else(|| LispValueVec::owned(vec![Value::NIL; len]));
                    unsafe {
                        std::ptr::write(
                            ptr,
                            MacroObj {
                                header: VecLikeHeader::new(VecLikeType::Macro),
                                data,
                                parsed_params: std::sync::OnceLock::new(),
                            },
                        );
                        Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                    }
                } else {
                    with_tagged_heap(|heap| heap.alloc_macro(vec![Value::NIL; len]))
                }
            }
            DumpHeapObject::ByteCode(_) => {
                let function = ByteCodeFunction {
                    source_id: crate::emacs_core::bytecode::fresh_bytecode_source_id(),
                    ops: Vec::new(),
                    ops_sealed: false,
                    stack_verified: false,
                    constants: Vec::new().into(),
                    max_stack: 0,
                    params: LambdaParams::simple(Vec::new()),
                    arglist: Value::NIL,
                    lexical: false,
                    env: None,
                    gnu_byte_offset_map: None,
                    gnu_bytecode_bytes: None,
                    docstring: None,
                    doc_form: None,
                    interactive: None,
                    closure_slot_count: 4,
                    extra_slots: Vec::new(),
                    #[cfg(feature = "jit")]
                    runtime: Some(crate::emacs_core::jit::Runtime::new()),
                    lazy_gnu_code: None,
                };
                // Install into the image-reserved ByteCodeObj when the dump
                // mapped one (same space-then-populate deal markers use);
                // populate overwrites the fields in place either way.
                if let Some(ptr) =
                    self.mapped_typed_object_for_object::<ByteCodeObj>(id, "bytecode")?
                {
                    unsafe {
                        std::ptr::write(
                            ptr,
                            ByteCodeObj {
                                header: VecLikeHeader::new(VecLikeType::ByteCode),
                                data: function,
                            },
                        );
                        Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                    }
                } else {
                    Value::make_bytecode(function)
                }
            }
            DumpHeapObject::Record(items) => {
                let len = self.mapped_slot_count_or(id, items.len())?;
                if let Some(ptr) = self.mapped_typed_object_for_object::<RecordObj>(id, "record")? {
                    let data = self
                        .mapped_slots_for_object_without_copy(id, len)?
                        .unwrap_or_else(|| LispValueVec::owned(vec![Value::NIL; len]));
                    unsafe {
                        std::ptr::write(
                            ptr,
                            RecordObj {
                                header: VecLikeHeader::new(VecLikeType::Record),
                                data,
                            },
                        );
                        Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                    }
                } else {
                    Value::make_record(vec![Value::NIL; len])
                }
            }
            DumpHeapObject::Marker(marker) => {
                let data = crate::heap_types::LispMarker {
                    buffer: marker.buffer.map(|id| BufferId(id.0)),
                    insertion_type: marker.insertion_type,
                    marker_id: marker.marker_id,
                    // v26: bytepos/charpos round-trip directly from LispMarker.
                    bytepos: marker.bytepos,
                    charpos: marker.charpos,
                    last_position_valid: marker.last_position_valid,
                    next_marker: std::ptr::null_mut(),
                };
                let value = if let Some(ptr) =
                    self.mapped_typed_object_for_object::<MarkerObj>(id, "marker")?
                {
                    unsafe {
                        std::ptr::write(
                            ptr,
                            MarkerObj {
                                header: VecLikeHeader::new(VecLikeType::Marker),
                                data,
                            },
                        );
                        Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                    }
                } else {
                    Value::make_marker(data)
                };
                // Index by `marker_id` so per-buffer chain reconstruction and
                // state-marker resolution can do an O(1) lookup instead of a
                // heap-wide walk. Both call sites consult `markers_by_id` in
                // place of the retired `find_marker_by_id_during_load`.
                if let Some(id) = marker.marker_id
                    && let Some(ptr) = value.as_veclike_ptr()
                {
                    self.state
                        .markers_by_id
                        .insert(id, ptr as *mut crate::tagged::header::MarkerObj);
                }
                value
            }
            DumpHeapObject::Overlay(overlay) => {
                let data = crate::heap_types::OverlayData {
                    serial: overlay.serial,
                    plist: Value::NIL,
                    buffer: overlay.buffer.map(|id| BufferId(id.0)),
                    start: overlay.start,
                    end: overlay.end,
                    position_handle: None,
                    front_advance: overlay.front_advance,
                    rear_advance: overlay.rear_advance,
                };
                crate::heap_types::observe_overlay_serial(data.serial);
                if let Some(ptr) =
                    self.mapped_typed_object_for_object::<OverlayObj>(id, "overlay")?
                {
                    unsafe {
                        std::ptr::write(
                            ptr,
                            OverlayObj {
                                header: VecLikeHeader::new(VecLikeType::Overlay),
                                data,
                            },
                        );
                        Value::from_veclike_ptr(ptr.cast::<VecLikeHeader>())
                    }
                } else {
                    Value::make_overlay(data)
                }
            }
            DumpHeapObject::Buffer(id) => self.load_cached_buffer(id.0),
            DumpHeapObject::Window(id) => self.load_cached_window(*id),
            DumpHeapObject::Frame(id) => self.load_cached_frame(*id),
            DumpHeapObject::Timer(id) => self.load_cached_timer(*id),
            DumpHeapObject::Subr { name, .. } => {
                let name_id = load_name_id(name);
                if let Some(sym_id) = intern::canonical_symbol_for_name(name_id) {
                    Value::subr_from_sym_id(sym_id)
                } else {
                    let n = intern::resolve_name(name_id);
                    Value::subr_from_sym_id(intern::intern(n))
                }
            }
            DumpHeapObject::Free => Value::NIL,
        };
        self.state.set_cached_value(id.index as usize, value);
        Ok(value)
    }

    /// The extras span of a self-contained bytecode object (the veclike
    /// span's tail past `ByteCodeObj`), or `None`.
    fn bytecode_extras_span(&self, id: TaggedHeapRef) -> Option<super::types::DumpByteSpan> {
        let span = self.state.spans.vectorlike(id.index as usize)?;
        let obj_len = std::mem::size_of::<ByteCodeObj>() as u64;
        if span.len <= obj_len {
            return None;
        }
        let mapped_heap = self.state.mapped_heap?;
        if mapped_heap.veclike_type(span).ok()? != VecLikeType::ByteCode {
            return None;
        }
        Some(super::types::DumpByteSpan {
            offset: span.offset + obj_len,
            len: span.len - obj_len,
        })
    }

    /// Rebuild a ByteCodeFunction from the image extras region (see
    /// `mapped_heap::BytecodeExtras`) — the self-contained replacement for
    /// the object-extra descriptor. Runs after the value-fixup pass, so the
    /// metadata and extra-slot words read as final tagged values.
    fn populate_bytecode_from_extras(
        &mut self,
        id: TaggedHeapRef,
        value: Value,
    ) -> Result<bool, DumpError> {
        // (The lazy materializer `materialize_bytecode_from_extras_at` below
        // is the loader-state-free twin of this pass; keep their decode
        // logic in lockstep.)
        use crate::emacs_core::pdump::mapped_heap::{
            BC_FLAG_HAS_ARGLIST, BC_FLAG_HAS_DOC_FORM, BC_FLAG_HAS_DOCSTRING, BC_FLAG_HAS_ENV,
            BC_FLAG_HAS_GNU, BC_FLAG_HAS_INTERACTIVE, BC_FLAG_HAS_REST, BC_FLAG_LEXICAL,
            BC_FLAG_OPS_SEALED, BytecodeExtras,
        };
        let Some(extras_span) = self.bytecode_extras_span(id) else {
            return Ok(false);
        };
        let mapped_heap = self.state.mapped_heap.ok_or_else(|| {
            DumpError::ImageFormatError("bytecode extras require a heap image".into())
        })?;

        // (a0) v15: the stub was BAKED at dump time and the loader wrote
        // nothing into the struct region, so this is the release-mode
        // integrity gate the old full-struct ptr::write used to provide for
        // free. Compare the mapped bytes against the canonical template —
        // byte-wise, never through typed bool/Vec reads on possibly-corrupt
        // bytes — with the per-object closure_slot_count word checked
        // against the loader-derived extras length. Costs no extra page
        // faults: the header page was read by the placeholder pass and the
        // trailing bytes by the extras validation below.
        {
            use crate::emacs_core::bytecode::chunk::ByteCodeFunction;
            static STUB_TEMPLATE_ZERO: std::sync::OnceLock<Box<[u8]>> = std::sync::OnceLock::new();
            let template = STUB_TEMPLATE_ZERO
                .get_or_init(|| crate::emacs_core::pdump::mapped_heap::baked_stub_template(0));
            let obj_ptr = value.as_veclike_ptr().ok_or_else(|| {
                DumpError::ImageFormatError(
                    "bytecode finalize on a value without a veclike pointer".into(),
                )
            })? as *const u8;
            let data_ptr = unsafe {
                obj_ptr.add(std::mem::offset_of!(
                    crate::tagged::header::ByteCodeObj,
                    data
                ))
            };
            let baked = unsafe { std::slice::from_raw_parts(data_ptr, template.len()) };
            let cnt = std::mem::offset_of!(ByteCodeFunction, closure_slot_count);
            let baked_count = usize::from_ne_bytes(
                baked[cnt..cnt + std::mem::size_of::<usize>()]
                    .try_into()
                    .expect("closure_slot_count word"),
            );
            if baked[..cnt] != template[..cnt]
                || baked[cnt + std::mem::size_of::<usize>()..]
                    != template[cnt + std::mem::size_of::<usize>()..]
                || baked_count != extras_span.len as usize
            {
                return Err(DumpError::ImageFormatError(format!(
                    "baked bytecode stub bytes are corrupt (object {}: extras len {} vs baked \
                     count {})",
                    id.index, extras_span.len, baked_count
                )));
            }
        }

        let extras = mapped_heap.bytes_unterminated(extras_span)?;
        let bytes = unsafe { std::slice::from_raw_parts(extras.ptr, extras.len) };
        let header_len = std::mem::size_of::<BytecodeExtras>();
        if bytes.len() < header_len {
            return Err(DumpError::ImageFormatError(
                "bytecode extras region shorter than its header".into(),
            ));
        }
        let header: BytecodeExtras = bytemuck::pod_read_unaligned(&bytes[..header_len]);
        let flags = header.flags;

        // v14 STUB-FINALIZE: the placeholder already wrote the lazy stub;
        // this pass (a) BOUNDS-VALIDATES the whole region so first-call
        // materialization is infallible by construction, (b) rewrites the
        // packed param-id words in place on NON-IDENTITY loads (they are
        // 4-byte LE words no value fixup can patch; the mapping is
        // MAP_PRIVATE, so the rewrite is process-local), and (c) honors the
        // eager-GNU toggle by materializing immediately, preserving that
        // mode's load-time decode+validate timing.
        let n_ids = header.n_required as usize + header.n_optional as usize;
        let ids_end = header_len + n_ids * 4;
        if bytes.len() < ids_end {
            return Err(DumpError::ImageFormatError(
                "bytecode extras param ids exceed the region".into(),
            ));
        }
        let extra_start = (ids_end + 7) & !7;
        let extra_end = extra_start + header.n_extra_slots as usize * 8;
        if bytes.len() < extra_end {
            return Err(DumpError::ImageFormatError(
                "bytecode extras slot words exceed the region".into(),
            ));
        }
        if flags & BC_FLAG_HAS_DOCSTRING != 0 {
            let doc_len = if header.docstring_size_byte >= 0 {
                header.docstring_size_byte as usize
            } else {
                header.docstring_size as usize
            };
            if bytes.len() < extra_end + doc_len {
                return Err(DumpError::ImageFormatError(
                    "bytecode extras docstring exceeds the region".into(),
                ));
            }
        }
        if flags & BC_FLAG_HAS_GNU != 0 {
            let obj_span = self
                .state
                .spans
                .vectorlike(id.index as usize)
                .ok_or_else(|| {
                    DumpError::ImageFormatError(
                        "bytecode extras claim a GNU region but the object has no span".into(),
                    )
                })?;
            let gnu_offset =
                u64::try_from(obj_span.offset as i64 + header.gnu_rel).map_err(|_| {
                    DumpError::ImageFormatError(
                        "bytecode GNU region offset underflows the heap image".into(),
                    )
                })?;
            // Structural presence check; decode-level validation happens at
            // materialization (or right below under the eager toggle).
            mapped_heap.bytes(&super::types::DumpByteData::Mapped(
                super::types::DumpByteSpan {
                    offset: gnu_offset,
                    len: header.gnu_len,
                },
            ))?;
        }
        if header.const_count > 0 {
            let obj_span = self
                .state
                .spans
                .vectorlike(id.index as usize)
                .ok_or_else(|| {
                    DumpError::ImageFormatError(
                        "bytecode extras claim constants but the object has no span".into(),
                    )
                })?;
            let const_offset =
                u64::try_from(obj_span.offset as i64 + header.const_rel).map_err(|_| {
                    DumpError::ImageFormatError(
                        "bytecode constants offset underflows the heap image".into(),
                    )
                })?;
            mapped_heap.slots_mut(
                super::types::DumpSlotSpan {
                    offset: const_offset,
                    len: u64::from(header.const_count),
                },
                header.const_count as usize,
            )?;
        }

        // (b) Non-identity loads: remap the packed param ids in place.
        if !PDUMP_LOAD_SYM_IDENTITY.with(|flag| flag.get()) {
            let ids_ptr = unsafe { extras.ptr.add(header_len) } as *mut u8;
            PDUMP_LOAD_SYM_REMAP.with(|slot| {
                let slot = slot.borrow();
                let remap = slot.as_deref().ok_or_else(|| {
                    DumpError::ImageFormatError(
                        "bytecode finalize ran before the dump symbol table was restored".into(),
                    )
                })?;
                let remap_one = |raw: u32| -> Result<u32, DumpError> {
                    remap.get(raw as usize).map(|sym| sym.0).ok_or_else(|| {
                        DumpError::ImageFormatError(format!(
                            "bytecode param symbol {raw} is outside the remap of {} slots",
                            remap.len()
                        ))
                    })
                };
                for i in 0..n_ids {
                    let word_ptr = unsafe { ids_ptr.add(i * 4) };
                    let raw = u32::from_le_bytes(unsafe {
                        std::ptr::read_unaligned(word_ptr.cast::<[u8; 4]>())
                    });
                    let mapped = remap_one(raw)?;
                    unsafe {
                        std::ptr::write_unaligned(word_ptr.cast::<[u8; 4]>(), mapped.to_le_bytes())
                    };
                }
                if flags & BC_FLAG_HAS_REST != 0 {
                    let rest_off = std::mem::offset_of!(BytecodeExtras, rest_sym);
                    let rest_ptr = unsafe { (extras.ptr as *mut u8).add(rest_off) };
                    let raw = u32::from_le_bytes(unsafe {
                        std::ptr::read_unaligned(rest_ptr.cast::<[u8; 4]>())
                    });
                    let mapped = remap_one(raw)?;
                    unsafe {
                        std::ptr::write_unaligned(rest_ptr.cast::<[u8; 4]>(), mapped.to_le_bytes())
                    };
                }
                Ok::<(), DumpError>(())
            })?;
        }

        // (c) Eager-GNU mode: materialize now, exactly as the old load did.
        if crate::emacs_core::bytecode::chunk::eager_gnu_bytecode() {
            let function = unsafe { materialize_bytecode_from_extras_at(value) };
            Self::install_restored_bytecode_data(value, function)?;
        }
        Ok(true)
    }

    fn populate_tagged_object(&mut self, id: TaggedHeapRef) -> Result<(), DumpError> {
        let index = id.index as usize;
        if self.state.is_populated(index) {
            return Ok(());
        }

        let value = self.allocate_tagged_placeholder(id)?;
        self.state.mark_populated(index);
        if self.state.objects.get(index).is_none()
            && self.populate_bytecode_from_extras(id, value)?
        {
            return Ok(());
        }
        let object = self.state.objects.take_or_free(index);
        if self.populate_from_mapped_heap_without_descriptor_clone(id, value, &object)? {
            return Ok(());
        }

        match object {
            DumpHeapObject::Cons { car, cdr } => {
                if !self.mapped_cons_has_raw_words(id, &car, &cdr) {
                    value.set_car(self.load_value(&car));
                    value.set_cdr(self.load_value(&cdr));
                }
            }
            DumpHeapObject::Vector(items) => {
                let len = self.mapped_slot_count_or(id, items.len())?;
                if let Some(storage) = self.mapped_slots_for_object_without_copy(id, len)? {
                    let _ = Self::install_mapped_vector_slots(value, storage);
                } else {
                    let slots: Vec<_> = items.iter().map(|item| self.load_value(item)).collect();
                    if let Some(storage) = self.mapped_slots_for_object(id, &slots)? {
                        let _ = Self::install_mapped_vector_slots(value, storage);
                    } else {
                        let _ = value.replace_vector_data(slots);
                    }
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
                if contents.len() != 64 {
                    return Err(DumpError::ImageFormatError(format!(
                        "char-table dump has {} content slots, expected 64",
                        contents.len()
                    )));
                }
                let mut slots = Vec::with_capacity(4 + contents.len() + extras.len());
                slots.push(self.load_value(&defalt));
                slots.push(self.load_value(&parent));
                slots.push(self.load_value(&purpose));
                slots.push(self.load_value(&ascii));
                slots.extend(contents.iter().map(|slot| self.load_value(slot)));
                slots.extend(extras.iter().map(|slot| self.load_value(slot)));
                let restored = make_char_table_from_external_slots(&slots)
                    .map_err(DumpError::ImageFormatError)?;
                value.with_char_table_mut(|table| {
                    let restored_obj = restored
                        .as_char_table_obj()
                        .expect("make-char-table returned char-table");
                    table.defalt = restored_obj.defalt;
                    table.parent = restored_obj.parent;
                    table.purpose = restored_obj.purpose;
                    table.ascii = restored_obj.ascii;
                    table.contents = restored_obj.contents;
                    table.extras = restored_obj.extras.clone();
                });
            }
            DumpHeapObject::SubCharTable {
                depth,
                min_char,
                contents,
            } => {
                let mut slots = Vec::with_capacity(2 + contents.len());
                slots.push(Value::fixnum(depth));
                slots.push(Value::fixnum(min_char));
                slots.extend(contents.iter().map(|slot| self.load_value(slot)));
                let restored = make_sub_char_table_from_external_slots(&slots)
                    .map_err(DumpError::ImageFormatError)?;
                value.with_sub_char_table_mut(|table| {
                    let restored_obj = restored
                        .as_sub_char_table_obj()
                        .expect("make-sub-char-table returned sub-char-table");
                    table.depth = restored_obj.depth;
                    table.min_char = restored_obj.min_char;
                    table.contents = restored_obj.contents.clone();
                });
            }
            DumpHeapObject::HashTable(ht) => {
                let DumpLispHashTable {
                    test,
                    test_name,
                    size,
                    weakness,
                    rehash_size,
                    rehash_threshold,
                    ordered_entries,
                } = ht;
                let restored_test = load_hash_table_test(&test);
                let entries: Vec<_> = ordered_entries
                    .into_iter()
                    .map(|(k, v, snap)| {
                        let value = self.load_value_owned(v);
                        let snapshot = snap.map(|s| self.load_value_owned(s));
                        let restored_key = snapshot.unwrap_or(value);
                        let rebuilt =
                            rebuild_target_hash_key(&k, restored_key, restored_test, usize::BITS);
                        let key = rebuilt.unwrap_or_else(|| load_hash_key_owned(self, k));
                        (key, value, snapshot)
                    })
                    .collect();
                let _ = value.with_hash_table_mut(|table| {
                    table.test = restored_test;
                    table.test_name = test_name.map(|s| load_sym_id(&s));
                    table.size = size;
                    table.weakness = weakness.as_ref().map(load_hash_table_weakness);
                    table.rehash_size = rehash_size;
                    table.rehash_threshold = rehash_threshold;
                    if table.weakness.is_some() {
                        // The weak sweep enumerates and removes entries through
                        // the hydrated index; keep weak tables eager.
                        table.rebuild_from_ordered_entries(entries);
                    } else {
                        // GNU pdumper's hash_rehash_needed, lazily: park the
                        // decoded entries; the first accessor hydrates. Most
                        // loaded tables are never touched at startup.
                        table.set_pending_dump_entries(entries);
                    }
                });
            }
            DumpHeapObject::Obarray { buckets, count } => {
                let buckets: Vec<_> = buckets
                    .into_iter()
                    .map(|bucket| self.load_value_owned(bucket))
                    .collect();
                let _ =
                    crate::emacs_core::builtins::symbols::replace_obarray_buckets(value, buckets);
                let _ = value.with_obarray_mut(|obj| obj.count = count);
            }
            DumpHeapObject::Str { text_props, .. } => {
                if !text_props.is_empty() {
                    for run in &text_props {
                        self.populate_value_graph(&run.plist)?;
                    }
                    let runs = text_props
                        .iter()
                        .map(|run| StringTextPropertyRun {
                            start: run.start,
                            end: run.end,
                            plist: self.load_value(&run.plist),
                        })
                        .collect();
                    set_string_text_properties_for_value(value, runs);
                }
            }
            DumpHeapObject::Float(_) => {}
            DumpHeapObject::Lambda(slots) | DumpHeapObject::Macro(slots) => {
                let len = self.mapped_slot_count_or(id, slots.len())?;
                if let Some(storage) = self.mapped_slots_for_object_without_copy(id, len)? {
                    let _ = Self::install_mapped_closure_slots(value, storage);
                } else {
                    let slots: Vec<_> = slots.iter().map(|slot| self.load_value(slot)).collect();
                    if let Some(storage) = self.mapped_slots_for_object(id, &slots)? {
                        let _ = Self::install_mapped_closure_slots(value, storage);
                    } else {
                        let _ = value.replace_closure_slots(slots);
                    }
                }
            }
            DumpHeapObject::ByteCode(bc) => {
                // Alias the constants pool directly in the mapped image when
                // the dump reserved a slot span (same phase contract as
                // vector/record slots: fixups patch the image in place).
                let len = self.mapped_slot_count_or(id, bc.constants.len())?;
                let mapped = self.mapped_slots_for_object_without_copy(id, len)?;
                let data = load_bytecode_owned(self, bc, mapped)?;
                Self::install_restored_bytecode_data(value, data)?;
            }
            DumpHeapObject::Record(items) => {
                let len = self.mapped_slot_count_or(id, items.len())?;
                if let Some(storage) = self.mapped_slots_for_object_without_copy(id, len)? {
                    let _ = Self::install_mapped_record_slots(value, storage);
                } else {
                    let slots: Vec<_> = items.iter().map(|item| self.load_value(item)).collect();
                    if let Some(storage) = self.mapped_slots_for_object(id, &slots)? {
                        let _ = Self::install_mapped_record_slots(value, storage);
                    } else {
                        let _ = value.replace_record_data(slots);
                    }
                }
            }
            DumpHeapObject::Marker(marker) => {
                let _ = value.with_marker_data_mut(|data| {
                    data.buffer = marker.buffer.map(|id| BufferId(id.0));
                    // v26: bytepos/charpos round-trip directly.
                    data.bytepos = marker.bytepos;
                    data.charpos = marker.charpos;
                    data.insertion_type = marker.insertion_type;
                    data.marker_id = marker.marker_id;
                });
            }
            DumpHeapObject::Overlay(overlay) => {
                let _ = value.with_overlay_data_mut(|data| {
                    data.plist = self.load_value(&overlay.plist);
                    data.buffer = overlay.buffer.map(|id| BufferId(id.0));
                    data.start = overlay.start;
                    data.end = overlay.end;
                    data.front_advance = overlay.front_advance;
                    data.rear_advance = overlay.rear_advance;
                });
            }
            DumpHeapObject::Buffer(_)
            | DumpHeapObject::Window(_)
            | DumpHeapObject::Frame(_)
            | DumpHeapObject::Timer(_)
            | DumpHeapObject::Subr { .. }
            | DumpHeapObject::Free => {}
        }
        Ok(())
    }

    fn populate_value_graph(&mut self, root: &DumpValue) -> Result<(), DumpError> {
        let mut stack = vec![root.clone()];
        let mut seen = FxHashSet::default();
        while let Some(value) = stack.pop() {
            let Some(id) = dump_value_heap_ref(&value) else {
                continue;
            };
            if !seen.insert(id.index) {
                continue;
            }
            self.populate_tagged_object(id)?;
            match self
                .state
                .objects
                .get(id.index as usize)
                .cloned()
                .unwrap_or(DumpHeapObject::Free)
            {
                DumpHeapObject::Cons { car, cdr } => {
                    stack.push(car);
                    stack.push(cdr);
                }
                DumpHeapObject::Vector(items)
                | DumpHeapObject::SubCharTable {
                    contents: items, ..
                }
                | DumpHeapObject::Lambda(items)
                | DumpHeapObject::Macro(items)
                | DumpHeapObject::Record(items) => {
                    stack.extend(items);
                }
                DumpHeapObject::CharTable {
                    defalt,
                    parent,
                    purpose,
                    ascii,
                    contents,
                    extras,
                } => {
                    stack.push(defalt);
                    stack.push(parent);
                    stack.push(purpose);
                    stack.push(ascii);
                    stack.extend(contents);
                    stack.extend(extras);
                }
                DumpHeapObject::HashTable(ht) => {
                    for (_, value, snapshot) in ht.ordered_entries {
                        stack.push(value);
                        if let Some(snap) = snapshot {
                            stack.push(snap);
                        }
                    }
                }
                DumpHeapObject::Obarray { buckets, .. } => {
                    stack.extend(buckets);
                }
                DumpHeapObject::Str { text_props, .. } => {
                    for run in text_props {
                        stack.push(run.plist);
                    }
                }
                DumpHeapObject::ByteCode(bc) => {
                    stack.extend(bc.constants);
                    if let Some(arglist) = bc.arglist {
                        stack.push(arglist);
                    }
                    if let Some(env) = bc.env {
                        stack.push(env);
                    }
                    if let Some(doc_form) = bc.doc_form {
                        stack.push(doc_form);
                    }
                    if let Some(interactive) = bc.interactive {
                        stack.push(interactive);
                    }
                    stack.extend(bc.extra_slots);
                }
                DumpHeapObject::Overlay(overlay) => {
                    stack.push(overlay.plist);
                }
                DumpHeapObject::Float(_)
                | DumpHeapObject::Marker(_)
                | DumpHeapObject::Buffer(_)
                | DumpHeapObject::Window(_)
                | DumpHeapObject::Frame(_)
                | DumpHeapObject::Timer(_)
                | DumpHeapObject::Subr { .. }
                | DumpHeapObject::Free => {}
            }
        }
        Ok(())
    }

    pub(crate) fn load_value(&mut self, v: &DumpValue) -> Value {
        match v {
            DumpValue::Nil => Value::NIL,
            DumpValue::True => Value::T,
            DumpValue::Int(n) => Value::make_int(*n),
            DumpValue::Float(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::Symbol(s) => Value::symbol(load_sym_id(s)),
            DumpValue::Str(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::Cons(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::Vector(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::CharTable(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::SubCharTable(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::Record(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::HashTable(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::Obarray(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::Lambda(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::Macro(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::Subr(s) => {
                let name_id = load_name_id(s);
                // Convert NameId -> canonical SymId for the PVEC_SUBR-like
                // object constructor.
                if let Some(sym_id) = intern::canonical_symbol_for_name(name_id) {
                    Value::subr_from_sym_id(sym_id)
                } else {
                    // Fallback: intern the name to get a canonical SymId.
                    let name = intern::resolve_name(name_id);
                    Value::subr_from_sym_id(intern::intern(name))
                }
            }
            DumpValue::ByteCode(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::Marker(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::Overlay(id) => self.heap_ref_to_value(tagged_heap_ref(id)),
            DumpValue::Buffer(bid) => self.load_cached_buffer(bid.0),
            DumpValue::Window(w) => self.load_cached_window(*w),
            DumpValue::Frame(f) => self.load_cached_frame(*f),
            DumpValue::Timer(t) => self.load_cached_timer(*t),
            DumpValue::Bignum(text) => Value::make_integer_from_str_or_zero(text),
            DumpValue::Unbound => Value::UNBOUND,
        }
    }

    fn load_value_owned(&mut self, v: DumpValue) -> Value {
        match v {
            DumpValue::Nil => Value::NIL,
            DumpValue::True => Value::T,
            DumpValue::Int(n) => Value::make_int(n),
            DumpValue::Float(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::Symbol(s) => Value::symbol(load_sym_id(&s)),
            DumpValue::Str(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::Cons(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::Vector(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::CharTable(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::SubCharTable(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::Record(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::HashTable(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::Obarray(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::Lambda(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::Macro(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::Subr(s) => {
                let name_id = load_name_id(&s);
                if let Some(sym_id) = intern::canonical_symbol_for_name(name_id) {
                    Value::subr_from_sym_id(sym_id)
                } else {
                    let name = intern::resolve_name(name_id);
                    Value::subr_from_sym_id(intern::intern(name))
                }
            }
            DumpValue::ByteCode(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::Marker(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::Overlay(id) => self.heap_ref_to_value(tagged_heap_ref(&id)),
            DumpValue::Buffer(bid) => self.load_cached_buffer(bid.0),
            DumpValue::Window(w) => self.load_cached_window(w),
            DumpValue::Frame(f) => self.load_cached_frame(f),
            DumpValue::Timer(t) => self.load_cached_timer(t),
            DumpValue::Bignum(text) => Value::make_integer_from_str_or_zero(&text),
            DumpValue::Unbound => Value::UNBOUND,
        }
    }

    pub(crate) fn load_opt_value(&mut self, v: &Option<DumpValue>) -> Option<Value> {
        v.as_ref().map(|value| self.load_value(value))
    }

    fn load_opt_value_owned(&mut self, v: Option<DumpValue>) -> Option<Value> {
        v.map(|value| self.load_value_owned(value))
    }
}

fn restored_file_object_descriptor_is_discardable(object: &DumpHeapObject) -> bool {
    match object {
        DumpHeapObject::Free => true,
        DumpHeapObject::Vector(slots)
        | DumpHeapObject::Lambda(slots)
        | DumpHeapObject::Macro(slots)
        | DumpHeapObject::Record(slots) => slots.is_empty(),
        DumpHeapObject::Str {
            data, text_props, ..
        } => {
            text_props.is_empty()
                && matches!(
                    data,
                    DumpByteData::Mapped(_) | DumpByteData::StaticRoData { .. }
                )
        }
        _ => false,
    }
}

fn dump_value_heap_ref(value: &DumpValue) -> Option<TaggedHeapRef> {
    match value {
        DumpValue::Float(id)
        | DumpValue::Str(id)
        | DumpValue::Cons(id)
        | DumpValue::Vector(id)
        | DumpValue::CharTable(id)
        | DumpValue::SubCharTable(id)
        | DumpValue::Record(id)
        | DumpValue::HashTable(id)
        | DumpValue::Obarray(id)
        | DumpValue::Lambda(id)
        | DumpValue::Macro(id)
        | DumpValue::ByteCode(id)
        | DumpValue::Marker(id)
        | DumpValue::Overlay(id) => Some(tagged_heap_ref(id)),
        DumpValue::Nil
        | DumpValue::True
        | DumpValue::Int(_)
        | DumpValue::Symbol(_)
        | DumpValue::Subr(_)
        | DumpValue::Buffer(_)
        | DumpValue::Window(_)
        | DumpValue::Frame(_)
        | DumpValue::Timer(_)
        | DumpValue::Bignum(_)
        | DumpValue::Unbound => None,
    }
}

fn dump_heap_ref(id: TaggedHeapRef) -> DumpHeapRef {
    DumpHeapRef { index: id.index }
}

fn tagged_heap_ref(id: &DumpHeapRef) -> TaggedHeapRef {
    TaggedHeapRef { index: id.index }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_hash_keys_follow_the_consumers_integer_representation() {
        let (minimum, maximum) = crate::tagged::value::fixnum_bounds_for_word_bits(32);
        assert_eq!(
            load_integer_hash_key_for_word_bits(minimum, 32),
            HashKey::Int(minimum)
        );
        assert_eq!(
            load_integer_hash_key_for_word_bits(maximum, 32),
            HashKey::Int(maximum)
        );

        let integer = Value::MOST_POSITIVE_FIXNUM + 1;
        let serialized = DumpHashKey::Int(integer);
        let restored_bignum = Value::bignum(malachite::Integer::from(integer));
        assert!(matches!(
            load_integer_hash_key_for_word_bits(integer, 32),
            HashKey::Bignum(_)
        ));
        assert!(hash_key_requires_restored_identity(
            &serialized,
            HashTableTest::Eq,
            32
        ));
        assert!(!hash_key_requires_restored_identity(
            &serialized,
            HashTableTest::Eql,
            32
        ));

        assert!(matches!(
            rebuild_target_hash_key(
                &serialized,
                restored_bignum,
                HashTableTest::Eq,
                32,
            ),
            Some(HashKey::Ptr(pointer)) if pointer == restored_bignum.bits()
        ));
        assert_eq!(
            rebuild_target_hash_key(&serialized, restored_bignum, HashTableTest::Eql, 32,),
            None,
            "value-based tests are normalized directly from the serialized key"
        );

        let wasm32_bignum = maximum + 1;
        let limbs = malachite::Integer::from(wasm32_bignum).to_twos_complement_limbs_asc();
        let serialized_bignum = DumpHashKey::Bignum(limbs.clone());
        assert_eq!(
            load_bignum_hash_key_for_word_bits(&limbs, 64),
            HashKey::Int(wasm32_bignum),
            "a 32-bit producer bignum that fits a 64-bit fixnum must be normalized"
        );
        assert_eq!(
            rebuild_target_hash_key(
                &serialized_bignum,
                Value::fixnum(wasm32_bignum),
                HashTableTest::Eql,
                64,
            ),
            None
        );
    }

    #[test]
    fn cross_width_equal_key_does_not_depend_on_object_population_order() {
        crate::test_utils::init_test_tracing();
        let integer = Value::MOST_POSITIVE_FIXNUM + 1;
        let key_ref = DumpHeapRef { index: 1 };
        let heap = DumpTaggedHeap {
            objects: vec![
                DumpHeapObject::HashTable(DumpLispHashTable {
                    test: DumpHashTableTest::Equal,
                    test_name: None,
                    size: 1,
                    weakness: None,
                    rehash_size: 1.5,
                    rehash_threshold: 0.8125,
                    ordered_entries: vec![(
                        DumpHashKey::EqualCons(
                            Box::new(DumpHashKey::Int(integer)),
                            Box::new(DumpHashKey::Nil),
                        ),
                        DumpValue::True,
                        Some(DumpValue::Cons(key_ref.clone())),
                    )],
                }),
                DumpHeapObject::Cons {
                    car: DumpValue::Int(integer),
                    cdr: DumpValue::Nil,
                },
            ],
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };
        let mut decoder = LoadDecoder::new(&heap);
        decoder.preload_tagged_heap().unwrap();

        let table = decoder.load_value(&DumpValue::HashTable(DumpHeapRef { index: 0 }));
        let lookup =
            Value::cons(Value::make_int(integer), Value::NIL).to_hash_key(&HashTableTest::Equal);
        assert_eq!(
            table
                .as_hash_table()
                .and_then(|table| table.data.get(&lookup))
                .copied(),
            Some(Value::T),
            "the key index must be derived without traversing an unpopulated cons placeholder"
        );
    }

    #[test]
    fn preload_tagged_heap_handles_deep_cons_chains_without_recursive_population() {
        crate::test_utils::init_test_tracing();

        let chain_len = 4096usize;
        let objects = (0..chain_len)
            .map(|index| DumpHeapObject::Cons {
                car: DumpValue::Int(index as i64),
                cdr: if index + 1 == chain_len {
                    DumpValue::Nil
                } else {
                    DumpValue::Cons(DumpHeapRef {
                        index: (index + 1) as u32,
                    })
                },
            })
            .collect();
        let heap = DumpTaggedHeap {
            objects,
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };
        let mut decoder = LoadDecoder::new(&heap);

        decoder
            .preload_tagged_heap()
            .expect("deep cons chain should preload without recursive overflow");

        let mut cursor = decoder.load_value(&DumpValue::Cons(DumpHeapRef { index: 0 }));
        for index in 0..chain_len {
            assert_eq!(cursor.cons_car(), Value::fixnum(index as i64));
            cursor = cursor.cons_cdr();
        }
        assert!(cursor.is_nil());
    }

    #[test]
    fn mapped_cons_raw_words_are_loader_source_of_truth_when_no_remap_needed() {
        crate::test_utils::init_test_tracing();
        let mut runtime_heap = Box::new(crate::tagged::gc::TaggedHeap::new());
        crate::tagged::gc::set_tagged_heap(&mut runtime_heap);

        let heap = DumpTaggedHeap {
            objects: vec![DumpHeapObject::Cons {
                car: DumpValue::Int(1),
                cdr: DumpValue::Int(2),
            }],
            mapped_cons: vec![Some(DumpConsSpan { offset: 0 })],
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };
        let mut bytes = vec![0u8; std::mem::size_of::<ConsCell>()];
        write_raw_word(&mut bytes, 0, Value::fixnum(99).bits());
        write_raw_word(
            &mut bytes,
            std::mem::size_of::<TaggedValue>(),
            Value::fixnum(100).bits(),
        );

        let mapped = MappedHeapView::from_mut_slice(&mut bytes);
        let mut decoder = LoadDecoder::new_with_mapped_heap(&heap, Some(mapped));
        decoder.preload_tagged_heap().unwrap();

        assert!(
            decoder.state.cached_value(0).is_none(),
            "mapped cons cells should stay out of the eager load cache"
        );
        assert!(
            !decoder.state.is_populated(0),
            "mapped cons cells should not run descriptor population"
        );

        let value = decoder.load_value(&DumpValue::Cons(DumpHeapRef { index: 0 }));
        assert_eq!(value.cons_car(), Value::fixnum(99));
        assert_eq!(value.cons_cdr(), Value::fixnum(100));
    }

    #[test]
    fn mapped_vector_raw_slots_are_loader_source_of_truth_when_no_remap_needed() {
        crate::test_utils::init_test_tracing();
        let mut runtime_heap = Box::new(crate::tagged::gc::TaggedHeap::new());
        crate::tagged::gc::set_tagged_heap(&mut runtime_heap);

        let slot_offset = std::mem::size_of::<VectorObj>();
        let heap = DumpTaggedHeap {
            objects: vec![DumpHeapObject::Vector(vec![
                DumpValue::Int(1),
                DumpValue::Int(2),
            ])],
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: vec![Some(DumpVecLikeSpan {
                offset: 0,
                len: std::mem::size_of::<VectorObj>() as u64,
            })],
            mapped_slots: vec![Some(DumpSlotSpan {
                offset: slot_offset as u64,
                len: 2,
            })],
        };
        let mut bytes = vec![0u8; slot_offset + 2 * std::mem::size_of::<TaggedValue>()];
        write_raw_word(&mut bytes, slot_offset, Value::fixnum(77).bits());
        write_raw_word(
            &mut bytes,
            slot_offset + std::mem::size_of::<TaggedValue>(),
            Value::fixnum(88).bits(),
        );

        let mapped = MappedHeapView::from_mut_slice(&mut bytes);
        let mut decoder = LoadDecoder::new_with_mapped_heap(&heap, Some(mapped));
        decoder.preload_tagged_heap().unwrap();

        assert!(
            decoder.state.cached_value(0).is_some(),
            "mapped vector object headers still need one load-time wrapper initialization"
        );
        assert!(
            !decoder.state.is_populated(0),
            "mapped vector slots should not run the descriptor population pass"
        );

        let value = decoder.load_value(&DumpValue::Vector(DumpHeapRef { index: 0 }));
        let slots = value.as_vector_data().unwrap();
        assert_eq!(slots.as_slice(), &[Value::fixnum(77), Value::fixnum(88)]);
    }

    #[test]
    fn mapped_vector_slot_count_comes_from_span_for_compact_descriptors() {
        crate::test_utils::init_test_tracing();
        let mut runtime_heap = Box::new(crate::tagged::gc::TaggedHeap::new());
        crate::tagged::gc::set_tagged_heap(&mut runtime_heap);

        let slot_offset = std::mem::size_of::<VectorObj>();
        let heap = DumpTaggedHeap {
            objects: vec![DumpHeapObject::Vector(Vec::new())],
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: vec![Some(DumpVecLikeSpan {
                offset: 0,
                len: std::mem::size_of::<VectorObj>() as u64,
            })],
            mapped_slots: vec![Some(DumpSlotSpan {
                offset: slot_offset as u64,
                len: 2,
            })],
        };
        let mut bytes = vec![0u8; slot_offset + 2 * std::mem::size_of::<TaggedValue>()];
        write_raw_word(&mut bytes, slot_offset, Value::fixnum(177).bits());
        write_raw_word(
            &mut bytes,
            slot_offset + std::mem::size_of::<TaggedValue>(),
            Value::fixnum(188).bits(),
        );

        let mapped = MappedHeapView::from_mut_slice(&mut bytes);
        let mut decoder = LoadDecoder::new_with_mapped_heap(&heap, Some(mapped));
        decoder.preload_tagged_heap().unwrap();

        let value = decoder.load_value(&DumpValue::Vector(DumpHeapRef { index: 0 }));
        let slots = value.as_vector_data().unwrap();
        assert_eq!(slots.as_slice(), &[Value::fixnum(177), Value::fixnum(188)]);
    }

    #[test]
    fn load_hash_table_makes_all_dumped_entries_iterable() {
        crate::test_utils::init_test_tracing();

        let heap = DumpTaggedHeap {
            objects: Vec::new(),
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };
        let mut decoder = LoadDecoder::new(&heap);
        let table = load_hash_table(
            &mut decoder,
            &DumpLispHashTable {
                test: DumpHashTableTest::Eq,
                test_name: None,
                size: 0,
                weakness: None,
                rehash_size: 1.5,
                rehash_threshold: 0.8125,
                ordered_entries: vec![
                    (DumpHashKey::Int(1), DumpValue::Int(1), None),
                    (DumpHashKey::Int(2), DumpValue::Int(2), None),
                ],
            },
        );

        assert_eq!(table.data.len(), 2);
        assert_eq!(
            table.live_hash_keys_in_slot_order().len(),
            2,
            "loaded hash tables must keep GNU maphash traversal in sync with live entries"
        );
    }

    #[test]
    fn cross_width_eq_key_uses_entry_value_when_snapshot_is_elided() {
        crate::test_utils::init_test_tracing();

        let integer = Value::MOST_POSITIVE_FIXNUM + 1;
        let heap = DumpTaggedHeap {
            objects: Vec::new(),
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        };
        let mut decoder = LoadDecoder::new(&heap);
        let table = load_hash_table(
            &mut decoder,
            &DumpLispHashTable {
                test: DumpHashTableTest::Eq,
                test_name: None,
                size: 1,
                weakness: None,
                rehash_size: 1.5,
                rehash_threshold: 0.8125,
                ordered_entries: vec![(DumpHashKey::Int(integer), DumpValue::Int(integer), None)],
            },
        );

        let entry = table
            .entries_in_slot_order()
            .next()
            .expect("the restored entry must remain iterable");
        assert_eq!(entry.key, entry.value);
        assert_eq!(
            table.data.get(&HashKey::Ptr(entry.key.bits())),
            Some(&entry.value),
            "None means the entry value is also the original Lisp key"
        );
    }

    #[test]
    fn producer_shaped_eq_bignum_key_demotes_to_consumer_fixnum() {
        crate::test_utils::init_test_tracing();

        let (_, wasm32_maximum) = crate::tagged::value::fixnum_bounds_for_word_bits(32);
        let integer = wasm32_maximum + 1;
        let producer_key = Value::bignum(Integer::from(integer));
        let mut encoder = DumpEncoder::new();
        let serialized_key =
            dump_hash_key(&mut encoder, &producer_key.to_hash_key(&HashTableTest::Eq));
        assert!(
            matches!(serialized_key, DumpHashKey::HeapRef(_)),
            "an eq bignum is serialized from its pointer-identity key"
        );

        let heap = encoder.finalize();
        let mut decoder = LoadDecoder::new(&heap);
        let table = load_hash_table(
            &mut decoder,
            &DumpLispHashTable {
                test: DumpHashTableTest::Eq,
                test_name: None,
                size: 1,
                weakness: None,
                rehash_size: 1.5,
                rehash_threshold: 0.8125,
                ordered_entries: vec![(
                    serialized_key,
                    DumpValue::True,
                    Some(DumpValue::Bignum(integer.to_string())),
                )],
            },
        );

        assert_eq!(
            table.data.get(&HashKey::Int(integer)),
            Some(&Value::T),
            "the 64-bit consumer must use its immediate fixnum identity"
        );
    }

    fn write_raw_word(bytes: &mut [u8], offset: usize, word: usize) {
        bytes[offset..offset + std::mem::size_of::<usize>()].copy_from_slice(&word.to_ne_bytes());
    }
}

// ===========================================================================
// Dump direction: Runtime → Dump
// ===========================================================================

// --- Primitives ---

pub(crate) fn dump_sym_id(id: SymId) -> DumpSymId {
    DumpSymId(id.0)
}

pub(crate) fn dump_name_id(id: NameId) -> DumpNameId {
    DumpNameId(id.0)
}

fn dump_lisp_string(string: &LispString) -> DumpLispString {
    DumpLispString {
        data: string.as_bytes().to_vec(),
        size: string.schars(),
        size_byte: string.size_byte(),
    }
}

/// Build the full ByteCodeFunction for a LAZY pdump stub, reading everything
/// from the mapped image via the object's own address — no loader state.
///
/// Preconditions (all established at load by the stub-finalize pass):
/// the object is a mapped `ByteCodeObj` whose data `is_pdump_stub()`;
/// `closure_slot_count` carries the extras length; the extras region is
/// bounds-validated; param-id words hold RUNTIME SymIds (identity loads by
/// construction, fallback loads rewrote them in place); relocations and
/// value fixups are long applied. Runs on the mutator thread only.
///
/// # Safety
/// `value` must satisfy the preconditions above.
pub(crate) unsafe fn materialize_bytecode_from_extras_at(value: Value) -> ByteCodeFunction {
    use crate::emacs_core::pdump::mapped_heap::{
        BC_FLAG_HAS_ARGLIST, BC_FLAG_HAS_DOC_FORM, BC_FLAG_HAS_DOCSTRING, BC_FLAG_HAS_ENV,
        BC_FLAG_HAS_GNU, BC_FLAG_HAS_INTERACTIVE, BC_FLAG_HAS_REST, BC_FLAG_LEXICAL,
        BC_FLAG_OPS_SEALED, BytecodeExtras,
    };
    let ptr = value
        .as_veclike_ptr()
        .expect("stub materialization requires a bytecode object")
        as *const crate::tagged::header::ByteCodeObj;
    let base = ptr as *const u8;
    let extras_len = unsafe { (*ptr).data.closure_slot_count };
    let extras_ptr = unsafe { base.add(std::mem::size_of::<crate::tagged::header::ByteCodeObj>()) };
    let bytes = unsafe { std::slice::from_raw_parts(extras_ptr, extras_len) };
    let header_len = std::mem::size_of::<BytecodeExtras>();
    let header: BytecodeExtras = bytemuck::pod_read_unaligned(&bytes[..header_len]);
    let flags = header.flags;

    let n_ids = header.n_required as usize + header.n_optional as usize;
    let ids_end = header_len + n_ids * 4;
    let mut ids = Vec::with_capacity(n_ids);
    for chunk in bytes[header_len..ids_end].chunks_exact(4) {
        // ALREADY-RUNTIME ids: no remap survives to first call, by design.
        ids.push(crate::emacs_core::intern::SymId(u32::from_le_bytes([
            chunk[0], chunk[1], chunk[2], chunk[3],
        ])));
    }
    let optional = ids.split_off(header.n_required as usize);
    let params = LambdaParams {
        required: ids,
        optional,
        rest: (flags & BC_FLAG_HAS_REST != 0)
            .then(|| crate::emacs_core::intern::SymId(header.rest_sym)),
    };

    let mut cursor = (ids_end + 7) & !7;
    let n_extra = header.n_extra_slots as usize;
    let extra_end = cursor + n_extra * 8;
    let mut extra_slots = Vec::with_capacity(n_extra);
    for chunk in bytes[cursor..extra_end].chunks_exact(8) {
        let word = u64::from_ne_bytes([
            chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
        ]);
        extra_slots.push(Value::from_bits(word as usize));
    }
    cursor = extra_end;

    let docstring = (flags & BC_FLAG_HAS_DOCSTRING != 0).then(|| {
        let size_byte = header.docstring_size_byte;
        let doc_len = if size_byte >= 0 {
            size_byte as usize
        } else {
            header.docstring_size as usize
        };
        load_lisp_string(&super::types::DumpLispString {
            data: bytes[cursor..cursor + doc_len].to_vec(),
            size: header.docstring_size as usize,
            size_byte,
        })
    });

    let gnu_bytecode_bytes = (flags & BC_FLAG_HAS_GNU != 0).then(|| {
        let gnu_ptr = unsafe { base.offset(header.gnu_rel as isize) };
        unsafe { crate::tagged::header::LispByteVec::mapped(gnu_ptr, header.gnu_len as usize) }
    });

    let constants = if header.const_count > 0 {
        let slots_ptr = unsafe { base.offset(header.const_rel as isize) }
            .cast::<crate::tagged::value::TaggedValue>();
        unsafe {
            crate::tagged::header::LispValueVec::mapped(slots_ptr, header.const_count as usize)
        }
    } else {
        Vec::new().into()
    };

    let word_value = |word: u64| Value::from_bits(word as usize);
    let arglist = if flags & BC_FLAG_HAS_ARGLIST != 0 {
        word_value(header.arglist_word)
    } else {
        // Fresh allocation at materialization time: born live under any
        // in-progress cycle (allocation coloring), so the stub walker's
        // "no synthesized child" stance is correct by construction.
        crate::emacs_core::builtins::lambda_params_to_value(&params)
    };

    let mut function = ByteCodeFunction {
        source_id: crate::emacs_core::bytecode::fresh_bytecode_source_id(),
        ops: Vec::new(),
        stack_verified: false,
        constants,
        max_stack: header.max_stack,
        params,
        arglist,
        lexical: flags & BC_FLAG_LEXICAL != 0,
        env: (flags & BC_FLAG_HAS_ENV != 0).then(|| word_value(header.env_word)),
        gnu_byte_offset_map: None,
        gnu_bytecode_bytes,
        docstring,
        doc_form: (flags & BC_FLAG_HAS_DOC_FORM != 0).then(|| word_value(header.doc_form_word)),
        interactive: (flags & BC_FLAG_HAS_INTERACTIVE != 0)
            .then(|| word_value(header.interactive_word)),
        closure_slot_count: header.closure_slot_count as usize,
        extra_slots,
        ops_sealed: flags & BC_FLAG_OPS_SEALED != 0,
        #[cfg(feature = "jit")]
        runtime: Some(crate::emacs_core::jit::Runtime::new()),
        lazy_gnu_code: None,
    };
    if function.gnu_bytecode_bytes.is_some() {
        function
            .restore_gnu_decode_policy()
            .unwrap_or_else(|error| {
                // Structural bounds were validated at load; a decode-level
                // failure here is image corruption discovered on first call.
                panic!("pdump bytecode stub failed GNU decode at materialization: {error}")
            });
    }
    function
}

/// The sanctioned post-publish mutation of a mapped `ByteCodeObj`:
/// materialize a lazy stub in place. Mutator-only; once-guarded by the
/// stub discriminator; the SATB pre-image (the stub's image-word children)
/// is logged via `note_heap_write` BEFORE the write so a mid-cycle
/// materialization loses no child.
#[cold]
#[inline(never)]
pub(crate) fn materialize_and_publish_stub(value: Value) {
    let ptr = value
        .as_veclike_ptr()
        .expect("stub materialization requires a bytecode object")
        as *mut crate::tagged::header::ByteCodeObj;
    // Once-guard: the single mutator thread is the only materializer; a
    // second look after the first publish sees a non-stub and returns.
    if unsafe { !(*ptr).data.is_pdump_stub() } {
        return;
    }
    crate::tagged::gc::note_heap_write(
        crate::tagged::value::TaggedValue::from_bits(value.bits()),
        crate::tagged::gc::HeapWriteKind::ByteCodeData,
    );
    let function = unsafe { materialize_bytecode_from_extras_at(value) };
    // Single whole-data publish; dropping the stub releases only empty
    // vectors and the shared stub Runtime's refcount.
    unsafe {
        (*ptr).data = function;
    }
}

/// The command-classification facts of a LAZY stub, from the raw mapped
/// extras header — replicating `observable_closure_slot_count` exactly (the
/// materialized twin's arithmetic, chunk.rs) without materializing.
///
/// # Safety
/// Same preconditions as [`materialize_bytecode_from_extras_at`].
pub(crate) unsafe fn stub_interactive_probe(
    obj: *const crate::tagged::header::ByteCodeObj,
    extras_len: usize,
) -> crate::emacs_core::value::BytecodeInteractiveProbe {
    use crate::emacs_core::pdump::mapped_heap::{
        BC_FLAG_HAS_DOC_FORM, BC_FLAG_HAS_DOCSTRING, BC_FLAG_HAS_INTERACTIVE, BytecodeExtras,
    };
    let base = obj as *const u8;
    let extras_ptr = unsafe { base.add(std::mem::size_of::<crate::tagged::header::ByteCodeObj>()) };
    debug_assert!(extras_len >= std::mem::size_of::<BytecodeExtras>());
    let header: BytecodeExtras =
        unsafe { std::ptr::read_unaligned(extras_ptr.cast::<BytecodeExtras>()) };
    let flags = header.flags;
    let mut count = (header.closure_slot_count as usize).max(4);
    if flags & (BC_FLAG_HAS_DOCSTRING | BC_FLAG_HAS_DOC_FORM) != 0 {
        count = count.max(5);
    }
    if flags & BC_FLAG_HAS_INTERACTIVE != 0 {
        count = count.max(6);
    }
    if header.n_extra_slots > 0 {
        count = count.max(6 + header.n_extra_slots as usize);
    }
    let word_value = |word: u64| Value::from_bits(word as usize);
    crate::emacs_core::value::BytecodeInteractiveProbe {
        slot_count: count,
        interactive: (flags & BC_FLAG_HAS_INTERACTIVE != 0)
            .then(|| word_value(header.interactive_word)),
        doc_form: (flags & BC_FLAG_HAS_DOC_FORM != 0).then(|| word_value(header.doc_form_word)),
    }
}

/// Raw-header check: does a LAZY stub declare a required-only signature?
/// (`n_optional == 0 && !HAS_REST` — provably equivalent to the materialized
/// `params.optional.is_empty() && params.rest.is_none()`.) Lets the AOT
/// mass scans reject ineligible functions without materializing them.
///
/// # Safety
/// Same preconditions as [`materialize_bytecode_from_extras_at`].
pub(crate) unsafe fn stub_params_required_only(
    obj: *const crate::tagged::header::ByteCodeObj,
    extras_len: usize,
) -> bool {
    use crate::emacs_core::pdump::mapped_heap::{BC_FLAG_HAS_REST, BytecodeExtras};
    debug_assert!(extras_len >= std::mem::size_of::<BytecodeExtras>());
    let extras_ptr = unsafe {
        (obj as *const u8).add(std::mem::size_of::<crate::tagged::header::ByteCodeObj>())
    };
    let header: BytecodeExtras =
        unsafe { std::ptr::read_unaligned(extras_ptr.cast::<BytecodeExtras>()) };
    header.n_optional == 0 && header.flags & BC_FLAG_HAS_REST == 0
}

pub(super) fn load_lisp_string(dump: &DumpLispString) -> LispString {
    // One spare slot so the constructor's trailing-NUL push cannot force a
    // realloc + full re-copy of an exact-capacity clone.
    let mut data = Vec::with_capacity(dump.data.len() + 1);
    data.extend_from_slice(&dump.data);
    LispString::from_dump(data, dump.size, dump.size_byte)
}

fn load_lisp_string_owned(dump: DumpLispString) -> LispString {
    LispString::from_dump(dump.data, dump.size, dump.size_byte)
}

// --- Op ---

// --- Lambda / ByteCode ---

pub(crate) fn dump_lambda_params(p: &LambdaParams) -> DumpLambdaParams {
    DumpLambdaParams {
        required: p.required.iter().map(|s| dump_sym_id(*s)).collect(),
        optional: p.optional.iter().map(|s| dump_sym_id(*s)).collect(),
        rest: p.rest.map(dump_sym_id),
    }
}

pub(crate) fn dump_bytecode(
    encoder: &mut DumpEncoder,
    bc: &ByteCodeFunction,
) -> DumpByteCodeFunction {
    let instructions = match &bc.gnu_bytecode_bytes {
        Some(bytes) => {
            DumpByteCodeInstructions::Gnu(DumpByteData::owned(bytes.as_slice().to_vec()))
        }
        None => DumpByteCodeInstructions::Decoded(bc.executable_ops().to_vec()),
    };
    DumpByteCodeFunction {
        instructions,
        constants: bc
            .constants
            .iter()
            .map(|value| encoder.dump_value(value))
            .collect(),
        max_stack: bc.max_stack,
        params: dump_lambda_params(&bc.params),
        arglist: Some(encoder.dump_value(&bc.arglist)),
        lexical: bc.lexical,
        env: encoder.dump_opt_value(&bc.env),
        docstring: bc.docstring.as_ref().map(dump_lisp_string),
        doc_form: encoder.dump_opt_value(&bc.doc_form),
        interactive: encoder.dump_opt_value(&bc.interactive),
        closure_slot_count: bc.observable_closure_slot_count(),
        extra_slots: bc
            .extra_slots
            .iter()
            .map(|value| encoder.dump_value(value))
            .collect(),
        ops_sealed: bc.ops_sealed,
    }
}

// --- Hash tables ---

pub(crate) fn dump_hash_key(encoder: &mut DumpEncoder, k: &HashKey) -> DumpHashKey {
    match k {
        HashKey::Nil => DumpHashKey::Nil,
        HashKey::True => DumpHashKey::True,
        HashKey::Int(n) => DumpHashKey::Int(*n),
        HashKey::Bignum(limbs) => DumpHashKey::Bignum(limbs.to_vec()),
        HashKey::Float(bits) => DumpHashKey::Float(*bits),
        HashKey::FloatEq(bits, id) => DumpHashKey::FloatEq(*bits, *id),
        HashKey::Symbol(s) => DumpHashKey::Symbol(dump_sym_id(*s)),
        HashKey::Keyword(s) => DumpHashKey::Keyword(dump_sym_id(*s)),
        HashKey::Char(c) => DumpHashKey::Char(*c),
        HashKey::Window(w) => DumpHashKey::Window(*w),
        HashKey::Frame(f) => DumpHashKey::Frame(*f),
        HashKey::Ptr(p) => {
            let value = TaggedValue(*p);
            if value.is_heap_object() {
                let id = encoder.value_to_heap_ref(&value);
                DumpHashKey::HeapRef(id.index)
            } else {
                DumpHashKey::Ptr(*p as u64)
            }
        }
        HashKey::EqualCons(a, b) => DumpHashKey::EqualCons(
            Box::new(dump_hash_key(encoder, a)),
            Box::new(dump_hash_key(encoder, b)),
        ),
        HashKey::EqualVec(v) => {
            DumpHashKey::EqualVec(v.iter().map(|key| dump_hash_key(encoder, key)).collect())
        }
        HashKey::ByteCode(parts) => DumpHashKey::ByteCode(
            parts
                .iter()
                .map(|part| dump_byte_code_key_part(encoder, part))
                .collect(),
        ),
        HashKey::Marker(parts) => DumpHashKey::Marker(parts.0, parts.1.get()),
        HashKey::Overlay(parts) => DumpHashKey::Overlay {
            buffer: parts.0,
            start: parts.1,
            end: parts.2,
            plist: Box::new(dump_hash_key(encoder, &parts.3)),
        },
        HashKey::BoolVec(parts) => DumpHashKey::BoolVec {
            len: parts.0 as u32,
            bits: parts.1,
        },
        HashKey::SymbolWithPos(sym, pos) => DumpHashKey::SymbolWithPos(
            Box::new(dump_hash_key(encoder, sym)),
            Box::new(dump_hash_key(encoder, pos)),
        ),
        HashKey::Cycle(index) => DumpHashKey::Cycle(*index),
        HashKey::Text(text) => DumpHashKey::Text(text.to_string()),
    }
}

fn dump_byte_code_key_part(
    encoder: &mut DumpEncoder,
    part: &ByteCodeKeyPart,
) -> DumpByteCodeKeyPart {
    match part {
        ByteCodeKeyPart::ObservableSlotCount(count) => {
            DumpByteCodeKeyPart::ObservableSlotCount(*count)
        }
        ByteCodeKeyPart::Value(value) => DumpByteCodeKeyPart::Value(dump_hash_key(encoder, value)),
        ByteCodeKeyPart::Bytes(bytes) => DumpByteCodeKeyPart::Bytes(bytes.to_vec()),
        ByteCodeKeyPart::Ops(ops) => DumpByteCodeKeyPart::Ops(ops.to_vec()),
        ByteCodeKeyPart::Values(values) => DumpByteCodeKeyPart::Values(
            values
                .iter()
                .map(|value| dump_hash_key(encoder, value))
                .collect(),
        ),
        ByteCodeKeyPart::Text { char_count, bytes } => DumpByteCodeKeyPart::Text {
            char_count: *char_count,
            bytes: bytes.to_vec(),
        },
        ByteCodeKeyPart::Absent => DumpByteCodeKeyPart::Absent,
    }
}

pub(crate) fn dump_hash_table_test(t: &HashTableTest) -> DumpHashTableTest {
    match t {
        HashTableTest::Eq => DumpHashTableTest::Eq,
        HashTableTest::Eql => DumpHashTableTest::Eql,
        HashTableTest::Equal => DumpHashTableTest::Equal,
    }
}

pub(crate) fn dump_hash_table_weakness(w: &HashTableWeakness) -> DumpHashTableWeakness {
    match w {
        HashTableWeakness::Key => DumpHashTableWeakness::Key,
        HashTableWeakness::Value => DumpHashTableWeakness::Value,
        HashTableWeakness::KeyOrValue => DumpHashTableWeakness::KeyOrValue,
        HashTableWeakness::KeyAndValue => DumpHashTableWeakness::KeyAndValue,
    }
}

pub(crate) fn dump_hash_table(encoder: &mut DumpEncoder, ht: &LispHashTable) -> DumpLispHashTable {
    DumpLispHashTable {
        test: dump_hash_table_test(&ht.test),
        test_name: ht.test_name.map(dump_sym_id),
        size: ht.size,
        weakness: ht.weakness.as_ref().map(dump_hash_table_weakness),
        rehash_size: ht.rehash_size,
        rehash_threshold: ht.rehash_threshold,
        // A dump-loaded table that was never touched still holds its parked
        // entries (lazy hydration); re-dump them directly - hydrating here
        // would mutate through a shared reference obtained from the raw heap
        // walk.
        ordered_entries: if let Some(pending) = ht.data.pending_entries() {
            pending
                .iter()
                .map(|(key, value, snapshot)| {
                    (
                        dump_hash_key(encoder, key),
                        encoder.dump_value(value),
                        snapshot.as_ref().map(|snap| encoder.dump_value(snap)),
                    )
                })
                .collect()
        } else {
            ht.live_hash_keys_in_slot_order()
                .into_iter()
                .filter_map(|key| {
                    let value = ht.data.get(key).copied()?;
                    let snapshot = ht
                        .key_snapshot(key)
                        .copied()
                        .map(|snap| encoder.dump_value(&snap));
                    Some((
                        dump_hash_key(encoder, key),
                        encoder.dump_value(&value),
                        snapshot,
                    ))
                })
                .collect()
        },
    }
}

// --- Heap objects ---

fn dump_closure_slots(encoder: &mut DumpEncoder, value: Value) -> Vec<DumpValue> {
    value
        .closure_slots()
        .map(|slots| slots.iter().map(|slot| encoder.dump_value(slot)).collect())
        .unwrap_or_default()
}

fn dump_heap_object_from_value(encoder: &mut DumpEncoder, value: Value) -> DumpHeapObject {
    match value.kind() {
        ValueKind::Cons => DumpHeapObject::Cons {
            car: encoder.dump_value(&value.cons_car()),
            cdr: encoder.dump_value(&value.cons_cdr()),
        },
        ValueKind::String => {
            let string = value.as_lisp_string().expect("string");
            let data = if string.is_rodata() {
                let key = string
                    .rodata_key()
                    .expect("rodata strings must carry a static rodata key");
                DumpByteData::static_rodata(
                    key,
                    u64::try_from(string.sbytes()).expect("string byte length should fit into u64"),
                )
            } else {
                DumpByteData::owned(string.as_bytes().to_vec())
            };
            DumpHeapObject::Str {
                data,
                size: string.schars(),
                size_byte: string.size_byte(),
                text_props: get_string_text_properties_for_value(value)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|run| DumpStringTextPropertyRun {
                        start: run.start,
                        end: run.end,
                        plist: encoder.dump_value(&run.plist),
                    })
                    .collect(),
            }
        }
        ValueKind::Float => DumpHeapObject::Float(value.xfloat()),
        ValueKind::Veclike(VecLikeType::Vector) => DumpHeapObject::Vector(
            value
                .as_vector_data()
                .expect("vector")
                .iter()
                .map(|item| encoder.dump_value(item))
                .collect(),
        ),
        ValueKind::Veclike(VecLikeType::CharTable) => {
            let slots = char_table_external_slots(&value).expect("char-table");
            let mut iter = slots.into_iter();
            let defalt = encoder.dump_value(&iter.next().expect("char-table default slot"));
            let parent = encoder.dump_value(&iter.next().expect("char-table parent slot"));
            let purpose = encoder.dump_value(&iter.next().expect("char-table purpose slot"));
            let ascii = encoder.dump_value(&iter.next().expect("char-table ascii slot"));
            let rest: Vec<_> = iter.collect();
            let (contents, extras) = rest.split_at(64);
            DumpHeapObject::CharTable {
                defalt,
                parent,
                purpose,
                ascii,
                contents: contents
                    .iter()
                    .map(|item| encoder.dump_value(item))
                    .collect(),
                extras: extras.iter().map(|item| encoder.dump_value(item)).collect(),
            }
        }
        ValueKind::Veclike(VecLikeType::SubCharTable) => {
            let (depth, min_char, contents) =
                sub_char_table_external_slots(&value).expect("sub-char-table");
            DumpHeapObject::SubCharTable {
                depth,
                min_char,
                contents: contents
                    .iter()
                    .map(|item| encoder.dump_value(item))
                    .collect(),
            }
        }
        ValueKind::Veclike(VecLikeType::HashTable) => DumpHeapObject::HashTable(dump_hash_table(
            encoder,
            value.as_hash_table().expect("hash-table"),
        )),
        ValueKind::Veclike(VecLikeType::Obarray) => {
            let obarray = value.as_obarray_obj().expect("obarray");
            DumpHeapObject::Obarray {
                buckets: obarray
                    .buckets
                    .iter()
                    .map(|item| encoder.dump_value(item))
                    .collect(),
                count: obarray.count,
            }
        }
        ValueKind::Veclike(VecLikeType::Lambda) => {
            DumpHeapObject::Lambda(dump_closure_slots(encoder, value))
        }
        ValueKind::Veclike(VecLikeType::Macro) => {
            DumpHeapObject::Macro(dump_closure_slots(encoder, value))
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => DumpHeapObject::ByteCode(dump_bytecode(
            encoder,
            value.get_bytecode_data().expect("bytecode"),
        )),
        ValueKind::Veclike(VecLikeType::Record) => DumpHeapObject::Record(
            value
                .as_record_data()
                .expect("record")
                .iter()
                .map(|item| encoder.dump_value(item))
                .collect(),
        ),
        // A window-configuration is structurally a record; serialize its slots
        // the same way. (Runtime-only objects are not part of the loadup dump,
        // but this keeps the encoder total instead of silently dumping `Free`.)
        ValueKind::Veclike(VecLikeType::WindowConfiguration) => DumpHeapObject::Record(
            value
                .as_window_configuration_data()
                .expect("window-configuration")
                .iter()
                .map(|item| encoder.dump_value(item))
                .collect(),
        ),
        ValueKind::Veclike(VecLikeType::Overlay) => DumpHeapObject::Overlay(dump_overlay(
            encoder,
            value.as_overlay_data().expect("overlay"),
        )),
        ValueKind::Veclike(VecLikeType::Marker) => {
            DumpHeapObject::Marker(dump_marker_object(value.as_marker_data().expect("marker")))
        }
        ValueKind::Veclike(VecLikeType::Buffer) => {
            DumpHeapObject::Buffer(DumpBufferId(value.as_buffer_id().expect("buffer").0))
        }
        ValueKind::Veclike(VecLikeType::Window) => {
            DumpHeapObject::Window(value.as_window_id().expect("window"))
        }
        ValueKind::Veclike(VecLikeType::Frame) => {
            DumpHeapObject::Frame(value.as_frame_id().expect("frame"))
        }
        ValueKind::Veclike(VecLikeType::Timer) => {
            DumpHeapObject::Timer(value.as_timer_id().expect("timer"))
        }
        ValueKind::Veclike(VecLikeType::Xwidget) | ValueKind::Veclike(VecLikeType::XwidgetView) => {
            panic!("pdump: xwidget objects are not portable")
        }
        // Explicit (not the `Free` fallback): a surface handle wraps live GPU
        // objects owned by the host render thread and must never be silently
        // dumped as `Free`.
        ValueKind::Veclike(VecLikeType::SurfaceHandle) => {
            panic!("pdump: shader-surface handles are not portable")
        }
        ValueKind::Veclike(VecLikeType::VideoHandle) => {
            panic!("pdump: video-session handles are not portable")
        }
        // Explicit (not the `Free` fallback) so a live finalizer can never be
        // silently dropped from an image. `dump-emacs-portable` pre-scans the
        // finalizer registry and signals an elisp error before writing, so
        // this is an unreachable backstop for non-builtin dump entry points.
        ValueKind::Veclike(VecLikeType::Finalizer) => {
            panic!("pdump: cannot dump finalizer objects")
        }
        ValueKind::Veclike(VecLikeType::Subr) => {
            let ptr = value.as_veclike_ptr().expect("subr") as *const SubrObj;
            let subr = unsafe { &*ptr };
            DumpHeapObject::Subr {
                name: dump_name_id(subr.name),
                min_args: subr.min_args,
                max_args: subr.max_args,
            }
        }
        _ => DumpHeapObject::Free,
    }
}

// --- Dump-wide symbol table ---

pub(crate) fn dump_symbol_table() -> DumpSymbolTable {
    let dumped = intern::dump_runtime_interner();
    DumpSymbolTable {
        names: dumped.names,
        symbols: dumped
            .symbol_names
            .into_iter()
            .zip(dumped.canonical)
            .map(|(name, canonical)| DumpSymbolEntry {
                name: DumpNameId(name),
                canonical,
            })
            .collect(),
    }
}

// --- Symbol / Obarray ---

/// Which of a BLV's possible forwarders the image has to rebuild.
///
/// `None` for the forward types whose storage is not the descriptor
/// (`Lisp_Fwd_Obj`, `Lisp_Fwd_Buffer_Obj`, `Lisp_Fwd_Kboard_Obj`): those are
/// re-installed from `BUFFER_SLOT_INFO` and from the registration tables, and
/// carry no per-symbol state a dump could lose.
fn dump_localized_forwarder(
    ty: crate::emacs_core::forward::LispFwdType,
) -> Option<crate::emacs_core::pdump::types::DumpLocalizedForwarder> {
    use crate::emacs_core::forward::LispFwdType;
    use crate::emacs_core::pdump::types::DumpLocalizedForwarder as Kind;
    match ty {
        LispFwdType::Bool => Some(Kind::Bool),
        LispFwdType::Int => Some(Kind::Int),
        LispFwdType::Obj => Some(DumpLocalizedForwarder::Obj),
        LispFwdType::KboardObj => Some(DumpLocalizedForwarder::Kboard),
        // A per-buffer slot's descriptor is registration metadata rebuilt from
        // `BUFFER_SLOT_INFO`, and `make_blv` never copies one into a BLV.
        LispFwdType::BufferObj => None,
    }
}

pub(crate) fn dump_symbol_data(
    encoder: &mut DumpEncoder,
    sd: &LispSymbol,
    dynamic_default: Option<Option<Value>>,
) -> DumpSymbolData {
    // Phase I (pdump v21): encode redirect + flags directly.
    use crate::emacs_core::symbol::SymbolRedirect;
    let redirect = sd.flags.redirect();
    let val = match redirect {
        SymbolRedirect::Plainval => {
            let v = match dynamic_default {
                Some(Some(value)) => value,
                Some(None) => Value::UNBOUND,
                None => unsafe { sd.val.plain },
            };
            // Preserve the UNBOUND sentinel — DumpValue::Unbound maps back to
            // Value::UNBOUND on load, which is the correct "unbound" state.
            DumpSymbolVal::Plain(encoder.dump_value(&v))
        }
        SymbolRedirect::Varalias => {
            let target = unsafe { sd.val.alias };
            DumpSymbolVal::Alias(dump_sym_id(target))
        }
        SymbolRedirect::Localized => {
            // Read the BLV to get the global default, the local_if_set flag and
            // the forwarder `make_blv` copied across (`src/data.c:2112-2140`).
            // The BLV is heap-allocated and valid while sd is alive.
            let (default_val, local_if_set, forwarder) = unsafe {
                let blv = &*sd.val.blv;
                let default_val = blv.defcell.cons_cdr();
                (default_val, blv.local_if_set, blv.fwd.map(|fwd| fwd.ty))
            };
            let default_val = match dynamic_default {
                Some(Some(value)) => value,
                Some(None) => Value::UNBOUND,
                None => default_val,
            };
            DumpSymbolVal::Localized {
                default: encoder.dump_value(&default_val),
                local_if_set,
                forwarder: forwarder.and_then(dump_localized_forwarder),
            }
        }
        SymbolRedirect::Forwarded => {
            let fwd = unsafe { &*sd.val.fwd };
            match fwd.ty {
                crate::emacs_core::forward::LispFwdType::Bool => {
                    let bool_fwd = unsafe {
                        &*(fwd as *const _ as *const crate::emacs_core::forward::LispBoolFwd)
                    };
                    DumpSymbolVal::BoolForwarded(bool_fwd.get())
                }
                // BUFFER_OBJFWD forwarders are re-installed from
                // BUFFER_SLOT_INFO in reconstruct_evaluator.
                crate::emacs_core::forward::LispFwdType::Int => {
                    let int_fwd = unsafe {
                        &*(fwd as *const _ as *const crate::emacs_core::forward::LispIntFwd)
                    };
                    DumpSymbolVal::IntForwarded(encoder.dump_value(&int_fwd.get()))
                }
                crate::emacs_core::forward::LispFwdType::Obj => {
                    let obj_fwd = unsafe {
                        &*(fwd as *const _ as *const crate::emacs_core::forward::LispObjFwd)
                    };
                    DumpSymbolVal::ObjForwarded(encoder.dump_value(&obj_fwd.get()))
                }
                crate::emacs_core::forward::LispFwdType::KboardObj => {
                    let kbd_fwd = unsafe {
                        &*(fwd as *const _ as *const crate::emacs_core::forward::LispKboardObjFwd)
                    };
                    DumpSymbolVal::KboardForwarded(encoder.dump_value(&kbd_fwd.get()))
                }
                crate::emacs_core::forward::LispFwdType::BufferObj => DumpSymbolVal::Forwarded,
            }
        }
    };
    DumpSymbolData {
        redirect: redirect as u8,
        trapped_write: sd.flags.trapped_write() as u8,
        interned: sd.flags.interned() as u8,
        declared_special: sd.flags.declared_special(),
        val,
        function: encoder.dump_value(&sd.function),
        plist: encoder.dump_value(&sd.plist),
    }
}

fn dynamic_default_for_dump(eval: &Context, sym_id: SymId) -> Option<Option<Value>> {
    eval.specpdl.iter().find_map(|binding| match binding {
        crate::emacs_core::eval::SpecBinding::Let {
            sym_id: binding_sym,
            old_value,
        }
        | crate::emacs_core::eval::SpecBinding::LetDefault {
            sym_id: binding_sym,
            old_value,
            ..
        } if *binding_sym == sym_id => Some(old_value.get()),
        _ => None,
    })
}

pub(crate) fn dump_obarray(encoder: &mut DumpEncoder, eval: &Context) -> DumpObarray {
    DumpObarray {
        symbols: eval
            .obarray
            .iter_symbols()
            .map(|(id, sd)| {
                (
                    dump_sym_id(id),
                    dump_symbol_data(encoder, sd, dynamic_default_for_dump(eval, id)),
                )
            })
            .collect(),
        global_members: eval.obarray.global_member_ids().map(dump_sym_id).collect(),
        function_unbound: eval
            .obarray
            .function_unbound_ids()
            .map(dump_sym_id)
            .collect(),
        function_epoch: eval.obarray.function_epoch(),
        // Filled by `extract_tagged_heap_payloads`, which partitions the
        // Plain/Alias symbols into fixed heap-image rows.
        plain_rows: None,
    }
}

// --- OrderedSymMap ---

fn dump_runtime_binding_value(
    encoder: &mut DumpEncoder,
    value: &RuntimeBindingValue,
) -> DumpRuntimeBindingValue {
    match value {
        RuntimeBindingValue::Bound(value) => {
            DumpRuntimeBindingValue::Bound(encoder.dump_value(value))
        }
        RuntimeBindingValue::Void => DumpRuntimeBindingValue::Void,
    }
}

fn load_runtime_binding_value(
    decoder: &mut LoadDecoder,
    value: &DumpRuntimeBindingValue,
) -> RuntimeBindingValue {
    match value {
        DumpRuntimeBindingValue::Bound(value) => {
            RuntimeBindingValue::Bound(decoder.load_value(value))
        }
        DumpRuntimeBindingValue::Void => RuntimeBindingValue::Void,
    }
}

// --- Buffer types ---

// `dump_insertion_type` / `load_insertion_type` were retired in v26: chain
// entries now serialize the `LispMarker::insertion_type` bool directly via
// `DumpMarker`. Together with the deletion of `DumpMarkerEntry` and the
// `DumpInsertionType` enum this removes the last consumer of the
// flat-tuple chain shape.
fn dump_property_interval(
    encoder: &mut DumpEncoder,
    pi: &PropertyInterval,
) -> DumpPropertyInterval {
    DumpPropertyInterval {
        start: pi.start,
        end: pi.end,
        properties: pi
            .properties
            .iter()
            .map(|(k, v)| (encoder.dump_value(k), encoder.dump_value(v)))
            .collect(),
    }
}

fn dump_text_property_table(
    encoder: &mut DumpEncoder,
    tpt: &TextPropertyTable,
) -> DumpTextPropertyTable {
    DumpTextPropertyTable {
        intervals: tpt
            .dump_intervals()
            .into_iter()
            .map(|iv| dump_property_interval(encoder, &iv))
            .collect(),
    }
}

fn dump_overlay(encoder: &mut DumpEncoder, o: &Overlay) -> DumpOverlay {
    let (start, end) = o.current_range();
    DumpOverlay {
        serial: o.serial,
        plist: encoder.dump_value(&o.plist),
        buffer: o.buffer.map(|id| DumpBufferId(id.0)),
        start,
        end,
        front_advance: o.front_advance,
        rear_advance: o.rear_advance,
    }
}

fn dump_marker_object(marker: &crate::heap_types::LispMarker) -> DumpMarker {
    // T10 (v26): LispMarker fields are authoritative. We round-trip
    // `bytepos` and `charpos` directly; the legacy `position` cache is
    // gone in both runtime and on-disk shapes.
    DumpMarker {
        buffer: marker.buffer.map(|id| DumpBufferId(id.0)),
        insertion_type: marker.insertion_type,
        marker_id: marker.marker_id,
        bytepos: marker.bytepos,
        charpos: marker.charpos,
        last_position_valid: marker.last_position_valid,
    }
}

fn dump_overlay_list(encoder: &mut DumpEncoder, ol: &OverlayList) -> DumpOverlayList {
    DumpOverlayList {
        overlays: ol
            .dump_overlays()
            .iter()
            .filter_map(|v| v.as_overlay_data())
            .map(|data| dump_overlay(encoder, data))
            .collect(),
    }
}

// dump_undo_record and dump_undo_list removed — undo state is now a
// buffer-local Lisp Value serialized through the properties map.

fn dump_buffer_text_backend_kind(
    kind: ImplementedBufferTextBackendKind,
) -> DumpBufferTextBackendKind {
    kind.public_kind().into()
}

fn load_buffer_text_backend_kind(
    kind: DumpBufferTextBackendKind,
) -> ImplementedBufferTextBackendKind {
    crate::buffer::BufferTextBackendKind::from(kind)
        .try_into()
        .expect("pdump buffer text backend tag must be implemented at runtime")
}

fn dump_lisp_char_pos(pos: LispCharPos1) -> usize {
    pos.to_one_based_usize()
}

fn load_lisp_char_pos(pos: Option<usize>) -> LispCharPos1 {
    LispCharPos1::from_one_based_usize(pos.unwrap_or(1))
}

fn dump_buffer(encoder: &mut DumpEncoder, buf: &Buffer) -> DumpBuffer {
    let is_shared_text_owner = buf.base_buffer.is_none();
    // The image format stores GNU's `struct timespec` as two optional halves;
    // an indirect buffer dumps its OWN (unknown) modtime and gets its base's
    // cell back on load, exactly like `text` and `undo_state`.
    let (modtime_sec, modtime_nsec) = buf.visited_file_modtime().to_dump_halves();
    DumpBuffer {
        id: DumpBufferId(buf.id.0),
        name_lisp: buf.name_value().as_lisp_string().map(dump_lisp_string),
        name: None,
        last_name_lisp: buf.last_name_value().as_lisp_string().map(dump_lisp_string),
        last_name: None,
        base_buffer: buf.base_buffer.map(|id| DumpBufferId(id.0)),
        text: DumpBufferText {
            backend_kind: dump_buffer_text_backend_kind(buf.dump_text_backend_kind()),
            text: buf.dump_text_bytes(),
        },
        pt: buf.point_emacs_byte_pos().get(),
        pt_char: Some(buf.point_char_pos().get()),
        mark: buf.mark_emacs_byte_pos().map(EmacsBytePos::get),
        mark_char: buf.mark_char_pos().map(|pos| pos.get()),
        begv: buf.point_min_emacs_byte_pos().get(),
        begv_char: Some(buf.point_min_char_pos().get()),
        zv: buf.point_max_emacs_byte_pos().get(),
        zv_char: Some(buf.point_max_char_pos().get()),
        modified: buf.is_modified(),
        modified_tick: buf.modified_tick(),
        chars_modified_tick: buf.chars_modified_tick(),
        save_modified_tick: Some(buf.save_modified_tick()),
        autosave_modified_tick: Some(buf.autosave_modified_tick),
        modtime_sec,
        modtime_nsec,
        modtime_size: buf.modtime_size,
        last_window_start: Some(dump_lisp_char_pos(buf.last_window_start)),
        read_only: buf.get_read_only(),
        multibyte: buf.get_multibyte(),
        file_name_lisp: buf.file_name_lisp_string().map(dump_lisp_string),
        file_name: None,
        auto_save_file_name_lisp: buf.auto_save_file_name_lisp_string().map(dump_lisp_string),
        auto_save_file_name: None,
        markers: if is_shared_text_owner {
            // T10 (v26): walk the intrusive chain head→tail and emit a
            // `DumpMarker` per node. Identity with the heap-object
            // Marker decode is preserved via `marker_id`; the load-side
            // chain reconstruction reuses the heap-allocated MarkerObj.
            let mut out = Vec::new();
            buf.walk_marker_data_for_dump(|data| {
                out.push(DumpMarker {
                    buffer: data.buffer.map(|id| DumpBufferId(id.0)),
                    insertion_type: data.insertion_type,
                    marker_id: data.marker_id,
                    bytepos: data.bytepos,
                    charpos: data.charpos,
                    last_position_valid: data.last_position_valid,
                });
            });
            out
        } else {
            Vec::new()
        },
        state_pt_marker: buf.state_markers.map(|markers| markers.pt_marker),
        state_begv_marker: buf.state_markers.map(|markers| markers.begv_marker),
        state_zv_marker: buf.state_markers.map(|markers| markers.zv_marker),
        properties_syms: buf
            .ordered_buffer_local_bindings()
            .into_iter()
            .map(|(sym_id, value)| {
                (
                    dump_sym_id(sym_id),
                    dump_runtime_binding_value(encoder, &value),
                )
            })
            .collect(),
        properties: Vec::new(),
        local_binding_syms: buf
            .ordered_buffer_local_names()
            .into_iter()
            .map(dump_sym_id)
            .collect(),
        local_binding_names: Vec::new(),
        local_map: encoder.dump_value(&buf.local_map()),
        text_props: if is_shared_text_owner {
            dump_text_property_table(encoder, &buf.text_props_snapshot())
        } else {
            dump_text_property_table(encoder, &TextPropertyTable::new())
        },
        overlays: dump_overlay_list(encoder, &buf.overlays),
        // Syntax table lives in `buf.slots[BUFFER_SLOT_SYNTAX_TABLE.index()]`
        // (serialized via the slots Vec below) — matches GNU where
        // `buffer->syntax_table` is a single Lisp_Object slot.
        undo_list: None,
        // Phase 11.1: round-trip the BUFFER_OBJFWD slot table.
        // Previously blocked on the BLV GC trace bug (5699c3569);
        // with BLVs traced as roots, slot round-trip is safe for
        // the slot vector overall.
        slots: buf
            .slots
            .iter()
            .map(|slot| encoder.dump_value(slot))
            .collect(),
        // Phase 11: per-slot local-flag bitmap. Mirrors
        // `Buffer::local_flags` (Phase 10D bitset). Safe to
        // round-trip — it's a `u64`.
        local_flags: buf.local_flags,
        // Phase 11: per-buffer alist for SYMBOL_LOCALIZED variables.
        // Mirrors GNU `BVAR(buf, local_var_alist)`. The cons cells
        // already round-trip safely via the dump heap.
        local_var_alist: encoder.dump_value(&buf.local_var_alist_value()),
    }
}

pub(crate) fn dump_buffer_manager(
    encoder: &mut DumpEncoder,
    bm: &BufferManager,
) -> DumpBufferManager {
    DumpBufferManager {
        buffers: bm
            .dump_buffers()
            .iter()
            .map(|(id, buf)| (DumpBufferId(id.0), dump_buffer(encoder, buf)))
            .collect(),
        buffer_order: bm
            .dump_buffer_order()
            .iter()
            .map(|id| DumpBufferId(id.0))
            .collect(),
        current: bm.dump_current().map(|id| DumpBufferId(id.0)),
        next_id: bm.dump_next_id(),
        next_marker_id: bm.dump_next_marker_id(),
        // Mirror GNU's `buffer_defaults` C-static struct through the
        // dump. Without this, `setq-default` writes during loadup
        // (notably bindings.el's rich `mode-line-format`) are lost
        // on pdump-load, and `reset_buffer_local_variables` reverts
        // every conditional slot to its install-time seed.
        buffer_defaults: bm
            .buffer_defaults
            .iter()
            .map(|value| encoder.dump_value(value))
            .collect(),
        default_text_backend_kind: dump_buffer_text_backend_kind(
            bm.implemented_default_text_backend_kind(),
        ),
    }
}

// --- Sub-managers ---

pub(crate) fn dump_autoload_manager(
    encoder: &mut DumpEncoder,
    am: &AutoloadManager,
) -> DumpAutoloadManager {
    DumpAutoloadManager {
        entries_syms: am
            .dump_entries()
            .iter()
            .map(|(k, v)| {
                (
                    dump_sym_id(*k),
                    DumpAutoloadEntry {
                        file: dump_lisp_string(&v.file),
                        docstring: v.docstring.as_ref().map(dump_lisp_string),
                        interactive: v.interactive,
                        autoload_type: match v.autoload_type {
                            AutoloadType::Function => DumpAutoloadType::Function,
                            AutoloadType::Macro => DumpAutoloadType::Macro,
                            AutoloadType::Keymap => DumpAutoloadType::Keymap,
                        },
                    },
                )
            })
            .collect(),
        entries: Vec::new(),
        after_load_lisp: am
            .dump_after_load()
            .iter()
            .map(|(k, v)| {
                (
                    dump_lisp_string(k.as_lisp_string()),
                    v.iter().map(|value| encoder.dump_value(value)).collect(),
                )
            })
            .collect(),
        after_load: Vec::new(),
        loaded_files: am
            .dump_loaded_files()
            .iter()
            .map(dump_lisp_string)
            .collect(),
        obsolete_functions_syms: am
            .dump_obsolete_functions()
            .iter()
            .map(|(name, (new_name, when))| {
                (
                    dump_sym_id(*name),
                    (dump_lisp_string(new_name), dump_lisp_string(when)),
                )
            })
            .collect(),
        obsolete_functions: Vec::new(),
        obsolete_variables_syms: am
            .dump_obsolete_variables()
            .iter()
            .map(|(name, (new_name, when))| {
                (
                    dump_sym_id(*name),
                    (dump_lisp_string(new_name), dump_lisp_string(when)),
                )
            })
            .collect(),
        obsolete_variables: Vec::new(),
    }
}

pub(crate) fn dump_custom_manager(_cm: &CustomManager) -> DumpCustomManager {
    // Phase D: auto_buffer_local mirror removed. Emit empty vecs so that
    // existing pdump readers that check the field for backward compat
    // still see a valid (empty) payload.
    DumpCustomManager {
        auto_buffer_local_syms: Vec::new(),
        auto_buffer_local: Vec::new(),
    }
}

fn dump_font_lock_keyword(kw: &FontLockKeyword) -> DumpFontLockKeyword {
    DumpFontLockKeyword {
        pattern_lisp: Some(dump_lisp_string(&kw.pattern)),
        pattern: None,
        face_sym: Some(dump_sym_id(kw.face)),
        face: None,
        group: kw.group,
        override_: kw.override_,
        laxmatch: kw.laxmatch,
    }
}

fn dump_font_lock_defaults(fld: &FontLockDefaults) -> DumpFontLockDefaults {
    DumpFontLockDefaults {
        keywords: fld.keywords.iter().map(dump_font_lock_keyword).collect(),
        case_fold: fld.case_fold,
        syntax_table_lisp: fld.syntax_table.as_ref().map(dump_lisp_string),
        syntax_table: None,
    }
}

fn dump_mode_custom_type(encoder: &mut DumpEncoder, ct: &ModeCustomType) -> DumpModeCustomType {
    match ct {
        ModeCustomType::Boolean => DumpModeCustomType::Boolean,
        ModeCustomType::Integer => DumpModeCustomType::Integer,
        ModeCustomType::Float => DumpModeCustomType::Float,
        ModeCustomType::String => DumpModeCustomType::String,
        ModeCustomType::Symbol => DumpModeCustomType::Symbol,
        ModeCustomType::Sexp => DumpModeCustomType::Sexp,
        ModeCustomType::Choice(choices) => DumpModeCustomType::Choice(
            choices
                .iter()
                .map(|(s, v)| (s.clone(), encoder.dump_value(v)))
                .collect(),
        ),
        ModeCustomType::List(inner) => {
            DumpModeCustomType::List(Box::new(dump_mode_custom_type(encoder, inner)))
        }
        ModeCustomType::Alist(k, v) => DumpModeCustomType::Alist(
            Box::new(dump_mode_custom_type(encoder, k)),
            Box::new(dump_mode_custom_type(encoder, v)),
        ),
        ModeCustomType::Plist(k, v) => DumpModeCustomType::Plist(
            Box::new(dump_mode_custom_type(encoder, k)),
            Box::new(dump_mode_custom_type(encoder, v)),
        ),
        ModeCustomType::Color => DumpModeCustomType::Color,
        ModeCustomType::Face => DumpModeCustomType::Face,
        ModeCustomType::File => DumpModeCustomType::File,
        ModeCustomType::Directory => DumpModeCustomType::Directory,
        ModeCustomType::Function => DumpModeCustomType::Function,
        ModeCustomType::Variable => DumpModeCustomType::Variable,
        ModeCustomType::Hook => DumpModeCustomType::Hook,
        ModeCustomType::Coding => DumpModeCustomType::Coding,
    }
}

pub(crate) fn dump_mode_registry(encoder: &mut DumpEncoder, mr: &ModeRegistry) -> DumpModeRegistry {
    DumpModeRegistry {
        major_modes: mr
            .dump_major_modes()
            .iter()
            .map(|(k, m)| {
                (
                    dump_sym_id(*k),
                    DumpMajorMode {
                        pretty_name: dump_lisp_string(&m.pretty_name),
                        parent: encoder.dump_opt_value(&m.parent),
                        mode_hook: encoder.dump_value(&m.mode_hook),
                        keymap_name: encoder.dump_opt_value(&m.keymap_name),
                        syntax_table_name: encoder.dump_opt_value(&m.syntax_table_name),
                        abbrev_table_name: encoder.dump_opt_value(&m.abbrev_table_name),
                        font_lock: m.font_lock.as_ref().map(dump_font_lock_defaults),
                        body: encoder.dump_opt_value(&m.body),
                    },
                )
            })
            .collect(),
        minor_modes: mr
            .dump_minor_modes()
            .iter()
            .map(|(k, m)| {
                (
                    dump_sym_id(*k),
                    DumpMinorMode {
                        lighter: m.lighter.as_ref().map(dump_lisp_string),
                        keymap_name: encoder.dump_opt_value(&m.keymap_name),
                        global: m.global,
                        body: encoder.dump_opt_value(&m.body),
                    },
                )
            })
            .collect(),
        buffer_major_modes: mr
            .dump_buffer_major_modes()
            .iter()
            .map(|(k, v)| (*k, encoder.dump_value(v)))
            .collect(),
        buffer_minor_modes: mr
            .dump_buffer_minor_modes()
            .iter()
            .map(|(k, v)| {
                (
                    *k,
                    v.iter().map(|value| encoder.dump_value(value)).collect(),
                )
            })
            .collect(),
        global_minor_modes: mr
            .dump_global_minor_modes()
            .iter()
            .map(|value| encoder.dump_value(value))
            .collect(),
        auto_mode_alist: Vec::new(),
        auto_mode_alist_lisp: mr
            .dump_auto_mode_alist()
            .iter()
            .map(|(pattern, value)| (dump_lisp_string(pattern), encoder.dump_value(value)))
            .collect(),
        custom_variables: mr
            .dump_custom_variables()
            .iter()
            .map(|(k, cv)| {
                (
                    dump_sym_id(*k),
                    DumpModeCustomVariable {
                        default_value: encoder.dump_value(&cv.default_value),
                        doc: cv.doc.as_ref().map(dump_lisp_string),
                        custom_type: dump_mode_custom_type(encoder, &cv.type_),
                        group: encoder.dump_opt_value(&cv.group),
                        set_function: encoder.dump_opt_value(&cv.set_function),
                        get_function: encoder.dump_opt_value(&cv.get_function),
                        tag: cv.tag.as_ref().map(dump_lisp_string),
                    },
                )
            })
            .collect(),
        custom_groups: mr
            .dump_custom_groups()
            .iter()
            .map(|(k, g)| {
                (
                    dump_sym_id(*k),
                    DumpModeCustomGroup {
                        doc: g.doc.as_ref().map(dump_lisp_string),
                        parent: encoder.dump_opt_value(&g.parent),
                        members: g
                            .members
                            .iter()
                            .map(|value| encoder.dump_value(value))
                            .collect(),
                    },
                )
            })
            .collect(),
        fundamental_mode: encoder.dump_value(&mr.dump_fundamental_mode()),
    }
}

fn dump_eol_type(e: &EolType) -> DumpEolType {
    match e {
        EolType::Unix => DumpEolType::Unix,
        EolType::Dos => DumpEolType::Dos,
        EolType::Mac => DumpEolType::Mac,
        EolType::Undecided => DumpEolType::Undecided,
    }
}

pub(crate) fn dump_coding_system_manager(
    encoder: &mut DumpEncoder,
    csm: &CodingSystemManager,
) -> DumpCodingSystemManager {
    DumpCodingSystemManager {
        systems_syms: csm
            .systems
            .iter()
            .map(|(k, v)| {
                (
                    dump_sym_id(*k),
                    DumpCodingSystemInfo {
                        name_sym: Some(dump_sym_id(v.name)),
                        name: None,
                        coding_type_sym: Some(dump_sym_id(v.coding_type)),
                        coding_type: None,
                        mnemonic: v.mnemonic,
                        eol_type: dump_eol_type(&v.eol_type),
                        ascii_compatible_p: v.ascii_compatible_p,
                        charset_list_syms: v
                            .charset_list
                            .iter()
                            .map(|id| dump_sym_id(*id))
                            .collect(),
                        charset_list: Vec::new(),
                        post_read_conversion_sym: v.post_read_conversion.map(dump_sym_id),
                        post_read_conversion: None,
                        pre_write_conversion_sym: v.pre_write_conversion.map(dump_sym_id),
                        pre_write_conversion: None,
                        default_char: v.default_char,
                        for_unibyte: v.for_unibyte,
                        properties_syms: v
                            .properties
                            .iter()
                            .map(|(k, v)| (dump_sym_id(*k), encoder.dump_value(v)))
                            .collect(),
                        properties: Vec::new(),
                        int_properties: v
                            .int_properties
                            .iter()
                            .map(|(k, v)| (*k, encoder.dump_value(v)))
                            .collect(),
                    },
                )
            })
            .collect(),
        systems: Vec::new(),
        aliases_syms: csm
            .aliases
            .iter()
            .map(|(k, v)| (dump_sym_id(*k), dump_sym_id(*v)))
            .collect(),
        aliases: Vec::new(),
        alias_order_syms: csm
            .alias_order
            .iter()
            .map(|(k, v)| {
                (
                    dump_sym_id(*k),
                    v.iter().map(|id| dump_sym_id(*id)).collect(),
                )
            })
            .collect(),
        alias_order: Vec::new(),
        priority_syms: csm.priority.iter().map(|id| dump_sym_id(*id)).collect(),
        priority: Vec::new(),
        keyboard_coding_sym: Some(dump_sym_id(csm.dump_keyboard_coding_sym())),
        keyboard_coding: None,
        terminal_coding_sym: Some(dump_sym_id(csm.dump_terminal_coding_sym())),
        terminal_coding: None,
    }
}

pub(crate) fn dump_charset_registry(encoder: &mut DumpEncoder) -> DumpCharsetRegistry {
    let snapshot = snapshot_charset_registry();
    DumpCharsetRegistry {
        charsets: snapshot
            .charsets
            .into_iter()
            .map(|info| DumpCharsetInfo {
                id: info.id,
                name_sym: Some(dump_sym_id(info.name)),
                name: None,
                dimension: info.dimension,
                code_space: info.code_space,
                min_code: info.min_code,
                max_code: info.max_code,
                iso_final_char: info.iso_final_char,
                iso_revision: info.iso_revision,
                emacs_mule_id: info.emacs_mule_id,
                ascii_compatible_p: info.ascii_compatible_p,
                supplementary_p: info.supplementary_p,
                unified_p: info.unified_p,
                invalid_code: info.invalid_code,
                unify_map: encoder.dump_value(&info.unify_map),
                method: match info.method {
                    CharsetMethodSnapshot::Offset(offset) => DumpCharsetMethod::Offset(offset),
                    CharsetMethodSnapshot::Map(map_name) => DumpCharsetMethod::Map(map_name),
                    CharsetMethodSnapshot::Subset(subset) => {
                        DumpCharsetMethod::Subset(DumpCharsetSubsetSpec {
                            parent_sym: Some(dump_sym_id(subset.parent)),
                            parent: None,
                            parent_min_code: subset.parent_min_code,
                            parent_max_code: subset.parent_max_code,
                            offset: subset.offset,
                        })
                    }
                    CharsetMethodSnapshot::Superset(members) => DumpCharsetMethod::SupersetSyms(
                        members
                            .into_iter()
                            .map(|(name, offset)| (dump_sym_id(name), offset))
                            .collect(),
                    ),
                },
                plist_syms: info
                    .plist
                    .into_iter()
                    .map(|(key, value)| (dump_sym_id(key), encoder.dump_value(&value)))
                    .collect(),
                plist: Vec::new(),
            })
            .collect(),
        priority_syms: snapshot.priority.into_iter().map(dump_sym_id).collect(),
        priority: Vec::new(),
        next_id: snapshot.next_id,
    }
}

fn dump_font_width(width: &FontWidth) -> DumpFontWidth {
    match width {
        FontWidth::UltraCondensed => DumpFontWidth::UltraCondensed,
        FontWidth::ExtraCondensed => DumpFontWidth::ExtraCondensed,
        FontWidth::Condensed => DumpFontWidth::Condensed,
        FontWidth::SemiCondensed => DumpFontWidth::SemiCondensed,
        FontWidth::Normal => DumpFontWidth::Normal,
        FontWidth::SemiExpanded => DumpFontWidth::SemiExpanded,
        FontWidth::Expanded => DumpFontWidth::Expanded,
        FontWidth::ExtraExpanded => DumpFontWidth::ExtraExpanded,
        FontWidth::UltraExpanded => DumpFontWidth::UltraExpanded,
    }
}

fn dump_font_repertory(repertory: FontRepertory) -> DumpFontRepertory {
    match repertory {
        FontRepertory::Charset(name) => DumpFontRepertory::CharsetSym(dump_sym_id(name)),
        FontRepertory::CharTableRanges(ranges) => DumpFontRepertory::CharTableRanges(ranges),
    }
}

fn dump_stored_font_spec(spec: StoredFontSpec) -> DumpStoredFontSpec {
    DumpStoredFontSpec {
        family_sym: spec.family.map(dump_sym_id),
        family: None,
        registry_sym: spec.registry.map(dump_sym_id),
        registry: None,
        lang_sym: spec.lang.map(dump_sym_id),
        lang: None,
        weight: spec.weight.map(FontWeight::dump_code),
        slant: spec.slant.map(|slant| dump_font_slant(&slant)),
        width: spec.width.map(|width| dump_font_width(&width)),
        repertory: spec.repertory.map(dump_font_repertory),
    }
}

fn dump_font_spec_entry(entry: FontSpecEntry) -> DumpFontSpecEntry {
    match entry {
        FontSpecEntry::Font(spec) => DumpFontSpecEntry::Font(dump_stored_font_spec(spec)),
        FontSpecEntry::ExplicitNone => DumpFontSpecEntry::ExplicitNone,
    }
}

pub(crate) fn dump_fontset_registry() -> DumpFontsetRegistry {
    let snapshot = snapshot_fontset_registry();
    DumpFontsetRegistry {
        ordered_names_lisp: snapshot
            .ordered_names
            .iter()
            .map(dump_lisp_string)
            .collect(),
        alias_to_name_lisp: snapshot
            .alias_to_name
            .iter()
            .map(|(alias, name)| (dump_lisp_string(alias), dump_lisp_string(name)))
            .collect(),
        fontsets_lisp: snapshot
            .fontsets
            .iter()
            .map(|(name, data)| {
                (
                    dump_lisp_string(name),
                    DumpFontsetData {
                        ranges: data
                            .ranges
                            .iter()
                            .map(|range| DumpFontsetRangeEntry {
                                from: range.from,
                                to: range.to,
                                entries: range
                                    .entries
                                    .iter()
                                    .cloned()
                                    .map(dump_font_spec_entry)
                                    .collect(),
                            })
                            .collect(),
                        fallback: data.fallback.as_ref().map(|entries| {
                            entries.iter().cloned().map(dump_font_spec_entry).collect()
                        }),
                    },
                )
            })
            .collect(),
        ordered_names: Vec::new(),
        alias_to_name: Vec::new(),
        fontsets: Vec::new(),
        generation: snapshot.generation,
    }
}

fn dump_color(c: &Color) -> DumpColor {
    DumpColor {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

fn dump_font_slant(s: &FontSlant) -> DumpFontSlant {
    match s {
        FontSlant::Normal => DumpFontSlant::Normal,
        FontSlant::Italic => DumpFontSlant::Italic,
        FontSlant::Oblique => DumpFontSlant::Oblique,
        FontSlant::ReverseItalic => DumpFontSlant::ReverseItalic,
        FontSlant::ReverseOblique => DumpFontSlant::ReverseOblique,
    }
}

fn dump_underline_style(s: &UnderlineStyle) -> DumpUnderlineStyle {
    match s {
        UnderlineStyle::Line => DumpUnderlineStyle::Line,
        UnderlineStyle::Wave => DumpUnderlineStyle::Wave,
        UnderlineStyle::Dots => DumpUnderlineStyle::Dot,
        UnderlineStyle::Dashes => DumpUnderlineStyle::Dash,
        UnderlineStyle::DoubleLine => DumpUnderlineStyle::DoubleLine,
    }
}

fn dump_box_style(s: &BoxStyle) -> DumpBoxStyle {
    match s {
        BoxStyle::Flat => DumpBoxStyle::Flat,
        BoxStyle::Raised => DumpBoxStyle::Raised,
        BoxStyle::Pressed => DumpBoxStyle::Pressed,
    }
}

fn dump_face_height(h: &FaceHeight) -> DumpFaceHeight {
    match h {
        FaceHeight::Absolute(n) => DumpFaceHeight::Absolute(*n),
        FaceHeight::Relative(f) => DumpFaceHeight::Relative(*f),
    }
}

fn dump_face(encoder: &mut DumpEncoder, f: &Face) -> DumpFace {
    DumpFace {
        foreground: f.foreground.map(|c| dump_color(&c)),
        background: f.background.map(|c| dump_color(&c)),
        family_value: f.family.as_ref().map(|value| encoder.dump_value(value)),
        family: None,
        foundry_value: f.foundry.as_ref().map(|value| encoder.dump_value(value)),
        foundry: None,
        height: f.height.as_ref().map(dump_face_height),
        weight: f.weight.map(FontWeight::dump_code),
        slant: f.slant.as_ref().map(dump_font_slant),
        underline_disabled: matches!(&f.underline, FaceDecoration::Disabled),
        underline: f.underline.enabled().map(|u| DumpUnderline {
            style: dump_underline_style(&u.style),
            color: u.color.map(|c| dump_color(&c)),
            position: match u.position {
                UnderlinePosition::FontMetric => None,
                UnderlinePosition::DescentLine { pixels_above } => {
                    Some(i32::try_from(pixels_above).unwrap_or(i32::MAX))
                }
            },
        }),
        overline: f.overline,
        strike_through: f.strike_through,
        box_disabled: matches!(&f.box_border, FaceDecoration::Disabled),
        box_border: f.box_border.enabled().map(|b| DumpBoxBorder {
            color: b.color.map(|c| dump_color(&c)),
            width: b.width,
            style: dump_box_style(&b.style),
        }),
        inverse_video: f.inverse_video,
        stipple_value: f.stipple.as_ref().map(|value| encoder.dump_value(value)),
        stipple: None,
        extend: f.extend,
        // Legacy dump schema: flatten the face_ref into a symbol list.
        // A symbol becomes a one-element list; a list of symbols is
        // preserved; plists and nested refs are dropped (a later schema
        // revision should store the raw face_ref with full fidelity).
        inherit_syms: match f.inherit {
            None => Vec::new(),
            Some(v) => {
                if let Some(id) = v.as_symbol_id() {
                    vec![dump_sym_id(id)]
                } else {
                    crate::emacs_core::value::list_to_vec(&v)
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(|entry| entry.as_symbol_id().map(dump_sym_id))
                                .collect()
                        })
                        .unwrap_or_default()
                }
            }
        },
        inherit: Vec::new(),
        overstrike: f.overstrike,
        doc_value: f.doc.as_ref().map(|value| encoder.dump_value(value)),
        doc: None,
    }
}

pub(crate) fn dump_face_table(encoder: &mut DumpEncoder, ft: &FaceTable) -> DumpFaceTable {
    DumpFaceTable {
        face_ids: ft
            .dump_faces_by_sym_id()
            .into_iter()
            .map(|(id, f)| (dump_sym_id(id), dump_face(encoder, &f)))
            .collect(),
        faces: Vec::new(),
    }
}

pub(crate) fn dump_rectangle(r: &RectangleState) -> DumpRectangleState {
    DumpRectangleState {
        killed: r.killed.iter().map(dump_lisp_string).collect(),
    }
}

pub(crate) fn dump_kmacro(encoder: &mut DumpEncoder, km: &KmacroManager) -> DumpKmacroManager {
    DumpKmacroManager {
        // Live recording/playback state is keyboard-runtime owned and is not
        // persisted in fresh dumps. Keep the fields for backward-compatible
        // decoding of older pdumps only.
        current_macro: Vec::new(),
        last_macro: None,
        macro_ring: km
            .macro_ring
            .iter()
            .map(|m| m.iter().map(|value| encoder.dump_value(value)).collect())
            .collect(),
        counter: km.counter,
        counter_format_lisp: Some(dump_lisp_string(&km.counter_format)),
        counter_format: None,
    }
}

pub(crate) fn dump_register_manager(
    encoder: &mut DumpEncoder,
    rm: &RegisterManager,
) -> DumpRegisterManager {
    DumpRegisterManager {
        registers: rm
            .dump_registers()
            .iter()
            .map(|(c, r)| {
                (
                    *c,
                    match r {
                        RegisterContent::Text(s) => DumpRegisterContent::Text {
                            data: s.as_bytes().to_vec(),
                            size: s.schars(),
                            size_byte: s.size_byte(),
                        },
                        RegisterContent::Number(n) => DumpRegisterContent::Number(*n),
                        RegisterContent::Marker(v) => {
                            DumpRegisterContent::Marker(encoder.dump_value(v))
                        }
                        RegisterContent::Rectangle(lines) => DumpRegisterContent::Rectangle(
                            lines.iter().map(dump_lisp_string).collect(),
                        ),
                        RegisterContent::FrameConfig(v) => {
                            DumpRegisterContent::FrameConfig(encoder.dump_value(v))
                        }
                        RegisterContent::File(s) => DumpRegisterContent::File(dump_lisp_string(s)),
                        RegisterContent::KbdMacro(keys) => DumpRegisterContent::KbdMacro(
                            keys.iter().map(|value| encoder.dump_value(value)).collect(),
                        ),
                    },
                )
            })
            .collect(),
    }
}

pub(crate) fn dump_bookmark_manager(bm: &BookmarkManager) -> DumpBookmarkManager {
    DumpBookmarkManager {
        bookmarks_lisp: bm
            .dump_bookmarks()
            .iter()
            .map(|(k, b)| {
                (
                    dump_lisp_string(k.as_lisp_string()),
                    DumpBookmark {
                        name: dump_lisp_string(&b.name),
                        filename: b.filename.as_ref().map(dump_lisp_string),
                        position: b.position.as_i64().max(1) as usize,
                        front_context: b.front_context.as_ref().map(dump_lisp_string),
                        rear_context: b.rear_context.as_ref().map(dump_lisp_string),
                        annotation: b.annotation.as_ref().map(dump_lisp_string),
                        handler: b.handler.as_ref().map(dump_lisp_string),
                    },
                )
            })
            .collect(),
        bookmarks: Vec::new(),
        recent: bm.dump_recent().iter().map(dump_lisp_string).collect(),
    }
}

pub(crate) fn dump_abbrev_manager(am: &AbbrevManager) -> DumpAbbrevManager {
    DumpAbbrevManager {
        tables_syms: am
            .dump_tables()
            .iter()
            .map(|(sym, t)| {
                (
                    dump_sym_id(*sym),
                    DumpAbbrevTable {
                        name: dump_lisp_string(&t.name),
                        abbrevs: t
                            .abbrevs
                            .iter()
                            .map(|(k, a)| {
                                (
                                    dump_lisp_string(k),
                                    DumpAbbrev {
                                        expansion: dump_lisp_string(&a.expansion),
                                        hook: a.hook.as_ref().map(dump_lisp_string),
                                        count: a.count,
                                        system: a.system,
                                    },
                                )
                            })
                            .collect(),
                        parent: t.parent.as_ref().map(dump_lisp_string),
                        case_fixed: t.case_fixed,
                        enable_quoting: t.enable_quoting,
                    },
                )
            })
            .collect(),
        tables: Vec::new(),
        global_table_sym: Some(dump_sym_id(am.dump_global_table_sym())),
        global_table_name: dump_lisp_string(&am.global_table_name()),
        abbrev_mode: am.dump_abbrev_mode(),
    }
}

pub(crate) fn dump_interactive_registry(
    encoder: &mut DumpEncoder,
    ir: &InteractiveRegistry,
) -> DumpInteractiveRegistry {
    DumpInteractiveRegistry {
        specs: ir
            .dump_specs()
            .iter()
            .map(|(k, s)| {
                (
                    dump_sym_id(*k),
                    DumpInteractiveSpec {
                        spec: encoder.dump_value(&s.spec),
                    },
                )
            })
            .collect(),
    }
}

pub(crate) fn dump_watcher_list(
    encoder: &mut DumpEncoder,
    wl: &VariableWatcherList,
) -> DumpVariableWatcherList {
    DumpVariableWatcherList {
        watchers: wl
            .dump_watchers()
            .iter()
            .map(|(k, watchers)| {
                (
                    dump_sym_id(*k),
                    watchers
                        .iter()
                        .map(|w| encoder.dump_value(&w.callback))
                        .collect(),
                )
            })
            .collect(),
    }
}

// --- Top-level dump ---

pub(crate) fn dump_evaluator(eval: &Context) -> DumpContextState {
    let mut encoder = DumpEncoder::new();

    let dump = DumpContextState {
        symbol_table: dump_symbol_table(),
        tagged_heap: DumpTaggedHeap {
            objects: Vec::new(),
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        },
        obarray: dump_obarray(&mut encoder, eval),
        dynamic: Vec::new(),
        lexenv: encoder.dump_value(&eval.lexenv),
        features: eval.features.iter().copied().map(dump_sym_id).collect(),
        require_stack: eval
            .require_stack
            .iter()
            .copied()
            .map(dump_sym_id)
            .collect(),
        loads_in_progress: eval
            .loads_in_progress
            .iter()
            .map(dump_lisp_string)
            .collect(),
        buffers: dump_buffer_manager(&mut encoder, &eval.buffers),
        autoloads: dump_autoload_manager(&mut encoder, &eval.autoloads),
        custom: dump_custom_manager(&eval.custom),
        modes: dump_mode_registry(&mut encoder, &eval.modes),
        coding_systems: dump_coding_system_manager(&mut encoder, &eval.coding_systems),
        charset_registry: dump_charset_registry(&mut encoder),
        fontset_registry: dump_fontset_registry(),
        face_table: dump_face_table(&mut encoder, &eval.face_table),
        abbrevs: dump_abbrev_manager(&eval.abbrevs),
        interactive: dump_interactive_registry(&mut encoder, &eval.interactive),
        rectangle: dump_rectangle(&eval.rectangle),
        standard_syntax_table: encoder.dump_value(&eval.standard_syntax_table),
        syntax_code_objects: encoder.dump_value(&eval.syntax_code_objects),
        standard_category_table: encoder.dump_value(&eval.standard_category_table),
        current_local_map: encoder.dump_value(&eval.current_local_map),
        current_global_map: encoder.dump_value(&eval.current_global_map()),
        kmacro: dump_kmacro(&mut encoder, &eval.kmacro),
        registers: dump_register_manager(&mut encoder, &eval.registers),
        bookmarks: dump_bookmark_manager(&eval.bookmarks),
        watchers: dump_watcher_list(&mut encoder, &eval.watchers),
    };

    let tagged_heap = encoder.finalize();

    DumpContextState {
        tagged_heap,
        ..dump
    }
}

// ===========================================================================
// Load direction: Dump → Runtime
// ===========================================================================

// --- Primitives ---

pub(crate) fn load_sym_id(id: &DumpSymId) -> SymId {
    // Dump symbol slots are local to the serialized interner ordering. They
    // must be translated back into the current process interner before any
    // runtime value or object can safely refer to them.
    PDUMP_LOAD_SYM_REMAP.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|remap| remap.get(id.0 as usize))
            .copied()
            .unwrap_or_else(|| panic!("pdump symbol slot {} should have a runtime remap", id.0))
    })
}

pub(crate) fn load_name_id(id: &DumpNameId) -> NameId {
    PDUMP_LOAD_NAME_REMAP.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|remap| remap.get(id.0 as usize))
            .copied()
            .unwrap_or_else(|| panic!("pdump name slot {} should have a runtime remap", id.0))
    })
}

// --- Op ---

// --- Lambda / ByteCode ---

fn load_lambda_params_owned(p: DumpLambdaParams) -> LambdaParams {
    LambdaParams {
        required: p.required.into_iter().map(|s| load_sym_id(&s)).collect(),
        optional: p.optional.into_iter().map(|s| load_sym_id(&s)).collect(),
        rest: p.rest.map(|s| load_sym_id(&s)),
    }
}

fn load_bytecode_owned(
    decoder: &mut LoadDecoder,
    bc: DumpByteCodeFunction,
    mapped_constants: Option<LispValueVec>,
) -> Result<ByteCodeFunction, DumpError> {
    let (ops, gnu_bytecode_bytes) = match bc.instructions {
        DumpByteCodeInstructions::Decoded(ops) => (ops, None),
        DumpByteCodeInstructions::Gnu(data) => {
            let bytes = match data {
                DumpByteData::Owned(bytes) => crate::tagged::header::LispByteVec::owned(bytes),
                // The span points into the retained dump image; aliasing it
                // skips one Vec alloc + copy per dumped function.
                DumpByteData::Mapped(_) => {
                    let mapped_heap = decoder.state.mapped_heap.ok_or_else(|| {
                        DumpError::ImageFormatError(
                            "mapped gnu bytecode span without a heap image".into(),
                        )
                    })?;
                    let bytes = mapped_heap.bytes(&data)?;
                    unsafe { crate::tagged::header::LispByteVec::mapped(bytes.ptr, bytes.len) }
                }
                DumpByteData::StaticRoData { .. } => {
                    return Err(DumpError::ImageFormatError(
                        "gnu bytecode bytes cannot come from static rodata".into(),
                    ));
                }
            };
            (Vec::new(), Some(bytes))
        }
    };
    let params = load_lambda_params_owned(bc.params);
    let arglist = bc
        .arglist
        .map(|value| decoder.load_value_owned(value))
        .unwrap_or_else(|| crate::emacs_core::builtins::lambda_params_to_value(&params));
    let mut function = ByteCodeFunction {
        source_id: crate::emacs_core::bytecode::fresh_bytecode_source_id(),
        ops,
        stack_verified: false,
        constants: match mapped_constants {
            Some(mapped) => mapped,
            None => bc
                .constants
                .into_iter()
                .map(|value| decoder.load_value_owned(value))
                .collect::<Vec<_>>()
                .into(),
        },
        max_stack: bc.max_stack,
        params,
        arglist,
        lexical: bc.lexical,
        env: decoder.load_opt_value_owned(bc.env),
        gnu_byte_offset_map: None,
        gnu_bytecode_bytes,
        docstring: bc.docstring.map(load_lisp_string_owned),
        doc_form: decoder.load_opt_value_owned(bc.doc_form),
        interactive: decoder.load_opt_value_owned(bc.interactive),
        closure_slot_count: bc.closure_slot_count,
        ops_sealed: bc.ops_sealed,
        extra_slots: bc
            .extra_slots
            .into_iter()
            .map(|value| decoder.load_value_owned(value))
            .collect(),
        #[cfg(feature = "jit")]
        runtime: Some(crate::emacs_core::jit::Runtime::new()),
        lazy_gnu_code: None,
    };
    if function.gnu_bytecode_bytes.is_some() {
        function.restore_gnu_decode_policy().map_err(|error| {
            DumpError::DeserializationError(format!("invalid GNU bytecode in pdump: {error}"))
        })?;
    }
    Ok(function)
}

// --- Hash tables ---

fn load_integer_hash_key_for_word_bits(integer: i64, word_bits: u32) -> HashKey {
    let (minimum, maximum) = crate::tagged::value::fixnum_bounds_for_word_bits(word_bits);
    if (minimum..=maximum).contains(&integer) {
        HashKey::Int(integer)
    } else {
        HashKey::Bignum(
            Integer::from(integer)
                .to_twos_complement_limbs_asc()
                .into_boxed_slice(),
        )
    }
}

fn load_bignum_hash_key_for_word_bits(limbs: &[u64], word_bits: u32) -> HashKey {
    let integer = Integer::from_twos_complement_limbs_asc(limbs);
    let (minimum, maximum) = crate::tagged::value::fixnum_bounds_for_word_bits(word_bits);
    if let Ok(integer) = i64::try_from(&integer)
        && (minimum..=maximum).contains(&integer)
    {
        HashKey::Int(integer)
    } else {
        HashKey::Bignum(limbs.to_vec().into_boxed_slice())
    }
}

/// Whether replaying this top-level producer key would lose target identity.
///
/// Value-based integer keys are normalized directly while decoding the
/// serialized key graph, including nested `equal` keys. Only identity-bearing
/// keys need the restored Lisp value: an `eq` integer whose immediate/boxed
/// representation changes, an `eq` heap reference, or an opaque target-specific
/// pointer encoding.
fn hash_key_requires_restored_identity(
    key: &DumpHashKey,
    test: HashTableTest,
    word_bits: u32,
) -> bool {
    let (minimum, maximum) = crate::tagged::value::fixnum_bounds_for_word_bits(word_bits);
    match key {
        DumpHashKey::Int(integer) => {
            test == HashTableTest::Eq && !(minimum..=maximum).contains(integer)
        }
        DumpHashKey::Bignum(limbs) => {
            let integer = Integer::from_twos_complement_limbs_asc(limbs);
            test == HashTableTest::Eq
                && i64::try_from(&integer)
                    .is_ok_and(|integer| (minimum..=maximum).contains(&integer))
        }
        DumpHashKey::Ptr(_) => true,
        DumpHashKey::HeapRef(_) => test == HashTableTest::Eq,
        DumpHashKey::Nil
        | DumpHashKey::True
        | DumpHashKey::Float(_)
        | DumpHashKey::FloatEq(_, _)
        | DumpHashKey::Symbol(_)
        | DumpHashKey::Keyword(_)
        | DumpHashKey::Str(_)
        | DumpHashKey::Char(_)
        | DumpHashKey::Window(_)
        | DumpHashKey::Frame(_)
        | DumpHashKey::EqualCons(_, _)
        | DumpHashKey::EqualVec(_)
        | DumpHashKey::ByteCode(_)
        | DumpHashKey::Marker(_, _)
        | DumpHashKey::Overlay { .. }
        | DumpHashKey::BoolVec { .. }
        | DumpHashKey::SymbolWithPos(_, _)
        | DumpHashKey::Cycle(_)
        | DumpHashKey::Text(_) => false,
    }
}

/// Recompute only keys whose identity cannot be decoded portably. Returning
/// `None` preserves the serialized-key path and never traverses a potentially
/// unpopulated restored object graph.
fn rebuild_target_hash_key(
    serialized: &DumpHashKey,
    restored_key: Value,
    test: HashTableTest,
    word_bits: u32,
) -> Option<HashKey> {
    if hash_key_requires_restored_identity(serialized, test, word_bits) {
        Some(restored_key.to_hash_key(&test))
    } else {
        None
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn load_hash_key(decoder: &mut LoadDecoder, k: &DumpHashKey) -> HashKey {
    match k {
        DumpHashKey::Nil => HashKey::Nil,
        DumpHashKey::True => HashKey::True,
        DumpHashKey::Int(n) => load_integer_hash_key_for_word_bits(*n, usize::BITS),
        DumpHashKey::Bignum(limbs) => load_bignum_hash_key_for_word_bits(limbs, usize::BITS),
        DumpHashKey::Float(bits) => HashKey::Float(*bits),
        DumpHashKey::FloatEq(bits, id) => HashKey::FloatEq(*bits, *id),
        DumpHashKey::Symbol(s) => HashKey::Symbol(load_sym_id(s)),
        DumpHashKey::Keyword(s) => HashKey::Keyword(load_sym_id(s)),
        DumpHashKey::Str(id) => HashKey::Ptr(decoder.heap_ref_to_value(tagged_heap_ref(id)).bits()),
        DumpHashKey::Char(c) => HashKey::Char(*c),
        DumpHashKey::Window(w) => HashKey::Window(*w),
        DumpHashKey::Frame(f) => HashKey::Frame(*f),
        DumpHashKey::Ptr(p) => HashKey::Ptr(*p as usize),
        DumpHashKey::HeapRef(a) => HashKey::Ptr(
            decoder
                .heap_ref_to_value(TaggedHeapRef { index: *a })
                .bits(),
        ),
        DumpHashKey::EqualCons(a, b) => HashKey::EqualCons(
            Box::new(load_hash_key(decoder, a)),
            Box::new(load_hash_key(decoder, b)),
        ),
        DumpHashKey::EqualVec(v) => HashKey::EqualVec(
            v.iter()
                .map(|item| load_hash_key(decoder, item))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        ),
        DumpHashKey::ByteCode(parts) => HashKey::ByteCode(
            parts
                .iter()
                .map(|part| load_byte_code_key_part(decoder, part))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        ),
        DumpHashKey::Marker(buffer, bytepos) => {
            HashKey::Marker(Box::new((*buffer, EmacsBytePos::new(*bytepos))))
        }
        DumpHashKey::Overlay {
            buffer,
            start,
            end,
            plist,
        } => HashKey::Overlay(Box::new((
            *buffer,
            *start,
            *end,
            load_hash_key(decoder, plist),
        ))),
        DumpHashKey::BoolVec { len, bits } => HashKey::BoolVec(Box::new((*len as usize, *bits))),
        DumpHashKey::SymbolWithPos(sym, pos) => HashKey::SymbolWithPos(
            Box::new(load_hash_key(decoder, sym)),
            Box::new(load_hash_key(decoder, pos)),
        ),
        DumpHashKey::Cycle(index) => HashKey::Cycle(*index),
        DumpHashKey::Text(text) => HashKey::Text(text.clone().into_boxed_str()),
    }
}

fn load_byte_code_key_part(
    decoder: &mut LoadDecoder,
    part: &DumpByteCodeKeyPart,
) -> ByteCodeKeyPart {
    match part {
        DumpByteCodeKeyPart::ObservableSlotCount(count) => {
            ByteCodeKeyPart::ObservableSlotCount(*count)
        }
        DumpByteCodeKeyPart::Value(value) => ByteCodeKeyPart::Value(load_hash_key(decoder, value)),
        DumpByteCodeKeyPart::Bytes(bytes) => {
            ByteCodeKeyPart::Bytes(bytes.clone().into_boxed_slice())
        }
        DumpByteCodeKeyPart::Ops(ops) => ByteCodeKeyPart::Ops(ops.clone().into_boxed_slice()),
        DumpByteCodeKeyPart::Values(values) => ByteCodeKeyPart::Values(
            values
                .iter()
                .map(|value| load_hash_key(decoder, value))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        ),
        DumpByteCodeKeyPart::Text { char_count, bytes } => ByteCodeKeyPart::Text {
            char_count: *char_count,
            bytes: bytes.clone().into_boxed_slice(),
        },
        DumpByteCodeKeyPart::Absent => ByteCodeKeyPart::Absent,
    }
}

fn load_hash_key_owned(decoder: &mut LoadDecoder, k: DumpHashKey) -> HashKey {
    match k {
        DumpHashKey::Nil => HashKey::Nil,
        DumpHashKey::True => HashKey::True,
        DumpHashKey::Int(n) => load_integer_hash_key_for_word_bits(n, usize::BITS),
        DumpHashKey::Bignum(limbs) => load_bignum_hash_key_for_word_bits(&limbs, usize::BITS),
        DumpHashKey::Float(bits) => HashKey::Float(bits),
        DumpHashKey::FloatEq(bits, id) => HashKey::FloatEq(bits, id),
        DumpHashKey::Symbol(s) => HashKey::Symbol(load_sym_id(&s)),
        DumpHashKey::Keyword(s) => HashKey::Keyword(load_sym_id(&s)),
        DumpHashKey::Str(id) => {
            HashKey::Ptr(decoder.heap_ref_to_value(tagged_heap_ref(&id)).bits())
        }
        DumpHashKey::Char(c) => HashKey::Char(c),
        DumpHashKey::Window(w) => HashKey::Window(w),
        DumpHashKey::Frame(f) => HashKey::Frame(f),
        DumpHashKey::Ptr(p) => HashKey::Ptr(p as usize),
        DumpHashKey::HeapRef(a) => {
            HashKey::Ptr(decoder.heap_ref_to_value(TaggedHeapRef { index: a }).bits())
        }
        DumpHashKey::EqualCons(a, b) => HashKey::EqualCons(
            Box::new(load_hash_key_owned(decoder, *a)),
            Box::new(load_hash_key_owned(decoder, *b)),
        ),
        DumpHashKey::EqualVec(v) => HashKey::EqualVec(
            v.into_iter()
                .map(|item| load_hash_key_owned(decoder, item))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        ),
        DumpHashKey::ByteCode(parts) => HashKey::ByteCode(
            parts
                .into_iter()
                .map(|part| load_byte_code_key_part_owned(decoder, part))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        ),
        DumpHashKey::Marker(buffer, bytepos) => {
            HashKey::Marker(Box::new((buffer, EmacsBytePos::new(bytepos))))
        }
        DumpHashKey::Overlay {
            buffer,
            start,
            end,
            plist,
        } => HashKey::Overlay(Box::new((
            buffer,
            start,
            end,
            load_hash_key_owned(decoder, *plist),
        ))),
        DumpHashKey::BoolVec { len, bits } => HashKey::BoolVec(Box::new((len as usize, bits))),
        DumpHashKey::SymbolWithPos(sym, pos) => HashKey::SymbolWithPos(
            Box::new(load_hash_key_owned(decoder, *sym)),
            Box::new(load_hash_key_owned(decoder, *pos)),
        ),
        DumpHashKey::Cycle(index) => HashKey::Cycle(index),
        DumpHashKey::Text(text) => HashKey::Text(text.into_boxed_str()),
    }
}

fn load_byte_code_key_part_owned(
    decoder: &mut LoadDecoder,
    part: DumpByteCodeKeyPart,
) -> ByteCodeKeyPart {
    match part {
        DumpByteCodeKeyPart::ObservableSlotCount(count) => {
            ByteCodeKeyPart::ObservableSlotCount(count)
        }
        DumpByteCodeKeyPart::Value(value) => {
            ByteCodeKeyPart::Value(load_hash_key_owned(decoder, value))
        }
        DumpByteCodeKeyPart::Bytes(bytes) => ByteCodeKeyPart::Bytes(bytes.into_boxed_slice()),
        DumpByteCodeKeyPart::Ops(ops) => ByteCodeKeyPart::Ops(ops.into_boxed_slice()),
        DumpByteCodeKeyPart::Values(values) => ByteCodeKeyPart::Values(
            values
                .into_iter()
                .map(|value| load_hash_key_owned(decoder, value))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        ),
        DumpByteCodeKeyPart::Text { char_count, bytes } => ByteCodeKeyPart::Text {
            char_count,
            bytes: bytes.into_boxed_slice(),
        },
        DumpByteCodeKeyPart::Absent => ByteCodeKeyPart::Absent,
    }
}

pub(crate) fn load_hash_table_test(t: &DumpHashTableTest) -> HashTableTest {
    match t {
        DumpHashTableTest::Eq => HashTableTest::Eq,
        DumpHashTableTest::Eql => HashTableTest::Eql,
        DumpHashTableTest::Equal => HashTableTest::Equal,
    }
}

pub(crate) fn load_hash_table_weakness(w: &DumpHashTableWeakness) -> HashTableWeakness {
    match w {
        DumpHashTableWeakness::Key => HashTableWeakness::Key,
        DumpHashTableWeakness::Value => HashTableWeakness::Value,
        DumpHashTableWeakness::KeyOrValue => HashTableWeakness::KeyOrValue,
        DumpHashTableWeakness::KeyAndValue => HashTableWeakness::KeyAndValue,
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn load_hash_table(decoder: &mut LoadDecoder, ht: &DumpLispHashTable) -> LispHashTable {
    let restored_test = load_hash_table_test(&ht.test);
    let entries: Vec<_> = ht
        .ordered_entries
        .iter()
        .map(|(k, v, snap)| {
            let value = decoder.load_value(v);
            let snapshot = snap.as_ref().map(|s| decoder.load_value(s));
            let restored_key = snapshot.unwrap_or(value);
            let rebuilt = rebuild_target_hash_key(k, restored_key, restored_test, usize::BITS);
            let key = rebuilt.unwrap_or_else(|| load_hash_key(decoder, k));
            (key, value, snapshot)
        })
        .collect();

    let mut table = LispHashTable::new_unpopulated_with_options(
        restored_test,
        ht.size,
        ht.weakness.as_ref().map(load_hash_table_weakness),
        ht.rehash_size,
        ht.rehash_threshold,
    );
    table.test_name = ht.test_name.map(|s| load_sym_id(&s));
    table.rebuild_from_ordered_entries(entries);
    table
}

// --- Dump-wide symbol table ---

pub(crate) fn load_symbol_table(table: &DumpSymbolTable) -> Result<(), DumpError> {
    let symbol_names: Vec<u32> = table.symbols.iter().map(|entry| entry.name.0).collect();
    let canonical: Vec<bool> = table.symbols.iter().map(|entry| entry.canonical).collect();
    load_symbol_table_parts(&table.names, &symbol_names, &canonical)
}

pub(crate) fn load_symbol_table_parts(
    names: &[LispString],
    symbol_names: &[u32],
    canonical: &[bool],
) -> Result<(), DumpError> {
    let remap = intern::restore_runtime_interner(names, symbol_names, Some(canonical))
        .map_err(DumpError::DeserializationError)?;
    let intern::RestoredDumpSymbolTable { names, symbols } = remap;
    PDUMP_LOAD_NAME_REMAP.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(
            slot.is_none(),
            "pdump name remap should not already be initialized"
        );
        *slot = Some(names);
    });
    PDUMP_LOAD_SYM_REMAP.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(
            slot.is_none(),
            "pdump symbol remap should not already be initialized"
        );
        let identity = symbols.iter().enumerate().all(|(i, id)| id.0 as usize == i);
        PDUMP_LOAD_SYM_IDENTITY.with(|flag| flag.set(identity));
        if !identity {
            // The fallback is a permanent production path (bootstrap
            // cache-miss same-process reloads feed the SHIPPED final image),
            // but a fallback on an ordinary editor launch is perf erosion —
            // keep it visible.
            tracing::info!(
                slots = symbols.len(),
                "pdump symbol remap is not identity; symbol fixups will be applied"
            );
        }
        // Audit probe: is the dump->runtime symbol remap identity on this
        // load? (With baked symbol words this decides whether the 127K-entry
        // fixup walk runs at all.)
        if std::env::var_os("NEOVM_PDUMP_REMAP_AUDIT").is_some() {
            let total = symbols.len();
            let mismatch_count = symbols
                .iter()
                .enumerate()
                .filter(|(i, id)| id.0 as usize != *i)
                .count();
            let first: Vec<(usize, u32)> = symbols
                .iter()
                .enumerate()
                .filter(|(i, id)| id.0 as usize != *i)
                .map(|(i, id)| (i, id.0))
                .take(8)
                .collect();
            eprintln!(
                "NEOVM_PDUMP_REMAP_AUDIT: {total} symbol slots, {mismatch_count} non-identity, first: {first:?}"
            );
        }
        *slot = Some(symbols);
    });
    Ok(())
}

pub(crate) fn finish_load_interner() {
    PDUMP_LOAD_NAME_REMAP.with(|slot| {
        slot.borrow_mut().take();
    });
    PDUMP_LOAD_SYM_REMAP.with(|slot| {
        slot.borrow_mut().take();
    });
    PDUMP_LOAD_SYM_IDENTITY.with(|flag| flag.set(false));
}

// --- Symbol / Obarray ---

pub(crate) fn load_symbol_data(
    decoder: &mut LoadDecoder,
    sym_id: SymId,
    sd: &DumpSymbolData,
) -> LispSymbol {
    use crate::emacs_core::symbol::{SymbolInterned, SymbolRedirect, SymbolVal};
    let mut symbol = LispSymbol::new(sym_id);

    // Restore flag fields.  The `redirect` field is also encoded in `val`'s
    // variant, but we set it here explicitly for clarity.
    let trapped_write: SymbolTrappedWrite = unsafe { std::mem::transmute(sd.trapped_write & 0b11) };
    let interned: SymbolInterned = unsafe { std::mem::transmute(sd.interned & 0b11) };
    symbol.flags.set_trapped_write(trapped_write);
    symbol.flags.set_interned(interned);
    symbol.flags.set_declared_special(sd.declared_special);

    match &sd.val {
        DumpSymbolVal::Plain(v) => {
            symbol.flags.set_redirect(SymbolRedirect::Plainval);
            symbol.val = SymbolVal {
                plain: decoder.load_value(v),
            };
        }
        DumpSymbolVal::Alias(target) => {
            symbol.set_alias_target(load_sym_id(target));
        }
        DumpSymbolVal::Localized { default, .. } => {
            // BLV reconstruction requires the Obarray to be live so that
            // make_symbol_localized can allocate and track the BLV pointer.
            // We cannot do it here (we don't have &mut Obarray).  Instead
            // we store the default in val.plain temporarily; load_obarray
            // performs a second pass after Obarray::from_dump to call
            // make_symbol_localized on every Localized symbol and fix the
            // redirect + BLV pointer.
            symbol.flags.set_redirect(SymbolRedirect::Plainval);
            symbol.val = SymbolVal {
                plain: decoder.load_value(default),
            };
        }
        DumpSymbolVal::Forwarded => {
            // BUFFER_OBJFWD forwarders are re-installed from BUFFER_SLOT_INFO
            // in reconstruct_evaluator after the obarray is built.  Leave the
            // redirect at Plainval / UNBOUND for now; reconstruct_evaluator
            // will call install_buffer_objfwd which flips it to Forwarded.
            symbol.flags.set_redirect(SymbolRedirect::Plainval);
            symbol.val = SymbolVal {
                plain: crate::emacs_core::value::Value::UNBOUND,
            };
        }
        DumpSymbolVal::BoolForwarded(value) => {
            // Like BUFFER_OBJFWD, the stable descriptor pointer is rebuilt
            // only after Obarray construction.  Keep the canonical value in
            // a temporary Plainval cell for the second pass.
            symbol.flags.set_redirect(SymbolRedirect::Plainval);
            symbol.val = SymbolVal {
                plain: crate::emacs_core::value::Value::bool_val(*value),
            };
        }
        DumpSymbolVal::IntForwarded(value)
        | DumpSymbolVal::ObjForwarded(value)
        | DumpSymbolVal::KboardForwarded(value) => {
            symbol.flags.set_redirect(SymbolRedirect::Plainval);
            symbol.val = SymbolVal {
                plain: decoder.load_value(value),
            };
        }
    }

    symbol.function = decoder.load_value(&sd.function);
    symbol.plist = decoder.load_value(&sd.plist);
    symbol
}

pub(crate) fn load_obarray(
    decoder: &mut LoadDecoder,
    dob: &DumpObarray,
) -> Result<Obarray, DumpError> {
    // Duplicate-slot detection is a property of the WRITER (each obarray
    // symbol is serialized once); like the object-extra completeness pass it
    // can only catch a dumper bug and a duplicate cannot corrupt memory --
    // from_dump just overwrites the slot. Debug builds (where every
    // round-trip test runs) keep the check; release trusts the dump.
    // Measured ~4M Ir across the dedup sets on a 17.6K-symbol load.
    #[cfg(debug_assertions)]
    let mut seen_symbol_ids = FxHashSet::default();
    // Collect (sym_id, dump_data) for a second pass over Localized symbols.
    let mut localized_entries: Vec<(SymId, &DumpSymbolData)> = Vec::new();
    let mut bool_forwarded_entries: Vec<(SymId, bool)> = Vec::new();
    let mut int_forwarded_entries: Vec<(SymId, crate::emacs_core::value::Value)> = Vec::new();
    let mut obj_forwarded_entries: Vec<(SymId, crate::emacs_core::value::Value)> = Vec::new();
    let mut kboard_forwarded_entries: Vec<(SymId, crate::emacs_core::value::Value)> = Vec::new();
    let mut symbols = Vec::with_capacity(dob.symbols.len());
    for (id, sd) in &dob.symbols {
        let sym_id = load_sym_id(id);
        #[cfg(debug_assertions)]
        if !seen_symbol_ids.insert(sym_id) {
            return Err(DumpError::DeserializationError(format!(
                "pdump obarray is inconsistent: duplicate symbol slot {}",
                sym_id.0
            )));
        }
        if matches!(sd.val, DumpSymbolVal::Localized { .. }) {
            localized_entries.push((sym_id, sd));
        }
        if let DumpSymbolVal::BoolForwarded(value) = &sd.val {
            bool_forwarded_entries.push((sym_id, *value));
        }
        if let DumpSymbolVal::IntForwarded(value) = &sd.val {
            int_forwarded_entries.push((sym_id, decoder.load_value(value)));
        }
        if let DumpSymbolVal::ObjForwarded(value) = &sd.val {
            obj_forwarded_entries.push((sym_id, decoder.load_value(value)));
        }
        if let DumpSymbolVal::KboardForwarded(value) = &sd.val {
            kboard_forwarded_entries.push((sym_id, decoder.load_value(value)));
        }
        symbols.push((sym_id, load_symbol_data(decoder, sym_id, sd)));
    }

    // Fixed symbol rows (Plain/Varalias): the value words were patched to
    // live runtime Values by the relocation/fixup passes at heap preload, so
    // each row is a header unpack plus three word reads - no DumpValue
    // decode. See `DumpObarray::plain_rows` for the layout.
    if let Some((rows_offset, rows_count)) = dob.plain_rows {
        use crate::emacs_core::symbol::{SymbolInterned, SymbolRedirect, SymbolVal};
        let mapped_heap = decoder.state.mapped_heap.ok_or_else(|| {
            DumpError::ImageFormatError("obarray symbol rows require a mapped heap image".into())
        })?;
        let row_size = crate::emacs_core::pdump::mapped_heap::OBARRAY_ROW_SIZE as u64;
        // One validation per row (the batch hoists the writable check and
        // limits), then three raw word reads - read_value_word re-ran the
        // full validation for every word of every row.
        let batch = mapped_heap.value_word_batch()?;
        symbols.reserve(rows_count as usize);
        for i in 0..rows_count {
            let base = rows_offset + i * row_size;
            let row = batch.word_ptr(base)?;
            debug_assert_eq!(row_size, 32);
            let head = unsafe { row.read_unaligned() } as u64;
            let dump_id = (head & 0xFFFF_FFFF) as u32;
            let redirect = ((head >> 32) & 0xFF) as u8;
            let trapped_write = ((head >> 40) & 0xFF) as u8;
            let interned = ((head >> 48) & 0xFF) as u8;
            let declared_special = ((head >> 56) & 0xFF) != 0;
            // The row is 32 bytes and `base` was validated against the word
            // limit; validate the LAST word so the three trailing reads are
            // covered, then read raw.
            let _ = batch.word_ptr(base + 24)?;
            let val_word = unsafe { row.add(1).read_unaligned() };
            let function = unsafe { row.add(2).read_unaligned() };
            let plist = unsafe { row.add(3).read_unaligned() };

            let sym_id = load_sym_id(&DumpSymId(dump_id));
            #[cfg(debug_assertions)]
            if !seen_symbol_ids.insert(sym_id) {
                return Err(DumpError::DeserializationError(format!(
                    "pdump obarray is inconsistent: duplicate symbol row {}",
                    sym_id.0
                )));
            }
            let mut symbol = crate::emacs_core::symbol::LispSymbol::new(sym_id);
            let trapped_write: SymbolTrappedWrite =
                unsafe { std::mem::transmute(trapped_write & 0b11) };
            let interned: SymbolInterned = unsafe { std::mem::transmute(interned & 0b11) };
            symbol.flags.set_trapped_write(trapped_write);
            symbol.flags.set_interned(interned);
            symbol.flags.set_declared_special(declared_special);
            let val = crate::tagged::value::TaggedValue::from_bits(val_word);
            match redirect {
                1 => {
                    let target = val.as_symbol_id().ok_or_else(|| {
                        DumpError::DeserializationError(format!(
                            "obarray alias row {} target is not a symbol",
                            sym_id.0
                        ))
                    })?;
                    symbol.set_alias_target(target);
                }
                _ => {
                    symbol.flags.set_redirect(SymbolRedirect::Plainval);
                    symbol.val = SymbolVal { plain: val };
                }
            }
            symbol.function = crate::tagged::value::TaggedValue::from_bits(function);
            symbol.plist = crate::tagged::value::TaggedValue::from_bits(plist);
            symbols.push((sym_id, symbol));
        }
    }

    #[cfg(not(debug_assertions))]
    let load_member_set = |_label: &str, ids: &[DumpSymId]| -> Result<Vec<SymId>, DumpError> {
        Ok(ids.iter().map(load_sym_id).collect())
    };
    #[cfg(debug_assertions)]
    let load_member_set = |label: &str, ids: &[DumpSymId]| -> Result<Vec<SymId>, DumpError> {
        let mut seen = FxHashSet::default();
        let mut loaded = Vec::with_capacity(ids.len());
        for id in ids {
            let sym_id = load_sym_id(id);
            if !seen.insert(sym_id) {
                return Err(DumpError::DeserializationError(format!(
                    "pdump obarray is inconsistent: duplicate {label} entry for symbol slot {}",
                    sym_id.0
                )));
            }
            if !seen_symbol_ids.contains(&sym_id) {
                return Err(DumpError::DeserializationError(format!(
                    "pdump obarray is inconsistent: {label} entry references missing symbol slot {}",
                    sym_id.0
                )));
            }
            loaded.push(sym_id);
        }
        Ok(loaded)
    };

    let global_members = load_member_set("global_members", &dob.global_members)?;
    let function_unbound = load_member_set("function_unbound", &dob.function_unbound)?;

    let mut obarray = Obarray::from_dump(
        symbols,
        global_members,
        function_unbound,
        dob.function_epoch,
    );

    // Second pass: reconstruct BLVs for LOCALIZED symbols.
    //
    // load_symbol_data temporarily stored the global default in val.plain
    // (with redirect=Plainval) because BLV allocation requires a live
    // &mut Obarray.  Now that the obarray is built we can call
    // make_symbol_localized to allocate and install the real BLV, then
    // optionally set local_if_set.
    for (sym_id, sd) in &localized_entries {
        if let DumpSymbolVal::Localized {
            default,
            local_if_set,
            forwarder,
        } = &sd.val
        {
            let default_val = decoder.load_value(default);
            obarray.make_symbol_localized(*sym_id, default_val);
            if *local_if_set {
                obarray.set_blv_local_if_set(*sym_id, true);
            }
            // `make_blv` moved the declaration's descriptor into the BLV
            // (`src/data.c:2112-2140`) and a process-lifetime pointer cannot
            // travel in the image, so give the BLV an equivalent one back,
            // seeded from the default the dump did carry.  Without this a
            // `DEFVAR_BOOL`/`DEFVAR_INT` variable Lisp localized would come
            // back as an ordinary buffer-local cell that neither coerces nor
            // type-checks.
            if let Some(kind) = forwarder {
                obarray.reattach_localized_forwarder(*sym_id, *kind);
            }
            // Restore non-redirect flags from the dump — make_symbol_localized
            // only sets the redirect bit, leaving trapped_write / interned /
            // declared_special as defaults.  Re-apply them from the dump.
            use crate::emacs_core::symbol::SymbolInterned;
            if let Some(sym) = obarray.get_mut_by_id(*sym_id) {
                let trapped_write: SymbolTrappedWrite =
                    unsafe { std::mem::transmute(sd.trapped_write & 0b11) };
                let interned: SymbolInterned = unsafe { std::mem::transmute(sd.interned & 0b11) };
                sym.flags.set_trapped_write(trapped_write);
                sym.flags.set_interned(interned);
                sym.flags.set_declared_special(sd.declared_special);
            }
        }
    }

    // Native Boolean forwarders carry process-lifetime pointers and therefore
    // cannot be copied into a portable dump.  Rebuild one independent cell per
    // loaded evaluator from the serialized Boolean value.
    for (sym_id, value) in bool_forwarded_entries {
        let fwd = crate::emacs_core::forward::alloc_boolfwd(value);
        obarray.install_boolfwd(sym_id, fwd);
    }

    // Integer forwarders, same contract as the Boolean ones above. The dumped
    // value went through `LispInteger::check` when it was stored, so it is
    // still an integer inside `intmax_t` range on the way back in; a corrupt
    // image falls back to the slot's zero rather than smuggling a non-integer
    // into a `DEFVAR_INT` variable.
    for (sym_id, value) in int_forwarded_entries {
        let checked = crate::emacs_core::forward::LispInteger::check(value)
            .unwrap_or_else(|_| crate::emacs_core::forward::LispInteger::from_i64(0));
        let fwd = crate::emacs_core::forward::alloc_intfwd(checked);
        obarray.install_intfwd(sym_id, fwd);
    }

    // `DEFVAR_LISP` and `DEFVAR_KBOARD` slots, same contract as the two above.
    // GNU dumps the forwarding pointer itself and relocates it
    // (`src/pdumper.c:2461-2462`); a process-lifetime `Box::leak` cannot
    // travel, so the image carries the value and the descriptor is rebuilt.
    for (sym_id, value) in obj_forwarded_entries {
        let fwd = crate::emacs_core::forward::alloc_objfwd(value);
        obarray.install_objfwd(sym_id, fwd);
    }
    for (sym_id, value) in kboard_forwarded_entries {
        let fwd = crate::emacs_core::forward::alloc_kboard_objfwd(value);
        obarray.install_kboard_objfwd(sym_id, fwd);
    }

    Ok(obarray)
}

// --- Buffer types ---

// `load_insertion_type` / `load_marker` were removed in v26: the per-buffer
// chain entries now serialize directly as `DumpMarker` (the same shape used
// by `DumpHeapObject::Marker`), so the dedicated tuple-decode helper has no
// remaining caller — `load_buffer` walks `db.markers` and feeds each
// `DumpMarker` straight to `BufferText::register_marker` after resolving
// the backing MarkerObj by `marker_id`.

fn load_property_interval(
    decoder: &mut LoadDecoder,
    pi: &DumpPropertyInterval,
) -> PropertyInterval {
    let properties: std::collections::HashMap<
        crate::emacs_core::value::Value,
        crate::emacs_core::value::Value,
    > = pi
        .properties
        .iter()
        .map(|(k, v)| (decoder.load_value(k), decoder.load_value(v)))
        .collect();
    let key_order: Vec<crate::emacs_core::value::Value> = pi
        .properties
        .iter()
        .map(|(k, _)| decoder.load_value(k))
        .collect();
    PropertyInterval {
        start: pi.start,
        end: pi.end,
        properties,
        key_order,
    }
}

// load_undo_record removed — undo state is loaded from buffer-local properties.

#[inline]
fn dump_buffer_byte_to_char_pos(text: &BufferText, byte_pos: EmacsBytePos) -> CharPos0 {
    text.emacs_byte_pos_to_char_pos(byte_pos)
}

#[inline]
fn dump_emacs_byte_pos(byte_pos: usize) -> EmacsBytePos {
    EmacsBytePos::new(byte_pos)
}

#[inline]
fn dump_text_position_anchor(char_pos: usize, byte_pos: usize) -> TextPositionAnchor {
    TextPositionAnchor::new(CharPos0::new(char_pos), dump_emacs_byte_pos(byte_pos))
}

fn load_buffer(
    decoder: &mut LoadDecoder,
    db: &DumpBuffer,
    saved_point_before_command: SavedPointBeforeCommand,
) -> Buffer {
    let text = BufferText::from_snapshot_with_backend_kind(
        BufferTextBytesSnapshot::new(db.text.text.clone(), db.multibyte),
        load_buffer_text_backend_kind(db.text.backend_kind),
    );
    let total_chars = text.char_count().get();
    let begv_char = db
        .begv_char
        .unwrap_or_else(|| dump_buffer_byte_to_char_pos(&text, dump_emacs_byte_pos(db.begv)).get());
    let zv_char = db.zv_char.unwrap_or_else(|| {
        if db.zv == text.emacs_byte_len().get() {
            total_chars
        } else {
            dump_buffer_byte_to_char_pos(&text, dump_emacs_byte_pos(db.zv)).get()
        }
    });
    let pt_char = db.pt_char.unwrap_or_else(|| {
        if db.pt == db.begv {
            begv_char
        } else if db.pt == db.zv {
            zv_char
        } else {
            dump_buffer_byte_to_char_pos(&text, dump_emacs_byte_pos(db.pt)).get()
        }
    });
    let _mark_char = db.mark.map(|mark| {
        db.mark_char.unwrap_or_else(|| {
            if mark == db.begv {
                begv_char
            } else if mark == db.zv {
                zv_char
            } else {
                dump_buffer_byte_to_char_pos(&text, dump_emacs_byte_pos(mark)).get()
            }
        })
    });
    // v26: walk `db.markers` in dumped chain order (head→tail). For each
    // entry, resolve the backing MarkerObj allocated during
    // `preload_tagged_heap` so the chain reuses the same allocation that
    // Lisp values reference. Allocate a fresh scratch MarkerObj only when
    // the dump's heap section did not contain a matching object (defensive
    // — should not happen for self-consistent v26 dumps).
    //
    // We splice at head (`register_marker` uses `chain_splice_at_head`),
    // so iterate in reverse to restore head→tail order on the loaded
    // chain.
    for dump_marker in db.markers.iter().rev() {
        let Some(marker_id) = dump_marker.marker_id else {
            continue;
        };
        let buffer_id = match dump_marker.buffer {
            Some(id) => BufferId(id.0),
            None => BufferId(db.id.0),
        };
        let insertion_type = if dump_marker.insertion_type {
            crate::buffer::InsertionType::After
        } else {
            crate::buffer::InsertionType::Before
        };
        let marker_ptr = decoder
            .state
            .markers_by_id
            .get(&marker_id)
            .copied()
            .unwrap_or_else(|| {
                let scratch =
                    crate::emacs_core::value::Value::make_marker(crate::heap_types::LispMarker {
                        buffer: Some(buffer_id),
                        insertion_type: dump_marker.insertion_type,
                        marker_id: Some(marker_id),
                        bytepos: dump_marker.bytepos,
                        charpos: dump_marker.charpos,
                        last_position_valid: dump_marker.last_position_valid,
                        next_marker: std::ptr::null_mut(),
                    });
                scratch
                    .as_veclike_ptr()
                    .expect("freshly allocated marker should have a veclike ptr")
                    as *mut crate::tagged::header::MarkerObj
            });
        // The MarkerObj may already be on a chain from a prior load in
        // the same process (e.g. reload after a bootstrap). Unlink
        // defensively from this buffer's chain before splicing.
        text.chain_unlink(marker_ptr);
        text.register_marker(
            marker_ptr,
            buffer_id,
            marker_id,
            TextPositionAnchor::new(
                CharPos0::new(dump_marker.charpos),
                EmacsBytePos::new(dump_marker.bytepos),
            ),
            insertion_type,
        );
    }
    // Phase 10F: the legacy `BufferLocals` struct is gone.
    // Reconstruct per-buffer state from the dump's properties list
    // directly into the new storage model:
    //
    //   * `buffer-undo-list` → `SharedUndoState` (the one
    //     always-present non-slot non-alist binding).
    //   * Slot-backed names (BUFFER_OBJFWD) → already restored via
    //     the `slots: ...` round-trip below; skip here.
    //   * Everything else → `local_var_alist`, walked in the
    //     original `local_binding_syms` order so the dumped
    //     ordering is preserved.
    let loaded_keymap = decoder.load_value(&db.local_map);
    let mut loaded_properties: std::collections::HashMap<SymId, RuntimeBindingValue> =
        if db.properties_syms.is_empty() {
            db.properties
                .iter()
                .map(|(name, value)| {
                    (
                        intern::intern(name),
                        load_runtime_binding_value(decoder, value),
                    )
                })
                .collect()
        } else {
            db.properties_syms
                .iter()
                .map(|(sym_id, value)| {
                    (
                        load_sym_id(sym_id),
                        load_runtime_binding_value(decoder, value),
                    )
                })
                .collect()
        };
    let mut loaded_undo_list = Value::NIL;
    if let Some(RuntimeBindingValue::Bound(value)) =
        loaded_properties.remove(&intern::intern("buffer-undo-list"))
    {
        loaded_undo_list = value;
    }
    // Reconstruct the alist in the ordered sequence the dump recorded,
    // falling back to sorted remainder for any properties missing from
    // the ordered list. Skip entries that map to BUFFER_OBJFWD slots
    // (they live in the slot table).
    let mut loaded_local_var_alist = Value::NIL;
    let prepend_alist_entry = |alist: &mut Value, sym_id: SymId, binding: RuntimeBindingValue| {
        if crate::buffer::buffer::lookup_buffer_slot(intern::resolve_sym(sym_id)).is_some() {
            return;
        }
        let RuntimeBindingValue::Bound(value) = binding else {
            return;
        };
        let key = Value::from_sym_id(sym_id);
        let cell = Value::cons(key, value);
        *alist = Value::cons(cell, *alist);
    };
    // Walk ordered names first (preserves relative ordering).
    // Because we prepend, iterate in reverse to restore the
    // original head-first order.
    let ordered_local_bindings: Vec<SymId> = if db.local_binding_syms.is_empty() {
        db.local_binding_names
            .iter()
            .map(|name| intern::intern(name))
            .collect()
    } else {
        db.local_binding_syms.iter().map(load_sym_id).collect()
    };
    for sym_id in ordered_local_bindings.into_iter().rev() {
        if sym_id == intern::intern("buffer-undo-list") {
            continue;
        }
        if let Some(binding) = loaded_properties.remove(&sym_id) {
            prepend_alist_entry(&mut loaded_local_var_alist, sym_id, binding);
        }
    }
    // Any remaining unordered properties (older dumps that didn't
    // carry `local_binding_syms`) get appended in sorted order.
    let mut remaining: Vec<_> = loaded_properties.into_iter().collect();
    remaining.sort_by(|left, right| intern::resolve_sym(left.0).cmp(intern::resolve_sym(right.0)));
    for (sym_id, binding) in remaining.into_iter().rev() {
        if sym_id == intern::intern("buffer-undo-list") {
            continue;
        }
        prepend_alist_entry(&mut loaded_local_var_alist, sym_id, binding);
    }
    let undo_list = loaded_undo_list;

    let save_modified_tick = db.save_modified_tick.unwrap_or_else(|| {
        if db.modified {
            db.modified_tick.saturating_sub(1)
        } else {
            db.modified_tick
        }
    });
    let autosave_modified_tick = db.autosave_modified_tick.unwrap_or(save_modified_tick);
    let last_window_start = load_lisp_char_pos(db.last_window_start);

    let text_props = TextPropertyTable::from_dump(
        db.text_props
            .intervals
            .iter()
            .map(|interval| load_property_interval(decoder, interval))
            .collect(),
    );
    text.text_props_replace(text_props);

    text.set_modification_state(db.modified_tick, db.chars_modified_tick, save_modified_tick);

    // v26: resolve state markers (pt/begv/zv) by walking the freshly-built
    // chain on `text` before moving `text` into the Buffer literal below.
    // Falls back to the LoadDecoder's `markers_by_id` index (built during
    // `preload_tagged_heap`) for any state marker missing from `db.markers`,
    // and only allocates a scratch MarkerObj as a final safety net.
    let markers_by_id = &decoder.state.markers_by_id;
    let state_markers = match (db.state_pt_marker, db.state_begv_marker, db.state_zv_marker) {
        (Some(pt_marker), Some(begv_marker), Some(zv_marker)) => {
            let resolve = |mid: u64| -> *mut crate::tagged::header::MarkerObj {
                let chain_hit = text.chain_find_by_id(mid);
                if !chain_hit.is_null() {
                    return chain_hit;
                }
                if let Some(p) = markers_by_id.get(&mid).copied() {
                    return p;
                }
                let scratch =
                    crate::emacs_core::value::Value::make_marker(crate::heap_types::LispMarker {
                        buffer: Some(BufferId(db.id.0)),
                        insertion_type: false,
                        marker_id: Some(mid),
                        bytepos: 0,
                        charpos: 0,
                        last_position_valid: true,
                        next_marker: std::ptr::null_mut(),
                    });
                scratch
                    .as_veclike_ptr()
                    .expect("freshly allocated marker should have a veclike ptr")
                    as *mut crate::tagged::header::MarkerObj
            };
            Some(crate::buffer::buffer::BufferStateMarkers {
                pt_marker,
                begv_marker,
                zv_marker,
                pt_marker_ptr: resolve(pt_marker),
                begv_marker_ptr: resolve(begv_marker),
                zv_marker_ptr: resolve(zv_marker),
            })
        }
        _ => None,
    };

    let name = if let Some(ref name) = db.name_lisp {
        Value::heap_string(load_lisp_string(name))
    } else {
        Value::string(db.name.clone().unwrap_or_default())
    };
    let last_name = if let Some(ref last_name) = db.last_name_lisp {
        Value::heap_string(load_lisp_string(last_name))
    } else if let Some(ref last_name) = db.last_name {
        Value::string(last_name.clone())
    } else {
        Value::NIL
    };

    Buffer::from_dump_parts(BufferDumpParts {
        id: BufferId(db.id.0),
        name,
        last_name,
        base_buffer: db.base_buffer.map(|id| BufferId(id.0)),
        text,
        point: dump_text_position_anchor(pt_char, db.pt),
        mark_marker_id: None,
        mark_marker_ptr: std::ptr::null_mut(),
        accessible_start: dump_text_position_anchor(begv_char, db.begv),
        accessible_end: dump_text_position_anchor(zv_char, db.zv),
        autosave_modified_tick,
        modtime: crate::buffer::VisitedFileModtime::from_dump_halves(
            db.modtime_sec,
            db.modtime_nsec,
        ),
        modtime_size: db.modtime_size,
        last_window_start,
        last_selected_window: None,
        inhibit_buffer_hooks: false,
        state_markers,
        // Phase 10F: per-buffer alist for SYMBOL_LOCALIZED variables.
        // Prefer the dump's `local_var_alist` field when present
        // (new format). Fall back to the alist we rebuilt from the
        // legacy `properties` table for older dumps that didn't
        // carry the alist directly.
        local_var_alist: {
            let dumped = decoder.load_value(&db.local_var_alist);
            if dumped.is_nil() && !loaded_local_var_alist.is_nil() {
                loaded_local_var_alist
            } else {
                dumped
            }
        },
        // Phase 10F: `BVAR(buf, keymap)` — the buffer's local
        // keymap, previously stored inside `BufferLocals::local_map`.
        keymap: loaded_keymap,
        // Phase 11.1: round-trip BUFFER_OBJFWD slots through pdump.
        // Previously blocked on the BLV GC trace bug (5699c3569);
        // with BLVs now traced as roots, slot Values stay live
        // through GCs in `apply_runtime_startup_state` and the
        // round-trip is safe. Falls back to per-slot defaults from
        // `BUFFER_SLOT_INFO` for any slot the dump didn't carry
        // (older format compatibility, or sentinel buffers without
        // a populated slot vector).
        slots: {
            let mut s =
                [crate::emacs_core::value::Value::NIL; crate::buffer::buffer::BUFFER_SLOT_COUNT];
            for info in crate::buffer::buffer::BUFFER_SLOT_INFO {
                s[info.offset.index()] = info.default.to_value();
            }
            for (idx, dumped) in db.slots.iter().enumerate() {
                if idx >= crate::buffer::buffer::BUFFER_SLOT_COUNT {
                    break;
                }
                s[idx] = decoder.load_value(dumped);
            }
            // Legacy header field overrides (older dump compat).
            if let Some(ref fname) = db.file_name_lisp {
                s[crate::buffer::buffer::BUFFER_SLOT_FILE_NAME.index()] =
                    crate::emacs_core::value::Value::heap_string(load_lisp_string(fname));
            } else if let Some(ref fname) = db.file_name {
                s[crate::buffer::buffer::BUFFER_SLOT_FILE_NAME.index()] =
                    crate::emacs_core::value::Value::string(fname);
            }
            if let Some(ref asname) = db.auto_save_file_name_lisp {
                s[crate::buffer::buffer::BUFFER_SLOT_AUTO_SAVE_FILE_NAME.index()] =
                    crate::emacs_core::value::Value::heap_string(load_lisp_string(asname));
            } else if let Some(ref asname) = db.auto_save_file_name {
                s[crate::buffer::buffer::BUFFER_SLOT_AUTO_SAVE_FILE_NAME.index()] =
                    crate::emacs_core::value::Value::string(asname);
            }
            if db.read_only {
                s[crate::buffer::buffer::BUFFER_SLOT_READ_ONLY.index()] =
                    crate::emacs_core::value::Value::T;
            }
            if db.multibyte {
                s[crate::buffer::buffer::BUFFER_SLOT_ENABLE_MULTIBYTE_CHARACTERS.index()] =
                    crate::emacs_core::value::Value::T;
            }
            s
        },
        // Phase 11: per-buffer local-flags bitmap round-trip.
        local_flags: db.local_flags,
        overlays: OverlayList::from_dump(
            db.overlays
                .overlays
                .iter()
                .map(|d| {
                    Value::make_overlay(crate::heap_types::OverlayData {
                        serial: d.serial,
                        plist: decoder.load_value(&d.plist),
                        buffer: d.buffer.map(|id| BufferId(id.0)),
                        start: d.start,
                        end: d.end,
                        position_handle: None,
                        front_advance: d.front_advance,
                        rear_advance: d.rear_advance,
                    })
                })
                .collect(),
        ),
        overlay_modified_tick: 1,
        undo_state: SharedUndoState::from_parts(undo_list, false, false),
        saved_point_before_command,
    })
}

pub(crate) fn load_buffer_manager(
    decoder: &mut LoadDecoder,
    dbm: &DumpBufferManager,
) -> BufferManager {
    // GNU's `point_before_last_command_or_undo` pair are plain statics: they
    // start over on each startup and no command has run yet.  Mint the one
    // cell here and hand every restored buffer a clone of it.
    let saved_point_before_command = SavedPointBeforeCommand::new_editor_global();
    let buffers: FxHashMap<BufferId, Buffer> = dbm
        .buffers
        .iter()
        .map(|(id, buf)| {
            (
                BufferId(id.0),
                load_buffer(decoder, buf, saved_point_before_command.clone()),
            )
        })
        .collect();
    // New in the current dump format: `buffer_defaults` ride through
    // pdump so runtime `setq-default` writes survive. Older dumps
    // (no `buffer_defaults` field) deserialize as an empty Vec via
    // `#[serde(default)]`, and `BufferManager::from_dump` then falls
    // back to the install-time seeds from `BUFFER_SLOT_INFO`.
    let defaults_values: Vec<crate::emacs_core::value::Value> = dbm
        .buffer_defaults
        .iter()
        .map(|value| decoder.load_value(value))
        .collect();
    let dumped_defaults = if defaults_values.is_empty() {
        None
    } else {
        Some(defaults_values.as_slice())
    };
    let order_values: Vec<BufferId> = dbm.buffer_order.iter().map(|id| BufferId(id.0)).collect();
    let dumped_order = if order_values.is_empty() {
        None
    } else {
        Some(order_values.as_slice())
    };
    BufferManager::from_dump(
        buffers,
        dbm.current.map(|id| BufferId(id.0)),
        dbm.next_id,
        dbm.next_marker_id,
        dumped_order,
        dumped_defaults,
        load_buffer_text_backend_kind(dbm.default_text_backend_kind),
        saved_point_before_command,
    )
}

// --- Sub-managers ---

// Restores exact Lisp-string keys from the dump into the runtime's non-moving
// GC representation.
#[allow(clippy::mutable_key_type)]
pub(crate) fn load_autoload_manager(
    decoder: &mut LoadDecoder,
    dam: &DumpAutoloadManager,
) -> AutoloadManager {
    let entries: HashMap<SymId, AutoloadEntry> = if dam.entries_syms.is_empty() {
        dam.entries
            .iter()
            .map(|(k, e)| {
                (
                    crate::emacs_core::intern::intern(k),
                    AutoloadEntry {
                        file: load_lisp_string(&e.file),
                        docstring: e.docstring.as_ref().map(load_lisp_string),
                        interactive: e.interactive,
                        autoload_type: match e.autoload_type {
                            DumpAutoloadType::Function => AutoloadType::Function,
                            DumpAutoloadType::Macro => AutoloadType::Macro,
                            DumpAutoloadType::Keymap => AutoloadType::Keymap,
                        },
                    },
                )
            })
            .collect()
    } else {
        dam.entries_syms
            .iter()
            .map(|(k, e)| {
                (
                    load_sym_id(k),
                    AutoloadEntry {
                        file: load_lisp_string(&e.file),
                        docstring: e.docstring.as_ref().map(load_lisp_string),
                        interactive: e.interactive,
                        autoload_type: match e.autoload_type {
                            DumpAutoloadType::Function => AutoloadType::Function,
                            DumpAutoloadType::Macro => AutoloadType::Macro,
                            DumpAutoloadType::Keymap => AutoloadType::Keymap,
                        },
                    },
                )
            })
            .collect()
    };
    let after_load: HashMap<crate::emacs_core::autoload::AfterLoadKey, Vec<Value>> =
        if !dam.after_load_lisp.is_empty() {
            dam.after_load_lisp
                .iter()
                .map(|(k, v)| {
                    (
                        crate::emacs_core::autoload::AfterLoadKey::from_lisp_string(
                            &load_lisp_string(k),
                        ),
                        v.iter().map(|value| decoder.load_value(value)).collect(),
                    )
                })
                .collect()
        } else {
            dam.after_load
                .iter()
                .map(|(k, v)| {
                    (
                        crate::emacs_core::autoload::AfterLoadKey::from_runtime(k),
                        v.iter().map(|value| decoder.load_value(value)).collect(),
                    )
                })
                .collect()
        };
    AutoloadManager::from_dump(
        entries,
        after_load,
        dam.loaded_files.iter().map(load_lisp_string).collect(),
        if dam.obsolete_functions_syms.is_empty() {
            dam.obsolete_functions
                .iter()
                .map(|(k, (new_name, when))| {
                    (
                        crate::emacs_core::intern::intern(k),
                        (
                            crate::emacs_core::builtins::plain_str_to_lisp_string(new_name, true),
                            crate::emacs_core::builtins::plain_str_to_lisp_string(when, true),
                        ),
                    )
                })
                .collect()
        } else {
            dam.obsolete_functions_syms
                .iter()
                .map(|(k, (new_name, when))| {
                    (
                        load_sym_id(k),
                        (load_lisp_string(new_name), load_lisp_string(when)),
                    )
                })
                .collect()
        },
        if dam.obsolete_variables_syms.is_empty() {
            dam.obsolete_variables
                .iter()
                .map(|(k, (new_name, when))| {
                    (
                        crate::emacs_core::intern::intern(k),
                        (
                            crate::emacs_core::builtins::plain_str_to_lisp_string(new_name, true),
                            crate::emacs_core::builtins::plain_str_to_lisp_string(when, true),
                        ),
                    )
                })
                .collect()
        } else {
            dam.obsolete_variables_syms
                .iter()
                .map(|(k, (new_name, when))| {
                    (
                        load_sym_id(k),
                        (load_lisp_string(new_name), load_lisp_string(when)),
                    )
                })
                .collect()
        },
    )
}

pub(crate) fn load_custom_manager(_dcm: &DumpCustomManager) -> CustomManager {
    // Phase D: auto_buffer_local was a pure mirror of LOCALIZED BLV
    // local_if_set flags. Those are restored when symbols are loaded
    // from the dump via their BLV state. No runtime set needed.
    CustomManager {}
}

fn load_mode_custom_type(decoder: &mut LoadDecoder, ct: &DumpModeCustomType) -> ModeCustomType {
    match ct {
        DumpModeCustomType::Boolean => ModeCustomType::Boolean,
        DumpModeCustomType::Integer => ModeCustomType::Integer,
        DumpModeCustomType::Float => ModeCustomType::Float,
        DumpModeCustomType::String => ModeCustomType::String,
        DumpModeCustomType::Symbol => ModeCustomType::Symbol,
        DumpModeCustomType::Sexp => ModeCustomType::Sexp,
        DumpModeCustomType::Choice(choices) => ModeCustomType::Choice(
            choices
                .iter()
                .map(|(s, v)| (s.clone(), decoder.load_value(v)))
                .collect(),
        ),
        DumpModeCustomType::List(inner) => {
            ModeCustomType::List(Box::new(load_mode_custom_type(decoder, inner)))
        }
        DumpModeCustomType::Alist(k, v) => ModeCustomType::Alist(
            Box::new(load_mode_custom_type(decoder, k)),
            Box::new(load_mode_custom_type(decoder, v)),
        ),
        DumpModeCustomType::Plist(k, v) => ModeCustomType::Plist(
            Box::new(load_mode_custom_type(decoder, k)),
            Box::new(load_mode_custom_type(decoder, v)),
        ),
        DumpModeCustomType::Color => ModeCustomType::Color,
        DumpModeCustomType::Face => ModeCustomType::Face,
        DumpModeCustomType::File => ModeCustomType::File,
        DumpModeCustomType::Directory => ModeCustomType::Directory,
        DumpModeCustomType::Function => ModeCustomType::Function,
        DumpModeCustomType::Variable => ModeCustomType::Variable,
        DumpModeCustomType::Hook => ModeCustomType::Hook,
        DumpModeCustomType::Coding => ModeCustomType::Coding,
    }
}

pub(crate) fn load_mode_registry(
    decoder: &mut LoadDecoder,
    dmr: &DumpModeRegistry,
) -> ModeRegistry {
    let major_modes: HashMap<SymId, MajorMode> = dmr
        .major_modes
        .iter()
        .map(|(k, m)| {
            (
                load_sym_id(k),
                MajorMode {
                    pretty_name: load_lisp_string(&m.pretty_name),
                    parent: decoder.load_opt_value(&m.parent),
                    mode_hook: decoder.load_value(&m.mode_hook),
                    keymap_name: decoder.load_opt_value(&m.keymap_name),
                    syntax_table_name: decoder.load_opt_value(&m.syntax_table_name),
                    abbrev_table_name: decoder.load_opt_value(&m.abbrev_table_name),
                    font_lock: m.font_lock.as_ref().map(|fl| FontLockDefaults {
                        keywords: fl
                            .keywords
                            .iter()
                            .map(|kw| FontLockKeyword {
                                pattern: kw
                                    .pattern_lisp
                                    .as_ref()
                                    .map(load_lisp_string)
                                    .unwrap_or_else(|| {
                                        LispString::from_utf8(
                                            kw.pattern.as_deref().unwrap_or_default(),
                                        )
                                    }),
                                face: kw.face_sym.as_ref().map(load_sym_id).unwrap_or_else(|| {
                                    crate::emacs_core::intern::intern(
                                        kw.face.as_deref().unwrap_or_default(),
                                    )
                                }),
                                group: kw.group,
                                override_: kw.override_,
                                laxmatch: kw.laxmatch,
                            })
                            .collect(),
                        case_fold: fl.case_fold,
                        syntax_table: fl
                            .syntax_table_lisp
                            .as_ref()
                            .map(load_lisp_string)
                            .or_else(|| fl.syntax_table.as_deref().map(LispString::from_utf8)),
                    }),
                    body: decoder.load_opt_value(&m.body),
                },
            )
        })
        .collect();
    let minor_modes: HashMap<SymId, MinorMode> = dmr
        .minor_modes
        .iter()
        .map(|(k, m)| {
            (
                load_sym_id(k),
                MinorMode {
                    lighter: m.lighter.as_ref().map(load_lisp_string),
                    keymap_name: decoder.load_opt_value(&m.keymap_name),
                    global: m.global,
                    body: decoder.load_opt_value(&m.body),
                },
            )
        })
        .collect();
    let custom_variables: HashMap<SymId, ModeCustomVariable> = dmr
        .custom_variables
        .iter()
        .map(|(k, cv)| {
            (
                load_sym_id(k),
                ModeCustomVariable {
                    default_value: decoder.load_value(&cv.default_value),
                    doc: cv.doc.as_ref().map(load_lisp_string),
                    type_: load_mode_custom_type(decoder, &cv.custom_type),
                    group: decoder.load_opt_value(&cv.group),
                    set_function: decoder.load_opt_value(&cv.set_function),
                    get_function: decoder.load_opt_value(&cv.get_function),
                    tag: cv.tag.as_ref().map(load_lisp_string),
                },
            )
        })
        .collect();
    let custom_groups: HashMap<SymId, ModeCustomGroup> = dmr
        .custom_groups
        .iter()
        .map(|(k, g)| {
            (
                load_sym_id(k),
                ModeCustomGroup {
                    doc: g.doc.as_ref().map(load_lisp_string),
                    parent: decoder.load_opt_value(&g.parent),
                    members: g
                        .members
                        .iter()
                        .map(|value| decoder.load_value(value))
                        .collect(),
                },
            )
        })
        .collect();
    ModeRegistry::from_dump(
        major_modes,
        minor_modes,
        dmr.buffer_major_modes
            .iter()
            .map(|(k, v)| (*k, decoder.load_value(v)))
            .collect(),
        dmr.buffer_minor_modes
            .iter()
            .map(|(k, v)| {
                (
                    *k,
                    v.iter().map(|value| decoder.load_value(value)).collect(),
                )
            })
            .collect(),
        dmr.global_minor_modes
            .iter()
            .map(|value| decoder.load_value(value))
            .collect(),
        if !dmr.auto_mode_alist_lisp.is_empty() {
            dmr.auto_mode_alist_lisp
                .iter()
                .map(|(pattern, value)| (load_lisp_string(pattern), decoder.load_value(value)))
                .collect()
        } else {
            dmr.auto_mode_alist
                .iter()
                .map(|(pattern, value)| (LispString::from_utf8(pattern), decoder.load_value(value)))
                .collect()
        },
        custom_variables,
        custom_groups,
        decoder.load_value(&dmr.fundamental_mode),
    )
}

fn rebuild_coding_alias_order(
    systems: &HashMap<SymId, CodingSystemInfo>,
    aliases: &HashMap<SymId, SymId>,
) -> HashMap<SymId, Vec<SymId>> {
    let mut order: HashMap<SymId, Vec<SymId>> =
        systems.keys().copied().map(|id| (id, vec![id])).collect();
    let mut alias_pairs: Vec<(SymId, SymId)> = aliases.iter().map(|(k, v)| (*k, *v)).collect();
    alias_pairs.sort_by(|(left, _), (right, _)| {
        intern::resolve_sym(*left).cmp(intern::resolve_sym(*right))
    });
    for (alias, target) in alias_pairs {
        let aliases = order.entry(target).or_insert_with(|| vec![target]);
        if !aliases.contains(&alias) {
            aliases.push(alias);
        }
    }
    order
}

pub(crate) fn load_coding_system_manager(
    decoder: &mut LoadDecoder,
    dcsm: &DumpCodingSystemManager,
) -> CodingSystemManager {
    let systems: HashMap<SymId, CodingSystemInfo> = if dcsm.systems_syms.is_empty() {
        dcsm.systems
            .iter()
            .map(|(k, v)| {
                (
                    crate::emacs_core::intern::intern(k),
                    CodingSystemInfo {
                        name: crate::emacs_core::intern::intern(
                            v.name
                                .as_deref()
                                .expect("legacy coding dump entry missing name"),
                        ),
                        coding_type: crate::emacs_core::intern::intern(
                            v.coding_type
                                .as_deref()
                                .expect("legacy coding dump entry missing coding type"),
                        ),
                        mnemonic: v.mnemonic,
                        eol_type: match v.eol_type {
                            DumpEolType::Unix => EolType::Unix,
                            DumpEolType::Dos => EolType::Dos,
                            DumpEolType::Mac => EolType::Mac,
                            DumpEolType::Undecided => EolType::Undecided,
                        },
                        ascii_compatible_p: v.ascii_compatible_p,
                        charset_list: v
                            .charset_list
                            .iter()
                            .map(|name| crate::emacs_core::intern::intern(name))
                            .collect(),
                        post_read_conversion: v
                            .post_read_conversion
                            .as_ref()
                            .map(|name| crate::emacs_core::intern::intern(name)),
                        pre_write_conversion: v
                            .pre_write_conversion
                            .as_ref()
                            .map(|name| crate::emacs_core::intern::intern(name)),
                        default_char: v.default_char,
                        for_unibyte: v.for_unibyte,
                        properties: v
                            .properties
                            .iter()
                            .map(|(k, v)| {
                                (crate::emacs_core::intern::intern(k), decoder.load_value(v))
                            })
                            .collect(),
                        int_properties: v
                            .int_properties
                            .iter()
                            .map(|(k, v)| (*k, decoder.load_value(v)))
                            .collect(),
                    },
                )
            })
            .collect()
    } else {
        dcsm.systems_syms
            .iter()
            .map(|(k, v)| {
                (
                    load_sym_id(k),
                    CodingSystemInfo {
                        name: v.name_sym.as_ref().map(load_sym_id).unwrap_or_else(|| {
                            crate::emacs_core::intern::intern(
                                v.name.as_deref().expect("coding dump entry missing name"),
                            )
                        }),
                        coding_type: v.coding_type_sym.as_ref().map(load_sym_id).unwrap_or_else(
                            || {
                                crate::emacs_core::intern::intern(
                                    v.coding_type
                                        .as_deref()
                                        .expect("coding dump entry missing coding type"),
                                )
                            },
                        ),
                        mnemonic: v.mnemonic,
                        eol_type: match v.eol_type {
                            DumpEolType::Unix => EolType::Unix,
                            DumpEolType::Dos => EolType::Dos,
                            DumpEolType::Mac => EolType::Mac,
                            DumpEolType::Undecided => EolType::Undecided,
                        },
                        ascii_compatible_p: v.ascii_compatible_p,
                        charset_list: if v.charset_list_syms.is_empty() {
                            v.charset_list
                                .iter()
                                .map(|name| crate::emacs_core::intern::intern(name))
                                .collect()
                        } else {
                            v.charset_list_syms.iter().map(load_sym_id).collect()
                        },
                        post_read_conversion: v
                            .post_read_conversion_sym
                            .as_ref()
                            .map(load_sym_id)
                            .or_else(|| {
                                v.post_read_conversion
                                    .as_ref()
                                    .map(|name| crate::emacs_core::intern::intern(name))
                            }),
                        pre_write_conversion: v
                            .pre_write_conversion_sym
                            .as_ref()
                            .map(load_sym_id)
                            .or_else(|| {
                                v.pre_write_conversion
                                    .as_ref()
                                    .map(|name| crate::emacs_core::intern::intern(name))
                            }),
                        default_char: v.default_char,
                        for_unibyte: v.for_unibyte,
                        properties: if v.properties_syms.is_empty() {
                            v.properties
                                .iter()
                                .map(|(k, v)| {
                                    (crate::emacs_core::intern::intern(k), decoder.load_value(v))
                                })
                                .collect()
                        } else {
                            v.properties_syms
                                .iter()
                                .map(|(k, v)| (load_sym_id(k), decoder.load_value(v)))
                                .collect()
                        },
                        int_properties: v
                            .int_properties
                            .iter()
                            .map(|(k, v)| (*k, decoder.load_value(v)))
                            .collect(),
                    },
                )
            })
            .collect()
    };
    let aliases: HashMap<SymId, SymId> = if dcsm.aliases_syms.is_empty() {
        dcsm.aliases
            .iter()
            .map(|(k, v)| {
                (
                    crate::emacs_core::intern::intern(k),
                    crate::emacs_core::intern::intern(v),
                )
            })
            .collect()
    } else {
        dcsm.aliases_syms
            .iter()
            .map(|(k, v)| (load_sym_id(k), load_sym_id(v)))
            .collect()
    };
    let alias_order: HashMap<SymId, Vec<SymId>> = if dcsm.alias_order_syms.is_empty() {
        if dcsm.alias_order.is_empty() {
            rebuild_coding_alias_order(&systems, &aliases)
        } else {
            dcsm.alias_order
                .iter()
                .map(|(k, v)| {
                    (
                        crate::emacs_core::intern::intern(k),
                        v.iter()
                            .map(|name| crate::emacs_core::intern::intern(name))
                            .collect(),
                    )
                })
                .collect()
        }
    } else {
        dcsm.alias_order_syms
            .iter()
            .map(|(k, v)| (load_sym_id(k), v.iter().map(load_sym_id).collect()))
            .collect()
    };
    CodingSystemManager::from_dump(
        systems,
        aliases,
        alias_order,
        if dcsm.priority_syms.is_empty() {
            dcsm.priority
                .iter()
                .map(|name| crate::emacs_core::intern::intern(name))
                .collect()
        } else {
            dcsm.priority_syms.iter().map(load_sym_id).collect()
        },
        dcsm.keyboard_coding_sym
            .as_ref()
            .map(load_sym_id)
            .unwrap_or_else(|| {
                crate::emacs_core::intern::intern(
                    dcsm.keyboard_coding
                        .as_deref()
                        .expect("legacy coding dump missing keyboard coding"),
                )
            }),
        dcsm.terminal_coding_sym
            .as_ref()
            .map(load_sym_id)
            .unwrap_or_else(|| {
                crate::emacs_core::intern::intern(
                    dcsm.terminal_coding
                        .as_deref()
                        .expect("legacy coding dump missing terminal coding"),
                )
            }),
    )
}

pub(crate) fn load_charset_registry(decoder: &mut LoadDecoder, dcr: &DumpCharsetRegistry) {
    let snapshot = CharsetRegistrySnapshot {
        charsets: dcr
            .charsets
            .iter()
            .map(|info| CharsetInfoSnapshot {
                id: info.id,
                name: info.name_sym.as_ref().map(load_sym_id).unwrap_or_else(|| {
                    crate::emacs_core::intern::intern(
                        info.name
                            .as_deref()
                            .expect("legacy charset dump entry missing name"),
                    )
                }),
                dimension: info.dimension,
                code_space: info.code_space,
                min_code: info.min_code,
                max_code: info.max_code,
                iso_final_char: info.iso_final_char,
                iso_revision: info.iso_revision,
                emacs_mule_id: info.emacs_mule_id,
                ascii_compatible_p: info.ascii_compatible_p,
                supplementary_p: info.supplementary_p,
                unified_p: info.unified_p,
                invalid_code: info.invalid_code,
                unify_map: decoder.load_value(&info.unify_map),
                method: match &info.method {
                    DumpCharsetMethod::Offset(offset) => CharsetMethodSnapshot::Offset(*offset),
                    DumpCharsetMethod::Map(map_name) => {
                        CharsetMethodSnapshot::Map(map_name.clone())
                    }
                    DumpCharsetMethod::Subset(subset) => CharsetMethodSnapshot::Subset(
                        crate::emacs_core::charset::CharsetSubsetSpecSnapshot {
                            parent: subset.parent_sym.as_ref().map(load_sym_id).unwrap_or_else(
                                || {
                                    crate::emacs_core::intern::intern(
                                        subset
                                            .parent
                                            .as_deref()
                                            .expect("legacy charset subset missing parent"),
                                    )
                                },
                            ),
                            parent_min_code: subset.parent_min_code,
                            parent_max_code: subset.parent_max_code,
                            offset: subset.offset,
                        },
                    ),
                    DumpCharsetMethod::SupersetSyms(members) => CharsetMethodSnapshot::Superset(
                        members
                            .iter()
                            .map(|(name, offset)| (load_sym_id(name), *offset))
                            .collect(),
                    ),
                    DumpCharsetMethod::Superset(members) => CharsetMethodSnapshot::Superset(
                        members
                            .iter()
                            .map(|(name, offset)| {
                                (crate::emacs_core::intern::intern(name), *offset)
                            })
                            .collect(),
                    ),
                },
                plist: if info.plist_syms.is_empty() {
                    info.plist
                        .iter()
                        .map(|(key, value)| {
                            (
                                crate::emacs_core::intern::intern(key),
                                decoder.load_value(value),
                            )
                        })
                        .collect()
                } else {
                    info.plist_syms
                        .iter()
                        .map(|(key, value)| (load_sym_id(key), decoder.load_value(value)))
                        .collect()
                },
            })
            .collect(),
        priority: if dcr.priority_syms.is_empty() {
            dcr.priority
                .iter()
                .map(|name| crate::emacs_core::intern::intern(name))
                .collect()
        } else {
            dcr.priority_syms.iter().map(load_sym_id).collect()
        },
        next_id: dcr.next_id,
        // The binary dump does not carry GNU's `Vcharset_non_preferred_head`
        // boundary; a freshly loaded session reproduces GNU's dumped default
        // (only `ascii` preferred -> non-ASCII BMP chars classify as `unicode`),
        // and `set-charset-priority` / `set-language-environment` reset it at
        // runtime. Index 1 == everything after `ascii` (priority[0]) is
        // non-preferred, matching `CharsetRegistry::new`.
        non_preferred_head: Some(1),
    };
    restore_charset_registry(snapshot);
}

fn load_font_width(width: &DumpFontWidth) -> FontWidth {
    match width {
        DumpFontWidth::UltraCondensed => FontWidth::UltraCondensed,
        DumpFontWidth::ExtraCondensed => FontWidth::ExtraCondensed,
        DumpFontWidth::Condensed => FontWidth::Condensed,
        DumpFontWidth::SemiCondensed => FontWidth::SemiCondensed,
        DumpFontWidth::Normal => FontWidth::Normal,
        DumpFontWidth::SemiExpanded => FontWidth::SemiExpanded,
        DumpFontWidth::Expanded => FontWidth::Expanded,
        DumpFontWidth::ExtraExpanded => FontWidth::ExtraExpanded,
        DumpFontWidth::UltraExpanded => FontWidth::UltraExpanded,
    }
}

fn load_font_repertory(repertory: &DumpFontRepertory) -> FontRepertory {
    match repertory {
        DumpFontRepertory::Charset(name) => {
            FontRepertory::Charset(crate::emacs_core::intern::intern(name))
        }
        DumpFontRepertory::CharsetSym(name) => FontRepertory::Charset(load_sym_id(name)),
        DumpFontRepertory::CharTableRanges(ranges) => {
            FontRepertory::CharTableRanges(ranges.clone())
        }
    }
}

fn load_font_spec_entry(entry: &DumpFontSpecEntry) -> FontSpecEntry {
    match entry {
        DumpFontSpecEntry::Font(spec) => FontSpecEntry::Font(StoredFontSpec {
            family: spec.family_sym.as_ref().map(load_sym_id).or_else(|| {
                spec.family
                    .as_deref()
                    .map(crate::emacs_core::intern::intern)
            }),
            registry: spec.registry_sym.as_ref().map(load_sym_id).or_else(|| {
                spec.registry
                    .as_deref()
                    .map(crate::emacs_core::intern::intern)
            }),
            lang: spec
                .lang_sym
                .as_ref()
                .map(load_sym_id)
                .or_else(|| spec.lang.as_deref().map(crate::emacs_core::intern::intern)),
            weight: spec.weight.map(FontWeight::from_dump_code),
            slant: spec.slant.as_ref().map(load_font_slant),
            width: spec.width.as_ref().map(load_font_width),
            repertory: spec.repertory.as_ref().map(load_font_repertory),
        }),
        DumpFontSpecEntry::ExplicitNone => FontSpecEntry::ExplicitNone,
    }
}

pub(crate) fn load_fontset_registry(dfr: &DumpFontsetRegistry) {
    let snapshot = FontsetRegistrySnapshot {
        ordered_names: if dfr.ordered_names_lisp.is_empty() {
            dfr.ordered_names
                .iter()
                .map(|name| LispString::from_utf8(name))
                .collect()
        } else {
            dfr.ordered_names_lisp
                .iter()
                .map(load_lisp_string)
                .collect()
        },
        alias_to_name: if dfr.alias_to_name_lisp.is_empty() {
            dfr.alias_to_name
                .iter()
                .map(|(alias, name)| (LispString::from_utf8(alias), LispString::from_utf8(name)))
                .collect()
        } else {
            dfr.alias_to_name_lisp
                .iter()
                .map(|(alias, name)| (load_lisp_string(alias), load_lisp_string(name)))
                .collect()
        },
        fontsets: if dfr.fontsets_lisp.is_empty() {
            dfr.fontsets
                .iter()
                .map(|(name, data)| {
                    (
                        LispString::from_utf8(name),
                        FontsetDataSnapshot {
                            ranges: data
                                .ranges
                                .iter()
                                .map(|range| FontsetRangeEntrySnapshot {
                                    from: range.from,
                                    to: range.to,
                                    entries: range
                                        .entries
                                        .iter()
                                        .map(load_font_spec_entry)
                                        .collect(),
                                })
                                .collect(),
                            fallback: data
                                .fallback
                                .as_ref()
                                .map(|entries| entries.iter().map(load_font_spec_entry).collect()),
                        },
                    )
                })
                .collect()
        } else {
            dfr.fontsets_lisp
                .iter()
                .map(|(name, data)| {
                    (
                        load_lisp_string(name),
                        FontsetDataSnapshot {
                            ranges: data
                                .ranges
                                .iter()
                                .map(|range| FontsetRangeEntrySnapshot {
                                    from: range.from,
                                    to: range.to,
                                    entries: range
                                        .entries
                                        .iter()
                                        .map(load_font_spec_entry)
                                        .collect(),
                                })
                                .collect(),
                            fallback: data
                                .fallback
                                .as_ref()
                                .map(|entries| entries.iter().map(load_font_spec_entry).collect()),
                        },
                    )
                })
                .collect()
        },
        generation: dfr.generation,
    };
    restore_fontset_registry(snapshot);
}

fn load_color(c: &DumpColor) -> Color {
    // No `terminal` in the dump, and none is wanted: a realized terminal colour
    // is the index `tty-color-desc` returned for a palette that a *terminal*
    // registered, and a dump is written in batch with no terminal at all. The
    // face table is re-realized per frame by
    // `sync_runtime_face_table_from_frame_lisp_faces`, which is where the index
    // is filled in.
    Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
        terminal: None,
    }
}

fn load_font_slant(s: &DumpFontSlant) -> FontSlant {
    match s {
        DumpFontSlant::Normal => FontSlant::Normal,
        DumpFontSlant::Italic => FontSlant::Italic,
        DumpFontSlant::Oblique => FontSlant::Oblique,
        DumpFontSlant::ReverseItalic => FontSlant::ReverseItalic,
        DumpFontSlant::ReverseOblique => FontSlant::ReverseOblique,
    }
}

fn load_face(decoder: &mut LoadDecoder, df: &DumpFace) -> Face {
    Face {
        foreground: df.foreground.map(|c| load_color(&c)),
        background: df.background.map(|c| load_color(&c)),
        family: df
            .family_value
            .as_ref()
            .map(|value| decoder.load_value(value))
            .or_else(|| df.family.as_ref().map(Value::string)),
        height: df.height.as_ref().map(|h| match h {
            DumpFaceHeight::Absolute(n) => FaceHeight::Absolute(*n),
            DumpFaceHeight::Relative(f) => FaceHeight::Relative(*f),
        }),
        weight: df.weight.map(FontWeight::from_dump_code),
        slant: df.slant.as_ref().map(load_font_slant),
        underline: if df.underline_disabled {
            FaceDecoration::Disabled
        } else if let Some(u) = &df.underline {
            FaceDecoration::Enabled(Underline {
                style: match u.style {
                    DumpUnderlineStyle::Line => UnderlineStyle::Line,
                    DumpUnderlineStyle::Wave => UnderlineStyle::Wave,
                    DumpUnderlineStyle::Dot => UnderlineStyle::Dots,
                    DumpUnderlineStyle::Dash => UnderlineStyle::Dashes,
                    DumpUnderlineStyle::DoubleLine => UnderlineStyle::DoubleLine,
                },
                color: u.color.map(|c| load_color(&c)),
                position: u
                    .position
                    .map(|pixels_above| UnderlinePosition::DescentLine {
                        pixels_above: pixels_above.max(0) as u32,
                    })
                    .unwrap_or(UnderlinePosition::FontMetric),
            })
        } else {
            FaceDecoration::Unspecified
        },
        overline: df.overline,
        strike_through: df.strike_through,
        box_border: if df.box_disabled {
            FaceDecoration::Disabled
        } else if let Some(b) = df.box_border.as_ref() {
            FaceDecoration::Enabled(BoxBorder {
                color: b.color.map(|c| load_color(&c)),
                width: b.width,
                style: match b.style {
                    DumpBoxStyle::Flat => BoxStyle::Flat,
                    DumpBoxStyle::Raised => BoxStyle::Raised,
                    DumpBoxStyle::Pressed => BoxStyle::Pressed,
                },
            })
        } else {
            FaceDecoration::Unspecified
        },
        inverse_video: df.inverse_video,
        stipple: df
            .stipple_value
            .as_ref()
            .map(|value| decoder.load_value(value))
            .or_else(|| df.stipple.as_ref().map(Value::string)),
        extend: df.extend,
        inherit: {
            // Dump legacy schema: Vec<symbol-name>. Reconstruct as a
            // single symbol if exactly one, or a face_ref list otherwise,
            // matching GNU's LFACE_INHERIT_INDEX value shape.
            let syms: Vec<Value> = if !df.inherit_syms.is_empty() {
                df.inherit_syms
                    .iter()
                    .map(|name| Value::from_sym_id(load_sym_id(name)))
                    .collect()
            } else {
                df.inherit
                    .iter()
                    .map(|name| Value::symbol(name.as_str()))
                    .collect()
            };
            match syms.len() {
                0 => None,
                1 => Some(syms[0]),
                _ => Some(Value::list(syms)),
            }
        },
        overstrike: df.overstrike,
        doc: df
            .doc_value
            .as_ref()
            .map(|value| decoder.load_value(value))
            .or_else(|| df.doc.as_ref().map(Value::string)),
        overline_color: None,
        strike_through_color: None,
        distant_foreground: None,
        foundry: df
            .foundry_value
            .as_ref()
            .map(|value| decoder.load_value(value))
            .or_else(|| df.foundry.as_ref().map(Value::string)),
        width: None,
    }
}

pub(crate) fn load_face_table(decoder: &mut LoadDecoder, dft: &DumpFaceTable) -> FaceTable {
    if !dft.face_ids.is_empty() {
        FaceTable::from_dump_sym_ids(
            dft.face_ids
                .iter()
                .map(|(k, f)| (load_sym_id(k), load_face(decoder, f)))
                .collect(),
        )
    } else {
        FaceTable::from_dump(
            dft.faces
                .iter()
                .map(|(k, f)| (k.clone(), load_face(decoder, f)))
                .collect(),
        )
    }
}

pub(crate) fn load_rectangle(dr: &DumpRectangleState) -> RectangleState {
    RectangleState {
        killed: dr.killed.iter().map(load_lisp_string).collect(),
    }
}

pub(crate) fn load_kmacro(decoder: &mut LoadDecoder, dkm: &DumpKmacroManager) -> KmacroManager {
    KmacroManager {
        macro_ring: dkm
            .macro_ring
            .iter()
            .map(|m| m.iter().map(|value| decoder.load_value(value)).collect())
            .collect(),
        counter: dkm.counter,
        counter_format: dkm
            .counter_format_lisp
            .as_ref()
            .map(load_lisp_string)
            .or_else(|| {
                dkm.counter_format
                    .as_ref()
                    .map(|text| crate::emacs_core::builtins::plain_str_to_lisp_string(text, true))
            })
            .unwrap_or_else(|| crate::heap_types::LispString::from_utf8("%d")),
    }
}

pub(crate) fn load_register_manager(
    decoder: &mut LoadDecoder,
    drm: &DumpRegisterManager,
) -> RegisterManager {
    let registers: HashMap<char, RegisterContent> = drm
        .registers
        .iter()
        .map(|(c, r)| {
            (
                *c,
                match r {
                    DumpRegisterContent::Text {
                        data,
                        size,
                        size_byte,
                    } => RegisterContent::Text(LispString::from_dump(
                        data.clone(),
                        *size,
                        *size_byte,
                    )),
                    DumpRegisterContent::Number(n) => RegisterContent::Number(*n),
                    DumpRegisterContent::Marker(v) => {
                        RegisterContent::Marker(decoder.load_value(v))
                    }
                    DumpRegisterContent::Rectangle(lines) => {
                        RegisterContent::Rectangle(lines.iter().map(load_lisp_string).collect())
                    }
                    DumpRegisterContent::FrameConfig(v) => {
                        RegisterContent::FrameConfig(decoder.load_value(v))
                    }
                    DumpRegisterContent::File(s) => RegisterContent::File(load_lisp_string(s)),
                    DumpRegisterContent::KbdMacro(keys) => RegisterContent::KbdMacro(
                        keys.iter().map(|value| decoder.load_value(value)).collect(),
                    ),
                },
            )
        })
        .collect();
    RegisterManager::from_dump(registers)
}

#[allow(clippy::mutable_key_type)] // restores exact Lisp bookmark-key semantics
pub(crate) fn load_bookmark_manager(dbm: &DumpBookmarkManager) -> BookmarkManager {
    let bookmarks: HashMap<crate::emacs_core::bookmark::BookmarkKey, Bookmark> =
        if !dbm.bookmarks_lisp.is_empty() {
            dbm.bookmarks_lisp
                .iter()
                .map(|(k, b)| {
                    (
                        crate::emacs_core::bookmark::BookmarkKey::from_lisp_string(
                            &load_lisp_string(k),
                        ),
                        Bookmark {
                            name: load_lisp_string(&b.name),
                            filename: b.filename.as_ref().map(load_lisp_string),
                            position: LispCharPos1::from_one_based_usize(b.position),
                            front_context: b.front_context.as_ref().map(load_lisp_string),
                            rear_context: b.rear_context.as_ref().map(load_lisp_string),
                            annotation: b.annotation.as_ref().map(load_lisp_string),
                            handler: b.handler.as_ref().map(load_lisp_string),
                        },
                    )
                })
                .collect()
        } else {
            dbm.bookmarks
                .iter()
                .map(|(k, b)| {
                    (
                        crate::emacs_core::bookmark::BookmarkKey::from_lisp_string(
                            &crate::emacs_core::builtins::plain_str_to_lisp_string(k, true),
                        ),
                        Bookmark {
                            name: load_lisp_string(&b.name),
                            filename: b.filename.as_ref().map(load_lisp_string),
                            position: LispCharPos1::from_one_based_usize(b.position),
                            front_context: b.front_context.as_ref().map(load_lisp_string),
                            rear_context: b.rear_context.as_ref().map(load_lisp_string),
                            annotation: b.annotation.as_ref().map(load_lisp_string),
                            handler: b.handler.as_ref().map(load_lisp_string),
                        },
                    )
                })
                .collect()
        };
    BookmarkManager::from_dump(bookmarks, dbm.recent.iter().map(load_lisp_string).collect())
}

pub(crate) fn load_abbrev_manager(dam: &DumpAbbrevManager) -> AbbrevManager {
    let tables: HashMap<SymId, AbbrevTable> = if !dam.tables_syms.is_empty() {
        dam.tables_syms
            .iter()
            .map(|(sym, t)| {
                (
                    load_sym_id(sym),
                    AbbrevTable {
                        name: load_lisp_string(&t.name),
                        abbrevs: t
                            .abbrevs
                            .iter()
                            .map(|(k, a)| {
                                (
                                    load_lisp_string(k),
                                    Abbrev {
                                        expansion: load_lisp_string(&a.expansion),
                                        hook: a.hook.as_ref().map(load_lisp_string),
                                        count: a.count,
                                        system: a.system,
                                    },
                                )
                            })
                            .collect(),
                        parent: t.parent.as_ref().map(load_lisp_string),
                        case_fixed: t.case_fixed,
                        enable_quoting: t.enable_quoting,
                    },
                )
            })
            .collect()
    } else {
        dam.tables
            .iter()
            .map(|(k, t)| {
                (
                    intern::intern(k),
                    AbbrevTable {
                        name: load_lisp_string(&t.name),
                        abbrevs: t
                            .abbrevs
                            .iter()
                            .map(|(k, a)| {
                                (
                                    load_lisp_string(k),
                                    Abbrev {
                                        expansion: load_lisp_string(&a.expansion),
                                        hook: a.hook.as_ref().map(load_lisp_string),
                                        count: a.count,
                                        system: a.system,
                                    },
                                )
                            })
                            .collect(),
                        parent: t.parent.as_ref().map(load_lisp_string),
                        case_fixed: t.case_fixed,
                        enable_quoting: t.enable_quoting,
                    },
                )
            })
            .collect()
    };
    let global_table_sym = dam
        .global_table_sym
        .map(|sym| load_sym_id(&sym))
        .unwrap_or_else(|| intern::intern_lisp_string(&load_lisp_string(&dam.global_table_name)));
    AbbrevManager::from_dump(tables, global_table_sym, dam.abbrev_mode)
}

pub(crate) fn load_interactive_registry(
    decoder: &mut LoadDecoder,
    dir: &DumpInteractiveRegistry,
) -> InteractiveRegistry {
    let specs: HashMap<SymId, InteractiveSpec> = dir
        .specs
        .iter()
        .map(|(k, s)| {
            (
                load_sym_id(k),
                InteractiveSpec {
                    spec: decoder.load_value(&s.spec),
                },
            )
        })
        .collect();
    InteractiveRegistry::from_dump(specs)
}

pub(crate) fn load_watcher_list(
    decoder: &mut LoadDecoder,
    dwl: &DumpVariableWatcherList,
) -> VariableWatcherList {
    let watchers: FxHashMap<SymId, Vec<VariableWatcher>> = dwl
        .watchers
        .iter()
        .map(|(k, callbacks)| {
            (
                load_sym_id(k),
                callbacks
                    .iter()
                    .map(|v| VariableWatcher {
                        callback: decoder.load_value(v),
                    })
                    .collect(),
            )
        })
        .collect();
    VariableWatcherList::from_dump(watchers)
}
