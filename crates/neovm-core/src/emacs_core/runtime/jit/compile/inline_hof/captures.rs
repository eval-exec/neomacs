//! Bounded captured-cell witnesses for list callbacks.
//! Threading: the plan is immutable compile-local Rust data. Its variables
//! name activation-local SSA values, never a TLS or cross-mutator Lisp cache.

use super::*;

/// One straight-line capture increment, with no incoming edge after its
/// constant load. The dup preserves this exact cell under the car/add1 result.
#[derive(Clone, Copy)]
struct Increment {
    constant_pc: usize,
    car_pc: usize,
    store_pc: usize,
    slot: usize,
}

/// Identities and cons tags valid between the activation's service polls.
/// The rooted executing callback owns immutable published constant storage;
/// native admitted ops cannot replace it or run a safe point, and concurrent
/// GC only traces it. Captured cons CONTENTS are mutable and never cached.
pub(crate) struct Captures {
    increments: Vec<Increment>,
    values: std::collections::BTreeMap<usize, Variable>,
}

fn increments(ops: &[Op], prefix: usize, leaders: &[usize]) -> Vec<Increment> {
    ops.windows(5)
        .enumerate()
        .filter_map(|(pc, window)| {
            let Op::Constant(slot) = &window[0] else {
                return None;
            };
            let slot = usize::from(*slot);
            (slot < prefix
                && matches!(window[1], Op::Dup)
                && matches!(window[2], Op::Car | Op::CarSafe)
                && matches!(window[3], Op::Add1)
                && matches!(window[4], Op::Setcar)
                && !(pc + 1..=pc + 4).any(|at| leaders.binary_search(&at).is_ok()))
            .then_some(Increment {
                constant_pc: pc,
                car_pc: pc + 2,
                store_pc: pc + 4,
                slot,
            })
        })
        .collect()
}

impl Captures {
    pub(crate) fn build(fb: &mut FunctionBuilder, site: HofSite) -> Result<Self, CompileError> {
        let mut result = Self {
            increments: Vec::new(),
            values: std::collections::BTreeMap::new(),
        };
        // Forced-deopt keeps the original per-op checks and precise snapshots.
        if site.prefix == 0 || super::super::jit_force_deopt() {
            return Ok(result);
        }
        let code = site
            .callback
            .get_bytecode_data()
            .ok_or(CompileError::BadOperand)?;
        let constants = mask_dynamic_prefix(&code.constants, site.prefix);
        let cfg = analyze_cfg(
            code.executable_ops(),
            &constants,
            code.executable_gnu_byte_offset_map(),
            1,
        )?;
        result.increments = increments(code.executable_ops(), site.prefix, &cfg.leaders);
        for site in &result.increments {
            result
                .values
                .entry(site.slot)
                .or_insert_with(|| fb.declare_var(types::I64));
        }
        Ok(result)
    }

    /// Run after callback entry attention/depth guards, with its original
    /// argument and callback_entered=false/pc0 chain already installed. A
    /// non-cons declines to the original callback even when its increment
    /// branch is inactive; it never raises an eager type error.
    pub(crate) fn enter(
        &self,
        frames: &Frames,
        fb: &mut FunctionBuilder,
        base: ClifValue,
        stack: &[ClifValue],
        reps: &[SlotRep],
        pending: &mut Vec<PendingDeopt>,
    ) -> Result<(), CompileError> {
        for (&slot, &variable) in &self.values {
            let offset = i32::try_from(slot * 8).map_err(|_| CompileError::BadOperand)?;
            let cell = fb
                .ins()
                .load(types::I64, MemFlagsData::trusted(), base, offset);
            let tag = lowering::band_imm_p(fb, cell, TAG_MASK as i64);
            let cons =
                lowering::icmp_imm_p(fb, IntCC::Equal, tag, crate::tagged::value::TAG_CONS as i64);
            let deopt = deopt_site(fb, 0, 0, stack, reps, pending);
            frames.mark(
                pending,
                crate::emacs_core::jit::reopt::DeoptCause::InlinedCall,
            );
            emit_guard(fb, deopt, cons);
            fb.def_var(variable, cell);
        }
        Ok(())
    }

    pub(crate) fn constant(&self, pc: usize) -> Option<Variable> {
        self.increments
            .iter()
            .find(|site| site.constant_pc == pc)
            .map(|site| self.values[&site.slot])
    }

    pub(crate) fn car(&self, pc: usize) -> bool {
        self.increments.iter().any(|site| site.car_pc == pc)
    }

    pub(crate) fn store(&self, pc: usize) -> bool {
        self.increments.iter().any(|site| site.store_pc == pc)
    }
}

#[cfg(test)]
#[path = "../../tests/inline_hof_captures_test.rs"]
mod tests;
