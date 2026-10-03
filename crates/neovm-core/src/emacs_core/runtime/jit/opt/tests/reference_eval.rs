//! Test-only, backend-independent execution of the optimizing IR.
//!
//! SSA values, simultaneous edge transfers and frame readback are interpreted
//! here. Opaque instructions deliberately use the Tier-0 opcode implementation,
//! rather than JIT helpers, so the reference does not duplicate Lisp semantics.
//! Tiny opcode frames do not reproduce the original enclosing Lisp backtrace;
//! native observability tests must validate that contract separately. Binding
//! and unwind extents are rejected rather than split across tiny frames.
//! All execution storage belongs to one invocation and its supplied mutator
//! context; there is no process-global or thread-local Lisp state.

use super::ir::{
    Cmp, Edge, FrameId, Func, InstData, MinMax, Opcode, Rep, Term, Value, ValueBits, ValueDef,
};
use super::types::TypeSet;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op, Vm};
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::{LambdaParams, Value as LispValue, ValueKind, VecLikeType};

/// Runtime inputs are already in native slot order: missing optionals are nil
/// and a rest argument is one list slot. OSR supplies the actual header stack.
/// Prefix constants belong to this invocation's mutator, not the compiler.
#[derive(Default)]
pub(crate) struct Inputs<'a> {
    pub args: &'a [LispValue],
    pub osr_stack: &'a [LispValue],
    pub prefix: &'a [LispValue],
    pub step_limit: usize,
}

/// A snapshot owns diagnostic strings, not GC roots. Raw stack bits may only
/// be decoded while their original Lisp objects are still rooted by the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub pc: u32,
    pub stack: Vec<ValueBits>,
    pub printed_stack: String,
    pub handlers: u16,
    pub binds: u16,
}

#[derive(Debug)]
pub(crate) enum Outcome {
    Returned(ValueBits),
    Deopt(Snapshot),
}

#[derive(Debug)]
pub(crate) struct Run {
    pub outcome: Outcome,
    pub trace: Vec<Snapshot>,
}

#[derive(Debug)]
pub(crate) enum EvalError {
    Invalid(String),
    Flow(Flow),
    StepLimit,
}

#[derive(Clone, Copy, Debug)]
enum Cell {
    Lisp(ValueBits),
    Bool(bool),
    Int(i64),
    F64 {
        bits: u64,
        identity: u64,
    },
    /// A symbolic slots pointer keeps the original Lisp vector identity;
    /// never an untraced Rust pointer into a possibly-reallocated Vec.
    Slots(ValueBits),
}

enum TypedStep {
    Cell(Option<Cell>),
    Deopt(Snapshot),
}

struct Evaluator<'a, 'b> {
    func: &'a Func,
    ctx: &'b mut Context,
    inputs: Inputs<'a>,
    values: Vec<Option<Cell>>,
    trace: Vec<Snapshot>,
    steps: usize,
    quitcounter: u8,
    live_before: Vec<Vec<Value>>,
    current_inst: usize,
    // Invocation-local identity recipes keep all aliases of one raw float
    // materialized as one box. Dead recipes do not root dead Lisp objects.
    next_float: std::cell::Cell<u64>,
    float_boxes: std::cell::RefCell<std::collections::HashMap<u64, ValueBits>>,
    unboxed: std::cell::RefCell<std::collections::HashMap<ValueBits, u64>>,
}

fn invalid(message: impl Into<String>) -> EvalError {
    EvalError::Invalid(message.into())
}

/// Evaluate without CLIF or machine code. The source states are observed before
/// their corresponding operators, including source-only stack shuffles.
pub(crate) fn evaluate(
    func: &Func,
    ctx: &mut Context,
    inputs: Inputs<'_>,
) -> Result<Run, EvalError> {
    let live_before = reference_liveness(func)?;
    let roots_base = ctx.bc_buf.len();
    for bits in &func.consts {
        ctx.bc_buf.push(bits.to_value());
    }
    ctx.bc_buf.extend_from_slice(inputs.args);
    ctx.bc_buf.extend_from_slice(inputs.osr_stack);
    ctx.bc_buf.extend_from_slice(inputs.prefix);
    let result = Evaluator {
        func,
        ctx,
        inputs,
        values: vec![None; func.values.len()],
        trace: Vec::new(),
        steps: 0,
        quitcounter: 1,
        live_before,
        current_inst: 0,
        next_float: std::cell::Cell::new(0),
        float_boxes: std::cell::RefCell::new(std::collections::HashMap::new()),
        unboxed: std::cell::RefCell::new(std::collections::HashMap::new()),
    }
    .run();
    ctx.bc_buf.truncate(roots_base);
    result
}

type Live = std::collections::BTreeSet<Value>;

fn live_value(func: &Func, value: Value) -> Result<Value, EvalError> {
    func.resolve(value)
        .ok_or_else(|| invalid("invalid live SSA value"))
}

fn live_frame(func: &Func, mut id: Option<FrameId>, live: &mut Live) -> Result<(), EvalError> {
    let mut seen = std::collections::HashSet::new();
    while let Some(frame) = id {
        if !seen.insert(frame) {
            return Err(invalid("cyclic live frame chain"));
        }
        let frame = func
            .frames
            .get(frame.index())
            .ok_or_else(|| invalid("missing live frame"))?;
        for &value in &frame.stack {
            live.insert(live_value(func, value)?);
        }
        id = frame.parent;
    }
    Ok(())
}

fn live_term(func: &Func, term: &Term, entries: &[Live]) -> Result<Live, EvalError> {
    let mut live = Live::new();
    for edge in term.edges() {
        let target = func
            .blocks
            .get(edge.target.index())
            .ok_or_else(|| invalid("missing live successor"))?;
        for &value in &entries[edge.target.index()] {
            let value = if let Some(index) = target.params.iter().position(|&param| param == value)
            {
                *edge
                    .args
                    .get(index)
                    .ok_or_else(|| invalid("missing live phi operand"))?
            } else {
                value
            };
            live.insert(live_value(func, value)?);
        }
    }
    match term {
        Term::Return(value) | Term::Branch { flag: value, .. } => {
            live.insert(live_value(func, *value)?);
        }
        Term::Switch { value, table, .. } => {
            live.insert(live_value(func, *value)?);
            live.insert(live_value(func, *table)?);
        }
        Term::Deopt(frame) => live_frame(func, Some(*frame), &mut live)?,
        _ => {}
    }
    Ok(live)
}

fn live_instruction(func: &Func, inst: &InstData, live: &mut Live) -> Result<(), EvalError> {
    if let Some(result) = inst.result {
        live.remove(&live_value(func, result)?);
    }
    for &value in &inst.args {
        live.insert(live_value(func, value)?);
    }
    if inst.op.requires_frame(inst.eff) {
        live_frame(func, inst.frame, live)?;
    }
    Ok(())
}

/// Semantic SSA liveness for reference roots, independent of CLIF emission.
/// Edge arguments substitute live successor parameters simultaneously. Exact
/// observation frames add uses even if the normal IR result no longer needs a
/// stack operand. Threading: invocation-owned SSA handles, no Lisp-state cache.
fn reference_liveness(func: &Func) -> Result<Vec<Vec<Value>>, EvalError> {
    let mut entries = vec![Live::new(); func.blocks.len()];
    loop {
        let mut changed = false;
        for (index, block) in func.blocks.iter().enumerate().rev() {
            let mut live = live_term(func, &block.term, &entries)?;
            for &id in block.insts.iter().rev() {
                live_instruction(func, &func.insts[id.index()], &mut live)?;
            }
            if live != entries[index] {
                entries[index] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut before = vec![Vec::new(); func.insts.len()];
    for block in &func.blocks {
        let mut live = live_term(func, &block.term, &entries)?;
        for &id in block.insts.iter().rev() {
            live_instruction(func, &func.insts[id.index()], &mut live)?;
            before[id.index()] = live.iter().copied().collect();
        }
    }
    Ok(before)
}

impl Evaluator<'_, '_> {
    /// Retain the exact frame plus SSA identities used after this safepoint.
    /// Storage is invocation-local; dead SSA cells are never GC roots. Raw
    /// slots identities retain their Lisp base, and live float recipes retain
    /// their materialized identity without publishing an untraced raw payload.
    fn root_current(&mut self) -> Result<(), EvalError> {
        let roots = self.live_before[self.current_inst]
            .iter()
            .map(|&value| match self.read(value)? {
                Cell::Lisp(bits) | Cell::Slots(bits) => Ok(Some(bits.to_value())),
                Cell::F64 { .. } => self.lisp(value).map(Some),
                Cell::Int(_) | Cell::Bool(_) => Ok(None),
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.ctx.bc_buf.extend(
            roots
                .into_iter()
                .flatten()
                .filter(|value| value.is_heap_object()),
        );
        Ok(())
    }

    fn read(&self, mut value: Value) -> Result<Cell, EvalError> {
        for _ in 0..=self.func.values.len() {
            if let Some(cell) = self.values.get(value.index()).and_then(|v| *v) {
                return Ok(cell);
            }
            let Some(data) = self.func.values.get(value.index()) else {
                return Err(invalid(format!("missing value {value:?}")));
            };
            match data.def {
                ValueDef::Alias(alias) => value = alias,
                _ => return Err(invalid(format!("undefined value {value:?}"))),
            }
        }
        Err(invalid("cyclic SSA aliases"))
    }

    fn lisp(&self, value: Value) -> Result<LispValue, EvalError> {
        match self.read(value)? {
            Cell::Lisp(bits) => Ok(bits.to_value()),
            Cell::Bool(value) => Ok(if value { LispValue::T } else { LispValue::NIL }),
            Cell::Int(value)
                if (LispValue::MOST_NEGATIVE_FIXNUM..=LispValue::MOST_POSITIVE_FIXNUM)
                    .contains(&value) =>
            {
                Ok(LispValue::fixnum(value))
            }
            Cell::Int(_) => Err(invalid("raw integer does not fit a Lisp fixnum")),
            Cell::F64 { bits, identity } => {
                if let Some(boxed) = self.float_boxes.borrow().get(&identity) {
                    return Ok(boxed.to_value());
                }
                let boxed = LispValue::make_float(f64::from_bits(bits));
                self.float_boxes
                    .borrow_mut()
                    .insert(identity, ValueBits::from_value(boxed));
                Ok(boxed)
            }
            Cell::Slots(_) => Err(invalid("a raw slots pointer cannot become a Lisp value")),
        }
    }

    fn flag(&self, value: Value) -> Result<bool, EvalError> {
        match self.read(value)? {
            Cell::Bool(value) => Ok(value),
            Cell::Lisp(bits) => Ok(!bits.to_value().is_nil()),
            _ => Err(invalid("branch requires Bool or tagged Lisp condition")),
        }
    }

    fn snapshot(
        &self,
        pc: u32,
        stack: &[Value],
        handlers: u16,
        binds: u16,
    ) -> Result<Snapshot, EvalError> {
        let stack = stack
            .iter()
            .map(|&value| self.lisp(value).map(ValueBits::from_value))
            .collect::<Result<Vec<_>, _>>()?;
        let printed = stack
            .iter()
            .map(|bits| crate::emacs_core::print::print_value(&bits.to_value()))
            .collect::<Vec<_>>();
        let printed_stack = if printed.is_empty() {
            "nil".to_owned()
        } else {
            format!("({})", printed.join(" "))
        };
        Ok(Snapshot {
            pc,
            stack,
            printed_stack,
            handlers,
            binds,
        })
    }

    fn frame(&self, id: FrameId) -> Result<Snapshot, EvalError> {
        let frame = self
            .func
            .frames
            .get(id.index())
            .ok_or_else(|| invalid("missing frame"))?;
        if frame.parent.is_some() {
            return Err(invalid(
                "multi-frame reference execution requires an explicit caller",
            ));
        }
        self.snapshot(frame.pc, &frame.stack, frame.handlers, frame.binds)
    }

    fn transfer(&mut self, edge: &Edge) -> Result<(), EvalError> {
        let params = &self.func.blocks[edge.target.index()].params;
        if params.len() != edge.args.len() {
            return Err(invalid("edge argument count"));
        }
        // Read every predecessor value before overwriting any loop parameter.
        let cells = edge
            .args
            .iter()
            .map(|&v| self.read(v))
            .collect::<Result<Vec<_>, _>>()?;
        for (&param, cell) in params.iter().zip(cells) {
            self.values[param.index()] = Some(cell);
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<(), EvalError> {
        self.steps += 1;
        let limit = if self.inputs.step_limit == 0 {
            100_000
        } else {
            self.inputs.step_limit
        };
        if self.steps > limit {
            Err(EvalError::StepLimit)
        } else {
            Ok(())
        }
    }

    fn inst(&mut self, id: super::ir::Inst) -> Result<Option<Snapshot>, EvalError> {
        self.tick()?;
        self.current_inst = id.index();
        let function = self.func;
        if let Some(step) = self.typed(&function.insts[id.index()])? {
            match step {
                TypedStep::Deopt(frame) => return Ok(Some(frame)),
                TypedStep::Cell(cell) => {
                    return self
                        .store_result(function.insts[id.index()].result, cell)
                        .map(|()| None);
                }
            }
        }
        let inst = &self.func.insts[id.index()];
        let args = inst
            .args
            .iter()
            .map(|&v| self.lisp(v))
            .collect::<Result<Vec<_>, _>>()?;
        let cell = match &inst.op {
            Opcode::BoolConst(value) => Some(Cell::Bool(*value)),
            Opcode::Const(index) => Some(Cell::Lisp(
                *self
                    .func
                    .consts
                    .get(*index as usize)
                    .ok_or_else(|| invalid("constant index"))?,
            )),
            Opcode::EnvConst(index) => Some(Cell::Lisp(ValueBits::from_value(
                *self
                    .inputs
                    .prefix
                    .get(*index as usize)
                    .ok_or_else(|| invalid("prefix constant index"))?,
            ))),
            Opcode::Arg(index) => Some(Cell::Lisp(ValueBits::from_value(
                *self
                    .inputs
                    .args
                    .get(*index as usize)
                    .ok_or_else(|| invalid("argument index"))?,
            ))),
            Opcode::OsrSlot(index) => Some(Cell::Lisp(ValueBits::from_value(
                *self
                    .inputs
                    .osr_stack
                    .get(*index as usize)
                    .ok_or_else(|| invalid("OSR slot index"))?,
            ))),
            Opcode::IsNonNil => Some(Cell::Bool(
                !args
                    .first()
                    .ok_or_else(|| invalid("nil-test operand"))?
                    .is_nil(),
            )),
            Opcode::Refine(_) => {
                Some(self.read(*inst.args.first().ok_or_else(|| invalid("refine operand"))?)?)
            }
            Opcode::CheckType(ty) => {
                let bits =
                    ValueBits::from_value(*args.first().ok_or_else(|| invalid("guard operand"))?);
                if !TypeSet::for_constant(bits).is_subset(*ty) {
                    return Ok(Some(self.frame(
                        inst.frame.ok_or_else(|| invalid("guard has no frame"))?,
                    )?));
                }
                Some(Cell::Lisp(bits))
            }
            Opcode::Opaque(op) => {
                let result = self.opaque(inst, op, &function.consts, &args)?;
                inst.result
                    .map(|_| Cell::Lisp(ValueBits::from_value(result)))
            }
            Opcode::OpaqueBool(op) => {
                // The real Tier-0 routine retains GNU numeric, identity and
                // dynamic positioned-symbol semantics. Only its successful
                // T/NIL result changes representation in this evaluator.
                let result = self.opaque(inst, op, &function.consts, &args)?;
                if !result.is_nil() && !result.is_t() {
                    return Err(invalid("opaque Bool returned a non-Boolean Lisp value"));
                }
                Some(Cell::Bool(result.is_t()))
            }
            Opcode::Poll => {
                self.quitcounter = self.quitcounter.wrapping_add(1);
                if self.quitcounter == 0 {
                    self.quitcounter = 1;
                    let root_base = self.ctx.bc_buf.len();
                    self.frame(inst.frame.ok_or_else(|| invalid("poll has no frame"))?)?;
                    self.root_current()?;
                    let result = self.ctx.bytecode_branch_maybe_gc_and_quit();
                    self.ctx.bc_buf.truncate(root_base);
                    result.map_err(EvalError::Flow)?;
                }
                None
            }
            Opcode::InlineEntry(_) => {
                // The source owner, not this IR, owns replay/depth/attention
                // side metadata. Never assume that an inline guard succeeds.
                return Ok(Some(
                    self.frame(
                        inst.frame
                            .ok_or_else(|| invalid("inline entry has no frame"))?,
                    )?,
                ));
            }
            other => return Err(invalid(format!("unsupported reference opcode {other:?}"))),
        };
        self.store_result(inst.result, cell)?;
        Ok(None)
    }

    fn store_result(&mut self, result: Option<Value>, cell: Option<Cell>) -> Result<(), EvalError> {
        match (result, cell) {
            (Some(value), Some(cell)) => self.values[value.index()] = Some(cell),
            (None, None) => (),
            _ => return Err(invalid("instruction result shape")),
        }
        Ok(())
    }

    fn integer(&self, value: Value) -> Result<i64, EvalError> {
        match self.read(value)? {
            Cell::Int(value) => Ok(value),
            Cell::Lisp(bits) => bits
                .to_value()
                .as_fixnum()
                .ok_or_else(|| invalid("typed fixnum operand")),
            _ => Err(invalid("typed fixnum operand representation")),
        }
    }

    fn f64(&self, value: Value) -> Result<f64, EvalError> {
        match self.read(value)? {
            Cell::F64 { bits, .. } => Ok(f64::from_bits(bits)),
            Cell::Lisp(bits) if bits.to_value().is_float() => Ok(bits.to_value().xfloat()),
            _ => Err(invalid("typed f64 operand")),
        }
    }

    fn int_result(&self, inst: &InstData, value: i64) -> Result<Cell, EvalError> {
        let result = inst
            .result
            .ok_or_else(|| invalid("integer instruction without result"))?;
        match self.func.values[result.index()].rep {
            Rep::RawInt => Ok(Cell::Int(value)),
            Rep::Tagged | Rep::TaggedFix
                if (LispValue::MOST_NEGATIVE_FIXNUM..=LispValue::MOST_POSITIVE_FIXNUM)
                    .contains(&value) =>
            {
                Ok(Cell::Lisp(ValueBits::from_value(LispValue::fixnum(value))))
            }
            _ => Err(invalid("integer instruction result representation/range")),
        }
    }

    fn deopt(&self, inst: &InstData) -> Result<TypedStep, EvalError> {
        Ok(TypedStep::Deopt(
            self.frame(
                inst.frame
                    .ok_or_else(|| invalid("typed guard has no frame"))?,
            )?,
        ))
    }

    fn guard_result(&self, inst: &InstData) -> Result<TypedStep, EvalError> {
        Ok(TypedStep::Cell(
            inst.result.map(|_| self.read(inst.args[0])).transpose()?,
        ))
    }

    fn opaque(
        &mut self,
        inst: &InstData,
        op: &Op,
        constants: &[ValueBits],
        args: &[LispValue],
    ) -> Result<LispValue, EvalError> {
        let root_base = self.ctx.bc_buf.len();
        self.root_current()?;
        let result = evaluate_opaque(self.ctx, op, constants, args, inst.result.is_some());
        self.ctx.bc_buf.truncate(root_base);
        result
    }

    /// Typed operations use their specified representations, and checked
    /// arithmetic exits through the pre-operation frame. Raw float identity
    /// recipes materialize once when a Lisp operand or framestate needs them.
    fn typed(&mut self, inst: &InstData) -> Result<Option<TypedStep>, EvalError> {
        let arg = |index: usize| {
            inst.args
                .get(index)
                .copied()
                .ok_or_else(|| invalid("typed instruction operand count"))
        };
        let cell = match &inst.op {
            Opcode::UntagFix => Some(Cell::Int(self.integer(arg(0)?)?)),
            Opcode::TagFix => Some(Cell::Lisp(ValueBits::from_value(self.lisp(arg(0)?)?))),
            Opcode::UnboxF64 | Opcode::LoadF64 => {
                let original = ValueBits::from_value(self.lisp(arg(0)?)?);
                let bits = self.f64(arg(0)?)?.to_bits();
                let existing = self.unboxed.borrow().get(&original).copied();
                let identity = if let Some(identity) = existing {
                    identity
                } else {
                    let identity = self.next_float.get();
                    self.next_float.set(identity + 1);
                    self.unboxed.borrow_mut().insert(original, identity);
                    self.float_boxes.borrow_mut().insert(identity, original);
                    identity
                };
                Some(Cell::F64 { bits, identity })
            }
            Opcode::BoolToLisp => Some(Cell::Lisp(ValueBits::from_value(if self.flag(arg(0)?)? {
                LispValue::T
            } else {
                LispValue::NIL
            }))),
            Opcode::FixAdd { checked }
            | Opcode::FixSub { checked }
            | Opcode::FixMul { checked } => {
                let a = i128::from(self.integer(arg(0)?)?);
                let b = i128::from(self.integer(arg(1)?)?);
                let value = match inst.op {
                    Opcode::FixAdd { .. } => a + b,
                    Opcode::FixSub { .. } => a - b,
                    _ => a * b,
                };
                if !(i128::from(LispValue::MOST_NEGATIVE_FIXNUM)
                    ..=i128::from(LispValue::MOST_POSITIVE_FIXNUM))
                    .contains(&value)
                {
                    if *checked {
                        return self.deopt(inst).map(Some);
                    }
                    return Err(invalid(
                        "unchecked fixnum arithmetic overflowed its proven range",
                    ));
                }
                Some(self.int_result(inst, value as i64)?)
            }
            Opcode::FixDiv | Opcode::FixRem => {
                let a = self.integer(arg(0)?)?;
                let b = self.integer(arg(1)?)?;
                if b == 0 {
                    return self.deopt(inst).map(Some);
                }
                let value = if matches!(inst.op, Opcode::FixDiv) {
                    i128::from(a) / i128::from(b)
                } else {
                    i128::from(a) % i128::from(b)
                };
                if !(i128::from(LispValue::MOST_NEGATIVE_FIXNUM)
                    ..=i128::from(LispValue::MOST_POSITIVE_FIXNUM))
                    .contains(&value)
                {
                    return self.deopt(inst).map(Some);
                }
                Some(self.int_result(inst, value as i64)?)
            }
            Opcode::FixCmp(cc) => {
                let a = self.integer(arg(0)?)?;
                let b = self.integer(arg(1)?)?;
                Some(Cell::Bool(compare(*cc, a, b)))
            }
            Opcode::FixMinMax(which) => {
                let a = self.integer(arg(0)?)?;
                let b = self.integer(arg(1)?)?;
                Some(self.int_result(
                    inst,
                    if matches!(which, MinMax::Min) {
                        a.min(b)
                    } else {
                        a.max(b)
                    },
                )?)
            }
            Opcode::F64Add | Opcode::F64Sub | Opcode::F64Mul | Opcode::F64Div => {
                let a = self.f64(arg(0)?)?;
                let b = self.f64(arg(1)?)?;
                let value = match inst.op {
                    Opcode::F64Add => a + b,
                    Opcode::F64Sub => a - b,
                    Opcode::F64Mul => a * b,
                    _ => a / b,
                };
                Some(self.raw_float(value.to_bits()))
            }
            Opcode::F64Neg => Some(self.raw_float(self.f64(arg(0)?)?.to_bits() ^ (1_u64 << 63))),
            Opcode::F64Sqrt => Some(self.raw_float(self.f64(arg(0)?)?.sqrt().to_bits())),
            Opcode::F64FromFix => Some(self.raw_float((self.integer(arg(0)?)? as f64).to_bits())),
            Opcode::F64Cmp(cc) => Some(Cell::Bool(compare(
                *cc,
                self.f64(arg(0)?)?,
                self.f64(arg(1)?)?,
            ))),
            Opcode::TypeTest(ty) => Some(Cell::Bool(self.runtime_type(arg(0)?)?.is_subset(*ty))),
            Opcode::CheckType(ty) => {
                if !self.runtime_type(arg(0)?)?.is_subset(*ty) {
                    return self.deopt(inst).map(Some);
                }
                return self.guard_result(inst).map(Some);
            }
            Opcode::CheckNonZero => {
                if self.integer(arg(0)?)? == 0 {
                    return self.deopt(inst).map(Some);
                }
                return self.guard_result(inst).map(Some);
            }
            Opcode::CheckBounds => {
                let i = self.integer(arg(0)?)?;
                let length = self.integer(arg(1)?)?;
                if i < 0 || i >= length {
                    return self.deopt(inst).map(Some);
                }
                return self.guard_result(inst).map(Some);
            }
            Opcode::CheckEq(expected) => {
                if self.lisp(arg(0)?)?.bits() as u64 != expected.0 {
                    return self.deopt(inst).map(Some);
                }
                return self.guard_result(inst).map(Some);
            }
            Opcode::Refine(_) => Some(self.read(arg(0)?)?),
            Opcode::Select => Some(self.read(if self.flag(arg(0)?)? {
                arg(1)?
            } else {
                arg(2)?
            })?),
            Opcode::Eq => {
                let args = [self.lisp(arg(0)?)?, self.lisp(arg(1)?)?];
                let value = self.opaque(inst, &Op::Eq, &[], &args)?;
                Some(Cell::Bool(!value.is_nil()))
            }
            Opcode::LoadCar
            | Opcode::LoadCdr
            | Opcode::StoreCar
            | Opcode::StoreCdr
            | Opcode::AllocCons => {
                let args = inst
                    .args
                    .iter()
                    .map(|&v| self.lisp(v))
                    .collect::<Result<Vec<_>, _>>()?;
                let op = match inst.op {
                    Opcode::LoadCar => Op::Car,
                    Opcode::LoadCdr => Op::Cdr,
                    Opcode::StoreCar => Op::Setcar,
                    Opcode::StoreCdr => Op::Setcdr,
                    _ => Op::Cons,
                };
                let value = self.opaque(inst, &op, &[], &args)?;
                inst.result
                    .map(|_| Cell::Lisp(ValueBits::from_value(value)))
            }
            Opcode::AllocFloat => Some(Cell::Lisp(ValueBits::from_value(LispValue::make_float(
                self.f64(arg(0)?)?,
            )))),
            Opcode::LoadVecLen => {
                let value = self.lisp(arg(0)?)?;
                let length = value
                    .as_vector_data()
                    .or_else(|| value.as_record_data())
                    .ok_or_else(|| invalid("vector-length operand"))?
                    .len();
                Some(self.int_result(inst, length as i64)?)
            }
            Opcode::LoadVecSlots => Some(Cell::Slots(ValueBits::from_value(self.lisp(arg(0)?)?))),
            Opcode::LoadVecElem | Opcode::StoreVecElem | Opcode::LoadRecTag => {
                let base = match self.read(arg(0)?)? {
                    Cell::Slots(bits) => bits.to_value(),
                    _ => self.lisp(arg(0)?)?,
                };
                let index = if matches!(inst.op, Opcode::LoadRecTag) {
                    LispValue::fixnum(0)
                } else {
                    self.lisp(arg(1)?)?
                };
                let mut args = vec![base, index];
                let op = if matches!(inst.op, Opcode::StoreVecElem) {
                    args.push(self.lisp(arg(2)?)?);
                    Op::Aset
                } else {
                    Op::Aref
                };
                let value = self.opaque(inst, &op, &[], &args)?;
                inst.result
                    .map(|_| Cell::Lisp(ValueBits::from_value(value)))
            }
            Opcode::LoadSymValue(symbol) | Opcode::StoreSymValue(symbol) => {
                let pool = [ValueBits::from_value(LispValue::from_sym_id(*symbol))];
                let store = matches!(inst.op, Opcode::StoreSymValue(_));
                let args = if store {
                    vec![self.lisp(arg(0)?)?]
                } else {
                    Vec::new()
                };
                let op = if store { Op::VarSet(0) } else { Op::VarRef(0) };
                let value = self.opaque(inst, &op, &pool, &args)?;
                inst.result
                    .map(|_| Cell::Lisp(ValueBits::from_value(value)))
            }
            Opcode::Call { .. } | Opcode::Builtin(_) => {
                let args = inst
                    .args
                    .iter()
                    .map(|&v| self.lisp(v))
                    .collect::<Result<Vec<_>, _>>()?;
                let op = match &inst.op {
                    Opcode::Builtin(op) => op.clone(),
                    _ => Op::Call(
                        u16::try_from(
                            args.len()
                                .checked_sub(1)
                                .ok_or_else(|| invalid("call without callee"))?,
                        )
                        .map_err(|_| invalid("call arity"))?,
                    ),
                };
                let constants = self.func.consts.clone();
                let value = self.opaque(inst, &op, &constants, &args)?;
                inst.result
                    .map(|_| Cell::Lisp(ValueBits::from_value(value)))
            }
            Opcode::PublishRoot => {
                self.lisp(arg(0)?)?;
                None
            }
            Opcode::CheckNoOverflow => {
                if self.flag(arg(0)?)? {
                    return self.deopt(inst).map(Some);
                }
                return self.guard_result(inst).map(Some);
            }
            _ => return Ok(None),
        };
        Ok(Some(TypedStep::Cell(cell)))
    }

    fn runtime_type(&self, value: Value) -> Result<TypeSet, EvalError> {
        match self.read(value)? {
            Cell::Int(value) => Ok(TypeSet::for_constant(ValueBits::from_value(
                LispValue::fixnum(value),
            ))),
            Cell::F64 { .. } => Ok(TypeSet::FLOAT),
            Cell::Bool(false) => Ok(TypeSet::NIL),
            Cell::Bool(true) => Ok(TypeSet::T),
            Cell::Slots(_) => Err(invalid("raw pointer has no Lisp kind")),
            Cell::Lisp(bits) => {
                let value = bits.to_value();
                let ty = match value.kind() {
                    ValueKind::Veclike(VecLikeType::Vector) => TypeSet::VECTOR,
                    ValueKind::Veclike(VecLikeType::Record) => TypeSet::RECORD,
                    ValueKind::Veclike(VecLikeType::Bignum) => TypeSet::BIGNUM,
                    ValueKind::Veclike(VecLikeType::Marker) => TypeSet::MARKER,
                    ValueKind::Veclike(_) => TypeSet::OTHER_VECLIKE,
                    _ => TypeSet::for_constant(bits),
                };
                Ok(ty.with_singleton(bits))
            }
        }
    }

    fn raw_float(&self, bits: u64) -> Cell {
        let identity = self.next_float.get();
        self.next_float.set(identity + 1);
        Cell::F64 { bits, identity }
    }

    fn run(mut self) -> Result<Run, EvalError> {
        let function = self.func;
        let mut block = self.func.entry;
        loop {
            self.tick()?;
            let data = &function.blocks[block.index()];
            let mut done = std::collections::HashSet::new();
            // Entry values are definitions of the initial operand stack, not
            // source operations. They must exist before source pc 0 is read.
            for &id in &data.insts {
                if matches!(
                    self.func.insts[id.index()].op,
                    Opcode::Arg(_) | Opcode::OsrSlot(_)
                ) {
                    self.inst(id)?;
                    done.insert(id);
                }
            }
            let source_pcs = function
                .source_states
                .iter()
                .enumerate()
                .filter_map(|(pc, state)| {
                    state
                        .as_ref()
                        .filter(|state| state.block == block)
                        .map(|_| pc)
                })
                .collect::<Vec<_>>();
            for pc in source_pcs {
                let state = function.source_states[pc].as_ref().unwrap();
                let frame = &function.frames[state.frame.index()];
                self.trace.push(self.snapshot(
                    pc as u32,
                    &state.pre,
                    frame.handlers,
                    frame.binds,
                )?);
                for &id in &data.insts {
                    if self.func.insts[id.index()].pc as usize == pc && done.insert(id) {
                        if let Some(frame) = self.inst(id)? {
                            return Ok(Run {
                                outcome: Outcome::Deopt(frame),
                                trace: self.trace,
                            });
                        }
                    }
                }
            }
            for &id in &data.insts {
                if done.insert(id) {
                    if let Some(frame) = self.inst(id)? {
                        return Ok(Run {
                            outcome: Outcome::Deopt(frame),
                            trace: self.trace,
                        });
                    }
                }
            }
            let edge = match &data.term {
                Term::Return(value) => {
                    return Ok(Run {
                        outcome: Outcome::Returned(ValueBits::from_value(self.lisp(*value)?)),
                        trace: self.trace,
                    });
                }
                Term::Deopt(frame) => {
                    return Ok(Run {
                        outcome: Outcome::Deopt(self.frame(*frame)?),
                        trace: self.trace,
                    });
                }
                Term::Jump(edge) => edge,
                Term::Branch {
                    flag,
                    if_true,
                    if_false,
                } => {
                    if self.flag(*flag)? {
                        if_true
                    } else {
                        if_false
                    }
                }
                Term::Switch {
                    value,
                    table,
                    cases,
                    default,
                } => {
                    let dispatch = self.lisp(*value)?;
                    let table = self.lisp(*table)?;
                    let ht = table
                        .as_hash_table()
                        .ok_or_else(|| invalid("switch table"))?;
                    match ht.switch_target(dispatch, self.ctx.symbols_with_pos_enabled) {
                        Some(target) => {
                            let raw = target
                                .as_fixnum()
                                .ok_or_else(|| invalid("switch target type"))?;
                            &cases
                                .iter()
                                .find(|case| case.key == raw)
                                .ok_or_else(|| invalid("switch target changed"))?
                                .edge
                        }
                        None => default,
                    }
                }
                Term::Unreachable => return Err(invalid("reached unreachable block")),
            };
            self.transfer(edge)?;
            block = edge.target;
        }
    }
}

fn compare<T: PartialOrd>(cc: Cmp, left: T, right: T) -> bool {
    match cc {
        Cmp::Eq => left == right,
        Cmp::Ne => left != right,
        Cmp::Lt => left < right,
        Cmp::Le => left <= right,
        Cmp::Gt => left > right,
        Cmp::Ge => left >= right,
    }
}

/// Execute one operator through Tier-0, preserving its native operand order.
/// This frame contains no user binding operation; binding/handler-bearing IR
/// remains inadmissible in O1/O2 and cannot be silently approximated here.
fn evaluate_opaque(
    ctx: &mut Context,
    op: &Op,
    constants: &[ValueBits],
    args: &[LispValue],
    has_result: bool,
) -> Result<LispValue, EvalError> {
    if matches!(
        op,
        Op::VarBind(_)
            | Op::Unbind(_)
            | Op::SaveCurrentBuffer
            | Op::SaveExcursion
            | Op::SaveRestriction
            | Op::UnwindProtectPop
            | Op::SaveWindowExcursion
            | Op::PushConditionCase(_)
            | Op::PushConditionCaseRaw(_)
            | Op::PushCatch(_)
            | Op::PopHandler
            | Op::Throw
    ) {
        return Err(invalid(
            "binding/handler operator requires a persistent interpreter frame",
        ));
    }
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: (0..args.len())
            .map(|_| crate::emacs_core::intern::intern("opt-reference-operand"))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    function.lexical = true;
    function.constants = constants
        .iter()
        .map(|bits| bits.to_value())
        .collect::<Vec<_>>()
        .into();
    function.max_stack =
        u16::try_from(args.len() + 8).map_err(|_| invalid("operator stack too large"))?;
    function.ops.push(op.clone());
    if !has_result {
        function.ops.push(Op::Nil);
    }
    function.ops.push(Op::Return);
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(&function, args.to_vec())
        .map_err(EvalError::Flow)
}
