//! Concurrent marking: the start handshake, launching and joining the background marker, SATB root feeding, and the first-partition-cycle policy.
//!
//! Moved out of `gc.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

impl TaggedHeap {
    /// True if a concurrent mark should drive THIS collection.
    ///
    /// Dump heaps: a partitioned post-dump heap whose first partition cycle
    /// has promoted + blackened the image (the young/old split bounds what is
    /// traced); that first cycle falls to the STW full path.
    ///
    /// Dump-less heaps: after the first completed STW collection — the same
    /// one-STW-bootstrap-then-concurrent shape as the dump path. Nothing
    /// tenures without a dump, so every cycle re-clears and re-marks the whole
    /// young heap (correct, just unpartitioned), and the concurrent job's dump
    /// checks never match (`dump_addr_lo/hi` stay MAX/0) while the
    /// remembered-set seeding is skipped entirely (`partition_dump` is false).
    ///
    /// A heap that registers a dump AFTER dump-less cycles switches back to
    /// the dump rule: the first partition cycle must be the STW full trace
    /// that promotes + blackens the image, regardless of earlier bootstraps.
    pub fn should_run_concurrent(&self) -> bool {
        std::cfg_select! {
            target_family = "wasm" => { false }
            _ => {
                if self.partition_dump {
                    self.dump_blackened
                } else {
                    self.bootstrap_collected
                }
            }
        }
    }

    /// True when the NEXT collection would be the first partition cycle (a
    /// registered dump not yet promoted+blackened). The driver runs it
    /// concurrently via [`Self::arm_first_cycle_concurrent`] +
    /// `concurrent_begin`/`launch_concurrent_mark` instead of the STW
    /// bootstrap.
    pub fn is_partition_first_cycle(&self) -> bool {
        std::cfg_select! {
            target_family = "wasm" => { false }
            _ => { self.partition_dump && !self.dump_blackened }
        }
    }

    /// Arm the concurrent first partition cycle (see the field doc).
    pub fn arm_first_cycle_concurrent(&mut self) {
        self.first_cycle_concurrent = true;
    }

    /// Complete the first partition cycle once its (possibly deferred) sweep
    /// has drained: promote survivors, blacken the image, build the initial
    /// remembered set — exactly `complete_collection`'s end-of-first-cycle
    /// block, run at the concurrent cycle's completion point instead. Also
    /// restores the mapped contribution to `live_bytes`, which the
    /// termination's accounting undercounted (mapped objects are never marked
    /// during the concurrent first cycle; blackening makes the marked-based
    /// sums whole). No-op on every later cycle and on dump-less heaps.
    pub fn finish_first_partition_cycle(&mut self) {
        if !(self.partition_dump && !self.dump_blackened) {
            self.first_cycle_concurrent = false;
            return;
        }
        self.promote_and_blacken();
        self.dump_blackened = true;
        self.first_cycle_concurrent = false;
        let mapped_cons_bytes: usize = self
            .mapped_cons_ranges
            .iter()
            .map(|range| range.live_count().saturating_mul(size_of::<ConsCell>()))
            .sum();
        self.live_bytes = self
            .live_bytes
            .saturating_add(self.mapped_non_cons_live_bytes())
            .saturating_add(mapped_cons_bytes);
    }

    /// True while the background GC thread is marking (between the start and
    /// termination handshakes) — the mutator is running concurrently.
    pub fn concurrent_mark_running(&self) -> bool {
        self.concurrent_mark_running
    }

    /// The GC thread has tentatively drained gray + SATB (Acquire pairs with the
    /// thread's Release). The mutator polls this at safe points to terminate.
    pub fn concurrent_mark_done(&self) -> bool {
        self.gc_done.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Start-of-cycle setup for a concurrent mark: clear young marks + seed the
    /// collector-internal and remembered roots (`begin_collection`), arm
    /// `mark_in_progress`. The caller then seeds context roots and calls
    /// `launch_concurrent_mark`. No Steele owner-tracking: the concurrent SATB
    /// barrier (keyed on `concurrent_mark_running`) preserves the snapshot.
    pub(crate) fn concurrent_begin(&mut self) {
        // Zero the seeding scratch so a skipped `seed_mapped_remembered`
        // (non-partitioned heap) does not leave a stale previous value in the
        // start slots filled below.
        self.last_remembered_seed_us = 0;
        self.last_remembered_seed_roots = 0;
        self.begin_collection();
        // Route this handshake's `begin_collection` phase costs to the START
        // slots (this entry point is exclusively the concurrent start).
        self.handshake.start_count += 1;
        self.handshake.last_start_clear_us = self.last_clear_us;
        self.handshake.last_start_clear_cons_us = self.last_clear_cons_us;
        self.handshake.last_start_clear_noncons_us = self.last_clear_noncons_us;
        self.handshake.last_start_clear_mapped_us = self.last_clear_mapped_us;
        self.handshake.last_start_runtime_us = self.last_runtime_seed_us;
        self.handshake.last_start_runtime_roots = self.last_runtime_seed_roots;
        self.handshake.last_start_remembered_us = self.last_remembered_seed_us;
        self.handshake.last_start_remembered_roots = self.last_remembered_seed_roots;
        self.mark_in_progress = true;
        self.incremental_mark_us = 0;
    }

    /// Hand the seeded gray queue (the full root snapshot) to the GC thread and
    /// start non-blocking concurrent marking. Returns immediately; the mutator
    /// resumes while the GC thread marks. Allocate-black turns on so new objects
    /// survive this cycle's sweep, and the SATB barrier starts logging.
    /// Stage 1b: stash the start-captured obarray scan snapshot for the next
    /// `launch_concurrent_mark` to move into the job. Called from
    /// `start_concurrent_mark` at the world-stopped start handshake (once per
    /// concurrent mark).
    pub(crate) fn set_pending_obarray_scan(
        &mut self,
        snap: crate::emacs_core::symbol::ObarrayScanSnapshot,
    ) {
        // Retain the start slot count for the termination residual re-seed before
        // the snapshot is moved into the GC job at `launch_concurrent_mark`.
        self.concurrent_obarray_start_slots = Some(snap.n_slots());
        self.pending_obarray_scan = Some(snap);
    }

    /// Stage 1b: take the start-of-cycle obarray slot count (set at the start
    /// handshake) for the termination residual re-seed. `None` for a cycle with
    /// no concurrent mark (e.g. a stop-the-world full collection).
    pub(crate) fn take_concurrent_obarray_start_slots(&mut self) -> Option<usize> {
        self.concurrent_obarray_start_slots.take()
    }

    pub(crate) fn launch_concurrent_mark(&mut self) {
        // Immutable snapshot of owned cons-block bases — read-only on the GC
        // thread. New blocks allocated during marking are absent, which is fine:
        // their conses allocate-black and never enter the GC's gray queue.
        let conssnap_t0 = crate::host_time::Instant::now();
        let mut owned =
            FxHashSet::with_capacity_and_hasher(self.cons_blocks.len(), Default::default());
        for block in &self.cons_blocks {
            owned.insert(block.base_addr());
        }
        // CONCURRENT STRING MARKING claim oracle (stage 3): capture the
        // string arena's page bases at this same world-stopped instant
        // (retired pages included — their tenured strings are claim-benign).
        // Built alongside the cons `owned_bases` so both snapshots share the
        // immutability argument: pages created after this point are absent
        // and their strings DEFER (fail-safe). Timed within the conssnap
        // handshake slot (same ownership-snapshot phase).
        let mut string_bases =
            FxHashSet::with_capacity_and_hasher(self.string_arena.pages.len(), Default::default());
        for page in &self.string_arena.pages {
            string_bases.insert(page.base_addr());
        }
        self.handshake.last_start_conssnap_us = conssnap_t0.elapsed().as_micros() as u64;
        self.handshake.probe_cons_blocks = self.cons_blocks.len();
        // CONCURRENT FLOAT CLAIMS (task 01) claim oracle: capture the float
        // arena's page bases at this same world-stopped instant (retired
        // pages included — their tenured floats recognize-and-drop at the
        // claim arm). Same immutability + Arc-publication argument as
        // `string_page_bases`; pages created after this point are absent and
        // their floats DEFER (fail-safe). O(pages); own handshake timer.
        let floatsnap_t0 = crate::host_time::Instant::now();
        let mut float_bases =
            FxHashSet::with_capacity_and_hasher(self.float_arena.pages.len(), Default::default());
        for page in &self.float_arena.pages {
            float_bases.insert(page.base_addr());
        }
        self.handshake.last_start_floatsnap_us = floatsnap_t0.elapsed().as_micros() as u64;
        // CONCURRENT VECTOR-HEADER CLAIMS (task 01) claim oracle: capture
        // the vector arena's page bases at this same world-stopped instant
        // (retired pages included — tenured vectors recognize-and-drop at
        // the claim arm). Same discipline as the float/string snapshots.
        // O(pages); own handshake timer, distinct from the Tier-B backing
        // `vecsnap` below.
        let vecbasesnap_t0 = crate::host_time::Instant::now();
        let mut vector_bases =
            FxHashSet::with_capacity_and_hasher(self.vector_arena.pages.len(), Default::default());
        for page in &self.vector_arena.pages {
            vector_bases.insert(page.base_addr());
        }
        self.handshake.last_start_vecbasesnap_us = vecbasesnap_t0.elapsed().as_micros() as u64;
        // CONCURRENT BYTECODE CLAIMS (task 01) claim oracle: capture the
        // bytecode arena's page bases at this same world-stopped instant
        // (retired pages included — tenured bytecode recognize-and-drops at
        // the claim arm). Same discipline as the float/vector snapshots.
        // O(pages); own handshake timer.
        let bcsnap_t0 = crate::host_time::Instant::now();
        let mut bytecode_bases = FxHashSet::with_capacity_and_hasher(
            self.bytecode_arena.pages.len(),
            Default::default(),
        );
        for page in &self.bytecode_arena.pages {
            bytecode_bases.insert(page.base_addr());
        }
        self.handshake.last_start_bcsnap_us = bcsnap_t0.elapsed().as_micros() as u64;
        let vecsnap_t0 = crate::host_time::Instant::now();
        // Stage 2 Tier B CONCURRENT VECTOR SCAN: snapshot every
        // OWNED/Mapped vector backing AT THIS world-stopped point (same instant the
        // cons `owned_bases` snapshot is taken and the roots are seeded), so the GC
        // thread can trace vectors concurrently instead of deferring them to the STW
        // termination. Vectors are heap-side, so capture directly here (no eval.rs
        // seam, unlike the Context-side obarray). Task #7 stage 2a (Fix A): iterate
        // the INCREMENTAL VECTOR REGISTRY (`vector_object_addrs`, maintained at
        // `link_veclike` + the sweep free sites) instead of filtering the whole
        // `non_cons_object_addrs` set — the filter walk was 11-32% of this
        // world-stopped start handshake. Vectors allocated mid-cycle are absent from
        // this capture and are covered by allocate-black.
        if (cfg!(test) && cfg!(debug_assertions))
            || std::env::var("NEOVM_GC_VERIFY_PARTITION").as_deref() == Ok("1")
        {
            // Fix A INVARIANT, stage-3 form: the registry equals the live
            // owned Vector population = ALLOCATED VECTOR-ARENA PAGE SLOTS ∪
            // the residual Box Vector subset of `non_cons_object_addrs`.
            // (The pre-stage-3 form — registry == Vector∩addr-set — would
            // fire on the first page vector; worse, if the registry were
            // silently EMPTY the old 0==0 check would pass and the Tier-B
            // vecsnap below would disable concurrent vector marking without
            // any test noticing.) Cross-check both directions: counts match
            // the union of the two disjoint sources, and every registry
            // address is page-owned xor addr-set-resident. Debug test builds
            // only (or explicit VERIFY_PARTITION): the release drain
            // profilers are themselves cfg(test) binaries, and this walk
            // would re-add cost inside the timed vecsnap region.
            let box_filter_count = self
                .non_cons_object_addrs
                .iter()
                .filter(|&&addr| unsafe {
                    (*(addr as *const GcHeader)).kind == HeapObjectKind::VecLike
                        && (*(addr as *const VecLikeHeader)).type_tag == VecLikeType::Vector
                })
                .count();
            let page_vector_count: usize =
                self.vector_arena.pages.iter().map(|p| p.allocated).sum();
            assert_eq!(
                self.vector_object_addrs.len(),
                box_filter_count + page_vector_count,
                "vector registry diverged from page slots ∪ residual Box vectors",
            );
            for &addr in &self.vector_object_addrs {
                let page_owned = self.vector_arena.owns(addr as *const u8);
                let box_owned = self.non_cons_object_addrs.contains(&addr);
                assert!(
                    page_owned ^ box_owned,
                    "vector registry address must be page-owned xor Box-owned",
                );
            }
            // Task 01 vector-claim inclusion, asserted from the CLAIM ARM's
            // perspective: every ALLOCATED vector-arena page slot must be
            // Tier-B-registered ({page vectors} ⊆ `vector_object_addrs`), so
            // a `vector_page_bases` HIT at `concurrent_try_mark_owned`
            // implies the claimed vector's backing is in the Tier-B snapshot
            // built below — its children trace concurrently, which is what
            // makes the header claim (and the removed termination re-trace)
            // sound. Retired pages included: their tenured slots drop at the
            // arm before any children question arises, but keeping them
            // registered is the standing registry invariant.
            for slot in self.vector_arena.collect_allocated_slots() {
                assert!(
                    self.vector_object_addrs.contains(&(slot as usize)),
                    "allocated vector page slot missing from the Tier-B \
                     registry — the claim arm would orphan its children",
                );
            }
        }
        let vectors = {
            let mut snap = crate::tagged::header::VectorScanSnapshot::with_capacity(
                self.vector_object_addrs.len(),
            );
            for &addr in &self.vector_object_addrs {
                // Safety: `addr` is a live owned Vector's `GcHeader` addr (the
                // registry invariant above); a VecLike header begins with its
                // `GcHeader`, so casting to `*const VectorObj` and reading its
                // backing is valid.
                let obj = unsafe { &*(addr as *const VectorObj) };
                snap.push(obj.data.scan_entry());
            }
            Some(snap)
        };
        self.handshake.last_start_vecsnap_us = vecsnap_t0.elapsed().as_micros() as u64;
        self.handshake.probe_vector_snapshot_len =
            vectors.as_ref().map(|snap| snap.len()).unwrap_or(0);
        let jobasm_t0 = crate::host_time::Instant::now();
        let gray = std::mem::take(&mut self.gray_queue);
        let (exited_tx, exited_rx) = std::sync::mpsc::channel();
        self.gc_done
            .store(false, std::sync::atomic::Ordering::Release);
        // Fresh per-cycle concurrent claim/drop counters.
        self.concurrent_str_claimed.store(0, Ordering::Relaxed);
        self.concurrent_float_claimed.store(0, Ordering::Relaxed);
        self.concurrent_subr_dropped.store(0, Ordering::Relaxed);
        self.concurrent_vec_claimed.store(0, Ordering::Relaxed);
        self.concurrent_bc_claimed.store(0, Ordering::Relaxed);
        self.gc_stop
            .store(false, std::sync::atomic::Ordering::Release);
        self.gc_exited = Some(exited_rx);
        self.concurrent_mark_running = true;
        // Keep the write-barrier fast path reaching `record_heap_write` so the
        // SATB log fires even with owner-tracking Disabled / no partition.
        TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.set(true));
        let job = ConcurrentMarkJob {
            gray,
            owned_bases: std::sync::Arc::new(owned),
            claims: ConcurrentClaimJob {
                // Mandated carry: the GC thread claims at THIS cycle's parity.
                parity: self.mark_parity,
                string_page_bases: std::sync::Arc::new(string_bases),
                float_page_bases: std::sync::Arc::new(float_bases),
                vector_page_bases: std::sync::Arc::new(vector_bases),
                bytecode_page_bases: std::sync::Arc::new(bytecode_bases),
                dump_lo: self.dump_addr_lo,
                dump_hi: self.dump_addr_hi,
                drop_dump_children: self.first_cycle_concurrent,
                str_claimed: self.concurrent_str_claimed.clone(),
                float_claimed: self.concurrent_float_claimed.clone(),
                subr_dropped: self.concurrent_subr_dropped.clone(),
                vec_claimed: self.concurrent_vec_claimed.clone(),
                bc_claimed: self.concurrent_bc_claimed.clone(),
            },
            satb: self.satb_shared.clone(),
            deferred: self.deferred_veclikes.clone(),
            done: self.gc_done.clone(),
            stop: self.gc_stop.clone(),
            wake: self.gc_wake.clone(),
            exited: exited_tx,
            // Stage 1b: consume the obarray snapshot the start handshake staged.
            // Take it so it is not left dangling for a later cycle.
            obarray: self.pending_obarray_scan.take(),
            // Stage 2 Tier B: the vector-backing snapshot captured just above.
            vectors,
            // First partition cycle: the staged mapped cons ranges (else None).
            mapped_cons_ranges: self.staged_mapped_cons_scan.take(),
            mapped_veclikes: self.staged_mapped_veclikes.take(),
        };
        launch_background_mark(job);
        self.handshake.last_start_jobasm_us = jobasm_t0.elapsed().as_micros() as u64;
        // Pacer: open this cycle's mark window (closed by `incremental_finish`).
        self.pace_mark_start = Some(crate::host_time::Instant::now());
        self.pace_mark_start_bytes = self.bytes_since_gc;
    }

    /// Stop the GC thread and fold its residual work back into the gray queue so
    /// the caller can finish marking stop-the-world. After this, the heap is
    /// owned exclusively by the mutator again (the GC thread has exited its loop).
    pub(crate) fn join_concurrent_mark(&mut self) {
        let join_t0 = crate::host_time::Instant::now();
        self.gc_stop
            .store(true, std::sync::atomic::Ordering::Release);
        // Task #7 stage 2a (Fix B): wake the GC thread out of its idle nap
        // NOW. Store-then-lock+notify pairs with the GC thread's
        // check-under-lock before waiting, so the notify cannot fall between
        // its flag check and its wait (no lost wakeup, no full-nap latency).
        {
            let (lock, cvar) = &*self.gc_wake;
            let _guard = lock.lock().unwrap();
            cvar.notify_all();
        }
        if let Some(rx) = self.gc_exited.take() {
            let _ = rx.recv(); // block until the GC thread leaves its mark loop
        }
        self.concurrent_mark_running = false;
        TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.set(false));
        // Residual SATB (children overwritten after the GC's last drain) +
        // deferred (every non-cons + non-owned cons the GC parked) become gray;
        // the caller reseeds roots, then drains to a fixpoint stop-the-world.
        // The fold is timed (`last_termination_fold_us`) so the termination's
        // cheap push half is attributable separately from the mark fixpoint.
        let fold_t0 = crate::host_time::Instant::now();
        let satb = std::mem::take(&mut *self.satb_shared.lock().unwrap());
        self.last_termination_satb = satb.len();
        self.gray_queue.extend(satb);
        let deferred = std::mem::take(&mut *self.deferred_veclikes.lock().unwrap());
        self.last_termination_deferred = deferred.len();
        self.max_termination_deferred = self.max_termination_deferred.max(deferred.len());
        // Strings/floats the GC thread claimed concurrently and subrs it
        // dropped (they never reached `deferred`); the exit handshake above
        // (`rx.recv()`) established the happens-before, so a Relaxed read
        // sees the final counts.
        self.last_concurrent_str_claimed = self.concurrent_str_claimed.load(Ordering::Relaxed);
        self.last_concurrent_float_claimed = self.concurrent_float_claimed.load(Ordering::Relaxed);
        self.last_concurrent_subr_dropped = self.concurrent_subr_dropped.load(Ordering::Relaxed);
        self.last_concurrent_vec_claimed = self.concurrent_vec_claimed.load(Ordering::Relaxed);
        self.last_concurrent_bc_claimed = self.concurrent_bc_claimed.load(Ordering::Relaxed);
        // Task 01 INSERTION-COVERAGE RE-TRACE (the load-bearing companion of
        // the vector-header claims): re-gray the CURRENT children of every
        // multi-child owner mutated this cycle (`satb_snapshotted_owners` —
        // populated by the write barrier's first-mutation dedup, so it is
        // exactly the mutated-owner set). The SATB deletion barrier preserves
        // only SNAPSHOT-time children; a value INSERTED mid-cycle (stored
        // from a mutator register — root→heap motion) into an
        // already-CLAIMED owner is otherwise invisible: the claimed mark bit
        // makes the termination's `mark_value` early-return, so the old
        // "every deferred veclike is re-traced on its CURRENT backing"
        // backstop no longer covers it. Bounded by mutation volume (each
        // owner once), not by the live vector population — which is the
        // whole point of claiming. Also covers claimed STRINGS that gained
        // interval tables mid-cycle (their wrapper barriers land the owner
        // in the same set).
        let written = std::mem::take(&mut self.satb_snapshotted_owners);
        for bits in written {
            self.push_value_children_to_gray(TaggedValue(bits), "satb-written-retrace");
        }
        // Classify what the drain is about to trace, per kind — the measurement
        // that decides which kinds a concurrent-tracing extension should take
        // on. Pure counting (marking behavior is unchanged), but the header
        // reads cost real STW time on a large buffer (~20ns/entry), so outside
        // the crate's own tests it only runs when the trace that prints it is
        // on; the kind buckets stay zero otherwise.
        if cfg!(test) || std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            let mut kinds = DrainKinds::default();
            for &val in &deferred {
                // Safety: parked entries are live heap values; nothing has been
                // swept since they were parked (see `DrainKinds::note`).
                unsafe { kinds.note(val) };
            }
            self.last_termination_kinds = kinds;
            self.max_termination_kinds.merge_max(&kinds);
        }
        self.termination_count += 1;
        self.gray_queue.extend(deferred);
        self.last_termination_fold_us = fold_t0.elapsed().as_micros() as u64;
        // Stage 2 Tier B CONCURRENT VECTOR SCAN: the GC thread has provably exited its
        // mark loop (the `rx.recv()` above), so its snapshot pointers into the retired
        // vector backings are no longer in use — this is the ONLY safe free point.
        // Drain + drop the retired originals and clear the per-cycle clone-dedup set.
        // Both are empty unless a clone-on-write fired this cycle.
        let retired = std::mem::take(&mut self.retired_vector_buffers);
        drop(retired);
        self.concurrent_cloned_vectors.clear();
        // Whole join cost (stop signal + GC-thread exit wait + the fold above);
        // the fold alone stays separately visible as `last_termination_fold_us`.
        self.handshake.last_term_join_us = join_t0.elapsed().as_micros() as u64;
    }

    /// SATB barrier path for concurrent marking: append the owner's current
    /// (pre-overwrite) children to the shared buffer the GC thread drains. Reuses
    /// the gray-queue child enumeration with `self.gray_queue` as scratch (it is
    /// empty during concurrent marking — the snapshot was handed to the thread).
    ///
    /// Per-cycle dedup for multi-child owners (veclike/string): the barrier can't
    /// know which slot the bulk closure will touch, so it logs the owner's WHOLE
    /// pre-image; doing that on every write is O(n) per write => O(n²) to build an
    /// n-element container (hash table, char-table, or a vector filled by `aset`
    /// in a loop — the `(ucs-names)` OOM). SATB only needs each owner's
    /// start-of-cycle child set logged ONCE: at the owner's FIRST mutation this
    /// cycle every snapshot-time child is still present (a child can only be
    /// unlinked by a mutation of THIS owner, i.e. this very first barrier firing
    /// pre-store), so one snapshot is a superset of the snapshot-time children;
    /// later writes overwrite only already-logged values (or born-black new ones,
    /// which need no logging). So re-snapshotting is pure waste — skip it. The
    /// snapshot set is cleared at every mark start (`concurrent_begin`).
    ///
    /// Conses (exactly two children) bypass the dedup: their barrier is already
    /// O(1), and a per-write `HashSet` insert on the hot car/cdr path would cost
    /// more than it saves. Re-logging a cons's 2 children is still SATB-correct.
    /// Hand a batch of LIVE mutator roots to the concurrent marker via the
    /// SATB channel. Extra live values in the SATB log are always safe (the
    /// marker treats each entry as gray; already-marked entries are skipped
    /// by the atomic mark test) — this exists so young data reachable ONLY
    /// from the mutator's stack marks CONCURRENTLY instead of all at once in
    /// the stop-the-world termination fold. A value that dies before the
    /// cycle ends floats one cycle, the standard SATB trade.
    pub(crate) fn feed_satb_roots(&self, values: &[TaggedValue]) {
        let mut shared = self.satb_shared.lock().unwrap();
        shared.extend(values.iter().copied().filter(|v| v.is_heap_object()));
    }

    pub(super) fn push_value_children_to_satb_shared(&mut self, owner: TaggedValue) {
        debug_assert!(self.gray_queue.is_empty());
        // Multi-child owners are deduped once per cycle; conses fall through to
        // the cheap direct enumeration below.
        if !owner.is_cons() && !self.satb_snapshotted_owners.insert(owner.bits()) {
            return; // this owner's full pre-image was already logged this cycle
        }
        self.push_value_children_to_gray(owner, "satb-concurrent");
        if !self.gray_queue.is_empty() {
            let mut shared = self.satb_shared.lock().unwrap();
            shared.extend(self.gray_queue.drain(..));
        }
    }

    /// SATB sink for a ROOT-slot overwrite (a symbol value/function/plist cell):
    /// log the pre-image VALUE itself so the concurrent mark grays and traces it
    /// (`join_concurrent_mark` folds `satb_shared` into the gray queue), keeping a
    /// symbol-only-reachable object live across the cycle. Unlike
    /// `push_value_children_to_satb_shared`, the retained thing is the overwritten
    /// value itself, not an owner's children — the symbol cell's "owner" is a
    /// non-heap root. No `concurrent_mark_running` assert: the caller already gated
    /// on the `TAGGED_HEAP_CONCURRENT_ACTIVE` thread-local (the source of truth),
    /// and an extra entry is at worst one cycle of floating garbage.
    pub(super) fn note_root_overwrite_value(&mut self, pre_image: TaggedValue) {
        self.satb_shared.lock().unwrap().push(pre_image);
    }

    /// Stage 2 Tier B CONCURRENT VECTOR SCAN clone-on-write hook. Called from
    /// `with_vector_data_mut` BEFORE a vector's OWNED backing is bulk-mutated, while a
    /// concurrent mark is active. On the owner's FIRST such
    /// mutation this cycle, if the backing is currently OWNED, replace it with a clone
    /// and RETIRE the original (kept alive to join) so the GC thread's start-of-cycle
    /// snapshot pointer keeps addressing an immutable, live buffer; the closure then
    /// mutates the clone. Idempotent per owner per cycle (dedup set), and a no-op when
    /// the backing is MAPPED (the snapshot points at the immutable dump; `ensure_owned`
    /// will promote it to a fresh OWNED the snapshot never reads, so no clone needed).
    ///
    /// Reachability of the pre-image children is handled separately by the
    /// `note_heap_write(VectorBulk)` SATB barrier the caller fires first; this hook
    /// only preserves the snapshot pointer's buffer for the concurrent READ.
    ///
    /// Safety: `owner` must be a live `VecLikeType::Vector` value on this heap.
    pub(crate) fn concurrent_clone_on_write_vector(&mut self, owner: TaggedValue) {
        // First mutation of this owner this cycle? `insert` returns false if already
        // present, so later mutations of the same owner skip the clone (they touch the
        // already-cloned live backing the snapshot does not point at).
        if !self.concurrent_cloned_vectors.insert(owner.bits()) {
            return;
        }
        let Some(header) = owner.as_veclike_ptr() else {
            return;
        };
        let obj = unsafe { &mut *(header as *mut VectorObj) };
        // Only OWNED backings need cloning: a MAPPED backing reads the immutable dump
        // span the snapshot captured; `ensure_owned` (run by the caller next) promotes
        // it to a brand-new OWNED buffer the snapshot never addresses.
        if !obj.data.is_owned() {
            return;
        }
        // Replace the backing with a clone; retire the original so the GC's snapshot
        // pointer keeps addressing it (immutable + alive) until the join free point.
        let original = obj.data.clone_owned_backing();
        self.retired_vector_buffers.push(original);
    }
}
