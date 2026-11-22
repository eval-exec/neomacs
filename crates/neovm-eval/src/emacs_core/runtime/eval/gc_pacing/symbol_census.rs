//! GNU's last-sweep symbol count, independent of the process interner span.
//!
//! GNU's fixed `lispsym` array comes from DEFSYM declarations; native function
//! and variable initialization interns ordinary symbols (make-docfile.c
//! 744-790, 880-895; lread.c 5225-5294). Neo's fixed equivalent is its constructor
//! seed prefix. Reconstruction/activation interns belong to the image baseline.

use super::super::*;
use neovm_value::tagged::gc::MemoryUseCountSlot;

const SEED_SYMBOL_COUNT: usize =
    neovm_value::intern::FIRST_WELL_KNOWN_SYM_ID as usize + neovm_value::intern::WellKnown::COUNT;

/// Accessor-owned state in the boxed World, without moving Context/JIT fields.
#[derive(Debug)]
pub(crate) struct SymbolGcCensus {
    activation_births: Option<u64>,
    last_gc_symbols: usize,
}

impl Default for SymbolGcCensus {
    fn default() -> Self {
        Self {
            activation_births: None,
            last_gc_symbols: SEED_SYMBOL_COUNT,
        }
    }
}

fn symbol_births() -> u64 {
    // The existing World-carried snapshot combines heap and pending native
    // counters. Neo's registry retains every allocated identity, including
    // uninterned symbols, so these runtime births remain live in Neo.
    Value::memory_use_counts_snapshot()[MemoryUseCountSlot::Symbols as usize] as u64
}

impl Context {
    /// Exclude image reconstruction and all native activation, including the
    /// late subr/fringe registrations after the fresh inner constructor.
    pub(crate) fn capture_gc_symbol_census_after_activation(&mut self) {
        let births = symbol_births();
        let census = &mut self.global_state_mut().gc_symbol_census;
        census.activation_births = Some(births);
        census.last_gc_symbols = SEED_SYMBOL_COUNT;
        // An activated World can return a stats list before its first new
        // sweep. Seed a snapshot with the same image-exclusion definition.
        self.tagged_heap.request_gc_object_counts();
        self.tagged_heap
            .publish_gc_object_counts_if_requested(SEED_SYMBOL_COUNT);
    }

    /// Publish after sweep and before finalizers/post-gc-hook. Their symbol
    /// allocations belong to the next collection's gcstat, as in GNU.
    pub(crate) fn refresh_gc_symbol_census_after_collection(&mut self) {
        let births = symbol_births();
        let census = &mut self.global_state_mut().gc_symbol_census;
        let Some(activation) = census.activation_births else {
            return;
        };
        let runtime = usize::try_from(births.saturating_sub(activation)).unwrap_or(usize::MAX);
        census.last_gc_symbols = SEED_SYMBOL_COUNT.saturating_add(runtime);
    }

    #[inline]
    pub(crate) fn gc_symbols_used(&self) -> usize {
        self.global_state().gc_symbol_census.last_gc_symbols
    }
}

#[cfg(test)]
#[path = "tests/symbol_census_test.rs"]
mod tests;
