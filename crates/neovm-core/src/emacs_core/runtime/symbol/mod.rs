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
//! | Tag         | payload ([`ValueCell`])        | GNU equivalent      |
//! | ----------- | ------------------------------ | ------------------- |
//! | `Plainval`  | direct [`Value`] (or UNBOUND)  | `SYMBOL_PLAINVAL`   |
//! | `Varalias`  | aliased [`SymId`]              | `SYMBOL_VARALIAS`   |
//! | `Localized` | the BLV record ([`BlvPtr`])    | `SYMBOL_LOCALIZED`  |
//! | `Forwarded` | `&'static LispFwd`             | `SYMBOL_FORWARDED`  |
//!
//! The tag and its payload live in the private `cell` module: they are read
//! together as a [`ValueCell`] and written together by one seam, so the
//! obarray cannot pair a tag with another arm's payload (see `cell`).

mod cell;
mod fn_stamps;
mod property_revision;
use cell::{ArmMut, CellWrite, MarkGate, read_symbol_children};
pub(crate) use cell::{
    BlvPtr, DumpedCell, DumpedSymbolFlags, LISP_SYMBOL_FLAGS_OFFSET,
    LISP_SYMBOL_INTERNED_GLOBAL_OFFSET, LISP_SYMBOL_SIZE, LISP_SYMBOL_VAL_OFFSET, ValueCell,
};
pub use cell::{LispSymbol, SymbolFlags};
#[cfg(test)]
pub(crate) use fn_stamps::force_fn_stamps_for_test;
pub use property_revision::SymbolPropertyRevision;

use super::defvar_bool::ByteBooleanVars;
use super::intern::{
    NameId, SymId, intern, intern_lisp_string, is_canonical_id, lookup_interned,
    lookup_interned_lisp_string, resolve_name, resolve_sym_lisp_string,
};
use super::value::{Value, ValueKind, VecLikeType};
use crate::emacs_core::error::Flow;
use crate::emacs_core::forward::FwdDescriptor;
use crate::gc_trace::GcTrace;
use crate::heap_types::LispString;
use crate::tagged::header::{load_value_atomic, store_value_atomic};
use num_enum::{IntoPrimitive, TryFromPrimitive};
#[cfg(test)]
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
thread_local! {
    static FUNCTION_CELL_LOOKUP_COUNT: Cell<usize> = const { Cell::new(0) };
    /// Slot visits made by the plain-value store paths, so a test can pin
    /// "one obarray visit per bind and per pop".
    static PLAIN_VALUE_SLOT_VISITS: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_function_cell_lookup_count() {
    FUNCTION_CELL_LOOKUP_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn reset_plain_value_slot_visits() {
    PLAIN_VALUE_SLOT_VISITS.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn plain_value_slot_visits() -> usize {
    PLAIN_VALUE_SLOT_VISITS.with(Cell::get)
}

#[cfg(test)]
fn note_plain_value_slot_visit() {
    PLAIN_VALUE_SLOT_VISITS.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
pub(crate) fn function_cell_lookup_count() -> usize {
    FUNCTION_CELL_LOOKUP_COUNT.with(Cell::get)
}

// ===========================================================================
// Redirect machinery — mirrors GNU `lisp.h:771-829`
// ===========================================================================

/// Two-bit `redirect` tag. Mirrors GNU `enum symbol_redirect`
/// (`src/lisp.h:771-777`). The tag of a symbol's [`ValueCell`].
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
    ///
    /// Written only through [`LispBufferLocalValue::set_where`], which keeps
    /// [`Self::where_buf_id`] in step.
    pub where_buf: Value,
    /// The raw [`BufferId`](crate::buffer::BufferId) of `where_buf`, or
    /// [`NO_WHERE_BUF`] when it is not a buffer: the read fast path's
    /// where-buffer test as one integer compare. Comparing the object meant
    /// a tag test, a header load and a field load on every read of a
    /// buffer-local variable -- a third of the read.
    pub where_buf_id: u64,
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

/// [`LispBufferLocalValue::where_buf_id`] when `where_buf` is not a buffer.
pub(crate) const NO_WHERE_BUF: u64 = u64::MAX;

impl LispBufferLocalValue {
    /// Point the cache at `buf` (a buffer, or `Value::NIL` for the global
    /// default): the object for the collector and for `eq` tests, its id
    /// for the read fast path. The one writer of both.
    #[inline]
    pub(crate) fn set_where(&mut self, buf: Value) {
        store_value_atomic(&mut self.where_buf, buf);
        self.where_buf_id = buf.as_buffer_id().map_or(NO_WHERE_BUF, |id| id.0);
    }
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

/// The address of [`BLV_ALIST_EPOCH`], a process static: what JIT code
/// (never AOT) reads the current epoch from when it tests a buffer-local
/// variable's cache inline (P1.4 Stage B).
#[inline]
pub(crate) fn blv_alist_epoch_addr() -> usize {
    std::ptr::from_ref(&BLV_ALIST_EPOCH) as usize
}

const _: () = assert!(std::mem::size_of::<std::sync::atomic::AtomicU64>() == 8);

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

/// Reserved [`NameId`] stored in the `name` cell of an EMPTY obarray slot.
/// The obarray's chunk store holds fully-initialized [`LispSymbol`]s rather
/// than `Option<LispSymbol>`; a slot is "empty" (never interned) iff its
/// `name` atom equals this sentinel. `u32::MAX` is reserved: real `NameId`s
/// mint densely from `NameId(strings.len())` (`intern.rs`), and the mint site
/// carries a `debug_assert` that a real id never reaches `u32::MAX`.
pub(crate) const SYMBOL_NAME_SENTINEL: NameId = NameId(u32::MAX);

/// What a `Localized` symbol's BLV cache holds when it is loaded for one
/// buffer at the current structural epoch: GNU `swap_in_symval_forwarding`'s
/// early-out (`blv->where` is already the buffer), the one hit rule that
/// `read_localized_symbol_for_buffer`, `has_per_buffer_binding` and
/// `set_internal_localized_with` each apply before trusting the cache.
/// Copied out by value, so no borrow of the BLV record outlives the read.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BlvCacheHit {
    /// The `(SYMBOL . VALUE)` cell loaded for the buffer: its local binding
    /// when `found`, otherwise normally `defcell`.
    pub(crate) valcell: Value,
    /// `(SYMBOL . DEFAULT-VALUE)`.
    pub(crate) defcell: Value,
    /// GNU `blv_found`: `valcell` is the buffer's own binding.
    pub(crate) found: bool,
    /// GNU `local_if_set`: a `set` with no binding creates one.
    pub(crate) local_if_set: bool,
    /// The forwarder whose type rule the value obeys (`make_blv` keeps it).
    pub(crate) fwd: Option<&'static crate::emacs_core::forward::LispFwd>,
}

/// A `Localized` symbol's cells as `set_default_internal` sees them
/// ([`LispSymbol::blv_default_cells`]); copied out like [`BlvCacheHit`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct BlvDefaultCells {
    /// `(SYMBOL . DEFAULT-VALUE)`; also the loaded cell whenever the
    /// default is what is loaded, so one store reaches both.
    pub(crate) defcell: Value,
    /// The forwarder whose type rule the value obeys.
    pub(crate) fwd: Option<&'static crate::emacs_core::forward::LispFwd>,
}

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
    blv.set_where(current_buffer);
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

/// A plain-cell store found the cell in another arm and stored nothing
/// ([`Obarray::store_plain_value_id`]): the caller has to re-dispatch on the
/// arm the cell is in now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use = "the value was not stored; re-dispatch on the cell's current arm"]
pub(crate) struct NotPlain;

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
    /// The BLV cache of a `Localized` symbol when it is loaded for BUFFER at
    /// the current epoch (see [`BlvCacheHit`]); `None` for a miss or any
    /// other redirect.
    #[inline]
    pub(crate) fn blv_cache_hit(&self, buffer: crate::buffer::BufferId) -> Option<BlvCacheHit> {
        let blv = self.localized_blv()?;
        // SAFETY: a `Localized` cell names a record `make_symbol_localized`
        // allocated and the obarray owns for its lifetime; the evaluator
        // thread is its only writer and nothing is written while this shared
        // borrow lives.
        let blv = unsafe { &*blv.as_ptr() };
        if blv.alist_epoch != blv_alist_epoch() || blv.where_buf_id != buffer.0 {
            return None;
        }
        debug_assert_eq!(
            blv.where_buf.as_buffer_id().map_or(NO_WHERE_BUF, |b| b.0),
            blv.where_buf_id,
            "a BLV writer bypassed set_where"
        );
        Some(BlvCacheHit {
            valcell: blv.valcell,
            defcell: blv.defcell,
            found: blv.found,
            local_if_set: blv.local_if_set,
            fwd: blv.fwd,
        })
    }

    /// A `Localized` symbol's default cell and forwarder, whatever buffer
    /// the cache is loaded for: what `set_default_internal`
    /// needs (it writes `defcell` in any buffer). `None` for any other
    /// redirect.
    #[inline]
    pub(crate) fn blv_default_cells(&self) -> Option<BlvDefaultCells> {
        let blv = self.localized_blv()?;
        // SAFETY: as in `blv_cache_hit`.
        let blv = unsafe { &*blv.as_ptr() };
        Some(BlvDefaultCells {
            defcell: blv.defcell,
            fwd: blv.fwd,
        })
    }
}

/// Why [`Obarray::function_epoch`] moved. Observability only: the JIT's
/// report counts bumps per reason (`jit::stats`), and a cconv memo verify
/// mismatch lists the writes its run made. No behaviour depends on it —
/// every writer bumps exactly when and how it did before.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, strum::EnumCount, strum::EnumIter, strum::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum FunctionEpochBump {
    /// `fset` (`builtin_fset_2`; GNU `Ffset`, data.c).
    Fset,
    /// `defalias` (`builtin_defalias`, `Context::defalias_value`; GNU
    /// `Fdefalias`, data.c).
    Defalias,
    /// Every other function-cell write through `set_symbol_function(_id)`:
    /// runtime installers, kmacro, abbrev, the AOT self-tests.
    InternalCellWrite,
    /// The pdump image restore writing a dumped function cell.
    PdumpRestore,
    /// `fmakunbound` (`fmakunbound_id`; also the pdump restore of an unbound cell).
    Fmakunbound,
    /// `clear_function_silent_id` (init-time masking; also the pdump restore).
    SilentClear,
    /// `unintern_id`.
    Unintern,
    /// A static subr entry rewritten in place (`install_subr`), no cell write.
    SubrRewrite,
    /// The `compiler_function_overrides` toggle
    /// (`sync_cached_runtime_binding_by_id`), no cell write.
    CompilerOverrides,
}

/// A function-cell write that stored the value already there: no epoch
/// move (the JIT report counts these next to the bumps).
#[inline]
fn note_function_cell_unchanged() {
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::stats::note_function_cell_unchanged();
}

/// The obarray — a table of interned symbols.
///
/// This is the central symbol registry. `intern` looks up or creates symbols,
/// ensuring that `(eq 'foo 'foo)` is always true.
///
/// Phase 4 of the symbol-redirect refactor adds a heap-allocated BLV
/// pool ([`Obarray::blvs`]) for `LOCALIZED` symbols. The Obarray owns
/// every BLV; a `Localized` symbol's cell names its record by [`BlvPtr`]
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
    /// Publication clock for function-binding changes. Producers hold exclusive
    /// obarray write access (including chunk growth), publish stamps/floors
    /// first, then Release-store this clock. Mutator readers Acquire-load it
    /// before reading stamps, retaining that snapshot if they refresh a cache.
    /// Atomic storage also keeps those readers safe when a writer runs on
    /// another mutator; it does not replace the producer's exclusive access.
    function_epoch: AtomicU64,
    /// Bumped whenever global-obarray MEMBERSHIP changes (mark/clear);
    /// keys the completion bucket-order cache below.
    members_epoch: u64,
    /// Memoized GNU-bucket-order symbol list for completion over the
    /// global obarray: try-completion/all-completions re-derive the same
    /// ~30k-symbol hash+sort per call (a bootstrap hotspot). Completion owns
    /// this cache on the mutator. The concurrent marker receives a leased
    /// symbol snapshot and never accesses the cache or `&Obarray`.
    completion_order_cache: std::sync::Mutex<Option<CompletionOrderCache>>,
    /// Heap-allocated BLVs for `SYMBOL_LOCALIZED` symbols. Each entry
    /// is a `Box::into_raw` pointer; freed in [`Obarray::drop`]. The
    /// pool is append-only — we never reuse a slot.
    blvs: Vec<BlvPtr>,
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
    /// `Op::Call` cost ~48 Ir/call on the Tier-0 differential.
    ///
    /// Never null, so the hot read is a pointer load and a byte test
    /// ([`Self::debug_on_next_call_armed_fast`]): it names the resolved
    /// descriptor, or `DEBUG_ON_NEXT_CALL_UNRESOLVED` (reads armed, so the
    /// reader's reference path resolves) until first resolved, or
    /// `DEBUG_ON_NEXT_CALL_ABSENT` (reads disarmed) when the obarray has no
    /// `DEFVAR_BOOL` for it.  The address is stable for THIS obarray once
    /// resolved: `define_bool_variable` reuses an existing descriptor rather
    /// than replacing it, `make_blv` moves the SAME cell into the BLV, and
    /// the two places that install a descriptor (`install_boolfwd`,
    /// `reattach_localized_forwarder`) store it here themselves.  `clone()`
    /// resets it because clone duplicates stateful forwarders.
    debug_on_next_call_fwd: std::sync::atomic::AtomicPtr<crate::emacs_core::forward::LispBoolFwd>,
    /// Whether `max-lisp-eval-depth` has EVER been made buffer-local in this
    /// obarray. The eval-depth guard on every function call has to know
    /// whether a buffer-local value could undercut the global limit; asking
    /// the symbol slot each time is a chunk lookup and a flag read per call,
    /// for an answer that is "no" in practice. Sticky: once set it only sends
    /// that guard down its exact slow path.
    pub(crate) max_lisp_eval_depth_localized: bool,
    /// Process-unique identity of this obarray's symbol storage, drawn from
    /// [`next_obarray_generation`] by every constructor (`new`, `clone`,
    /// `from_dump`). A consumer that keeps an address inside this obarray --
    /// a symbol cell, a BLV record, a forwarder descriptor -- records the
    /// generation beside it and compares before trusting the address, so a
    /// replaced obarray is detected instead of dereferenced. `ctx.obarray` is
    /// assigned only by the Context constructors, so in practice this moves
    /// only with the heap; the JIT cache checks it anyway on every GC root
    /// walk (`jit::cache::sync_cache_to_obarray`).
    generation: u64,
}

// The owner contains thread-local Values and mutable BLV records. Atomic
// symbol words do not admit sharing their containing owner; the marker moves
// only an admitted ObarrayScanSnapshot with retained chunk storage.
static_assertions::assert_impl_all!(Obarray: Clone, std::fmt::Debug);
static_assertions::assert_not_impl_any!(Obarray: Send, Sync);

/// The next [`Obarray::generation`]: one process-global counter, so no two
/// obarrays alive at once (or ever) share a generation.
fn next_obarray_generation() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
    /// Per-chunk side data (one box per chunk, index-aligned with `chunks`):
    /// the chunk's seqlock and its function-binding stamps
    /// ([`fn_stamps::ChunkSide`]). Boxed so the seqlock's address stays stable
    /// for the concurrent GC reader even when the `Vec` spine reallocs. Seqlock:
    /// even = stable; odd = a `(flags, val)` write is in flight in that chunk.
    /// Only ever bumped while a concurrent mark is active (Stage 1b); zero cost
    /// otherwise. The GC reads it with the standard seqlock protocol (retry
    /// while odd / changed).
    // Each side must keep a stable address while the Vec spine grows; the
    // per-element box is the concurrency invariant, not redundant storage.
    #[allow(clippy::vec_box)]
    sides: Vec<Box<fn_stamps::ChunkSide>>,
    /// Logical slot count; grows to a chunk boundary as chunks are appended.
    len: usize,
    /// Address of `chunks`' buffer — an array of thin pointers, one per chunk
    /// — kept equal to `chunks.as_ptr()` by every operation that can move it
    /// (construction, clone, growth). Compiled code reads it at a fixed offset
    /// to reach a symbol's cell without a call (see
    /// [`OBARRAY_JIT_SPINE_OFFSET`]). This integer preserves the JIT layout;
    /// it does not make the containing thread-local symbol store Send/Sync.
    spine_addr: usize,
}

impl Drop for SymbolChunks {
    fn drop(&mut self) {
        // Arc uniqueness proves that all snapshot leases have ended after
        // their final raw-pointer reads. An outstanding lease retains both
        // allocations without waiting for the marker or invoking callbacks.
        for (chunk, mut side) in self.chunks.drain(..).zip(self.sides.drain(..)) {
            if side.scan_storage.has_leases() {
                std::mem::forget(chunk);
                std::mem::forget(side);
            }
        }
    }
}

/// Where compiled code finds a symbol's value cell from the `Obarray`:
/// `len` at [`OBARRAY_JIT_LEN_OFFSET`] bounds the slot index, the spine at
/// [`OBARRAY_JIT_SPINE_OFFSET`] holds one pointer per chunk, and slot `idx`
/// lives at `spine[idx >> OBARRAY_CHUNK_BITS] + (idx & (OBARRAY_CHUNK - 1)) *
/// LISP_SYMBOL_SIZE`. An empty slot reads as a `Plainval` unbound cell.
pub(crate) const OBARRAY_JIT_SPINE_OFFSET: usize =
    std::mem::offset_of!(Obarray, symbols) + std::mem::offset_of!(SymbolChunks, spine_addr);
/// Where compiled code reads the obarray's function epoch (a `u64`), which
/// every function-cell write moves: an armed call site compares it inline.
pub(crate) const OBARRAY_FUNCTION_EPOCH_OFFSET: usize =
    std::mem::offset_of!(Obarray, function_epoch);

// The clock keeps the old field's layout, including baked JIT offsets.
const _: () = {
    assert!(std::mem::size_of::<AtomicU64>() == std::mem::size_of::<u64>());
    assert!(std::mem::align_of::<AtomicU64>() == std::mem::align_of::<u64>());
};
/// Where compiled code reads the memoized `debug-on-next-call` descriptor
/// pointer (never null: a stand-in until resolved; see
/// `Obarray::debug_on_next_call_fwd`).
pub(crate) const OBARRAY_DEBUG_ON_NEXT_CALL_FWD_OFFSET: usize =
    std::mem::offset_of!(Obarray, debug_on_next_call_fwd);
/// See [`OBARRAY_JIT_SPINE_OFFSET`].
pub(crate) const OBARRAY_JIT_LEN_OFFSET: usize =
    std::mem::offset_of!(Obarray, symbols) + std::mem::offset_of!(SymbolChunks, len);
/// See [`OBARRAY_JIT_SPINE_OFFSET`].
pub(crate) const OBARRAY_CHUNK_BITS: u32 = 12;
/// See [`OBARRAY_JIT_SPINE_OFFSET`].
pub(crate) const OBARRAY_CHUNK_SLOTS: usize = OBARRAY_CHUNK;
/// Mask of the redirect bits in the flags byte; `Plainval` is zero.
pub(crate) const SYMBOL_FLAGS_REDIRECT_MASK: u8 = SymbolFlags::REDIRECT_MASK;
/// The aligned 4-byte word of a symbol that holds its write-relevant bytes:
/// the flags byte and `interned_global` today, and the watch byte P1.3 plans.
/// One aligned word, so a JIT guard reads them all with one load and a
/// concurrent reader never sees two of them torn across words.
pub(crate) const LISP_SYMBOL_WRITE_WINDOW_OFFSET: usize = LISP_SYMBOL_FLAGS_OFFSET & !3;

/// The bits of a symbol's [`LispSymbol::write_window`] every inline or
/// cached symbol-cell write tests: the redirect, the trapped-write state, the
/// host-projection bit, and the whole `interned_global` byte. P1.3's watch
/// byte joins the mask in the commit that adds it.
pub(crate) const SYMCELL_INLINE_WRITE_MASK: u16 = (SymbolFlags::REDIRECT_MASK
    | SymbolFlags::TRAPPED_WRITE_MASK
    | SymbolFlags::RUNTIME_PROJECTED_BIT) as u16
    | 0xFF00;
/// The masked window of a symbol a store may write without the general
/// path: untrapped (no watcher, not a constant), not host-projected, and an
/// interned member of the obarray, with redirect `Plainval`. For another
/// redirect OR its code in: [`symcell_inline_write_value`].
pub(crate) const SYMCELL_INLINE_WRITE_VALUE: u16 = 1 << 8;

/// [`SYMCELL_INLINE_WRITE_VALUE`] for a cell of REDIRECT.
#[inline(always)]
pub(crate) const fn symcell_inline_write_value(redirect: SymbolRedirect) -> u16 {
    SYMCELL_INLINE_WRITE_VALUE | redirect as u16
}

const _: () = {
    assert!(std::mem::size_of::<bool>() == 1);
    assert!(SymbolTrappedWrite::Untrapped as u8 == 0);
    assert!(SYMCELL_INLINE_WRITE_MASK == 0xFF8F);
    assert!(LISP_SYMBOL_INTERNED_GLOBAL_OFFSET == LISP_SYMBOL_FLAGS_OFFSET + 1);
    assert!(LISP_SYMBOL_INTERNED_GLOBAL_OFFSET & !3 == LISP_SYMBOL_WRITE_WINDOW_OFFSET);
    assert!(LISP_SYMBOL_WRITE_WINDOW_OFFSET + 4 <= LISP_SYMBOL_SIZE);
};

const _: () = {
    assert!(OBARRAY_CHUNK == 1 << OBARRAY_CHUNK_BITS);
    assert!(
        std::mem::size_of::<Box<[LispSymbol; OBARRAY_CHUNK]>>() == std::mem::size_of::<usize>()
    );
    assert!(std::mem::size_of::<SymbolFlags>() == 1);
    assert!(SymbolRedirect::Plainval as u8 == 0);
};

impl Clone for SymbolChunks {
    fn clone(&self) -> Self {
        // A cloned obarray is never concurrently marked, so the seqlocks reset
        // to 0 (even); the function stamps carry over. (The side boxes hold
        // atomics, which are not `Clone`, hence the manual impl.)
        let chunks = self.chunks.clone();
        let spine_addr = chunks.as_ptr() as usize;
        Self {
            chunks,
            sides: self
                .sides
                .iter()
                .map(|side| Box::new(side.clone_for_new_obarray()))
                .collect(),
            len: self.len,
            spine_addr,
        }
    }
}

impl SymbolChunks {
    fn new() -> Self {
        let chunks = Vec::new();
        let spine_addr = chunks.as_ptr() as usize;
        Self {
            chunks,
            sides: Vec::new(),
            len: 0,
            spine_addr,
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
            // Every chunk carries the same function-stamp floor; a new one
            // takes the current value (none yet: no floor was ever raised,
            // `raise_fn_floor` creates chunk 0 first).
            let floor = self.sides.first().map_or(0, |side| side.floor());
            self.sides.push(Box::new(fn_stamps::ChunkSide::new(floor)));
            // The push may have moved the spine; publish it BEFORE `len` covers
            // the new slots, so a reader bounded by `len` never indexes a
            // stale spine.
            self.spine_addr = self.chunks.as_ptr() as usize;
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

    /// Begin a write of the value cell of the PRESENT slot `idx`, with the
    /// seqlock of the chunk that holds it (see [`CellWrite`]); `None` when the
    /// slot is out of range or empty. The slot and its seqlock live in
    /// different allocations (`chunks` and `sides`), so both borrows coexist.
    #[inline(always)]
    fn cell_write(&mut self, idx: usize, gate: MarkGate) -> Option<CellWrite<'_>> {
        if idx >= self.len {
            return None;
        }
        let slot = &mut self.chunks[idx >> 12][idx & (OBARRAY_CHUNK - 1)];
        if !slot.is_present() {
            return None;
        }
        let sides = &self.sides;
        Some(CellWrite::begin(slot, || &sides[idx >> 12].seq, gate))
    }

    /// The function-stamp validity rule for slot `idx` (see
    /// [`fn_stamps`]): its function binding has not changed, and no change of
    /// every symbol's resolution happened, since the clock read `since`. A
    /// slot past the last chunk proves nothing.
    #[inline]
    fn fn_unchanged_since(&self, idx: usize, since: u64) -> bool {
        let Some(side) = self.sides.get(idx >> 12) else {
            return false;
        };
        since != u64::MAX && since >= side.floor() && side.stamp(idx & (OBARRAY_CHUNK - 1)) <= since
    }

    /// Record that slot `idx`'s function binding changed at clock value
    /// `epoch` (before the clock moves there). Grows the store to cover the
    /// slot, so a later fill of that chunk cannot lose the stamp.
    fn stamp_function(&mut self, idx: usize, epoch: u64) {
        if self.len <= idx {
            self.grow_for(idx);
        }
        self.sides[idx >> 12].set_stamp(idx & (OBARRAY_CHUNK - 1), epoch);
    }

    /// Void every entry validated before clock value `floor`, for every
    /// slot (before the clock moves there). Chunk 0 is created if none
    /// exists, so the chunks created later copy the floor.
    fn raise_fn_floor(&mut self, floor: u64) {
        if self.sides.is_empty() {
            self.grow_for(0);
        }
        for side in &self.sides {
            side.set_floor(floor);
        }
    }

    /// Whether slot `idx`'s chunk holds a stamp array (tests).
    #[cfg(test)]
    fn chunk_has_fn_stamps(&self, idx: usize) -> bool {
        self.sides
            .get(idx >> 12)
            .is_some_and(|side| side.has_stamps())
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
        world: &crate::tagged::gc::scan_contract::SingleMutatorWorld<'_>,
    ) -> (Vec<ObarrayScanEntry>, usize) {
        let parts = self
            .chunks
            .iter()
            .zip(self.sides.iter())
            .map(|(chunk, side)| ObarrayScanEntry {
                slots: chunk.as_ptr(),
                seq: &side.seq,
                _lease: crate::tagged::gc::scan_contract::ScanStorageLease::capture(
                    &side.scan_storage,
                    world,
                ),
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
/// protocol (`cell::read_symbol_children`). Chunks (and slots) interned
/// mid-cycle live beyond `n_chunks`/`n_slots` and are NOT in the snapshot; they are
/// allocate-black-equivalent in the obarray sense and are picked up by the
/// termination re-seed of the new range.
///
/// Admission names the heap's serialized-writer protocol. Each entry leases its
/// stable chunk and side box; dropping their owner with a live reader abandons
/// those allocations instead of freeing storage the marker still reads.
#[derive(Debug)]
struct ObarrayScanEntry {
    slots: *const LispSymbol,
    seq: *const std::sync::atomic::AtomicU32,
    _lease: crate::tagged::gc::scan_contract::ScanStorageLease,
}

static_assertions::assert_not_impl_any!(ObarrayScanEntry: Send, Sync);

pub(crate) struct ObarrayScanSnapshot {
    /// (slots-array base ptr, chunk seqlock ptr) for each chunk present at start.
    chunks: Vec<ObarrayScanEntry>,
    /// Logical live-slot count at start (so the scan covers slots [0, n_slots)).
    n_slots: usize,
    heap_identity: usize,
}

// This snapshot is embedded in TaggedHeap. Preserve its original envelope so
// adding heap identity does not move the heap's JIT-visible allocation fields.
static_assertions::assert_eq_size!(ObarrayScanSnapshot, [usize; 5]);
static_assertions::assert_eq_size!(Option<ObarrayScanSnapshot>, [usize; 5]);
static_assertions::const_assert_eq!(
    std::mem::align_of::<ObarrayScanSnapshot>(),
    std::mem::align_of::<usize>()
);
static_assertions::const_assert_eq!(std::mem::offset_of!(ObarrayScanSnapshot, chunks), 0);
static_assertions::const_assert_eq!(
    std::mem::offset_of!(ObarrayScanSnapshot, n_slots),
    3 * std::mem::size_of::<usize>()
);
static_assertions::const_assert_eq!(
    std::mem::offset_of!(ObarrayScanSnapshot, heap_identity),
    4 * std::mem::size_of::<usize>()
);

// SAFETY: construction requires the heap-identified serialized-writer admission.
// Every raw chunk/side pointer has a storage lease: owner destruction retains
// the allocations until the marker has finished reading. The one owning marker
// uses the Acquire presence gate and the admitted writer's seqlock protocol
// for the redirect/payload pair. Function and plist are independent atomic
// words. It never accesses BLVs, descriptor interiors or the completion cache.
unsafe impl Send for ObarrayScanSnapshot {}
static_assertions::assert_impl_all!(ObarrayScanSnapshot: Send, std::fmt::Debug);
static_assertions::assert_not_impl_any!(ObarrayScanSnapshot: Sync, Copy, Clone);

impl std::fmt::Debug for ObarrayScanSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObarrayScanSnapshot")
            .field("heap_identity", &self.heap_identity)
            .field("chunks", &self.chunks.len())
            .field("slots", &self.n_slots)
            .finish()
    }
}

impl ObarrayScanSnapshot {
    pub(crate) fn heap_identity(&self) -> usize {
        self.heap_identity
    }

    /// Chunk count captured at start. Symbols interned mid-cycle live in chunks
    /// `>= n_chunks` (slots `>= n_slots`) and are not covered by this scan; the
    /// termination re-seed covers that new range.
    #[inline]
    pub(crate) fn n_chunks(&self) -> usize {
        self.chunks.len()
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
    /// must still address live, non-moving storage. The entry's retention lease
    /// keeps chunk and side allocations valid even if their owner is dropped.
    pub(crate) unsafe fn scan(&self, push: impl FnMut(Value)) {
        // SAFETY: the caller supplies the admitted cycle's writer protocol.
        unsafe { self.scan_children::<false>(push) };
    }

    /// Major variant of `scan`, also visiting bare symbol children.
    ///
    /// # Safety
    /// Same start-snapshot, presence and seqlock lifetime as `scan`.
    pub(crate) unsafe fn scan_for_major(&self, push: impl FnMut(Value)) {
        // SAFETY: the caller supplies the same admitted writer protocol.
        unsafe { self.scan_children::<true>(push) };
    }

    unsafe fn scan_children<const MAJOR: bool>(&self, mut push: impl FnMut(Value)) {
        let mut global_idx = 0usize;
        for entry in &self.chunks {
            if global_idx >= self.n_slots {
                break;
            }
            // SAFETY: entry's lease retains this stable side box through read.
            let seq = unsafe { &*entry.seq };
            for offset in 0..OBARRAY_CHUNK {
                if global_idx >= self.n_slots {
                    break;
                }
                // SAFETY: entry.slots is this chunk's [LispSymbol; CHUNK] base;
                // `offset < OBARRAY_CHUNK` is in bounds; the chunk never moves.
                // Every slot is a valid (possibly EMPTY) LispSymbol — there is no
                // uninitialized memory to read. A concurrent mutator only either
                // (a) publishes an empty slot via a terminal `Release` store to
                // `name` after writing the arms, or (b) mutates an
                // already-published slot's value-cell ARM under the seqlock;
                // neither resizes or relocates the slot.
                let slot = unsafe { &*entry.slots.add(offset) };
                // PRESENCE GATE — the ONLY cross-thread presence read. `Acquire`
                // load of the write-once `name` cell, pairing with the fill's
                // terminal `Release` (`publish_fill`): observing a non-sentinel
                // name happens-after every arm write, so the seqlock read below
                // sees a fully-initialized slot (no data race on the arms). A
                // slot still reading SENTINEL is never-interned OR a fresh fill
                // not yet published — skip it; a symbol interned mid-cycle is
                // allocate-black / SATB-retained and need not be scanned now.
                if slot.name.load(Ordering::Acquire) != SYMBOL_NAME_SENTINEL.0 {
                    read_symbol_children::<MAJOR>(seq, slot, &mut push);
                }
                global_idx += 1;
            }
        }
    }
}

impl std::fmt::Debug for Obarray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Obarray")
            .field("global_member_count", &self.global_member_count)
            .field("function_epoch", &self.function_epoch())
            .field("blvs", &self.blvs.len())
            .finish_non_exhaustive()
    }
}

impl Drop for Obarray {
    fn drop(&mut self) {
        for blv in self.blvs.drain(..) {
            // SAFETY: every record in the pool was leaked by
            // `make_symbol_localized` or `clone` for this obarray alone, and
            // the only cells that name it go away with `self`.
            unsafe { blv.free() };
        }
    }
}

impl Clone for Obarray {
    fn clone(&self) -> Self {
        // Deep-copy the BLV pool. Build a `old → new` map so we can
        // re-home each LOCALIZED symbol's cell to its record's copy.
        let mut blvs: Vec<BlvPtr> = Vec::with_capacity(self.blvs.len());
        let mut blv_map: rustc_hash::FxHashMap<BlvPtr, BlvPtr> = rustc_hash::FxHashMap::default();
        for &orig in &self.blvs {
            // SAFETY: the pool's records are alive for the duration of `&self`.
            let copy = BlvPtr::leak(Box::new(unsafe { (*orig.as_ptr()).clone() }));
            blvs.push(copy);
            blv_map.insert(orig, copy);
        }
        let mut symbols = self.symbols.clone();
        // Duplicate the forwarders that OWN a value. Sharing them would make
        // `(setq gc-cons-threshold ...)` in one obarray visible in the other,
        // which is the very desync the per-context descriptor exists to avoid.
        let mut fwd_map: rustc_hash::FxHashMap<
            usize,
            &'static crate::emacs_core::forward::LispFwd,
        > = rustc_hash::FxHashMap::default();
        let mut value_fwds = Vec::with_capacity(self.value_fwds.len());
        for idx in 0..symbols.len() {
            let Some(mut write) = symbols.cell_write(idx, MarkGate::read()) else {
                continue;
            };
            match write.arm() {
                ArmMut::Localized(local) => {
                    if let Some(&copy) = blv_map.get(&local.blv()) {
                        local.rehome(copy);
                    }
                }
                ArmMut::Forwarded(forwarded) => {
                    let orig = forwarded.descriptor();
                    let key = std::ptr::from_ref(orig) as usize;
                    let copy = match fwd_map.get(&key) {
                        Some(&existing) => existing,
                        None => {
                            let Some(copy) = orig.clone_stateful() else {
                                continue;
                            };
                            if copy.owned_value().is_some() {
                                value_fwds.push(copy);
                            }
                            fwd_map.insert(key, copy);
                            copy
                        }
                    };
                    forwarded.forward_to(copy);
                }
                ArmMut::Plain(_) | ArmMut::Alias(_) => {}
            }
        }
        // A BLV built from a forwarded symbol keeps a pointer to the same
        // descriptor; re-point it at the clone so the pair stays consistent.
        for &blv_ptr in &blvs {
            // SAFETY: a record this clone just leaked and alone owns.
            let blv = unsafe { &mut *blv_ptr.as_ptr() };
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
            function_epoch: AtomicU64::new(self.function_epoch()),
            members_epoch: self.members_epoch,
            completion_order_cache: std::sync::Mutex::new(None),
            blvs,
            value_fwds,
            // The clone re-leaked every stateful forwarder above; the cached
            // descriptor belongs to the source obarray, so the clone starts
            // unresolved.
            debug_on_next_call_fwd: Self::debug_on_next_call_unresolved(),
            max_lisp_eval_depth_localized: self.max_lisp_eval_depth_localized,
            // A deep copy is new storage: every address it holds is new.
            generation: next_obarray_generation(),
        }
    }
}

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

    /// The address of ID's slot, for JIT code that reads and writes the
    /// symbol's cell in place from a baked address (P1.4 Stage B): chunks
    /// are only ever appended and live as long as the obarray (see
    /// [`SymbolChunks`]), so the address is stable for the obarray's life,
    /// whose [`Self::generation`] the JIT cache pins. `None` for an id past
    /// the last chunk. The slot may be empty (it reads as an unbound
    /// `Plainval` cell).
    pub(crate) fn jit_symbol_cell_addr(&self, id: SymId) -> Option<usize> {
        let idx = Self::slot_index(id);
        (idx < self.symbols.len).then(|| {
            self.symbols.chunks[idx >> OBARRAY_CHUNK_BITS].as_ptr() as usize
                + (idx & (OBARRAY_CHUNK - 1)) * LISP_SYMBOL_SIZE
        })
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

    /// Begin a write of `id`'s value cell, interning the slot if it is
    /// empty: the entry for the cold transitions (alias, localize, forward,
    /// unbind). See [`CellWrite`].
    fn cell_write_ensure(&mut self, id: SymId) -> CellWrite<'_> {
        self.ensure_slot(id);
        let gate = MarkGate::read();
        self.symbols
            .cell_write(Self::slot_index(id), gate)
            .unwrap_or_else(|| unreachable!("ensure_slot published the slot"))
    }

    /// Begin a write of `id`'s value cell when its slot is present.
    #[inline(always)]
    fn cell_write(&mut self, id: SymId) -> Option<CellWrite<'_>> {
        let gate = MarkGate::read();
        self.symbols.cell_write(Self::slot_index(id), gate)
    }

    /// Capture a start-of-cycle [`ObarrayScanSnapshot`] for the Stage 1b concurrent
    /// obarray scan. MUST be called at the world-stopped start handshake (the same
    /// point the cons-block snapshot is taken), so `n_slots`/`n_chunks` are a
    /// consistent picture of the obarray at start. Chunk arrays + seq boxes never
    /// move, so the captured raw pointers stay valid for the whole cycle.
    pub(crate) fn scan_snapshot(
        &self,
        world: &crate::tagged::gc::scan_contract::SingleMutatorWorld<'_>,
    ) -> ObarrayScanSnapshot {
        let (chunks, n_slots) = self.symbols.snapshot_parts(world);
        ObarrayScanSnapshot {
            chunks,
            n_slots,
            heap_identity: world.heap_identity(),
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
            match sym.value_cell_acquire() {
                ValueCell::Plain(v) => {
                    if v != Value::UNBOUND {
                        push(v);
                    }
                }
                ValueCell::Alias(_) | ValueCell::Forwarded(_) | ValueCell::Localized(_) => {}
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
        let keyword = {
            let sym = self.ensure_slot(id);
            if sym.interned_global {
                return;
            }
            sym.interned_global = true;
            sym.set_interned(SymbolInterned::InternedInInitial);
            let name = resolve_sym_lisp_string(id);
            let keyword = name.as_bytes().first().is_some_and(|byte| *byte == b':');
            if keyword {
                // Match GNU lread.c intern_sym: keywords interned in the
                // initial obarray are self-evaluating constants and are marked
                // declared-special.
                sym.set_declared_special(true);
                sym.set_trapped_write(SymbolTrappedWrite::NoWrite);
            }
            keyword
        };
        if keyword
            && let Some(mut write) = self.cell_write(id)
            && let ArmMut::Plain(plain) = write.arm()
            // Only initialize if not already set (idempotent).
            && plain.value().is_unbound()
        {
            plain.store(Value::keyword_id(id));
        }
        self.global_member_count += 1;
        self.members_epoch += 1;
    }

    fn clear_global_member(&mut self, id: SymId) -> bool {
        let Some(sym) = self.slot_mut(id) else {
            return false;
        };
        if !sym.interned_global {
            return false;
        }
        sym.interned_global = false;
        sym.set_interned(SymbolInterned::Uninterned);
        self.global_member_count = self.global_member_count.saturating_sub(1);
        self.members_epoch += 1;
        true
    }

    /// GNU `set_symbol_trapped_write` as `Fadd_variable_watcher` and
    /// `Fremove_variable_watcher` call it (`src/data.c`): a watched variable
    /// is `SYMBOL_TRAPPED_WRITE`, and it returns to `SYMBOL_UNTRAPPED_WRITE`
    /// when its last watcher goes.  Constants keep `NoWrite` (their write
    /// check runs first, and no `let` can reach them), so on any bindable
    /// symbol the flag answers "does a watcher exist" from the slot already
    /// in hand -- GNU's `specbind`/`do_one_unbind` never probe a side table,
    /// and this port's watcher map is never empty (the display variables are
    /// watched from startup), so every bind and unbind used to hash into it.
    pub(crate) fn note_watchers_changed(&mut self, id: SymId, has_watchers: bool) {
        let sym = self.ensure_slot(id);
        match (sym.trapped_write(), has_watchers) {
            (SymbolTrappedWrite::Untrapped, true) => {
                sym.set_trapped_write(SymbolTrappedWrite::Trapped);
            }
            (SymbolTrappedWrite::Trapped, false) => {
                sym.set_trapped_write(SymbolTrappedWrite::Untrapped);
            }
            _ => {}
        }
    }

    #[inline]
    fn ensure_global_member_if_canonical(&mut self, id: SymId) {
        // Steady state is one presence-checked slot read: membership is a
        // slot property (GNU: `intern` sets it once), so ask the slot before
        // paying the epoch-checked interning lookup for a symbol that is
        // already a member.
        if self.slot(id).is_some_and(|s| s.interned_global) {
            return;
        }
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
            function_epoch: AtomicU64::new(0),
            members_epoch: 0,
            completion_order_cache: std::sync::Mutex::new(None),
            blvs: Vec::new(),
            value_fwds: Vec::new(),
            debug_on_next_call_fwd: Self::debug_on_next_call_unresolved(),
            max_lisp_eval_depth_localized: false,
            generation: next_obarray_generation(),
        };

        // Pre-intern fundamental symbols. Both `t` and `nil` are
        // self-referential constants in GNU.
        for (id, value) in [(intern("t"), Value::T), (intern("nil"), Value::NIL)] {
            if let ArmMut::Plain(plain) = ob.cell_write_ensure(id).arm() {
                plain.store(value);
            }
            let sym = ob.ensure_slot(id);
            sym.set_trapped_write(SymbolTrappedWrite::NoWrite);
            sym.set_declared_special(true);
            ob.mark_global_member(id);
        }

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

    /// Test hook: the slot count the compiled-code bounds check reads.
    #[cfg(test)]
    pub(crate) fn symbol_count_upper_bound_for_test(&self) -> usize {
        self.symbols.len()
    }

    /// Get symbol data by identity.
    pub fn get_by_id(&self, id: SymId) -> Option<&LispSymbol> {
        self.slot(id)
    }

    /// Get mutable symbol data by identity.
    pub fn get_mut_by_id(&mut self, id: SymId) -> Option<&mut LispSymbol> {
        self.slot_mut(id)
    }

    /// Copy the global value of a symbol.
    ///
    /// **This is not GNU's `Vfoo`.** For a symbol some buffer has localised it
    /// answers the BLV *defcell*, and for a `DEFVAR_PER_BUFFER` name it
    /// answers `None`; see [`BufferlessValue`] for why, and
    /// [`Self::value_in_buffer`] for the reader that does mirror GNU's C.
    ///
    /// The returned word does not borrow the mutable symbol or descriptor slot.
    #[inline]
    pub fn symbol_value_copied(&self, name: &str) -> Option<Value> {
        self.symbol_value_id_copied(intern(name))
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
    /// [`Self::symbol_value_copied`] on the overwhelmingly common global path, which
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
            .is_some_and(|sym| sym.redirect() == SymbolRedirect::Localized);
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
            let Some(target) = self.slot(current).and_then(LispSymbol::alias_target) else {
                return current;
            };
            current = target;
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
        match sym.value_cell() {
            // UNBOUND sentinel = unbound.
            ValueCell::Plain(value) => {
                if value.is_unbound() {
                    None
                } else {
                    Some(value)
                }
            }
            ValueCell::Alias(current) => self.symbol_value_id_copied_slow(current, 49),
            ValueCell::Localized(_) => {
                let value = self.blv(id)?.defcell.cons_cdr();
                if value.is_unbound() {
                    None
                } else {
                    Some(value)
                }
            }
            ValueCell::Forwarded(fwd) => fwd.load(),
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
            match sym.value_cell() {
                // UNBOUND sentinel = unbound.
                ValueCell::Plain(v) => {
                    if v.is_unbound() {
                        return None;
                    }
                    return Some(v);
                }
                ValueCell::Alias(target) => {
                    current = target;
                }
                ValueCell::Localized(_) => {
                    let value = self.blv(current)?.defcell.cons_cdr();
                    if value.is_unbound() {
                        return None;
                    }
                    return Some(value);
                }
                ValueCell::Forwarded(fwd) => {
                    return fwd.load();
                }
            }
        }
        None // alias cycle
    }

    /// Get a symbol's value by identity, returning nil when unbound.
    ///
    /// GNU's `find_symbol_value` returns a `Lisp_Object` directly; keeping
    /// hot evaluator reads in this shape avoids an extra Option path.
    #[inline(always)]
    pub fn symbol_value_id_or_nil(&self, id: SymId) -> Value {
        match self.symbol_value_id_copied(id) {
            Some(value) => value,
            None => Value::NIL,
        }
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

    /// GNU `SET_SYMBOL_VAL` for a caller that already holds
    /// `redirect == Plainval` for `id` (a `specbind` or `unbind_to` of a plain
    /// cell): the membership check, the seqlock bracket and the SATB pre-image
    /// note stay; the alias walk, slot growth and redirect re-arming of the
    /// general store do not apply.  `Value::UNBOUND` stores "unbound", exactly
    /// as `makunbound_id` leaves a plain cell.
    /// GNU `do_one_unbind`'s SPECPDL_LET arm and `do_specbind`'s untrapped
    /// case, as one obarray visit: refuse anything that is not an interned,
    /// plain, untrapped, unprojected cell (those keep the general paths), else
    /// store VALUE and return the value the cell held (`Value::UNBOUND` when
    /// it was unbound, which is what `makunbound` leaves).
    ///
    /// The caller holds no Lisp-visible state between the swap and its
    /// specpdl push/pop: nothing here runs Lisp or reaches a safe point, and
    /// the SATB pre-image note below keeps a concurrent mark exact.
    ///
    /// Off the concurrent mark (the common case) the slot is visited once,
    /// with no seqlock bracket and no SATB note, neither of which a swap
    /// needs then; the mark cannot start inside this call, which reaches no
    /// safe point.
    #[inline(always)]
    pub(crate) fn swap_plain_untrapped_value_id(
        &mut self,
        id: SymId,
        value: Value,
    ) -> Option<Value> {
        let gate = MarkGate::read();
        if gate.is_marking() {
            return self.swap_plain_untrapped_value_id_while_marking(id, value, gate);
        }
        let mut write = self.symbols.cell_write(Self::slot_index(id), gate)?;
        if !write.symbol().interned_global {
            return None;
        }
        let plain = write.plain_untrapped_unprojected()?;
        #[cfg(test)]
        note_plain_value_slot_visit();
        Some(plain.store(value))
    }

    /// [`Self::swap_plain_untrapped_value_id`] during a concurrent mark: the
    /// [`CellWrite`] brackets the store with the chunk's seqlock and notes
    /// the pre-image, so the snapshot-at-the-beginning mark stays exact.
    #[cold]
    #[inline(never)]
    fn swap_plain_untrapped_value_id_while_marking(
        &mut self,
        id: SymId,
        value: Value,
        gate: MarkGate,
    ) -> Option<Value> {
        let mut write = self.symbols.cell_write(Self::slot_index(id), gate)?;
        if !write.symbol().interned_global {
            return None;
        }
        let plain = write.plain_untrapped_unprojected()?;
        #[cfg(test)]
        note_plain_value_slot_visit();
        Some(plain.store(value))
    }

    /// [`Self::swap_plain_untrapped_value_id`] for a writer that does not
    /// want the old value: `false`, storing nothing, unless `id` is an
    /// interned, plain, untrapped, unprojected cell. Off the concurrent mark
    /// (the common case) the slot is visited once, with no seqlock bracket
    /// and no SATB note, neither of which a store needs then.
    #[inline(always)]
    pub(crate) fn set_plain_untrapped_value_id(&mut self, id: SymId, value: Value) -> bool {
        let gate = MarkGate::read();
        if gate.is_marking() {
            return self
                .swap_plain_untrapped_value_id_while_marking(id, value, gate)
                .is_some();
        }
        let Some(mut write) = self.symbols.cell_write(Self::slot_index(id), gate) else {
            return false;
        };
        if !write.symbol().interned_global {
            return false;
        }
        let Some(plain) = write.plain_untrapped_unprojected() else {
            return false;
        };
        #[cfg(test)]
        note_plain_value_slot_visit();
        plain.set(value);
        true
    }

    /// Whether a write to `id` could be a bare `SET_SYMBOL_VAL`: an interned,
    /// plain, untrapped, unprojected cell — the shape
    /// [`Self::swap_plain_untrapped_value_id`] accepts, asked without storing
    /// so a writer can run its own refusals first and so the writes that are
    /// not plain (buffer-local, forwarded, watched) leave after one load.
    #[inline]
    pub(crate) fn is_plain_value_cell_id(&self, id: SymId) -> bool {
        self.slot(id)
            .is_some_and(|sym| sym.flags().is_plain_untrapped_unprojected() && sym.interned_global)
    }

    /// Mark `id` as carrying a host projection (see
    /// `SymbolFlags::RUNTIME_PROJECTED_BIT`).  Armed at `Context`
    /// construction, not carried by a dump image.
    pub(crate) fn mark_runtime_projected_id(&mut self, id: SymId) {
        self.ensure_slot(id).set_runtime_projected(true);
    }

    /// GNU `SET_SYMBOL_VAL` on `id`'s cell -- watched, host-projected and
    /// non-member cells included -- when that cell is still `Plainval`.
    /// A cell some other arm took over (a watcher made the variable
    /// buffer-local, say) is left untouched and reported, so the caller
    /// re-dispatches the way GNU's `set_internal` does after its watchers
    /// (`src/data.c:1714-1718`): a plain store can no longer land in a
    /// buffer-local record or a forwarder's place.
    #[inline]
    pub(crate) fn store_plain_value_id(&mut self, id: SymId, value: Value) -> Result<(), NotPlain> {
        let idx = Self::slot_index(id);
        if !self.symbols.get(idx).is_some_and(|sym| sym.interned_global) {
            // Membership is marked before the store, for canonical ids only —
            // the old `ensure_global_member_if_canonical` prologue, now paid
            // only by the symbols that are not members yet.
            return self.store_plain_value_id_nonmember(id, value);
        }
        let gate = MarkGate::read();
        let Some(mut write) = self.symbols.cell_write(idx, gate) else {
            return Err(NotPlain);
        };
        #[cfg(test)]
        note_plain_value_slot_visit();
        write.plain().map(|plain| plain.set(value)).ok_or(NotPlain)
    }

    #[cold]
    #[inline(never)]
    fn store_plain_value_id_nonmember(&mut self, id: SymId, value: Value) -> Result<(), NotPlain> {
        if Self::is_canonical_symbol_id(id) {
            self.mark_global_member(id);
        }
        #[cfg(test)]
        note_plain_value_slot_visit();
        self.cell_write_ensure(id)
            .plain()
            .map(|plain| plain.set(value))
            .ok_or(NotPlain)
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
    /// the existing BLV record). `None` when the alias chain from `id` does
    /// not end (a cycle GNU's `indirect_variable` refuses): an alias cell
    /// cannot be localized.
    pub(crate) fn make_symbol_localized(&mut self, id: SymId, default: Value) -> Option<BlvPtr> {
        let target = self.resolve_alias_for_write(id);
        // Check existing state before mutating. GNU `make_blv` keeps the
        // forwarder when the symbol it localizes was SYMBOL_FORWARDED
        // (`src/data.c:2112-2140`, `blv->fwd = valcontents`), which is why a
        // per-buffer binding of a `DEFVAR_INT` variable is still an integer
        // slot and a per-buffer `DEFVAR_BOOL` still reads back `t`. Dropping
        // it here would disarm the type rule for the rest of the session the
        // first time any buffer made a local binding.
        let forwarder = match self.slot(target).map(LispSymbol::value_cell) {
            Some(ValueCell::Localized(existing)) => return Some(existing),
            Some(ValueCell::Alias(_)) => return None,
            Some(ValueCell::Forwarded(fwd)) => Some(fwd),
            Some(ValueCell::Plain(_)) | None => None,
        };
        // Build defcell = (sym . default). The same cons doubles as
        // valcell until per-buffer bindings are swapped in.
        let defcell = Value::cons(Value::from_sym_id(target), default);
        let blv = Box::new(LispBufferLocalValue {
            local_if_set: false,
            found: false,
            fwd: forwarder,
            where_buf: Value::NIL,
            where_buf_id: NO_WHERE_BUF,
            defcell,
            valcell: defcell,
            // 0 < the global epoch's initial 1: a fresh BLV never
            // fast-path-hits before its first swap_in records reality.
            alist_epoch: 0,
        });
        let record = BlvPtr::leak(blv);
        self.blvs.push(record);
        // The cell write notes the plain pre-image during a concurrent mark
        // (it only survives transitively if it equals `default`).
        match self.cell_write_ensure(target).arm() {
            ArmMut::Plain(plain) => plain.localize(record),
            ArmMut::Forwarded(forwarded) => forwarded.localize(record),
            ArmMut::Alias(_) | ArmMut::Localized(_) => {
                unreachable!("the arm was read above with no write since")
            }
        }
        if crate::emacs_core::intern::resolve_sym(target) == "max-lisp-eval-depth" {
            self.max_lisp_eval_depth_localized = true;
        }
        Some(record)
    }

    /// The forward descriptor installed on `id`, if the symbol is
    /// `SYMBOL_FORWARDED`. Mirrors GNU `SYMBOL_FWD` (`src/lisp.h:1082`).
    pub fn forwarder(&self, id: SymId) -> Option<&'static crate::emacs_core::forward::LispFwd> {
        self.slot(id)?.forwarded_descriptor()
    }

    /// Which `Lisp_Fwd` variant a symbol forwards through, if it forwards.
    ///
    /// GNU spells this as a chain of `BUFFER_OBJFWDP` / `KBOARD_OBJFWDP`
    /// predicates over `SYMBOL_FWD (sym)`; handing back the closed
    /// [`LispFwdType`](crate::emacs_core::forward::LispFwdType) instead means a
    /// caller that cares about one variant has to say what it does about the
    /// others.
    pub fn forward_type(&self, id: SymId) -> Option<crate::emacs_core::forward::LispFwdType> {
        self.forwarder(id).map(|fwd| fwd.ty())
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
        match self.slot(id)?.value_cell() {
            ValueCell::Forwarded(fwd) => fwd.as_bool_fwd(),
            ValueCell::Localized(_) => self.blv(id)?.fwd?.as_bool_fwd(),
            ValueCell::Plain(_) | ValueCell::Alias(_) => None,
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
        self.debug_on_next_call_bool_fwd_cached()
            .or_else(|| self.debug_on_next_call_bool_fwd_slow(id))
    }

    /// The memoized descriptor of [`Self::debug_on_next_call_bool_fwd`], when
    /// resolved: for a caller that names the symbol only on the miss (its
    /// `OnceLock` read was most of the per-call check).  `None` while the
    /// pointer names a stand-in.
    #[inline(always)]
    pub(crate) fn debug_on_next_call_bool_fwd_cached(
        &self,
    ) -> Option<&'static crate::emacs_core::forward::LispBoolFwd> {
        let cell = self.debug_on_next_call_cell_ref();
        (!cell.is_debug_on_next_call_stand_in()).then_some(cell)
    }

    /// GNU's bare `if (debug_on_next_call)` (`src/bytecode.c:798`): one
    /// pointer load and one byte test, no null check. True also while the
    /// cell is unresolved (the stand-in reads armed), which only sends the
    /// caller to the reference path that resolves it; see the field.
    #[inline(always)]
    pub(crate) fn debug_on_next_call_armed_fast(&self) -> bool {
        self.debug_on_next_call_cell_ref().get()
    }

    /// The cell the pointer names: a leaked descriptor or a stand-in.
    #[inline(always)]
    fn debug_on_next_call_cell_ref(&self) -> &'static crate::emacs_core::forward::LispBoolFwd {
        let cell = self
            .debug_on_next_call_fwd
            .load(std::sync::atomic::Ordering::Relaxed);
        // Safety: never null. Every store puts a `Box::leak`ed descriptor or
        // one of the two `'static` stand-ins here (the initializers, the slow
        // path below, and the install hooks).
        unsafe { &*cell }
    }

    /// A pointer word naming the unresolved stand-in: every constructor's
    /// initial value.
    fn debug_on_next_call_unresolved()
    -> std::sync::atomic::AtomicPtr<crate::emacs_core::forward::LispBoolFwd> {
        std::sync::atomic::AtomicPtr::new(
            std::ptr::from_ref(&crate::emacs_core::forward::DEBUG_ON_NEXT_CALL_UNRESOLVED)
                .cast_mut(),
        )
    }

    /// Point the memoized cell at FWD, the descriptor now canonical for
    /// `debug-on-next-call` in this obarray.
    fn set_debug_on_next_call_cell(&self, fwd: &'static crate::emacs_core::forward::LispBoolFwd) {
        self.debug_on_next_call_fwd.store(
            std::ptr::from_ref(fwd).cast_mut(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    #[cold]
    #[inline(never)]
    fn debug_on_next_call_bool_fwd_slow(
        &self,
        id: SymId,
    ) -> Option<&'static crate::emacs_core::forward::LispBoolFwd> {
        let Some(fwd) = self.bool_forwarder(id) else {
            // No `DEFVAR_BOOL`: the missing cell reads disarmed, as `None`
            // always meant. A descriptor installed later replaces this
            // through the install hooks.
            self.set_debug_on_next_call_cell(
                &crate::emacs_core::forward::DEBUG_ON_NEXT_CALL_ABSENT,
            );
            return None;
        };
        self.set_debug_on_next_call_cell(fwd);
        Some(fwd)
    }

    /// The install hooks' test: is ID `debug-on-next-call`?
    fn is_debug_on_next_call_symbol(id: SymId) -> bool {
        static SYMBOL: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
        id == *SYMBOL.get_or_init(|| intern("debug-on-next-call"))
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
        match self.slot(id)?.value_cell() {
            ValueCell::Forwarded(fwd) => fwd.as_int_fwd(),
            ValueCell::Localized(_) => self.blv(id)?.fwd?.as_int_fwd(),
            ValueCell::Plain(_) | ValueCell::Alias(_) => None,
        }
    }

    /// Set the `local_if_set` flag on a LOCALIZED symbol's BLV. Used
    /// by `make-variable-buffer-local` (Phase 6) which differs from
    /// `make-local-variable` only in this flag. Phase 4 exposes the
    /// helper so the LOCALIZED tests can flip it directly.
    pub fn set_blv_local_if_set(&mut self, id: SymId, local_if_set: bool) {
        let target = self.resolve_alias_for_write(id);
        if let Some(blv) = self.blv_mut(target) {
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
            .is_some_and(|sym| sym.redirect() == SymbolRedirect::Localized)
    }

    pub fn blv(&self, id: SymId) -> Option<&LispBufferLocalValue> {
        let blv = self.slot(id)?.localized_blv()?;
        // Safety: a `Localized` cell names a record make_symbol_localized
        // allocated and self.blvs owns. It stays valid for &self's
        // lifetime because Drop can't run while we hold &self.
        Some(unsafe { &*blv.as_ptr() })
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
    /// [`Self::read_localized`] for a caller that holds the buffer's ID.
    ///
    /// GNU's `swap_in_symval_forwarding` (`data.c:1574-1604`) decides the
    /// same-buffer question with a pointer compare against the last-swapped
    /// buffer.  Ours compared two `Value`s, so every read first built one
    /// with `Value::make_buffer`, and that goes through the thread-local heap
    /// and a registry lookup -- 52 instructions to answer a question the
    /// caller already had the ID for.  The miss path still needs the `Value`
    /// and constructs it there.
    pub fn read_localized_for_buffer(
        &self,
        id: SymId,
        target_buf_id: crate::buffer::BufferId,
        target_alist: Value,
    ) -> Option<Value> {
        let sym = self.slot(id)?;
        self.read_localized_symbol_for_buffer(id, sym, target_buf_id, target_alist)
    }

    /// [`Self::read_localized_for_buffer`] for a caller that already holds
    /// the symbol's slot (it dispatched on its redirect): the obarray lookup
    /// is not repeated. `None` unless `sym` is LOCALIZED.
    #[inline]
    pub(crate) fn read_localized_symbol_for_buffer(
        &self,
        id: SymId,
        sym: &LispSymbol,
        target_buf_id: crate::buffer::BufferId,
        target_alist: Value,
    ) -> Option<Value> {
        let blv_ptr = sym.localized_blv()?.as_ptr();
        let epoch = blv_alist_epoch();
        // SAFETY: identical to `read_localized` -- the BLV record is reached
        // only through the symbol's raw pointer and the evaluator thread is
        // its only writer; no reference is held across a write.
        unsafe {
            if (*blv_ptr).alist_epoch == epoch && (*blv_ptr).where_buf_id == target_buf_id.0 {
                debug_assert_eq!(
                    (*blv_ptr)
                        .where_buf
                        .as_buffer_id()
                        .map_or(NO_WHERE_BUF, |b| b.0),
                    (*blv_ptr).where_buf_id,
                    "a BLV writer bypassed set_where"
                );
                return Some((*blv_ptr).valcell.cons_cdr());
            }
        }
        self.read_localized(id, Value::make_buffer(target_buf_id), target_alist)
    }

    pub fn read_localized(
        &self,
        id: SymId,
        target_buf: Value,
        target_alist: Value,
    ) -> Option<Value> {
        self.read_localized_with_lookup(id, target_buf, || {
            assq(Value::from_sym_id(id), target_alist)
        })
    }

    /// Resolve through the buffer's canonical binding index when its owner
    /// is available. A cache miss must not scan unrelated binding conses:
    /// dependency observers would otherwise subscribe to their value writes.
    pub(crate) fn read_localized_in_buffer(
        &self,
        id: SymId,
        buffer: &crate::buffer::Buffer,
    ) -> Option<Value> {
        self.read_localized_with_lookup(id, Value::make_buffer(buffer.id), || {
            buffer.local_variable_binding_cell(id).unwrap_or(Value::NIL)
        })
    }

    fn read_localized_with_lookup(
        &self,
        id: SymId,
        target_buf: Value,
        binding: impl FnOnce() -> Value,
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
            let found_cell = binding();
            let found = !found_cell.is_nil();
            let valcell = if found {
                found_cell
            } else {
                (*blv_ptr).defcell
            };
            (*blv_ptr).set_where(target_buf);
            (*blv_ptr).found = found;
            store_value_atomic(&mut (*blv_ptr).valcell, valcell);
            (*blv_ptr).alist_epoch = epoch;
            Some(valcell.cons_cdr())
        }
    }

    /// Raw pointer to a LOCALIZED symbol's BLV record (see `read_localized`
    /// for the aliasing contract), `None` for any other redirect.
    fn blv_ptr(&self, id: SymId) -> Option<*mut LispBufferLocalValue> {
        Some(self.slot(id)?.localized_blv()?.as_ptr())
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
        let blv = self.slot(id)?.localized_blv()?;
        // Safety: same rationale as `blv`. The mutable borrow follows
        // from `&mut self`.
        Some(unsafe { &mut *blv.as_ptr() })
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
        self.forward_cell(id, fwd.header());
    }

    /// GNU `defvar_int` / `defvar_bool` / `defvar_lisp` /
    /// `defvar_per_buffer` / `defvar_kboard` (`src/lread.c`,
    /// `src/buffer.c`): make `id` a `SYMBOL_FORWARDED` variable through FWD,
    /// declared special. A plain cell, or one that already forwards, takes
    /// the descriptor; the cell write notes a plain pre-image during a
    /// concurrent mark. GNU declares its C variables before Lisp can alias
    /// or localize them, so an alias or a localized cell is left as it is
    /// (`false`): a late declaration of a localized variable goes through
    /// [`Self::reattach_localized_forwarder`] instead, and replacing the arm
    /// would orphan the alias edge or the buffer-local record.
    fn forward_cell(
        &mut self,
        id: SymId,
        fwd: &'static crate::emacs_core::forward::LispFwd,
    ) -> bool {
        let mut write = self.cell_write_ensure(id);
        let installed = match write.arm() {
            ArmMut::Plain(plain) => {
                plain.forward_to(fwd);
                true
            }
            ArmMut::Forwarded(forwarded) => {
                forwarded.forward_to(fwd);
                true
            }
            ArmMut::Alias(_) | ArmMut::Localized(_) => false,
        };
        if installed {
            write.set_declared_special(true);
        }
        debug_assert!(
            installed,
            "a C variable is declared after Lisp aliased or localized it: {id:?}"
        );
        installed
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
        if !self.forward_cell(id, fwd.header()) {
            return;
        }
        // This descriptor is now the variable's cell: the memoized pointer
        // follows it (an earlier probe may have found none, or a different
        // descriptor).
        if Self::is_debug_on_next_call_symbol(id) {
            self.set_debug_on_next_call_cell(fwd);
        }
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
                let fwd = crate::emacs_core::forward::alloc_boolfwd(flag).header();
                (fwd, if flag { Value::T } else { Value::NIL })
            }
            Kind::Int => {
                let integer = crate::emacs_core::forward::LispInteger::check(restored)
                    .unwrap_or_else(|_| crate::emacs_core::forward::LispInteger::from_i64(0));
                let fwd = crate::emacs_core::forward::alloc_intfwd(integer).header();
                (fwd, integer.value())
            }
            // A `Lisp_Fwd_Obj` accepts anything and canonicalises nothing, so
            // the BLV's default comes back unchanged; the descriptor exists
            // for the redirect tag, which is what refuses an unbind through
            // `blv->fwd` (`src/data.c:1723-1727`).
            Kind::Obj => {
                let fwd = crate::emacs_core::forward::alloc_objfwd(restored).header();
                (fwd, restored)
            }
            Kind::Kboard => {
                let fwd = crate::emacs_core::forward::alloc_kboard_objfwd(restored).header();
                (fwd, restored)
            }
        };
        blv.fwd = Some(fwd);
        blv.defcell.set_cdr(canonical);
        if super::value::eq_value(&blv.valcell, &blv.defcell) {
            blv.valcell.set_cdr(canonical);
        }
        // The BLV's new descriptor is the variable's cell now; the memoized
        // pointer follows it (see `install_boolfwd`).
        if Self::is_debug_on_next_call_symbol(id)
            && let Some(bool_fwd) = fwd.as_bool_fwd()
        {
            self.set_debug_on_next_call_cell(bool_fwd);
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
        if !self.forward_cell(id, fwd.header()) {
            return;
        }
        self.register_value_fwd(fwd.header());
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
        if !self.forward_cell(id, fwd.header()) {
            return;
        }
        self.register_value_fwd(fwd.header());
    }

    /// Install a GNU `Lisp_Kboard_Objfwd`-equivalent descriptor on a symbol
    /// (`src/lread.c:5291-5298`, `defvar_kboard`).
    pub fn install_kboard_objfwd(
        &mut self,
        id: SymId,
        fwd: &'static crate::emacs_core::forward::LispKboardObjFwd,
    ) {
        if !self.forward_cell(id, fwd.header()) {
            return;
        }
        self.register_value_fwd(fwd.header());
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
            match sym.value_cell() {
                ValueCell::Plain(v) => {
                    // UNBOUND sentinel means void.
                    if v.is_unbound() {
                        return None;
                    }
                    return Some(v);
                }
                ValueCell::Alias(target) => {
                    current = target;
                    continue;
                }
                ValueCell::Localized(_) => {
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
                    // Use the safe `Obarray::blv` accessor so this code
                    // path stays out of `unsafe` blocks.
                    return self.blv(current).map(|blv| blv.defcell.cons_cdr());
                }
                ValueCell::Forwarded(fwd) => {
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
                    if let Some(value) = fwd.load() {
                        return Some(value);
                    }
                    // `load` answers for every family but the per-buffer slot.
                    return fwd.as_buffer_obj_fwd().map(|buf_fwd| buf_fwd.default);
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
            match self.slot(current)?.value_cell() {
                ValueCell::Plain(_) => {
                    return self.find_symbol_value(current);
                }
                ValueCell::Alias(next) => {
                    current = next;
                    continue;
                }
                ValueCell::Localized(_) => {
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
                ValueCell::Forwarded(fwd) => {
                    // Phase 8a: read through the forwarder descriptor.
                    // Phase 10D: dispatch on `local_flags_idx`.
                    // Always-local slots (`-1`) read `slots[off]`
                    // unconditionally; conditional slots (`>= 0`)
                    // gate the read on `local_flags`'s bit and fall
                    // through to `buffer_defaults` when clear.
                    // Mirrors GNU `do_symval_forwarding` BUFFER_OBJFWD
                    // arm + `PER_BUFFER_VALUE_P` (`buffer.h:1640`).
                    use crate::emacs_core::forward::ForwardSlot;
                    match fwd.slot() {
                        ForwardSlot::BufferObj(buf_fwd) => {
                            // Shared with the cached read tier
                            // (`Context::read_var_cached`), so the two
                            // cannot drift.
                            return Some(buf_fwd.value_in(
                                current_buffer_slots,
                                current_buffer_local_flags,
                                buffer_defaults,
                            ));
                        }
                        // `Int`, `Bool`, `Obj` and `KboardObj` keep their
                        // storage in the descriptor, so none of the buffer
                        // context this function was handed applies to them;
                        // the buffer-free walk reads them through
                        // `do_symval_forwarding` and is the whole answer.
                        ForwardSlot::Int(_)
                        | ForwardSlot::Bool(_)
                        | ForwardSlot::Obj(_)
                        | ForwardSlot::KboardObj(_) => {
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
    pub(crate) fn set_internal_localized(
        &mut self,
        sym_id: SymId,
        value: Value,
        target_buf: Value,
        target_alist: Value,
        bindflag: SetInternalBind,
        let_shadows: bool,
    ) -> SetInternalAlist {
        self.set_internal_localized_with(sym_id, value, target_buf, target_alist, bindflag, || {
            let_shadows
        })
    }

    /// [`Self::set_internal_localized`] asking `let_shadows` only when it
    /// decides something: a `Set` of a `local_if_set` variable the target
    /// buffer has no binding for.  GNU `set_internal` calls
    /// `let_shadows_buffer_binding_p` -- a walk of the whole specpdl -- only
    /// there too; asking on every write made a `setq` of a buffer-local
    /// variable O(stack depth).
    pub(crate) fn set_internal_localized_with(
        &mut self,
        sym_id: SymId,
        value: Value,
        target_buf: Value,
        target_alist: Value,
        bindflag: SetInternalBind,
        let_shadows: impl FnOnce() -> bool,
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
        blv.set_where(target_buf);
        blv.alist_epoch = epoch;
        blv.found = true;

        if cell.is_nil() {
            // No existing binding for this buffer.
            let auto_create =
                bindflag == SetInternalBind::Set && blv.local_if_set && !let_shadows();
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
        let blv = match self.cell_write_ensure(target).arm() {
            ArmMut::Localized(local) => local.blv(),
            ArmMut::Forwarded(forwarded) => {
                let fwd = forwarded.descriptor();
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
                return;
            }
            ArmMut::Plain(plain) => {
                plain.store(value);
                return;
            }
            // `resolve_alias_for_write` stops on an alias only when the chain
            // does not end; the write lands here as it always has.
            ArmMut::Alias(alias) => {
                alias.unalias(value);
                return;
            }
        };
        // LOCALIZED: write to BLV defcell (the default). The cell keeps
        // its arm -- replacing it would orphan the BLV record.
        // Safety: a `Localized` cell names a record owned by self.blvs.
        unsafe {
            let blv = &mut *blv.as_ptr();
            blv.defcell.set_cdr(value);
            // If the BLV cache is currently swapped to defcell
            // (no per-buffer entry loaded), mirror the new value
            // through valcell as well so subsequent reads
            // observe it without re-swapping.
            if super::value::eq_value(&blv.valcell, &blv.defcell) {
                blv.valcell.set_cdr(value);
            }
        }
    }

    /// Visit each stored symbol value cell that currently holds a `Value`.
    ///
    /// Phase F: reads from the redirect union (`val`) rather than the
    /// legacy `value` enum field. Visits Plainval symbols (non-UNBOUND)
    /// and BLV defcell defaults (for Localized symbols).
    pub fn for_each_value_cell_mut(&mut self, mut f: impl FnMut(&mut Value)) {
        for idx in 0..self.symbols.len() {
            let Some(mut write) = self.symbols.cell_write(idx, MarkGate::read()) else {
                continue;
            };
            match write.arm() {
                ArmMut::Plain(plain) => {
                    let mut v = plain.value();
                    if v != Value::UNBOUND {
                        // The store notes the pre-image during a concurrent
                        // mark, as an in-place mutation must.
                        f(&mut v);
                        plain.store(v);
                    }
                }
                ArmMut::Localized(local) => {
                    // Visit the BLV defcell default. Route the write back through
                    // `set_cdr` so the heap SATB barrier logs the old cdr — the
                    // original raw-pointer write to the defcell cons bypassed it.
                    // Safety: a `Localized` cell names a record self.blvs owns.
                    unsafe {
                        let blv = &mut *local.blv().as_ptr();
                        let mut cdr = blv.defcell.cons_cdr();
                        if cdr != Value::UNBOUND {
                            f(&mut cdr);
                            blv.defcell.set_cdr(cdr);
                        }
                    }
                }
                ArmMut::Alias(_) | ArmMut::Forwarded(_) => {}
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
            match self.slot(current).and_then(LispSymbol::alias_target) {
                Some(target) => current = target,
                None => return current,
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
        // See `set_symbol_function_id': the same value redefines nothing.
        let unchanged = !sym.function_unbound && sym.function.bits() == function.bits();
        let previous = sym.function;
        // SATB: retain the function cell's pre-image during a concurrent mark.
        crate::tagged::gc::note_root_overwrite(sym.function);
        store_value_atomic(&mut sym.function, function);
        sym.function_unbound = false;
        if !unchanged {
            self.note_function_redefined(id, FunctionEpochBump::InternalCellWrite, previous);
        } else {
            note_function_cell_unchanged();
        }
    }

    /// Record that function-call behavior changed for EVERY symbol without a
    /// cell write -- the static subr table (`register_global_subr_entry`)
    /// rewrites a subr's fn pointer/arity in place, invisibly to the cells
    /// (and to every other cell holding the same `#<subr>`), and the
    /// compiler-overrides toggle changes what every symbol resolves to. The
    /// clock moves, keeping `function_epoch` a complete "any function binding
    /// may have changed" signal, and the per-symbol stamp floor rises to the
    /// new value, so [`Self::fn_unchanged_since`] proves nothing validated
    /// before it. `why` is observability only (the JIT's epoch report counts
    /// per reason).
    pub(crate) fn invalidate_all_function_bindings(&mut self, why: FunctionEpochBump) {
        crate::emacs_core::subr::leaf::debug_assert_no_leaf_active!("a function-epoch bump");
        let epoch = self.next_function_epoch();
        // Floor first, then the clock (the publication order, `fn_stamps`).
        self.symbols.raise_fn_floor(epoch);
        self.function_epoch.store(epoch, Ordering::Release);
        crate::emacs_core::eval::note_function_epoch_move(why, None);
        #[cfg(feature = "jit")]
        crate::emacs_core::jit::stats::note_function_epoch_bump(why, None);
    }

    /// The value `function_epoch` moves to next: one more, skipping the
    /// reserved `u64::MAX`.
    fn next_function_epoch(&self) -> u64 {
        let current = self.function_epoch();
        let next = current.wrapping_add(1);
        // u64::MAX is RESERVED as the JIT/AOT spec DISARMED sentinel
        // (jit::compile::SPEC_EPOCH_DISARMED); a live epoch must never equal it or
        // a legitimately-armed spec slot would read as disarmed. Skip it on the
        // (astronomically unreachable) wrap, which would also make the
        // per-symbol stamps non-monotone (design I3).
        let next = if next == u64::MAX { 0 } else { next };
        // Wrap breaks stamp monotonicity. Preserve the existing, explicit
        // test of sentinel skipping, while catching it in production debug
        // builds rather than silently proving an old binding current.
        debug_assert!(
            next > current || (cfg!(test) && current == u64::MAX - 1),
            "the function-binding clock wrapped"
        );
        next
    }

    /// Whether `id`'s function binding is provably the one it held when the
    /// function clock read `since`: it has not changed since, and no change
    /// of every symbol's resolution happened since (design §4.1, I1-I4). The
    /// caller must have loaded the clock BEFORE calling, and record THAT
    /// value as its entry's new epoch (see `fn_stamps` for why that order
    /// keeps a resync racing a redefinition sound). `since == u64::MAX` (every
    /// cache's EMPTY / DISARMED sentinel) never validates. Always `false`
    /// unless `NEOVM_FN_STAMPS` is on. Cold paths only: hot paths keep their
    /// one compare against the clock and ask this after it fails.
    #[inline]
    pub(crate) fn fn_unchanged_since(&self, id: SymId, since: u64) -> bool {
        fn_stamps::fn_stamps_enabled()
            && self.symbols.fn_unchanged_since(Self::slot_index(id), since)
    }

    /// Test hook: whether `id`'s chunk has allocated its stamp array.
    #[cfg(test)]
    pub(crate) fn chunk_has_fn_stamps_for_test(&self, id: SymId) -> bool {
        self.symbols.chunk_has_fn_stamps(Self::slot_index(id))
    }

    /// A specific function `id` was redefined (cell write / fmakunbound): bump the
    /// epoch (the coarse "any binding may have changed" signal JIT call
    /// speculation re-arms on). When JIT is enabled, also evict the JIT cache
    /// entries of callers that INLINED `id` -- the only invalidation inlined
    /// callees get, so every function-cell write must come through here (see
    /// jit::cache::evict_inline_dependents). `why` only feeds the JIT's
    /// per-reason bump counters and the cconv memo's verify report; it
    /// changes nothing else. `previous` is what
    /// the cell held: a compiled activation called through `id` may still be
    /// running it, and its frame records only the symbol, so the JIT keeps it
    /// alive while such a frame lives (`jit::cache::pin_redefined_function`;
    /// GNU's bytecode frame holds its `fun`, src/bytecode.c:518).
    fn note_function_redefined(&mut self, id: SymId, why: FunctionEpochBump, previous: Value) {
        let epoch = self.next_function_epoch();
        // The symbol's stamp first, then the clock (the publication order,
        // `fn_stamps`): every cache entry made before this change fails its
        // clock compare, and then its per-symbol test, for exactly `id`.
        self.symbols.stamp_function(Self::slot_index(id), epoch);
        self.function_epoch.store(epoch, Ordering::Release);
        crate::emacs_core::eval::note_function_epoch_move(why, Some(id));
        #[cfg(feature = "jit")]
        {
            crate::emacs_core::jit::stats::note_function_epoch_bump(why, Some(id));
            crate::emacs_core::jit::cache::evict_inline_dependents(id);
            crate::emacs_core::jit::cache::pin_redefined_function(id, previous);
        }
        #[cfg(not(feature = "jit"))]
        let _ = (id, why, previous);
    }

    /// Set the function cell of a symbol by identity.
    pub fn set_symbol_function_id(&mut self, id: SymId, function: Value) {
        self.set_symbol_function_id_for(id, function, FunctionEpochBump::InternalCellWrite);
    }

    /// [`Self::set_symbol_function_id`] attributing the epoch bump to `why`
    /// (the Lisp-level writers: `fset`, `defalias`, the pdump restore).
    pub(crate) fn set_symbol_function_id_for(
        &mut self,
        id: SymId,
        function: Value,
        why: FunctionEpochBump,
    ) {
        self.ensure_global_member_if_canonical(id);
        let sym = self.ensure_symbol_id(id);
        // Storing the value the cell already holds changes no call's
        // behavior, so it redefines nothing: no epoch move, no JIT eviction
        // (a `defalias' re-run while a file reloads, an `fset' in a loop).
        let unchanged = !sym.function_unbound && sym.function.bits() == function.bits();
        let previous = sym.function;
        // SATB: retain the function cell's pre-image during a concurrent mark.
        crate::tagged::gc::note_root_overwrite(sym.function);
        store_value_atomic(&mut sym.function, function);
        sym.function_unbound = false;
        if !unchanged {
            self.note_function_redefined(id, why, previous);
        } else {
            note_function_cell_unchanged();
        }
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
        let previous = sym.function;
        sym.function_unbound = true;
        // SATB: retain the function cell's pre-image during a concurrent mark.
        crate::tagged::gc::note_root_overwrite(sym.function);
        store_value_atomic(&mut sym.function, Value::NIL);
        if !was_unbound || was_bound_function {
            self.note_function_redefined(id, FunctionEpochBump::Fmakunbound, previous);
        }
    }

    /// Remove function cell without marking as explicitly unbound.
    /// Used for init-time masking of lazily-materialized builtins.
    pub fn clear_function_silent(&mut self, name: &str) {
        self.clear_function_silent_id(intern(name));
    }

    /// Remove function cell without marking as explicitly unbound, by identity.
    pub fn clear_function_silent_id(&mut self, id: SymId) {
        let mut redefined = None;
        if let Some(sym) = self.slot_mut(id)
            && !sym.function.is_nil()
        {
            let previous = sym.function;
            // SATB: retain the function cell's pre-image during a concurrent mark.
            crate::tagged::gc::note_root_overwrite(sym.function);
            store_value_atomic(&mut sym.function, Value::NIL);
            redefined = Some(previous);
        }
        if let Some(previous) = redefined {
            self.note_function_redefined(id, FunctionEpochBump::SilentClear, previous);
        }
    }

    /// Remove the value cell (makunbound).
    pub fn makunbound(&mut self, name: &str) {
        self.makunbound_id(intern(name));
    }

    /// Remove the value cell by identity: the store GNU's `set_internal`
    /// makes for `Qunbound` when no buffer binding takes it
    /// (`src/data.c:1714-1795`). Follows alias chains (max 50 hops).
    ///
    /// - A plain cell becomes void (`SET_SYMBOL_VAL (sym, Qunbound)`).
    /// - A buffer-local variable keeps its arm and its record: its DEFAULT
    ///   becomes void, which is what `set_internal` stores into
    ///   `blv->defcell` for a buffer with no binding of its own. (Replacing
    ///   the arm used to orphan the record and every other buffer's binding.)
    /// - A forwarded variable, or a buffer-local one with a forwarder, is
    ///   left alone: GNU refuses "Built-in variable may not be unbound"
    ///   (`src/data.c:1723-1727`, `:1802-1809`), which the evaluator signals
    ///   before it gets here (`check_forwarded_unbind`).
    /// - A constant is left alone.
    pub fn makunbound_id(&mut self, id: SymId) {
        self.ensure_global_member_if_canonical(id);
        let target = self.resolve_alias_for_write(id);
        if self
            .slot(target)
            .is_none_or(|sym| sym.trapped_write() == SymbolTrappedWrite::NoWrite)
        {
            return;
        }
        let blv = match self.cell_write_ensure(target).arm() {
            ArmMut::Plain(plain) => {
                plain.store(Value::UNBOUND);
                return;
            }
            // `resolve_alias_for_write` stops on an alias only when the chain
            // does not end; GNU `Fmakunbound` on an alias undoes it
            // (`src/data.c:781-784`).
            ArmMut::Alias(alias) => {
                alias.unalias(Value::UNBOUND);
                return;
            }
            ArmMut::Forwarded(_) => return,
            ArmMut::Localized(local) => local.blv(),
        };
        // Safety: a `Localized` cell names a record self.blvs owns.
        let blv = unsafe { &mut *blv.as_ptr() };
        if blv.fwd.is_some() {
            return;
        }
        blv.defcell.set_cdr(Value::UNBOUND);
        if super::value::eq_value(&blv.valcell, &blv.defcell) {
            blv.valcell.set_cdr(Value::UNBOUND);
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
            match s.value_cell() {
                ValueCell::Plain(v) => {
                    return v != Value::UNBOUND;
                }
                ValueCell::Alias(target) => {
                    current = target;
                }
                ValueCell::Localized(_) => {
                    // Bound if the BLV defcell has a non-UNBOUND default.
                    return self
                        .blv(current)
                        .is_some_and(|blv| blv.defcell.cons_cdr() != Value::UNBOUND);
                }
                ValueCell::Forwarded(_) => {
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
        SymbolPropertyRevision::changed(symbol);
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
        SymbolPropertyRevision::changed(symbol);
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
        SymbolPropertyRevision::changed(symbol);
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
        SymbolPropertyRevision::changed(symbol);
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
        self.ensure_symbol_id(id).set_declared_special(true);
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
        self.ensure_symbol_id(id).set_declared_special(true);
    }

    /// Clear the special flag on a symbol.
    pub fn make_non_special(&mut self, name: &str) {
        let id = intern(name);
        self.mark_global_member(id);
        self.ensure_symbol_id(id).set_declared_special(false);
    }

    /// Clear the special flag on a symbol by identity.
    pub fn make_non_special_id(&mut self, id: SymId) {
        self.ensure_global_member_if_canonical(id);
        self.ensure_symbol_id(id).set_declared_special(false);
    }

    /// Check if a symbol is special.
    pub fn is_special(&self, name: &str) -> bool {
        self.is_special_id(intern(name))
    }

    /// Check if a symbol is special by identity.
    pub fn is_special_id(&self, id: SymId) -> bool {
        self.slot(id).is_some_and(|s| s.flags().declared_special())
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
                .is_some_and(|s| s.trapped_write() == SymbolTrappedWrite::NoWrite)
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
    ///
    /// A plain cell or an existing alias takes the edge (the cell write notes
    /// a plain pre-image during a concurrent mark); a forwarded or
    /// buffer-local cell is refused with GNU `Fdefvaralias`'s reason
    /// (`src/eval.c:665-679`) and left as it is.
    pub fn make_alias(&mut self, id: SymId, target: SymId) -> Result<(), MakeAliasError> {
        match self.cell_write_ensure(id).arm() {
            ArmMut::Plain(plain) => plain.alias_to(target),
            ArmMut::Alias(alias) => alias.alias_to(target),
            ArmMut::Forwarded(_) => return Err(MakeAliasError::Forwarded),
            ArmMut::Localized(_) => return Err(MakeAliasError::Localized),
        }
        Ok(())
    }

    /// Check whether a symbol is a buffer-local variable in the obarray.
    pub fn is_buffer_local(&self, name: &str) -> bool {
        self.is_buffer_local_id(intern(name))
    }

    /// Check whether a symbol is a buffer-local variable by identity.
    /// Phase F: uses the redirect tag rather than the legacy value enum.
    pub fn is_buffer_local_id(&self, id: SymId) -> bool {
        self.slot(id)
            .is_some_and(|s| s.redirect() == SymbolRedirect::Localized)
    }

    /// Check whether a symbol is an alias by identity. Reads through the
    /// new redirect tag (Phase 3 of the symbol-redirect refactor).
    pub fn is_alias_id(&self, id: SymId) -> bool {
        self.slot(id)
            .is_some_and(|s| s.redirect() == SymbolRedirect::Varalias)
    }

    /// Remove a variable alias without following it and leave SYMBOL void.
    /// Mirrors GNU `internal-delete-indirect-variable`: the alias symbol is
    /// restored to `SYMBOL_PLAINVAL` with `Qunbound` in its value cell.
    /// A symbol that is not an alias is left as it is (GNU signals "Cannot
    /// undeclare a variable that is not an alias", which the subr checks
    /// first).
    pub fn delete_variable_alias_id(&mut self, id: SymId) {
        self.ensure_global_member_if_canonical(id);
        match self.cell_write_ensure(id).arm() {
            ArmMut::Alias(alias) => alias.unalias(Value::UNBOUND),
            ArmMut::Plain(_) | ArmMut::Localized(_) | ArmMut::Forwarded(_) => {}
        }
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
            // (No slot, or any other arm, means "not an alias".)
            let Some(next) = self.slot(slow).and_then(LispSymbol::alias_target) else {
                return Some(slow);
            };
            slow = next;

            // Hare: advance two hops (or stop if not an alias).
            for _ in 0..2 {
                let Some(next) = self.slot(fast).and_then(LispSymbol::alias_target) else {
                    return Some(slow);
                };
                fast = next;
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
        // Install the alias edge.
        self.make_alias(new_alias, base)?;
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
            && sym.trapped_write() == SymbolTrappedWrite::NoWrite
        {
            return Err(MakeAliasError::Constant);
        }

        // Walk the base chain looking for new_alias.
        let mut current = base;
        loop {
            if current == new_alias {
                return Err(MakeAliasError::Cycle);
            }
            let Some(next) = self.slot(current).and_then(LispSymbol::alias_target) else {
                break;
            };
            current = next;
        }

        if let Some(sym) = self.slot(new_alias) {
            match sym.value_cell() {
                ValueCell::Forwarded(_) => return Err(MakeAliasError::Forwarded),
                ValueCell::Localized(_) => return Err(MakeAliasError::Localized),
                ValueCell::Plain(_) | ValueCell::Alias(_) => {}
            }
        }
        Ok(())
    }

    /// Copy the global default, including a per-buffer forwarder's default.
    ///
    /// Aliases and localized variables retain the same resolution rules as a
    /// global read. No returned value borrows a symbol, BLV or descriptor slot.
    pub fn default_value_id_copied(&self, id: SymId) -> Option<Value> {
        let mut current = id;
        for _ in 0..50 {
            let sym = self.slot(current)?;
            match sym.value_cell() {
                ValueCell::Plain(value) => return (!value.is_unbound()).then_some(value),
                ValueCell::Alias(target) => current = target,
                ValueCell::Localized(_) => {
                    let value = self.blv(current)?.defcell.cons_cdr();
                    return (!value.is_unbound()).then_some(value);
                }
                ValueCell::Forwarded(fwd) => {
                    return fwd
                        .load()
                        .or_else(|| fwd.as_buffer_obj_fwd().map(|buf_fwd| buf_fwd.default));
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
            // The cell stays with the symbol; pinning it keeps a running
            // definition alive should the symbol itself go.
            let previous = self.symbol_function_id(id).unwrap_or(Value::NIL);
            self.note_function_redefined(id, FunctionEpochBump::Unintern, previous);
        }
        removed_symbol
    }

    /// Function-cell mutation epoch: a `u64` counter bumped on every `fset`. The
    /// JIT's speculative direct-call guards compare against a snapshot of this
    /// value, so it is "monotonic" only modulo 2^64. A wrap could falsely
    /// validate a stale baked call, but at ~1e7 fsets/s that is ~58,000 years
    /// away — physically unreachable; widen to u128 if that ever stops holding.
    /// See the `generation` field.
    #[inline]
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Acquire the function-binding publication clock before reading a stamp
    /// or binding. A refresh must retain this snapshot, so a redefinition
    /// following its proof leaves the refreshed entry below the new clock.
    #[inline(always)]
    pub fn function_epoch(&self) -> u64 {
        self.function_epoch.load(Ordering::Acquire)
    }

    /// Read the publication clock while this obarray is exclusively owned.
    /// The mutable borrow excludes concurrent clock readers as well as writers;
    /// independent mutators may own separate obarrays (clones have fresh clocks).
    /// Shared readers must use `function_epoch` and its Acquire ordering instead.
    /// Return a copy so no clock borrow survives native re-entry. Keeping this
    /// load ordinary lets the warmed JIT guard fold it into its comparison.
    #[cfg(feature = "jit")]
    #[inline]
    pub(crate) fn function_epoch_exclusive(&mut self) -> u64 {
        *self.function_epoch.get_mut()
    }

    /// Test-only: set `function_epoch` (the wrap-skip tests).
    #[cfg(test)]
    pub(crate) fn set_function_epoch_for_test(&mut self, epoch: u64) {
        self.function_epoch.store(epoch, Ordering::Release);
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
            function_epoch: AtomicU64::new(function_epoch),
            members_epoch: 0,
            completion_order_cache: std::sync::Mutex::new(None),
            blvs: Vec::new(),
            value_fwds: Vec::new(),
            debug_on_next_call_fwd: Self::debug_on_next_call_unresolved(),
            // Set by `load_obarray`'s second pass, which re-localizes every
            // dumped Localized symbol through `make_symbol_localized`.
            max_lisp_eval_depth_localized: false,
            generation: next_obarray_generation(),
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
        // The cells were written without stamps: nothing validated before
        // the restored clock may be proven current through them.
        ob.symbols.raise_fn_floor(function_epoch);
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
            // Acquire loads: a concurrent mutator may publish into the
            // same word (`CellWrite`).
            match sym.value_cell_acquire() {
                ValueCell::Plain(v) => {
                    if v != Value::UNBOUND {
                        roots.push(v);
                    }
                }
                // Alias:     a SymId, not a heap ref.
                // Forwarded: 'static forwarder metadata (owned values are
                //            traced from `value_fwds` below).
                // Localized: BLV contents traced via self.blvs below.
                ValueCell::Alias(_) | ValueCell::Forwarded(_) | ValueCell::Localized(_) => {}
            }
            roots.push(load_value_atomic(&sym.function));
            roots.push(load_value_atomic(&sym.plist));
        }
        // BLV contents for LOCALIZED symbols. Unchanged.
        for &blv_ptr in &self.blvs {
            // SAFETY: the pool's records live as long as `self`.
            let blv = unsafe { &*blv_ptr.as_ptr() };
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
#[must_use = "the thread-local extent ends when this guard drops"]
#[derive(Debug)]
pub(crate) struct ObarraySymbolCellSkipGuard {
    _scope: crate::tls_scope::TlsScope<bool, std::cell::Cell<bool>>,
}
static_assertions::assert_not_impl_any!(ObarraySymbolCellSkipGuard: Send, Sync);

impl ObarraySymbolCellSkipGuard {
    pub(crate) fn new() -> Self {
        Self {
            _scope: crate::tls_scope::TlsScope::new(&SEED_SKIP_OBARRAY_SYMBOL_CELLS, true),
        }
    }
}

#[cfg(test)]
#[path = "tests/symbol_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/jit_layout_test.rs"]
mod jit_layout_tests;

#[cfg(test)]
#[path = "tests/fn_stamps_test.rs"]
mod fn_stamps_tests;

/// Ledger 196: the buffer-local-read class ledger 191 named, pinned per site.
#[cfg(test)]
#[path = "tests/buffer_local_global_read_test.rs"]
mod buffer_local_global_read_tests;

#[cfg(test)]
#[path = "tests/copied_value_test.rs"]
mod copied_value_tests;
