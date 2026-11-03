//! Value-layer vocabulary for cold function-write validation and retirement.
//!
//! The compiler belongs above the heap. Static implementations supply its
//! checks/effects without a callback allocation, extra TLS lookup or JIT type
//! in this interface. Existing symbol authority still guards the cell itself.
use super::{KeeperSymbols, SymId, SymbolWriteToken};
use crate::tagged::value::TaggedValue;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialDefinition {
    OpaqueScalarConstant,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompilerActivity {
    NativeActivation,
    BackgroundCompilation,
}

/// Cold classification only; syntax alone cannot authorize parameter binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallbackParameterKind {
    Ordinary,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HofOperation {
    Mapc,
    Mapcar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HofPreparationError {
    MissingCanonicalSymbol,
    UnsupportedBuiltin,
    SubrMetadataBusy,
}

/// Observed publication data, not authority to prepare or select a builtin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HofRegistrationEpoch(u64);
impl HofRegistrationEpoch {
    pub(crate) const fn from_observed(generation: u64) -> Self {
        Self(generation)
    }
}

/// Read-only observer of the one existing completed-registration publisher.
/// No owned atomic, function pointer, metadata access, or TLS is introduced.
#[derive(Clone, Copy, Debug)]
pub struct HofRegistrationClock(&'static AtomicU64);
impl HofRegistrationClock {
    /// # Safety
    /// The static is the actual completed native-registration publication
    /// clock. Every relevant registration/reset publishes Release after its
    /// complete update. A seed must carry that same clock and its observed stamp.
    pub(crate) unsafe fn from_publisher(clock: &'static AtomicU64) -> Self {
        Self(clock)
    }
    pub(crate) fn snapshot(self) -> HofRegistrationEpoch {
        HofRegistrationEpoch::from_observed(self.0.load(Ordering::Acquire))
    }
}

/// Identity comparison only: this never exposes a value, address or subr entry.
/// Construction is confined to the unsafe canonical seed boundary below.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct StaticSubrIdentity(usize);
impl StaticSubrIdentity {
    pub(crate) fn matches(self, value: TaggedValue) -> bool {
        self.0 == value.bits()
    }
}
impl std::fmt::Debug for StaticSubrIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StaticSubrIdentity")
    }
}

/// Cold canonical identity, not an era admission or callback call capability.
/// The receiving era must still bind and validate its own symbol handle.
#[derive(Clone, Copy, Debug)]
pub struct CanonicalHofSeed {
    operation: HofOperation,
    symbol: SymId,
    identity: StaticSubrIdentity,
    registration_epoch: HofRegistrationEpoch,
    registration_clock: HofRegistrationClock,
}
impl CanonicalHofSeed {
    /// # Safety
    /// `symbol` is the canonical symbol for `operation`'s fixed spelling and
    /// `canonical` is its actual leaked canonical static subr, obtained from
    /// the original keeper's correctly installed canonical-subr table during
    /// exclusive preparation. Neither the fixed-name mapping nor registration
    /// metadata may change during its unsafe metadata reads. A current
    /// mutable function-cell value cannot establish this canonical identity.
    /// The canonical object's direct entry and current registration must still
    /// be the original builtin implementation for this operation and arity.
    /// `registration_epoch` is the original completed-registration stamp
    /// observed before verification and confirmed unchanged afterward. Running
    /// selection must validate that stamp again under its symbol reader; a
    /// subsequent registration makes this seed stale without revoking an
    /// already selected constant callback.
    /// `registration_clock` is the very publisher from which this stamp was
    /// observed; the live observer must be retained for running revalidation.
    /// Consumers may compare identity only; no subr entry is authorized to run.
    pub(crate) unsafe fn from_canonical_static_subr(
        operation: HofOperation,
        symbol: SymId,
        canonical: TaggedValue,
        registration_epoch: HofRegistrationEpoch,
        registration_clock: HofRegistrationClock,
    ) -> Self {
        Self {
            operation,
            symbol,
            identity: StaticSubrIdentity(canonical.bits()),
            registration_epoch,
            registration_clock,
        }
    }

    pub(crate) fn operation(&self) -> HofOperation {
        self.operation
    }

    pub(crate) fn symbol(&self) -> SymId {
        self.symbol
    }

    pub(crate) fn identity(&self) -> StaticSubrIdentity {
        self.identity
    }

    pub(crate) fn registration_clock(&self) -> HofRegistrationClock {
        self.registration_clock
    }

    pub(crate) fn registration_epoch(&self) -> HofRegistrationEpoch {
        self.registration_epoch
    }
}

pub trait FunctionWriteHooks {
    /// Prepare a fixed canonical mapper identity, independently of its current
    /// function cell. Existing cold canonical-subr lookup may use keeper TLS;
    /// this hook must never be called by a running selection.
    ///
    /// # Safety
    /// The matching checked admission writer retains this backing before any
    /// owner registers. The original keeper's canonical-subr table is installed
    /// and unsafe metadata reads exclude concurrent registration writes. The
    /// returned seed is stamped; running selection must revalidate that stamp.
    unsafe fn prepare_hof_builtin(
        &self,
        operation: HofOperation,
        authority: &SymbolWriteToken<'_>,
    ) -> Result<CanonicalHofSeed, HofPreparationError>;

    /// Classify an existing immutable definition; never materialize a dump
    /// payload, run a callback, or return a borrowed compiler object.
    ///
    /// # Safety
    /// The symbol writer belongs to this backing, the value is retained, and
    /// serial preparation excludes overlapping definition or heap mutation.
    unsafe fn classify_initial_definition(
        &self,
        value: TaggedValue,
        authority: &SymbolWriteToken<'_>,
    ) -> InitialDefinition;

    /// Inspect the keeper's existing compiler state, in the same order as
    /// legacy admission. This is a cold preflight, not a running-owner query.
    fn check_compiler_quiescent(&self) -> Result<(), CompilerActivity>;

    /// Retire compiler observations of a changed original target. The upper
    /// implementation owns cache eviction and the internal-write reason.
    ///
    /// # Safety
    /// The keeper is this backing's matching AllStopped, on its keeper thread,
    /// with zero symbol word and compiler quiescence already established.
    unsafe fn reconcile_function_write(&self, symbol: SymId, keeper: &KeeperSymbols<'_>);
}
