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
    pub(crate) retiring: usize,
    pub cancelled_starts: usize,
    pub allocated_count: usize,
    pub bytes_since_gc: usize,
    pub(crate) bytes_banked_at_resets: u64,
    pub(crate) promoted_since_major: usize,
    pub(crate) minors_since_major: usize,
    pub(crate) stress_cycles_since_major: usize,
    pub(crate) remembered_words: usize,
    pub(crate) mapped_owners: usize,
    pub(crate) black_headers: usize,
    pub(crate) black_regions: usize,
    pub(crate) symbol_preimages: usize,
    pub(crate) cons_writes: usize,
    pub(crate) snapshotted_owners: usize,
    pub(crate) string_preimage_addrs: usize,
    pub(crate) root_words: usize,
    pub(crate) young_blocks: usize,
}

#[cfg(feature = "test-support")]
impl RegistrationCensus {
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn cancelled_starts(&self) -> usize {
        self.cancelled_starts
    }
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
        // The keeper holding a stop takes its census through
        // `AllStopped::summaries`, whose borrow keeps `trace` and `complete`
        // from running while this guard holds the gate. Here nothing would:
        // a `complete` under this guard refuses, and that refusal poisons.
        if std::thread::current().id() == registry.keeper_thread
            && matches!(
                gate.rendezvous.phase,
                super::rendezvous::control::Phase::Stopping(_)
                    | super::rendezvous::control::Phase::Tracing(_)
            )
        {
            return Err(RegistrationError::WrongThread);
        }
        Self::from_gate(registry, gate)
    }

    /// The census taken by the keeper under its stop (`AllStopped::summaries`).
    pub(super) fn under_stop(registry: &'era Registry) -> Result<Self, RegistrationError> {
        let gate = registry.lock_for_keeper()?;
        Self::from_gate(registry, gate)
    }

    fn from_gate(
        registry: &'era Registry,
        gate: RegistryGuard<'era>,
    ) -> Result<Self, RegistrationError> {
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
            // A peer that enters `Mode::NativeStopped` (or retires from it)
            // before any stop has completed publishes at `completed == 0`
            // (`rendezvous/owner.rs`). Its `publication_generation` is nonzero
            // and its root batch carries `publication_epoch == 0`; stop #1
            // advances `coverage_epoch` while leaving `epoch == 0` so
            // `prepare_finish` keeps matching the batch identity.
            let pre_stop_native = state.rendezvous.mode == rendezvous::Mode::NativeStopped
                && state
                    .roots
                    .as_ref()
                    .is_some_and(|roots| roots.identity().publication_epoch() == 0);
            if state.phase != EntryPhase::Frozen
                && (state.publication_generation == 0 || (state.epoch == 0 && !pre_stop_native))
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

    /// Copied observer for the re-homed leaked-view test (C4 VIS): walk the
    /// view's work families and report the visit count through a plain
    /// `FnMut()`. The `MutatorKey` and `PublishedGcWork` never cross.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn visit_work_count_for_test(
        &self,
        mut visit: impl FnMut(),
    ) -> Result<(), RegistrationError> {
        self.visit_work(|_, _| visit())
    }

    /// Copied observer for the re-homed leaked-view test (C4 VIS): the
    /// young-log lookup outcome for an entry, with the log and the private
    /// young id kept inside value.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_young_entry_for_test(
        &mut self,
        entry: &MutatorEntry,
    ) -> Result<(), RegistrationError> {
        self.with_young(entry.key, entry.young, |_| ())
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
