//! Roots for Rust mode-line accumulators while recursive elements run Lisp.

use super::{ModeLineRendered, Value};
use crate::emacs_core::eval::{
    Context, push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
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

#[inline]
pub(super) fn pin_rendered(eval: &mut Context, rendered: &ModeLineRendered) {
    visit_rendered_roots(rendered, |value| eval.push_specpdl_root(value));
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
    pub(super) fn pin_rendered(&self, rendered: &ModeLineRendered) {
        visit_rendered_roots(rendered, |value| self.pin(value));
    }
}

impl Drop for ScratchRoots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.saved_len);
    }
}
