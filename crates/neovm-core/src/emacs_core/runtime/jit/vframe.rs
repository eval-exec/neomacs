//! Immutable multi-frame deopt metadata (P2.3 §4.4).
//!
//! Threading: metadata contains only indices and counts, is initialized
//! before a leaf is published, and can be shared by compiler workers and
//! mutators. Lisp values live in the leaf's mutator-owned relocation vector,
//! never in metadata. Readback copies them into a mutator-owned payload;
//! that payload must reach `Vm::run_resumed_chain` before a Lisp safepoint.

use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::vm::{ChainBacktrace, ChainLink};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

/// Index of a bytecode object in the leaf's existing, GC-traced relocations.
/// Threading: immutable index, shareable without a Lisp pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RelocIdx(pub(crate) u32);

/// Tagged spill slots owned by one frame. Raw fixnums have been retagged,
/// flonums boxed, and virtual objects rebuilt by the existing cold emitter.
/// Threading: immutable slot counts, shared as part of published metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SpillRange {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// The GNU call protocol that entered an inlined activation.
/// Threading: immutable protocol data, shared as part of published metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Link {
    Bcall { nargs: u16 },
    Funcall { nargs: u16 },
    HofCallback,
}

/// The reserved HOF protocol. Threading: immutable, freely shareable data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HofKind {
    Mapc,
    Mapcar,
}

/// One activation's protocol and relocation index. Threading: immutable
/// data, shared as part of published metadata without Lisp pointers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VFrameKind {
    /// The physical function is supplied by the native call's invoker.
    PhysicalBytecode,
    Bytecode {
        func: RelocIdx,
        link: Link,
    },
    /// Reserved for P2.3's HOF producer; v1 readback refuses this kind.
    Hof {
        kind: HofKind,
    },
}

/// Where a frame's backtrace entry lives. Threading: immutable offset data;
/// only readback resolves it against the current mutator's specpdl.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BtState {
    Physical,
    Virtual,
    /// Index relative to the physical activation's entry specpdl base.
    Materialized {
        spec_offset: u32,
    },
}

/// One frame's immutable deopt state. Threading: initialized before leaf
/// publication and shareable; contains only indices, counts and protocols.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VFrameMeta {
    pub(crate) kind: VFrameKind,
    /// Suspended caller: its Call pc. Innermost: the next op to execute.
    pub(crate) pc: u32,
    pub(crate) stack: SpillRange,
    pub(crate) binds: u16,
    pub(crate) handlers: u16,
    pub(crate) bt: BtState,
}

/// Physical frame first, then inlined activations in GNU call order.
/// Threading: fully initialized, immutable and free of Lisp pointers; a
/// compiler publishes it with its leaf, before any mutator can enter code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeoptChain {
    pub(crate) frames: Box<[VFrameMeta]>,
}

/// One frame's readback values. Threading: these values belong to the
/// running mutator, are temporarily untraced, and must be seeded before GC.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct InlinedFrameResume {
    pub(crate) function: Value,
    pub(crate) pc: usize,
    pub(crate) stack: Vec<Value>,
    pub(crate) binds: Vec<usize>,
    pub(crate) link: ChainLink,
    pub(crate) backtrace: ChainBacktrace,
}

/// Owned readback for the inlined levels. No leaf reference survives:
/// retirement or eviction cannot invalidate the metadata after readback.
/// Threading: this is a single mutator's untraced transition payload. The
/// marker prohibits sending it to a different mutator or compiler worker.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct InlinedResume {
    pub(crate) frames: Box<[InlinedFrameResume]>,
    /// The native cold cell's pc, which indexes the physical leaf's guard
    /// counters. This can differ from the physical caller's resume pc and
    /// the innermost source pc used for feedback.
    pub(crate) guard_site_pc: usize,
    _mutator: core::marker::PhantomData<std::rc::Rc<()>>,
}

/// Physical fields remain in the existing `DeoptResume` envelope.
/// Threading: owned by the running mutator, temporarily untraced and
/// prohibited from crossing threads by its `InlinedResume` payload.
pub(crate) struct ChainReadback {
    pub(crate) pc: usize,
    pub(crate) stack: Vec<Value>,
    pub(crate) binds: Vec<usize>,
    pub(crate) inlined: InlinedResume,
}

/// Defensive metadata rejection. Threading: immutable diagnostic data,
/// shareable without retaining any mutator or Lisp state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChainReadError {
    MissingSite,
    InvalidPhysical,
    ActiveHandlers,
    UnsupportedLink,
    UnsupportedHof,
    Osr,
    InvalidSpill,
    UnboxedFloat,
    InvalidBinds,
    InvalidBacktrace,
    InvalidCallee,
    InvalidCall,
}

impl DeoptChain {
    /// Split already boxed/tagged spills and outstanding JIT bindings into
    /// frame-owned payloads. This runs after native return without a Lisp
    /// allocation or safepoint; Rust buffer allocation does not collect.
    /// Production v1 refuses active handlers in *every* frame, Funcall/HOF
    /// links, and non-iteratively-enterable callees (P2.3 §§4.7, 7).
    pub(crate) fn readback(
        &self,
        spill: &[Value],
        binds: &[usize],
        relocs: &[Value],
        ctx: &Context,
        spec_base: usize,
        guard_site_pc: usize,
    ) -> Result<ChainReadback, ChainReadError> {
        let Some(physical) = self.frames.first() else {
            return Err(ChainReadError::InvalidPhysical);
        };
        if physical.kind != VFrameKind::PhysicalBytecode || physical.bt != BtState::Physical {
            return Err(ChainReadError::InvalidPhysical);
        }
        let mut spill_end = 0usize;
        let mut bind_end = 0usize;
        let mut virtual_seen = false;
        let mut previous_backtrace = None;
        let mut frames: Vec<InlinedFrameResume> = Vec::with_capacity(self.frames.len() - 1);
        for (i, meta) in self.frames.iter().enumerate() {
            if meta.handlers != 0 {
                return Err(ChainReadError::ActiveHandlers);
            }
            let start = meta.stack.start as usize;
            let end = start
                .checked_add(meta.stack.len as usize)
                .ok_or(ChainReadError::InvalidSpill)?;
            if start != spill_end || end > spill.len() {
                return Err(ChainReadError::InvalidSpill);
            }
            let stack = &spill[start..end];
            if stack
                .iter()
                .any(|v| v.bits() as i64 == super::compile::UNBOXED_FLOAT_TAG_WORD)
            {
                return Err(ChainReadError::UnboxedFloat);
            }
            spill_end = end;
            let bind_start = bind_end;
            bind_end = bind_start
                .checked_add(meta.binds as usize)
                .ok_or(ChainReadError::InvalidBinds)?;
            if bind_end > binds.len() {
                return Err(ChainReadError::InvalidBinds);
            }
            let own_binds = &binds[bind_start..bind_end];
            if own_binds
                .iter()
                .any(|&index| index < spec_base || index >= ctx.specpdl.len())
            {
                return Err(ChainReadError::InvalidBinds);
            }
            if i == 0 {
                continue;
            }
            let (function, nargs) = match meta.kind {
                VFrameKind::Bytecode {
                    func,
                    link: Link::Bcall { nargs },
                } => {
                    let function = *relocs
                        .get(func.0 as usize)
                        .ok_or(ChainReadError::InvalidCallee)?;
                    (function, nargs)
                }
                VFrameKind::Bytecode { .. } => return Err(ChainReadError::UnsupportedLink),
                VFrameKind::Hof { .. } => return Err(ChainReadError::UnsupportedHof),
                VFrameKind::PhysicalBytecode => return Err(ChainReadError::InvalidPhysical),
            };
            let code = function
                .get_bytecode_data()
                .ok_or(ChainReadError::InvalidCallee)?;
            let params_on_stack = code.lexical || code.arglist.as_fixnum().is_some();
            if code.env.is_some()
                || code.params.rest.is_some()
                || !code.params.optional.is_empty()
                || code.params.required.len() != nargs as usize
                || (!code.params.required.is_empty() && !params_on_stack)
                || !code.executes_sealed_ops()
                || !code.executes_verified_ops()
                || stack.len() > code.max_stack as usize
                || meta.pc as usize >= code.executable_ops().len()
            {
                return Err(ChainReadError::InvalidCallee);
            }
            let caller = &self.frames[i - 1];
            if caller.stack.len as usize <= nargs as usize {
                return Err(ChainReadError::InvalidCall);
            }
            // Physical source code arrives at the consumer, where Vm validates
            // its Call pc. Every other caller is already resolved here.
            if let Some(previous) = frames.last() {
                let caller_code = previous
                    .function
                    .get_bytecode_data()
                    .ok_or(ChainReadError::InvalidCallee)?;
                if !matches!(caller_code.executable_ops().get(caller.pc as usize),
                    Some(Op::Call(n)) if *n == nargs)
                {
                    return Err(ChainReadError::InvalidCall);
                }
            }
            let backtrace = match meta.bt {
                BtState::Physical => return Err(ChainReadError::InvalidBacktrace),
                BtState::Virtual => {
                    virtual_seen = true;
                    if !own_binds.is_empty() {
                        return Err(ChainReadError::InvalidBinds);
                    }
                    ChainBacktrace::Virtual
                }
                BtState::Materialized { spec_offset } => {
                    let index = spec_base
                        .checked_add(spec_offset as usize)
                        .ok_or(ChainReadError::InvalidBacktrace)?;
                    if virtual_seen
                        || !ctx.specpdl_entry_is_backtrace(index)
                        || previous_backtrace.is_some_and(|previous| index <= previous)
                    {
                        return Err(ChainReadError::InvalidBacktrace);
                    }
                    previous_backtrace = Some(index);
                    ChainBacktrace::Materialized { index }
                }
            };
            frames.push(InlinedFrameResume {
                function,
                pc: meta.pc as usize,
                stack: stack.to_vec(),
                binds: own_binds.to_vec(),
                link: ChainLink::Bcall { nargs },
                backtrace,
            });
        }
        if spill_end != spill.len() || bind_end != binds.len() {
            return Err(ChainReadError::InvalidSpill);
        }
        Ok(ChainReadback {
            pc: physical.pc as usize,
            stack: spill[..physical.stack.len as usize].to_vec(),
            binds: binds[..physical.binds as usize].to_vec(),
            inlined: InlinedResume {
                frames: frames.into_boxed_slice(),
                guard_site_pc,
                _mutator: core::marker::PhantomData,
            },
        })
    }
}

#[cfg(test)]
#[path = "tests/inline_chain_deopt.rs"]
mod tests;
