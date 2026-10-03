//! Bounded lowering facts from independently verified numeric recipe origins.
//! Threading: this immutable scalar table belongs to one compilation. It holds
//! no Lisp objects, runtime caches, feedback, or mutator/thread-local state.
//! Facts are keyed by exact RecipeVersionId; later cache versions never supply
//! facts for an earlier snapshot. Unknown phi/cache-phi tuples stay generic.

use super::*;
use crate::emacs_core::jit::opt::sink_recipes::{RecipeOrigin, RecipeVersionId, VersionCause};
use numeric_carrier::{CarrierFacts, NumericCache, NumericKind, NumericReady};
use std::collections::VecDeque;

#[derive(Default)]
pub(crate) struct NumericFacts {
    versions: Vec<CarrierFacts>,
}

impl NumericFacts {
    pub(super) fn at(&self, id: RecipeVersionId) -> CarrierFacts {
        self.versions
            .get(id.0 as usize)
            .copied()
            .unwrap_or_default()
    }

    /// The capability has already independently checked every source,
    /// projection, version, and use point. This pass only derives narrower
    /// machine-emission facts; it never changes the IR or extends admission.
    /// Both worklists are monotone: each scalar fact changes at most once and
    /// each dependency is visited a bounded number of times. No recursive
    /// per-snapshot walk or proof from a seedless/unknown phi is performed.
    pub(super) fn build(func: &ir::Func, proof: &VerifiedSinkRecipes) -> Self {
        let table = &func.sink_recipes;
        let mut float = vec![false; func.values.len()];
        let mut users = vec![Vec::new(); func.values.len()];
        let mut queue = VecDeque::new();
        let mut owners: Vec<_> = table.owners.keys().copied().collect();
        owners.sort_unstable();
        for &owner in &owners {
            let info = &table.owners[&owner];
            let seed = match &info.origin {
                RecipeOrigin::Borrow { original, .. } => static_float(func, *original),
                RecipeOrigin::NumericSource { inst, .. } => {
                    if func.insts[inst.index()].op
                        == ir::Opcode::Sink(sink_recipes::SinkOp::SourceSqrt)
                    {
                        true
                    } else {
                        for &input in &func.insts[inst.index()].args {
                            if table.owners.contains_key(&input) {
                                users[input.index()].push(owner);
                            }
                        }
                        false
                    }
                }
                RecipeOrigin::SameIdentityView { input, .. } => {
                    if table.owners.contains_key(input) {
                        users[input.index()].push(owner);
                    }
                    false
                }
                // In particular, a Float declaration or Float feedback on a
                // phi is not a grounded numeric-kind proof.
                RecipeOrigin::Phi { .. } | RecipeOrigin::ConsSource { .. } => false,
            };
            if seed {
                float[owner.index()] = true;
                queue.push_back(owner);
            }
        }
        while let Some(input) = queue.pop_front() {
            for &owner in &users[input.index()] {
                if !float[owner.index()] {
                    // A successful Add/Sub/Mul/Div with one actual Float
                    // operand is Float by the unchanged GNU/shared dispatcher.
                    // Other operands still execute their original guards.
                    float[owner.index()] = true;
                    queue.push_back(owner);
                }
            }
        }

        let mut versions = vec![CarrierFacts::default(); table.versions.len()];
        let mut version_users = vec![Vec::new(); versions.len()];
        let mut queue = VecDeque::new();
        for (index, version) in table.versions.iter().enumerate() {
            if !matches!(version.fields, RecipeFields::Number(_)) {
                continue;
            }
            let id = RecipeVersionId(index as u32);
            let mut facts = CarrierFacts::default();
            if proof.guaranteed_boxed(id) {
                facts.cache = NumericCache::Boxed;
            }
            match &version.cause {
                VersionCause::Definition => {
                    match table.owners.get(&version.owner).map(|owner| &owner.origin) {
                        Some(RecipeOrigin::Borrow { .. }) => facts.ready = NumericReady::Borrowed,
                        Some(RecipeOrigin::NumericSource { .. }) => {
                            facts.ready = NumericReady::Ready;
                            facts.cache = NumericCache::Fresh;
                        }
                        _ => {}
                    }
                    if float[version.owner.index()] {
                        facts.kind = NumericKind::Float;
                    }
                }
                VersionCause::SameIdentity { input } => {
                    version_users[input.0 as usize].push(index);
                }
                VersionCause::CacheAfter { previous, .. }
                | VersionCause::AliasCacheAfter { previous, .. } => {
                    // AliasCacheAfter adopts only the box. Its readiness and
                    // payload kind belong to PREVIOUS, not the input cache.
                    version_users[previous.0 as usize].push(index);
                }
                VersionCause::Parameter | VersionCause::CachePhi { .. } => {}
            }
            versions[index] = facts;
            queue.push_back(index);
        }
        while let Some(input_index) = queue.pop_front() {
            for &index in &version_users[input_index] {
                let previous = versions[index];
                let input = versions[input_index];
                let mut facts = previous;
                facts.ready = input.ready;
                facts.kind = input.kind;
                if matches!(
                    table.versions[index].cause,
                    VersionCause::SameIdentity { .. }
                ) && facts.cache != NumericCache::Boxed
                {
                    facts.cache = input.cache;
                }
                if facts != previous {
                    versions[index] = facts;
                    queue.push_back(index);
                }
            }
        }
        Self { versions }
    }
}

/// Actual immutable direct constant provenance only. This checks the Value
/// tag without dereferencing its payload. EnvConst/patched prefixes, declared
/// TypeSet/feedback and even pure Refine views are not constant witnesses.
fn static_float(func: &ir::Func, original: ir::Value) -> bool {
    let Some(original) = func.resolve(original) else {
        return false;
    };
    let ir::ValueDef::Inst(inst) = func.values[original.index()].def else {
        return false;
    };
    let ir::Opcode::Const(index) = func.insts[inst.index()].op else {
        return false;
    };
    index as usize >= func.dynamic_prefix
        && func
            .consts
            .get(index as usize)
            .is_some_and(|bits| Value::from_bits(bits.0 as usize).is_float())
}
