//! Multi-frame deopt resume: rebuild a chain of Tier-0 frames and run it to
//! completion (design `p2-3-inlining-multiframe-deopt` §4.5).
//!
//! A deopt inside code that inlined calls stops in the middle of several
//! Lisp activations at once: the physical (compiled) frame, suspended at the
//! `Op::Call` it inlined, and one frame per inlined level, the innermost
//! stopped at the op to (re)execute. [`Vm::run_resumed_chain`] turns that
//! into exactly the state Tier-0 would hold had it run the same calls:
//!
//! - every frame's operand stack is seeded into `bc_buf` in Tier-0's own
//!   layout -- a callee's frame directly above its caller's pre-call stack
//!   `[.., F, a1..an]`, F replaced by the exact callee object as Tier-0's
//!   iterative `Bcall` does (`install_exact_callee`, GNU's `fp->fun`), so the
//!   running callee stays alive if its symbol is redefined;
//! - every inlined call has GNU's `Bcall` frame (`record_in_backtrace (F,
//!   args)`, src/bytecode.c:795) at its specpdl position: the frames the
//!   compiled code materialized are re-anchored where they stand, the
//!   virtual ones are pushed with the `Bcall` shape, and `lisp_eval_depth`
//!   counts every level;
//! - each frame's dynamic bindings and condition handlers stay registered,
//!   now owned by its resumed frame.
//!
//! Then the frames run innermost first, each as an ordinary interpreter
//! frame, and each call is completed the way Tier-0 completes it: the
//! callee's own unwind, `lisp_eval_depth--`, the exit debugger if the frame
//! was flagged (its value replaces the call's), the pop, and the value
//! delivered into the caller's consumed function slot (GNU `Breturn`,
//! src/bytecode.c:892-922). A nonlocal exit is offered to the caller's own
//! handlers exactly as the iterative driver offers it to a suspended caller.
//! Every instruction after the deopt therefore runs in the interpreter --
//! the correctness oracle -- in the frame state the interpreter would have.
//!
//! Threading: a chain resume touches only the running mutator's `Context`
//! (its `bc_buf`, specpdl, condition stack and depth); it reads no shared
//! state and publishes none, so concurrent mutators each resume their own.

use super::*;

/// How an inlined frame was entered: the call shape whose frame push, depth
/// accounting and pop a chain resume reproduces. Threading: immutable call
/// protocol metadata, independent of any mutator's Lisp state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChainLink {
    /// GNU `Bcall` (src/bytecode.c:781-831): the caller frame's
    /// `Op::Call(nargs)`. Its frame records the caller's function slot -- the
    /// symbol for a named call, the object for a call of a function value --
    /// with the `nargs` arguments above it, and a depth overflow raises
    /// `Bcall`'s `error`.
    Bcall { nargs: u16 },
}

impl ChainLink {
    /// The arguments the call consumed from its caller's stack.
    pub(crate) fn nargs(self) -> usize {
        match self {
            ChainLink::Bcall { nargs } => nargs as usize,
        }
    }
}

/// An inlined frame's backtrace entry at the deopt point (design §4.1): the
/// state is static per program point in the compiled code. Threading: an index
/// belongs to the resuming mutator's specpdl, never another context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChainBacktrace {
    /// The compiled code pushed the entry at this specpdl index, and
    /// `ctx.depth` counts it.
    Materialized { index: usize },
    /// Nothing was pushed and `ctx.depth` does not count it; the resume
    /// pushes it. Virtual frames have no bindings or handlers of their own
    /// (pushing one is an observation that materializes the frame).
    Virtual,
}

/// One bytecode activation of a chain, in the terms Tier-0 keeps it.
/// Threading: borrowed, temporarily unrooted values belong to the resuming
/// mutator and must be seeded into that Context before any Lisp safepoint.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChainFrame<'s> {
    /// The exact byte-code object running in this frame.
    pub(crate) function: Value,
    /// The innermost frame: the op to execute next. Every other frame: the
    /// `Op::Call` it is suspended at; it resumes at `pc + 1`.
    pub(crate) pc: usize,
    /// The live operand stack. A suspended frame's ends with the call's
    /// function slot and arguments, the function slot holding the call's
    /// designator (what its backtrace frame records).
    pub(crate) stack: &'s [Value],
    /// Condition frames this frame registered, topmost on the condition
    /// stack in frame order; their `stack_len`s are frame-relative.
    pub(crate) handlers: usize,
    /// Pre-push specpdl depths of this frame's outstanding bindings.
    pub(crate) binds: &'s [usize],
}

/// An inlined level of a chain: its frame, how it was called, and where its
/// backtrace entry is. Threading: the frame and specpdl index belong exclusively
/// to the resuming mutator; callers must not transfer them across contexts.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InlinedChainFrame<'s> {
    pub(crate) frame: ChainFrame<'s>,
    pub(crate) link: ChainLink,
    pub(crate) backtrace: ChainBacktrace,
}

/// How [`Vm::run_chain_frame`] enters its frame. Threading: a nonlocal flow is
/// owned by the resuming mutator; the callee does not publish it.
enum ChainFrameEntry {
    /// Interpret from the frame's pc.
    Run,
    /// The frame's callee exited nonlocally: offer the flow to this frame's
    /// handlers first, as the iterative driver does for a suspended caller.
    Raise(Flow),
}

/// The backtrace entry of one inlined call, as the resume must pop it.
/// Threading: each token/index names the resuming mutator's specpdl only.
enum ChainCallFrame {
    /// Pushed by the resume itself, with Tier-0's `Bcall` token.
    Pushed(BytecodeBacktraceFrame),
    /// Pushed by the compiled code at this specpdl index.
    Materialized(usize),
}

/// Physical state after the inner calls have finished. Its operand stack
/// remains in this mutator's traced `bc_buf`, so handing it back to an OSR
/// frame does not create an unrooted Lisp-value transition.
struct SeededChainResume {
    pc: usize,
    entry: ChainFrameEntry,
}

impl<'a> Vm<'a> {
    /// Resume a deopt that stopped inside inlined calls (module doc).
    ///
    /// `physical` is the compiled frame, whose own backtrace frame and depth
    /// were made by its caller; `physical_code` is its function, which
    /// `physical.function` designates (or `nil` for an entry that roots it
    /// elsewhere, as `run_resumed_frame` allows). `inlined` lists the
    /// inlined levels outermost first. `specpdl_base` and
    /// `condition_stack_base` are the physical frame's entry bases, to which
    /// the resume unwinds on every exit. `ctx.depth` must count the physical
    /// frame and the materialized inlined frames, which form a prefix of
    /// `inlined`.
    ///
    /// With `inlined` empty this is `run_resumed_frame`.
    #[cold]
    #[inline(never)]
    pub(crate) fn run_resumed_chain(
        &mut self,
        physical_code: &ByteCodeFunction,
        physical: ChainFrame<'_>,
        inlined: &[InlinedChainFrame<'_>],
        specpdl_base: usize,
        condition_stack_base: usize,
    ) -> EvalResult {
        let frame_base = self.ctx.bc_buf.len();
        let Some(resume) = self.resume_inner_chain(
            physical_code,
            &physical,
            inlined,
            frame_base,
            specpdl_base,
            condition_stack_base,
        ) else {
            return self.abandon_chain(specpdl_base, condition_stack_base);
        };
        self.run_chain_frame(
            physical_code,
            &physical,
            frame_base,
            resume.pc,
            specpdl_base,
            condition_stack_base,
            resume.entry,
        )
    }

    /// Complete only an OSR deopt's inlined calls, leaving the physical
    /// frame in place. The existing interpreter frame keeps ownership of
    /// its bindings, handlers, backtrace entry and final cleanup. On success
    /// its evolved operand stack is at `bc_buf[frame_base..]` and the return
    /// is the next physical pc. A nonlocal exit leaves that stack rooted and
    /// returns the flow for the existing driver's handler dispatch.
    ///
    /// Threading: all values and frame indices belong to this mutator's
    /// Context; the method creates no shared state or thread-local cache.
    #[allow(clippy::too_many_arguments)]
    #[cold]
    #[inline(never)]
    pub(crate) fn run_resumed_chain_in_place(
        &mut self,
        physical_code: &ByteCodeFunction,
        physical: ChainFrame<'_>,
        inlined: &[InlinedChainFrame<'_>],
        frame_base: usize,
        specpdl_base: usize,
        condition_stack_base: usize,
    ) -> Result<usize, Flow> {
        let Some(resume) = self.resume_inner_chain(
            physical_code,
            &physical,
            inlined,
            frame_base,
            specpdl_base,
            condition_stack_base,
        ) else {
            tracing::error!(
                target: "neovm::jit::deopt",
                "OSR inlined-frame chain does not match its code"
            );
            return Err(invalid_bytecode_flow());
        };
        match resume.entry {
            ChainFrameEntry::Run => Ok(resume.pc),
            ChainFrameEntry::Raise(flow) => Err(flow),
        }
    }

    /// Seed the complete chain, then execute and finish its inner calls.
    /// Return the physical frame's rooted state without running or cleaning
    /// it. Ordinary native entries append a physical frame; OSR replaces
    /// the stale segment of the suspended physical frame at `frame_base`.
    #[allow(clippy::too_many_arguments)]
    #[cold]
    #[inline(never)]
    fn resume_inner_chain(
        &mut self,
        physical_code: &ByteCodeFunction,
        physical: &ChainFrame<'_>,
        inlined: &[InlinedChainFrame<'_>],
        frame_base: usize,
        specpdl_base: usize,
        condition_stack_base: usize,
    ) -> Option<SeededChainResume> {
        if frame_base > self.ctx.bc_buf.len() {
            return None;
        }
        let codes = Self::validate_chain(physical_code, physical, inlined)?;
        let mut previous = None;
        let mut materialized = 0;
        for level in inlined {
            if let ChainBacktrace::Materialized { index } = level.backtrace {
                if index < specpdl_base
                    || previous.is_some_and(|outer| index <= outer)
                    || !self.ctx.specpdl_entry_is_backtrace(index)
                {
                    return None;
                }
                previous = Some(index);
                materialized += 1;
            }
        }
        if self.ctx.depth < materialized {
            return None;
        }
        let frame = |i: usize| -> &ChainFrame<'_> {
            if i == 0 {
                physical
            } else {
                &inlined[i - 1].frame
            }
        };
        let k = inlined.len();

        // Seed every operand stack before anything can collect: the values
        // arrive in untraced Rust storage, and `bc_buf` is traced.
        let total: usize = (0..=k).map(|i| frame(i).stack.len()).sum();
        self.ctx.bc_buf.truncate(frame_base);
        self.ctx.bc_buf.reserve(total);
        let mut bases: SmallVec<[usize; 5]> = SmallVec::new();
        let mut function_slots: SmallVec<[usize; 4]> = SmallVec::new();
        for i in 0..=k {
            let top = self.ctx.bc_buf.len();
            if i > 0 {
                // The caller's stack ends `[F, a1..an]`; validated above.
                function_slots.push(top - inlined[i - 1].link.nargs() - 1);
            }
            bases.push(top);
            self.ctx.bc_buf.extend_from_slice(frame(i).stack);
        }

        // Each inlined call's backtrace frame, outermost first (GNU order),
        // then the depth: one level per inlined call.
        let mut calls: SmallVec<[ChainCallFrame; 4]> = SmallVec::new();
        let mut specpdl_bases: SmallVec<[usize; 5]> = SmallVec::new();
        specpdl_bases.push(specpdl_base);
        for (level, &slot) in inlined.iter().zip(function_slots.iter()) {
            let nargs = level.link.nargs();
            let (call, index) = match level.backtrace {
                ChainBacktrace::Virtual => {
                    let designator = self.ctx.bc_buf[slot];
                    let index = self.ctx.specpdl.len();
                    let token =
                        self.ctx
                            .push_backtrace_frame_from_bc_stack(designator, slot + 1, nargs);
                    (ChainCallFrame::Pushed(token), index)
                }
                ChainBacktrace::Materialized { index } => {
                    let rebound = self
                        .ctx
                        .rebind_resumed_backtrace_frame(index, slot + 1, nargs);
                    debug_assert!(
                        rebound,
                        "a materialized inlined frame names its backtrace frame"
                    );
                    (ChainCallFrame::Materialized(index), index)
                }
            };
            // The callee unwinds to just above its own frame, as Tier-0's
            // iterative callee does (`park_in_frame`).
            specpdl_bases.push(index + 1);
            calls.push(call);
            // Tier-0's iterative Bcall replaces the consumed designator with
            // the exact callee (GNU `fp->fun`): the running function stays
            // reachable through the caller's stack even if its symbol is
            // redefined while it runs.
            self.ctx.bc_buf[slot] = level.frame.function;
        }
        debug_assert!(self.ctx.depth >= materialized);
        self.ctx.depth = self.ctx.depth - materialized + k;

        // Rebase each frame's handlers onto its seeded stack.
        let mut condition_bases: SmallVec<[usize; 5]> = SmallVec::new();
        let mut next = condition_stack_base;
        for (i, &base) in bases.iter().enumerate() {
            condition_bases.push(next);
            let handlers = frame(i).handlers;
            if handlers > 0 {
                next = self
                    .ctx
                    .rebase_resumed_vm_handler_range(next, handlers, base);
            }
        }

        if k == 0 {
            return Some(SeededChainResume {
                pc: physical.pc,
                entry: ChainFrameEntry::Run,
            });
        }

        // Run the innermost frame, then complete each call outward. The
        // physical frame is handed back to its owner instead of run here.
        let innermost = frame(k);
        let mut result = self.run_chain_frame(
            codes[k],
            innermost,
            bases[k],
            innermost.pc,
            specpdl_bases[k],
            condition_bases[k],
            ChainFrameEntry::Run,
        );
        for i in (1..=k).rev() {
            // Tier-0's Breturn order (GNU src/bytecode.c:899-902): the
            // callee has unwound to its base; `lisp_eval_depth--`, then the
            // exit debugger if the frame is flagged, then the pop.
            self.leave_bytecode_call_depth();
            result = match calls.pop().expect("one call per inlined level") {
                ChainCallFrame::Pushed(token) => self
                    .ctx
                    .pop_bytecode_backtrace_token_fast_or_slow(token, result),
                ChainCallFrame::Materialized(index) => self
                    .ctx
                    .pop_bytecode_backtrace_frame_with_result(index, result),
            };
            let caller = frame(i - 1);
            let entry = match result {
                Ok(value) => {
                    // The value lands in the consumed function slot.
                    self.ctx.bc_buf.truncate(function_slots[i - 1]);
                    self.ctx.bc_buf.push(value);
                    ChainFrameEntry::Run
                }
                Err(flow) => ChainFrameEntry::Raise(flow),
            };
            if i == 1 {
                return Some(SeededChainResume {
                    pc: caller.pc + 1,
                    entry,
                });
            }
            result = self.run_chain_frame(
                codes[i - 1],
                caller,
                bases[i - 1],
                caller.pc + 1,
                specpdl_bases[i - 1],
                condition_bases[i - 1],
                entry,
            );
        }
        unreachable!("a nonempty chain returns its physical frame")
    }

    /// Check a chain's shape against its code before touching any state:
    /// every inlined function is byte-code, every suspended frame stands at
    /// the `Op::Call` its link names with the call's operands on its stack,
    /// the materialized frames form a prefix, and every stack fits its
    /// frame. Returns each frame's code, physical first.
    #[cold]
    fn validate_chain<'c>(
        physical_code: &'c ByteCodeFunction,
        physical: &ChainFrame<'_>,
        inlined: &[InlinedChainFrame<'_>],
    ) -> Option<SmallVec<[&'c ByteCodeFunction; 5]>> {
        let mut codes: SmallVec<[&'c ByteCodeFunction; 5]> = SmallVec::new();
        codes.push(physical_code);
        for level in inlined {
            codes.push(level.frame.function.get_bytecode_data()?);
        }
        let mut virtual_seen = false;
        for (i, level) in inlined.iter().enumerate() {
            match level.backtrace {
                ChainBacktrace::Virtual => virtual_seen = true,
                ChainBacktrace::Materialized { .. } if virtual_seen => return None,
                ChainBacktrace::Materialized { .. } => {}
            }
            if matches!(level.backtrace, ChainBacktrace::Virtual)
                && (level.frame.handlers > 0 || !level.frame.binds.is_empty())
            {
                return None;
            }
            let caller = if i == 0 {
                physical
            } else {
                &inlined[i - 1].frame
            };
            let nargs = level.link.nargs();
            if caller.stack.len() <= nargs {
                return None;
            }
            match codes[i].executable_ops().get(caller.pc) {
                Some(Op::Call(n)) if *n as usize == nargs => {}
                _ => return None,
            }
        }
        for (i, code) in codes.iter().enumerate() {
            let frame = if i == 0 {
                physical
            } else {
                &inlined[i - 1].frame
            };
            if frame.stack.len() > code.max_stack as usize
                || frame.pc >= code.executable_ops().len()
            {
                return None;
            }
        }
        Some(codes)
    }

    /// A chain whose shape does not match its code is a compiler bug; the
    /// physical frame still owns its bindings and handlers, so unwind them
    /// as its exit would before signaling.
    #[cold]
    #[inline(never)]
    fn abandon_chain(&mut self, specpdl_base: usize, condition_stack_base: usize) -> EvalResult {
        tracing::error!(
            target: "neovm::jit::deopt",
            "inlined-frame chain does not match its code; signaling invalid byte-code"
        );
        self.ctx.truncate_condition_stack(condition_stack_base);
        self.ctx
            .unbind_to_with_result(specpdl_base, Err(invalid_bytecode_flow()))
    }

    /// Run one frame of a chain whose operand stack is already seeded at
    /// `bc_buf[frame_base..]`, as an interpreter entry frame from `pc`, and
    /// unwind it to its bases on every exit (the cleanup
    /// `run_resumed_frame` uses).
    #[allow(clippy::too_many_arguments)]
    #[cold]
    #[inline(never)]
    fn run_chain_frame(
        &mut self,
        code: &ByteCodeFunction,
        frame: &ChainFrame<'_>,
        frame_base: usize,
        pc: usize,
        specpdl_base: usize,
        condition_stack_base: usize,
        entry: ChainFrameEntry,
    ) -> EvalResult {
        self.ctx.bc_frames.push(crate::emacs_core::eval::BcFrame {
            base: frame_base,
            fun: frame.function,
        });
        // Validated: frame_base + max_stack bounds the frame.
        let frame_limit = frame_base + code.max_stack as usize;
        if self.ctx.bc_buf.capacity() < frame_limit {
            self.ctx
                .bc_buf
                .reserve_exact(frame_limit - self.ctx.bc_buf.len());
        }
        let mut pc = pc;
        let mut handlers = HandlerStack::new();
        for _ in 0..frame.handlers {
            handlers.push(Handler::Condition);
        }
        let mut bind_stack: BindStack = frame.binds.iter().copied().collect();
        let result = match entry {
            ChainFrameEntry::Run => self.run_loop(
                code,
                frame_base,
                &mut pc,
                &mut handlers,
                &mut bind_stack,
                false,
            ),
            ChainFrameEntry::Raise(flow) => {
                match self.resume_nonlocal(code, &mut pc, &mut handlers, &mut bind_stack, flow) {
                    Ok(()) => self.run_loop(
                        code,
                        frame_base,
                        &mut pc,
                        &mut handlers,
                        &mut bind_stack,
                        false,
                    ),
                    Err(flow) => Err(flow),
                }
            }
        };
        self.cleanup_bytecode_frame(result, condition_stack_base, specpdl_base, frame_base)
    }
}

#[cfg(test)]
#[path = "tests/resumed_chain_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/osr_chain_test.rs"]
mod osr_tests;
