//! Thread-projected cells (Phase 0 step 2): reveal and conceal.
//!
//! A projected cell is `Forwarded` over a [`ThreadProjectionDescriptor`]
//! (see `forward::thread_projection`). For a single owner, a projected
//! variable must behave exactly like the variable it wraps. The slow paths
//! that meet a projected cell therefore REVEAL it -- give the cell back its
//! logical arm and word for the length of the operation -- run the existing
//! operation on that unprojected representation, and CONCEAL it again,
//! recording whatever the operation left as the new canonical storage.
//!
//! Reveal and conceal are cell writes like any other ([`CellWrite`]): a
//! running mark sees the old pair or the new one, and a plain word they
//! overwrite is noted first. While revealed, the variable IS its logical
//! cell, so Lisp an operation runs (a watcher) observes exactly the
//! unprojected variable, and a nested operation on the same symbol finds no
//! projection and runs as it always has. Compiled code baked against either
//! representation re-tests the cell's redirect or word on every access, so
//! nothing it caches outlives a reveal or a conceal.
//!
//! Dynamic bindings never reveal: they live in the binding owner's slab
//! (`eval::owner_bindings`), and no Lisp runs while a cell is revealed.

use super::cell::{ArmMut, RevealedCell};
use super::{Obarray, SymId, SymbolTrappedWrite};
use crate::emacs_core::forward::{
    BuiltinForwarder, CanonicalArm, CanonicalBlv, FwdRef, ThreadProjectionDescriptor,
    ThreadProjectionTable,
};
use crate::emacs_core::value::Value;
use std::cell::RefCell;
use std::ptr::NonNull;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// An admitted World's projection scope, keyed by its obarray's generation:
/// where obarray-level code that is not handed the World (a fresh symbol, a
/// VOID cell's first write) finds the World's table and VOID descriptor.
#[derive(Clone, Copy, Debug)]
struct ProjectionScope {
    generation: u64,
    /// The World's table; the World removes the scope before it drops.
    table: NonNull<RefCell<ThreadProjectionTable>>,
    void: &'static ThreadProjectionDescriptor,
}

// SAFETY: a scope is only a key and two addresses. The table is dereferenced
// only by the obarray of GENERATION, whose World confines it to the thread
// running that World's owner (`materialize_void`), never by the thread that
// happens to hold the registry lock.
unsafe impl Send for ProjectionScope {}

/// How many projection scopes exist in the process: a dormant process
/// answers a fresh symbol with one load.
static PROJECTION_SCOPES_LIVE: AtomicUsize = AtomicUsize::new(0);

/// The admitted Worlds' scopes. A generation names one obarray for the life
/// of the process, so the registry is process-wide: it adds no thread-local
/// storage, and a World's owners find its scope on any thread.
static PROJECTION_SCOPES: Mutex<Vec<ProjectionScope>> = Mutex::new(Vec::new());

fn projection_scopes() -> std::sync::MutexGuard<'static, Vec<ProjectionScope>> {
    // A panic while held leaves a consistent Vec: every update is one push
    // or one swap_remove.
    PROJECTION_SCOPES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Why admission could not reserve what its commit needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProjectionPlanError;

/// Make room to enter one projection scope without allocating.
pub(crate) fn reserve_projection_scope() -> Result<(), ProjectionPlanError> {
    projection_scopes()
        .try_reserve(1)
        .map_err(|_| ProjectionPlanError)
}

/// Enter GENERATION's projection scope: from now on its obarray births
/// symbols on VOID and materializes VOID cells through TABLE. Allocates
/// nothing after [`reserve_projection_scope`].
///
/// # Safety
/// TABLE is the World's, inside the shared World allocation that owns the
/// obarray of GENERATION, and the World leaves the scope
/// ([`leave_projection_scope`]) before it drops.
pub(crate) unsafe fn enter_projection_scope(
    generation: u64,
    table: &RefCell<ThreadProjectionTable>,
    void: &'static ThreadProjectionDescriptor,
) {
    let mut scopes = projection_scopes();
    if scopes.iter().any(|scope| scope.generation == generation) {
        return;
    }
    scopes.push(ProjectionScope {
        generation,
        table: NonNull::from(table),
        void,
    });
    PROJECTION_SCOPES_LIVE.fetch_add(1, Ordering::Relaxed);
}

/// Leave GENERATION's projection scope, if it has one.
pub(crate) fn leave_projection_scope(generation: u64) {
    if PROJECTION_SCOPES_LIVE.load(Ordering::Relaxed) == 0 {
        return;
    }
    let mut scopes = projection_scopes();
    if let Some(position) = scopes
        .iter()
        .position(|scope| scope.generation == generation)
    {
        scopes.swap_remove(position);
        PROJECTION_SCOPES_LIVE.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cold]
#[inline(never)]
fn scope_for(generation: u64) -> Option<ProjectionScope> {
    if PROJECTION_SCOPES_LIVE.load(Ordering::Relaxed) == 0 {
        return None;
    }
    projection_scopes()
        .iter()
        .find(|scope| scope.generation == generation)
        .copied()
}

/// Whether any World in the process has admitted threads: one load.
#[inline(always)]
pub(super) fn scopes_live() -> bool {
    PROJECTION_SCOPES_LIVE.load(Ordering::Relaxed) != 0
}

/// The header a fresh symbol of GENERATION's obarray is born on: VOID once
/// its World admitted threads, none before. One load in a dormant process.
#[inline]
pub(super) fn born_projected(generation: u64) -> Option<FwdRef> {
    if PROJECTION_SCOPES_LIVE.load(Ordering::Relaxed) == 0 {
        return None;
    }
    born_projected_slow(generation)
}

#[cold]
#[inline(never)]
fn born_projected_slow(generation: u64) -> Option<FwdRef> {
    scope_for(generation).map(|scope| scope.void.header())
}

/// The scope whose VOID descriptor is VOID.
#[cold]
#[inline(never)]
fn scope_of_void(void: &ThreadProjectionDescriptor) -> Option<ProjectionScope> {
    projection_scopes()
        .iter()
        .find(|scope| std::ptr::eq(scope.void, void))
        .copied()
}

/// Give the cell FORWARDED writes, projected on its World's VOID, a
/// descriptor of its own from that World's table: complete before the
/// cell is redirected, rooted by the table. `None` if the World has no
/// scope.
fn materialize_in_place(
    forwarded: super::cell::ForwardedArm<'_, '_>,
    void: &'static ThreadProjectionDescriptor,
) -> Option<&'static ThreadProjectionDescriptor> {
    let scope = scope_of_void(void)?;
    // SAFETY: the scope names the World that owns this cell's obarray,
    // alive while the obarray is; the table is borrowed for this issue only.
    let table = unsafe { scope.table.as_ref() };
    let projection = table
        .borrow_mut()
        .issue(CanonicalArm::Plain, Value::UNBOUND, false);
    forwarded.conceal_projection(projection);
    Some(projection)
}

/// A storage-level write of VALUE to a projected variable, whose cell
/// FORWARDED writes: its canonical storage takes it in place, as the
/// unprojected cell would have. A cell on VOID (born by this write) gets a
/// descriptor of its own first. Needs no obarray, so the writer's own path
/// stays as it was.
#[cold]
#[inline(never)]
pub(super) fn store_projected_canonical(
    forwarded: super::cell::ForwardedArm<'_, '_>,
    projection: &'static ThreadProjectionDescriptor,
    value: Value,
) {
    let projection = if projection.is_void() {
        match materialize_in_place(forwarded, projection) {
            Some(projection) => projection,
            None => {
                debug_assert!(false, "a VOID cell outside any admitted World");
                return;
            }
        }
    } else {
        projection
    };
    match projection.arm() {
        CanonicalArm::Plain => projection.set_plain_value(value),
        // SAFETY: a projection's localized arm names a live record of the
        // obarray's BLV pool; its default cell is written as the
        // unprojected `Localized` arm writes it.
        CanonicalArm::Localized(blv) => unsafe {
            let blv = &mut *blv.blv().as_ptr();
            blv.defcell.get().set_cdr(value);
            if super::super::value::eq_value(&blv.valcell.get(), &blv.defcell.get()) {
                blv.valcell.get().set_cdr(value);
            }
        },
        CanonicalArm::Forwarded(builtin) => {
            let fwd = builtin.descriptor();
            match fwd.store(value) {
                Ok(store) => {
                    fwd.commit(store);
                }
                Err(error) => {
                    debug_assert!(false, "forwarded slot refused an internal write: {error:?}")
                }
            }
        }
    }
}

/// What admission does with one cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdmissionPlanCell {
    /// Storage of its own: a value, a buffer-local record, a builtin, or the
    /// host-projection bit. It gets its own descriptor and index.
    Owned,
    /// A void, unflagged cell: the World's shared VOID descriptor.
    Void,
}

/// A projected cell an operation has revealed, to be concealed again with
/// [`Obarray::conceal_projection`]. Neither `Copy` nor `Clone`: one reveal,
/// one conceal.
#[must_use = "a revealed projection must be concealed again"]
#[derive(Debug)]
pub(crate) struct RevealedProjection {
    id: SymId,
    projection: &'static ThreadProjectionDescriptor,
}

static_assertions::assert_not_impl_any!(RevealedProjection: Copy, Clone, Send, Sync);

/// What concealing a revealed cell found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConcealOutcome {
    /// The cell is projected again over its (possibly changed) logical arm.
    Concealed,
    /// The operation made the variable an alias (`defvaralias`): an alias
    /// stays an alias and resolves to its base, so the projection retires.
    RetiredToAlias,
}

impl Obarray {
    /// Whether a cell made now is born projected: this obarray's World has
    /// admitted threads. One load in a dormant process.
    #[inline]
    pub(crate) fn births_projected(&self) -> bool {
        born_projected(self.generation).is_some()
    }

    /// The thread projection ID's cell heads, if it is projected.
    #[inline]
    pub(crate) fn thread_projection_of(
        &self,
        id: SymId,
    ) -> Option<&'static ThreadProjectionDescriptor> {
        self.slot(id)?.forwarded_descriptor()?.thread_projection()
    }

    /// Give ID's projected cell its logical arm, word and host-projection
    /// bit back. `None`, changing nothing, unless ID is projected.
    pub(crate) fn reveal_projection(&mut self, id: SymId) -> Option<RevealedProjection> {
        if self.slot(id).is_none() && self.births_projected() {
            // The operation would make the cell; it is born projected, so
            // make it now and let the operation see it unprojected.
            self.ensure_slot(id);
        }
        let projection = self.owned_thread_projection(id)?;
        let logical = match projection.arm() {
            CanonicalArm::Plain => RevealedCell::Plain(projection.plain_value()),
            CanonicalArm::Localized(blv) => RevealedCell::Localized(blv.blv()),
            CanonicalArm::Forwarded(builtin) => RevealedCell::Forwarded(builtin.descriptor()),
        };
        {
            let mut write = self.cell_write_ensure(id);
            let ArmMut::Forwarded(forwarded) = write.arm() else {
                unreachable!("a projected cell is forwarded");
            };
            forwarded.reveal_projection(logical);
        }
        self.ensure_slot(id)
            .set_runtime_projected(projection.host_projected());
        Some(RevealedProjection { id, projection })
    }

    /// Conceal a revealed cell again over its projection, recording the
    /// logical arm and word the operation left.
    pub(crate) fn conceal_projection(&mut self, revealed: RevealedProjection) -> ConcealOutcome {
        let RevealedProjection { id, projection } = revealed;
        {
            let mut write = self.cell_write_ensure(id);
            match write.arm() {
                ArmMut::Plain(plain) => {
                    projection.set_plain_value(plain.value());
                    projection.set_arm(CanonicalArm::Plain);
                    plain.conceal_projection(projection);
                }
                ArmMut::Localized(local) => {
                    projection.set_arm(CanonicalArm::Localized(CanonicalBlv::new(local.blv())));
                    local.conceal_projection(projection);
                }
                ArmMut::Forwarded(forwarded) => {
                    let fwd = forwarded.descriptor();
                    // A nested operation may already have concealed it.
                    if fwd != projection.header() {
                        let builtin = BuiltinForwarder::new(fwd)
                            .expect("a revealed cell never forwards to another projection");
                        projection.set_arm(CanonicalArm::Forwarded(builtin));
                        forwarded.conceal_projection(projection);
                    }
                }
                ArmMut::Alias(_) => return ConcealOutcome::RetiredToAlias,
            }
        }
        self.mark_runtime_projected_id(id);
        ConcealOutcome::Concealed
    }

    /// The projection ID's cell heads, materialized onto a descriptor of its
    /// own if it is on the World's shared VOID descriptor: what a bind or a
    /// canonical write needs, since neither may touch VOID. Cold: a cell
    /// leaves VOID once.
    pub(crate) fn owned_thread_projection(
        &mut self,
        id: SymId,
    ) -> Option<&'static ThreadProjectionDescriptor> {
        let projection = self.thread_projection_of(id)?;
        if !projection.is_void() {
            return Some(projection);
        }
        self.materialize_void(id)
    }

    #[cold]
    #[inline(never)]
    fn materialize_void(&mut self, id: SymId) -> Option<&'static ThreadProjectionDescriptor> {
        let void = self.thread_projection_of(id)?;
        let mut write = self.cell_write_ensure(id);
        let ArmMut::Forwarded(forwarded) = write.arm() else {
            unreachable!("a projected cell is forwarded");
        };
        materialize_in_place(forwarded, void)
    }
    /// What admission does with ID's cell: `None` for a cell it leaves alone
    /// (empty, an alias, a constant, already projected).
    pub(crate) fn admission_plan(&self, id: SymId) -> Option<AdmissionPlanCell> {
        let sym = self.slot(id)?;
        if sym.trapped_write() == SymbolTrappedWrite::NoWrite {
            return None;
        }
        let host_projected = sym.flags().runtime_projected();
        match sym.value_cell() {
            super::ValueCell::Plain(value) => Some(if value.is_unbound() && !host_projected {
                AdmissionPlanCell::Void
            } else {
                AdmissionPlanCell::Owned
            }),
            super::ValueCell::Localized(_) => Some(AdmissionPlanCell::Owned),
            super::ValueCell::Forwarded(fwd) => {
                BuiltinForwarder::new(fwd).map(|_| AdmissionPlanCell::Owned)
            }
            super::ValueCell::Alias(_) => None,
        }
    }

    /// `defvaralias` of the projected variable ID onto TARGET: a plain
    /// variable becomes the alias, and an alias resolves to its base, so
    /// the projection retires. A buffer-local or built-in variable is
    /// refused as its unprojected cell would be.
    #[cold]
    #[inline(never)]
    pub(super) fn make_projected_alias(
        &mut self,
        id: SymId,
        target: SymId,
    ) -> Result<(), super::MakeAliasError> {
        let projection = self
            .thread_projection_of(id)
            .expect("the caller saw a projection");
        match projection.arm() {
            CanonicalArm::Plain => {}
            CanonicalArm::Localized(_) => return Err(super::MakeAliasError::Localized),
            CanonicalArm::Forwarded(_) => return Err(super::MakeAliasError::Forwarded),
        }
        let revealed = self
            .reveal_projection(id)
            .expect("a projected plain variable reveals");
        let made = self.make_alias(id, target);
        let outcome = self.conceal_projection(revealed);
        debug_assert!(
            made.is_err() || outcome == ConcealOutcome::RetiredToAlias,
            "an alias retires its projection"
        );
        made
    }

    /// Admission's projection of ID's cell, as PLAN said: onto a descriptor
    /// TABLE issues (complete before the cell is redirected), or onto VOID.
    /// Allocates nothing once TABLE is reserved.
    pub(crate) fn project_admitted_cell(
        &mut self,
        id: SymId,
        plan: AdmissionPlanCell,
        table: &mut ThreadProjectionTable,
    ) {
        match plan {
            AdmissionPlanCell::Owned => {
                let _ = self.project_owned(id, table);
            }
            AdmissionPlanCell::Void => {
                let void = table.void_descriptor();
                {
                    let mut write = self.cell_write_ensure(id);
                    let ArmMut::Plain(plain) = write.arm() else {
                        unreachable!("admission plans VOID only for a plain cell");
                    };
                    plain.conceal_projection(void);
                }
                self.mark_runtime_projected_id(id);
            }
        }
    }

    /// Project `id`'s variable onto a descriptor of its own from TABLE: its
    /// cell becomes `Forwarded` over it, its canonical storage keeps the
    /// cell's logical arm and word, and its host-projection bit is set so
    /// every checked writer refuses it. `None`, changing nothing, for an
    /// alias or a variable already projected.
    fn project_owned(
        &mut self,
        id: SymId,
        table: &mut ThreadProjectionTable,
    ) -> Option<&'static ThreadProjectionDescriptor> {
        let host_projected = self.ensure_slot(id).flags().runtime_projected();
        let projection = {
            let mut write = self.cell_write_ensure(id);
            match write.arm() {
                ArmMut::Plain(plain) => {
                    let projection =
                        table.issue(CanonicalArm::Plain, plain.value(), host_projected);
                    plain.conceal_projection(projection);
                    projection
                }
                ArmMut::Localized(local) => {
                    let projection = table.issue(
                        CanonicalArm::Localized(CanonicalBlv::new(local.blv())),
                        Value::UNBOUND,
                        host_projected,
                    );
                    local.conceal_projection(projection);
                    projection
                }
                ArmMut::Forwarded(forwarded) => {
                    let builtin = BuiltinForwarder::new(forwarded.descriptor())?;
                    let projection = table.issue(
                        CanonicalArm::Forwarded(builtin),
                        Value::UNBOUND,
                        host_projected,
                    );
                    forwarded.conceal_projection(projection);
                    projection
                }
                ArmMut::Alias(_) => return None,
            }
        };
        self.mark_runtime_projected_id(id);
        Some(projection)
    }

    /// [`Self::project_owned`] of one variable, for tests that project
    /// without admitting.
    #[cfg(test)]
    pub(crate) fn project_thread_variable_for_test(
        &mut self,
        id: SymId,
        table: &mut ThreadProjectionTable,
    ) -> Option<&'static ThreadProjectionDescriptor> {
        self.project_owned(id, table)
    }
}

impl Obarray {
    /// GNU `find_symbol_value` of a projected variable whose logical arm is
    /// `Localized` BLV, in BUFFER (the record's default without one): the
    /// read [`Self::read_localized_in_buffer`] makes, on the record the
    /// projection names rather than the one a cell would.
    ///
    /// The target buffer is its registered object only. `Value::make_buffer`
    /// would allocate one when the registry has none yet, and admission's
    /// commit republishes its mirrors through here with the collector closed
    /// and nothing may allocate; a fabricated object could not be stored
    /// into the BLV cache either, never being `eq` the registered one. With
    /// no object yet, the load still records THIS buffer's answer --
    /// `found` and `valcell`, with no buffer to name: `set_where(NIL)`
    /// stores `NO_WHERE_BUF`, which no read fast path matches, and
    /// `variable-binding-locus` refuses a nil `where_buf`, re-deriving from
    /// the buffer itself and answering with the object it allocates there.
    /// The next read after the object exists swaps the cache in as usual.
    pub(crate) fn read_projected_localized(
        &self,
        id: SymId,
        blv: CanonicalBlv,
        buffer: Option<&crate::buffer::Buffer>,
    ) -> crate::emacs_core::value::Value {
        let blv_ptr = blv.blv().as_ptr();
        match buffer {
            Some(buffer) => {
                let target_buf =
                    crate::tagged::gc::with_tagged_heap(|heap| heap.buffer_value(buffer.id));
                match target_buf {
                    // SAFETY: a projection's localized arm names a record this
                    // obarray's BLV pool owns (it was the cell's record when the
                    // variable was projected or revealed), and no reference to it
                    // is held across the call.
                    Some(target_buf) => unsafe {
                        Self::read_blv_with_lookup(blv_ptr, target_buf, || {
                            buffer.local_variable_binding_cell(id).unwrap_or(Value::NIL)
                        })
                    },
                    None => {
                        let found_cell =
                            buffer.local_variable_binding_cell(id).unwrap_or(Value::NIL);
                        let found = !found_cell.is_nil();
                        // SAFETY: as above; the same stores
                        // `read_blv_with_lookup` makes on its miss, so a
                        // concurrent collector reads the same pair under the
                        // same release ordering.
                        unsafe {
                            (*blv_ptr).set_where(Value::NIL);
                            (*blv_ptr).found = found;
                            let valcell = if found {
                                found_cell
                            } else {
                                (*blv_ptr).defcell.get()
                            };
                            (*blv_ptr).valcell.store_atomic(valcell);
                            (*blv_ptr).alist_epoch = super::blv_alist_epoch();
                            valcell.cons_cdr()
                        }
                    }
                }
            }
            // SAFETY: as above; the default cell is read without a write.
            None => unsafe { (*blv_ptr).defcell.get().cons_cdr() },
        }
    }
}

impl Obarray {
    /// Run OP with ID's projected cell revealed, then conceal it again: the
    /// storage-level form of `Context::with_revealed_projection`, for writers
    /// below the evaluator. ID must be projected; OP sees the unprojected
    /// variable.
    pub(crate) fn with_revealed_projection<R>(
        &mut self,
        id: SymId,
        op: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let revealed = self.reveal_projection(id);
        let result = op(self);
        if let Some(revealed) = revealed {
            let _ = self.conceal_projection(revealed);
        }
        result
    }

    /// GNU `default_value` of a projected variable without buffer context:
    /// the plain word, the BLV default cell, or the builtin's own storage
    /// (a per-buffer builtin's installation default). `None` when void.
    #[cold]
    #[inline(never)]
    pub(crate) fn projected_default_value(
        &self,
        projection: &ThreadProjectionDescriptor,
    ) -> Option<crate::emacs_core::value::Value> {
        let value = match projection.arm() {
            CanonicalArm::Plain => projection.plain_value(),
            // SAFETY: a projection's localized arm names a record this
            // obarray's BLV pool owns; the default cell is only read.
            CanonicalArm::Localized(blv) => unsafe {
                (*blv.blv().as_ptr()).defcell.get().cons_cdr()
            },
            CanonicalArm::Forwarded(builtin) => {
                let fwd = builtin.descriptor();
                return fwd
                    .load()
                    .or_else(|| fwd.as_buffer_obj_fwd().map(|buf_fwd| buf_fwd.default));
            }
        };
        (!value.is_unbound()).then_some(value)
    }
}
