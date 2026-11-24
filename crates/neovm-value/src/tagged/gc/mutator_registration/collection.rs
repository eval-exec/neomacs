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
use crate::tagged::header::HashTableObj;

#[cfg(all(
    any(test, feature = "test-support"),
    not(feature = "gc-memory-telemetry")
))]
#[path = "weak_policy_tests.rs"]
mod weak_policy_tests;
#[cfg(all(
    any(test, feature = "test-support"),
    not(feature = "gc-memory-telemetry")
))]
pub(crate) use weak_policy_tests::{WeakPolicyCase, WeakPolicyHost, run_weak_policy_case};

#[cfg(all(
    any(test, feature = "test-support"),
    not(feature = "gc-memory-telemetry")
))]
#[path = "stopped_adapter_fixture_tests.rs"]
mod stopped_adapter_fixture_tests;
#[cfg(all(
    any(test, feature = "test-support"),
    not(feature = "gc-memory-telemetry")
))]
pub use stopped_adapter_fixture_tests::{
    StoppedAdapterEphemeronFixture, StoppedAdapterResourceFixture,
    assert_stopped_adapter_ephemerons, assert_stopped_adapter_resources,
    prepare_stopped_adapter_ephemerons, prepare_stopped_adapter_resources,
};

#[cfg(all(test, not(feature = "gc-memory-telemetry")))]
mod threading_payload_tests {
    //! Narrow stopped-collector test access, with no dependency on World code.
    //! Real-World integration proofs are registered by the runtime thread suite.

    use super::{AllStopped, RendezvousError};
    use crate::tagged::header::VecLikeHeader;
    use crate::tagged::value::TaggedValue;

    #[cfg(any(test, feature = "test-support"))]
    impl TaggedValue {
        /// Preserve the collector projection inside a stopped graph check. The
        /// caller checks exact heap membership before dereferencing the raw pointer;
        /// this accessor does not publish through serial observation/TLS machinery.
        pub(crate) fn gc_veclike_ptr_for_stopped_threading_test(
            self,
            _stopped: &AllStopped<'_>,
        ) -> Option<*const VecLikeHeader> {
            self.gc_veclike_ptr()
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    impl AllStopped<'_> {
        /// Acknowledge the no-collection result after the GEN0 minor refusal.
        /// The callback neither inspects the heap nor changes liveness.
        pub(crate) fn acknowledge_refused_minor_for_threading_test(
            &mut self,
        ) -> Result<(), RendezvousError> {
            self.trace(|_, _| Ok(()))
        }
    }
}

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
    /// Frozen owner root words the keeper actually seeded before the mark.
    ///
    /// This isolates the joined-scan contribution from legacy, World, image
    /// and shared-root transport sources in `roots`, and it is counted at the
    /// seed site (not at the read site), so a two-owner join can assert that
    /// BOTH batches were seeded even when a dropped `SharedRoot` would
    /// otherwise keep an object alive for one collection.
    pub frozen_root_words: usize,
    pub cycle: usize,
    /// Last completed runtime-owned heap census, excluding image object
    /// storage while retaining separately owned hash backing.
    pub live_bytes: usize,
    /// Post-sweep global charged live balance, retained canonically in owner 0.
    pub allocated_count: usize,
    pub(crate) memory_use_counts: [u64; MEMORY_USE_COUNT_LEN],
    pub(crate) total_allocated_bytes: u64,
}

#[cfg(feature = "test-support")]
impl CollectionReport {
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn frozen_owners(&self) -> usize {
        self.frozen_owners
    }
}

fn supported(heap: &TaggedHeap) -> Result<(), CollectionError> {
    // E3.3 policy: this optional measurement build takes the DESIGN-allowed
    // explicit serial-only telemetry route. A complete stopped aggregation is
    // the named follow-up; returning partial data is never an option. The
    // typed reason routes the heap to the Baton-serial fallback, and a static
    // test proves gc-memory-telemetry is in no default product feature set.
    if cfg!(feature = "gc-memory-telemetry") {
        return Err(RendezvousError::TelemetryStoppedGcUnavailable);
    }
    // The stopped major runs `finish_stopped_mark_adapters` before the sweep
    // (E3 slice 1), so a nonempty finalizer registry, doomed batch, weak-table
    // list, always-swept list, or pending surface/video drain no longer makes
    // the heap uncollectible: F, W, the three reclaims, and the record-only
    // resource arms all run inside the stop.
    let inspect = |header: *const GcHeader| -> Result<(), CollectionError> {
        // SAFETY: admission or AllStopped retains initialized catalog storage;
        // no mutator/worker, callback, or collection overlaps this preflight.
        unsafe {
            if (*header).kind != HeapObjectKind::VecLike {
                return Ok(());
            }
            let ptr = header.cast::<VecLikeHeader>();
            match (*ptr).type_tag {
                // E3.2: the stopped sweep detaches a user-ptr/module-function
                // payload into the pending-native batch instead of invoking
                // its finalizer under the stopped heap loan; the stopped
                // completion drains it after the loan and before owners
                // resume (the completed-cycle block is the serial backstop),
                // matching GNU's in-sweep ordering. SQLite stays refused
                // until the GNU-observable DB-lifecycle row decides its
                // admission (its explicit drop may close where GNU's
                // cleanup_vector is a no-op).
                VecLikeType::UserPtr | VecLikeType::ModuleFunction => Ok(()),
                VecLikeType::Sqlite => {
                    #[cfg(any(test, feature = "test-support"))]
                    eprintln!(
                        "A5 unsupported callback-bearing payload: {:?}",
                        (*ptr).type_tag
                    );
                    Err(RendezvousError::Publication)
                }
                VecLikeType::HashTable => {
                    // Admission has exclusive serial authority; a stopped job
                    // installs its real AllStopped before reaching this visit.
                    // A weak table is swept by `mark_and_sweep_weak_tables`
                    // inside the stopped major (E3 slice 1), so it needs no
                    // test permit. The structural eligibility below still
                    // refuses a table the stopped sweep cannot own: unhydrated
                    // storage, an active table authority, or a permanent
                    // image-resident shell.
                    // Depth and the authority word are object metadata. Read
                    // them before inspect: a stopped visit's keeper lease
                    // CASes the word to WRITER, so an in-callback load would
                    // see the lease's own bit.
                    let object = ptr.cast::<HashTableObj>();
                    let (depth, authority) = (
                        (*object).test_depth.load(Ordering::Acquire),
                        (*object).authority.load(Ordering::Acquire),
                    );
                    let (weakness, pending_clear) = heap.collector_tables().inspect(
                        ptr.cast_mut().cast(),
                        |ht| (ht.weakness, ht.data.pending_entries().is_none()),
                    );
                    let eligible = pending_clear && depth == 0 && authority == 0;
                    let admitted =
                        (*header).flags.boxed() && !(*header).generation.permanent() && eligible;
                    if weakness.is_some() && !admitted {
                        #[cfg(any(test, feature = "test-support"))]
                        eprintln!("A5 unsupported weak hash payload: {:?}", weakness);
                        Err(RendezvousError::Publication)
                    } else {
                        Ok(())
                    }
                }
                // These own mutable Lisp edges, not just native identities.
                // The keeper job retains its real AllStopped scope across
                // full/minor tracing and sweep; both child dispatches use
                // the concrete payload's exhaustive visit_children schema.
                VecLikeType::Thread | VecLikeType::Mutex | VecLikeType::CondVar => Ok(()),
                // Every admitted non-threading kind is named so a new tag
                // requires an explicit stopped-collection policy decision.
                VecLikeType::Vector
                | VecLikeType::Bignum
                | VecLikeType::Marker
                | VecLikeType::Overlay
                | VecLikeType::SymbolWithPos
                | VecLikeType::Process
                | VecLikeType::Frame
                | VecLikeType::Window
                | VecLikeType::BoolVector
                | VecLikeType::Buffer
                | VecLikeType::Obarray
                | VecLikeType::Terminal
                | VecLikeType::WindowConfiguration
                | VecLikeType::TsNode
                | VecLikeType::Subr
                | VecLikeType::Xwidget
                | VecLikeType::XwidgetView
                | VecLikeType::Lambda
                | VecLikeType::CharTable
                | VecLikeType::SubCharTable
                | VecLikeType::Record
                | VecLikeType::Font
                | VecLikeType::Macro
                | VecLikeType::ByteCode
                | VecLikeType::Timer
                // E3 slice 1: the stopped major queues a finalizer's function
                // (F) and records a dead surface/video handle's id for the
                // post-stop drain; the sweep itself only drops the shell or
                // records the id. No native callback runs inside the stop.
                | VecLikeType::Finalizer
                | VecLikeType::SurfaceHandle
                | VecLikeType::VideoHandle => Ok(()),
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

/// Test-support entry to the stopped admission preflight without constructing
/// a full owner protocol. The accessor stays crate-private; the value-crate
/// collection tests exercise the same `supported` path the keeper uses.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub(super) fn supported_for_test(heap: &TaggedHeap) -> Result<(), CollectionError> {
    supported(heap)
}

impl<'heap, K: EraKeeper> EraCore<'heap, K> {
    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn admit_rooted_with_weak_tables_for_test<
        A: construction::CoreAdmission<'heap, K>,
    >(
        admission: A,
        roots: FrozenLegacyRoots,
        tables: &[TaggedValue],
        prepare: impl FnOnce(
            &K,
        ) -> Result<
            (
                super::super::FacadeEpochRetention,
                symbol_authority::SymbolBacking,
            ),
            RegistrationError,
        >,
    ) -> Result<(Self, A::Evidence), EraAdmissionFailure<'heap, K>> {
        let (mut checked, witness) = admission.into_checked();
        let (collector, keeper) = checked.parts_mut();
        let permission = if roots.heap_identity() != collector.heap_identity() {
            Err(RegistrationError::WrongHeap)
        } else {
            WeakSweepPermit::mint(collector.registration_heap_mut(), keeper, tables).and_then(
                |permit| {
                    supported(collector.registration_heap_mut())
                        .map(|()| permit)
                        .map_err(|_| RegistrationError::Publication)
                },
            )
        };
        let permit = match permission {
            Ok(permit) => permit,
            Err(reason) => return Err(checked.refuse(reason)),
        };
        let (mut era, witness) = Self::admit_checked(
            checked,
            witness,
            prepare,
            #[cfg(any(test, feature = "test-support"))]
            InitialTablePreparation::Actual,
        )?;
        era.weak_sweep = Some(permit.bind(era.registry.as_ref().expect("new era registry")));
        era.legacy_roots = Some(roots);
        Ok((era, witness))
    }

    fn weak_sweep_permit(&self) -> Option<&WeakSweepPermit> {
        #[cfg(any(test, feature = "test-support"))]
        {
            self.weak_sweep.as_ref()
        }
        #[cfg(not(any(test, feature = "test-support")))]
        {
            None
        }
    }

    #[cold]
    pub(super) fn admit_rooted<A: construction::CoreAdmission<'heap, K>>(
        admission: A,
        roots: FrozenLegacyRoots,
        prepare: impl FnOnce(
            &K,
        ) -> Result<
            (
                super::super::FacadeEpochRetention,
                symbol_authority::SymbolBacking,
            ),
            RegistrationError,
        >,
    ) -> Result<(Self, A::Evidence), EraAdmissionFailure<'heap, K>> {
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
        let (mut era, witness) = Self::admit_checked(
            checked,
            witness,
            prepare,
            #[cfg(any(test, feature = "test-support"))]
            InitialTablePreparation::Actual,
        )?;
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

    #[cfg(any(test, feature = "test-support"))]
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

    #[cfg(any(test, feature = "test-support"))]
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
    pub(super) fn collect_stopped(
        &self,
        stopped: &mut AllStopped<'_>,
        roots: impl FnOnce(
            &K,
            &mut AllStopped<'_>,
            &mut Vec<TaggedValue>,
        )
            -> Result<Vec<*mut *mut crate::tagged::header::MarkerObj>, RendezvousError>,
    ) -> Result<CollectionReport, CollectionError> {
        self.collect_stopped_kind(stopped, GenerationCycle::Major, roots)
    }

    /// A genuine all-owner minor, with the same checked current root census,
    /// image transfer, eager promotion/sweep and response restoration as full.
    /// GEN0 refusal happens before entering the terminal trace callback.
    #[cold]
    pub(super) fn collect_stopped_minor(
        &self,
        stopped: &mut AllStopped<'_>,
        roots: impl FnOnce(
            &K,
            &mut AllStopped<'_>,
            &mut Vec<TaggedValue>,
        )
            -> Result<Vec<*mut *mut crate::tagged::header::MarkerObj>, RendezvousError>,
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
        self.collect_stopped_kind(stopped, GenerationCycle::Minor, roots)
            .map_err(MinorCollectionError::Collection)
    }

    #[cold]
    fn collect_stopped_kind(
        &self,
        stopped: &mut AllStopped<'_>,
        kind: GenerationCycle,
        collect_world: impl FnOnce(
            &K,
            &mut AllStopped<'_>,
            &mut Vec<TaggedValue>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
    ) -> Result<CollectionReport, CollectionError> {
        self.check_collection(stopped)?;
        // Return every leased book before root readers, World callbacks, or
        // image transfer. A poisoned allocator cannot return, and that
        // poison is permanent, so it is Publication rather than a Busy
        // the keeper would retry forever.
        {
            let mut collector = self
                .collector
                .try_borrow_mut()
                .map_err(|_| RendezvousError::Busy)?;
            let heap = collector
                .as_mut()
                .ok_or(RendezvousError::Publication)?
                .registration_heap_mut();
            heap.return_leased_owner_cons_reserves();
            if heap.has_owner_cons_reserves() {
                if heap.cons_allocator.is_poisoned() {
                    return Err(RendezvousError::Publication);
                }
                return Err(RendezvousError::Busy);
            }
        }
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
        let heads = collect_world(
            self.keeper.as_ref().ok_or(RendezvousError::Publication)?,
            stopped,
            &mut roots,
        )?;
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
            collector_tables::with_stopped_tables(heap, stopped, |heap| supported(heap))??;
            facade_mark::CollectorQuiescence::check(heap).map_err(|_| RendezvousError::Busy)?;
            crate::tagged::transport::collect_shared_root_gc_roots(heap, &mut roots);
        }
        let root_count = roots.len() + words.len();
        #[cfg(any(test, feature = "test-support"))]
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
                    let job = StoppedCollectorJob::new(heap, proof, guard, registry, kind)?;
                    let mut images = ImageSession::take(registry, &entries, &keys)?;
                    let report =
                        job.run(&mut images, roots, words, heads, root_count, keys.len())?;
                    // Restore publications before releasing the table bridge.
                    images.restore()?;
                    Ok(report)
                },
            )?
        });
        #[cfg(any(test, feature = "test-support"))]
        super::a8_probes::mark(super::a8_probes::MARK_SWEEP_DONE, stopped.epoch().get(), 0);
        // E3.2: the stopped sweep deferred native finalizers; run them now,
        // after the heap loan and image restoration and before this call can
        // return to the owner/Baton side (owners are still stopped). This also
        // clears the deferral mode on the error paths.
        let drained = self.drain_stopped_native_batch();
        let report = traced?;
        drained?;
        Ok(report)
    }

    /// Drain the batch a stopped job detached, on the caller's thread, once
    /// the stopped heap loan has ended. The batch's mode is cleared by the
    /// drain, so a later serial collection reclaims native payloads inline.
    #[cold]
    fn drain_stopped_native_batch(&self) -> Result<usize, RendezvousError> {
        let mut collector = self
            .collector
            .try_borrow_mut()
            .map_err(|_| RendezvousError::Busy)?;
        let heap = collector
            .as_mut()
            .ok_or(RendezvousError::Publication)?
            .registration_heap_mut();
        Ok(heap.drain_pending_native_destroys())
    }
}

/// The unique stopped heap reborrow and its nonforgeable epoch authority.
/// No Deref, raw designator, Send/Sync, cloning, or escaping callback exists.
struct StoppedCollectorJob<'heap, 'stop, 'era> {
    heap: &'heap mut TaggedHeap,
    _proof: &'stop scan_contract::ParallelStoppedMutatorsProof<'stop>,
    _stopped: &'stop AllStopped<'era>,
    epoch: u64,
    kind: GenerationCycle,
}
impl<'heap, 'stop, 'era> StoppedCollectorJob<'heap, 'stop, 'era> {
    fn new(
        heap: &'heap mut TaggedHeap,
        proof: &'stop scan_contract::ParallelStoppedMutatorsProof<'stop>,
        stopped: &'stop AllStopped<'era>,
        registry: &Registry,
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
        // Count at the seed site: this is the number of frozen owner root
        // words the collector actually seeds, not merely the number it read.
        let mut frozen_root_words = 0usize;
        for word in words {
            heap.seed_root(unsafe {
                /* SAFETY: value-internal construction from a real Value */
                TaggedValue::from_bits(word)
            });
            frozen_root_words += 1;
        }
        for item in &images.work {
            trace_work(heap, item);
        }
        // E3.2: the stopped job must not run a native finalizer or SQLite
        // teardown while it holds the stopped heap loan. The sweep detaches
        // such payloads into the heap's pending-native batch; the stopped
        // completion drains it after the loan and before owners resume (the
        // evaluator's completed-cycle block is the serial backstop).
        heap.begin_native_deferral();
        match self.kind {
            GenerationCycle::Major => {
                // This scoped keeper job never sends/clones a live heap borrow.
                // Existing worker MarkAll remains exclusively on its serial path.
                let mark_started = std::time::Instant::now();
                heap.mark_all();
                // E3 slice 1: the same post-mark adapters the serial stop and
                // incremental termination run. Ephemeron fixpoint, F, fixpoint,
                // W, then the three explicit reclaims, all before the sweep.
                // The test `WeakSweepPermit` no longer gates this call, because
                // admission already accepted the registries this job must
                // finish. `finish_stopped_mark_adapters` is the one production
                // order (see the architecture guard).
                let _ = heap.finish_stopped_mark_adapters();
                // The weak fixpoint consumes every table it registered; the
                // serial stop relies on the same post-condition. A leftover
                // list would mean the sweep never reached fixpoint.
                debug_assert!(
                    heap.weak_hash_tables.is_empty(),
                    "the stopped weak sweep must consume every registered weak table"
                );
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
        let generational = heap.tagged_heap_generational_enabled();
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
            frozen_root_words,
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
        heap.push_value_children_to_gray(
            unsafe {
                /* SAFETY: value-internal construction from a real Value */
                TaggedValue::from_bits(bits)
            },
            "parallel-owner-children",
        );
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
