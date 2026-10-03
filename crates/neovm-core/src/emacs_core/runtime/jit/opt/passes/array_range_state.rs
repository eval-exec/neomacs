//! Versioned current-length facts owned by one compilation.
//! Mutable length is never Immutable.
//!
//! Threading: one compiler exclusively owns all maps, scalar generations and
//! provenance IDs. No runtime state, pointer, cache or mutator assumption is
//! introduced. Final native verification independently rebuilds its epochs.

use super::array_reads::LengthFloorWitness;
use crate::emacs_core::jit::opt::{
    ir::*,
    mem::{AliasClass, Effects},
    types::{Range, TypeSet},
};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocalEpoch(u32);

/// A fresh load freezes the floor already established BEFORE that load. A
/// later successful guard cannot retroactively authorize an earlier load.
#[derive(Clone)]
pub(crate) struct FreshLengthFact {
    pub(crate) base: Value,
    pub(crate) value: Value,
    pub(crate) load: Inst,
    pub(crate) range: Range,
    pub(crate) floor: Option<LengthFloorWitness>,
    epoch: LocalEpoch,
}

/// Bounded initial native proof is block-local. Every block and semantic
/// barrier clears both maps. Globally fresh local generations never roll back
/// across sibling blocks. Overflow disables this analysis conservatively.
/// Scalar old length values remain numbers; they cease to certify layout.
pub(crate) struct LengthVersions {
    active: Option<LocalEpoch>,
    minimum: HashMap<Value, LengthFloorWitness>,
    loaded: HashMap<Value, FreshLengthFact>,
}

impl LengthVersions {
    pub(crate) fn new() -> Self {
        Self {
            active: Some(LocalEpoch(0)),
            minimum: HashMap::new(),
            loaded: HashMap::new(),
        }
    }

    pub(crate) fn begin_block(&mut self) {
        self.kill();
    }

    pub(crate) fn kill(&mut self) {
        self.minimum.clear();
        self.loaded.clear();
        self.active = self
            .active
            .and_then(|old| old.0.checked_add(1).map(LocalEpoch));
    }

    /// Caller supplies the numeric/word origin already computed by Range:
    /// aliases and exact checked/refined same-word views collapse, never a phi,
    /// unknown argument, environment value or equal object contents.
    pub(crate) fn loaded(&mut self, func: &Func, id: Inst, base: Value) -> Option<FreshLengthFact> {
        let epoch = self.active?;
        let inst = func.insts.get(id.index())?;
        if inst.op != Opcode::LoadVecLen
            || inst.args.len() != 1
            || inst.eff != Effects::READ_HEAP
            || inst.mem != AliasClass::Unknown
        {
            return None;
        }
        let input = &func.values[func.resolve(inst.args[0])?.index()];
        if input.rep != Rep::Tagged
            || input.ty.is_bottom()
            || !input.ty.is_subset(TypeSet::VECTOR.join(TypeSet::RECORD))
        {
            return None;
        }
        let value = inst.result?;
        let data = &func.values[value.index()];
        // Current array lift deliberately declares the complete possible
        // mutable length. Narrower declarations need another witness kind.
        if data.rep != Rep::RawInt
            || data.ty.is_bottom()
            || !data.ty.is_subset(TypeSet::FIXNUM)
            || data.ty.range()
                != Some(Range {
                    lo: 0,
                    hi: Range::FULL.hi,
                })
        {
            return None;
        }
        let floor = self.minimum.get(&base).cloned();
        let range = Range {
            lo: floor.as_ref().map_or(0, |proof| proof.floor),
            hi: Range::FULL.hi,
        };
        let fact = FreshLengthFact {
            base,
            value,
            load: id,
            range,
            floor,
            epoch,
        };
        self.loaded.insert(value, fact.clone());
        Some(fact)
    }

    pub(crate) fn fact(&self, value: Value) -> Option<&FreshLengthFact> {
        let fact = self.loaded.get(&value)?;
        (Some(fact.epoch) == self.active).then_some(fact)
    }

    /// The supported interval came from array_reads::checked_index_ranges:
    /// actual static/nonprefix literal or an already executed interval guard.
    /// Branch-only facts are never supplied. Inspect CURRENT opcode after
    /// rewrite: an elided Refine must never become a new floor provenance.
    pub(crate) fn successful_bounds(&mut self, func: &Func, id: Inst, index: Range) {
        let Some(inst) = func.insts.get(id.index()) else {
            return;
        };
        if inst.op != Opcode::CheckBounds
            || inst.args.len() != 2
            || inst.eff != Effects::MAY_DEOPT
            || inst.mem != AliasClass::None
            || inst.frame.is_none()
        {
            return;
        }
        let Some(length) = func.resolve(inst.args[1]) else {
            return;
        };
        let Some(loaded) = self.fact(length).cloned() else {
            return;
        };
        let Some(floor) = index.lo.max(0).checked_add(1) else {
            return;
        };
        if floor > Range::FULL.hi
            || self
                .minimum
                .get(&loaded.base)
                .is_some_and(|old| old.floor >= floor)
        {
            return;
        }
        self.minimum.insert(
            loaded.base,
            LengthFloorWitness {
                guard: id,
                length_read: loaded.load,
                index: inst.args[0],
                floor,
            },
        );
    }
}
