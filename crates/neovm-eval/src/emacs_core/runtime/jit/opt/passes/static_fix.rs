//! Shared proof of actual immutable fixnum literal identity.
//! Caller must gate new recognition on the selected LICM stage. Both Reps cost
//! accounting and native immediate payload lowering must call this exact proof;
//! neither may trust a singleton hint without an actual immutable pool Const.
//!
//! Threading: per-compilation IDs/bits only; no Lisp heap access/runtime cache.

use crate::emacs_core::jit::opt::{ir::*, types::TypeSet};

/// A successful proof names the real nonprefix Const definition and actual
/// opaque tagged bits. Aliases and Refine can preserve the physical tagged
/// word; CheckType/TagFix/UntagFix, unknown/Env/OSR, phis and arithmetic do not
/// enter this origin proof. Every traversed view admits the actual literal.
pub(crate) fn static_fix_origin(func: &Func, mut value: Value) -> Option<(u32, ValueBits)> {
    let mut views = Vec::new();
    for _ in 0..=func.values.len() {
        value = func.resolve(value)?;
        let data = func.values.get(value.index())?;
        if !data.rep.is_tagged() || data.ty.is_bottom() || !data.ty.is_subset(TypeSet::FIXNUM) {
            return None;
        }
        views.push(data.ty);
        let ValueDef::Inst(id) = data.def else {
            return None;
        };
        let inst = func.insts.get(id.index())?;
        match inst.op {
            Opcode::Const(index) if index as usize >= func.dynamic_prefix => {
                let bits = *func.consts.get(index as usize)?;
                let actual = TypeSet::for_constant(bits);
                if actual.is_bottom()
                    || !actual.is_subset(TypeSet::FIXNUM)
                    || views.iter().any(|view| !actual.is_subset(*view))
                {
                    return None;
                }
                return Some((index, bits));
            }
            Opcode::Refine(_) if inst.args.len() == 1 => {
                let input = func.resolve(inst.args[0])?;
                if !func.values[input.index()].rep.is_tagged() {
                    return None;
                }
                value = input;
            }
            _ => return None,
        }
    }
    None
}
