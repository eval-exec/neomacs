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
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

static_assertions::assert_impl_all!(LispFwdType: Copy, Clone, std::fmt::Debug, Send, Sync);

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
///
/// # Invariant
///
/// A `&LispFwd` always addresses the header of a live descriptor of the
/// family its [`ty`](Self::ty) names. That is what makes [`Self::slot`] --
/// the one place the header is re-cast to its body -- sound, and it holds by
/// construction:
/// - the header's field is private, so no code outside this module can build
///   one;
/// - it is neither `Copy` nor `Clone` (pinned below), so a header cannot be
///   lifted out of its descriptor and re-read without the body after it;
/// - every body's `ty` is private too and set only by its allocator, and the
///   only way from a body to a `&LispFwd` is [`FwdDescriptor::header`].
///
/// A symbol's `Forwarded` value cell holds such a reference, which is why
/// that cell cannot be made to point at anything but a real descriptor.
#[repr(C)]
#[derive(Debug)]
pub struct LispFwd {
    ty: LispFwdType,
    // `slot` can expose BufferObj registration metadata containing a raw
    // Value, so an erased header is confined even when a concrete body is atomic.
    _thread_confined: PhantomData<*const ()>,
}

static_assertions::assert_impl_all!(LispFwd: std::fmt::Debug);
static_assertions::assert_not_impl_any!(LispFwd: Copy, Clone, Send, Sync);
const _: () = {
    assert!(std::mem::size_of::<LispFwd>() == 1);
    assert!(std::mem::align_of::<LispFwd>() == 1);
    assert!(std::mem::offset_of!(LispFwd, ty) == 0);
    assert!(std::mem::size_of::<PhantomData<*const ()>>() == 0);
};

/// A descriptor body that begins with a [`LispFwd`] header (GNU's `struct
/// Lisp_Intfwd`, `Lisp_Boolfwd`, ...): the safe upcast every installer uses
/// instead of a pointer cast. Sealed: only this module's five bodies are
/// descriptors.
pub trait FwdDescriptor: sealed::Sealed {
    /// The family every value of this body carries in its header.
    const TYPE: LispFwdType;

    /// The header this body starts with.
    fn header(&self) -> &LispFwd;
}

mod sealed {
    pub trait Sealed {}
}

macro_rules! fwd_descriptor {
    ($body:ty, $family:ident) => {
        impl sealed::Sealed for $body {}

        impl FwdDescriptor for $body {
            const TYPE: LispFwdType = LispFwdType::$family;

            #[inline(always)]
            fn header(&self) -> &LispFwd {
                debug_assert_eq!(self.ty, Self::TYPE);
                // SAFETY: the body is `#[repr(C)]` and its first field is
                // the same `LispFwdType` at offset zero. LispFwd's only other
                // field is a zero-sized PhantomData, with no validity or
                // storage requirements; size/alignment are pinned below.
                // The returned borrow has the body's lifetime.
                unsafe { &*std::ptr::from_ref(self).cast::<LispFwd>() }
            }
        }

        const _: () = assert!(std::mem::offset_of!($body, ty) == 0);
    };
}

fwd_descriptor!(LispIntFwd, Int);
fwd_descriptor!(LispBoolFwd, Bool);
fwd_descriptor!(LispObjFwd, Obj);
fwd_descriptor!(LispBufferObjFwd, BufferObj);
fwd_descriptor!(LispKboardObjFwd, KboardObj);

/// A descriptor resolved to its family: GNU's `XFWDTYPE` switch together with
/// the `XINTFWD` / `XBOOLFWD` / `XOBJFWD` / `XBUFFER_OBJFWD` /
/// `XKBOARD_OBJFWD` casts it guards, as one exhaustive value.
///
/// [`LispFwd::slot`] is the only producer, so a caller matching on this can
/// neither forget a family nor read a body as the wrong one.
#[derive(Clone, Copy, Debug)]
pub enum ForwardSlot<'a> {
    /// `Lisp_Intfwd`.
    Int(&'a LispIntFwd),
    /// `Lisp_Boolfwd`.
    Bool(&'a LispBoolFwd),
    /// `Lisp_Objfwd`.
    Obj(&'a LispObjFwd),
    /// `Lisp_Buffer_Objfwd`.
    BufferObj(&'a LispBufferObjFwd),
    /// `Lisp_Kboard_Objfwd`.
    KboardObj(&'a LispKboardObjFwd),
}

static_assertions::assert_impl_all!(ForwardSlot<'static>: Copy, Clone, std::fmt::Debug);
static_assertions::assert_not_impl_any!(ForwardSlot<'static>: Send, Sync);

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

static_assertions::assert_impl_all!(ForwardStore: Copy, Clone, std::fmt::Debug);
static_assertions::assert_not_impl_any!(ForwardStore: Send, Sync);

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
    /// The descriptor family the header names.
    #[inline(always)]
    pub fn ty(&self) -> LispFwdType {
        self.ty
    }

    /// The descriptor resolved to its family (see [`ForwardSlot`]).
    #[inline]
    pub fn slot(&self) -> ForwardSlot<'_> {
        let header = std::ptr::from_ref(self);
        // SAFETY: by the type's invariant `self` heads a live descriptor of
        // the family `ty` names, and every body is `#[repr(C)]` with that
        // header first (`fwd_descriptor!`), so the cast names the body the
        // header was allocated with. The borrow keeps `self`'s lifetime.
        unsafe {
            match self.ty {
                LispFwdType::Int => ForwardSlot::Int(&*header.cast::<LispIntFwd>()),
                LispFwdType::Bool => ForwardSlot::Bool(&*header.cast::<LispBoolFwd>()),
                LispFwdType::Obj => ForwardSlot::Obj(&*header.cast::<LispObjFwd>()),
                LispFwdType::BufferObj => {
                    ForwardSlot::BufferObj(&*header.cast::<LispBufferObjFwd>())
                }
                LispFwdType::KboardObj => {
                    ForwardSlot::KboardObj(&*header.cast::<LispKboardObjFwd>())
                }
            }
        }
    }

    /// GNU `store_symval_forwarding` (`src/data.c:1469-1530`): the one switch
    /// on the forward type that decides whether an assignment is allowed and
    /// what the slot will hold.
    pub fn store(&self, newval: Value) -> Result<ForwardStore, ForwardStoreError> {
        match self.slot() {
            ForwardSlot::Int(_) => Ok(ForwardStore::Int(LispInteger::check(newval)?)),
            ForwardSlot::Bool(_) => Ok(ForwardStore::Bool(!newval.is_nil())),
            ForwardSlot::BufferObj(buf_fwd) => {
                // GNU checks the predicate only for a non-nil value
                // (`data.c:1520-1521`); `BufferSlotPredicate::check` already
                // encodes that bypass.
                buf_fwd.predicate.check(newval)?;
                Ok(ForwardStore::Object(newval))
            }
            ForwardSlot::Obj(_) | ForwardSlot::KboardObj(_) => Ok(ForwardStore::Object(newval)),
        }
    }

    /// GNU `do_symval_forwarding` (`src/data.c:1337-1360`) for the variants
    /// whose storage lives in the descriptor itself.
    ///
    /// `BufferObj` is the only variant that reads out of context this borrow
    /// does not have (the current buffer's slot array), so it is the only one
    /// that answers `None`.
    pub fn load(&self) -> Option<Value> {
        match self.slot() {
            ForwardSlot::Int(int_fwd) => Some(int_fwd.get()),
            ForwardSlot::Bool(bool_fwd) => Some(Value::bool_val(bool_fwd.get())),
            ForwardSlot::Obj(obj_fwd) => Some(obj_fwd.get()),
            ForwardSlot::KboardObj(kbd_fwd) => Some(kbd_fwd.get()),
            ForwardSlot::BufferObj(_) => None,
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
        match self.slot() {
            // `Bool` owns a native `bool`; `BufferObj`'s storage is the
            // buffer's slot array, traced with the buffer.
            ForwardSlot::Bool(_) | ForwardSlot::BufferObj(_) => None,
            ForwardSlot::Int(int_fwd) => Some(int_fwd.get()),
            ForwardSlot::Obj(obj_fwd) => Some(obj_fwd.get()),
            ForwardSlot::KboardObj(kbd_fwd) => Some(kbd_fwd.get()),
        }
    }

    /// Duplicate a descriptor that OWNS mutable state, so two obarrays never
    /// share one slot.
    ///
    /// `Int`, `Bool`, `Obj` and `KboardObj` hold the variable's value;
    /// `BufferObj` holds immutable registration metadata (offset,
    /// predicate, default) that stays in the owning heap, so it answers
    /// `None`. This does not admit a raw default Value to another thread.
    pub fn clone_stateful(&self) -> Option<&'static Self> {
        match self.slot() {
            // Re-wrapping without re-checking is sound only here, inside the
            // module that owns the invariant: the value being copied came out
            // of a slot that `LispInteger::check` already passed.
            ForwardSlot::Int(int_fwd) => Some(alloc_intfwd(LispInteger(int_fwd.get())).header()),
            ForwardSlot::Bool(bool_fwd) => Some(alloc_boolfwd(bool_fwd.get()).header()),
            ForwardSlot::Obj(obj_fwd) => Some(alloc_objfwd(obj_fwd.get()).header()),
            ForwardSlot::KboardObj(kbd_fwd) => Some(alloc_kboard_objfwd(kbd_fwd.get()).header()),
            ForwardSlot::BufferObj(_) => None,
        }
    }

    /// The descriptor as an integer forwarder, if that is what it is.
    pub fn as_int_fwd(&self) -> Option<&LispIntFwd> {
        match self.slot() {
            ForwardSlot::Int(int_fwd) => Some(int_fwd),
            ForwardSlot::Bool(_)
            | ForwardSlot::Obj(_)
            | ForwardSlot::BufferObj(_)
            | ForwardSlot::KboardObj(_) => None,
        }
    }

    /// The descriptor as a Boolean forwarder, if that is what it is.
    pub fn as_bool_fwd(&self) -> Option<&LispBoolFwd> {
        match self.slot() {
            ForwardSlot::Bool(bool_fwd) => Some(bool_fwd),
            ForwardSlot::Int(_)
            | ForwardSlot::Obj(_)
            | ForwardSlot::BufferObj(_)
            | ForwardSlot::KboardObj(_) => None,
        }
    }

    /// The descriptor as a Lisp-object forwarder, if that is what it is.
    pub fn as_obj_fwd(&self) -> Option<&LispObjFwd> {
        match self.slot() {
            ForwardSlot::Obj(obj_fwd) => Some(obj_fwd),
            ForwardSlot::Int(_)
            | ForwardSlot::Bool(_)
            | ForwardSlot::BufferObj(_)
            | ForwardSlot::KboardObj(_) => None,
        }
    }

    /// The descriptor as a per-buffer slot forwarder, if that is what it is.
    pub fn as_buffer_obj_fwd(&self) -> Option<&LispBufferObjFwd> {
        match self.slot() {
            ForwardSlot::BufferObj(buf_fwd) => Some(buf_fwd),
            ForwardSlot::Int(_)
            | ForwardSlot::Bool(_)
            | ForwardSlot::Obj(_)
            | ForwardSlot::KboardObj(_) => None,
        }
    }

    /// The descriptor as a keyboard-object forwarder, if that is what it is.
    pub fn as_kboard_obj_fwd(&self) -> Option<&LispKboardObjFwd> {
        match self.slot() {
            ForwardSlot::KboardObj(kbd_fwd) => Some(kbd_fwd),
            ForwardSlot::Int(_)
            | ForwardSlot::Bool(_)
            | ForwardSlot::Obj(_)
            | ForwardSlot::BufferObj(_) => None,
        }
    }

    /// Perform the store for the variants whose storage is the descriptor.
    /// Returns the canonical value so callers that also mirror the write into
    /// buffer-local storage do not have to recompute it.
    pub fn commit(&self, store: ForwardStore) -> Value {
        match (store, self.slot()) {
            (ForwardStore::Int(integer), ForwardSlot::Int(int_fwd)) => int_fwd.set(integer),
            (ForwardStore::Bool(flag), ForwardSlot::Bool(bool_fwd)) => bool_fwd.set(flag),
            (ForwardStore::Object(value), ForwardSlot::Obj(obj_fwd)) => obj_fwd.set(value),
            (ForwardStore::Object(value), ForwardSlot::KboardObj(kbd_fwd)) => kbd_fwd.set(value),
            // A per-buffer slot's storage is the buffer's slot array; the
            // caller writes it.
            (ForwardStore::Object(_), ForwardSlot::BufferObj(_)) => {}
            // `store` built the value from this same header, so its variant
            // always matches; anything else is a caller that paired one
            // descriptor's store with another's commit.
            (
                ForwardStore::Int(_) | ForwardStore::Bool(_) | ForwardStore::Object(_),
                ForwardSlot::Int(_)
                | ForwardSlot::Bool(_)
                | ForwardSlot::Obj(_)
                | ForwardSlot::BufferObj(_)
                | ForwardSlot::KboardObj(_),
            ) => debug_assert!(false, "{store:?} cannot commit into a {:?} slot", self.ty),
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

static_assertions::assert_impl_all!(ForwardStoreError: Copy, Clone, std::fmt::Debug, Send, Sync);

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

static_assertions::assert_impl_all!(LispInteger: Copy, Clone, std::fmt::Debug);
static_assertions::assert_not_impl_any!(LispInteger: Send, Sync);

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

/// A descriptor-owned Lisp word with atomic publication.
///
/// The integer bits are shared; a loaded raw Value still belongs to the
/// descriptor's heap and must be used under that heap's mutator/root protocol.
/// Private construction and storage prevent a borrowed Value from aliasing
/// an atomic slot. All concurrent readers, including generated code and the
/// marker, must use atomic access. The JIT-read representation is pinned below.
#[repr(transparent)]
struct AtomicValue(AtomicUsize);

static_assertions::assert_impl_all!(AtomicValue: std::fmt::Debug, Send, Sync);
static_assertions::assert_not_impl_any!(AtomicValue: Copy, Clone);
const _: () = {
    assert!(std::mem::size_of::<AtomicValue>() == std::mem::size_of::<Value>());
    assert!(std::mem::align_of::<AtomicValue>() == std::mem::align_of::<Value>());
    assert!(std::mem::offset_of!(AtomicValue, 0) == 0);
};

impl AtomicValue {
    #[inline]
    fn new(value: Value) -> Self {
        Self(AtomicUsize::new(value.bits()))
    }

    #[inline(always)]
    fn load(&self) -> Value {
        Value::from_bits(self.0.load(Ordering::Acquire))
    }

    /// Called by the owning mutator: retain the old root before publishing
    /// its replacement. Atomicity alone does not admit an unrelated heap or
    /// an unregistered non-mutator writer to this descriptor's root protocol.
    #[inline(always)]
    fn store(&self, value: Value) {
        crate::tagged::gc::note_root_overwrite(self.load());
        self.0.store(value.bits(), Ordering::Release);
    }
}

impl std::fmt::Debug for AtomicValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Formatting an atomic descriptor must not traverse an unrooted heap
        // object on the thread formatting it. Report the encoded word only.
        f.debug_tuple("AtomicValue")
            .field(&format_args!("{:#x}", self.0.load(Ordering::Acquire)))
            .finish()
    }
}

/// `Lisp_Intfwd`: forward to an integer slot (`src/lisp.h:3124`).
///
/// GNU stores an `intmax_t`. Neomacs stores the Lisp integer in an atomic
/// word, read as a copy; a bignum is rooted through the owning obarray. The
/// private field and [`Self::set`]'s [`LispInteger`] argument keep a string
/// out of this slot just as GNU's `intmax_t` does.
#[repr(C)]
pub struct LispIntFwd {
    ty: LispFwdType,
    /// Always an integer inside `intmax_t` range -- see [`LispInteger`].
    /// Concurrent root scans and the mutator use the same atomic word.
    value: AtomicValue,
    /// The integer may be a heap-backed bignum. Atomic storage alone does
    /// not register a reader with its heap or keep that object alive.
    _mutator: PhantomData<*const ()>,
}

/// Byte offset of a [`LispIntFwd`]'s slot (a Lisp integer), for JIT code's
/// inline read and fixnum store (P1.4 Stage B).
pub(crate) const LISP_INT_FWD_VALUE_OFFSET: usize = std::mem::offset_of!(LispIntFwd, value);

static_assertions::assert_impl_all!(LispIntFwd: std::fmt::Debug);
static_assertions::assert_not_impl_any!(LispIntFwd: Copy, Clone, Send, Sync);

impl LispIntFwd {
    /// GNU `do_symval_forwarding`'s `Lisp_Fwd_Int` arm (`src/data.c:1341-1342`),
    /// which wraps the C slot back up with `make_int`.
    #[inline]
    pub fn get(&self) -> Value {
        self.value.load()
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

    /// The store half of GNU's `Lisp_Fwd_Int` arm. Takes a [`LispInteger`],
    /// so there is no spelling of this call that stores a non-integer.
    #[inline]
    pub fn set(&self, value: LispInteger) {
        self.value.store(value.value());
    }
}

/// `Lisp_Boolfwd`: forward to a native Boolean cell.
///
/// GNU stores a pointer to a C `bool`.  Each Neomacs context owns an
/// independently leaked descriptor instead, avoiding process-global state
/// between evaluators while retaining the same forwarded-value semantics.
#[repr(C)]
pub struct LispBoolFwd {
    ty: LispFwdType,
    value: AtomicBool,
}

/// Byte offset of a [`LispBoolFwd`]'s flag, for compiled code's inline read.
pub(crate) const LISP_BOOL_FWD_VALUE_OFFSET: usize = std::mem::offset_of!(LispBoolFwd, value);

/// What an obarray's `debug-on-next-call` cell pointer names before the
/// obarray has resolved the `DEFVAR_BOOL` (`Obarray::debug_on_next_call_fwd`):
/// it reads ARMED, so every reader takes its reference path, which resolves
/// the real descriptor. Read-only.
pub(crate) static DEBUG_ON_NEXT_CALL_UNRESOLVED: LispBoolFwd = LispBoolFwd {
    ty: LispFwdType::Bool,
    value: AtomicBool::new(true),
};

/// What the pointer names once the obarray found no `DEFVAR_BOOL` for
/// `debug-on-next-call` (a bare `Obarray::new()` harness): it reads
/// DISARMED, exactly what the missing cell has always meant. Read-only.
pub(crate) static DEBUG_ON_NEXT_CALL_ABSENT: LispBoolFwd = LispBoolFwd {
    ty: LispFwdType::Bool,
    value: AtomicBool::new(false),
};

static_assertions::assert_impl_all!(LispBoolFwd: std::fmt::Debug, Send, Sync);
static_assertions::assert_not_impl_any!(LispBoolFwd: Copy, Clone);

impl LispBoolFwd {
    #[inline]
    pub fn get(&self) -> bool {
        self.value.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn set(&self, value: bool) {
        debug_assert!(
            !self.is_debug_on_next_call_stand_in(),
            "a debug-on-next-call stand-in is read-only"
        );
        self.value.store(value, Ordering::Relaxed);
    }

    /// Whether this is one of the two `'static` stand-ins an obarray's
    /// `debug-on-next-call` pointer names before (or instead of) a real
    /// descriptor.
    #[inline]
    pub(crate) fn is_debug_on_next_call_stand_in(&self) -> bool {
        std::ptr::eq(self, &DEBUG_ON_NEXT_CALL_UNRESOLVED)
            || std::ptr::eq(self, &DEBUG_ON_NEXT_CALL_ABSENT)
    }
}

/// `Lisp_Objfwd`: forward to a `Value` global (`src/lisp.h:3479-3484`).
///
/// GNU's descriptor holds `&globals.f_Vfoo`, a slot inside `struct
/// emacs_globals` that `staticpro` roots.  Neomacs's holds the slot itself,
/// at a stable address used by symbol cells and generated code. Loads copy
/// the atomic word; the obarray's root scan roots it as `staticpro` roots
/// GNU's. The descriptor remains mutator-local: sharing the atomic bits is
/// not admission of its heap-backed Value to an unregistered reader.
#[repr(C)]
pub struct LispObjFwd {
    ty: LispFwdType,
    /// Concurrent root scans and the mutator use the same atomic word.
    value: AtomicValue,
    _mutator: PhantomData<*const ()>,
}

/// Byte offset of a [`LispObjFwd`]'s slot, for JIT code's inline read and
/// store (P1.4 Stage B).
pub(crate) const LISP_OBJ_FWD_VALUE_OFFSET: usize = std::mem::offset_of!(LispObjFwd, value);

static_assertions::assert_impl_all!(LispObjFwd: std::fmt::Debug);
static_assertions::assert_not_impl_any!(LispObjFwd: Copy, Clone, Send, Sync);

impl LispObjFwd {
    /// GNU `do_symval_forwarding`'s `Lisp_Fwd_Obj` arm (`src/data.c:1343-1344`).
    #[inline]
    pub fn get(&self) -> Value {
        self.value.load()
    }

    /// The store half of GNU's `Lisp_Fwd_Obj` arm (`src/data.c:1489-1516`).
    #[inline]
    pub fn set(&self, value: Value) {
        self.value.store(value);
    }
}

/// `Lisp_Buffer_Objfwd`: forward to a per-buffer slot. The `offset`
/// field indexes into `Buffer::slots: [Value; BUFFER_SLOT_COUNT]`,
/// playing the same role as GNU's `Lisp_Buffer_Objfwd::offset` (a byte
/// offset into `struct buffer`). Phase 8 wires this up.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct LispBufferObjFwd {
    ty: LispFwdType,
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

static_assertions::assert_impl_all!(LispBufferObjFwd: Copy, Clone, std::fmt::Debug);
static_assertions::assert_not_impl_any!(LispBufferObjFwd: Send, Sync);

impl LispBufferObjFwd {
    /// GNU `do_symval_forwarding`'s `Lisp_Fwd_Buffer_Obj` arm plus
    /// `PER_BUFFER_VALUE_P` (`src/data.c:1345-1350`, `buffer.h:1640`): the
    /// value this slot reads in the buffer whose slot array is SLOTS and whose
    /// conditional-local bits are LOCAL_FLAGS, falling back to DEFAULTS (the
    /// shared `buffer_defaults`) and then to the descriptor's own default.
    ///
    /// Always-local slots (`local_flags_idx < 0`) read the buffer's slot
    /// unconditionally; conditional ones only while the buffer's bit is set.
    /// No current buffer is SLOTS `None` (and no flag set).
    #[inline]
    pub(crate) fn value_in(
        &self,
        slots: Option<&[Value]>,
        local_flags: u64,
        defaults: Option<&[Value]>,
    ) -> Value {
        let off = self.offset as usize;
        if self.local_flags_idx >= 0 {
            // NeoMacs reuses `offset` as the local-flags bit index; both fit
            // in BUFFER_SLOT_COUNT.
            let bit_set = (local_flags >> (off as u32)) & 1 != 0;
            if bit_set
                && let Some(slots) = slots
                && off < slots.len()
            {
                return slots[off];
            }
            if let Some(defaults) = defaults
                && off < defaults.len()
            {
                return defaults[off];
            }
            return self.default;
        }
        match slots {
            Some(slots) if off < slots.len() => slots[off],
            _ => self.default,
        }
    }
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
    ty: LispFwdType,
    /// The same atomic storage and root protocol as [`LispObjFwd`].
    value: AtomicValue,
    _mutator: PhantomData<*const ()>,
}

/// Byte offset of a [`LispKboardObjFwd`]'s slot, for JIT code's inline read
/// and store (P1.4 Stage B).
pub(crate) const LISP_KBOARD_OBJ_FWD_VALUE_OFFSET: usize =
    std::mem::offset_of!(LispKboardObjFwd, value);

// Pin the physical layout generated code bakes. Replacing the word by a
// real atomic and adding zero-size thread markers must not move any slot.
const _: () = {
    use std::mem::{align_of, offset_of, size_of};
    assert!(size_of::<AtomicValue>() == size_of::<usize>());
    assert!(align_of::<AtomicValue>() == align_of::<AtomicUsize>());
    assert!(size_of::<AtomicBool>() == 1);
    assert!(align_of::<AtomicBool>() == 1);
    assert!(size_of::<PhantomData<*const ()>>() == 0);
    assert!(align_of::<PhantomData<*const ()>>() == 1);
    assert!(LISP_INT_FWD_VALUE_OFFSET == 8);
    assert!(LISP_OBJ_FWD_VALUE_OFFSET == 8);
    assert!(LISP_KBOARD_OBJ_FWD_VALUE_OFFSET == 8);
    assert!(LISP_BOOL_FWD_VALUE_OFFSET == 1);
    assert!(LISP_INT_FWD_VALUE_OFFSET % align_of::<AtomicValue>() == 0);
    assert!(LISP_OBJ_FWD_VALUE_OFFSET % align_of::<AtomicValue>() == 0);
    assert!(LISP_KBOARD_OBJ_FWD_VALUE_OFFSET % align_of::<AtomicValue>() == 0);
    assert!(align_of::<LispIntFwd>() == 8);
    assert!(align_of::<LispObjFwd>() == 8);
    assert!(align_of::<LispKboardObjFwd>() == 8);
    assert!(align_of::<LispBoolFwd>() == 1);
    assert!(size_of::<LispIntFwd>() == 16);
    assert!(size_of::<LispObjFwd>() == 16);
    assert!(size_of::<LispKboardObjFwd>() == 16);
    assert!(offset_of!(LispIntFwd, _mutator) == 16);
    assert!(offset_of!(LispObjFwd, _mutator) == 16);
    assert!(offset_of!(LispKboardObjFwd, _mutator) == 16);
    assert!(size_of::<LispBoolFwd>() == 2);
    // Immutable buffer metadata retains its original repr(C) offsets too.
    assert!(offset_of!(LispBufferObjFwd, offset) == 2);
    assert!(offset_of!(LispBufferObjFwd, local_flags_idx) == 4);
    assert!(offset_of!(LispBufferObjFwd, predicate) == 6);
    assert!(offset_of!(LispBufferObjFwd, default) == 8);
    assert!(size_of::<LispBufferObjFwd>() == 16);
    assert!(align_of::<LispBufferObjFwd>() == 8);
};

impl std::fmt::Debug for LispIntFwd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LispIntFwd")
            .field("value", &self.value)
            .finish()
    }
}

impl std::fmt::Debug for LispBoolFwd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LispBoolFwd")
            .field("value", &self.get())
            .finish()
    }
}

impl std::fmt::Debug for LispObjFwd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LispObjFwd")
            .field("value", &self.value)
            .finish()
    }
}

impl std::fmt::Debug for LispKboardObjFwd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LispKboardObjFwd")
            .field("value", &self.value)
            .finish()
    }
}

impl std::fmt::Debug for LispBufferObjFwd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LispBufferObjFwd")
            .field("offset", &self.offset)
            .field("local_flags_idx", &self.local_flags_idx)
            .field("default", &self.default)
            .finish_non_exhaustive()
    }
}

static_assertions::assert_impl_all!(LispKboardObjFwd: std::fmt::Debug);
static_assertions::assert_not_impl_any!(LispKboardObjFwd: Copy, Clone, Send, Sync);

impl LispKboardObjFwd {
    /// GNU `do_symval_forwarding`'s `Lisp_Fwd_Kboard_Obj` arm
    /// (`src/data.c:1352-1356`).
    #[inline]
    pub fn get(&self) -> Value {
        self.value.load()
    }

    /// GNU `store_symval_forwarding`'s `Lisp_Fwd_Kboard_Obj` arm
    /// (`src/data.c:1529-1536`), which checks nothing.
    #[inline]
    pub fn set(&self, value: Value) {
        self.value.store(value);
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
/// static C objects, and symbol cells and generated code retain the
/// descriptor's address. Unlike GNU's, this slot is the storage, so its
/// installer must also register it as a GC root; `Obarray::install_objfwd`
/// performs that registration.
pub fn alloc_objfwd(initial: Value) -> &'static LispObjFwd {
    Box::leak(Box::new(LispObjFwd {
        ty: LispFwdType::Obj,
        value: AtomicValue::new(initial),
        _mutator: PhantomData,
    }))
}

/// Allocate a process-lifetime keyboard-object forwarder, GNU's
/// `DEFVAR_KBOARD` slot.  See [`alloc_objfwd`].
pub fn alloc_kboard_objfwd(initial: Value) -> &'static LispKboardObjFwd {
    Box::leak(Box::new(LispKboardObjFwd {
        ty: LispFwdType::KboardObj,
        value: AtomicValue::new(initial),
        _mutator: PhantomData,
    }))
}

/// Allocate a process-lifetime integer forwarder, GNU's `DEFVAR_INT` slot.
///
/// Leaked for the same reason as [`alloc_boolfwd`]: GNU's descriptors are
/// static C objects, and symbol cells and generated code retain the
/// descriptor's stable address.
pub fn alloc_intfwd(initial: LispInteger) -> &'static LispIntFwd {
    Box::leak(Box::new(LispIntFwd {
        ty: LispFwdType::Int,
        value: AtomicValue::new(initial.value()),
        _mutator: PhantomData,
    }))
}

#[cfg(test)]
#[path = "tests/forward_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/forward_module_test.rs"]
mod gnu_parity_tests;

#[cfg(test)]
#[path = "tests/atomic_forward_test.rs"]
mod atomic_tests;
