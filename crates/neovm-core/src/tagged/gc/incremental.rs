//! Incremental marking and sweeping: gray-queue draining, the termination re-seed of runtime and remembered roots, sweep slices, and the mark entry points (push_gray, mark_symbol).
//!
//! Moved out of `gc.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

impl TaggedHeap {
    /// True while a mark is underway (between the start handshake and sweep).
    pub fn mark_in_progress(&self) -> bool {
        self.mark_in_progress
    }

    /// Re-seed the collector-internal roots at mark termination: the runtime
    /// object registries and the dump remembered set (the non-clearing seeds
    /// that `begin_collection` runs at the start). Mark termination must
    /// re-snapshot the COMPLETE root set, not just the evaluator/context roots —
    /// otherwise an object that became reachable only through one of these roots
    /// during the marking window is left unmarked and swept while live.
    pub(crate) fn reseed_runtime_and_remembered_roots(&mut self) {
        // Zero the remembered scratch so the skip branch below does not leave
        // a stale previous value in the termination slots filled after.
        self.last_remembered_seed_us = 0;
        self.last_remembered_seed_roots = 0;
        self.seed_internal_runtime_roots();
        if self.partition_dump && self.dump_blackened {
            self.seed_mapped_remembered();
        }
        // Route this handshake's reseed costs to the TERMINATION slots (this
        // entry point is exclusively the concurrent termination).
        self.handshake.term_count += 1;
        self.handshake.last_term_runtime_us = self.last_runtime_seed_us;
        self.handshake.last_term_runtime_roots = self.last_runtime_seed_roots;
        self.handshake.last_term_remembered_us = self.last_remembered_seed_us;
        self.handshake.last_term_remembered_roots = self.last_remembered_seed_roots;
    }

    /// Drain ALL remaining marking work to a fixpoint (no budget). Used at mark
    /// termination, after the roots have been re-snapshotted, while the world is
    /// stopped. A single `mark_all` reaches the fixpoint: `mark_value` re-pushes
    /// each marked object's children, so the gray queue drains completely.
    pub(crate) fn incremental_drain_all(&mut self) {
        let t0 = std::time::Instant::now();
        self.mark_all();
        self.incremental_mark_us += t0.elapsed().as_micros() as u64;
    }

    /// Run mark termination's sweep + accounting, then leave the incremental
    /// state. Marking must already be drained to a fixpoint and the marker
    /// chain heads installed. `pause_t0` stamps the termination (sweep) pause.
    /// Mark termination: verify, unchain dead markers, then DEFER the sweep.
    /// The reclaim drains in bounded slices at later safe points
    /// (`incremental_sweep_slice`), so it is no longer part of the STW pause.
    /// Marking is complete here; the barrier is dropped.
    pub(crate) fn incremental_finish(
        &mut self,
        bytes_before: usize,
        _pause_t0: std::time::Instant,
    ) {
        // Queue doomed finalizers first (mirrors `complete_collection`; a miss
        // here would mean finalizers silently never run under the concurrent
        // collector). The main mark has drained — the termination handshake
        // already traced the deferred veclikes — so marks are final.
        let finalizer_t0 = std::time::Instant::now();
        self.mark_and_queue_doomed_finalizers();
        self.handshake.last_term_finalizer_us = finalizer_t0.elapsed().as_micros() as u64;
        // Resolve weak hash tables (GNU mark_and_sweep_weak_table_contents): mark
        // entries that survive per their table's weakness, then drop the rest. This
        // mirrors `complete_collection` and MUST run on the concurrent/incremental
        // termination too — otherwise a weak table's only-weakly-reachable entries
        // are neither marked nor removed, so they are swept while still referenced
        // by the table (UAF). The main mark has already drained at this point.
        let weak_t0 = std::time::Instant::now();
        self.mark_and_sweep_weak_tables();
        self.handshake.last_term_weak_us = weak_t0.elapsed().as_micros() as u64;

        // Dump-partition safety gate (marks still intact). Same as
        // `finalize_collection`'s, run before any object is freed.
        if self.partition_dump
            && self.dump_blackened
            && std::env::var("NEOVM_GC_VERIFY_PARTITION").as_deref() == Ok("1")
        {
            self.verify_dump_partition();
            self.verify_incremental_tricolor();
        }
        // Unchain dead markers before the sweep frees them (mirrors GNU
        // sweep_buffer -> unchain_dead_markers). Reads marks, which are intact.
        let unchain_t0 = std::time::Instant::now();
        self.unchain_dead_markers();
        self.handshake.last_term_unchain_us = unchain_t0.elapsed().as_micros() as u64;

        // Begin the deferred sweep. Detach the young non-cons list (new non-cons
        // allocations link onto a fresh `all_objects` and are not swept this
        // cycle) and reset the cons free list (rebuilt as blocks are swept).
        self.sweep_noncons_pending = self.all_objects;
        self.all_objects = std::ptr::null_mut();
        self.cons_free_list = std::ptr::null_mut();
        self.sweep_cons_cursor = 0;
        // Object arena pages are swept in place behind these cursors (no
        // detached list exists for them; the bitmap is re-read per slice).
        self.sweep_float_page_cursor = 0;
        self.sweep_string_page_cursor = 0;
        self.sweep_vector_page_cursor = 0;
        self.sweep_bytecode_page_cursor = 0;
        self.sweep_lambda_page_cursor = 0;
        self.sweep_macro_page_cursor = 0;
        self.sweep_record_page_cursor = 0;
        self.sweep_symbol_with_pos_page_cursor = 0;
        self.sweep_marker_page_cursor = 0;
        self.sweep_noncons_live_bytes = 0;
        self.sweep_mark_us = self.incremental_mark_us;
        self.sweep_bytes_before = bytes_before;
        self.sweep_slice_us_total = 0;
        self.sweep_slice_count = 0;
        self.sweep_cons_blocks_swept = 0;
        self.sweep_noncons_freed = 0;
        self.sweep_in_progress = true;
        // Pacer: close the mark window. Sample the allocation rate + wall
        // duration of the just-terminated concurrent mark and project the
        // next window's allocation (`pace_lead_bytes`).
        let pace_wall_us = self
            .pace_mark_start
            .take()
            .map(|t0| t0.elapsed().as_micros() as u64)
            .unwrap_or(0);
        let pace_alloc = self
            .bytes_since_gc
            .saturating_sub(self.pace_mark_start_bytes);
        let forced = self.forced_termination_pending;
        self.forced_termination_pending = false;
        self.pace_close_mark_window(pace_wall_us, pace_alloc, forced);
        if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            eprintln!(
                "NEOVM_GC mark_window alloc={}B wall={}us start_bytes={} forced={} \
                 rate_ewma={}B/s dur_ewma={}us lead={}B",
                pace_alloc,
                pace_wall_us,
                self.pace_mark_start_bytes,
                forced,
                self.pace_alloc_rate_bps,
                self.pace_mark_dur_us,
                self.pace_lead_bytes,
            );
        }
        // The triggering allocation budget is spent; the next mark fires once a
        // fresh threshold's worth has been allocated.
        self.bytes_since_gc = 0;

        // Marking is done; drop the marking barrier. The dump remembered set is
        // still maintained unconditionally in `record_heap_write`.
        self.set_write_tracking_mode(WriteTrackingMode::Disabled);
        self.mark_in_progress = false;
    }

    /// True while the deferred sweep is draining.
    pub fn sweep_in_progress(&self) -> bool {
        self.sweep_in_progress
    }

    /// True if a panic ever unwound while one of the collector's own locks was
    /// held. Those critical sections live entirely inside GC machinery, so
    /// poison proves a panic escaped mid-protocol and the heap's invariants
    /// are unknown. Module-boundary panic containment probes this and refuses
    /// to contain (re-raises) when it fires; the locks keep plain `.unwrap()`
    /// at their use sites on purpose — clearing poison would assert a
    /// coherence nothing can verify and erase the only evidence.
    pub(crate) fn gc_locks_poisoned(&self) -> bool {
        self.satb_shared.is_poisoned()
            || self.deferred_veclikes.is_poisoned()
            || self.gc_wake.0.is_poisoned()
    }

    /// Test-only: poison one of the collector's own locks by panicking while
    /// holding it, so containment tests can exercise the refuse-to-contain
    /// probe without unwinding real GC machinery. Poison is permanent for the
    /// heap (that is the point) — callers run process-per-test under nextest.
    #[cfg(test)]
    pub(crate) fn poison_gc_locks_for_test(&self) {
        let lock = self.satb_shared.clone();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _guard = lock.lock().unwrap();
            panic!("poison a GC lock for the containment probe test");
        }));
        assert!(self.gc_locks_poisoned(), "test poison must be observable");
    }

    /// Advance the deferred sweep by one bounded slice: reclaim up to `budget`
    /// cons blocks and up to `budget` pending non-cons objects. Returns true
    /// (and finalizes accounting) once the whole sweep is done. New conses
    /// allocated meanwhile are born black (see `alloc_cons`), so an unswept
    /// block never reclaims a live new cell.
    pub(crate) fn incremental_sweep_slice(&mut self, budget: usize) -> bool {
        let t0 = std::time::Instant::now();
        // -- cons: reclaim up to `budget` blocks (each ~64KB of cells) --
        let mut swept_blocks = 0usize;
        while swept_blocks < budget && self.sweep_cons_cursor < self.cons_blocks.len() {
            let idx = self.sweep_cons_cursor;
            let free_list: *mut *mut ConsCell = &mut self.cons_free_list;
            self.cons_blocks[idx].sweep(unsafe { &mut *free_list });
            self.sweep_cons_cursor += 1;
            swept_blocks += 1;
        }
        // -- object arena pages: reclaim up to `budget` pages PER CLASS
        //    (64KB each, like cons blocks), page-at-a-time behind the
        //    per-class cursors. Each visit re-reads the live bitmap (the
        //    mutator can reallocate freed slots between slices — see
        //    `ObjectArena::sweep_range`). Pages created mid-sweep may or may
        //    not be visited by the moving cursor/len race; either is correct
        //    — every slot in them is born-at-parity (marked), so a visit
        //    counts survivors and a skip frees nothing it shouldn't. Page
        //    survivor bytes accumulate into `sweep_noncons_live_bytes`, the
        //    incremental half of the live-bytes recompute
        //    (`finish_incremental_sweep`). --
        let mut float_freed = 0usize;
        {
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_float_page_cursor < self.float_arena.pages.len()
            {
                let idx = self.sweep_float_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (idx, idx + 1),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_float_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_string_page_cursor < self.string_arena.pages.len()
            {
                let idx = self.sweep_string_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (idx, idx + 1),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_string_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_vector_page_cursor < self.vector_arena.pages.len()
            {
                let idx = self.sweep_vector_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (0, 0),
                    (idx, idx + 1),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_vector_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_bytecode_page_cursor < self.bytecode_arena.pages.len()
            {
                let idx = self.sweep_bytecode_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (idx, idx + 1),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_bytecode_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_lambda_page_cursor < self.lambda_arena.pages.len()
            {
                let idx = self.sweep_lambda_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (idx, idx + 1),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_lambda_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_macro_page_cursor < self.macro_arena.pages.len()
            {
                let idx = self.sweep_macro_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (idx, idx + 1),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_macro_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_record_page_cursor < self.record_arena.pages.len()
            {
                let idx = self.sweep_record_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (idx, idx + 1),
                    (0, 0),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_record_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_symbol_with_pos_page_cursor < self.symbol_with_pos_arena.pages.len()
            {
                let idx = self.sweep_symbol_with_pos_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (idx, idx + 1),
                    (0, 0),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_symbol_with_pos_page_cursor += 1;
                swept_pages += 1;
            }
            let mut swept_pages = 0usize;
            while swept_pages < budget
                && self.sweep_marker_page_cursor < self.marker_arena.pages.len()
            {
                let idx = self.sweep_marker_page_cursor;
                let (live, freed) = self.sweep_arena_pages_ranges(
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (idx, idx + 1),
                );
                self.sweep_noncons_live_bytes = self.sweep_noncons_live_bytes.saturating_add(live);
                float_freed += freed;
                self.sweep_marker_page_cursor += 1;
                swept_pages += 1;
            }
        }
        // -- non-cons: reclaim more objects per slice than cons blocks, since a
        //    cons block holds thousands of cells while a non-cons node is one
        //    object (with a heavier per-object free). --
        let noncons_budget = budget.saturating_mul(256);
        let mut processed = 0usize;
        let mut noncons_freed = 0usize;
        // The detached sweep list is young-only (`all_objects` never holds
        // tenured objects), and `begin_collection` hard-asserts no flip can
        // happen while this sweep drains, so the bits are interpreted at the
        // parity of the cycle that just marked them.
        let parity = self.mark_parity;
        while processed < noncons_budget && !self.sweep_noncons_pending.is_null() {
            let current = self.sweep_noncons_pending;
            unsafe {
                self.sweep_noncons_pending = (*current).next;
                debug_assert!(
                    !(*current).tenured,
                    "tenured object on the young sweep list"
                );
                if (*current).is_marked_at(parity) {
                    // Survivor: relink onto the (fresh) young list.
                    (*current).next = self.all_objects;
                    self.all_objects = current;
                    self.sweep_noncons_live_bytes = self
                        .sweep_noncons_live_bytes
                        .saturating_add(Self::object_bytes_from_header(current));
                } else {
                    self.non_cons_object_addrs.remove(&(current as usize));
                    self.unregister_vector_object(current);
                    self.free_gc_object(current);
                    self.allocated_count = self.allocated_count.saturating_sub(1);
                    noncons_freed += 1;
                }
            }
            processed += 1;
        }

        let done = self.sweep_cons_cursor >= self.cons_blocks.len()
            && self.sweep_noncons_pending.is_null()
            && self.sweep_float_page_cursor >= self.float_arena.pages.len()
            && self.sweep_string_page_cursor >= self.string_arena.pages.len()
            && self.sweep_vector_page_cursor >= self.vector_arena.pages.len()
            && self.sweep_bytecode_page_cursor >= self.bytecode_arena.pages.len()
            && self.sweep_lambda_page_cursor >= self.lambda_arena.pages.len()
            && self.sweep_macro_page_cursor >= self.macro_arena.pages.len()
            && self.sweep_record_page_cursor >= self.record_arena.pages.len()
            && self.sweep_symbol_with_pos_page_cursor >= self.symbol_with_pos_arena.pages.len()
            && self.sweep_marker_page_cursor >= self.marker_arena.pages.len();
        let slice_us = t0.elapsed().as_micros() as u64;
        self.sweep_slice_us_total += slice_us;
        self.sweep_slice_count += 1;
        self.sweep_cons_blocks_swept += swept_blocks;
        self.sweep_noncons_freed += noncons_freed + float_freed;
        if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            eprintln!(
                "NEOVM_GC sweep_slice {slice_us}us cons={}/{} noncons_left={} done={done}",
                self.sweep_cons_cursor,
                self.cons_blocks.len(),
                if self.sweep_noncons_pending.is_null() {
                    0
                } else {
                    1
                },
            );
        }
        if done {
            self.finish_incremental_sweep();
        }
        done
    }

    /// Drive the deferred sweep to completion in one shot (forced GC, or before
    /// the next mark / a stop-the-world collection can begin).
    pub(crate) fn finish_incremental_sweep_now(&mut self) {
        while self.sweep_in_progress {
            self.incremental_sweep_slice(usize::MAX);
        }
    }

    /// Finalize the deferred sweep: recompute the cons live count from the mark
    /// bitmaps (cheap popcount; counts allocate-black new conses, excludes
    /// reclaimed ones), fix the allocation accounting, and emit the cycle trace.
    pub(super) fn finish_incremental_sweep(&mut self) {
        let recount: usize = self.cons_blocks.iter().map(ConsBlock::count_marked).sum();
        // allocated_count carries the tracked cons live count; replace it with
        // the true recount (delta may be negative -> use checked sub).
        if recount >= self.cons_live_count {
            self.allocated_count = self
                .allocated_count
                .saturating_add(recount - self.cons_live_count);
        } else {
            self.allocated_count = self
                .allocated_count
                .saturating_sub(self.cons_live_count - recount);
        }
        self.cons_live_count = recount;

        let _released_cons_blocks = self.release_empty_cons_blocks();
        let _released_object_pages = self.release_empty_object_pages();

        let mapped_cons_live: usize = self
            .mapped_cons_ranges
            .iter()
            .map(MappedConsRange::live_count)
            .sum();
        let cons_live_bytes = recount
            .saturating_add(mapped_cons_live)
            .saturating_mul(size_of::<ConsCell>());
        let mapped_object_live_bytes = self.mapped_non_cons_live_bytes();
        self.live_bytes = cons_live_bytes
            .saturating_add(self.sweep_noncons_live_bytes)
            .saturating_add(mapped_object_live_bytes);

        self.gc_collections += 1;
        self.sweep_lifetime_us += self.sweep_slice_us_total;
        self.sweep_lifetime_slices += self.sweep_slice_count;
        self.sweep_lifetime_cons_blocks_swept += self.sweep_cons_blocks_swept;
        self.sweep_lifetime_noncons_freed += self.sweep_noncons_freed;
        if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            let (mapped_total, mapped_marked) = self.mapped_object_stats();
            eprintln!(
                "NEOVM_GC gc#{} [incremental mark={}us sweep_total={}us slices={} blocks={} \
                 noncons_freed={}] cons_live={} heap_noncons={} dump_marked={}/{} live={}B",
                self.gc_collections,
                self.sweep_mark_us,
                self.sweep_slice_us_total,
                self.sweep_slice_count,
                self.sweep_cons_blocks_swept,
                self.sweep_noncons_freed,
                self.cons_live_count,
                self.non_cons_object_addrs.len(),
                mapped_marked,
                mapped_total,
                self.live_bytes,
            );
        }
        // Owner-tracking remembered-set precursor: NOT cleared at sweep
        // completion. Its lifecycle is clear-at-BEGIN (`begin_collection`), the
        // ABA-safe per-cycle discipline shared with the SATB sets; see the note
        // in `begin_collection` and `complete_collection`.
        self.sweep_in_progress = false;
    }

    pub(super) fn push_gray(&mut self, val: TaggedValue, origin: &str) {
        debug_assert!(val.is_heap_object());
        self.debug_assert_heap_tag_matches_header(val, origin);
        self.gray_queue.push(val);
    }

    pub(super) fn mark_symbol(&mut self, id: SymId) {
        // GNU `mark_object` sets the symbol's mark bit and moves on.  Whether
        // the symbol is interned is a sweep-time question (`is_value_marked`),
        // not a mark-time one: asking `is_canonical_id` here cost an
        // epoch-checked thread-local lookup per symbol reference visited.
        self.marked_symbols.insert(id);
    }

    pub(super) fn mark_or_push_child(&mut self, val: TaggedValue, origin: &str) {
        match val.kind() {
            crate::tagged::value::ValueKind::Symbol(id) => self.mark_symbol(id),
            _ if val.is_heap_object() => self.push_gray(val, origin),
            _ => {}
        }
    }

    #[cfg(debug_assertions)]
    pub(super) fn debug_assert_heap_tag_matches_header(&self, val: TaggedValue, origin: &str) {
        if val.is_cons() {
            return;
        }

        let (ptr, expected) = if val.is_string() {
            (
                val.as_string_ptr().unwrap() as *const u8,
                HeapObjectKind::String,
            )
        } else if val.is_float() {
            (
                val.as_float_ptr().unwrap() as *const u8,
                HeapObjectKind::Float,
            )
        } else if val.is_veclike() {
            (
                val.as_veclike_ptr().unwrap() as *const u8,
                HeapObjectKind::VecLike,
            )
        } else {
            return;
        };

        if !self.owns_non_cons_object(ptr) {
            return;
        }

        let header = unsafe { &*(ptr as *const GcHeader) };
        assert_eq!(
            header.kind,
            expected,
            "GC gray queue received malformed tagged heap value from {origin}: \
             value={:#x}, ptr={:?}, tag={}, header.kind={:?}, expected={:?}",
            val.0,
            ptr,
            val.tag(),
            header.kind,
            expected
        );
    }

    #[cfg(not(debug_assertions))]
    pub(super) fn debug_assert_heap_tag_matches_header(&self, _val: TaggedValue, _origin: &str) {}

    /// Mark a single tagged value and push its children onto the gray queue.
    pub(super) fn mark_value(&mut self, val: TaggedValue) {
        if let crate::tagged::value::ValueKind::Symbol(id) = val.kind() {
            self.mark_symbol(id);
        } else if val.is_cons() {
            // GNU `mark_object`'s cdr-chasing loop: a list is marked along
            // its spine inline, without a gray-queue push + pop + re-dispatch
            // per cell — for a megacons list that round trip is a second
            // multi-megabyte buffer streamed through the cache.
            let mut ptr = val.xcons_ptr();
            while self.mark_cons(ptr) {
                let car = unsafe { (*ptr).load_car() };
                let cdr = unsafe { (*ptr).load_cdr() };
                self.mark_or_push_child(car, "cons-car");
                if cdr.is_cons() {
                    ptr = cdr.xcons_ptr();
                    continue;
                }
                self.mark_or_push_child(cdr, "cons-cdr");
                break;
            }
        } else if val.is_string() {
            let ptr = val.as_string_ptr().unwrap() as *mut StringObj;
            // Dump-span test first: a mapped string used to walk the string
            // arena and miss the residual addr-set before classification.
            let addr = ptr as usize;
            if (addr >= self.dump_addr_lo && addr < self.dump_addr_hi)
                || !self.owns_string_object(ptr as *const u8)
            {
                if self.mark_mapped_string(ptr) {
                    unsafe {
                        let intervals = (*ptr).data.intervals();
                        if !intervals.is_empty() {
                            intervals.for_each_root(|root| {
                                self.mark_or_push_child(root, "mapped-string-interval");
                            });
                        }
                    }
                }
                return;
            }
            unsafe {
                // TENURED SHORT-CIRCUIT before the bit read: tenured objects
                // stay in `non_cons_object_addrs`, so this owned arm sees them
                // too. Their bit froze at promotion; interpreting it against
                // the current parity would read "unmarked" every other cycle
                // and re-trace the old generation (and trip the partition/
                // tricolor verifiers). Tenured ≡ permanently marked, never
                // re-traced — identical to the frozen-`true` behavior the
                // parity scheme replaced.
                if (*ptr).header.tenured {
                    return;
                }
                if (*ptr).header.is_marked_at(self.mark_parity) {
                    return;
                }
                (*ptr).header.set_marked(self.mark_parity);
                let intervals = (*ptr).data.intervals();
                if !intervals.is_empty() {
                    intervals.for_each_root(|root| {
                        self.mark_or_push_child(root, "string-interval");
                    });
                }
            };
        } else if val.is_float() {
            let ptr = val.as_float_ptr().unwrap() as *mut FloatObj;
            let addr = ptr as usize;
            if (addr >= self.dump_addr_lo && addr < self.dump_addr_hi)
                || !self.owns_float_object(ptr as *const u8)
            {
                let _ = self.mark_mapped_float(ptr);
                return;
            }
            unsafe {
                // Tenured short-circuit before the bit read (see string arm).
                if (*ptr).header.tenured || (*ptr).header.is_marked_at(self.mark_parity) {
                    return;
                }
                (*ptr).header.set_marked(self.mark_parity);
            };
        } else if val.is_veclike() {
            let ptr = val.as_veclike_ptr().unwrap() as *mut VecLikeHeader;
            // Dump-span test first: mapped veclikes paid all six arena range
            // checks plus an FxHashSet miss per mark (~10.7M Ir in the
            // first-cycle window of the type sim) before being classified.
            let addr = ptr as usize;
            if (addr >= self.dump_addr_lo && addr < self.dump_addr_hi)
                || !self.owns_veclike_object(ptr as *const u8)
            {
                if self.mark_mapped_veclike(ptr) {
                    unsafe {
                        self.trace_veclike(ptr);
                    }
                }
                return;
            }
            unsafe {
                // Tenured short-circuit before the bit read (see string arm):
                // permanent-black, never re-traced.
                if (*ptr).gc.tenured {
                    return;
                }
                if (*ptr).gc.is_marked_at(self.mark_parity) {
                    return;
                }
                (*ptr).gc.set_marked(self.mark_parity);
                self.trace_veclike(ptr);
            }
        }
    }

    /// Mark a cons cell. Returns true if newly marked (not previously marked).
    pub(super) fn mark_cons(&mut self, ptr: *const ConsCell) -> bool {
        // Mapped-world fast classification: in a fresh session MOST marked
        // conses are dump objects, and the old order made each of them miss
        // the block cache and probe `cons_block_index_by_base` before being
        // classified. Two compares against the dump span settle it first.
        let addr = ptr as usize;
        if addr >= self.dump_addr_lo && addr < self.dump_addr_hi {
            return self.mark_mapped_cons(ptr);
        }
        if ptr.is_null() || !ConsBlock::ptr_is_cell_aligned(ptr) {
            return self.mark_mapped_cons(ptr);
        }
        let block_base = ConsBlock::block_base_for_ptr(ptr);
        let block_index = match self.mark_cons_block_cache {
            Some(cache) if cache.block_base == block_base => cache.block_index,
            _ => {
                let Some(&block_index) = self.cons_block_index_by_base.get(&block_base) else {
                    return self.mark_mapped_cons(ptr);
                };
                self.mark_cons_block_cache =
                    Some(ConsBlockCacheEntry::new(block_base, block_index));
                block_index
            }
        };
        let block = &mut self.cons_blocks[block_index];
        if block.is_marked_ptr(ptr) {
            return false;
        }
        block.mark_ptr(ptr);
        true
    }

    pub(super) fn mark_mapped_cons(&mut self, ptr: *const ConsCell) -> bool {
        for range in &mut self.mapped_cons_ranges {
            if !range.contains_ptr(ptr) {
                continue;
            }
            if range.is_marked_ptr(ptr) {
                return false;
            }
            range.mark_ptr(ptr);
            return true;
        }
        false
    }

    pub(super) fn mark_mapped_float(&mut self, ptr: *const FloatObj) -> bool {
        for range in &mut self.mapped_float_ranges {
            if !range.contains_ptr(ptr) {
                continue;
            }
            if range.is_marked_ptr(ptr) {
                return false;
            }
            range.mark_ptr(ptr);
            return true;
        }
        false
    }

    pub(super) fn mark_mapped_veclike(&mut self, ptr: *const VecLikeHeader) -> bool {
        let Some(&index) = self.mapped_veclike_index_by_addr.get(&(ptr as usize)) else {
            return false;
        };
        let object = &mut self.mapped_veclike_objects[index];
        debug_assert!(std::ptr::eq(object.header as *const VecLikeHeader, ptr));
        if object.marked {
            return false;
        }
        object.marked = true;
        true
    }

    pub(super) fn mark_mapped_string(&mut self, ptr: *const StringObj) -> bool {
        let Some(&index) = self.mapped_string_index_by_addr.get(&(ptr as usize)) else {
            return false;
        };
        let object = &mut self.mapped_string_objects[index];
        debug_assert!(std::ptr::eq(object.ptr as *const StringObj, ptr));
        if object.marked {
            return false;
        }
        object.marked = true;
        true
    }

    /// Trace children of a vectorlike object, pushing them onto the gray queue.
    pub(super) unsafe fn trace_veclike(&mut self, ptr: *mut VecLikeHeader) {
        match unsafe { (*ptr).type_tag } {
            VecLikeType::Vector => {
                let obj = ptr as *const VectorObj;
                for val in unsafe { (*obj).data.iter_atomic() } {
                    self.mark_or_push_child(val, "vector-slot");
                }
            }
            VecLikeType::CharTable => {
                let obj = unsafe { &*(ptr as *const CharTableObj) };
                for (value, origin) in [
                    (load_value_atomic(&obj.defalt), "char-table-default"),
                    (load_value_atomic(&obj.parent), "char-table-parent"),
                    (load_value_atomic(&obj.purpose), "char-table-purpose"),
                    (load_value_atomic(&obj.ascii), "char-table-ascii"),
                ] {
                    self.mark_or_push_child(value, origin);
                }
                for slot in &obj.contents {
                    let val = load_value_atomic(slot);
                    self.mark_or_push_child(val, "char-table-content");
                }
                for val in obj.extras.iter_atomic() {
                    self.mark_or_push_child(val, "char-table-extra");
                }
            }
            VecLikeType::SubCharTable => {
                let obj = unsafe { &*(ptr as *const SubCharTableObj) };
                for val in obj.contents.iter_atomic() {
                    self.mark_or_push_child(val, "sub-char-table-content");
                }
            }
            VecLikeType::Record | VecLikeType::WindowConfiguration => {
                let obj = ptr as *const RecordObj;
                for val in unsafe { (*obj).data.iter_atomic() } {
                    self.mark_or_push_child(val, "record-slot");
                }
            }
            VecLikeType::Font => {
                let font = unsafe { &(*(ptr as *const FontObj)).data };
                for val in font.fields.iter_atomic() {
                    self.mark_or_push_child(val, "font-property");
                }
                self.mark_or_push_child(load_value_atomic(&font.capability), "font-capability");
            }
            VecLikeType::HashTable => {
                let obj = ptr as *const HashTableObj;
                let ht = unsafe { &(*obj).table };
                if ht.weakness.is_some() {
                    // Weak table: DON'T trace its entries here — that would keep
                    // every key/value alive and defeat weakness. Record it; the
                    // per-entry survival decision happens in
                    // `mark_and_sweep_weak_tables` at the stop-the-world
                    // `complete_collection`, after the main mark drains (GNU
                    // `mark_and_sweep_weak_table_contents`). The remembered-set /
                    // SATB / permanent-scan paths now also defer weak entries
                    // (`register_weak_hash_table_for_sweep` registers the table
                    // and pushes only its non-weak closures), and a tenured/mapped
                    // weak table is re-registered every cycle via
                    // `permanent_weak_hash_tables`, so weak semantics hold for
                    // young, tenured, and dumped tables alike. The weak sweep runs
                    // before `verify_dump_partition`, so dead entries are removed
                    // before the verifier enumerates — no UAF.
                    let tptr = obj as *mut HashTableObj;
                    if self.weak_hash_tables_set.insert(tptr) {
                        self.weak_hash_tables.push(tptr);
                    }
                } else if let Some(pending) = ht.data.pending_entries() {
                    // Un-hydrated dump table: its entries live in the parked
                    // vec; trace exactly the set the hydrated arms below
                    // would (values + key snapshots - HashKeys are not
                    // walked in either form).
                    for (_, value, snapshot) in pending {
                        self.mark_or_push_child(*value, "hash-table-pending-value");
                        if let Some(snapshot) = snapshot {
                            self.mark_or_push_child(*snapshot, "hash-table-pending-key");
                        }
                    }
                } else {
                    // Trace all values in the hash table
                    for slot in ht.data.values() {
                        let val = load_value_atomic(slot);
                        self.mark_or_push_child(val, "hash-table-value");
                    }
                    // Trace key snapshots (original key objects)
                    for slot in ht.key_snapshots() {
                        let val = load_value_atomic(slot);
                        self.mark_or_push_child(val, "hash-table-key-snapshot");
                    }
                }
                // Custom test/hash closures (from `define-hash-table-test`) live
                // ONLY in these fields. Without tracing them the closure is swept
                // while the table is still live, and the next custom-test
                // gethash/puthash calls a freed function (use-after-free). The
                // fields are immutable after table creation, so a plain read is
                // race-free during a concurrent mark.
                if let Some(f) = ht.user_cmp_function {
                    self.mark_or_push_child(f, "hash-table-user-cmp");
                }
                if let Some(f) = ht.user_hash_function {
                    self.mark_or_push_child(f, "hash-table-user-hash");
                }
            }
            VecLikeType::Obarray => {
                let obj = unsafe { &*(ptr as *const ObarrayObj) };
                for val in obj.buckets.iter_atomic() {
                    self.mark_or_push_child(val, "obarray-bucket");
                }
            }
            VecLikeType::Lambda | VecLikeType::Macro => {
                // Closures are plain Value vectors (GNU PVEC_CLOSURE compat).
                // Trace ALL slots uniformly — no type-specific logic needed.
                let obj = ptr as *const LambdaObj;
                for val in unsafe { (*obj).data.iter_atomic() } {
                    self.mark_or_push_child(val, "closure-slot");
                }
            }
            VecLikeType::ByteCode => {
                let obj = ptr as *const ByteCodeObj;
                let data = unsafe { &(*obj).data };
                // LAZY STUB LEG — lockstep with the collect arm: children
                // are read from the patched image, never from the (empty)
                // struct vectors, and nothing allocates under GC.
                if data.is_pdump_stub() {
                    unsafe {
                        crate::emacs_core::pdump::mapped_heap::for_each_stub_bytecode_child(
                            obj,
                            data.closure_slot_count,
                            |child| self.mark_or_push_child(child, "bytecode-stub-image"),
                        );
                    }
                    return;
                }
                self.mark_or_push_child(data.arglist, "bytecode-arglist");
                // Trace constants vector
                for val in &data.constants {
                    self.mark_or_push_child(*val, "bytecode-constant");
                }
                // Trace captured lexical environment
                if let Some(env) = data.env {
                    self.mark_or_push_child(env, "bytecode-env");
                }
                // Trace doc_form (can be a Value)
                if let Some(doc_form) = data.doc_form {
                    self.mark_or_push_child(doc_form, "bytecode-doc-form");
                }
                // Trace interactive spec
                if let Some(interactive) = data.interactive {
                    self.mark_or_push_child(interactive, "bytecode-interactive");
                }
                for val in &data.extra_slots {
                    self.mark_or_push_child(*val, "bytecode-extra-slot");
                }
            }
            VecLikeType::Overlay => {
                let obj = ptr as *const OverlayObj;
                let data = unsafe { &(*obj).data };
                // Trace the property list
                let plist = load_value_atomic(&data.plist);
                self.mark_or_push_child(plist, "overlay-plist");
            }
            VecLikeType::SymbolWithPos => {
                // Trace both the symbol and the position fields.
                let obj = ptr as *const SymbolWithPosObj;
                let sym = unsafe { (*obj).sym };
                let pos = unsafe { (*obj).pos };
                self.mark_or_push_child(sym, "symbol-with-pos-symbol");
                self.mark_or_push_child(pos, "symbol-with-pos-position");
            }
            VecLikeType::Finalizer => {
                // A REACHABLE finalizer keeps its function alive (GNU
                // `mark_vectorlike` on PVEC_FINALIZER). Unreachable ones are
                // handled at mark termination by
                // `mark_and_queue_doomed_finalizers`.
                let function = unsafe { (*(ptr as *const FinalizerObj)).function };
                self.mark_or_push_child(function, "finalizer-function");
            }
            VecLikeType::ModuleFunction => {
                let obj = ptr as *const ModuleFunctionObj;
                let doc = unsafe { (*obj).documentation };
                let interactive = unsafe { (*obj).interactive_form };
                self.mark_or_push_child(doc, "module-function-documentation");
                self.mark_or_push_child(interactive, "module-function-interactive");
            }
            VecLikeType::Xwidget => {
                let obj = ptr as *const XwidgetObj;
                let fields = unsafe {
                    [
                        (load_value_atomic(&(*obj).plist), "xwidget-plist"),
                        (load_value_atomic(&(*obj).type_), "xwidget-type"),
                        (load_value_atomic(&(*obj).buffer), "xwidget-buffer"),
                        (load_value_atomic(&(*obj).title), "xwidget-title"),
                        (
                            load_value_atomic(&(*obj).script_callbacks),
                            "xwidget-script-callbacks",
                        ),
                    ]
                };
                for (value, label) in fields {
                    self.mark_or_push_child(value, label);
                }
            }
            VecLikeType::XwidgetView => {
                let obj = ptr as *const XwidgetViewObj;
                let fields = unsafe {
                    [
                        ((*obj).model, "xwidget-view-model"),
                        ((*obj).window, "xwidget-view-window"),
                    ]
                };
                for (value, label) in fields {
                    self.mark_or_push_child(value, label);
                }
            }
            VecLikeType::Buffer
            | VecLikeType::Window
            | VecLikeType::Frame
            | VecLikeType::Timer
            | VecLikeType::Process
            | VecLikeType::Terminal
            | VecLikeType::Marker
            | VecLikeType::Subr
            | VecLikeType::Bignum
            | VecLikeType::Sqlite
            | VecLikeType::UserPtr
            | VecLikeType::SurfaceHandle
            | VecLikeType::VideoHandle => {
                // These have no Value children to trace.
                //
                // Bignums own a `malachite::Integer`, which manages
                // its own limb buffer, but no Lisp_Object children —
                // `Drop` takes care of the memory in `free_gc_object`.
                //
                // UserPtr has only a raw C pointer and finalizer, no
                // Lisp children.
                //
                // SurfaceHandle and VideoHandle contain only typed ids.
            }
        }
    }

    /// Sweep unmarked cons cells back to free lists.
    pub(super) fn sweep_cons(&mut self) -> usize {
        let old_live = self.cons_live_count;
        let mut new_live = 0;
        self.cons_free_list = std::ptr::null_mut();
        for block in &mut self.cons_blocks {
            new_live += block.sweep(&mut self.cons_free_list);
        }
        self.cons_live_count = new_live;
        self.allocated_count = self
            .allocated_count
            .saturating_sub(old_live)
            .saturating_add(new_live);
        let mapped_live = self
            .mapped_cons_ranges
            .iter()
            .map(MappedConsRange::live_count)
            .sum::<usize>();
        new_live
            .saturating_add(mapped_live)
            .saturating_mul(size_of::<ConsCell>())
    }

    /// Drop ordinary cons blocks with no survivors and rebuild the intrusive
    /// free list plus base-address registry. The free list contains pointers
    /// into dead cells, so it must be discarded before empty block storage is
    /// deallocated and reconstructed from the retained blocks afterward.
    /// Call only after a complete eager or deferred sweep, when mark bits are
    /// the authoritative live-cell set and no sweep cursor remains active.
    pub(super) fn release_empty_cons_blocks(&mut self) -> usize {
        let old_len = self.cons_blocks.len();
        if !self
            .cons_blocks
            .iter()
            .any(|block| block.count_marked() == 0)
        {
            return 0;
        }

        self.cons_free_list = std::ptr::null_mut();
        self.mark_cons_block_cache = None;
        self.cons_blocks.retain(|block| block.count_marked() != 0);
        self.cons_blocks.shrink_to_fit();

        self.cons_block_index_by_base =
            FxHashMap::with_capacity_and_hasher(self.cons_blocks.len(), Default::default());
        let mut rebuilt_live = 0usize;
        for (block_index, block) in self.cons_blocks.iter_mut().enumerate() {
            let previous = self
                .cons_block_index_by_base
                .insert(block.base_addr(), block_index);
            debug_assert!(previous.is_none(), "cons block base registered twice");
            rebuilt_live += block.sweep(&mut self.cons_free_list);
        }
        debug_assert_eq!(rebuilt_live, self.cons_live_count);

        old_len - self.cons_blocks.len()
    }

    /// Release every completely empty young arena page after a full sweep.
    /// Per-class indices and partial chains are rebuilt inside each arena.
    pub(super) fn release_empty_object_pages(&mut self) -> usize {
        self.float_arena.release_empty_pages()
            + self.string_arena.release_empty_pages()
            + self.vector_arena.release_empty_pages()
            + self.bytecode_arena.release_empty_pages()
            + self.lambda_arena.release_empty_pages()
            + self.macro_arena.release_empty_pages()
            + self.record_arena.release_empty_pages()
            + self.symbol_with_pos_arena.release_empty_pages()
            + self.marker_arena.release_empty_pages()
    }

    /// Sweep non-cons objects: walk intrusive list, free unmarked, rebuild list.
    pub(super) fn sweep_objects(&mut self) -> usize {
        // `unchain_dead_markers` (invoked in `complete_collection`
        // between mark and sweep) has already spliced unmarked markers
        // out of every live buffer's intrusive chain, so freeing them
        // here leaves no dangling chain pointers. Mirrors GNU
        // `sweep_buffer → unchain_dead_markers` (alloc.c).
        // `all_objects` is young-only; interpret bits at the parity of the
        // cycle that just marked them (this eager sweep runs inside the same
        // collection, before any next flip).
        let parity = self.mark_parity;
        let mut prev: *mut *mut GcHeader = &mut self.all_objects;
        let mut current = self.all_objects;
        let mut live_bytes = 0usize;
        while !current.is_null() {
            unsafe {
                let next = (*current).next;
                debug_assert!(!(*current).tenured, "tenured object on the young list");
                if (*current).is_marked_at(parity) {
                    // Keep it — advance prev
                    live_bytes = live_bytes.saturating_add(Self::object_bytes_from_header(current));
                    prev = &mut (*current).next;
                    current = next;
                } else {
                    // Free it — unlink from list
                    *prev = next;
                    self.non_cons_object_addrs.remove(&(current as usize));
                    self.unregister_vector_object(current);
                    self.free_gc_object(current);
                    self.allocated_count = self.allocated_count.saturating_sub(1);
                    current = next;
                }
            }
        }

        live_bytes
    }

    /// Sweep every class arena's pages `[start, end)` (per-class ranges) —
    /// the shared page reclaimer behind both sweep entry points. See
    /// `ObjectArena::sweep_range` for the visit contract (allocated-bit-first,
    /// tenured-skip, drop-in-place-before-bit-clear, retired-page skip).
    /// Vector slots are evicted from the incremental vector registry at the
    /// free hook — page vectors never pass `unregister_vector_object`.
    /// Bytecode / lambda / macro / record have no side registry: their free
    /// hook is a no-op (the `drop_in_place` inside `sweep_range` frees the
    /// REAL payload — bytecode's ops + constants vectors, params, GNU byte
    /// maps, docstring; lambda/macro/record's slot `Vec`). SymbolWithPos is
    /// POD (no payload — its `drop_in_place` compiles out) and likewise has no
    /// registry.
    ///
    /// Returns `(survivor bytes, slots freed)` summed over the classes.
    // One `(start, end)` per size class — a mechanical fan-out over the
    // per-class arenas, not distinct conceptual parameters. The eager path
    // passes every class's full range; the incremental path passes one real
    // range and `(0, 0)` for the rest.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sweep_arena_pages_ranges(
        &mut self,
        float_range: (usize, usize),
        string_range: (usize, usize),
        vector_range: (usize, usize),
        bytecode_range: (usize, usize),
        lambda_range: (usize, usize),
        macro_range: (usize, usize),
        record_range: (usize, usize),
        symbol_with_pos_range: (usize, usize),
        marker_range: (usize, usize),
    ) -> (usize, usize) {
        let parity = self.mark_parity;
        let (fl, ff) = self
            .float_arena
            .sweep_range(float_range.0, float_range.1, parity, |_| {});
        let (sl, sf) =
            self.string_arena
                .sweep_range(string_range.0, string_range.1, parity, |_| {});
        let (bl, bf) =
            self.bytecode_arena
                .sweep_range(bytecode_range.0, bytecode_range.1, parity, |_| {});
        let (lal, laf) =
            self.lambda_arena
                .sweep_range(lambda_range.0, lambda_range.1, parity, |_| {});
        let (mal, maf) = self
            .macro_arena
            .sweep_range(macro_range.0, macro_range.1, parity, |_| {});
        let (rel, ref_) =
            self.record_arena
                .sweep_range(record_range.0, record_range.1, parity, |_| {});
        let (swl, swf) = self.symbol_with_pos_arena.sweep_range(
            symbol_with_pos_range.0,
            symbol_with_pos_range.1,
            parity,
            |_| {},
        );
        // Markers: `unchain_dead_markers` already detached every unmarked
        // marker from its buffer chain (it runs before the first sweep slice
        // and before the eager sweep), so freeing the slot here cannot leave
        // a dangling chain link.
        let (mkl, mkf) =
            self.marker_arena
                .sweep_range(marker_range.0, marker_range.1, parity, |_| {});
        let TaggedHeap {
            vector_arena,
            vector_object_addrs,
            ..
        } = self;
        let (vl, vf) = vector_arena.sweep_range(vector_range.0, vector_range.1, parity, |addr| {
            let removed = vector_object_addrs.remove(&addr);
            debug_assert!(removed, "freed page vector was not in the registry");
        });
        let freed = ff + sf + vf + bf + laf + maf + ref_ + swf + mkf;
        self.allocated_count = self.allocated_count.saturating_sub(freed);
        (fl + sl + vl + bl + lal + mal + rel + swl + mkl, freed)
    }

    /// `(total mapped objects, mapped objects currently marked)`.
    ///
    /// The marked count is how many immutable pdump (mapped) objects the mark
    /// phase re-traced this cycle — pure overhead that a "dump as permanent
    /// tenured region" partition would eliminate, since mapped objects are
    /// never freed. Used only for GC phase instrumentation.
    pub(super) fn mapped_object_stats(&self) -> (usize, usize) {
        let veclike_total = self.mapped_veclike_objects.len();
        let veclike_marked = self
            .mapped_veclike_objects
            .iter()
            .filter(|object| object.marked)
            .count();
        let string_total = self.mapped_string_objects.len();
        let string_marked = self
            .mapped_string_objects
            .iter()
            .filter(|object| object.marked)
            .count();
        let cons_total: usize = self.mapped_cons_ranges.iter().map(|range| range.len).sum();
        let cons_marked: usize = self
            .mapped_cons_ranges
            .iter()
            .map(MappedConsRange::live_count)
            .sum();
        let float_total: usize = self.mapped_float_ranges.iter().map(|range| range.len).sum();
        let float_marked: usize = self
            .mapped_float_ranges
            .iter()
            .map(MappedFloatRange::live_count)
            .sum();
        (
            veclike_total + string_total + cons_total + float_total,
            veclike_marked + string_marked + cons_marked + float_marked,
        )
    }

    pub(super) fn mapped_non_cons_live_bytes(&self) -> usize {
        self.mapped_float_ranges
            .iter()
            .map(|range| range.live_count().saturating_mul(size_of::<FloatObj>()))
            .chain(
                self.mapped_veclike_objects
                    .iter()
                    .filter(|object| object.marked)
                    .map(|object| object.byte_len),
            )
            .chain(
                self.mapped_string_objects
                    .iter()
                    .filter(|object| object.marked)
                    .map(|object| object.byte_len),
            )
            .sum()
    }

    /// Free a GC object by its header pointer.
    /// Must determine the actual type to call the correct Drop and dealloc.
    pub(super) unsafe fn free_gc_object(&mut self, header: *mut GcHeader) {
        let kind = unsafe { (*header).kind };
        match kind {
            HeapObjectKind::String => {
                unsafe { drop(Box::from_raw(header as *mut StringObj)) };
            }
            HeapObjectKind::Float => {
                unsafe { drop(Box::from_raw(header as *mut FloatObj)) };
            }
            HeapObjectKind::VecLike => {
                let ptr = header as *mut VecLikeHeader;
                let type_tag = unsafe { (*ptr).type_tag };
                match type_tag {
                    VecLikeType::Vector => unsafe { drop(Box::from_raw(ptr as *mut VectorObj)) },
                    VecLikeType::CharTable => unsafe {
                        drop(Box::from_raw(ptr as *mut CharTableObj))
                    },
                    VecLikeType::SubCharTable => unsafe {
                        drop(Box::from_raw(ptr as *mut SubCharTableObj))
                    },
                    VecLikeType::HashTable => unsafe {
                        drop(Box::from_raw(ptr as *mut HashTableObj))
                    },
                    VecLikeType::Obarray => unsafe { drop(Box::from_raw(ptr as *mut ObarrayObj)) },
                    VecLikeType::Lambda => unsafe { drop(Box::from_raw(ptr as *mut LambdaObj)) },
                    VecLikeType::Macro => unsafe { drop(Box::from_raw(ptr as *mut MacroObj)) },
                    VecLikeType::ByteCode => unsafe {
                        // Residual-Box seam only: page bytecode never enters
                        // the intrusive lists this fn sweeps (task 03/3a —
                        // `alloc_bytecode` is page-only, so this arm is
                        // unreachable today; kept so any future Box producer
                        // stays leak-free by construction).
                        drop(Box::from_raw(ptr as *mut ByteCodeObj))
                    },
                    VecLikeType::Record | VecLikeType::WindowConfiguration => unsafe {
                        drop(Box::from_raw(ptr as *mut RecordObj))
                    },
                    VecLikeType::Font => unsafe { drop(Box::from_raw(ptr as *mut FontObj)) },
                    VecLikeType::Overlay => unsafe { drop(Box::from_raw(ptr as *mut OverlayObj)) },
                    VecLikeType::Marker => unsafe { drop(Box::from_raw(ptr as *mut MarkerObj)) },
                    VecLikeType::Buffer => unsafe { drop(Box::from_raw(ptr as *mut BufferObj)) },
                    VecLikeType::Window => unsafe { drop(Box::from_raw(ptr as *mut WindowObj)) },
                    VecLikeType::Frame => unsafe { drop(Box::from_raw(ptr as *mut FrameObj)) },
                    VecLikeType::Timer => unsafe { drop(Box::from_raw(ptr as *mut TimerObj)) },
                    VecLikeType::Process => unsafe { drop(Box::from_raw(ptr as *mut ProcessObj)) },
                    VecLikeType::Terminal => unsafe {
                        drop(Box::from_raw(ptr as *mut TerminalObj))
                    },
                    VecLikeType::Xwidget => unsafe { drop(Box::from_raw(ptr as *mut XwidgetObj)) },
                    VecLikeType::XwidgetView => unsafe {
                        drop(Box::from_raw(ptr as *mut XwidgetViewObj))
                    },
                    VecLikeType::SurfaceHandle => {
                        // A dead handle means Lisp dropped its last reference
                        // to the GPU surface: queue the id so the evaluator's
                        // post-collection drain can destroy the host objects.
                        // The sweep has no display-host access, so record only.
                        let obj = ptr as *mut SurfaceObj;
                        let surface_id = unsafe { (*obj).surface_id };
                        self.pending_surface_destroys.push(surface_id);
                        unsafe { drop(Box::from_raw(obj)) };
                    }
                    VecLikeType::VideoHandle => {
                        let obj = ptr as *mut VideoObj;
                        let video_id = unsafe { (*obj).video_id };
                        self.pending_video_destroys.push(video_id);
                        unsafe { drop(Box::from_raw(obj)) };
                    }
                    VecLikeType::Subr => unsafe { drop(Box::from_raw(ptr as *mut SubrObj)) },
                    VecLikeType::Bignum => unsafe {
                        // Box::drop runs malachite::Integer::drop, which
                        // frees the underlying limb buffer.
                        drop(Box::from_raw(ptr as *mut BignumObj))
                    },
                    VecLikeType::SymbolWithPos => unsafe {
                        drop(Box::from_raw(ptr as *mut SymbolWithPosObj))
                    },
                    VecLikeType::Finalizer => unsafe {
                        // The registry entry was already removed by the
                        // mark-termination scan that doomed this object; the
                        // function it queued survives independently.
                        drop(Box::from_raw(ptr as *mut FinalizerObj))
                    },
                    VecLikeType::Sqlite => unsafe { drop(Box::from_raw(ptr as *mut SqliteObj)) },
                    VecLikeType::UserPtr => {
                        // Call the finalizer if present before dropping.
                        let up = ptr as *mut UserPtrObj;
                        if let Some(fin) = unsafe { (*up).finalizer } {
                            unsafe { fin((*up).ptr) };
                        }
                        unsafe { drop(Box::from_raw(up)) };
                    }
                    VecLikeType::ModuleFunction => {
                        // Call the finalizer if present before dropping.
                        let mf = ptr as *mut ModuleFunctionObj;
                        if let Some(fin) = unsafe { (*mf).finalizer } {
                            unsafe { fin((*mf).data) };
                        }
                        unsafe { drop(Box::from_raw(mf)) };
                    }
                }
            }
        }
    }

    /// Per-kind ownership oracles (tag-first dispatch): each consults ONLY
    /// its class's page-span registry plus the residual `Box` addr-set, so a
    /// page hit can never be a cross-class collision. Mapped (pdump) objects
    /// answer false everywhere here — the not-owned fallback keeps routing
    /// them to the mapped side-table arms, unchanged.
    #[inline]
    pub(super) fn owns_float_object(&self, ptr: *const u8) -> bool {
        !ptr.is_null()
            && (self.float_arena.owns(ptr) || self.non_cons_object_addrs.contains(&(ptr as usize)))
    }

    #[inline]
    pub(super) fn owns_string_object(&self, ptr: *const u8) -> bool {
        !ptr.is_null()
            && (self.string_arena.owns(ptr) || self.non_cons_object_addrs.contains(&(ptr as usize)))
    }

    #[inline]
    pub(super) fn owns_veclike_object(&self, ptr: *const u8) -> bool {
        // `VecLikeType::Vector`, `ByteCode`, `Lambda`, `Macro`, `Record`
        // (incl. the `WindowConfiguration` tag — same `RecordObj`),
        // `SymbolWithPos` and `Marker` are paged (each in its own class arena —
        // distinct registries, so a hit is never a cross-class collision);
        // every other veclike is a residual `Box` in the addr-set.
        !ptr.is_null()
            && (self.vector_arena.owns(ptr)
                || self.bytecode_arena.owns(ptr)
                || self.lambda_arena.owns(ptr)
                || self.macro_arena.owns(ptr)
                || self.record_arena.owns(ptr)
                || self.symbol_with_pos_arena.owns(ptr)
                || self.marker_arena.owns(ptr)
                || self.non_cons_object_addrs.contains(&(ptr as usize)))
    }

    /// Tag-dispatched ownership for a heap value whose raw object address is
    /// `addr` (`value_heap_addr`). The per-class page registries are checked
    /// per the value's TAG (never merged — see `ObjectArena`), with the
    /// residual addr-set covering the unmigrated `Box` types.
    #[inline]
    pub(super) fn owns_heap_value_object(&self, value: TaggedValue, addr: usize) -> bool {
        let ptr = addr as *const u8;
        if value.is_string() {
            self.owns_string_object(ptr)
        } else if value.is_float() {
            self.owns_float_object(ptr)
        } else if value.is_veclike() {
            self.owns_veclike_object(ptr)
        } else {
            false
        }
    }

    /// Tag-less union oracle used by debug checks and GC tests that have not
    /// decoded the value's heap tag yet.
    #[cfg(any(debug_assertions, test))]
    pub(super) fn owns_non_cons_object(&self, ptr: *const u8) -> bool {
        !ptr.is_null()
            && (self.string_arena.owns(ptr)
                || self.vector_arena.owns(ptr)
                || self.bytecode_arena.owns(ptr)
                || self.lambda_arena.owns(ptr)
                || self.macro_arena.owns(ptr)
                || self.record_arena.owns(ptr)
                || self.symbol_with_pos_arena.owns(ptr)
                || self.marker_arena.owns(ptr)
                || self.float_arena.owns(ptr)
                || self.non_cons_object_addrs.contains(&(ptr as usize)))
    }

    /// Post-mark verification: check that every marked non-cons object is
    /// actually in one of our intrusive lists (young `all_objects` or tenured
    /// `tenured_objects`). If a marked object is NOT in a list, it means a
    /// root pointed to freed memory that happened to look like a valid tagged
    /// pointer — precisely the failure DIVERGENCES.md 161 chased for a day.
    ///
    /// Returns the number of problems found. Wired into `complete_collection`
    /// behind [`verify_marked_objects_enabled`], which is off unless
    /// `NEOVM_GC_VERIFY_MARKED=1` or a test turns it on: the walk is O(live
    /// objects) per collection. It was dead code with an `#[allow(dead_code)]`
    /// "delete or wire up" note until ledger 162 wired it up.
    #[cfg(debug_assertions)]
    pub(super) fn verify_marked_objects_owned(&self) -> usize {
        let mut problems = 0usize;
        // Build a set of all owned non-cons object addresses
        let mut owned_addrs: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for head in [self.all_objects, self.tenured_objects] {
            let mut obj = head;
            while !obj.is_null() {
                owned_addrs.insert(obj as usize);
                unsafe {
                    obj = (*obj).next;
                }
            }
        }

        // Now walk both lists again and check marked objects. Tenured objects
        // are permanently marked (frozen bit, exempt from parity); young ones
        // are interpreted at the current parity.
        let parity = self.mark_parity;
        let mut total_marked = 0usize;
        for head in [self.all_objects, self.tenured_objects] {
            let mut current = head;
            while !current.is_null() {
                unsafe {
                    if (*current).tenured || (*current).is_marked_at(parity) {
                        total_marked += 1;
                        // Verify the object's internal data is sane
                        if (*current).kind == HeapObjectKind::String {
                            let ptr = current as *const StringObj;
                            let s = &(*ptr).data;
                            // Check string data pointer is reasonable
                            let str_ptr = s.as_bytes().as_ptr() as usize;
                            if str_ptr != 0 && str_ptr < 0x1000 {
                                problems += 1;
                                tracing::error!(
                                    "GC VERIFY: marked StringObj at {:p} has \
                                     corrupt data pointer {:#x}",
                                    current,
                                    str_ptr
                                );
                            }
                        }
                    }
                    current = (*current).next;
                }
            }
        }
        // OBJECT ARENA PAGES: page objects live outside the intrusive lists —
        // their ownership authority is the page-span oracle (per-class
        // registry + stride + allocation bitmap), NOT `non_cons_object_addrs`.
        // Walk them ALLOCATED-BIT-FIRST (reading a clear-bit slot's header
        // would itself be the class of bug this verifier hunts) and check each
        // live slot's header coherence and that it is NOT in the residual
        // addr-set (a page slot in the addr-set would be a double-ownership
        // corruption: two reclaimers for one object). Tenured slots are legal
        // (the promotion page walk) and count as marked, frozen-bit-first.
        fn verify_arena_slots<T: PagedObject>(
            arena: &ObjectArena<T>,
            non_cons_object_addrs: &FxHashSet<usize>,
            parity: bool,
            total_marked: &mut usize,
            problems: &mut usize,
        ) {
            for slot in arena.collect_allocated_slots() {
                let header = slot as *const GcHeader;
                unsafe {
                    if (*header).kind != T::KIND {
                        *problems += 1;
                        tracing::error!(
                            "GC VERIFY: {} arena slot at {:p} has a wrong-kind header",
                            T::CLASS,
                            slot
                        );
                    }
                    if (*header).tenured || (*header).is_marked_at(parity) {
                        *total_marked += 1;
                    }
                }
                if non_cons_object_addrs.contains(&(slot as usize)) {
                    *problems += 1;
                    tracing::error!(
                        "GC VERIFY: {} arena slot {:p} must NOT be in \
                         non_cons_object_addrs (page-span oracle owns it)",
                        T::CLASS,
                        slot
                    );
                }
            }
        }
        verify_arena_slots(
            &self.float_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.string_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.vector_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.bytecode_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.lambda_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.macro_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.record_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.symbol_with_pos_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        verify_arena_slots(
            &self.marker_arena,
            &self.non_cons_object_addrs,
            parity,
            &mut total_marked,
            &mut problems,
        );
        tracing::trace!(
            "GC verify: {} marked non-cons objects, {} problem(s)",
            total_marked,
            problems
        );
        problems
    }

    /// TEST-ONLY object-arena coherence check over ALL class arenas:
    /// the allocation bitmaps, occupancy counts, page-local free lists,
    /// partial-page chains, page-base registries, retirement invariants, the
    /// page-span ownership oracle, and the residual addr-set must all agree.
    /// Free lists are walked via the trailing link words only — a freed
    /// slot's header bytes are never read (allocated-bit-first applies to
    /// verifiers too). Page slots must NOT be in `non_cons_object_addrs`
    /// (the page-span oracle is their sole ownership authority — the
    /// INVERSE of the float-v1 assertion). For the vector arena, every
    /// allocated slot must be in the incremental vector registry; bytecode
    /// slots must NOT be (the registry is the Tier-B vector snapshot source
    /// — bytecode stays deferred-at-termination and has no registry).
    /// TEST-ONLY page-span ownership probe for the bytecode arena, for tests
    /// that live outside this module (the pdump restore-path round-trip).
    #[cfg(test)]
    pub(crate) fn bytecode_arena_owns_for_test(&self, ptr: *const u8) -> bool {
        self.bytecode_arena.owns(ptr)
    }

    /// TEST-ONLY mapped-image ownership probe: true when the value's storage
    /// lives inside the loaded dump image span (image-resident objects).
    #[cfg(test)]
    pub(crate) fn mapped_image_owns_for_test(&self, value: TaggedValue) -> bool {
        self.owner_is_mapped(value)
    }

    #[cfg(test)]
    pub(crate) fn assert_object_arenas_coherent(&self) {
        self.assert_one_arena_coherent(&self.float_arena);
        self.assert_one_arena_coherent(&self.string_arena);
        self.assert_one_arena_coherent(&self.vector_arena);
        self.assert_one_arena_coherent(&self.bytecode_arena);
        self.assert_one_arena_coherent(&self.lambda_arena);
        self.assert_one_arena_coherent(&self.macro_arena);
        self.assert_one_arena_coherent(&self.record_arena);
        self.assert_one_arena_coherent(&self.symbol_with_pos_arena);
        self.assert_one_arena_coherent(&self.marker_arena);
        // Vector registry ⊇ page vector slots (page alloc inserts; page sweep
        // removes). The registry may also hold residual Box vectors.
        for slot in self.vector_arena.collect_allocated_slots() {
            assert!(
                self.vector_object_addrs.contains(&(slot as usize)),
                "allocated page vector slot {slot:p} missing from the vector registry",
            );
        }
        // Bytecode slots carry the right type tag (the generic per-arena
        // check can only see the shared VecLike GcHeader kind) and never
        // leak into the vector registry.
        for slot in self.bytecode_arena.collect_allocated_slots() {
            assert_eq!(
                unsafe { (*(slot as *const VecLikeHeader)).type_tag },
                VecLikeType::ByteCode,
                "bytecode arena slot {slot:p} carries a non-ByteCode type tag",
            );
            assert!(
                !self.vector_object_addrs.contains(&(slot as usize)),
                "bytecode arena slot {slot:p} must NOT be in the vector registry",
            );
        }
        // Lambda/macro slots carry their own type tag and never leak into the
        // vector registry (they have no side registry — like bytecode).
        for slot in self.lambda_arena.collect_allocated_slots() {
            assert_eq!(
                unsafe { (*(slot as *const VecLikeHeader)).type_tag },
                VecLikeType::Lambda,
                "lambda arena slot {slot:p} carries a non-Lambda type tag",
            );
            assert!(
                !self.vector_object_addrs.contains(&(slot as usize)),
                "lambda arena slot {slot:p} must NOT be in the vector registry",
            );
        }
        for slot in self.macro_arena.collect_allocated_slots() {
            assert_eq!(
                unsafe { (*(slot as *const VecLikeHeader)).type_tag },
                VecLikeType::Macro,
                "macro arena slot {slot:p} carries a non-Macro type tag",
            );
            assert!(
                !self.vector_object_addrs.contains(&(slot as usize)),
                "macro arena slot {slot:p} must NOT be in the vector registry",
            );
        }
        // Record slots carry the Record or WindowConfiguration tag (same
        // `RecordObj`, distinct pseudovector type) and never leak into the
        // vector registry. Native FontObj values use the residual boxed path.
        for slot in self.record_arena.collect_allocated_slots() {
            let tag = unsafe { (*(slot as *const VecLikeHeader)).type_tag };
            assert!(
                matches!(tag, VecLikeType::Record | VecLikeType::WindowConfiguration),
                "record arena slot {slot:p} carries an unrelated tag ({tag:?})",
            );
            assert!(
                !self.vector_object_addrs.contains(&(slot as usize)),
                "record arena slot {slot:p} must NOT be in the vector registry",
            );
        }
        for slot in self.marker_arena.collect_allocated_slots() {
            let tag = unsafe { (*(slot as *const VecLikeHeader)).type_tag };
            assert!(
                matches!(tag, VecLikeType::Marker),
                "marker arena slot {slot:p} carries an unrelated tag ({tag:?})",
            );
        }
        for slot in self.symbol_with_pos_arena.collect_allocated_slots() {
            assert_eq!(
                unsafe { (*(slot as *const VecLikeHeader)).type_tag },
                VecLikeType::SymbolWithPos,
                "symbol-with-pos arena slot {slot:p} carries a non-SymbolWithPos type tag",
            );
            assert!(
                !self.vector_object_addrs.contains(&(slot as usize)),
                "symbol-with-pos arena slot {slot:p} must NOT be in the vector registry",
            );
        }
    }

    #[cfg(test)]
    pub(super) fn assert_one_arena_coherent<T: PagedObject>(&self, arena: &ObjectArena<T>) {
        use std::collections::HashSet;
        // Partial chain: acyclic, flags consistent, members have free slots,
        // no retired page on the chain.
        let mut on_chain: HashSet<usize> = HashSet::new();
        let mut cursor = arena.partial_head;
        while cursor != PAGE_NONE {
            assert!(
                on_chain.insert(cursor),
                "partial chain cycle at page {cursor}"
            );
            let page = &arena.pages[cursor];
            assert!(
                page.on_partial,
                "chained page {cursor} not flagged on_partial"
            );
            assert!(!page.retired, "retired page {cursor} on the partial chain");
            assert_ne!(
                page.free_head, PAGE_NONE,
                "chained page {cursor} has an empty free list",
            );
            cursor = page.next_partial;
        }
        for (page_index, page) in arena.pages.iter().enumerate() {
            assert_eq!(
                arena.page_index_by_base.get(&page.base_addr()),
                Some(&page_index),
                "page-base registry mismatch for {} page {page_index} (retired \
                 pages must STAY registered)",
                T::CLASS,
            );
            assert_eq!(
                page.on_partial,
                on_chain.contains(&page_index),
                "page {page_index} on_partial flag disagrees with the chain",
            );
            if page.retired {
                // Retirement invariants: full, no free slots, off the chain.
                assert_eq!(
                    page.allocated,
                    ObjectPage::<T>::SLOTS,
                    "retired {} page {page_index} is not full",
                    T::CLASS,
                );
                assert_eq!(page.free_head, PAGE_NONE, "retired page with free slots");
                assert!(!page.on_partial, "retired page on the partial chain");
            }
            // Occupancy == bitmap popcount; every allocated slot is
            // bump-reached, answers OWNED via the page-span oracle, and is
            // NOT in the residual addr-set.
            let mut popcount = 0usize;
            for word_index in 0..ObjectPage::<T>::ALLOC_WORDS {
                let mut bits = page.alloc_bits[word_index];
                popcount += bits.count_ones() as usize;
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let index = word_index * usize::BITS as usize + bit;
                    assert!(
                        index < page.next_index,
                        "allocated bit beyond the bump cursor",
                    );
                    let addr = page.slot_ptr(index) as usize;
                    assert!(
                        arena.owns(addr as *const u8),
                        "allocated {} slot {addr:#x} not owned by the page-span oracle",
                        T::CLASS,
                    );
                    assert!(
                        !self.non_cons_object_addrs.contains(&addr),
                        "{} arena slot {addr:#x} must NOT be in non_cons_object_addrs",
                        T::CLASS,
                    );
                }
            }
            assert_eq!(
                page.allocated, popcount,
                "page {page_index} occupancy != bitmap popcount",
            );
            // Free list: entries bump-reached, bit-clear, duplicate-free,
            // NOT owned per the oracle, and together with the allocated
            // slots exactly cover the bumped span.
            let mut free_seen: HashSet<usize> = HashSet::new();
            let mut fcursor = page.free_head;
            while fcursor != PAGE_NONE {
                assert!(
                    fcursor < page.next_index,
                    "free slot beyond the bump cursor"
                );
                assert!(
                    !page.is_allocated(fcursor),
                    "free-listed slot {fcursor} has its alloc bit set",
                );
                assert!(
                    !arena.owns(page.slot_ptr(fcursor) as *const u8),
                    "freed slot must answer NOT-owned (alloc-bit oracle)",
                );
                assert!(
                    free_seen.insert(fcursor),
                    "free-list cycle/duplicate at slot {fcursor}",
                );
                fcursor = unsafe { page.free_link_ptr(fcursor).read() };
            }
            assert_eq!(
                page.allocated + free_seen.len(),
                page.next_index,
                "page {page_index}: occupancy + free-list length != bump cursor",
            );
            if page.free_head != PAGE_NONE {
                assert!(
                    page.on_partial,
                    "page {page_index} has free slots but is off the partial chain",
                );
            }
        }
    }
}
