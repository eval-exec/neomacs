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

use super::defvar_bool::ByteBooleanVars;
use super::intern::{
    NameId, SymId, intern, intern_lisp_string, is_canonical_id, lookup_interned,
    lookup_interned_lisp_string, resolve_name, resolve_sym_lisp_string, symbol_name_id,
};
use super::value::{Value, ValueKind, VecLikeType};
use crate::emacs_core::error::Flow;
use crate::gc_trace::GcTrace;
use crate::heap_types::LispString;
use crate::tagged::header::{load_value_atomic, store_value_atomic};
use num_enum::{IntoPrimitive, TryFromPrimitive};
#[cfg(test)]
use std::cell::Cell;
use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(test)]
thread_local! {
    static FUNCTION_CELL_LOOKUP_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_function_cell_lookup_count() {
    FUNCTION_CELL_LOOKUP_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn function_cell_lookup_count() -> usize {
    FUNCTION_CELL_LOOKUP_COUNT.with(Cell::get)
}

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

/// Locality established while declaring a Lisp-visible C/Rust variable.
///
/// GNU's `DEFVAR_*` family always declares the symbol dynamically special;
/// some declarations additionally call `make-variable-buffer-local`, making
/// the variable local in a buffer on first assignment.  Keeping that choice in
/// the declaration type prevents bootstrap code from installing only a value
/// and silently omitting either part of the binding contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LispVariableLocality {
    /// One dynamically special value shared by all buffers.
    Global,
    /// Dynamically special, with a buffer-local binding created on assignment.
    BufferLocalIfSet,
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

/// What GNU does with a variable write, once the symbol's `trapped_write`
/// flag has been consulted.
///
/// GNU spells this out twice, identically, in `set_internal`
/// (`src/data.c:1687-1697`) and `set_default_internal`
/// (`src/data.c:2039-2049`); [`Obarray::classify_constant_write`] is the one
/// place Neomacs spells it.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ConstantWrite {
    /// Not a constant: store the value.
    Writable,
    /// A keyword being assigned the value it already has.  GNU's comment is
    /// "Allow setting keywords to their own value"; it `return`s without
    /// storing and without signalling.
    KeywordSelfAssign,
    /// GNU signals `(setting-constant SYMBOL)`.
    Refused,
}

/// What a read of a special variable found when the reader had **no buffer**,
/// and whether GNU's C would have answered the same thing.
///
/// GNU has no buffer-less read of a special variable.
/// `swap_in_symval_forwarding` (`src/data.c:1573-1603`) ends with
/// `store_symval_forwarding (blv->fwd, blv_value (blv), NULL)`, which writes
/// `current_buffer`'s binding into the very cell the C code dereferences -- so
/// `Vfoo` *is* that buffer's value. And a `DEFVAR_PER_BUFFER` name has no
/// global at all: `BVAR (current_buffer, foo)` is its only spelling.
///
/// This port keeps the global obarray and the buffer-local binding in two
/// different places, so a Rust site holding only an `&Obarray` agrees with GNU
/// in the [`Global`](Self::Global) arm and nowhere else. Ledger 191's
/// `beginning-of-visual-line` defect was the [`DefaultOfLocalized`](Self::DefaultOfLocalized)
/// arm read as if it were [`Global`](Self::Global); ledger 196 audited the rest
/// of the class.
///
/// Handing back this closed enum rather than a bare `Option<Value>` is the
/// point: a caller that wants "what GNU's C reads here" has to say what it does
/// about the arms that are not [`Global`](Self::Global), and the same shape is
/// already how [`LispFwdType`](crate::emacs_core::forward::LispFwdType) and
/// [`ConstantWrite`] keep their callers honest.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[must_use]
pub enum BufferlessValue {
    /// Nothing has localised the symbol, so there is one value and this is it.
    /// GNU's C global holds exactly this.
    Global(Value),
    /// The symbol is `SYMBOL_LOCALIZED`: some buffer holds a binding of its
    /// own, so GNU's C global holds `current_buffer`'s value while this is only
    /// the BLV *defcell*.
    DefaultOfLocalized(Value),
    /// The symbol forwards into a `struct buffer` slot (GNU
    /// `DEFVAR_PER_BUFFER`). There is no global to read at all:
    /// [`LispFwd::load`](crate::emacs_core::forward::LispFwd::load) answers
    /// `None` for `LispFwdType::BufferObj` by construction, so a buffer-less
    /// read here sees nothing -- not even a default.
    PerBufferSlot,
    /// Void. GNU `Qunbound`.
    Void,
}

impl BufferlessValue {
    /// The value from whichever arm produced one.
    ///
    /// Spelled out at the call site so that accepting the localised answer is a
    /// decision with a name, not the silent default it used to be.
    #[must_use]
    pub fn any_arm(self) -> Option<Value> {
        match self {
            Self::Global(value) | Self::DefaultOfLocalized(value) => Some(value),
            Self::PerBufferSlot | Self::Void => None,
        }
    }

    /// Only the arm that agrees with GNU unconditionally.
    #[must_use]
    pub fn global_only(self) -> Option<Value> {
        match self {
            Self::Global(value) => Some(value),
            Self::DefaultOfLocalized(_) | Self::PerBufferSlot | Self::Void => None,
        }
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
    /// [`blv_alist_epoch`] value at the last `where_buf`/`valcell`
    /// refresh. The read fast path trusts `valcell` only while this
    /// matches the global epoch AND `where_buf` is the current buffer —
    /// the guard that makes GNU's same-buffer swap early-out sound here
    /// even though some paths edit `local_var_alist` structure without
    /// touching the BLV cache (they bump the epoch instead). Starts 0;
    /// the global epoch starts 1, so a fresh BLV always rescans first.
    pub alist_epoch: u64,
}

/// Global structural-mutation epoch for every buffer's `local_var_alist`:
/// bumped (via [`note_blv_alist_structural_mutation`]) whenever an alist
/// entry is REMOVED, REPLACED by a new cons, or the alist is rebuilt/reset
/// behind the BLV cache's back — `kill-local-variable`,
/// `kill-all-local-variables`, `make-local-variable`'s seed-prepend, the
/// raw `set_local_var_alist_entry` prepend. In-place `set_cdr` writes on an
/// existing entry do NOT bump (the cached `valcell` IS that cons, so the
/// write flows through it), and `set_internal_localized`'s auto-create
/// prepend does NOT bump (it re-points THIS symbol's cache itself; other
/// symbols' cells are untouched by a prepend).
///
/// Coarse by design: kill/make-local are mode-setup-rare while localized
/// reads are the session's hottest VarRef class (58% — Task 4 §2c), so
/// over-invalidation costs one extra assq rescan per cached symbol while
/// missing a bump would serve stale values. Relaxed ordering: Lisp mutators
/// run one at a time (GNU thread semantics); a racing reader at worst sees
/// the OLD epoch and rescans.
static BLV_ALIST_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Current structural epoch (see [`BLV_ALIST_EPOCH`]).
#[inline]
pub(crate) fn blv_alist_epoch() -> u64 {
    BLV_ALIST_EPOCH.load(std::sync::atomic::Ordering::Relaxed)
}

/// Record a structural `local_var_alist` mutation (see [`BLV_ALIST_EPOCH`]).
#[inline]
pub(crate) fn note_blv_alist_structural_mutation() {
    BLV_ALIST_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
/// Reserved [`NameId`] stored in the `name` cell of an EMPTY obarray slot.
/// The obarray's chunk store holds fully-initialized [`LispSymbol`]s rather
/// than `Option<LispSymbol>`; a slot is "empty" (never interned) iff its
/// `name` atom equals this sentinel. `u32::MAX` is reserved: real `NameId`s
/// mint densely from `NameId(strings.len())` (`intern.rs`), and the mint site
/// carries a `debug_assert` that a real id never reaches `u32::MAX`.
pub(crate) const SYMBOL_NAME_SENTINEL: NameId = NameId(u32::MAX);

#[derive(Debug)]
pub struct LispSymbol {
    /// The symbol's name, held as the raw [`NameId`] `u32` in an atomic cell.
    /// This field is BOTH the real name AND the obarray slot's presence
    /// discriminant ([`SYMBOL_NAME_SENTINEL`] == empty), so that the concurrent
    /// GC obarray scan ([`ObarrayScanSnapshot::scan`], the only cross-thread
    /// reader) can gate on presence with an `Acquire` load that pairs with the
    /// slot fill's terminal `Release` store (see [`LispSymbol::publish_fill`]):
    /// observing a non-sentinel name happens-after every arm write. Write-once
    /// (name is never reset to the sentinel on a live slot — presence is
    /// monotonic), so the single mutator reads it `Relaxed` ([`LispSymbol::name`]).
    name: AtomicU32,
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

// The obarray scan is the pause-floor bottleneck (CONCURRENT_GC.md:185-222): it
// walks `[LispSymbol; 4096]` chunks cache-line by cache-line. Reusing the
// write-once `name` field as the presence discriminant (an `AtomicU32`, same 4
// bytes as the old `NameId`) keeps the slot at its historical 32 bytes — do NOT
// grow it. This const-asserts that invariant so a future field addition trips
// the build rather than silently regressing scan throughput.
const _: () = {
    assert!(core::mem::size_of::<LispSymbol>() == 32);
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
    // Sample the structural epoch BEFORE the scan: if a mutation lands
    // mid-scan (impossible today — one Lisp mutator — but cheap to order
    // correctly), the cache records the pre-scan epoch and the next read
    // re-validates.
    let epoch = blv_alist_epoch();
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
    blv.alist_epoch = epoch;
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

/// A `local_var_alist` head produced by [`Obarray::set_internal_localized`].
///
/// That function only ever rewrites an existing binding cons's cdr IN PLACE or
/// prepends a fresh head cons -- it never unlinks an interior entry. That is
/// exactly the precondition behind the head-identity fast path in
/// `LocalVariableBindings::replace_alist`, which keeps the derived
/// symbol -> binding-cons index alive whenever the head is unchanged.
///
/// `Buffer::replace_local_var_alist` therefore accepts this type and nothing
/// else, and only this module can construct one. A caller that FILTERS the
/// binding list leaves the head cons in place while unlinking interior
/// entries, which the fast path cannot detect; such a caller must go through
/// `LocalVariableBindings::retain_bindings` instead, which splices and
/// invalidates on a single path.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SetInternalAlist(Value);

impl SetInternalAlist {
    /// The alist head, for storing into a buffer.
    pub(crate) fn into_value(self) -> Value {
        self.0
    }
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
    /// `new_alias` is dynamically rebound somewhere on the specpdl
    /// (`src/eval.c:702-711`).  GNU rejects with "Don't know how to make a
    /// let-bound variable an alias".
    ///
    /// The only member of this set that is NOT decidable from the obarray:
    /// GNU asks the binding stack, and it asks it *after* the value migration
    /// and the "Overwriting value" warning rather than in the redirect switch
    /// with the other four.  So [`Obarray::check_variable_alias`] cannot raise
    /// it and does not try; `defvaralias_impl` raises it at GNU's position
    /// from [`crate::emacs_core::eval::Context::symbol_is_let_bound`].  It
    /// lives in this enum anyway so the refusal set and its messages stay in
    /// one place (ledger 183).
    LetBound,
}

impl LispSymbol {
    /// A fully-initialized EMPTY obarray slot: `name == `[`SYMBOL_NAME_SENTINEL`]
    /// with the arm defaults GNU gives a freshly-interned symbol (Plainval /
    /// UNBOUND / NIL / NIL). Chunks are built heap-direct from this value in
    /// [`SymbolChunks::grow_for`]; a slot is later published
    /// by [`Self::publish_fill`], which flips the name off the sentinel LAST.
    fn empty() -> Self {
        let mut flags = SymbolFlags::default();
        flags.set_redirect(SymbolRedirect::Plainval);
        Self {
            name: AtomicU32::new(SYMBOL_NAME_SENTINEL.0),
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

    pub fn new(id: SymId) -> Self {
        let sym = Self::empty();
        // Fresh single-threaded construction. The cross-thread PUBLISH (a
        // `Release` store) happens at the obarray slot fill (`publish_fill`),
        // not here, so `Relaxed` is correct for building a detached symbol.
        sym.name.store(symbol_name_id(id).0, Ordering::Relaxed);
        sym
    }

    /// The symbol's name. Write-once, and read here only by the single mutator
    /// thread, so `Relaxed` is correct (program order orders the construction /
    /// publish store before any same-thread read). The concurrent GC obarray
    /// scan is the ONLY cross-thread reader and loads the name atom with
    /// `Acquire` itself — it does not go through this accessor.
    #[inline]
    pub fn name(&self) -> NameId {
        NameId(self.name.load(Ordering::Relaxed))
    }

    /// Presence predicate for the single mutator and stop-the-world callers
    /// (get/get_mut/iter/from_dump/trace/clone). `Relaxed`: the concurrent GC
    /// scan is the only reader that needs `Acquire`, and it loads the atom
    /// directly at its presence gate.
    #[inline]
    fn is_present(&self) -> bool {
        self.name.load(Ordering::Relaxed) != SYMBOL_NAME_SENTINEL.0
    }

    /// Publish `src` into this (currently EMPTY) slot: write ALL arm fields
    /// FIRST, THEN `Release`-store the name LAST. The terminal `Release`
    /// publishes the whole fill; the concurrent GC obarray scan's `Acquire`
    /// load of the name is the pairing entry gate, so once the scan observes
    /// the published (non-sentinel) name every arm write above happens-before
    /// its arm reads. A plain struct memcpy would NOT establish that ordering —
    /// the name store MUST be a separate `Release` after the arm writes. Called
    /// only on a pristine empty slot (presence is monotonic: None -> Some only).
    #[inline]
    fn publish_fill(&mut self, src: LispSymbol) {
        let published_name = src.name.load(Ordering::Relaxed);
        self.flags = src.flags;
        self.val = src.val;
        self.function = src.function;
        self.plist = src.plist;
        self.interned_global = src.interned_global;
        self.function_unbound = src.function_unbound;
        // Terminal Release: publishes the arm writes above to the GC scan's
        // Acquire load of `name`.
        self.name.store(published_name, Ordering::Release);
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

// Hand-written because `name: AtomicU32` is not `Clone`-derivable. A cloned
// obarray is never concurrently scanned, so a `Relaxed` load of the source name
// is sufficient; the arms are plain `Copy`. Mirrors what `#[derive(Clone)]`
// produced before the presence-byte-race fix (empty slots clone to empty:
// name == SENTINEL is copied verbatim).
impl Clone for LispSymbol {
    fn clone(&self) -> Self {
        Self {
            name: AtomicU32::new(self.name.load(Ordering::Relaxed)),
            flags: self.flags,
            val: self.val,
            function: self.function,
            plist: self.plist,
            interned_global: self.interned_global,
            function_unbound: self.function_unbound,
        }
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
    /// Test-only visibility into logical symbol-table access. Kept per
    /// obarray so parallel tests cannot contaminate one another's counts.
    #[cfg(test)]
    symbol_slot_read_count: std::sync::atomic::AtomicUsize,
    global_member_count: usize,
    function_epoch: u64,
    value_epoch: u64,
    /// Bumped whenever global-obarray MEMBERSHIP changes (mark/clear);
    /// keys the completion bucket-order cache below.
    members_epoch: u64,
    /// Memoized GNU-bucket-order symbol list for completion over the
    /// global obarray: try-completion/all-completions re-derive the same
    /// ~30k-symbol hash+sort per call (a bootstrap hotspot). Mutex, not
    /// RefCell — `&Obarray` is shared with the concurrent GC scan thread
    /// (uncontended in practice: completion runs on the Lisp thread).
    completion_order_cache: std::sync::Mutex<Option<CompletionOrderCache>>,
    /// Heap-allocated BLVs for `SYMBOL_LOCALIZED` symbols. Each entry
    /// is a `Box::into_raw` pointer; freed in [`Obarray::drop`]. The
    /// pool is append-only — we never reuse a slot.
    blvs: Vec<*mut LispBufferLocalValue>,
    /// Every forwarder descriptor installed in this obarray that OWNS a Lisp
    /// value -- `Lisp_Fwd_Int`, `Lisp_Fwd_Obj`, `Lisp_Fwd_Kboard_Obj` -- so
    /// the GC can trace what they hold (see [`Obarray::trace_roots`]).
    /// Append-only and leaked, exactly like GNU's static `DEFVAR_*` slots,
    /// which `staticpro` and `mark_kboards` root for the same reason.
    /// Membership is decided by `LispFwd::owned_value`, not by the caller.
    value_fwds: Vec<&'static crate::emacs_core::forward::LispFwd>,
    /// Cached `debug-on-next-call` `DEFVAR_BOOL` descriptor.  GNU's three
    /// armed dispatch sites read `globals.f_debug_on_next_call` as ONE load
    /// (`src/bytecode.c:798`, `src/eval.c:2601`, `src/eval.c:3189`);
    /// re-resolving the descriptor through the symbol slot on every bytecode
    /// `Op::Call` cost ~48 Ir/call on the Tier-0 differential.  Null until
    /// first resolved.  The address is stable for THIS obarray once resolved:
    /// `define_bool_variable` reuses an existing descriptor rather than
    /// replacing it, `make_blv` moves the SAME cell into the BLV, and
    /// `reattach_localized_forwarder` refuses a BLV that already has one.
    /// `clone()` resets it because clone duplicates stateful forwarders.
    debug_on_next_call_fwd: std::sync::atomic::AtomicPtr<crate::emacs_core::forward::LispBoolFwd>,
}

/// One logical read of a symbol's complete function-cell state.
///
/// `ExplicitlyUnbound` is distinct from `Empty`: GNU `fmakunbound` suppresses
/// Neomacs's lazily materialized canonical builtin fallback, while an ordinary
/// empty cell may still use that fallback. Keeping the states closed prevents
/// callers from re-reading the symbol slot to recover information discarded by
/// an `Option<Value>`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FunctionCellSnapshot {
    ExplicitlyUnbound,
    Empty,
    Bound(Value),
}

/// One logical read of a symbol's property-list state for lookup.
///
/// GNU's `plist_get` immediately returns nil for nil and malformed non-cons
/// plists.  Representing that terminal state explicitly lets hot `get` callers
/// avoid entering the general cycle-safe list walker while preserving the
/// verbatim value exposed by `symbol-plist`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SymbolPlistSnapshot {
    NoEntries,
    Entries(Value),
}

/// See [`Obarray::completion_order_cache`].
struct CompletionOrderCache {
    members_epoch: u64,
    obarray_len: usize,
    /// Immutable bucket-order snapshot shared by completion readers.  An
    /// `Arc<[SymId]>` makes a cache hit O(1) without lending the cache mutex
    /// across the caller's (potentially much longer) symbol-name scan.
    ids: std::sync::Arc<[SymId]>,
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
    /// Fully-initialized symbol slots. Each is a valid [`LispSymbol`]; an
    /// unfilled (never-interned) slot is [`LispSymbol::empty`]
    /// (`name == `[`SYMBOL_NAME_SENTINEL`]). Storing `LispSymbol` rather than
    /// `Option<LispSymbol>` removes the `Option` niche (which Rust packed into
    /// the `function_unbound` byte), so the concurrent GC scan's presence read
    /// no longer races the mutator's in-place flag flips / fresh fills — presence
    /// is now the atomic write-once `name` cell (task #23).
    chunks: Vec<Box<[LispSymbol; OBARRAY_CHUNK]>>,
    /// Per-chunk seqlock (one `AtomicU32` per chunk, index-aligned with `chunks`).
    /// Boxed so the counter address stays stable for the concurrent GC reader even
    /// when the `Vec` spine reallocs. Even = stable; odd = a `(flags, val)` write
    /// is in flight in that chunk. Only ever bumped while a concurrent mark is
    /// active (Stage 1b); zero cost otherwise. The GC reads it with the standard
    /// seqlock protocol (retry while odd / changed).
    // Each counter must keep a stable address while the Vec spine grows; the
    // per-element box is the concurrency invariant, not redundant storage.
    #[allow(clippy::vec_box)]
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

    /// Borrow the slot at `idx` iff it is in range AND PRESENT (published).
    /// Empty (never-interned) slots read as `None`, exactly like the old
    /// `Option<LispSymbol>` tail did under `.flatten()`. Mutator/STW caller, so
    /// the presence check is `Relaxed` (see [`LispSymbol::is_present`]).
    #[inline(always)]
    fn get(&self, idx: usize) -> Option<&LispSymbol> {
        if idx >= self.len {
            return None;
        }
        let slot = &self.chunks[idx >> 12][idx & (OBARRAY_CHUNK - 1)];
        slot.is_present().then_some(slot)
    }

    #[inline(always)]
    fn get_mut(&mut self, idx: usize) -> Option<&mut LispSymbol> {
        if idx >= self.len {
            return None;
        }
        let slot = &mut self.chunks[idx >> 12][idx & (OBARRAY_CHUNK - 1)];
        if slot.is_present() { Some(slot) } else { None }
    }

    /// Grow (appending chunks; existing chunks never move) until `idx` is in
    /// range, returning a mutable reference to its (possibly EMPTY) slot. New
    /// chunks are filled with [`LispSymbol::empty`]; a fresh slot is published
    /// by [`LispSymbol::publish_fill`].
    #[inline(always)]
    fn ensure(&mut self, idx: usize) -> &mut LispSymbol {
        if self.len <= idx {
            self.grow_for(idx);
        }
        &mut self.chunks[idx >> 12][idx & (OBARRAY_CHUNK - 1)]
    }

    /// Cold growth path, split out of [`SymbolChunks::ensure`] so the hot
    /// per-store path keeps a tiny stack frame. The chunk is built DIRECTLY on
    /// the heap (`Vec::collect` writes into the final allocation): the old
    /// `Box::new(std::array::from_fn(..))` materialized the 128 KiB
    /// `[LispSymbol; 4096]` array in `ensure`'s own stack frame (256 KiB with
    /// the extra move temp), which forced rustc's inline stack probing —
    /// a 64-page probe loop executed on EVERY call, growth or not. That probe
    /// loop alone was ~53% of a dynamic-binding setq benchmark's CPU.
    #[cold]
    #[inline(never)]
    fn grow_for(&mut self, idx: usize) {
        while self.len <= idx {
            let chunk: Box<[LispSymbol]> = (0..OBARRAY_CHUNK)
                .map(|_| LispSymbol::empty())
                .collect::<Vec<_>>()
                .into_boxed_slice();
            let chunk: Box<[LispSymbol; OBARRAY_CHUNK]> = chunk
                .try_into()
                .unwrap_or_else(|_| unreachable!("chunk built with OBARRAY_CHUNK elements"));
            self.chunks.push(chunk);
            self.seqs
                .push(Box::new(std::sync::atomic::AtomicU32::new(0)));
            self.len += OBARRAY_CHUNK;
        }
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.len
    }

    /// Iterate every slot in global `SymId` order — INCLUDING empty
    /// (never-interned) tail slots, which read `is_present() == false`.
    /// `.enumerate()` yields the global index; callers skip empties with
    /// `.filter(|s| s.is_present())` (was `.flatten()` over `Option`).
    fn iter(&self) -> impl Iterator<Item = &LispSymbol> {
        self.chunks.iter().flat_map(|c| c.iter())
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = &mut LispSymbol> {
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
        Vec<(*const LispSymbol, *const std::sync::atomic::AtomicU32)>,
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
    chunks: Vec<(*const LispSymbol, *const std::sync::atomic::AtomicU32)>,
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
                // Safety: slots_ptr is this chunk's [LispSymbol; CHUNK] base;
                // `offset < OBARRAY_CHUNK` is in bounds; the chunk never moves.
                // Every slot is a valid (possibly EMPTY) LispSymbol — there is no
                // uninitialized memory to read. A concurrent mutator only either
                // (a) publishes an empty slot via a terminal `Release` store to
                // `name` after writing the arms, or (b) mutates an
                // already-published slot's value-cell ARM under the seqlock;
                // neither resizes or relocates the slot.
                let slot = unsafe { &*slots_ptr.add(offset) };
                // PRESENCE GATE — the ONLY cross-thread presence read. `Acquire`
                // load of the write-once `name` cell, pairing with the fill's
                // terminal `Release` (`publish_fill`): observing a non-sentinel
                // name happens-after every arm write, so the seqlock read below
                // sees a fully-initialized slot (no data race on the arms). A
                // slot still reading SENTINEL is never-interned OR a fresh fill
                // not yet published — skip it; a symbol interned mid-cycle is
                // allocate-black / SATB-retained and need not be scanned now.
                if slot.name.load(Ordering::Acquire) != SYMBOL_NAME_SENTINEL.0 {
                    read_symbol_children_consistent(seq, slot, &mut push);
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
        for slot in symbols.iter_mut().filter(|s| s.is_present()) {
            if slot.flags.redirect() == SymbolRedirect::Localized {
                let orig = unsafe { slot.val.blv };
                if let Some(&new_ptr) = blv_map.get(&(orig as usize)) {
                    slot.val = SymbolVal { blv: new_ptr };
                }
            }
        }
        // Duplicate the forwarders that OWN a value. Sharing them would make
        // `(setq gc-cons-threshold ...)` in one obarray visible in the other,
        // which is the very desync the per-context descriptor exists to avoid.
        let mut fwd_map: rustc_hash::FxHashMap<
            usize,
            &'static crate::emacs_core::forward::LispFwd,
        > = rustc_hash::FxHashMap::default();
        let mut value_fwds = Vec::with_capacity(self.value_fwds.len());
        for slot in symbols.iter_mut().filter(|s| s.is_present()) {
            if slot.flags.redirect() != SymbolRedirect::Forwarded {
                continue;
            }
            let orig = unsafe { slot.val.fwd };
            let copy = match fwd_map.get(&(orig as usize)) {
                Some(&existing) => existing,
                None => {
                    // Safety: every descriptor is leaked at installation.
                    let orig_ref: &'static crate::emacs_core::forward::LispFwd = unsafe { &*orig };
                    let Some(copy) = orig_ref.clone_stateful() else {
                        continue;
                    };
                    if copy.owned_value().is_some() {
                        value_fwds.push(copy);
                    }
                    fwd_map.insert(orig as usize, copy);
                    copy
                }
            };
            slot.val = SymbolVal {
                fwd: copy as *const crate::emacs_core::forward::LispFwd,
            };
        }
        // A BLV built from a forwarded symbol keeps a pointer to the same
        // descriptor; re-point it at the clone so the pair stays consistent.
        for &blv_ptr in &blvs {
            let blv = unsafe { &mut *blv_ptr };
            if let Some(fwd) = blv.fwd
                && let Some(&copy) =
                    fwd_map.get(&(fwd as *const crate::emacs_core::forward::LispFwd as usize))
            {
                blv.fwd = Some(copy);
            }
        }
        Self {
            symbols,
            #[cfg(test)]
            symbol_slot_read_count: std::sync::atomic::AtomicUsize::new(0),
            global_member_count: self.global_member_count,
            function_epoch: self.function_epoch,
            value_epoch: self.value_epoch,
            members_epoch: self.members_epoch,
            completion_order_cache: std::sync::Mutex::new(None),
            blvs,
            value_fwds,
            // The clone re-leaked every stateful forwarder above; the cached
            // descriptor belongs to the source obarray, so the clone starts
            // unresolved.
            debug_on_next_call_fwd: std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()),
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
        #[cfg(test)]
        self.symbol_slot_read_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // `get` already folds presence (empty slots read as `None`).
        self.symbols.get(Self::slot_index(id))
    }

    #[cfg(test)]
    pub(crate) fn reset_symbol_slot_read_count(&self) {
        self.symbol_slot_read_count
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(crate) fn symbol_slot_read_count(&self) -> usize {
        self.symbol_slot_read_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    #[inline(always)]
    fn slot_mut(&mut self, id: SymId) -> Option<&mut LispSymbol> {
        self.symbols.get_mut(Self::slot_index(id))
    }

    fn ensure_slot(&mut self, id: SymId) -> &mut LispSymbol {
        let idx = Self::slot_index(id);
        let slot = self.symbols.ensure(idx);
        if !slot.is_present() {
            Self::publish_fresh_slot(slot, id);
        }
        slot
    }

    /// Cold miss path of [`Obarray::ensure_slot`], outlined so the per-store
    /// hit path (bounds check + index + presence check) stays a handful of
    /// instructions — the write side now has the same shape as the read side
    /// (`slot_mut`). Fresh fill (None -> Some): publish arms-then-name via a
    /// terminal `Release` store so the concurrent GC obarray scan never reads
    /// a half-written slot's arms (see [`LispSymbol::publish_fill`]). Matches
    /// the old `get_or_insert_with(|| LispSymbol::new(id))` semantics — only
    /// an empty slot is written.
    #[cold]
    #[inline(never)]
    fn publish_fresh_slot(slot: &mut LispSymbol, id: SymId) {
        slot.publish_fill(LispSymbol::new(id));
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
            let Some(sym) = self.symbols.get(idx) else {
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
        // Fast path: already a member. Read-only — no ensure_slot, no growth
        // machinery — so the per-store caller (set_symbol_value_id ->
        // ensure_global_member_if_canonical) pays one presence-checked slot
        // read in steady state. Mirrors GNU, where obarray membership is a
        // read-time event (lread.c intern) that set_internal never re-checks.
        // The predicate must stay slot()-based (presence-checked): slots can
        // exist unmarked via ensure_symbol_id/function-cell paths, and those
        // must still fall through to the marking slow path below.
        if self.slot(id).is_some_and(|s| s.interned_global) {
            return;
        }
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
                if unsafe { sym.val.plain }.is_unbound() {
                    let kw = Value::keyword_id(id);
                    sym.flags.set_redirect(SymbolRedirect::Plainval);
                    sym.val = SymbolVal { plain: kw };
                }
            }
            true
        };
        if added {
            self.global_member_count += 1;
            self.members_epoch += 1;
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
        self.members_epoch += 1;
        true
    }

    fn ensure_global_member_if_canonical(&mut self, id: SymId) {
        if Self::is_canonical_symbol_id(id) {
            self.mark_global_member(id);
        }
    }

    /// GNU's `oblookup` outcome: is this name a symbol *in this obarray*?
    ///
    /// `Fsnarf_documentation` asks it of every `etc/DOC` record and skips the
    /// ones that answer no (`if (SYMBOLP (sym))`, `src/doc.c:600`), which is
    /// why scanning the DOC file cannot add symbols to the obarray.
    pub(crate) fn is_global_member(&self, id: SymId) -> bool {
        self.slot(id).is_some_and(|sym| sym.interned_global)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
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
            #[cfg(test)]
            symbol_slot_read_count: std::sync::atomic::AtomicUsize::new(0),
            global_member_count: 0,
            function_epoch: 0,
            value_epoch: 0,
            members_epoch: 0,
            completion_order_cache: std::sync::Mutex::new(None),
            blvs: Vec::new(),
            value_fwds: Vec::new(),
            debug_on_next_call_fwd: std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()),
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

    /// Intern a symbol from a Lisp string OBJECT, which becomes the symbol's
    /// name when this call creates it -- GNU `intern`. Use this rather than
    /// [`Self::intern_lisp_string`] whenever the name came from Lisp, so
    /// `symbol-name` gives that object back with its text properties.
    pub fn intern_lisp_value(&mut self, name_value: crate::tagged::value::TaggedValue) -> SymId {
        let id = crate::emacs_core::intern::intern_lisp_value(name_value);
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
                    for key_value in table.key_snapshots().copied() {
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
    ///
    /// **This is not GNU's `Vfoo`.** For a symbol some buffer has localised it
    /// answers the BLV *defcell*, and for a `DEFVAR_PER_BUFFER` name it
    /// answers `None`; see [`BufferlessValue`] for why, and
    /// [`Self::value_in_buffer`] for the reader that does mirror GNU's C.
    pub fn symbol_value(&self, name: &str) -> Option<&Value> {
        self.symbol_value_id(intern(name))
    }

    /// GNU's `Vfoo` / `foo` / `BVAR (current_buffer, foo)` -- the one spelling
    /// for "what the C code reads here", given the buffer that is current.
    ///
    /// GNU needs no such helper because it has no choice to make: the swap-in
    /// has already put `current_buffer`'s binding in the cell the C code
    /// dereferences (`src/data.c:1573-1603`), and a `DEFVAR_PER_BUFFER` name
    /// is only ever spelled `BVAR (current_buffer, ...)`. This port keeps the
    /// two places apart, so a Rust site has to name the buffer -- and the
    /// sites that could not were ledger 191's class.
    ///
    /// A `struct buffer` slot wins over the obarray unconditionally: for an
    /// always-local slot it *is* the value, and for a conditional slot GNU's
    /// `set-default` propagation leaves the live default in that same slot, so
    /// reading it is right in both cases. `set_default_internal`'s
    /// `BUFFER_OBJFWDP` arm calls `set_per_buffer_default` (`src/buffer.h:1627`)
    /// and then walks `FOR_EACH_LIVE_BUFFER` writing the new default into every
    /// buffer whose `PER_BUFFER_VALUE_P` is clear (`src/data.c:2087-2114`).
    ///
    /// `indent::dynamic_buffer_or_global_symbol_value` is the older, identical
    /// reader; it lives in a file ledger 195 owns, so collapsing the two is
    /// owed rather than done here (ledger 196).
    pub fn value_in_buffer(
        &self,
        buf: Option<&crate::buffer::Buffer>,
        name: &str,
    ) -> Option<Value> {
        self.value_in_buffer_id(buf, intern(name))
    }

    /// [`Self::value_in_buffer`] by identity.
    ///
    /// The `local_var_alist` lookup is gated on [`Self::is_localized`]: a
    /// symbol no buffer has ever localised can have no alist entry (every
    /// insertion path marks it `Localized` first), so the walk would only ever
    /// answer `None`. That keeps this reader roughly the cost of
    /// [`Self::symbol_value`] on the overwhelmingly common global path, which
    /// matters where a caller reads a dozen names at once -- the `print-*`
    /// family, for one.
    pub fn value_in_buffer_id(
        &self,
        buf: Option<&crate::buffer::Buffer>,
        id: SymId,
    ) -> Option<Value> {
        if let Some(buf) = buf {
            // A `struct buffer` slot is read UNCONDITIONALLY, including a
            // conditional slot whose local-flags bit is clear: GNU's
            // `set-default` propagation leaves the live default in that same
            // slot, so it is the right answer in both cases, where
            // `get_buffer_local` would answer `None` and lose it.
            if let Some(info) = crate::buffer::buffer::lookup_buffer_slot_by_sym_id(id) {
                return Some(buf.slots[info.offset.index()]);
            }
            if let Some(value) = buf.get_buffer_local_by_sym_id_gated(id, self.is_localized(id)) {
                return Some(value);
            }
        }
        self.symbol_value_id_copied(id)
    }

    /// A deliberate buffer-less read, with the disagreement with GNU named.
    ///
    /// Use this where a site genuinely has no buffer and the ledger row that
    /// licensed it can be cited at the `match`; use [`Self::value_in_buffer`]
    /// everywhere else.
    pub fn value_without_buffer(&self, name: &str) -> BufferlessValue {
        self.value_without_buffer_id(intern(name))
    }

    /// [`Self::value_without_buffer`] by identity.
    pub fn value_without_buffer_id(&self, id: SymId) -> BufferlessValue {
        if crate::buffer::buffer::lookup_buffer_slot_by_sym_id(id).is_some() {
            return BufferlessValue::PerBufferSlot;
        }
        let localized = self
            .slot(self.resolve_alias_for_read(id))
            .is_some_and(|sym| sym.flags.redirect() == SymbolRedirect::Localized);
        match self.symbol_value_id_copied(id) {
            None => BufferlessValue::Void,
            Some(value) if localized => BufferlessValue::DefaultOfLocalized(value),
            Some(value) => BufferlessValue::Global(value),
        }
    }

    /// Follow a `Varalias` chain to the symbol that owns the value cell, for a
    /// read. Mirrors the walk [`Self::symbol_value_id_copied`] performs, split
    /// out so the redirect of the *target* can be inspected.
    fn resolve_alias_for_read(&self, id: SymId) -> SymId {
        let mut current = id;
        for _ in 0..50 {
            let Some(sym) = self.slot(current) else {
                return current;
            };
            if sym.flags.redirect() != SymbolRedirect::Varalias {
                return current;
            }
            current = unsafe { sym.val.alias };
        }
        current
    }

    /// Get the value cell of a symbol by identity.
    /// Follows alias chains (with cycle detection, max 50 hops).
    ///
    /// Phase F: reads from the redirect union (`val`) rather than the
    /// legacy `value` enum field.
    #[inline(always)]
    pub fn symbol_value_id_copied(&self, id: SymId) -> Option<Value> {
        let sym = match self.symbols.get(Self::slot_index(id)) {
            Some(sym) => sym,
            _ => return None,
        };
        match sym.flags.redirect() {
            SymbolRedirect::Plainval => {
                // Safety: redirect=Plainval guarantees val.plain is
                // the live value field. UNBOUND sentinel = unbound.
                let value = unsafe { sym.val.plain };
                if value.is_unbound() {
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
                if value.is_unbound() {
                    None
                } else {
                    Some(value)
                }
            }
            SymbolRedirect::Forwarded => {
                let fwd = unsafe { &*sym.val.fwd };
                fwd.load()
            }
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
                Some(sym) => sym,
                _ => return None,
            };
            match sym.flags.redirect() {
                SymbolRedirect::Plainval => {
                    // Safety: redirect=Plainval guarantees val.plain is
                    // the live value field. UNBOUND sentinel = unbound.
                    let v = unsafe { sym.val.plain };
                    if v.is_unbound() {
                        return None;
                    }
                    return Some(v);
                }
                SymbolRedirect::Varalias => {
                    current = unsafe { sym.val.alias };
                }
                SymbolRedirect::Localized => {
                    let value = self.blv(current)?.defcell.cons_cdr();
                    if value.is_unbound() {
                        return None;
                    }
                    return Some(value);
                }
                SymbolRedirect::Forwarded => {
                    let fwd = unsafe { &*sym.val.fwd };
                    return fwd.load();
                }
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
                Some(sym) => sym,
                _ => return None,
            };
            match sym.flags.redirect() {
                SymbolRedirect::Plainval => {
                    // Safety: redirect=Plainval guarantees val.plain is
                    // the live value field. UNBOUND sentinel = unbound.
                    let v = unsafe { &sym.val.plain };
                    if v.is_unbound() {
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
                        if cdr_ref.is_unbound() {
                            None
                        } else {
                            Some(cdr_ref)
                        }
                    });
                }
                SymbolRedirect::Forwarded => {
                    // Safety: `install_*fwd` leaks every descriptor, so the
                    // borrow the accessor hands back really is `'static`.
                    let fwd: &'static crate::emacs_core::forward::LispFwd =
                        unsafe { &*sym.val.fwd };
                    return fwd.load_ref();
                }
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

    /// Declare and initialize a Lisp-visible variable with GNU `DEFVAR_*`
    /// binding semantics.
    pub fn define_lisp_variable(
        &mut self,
        name: &str,
        value: Value,
        locality: LispVariableLocality,
    ) {
        self.set_symbol_value(name, value);
        self.make_special(name);
        match locality {
            LispVariableLocality::Global => {}
            LispVariableLocality::BufferLocalIfSet => self.make_buffer_local(name, true),
        }
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
        if let Some(existing) = self.slot(target)
            && existing.flags.redirect() == SymbolRedirect::Localized
        {
            return unsafe { existing.val.blv };
        }
        // GNU `make_blv` keeps the forwarder when the symbol it localizes was
        // SYMBOL_FORWARDED (`src/data.c:2112-2140`, `blv->fwd = valcontents`),
        // which is why a per-buffer binding of a `DEFVAR_INT` variable is
        // still an integer slot and a per-buffer `DEFVAR_BOOL` still reads
        // back `t`. Dropping it here would disarm the type rule for the rest
        // of the session the first time any buffer made a local binding.
        let forwarder: Option<&'static crate::emacs_core::forward::LispFwd> = self
            .slot(target)
            .filter(|sym| sym.flags.redirect() == SymbolRedirect::Forwarded)
            .map(|sym| unsafe { &*sym.val.fwd });
        // Build defcell = (sym . default). The same cons doubles as
        // valcell until per-buffer bindings are swapped in.
        let defcell = Value::cons(Value::from_sym_id(target), default);
        let blv = Box::new(LispBufferLocalValue {
            local_if_set: false,
            found: false,
            fwd: forwarder,
            where_buf: Value::NIL,
            defcell,
            valcell: defcell,
            // 0 < the global epoch's initial 1: a fresh BLV never
            // fast-path-hits before its first swap_in records reality.
            alist_epoch: 0,
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

    /// The forward descriptor installed on `id`, if the symbol is
    /// `SYMBOL_FORWARDED`. Mirrors GNU `SYMBOL_FWD` (`src/lisp.h:1082`).
    pub fn forwarder(&self, id: SymId) -> Option<&'static crate::emacs_core::forward::LispFwd> {
        let sym = self.slot(id)?;
        if sym.flags.redirect() != SymbolRedirect::Forwarded {
            return None;
        }
        // Safety: `install_*fwd` is the only writer of this arm and it always
        // stores a `Box::leak`ed descriptor.
        Some(unsafe { &*sym.val.fwd })
    }

    /// Which `Lisp_Fwd` variant a symbol forwards through, if it forwards.
    ///
    /// GNU spells this as a chain of `BUFFER_OBJFWDP` / `KBOARD_OBJFWDP`
    /// predicates over `SYMBOL_FWD (sym)`; handing back the closed
    /// [`LispFwdType`](crate::emacs_core::forward::LispFwdType) instead means a
    /// caller that cares about one variant has to say what it does about the
    /// others.
    pub fn forward_type(&self, id: SymId) -> Option<crate::emacs_core::forward::LispFwdType> {
        self.forwarder(id).map(|fwd| fwd.ty)
    }

    /// The `Lisp_Boolfwd` cell behind a `DEFVAR_BOOL` symbol -- GNU's `bool *`,
    /// which C code reads directly (`debug_on_next_call` is
    /// `globals.f_debug_on_next_call`, `src/globals.h:1170-1171`) rather than
    /// through a symbol lookup.
    ///
    /// Both redirects that can own the descriptor are handled: `Forwarded`
    /// normally, and the BLV after `make_blv` copied it there
    /// (`src/data.c:2112-2140`).  In the localized case GNU's swap-in leaves
    /// the current buffer's value in that same cell, so the cell is still the
    /// right thing to read.
    pub fn bool_forwarder(
        &self,
        id: SymId,
    ) -> Option<&'static crate::emacs_core::forward::LispBoolFwd> {
        let sym = self.slot(id)?;
        match sym.flags.redirect() {
            // Safety: `install_*fwd` is the only writer of this arm and always
            // stores a `Box::leak`ed descriptor (see `Obarray::forwarder`).
            SymbolRedirect::Forwarded => unsafe { &*sym.val.fwd }.as_bool_fwd(),
            SymbolRedirect::Localized => self.blv(id)?.fwd?.as_bool_fwd(),
            _ => None,
        }
    }

    /// [`Self::bool_forwarder`] for `debug-on-next-call`, memoized -- the read
    /// GNU spells `globals.f_debug_on_next_call`: one load, no symbol lookup.
    /// The bytecode `Op::Call` arm performs this test on every call
    /// (`src/bytecode.c:798`), which is why it cannot afford the slot walk.
    #[inline]
    pub(crate) fn debug_on_next_call_bool_fwd(
        &self,
        id: SymId,
    ) -> Option<&'static crate::emacs_core::forward::LispBoolFwd> {
        let cached = self
            .debug_on_next_call_fwd
            .load(std::sync::atomic::Ordering::Relaxed);
        if !cached.is_null() {
            // Safety: the only store is the slow path below, which puts a
            // `Box::leak`ed descriptor here, and no path replaces a resolved
            // descriptor for a live obarray (see the field's invariant note).
            return Some(unsafe { &*cached });
        }
        self.debug_on_next_call_bool_fwd_slow(id)
    }

    #[cold]
    #[inline(never)]
    fn debug_on_next_call_bool_fwd_slow(
        &self,
        id: SymId,
    ) -> Option<&'static crate::emacs_core::forward::LispBoolFwd> {
        let fwd = self.bool_forwarder(id)?;
        self.debug_on_next_call_fwd.store(
            fwd as *const crate::emacs_core::forward::LispBoolFwd as *mut _,
            std::sync::atomic::Ordering::Relaxed,
        );
        Some(fwd)
    }

    /// The `Lisp_Intfwd` cell behind a `DEFVAR_INT` symbol -- GNU's
    /// `intmax_t *`, which C code reads and writes as a plain global.
    ///
    /// `num_nonmacro_input_events` (`src/keyboard.c:13903`) and
    /// `when_entered_debugger` (`src/eval.c:4554`) are both this, and both are
    /// read by C in the same expression Lisp can `setq` (`src/eval.c:2212`) --
    /// so the counter and the Lisp variable have to be one slot, not two.
    /// Same two-redirect handling as [`Obarray::bool_forwarder`].
    pub fn int_forwarder(
        &self,
        id: SymId,
    ) -> Option<&'static crate::emacs_core::forward::LispIntFwd> {
        let sym = self.slot(id)?;
        match sym.flags.redirect() {
            // Safety: `install_*fwd` is the only writer of this arm and always
            // stores a `Box::leak`ed descriptor (see `Obarray::forwarder`).
            SymbolRedirect::Forwarded => unsafe { &*sym.val.fwd }.as_int_fwd(),
            SymbolRedirect::Localized => self.blv(id)?.fwd?.as_int_fwd(),
            _ => None,
        }
    }

    /// Set the `local_if_set` flag on a LOCALIZED symbol's BLV. Used
    /// by `make-variable-buffer-local` (Phase 6) which differs from
    /// `make-local-variable` only in this flag. Phase 4 exposes the
    /// helper so the LOCALIZED tests can flip it directly.
    pub fn set_blv_local_if_set(&mut self, id: SymId, local_if_set: bool) {
        let target = self.resolve_alias_for_write(id);
        if let Some(sym) = self.slot(target)
            && sym.flags.redirect() == SymbolRedirect::Localized
        {
            let blv = unsafe { &mut *sym.val.blv };
            blv.local_if_set = local_if_set;
        }
    }

    /// Read a LOCALIZED symbol's BLV (immutable borrow). Returns
    /// `None` if the symbol is not LOCALIZED.
    /// Whether `id`'s redirect is `Localized` — i.e. the symbol has ever been
    /// made buffer-local somewhere, so a per-buffer binding *could* exist in a
    /// buffer's `local_var_alist`. A `Plainval`/global symbol is never inserted
    /// into any `local_var_alist` (every insertion path first marks the symbol
    /// `Localized` via `make_symbol_localized`), so display/VM variable
    /// resolution can skip the O(n) alist walk for non-localized symbols. O(1).
    #[inline]
    pub fn is_localized(&self, id: SymId) -> bool {
        self.slot(id)
            .is_some_and(|sym| sym.flags.redirect() == SymbolRedirect::Localized)
    }

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
        let blv_ptr = self.blv_ptr(id)?;
        let epoch = blv_alist_epoch();
        // SAFETY: the BLV record is a separate heap allocation reached only
        // through the symbol's raw pointer; the evaluator thread is its only
        // writer, and the two `Value` slots the GC thread may read
        // concurrently are written with release stores -- the same contract
        // `swap_in_blv` honours through `&mut`.  No `&LispBufferLocalValue`
        // is held across the writes.
        unsafe {
            // Same-buffer fast path -- the SAME soundness guard as
            // `find_symbol_value_in_buffer`'s GNU `swap_in_symval_forwarding`
            // early-out: trust the cached `valcell` iff it was loaded for THIS
            // buffer and no structural `local_var_alist` mutation happened
            // since (`alist_epoch` vs the global epoch).  Every value write
            // updates that shared cons's cdr in place, so an epoch-valid cell
            // carries the identical live value.
            if (*blv_ptr).alist_epoch == epoch
                && crate::emacs_core::value::eq_value(&(*blv_ptr).where_buf, &target_buf)
            {
                return Some((*blv_ptr).valcell.cons_cdr());
            }
            // Miss: GNU `find_symbol_value` swaps the binding in
            // (`swap_in_symval_forwarding`) so the NEXT read is a cell read.
            // This path used to scan and return without reloading the cache,
            // so after any epoch bump every read of the symbol paid the
            // whole-alist `assq` (~1K Ir on a 65-local buffer) until some
            // write path happened to swap it in -- `parse-sexp-ignore-comments`
            // read per `scan-sexps` in indent-region was the visible case.
            let key = Value::from_sym_id(id);
            let found_cell = assq(key, target_alist);
            let found = !found_cell.is_nil();
            let valcell = if found {
                found_cell
            } else {
                (*blv_ptr).defcell
            };
            store_value_atomic(&mut (*blv_ptr).where_buf, target_buf);
            (*blv_ptr).found = found;
            store_value_atomic(&mut (*blv_ptr).valcell, valcell);
            (*blv_ptr).alist_epoch = epoch;
            Some(valcell.cons_cdr())
        }
    }

    /// Raw pointer to a LOCALIZED symbol's BLV record (see `read_localized`
    /// for the aliasing contract), `None` for any other redirect.
    fn blv_ptr(&self, id: SymId) -> Option<*mut LispBufferLocalValue> {
        let sym = self.slot(id)?;
        if sym.flags.redirect() != SymbolRedirect::Localized {
            return None;
        }
        Some(unsafe { sym.val.blv })
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
        // GNU `blv_found`: a cache loaded for this buffer at the current
        // epoch already knows whether the cell is per-buffer (the same
        // contract `read_localized` trusts).  `specbind` and `unbind_to`
        // asked this right after `find_symbol_value` had swapped the cache
        // in, so every buffer-local `let` paid a second whole-alist assq.
        if blv.alist_epoch == blv_alist_epoch()
            && crate::emacs_core::value::eq_value(&blv.where_buf, &target_buf)
        {
            return blv.found;
        }
        // Otherwise the alist is authoritative (see `read_localized`).
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

    /// Install a GNU `Lisp_Boolfwd`-equivalent descriptor on a symbol.
    /// Every non-nil write becomes native `true`, and reads expose only `t`
    /// or `nil`, matching `do_symval_forwarding` / `store_symval_forwarding`
    /// in GNU `src/data.c`.
    pub fn install_boolfwd(
        &mut self,
        id: SymId,
        fwd: &'static crate::emacs_core::forward::LispBoolFwd,
    ) {
        let _seq_guard = self.seqlock_guard(id);
        let sym = self.ensure_symbol_id(id);
        if sym.flags.redirect() == SymbolRedirect::Plainval {
            crate::tagged::gc::note_root_overwrite(unsafe { sym.val.plain });
        }
        sym.flags.set_redirect(SymbolRedirect::Forwarded);
        sym.flags.set_declared_special(true);
        sym.val = SymbolVal {
            fwd: fwd as *const crate::emacs_core::forward::LispBoolFwd
                as *const crate::emacs_core::forward::LispFwd,
        };
    }

    /// Define a global Lisp variable with GNU `DEFVAR_BOOL` storage.
    ///
    /// The initial value is a `bool` rather than a [`Value`] for the same
    /// reason GNU's is a `bool *`: there is no way to register a `DEFVAR_BOOL`
    /// variable seeded with something that is not a Boolean.
    ///
    /// Registration has a second effect in GNU, inside `defvar_bool` itself
    /// (`src/lread.c:5254-5262`): the symbol is consed onto `byte-boolean-vars`.
    /// The byte optimizer reads that list to decide it may NOT fold a
    /// `varset X; varref X` pair back into the value it stored -- "what we put
    /// in might not be what we get out" (`lisp/emacs-lisp/byte-opt.el:2285-2300`)
    /// -- which is the coercion rule reaching the compiler.  Doing it here
    /// rather than at the call site keeps it a property of the declaration.
    ///
    /// Whether that cons survives is not `defvar_bool`'s decision:
    /// `syms_of_lread` sets the list back to nil when it declares it
    /// (`src/lread.c:5774`), erasing every registration `main` performed
    /// earlier.  [`ByteBooleanVars`] is that fact, and it is a required
    /// argument because the alternative is each caller re-deriving GNU's
    /// startup order.
    pub fn define_bool_variable(
        &mut self,
        name: &str,
        initial: bool,
        byte_boolean_vars: ByteBooleanVars,
    ) {
        let id = intern(name);
        self.mark_global_member(id);
        // Idempotent, like re-running a `DEFVAR_BOOL` would be: installing a
        // second descriptor would leave the first one still reachable from a
        // BLV, and would cons the symbol on twice.
        if self.blv(id).is_some() {
            // Lisp has already localized it, so `make_blv` moved the
            // descriptor into the BLV (`src/data.c:2112-2140`); flipping the
            // redirect back to `Forwarded` here would orphan every per-buffer
            // binding.  Declare into the BLV instead.
            self.reattach_localized_forwarder(
                id,
                crate::emacs_core::pdump::types::DumpLocalizedForwarder::Bool,
            );
            self.set_symbol_value_id(id, if initial { Value::T } else { Value::NIL });
        } else {
            match self.forwarder(id).and_then(|fwd| fwd.as_bool_fwd()) {
                Some(existing) => existing.set(initial),
                None => {
                    let fwd = crate::emacs_core::forward::alloc_boolfwd(initial);
                    self.install_boolfwd(id, fwd);
                }
            }
        }

        if byte_boolean_vars == ByteBooleanVars::ErasedByLreadInit {
            return;
        }
        let list_id = intern("byte-boolean-vars");
        let current = self.find_symbol_value(list_id).unwrap_or(Value::NIL);
        let symbol = Value::from_sym_id(id);
        let mut tail = current;
        while tail.is_cons() {
            if super::value::eq_value(&tail.cons_car(), &symbol) {
                return;
            }
            tail = tail.cons_cdr();
        }
        let updated = Value::cons(symbol, current);
        self.set_symbol_value_id(list_id, updated);
        self.make_special_id(list_id);
    }

    /// Give a localized symbol back the descriptor `make_blv` copied into its
    /// BLV (`src/data.c:2112-2140`).
    ///
    /// A no-op unless the symbol is `Localized` with no forwarder, which is
    /// only reachable after loading a portable dump: the descriptor is a
    /// process-lifetime pointer, so a localized symbol's image carries its
    /// default value plus the KIND of forwarder to rebuild
    /// ([`DumpLocalizedForwarder`](crate::emacs_core::pdump::types::DumpLocalizedForwarder)),
    /// never the pointer.  The new descriptor is seeded from that restored
    /// default rather than from a declaration's initial value, so a variable
    /// the bootstrap changed keeps what the dump recorded.
    ///
    /// The default is then canonicalised the way `do_symval_forwarding` would
    /// have rebuilt it on the way out (`src/data.c:1337-1360`): `t`/`nil` for a
    /// Boolean slot, and for an integer slot a value that failed
    /// `LispInteger::check` is impossible to have stored, so a corrupt image
    /// falls back to the slot's zero rather than smuggling a non-integer in.
    pub fn reattach_localized_forwarder(
        &mut self,
        id: SymId,
        kind: crate::emacs_core::pdump::types::DumpLocalizedForwarder,
    ) {
        use crate::emacs_core::pdump::types::DumpLocalizedForwarder as Kind;
        let Some(blv) = self.blv_mut(id) else { return };
        if blv.fwd.is_some() {
            return;
        }
        let restored = blv.defcell.cons_cdr();
        let (fwd, canonical) = match kind {
            Kind::Bool => {
                let flag = restored.is_truthy();
                let fwd = crate::emacs_core::forward::alloc_boolfwd(flag);
                let fwd = unsafe {
                    &*(fwd as *const crate::emacs_core::forward::LispBoolFwd
                        as *const crate::emacs_core::forward::LispFwd)
                };
                (fwd, if flag { Value::T } else { Value::NIL })
            }
            Kind::Int => {
                let integer = crate::emacs_core::forward::LispInteger::check(restored)
                    .unwrap_or_else(|_| crate::emacs_core::forward::LispInteger::from_i64(0));
                let fwd = crate::emacs_core::forward::alloc_intfwd(integer);
                let fwd = unsafe {
                    &*(fwd as *const crate::emacs_core::forward::LispIntFwd
                        as *const crate::emacs_core::forward::LispFwd)
                };
                (fwd, integer.value())
            }
            // A `Lisp_Fwd_Obj` accepts anything and canonicalises nothing, so
            // the BLV's default comes back unchanged; the descriptor exists
            // for the redirect tag, which is what refuses an unbind through
            // `blv->fwd` (`src/data.c:1723-1727`).
            Kind::Obj => {
                let fwd = crate::emacs_core::forward::alloc_objfwd(restored);
                let fwd = unsafe {
                    &*(fwd as *const crate::emacs_core::forward::LispObjFwd
                        as *const crate::emacs_core::forward::LispFwd)
                };
                (fwd, restored)
            }
            Kind::Kboard => {
                let fwd = crate::emacs_core::forward::alloc_kboard_objfwd(restored);
                let fwd = unsafe {
                    &*(fwd as *const crate::emacs_core::forward::LispKboardObjFwd
                        as *const crate::emacs_core::forward::LispFwd)
                };
                (fwd, restored)
            }
        };
        blv.fwd = Some(fwd);
        blv.defcell.set_cdr(canonical);
        if super::value::eq_value(&blv.valcell, &blv.defcell) {
            blv.valcell.set_cdr(canonical);
        }
        // The descriptor just allocated owns the value it was seeded with, so
        // it is a root like every other value-owning forwarder.  `blv` is
        // dropped above; `register_value_fwd` needs `&mut self`.
        self.register_value_fwd(fwd);
    }

    /// Install a GNU `Lisp_Intfwd`-equivalent descriptor on a symbol
    /// (`src/data.c:defvar_int`).  Every subsequent assignment has to satisfy
    /// `CHECK_INTEGER` because the slot has nowhere else to put the value.
    pub fn install_intfwd(
        &mut self,
        id: SymId,
        fwd: &'static crate::emacs_core::forward::LispIntFwd,
    ) {
        let _seq_guard = self.seqlock_guard(id);
        let sym = self.ensure_symbol_id(id);
        if sym.flags.redirect() == SymbolRedirect::Plainval {
            crate::tagged::gc::note_root_overwrite(unsafe { sym.val.plain });
        }
        sym.flags.set_redirect(SymbolRedirect::Forwarded);
        sym.flags.set_declared_special(true);
        sym.val = SymbolVal {
            fwd: fwd as *const crate::emacs_core::forward::LispIntFwd
                as *const crate::emacs_core::forward::LispFwd,
        };
        self.register_value_fwd(unsafe {
            &*(fwd as *const crate::emacs_core::forward::LispIntFwd
                as *const crate::emacs_core::forward::LispFwd)
        });
    }

    /// Record a descriptor that owns a Lisp value as a GC root.
    ///
    /// The predicate lives in `LispFwd::owned_value`, so a new forward variant
    /// cannot be added and silently left untraced by a registry that forgot
    /// about it.
    fn register_value_fwd(&mut self, fwd: &'static crate::emacs_core::forward::LispFwd) {
        if fwd.owned_value().is_some() {
            self.value_fwds.push(fwd);
        }
    }

    /// Install a GNU `Lisp_Objfwd`-equivalent descriptor on a symbol
    /// (`src/lread.c:5270-5277`, `defvar_lisp_nopro`).  The symbol becomes
    /// `SYMBOL_FORWARDED`, which is what every refusal in GNU's redirect
    /// switch keys on -- the unbind refusal in `set_internal`
    /// (`src/data.c:1802-1809`) and the alias refusal in `Fdefvaralias`
    /// (`src/eval.c:665-668`) both signal from the arm without ever reading
    /// the value the descriptor points at.
    pub fn install_objfwd(
        &mut self,
        id: SymId,
        fwd: &'static crate::emacs_core::forward::LispObjFwd,
    ) {
        let _seq_guard = self.seqlock_guard(id);
        let sym = self.ensure_symbol_id(id);
        if sym.flags.redirect() == SymbolRedirect::Plainval {
            crate::tagged::gc::note_root_overwrite(unsafe { sym.val.plain });
        }
        sym.flags.set_redirect(SymbolRedirect::Forwarded);
        sym.flags.set_declared_special(true);
        sym.val = SymbolVal {
            fwd: fwd as *const crate::emacs_core::forward::LispObjFwd
                as *const crate::emacs_core::forward::LispFwd,
        };
        self.register_value_fwd(unsafe {
            &*(fwd as *const crate::emacs_core::forward::LispObjFwd
                as *const crate::emacs_core::forward::LispFwd)
        });
    }

    /// Install a GNU `Lisp_Kboard_Objfwd`-equivalent descriptor on a symbol
    /// (`src/lread.c:5291-5298`, `defvar_kboard`).
    pub fn install_kboard_objfwd(
        &mut self,
        id: SymId,
        fwd: &'static crate::emacs_core::forward::LispKboardObjFwd,
    ) {
        let _seq_guard = self.seqlock_guard(id);
        let sym = self.ensure_symbol_id(id);
        if sym.flags.redirect() == SymbolRedirect::Plainval {
            crate::tagged::gc::note_root_overwrite(unsafe { sym.val.plain });
        }
        sym.flags.set_redirect(SymbolRedirect::Forwarded);
        sym.flags.set_declared_special(true);
        sym.val = SymbolVal {
            fwd: fwd as *const crate::emacs_core::forward::LispKboardObjFwd
                as *const crate::emacs_core::forward::LispFwd,
        };
        self.register_value_fwd(unsafe {
            &*(fwd as *const crate::emacs_core::forward::LispKboardObjFwd
                as *const crate::emacs_core::forward::LispFwd)
        });
    }

    /// Define a global Lisp variable with GNU `DEFVAR_INT` storage.
    ///
    /// The initial value is an `i64` rather than a [`Value`] for the same
    /// reason GNU's is an `intmax_t`: there is no way to register a
    /// `DEFVAR_INT` variable seeded with something that is not an integer.
    pub fn define_int_variable(&mut self, name: &str, initial: i64) {
        let id = intern(name);
        self.mark_global_member(id);
        let value = crate::emacs_core::forward::LispInteger::from_i64(initial);
        // Idempotent, like re-running a `DEFVAR_INT` would be: several
        // bootstrap tables register the same variable, and installing a second
        // descriptor would leave the first one still reachable from a BLV.
        if self.blv(id).is_some() {
            // Already localized, so `make_blv` moved the descriptor into the
            // BLV (`src/data.c:2112-2140`); flipping the redirect back to
            // `Forwarded` here would orphan every per-buffer binding.  Declare
            // into the BLV instead -- the case `display-line-numbers-offset`
            // reaches, being both `DEFVAR_INT` and `Fmake_variable_buffer_local`
            // (`src/xdisp.c:38999-39005`).
            self.reattach_localized_forwarder(
                id,
                crate::emacs_core::pdump::types::DumpLocalizedForwarder::Int,
            );
            self.set_symbol_value_id(id, value.value());
            return;
        }
        if let Some(existing) = self.forwarder(id).and_then(|fwd| fwd.as_int_fwd()) {
            existing.set(value);
            return;
        }
        let fwd = crate::emacs_core::forward::alloc_intfwd(value);
        self.install_intfwd(id, fwd);
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
                    if v.is_unbound() {
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
                    if let Some(value) = fwd.load() {
                        return Some(value);
                    }
                    if fwd.ty == LispFwdType::BufferObj {
                        let buf_fwd = unsafe { &*(fwd as *const _ as *const LispBufferObjFwd) };
                        return Some(buf_fwd.default);
                    }
                    // Obj / KboardObj forwarders are not installed here.
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
    #[allow(clippy::too_many_arguments)] // keeps independently borrowed symbol/buffer state allocation-free
    pub fn find_symbol_value_in_buffer(
        &mut self,
        id: SymId,
        _current_buffer_id: Option<crate::buffer::BufferId>,
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
                    // Same-buffer fast path (GNU `swap_in_symval_forwarding`
                    // early-outs when `blv->where` is already the current
                    // buffer): trust the cached `valcell` iff the cache was
                    // loaded for THIS buffer and no structural alist
                    // mutation happened since (`alist_epoch`). Every value
                    // write goes through `valcell.set_cdr` on the shared
                    // cons, so an epoch-valid cell always carries the live
                    // value. This removes the per-read whole-alist assq that
                    // dominates localized VarRef cost (Task 4: 58% of
                    // session VarRefs, 60.8ns -> ~cons_cdr).
                    if let Some(blv) = self.blv(current)
                        && blv.alist_epoch == blv_alist_epoch()
                        && crate::emacs_core::value::eq_value(&blv.where_buf, &current_buffer_value)
                    {
                        return Some(blv.valcell.cons_cdr());
                    }
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
                                if bit_set
                                    && let Some(slots) = current_buffer_slots
                                    && off < slots.len()
                                {
                                    return Some(slots[off]);
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
                        // `Int`, `Bool`, `Obj` and `KboardObj` keep their
                        // storage in the descriptor, so none of the buffer
                        // context this function was handed applies to them;
                        // the buffer-free walk reads them through
                        // `do_symval_forwarding` and is the whole answer.
                        LispFwdType::Int
                        | LispFwdType::Bool
                        | LispFwdType::Obj
                        | LispFwdType::KboardObj => {
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
    ) -> SetInternalAlist {
        let mut new_alist = target_alist;
        let blv = match self.blv_mut(sym_id) {
            Some(blv) => blv,
            None => return SetInternalAlist(new_alist),
        };

        // Step 1: select the binding cell for this target buffer.
        // GNU's BLV cache is kept coherent with `local_var_alist`, so
        // `set_internal` can usually trust `blv->valcell` when `where`
        // already matches. Neomacs stores `local_var_alist` as the
        // authoritative binding list and some Lisp paths replace alist
        // entries without touching the BLV cache, so refresh from the
        // target alist before every LOCALIZED write.
        let key = Value::from_sym_id(sym_id);
        let epoch = blv_alist_epoch();
        // GNU `set_internal` (SYMBOL_LOCALIZED): `swap_in_symval_forwarding`
        // scans the alist only when `blv->where` is not this buffer; a cache
        // loaded for it at the current epoch yields the cell directly (nil
        // when `found` is false, so the auto-create decision below is the
        // same one the scan would reach).
        let mut cell = if blv.alist_epoch == epoch
            && crate::emacs_core::value::eq_value(&blv.where_buf, &target_buf)
        {
            if blv.found { blv.valcell } else { Value::NIL }
        } else {
            assq(key, new_alist)
        };
        store_value_atomic(&mut blv.where_buf, target_buf);
        blv.alist_epoch = epoch;
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
        SetInternalAlist(new_alist)
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
        // FORWARDED goes through the descriptor. Everything else becomes
        // Plainval.
        match sym.flags.redirect() {
            SymbolRedirect::Forwarded => {
                let fwd = unsafe { &*sym.val.fwd };
                // This is the storage-level entry point, below the evaluator,
                // so a refusal has no way to become a Lisp signal here; the
                // Lisp-visible check runs in `set_runtime_binding_in_state`.
                // Refusing to store is what GNU's longjmp out of
                // `store_symval_forwarding` leaves behind, so the slot keeps
                // its old value either way.
                match fwd.store(value) {
                    Ok(store) => {
                        fwd.commit(store);
                    }
                    Err(error) => {
                        debug_assert!(false, "forwarded slot refused an internal write: {error:?}")
                    }
                }
            }
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
        for sym in self.symbols.iter_mut().filter(|s| s.is_present()) {
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
        #[cfg(test)]
        FUNCTION_CELL_LOOKUP_COUNT.with(|count| count.set(count.get() + 1));
        match self.function_cell_snapshot(id) {
            FunctionCellSnapshot::Bound(function) => Some(function),
            FunctionCellSnapshot::ExplicitlyUnbound | FunctionCellSnapshot::Empty => None,
        }
    }

    /// Snapshot the complete function-cell state with one symbol-slot read.
    #[inline(always)]
    pub(crate) fn function_cell_snapshot(&self, id: SymId) -> FunctionCellSnapshot {
        let Some(symbol) = self.slot(id) else {
            return FunctionCellSnapshot::Empty;
        };
        if symbol.function_unbound {
            FunctionCellSnapshot::ExplicitlyUnbound
        } else if symbol.function.is_nil() {
            FunctionCellSnapshot::Empty
        } else {
            FunctionCellSnapshot::Bound(symbol.function)
        }
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
        // u64::MAX is RESERVED as the JIT/AOT spec DISARMED sentinel
        // (jit::compile::SPEC_EPOCH_DISARMED); a live epoch must never equal it or
        // a legitimately-armed spec slot would read as disarmed. Skip it on the
        // (astronomically unreachable) wrap.
        if self.function_epoch == u64::MAX {
            self.function_epoch = 0;
        }
    }

    /// A specific function `id` was redefined (cell write / fmakunbound): bump the
    /// epoch (the coarse "any binding may have changed" signal + JIT backstop).
    /// When JIT is enabled, also precisely evict the JIT cache entries of callers
    /// that INLINED `id`, so an unrelated redefinition no longer re-JITs every
    /// inlined function. Pure optimization layered on the epoch backstop — see
    /// jit::cache::evict_inline_dependents.
    fn note_function_redefined(&mut self, _id: SymId) {
        self.function_epoch = self.function_epoch.wrapping_add(1);
        // u64::MAX is RESERVED as the JIT/AOT spec DISARMED sentinel
        // (jit::compile::SPEC_EPOCH_DISARMED); a live epoch must never equal it or
        // a legitimately-armed spec slot would read as disarmed. Skip it on the
        // (astronomically unreachable) wrap.
        if self.function_epoch == u64::MAX {
            self.function_epoch = 0;
        }
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
        if let Some(sym) = self.slot_mut(id)
            && !sym.function.is_nil()
        {
            // SATB: retain the function cell's pre-image during a concurrent mark.
            crate::tagged::gc::note_root_overwrite(sym.function);
            store_value_atomic(&mut sym.function, Value::NIL);
            redefined = true;
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
        if let Some(sym) = self.slot_mut(target)
            && sym.flags.trapped_write() != SymbolTrappedWrite::NoWrite
        {
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
                    // A forwarded slot is never unbound, whatever it forwards
                    // to: GNU's C storage has no "unbound" representation for
                    // any `Lisp_Fwd` variant, which is the same fact that
                    // makes `set_internal` refuse `makunbound` from the arm
                    // above (`src/data.c:1802-1809`).  `Fboundp` never even
                    // reaches the descriptor -- its SYMBOL_FORWARDED arm is
                    // `valid = true;` with no inner switch
                    // (`src/data.c:733-736`).  Enumerating the variants here
                    // was how `DEFVAR_LISP` and `DEFVAR_KBOARD` came back
                    // unbound the moment they became forwarded (ledger 170).
                    return true;
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
        match self.symbol_plist_snapshot_id(symbol) {
            SymbolPlistSnapshot::NoEntries => None,
            SymbolPlistSnapshot::Entries(plist) => {
                crate::emacs_core::plist::plist_get(plist, &Value::from_sym_id(prop))
            }
        }
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

    /// Snapshot the states relevant to property lookup in one symbol-slot read.
    ///
    /// `setplist` accepts arbitrary Lisp objects.  A non-cons value therefore
    /// means the same thing as nil to GNU's `plist_get`, even though
    /// [`Self::symbol_plist_id`] must continue returning it verbatim.
    pub(crate) fn symbol_plist_snapshot_id(&self, id: SymId) -> SymbolPlistSnapshot {
        match self.slot(id).map(|symbol| symbol.plist) {
            Some(plist) if plist.is_cons() => SymbolPlistSnapshot::Entries(plist),
            _ => SymbolPlistSnapshot::NoEntries,
        }
    }

    /// Mark a symbol as special (dynamically bound).
    pub fn make_special(&mut self, name: &str) {
        let id = intern(name);
        self.mark_global_member(id);
        self.ensure_symbol_id(id).flags.set_declared_special(true);
    }

    /// Define a bound special variable in one semantic operation.
    ///
    /// GNU's `DEFVAR_LISP`, `DEFVAR_BOOL`, and related C registration macros
    /// both initialize the value cell and set `declared_special`.  Keeping
    /// those steps behind one Rust API prevents bootstrap call sites from
    /// constructing the invalid half-registered state where a variable is
    /// bound but lexical Lisp does not treat it as dynamically scoped.
    pub fn define_special_variable(&mut self, name: &str, value: Value) {
        self.set_symbol_value(name, value);
        self.make_special(name);
    }

    /// Define a C-level hook variable, the way GNU's `DEFVAR_LISP` does for
    /// every hook that lives in C: the variable is bound and special from the
    /// first Lisp form, and its value is `nil`.
    ///
    /// A hook's *variable* belongs to the engine; its *contents* belong to
    /// Lisp.  Every function a running Emacs finds on a C-level hook was put
    /// there by an `add-hook` in preloaded Lisp -- see GNU
    /// `src/minibuf.c:2553-2559`, which DEFVARs `minibuffer-setup-hook' and
    /// `minibuffer-exit-hook' and sets both to `Qnil'.  Because `add-hook'
    /// conses onto the front and does nothing when the function is already a
    /// member, the list's ORDER is a record of preload order, and any seed
    /// here would both turn the matching `add-hook' calls into no-ops and
    /// freeze an order that stops tracking GNU as new modes are preloaded.
    ///
    /// This constructor therefore takes no value: the seeded state is not
    /// expressible through it.
    pub fn define_c_hook_variable(&mut self, name: &str) {
        self.define_special_variable(name, Value::NIL);
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
        // Keywords (`:foo`) are self-evaluating constants. Use the thread-local
        // cached `is_keyword_id` predicate rather than re-resolving the symbol's
        // name to a string on every call: this runs on every `setq`/`set`, and
        // the old `resolve_sym_lisp_string` path took a registry read-lock and
        // materialized the name string each time (~7.6% of total CPU on a
        // setq-heavy load). `is_keyword_id` is exactly `canonical &&
        // name-starts-with-':'`, cached after the first lookup.
        crate::emacs_core::intern::is_keyword_id(id)
            || self
                .slot(id)
                .is_some_and(|s| s.flags.trapped_write() == SymbolTrappedWrite::NoWrite)
    }

    /// Decide what GNU does when `new_value` is written to `id`.
    ///
    /// This is the single authority for the `SYMBOL_NOWRITE` arm that GNU
    /// duplicates in `set_internal` (`src/data.c:1687-1697`) and
    /// `set_default_internal` (`src/data.c:2039-2049`).  Both read:
    ///
    /// ```c
    /// case SYMBOL_NOWRITE:
    ///   if (NILP (Fkeywordp (symbol))
    ///       || !EQ (newval, Fsymbol_value (symbol)))
    ///     xsignal1 (Qsetting_constant, symbol);
    ///   else
    ///     /* Allow setting keywords to their own value.  */
    ///     return;
    /// ```
    ///
    /// Every write path that GNU funnels through those two functions —
    /// `set`, `setq`, `set-default`, and `specbind` (via `do_specbind`,
    /// `src/eval.c:3597-3604`) — must ask this instead of testing
    /// [`Obarray::is_constant_id`] alone, or a keyword re-assigned its own
    /// value signals where GNU quietly does nothing.
    pub fn classify_constant_write(&self, id: SymId, new_value: Value) -> ConstantWrite {
        if !self.is_constant_id(id) {
            return ConstantWrite::Writable;
        }
        if crate::emacs_core::intern::is_keyword_id(id)
            && crate::emacs_core::value::eq_value(&Value::keyword_id(id), &new_value)
        {
            return ConstantWrite::KeywordSelfAssign;
        }
        ConstantWrite::Refused
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
        self.check_variable_alias(new_alias, base)?;
        // Install the alias edge. `make_alias` keeps both
        // representations in sync.
        self.make_alias(new_alias, base);
        self.make_special_id(new_alias);
        self.make_special_id(base);
        Ok(())
    }

    /// Every reason GNU's `Fdefvaralias` refuses, in GNU's order, and nothing
    /// else.
    ///
    /// Split out of [`Self::make_variable_alias`] so the Lisp-visible
    /// `defvaralias` subr and this obarray-level helper cannot disagree about
    /// the refusal set: `defvaralias` used to re-implement two of the four
    /// checks and simply omit the redirect switch, which is why every
    /// `DEFVAR_LISP` and `DEFVAR_KBOARD` name accepted an alias GNU refuses
    /// (ledger 170).  Returning the closed [`MakeAliasError`] rather than a
    /// pre-built signal keeps the obarray free of the evaluator's non-local
    /// control flow, the same split [`crate::emacs_core::forward::ForwardStoreError`]
    /// uses.
    pub fn check_variable_alias(
        &self,
        new_alias: SymId,
        base: SymId,
    ) -> Result<(), MakeAliasError> {
        // GNU checks the constant first (`src/eval.c:647-651`), then walks the
        // base chain for a cycle (`:654-662`), then switches on `new_alias`'s
        // redirect (`:665-679`).  The order is Lisp-visible: a constant that
        // would also cycle reports the constant.
        if let Some(sym) = self.slot(new_alias)
            && sym.flags.trapped_write() == SymbolTrappedWrite::NoWrite
        {
            return Err(MakeAliasError::Constant);
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

        if let Some(sym) = self.slot(new_alias) {
            match sym.flags.redirect() {
                SymbolRedirect::Forwarded => return Err(MakeAliasError::Forwarded),
                SymbolRedirect::Localized => return Err(MakeAliasError::Localized),
                SymbolRedirect::Plainval | SymbolRedirect::Varalias => {}
            }
        }
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
                    if v.is_unbound() {
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
                        if cdr_ref.is_unbound() {
                            None
                        } else {
                            Some(cdr_ref)
                        }
                    });
                }
                SymbolRedirect::Forwarded => {
                    use crate::emacs_core::forward::{LispBufferObjFwd, LispFwd, LispFwdType};
                    // Safety: `install_*fwd` leaks every descriptor.
                    let fwd: &'static LispFwd = unsafe { &*sym.val.fwd };
                    if let Some(value) = fwd.load_ref() {
                        return Some(value);
                    }
                    if fwd.ty == LispFwdType::BufferObj {
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
            .filter(|sym| sym.is_present() && sym.interned_global)
            .map(|sym| resolve_name(sym.name()))
            .collect()
    }

    /// All interned symbols' BOUND function-cell values (with the symbol's
    /// `NameId`), straight off the chunk storage — the whole-obarray-scan fast
    /// path (Gap 4b: `jit::aot::prepopulate_aot_from_preload`, which runs at
    /// every `NEOVM_AOT` startup). Same visibility filter as
    /// [`Self::all_symbols`] (`interned_global`) and the same bound-ness rule
    /// as [`Self::symbol_function_id`] (skip `function_unbound` / nil cells),
    /// but WITHOUT the per-symbol name→`intern`→`slot` round-trip a name-based
    /// walk pays (~3 lookups × every interned symbol; measured as the dominant
    /// cost of the AOT prepopulate pass). The `NameId` is handed out UNresolved
    /// so a caller that filters further (e.g. to bytecode-bound symbols) only
    /// pays the name resolution for the survivors (task #11: the manifest
    /// pre-filter keys candidates by symbol name).
    pub fn interned_function_cells_with_names(&self) -> impl Iterator<Item = (NameId, Value)> + '_ {
        self.symbols
            .iter()
            .filter(|sym| {
                sym.is_present()
                    && sym.interned_global
                    && !sym.function_unbound
                    && !sym.function.is_nil()
            })
            .map(|sym| (sym.name(), sym.function))
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
            // `iter()` yields every slot including empty tail slots; skip those.
            slot.is_present().then_some((SymId(idx as u32), slot))
        })
    }

    /// Iterate over ids interned in the global obarray.
    pub(crate) fn global_member_ids(&self) -> impl Iterator<Item = SymId> + '_ {
        self.iter_symbols()
            .filter(|(_, sym)| sym.interned_global)
            .map(|(id, _)| id)
    }

    /// Return the memoized completion bucket order for the current
    /// membership epoch + obarray length, computing (and caching) it on
    /// miss. The dump-load path resets nothing here: from_dump builds a
    /// fresh Obarray with an empty cache.
    pub(crate) fn completion_bucket_order_cached(
        &self,
        obarray_len: usize,
        compute: impl FnOnce() -> Vec<SymId>,
    ) -> std::sync::Arc<[SymId]> {
        let mut guard = self
            .completion_order_cache
            .lock()
            .expect("completion order cache poisoned");
        if let Some(cache) = guard.as_ref()
            && cache.members_epoch == self.members_epoch
            && cache.obarray_len == obarray_len
        {
            return std::sync::Arc::clone(&cache.ids);
        }
        let ids: std::sync::Arc<[SymId]> = compute().into();
        *guard = Some(CompletionOrderCache {
            members_epoch: self.members_epoch,
            obarray_len,
            ids: std::sync::Arc::clone(&ids),
        });
        ids
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
            #[cfg(test)]
            symbol_slot_read_count: std::sync::atomic::AtomicUsize::new(0),
            global_member_count: 0,
            function_epoch,
            value_epoch: 0,
            members_epoch: 0,
            completion_order_cache: std::sync::Mutex::new(None),
            blvs: Vec::new(),
            value_fwds: Vec::new(),
            debug_on_next_call_fwd: std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()),
        };
        for (id, mut sym) in symbols {
            sym.interned_global = false;
            sym.function_unbound = false;
            // Publish arms-then-name (Release) into the empty slot, consistent
            // with the live `ensure_slot` fill. Dump load is single-threaded (no
            // concurrent mark), but keeping the one fill discipline avoids a
            // second, subtly-different publish path.
            ob.symbols.ensure(Self::slot_index(id)).publish_fill(sym);
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
        for sym in self.symbols.iter().filter(|s| s.is_present()) {
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
        // Value-owning forwarder slots. GNU's `DEFVAR_INT` slot is an
        // `intmax_t` and needs no marking, but Neomacs stores the Lisp integer
        // (a heap object once it leaves fixnum range); GNU's `DEFVAR_LISP` and
        // `DEFVAR_KBOARD` slots live in `struct emacs_globals` / `struct
        // KBOARD`, which `staticpro` and `mark_kboards` root, and here they
        // live in the descriptor -- so in all three cases the descriptor is
        // the root. Traced from this list rather than from the symbol walk
        // because a symbol that was later localized no longer points at its
        // descriptor while the BLV still forwards through it. Like the BLV
        // pool, this loop always runs -- the SATB barrier in each `set`
        // covers the mark window, and the start seed / STW collection need
        // the full list.
        for fwd in &self.value_fwds {
            if let Some(value) = fwd.owned_value() {
                roots.push(value);
            }
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
#[path = "tests/mod.rs"]
mod tests;

/// Ledger 196: the buffer-local-read class ledger 191 named, pinned per site.
#[cfg(test)]
#[path = "tests/buffer_local_global_read.rs"]
mod buffer_local_global_read_tests;
