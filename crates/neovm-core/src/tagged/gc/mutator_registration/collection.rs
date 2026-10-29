//! Keeper-only full collection over owned publications. No heap loan escapes.
use super::super::cons_allocator::{CapturedParallelYoung, ParallelYoungLogs};
use super::super::generational::{
    GenerationCycle, MinorCollectionAdmissionError, ParallelMinorAdmissionError,
};
use super::super::young_registry::YoungBlockLog;
use super::super::*;
use super::image::{CollectionHistory, CollectionWork, ResponsePacing};
use super::rendezvous::entry_state;
use super::*;

#[cfg(all(test, not(feature = "gc-memory-telemetry")))]
#[path = "weak_policy_tests.rs"]
mod weak_policy_tests;

pub type CollectionError = RendezvousError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MinorCollectionError {
    #[error("minor collection requires the generational collector")]
    GenerationalDisabled,
    #[error(transparent)]
    Collection(#[from] CollectionError),
}

static_assertions::assert_impl_all!(MinorCollectionError: Send, Sync, Copy);

fn minor_admission(error: ParallelMinorAdmissionError) -> CollectionError {
    match error {
        ParallelMinorAdmissionError::Admission(MinorCollectionAdmissionError::CollectorBusy) => {
            RendezvousError::Busy
        }
        ParallelMinorAdmissionError::Admission(
            MinorCollectionAdmissionError::GenerationalDisabled,
        ) => RendezvousError::Publication,
        ParallelMinorAdmissionError::Capture(error) => error.into(),
    }
}

enum YoungCapture {
    Major,
    Minor(CapturedParallelYoung),
}

impl From<YoungRegistryError> for RendezvousError {
    fn from(error: YoungRegistryError) -> Self {
        RegistrationError::from(error).into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollectionReport {
    pub epoch: u64,
    pub major: bool,
    pub roots: usize,
    pub frozen_owners: usize,
    pub cycle: usize,
    pub live_bytes: usize,
    /// Post-sweep global charged live balance, retained canonically in owner 0.
    pub allocated_count: usize,
    pub memory_use_counts: [u64; MEMORY_USE_COUNT_LEN],
    pub total_allocated_bytes: u64,
}

fn supported(heap: &TaggedHeap) -> Result<(), CollectionError> {
    supported_with_weak(heap, None)
}

fn supported_with_weak(
    heap: &TaggedHeap,
    weak_sweep: Option<&WeakSweepPermit>,
) -> Result<(), CollectionError> {
    // This optional inventory includes private buffer capacities, for which
    // A5 has no all-owner adapter. Refuse admission rather than emit partial data.
    if cfg!(feature = "gc-memory-telemetry") {
        return Err(RendezvousError::Publication);
    }
    if !heap.finalizer_registry.is_empty()
        || !heap.doomed_finalizer_functions.is_empty()
        || !heap.permanent_weak_hash_tables.is_empty()
        || !heap.weak_hash_tables.is_empty()
        || !heap.pending_surface_destroys.is_empty()
        || !heap.pending_video_destroys.is_empty()
    {
        #[cfg(test)]
        eprintln!(
            "A5 unsupported registry: finalizers={} doomed={} permanent_weak={} weak={} surfaces={} videos={}",
            heap.finalizer_registry.len(),
            heap.doomed_finalizer_functions.len(),
            heap.permanent_weak_hash_tables.len(),
            heap.weak_hash_tables.len(),
            heap.pending_surface_destroys.len(),
            heap.pending_video_destroys.len()
        );
        return Err(RendezvousError::Publication);
    }
    let inspect = |header: *const GcHeader| -> Result<(), CollectionError> {
        // SAFETY: admission or AllStopped retains initialized catalog storage;
        // no mutator/worker, callback, or collection overlaps this preflight.
        unsafe {
            if (*header).kind != HeapObjectKind::VecLike {
                return Ok(());
            }
            let ptr = header.cast::<VecLikeHeader>();
            match (*ptr).type_tag {
                VecLikeType::Finalizer
                | VecLikeType::UserPtr
                | VecLikeType::ModuleFunction
                | VecLikeType::Sqlite
                | VecLikeType::SurfaceHandle
                | VecLikeType::VideoHandle => {
                    #[cfg(test)]
                    eprintln!(
                        "A5 unsupported callback-bearing payload: {:?}",
                        (*ptr).type_tag
                    );
                    Err(RendezvousError::Publication)
                }
                VecLikeType::HashTable => {
                    // Admission has exclusive serial authority; a stopped job
                    // installs its real AllStopped before reaching this visit.
                    let (weakness, eligible) =
                        heap.collector_tables()
                            .inspect(ptr.cast_mut().cast(), |ht| {
                                (
                                    ht.weakness,
                                    ht.data.pending_entries().is_none()
                                        && ht.test_depth.load(Ordering::Acquire) == 0
                                        && ht.data.authority.load(Ordering::Acquire) == 0,
                                )
                            });
                    let allowed = weak_sweep.is_some_and(|permit| {
                        permit.covers(heap, header as usize)
                            && (*header).flags.boxed()
                            && !(*header).generation.permanent()
                            && eligible
                    });
                    if weakness.is_some() && !allowed {
                        #[cfg(test)]
                        eprintln!("A5 unsupported weak hash payload: {:?}", weakness);
                        Err(RendezvousError::Publication)
                    } else {
                        Ok(())
                    }
                }
                _ => Ok(()),
            }
        }
    };
    for &address in &heap.non_cons_object_addrs {
        inspect(address as *const GcHeader)?;
    }
    for item in &heap.mapped_veclike_objects {
        inspect(item.header.cast::<GcHeader>())?;
    }
    Ok(())
}

impl<'heap> EraCore<'heap> {
    #[cfg(test)]
    pub(super) fn admit_rooted_with_weak_tables_for_test<A: construction::CoreAdmission<'heap>>(
        admission: A,
        roots: FrozenLegacyRoots,
        tables: &[TaggedValue],
    ) -> Result<(Self, A::Evidence), EraAdmissionFailure<'heap>> {
        let (mut checked, witness) = admission.into_checked();
        let (collector, keeper) = checked.parts_mut();
        let permission = if roots.heap_identity() != collector.heap_identity() {
            Err(RegistrationError::WrongHeap)
        } else {
            WeakSweepPermit::mint(collector.registration_heap_mut(), keeper, tables).and_then(
                |permit| {
                    supported_with_weak(collector.registration_heap_mut(), Some(&permit))
                        .map(|()| permit)
                        .map_err(|_| RegistrationError::Publication)
                },
            )
        };
        let permit = match permission {
            Ok(permit) => permit,
            Err(reason) => return Err(checked.refuse(reason)),
        };
        let (mut era, witness) = Self::admit_checked(checked, witness)?;
        era.weak_sweep = Some(permit.bind(era.registry.as_ref().expect("new era registry")));
        era.legacy_roots = Some(roots);
        Ok((era, witness))
    }

    fn weak_sweep_permit(&self) -> Option<&WeakSweepPermit> {
        #[cfg(test)]
        {
            self.weak_sweep.as_ref()
        }
        #[cfg(not(test))]
        {
            None
        }
    }

    #[cold]
    pub(super) fn admit_rooted<A: construction::CoreAdmission<'heap>>(
        admission: A,
        roots: FrozenLegacyRoots,
    ) -> Result<(Self, A::Evidence), EraAdmissionFailure<'heap>> {
        let (mut checked, witness) = admission.into_checked();
        let (collector, _) = checked.parts_mut();
        let validation = if roots.heap_identity() != collector.heap_identity() {
            Err(RegistrationError::WrongHeap)
        } else {
            supported(collector.registration_heap_mut()).map_err(|_| RegistrationError::Publication)
        };
        if let Err(reason) = validation {
            return Err(checked.refuse(reason));
        }
        let (mut era, witness) = Self::admit_checked(checked, witness)?;
        era.legacy_roots = Some(roots);
        Ok((era, witness))
    }

    fn check_collection(&self, stopped: &AllStopped<'_>) -> Result<(), CollectionError> {
        if !stopped.matches_registry(self.registry()) {
            return Err(RendezvousError::Identity);
        }
        stopped.check_world_roots(self.heap_identity())?;
        if self.legacy_roots.is_none() {
            return Err(RendezvousError::Publication);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn with_stopped_heap_for_test<R>(
        &self,
        stopped: &AllStopped<'_>,
        body: impl for<'h> FnOnce(&'h TaggedHeap) -> R,
    ) -> Result<R, CollectionError> {
        self.check_collection(stopped)?;
        let mut collector = self
            .collector
            .try_borrow_mut()
            .map_err(|_| RendezvousError::Busy)?;
        Ok(body(
            collector
                .as_mut()
                .ok_or(RendezvousError::Publication)?
                .registration_heap_mut(),
        ))
    }

    #[cfg(test)]
    pub(crate) fn shutdown_admission_stopped_for_test(
        &self,
        stopped: &AllStopped<'_>,
    ) -> Result<Result<(), MarkFinishError>, RendezvousError> {
        self.check_collection(stopped)?;
        let mut collector = self
            .collector
            .try_borrow_mut()
            .map_err(|_| RendezvousError::Busy)?;
        Ok(collector
            .as_mut()
            .ok_or(RendezvousError::Publication)?
            .registration_heap_mut()
            .shutdown_admission())
    }

    #[cold]
    pub fn collect_stopped(
        &self,
        stopped: &mut AllStopped<'_>,
    ) -> Result<CollectionReport, CollectionError> {
        self.collect_stopped_kind(stopped, GenerationCycle::Major)
    }

    /// A genuine all-owner minor, with the same checked current root census,
    /// image transfer, eager promotion/sweep and response restoration as full.
    /// GEN0 refusal happens before entering the terminal trace callback.
    #[cold]
    pub fn collect_stopped_minor(
        &self,
        stopped: &mut AllStopped<'_>,
    ) -> Result<CollectionReport, MinorCollectionError> {
        self.check_collection(stopped)?;
        {
            let mut slot = self
                .collector
                .try_borrow_mut()
                .map_err(|_| RendezvousError::Busy)?;
            let heap = slot
                .as_mut()
                .ok_or(RendezvousError::Publication)?
                .registration_heap_mut();
            match heap.check_minor_admission() {
                Ok(()) => {}
                Err(MinorCollectionAdmissionError::CollectorBusy) => {
                    return Err(RendezvousError::Busy.into());
                }
                Err(MinorCollectionAdmissionError::GenerationalDisabled) => {
                    return Err(MinorCollectionError::GenerationalDisabled);
                }
            }
        }
        self.collect_stopped_kind(stopped, GenerationCycle::Minor)
            .map_err(MinorCollectionError::Collection)
    }

    #[cold]
    fn collect_stopped_kind(
        &self,
        stopped: &mut AllStopped<'_>,
        kind: GenerationCycle,
    ) -> Result<CollectionReport, CollectionError> {
        self.check_collection(stopped)?;
        let registry = self.registry();
        let (keys, entries) = {
            let gate = registry.lock()?;
            (gate.rendezvous.frozen.clone(), gate.entries.clone())
        };
        // Refuse a retained/abandoned reader BEFORE any mark or image mutation.
        // Root-table locks never overlap the control gate or the collector job.
        for entry in &entries {
            let state = entry_state(entry)?;
            if let Some(roots) = &state.roots {
                roots.validate_no_readers()?;
            }
            for roots in &state.retained_roots {
                roots.validate_no_readers()?;
            }
        }
        let mut words = Vec::new();
        for key in &keys {
            stopped.with_root_reader(*key, |reader| {
                reader.append_words(&mut words);
                reader.finish()
            })??;
        }
        let mut roots = Vec::new();
        stopped.symbols()?.append_function_roots(&mut roots)?;
        stopped.symbols()?.append_name_roots(&mut roots)?;
        self.legacy_roots
            .as_ref()
            .ok_or(RendezvousError::Publication)?
            .append_words(&mut roots);
        let heads = self
            .keeper
            .as_ref()
            .ok_or(RendezvousError::Publication)?
            .collect_parallel_world_roots(stopped, &mut roots)?;
        {
            let mut collector = self
                .collector
                .try_borrow_mut()
                .map_err(|_| RendezvousError::Busy)?;
            let heap = collector
                .as_mut()
                .ok_or(RendezvousError::Publication)?
                .registration_heap_mut();
            // Preserve the preflight-before-image-transfer boundary. This
            // short stop scope uses the same real guard as the full job below.
            if let Some(permit) = self.weak_sweep_permit() {
                permit.validate_stopped(
                    heap,
                    stopped,
                    self.registry.as_ref().expect("live era registry"),
                )?;
            }
            collector_tables::with_stopped_tables(heap, stopped, |heap| {
                supported_with_weak(heap, self.weak_sweep_permit())
            })??;
            facade_mark::CollectorQuiescence::check(heap).map_err(|_| RendezvousError::Busy)?;
            crate::tagged::transport::collect_shared_root_gc_roots(heap, &mut roots);
        }
        let root_count = roots.len() + words.len();
        #[cfg(test)]
        super::a8_probes::mark(super::a8_probes::ROOT_CENSUS_DONE, stopped.epoch().get(), 0);
        let traced = stopped.trace(|guard, proof| {
            let mut slot = self
                .collector
                .try_borrow_mut()
                .map_err(|_| RendezvousError::Busy)?;
            let collector = slot.as_mut().ok_or(RendezvousError::Publication)?;
            // Validate stop authority while every publication is still in its
            // registry entry. ImageSession::take temporarily removes those
            // images, so a fresh verify_stopped after that transfer must fail.
            // Retain this actual guard through the job and image restoration.
            collector_tables::with_stopped_tables_for_trace(
                collector.registration_heap_mut(),
                guard,
                |heap| {
                    let job = StoppedCollectorJob::new(
                        heap,
                        proof,
                        guard,
                        registry,
                        self.weak_sweep_permit().is_some(),
                        kind,
                    )?;
                    let mut images = ImageSession::take(registry, &entries, &keys)?;
                    let report =
                        job.run(&mut images, roots, words, heads, root_count, keys.len())?;
                    // Restore publications before releasing the table bridge.
                    images.restore()?;
                    Ok(report)
                },
            )?
        });
        #[cfg(test)]
        super::a8_probes::mark(super::a8_probes::MARK_SWEEP_DONE, stopped.epoch().get(), 0);
        traced
    }
}

/// The unique stopped heap reborrow and its nonforgeable epoch authority.
/// No Deref, raw designator, Send/Sync, cloning, or escaping callback exists.
struct StoppedCollectorJob<'heap, 'stop, 'era> {
    heap: &'heap mut TaggedHeap,
    _proof: &'stop scan_contract::ParallelStoppedMutatorsProof<'stop>,
    _stopped: &'stop AllStopped<'era>,
    epoch: u64,
    weak_sweep: bool,
    kind: GenerationCycle,
}
impl<'heap, 'stop, 'era> StoppedCollectorJob<'heap, 'stop, 'era> {
    fn new(
        heap: &'heap mut TaggedHeap,
        proof: &'stop scan_contract::ParallelStoppedMutatorsProof<'stop>,
        stopped: &'stop AllStopped<'era>,
        registry: &Registry,
        weak_sweep: bool,
        kind: GenerationCycle,
    ) -> Result<Self, CollectionError> {
        if !stopped.matches_registry(registry) || heap.heap_identity() != registry.heap {
            return Err(RendezvousError::Identity);
        }
        if registry.keeper_thread != std::thread::current().id() {
            return Err(RegistrationError::WrongThread.into());
        }
        Ok(Self {
            heap,
            _proof: proof,
            _stopped: stopped,
            epoch: stopped.epoch().get(),
            weak_sweep,
            kind,
        })
    }
    fn run(
        self,
        images: &mut ImageSession<'_>,
        roots: Vec<TaggedValue>,
        words: Vec<usize>,
        heads: Vec<*mut *mut MarkerObj>,
        root_count: usize,
        frozen_owners: usize,
    ) -> Result<CollectionReport, CollectionError> {
        let epoch = self.epoch;
        let heap = self.heap;
        images.work = images
            .images
            .iter_mut()
            .map(|(_, image)| {
                image
                    .as_mut()
                    .expect("exported image")
                    .take_collection_work()
            })
            .collect();
        // Recover the serial owner's original young payload before any
        // legacy collector method asks its inline registry to resolve it.
        let legacy = images.images[0].1.take().expect("frozen serial image");
        if let Err(failure) = legacy.restore_legacy(heap) {
            images.images[0].1 = Some(failure.image);
            return Err(RegistrationError::from(failure.error).into());
        }
        let history = CollectionHistory::save(heap);
        for (_, image) in &images.images[1..] {
            image
                .as_ref()
                .expect("published peer")
                .add_collection_scalars(heap);
        }
        let captured = capture_young(heap, images, self.kind)?;
        for item in &images.work {
            heap.mapped_remembered.extend(item.mapped.iter().copied());
        }
        let bytes_before = heap.live_bytes();
        let started = std::time::Instant::now();
        match captured {
            YoungCapture::Major => heap.begin_stw_collection(),
            YoungCapture::Minor(capture) => heap
                .begin_parallel_minor_collection(capture, self._proof)
                .map_err(minor_admission)?,
        }
        // SAFETY: the matching Baton produced stable frozen World slots;
        // all buffer/owner borrows ended before the complete stopped job.
        unsafe {
            heap.set_marker_chain_head_slots(heads);
        }
        for root in roots {
            heap.seed_root(root);
        }
        for word in words {
            heap.seed_root(TaggedValue::from_bits(word));
        }
        for item in &images.work {
            trace_work(heap, item);
        }
        match self.kind {
            GenerationCycle::Major => {
                // This scoped keeper job never sends/clones a live heap borrow.
                // Existing worker MarkAll remains exclusively on its serial path.
                let mark_started = std::time::Instant::now();
                heap.mark_all();
                if self.weak_sweep {
                    heap.mark_and_sweep_weak_tables();
                }
                if !heap.weak_hash_tables.is_empty() {
                    return Err(RendezvousError::Publication);
                }
                heap.finalize_collection(
                    mark_started.elapsed().as_micros() as u64,
                    bytes_before,
                    started,
                );
            }
            GenerationCycle::Minor => {
                heap.complete_minor_collection();
                heap.finish_incremental_sweep_now();
            }
        }
        facade_mark::CollectorQuiescence::check(heap).map_err(|_| RendezvousError::Publication)?;
        let allocated_count = heap.current_mutator_gc().allocated_count;
        let memory_use_counts = heap.memory_use_counts_snapshot();
        let total_allocated_bytes = heap.total_allocated_bytes();
        let generational = heap.generational_enabled();
        history.restore(heap, generational);
        let response_pacing = if generational {
            ResponsePacing::Reset
        } else {
            ResponsePacing::Preserve
        };
        images.images[0].1 =
            Some(PublishedGcImage::from_legacy(heap).map_err(RegistrationError::from)?);
        for (_, image) in &mut images.images[1..] {
            image
                .as_mut()
                .expect("published peer")
                .finish_collection_response(response_pacing);
        }
        let image_words: usize = images
            .work
            .iter()
            .map(|work| {
                work.roots.len()
                    + work.remembered.len()
                    + work.cons_writes.len()
                    + work.mapped.len()
                    + work.written_owners.len()
            })
            .sum();
        let report = CollectionReport {
            epoch,
            major: match self.kind {
                GenerationCycle::Major => true,
                GenerationCycle::Minor => false,
            },
            roots: root_count + image_words,
            frozen_owners,
            cycle: heap.gc_collections(),
            live_bytes: heap.live_bytes(),
            allocated_count,
            memory_use_counts,
            total_allocated_bytes,
        };
        Ok(report)
    }
}
static_assertions::assert_not_impl_any!(StoppedCollectorJob<'static, 'static, 'static>: Send, Sync, Clone, Copy);

struct ImageSession<'a> {
    registry: &'a Registry,
    images: Vec<(Arc<MutatorEntry>, Option<PublishedGcImage>)>,
    work: Vec<CollectionWork>,
    restored: bool,
}
impl<'a> ImageSession<'a> {
    fn take(
        registry: &'a Registry,
        entries: &[Arc<MutatorEntry>],
        frozen: &[MutatorKey],
    ) -> Result<Self, CollectionError> {
        let gate = registry.lock_blocking()?;
        let selected: Vec<_> = entries
            .iter()
            .filter(|entry| entry.key.owner == 0 || frozen.contains(&entry.key))
            .cloned()
            .collect();
        let mut guards = Vec::new();
        for entry in &selected {
            let state = entry_state(entry)?;
            if state.image.is_none() {
                return Err(RendezvousError::Publication);
            }
            guards.push(state);
        }
        let mut images = Vec::with_capacity(selected.len());
        for (entry, state) in selected.iter().zip(&mut guards) {
            images.push((Arc::clone(entry), state.image.take()));
        }
        drop(guards);
        drop(gate);
        Ok(Self {
            registry,
            images,
            work: Vec::new(),
            restored: false,
        })
    }
    fn restore(&mut self) -> Result<(), CollectionError> {
        // The complete collector job owns no control, entry, allocator or
        // root-table lock here. Public requesters may briefly hold the gate;
        // that ordinary contention cannot invalidate a completed collection.
        let _gate = self.registry.lock_blocking()?;
        let entries: Vec<_> = self
            .images
            .iter()
            .map(|(entry, _)| Arc::clone(entry))
            .collect();
        let mut guards = Vec::new();
        for (entry, (_, image)) in entries.iter().zip(&self.images) {
            let state = entry_state(entry)?;
            if state.image.is_some() || image.is_none() {
                return Err(RendezvousError::Publication);
            }
            guards.push(state);
        }
        for (state, (_, image)) in guards.iter_mut().zip(&mut self.images) {
            state.image = image.take();
        }
        self.restored = true;
        Ok(())
    }
}
impl Drop for ImageSession<'_> {
    fn drop(&mut self) {
        if !self.restored {
            self.registry.poison();
            for (_, image) in &mut self.images {
                if let Some(image) = image.take() {
                    std::mem::forget(image);
                }
            }
            std::mem::forget(std::mem::take(&mut self.work));
        }
    }
}

struct YoungImages<'a> {
    heap: HeapIdentity,
    ids: Vec<YoungRegistrationId>,
    legacy: &'a mut YoungBlockLog,
    peers: &'a mut [(Arc<MutatorEntry>, Option<PublishedGcImage>)],
}
impl ParallelYoungLogs for YoungImages<'_> {
    fn heap_identity(&self) -> HeapIdentity {
        self.heap
    }
    fn registrations(&self) -> &[YoungRegistrationId] {
        &self.ids
    }
    fn resolve_mut(
        &mut self,
        id: YoungRegistrationId,
    ) -> Result<&mut YoungBlockLog, YoungRegistryError> {
        if id.heap_identity() != self.heap {
            return Err(YoungRegistryError::WrongHeap);
        }
        if self.legacy.id() == id {
            return Ok(self.legacy);
        }
        self.peers
            .iter_mut()
            .find(|(entry, _)| entry.young == id)
            .and_then(|(_, image)| image.as_mut())
            .map(|image| image.young_mut())
            .ok_or(YoungRegistryError::UnknownRegistration)
    }
}

fn capture_young(
    heap: &mut TaggedHeap,
    images: &mut ImageSession<'_>,
    kind: GenerationCycle,
) -> Result<YoungCapture, CollectionError> {
    let ids = images.images.iter().map(|(entry, _)| entry.young).collect();
    let identity = heap.heap_identity();
    let heap = &mut *heap;
    // SAFETY: AllStopped and the unique collector RefCell loan exclude every
    // allocator/worker access; the split covers disjoint fields only.
    let mut stopped = unsafe { heap.cons_allocator.stopped_world() };
    let legacy = heap._satb_layout_reserve.current_young_log_mut()?;
    let mut logs = YoungImages {
        heap: identity,
        ids,
        legacy,
        peers: &mut images.images[1..],
    };
    if heap.generational.enabled {
        match kind {
            GenerationCycle::Major => {
                stopped.capture_parallel_major(&mut logs)?;
                Ok(YoungCapture::Major)
            }
            GenerationCycle::Minor => Ok(YoungCapture::Minor(
                stopped.capture_parallel_minor(&mut logs)?,
            )),
        }
    } else {
        for id in logs.ids.clone() {
            let log = logs.resolve_mut(id)?;
            if !log.blocks().is_empty() || log.pending_index().is_some() {
                return Err(RendezvousError::Publication);
            }
        }
        match kind {
            GenerationCycle::Major => Ok(YoungCapture::Major),
            GenerationCycle::Minor => Err(RendezvousError::Publication),
        }
    }
}

fn trace_work(heap: &mut TaggedHeap, work: &CollectionWork) {
    for word in work
        .roots
        .iter()
        .chain(&work.remembered)
        .chain(&work.cons_writes)
    {
        heap.seed_root(word.value());
    }
    for word in &work.remembered {
        let owner = word.value();
        if !owner.is_cons() && !heap.owner_is_mapped(owner) {
            if let Some(address) = TaggedHeap::value_heap_addr(owner) {
                // SAFETY: the pending remembered owner remains retained until
                // this stopped major consumes its claim, before any sweep.
                unsafe { &*(address as *const GcHeader) }
                    .remembered
                    .store(RememberedState::Unlogged as u8, Ordering::Relaxed);
            }
        }
    }
    for word in work.remembered.iter().chain(&work.cons_writes) {
        heap.push_value_children_to_gray(word.value(), "parallel-written-owner");
    }
    for &bits in work.mapped.iter().chain(&work.written_owners) {
        heap.push_value_children_to_gray(TaggedValue::from_bits(bits), "parallel-owner-children");
    }
    for &id in &work.symbols {
        heap.mark_symbol(id);
    }
    for &address in &work.black_headers {
        let header = address as *mut GcHeader;
        // SAFETY: the typed birth log retains initialized owned storage until
        // this complete stopped mark/sweep. Never decode an address as a word.
        let owner = unsafe {
            match (*header).kind {
                HeapObjectKind::String => TaggedValue::from_string_ptr(header.cast()),
                HeapObjectKind::Float => TaggedValue::from_float_ptr(header.cast()),
                HeapObjectKind::VecLike => TaggedValue::from_veclike_ptr(header.cast()),
            }
        };
        heap.seed_root(owner);
    }
    for region in &work.black_regions {
        for owner in region.cons_owners() {
            heap.seed_root(owner);
        }
        for header in region.float_headers() {
            // SAFETY: the closed descriptor covers initialized float slots.
            heap.seed_root(unsafe { TaggedValue::from_float_ptr(header.cast()) });
        }
    }
}

#[cfg(all(test, not(feature = "gc-memory-telemetry")))]
mod keeper_table_scope_tests {
    use super::*;
    use crate::emacs_core::eval::Context;

    #[test]
    fn keeper_table_scope_guard_test_clears_on_unwind_and_refuses_nested_install() {
        Context::with_test_collection_era(
            |context| {
                use crate::emacs_core::value::{HashTableTest, LispHashTable};
                let table = context
                    .tagged_heap
                    .alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
                context.set_variable("p2ht-keeper-scope-root", table);
                table.bits()
            },
            |era, table_bits| {
                let mut stopped = era.stop().unwrap();
                {
                    let mut slot = era.core.collector.try_borrow_mut().unwrap();
                    let heap = slot.as_mut().unwrap().registration_heap_mut();
                    assert!(heap._satb_layout_reserve.collector_tables().is_null());
                    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        collector_tables::with_stopped_tables(heap, &stopped, |heap| {
                            assert!(!heap._satb_layout_reserve.collector_tables().is_null());
                            assert!(matches!(
                                collector_tables::with_stopped_tables(heap, &stopped, |_| ()),
                                Err(RendezvousError::Busy)
                            ));
                            let table = TaggedValue::from_bits(table_bits);
                            let ptr = table.gc_veclike_ptr().unwrap().cast_mut().cast();
                            let lease = KeeperTableMut::enter(&stopped, table).unwrap();
                            // Simulate a leaked reader while all real owners are
                            // stopped. The installed adapter must mint a keeper
                            // lease and refuse before invoking its callback.
                            for word in [1, super::super::table_authority::WRITER] {
                                lease.authority_word().store(word, Ordering::Release);
                                let rejected =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        // SAFETY: rooted table and active real stop scope.
                                        unsafe { heap.collector_tables().inspect(ptr, |_| ()) };
                                    }));
                                lease.authority_word().store(0, Ordering::Release);
                                assert!(
                                    rejected.is_err(),
                                    "active collector read bypassed keeper mint"
                                );
                            }
                            // SAFETY: same rooted table and still-active stop scope.
                            unsafe {
                                heap.collector_tables()
                                    .inspect(ptr, |ht| assert!(ht.data.is_empty()))
                            };
                            panic!("exercise scoped bridge cleanup");
                        })
                        .unwrap();
                    }));
                    assert!(panic.is_err());
                    assert!(heap._satb_layout_reserve.collector_tables().is_null());
                }
                // The cleaned bridge allows a subsequent real collector job.
                era.collect_stopped(&mut stopped).unwrap();
                stopped.complete().unwrap();
                let (collector, keeper) = era.finish().unwrap();
                drop(collector);
                drop(keeper);
            },
        );
    }
}
