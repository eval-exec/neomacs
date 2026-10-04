//! Owned global SSA, with GNU operand-stack snapshots at observable sites.
//!
//! Threading: a compilation owns and mutates a `Func`; once published it is
//! read-only and transferable. Constants are opaque bits, with lifetime and
//! rooting retained by the source/front owner. No operation here dereferences
//! a Lisp heap object or stores Lisp state in a process/thread cache.

use std::collections::HashMap;
use std::fmt;

use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::Value as LispValue;

use super::mem::{AliasClass, Effects};
use super::types::TypeSet;

macro_rules! id {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub(crate) struct $name(pub u32);
        impl $name {
            pub(crate) const fn index(self) -> usize {
                self.0 as usize
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }
    };
}
id!(Block, "b");
id!(Inst, "i");
id!(Value, "v");
id!(FrameId, "f");
id!(InlineSiteId, "site");
id!(LoopId, "loop");
id!(AllocId, "alloc");

/// Opaque tagged bits, never a worker-accessible Lisp object. Threading:
/// immutable and transferable; the source owner keeps any referenced heap
/// object rooted until compilation/installation finishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ValueBits(pub u64);

impl ValueBits {
    pub(crate) fn from_value(value: LispValue) -> Self {
        Self(value.bits() as u64)
    }

    /// Tests run on the mutator that owns and roots the original constants.
    #[cfg(test)]
    pub(crate) fn to_value(self) -> LispValue {
        LispValue::from_bits(self.0 as usize)
    }
}

/// Native argument slots include nil-padded optionals and one rest-list slot.
/// Threading: immutable compiler metadata, independent of parameter symbols.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct ParamShape {
    pub required: usize,
    pub optional: usize,
    pub has_rest: bool,
}

impl ParamShape {
    pub(crate) const fn native_arity(self) -> usize {
        self.required + self.optional + self.has_rest as usize
    }
}

/// Per-compilation counters; threading: owned by the compiler, aggregated only
/// after compilation through the existing diagnostic/compile-stat seams.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OptCensus {
    pub blocks: usize,
    pub insts: usize,
    pub phis: usize,
    pub frames: usize,
    pub dead_leaders: usize,
    pub critical_edges: usize,
    pub refinements: usize,
    pub fold: Option<super::passes::fold::FoldStats>,
    pub bools: Option<super::passes::bools::BoolStats>,
    pub reps: Option<RepsCensus>,
    pub gvn: Option<super::passes::gvn::GvnStats>,
    pub range: Option<super::passes::range::RangeStats>,
    pub licm: Option<super::passes::licm::LicmStats>,
    pub arrays: Option<super::passes::array_reads::ArrayLiftStats>,
    pub sink: Option<super::sink_recipes::SinkStats>,
}

/// Immutable numeric pass counters. Threading: a compiler owns these scalar
/// counts, then transfers them with its leaf; they contain no Lisp state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RepsCensus {
    pub lift: super::passes::reps_lift::LiftStats,
    pub selection: super::passes::reps::RepsStats,
}

/// One owned function. Constants carry source-pool indices in `Const` and
/// patched-prefix indices in `EnvConst`; no pass may replace the latter with
/// an instance-specific constant. Threading: exclusive while being built and
/// immutable when transferred to a compiler worker.
#[derive(Clone, Debug)]
pub(crate) struct Func {
    pub blocks: Vec<BlockData>,
    pub insts: Vec<InstData>,
    pub values: Vec<ValueData>,
    pub frames: Vec<FrameState>,
    pub entry: Block,
    pub osr: Option<OsrEntry>,
    pub consts: Box<[ValueBits]>,
    pub dynamic_prefix: usize,
    pub arity: ParamShape,
    pub census: OptCensus,
    /// Exact original Aref guards and mutable-layout witnesses, owned by this
    /// compilation. This sidecar contains IDs/scalars only and is immutable
    /// when a backend worker receives the plan; it stores no Lisp pointers.
    pub array_reads: super::passes::array_reads::ArrayReadProofs,
    /// Compiler-owned exact-point identities and physical SSA cache versions.
    /// Immutable after publication; no runtime/TLS Lisp object cache.
    pub sink_recipes: super::sink_recipes::SinkRecipes,
    /// Per-source-pc stacks for the shared baseline-emitter adapter.
    pub source_states: Vec<Option<SourceState>>,
    /// Full GNU stack at each block entry, including invariant non-phi values.
    pub entry_stacks: Vec<Box<[Value]>>,
    frame_intern: HashMap<FrameState, FrameId>,
}

impl Func {
    pub(crate) fn new(consts: Box<[ValueBits]>, arity: ParamShape, dynamic_prefix: usize) -> Self {
        Self {
            blocks: Vec::new(),
            insts: Vec::new(),
            values: Vec::new(),
            frames: Vec::new(),
            entry: Block(0),
            osr: None,
            consts,
            dynamic_prefix,
            arity,
            census: OptCensus::default(),
            array_reads: super::passes::array_reads::ArrayReadProofs::default(),
            sink_recipes: super::sink_recipes::SinkRecipes::default(),
            source_states: Vec::new(),
            entry_stacks: Vec::new(),
            frame_intern: HashMap::new(),
        }
    }

    pub(crate) fn intern_frame(&mut self, state: FrameState) -> FrameId {
        if let Some(&frame) = self.frame_intern.get(&state) {
            return frame;
        }
        let frame = FrameId(self.frames.len() as u32);
        self.frames.push(state.clone());
        self.frame_intern.insert(state, frame);
        frame
    }

    /// Refresh after a builder rewrites alias handles in frame stacks. The
    /// builder owns any frame-id compaction and rewrites all frame users first.
    pub(crate) fn rebuild_frame_intern(&mut self) {
        self.frame_intern.clear();
        for (index, frame) in self.frames.iter().enumerate() {
            self.frame_intern
                .entry(frame.clone())
                .or_insert(FrameId(index as u32));
        }
    }

    /// Precise root-window candidates: retain the full GNU-stack frame chain,
    /// then add compiler-only live values and derived-pointer bases. Virtual
    /// fields are ordinary SSA operands and are visited without publishing the
    /// virtual identity. The caller supplies liveness; this function never
    /// publishes raw payloads or NumPair sentinel words.
    pub(crate) fn roots_for(&self, frame: FrameId, live: &[Value]) -> Option<Vec<Value>> {
        // Recipe caches depend on exact source cuts. A point-free caller must
        // use the independent selected roots_at capability instead.
        if !self.sink_recipes.owners.is_empty() {
            return None;
        }
        let mut pending = live.to_vec();
        let mut current = Some(frame);
        let mut frames_seen = std::collections::HashSet::new();
        while let Some(frame) = current {
            if !frames_seen.insert(frame) {
                return None;
            }
            let frame = self.frames.get(frame.index())?;
            pending.extend_from_slice(&frame.stack);
            current = frame.parent;
        }
        let mut seen = std::collections::HashSet::new();
        let mut roots = Vec::new();
        while let Some(value) = pending.pop() {
            let value = self.resolve(value)?;
            if !seen.insert(value) {
                continue;
            }
            let data = self.values.get(value.index())?;
            match data.rep {
                Rep::Tagged if data.ty.may_need_root() => roots.push(value),
                Rep::RawPtr { base } => pending.push(base),
                Rep::Virtual(_) => match data.def {
                    ValueDef::Inst(inst) => {
                        pending.extend_from_slice(&self.insts.get(inst.index())?.args)
                    }
                    // Virtual phis require explicit field/materialization
                    // metadata before they may cross a safepoint.
                    ValueDef::Param { .. } | ValueDef::Alias(_) => return None,
                },
                Rep::Tagged
                | Rep::TaggedFix
                | Rep::RawInt
                | Rep::RawF64
                | Rep::RawWord
                | Rep::NumPair
                | Rep::Bool => {}
            }
        }
        roots.sort_unstable();
        Some(roots)
    }

    /// Resolve a trivial-phi alias without trusting a potentially invalid IR.
    pub(crate) fn resolve(&self, mut value: Value) -> Option<Value> {
        for _ in 0..=self.values.len() {
            match self.values.get(value.index())?.def {
                ValueDef::Alias(next) => value = next,
                _ => return Some(value),
            }
        }
        None
    }

    pub(crate) fn display(&self) -> impl fmt::Display + '_ {
        self
    }
}

/// A basic block; real phis are block parameters. Threading: compiler-owned
/// metadata; predecessor and edge arguments are immutable after SSA sealing.
#[derive(Clone, Debug)]
pub(crate) struct BlockData {
    pub params: Vec<Value>,
    pub insts: Vec<Inst>,
    pub term: Term,
    pub preds: Vec<Block>,
    pub pc: u32,
    pub loop_header: Option<LoopId>,
    pub cold: bool,
}

impl BlockData {
    pub(crate) fn new(pc: u32) -> Self {
        Self {
            params: Vec::new(),
            insts: Vec::new(),
            term: Term::Unreachable,
            preds: Vec::new(),
            pc,
            loop_header: None,
            cold: false,
        }
    }
}

/// A value and its globally valid type/representation. Threading: compiler
/// metadata only; a `RawPtr` carries a base SSA value rather than a Rust pointer.
#[derive(Clone, Debug)]
pub(crate) struct ValueData {
    pub ty: TypeSet,
    pub rep: Rep,
    pub def: ValueDef,
}

/// Definition metadata; threading: immutable once the compiler seals SSA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ValueDef {
    Param { block: Block, index: u32 },
    Inst(Inst),
    Alias(Value),
}

/// Selected machine representation; threading: compile-owned metadata only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Rep {
    Tagged,
    TaggedFix,
    RawInt,
    RawF64,
    /// Opaque numeric transport; interpreted only with verified ready/box fields.
    RawWord,
    NumPair,
    Bool,
    RawPtr {
        base: Value,
    },
    Virtual(AllocId),
}

impl Rep {
    pub(crate) const fn is_tagged(self) -> bool {
        matches!(self, Self::Tagged | Self::TaggedFix)
    }
}

/// An ordered instruction. Calls and polls remain pinned, and guards/safepoints
/// carry the exact pre-operation GNU stack. Threading: compiler-owned data;
/// effect declarations use the same immutable table as baseline/MIR/AOT.
#[derive(Clone, Debug)]
pub(crate) struct InstData {
    pub op: Opcode,
    pub args: Vec<Value>,
    pub result: Option<Value>,
    pub eff: Effects,
    pub mem: AliasClass,
    pub frame: Option<FrameId>,
    pub pc: u32,
}

/// Exact comparison relation; threading: immutable compiler metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// A numeric selection operation; threading: immutable compiler metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MinMax {
    Min,
    Max,
}

/// Ordered IR operation; threading: compiler-owned, with opaque constants and
/// immutable source bytecodes rather than mutator-local Lisp references.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Opcode {
    Sink(super::sink_recipes::SinkOp),
    Const(u32),
    EnvConst(u32),
    Arg(u16),
    OsrSlot(u16),
    BoolConst(bool),
    TagFix,
    UntagFix,
    UnboxF64,
    BoolToLisp,
    IsNonNil,
    FixAdd {
        checked: bool,
    },
    FixSub {
        checked: bool,
    },
    FixMul {
        checked: bool,
    },
    FixDiv,
    FixRem,
    FixCmp(Cmp),
    FixMinMax(MinMax),
    F64Add,
    F64Sub,
    F64Mul,
    F64Div,
    F64Cmp(Cmp),
    F64FromFix,
    /// Lower with a sign-bit xor; CLIF fneg permits a NaN-sign rewrite.
    F64Neg,
    F64Sqrt,
    TypeTest(TypeSet),
    Eq,
    Select,
    CheckType(TypeSet),
    CheckNonZero,
    CheckBounds,
    CheckEq(ValueBits),
    CheckNoOverflow,
    Refine(TypeSet),
    LoadCar,
    LoadCdr,
    StoreCar,
    StoreCdr,
    LoadVecLen,
    LoadVecSlots,
    LoadVecElem,
    StoreVecElem,
    LoadRecTag,
    LoadSymValue(SymId),
    StoreSymValue(SymId),
    LoadF64,
    AllocCons,
    AllocFloat,
    Call {
        site: u32,
    },
    Builtin(Op),
    /// Initial lowering uses the baseline emitter with its source framestate.
    Opaque(Op),
    /// The same ordered baseline operation with a normalized Boolean result.
    /// Explicit operands remain Lisp words; effects and source frames are
    /// preserved rather than inferred from the successful result's type.
    OpaqueBool(Op),
    /// Pure inline-region replay/attention/depth guard; indexes fuser metadata.
    InlineEntry(u32),
    /// Pinned back-edge poll, with the baseline's exact countdown cadence.
    Poll,
    /// Explicit root-window publication; only tagged heap-possible values.
    PublishRoot,
}

impl Opcode {
    pub(crate) fn is_guard(&self) -> bool {
        matches!(
            self,
            Self::CheckType(_)
                | Self::CheckNonZero
                | Self::CheckBounds
                | Self::CheckEq(_)
                | Self::CheckNoOverflow
                | Self::InlineEntry(_)
                | Self::FixAdd { checked: true }
                | Self::FixSub { checked: true }
                | Self::FixMul { checked: true }
                | Self::FixDiv
                | Self::FixRem
                | Self::Sink(
                    super::sink_recipes::SinkOp::SourceNum(_)
                        | super::sink_recipes::SinkOp::SourceSqrt
                )
        )
    }

    pub(crate) fn requires_frame(&self, effects: Effects) -> bool {
        self.is_guard()
            || effects.intersects(
                Effects::MAY_DEOPT
                    .with(Effects::MAY_GC)
                    .with(Effects::MAY_REENTER)
                    .with(Effects::MAY_SIGNAL),
            )
            || matches!(
                self,
                Self::Call { .. }
                    | Self::Opaque(_)
                    | Self::OpaqueBool(_)
                    | Self::Poll
                    | Self::Sink(super::sink_recipes::SinkOp::SourceCons(_))
            )
    }

    /// Allocation and an undispatched signal do not collect. An opaque
    /// primitive needs roots only when its body may collect or run Lisp;
    /// genuine calls and polls retain their unconditional runtime protocol.
    pub(crate) fn is_safepoint(&self, effects: Effects) -> bool {
        effects.intersects(Effects::MAY_GC.with(Effects::MAY_REENTER))
            || matches!(self, Self::Call { .. } | Self::Poll)
    }
}

/// Exact successful T/NIL producers supported by the Boolean shared-emitter
/// adapter. Threading: immutable opcode classification, no Lisp state. Numeric
/// comparisons retain the original ordered guards and slow error protocol.
pub(crate) fn opaque_bool_arity(op: &Op) -> Option<usize> {
    match op {
        Op::Null
        | Op::Not
        | Op::Consp
        | Op::Stringp
        | Op::Listp
        | Op::Symbolp
        | Op::Integerp
        | Op::Numberp => Some(1),
        Op::Eq | Op::Eqlsign | Op::Lss | Op::Gtr | Op::Leq | Op::Geq => Some(2),
        _ => None,
    }
}

/// An exact GNU stack; parent/site are reserved for the shared inline-chain
/// format. Threading: immutable compiler metadata, interned only within a Func.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FrameState {
    pub pc: u32,
    pub stack: Box<[Value]>,
    pub handlers: u16,
    pub binds: u16,
    pub parent: Option<FrameId>,
    pub site: Option<InlineSiteId>,
}

/// Source mapping for the initial shared-emitter adapter. Threading: immutable
/// SSA handles only; preserving the full stack also preserves GNU GC retention.
#[derive(Clone, Debug)]
pub(crate) struct SourceState {
    pub pre: Box<[Value]>,
    pub post: Box<[Value]>,
    pub frame: FrameId,
    pub block: Block,
}

/// An OSR snapshot shape. Threading: immutable metadata; live Lisp snapshot
/// values remain owned and rooted by the transferring mutator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OsrEntry {
    pub entry_pc: u32,
    pub depth: usize,
    pub header: Block,
}

/// A successor and its real-phi arguments; threading: sealed compiler metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edge {
    pub target: Block,
    pub args: Vec<Value>,
}

/// Static switch landing; threading: integer metadata captured by the front.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SwitchCase {
    /// Raw jump-table target address returned by the shared switch helper.
    pub key: i64,
    pub edge: Edge,
}

/// Explicit control flow; threading: compiler-owned until SSA is sealed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Term {
    Unreachable,
    Jump(Edge),
    Branch {
        flag: Value,
        if_true: Edge,
        if_false: Edge,
    },
    Switch {
        value: Value,
        table: Value,
        cases: Vec<SwitchCase>,
        default: Edge,
    },
    Return(Value),
    Deopt(FrameId),
}

impl Term {
    pub(crate) fn edges(&self) -> Vec<&Edge> {
        match self {
            Self::Jump(edge) => vec![edge],
            Self::Branch {
                if_true, if_false, ..
            } => vec![if_true, if_false],
            Self::Switch { cases, default, .. } => {
                cases.iter().map(|c| &c.edge).chain([default]).collect()
            }
            Self::Unreachable | Self::Return(_) | Self::Deopt(_) => Vec::new(),
        }
    }
}

impl fmt::Display for Func {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "func {:?} entry={} prefix={} osr={:?}",
            self.arity, self.entry, self.dynamic_prefix, self.osr
        )?;
        if let Some(fold) = &self.census.fold {
            writeln!(f, "  fold {fold:?}")?;
        }
        if let Some(bools) = &self.census.bools {
            writeln!(f, "  bool {bools:?}")?;
        }
        for (i, constant) in self.consts.iter().enumerate() {
            writeln!(f, "  const{k} = 0x{bits:016x}", k = i, bits = constant.0)?;
        }
        for (i, frame) in self.frames.iter().enumerate() {
            writeln!(
                f,
                "  {} = pc:{} stack:{:?} handlers:{} binds:{} parent:{:?} site:{:?}",
                FrameId(i as u32),
                frame.pc,
                frame.stack,
                frame.handlers,
                frame.binds,
                frame.parent,
                frame.site
            )?;
        }
        for (i, block) in self.blocks.iter().enumerate() {
            write!(f, "{}(", Block(i as u32))?;
            for (j, &param) in block.params.iter().enumerate() {
                if j != 0 {
                    write!(f, ", ")?;
                }
                match self.values.get(param.index()) {
                    Some(data) => write!(f, "{param}:{}:{:?}", data.ty, data.rep)?,
                    None => write!(f, "{param}:INVALID")?,
                }
            }
            writeln!(
                f,
                ") pc:{} preds:{:?} loop:{:?} cold:{}",
                block.pc, block.preds, block.loop_header, block.cold
            )?;
            for &inst in &block.insts {
                match self.insts.get(inst.index()) {
                    Some(data) => {
                        write!(f, "  {inst}: ")?;
                        if let Some(value) = data.result {
                            match self.values.get(value.index()) {
                                Some(data) => write!(f, "{value}:{}:{:?} = ", data.ty, data.rep)?,
                                None => write!(f, "{value}:INVALID = ")?,
                            }
                        }
                        writeln!(
                            f,
                            "{:?} {:?} pc:{} effects:{:?} alias:{:?} frame:{:?}",
                            data.op, data.args, data.pc, data.eff, data.mem, data.frame
                        )?;
                    }
                    None => writeln!(f, "  {inst}: INVALID")?,
                }
            }
            writeln!(f, "  {:?}", block.term)?;
        }
        Ok(())
    }
}
