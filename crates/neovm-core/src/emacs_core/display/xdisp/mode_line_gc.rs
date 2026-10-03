//! Roots for Rust mode-line accumulators while recursive elements run Lisp.

use super::{ModeLineRendered, Value};
use crate::emacs_core::eval::{
    push_scratch_gc_root, push_scratch_gc_root_slot, restore_scratch_gc_roots,
    save_scratch_gc_roots, set_scratch_gc_root,
};

#[inline]
fn visit_rendered_roots(rendered: &ModeLineRendered, mut visit: impl FnMut(Value)) {
    // These are freshly copied plist spines; rooting a source string does not
    // retain the destination's interval conses.
    rendered.text_props.for_each_root(&mut visit);
    for span in &rendered.source_spans {
        visit(span.source());
    }
    for transition in &rendered.min_width_transitions {
        visit(transition.run.width_spec);
        for (&name, &value) in &transition.run.padding_properties {
            visit(name);
            visit(value);
        }
    }
}

/// A short child usually retains only its source and copied interval plist.
/// Keep those identities inline rather than allocating a hash table per child.
/// Larger accumulators still deduplicate in constant time using tagged bits.
#[derive(Clone)]
pub(super) enum AccumulatorRoots {
    Inline { bits: [usize; 2], len: usize },
    Hashed(rustc_hash::FxHashSet<usize>),
}

impl Default for AccumulatorRoots {
    fn default() -> Self {
        Self::Inline {
            bits: [0; 2],
            len: 0,
        }
    }
}

impl AccumulatorRoots {
    #[inline]
    fn insert(&mut self, value: usize) -> bool {
        match self {
            Self::Inline { bits, len } => {
                if bits[..*len].contains(&value) {
                    return false;
                }
                if *len < bits.len() {
                    bits[*len] = value;
                    *len += 1;
                } else {
                    // A parent quickly collects several child plist heads.
                    // Avoid repeatedly growing the smallest hash tables.
                    let mut roots =
                        rustc_hash::FxHashSet::with_capacity_and_hasher(16, Default::default());
                    roots.extend(*bits);
                    roots.insert(value);
                    *self = Self::Hashed(roots);
                }
                true
            }
            Self::Hashed(roots) => roots.insert(value),
        }
    }
}

#[inline]
pub(super) fn pin_accumulator_value(roots: &mut AccumulatorRoots, value: Value) {
    if !value.is_nil() && roots.insert(value.bits()) {
        push_scratch_gc_root(value);
    }
}

#[inline]
pub(super) fn pin_rendered_scratch(rendered: &ModeLineRendered) -> AccumulatorRoots {
    let mut roots = AccumulatorRoots::default();
    visit_rendered_roots(rendered, |value| pin_accumulator_value(&mut roots, value));
    roots
}

/// Root scope for the split-state compatibility callback, which has no
/// Context reference. The caller must keep the same tagged heap active while
/// the callback runs: the existing scratch registry records that heap's
/// identity and the collector filters these roots by it.
pub(super) struct ScratchRoots {
    saved_len: usize,
}

impl ScratchRoots {
    #[inline]
    pub(super) fn new() -> Self {
        Self {
            saved_len: save_scratch_gc_roots(),
        }
    }

    #[inline]
    pub(super) fn pin(&self, value: Value) {
        push_scratch_gc_root(value);
    }

    #[inline]
    pub(super) fn slot(&self, value: Value) -> usize {
        push_scratch_gc_root_slot(value)
    }

    #[inline]
    pub(super) fn set(&self, slot: usize, value: Value) {
        set_scratch_gc_root(slot, value);
    }
}

impl Drop for ScratchRoots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.saved_len);
    }
}
