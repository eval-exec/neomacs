//! Thread admission (Phase 0 step 2, round 3c): the one-time, sticky move of
//! a World from one dormant owner to owner-local dynamic bindings.
//!
//! Admission runs with the owner stopped and the collector closed: GC
//! inhibited, any running cycle drained, concurrent marking excluded. It
//! first plans and reserves everything it needs, and only then commits, with
//! no Lisp, no watcher and no allocation:
//!
//! 1. project every valued cell onto a descriptor of its own and every void
//!    one onto the World's shared VOID descriptor, each descriptor complete
//!    before its cell is redirected;
//! 2. rewrite the owner's live `let` entries in place into owner bindings
//!    (one forward walk; frames keep their specpdl counts);
//! 3. publish, per bound variable and channel, the value its outermost
//!    binding hides as the canonical value;
//! 4. move the function epoch, so every epoch-guarded cache revalidates;
//! 5. republish the host mirrors and the debugger header;
//! 6. make the mode sticky and let symbols born later start on VOID.
//!
//! The rewritten owner is the one an owner that had bound projected variables
//! from the start would be (design §5.2). Its `let` entries per variable and
//! channel, outermost first, become: the first one `Inherited` (popping it
//! exposes canonical storage), the rest `Previous` with the value each saved.
//! To get there, admission unwinds a copy of the entries top down, as
//! `unbind_to` would (a `backtrace-eval` rewind entry re-exchanging the
//! entries below it), then binds them again bottom up as owner bindings,
//! replaying each exchange as an owner exchange where its rewind entry sits.
//! A buffer-local binding whose local is gone keeps the value it hid as
//! `Captured`, restored only if the local is made again (GNU,
//! `src/eval.c:3871-3883`).
//!
//! Left as they are: a per-buffer variable binds in place on both channels
//! (design §1.3b), so its `LetLocal` and `LetDefault` entries stay legacy; `buffer-undo-list`, whose
//! storage is the buffer's undo state, is never projected; nor are
//! constants (the standard case table's variable becomes one: storage for
//! C globals in GNU) and aliases.

use super::owner_bindings::{
    BindingKind, Channel, OwnerBindingRef, OwnerSlab, SlotState, SlotValue, UndoTarget,
    exchanged_binding,
};
use super::{Context, NativeUnwindAction, SpecBinding};
use neovm_value::forward::{LispBoolFwd, TlsIndex};
use neovm_value::intern::SymId;
use neovm_value::symbol::{AdmissionPlanCell, FunctionEpochBump, ValueCell};
use neovm_value::tagged::gc::{
    CollectorQuiescenceError, FacadeMarkExclusion, FacadeMarkExclusionError, TaggedHeap,
};
use neovm_value::value::Value;
use rustc_hash::FxHashMap;
use std::ptr::NonNull;

/// How a World runs Lisp threads, fixed at its construction.
///
/// TEST-SUPPORT-VIS: the enum stays crate-private. The compile-fail goldens
/// name `eval::admission::ThreadingMode` (`Cell<...>: Sync` diagnostics), and
/// V1 is fail-closed over goldens; re-homed Green tests use
/// `Context::set_green_threading_mode_at_construction_for_test` instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThreadingMode {
    /// `make-thread` simulates threads in the one owner, swapping dynamic
    /// bindings in place (`threads`); there is no admission.
    Sim,
    /// Lisp threads are owners with owner-local bindings, after admission.
    Green,
}

impl ThreadingMode {
    /// The mode `NEOVM_THREADS` selects for a new World: `green`, or `sim`
    /// (the default, also when unset). Any other value warns once and
    /// selects `sim`.
    pub(crate) fn from_env() -> Self {
        match std::env::var("NEOVM_THREADS").as_deref() {
            Ok("green") => Self::Green,
            Ok("sim") | Err(_) => Self::Sim,
            Ok(other) => {
                static WARNED: std::sync::Once = std::sync::Once::new();
                WARNED.call_once(|| {
                    tracing::warn!(
                        value = other,
                        "NEOVM_THREADS is neither \"sim\" nor \"green\"; using sim"
                    );
                });
                Self::Sim
            }
        }
    }
}

/// Why admission refused. Every refusal leaves the owner and the World as
/// they were.
#[derive(Debug, thiserror::Error)]
pub(crate) enum AdmissionError {
    #[error("the World does not run green Lisp threads")]
    NotGreen,
    #[error("the World has already admitted threads")]
    AlreadyAdmitted,
    #[error("simulated threads hold suspended bindings")]
    SimulatedThreadsLive,
    #[error("the GC inhibition depth is exhausted")]
    InhibitDepthOverflow,
    #[error(transparent)]
    Collector(#[from] CollectorQuiescenceError),
    #[error(transparent)]
    MarkExclusion(#[from] FacadeMarkExclusionError),
    #[error("admission could not reserve its storage")]
    Capacity,
}

/// What a test makes the reservation do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reservation {
    Real,
    /// Reserve everything, then refuse as if the last reservation had.
    #[cfg(test)]
    RefuseAfterReserving,
}

/// The owner stopped and the collector closed: no collection, no marker
/// launch, no Lisp. Dropping it reopens both.
struct CollectorClosedPhase<'owner> {
    context: &'owner mut Context,
    saved_inhibit: usize,
    _exclusion: FacadeMarkExclusion,
}

static_assertions::assert_not_impl_any!(CollectorClosedPhase<'static>: Send, Sync);

impl<'owner> CollectorClosedPhase<'owner> {
    #[doc(hidden)]
    pub fn enter(context: &'owner mut Context) -> Result<Self, AdmissionError> {
        let saved_inhibit = context.gc_inhibit_depth;
        let next = saved_inhibit
            .checked_add(1)
            .ok_or(AdmissionError::InhibitDepthOverflow)?;
        let heap: *mut TaggedHeap = context.tagged_heap.heap_root().as_ptr();
        // SAFETY: admission owns the stopped, sole owner of this heap and
        // runs no Lisp, callback or finalizer-reentrant code; no shared
        // manager loan is live. The reseed enumerates the complete
        // registered root census and only adds mark roots. The heap loan
        // ends when quiescence becomes the retained exclusion.
        let exclusion = unsafe {
            (*heap)
                .drain_to_quiescent(|heap| {
                    context.seed_registered_mutator_roots_world_stopped(heap);
                })?
                .exclude_concurrent_mark()?
        };
        context.gc_inhibit_depth = next;
        Ok(Self {
            context,
            saved_inhibit,
            _exclusion: exclusion,
        })
    }

    /// Commit admission only while the sole owner is stopped and marking is
    /// excluded. The allocator and the World observe the same sticky state.
    #[doc(hidden)]
    pub fn commit(&mut self, plan: &AdmissionPlan, fresh: Option<NonNull<OwnerSlab>>) {
        self.context.commit_admission(plan, fresh);
        // SAFETY: this phase owns the sole stopped Context, retains marker
        // exclusion, and permits no Lisp/callback or allocation-region access
        // across the transition. The heap closes its regions before the flip.
        unsafe { self.context.tagged_heap.admit_threads_world_stopped() };
        self.context.enter_world_projection_scope();
    }
}

impl Drop for CollectorClosedPhase<'_> {
    fn drop(&mut self) {
        // No callback, collection, barrier or allocation.
        self.context.gc_inhibit_depth = self.saved_inhibit;
    }
}

/// One live `let` entry admission turns into an owner binding.
#[derive(Clone, Copy, Debug)]
struct PlannedBinding {
    /// The entry's specpdl position.
    position: usize,
    sym: SymId,
    kind: BindingKind,
    /// Its variable and channel, in [`AdmissionPlan::chains`].
    chain: usize,
    /// The value the entry saves, while the simulation runs.
    saved: Value,
    undo: UndoTarget,
    /// Whether an odd number of `backtrace-eval`s have exchanged it.
    exchanged: bool,
}

/// The bindings of one variable on one channel.
#[derive(Clone, Copy, Debug)]
struct PlannedChain {
    sym: SymId,
    channel: Channel,
    /// The live storage's value ([`Value::UNBOUND`] when void); `None` for a
    /// buffer-local channel whose local is gone.
    live: Option<Value>,
    /// What the owner's slot holds once admitted.
    slot: SlotState,
    /// What canonical storage holds once admitted, for a live channel.
    canonical: Option<Value>,
}

/// A bound variable's owned-cell ordinal while admission plans. Its own
/// type also keeps this map's `insert` from being the one cconv's
/// `FxHashMap<SymId, usize>` uses: shared, that instance stops being
/// inlined into `FactsBuilder::note` once admission is reachable.
#[derive(Clone, Copy)]
struct SlotOrdinal(usize);

/// Everything the commit does, decided and reserved beforehand.
struct AdmissionPlan {
    cells: Vec<(SymId, AdmissionPlanCell)>,
    owned: usize,
    bindings: Vec<PlannedBinding>,
    chains: Vec<PlannedChain>,
    /// The index each bound variable's descriptor will get.
    indices: FxHashMap<SymId, TlsIndex>,
    /// The owner's `debug-on-next-call` cell, when it binds the default.
    debugger: Option<&'static LispBoolFwd>,
}

impl Context {
    /// A Context whose World runs Lisp threads in MODE, fixed before any
    /// Lisp runs. [`Context::new`] is `Sim`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn new_with_threading(
        mode: ThreadingMode,
        _hooks: super::hooks_proof::EvalHooksInstalled,
    ) -> Self {
        let mut ctx = Self::new(_hooks);
        ctx.set_threading_mode_at_construction(mode);
        ctx
    }

    /// Test-support: fix a freshly constructed Context to Green threading.
    ///
    /// Re-homed provider tests need a Green World but must not name the
    /// crate-private `ThreadingMode` enum (TEST-SUPPORT-VIS V1: the
    /// compile-fail goldens name it). This is the named test-support seam.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_green_threading_mode_at_construction_for_test(&mut self) {
        self.set_threading_mode_at_construction(ThreadingMode::Green);
    }

    /// Admit Lisp threads in this Green World: the one-time, sticky move to
    /// owner-local dynamic bindings, this Context being the first owner.
    /// Reachable from tests and the green-thread runtime only.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn admit_threads(&mut self) -> Result<(), AdmissionError> {
        self.fix_standard_case_table_cell();
        self.admit_threads_with(Reservation::Real)
    }

    /// [`Self::admit_threads`] refusing after it reserved everything.
    #[cfg(test)]
    pub(crate) fn admit_threads_refusing_capacity_for_test(
        &mut self,
    ) -> Result<(), AdmissionError> {
        self.admit_threads_with(Reservation::RefuseAfterReserving)
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn admit_threads_with(
        &mut self,
        reservation: Reservation,
    ) -> Result<(), AdmissionError> {
        if self.threading_mode() != ThreadingMode::Green {
            return Err(AdmissionError::NotGreen);
        }
        if self.threads_admitted() {
            return Err(AdmissionError::AlreadyAdmitted);
        }
        if !self.suspended_thread_bindings.is_empty() {
            return Err(AdmissionError::SimulatedThreadsLive);
        }
        // The heap the collector closes is this owner's, installed on this
        // thread.
        self.setup_thread_locals();
        let mut phase = CollectorClosedPhase::enter(self)?;
        let plan = phase.context.plan_admission();
        let fresh = phase.context.reserve_admission(&plan, reservation)?;
        phase.commit(&plan, fresh);
        Ok(())
    }

    /// Before admitting a Green World, fix the standard case table's
    /// variable as one World value: storage for what GNU keeps in C globals
    /// (`Vascii_downcase_table`), which no thread binds. The table is made
    /// now if it was not yet, as a cell born after admission is born
    /// projected, and the variable becomes a constant, which admission
    /// leaves unprojected. Only Lisp `setq` of this internal variable
    /// notices; casetab writes its storage.
    #[cold]
    #[inline(never)]
    pub(crate) fn fix_standard_case_table_cell(&mut self) {
        if self.threading_mode() != ThreadingMode::Green || self.threads_admitted() {
            return;
        }
        self.setup_thread_locals();
        let _ = crate::emacs_core::eval::case_tables::standard_case_table(self);
        self.obarray.set_constant_id(
            neovm_text::emacs_core::casetab::standard_case_table_object_symbol_id(),
        );
    }

    /// Decide what admission does: the bindings it rewrites, what each
    /// channel publishes and keeps, and the cells it projects.
    pub(crate) fn plan_admission(&mut self) -> AdmissionPlan {
        let undo_list = super::buffer_undo_list_symbol();
        let mut bindings = Vec::new();
        let mut chains: Vec<PlannedChain> = Vec::new();
        let mut chain_of: FxHashMap<(SymId, Channel), usize> = FxHashMap::default();
        let mut rewinds = Vec::new();
        for (position, entry) in self.specpdl.iter().enumerate() {
            let (sym, kind, channel, saved) = match *entry {
                SpecBinding::Let { sym_id, old_value } => (
                    sym_id,
                    BindingKind::Let,
                    Channel::Default,
                    old_value.as_plain(),
                ),
                SpecBinding::LetDefault {
                    sym_id,
                    old_value,
                    buffer_id,
                } => {
                    if self.binds_in_place(sym_id) {
                        continue;
                    }
                    (
                        sym_id,
                        BindingKind::LetDefault {
                            where_buf: buffer_id.get(),
                        },
                        Channel::Default,
                        old_value.as_plain(),
                    )
                }
                SpecBinding::LetLocal {
                    sym_id,
                    old_value,
                    buffer_id,
                } => {
                    if self.binds_in_place(sym_id) {
                        continue;
                    }
                    (
                        sym_id,
                        BindingKind::LetLocal,
                        Channel::BufferLocal(buffer_id),
                        old_value,
                    )
                }
                SpecBinding::NativeUnwind {
                    action: NativeUnwindAction::BacktraceEvalRewind { distance },
                } => {
                    if let Some(base) = position.checked_sub(distance) {
                        rewinds.push((position, base..position));
                    }
                    continue;
                }
                _ => continue,
            };
            if sym == undo_list || self.obarray.admission_plan(sym).is_none() {
                continue;
            }
            let chain = *chain_of.entry((sym, channel)).or_insert_with(|| {
                chains.push(PlannedChain {
                    sym,
                    channel,
                    live: None,
                    slot: SlotState::NoBinding,
                    canonical: None,
                });
                chains.len() - 1
            });
            bindings.push(PlannedBinding {
                position,
                sym,
                kind,
                chain,
                saved,
                undo: UndoTarget::Retired,
                exchanged: false,
            });
        }
        for chain in &mut chains {
            chain.live = match chain.channel {
                Channel::Default => Some(
                    super::super::data::default_value_by_id(self, chain.sym)
                        .unwrap_or(Value::UNBOUND),
                ),
                Channel::BufferLocal(buffer) => {
                    self.local_binding_value_for_thread_switch(chain.sym, buffer)
                }
            };
        }
        reconstruct_owner(&mut bindings, &mut chains, &rewinds);

        let mut ordinals: FxHashMap<SymId, SlotOrdinal> = chains
            .iter()
            .map(|chain| (chain.sym, SlotOrdinal(0)))
            .collect();
        let mut cells = Vec::new();
        let mut owned = 0;
        for (id, _) in self.obarray.iter_symbols() {
            if id == undo_list {
                continue;
            }
            let Some(mut plan) = self.obarray.admission_plan(id) else {
                continue;
            };
            if let Some(ordinal) = ordinals.get_mut(&id) {
                // A bound variable needs a slot index of its own.
                plan = AdmissionPlanCell::Owned;
                *ordinal = SlotOrdinal(owned);
            }
            if plan == AdmissionPlanCell::Owned {
                owned += 1;
            }
            cells.push((id, plan));
        }
        let indices = ordinals
            .into_iter()
            .map(|(id, SlotOrdinal(ordinal))| (id, self.upcoming_thread_projection(ordinal)))
            .collect();
        let debugger_symbol = super::owner_mirrors::debug_on_next_call_symbol();
        let debugger = (chain_of.contains_key(&(debugger_symbol, Channel::Default))
            && !self
                .owner
                .slab()
                .is_some_and(|slab| slab.has_debugger_cell()))
        .then(|| neovm_value::forward::alloc_owner_debugger_cell(false));
        AdmissionPlan {
            cells,
            owned,
            bindings,
            chains,
            indices,
            debugger,
        }
    }

    /// Whether a binding of ID is a per-buffer variable's, which binds in
    /// place on both channels and stays legacy (design §1.3b).
    pub(crate) fn binds_in_place(&self, id: SymId) -> bool {
        self.obarray
            .get_by_id(id)
            .is_some_and(|symbol| match symbol.value_cell() {
                ValueCell::Forwarded(fwd) => fwd.as_buffer_obj_fwd().is_some(),
                _ => false,
            })
    }

    /// Reserve everything PLAN's commit allocates, and the owner's slab if
    /// it has none yet, which the commit installs. A refusal leaves this
    /// owner as it was: what was reserved stays reserved, unseen.
    pub(crate) fn reserve_admission(
        &mut self,
        plan: &AdmissionPlan,
        reservation: Reservation,
    ) -> Result<Option<NonNull<OwnerSlab>>, AdmissionError> {
        self.reserve_thread_projections(plan.owned)
            .map_err(|()| AdmissionError::Capacity)?;
        neovm_value::symbol::reserve_projection_scope().map_err(|_| AdmissionError::Capacity)?;
        let locals = plan
            .chains
            .iter()
            .filter(|chain| matches!(chain.channel, Channel::BufferLocal(_)))
            .count();
        let slots = plan
            .chains
            .iter()
            .filter(|chain| chain.channel == Channel::Default)
            .filter_map(|chain| plan.indices.get(&chain.sym))
            .map(|index| index.slot());
        let fresh = self
            .owner
            .slab()
            .is_none()
            .then(|| self.register_owner_slab());
        match fresh {
            // SAFETY: the registry just made this slab; nothing else names it
            // until the commit installs it.
            Some(mut slab) => unsafe { slab.as_mut() },
            None => self.owner.slab_mut().expect("an installed slab"),
        }
        .try_reserve_admission(plan.bindings.len(), locals, slots)
        .map_err(|_| AdmissionError::Capacity)?;
        match reservation {
            Reservation::Real => Ok(fresh),
            #[cfg(test)]
            Reservation::RefuseAfterReserving => Err(AdmissionError::Capacity),
        }
    }

    /// Commit PLAN, installing FRESH as the owner's slab: no Lisp, no
    /// watcher, no allocation.
    pub(crate) fn commit_admission(
        &mut self,
        plan: &AdmissionPlan,
        fresh: Option<NonNull<OwnerSlab>>,
    ) {
        if let Some(slab) = fresh {
            self.owner.install(slab);
        }
        self.project_admitted_cells(&plan.cells);

        let debugger_symbol = super::owner_mirrors::debug_on_next_call_symbol();
        let slab = self
            .owner
            .slab_mut()
            .expect("admission reserved this owner's slab");
        for binding in &plan.bindings {
            let chain = &plan.chains[binding.chain];
            let index = plan.indices[&binding.sym];
            let record = slab.push_record(
                binding.sym,
                index,
                binding.kind,
                chain.channel,
                binding.undo,
            );
            self.specpdl[binding.position] = SpecBinding::NativeUnwind {
                action: NativeUnwindAction::OwnerBinding(OwnerBindingRef::new(binding.sym, record)),
            };
        }
        for chain in &plan.chains {
            let index = plan.indices[&chain.sym];
            debug_assert_eq!(
                self.obarray
                    .thread_projection_of(chain.sym)
                    .map(|projection| projection.index()),
                Some(index),
                "admission predicted the descriptor's index"
            );
            if chain.sym == debugger_symbol
                && chain.channel == Channel::Default
                && let Some(cell) = plan.debugger
                && let Some(slab) = self.owner.slab_mut()
            {
                slab.adopt_debugger_cell(index, cell);
            }
            if let Some(slab) = self.owner.slab_mut() {
                slab.set_slot(index, chain.channel, chain.slot);
            }
        }

        for chain in &plan.chains {
            if let Some(canonical) = chain.canonical {
                self.owner_canonical_store(chain.sym, chain.channel, canonical);
            }
        }

        self.obarray
            .invalidate_all_function_bindings(FunctionEpochBump::ThreadAdmission);
        self.republish_owner_mirrors();
    }
}

/// A point of the specpdl reconstruction walks.
#[derive(Clone, Copy, Debug)]
enum HistoryPoint {
    /// The planned binding at this index.
    Binding(usize),
    /// The rewind entry of a `backtrace-eval`: the planned bindings in this
    /// index range are the ones it exchanged.
    Rewind(usize, usize),
}

/// Turn the planned entries into the owner an owner that had bound these
/// variables owner-locally from the start would be. REWINDS are the
/// `backtrace-eval` rewind entries, each with the specpdl range it
/// exchanged.
///
/// The walk first unwinds a copy of the entries and their live values, as
/// `unbind_to` would, top down -- a rewind entry re-exchanging its range,
/// a binding giving its value up and restoring the one it saved -- which
/// leaves the value of each binding and, per channel, the value no binding
/// hides: canonical storage. It then binds them again bottom up as owner
/// bindings, replaying each `backtrace-eval` exchange as an owner exchange
/// where its rewind entry sits. A binding made while an exchange hid the
/// outer ones therefore hides canonical storage, as it would have.
///
/// A buffer-local channel whose local is gone has no live value to walk:
/// its first active binding keeps the value it saved as `Captured`, the
/// remaining active entries as `Previous`. Exchanged entries keep their
/// saved binding suspended: GNU only retires them if a later exchange
/// actually encounters the absent local, which may be remade first.
fn reconstruct_owner(
    bindings: &mut [PlannedBinding],
    chains: &mut [PlannedChain],
    rewinds: &[(usize, std::ops::Range<usize>)],
) {
    let mut points: Vec<(usize, HistoryPoint)> = Vec::with_capacity(bindings.len() + rewinds.len());
    points.extend(
        bindings
            .iter()
            .enumerate()
            .map(|(index, binding)| (binding.position, HistoryPoint::Binding(index))),
    );
    for (position, range) in rewinds {
        let start = bindings.partition_point(|binding| binding.position < range.start);
        let end = bindings.partition_point(|binding| binding.position < range.end);
        for binding in &mut bindings[start..end] {
            binding.exchanged = !binding.exchanged;
        }
        points.push((*position, HistoryPoint::Rewind(start, end)));
    }
    points.sort_unstable_by_key(|(position, _)| *position);

    // Unwind the copy, top down.
    let mut values = vec![Value::UNBOUND; bindings.len()];
    for &(_, point) in points.iter().rev() {
        match point {
            HistoryPoint::Binding(index) => {
                let binding = &mut bindings[index];
                if let Some(live) = chains[binding.chain].live.as_mut() {
                    values[index] = std::mem::replace(live, binding.saved);
                }
            }
            HistoryPoint::Rewind(start, end) => {
                for binding in &mut bindings[start..end] {
                    if let Some(live) = chains[binding.chain].live.as_mut() {
                        std::mem::swap(&mut binding.saved, live);
                    }
                }
            }
        }
    }
    for chain in chains.iter_mut() {
        chain.canonical = chain.live;
        chain.slot = SlotState::NoBinding;
    }

    // Bind again as owner bindings, bottom up.
    let mut seen = vec![false; chains.len()];
    for &(_, point) in &points {
        match point {
            HistoryPoint::Binding(index) => {
                let binding = &mut bindings[index];
                let chain = &mut chains[binding.chain];
                if chain.live.is_none() {
                    let saved = SlotValue::new(binding.saved);
                    binding.undo = if binding.exchanged {
                        UndoTarget::Inherited {
                            suspended: Some(saved),
                        }
                    } else if !std::mem::replace(&mut seen[binding.chain], true) {
                        UndoTarget::Captured(saved)
                    } else {
                        UndoTarget::Previous(saved)
                    };
                    continue;
                }
                binding.undo = match chain.slot {
                    SlotState::NoBinding => UndoTarget::Inherited { suspended: None },
                    SlotState::Bound(outer) => UndoTarget::Previous(outer),
                };
                chain.slot = SlotState::Bound(SlotValue::new(values[index]));
            }
            HistoryPoint::Rewind(start, end) => {
                for binding in bindings[start..end].iter_mut().rev() {
                    let chain = &mut chains[binding.chain];
                    if chain.live.is_none() {
                        continue;
                    }
                    if let Some((undo, slot)) = exchanged_binding(binding.undo, chain.slot) {
                        binding.undo = undo;
                        chain.slot = slot;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/admission_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/admission_scenarios_test.rs"]
mod scenario_tests;

#[cfg(test)]
#[path = "tests/admission_allocator_test.rs"]
mod allocator_tests;
