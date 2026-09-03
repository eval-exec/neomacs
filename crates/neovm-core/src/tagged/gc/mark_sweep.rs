//! Stop-the-world collection: begin_collection, dump promotion and blackening, root seeding of mapped children, and the mark-sweep cycle.
//!
//! Moved out of `gc.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

impl TaggedHeap {
    /// Run a full mark-sweep garbage collection.
    ///
    /// `roots` must yield every reachable `TaggedValue`.
    pub fn collect(&mut self, roots: impl Iterator<Item = TaggedValue>) {
        self.collect_exact(roots);
    }

    /// Run a full mark-sweep collection using only the explicit roots provided.
    pub fn collect_exact(&mut self, roots: impl Iterator<Item = TaggedValue>) {
        self.begin_collection();
        for root in roots {
            self.seed_root(root);
        }
        self.complete_collection();
    }

    pub(crate) fn begin_collection(&mut self) {
        // (Pre-mark verification removed — unmarked objects may have stale data
        //  that will be swept. Only post-mark verification is meaningful.)

        // A mark must never start while a deferred sweep is still draining: the
        // sweep reads the mark bits the parity flip below would re-interpret.
        // The driver finishes any in-flight sweep before getting here
        // (`gc_collect_from_current_roots` checks `sweep_in_progress` on every
        // path). HARD assert (not debug): under parity marks, flipping while
        // the detached sweep list is still draining would make every dead
        // object read as marked — the remainder of the sweep would relink
        // garbage as survivors (a leak), and a later cycle could trace through
        // their stale children (worse).
        assert!(
            !self.sweep_in_progress,
            "begin_collection while a deferred sweep is in progress"
        );
        // YOUNG NON-CONS PARITY FLIP (task #7 stage 2b): this single store
        // un-marks the entire young non-cons generation — everything the last
        // cycle marked has bit == old parity, which the new parity reads as
        // unmarked. It replaces the O(objects) `all_objects` pointer-chase
        // clear walk that measured ~98% of the clear phase. The flip lives in
        // `begin_collection` ONLY (`concurrent_begin` delegates here; no other
        // entry point may flip).
        self.mark_parity = !self.mark_parity;

        let clear_t0 = crate::host_time::Instant::now();
        // The first partition cycle runs a NORMAL full collection (so it traces
        // everything and frees load transients); promotion + blackening happen
        // at the end of that cycle (`complete_collection`). Only once
        // `dump_blackened` is set do the partitioned skips apply.
        let partitioned = self.partition_dump && self.dump_blackened;

        // -- Clear marks (heap cons) --
        for block in &mut self.cons_blocks {
            block.clear_marks();
        }
        let clear_cons_done = crate::host_time::Instant::now();
        // -- Mapped (pdump) marks: permanent black region when partitioned --
        if !partitioned {
            for range in &mut self.mapped_cons_ranges {
                range.clear_marks();
            }
            for range in &mut self.mapped_float_ranges {
                range.clear_marks();
            }
            for object in &mut self.mapped_veclike_objects {
                object.marked = false;
            }
            for object in &mut self.mapped_string_objects {
                object.marked = false;
            }
        }
        let clear_mapped_done = crate::host_time::Instant::now();
        // -- YOUNG non-cons (heap) marks: NO WALK. The parity flip at the top
        //    of this fn already un-marked the whole young `all_objects` list
        //    in O(1). The tenured old generation lives on a separate list
        //    (`tenured_objects`) whose frozen bits are never interpreted —
        //    tenured readers short-circuit on the `tenured` flag, so it stays
        //    permanently black. Before the first-cycle promotion every object
        //    is still on `all_objects` with bit == false, and the first flip
        //    (parity false -> true) reads the full preloaded world as
        //    unmarked, so that one cycle traces everything. --

        // Task #7 stage 2a/2b: the clear split (cons bitmap memset / mapped
        // resets / young non-cons segment) sized the parity mark-bit design;
        // the non-cons segment is now the flip (~0), kept as the regression
        // gauge for the removed pointer-chase walk.
        let clear_end = crate::host_time::Instant::now();
        self.last_clear_cons_us = (clear_cons_done - clear_t0).as_micros() as u64;
        self.last_clear_mapped_us = (clear_mapped_done - clear_cons_done).as_micros() as u64;
        self.last_clear_noncons_us = (clear_end - clear_mapped_done).as_micros() as u64;
        self.last_clear_us = (clear_end - clear_t0).as_micros() as u64;

        // -- Seed gray queue from roots --
        self.gray_queue.clear();
        self.marked_symbols.clear();
        self.weak_hash_tables.clear();
        self.weak_hash_tables_set.clear();
        self.mark_cons_block_cache = None;
        // New mark cycle: the per-cycle SATB pre-image dedup set must start empty
        // so each owner's full pre-image is snapshotted once for THIS cycle's
        // start-of-cycle reachability (a carried-over entry would wrongly suppress
        // the snapshot of an owner whose children differ this cycle).
        self.satb_snapshotted_owners.clear();
        // CONCURRENT STRING MARKING: same per-cycle reset for the enforced
        // in-mutator string interval pre-image dedup (`note_string_interval_preimage`).
        self.satb_string_preimage_addrs.clear();
        // Stage 2 Tier B CONCURRENT VECTOR SCAN: the per-cycle clone-on-write dedup
        // set must start empty so each vector owner is cloned+retired at most once
        // per cycle (a carried-over entry would wrongly suppress this cycle's clone).
        self.concurrent_cloned_vectors.clear();
        // OWNER-TRACKING REMEMBERED-SET PRECURSOR (`dirty_owners` /
        // `dirty_owner_bits` / `dirty_writes`): clear it HERE, at the START of the
        // cycle, on the same per-cycle lifecycle as the SATB dedup sets above —
        // NOT at end-of-collection. A carried-over entry is not merely wasteful;
        // it is an ABA hazard. An owner address recorded before this cycle can be
        // FREED by this cycle's sweep and its slot handed to a NEW same-class
        // object by the arena; the stale `dirty_owner_bits` entry would then dedup
        // (suppress) the new object's barriered write — a missed remembered-write.
        // Clearing at begin makes the tables hold only writes made SINCE this
        // cycle started, so no entry outlives the object it names into a
        // sweep+reuse. This is the exact ABA-safety argument the SATB sets rely
        // on (per-cycle; no free during mark; cleared at begin). The tables are
        // the seam for the future generational remembered set (task 06), whose
        // consumer walks them per cycle and needs no cross-cycle accumulation;
        // every reader today is a test.
        self.clear_dirty_owners();
        self.clear_dirty_writes();
        self.seed_internal_runtime_roots();
        if partitioned {
            // Re-scan dumped/tenured objects mutated to point at young heap
            // objects: those children must be kept live even though the dump and
            // the tenured old generation are black.
            self.seed_mapped_remembered();
        } else if self.partition_dump {
            if self.first_cycle_concurrent {
                // Concurrent first cycle: string intervals seed here
                // (handshake); veclike headers and cons ranges are STAGED
                // for the GC thread (`launch_concurrent_mark` moves them
                // into the job).
                self.seed_mapped_string_children();
                self.staged_mapped_veclikes = Some(
                    self.mapped_veclike_objects
                        .iter()
                        .map(|o| o.header as usize)
                        .collect(),
                );
                self.staged_mapped_cons_scan = Some(
                    self.mapped_cons_ranges
                        .iter()
                        .map(|range| (range.start as usize, range.len))
                        .collect(),
                );
            } else {
                // First partition cycle, STW (explicit garbage-collect /
                // dump-less bootstrap): keep every dump-referenced heap
                // object alive so none is swept and left dangling when the
                // image is blackened at the end of this cycle.
                self.seed_all_mapped_children();
            }
        }
    }

    /// Run once at the END of the first partition cycle (after a full
    /// trace+sweep): promote every survivor to the tenured old generation,
    /// blacken the mapped dump image, and build the initial remembered set.
    /// Thereafter both regions are permanently black and skipped each cycle.
    pub(super) fn promote_and_blacken(&mut self) {
        // 1. Promote every surviving heap object to tenured (old generation).
        //    The first partition cycle ran a full trace+sweep, so everything
        //    still in `all_objects` is alive = a permanent (the preloaded world
        //    plus whatever the session has retained). They are already marked;
        //    setting `tenured` FREEZES that bit — no later parity flip may be
        //    interpreted against it (every tenured reader short-circuits on
        //    the flag), so these objects are permanently black without ever
        //    being re-touched.
        //    Move the whole young list onto the tenured list and flag each
        //    node so the nursery (`all_objects`) starts empty; from now on only
        //    post-loadup allocations land there and get cleared/swept.
        let mut tail: *mut GcHeader = std::ptr::null_mut();
        let mut obj = self.all_objects;
        while !obj.is_null() {
            unsafe {
                (*obj).tenured = true;
                // A weak hash table being tenured becomes permanent-black and the
                // main mark will never re-touch it; record it so the weak sweep
                // keeps re-evaluating its entries every GC (GNU sweeps every weak
                // table every GC). See `permanent_weak_hash_tables`.
                if (*obj).kind == HeapObjectKind::VecLike {
                    let vptr = obj as *mut VecLikeHeader;
                    if (*vptr).type_tag == VecLikeType::HashTable {
                        let ht_ptr = vptr as *mut HashTableObj;
                        if (*ht_ptr).table.weakness.is_some()
                            && !self.permanent_weak_hash_tables_set.contains(&ht_ptr)
                        {
                            self.permanent_weak_hash_tables_set.insert(ht_ptr);
                            self.permanent_weak_hash_tables.push(ht_ptr);
                        }
                    }
                }
                tail = obj;
                obj = (*obj).next;
            }
        }
        if !tail.is_null() {
            // Splice: [all_objects .. tail] -> front of tenured_objects.
            unsafe {
                (*tail).next = self.tenured_objects;
            }
            self.tenured_objects = self.all_objects;
            self.all_objects = std::ptr::null_mut();
        }
        // 1b. PROMOTION PAGE WALK (stage 3): page objects are on no intrusive
        //     list, so the splice above cannot tenure them — without this
        //     walk the (loadup-sized) paged survivor set would stay young and
        //     be re-seeded + re-traced + re-swept every cycle, defeating
        //     tenuring. Flip `header.tenured` on every ALLOCATED slot (the
        //     sweep has already run, so allocated ≡ survivor); the per-object
        //     header REMAINS the sole mark-path authority — no page-level
        //     flag is consulted by any mark path. No weak-table registration
        //     is needed here (the paged classes are Float/String/Vector;
        //     hash tables stay Box and ride the splice above). Then RETIRE
        //     full pages: a page whose every slot is allocated (hence, after
        //     this walk, tenured) can never free a slot again — the sweep
        //     skips it whole, the allocator never touches it, and it is
        //     freed only at heap teardown, while STAYING in the page-base
        //     registry so the ownership oracle keeps answering "owned"
        //     (`value_is_tenured` gates on ownership — see C1 on the arena
        //     doc). The criterion is deliberately occupancy==SLOTS, NOT "all
        //     allocated slots tenured": right after this walk EVERY page
        //     trivially satisfies the latter, which would retire
        //     nearly-empty pages and strand their free slots forever.
        //     Partial pages stay in rotation as MIXED pages — every later
        //     sweep re-skips their tenured slots, a perpetual per-slot
        //     branch bounded by the one-time loadup survivor set (the only
        //     population this one-shot promotion ever tenures).
        self.promote_arena_pages_and_retire_full();
        // 2. Blacken the mapped image.
        for range in &mut self.mapped_cons_ranges {
            range.mark_all();
        }
        for range in &mut self.mapped_float_ranges {
            range.mark_all();
        }
        for object in &mut self.mapped_veclike_objects {
            object.marked = true;
        }
        for object in &mut self.mapped_string_objects {
            object.marked = true;
        }
        // Mapped (pdump) weak hash tables become permanent-black here too (the
        // preloaded image ships several, e.g. `print-number-table` helpers and
        // internal caches). Like tenured weak tables, they would never be
        // re-traced and their entries would be pinned forever; register them so
        // `mark_and_sweep_weak_tables` re-evaluates them every GC.
        let mapped_weak: Vec<*mut HashTableObj> = self
            .mapped_veclike_objects
            .iter()
            .filter_map(|object| {
                let header = object.header;
                // SAFETY: `header` is a live mapped veclike for the dump's lifetime.
                unsafe {
                    if (*header).type_tag == VecLikeType::HashTable {
                        let ht_ptr = header as *mut HashTableObj;
                        if (*ht_ptr).table.weakness.is_some() {
                            return Some(ht_ptr);
                        }
                    }
                }
                None
            })
            .collect();
        for ht_ptr in mapped_weak {
            if self.permanent_weak_hash_tables_set.insert(ht_ptr) {
                self.permanent_weak_hash_tables.push(ht_ptr);
            }
        }
        // 3. Remember permanents (mapped or tenured) that point at a YOUNG
        //    heap object so its children stay live. After promotion (list
        //    splice + page walk) the only young heap objects are heap CONSES
        //    (header-less, cannot be tenured), so this scan retains exactly
        //    the permanent→cons edges. It covers page-tenured owners too —
        //    see the page walk inside `scan_permanents_for_young_children`.
        self.scan_permanents_for_young_children();
    }

    /// Stage-3 promotion page walk + retirement (see the call site in
    /// `promote_and_blacken` for the full rationale). One-shot: runs only at
    /// the single promotion of the first partition cycle.
    pub(super) fn promote_arena_pages_and_retire_full(&mut self) {
        fn walk_one<T: PagedObject>(arena: &mut ObjectArena<T>) {
            for page in &mut arena.pages {
                for word_index in 0..ObjectPage::<T>::ALLOC_WORDS {
                    let mut bits = page.alloc_bits[word_index];
                    while bits != 0 {
                        let bit = bits.trailing_zeros() as usize;
                        bits &= bits - 1;
                        let index = word_index * usize::BITS as usize + bit;
                        let slot = page.slot_ptr(index);
                        // Allocated ⇒ survivor of the just-completed sweep ⇒
                        // a permanent. Plain store: promotion is STW.
                        unsafe { (*(slot as *mut GcHeader)).tenured = true };
                    }
                }
                // RETIREMENT: FULL pages only (occupancy == slots, which
                // after the flip above implies all-tenured). A full page has
                // an empty free list and is off the partial chain by
                // construction.
                if page.allocated == ObjectPage::<T>::SLOTS {
                    debug_assert!(!page.on_partial, "full page on the partial chain");
                    debug_assert_eq!(page.free_head, PAGE_NONE);
                    page.retired = true;
                }
            }
        }
        walk_one(&mut self.float_arena);
        walk_one(&mut self.string_arena);
        walk_one(&mut self.vector_arena);
        // Bytecode is loadup-heavy: most of the population tenures at this
        // one-time promotion, so FULL-page retirement fires for real here
        // (unlike floats) — retired pages stay registered/owned (C1).
        walk_one(&mut self.bytecode_arena);
        // Lambdas/macros likewise tenure at the loadup promotion (interpreted
        // closures are loadup-heavy); their arenas retire full pages too.
        walk_one(&mut self.lambda_arena);
        walk_one(&mut self.macro_arena);
        walk_one(&mut self.record_arena);
        walk_one(&mut self.symbol_with_pos_arena);
    }

    /// Scan every permanent object (mapped dump + tenured old gen) for edges to
    /// young heap objects and add such permanents to the remembered set. Used
    /// at promotion and re-buildable on demand; the result is seeded each cycle.
    pub(super) fn scan_permanents_for_young_children(&mut self) {
        // -- mapped vectorlike --
        let veclike: Vec<*mut VecLikeHeader> = self
            .mapped_veclike_objects
            .iter()
            .map(|o| o.header)
            .collect();
        for ptr in veclike {
            if self
                .collect_veclike_children(ptr)
                .iter()
                .any(|c| self.is_heap_young(*c))
            {
                let value = unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) };
                self.mapped_remembered.insert(value.bits());
            }
        }
        // -- mapped conses --
        let cons_ranges: Vec<(*mut ConsCell, usize)> = self
            .mapped_cons_ranges
            .iter()
            .map(|range| (range.start, range.len))
            .collect();
        for (start, len) in cons_ranges {
            for i in 0..len {
                let cell = unsafe { start.add(i) };
                let car = unsafe { (*cell).load_car() };
                let cdr = unsafe { (*cell).load_cdr() };
                if self.is_heap_young(car) || self.is_heap_young(cdr) {
                    let value = unsafe { TaggedValue::from_cons_ptr(cell) };
                    self.mapped_remembered.insert(value.bits());
                }
            }
        }
        // -- mapped strings (text-prop intervals) --
        let strings: Vec<*mut StringObj> =
            self.mapped_string_objects.iter().map(|o| o.ptr).collect();
        for ptr in strings {
            let mut roots: Vec<TaggedValue> = Vec::new();
            let intervals = unsafe { (*ptr).data.intervals() };
            if !intervals.is_empty() {
                intervals.for_each_root(|root| roots.push(root));
            }
            if roots.iter().any(|r| self.is_heap_young(*r)) {
                let value = unsafe { TaggedValue::from_string_ptr(ptr) };
                self.mapped_remembered.insert(value.bits());
            }
        }
        // -- tenured heap objects (old generation) --
        let tenured: Vec<*mut GcHeader> = {
            let mut out = Vec::new();
            let mut obj = self.tenured_objects;
            while !obj.is_null() {
                unsafe {
                    out.push(obj);
                    obj = (*obj).next;
                }
            }
            out
        };
        for header in tenured {
            self.remember_tenured_owner_if_young_children(header);
        }
        // -- PAGE-TENURED objects (stage 3): the arenas' tenured slots are on
        //    no intrusive list, so the walk above never sees them. They MUST
        //    be scanned here: heap CONSES never tenure, so a page-tenured
        //    vector whose element (or string whose interval plist) references
        //    a cons has a YOUNG child RIGHT AT promotion — the design note
        //    "page-tenured objects have no young children at promotion" is
        //    false for exactly this cons-child case, and skipping the walk
        //    would sweep such a cons on the next cycle while its
        //    permanently-black owner (never re-traced) still points at it: a
        //    UAF (regression-tested by
        //    tenured_page_vector_keeps_young_cons_child_alive). ONGOING
        //    (post-promotion) edges are separately caught by the write
        //    barrier: `value_is_tenured` answers through the page-span
        //    oracle — retired pages included — so `record_heap_write` keeps
        //    remembering mutated page-tenured owners. --
        for header in self.collect_tenured_page_slot_headers() {
            self.remember_tenured_owner_if_young_children(header);
        }
    }

    /// Insert a tenured (list or page) owner into the dump remembered set if
    /// any of its direct heap children is YOUNG. Floats have no children.
    pub(super) fn remember_tenured_owner_if_young_children(&mut self, header: *mut GcHeader) {
        let kind = unsafe { (*header).kind };
        let has_young = match kind {
            HeapObjectKind::VecLike | HeapObjectKind::String => self
                .heap_object_children(header)
                .iter()
                .any(|c| self.is_heap_young(*c)),
            HeapObjectKind::Float => false,
        };
        if has_young {
            let value = match kind {
                HeapObjectKind::VecLike => unsafe {
                    TaggedValue::from_veclike_ptr(header as *const VecLikeHeader)
                },
                HeapObjectKind::String => unsafe {
                    TaggedValue::from_string_ptr(header as *mut StringObj)
                },
                HeapObjectKind::Float => return,
            };
            self.mapped_remembered.insert(value.bits());
        }
    }

    /// True if `value` is a YOUNG heap object: a real heap allocation that is
    /// neither mapped (dump) nor tenured (old gen) — i.e. it participates in the
    /// normal clear/mark/sweep each cycle. Heap cons cells are always young
    /// (header-less, cannot be tenured).
    pub(super) fn is_heap_young(&self, value: TaggedValue) -> bool {
        if !value.is_heap_object() || self.owner_is_mapped(value) {
            return false;
        }
        if value.is_cons() {
            return true; // heap cons: header-less, cannot be tenured
        }
        // Non-cons: young iff heap-OWNED and not tenured. Static/untracked
        // objects (e.g. Subrs) are permanently live, never young.
        match Self::value_heap_addr(value) {
            Some(addr) => {
                self.owns_heap_value_object(value, addr)
                    && !unsafe { (*(addr as *const GcHeader)).tenured }
            }
            None => false,
        }
    }

    /// True if `value` is a tenured (old-gen) heap non-cons object.
    ///
    /// Gates on OWNERSHIP first, so the page-span oracle must keep answering
    /// "owned" for RETIRED pages (C1): a retired-page tenured object that
    /// answered not-owned here would read as neither mapped nor tenured, the
    /// write barrier (`record_heap_write`) would skip its first
    /// post-retirement tenured→young edge, the child would never be re-seeded
    /// (`seed_mapped_remembered`) and would be swept while live.
    pub(super) fn value_is_tenured(&self, value: TaggedValue) -> bool {
        if value.is_cons() {
            return false;
        }
        let Some(addr) = Self::value_heap_addr(value) else {
            return false;
        };
        if !self.owns_heap_value_object(value, addr) {
            return false; // mapped, not a tenured heap object
        }
        unsafe { (*(addr as *const GcHeader)).tenured }
    }

    /// First-cycle only: seed the heap children of EVERY mapped object so they
    /// survive the cycle's sweep. Dumped objects are never freed, so a heap
    /// object referenced only by an (otherwise unreachable) dumped object must
    /// still be kept — otherwise it would be swept and the dumped object would
    /// be left holding a dangling pointer once the image is blackened.
    pub(super) fn seed_all_mapped_children(&mut self) {
        self.seed_mapped_veclike_and_string_children();
        let cons_ranges: Vec<(*mut ConsCell, usize)> = self
            .mapped_cons_ranges
            .iter()
            .map(|range| (range.start, range.len))
            .collect();
        for (start, len) in cons_ranges {
            for i in 0..len {
                let cell = unsafe { start.add(i) };
                let car = unsafe { (*cell).load_car() };
                let cdr = unsafe { (*cell).load_cdr() };
                self.mark_or_push_child(car, "first-cycle-mapped-cons-car");
                self.mark_or_push_child(cdr, "first-cycle-mapped-cons-cdr");
            }
        }
    }

    /// The veclike + string-interval half of [`Self::seed_all_mapped_children`]
    /// (STW path).
    pub(super) fn seed_mapped_veclike_and_string_children(&mut self) {
        let veclike: Vec<*mut VecLikeHeader> = self
            .mapped_veclike_objects
            .iter()
            .map(|o| o.header)
            .collect();
        for ptr in veclike {
            unsafe { self.trace_veclike(ptr) };
        }
        self.seed_mapped_string_children();
    }

    /// String-interval children only: interval trees carry no concurrent-read
    /// guarantee, so the concurrent first cycle keeps THIS part in the start
    /// handshake while staging veclikes (atomic-read slots) and cons ranges
    /// for the GC thread.
    pub(super) fn seed_mapped_string_children(&mut self) {
        let strings: Vec<*mut StringObj> =
            self.mapped_string_objects.iter().map(|o| o.ptr).collect();
        for ptr in strings {
            let mut roots: Vec<TaggedValue> = Vec::new();
            let intervals = unsafe { (*ptr).data.intervals() };
            if !intervals.is_empty() {
                intervals.for_each_root(|root| roots.push(root));
            }
            for root in roots {
                self.mark_or_push_child(root, "first-cycle-mapped-string-interval");
            }
        }
    }

    /// Seed the gray queue with the heap children of every dumped object that
    /// has been mutated since load (the dump remembered set). Because the dump
    /// is black, `mark_value` would otherwise never re-trace these, so we
    /// enqueue their children directly. Mapped children are already black and
    /// are skipped when popped; only heap children get marked.
    pub(super) fn seed_mapped_remembered(&mut self) {
        // Handshake instrumentation: owners re-scanned + wall cost, routed to
        // the start/termination slot by the caller. The remembered set is
        // append-only (never cleared), so this count is the monotonic-growth
        // probe as well.
        let seed_t0 = crate::host_time::Instant::now();
        self.last_remembered_seed_roots = self.mapped_remembered.len();
        self.handshake.probe_mapped_remembered = self.mapped_remembered.len();
        if self.mapped_remembered.is_empty() {
            self.last_remembered_seed_us = 0;
            return;
        }
        let owners: Vec<TaggedValue> = self
            .mapped_remembered
            .iter()
            .map(|&bits| TaggedValue(bits))
            .collect();
        for owner in owners {
            self.push_value_children_to_gray(owner, "remembered-dump-child");
        }
        self.last_remembered_seed_us = seed_t0.elapsed().as_micros() as u64;
    }

    /// Push every heap child of `owner` onto the gray queue (re-trace its
    /// outgoing references). Unlike `mark_value`, this does NOT consult the
    /// owner's own mark bit, so it re-examines an already-black owner's slots —
    /// exactly what the incremental-update barrier and the dump remembered set
    /// both need. Mirrors `trace_veclike`/cons/string child enumeration.
    pub(super) fn push_value_children_to_gray(&mut self, owner: TaggedValue, origin: &'static str) {
        if owner.is_cons() {
            let ptr = owner.xcons_ptr();
            let car = unsafe { (*ptr).load_car() };
            let cdr = unsafe { (*ptr).load_cdr() };
            self.mark_or_push_child(car, origin);
            self.mark_or_push_child(cdr, origin);
        } else if owner.is_veclike() {
            if let Some(ptr) = owner.as_veclike_ptr() {
                // A dumped/tenured WEAK hash table is permanent-black, so the main
                // mark never re-runs `trace_veclike` on it and it would otherwise
                // never re-register for the weak sweep. Register it here (the
                // remembered-set / SATB / permanent scan is the ONLY path that
                // reaches such a table) and push only its NON-weak children
                // (custom test/hash closures) strongly. Its weak keys/values are
                // deliberately NOT traced here — `mark_and_sweep_weak_tables`
                // (which runs at every mark termination, before
                // `verify_dump_partition`) decides per-entry survival against the
                // current marks and physically removes the dead entries, so the
                // verifier never sees an unmarked weak child. This mirrors GNU's
                // `mark_object` PVEC_HASH_TABLE (alloc.c): weak tables register
                // themselves and do NOT mark their contents.
                if let Some(weak_children) = self.register_weak_hash_table_for_sweep(ptr) {
                    for child in weak_children {
                        self.mark_or_push_child(child, origin);
                    }
                } else {
                    // STRONG enumeration for every other veclike (and non-weak
                    // hash tables): the remembered-set / SATB paths and the
                    // dump-partition verifier require every heap child of a
                    // permanent owner to be marked, or it is swept while still
                    // referenced (UAF).
                    for child in self.collect_veclike_children(ptr as *mut VecLikeHeader) {
                        self.mark_or_push_child(child, origin);
                    }
                }
            }
        } else if owner.is_string()
            && let Some(ptr) = owner.as_string_ptr()
        {
            let intervals = unsafe { (*ptr).data.intervals() };
            if !intervals.is_empty() {
                intervals.for_each_root(|root| {
                    self.mark_or_push_child(root, origin);
                });
            }
        }
        // Floats have no heap children.
    }

    /// Is `value` currently marked? Covers heap and mapped objects of every
    /// category. Used only by the dump-partition verifier.
    pub(super) fn is_value_marked(&self, value: TaggedValue) -> bool {
        if let crate::tagged::value::ValueKind::Symbol(id) = value.kind() {
            return crate::emacs_core::intern::is_canonical_id(id)
                || self.marked_symbols.contains(id);
        }
        if value.is_cons() {
            let ptr = value.xcons_ptr();
            if ConsBlock::ptr_is_cell_aligned(ptr) {
                let base = ConsBlock::block_base_for_ptr(ptr);
                if let Some(&idx) = self.cons_block_index_by_base.get(&base) {
                    return self.cons_blocks[idx].is_marked_ptr(ptr);
                }
            }
            return self
                .mapped_cons_ranges
                .iter()
                .find(|range| range.contains_ptr(ptr))
                .map(|range| range.is_marked_ptr(ptr))
                .unwrap_or(false);
        }
        let Some(addr) = Self::value_heap_addr(value) else {
            return true;
        };
        if self.owns_heap_value_object(value, addr) {
            // Heap-owned non-cons: every such object starts with a `GcHeader`
            // (`StringObj`/`FloatObj` headers and `VecLikeHeader.gc` are all
            // at offset 0). TENURED SHORT-CIRCUIT before the bit read:
            // `promote_and_blacken` never removes tenured objects from
            // `non_cons_object_addrs`, and a tenured bit froze at promotion,
            // so interpreting it against the current parity would read
            // "unmarked" on every other cycle — spurious partition/tricolor
            // verifier panics and needless old-gen concern. Tenured ≡ marked.
            let header = addr as *const GcHeader;
            if unsafe { (*header).tenured } {
                return true;
            }
            return unsafe { (*header).is_marked_at(self.mark_parity) };
        }
        // A non-cons object that is neither heap-owned nor mapped is a static,
        // never-swept runtime object (e.g. a `Subr`) — permanently live, so
        // treat it as marked (`unwrap_or(true)`). This relies on the dump
        // partition keeping every dump-referenced heap object live, so a
        // not-owned/not-mapped pointer is never a dangling reference.
        if value.is_string() {
            return self
                .mapped_string_index_by_addr
                .get(&addr)
                .map(|&i| self.mapped_string_objects[i].marked)
                .unwrap_or(true);
        }
        if value.is_float() {
            let ptr = addr as *const FloatObj;
            return self
                .mapped_float_ranges
                .iter()
                .find(|range| range.contains_ptr(ptr))
                .map(|range| range.is_marked_ptr(ptr))
                .unwrap_or(true);
        }
        if value.is_veclike() {
            return self
                .mapped_veclike_index_by_addr
                .get(&addr)
                .map(|&i| self.mapped_veclike_objects[i].marked)
                .unwrap_or(true);
        }
        true
    }

    /// Verification gate for the dump partition (env `NEOVM_GC_VERIFY_PARTITION`).
    /// After the partitioned mark, every direct heap child of every dumped
    /// object MUST already be marked — otherwise the write barrier missed a
    /// dumped→heap mutation and the partition is about to free a live object.
    /// Panics on the first violation. Expensive (full dump scan); verification
    /// runs only.
    pub(super) fn verify_dump_partition(&mut self) {
        let mut violations: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut sample: Option<usize> = None;
        let mut record = |owner: &str, child: TaggedValue| {
            let child_kind = if child.is_cons() {
                "cons".to_string()
            } else if child.is_string() {
                "string".to_string()
            } else if child.is_float() {
                "float".to_string()
            } else if child.is_veclike() {
                format!("{:?}", child.veclike_type())
            } else {
                "other".to_string()
            };
            *violations
                .entry(format!("{owner} -> {child_kind}"))
                .or_insert(0) += 1;
            sample.get_or_insert(child.0);
        };

        // Mapped veclike objects (char-tables etc.), grouped by owner type.
        let veclike: Vec<(*mut VecLikeHeader, VecLikeType)> = self
            .mapped_veclike_objects
            .iter()
            .map(|o| (o.header, unsafe { (*o.header).type_tag }))
            .collect();
        for (ptr, ty) in veclike {
            let owner = format!("{ty:?}");
            for child in self.collect_veclike_children(ptr) {
                if child.is_heap_object() && !self.is_value_marked(child) {
                    record(&owner, child);
                }
            }
        }
        // Mapped conses.
        let cons_ranges: Vec<(*mut ConsCell, usize)> = self
            .mapped_cons_ranges
            .iter()
            .map(|range| (range.start, range.len))
            .collect();
        for (start, len) in cons_ranges {
            for i in 0..len {
                let cell = unsafe { start.add(i) };
                for child in [unsafe { (*cell).load_car() }, unsafe { (*cell).load_cdr() }] {
                    if child.is_heap_object() && !self.is_value_marked(child) {
                        record("Cons", child);
                    }
                }
            }
        }
        // Mapped strings (text-property intervals).
        let strings: Vec<*mut StringObj> =
            self.mapped_string_objects.iter().map(|o| o.ptr).collect();
        for ptr in strings {
            let mut roots: Vec<TaggedValue> = Vec::new();
            let intervals = unsafe { (*ptr).data.intervals() };
            if !intervals.is_empty() {
                intervals.for_each_root(|root| roots.push(root));
            }
            for child in roots {
                if child.is_heap_object() && !self.is_value_marked(child) {
                    record("String", child);
                }
            }
        }
        // Tenured heap objects (old generation): their direct heap children
        // must also be marked, or a survival-promoted permanent mutated to
        // point at a young object would free it.
        let tenured: Vec<*mut GcHeader> = {
            let mut out = Vec::new();
            let mut obj = self.tenured_objects;
            while !obj.is_null() {
                unsafe {
                    out.push(obj);
                    obj = (*obj).next;
                }
            }
            out
        };
        for header in tenured {
            let kind = unsafe { (*header).kind };
            let owner = format!("tenured:{kind:?}");
            let children: Vec<TaggedValue> = self.heap_object_children(header);
            for child in children {
                if child.is_heap_object() && !self.is_value_marked(child) {
                    record(&owner, child);
                }
            }
        }
        // TENURED PAGE SLOTS (stage 3): page-tenured strings/vectors are on
        // NO intrusive list — without this walk they would be INVISIBLE to
        // this detector and a missed tenured→young barrier edge on them
        // would pass verification straight into a UAF. Allocated-bit-first;
        // clear-bit slot bytes are garbage.
        for header in self.collect_tenured_page_slot_headers() {
            let kind = unsafe { (*header).kind };
            let owner = format!("tenured-page:{kind:?}");
            for child in self.heap_object_children(header) {
                if child.is_heap_object() && !self.is_value_marked(child) {
                    record(&owner, child);
                }
            }
        }

        if !violations.is_empty() {
            let total: usize = violations.values().sum();
            eprintln!("DUMP_PARTITION_VIOLATIONS total={total}");
            for (k, n) in &violations {
                eprintln!("  {n:>6}  {k}");
            }
            panic!(
                "dump-partition verification: {total} unmarked heap children of mapped objects \
                 (sample value={:#x}) — write barrier missed dumped->heap mutations (UAF risk). \
                 See DUMP_PARTITION_VIOLATIONS above.",
                sample.unwrap_or(0)
            );
        }
    }

    /// Verification gate for incremental marking (env `NEOVM_GC_VERIFY_PARTITION`,
    /// incremental builds). Complements `verify_dump_partition`, which covers
    /// mapped + tenured owners: this checks the remaining black objects —
    /// YOUNG non-cons (`all_objects`) and every marked heap CONS — for the
    /// strong tri-color invariant (no black object points to a white object).
    /// A violation means the incremental-update barrier missed a black->white
    /// edge created by the mutator during marking (a UAF about to happen).
    /// Panics on the first batch of violations. Expensive; verification only.
    pub(super) fn verify_incremental_tricolor(&mut self) {
        let mut violations: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut sample: Option<usize> = None;

        // -- Young non-cons objects that are marked (black). `all_objects` is
        //    young-only (tenured objects live on `tenured_objects`), so the
        //    parity interpretation applies to every node. --
        let young: Vec<*mut GcHeader> = {
            let mut out = Vec::new();
            let parity = self.mark_parity;
            let mut obj = self.all_objects;
            while !obj.is_null() {
                unsafe {
                    if (*obj).is_marked_at(parity) {
                        out.push(obj);
                    }
                    obj = (*obj).next;
                }
            }
            out
        };
        for header in young {
            let kind = unsafe { (*header).kind };
            let children: Vec<TaggedValue> = self.heap_object_children(header);
            let owner = format!("young:{kind:?}");
            for child in children {
                if child.is_heap_object() && !self.is_value_marked(child) {
                    *violations.entry(owner.clone()).or_insert(0) += 1;
                    sample.get_or_insert(child.0);
                }
            }
        }

        // -- YOUNG PAGE SLOTS that are marked (black), stage 3: page
        //    strings/vectors/floats are on NO intrusive list — without this
        //    walk a black page string/vector pointing at a white object would
        //    be INVISIBLE to this detector (a live hole in the black→white
        //    scan). Allocated-bit-first; tenured slots are covered by
        //    `verify_dump_partition`'s tenured-page walk. --
        for header in self.collect_young_marked_page_slot_headers() {
            let kind = unsafe { (*header).kind };
            let owner = format!("young-page:{kind:?}");
            for child in self.heap_object_children(header) {
                if child.is_heap_object() && !self.is_value_marked(child) {
                    *violations.entry(owner.clone()).or_insert(0) += 1;
                    sample.get_or_insert(child.0);
                }
            }
        }

        // -- Every marked heap cons cell: car/cdr must be marked. --
        let blocks: Vec<(*mut ConsCell, usize)> = self
            .cons_blocks
            .iter()
            .map(|b| (b.cells_ptr(), b.next_index as usize))
            .collect();
        for (cells, count) in blocks {
            for i in 0..count {
                let cell = unsafe { cells.add(i) };
                if !self.is_value_marked(unsafe { TaggedValue::from_cons_ptr(cell) }) {
                    continue;
                }
                for child in [unsafe { (*cell).load_car() }, unsafe { (*cell).load_cdr() }] {
                    if child.is_heap_object() && !self.is_value_marked(child) {
                        *violations.entry("young:Cons".to_string()).or_insert(0) += 1;
                        sample.get_or_insert(child.0);
                    }
                }
            }
        }

        if !violations.is_empty() {
            let total: usize = violations.values().sum();
            eprintln!("INCREMENTAL_TRICOLOR_VIOLATIONS total={total}");
            for (k, n) in &violations {
                eprintln!("  {n:>6}  {k}");
            }
            panic!(
                "incremental tri-color verification: {total} black->white edges \
                 (sample value={:#x}) — the incremental-update barrier missed a mutation \
                 (UAF risk). See INCREMENTAL_TRICOLOR_VIOLATIONS above.",
                sample.unwrap_or(0)
            );
        }
    }

    /// Direct heap children of any owned non-cons object by header, for the
    /// verifiers and the promotion-time permanents scan: veclike slots,
    /// string text-property interval roots, floats none.
    pub(super) fn heap_object_children(&self, header: *mut GcHeader) -> Vec<TaggedValue> {
        match unsafe { (*header).kind } {
            HeapObjectKind::VecLike => self.collect_veclike_children(header as *mut VecLikeHeader),
            HeapObjectKind::String => {
                let mut roots = Vec::new();
                let intervals = unsafe { (*(header as *const StringObj)).data.intervals() };
                if !intervals.is_empty() {
                    intervals.for_each_root(|root| roots.push(root));
                }
                roots
            }
            HeapObjectKind::Float => Vec::new(),
        }
    }

    /// Every allocated page slot (all class arenas) whose header is
    /// TENURED, as `GcHeader` pointers. Allocated-bit-first walk.
    pub(super) fn collect_tenured_page_slot_headers(&self) -> Vec<*mut GcHeader> {
        let mut out: Vec<*mut GcHeader> = Vec::new();
        let mut push = |header: *mut GcHeader| {
            if unsafe { (*header).tenured } {
                out.push(header);
            }
        };
        for slot in self.float_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.string_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.vector_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.bytecode_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.lambda_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.macro_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.record_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.symbol_with_pos_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        out
    }

    /// Every allocated page slot (all class arenas) that is YOUNG and
    /// MARKED at the current parity (black), as `GcHeader` pointers.
    pub(super) fn collect_young_marked_page_slot_headers(&self) -> Vec<*mut GcHeader> {
        let parity = self.mark_parity;
        let mut out: Vec<*mut GcHeader> = Vec::new();
        let mut push = |header: *mut GcHeader| unsafe {
            if !(*header).tenured && (*header).is_marked_at(parity) {
                out.push(header);
            }
        };
        for slot in self.float_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.string_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.vector_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.bytecode_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.lambda_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.macro_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.record_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        for slot in self.symbol_with_pos_arena.collect_allocated_slots() {
            push(slot as *mut GcHeader);
        }
        out
    }

    /// If `ptr` is a WEAK hash table, register it for this cycle's weak sweep
    /// (deduplicated) and return its NON-weak children — the custom test/hash
    /// closures from `define-hash-table-test`, which must be traced strongly so
    /// they outlive the table. Returns `None` for non-weak tables and every
    /// other veclike, signalling the caller to fall back to the normal strong
    /// child enumeration.
    ///
    /// This is the bridge that lets a dumped/tenured weak table — which the main
    /// mark never re-touches because it is permanent-black — still be swept every
    /// full collection, matching GNU (whose non-generational mark re-encounters
    /// every live weak table every GC and rebuilds `weak_hash_tables`).
    pub(super) fn register_weak_hash_table_for_sweep(
        &mut self,
        ptr: *const VecLikeHeader,
    ) -> Option<Vec<TaggedValue>> {
        let ht_ptr = ptr as *mut HashTableObj;
        // SAFETY: caller verified `ptr` is a live veclike; the heap is owned
        // exclusively during marking. Reading the immutable weakness / closure
        // fields is race-free.
        let (is_weak, user_cmp, user_hash) = unsafe {
            if (*ptr).type_tag != VecLikeType::HashTable {
                return None;
            }
            let ht = &(*ht_ptr).table;
            (
                ht.weakness.is_some(),
                ht.user_cmp_function,
                ht.user_hash_function,
            )
        };
        if !is_weak {
            return None;
        }
        if self.weak_hash_tables_set.insert(ht_ptr) {
            self.weak_hash_tables.push(ht_ptr);
        }
        let mut nonweak = Vec::new();
        if let Some(f) = user_cmp {
            nonweak.push(f);
        }
        if let Some(f) = user_hash {
            nonweak.push(f);
        }
        Some(nonweak)
    }

    /// Direct children of a mapped vectorlike object (read-only) for the verifier.
    pub(super) fn collect_veclike_children(&self, ptr: *mut VecLikeHeader) -> Vec<TaggedValue> {
        let mut out = Vec::new();
        unsafe {
            match (*ptr).type_tag {
                VecLikeType::Vector => {
                    out.extend((*(ptr as *const VectorObj)).data.iter().copied());
                }
                VecLikeType::Record | VecLikeType::WindowConfiguration => {
                    out.extend((*(ptr as *const RecordObj)).data.iter().copied());
                }
                VecLikeType::Font => {
                    let font = &(*(ptr as *const FontObj)).data;
                    out.extend(font.fields.iter().copied());
                    out.push(font.capability);
                }
                VecLikeType::CharTable => {
                    let o = &*(ptr as *const CharTableObj);
                    out.extend([o.defalt, o.parent, o.purpose, o.ascii]);
                    out.extend(o.contents.iter().copied());
                    out.extend(o.extras.iter().copied());
                }
                VecLikeType::SubCharTable => {
                    out.extend((*(ptr as *const SubCharTableObj)).contents.iter().copied());
                }
                VecLikeType::Obarray => {
                    out.extend((*(ptr as *const ObarrayObj)).buckets.iter().copied());
                }
                VecLikeType::Lambda | VecLikeType::Macro => {
                    out.extend((*(ptr as *const LambdaObj)).data.iter().copied());
                }
                VecLikeType::HashTable => {
                    let ht = &(*(ptr as *const HashTableObj)).table;
                    if let Some(pending) = ht.data.pending_entries() {
                        // Un-hydrated dump table (see `trace_veclike`).
                        for (_, value, snapshot) in pending {
                            out.push(*value);
                            if let Some(snapshot) = snapshot {
                                out.push(*snapshot);
                            }
                        }
                    }
                    out.extend(ht.data.values().copied());
                    out.extend(ht.key_snapshots().copied());
                    // Custom test/hash closures (`define-hash-table-test`) live
                    // ONLY in these fields and are traced by `trace_veclike`; keep
                    // the two enumerations in sync so the remembered/SATB strong-
                    // trace (which uses this) and the dump-partition verifier both
                    // cover them — otherwise a dumped/tenured custom-test table's
                    // closures are swept while the table still calls them (UAF).
                    if let Some(f) = ht.user_cmp_function {
                        out.push(f);
                    }
                    if let Some(f) = ht.user_hash_function {
                        out.push(f);
                    }
                }
                VecLikeType::ByteCode => {
                    let obj = ptr as *const ByteCodeObj;
                    let data = &(*obj).data;
                    // LAZY STUB LEG — keep in lockstep with the marking arm
                    // below: a stub's vectors are empty, its children live
                    // only in the PATCHED image regions. Walk those without
                    // materializing or allocating (GC context). On a stub,
                    // closure_slot_count carries the extras length.
                    if data.is_pdump_stub() {
                        crate::emacs_core::pdump::mapped_heap::for_each_stub_bytecode_child(
                            obj,
                            data.closure_slot_count,
                            |child| out.push(child),
                        );
                        return out;
                    }
                    out.push(data.arglist);
                    out.extend(data.constants.iter().copied());
                    if let Some(env) = data.env {
                        out.push(env);
                    }
                    if let Some(doc_form) = data.doc_form {
                        out.push(doc_form);
                    }
                    if let Some(interactive) = data.interactive {
                        out.push(interactive);
                    }
                    out.extend(data.extra_slots.iter().copied());
                }
                VecLikeType::Overlay => {
                    out.push((*(ptr as *const OverlayObj)).data.plist);
                }
                VecLikeType::SymbolWithPos => {
                    let o = &*(ptr as *const SymbolWithPosObj);
                    out.extend([o.sym, o.pos]);
                }
                VecLikeType::Finalizer => {
                    out.push((*(ptr as *const FinalizerObj)).function);
                }
                VecLikeType::ModuleFunction => {
                    let o = &*(ptr as *const ModuleFunctionObj);
                    out.extend([o.documentation, o.interactive_form]);
                }
                VecLikeType::Xwidget => {
                    let o = &*(ptr as *const XwidgetObj);
                    out.extend([o.plist, o.type_, o.buffer, o.title, o.script_callbacks]);
                }
                VecLikeType::XwidgetView => {
                    let o = &*(ptr as *const XwidgetViewObj);
                    out.extend([o.model, o.window]);
                }
                // Buffer/Window/Frame/Timer/Process/Terminal/Marker/Subr/
                // Bignum/Sqlite/UserPtr/SurfaceHandle/VideoHandle have no Value children
                // to trace (mirrors trace_veclike).
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
                | VecLikeType::VideoHandle => {}
            }
        }
        out
    }

    pub(crate) fn seed_root(&mut self, root: TaggedValue) {
        self.seed_root_with_origin(root, "explicit-root");
    }

    pub(crate) fn seed_root_with_origin(&mut self, root: TaggedValue, origin: &str) {
        if let crate::tagged::value::ValueKind::Symbol(id) = root.kind() {
            self.mark_symbol(id);
            return;
        }
        if !root.is_heap_object() {
            return;
        }
        // Stage 0: in the blackened dump partition, a root that points into the
        // dump image is already permanent-black (never cleared or swept), so it
        // needs no marking; any young child it gained through mutation is covered
        // by the dump remembered set (`seed_mapped_remembered`). Skipping these
        // avoids pushing+draining the ~450k interned-symbol value/function/plist
        // cells that still point at dumped objects on every root handshake — the
        // dominant cost of the start + termination pauses.
        if self.dump_blackened && self.owner_is_mapped(root) {
            return;
        }
        self.push_gray(root, origin);
    }

    pub(super) fn seed_internal_runtime_roots(&mut self) {
        let seed_t0 = crate::host_time::Instant::now();
        // Static subr objects are leaked process/thread runtime objects, matching
        // GNU's static `Lisp_Subr` storage. They are not swept by this heap.
        let roots: Vec<(TaggedValue, &'static str)> = self
            .buffer_registry
            .values()
            .map(|value| (*value, "buffer-registry"))
            .chain(
                self.window_registry
                    .values()
                    .map(|value| (*value, "window-registry")),
            )
            .chain(
                self.frame_registry
                    .values()
                    .map(|value| (*value, "frame-registry")),
            )
            .chain(
                self.timer_registry
                    .values()
                    .map(|value| (*value, "timer-registry")),
            )
            .chain(
                self.process_registry
                    .values()
                    .map(|value| (*value, "process-registry")),
            )
            .chain(
                self.canonical_empty_strings
                    .values()
                    .map(|value| (value, "canonical-empty-string")),
            )
            // Doomed finalizer functions not yet run must survive any cycle
            // that starts before the evaluator drains them (e.g. one queued
            // during a finalizer run, or an explicit GC before the drain).
            .chain(
                self.doomed_finalizer_functions
                    .iter()
                    .map(|value| (*value, "doomed-finalizer-function")),
            )
            .collect();

        // Handshake instrumentation: enumeration volume + wall cost, routed to
        // the start/termination slot by the caller (`concurrent_begin` /
        // `reseed_runtime_and_remembered_roots`).
        self.last_runtime_seed_roots = roots.len();
        for (value, origin) in roots {
            self.mark_or_push_child(value, origin);
        }
        self.last_runtime_seed_us = seed_t0.elapsed().as_micros() as u64;
    }

    pub(crate) fn complete_collection(&mut self) {
        let bytes_before = self.live_bytes;
        let t0 = crate::host_time::Instant::now();

        // -- Mark phase: drain the gray queue on the GC thread. This is the STW
        //    full/bootstrap path (first cycle, no-dump heaps, explicit
        //    garbage-collect); the mutator blocks until the GC thread finishes,
        //    so heap access is exclusive (no concurrency hazard here). --
        let mark_t0 = crate::host_time::Instant::now();
        self.mark_all_on_gc_thread();
        // Queue doomed finalizers before the weak sweep (GNU
        // `queue_doomed_finalizers` runs before
        // `mark_and_sweep_weak_table_contents` in `garbage_collect`): their
        // functions are re-marked so both the weak sweep and the object sweep
        // see them as live.
        self.mark_and_queue_doomed_finalizers();
        // Resolve weak hash tables now that the main mark has drained. Both the
        // sync and concurrent paths converge here with the mutator stopped, so
        // this is single-threaded and path-agnostic.
        self.mark_and_sweep_weak_tables();
        let mark_us = mark_t0.elapsed().as_micros() as u64;

        // The mark has drained and the sweep has not started: the one moment
        // where "marked" and "owned" must agree. A marked object that no arena
        // or intrusive list owns is a root that pointed at freed memory.
        #[cfg(debug_assertions)]
        if verify_marked_objects_enabled() {
            let problems = self.verify_marked_objects_owned();
            assert_eq!(
                problems, 0,
                "post-mark ownership verification found {problems} problem(s):                  a root pointed at memory no arena owns (see the GC VERIFY                  lines above)"
            );
        }

        self.finalize_collection(mark_us, bytes_before, t0);
    }

    /// Queue the functions of finalizer objects this cycle found unreachable —
    /// GNU `queue_doomed_finalizers` + `mark_finalizers` (alloc.c). Must run
    /// at BOTH mark terminations (`complete_collection` and
    /// `incremental_finish`), after the main mark drains and before the weak
    /// sweep. A doomed finalizer leaves the registry and is swept normally;
    /// only its `function` is queued, re-marked transitively (same marking
    /// helpers as the weak-table fixpoint) so the imminent sweep keeps
    /// everything it needs. Still-marked finalizers stay registered.
    pub(super) fn mark_and_queue_doomed_finalizers(&mut self) {
        if self.finalizer_registry.is_empty() {
            return;
        }
        let registry = std::mem::take(&mut self.finalizer_registry);
        let mut doomed = Vec::new();
        for ptr in registry {
            // SAFETY: registered at allocation; every sweep that could free an
            // unmarked finalizer is preceded by this scan, which removes it
            // from the registry first, so `ptr` is live. The world is stopped
            // and marking has drained, so the mark bit is final.
            //
            // The registry can hold TENURED finalizers (promoted at the first
            // partition cycle, never swept): their frozen bit must not be
            // interpreted against the current parity — a tenured finalizer is
            // permanently live, never doomed.
            if unsafe {
                (*ptr).header.gc.tenured || (*ptr).header.gc.is_marked_at(self.mark_parity)
            } {
                self.finalizer_registry.push(ptr);
            } else {
                doomed.push(unsafe { (*ptr).function });
            }
        }
        if doomed.is_empty() {
            return;
        }
        for function in doomed.iter().copied() {
            self.mark_or_push_child(function, "doomed-finalizer-function");
        }
        self.mark_all();
        self.doomed_finalizer_functions.extend(doomed);
    }

    /// Take every function queued by the doomed-finalizer scans so far. The
    /// evaluator's cycle-completed block calls each with zero args, errors
    /// ignored (GNU `run_finalizers`). Taking the whole batch means a
    /// finalizer created — and doomed — during a finalizer run lands in a
    /// later batch, run after a later cycle.
    pub fn take_doomed_finalizer_functions(&mut self) -> Vec<TaggedValue> {
        std::mem::take(&mut self.doomed_finalizer_functions)
    }

    /// Number of live finalizer objects still registered. `dump-emacs-portable`
    /// consults this after its pre-dump collection: the portable dump cannot
    /// represent finalizer objects (the writer arms refuse them), so a
    /// non-empty registry means the dump must be refused with an elisp error
    /// before writing starts. Registry emptiness is a sound precondition for
    /// the writer: every finalizer the dump walk could reach is live
    /// (registered at allocation, deregistered only when doomed — at which
    /// point it is unreachable and swept within the same completed cycle).
    pub(crate) fn live_finalizer_count(&self) -> usize {
        self.finalizer_registry.len()
    }

    /// True when doomed finalizer functions are queued but have not yet run.
    /// Empty whenever `gc_collect_exact` returns (its cycle-completed block
    /// drains and runs the whole batch); `dump-emacs-portable` asserts this
    /// before writing so a dumped image can never silently lose pending runs.
    pub(crate) fn has_pending_doomed_finalizers(&self) -> bool {
        !self.doomed_finalizer_functions.is_empty()
    }

    /// Resolve the weak hash tables discovered during this cycle's mark — GNU
    /// `mark_and_sweep_weak_table_contents` (alloc.c) + `sweep_weak_table`
    /// (fns.c). Runs at the stop-the-world `complete_collection` after the main
    /// mark drains. First a fixpoint marks the key/value of every entry that
    /// survives per its table's weakness — iterate to stability because a value
    /// in one weak table may be a key in another — then non-surviving entries
    /// are removed.
    pub(super) fn mark_and_sweep_weak_tables(&mut self) {
        // Seed every PERMANENT (tenured/mapped) weak table into this cycle's
        // worklist. The main mark skips permanent-black objects, so these would
        // otherwise never be swept again and their entries would be pinned
        // forever. GNU re-encounters and re-sweeps every live weak table on every
        // GC; this restores that for permanents. Young/runtime weak tables are
        // already registered by `trace_veclike` / `register_weak_hash_table_for_
        // sweep` during this cycle's mark.
        for &tptr in &self.permanent_weak_hash_tables {
            if self.weak_hash_tables_set.insert(tptr) {
                self.weak_hash_tables.push(tptr);
            }
        }

        if self.weak_hash_tables.is_empty() {
            return;
        }

        // -- Mark phase: keep marking surviving entries until nothing changes. --
        loop {
            let mut marked = false;
            // The worklist holds raw pointers, stable across this stop-the-world
            // step; copy them so the body can call `&mut self` methods.
            let tables = self.weak_hash_tables.clone();
            for tptr in tables {
                // SAFETY: `tptr` was recorded this cycle from a live veclike; the
                // heap is exclusively owned here (mutator stopped). Snapshot the
                // entries so the `ht` borrow is released before `push_gray`.
                let (weakness, entries): (
                    Option<HashTableWeakness>,
                    Vec<(TaggedValue, TaggedValue)>,
                ) = unsafe {
                    let ht = &(*tptr).table;
                    let entries = ht
                        .data
                        .iter()
                        .map(|(hk, &value)| {
                            let key = ht.key_snapshot(hk).copied().unwrap_or(value);
                            (key, value)
                        })
                        .collect();
                    (ht.weakness, entries)
                };
                for (key, value) in entries {
                    let key_survives = self.is_value_marked(key);
                    let value_survives = self.is_value_marked(value);
                    if Self::keep_weak_entry(weakness, key_survives, value_survives) {
                        if !key_survives {
                            self.mark_or_push_child(key, "weak-hash-key");
                            marked = true;
                        }
                        if !value_survives {
                            self.mark_or_push_child(value, "weak-hash-value");
                            marked = true;
                        }
                    }
                }
            }
            // Drain whatever those surviving entries reached, then re-check.
            self.mark_all();
            if !marked {
                break;
            }
        }

        // -- Sweep phase: drop entries that did not survive. --
        let tables = std::mem::take(&mut self.weak_hash_tables);
        self.weak_hash_tables_set.clear();
        for tptr in tables {
            // SAFETY: as above; exclusive heap access.
            let (weakness, entries): (
                Option<HashTableWeakness>,
                Vec<(HashKey, TaggedValue, TaggedValue)>,
            ) = unsafe {
                let ht = &(*tptr).table;
                let entries = ht
                    .data
                    .iter()
                    .map(|(hk, &value)| {
                        let key = ht.key_snapshot(hk).copied().unwrap_or(value);
                        (hk.clone(), key, value)
                    })
                    .collect();
                (ht.weakness, entries)
            };
            let dead: Vec<HashKey> = entries
                .into_iter()
                .filter_map(|(hk, key, value)| {
                    let keep = Self::keep_weak_entry(
                        weakness,
                        self.is_value_marked(key),
                        self.is_value_marked(value),
                    );
                    (!keep).then_some(hk)
                })
                .collect();
            if dead.is_empty() {
                continue;
            }
            // SAFETY: exclusive heap access. Mirror `builtin_remhash`'s removal.
            let ht = unsafe { &mut (*tptr).table };
            for hk in dead {
                let _ = ht.data.remove(&hk);
            }
        }
    }

    /// GNU `keep_entry_p` (fns.c): does a weak-table entry survive, given whether
    /// its key and value are independently reachable?
    pub(super) fn keep_weak_entry(
        weakness: Option<HashTableWeakness>,
        strong_key: bool,
        strong_value: bool,
    ) -> bool {
        match weakness {
            None => true,
            Some(HashTableWeakness::Key) => strong_key,
            Some(HashTableWeakness::Value) => strong_value,
            Some(HashTableWeakness::KeyOrValue) => strong_key || strong_value,
            Some(HashTableWeakness::KeyAndValue) => strong_key && strong_value,
        }
    }

    /// Post-mark portion of a collection: verify, sweep, promote, account, and
    /// clear the remembered/dirty bookkeeping. Shared by the stop-the-world
    /// `complete_collection` and the incremental mark-termination path. By the
    /// time this runs the gray queue is fully drained (marking is complete) and
    /// the marker chain heads are installed.
    pub(super) fn finalize_collection(
        &mut self,
        mark_us: u64,
        bytes_before: usize,
        t0: crate::host_time::Instant,
    ) {
        // Dump-partition safety gate: prove no live heap object reachable only
        // through a dumped object was left unmarked (i.e. the write barrier's
        // remembered set is complete). Off unless explicitly verifying.
        if self.partition_dump
            && self.dump_blackened
            && std::env::var("NEOVM_GC_VERIFY_PARTITION").as_deref() == Ok("1")
        {
            self.verify_dump_partition();
            // Incremental marking adds young-black->young-white as a possible
            // failure mode (a missed write-barrier owner). Check it too.
            self.verify_incremental_tricolor();
        }

        let sweep_t0 = crate::host_time::Instant::now();

        // Unchain dead markers BEFORE `sweep_objects` frees them; the
        // chain would otherwise hold dangling pointers after the sweep.
        // Mirrors GNU `sweep_buffer → unchain_dead_markers` (`alloc.c`).
        // Reading `header.gc.marked` is sound here because the
        // allocation is still live until `sweep_objects` runs below.
        self.unchain_dead_markers();

        // -- Sweep phase --
        let cons_live_bytes = self.sweep_cons();
        let object_live_bytes = self.sweep_objects();
        // Object arena pages: the intrusive-list sweep above never sees page
        // floats/strings/vectors; their page sweeps are the second half of
        // the eager sweep. Survivor bytes are VARIABLE-size
        // (`object_bytes_from_header`) and feed this recompute site exactly
        // like the list survivors' — `live_bytes` drives the adaptive pacer
        // (`effective_gc_threshold_bytes`), so an undercount here means
        // overtriggering.
        let (page_live_bytes, _page_freed) = self.sweep_arena_pages_ranges(
            (0, self.float_arena.pages.len()),
            (0, self.string_arena.pages.len()),
            (0, self.vector_arena.pages.len()),
            (0, self.bytecode_arena.pages.len()),
            (0, self.lambda_arena.pages.len()),
            (0, self.macro_arena.pages.len()),
            (0, self.record_arena.pages.len()),
            (0, self.symbol_with_pos_arena.pages.len()),
        );
        let _released_cons_blocks = self.release_empty_cons_blocks();
        let _released_object_pages = self.release_empty_object_pages();
        let mapped_object_live_bytes = self.mapped_non_cons_live_bytes();
        self.live_bytes = cons_live_bytes
            .saturating_add(object_live_bytes)
            .saturating_add(page_live_bytes)
            .saturating_add(mapped_object_live_bytes);
        self.bytes_since_gc = 0;
        // Pacer: a stop-the-world cycle has no concurrent mark window; drop
        // any stale stamp so the next concurrent cycle measures cleanly.
        self.pace_mark_start = None;
        self.forced_termination_pending = false;

        // End of the first partition cycle: every survivor is a permanent.
        // Promote them to the tenured old generation and blacken the dump so
        // all later cycles skip both regions.
        if self.partition_dump && !self.dump_blackened {
            self.promote_and_blacken();
            self.dump_blackened = true;
        }
        self.first_cycle_concurrent = false;
        self.staged_mapped_cons_scan = None;
        self.staged_mapped_veclikes = None;

        let sweep_us = sweep_t0.elapsed().as_micros() as u64;
        // Eager STW sweep cost feeds the same lifetime total as the deferred
        // slices, so the two sweep paths are comparable.
        self.sweep_lifetime_us += sweep_us;
        let elapsed = t0.elapsed();
        self.gc_collections += 1;
        self.gc_total_elapsed_us += elapsed.as_micros() as u64;

        // Phase split + dump-partition opportunity sizing. `mapped_marked` is
        // the immutable pdump (mapped) objects re-traced this cycle — the work
        // a "dump as permanent tenured region" partition would eliminate —
        // versus the mutable heap (`cons_live` + `heap_noncons`).
        let (mapped_total, mapped_marked) = self.mapped_object_stats();
        // Batch/headless runs don't install the tracing subscriber, so mirror
        // the phase split to stderr when `NEOVM_GC_TRACE=1` for profiling.
        if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            // Per-class dump composition: sizes the first-cycle-concurrent
            // work split (conses scan on the GC thread; veclikes/strings are
            // handshake-side until their concurrent-read safety is proven).
            let dump_cons: usize = self.mapped_cons_ranges.iter().map(|range| range.len).sum();
            let dump_float: usize = self.mapped_float_ranges.iter().map(|range| range.len).sum();
            eprintln!(
                "NEOVM_GC gc#{} {:.2}ms [clear={}us mark={}us sweep={}us] \
                 cons_live={} heap_noncons={} dump_marked={}/{} \
                 dump[cons={} vec={} str={} float={}] dirty_owners={} live={}B",
                self.gc_collections,
                elapsed.as_micros() as f64 / 1000.0,
                self.last_clear_us,
                mark_us,
                sweep_us,
                self.cons_live_count,
                self.non_cons_object_addrs.len(),
                mapped_marked,
                mapped_total,
                dump_cons,
                self.mapped_veclike_objects.len(),
                self.mapped_string_objects.len(),
                dump_float,
                self.dirty_owners.len(),
                self.live_bytes,
            );
        }
        tracing::debug!(
            "gc#{} {:.2}ms [clear={}us mark={}us sweep={}us] {} → {} bytes ({:+.1}%), \
             cons_live={}, heap_noncons={}, dump_marked={}/{}, dirty_owners={}, threshold={}",
            self.gc_collections,
            elapsed.as_micros() as f64 / 1000.0,
            self.last_clear_us,
            mark_us,
            sweep_us,
            bytes_before,
            self.live_bytes,
            if bytes_before > 0 {
                (self.live_bytes as f64 - bytes_before as f64) / bytes_before as f64 * 100.0
            } else {
                0.0
            },
            self.cons_live_count,
            self.non_cons_object_addrs.len(),
            mapped_marked,
            mapped_total,
            self.dirty_owners.len(),
            self.gc_threshold,
        );

        // Owner-tracking remembered-set precursor: NOT cleared here. Its
        // per-cycle lifecycle is clear-at-BEGIN (`begin_collection`), aligned with
        // the SATB dedup sets, so a freed owner's address cannot linger across a
        // sweep+arena-reuse into a stale dedup (the dirty_owners ABA). Clearing at
        // end would restore that hazard for any consumer that keeps the tables
        // live through the sweep.

        // A full STW cycle has completed: the heap now has consistent live
        // accounting and an empty gray queue, the baseline the concurrent
        // collector starts from. Dump-less heaps run concurrent marking from
        // the next safe-point collection on (`should_run_concurrent`).
        self.bootstrap_collected = true;
    }

    /// Drain the gray queue, marking and tracing all reachable objects.
    pub(super) fn mark_all(&mut self) {
        while let Some(val) = self.gray_queue.pop() {
            self.mark_value(val);
        }
    }

    /// Drain the gray queue on the background GC thread (Phase 4). The mutator
    /// blocks on the done-channel until the GC thread finishes, so heap access
    /// is exclusive (no concurrency hazard yet). This proves the thread +
    /// heap-sharing + handshake; the pause is not yet reduced. Phase 5 removes
    /// the block so marking actually overlaps mutator execution.
    pub(super) fn mark_all_on_gc_thread(&mut self) {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let ptr = self as *mut TaggedHeap;
        gc_thread()
            .send(GcRequest::MarkAll(HeapPtr(ptr), done_tx))
            .expect("neovm-gc thread is gone");
        // Block until the GC thread has finished marking on the shared heap.
        done_rx.recv().expect("neovm-gc thread did not respond");
    }
}
