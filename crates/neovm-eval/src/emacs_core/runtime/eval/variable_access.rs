//! Rust reads and writes of Lisp variables: GNU's `Vfoo` and
//! `BVAR (current_buffer, foo)` (Phase 0 step 2).
//!
//! GNU's C reads and writes a variable's cell, and while a thread runs that
//! cell holds the thread's own dynamic binding. Here an owner's bindings of a
//! projected variable live in its slab (`eval::owner_bindings`) and the
//! obarray holds only the canonical value, so every Rust site that stands in
//! for C goes through this module: the running owner's binding first,
//! canonical storage otherwise. The obarray's own accessors answer canonical
//! storage only and ask the caller to say why that is right
//! ([`CanonicalAccess`]).
//!
//! A Context with no owner bindings is DORMANT: no projected cell exists
//! that it could see (a test projection and admission install the owner's
//! slab first), so each read is the obarray's answer. A read whose canonical
//! answer is a value returns it untouched; only a void answer -- which every
//! projected cell gives -- tests for an owner. A write, a default read and a
//! `boundp` test for an owner first.
//!
//! Reads go through a [`VariableView`], a copyable pair of the obarray and
//! the owner's slab, so helpers that only read variables take a view instead
//! of the whole Context. Writes are `Context` methods.

use super::owner_bindings::{
    Channel, OwnerSlab, OwnerSlot, SlotState, canonical_bufferless_value, per_buffer_local,
};
use super::{Context, SymId, Value};
use neovm_value::forward::{CanonicalArm, ThreadProjectionDescriptor};
use neovm_value::intern::intern;
use neovm_value::symbol::{BufferlessValue, CanonicalAccess, CanonicalWrite, Obarray};

/// The variables one owner sees: canonical storage under its bindings.
///
/// It holds the owner's slot, not its slab: making a view reads nothing, and
/// a read reaches the slot only when canonical storage answers void.
#[derive(Clone, Copy)]
pub struct VariableView<'a> {
    obarray: &'a Obarray,
    owner: &'a OwnerSlot,
}

/// The owner slot of a view of canonical storage alone. Never written: a
/// slot is mutated only through `&mut`.
struct NoOwner(OwnerSlot);

// SAFETY: the slot is DORMANT for good, a `None` no thread can change.
unsafe impl Sync for NoOwner {}

static NO_OWNER: NoOwner = NoOwner(OwnerSlot::DORMANT);

static_assertions::assert_not_impl_any!(VariableView<'static>: Send, Sync);

/// The edit layer's variable-read surface (stage-3 V ruling): pure
/// `#[inline(always)]` delegation to the inherent methods, no semantic
/// change. Edit-owned callees name only this trait, never the view type.
impl<'a> neovm_edit::emacs_core::edit_host::EditVariableRead<'a> for VariableView<'a> {
    #[inline(always)]
    fn obarray(self) -> &'a Obarray {
        VariableView::obarray(self)
    }

    #[inline(always)]
    fn value(self, id: SymId) -> Option<Value> {
        VariableView::value(self, id)
    }

    #[inline(always)]
    fn value_by_name(self, name: &str) -> Option<Value> {
        VariableView::value_by_name(self, name)
    }

    #[inline(always)]
    fn value_in_buffer(self, buf: Option<&neovm_edit::buffer::Buffer>, id: SymId) -> Option<Value> {
        VariableView::value_in_buffer(self, buf, id)
    }

    #[inline(always)]
    fn value_or_nil(self, id: SymId) -> Value {
        VariableView::value_or_nil(self, id)
    }
}

/// The text layer's CCL raw-publication surface (stage-3b T2): the signed
/// unsafe row, separate from the read trait. The raw form is this module's
/// two-pointer [`RawVariableView`]; reconstruction copies the parts back and
/// is sound only while the source view's borrows outlive `'a`.
impl<'a> neovm_text::emacs_core::text_host::RawTextVariableView<'a> for VariableView<'a> {
    type Raw = RawVariableView;

    #[inline]
    fn into_raw(self) -> RawVariableView {
        VariableView::into_raw(self)
    }

    #[inline]
    unsafe fn view_from_raw(raw: RawVariableView) -> Self {
        // SAFETY: the caller keeps the source view's obarray and slab
        // borrows alive for 'a, on this thread (trait Safety contract).
        unsafe { raw.view() }
    }
}

impl std::fmt::Debug for VariableView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VariableView")
            .field("owner", &!self.owner.is_dormant())
            .finish_non_exhaustive()
    }
}

impl<'a> VariableView<'a> {
    /// The view of OBARRAY for the owner OWNER names.
    #[inline(always)]
    #[doc(hidden)]
    pub fn new(obarray: &'a Obarray, owner: &'a super::owner_bindings::OwnerSlot) -> Self {
        Self { obarray, owner }
    }

    /// The view of canonical storage alone, with no owner: bootstrap, dump
    /// and tests, which name why canonical storage is the answer.
    #[inline(always)]
    pub fn canonical(_access: CanonicalAccess, obarray: &'a Obarray) -> Self {
        Self {
            obarray,
            owner: &NO_OWNER.0,
        }
    }

    /// The view as raw parts, for a scope that publishes it to same-thread
    /// code without a borrow (CCL's program interpreter).
    #[inline]
    #[doc(hidden)]
    pub fn into_raw(self) -> RawVariableView {
        RawVariableView {
            obarray: self.obarray,
            owner: self.owner,
        }
    }

    /// A Context that can see a projected cell holds an owner slab; only a
    /// canonical view reads one without an owner.
    #[inline(always)]
    #[doc(hidden)]
    pub fn debug_assert_owner_free(self, id: SymId) {
        debug_assert!(
            std::ptr::eq(self.owner, &NO_OWNER.0) || projection_behind(self.obarray, id).is_none(),
            "a Context that sees a projected cell has an owner slab"
        );
        let _ = id;
    }

    /// The obarray, for what a view does not cover (functions, plists).
    #[inline(always)]
    pub fn obarray(self) -> &'a Obarray {
        self.obarray
    }

    /// GNU `Vfoo`: the variable's value without a buffer -- the default of a
    /// buffer-local variable, `None` for a per-buffer slot and when void. The
    /// owner-aware form of `Obarray::canonical_value`.
    #[inline(always)]
    pub fn value(self, id: SymId) -> Option<Value> {
        // The owner is asked only from the arms a thread projection takes
        // (a void forwarded projection, a void alias chain): a dormant read,
        // void or not, executes the canonical read alone.
        let owner = self.owner;
        self.obarray.value_with(id, move |obarray, id| {
            VariableView { obarray, owner }.owner_void_value(id)
        })
    }

    /// [`Self::value`] of a projected cell canonical storage answered void
    /// for: this owner's binding.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn owner_void_value(self, id: SymId) -> Option<Value> {
        let Some(slab) = self.owner.slab() else {
            self.debug_assert_owner_free(id);
            return None;
        };
        owner_value(self.obarray, slab, id)
    }

    /// [`Self::value`] by name: the canonical read by name, then the
    /// owner's only when it answers void.
    #[inline(always)]
    pub fn value_by_name(self, name: &str) -> Option<Value> {
        match self
            .obarray
            .canonical_value_by_name(CanonicalAccess::ROUTED, name)
        {
            Some(value) => Some(value),
            None => self.owner_value_by_name(name),
        }
    }

    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn owner_value_by_name(self, name: &str) -> Option<Value> {
        let id = intern(name);
        let Some(slab) = self.owner.slab() else {
            self.debug_assert_owner_free(id);
            return None;
        };
        owner_value(self.obarray, slab, id)
    }

    /// [`Self::value`], nil when void. The former `symbol_value_id_or_nil`
    /// shape -- the canonical read, then one inline owner-word test on a
    /// void answer -- keeps the register use of its hot callers (`get`'s
    /// `overriding-plist-environment` read) where [`Self::value`]'s routed
    /// walk measured +6 Ir per command-execute.
    #[inline(always)]
    pub fn value_or_nil(self, id: SymId) -> Value {
        match self.obarray.canonical_value(CanonicalAccess::ROUTED, id) {
            Some(value) => value,
            None => match self.owner.slab() {
                None => Value::NIL,
                Some(slab) => owner_value(self.obarray, slab, id).unwrap_or(Value::NIL),
            },
        }
    }

    /// GNU `BVAR (BUF, foo)` / `Vfoo` with BUF current: BUF's slot or local
    /// binding, else the default. The owner-aware form of
    /// `Obarray::canonical_value_in_buffer_id`.
    pub fn value_in_buffer(
        self,
        buf: Option<&neovm_edit::buffer::Buffer>,
        id: SymId,
    ) -> Option<Value> {
        self.read_in_buffer(buf, id)
    }

    /// [`Self::value_in_buffer`]'s body: the former `value_in_buffer_id`
    /// read, asking the owner only from the arms a thread projection takes
    /// (as [`Self::value`] does). A slot or local binding of BUF is
    /// canonical storage, so a dormant read executes the canonical read
    /// alone, with no owner test on its answer.
    #[inline(always)]
    #[doc(hidden)]
    pub fn read_in_buffer(
        self,
        buf: Option<&neovm_edit::buffer::Buffer>,
        id: SymId,
    ) -> Option<Value> {
        let owner = self.owner;
        self.obarray
            .value_in_buffer_with(buf, id, move |obarray, id| {
                VariableView { obarray, owner }.owner_void_value_in_buffer(buf, id)
            })
    }

    /// [`Self::value_in_buffer`] of a projected cell canonical storage
    /// answered void for: this owner's binding in BUF.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn owner_void_value_in_buffer(
        self,
        buf: Option<&neovm_edit::buffer::Buffer>,
        id: SymId,
    ) -> Option<Value> {
        let Some(slab) = self.owner.slab() else {
            self.debug_assert_owner_free(id);
            return None;
        };
        owner_value_in_buffer(self.obarray, slab, buf, id)
    }

    /// [`Self::value_in_buffer`] by name.
    #[inline]
    pub fn value_in_buffer_by_name(
        self,
        buf: Option<&neovm_edit::buffer::Buffer>,
        name: &str,
    ) -> Option<Value> {
        self.value_in_buffer(buf, intern(name))
    }

    /// GNU `default_value` without a buffer (`src/data.c:1962-2004`),
    /// including a per-buffer forwarder's default. The owner-aware form of
    /// `Obarray::canonical_default_value`.
    #[inline]
    pub fn default_value(self, id: SymId) -> Option<Value> {
        // The owner is asked only from the arm a thread projection takes.
        self.obarray
            .default_value_with(id, |projection| self.owner_default_value(projection))
    }

    /// [`Self::default_value`] of a thread projection: this owner's default
    /// binding when it has one, the canonical default otherwise.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn owner_default_value(self, projection: &ThreadProjectionDescriptor) -> Option<Value> {
        match self.owner_default_slot(projection) {
            SlotState::Bound(binding) => binding.bound(),
            SlotState::NoBinding => self.obarray.projected_default_value(projection),
        }
    }

    /// This owner's default-channel slot of PROJECTION (none for a view
    /// without an owner).
    #[inline]
    #[doc(hidden)]
    pub fn owner_default_slot(self, projection: &ThreadProjectionDescriptor) -> SlotState {
        match self.owner.slab() {
            Some(slab) => slab.slot(projection.index(), Channel::Default),
            None => {
                debug_assert!(
                    std::ptr::eq(self.owner, &NO_OWNER.0),
                    "a Context that sees a projected cell has an owner slab"
                );
                SlotState::NoBinding
            }
        }
    }

    /// GNU `boundp` without a buffer. The owner-aware form of
    /// `Obarray::canonical_boundp_id`.
    #[inline]
    pub fn boundp(self, id: SymId) -> bool {
        // The owner is asked only from the arm a host-projected forwarded
        // cell takes, and only for a thread projection there.
        self.obarray
            .boundp_id_with(id, |fwd| self.projected_boundp(fwd))
    }

    /// [`Self::boundp`] of a host-projected forwarded cell.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn projected_boundp(self, fwd: neovm_value::forward::FwdRef) -> bool {
        let Some(projection) = fwd.thread_projection() else {
            return true;
        };
        match self.owner_default_slot(projection) {
            SlotState::Bound(binding) => binding.bound().is_some(),
            SlotState::NoBinding => self.obarray.projected_default_value(projection).is_some(),
        }
    }

    /// [`Self::boundp`] by name.
    #[inline]
    pub fn boundp_by_name(self, name: &str) -> bool {
        self.boundp(intern(name))
    }

    /// The buffer-less read with GNU's disagreement named
    /// (`Obarray::canonical_value_without_buffer_id`), owner-aware.
    pub fn value_without_buffer(self, id: SymId) -> BufferlessValue {
        let canonical = self
            .obarray
            .canonical_value_without_buffer_id(CanonicalAccess::ROUTED, id);
        match canonical {
            BufferlessValue::Void => match self.owner.slab() {
                Some(slab) => match owner_value(self.obarray, slab, id) {
                    Some(value) => BufferlessValue::Global(value),
                    None => BufferlessValue::Void,
                },
                None => {
                    self.debug_assert_owner_free(id);
                    BufferlessValue::Void
                }
            },
            canonical => canonical,
        }
    }

    /// [`Self::value_without_buffer`] by name.
    #[inline]
    pub fn value_without_buffer_by_name(self, name: &str) -> BufferlessValue {
        self.value_without_buffer(intern(name))
    }
}

/// The variables one owner sees, for writing: the obarray and the owner's
/// slot, borrowed mutably, for helpers that hold the obarray's manager
/// rather than the Context. No owner is canonical storage alone.
pub struct VariableStore<'a> {
    obarray: &'a mut Obarray,
    owner: Option<&'a mut OwnerSlot>,
}

static_assertions::assert_not_impl_any!(VariableStore<'static>: Send, Sync);

impl std::fmt::Debug for VariableStore<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VariableStore")
            .field(
                "owner",
                &self.owner.as_ref().is_some_and(|owner| !owner.is_dormant()),
            )
            .finish_non_exhaustive()
    }
}

impl<'a> VariableStore<'a> {
    /// The store of OBARRAY for the owner whose slot is OWNER.
    #[inline(always)]
    #[doc(hidden)]
    pub fn new(obarray: &'a mut Obarray, owner: Option<&'a mut OwnerSlot>) -> Self {
        Self { obarray, owner }
    }

    /// The obarray, for what a store does not cover.
    #[inline(always)]
    pub fn obarray(&mut self) -> &mut Obarray {
        self.obarray
    }

    /// The read view of the same variables.
    #[inline(always)]
    pub fn view(&self) -> VariableView<'_> {
        VariableView {
            obarray: self.obarray,
            owner: self.owner.as_deref().unwrap_or(&NO_OWNER.0),
        }
    }

    /// GNU `Vfoo = VALUE` from C (see [`Context::store_variable`]).
    #[inline]
    pub fn store(&mut self, id: SymId, value: Value) {
        if self.obarray.set_canonical_value_routed(id, value) == CanonicalWrite::Projected {
            self.store_projected(id, value);
        }
    }

    /// [`Self::store`] by name (interning it).
    #[inline]
    pub fn store_by_name(&mut self, name: &str, value: Value) {
        self.store(intern(name), value);
    }

    /// [`Self::store`] of a projected cell: the owner's write.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn store_projected(&mut self, id: SymId, value: Value) {
        let slab = self.owner.as_deref_mut().and_then(OwnerSlot::slab_mut);
        debug_assert!(
            slab.is_some(),
            "a store that sees a projected cell has an owner slab"
        );
        if let Some(slab) = slab {
            owner_store(self.obarray, slab, id, value);
        }
    }
}

/// GNU `Vfoo = VALUE` for an owner: its binding of the default when it has
/// one, canonical storage otherwise.
#[cold]
#[inline(never)]
fn owner_store(obarray: &mut Obarray, slab: &mut OwnerSlab, id: SymId, value: Value) {
    let Some((base, projection)) = projection_behind(obarray, id) else {
        obarray.set_canonical_value(CanonicalAccess::ROUTED, id, value);
        return;
    };
    let index = projection.index();
    if slab.slot(index, Channel::Default).value().is_some() {
        slab.set_slot(
            index,
            Channel::Default,
            SlotState::Bound(super::owner_bindings::SlotValue::new(value)),
        );
        return;
    }
    obarray.with_revealed_projection(base, |obarray| {
        obarray.set_canonical_value(CanonicalAccess::STORAGE, base, value);
    });
}

/// A [`VariableView`] as raw parts. Rebuilding the view is `unsafe`: the
/// scope that made it must still hold the borrow it was made from.
#[derive(Clone, Copy, Debug)]
#[doc(hidden)]
pub struct RawVariableView {
    obarray: *const Obarray,
    owner: *const OwnerSlot,
}

impl RawVariableView {
    /// The view these parts came from.
    ///
    /// # Safety
    /// The [`VariableView`] these parts came from must still be valid: its
    /// obarray and slab borrows outlive `'a`, on this thread.
    #[inline]
    #[doc(hidden)]
    pub unsafe fn view<'a>(self) -> VariableView<'a> {
        // SAFETY: the caller keeps the original borrows alive for `'a`.
        unsafe {
            VariableView {
                obarray: &*self.obarray,
                owner: &*self.owner,
            }
        }
    }
}

/// The projection behind ID, through its aliases.
fn projection_behind(
    obarray: &Obarray,
    id: SymId,
) -> Option<(SymId, &'static ThreadProjectionDescriptor)> {
    let base = obarray.indirect_variable_id(id)?;
    Some((base, obarray.thread_projection_of(base)?))
}

/// [`VariableView::value`] once canonical storage answered void.
#[cold]
#[inline(never)]
fn owner_value(obarray: &Obarray, slab: &OwnerSlab, id: SymId) -> Option<Value> {
    let (_, projection) = projection_behind(obarray, id)?;
    match slab.slot(projection.index(), Channel::Default) {
        SlotState::Bound(binding) => binding.bound(),
        SlotState::NoBinding => canonical_bufferless_value(projection),
    }
}

/// [`VariableView::value_in_buffer`] once canonical storage answered void:
/// BUF's local channel when BUF has the local, the default otherwise, the
/// owner's binding first on either.
#[cold]
#[inline(never)]
fn owner_value_in_buffer(
    obarray: &Obarray,
    slab: &OwnerSlab,
    buf: Option<&neovm_edit::buffer::Buffer>,
    id: SymId,
) -> Option<Value> {
    let (base, projection) = projection_behind(obarray, id)?;
    let Some(buffer) = buf else {
        return owner_value(obarray, slab, id);
    };
    let channel = match projection.arm() {
        CanonicalArm::Plain => Channel::Default,
        CanonicalArm::Localized(_) => match buffer.local_variable_binding_cell(base) {
            Some(_) => Channel::BufferLocal(buffer.id()),
            None => Channel::Default,
        },
        // A per-buffer slot without the local flag holds the live default,
        // which an owner's default binding shadows, as GNU's let-default
        // writes every buffer without the flag.
        CanonicalArm::Forwarded(builtin) => match builtin.descriptor().as_buffer_obj_fwd() {
            Some(buf_fwd) if per_buffer_local(buffer, buf_fwd) => Channel::BufferLocal(buffer.id()),
            _ => Channel::Default,
        },
    };
    if let SlotState::Bound(binding) = slab.slot(projection.index(), channel) {
        return binding.bound();
    }
    match projection.arm() {
        CanonicalArm::Localized(blv) if channel != Channel::Default => {
            let value = obarray.read_projected_localized(base, blv, Some(buffer));
            (!value.is_unbound()).then_some(value)
        }
        CanonicalArm::Forwarded(builtin) => match builtin.descriptor().as_buffer_obj_fwd() {
            Some(buf_fwd) => buf_fwd
                .buffer_slot()
                .map(|slot| buffer.slots()[slot.index()]),
            None => builtin.descriptor().load(),
        },
        _ => canonical_bufferless_value(projection),
    }
}

impl Context {
    /// The variables this owner sees.
    #[inline(always)]
    pub fn variables(&self) -> VariableView<'_> {
        VariableView::new(&self.obarray, &self.owner)
    }

    /// The variable's DEFAULT-ONLY value: [`VariableView::value`] of this
    /// owner's view, with no buffer. For a buffer-local variable that is the
    /// default (owner) binding, never the current buffer's binding; for a
    /// plain global it is the obarray cell (which a dynamic `let` writes); for
    /// a forwarded variable it is the forwarder's bufferless load.
    ///
    /// GNU spells these reads `Fdefault_value` (src/data.c) or
    /// `BVAR (&buffer_defaults, slot)` (src/buffer.c). A GNU C read of a
    /// `DEFVAR_*` global, or a Lisp `symbol-value`, is the VISIBLE read and
    /// must use [`Self::builtin_var_value`] /
    /// [`Self::builtin_var_value_by_name`] / [`Self::builtin_var_value_cached`]
    /// (or [`Self::special_variable_value_by_id`]) instead. The callers of
    /// this accessor are counted and frozen by
    /// `emacs_core/tests/architecture/default_value_only_ratchet_test.rs`.
    #[inline(always)]
    pub fn default_value_only(&self, id: SymId) -> Option<Value> {
        self.variables().value(id)
    }

    /// [`Self::default_value_only`] starting from the variable's name. The
    /// name is interned per call; hot paths pre-intern and use the id form.
    #[inline(always)]
    pub fn default_value_only_by_name(&self, name: &str) -> Option<Value> {
        self.variables().value_by_name(name)
    }

    /// [`VariableView::value_or_nil`] of this owner's view.
    #[inline(always)]
    pub fn variable_value_or_nil(&self, id: SymId) -> Value {
        self.variables().value_or_nil(id)
    }

    /// [`VariableView::value_in_buffer`] of this owner's view, called with
    /// the Context alone: the former `Obarray::value_in_buffer_id` call
    /// shape, one pointer for the whole view, where a two-pointer view kept
    /// live across the call cost a register (elb-scroll).
    #[inline(never)]
    pub fn variable_value_in_buffer(
        &self,
        buf: Option<&neovm_edit::buffer::Buffer>,
        id: SymId,
    ) -> Option<Value> {
        self.variable_value_in_buffer_inline(buf, id)
    }

    /// [`Self::variable_value_in_buffer`]'s body, for a caller that reads
    /// several variables per character: a slot of BUF is a leaf -- no
    /// frame, no owner -- and everything else is one out-of-line call.
    #[inline(always)]
    #[doc(hidden)]
    pub fn variable_value_in_buffer_inline(
        &self,
        buf: Option<&neovm_edit::buffer::Buffer>,
        id: SymId,
    ) -> Option<Value> {
        if let Some(value) = buf.and_then(|buf| Obarray::buffer_slot_value(buf, id)) {
            return Some(value);
        }
        self.unslotted_variable_value(buf, id)
    }

    /// [`Self::variable_value_in_buffer`] of an ID that is not one of BUF's
    /// slots: BUF's local binding, else the default, asking the owner only
    /// from the arms a thread projection takes.
    #[inline(never)]
    #[doc(hidden)]
    pub fn unslotted_variable_value(
        &self,
        buf: Option<&neovm_edit::buffer::Buffer>,
        id: SymId,
    ) -> Option<Value> {
        let owner = &self.owner;
        self.obarray
            .unslotted_value_with(buf, id, move |obarray, id| {
                VariableView::new(obarray, owner).owner_void_value_in_buffer(buf, id)
            })
    }

    /// [`VariableView::value_in_buffer`] in the current buffer.
    #[inline]
    pub fn variable_value_in_current_buffer(&self, id: SymId) -> Option<Value> {
        self.variables()
            .value_in_buffer(self.edit.buffers.current_buffer(), id)
    }

    /// GNU `Vfoo = VALUE` from C: the running owner's binding of the
    /// variable's default when it has one, the canonical default otherwise.
    /// No watchers, no type rule: the storage the obarray's
    /// `set_canonical_value` writes.
    ///
    /// A dormant write executes the canonical write alone: only a thread
    /// projection, from the per-buffer arm its header already takes, reaches
    /// the owner.
    #[inline(never)]
    pub fn store_variable(&mut self, id: SymId, value: Value) {
        if self.obarray.set_canonical_value_routed(id, value) == CanonicalWrite::Projected {
            self.store_owner_variable(id, value);
        }
    }

    /// [`Self::store_variable`] by name (interning it).
    #[inline(never)]
    pub fn store_variable_by_name(&mut self, name: &str, value: Value) {
        let id = intern(name);
        if self.obarray.set_canonical_value_routed(id, value) == CanonicalWrite::Projected {
            self.store_owner_variable(id, value);
        }
    }

    /// Make the variable void the way [`Self::store_variable`] writes it.
    #[inline(never)]
    pub fn makunbound_variable(&mut self, id: SymId) {
        if self.obarray.makunbound_canonical_routed(id) == CanonicalWrite::Projected {
            self.makunbound_owner_variable(id);
        }
    }
}

use neovm_value::error::{EvalResult, LispCondition, expect_args, signal};
use neovm_value::value::ValueKind;

#[doc(hidden)]
pub fn builtin_buffer_local_value(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    neovm_value::error::expect_args("buffer-local-value", &args, 2)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_buffer_local_value_2(eval, arg(0), arg(1))
}
/// `buffer-local-value` as registered: fixed arity 2, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a2` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
#[doc(hidden)]
pub fn builtin_buffer_local_value_2(
    eval: &mut Context,
    variable: Value,
    buffer: Value,
) -> EvalResult {
    buffer_local_value_in(eval, variable, buffer)
}

/// `buffer-local-value`'s whole body: it only READS the evaluator (the
/// obarray, the buffer table), so it takes a shared borrow, which is what
/// lets the same body serve as the builtin's leaf (`builtins::leaves`).
#[doc(hidden)]
pub fn buffer_local_value_in(eval: &Context, variable: Value, buffer: Value) -> EvalResult {
    let args: [Value; 2] = [variable, buffer];
    use neovm_value::intern::{intern, resolve_sym};
    use neovm_value::symbol::SymbolRedirect;

    expect_args("buffer-local-value", &args, 2)?;
    let original_arg = args[0];
    let symbol = match args[0].kind() {
        ValueKind::Symbol(id) => id,
        ValueKind::Nil => intern("nil"),
        ValueKind::T => intern("t"),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[0]],
            ));
        }
    };
    let resolved_id = crate::emacs_core::builtins::symbols::resolve_variable_alias_id_in_obarray(
        eval.obarray(),
        symbol,
    )?;
    let id = neovm_edit::emacs_core::buffer::expect_buffer_id(&args[1])?;
    let buf = eval
        .edit
        .buffers
        .get_any(id)
        .ok_or_else(|| signal("error", vec![Value::string("No such buffer")]))?;

    // GNU's local cell holds the running thread's binding of it.
    if !eval.owner.is_dormant()
        && let Some(binding) = eval.owner_local_binding(resolved_id, buf.id())
    {
        return binding
            .bound()
            .ok_or_else(|| signal(LispCondition::VoidVariable, vec![original_arg]));
    }

    // Phase 10E: route LOCALIZED reads through the BLV machinery
    // (immutable walker — buffer-local-value never swaps the cache).
    // Mirrors GNU `Fbuffer_local_value` SYMBOL_LOCALIZED arm at
    // `data.c:1696-1740` which uses `blv_value` (returning the
    // already-loaded valcell.cdr if `where == buf`, else walks
    // `BVAR(buf, local_var_alist)`), then signals void-variable if
    // the result is `Qunbound`.
    if let Some(sym_slot) = eval.obarray().get_by_id(resolved_id)
        && sym_slot.redirect() == SymbolRedirect::Localized
    {
        let target_buf = Value::make_buffer(buf.id());
        if let Some(value) =
            eval.obarray()
                .read_localized(resolved_id, target_buf, buf.local_var_alist_value())
        {
            if value.is_unbound() {
                return Err(signal(LispCondition::VoidVariable, vec![original_arg]));
            }
            return Ok(value);
        }
    }

    match buf.get_buffer_local_binding_by_sym_id(resolved_id) {
        Some(binding) => binding
            .as_value()
            .ok_or_else(|| signal(LispCondition::VoidVariable, vec![original_arg])),
        None if let Some(info) =
            neovm_edit::buffer::buffer::lookup_buffer_slot_by_sym_id(resolved_id) =>
        {
            Ok(buf.slots()[info.offset.index()])
        }
        None if resolved_id == intern("nil") => Ok(Value::NIL),
        None if resolved_id == intern("t") => Ok(Value::T),
        None if resolve_sym(resolved_id).starts_with(':') => Ok(Value::from_sym_id(resolved_id)),
        None => match eval
            .obarray()
            .canonical_find_value(neovm_value::symbol::CanonicalAccess::STORAGE, resolved_id)
        {
            // Only a thread projection's per-buffer header reads UNBOUND
            // here: the variable it projects answers its default.
            Some(value) if value.is_unbound() => {
                projected_buffer_local_default(eval, resolved_id, original_arg)
            }
            Some(value) => Ok(value),
            None => Err(signal(LispCondition::VoidVariable, vec![original_arg])),
        },
    }
}

/// `buffer-local-value` of a thread-projected variable without a binding in
/// the buffer: the default of the variable it projects.
#[cold]
#[inline(never)]
fn projected_buffer_local_default(
    eval: &Context,
    resolved: neovm_value::intern::SymId,
    original_arg: Value,
) -> EvalResult {
    let obarray = eval.obarray();
    // The default this owner binds, if it binds one (GNU reads the running
    // thread's default cell).
    let value = match eval.owner_default_binding(resolved) {
        Some(binding) => binding.bound(),
        None => obarray
            .thread_projection_of(resolved)
            .and_then(|projection| obarray.projected_default_value(projection)),
    };
    value.ok_or_else(|| signal(LispCondition::VoidVariable, vec![original_arg]))
}

/// The text layer's variable-write surface (stage-3b T6.2/T6.3): pure
/// delegation to the inherent store. Core constructs every `VariableStore`
/// (including the bootstrap one), so text never names the type or the owner
/// slot; `store` keeps today's canonical-routed + projected-store semantics.
impl<'a> neovm_text::emacs_core::text_host::TextVariableStore for VariableStore<'a> {
    type View<'b>
        = VariableView<'b>
    where
        Self: 'b;

    #[inline(always)]
    fn obarray(&mut self) -> &mut Obarray {
        VariableStore::obarray(self)
    }

    #[inline(always)]
    fn store(&mut self, id: SymId, value: Value) {
        VariableStore::store(self, id, value)
    }

    #[inline(always)]
    fn view(&self) -> Self::View<'_> {
        VariableStore::view(self)
    }
}

#[cfg(test)]
#[path = "tests/variable_access_test.rs"]
mod tests;
