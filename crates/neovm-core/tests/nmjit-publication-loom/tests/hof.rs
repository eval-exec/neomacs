//! HOF guard/selection protocol, not engine or generated-code execution.
//! The counted symbol word excludes writers across both snapshots. Mapper and
//! callback updates are SEPARATE atomic primitives, never one invented fset.
mod support;

use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::sync::{Arc, Mutex};
use loom::thread;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mapper {
    Canonical,
    Redefined,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CallbackVersion(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DefinitionId(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CallbackSnapshot {
    version: CallbackVersion,
    definition: DefinitionId,
}
impl CallbackSnapshot {
    const OLD: Self = Self {
        version: CallbackVersion(1),
        definition: DefinitionId(0),
    };
    const NEW: Self = Self {
        version: CallbackVersion(2),
        definition: DefinitionId(1),
    };

    fn check(self) {
        assert!(
            self == Self::OLD || self == Self::NEW,
            "incoherent callback snapshot"
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReaderCount(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActivationCount(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RootCount(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WriterState {
    Free,
    Held,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refusal {
    Busy,
}
#[derive(Clone, Copy)]
enum LeaseProtocol {
    SameReader,
    SplitReaders,
}
#[derive(Clone, Copy)]
enum RootTransfer {
    BeforeReaderRelease,
    AfterReaderRelease,
}
#[derive(Clone, Copy)]
enum RegistrationCheck {
    PreparedStamp,
    IgnorePreparedStamp,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegistrationEpoch(usize);
impl RegistrationEpoch {
    const ORIGINAL: Self = Self(1);
    const REPLACED: Self = Self(2);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeSemantics {
    Canonical,
    Replaced,
}
#[derive(Clone, Copy)]
struct NativeMetadata {
    semantics: NativeSemantics,
    generation: RegistrationEpoch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreparedBuiltin {
    registration: RegistrationEpoch,
    // Specification oracle for the semantic identity copied under the guard,
    // not another runtime metadata dereference or an engine receipt field.
    copied_generation: RegistrationEpoch,
}
#[derive(Clone, Copy)]
enum PreparationProtocol {
    BracketMetadataCopy,
    PostloadOnly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreparationOutcome {
    Prepared(PreparedBuiltin),
    GenerationChanged {
        before: RegistrationEpoch,
        after: RegistrationEpoch,
    },
    NonCanonical,
}
#[derive(Clone, Copy)]
enum Update {
    RedefineMapper,
    RedefineCallback,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateOutcome {
    Committed,
    Busy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionOutcome {
    Selected(CallbackSnapshot),
    ContractViolation(SelectionViolation),
    MapperRefused,
    RegistrationChanged,
    Busy,
}
#[derive(Clone, Copy)]
enum LeaseState {
    Active,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionViolation {
    MixedBuiltinAndCallback,
    UnrootedCallback,
    StaleNativeRegistration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootRelease {
    Rooted,
    Missing,
}

/// Report only after every actor has joined and the runtime has been dropped.
/// A named negative must not unwind a live modeled RAII activation or reader.
fn assert_selection_contract(outcome: SelectionOutcome) {
    match outcome {
        SelectionOutcome::Selected(snapshot) => snapshot.check(),
        SelectionOutcome::MapperRefused
        | SelectionOutcome::RegistrationChanged
        | SelectionOutcome::Busy => {}
        SelectionOutcome::ContractViolation(violation) => match violation {
            SelectionViolation::MixedBuiltinAndCallback => {
                panic!("HOF_MIXED_BUILTIN_AND_CALLBACK")
            }
            SelectionViolation::UnrootedCallback => panic!("HOF_UNROOTED_CALLBACK_SELECTION"),
            SelectionViolation::StaleNativeRegistration => panic!("HOF_STALE_NATIVE_REGISTRATION"),
        },
    }
}

struct State {
    readers: ReaderCount,
    writer: WriterState,
    in_slice: ActivationCount,
    mapper: Mapper,
    callback: CallbackSnapshot,
    private_roots: [RootCount; 2],
}
struct Runtime {
    state: Mutex<State>,
    native_metadata: Mutex<NativeMetadata>,
    native_clock: AtomicUsize,
}
fn runtime() -> Arc<Runtime> {
    Arc::new(Runtime {
        state: Mutex::new(State {
            readers: ReaderCount(0),
            writer: WriterState::Free,
            in_slice: ActivationCount(0),
            mapper: Mapper::Canonical,
            callback: CallbackSnapshot::OLD,
            private_roots: [RootCount(0); 2],
        }),
        native_metadata: Mutex::new(NativeMetadata {
            semantics: NativeSemantics::Canonical,
            generation: RegistrationEpoch::ORIGINAL,
        }),
        native_clock: AtomicUsize::new(RegistrationEpoch::ORIGINAL.0),
    })
}

fn prepare_builtin(runtime: &Runtime) -> PreparedBuiltin {
    // Existing selection cases prepare before starting any concurrent actor.
    // They still use the engine-shaped bracketing transaction, not a single
    // epoch read inside the metadata guard.
    match prepare(runtime, PreparationProtocol::BracketMetadataCopy) {
        PreparationOutcome::Prepared(prepared) => prepared,
        PreparationOutcome::GenerationChanged { .. } | PreparationOutcome::NonCanonical => {
            panic!("uncontended initial preparation refused")
        }
    }
}

fn prepare(runtime: &Runtime, protocol: PreparationProtocol) -> PreparationOutcome {
    let before = RegistrationEpoch(runtime.native_clock.load(Ordering::Acquire));
    // Preparation alone copies native metadata under its existing guard. The
    // guard excludes metadata COPY races; clock publication can occur after
    // the registrar releases it. Metadata here is an enum/identity abstraction,
    // not an entry pointer, lifetime proof or actual registry implementation.
    let copied = *runtime.native_metadata.lock().unwrap();
    let after = RegistrationEpoch(runtime.native_clock.load(Ordering::Acquire));
    let registration = match protocol {
        PreparationProtocol::BracketMetadataCopy => {
            if before != after {
                return PreparationOutcome::GenerationChanged { before, after };
            }
            before
        }
        PreparationProtocol::PostloadOnly => after,
    };
    match copied.semantics {
        NativeSemantics::Canonical => PreparationOutcome::Prepared(PreparedBuiltin {
            registration,
            copied_generation: copied.generation,
        }),
        NativeSemantics::Replaced => PreparationOutcome::NonCanonical,
    }
}

fn replace_native_registration(runtime: &Runtime) {
    // This is NOT a symbol writer. A live counted reader cannot exclude it.
    // The semantic commit linearizes at clock publication, not at the earlier
    // metadata edit. Running selection never dereferences native entry metadata.
    {
        let mut metadata = runtime.native_metadata.lock().unwrap();
        *metadata = NativeMetadata {
            semantics: NativeSemantics::Replaced,
            generation: RegistrationEpoch::REPLACED,
        };
    }
    // Match the engine: metadata write guard is released BEFORE this store.
    // A symbol reader does not synchronize either the guard or this window.
    runtime
        .native_clock
        .store(RegistrationEpoch::REPLACED.0, Ordering::Release);
}

// The mutex makes each modeled word operation and protected field access
// explicit. It is not held for the lease duration: the reader COUNT excludes
// writer acquisition between mapper validation and callback/root selection.
#[must_use]
struct Reader {
    runtime: Arc<Runtime>,
    state: LeaseState,
    selected: Option<CallbackSnapshot>,
}
impl Reader {
    fn acquire(runtime: &Arc<Runtime>) -> Result<Self, Refusal> {
        let mut state = runtime.state.lock().unwrap();
        match state.writer {
            WriterState::Held => return Err(Refusal::Busy),
            WriterState::Free => state.readers.0 += 1,
        }
        Ok(Self {
            runtime: runtime.clone(),
            state: LeaseState::Active,
            selected: None,
        })
    }

    fn mapper(&self) -> Mapper {
        let state = self.runtime.state.lock().unwrap();
        assert_eq!(state.writer, WriterState::Free);
        state.mapper
    }

    fn select(&mut self) -> (Mapper, CallbackSnapshot) {
        let state = self.runtime.state.lock().unwrap();
        assert_eq!(state.writer, WriterState::Free);
        assert!(state.in_slice.0 > 0);
        let snapshot = state.callback;
        snapshot.check();
        self.selected = Some(snapshot);
        // The mapper at THIS selection instant is the specification oracle.
        // A previously validated canonical mapper cannot justify a selection
        // made after that mapper has been redefined in a split-lease gap.
        (state.mapper, snapshot)
    }

    fn release(mut self, activation: &Activation) -> RootRelease {
        let release = match self.selected {
            None => RootRelease::Rooted,
            Some(selected) if activation.home == Some(selected) => RootRelease::Rooted,
            Some(_) => RootRelease::Missing,
        };
        self.leave();
        // Record at release, but assert only AFTER modeled RAII cleanup and
        // all actor joins. This is the stronger root-before-reader-release
        // contract, NOT a claim that GC can stop an IN_SLICE owner here.
        release
    }

    fn leave(&mut self) {
        match self.state {
            LeaseState::Active => {
                self.runtime.state.lock().unwrap().readers.0 -= 1;
                self.state = LeaseState::Released;
            }
            LeaseState::Released => {}
        }
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        self.leave();
    }
}

#[must_use]
struct Writer {
    runtime: Arc<Runtime>,
}
impl Writer {
    fn acquire(runtime: &Arc<Runtime>) -> Result<Self, Refusal> {
        let mut state = runtime.state.lock().unwrap();
        match (state.writer, state.readers) {
            (WriterState::Free, ReaderCount(0)) => state.writer = WriterState::Held,
            (WriterState::Free, ReaderCount(_)) | (WriterState::Held, _) => {
                return Err(Refusal::Busy);
            }
        }
        Ok(Self {
            runtime: runtime.clone(),
        })
    }

    fn update(&self, update: Update) {
        let mut state = self.runtime.state.lock().unwrap();
        assert_eq!(state.writer, WriterState::Held);
        assert_eq!(state.readers, ReaderCount(0));
        match update {
            Update::RedefineMapper => state.mapper = Mapper::Redefined,
            Update::RedefineCallback => state.callback = CallbackSnapshot::NEW,
        }
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        self.runtime.state.lock().unwrap().writer = WriterState::Free;
    }
}

// This separate activation remains IN_SLICE even across a deliberately broken
// reader gap. A private home, rather than another owner's root of the same
// definition, must receive this exact selected snapshot before reader release.
struct Activation {
    runtime: Arc<Runtime>,
    home: Option<CallbackSnapshot>,
}
impl Activation {
    fn enter(runtime: &Arc<Runtime>) -> Self {
        runtime.state.lock().unwrap().in_slice.0 += 1;
        Self {
            runtime: runtime.clone(),
            home: None,
        }
    }

    fn install(&mut self, selected: CallbackSnapshot) {
        assert_eq!(self.home, None);
        self.runtime.state.lock().unwrap().private_roots[selected.definition.0].0 += 1;
        self.home = Some(selected);
    }
}
impl Drop for Activation {
    fn drop(&mut self) {
        let mut state = self.runtime.state.lock().unwrap();
        if let Some(selected) = self.home {
            state.private_roots[selected.definition.0].0 -= 1;
        }
        state.in_slice.0 -= 1;
    }
}

fn select(
    runtime: &Arc<Runtime>,
    prepared: PreparedBuiltin,
    lease: LeaseProtocol,
    transfer: RootTransfer,
    registration: RegistrationCheck,
) -> SelectionOutcome {
    let mut activation = Activation::enter(runtime);
    let mut reader = match Reader::acquire(runtime) {
        Ok(reader) => reader,
        Err(Refusal::Busy) => return SelectionOutcome::Busy,
    };
    match reader.mapper() {
        Mapper::Canonical => {}
        Mapper::Redefined => {
            reader.release(&activation);
            return SelectionOutcome::MapperRefused;
        }
    }
    match lease {
        LeaseProtocol::SameReader => {}
        LeaseProtocol::SplitReaders => {
            reader.release(&activation);
            reader = match Reader::acquire(runtime) {
                Ok(reader) => reader,
                Err(Refusal::Busy) => return SelectionOutcome::Busy,
            };
        }
    }
    // The SAME reader still protects mapper/callback cells during this load.
    // It does not protect native metadata. The Acquire observation is this
    // selection's native-registration linearization point: a concurrent later
    // publication can legally follow an old selection. Do not compare a later
    // clock load and retroactively invalidate that selection.
    let observed = RegistrationEpoch(runtime.native_clock.load(Ordering::Acquire));
    match registration {
        RegistrationCheck::PreparedStamp => {
            if observed != prepared.registration {
                reader.release(&activation);
                return SelectionOutcome::RegistrationChanged;
            }
        }
        // The load remains only the specification oracle in this negative;
        // omitting/refusing to use its check is the deliberately broken edge.
        RegistrationCheck::IgnorePreparedStamp => {}
    }
    let (mapper_at_selection, selected) = reader.select();
    let release = match transfer {
        RootTransfer::BeforeReaderRelease => {
            activation.install(selected);
            reader.release(&activation)
        }
        RootTransfer::AfterReaderRelease => {
            let release = reader.release(&activation);
            activation.install(selected);
            release
        }
    };
    assert_eq!(activation.home, Some(selected));
    selected.check();
    if mapper_at_selection != Mapper::Canonical {
        SelectionOutcome::ContractViolation(SelectionViolation::MixedBuiltinAndCallback)
    } else if release == RootRelease::Missing {
        SelectionOutcome::ContractViolation(SelectionViolation::UnrootedCallback)
    } else if observed != prepared.registration {
        SelectionOutcome::ContractViolation(SelectionViolation::StaleNativeRegistration)
    } else {
        SelectionOutcome::Selected(selected)
    }
}

fn update(runtime: &Arc<Runtime>, update: Update) -> UpdateOutcome {
    match Writer::acquire(runtime) {
        Ok(writer) => {
            // These modeled mutex operations already expose optional switches.
            // A forced yield while WRITER is held would make a gap reader run
            // early, see Busy, and suppress the completed-writer counterexample.
            writer.update(update);
            UpdateOutcome::Committed
        }
        Err(Refusal::Busy) => UpdateOutcome::Busy,
    }
}

fn redefine(runtime: &Arc<Runtime>) {
    // There is a release/reacquire between these distinct symbol primitives.
    // Either may independently refuse Busy; there is no retry or assumption
    // that successful mapper redefinition implies callback update succeeds.
    let _mapper_outcome = update(runtime, Update::RedefineMapper);
    let _callback_outcome = update(runtime, Update::RedefineCallback);
}

fn check_quiescent(runtime: &Runtime) {
    let state = runtime.state.lock().unwrap();
    assert_eq!(state.readers, ReaderCount(0));
    assert_eq!(state.writer, WriterState::Free);
    assert_eq!(state.in_slice, ActivationCount(0));
    assert_eq!(state.private_roots, [RootCount(0); 2]);
}

#[test]
fn same_counted_reader_covers_mapper_callback_and_private_root() {
    support::check(3, || {
        let runtime = runtime();
        let prepared = prepare_builtin(&runtime);
        let mut readers = Vec::new();
        for _ in 0..2 {
            let runtime = runtime.clone();
            readers.push(thread::spawn(move || {
                select(
                    &runtime,
                    prepared,
                    LeaseProtocol::SameReader,
                    RootTransfer::BeforeReaderRelease,
                    RegistrationCheck::PreparedStamp,
                )
            }));
        }
        redefine(&runtime);
        let outcomes: Vec<_> = readers.into_iter().map(support::join).collect();
        check_quiescent(&runtime);
        drop(runtime);
        for outcome in outcomes {
            assert_selection_contract(outcome);
        }
    });
}

#[test]
fn negative_split_readers_mix_mapper_guard_and_callback_selection() {
    support::negative(3, "HOF_MIXED_BUILTIN_AND_CALLBACK", || {
        let runtime = runtime();
        let prepared = prepare_builtin(&runtime);
        let writer = {
            let runtime = runtime.clone();
            thread::spawn(move || redefine(&runtime))
        };
        let outcome = select(
            &runtime,
            prepared,
            LeaseProtocol::SplitReaders,
            RootTransfer::BeforeReaderRelease,
            RegistrationCheck::PreparedStamp,
        );
        support::join(writer);
        check_quiescent(&runtime);
        drop(runtime);
        assert_selection_contract(outcome);
    });
}

#[test]
fn negative_reader_release_precedes_private_root_contract() {
    support::negative(3, "HOF_UNROOTED_CALLBACK_SELECTION", || {
        let runtime = runtime();
        let prepared = prepare_builtin(&runtime);
        let writer = {
            let runtime = runtime.clone();
            thread::spawn(move || redefine(&runtime))
        };
        let outcome = select(
            &runtime,
            prepared,
            LeaseProtocol::SameReader,
            RootTransfer::AfterReaderRelease,
            RegistrationCheck::PreparedStamp,
        );
        support::join(writer);
        check_quiescent(&runtime);
        drop(runtime);
        assert_selection_contract(outcome);
    });
}

#[test]
fn registration_clock_change_refuses_stale_prepared_builtin() {
    support::check(3, || {
        let runtime = runtime();
        let prepared = prepare_builtin(&runtime);
        let registrar = {
            let runtime = runtime.clone();
            thread::spawn(move || replace_native_registration(&runtime))
        };
        let writer = {
            let runtime = runtime.clone();
            thread::spawn(move || update(&runtime, Update::RedefineCallback))
        };
        let outcome = select(
            &runtime,
            prepared,
            LeaseProtocol::SameReader,
            RootTransfer::BeforeReaderRelease,
            RegistrationCheck::PreparedStamp,
        );
        support::join(registrar);
        let _legal_writer_outcome = support::join(writer);
        // Joined publication happens before this fresh acquisition. Refusal is
        // now mandatory; no metadata dereference or symbol-writer exclusion is
        // used to establish the stale-registration outcome.
        let after_registration = select(
            &runtime,
            prepared,
            LeaseProtocol::SameReader,
            RootTransfer::BeforeReaderRelease,
            RegistrationCheck::PreparedStamp,
        );
        check_quiescent(&runtime);
        drop(runtime);
        assert_selection_contract(outcome);
        assert_ne!(outcome, SelectionOutcome::MapperRefused);
        assert_eq!(after_registration, SelectionOutcome::RegistrationChanged);
    });
}

#[test]
fn negative_ignored_stamp_accepts_stale_native_registration() {
    support::negative(3, "HOF_STALE_NATIVE_REGISTRATION", || {
        let runtime = runtime();
        let prepared = prepare_builtin(&runtime);
        let registrar = {
            let runtime = runtime.clone();
            thread::spawn(move || replace_native_registration(&runtime))
        };
        let writer = {
            let runtime = runtime.clone();
            thread::spawn(move || update(&runtime, Update::RedefineCallback))
        };
        support::join(registrar);
        let outcome = select(
            &runtime,
            prepared,
            LeaseProtocol::SameReader,
            RootTransfer::BeforeReaderRelease,
            RegistrationCheck::IgnorePreparedStamp,
        );
        let _legal_writer_outcome = support::join(writer);
        check_quiescent(&runtime);
        drop(runtime);
        assert_selection_contract(outcome);
    });
}

#[test]
fn bracketed_preparation_refuses_changed_registration_generation() {
    support::check(3, || {
        let runtime = runtime();
        let registrar = {
            let runtime = runtime.clone();
            thread::spawn(move || replace_native_registration(&runtime))
        };
        let outcome = prepare(&runtime, PreparationProtocol::BracketMetadataCopy);
        support::join(registrar);
        check_quiescent(&runtime);
        drop(runtime);
        match outcome {
            PreparationOutcome::Prepared(prepared) => {
                // An original receipt may precede a later publication; compare
                // its copied identity, never a subsequently loaded clock.
                assert_eq!(prepared.registration, prepared.copied_generation);
            }
            PreparationOutcome::GenerationChanged { before, after } => {
                assert_ne!(before, after);
            }
            PreparationOutcome::NonCanonical => {}
        }
    });
}

#[test]
fn negative_postload_only_blesses_mixed_preparation_generation() {
    support::negative(3, "HOF_MIXED_PREPARATION_GENERATION", || {
        let runtime = runtime();
        let registrar = {
            let runtime = runtime.clone();
            thread::spawn(move || replace_native_registration(&runtime))
        };
        let outcome = prepare(&runtime, PreparationProtocol::PostloadOnly);
        support::join(registrar);
        check_quiescent(&runtime);
        drop(runtime);
        match outcome {
            PreparationOutcome::Prepared(prepared) => {
                // Broken history: copy OLD canonical semantics, release the
                // metadata guard, registrar changes semantics and publishes,
                // then stamp that OLD copy with the NEW postload epoch.
                assert!(
                    prepared.registration == prepared.copied_generation,
                    "HOF_MIXED_PREPARATION_GENERATION"
                );
            }
            PreparationOutcome::GenerationChanged { .. } => {
                panic!("postload-only protocol cannot detect a generation change")
            }
            PreparationOutcome::NonCanonical => {}
        }
    });
}
