//! Local physical shapes for selected numeric and Cons recipe instructions.
//!
//! Threading: this only borrows compilation-owned SSA and scalar metadata. It
//! never dereferences Lisp bits or supplies provenance from an annotation. The
//! independent recipe verifier runs after ordinary SSA validation and must
//! certify every logical owner, physical field and exact-point cache version.

use super::ir::{Func, Inst, InstData, Rep, Value, ValueData};
use super::mem::{AliasClass, Effects};
use super::sink_recipes::{NumericMode, RecipeField, RecipeKind, SinkOp};
use super::types::TypeSet;
use super::verify::VerifyError;
use crate::emacs_core::bytecode::opcode::Op;

/// A value-type exception is provisional, never a recipe capability. Only a
/// real table-owned Borrowable number may retain a nonnumeric original seed.
/// The final independent verifier proves Borrow/phi/view lineage and rejects
/// a forged owner table, including an unknown NumPair without that lineage.
pub(crate) fn owned_borrowable(func: &Func, value: Value, data: &ValueData) -> bool {
    func.sink_recipes.owners.get(&value).is_some_and(|owner| {
        owner.owner == value
            && owner.kind == RecipeKind::Number(NumericMode::Borrowable)
            && owner.semantic_type == data.ty
            && !data.ty.is_bottom()
    })
}

/// Detect sidecar data even when a malformed input has no actual sink op. This
/// avoids treating an unused forged owner/version/root view as verified data.
pub(crate) fn has_metadata(func: &Func) -> bool {
    let table = &func.sink_recipes;
    !table.owners.is_empty()
        || !table.versions.is_empty()
        || !table.frames.is_empty()
        || !table.uses.is_empty()
        || !table.edges.is_empty()
        || !table.source_sqrt_sites.is_empty()
}

fn value(func: &Func, input: Value) -> Result<(Value, &ValueData), VerifyError> {
    let id = func
        .resolve(input)
        .ok_or(VerifyError::InvalidValue(input))?;
    let data = func
        .values
        .get(id.index())
        .ok_or(VerifyError::InvalidValue(id))?;
    Ok((id, data))
}

fn owner_kind(func: &Func, input: Value, inst: Inst) -> Result<RecipeKind, VerifyError> {
    let (id, data) = value(func, input)?;
    let owner = func
        .sink_recipes
        .owners
        .get(&id)
        .ok_or(VerifyError::InvalidInst(inst))?;
    if owner.owner != id || owner.semantic_type != data.ty || data.ty.is_bottom() {
        return Err(VerifyError::TypeMismatch(id));
    }
    let valid = match owner.kind {
        RecipeKind::Number(_) => data.rep == Rep::NumPair,
        RecipeKind::Cons => matches!(data.rep, Rep::Virtual(_)) && data.ty.is_subset(TypeSet::CONS),
    };
    if !valid {
        return Err(VerifyError::RepMismatch {
            inst: Some(inst),
            value: id,
        });
    }
    Ok(owner.kind)
}

fn semantic_input(func: &Func, input: Value, inst: Inst) -> Result<(), VerifyError> {
    let (id, data) = value(func, input)?;
    if data.ty.is_bottom() {
        return Err(VerifyError::TypeMismatch(id));
    }
    if data.rep.is_tagged() {
        Ok(())
    } else {
        owner_kind(func, id, inst).map(|_| ())
    }
}

/// Exact local shapes only. Table presence here cannot authorize emission: a
/// successful Func::verify also runs the independent recipe provenance proof.
pub(crate) fn validate_inst(
    func: &Func,
    inst: Inst,
    data: &InstData,
    op: &SinkOp,
) -> Result<(), VerifyError> {
    let args = |count| {
        if data.args.len() == count {
            Ok(())
        } else {
            Err(VerifyError::OperandArity(inst))
        }
    };
    let input = |index| -> Result<(Value, &ValueData), VerifyError> {
        let id = *data
            .args
            .get(index)
            .ok_or(VerifyError::OperandArity(inst))?;
        value(func, id)
    };
    let result = || -> Result<(Value, &ValueData), VerifyError> {
        let id = data.result.ok_or(VerifyError::OperandArity(inst))?;
        value(func, id)
    };
    let rep = |index, want| -> Result<(), VerifyError> {
        let (id, actual) = input(index)?;
        if actual.rep != want {
            return Err(VerifyError::RepMismatch {
                inst: Some(inst),
                value: id,
            });
        }
        if actual.ty.is_bottom() {
            return Err(VerifyError::TypeMismatch(id));
        }
        Ok(())
    };
    let output = |want| -> Result<(), VerifyError> {
        let (id, actual) = result()?;
        if actual.rep != want {
            return Err(VerifyError::RepMismatch {
                inst: Some(inst),
                value: id,
            });
        }
        if actual.ty.is_bottom() {
            return Err(VerifyError::TypeMismatch(id));
        }
        Ok(())
    };
    let effects = |eff: Effects, mem: AliasClass, pure_frame: bool| {
        if data.eff == eff && data.mem == mem && (!pure_frame || data.frame.is_none()) {
            Ok(())
        } else {
            Err(VerifyError::InvalidInst(inst))
        }
    };
    let number = |id| -> Result<(), VerifyError> {
        if matches!(owner_kind(func, id, inst)?, RecipeKind::Number(_)) {
            Ok(())
        } else {
            Err(VerifyError::TypeMismatch(id))
        }
    };
    let source_frame = || {
        data.frame
            .map(|_| ())
            .ok_or(VerifyError::MissingFrame(inst))
    };
    match op {
        SinkOp::BorrowNum => {
            args(1)?;
            let (id, seed) = input(0)?;
            if !seed.rep.is_tagged() || seed.ty.is_bottom() {
                return Err(VerifyError::RepMismatch {
                    inst: Some(inst),
                    value: id,
                });
            }
            output(Rep::NumPair)?;
            number(result()?.0)?;
            if result()?.1.ty != seed.ty {
                return Err(VerifyError::TypeMismatch(result()?.0));
            }
            effects(Effects::PURE, AliasClass::None, true)?;
        }
        SinkOp::SourceNum(original) => {
            if !matches!(original, Op::Add | Op::Sub | Op::Mul | Op::Div) {
                return Err(VerifyError::InvalidInst(inst));
            }
            args(2)?;
            number(input(0)?.0)?;
            number(input(1)?.0)?;
            output(Rep::NumPair)?;
            number(result()?.0)?;
            if !result()?
                .1
                .ty
                .is_subset(TypeSet::FIXNUM.join(TypeSet::FLOAT))
            {
                return Err(VerifyError::TypeMismatch(result()?.0));
            }
            effects(
                Effects::ALLOCATES.with(Effects::MAY_DEOPT),
                AliasClass::None,
                false,
            )?;
            source_frame()?;
        }
        SinkOp::SourceSqrt => {
            args(2)?;
            let (callee, callee_data) = input(0)?;
            if !callee_data.rep.is_tagged() || callee_data.ty.is_bottom() {
                return Err(VerifyError::RepMismatch {
                    inst: Some(inst),
                    value: callee,
                });
            }
            number(input(1)?.0)?;
            output(Rep::NumPair)?;
            number(result()?.0)?;
            if !result()?.1.ty.is_subset(TypeSet::FLOAT) {
                return Err(VerifyError::TypeMismatch(result()?.0));
            }
            effects(Effects::UNKNOWN, AliasClass::Unknown, false)?;
            source_frame()?;
        }
        SinkOp::SourceCons(original) => {
            args(match original {
                Op::Cons => 2,
                Op::List(1) => 1,
                _ => return Err(VerifyError::InvalidInst(inst)),
            })?;
            for &input in &data.args {
                semantic_input(func, input, inst)?;
            }
            if owner_kind(func, result()?.0, inst)? != RecipeKind::Cons {
                return Err(VerifyError::TypeMismatch(result()?.0));
            }
            effects(Effects::ALLOCATES, AliasClass::None, false)?;
            source_frame()?;
        }
        SinkOp::RecipeField(field) => {
            args(1)?;
            let kind = owner_kind(func, input(0)?.0, inst)?;
            let want = match (kind, *field) {
                (RecipeKind::Number(_), RecipeField::Payload) => Rep::RawF64,
                (RecipeKind::Number(_), RecipeField::Word) => Rep::RawWord,
                (RecipeKind::Number(_), RecipeField::Ready) => Rep::Bool,
                (_, RecipeField::RealBox) => Rep::Tagged,
                // Cons fields are actual original semantic SSA args. The
                // initial supported source unit projects only its box cache.
                _ => return Err(VerifyError::InvalidInst(inst)),
            };
            output(want)?;
            effects(Effects::PURE, AliasClass::None, true)?;
        }
        SinkOp::MaterializeNum | SinkOp::MaterializeCons => {
            let numeric = matches!(op, SinkOp::MaterializeNum);
            args(if numeric { 5 } else { 4 })?;
            let owner = input(0)?.0;
            let kind = owner_kind(func, owner, inst)?;
            if numeric != matches!(kind, RecipeKind::Number(_)) {
                return Err(VerifyError::TypeMismatch(owner));
            }
            if numeric {
                rep(1, Rep::RawF64)?;
                rep(2, Rep::RawWord)?;
                rep(3, Rep::Bool)?;
                rep(4, Rep::Tagged)?;
            } else {
                semantic_input(func, input(1)?.0, inst)?;
                semantic_input(func, input(2)?.0, inst)?;
                rep(3, Rep::Tagged)?;
            }
            output(Rep::Tagged)?;
            if result()?.1.ty != func.sink_recipes.owners[&owner].semantic_type {
                return Err(VerifyError::TypeMismatch(result()?.0));
            }
            effects(Effects::ALLOCATES, AliasClass::None, false)?;
        }
        SinkOp::CacheBoxAfter => {
            args(3)?;
            let owner = input(0)?.0;
            owner_kind(func, owner, inst)?;
            rep(1, Rep::Tagged)?;
            rep(2, Rep::Tagged)?;
            output(Rep::Tagged)?;
            if result()?.1.ty != func.sink_recipes.owners[&owner].semantic_type {
                return Err(VerifyError::TypeMismatch(result()?.0));
            }
            effects(Effects::PURE, AliasClass::None, true)?;
        }
    }
    Ok(())
}
