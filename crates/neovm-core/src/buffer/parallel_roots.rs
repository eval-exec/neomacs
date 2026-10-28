//! Cold stopped-World root projection over storage, independent of an owner's
//! BufferManager selection. Existing serial/Sim root enumeration is unchanged.

use super::*;

impl BufferStorage {
    /// The keeper has stopped every participant and retains the World's Baton.
    /// This visits storage only; it never constructs an owner view or runs Lisp.
    #[cold]
    #[inline(never)]
    pub(crate) fn trace_parallel_roots(&self, visit: &mut dyn FnMut(Value)) {
        for buffer in self.buffers.values() {
            visit(buffer.name);
            visit(buffer.last_name);
            buffer.text.trace_text_prop_roots_with(visit);
            visit(buffer.undo_state.list());
            buffer.overlays.trace_roots_with(visit);
            // BUFFER_OBJFWD slot table holds Lisp values that must
            // be GC-rooted. Mirrors GNU's `mark_buffer` walking the
            // C-side BVAR slots in `alloc.c`.
            for slot in &buffer.slots {
                visit(*slot);
            }
            // Phase 10F: `local_var_alist` is the single source of
            // truth for non-slot per-buffer bindings. The cons
            // cells forming the alist must be rooted (along with
            // every entry's value). A single push of the alist
            // head is sufficient — the GC's reachability walk
            // follows the spine.
            visit(buffer.local_var_alist.as_lisp_alist());
            // `local_map` (buffer's keymap) must also be rooted.
            visit(buffer.keymap);
            // GNU stores the buffer mark in `BVAR (buffer, mark)`, so
            // `mark_vectorlike (&buffer->header)` in `mark_buffer` roots it
            // with the rest of the buffer's Lisp slots.  Neomacs stores the
            // same live marker as a raw pointer in Buffer; synthesize the
            // tagged Value here so GC does not reclaim it while the buffer
            // remains live.
            if let Some(value) = buffer.mark_marker.as_value() {
                visit(value);
            }
            // T8 C-1: the noncurrent PT/BEGV/ZV markers stashed in
            // `state_markers` are referenced only by raw pointers and
            // the intrusive marker chain. Neither is a GC root on its
            // own: the chain is spliced by `unchain_dead_markers` in
            // the mark/sweep gap, so anything not independently marked
            // would be freed here and leave `BufferStateMarkers`
            // holding dangling pointers. Synthesize a Value for each
            // `MarkerObj*` and seed it into the root set so the mark
            // phase walks them. Mirrors GNU `mark_buffer` walking the
            // buffer's `pt_marker` / `begv_marker` / `zv_marker`
            // `Lisp_Object` BVAR slots in `alloc.c`.
            if let Some(sm) = buffer.state_markers {
                for marker in [sm.pt_marker, sm.begv_marker, sm.zv_marker] {
                    if let Some(value) = marker.as_value() {
                        visit(value);
                    }
                }
            }
        }
        for buffer in self.dead_buffers.values() {
            visit(buffer.name);
            visit(buffer.last_name);
            buffer.text.trace_text_prop_roots_with(visit);
            visit(buffer.undo_state.list());
            buffer.overlays.trace_roots_with(visit);
            for slot in &buffer.slots {
                visit(*slot);
            }
            visit(buffer.local_var_alist.as_lisp_alist());
            visit(buffer.keymap);
        }
        // Phase 10D: `buffer_defaults` holds the global default
        // values for every per-buffer slot. Mirrors GNU's
        // `mark_buffer (&buffer_defaults)` in `alloc.c`.
        for slot in &self.buffer_defaults {
            visit(*slot);
        }
        for (buffer_id, restrictions) in &self.labeled_restrictions {
            let buffer = self.buffers.get(buffer_id);
            for restriction in restrictions {
                if let LabeledRestrictionLabel::User(label) = restriction.label {
                    visit(label);
                }
                // The bounds markers are referenced only by u64 id from this
                // map; an unmarked marker is spliced out and freed by
                // unchain_dead_markers, silently widening the restriction.
                // Synthesize Lisp handles so the mark phase walks them
                // (same hazard and fix as state_markers above; GNU keeps
                // these alive via staticpro'd narrowing_locks, editfns.c).
                let _ = buffer;
                if let Some(value) = restriction.beg_marker.as_value() {
                    visit(value);
                }
                if let Some(value) = restriction.end_marker.as_value() {
                    visit(value);
                }
            }
        }
    }

    /// Derive the marker unlink slots for the bounded stopped collection.
    ///
    /// # Safety
    /// The caller holds the World's Baton and AllStopped. No manager loan or
    /// mutator access may overlap use of these slots; discard before resume.
    #[cold]
    #[inline(never)]
    pub(crate) unsafe fn parallel_marker_chain_head_slots(
        &self,
    ) -> Vec<*mut *mut crate::tagged::header::MarkerObj> {
        self.buffers
            .values()
            .map(|buffer| {
                // SAFETY: the caller's stopped-World proof excludes every writer.
                unsafe { buffer.text.markers_head_slot_raw() }
            })
            .collect()
    }
}
