//! Heap object headers and layouts for the tagged pointer GC.
//!
//! # Object categories
//!
//! **Cons cells** — no header, just `(car, cdr)` = 16 bytes.
//! GC uses an external mark bitmap in the cons block allocator.
//!
//! **Strings, Floats** — have a `GcHeader` for mark bit and sweep list.
//!
//! **Vectorlike objects** — have a `VecLikeHeader` (extends `GcHeader`)
//! with a `type_tag` field distinguishing vectors, hash tables, lambdas,
//! macros, bytecode, buffers, markers, overlays, records, etc.

use super::value::TaggedValue;
use malachite::integer::Integer;
use neomacs_display_protocol::WebViewId;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// ---------------------------------------------------------------------------
// ConsCell — no header, minimal size
// ---------------------------------------------------------------------------

/// A cons cell: two tagged values, no header.
///
/// 16 bytes on 64-bit. GC marks cons cells via an external bitmap
/// in the block allocator, not via an in-object flag.
#[derive(Clone, Copy)]
#[repr(C)]
pub union ConsCdrOrNext {
    pub cdr: TaggedValue,
    pub next_free: *mut ConsCell,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct ConsCell {
    pub car: TaggedValue,
    pub cdr_or_next: ConsCdrOrNext,
}

impl ConsCell {
    /// Read the live cons cell's cdr union member.
    ///
    /// # Safety
    ///
    /// `self` must represent an allocated cons cell, so `cdr_or_next.cdr` is
    /// the active union field rather than `next_free`.
    #[inline]
    pub unsafe fn cdr(&self) -> TaggedValue {
        unsafe { self.cdr_or_next.cdr }
    }

    /// Atomic (acquire) read of `car` — used by GC tracing, which may run on a
    /// concurrent collector thread while the mutator stores via `set_car`.
    /// Acquire pairs with the release publication stores (see the module's
    /// publication-ordering contract on [`load_value_atomic`]): dereferencing
    /// a freshly-published heap pointer loaded here observes the pointee's
    /// fully-written `GcHeader`.
    ///
    /// # Safety
    ///
    /// `self` must point to a live cons cell whose words are accessed only by
    /// the atomic load/store helpers while concurrent collection is possible.
    #[inline]
    pub unsafe fn load_car(&self) -> TaggedValue {
        let p = &self.car as *const TaggedValue as *const AtomicUsize;
        TaggedValue(unsafe { (*p).load(Ordering::Acquire) })
    }

    /// Atomic (acquire) read of `cdr` (the cdr/next-free union word).
    ///
    /// # Safety
    ///
    /// `self` must represent a live cons cell, making `cdr` the active union
    /// field, and concurrent writes must use [`Self::set_cdr`].
    #[inline]
    pub unsafe fn load_cdr(&self) -> TaggedValue {
        let p = &self.cdr_or_next as *const ConsCdrOrNext as *const AtomicUsize;
        TaggedValue(unsafe { (*p).load(Ordering::Acquire) })
    }

    /// Store `car` atomically so a concurrent GC read sees a whole value,
    /// never a torn word. Release-ordered: publishing a heap pointer into a
    /// GC-visible cell must happen-after the pointee's header/constructor
    /// writes (the publication-ordering contract on [`load_value_atomic`]).
    /// On x86-TSO both release stores and plain stores compile to `mov`.
    ///
    /// # Safety
    ///
    /// `self` must be a live cons cell and the supplied tagged value must obey
    /// the GC publication contract described above.
    #[inline]
    pub unsafe fn set_car(&mut self, value: TaggedValue) {
        let p = &self.car as *const TaggedValue as *const AtomicUsize;
        unsafe { (*p).store(value.0, Ordering::Release) };
    }

    /// Store the cdr using release ordering.
    ///
    /// # Safety
    ///
    /// `self` must be a live cons cell and `value` must be a valid tagged value
    /// whose pointee, if any, has been fully initialized before publication.
    #[inline]
    pub unsafe fn set_cdr(&mut self, value: TaggedValue) {
        let p = &self.cdr_or_next as *const ConsCdrOrNext as *const AtomicUsize;
        unsafe { (*p).store(value.0, Ordering::Release) };
    }

    /// Read the free-list link from a reclaimed cons cell.
    ///
    /// # Safety
    ///
    /// `self` must be a free cons cell, making `next_free` the active union
    /// field.
    #[inline]
    pub unsafe fn free_next(&self) -> *mut ConsCell {
        unsafe { self.cdr_or_next.next_free }
    }

    /// Convert a reclaimed cons cell into a free-list node.
    ///
    /// # Safety
    ///
    /// The cell must no longer be reachable as a live cons, and `next` must be
    /// null or point to another valid free-list node.
    #[inline]
    pub unsafe fn set_free_next(&mut self, next: *mut ConsCell) {
        // GNU `sweep_conses` (src/alloc.c:6856-6858) writes the free-list link
        // into the cdr union and then poisons the car with `dead_object ()`.
        // The poison is load-bearing, not decoration: the link is a raw
        // `*mut ConsCell` whose low three bits are `TAG_SYMBOL`, so a
        // use-after-free read of the cdr decodes as a symbol with a garbage id
        // and travels arbitrarily far before it faults. `nil` here would be a
        // perfectly ordinary Lisp value; `dead_object` is one no live object
        // can hold, so the car answers "this cell is on the free list" in O(1)
        // (GNU's `deadp`, src/alloc.c:425-429).
        self.car = TaggedValue::DEAD;
        self.cdr_or_next.next_free = next;
    }
}

// ---------------------------------------------------------------------------
// GcHeader — shared header for all non-cons heap objects
// ---------------------------------------------------------------------------

/// GC header prepended to every non-cons heap object.
///
/// Provides mark bit for garbage collection and an intrusive linked list
/// pointer for sweep-phase traversal.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
pub enum HeapObjectKind {
    String = 0,
    Float = 1,
    VecLike = 2,
}

#[repr(C)]
pub struct GcHeader {
    /// RAW mark-parity bit. For YOUNG (non-tenured, heap-owned) non-cons
    /// objects, "marked this cycle" ≡ `bit == TaggedHeap::mark_parity`; the
    /// heap flips its parity at `begin_collection` instead of walking
    /// `all_objects` to clear bits, so the raw value alone is meaningless —
    /// interpret it via `is_marked_at`/`mark_claim_at` with the owning heap's
    /// parity. Tenured objects freeze this bit at promotion and every reader
    /// short-circuits on `tenured` first; mapped (pdump) objects mark via the
    /// heap's side tables and never interpret this bit at all. Accessed via
    /// relaxed atomics so the concurrent GC thread can claim it while the
    /// mutator allocate-blacks / reads it without a data race; `AtomicBool`
    /// has the same size/layout as `bool`.
    pub marked: AtomicBool,
    /// Exact object category for typed sweep/deallocation.
    pub kind: HeapObjectKind,
    /// Tenured (old generation): a permanently-live heap object — the
    /// heap-reconstructed dump permanents (bytecode/hash-tables/closures) and,
    /// later, survival-promoted long-lived objects. When the dump partition is
    /// active, tenured objects are born black, never cleared/re-traced, and
    /// never swept; mutations of them are caught by the write barrier. Occupies
    /// padding between `kind` and `next`, so the header does not grow.
    pub tenured: bool,
    /// Intrusive linked list of all GC-managed objects (for sweep).
    pub next: *mut GcHeader,
}

impl GcHeader {
    pub fn new(kind: HeapObjectKind) -> Self {
        Self {
            // `false` pairs with `TaggedHeap::mark_parity` starting `false`:
            // objects created before the first collection read as unmarked
            // once the first `begin_collection` flips the parity to `true`
            // (see the parity invariant comment on `TaggedHeap::mark_parity`).
            // Heap allocation paths overwrite this at the link seams
            // (born-at-parity); this default is what mapped/static headers keep.
            marked: AtomicBool::new(false),
            kind,
            tenured: false,
            next: std::ptr::null_mut(),
        }
    }

    /// Read the RAW mark-parity bit (relaxed). Only meaningful compared
    /// against the owning heap's parity — use `is_marked_at` unless you are
    /// asserting the raw bit itself (e.g. "never touched").
    #[inline]
    pub fn is_marked(&self) -> bool {
        self.marked.load(Ordering::Relaxed)
    }

    /// Store the RAW mark-parity bit (relaxed). Call sites pass the owning
    /// heap's current parity to mark ("this cycle" black), which is also the
    /// correct born-at-parity value at the allocation link seams.
    #[inline]
    pub fn set_marked(&self, value: bool) {
        self.marked.store(value, Ordering::Relaxed);
    }

    /// Is this young object marked for the cycle whose parity is `parity`?
    #[inline]
    pub fn is_marked_at(&self, parity: bool) -> bool {
        self.marked.load(Ordering::Relaxed) == parity
    }

    /// Atomically CLAIM the mark bit for the cycle whose parity is `parity`:
    /// set it and return `true` iff this call is the one that flipped it from
    /// unmarked (`!= parity`) → marked (`== parity`). Used by the concurrent GC
    /// thread to mark a heap object exactly once with no `&mut TaggedHeap`, the
    /// non-cons analogue of `atomic_mark_owned_cons_ptr`. `swap` is atomic, so
    /// two threads racing to claim the same object cannot both observe `true`;
    /// a lost race is benign (the object ends up `== parity` either way).
    #[inline]
    pub fn mark_claim_at(&self, parity: bool) -> bool {
        self.marked.swap(parity, Ordering::Relaxed) != parity
    }
}

// ---------------------------------------------------------------------------
// Typed heap objects
// ---------------------------------------------------------------------------

/// Heap-allocated string object.
#[repr(C)]
pub struct StringObj {
    pub header: GcHeader,
    pub data: crate::heap_types::LispString,
}

/// Heap-allocated float object.
#[repr(C)]
pub struct FloatObj {
    pub header: GcHeader,
    pub value: f64,
}

// ---------------------------------------------------------------------------
// Vectorlike — catch-all for complex heap types
// ---------------------------------------------------------------------------

/// Sub-type tag for vectorlike objects.
/// Stored in the `VecLikeHeader`, distinguishes the many heap types
/// that share the GNU `Lisp_Vectorlike` pointer tag.
///
/// Discriminants mirror GNU's `enum pvec_type` for every runtime object that
/// has a GNU counterpart.  Neomacs-only transitional tags live after
/// `PVEC_FONT`; those are explicit compatibility debt, not GNU semantics.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
pub enum VecLikeType {
    Vector = 0,
    /// Arbitrary-precision integer (GNU `PVEC_BIGNUM`).
    Bignum = 2,
    Marker = 3,
    Overlay = 4,
    /// Finalizer object (GNU `PVEC_FINALIZER`).
    Finalizer = 5,
    /// Symbol with source position (GNU `PVEC_SYMBOL_WITH_POS`).
    SymbolWithPos = 6,
    /// User pointer for dynamic module API (GNU `PVEC_USER_PTR`).
    UserPtr = 8,
    /// Asynchronous subprocess / network / pipe / serial connection
    /// (GNU `PVEC_PROCESS`).
    Process = 9,
    Frame = 10,
    Window = 11,
    Buffer = 13,
    HashTable = 14,
    /// Obarray object (GNU `PVEC_OBARRAY`).
    Obarray = 15,
    /// Display terminal object (GNU `PVEC_TERMINAL`).
    Terminal = 16,
    /// Saved window configuration (GNU `PVEC_WINDOW_CONFIGURATION`). Stored with
    /// the same `{header, data}` layout as a record, but a distinct type tag so
    /// it is opaque to the vector/array/sequence predicates (as in GNU).
    WindowConfiguration = 17,
    /// Built-in function (like GNU's PVEC_SUBR).
    Subr = 18,
    /// Embedded widget model object (GNU `PVEC_XWIDGET`).
    Xwidget = 20,
    /// Embedded widget view object (GNU `PVEC_XWIDGET_VIEW`).
    XwidgetView = 21,
    /// Dynamic module function (GNU `PVEC_MODULE_FUNCTION`).
    ModuleFunction = 25,
    /// SQLite database or statement object (like GNU's PVEC_SQLITE).
    Sqlite = 30,
    /// Lisp closures are GNU `PVEC_CLOSURE`.
    Lambda = 31,
    /// Character table (like GNU's PVEC_CHAR_TABLE).
    CharTable = 32,
    /// Internal sub character table (like GNU's PVEC_SUB_CHAR_TABLE).
    SubCharTable = 33,
    Record = 34,
    /// Opened font object (GNU `PVEC_FONT`). Font specs and entities remain
    /// ordinary tagged vectors; only runtime-opened fonts use this opaque tag.
    Font = 35,
    Macro = 36,
    ByteCode = 37,
    Timer = 38,
    /// GC-managed shader-surface handle (NeoMacs-only; no GNU counterpart).
    /// Wraps a host-allocated surface id from `neomacs-surface-create`; the
    /// sweep queues the id for a best-effort
    /// `DisplayHost::destroy_shader_surface` when the handle dies.
    SurfaceHandle = 39,
}

impl VecLikeType {
    pub fn gnu_pvec_type(self) -> Option<GnuPvecType> {
        Some(match self {
            Self::Vector => GnuPvecType::NormalVector,
            Self::Bignum => GnuPvecType::Bignum,
            Self::Marker => GnuPvecType::Marker,
            Self::Overlay => GnuPvecType::Overlay,
            Self::Finalizer => GnuPvecType::Finalizer,
            Self::SymbolWithPos => GnuPvecType::SymbolWithPos,
            Self::UserPtr => GnuPvecType::UserPtr,
            Self::Process => GnuPvecType::Process,
            Self::Frame => GnuPvecType::Frame,
            Self::Window => GnuPvecType::Window,
            Self::Buffer => GnuPvecType::Buffer,
            Self::HashTable => GnuPvecType::HashTable,
            Self::Obarray => GnuPvecType::Obarray,
            Self::Terminal => GnuPvecType::Terminal,
            Self::WindowConfiguration => GnuPvecType::WindowConfiguration,
            Self::Subr => GnuPvecType::Subr,
            Self::Xwidget => GnuPvecType::Xwidget,
            Self::XwidgetView => GnuPvecType::XwidgetView,
            Self::ModuleFunction => GnuPvecType::ModuleFunction,
            Self::Sqlite => GnuPvecType::Sqlite,
            Self::Lambda => GnuPvecType::Closure,
            Self::CharTable => GnuPvecType::CharTable,
            Self::SubCharTable => GnuPvecType::SubCharTable,
            Self::Record => GnuPvecType::Record,
            Self::Font => GnuPvecType::Font,
            Self::Macro | Self::ByteCode | Self::Timer | Self::SurfaceHandle => return None,
        })
    }

    pub fn gnu_pvec_code(self) -> Option<u8> {
        self.gnu_pvec_type().map(GnuPvecType::gnu_code)
    }
}

/// Complete GNU `enum pvec_type` domain (`src/lisp.h`). This records all
/// public GNU pseudovector tag codes even when Neomacs does not yet allocate
/// a corresponding runtime object.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
pub enum GnuPvecType {
    NormalVector = 0,
    Free = 1,
    Bignum = 2,
    Marker = 3,
    Overlay = 4,
    Finalizer = 5,
    SymbolWithPos = 6,
    MiscPtr = 7,
    UserPtr = 8,
    Process = 9,
    Frame = 10,
    Window = 11,
    BoolVector = 12,
    Buffer = 13,
    HashTable = 14,
    Obarray = 15,
    Terminal = 16,
    WindowConfiguration = 17,
    Subr = 18,
    Other = 19,
    Xwidget = 20,
    XwidgetView = 21,
    Thread = 22,
    Mutex = 23,
    Condvar = 24,
    ModuleFunction = 25,
    NativeCompUnit = 26,
    TsParser = 27,
    TsNode = 28,
    TsCompiledQuery = 29,
    Sqlite = 30,
    Closure = 31,
    CharTable = 32,
    SubCharTable = 33,
    Record = 34,
    Font = 35,
}

impl GnuPvecType {
    pub fn from_gnu_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    pub fn gnu_code(self) -> u8 {
        self.into()
    }
}

use std::sync::OnceLock;

/// Slot storage for vectorlike objects that can either be ordinary Rust-owned
/// storage or a borrowed slice in a mapped pdump image.
pub struct LispValueVec {
    storage: LispValueVecStorage,
}

/// Byte storage that may alias the mapped dump image — the byte twin of
/// [`LispValueVec`]. Mapped storage is read-only; every consumer of the
/// wrapped bytes reads through `as_slice`.
pub struct LispByteVec {
    storage: LispByteVecStorage,
}

enum LispByteVecStorage {
    Owned(Vec<u8>),
    Mapped { ptr: *const u8, len: usize },
}

// Mapped bytes are read-only through shared references, same contract as
// `LispValueVecStorage`.
unsafe impl Send for LispByteVecStorage {}
unsafe impl Sync for LispByteVecStorage {}

impl LispByteVec {
    pub fn owned(bytes: Vec<u8>) -> Self {
        Self {
            storage: LispByteVecStorage::Owned(bytes),
        }
    }

    /// SAFETY: `ptr..ptr+len` must stay valid and immutable for the
    /// process lifetime (the mapped dump image satisfies this).
    pub unsafe fn mapped(ptr: *const u8, len: usize) -> Self {
        Self {
            storage: LispByteVecStorage::Mapped { ptr, len },
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        match &self.storage {
            LispByteVecStorage::Owned(bytes) => bytes,
            LispByteVecStorage::Mapped { ptr, len } => unsafe {
                std::slice::from_raw_parts(*ptr, *len)
            },
        }
    }

    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    /// Bytes owned on the Rust heap (0 for mapped storage) — GC size
    /// accounting.
    pub fn owned_bytes(&self) -> usize {
        match &self.storage {
            LispByteVecStorage::Owned(bytes) => bytes.capacity(),
            LispByteVecStorage::Mapped { .. } => 0,
        }
    }
}

impl std::ops::Deref for LispByteVec {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl Clone for LispByteVec {
    fn clone(&self) -> Self {
        match &self.storage {
            LispByteVecStorage::Owned(bytes) => Self::owned(bytes.clone()),
            LispByteVecStorage::Mapped { ptr, len } => Self {
                storage: LispByteVecStorage::Mapped {
                    ptr: *ptr,
                    len: *len,
                },
            },
        }
    }
}

impl std::fmt::Debug for LispByteVec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_slice().fmt(f)
    }
}

#[repr(transparent)]
pub struct LispValueSlice([TaggedValue]);

impl LispValueSlice {
    pub fn from_slice(slice: &[TaggedValue]) -> &Self {
        unsafe { &*(slice as *const [TaggedValue] as *const Self) }
    }

    pub fn as_slice(&self) -> &[TaggedValue] {
        &self.0
    }

    pub fn to_vec(&self) -> Vec<TaggedValue> {
        self.0.to_vec()
    }

    // This unsized slice view cannot implement `Clone`; the historical method
    // intentionally materializes an owned vector.
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> Vec<TaggedValue> {
        self.to_vec()
    }
}

impl std::ops::Deref for LispValueSlice {
    type Target = [TaggedValue];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl std::fmt::Debug for LispValueSlice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl PartialEq<Vec<TaggedValue>> for LispValueSlice {
    fn eq(&self, other: &Vec<TaggedValue>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl PartialEq<LispValueSlice> for Vec<TaggedValue> {
    fn eq(&self, other: &LispValueSlice) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<'a> IntoIterator for &'a LispValueSlice {
    type Item = &'a TaggedValue;
    type IntoIter = std::slice::Iter<'a, TaggedValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl<'a> IntoIterator for &'a &'a LispValueSlice {
    type Item = &'a TaggedValue;
    type IntoIter = std::slice::Iter<'a, TaggedValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

enum LispValueVecStorage {
    Owned(Vec<TaggedValue>),
    Mapped { ptr: *const TaggedValue, len: usize },
}

// Mapped slots are read-only through shared references.  Mutation paths use
// `ensure_owned` before exposing `&mut Vec<TaggedValue>`.
unsafe impl Send for LispValueVecStorage {}
unsafe impl Sync for LispValueVecStorage {}

impl std::fmt::Debug for LispValueVec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl LispValueVec {
    pub fn owned(items: Vec<TaggedValue>) -> Self {
        Self {
            storage: LispValueVecStorage::Owned(items),
        }
    }

    /// Build slot storage whose contents live in a mapped pdump image.
    ///
    /// # Safety
    /// `ptr..ptr+len` must remain mapped and immutable for the lifetime of the
    /// returned storage unless a mutation first copies the slots into owned
    /// storage.
    pub(crate) unsafe fn mapped(ptr: *const TaggedValue, len: usize) -> Self {
        Self {
            storage: LispValueVecStorage::Mapped { ptr, len },
        }
    }

    pub fn as_slice(&self) -> &[TaggedValue] {
        match self.storage {
            LispValueVecStorage::Owned(ref items) => items,
            LispValueVecStorage::Mapped { ptr, len } => {
                if len == 0 {
                    &[]
                } else {
                    unsafe { std::slice::from_raw_parts(ptr, len) }
                }
            }
        }
    }

    pub fn ensure_owned(&mut self) -> &mut Vec<TaggedValue> {
        if let LispValueVecStorage::Mapped { .. } = self.storage {
            let items = self.as_slice().to_vec();
            self.storage = LispValueVecStorage::Owned(items);
        }
        match self.storage {
            LispValueVecStorage::Owned(ref mut items) => items,
            LispValueVecStorage::Mapped { .. } => {
                unreachable!("mapped vector storage was copied to owned slots")
            }
        }
    }

    pub fn owned_capacity(&self) -> usize {
        match self.storage {
            LispValueVecStorage::Owned(ref items) => items.capacity(),
            LispValueVecStorage::Mapped { .. } => 0,
        }
    }

    /// Atomic (acquire) load of element `i`, for GC tracing that may run on a
    /// concurrent collector thread while the mutator stores via `store_atomic`
    /// (see the publication-ordering contract on [`load_value_atomic`]).
    /// Panics on out-of-bounds, like slice indexing.
    #[inline]
    pub fn load_atomic(&self, i: usize) -> TaggedValue {
        let p = &self.as_slice()[i] as *const TaggedValue as *const AtomicUsize;
        TaggedValue(unsafe { (*p).load(Ordering::Acquire) })
    }

    /// Atomic (release) store to element `i` of owned storage. The element must
    /// exist; this is for in-place slot writes (e.g. `aset`), not growth.
    /// Release publishes the stored pointee's header/constructor writes to the
    /// GC thread's acquire loads (contract on [`load_value_atomic`]).
    #[inline]
    pub fn store_atomic(&mut self, i: usize, value: TaggedValue) {
        let data = self.ensure_owned();
        let p = &data[i] as *const TaggedValue as *const AtomicUsize;
        unsafe { (*p).store(value.0, Ordering::Release) };
    }

    /// Iterate every element via atomic (acquire) loads — the GC tracing read
    /// path. Yields owned `TaggedValue`s (snapshots of each slot word).
    #[inline]
    pub fn iter_atomic(&self) -> impl Iterator<Item = TaggedValue> + '_ {
        self.as_slice().iter().map(|slot| {
            let p = slot as *const TaggedValue as *const AtomicUsize;
            TaggedValue(unsafe { (*p).load(Ordering::Acquire) })
        })
    }

    /// Stage 2 Tier B CONCURRENT VECTOR SCAN: capture this backing's base pointer,
    /// length, and storage kind for the start-of-cycle [`VectorScanSnapshot`]. The
    /// GC thread reads `[base, base+len)` via atomic loads (`load_value_atomic`).
    ///
    /// The base pointer is the backing's contiguous `TaggedValue` array — the `Vec`'s
    /// heap buffer for `Owned`, the immutable mapped span for `Mapped`. It stays
    /// valid for the whole cycle because: a `Mapped` backing addresses the immutable
    /// pdump image; an `Owned` backing is retired (kept alive to join) the instant a
    /// mutation would reallocate/replace it (clone-on-write in `with_vector_data_mut`).
    #[inline]
    pub(crate) fn scan_entry(&self) -> VectorScanEntry {
        match self.storage {
            LispValueVecStorage::Owned(ref items) => VectorScanEntry {
                base: items.as_ptr(),
                len: items.len(),
                is_mapped: false,
            },
            LispValueVecStorage::Mapped { ptr, len } => VectorScanEntry {
                base: ptr,
                len,
                is_mapped: true,
            },
        }
    }

    /// Stage 2 Tier B CONCURRENT VECTOR SCAN clone-on-write: whether this backing is
    /// currently `Owned`. Only `Owned` backings need cloning before a concurrent
    /// mutation (a `Mapped` backing reads the immutable dump; `ensure_owned` promotes
    /// it to a fresh `Owned` the GC's snapshot never points at, so no clone needed).
    #[inline]
    pub(crate) fn is_owned(&self) -> bool {
        matches!(self.storage, LispValueVecStorage::Owned(_))
    }

    /// Stage 2 Tier B CONCURRENT VECTOR SCAN clone-on-write: replace this `Owned`
    /// backing's `Vec` with a clone and return the ORIGINAL `Vec` so the caller can
    /// retire it (keep it alive + immutable until the GC thread joins). The GC's
    /// snapshot pointer still addresses the returned original; subsequent mutations
    /// touch the fresh clone. Must only be called when `is_owned()` is true.
    #[inline]
    pub(crate) fn clone_owned_backing(&mut self) -> Vec<TaggedValue> {
        match self.storage {
            LispValueVecStorage::Owned(ref mut items) => {
                let clone = items.clone();
                std::mem::replace(items, clone)
            }
            LispValueVecStorage::Mapped { .. } => {
                unreachable!("clone_owned_backing called on a mapped vector backing")
            }
        }
    }
}

/// One entry of a [`VectorScanSnapshot`]: a vector backing's contiguous slot array
/// captured at the world-stopped start handshake. `base`/`len` delimit the slots the
/// GC thread reads via [`load_value_atomic`]; `is_mapped` records whether the backing
/// was `Mapped` (immutable pdump span) or `Owned` (a Rust `Vec` buffer kept alive +
/// immutable for the cycle by the clone-on-write retire path). The scan reads both
/// kinds identically (both are contiguous `TaggedValue` arrays).
pub(crate) struct VectorScanEntry {
    pub(crate) base: *const TaggedValue,
    pub(crate) len: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) is_mapped: bool,
}

/// Start-of-cycle snapshot of every OWNED/Mapped vector backing for the Stage 2 Tier
/// B CONCURRENT VECTOR SCAN. Captured on the heap thread inside
/// `launch_concurrent_mark` (vectors are heap-side, unlike the Context-side obarray):
/// each entry holds a backing's base ptr + len + kind. The GC thread walks each
/// entry's `[0, len)` ONCE per cycle via atomic loads, feeding heap-object children
/// into the gray worklist (conses) / deferred list (non-cons) exactly like the
/// gray-drain cons branch. Mirrors [`ObarrayScanSnapshot`].
///
/// Pointer validity for the whole cycle: a `Mapped` entry addresses the immutable
/// pdump image; an `Owned` entry addresses a `Vec` buffer that the mutator's
/// clone-on-write hook (`with_vector_data_mut`) retires — keeping the original
/// pointer's buffer alive + immutable — on the owner's first bulk mutation this cycle,
/// before any realloc/replace. Vectors allocated mid-cycle are absent (allocate-black).
pub(crate) struct VectorScanSnapshot {
    entries: Vec<VectorScanEntry>,
}

// Safety: the snapshot holds raw base pointers into vector backings the heap keeps
// alive for the whole GC cycle (Mapped = immutable dump; Owned = retired-on-write,
// so the snapshot pointer always addresses a live, immutable buffer). The GC thread
// only READS through them via relaxed atomic loads, coordinated with the single
// mutator by the retire-before-replace clone-on-write hook, so handing the snapshot
// to the GC thread is sound.
unsafe impl Send for VectorScanSnapshot {}

impl VectorScanSnapshot {
    /// Build an empty snapshot; entries are pushed during the world-stopped capture.
    #[inline]
    pub(crate) fn with_capacity(cap: usize) -> Self {
        Self {
            entries: Vec::with_capacity(cap),
        }
    }

    /// Append one captured vector-backing entry.
    #[inline]
    pub(crate) fn push(&mut self, entry: VectorScanEntry) {
        self.entries.push(entry);
    }

    /// Number of captured vector backings (diagnostic).
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Scan every snapshotted vector backing ONCE, on the GC thread, reading each
    /// slot via an atomic load and invoking `push` for each heap-object child. The
    /// caller routes each pushed child to the gray worklist (conses) or the deferred
    /// list (non-cons), exactly like the gray-drain cons branch. Both Owned and
    /// Mapped entries are contiguous `TaggedValue` arrays, read identically.
    ///
    /// # Safety
    /// Must run on the GC thread for a snapshot captured at the world-stopped start
    /// handshake of the CURRENTLY-RUNNING concurrent mark; each entry's `base`/`len`
    /// must still address a live, immutable backing (guaranteed: Mapped = immutable
    /// dump; Owned = retired-before-replace by `with_vector_data_mut`).
    pub(crate) unsafe fn scan(&self, mut push: impl FnMut(TaggedValue)) {
        for entry in &self.entries {
            for i in 0..entry.len {
                // Safety: `base` addresses a contiguous `[TaggedValue; len]` backing
                // kept alive + immutable for the cycle; `i < len` is in bounds. The
                // read is a relaxed atomic load (pairs with the mutator's atomic slot
                // stores on the live, non-retired backing — never this retired one).
                let slot = unsafe { &*entry.base.add(i) };
                let child = load_value_atomic(slot);
                if child.is_heap_object() {
                    push(child);
                }
            }
        }
    }
}

/// Atomic (acquire) load of a single `TaggedValue` slot in place — for GC reads
/// of individual object fields (char-table default/parent/..., symbol-with-pos,
/// xwidget, overlay plist, module-function docs) and the concurrent obarray /
/// vector-backing scans.
///
/// PUBLICATION-ORDERING CONTRACT (concurrent GC, task #24 fix A): every store
/// that can make a heap pointer visible to the GC thread mid-mark (cons
/// car/cdr, vector slots, symbol value/function/plist cells — this module's
/// atomic store helpers) is `Release`, and every GC-thread load that can be
/// the first acquisition of such a pointer (cons car/cdr, slot/field loads —
/// this module's atomic load helpers) is `Acquire`. The pairing makes the
/// mutator's pre-publication writes — the arena `ptr::write` of the whole
/// `GcHeader`/constructor, including `tenured`, `kind`, and the born-black
/// parity bit — happen-before any dereference by the claim dispatcher
/// (`concurrent_try_mark_owned`), which may otherwise read a torn/premature
/// header on weakly-ordered targets (real UB on ARM; x86-TSO retired the
/// writes in order anyway). Both orderings compile to plain `mov` on x86-64,
/// and to `stlr`/`ldar` on AArch64. Objects reached through pre-cycle
/// channels (start-handshake snapshots, the gray-queue channel handoff, the
/// SATB/deferred mutexes) already carry happens-before from those seams; this
/// contract closes the one remaining path — a fresh pointer stored into an
/// already-snapshotted cell and read back mid-cycle. The mark bits themselves
/// (`GcHeader.marked`, cons block bitmaps) stay `Relaxed`: they never
/// publish field data, and a claim's field reads are ordered by this
/// contract's pointer chain, not by the bit.
#[inline]
pub fn load_value_atomic(slot: &TaggedValue) -> TaggedValue {
    let p = slot as *const TaggedValue as *const AtomicUsize;
    TaggedValue(unsafe { (*p).load(Ordering::Acquire) })
}

/// Atomic (release) store to a single `TaggedValue` slot in place. See the
/// publication-ordering contract on [`load_value_atomic`].
#[inline]
pub fn store_value_atomic(slot: &mut TaggedValue, value: TaggedValue) {
    let p = slot as *const TaggedValue as *const AtomicUsize;
    unsafe { (*p).store(value.0, Ordering::Release) };
}

impl From<Vec<TaggedValue>> for LispValueVec {
    fn from(value: Vec<TaggedValue>) -> Self {
        Self::owned(value)
    }
}

impl Clone for LispValueVec {
    fn clone(&self) -> Self {
        Self::owned(self.as_slice().to_vec())
    }
}

impl std::ops::Deref for LispValueVec {
    type Target = [TaggedValue];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl std::ops::DerefMut for LispValueVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ensure_owned().as_mut_slice()
    }
}

impl<'a> IntoIterator for &'a LispValueVec {
    type Item = &'a TaggedValue;
    type IntoIter = std::slice::Iter<'a, TaggedValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

/// Header for all vectorlike heap objects.
///
/// Extends `GcHeader` with a type tag. The type-specific data follows
/// this header in memory (accessed via pointer cast to the concrete type).
#[repr(C)]
pub struct VecLikeHeader {
    pub gc: GcHeader,
    pub type_tag: VecLikeType,
}

impl VecLikeHeader {
    pub fn new(type_tag: VecLikeType) -> Self {
        Self {
            gc: GcHeader::new(HeapObjectKind::VecLike),
            type_tag,
        }
    }
}

// -- Concrete vectorlike types --

/// Heap-allocated vector (dynamic array of Values).
#[repr(C)]
pub struct VectorObj {
    pub header: VecLikeHeader,
    pub data: LispValueVec,
}

/// Number of slots in GNU's top-level char-table contents vector.
pub const CHAR_TABLE_TOP_SLOTS: usize = 64;

/// Heap-allocated character table.
///
/// Mirrors GNU Emacs's `struct Lisp_Char_Table`: default, parent, purpose,
/// ASCII cache, 64 top-level contents slots, then extra slots.
#[repr(C)]
pub struct CharTableObj {
    pub header: VecLikeHeader,
    pub defalt: TaggedValue,
    pub parent: TaggedValue,
    pub purpose: TaggedValue,
    pub ascii: TaggedValue,
    pub contents: [TaggedValue; CHAR_TABLE_TOP_SLOTS],
    pub extras: LispValueVec,
}

/// Heap-allocated sub character table.
///
/// Mirrors GNU Emacs's `struct Lisp_Sub_Char_Table`: depth, minimum
/// character, and a depth-dependent contents vector.
#[repr(C)]
pub struct SubCharTableObj {
    pub header: VecLikeHeader,
    pub depth: i32,
    pub min_char: i32,
    pub contents: LispValueVec,
}

/// Heap-allocated display terminal object.
///
/// GNU exposes terminals as `PVEC_TERMINAL` pseudovectors. Neomacs stores the
/// mutable terminal state in `TerminalManager`; the Lisp object carries only the
/// stable terminal id and the GNU-compatible vec-like tag.
#[repr(C)]
pub struct TerminalObj {
    pub header: VecLikeHeader,
    pub id: u64,
}

/// Heap-allocated hash table.
#[repr(C)]
pub struct HashTableObj {
    pub header: VecLikeHeader,
    pub table: crate::emacs_core::value::LispHashTable,
}

/// Heap-allocated obarray.
///
/// Mirrors GNU Emacs's `struct Lisp_Obarray`: a vectorlike object with
/// bucket storage and a symbol count.  Legacy vector obarrays are still
/// accepted by `check_obarray` compatibility code.
#[repr(C)]
pub struct ObarrayObj {
    pub header: VecLikeHeader,
    pub buckets: LispValueVec,
    pub count: u32,
}

/// Heap-allocated lambda (interpreted closure).
///
/// Matches GNU Emacs's PVEC_CLOSURE: a plain vector of Lisp_Object slots.
/// The GC traces ALL slots uniformly — no type-specific tracing needed.
///
/// Slot layout (GNU Emacs compatible):
///   [0] CLOSURE_ARGLIST    — parameter list (e.g., (x y &optional z))
///   [1] CLOSURE_CODE       — body forms as Lisp list (interpreted) or bytecode
///   [2] CLOSURE_CONSTANTS  — lexical environment (interpreted) or constants vector
///   [3] CLOSURE_STACK_DEPTH — nil for interpreted, fixnum for bytecode
///   [4] CLOSURE_DOC_STRING — docstring or doc-form
///   [5] CLOSURE_INTERACTIVE — interactive spec
///   [6..] extra slots for oclosures
#[repr(C)]
pub struct LambdaObj {
    pub header: VecLikeHeader,
    /// All closure data as GC-managed Value slots.
    pub data: LispValueVec,
    /// Parsed lambda params cached from slot 0 for fast calls/arity checks.
    pub parsed_params: OnceLock<crate::emacs_core::value::LambdaParams>,
}

/// Closure slot indices matching GNU Emacs (lisp.h).
pub const CLOSURE_ARGLIST: usize = 0;
pub const CLOSURE_CODE: usize = 1;
pub const CLOSURE_CONSTANTS: usize = 2;
pub const CLOSURE_STACK_DEPTH: usize = 3;
pub const CLOSURE_DOC_STRING: usize = 4;
pub const CLOSURE_INTERACTIVE: usize = 5;
/// Minimum number of slots in a closure vector.
pub const CLOSURE_MIN_SLOTS: usize = 6;

/// Heap-allocated macro — same layout as Lambda but with VecLikeType::Macro.
#[repr(C)]
pub struct MacroObj {
    pub header: VecLikeHeader,
    pub data: LispValueVec,
    /// Parsed lambda params cached from slot 0 for fast calls/arity checks.
    pub parsed_params: OnceLock<crate::emacs_core::value::LambdaParams>,
}

/// Heap-allocated bytecode function.
#[repr(C)]
pub struct ByteCodeObj {
    pub header: VecLikeHeader,
    pub data: crate::emacs_core::bytecode::ByteCodeFunction,
}

/// Heap-allocated record (like vector with a type tag in slot 0).
#[repr(C)]
pub struct RecordObj {
    pub header: VecLikeHeader,
    pub data: LispValueVec,
}

/// Native metrics stored by an opened `PVEC_FONT` object.
///
/// These are named fields rather than private Lisp-vector positions because
/// GNU's `struct font` owns them as native state and `query-font` reads that
/// state directly.  Keeping the representation typed prevents unrelated Lisp
/// vector layout changes from corrupting font metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontObjectMetrics {
    pub pixel_size: i64,
    pub height: i64,
    pub max_width: i64,
    pub ascent: i64,
    pub descent: i64,
    pub space_width: i64,
    pub average_width: i64,
}

/// Native payload of an opened font object.
#[derive(Clone, Debug)]
pub struct FontObjectData {
    /// Lisp-visible font properties (`:font-object`, keyword/value pairs).
    pub fields: LispValueVec,
    pub metrics: FontObjectMetrics,
    pub capability: TaggedValue,
    /// Exact backend identity retained from realization.  This is deliberately
    /// not reduced to file/index: native selectors and variation coordinates
    /// are required to reopen the same font instance.
    pub identity: neomacs_display_protocol::font::ResolvedFontIdentity,
}

/// Heap-allocated opened-font pseudovector (`PVEC_FONT`).
#[repr(C)]
pub struct FontObj {
    pub header: VecLikeHeader,
    pub data: FontObjectData,
}

/// Heap-allocated overlay.
#[repr(C)]
pub struct OverlayObj {
    pub header: VecLikeHeader,
    pub data: crate::heap_types::OverlayData,
}

/// Heap-allocated marker.
#[repr(C)]
pub struct MarkerObj {
    pub header: VecLikeHeader,
    pub data: crate::heap_types::LispMarker,
}

/// Heap-allocated buffer reference (wraps a BufferId).
#[repr(C)]
pub struct BufferObj {
    pub header: VecLikeHeader,
    pub id: crate::buffer::BufferId,
}

/// Heap-allocated window reference (wraps a u64 id).
#[repr(C)]
pub struct WindowObj {
    pub header: VecLikeHeader,
    pub id: u64,
}

/// Heap-allocated process reference (wraps a ProcessId).
///
/// Mirrors `WindowObj`/`FrameObj`/`TimerObj`: the heap object holds only a
/// numeric manager key, no Lisp Values, so GC tracing is a no-op.
#[repr(C)]
pub struct ProcessObj {
    pub header: VecLikeHeader,
    pub id: crate::emacs_core::process::ProcessId,
}

/// Heap-allocated frame reference (wraps a u64 id).
#[repr(C)]
pub struct FrameObj {
    pub header: VecLikeHeader,
    pub id: u64,
}

/// Heap-allocated timer reference (wraps a u64 id).
#[repr(C)]
pub struct TimerObj {
    pub header: VecLikeHeader,
    pub id: u64,
}

/// Heap-allocated shader-surface handle (wraps a host surface id).
///
/// NeoMacs-only pseudovector returned by `neomacs-surface-create`. Unlike
/// xwidgets (kept alive by `internal_xwidget_list`), surface handles are NOT
/// registry-rooted: when Lisp drops the last reference, the GC sweep pushes
/// `surface_id` onto the heap's pending-destroy list, which the evaluator
/// drains after the collection completes by queuing
/// `DisplayHost::destroy_shader_surface` (best-effort). No Lisp children.
#[repr(C)]
pub struct SurfaceObj {
    pub header: VecLikeHeader,
    pub surface_id: u32,
}

/// Heap-allocated xwidget model object.
///
/// Mirrors GNU `struct xwidget`: Lisp-traced fields come first
/// (`plist`, `type`, `buffer`, `title`, `script_callbacks`), followed by
/// native geometry/lifetime fields.
#[repr(C)]
pub struct XwidgetObj {
    pub header: VecLikeHeader,
    pub plist: TaggedValue,
    pub type_: TaggedValue,
    pub buffer: TaggedValue,
    pub title: TaggedValue,
    pub script_callbacks: TaggedValue,
    pub height: i32,
    pub width: i32,
    pub xwidget_id: u32,
    /// Browser identity explicitly associated with this GNU xwidget model.
    pub webview_id: WebViewId,
    /// GNU stores `kill_without_query`; query-on-exit returns nil when this is
    /// true and t otherwise.
    pub kill_without_query: bool,
}

/// Heap-allocated xwidget view object.
///
/// GNU's public view object keeps the model and window as Lisp references.
/// Native window-system payload is owned by frontend/backends, not by this VM
/// object.
#[repr(C)]
pub struct XwidgetViewObj {
    pub header: VecLikeHeader,
    pub model: TaggedValue,
    pub window: TaggedValue,
    pub x: i32,
    pub y: i32,
    pub clip_right: i32,
    pub clip_bottom: i32,
    pub clip_top: i32,
    pub clip_left: i32,
    pub redisplayed: bool,
    pub hidden: bool,
}

/// Heap-allocated built-in function (like GNU's PVEC_SUBR).
/// Contains a GNU-shaped fixed-arity or variadic entry point together with
/// arity metadata stored on the SubrObj itself.
pub type SubrFnMany = fn(
    &mut crate::emacs_core::eval::Context,
    Vec<super::value::TaggedValue>,
) -> crate::emacs_core::error::EvalResult;
/// Variadic native subroutine that does not need evaluator state.
///
/// Keeping this shape distinct avoids erasing the implementation's actual
/// dependency behind an adapter closure that accepts and discards `Context`.
pub type SubrFnManyNoContext =
    fn(Vec<super::value::TaggedValue>) -> crate::emacs_core::error::EvalResult;
pub type SubrFnManySlice = fn(
    &mut crate::emacs_core::eval::Context,
    &[super::value::TaggedValue],
) -> crate::emacs_core::error::EvalResult;
pub type SubrFn0 =
    fn(&mut crate::emacs_core::eval::Context) -> crate::emacs_core::error::EvalResult;
pub type SubrFn1 = fn(
    &mut crate::emacs_core::eval::Context,
    super::value::TaggedValue,
) -> crate::emacs_core::error::EvalResult;
pub type SubrFn2 = fn(
    &mut crate::emacs_core::eval::Context,
    super::value::TaggedValue,
    super::value::TaggedValue,
) -> crate::emacs_core::error::EvalResult;
pub type SubrFn3 = fn(
    &mut crate::emacs_core::eval::Context,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
) -> crate::emacs_core::error::EvalResult;
pub type SubrFn4 = fn(
    &mut crate::emacs_core::eval::Context,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
) -> crate::emacs_core::error::EvalResult;
pub type SubrFn5 = fn(
    &mut crate::emacs_core::eval::Context,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
) -> crate::emacs_core::error::EvalResult;
pub type SubrFn6 = fn(
    &mut crate::emacs_core::eval::Context,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
) -> crate::emacs_core::error::EvalResult;
pub type SubrFn7 = fn(
    &mut crate::emacs_core::eval::Context,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
) -> crate::emacs_core::error::EvalResult;
pub type SubrFn8 = fn(
    &mut crate::emacs_core::eval::Context,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
    super::value::TaggedValue,
) -> crate::emacs_core::error::EvalResult;

#[derive(Clone, Copy)]
pub enum SubrFn {
    Many(SubrFnMany),
    ManyNoContext(SubrFnManyNoContext),
    ManySlice(SubrFnManySlice),
    A0(SubrFn0),
    A1(SubrFn1),
    A2(SubrFn2),
    A3(SubrFn3),
    A4(SubrFn4),
    A5(SubrFn5),
    A6(SubrFn6),
    A7(SubrFn7),
    A8(SubrFn8),
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum SubrDispatchKind {
    Builtin,
    ContextCallable,
    SpecialForm,
}

/// Whether a primitive function has a GNU `Lisp_Subr::intspec`.
///
/// This is intrinsic function-object metadata, not a property of the symbol
/// whose function cell happens to reference the object.  Keeping the state as
/// a closed enum makes a newly constructed or refreshed [`SubrObj`] choose its
/// command classification explicitly.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubrInteractivity {
    NonInteractive,
    Interactive,
}

impl SubrInteractivity {
    #[inline(always)]
    pub fn is_interactive(self) -> bool {
        matches!(self, Self::Interactive)
    }
}

impl From<bool> for SubrInteractivity {
    fn from(interactive: bool) -> Self {
        if interactive {
            Self::Interactive
        } else {
            Self::NonInteractive
        }
    }
}

#[repr(C)]
pub struct SubrObj {
    pub header: VecLikeHeader,
    /// The canonical symbol identity for this primitive function.
    pub sym_id: crate::emacs_core::intern::SymId,
    /// The runtime-local name atom for the subr's public name.
    pub name: crate::emacs_core::intern::NameId,
    /// Minimum number of arguments.
    pub min_args: u16,
    /// Maximum number of arguments (None = unlimited/&rest).
    pub max_args: Option<u16>,
    /// How the evaluator should dispatch this public subr surface.
    pub dispatch_kind: SubrDispatchKind,
    /// GNU `Lisp_Subr::intspec.string` presence, used directly by `commandp`.
    pub interactivity: SubrInteractivity,
    /// Native Rust entry point for the builtin, if fully registered.
    pub function: Option<SubrFn>,
}

/// Heap-allocated arbitrary-precision integer (mirrors GNU
/// `struct Lisp_Bignum` in `src/bignum.h`).
///
/// GNU stores an `mpz_t` directly inside the struct. NeoMacs wraps
/// `malachite::Integer`, a pure-Rust bignum derived from GMP/FLINT
/// algorithms. The GC has no Lisp_Object children to trace — the only
/// owned resource is the `Integer`'s internal limb buffer, which is
/// freed when `Drop` runs in `free_gc_object`.
#[repr(C)]
pub struct BignumObj {
    pub header: VecLikeHeader,
    pub value: Integer,
}

/// A symbol annotated with its source byte offset.
/// Mirrors GNU `struct Lisp_Symbol_With_Pos` (`lisp.h:958`).
/// Both fields are `TaggedValue` (GC-traced), matching GNU's LISPSIZE=2.
#[repr(C)]
pub struct SymbolWithPosObj {
    pub header: VecLikeHeader,
    /// The bare symbol. Must always be a plain symbol (TAG_SYMBOL).
    pub sym: TaggedValue,
    /// Source byte offset. Must always be a fixnum.
    pub pos: TaggedValue,
}

/// Heap-allocated finalizer object.
///
/// Mirrors GNU `struct Lisp_Finalizer` (`lisp.h`): one traced Lisp slot, the
/// zero-argument `function` to run once the finalizer object itself becomes
/// unreachable. GNU's intrusive prev/next registration list is replaced by
/// the heap-side `finalizer_registry`; mark termination scans it, queues the
/// functions of unmarked finalizers, and re-marks them so they survive the
/// sweep until they have run (errors ignored).
#[repr(C)]
pub struct FinalizerObj {
    pub header: VecLikeHeader,
    /// Called with zero args after the GC cycle that finds this object
    /// unreachable. Immutable after allocation.
    pub function: TaggedValue,
}

/// Heap-allocated SQLite database or statement object.
///
/// The native SQLite resources are owned by the sqlite module's runtime maps;
/// this object is the opaque Lisp identity and carries the handle key plus the
/// database/statement discriminator, mirroring GNU's single PVEC_SQLITE tag.
#[repr(C)]
pub struct SqliteObj {
    pub header: VecLikeHeader,
    pub is_statement: bool,
    pub id: i64,
}

/// Heap-allocated user pointer for dynamic module API.
///
/// Mirrors GNU `struct Lisp_User_Ptr` (`emacs-module.c`).
/// Carries a raw C `void *` pointer plus an optional finalizer.
/// The GC never traces the raw pointer — it calls the finalizer on sweep.
///
/// The finalizer function pointer signature follows GNU Emacs:
/// `void (*fin)(void *ptr)`.
pub type EmacsFinalizer = Option<unsafe extern "C" fn(*mut std::ffi::c_void)>;

#[repr(C)]
pub struct UserPtrObj {
    pub header: VecLikeHeader,
    /// The raw C pointer owned by the module.
    pub ptr: *mut std::ffi::c_void,
    /// Optional finalizer invoked when the user-ptr is garbage-collected.
    pub finalizer: EmacsFinalizer,
}

/// Heap-allocated module function for dynamic module API.
///
/// Mirrors GNU `struct Lisp_Module_Function` (`emacs-module.c`).
/// Stores the C function pointer, closure data, optional finalizer,
/// arity metadata, and Lisp-visible doc/interactive slots.
#[repr(C)]
pub struct ModuleFunctionObj {
    pub header: VecLikeHeader,
    /// Minimum number of required arguments.
    pub min_arity: isize,
    /// Maximum number of arguments (-2 = GNU `emacs_variadic_function`).
    pub max_arity: isize,
    /// The raw C function pointer (emacs_function from emacs-module.h).
    ///
    /// Signature: `emacs_value (*)(emacs_env *env, ptrdiff_t nargs,
    ///                              emacs_value *args, void *data)`.
    pub subr: *const std::ffi::c_void,
    /// User-supplied closure data pointer.
    pub data: *mut std::ffi::c_void,
    /// Optional finalizer invoked when the module-function is GC'd.
    pub finalizer: EmacsFinalizer,
    /// Docstring (Lisp string value).
    pub documentation: TaggedValue,
    /// Interactive form (Lisp value).
    pub interactive_form: TaggedValue,
}

#[cfg(test)]
mod tests {
    use super::{LispValueSlice, LispValueVec};
    use crate::tagged::value::TaggedValue;

    #[test]
    fn mapped_lisp_value_vec_borrows_until_mutation() {
        let slots = vec![TaggedValue::fixnum(1), TaggedValue::fixnum(2)];
        let mut values = unsafe { LispValueVec::mapped(slots.as_ptr(), slots.len()) };

        assert_eq!(values.as_slice(), slots.as_slice());
        values.ensure_owned().push(TaggedValue::fixnum(3));

        drop(slots);
        assert_eq!(
            values.as_slice(),
            &[
                TaggedValue::fixnum(1),
                TaggedValue::fixnum(2),
                TaggedValue::fixnum(3)
            ]
        );
    }

    #[test]
    fn lisp_value_slice_clone_returns_owned_vec_for_compat_callers() {
        let slots = vec![TaggedValue::fixnum(1), TaggedValue::fixnum(2)];
        let slice = LispValueSlice::from_slice(&slots);

        let owned = slice.clone();
        drop(slots);
        assert_eq!(owned, vec![TaggedValue::fixnum(1), TaggedValue::fixnum(2)]);
    }
}
