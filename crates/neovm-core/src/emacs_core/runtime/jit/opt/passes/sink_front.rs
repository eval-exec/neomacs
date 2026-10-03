//! Numeric recipe definitions and explicit four-field SSA phi transport.
//! Threading: exclusively invocation-owned Func/ID scratch. Feedback is an
//! immutable admission snapshot, never proof of a Float-only operand/result.

use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::opt::{
    ir::{Block, Edge, Func, Inst, InstData, Opcode, Rep, Term, Value, ValueData, ValueDef},
    mem::{AliasClass, Effects},
    types::TypeSet,
    verify::VerifyError,
};
use std::collections::HashMap;
// Future registered path; this is the schema in t34-o36-recipe-contract.rs.
use crate::emacs_core::jit::opt::sink_recipes::{
    NumericFields, NumericMode, OwnerRecipe, RecipeEdge, RecipeField, RecipeFields, RecipeKind,
    RecipeOrigin, RecipeVersion, RecipeVersionId, SinkOp, SinkRecipes, VersionCause,
};

pub(crate) fn selected_source(inst: &InstData, feedback: &[NumericFeedback]) -> bool {
    matches!(
        &inst.op,
        Opcode::Opaque(Op::Add | Op::Sub | Op::Mul | Op::Div)
    ) && inst.args.len() == 2
        && inst.result.is_some()
        && inst.frame.is_some()
        && inst.mem == AliasClass::None
        && inst.eff.contains(Effects::MAY_DEOPT)
        && feedback.get(inst.pc as usize) == Some(&NumericFeedback::Float)
}

fn append_value(
    func: &mut Func,
    op: Opcode,
    args: Vec<Value>,
    ty: TypeSet,
    rep: Rep,
    pc: u32,
    order: &mut Vec<Inst>,
) -> Value {
    let id = Inst(func.insts.len() as u32);
    let result = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty,
        rep,
        def: ValueDef::Inst(id),
    });
    func.insts.push(InstData {
        op,
        args,
        result: Some(result),
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc,
    });
    order.push(id);
    result
}

fn numeric_projections(
    func: &mut Func,
    owner: Value,
    pc: u32,
    box_type: TypeSet,
    order: &mut Vec<Inst>,
) -> NumericFields {
    NumericFields {
        payload: append_value(
            func,
            Opcode::Sink(SinkOp::RecipeField(RecipeField::Payload)),
            vec![owner],
            TypeSet::FLOAT,
            Rep::RawF64,
            pc,
            order,
        ),
        word: append_value(
            func,
            Opcode::Sink(SinkOp::RecipeField(RecipeField::Word)),
            vec![owner],
            TypeSet::TOP,
            Rep::RawWord,
            pc,
            order,
        ),
        ready: append_value(
            func,
            Opcode::Sink(SinkOp::RecipeField(RecipeField::Ready)),
            vec![owner],
            TypeSet::BOOLEAN,
            Rep::Bool,
            pc,
            order,
        ),
        real_box: append_value(
            func,
            Opcode::Sink(SinkOp::RecipeField(RecipeField::RealBox)),
            vec![owner],
            box_type,
            Rep::Tagged,
            pc,
            order,
        ),
    }
}

fn definition(
    table: &mut SinkRecipes,
    owner: Value,
    kind: RecipeKind,
    semantic_type: TypeSet,
    origin: RecipeOrigin,
    fields: NumericFields,
    cause: VersionCause,
) -> RecipeVersionId {
    let version = RecipeVersionId(table.versions.len() as u32);
    table.versions.push(RecipeVersion {
        owner,
        fields: RecipeFields::Number(fields),
        cause,
    });
    table.owners.insert(
        owner,
        OwnerRecipe {
            owner,
            kind,
            semantic_type,
            origin,
            definition_version: version,
        },
    );
    version
}

/// Actual Tagged seeds are borrowed without guards/dereference. TaggedFix is
/// still an actual word; its box projection is a Tagged exact-fix view. RawInt,
/// Bool, RawF64 and other non-word leaves require an explicit semantic adapter
/// before reaching this helper; discovery declines them initially.
pub(crate) fn borrow_word(
    func: &mut Func,
    table: &mut SinkRecipes,
    original: Value,
    pc: u32,
    order: &mut Vec<Inst>,
) -> Result<Value, VerifyError> {
    let original = func
        .resolve(original)
        .ok_or(VerifyError::AliasCycle(original))?;
    let input = &func.values[original.index()];
    if !input.rep.is_tagged() || input.ty.is_bottom() {
        return Err(VerifyError::RepMismatch {
            inst: None,
            value: original,
        });
    }
    let ty = input.ty;
    let owner = append_value(
        func,
        Opcode::Sink(SinkOp::BorrowNum),
        vec![original],
        ty,
        Rep::NumPair,
        pc,
        order,
    );
    let ValueDef::Inst(id) = func.values[owner.index()].def else {
        unreachable!()
    };
    let fields = numeric_projections(func, owner, pc, ty, order);
    definition(
        table,
        owner,
        RecipeKind::Number(NumericMode::Borrowable),
        ty,
        RecipeOrigin::Borrow { inst: id, original },
        fields,
        VersionCause::Definition,
    );
    Ok(owner)
}

/// Original source Inst/result/frame/PC are kept; projections are appended
/// immediately AFTER it at the same source PC. New result is FIX|FLOAT only
/// because unsupported successes deopt BEFORE the original operation executes.
pub(crate) fn rewrite_source(
    func: &mut Func,
    table: &mut SinkRecipes,
    inst: Inst,
    operands: [Value; 2],
    order: &mut Vec<Inst>,
) -> Result<Value, VerifyError> {
    let original = func.insts[inst.index()].clone();
    let Opcode::Opaque(op @ (Op::Add | Op::Sub | Op::Mul | Op::Div)) = original.op else {
        return Err(VerifyError::InvalidInst(inst));
    };
    let owner = original.result.ok_or(VerifyError::OperandArity(inst))?;
    let frame = original.frame.ok_or(VerifyError::MissingFrame(inst))?;
    let ty = func.values[owner.index()]
        .ty
        .meet(TypeSet::FIXNUM.join(TypeSet::FLOAT));
    if ty.is_bottom() {
        return Err(VerifyError::TypeMismatch(owner));
    }
    func.values[owner.index()].ty = ty;
    func.values[owner.index()].rep = Rep::NumPair;
    let data = &mut func.insts[inst.index()];
    data.op = Opcode::Sink(SinkOp::SourceNum(op.clone()));
    data.args = operands.into();
    // Preserve original guard effects, source frame, memory declaration and PC.
    order.push(inst);
    let fields = numeric_projections(
        func,
        owner,
        original.pc,
        TypeSet::FIXNUM.join(TypeSet::NIL),
        order,
    );
    definition(
        table,
        owner,
        RecipeKind::Number(NumericMode::FixOrFloat),
        ty,
        RecipeOrigin::NumericSource {
            inst,
            original_op: op,
            frame,
            pc: original.pc,
        },
        fields,
        VersionCause::Definition,
    );
    Ok(owner)
}

/// Append four real physical params. Original logical param ID/index remains
/// present, but native transport declares machine params ONLY for its fields.
pub(crate) fn append_phi_fields(
    func: &mut Func,
    table: &mut SinkRecipes,
    owner: Value,
) -> Result<NumericFields, VerifyError> {
    let ValueDef::Param { block, .. } = func.values[owner.index()].def else {
        return Err(VerifyError::BadDefinition(owner));
    };
    let ty = func.values[owner.index()].ty;
    if !func.values[owner.index()].rep.is_tagged() || ty.is_bottom() {
        return Err(VerifyError::TypeMismatch(owner));
    }
    func.values[owner.index()].rep = Rep::NumPair;
    let mut param = |rep, ty| {
        let value = Value(func.values.len() as u32);
        let index = func.blocks[block.index()].params.len() as u32;
        func.values.push(ValueData {
            ty,
            rep,
            def: ValueDef::Param { block, index },
        });
        func.blocks[block.index()].params.push(value);
        value
    };
    let fields = NumericFields {
        payload: param(Rep::RawF64, TypeSet::FLOAT),
        word: param(Rep::RawWord, TypeSet::TOP),
        ready: param(Rep::Bool, TypeSet::BOOLEAN),
        // Original verified edge operands fit this logical phi declaration.
        // Borrow/cached words preserve that semantic type; fresh virtual
        // results additionally carry NIL while their actual box is absent.
        // Feedback does not establish or narrow this declaration.
        real_box: param(Rep::Tagged, ty.join(TypeSet::NIL)),
    };
    definition(
        table,
        owner,
        RecipeKind::Number(NumericMode::Borrowable),
        ty,
        RecipeOrigin::Phi {
            block,
            field_params: numeric_values(fields).into(),
        },
        fields,
        VersionCause::Parameter,
    );
    Ok(fields)
}

pub(crate) fn numeric_values(fields: NumericFields) -> Vec<Value> {
    vec![fields.payload, fields.word, fields.ready, fields.real_box]
}

/// Preserve distinct edge occurrences, including two branch edges with one
/// target. Use an explicit match instead of collapsing predecessors by Block.
pub(crate) fn edge_mut(term: &mut Term, index: usize) -> Option<&mut Edge> {
    match term {
        Term::Jump(edge) => (index == 0).then_some(edge),
        Term::Branch {
            if_true, if_false, ..
        } => match index {
            0 => Some(if_true),
            1 => Some(if_false),
            _ => None,
        },
        Term::Switch { cases, default, .. } => {
            if index < cases.len() {
                Some(&mut cases[index].edge)
            } else {
                (index == cases.len()).then_some(default)
            }
        }
        _ => None,
    }
}

/// Invoke in the same owner order used to append field params for the target.
/// The version/escape phase replaces default versions with exact edge-current
/// SSA box versions; this intermediate helper cannot publish a final plan.
pub(crate) fn append_edge_fields(
    func: &mut Func,
    table: &mut SinkRecipes,
    source: Block,
    edge_index: usize,
    owner: Value,
    incoming: Value,
    version: RecipeVersionId,
) -> Result<(), VerifyError> {
    let ValueDef::Param {
        block: target,
        index,
    } = func.values[owner.index()].def
    else {
        return Err(VerifyError::BadDefinition(owner));
    };
    let RecipeFields::Number(fields) = table.versions[version.0 as usize].fields else {
        return Err(VerifyError::TypeMismatch(incoming));
    };
    let edge = edge_mut(&mut func.blocks[source.index()].term, edge_index)
        .ok_or(VerifyError::InvalidBlock(source))?;
    if edge.target != target || table.versions[version.0 as usize].owner != incoming {
        return Err(VerifyError::TypeMismatch(incoming));
    }
    edge.args[index as usize] = incoming;
    let args = numeric_values(fields);
    edge.args.extend_from_slice(&args);
    table.edges.push(RecipeEdge {
        source,
        edge_index: edge_index as u32,
        target,
        owner_param: owner,
        incoming_owner: incoming,
        incoming_version: version,
        field_args: args.into(),
    });
    Ok(())
}

/// Scope-local memoization only; a definition inserted on one sibling arm does
/// not dominate the other. Use actual dominance before sharing across blocks.
pub(crate) type BorrowMemo = HashMap<(Block, Value), Value>;
