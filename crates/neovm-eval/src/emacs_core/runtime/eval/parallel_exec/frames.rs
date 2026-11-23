//! Owned frames, handler/bind/slot records and rooting records (E4-DESIGN §5).
//!
//! E4 is stackful: a parallel owner recurses on its own OS thread, exactly as
//! the serial engines do, and these records are owned state for rooting,
//! handler matching and backtrace — never stackless continuations. The arena
//! is plain data: no E1 capability is constructed here and no serial engine
//! reads it.
//!
//! Root-schema coverage is compile-time: every detachable frame implements
//! [`FrameRoots`] by destructuring `Self` WITHOUT a `..` rest pattern, so a
//! new field fails to compile (E0027) until it is classified as a root or as
//! frame-local state. [`FrameKind`] enumerates the frames the arena can hold;
//! the architecture test pins that enumeration against this source.

use super::super::owner_bindings::OwnerRecordId;
use neovm_value::intern::SymId;
use neovm_value::tagged::gc::{
    OwnerAllocRegion, OwnerEraReserve, OwnerRegionRefund, RendezvousError, ReserveError, TaggedHeap,
};
use neovm_value::tagged::transport::SharedRoot;
use neovm_value::value::Value;
#[cfg(test)]
use std::cell::Cell;
use std::num::{NonZeroU32, NonZeroU64};

/// The canonical E3 private-root groups (`gc_pacing::trace_root_groups`) the
/// executor publishes. The names are exactly that roster so stage 1 plugs the
/// owner publication in without a second vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum RootGroup {
    VmFrames,
    EvalTemp,
    Bc,
    JitWindow,
    Handlers,
    Specpdl,
    Misc,
}

impl RootGroup {
    /// The complete roster, in `gc_pacing` order.
    #[doc(hidden)]
    pub const ALL: [Self; 7] = [
        Self::VmFrames,
        Self::EvalTemp,
        Self::Bc,
        Self::JitWindow,
        Self::Handlers,
        Self::Specpdl,
        Self::Misc,
    ];

    #[doc(hidden)]
    pub const fn name(self) -> &'static str {
        match self {
            Self::VmFrames => "vm_frames",
            Self::EvalTemp => "eval_temp",
            Self::Bc => "bc",
            Self::JitWindow => "jit_window",
            Self::Handlers => "handlers",
            Self::Specpdl => "specpdl",
            Self::Misc => "misc",
        }
    }

    /// Index into a scan counted in [`Self::ALL`] order.
    const fn ordinal(self) -> usize {
        match self {
            Self::VmFrames => 0,
            Self::EvalTemp => 1,
            Self::Bc => 2,
            Self::JitWindow => 3,
            Self::Handlers => 4,
            Self::Specpdl => 5,
            Self::Misc => 6,
        }
    }
}

const _: () = {
    let mut index = 0;
    while index < RootGroup::ALL.len() {
        assert!(RootGroup::ALL[index].ordinal() == index);
        index += 1;
    }
};

/// Why a root could not be published or revoked.
///
/// [`RootPublishError::GenerationExhausted`] is the non-wrapping end of the
/// `NonZeroU64` counter. The registry stays as it was; it does not recycle a
/// generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RootPublishError {
    GenerationExhausted,
    IndexExhausted,
}

/// Why a revoke did not remove a root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RootRevokeError {
    /// The handle's generation is not the slot's current generation, or the
    /// handle does not name the live top. The slot payload is not read.
    Stale,
    GenerationExhausted,
}

/// A revocable token for one published root.
///
/// `generation` is a `NonZeroU64` taken from a counter that refuses to wrap.
/// After the slot is reused, this handle no longer names it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct RootHandle {
    index: u32,
    generation: NonZeroU64,
}

impl RootHandle {
    #[doc(hidden)]
    pub const fn index(self) -> u32 {
        self.index
    }

    #[doc(hidden)]
    pub const fn generation(self) -> NonZeroU64 {
        self.generation
    }
}

/// The value stored in a slot. Vacant slots stay in the table so a later push
/// can reuse the index under a new generation.
#[derive(Clone, Copy, Debug)]
enum SlotPayload {
    Vacant,
    Live { group: RootGroup, value: Value },
}

struct RootSlot {
    generation: NonZeroU64,
    payload: SlotPayload,
}

/// One vector per canonical group. Adding a field without naming it in
/// [`RootRegistry::trace_root_groups`] is a compile error (E0027).
struct GroupPayloads {
    vm_frames: Vec<Value>,
    eval_temp: Vec<Value>,
    bc: Vec<Value>,
    jit_window: Vec<Value>,
    handlers: Vec<Value>,
    specpdl: Vec<Value>,
    misc: Vec<Value>,
}

impl GroupPayloads {
    fn from_live<'a>(slots: impl Iterator<Item = &'a RootSlot>) -> Self {
        let mut payloads = Self {
            vm_frames: Vec::new(),
            eval_temp: Vec::new(),
            bc: Vec::new(),
            jit_window: Vec::new(),
            handlers: Vec::new(),
            specpdl: Vec::new(),
            misc: Vec::new(),
        };
        for slot in slots {
            let SlotPayload::Live { group, value } = slot.payload else {
                continue;
            };
            match group {
                RootGroup::VmFrames => payloads.vm_frames.push(value),
                RootGroup::EvalTemp => payloads.eval_temp.push(value),
                RootGroup::Bc => payloads.bc.push(value),
                RootGroup::JitWindow => payloads.jit_window.push(value),
                RootGroup::Handlers => payloads.handlers.push(value),
                RootGroup::Specpdl => payloads.specpdl.push(value),
                RootGroup::Misc => payloads.misc.push(value),
            }
        }
        payloads
    }
}

/// The owner's auxiliary published roots, separate from the frame arena.
///
/// Slots are a dense live prefix. Revoke bumps the slot generation before
/// clearing the payload, so a stale [`RootHandle`] fails the generation
/// compare and does not read the value.
pub(crate) struct RootRegistry {
    slots: Vec<RootSlot>,
    live: u32,
    next_generation: NonZeroU64,
}

impl RootRegistry {
    #[doc(hidden)]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            live: 0,
            next_generation: NonZeroU64::MIN,
        }
    }

    /// Publish VALUE in GROUP. The returned handle is the only lookup key.
    #[must_use]
    #[doc(hidden)]
    pub fn push(&mut self, group: RootGroup, value: Value) -> Result<RootHandle, RootPublishError> {
        if self.live == u32::MAX {
            return Err(RootPublishError::IndexExhausted);
        }
        let generation = self.issue()?;
        let index = self.live;
        if index as usize == self.slots.len() {
            self.slots.push(RootSlot {
                generation,
                payload: SlotPayload::Vacant,
            });
        }
        let slot = &mut self.slots[index as usize];
        slot.generation = generation;
        slot.payload = SlotPayload::Live { group, value };
        self.live += 1;
        Ok(RootHandle { index, generation })
    }

    /// The value HANDLE names, or `None` when the handle is stale.
    ///
    /// The generation compare happens before the payload match. A mismatch
    /// returns here and does not read [`SlotPayload`].
    #[doc(hidden)]
    pub fn lookup(&self, handle: RootHandle) -> Option<Value> {
        let slot = self.slots.get(handle.index as usize)?;
        if slot.generation != handle.generation {
            return None;
        }
        match slot.payload {
            SlotPayload::Live { value, .. } => Some(value),
            SlotPayload::Vacant => None,
        }
    }

    /// Drop the top root when HANDLE names it. A stale or non-top handle is
    /// refused before the payload is read.
    #[doc(hidden)]
    pub fn pop(&mut self, handle: RootHandle) -> Result<Value, RootRevokeError> {
        let Some(slot) = self.slots.get(handle.index as usize) else {
            return Err(RootRevokeError::Stale);
        };
        if slot.generation != handle.generation || handle.index.saturating_add(1) != self.live {
            return Err(RootRevokeError::Stale);
        }
        let value = match slot.payload {
            SlotPayload::Live { value, .. } => value,
            SlotPayload::Vacant => return Err(RootRevokeError::Stale),
        };
        self.retire_last()
            .map_err(|_| RootRevokeError::GenerationExhausted)?;
        Ok(value)
    }

    /// Drop every live root at or above KEEP. Returns how many were removed.
    #[doc(hidden)]
    pub fn truncate(&mut self, keep: usize) -> Result<usize, RootPublishError> {
        let keep = u32::try_from(keep).unwrap_or(self.live).min(self.live);
        let removed = (self.live - keep) as usize;
        while self.live > keep {
            self.retire_last()?;
        }
        Ok(removed)
    }

    #[doc(hidden)]
    pub fn len(&self) -> usize {
        self.live as usize
    }

    #[doc(hidden)]
    pub fn trace_roots(&self, visit: &mut dyn FnMut(RootGroup, Value)) {
        for slot in self.slots.iter().take(self.live as usize) {
            if let SlotPayload::Live { group, value } = slot.payload {
                visit(group, value);
            }
        }
    }

    /// Enter every [`RootGroup`] once, including a group with no live roots.
    ///
    /// The payload struct is destructured without a rest pattern, and
    /// [`GroupPayloads::from_live`] matches every variant. A new group fails
    /// to compile until both sites name it.
    #[doc(hidden)]
    pub fn trace_root_groups(&self, visit: &mut dyn FnMut(RootGroup, &[Value])) {
        let GroupPayloads {
            vm_frames,
            eval_temp,
            bc,
            jit_window,
            handlers,
            specpdl,
            misc,
        } = GroupPayloads::from_live(self.slots.iter().take(self.live as usize));
        visit(RootGroup::VmFrames, &vm_frames);
        visit(RootGroup::EvalTemp, &eval_temp);
        visit(RootGroup::Bc, &bc);
        visit(RootGroup::JitWindow, &jit_window);
        visit(RootGroup::Handlers, &handlers);
        visit(RootGroup::Specpdl, &specpdl);
        visit(RootGroup::Misc, &misc);
    }

    /// Park the counter at its last value so the next issue refuses.
    #[cfg(test)]
    pub(crate) fn exhaust_generations_for_test(&mut self) {
        self.next_generation = NonZeroU64::MAX;
    }

    fn issue(&mut self) -> Result<NonZeroU64, RootPublishError> {
        let generation = self.next_generation;
        self.next_generation = generation
            .checked_add(1)
            .ok_or(RootPublishError::GenerationExhausted)?;
        Ok(generation)
    }

    fn retire_last(&mut self) -> Result<(), RootPublishError> {
        debug_assert!(self.live > 0, "retiring an empty root registry");
        let generation = self.issue()?;
        let index = self.live - 1;
        let slot = &mut self.slots[index as usize];
        slot.generation = generation;
        slot.payload = SlotPayload::Vacant;
        self.live = index;
        Ok(())
    }
}

/// A bytecode program counter in the owning frame's code vector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Pc(usize);

impl Pc {
    #[doc(hidden)]
    pub const fn new(position: usize) -> Self {
        Self(position)
    }

    #[doc(hidden)]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// The owner operand stack segment a VM frame owns: base and length, never a
/// raw pair of bare `usize` meanings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OperandCursor {
    base: usize,
    len: usize,
}

impl OperandCursor {
    #[doc(hidden)]
    pub const fn new(base: usize, len: usize) -> Self {
        Self { base, len }
    }

    #[doc(hidden)]
    pub const fn base(self) -> usize {
        self.base
    }

    #[doc(hidden)]
    pub const fn len(self) -> usize {
        self.len
    }
}

/// Per-owner call depth (GNU `lisp_eval_depth` / `max-lisp-eval-depth`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CallDepth {
    current: u32,
    limit: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DepthLimitExceeded {
    limit: u32,
}

impl DepthLimitExceeded {
    #[doc(hidden)]
    pub const fn limit(self) -> u32 {
        self.limit
    }
}

/// Why a frame could not be published.
///
/// [`FramePublishError::GenerationExhausted`] is the non-wrapping end of the
/// `NonZeroU64` counter. The arena does not recycle a generation, and the
/// call-depth enter is undone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FramePublishError {
    Depth(DepthLimitExceeded),
    GenerationExhausted,
}

impl CallDepth {
    #[doc(hidden)]
    pub const fn new(limit: u32) -> Self {
        Self { current: 0, limit }
    }

    #[doc(hidden)]
    pub const fn current(self) -> u32 {
        self.current
    }

    #[doc(hidden)]
    pub const fn limit(self) -> u32 {
        self.limit
    }

    #[doc(hidden)]
    pub fn enter(&mut self) -> Result<u32, DepthLimitExceeded> {
        if self.current >= self.limit {
            return Err(DepthLimitExceeded { limit: self.limit });
        }
        self.current += 1;
        Ok(self.current)
    }

    #[doc(hidden)]
    pub fn leave(&mut self) {
        debug_assert!(self.current > 0, "call depth underflow");
        self.current = self.current.saturating_sub(1);
    }
}

/// A frame's poll budget: how many operations may pass before the next
/// named poll point publishes the frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PollBudget {
    remaining: u32,
    interval: NonZeroU32,
}

impl PollBudget {
    #[doc(hidden)]
    pub const fn new(interval: NonZeroU32) -> Self {
        Self {
            remaining: interval.get(),
            interval,
        }
    }

    /// Consume one operation; `true` means this operation reached the poll.
    #[doc(hidden)]
    pub fn consume(&mut self) -> bool {
        if self.remaining <= 1 {
            self.remaining = self.interval.get();
            true
        } else {
            self.remaining -= 1;
            false
        }
    }

    #[doc(hidden)]
    pub const fn remaining(self) -> u32 {
        self.remaining
    }
}

/// The depth of a handler stack, so a mark taken before a frame's handlers
/// cannot be confused with a bare index.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct HandlerMark(usize);

impl HandlerMark {
    #[doc(hidden)]
    pub const EMPTY: Self = Self(0);

    #[doc(hidden)]
    pub const fn depth(self) -> usize {
        self.0
    }
}

/// The depth of the owner binding-record stack (the specpdl-equivalent
/// record stack E4's frames save and restore).
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct OwnerRecordMark(usize);

impl OwnerRecordMark {
    #[doc(hidden)]
    pub const EMPTY: Self = Self(0);

    #[doc(hidden)]
    pub const fn depth(self) -> usize {
        self.0
    }
}

/// A live frame handle: its arena position plus the generation it was issued
/// at, so a stale handle never names a reused slot.
///
/// `generation` is a `NonZeroU64` from a counter that refuses to wrap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct FrameId {
    depth: NonZeroU32,
    generation: NonZeroU64,
}

impl FrameId {
    #[doc(hidden)]
    pub const fn generation(self) -> NonZeroU64 {
        self.generation
    }
}

/// The catch/condition-case/handler-bind records an owner frame owns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HandlerKind {
    /// A `catch` tag record.
    Catch,
    /// A `condition-case` handler record.
    ConditionCase,
    /// A `handler-bind` record.
    HandlerBind,
}

/// One owner-private handler record. Both `tag` and `handlers` are rooted in
/// the published frame; the marks locate the frame and its binding depth.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HandlerRecord {
    pub(crate) kind: HandlerKind,
    pub(crate) tag: Value,
    pub(crate) handlers: Value,
    pub(crate) frame: FrameId,
    pub(crate) bind_mark: OwnerRecordMark,
}

impl HandlerRecord {
    #[doc(hidden)]
    pub const fn new(
        kind: HandlerKind,
        tag: Value,
        handlers: Value,
        frame: FrameId,
        bind_mark: OwnerRecordMark,
    ) -> Self {
        Self {
            kind,
            tag,
            handlers,
            frame,
            bind_mark,
        }
    }
}

/// One owner binding as the executor holds it: the frame-unwind index of the
/// record the E2 owner slab stores (one record, two views). The slab record
/// is the storage — the undo target, the channel and the slot — and the
/// collector's root phase traces its values through the slab, so this entry
/// names it by its [`OwnerRecordId`] and carries no `Value`: the arena's
/// root walk publishes nothing for it. `symbol` is the bound symbol, as the
/// serial specpdl's `OwnerBindingRef` carries it for the unwinder.
///
/// The pairing is kept by construction: a bind pushes the slab record and
/// then this entry for the id it got; an unbind takes the slab record the id
/// names from the top and then removes the entry. An entry whose id is not
/// the slab's top is a defect on one side of that pairing, not a Lisp
/// outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SlotRecord {
    pub(crate) symbol: SymId,
    pub(crate) record: OwnerRecordId,
}

impl SlotRecord {
    #[doc(hidden)]
    pub const fn new(symbol: SymId, record: OwnerRecordId) -> Self {
        Self { symbol, record }
    }
}

/// Names one dynamic record the serial side holds for this owner: a specpdl
/// entry the owner did not mirror (`save-excursion`, an unwind-protect, a
/// localized binding). Opaque: the owner never dereferences it, and it roots
/// nothing here. E5's `OwnerSerialState` mints it; the V1 harness double does.
///
/// GNU's `Bunbind` pops entries of any kind (`unbind_to` walks the specpdl,
/// `eval.c:3946-3957`), so owner bindings and serial records share one order.
/// That is why this is a marker in the one LIFO, not a count beside it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SerialRecordToken(pub(crate) u64);

/// Names one handler the serial side holds for this owner (`catch`,
/// `condition-case`). A second type because GNU keeps the handler list apart
/// from the specpdl (`struct handler` carries its own `pdlcount`,
/// `lisp.h:3863`), and so does this arena.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SerialHandlerToken(pub(crate) u64);

/// One entry of the owner's dynamic-record LIFO: an owner binding, or the
/// marker of a record the serial side holds. Neither entry publishes a
/// root: a [`SlotRecord`] only names the slab record that stores and roots
/// the binding, and a `Serial` entry is an opaque marker.
#[derive(Clone, Copy, Debug)]
pub(crate) enum OwnerRecord {
    Plain(SlotRecord),
    Serial(SerialRecordToken),
}

impl From<SlotRecord> for OwnerRecord {
    fn from(record: SlotRecord) -> Self {
        Self::Plain(record)
    }
}

impl From<SerialRecordToken> for OwnerRecord {
    fn from(token: SerialRecordToken) -> Self {
        Self::Serial(token)
    }
}

/// One entry of the owner's handler LIFO. See [`OwnerRecord`].
#[derive(Clone, Copy, Debug)]
pub(crate) enum OwnerHandler {
    Owner(HandlerRecord),
    Serial(SerialHandlerToken),
}

impl From<HandlerRecord> for OwnerHandler {
    fn from(record: HandlerRecord) -> Self {
        Self::Owner(record)
    }
}

impl From<SerialHandlerToken> for OwnerHandler {
    fn from(token: SerialHandlerToken) -> Self {
        Self::Serial(token)
    }
}

/// The owner's handler stack with typed marks.
///
/// Marks are depths, so they keep their meaning over entries of either kind.
/// [`HandlerStack::truncate`] removes every entry above the mark. A frame
/// that owns an [`OwnerHandler::Serial`] entry above its base never truncates
/// owner-locally: the serial cleanup removes it and reports what it removed
/// (VM-EXECUTOR-DESIGN r1 §5).
#[derive(Default)]
pub(crate) struct HandlerStack {
    records: Vec<OwnerHandler>,
}

impl HandlerStack {
    #[doc(hidden)]
    pub fn new() -> Self {
        Self::default()
    }

    #[doc(hidden)]
    pub fn mark(&self) -> HandlerMark {
        HandlerMark(self.records.len())
    }

    #[doc(hidden)]
    pub fn push(&mut self, record: impl Into<OwnerHandler>) {
        self.records.push(record.into());
    }

    /// Pop every record above MARK; returns how many were removed.
    #[doc(hidden)]
    pub fn truncate(&mut self, mark: HandlerMark) -> usize {
        let keep = mark.depth().min(self.records.len());
        let removed = self.records.len() - keep;
        self.records.truncate(keep);
        removed
    }

    #[doc(hidden)]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// The entries above MARK, outermost first.
    #[doc(hidden)]
    pub fn above(&self, mark: HandlerMark) -> impl Iterator<Item = OwnerHandler> + '_ {
        let keep = mark.depth().min(self.records.len());
        self.records[keep..].iter().copied()
    }

    /// Whether a serial handler sits above MARK.
    #[doc(hidden)]
    pub fn has_serial_above(&self, mark: HandlerMark) -> bool {
        self.above(mark)
            .any(|entry| matches!(entry, OwnerHandler::Serial(_)))
    }

    /// Whether exactly these serial markers are the top run, `tokens[0]`
    /// innermost (the pop order of `unbind_to`). A caller checks every LIFO
    /// it will touch before it pops on any of them.
    #[doc(hidden)]
    pub fn serial_run_matches(&self, tokens: &[SerialHandlerToken]) -> bool {
        self.records
            .len()
            .checked_sub(tokens.len())
            .is_some_and(|keep| {
                self.records[keep..].iter().rev().zip(tokens).all(
                    |(entry, token)| matches!(entry, OwnerHandler::Serial(seen) if *seen == *token),
                )
            })
    }

    /// Remove exactly these serial markers from the top, `tokens[0]` innermost.
    ///
    /// Every entry is compared before any is removed: on a mismatch nothing
    /// changes and the caller has applied nothing.
    #[doc(hidden)]
    pub fn pop_serial_run(&mut self, tokens: &[SerialHandlerToken]) -> Result<usize, ()> {
        if !self.serial_run_matches(tokens) {
            return Err(());
        }
        self.records.truncate(self.records.len() - tokens.len());
        Ok(tokens.len())
    }

    #[doc(hidden)]
    pub fn trace_roots(&self, visit: &mut dyn FnMut(RootGroup, Value)) {
        for record in &self.records {
            if let OwnerHandler::Owner(record) = record {
                record.trace_frame_roots(visit);
            }
        }
    }
}

/// The owner's binding-record stack with typed marks. Entry kinds and the
/// truncation rule are [`HandlerStack`]'s.
///
/// A [`SlotRecord`] here stands for the slab record it names (see the type):
/// a bind pushes the slab record first and this entry second; an unbind
/// takes the slab record first and removes this entry second.
#[derive(Default)]
pub(crate) struct BindStack {
    records: Vec<OwnerRecord>,
}

impl BindStack {
    #[doc(hidden)]
    pub fn new() -> Self {
        Self::default()
    }

    #[doc(hidden)]
    pub fn mark(&self) -> OwnerRecordMark {
        OwnerRecordMark(self.records.len())
    }

    #[doc(hidden)]
    pub fn push(&mut self, record: impl Into<OwnerRecord>) {
        self.records.push(record.into());
    }

    /// Pop the top record; `None` on an empty stack. The `Unbind` walk of
    /// the VM stepper removes one owner binding at a time (V2, R1b).
    #[doc(hidden)]
    pub fn pop(&mut self) -> Option<OwnerRecord> {
        self.records.pop()
    }

    /// Pop every record above MARK; returns how many were removed.
    #[doc(hidden)]
    pub fn truncate(&mut self, mark: OwnerRecordMark) -> usize {
        let keep = mark.depth().min(self.records.len());
        let removed = self.records.len() - keep;
        self.records.truncate(keep);
        removed
    }

    #[doc(hidden)]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// The entries above MARK, outermost first; double-ended, so the top
    /// run can be read from the inner end (`Unbind`, vm.rs) without a copy.
    #[doc(hidden)]
    pub fn above(
        &self,
        mark: OwnerRecordMark,
    ) -> impl DoubleEndedIterator<Item = OwnerRecord> + '_ {
        let keep = mark.depth().min(self.records.len());
        self.records[keep..].iter().copied()
    }

    /// Whether a serial record sits above MARK.
    #[doc(hidden)]
    pub fn has_serial_above(&self, mark: OwnerRecordMark) -> bool {
        self.above(mark)
            .any(|entry| matches!(entry, OwnerRecord::Serial(_)))
    }

    /// Whether exactly these serial markers are the top run; see
    /// [`HandlerStack::serial_run_matches`].
    #[doc(hidden)]
    pub fn serial_run_matches(&self, tokens: &[SerialRecordToken]) -> bool {
        self.records
            .len()
            .checked_sub(tokens.len())
            .is_some_and(|keep| {
                self.records[keep..].iter().rev().zip(tokens).all(
                    |(entry, token)| matches!(entry, OwnerRecord::Serial(seen) if *seen == *token),
                )
            })
    }

    /// Remove exactly these serial markers from the top, `tokens[0]` innermost.
    /// Every entry is compared before any is removed; see
    /// [`HandlerStack::pop_serial_run`].
    #[doc(hidden)]
    pub fn pop_serial_run(&mut self, tokens: &[SerialRecordToken]) -> Result<usize, ()> {
        if !self.serial_run_matches(tokens) {
            return Err(());
        }
        self.records.truncate(self.records.len() - tokens.len());
        Ok(tokens.len())
    }
}

/// A frame's compile-time root schema: the canonical groups it publishes and
/// an exhaustive walk of every heap reference it holds.
pub(crate) trait FrameRoots {
    /// The groups this frame publishes (a subset of [`RootGroup::ALL`]).
    const ROOT_GROUPS: &'static [RootGroup];

    /// Visit every heap reference. Implementations destructure `Self`
    /// without `..`; adding a field is a compile error until classified.
    fn trace_frame_roots(&self, visit: &mut dyn FnMut(RootGroup, Value));
}

/// The tree-walk interpreter's owned frame (E4-DESIGN §5). Values are rooted
/// through [`FrameRoots`]; the depths and marks are frame-local state.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InterpFrame {
    pub(crate) function: Value,
    pub(crate) body: Value,
    pub(crate) cursor: Value,
    pub(crate) lexenv: Value,
    /// The value of the last completed form in this progn. GC-JOIN scans
    /// the arena, so the accumulator lives here rather than beside it.
    pub(crate) last: Value,
    pub(crate) eval_depth: u32,
    pub(crate) handler_base: HandlerMark,
    pub(crate) spec_base: OwnerRecordMark,
    pub(crate) poll_budget: PollBudget,
}

impl InterpFrame {
    #[doc(hidden)]
    pub const fn new(
        function: Value,
        body: Value,
        cursor: Value,
        lexenv: Value,
        last: Value,
        eval_depth: u32,
        handler_base: HandlerMark,
        spec_base: OwnerRecordMark,
        poll_budget: PollBudget,
    ) -> Self {
        Self {
            function,
            body,
            cursor,
            lexenv,
            last,
            eval_depth,
            handler_base,
            spec_base,
            poll_budget,
        }
    }
}

impl FrameRoots for InterpFrame {
    const ROOT_GROUPS: &'static [RootGroup] = &[RootGroup::EvalTemp];

    fn trace_frame_roots(&self, visit: &mut dyn FnMut(RootGroup, Value)) {
        // Exhaustive destructure: a new field fails E0027 here.
        let Self {
            function,
            body,
            cursor,
            lexenv,
            last,
            eval_depth: _,
            handler_base: _,
            spec_base: _,
            poll_budget: _,
        } = self;
        visit(RootGroup::EvalTemp, *function);
        visit(RootGroup::EvalTemp, *body);
        visit(RootGroup::EvalTemp, *cursor);
        visit(RootGroup::EvalTemp, *lexenv);
        visit(RootGroup::EvalTemp, *last);
    }
}

/// The parallel VM's owned frame. The operand segment lives in the arena's
/// operand stack; the arena publishes it in the same group.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VmFrame {
    pub(crate) function: Value,
    pub(crate) code: Value,
    pub(crate) pc: Pc,
    pub(crate) operands: OperandCursor,
    pub(crate) bind_base: OwnerRecordMark,
    pub(crate) handler_base: HandlerMark,
    pub(crate) poll_budget: PollBudget,
}

impl VmFrame {
    #[doc(hidden)]
    pub const fn new(
        function: Value,
        code: Value,
        pc: Pc,
        operands: OperandCursor,
        bind_base: OwnerRecordMark,
        handler_base: HandlerMark,
        poll_budget: PollBudget,
    ) -> Self {
        Self {
            function,
            code,
            pc,
            operands,
            bind_base,
            handler_base,
            poll_budget,
        }
    }
}

impl FrameRoots for VmFrame {
    const ROOT_GROUPS: &'static [RootGroup] = &[RootGroup::VmFrames];

    fn trace_frame_roots(&self, visit: &mut dyn FnMut(RootGroup, Value)) {
        let Self {
            function,
            code,
            pc: _,
            operands: _,
            bind_base: _,
            handler_base: _,
            poll_budget: _,
        } = self;
        visit(RootGroup::VmFrames, *function);
        visit(RootGroup::VmFrames, *code);
    }
}

impl FrameRoots for HandlerRecord {
    const ROOT_GROUPS: &'static [RootGroup] = &[RootGroup::Handlers];

    fn trace_frame_roots(&self, visit: &mut dyn FnMut(RootGroup, Value)) {
        let Self {
            kind: _,
            tag,
            handlers,
            frame: _,
            bind_mark: _,
        } = self;
        visit(RootGroup::Handlers, *tag);
        visit(RootGroup::Handlers, *handlers);
    }
}

/// The detachable frame kinds the arena can hold. The architecture test pins
/// this enumeration against the actual `*Frame` structs in this file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum FrameKind {
    Interp,
    Vm,
}

impl FrameKind {
    #[doc(hidden)]
    pub const ALL: [Self; 2] = [Self::Interp, Self::Vm];

    #[doc(hidden)]
    pub const fn struct_name(self) -> &'static str {
        match self {
            Self::Interp => "InterpFrame",
            Self::Vm => "VmFrame",
        }
    }

    #[doc(hidden)]
    pub const fn root_groups(self) -> &'static [RootGroup] {
        match self {
            Self::Interp => InterpFrame::ROOT_GROUPS,
            Self::Vm => VmFrame::ROOT_GROUPS,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum OwnedFrame {
    Interp(InterpFrame),
    Vm(VmFrame),
}

#[derive(Clone, Copy, Debug)]
struct FrameSlot {
    generation: NonZeroU64,
    frame: OwnedFrame,
}

impl OwnedFrame {
    #[doc(hidden)]
    pub fn kind(self) -> FrameKind {
        match self {
            Self::Interp(_) => FrameKind::Interp,
            Self::Vm(_) => FrameKind::Vm,
        }
    }
}

/// The owner's owned frame arena plus its operand stack, handler and binding
/// record stacks, call-depth record and auxiliary root registry.
pub(crate) struct FrameArena {
    frames: Vec<FrameSlot>,
    operands: Vec<Value>,
    handlers: HandlerStack,
    bindings: BindStack,
    roots: RootRegistry,
    /// Cons and float cursors for this owner. The bump does not touch
    /// `TaggedHeap`'s JIT window.
    region: OwnerAllocRegion,
    depth: CallDepth,
    next_generation: NonZeroU64,
    /// The next [`OwnerScanJoin::publish_scan`] fails before it reserves.
    #[cfg(test)]
    fail_scan_reserve: Cell<bool>,
}

static_assertions::assert_not_impl_any!(FrameArena: Send, Sync);

impl FrameArena {
    #[doc(hidden)]
    pub fn new(depth_limit: u32) -> Self {
        Self {
            frames: Vec::new(),
            operands: Vec::new(),
            handlers: HandlerStack::new(),
            bindings: BindStack::new(),
            roots: RootRegistry::new(),
            region: OwnerAllocRegion::new(),
            depth: CallDepth::new(depth_limit),
            next_generation: NonZeroU64::MIN,
            #[cfg(test)]
            fail_scan_reserve: Cell::new(false),
        }
    }

    #[doc(hidden)]
    pub fn push_interp(&mut self, frame: InterpFrame) -> Result<FrameId, FramePublishError> {
        self.push(OwnedFrame::Interp(frame))
    }

    #[doc(hidden)]
    pub fn push_vm(&mut self, frame: VmFrame) -> Result<FrameId, FramePublishError> {
        self.push(OwnedFrame::Vm(frame))
    }

    #[doc(hidden)]
    pub fn push(&mut self, frame: OwnedFrame) -> Result<FrameId, FramePublishError> {
        self.depth.enter().map_err(FramePublishError::Depth)?;
        let generation = self.next_generation;
        let Some(next) = generation.checked_add(1) else {
            self.depth.leave();
            return Err(FramePublishError::GenerationExhausted);
        };
        self.next_generation = next;
        self.frames.push(FrameSlot { generation, frame });
        let depth = u32::try_from(self.frames.len())
            .ok()
            .and_then(NonZeroU32::new)
            .expect("a frame arena deeper than u32");
        Ok(FrameId { depth, generation })
    }

    /// The interpreter frame ID names, if it is still the live top.
    ///
    /// A stale or non-top handle is `None`. The generation compare happens
    /// before the frame is read.
    #[doc(hidden)]
    pub fn interp(&self, id: FrameId) -> Option<&InterpFrame> {
        let slot = self.frames.last()?;
        if slot.generation != id.generation {
            return None;
        }
        if u32::try_from(self.frames.len()).ok() != Some(id.depth.get()) {
            return None;
        }
        match &slot.frame {
            OwnedFrame::Interp(frame) => Some(frame),
            OwnedFrame::Vm(_) => None,
        }
    }

    /// Update the interpreter frame ID names in place.
    ///
    /// The [`FrameId`] does not change: stage 1 resumes the same handle.
    /// A stale, non-top, or non-interpreter frame is `None` and is not written.
    #[doc(hidden)]
    pub fn with_interp<R>(
        &mut self,
        id: FrameId,
        update: impl FnOnce(&mut InterpFrame) -> R,
    ) -> Option<R> {
        let depth = u32::try_from(self.frames.len()).ok();
        let slot = self.frames.last_mut()?;
        if slot.generation != id.generation {
            return None;
        }
        if depth != Some(id.depth.get()) {
            return None;
        }
        match &mut slot.frame {
            OwnedFrame::Interp(frame) => Some(update(frame)),
            OwnedFrame::Vm(_) => None,
        }
    }

    /// The VM frame ID names, if it is still the live top. Refusal rules are
    /// [`FrameArena::interp`]'s.
    #[doc(hidden)]
    pub fn vm(&self, id: FrameId) -> Option<&VmFrame> {
        let slot = self.frames.last()?;
        if slot.generation != id.generation {
            return None;
        }
        if u32::try_from(self.frames.len()).ok() != Some(id.depth.get()) {
            return None;
        }
        match &slot.frame {
            OwnedFrame::Vm(frame) => Some(frame),
            OwnedFrame::Interp(_) => None,
        }
    }

    /// Update the VM frame ID names in place. Refusal rules are
    /// [`FrameArena::with_interp`]'s.
    #[doc(hidden)]
    pub fn with_vm<R>(&mut self, id: FrameId, update: impl FnOnce(&mut VmFrame) -> R) -> Option<R> {
        let depth = u32::try_from(self.frames.len()).ok();
        let slot = self.frames.last_mut()?;
        if slot.generation != id.generation {
            return None;
        }
        if depth != Some(id.depth.get()) {
            return None;
        }
        match &mut slot.frame {
            OwnedFrame::Vm(frame) => Some(update(frame)),
            OwnedFrame::Interp(_) => None,
        }
    }

    /// The live top VM frame and its segment, in one borrow.
    ///
    /// The executor reads an opcode's operands here. The segment is the
    /// arena's operand vector from the frame's base to its top.
    #[doc(hidden)]
    pub fn vm_segment(&self, id: FrameId) -> Option<&[Value]> {
        let frame = self.vm(id)?;
        Some(self.operand_segment(frame.operands))
    }

    /// The live top frame's segment when it is a VM frame. A poll or escape
    /// handler that holds no [`FrameId`] reads the published operands here.
    #[doc(hidden)]
    pub fn top_vm_segment(&self) -> Option<&[Value]> {
        let OwnedFrame::Vm(frame) = &self.frames.last()?.frame else {
            return None;
        };
        Some(self.operand_segment(frame.operands))
    }

    /// Pop the frame ID names. A stale or non-top handle is refused, exactly
    /// like an owner binding record: the arena is a stack, not a map.
    #[doc(hidden)]
    pub fn pop(&mut self, id: FrameId) -> Option<OwnedFrame> {
        let slot = self.frames.last()?;
        if slot.generation != id.generation {
            return None;
        }
        if u32::try_from(self.frames.len()).ok() != Some(id.depth.get()) {
            return None;
        }
        let slot = self.frames.pop()?;
        self.depth.leave();
        Some(slot.frame)
    }

    #[doc(hidden)]
    pub fn depth(&self) -> u32 {
        self.depth.current()
    }

    /// Live published roots. Revoked slots are not counted.
    #[doc(hidden)]
    pub fn root_len(&self) -> usize {
        self.roots.len()
    }

    #[doc(hidden)]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    #[doc(hidden)]
    pub fn push_operand(&mut self, value: Value) -> OperandSlot {
        let index = self.operands.len();
        self.operands.push(value);
        OperandSlot(index)
    }

    #[doc(hidden)]
    pub fn operand(&self, slot: OperandSlot) -> Option<Value> {
        self.operands.get(slot.index()).copied()
    }

    /// The operand segment CURSOR names, or an empty slice for an out-of-range
    /// cursor (a malformed frame must not panic the root walk).
    #[doc(hidden)]
    pub fn operand_segment(&self, cursor: OperandCursor) -> &[Value] {
        let start = cursor.base();
        let end = start.saturating_add(cursor.len());
        self.operands.get(start..end).unwrap_or(&[])
    }

    /// The top VM frame's cursor and the operand vector, after the
    /// stale/non-top check. Two disjoint borrows, so a mutator updates both in
    /// one call.
    ///
    /// The segment mutators refuse a frame whose segment does not end at the
    /// operand vector's end: only the top frame appends, and a frame whose
    /// cursor was not maintained through these methods is not one this arena
    /// can grow consistently.
    fn top_vm_parts(&mut self, id: FrameId) -> Option<(&mut OperandCursor, &mut Vec<Value>)> {
        let depth = u32::try_from(self.frames.len()).ok();
        let slot = self.frames.last_mut()?;
        if slot.generation != id.generation {
            return None;
        }
        if depth != Some(id.depth.get()) {
            return None;
        }
        let OwnedFrame::Vm(frame) = &mut slot.frame else {
            return None;
        };
        if frame.operands.base() + frame.operands.len() != self.operands.len() {
            return None;
        }
        Some((&mut frame.operands, &mut self.operands))
    }

    /// Append VALUE to the top VM frame's segment. One store site: the value
    /// is published (it is in the operand vector the scan walks) when this
    /// returns.
    #[doc(hidden)]
    pub fn vm_push(&mut self, id: FrameId, value: Value) -> Option<()> {
        let (cursor, operands) = self.top_vm_parts(id)?;
        operands.push(value);
        *cursor = OperandCursor::new(cursor.base(), cursor.len() + 1);
        Some(())
    }

    /// Remove and return the top VM frame's top operand. Never pops below the
    /// frame's base; an empty segment is `None`.
    #[doc(hidden)]
    pub fn vm_pop(&mut self, id: FrameId) -> Option<Value> {
        let (cursor, operands) = self.top_vm_parts(id)?;
        if cursor.len() == 0 {
            return None;
        }
        let value = operands.pop()?;
        *cursor = OperandCursor::new(cursor.base(), cursor.len() - 1);
        Some(value)
    }

    /// Replace the operand OFFSET slots below the top (0 is the top).
    #[doc(hidden)]
    pub fn vm_set_operand(&mut self, id: FrameId, offset: usize, value: Value) -> Option<()> {
        let (cursor, operands) = self.top_vm_parts(id)?;
        if offset >= cursor.len() {
            return None;
        }
        let index = cursor.base() + cursor.len() - 1 - offset;
        *operands.get_mut(index)? = value;
        Some(())
    }

    /// Cut the top VM frame's segment down to LEN operands. LEN above the
    /// current length is refused.
    #[doc(hidden)]
    pub fn vm_truncate_operands(&mut self, id: FrameId, len: usize) -> Option<()> {
        let (cursor, operands) = self.top_vm_parts(id)?;
        if len > cursor.len() {
            return None;
        }
        operands.truncate(cursor.base() + len);
        *cursor = OperandCursor::new(cursor.base(), len);
        Some(())
    }

    #[doc(hidden)]
    pub fn handlers(&self) -> &HandlerStack {
        &self.handlers
    }

    #[doc(hidden)]
    pub fn handlers_mut(&mut self) -> &mut HandlerStack {
        &mut self.handlers
    }

    #[doc(hidden)]
    pub fn bindings(&self) -> &BindStack {
        &self.bindings
    }

    #[doc(hidden)]
    pub fn bindings_mut(&mut self) -> &mut BindStack {
        &mut self.bindings
    }

    #[doc(hidden)]
    pub fn roots_mut(&mut self) -> &mut RootRegistry {
        &mut self.roots
    }

    /// Test seam: install a free-list cons run into this owner's cursors.
    ///
    /// Crate-private and compiled only into this crate's tests. The heap
    /// JIT window is not read or written. A product build has no installer.
    #[cfg(test)]
    #[doc(hidden)]
    pub(crate) fn install_cons_free_list(&mut self, first: usize, cells: usize) {
        self.region.install_cons_free_list(first, cells);
    }

    /// Test seam: install a block-tail cons run into this owner's cursors.
    #[cfg(test)]
    #[doc(hidden)]
    pub(crate) fn install_cons_block_tail(&mut self, first: usize, cells: usize, block: usize) {
        self.region.install_cons_block_tail(first, cells, block);
    }

    /// Test seam: install a float-page bump tail into this owner's cursors.
    #[cfg(test)]
    #[doc(hidden)]
    pub(crate) fn install_float_page_tail(&mut self, first: usize, slots: usize, page: usize) {
        self.region.install_float_page_tail(first, slots, page);
    }

    /// The next cons cell of this owner's run, or `None` when it is exhausted.
    #[doc(hidden)]
    pub fn take_cons_cell(&self) -> Option<usize> {
        self.region.take_cons_cell()
    }

    /// The next float slot of this owner's run, or `None` when it is exhausted.
    #[doc(hidden)]
    pub fn take_float_slot(&self) -> Option<usize> {
        self.region.take_float_slot()
    }

    /// Test seam: install a float-page free-list run into this owner's cursors.
    #[cfg(test)]
    #[doc(hidden)]
    pub(crate) fn install_float_page_free(&mut self, first: usize, slots: usize, page: usize) {
        self.region.install_float_page_free(first, slots, page);
    }

    /// A poll that does not block. The allocation region stays as it is.
    #[doc(hidden)]
    pub fn on_nonblocking_poll(&self) {
        // Closing here would refund the tail and force a refill after every
        // poll that returns without a stop. The region survives on purpose.
    }

    /// Refund this owner's unused tails. Only a publication that precedes a
    /// stop calls this. The shared free list is not written.
    ///
    /// This records the decision and does not apply it. A cursor installed
    /// from a raw address has no reserve, so applying the decision here
    /// would write the shared free list.
    #[doc(hidden)]
    pub fn close_region_for_stop(&mut self) -> OwnerRegionRefund {
        self.region.close_for_stop()
    }

    /// Carve the next cons run of `reserve` into this arena's book.
    ///
    /// A miss returns before the caller publishes and parks. The caller can
    /// then transfer the closed reserve by value, without retaining a mutable
    /// borrow of the active book across the refill rendezvous.
    #[doc(hidden)]
    pub fn carve_granted_cons(
        &mut self,
        heap: &mut TaggedHeap,
        reserve: &mut OwnerEraReserve,
    ) -> Result<(), ReserveError> {
        reserve.carve_cons(heap, &mut self.region)
    }

    /// Close this book and apply the reserve's refund.
    ///
    /// Pacing loses the unhanded tail. The block's `next_index` stays at
    /// the grant end until [`OwnerEraReserve::return_unused_before_sweep`].
    #[doc(hidden)]
    pub fn close_granted_reserve(
        &mut self,
        heap: &mut TaggedHeap,
        reserve: &mut OwnerEraReserve,
    ) -> Result<OwnerRegionRefund, ReserveError> {
        reserve.close_for_stop(heap, &mut self.region)
    }

    /// Publish every root this arena owns, grouped by the canonical E3 names.
    #[doc(hidden)]
    pub fn trace_roots(&self, visit: &mut dyn FnMut(RootGroup, Value)) {
        for slot in &self.frames {
            match &slot.frame {
                OwnedFrame::Interp(frame) => frame.trace_frame_roots(visit),
                OwnedFrame::Vm(frame) => {
                    frame.trace_frame_roots(visit);
                    for value in self.operand_segment(frame.operands) {
                        visit(RootGroup::VmFrames, *value);
                    }
                }
            }
        }
        self.handlers.trace_roots(visit);
        // The record LIFO publishes nothing: an owner binding's values live
        // in the slab record its `SlotRecord` names, and the collector's
        // root phase traces the slab.
        self.roots.trace_roots(visit);
    }

    /// The frame kinds currently present, for the architecture test.
    #[doc(hidden)]
    pub fn frame_kinds(&self) -> impl Iterator<Item = FrameKind> + '_ {
        self.frames.iter().map(|slot| slot.frame.kind())
    }

    /// Park the frame-id counter at its last value so the next push refuses.
    #[cfg(test)]
    pub(crate) fn exhaust_frame_generations_for_test(&mut self) {
        self.next_generation = NonZeroU64::MAX;
    }

    /// The next scan returns [`RendezvousError::Capacity`] and reserves nothing.
    #[cfg(test)]
    pub(crate) fn fail_next_scan_reserve_for_test(&self) {
        self.fail_scan_reserve.set(true);
    }

    /// Visit every owned root once.
    ///
    /// Interpreter fields publish as [`RootGroup::EvalTemp`] because that is
    /// [`InterpFrame::ROOT_GROUPS`]. VM frame fields and the operand stack
    /// publish as [`RootGroup::VmFrames`]. The operand vector is one source:
    /// a frame cursor is a view of it, so the cursor is not walked again.
    /// Handler tags and the registry keep their own groups. The record LIFO
    /// is not a source: a [`SlotRecord`] names the slab record that stores
    /// and roots the binding, and a serial marker roots nothing. The
    /// destructure names every arena field, and [`ArenaScan`] names every
    /// [`RootGroup`], so a new source or group fails to compile until this
    /// scan publishes it.
    fn for_each_owner_root(&self, visit: &mut dyn FnMut(RootGroup, Value)) {
        let Self {
            frames,
            operands,
            handlers,
            bindings: _,
            roots,
            region: _,
            depth: _,
            next_generation: _,
            #[cfg(test)]
                fail_scan_reserve: _,
        } = self;
        for slot in frames {
            match slot.frame {
                OwnedFrame::Interp(frame) => frame.trace_frame_roots(visit),
                OwnedFrame::Vm(frame) => frame.trace_frame_roots(visit),
            }
        }
        for value in operands {
            visit(RootGroup::VmFrames, *value);
        }
        handlers.trace_roots(visit);
        // Registry contribution: every group, including an empty one.
        // `trace_root_groups` is not the owner publication.
        roots.trace_root_groups(&mut |group, values| {
            for value in values {
                visit(group, *value);
            }
        });
    }
}

/// The owner's scan, copied into the `&[SharedRoot]` batch a stop
/// acknowledgement already takes.
///
/// The trait does not acknowledge and carries no epoch. On `Err`, `out` is
/// unchanged and this call leaves no shared root behind.
pub(crate) trait OwnerScanJoin {
    fn publish_scan(
        &self,
        heap: &TaggedHeap,
        out: &mut Vec<SharedRoot>,
    ) -> Result<(), RendezvousError>;
}

/// One vector per canonical group. Destructuring this without `..` is the
/// scan's exactly-once site for [`RootGroup`].
struct ArenaScan {
    vm_frames: Vec<Value>,
    eval_temp: Vec<Value>,
    bc: Vec<Value>,
    jit_window: Vec<Value>,
    handlers: Vec<Value>,
    specpdl: Vec<Value>,
    misc: Vec<Value>,
}

impl ArenaScan {
    fn try_reserve_counts(counts: &[usize; RootGroup::ALL.len()]) -> Result<Self, RendezvousError> {
        let mut vm_frames = Vec::new();
        let mut eval_temp = Vec::new();
        let mut bc = Vec::new();
        let mut jit_window = Vec::new();
        let mut handlers = Vec::new();
        let mut specpdl = Vec::new();
        let mut misc = Vec::new();
        vm_frames
            .try_reserve(counts[RootGroup::VmFrames.ordinal()])
            .map_err(|_| RendezvousError::Capacity)?;
        eval_temp
            .try_reserve(counts[RootGroup::EvalTemp.ordinal()])
            .map_err(|_| RendezvousError::Capacity)?;
        bc.try_reserve(counts[RootGroup::Bc.ordinal()])
            .map_err(|_| RendezvousError::Capacity)?;
        jit_window
            .try_reserve(counts[RootGroup::JitWindow.ordinal()])
            .map_err(|_| RendezvousError::Capacity)?;
        handlers
            .try_reserve(counts[RootGroup::Handlers.ordinal()])
            .map_err(|_| RendezvousError::Capacity)?;
        specpdl
            .try_reserve(counts[RootGroup::Specpdl.ordinal()])
            .map_err(|_| RendezvousError::Capacity)?;
        misc.try_reserve(counts[RootGroup::Misc.ordinal()])
            .map_err(|_| RendezvousError::Capacity)?;
        Ok(Self {
            vm_frames,
            eval_temp,
            bc,
            jit_window,
            handlers,
            specpdl,
            misc,
        })
    }

    fn push(&mut self, group: RootGroup, value: Value) {
        // Capacity was reserved for this group, so `push` does not allocate.
        match group {
            RootGroup::VmFrames => self.vm_frames.push(value),
            RootGroup::EvalTemp => self.eval_temp.push(value),
            RootGroup::Bc => self.bc.push(value),
            RootGroup::JitWindow => self.jit_window.push(value),
            RootGroup::Handlers => self.handlers.push(value),
            RootGroup::Specpdl => self.specpdl.push(value),
            RootGroup::Misc => self.misc.push(value),
        }
    }
}

fn admit_shared(heap: &TaggedHeap, values: &[Value], out: &mut Vec<SharedRoot>) {
    for value in values {
        // SAFETY: `SharedRoot::new` requires a mutator of `heap`, a live
        // value whose traced objects belong to that heap, and reachability
        // until registration completes. The caller is that mutator, and the
        // arena still holds `value` until this function returns.
        out.push(unsafe { SharedRoot::new(heap, *value) });
    }
}

impl OwnerScanJoin for FrameArena {
    fn publish_scan(
        &self,
        heap: &TaggedHeap,
        out: &mut Vec<SharedRoot>,
    ) -> Result<(), RendezvousError> {
        // An open book is not a stop publication. Closing happens only in
        // `close_region_for_stop`, which the caller runs when this publication
        // precedes a stop. `out` stays unchanged.
        if !self.region.is_closed() {
            return Err(RendezvousError::Publication);
        }
        #[cfg(test)]
        if self.fail_scan_reserve.replace(false) {
            return Err(RendezvousError::Capacity);
        }
        let mut counts = [0usize; RootGroup::ALL.len()];
        self.for_each_owner_root(&mut |group, _value| {
            counts[group.ordinal()] += 1;
        });
        let mut scan = ArenaScan::try_reserve_counts(&counts)?;
        self.for_each_owner_root(&mut |group, value| scan.push(group, value));
        let ArenaScan {
            vm_frames,
            eval_temp,
            bc,
            jit_window,
            handlers,
            specpdl,
            misc,
        } = scan;
        let total = vm_frames.len()
            + eval_temp.len()
            + bc.len()
            + jit_window.len()
            + handlers.len()
            + specpdl.len()
            + misc.len();
        let mut local = Vec::new();
        local
            .try_reserve(total)
            .map_err(|_| RendezvousError::Capacity)?;
        admit_shared(heap, &vm_frames, &mut local);
        admit_shared(heap, &eval_temp, &mut local);
        admit_shared(heap, &bc, &mut local);
        admit_shared(heap, &jit_window, &mut local);
        admit_shared(heap, &handlers, &mut local);
        admit_shared(heap, &specpdl, &mut local);
        admit_shared(heap, &misc, &mut local);
        debug_assert_eq!(local.len(), total);
        out.try_reserve(local.len())
            .map_err(|_| RendezvousError::Capacity)?;
        out.extend(local);
        Ok(())
    }
}

/// A position in the arena's operand stack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OperandSlot(usize);

impl OperandSlot {
    #[doc(hidden)]
    pub const fn index(self) -> usize {
        self.0
    }
}
