//! Forwarder descriptors for `SYMBOL_FORWARDED` symbols.
//!
//! Mirrors GNU Emacs's `Lisp_Fwd` family in `src/lisp.h:3060-3145`. A
//! forwarded symbol stores a pointer to a static [`LispFwd`] descriptor;
//! reads and writes go through the descriptor instead of touching the
//! symbol's value cell directly. This is how variables like
//! `buffer-file-name`, `point`, `mark-active`, and `case-fold-search`
//! get their backing storage in dedicated C-side slots.
//!
//! # What a forward type enforces
//!
//! The reason GNU routes assignment through the descriptor is not only
//! *where* the bytes land: each `Lisp_Fwd` variant also decides what the slot
//! will accept, and `store_symval_forwarding` (`src/data.c:1469-1530`) applies
//! that decision once, below every assignment path, so `Fset`, `set_default`,
//! `specbind` and the bytecode `varset` cannot each forget it.  The four rules
//! are genuinely different and do not generalise from one another:
//!
//! | variant             | rule at assignment                                     |
//! |---------------------|--------------------------------------------------------|
//! | `Lisp_Fwd_Int`      | `CHECK_INTEGER`, then `integer_to_intmax` or `overflow-error` (`data.c:1475-1483`) |
//! | `Lisp_Fwd_Bool`     | no signal at all — coerces to `!NILP (newval)` (`data.c:1485-1487`) |
//! | `Lisp_Fwd_Obj`      | anything (`data.c:1489-1516`)                           |
//! | `Lisp_Fwd_Buffer_Obj` | the slot's closed predicate, bypassed for `nil` (`data.c:1518-1526`) |
//! | `Lisp_Fwd_Kboard_Obj` | anything (`data.c:1529-1536`)                         |
//!
//! [`LispFwd::store`] is that switch.  It is the only way to produce a
//! [`ForwardStore`], and a [`ForwardStore`] is the only thing the storage
//! setters accept, so a caller cannot reach a forwarded slot with a value the
//! forward type has not seen.
//!
//! # Implementation status
//!
//! All five variants are wired.  `Int` (GNU's `Lisp_Intfwd`), `Bool`
//! (`Lisp_Boolfwd`) and `BufferObj` (`Lisp_Buffer_Objfwd`) were wired by
//! ledger entries 132 and the Phase 8/10 work; `Obj` (`Lisp_Objfwd`) and
//! `KboardObj` (`Lisp_Kboard_Objfwd`) by ledger 170.
//!
//! # What being `SYMBOL_FORWARDED` costs beyond the store rule
//!
//! The table above is not the whole of the difference, and entry 132's
//! conclusion that `Lisp_Fwd_Obj` "enforces nothing, so no divergence
//! follows" was wrong by 447 names (measured by entry 168, re-derived by
//! entry 170).  GNU's redirect switch reaches three refusals BEFORE it ever
//! looks at what the descriptor points to:
//!
//! - `set_internal` refuses an unbind: `error ("Built-in variable may not be
//!   unbound : %s")` (`src/data.c:1802-1809`), and the localized twin at
//!   `src/data.c:1723-1727`, which keys on the mere presence of `blv->fwd`.
//!   There is no "unbound" bit pattern in a C slot.
//! - `Fdefvaralias` refuses the symbol as a NEW-ALIAS: `error ("Cannot make a
//!   built-in variable an alias: %s")` (`src/eval.c:665-668`).
//! - For `Lisp_Fwd_Kboard_Obj` only, `Fmake_variable_buffer_local` and
//!   `Fmake_local_variable` refuse it -- `error ("Symbol %s may not be
//!   buffer-local")` (`src/data.c:2220-2223`, `src/data.c:2286-2288`) -- and
//!   `variable-binding-locus` answers the terminal (`src/data.c:2519-2521`).
//!
//! Every one keys on the symbol's redirect TAG, which is why entry 170's fix
//! is the tag and the storage is only what the tag costs: a symbol's value
//! cell is one word, so once it holds the descriptor pointer the value needs
//! somewhere else to live.  See `emacs_core::defvar_object` for the
//! declaration table and the adoption pass, and `Obarray::trace_roots` for
//! why a leaked descriptor owning a heap [`Value`] is a rooted object here
//! rather than the failure class entries 161-163 closed -- `Int` has worked
//! that way since 132, and `LispFwd::owned_value` now decides membership so
//! a new variant cannot be added and left untraced.

use super::value::Value;
use crate::buffer::buffer::BufferSlotPredicateError;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, Ordering};

/// Discriminant for [`LispFwd`]. Mirrors GNU `enum Lisp_Fwd_Type`
/// (`src/lisp.h:3046-3055`). Always read the first field of any `*Fwd`
/// struct to determine its concrete type — exactly the GNU trick.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum LispFwdType {
    /// `Lisp_Intfwd`: forward to a static `intmax_t`.
    Int = 0,
    /// `Lisp_Boolfwd`: forward to a static `bool`.
    Bool = 1,
    /// `Lisp_Objfwd`: forward to a static `Lisp_Object` (a top-level
    /// global variable).
    Obj = 2,
    /// `Lisp_Buffer_Objfwd`: forward to a slot inside the current
    /// buffer's per-buffer storage.
    BufferObj = 3,
    /// `Lisp_Kboard_Objfwd`: forward to a slot inside the current
    /// keyboard's per-kboard storage.
    KboardObj = 4,
}

impl LispFwdType {
    pub fn from_gnu_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    pub fn gnu_code(self) -> u8 {
        self.into()
    }
}

/// Common header. Every `Lisp_*Fwd` struct begins with this so the
/// dispatch code can read the discriminant from a `*const LispFwd`
/// without knowing the concrete type. Mirrors GNU `lispfwd` (`lisp.h:760`)
/// + the `type` field on each `Lisp_*fwd` body (`lisp.h:3060-3094`).
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct LispFwd {
    pub ty: LispFwdType,
    // The body fields differ per variant. Code that has a `*const LispFwd`
    // matches on `ty` and re-casts to the concrete `Lisp*Fwd` pointer.
}

/// A value one forward type has accepted, in the form that type stores.
///
/// Produced only by [`LispFwd::store`], and the only argument the forwarded
/// setters take.  That is what stops the enforcement from being an invariant
/// each assignment site has to remember: a write cannot be spelled without one
/// of these, and one of these cannot be obtained without the type's rule
/// having run.
#[derive(Copy, Clone, Debug)]
pub enum ForwardStore {
    /// `Lisp_Fwd_Int` -- checked and in `intmax_t` range.
    Int(LispInteger),
    /// `Lisp_Fwd_Bool` -- already collapsed to `!NILP (newval)`.
    Bool(bool),
    /// `Lisp_Fwd_Obj`, `Lisp_Fwd_Buffer_Obj`, `Lisp_Fwd_Kboard_Obj` -- the
    /// Lisp object is stored verbatim (the per-buffer predicate, if any, has
    /// already passed).
    Object(Value),
}

impl ForwardStore {
    /// The Lisp object a read of the slot will return after this store.
    ///
    /// GNU gets this for free: the write goes through `store_symval_forwarding`
    /// and the read comes back through `do_symval_forwarding`, so a Boolean
    /// slot given `5` reads back `t`.  Neomacs canonicalises once on the way in
    /// instead, which is observationally the same and keeps the per-buffer and
    /// buffer-local storage paths -- which hold `Value`s, not C slots -- from
    /// needing a round trip of their own.
    #[inline]
    pub fn canonical_value(self) -> Value {
        match self {
            Self::Int(integer) => integer.value(),
            Self::Bool(flag) => Value::bool_val(flag),
            Self::Object(value) => value,
        }
    }
}

impl LispFwd {
    /// GNU `store_symval_forwarding` (`src/data.c:1469-1530`): the one switch
    /// on the forward type that decides whether an assignment is allowed and
    /// what the slot will hold.
    ///
    /// # Safety
    ///
    /// `self` must be the header of a live descriptor of the variant its `ty`
    /// names -- the invariant every `install_*fwd` upholds by leaking a
    /// `'static` descriptor and never re-tagging the symbol.
    pub fn store(&self, newval: Value) -> Result<ForwardStore, ForwardStoreError> {
        match self.ty {
            LispFwdType::Int => Ok(ForwardStore::Int(LispInteger::check(newval)?)),
            LispFwdType::Bool => Ok(ForwardStore::Bool(!newval.is_nil())),
            LispFwdType::BufferObj => {
                // GNU checks the predicate only for a non-nil value
                // (`data.c:1520-1521`); `BufferSlotPredicate::check` already
                // encodes that bypass.
                let buf_fwd = unsafe { &*(self as *const Self as *const LispBufferObjFwd) };
                buf_fwd.predicate.check(newval)?;
                Ok(ForwardStore::Object(newval))
            }
            LispFwdType::Obj | LispFwdType::KboardObj => Ok(ForwardStore::Object(newval)),
        }
    }

    /// GNU `do_symval_forwarding` (`src/data.c:1337-1360`) for the variants
    /// whose storage lives in the descriptor itself.
    ///
    /// `BufferObj` is the only variant that reads out of context this borrow
    /// does not have (the current buffer's slot array), so it is the only one
    /// that answers `None`.
    pub fn load(&self) -> Option<Value> {
        match self.ty {
            LispFwdType::Int => {
                let int_fwd = unsafe { &*(self as *const Self as *const LispIntFwd) };
                Some(int_fwd.get())
            }
            LispFwdType::Bool => {
                let bool_fwd = unsafe { &*(self as *const Self as *const LispBoolFwd) };
                Some(Value::bool_val(bool_fwd.get()))
            }
            LispFwdType::Obj => {
                let obj_fwd = unsafe { &*(self as *const Self as *const LispObjFwd) };
                Some(obj_fwd.get())
            }
            LispFwdType::KboardObj => {
                let kbd_fwd = unsafe { &*(self as *const Self as *const LispKboardObjFwd) };
                Some(kbd_fwd.get())
            }
            LispFwdType::BufferObj => None,
        }
    }

    /// The heap [`Value`] this descriptor OWNS, for the GC root scan.
    ///
    /// GNU needs no equivalent: its `Lisp_Intfwd` slot is an `intmax_t`, its
    /// `Lisp_Boolfwd` slot a `bool`, and its `Lisp_Objfwd` /
    /// `Lisp_Kboard_Objfwd` slots are inside `struct emacs_globals` and
    /// `struct KBOARD`, which `staticpro` and `mark_kboards` already reach.
    /// Here those slots live in the leaked descriptor, so the descriptor is
    /// the root -- and this is the one place that decides which variants are
    /// roots at all, rather than each registry deciding for itself.
    pub fn owned_value(&self) -> Option<Value> {
        match self.ty {
            // `Bool` owns a native `bool`; `BufferObj`'s storage is the
            // buffer's slot array, traced with the buffer.
            LispFwdType::Bool | LispFwdType::BufferObj => None,
            LispFwdType::Int | LispFwdType::Obj | LispFwdType::KboardObj => self.load(),
        }
    }

    /// [`Self::load`] for a leaked descriptor, for the `&Value`-returning
    /// symbol accessors.
    pub fn load_ref(&'static self) -> Option<&'static Value> {
        match self.ty {
            LispFwdType::Int => {
                let int_fwd = unsafe { &*(self as *const Self as *const LispIntFwd) };
                Some(int_fwd.get_ref())
            }
            LispFwdType::Bool => {
                let bool_fwd = unsafe { &*(self as *const Self as *const LispBoolFwd) };
                Some(if bool_fwd.get() {
                    &Value::T
                } else {
                    &Value::NIL
                })
            }
            LispFwdType::Obj => {
                let obj_fwd = unsafe { &*(self as *const Self as *const LispObjFwd) };
                Some(obj_fwd.get_ref())
            }
            LispFwdType::KboardObj => {
                let kbd_fwd = unsafe { &*(self as *const Self as *const LispKboardObjFwd) };
                Some(kbd_fwd.get_ref())
            }
            LispFwdType::BufferObj => None,
        }
    }

    /// Duplicate a descriptor that OWNS mutable state, so two obarrays never
    /// share one slot.
    ///
    /// `Int`, `Bool`, `Obj` and `KboardObj` hold the variable's value;
    /// `BufferObj` holds only immutable registration metadata (offset,
    /// predicate, default) and is safe to share, which is why it answers
    /// `None`.
    pub fn clone_stateful(&'static self) -> Option<&'static Self> {
        match self.ty {
            LispFwdType::Int => {
                let int_fwd = unsafe { &*(self as *const Self as *const LispIntFwd) };
                // Re-wrapping without re-checking is sound only here, inside
                // the module that owns the invariant: the value being copied
                // came out of a slot that `LispInteger::check` already passed.
                let copy = alloc_intfwd(LispInteger(int_fwd.get()));
                Some(unsafe { &*(copy as *const LispIntFwd as *const Self) })
            }
            LispFwdType::Bool => {
                let bool_fwd = unsafe { &*(self as *const Self as *const LispBoolFwd) };
                let copy = alloc_boolfwd(bool_fwd.get());
                Some(unsafe { &*(copy as *const LispBoolFwd as *const Self) })
            }
            LispFwdType::Obj => {
                let obj_fwd = unsafe { &*(self as *const Self as *const LispObjFwd) };
                let copy = alloc_objfwd(obj_fwd.get());
                Some(unsafe { &*(copy as *const LispObjFwd as *const Self) })
            }
            LispFwdType::KboardObj => {
                let kbd_fwd = unsafe { &*(self as *const Self as *const LispKboardObjFwd) };
                let copy = alloc_kboard_objfwd(kbd_fwd.get());
                Some(unsafe { &*(copy as *const LispKboardObjFwd as *const Self) })
            }
            LispFwdType::BufferObj => None,
        }
    }

    /// The descriptor as an integer forwarder, if that is what it is.
    pub fn as_int_fwd(&'static self) -> Option<&'static LispIntFwd> {
        (self.ty == LispFwdType::Int)
            .then(|| unsafe { &*(self as *const Self as *const LispIntFwd) })
    }

    /// The descriptor as a Boolean forwarder, if that is what it is.
    pub fn as_bool_fwd(&'static self) -> Option<&'static LispBoolFwd> {
        (self.ty == LispFwdType::Bool)
            .then(|| unsafe { &*(self as *const Self as *const LispBoolFwd) })
    }

    /// The descriptor as a Lisp-object forwarder, if that is what it is.
    pub fn as_obj_fwd(&'static self) -> Option<&'static LispObjFwd> {
        (self.ty == LispFwdType::Obj)
            .then(|| unsafe { &*(self as *const Self as *const LispObjFwd) })
    }

    /// The descriptor as a keyboard-object forwarder, if that is what it is.
    pub fn as_kboard_obj_fwd(&'static self) -> Option<&'static LispKboardObjFwd> {
        (self.ty == LispFwdType::KboardObj)
            .then(|| unsafe { &*(self as *const Self as *const LispKboardObjFwd) })
    }

    /// Perform the store for the variants whose storage is the descriptor.
    /// Returns the canonical value so callers that also mirror the write into
    /// buffer-local storage do not have to recompute it.
    pub fn commit(&self, store: ForwardStore) -> Value {
        match store {
            ForwardStore::Int(integer) => {
                debug_assert_eq!(self.ty, LispFwdType::Int);
                let int_fwd = unsafe { &*(self as *const Self as *const LispIntFwd) };
                int_fwd.set(integer);
            }
            ForwardStore::Bool(flag) => {
                debug_assert_eq!(self.ty, LispFwdType::Bool);
                let bool_fwd = unsafe { &*(self as *const Self as *const LispBoolFwd) };
                bool_fwd.set(flag);
            }
            ForwardStore::Object(value) => match self.ty {
                LispFwdType::Obj => {
                    let obj_fwd = unsafe { &*(self as *const Self as *const LispObjFwd) };
                    obj_fwd.set(value);
                }
                LispFwdType::KboardObj => {
                    let kbd_fwd = unsafe { &*(self as *const Self as *const LispKboardObjFwd) };
                    kbd_fwd.set(value);
                }
                // A per-buffer slot's storage is the buffer's slot array; the
                // caller writes it.
                LispFwdType::BufferObj => {}
                LispFwdType::Int | LispFwdType::Bool => {
                    debug_assert!(false, "{:?} cannot store a Lisp object", self.ty)
                }
            },
        }
        store.canonical_value()
    }
}

/// Why a forwarded slot refused a value.
///
/// GNU signals from inside `store_symval_forwarding` itself.  Neomacs returns
/// the refusal instead, so the storage layer stays independent of the
/// evaluator's non-local control flow -- the same split
/// [`BufferSlotPredicateError`] already uses.  The evaluator maps each variant
/// to GNU's signal data at the boundary.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ForwardStoreError {
    /// GNU `wrong_type_argument (Qintegerp, newval)` and friends.
    WrongType(&'static str),
    /// GNU `xsignal1 (Qoverflow_error, newval)` -- an integer past `intmax_t`.
    Overflow,
    /// A per-buffer slot's closed predicate said no.
    Predicate(BufferSlotPredicateError),
}

impl From<BufferSlotPredicateError> for ForwardStoreError {
    fn from(error: BufferSlotPredicateError) -> Self {
        Self::Predicate(error)
    }
}

/// A Lisp integer a `Lisp_Fwd_Int` slot will accept: an integer whose value
/// fits `intmax_t`.
///
/// GNU's slot is an `intmax_t`, so "not an integer" and "an integer too big
/// for the slot" are unrepresentable there by construction.  This newtype is
/// how that is unrepresentable here: the only constructor is
/// [`LispInteger::check`], which is GNU's `CHECK_INTEGER` +
/// `integer_to_intmax` pair (`src/data.c:1475-1483`), and it is the only thing
/// [`LispIntFwd::set`] accepts.
#[derive(Copy, Clone, Debug)]
pub struct LispInteger(Value);

impl LispInteger {
    /// GNU `CHECK_INTEGER (newval)` followed by `integer_to_intmax`.
    pub fn check(value: Value) -> Result<Self, ForwardStoreError> {
        if !value.is_integer() {
            return Err(ForwardStoreError::WrongType("integerp"));
        }
        if value.is_fixnum() {
            return Ok(Self(value));
        }
        match value.as_bignum().and_then(|big| i64::try_from(big).ok()) {
            Some(_) => Ok(Self(value)),
            None => Err(ForwardStoreError::Overflow),
        }
    }

    /// Build one from a Rust integer. Infallible: every `i64` fits the slot.
    pub fn from_i64(value: i64) -> Self {
        Self(Value::make_int(value))
    }

    /// The Lisp object to store.
    #[inline]
    pub fn value(self) -> Value {
        self.0
    }

    /// The slot's value as GNU's `intmax_t` would hold it.
    #[inline]
    pub fn as_i64(self) -> i64 {
        match self.0.as_fixnum() {
            Some(small) => small,
            // `check`/`from_i64` are the only constructors and both guarantee
            // `intmax_t` range, so the bignum conversion cannot fail here.
            None => self
                .0
                .as_bignum()
                .and_then(|big| i64::try_from(big).ok())
                .unwrap_or(0),
        }
    }
}

/// `Lisp_Intfwd`: forward to an integer slot (`src/lisp.h:3124`).
///
/// GNU stores an `intmax_t`. Neomacs stores the Lisp integer itself so the
/// `&Value`-returning symbol accessors have a stable address to hand back, but
/// the field is private and [`Self::set`] takes a [`LispInteger`], so the slot
/// is exactly as unable to hold a string as GNU's `intmax_t` is.
#[repr(C)]
pub struct LispIntFwd {
    pub ty: LispFwdType,
    /// Always an integer inside `intmax_t` range -- see [`LispInteger`].
    /// `UnsafeCell` because the descriptor is shared as `&'static` while the
    /// single Lisp thread writes through it, mirroring the `val.plain` symbol
    /// cell, which is likewise written in place behind a shared borrow.
    value: UnsafeCell<Value>,
}

// Safety: a `LispIntFwd` is only ever mutated from the Lisp thread that owns
// its `Obarray`, exactly like the symbol value cells beside it; the `Sync`
// bound is needed solely so the descriptor can be a `&'static` shared with the
// GC's root scan, which reads it with `load_value_atomic`.
unsafe impl Sync for LispIntFwd {}

impl LispIntFwd {
    /// GNU `do_symval_forwarding`'s `Lisp_Fwd_Int` arm (`src/data.c:1341-1342`),
    /// which wraps the C slot back up with `make_int`.
    #[inline]
    pub fn get(&self) -> Value {
        crate::tagged::header::load_value_atomic(unsafe { &*self.value.get() })
    }

    /// The slot as GNU's C reads it: a plain `intmax_t`, no Lisp object.
    ///
    /// GNU's C never goes through `do_symval_forwarding` for its own globals --
    /// `num_nonmacro_input_events` and `when_entered_debugger` are compared as
    /// integers (`src/eval.c:2212`) -- so a caller that wants the number should
    /// not have to re-derive the invariant [`LispInteger`] already carries.
    #[inline]
    pub fn get_i64(&self) -> i64 {
        let stored = self.get();
        match stored.as_fixnum() {
            Some(small) => small,
            // `set` takes a `LispInteger`, whose constructors both guarantee
            // `intmax_t` range, so this conversion cannot fail.
            None => stored
                .as_bignum()
                .and_then(|big| i64::try_from(big).ok())
                .unwrap_or(0),
        }
    }

    /// Borrow the stored integer. The descriptor is leaked at registration, so
    /// the borrow is genuinely `'static`.
    #[inline]
    pub fn get_ref(&'static self) -> &'static Value {
        unsafe { &*self.value.get() }
    }

    /// The store half of GNU's `Lisp_Fwd_Int` arm. Takes a [`LispInteger`],
    /// so there is no spelling of this call that stores a non-integer.
    #[inline]
    pub fn set(&self, value: LispInteger) {
        let slot = unsafe { &mut *self.value.get() };
        // SATB: a bignum slot value is a heap object about to be replaced.
        crate::tagged::gc::note_root_overwrite(*slot);
        crate::tagged::header::store_value_atomic(slot, value.value());
    }
}

/// `Lisp_Boolfwd`: forward to a native Boolean cell.
///
/// GNU stores a pointer to a C `bool`.  Each Neomacs context owns an
/// independently leaked descriptor instead, avoiding process-global state
/// between evaluators while retaining the same forwarded-value semantics.
#[repr(C)]
pub struct LispBoolFwd {
    pub ty: LispFwdType,
    value: AtomicBool,
}

impl LispBoolFwd {
    #[inline]
    pub fn get(&self) -> bool {
        self.value.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn set(&self, value: bool) {
        self.value.store(value, Ordering::Relaxed);
    }
}

/// `Lisp_Objfwd`: forward to a `Value` global (`src/lisp.h:3479-3484`).
///
/// GNU's descriptor holds `&globals.f_Vfoo`, a slot inside `struct
/// emacs_globals` that `staticpro` roots.  Neomacs's holds the slot itself,
/// for the same reason [`LispIntFwd`] does: the descriptor is leaked at
/// registration, so it is a stable address the `&Value`-returning symbol
/// accessors can hand back, and the obarray's root scan roots it exactly the
/// way `staticpro` roots GNU's.
#[repr(C)]
pub struct LispObjFwd {
    pub ty: LispFwdType,
    /// `UnsafeCell` for the same reason as [`LispIntFwd`]'s slot: the
    /// descriptor is shared as `&'static` while the single Lisp thread writes
    /// through it, mirroring the `val.plain` symbol cell beside it.
    value: UnsafeCell<Value>,
}

// Safety: identical to `LispIntFwd` -- mutated only from the Lisp thread that
// owns its `Obarray`; `Sync` is needed solely so the descriptor can be the
// `&'static` the GC root scan reads with `load_value_atomic`.
unsafe impl Sync for LispObjFwd {}

impl LispObjFwd {
    /// GNU `do_symval_forwarding`'s `Lisp_Fwd_Obj` arm (`src/data.c:1343-1344`).
    #[inline]
    pub fn get(&self) -> Value {
        crate::tagged::header::load_value_atomic(unsafe { &*self.value.get() })
    }

    /// Borrow the stored value.  The descriptor is leaked at registration, so
    /// the borrow is genuinely `'static`.
    #[inline]
    pub fn get_ref(&'static self) -> &'static Value {
        unsafe { &*self.value.get() }
    }

    /// The store half of GNU's `Lisp_Fwd_Obj` arm (`src/data.c:1489-1516`).
    #[inline]
    pub fn set(&self, value: Value) {
        let slot = unsafe { &mut *self.value.get() };
        // SATB: the slot holds a heap object about to be replaced.
        crate::tagged::gc::note_root_overwrite(*slot);
        crate::tagged::header::store_value_atomic(slot, value);
    }
}

/// `Lisp_Buffer_Objfwd`: forward to a per-buffer slot. The `offset`
/// field indexes into `Buffer::slots: [Value; BUFFER_SLOT_COUNT]`,
/// playing the same role as GNU's `Lisp_Buffer_Objfwd::offset` (a byte
/// offset into `struct buffer`). Phase 8 wires this up.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct LispBufferObjFwd {
    pub ty: LispFwdType,
    /// Index into `Buffer::slots`. Mirrors GNU `Lisp_Buffer_Objfwd::offset`.
    pub offset: u16,
    /// Index into `buffer_local_flags` for "is this buffer-local in the
    /// current buffer?" tests. -1 means "always local everywhere",
    /// matching GNU's `PER_BUFFER_IDX(idx) == -1`.
    pub local_flags_idx: i16,
    /// Closed write predicate checked for live-slot writes. Mirrors GNU
    /// `enum Lisp_Fwd_Predicate` instead of encoding this finite domain as an
    /// open-ended Lisp symbol.
    pub predicate: crate::buffer::buffer::BufferSlotPredicate,
    /// Default value copied into `Buffer::slots[offset]` at buffer
    /// creation. Mirrors GNU `buffer_defaults`.
    pub default: Value,
}

/// `Lisp_Kboard_Objfwd`: forward to a per-keyboard slot (`src/lisp.h:3490-3495`).
///
/// GNU's descriptor holds `offsetof (KBOARD, vname_)` and
/// `do_symval_forwarding` applies it to `current_kboard`
/// (`src/data.c:1352-1356`).  Neomacs has one keyboard, so the offset would
/// index a one-element array; the slot lives in the descriptor instead, which
/// is the same storage with the indirection folded out.  What the variant
/// really carries is the two refusals `Lisp_Fwd_Obj` does not have --
/// `Fmake_variable_buffer_local` and `Fmake_local_variable` both signal
/// "Symbol %s may not be buffer-local" for it (`src/data.c:2220-2223`,
/// `src/data.c:2287-2288`) -- and those key on the TYPE, not on the offset.
///
/// The second half of the same single-terminal limitation, which ledger 170
/// named and ledger 183 re-checked against the source: `specbind` records
/// `specpdl_ptr->let.where.kbd = kboard_for_bindings ()` for a keyboard
/// variable and takes `SPECPDL_LET` rather than `SPECPDL_LET_LOCAL`
/// (`src/eval.c:3681-3683`), so `unbind_to` restores the old value into the
/// keyboard the binding was made on.  With one keyboard there is nowhere else
/// to restore it to, so the field has no counterpart here; a second terminal
/// would need both it and the offset back, together, and neither is
/// observable from Lisp until then.
#[repr(C)]
pub struct LispKboardObjFwd {
    pub ty: LispFwdType,
    /// See [`LispObjFwd`] for why this is an `UnsafeCell`.
    value: UnsafeCell<Value>,
}

// Safety: see `LispObjFwd`.
unsafe impl Sync for LispKboardObjFwd {}

impl LispKboardObjFwd {
    /// GNU `do_symval_forwarding`'s `Lisp_Fwd_Kboard_Obj` arm
    /// (`src/data.c:1352-1356`).
    #[inline]
    pub fn get(&self) -> Value {
        crate::tagged::header::load_value_atomic(unsafe { &*self.value.get() })
    }

    /// Borrow the stored value; the descriptor is leaked at registration.
    #[inline]
    pub fn get_ref(&'static self) -> &'static Value {
        unsafe { &*self.value.get() }
    }

    /// GNU `store_symval_forwarding`'s `Lisp_Fwd_Kboard_Obj` arm
    /// (`src/data.c:1529-1536`), which checks nothing.
    #[inline]
    pub fn set(&self, value: Value) {
        let slot = unsafe { &mut *self.value.get() };
        crate::tagged::gc::note_root_overwrite(*slot);
        crate::tagged::header::store_value_atomic(slot, value);
    }
}

// ===========================================================================
// Phase 8a — BUFFER_OBJFWD allocation and registration
// ===========================================================================

/// Leak a fresh [`LispBufferObjFwd`] descriptor into a `'static`
/// pointer. Mirrors GNU's `defvar_per_buffer` (`buffer.c:4990-5012`):
/// every per-buffer forwarder is allocated once at process init and
/// lives until exit. NeoMacs uses `Box::leak` instead of static
/// initialization because the per-process forwarders are constructed
/// from runtime data (slot index assignments).
///
/// `offset` is the index into [`crate::buffer::buffer::Buffer::slots`].
/// `local_flags_idx` mirrors GNU's `local-flags` index: `-1` means
/// "always-local in every buffer" (e.g. `buffer-file-name`,
/// `point`); a positive index points at a bit in
/// `Buffer::local_flags` (currently unused — Phase 8b will wire it).
/// `predicate` is the closed predicate used by `store_symval_forwarding`.
/// `default` is the value copied into every fresh buffer's slot.
pub fn alloc_buffer_objfwd(
    offset: u16,
    local_flags_idx: i16,
    predicate: crate::buffer::buffer::BufferSlotPredicate,
    default: Value,
) -> &'static LispBufferObjFwd {
    let fwd = Box::new(LispBufferObjFwd {
        ty: LispFwdType::BufferObj,
        offset,
        local_flags_idx,
        predicate,
        default,
    });
    Box::leak(fwd)
}

/// Allocate a process-lifetime native Boolean forwarder.
///
/// GNU's `DEFVAR_BOOL` descriptors are static C objects.  Neomacs constructs
/// contexts dynamically, so leaking one tiny descriptor per registered
/// variable and context provides the same stable-pointer contract without
/// coupling otherwise independent evaluators through a global Boolean.
pub fn alloc_boolfwd(initial: bool) -> &'static LispBoolFwd {
    Box::leak(Box::new(LispBoolFwd {
        ty: LispFwdType::Bool,
        value: AtomicBool::new(initial),
    }))
}

/// Allocate a process-lifetime Lisp-object forwarder, GNU's `DEFVAR_LISP`
/// slot.
///
/// Leaked for the same reason as [`alloc_boolfwd`]: GNU's descriptors are
/// static C objects, and the `&Value`-returning symbol accessors hand out
/// borrows of the slot, so the descriptor must outlive every reader.  Unlike
/// GNU's, this slot IS the storage, so whoever installs it must also register
/// it as a GC root -- `Obarray::install_objfwd` is the only caller and does
/// exactly that.
pub fn alloc_objfwd(initial: Value) -> &'static LispObjFwd {
    Box::leak(Box::new(LispObjFwd {
        ty: LispFwdType::Obj,
        value: UnsafeCell::new(initial),
    }))
}

/// Allocate a process-lifetime keyboard-object forwarder, GNU's
/// `DEFVAR_KBOARD` slot.  See [`alloc_objfwd`].
pub fn alloc_kboard_objfwd(initial: Value) -> &'static LispKboardObjFwd {
    Box::leak(Box::new(LispKboardObjFwd {
        ty: LispFwdType::KboardObj,
        value: UnsafeCell::new(initial),
    }))
}

/// Allocate a process-lifetime integer forwarder, GNU's `DEFVAR_INT` slot.
///
/// Leaked for the same reason as [`alloc_boolfwd`]: GNU's descriptors are
/// static C objects, and the `&Value`-returning symbol accessors hand out
/// borrows of the slot, so the descriptor must outlive every reader.
pub fn alloc_intfwd(initial: LispInteger) -> &'static LispIntFwd {
    Box::leak(Box::new(LispIntFwd {
        ty: LispFwdType::Int,
        value: UnsafeCell::new(initial.value()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lisp_fwd_type_codes_match_gnu_lisp_fwd_type() {
        let cases = [
            (LispFwdType::Int, 0),
            (LispFwdType::Bool, 1),
            (LispFwdType::Obj, 2),
            (LispFwdType::BufferObj, 3),
            (LispFwdType::KboardObj, 4),
        ];

        for (ty, code) in cases {
            assert_eq!(ty.gnu_code(), code);
            assert_eq!(LispFwdType::from_gnu_code(code), Some(ty));
        }
        assert_eq!(LispFwdType::from_gnu_code(5), None);
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod gnu_parity_tests;
