//! Immediacy of the value each optimized store writes, for the GEN1
//! remembered-set barrier. A fixnum is an immediate: storing one never makes
//! an old-to-young edge, so the barrier has nothing to filter. A value proven
//! never a fixnum makes the barrier's dynamic fixnum-tag test dead.
//! Threading: this immutable table belongs to one compilation. It holds only
//! compiler ids and type sets; no Lisp object, runtime cache or mutator state.

use super::*;
use crate::emacs_core::jit::opt::passes::cfg::Dominance;
use crate::emacs_core::jit::opt::types::TypeKind;
use heap_inline::StoredValueClass;

/// Per-instruction classes of Setcar/Setcdr/Aset values; empty unless built.
#[derive(Default)]
pub(crate) struct StoredValueFacts {
    classes: Vec<Option<StoredValueClass>>,
}

impl StoredValueFacts {
    /// Facts come from three sources, all holding where the store executes:
    /// the value's own type, the types along its Refine/CheckType input chain
    /// (the same runtime word, defined before it), and every Refine/CheckType
    /// of that word that dominates the store. A CheckType deopts on failure,
    /// so code it dominates sees only its result type; a Refine is a branch
    /// fact on the edge that its block alone receives. An unreachable value
    /// or a graph without dominance yields no fact, never a guess.
    #[doc(hidden)]
    pub fn build(func: &ir::Func) -> Self {
        let Ok(dominance) = Dominance::new(func) else {
            return Self::default();
        };
        let mut refinements: HashMap<ir::Value, Vec<(ir::Block, usize, TypeSet)>> = HashMap::new();
        for (index, data) in func.blocks.iter().enumerate() {
            for (position, &id) in data.insts.iter().enumerate() {
                let inst = &func.insts[id.index()];
                if let (Some(input), Some(result)) = (word_input(inst), inst.result) {
                    refinements
                        .entry(word_root(func, input))
                        .or_default()
                        .push((
                            ir::Block(index as u32),
                            position,
                            func.values[result.index()].ty,
                        ));
                }
            }
        }
        let mut classes = vec![None; func.insts.len()];
        for (index, data) in func.blocks.iter().enumerate() {
            let block = ir::Block(index as u32);
            for (position, &id) in data.insts.iter().enumerate() {
                let inst = &func.insts[id.index()];
                let Some(stored) = stored_value(inst) else {
                    continue;
                };
                let mut ty = TypeSet::TOP;
                let mut word = func.resolve(stored);
                while let Some(value) = word {
                    ty = ty.meet(func.values[value.index()].ty);
                    word = definition(func, value).and_then(word_input);
                    word = word.and_then(|input| func.resolve(input));
                }
                let root = word_root(func, stored);
                for &(at, before, fact) in refinements.get(&root).into_iter().flatten() {
                    let dominates = if at == block {
                        before < position
                    } else {
                        dominance.dominates(at, block)
                    };
                    if dominates {
                        ty = ty.meet(fact);
                    }
                }
                classes[id.index()] = if ty.is_bottom() {
                    None
                } else if ty.is_subset(TypeSet::FIXNUM) {
                    Some(StoredValueClass::Fixnum)
                } else if !ty.contains(TypeKind::Fixnum) {
                    Some(StoredValueClass::NonFixnum)
                } else {
                    None
                };
            }
        }
        Self { classes }
    }

    #[doc(hidden)]
    pub fn class(&self, inst: ir::Inst) -> Option<StoredValueClass> {
        self.classes.get(inst.index()).copied().flatten()
    }
}

/// The value operand of an explicit heap store whose barrier filters values.
fn stored_value(inst: &ir::InstData) -> Option<ir::Value> {
    match inst.op {
        ir::Opcode::Opaque(Op::Setcar | Op::Setcdr) => inst.args.get(1).copied(),
        ir::Opcode::Opaque(Op::Aset) => inst.args.get(2).copied(),
        _ => None,
    }
}

/// Refine and CheckType narrow a type without changing the runtime word.
fn word_input(inst: &ir::InstData) -> Option<ir::Value> {
    match (&inst.op, &inst.args[..]) {
        (ir::Opcode::Refine(_) | ir::Opcode::CheckType(_), &[input]) => Some(input),
        _ => None,
    }
}

fn definition(func: &ir::Func, value: ir::Value) -> Option<&ir::InstData> {
    match func.values[value.index()].def {
        ir::ValueDef::Inst(inst) => Some(&func.insts[inst.index()]),
        ir::ValueDef::Param { .. } | ir::ValueDef::Alias(_) => None,
    }
}

/// The value that first produced this runtime word.
fn word_root(func: &ir::Func, value: ir::Value) -> ir::Value {
    let mut root = func.resolve(value).unwrap_or(value);
    while let Some(input) = definition(func, root).and_then(word_input) {
        root = func.resolve(input).unwrap_or(input);
    }
    root
}
