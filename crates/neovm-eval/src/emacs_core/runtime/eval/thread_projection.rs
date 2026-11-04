//! Routing a thread-projected variable (Phase 0 step 2).
//!
//! Every hot path refuses a projected cell (see `forward::thread_projection`);
//! the slow paths it then reaches recognise it inside their existing
//! per-buffer handling -- on a branch a real per-buffer forwarder never
//! takes, because its slot index is valid -- and come here:
//! - a dynamic binding is the binding owner's (`eval::owner_bindings`);
//! - reads and assignments take the owner's binding first, and the
//!   variable's canonical storage otherwise;
//! - canonical reads with only `&self` compute the logical value from the
//!   canonical storage ([`Context::projected_value`]); every other canonical
//!   operation REVEALS the cell (gives it back its logical arm), runs the
//!   unchanged operation on it, and conceals it again
//!   ([`Context::with_revealed_projection`]). No Lisp runs while a cell is
//!   revealed.
//!
//! Each entry is cold and out of line.

use super::{Context, EvalResult, Flow, LispCondition, SymbolValueLookup, signal};
use neovm_value::forward::{CanonicalArm, LispBufferObjFwd, ThreadProjectionDescriptor};
use neovm_value::intern::SymId;
use neovm_value::symbol::SetInternalBind;
use neovm_value::value::Value;

impl Context {
    /// GNU `find_symbol_value` of the variable PROJECTION wraps, for symbol
    /// ID in the current buffer.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn projected_value(
        &self,
        id: SymId,
        projection: &ThreadProjectionDescriptor,
    ) -> SymbolValueLookup {
        if let Some(slab) = self.owner.slab()
            && let Some(binding) = slab
                .slot(
                    projection.index(),
                    super::owner_bindings::access_channel(&self.edit.buffers, id, projection),
                )
                .value()
        {
            return match binding.bound() {
                Some(value) => SymbolValueLookup::Bound(value),
                None => SymbolValueLookup::Unbound,
            };
        }
        let value = match projection.arm() {
            CanonicalArm::Plain => projection.plain_value(),
            CanonicalArm::Localized(blv) => {
                self.obarray
                    .read_projected_localized(id, blv, self.edit.buffers.current_buffer())
            }
            CanonicalArm::Forwarded(builtin) => {
                let fwd = builtin.descriptor();
                match fwd.load() {
                    Some(value) => value,
                    None => match fwd.as_buffer_obj_fwd() {
                        Some(buf_fwd) => self.builtin_per_buffer_value(buf_fwd),
                        None => Value::UNBOUND,
                    },
                }
            }
        };
        if value.is_unbound() {
            SymbolValueLookup::Unbound
        } else {
            SymbolValueLookup::Bound(value)
        }
    }

    /// `Bvarref`'s void branch: NAME (resolving to RESOLVED) read UNBOUND.
    /// A projected variable answers its canonical value; anything else, and
    /// a void projected variable, signals `void-variable`.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn projected_value_or_void(&self, name: SymId, resolved: SymId) -> EvalResult {
        if let Some(projection) = self.obarray.thread_projection_of(resolved)
            && let SymbolValueLookup::Bound(value) = self.projected_value(resolved, projection)
        {
            return Ok(value);
        }
        Err(signal(
            LispCondition::VoidVariable,
            vec![Value::from_sym_id(name)],
        ))
    }

    /// The current value of a real per-buffer forwarder.
    #[doc(hidden)]
    pub fn builtin_per_buffer_value(&self, buf_fwd: &LispBufferObjFwd) -> Value {
        match buf_fwd.buffer_slot() {
            Some(slot) => self.per_buffer_value(buf_fwd, slot),
            None => Value::UNBOUND,
        }
    }

    /// Run OP with ID's projected cell revealed as the variable it wraps,
    /// then conceal it again over whatever logical arm OP left. ID must be
    /// projected; OP sees exactly the unprojected variable.
    #[inline]
    #[doc(hidden)]
    pub fn with_revealed_projection<R>(&mut self, id: SymId, op: impl FnOnce(&mut Self) -> R) -> R {
        let revealed = self.reveal_projection(id);
        let result = op(self);
        if let Some(revealed) = revealed {
            self.conceal_projection(revealed);
        }
        result
    }

    /// [`Self::with_revealed_projection`] for several variables at once;
    /// IDS that are not projected are skipped.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn with_revealed_projections<R>(
        &mut self,
        ids: &[SymId],
        op: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let revealed: Vec<_> = ids
            .iter()
            .filter_map(|id| self.reveal_projection(*id))
            .collect();
        let result = op(self);
        for revealed in revealed.into_iter().rev() {
            self.conceal_projection(revealed);
        }
        result
    }

    /// The projected variables BUFFER holds a `Localized` local of: what a
    /// reset of BUFFER's locals must see unprojected, so their buffer-local
    /// caches reset with them.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn projected_locals_of(&self, buffer: neovm_edit::buffer::BufferId) -> Vec<SymId> {
        let Some(buffer) = self.edit.buffers.get(buffer) else {
            return Vec::new();
        };
        let mut ids = Vec::new();
        let mut tail = buffer.local_var_alist_value();
        while tail.is_cons() {
            let entry = tail.cons_car();
            if entry.is_cons()
                && let Some(id) = entry.cons_car().as_symbol_id(self.symbol_mode())
                && self.obarray.thread_projection_of(id).is_some()
            {
                ids.push(id);
            }
            tail = tail.cons_cdr();
        }
        ids
    }

    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn reveal_projection(
        &mut self,
        id: SymId,
    ) -> Option<neovm_value::symbol::RevealedProjection> {
        self.obarray.reveal_projection(id)
    }

    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn conceal_projection(&mut self, revealed: neovm_value::symbol::RevealedProjection) {
        // An alias retires the projection; the variable it names is
        // projected on its own (admission projects bases).
        let _ = self.obarray.conceal_projection(revealed);
    }

    /// `specbind` of a projected variable: a binding of this owner
    /// ([`Context::specbind_owner`]); canonical storage does not change.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn specbind_projected(&mut self, resolved: SymId, value: Value) -> Result<(), Flow> {
        // A bind never touches the shared VOID descriptor: the cell gets
        // its own first.
        match self.obarray.owned_thread_projection(resolved) {
            Some(projection) => self.specbind_owner(resolved, projection, value),
            // Only a projection's header reaches here.
            None => self.specbind_uncached(resolved, value),
        }
    }

    /// `set_internal` of a projected variable once its watchers have run
    /// (`src/data.c:1678-1831`): this owner's binding takes the store when it
    /// binds the channel the store meets, canonical storage otherwise.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn set_projected_internal(
        &mut self,
        sym_id: SymId,
        resolved: SymId,
        value: Value,
        bindflag: SetInternalBind,
    ) -> Result<(), Flow> {
        if let Some(projection) = self.obarray.thread_projection_of(resolved)
            && let Some(slab) = self.owner.slab()
            && let Some(channel) = super::owner_bindings::owner_store_channel(
                slab,
                &self.edit.buffers,
                resolved,
                projection,
                bindflag,
            )
        {
            let site = if bindflag == SetInternalBind::Set {
                super::ForwardStoreSite::Set
            } else {
                super::ForwardStoreSite::Bind
            };
            let stored = super::check_forwarded_store_at(
                &self.obarray,
                &self.edit.buffers,
                &self.specpdl,
                resolved,
                value,
                site,
            )?
            .value();
            if let Some(slab) = self.owner.slab_mut() {
                slab.set_slot(
                    projection.index(),
                    channel,
                    super::owner_bindings::SlotState::Bound(super::owner_bindings::SlotValue::new(
                        stored,
                    )),
                );
            }
            let visible = if stored.is_unbound() {
                Value::NIL
            } else {
                stored
            };
            self.publish_runtime_binding_write_by_resolved_id(resolved, visible);
            self.sync_user_test_gc_binding_by_id(resolved);
            return Ok(());
        }
        self.with_revealed_projection(resolved, |ctx| {
            ctx.set_internal_after_watchers(sym_id, value, bindflag)
        })
    }

    /// Panic-recovery restore of a projected variable's binding.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn discard_restore_projected(&mut self, resolved: SymId, saved: Option<Value>) {
        self.with_revealed_projection(resolved, |ctx| {
            ctx.discard_restore_default_binding(resolved, saved)
        });
    }

    /// `variable_watcher_where_for_set_by_id` of a projected variable: the
    /// buffer a `set` of the variable it projects would land in.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn projected_watcher_where(
        &self,
        id: SymId,
        projection: &ThreadProjectionDescriptor,
        current: neovm_edit::buffer::BufferId,
    ) -> Value {
        let in_buffer = match projection.arm() {
            CanonicalArm::Plain => false,
            CanonicalArm::Localized(blv) => {
                // SAFETY: a projection's localized arm names a live record
                // of the obarray's BLV pool; one flag is read.
                let local_if_set = unsafe { (*blv.blv().as_ptr()).local_if_set };
                local_if_set
                    || self
                        .edit
                        .buffers
                        .get(current)
                        .is_some_and(|buf| buf.has_buffer_local_by_sym_id(id))
            }
            CanonicalArm::Forwarded(builtin) => builtin.descriptor().as_buffer_obj_fwd().is_some(),
        };
        if in_buffer {
            Value::make_buffer(current)
        } else {
            Value::NIL
        }
    }
}

/// [`store_runtime_binding`] of a thread-projected variable: GNU
/// `set_internal`'s `SET` (`src/data.c:1678-1831`). OWNER's binding takes
/// the store when it binds the channel the store meets; otherwise the same
/// store runs on the cell revealed as the variable it projects.
#[cold]
#[inline(never)]
pub(super) fn store_projected_runtime_binding(
    obarray: &mut neovm_value::symbol::Obarray,
    buffers: &mut neovm_edit::buffer::BufferManager,
    mut owner: Option<&mut super::owner_bindings::OwnerSlab>,
    specpdl: &[super::SpecBinding],
    sym_id: SymId,
    checked: super::ForwardChecked,
) -> Option<neovm_edit::buffer::BufferId> {
    let mut shadowed = false;
    if let Some(slab) = owner.as_deref_mut()
        && let Some(projection) = obarray.thread_projection_of(sym_id)
    {
        if let Some(channel) = super::owner_bindings::owner_store_channel(
            slab,
            buffers,
            sym_id,
            projection,
            neovm_value::symbol::SetInternalBind::Set,
        ) {
            slab.set_slot(
                projection.index(),
                channel,
                super::owner_bindings::SlotState::Bound(super::owner_bindings::SlotValue::new(
                    checked.value(),
                )),
            );
            return channel.locus();
        }
        shadowed = slab.shadows_buffer_binding(sym_id, buffers.current_buffer_id());
    }
    // A default binding of this owner made in this buffer that
    // `backtrace-eval` has set aside still shadows the buffer's binding, as
    // its `SPECPDL_LET_DEFAULT` entry does for GNU's scan
    // (`let_shadows_buffer_binding_p`, `src/eval.c:3593-3610`): the store
    // lands in the default. The owner's record stands in for that entry.
    let evidence = [super::SpecBinding::LetDefault {
        sym_id,
        old_value: super::SavedBindingValue::from_plain(Value::UNBOUND),
        buffer_id: super::SavedBufferId::from_option(buffers.current_buffer_id()),
    }];
    let specpdl = if shadowed { &evidence[..] } else { specpdl };
    let locus = obarray.with_revealed_projection(sym_id, |obarray| {
        super::store_runtime_binding(obarray, buffers, None, specpdl, sym_id, checked)
    });
    // A `set` that made the local again gives a binding of this owner whose
    // local was killed its storage back, as `make-local-variable` does
    // (`Context::owner_local_made`).
    if let Some(buffer) = locus
        && let Some(cell) = buffers
            .get(buffer)
            .and_then(|buf| buf.local_variable_binding_cell(sym_id))
        && let Some(slab) = owner
        && let Some(projection) = obarray.thread_projection_of(sym_id)
        && let Some(hidden) = slab.reattach_captured(
            sym_id,
            projection.index(),
            super::owner_bindings::Channel::BufferLocal(buffer),
            checked.value(),
        )
    {
        cell.set_cdr(hidden.get());
    }
    locus
}
