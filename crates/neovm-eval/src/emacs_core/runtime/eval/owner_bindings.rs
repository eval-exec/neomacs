//! Owner-local dynamic bindings (Phase 0 step 2, round 3).
//!
//! Once a variable is projected (`forward::thread_projection`), a dynamic
//! binding of it no longer writes shared storage. It lives in the binding
//! OWNER's slab: a slot per projected variable and channel holding the
//! owner's current binding, and a binding stack of records saying what each
//! binding hides. The specpdl carries one `NativeUnwindAction::OwnerBinding`
//! per binding, in GNU's order, so unwinding, `backtrace-eval` and panic
//! recovery reach the record exactly where they would reach a `Let` entry.
//!
//! Reads and writes consult the owner's slot first and the variable's
//! canonical storage otherwise (`eval::thread_projection`). The canonical
//! storage is what every other owner sees: an owner's binding is invisible
//! to them, and a write by an owner with no binding is visible to all, as
//! in GNU's threads, where a thread switch swaps the bindings out
//! (`src/thread.c:87-99`).
//!
//! Slabs are allocated by the World and registered there; the collector
//! traces every registered slab in its root phase, so a parked owner's
//! bindings stay rooted without reading its Context. A slot write first
//! notes the value it overwrites for a running mark (SATB).
//!
//! The same cold per-owner allocation holds native Green-seat and file-I/O
//! policy state. An owner may allocate its slab before binding any projected
//! variable; those native fields contain no Lisp values to trace.

use super::{AutoSaveNativeWritePolicy, Context, NativeUnwindAction, SpecBinding};
use neovm_edit::buffer::{BufferId, BufferManager};
use neovm_value::error::Flow;
use neovm_value::forward::{
    CanonicalArm, LispBoolFwd, LispBufferObjFwd, LispInteger, ThreadProjectionDescriptor, TlsIndex,
};
use neovm_value::intern::SymId;
use neovm_value::symbol::{CanonicalAccess, SetInternalBind};
use neovm_value::value::Value;
use rustc_hash::FxHashMap;
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};

/// Slots per page of an owner's default-channel table.
const SLOT_PAGE: usize = 256;

/// The value of an owner's binding. Any value, including
/// [`Value::UNBOUND`] for a binding made void (`makunbound` inside a `let`).
#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub(crate) struct SlotValue(Value);

impl SlotValue {
    #[inline]
    #[doc(hidden)]
    pub fn new(value: Value) -> Self {
        Self(value)
    }

    /// The stored value; [`Value::UNBOUND`] for a void binding.
    #[inline]
    #[doc(hidden)]
    pub fn get(self) -> Value {
        self.0
    }

    /// The value, or `None` for a void binding.
    #[inline]
    #[doc(hidden)]
    pub fn bound(self) -> Option<Value> {
        (!self.0.is_unbound()).then_some(self.0)
    }
}

/// Whether this owner binds a variable on one channel. `NoBinding` is not a
/// value: reads then fall through to canonical storage.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum SlotState {
    #[default]
    NoBinding,
    Bound(SlotValue),
}

impl SlotState {
    #[inline]
    #[doc(hidden)]
    pub fn value(self) -> Option<SlotValue> {
        match self {
            Self::NoBinding => None,
            Self::Bound(value) => Some(value),
        }
    }
}

/// Which storage of a variable a binding shadows: its default (global, or
/// the default of a buffer-local variable) or one buffer's local binding.
/// A buffer id is never reused, so it names that buffer for good.
///
/// Neomacs has one keyboard, so a `DEFVAR_KBOARD` variable binds on the
/// default channel; several terminals would add a keyboard channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Channel {
    Default,
    BufferLocal(BufferId),
}

impl Channel {
    /// The buffer an assignment on this channel lands in, as
    /// `store_runtime_binding` reports it.
    #[inline]
    pub(crate) fn locus(self) -> Option<BufferId> {
        match self {
            Self::Default => None,
            Self::BufferLocal(buffer) => Some(buffer),
        }
    }
}

/// GNU's kind of `let` entry for the binding (`src/lisp.h:3548-3567`): what
/// `default-toplevel-value` and `let_shadows_buffer_binding_p` look for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingKind {
    /// `SPECPDL_LET`.
    Let,
    /// `SPECPDL_LET_DEFAULT`, recording the buffer current at the bind.
    LetDefault { where_buf: Option<BufferId> },
    /// `SPECPDL_LET_LOCAL`: the channel names the buffer.
    LetLocal,
}

/// What popping a binding restores.
#[derive(Clone, Copy, Debug)]
pub(crate) enum UndoTarget {
    /// The binding hides no binding of this owner: popping it exposes the
    /// canonical value, whatever another owner wrote meanwhile. SUSPENDED
    /// holds the binding while `backtrace-eval` has unwound it.
    Inherited { suspended: Option<SlotValue> },
    /// The binding hides this owner's outer binding with this value.
    Previous(SlotValue),
    /// The canonical storage the binding hid was killed by this owner
    /// (`kill-local-variable`) while bound; this is the value it held, which
    /// GNU's specpdl entry still carries and restores if the local is made
    /// again.
    Captured(SlotValue),
    /// The binding was dropped, as GNU turns a `let` whose local is gone
    /// into `SPECPDL_NOP` (`src/eval.c:4204-4228`); popping it does nothing.
    Retired,
}

impl UndoTarget {
    #[doc(hidden)]
    pub fn values(self) -> [Option<SlotValue>; 1] {
        match self {
            Self::Inherited { suspended } => [suspended],
            Self::Previous(value) | Self::Captured(value) => [Some(value)],
            Self::Retired => [None],
        }
    }
}

/// GNU `specpdl_unrewind`'s exchange of one owner binding whose record
/// says UNDO while its slot holds SLOT (`backtrace-eval`, eval.c:4177-4228):
/// what the record would restore and what the slot holds trade places, so
/// the exchange is its own inverse. A record restores `NoBinding` -- the
/// canonical value shows through -- when it hides no binding of the owner
/// (`Inherited`), and the binding it holds otherwise; a binding set aside
/// while canonical storage shows is `Inherited { suspended }`. `None` when
/// there is nothing to exchange: a binding whose storage was killed or
/// dropped.
pub(super) fn exchanged_binding(
    undo: UndoTarget,
    slot: SlotState,
) -> Option<(UndoTarget, SlotState)> {
    let restores = match undo {
        UndoTarget::Inherited { suspended: None } => SlotState::NoBinding,
        UndoTarget::Inherited {
            suspended: Some(value),
        }
        | UndoTarget::Previous(value) => SlotState::Bound(value),
        UndoTarget::Captured(_) | UndoTarget::Retired => return None,
    };
    let undo = match (slot, restores) {
        (SlotState::NoBinding, _) => UndoTarget::Inherited { suspended: None },
        (SlotState::Bound(value), SlotState::NoBinding) => UndoTarget::Inherited {
            suspended: Some(value),
        },
        (SlotState::Bound(value), SlotState::Bound(_)) => UndoTarget::Previous(value),
    };
    Some((undo, restores))
}

/// One binding on an owner's binding stack.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OwnerBindingRecord {
    pub(crate) sym: SymId,
    pub(crate) index: TlsIndex,
    pub(crate) kind: BindingKind,
    pub(crate) channel: Channel,
    pub(crate) undo: UndoTarget,
    generation: NonZeroU32,
}

/// A record's position on its owner's binding stack plus the generation it
/// was issued at, so a stale id (a record already popped, its position
/// reused) never names the new record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OwnerRecordId {
    depth: NonZeroU32,
    generation: NonZeroU32,
}

impl OwnerRecordId {
    #[inline]
    #[doc(hidden)]
    pub fn position(self) -> usize {
        // A depth fits `usize` on every supported target.
        (self.depth.get() - 1) as usize
    }
}

/// The specpdl's handle on an owner binding: the bound symbol, read by the
/// unwinder to decide whether watchers make the pop a Lisp cleanup, and the
/// record. No `Value`: the record's values live in the slab, which the
/// collector traces.
#[derive(Clone, Copy, Debug)]
pub struct OwnerBindingRef {
    sym: SymId,
    record: OwnerRecordId,
}

impl OwnerBindingRef {
    /// The specpdl's handle on RECORD, a binding of SYM.
    #[inline]
    #[doc(hidden)]
    pub(crate) fn new(sym: SymId, record: OwnerRecordId) -> Self {
        Self { sym, record }
    }

    /// The symbol the binding binds (alias-resolved, as GNU records it).
    #[inline]
    #[doc(hidden)]
    pub fn symbol(self) -> SymId {
        self.sym
    }
}

/// One owner's cold native state and bindings: the current variable slots
/// and binding stack, Green seat, and native file-I/O policy.
///
/// Owned by the World's [`OwnerSlabRegistry`], mutated only by its owner,
/// and read by the collector's root phase.
#[doc(hidden)]
pub struct OwnerSlab {
    /// The heap whose values the slab holds.
    heap: usize,
    /// Default-channel slots, by `TlsIndex::slot()`, in lazily made pages.
    pages: Vec<Option<Box<[SlotState; SLOT_PAGE]>>>,
    /// Buffer-local-channel slots that hold a binding.
    locals: FxHashMap<(TlsIndex, BufferId), SlotValue>,
    /// The binding stack, in specpdl order; a popped record leaves `None`
    /// until every record above it is popped too.
    records: Vec<Option<OwnerBindingRecord>>,
    next_generation: NonZeroU32,
    /// `debug-on-next-call`'s slot index and the owner's own cell for it,
    /// once the owner binds it: the cell mirrors the default-channel slot,
    /// so compiled code's armed test (one load through the admission
    /// header) reads the owner's binding.
    debugger: Option<(TlsIndex, &'static LispBoolFwd)>,
    /// The values of the owner's in-place buffer bindings while it is
    /// parked: its specpdl saves them then, and the slab keeps them traced.
    parked: Vec<Value>,
    /// This owner's seat in a Green runtime (round 3d): its Lisp thread's
    /// key, channel and, while it runs, the World's Baton.
    green: Option<Box<super::green::GreenSeat>>,
    /// This owner's `condition-wait` typestate when running as an unseated
    /// parallel owner (E5.4a D4), mirroring `GreenSeat::condition_wait`.
    pub(crate) parallel_condition_wait: super::green::condition_wait::ConditionWait,
    /// GNU fileio.c's auto-saving flag and mode bits, local to this owner.
    /// No Lisp argument, manager reference or native handle is retained.
    native_fileio: AutoSaveNativeWritePolicy,
    /// GNU fileio.c's `auto_saving_dir_umask` (fileio.c:159): zero normally,
    /// `077` only while `do_auto_save_make_dir` (fileio.c:6297-6306) runs the
    /// `make-directory` funcall, with `do_auto_save_eh` (:6308-6313) clearing
    /// it on error. Kept beside `native_fileio` but separate, exactly as GNU
    /// keeps the write policy and the directory umask as independent statics.
    native_fileio_dir_umask: u32,
    /// Dormant selection arm. No Lisp values, so the collector does not
    /// trace it. The clipboard stubs do not read it. Absent until this
    /// owner allocates a slab, and uninstalled when the slab is new.
    pub(crate) selection_activation: super::activation::SelectionActivation,
    /// Whether this slab's unique parallel [`OwnerSlabHandle`] has already
    /// been handed out (E4-P2 C1). At most one handle may exist per slab.
    parallel_handle_issued: bool,
    /// Whether a parallel [`OwnerSlabLease`] currently holds exclusive access
    /// to this slab (E4-P2 C1). Checked via raw pointer before materializing
    /// any `&OwnerSlab` / `&mut OwnerSlab` through `OwnerSlot`, so aliasing UB
    /// is impossible even under synthetic test tickets or forgotten leases.
    parallel_lease_active: AtomicBool,
    _thread_confined: PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(OwnerSlab: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(SlotValue: Send, Sync);

impl std::fmt::Debug for OwnerSlab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnerSlab")
            .field("heap", &format_args!("{:#x}", self.heap))
            .field("records", &self.records.len())
            .field("locals", &self.locals.len())
            .field("selection", &self.selection_activation.phase())
            .finish_non_exhaustive()
    }
}

/// What a `DEFVAR_BOOL` cell holds for a binding STATE: GNU's `!NILP`.
#[inline]
fn debugger_armed(state: SlotState) -> bool {
    state
        .value()
        .and_then(SlotValue::bound)
        .is_some_and(|value| !value.is_nil())
}

/// Note a value leaving the slab for a running mark: the snapshot taken at
/// the mark's start may name it, and nothing else need hold it now.
#[inline]
fn note_dropped(value: Option<SlotValue>) {
    if let Some(value) = value {
        neovm_value::tagged::gc::note_root_overwrite(value.get());
    }
}

impl OwnerSlab {
    #[doc(hidden)]
    pub fn new(heap: usize) -> Self {
        Self {
            heap,
            pages: Vec::new(),
            locals: FxHashMap::default(),
            records: Vec::new(),
            next_generation: NonZeroU32::MIN,
            debugger: None,
            parked: Vec::new(),
            green: None,
            parallel_condition_wait: super::green::condition_wait::ConditionWait::Completed,
            native_fileio: AutoSaveNativeWritePolicy::Ordinary { preserved_mode: 0 },
            native_fileio_dir_umask: 0,
            selection_activation: super::activation::SelectionActivation::dormant(),
            parallel_handle_issued: false,
            parallel_lease_active: AtomicBool::new(false),
            _thread_confined: PhantomData,
        }
    }

    /// The slot of the variable INDEX names on CHANNEL.
    #[inline]
    #[doc(hidden)]
    pub fn slot(&self, index: TlsIndex, channel: Channel) -> SlotState {
        match channel {
            Channel::Default => {
                let slot = index.slot();
                self.pages
                    .get(slot / SLOT_PAGE)
                    .and_then(Option::as_deref)
                    .map_or(SlotState::NoBinding, |page| page[slot % SLOT_PAGE])
            }
            Channel::BufferLocal(buffer) => self
                .locals
                .get(&(index, buffer))
                .map_or(SlotState::NoBinding, |value| SlotState::Bound(*value)),
        }
    }

    /// Replace the slot of INDEX on CHANNEL, noting the value it held.
    #[inline]
    #[doc(hidden)]
    pub fn set_slot(&mut self, index: TlsIndex, channel: Channel, state: SlotState) {
        let old = match channel {
            Channel::Default => {
                let slot = index.slot();
                match self
                    .pages
                    .get_mut(slot / SLOT_PAGE)
                    .and_then(Option::as_deref_mut)
                {
                    Some(page) => std::mem::replace(&mut page[slot % SLOT_PAGE], state).value(),
                    None if matches!(state, SlotState::NoBinding) => None,
                    None => {
                        self.new_page(slot)[slot % SLOT_PAGE] = state;
                        None
                    }
                }
            }
            Channel::BufferLocal(buffer) => match state {
                SlotState::Bound(value) => self.locals.insert((index, buffer), value),
                SlotState::NoBinding => self.locals.remove(&(index, buffer)),
            },
        };
        note_dropped(old);
        if let Some((debugger, cell)) = self.debugger
            && debugger == index
            && channel == Channel::Default
        {
            cell.set(debugger_armed(state));
        }
    }

    /// Replace a default-channel binding without the local-channel map
    /// dispatch. This is used only after a projected binding has selected
    /// its channel; ordinary specbind and unbind paths remain unchanged.
    #[inline(always)]
    pub(crate) fn replace_default_slot(&mut self, index: TlsIndex, state: SlotState) -> SlotState {
        let slot = index.slot();
        let old = match self
            .pages
            .get_mut(slot / SLOT_PAGE)
            .and_then(Option::as_deref_mut)
        {
            Some(page) => std::mem::replace(&mut page[slot % SLOT_PAGE], state),
            None if matches!(state, SlotState::NoBinding) => SlotState::NoBinding,
            None => {
                self.new_page(slot)[slot % SLOT_PAGE] = state;
                SlotState::NoBinding
            }
        };
        note_dropped(old.value());
        if let Some((debugger, cell)) = self.debugger
            && debugger == index
        {
            cell.set(debugger_armed(state));
        }
        old
    }

    /// Restore an unwatched default binding without materializing its
    /// complete record in the unwinder's frame. Eligibility is checked
    /// against the record's current storage symbol: alias migration can
    /// change it while the specpdl reference keeps the original symbol.
    #[inline(always)]
    pub(crate) fn pop_default_binding(
        &mut self,
        id: OwnerRecordId,
        unwatched: impl FnOnce(SymId) -> bool,
    ) -> Option<SymId> {
        let record = self.record(id)?;
        if record.channel != Channel::Default {
            return None;
        }
        let state = match record.undo {
            UndoTarget::Previous(outer) => SlotState::Bound(outer),
            UndoTarget::Inherited { .. } => SlotState::NoBinding,
            UndoTarget::Captured(_) | UndoTarget::Retired => return None,
        };
        let (sym, index) = (record.sym, record.index);
        if !unwatched(sym) {
            return None;
        }
        // The record was validated above and nothing mutated it. Discard
        // the returned payload so only stack retirement is needed here.
        let _ = self.take_record(id);
        self.replace_default_slot(index, state);
        Some(sym)
    }

    /// Keep VALUES traced while the owner is parked.
    #[doc(hidden)]
    pub fn park(&mut self, values: Vec<Value>) {
        debug_assert!(self.parked.is_empty(), "an owner parks once");
        self.parked = values;
    }

    /// The owner runs again: its parked values are back in its specpdl.
    pub(crate) fn unpark(&mut self) {
        for value in std::mem::take(&mut self.parked) {
            note_dropped(Some(SlotValue::new(value)));
        }
    }

    /// This owner's own `debug-on-next-call` cell for the variable INDEX
    /// names, made on first use and holding the default-channel slot.
    pub(crate) fn debugger_cell(&mut self, index: TlsIndex) -> &'static LispBoolFwd {
        match self.debugger {
            Some((debugger, cell)) => {
                debug_assert_eq!(debugger, index, "one debug-on-next-call per owner");
                cell
            }
            None => {
                let armed = debugger_armed(self.slot(index, Channel::Default));
                let cell = neovm_value::forward::alloc_owner_debugger_cell(armed);
                self.debugger = Some((index, cell));
                cell
            }
        }
    }

    /// Make room for admission's commit to push RECORDS records, bind
    /// LOCALS buffer-local channels and fill the default-channel SLOTS
    /// without allocating: their pages are made now, empty.
    pub(crate) fn try_reserve_admission(
        &mut self,
        records: usize,
        locals: usize,
        slots: impl Iterator<Item = usize>,
    ) -> Result<(), std::collections::TryReserveError> {
        self.records.try_reserve(records)?;
        self.locals.try_reserve(locals)?;
        for slot in slots {
            let page = slot / SLOT_PAGE;
            if page >= self.pages.len() {
                self.pages.try_reserve(page + 1 - self.pages.len())?;
                self.pages.resize_with(page + 1, || None);
            }
            if self.pages[page].is_none() {
                let mut fresh = Vec::new();
                fresh.try_reserve_exact(SLOT_PAGE)?;
                fresh.resize(SLOT_PAGE, SlotState::NoBinding);
                let fresh: Box<[SlotState; SLOT_PAGE]> = fresh
                    .into_boxed_slice()
                    .try_into()
                    .expect("a page holds SLOT_PAGE slots");
                self.pages[page] = Some(fresh);
            }
        }
        Ok(())
    }

    /// A local of SYM was made again on CHANNEL and took VALUE: the
    /// outermost binding of SYM there whose local had been killed takes it
    /// back as its storage. Returns the value that binding hid, which the
    /// new local must hold instead (GNU's entry exchanges with the local it
    /// finds, `src/eval.c:3871-3883`, `:4204-4228`); `None` when no such
    /// binding exists.
    pub(crate) fn reattach_captured(
        &mut self,
        sym: SymId,
        index: TlsIndex,
        channel: Channel,
        value: Value,
    ) -> Option<SlotValue> {
        let outermost = self.canonical_root_record(sym, channel)?;
        let UndoTarget::Captured(hidden) = self.record(outermost)?.undo else {
            return None;
        };
        self.replace_undo(outermost, UndoTarget::Inherited { suspended: None });
        self.set_slot(index, channel, SlotState::Bound(SlotValue::new(value)));
        Some(hidden)
    }

    /// The active chain's link to canonical storage. An older chain can be
    /// suspended by `backtrace-eval` while a new binding starts another one.
    /// Killing/remaking the local must preserve this newer link's restore,
    /// rather than selecting the older suspended entry.
    pub(crate) fn canonical_root_record(
        &self,
        sym: SymId,
        channel: Channel,
    ) -> Option<OwnerRecordId> {
        self.record_ids_of(sym).into_iter().find(|id| {
            self.record(*id).is_some_and(|record| {
                record.channel == channel
                    && matches!(
                        record.undo,
                        UndoTarget::Inherited { suspended: None } | UndoTarget::Captured(_)
                    )
            })
        })
    }

    /// This owner's Green seat, if it runs in a Green runtime.
    pub(crate) fn green_seat_mut(&mut self) -> Option<&mut super::green::GreenSeat> {
        self.green.as_deref_mut()
    }

    /// This owner's Green seat for a read-only classification (owner vs
    /// main); no seat mutation or channel access.
    pub(crate) fn green_seat(&self) -> Option<&super::green::GreenSeat> {
        self.green.as_deref()
    }

    /// A Context's cold retirement breaks the seat's Baton -> World cycle
    /// and cancels any unused root-batch reservation before heap destruction.
    pub(crate) fn take_green_seat(&mut self) -> Option<Box<super::green::GreenSeat>> {
        self.green.take()
    }

    /// Seat this owner in a Green runtime (once).
    pub(crate) fn install_green_seat(&mut self, seat: Box<super::green::GreenSeat>) {
        debug_assert!(self.green.is_none(), "an owner takes one seat");
        self.green = Some(seat);
    }

    /// Whether this owner has its own `debug-on-next-call` cell.
    pub(crate) fn has_debugger_cell(&self) -> bool {
        self.debugger.is_some()
    }

    /// Take CELL, made before admission's commit, as this owner's
    /// `debug-on-next-call` cell for the variable INDEX names.
    pub(crate) fn adopt_debugger_cell(&mut self, index: TlsIndex, cell: &'static LispBoolFwd) {
        debug_assert!(self.debugger.is_none(), "one debug-on-next-call per owner");
        cell.set(debugger_armed(self.slot(index, Channel::Default)));
        self.debugger = Some((index, cell));
    }

    /// The page holding default-channel SLOT, made on first use.
    #[cold]
    #[inline(never)]
    pub(crate) fn new_page(&mut self, slot: usize) -> &mut [SlotState; SLOT_PAGE] {
        let page = slot / SLOT_PAGE;
        if page >= self.pages.len() {
            self.pages.resize_with(page + 1, || None);
        }
        self.pages[page].get_or_insert_with(|| Box::new([SlotState::NoBinding; SLOT_PAGE]))
    }

    /// Push a record and return its id.
    pub(crate) fn push_record(
        &mut self,
        sym: SymId,
        index: TlsIndex,
        kind: BindingKind,
        channel: Channel,
        undo: UndoTarget,
    ) -> OwnerRecordId {
        let generation = self.next_generation;
        self.next_generation = generation.checked_add(1).unwrap_or(NonZeroU32::MIN);
        self.records.push(Some(OwnerBindingRecord {
            sym,
            index,
            kind,
            channel,
            undo,
            generation,
        }));
        let depth = u32::try_from(self.records.len())
            .ok()
            .and_then(NonZeroU32::new)
            .expect("an owner binding stack deeper than u32");
        OwnerRecordId { depth, generation }
    }

    /// The live record ID names, if any.
    #[inline]
    #[doc(hidden)]
    pub fn record(&self, id: OwnerRecordId) -> Option<&OwnerBindingRecord> {
        self.records
            .get(id.position())?
            .as_ref()
            .filter(|record| record.generation == id.generation)
    }

    /// Replace the undo target of the live record ID names, noting the
    /// value it held. Returns the previous target.
    pub(crate) fn replace_undo(
        &mut self,
        id: OwnerRecordId,
        undo: UndoTarget,
    ) -> Option<UndoTarget> {
        let record = self
            .records
            .get_mut(id.position())?
            .as_mut()
            .filter(|record| record.generation == id.generation)?;
        let old = std::mem::replace(&mut record.undo, undo);
        let [value] = old.values();
        note_dropped(value);
        Some(old)
    }

    /// Take the record ID names off the stack: `None` if it is already gone.
    /// The caller owns the record's values from here on.
    #[inline]
    pub(crate) fn take_record(&mut self, id: OwnerRecordId) -> Option<OwnerBindingRecord> {
        if id.position() + 1 != self.records.len() {
            return self.take_record_below(id);
        }
        if self.records.last()?.as_ref()?.generation != id.generation {
            return None;
        }
        let record = self.records.pop()??;
        if self.records.last().is_some_and(Option::is_none) {
            self.trim_taken();
        }
        Some(record)
    }

    /// [`Self::take_record`] of a record with live records above it.
    #[cold]
    #[inline(never)]
    pub(crate) fn take_record_below(&mut self, id: OwnerRecordId) -> Option<OwnerBindingRecord> {
        let slot = self.records.get_mut(id.position())?;
        if slot.as_ref()?.generation != id.generation {
            return None;
        }
        slot.take()
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn trim_taken(&mut self) {
        while self.records.last().is_some_and(Option::is_none) {
            self.records.pop();
        }
    }

    /// The id of the innermost live record.
    ///
    /// [`Self::take_record`] of the top pops it and trims trailing holes, and
    /// a take below the top leaves the last slot live, so the last slot is
    /// the top whenever any record remains. O(1).
    pub(crate) fn top_record_id(&self) -> Option<OwnerRecordId> {
        let record = self.records.last()?.as_ref()?;
        let depth = u32::try_from(self.records.len())
            .ok()
            .and_then(NonZeroU32::new)?;
        Some(OwnerRecordId {
            depth,
            generation: record.generation,
        })
    }

    /// Take the record `id` names when it is the top, and nothing otherwise.
    ///
    /// A record below the top, or one already taken, leaves the stack as it
    /// was. [`Self::take_record`] of a record below the top takes it and
    /// leaves a hole, so the top check has to happen first.
    pub(crate) fn take_top_record(&mut self, id: OwnerRecordId) -> Option<OwnerBindingRecord> {
        let top = self.top_record_id()?;
        (top == id).then(|| self.take_record(id)).flatten()
    }

    /// The live records binding SYM, outermost first.
    pub(crate) fn records_of(&self, sym: SymId) -> impl Iterator<Item = &OwnerBindingRecord> {
        self.records
            .iter()
            .flatten()
            .filter(move |record| record.sym == sym)
    }

    /// The live records binding SYM, with their ids, outermost first.
    pub(crate) fn record_ids_of(&self, sym: SymId) -> Vec<OwnerRecordId> {
        self.live_records()
            .filter(|(_, record)| record.sym == sym)
            .map(|(id, _)| id)
            .collect()
    }

    /// Every live record with its id, outermost first.
    pub(crate) fn live_records(
        &self,
    ) -> impl Iterator<Item = (OwnerRecordId, &OwnerBindingRecord)> {
        self.records
            .iter()
            .enumerate()
            .filter_map(|(position, record)| {
                let record = record.as_ref()?;
                let depth = u32::try_from(position + 1).ok().and_then(NonZeroU32::new)?;
                Some((
                    OwnerRecordId {
                        depth,
                        generation: record.generation,
                    },
                    record,
                ))
            })
    }

    /// GNU `let_shadows_buffer_binding_p` (`src/eval.c:3593-3610`): a
    /// default binding of SYM made with BUFFER current.
    pub(crate) fn shadows_buffer_binding(&self, sym: SymId, buffer: Option<BufferId>) -> bool {
        buffer.is_some()
            && self
                .records_of(sym)
                .any(|record| record.kind == BindingKind::LetDefault { where_buf: buffer })
    }

    /// Visit every value the slab holds: each record's saved value and the
    /// current value of the channel it binds. Every bound slot belongs to a
    /// live record, so this visits each bound slot.
    #[doc(hidden)]
    pub fn trace_roots(&self, visit: &mut dyn FnMut(Value)) {
        for value in &self.parked {
            visit(*value);
        }
        for record in self.records.iter().flatten() {
            let [saved] = record.undo.values();
            if let Some(saved) = saved {
                visit(saved.get());
            }
            if let SlotState::Bound(current) = self.slot(record.index, record.channel) {
                visit(current.get());
            }
        }
        if let Some(seat) = &self.green {
            seat.trace_binding_resume_error(visit);
        }
    }
}

/// The World's owner slabs: allocation and the collector's root walk.
#[derive(Default)]
pub(crate) struct OwnerSlabRegistry {
    /// Allocations owned by this registry (`Box::into_raw`), freed on drop.
    slabs: Vec<NonNull<OwnerSlab>>,
    _thread_confined: PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(OwnerSlabRegistry: Send, Sync, Clone);

impl std::fmt::Debug for OwnerSlabRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnerSlabRegistry")
            .field("slabs", &self.slabs.len())
            .finish()
    }
}

impl OwnerSlabRegistry {
    /// Number of owner slabs registered in this World (0 while Green/admission
    /// is dormant).
    #[cfg(test)]
    pub(crate) fn len_for_test(&self) -> usize {
        self.slabs.len()
    }

    /// Allocate and register a slab for a new owner on HEAP.
    #[doc(hidden)]
    pub fn register(&mut self, heap: usize) -> NonNull<OwnerSlab> {
        self.slabs.reserve(1);
        let slab = NonNull::from(Box::leak(Box::new(OwnerSlab::new(heap))));
        self.slabs.push(slab);
        slab
    }

    /// Remove and deallocate a retiring owner's slab from this World's
    /// registry (E5.3b G5 Step 5).
    ///
    /// Returns `true` when `slab` was registered with no active parallel
    /// [`OwnerSlabLease`] and has now been removed and freed. Returns `false`
    /// when `slab` is not registered or a leaked lease still marks
    /// `parallel_lease_active` (leaving the allocation owned by this registry
    /// until `OwnerSlabRegistry::drop` to prevent use-after-free).
    ///
    /// # Safety
    /// - The caller must hold exclusive World authority (an active serial turn
    ///   ticket or the World's Baton) on this OS thread, excluding any
    ///   concurrent `OwnerSlabRegistry` access or collector root walk.
    /// - Both the retiring `Context` (whose `OwnerSlot` named `slab`) and any
    ///   [`OwnerSlabHandle`] minted for `slab` must already have been dropped
    ///   before this call so no live pointer can dereference `slab` afterward.
    pub(crate) unsafe fn unregister(&mut self, slab: NonNull<OwnerSlab>) -> bool {
        let Some(pos) = self.slabs.iter().position(|entry| *entry == slab) else {
            return false;
        };
        // SAFETY: `slab` is currently owned by `self.slabs`, so its allocation
        // is live; only the atomic `parallel_lease_active` flag is loaded here
        // without materializing `&OwnerSlab`.
        let lease_active = unsafe {
            (*slab.as_ptr())
                .parallel_lease_active
                .load(Ordering::Acquire)
        };
        if lease_active {
            return false;
        }
        let removed = self.slabs.swap_remove(pos);
        // SAFETY: `removed` was allocated via `Box::leak(Box::new(...))` in
        // `Self::register`, has just been removed from `self.slabs` so
        // `OwnerSlabRegistry::drop` will not free it again, has
        // `!parallel_lease_active`, and this method's `# Safety` preconditions
        // guarantee that exclusive World authority is held and no `Context` or
        // `OwnerSlabHandle` referencing `removed` survives.
        drop(unsafe { Box::from_raw(removed.as_ptr()) });
        true
    }

    /// Visit every registered slab's values (the collector's root phase).
    #[doc(hidden)]
    pub fn trace_roots(&self, heap: usize, visit: &mut dyn FnMut(Value)) {
        for slab in &self.slabs {
            // SAFETY (E4-P2 C1): the registry owns every registered allocation
            // until World drop. This legacy serial root walk runs only on a
            // serial World with no parallel `OwnerSlabLease` live (parallel
            // eras trace exclusively via `trace_roots_for_heap` under
            // `AllStopped`, after every `RunAccess` and its `OwnerSlabLease`
            // have ended). On the serial path, the owner mutates a slab only
            // through an exclusive `&mut Context` borrow, which this `&self`
            // root phase excludes on the owner's thread.
            debug_assert!(
                !unsafe {
                    (*slab.as_ptr())
                        .parallel_lease_active
                        .load(Ordering::Acquire)
                },
                "OwnerSlabRegistry::trace_roots observed an active parallel OwnerSlabLease"
            );
            let slab = unsafe { slab.as_ref() };
            debug_assert_eq!(slab.heap, heap, "an owner slab traced for another heap");
            slab.trace_roots(visit);
        }
    }

    /// Visit every registered slab's values under `AllStopped` in a parallel
    /// or stowed Green World, validating that `stopped` proves exclusive
    /// stopped-world access for `heap` before reading any slab.
    pub(crate) fn trace_roots_for_heap(
        &self,
        stopped: &neovm_value::tagged::gc::AllStopped<'_>,
        heap: neovm_value::tagged::gc::HeapIdentity,
        visit: &mut dyn FnMut(Value),
    ) -> Result<(), neovm_value::tagged::gc::RendezvousError> {
        stopped.check_world_roots(heap)?;
        for slab in &self.slabs {
            // SAFETY (E4-P2 C1): the registry owns every registered slab until
            // World drop, and `stopped.check_world_roots(heap)` proved
            // `AllStopped` excludes every `RunAccess` on `heap`. Every
            // `OwnerSlabLease` borrows `&mut RunAccess` (and `slab_mut`
            // borrows `&mut lease`), so no `&mut OwnerSlab` can coexist with
            // this stopped-world read — including if a lease was `mem::forget`ted
            // (moving `lease` into `forget` ends all slab references).
            let slab = unsafe { slab.as_ref() };
            debug_assert_eq!(
                neovm_value::tagged::gc::HeapIdentity::from_legacy_word(slab.heap),
                Some(heap),
                "an owner slab traced for another heap"
            );
            slab.trace_roots(visit);
        }
        Ok(())
    }
}

impl Drop for OwnerSlabRegistry {
    fn drop(&mut self) {
        for slab in self.slabs.drain(..) {
            // SAFETY: each pointer came from `Box::leak` in `register` and is
            // dropped exactly once, here; the Contexts that named it are gone
            // (each holds a World lease, so the World outlives them).
            drop(unsafe { Box::from_raw(slab.as_ptr()) });
        }
    }
}

/// A Context's owner slab, or none before it needs native owner state or
/// a projected variable binding. Occupies the former reserved opcode-epoch
/// word: same size, alignment and (absent) niche, so no Context offset moves.
#[repr(transparent)]
#[doc(hidden)]
pub struct OwnerSlot(Option<NonNull<OwnerSlab>>);

static_assertions::assert_eq_size!(OwnerSlot, std::cell::Cell<u64>);
static_assertions::assert_eq_align!(OwnerSlot, std::cell::Cell<u64>);
static_assertions::assert_not_impl_any!(OwnerSlot: Send, Sync, Clone);

impl std::fmt::Debug for OwnerSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("OwnerSlot").field(&self.0).finish()
    }
}

impl OwnerSlot {
    /// No owner slab: every Context starts so.
    pub(crate) const DORMANT: Self = Self(None);

    /// Install SLAB, which the World's registry owns.
    #[doc(hidden)]
    pub fn install(&mut self, slab: NonNull<OwnerSlab>) {
        debug_assert!(self.0.is_none(), "a Context owns one slab");
        self.0 = Some(slab);
    }

    /// Whether no native owner state or projected binding has needed a slab.
    #[inline(always)]
    #[doc(hidden)]
    pub fn is_dormant(&self) -> bool {
        self.0.is_none()
    }

    /// Raw pointer to this owner's slab allocation, if one was registered.
    /// Used solely by G5 Step 5 retirement to identify the slab to remove from
    /// `OwnerSlabRegistry` after this `Context` has been dropped.
    #[inline]
    pub(crate) fn raw_slab_ptr(&self) -> Option<NonNull<OwnerSlab>> {
        self.0
    }

    /// Whether a parallel [`OwnerSlabLease`] currently marks this slab active
    /// (checked via raw pointer without materializing a reference).
    #[inline]
    pub(crate) fn is_parallel_lease_active(&self) -> bool {
        self.0.is_some_and(|slab| {
            // SAFETY: the World's registry retains the allocation while this
            // Context holds its World lease; only the atomic flag is read.
            unsafe {
                (*slab.as_ptr())
                    .parallel_lease_active
                    .load(Ordering::Acquire)
            }
        })
    }

    /// The slab, for reading.
    #[inline]
    pub(crate) fn slab(&self) -> Option<&OwnerSlab> {
        // SAFETY (E4-P2 C1): the World's registry keeps the allocation alive
        // for as long as this Context holds its World lease. No parallel
        // `OwnerSlabLease` is active (structurally excluded by `DormantContext`
        // and `NativeStopped` during serial turns, and checked via raw pointer
        // before materializing `&OwnerSlab`), and this shared borrow excludes
        // `slab_mut`.
        self.0.map(|slab| {
            assert!(
                !unsafe {
                    (*slab.as_ptr())
                        .parallel_lease_active
                        .load(Ordering::Acquire)
                },
                "OwnerSlot accessed while a parallel OwnerSlabLease is active"
            );
            unsafe { slab.as_ref() }
        })
    }

    /// The slab, for this owner's writes.
    #[inline]
    pub(crate) fn slab_mut(&mut self) -> Option<&mut OwnerSlab> {
        // SAFETY (E4-P2 C1): as in `slab`; the exclusive borrow of the slot is
        // the serial owner's exclusive access. No parallel `OwnerSlabLease` is
        // active (structurally excluded by `DormantContext` and `NativeStopped`
        // during serial turns, and checked via raw pointer before materializing
        // `&mut OwnerSlab`), and the collector reads slabs only in a serial
        // root phase or under `AllStopped`, neither of which overlaps a live
        // `&mut Context` borrow.
        self.0.map(|mut slab| {
            assert!(
                !unsafe {
                    (*slab.as_ptr())
                        .parallel_lease_active
                        .load(Ordering::Acquire)
                },
                "OwnerSlot accessed while a parallel OwnerSlabLease is active"
            );
            unsafe { slab.as_mut() }
        })
    }
}

/// Why an [`OwnerSlabHandle`] or [`OwnerSlabLease`] could not be minted
/// (E4-P2 C1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerSlabLeaseError {
    /// This owner already handed out its unique parallel [`OwnerSlabHandle`].
    HandleAlreadyIssued,
    /// The slab's legacy heap word is not a valid parallel [`HeapIdentity`].
    InvalidHeapIdentity,
    /// The [`RunAccess`](neovm_value::tagged::gc::RunAccess) belongs to a
    /// different heap than the slab.
    WrongHeap,
    /// A prior [`OwnerSlabLease`] on this slab is still marked active (for
    /// example because it was leaked with `std::mem::forget`).
    ActiveLease,
}

/// Unique, thread-confined capability to lease one owner's [`OwnerSlab`]
/// during an admitted [`RunAccess`](neovm_value::tagged::gc::RunAccess)
/// stretch (E4-P2 C1).
///
/// Extracted once from a `Context` before moving it into `OwnerSerialState::new`,
/// preserving E5.1's zero-accessor `DormantContext` invariant. Holding this
/// handle never dereferences the slab; exclusive access exists only through
/// [`Self::lease`], whose [`OwnerSlabLease`] borrows `&mut RunAccess` for its
/// entire lifetime.
pub struct OwnerSlabHandle {
    slab: NonNull<OwnerSlab>,
    heap: neovm_value::tagged::gc::HeapIdentity,
    _thread_confined: PhantomData<std::rc::Rc<()>>,
}

static_assertions::assert_not_impl_any!(OwnerSlabHandle: Send, Sync, Clone, Copy);

impl std::fmt::Debug for OwnerSlabHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnerSlabHandle")
            .field("heap", &self.heap)
            .finish_non_exhaustive()
    }
}

#[cfg_attr(
    not(any(test, feature = "contract-fixtures")),
    expect(
        dead_code,
        reason = "dormant E4-P2 C1; Slice 2 CanonicalPlain wires this"
    )
)]
impl OwnerSlabHandle {
    /// The parallel heap identity that owns this slab.
    #[inline]
    pub fn heap_identity(&self) -> neovm_value::tagged::gc::HeapIdentity {
        self.heap
    }

    /// Mint an exclusive [`OwnerSlabLease`] anchored to an admitted
    /// [`RunAccess`](neovm_value::tagged::gc::RunAccess) stretch.
    ///
    /// While the returned lease is live:
    /// - `run` (and therefore its `Mutator`) is mutably borrowed, so
    ///   `Mutator::poll`, `Mutator::enter_native`, and dropping `run` are
    ///   rejected at compile time (`E0499`/`E0505`);
    /// - `self` is mutably borrowed, so a second lease on the same handle is
    ///   rejected at compile time (`E0499`);
    /// - `RunAccess` holds `AdmissionWord::IN_SLICE` with the Dekker exit
    ///   fence, so the collector cannot reach `AllStopped` or call
    ///   `OwnerSlabRegistry::trace_roots_for_heap` until the lease and its
    ///   `RunAccess` drop and the owner ACKs the stop.
    pub fn lease<'run, 'owner, 'era>(
        &'run mut self,
        run: &'run mut neovm_value::tagged::gc::RunAccess<'owner, 'era>,
    ) -> Result<OwnerSlabLease<'run, 'owner, 'era>, OwnerSlabLeaseError> {
        if run.heap_identity() != self.heap {
            return Err(OwnerSlabLeaseError::WrongHeap);
        }
        // SAFETY: `run.heap_identity() == self.heap` proves the World's
        // `EraWorldLoan` (and therefore its `OwnerSlabRegistry`, which retains
        // every registered `Box::leak`ed `OwnerSlab` until World drop) is alive
        // for all of `'era`. We access only the atomic flag through a raw
        // pointer before creating any `&OwnerSlab`/`&mut OwnerSlab`.
        let acquired = unsafe {
            (*self.slab.as_ptr())
                .parallel_lease_active
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        };
        if !acquired {
            return Err(OwnerSlabLeaseError::ActiveLease);
        }
        Ok(OwnerSlabLease {
            slab: self.slab,
            run,
            _handle: PhantomData,
            _thread_confined: PhantomData,
        })
    }
}

/// Exclusive parallel lease of one owner's [`OwnerSlab`], anchored to an
/// admitted [`RunAccess`](neovm_value::tagged::gc::RunAccess) stretch
/// (E4-P2 C1).
///
/// The lease cannot cross a safepoint poll (`Mutator::poll`), a native-stop
/// boundary (`Mutator::enter_native`), or `RunAccess` drop, and cannot cross
/// threads (`!Send`, `!Sync`).
#[must_use = "drop the owner slab lease before polling or entering a serial turn"]
pub struct OwnerSlabLease<'run, 'owner, 'era> {
    slab: NonNull<OwnerSlab>,
    run: &'run mut neovm_value::tagged::gc::RunAccess<'owner, 'era>,
    _handle: PhantomData<&'run mut OwnerSlabHandle>,
    _thread_confined: PhantomData<std::rc::Rc<()>>,
}

static_assertions::assert_not_impl_any!(
    OwnerSlabLease<'static, 'static, 'static>: Send, Sync, Clone, Copy
);

impl std::fmt::Debug for OwnerSlabLease<'_, '_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnerSlabLease")
            .field("heap", &self.run.heap_identity())
            .finish_non_exhaustive()
    }
}

impl Drop for OwnerSlabLease<'_, '_, '_> {
    fn drop(&mut self) {
        // SAFETY: `self.slab` stays valid for `'era` (proven by `self.run`).
        // Clear through the raw pointer without forming `&mut OwnerSlab`.
        unsafe {
            (*self.slab.as_ptr())
                .parallel_lease_active
                .store(false, Ordering::Release);
        }
    }
}

impl<'owner, 'era> OwnerSlabLease<'_, 'owner, 'era> {
    /// The parallel heap identity of this leased slab and run stretch.
    #[inline]
    #[cfg_attr(
        not(any(test, feature = "contract-fixtures")),
        expect(
            dead_code,
            reason = "dormant E4-P2 C1; Slice 2 CanonicalPlain wires this"
        )
    )]
    pub fn heap_identity(&self) -> neovm_value::tagged::gc::HeapIdentity {
        self.run.heap_identity()
    }

    /// Read the leased [`OwnerSlab`]. The returned reference cannot outlive
    /// this lease.
    #[inline]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "dormant E4-P2 C1; Slice 2 CanonicalPlain wires this"
        )
    )]
    pub(crate) fn slab(&self) -> &OwnerSlab {
        // SAFETY: `self` owns the unique active parallel lease for this slab
        // on a live `'era` `RunAccess`, excluding both `AllStopped` root scans
        // and any `OwnerSlot` reference.
        unsafe { self.slab.as_ref() }
    }

    /// Mutate the leased [`OwnerSlab`]. The returned reference cannot outlive
    /// this lease.
    #[inline]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "dormant E4-P2 C1; Slice 2 CanonicalPlain wires this"
        )
    )]
    pub(crate) fn slab_mut(&mut self) -> &mut OwnerSlab {
        // SAFETY: as in `slab`; `&mut self` grants exclusive access for the
        // duration of the returned borrow.
        unsafe { self.slab.as_mut() }
    }

    /// Read the underlying [`RunAccess`](neovm_value::tagged::gc::RunAccess).
    #[inline]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "dormant E4-P2 C1; Slice 2 CanonicalPlain wires this"
        )
    )]
    pub(crate) fn run_access(&self) -> &neovm_value::tagged::gc::RunAccess<'owner, 'era> {
        self.run
    }

    /// Mutably borrow the underlying
    /// [`RunAccess`](neovm_value::tagged::gc::RunAccess) for same-stretch
    /// authority calls (such as Slice 2's canonical symbol seam).
    #[inline]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "dormant E4-P2 C1; Slice 2 CanonicalPlain wires this"
        )
    )]
    pub(crate) fn run_access_mut(
        &mut self,
    ) -> &mut neovm_value::tagged::gc::RunAccess<'owner, 'era> {
        self.run
    }

    /// Split-borrow the leased [`OwnerSlab`] and its anchoring
    /// [`RunAccess`](neovm_value::tagged::gc::RunAccess) simultaneously.
    #[inline]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "dormant E4-P2 C1; Slice 2 CanonicalPlain wires this"
        )
    )]
    pub(crate) fn parts_mut(
        &mut self,
    ) -> (
        &mut OwnerSlab,
        &mut neovm_value::tagged::gc::RunAccess<'owner, 'era>,
    ) {
        // SAFETY: `self.slab` is disjoint from `self.run` (`RunAccess` holds
        // GC registration state, never an `OwnerSlab` reference) and `&mut self`
        // bounds both returned references to this lease.
        let slab = unsafe { self.slab.as_mut() };
        (slab, self.run)
    }
}

/// Whether BUFFER holds a local binding of a per-buffer variable.
#[inline]
pub(super) fn per_buffer_local(
    buffer: &neovm_edit::buffer::Buffer,
    buf_fwd: &LispBufferObjFwd,
) -> bool {
    buf_fwd.local_flags_idx < 0
        || buf_fwd
            .buffer_slot()
            .is_some_and(|slot| buffer.slot_local_flag(slot))
}

/// GNU `specbind`'s choice of entry for the variable PROJECTION wraps
/// (`src/eval.c:3656-3715`), and the channel the binding shadows. The same
/// channel is the one a read or an assignment in the current buffer meets
/// (`find_symbol_value`, `set_internal`).
pub(crate) fn binding_shape(
    buffers: &BufferManager,
    id: SymId,
    projection: &ThreadProjectionDescriptor,
) -> (BindingKind, Channel) {
    let arm = projection.arm();
    if arm == CanonicalArm::Plain {
        return (BindingKind::Let, Channel::Default);
    }
    let current = buffers.current_buffer();
    match arm {
        CanonicalArm::Plain => (BindingKind::Let, Channel::Default),
        CanonicalArm::Localized(_) => match current {
            Some(buffer) if buffer.local_variable_binding_cell(id).is_some() => {
                (BindingKind::LetLocal, Channel::BufferLocal(buffer.id()))
            }
            Some(buffer) => (
                BindingKind::LetDefault {
                    where_buf: Some(buffer.id()),
                },
                Channel::Default,
            ),
            None => (BindingKind::Let, Channel::Default),
        },
        CanonicalArm::Forwarded(builtin) => match builtin.descriptor().as_buffer_obj_fwd() {
            Some(buf_fwd) => match current {
                Some(buffer) if per_buffer_local(buffer, buf_fwd) => {
                    (BindingKind::LetLocal, Channel::BufferLocal(buffer.id()))
                }
                _ => (
                    BindingKind::LetDefault {
                        where_buf: current.map(|buffer| buffer.id()),
                    },
                    Channel::Default,
                ),
            },
            None => (BindingKind::Let, Channel::Default),
        },
    }
}

/// The channel a read or assignment of the variable PROJECTION wraps meets
/// in the current buffer.
#[inline]
pub(crate) fn access_channel(
    buffers: &BufferManager,
    id: SymId,
    projection: &ThreadProjectionDescriptor,
) -> Channel {
    binding_shape(buffers, id, projection).1
}

/// Where an assignment (GNU `set_internal`, `src/data.c:1678-1831`) of the
/// variable PROJECTION wraps lands for this owner: the channel of the
/// owner's binding, or `None` for canonical storage.
///
/// A `set` of an automatically buffer-local variable with no local binding
/// in the current buffer makes one (canonical) unless a default binding of
/// this owner made in this buffer shadows it (`src/data.c:1756-1782`,
/// `:1810-1823`); a `let` or its unwind never makes one.
pub(crate) fn owner_store_channel(
    slab: &OwnerSlab,
    buffers: &BufferManager,
    id: SymId,
    projection: &ThreadProjectionDescriptor,
    bindflag: SetInternalBind,
) -> Option<Channel> {
    let channel = access_channel(buffers, id, projection);
    if channel == Channel::Default
        && bindflag == SetInternalBind::Set
        && set_makes_local(projection)
        && buffers.current_buffer_id().is_some()
        && !slab.shadows_buffer_binding(id, buffers.current_buffer_id())
    {
        return None;
    }
    slab.slot(projection.index(), channel)
        .value()
        .map(|_| channel)
}

/// Whether a `set` with no local binding makes one: a local-if-set variable
/// or a per-buffer variable with a local flag.
fn set_makes_local(projection: &ThreadProjectionDescriptor) -> bool {
    match projection.arm() {
        CanonicalArm::Plain => false,
        // SAFETY: a projection's localized arm names a live record of the
        // obarray's BLV pool; one flag is read.
        CanonicalArm::Localized(blv) => unsafe { (*blv.blv().as_ptr()).local_if_set },
        CanonicalArm::Forwarded(builtin) => builtin
            .descriptor()
            .as_buffer_obj_fwd()
            .is_some_and(|buf_fwd| buf_fwd.local_flags_idx >= 0),
    }
}

impl Context {
    /// Copy this owner's native write policy without activating an owner slab.
    /// GNU's static auto-save mode bits begin at zero; a dormant owner has
    /// exactly that ordinary state. Independent owners of one World do not
    /// share an active auto-save operation.
    #[doc(hidden)]
    pub fn auto_save_native_write_policy(&self) -> AutoSaveNativeWritePolicy {
        self.owner.slab().map_or(
            AutoSaveNativeWritePolicy::Ordinary { preserved_mode: 0 },
            |slab| slab.native_fileio,
        )
    }

    /// Replace only this owner's native file-I/O policy, returning its prior
    /// value. The system file-I/O scope owns GNU's entry/exit transitions;
    /// in particular, nested exit clears Active while preserving the CURRENT
    /// mode, rather than restoring the state returned by this method.
    /// No slab borrow is retained while a caller invokes Lisp or native I/O.
    #[doc(hidden)]
    pub fn replace_auto_save_native_write_policy(
        &mut self,
        policy: AutoSaveNativeWritePolicy,
    ) -> AutoSaveNativeWritePolicy {
        std::mem::replace(&mut self.ensure_owner_slab().native_fileio, policy)
    }

    /// Copy this owner's directory-creation umask without activating an owner
    /// slab. GNU's static `auto_saving_dir_umask` begins at zero; a dormant
    /// owner has exactly that state, so `make-directory-internal` outside the
    /// list-file window always sees the ordinary umask.
    #[doc(hidden)]
    pub fn auto_saving_dir_umask(&self) -> u32 {
        self.owner
            .slab()
            .map_or(0, |slab| slab.native_fileio_dir_umask)
    }

    /// Replace only this owner's directory-creation umask, returning its
    /// prior value. The system file-I/O scope owns GNU's set/clear window:
    /// `do_auto_save_make_dir` sets `077` around the funcall and both the
    /// normal return and the error arm clear it, rather than restoring a
    /// saved value. No slab borrow is retained while a caller invokes Lisp.
    #[doc(hidden)]
    pub fn replace_auto_saving_dir_umask(&mut self, umask: u32) -> u32 {
        std::mem::replace(&mut self.ensure_owner_slab().native_fileio_dir_umask, umask)
    }

    /// This owner's slab, made and registered with the World on first use.
    pub(crate) fn ensure_owner_slab(&mut self) -> &mut OwnerSlab {
        if self.owner.slab().is_none() {
            let slab = self.register_owner_slab();
            self.owner.install(slab);
        }
        self.owner
            .slab_mut()
            .expect("the owner slab was just installed")
    }

    /// Extract this owner's unique parallel [`OwnerSlabHandle`] (E4-P2 C1),
    /// allocating and registering the slab in the World if it is still dormant.
    ///
    /// Call once before moving the `Context` into `OwnerSerialState::new` so
    /// E5.1's zero-accessor `DormantContext` stays untouched. A second call on
    /// the same owner refuses with
    /// [`OwnerSlabLeaseError::HandleAlreadyIssued`].
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "dormant E4-P2 C1; Slice 2 CanonicalPlain and E6-P wire this"
        )
    )]
    pub(crate) fn take_parallel_owner_slab_handle(
        &mut self,
    ) -> Result<OwnerSlabHandle, OwnerSlabLeaseError> {
        let heap = {
            let slab = self.ensure_owner_slab();
            if slab.parallel_handle_issued {
                return Err(OwnerSlabLeaseError::HandleAlreadyIssued);
            }
            let heap = neovm_value::tagged::gc::HeapIdentity::from_legacy_word(slab.heap)
                .ok_or(OwnerSlabLeaseError::InvalidHeapIdentity)?;
            slab.parallel_handle_issued = true;
            heap
        };
        let slab = self
            .owner
            .0
            .expect("ensure_owner_slab installed the owner slab");
        Ok(OwnerSlabHandle {
            slab,
            heap,
            _thread_confined: PhantomData,
        })
    }

    /// GNU `specbind` (`src/eval.c:3656-3715`) of the projected variable
    /// RESOLVED: record the binding, run the `let` watchers, then store on
    /// the channel they left (`do_specbind` → `set_internal`). The binding
    /// is this owner's; canonical storage does not change.
    pub(crate) fn specbind_owner(
        &mut self,
        resolved: SymId,
        projection: &'static ThreadProjectionDescriptor,
        value: Value,
    ) -> Result<(), Flow> {
        // An unwatched plain projection always binds the default channel:
        // no type rule, buffer lookup, or post-watcher routing is needed.
        if projection.arm() == CanonicalArm::Plain && !self.watchers.has_watchers(resolved) {
            let index = projection.index();
            let slab = self.ensure_owner_slab();
            let old = slab.replace_default_slot(index, SlotState::Bound(SlotValue::new(value)));
            let undo = match old {
                SlotState::Bound(outer) => UndoTarget::Previous(outer),
                SlotState::NoBinding => UndoTarget::Inherited { suspended: None },
            };
            // Native page/Vec growth cannot execute Lisp, reach a GC
            // safepoint, or hand off the Baton. Once pushed, the record
            // roots the new slot and any prior binding it must restore.
            let record =
                slab.push_record(resolved, index, BindingKind::Let, Channel::Default, undo);
            self.push_specpdl_with(|| SpecBinding::NativeUnwind {
                action: NativeUnwindAction::OwnerBinding(OwnerBindingRef {
                    sym: resolved,
                    record,
                }),
            });
            self.publish_owner_binding(resolved);
            return Ok(());
        }
        let (kind, channel) = binding_shape(&self.edit.buffers, resolved, projection);
        // A buffer's own per-buffer slot binds in place (design §1.3b).
        if let (
            BindingKind::LetLocal,
            Channel::BufferLocal(buffer),
            CanonicalArm::Forwarded(builtin),
        ) = (kind, channel, projection.arm())
        {
            return self.specbind_slot_in_place(resolved, builtin.descriptor(), buffer, value);
        }
        // So does its default, in every buffer without a local (§1.3b).
        if let (BindingKind::LetDefault { where_buf }, CanonicalArm::Forwarded(builtin)) =
            (kind, projection.arm())
            && let Some(buf_fwd) = builtin.descriptor().as_buffer_obj_fwd()
        {
            return self.specbind_default_in_place(resolved, buf_fwd, where_buf, value);
        }
        let index = projection.index();
        let slab = self.ensure_owner_slab();
        let undo = match slab.slot(index, channel) {
            SlotState::Bound(outer) => UndoTarget::Previous(outer),
            SlotState::NoBinding => UndoTarget::Inherited { suspended: None },
        };
        let record = slab.push_record(resolved, index, kind, channel, undo);
        self.push_specpdl_with(|| SpecBinding::NativeUnwind {
            action: NativeUnwindAction::OwnerBinding(OwnerBindingRef {
                sym: resolved,
                record,
            }),
        });
        let watched = self.watchers.has_watchers(resolved);
        if watched {
            self.run_specbind_watcher(resolved, value, "let")?;
        }
        // A plain variable has no type rule (GNU's PLAINVAL store).
        let stored = match projection.logical_forwarder() {
            None => value,
            Some(_) => super::check_forwarded_store_at(
                &self.obarray,
                &self.edit.buffers,
                &self.specpdl,
                resolved,
                value,
                super::ForwardStoreSite::Bind,
            )?
            .value(),
        };
        // A watcher may have localized the variable or bound another
        // channel: the value lands where GNU's `set_internal` would put it.
        if watched
            && !(self
                .obarray
                .thread_projection_of(resolved)
                .is_some_and(|now| std::ptr::eq(now, projection))
                && access_channel(&self.edit.buffers, resolved, projection) == channel)
        {
            return self.set_internal_after_watchers(resolved, value, SetInternalBind::Bind);
        }
        if let Some(slab) = self.owner.slab_mut() {
            slab.set_slot(index, channel, SlotState::Bound(SlotValue::new(stored)));
        }
        self.publish_owner_binding(resolved);
        Ok(())
    }

    /// GNU `specbind`'s `SPECPDL_LET_LOCAL` of a per-buffer slot the current
    /// BUFFER holds (`src/eval.c:3641-3677`), for a projected variable: the
    /// binding lives in the slot, a shallow binding as GNU's, recorded by a
    /// `LetLocal` entry. A buffer's per-buffer slots always hold the
    /// effective value of the owner running in it, so every `BVAR` reader
    /// stays a slot load; an owner leaving the baton parks these bindings
    /// (design §1.3b).
    #[cold]
    #[inline(never)]
    pub(crate) fn specbind_slot_in_place(
        &mut self,
        resolved: SymId,
        logical: neovm_value::forward::FwdRef,
        buffer: BufferId,
        value: Value,
    ) -> Result<(), Flow> {
        let slot = logical
            .as_buffer_obj_fwd()
            .and_then(LispBufferObjFwd::buffer_slot)
            .expect("a per-buffer forwarder names a slot");
        let off = slot.index();
        let old_value = self
            .edit
            .buffers
            .get(buffer)
            .map_or(Value::NIL, |buf| buf.slots()[off]);
        self.push_specpdl_with(|| SpecBinding::LetLocal {
            sym_id: resolved,
            old_value,
            buffer_id: buffer,
        });
        self.run_specbind_watcher(resolved, value, "let")?;
        let stored = match logical.store(value) {
            Ok(store) => store.canonical_value(),
            Err(error) => return Err(super::forward_store_signal(error, value)),
        };
        if let Some(buf) = self.edit.buffers.get_mut(buffer) {
            buf.slots_mut()[off] = stored;
        }
        Ok(())
    }

    /// GNU `specbind`'s `SPECPDL_LET_DEFAULT` of a per-buffer variable the
    /// current buffer has no local of (`src/eval.c:3678-3700`), for a
    /// projected variable: `set_default` in place, so every buffer without
    /// a local -- and every direct slot reader in it -- sees the binding, as
    /// GNU's shared default does. The legacy entry restores the default at
    /// its pop; an owner leaving the baton swaps it out (design §1.3b).
    #[cold]
    #[inline(never)]
    pub(crate) fn specbind_default_in_place(
        &mut self,
        resolved: SymId,
        buf_fwd: &LispBufferObjFwd,
        where_buf: Option<BufferId>,
        value: Value,
    ) -> Result<(), Flow> {
        let old_default = buf_fwd
            .buffer_slot()
            .map(|slot| slot.index())
            .filter(|&off| off < self.edit.buffers.buffer_defaults.len())
            .map_or(buf_fwd.default, |off| {
                self.edit.buffers.buffer_defaults[off]
            });
        self.push_specpdl_with(|| SpecBinding::LetDefault {
            sym_id: resolved,
            old_value: super::SavedBindingValue::from_plain(old_default),
            buffer_id: super::SavedBufferId::from_option(where_buf),
        });
        super::super::data::set_default_internal_resolved(
            self,
            resolved,
            value,
            SetInternalBind::Bind,
        )?;
        Ok(())
    }

    /// Republish host mirrors of RESOLVED after an owner binding changed.
    #[inline(always)]
    pub(crate) fn publish_owner_binding(&mut self, resolved: SymId) {
        if resolved == super::owner_mirrors::debug_on_next_call_symbol() {
            self.point_owner_debugger();
        }
        // The mask includes the GC settings, so an unmirrored variable needs
        // neither publication nor the GC-binding sync.
        if self.runtime_binding_has_projection(resolved) {
            self.publish_owner_binding_mirrors(resolved);
        }
    }

    /// Keep the value lookup and publication frame outside the ordinary
    /// unmirrored bind/pop path, whose inline gate is only a bit test.
    #[cold]
    #[inline(never)]
    pub(crate) fn publish_owner_binding_mirrors(&mut self, resolved: SymId) {
        let visible = match self.find_symbol_value_by_id(resolved) {
            Ok(super::SymbolValueLookup::Bound(value)) => value,
            _ => Value::NIL,
        };
        self.publish_runtime_binding_write_by_resolved_id(resolved, visible);
        self.sync_user_test_gc_binding_by_id(resolved);
    }

    /// GNU `do_one_unbind` (`src/eval.c:3806-3885`) of an owner binding.
    ///
    /// A nested binding gives the owner's outer binding back. The outermost
    /// one leaves the owner unbound, so canonical storage shows through. With
    /// watchers, GNU notifies `unlet` while the binding is still in the shared
    /// cell and the entry already off the stack: the binding is visible to
    /// every thread until the restore, and a watcher that signals leaves it
    /// there. The binding is published to canonical storage for that window,
    /// then the restore runs through the paths a `Let` entry takes.
    #[cold]
    #[inline(never)]
    pub(crate) fn unbind_owner_binding(&mut self, binding: OwnerBindingRef) -> Result<(), Flow> {
        let restored = {
            let watchers = &self.watchers;
            self.owner.slab_mut().and_then(|slab| {
                slab.pop_default_binding(binding.record, |sym| !watchers.has_watchers(sym))
            })
        };
        if let Some(sym) = restored {
            self.publish_owner_binding(sym);
            return Ok(());
        }
        self.unbind_owner_binding_slow(binding)
    }

    /// Full-record restoration is needed only for watched, local, captured
    /// or retired bindings. Keep its larger frame off the common pop.
    #[cold]
    #[inline(never)]
    pub(crate) fn unbind_owner_binding_slow(
        &mut self,
        binding: OwnerBindingRef,
    ) -> Result<(), Flow> {
        let Some(record) = self
            .owner
            .slab_mut()
            .and_then(|slab| slab.take_record(binding.record))
        else {
            return Ok(());
        };
        let watched = self.watchers.has_watchers(record.sym);
        // The entry wrapper already consumed every unwatched default
        // Previous/Inherited binding. Only full-record cases reach here.
        let Some(projection) = self.obarray.thread_projection_of(record.sym) else {
            // `defvaralias` retired the projection; the alias resolves to its
            // base, which holds no binding of this owner.
            return self.unbind_retired_owner_record(record);
        };
        if !self.owner_channel_live(record.sym, projection, record.channel) {
            // GNU restores a buffer-local binding only while the buffer still
            // has the local (`Flocal_variable_p`, `src/eval.c:3871-3883`).
            note_dropped(record.undo.values()[0]);
            self.clear_owner_slot(record.index, record.channel);
            return Ok(());
        }
        match record.undo {
            UndoTarget::Retired => Ok(()),
            UndoTarget::Previous(outer) if !watched => {
                if let Some(slab) = self.owner.slab_mut() {
                    slab.set_slot(record.index, record.channel, SlotState::Bound(outer));
                }
                self.publish_owner_binding(record.sym);
                Ok(())
            }
            UndoTarget::Previous(outer) => {
                self.push_vm_frame_root(outer.get());
                self.restore_owner_binding(&record, outer.get())
            }
            UndoTarget::Inherited { .. } if !watched => {
                self.clear_owner_slot(record.index, record.channel);
                self.publish_owner_binding(record.sym);
                Ok(())
            }
            UndoTarget::Inherited { .. } => {
                let restore = self.owner_canonical_value(record.sym, record.channel);
                self.push_vm_frame_root(restore);
                self.publish_owner_slot(&record);
                self.restore_owner_binding(&record, restore)
            }
            UndoTarget::Captured(saved) if !watched => {
                self.clear_owner_slot(record.index, record.channel);
                self.owner_canonical_store(record.sym, record.channel, saved.get());
                self.publish_owner_binding(record.sym);
                Ok(())
            }
            UndoTarget::Captured(saved) => {
                self.push_vm_frame_root(saved.get());
                self.publish_owner_slot(&record);
                self.restore_owner_binding(&record, saved.get())
            }
        }
    }

    /// A record whose variable is no longer projected: restore what it saved
    /// through the variable's own paths.
    #[cold]
    pub(crate) fn unbind_retired_owner_record(
        &mut self,
        record: OwnerBindingRecord,
    ) -> Result<(), Flow> {
        match record.undo {
            UndoTarget::Previous(saved) | UndoTarget::Captured(saved) => {
                self.push_vm_frame_root(saved.get());
                self.set_internal_after_watchers(record.sym, saved.get(), SetInternalBind::Unbind)
            }
            UndoTarget::Inherited { .. } | UndoTarget::Retired => Ok(()),
        }
    }

    /// Move the owner's current binding of RECORD's channel into canonical
    /// storage and leave the owner unbound there: GNU's shared cell holding
    /// a popped binding while its `unlet` watchers run.
    pub(crate) fn publish_owner_slot(&mut self, record: &OwnerBindingRecord) {
        let current = self
            .owner
            .slab()
            .and_then(|slab| slab.slot(record.index, record.channel).value());
        if let Some(current) = current {
            self.push_vm_frame_root(current.get());
            self.owner_canonical_store(record.sym, record.channel, current.get());
        }
        self.clear_owner_slot(record.index, record.channel);
    }

    /// GNU's notify-then-store of VALUE for RECORD's kind of entry, through
    /// the same paths a `Let`, `LetDefault` or `LetLocal` pop takes; the
    /// stores land in this owner's slot when it still binds the channel.
    pub(crate) fn restore_owner_binding(
        &mut self,
        record: &OwnerBindingRecord,
        value: Value,
    ) -> Result<(), Flow> {
        let plain = self
            .obarray
            .thread_projection_of(record.sym)
            .is_some_and(|projection| projection.arm() == CanonicalArm::Plain);
        match (record.kind, record.channel) {
            (BindingKind::LetLocal, Channel::BufferLocal(buffer)) => {
                self.run_variable_watchers_by_id_with_where(
                    record.sym,
                    &value,
                    &Value::NIL,
                    "unlet",
                    &Value::make_buffer(buffer),
                )?;
                self.store_owner_channel(record, value);
                Ok(())
            }
            (BindingKind::Let, _) if plain => {
                let reported = if value.is_unbound() {
                    Value::NIL
                } else {
                    value
                };
                self.run_variable_watchers_by_id(record.sym, &reported, &Value::NIL, "unlet")?;
                self.set_internal_after_watchers(record.sym, value, SetInternalBind::Unbind)
            }
            _ => self.restore_default_binding_by_id(
                record.sym,
                SlotValue::new(value).bound(),
                SetInternalBind::Unbind,
            ),
        }
    }

    /// Store VALUE on RECORD's channel without watchers: the owner's slot
    /// while it binds the channel, canonical storage otherwise.
    pub(crate) fn store_owner_channel(&mut self, record: &OwnerBindingRecord, value: Value) {
        let bound = self
            .owner
            .slab()
            .is_some_and(|slab| slab.slot(record.index, record.channel).value().is_some());
        if bound {
            if let Some(slab) = self.owner.slab_mut() {
                slab.set_slot(
                    record.index,
                    record.channel,
                    SlotState::Bound(SlotValue::new(value)),
                );
            }
        } else {
            self.owner_canonical_store(record.sym, record.channel, value);
        }
        self.publish_owner_binding(record.sym);
    }

    pub(crate) fn clear_owner_slot(&mut self, index: TlsIndex, channel: Channel) {
        if let Some(slab) = self.owner.slab_mut() {
            slab.set_slot(index, channel, SlotState::NoBinding);
        }
    }

    /// Whether CHANNEL still names storage of the variable: a buffer-local
    /// channel only while its buffer has the local.
    pub(crate) fn owner_channel_live(
        &self,
        id: SymId,
        projection: &ThreadProjectionDescriptor,
        channel: Channel,
    ) -> bool {
        let Channel::BufferLocal(buffer) = channel else {
            return true;
        };
        let Some(buffer) = self.edit.buffers.get(buffer) else {
            return false;
        };
        match projection.arm() {
            CanonicalArm::Plain => false,
            CanonicalArm::Localized(_) => buffer.local_variable_binding_cell(id).is_some(),
            CanonicalArm::Forwarded(builtin) => builtin
                .descriptor()
                .as_buffer_obj_fwd()
                .is_some_and(|buf_fwd| per_buffer_local(buffer, buf_fwd)),
        }
    }

    /// The canonical value of the projected variable ID on CHANNEL
    /// ([`Value::UNBOUND`] when void): what every owner without a binding
    /// there sees.
    pub(crate) fn owner_canonical_value(&mut self, id: SymId, channel: Channel) -> Value {
        self.with_revealed_projection(id, |ctx| match channel {
            Channel::Default => super::super::data::default_value_by_id(ctx, id),
            Channel::BufferLocal(buffer) => ctx.local_binding_value_for_thread_switch(id, buffer),
        })
        .unwrap_or(Value::UNBOUND)
    }

    /// Store VALUE in the canonical storage of the projected variable ID on
    /// CHANNEL, below watchers and type signals (a value the storage refuses
    /// leaves it unchanged), as GNU's specpdl swaps do.
    pub(crate) fn owner_canonical_store(&mut self, id: SymId, channel: Channel, value: Value) {
        self.with_revealed_projection(id, |ctx| match channel {
            Channel::Default => {
                ctx.discard_restore_default_binding(id, SlotValue::new(value).bound());
            }
            Channel::BufferLocal(buffer) => {
                if ctx
                    .local_binding_value_for_thread_switch(id, buffer)
                    .is_some()
                {
                    ctx.set_local_binding_for_thread_switch(id, buffer, value);
                }
            }
        });
    }

    /// Panic-recovery restore of an owner binding: storage only, no Lisp.
    #[cold]
    #[inline(never)]
    pub(crate) fn discard_owner_binding(&mut self, binding: OwnerBindingRef) {
        let Some(record) = self
            .owner
            .slab_mut()
            .and_then(|slab| slab.take_record(binding.record))
        else {
            return;
        };
        let Some(projection) = self.obarray.thread_projection_of(record.sym) else {
            return;
        };
        if !self.owner_channel_live(record.sym, projection, record.channel) {
            note_dropped(record.undo.values()[0]);
            self.clear_owner_slot(record.index, record.channel);
            return;
        }
        match record.undo {
            UndoTarget::Retired => {}
            UndoTarget::Previous(outer) => {
                if let Some(slab) = self.owner.slab_mut() {
                    slab.set_slot(record.index, record.channel, SlotState::Bound(outer));
                }
            }
            UndoTarget::Inherited { .. } => self.clear_owner_slot(record.index, record.channel),
            UndoTarget::Captured(saved) => {
                self.clear_owner_slot(record.index, record.channel);
                self.owner_canonical_store(record.sym, record.channel, saved.get());
            }
        }
        self.publish_owner_binding(record.sym);
    }

    /// GNU `specpdl_unrewind`'s exchange for an owner binding
    /// (`src/eval.c:4177-4228`), owner-locally: the binding's value and the
    /// value it hides trade places, so `backtrace-eval` sees the frame's
    /// outer value and its rewind restores the binding. A binding that hid
    /// canonical storage is set aside (the owner reads canonical storage
    /// meanwhile) instead of being written there; a buffer-local binding
    /// whose local is gone is dropped, as GNU turns it into `SPECPDL_NOP`.
    #[cold]
    #[inline(never)]
    pub(crate) fn swap_owner_binding(&mut self, binding: OwnerBindingRef) {
        let Some(record) = self
            .owner
            .slab()
            .and_then(|slab| slab.record(binding.record).copied())
        else {
            return;
        };
        let live = self
            .obarray
            .thread_projection_of(record.sym)
            .is_some_and(|projection| {
                self.owner_channel_live(record.sym, projection, record.channel)
            });
        let Some(slab) = self.owner.slab_mut() else {
            return;
        };
        if !live {
            slab.replace_undo(binding.record, UndoTarget::Retired);
            return;
        }
        let current = slab.slot(record.index, record.channel);
        let Some((undo, slot)) = exchanged_binding(record.undo, current) else {
            return;
        };
        slab.set_slot(record.index, record.channel, slot);
        slab.replace_undo(binding.record, undo);
        self.publish_owner_binding(record.sym);
    }

    /// The saved side of an owner entry after a backtrace exchange, with
    /// GNU's original specpdl symbol rather than its migrated storage key.
    #[cold]
    pub(crate) fn owner_binding_saved_value(
        &self,
        binding: OwnerBindingRef,
    ) -> Option<(SymId, Value)> {
        let record = self.owner.slab()?.record(binding.record)?;
        let value = record.undo.values()[0]?;
        Some((binding.symbol(), value.get()))
    }

    /// GNU keeps the original symbol in each specpdl entry. If another
    /// thread aliases it while this owner is parked, resuming that entry
    /// exchanges its saved binding with the alias's new default storage
    /// (`eval.c:4181-4208`). Rebuild only the affected owner slots, in the
    /// original specpdl order, so interleaved bindings of source and target
    /// become one binding chain. The specpdl reference retains the original
    /// symbol for GNU's EQ-based let-bound and toplevel scans.
    #[cold]
    #[inline(never)]
    pub(crate) fn reconcile_aliased_owner_bindings(&mut self) {
        self.reconcile_owner_alias_redirect(None);
    }

    /// GNU's next exchange of a previously failed constant alias will fail
    /// again. Keywords assigned their own value remain an allowed no-op.
    #[cold]
    #[inline(never)]
    pub(crate) fn failed_constant_owner_binding(&self, outgoing: bool) -> Option<SymId> {
        let slab = self.owner.slab()?;
        let mut failure = None;
        // Outgoing exchanges walk inner to outer; returning exchanges walk
        // outer to inner. Selecting the last or first failure preserves both
        // orders without allocating a temporary record vector at every park.
        for (_, record) in slab.live_records() {
            let UndoTarget::Captured(saved) = record.undo else {
                continue;
            };
            if record.channel != Channel::Default || !self.obarray.is_alias_id(record.sym) {
                continue;
            }
            let Some(base) = self.obarray.indirect_variable_id(record.sym) else {
                continue;
            };
            if matches!(
                self.obarray.classify_constant_write(base, saved.get()),
                neovm_value::symbol::ConstantWrite::Refused
            ) {
                failure = Some(record.sym);
                if !outgoing {
                    break;
                }
            }
        }
        failure
    }

    /// The old plain canonical cell is otherwise lost when this owner aliases
    /// a cell that an older, differently named binding currently reaches.
    #[cold]
    #[inline(never)]
    pub(crate) fn owner_alias_redirect_value(&mut self, alias: SymId) -> Option<Value> {
        if !self
            .owner
            .slab()?
            .live_records()
            .any(|(_, record)| record.sym == alias && record.channel == Channel::Default)
        {
            return None;
        }
        Some(self.owner_canonical_value(alias, Channel::Default))
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn finish_owner_alias_redirect(&mut self, alias: SymId, old_value: Value) {
        self.reconcile_owner_alias_redirect(Some((alias, old_value)));
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn reconcile_owner_alias_redirect(&mut self, active: Option<(SymId, Value)>) {
        let Some(slab) = self.owner.slab() else {
            return;
        };
        let changed = self.specpdl.iter().any(|entry| {
            let SpecBinding::NativeUnwind {
                action: NativeUnwindAction::OwnerBinding(binding),
            } = entry
            else {
                return false;
            };
            slab.record(binding.record).is_some_and(|record| {
                record.channel == Channel::Default
                    && !matches!(record.undo, UndoTarget::Captured(_) | UndoTarget::Retired)
                    && self
                        .obarray
                        .indirect_variable_id(binding.symbol())
                        .is_some_and(|base| base != record.sym)
            })
        });
        if !changed {
            return;
        }
        let mut records: Vec<_> = self
            .specpdl
            .iter()
            .filter_map(|entry| {
                let SpecBinding::NativeUnwind {
                    action: NativeUnwindAction::OwnerBinding(binding),
                } = entry
                else {
                    return None;
                };
                let record = slab.record(binding.record).copied()?;
                Some((*binding, record, record.sym, record.index))
            })
            .collect();
        let mut affected = rustc_hash::FxHashSet::default();
        let mut constants = Vec::new();
        for (binding, record, target, index) in &mut records {
            if record.channel != Channel::Default
                || matches!(record.undo, UndoTarget::Captured(_) | UndoTarget::Retired)
            {
                continue;
            }
            let Some(base) = self.obarray.indirect_variable_id(binding.symbol()) else {
                continue;
            };
            if base == record.sym {
                continue;
            }
            if self.obarray.is_constant_id(base) {
                let value = self.builtin_var_value(base).unwrap_or(Value::UNBOUND);
                constants.push((binding.record, base, value));
                affected.insert((record.index, record.channel));
                continue;
            }
            let Some(projection) = self.obarray.owned_thread_projection(base) else {
                continue;
            };
            *target = base;
            *index = projection.index();
            affected.insert((record.index, record.channel));
            affected.insert((*index, record.channel));
        }
        if affected.is_empty() {
            return;
        }
        records.retain(|(_, record, _, index)| {
            affected.contains(&(record.index, record.channel))
                || affected.contains(&(*index, record.channel))
        });
        // At an active redirect GNU abandons the old cell's live binding.
        // Simulate its next reverse exchange now: no peer can run before a
        // park, and the rebuilt overlay keeps this owner's visible value.
        let mut active_values = None;
        if let Some((alias, old_canonical)) = active {
            let mut physical = FxHashMap::default();
            let mut assigned = vec![SlotState::NoBinding; records.len()];
            let mut destinations = FxHashMap::default();
            for (_, record, target, index) in &records {
                let key = (*index, record.channel);
                if !physical.contains_key(&key) {
                    let canonical = self.owner_canonical_value(*target, record.channel);
                    let current = self
                        .owner
                        .slab()
                        .and_then(|slab| slab.slot(*index, record.channel).value())
                        .map_or(canonical, SlotValue::get);
                    physical.insert(key, current);
                    destinations.insert(key, *target);
                }
            }
            for (position, (_, record, _, index)) in records.iter().enumerate().rev() {
                let key = (*index, record.channel);
                assigned[position] = SlotState::Bound(SlotValue::new(physical[&key]));
                let restores = match record.undo {
                    UndoTarget::Inherited { suspended: None } if record.sym == alias => {
                        old_canonical
                    }
                    UndoTarget::Inherited { suspended: None } => {
                        self.owner_canonical_value(record.sym, record.channel)
                    }
                    UndoTarget::Inherited {
                        suspended: Some(saved),
                    }
                    | UndoTarget::Previous(saved)
                    | UndoTarget::Captured(saved) => saved.get(),
                    UndoTarget::Retired => continue,
                };
                physical.insert(key, restores);
            }
            for (key, value) in physical {
                self.owner_canonical_store(destinations[&key], key.1, value);
            }
            active_values = Some(assigned);
        }
        let target_canonical: Vec<_> = records
            .iter()
            .map(|(_, record, target, _)| self.owner_canonical_value(*target, record.channel))
            .collect();
        let target_forwarders: Vec<_> = records
            .iter()
            .map(|(_, _, target, _)| {
                self.obarray
                    .thread_projection_of(*target)
                    .and_then(|p| p.logical_forwarder())
            })
            .collect();
        let slab = self.owner.slab_mut().expect("owner records have a slab");
        let mut old_slots = FxHashMap::default();
        let mut values = vec![SlotState::NoBinding; records.len()];
        for (position, (_, record, _, _)) in records.iter().enumerate().rev() {
            let current = old_slots
                .entry((record.index, record.channel))
                .or_insert_with(|| slab.slot(record.index, record.channel));
            values[position] = *current;
            *current = match record.undo {
                UndoTarget::Inherited { suspended: None } => SlotState::NoBinding,
                UndoTarget::Inherited {
                    suspended: Some(saved),
                }
                | UndoTarget::Previous(saved) => SlotState::Bound(saved),
                UndoTarget::Captured(_) | UndoTarget::Retired => SlotState::NoBinding,
            };
        }
        if let Some(active_values) = active_values {
            values = active_values;
        }
        for &(index, channel) in &affected {
            slab.set_slot(index, channel, SlotState::NoBinding);
        }
        let mut resume_error = None;
        let mut forward_error = None;
        for (position, ((binding, record, target, index), mut value)) in
            records.into_iter().zip(values).enumerate()
        {
            if let Some((_, base, canonical)) =
                constants.iter().find(|(id, _, _)| *id == binding.record)
            {
                // GNU saves the constant's value before its attempted
                // rebind signals. Keep a restoring record: popping the let
                // can signal again, even if an inner handler caught the
                // resume error. No owner overlay may remain on the old cell.
                let assigned = value.value().map_or(*canonical, SlotValue::get);
                if matches!(
                    self.obarray.classify_constant_write(*base, assigned),
                    neovm_value::symbol::ConstantWrite::Refused
                ) {
                    if resume_error.is_none() && forward_error.is_none() {
                        resume_error = Some(binding.symbol());
                    }
                }
                slab.replace_undo(
                    binding.record,
                    UndoTarget::Captured(SlotValue::new(*canonical)),
                );
                slab.records[binding.record.position()]
                    .as_mut()
                    .expect("live constant alias record")
                    .sym = binding.symbol();
                continue;
            }
            if matches!(record.undo, UndoTarget::Retired | UndoTarget::Captured(_)) {
                continue;
            }
            if let (Some(forwarder), SlotState::Bound(assigned)) =
                (target_forwarders[position], value)
            {
                match forwarder.store(assigned.get()) {
                    Ok(stored) => {
                        value = SlotState::Bound(SlotValue::new(stored.canonical_value()))
                    }
                    Err(error) => {
                        if resume_error.is_none() && forward_error.is_none() {
                            forward_error = Some((error, assigned.get()));
                        }
                        slab.replace_undo(
                            binding.record,
                            UndoTarget::Captured(SlotValue::new(target_canonical[position])),
                        );
                        slab.records[binding.record.position()]
                            .as_mut()
                            .expect("live refused alias record")
                            .sym = binding.symbol();
                        continue;
                    }
                }
            }
            let undo = match (value, slab.slot(index, record.channel), record.undo) {
                (
                    SlotState::NoBinding,
                    SlotState::NoBinding,
                    suspended @ UndoTarget::Inherited { suspended: Some(_) },
                ) => suspended,
                (_, SlotState::Bound(previous), _) => UndoTarget::Previous(previous),
                (_, SlotState::NoBinding, _) => UndoTarget::Inherited { suspended: None },
            };
            slab.replace_undo(binding.record, undo);
            let migrated = slab.records[binding.record.position()]
                .as_mut()
                .expect("live record in the reconciliation plan");
            migrated.sym = target;
            migrated.index = index;
            slab.set_slot(index, record.channel, value);
        }
        if let Some(symbol) = resume_error {
            // condition-wait must first retake its mutex, as it does for a
            // pending external signal, before this internal error is raised.
            self.green_defer_binding_resume_error(symbol);
        } else if let Some((error, value)) = forward_error {
            self.green_defer_forward_resume_error(error, value);
        }
    }

    /// Owner entries naming SYMBOL, preserving GNU's original specpdl
    /// symbol even after their effective storage followed an alias.
    pub(crate) fn original_owner_records(
        &self,
        symbol: SymId,
    ) -> impl Iterator<Item = (OwnerRecordId, &OwnerBindingRecord)> {
        let slab = self.owner.slab();
        self.specpdl.iter().filter_map(move |entry| {
            let SpecBinding::NativeUnwind {
                action: NativeUnwindAction::OwnerBinding(binding),
            } = entry
            else {
                return None;
            };
            if binding.symbol() != symbol {
                return None;
            }
            Some((binding.record, slab?.record(binding.record)?))
        })
    }

    /// GNU `Fdefault_toplevel_value`'s answer for an owner binding
    /// (`src/eval.c:811-825`): the value the outermost default binding of ID
    /// hides, `Some(None)` when void, or `None` when this owner has no
    /// default binding of ID.
    pub(crate) fn owner_default_toplevel_value(&mut self, id: SymId) -> Option<Option<Value>> {
        let outermost = self
            .original_owner_records(id)
            .find(|(_, record)| {
                matches!(
                    record.kind,
                    BindingKind::Let | BindingKind::LetDefault { .. }
                )
            })
            .map(|(_, record)| *record)?;
        let value = match outermost.undo {
            UndoTarget::Previous(value) | UndoTarget::Captured(value) => value.get(),
            UndoTarget::Inherited { .. } | UndoTarget::Retired => {
                self.owner_canonical_value(outermost.sym, outermost.channel)
            }
        };
        Some(SlotValue::new(value).bound())
    }

    /// GNU `Fset_default_toplevel_value` for an owner binding
    /// (`src/eval.c:827-840`): set the value the outermost default binding of
    /// ID hides. `false` when this owner has no default binding of ID.
    pub(crate) fn set_owner_default_toplevel_value(&mut self, id: SymId, value: Value) -> bool {
        let Some(slab) = self.owner.slab() else {
            return false;
        };
        let Some(outermost) = self
            .original_owner_records(id)
            .find(|(_, record)| {
                matches!(
                    record.kind,
                    BindingKind::Let | BindingKind::LetDefault { .. }
                )
            })
            .map(|(record, _)| record)
        else {
            return false;
        };
        let Some(record) = slab.record(outermost).copied() else {
            return false;
        };
        match record.undo {
            UndoTarget::Previous(_) | UndoTarget::Captured(_) => {
                if let Some(slab) = self.owner.slab_mut() {
                    let undo = match record.undo {
                        UndoTarget::Captured(_) => UndoTarget::Captured(SlotValue::new(value)),
                        _ => UndoTarget::Previous(SlotValue::new(value)),
                    };
                    slab.replace_undo(outermost, undo);
                }
            }
            UndoTarget::Inherited { .. } | UndoTarget::Retired => {
                self.owner_canonical_store(record.sym, record.channel, value);
            }
        }
        true
    }

    /// The same two questions for GNU's `local_toplevel_binding`
    /// (`src/eval.c:766-782`): the outermost buffer-local binding of ID in
    /// BUFFER. `None` when this owner has none.
    pub(crate) fn owner_local_toplevel_value(
        &mut self,
        id: SymId,
        buffer: BufferId,
    ) -> Option<Value> {
        let outermost = self
            .owner
            .slab()?
            .records_of(id)
            .find(|record| record.channel == Channel::BufferLocal(buffer))
            .copied()?;
        Some(match outermost.undo {
            UndoTarget::Previous(value) | UndoTarget::Captured(value) => value.get(),
            UndoTarget::Inherited { .. } | UndoTarget::Retired => {
                self.owner_canonical_value(id, outermost.channel)
            }
        })
    }

    /// Set the value the outermost buffer-local binding of ID in BUFFER
    /// hides. `false` when this owner has none.
    pub(crate) fn set_owner_local_toplevel_value(
        &mut self,
        id: SymId,
        buffer: BufferId,
        value: Value,
    ) -> bool {
        let Some(slab) = self.owner.slab() else {
            return false;
        };
        let Some(outermost) = slab.record_ids_of(id).into_iter().find(|record| {
            slab.record(*record)
                .is_some_and(|record| record.channel == Channel::BufferLocal(buffer))
        }) else {
            return false;
        };
        let Some(record) = slab.record(outermost).copied() else {
            return false;
        };
        match record.undo {
            UndoTarget::Previous(_) | UndoTarget::Captured(_) => {
                if let Some(slab) = self.owner.slab_mut() {
                    let undo = match record.undo {
                        UndoTarget::Captured(_) => UndoTarget::Captured(SlotValue::new(value)),
                        _ => UndoTarget::Previous(SlotValue::new(value)),
                    };
                    slab.replace_undo(outermost, undo);
                }
            }
            UndoTarget::Inherited { .. } | UndoTarget::Retired => {
                self.owner_canonical_store(id, record.channel, value);
            }
        }
        true
    }

    /// Whether this owner binds ID (`Fdefvaralias`'s let-bound refusal).
    #[inline]
    pub(crate) fn owner_binds(&self, id: SymId) -> bool {
        self.original_owner_records(id).next().is_some()
    }

    /// This owner's default-channel binding of the projected variable ID, if
    /// any: what GNU's `default_value` reads while the running thread binds
    /// the default (`src/data.c:1962-2004`). A Context with no owner bindings
    /// pays one compare of its owner word and a branch.
    #[inline]
    pub(crate) fn owner_default_binding(&self, id: SymId) -> Option<SlotValue> {
        if self.owner.is_dormant() {
            return None;
        }
        self.owner_default_binding_slow(id)
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn owner_default_binding_slow(&self, id: SymId) -> Option<SlotValue> {
        let slab = self.owner.slab()?;
        let projection = self.obarray.thread_projection_of(id)?;
        slab.slot(projection.index(), Channel::Default).value()
    }

    /// Store VALUE in this owner's default binding of the projected variable
    /// ID (`set-default` while the owner binds the default).
    pub(crate) fn store_owner_default(&mut self, id: SymId, value: Value) {
        let Some(projection) = self.obarray.thread_projection_of(id) else {
            return;
        };
        if let Some(slab) = self.owner.slab_mut() {
            slab.set_slot(
                projection.index(),
                Channel::Default,
                SlotState::Bound(SlotValue::new(value)),
            );
        }
    }

    /// `kill-local-variable` of the projected variable ID in the current
    /// buffer, before the local goes: a binding of this owner hiding that
    /// local now hides nothing. Its outermost binding keeps the value the
    /// local held, which GNU's entry carries and restores if the local is
    /// made again (`src/eval.c:3871-3883`).
    pub(crate) fn owner_local_killed(&mut self, id: SymId) {
        let Some(buffer) = self.edit.buffers.current_buffer_id() else {
            return;
        };
        let channel = Channel::BufferLocal(buffer);
        if self.owner.slab().is_none() {
            return;
        }
        let canonical = self.owner_canonical_value(id, channel);
        self.capture_owner_local(id, buffer, canonical);
    }

    /// The local of ID in BUFFER, which held CANONICAL, is gone: this owner's
    /// outermost binding of it keeps CANONICAL to restore if the local is
    /// made again, and the owner's slot for it goes.
    pub(crate) fn capture_owner_local(&mut self, id: SymId, buffer: BufferId, canonical: Value) {
        let channel = Channel::BufferLocal(buffer);
        let Some(slab) = self.owner.slab() else {
            return;
        };
        let Some(outermost) = slab.canonical_root_record(id, channel) else {
            return;
        };
        let Some(record) = slab.record(outermost).copied() else {
            return;
        };
        // Save the active chain's restore. Older exchanged entries already
        // carry their own values and must remain untouched.
        if let UndoTarget::Inherited { suspended: None } = record.undo
            && let Some(slab) = self.owner.slab_mut()
        {
            slab.replace_undo(outermost, UndoTarget::Captured(SlotValue::new(canonical)));
        }
        self.clear_owner_slot(record.index, channel);
    }

    /// Before `kill-all-local-variables` resets BUFFER's locals: the
    /// variables this owner binds there and the canonical local each binding
    /// hides, for [`Self::owner_locals_reset`].
    #[cold]
    #[inline(never)]
    pub(crate) fn owner_locals_before_reset(&mut self, buffer: BufferId) -> Vec<(SymId, Value)> {
        let channel = Channel::BufferLocal(buffer);
        let Some(slab) = self.owner.slab() else {
            return Vec::new();
        };
        let mut bound: Vec<SymId> = slab
            .live_records()
            .filter(|(_, record)| record.channel == channel)
            .map(|(_, record)| record.sym)
            .collect();
        bound.sort_unstable_by_key(|id| id.to_raw());
        bound.dedup();
        bound
            .into_iter()
            .map(|id| (id, self.owner_canonical_value(id, channel)))
            .collect()
    }

    /// `kill-all-local-variables` reset BUFFER's locals: each binding of
    /// this owner whose local is gone captures what the local held, as
    /// `kill-local-variable` makes it do (GNU's entry keeps its saved value,
    /// `src/eval.c:3871-3883`). BEFORE comes from
    /// [`Self::owner_locals_before_reset`]; no Lisp ran in between, so its
    /// values are still reachable from nothing a collection could miss.
    #[cold]
    #[inline(never)]
    pub(crate) fn owner_locals_reset(&mut self, buffer: BufferId, before: Vec<(SymId, Value)>) {
        let channel = Channel::BufferLocal(buffer);
        for (id, canonical) in before {
            let Some(projection) = self.obarray.thread_projection_of(id) else {
                continue;
            };
            if !self.owner_channel_live(id, projection, channel) {
                self.capture_owner_local(id, buffer, canonical);
            }
        }
    }

    /// BUFFERS were killed: this owner's bindings of their locals restore
    /// nothing at their pops (GNU's `Flocal_variable_p` test fails on a dead
    /// buffer, `src/eval.c:3871`), and their slots go.
    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn owner_buffers_killed(&mut self, buffers: &[BufferId]) {
        let Some(slab) = self.owner.slab() else {
            return;
        };
        let dead: Vec<_> = slab
            .live_records()
            .filter(|(_, record)| {
                matches!(record.channel, Channel::BufferLocal(buffer) if buffers.contains(&buffer))
            })
            .map(|(id, record)| (id, record.index, record.channel))
            .collect();
        for (id, index, channel) in dead {
            if let Some(slab) = self.owner.slab_mut() {
                slab.replace_undo(id, UndoTarget::Retired);
            }
            self.clear_owner_slot(index, channel);
        }
    }

    /// This owner's binding of the projected variable ID in BUFFER's local,
    /// if it binds one: what `buffer-local-value` answers for BUFFER, as GNU's
    /// local cell holds the running thread's binding.
    #[cold]
    #[inline(never)]
    pub(crate) fn owner_local_binding(&self, id: SymId, buffer: BufferId) -> Option<SlotValue> {
        let projection = self.obarray.thread_projection_of(id)?;
        self.owner
            .slab()?
            .slot(projection.index(), Channel::BufferLocal(buffer))
            .value()
    }

    /// Whether the current buffer has a local binding of the projected
    /// variable ID.
    pub(crate) fn projected_local_in_current_buffer(&self, id: SymId) -> bool {
        let (Some(projection), Some(buffer)) = (
            self.obarray.thread_projection_of(id),
            self.edit.buffers.current_buffer_id(),
        ) else {
            return false;
        };
        self.owner_channel_live(id, projection, Channel::BufferLocal(buffer))
    }

    /// `make-local-variable` of the projected variable ID made a local in
    /// the current buffer. GNU starts it at the default the running thread
    /// sees (`src/data.c:2245-2348`), which is this owner's default binding
    /// when it has one. A binding of this owner whose local was killed takes
    /// the new local back: the local holds what its outermost binding hid,
    /// and the owner's slot the value the local starts with.
    pub(crate) fn owner_local_made(&mut self, id: SymId) {
        let Some(buffer) = self.edit.buffers.current_buffer_id() else {
            return;
        };
        let channel = Channel::BufferLocal(buffer);
        let Some(projection) = self.obarray.thread_projection_of(id) else {
            return;
        };
        if !self.owner_channel_live(id, projection, channel) {
            return;
        }
        let Some(slab) = self.owner.slab() else {
            return;
        };
        let default = slab.slot(projection.index(), Channel::Default).value();
        // A retired binding is GNU's `SPECPDL_NOP`: it binds nothing.
        let outermost = slab.canonical_root_record(id, channel).or_else(|| {
            slab.record_ids_of(id).into_iter().find(|record| {
                slab.record(*record).is_some_and(|record| {
                    record.channel == channel && !matches!(record.undo, UndoTarget::Retired)
                })
            })
        });
        let start = match default {
            Some(default) => default.get(),
            None => self.owner_canonical_value(id, Channel::Default),
        };
        let suspended = outermost.is_some_and(|outermost| {
            self.owner
                .slab()
                .and_then(|slab| slab.record(outermost))
                .is_some_and(|record| {
                    matches!(record.undo, UndoTarget::Inherited { suspended: Some(_) })
                })
        });
        match outermost {
            // A binding `backtrace-eval` has set aside stays aside: the new
            // local is what it hides, and the rewind brings the binding back
            // (GNU's exchange of an entry whose local exists again,
            // `src/eval.c:4204-4228`).
            Some(_) if suspended => self.owner_canonical_store(id, channel, start),
            Some(outermost) => {
                let hidden = match self.owner.slab().and_then(|slab| slab.record(outermost)) {
                    Some(OwnerBindingRecord {
                        undo: UndoTarget::Captured(saved),
                        ..
                    }) => Some(*saved),
                    _ => None,
                };
                if let Some(hidden) = hidden {
                    self.owner_canonical_store(id, channel, hidden.get());
                    if let Some(slab) = self.owner.slab_mut() {
                        slab.replace_undo(outermost, UndoTarget::Inherited { suspended: None });
                    }
                }
                if let Some(slab) = self.owner.slab_mut() {
                    slab.set_slot(
                        projection.index(),
                        channel,
                        SlotState::Bound(SlotValue::new(start)),
                    );
                }
            }
            None if default.is_some() => self.owner_canonical_store(id, channel, start),
            None => {}
        }
    }
}

/// The canonical value of the variable PROJECTION wraps without a buffer,
/// as `Obarray::canonical_value` reads an unprojected cell: the plain word,
/// a localized variable's default, a builtin's own storage (none for a
/// per-buffer slot). `None` when void.
pub(super) fn canonical_bufferless_value(projection: &ThreadProjectionDescriptor) -> Option<Value> {
    let value = match projection.arm() {
        CanonicalArm::Plain => projection.plain_value(),
        // SAFETY: a projection's localized arm names a live record of the
        // obarray's BLV pool; its default cell is only read.
        CanonicalArm::Localized(blv) => unsafe { (*blv.blv().as_ptr()).defcell().cons_cdr() },
        CanonicalArm::Forwarded(builtin) => return builtin.descriptor().load(),
    };
    (!value.is_unbound()).then_some(value)
}

/// The owner-aware halves of `eval::variable_access`: cold, reached only by a
/// Context with owner bindings.
impl Context {
    /// The projection behind ID, through its aliases.
    pub(crate) fn projection_behind(
        &self,
        id: SymId,
    ) -> Option<(SymId, &'static ThreadProjectionDescriptor)> {
        let base = self.obarray.indirect_variable_id(id)?;
        Some((base, self.obarray.thread_projection_of(base)?))
    }

    /// Signal-time native integer storage only: no watchers, automatic locals,
    /// generic forwarder dispatch, or host mirror callbacks. The caller updates
    /// max_depth after completing the paired depth/reserve stores.
    #[cold]
    #[inline(never)]
    pub(crate) fn store_owner_signal_depth_integer(&mut self, id: SymId, value: LispInteger) {
        debug_assert!(
            id == super::max_lisp_eval_depth_symbol()
                || id == super::lisp_eval_depth_reserve_symbol()
        );
        let projection = self
            .obarray
            .thread_projection_of(id)
            .expect("projected depth variable");
        let index = projection.index();
        let channel = access_channel(&self.edit.buffers, id, projection);
        let bound = self
            .owner
            .slab()
            .is_some_and(|slab| slab.slot(index, channel).value().is_some());
        if bound {
            self.owner
                .slab_mut()
                .expect("the active owner binding")
                .set_slot(
                    index,
                    channel,
                    SlotState::Bound(SlotValue::new(value.value())),
                );
            return;
        }
        let (forwarder, cell) = match projection.arm() {
            CanonicalArm::Forwarded(builtin) => (builtin.descriptor(), None),
            CanonicalArm::Localized(blv) => {
                // SAFETY: the projection names its obarray's live pooled BLV;
                // only the retained forwarder and default cons are copied.
                let blv = unsafe { &*blv.blv().as_ptr() };
                let cell = match channel {
                    Channel::Default => blv.defcell(),
                    Channel::BufferLocal(buffer) => self
                        .edit
                        .buffers
                        .get(buffer)
                        .and_then(|buffer| buffer.local_variable_binding_cell(id))
                        .expect("the selected local depth binding"),
                };
                (blv.fwd.expect("native depth forwarder"), Some(cell))
            }
            CanonicalArm::Plain => unreachable!("native depth variable retains its forwarder"),
        };
        forwarder
            .as_int_fwd()
            .expect("native depth variable has integer storage")
            .set(value);
        if let Some(cell) = cell {
            cell.set_cdr(value.value());
        }
    }

    /// [`Context::store_variable`] of a projected cell.
    #[cold]
    #[inline(never)]
    pub(crate) fn store_owner_variable(&mut self, id: SymId, value: Value) {
        debug_assert!(
            self.owner.slab().is_some(),
            "a Context that sees a projected cell has an owner slab"
        );
        let Some((base, projection)) = self.projection_behind(id) else {
            self.obarray
                .set_canonical_value(CanonicalAccess::ROUTED, id, value);
            return;
        };
        let index = projection.index();
        if let Some(slab) = self.owner.slab_mut()
            && slab.slot(index, Channel::Default).value().is_some()
        {
            slab.set_slot(
                index,
                Channel::Default,
                SlotState::Bound(SlotValue::new(value)),
            );
            return;
        }
        self.with_revealed_projection(base, |ctx| {
            ctx.obarray
                .set_canonical_value(CanonicalAccess::STORAGE, base, value);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neovm_value::intern::NIL_SYM_ID;

    fn push(slab: &mut OwnerSlab) -> OwnerRecordId {
        slab.push_record(
            NIL_SYM_ID,
            TlsIndex::VOID,
            BindingKind::Let,
            Channel::Default,
            UndoTarget::Retired,
        )
    }

    #[test]
    fn top_record_id_is_the_last_live_slot_and_take_top_refuses_below() {
        let mut slab = OwnerSlab::new(1);
        assert_eq!(slab.top_record_id(), None);

        let outer = push(&mut slab);
        assert_eq!(slab.top_record_id(), Some(outer));
        let inner = push(&mut slab);
        assert_eq!(slab.top_record_id(), Some(inner));
        assert_eq!(outer.position(), 0);
        assert_eq!(inner.position(), 1);

        assert!(slab.take_top_record(outer).is_none());
        assert_eq!(slab.top_record_id(), Some(inner));
        assert_eq!(slab.live_records().count(), 2);

        let holed = slab
            .take_record_below(outer)
            .expect("the record below the top");
        assert!(matches!(holed.undo, UndoTarget::Retired));
        assert_eq!(slab.top_record_id(), Some(inner));
        assert_eq!(slab.live_records().count(), 1);

        let taken = slab.take_top_record(inner).expect("the top record");
        assert!(matches!(taken.undo, UndoTarget::Retired));
        assert_eq!(slab.top_record_id(), None);
        assert_eq!(slab.live_records().count(), 0);
        assert!(slab.take_top_record(inner).is_none());

        let again = push(&mut slab);
        assert_eq!(again.position(), 0, "trailing holes are trimmed");
        assert_ne!(again, inner);
        assert!(slab.take_top_record(inner).is_none());
        assert_eq!(slab.top_record_id(), Some(again));
    }
}
