//! Test-only probe in the Tier-0 driver for P2.3's F-I2 falsifier (design
//! `p2-3-inlining-multiframe-deopt` §8): build deopt chains from a RUNNING
//! interpreter.
//!
//! Armed, the driver calls [`Vm::resumed_chain_probe`] before every op. It
//! records the op's frame state -- the function, pc, `lisp_eval_depth`,
//! specpdl, condition-stack, operand-stack and JIT-bind-stack lengths, and
//! digests of the specpdl and operand-stack contents -- so that two runs can
//! be compared op for op. At one requested op it instead stops the driver:
//! it snapshots every frame of the driver's call chain the way deopt
//! metadata will describe it (function, pc, operand stack with the call's
//! designator in each function slot, handlers, bindings), puts the context
//! into the state compiled code leaves at a deopt (the inlined frames past a
//! materialized prefix have no backtrace entry and no depth; the
//! materialized ones hold the compact entries a native push writes; the
//! handlers' stack lengths are frame-relative; no operand stack), abandons
//! the driver's frames without unwinding them, and finishes the call with
//! [`Vm::run_resumed_chain`]. A resume that equals Tier-0 leaves every later
//! op's recorded state, and the program's result, unchanged.
//!
//! Heap objects are digested by the order in which the run first saw them,
//! so the digests compare across runs: run with collection inhibited, so no
//! address is reused within a run.

use super::*;
use crate::emacs_core::eval::SpecBinding;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static PROBE: RefCell<Option<ChainProbe>> = const { RefCell::new(None) };
}

/// Whether the driver must call the probe before each op.
#[inline]
pub(crate) fn armed() -> bool {
    ARMED.with(|armed| armed.get())
}

/// Stop the driver at the `op`-th probed op and resume its call chain with
/// its first `materialized` inlined levels materialized.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FireRequest {
    pub(crate) op: usize,
    pub(crate) materialized: usize,
}

/// What became of a [`FireRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Fired {
    /// The run never reached the requested op.
    No,
    /// The chain of `levels` inlined frames above the driver's entry frame
    /// was resumed, the first `materialized` of them materialized.
    Resumed { levels: usize, materialized: usize },
    /// The state at the op cannot be a deopt point with that many
    /// materialized frames (a frame past the prefix holds bindings, handlers
    /// or a flagged entry, or more were requested than exist); the run went
    /// on in Tier-0.
    Refused(&'static str),
}

/// One probed op's frame state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OpState {
    /// The executing function, by first-seen order.
    pub(crate) code: u32,
    pub(crate) pc: usize,
    pub(crate) depth: usize,
    pub(crate) max_depth: usize,
    pub(crate) specpdl_len: usize,
    pub(crate) condition_len: usize,
    pub(crate) bc_len: usize,
    pub(crate) jit_binds: usize,
    pub(crate) lexenv: u64,
    pub(crate) specpdl_digest: u64,
    pub(crate) stack_digest: u64,
}

impl OpState {
    /// The lengths only: what a run that collects garbage can compare (the
    /// digests key objects by address).
    pub(crate) fn shape(&self) -> (u32, usize, usize, usize, usize, usize, usize, usize) {
        (
            self.code,
            self.pc,
            self.depth,
            self.max_depth,
            self.specpdl_len,
            self.condition_len,
            self.bc_len,
            self.jit_binds,
        )
    }
}

pub(crate) struct ChainProbe {
    pub(crate) trace: Vec<OpState>,
    /// The driver's suspended-caller count at each op: the number of levels
    /// a chain stopped there has.
    pub(crate) levels: Vec<usize>,
    pub(crate) fire: Option<FireRequest>,
    pub(crate) fired: Fired,
    identities: HashMap<usize, u64>,
    functions: HashMap<usize, u32>,
    spec_floor: usize,
    bc_floor: usize,
    /// Argument slots the simulated native pushes point into; they outlive
    /// the run, as a native caller's call-args slot outlives its frame.
    native_args: Vec<Box<[i64]>>,
}

/// Arm the probe on this thread: record every op from now on, digesting
/// the specpdl and operand stack above their current lengths.
pub(crate) fn arm(ctx: &crate::emacs_core::eval::Context, fire: Option<FireRequest>) {
    PROBE.with(|probe| {
        *probe.borrow_mut() = Some(ChainProbe {
            trace: Vec::new(),
            levels: Vec::new(),
            fire,
            fired: Fired::No,
            identities: HashMap::new(),
            functions: HashMap::new(),
            spec_floor: ctx.specpdl.len(),
            bc_floor: ctx.bc_buf.len(),
            native_args: Vec::new(),
        });
    });
    ARMED.with(|armed| armed.set(true));
}

/// Disarm and return what the probe saw.
pub(crate) fn disarm() -> ChainProbe {
    ARMED.with(|armed| armed.set(false));
    PROBE
        .with(|probe| probe.borrow_mut().take())
        .expect("the probe was armed")
}

impl ChainProbe {
    fn identity(&mut self, value: Value) -> u64 {
        if !value.is_heap_object() {
            return value.bits() as u64;
        }
        let next = self.identities.len() as u64;
        // Tag the ordinal so it never equals an immediate's bits.
        (*self.identities.entry(value.bits()).or_insert(next) << 3) | 0b111
    }

    fn record(
        &mut self,
        ctx: &crate::emacs_core::eval::Context,
        code: usize,
        pc: usize,
        levels: usize,
    ) {
        let next_function = self.functions.len() as u32;
        let code = *self.functions.entry(code).or_insert(next_function);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for value in &ctx.bc_buf[self.bc_floor.min(ctx.bc_buf.len())..] {
            self.identity(*value).hash(&mut hasher);
        }
        let stack_digest = hasher.finish();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for entry in &ctx.specpdl[self.spec_floor.min(ctx.specpdl.len())..] {
            // A backtrace frame is what a reader sees -- function, arguments,
            // flags -- whichever compact shape holds it.
            if let Some((function, args, debug_on_exit, unevalled)) =
                ctx.backtrace_entry_values(entry)
            {
                "backtrace".hash(&mut hasher);
                self.identity(function).hash(&mut hasher);
                for arg in args {
                    self.identity(arg).hash(&mut hasher);
                }
                (debug_on_exit, unevalled).hash(&mut hasher);
                continue;
            }
            std::mem::discriminant(entry).hash(&mut hasher);
            match entry {
                SpecBinding::Let { sym_id, old_value } => {
                    sym_id.hash(&mut hasher);
                    self.identity(old_value.as_plain()).hash(&mut hasher);
                }
                SpecBinding::LexicalEnv { old_lexenv } => {
                    self.identity(*old_lexenv).hash(&mut hasher);
                }
                SpecBinding::GcRoot { value } => self.identity(*value).hash(&mut hasher),
                _ => {}
            }
        }
        let specpdl_digest = hasher.finish();
        let lexenv = self.identity(ctx.lexenv);
        self.trace.push(OpState {
            code,
            pc,
            depth: ctx.depth,
            max_depth: ctx.max_depth,
            specpdl_len: ctx.specpdl.len(),
            condition_len: ctx.condition_stack.len(),
            bc_len: ctx.bc_buf.len(),
            jit_binds: ctx.jit_bind_stack.len(),
            lexenv,
            specpdl_digest,
            stack_digest,
        });
        self.levels.push(levels);
    }
}

/// One frame of the driver, as deopt metadata would describe it.
struct SnapshotFrame {
    function: Value,
    pc: usize,
    frame_base: usize,
    stack: Vec<Value>,
    handlers: usize,
    binds: Vec<usize>,
    /// Inlined frames: the call that entered it and its backtrace frame's
    /// specpdl index.
    call: Option<(u16, usize)>,
}

impl<'a> Vm<'a> {
    /// The per-op probe (module doc). `Some` when it took the driver's
    /// frames over: the driver returns that result as its entry frame's.
    pub(super) fn resumed_chain_probe(
        &mut self,
        callers: &mut InterpreterCallerStack,
        aux_stack: &mut InterpreterFrameAuxStack,
    ) -> Option<EvalResult> {
        let active = callers.active();
        let code = active.function.code() as *const ByteCodeFunction as usize;
        let pc = active.pc();
        let levels = callers.suspended_len();
        let request = PROBE.with(|probe| {
            let mut probe = probe.borrow_mut();
            let probe = probe.as_mut()?;
            probe.record(self.ctx, code, pc, levels);
            let index = probe.trace.len() - 1;
            let request = probe.fire.filter(|request| request.op == index)?;
            probe.fire = None;
            // The resumed innermost frame executes this op itself, and its
            // probe records it again.
            probe.trace.pop();
            probe.levels.pop();
            Some(request)
        })?;
        match self.snapshot_chain(callers, aux_stack, request.materialized) {
            Ok((frames, spec_base, cond_base)) => {
                PROBE.with(|probe| {
                    if let Some(probe) = probe.borrow_mut().as_mut() {
                        probe.fired = Fired::Resumed {
                            levels,
                            materialized: request.materialized,
                        };
                    }
                });
                Some(self.resume_snapshot(
                    callers,
                    aux_stack,
                    frames,
                    request.materialized,
                    spec_base,
                    cond_base,
                ))
            }
            Err(reason) => {
                PROBE.with(|probe| {
                    if let Some(probe) = probe.borrow_mut().as_mut() {
                        probe.fired = Fired::Refused(reason);
                    }
                });
                None
            }
        }
    }

    /// The driver's frames, entry first, plus the entry frame's specpdl and
    /// condition-stack bases.
    fn snapshot_chain(
        &self,
        callers: &InterpreterCallerStack,
        aux_stack: &InterpreterFrameAuxStack,
        materialized: usize,
    ) -> Result<(Vec<SnapshotFrame>, usize, usize), &'static str> {
        let k = callers.suspended_len();
        if materialized > k {
            return Err("more materialized levels than the chain has");
        }
        let frames = &callers.frames;
        let entry_root = self
            .ctx
            .bc_frames
            .last()
            .ok_or("the entry frame is not context-rooted")?;
        if entry_root.base != frames[0].frame_base {
            return Err("the entry frame is not context-rooted");
        }
        let aux_of = |depth: usize| -> (usize, Vec<usize>) {
            let aux = if depth == k {
                Some(&aux_stack.current)
            } else {
                aux_stack
                    .suspended
                    .iter()
                    .find(|suspended| suspended.depth.0 == depth)
                    .map(|suspended| &suspended.state)
            };
            aux.map_or((0, Vec::new()), |aux| {
                (
                    aux.handlers.len(),
                    aux.bind_stack.as_slice().iter().copied().collect(),
                )
            })
        };
        let mut snapshot = Vec::with_capacity(k + 1);
        for (i, frame) in frames.iter().enumerate() {
            let start = frame.frame_base;
            let end = if i < k {
                frames[i + 1].frame_base
            } else {
                self.ctx.bc_buf.len()
            };
            let mut stack = self.ctx.bc_buf[start..end].to_vec();
            let pc = if i < k { frame.pc() - 1 } else { frame.pc() };
            if i < k {
                let Some(Op::Call(nargs)) = frame.function.code().executable_ops().get(pc) else {
                    return Err("a suspended frame is not at an Op::Call");
                };
                let slot = frames[i + 1].caller_return.stack_after_call();
                if slot + 1 + *nargs as usize != end {
                    return Err("the callee's arguments do not end its caller's stack");
                }
                // The function slot holds the exact callee (Tier-0's
                // install_exact_callee); deopt metadata carries the call's
                // designator there, which the callee's frame records.
                let entry = frames[i + 1].cleanup.specpdl_base - 1;
                let (designator, ..) = self
                    .ctx
                    .backtrace_entry_values(&self.ctx.specpdl[entry])
                    .ok_or("an inlined call has no backtrace frame")?;
                stack[slot - start] = designator;
            }
            let function = if i == 0 {
                entry_root.fun
            } else {
                self.ctx.bc_buf[frames[i - 1 + 1].caller_return.stack_after_call()]
            };
            let (handlers, binds) = aux_of(i);
            let call = (i > 0).then(|| {
                let nargs = match frames[i - 1]
                    .function
                    .code()
                    .executable_ops()
                    .get(frames[i - 1].pc() - 1)
                {
                    Some(Op::Call(nargs)) => *nargs,
                    _ => unreachable!("checked for the caller above"),
                };
                (nargs, frame.cleanup.specpdl_base - 1)
            });
            snapshot.push(SnapshotFrame {
                function,
                pc,
                frame_base: start,
                stack,
                handlers,
                binds,
                call,
            });
        }
        // Frames past the materialized prefix are virtual: no bindings, no
        // handlers, no flag (each is an observation that materializes it),
        // and their entries are the specpdl's top, in call order.
        let virtual_count = k - materialized;
        for (offset, frame) in snapshot[materialized + 1..].iter().enumerate() {
            if frame.handlers > 0 || !frame.binds.is_empty() {
                return Err("a frame past the materialized prefix binds or handles");
            }
            let (_, entry) = frame.call.expect("inlined");
            if self.ctx.backtrace_frame_wants_debug_on_exit(entry) {
                return Err("a frame past the materialized prefix is flagged");
            }
            if entry != self.ctx.specpdl.len() - virtual_count + offset {
                return Err("a virtual frame's entry is not on the specpdl top");
            }
        }
        let spec_base = match (snapshot[0].binds.first(), snapshot.get(1)) {
            (Some(&first), _) => first,
            (None, Some(callee)) => callee.call.expect("inlined").1,
            (None, None) => self.ctx.specpdl.len(),
        };
        let handlers: usize = snapshot.iter().map(|frame| frame.handlers).sum();
        let cond_base = self.ctx.condition_stack.len() - handlers;
        Ok((snapshot, spec_base, cond_base))
    }

    /// Put the context into the state compiled code leaves at a deopt, drop
    /// the driver's frames without unwinding them, and finish the call as a
    /// resumed chain.
    fn resume_snapshot(
        &mut self,
        callers: &mut InterpreterCallerStack,
        aux_stack: &mut InterpreterFrameAuxStack,
        frames: Vec<SnapshotFrame>,
        materialized: usize,
        spec_base: usize,
        cond_base: usize,
    ) -> EvalResult {
        let k = frames.len() - 1;
        let entry_function = callers.frames[0].function;
        // Native handlers record frame-relative stack lengths.
        let mut index = cond_base;
        for frame in &frames {
            let mut remaining = frame.handlers;
            while remaining > 0 {
                if let ConditionFrame::Catch { resume, .. }
                | ConditionFrame::ConditionCase { resume, .. } =
                    &mut self.ctx.condition_stack[index]
                    && let ResumeTarget::VmCatch { stack_len, .. }
                    | ResumeTarget::VmConditionCase { stack_len, .. } = resume
                {
                    *stack_len -= frame.frame_base;
                    remaining -= 1;
                }
                index += 1;
            }
        }
        // Materialized frames hold the compact entries a native push writes
        // (`push_backtrace_frame_from_native_args`), keeping any flag.
        for frame in &frames[1..=materialized] {
            let (_, entry) = frame.call.expect("inlined");
            let (function, args, debug_on_exit, _) = self
                .ctx
                .backtrace_entry_values(&self.ctx.specpdl[entry])
                .expect("an inlined frame's entry");
            let native = match args.as_slice() {
                [arg] => Some(SpecBinding::Backtrace1 {
                    function,
                    arg: *arg,
                    debug_on_exit,
                }),
                [arg0, arg1] if !debug_on_exit => Some(SpecBinding::Backtrace2 {
                    function,
                    arg0: *arg0,
                    arg1: *arg1,
                }),
                args if !debug_on_exit => {
                    // The slot dies with the physical leaf in a real deopt:
                    // poison it, so a resume that still read through it
                    // would show `-1`s.
                    let slot: Box<[i64]> = args
                        .iter()
                        .map(|_| Value::fixnum(-1).bits() as i64)
                        .collect();
                    let args_ptr = slot.as_ptr();
                    PROBE.with(|probe| {
                        probe
                            .borrow_mut()
                            .as_mut()
                            .expect("armed")
                            .native_args
                            .push(slot)
                    });
                    Some(SpecBinding::BacktraceNative {
                        function,
                        args_ptr,
                        nargs: args.len() as u32,
                    })
                }
                _ => None,
            };
            if let Some(native) = native {
                self.ctx.specpdl[entry] = native;
            }
        }
        // Virtual frames: no entry, no depth.
        let len = self.ctx.specpdl.len();
        self.ctx.specpdl.truncate(len - (k - materialized));
        self.ctx.depth -= k - materialized;
        // A native frame keeps no operands on bc_buf.
        self.ctx.bc_buf.truncate(frames[0].frame_base);
        // Abandon the driver's frames: the chain owns them now.
        callers.frames.truncate(1);
        aux_stack.suspended.clear();
        aux_stack.current = InterpreterFrameAux::empty();
        aux_stack.current_occupancy = InterpreterFrameAuxOccupancy::KnownEmpty;

        let physical = ChainFrame {
            function: frames[0].function,
            pc: frames[0].pc,
            stack: &frames[0].stack,
            handlers: frames[0].handlers,
            binds: &frames[0].binds,
        };
        let inlined: Vec<InlinedChainFrame<'_>> = frames[1..]
            .iter()
            .enumerate()
            .map(|(i, frame)| {
                let (nargs, entry) = frame.call.expect("inlined");
                InlinedChainFrame {
                    frame: ChainFrame {
                        function: frame.function,
                        pc: frame.pc,
                        stack: &frame.stack,
                        handlers: frame.handlers,
                        binds: &frame.binds,
                    },
                    link: ChainLink::Bcall { nargs },
                    backtrace: if i < materialized {
                        ChainBacktrace::Materialized { index: entry }
                    } else {
                        ChainBacktrace::Virtual
                    },
                }
            })
            .collect();
        self.run_resumed_chain(
            entry_function.code(),
            physical,
            &inlined,
            spec_base,
            cond_base,
        )
    }
}
