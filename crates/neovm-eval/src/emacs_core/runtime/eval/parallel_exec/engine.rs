//! Stage-1 tree-walk stepper (E4-DESIGN §9), harness only.
//!
//! The stepper runs on the owned [`FrameArena`]: `progn` and `quote` are
//! interpreter syntax, and every other call is resolved through the cold
//! catalog. Scalar primitives call the serial builtin core. Implemented
//! sealed arms call [`SealedOps`]. Every World arm, unknown name, and
//! unimplemented sealed authority records one [`SerialEscape`] and does not
//! evaluate arguments or run a body.
//!
//! A poll publishes the pending value through [`RootRegistry::push`] and
//! returns that [`RootHandle`]. Resume continues the same [`FrameId`].
//! This module starts no executor thread and is not reachable from serial
//! dispatch.
//!
//! [`OwnerPoll`] is the production E2 safepoint capability: the stepper calls
//! it at the form-return boundary (`poll_interp_call_entry`) and before every
//! serial escape (`poll_serial_request`). It does not replace either existing
//! suspension: the harness `PollBudget` and the [`SerialEscape`] record stay.

#![allow(dead_code)]

use super::dispatch::primitive_slot_by_name;
use super::effective::{
    EffectivePlainKey, EffectiveRead, EffectiveWrite, EraId, OwnerSlotsMut, ShapeGeneration,
    bound_effective, read_effective, write_effective,
};
use super::frames::{
    FrameArena, FrameId, FramePublishError, HandlerMark, InterpFrame, OwnerRecordMark, PollBudget,
    RootGroup, RootHandle, RootPublishError, RootRevokeError,
};
use crate::emacs_core::builtins::{
    builtin_atom_1, builtin_characterp, builtin_consp_1, builtin_floatp_1, builtin_identity,
    builtin_listp_1, builtin_native_comp_available_p, builtin_nlistp_1, builtin_null_1,
    builtin_stringp_1, builtin_treesit_available_p,
};
use crate::emacs_core::builtins_extra::builtin_byteorder;
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::world::poll::PollOutcome;
use crate::emacs_core::subr::effect_schema::{ParallelArm, SealedAuthority, WorldSplit};
use neovm_text::emacs_core::xml::builtin_libxml_available_p;
use neovm_text::emacs_core::zlib::builtin_zlib_available_p;
use neovm_value::error::{EvalResult, Flow, expect_args};
use neovm_value::tagged::SymbolMode;
use neovm_value::tagged::gc::RendezvousError;
use neovm_value::value::Value;
use std::num::NonZeroU32;

/// One serial-escape request. The real production trait arrives with READY-E2B.
pub(crate) trait SerialEscape {
    fn escape(&mut self, request: EscapeRequest);
}

/// Production E2 safepoint capability (E4-DESIGN §5).
///
/// The engine calls these at the edges it already crosses; the impl owns the
/// [`Mutator`](neovm_value::tagged::gc::Mutator) and builds the whole
/// `publish_scan` batch on the slow path. The methods return
/// [`PollOutcome`] and never build or hold a root themselves.
///
/// [`Mutator::poll`](neovm_value::tagged::gc::Mutator::poll) retries `Busy`
/// internally; an `Err` that reaches the engine is a genuine owner refusal
/// and becomes [`EngineFault::Poll`].
///
/// # Publication invariant (R1)
///
/// At every call, each Lisp [`Value`] that must survive the poll is reachable
/// either from the published [`FrameArena`] or from an owner record the
/// production impl includes in its batch. A caller, or a production holder,
/// must not carry an unrooted `Value` across the call.
///
/// The harness satisfies this by construction at its three sites: the form is
/// the top frame's `cursor`, the previous body is the frame's `body`, and an
/// escape's form is reachable from `cursor`. `Stage1`'s `slots`, `ops` and
/// `escape` fields and the `&mut Context` argument are **outside**
/// `publish_scan`; a production executor must publish them or prove them
/// value-free at a poll.
///
/// One existing escape edge shows the boundary of the proof: `eval_arg_list`
/// calls `record` (and so [`OwnerPoll::serial_request`]) while its
/// `args` vector holds earlier arguments. That vector is abandoned on the
/// escape path (the engine drops it without storing it), and every element is
/// a quoted payload or a literal atom that is a sub-object of the frame's
/// cursor form, so nothing in it needs to survive the poll. A caller may not
/// generalize that: a value that is stored after the poll must be rooted
/// before it.
pub(crate) trait OwnerPoll {
    /// Compound-form entry: every form `eval_call` receives, including the
    /// `quote`/`progn` special cases.
    fn interp_call_entry(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError>;

    /// The `progn` body step: one poll per form fetched into a frame's cursor.
    fn interp_back_edge(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError>;

    /// The edge immediately before a serial escape is recorded.
    fn serial_request(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError>;
}

/// Why the harness refused to run a form.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EscapeClass {
    /// The name is not an E1 primitive row.
    UnknownName,
    /// A World arm, including a typed split. Stage 1 runs none of them.
    World(WorldSplit),
    /// A sealed authority this slice does not implement.
    UnimplementedSealed(SealedAuthority),
    /// [`EffectivePlainKey::is_valid_for`] failed. The slot was not read.
    StaleEffectiveKey,
    /// A variable, a non-quote call argument, or a form this slice cannot step.
    UnsupportedForm,
}

/// The form the escape saw. Arguments are unevaluated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EscapeRequest {
    pub class: EscapeClass,
    pub form: Value,
}

/// What a shared serial core returned.
///
/// `MissingCore` means the catalog admitted the name and this engine has no
/// arm that calls the serial builtin. The matrix test fails on that outcome.
#[derive(Debug)]
pub(crate) enum CoreOutcome {
    Value(Value),
    Signal(Flow),
    MissingCore,
}

/// Sealed operations the harness may run.
///
/// [`SealedAuthority::ConsFieldRead`] reads a cons the harness already owns.
/// The impl does not fetch a cons from another owner. This slice installs no
/// generational write barrier and no sealed heap. [`SealedAuthority::ConsFieldWrite`],
/// [`SealedAuthority::ConsAllocation`], [`SealedAuthority::HashRead`], and
/// [`SealedAuthority::CanonicalSymbol`] are outside the impl: the stepper
/// records one escape and runs no body.
pub(crate) trait SealedOps {
    fn read_cons_field(&mut self, ctx: &mut Context, name: &str, args: &[Value]) -> CoreOutcome;

    fn read_header_word(&mut self, ctx: &mut Context, name: &str, args: &[Value]) -> CoreOutcome;
}

/// Why the stepper stopped before a Lisp value.
#[derive(Debug)]
pub(crate) enum EngineFault {
    StaleFrame,
    StaleRoot,
    NotSuspended,
    Busy,
    /// An unwind was refused. The leftover frames stay put, and `start` will not push another.
    Poisoned,
    NoFrame,
    Frame(FramePublishError),
    Root(RootPublishError),
    /// A [`OwnerPoll`] edge refused. `Busy` is retried inside `Mutator::poll`,
    /// so this is a genuine refusal. [`Stage1::settle`] unwinds it to `Idle`.
    Poll(RendezvousError),
}

/// Why a plain slot access stopped before it touched the slot.
#[derive(Debug)]
pub(crate) enum PlainStop {
    /// The key was stale. The same [`EscapeRequest`] was recorded with the
    /// escape handler.
    Escape(EscapeRequest),
    /// The serial-request poll refused. The caller abandons the access; a
    /// driving path unwinds through [`Stage1::settle`].
    Poll(RendezvousError),
}

/// A finished step. `pending` is a [`RootHandle`] from [`RootRegistry::push`].
pub(crate) enum StepOutcome {
    /// The implicit progn finished. `frame` is the frame that produced `pending`.
    Done {
        frame: FrameId,
        pending: RootHandle,
    },
    /// The poll fired and the body still has forms. Resume this same `frame`.
    Suspended {
        frame: FrameId,
        pending: RootHandle,
    },
    Escaped(EscapeRequest),
    MissingCore {
        name: &'static str,
    },
    /// The serial core signaled. This is not an escape and not a substitute value.
    Signaled(Flow),
    Fault(EngineFault),
}

/// Where one harness run is. `Idle` is the only state in which [`Stage1::start`]
/// accepts work. `Finished` holds the done root until [`Stage1::take`].
/// `Poisoned` means an unwind was refused; the leftover stays put.
enum RunState {
    Idle,
    Suspended {
        frame: FrameId,
        handle: RootHandle,
    },
    Escaped {
        frame: FrameId,
        request: EscapeRequest,
    },
    Finished {
        handle: RootHandle,
    },
    Poisoned,
}

struct FrameNote {
    id: FrameId,
}

enum FormStep {
    Value(Value),
    Pushed,
    Stop(StepOutcome),
}

const DEPTH_LIMIT: u32 = 32;

/// The harness stepper. `S` is the owner slot view, `P` the production poll
/// capability. Tests supply the escape and poll doubles and the sealed-ops
/// impl.
pub(crate) struct Stage1<S, E, O, P> {
    arena: FrameArena,
    slots: S,
    escape: E,
    ops: O,
    poll: P,
    era: EraId,
    shape: ShapeGeneration,
    interval: NonZeroU32,
    notes: Vec<FrameNote>,
    state: RunState,
}

impl<S, E, O, P> Stage1<S, E, O, P>
where
    S: OwnerSlotsMut,
    E: SerialEscape,
    O: SealedOps,
    P: OwnerPoll,
{
    pub(crate) fn new(
        slots: S,
        escape: E,
        ops: O,
        poll: P,
        era: EraId,
        shape: ShapeGeneration,
    ) -> Self {
        Self {
            arena: FrameArena::new(DEPTH_LIMIT),
            slots,
            escape,
            ops,
            poll,
            era,
            shape,
            interval: NonZeroU32::MIN,
            notes: Vec::new(),
            state: RunState::Idle,
        }
    }

    pub(crate) fn escape_handler(&self) -> &E {
        &self.escape
    }

    pub(crate) fn poll_handler(&self) -> &P {
        &self.poll
    }

    /// Release the poll capability. An owner-backed impl returns its
    /// [`Mutator`](neovm_value::tagged::gc::Mutator) so the caller retires it.
    pub(crate) fn into_poll(self) -> P {
        self.poll
    }

    pub(crate) fn lookup_root(&mut self, handle: RootHandle) -> Option<Value> {
        self.arena.roots_mut().lookup(handle)
    }

    pub(crate) fn frame_is_live(&self, id: FrameId) -> bool {
        self.arena.interp(id).is_some()
    }

    pub(crate) fn arena_depth(&self) -> u32 {
        self.arena.depth()
    }

    pub(crate) fn live_root_count(&self) -> usize {
        self.arena.root_len()
    }

    /// Pop the root a finished run published. A handle that is not that
    /// root is refused, and the registry is left as it was.
    pub(crate) fn take(&mut self, handle: RootHandle) -> Result<Value, RootRevokeError> {
        let RunState::Finished { handle: done } = self.state else {
            return Err(RootRevokeError::Stale);
        };
        if done != handle {
            return Err(RootRevokeError::Stale);
        }
        let value = self.arena.roots_mut().pop(handle)?;
        self.state = RunState::Idle;
        Ok(value)
    }

    /// Step `forms`, a proper list, as an implicit progn.
    ///
    /// `interval` is the poll budget copied into every frame this run pushes.
    /// One means every completed form polls.
    pub(crate) fn start(
        &mut self,
        ctx: &mut Context,
        forms: Value,
        interval: NonZeroU32,
    ) -> StepOutcome {
        if matches!(self.state, RunState::Poisoned) {
            return StepOutcome::Fault(EngineFault::Poisoned);
        }
        if !matches!(self.state, RunState::Idle) {
            return StepOutcome::Fault(EngineFault::Busy);
        }
        self.interval = interval;
        if let Err(error) = self.push_frame(forms) {
            return StepOutcome::Fault(EngineFault::Frame(error));
        }
        self.drive(ctx)
    }

    /// Continue the suspended frame. Pops the pending root first. The
    /// progn accumulator on the frame is the value that was published.
    pub(crate) fn resume(&mut self, ctx: &mut Context) -> StepOutcome {
        if matches!(self.state, RunState::Poisoned) {
            return StepOutcome::Fault(EngineFault::Poisoned);
        }
        let RunState::Suspended { handle, .. } = self.state else {
            return StepOutcome::Fault(EngineFault::NotSuspended);
        };
        if self.arena.roots_mut().pop(handle).is_err() {
            return match self.unwind_to_idle() {
                Ok(()) => StepOutcome::Fault(EngineFault::StaleRoot),
                Err(fault) => StepOutcome::Fault(fault),
            };
        }
        self.state = RunState::Idle;
        self.drive(ctx)
    }

    pub(crate) fn read_plain(
        &mut self,
        key: &EffectivePlainKey<'_>,
        canonical: impl FnOnce() -> Value,
    ) -> Result<EffectiveRead, PlainStop> {
        self.guard_key(key)?;
        Ok(read_effective(&self.slots, key, canonical))
    }

    pub(crate) fn write_plain(
        &mut self,
        key: &EffectivePlainKey<'_>,
        value: Value,
        canonical: impl FnOnce(Value),
    ) -> Result<EffectiveWrite, PlainStop> {
        self.guard_key(key)?;
        Ok(write_effective(&mut self.slots, key, value, canonical))
    }

    pub(crate) fn bound_plain(
        &mut self,
        key: &EffectivePlainKey<'_>,
        canonical_bound: impl FnOnce() -> bool,
    ) -> Result<bool, PlainStop> {
        self.guard_key(key)?;
        Ok(bound_effective(&self.slots, key, canonical_bound))
    }

    /// Apply the pure key rule. A valid key never polls: its caller may hold
    /// an unrooted value on the Rust stack (for example `write_plain`'s
    /// `value`), and `publish_scan` cannot see it. Only the stale-key path
    /// polls, immediately before recording the escape that abandons the
    /// access; a refusal then records no escape.
    fn guard_key(&mut self, key: &EffectivePlainKey<'_>) -> Result<(), PlainStop> {
        if key.is_valid_for(self.era, self.shape) {
            return Ok(());
        }
        if let Err(error) = self.poll.serial_request(&self.arena) {
            return Err(PlainStop::Poll(error));
        }
        let request = EscapeRequest {
            class: EscapeClass::StaleEffectiveKey,
            form: Value::NIL,
        };
        self.escape.escape(request);
        Err(PlainStop::Escape(request))
    }

    fn drive(&mut self, ctx: &mut Context) -> StepOutcome {
        let outcome = self.drive_inner(ctx);
        self.settle(outcome)
    }

    fn drive_inner(&mut self, ctx: &mut Context) -> StepOutcome {
        loop {
            let Some(id) = self.notes.last().map(|note| note.id) else {
                return StepOutcome::Fault(EngineFault::NoFrame);
            };
            let Some(frame) = self.arena.interp(id) else {
                return StepOutcome::Fault(EngineFault::StaleFrame);
            };
            let cursor_is_nil = frame.cursor.is_nil();
            let body = frame.body;
            if cursor_is_nil && body.is_nil() {
                match self.finish_top() {
                    TopFinish::Done { frame, value } => return self.publish_done(frame, value),
                    TopFinish::Continued { parent } => {
                        if let Some(outcome) = self.after_form(parent) {
                            return outcome;
                        }
                    }
                    TopFinish::Fault(fault) => return StepOutcome::Fault(fault),
                }
                continue;
            }
            if cursor_is_nil {
                if !body.is_cons() {
                    return self.record(EscapeClass::UnsupportedForm, body);
                }
                let form = body.cons_car();
                let rest = body.cons_cdr();
                if self
                    .arena
                    .with_interp(id, |frame| {
                        frame.cursor = form;
                        frame.body = rest;
                    })
                    .is_none()
                {
                    return StepOutcome::Fault(EngineFault::StaleFrame);
                }
                // The `progn` body step. Both live values were just stored on
                // the top frame: `form` is its cursor, `rest` its body, and
                // `publish_scan` walks both. Nothing else is live here.
                if let Err(error) = self.poll.interp_back_edge(&self.arena) {
                    return StepOutcome::Fault(EngineFault::Poll(error));
                }
                continue;
            }
            let form = frame.cursor;
            match self.eval_form(ctx, form) {
                FormStep::Value(value) => {
                    if self
                        .arena
                        .with_interp(id, |frame| {
                            frame.last = value;
                            frame.cursor = Value::NIL;
                        })
                        .is_none()
                    {
                        return StepOutcome::Fault(EngineFault::StaleFrame);
                    }
                    if let Some(outcome) = self.after_form(id) {
                        return outcome;
                    }
                }
                FormStep::Pushed => {}
                FormStep::Stop(outcome) => return outcome,
            }
        }
    }

    fn finish_top(&mut self) -> TopFinish {
        let Some(note) = self.notes.pop() else {
            return TopFinish::Fault(EngineFault::NoFrame);
        };
        let Some(child) = self.arena.interp(note.id) else {
            return TopFinish::Fault(EngineFault::StaleFrame);
        };
        let value = child.last;
        if self.arena.pop(note.id).is_none() {
            return TopFinish::Fault(EngineFault::StaleFrame);
        }
        let Some(parent) = self.notes.last() else {
            return TopFinish::Done {
                frame: note.id,
                value,
            };
        };
        let parent_id = parent.id;
        if self
            .arena
            .with_interp(parent_id, |frame| {
                frame.last = value;
                frame.cursor = Value::NIL;
            })
            .is_none()
        {
            return TopFinish::Fault(EngineFault::StaleFrame);
        }
        TopFinish::Continued { parent: parent_id }
    }

    /// Poll after a completed form, beside the harness suspension. A finished
    /// frame does not suspend.
    fn after_form(&mut self, id: FrameId) -> Option<StepOutcome> {
        // The production poll is unconditional and runs before the budget.
        // A `Resumed` outcome continues this same frame with its budget
        // untouched; the harness `PollBudget` below still owns suspension.
        if let Err(error) = self.poll.interp_call_entry(&self.arena) {
            return Some(StepOutcome::Fault(EngineFault::Poll(error)));
        }
        let more = match self.arena.interp(id) {
            Some(frame) => !frame.body.is_nil(),
            None => return Some(StepOutcome::Fault(EngineFault::StaleFrame)),
        };
        let Some(fired) = self
            .arena
            .with_interp(id, |frame| frame.poll_budget.consume())
        else {
            return Some(StepOutcome::Fault(EngineFault::StaleFrame));
        };
        if !(more && fired) {
            return None;
        }
        let last = self
            .arena
            .interp(id)
            .map(|frame| frame.last)
            .unwrap_or(Value::NIL);
        match self.arena.roots_mut().push(RootGroup::EvalTemp, last) {
            Ok(pending) => {
                self.state = RunState::Suspended {
                    frame: id,
                    handle: pending,
                };
                Some(StepOutcome::Suspended { frame: id, pending })
            }
            Err(error) => Some(StepOutcome::Fault(EngineFault::Root(error))),
        }
    }

    fn publish_done(&mut self, frame: FrameId, value: Value) -> StepOutcome {
        match self.arena.roots_mut().push(RootGroup::EvalTemp, value) {
            Ok(pending) => {
                self.state = RunState::Finished { handle: pending };
                StepOutcome::Done { frame, pending }
            }
            Err(error) => StepOutcome::Fault(EngineFault::Root(error)),
        }
    }

    fn push_frame(&mut self, body: Value) -> Result<(), FramePublishError> {
        let frame = InterpFrame::new(
            Value::NIL,
            body,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            0,
            HandlerMark::EMPTY,
            OwnerRecordMark::EMPTY,
            PollBudget::new(self.interval),
        );
        let id = self.arena.push_interp(frame)?;
        self.notes.push(FrameNote { id });
        Ok(())
    }

    fn eval_form(&mut self, ctx: &mut Context, form: Value) -> FormStep {
        if form.is_cons() {
            return self.eval_call(ctx, form);
        }
        if form.is_symbol(SymbolMode::Bare) && !form.is_nil() && form != Value::T {
            return FormStep::Stop(self.record(EscapeClass::UnsupportedForm, form));
        }
        FormStep::Value(form)
    }

    fn eval_call(&mut self, ctx: &mut Context, form: Value) -> FormStep {
        // Compound-form entry, before head resolution or argument evaluation.
        // `form` is the top frame's cursor, so `publish_scan` already sees it;
        // no argument vector exists yet.
        if let Err(error) = self.poll.interp_call_entry(&self.arena) {
            return FormStep::Stop(StepOutcome::Fault(EngineFault::Poll(error)));
        }
        let head = form.cons_car();
        let tail = form.cons_cdr();
        let Some(name) = head.as_symbol_name(SymbolMode::Bare) else {
            return FormStep::Stop(self.record(EscapeClass::UnsupportedForm, form));
        };
        if name == "quote" {
            return match quoted_payload(tail) {
                Some(value) => FormStep::Value(value),
                None => FormStep::Stop(self.record(EscapeClass::UnsupportedForm, form)),
            };
        }
        if name == "progn" {
            return match self.push_frame(tail) {
                Ok(()) => FormStep::Pushed,
                Err(error) => FormStep::Stop(StepOutcome::Fault(EngineFault::Frame(error))),
            };
        }
        match primitive_slot_by_name(name) {
            None => FormStep::Stop(self.record(EscapeClass::UnknownName, form)),
            Some(slot) => match slot.arm {
                ParallelArm::World(split) => {
                    FormStep::Stop(self.record(EscapeClass::World(split), form))
                }
                ParallelArm::Scalar => self.eval_scalar(ctx, name, form),
                ParallelArm::Sealed(authority) => self.eval_sealed(ctx, name, form, authority),
            },
        }
    }

    fn eval_scalar(&mut self, ctx: &mut Context, name: &'static str, form: Value) -> FormStep {
        let args = match self.eval_arg_list(form.cons_cdr()) {
            Ok(args) => args,
            Err(outcome) => return FormStep::Stop(outcome),
        };
        core_step(name, scalar_core(ctx, name, &args))
    }

    fn eval_sealed(
        &mut self,
        ctx: &mut Context,
        name: &'static str,
        form: Value,
        authority: SealedAuthority,
    ) -> FormStep {
        let implemented = matches!(
            authority,
            SealedAuthority::ConsFieldRead | SealedAuthority::HeaderWord
        );
        if !implemented {
            return FormStep::Stop(self.record(EscapeClass::UnimplementedSealed(authority), form));
        }
        let args = match self.eval_arg_list(form.cons_cdr()) {
            Ok(args) => args,
            Err(outcome) => return FormStep::Stop(outcome),
        };
        let outcome = match authority {
            SealedAuthority::ConsFieldRead => self.ops.read_cons_field(ctx, name, &args),
            SealedAuthority::HeaderWord => self.ops.read_header_word(ctx, name, &args),
            SealedAuthority::ConsFieldWrite
            | SealedAuthority::ConsAllocation
            | SealedAuthority::HashRead
            | SealedAuthority::CanonicalSymbol => {
                return FormStep::Stop(
                    self.record(EscapeClass::UnimplementedSealed(authority), form),
                );
            }
        };
        core_step(name, outcome)
    }

    fn eval_arg_list(&mut self, mut rest: Value) -> Result<Vec<Value>, StepOutcome> {
        let mut args = Vec::new();
        while !rest.is_nil() {
            if !rest.is_cons() {
                return Err(self.record(EscapeClass::UnsupportedForm, rest));
            }
            let arg = rest.cons_car();
            rest = rest.cons_cdr();
            if arg.is_cons() {
                if arg.cons_car().as_symbol_name(SymbolMode::Bare) == Some("quote") {
                    match quoted_payload(arg.cons_cdr()) {
                        Some(value) => args.push(value),
                        None => return Err(self.record(EscapeClass::UnsupportedForm, arg)),
                    }
                    continue;
                }
                return Err(self.record(EscapeClass::UnsupportedForm, arg));
            }
            if arg.is_symbol(SymbolMode::Bare) && !arg.is_nil() && arg != Value::T {
                return Err(self.record(EscapeClass::UnsupportedForm, arg));
            }
            args.push(arg);
        }
        Ok(args)
    }

    fn record(&mut self, class: EscapeClass, form: Value) -> StepOutcome {
        // The serial-request poll runs first. It does not replace the escape:
        // a successful poll still records exactly one `EscapeRequest`.
        if let Err(error) = self.poll.serial_request(&self.arena) {
            return StepOutcome::Fault(EngineFault::Poll(error));
        }
        let request = EscapeRequest { class, form };
        self.escape.escape(request);
        StepOutcome::Escaped(request)
    }

    /// `Done` and `Suspended` already published their state. An escape keeps
    /// the top frame. A signal, a missing core, or a fault drops every frame
    /// and root this run pushed.
    fn settle(&mut self, outcome: StepOutcome) -> StepOutcome {
        match &outcome {
            StepOutcome::Done { .. } | StepOutcome::Suspended { .. } => {}
            StepOutcome::Escaped(request) => {
                if let Some(note) = self.notes.last() {
                    self.state = RunState::Escaped {
                        frame: note.id,
                        request: *request,
                    };
                }
            }
            StepOutcome::Fault(EngineFault::Busy | EngineFault::Poisoned) => {}
            StepOutcome::MissingCore { .. } | StepOutcome::Signaled(_) | StepOutcome::Fault(_) => {
                if let Err(fault) = self.unwind_to_idle() {
                    return StepOutcome::Fault(fault);
                }
            }
        }
        outcome
    }

    /// Drop every frame and root this run pushed. A refused pop or truncate
    /// sets `Poisoned` and leaves the leftover in place. `Idle` is set only
    /// after both succeed.
    fn unwind_to_idle(&mut self) -> Result<(), EngineFault> {
        while let Some(id) = self.notes.last().map(|note| note.id) {
            if self.arena.pop(id).is_none() {
                self.state = RunState::Poisoned;
                return Err(EngineFault::StaleFrame);
            }
            self.notes.pop();
        }
        // truncate(0) is correct only because Stage1 owns its FrameArena exclusively.
        if let Err(error) = self.arena.roots_mut().truncate(0) {
            self.state = RunState::Poisoned;
            return Err(EngineFault::Root(error));
        }
        self.state = RunState::Idle;
        Ok(())
    }

    /// Pop the arena frame and keep its note, so the next unwind is refused.
    #[cfg(test)]
    pub(crate) fn pop_top_frame_keep_note(&mut self) -> bool {
        let Some(id) = self.notes.last().map(|note| note.id) else {
            return false;
        };
        self.arena.pop(id).is_some()
    }
}

enum TopFinish {
    Done { frame: FrameId, value: Value },
    Continued { parent: FrameId },
    Fault(EngineFault),
}

fn quoted_payload(tail: Value) -> Option<Value> {
    if tail.is_cons() && tail.cons_cdr().is_nil() {
        Some(tail.cons_car())
    } else {
        None
    }
}

fn core_step(name: &'static str, outcome: CoreOutcome) -> FormStep {
    match outcome {
        CoreOutcome::Value(value) => FormStep::Value(value),
        CoreOutcome::Signal(flow) => FormStep::Stop(StepOutcome::Signaled(flow)),
        CoreOutcome::MissingCore => FormStep::Stop(StepOutcome::MissingCore { name }),
    }
}

pub(crate) fn apply_one(
    ctx: &mut Context,
    name: &'static str,
    args: &[Value],
    call: fn(&mut Context, Value) -> EvalResult,
) -> CoreOutcome {
    match expect_args(name, args, 1) {
        Ok(()) => core_result(call(ctx, args[0])),
        Err(flow) => CoreOutcome::Signal(flow),
    }
}

pub(crate) fn apply_vec(args: &[Value], call: fn(Vec<Value>) -> EvalResult) -> CoreOutcome {
    core_result(call(args.to_vec()))
}

pub(crate) fn apply_slice(args: &[Value], call: fn(&[Value]) -> EvalResult) -> CoreOutcome {
    core_result(call(args))
}

fn core_result(result: EvalResult) -> CoreOutcome {
    match result {
        Ok(value) => CoreOutcome::Value(value),
        Err(flow) => CoreOutcome::Signal(flow),
    }
}

/// The one Scalar arm. Each admitted primitive calls the serial builtin.
/// A name with no arm is [`CoreOutcome::MissingCore`].
pub(crate) fn scalar_core(ctx: &mut Context, name: &str, args: &[Value]) -> CoreOutcome {
    match name {
        "atom" => apply_one(ctx, "atom", args, builtin_atom_1),
        "byteorder" => apply_vec(args, builtin_byteorder),
        "characterp" => apply_vec(args, builtin_characterp),
        "consp" => apply_one(ctx, "consp", args, builtin_consp_1),
        "floatp" => apply_one(ctx, "floatp", args, builtin_floatp_1),
        "identity" => apply_vec(args, builtin_identity),
        "libxml-available-p" => apply_vec(args, builtin_libxml_available_p),
        "listp" => apply_one(ctx, "listp", args, builtin_listp_1),
        "native-comp-available-p" => apply_vec(args, builtin_native_comp_available_p),
        "nlistp" => apply_one(ctx, "nlistp", args, builtin_nlistp_1),
        "null" => apply_one(ctx, "null", args, builtin_null_1),
        "stringp" => apply_one(ctx, "stringp", args, builtin_stringp_1),
        "treesit-available-p" => apply_vec(args, builtin_treesit_available_p),
        "zlib-available-p" => apply_vec(args, builtin_zlib_available_p),
        _ => CoreOutcome::MissingCore,
    }
}
