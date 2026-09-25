//! The runtime-projection gate: which variable writes must republish host-side
//! state the `Context` mirrors (quit state, the evaluator depth limit, the
//! keyboard maps, GC settings, redisplay).
//!
//! A child module of `eval`, like its siblings, so it keeps the same view of
//! `Context` and the parent's private items (`use super::*`).

use super::*;

/// Where compiled code reads `Context::attention` (a `u32`), the Context half
/// of `maybe_quit_hot_ok`'s two-word test; it also carries active compiler
/// overrides, which bounce an armed call site to the generic call. Baked
/// only by JIT code (the inline guards refuse AOT), so it is not salted into
/// the AOT ABI tag.
pub(crate) const CONTEXT_ATTENTION_OFFSET: usize = std::mem::offset_of!(Context, attention);
const _: () = assert!(std::mem::size_of::<u32>() == 4);
/// Where compiled code finds the buffer manager, the first hop of the walk to
/// the current buffer's `point` / `BEGV` / `ZV`. See
/// [`crate::buffer::buffer::jit_layout`] for the rest of the chain and for why
/// two of its hops are probed rather than baked.
pub(crate) const CONTEXT_BUFFERS_OFFSET: usize = std::mem::offset_of!(Context, buffers);
/// Where compiled code reads the `Box<TaggedHeap>` pointer, the first hop to
/// the heap state it tests and bumps in place (`tagged::gc::JitHeapState`).
/// The box is assigned only by the constructors, so the pointer is stable
/// for the context's life.
pub(crate) const CONTEXT_TAGGED_HEAP_OFFSET: usize = std::mem::offset_of!(Context, tagged_heap);
const _: () = assert!(
    std::mem::size_of::<Box<crate::tagged::gc::TaggedHeap>>() == std::mem::size_of::<usize>()
);
/// The gate's membership as one bit per symbol id, resolved against the
/// current obarray (after any dump remap): the Context mirrors passed in by
/// `install_core_eval_symbols`, the keyboard maps, the GC settings the
/// threshold formula reads, and every display-affecting variable. Fixed
/// names, so nothing interned later can belong to it.
pub(super) fn runtime_projection_mask_for(core_mirrors: &[SymId]) -> Box<[u64]> {
    let mut ids: Vec<SymId> = core_mirrors.to_vec();
    ids.extend([
        max_lisp_eval_depth_symbol(),
        input_decode_map_symbol(),
        local_function_key_map_symbol(),
    ]);
    ids.extend(GcSettingSyms::resolve().all());
    ids.extend(
        crate::buffer::buffer::DISPLAY_AFFECTING_BUFFER_SLOTS
            .iter()
            .chain(crate::buffer::buffer::DISPLAY_AFFECTING_GLOBAL_VARS)
            .map(|name| intern(name)),
    );
    let words = ids
        .iter()
        .map(|id| id.0 as usize / 64 + 1)
        .max()
        .unwrap_or(0);
    let mut mask = vec![0u64; words];
    for id in &ids {
        mask[id.0 as usize / 64] |= 1 << (id.0 % 64);
    }
    mask.into_boxed_slice()
}

impl Context {
    /// Whether `publish_runtime_binding_write_by_id` would do anything for
    /// `resolved` (an alias-resolved symbol): the union of the four
    /// projections' own tests.  Lets a writer skip computing the value Lisp
    /// sees -- a lexenv scan plus a full variable lookup -- for the vast
    /// majority of symbols, which project to nothing.  GNU has no such
    /// projection layer at all (its C globals ARE the value).
    ///
    /// One bit test: this runs on every write of a plain special
    /// (`try_set_plain_variable`), where the chain of a dozen comparisons it
    /// replaced cost 53 instructions per `setq`.
    #[inline]
    pub(crate) fn runtime_binding_has_projection(&self, resolved: SymId) -> bool {
        let id = resolved.0 as usize;
        self.runtime_projection_mask
            .get(id / 64)
            .is_some_and(|word| word & (1 << (id % 64)) != 0)
    }

    /// The comparison chain the mask replaced, kept to pin their equivalence.
    #[cfg(test)]
    pub(crate) fn runtime_binding_has_projection_by_comparison(&self, resolved: SymId) -> bool {
        resolved == self.quit_flag_symbol
            || resolved == self.inhibit_quit_symbol
            || resolved == self.throw_on_input_symbol
            || resolved == self.compiler_function_overrides_symbol
            || resolved == self.noninteractive_symbol
            || resolved == self.symbols_with_pos_enabled_symbol
            || resolved == self.print_symbols_bare_symbol
            || resolved == max_lisp_eval_depth_symbol()
            || resolved == input_decode_map_symbol()
            || resolved == local_function_key_map_symbol()
            || self.is_gc_runtime_setting_symbol(resolved)
            || crate::buffer::buffer::variable_affects_display_by_sym_id(resolved)
    }
}
