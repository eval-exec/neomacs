//! Metadata adapters over one canonical registration set. None of these
//! adapters authorizes heap dereference or mints StoppedMutatorsProof.
use super::super::pacing::sum_pacing_counters;
use super::super::young_registry::YoungBlockLog;
use super::image::PublishedGcWork;
use super::*;

fn lock_entry<'entry>(
    registry: &Registry,
    entry: &'entry MutatorEntry,
) -> Result<MutexGuard<'entry, EntryState>, RegistrationError> {
    // A visitor may inspect the same view again. Keep the image borrowed for
    // its callback, but refuse reentry instead of waiting on our own guard.
    match entry.state.try_lock() {
        Ok(state) => Ok(state),
        Err(std::sync::TryLockError::WouldBlock) => Err(RegistrationError::Busy),
        Err(std::sync::TryLockError::Poisoned(_)) => {
            registry.poison();
            Err(RegistrationError::Poisoned)
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RegistrationCensus {
    pub entries: usize,
    pub retiring: usize,
    pub cancelled_starts: usize,
    pub allocated_count: usize,
    pub bytes_since_gc: usize,
    pub bytes_banked_at_resets: u64,
    pub promoted_since_major: usize,
    pub minors_since_major: usize,
    pub stress_cycles_since_major: usize,
    pub remembered_words: usize,
    pub mapped_owners: usize,
    pub black_headers: usize,
    pub black_regions: usize,
    pub symbol_preimages: usize,
    pub cons_writes: usize,
    pub snapshotted_owners: usize,
    pub string_preimage_addrs: usize,
    pub root_words: usize,
    pub young_blocks: usize,
}

/// A gate-bounded metadata census. It is deliberately NOT collection authority:
/// no heap pointer, stopped-world constructor, raw decoding or mark operation.
pub struct StoppedSummaries<'era> {
    registry: &'era Registry,
    gate: RegistryGuard<'era>,
    _thread: PhantomData<Rc<()>>,
}

impl std::fmt::Debug for StoppedSummaries<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoppedSummaries")
            .field("heap", &self.registry.heap)
            .finish_non_exhaustive()
    }
}

impl<'era> StoppedSummaries<'era> {
    pub(super) fn new(registry: &'era Registry) -> Result<Self, RegistrationError> {
        let gate = registry.lock()?;
        // Do not filter out Running, Starting, loaned or missing entries.
        for entry in &gate.entries {
            if entry.cancelled_start.load(Ordering::Acquire) {
                continue;
            }
            registry.validate(&gate, entry.key)?;
            let state = lock_entry(registry, entry)?;
            if !matches!(
                state.phase,
                EntryPhase::Frozen | EntryPhase::Published | EntryPhase::Retiring
            ) {
                return Err(RegistrationError::Busy);
            }
            if state.run_loan || state.worker_loan {
                return Err(RegistrationError::OutstandingLoan);
            }
            let image = state.image.as_ref().ok_or(RegistrationError::Publication)?;
            if image.young_id() != entry.young {
                return Err(RegistrationError::Publication);
            }
            if state.phase != EntryPhase::Frozen
                && (state.publication_generation == 0 || state.epoch == 0)
            {
                return Err(RegistrationError::Publication);
            }
        }
        Ok(Self {
            registry,
            gate,
            _thread: PhantomData,
        })
    }

    fn visit_images(
        &self,
        mut visit: impl FnMut(MutatorKey, EntryPhase, &PublishedGcImage),
    ) -> Result<(), RegistrationError> {
        if self.registry.park.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        for entry in &self.gate.entries {
            if entry.cancelled_start.load(Ordering::Acquire) {
                continue;
            }
            let state = lock_entry(self.registry, entry)?;
            let image = state.image.as_ref().ok_or(RegistrationError::Publication)?;
            visit(entry.key, state.phase, image);
        }
        Ok(())
    }

    pub fn census(&self) -> Result<RegistrationCensus, RegistrationError> {
        let mut total = RegistrationCensus::default();
        let mut pacing = Vec::with_capacity(self.gate.entries.len());
        total.cancelled_starts = self
            .gate
            .entries
            .iter()
            .filter(|e| e.cancelled_start.load(Ordering::Acquire))
            .count();
        self.visit_images(|_, phase, image| {
            let n = image.summary();
            total.entries += 1;
            total.retiring += usize::from(phase == EntryPhase::Retiring);
            total.allocated_count = total.allocated_count.saturating_add(n.allocated_count);
            total.bytes_since_gc = total.bytes_since_gc.saturating_add(n.bytes_since_gc);
            total.bytes_banked_at_resets = total
                .bytes_banked_at_resets
                .saturating_add(n.bytes_banked_at_resets);
            total.remembered_words += n.remembered_words;
            total.mapped_owners += n.mapped_owners;
            total.black_headers += n.black_headers;
            total.black_regions += n.black_regions;
            total.symbol_preimages += n.symbol_preimages;
            total.cons_writes += n.cons_writes;
            total.snapshotted_owners += n.snapshotted_owners;
            total.string_preimage_addrs += n.string_preimage_addrs;
            total.root_words += n.root_words;
            total.young_blocks += n.young_blocks;
            pacing.push(image.pacing());
        })?;
        let totals = sum_pacing_counters(pacing.iter());
        total.promoted_since_major = totals.promoted_since_major;
        total.minors_since_major = totals.minors_since_major;
        total.stress_cycles_since_major = totals.stress_cycles_since_major;
        Ok(total)
    }

    /// Borrow every work family from the same census without consuming a
    /// snapshot twice or silently skipping a retained retiring registration.
    pub(in crate::tagged::gc) fn visit_work(
        &self,
        mut visit: impl FnMut(MutatorKey, PublishedGcWork<'_>),
    ) -> Result<(), RegistrationError> {
        self.visit_images(|key, _, image| image.visit_work(|work| visit(key, work)))
    }

    pub(in crate::tagged::gc) fn with_young<R>(
        &mut self,
        expected: MutatorKey,
        young: YoungRegistrationId,
        use_log: impl FnOnce(&mut YoungBlockLog) -> R,
    ) -> Result<R, RegistrationError> {
        if self.registry.park.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        if young.heap_identity() != self.registry.heap {
            return Err(RegistrationError::WrongHeap);
        }
        let index = *self
            .gate
            .young_index
            .get(&young)
            .ok_or(RegistrationError::StaleRegistration)?;
        let entry = self.registry.validate(&self.gate, expected)?;
        if self.gate.entries[index].key != expected || entry.young != young {
            return Err(RegistrationError::StaleRegistration);
        }
        let mut state = lock_entry(self.registry, &entry)?;
        let image = state.image.as_mut().ok_or(RegistrationError::Publication)?;
        if image.young_id() != young {
            return Err(RegistrationError::Publication);
        }
        Ok(use_log(image.young_mut()))
    }
}

static_assertions::assert_not_impl_any!(StoppedSummaries<'static>: Send, Sync, Copy, Clone);
