//! Obarray and symbol interning.
//!
//! In Emacs, symbols are unique objects stored in an "obarray" (hash table).
//! Each symbol has:
//! - A name (string)
//! - A value cell (variable binding)
//! - A function cell (function binding)
//! - A property list (plist)
//! - A `special` flag (for dynamic binding in lexical scope)
//!
//! # Redirect machinery (GNU `Lisp_Symbol::redirect`)
//!
//! Mirrors GNU Emacs's `enum symbol_redirect` (`src/lisp.h:771-777`). Every
//! symbol has a [`SymbolRedirect`] tag that determines how its value cell is
//! interpreted:
//!
//! | Tag         | `val` payload                  | GNU equivalent      |
//! | ----------- | ------------------------------ | ------------------- |
//! | `Plainval`  | direct [`Value`] (or UNBOUND)  | `SYMBOL_PLAINVAL`   |
//! | `Varalias`  | aliased [`SymId`]              | `SYMBOL_VARALIAS`   |
//! | `Localized` | `*mut LispBufferLocalValue`    | `SYMBOL_LOCALIZED`  |
//! | `Forwarded` | `*const LispFwd`               | `SYMBOL_FORWARDED`  |
//!
//! Phase 1 of the symbol-redirect refactor (`drafts/symbol-redirect-plan.md`)
//! introduces the new shape but every existing symbol still routes through
//! `Plainval`. The `BufferLocal` and `Forwarded` paths still also live on
//! the legacy `SymbolValue` enum during the transition; Phases 4-8 cut them
//! over to the redirect dispatch and Phase 10 deletes the legacy enum.

use super::intern::{
    NameId, SymId, intern, intern_lisp_string, is_canonical_id, lookup_interned,
    lookup_interned_lisp_string, resolve_name, resolve_sym, resolve_sym_lisp_string,
    symbol_name_id,
};
use super::value::{Value, ValueKind, VecLikeType};
use crate::emacs_core::error::Flow;
use crate::gc_trace::GcTrace;
use crate::heap_types::LispString;
use crate::tagged::header::{load_value_atomic, store_value_atomic};
use num_enum::{IntoPrimitive, TryFromPrimitive};

// ===========================================================================
// Redirect machinery — mirrors GNU `lisp.h:771-829`
// ===========================================================================

/// Two-bit `redirect` tag. Mirrors GNU `enum symbol_redirect`
/// (`src/lisp.h:771-777`). Discriminant for [`SymbolVal`].
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, IntoPrimitive, TryFromPrimitive)]
pub enum SymbolRedirect {
    /// Value is in `val.plain`. GNU `SYMBOL_PLAINVAL`.
    #[default]
    Plainval = 0,
    /// Value is really in another symbol. GNU `SYMBOL_VARALIAS`.
    Varalias = 1,
    /// Value is in a buffer-local cache. GNU `SYMBOL_LOCALIZED`.
    Localized = 2,
    /// Value is in a static C-side variable. GNU `SYMBOL_FORWARDED`.
    Forwarded = 3,
}

impl SymbolRedirect {
    pub fn from_gnu_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    pub fn gnu_code(self) -> u8 {
        self.into()
    }
}

/// Two-bit `trapped_write` flag. Mirrors GNU `enum symbol_trapped_write`
/// (`src/lisp.h:780-785`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, IntoPrimitive, TryFromPrimitive)]
pub enum SymbolTrappedWrite {
    /// Normal symbol. GNU `SYMBOL_UNTRAPPED_WRITE`.
    #[default]
    Untrapped = 0,
    /// Constant — write attempts signal `setting-constant`. GNU `SYMBOL_NOWRITE`.
    NoWrite = 1,
    /// Variable watchers fire on every write. GNU `SYMBOL_TRAPPED_WRITE`.
    Trapped = 2,
}

impl SymbolTrappedWrite {
    pub fn from_gnu_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    pub fn gnu_code(self) -> u8 {
        self.into()
    }
}

/// Two-bit `interned` flag. Mirrors GNU `enum symbol_interned`
/// (`src/lisp.h:782-787`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, IntoPrimitive, TryFromPrimitive)]
pub enum SymbolInterned {
    /// Uninterned (e.g. `make-symbol`). GNU `SYMBOL_UNINTERNED`.
    #[default]
    Uninterned = 0,
    /// Interned in some obarray. GNU `SYMBOL_INTERNED`.
    Interned = 1,
    /// Interned in the *initial* obarray (the global one). GNU
    /// `SYMBOL_INTERNED_IN_INITIAL_OBARRAY`. Used for keywords.
    InternedInInitial = 2,
}

impl SymbolInterned {
    pub fn from_gnu_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    pub fn gnu_code(self) -> u8 {
        self.into()
    }
}

/// Packed flags byte for a [`LispSymbol`]. Mirrors the bit-packed first byte
/// of GNU `Lisp_Symbol::s` (`src/lisp.h:786-792`).
///
/// Bit layout:
/// ```text
///   bits 0..2 : SymbolRedirect
///   bits 2..4 : SymbolTrappedWrite
///   bits 4..6 : SymbolInterned
///   bit  6    : declared_special
///   bit  7    : reserved
/// ```
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default)]
pub struct SymbolFlags(u8);

impl SymbolFlags {
    const REDIRECT_MASK: u8 = 0b0000_0011;
    const TRAPPED_WRITE_SHIFT: u8 = 2;
    const TRAPPED_WRITE_MASK: u8 = 0b0000_1100;
    const INTERNED_SHIFT: u8 = 4;
    const INTERNED_MASK: u8 = 0b0011_0000;
    const DECLARED_SPECIAL_BIT: u8 = 0b0100_0000;

    #[inline(always)]
    pub fn redirect(self) -> SymbolRedirect {
        SymbolRedirect::try_from(self.0 & Self::REDIRECT_MASK)
            .expect("symbol redirect flag contains valid GNU symbol_redirect code")
    }

    #[inline]
    pub fn set_redirect(&mut self, r: SymbolRedirect) {
        self.store_byte((self.0 & !Self::REDIRECT_MASK) | r.gnu_code());
    }

    #[inline]
    pub fn trapped_write(self) -> SymbolTrappedWrite {
        let raw = (self.0 & Self::TRAPPED_WRITE_MASK) >> Self::TRAPPED_WRITE_SHIFT;
        SymbolTrappedWrite::try_from(raw)
            .expect("symbol trapped-write flag contains valid GNU symbol_trapped_write code")
    }

    #[inline]
    pub fn set_trapped_write(&mut self, t: SymbolTrappedWrite) {
        self.store_byte(
            (self.0 & !Self::TRAPPED_WRITE_MASK) | (t.gnu_code() << Self::TRAPPED_WRITE_SHIFT),
        );
    }

    #[inline]
    pub fn interned(self) -> SymbolInterned {
        let raw = (self.0 & Self::INTERNED_MASK) >> Self::INTERNED_SHIFT;
        SymbolInterned::try_from(raw)
            .expect("symbol interned flag contains valid GNU symbol_interned code")
    }

    #[inline]
    pub fn set_interned(&mut self, i: SymbolInterned) {
        self.store_byte((self.0 & !Self::INTERNED_MASK) | (i.gnu_code() << Self::INTERNED_SHIFT));
    }

    #[inline]
    pub fn declared_special(self) -> bool {
        self.0 & Self::DECLARED_SPECIAL_BIT != 0
    }

    #[inline]
    pub fn set_declared_special(&mut self, v: bool) {
        let byte = if v {
            self.0 | Self::DECLARED_SPECIAL_BIT
        } else {
            self.0 & !Self::DECLARED_SPECIAL_BIT
        };
        self.store_byte(byte);
    }

    /// Atomic (relaxed) store of the whole flags byte so a concurrent GC reader
    /// (`load_redirect`) never observes a torn byte. Mirrors `ConsCell::set_car`:
    /// the field stays a plain `u8`, accessed atomically via a raw cast. There is
    /// a single mutator, so the caller's plain read of `self.0` to compute `byte`
    /// does not race (the GC thread only ever reads this byte).
    #[inline]
    fn store_byte(&mut self, byte: u8) {
        let p = &self.0 as *const u8 as *const std::sync::atomic::AtomicU8;
        unsafe { (*p).store(byte, std::sync::atomic::Ordering::Relaxed) };
    }

    /// Atomic (relaxed) read of the redirect tag, for the concurrent GC obarray
    /// scan. Pairs with the `store_byte` writes above so the scan never reads a
    /// torn flags byte while the mutator changes a redirect/flag bit.
    #[inline]
    pub fn load_redirect(&self) -> SymbolRedirect {
        let p = &self.0 as *const u8 as *const std::sync::atomic::AtomicU8;
        let byte = unsafe { (*p).load(std::sync::atomic::Ordering::Relaxed) };
        SymbolRedirect::try_from(byte & Self::REDIRECT_MASK)
            .expect("symbol redirect flag contains valid GNU symbol_redirect code")
    }
}

/// One-word value cell for a symbol, reinterpreted by the [`SymbolFlags`]
/// `redirect` tag. Mirrors GNU `union { Lisp_Object value; struct
/// Lisp_Symbol *alias; struct Lisp_Buffer_Local_Value *blv; lispfwd fwd; }`
/// at `src/lisp.h:797-802`.
#[repr(C)]
#[derive(Copy, Clone)]
pub union SymbolVal {
    /// Live when redirect == Plainval. The value, or [`Value::NIL`] for
    /// "still unbound" (Phase 1 keeps an explicit "bound" bit on the side
    /// in [`LispSymbol::value`] until the legacy [`SymbolValue`] is removed
    /// in Phase 4-10).
    pub plain: Value,
    /// Live when redirect == Varalias. The aliased symbol id.
    pub alias: SymId,
    /// Live when redirect == Localized. Pointer to a heap-allocated
    /// per-symbol BLV cache. Null until Phase 4 wires up the LOCALIZED
    /// dispatch.
    pub blv: *mut LispBufferLocalValue,
    /// Live when redirect == Forwarded. Pointer to a 'static forwarder
    /// descriptor. Null until Phase 8 introduces forwarded variables.
    pub fwd: *const crate::emacs_core::forward::LispFwd,
}

impl Default for SymbolVal {
    fn default() -> Self {
        // Plainval / UNBOUND is the correct initial state — matches GNU
        // where freshly-interned symbols have val.value == Qunbound.
        Self {
            plain: Value::UNBOUND,
        }
    }
}

impl std::fmt::Debug for SymbolVal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Without the redirect tag we can't safely interpret the union;
        // print the raw bits for diagnostics.
        let raw: usize = unsafe { std::mem::transmute_copy(self) };
        write!(f, "SymbolVal({:#x})", raw)
    }
}

/// Per-symbol buffer-local cache. Mirrors GNU `struct
/// Lisp_Buffer_Local_Value` at `src/lisp.h:3116-3137`.
///
/// Phase 1 only declares the type; allocation and dispatch through it
/// land in Phases 4-6.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct LispBufferLocalValue {
    /// True if `make-variable-buffer-local` was called: any subsequent
    /// `set` creates a per-buffer binding. GNU `local_if_set`.
    pub local_if_set: bool,
    /// True if the loaded binding (`valcell`) was actually found in the
    /// buffer's `local_var_alist`, vs. the default. GNU `found`.
    pub found: bool,
    /// Optional forwarder for variables that have BOTH a per-buffer
    /// binding *and* a static C slot (e.g. `case-fold-search`). Must not
    /// be a `BufferObj` or `KboardObj`.
    pub fwd: Option<&'static crate::emacs_core::forward::LispFwd>,
    /// Buffer for which `valcell` was loaded, or `Value::NIL` for the
    /// global default. GNU `where`.
    pub where_buf: Value,
    /// `(SYMBOL . DEFAULT-VALUE)` cons. GNU `defcell`.
    pub defcell: Value,
    /// `(SYMBOL . CURRENT-VALUE)` cons. Equal to `defcell` when no
    /// per-buffer binding is loaded. GNU `valcell`.
    pub valcell: Value,
}

// ===========================================================================
// Legacy value-cell enum — to be removed in Phase 4-10
// ===========================================================================

// ===========================================================================
// LispSymbol — per-symbol metadata stored in the obarray
// ===========================================================================

/// Per-symbol metadata stored in the obarray. Mirrors GNU `struct
/// Lisp_Symbol` at `src/lisp.h:786-829`.
///
/// Renamed from `LispSymbol` as part of the symbol-redirect refactor
/// (Phase 1). As of Phase H the legacy `SymbolValue`/`special`/`constant`
/// mirror fields have been removed; all reads and writes go through
/// `flags` + `val`.
#[derive(Clone, Debug)]
pub struct LispSymbol {
    /// The symbol's name.
    pub name: NameId,
    /// Packed flags: redirect tag, trapped-write tag, interned tag,
    /// declared-special bit. Mirrors the first byte of GNU
    /// `Lisp_Symbol::s` (`lisp.h:786-792`).
    pub flags: SymbolFlags,
    /// One-word value cell. Reinterpreted by `flags.redirect()`.
    pub val: SymbolVal,
    /// Function slot. `Value::NIL` is the unbound sentinel (GNU `Qnil` in
    /// `struct Lisp_Symbol::s.function`, `lisp.h:820`).
    pub function: Value,
    /// Property list as a Lisp cons list (NIL = empty). Matches GNU
    /// `struct Lisp_Symbol::s.plist` (`lisp.h:820`).
    pub plist: Value,
    /// Whether this symbol is interned in the global obarray.
    interned_global: bool,
    /// Whether `fmakunbound` explicitly masked the symbol's fallback function.
    function_unbound: bool,
}

// Compile-time layout guard for the relaxed-atomic symbol-cell accesses
// (`load_value_atomic`/`store_value_atomic`). They reinterpret a one-word
// `Value` slot as `AtomicUsize`, which is only sound if `Value` is exactly a
// machine word wide and at least word-aligned.
const _: () = {
    assert!(core::mem::align_of::<Value>() >= core::mem::align_of::<usize>());
    assert!(core::mem::size_of::<Value>() == core::mem::size_of::<usize>());
};

/// Mirrors GNU `swap_in_symval_forwarding` (`src/data.c:1539-1571`).
///
/// Loads the BLV's `valcell` from the current buffer's
/// `local_var_alist` if `where_buf` doesn't already match. The Phase 4
/// shape doesn't yet support `Lisp_*Fwd` predicates or the
/// `local-flags` buffer slot — those land in Phase 8.
///
/// `current_buffer` is the buffer we're switching the cache to (a
/// `Value::buffer` or `Value::NIL` for the global default).
/// `local_var_alist` is `current_buffer`'s alist of `(sym . val)`
/// per-buffer bindings.
fn swap_in_blv(
    obarray: &mut Obarray,
    sym_id: SymId,
    current_buffer: Value,
    local_var_alist: Value,
) {
    let Some(blv) = obarray.blv_mut(sym_id) else {
        return;
    };
    // Find this symbol in the new buffer's alist.
    let key = Value::from_sym_id(sym_id);
    let found_cell = assq(key, local_var_alist);
    store_value_atomic(&mut blv.where_buf, current_buffer);
    blv.found = !found_cell.is_nil();
    let new_valcell = if blv.found { found_cell } else { blv.defcell };
    store_value_atomic(&mut blv.valcell, new_valcell);
}

/// Walk an alist looking for the cons whose car is `eq` to `key`.
/// Returns the matching cons or `Value::NIL`. Mirrors GNU `Fassq`.
///
/// Free function rather than a method on `Value` because Phase 4 needs
/// it locally and we don't want to grow the public Value API for an
/// internal helper.
fn assq(key: Value, mut alist: Value) -> Value {
    while alist.is_cons() {
        let entry = alist.cons_car();
        if entry.is_cons() && super::value::eq_value(&entry.cons_car(), &key) {
            return entry;
        }
        alist = alist.cons_cdr();
    }
    Value::NIL
}

/// `bindflag` argument for [`Obarray::set_internal_localized`].
/// Mirrors GNU `enum Set_Internal_Bind` (`src/lisp.h:3590-3596`).
#[derive(Copy, Clone, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum SetInternalBind {
    /// Ordinary `(setq foo bar)`. Auto-creates a per-buffer binding
    /// when `local_if_set` is true.
    Set = 0,
    /// `let`-binding initial assignment. Never auto-creates a new
    /// per-buffer binding (the existing one or the default is
    /// stashed in specpdl for unwind).
    Bind = 1,
    /// `let`-binding unwind. Restores the previous value.
    Unbind = 2,
    /// Thread-switch assignment. GNU uses this path to avoid hooks and
    /// buffer-local shadowing work while switching thread state.
    ThreadSwitch = 3,
}

impl SetInternalBind {
    pub fn from_gnu_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    pub fn gnu_code(self) -> u8 {
        self.into()
    }
}

/// Stub for GNU `let_shadows_buffer_binding_p`
/// (`src/eval.c:3559-3577`). Returns `true` if the symbol is
/// currently `let`-bound to a buffer-local binding shadowing the
/// per-buffer slot.
///
/// Phase 5 stub: always `false`. Phase 7 wires this against the
/// specpdl `LET_LOCAL` records.
pub fn let_shadows_buffer_binding_p(_sym_id: SymId) -> bool {
    false
}

/// Reasons [`Obarray::make_variable_alias`] can fail. Mirrors the
/// `xsignal` callsites in GNU `Fdefvaralias` (`src/eval.c:631-726`).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MakeAliasError {
    /// `new_alias` is a constant — cannot be redirected.
    Constant,
    /// `new_alias` is currently `SYMBOL_FORWARDED` (a built-in C
    /// variable). GNU rejects with "Cannot make a built-in variable
    /// an alias".
    Forwarded,
    /// `new_alias` is currently `SYMBOL_LOCALIZED` (a buffer-local).
    /// GNU rejects with "Don't know how to make a buffer-local
    /// variable an alias".
    Localized,
    /// Following `base`'s alias chain reaches `new_alias` — would
    /// create `cyclic-variable-indirection`.
    Cycle,
}

impl LispSymbol {
    pub fn new(id: SymId) -> Self {
        let mut flags = SymbolFlags::default();
        flags.set_redirect(SymbolRedirect::Plainval);
        Self {
            name: symbol_name_id(id),
            flags,
            val: SymbolVal {
                plain: Value::UNBOUND,
            },
            function: Value::NIL,
            plist: Value::NIL,
            interned_global: false,
            function_unbound: false,
        }
    }

    /// Read the redirect tag.
    #[inline]
    pub fn redirect(&self) -> SymbolRedirect {
        self.flags.redirect()
    }

    #[inline]
    pub fn is_interned_global(&self) -> bool {
        self.interned_global
    }

    /// Read the value cell as a plain `Value`. Caller must have verified
    /// the redirect is `Plainval`.
    #[inline]
    pub fn plain(&self) -> Value {
        debug_assert_eq!(self.redirect(), SymbolRedirect::Plainval);
        unsafe { self.val.plain }
    }

    /// Write the value cell as a plain `Value`. Caller must have set the
    /// redirect to `Plainval` (or be initializing a fresh symbol).
    #[inline]
    pub fn set_plain(&mut self, v: Value) {
        debug_assert_eq!(self.redirect(), SymbolRedirect::Plainval);
        self.val = SymbolVal { plain: v };
    }

    /// Read the alias target. Caller must have verified the redirect is
    /// `Varalias`.
    #[inline]
    pub fn alias_target(&self) -> SymId {
        debug_assert_eq!(self.redirect(), SymbolRedirect::Varalias);
        unsafe { self.val.alias }
    }

    /// Switch this symbol to `Varalias` and store the target id.
    #[inline]
    pub fn set_alias_target(&mut self, target: SymId) {
        // SATB: a Plainval cell holds a heap Value about to become a non-heap alias
        // SymId — retain its pre-image during a concurrent mark before the clobber.
        if self.redirect() == SymbolRedirect::Plainval {
            crate::tagged::gc::note_root_overwrite(unsafe { self.val.plain });
        }
        self.flags.set_redirect(SymbolRedirect::Varalias);
        self.val = SymbolVal { alias: target };
    }
}

/// The obarray — a table of interned symbols.
///
/// This is the central symbol registry. `intern` looks up or creates symbols,
/// ensuring that `(eq 'foo 'foo)` is always true.
///
/// Phase 4 of the symbol-redirect refactor adds a heap-allocated BLV
/// pool ([`Obarray::blvs`]) for `LOCALIZED` symbols. The Obarray owns
/// every BLV; symbols' [`SymbolVal::blv`] field stores a raw pointer
/// into the pool. The custom [`Clone`] impl deep-copies BLVs and
/// remaps the pointers in the cloned symbols, so `Obarray::clone()`
/// stays semantically a deep copy. The custom [`Drop`] impl frees the
/// heap allocations.
pub struct Obarray {
    symbols: SymbolChunks,
    global_member_count: usize,
    function_epoch: u64,
    value_epoch: u64,
    /// Heap-allocated BLVs for `SYMBOL_LOCALIZED` symbols. Each entry
    /// is a `Box::into_raw` pointer; freed in [`Obarray::drop`]. The
    /// pool is append-only — we never reuse a slot.
    blvs: Vec<*mut LispBufferLocalValue>,
}

/// Power-of-two slots per obarray chunk (`idx >> 12` / `idx & 4095`).
const OBARRAY_CHUNK: usize = 4096;

/// Non-moving chunked backing for the obarray's symbol slots: a `Vec` spine of
/// fixed-size boxed arrays. Growth APPENDS a chunk, so existing chunk arrays never
/// move (only the 8-byte spine pointers do) — unlike the old flat `Vec`, whose
/// `resize_with` relocated every `LispSymbol`. This is the Stage 1b foundation: a
/// stable chunk address lets the GC thread scan a chunk concurrently with no
/// realloc UAF. Slot `idx` (== `SymId`) lives at `chunks[idx >> 12][idx & 4095]`,
/// preserving the dense `SymId == slot-index` identity the dump + iteration rely on.
struct SymbolChunks {
    chunks: Vec<Box<[Option<LispSymbol>; OBARRAY_CHUNK]>>,
    /// Per-chunk seqlock (one `AtomicU32` per chunk, index-aligned with `chunks`).
    /// Boxed so the counter address stays stable for the concurrent GC reader even
    /// when the `Vec` spine reallocs. Even = stable; odd = a `(flags, val)` write
    /// is in flight in that chunk. Only ever bumped while a concurrent mark is
    /// active (Stage 1b); zero cost otherwise. The GC reads it with the standard
    /// seqlock protocol (retry while odd / changed).
    seqs: Vec<Box<std::sync::atomic::AtomicU32>>,
    /// Logical slot count; grows to a chunk boundary as chunks are appended.
    len: usize,
}

impl Clone for SymbolChunks {
    fn clone(&self) -> Self {
        // A cloned obarray is never concurrently marked, so the seqlocks reset
        // to 0 (even). (`AtomicU32` is not `Clone`, hence the manual impl.)
        Self {
            chunks: self.chunks.clone(),
            seqs: self
                .chunks
                .iter()
                .map(|_| Box::new(std::sync::atomic::AtomicU32::new(0)))
                .collect(),
            len: self.len,
        }
    }
}

impl SymbolChunks {
    fn new() -> Self {
        Self {
            chunks: Vec::new(),
            seqs: Vec::new(),
            len: 0,
        }
    }

    #[inline(always)]
    fn get(&self, idx: usize) -> Option<&Option<LispSymbol>> {
        if idx >= self.len {
            return None;
        }
        Some(&self.chunks[idx >> 12][idx & (OBARRAY_CHUNK - 1)])
    }

    #[inline(always)]
    fn get_mut(&mut self, idx: usize) -> Option<&mut Option<LispSymbol>> {
        if idx >= self.len {
            return None;
        }
        Some(&mut self.chunks[idx >> 12][idx & (OBARRAY_CHUNK - 1)])
    }

    /// Grow (appending chunks; existing chunks never move) until `idx` is in
    /// range, returning a mutable reference to its slot.
    fn ensure(&mut self, idx: usize) -> &mut Option<LispSymbol> {
        while self.len <= idx {
            self.chunks.push(Box::new(std::array::from_fn(|_| None)));
            self.seqs
                .push(Box::new(std::sync::atomic::AtomicU32::new(0)));
            self.len += OBARRAY_CHUNK;
        }
        &mut self.chunks[idx >> 12][idx & (OBARRAY_CHUNK - 1)]
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.len
    }

    /// Iterate every slot in global `SymId` order (untouched tail slots are
    /// `None`, inert under `.flatten()`; `.enumerate()` yields the global index).
    fn iter(&self) -> impl Iterator<Item = &Option<LispSymbol>> {
        self.chunks.iter().flat_map(|c| c.iter())
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = &mut Option<LispSymbol>> {
        self.chunks.iter_mut().flat_map(|c| c.iter_mut())
    }

    /// Raw pointer to the seqlock guarding the chunk that holds slot `idx`. The
    /// seq box never moves, so the pointer stays valid for the concurrent reader
    /// even across a spine realloc. Returns `None` if `idx`'s chunk does not yet
    /// exist (the slot was never `ensure`d). Returning a raw pointer (not a
    /// borrow) lets a write site bump the seqlock and then take a `&mut` to the
    /// slot without a borrow conflict (the `AtomicU32` is interior-mutable).
    // Used by the write-site seqlock bump + the GC scan in the next increment.
    #[inline(always)]
    fn chunk_seq_ptr(&self, idx: usize) -> Option<*const std::sync::atomic::AtomicU32> {
        self.seqs
            .get(idx >> 12)
            .map(|b| &**b as *const std::sync::atomic::AtomicU32)
    }

    /// Capture the start-of-cycle scan parts for the Stage 1b concurrent obarray
    /// scan: per existing chunk, its slots-array base pointer + its seqlock pointer,
    /// plus the logical live-slot count. The chunk arrays and seq boxes never move
    /// once allocated, so these raw pointers stay valid for the whole GC cycle even
    /// if the mutator appends new chunks (the `Vec` spines may realloc, but the
    /// boxed targets do not). Kept inside `SymbolChunks` so the private fields are
    /// in scope.
    fn snapshot_parts(
        &self,
    ) -> (
        Vec<(
            *const Option<LispSymbol>,
            *const std::sync::atomic::AtomicU32,
        )>,
        usize,
    ) {
        let parts = self
            .chunks
            .iter()
            .zip(self.seqs.iter())
            .map(|(chunk, seqbox)| {
                (
                    chunk.as_ptr(),
                    &**seqbox as *const std::sync::atomic::AtomicU32,
                )
            })
            .collect();
        (parts, self.len)
    }
}

/// Start-of-cycle snapshot of the obarray's chunked symbol store for the Stage 1b
/// CONCURRENT OBARRAY SCAN. Captures, per chunk present at start, the chunk's
/// slots-array base pointer and its per-chunk seqlock pointer, the logical
/// live-slot count, and the chunk count. The GC thread walks slots `[0, n_slots)`
/// across these chunks, reading each symbol's heap children via the seqlock
/// protocol ([`read_symbol_children_consistent`]). Chunks (and slots) interned
/// mid-cycle live beyond `n_chunks`/`n_slots` and are NOT in the snapshot; they are
/// allocate-black-equivalent in the obarray sense and are picked up by the
/// termination re-seed of the new range.
///
/// The raw pointers are valid for the whole cycle because chunk arrays + seq boxes
/// never move (see [`SymbolChunks`]). Single mutator, single GC thread.
pub(crate) struct ObarrayScanSnapshot {
    /// (slots-array base ptr, chunk seqlock ptr) for each chunk present at start.
    chunks: Vec<(
        *const Option<LispSymbol>,
        *const std::sync::atomic::AtomicU32,
    )>,
    /// Logical live-slot count at start (so the scan covers slots [0, n_slots)).
    n_slots: usize,
    /// Chunk count at start (chunks beyond this are interned mid-cycle).
    n_chunks: usize,
}

// Safety: the snapshot holds raw pointers into the obarray's non-moving chunk
// arrays + seq boxes, which the obarray owns and keeps alive for the whole GC
// cycle. The GC thread only READS through them (the seqlock protocol coordinates
// with the single mutator's arm writes), so handing the snapshot to the GC thread
// is sound.
unsafe impl Send for ObarrayScanSnapshot {}

impl ObarrayScanSnapshot {
    /// Chunk count captured at start. Symbols interned mid-cycle live in chunks
    /// `>= n_chunks` (slots `>= n_slots`) and are not covered by this scan; the
    /// termination re-seed covers that new range.
    #[inline]
    pub(crate) fn n_chunks(&self) -> usize {
        self.n_chunks
    }

    /// Logical live-slot count captured at start. The scan covers slots
    /// `[0, n_slots)`; the termination re-seed covers `[n_slots, current_len)`.
    #[inline]
    pub(crate) fn n_slots(&self) -> usize {
        self.n_slots
    }

    /// Scan the snapshotted obarray symbol cells ONCE, on the GC thread, reading
    /// each present symbol's heap children via the seqlock protocol and invoking
    /// `push` for each heap-object child. The caller routes each pushed child to
    /// the gray worklist (conses) or the deferred list (non-cons), exactly like the
    /// gray-drain cons branch. Walks chunks in `SymId` order, stopping at the global
    /// slot index `n_slots`.
    ///
    /// # Safety
    /// Must run on the GC thread for a snapshot captured at the world-stopped start
    /// handshake of the CURRENTLY-RUNNING concurrent mark; the chunk + seq pointers
    /// must still address the live, non-moving obarray storage (guaranteed because
    /// chunk arrays + seq boxes never move, and the obarray outlives the cycle).
    pub(crate) unsafe fn scan(&self, mut push: impl FnMut(Value)) {
        let mut global_idx = 0usize;
        for &(slots_ptr, seq_ptr) in &self.chunks {
            if global_idx >= self.n_slots {
                break;
            }
            // Safety: seq_ptr addresses this chunk's boxed seqlock, which never
            // moves; valid for the whole cycle.
            let seq = unsafe { &*seq_ptr };
            for offset in 0..OBARRAY_CHUNK {
                if global_idx >= self.n_slots {
                    break;
                }
                // Safety: slots_ptr is this chunk's [Option<LispSymbol>; CHUNK]
                // base; `offset < OBARRAY_CHUNK` is in bounds; the chunk never
                // moves. A concurrent mutator only mutates the value-cell ARM
                // (flags + val word) under the seqlock, which the read protocol
                // validates — it never resizes or relocates the slot.
                let sym_opt = unsafe { &*slots_ptr.add(offset) };
                if let Some(sym) = sym_opt {
                    read_symbol_children_consistent(seq, sym, &mut push);
                }
                global_idx += 1;
            }
        }
    }
}

/// Brackets a symbol value-cell ARM change (redirect tag + val word) with the
/// per-chunk seqlock so a concurrent GC reader sees a consistent (redirect,val)
/// pair. Bumps the chunk seqlock to ODD on construction and back to EVEN on
/// drop. No-op unless a concurrent mark is active. Holds a raw pointer (not a
/// borrow) so the caller can still take `&mut` to the slot.
struct SeqlockWriteGuard {
    seq: Option<*const std::sync::atomic::AtomicU32>,
}
impl SeqlockWriteGuard {
    #[inline]
    fn new(seq: Option<*const std::sync::atomic::AtomicU32>) -> Self {
        if let Some(p) = seq {
            unsafe { (*p).fetch_add(1, std::sync::atomic::Ordering::Release) }; // -> odd
        }
        Self { seq }
    }
}
impl Drop for SeqlockWriteGuard {
    #[inline]
    fn drop(&mut self) {
        if let Some(p) = self.seq {
            unsafe { (*p).fetch_add(1, std::sync::atomic::Ordering::Release) }; // -> even
        }
    }
}

/// Read a symbol's traceable heap children CONSISTENTLY with concurrent mutator
/// arm changes, for the Stage 1b concurrent obarray scan (the GC-thread read
/// side; pairs with [`SeqlockWriteGuard`] on the write side).
///
/// `seq` is the symbol's per-chunk seqlock; `sym` the symbol in that chunk. The
/// standard seqlock read protocol (retry while the counter is odd or changes
/// across the read) guarantees the `(redirect, val)` pair is observed from a
/// single epoch — never torn — so `val` is interpreted only as the arm the
/// consistently-observed `redirect` names. Only `Plainval` holds a heap value
/// cell: alias = a non-heap `SymId`, localized = a `*mut BLV`, forwarded = a raw
/// fwd ptr — none is a heap `Value` to trace here (BLV interiors are reached via
/// the BLV-pool root). `function`/`plist` are single-word atomic `Value`s with no
/// discriminant, so they are always consistent. `push` is called for each
/// heap-object child to enqueue onto the GC gray set.
///
/// Caller must hold the start-of-cycle chunk snapshot so `sym`/`seq` address live,
/// non-moving memory. Bounded in practice: with a single mutator the odd window
/// is ~4 stores, so the retry loop converges immediately.
pub(crate) fn read_symbol_children_consistent(
    seq: &std::sync::atomic::AtomicU32,
    sym: &LispSymbol,
    mut push: impl FnMut(Value),
) {
    use std::sync::atomic::Ordering;
    loop {
        let s1 = seq.load(Ordering::Acquire);
        if s1 & 1 != 0 {
            // A `(flags, val)` arm change is in flight in this chunk — wait it out.
            std::hint::spin_loop();
            continue;
        }
        let redirect = sym.flags.load_redirect();
        // Read `val` as a raw word regardless of arm; it is only INTERPRETED
        // below when the consistently-observed redirect is `Plainval`.
        let plain = load_value_atomic(unsafe { &sym.val.plain });
        let function = load_value_atomic(&sym.function);
        let plist = load_value_atomic(&sym.plist);
        if seq.load(Ordering::Acquire) != s1 {
            // An arm change landed during the read — the quadruple may be torn.
            continue;
        }
        // Consistent snapshot. `is_heap_object()` excludes fixnums, nil, symbol
        // ids and UNBOUND, so the Plainval gate never traces a non-heap word.
        if redirect == SymbolRedirect::Plainval && plain.is_heap_object() {
            push(plain);
        }
        if function.is_heap_object() {
            push(function);
        }
        if plist.is_heap_object() {
            push(plist);
        }
        return;
    }
}

impl std::fmt::Debug for Obarray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Obarray")
            .field("global_member_count", &self.global_member_count)
            .field("function_epoch", &self.function_epoch)
            .field("blvs", &self.blvs.len())
            .finish_non_exhaustive()
    }
}

impl Drop for Obarray {
    fn drop(&mut self) {
        for ptr in self.blvs.drain(..) {
            // Safety: we created each pointer via `Box::into_raw` in
            // `make_symbol_localized` and never alias it elsewhere
            // (the only other reference lives inside a `LispSymbol`'s
            // `val.blv` field, which goes away with `self`).
            unsafe { drop(Box::from_raw(ptr)) };
        }
    }
}

impl Clone for Obarray {
    fn clone(&self) -> Self {
        // Deep-copy the BLV pool. Build a `old → new` map so we can
        // remap each LOCALIZED symbol's `val.blv` to its clone.
        let mut blvs: Vec<*mut LispBufferLocalValue> = Vec::with_capacity(self.blvs.len());
        let mut blv_map: rustc_hash::FxHashMap<usize, *mut LispBufferLocalValue> =
            rustc_hash::FxHashMap::default();
        for &orig in &self.blvs {
            // Safety: each entry was Box::into_raw'd by us and is
            // alive for the duration of `&self`.
            let cloned_box = Box::new(unsafe { (*orig).clone() });
            let cloned_ptr = Box::into_raw(cloned_box);
            blvs.push(cloned_ptr);
            blv_map.insert(orig as usize, cloned_ptr);
        }
        let mut symbols = self.symbols.clone();
        for slot in symbols.iter_mut().flatten() {
            if slot.flags.redirect() == SymbolRedirect::Localized {
                let orig = unsafe { slot.val.blv };
                if let Some(&new_ptr) = blv_map.get(&(orig as usize)) {
                    slot.val = SymbolVal { blv: new_ptr };
                }
            }
        }
        Self {
            symbols,
            global_member_count: self.global_member_count,
            function_epoch: self.function_epoch,
            value_epoch: self.value_epoch,
            blvs,
        }
    }
}

// Safety: Obarray contains raw pointers to its own heap allocations.
// They're owned by the obarray, so sending the obarray across threads
// (via Send) or sharing it via &Obarray (via Sync) is safe — the
// pointers don't escape and don't carry interior mutability.
unsafe impl Send for Obarray {}
unsafe impl Sync for Obarray {}

impl Default for Obarray {
    fn default() -> Self {
        Self::new()
    }
}

impl Obarray {
    fn is_canonical_symbol_id(id: SymId) -> bool {
        is_canonical_id(id)
    }

    #[inline(always)]
    fn slot_index(id: SymId) -> usize {
        id.0 as usize
    }

    #[inline(always)]
    fn slot(&self, id: SymId) -> Option<&LispSymbol> {
        self.symbols
            .get(Self::slot_index(id))
            .and_then(Option::as_ref)
    }

    #[inline(always)]
    fn slot_mut(&mut self, id: SymId) -> Option<&mut LispSymbol> {
        self.symbols
            .get_mut(Self::slot_index(id))
            .and_then(Option::as_mut)
    }

    fn ensure_slot(&mut self, id: SymId) -> &mut LispSymbol {
        let idx = Self::slot_index(id);
        self.symbols
            .ensure(idx)
            .get_or_insert_with(|| LispSymbol::new(id))
    }

    /// Returns a seqlock guard for the chunk holding `id`'s slot, armed only while a
    /// concurrent mark is active. Must be created BEFORE the redirect/val write and
    /// held until after it (the RAII drop closes the window).
    #[inline]
    fn seqlock_guard(&self, id: SymId) -> SeqlockWriteGuard {
        let seq = if crate::tagged::gc::concurrent_mark_active() {
            self.symbols.chunk_seq_ptr(Self::slot_index(id))
        } else {
            None
        };
        SeqlockWriteGuard::new(seq)
    }

    /// Capture a start-of-cycle [`ObarrayScanSnapshot`] for the Stage 1b concurrent
    /// obarray scan. MUST be called at the world-stopped start handshake (the same
    /// point the cons-block snapshot is taken), so `n_slots`/`n_chunks` are a
    /// consistent picture of the obarray at start. Chunk arrays + seq boxes never
    /// move, so the captured raw pointers stay valid for the whole cycle.
    pub(crate) fn scan_snapshot(&self) -> ObarrayScanSnapshot {
        let (chunks, n_slots) = self.symbols.snapshot_parts();
        let n_chunks = chunks.len();
        ObarrayScanSnapshot {
            chunks,
            n_slots,
            n_chunks,
        }
    }

    /// Current logical slot count (chunk-boundary-rounded). Used by the Stage 1b
    /// termination residual to bound the new-symbol re-seed range.
    pub(crate) fn current_slot_len(&self) -> usize {
        self.symbols.len()
    }

    /// Stage 1b termination residual: seed the val/function/plist roots for symbols
    /// interned MID-CYCLE — slots `[from_slot, len)` that were not in the start
    /// snapshot and so were never scanned by the GC thread. Mirrors the symbol-cell
    /// arm of [`trace_roots`] but bounded to the new range. Runs at the STW
    /// termination (single-threaded, no seqlock needed). The BLV pool is re-scanned
    /// separately by the unbounded `trace_roots` BLV loop, so it is not repeated here.
    pub(crate) fn trace_new_symbol_cells(&self, from_slot: usize, mut push: impl FnMut(Value)) {
        let len = self.symbols.len();
        for idx in from_slot..len {
            let Some(Some(sym)) = self.symbols.get(idx) else {
                continue;
            };
            match sym.flags.redirect() {
                SymbolRedirect::Plainval => {
                    let v = load_value_atomic(unsafe { &sym.val.plain });
                    if v != Value::UNBOUND {
                        push(v);
                    }
                }
                SymbolRedirect::Varalias
                | SymbolRedirect::Forwarded
                | SymbolRedirect::Localized => {}
            }
            push(load_value_atomic(&sym.function));
            push(load_value_atomic(&sym.plist));
        }
    }

    fn mark_global_member(&mut self, id: SymId) {
        let added = {
            let sym = self.ensure_slot(id);
            if sym.interned_global {
                return;
            }
            sym.interned_global = true;
            sym.flags.set_interned(SymbolInterned::InternedInInitial);
            let name = resolve_sym_lisp_string(id);
            if name.as_bytes().first().is_some_and(|byte| *byte == b':') {
                // Match GNU lread.c intern_sym: keywords interned in the
                // initial obarray are self-evaluating constants and are marked
                // declared-special.
                sym.flags.set_declared_special(true);
                sym.flags.set_trapped_write(SymbolTrappedWrite::NoWrite);
                // Only initialize if not already set (idempotent).
                // Phase F: check val.plain (UNBOUND = not yet set).
                if unsafe { sym.val.plain } == Value::UNBOUND {
                    let kw = Value::keyword_id(id);
                    sym.flags.set_redirect(SymbolRedirect::Plainval);
                    sym.val = SymbolVal { plain: kw };
                }
            }
            true
        };
        if added {
            self.global_member_count += 1;
        }
    }

    fn clear_global_member(&mut self, id: SymId) -> bool {
        let Some(sym) = self.slot_mut(id) else {
            return false;
        };
        if !sym.interned_global {
            return false;
        }
        sym.interned_global = false;
        sym.flags.set_interned(SymbolInterned::Uninterned);
        self.global_member_count = self.global_member_count.saturating_sub(1);
        true
    }

    fn ensure_global_member_if_canonical(&mut self, id: SymId) {
        if Self::is_canonical_symbol_id(id) {
            self.mark_global_member(id);
        }
    }

    fn is_global_member(&self, id: SymId) -> bool {
        self.slot(id).is_some_and(|sym| sym.interned_global)
    }

    fn value_from_symbol_id(&self, id: SymId) -> Value {
        if self.is_global_member(id) {
            let name = resolve_sym_lisp_string(id);
            if name.as_bytes() == b"nil" {
                return Value::NIL;
            }
            if name.as_bytes() == b"t" {
                return Value::T;
            }
            if name.as_bytes().first().is_some_and(|byte| *byte == b':') {
                return Value::keyword_id(id);
            }
        }
        Value::symbol(id)
    }

    pub fn new() -> Self {
        let mut ob = Self {
            symbols: SymbolChunks::new(),
            global_member_count: 0,
            function_epoch: 0,
            value_epoch: 0,
            blvs: Vec::new(),
        };

        // Pre-intern fundamental symbols. Both `t` and `nil` are
        // self-referential constants in GNU.
        let t_id = intern("t");
        {
            let t_sym = ob.ensure_slot(t_id);
            t_sym.flags.set_redirect(SymbolRedirect::Plainval);
            t_sym.val = SymbolVal { plain: Value::T };
            t_sym.flags.set_trapped_write(SymbolTrappedWrite::NoWrite);
            t_sym.flags.set_declared_special(true);
        }
        ob.mark_global_member(t_id);

        let nil_id = intern("nil");
        {
            let nil_sym = ob.ensure_slot(nil_id);
            nil_sym.flags.set_redirect(SymbolRedirect::Plainval);
            nil_sym.val = SymbolVal { plain: Value::NIL };
            nil_sym.flags.set_trapped_write(SymbolTrappedWrite::NoWrite);
            nil_sym.flags.set_declared_special(true);
        }
        ob.mark_global_member(nil_id);

        ob
    }

    /// Intern a symbol: look up by name, creating if absent.
    /// Returns the symbol name (which is the key for identity).
    pub fn intern(&mut self, name: &str) -> String {
        let id = intern(name);
        self.ensure_symbol_id(id);
        self.mark_global_member(id);
        name.to_string()
    }

    /// Intern a symbol from an exact Lisp-string name, preserving raw
    /// unibyte and multibyte storage.
    pub fn intern_lisp_string(&mut self, name: &LispString) -> SymId {
        let id = intern_lisp_string(name);
        self.ensure_symbol_id(id);
        self.mark_global_member(id);
        id
    }

    /// Materialize a canonical symbol in the global obarray.
    ///
    /// GNU does this as part of interning into the initial obarray. Neomacs
    /// keeps string interning separate from obarray storage, so runtime paths
    /// that operate on canonical symbols can explicitly request the same
    /// initial-obarray semantics here.
    pub fn ensure_interned_global_id(&mut self, id: SymId) {
        self.ensure_global_member_if_canonical(id);
    }

    /// Materialize the symbols read from Lisp source in the active global
    /// obarray.  GNU's reader interns symbol tokens into `Vobarray` while
    /// reading; Neomacs' value reader allocates canonical symbol ids first,
    /// so callers that read source must apply the same obarray side effect.
    pub(crate) fn materialize_read_symbols(&mut self, value: Value) {
        // Cycle detection must use object *identity*, not `Value`'s `==`
        // (which is structural `equal`).  A `Vec` + `contains` here was
        // O(n^2) deep-`equal` over every loaded form -- the dominant cost of
        // startup.  Track visited heap objects by their tagged-pointer bits.
        let mut seen = rustc_hash::FxHashSet::default();
        self.materialize_read_symbols_1(value, &mut seen);
    }

    fn materialize_read_symbols_1(
        &mut self,
        value: Value,
        seen: &mut rustc_hash::FxHashSet<usize>,
    ) {
        match value.kind() {
            ValueKind::Symbol(id) => self.ensure_interned_global_id(id),
            ValueKind::Cons => {
                if !seen.insert(value.bits()) {
                    return;
                }
                self.materialize_read_symbols_1(value.cons_car(), seen);
                self.materialize_read_symbols_1(value.cons_cdr(), seen);
            }
            ValueKind::Veclike(
                VecLikeType::Vector
                | VecLikeType::Record
                | VecLikeType::Lambda
                | VecLikeType::Macro,
            ) => {
                if !seen.insert(value.bits()) {
                    return;
                }
                if let Some(slots) = value
                    .as_vector_data()
                    .or_else(|| value.as_record_data())
                    .or_else(|| value.closure_slots())
                {
                    for slot in slots.iter().copied() {
                        self.materialize_read_symbols_1(slot, seen);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::CharTable) => {
                if !seen.insert(value.bits()) {
                    return;
                }
                if let Some(slots) = value.char_table_external_slots() {
                    for slot in slots {
                        self.materialize_read_symbols_1(slot, seen);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::SubCharTable) => {
                if !seen.insert(value.bits()) {
                    return;
                }
                if let Some(table) = value.as_sub_char_table_obj() {
                    for slot in table.contents.iter().copied() {
                        self.materialize_read_symbols_1(slot, seen);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::HashTable) => {
                if !seen.insert(value.bits()) {
                    return;
                }
                if let Some(table) = value.as_hash_table() {
                    for key_value in table.key_snapshots.values().copied() {
                        self.materialize_read_symbols_1(key_value, seen);
                    }
                    for value in table.data.values().copied() {
                        self.materialize_read_symbols_1(value, seen);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                if !seen.insert(value.bits()) {
                    return;
                }
                if let Some(bytecode) = value.get_bytecode_data() {
                    self.materialize_read_symbols_1(bytecode.arglist, seen);
                    for constant in bytecode.constants.iter().copied() {
                        self.materialize_read_symbols_1(constant, seen);
                    }
                    if let Some(env) = bytecode.env {
                        self.materialize_read_symbols_1(env, seen);
                    }
                    if let Some(doc_form) = bytecode.doc_form {
                        self.materialize_read_symbols_1(doc_form, seen);
                    }
                    if let Some(interactive) = bytecode.interactive {
                        self.materialize_read_symbols_1(interactive, seen);
                    }
                    for slot in bytecode.extra_slots.iter().copied() {
                        self.materialize_read_symbols_1(slot, seen);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::SymbolWithPos) => {
                if let Some(symbol) = value.as_symbol_with_pos_sym() {
                    self.materialize_read_symbols_1(symbol, seen);
                }
            }
            _ => {}
        }
    }

    /// Look up a symbol without creating it. Returns None if not interned.
    pub fn intern_soft(&self, name: &str) -> Option<&LispSymbol> {
        let id = lookup_interned(name)?;
        self.slot(id).filter(|sym| sym.interned_global)
    }

    /// Look up a symbol without creating it, using exact Lisp-string storage.
    pub fn intern_soft_lisp_string(&self, name: &LispString) -> Option<SymId> {
        let id = lookup_interned_lisp_string(name)?;
        self.slot(id).filter(|sym| sym.interned_global)?;
        Some(id)
    }

    /// Get symbol data (mutable). Interns the symbol if needed.
    pub fn get_or_intern(&mut self, name: &str) -> &mut LispSymbol {
        let id = intern(name);
        self.mark_global_member(id);
        self.ensure_symbol_id(id)
    }

    /// Get symbol data (immutable).
    pub fn get(&self, name: &str) -> Option<&LispSymbol> {
        let id = lookup_interned(name)?;
        self.slot(id).filter(|sym| sym.interned_global)
    }

    /// Get symbol data (mutable).
    pub fn get_mut(&mut self, name: &str) -> Option<&mut LispSymbol> {
        let id = lookup_interned(name)?;
        self.slot_mut(id).filter(|sym| sym.interned_global)
    }

    /// Ensure symbol storage exists for an arbitrary symbol id.
    pub fn ensure_symbol_id(&mut self, id: SymId) -> &mut LispSymbol {
        self.ensure_slot(id)
    }

    /// Get symbol data by identity.
    pub fn get_by_id(&self, id: SymId) -> Option<&LispSymbol> {
        self.slot(id)
    }

    /// Get mutable symbol data by identity.
    pub fn get_mut_by_id(&mut self, id: SymId) -> Option<&mut LispSymbol> {
        self.slot_mut(id)
    }

    /// Get the value cell of a symbol.
    pub fn symbol_value(&self, name: &str) -> Option<&Value> {
        self.symbol_value_id(intern(name))
    }

    /// Get the value cell of a symbol by identity.
    /// Follows alias chains (with cycle detection, max 50 hops).
    ///
    /// Phase F: reads from the redirect union (`val`) rather than the
    /// legacy `value` enum field.
    #[inline(always)]
    pub fn symbol_value_id_copied(&self, id: SymId) -> Option<Value> {
        let sym = match self.symbols.get(Self::slot_index(id)) {
            Some(Some(sym)) => sym,
            _ => return None,
        };
        match sym.flags.redirect() {
            SymbolRedirect::Plainval => {
                // Safety: redirect=Plainval guarantees val.plain is
                // the live value field. UNBOUND sentinel = unbound.
                let value = unsafe { sym.val.plain };
                if value == Value::UNBOUND {
                    None
                } else {
                    Some(value)
                }
            }
            SymbolRedirect::Varalias => {
                let current = unsafe { sym.val.alias };
                self.symbol_value_id_copied_slow(current, 49)
            }
            SymbolRedirect::Localized => {
                let value = self.blv(id)?.defcell.cons_cdr();
                if value == Value::UNBOUND {
                    None
                } else {
                    Some(value)
                }
            }
            SymbolRedirect::Forwarded => None,
        }
    }

    #[cold]
    fn symbol_value_id_copied_slow(
        &self,
        mut current: SymId,
        mut remaining: usize,
    ) -> Option<Value> {
        while remaining > 0 {
            remaining -= 1;
            let sym = match self.symbols.get(Self::slot_index(current)) {
                Some(Some(sym)) => sym,
                _ => return None,
            };
            match sym.flags.redirect() {
                SymbolRedirect::Plainval => {
                    // Safety: redirect=Plainval guarantees val.plain is
                    // the live value field. UNBOUND sentinel = unbound.
                    let v = unsafe { sym.val.plain };
                    if v == Value::UNBOUND {
                        return None;
                    }
                    return Some(v);
                }
                SymbolRedirect::Varalias => {
                    current = unsafe { sym.val.alias };
                }
                SymbolRedirect::Localized => {
                    let value = self.blv(current)?.defcell.cons_cdr();
                    if value == Value::UNBOUND {
                        return None;
                    }
                    return Some(value);
                }
                SymbolRedirect::Forwarded => return None,
            }
        }
        None // alias cycle
    }

    /// Get a symbol's value by identity, returning nil when unbound.
    ///
    /// This is the copied-value equivalent of the common
    /// `symbol_value_id(...).copied().unwrap_or(Value::NIL)` pattern.
    /// GNU's `find_symbol_value` returns a `Lisp_Object` directly; keeping
    /// hot evaluator reads in this shape avoids an extra borrowed Option path.
    #[inline(always)]
    pub fn symbol_value_id_or_nil(&self, id: SymId) -> Value {
        match self.symbol_value_id_copied(id) {
            Some(value) => value,
            None => Value::NIL,
        }
    }

    pub fn symbol_value_id(&self, id: SymId) -> Option<&Value> {
        let mut current = id;
        for _ in 0..50 {
            let sym = match self.symbols.get(Self::slot_index(current)) {
                Some(Some(sym)) => sym,
                _ => return None,
            };
            match sym.flags.redirect() {
                SymbolRedirect::Plainval => {
                    // Safety: redirect=Plainval guarantees val.plain is
                    // the live value field. UNBOUND sentinel = unbound.
                    let v = unsafe { &sym.val.plain };
                    if *v == Value::UNBOUND {
                        return None;
                    }
                    return Some(v);
                }
                SymbolRedirect::Varalias => {
                    current = unsafe { sym.val.alias };
                }
                SymbolRedirect::Localized => {
                    // Return the BLV defcell default (global) value.
                    // The defcell is a heap-allocated cons (sym . default);
                    // its cdr field lives in the GC heap, which is owned
                    // by `self` for the lifetime of `&self`.
                    // UNBOUND cdr means the symbol has no global default.
                    return self.blv(current).and_then(|blv| {
                        // Safety: defcell is a valid heap cons (allocated
                        // by Value::cons in make_symbol_localized and kept
                        // alive by the GC root in blv.defcell). The cdr
                        // field lives in the ConsCell in the GC heap and
                        // is valid for the lifetime of `&self`.
                        let cdr_ref = unsafe {
                            let cons_ptr = blv.defcell.xcons_ptr();
                            &(*cons_ptr).cdr_or_next.cdr
                        };
                        if *cdr_ref == Value::UNBOUND {
                            None
                        } else {
                            Some(cdr_ref)
                        }
                    });
                }
                SymbolRedirect::Forwarded => return None,
            }
        }
        None // alias cycle
    }

    /// Set the value cell of a symbol. Interns if needed.
    pub fn set_symbol_value(&mut self, name: &str, value: Value) {
        let id = intern(name);
        self.mark_global_member(id);
        self.set_symbol_value_id_inner(id, value);
    }

    /// Set the value cell of a symbol by identity.
    pub fn set_symbol_value_id(&mut self, id: SymId, value: Value) {
        self.ensure_global_member_if_canonical(id);
        self.set_symbol_value_id_inner(id, value);
    }

    /// Allocate a fresh `LispBufferLocalValue` for `id`, flip the
    /// symbol's redirect to `Localized`, and store the BLV pointer in
    /// `val.blv`. Mirrors GNU `make_blv` (`src/data.c:2112-2140`).
    ///
    /// `default` becomes the cdr of `defcell` and `valcell` (initially
    /// the same cons, mirroring GNU's "valcell == defcell when no
    /// per-buffer binding loaded" invariant).
    ///
    /// If the symbol is already LOCALIZED, this is a no-op (returns
    /// the existing BLV pointer).
    pub fn make_symbol_localized(
        &mut self,
        id: SymId,
        default: Value,
    ) -> *mut LispBufferLocalValue {
        let target = self.resolve_alias_for_write(id);
        // Check existing state before mutating.
        if let Some(existing) = self.slot(target) {
            if existing.flags.redirect() == SymbolRedirect::Localized {
                return unsafe { existing.val.blv };
            }
        }
        // Build defcell = (sym . default). The same cons doubles as
        // valcell until per-buffer bindings are swapped in.
        let defcell = Value::cons(Value::from_sym_id(target), default);
        let blv = Box::new(LispBufferLocalValue {
            local_if_set: false,
            found: false,
            fwd: None,
            where_buf: Value::NIL,
            defcell,
            valcell: defcell,
        });
        let raw = Box::into_raw(blv);
        self.blvs.push(raw);
        // Stage 1b: bracket the redirect-arm change (Plainval/... -> Localized) +
        // val-word store with the per-chunk seqlock, armed only during a concurrent
        // mark. Created BEFORE the &mut slot borrow (holds a raw ptr, no borrow).
        let _seq_guard = self.seqlock_guard(target);
        let sym = self.ensure_symbol_id(target);
        // SATB: a Plainval cell holds a heap Value about to be replaced by the BLV
        // pointer — retain its pre-image during a concurrent mark (it only survives
        // transitively if it equals `default`, so log it unconditionally here).
        if sym.flags.redirect() == SymbolRedirect::Plainval {
            crate::tagged::gc::note_root_overwrite(unsafe { sym.val.plain });
        }
        sym.flags.set_redirect(SymbolRedirect::Localized);
        sym.val = SymbolVal { blv: raw };
        raw
    }

    /// Set the `local_if_set` flag on a LOCALIZED symbol's BLV. Used
    /// by `make-variable-buffer-local` (Phase 6) which differs from
    /// `make-local-variable` only in this flag. Phase 4 exposes the
    /// helper so the LOCALIZED tests can flip it directly.
    pub fn set_blv_local_if_set(&mut self, id: SymId, local_if_set: bool) {
        let target = self.resolve_alias_for_write(id);
        if let Some(sym) = self.slot(target) {
            if sym.flags.redirect() == SymbolRedirect::Localized {
                let blv = unsafe { &mut *sym.val.blv };
                blv.local_if_set = local_if_set;
            }
        }
    }

    /// Read a LOCALIZED symbol's BLV (immutable borrow). Returns
    /// `None` if the symbol is not LOCALIZED.
    pub fn blv(&self, id: SymId) -> Option<&LispBufferLocalValue> {
        let sym = self.slot(id)?;
        if sym.flags.redirect() != SymbolRedirect::Localized {
            return None;
        }
        // Safety: the symbol's val.blv was allocated by
        // make_symbol_localized and is owned by self.blvs. The
        // pointer stays valid for &self's lifetime because Drop
        // can't run while we hold &self.
        Some(unsafe { &*sym.val.blv })
    }

    /// Look up a LOCALIZED symbol's value in `target_buf` without
    /// mutating the BLV cache. Mirrors the GNU `Flocal_variable_p`
    /// fallback walk at `data.c:2399-2412`:
    ///
    /// 1. If the symbol isn't LOCALIZED, return `None`.
    /// 2. If the BLV cache is currently swapped to `target_buf`,
    ///    return `valcell.cdr` (the cached per-buffer or default
    ///    value, depending on `blv.found`).
    /// 3. Otherwise walk `target_alist` for an `(sym . val)` entry
    ///    and return its cdr if present (per-buffer binding without
    ///    swap-in).
    /// 4. Otherwise return `defcell.cdr` (the global default).
    ///
    /// Read-only — safe for `&self` callers like `eval_symbol_by_id`
    /// where the borrow checker can't accommodate the mutable
    /// `swap_in_blv` path that vm.rs `lookup_var_id` uses.
    pub fn read_localized(
        &self,
        id: SymId,
        target_buf: Value,
        target_alist: Value,
    ) -> Option<Value> {
        let blv = self.blv(id)?;
        // Neomacs keeps `local_var_alist` as the source of truth for
        // LOCALIZED per-buffer bindings. The BLV cache is an acceleration
        // structure and can be stale when an immutable read races with a
        // later make-local-variable/set path, so prefer the alist here.
        let key = Value::from_sym_id(id);
        let cell = assq(key, target_alist);
        if !cell.is_nil() {
            return Some(cell.cons_cdr());
        }
        // Fall back to the global default.
        Some(blv.defcell.cons_cdr())
    }

    /// Look up whether a LOCALIZED symbol has an explicit per-buffer
    /// binding in `target_buf`. Mirrors GNU `Flocal_variable_p`
    /// (`data.c:2380-2412`).
    pub fn has_per_buffer_binding(
        &self,
        id: SymId,
        target_buf: Value,
        target_alist: Value,
    ) -> bool {
        let Some(blv) = self.blv(id) else {
            return false;
        };
        // See `read_localized`: in Neomacs the alist is authoritative.
        let key = Value::from_sym_id(id);
        !assq(key, target_alist).is_nil()
    }

    /// Mutable BLV access. Used by `set_internal` (Phase 5) and
    /// `swap_in_symval_forwarding` (Phase 4).
    pub fn blv_mut(&mut self, id: SymId) -> Option<&mut LispBufferLocalValue> {
        let sym = self.slot(id)?;
        if sym.flags.redirect() != SymbolRedirect::Localized {
            return None;
        }
        // Safety: same rationale as `blv`. The mutable borrow follows
        // from `&mut self`.
        Some(unsafe { &mut *sym.val.blv })
    }

    /// Install a `BUFFER_OBJFWD` forwarder on a symbol. Phase 8a of
    /// the symbol-redirect refactor. Mirrors GNU `defvar_per_buffer`
    /// (`src/buffer.c:4990-5012`).
    ///
    /// The forwarder is leaked into a `'static` reference (the GNU
    /// `xmalloc` equivalent — these live until process exit). The
    /// symbol's redirect flips to `Forwarded` and `val.fwd` points
    /// at the descriptor. Subsequent reads of the symbol via
    /// [`Self::find_symbol_value_in_buffer`] will fetch the value
    /// from `Buffer::slots[offset]`.
    pub fn install_buffer_objfwd(
        &mut self,
        id: SymId,
        fwd: &'static crate::emacs_core::forward::LispBufferObjFwd,
    ) {
        // Stage 1b: bracket the redirect-arm change (Plainval/... -> Forwarded) +
        // val-word store with the per-chunk seqlock, armed only during a concurrent
        // mark. Created BEFORE the &mut slot borrow (holds a raw ptr, no borrow).
        let _seq_guard = self.seqlock_guard(id);
        let sym = self.ensure_symbol_id(id);
        // SATB: a Plainval cell holds a heap Value about to be replaced by a
        // forwarder descriptor — retain its pre-image during a concurrent mark.
        if sym.flags.redirect() == SymbolRedirect::Plainval {
            crate::tagged::gc::note_root_overwrite(unsafe { sym.val.plain });
        }
        sym.flags.set_redirect(SymbolRedirect::Forwarded);
        sym.flags.set_declared_special(true);
        sym.val = SymbolVal {
            fwd: fwd as *const crate::emacs_core::forward::LispBufferObjFwd
                as *const crate::emacs_core::forward::LispFwd,
        };
    }

    /// Read a symbol's value via the redirect dispatch. Mirrors GNU
    /// `find_symbol_value` (`src/data.c:1584-1609`).
    ///
    /// **Note:** this variant takes only the obarray and is correct
    /// for PLAINVAL / VARALIAS / FORWARDED cases. The LOCALIZED case
    /// returns the BLV's *defcell* default; per-buffer dispatch
    /// requires the buffer-aware [`Self::find_symbol_value_in_buffer`]
    /// variant.
    ///
    /// Returns `None` for unbound (`void-variable` callsite signals).
    pub fn find_symbol_value(&self, id: SymId) -> Option<Value> {
        let mut current = id;
        for _ in 0..50 {
            let sym = self.slot(current)?;
            match sym.flags.redirect() {
                SymbolRedirect::Plainval => {
                    // Read val.plain directly. UNBOUND sentinel means void.
                    let v = unsafe { sym.val.plain };
                    if v == Value::UNBOUND {
                        return None;
                    }
                    return Some(v);
                }
                SymbolRedirect::Varalias => {
                    // Phase 1 still keeps the legacy `value` field too,
                    // but we follow the redirect-side chain since it's
                    // the eventual source of truth.
                    current = unsafe { sym.val.alias };
                    continue;
                }
                SymbolRedirect::Localized => {
                    // Bare obarray reads of a LOCALIZED symbol return
                    // the BLV `defcell` (default cell), NOT the
                    // currently-loaded `valcell`. The valcell points
                    // at whatever buffer most recently swapped its
                    // per-buffer binding in via `swap_in_blv`, which
                    // is irrelevant when there is no caller-supplied
                    // buffer context.
                    //
                    // Buffer-local audit Medium 6 in
                    // `drafts/buffer-local-variables-audit.md`: the
                    // earlier code read `valcell.cons_cdr()` which
                    // could leak the per-buffer binding from another
                    // buffer when this function is called via
                    // `default-value` / `symbol-value` outside a
                    // buffer context.
                    //
                    // Mirrors GNU `find_symbol_value`
                    // (`src/data.c:1591-1607`) for the case when
                    // `current_buffer` is NULL: the SYMBOL_LOCALIZED
                    // arm reads the BLV default cell.
                    //
                    // Use the safe `Obarray::blv` accessor instead
                    // of dereferencing `sym.val.blv` directly so this
                    // code path stays out of `unsafe` blocks.
                    return self.blv(current).map(|blv| blv.defcell.cons_cdr());
                }
                SymbolRedirect::Forwarded => {
                    // Phase 10D: bare-obarray reads of FORWARDED
                    // BUFFER_OBJFWD symbols return the forwarder's
                    // default. Mirrors GNU `find_symbol_value`
                    // (`data.c:1591-1607`) which dispatches through
                    // `do_symval_forwarding` even without a current
                    // buffer; for BUFFER_OBJFWD that reads
                    // `buffer_defaults` (which we mirror as the
                    // forwarder's stored `default` field — keeping
                    // this in sync with `BufferManager::buffer_defaults`
                    // is `setq-default`'s job).
                    let fwd = unsafe { &*sym.val.fwd };
                    use crate::emacs_core::forward::{LispBufferObjFwd, LispFwdType};
                    if matches!(fwd.ty, LispFwdType::BufferObj) {
                        let buf_fwd = unsafe { &*(fwd as *const _ as *const LispBufferObjFwd) };
                        return Some(buf_fwd.default);
                    }
                    // Other forwarder types not yet implemented.
                    return None;
                }
            }
        }
        None // alias cycle
    }

    /// Buffer-aware variant of [`Self::find_symbol_value`]. Mirrors
    /// GNU `find_symbol_value` + `swap_in_symval_forwarding`
    /// (`src/data.c:1584-1571`).
    ///
    /// For LOCALIZED symbols, swaps the BLV cache to point at
    /// `current_buffer`'s per-buffer binding (if any) before reading.
    /// For FORWARDED symbols, reads through the forwarder descriptor:
    /// `BUFFER_OBJFWD` returns `current_buffer_slots[offset]`. Other
    /// variants are identical to [`Self::find_symbol_value`].
    ///
    /// `current_buffer_slots` is the current buffer's
    /// `Buffer::slots` array (or `None` if there's no current
    /// buffer — Forwarded reads then return the forwarder's default).
    pub fn find_symbol_value_in_buffer(
        &mut self,
        id: SymId,
        current_buffer_id: Option<crate::buffer::BufferId>,
        current_buffer_value: Value,
        local_var_alist: Value,
        current_buffer_slots: Option<&[Value]>,
        current_buffer_local_flags: u64,
        buffer_defaults: Option<&[Value]>,
    ) -> Option<Value> {
        let mut current = id;
        for _ in 0..50 {
            // Phase 4: only the LOCALIZED arm needs &mut self for the
            // cache swap. Borrow-check it carefully so the rest of the
            // walk can stay on a shared reference.
            let redirect = self.slot(current)?.flags.redirect();
            match redirect {
                SymbolRedirect::Plainval => {
                    return self.find_symbol_value(current);
                }
                SymbolRedirect::Varalias => {
                    let next = unsafe { self.slot(current)?.val.alias };
                    current = next;
                    continue;
                }
                SymbolRedirect::Localized => {
                    // Swap-in: if `where_buf` doesn't match the
                    // current buffer, scan the new buffer's
                    // local_var_alist for `(sym . val)` and update
                    // valcell. Mirrors GNU
                    // `swap_in_symval_forwarding`.
                    swap_in_blv(self, current, current_buffer_value, local_var_alist);
                    let blv = self.blv(current)?;
                    return Some(blv.valcell.cons_cdr());
                }
                SymbolRedirect::Forwarded => {
                    // Phase 8a: read through the forwarder descriptor.
                    // Phase 10D: dispatch on `local_flags_idx`.
                    // Always-local slots (`-1`) read `slots[off]`
                    // unconditionally; conditional slots (`>= 0`)
                    // gate the read on `local_flags`'s bit and fall
                    // through to `buffer_defaults` when clear.
                    // Mirrors GNU `do_symval_forwarding` BUFFER_OBJFWD
                    // arm + `PER_BUFFER_VALUE_P` (`buffer.h:1640`).
                    let sym = self.slot(current)?;
                    let fwd = unsafe { &*sym.val.fwd };
                    use crate::emacs_core::forward::{LispBufferObjFwd, LispFwdType};
                    match fwd.ty {
                        LispFwdType::BufferObj => {
                            let buf_fwd = unsafe { &*(fwd as *const _ as *const LispBufferObjFwd) };
                            let off = buf_fwd.offset as usize;
                            let flags_idx = buf_fwd.local_flags_idx;
                            // Conditional slot: gate on local_flags.
                            // GNU uses a separate `local_flags_idx`
                            // counter, but NeoMacs reuses `offset`
                            // as the bit index since both fit in
                            // BUFFER_SLOT_COUNT.
                            if flags_idx >= 0 {
                                let bit_set = (current_buffer_local_flags >> (off as u32)) & 1 != 0;
                                if bit_set {
                                    if let Some(slots) = current_buffer_slots
                                        && off < slots.len()
                                    {
                                        return Some(slots[off]);
                                    }
                                }
                                // Fall through to defaults.
                                if let Some(defaults) = buffer_defaults
                                    && off < defaults.len()
                                {
                                    return Some(defaults[off]);
                                }
                                return Some(buf_fwd.default);
                            }
                            // Always-local: slots are authoritative.
                            return Some(match current_buffer_slots {
                                Some(slots) if off < slots.len() => slots[off],
                                _ => buf_fwd.default,
                            });
                        }
                        _ => {
                            // Phase 8a stub: other forwarder types
                            // not yet implemented. Return the legacy
                            // PLAINVAL fallback for now.
                            return self.find_symbol_value(current);
                        }
                    }
                }
            }
        }
        None
    }

    /// Write a symbol's value via the redirect dispatch. Mirrors GNU
    /// `set_internal` (`src/data.c:1644-1795`).
    ///
    /// Phase 2: thin wrapper over `set_symbol_value_id` that exposes
    /// the GNU name. Phase 5+ adds the LOCALIZED-aware logic and the
    /// `where`/`bindflag` parameters via [`Self::set_internal_localized`].
    pub fn set_internal(&mut self, id: SymId, value: Value) {
        self.set_symbol_value_id(id, value);
    }

    /// LOCALIZED arm of `set_internal`. Mirrors GNU
    /// `set_internal` lines 1687-1763 (`src/data.c`).
    ///
    /// Updates the BLV cache and (for `Set` writes) creates a new
    /// per-buffer binding when `local_if_set` is true and no current
    /// binding exists. Returns the (possibly new) `local_var_alist`
    /// for the target buffer; the caller is responsible for storing
    /// it back into the buffer.
    ///
    /// Parameters:
    /// - `sym_id`: the symbol being written.
    /// - `value`: the new value.
    /// - `target_buf`: the buffer the write is targeting (a
    ///   `Value::buffer` for explicit, or whatever the caller treats
    ///   as the "current" buffer Value). Used as the cache key.
    /// - `target_alist`: the target buffer's current
    ///   `local_var_alist`. May be updated.
    /// - `bindflag`: `Set` for ordinary `(setq)` writes, `Bind` for
    ///   `let` initial bindings (which never auto-create).
    /// - `let_shadows`: result of [`let_shadows_buffer_binding_p`]
    ///   for this symbol — Phase 7 wires this; Phase 5 callers pass
    ///   `false`.
    ///
    /// Returns the updated alist (consed if a new cell was created;
    /// unchanged otherwise).
    pub fn set_internal_localized(
        &mut self,
        sym_id: SymId,
        value: Value,
        target_buf: Value,
        target_alist: Value,
        bindflag: SetInternalBind,
        let_shadows: bool,
    ) -> Value {
        let mut new_alist = target_alist;
        let blv = match self.blv_mut(sym_id) {
            Some(blv) => blv,
            None => return new_alist,
        };

        // Step 1: select the binding cell for this target buffer.
        // GNU's BLV cache is kept coherent with `local_var_alist`, so
        // `set_internal` can usually trust `blv->valcell` when `where`
        // already matches. Neomacs stores `local_var_alist` as the
        // authoritative binding list and some Lisp paths replace alist
        // entries without touching the BLV cache, so refresh from the
        // target alist before every LOCALIZED write.
        let key = Value::from_sym_id(sym_id);
        let mut cell = assq(key, new_alist);
        store_value_atomic(&mut blv.where_buf, target_buf);
        blv.found = true;

        if cell.is_nil() {
            // No existing binding for this buffer.
            let auto_create = bindflag == SetInternalBind::Set && blv.local_if_set && !let_shadows;
            if !auto_create {
                // Fall through to writing the default.
                blv.found = false;
                cell = blv.defcell;
            } else {
                // Cons up `(sym . current-default-cdr)` and prepend it
                // to the buffer's local_var_alist.
                let default_cdr = blv.defcell.cons_cdr();
                cell = Value::cons(key, default_cdr);
                new_alist = Value::cons(cell, new_alist);
            }
        }
        store_value_atomic(&mut blv.valcell, cell);

        // Step 2: actually write the new value into valcell's cdr.
        // The BLV's valcell is a shared cons whose cdr lives in the
        // tagged heap; mutate it via Value::set_cdr. Capture
        // valcell + defcell first so the BLV borrow ends before we
        // touch the cons cell.
        let valcell = blv.valcell;
        let defcell = blv.defcell;
        let _writing_default = super::value::eq_value(&valcell, &defcell);
        let _ = blv;
        valcell.set_cdr(value);
        self.value_epoch = self.value_epoch.wrapping_add(1);

        // Phase F: the legacy SymbolValue::BufferLocal mirror is no
        // longer written; symbol_value_id reads directly from the BLV
        // defcell cons via xcons_ptr. No legacy sync needed.
        new_alist
    }

    /// Inner helper: follow aliases and write the value at the resolved target.
    ///
    /// For LOCALIZED symbols, writes to the BLV's defcell.cdr (the global
    /// default). The redirect tag and BLV pointer are preserved — clobbering
    /// them would orphan the BLV. Mirrors GNU `set_default_internal`'s
    /// SYMBOL_LOCALIZED arm at `data.c:1853-1880` which writes through
    /// `XSETCDR(blv->defcell, value)` and propagates to all buffers
    /// without per-buffer entries.
    fn set_symbol_value_id_inner(&mut self, id: SymId, value: Value) {
        let target = self.resolve_alias_for_write(id);
        self.value_epoch = self.value_epoch.wrapping_add(1);
        // Stage 1b: bracket the redirect-arm change (the `_ =>` arm below resets to
        // Plainval) + val-word store with the per-chunk seqlock, armed only during a
        // concurrent mark. Created BEFORE the &mut slot borrow (holds a raw ptr, no
        // borrow). The Localized fast-path returns early, dropping the guard then;
        // the val word it touches lives in the BLV pool, not the seqlock'd slot.
        let _seq_guard = self.seqlock_guard(target);
        let sym = self.ensure_symbol_id(target);

        // LOCALIZED: write to BLV defcell (the default). Do NOT touch
        // the redirect or val.blv — that would orphan the BLV cache.
        // Phase F: no legacy SymbolValue mirror write needed.
        if sym.flags.redirect() == SymbolRedirect::Localized {
            // Safety: redirect=Localized guarantees val.blv is a
            // valid pointer to a BLV owned by self.blvs.
            unsafe {
                let blv = &mut *sym.val.blv;
                blv.defcell.set_cdr(value);
                // If the BLV cache is currently swapped to defcell
                // (no per-buffer entry loaded), mirror the new value
                // through valcell as well so subsequent reads
                // observe it without re-swapping.
                if super::value::eq_value(&blv.valcell, &blv.defcell) {
                    blv.valcell.set_cdr(value);
                }
            }
            return;
        }

        // Write through the redirect union. LOCALIZED is handled above.
        // VARALIAS should have been resolved by resolve_alias_for_write;
        // FORWARDED is a no-op placeholder. Everything else becomes Plainval.
        match sym.flags.redirect() {
            SymbolRedirect::Forwarded => { /* no-op placeholder */ }
            _ => {
                // SATB: a Plainval cell holds a heap Value about to be clobbered —
                // retain its pre-image during a concurrent mark. Gated on the OLD
                // redirect (set_redirect runs after) so `val.plain` is the live
                // union arm; a Varalias `_` holds a non-heap SymId, so skip it.
                if sym.flags.redirect() == SymbolRedirect::Plainval {
                    crate::tagged::gc::note_root_overwrite(unsafe { sym.val.plain });
                }
                sym.flags.set_redirect(SymbolRedirect::Plainval);
                store_value_atomic(unsafe { &mut sym.val.plain }, value);
            }
        }
    }

    /// Visit each stored symbol value cell that currently holds a `Value`.
    ///
    /// Phase F: reads from the redirect union (`val`) rather than the
    /// legacy `value` enum field. Visits Plainval symbols (non-UNBOUND)
    /// and BLV defcell defaults (for Localized symbols).
    pub fn for_each_value_cell_mut(&mut self, mut f: impl FnMut(&mut Value)) {
        for sym in self.symbols.iter_mut().flatten() {
            match sym.flags.redirect() {
                SymbolRedirect::Plainval => {
                    // Safety: redirect=Plainval guarantees val.plain is live.
                    let mut v = unsafe { sym.val.plain };
                    if v != Value::UNBOUND {
                        // SATB: this closure mutates the value cell in place, so
                        // retain the pre-image during a concurrent mark before f.
                        crate::tagged::gc::note_root_overwrite(v);
                        f(&mut v);
                        store_value_atomic(unsafe { &mut sym.val.plain }, v);
                    }
                }
                SymbolRedirect::Localized => {
                    // Visit the BLV defcell default. Route the write back through
                    // `set_cdr` so the heap SATB barrier logs the old cdr — the
                    // original raw-pointer write to the defcell cons bypassed it.
                    // Safety: redirect=Localized guarantees val.blv is valid.
                    unsafe {
                        let blv = &mut *sym.val.blv;
                        let mut cdr = blv.defcell.cons_cdr();
                        if cdr != Value::UNBOUND {
                            f(&mut cdr);
                            blv.defcell.set_cdr(cdr);
                        }
                    }
                }
                SymbolRedirect::Varalias | SymbolRedirect::Forwarded => {}
            }
        }
    }

    /// Follow alias chain for a mutable write, returning the resolved SymId.
    /// Max 50 hops to prevent infinite loops.
    ///
    /// Phase F: uses the redirect tag + val.alias rather than the legacy
    /// SymbolValue::Alias enum field.
    fn resolve_alias_for_write(&mut self, id: SymId) -> SymId {
        let mut current = id;
        for _ in 0..50 {
            match self.slot(current) {
                Some(s) if s.flags.redirect() == SymbolRedirect::Varalias => {
                    // Safety: redirect=Varalias guarantees val.alias is set.
                    current = unsafe { s.val.alias };
                }
                _ => return current,
            }
        }
        current // cycle — write to the last hop
    }

    /// Get the function cell of a symbol.
    pub fn symbol_function(&self, name: &str) -> Option<Value> {
        self.symbol_function_id(intern(name))
    }

    /// Get the function cell of a symbol by identity.
    pub fn symbol_function_id(&self, id: SymId) -> Option<Value> {
        let sym = self.slot(id)?;
        if sym.function_unbound || sym.function.is_nil() {
            return None;
        }
        Some(sym.function)
    }

    /// Get the function cell of a symbol from its Value representation.
    /// Uses the SymId directly, which works correctly for both interned
    /// and uninterned symbols (unlike `symbol_function(name)` which
    /// re-interns the name and would miss uninterned symbol function cells).
    pub fn symbol_function_of_value(&self, value: &Value) -> Option<Value> {
        match value.kind() {
            ValueKind::Symbol(id) => self.symbol_function_id(id),
            ValueKind::Nil => self.symbol_function("nil"),
            ValueKind::T => self.symbol_function("t"),
            _ => None,
        }
    }

    /// Set the function cell of a symbol (fset). Interns if needed.
    pub fn set_symbol_function(&mut self, name: &str, function: Value) {
        let id = intern(name);
        self.mark_global_member(id);
        let sym = self.ensure_symbol_id(id);
        // SATB: retain the function cell's pre-image during a concurrent mark.
        crate::tagged::gc::note_root_overwrite(sym.function);
        store_value_atomic(&mut sym.function, function);
        sym.function_unbound = false;
        self.note_function_redefined(id);
    }

    /// Record that function-call behavior changed WITHOUT a cell write — the
    /// static subr table (`register_global_subr_entry`) rewrites a subr's fn
    /// pointer/arity in place, invisibly to the cells. Bumping here keeps
    /// `function_epoch` a complete "any function binding may have changed"
    /// signal, which JIT call speculation relies on for validity.
    pub(crate) fn bump_function_epoch(&mut self) {
        self.function_epoch = self.function_epoch.wrapping_add(1);
    }

    /// A specific function `id` was redefined (cell write / fmakunbound): bump the
    /// epoch (the coarse "any binding may have changed" signal + JIT backstop).
    /// When JIT is enabled, also precisely evict the JIT cache entries of callers
    /// that INLINED `id`, so an unrelated redefinition no longer re-JITs every
    /// inlined function. Pure optimization layered on the epoch backstop — see
    /// jit::cache::evict_inline_dependents.
    fn note_function_redefined(&mut self, _id: SymId) {
        self.function_epoch = self.function_epoch.wrapping_add(1);
        #[cfg(feature = "jit")]
        crate::emacs_core::jit::cache::evict_inline_dependents(_id);
    }

    /// Set the function cell of a symbol by identity.
    pub fn set_symbol_function_id(&mut self, id: SymId, function: Value) {
        self.ensure_global_member_if_canonical(id);
        let sym = self.ensure_symbol_id(id);
        // SATB: retain the function cell's pre-image during a concurrent mark.
        crate::tagged::gc::note_root_overwrite(sym.function);
        store_value_atomic(&mut sym.function, function);
        sym.function_unbound = false;
        self.note_function_redefined(id);
    }

    /// Remove the function cell (fmakunbound).
    pub fn fmakunbound(&mut self, name: &str) {
        self.fmakunbound_id(intern(name));
    }

    /// Remove the function cell by identity.
    pub fn fmakunbound_id(&mut self, id: SymId) {
        self.ensure_global_member_if_canonical(id);
        let sym = self.ensure_symbol_id(id);
        let was_unbound = sym.function_unbound;
        let was_bound_function = !sym.function.is_nil();
        sym.function_unbound = true;
        // SATB: retain the function cell's pre-image during a concurrent mark.
        crate::tagged::gc::note_root_overwrite(sym.function);
        store_value_atomic(&mut sym.function, Value::NIL);
        if !was_unbound || was_bound_function {
            self.note_function_redefined(id);
        }
    }

    /// Remove function cell without marking as explicitly unbound.
    /// Used for init-time masking of lazily-materialized builtins.
    pub fn clear_function_silent(&mut self, name: &str) {
        self.clear_function_silent_id(intern(name));
    }

    /// Remove function cell without marking as explicitly unbound, by identity.
    pub fn clear_function_silent_id(&mut self, id: SymId) {
        let mut redefined = false;
        if let Some(sym) = self.slot_mut(id) {
            if !sym.function.is_nil() {
                // SATB: retain the function cell's pre-image during a concurrent mark.
                crate::tagged::gc::note_root_overwrite(sym.function);
                store_value_atomic(&mut sym.function, Value::NIL);
                redefined = true;
            }
        }
        if redefined {
            self.note_function_redefined(id);
        }
    }

    /// Remove the value cell (makunbound).
    pub fn makunbound(&mut self, name: &str) {
        self.makunbound_id(intern(name));
    }

    /// Remove the value cell by identity.
    /// Follows alias chains (max 50 hops).
    pub fn makunbound_id(&mut self, id: SymId) {
        self.ensure_global_member_if_canonical(id);
        let target = self.resolve_alias_for_write(id);
        // Stage 1b: bracket the redirect-arm change (-> Plainval/UNBOUND) + val-word
        // store with the per-chunk seqlock, armed only during a concurrent mark.
        // Created BEFORE the &mut slot borrow (holds a raw ptr, no borrow).
        let _seq_guard = self.seqlock_guard(target);
        if let Some(sym) = self.slot_mut(target) {
            if sym.flags.trapped_write() != SymbolTrappedWrite::NoWrite {
                // SATB: retain the old plain value during a concurrent mark before
                // clobbering to UNBOUND. Only the Plainval arm holds a heap value;
                // a Localized blv stays reachable via the BLV pool root.
                if sym.flags.redirect() == SymbolRedirect::Plainval {
                    crate::tagged::gc::note_root_overwrite(unsafe { sym.val.plain });
                }
                // Plainval / UNBOUND is the "no value" state, matching
                // GNU where makunbound sets val.value = Qunbound.
                sym.flags.set_redirect(SymbolRedirect::Plainval);
                sym.val = SymbolVal {
                    plain: Value::UNBOUND,
                };
                self.value_epoch = self.value_epoch.wrapping_add(1);
            }
        }
    }

    /// Check if a symbol is bound (has a value cell).
    pub fn boundp(&self, name: &str) -> bool {
        self.boundp_id(intern(name))
    }

    /// Check if a symbol is bound by identity.
    /// Follows alias chains (max 50 hops).
    ///
    /// Phase F: reads from the redirect union (`val`) rather than the
    /// legacy `value` enum field. Mirrors GNU `boundp` (`data.c:805-810`).
    pub fn boundp_id(&self, id: SymId) -> bool {
        let mut current = id;
        for _ in 0..50 {
            let Some(s) = self.slot(current) else {
                return false;
            };
            match s.flags.redirect() {
                SymbolRedirect::Plainval => {
                    // Safety: redirect=Plainval guarantees val.plain is live.
                    let v = unsafe { s.val.plain };
                    return v != Value::UNBOUND;
                }
                SymbolRedirect::Varalias => {
                    current = unsafe { s.val.alias };
                }
                SymbolRedirect::Localized => {
                    // Bound if the BLV defcell has a non-UNBOUND default.
                    return self
                        .blv(current)
                        .is_some_and(|blv| blv.defcell.cons_cdr() != Value::UNBOUND);
                }
                SymbolRedirect::Forwarded => {
                    // Phase 10D: BUFFER_OBJFWD slots are never unbound.
                    use crate::emacs_core::forward::LispFwdType;
                    let fwd = unsafe { &*s.val.fwd };
                    return matches!(fwd.ty, LispFwdType::BufferObj);
                }
            }
        }
        false // cycle
    }

    /// Check if a symbol has a function cell.
    pub fn fboundp(&self, name: &str) -> bool {
        self.fboundp_id(intern(name))
    }

    /// Check if a symbol has a function cell by identity.
    pub fn fboundp_id(&self, id: SymId) -> bool {
        self.slot(id)
            .is_some_and(|s| !s.function_unbound && !s.function.is_nil())
    }

    /// Get a property from the symbol's plist.
    pub fn get_property(&self, name: &str, prop: &str) -> Option<Value> {
        self.get_property_id(intern(name), intern(prop))
    }

    /// Get a property from the symbol's plist by identity.
    pub fn get_property_id(&self, symbol: SymId, prop: SymId) -> Option<Value> {
        let sym = self.slot(symbol)?;
        crate::emacs_core::plist::plist_get(sym.plist, &Value::from_sym_id(prop))
    }

    /// Set a property on the symbol's plist.
    ///
    /// Returns `Err(Flow)` if the existing plist is malformed (non-cons non-nil),
    /// matching GNU `Fput` / `Fplist_put` semantics.
    pub fn put_property(&mut self, name: &str, prop: &str, value: Value) -> Result<(), Flow> {
        let symbol = intern(name);
        self.mark_global_member(symbol);
        let sym = self.ensure_symbol_id(symbol);
        let (new_plist, _changed) = crate::emacs_core::plist::plist_put(
            sym.plist,
            Value::from_sym_id(intern(prop)),
            value,
        )?;
        // SATB: retain the plist cell's pre-image during a concurrent mark.
        crate::tagged::gc::note_root_overwrite(sym.plist);
        store_value_atomic(&mut sym.plist, new_plist);
        Ok(())
    }

    /// Set a property on the symbol's plist by identity.
    ///
    /// Returns `Err(Flow)` if the existing plist is malformed (non-cons non-nil),
    /// matching GNU `Fput` / `Fplist_put` semantics.
    pub fn put_property_id(
        &mut self,
        symbol: SymId,
        prop: SymId,
        value: Value,
    ) -> Result<(), Flow> {
        self.ensure_global_member_if_canonical(symbol);
        let sym = self.ensure_symbol_id(symbol);
        let (new_plist, _changed) =
            crate::emacs_core::plist::plist_put(sym.plist, Value::from_sym_id(prop), value)?;
        // SATB: retain the plist cell's pre-image during a concurrent mark.
        crate::tagged::gc::note_root_overwrite(sym.plist);
        store_value_atomic(&mut sym.plist, new_plist);
        Ok(())
    }

    /// Replace the complete plist for a symbol by identity.
    pub fn replace_symbol_plist_id<I>(&mut self, symbol: SymId, entries: I)
    where
        I: IntoIterator<Item = (SymId, Value)>,
    {
        self.ensure_global_member_if_canonical(symbol);
        let mut flat: Vec<Value> = Vec::new();
        for (k, v) in entries {
            flat.push(Value::from_sym_id(k));
            flat.push(v);
        }
        let new_plist = if flat.is_empty() {
            Value::NIL
        } else {
            Value::list(flat)
        };
        let sym = self.ensure_symbol_id(symbol);
        // SATB: retain the plist cell's pre-image during a concurrent mark.
        crate::tagged::gc::note_root_overwrite(sym.plist);
        store_value_atomic(&mut sym.plist, new_plist);
    }

    /// Store `plist` verbatim as the symbol's property list. Matches GNU
    /// `setplist`. `plist` is typically a Lisp cons list but may be any
    /// value (including NIL).
    pub fn set_symbol_plist_id(&mut self, symbol: SymId, plist: Value) {
        self.ensure_global_member_if_canonical(symbol);
        let sym = self.ensure_symbol_id(symbol);
        // SATB: retain the plist cell's pre-image during a concurrent mark.
        crate::tagged::gc::note_root_overwrite(sym.plist);
        store_value_atomic(&mut sym.plist, plist);
    }

    /// Get the symbol's full plist as a flat list.
    pub fn symbol_plist(&self, name: &str) -> Value {
        self.symbol_plist_id(intern(name))
    }

    /// Get the symbol's full plist as a flat list by identity.
    pub fn symbol_plist_id(&self, id: SymId) -> Value {
        self.slot(id).map(|s| s.plist).unwrap_or(Value::NIL)
    }

    /// Mark a symbol as special (dynamically bound).
    pub fn make_special(&mut self, name: &str) {
        let id = intern(name);
        self.mark_global_member(id);
        self.ensure_symbol_id(id).flags.set_declared_special(true);
    }

    /// Mark a symbol as special by identity.
    pub fn make_special_id(&mut self, id: SymId) {
        self.ensure_global_member_if_canonical(id);
        self.ensure_symbol_id(id).flags.set_declared_special(true);
    }

    /// Clear the special flag on a symbol.
    pub fn make_non_special(&mut self, name: &str) {
        let id = intern(name);
        self.mark_global_member(id);
        self.ensure_symbol_id(id).flags.set_declared_special(false);
    }

    /// Clear the special flag on a symbol by identity.
    pub fn make_non_special_id(&mut self, id: SymId) {
        self.ensure_global_member_if_canonical(id);
        self.ensure_symbol_id(id).flags.set_declared_special(false);
    }

    /// Check if a symbol is special.
    pub fn is_special(&self, name: &str) -> bool {
        self.is_special_id(intern(name))
    }

    /// Check if a symbol is special by identity.
    pub fn is_special_id(&self, id: SymId) -> bool {
        self.slot(id).is_some_and(|s| s.flags.declared_special())
    }

    /// Check if a symbol is a constant.
    pub fn is_constant(&self, name: &str) -> bool {
        self.is_constant_id(intern(name))
    }

    /// Check if a symbol is a constant by identity.
    pub fn is_constant_id(&self, id: SymId) -> bool {
        (Self::is_canonical_symbol_id(id)
            && resolve_sym_lisp_string(id)
                .as_bytes()
                .first()
                .is_some_and(|byte| *byte == b':'))
            || self
                .slot(id)
                .is_some_and(|s| s.flags.trapped_write() == SymbolTrappedWrite::NoWrite)
    }

    /// Mark a symbol as a hard constant (like SYMBOL_NOWRITE in GNU Emacs).
    pub fn set_constant(&mut self, name: &str) {
        let id = intern(name);
        self.set_constant_id(id);
    }

    /// Mark a symbol as a hard constant (like SYMBOL_NOWRITE in GNU Emacs) by identity.
    pub fn set_constant_id(&mut self, id: SymId) {
        self.ensure_global_member_if_canonical(id);
        self.ensure_symbol_id(id)
            .flags
            .set_trapped_write(SymbolTrappedWrite::NoWrite);
    }

    // ------------------------------------------------------------------
    // SymbolValue-aware helpers (buffer-local / alias introspection)
    // ------------------------------------------------------------------

    /// Mark a symbol as a buffer-local variable in the obarray.
    /// Preserves any existing default value from `Plain` or `BufferLocal`.
    ///
    /// Installs GNU-style `SYMBOL_LOCALIZED` state. If the symbol is
    /// already localized, only the `local_if_set` flag is updated.
    pub fn make_buffer_local(&mut self, name: &str, local_if_set: bool) {
        let id = intern(name);
        self.mark_global_member(id);
        let default = self.find_symbol_value(id).unwrap_or(Value::NIL);
        self.make_symbol_localized(id, default);
        self.set_blv_local_if_set(id, local_if_set);
    }

    /// Install a variable-alias edge: reading/writing `id` will redirect to `target`.
    ///
    /// Phase 1: maintains both the legacy enum and the new redirect tag.
    /// Phase 3 cuts callers over to the redirect-only path.
    pub fn make_alias(&mut self, id: SymId, target: SymId) {
        // Stage 1b: bracket the redirect-arm change (Plainval/... -> Varalias) +
        // val-word store performed inside `set_alias_target` with the per-chunk
        // seqlock, armed only during a concurrent mark. Created BEFORE the &mut slot
        // borrow (holds a raw ptr, no borrow).
        let _seq_guard = self.seqlock_guard(id);
        let sym = self.ensure_symbol_id(id);
        sym.set_alias_target(target);
    }

    /// Check whether a symbol is a buffer-local variable in the obarray.
    pub fn is_buffer_local(&self, name: &str) -> bool {
        self.is_buffer_local_id(intern(name))
    }

    /// Check whether a symbol is a buffer-local variable by identity.
    /// Phase F: uses the redirect tag rather than the legacy value enum.
    pub fn is_buffer_local_id(&self, id: SymId) -> bool {
        self.slot(id)
            .is_some_and(|s| s.flags.redirect() == SymbolRedirect::Localized)
    }

    /// Check whether a symbol is an alias by identity. Reads through the
    /// new redirect tag (Phase 3 of the symbol-redirect refactor).
    pub fn is_alias_id(&self, id: SymId) -> bool {
        self.slot(id)
            .is_some_and(|s| s.flags.redirect() == SymbolRedirect::Varalias)
    }

    /// Remove a variable alias without following it and leave SYMBOL void.
    /// Mirrors GNU `internal-delete-indirect-variable`: the alias symbol is
    /// restored to `SYMBOL_PLAINVAL` with `Qunbound` in its value cell.
    pub fn delete_variable_alias_id(&mut self, id: SymId) {
        self.ensure_global_member_if_canonical(id);
        // Stage 1b: bracket the redirect-arm change (Varalias -> Plainval) + val-word
        // store with the per-chunk seqlock, armed only during a concurrent mark.
        // Created BEFORE the &mut slot borrow (holds a raw ptr, no borrow).
        let _seq_guard = self.seqlock_guard(id);
        let sym = self.ensure_symbol_id(id);
        // SATB: normally the prior redirect is Varalias (a non-heap SymId), but
        // guard for a Plainval heap value being clobbered to UNBOUND during a mark.
        if sym.flags.redirect() == SymbolRedirect::Plainval {
            crate::tagged::gc::note_root_overwrite(unsafe { sym.val.plain });
        }
        sym.flags.set_redirect(SymbolRedirect::Plainval);
        sym.val = SymbolVal {
            plain: Value::UNBOUND,
        };
        self.value_epoch = self.value_epoch.wrapping_add(1);
    }

    /// Walk an alias chain to its terminus and return the resolved
    /// SymId. Mirrors GNU `indirect_variable` (`src/data.c:1284-1301`).
    /// Returns `None` if (and only if) a true cycle is detected via
    /// Floyd's tortoise/hare. Symbols that don't yet have a slot in
    /// the obarray are treated as "not an alias" and returned as-is —
    /// matching GNU's `XSYMBOL(sym)->u.s.redirect != SYMBOL_VARALIAS`
    /// fall-through path.
    pub fn indirect_variable_id(&self, id: SymId) -> Option<SymId> {
        let mut slow = id;
        let mut fast = id;
        loop {
            // Tortoise: advance one hop (or stop if not an alias).
            let Some(slow_sym) = self.slot(slow) else {
                return Some(slow); // no slot → not an alias
            };
            if slow_sym.flags.redirect() != SymbolRedirect::Varalias {
                return Some(slow);
            }
            slow = unsafe { slow_sym.val.alias };

            // Hare: advance two hops (or stop if not an alias).
            for _ in 0..2 {
                let Some(fast_sym) = self.slot(fast) else {
                    return Some(slow);
                };
                if fast_sym.flags.redirect() != SymbolRedirect::Varalias {
                    return Some(slow);
                }
                fast = unsafe { fast_sym.val.alias };
            }

            if slow == fast {
                return None; // cycle
            }
        }
    }

    /// Install a variable alias edge with full GNU semantics. Mirrors
    /// `Fdefvaralias` (`src/eval.c:631-726`):
    ///
    /// 1. `new_alias` must not be a constant.
    /// 2. `new_alias` must not currently be FORWARDED (a built-in C
    ///    variable).
    /// 3. `new_alias` must not currently be LOCALIZED (a buffer-local).
    /// 4. Walking from `base` along the alias chain must not pass through
    ///    `new_alias` (cycle detection).
    ///
    /// On success, flips `new_alias`'s redirect to `Varalias` pointing
    /// at `base` and marks both symbols `declared_special`. The legacy
    /// `value: SymbolValue::Alias` mirror stays in sync (deleted in
    /// Phase 10).
    ///
    /// Returns `Err(())` for cycle, constant, forwarded, or localized;
    /// the caller is responsible for translating into a Lisp signal.
    pub fn make_variable_alias(
        &mut self,
        new_alias: SymId,
        base: SymId,
    ) -> Result<(), MakeAliasError> {
        // Check current state of new_alias.
        if let Some(sym) = self.slot(new_alias) {
            if sym.flags.trapped_write() == SymbolTrappedWrite::NoWrite {
                return Err(MakeAliasError::Constant);
            }
            match sym.flags.redirect() {
                SymbolRedirect::Forwarded => return Err(MakeAliasError::Forwarded),
                SymbolRedirect::Localized => return Err(MakeAliasError::Localized),
                _ => {}
            }
        }

        // Walk the base chain looking for new_alias.
        let mut current = base;
        loop {
            if current == new_alias {
                return Err(MakeAliasError::Cycle);
            }
            let Some(sym) = self.slot(current) else {
                break;
            };
            if sym.flags.redirect() != SymbolRedirect::Varalias {
                break;
            }
            current = unsafe { sym.val.alias };
        }

        // Install the alias edge. `make_alias` keeps both
        // representations in sync.
        self.make_alias(new_alias, base);
        self.make_special_id(new_alias);
        self.make_special_id(base);
        Ok(())
    }

    /// Get the default value of a symbol, following aliases.
    /// For `Plainval` this is the direct value; for `Localized` it's the
    /// BLV defcell default; for `Varalias` it follows the chain; for
    /// `Forwarded` BUFFER_OBJFWD it returns the forwarder's static default.
    ///
    /// Phase F: reads from the redirect union (`val`) rather than the
    /// legacy `value` enum field.
    pub fn default_value_id(&self, id: SymId) -> Option<&Value> {
        let mut current = id;
        for _ in 0..50 {
            let sym = self.slot(current)?;
            match sym.flags.redirect() {
                SymbolRedirect::Plainval => {
                    // Safety: redirect=Plainval guarantees val.plain is live.
                    let v = unsafe { &sym.val.plain };
                    if *v == Value::UNBOUND {
                        return None;
                    }
                    return Some(v);
                }
                SymbolRedirect::Varalias => {
                    current = unsafe { sym.val.alias };
                }
                SymbolRedirect::Localized => {
                    // Return a reference to the BLV defcell cdr (the default).
                    return self.blv(current).and_then(|blv| {
                        // Safety: same as symbol_value_id's Localized arm.
                        let cdr_ref = unsafe {
                            let cons_ptr = blv.defcell.xcons_ptr();
                            &(*cons_ptr).cdr_or_next.cdr
                        };
                        if *cdr_ref == Value::UNBOUND {
                            None
                        } else {
                            Some(cdr_ref)
                        }
                    });
                }
                SymbolRedirect::Forwarded => {
                    use crate::emacs_core::forward::{LispBufferObjFwd, LispFwdType};
                    let fwd = unsafe { &*sym.val.fwd };
                    if matches!(fwd.ty, LispFwdType::BufferObj) {
                        let buf_fwd = unsafe { &*(fwd as *const _ as *const LispBufferObjFwd) };
                        return Some(&buf_fwd.default);
                    }
                    return None;
                }
            }
        }
        None
    }

    /// Follow function indirection (defalias chains).
    /// Returns the final function value, following symbol aliases.
    pub fn indirect_function(&self, name: &str) -> Option<Value> {
        self.indirect_function_id(intern(name))
    }

    /// Follow function indirection (defalias chains) by canonical symbol id.
    /// Returns the final function value, following symbol aliases.
    pub fn indirect_function_id(&self, id: SymId) -> Option<Value> {
        let mut current_id = id;
        loop {
            let sym = self.slot(current_id)?;
            if sym.function.is_nil() {
                return None;
            }
            let func = sym.function;
            match func.kind() {
                ValueKind::Symbol(id) => {
                    current_id = id;
                }
                _ => return Some(func),
            }
        }
    }

    /// Number of interned symbols.
    pub fn len(&self) -> usize {
        self.global_member_count
    }

    pub fn is_empty(&self) -> bool {
        self.global_member_count == 0
    }

    /// All interned symbol names.
    pub fn all_symbols(&self) -> Vec<&str> {
        self.symbols
            .iter()
            .flatten()
            .filter(|sym| sym.interned_global)
            .map(|sym| resolve_name(sym.name))
            .collect()
    }

    /// Remove a symbol from the obarray.  Returns `true` if it was present.
    pub fn unintern_name(&mut self, name: &str) -> bool {
        let Some(id) = lookup_interned(name) else {
            return false;
        };
        self.unintern_id(id)
    }

    /// Remove a symbol from the obarray by exact Lisp-string name.
    pub fn unintern_lisp_string(&mut self, name: &LispString) -> bool {
        let Some(id) = lookup_interned_lisp_string(name) else {
            return false;
        };
        self.unintern_id(id)
    }

    /// Remove an exact symbol object from the obarray. Returns `true` if that
    /// symbol was interned in this obarray.
    pub fn unintern_id(&mut self, id: SymId) -> bool {
        let removed_symbol = self.clear_global_member(id);
        if removed_symbol {
            crate::emacs_core::intern::unintern_canonical_id(id);
            self.note_function_redefined(id);
        }
        removed_symbol
    }

    /// Function-cell mutation epoch: a `u64` counter bumped on every `fset`. The
    /// JIT's speculative direct-call guards compare against a snapshot of this
    /// value, so it is "monotonic" only modulo 2^64. A wrap could falsely
    /// validate a stale baked call, but at ~1e7 fsets/s that is ~58,000 years
    /// away — physically unreachable; widen to u128 if that ever stops holding.
    pub fn function_epoch(&self) -> u64 {
        self.function_epoch
    }

    /// Value-cell mutation epoch — a `u64` counter bumped on every `set` (see
    /// `function_epoch` for the wrap caveat).
    pub fn value_epoch(&self) -> u64 {
        self.value_epoch
    }

    /// True when `fmakunbound` explicitly masked this symbol's fallback function definition.
    pub fn is_function_unbound(&self, name: &str) -> bool {
        self.is_function_unbound_id(intern(name))
    }

    /// True when `fmakunbound` explicitly masked this symbol's fallback function definition.
    pub fn is_function_unbound_id(&self, id: SymId) -> bool {
        self.slot(id).is_some_and(|sym| sym.function_unbound)
    }

    // -----------------------------------------------------------------------
    // pdump accessors
    // -----------------------------------------------------------------------

    /// Iterate over all (SymId, &LispSymbol) pairs (for pdump serialization).
    pub(crate) fn iter_symbols(&self) -> impl Iterator<Item = (SymId, &LispSymbol)> {
        self.symbols.iter().enumerate().filter_map(|(idx, slot)| {
            debug_assert!(idx <= u32::MAX as usize, "symbol index overflow");
            slot.as_ref().map(|sym| (SymId(idx as u32), sym))
        })
    }

    /// Iterate over ids interned in the global obarray.
    pub(crate) fn global_member_ids(&self) -> impl Iterator<Item = SymId> + '_ {
        self.iter_symbols()
            .filter(|(_, sym)| sym.interned_global)
            .map(|(id, _)| id)
    }

    /// Iterate over fmakunbound'd symbol ids (for pdump serialization).
    pub(crate) fn function_unbound_ids(&self) -> impl Iterator<Item = SymId> + '_ {
        self.iter_symbols()
            .filter(|(_, sym)| sym.function_unbound)
            .map(|(id, _)| id)
    }

    /// Reconstruct an Obarray from pdump data.
    pub(crate) fn from_dump(
        symbols: Vec<(SymId, LispSymbol)>,
        global_members: Vec<SymId>,
        function_unbound: Vec<SymId>,
        function_epoch: u64,
    ) -> Self {
        let max_slot = symbols
            .iter()
            .map(|(id, _)| Self::slot_index(*id))
            .chain(global_members.iter().map(|id| Self::slot_index(*id)))
            .chain(function_unbound.iter().map(|id| Self::slot_index(*id)))
            .max();
        let mut slots = SymbolChunks::new();
        if let Some(max_slot) = max_slot {
            slots.ensure(max_slot);
        }

        let mut ob = Self {
            symbols: slots,
            global_member_count: 0,
            function_epoch,
            value_epoch: 0,
            blvs: Vec::new(),
        };
        for (id, mut sym) in symbols {
            sym.interned_global = false;
            sym.function_unbound = false;
            *ob.symbols.ensure(Self::slot_index(id)) = Some(sym);
        }
        for id in global_members {
            let sym = ob
                .slot_mut(id)
                .expect("pdump global member must reference a loaded symbol");
            if !sym.interned_global {
                sym.interned_global = true;
                ob.global_member_count += 1;
            }
        }
        for id in function_unbound {
            ob.slot_mut(id)
                .expect("pdump function-unbound entry must reference a loaded symbol")
                .function_unbound = true;
        }
        ob
    }
}

impl GcTrace for Obarray {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        // The concurrent-mark TERMINATION re-seed skips this per-symbol
        // val/function/plist walk: the symbol-cell SATB barrier
        // (crate::tagged::gc::note_root_overwrite) already retained every overwrite
        // during the mark window. The flag is false everywhere else (start seed +
        // STW full collection) => full scan. The BLV-pool loop below ALWAYS runs —
        // the barrier does not track BLV valcell/where_buf rebinds, so it stays a
        // per-termination residual.
        let skip_symbol_cells = SEED_SKIP_OBARRAY_SYMBOL_CELLS.with(|c| c.get());
        for sym in self.symbols.iter().flatten() {
            if skip_symbol_cells {
                continue;
            }
            match sym.flags.redirect() {
                SymbolRedirect::Plainval => {
                    // Safety: redirect==Plainval guarantees val.plain is
                    // the live union variant. TaggedValue is Copy. Relaxed
                    // atomic load: a concurrent mutator may store via
                    // store_value_atomic into the same word.
                    let v = load_value_atomic(unsafe { &sym.val.plain });
                    if v != Value::UNBOUND {
                        roots.push(v);
                    }
                }
                // Varalias:  val.alias is a SymId, not a heap ref.
                // Forwarded: val.fwd is 'static forwarder metadata.
                // Localized: BLV contents traced via self.blvs below.
                SymbolRedirect::Varalias
                | SymbolRedirect::Forwarded
                | SymbolRedirect::Localized => {}
            }
            roots.push(load_value_atomic(&sym.function));
            roots.push(load_value_atomic(&sym.plist));
        }
        // BLV contents for LOCALIZED symbols. Unchanged.
        for &blv_ptr in &self.blvs {
            let blv = unsafe { &*blv_ptr };
            roots.push(load_value_atomic(&blv.defcell));
            roots.push(load_value_atomic(&blv.valcell));
            roots.push(load_value_atomic(&blv.where_buf));
        }
    }
}

thread_local! {
    /// Set ONLY during the concurrent-mark termination re-seed (see
    /// [`ObarraySymbolCellSkipGuard`]). When set, [`Obarray::trace_roots`] skips
    /// the ~450k-symbol value/function/plist walk because the symbol-cell SATB
    /// barrier ([`crate::tagged::gc::note_root_overwrite`]) already retained every
    /// such overwrite during the mark window. False elsewhere => full scan.
    static SEED_SKIP_OBARRAY_SYMBOL_CELLS: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

/// RAII guard that suppresses the obarray symbol-cell walk in
/// [`Obarray::trace_roots`] for its lifetime — used to wrap ONLY the
/// concurrent-mark termination re-seed, so it seeds the BLV-pool residual + the
/// non-obarray Context roots without the dominant per-symbol pass. `Drop` restores
/// the full-scan default (panic-safe). MUST NOT wrap the start seed or the STW
/// full-collection seeds, which require the complete obarray scan.
pub(crate) struct ObarraySymbolCellSkipGuard;

impl ObarraySymbolCellSkipGuard {
    pub(crate) fn new() -> Self {
        SEED_SKIP_OBARRAY_SYMBOL_CELLS.with(|c| c.set(true));
        Self
    }
}

impl Drop for ObarraySymbolCellSkipGuard {
    fn drop(&mut self) {
        SEED_SKIP_OBARRAY_SYMBOL_CELLS.with(|c| c.set(false));
    }
}

#[cfg(test)]
#[path = "symbol_test.rs"]
mod tests;
