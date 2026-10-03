//! Structural, dominance and representation checks for owned global SSA.
//!
//! Threading: the verifier only borrows compiler data. It never dereferences
//! opaque Lisp bits, mutates the source, or reads mutator-local runtime state.

use std::collections::HashSet;
use std::fmt;

use super::ir::*;
use super::mem::Effects;
use super::types::TypeSet;

/// A structural compiler error; threading: owned diagnostics, no Lisp values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VerifyError {
    InvalidEntry(Block),
    InvalidBlock(Block),
    InvalidInst(Inst),
    DuplicateInst(Inst),
    DetachedInst(Inst),
    InvalidValue(Value),
    AliasCycle(Value),
    BadDefinition(Value),
    BadParam(Block, Value),
    BadPredecessors(Block),
    EdgeArity(Block, Block),
    NonDominating {
        value: Value,
        block: Block,
        inst: Option<Inst>,
    },
    MissingFrame(Inst),
    InvalidFrame(FrameId),
    FrameCycle(FrameId),
    MissingGuardEffect(Inst),
    RepMismatch {
        inst: Option<Inst>,
        value: Value,
    },
    TypeMismatch(Value),
    OperandArity(Inst),
    ConstantIndex(Inst),
    ArgumentIndex(Inst),
    OsrShape,
    SourceState(u32),
    PublishedNonTagged(Value),
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid opt IR: {self:?}")
    }
}
impl std::error::Error for VerifyError {}

impl Func {
    pub(crate) fn verify(&self) -> Result<(), VerifyError> {
        verify(self)
    }
}

pub(crate) fn verify(func: &Func) -> Result<(), VerifyError> {
    let get_block = |block: Block| {
        func.blocks
            .get(block.index())
            .ok_or(VerifyError::InvalidBlock(block))
    };
    get_block(func.entry).map_err(|_| VerifyError::InvalidEntry(func.entry))?;
    let mut actual_preds = vec![Vec::new(); func.blocks.len()];
    let mut positions = vec![None; func.insts.len()];
    for (index, data) in func.blocks.iter().enumerate() {
        let block = Block(index as u32);
        for (param_index, &param) in data.params.iter().enumerate() {
            let value = func
                .values
                .get(param.index())
                .ok_or(VerifyError::InvalidValue(param))?;
            if value.def
                != (ValueDef::Param {
                    block,
                    index: param_index as u32,
                })
            {
                return Err(VerifyError::BadParam(block, param));
            }
        }
        for (position, &inst) in data.insts.iter().enumerate() {
            let instruction = func
                .insts
                .get(inst.index())
                .ok_or(VerifyError::InvalidInst(inst))?;
            if let Some(result) = instruction.result {
                if func
                    .values
                    .get(result.index())
                    .ok_or(VerifyError::InvalidValue(result))?
                    .def
                    != ValueDef::Inst(inst)
                {
                    return Err(VerifyError::BadDefinition(result));
                }
            }
            let owner = positions
                .get_mut(inst.index())
                .ok_or(VerifyError::InvalidInst(inst))?;
            if owner.replace((block, position)).is_some() {
                return Err(VerifyError::DuplicateInst(inst));
            }
        }
        for edge in data.term.edges() {
            let target = get_block(edge.target)?;
            if target.params.len() != edge.args.len() {
                return Err(VerifyError::EdgeArity(block, edge.target));
            }
            actual_preds[edge.target.index()].push(block);
        }
    }
    for (index, (data, actual)) in func.blocks.iter().zip(&mut actual_preds).enumerate() {
        actual.sort_unstable();
        actual.dedup();
        let mut declared = data.preds.clone();
        declared.sort_unstable();
        declared.dedup();
        if &declared != actual {
            return Err(VerifyError::BadPredecessors(Block(index as u32)));
        }
    }
    for (index, position) in positions.iter().enumerate() {
        if position.is_none() {
            return Err(VerifyError::DetachedInst(Inst(index as u32)));
        }
    }
    for (index, value) in func.values.iter().enumerate() {
        let id = Value(index as u32);
        match value.def {
            ValueDef::Param { block, index } => {
                if get_block(block)?.params.get(index as usize) != Some(&id) {
                    return Err(VerifyError::BadDefinition(id));
                }
            }
            ValueDef::Inst(inst) => {
                if func
                    .insts
                    .get(inst.index())
                    .ok_or(VerifyError::InvalidInst(inst))?
                    .result
                    != Some(id)
                {
                    return Err(VerifyError::BadDefinition(id));
                }
            }
            ValueDef::Alias(other) => {
                func.values
                    .get(other.index())
                    .ok_or(VerifyError::InvalidValue(other))?;
                if func.resolve(id).is_none() {
                    return Err(VerifyError::AliasCycle(id));
                }
            }
        }
        validate_value_type(id, value)?;
    }
    for (index, _) in func.frames.iter().enumerate() {
        let frame = FrameId(index as u32);
        let mut seen = HashSet::new();
        let mut current = Some(frame);
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(VerifyError::FrameCycle(id));
            }
            let state = func
                .frames
                .get(id.index())
                .ok_or(VerifyError::InvalidFrame(id))?;
            for &value in &state.stack {
                func.values
                    .get(value.index())
                    .ok_or(VerifyError::InvalidValue(value))?;
            }
            current = state.parent;
        }
    }
    let dominance = Dominance::new(func, &actual_preds);
    let check_use = |value, block, position, inst| {
        check_dominance(func, &positions, &dominance, value, block, position, inst)
    };
    let check_frame = |frame: FrameId, block, position, inst| -> Result<(), VerifyError> {
        let mut current = Some(frame);
        while let Some(frame) = current {
            let data = func
                .frames
                .get(frame.index())
                .ok_or(VerifyError::InvalidFrame(frame))?;
            for &value in &data.stack {
                check_use(value, block, position, inst)?;
            }
            current = data.parent;
        }
        Ok(())
    };
    for (index, block_data) in func.blocks.iter().enumerate() {
        let block = Block(index as u32);
        for (position, &inst) in block_data.insts.iter().enumerate() {
            let data = &func.insts[inst.index()];
            for &arg in &data.args {
                check_use(arg, block, position, Some(inst))?;
            }
            if data.op.requires_frame(data.eff) && data.frame.is_none() {
                return Err(VerifyError::MissingFrame(inst));
            }
            if data.op.is_guard() && !data.eff.contains(Effects::MAY_DEOPT) {
                return Err(VerifyError::MissingGuardEffect(inst));
            }
            if let Some(frame) = data.frame {
                check_frame(frame, block, position, Some(inst))?;
            }
            validate_inst(func, inst, data)?;
        }
        let position = block_data.insts.len();
        match &block_data.term {
            Term::Branch { flag, .. } => {
                check_use(*flag, block, position, None)?;
                let rep = func.values[func
                    .resolve(*flag)
                    .ok_or(VerifyError::AliasCycle(*flag))?
                    .index()]
                .rep;
                if !matches!(rep, Rep::Bool | Rep::Tagged | Rep::TaggedFix) {
                    return Err(VerifyError::RepMismatch {
                        inst: None,
                        value: *flag,
                    });
                }
            }
            Term::Switch {
                value,
                table,
                cases,
                ..
            } => {
                let mut keys = HashSet::new();
                if cases.iter().any(|case| !keys.insert(case.key)) {
                    return Err(VerifyError::SourceState(block_data.pc));
                }
                for &value in [value, table] {
                    check_use(value, block, position, None)?;
                    require_tagged(func, value, None)?;
                }
            }
            Term::Return(value) => {
                check_use(*value, block, position, None)?;
                require_tagged(func, *value, None)?;
            }
            Term::Deopt(frame) => check_frame(*frame, block, position, None)?,
            Term::Jump(_) | Term::Unreachable => {}
        }
        for edge in block_data.term.edges() {
            for (&arg, &param) in edge
                .args
                .iter()
                .zip(&func.blocks[edge.target.index()].params)
            {
                check_use(arg, block, position, None)?;
                let arg_data = &func.values[func
                    .resolve(arg)
                    .ok_or(VerifyError::AliasCycle(arg))?
                    .index()];
                let param_data = &func.values[param.index()];
                if arg_data.rep != param_data.rep
                    && !(arg_data.rep.is_tagged() && param_data.rep.is_tagged())
                {
                    return Err(VerifyError::RepMismatch {
                        inst: None,
                        value: arg,
                    });
                }
                if !arg_data.ty.is_subset(param_data.ty) {
                    return Err(VerifyError::TypeMismatch(arg));
                }
            }
        }
    }
    if !func.entry_stacks.is_empty() {
        if func.entry_stacks.len() != func.blocks.len() {
            return Err(VerifyError::SourceState(0));
        }
        for (index, stack) in func.entry_stacks.iter().enumerate() {
            // Normal and OSR entries first seed the GNU stack with Arg or
            // OsrSlot instructions; those seeds are available at its first op.
            let block = Block(index as u32);
            let position = if block == func.entry {
                func.blocks[index]
                    .insts
                    .iter()
                    .take_while(|&&inst| {
                        matches!(
                            func.insts[inst.index()].op,
                            Opcode::Arg(_) | Opcode::OsrSlot(_)
                        )
                    })
                    .count()
            } else {
                0
            };
            for &value in stack {
                check_use(value, block, position, None)?;
            }
        }
    }
    for (pc, source) in func.source_states.iter().enumerate() {
        if let Some(source) = source {
            let block = get_block(source.block)?;
            let pre_position = block
                .insts
                .iter()
                .take_while(|&&inst| {
                    let data = &func.insts[inst.index()];
                    data.pc < pc as u32 || matches!(data.op, Opcode::Arg(_) | Opcode::OsrSlot(_))
                })
                .count();
            let post_position = block
                .insts
                .iter()
                .take_while(|&&inst| {
                    let data = &func.insts[inst.index()];
                    data.pc <= pc as u32 || matches!(data.op, Opcode::Arg(_) | Opcode::OsrSlot(_))
                })
                .count();
            let frame = func
                .frames
                .get(source.frame.index())
                .ok_or(VerifyError::InvalidFrame(source.frame))?;
            // Pure fused regions replay the caller call-site state, which
            // deliberately differs from the callee's current operand stack.
            if frame.site.is_none() && frame.stack.as_ref() != source.pre.as_ref() {
                return Err(VerifyError::SourceState(pc as u32));
            }
            check_frame(source.frame, source.block, pre_position, None)?;
            for &value in &source.pre {
                check_use(value, source.block, pre_position, None)?;
            }
            for &value in &source.post {
                check_use(value, source.block, post_position, None)?;
            }
        }
    }
    if let Some(osr) = &func.osr {
        let header = get_block(osr.header)?;
        if header.pc != osr.entry_pc
            || (!func.entry_stacks.is_empty()
                && func.entry_stacks[osr.header.index()].len() != osr.depth)
        {
            return Err(VerifyError::OsrShape);
        }
    }
    Ok(())
}

fn validate_value_type(value: Value, data: &ValueData) -> Result<(), VerifyError> {
    let valid = match data.rep {
        Rep::Tagged => true,
        Rep::TaggedFix | Rep::RawInt => data.ty.is_subset(TypeSet::FIXNUM),
        Rep::RawF64 => data.ty.is_subset(TypeSet::FLOAT),
        Rep::NumPair => data.ty.is_subset(TypeSet::FIXNUM.join(TypeSet::FLOAT)),
        Rep::Bool => data.ty.is_subset(TypeSet::BOOLEAN),
        Rep::RawPtr { .. } => true,
        Rep::Virtual(_) => data.ty.is_subset(TypeSet::CONS.join(TypeSet::FLOAT)),
    };
    if valid {
        Ok(())
    } else {
        Err(VerifyError::TypeMismatch(value))
    }
}

fn require_tagged(func: &Func, value: Value, inst: Option<Inst>) -> Result<(), VerifyError> {
    let resolved = func
        .resolve(value)
        .ok_or(VerifyError::InvalidValue(value))?;
    if func.values[resolved.index()].rep.is_tagged() {
        Ok(())
    } else {
        Err(VerifyError::RepMismatch { inst, value })
    }
}

fn validate_inst(func: &Func, inst: Inst, data: &InstData) -> Result<(), VerifyError> {
    let operand = |index: usize| -> Result<(Value, &ValueData), VerifyError> {
        let value = *data
            .args
            .get(index)
            .ok_or(VerifyError::OperandArity(inst))?;
        let resolved = func
            .resolve(value)
            .ok_or(VerifyError::InvalidValue(value))?;
        Ok((value, &func.values[resolved.index()]))
    };
    let result = || -> Result<(Value, &ValueData), VerifyError> {
        let value = data.result.ok_or(VerifyError::OperandArity(inst))?;
        Ok((
            value,
            func.values
                .get(value.index())
                .ok_or(VerifyError::InvalidValue(value))?,
        ))
    };
    let rep = |index: usize, expected: Rep| -> Result<(), VerifyError> {
        let (value, actual) = operand(index)?;
        if actual.rep != expected {
            return Err(VerifyError::RepMismatch {
                inst: Some(inst),
                value,
            });
        }
        Ok(())
    };
    let result_rep = |expected: Rep| -> Result<(), VerifyError> {
        let (value, actual) = result()?;
        if actual.rep != expected {
            return Err(VerifyError::RepMismatch {
                inst: Some(inst),
                value,
            });
        }
        Ok(())
    };
    let args = |count| {
        if data.args.len() == count {
            Ok(())
        } else {
            Err(VerifyError::OperandArity(inst))
        }
    };
    match &data.op {
        Opcode::Const(index) | Opcode::EnvConst(index) => {
            args(0)?;
            if *index as usize >= func.consts.len()
                || matches!(data.op, Opcode::EnvConst(_)) && *index as usize >= func.dynamic_prefix
            {
                return Err(VerifyError::ConstantIndex(inst));
            }
            require_tagged(func, result()?.0, Some(inst))?;
        }
        Opcode::Arg(index) => {
            args(0)?;
            if *index as usize >= func.arity.native_arity() {
                return Err(VerifyError::ArgumentIndex(inst));
            }
            require_tagged(func, result()?.0, Some(inst))?;
        }
        Opcode::OsrSlot(index) => {
            args(0)?;
            if func
                .osr
                .as_ref()
                .is_none_or(|osr| *index as usize >= osr.depth)
            {
                return Err(VerifyError::ArgumentIndex(inst));
            }
            require_tagged(func, result()?.0, Some(inst))?;
        }
        Opcode::TagFix => {
            args(1)?;
            rep(0, Rep::RawInt)?;
            require_tagged(func, result()?.0, Some(inst))?;
        }
        Opcode::UntagFix => {
            args(1)?;
            require_tagged(func, operand(0)?.0, Some(inst))?;
            result_rep(Rep::RawInt)?;
        }
        Opcode::UnboxF64 => {
            args(1)?;
            require_tagged(func, operand(0)?.0, Some(inst))?;
            result_rep(Rep::RawF64)?;
        }
        Opcode::BoolToLisp => {
            args(1)?;
            rep(0, Rep::Bool)?;
            require_tagged(func, result()?.0, Some(inst))?;
        }
        Opcode::IsNonNil | Opcode::TypeTest(_) => {
            args(1)?;
            require_tagged(func, operand(0)?.0, Some(inst))?;
            result_rep(Rep::Bool)?;
        }
        Opcode::Eq => {
            args(2)?;
            for &value in &data.args {
                require_tagged(func, value, Some(inst))?;
            }
            result_rep(Rep::Bool)?;
        }
        Opcode::FixAdd { .. }
        | Opcode::FixSub { .. }
        | Opcode::FixMul { .. }
        | Opcode::FixDiv
        | Opcode::FixRem
        | Opcode::FixMinMax(_) => {
            args(2)?;
            let expected = result()?.1.rep;
            if !matches!(expected, Rep::TaggedFix | Rep::RawInt) {
                return Err(VerifyError::RepMismatch {
                    inst: Some(inst),
                    value: result()?.0,
                });
            }
            rep(0, expected)?;
            rep(1, expected)?;
        }
        Opcode::FixCmp(_) => {
            args(2)?;
            let expected = operand(0)?.1.rep;
            if !matches!(expected, Rep::TaggedFix | Rep::RawInt) {
                return Err(VerifyError::RepMismatch {
                    inst: Some(inst),
                    value: operand(0)?.0,
                });
            }
            rep(1, expected)?;
            result_rep(Rep::Bool)?;
        }
        Opcode::F64Add | Opcode::F64Sub | Opcode::F64Mul | Opcode::F64Div => {
            args(2)?;
            rep(0, Rep::RawF64)?;
            rep(1, Rep::RawF64)?;
            result_rep(Rep::RawF64)?;
        }
        Opcode::F64Cmp(_) => {
            args(2)?;
            rep(0, Rep::RawF64)?;
            rep(1, Rep::RawF64)?;
            result_rep(Rep::Bool)?;
        }
        Opcode::F64Neg | Opcode::F64Sqrt => {
            args(1)?;
            rep(0, Rep::RawF64)?;
            result_rep(Rep::RawF64)?;
        }
        Opcode::F64FromFix => {
            args(1)?;
            if !matches!(operand(0)?.1.rep, Rep::TaggedFix | Rep::RawInt) {
                return Err(VerifyError::RepMismatch {
                    inst: Some(inst),
                    value: operand(0)?.0,
                });
            }
            result_rep(Rep::RawF64)?;
        }
        Opcode::CheckType(ty) | Opcode::Refine(ty) => {
            args(1)?;
            let (input, actual) = operand(0)?;
            let (output, output_data) = result()?;
            if actual.rep != output_data.rep {
                return Err(VerifyError::RepMismatch {
                    inst: Some(inst),
                    value: input,
                });
            }
            if !output_data.ty.is_subset(actual.ty.meet(*ty)) {
                return Err(VerifyError::TypeMismatch(output));
            }
        }
        Opcode::CheckNonZero | Opcode::CheckEq(_) => {
            args(1)?;
        }
        Opcode::CheckBounds => {
            args(2)?;
        }
        Opcode::CheckNoOverflow => {
            args(1)?;
            rep(0, Rep::Bool)?;
        }
        Opcode::Select => {
            args(3)?;
            rep(0, Rep::Bool)?;
            let expected = result()?.1.rep;
            rep(1, expected)?;
            rep(2, expected)?;
        }
        Opcode::LoadVecSlots => {
            args(1)?;
            require_tagged(func, operand(0)?.0, Some(inst))?;
            let (value, data) = result()?;
            if data.rep
                != (Rep::RawPtr {
                    base: operand(0)?.0,
                })
            {
                return Err(VerifyError::RepMismatch {
                    inst: Some(inst),
                    value,
                });
            }
        }
        Opcode::LoadF64 => {
            args(1)?;
            require_tagged(func, operand(0)?.0, Some(inst))?;
            result_rep(Rep::RawF64)?;
        }
        Opcode::AllocFloat => {
            args(1)?;
            rep(0, Rep::RawF64)?;
            require_tagged(func, result()?.0, Some(inst))?;
        }
        Opcode::PublishRoot => {
            args(1)?;
            let (value, actual) = operand(0)?;
            if actual.rep != Rep::Tagged {
                return Err(VerifyError::PublishedNonTagged(value));
            }
        }
        Opcode::Call { .. } | Opcode::Builtin(_) | Opcode::Opaque(_) => {
            for &value in &data.args {
                require_tagged(func, value, Some(inst))?;
            }
            if let Some(value) = data.result {
                require_tagged(func, value, Some(inst))?;
            }
        }
        Opcode::LoadCar | Opcode::LoadCdr | Opcode::LoadVecLen | Opcode::LoadRecTag => {
            args(1)?;
            require_tagged(func, operand(0)?.0, Some(inst))?;
            require_tagged(func, result()?.0, Some(inst))?;
        }
        Opcode::StoreCar | Opcode::StoreCdr | Opcode::AllocCons => {
            args(2)?;
            for &value in &data.args {
                require_tagged(func, value, Some(inst))?;
            }
            if let Some(value) = data.result {
                require_tagged(func, value, Some(inst))?;
            }
        }
        Opcode::LoadVecElem | Opcode::StoreVecElem => {
            args(if matches!(data.op, Opcode::LoadVecElem) {
                2
            } else {
                3
            })?;
            // Element addressing may consume a derived pointer and raw index.
            if let Some(value) = data.result {
                require_tagged(func, value, Some(inst))?;
            }
        }
        Opcode::LoadSymValue(_) => {
            args(0)?;
            require_tagged(func, result()?.0, Some(inst))?;
        }
        Opcode::StoreSymValue(_) => {
            args(1)?;
            require_tagged(func, operand(0)?.0, Some(inst))?;
        }
        Opcode::InlineEntry(_) => {
            for &value in &data.args {
                require_tagged(func, value, Some(inst))?;
            }
        }
        Opcode::Poll => {
            args(0)?;
        }
    }
    Ok(())
}

fn check_dominance(
    func: &Func,
    positions: &[Option<(Block, usize)>],
    dom: &Dominance,
    value: Value,
    block: Block,
    position: usize,
    inst: Option<Inst>,
) -> Result<(), VerifyError> {
    let resolved = func
        .resolve(value)
        .ok_or(VerifyError::InvalidValue(value))?;
    let data = &func.values[resolved.index()];
    let (def_block, def_position) = match data.def {
        ValueDef::Param { block, .. } => (block, None),
        ValueDef::Inst(def) => {
            let (block, position) = positions[def.index()].ok_or(VerifyError::DetachedInst(def))?;
            (block, Some(position))
        }
        ValueDef::Alias(_) => return Err(VerifyError::AliasCycle(value)),
    };
    let dominates = if def_block == block {
        def_position.is_none_or(|def| def < position)
    } else {
        dom.dominates(def_block, block)
    };
    if !dominates {
        return Err(VerifyError::NonDominating { value, block, inst });
    }
    if let Rep::RawPtr { base } = data.rep {
        // The keep-alive dependency belongs to every pointer use, including
        // framestates, so it cannot be accidentally dropped by a later pass.
        let resolved_base = func.resolve(base).ok_or(VerifyError::InvalidValue(base))?;
        if resolved_base == resolved
            || matches!(
                func.values.get(resolved_base.index()).map(|v| v.rep),
                Some(Rep::RawPtr { .. })
            )
        {
            return Err(VerifyError::RepMismatch { inst, value });
        }
        check_dominance(func, positions, dom, base, block, position, inst)?;
        if !func.values[func
            .resolve(base)
            .ok_or(VerifyError::InvalidValue(base))?
            .index()]
        .ty
        .may_need_root()
        {
            return Err(VerifyError::TypeMismatch(base));
        }
    }
    Ok(())
}

/// Cooper-style immediate dominators over reverse postorder: O(blocks) storage
/// rather than a quadratic dominance matrix. Threading: local scratch only.
struct Dominance {
    idom: Vec<Option<Block>>,
}

impl Dominance {
    fn new(func: &Func, preds: &[Vec<Block>]) -> Self {
        let mut visited = vec![false; func.blocks.len()];
        let mut post = Vec::new();
        let mut walk = vec![(func.entry, false)];
        while let Some((block, exiting)) = walk.pop() {
            if exiting {
                post.push(block);
                continue;
            }
            if std::mem::replace(&mut visited[block.index()], true) {
                continue;
            }
            walk.push((block, true));
            for edge in func.blocks[block.index()].term.edges().into_iter().rev() {
                if !visited[edge.target.index()] {
                    walk.push((edge.target, false));
                }
            }
        }
        post.reverse();
        let mut order = vec![usize::MAX; func.blocks.len()];
        for (index, &block) in post.iter().enumerate() {
            order[block.index()] = index;
        }
        let mut idom = vec![None; func.blocks.len()];
        idom[func.entry.index()] = Some(func.entry);
        let mut changed = true;
        while changed {
            changed = false;
            for &block in post.iter().skip(1) {
                let mut incoming = preds[block.index()]
                    .iter()
                    .copied()
                    .filter(|p| idom[p.index()].is_some());
                let Some(mut parent) = incoming.next() else {
                    continue;
                };
                for mut other in incoming {
                    while parent != other {
                        while order[parent.index()] > order[other.index()] {
                            parent = idom[parent.index()].expect("known predecessor");
                        }
                        while order[other.index()] > order[parent.index()] {
                            other = idom[other.index()].expect("known predecessor");
                        }
                    }
                }
                if idom[block.index()] != Some(parent) {
                    idom[block.index()] = Some(parent);
                    changed = true;
                }
            }
        }
        Self { idom }
    }

    fn dominates(&self, definition: Block, mut block: Block) -> bool {
        loop {
            if definition == block {
                return true;
            }
            match self.idom[block.index()] {
                Some(parent) if parent != block => block = parent,
                _ => return false,
            }
        }
    }
}
