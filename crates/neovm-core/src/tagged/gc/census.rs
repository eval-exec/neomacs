//! The generation census (design P3.1 C0.1, falsifier F-G (a)): what a
//! generational collector would have traced, measured on today's.
//!
//! Trace-only. With `NEOVM_GC_CENSUS=1` every cycle, at the point where its
//! marks are final and before its sweep frees anything, classifies each
//! marked young heap object (tenured and image objects are excluded: they
//! are permanent already) by whether it was also marked at the previous
//! cycle's termination:
//!
//! - **old survivor**: marked then and now. Eager promotion would have made
//!   it old at the previous termination, so a minor collection would not
//!   trace it;
//! - **young survivor**: marked now, not then. A minor traces exactly these;
//! - **promoted then dead**: a young survivor of the previous cycle that is
//!   unmarked now. Eager promotion would have promoted it, and only a major
//!   frees it.
//!
//! Why "marked at the previous termination" is exact per address: an object
//! marked at a termination survives that cycle's sweep, so its address
//! names the same object until the next cycle's sweep, which runs after the
//! next census. Nothing else frees a heap object.
//!
//! With `NEOVM_GC_CENSUS_REMSET=1` it also estimates the remembered set a
//! minor would seed: the barrier window covers every owner (so every store
//! reaches [`TaggedHeap::census_note_write`]) and the census counts the
//! distinct old owners (census-old, tenured or image) that received a heap
//! value since the previous termination, and their heap children at this
//! one. That perturbs time, not counts.
//!
//! One record per cycle goes to the `neovm::gc::census` tracing target at
//! INFO, e.g. with `RUST_LOG=neovm::gc::census=info
//! NEOMACS_LOG_FILE=census.log` (see [`CensusRecord`]'s `Display`).

use super::mark_sweep::VisitChild;
use super::*;

/// Objects of one census class in one cycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CensusCounts {
    pub(crate) conses: usize,
    pub(crate) objects: usize,
    /// Conses at their cell size plus each object's
    /// `object_bytes_from_header` (fixed part and payload).
    pub(crate) bytes: usize,
}

impl CensusCounts {
    fn add_conses(&mut self, n: usize) {
        self.conses += n;
        self.bytes += n * size_of::<ConsCell>();
    }

    fn add_object(&mut self, bytes: usize) {
        self.objects += 1;
        self.bytes += bytes;
    }
}

/// Which termination a census record was taken at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CensusCycleKind {
    /// A concurrent mark's stop-the-world termination (`incremental_finish`).
    Concurrent,
    /// A stop-the-world collection (`complete_collection`).
    StopTheWorld,
}

/// One cycle's census.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CensusRecord {
    /// The collection number this cycle completes as (`gc_collections`
    /// after it).
    pub(crate) cycle: usize,
    pub(crate) kind: CensusCycleKind,
    pub(crate) old_survivors: CensusCounts,
    pub(crate) young_survivors: CensusCounts,
    pub(crate) promoted_dead: CensusCounts,
    /// Distinct old owners written with a heap value since the previous
    /// termination (only with the remembered-set probe; else 0).
    pub(crate) remset_owners: usize,
    /// Their heap children now.
    pub(crate) remset_children: usize,
    /// Bytes allocated while this cycle's concurrent mark ran (born marked,
    /// so counted as young survivors); 0 for a stop-the-world cycle.
    pub(crate) mark_window_alloc_bytes: usize,
}

impl std::fmt::Display for CensusRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let triple = |c: &CensusCounts| format!("({},{},{})", c.conses, c.objects, c.bytes);
        write!(
            f,
            "NEOVM_GC census gc#{} kind={} old_surv={} young_surv={} promoted_dead={} \
             remset_owners={} remset_children={} mark_window_alloc={}",
            self.cycle,
            match self.kind {
                CensusCycleKind::Concurrent => "concurrent",
                CensusCycleKind::StopTheWorld => "stw",
            },
            triple(&self.old_survivors),
            triple(&self.young_survivors),
            triple(&self.promoted_dead),
            self.remset_owners,
            self.remset_children,
            self.mark_window_alloc_bytes,
        )
    }
}

/// A cons block's marks at the previous termination and, of those, the
/// cells that were young survivors then.
struct ConsBlockCensus {
    prev: [usize; CONS_MARK_WORDS],
    young_last: [usize; CONS_MARK_WORDS],
}

/// A heap's census state (`TaggedHeap::census`, present only when on).
pub(super) struct GenCensus {
    remset_probe: bool,
    /// Per cons block base. An entry leaves when its block is released
    /// ([`TaggedHeap::census_forget_cons_block`]): a new block at the same
    /// address must start with no history.
    cons: FxHashMap<usize, Box<ConsBlockCensus>>,
    /// Young non-cons objects marked at the previous termination.
    prev_objects: FxHashSet<usize>,
    /// The young survivors among them.
    young_last_objects: FxHashSet<usize>,
    /// Old owners written with a heap value since the previous termination,
    /// deduplicated by bits, in first-write order.
    remset_seen: FxHashSet<usize>,
    remset: Vec<TaggedValue>,
    last: Option<CensusRecord>,
}

impl GenCensus {
    /// The census state for a new heap, per the knob.
    pub(super) fn from_knob() -> Option<Box<Self>> {
        let remset_probe = match knobs::census_mode() {
            knobs::CensusMode::Off => return None,
            knobs::CensusMode::Survivors => false,
            knobs::CensusMode::SurvivorsAndRemset => true,
        };
        Some(Box::new(Self {
            remset_probe,
            cons: FxHashMap::default(),
            prev_objects: FxHashSet::default(),
            young_last_objects: FxHashSet::default(),
            remset_seen: FxHashSet::default(),
            remset: Vec::new(),
            last: None,
        }))
    }

    /// Whether the remembered-set probe is on (the barrier window then
    /// covers every owner).
    pub(super) fn remset_probe(&self) -> bool {
        self.remset_probe
    }

    /// Drop a released cons block's history (see the `cons` field).
    pub(super) fn forget_cons_block(&mut self, base: usize) {
        self.cons.remove(&base);
    }

    /// Test hook: does the census hold history for the block at `base`?
    #[cfg(test)]
    pub(super) fn has_cons_block_history(&self, base: usize) -> bool {
        self.cons.contains_key(&base)
    }
}

impl TaggedHeap {
    /// Take this cycle's census. Marks must be final (after the doomed
    /// finalizers and the weak tables) and nothing swept yet; no-op unless
    /// the census is on.
    #[cold]
    #[inline(never)]
    pub(super) fn census_at_termination(
        &mut self,
        kind: CensusCycleKind,
        mark_window_alloc_bytes: usize,
    ) {
        let Some(mut census) = self.census.take() else {
            return;
        };
        debug_assert!(!self.alloc_regions_open(), "census with an open region");
        let mut record = CensusRecord {
            cycle: self.gc_collections + 1,
            kind,
            old_survivors: CensusCounts::default(),
            young_survivors: CensusCounts::default(),
            promoted_dead: CensusCounts::default(),
            remset_owners: 0,
            remset_children: 0,
            mark_window_alloc_bytes,
        };

        // -- Conses: one pass over each block's mark words. Bits at or above
        //    a block's bump cursor are never set, so whole words are exact. --
        let mut cons: FxHashMap<usize, Box<ConsBlockCensus>> =
            FxHashMap::with_capacity_and_hasher(self.cons_blocks.len(), Default::default());
        for block in &self.cons_blocks {
            let base = block.base_addr();
            let mut entry = census.cons.remove(&base).unwrap_or_else(|| {
                Box::new(ConsBlockCensus {
                    prev: [0; CONS_MARK_WORDS],
                    young_last: [0; CONS_MARK_WORDS],
                })
            });
            for w in 0..CONS_MARK_WORDS {
                let marks = block.mark_word(w).load(Ordering::Relaxed);
                let prev = entry.prev[w];
                let young_last = entry.young_last[w];
                record
                    .old_survivors
                    .add_conses((marks & prev).count_ones() as usize);
                record
                    .young_survivors
                    .add_conses((marks & !prev).count_ones() as usize);
                record
                    .promoted_dead
                    .add_conses((prev & !marks & young_last).count_ones() as usize);
                entry.young_last[w] = marks & !prev;
                entry.prev[w] = marks;
            }
            cons.insert(base, entry);
        }
        census.cons = cons;

        // -- Young non-cons objects: the arena slots and the Box list. --
        let parity = self.mark_parity;
        let mut now: FxHashSet<usize> = FxHashSet::with_capacity_and_hasher(
            census.prev_objects.len() + census.prev_objects.len() / 4,
            Default::default(),
        );
        let mut young_now: FxHashSet<usize> = FxHashSet::default();
        {
            let prev = &census.prev_objects;
            let mut visit = |header: *mut GcHeader| {
                // SAFETY: an allocated arena slot or a node of the young
                // list, so a live, fully written header.
                let h = unsafe { &*header };
                if h.tenured || !h.is_marked_at(parity) {
                    return;
                }
                let bytes = Self::object_bytes_from_header(header);
                let addr = header as usize;
                if prev.contains(&addr) {
                    record.old_survivors.add_object(bytes);
                } else {
                    record.young_survivors.add_object(bytes);
                    young_now.insert(addr);
                }
                now.insert(addr);
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
            let mut obj = self.all_objects;
            while !obj.is_null() {
                visit(obj);
                // SAFETY: a young-list node.
                obj = unsafe { (*obj).next };
            }
        }
        for &addr in &census.young_last_objects {
            // SAFETY: marked at the previous termination, so not swept since
            // (see the module doc): a live header.
            let header = addr as *const GcHeader;
            if unsafe { (*header).tenured } || now.contains(&addr) {
                continue;
            }
            record
                .promoted_dead
                .add_object(Self::object_bytes_from_header(header));
        }
        census.prev_objects = now;
        census.young_last_objects = young_now;

        // -- The remembered-set estimate. Every owner is still allocated:
        //    image and tenured objects are never freed, and a census-old one
        //    was marked at the previous termination. --
        if census.remset_probe {
            record.remset_owners = census.remset.len();
            for owner in census.remset.drain(..) {
                record.remset_children += Self::census_heap_child_count(owner);
            }
            census.remset_seen.clear();
        }

        tracing::info!(target: "neovm::gc::census", "{record}");
        census.last = Some(record);
        self.census = Some(census);
    }

    /// Heap children of `owner` (immediates create no edge a minor traces).
    fn census_heap_child_count(owner: TaggedValue) -> usize {
        let mut count = 0usize;
        if owner.is_cons() {
            let ptr = owner.xcons_ptr();
            // SAFETY: a live cons (see the caller).
            let (car, cdr) = unsafe { ((*ptr).load_car(), (*ptr).load_cdr()) };
            count += usize::from(car.is_heap_object()) + usize::from(cdr.is_heap_object());
        } else if let Some(ptr) = owner.as_veclike_ptr() {
            Self::for_each_veclike_child(
                ptr as *mut VecLikeHeader,
                &mut VisitChild(|child: TaggedValue| {
                    count += usize::from(child.is_heap_object());
                }),
            );
        } else if let Some(ptr) = owner.as_string_ptr() {
            // SAFETY: a live string (see the caller).
            let intervals = unsafe { (*ptr).data.intervals() };
            if !intervals.is_empty() {
                intervals.for_each_root(|root| count += usize::from(root.is_heap_object()));
            }
        }
        count
    }

    /// A cons block is being released: its address may come back as a new
    /// block, which must start with no census history.
    pub(super) fn census_forget_cons_block(&mut self, base: usize) {
        if let Some(census) = self.census.as_deref_mut() {
            census.forget_cons_block(base);
        }
    }

    /// The remembered-set probe's barrier hook: `record` passed the barrier
    /// gate (the probe makes the window cover every owner). Logs its owner
    /// once per cycle when it is old and the store may add a heap edge.
    #[cold]
    #[inline(never)]
    pub(super) fn census_note_write(&mut self, record: HeapWriteRecord) {
        if !self.census.as_deref().is_some_and(GenCensus::remset_probe) {
            return;
        }
        if let Some(value) = record.value
            && !value.is_heap_object()
        {
            return;
        }
        let owner = record.owner;
        let Some(addr) = Self::value_heap_addr(owner) else {
            return;
        };
        let mapped = self.owner_is_mapped(owner);
        let census = self.census.as_deref_mut().expect("checked above");
        if census.remset_seen.contains(&owner.bits()) {
            return;
        }
        let old = if mapped {
            true
        } else if owner.is_cons() {
            let base = addr & !(CONS_BLOCK_ALIGN - 1);
            let offset = addr - base;
            census.cons.get(&base).is_some_and(|entry| {
                offset < CONS_CELLS_BYTES && {
                    let bit = ConsBlock::mark_bit(offset / size_of::<ConsCell>());
                    entry.prev[bit.word_index] & bit.mask != 0
                }
            })
        } else {
            // SAFETY: a non-cons heap value points at a live `GcHeader`.
            let tenured = unsafe { (*(addr as *const GcHeader)).tenured };
            tenured || census.prev_objects.contains(&addr)
        };
        if old {
            census.remset_seen.insert(owner.bits());
            census.remset.push(owner);
        }
    }

    /// Test hook: the last cycle's census, when the census is on.
    #[cfg(test)]
    pub(crate) fn last_census_for_test(&self) -> Option<CensusRecord> {
        self.census.as_deref().and_then(|census| census.last)
    }
}

/// Whether the remembered-set probe is on for this process: the barrier's
/// outlined part then hands every write to the census too.
#[inline]
pub(super) fn census_remset_probe_on() -> bool {
    knobs::census_mode() == knobs::CensusMode::SurvivorsAndRemset
}
