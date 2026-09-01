//! Tagged pointer `Value` — a single `usize` encoding type + payload.
//!
//! # Tag layout (3 low bits, 8-byte aligned heap pointers)
//!
//! ```text
//! Tag   Type         Payload                         Fast check
//! 000   Symbol       sym_index << 3                  (v & 7) == 0
//! 001   Unused       reserved by GNU Emacs
//! xx10  Fixnum       integer << 2 | 2                 (v & 3) == 2
//! 011   Cons         pointer | 3                     (v & 7) == 3
//! 100   String       pointer | 4                     (v & 7) == 4
//! 101   Vectorlike   pointer | 5                     (v & 7) == 5
//! 111   Float        pointer | 7                     (v & 7) == 7
//! ```
//!
//! Fixnum uses tags 010 and 110 (both have `(v & 3) == 2`), giving
//! 62-bit signed integer range without heap allocation.
//!
//! Special values:
//! - `nil`  = Symbol(0) = `0x0` (intern "nil" as SymId(0))
//! - `t`    = Symbol(1) = `0x8` (intern "t" as SymId(1))
//! - `Qunbound` = noncanonical Symbol(2), matching GNU's symbol sentinel.

use malachite::integer::Integer;
use std::cell::RefCell;
use std::fmt;
use std::hash::{Hash, Hasher};

use crate::emacs_core::intern::{
    SymId, UNBOUND_SYM_ID, canonical_symbol_for_name, resolve_sym_lisp_string, symbol_name_id,
};
use crate::heap_types::LispString;

use super::header::{
    BignumObj, ConsCell, FloatObj, ModuleFunctionObj, SqliteObj, StringObj, SubrObj,
    SymbolWithPosObj, UserPtrObj, VecLikeHeader, VecLikeType,
};

/// Clear the old subr registry on the tagged heap — no-op now that subrs use
/// the static global table, but kept to avoid breaking pdump callers until
/// the tagged heap's subr fields are fully removed.
pub(crate) fn reset_current_subrs() {
    // No-op: the old per-heap subr registry is no longer used.
}

// ---------------------------------------------------------------------------
// Tag constants
// ---------------------------------------------------------------------------

pub(crate) const TAG_BITS: usize = 3;
pub(crate) const TAG_MASK: usize = 0b111;

// pub(crate) so the JIT backend can lower type predicates against the same tag
// layout instead of hardcoding it.
pub(crate) const TAG_SYMBOL: usize = 0b000;
pub(crate) const TAG_CONS: usize = 0b011;
pub(crate) const TAG_STRING: usize = 0b100;
pub(crate) const TAG_VECLIKE: usize = 0b101;
pub(crate) const TAG_FLOAT: usize = 0b111;

// Fixnum uses two tags: 010 and 110. Both have (v & 3) == 2.
// pub(crate) so the JIT backend (emacs_core::jit) lowers fixnum ops against the
// same single source of truth instead of hardcoding the layout.
pub(crate) const FIXNUM_CHECK_MASK: usize = 0b11;
pub(crate) const FIXNUM_CHECK_VALUE: usize = 0b10;
pub(crate) const FIXNUM_SHIFT: u32 = 2; // integer stored in bits 2..63

thread_local! {
    static STATIC_SUBR_OBJECTS: RefCell<Vec<Option<TaggedValue>>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn update_static_subr_object_entry(
    sym_id: SymId,
    function: Option<super::header::SubrFn>,
    min_args: u16,
    max_args: Option<u16>,
    dispatch_kind: super::header::SubrDispatchKind,
    interactivity: super::header::SubrInteractivity,
) {
    STATIC_SUBR_OBJECTS.with(|objects| {
        let Some(value) = objects
            .borrow()
            .get(sym_id.0 as usize)
            .and_then(|value| *value)
        else {
            return;
        };
        if value.veclike_type() != Some(VecLikeType::Subr) {
            return;
        }

        let ptr = value.as_veclike_ptr().unwrap() as *mut SubrObj;
        // Static subr objects are leaked and never moved. Native subr
        // registration is the single writer for their entry metadata, matching GNU's static
        // `struct Lisp_Subr` initialization model.
        unsafe {
            (*ptr).function = function;
            (*ptr).min_args = min_args;
            (*ptr).max_args = max_args;
            (*ptr).dispatch_kind = dispatch_kind;
            (*ptr).interactivity = interactivity;
        }
    });
}

// ---------------------------------------------------------------------------
// TaggedValue — the core type
// ---------------------------------------------------------------------------

/// A Lisp value encoded as a tagged pointer in a single machine word.
///
/// This is `Copy` and `Eq` — can be freely duplicated and compared.
/// Heap access is via direct pointer dereference (no ObjId indirection).
#[derive(Clone, Copy, PartialOrd, Ord)]
#[repr(transparent)]
pub struct TaggedValue(pub(crate) usize);

/// `PartialEq` uses structural comparison (`equal`), matching the behavior
/// of the old `Value` enum. This allows `assert_eq!` in tests to work
/// naturally.  For Emacs `eq` (pointer identity), use `eq_value()` or
/// `a.bits() == b.bits()`.
///
/// NOTE: This is `equal`-style structural equality. Code that needs Lisp
/// hash-table semantics for a specific test (`eq`, `eql`, or `equal`) should
/// convert through `HashKey` with the selected `HashTableTest`.
impl PartialEq for TaggedValue {
    fn eq(&self, other: &Self) -> bool {
        if self.0 == other.0 {
            return true;
        }
        crate::emacs_core::value::equal_value(self, other, 0)
    }
}

impl Eq for TaggedValue {}

impl Hash for TaggedValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_hash_key(&crate::emacs_core::value::HashTableTest::Equal)
            .hash(state);
    }
}

fn canonical_subr_object(sym_id: SymId) -> TaggedValue {
    STATIC_SUBR_OBJECTS.with(|objects| {
        let idx = sym_id.0 as usize;
        if let Some(value) = objects.borrow().get(idx).and_then(|value| *value) {
            return value;
        }

        let value = allocate_static_subr_object(sym_id);
        let mut objects = objects.borrow_mut();
        if objects.len() <= idx {
            objects.resize_with(idx + 1, || None);
        }
        if let Some(existing) = objects[idx] {
            existing
        } else {
            objects[idx] = Some(value);
            value
        }
    })
}

fn allocate_static_subr_object(sym_id: SymId) -> TaggedValue {
    let name_id = symbol_name_id(sym_id);
    let entry = crate::emacs_core::eval::lookup_global_subr_entry(sym_id);
    let (function, min_args, max_args, dispatch_kind, interactivity) = if let Some(entry) = entry {
        (
            entry.function,
            entry.min_args,
            entry.max_args,
            entry.dispatch_kind,
            super::header::SubrInteractivity::from(entry.interactive_spec.is_some()),
        )
    } else {
        (
            None,
            0,
            None,
            super::header::SubrDispatchKind::Builtin,
            super::header::SubrInteractivity::NonInteractive,
        )
    };

    let obj = Box::new(SubrObj {
        header: super::header::VecLikeHeader::new(VecLikeType::Subr),
        sym_id,
        name: name_id,
        min_args,
        max_args,
        dispatch_kind,
        interactivity,
        function,
    });
    let ptr = Box::leak(obj) as *mut SubrObj;
    unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

impl TaggedValue {
    // -- Special values --

    /// The nil value. `nil = Symbol(0) = 0`.
    pub const NIL: Self = Self(0);

    /// The t (true) value. `t = Symbol(1) = 0x8`.
    pub const T: Self = Self(1 << TAG_BITS);

    /// The `Qunbound` sentinel. GNU represents this as a real symbol
    /// object, not as an immediate tag. Neomacs mirrors that shape with
    /// a noncanonical process-global symbol named `unbound`.
    ///
    /// This must never leak into ordinary Lisp code — callers that
    /// observe it should either signal `void-variable` or treat it
    /// as "absent" depending on context.
    pub const UNBOUND: Self = Self((UNBOUND_SYM_ID.0 as usize) << TAG_BITS | TAG_SYMBOL);

    /// GNU's `dead_object ()` (`src/lisp.h:1353-1357`): "Return a Lisp_Object
    /// value that does not correspond to any object. This can make some Lisp
    /// objects on free lists recognizable in O(1)." GNU spells it
    /// `make_lisp_ptr (NULL, Lisp_String)`; this is the same bit pattern here,
    /// a STRING tag over a null pointer, which no live object can ever hold.
    ///
    /// It exists because the free list is threaded through the dead cells
    /// themselves (`ConsCell::set_free_next`, matching GNU `sweep_conses`), so
    /// a reclaimed cell's slots still decode as ordinary Lisp values. `nil` in
    /// the car is indistinguishable from a real `nil`; `dead_object` is
    /// distinguishable from every live value, which is the whole point.
    pub const DEAD: Self = Self(TAG_STRING);

    /// GNU `deadp` (`src/alloc.c:425-429`) — is this the free-list poison?
    #[inline]
    pub fn is_dead(self) -> bool {
        self.0 == Self::DEAD.0
    }

    // -- Fixnum --

    /// Create a fixnum (62-bit signed integer, no heap allocation).
    #[inline]
    pub fn fixnum(n: i64) -> Self {
        // Encode: (n << 2) | 2. The low 2 bits are `10`, matching GNU's
        // fixnum tags 010 and 110.
        Self(((n as usize) << FIXNUM_SHIFT) | FIXNUM_CHECK_VALUE)
    }

    /// Maximum fixnum value (62-bit signed).
    pub const MOST_POSITIVE_FIXNUM: i64 = (1_i64 << (64 - FIXNUM_SHIFT - 1)) - 1;
    /// Minimum fixnum value (62-bit signed).
    pub const MOST_NEGATIVE_FIXNUM: i64 = -(1_i64 << (64 - FIXNUM_SHIFT - 1));

    // -- Symbol --

    /// Create a symbol value from a SymId.
    #[inline]
    pub fn from_sym_id(id: SymId) -> Self {
        Self((id.0 as usize) << TAG_BITS | TAG_SYMBOL)
    }

    // -- Cons --

    /// Create a cons value from a pointer to a ConsCell.
    ///
    /// # Safety
    /// `cell` must be a valid, 8-byte-aligned pointer to a live `ConsCell`.
    #[inline]
    pub unsafe fn from_cons_ptr(cell: *const ConsCell) -> Self {
        debug_assert!(!cell.is_null());
        debug_assert!(cell as usize & TAG_MASK == 0, "ConsCell not aligned");
        Self(cell as usize | TAG_CONS)
    }

    // -- String --

    /// Create a string value from a pointer to a StringObj.
    ///
    /// # Safety
    /// `obj` must be a valid, 8-byte-aligned pointer to a live `StringObj`.
    #[inline]
    pub unsafe fn from_string_ptr(obj: *const StringObj) -> Self {
        debug_assert!(!obj.is_null());
        debug_assert!(obj as usize & TAG_MASK == 0, "StringObj not aligned");
        Self(obj as usize | TAG_STRING)
    }

    // -- Float --

    /// Create a float value from a pointer to a FloatObj.
    ///
    /// # Safety
    /// `obj` must be a valid, 8-byte-aligned pointer to a live `FloatObj`.
    #[inline]
    pub unsafe fn from_float_ptr(obj: *const FloatObj) -> Self {
        debug_assert!(!obj.is_null());
        debug_assert!(obj as usize & TAG_MASK == 0, "FloatObj not aligned");
        Self(obj as usize | TAG_FLOAT)
    }

    // -- Vectorlike --

    /// Create a vectorlike value from a pointer to a VecLikeHeader.
    ///
    /// # Safety
    /// `obj` must be a valid, 8-byte-aligned pointer to a live veclike object.
    #[inline]
    pub unsafe fn from_veclike_ptr(obj: *const VecLikeHeader) -> Self {
        debug_assert!(!obj.is_null());
        debug_assert!(obj as usize & TAG_MASK == 0, "VecLikeHeader not aligned");
        Self(obj as usize | TAG_VECLIKE)
    }

    // -- GNU Lisp object constructors --

    /// Create a char value. In GNU Emacs, characters ARE integers (fixnums).
    /// `?A` is just the integer 65.
    #[inline]
    pub fn char(c: char) -> Self {
        Self::fixnum(c as i64)
    }

    /// Create a keyword value from a SymId.
    /// In GNU Emacs, keywords are ordinary symbols with `:` prefix names.
    #[inline]
    pub fn from_kw_id(id: SymId) -> Self {
        Self::from_sym_id(id)
    }

    /// Create a subr value from a SymId.
    ///
    /// GNU Emacs represents subrs as `PVEC_SUBR` vectorlike objects, not as
    /// immediate values. Neomacs keeps the Rust entry point in the global subr
    /// table, while this value is the Lisp-visible `#<subr NAME>` object.
    #[inline]
    pub fn subr_from_sym_id(sym_id: crate::emacs_core::intern::SymId) -> Self {
        let canonical = canonical_symbol_for_name(symbol_name_id(sym_id)).unwrap_or(sym_id);
        canonical_subr_object(canonical)
    }

    /// Create a subr (builtin function) value.
    pub fn subr(id: SymId) -> Self {
        Self::subr_from_sym_id(id)
    }

    // ---------------------------------------------------------------------------
    // Tag checks — all compile to a single AND + CMP
    // ---------------------------------------------------------------------------

    /// Raw tag (low 3 bits).
    #[inline(always)]
    pub fn tag(self) -> usize {
        self.0 & TAG_MASK
    }

    /// Raw bits (for hashing, pointer identity, etc.).
    #[inline(always)]
    pub fn bits(self) -> usize {
        self.0
    }

    /// Reconstruct a value from its raw tagged bits (the inverse of [`bits`]).
    ///
    /// `pub(crate)` and unchecked: the caller must supply bits that originated
    /// from a real `Value` (e.g. the baseline JIT, which only ever returns a
    /// constant-pool entry, nil/t, or a freshly tagged fixnum).
    ///
    /// [`bits`]: Self::bits
    #[inline(always)]
    pub(crate) const fn from_bits(bits: usize) -> Self {
        Self(bits)
    }

    #[inline(always)]
    pub fn is_nil(self) -> bool {
        self.0 == 0
    }

    /// Check for `t` (the canonical true value).
    #[inline(always)]
    pub fn is_t(self) -> bool {
        self.0 == Self::T.0
    }

    #[inline(always)]
    pub fn is_fixnum(self) -> bool {
        self.0 & FIXNUM_CHECK_MASK == FIXNUM_CHECK_VALUE
    }

    /// Check if this value is a symbol.
    /// In GNU Emacs, keywords are symbols (interned with `:` prefix).
    /// Check if this value is a symbol. Keywords are symbols (name starts
    /// with `:`). nil and t are also symbols.
    #[inline(always)]
    pub fn is_symbol(self) -> bool {
        self.0 & TAG_MASK == TAG_SYMBOL
    }

    #[inline(always)]
    pub fn is_cons(self) -> bool {
        self.0 & TAG_MASK == TAG_CONS
    }

    #[inline(always)]
    pub fn is_string(self) -> bool {
        self.0 & TAG_MASK == TAG_STRING
    }

    #[inline(always)]
    pub fn is_float(self) -> bool {
        self.0 & TAG_MASK == TAG_FLOAT
    }

    #[inline(always)]
    pub fn is_veclike(self) -> bool {
        self.0 & TAG_MASK == TAG_VECLIKE
    }

    /// True if this is the `Qunbound` sentinel.
    ///
    /// `Qunbound` marks a "no value" state for symbol value cells
    /// and buffer-local alist entries. Seeing it in an ordinary
    /// read path means the caller should signal `void-variable`
    /// or treat the binding as absent. Mirrors GNU's `BASE_EQ (x,
    /// Qunbound)` checks throughout `data.c`.
    #[inline]
    pub fn is_unbound(self) -> bool {
        self.0 == Self::UNBOUND.0
    }

    /// In GNU Emacs, characters are integers. `characterp` checks if the
    /// integer is in the valid Unicode codepoint range (0..=0x3FFFFF in GNU,
    /// 0..=0x10FFFF for valid Unicode).
    #[inline]
    pub fn is_char(self) -> bool {
        if let Some(n) = self.as_fixnum() {
            (0..=0x3F_FFFF).contains(&n) // GNU MAX_CHAR
        } else {
            false
        }
    }

    /// In GNU Emacs, keywords are symbols whose name starts with `:`.
    #[inline]
    pub fn is_keyword(self) -> bool {
        self.as_symbol_id()
            .is_some_and(crate::emacs_core::intern::is_keyword_id)
    }

    /// Subrs are PVEC_SUBR-like vectorlike objects, matching GNU Emacs.
    #[inline(always)]
    pub fn is_subr(self) -> bool {
        self.veclike_type() == Some(super::header::VecLikeType::Subr)
    }

    /// Bignums are PVEC_BIGNUM veclike heap objects (mirrors GNU `BIGNUMP`).
    #[inline]
    pub fn is_bignum(self) -> bool {
        self.veclike_type() == Some(super::header::VecLikeType::Bignum)
    }

    /// Get a borrowed reference to the underlying `malachite::Integer`.
    /// Returns `None` if this value isn't a bignum.
    ///
    /// # Safety / lifetime
    /// The returned reference is `'static` because callers can't easily
    /// thread a heap lifetime through `Value`. The pointer is only
    /// valid for as long as the underlying heap object is alive — the
    /// caller must avoid GC across the borrow. This matches the
    /// existing `as_str` / `xfloat` pattern.
    #[inline]
    pub fn as_bignum(self) -> Option<&'static Integer> {
        if self.is_bignum() {
            let ptr = (self.0 & !TAG_MASK) as *const BignumObj;
            // Safety: tag check ensures this is a BignumObj allocated
            // through `alloc_bignum`, which puts a `VecLikeHeader` at
            // offset 0 followed by `value: Integer`.
            Some(unsafe { &(*ptr).value })
        } else {
            None
        }
    }

    /// If this is a symbol-with-pos, return a reference to the object.
    pub fn as_symbol_with_pos(&self) -> Option<&SymbolWithPosObj> {
        if self.is_symbol_with_pos() {
            Some(unsafe { &*(self.as_veclike_ptr()? as *const SymbolWithPosObj) })
        } else {
            None
        }
    }

    /// If this is an SQLite object, return a reference to the object.
    pub fn as_sqlite(&self) -> Option<&SqliteObj> {
        if self.veclike_type() == Some(VecLikeType::Sqlite) {
            Some(unsafe { &*(self.as_veclike_ptr()? as *const SqliteObj) })
        } else {
            None
        }
    }

    /// If this is a user-pointer object, return a reference to the object.
    pub fn as_user_ptr(&self) -> Option<&UserPtrObj> {
        if self.veclike_type() == Some(VecLikeType::UserPtr) {
            Some(unsafe { &*(self.as_veclike_ptr()? as *const UserPtrObj) })
        } else {
            None
        }
    }

    /// If this is a module-function object, return a reference to the object.
    pub fn as_module_function(&self) -> Option<&ModuleFunctionObj> {
        if self.veclike_type() == Some(VecLikeType::ModuleFunction) {
            Some(unsafe { &*(self.as_veclike_ptr()? as *const ModuleFunctionObj) })
        } else {
            None
        }
    }

    /// If this is a symbol-with-pos, return the bare symbol Value.
    pub fn as_symbol_with_pos_sym(&self) -> Option<TaggedValue> {
        self.as_symbol_with_pos().map(|swp| swp.sym)
    }

    /// If this is a symbol-with-pos, return the position as i64.
    pub fn as_symbol_with_pos_pos(&self) -> Option<i64> {
        self.as_symbol_with_pos()
            .and_then(|swp| swp.pos.as_fixnum())
    }

    /// True if this value holds a heap pointer (needs GC tracing).
    #[inline]
    pub fn is_heap_object(self) -> bool {
        matches!(self.tag(), TAG_CONS | TAG_STRING | TAG_FLOAT | TAG_VECLIKE)
    }

    /// Check if this value is a list (nil or cons).
    #[inline]
    pub fn is_list(self) -> bool {
        self.is_nil() || self.is_cons()
    }

    // ---------------------------------------------------------------------------
    // Extractors
    // ---------------------------------------------------------------------------

    /// Extract fixnum value. Returns None if not a fixnum.
    #[inline]
    pub fn as_fixnum(self) -> Option<i64> {
        if self.is_fixnum() {
            Some((self.0 as i64) >> FIXNUM_SHIFT)
        } else {
            None
        }
    }

    /// Extract fixnum value without tag check. Caller must ensure `is_fixnum()`.
    #[inline]
    pub fn xfixnum(self) -> i64 {
        debug_assert!(self.is_fixnum());
        (self.0 as i64) >> FIXNUM_SHIFT
    }

    /// Extract SymId for a symbol (including keywords, which are symbols
    /// Extract SymId for a symbol (including keywords). Returns None if not a symbol.
    #[inline(always)]
    pub fn as_symbol_id(self) -> Option<SymId> {
        if self.0 & TAG_MASK == TAG_SYMBOL {
            Some(SymId((self.0 >> TAG_BITS) as u32))
        } else {
            None
        }
    }

    /// Extract SymId without tag check. Caller must ensure `is_symbol()`.
    #[inline(always)]
    pub fn xsymbol_id(self) -> SymId {
        debug_assert!(self.is_symbol());
        SymId((self.0 >> TAG_BITS) as u32)
    }

    /// Extract char. Characters are fixnums in the valid codepoint range.
    /// Returns None if not a character (not fixnum or out of range).
    #[inline]
    pub fn as_char(self) -> Option<char> {
        if let Some(n) = self.as_fixnum() {
            if (0..=0x3F_FFFF).contains(&n) {
                // GNU Emacs allows codepoints up to MAX_CHAR (0x3FFFFF)
                // which includes non-Unicode internal chars. For Rust char,
                // we can only convert valid Unicode codepoints.
                char::from_u32(n as u32)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Extract keyword SymId. Returns None if not a keyword.
    /// Keywords are symbols with `:` prefix, so this extracts the symbol id.
    #[inline]
    pub fn as_keyword_id(self) -> Option<SymId> {
        if self.is_keyword() {
            self.as_symbol_id()
        } else {
            None
        }
    }

    /// Extract the canonical public symbol id for a subr.
    #[inline(always)]
    pub fn as_subr_id(self) -> Option<SymId> {
        if self.veclike_type() == Some(super::header::VecLikeType::Subr) {
            let ptr = self.as_veclike_ptr().unwrap() as *const super::header::SubrObj;
            Some(unsafe { (*ptr).sym_id })
        } else {
            None
        }
    }

    /// Read GNU's intrinsic primitive-command state from the subr object.
    #[inline(always)]
    pub fn subr_interactivity(self) -> Option<super::header::SubrInteractivity> {
        if self.veclike_type() != Some(super::header::VecLikeType::Subr) {
            return None;
        }
        let ptr = self.as_veclike_ptr().unwrap() as *const super::header::SubrObj;
        Some(unsafe { (*ptr).interactivity })
    }

    // -- Heap pointer extractors --

    /// Extract raw cons cell pointer. Returns None if not a cons.
    #[inline(always)]
    pub fn as_cons_ptr(self) -> Option<*const ConsCell> {
        if self.is_cons() {
            Some((self.0 & !TAG_MASK) as *const ConsCell)
        } else {
            None
        }
    }

    /// Extract raw cons cell pointer without tag check.
    #[inline(always)]
    pub fn xcons_ptr(self) -> *const ConsCell {
        debug_assert!(self.is_cons());
        (self.0 & !TAG_MASK) as *const ConsCell
    }

    /// Extract raw string object pointer. Returns None if not a string.
    #[inline(always)]
    pub fn as_string_ptr(self) -> Option<*const StringObj> {
        if self.is_string() {
            Some((self.0 & !TAG_MASK) as *const StringObj)
        } else {
            None
        }
    }

    /// Extract raw float object pointer. Returns None if not a float.
    #[inline(always)]
    pub fn as_float_ptr(self) -> Option<*const FloatObj> {
        if self.is_float() {
            Some((self.0 & !TAG_MASK) as *const FloatObj)
        } else {
            None
        }
    }

    /// Extract raw veclike header pointer. Returns None if not veclike.
    #[inline(always)]
    pub fn as_veclike_ptr(self) -> Option<*const VecLikeHeader> {
        if self.is_veclike() {
            Some((self.0 & !TAG_MASK) as *const VecLikeHeader)
        } else {
            None
        }
    }

    /// Extract raw heap pointer (any heap type). Returns None if immediate.
    #[inline]
    pub fn heap_ptr(self) -> Option<*const u8> {
        if self.is_heap_object() {
            Some((self.0 & !TAG_MASK) as *const u8)
        } else {
            None
        }
    }

    // ---------------------------------------------------------------------------
    // Cons accessors (direct pointer deref, no heap indirection)
    // ---------------------------------------------------------------------------

    /// Get the car of a cons cell. Panics if not a cons.
    #[inline(always)]
    pub fn cons_car(self) -> Self {
        unsafe { (*self.xcons_ptr()).car }
    }

    /// Get the cdr of a cons cell. Panics if not a cons.
    #[inline(always)]
    pub fn cons_cdr(self) -> Self {
        unsafe { (*self.xcons_ptr()).cdr() }
    }

    /// Set the car of a cons cell. Panics if not a cons.
    #[inline]
    pub fn set_car(self, val: Self) {
        assert!(crate::tagged::mutate::set_cons_car(self, val));
    }

    /// Set the cdr of a cons cell. Panics if not a cons.
    #[inline]
    pub fn set_cdr(self, val: Self) {
        assert!(crate::tagged::mutate::set_cons_cdr(self, val));
    }

    // ---------------------------------------------------------------------------
    // Float accessor
    // ---------------------------------------------------------------------------

    /// Get the f64 value of a float. Panics if not a float.
    #[inline(always)]
    pub fn xfloat(self) -> f64 {
        debug_assert!(self.is_float());
        unsafe { (*(self.as_float_ptr().unwrap())).value }
    }

    // ---------------------------------------------------------------------------
    // Veclike accessors
    // ---------------------------------------------------------------------------

    /// Get the veclike sub-type. Returns None if not veclike.
    #[inline(always)]
    pub fn veclike_type(self) -> Option<VecLikeType> {
        if self.is_veclike() {
            Some(unsafe { (*self.as_veclike_ptr().unwrap()).type_tag })
        } else {
            None
        }
    }

    // ---------------------------------------------------------------------------
    // Type dispatch enum (for exhaustive matching)
    // ---------------------------------------------------------------------------

    /// Decode into a `ValueKind` enum for exhaustive pattern matching.
    /// This provides Rust `match` ergonomics without the old `Value` enum.
    #[inline(always)]
    pub fn kind(self) -> ValueKind {
        match self.tag() {
            TAG_SYMBOL => {
                if self.is_nil() {
                    ValueKind::Nil
                } else if self.is_t() {
                    ValueKind::T
                } else if self.is_unbound() {
                    ValueKind::Unbound
                } else {
                    ValueKind::Symbol(self.xsymbol_id())
                }
            }
            _ if self.is_fixnum() => ValueKind::Fixnum(self.xfixnum()),
            TAG_CONS => ValueKind::Cons,
            TAG_VECLIKE => {
                ValueKind::Veclike(unsafe { (*self.as_veclike_ptr().unwrap()).type_tag })
            }
            TAG_STRING => ValueKind::String,
            TAG_FLOAT => ValueKind::Float,
            _ => ValueKind::Unknown,
        }
    }
}

// ---------------------------------------------------------------------------
// Backward-compatible API (matches old Value enum methods)
// ---------------------------------------------------------------------------

impl TaggedValue {
    // -- Compat constructors that allocate on the thread-local heap --

    /// Create a symbol by interning a name string.
    pub fn symbol_by_name(s: impl AsRef<str>) -> Self {
        Self::from_sym_id(crate::emacs_core::intern::intern(s.as_ref()))
    }

    /// Create a keyword by interning a name string.
    pub fn keyword_by_name(s: impl AsRef<str>) -> Self {
        Self::from_kw_id(crate::emacs_core::intern::intern(s.as_ref()))
    }

    /// `Value::t()` — compat alias for `Value::T`.
    pub fn t() -> Self {
        Self::T
    }

    /// `Value::bool(b)` — convert bool to nil/t.
    pub fn bool_val(b: bool) -> Self {
        if b { Self::T } else { Self::NIL }
    }

    // -- Compat predicates --

    /// True if this value is "truthy" (not nil).
    #[inline]
    pub fn is_truthy(self) -> bool {
        !self.is_nil()
    }

    /// True for integers — both fixnums and bignums (matches GNU `INTEGERP`).
    /// Characters are also integers in GNU Emacs, and since chars are encoded
    /// as fixnums, they fall through the fixnum branch.
    #[inline]
    pub fn is_integer(self) -> bool {
        self.is_fixnum() || self.is_bignum()
    }

    /// True for any number (fixnum, bignum, or float). Mirrors GNU `NUMBERP`.
    #[inline]
    pub fn is_number(self) -> bool {
        self.is_fixnum() || self.is_bignum() || self.is_float()
    }

    /// True if this value is a vector (veclike with Vector type tag).
    #[inline]
    pub fn is_vector(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Vector)
    }

    /// True if this value is a char-table (GNU PVEC_CHAR_TABLE shape).
    #[inline]
    pub fn is_char_table(self) -> bool {
        self.veclike_type() == Some(VecLikeType::CharTable)
    }

    /// True if this value is a sub-char-table (GNU PVEC_SUB_CHAR_TABLE shape).
    #[inline]
    pub fn is_sub_char_table(self) -> bool {
        self.veclike_type() == Some(VecLikeType::SubCharTable)
    }

    /// True if this value is a record (veclike with Record type tag).
    #[inline]
    pub fn is_record(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Record)
    }

    /// True if this value is an opened font pseudovector.
    #[inline]
    pub fn is_font_object(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Font)
    }

    /// True if this value is a window configuration (veclike with the
    /// WindowConfiguration type tag). Opaque to vector/array/sequence predicates.
    #[inline]
    pub fn is_window_configuration(self) -> bool {
        self.veclike_type() == Some(VecLikeType::WindowConfiguration)
    }

    /// True if this value is a hash table.
    #[inline]
    pub fn is_hash_table(self) -> bool {
        self.veclike_type() == Some(VecLikeType::HashTable)
    }

    /// True if this value is a GNU-shaped obarray object.
    #[inline]
    pub fn is_obarray(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Obarray)
    }

    /// True if this value is a symbol-with-pos pseudo-vector.
    #[inline]
    pub fn is_symbol_with_pos(self) -> bool {
        self.veclike_type() == Some(VecLikeType::SymbolWithPos)
    }

    /// True if this is an SQLite database or statement object.
    #[inline]
    pub fn is_sqlite(self) -> bool {
        self.veclike_type() == Some(VecLikeType::Sqlite)
    }

    /// True if this is a user-pointer object (PVEC_USER_PTR).
    #[inline]
    pub fn is_user_ptr(self) -> bool {
        self.veclike_type() == Some(VecLikeType::UserPtr)
    }

    /// True if this is a module-function object (PVEC_MODULE_FUNCTION).
    #[inline]
    pub fn is_module_function(self) -> bool {
        self.veclike_type() == Some(VecLikeType::ModuleFunction)
    }

    /// True if this value is callable (lambda, macro, bytecode, subr, module-function).
    #[inline]
    pub fn is_function(self) -> bool {
        self.is_subr()
            || matches!(
                self.veclike_type(),
                Some(VecLikeType::Lambda | VecLikeType::ByteCode | VecLikeType::ModuleFunction)
            )
    }

    /// Human-readable type name.
    pub fn type_name(self) -> &'static str {
        match self.kind() {
            ValueKind::Nil => "nil",
            ValueKind::T => "symbol",
            ValueKind::Fixnum(_) => "integer",
            ValueKind::Symbol(_) => "symbol",
            ValueKind::Cons => "cons",
            ValueKind::String => "string",
            ValueKind::Float => "float",
            ValueKind::Subr(_) => "subr",
            ValueKind::Veclike(ty) => match ty {
                VecLikeType::Subr => "subr",
                VecLikeType::Xwidget => "xwidget",
                VecLikeType::XwidgetView => "xwidget-view",
                VecLikeType::Vector => "vector",
                VecLikeType::HashTable => "hash-table",
                VecLikeType::Lambda => "closure",
                VecLikeType::Macro => "macro",
                VecLikeType::ByteCode => "byte-code",
                VecLikeType::Record => "record",
                VecLikeType::Font => "font-object",
                VecLikeType::WindowConfiguration => "window-configuration",
                VecLikeType::Overlay => "overlay",
                VecLikeType::Marker => "marker",
                VecLikeType::Buffer => "buffer",
                VecLikeType::Window => "window",
                VecLikeType::Frame => "frame",
                VecLikeType::Timer => "timer",
                VecLikeType::Process => "process",
                VecLikeType::Terminal => "terminal",
                // GNU Emacs reports both fixnums and bignums as
                // "integer" via `Ftype_of` / `Fcl_type_of`.
                VecLikeType::Bignum => "integer",
                VecLikeType::SymbolWithPos => "symbol-with-pos",
                VecLikeType::Finalizer => "finalizer",
                VecLikeType::Sqlite => "sqlite",
                VecLikeType::UserPtr => "user-ptr",
                VecLikeType::ModuleFunction => "module-function",
                VecLikeType::CharTable => "char-table",
                VecLikeType::SubCharTable => "sub-char-table",
                VecLikeType::Obarray => "obarray",
                VecLikeType::SurfaceHandle => "neomacs-surface",
            },
            ValueKind::Unbound => "unbound",
            ValueKind::Unknown => "unknown",
        }
    }

    // -- Numeric extraction --

    /// Extract integer value (alias for as_fixnum).
    #[inline]
    pub fn as_int(self) -> Option<i64> {
        self.as_fixnum()
    }

    /// Extract float value. Returns None if not a float.
    #[inline]
    pub fn as_float(self) -> Option<f64> {
        if self.is_float() {
            Some(self.xfloat())
        } else {
            None
        }
    }

    /// Extract numeric value as f64 (works for both fixnum and float).
    #[inline]
    pub fn as_number_f64(self) -> Option<f64> {
        if let Some(n) = self.as_fixnum() {
            Some(n as f64)
        } else {
            self.as_float()
        }
    }

    // -- String extraction --

    /// Get the string content as UTF-8 `&str`. Returns `None` if not a string
    /// **or** if the bytes are not valid UTF-8 (e.g. raw-byte Emacs encoding).
    ///
    /// Prefer `as_bytes()` / `equal_value` for byte-level equality — two
    /// different non-UTF-8 strings both return `None`, so comparing
    /// `as_utf8_str()` values with `==` will silently treat them as equal.
    pub fn as_utf8_str(self) -> Option<&'static str> {
        if self.is_string() {
            let ptr = self.as_string_ptr().unwrap();
            // Safety: the string object is alive (caller must ensure no GC).
            // Lifetime is extended to 'static — same pattern as old Value::as_str.
            unsafe {
                let header = &(*ptr).header;
                if !matches!(header.kind, super::header::HeapObjectKind::String) {
                    panic!(
                        "BUG: StringObj header.kind = {:?} (expected String) — \
                         possible use-after-free. Tagged value = {:#x}, ptr = {:?}",
                        header.kind, self.0, ptr,
                    );
                }
                // The kind check above cannot see a slot reclaimed and handed
                // back to the SAME class arena, which is the common case for
                // strings. GNU's free marker can (DIVERGENCES.md 163):
                // `sweep_strings` nulls `s->u.s.data` "so that we know it's
                // free" (src/alloc.c:1878-1882).
                if (*ptr).data.is_reclaimed() {
                    panic!(
                        "use-after-free: borrowed a string object the collector has \
                         reclaimed (StringObj at {ptr:?} carries GNU sweep_strings' \
                         null-data free marker). Tagged value = {:#x}. See \
                         `Value::as_lisp_string` and DIVERGENCES.md 163.",
                        self.0,
                    );
                }
                (*ptr).data.as_utf8_str()
            }
        } else {
            None
        }
    }

    /// Get symbol name. Returns None if not a symbol.
    /// For keywords (which are symbols in GNU Emacs), returns the keyword name
    /// (e.g., ":foo").
    pub fn as_symbol_name(self) -> Option<&'static str> {
        self.as_symbol_lisp_string()
            .and_then(LispString::as_utf8_str)
    }

    /// Get the exact Lisp-string storage for a symbol name.
    pub fn as_symbol_lisp_string(self) -> Option<&'static LispString> {
        self.as_symbol_id().map(resolve_sym_lisp_string)
    }

    /// Check if this symbol has the given name.
    ///
    /// Byte equality, not `as_symbol_name` — `name` is valid UTF-8, so
    /// equal bytes imply the symbol's name is too, and the old
    /// validate-then-compare paid a `from_utf8` walk per call (face
    /// attribute merging asks `is_symbol_named("unspecified")` per
    /// attribute per face).
    pub fn is_symbol_named(self, name: &str) -> bool {
        self.as_symbol_lisp_string()
            .is_some_and(|s| s.as_bytes() == name.as_bytes())
    }
}

// ---------------------------------------------------------------------------
// ValueKind — exhaustive dispatch enum
// ---------------------------------------------------------------------------

/// Decoded value kind for `match` ergonomics.
/// Use `value.kind()` to get this.
#[derive(Debug, Clone, Copy)]
pub enum ValueKind {
    Nil,
    T,
    /// Integer (fixnum). In GNU Emacs, characters are also integers.
    Fixnum(i64),
    /// Symbol (including keywords — they're symbols with `:` prefix names).
    Symbol(SymId),
    Cons,
    String,
    Float,
    // NOTE: No Char variant. Characters are Fixnum in GNU Emacs.
    // NOTE: No Keyword variant. Keywords are Symbol in GNU Emacs.
    /// Legacy decoded subr variant. GNU-shaped runtime subrs are
    /// `Veclike(VecLikeType::Subr)`.
    Subr(SymId),
    Veclike(VecLikeType),
    /// The `Qunbound` sentinel. Never reached by ordinary Lisp
    /// reads — a caller that dispatches on this should signal
    /// `void-variable` or treat the binding as absent.
    Unbound,
    Unknown,
}

// ---------------------------------------------------------------------------
// Debug / Display
// ---------------------------------------------------------------------------

impl fmt::Debug for TaggedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            ValueKind::Nil => write!(f, "nil"),
            ValueKind::T => write!(f, "t"),
            ValueKind::Fixnum(n) => write!(f, "{}", n),
            ValueKind::Symbol(id) => write!(f, "Symbol({:?})", id),
            ValueKind::Cons => write!(f, "Cons@{:#x}", self.0 & !TAG_MASK),
            ValueKind::String => write!(f, "String@{:#x}", self.0 & !TAG_MASK),
            ValueKind::Float => {
                write!(f, "Float({})", self.xfloat())
            }
            ValueKind::Subr(sym_id) => write!(f, "Subr({:?})", sym_id),
            ValueKind::Veclike(ty) => write!(f, "{:?}@{:#x}", ty, self.0 & !TAG_MASK),
            ValueKind::Unbound => write!(f, "#<unbound>"),
            ValueKind::Unknown => write!(f, "Unknown({:#x})", self.0),
        }
    }
}
