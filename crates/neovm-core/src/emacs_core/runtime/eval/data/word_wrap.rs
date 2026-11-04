//! Projection of the buffer's category table into GNU's wrap policy.
//!
//! The policy type and its predicates live in the edit layer; projecting one
//! binds the text layer's category machinery, so this projection lives below
//! the evaluator and edit reaches it through `EditHost::word_wrap_policy`.
//! The per-buffer cache keeps the projection (and its collection
//! dependencies) for the current category-table epoch.

use neovm_edit::buffer::Buffer;
use neovm_edit::emacs_core::word_wrap::{CachedWordWrapPolicy, WordWrapMode, WordWrapPolicy};
use neovm_text::emacs_core::category::{
    active_category_table_for_buffer, category_bit_runs_in_table,
};
use neovm_value::tagged::{collection_reads, gc};

/// Freeze only native category data. No category-table or bool-vector Lisp
/// object may be retained by an independent row worker.
pub fn word_wrap_policy_for_buffer(buffer: &Buffer, mode: WordWrapMode) -> WordWrapPolicy {
    if mode == WordWrapMode::Whitespace {
        return WordWrapPolicy::whitespace();
    }
    let Ok(table) = active_category_table_for_buffer(Some(buffer)) else {
        return WordWrapPolicy::category_from_runs(std::iter::empty());
    };
    let table_bits = table.bits();
    let reclamation = gc::collection_observation_epoch();
    if let Some(cached) = buffer.cached_word_wrap_policy()
        && cached.matches(table_bits, reclamation)
        && cached.dependencies_unchanged_and_observe()
    {
        return cached.policy();
    }
    let (runs, dependencies) =
        collection_reads::capture(|| category_bit_runs_in_table(table).unwrap_or_default());
    let policy = WordWrapPolicy::category_from_runs(runs);
    buffer.store_word_wrap_policy_cache(dependencies.map(|dependencies| {
        std::rc::Rc::new(CachedWordWrapPolicy::new(
            table_bits,
            reclamation,
            dependencies,
            policy.clone(),
        ))
    }));
    policy
}
