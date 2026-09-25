//! MIR — a typed SSA intermediate representation for the optimizing **Tier-2**
//! JIT (a tier *above* the Cranelift baseline in `compile.rs`).
//!
//! The baseline tier lowers GNU bytecode straight to CLIF in a single pass, so
//! it can't do lisp-semantic optimization (type specialization, guard
//! elimination, unboxing, inlining) — Cranelift sees only opaque `i64`s. MIR is
//! the layer where those optimizations live: a control-flow graph of SSA
//! operations over lisp `Value`s, each carrying a [`LispType`] fact (the type
//! lattice that lets a proven-fixnum drop its guard / stay unboxed) and an
//! [`Effect`] fact (state access, allocation, reentry and deopt — for reordering and GC
//! safety). Passes run over the MIR, then it lowers to CLIF reusing the
//! baseline tier's shims and precise-deopt emission.
//!
//! **Construction** abstract-interprets the stack machine: each bytecode push
//! becomes a fresh SSA value, the operand stack becomes a vector of [`MirValue`]
//! handles, and block joins use **block parameters as phis** (Cranelift / Swift
//! SIL style — simpler than explicit phi nodes, and the existing
//! [`super::compile::analyze_cfg`] already hands us the block leaders and the
//! operand-stack depth at every block entry).
//!
//! **Status:** wired into the live compile pipeline as the optimizing Tier-2.
//! `super::compile::compile_bytecode_function_inner` builds the MIR for pure
//! required-only bodies and lowers it via `super::compile::lower_mir_pure`, with
//! MIR→CLIF lowering, precise deopt framestates, guard elimination, fixnum
//! unboxing, pure single-block inlining, and cons-escape scalar-replacement all
//! in place; it falls back to the baseline tier on any bail. Handler / `Switch`
//! / `Throw` opcodes are deferred: the builder bails on them (the baseline tier
//! still handles those functions).

use std::collections::HashMap;
use std::fmt;

use super::compile::{CompileError, analyze_cfg, simple_effect};
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::value::Value;

/// An SSA value handle — an index into [`MirFunction`]'s value table. Each MIR
/// instruction result and each block parameter is one `MirValue`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MirValue(pub u32);

/// A basic-block handle — an index into [`MirFunction::blocks`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MirBlockId(pub u32);

/// The lisp-type lattice attached to every [`MirValue`]. `Unknown` is the
/// bottom (no information yet), `Any` the top (could be anything). Concrete
/// types in between let later passes prove a value is a fixnum (drop its guard,
/// keep it unboxed), a cons (skip the consp guard on `car`), etc. — the narrowing
/// the unboxing + guard-elision passes consume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LispType {
    /// No information (a fresh block parameter before inference).
    Unknown,
    Fixnum,
    Nil,
    True,
    Cons,
    Str,
    Symbol,
    Float,
    /// Vector / record / bytecode object / hash-table / …
    Veclike,
    /// Provably a boolean-ish result of a predicate (`t` or `nil`).
    Boolean,
    /// Could be anything (the join of incompatible types).
    Any,
}

impl LispType {
    /// Classify a compile-time constant `Value`.
    pub fn of_value(v: Value) -> LispType {
        if v.is_nil() {
            LispType::Nil
        } else if v == Value::T {
            LispType::True
        } else if v.is_fixnum() {
            LispType::Fixnum
        } else if v.is_cons() {
            LispType::Cons
        } else if v.is_string() {
            LispType::Str
        } else if v.is_symbol() {
            LispType::Symbol
        } else if v.is_float() {
            LispType::Float
        } else {
            LispType::Veclike
        }
    }

    /// True when a value of this type can never be a heap object, so it never
    /// needs operand-stack GC rooting across a call or allocation. This mirrors
    /// exactly what `neovm_jit_gc_push` skips at runtime: immediates (fixnum,
    /// nil, t, predicate booleans) are never heap-allocated, and symbols are
    /// kept live by the obarray (always a GC root), not the operand stack.
    /// `Unknown`/`Any` and every heap type conservatively return false.
    pub fn never_needs_gc_root(self) -> bool {
        matches!(
            self,
            LispType::Fixnum
                | LispType::Nil
                | LispType::True
                | LispType::Boolean
                | LispType::Symbol
        )
    }

    /// True when a value of this type is *definitely* a heap object, so the GC
    /// root push across a call/allocation is unconditionally required. This is
    /// the exact complement partner of [`never_needs_gc_root`] on the KNOWN
    /// types: for a residual with one of these types, an inlined runtime
    /// `is_heap_object` tag test would only ever fall through to the push, so
    /// the caller emits an *unconditional* `neovm_jit_gc_push` and skips the
    /// test. Only `Unknown`/`Any` (neither provably immediate nor provably
    /// heap) are worth an inlined runtime tag test — and both return `false`
    /// from BOTH methods, which is how the codegen distinguishes them.
    pub fn provably_heap(self) -> bool {
        matches!(
            self,
            LispType::Cons | LispType::Str | LispType::Float | LispType::Veclike
        )
    }

    /// The lattice join (least upper bound) used at block-parameter merges:
    /// identical types stay; `Unknown` is the identity; anything else widens to
    /// `Any`. (A richer lattice with unions — `fixnum|nil` etc. — is a later
    /// refinement; Phase 4a keeps it coarse.)
    pub fn join(self, other: LispType) -> LispType {
        match (self, other) {
            (a, b) if a == b => a,
            (LispType::Unknown, b) => b,
            (a, LispType::Unknown) => a,
            _ => LispType::Any,
        }
    }
}

/// Independent effects shared with the call lowering contracts.
pub use super::compile::calls::Effects as Effect;

/// A comparison kind for the fixnum-comparison MIR op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpKind {
    NumEq,
    Lt,
    Gt,
    Le,
    Ge,
}

/// A one-argument predicate kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredKind {
    Null,
    Not,
    Consp,
    Stringp,
    Listp,
    Symbolp,
    Integerp,
    Numberp,
}

/// A one-argument fixnum unary kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryKind {
    Add1,
    Sub1,
    Negate,
}

/// A two-argument fixnum binary kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinKind {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Max,
    Min,
}

/// One MIR instruction (the operation; the SSA result handle and its type/effect
/// live alongside in [`MirInst`]). The optimization-relevant ops — arithmetic,
/// type predicates, `car`/`cdr`, `cons`, `eq` — are modelled explicitly so
/// passes can reason about them. Everything else is wrapped [`MirOp::Opaque`],
/// which preserves operand-stack discipline (via [`simple_effect`]) without yet
/// modelling the op's semantics; later phases promote opaque ops to explicit
/// ones as the optimizer learns to handle them.
#[derive(Clone, Debug)]
pub enum MirOp {
    /// Function argument `i` (`0..arity`), seeding the entry block's stack.
    Arg(usize),
    /// The original call boundary before an inlined body, including an
    /// identity callee with no instructions. Carries the call's framestate.
    InlineEntry,
    /// A compile-time constant from the function's constant pool.
    Const(Value),
    /// Fixnum-fast-path binary arithmetic (guards emitted at CLIF lowering).
    Bin(BinKind, MirValue, MirValue),
    /// Fixnum-fast-path unary arithmetic.
    Unary(UnaryKind, MirValue),
    /// Fixnum comparison → `t`/`nil`.
    Cmp(CmpKind, MirValue, MirValue),
    /// One-argument type predicate → `t`/`nil`.
    Pred(PredKind, MirValue),
    /// `eq` (identity) → `t`/`nil`.
    Eq(MirValue, MirValue),
    /// `car`/`cdr`; `safe` is the `*-safe` variant (non-cons → nil, no guard).
    CarCdr {
        cdr: bool,
        safe: bool,
        arg: MirValue,
    },
    /// `(cons car cdr)` — allocates.
    Cons(MirValue, MirValue),
    /// Any opcode not yet modelled explicitly. `args` are the popped operands
    /// (innermost last); the result count is implied by [`simple_effect`].
    /// `op` is kept verbatim so lowering can reuse the baseline emission.
    Opaque { op: Op, args: Vec<MirValue> },
}

/// A complete MIR instruction: the operation plus its SSA result, type fact, and
/// effect fact.
#[derive(Clone, Debug)]
pub struct MirInst {
    pub result: MirValue,
    pub op: MirOp,
    pub ty: LispType,
    pub effect: Effect,
    /// Bytecode index of the op that produced this inst — the pc a precise deopt
    /// resumes the interpreter at. (Synthetic `Arg` insts use 0; never deopt sites.)
    pub pc: usize,
    /// The operand stack (SSA values) just BEFORE this op executed — the framestate
    /// a precise deopt spills (the interpreter reruns the op at `pc` with this
    /// stack) and the full residual stack an `Opaque` (call) delegation needs.
    /// Captured from build_mir's ACCURATE model stack, which includes the inst-less
    /// Pop/Dup/StackRef/StackSet/DiscardN folds the SSA representation otherwise
    /// loses (without this, the reconstructed framestate would be a silent
    /// miscompile).
    pub pre_stack: Vec<MirValue>,
}

/// A block terminator. Successor edges carry the live operand stack as argument
/// lists (block-params-as-phis): each `*_args` vector matches the target block's
/// parameter list one-for-one.
#[derive(Clone, Debug)]
pub enum MirTerm {
    /// Return the top of stack.
    Return(MirValue),
    /// Unconditional branch.
    Goto {
        target: MirBlockId,
        args: Vec<MirValue>,
    },
    /// Conditional branch on `cond` being nil. `nil_else_pop` marks the
    /// `GotoIf*ElsePop` variants where the taken edge keeps the tested value on
    /// the stack and the fall-through pops it.
    Branch {
        cond: MirValue,
        /// True for the `GotoIfNil*` family (branch when nil); false for
        /// `GotoIfNotNil*` (branch when non-nil).
        on_nil: bool,
        else_pop: bool,
        taken: MirBlockId,
        taken_args: Vec<MirValue>,
        fallthrough: MirBlockId,
        fallthrough_args: Vec<MirValue>,
    },
}

/// A basic block: its parameters (the operand stack at entry — phis), its
/// straight-line instructions, and its terminator.
#[derive(Clone, Debug)]
pub struct MirBlockData {
    /// The bytecode index this block starts at (its leader) — the anchor for
    /// deopt framestates.
    pub bytecode_pc: usize,
    /// Entry operand stack = block parameters (one `MirValue` per slot).
    pub params: Vec<MirValue>,
    pub insts: Vec<MirInst>,
    pub term: MirTerm,
}

/// A function in MIR form: a CFG of [`MirBlockData`] over an SSA value space.
/// `value_types[v.0]` is the [`LispType`] of value `v`.
#[derive(Clone, Debug)]
pub struct MirFunction {
    pub arity: usize,
    pub blocks: Vec<MirBlockData>,
    /// Type fact per SSA value (block params + instruction results), indexed by
    /// `MirValue`.
    pub value_types: Vec<LispType>,
    /// Map from a bytecode leader index to its [`MirBlockId`].
    pub block_for: HashMap<usize, MirBlockId>,
    /// The body's constant pool: an [`MirOp::Opaque`] keeps its bytecode `op`
    /// verbatim, and a `VarRef(idx)`/`CallBuiltinSym(idx, ..)` indexes THIS
    /// pool when the baseline's emitter lowers it. (An inlined callee never
    /// carries an `Opaque`, so no spliced op indexes the wrong pool.)
    pub constants: Box<[Value]>,
    /// Function epoch used by inline-entry guards in reentrant bodies.
    /// Pure bodies retain the cache's entry validation; AOT never inlines.
    pub(crate) inline_epoch: Option<u64>,
}

impl MirFunction {
    pub fn value_type(&self, v: MirValue) -> LispType {
        self.value_types[v.0 as usize]
    }
}

/// Unmodelled instructions conservatively include arbitrary Lisp reentry.
/// A named builtin's fast-path contract never weakens its fallback effects.
fn opaque_effect(op: &Op) -> Effect {
    super::compile::calls::named_builtin_call(op).map_or(Effect::UNKNOWN, |call| call.effects)
}

/// Builder state: the growing SSA value space + per-value types.
struct Builder {
    value_types: Vec<LispType>,
}

impl Builder {
    fn fresh(&mut self, ty: LispType) -> MirValue {
        let v = MirValue(self.value_types.len() as u32);
        self.value_types.push(ty);
        v
    }
}

/// Build the MIR for a leaf bytecode body, or bail (`Err`) for anything the
/// Phase 4a builder doesn't model yet (handler / `Switch` / `Throw` opcodes, or
/// any op `simple_effect` rejects). On success the returned [`MirFunction`] is a
/// structurally-valid SSA CFG mirroring the bytecode's blocks.
///
/// Deliberately mirrors [`super::compile::analyze_cfg`]'s block + terminator
/// model so the two agree on structure (a later phase will assert this).
///
/// Feedback-blind: every arithmetic site is typed as if `FixnumOnly`. Exact
/// for a callee resolved for inlining and for the AOT path, because every MIR
/// `Bin` guards both operands and range-checks its result — the value IS a
/// fixnum or the body deopts. The tier-up path passes the body's real
/// feedback through [`build_mir_with_feedback`].
pub fn build_mir(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
) -> Result<MirFunction, CompileError> {
    build_mir_with_feedback(ops, constants, arity, &|_| NumericFeedback::FixnumOnly)
}

/// [`build_mir`] with the body's per-site numeric feedback.
///
/// `feedback(pc)` is read HERE, at build time, where `pc` still indexes this
/// body's ops — after [`inline_pure_single_block_callees`] a spliced callee
/// inst carries the CALL SITE's pc, so a consumer that re-derived it from
/// `inst.pc` would index the wrong body. The one rule it applies is the
/// baseline analysis's (`apply_known_fixnum_op`): `+ - * /` at a `Float` site
/// have an f64 lowering whose result is a boxed float on the float path and a
/// fixnum on the both-fixnum path, so the result is typed `Any`, not `Fixnum`,
/// and the block-parameter inference never proves a float a fixnum.
/// `% max min` and `1+ 1- -` have no float lowering (both operands guarded,
/// result range-checked) and stay `Fixnum`.
pub fn build_mir_with_feedback(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    feedback: &dyn Fn(usize) -> NumericFeedback,
) -> Result<MirFunction, CompileError> {
    // Reuse the baseline CFG analysis (block leaders + entry stack depths).
    // No GNU byte-offset map: a `Switch` here bails anyway (unmodelled in 4a).
    let cfg = analyze_cfg(ops, constants, None, arity)?;
    let n = ops.len();

    let mut b = Builder {
        value_types: Vec::new(),
    };

    // Assign a block id per leader, and create its parameters. Block 0's params
    // are the function arguments; every other block's params are phis (typed
    // Unknown until inference).
    let mut block_for: HashMap<usize, MirBlockId> = HashMap::new();
    let mut block_params: HashMap<usize, Vec<MirValue>> = HashMap::new();
    for (bid, &leader) in cfg.leaders.iter().enumerate() {
        block_for.insert(leader, MirBlockId(bid as u32));
        // `analyze_cfg` propagates entry depths forward from block 0, so an
        // UNREACHABLE leader (dead code — e.g. a block after an unconditional
        // jump that nothing targets) has no entry depth. The baseline tolerates
        // this (`entry_depth.get(&l).unwrap_or(0)` — depth-0 dead code); the MIR
        // builder bails instead, so such bodies fall back to the baseline rather
        // than build a CFG with a malformed (depthless) block.
        let depth = match cfg.entry_depth.get(&leader) {
            Some(&d) => d,
            None => return Err(CompileError::UnsupportedOp("mir-unreachable-block")),
        };
        let params: Vec<MirValue> = (0..depth)
            .map(|i| {
                if leader == 0 {
                    let v = b.fresh(LispType::Any);
                    // arg seeding handled below as Arg insts; param IS the arg.
                    let _ = i;
                    v
                } else {
                    b.fresh(LispType::Unknown)
                }
            })
            .collect();
        block_params.insert(leader, params);
    }

    let mut blocks: Vec<MirBlockData> = Vec::with_capacity(cfg.leaders.len());

    for (leader_index, &leader) in cfg.leaders.iter().enumerate() {
        let params = block_params[&leader].clone();
        let mut stack: Vec<MirValue> = params.clone();
        let mut insts: Vec<MirInst> = Vec::new();

        // Entry block: emit Arg insts so the params have a definition. We model
        // each argument param as its own Arg instruction whose result IS the
        // param value (the value was already allocated as the param).
        if leader == 0 {
            for (i, &p) in params.iter().enumerate() {
                insts.push(MirInst {
                    result: p,
                    op: MirOp::Arg(i),
                    ty: LispType::Any,
                    effect: Effect::PURE,
                    pc: 0,
                    pre_stack: Vec::new(),
                });
            }
        }

        let end = cfg.leaders.get(leader_index + 1).copied().unwrap_or(n);
        let mut term: Option<MirTerm> = None;

        for (off, op) in ops[leader..end].iter().enumerate() {
            let i = leader + off;
            // Terminators end the block.
            match op {
                Op::Return => {
                    let v = stack.pop().ok_or(CompileError::StackUnderflow)?;
                    term = Some(MirTerm::Return(v));
                    break;
                }
                Op::Goto(t) => {
                    let target = block_for[&(*t as usize)];
                    term = Some(MirTerm::Goto {
                        target,
                        args: stack.clone(),
                    });
                    break;
                }
                Op::GotoIfNil(t) | Op::GotoIfNotNil(t) => {
                    let cond = stack.pop().ok_or(CompileError::StackUnderflow)?;
                    let taken = block_for[&(*t as usize)];
                    let fallthrough = block_for[&(i + 1)];
                    term = Some(MirTerm::Branch {
                        cond,
                        on_nil: matches!(op, Op::GotoIfNil(_)),
                        else_pop: false,
                        taken,
                        taken_args: stack.clone(),
                        fallthrough,
                        fallthrough_args: stack.clone(),
                    });
                    break;
                }
                Op::GotoIfNilElsePop(t) | Op::GotoIfNotNilElsePop(t) => {
                    // The tested value stays on the taken edge and is popped on
                    // the fall-through.
                    let cond = *stack.last().ok_or(CompileError::StackUnderflow)?;
                    let taken = block_for[&(*t as usize)];
                    let fallthrough = block_for[&(i + 1)];
                    let taken_args = stack.clone();
                    let mut fall = stack.clone();
                    fall.pop();
                    term = Some(MirTerm::Branch {
                        cond,
                        on_nil: matches!(op, Op::GotoIfNilElsePop(_)),
                        else_pop: true,
                        taken,
                        taken_args,
                        fallthrough,
                        fallthrough_args: fall,
                    });
                    break;
                }
                // Unmodelled-in-4a control flow: bail (the baseline tier handles
                // these functions).
                Op::Switch
                | Op::Throw
                | Op::PushConditionCase(_)
                | Op::PushConditionCaseRaw(_)
                | Op::PushCatch(_)
                | Op::PopHandler => {
                    return Err(CompileError::UnsupportedOp("mir-unmodelled-control"));
                }
                _ => {
                    // Snapshot the pre-op stack (the accurate model stack, incl. the
                    // inst-less stack-shuffle folds) and stamp it + the bytecode pc
                    // onto every inst this op emits — the precise-deopt framestate
                    // and the Opaque-delegation full stack read these directly
                    // instead of replaying the (lossy) folds.
                    let pre = stack.clone();
                    let before = insts.len();
                    lower_value_op(&mut b, op, constants, &mut stack, &mut insts)?;
                    let float_site = feedback(i) == NumericFeedback::Float;
                    for inst in &mut insts[before..] {
                        inst.pc = i;
                        inst.pre_stack = pre.clone();
                        // The Float rule (see `build_mir_with_feedback`).
                        if float_site
                            && matches!(
                                inst.op,
                                MirOp::Bin(
                                    BinKind::Add | BinKind::Sub | BinKind::Mul | BinKind::Div,
                                    ..
                                )
                            )
                        {
                            inst.ty = LispType::Any;
                            b.value_types[inst.result.0 as usize] = LispType::Any;
                        }
                    }
                }
            }
        }

        let term = match term {
            Some(t) => t,
            // A block that runs off the end without a terminator falls through
            // to the next leader (analyze_cfg guarantees one exists and the
            // depth is consistent).
            None => {
                if end >= n {
                    return Err(CompileError::NoReturn);
                }
                MirTerm::Goto {
                    target: block_for[&end],
                    args: stack.clone(),
                }
            }
        };

        blocks.push(MirBlockData {
            bytecode_pc: leader,
            params,
            insts,
            term,
        });
    }

    Ok(MirFunction {
        arity,
        blocks,
        value_types: b.value_types,
        block_for,
        constants: constants.into(),
        inline_epoch: None,
    })
}

/// Lower one non-terminator opcode into MIR instruction(s), updating the model
/// `stack`. Explicit ops (arithmetic / predicates / car-cdr / cons / eq) get a
/// typed [`MirOp`]; everything else becomes an [`MirOp::Opaque`] whose arity
/// comes from [`simple_effect`].
fn lower_value_op(
    b: &mut Builder,
    op: &Op,
    constants: &[Value],
    stack: &mut Vec<MirValue>,
    insts: &mut Vec<MirInst>,
) -> Result<(), CompileError> {
    let mut emit = |b: &mut Builder, mop: MirOp, ty: LispType, eff: Effect| {
        let r = b.fresh(ty);
        insts.push(MirInst {
            result: r,
            op: mop,
            ty,
            effect: eff,
            // Stamped by the caller (build_mir's block loop) once the pre-op stack
            // + bytecode pc are known; placeholders here.
            pc: 0,
            pre_stack: Vec::new(),
        });
        r
    };

    macro_rules! pop {
        () => {
            stack.pop().ok_or(CompileError::StackUnderflow)?
        };
    }

    match op {
        Op::Constant(idx) => {
            let v = *constants
                .get(*idx as usize)
                .ok_or(CompileError::BadOperand)?;
            let r = emit(b, MirOp::Const(v), LispType::of_value(v), Effect::PURE);
            stack.push(r);
        }
        Op::Nil | Op::List(0) => {
            let r = emit(b, MirOp::Const(Value::NIL), LispType::Nil, Effect::PURE);
            stack.push(r);
        }
        Op::True => {
            let r = emit(b, MirOp::Const(Value::T), LispType::True, Effect::PURE);
            stack.push(r);
        }
        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Rem | Op::Max | Op::Min => {
            let rhs = pop!();
            let lhs = pop!();
            let kind = match op {
                Op::Add => BinKind::Add,
                Op::Sub => BinKind::Sub,
                Op::Mul => BinKind::Mul,
                Op::Div => BinKind::Div,
                Op::Rem => BinKind::Rem,
                Op::Max => BinKind::Max,
                Op::Min => BinKind::Min,
                _ => unreachable!(),
            };
            let r = emit(
                b,
                MirOp::Bin(kind, lhs, rhs),
                LispType::Fixnum,
                Effect::MAY_DEOPT,
            );
            stack.push(r);
        }
        Op::Add1 | Op::Sub1 | Op::Negate => {
            let a = pop!();
            let kind = match op {
                Op::Add1 => UnaryKind::Add1,
                Op::Sub1 => UnaryKind::Sub1,
                Op::Negate => UnaryKind::Negate,
                _ => unreachable!(),
            };
            let r = emit(
                b,
                MirOp::Unary(kind, a),
                LispType::Fixnum,
                Effect::MAY_DEOPT,
            );
            stack.push(r);
        }
        Op::Eqlsign | Op::Lss | Op::Gtr | Op::Leq | Op::Geq => {
            let rhs = pop!();
            let lhs = pop!();
            let kind = match op {
                Op::Eqlsign => CmpKind::NumEq,
                Op::Lss => CmpKind::Lt,
                Op::Gtr => CmpKind::Gt,
                Op::Leq => CmpKind::Le,
                Op::Geq => CmpKind::Ge,
                _ => unreachable!(),
            };
            let r = emit(
                b,
                MirOp::Cmp(kind, lhs, rhs),
                LispType::Boolean,
                Effect::MAY_DEOPT,
            );
            stack.push(r);
        }
        Op::Null
        | Op::Not
        | Op::Consp
        | Op::Stringp
        | Op::Listp
        | Op::Symbolp
        | Op::Integerp
        | Op::Numberp => {
            let a = pop!();
            let kind = match op {
                Op::Null => PredKind::Null,
                Op::Not => PredKind::Not,
                Op::Consp => PredKind::Consp,
                Op::Stringp => PredKind::Stringp,
                Op::Listp => PredKind::Listp,
                Op::Symbolp => PredKind::Symbolp,
                Op::Integerp => PredKind::Integerp,
                Op::Numberp => PredKind::Numberp,
                _ => unreachable!(),
            };
            let r = emit(
                b,
                MirOp::Pred(kind, a),
                LispType::Boolean,
                Effect::READ_HEAP,
            );
            stack.push(r);
        }
        Op::Eq => {
            let rhs = pop!();
            let lhs = pop!();
            let r = emit(b, MirOp::Eq(lhs, rhs), LispType::Boolean, Effect::READ_HEAP);
            stack.push(r);
        }
        Op::Car | Op::Cdr | Op::CarSafe | Op::CdrSafe => {
            let a = pop!();
            let cdr = matches!(op, Op::Cdr | Op::CdrSafe);
            let safe = matches!(op, Op::CarSafe | Op::CdrSafe);
            let r = emit(
                b,
                MirOp::CarCdr { cdr, safe, arg: a },
                LispType::Any,
                if safe {
                    Effect::READ_HEAP
                } else {
                    Effect::READ_HEAP.with(Effect::MAY_DEOPT)
                },
            );
            stack.push(r);
        }
        Op::Cons => {
            let cdr = pop!();
            let car = pop!();
            let r = emit(b, MirOp::Cons(car, cdr), LispType::Cons, Effect::ALLOCATES);
            stack.push(r);
        }
        Op::List(1) => {
            // The byte compiler also uses list1 for (cons x nil). Expose the
            // pair to ordinary cons escape analysis and cold reconstruction.
            // Both allocation shims are non-collecting; neither can run Lisp.
            // Wider lists remain opaque until nested virtual objects qualify.
            let car = pop!();
            let cdr = emit(b, MirOp::Const(Value::NIL), LispType::Nil, Effect::PURE);
            let r = emit(b, MirOp::Cons(car, cdr), LispType::Cons, Effect::ALLOCATES);
            stack.push(r);
        }
        // Pure operand-stack shuffles — modelled as stack manipulation, no inst.
        Op::Pop => {
            pop!();
        }
        Op::Dup => {
            let top = *stack.last().ok_or(CompileError::StackUnderflow)?;
            stack.push(top);
        }
        Op::StackRef(k) => {
            let idx = stack
                .len()
                .checked_sub(1 + *k as usize)
                .ok_or(CompileError::StackUnderflow)?;
            stack.push(stack[idx]);
        }
        Op::StackSet(k) => {
            let k = *k as usize;
            let top = pop!();
            if k != 0 {
                let idx = stack
                    .len()
                    .checked_sub(k)
                    .ok_or(CompileError::StackUnderflow)?;
                stack[idx] = top;
            }
        }
        Op::DiscardN(raw) => {
            let preserve = (*raw & 0x80) != 0;
            let cnt = (*raw & 0x7F) as usize;
            if cnt != 0 {
                let len = stack.len();
                if preserve {
                    let target = len
                        .checked_sub(1 + cnt)
                        .ok_or(CompileError::StackUnderflow)?;
                    stack[target] = stack[len - 1];
                } else if cnt > len {
                    return Err(CompileError::StackUnderflow);
                }
                stack.truncate(len - cnt);
            }
        }
        // Everything else: opaque, arity from simple_effect.
        other => {
            let (needs, delta) = simple_effect(other)?;
            if stack.len() < needs {
                return Err(CompileError::StackUnderflow);
            }
            let at = stack.len() - needs;
            let args: Vec<MirValue> = stack.split_off(at);
            let produces = needs as i64 + delta;
            if produces < 0 {
                return Err(CompileError::StackUnderflow);
            }
            let eff = opaque_effect(other);
            // Emit the opaque instruction EXACTLY ONCE — even a 0-result op has a
            // side effect that must survive into the MIR. `produces` is 0 or 1 in
            // practice: a 1-result op (e.g. `Aset`) pushes its value; a 0-result op
            // (`VarSet`/`VarBind`/`Save*`/`UnwindProtectPop`, which pop their args
            // and push nothing) is evaluated for effect only. Emitting inside a
            // `for _ in 0..produces` loop DROPPED 0-result ops entirely — a compiled
            // `(setq special val)` returned `val` (via the bytecode's `dup`) but
            // never performed the assignment, because `lower_mir_pure` never saw the
            // Opaque it bails on and the MIR tier silently claimed the body.
            debug_assert!(
                produces <= 1,
                "opaque op modelled with >1 results: {other:?}"
            );
            let r = emit(
                b,
                MirOp::Opaque {
                    op: other.clone(),
                    args,
                },
                LispType::Any,
                eff,
            );
            if produces >= 1 {
                stack.push(r);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Pretty-printer (debugging aid; not on any hot path).
// ---------------------------------------------------------------------------

impl fmt::Display for MirFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "mir fn (arity {}):", self.arity)?;
        for (bid, blk) in self.blocks.iter().enumerate() {
            let plist: Vec<String> = blk.params.iter().map(|p| format!("v{}", p.0)).collect();
            writeln!(
                f,
                "  block{bid} @pc{}({}):",
                blk.bytecode_pc,
                plist.join(", ")
            )?;
            for inst in &blk.insts {
                writeln!(
                    f,
                    "    v{} = {:?}  [{:?}/{:?}]",
                    inst.result.0, inst.op, inst.ty, inst.effect
                )?;
            }
            writeln!(f, "    -> {:?}", blk.term)?;
        }
        Ok(())
    }
}

/// Apply `f` to every `MirValue` operand of `op`, in place. Used for value
/// renumbering during inline-splicing and for inlined-call result substitution.
fn map_op_operands(op: &mut MirOp, mut f: impl FnMut(MirValue) -> MirValue) {
    match op {
        MirOp::Arg(_) | MirOp::Const(_) | MirOp::InlineEntry => {}
        MirOp::Bin(_, a, b) | MirOp::Cmp(_, a, b) | MirOp::Eq(a, b) | MirOp::Cons(a, b) => {
            *a = f(*a);
            *b = f(*b);
        }
        MirOp::Unary(_, a) | MirOp::Pred(_, a) => *a = f(*a),
        MirOp::CarCdr { arg, .. } => *arg = f(*arg),
        MirOp::Opaque { args, .. } => {
            for a in args.iter_mut() {
                *a = f(*a);
            }
        }
    }
}

/// The `MirValue` operands of an op (read-only twin of [`map_op_operands`]).
pub(crate) fn op_operands(op: &MirOp) -> impl Iterator<Item = MirValue> + '_ {
    let (fixed, rest): ([Option<MirValue>; 2], &[MirValue]) = match op {
        MirOp::Arg(_) | MirOp::Const(_) | MirOp::InlineEntry => ([None, None], &[]),
        MirOp::Bin(_, a, b) | MirOp::Cmp(_, a, b) | MirOp::Eq(a, b) | MirOp::Cons(a, b) => {
            ([Some(*a), Some(*b)], &[])
        }
        MirOp::Unary(_, a) | MirOp::Pred(_, a) => ([Some(*a), None], &[]),
        MirOp::CarCdr { arg, .. } => ([Some(*arg), None], &[]),
        MirOp::Opaque { args, .. } => ([None, None], args.as_slice()),
    };
    fixed.into_iter().flatten().chain(rest.iter().copied())
}

/// The `MirValue` operands of a terminator (read-only twin of
/// [`map_term_operands`]): the condition and every edge argument.
pub(crate) fn term_operands(term: &MirTerm) -> impl Iterator<Item = MirValue> + '_ {
    let cond = match term {
        MirTerm::Return(v) => Some(*v),
        MirTerm::Goto { .. } => None,
        MirTerm::Branch { cond, .. } => Some(*cond),
    };
    cond.into_iter()
        .chain(successor_edges(term).flat_map(|(_, args)| args.iter().copied()))
}

/// Apply `f` to every `MirValue` operand of a terminator, in place.
fn map_term_operands(term: &mut MirTerm, mut f: impl FnMut(MirValue) -> MirValue) {
    match term {
        MirTerm::Return(v) => *v = f(*v),
        MirTerm::Goto { args, .. } => {
            for a in args.iter_mut() {
                *a = f(*a);
            }
        }
        MirTerm::Branch {
            cond,
            taken_args,
            fallthrough_args,
            ..
        } => {
            *cond = f(*cond);
            for a in taken_args.iter_mut() {
                *a = f(*a);
            }
            for a in fallthrough_args.iter_mut() {
                *a = f(*a);
            }
        }
    }
}

/// The successor edges of a terminator: each target block with the argument
/// list that feeds its parameters (one-for-one, see [`MirTerm`]).
pub(crate) fn successor_edges(term: &MirTerm) -> impl Iterator<Item = (MirBlockId, &[MirValue])> {
    let (a, b): (Option<_>, Option<_>) = match term {
        MirTerm::Return(_) => (None, None),
        MirTerm::Goto { target, args } => (Some((*target, args.as_slice())), None),
        MirTerm::Branch {
            taken,
            taken_args,
            fallthrough,
            fallthrough_args,
            ..
        } => (
            Some((*taken, taken_args.as_slice())),
            Some((*fallthrough, fallthrough_args.as_slice())),
        ),
    };
    a.into_iter().chain(b)
}

/// Infer the type of every value, resolving the `Unknown` block parameters
/// the builder leaves behind: the MIR twin of the baseline's
/// `compute_known_fixnum_slots`.
///
/// A forward JOIN fixpoint over the CFG's edges: a parameter's type is the
/// join of every argument that reaches it. Instruction results keep the type
/// the builder stamped (an op's result type does not depend on its operands
/// — a `Bin` at a non-`Float` site is a fixnum or deopts, whatever it was
/// fed). The baseline's MUST analysis seeds its entry block all-false and
/// every other leader TOP and narrows with AND; here block-0 params are `Any`
/// (the entry jump passes untyped argument loads, and `join(Any, _) == Any`
/// keeps them so even when block 0 is a loop header) and every other param
/// starts `Unknown`, the join's identity, so its first incoming edge decides
/// it and any disagreement widens it to `Any`.
///
/// Terminates: a param moves at most `Unknown -> concrete -> Any`. Sound for
/// guard elision (a `Fixnum`-typed value IS a fixnum at run time) because
/// every non-entry leader has a real incoming edge (`build_mir` bails on an
/// unreachable block), so no reachable param ends `Unknown`, and because the
/// builder types a `Bin` result `Fixnum` only where the lowering guards it
/// (the `Float` rule, [`build_mir_with_feedback`]). Runs on the INLINED
/// function: it reads no pc, and a `Fixnum` flows through an inlined callee's
/// substituted return value.
pub fn infer_value_types(m: &MirFunction) -> Vec<LispType> {
    let mut ty = m.value_types.clone();
    loop {
        let mut changed = false;
        for blk in &m.blocks {
            for (t, args) in successor_edges(&blk.term) {
                let params = &m.blocks[t.0 as usize].params;
                debug_assert_eq!(params.len(), args.len(), "edge args match target params");
                for (p, a) in params.iter().zip(args) {
                    let old = ty[p.0 as usize];
                    let new = old.join(ty[a.0 as usize]);
                    if new != old {
                        ty[p.0 as usize] = new;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    debug_assert!(
        m.blocks[0]
            .params
            .iter()
            .all(|p| ty[p.0 as usize] == LispType::Any),
        "block-0 params are the untyped arguments"
    );
    debug_assert!(
        m.blocks
            .iter()
            .flat_map(|b| b.params.iter())
            .all(|p| ty[p.0 as usize] != LispType::Unknown),
        "every reachable block param has an incoming edge"
    );
    ty
}

/// A callee is inlinable into a pure MIR iff it is a single block ending in
/// `Return`, contains no opaque/allocating/identity ops (so the splice keeps the
/// caller in `lower_mir_pure`'s pure subset), and is within the size budget.
fn callee_inlinable(c: &MirFunction, max_insts: usize) -> bool {
    c.blocks.len() == 1
        && matches!(c.blocks[0].term, MirTerm::Return(_))
        && c.blocks[0].insts.iter().all(|i| {
            !matches!(
                i.op,
                MirOp::Opaque { .. } | MirOp::Eq(..) | MirOp::Cons(..) | MirOp::InlineEntry
            )
        })
        // Even before feedback arrives, a known non-fixnum operand cannot
        // pass this tier's numeric guards. Keep its real call path.
        && c.blocks[0].insts.iter().all(|i| {
            !matches!(i.op, MirOp::Bin(..) | MirOp::Unary(..) | MirOp::Cmp(..))
                || op_operands(&i.op).all(|v| {
                    matches!(c.value_type(v), LispType::Unknown | LispType::Any | LispType::Fixnum)
                })
        })
        && c.blocks[0]
            .insts
            .iter()
            .filter(|i| !matches!(i.op, MirOp::Arg(_)))
            .count()
            <= max_insts
}

/// Inline pure single-block callees at `Opaque{Op::Call(n)}` sites whose function
/// operand is a compile-time `Const` resolving (via `resolve`) to an inlinable
/// MIR. Splices the callee's body in place of the call: the callee's params map to
/// the call's args, its other values are renumbered into the caller's value space,
/// and the call's result is substituted with the callee's return value.
///
/// The spliced result stays a PURE MIR (the call is gone), so `lower_mir_pure`
/// lowers it with sound rerun-from-start deopt — and unboxing/guard-elision then
/// flow ACROSS the former call boundary (the optimization the per-pc baseline
/// cannot do). Returns the number of sites inlined.
///
/// Each splice retains an `InlineEntry` marker. Production compilation arms
/// `inline_epoch`: reentrant bodies guard each marker and deopt to the original
/// call; pure bodies rely on the cache's entry validation.
pub fn inline_pure_single_block_callees(
    m: &mut MirFunction,
    // The call site's pc (a deopt may have barred it) and the callee symbol.
    resolve: &impl Fn(usize, Value) -> Option<MirFunction>,
    max_insts: usize,
    // Out: the SymId of each callee actually inlined (for the precise
    // dependency/invalidation map — redefining one of these must re-JIT this caller).
    inlined_syms: &mut Vec<crate::emacs_core::intern::SymId>,
) -> usize {
    let mut inlined = 0usize;
    let mut subs: Vec<(MirValue, MirValue)> = Vec::new();

    for bi in 0..m.blocks.len() {
        // Within-block map from a value to the constant defining it (a call's
        // function operand is a `Const` symbol pushed just before its args).
        let consts: HashMap<MirValue, Value> = m.blocks[bi]
            .insts
            .iter()
            .filter_map(|i| match &i.op {
                MirOp::Const(v) => Some((i.result, *v)),
                _ => None,
            })
            .collect();

        let old_insts = std::mem::take(&mut m.blocks[bi].insts);
        let mut new_insts: Vec<MirInst> = Vec::with_capacity(old_insts.len());

        for inst in old_insts {
            let resolved = match &inst.op {
                MirOp::Opaque {
                    op: Op::Call(n),
                    args,
                } if args.len() == *n as usize + 1 => consts
                    .get(&args[0])
                    .and_then(|sym| sym.as_symbol_id().map(|id| (*sym, id)))
                    .and_then(|(sym, id)| resolve(inst.pc, sym).map(|c| (c, id)))
                    .filter(|(c, _)| c.arity == *n as usize && callee_inlinable(c, max_insts))
                    .map(|(c, id)| (c, args[1..].to_vec(), id)),
                _ => None,
            };

            let Some((callee, arg_vals, callee_sym)) = resolved else {
                new_insts.push(inst);
                continue;
            };

            let marker = MirValue(m.value_types.len() as u32);
            m.value_types.push(LispType::Nil);
            new_insts.push(MirInst {
                result: marker,
                op: MirOp::InlineEntry,
                ty: LispType::Nil,
                // The guard observes function bindings and runtime flags.
                effect: Effect::READ_BINDINGS
                    .with(Effect::READ_HEAP)
                    .with(Effect::MAY_DEOPT),
                pc: inst.pc,
                pre_stack: inst.pre_stack.clone(),
            });

            // Splice the callee's single block: params -> the call's args; other
            // values -> fresh caller values.
            let cblk = &callee.blocks[0];
            let mut remap: HashMap<MirValue, MirValue> = HashMap::new();
            for (i, &p) in cblk.params.iter().enumerate() {
                remap.insert(p, arg_vals[i]);
            }
            for cinst in &cblk.insts {
                if matches!(cinst.op, MirOp::Arg(_)) {
                    continue; // params already mapped to the call's args
                }
                debug_assert!(
                    !matches!(cinst.op, MirOp::Opaque { .. }),
                    "callee_inlinable admits no Opaque (its operands would index the caller's pool)"
                );
                let new_v = MirValue(m.value_types.len() as u32);
                m.value_types.push(cinst.ty);
                remap.insert(cinst.result, new_v);
                let mut op = cinst.op.clone();
                map_op_operands(&mut op, |v| remap.get(&v).copied().unwrap_or(v));
                new_insts.push(MirInst {
                    result: new_v,
                    op,
                    ty: cinst.ty,
                    effect: cinst.effect,
                    // The inlined body inherits the CALL SITE's framestate anchor
                    // (pc + pre-call stack). A pure body deopts by rerun-from-
                    // start, which needs no framestate; a call-bearing body
                    // deopts PRECISELY at this anchor, which re-executes the
                    // call in the interpreter — sound only because the callee is
                    // pure (`callee_inlinable`), so re-running it is
                    // unobservable. (The substitution pass below fixes up the
                    // VALUES in these snapshots, so they reference no dead SSA
                    // values.) Anything reading `pc` as an index into THIS
                    // body's ops must run before this pass (see
                    // `build_mir_with_feedback`).
                    pc: inst.pc,
                    pre_stack: inst.pre_stack.clone(),
                });
            }
            let MirTerm::Return(rv) = &cblk.term else {
                unreachable!("callee_inlinable guarantees a Return terminator");
            };
            let new_result = remap.get(rv).copied().unwrap_or(*rv);
            subs.push((inst.result, new_result));
            inlined_syms.push(callee_sym);
            inlined += 1;
        }

        m.blocks[bi].insts = new_insts;
    }

    // Substitute each inlined call's old result with the callee's return value,
    // across the whole function (later uses, terminators, AND framestate snapshots).
    // COMPOSE the substitutions: a callee that returns a param directly (identity /
    // accessor wrappers) yields a pre-existing caller value, which may itself be an
    // earlier inlined call's result — so resolve each value through the substitution
    // chain to a fixpoint. (Sequential application would reintroduce a value an
    // earlier substitution already deleted.)
    if !subs.is_empty() {
        let resolve = |v: MirValue| -> MirValue {
            let mut v = v;
            // Each `old` is a fresh, distinct call result and each `new` is an older
            // value, so the chain strictly decreases and terminates.
            while let Some((_, n)) = subs.iter().find(|(o, _)| *o == v) {
                v = *n;
            }
            v
        };
        for blk in &mut m.blocks {
            for inst in &mut blk.insts {
                map_op_operands(&mut inst.op, &resolve);
                for pv in inst.pre_stack.iter_mut() {
                    *pv = resolve(*pv);
                }
            }
            map_term_operands(&mut blk.term, &resolve);
        }
    }

    inlined
}

/// Escape analysis for cons SCALAR-REPLACEMENT. Returns, indexed by `MirValue.0`,
/// `Some((car, cdr))` for each `MirOp::Cons` result whose EVERY use is a local
/// `MirOp::CarCdr` read — i.e. it never escapes: not returned, not a cross-block
/// (phi) arg, not consed into another cons, not `eq`'d, not an `Opaque` operand,
/// not an arithmetic/predicate/compare operand. Such a cons can be eliminated with
/// NO heap allocation — its car/cdr reads forward directly to the operand SSA
/// values. Precise deopt records reconstruct the cons in the cold exit;
/// opaque safepoints and outgoing block edges require real values instead.
///
/// Conservative: any non-CarCdr use marks the cons escaping (`None` — keep the
/// allocation / bail). A cons consed into ANOTHER cons is treated as escaping (the
/// simplest sound rule, no fixpoint). The use classification matches op KIND
/// explicitly (a `CarCdr.arg` is the sole non-escaping position) rather than a
/// position-blind operand walk.
pub(crate) fn cons_scalar_repl_targets(m: &MirFunction) -> Vec<Option<(MirValue, MirValue)>> {
    let n = m.value_types.len();
    let mut cons_of: Vec<Option<(MirValue, MirValue)>> = vec![None; n];
    let mut escapes = vec![false; n];
    let esc = |v: MirValue, escapes: &mut Vec<bool>| escapes[v.0 as usize] = true;
    for blk in &m.blocks {
        for inst in &blk.insts {
            if let MirOp::Cons(car, cdr) = &inst.op {
                cons_of[inst.result.0 as usize] = Some((*car, *cdr));
            }
            match &inst.op {
                MirOp::Arg(_) | MirOp::Const(_) | MirOp::InlineEntry => {}
                // A car/cdr read is the ONLY non-escaping use of a cons.
                MirOp::CarCdr { .. } => {}
                MirOp::Bin(_, a, b) | MirOp::Cmp(_, a, b) | MirOp::Eq(a, b) | MirOp::Cons(a, b) => {
                    esc(*a, &mut escapes);
                    esc(*b, &mut escapes);
                }
                MirOp::Unary(_, a) | MirOp::Pred(_, a) => esc(*a, &mut escapes),
                MirOp::Opaque { args, .. } => {
                    for a in args.iter().chain(inst.pre_stack.iter()) {
                        // A runtime safepoint needs real stack roots. Keep any
                        // cons live in its framestate, even when it is not a
                        // direct call operand. Virtual conses stay block-local
                        // and cannot survive a service poll or Lisp callback.
                        esc(*a, &mut escapes);
                    }
                }
            }
        }
        match &blk.term {
            MirTerm::Return(v) => esc(*v, &mut escapes),
            MirTerm::Goto { args, .. } => {
                for v in args {
                    esc(*v, &mut escapes);
                }
            }
            MirTerm::Branch {
                cond,
                taken_args,
                fallthrough_args,
                ..
            } => {
                esc(*cond, &mut escapes);
                for v in taken_args {
                    esc(*v, &mut escapes);
                }
                for v in fallthrough_args {
                    esc(*v, &mut escapes);
                }
            }
        }
    }
    let mut out = vec![None; n];
    for (i, cc) in cons_of.iter().enumerate() {
        if let Some(cc) = cc
            && !escapes[i]
        {
            out[i] = Some(*cc);
        }
    }
    out
}

#[cfg(test)]
#[path = "mir/tests/mir_test.rs"]
mod tests;
