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
use crate::emacs_core::jit::compile::param_shape::JitParamShape;
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

/// The list mapping protocol. Threading: immutable, freely shareable data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i64)]
pub(crate) enum HofKind {
    Mapc = 0,
    Mapcar = 1,
}

impl HofKind {
    pub(crate) fn from_word(word: i64) -> Option<Self> {
        match word {
            0 => Some(Self::Mapc),
            1 => Some(Self::Mapcar),
            _ => None,
        }
    }
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
    /// An in-unit closure's exact running instance, rooted in its caller's
    /// pre-call spill. The template is only a compile-time source identity.
    ClosureBytecode {
        function_slot: u32,
        link: Link,
    },
    /// Reserved for P2.3's HOF producer; v1 readback refuses this kind.
    Hof {
        kind: HofKind,
    },
    /// A list mapping activation whose eager backtrace and root frame are
    /// described by its eight tagged state slots. The callback flag records
    /// whether its GNU Ffuncall entry protocol has already succeeded.
    HofMapping {
        kind: HofKind,
        callback_entered: bool,
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
    /// A suspended mapping builtin is resumed by the HOF consumer, which
    /// completes its current callback before advancing the list cursor.
    pub(crate) hof: Option<HofResume>,
    /// The native cold cell's pc, which indexes the physical leaf's guard
    /// counters. This can differ from the physical caller's resume pc and
    /// the innermost source pc used for feedback.
    pub(crate) guard_site_pc: usize,
    _mutator: core::marker::PhantomData<std::rc::Rc<()>>,
}

/// Owned state of one suspended list mapping activation. The active VM root
/// frame belongs exclusively to the running mutator and keeps its parent
/// stack, cursor and partial results alive; no state crosses mutator threads.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct HofResume {
    pub(crate) kind: HofKind,
    pub(crate) function: Value,
    pub(crate) sequence: Value,
    pub(crate) len: usize,
    pub(crate) tail: Value,
    /// Original argument retained even if the callback mutates the list car.
    pub(crate) item: Value,
    pub(crate) index: usize,
    pub(crate) bt: usize,
    pub(crate) sink_base: usize,
    pub(crate) callback_entered: bool,
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
    /// Production v1 refuses active handlers in every frame. Exact funcall
    /// and list-HOF producers have dedicated protocols; other HOF links and
    /// non-iteratively-enterable ordinary chains are rejected (P2.3 §§4.7, 7).
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
        if self
            .frames
            .iter()
            .any(|frame| matches!(frame.kind, VFrameKind::HofMapping { .. }))
        {
            return self.readback_hof(spill, binds, relocs, ctx, spec_base, guard_site_pc);
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
                .any(|&index| (i > 0 && index < spec_base) || index >= ctx.specpdl.len())
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
                VFrameKind::ClosureBytecode {
                    function_slot,
                    link: Link::Bcall { nargs },
                } => {
                    let function = *spill
                        .get(function_slot as usize)
                        .ok_or(ChainReadError::InvalidCallee)?;
                    (function, nargs)
                }
                VFrameKind::Bytecode { .. } | VFrameKind::ClosureBytecode { .. } => {
                    return Err(ChainReadError::UnsupportedLink);
                }
                VFrameKind::Hof { .. } | VFrameKind::HofMapping { .. } => {
                    return Err(ChainReadError::UnsupportedHof);
                }
                VFrameKind::PhysicalBytecode => return Err(ChainReadError::InvalidPhysical),
            };
            let code = function
                .get_bytecode_data()
                .ok_or(ChainReadError::InvalidCallee)?;
            let params_on_stack = code.lexical || code.arglist.as_fixnum().is_some();
            if code.env.is_some()
                || JitParamShape::try_from(code)
                    .ok()
                    .and_then(JitParamShape::fixed_arity)
                    != Some(nargs as usize)
                || (nargs != 0 && !params_on_stack)
                || !code.executes_sealed_ops()
                || !code.executes_verified_ops()
                || stack.len() > code.max_stack.get()
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
                hof: None,
                guard_site_pc,
                _mutator: core::marker::PhantomData,
            },
        })
    }

    fn readback_hof(
        &self,
        spill: &[Value],
        binds: &[usize],
        relocs: &[Value],
        ctx: &Context,
        spec_base: usize,
        guard_site_pc: usize,
    ) -> Result<ChainReadback, ChainReadError> {
        let [physical, mapping, callback] = self.frames.as_ref() else {
            return Err(ChainReadError::UnsupportedHof);
        };
        let VFrameKind::HofMapping {
            kind,
            callback_entered,
        } = mapping.kind
        else {
            return Err(ChainReadError::UnsupportedHof);
        };
        if self.frames.iter().any(|frame| frame.handlers != 0) {
            return Err(ChainReadError::ActiveHandlers);
        }
        let parent_len = physical.stack.len as usize;
        let callback_start = parent_len
            .checked_add(8)
            .ok_or(ChainReadError::InvalidSpill)?;
        if physical.stack.start != 0
            || parent_len < 3
            || mapping.stack.start as usize != parent_len
            || mapping.stack.len != 8
            || callback.stack.start as usize != callback_start
            || callback_start.checked_add(callback.stack.len as usize) != Some(spill.len())
        {
            return Err(ChainReadError::InvalidSpill);
        }
        if spill
            .iter()
            .any(|value| value.bits() as i64 == super::compile::UNBOXED_FLOAT_TAG_WORD)
        {
            return Err(ChainReadError::UnboxedFloat);
        }
        if mapping.binds != 0
            || callback.binds != 0
            || physical.binds as usize != binds.len()
            || binds.iter().any(|&index| index >= ctx.specpdl.len())
        {
            return Err(ChainReadError::InvalidBinds);
        }
        if mapping.bt != BtState::Virtual || callback.bt != BtState::Virtual {
            return Err(ChainReadError::InvalidBacktrace);
        }
        let state = &spill[parent_len..callback_start];
        let number = |slot: usize| {
            state[slot]
                .as_fixnum()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or(ChainReadError::InvalidSpill)
        };
        let hof = HofResume {
            kind,
            callback_entered,
            function: state[0],
            sequence: state[1],
            len: number(2)?,
            tail: state[3],
            item: state[7],
            index: number(4)?,
            bt: number(5)?,
            sink_base: number(6)?,
        };
        if hof.index >= hof.len
            || !hof.sequence.is_cons()
            || !hof.tail.is_cons()
            || hof.sink_base != parent_len + 1
            || spill[parent_len - 2] != hof.function
            || spill[parent_len - 1] != hof.sequence
        {
            return Err(ChainReadError::InvalidCall);
        }
        let rooted = hof
            .sink_base
            .checked_add(match hof.kind {
                HofKind::Mapc => 0,
                HofKind::Mapcar => hof.len,
            })
            .ok_or(ChainReadError::InvalidSpill)?;
        let roots = ctx
            .vm_frame_root_slots_checked(0, rooted)
            .ok_or(ChainReadError::InvalidSpill)?;
        if roots[..parent_len] != spill[..parent_len] {
            return Err(ChainReadError::InvalidSpill);
        }
        if hof.bt < spec_base
            || !ctx.specpdl_entry_is_backtrace(hof.bt)
            || binds.iter().any(|&index| index >= hof.bt)
        {
            return Err(ChainReadError::InvalidBacktrace);
        }
        match callback.kind {
            VFrameKind::ClosureBytecode {
                function_slot,
                link: Link::HofCallback,
            } if function_slot as usize == parent_len => {}
            VFrameKind::Bytecode {
                func,
                link: Link::HofCallback,
            } => {
                let source = relocs
                    .get(func.0 as usize)
                    .and_then(|value| value.get_bytecode_data())
                    .ok_or(ChainReadError::InvalidCallee)?;
                let running = hof
                    .function
                    .get_bytecode_data()
                    .ok_or(ChainReadError::InvalidCallee)?;
                if super::compile::jit_layout::runtime_identity_word(&source.jit_runtime())
                    != super::compile::jit_layout::runtime_identity_word(&running.jit_runtime())
                {
                    return Err(ChainReadError::InvalidCallee);
                }
            }
            _ => return Err(ChainReadError::UnsupportedLink),
        }
        let code = hof
            .function
            .get_bytecode_data()
            .ok_or(ChainReadError::InvalidCallee)?;
        if JitParamShape::try_from(code)
            .ok()
            .and_then(JitParamShape::fixed_arity)
            != Some(1)
            || !code.executes_sealed_ops()
            || !code.executes_verified_ops()
            || callback.pc as usize >= code.executable_ops().len()
            || callback.stack.len as usize > code.max_stack.get()
            || (!callback_entered && callback.pc != 0)
        {
            return Err(ChainReadError::InvalidCallee);
        }
        Ok(ChainReadback {
            pc: physical.pc as usize,
            stack: spill[..parent_len].to_vec(),
            binds: binds.to_vec(),
            inlined: InlinedResume {
                frames: vec![InlinedFrameResume {
                    function: hof.function,
                    pc: callback.pc as usize,
                    stack: spill[callback_start..].to_vec(),
                    binds: Vec::new(),
                    link: ChainLink::Bcall { nargs: 1 },
                    backtrace: ChainBacktrace::Virtual,
                }]
                .into_boxed_slice(),
                hof: Some(hof),
                guard_site_pc,
                _mutator: core::marker::PhantomData,
            },
        })
    }
}

#[cfg(test)]
#[path = "tests/inline_chain_deopt_test.rs"]
mod tests;
