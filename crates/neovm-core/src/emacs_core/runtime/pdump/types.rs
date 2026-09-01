//! Snapshot types for portable dump (pdump) serialization.
//!
//! These are rkyv-serializable mirrors of the runtime types in the evaluator.
//! Each `Dump*` type maps 1:1 to a runtime type but uses only plain data
//! (no Rc, HashMap, raw pointers, thread-locals).

use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};

use crate::buffer::BufferTextBackendKind;
use crate::heap_types::LispString;

// ---------------------------------------------------------------------------
// Primitive identifiers
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DumpHeapRef {
    pub index: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DumpSymId(pub u32);

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DumpNameId(pub u32);

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DumpBufferId(pub u64);

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DumpByteSpan {
    pub offset: u64,
    pub len: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DumpSlotSpan {
    pub offset: u64,
    pub len: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DumpConsSpan {
    pub offset: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DumpFloatSpan {
    pub offset: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DumpStringSpan {
    pub offset: u64,
    pub len: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DumpVecLikeSpan {
    pub offset: u64,
    pub len: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum DumpByteData {
    Owned(Vec<u8>),
    Mapped(DumpByteSpan),
    StaticRoData { key: u64, len: u64 },
}

impl DumpByteData {
    pub fn owned(data: Vec<u8>) -> Self {
        Self::Owned(data)
    }

    pub fn mapped(offset: u64, len: u64) -> Self {
        Self::Mapped(DumpByteSpan { offset, len })
    }

    pub fn static_rodata(key: u64, len: u64) -> Self {
        Self::StaticRoData { key, len }
    }

    pub fn as_owned_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Owned(data) => Some(data),
            Self::Mapped(_) | Self::StaticRoData { .. } => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Value
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub enum DumpValue {
    #[default]
    Nil,
    True,
    Int(i64),
    Float(DumpHeapRef),
    Symbol(DumpSymId),
    Str(DumpHeapRef),
    Cons(DumpHeapRef),
    Vector(DumpHeapRef),
    CharTable(DumpHeapRef),
    SubCharTable(DumpHeapRef),
    Record(DumpHeapRef),
    HashTable(DumpHeapRef),
    Obarray(DumpHeapRef),
    Lambda(DumpHeapRef),
    Macro(DumpHeapRef),
    Subr(DumpNameId),
    ByteCode(DumpHeapRef),
    Marker(DumpHeapRef),
    Overlay(DumpHeapRef),
    Buffer(DumpBufferId),
    Window(u64),
    Frame(u64),
    Timer(u64),
    /// Bignum serialized as a base-10 decimal string. We don't share
    /// bignums via heap refs because they're immutable and the dump
    /// format only needs to recreate the value, not its identity.
    Bignum(String),
    /// The `Qunbound` sentinel. Reaches the dump path only via a
    /// `local_var_alist` entry whose cdr marks a void per-buffer
    /// binding (mirrors GNU storing `(sym . Qunbound)` for
    /// `make-local-variable` on a void symbol, `data.c:2285-2289`).
    Unbound,
}

// ---------------------------------------------------------------------------
// Heap objects
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpHeapObject {
    Cons {
        car: DumpValue,
        cdr: DumpValue,
    },
    Vector(Vec<DumpValue>),
    CharTable {
        defalt: DumpValue,
        parent: DumpValue,
        purpose: DumpValue,
        ascii: DumpValue,
        contents: Vec<DumpValue>,
        extras: Vec<DumpValue>,
    },
    SubCharTable {
        depth: i64,
        min_char: i64,
        contents: Vec<DumpValue>,
    },
    HashTable(DumpLispHashTable),
    Obarray {
        buckets: Vec<DumpValue>,
        count: u32,
    },
    Str {
        data: DumpByteData,
        size: usize,
        size_byte: i64,
        #[serde(default)]
        text_props: Vec<DumpStringTextPropertyRun>,
    },
    Float(f64),
    Lambda(Vec<DumpValue>),
    Macro(Vec<DumpValue>),
    ByteCode(DumpByteCodeFunction),
    Record(Vec<DumpValue>),
    Marker(DumpMarker),
    Overlay(DumpOverlay),
    Buffer(DumpBufferId),
    Window(u64),
    Frame(u64),
    Timer(u64),
    Subr {
        name: DumpNameId,
        min_args: u16,
        max_args: Option<u16>,
    },
    Free,
}

// ---------------------------------------------------------------------------
// Lambda / ByteCode
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpLambdaParams {
    pub required: Vec<DumpSymId>,
    pub optional: Vec<DumpSymId>,
    pub rest: Option<DumpSymId>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpByteCodeFunction {
    /// Executable instruction source for this function.
    ///
    /// GNU bytecode is kept as its original byte stream and decoded according
    /// to the runtime's eager/lazy policy. Native NeoVM bytecode has no such
    /// byte stream, so it carries its decoded instruction sequence instead.
    /// Keeping these alternatives in an enum prevents a dump from serializing
    /// both the source bytes and redundant derived instructions.
    pub instructions: DumpByteCodeInstructions,
    pub constants: Vec<DumpValue>,
    pub max_stack: u16,
    pub params: DumpLambdaParams,
    #[serde(default)]
    pub arglist: Option<DumpValue>,
    #[serde(default)]
    pub lexical: bool,
    pub env: Option<DumpValue>,
    pub docstring: Option<DumpLispString>,
    pub doc_form: Option<DumpValue>,
    #[serde(default)]
    pub interactive: Option<DumpValue>,
    #[serde(default)]
    pub closure_slot_count: usize,
    #[serde(default)]
    pub extra_slots: Vec<DumpValue>,
    /// Whether decoded instructions are seal_ops-normalized (see
    /// `ByteCodeFunction::ops_sealed`). Meaningful only for
    /// [`DumpByteCodeInstructions::Decoded`]; GNU byte streams re-decode and
    /// re-seal on load.
    #[serde(default)]
    pub ops_sealed: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpByteCodeInstructions {
    /// Native NeoVM instructions that have no GNU byte-string source.
    Decoded(Vec<crate::emacs_core::bytecode::opcode::Op>),
    /// Original GNU bytecode. Decoded instructions and byte-offset maps are
    /// derived from this stream and therefore do not belong in the dump.
    Gnu(DumpByteData),
}

// ---------------------------------------------------------------------------
// Bytecode opcodes
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Hash tables
// ---------------------------------------------------------------------------

#[repr(u8)]
#[derive(Serialize, Deserialize, Debug, Clone, Copy, IntoPrimitive, TryFromPrimitive)]
pub enum DumpHashTableTest {
    Eq = 0,
    Eql = 1,
    Equal = 2,
}

#[repr(u8)]
#[derive(Serialize, Deserialize, Debug, Clone, Copy, IntoPrimitive, TryFromPrimitive)]
pub enum DumpHashTableWeakness {
    Key = 0,
    Value = 1,
    KeyOrValue = 2,
    KeyAndValue = 3,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpHashKey {
    Nil,
    True,
    Int(i64),
    /// Bignum key: canonical two's-complement little-endian limbs.
    Bignum(Vec<u64>),
    Float(u64),
    FloatEq(u64, u32),
    Symbol(DumpSymId),
    Keyword(DumpSymId),
    Str(DumpHeapRef),
    Char(char),
    Window(u64),
    Frame(u64),
    Ptr(u64),
    HeapRef(u32),
    EqualCons(Box<DumpHashKey>, Box<DumpHashKey>),
    EqualVec(Vec<DumpHashKey>),
    Marker(Option<u64>, usize),
    Overlay {
        buffer: Option<u64>,
        start: usize,
        end: usize,
        plist: Box<DumpHashKey>,
    },
    BoolVec {
        len: u32,
        bits: u128,
    },
    SymbolWithPos(Box<DumpHashKey>, Box<DumpHashKey>),
    Cycle(u32),
    Text(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpLispHashTable {
    pub test: DumpHashTableTest,
    pub test_name: Option<DumpSymId>,
    pub size: i64,
    pub weakness: Option<DumpHashTableWeakness>,
    pub rehash_size: f64,
    pub rehash_threshold: f64,
    /// Entries in INSERTION ORDER, each carrying its key-snapshot Value
    /// (`None` when the snapshot is just the entry value — the common case).
    /// One ordered list replaces the old entries/key_snapshots/
    /// insertion_order triple: the loader used to build two temporary maps
    /// and re-join them per table (4-5 hash operations per entry); ordered
    /// entries load with ONE insert per entry.
    pub ordered_entries: Vec<(DumpHashKey, DumpValue, Option<DumpValue>)>,
}

// ---------------------------------------------------------------------------
// Symbols / Obarray
// ---------------------------------------------------------------------------

/// Serialized value cell for a symbol.  Replaces the old
/// `DumpSymbolValue` (which wrapped `Option<DumpValue>` for the plain case
/// and had separate legacy `value`/`special`/`constant` fields).
///
/// Added in pdump format v21 alongside the removal of the `SymbolValue`
/// enum from `LispSymbol`.  The variant tag directly mirrors the
/// `SymbolRedirect` discriminant stored in `SymbolFlags`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpSymbolVal {
    /// `SymbolRedirect::Plainval` — value is in `val.plain`.
    /// `DumpValue::Unbound` encodes the unbound sentinel.
    Plain(DumpValue),
    /// `SymbolRedirect::Varalias` — value cell aliases another symbol.
    Alias(DumpSymId),
    /// `SymbolRedirect::Localized` — buffer-local variable with a BLV.
    /// `default` is the global default value (the `defcell` cdr).
    /// `local_if_set` mirrors `LispBufferLocalValue::local_if_set`.
    /// `forwarder` mirrors `LispBufferLocalValue::fwd` — see
    /// [`DumpLocalizedForwarder`].
    Localized {
        default: DumpValue,
        local_if_set: bool,
        forwarder: Option<DumpLocalizedForwarder>,
    },
    /// `SymbolRedirect::Forwarded` — forwarded to a Rust-side variable.
    /// These are re-installed from `BUFFER_SLOT_INFO` at load time, so
    /// the dump only needs to signal "this symbol is a forwarder"; the
    /// actual descriptor pointer is never serialized.
    Forwarded,
    /// `SymbolRedirect::Forwarded` backed by GNU `Lisp_Boolfwd` semantics.
    /// The stable descriptor pointer is rebuilt on load; only its current
    /// native Boolean value belongs in the portable image.
    BoolForwarded(bool),
    /// `SymbolRedirect::Forwarded` backed by GNU `Lisp_Intfwd` semantics.
    /// As with `BoolForwarded`, only the slot's current integer is portable;
    /// the descriptor is rebuilt on load.
    IntForwarded(DumpValue),
    /// `SymbolRedirect::Forwarded` backed by GNU `Lisp_Objfwd` semantics --
    /// every `DEFVAR_LISP` name.  GNU's dumper writes the forwarding pointer
    /// itself plus the `Lisp_Object` it names (`src/pdumper.c:2461-2462`,
    /// `dump_fwd_obj`); here the descriptor is a process-lifetime pointer, so
    /// the image carries the value and the descriptor is rebuilt, exactly as
    /// for `BoolForwarded` and `IntForwarded`.
    ObjForwarded(DumpValue),
    /// `SymbolRedirect::Forwarded` backed by GNU `Lisp_Kboard_Objfwd`
    /// semantics -- every `DEFVAR_KBOARD` name.
    KboardForwarded(DumpValue),
}

/// The forward types a `Localized` symbol's BLV can be carrying.
///
/// GNU's `make_blv` copies the symbol's forwarder into the BLV
/// (`src/data.c:2112-2140`), so a `DEFVAR_BOOL` or `DEFVAR_INT` variable that
/// Lisp then made buffer-local -- `indent-tabs-mode`,
/// `display-line-numbers-offset` -- keeps its coercion and its type check per
/// buffer.  GNU needs nothing in its dump for that, because its slots are C
/// statics the dumper relocates; here the descriptor is a process-lifetime
/// pointer that cannot travel, so the image has to say which kind to rebuild.
///
/// Only the two forward types whose storage IS the descriptor appear:
/// `Lisp_Fwd_Obj` / `Lisp_Fwd_Buffer_Obj` / `Lisp_Fwd_Kboard_Obj` keep their
/// value elsewhere, which is the same distinction `LispFwd::clone_stateful`
/// makes.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DumpLocalizedForwarder {
    /// GNU `Lisp_Fwd_Bool` — `*XBOOLVAR (valcontents) = !NILP (newval)`.
    Bool,
    /// GNU `Lisp_Fwd_Int` — `CHECK_INTEGER` then `integer_to_intmax`.
    Int,
    /// GNU `Lisp_Fwd_Obj` — stores anything.  Carries no store rule at all;
    /// what it carries is the symbol's redirect tag, which is what
    /// `set_internal` consults to refuse an unbind through the BLV
    /// (`src/data.c:1723-1727`).
    Obj,
    /// GNU `Lisp_Fwd_Kboard_Obj`.  `Fmake_local_variable` refuses to produce a
    /// BLV for one of these (`src/data.c:2286-2288`), so a dump can only reach
    /// this arm from an image older than that refusal.
    Kboard,
}

/// Serialized per-symbol metadata.  Format v21: all legacy fields
/// (`name`, `value`, `symbol_value`, `special`, `constant`) are removed;
/// the value cell is encoded directly as a `DumpSymbolVal` variant, and
/// the flag byte fields (`redirect`, `trapped_write`, `interned`,
/// `declared_special`) mirror `SymbolFlags` exactly.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpSymbolData {
    /// Redirect tag: 0=Plainval, 1=Varalias, 2=Localized, 3=Forwarded.
    /// Redundant with `val`'s variant tag but kept for clarity and to
    /// allow future validation on load.
    pub redirect: u8,
    /// Trapped-write tag: 0=Untrapped, 1=NoWrite, 2=Trapped.
    pub trapped_write: u8,
    /// Interned tag: 0=Uninterned, 1=Interned, 2=InternedInInitial.
    pub interned: u8,
    /// `declared_special` flag (mirrors `SymbolFlags::declared_special`).
    pub declared_special: bool,
    /// The value cell, encoded as a `DumpSymbolVal` variant.
    pub val: DumpSymbolVal,
    /// Function slot. `DumpValue::Nil` is the unbound sentinel.
    pub function: DumpValue,
    /// Property list as a Lisp cons list (DumpValue::Nil = empty).
    pub plist: DumpValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpObarray {
    /// Residual symbols that need per-symbol reconstruction (Localized and
    /// Forwarded redirects - runtime BLV/forwarder pointers cannot be baked).
    /// Plain and Varalias symbols ride `plain_rows` instead.
    pub symbols: Vec<(DumpSymId, DumpSymbolData)>,
    pub global_members: Vec<DumpSymId>,
    pub function_unbound: Vec<DumpSymId>,
    pub function_epoch: u64,
    /// (heap-image offset, count) of the fixed 32-byte symbol-row region:
    /// {sym u32, redirect u8, trapped u8, interned u8, special u8,
    /// val u64, function u64, plist u64}. The three value words are written
    /// through `write_dump_value_word`, so the relocation/fixup machinery
    /// patches them to live runtime Values before `load_obarray` reads the
    /// rows back - no per-symbol DumpValue decode at load.
    pub plain_rows: Option<(u64, u64)>,
}

// ---------------------------------------------------------------------------
// Dump-wide symbol table
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpSymbolEntry {
    /// Dump-local name atom id for this symbol slot.
    pub name: DumpNameId,
    /// `true` when the corresponding symbol id is canonical/interned and
    /// `false` for uninterned symbols created via `make-symbol`.
    pub canonical: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpSymbolTable {
    /// Dump-local symbol-name atoms. Multiple symbols may point at the same
    /// `DumpNameId` when they share a print name.
    pub names: Vec<LispString>,
    /// One entry per dump-local symbol id.
    pub symbols: Vec<DumpSymbolEntry>,
}

// ---------------------------------------------------------------------------
// Tagged heap snapshot
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpTaggedHeap {
    pub objects: Vec<DumpHeapObject>,
    #[serde(default)]
    pub mapped_cons: Vec<Option<DumpConsSpan>>,
    #[serde(default)]
    pub mapped_floats: Vec<Option<DumpFloatSpan>>,
    #[serde(default)]
    pub mapped_strings: Vec<Option<DumpStringSpan>>,
    #[serde(default)]
    pub mapped_veclikes: Vec<Option<DumpVecLikeSpan>>,
    #[serde(default)]
    pub mapped_slots: Vec<Option<DumpSlotSpan>>,
}

// ---------------------------------------------------------------------------
// OrderedSymMap (dynamic binding frame)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpRuntimeBindingValue {
    Bound(DumpValue),
    Void,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpOrderedSymMap {
    pub entries: Vec<(DumpSymId, DumpRuntimeBindingValue)>,
}

// ---------------------------------------------------------------------------
// String text properties
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpStringTextPropertyRun {
    pub start: usize,
    pub end: usize,
    pub plist: DumpValue,
}

// ---------------------------------------------------------------------------
// Buffer types
// ---------------------------------------------------------------------------

#[repr(u8)]
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    IntoPrimitive,
    TryFromPrimitive,
)]
pub enum DumpBufferTextBackendKind {
    #[default]
    GapBuffer = 0,
    PieceTree = 1,
    Rope = 2,
}

impl From<BufferTextBackendKind> for DumpBufferTextBackendKind {
    fn from(kind: BufferTextBackendKind) -> Self {
        Self::try_from(u8::from(kind))
            .expect("implemented buffer text backend must have a pdump tag")
    }
}

impl From<DumpBufferTextBackendKind> for BufferTextBackendKind {
    fn from(kind: DumpBufferTextBackendKind) -> Self {
        Self::try_from(u8::from(kind))
            .expect("pdump buffer text backend tag must be implemented at runtime")
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpBufferText {
    pub backend_kind: DumpBufferTextBackendKind,
    pub text: Vec<u8>,
}

// `DumpInsertionType` was retired in v26 alongside `DumpMarkerEntry`:
// the marker chain now serializes through `DumpMarker`, which encodes
// the insertion type as a plain `bool` matching `LispMarker`.

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpPropertyInterval {
    pub start: usize,
    pub end: usize,
    pub properties: Vec<(DumpValue, DumpValue)>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpTextPropertyTable {
    pub intervals: Vec<DumpPropertyInterval>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpOverlay {
    pub serial: u64,
    pub plist: DumpValue,
    pub buffer: Option<DumpBufferId>,
    pub start: usize,
    pub end: usize,
    pub front_advance: bool,
    pub rear_advance: bool,
}

/// Pdump v26: marker shape mirrors `LispMarker` post-GNU-parity refactor.
///
/// The legacy `position: Option<i64>` cache is gone — `bytepos` and `charpos`
/// are the authoritative on-disk fields, matching the runtime `LispMarker`
/// layout. Used both for individual heap-object marker decode
/// (`DumpHeapObject::Marker`) and for `DumpBuffer.markers` chain entries.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpMarker {
    pub buffer: Option<DumpBufferId>,
    pub insertion_type: bool,
    pub marker_id: Option<u64>,
    pub bytepos: usize,
    pub charpos: usize,
    /// Mirror of `LispMarker.last_position_valid`. Defaulted for back-compat
    /// with pre-parity dumps; older dumps come back as `false` and a single
    /// re-set will repopulate the flag.
    #[serde(default)]
    pub last_position_valid: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpOverlayList {
    pub overlays: Vec<DumpOverlay>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpUndoRecord {
    Insert {
        pos: usize,
        len: usize,
    },
    Delete {
        pos: usize,
        text: String,
    },
    PropertyChange {
        pos: usize,
        len: usize,
        old_props: Vec<(String, DumpValue)>,
    },
    CursorMove {
        pos: usize,
    },
    FirstChange {
        visited_file_modtime: i64,
    },
    Boundary,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpUndoList {
    pub records: Vec<DumpUndoRecord>,
    pub limit: usize,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpBuffer {
    pub id: DumpBufferId,
    #[serde(default)]
    pub name_lisp: Option<DumpLispString>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub last_name_lisp: Option<DumpLispString>,
    #[serde(default)]
    pub last_name: Option<String>,
    pub base_buffer: Option<DumpBufferId>,
    pub text: DumpBufferText,
    pub pt: usize,
    #[serde(default)]
    pub pt_char: Option<usize>,
    pub mark: Option<usize>,
    #[serde(default)]
    pub mark_char: Option<usize>,
    pub begv: usize,
    #[serde(default)]
    pub begv_char: Option<usize>,
    pub zv: usize,
    #[serde(default)]
    pub zv_char: Option<usize>,
    pub modified: bool,
    pub modified_tick: i64,
    pub chars_modified_tick: i64,
    #[serde(default)]
    pub save_modified_tick: Option<i64>,
    #[serde(default)]
    pub autosave_modified_tick: Option<i64>,
    #[serde(default)]
    pub modtime_sec: Option<i64>,
    #[serde(default)]
    pub modtime_nsec: Option<i32>,
    #[serde(default)]
    pub modtime_size: Option<i64>,
    /// GNU `last_window_start` serialized as a Lisp-visible one-based
    /// character position. Runtime code uses `LispCharPos1`; pdump keeps the
    /// numeric payload for format compatibility.
    #[serde(default)]
    pub last_window_start: Option<usize>,
    pub read_only: bool,
    pub multibyte: bool,
    #[serde(default)]
    pub file_name_lisp: Option<DumpLispString>,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub auto_save_file_name_lisp: Option<DumpLispString>,
    #[serde(default)]
    pub auto_save_file_name: Option<String>,
    /// v26: chain order, head→tail. Each entry is a full `DumpMarker`
    /// (the same shape used by `DumpHeapObject::Marker`); the load-side
    /// chain reconstruction reuses the heap-allocated MarkerObj for the
    /// same `marker_id` to preserve identity with Lisp references.
    pub markers: Vec<DumpMarker>,
    #[serde(default)]
    pub state_pt_marker: Option<u64>,
    #[serde(default)]
    pub state_begv_marker: Option<u64>,
    #[serde(default)]
    pub state_zv_marker: Option<u64>,
    #[serde(default)]
    pub properties_syms: Vec<(DumpSymId, DumpRuntimeBindingValue)>,
    pub properties: Vec<(String, DumpRuntimeBindingValue)>,
    #[serde(default)]
    pub local_binding_syms: Vec<DumpSymId>,
    #[serde(default)]
    pub local_binding_names: Vec<String>,
    #[serde(default)]
    pub local_map: DumpValue,
    pub text_props: DumpTextPropertyTable,
    pub overlays: DumpOverlayList,
    /// Legacy field — retained for backward compatibility with old pdump files.
    /// New dumps always write an empty DumpUndoList here; the real undo state
    /// lives inside the `properties` map as `buffer-undo-list`.
    #[serde(default)]
    pub undo_list: Option<DumpUndoList>,
    /// Phase 11: BUFFER_OBJFWD slot table values. One DumpValue per
    /// `Buffer::slots[]` entry, in offset order. Empty for legacy
    /// (pre-format-11) dumps; load_buffer falls back to seeding
    /// from BUFFER_SLOT_INFO defaults + the legacy file_name etc.
    /// fields when this is empty.
    #[serde(default)]
    pub slots: Vec<DumpValue>,
    /// Phase 11: per-slot "is buffer-local in this buffer" bitmap.
    /// Mirrors `Buffer::local_flags` (Phase 10D). Defaults to 0
    /// for legacy dumps.
    #[serde(default)]
    pub local_flags: u64,
    /// Phase 11: `local_var_alist` for SYMBOL_LOCALIZED variables.
    /// Defaults to `Nil` for legacy dumps.
    #[serde(default)]
    pub local_var_alist: DumpValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpBufferManager {
    pub buffers: Vec<(DumpBufferId, DumpBuffer)>,
    /// GNU's `Vbuffer_alist` order. `buffer-list` with no frame argument is
    /// just this global order (`buffer.c:Fbuffer_list`), and pdump preserves
    /// it as ordinary Lisp state. Keep the explicit order here so pdump-load
    /// does not reconstruct it from allocation ids.
    #[serde(default)]
    pub buffer_order: Vec<DumpBufferId>,
    pub current: Option<DumpBufferId>,
    pub next_id: u64,
    pub next_marker_id: u64,
    /// Runtime `buffer-defaults` slot table — one DumpValue per
    /// `BufferManager::buffer_defaults[]` entry in offset order.
    /// Mirrors GNU's static `struct buffer buffer_defaults` that
    /// pdump preserves as part of the dumped image.
    ///
    /// Bindings.el's `setq-default mode-line-format <rich-list>` at
    /// load time mutates this table; before this field was added to
    /// the dump schema, the mutation was lost on pdump-load and the
    /// layout engine saw the install-time `"%-"` seed. See
    /// `project_modeline_buffer_defaults_dump.md`.
    ///
    /// `#[serde(default)]` so older pdumps (no field present) keep
    /// loading and fall back to the install-time seeds via
    /// `BUFFER_SLOT_INFO` in `load_buffer_manager`.
    #[serde(default)]
    pub buffer_defaults: Vec<DumpValue>,
    /// Neomacs text backend selector for future buffers. GNU always creates
    /// ordinary buffers with gap-buffer text; this extension keeps that as the
    /// initial value and only changes future buffer creation when explicitly
    /// set at runtime.
    pub default_text_backend_kind: DumpBufferTextBackendKind,
}

// ---------------------------------------------------------------------------
// Sub-manager types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpLispString {
    pub data: Vec<u8>,
    pub size: usize,
    pub size_byte: i64,
}

// Autoload
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpAutoloadType {
    Function,
    Macro,
    Keymap,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpAutoloadEntry {
    pub file: DumpLispString,
    pub docstring: Option<DumpLispString>,
    pub interactive: bool,
    pub autoload_type: DumpAutoloadType,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpAutoloadManager {
    #[serde(default)]
    pub entries_syms: Vec<(DumpSymId, DumpAutoloadEntry)>,
    pub entries: Vec<(String, DumpAutoloadEntry)>,
    #[serde(default)]
    pub after_load_lisp: Vec<(DumpLispString, Vec<DumpValue>)>,
    #[serde(default)]
    pub after_load: Vec<(String, Vec<DumpValue>)>,
    pub loaded_files: Vec<DumpLispString>,
    #[serde(default)]
    pub obsolete_functions_syms: Vec<(DumpSymId, (DumpLispString, DumpLispString))>,
    pub obsolete_functions: Vec<(String, (String, String))>,
    #[serde(default)]
    pub obsolete_variables_syms: Vec<(DumpSymId, (DumpLispString, DumpLispString))>,
    pub obsolete_variables: Vec<(String, (String, String))>,
}

// Custom
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpCustomManager {
    #[serde(default)]
    pub auto_buffer_local_syms: Vec<DumpSymId>,
    #[serde(default)]
    pub auto_buffer_local: Vec<String>,
}

// Abbrev
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpAbbrev {
    pub expansion: DumpLispString,
    pub hook: Option<DumpLispString>,
    pub count: usize,
    pub system: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpAbbrevTable {
    pub name: DumpLispString,
    pub abbrevs: Vec<(DumpLispString, DumpAbbrev)>,
    pub parent: Option<DumpLispString>,
    pub case_fixed: bool,
    pub enable_quoting: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpAbbrevManager {
    #[serde(default)]
    pub tables_syms: Vec<(DumpSymId, DumpAbbrevTable)>,
    #[serde(default)]
    pub tables: Vec<(String, DumpAbbrevTable)>,
    #[serde(default)]
    pub global_table_sym: Option<DumpSymId>,
    pub global_table_name: DumpLispString,
    pub abbrev_mode: bool,
}

// Interactive
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpInteractiveSpec {
    pub spec: DumpValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpInteractiveRegistry {
    pub specs: Vec<(DumpSymId, DumpInteractiveSpec)>,
}

// Mode
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpFontLockKeyword {
    #[serde(default)]
    pub pattern_lisp: Option<DumpLispString>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub face_sym: Option<DumpSymId>,
    #[serde(default)]
    pub face: Option<String>,
    pub group: usize,
    pub override_: bool,
    pub laxmatch: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpFontLockDefaults {
    pub keywords: Vec<DumpFontLockKeyword>,
    pub case_fold: bool,
    #[serde(default)]
    pub syntax_table_lisp: Option<DumpLispString>,
    #[serde(default)]
    pub syntax_table: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpMajorMode {
    pub pretty_name: DumpLispString,
    pub parent: Option<DumpValue>,
    pub mode_hook: DumpValue,
    pub keymap_name: Option<DumpValue>,
    pub syntax_table_name: Option<DumpValue>,
    pub abbrev_table_name: Option<DumpValue>,
    pub font_lock: Option<DumpFontLockDefaults>,
    pub body: Option<DumpValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpMinorMode {
    pub lighter: Option<DumpLispString>,
    pub keymap_name: Option<DumpValue>,
    pub global: bool,
    pub body: Option<DumpValue>,
}

// mode.rs has its own CustomVariable/CustomGroup — we mirror those separately
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpModeCustomVariable {
    pub default_value: DumpValue,
    pub doc: Option<DumpLispString>,
    pub custom_type: DumpModeCustomType,
    pub group: Option<DumpValue>,
    pub set_function: Option<DumpValue>,
    pub get_function: Option<DumpValue>,
    pub tag: Option<DumpLispString>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpModeCustomType {
    Boolean,
    Integer,
    Float,
    String,
    Symbol,
    Sexp,
    Choice(Vec<(String, DumpValue)>),
    List(Box<DumpModeCustomType>),
    Alist(Box<DumpModeCustomType>, Box<DumpModeCustomType>),
    Plist(Box<DumpModeCustomType>, Box<DumpModeCustomType>),
    Color,
    Face,
    File,
    Directory,
    Function,
    Variable,
    Hook,
    Coding,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpModeCustomGroup {
    pub doc: Option<DumpLispString>,
    pub parent: Option<DumpValue>,
    pub members: Vec<DumpValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpModeRegistry {
    pub major_modes: Vec<(DumpSymId, DumpMajorMode)>,
    pub minor_modes: Vec<(DumpSymId, DumpMinorMode)>,
    pub buffer_major_modes: Vec<(u64, DumpValue)>,
    pub buffer_minor_modes: Vec<(u64, Vec<DumpValue>)>,
    pub global_minor_modes: Vec<DumpValue>,
    #[serde(default)]
    pub auto_mode_alist_lisp: Vec<(DumpLispString, DumpValue)>,
    #[serde(default)]
    pub auto_mode_alist: Vec<(String, DumpValue)>,
    pub custom_variables: Vec<(DumpSymId, DumpModeCustomVariable)>,
    pub custom_groups: Vec<(DumpSymId, DumpModeCustomGroup)>,
    pub fundamental_mode: DumpValue,
}

// Coding
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpEolType {
    Unix,
    Dos,
    Mac,
    Undecided,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpCodingSystemInfo {
    #[serde(default)]
    pub name_sym: Option<DumpSymId>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub coding_type_sym: Option<DumpSymId>,
    #[serde(default)]
    pub coding_type: Option<String>,
    pub mnemonic: char,
    pub eol_type: DumpEolType,
    pub ascii_compatible_p: bool,
    #[serde(default)]
    pub charset_list_syms: Vec<DumpSymId>,
    #[serde(default)]
    pub charset_list: Vec<String>,
    #[serde(default)]
    pub post_read_conversion_sym: Option<DumpSymId>,
    #[serde(default)]
    pub post_read_conversion: Option<String>,
    #[serde(default)]
    pub pre_write_conversion_sym: Option<DumpSymId>,
    #[serde(default)]
    pub pre_write_conversion: Option<String>,
    pub default_char: Option<char>,
    pub for_unibyte: bool,
    #[serde(default)]
    pub properties_syms: Vec<(DumpSymId, DumpValue)>,
    #[serde(default)]
    pub properties: Vec<(String, DumpValue)>,
    pub int_properties: Vec<(i64, DumpValue)>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpCodingSystemManager {
    #[serde(default)]
    pub systems_syms: Vec<(DumpSymId, DumpCodingSystemInfo)>,
    #[serde(default)]
    pub systems: Vec<(String, DumpCodingSystemInfo)>,
    #[serde(default)]
    pub aliases_syms: Vec<(DumpSymId, DumpSymId)>,
    #[serde(default)]
    pub aliases: Vec<(String, String)>,
    #[serde(default)]
    pub alias_order_syms: Vec<(DumpSymId, Vec<DumpSymId>)>,
    #[serde(default)]
    pub alias_order: Vec<(String, Vec<String>)>,
    #[serde(default)]
    pub priority_syms: Vec<DumpSymId>,
    #[serde(default)]
    pub priority: Vec<String>,
    #[serde(default)]
    pub keyboard_coding_sym: Option<DumpSymId>,
    #[serde(default)]
    pub keyboard_coding: Option<String>,
    #[serde(default)]
    pub terminal_coding_sym: Option<DumpSymId>,
    #[serde(default)]
    pub terminal_coding: Option<String>,
}

// Charset
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpCharsetSubsetSpec {
    #[serde(default)]
    pub parent_sym: Option<DumpSymId>,
    #[serde(default)]
    pub parent: Option<String>,
    pub parent_min_code: i64,
    pub parent_max_code: i64,
    pub offset: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpCharsetMethod {
    Offset(i64),
    Map(String),
    Subset(DumpCharsetSubsetSpec),
    SupersetSyms(Vec<(DumpSymId, i64)>),
    Superset(Vec<(String, i64)>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpCharsetInfo {
    pub id: i64,
    #[serde(default)]
    pub name_sym: Option<DumpSymId>,
    #[serde(default)]
    pub name: Option<String>,
    pub dimension: i64,
    pub code_space: [i64; 8],
    pub min_code: i64,
    pub max_code: i64,
    pub iso_final_char: Option<i64>,
    pub iso_revision: Option<i64>,
    pub emacs_mule_id: Option<i64>,
    pub ascii_compatible_p: bool,
    pub supplementary_p: bool,
    #[serde(default)]
    pub unified_p: bool,
    pub invalid_code: Option<i64>,
    pub unify_map: DumpValue,
    pub method: DumpCharsetMethod,
    #[serde(default)]
    pub plist_syms: Vec<(DumpSymId, DumpValue)>,
    #[serde(default)]
    pub plist: Vec<(String, DumpValue)>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpCharsetRegistry {
    pub charsets: Vec<DumpCharsetInfo>,
    #[serde(default)]
    pub priority_syms: Vec<DumpSymId>,
    #[serde(default)]
    pub priority: Vec<String>,
    pub next_id: i64,
}

#[repr(u8)]
#[derive(Serialize, Deserialize, Debug, Clone, Copy, IntoPrimitive, TryFromPrimitive)]
pub enum DumpFontWidth {
    UltraCondensed = 0,
    ExtraCondensed = 1,
    Condensed = 2,
    SemiCondensed = 3,
    Normal = 4,
    SemiExpanded = 5,
    Expanded = 6,
    ExtraExpanded = 7,
    UltraExpanded = 8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpFontRepertory {
    Charset(String),
    CharTableRanges(Vec<(u32, u32)>),
    CharsetSym(DumpSymId),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpStoredFontSpec {
    #[serde(default)]
    pub family_sym: Option<DumpSymId>,
    pub family: Option<String>,
    #[serde(default)]
    pub registry_sym: Option<DumpSymId>,
    pub registry: Option<String>,
    #[serde(default)]
    pub lang_sym: Option<DumpSymId>,
    pub lang: Option<String>,
    pub weight: Option<u16>,
    pub slant: Option<DumpFontSlant>,
    pub width: Option<DumpFontWidth>,
    pub repertory: Option<DumpFontRepertory>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpFontSpecEntry {
    Font(DumpStoredFontSpec),
    ExplicitNone,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpFontsetRangeEntry {
    pub from: u32,
    pub to: u32,
    pub entries: Vec<DumpFontSpecEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpFontsetData {
    pub ranges: Vec<DumpFontsetRangeEntry>,
    pub fallback: Option<Vec<DumpFontSpecEntry>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpFontsetRegistry {
    #[serde(default)]
    pub ordered_names_lisp: Vec<DumpLispString>,
    #[serde(default)]
    pub alias_to_name_lisp: Vec<(DumpLispString, DumpLispString)>,
    #[serde(default)]
    pub fontsets_lisp: Vec<(DumpLispString, DumpFontsetData)>,
    #[serde(default)]
    pub ordered_names: Vec<String>,
    #[serde(default)]
    pub alias_to_name: Vec<(String, String)>,
    #[serde(default)]
    pub fontsets: Vec<(String, DumpFontsetData)>,
    pub generation: u64,
}

// Face
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct DumpColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[repr(u8)]
#[derive(Serialize, Deserialize, Debug, Clone, Copy, IntoPrimitive, TryFromPrimitive)]
pub enum DumpFontSlant {
    Normal = 0,
    Italic = 1,
    Oblique = 2,
    ReverseItalic = 3,
    ReverseOblique = 4,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum DumpUnderlineStyle {
    Line,
    Wave,
    Dot,
    Dash,
    DoubleLine,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpUnderline {
    pub style: DumpUnderlineStyle,
    pub color: Option<DumpColor>,
    pub position: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum DumpBoxStyle {
    Flat,
    Raised,
    Pressed,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpBoxBorder {
    pub color: Option<DumpColor>,
    pub width: i32,
    pub style: DumpBoxStyle,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpFaceHeight {
    Absolute(i32),
    Relative(f64),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpFace {
    pub foreground: Option<DumpColor>,
    pub background: Option<DumpColor>,
    #[serde(default)]
    pub family_value: Option<DumpValue>,
    pub family: Option<String>,
    #[serde(default)]
    pub foundry_value: Option<DumpValue>,
    pub foundry: Option<String>,
    pub height: Option<DumpFaceHeight>,
    pub weight: Option<u16>,
    pub slant: Option<DumpFontSlant>,
    #[serde(default)]
    pub underline_disabled: bool,
    pub underline: Option<DumpUnderline>,
    pub overline: Option<bool>,
    pub strike_through: Option<bool>,
    #[serde(default)]
    pub box_disabled: bool,
    pub box_border: Option<DumpBoxBorder>,
    pub inverse_video: Option<bool>,
    #[serde(default)]
    pub stipple_value: Option<DumpValue>,
    pub stipple: Option<String>,
    pub extend: Option<bool>,
    #[serde(default)]
    pub inherit_syms: Vec<DumpSymId>,
    #[serde(default)]
    pub inherit: Vec<String>,
    pub overstrike: bool,
    #[serde(default)]
    pub doc_value: Option<DumpValue>,
    pub doc: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpFaceTable {
    #[serde(default)]
    pub face_ids: Vec<(DumpSymId, DumpFace)>,
    #[serde(default)]
    pub faces: Vec<(String, DumpFace)>,
}

// Rectangle
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpRectangleState {
    pub killed: Vec<DumpLispString>,
}

// Kmacro
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpKmacroManager {
    pub current_macro: Vec<DumpValue>,
    pub last_macro: Option<Vec<DumpValue>>,
    pub macro_ring: Vec<Vec<DumpValue>>,
    pub counter: i64,
    #[serde(default)]
    pub counter_format_lisp: Option<DumpLispString>,
    #[serde(default)]
    pub counter_format: Option<String>,
}

// Register
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DumpRegisterContent {
    Text {
        data: Vec<u8>,
        size: usize,
        size_byte: i64,
    },
    Number(i64),
    Marker(DumpValue),
    Rectangle(Vec<DumpLispString>),
    FrameConfig(DumpValue),
    File(DumpLispString),
    KbdMacro(Vec<DumpValue>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpRegisterManager {
    pub registers: Vec<(char, DumpRegisterContent)>,
}

// Bookmark
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpBookmark {
    pub name: DumpLispString,
    pub filename: Option<DumpLispString>,
    pub position: usize,
    pub front_context: Option<DumpLispString>,
    pub rear_context: Option<DumpLispString>,
    pub annotation: Option<DumpLispString>,
    pub handler: Option<DumpLispString>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpBookmarkManager {
    #[serde(default)]
    pub bookmarks_lisp: Vec<(DumpLispString, DumpBookmark)>,
    #[serde(default)]
    pub bookmarks: Vec<(String, DumpBookmark)>,
    pub recent: Vec<DumpLispString>,
}

// Variable watchers
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpVariableWatcherList {
    pub watchers: Vec<(DumpSymId, Vec<DumpValue>)>,
}

// ---------------------------------------------------------------------------
// Top-level evaluator state
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpContextState {
    pub symbol_table: DumpSymbolTable,
    pub tagged_heap: DumpTaggedHeap,
    pub obarray: DumpObarray,
    pub dynamic: Vec<DumpOrderedSymMap>,
    pub lexenv: DumpValue,
    pub features: Vec<DumpSymId>,
    pub require_stack: Vec<DumpSymId>,
    pub loads_in_progress: Vec<DumpLispString>,
    pub buffers: DumpBufferManager,
    pub autoloads: DumpAutoloadManager,
    pub custom: DumpCustomManager,
    pub modes: DumpModeRegistry,
    pub coding_systems: DumpCodingSystemManager,
    pub charset_registry: DumpCharsetRegistry,
    pub fontset_registry: DumpFontsetRegistry,
    pub face_table: DumpFaceTable,
    pub abbrevs: DumpAbbrevManager,
    pub interactive: DumpInteractiveRegistry,
    pub rectangle: DumpRectangleState,
    pub standard_syntax_table: DumpValue,
    pub syntax_code_objects: DumpValue,
    pub standard_category_table: DumpValue,
    pub current_local_map: DumpValue,
    pub current_global_map: DumpValue,
    pub kmacro: DumpKmacroManager,
    pub registers: DumpRegisterManager,
    pub bookmarks: DumpBookmarkManager,
    pub watchers: DumpVariableWatcherList,
}
