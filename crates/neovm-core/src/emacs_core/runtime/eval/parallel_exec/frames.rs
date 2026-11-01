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

use super::super::owner_bindings::{BindingKind, Channel, UndoTarget};
use crate::emacs_core::forward::TlsIndex;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::Value;
use std::num::NonZeroU32;

/// The canonical E3 private-root groups (`gc_pacing::trace_root_groups`) the
/// executor publishes. The names are exactly that roster so stage 1 plugs the
/// owner publication in without a second vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RootGroup {
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
    pub(crate) const ALL: [Self; 7] = [
        Self::VmFrames,
        Self::EvalTemp,
        Self::Bc,
        Self::JitWindow,
        Self::Handlers,
        Self::Specpdl,
        Self::Misc,
    ];

    pub(crate) const fn name(self) -> &'static str {
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
}

/// One explicitly published root: an owned value the frame walk does not
/// reach (a pending escape or native obligation, a result awaiting resume).
#[derive(Clone, Copy, Debug)]
pub(crate) struct RootRecord {
    pub(crate) group: RootGroup,
    pub(crate) value: Value,
}

/// The owner's auxiliary published roots, separate from the frame arena.
#[derive(Default)]
pub(crate) struct RootRegistry {
    records: Vec<RootRecord>,
}

impl RootRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Publish VALUE in GROUP and return the token proving it is rooted.
    #[must_use]
    pub(crate) fn push(&mut self, group: RootGroup, value: Value) -> RootedValue {
        self.records.push(RootRecord { group, value });
        RootedValue(value)
    }

    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }

    pub(crate) fn trace_roots(&self, visit: &mut dyn FnMut(RootGroup, Value)) {
        for record in &self.records {
            visit(record.group, record.value);
        }
    }
}

/// A value the registry has published. The private field prevents a caller
/// from claiming an unrooted value; stage 1 hands these across a poll.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RootedValue(Value);

impl RootedValue {
    pub(crate) fn get(self) -> Value {
        self.0
    }
}

/// A bytecode program counter in the owning frame's code vector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Pc(usize);

impl Pc {
    pub(crate) const fn new(position: usize) -> Self {
        Self(position)
    }

    pub(crate) const fn get(self) -> usize {
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
    pub(crate) const fn new(base: usize, len: usize) -> Self {
        Self { base, len }
    }

    pub(crate) const fn base(self) -> usize {
        self.base
    }

    pub(crate) const fn len(self) -> usize {
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
    pub(crate) const fn limit(self) -> u32 {
        self.limit
    }
}

impl CallDepth {
    pub(crate) const fn new(limit: u32) -> Self {
        Self { current: 0, limit }
    }

    pub(crate) const fn current(self) -> u32 {
        self.current
    }

    pub(crate) const fn limit(self) -> u32 {
        self.limit
    }

    fn enter(&mut self) -> Result<u32, DepthLimitExceeded> {
        if self.current >= self.limit {
            return Err(DepthLimitExceeded { limit: self.limit });
        }
        self.current += 1;
        Ok(self.current)
    }

    fn leave(&mut self) {
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
    pub(crate) const fn new(interval: NonZeroU32) -> Self {
        Self {
            remaining: interval.get(),
            interval,
        }
    }

    /// Consume one operation; `true` means this operation reached the poll.
    pub(crate) fn consume(&mut self) -> bool {
        if self.remaining <= 1 {
            self.remaining = self.interval.get();
            true
        } else {
            self.remaining -= 1;
            false
        }
    }

    pub(crate) const fn remaining(self) -> u32 {
        self.remaining
    }
}

/// The depth of a handler stack, so a mark taken before a frame's handlers
/// cannot be confused with a bare index.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct HandlerMark(usize);

impl HandlerMark {
    pub(crate) const EMPTY: Self = Self(0);

    pub(crate) const fn depth(self) -> usize {
        self.0
    }
}

/// The depth of the owner binding-record stack (the specpdl-equivalent
/// record stack E4's frames save and restore).
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct OwnerRecordMark(usize);

impl OwnerRecordMark {
    pub(crate) const EMPTY: Self = Self(0);

    pub(crate) const fn depth(self) -> usize {
        self.0
    }
}

/// A live frame handle: its arena position plus the generation it was issued
/// at, so a stale handle never names a reused slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FrameId {
    depth: NonZeroU32,
    generation: NonZeroU32,
}

impl FrameId {
    pub(crate) const fn generation(self) -> NonZeroU32 {
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
    pub(crate) const fn new(
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

/// The generation a slot record was issued at, so a stale id never names a
/// reused record position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SlotGeneration(NonZeroU32);

impl SlotGeneration {
    pub(crate) const fn new(generation: NonZeroU32) -> Self {
        Self(generation)
    }

    pub(crate) const fn get(self) -> NonZeroU32 {
        self.0
    }
}

/// One owner binding record: the executor's plain mirror of what the E2
/// owner slab stores, carrying the saved value the root walk publishes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SlotRecord {
    pub(crate) symbol: SymId,
    pub(crate) slot: TlsIndex,
    pub(crate) channel: Channel,
    pub(crate) kind: BindingKind,
    pub(crate) undo: UndoTarget,
    pub(crate) generation: SlotGeneration,
}

impl SlotRecord {
    pub(crate) const fn new(
        symbol: SymId,
        slot: TlsIndex,
        channel: Channel,
        kind: BindingKind,
        undo: UndoTarget,
        generation: SlotGeneration,
    ) -> Self {
        Self {
            symbol,
            slot,
            channel,
            kind,
            undo,
            generation,
        }
    }
}

/// The owner's handler stack with typed marks.
#[derive(Default)]
pub(crate) struct HandlerStack {
    records: Vec<HandlerRecord>,
}

impl HandlerStack {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn mark(&self) -> HandlerMark {
        HandlerMark(self.records.len())
    }

    pub(crate) fn push(&mut self, record: HandlerRecord) {
        self.records.push(record);
    }

    /// Pop every record above MARK; returns how many were removed.
    pub(crate) fn truncate(&mut self, mark: HandlerMark) -> usize {
        let keep = mark.depth().min(self.records.len());
        let removed = self.records.len() - keep;
        self.records.truncate(keep);
        removed
    }

    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }

    pub(crate) fn trace_roots(&self, visit: &mut dyn FnMut(RootGroup, Value)) {
        for record in &self.records {
            record.trace_frame_roots(visit);
        }
    }
}

/// The owner's binding-record stack with typed marks.
#[derive(Default)]
pub(crate) struct BindStack {
    records: Vec<SlotRecord>,
}

impl BindStack {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn mark(&self) -> OwnerRecordMark {
        OwnerRecordMark(self.records.len())
    }

    pub(crate) fn push(&mut self, record: SlotRecord) {
        self.records.push(record);
    }

    /// Pop every record above MARK; returns how many were removed.
    pub(crate) fn truncate(&mut self, mark: OwnerRecordMark) -> usize {
        let keep = mark.depth().min(self.records.len());
        let removed = self.records.len() - keep;
        self.records.truncate(keep);
        removed
    }

    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }

    pub(crate) fn trace_roots(&self, visit: &mut dyn FnMut(RootGroup, Value)) {
        for record in &self.records {
            record.trace_frame_roots(visit);
        }
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
    pub(crate) eval_depth: u32,
    pub(crate) handler_base: HandlerMark,
    pub(crate) spec_base: OwnerRecordMark,
    pub(crate) poll_budget: PollBudget,
}

impl InterpFrame {
    pub(crate) const fn new(
        function: Value,
        body: Value,
        cursor: Value,
        lexenv: Value,
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
            eval_depth: _,
            handler_base: _,
            spec_base: _,
            poll_budget: _,
        } = self;
        visit(RootGroup::EvalTemp, *function);
        visit(RootGroup::EvalTemp, *body);
        visit(RootGroup::EvalTemp, *cursor);
        visit(RootGroup::EvalTemp, *lexenv);
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
    pub(crate) const fn new(
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

impl FrameRoots for SlotRecord {
    const ROOT_GROUPS: &'static [RootGroup] = &[RootGroup::Specpdl];

    fn trace_frame_roots(&self, visit: &mut dyn FnMut(RootGroup, Value)) {
        let Self {
            symbol: _,
            slot: _,
            channel: _,
            kind: _,
            undo,
            generation: _,
        } = self;
        match undo {
            UndoTarget::Inherited { suspended } => {
                if let Some(value) = suspended {
                    visit(RootGroup::Specpdl, value.get());
                }
            }
            UndoTarget::Previous(value) | UndoTarget::Captured(value) => {
                visit(RootGroup::Specpdl, value.get());
            }
            UndoTarget::Retired => {}
        }
    }
}

/// The detachable frame kinds the arena can hold. The architecture test pins
/// this enumeration against the actual `*Frame` structs in this file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FrameKind {
    Interp,
    Vm,
}

impl FrameKind {
    pub(crate) const ALL: [Self; 2] = [Self::Interp, Self::Vm];

    pub(crate) const fn struct_name(self) -> &'static str {
        match self {
            Self::Interp => "InterpFrame",
            Self::Vm => "VmFrame",
        }
    }

    pub(crate) const fn root_groups(self) -> &'static [RootGroup] {
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
    generation: NonZeroU32,
    frame: OwnedFrame,
}

impl OwnedFrame {
    fn kind(self) -> FrameKind {
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
    depth: CallDepth,
    next_generation: NonZeroU32,
}

impl FrameArena {
    pub(crate) fn new(depth_limit: u32) -> Self {
        Self {
            frames: Vec::new(),
            operands: Vec::new(),
            handlers: HandlerStack::new(),
            bindings: BindStack::new(),
            roots: RootRegistry::new(),
            depth: CallDepth::new(depth_limit),
            next_generation: NonZeroU32::MIN,
        }
    }

    pub(crate) fn push_interp(
        &mut self,
        frame: InterpFrame,
    ) -> Result<FrameId, DepthLimitExceeded> {
        self.push(OwnedFrame::Interp(frame))
    }

    pub(crate) fn push_vm(&mut self, frame: VmFrame) -> Result<FrameId, DepthLimitExceeded> {
        self.push(OwnedFrame::Vm(frame))
    }

    fn push(&mut self, frame: OwnedFrame) -> Result<FrameId, DepthLimitExceeded> {
        self.depth.enter()?;
        let generation = self.next_generation;
        self.next_generation = generation.checked_add(1).unwrap_or(NonZeroU32::MIN);
        self.frames.push(FrameSlot { generation, frame });
        let depth = u32::try_from(self.frames.len())
            .ok()
            .and_then(NonZeroU32::new)
            .expect("a frame arena deeper than u32");
        Ok(FrameId { depth, generation })
    }

    /// Pop the frame ID names. A stale or non-top handle is refused, exactly
    /// like an owner binding record: the arena is a stack, not a map.
    pub(crate) fn pop(&mut self, id: FrameId) -> Option<OwnedFrame> {
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

    pub(crate) fn depth(&self) -> u32 {
        self.depth.current()
    }

    pub(crate) fn len(&self) -> usize {
        self.frames.len()
    }

    pub(crate) fn push_operand(&mut self, value: Value) -> OperandSlot {
        let index = self.operands.len();
        self.operands.push(value);
        OperandSlot(index)
    }

    pub(crate) fn operand(&self, slot: OperandSlot) -> Option<Value> {
        self.operands.get(slot.index()).copied()
    }

    /// The operand segment CURSOR names, or an empty slice for an out-of-range
    /// cursor (a malformed frame must not panic the root walk).
    pub(crate) fn operand_segment(&self, cursor: OperandCursor) -> &[Value] {
        let start = cursor.base();
        let end = start.saturating_add(cursor.len());
        self.operands.get(start..end).unwrap_or(&[])
    }

    pub(crate) fn handlers_mut(&mut self) -> &mut HandlerStack {
        &mut self.handlers
    }

    pub(crate) fn bindings_mut(&mut self) -> &mut BindStack {
        &mut self.bindings
    }

    pub(crate) fn roots_mut(&mut self) -> &mut RootRegistry {
        &mut self.roots
    }

    /// Publish every root this arena owns, grouped by the canonical E3 names.
    pub(crate) fn trace_roots(&self, visit: &mut dyn FnMut(RootGroup, Value)) {
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
        self.bindings.trace_roots(visit);
        self.roots.trace_roots(visit);
    }

    /// The frame kinds currently present, for the architecture test.
    pub(crate) fn frame_kinds(&self) -> impl Iterator<Item = FrameKind> + '_ {
        self.frames.iter().map(|slot| slot.frame.kind())
    }
}

/// A position in the arena's operand stack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OperandSlot(usize);

impl OperandSlot {
    pub(crate) const fn index(self) -> usize {
        self.0
    }
}
