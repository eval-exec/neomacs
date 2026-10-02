//! Major-only reclamation of ordinary-old residual Box objects.
use super::*;

impl TaggedHeap {
    /// All mutators are stopped and the marker is joined. Detaching once
    /// keeps newly promoted Boxes outside the cursor whose marks we reset.
    #[cold]
    #[inline(never)]
    pub(super) fn begin_old_sweep_world_stopped(&mut self) {
        if self.generational.major_in_progress {
            debug_assert!(self.generational.old_sweep_pending.is_null());
            self.generational.old_sweep_pending = self.generational.old_objects;
            self.generational.old_objects = std::ptr::null_mut();
        }
    }

    #[cold]
    #[inline(never)]
    pub(super) fn sweep_old_objects_slice(&mut self, budget: usize) -> usize {
        let mut freed = 0;
        for _ in 0..budget {
            let ptr = self.generational.old_sweep_pending;
            if ptr.is_null() {
                break;
            }
            // No object is freed during marking. The slice owns the heap;
            // no mutator executes or appends a barrier while it runs.
            unsafe {
                let header = &*ptr;
                debug_assert!(header.tenured && !header.generation.permanent());
                self.generational.old_sweep_pending = header.gc_link();
                if header.is_marked_at(self.mark_parity) {
                    self.link_old_object_world_stopped(ptr);
                } else {
                    self.non_cons_object_addrs.remove(&(ptr as usize));
                    self.unregister_vector_object(ptr);
                    self.free_gc_object(ptr);
                    freed += 1;
                }
            }
        }
        self.current_mutator_gc_mut().allocated_count = self
            .current_mutator_gc()
            .allocated_count
            .saturating_sub(freed);
        freed
    }

    /// Exact ordinary-old inventory after all sweep cursors complete. Arena
    /// live-bit walks include old slots and exclude permanent image survivors.
    #[cold]
    #[inline(never)]
    pub(super) fn recompute_old_bytes_world_stopped(&mut self) {
        if !self.generational.enabled {
            return;
        }
        debug_assert!(self.generational.old_sweep_pending.is_null());
        let conses: usize = self.cons_blocks.iter().map(ConsBlock::count_old).sum();
        let mut bytes = conses * size_of::<ConsCell>();
        let mut visit = |ptr: *mut GcHeader| {
            let header = unsafe { &*ptr };
            if header.tenured && !header.generation.permanent() {
                bytes = bytes.saturating_add(Self::object_bytes_from_header(ptr));
            }
        };
        self.float_arena.for_each_allocated_slot(&mut visit);
        self.string_arena.for_each_allocated_slot(&mut visit);
        self.vector_arena.for_each_allocated_slot(&mut visit);
        self.bytecode_arena.for_each_allocated_slot(&mut visit);
        self.lambda_arena.for_each_allocated_slot(&mut visit);
        self.macro_arena.for_each_allocated_slot(&mut visit);
        self.record_arena.for_each_allocated_slot(&mut visit);
        self.symbol_with_pos_arena
            .for_each_allocated_slot(&mut visit);
        self.marker_arena.for_each_allocated_slot(&mut visit);
        self.bignum_arena.for_each_allocated_slot(&mut visit);
        let mut ptr = self.generational.old_objects;
        while !ptr.is_null() {
            visit(ptr);
            ptr = unsafe { (*ptr).gc_link() };
        }
        self.generational.old_cons_count = conses;
        self.generational.old_bytes = bytes;
    }

    /// A late-loaded dump may follow earlier minors/majors. Only surviving
    /// ordinary-old Boxes join the existing first-partition permanent walk.
    #[cold]
    #[inline(never)]
    pub(super) fn join_old_boxes_for_partition_world_stopped(&mut self) {
        if !self.generational.enabled {
            return;
        }
        let mut ptr = std::mem::replace(&mut self.generational.old_objects, std::ptr::null_mut());
        while !ptr.is_null() {
            unsafe {
                let next = (*ptr).gc_link();
                (*ptr).set_gc_link_world_stopped(self.all_objects);
                self.all_objects = ptr;
                ptr = next;
            }
        }
    }
}
