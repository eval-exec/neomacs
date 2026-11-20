//! Cold retention across a scoped era constructor. There is one execution
//! proxy. Its companion retains the allocation and exposes no loan.

use super::{EraWorldLoan, SymbolRootDesignators, WorldLoan};
use crate::tagged::gc::heap_transfer::HeapRootDesignator;
use crate::tagged::gc::{FacadeEpochRetention, FacadeMarkExclusionError, HeapIdentity};
use crate::tagged::header::MarkerObj;
use crate::tagged::value::TaggedValue;
use std::any::Any;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};

use super::super::{AllStopped, RegistrationError, RendezvousError};

type Storage = Arc<Mutex<ManuallyDrop<WorldLoan>>>;

/// No heap, World, or owner accessor. Abandonment retains the original owner
/// even when a partially constructed era unwinds before its own drop guard.
#[must_use]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct RetainedEraOwner {
    storage: ManuallyDrop<Storage>,
    _local: PhantomData<Rc<()>>,
}

struct OwnerProxy {
    storage: Storage,
    _not_sync: PhantomData<std::cell::Cell<()>>,
}

impl OwnerProxy {
    fn owner(&self) -> MutexGuard<'_, ManuallyDrop<WorldLoan>> {
        self.storage
            .lock()
            .unwrap_or_else(|_| panic!("retained era owner lock poisoned"))
    }
}

// SAFETY: `new` constructs the only proxy, and the companion has no execution
// API. Each method delegates to that same affine owner under a private mutex.
// No guard or reference escapes a call. The owner hooks do not call Lisp,
// wait, or reenter this proxy. No other mutator can acquire this mutex, and
// no shared scheduling lock is exposed. `into_retained_owner` keeps the proxy
// until `recover` checks that exact allocation and extracts the original
// owner. No existing `!Send` loan becomes `Send`.
unsafe impl EraWorldLoan for OwnerProxy {
    fn held_here(&self) -> bool {
        self.owner().held_here()
    }

    fn parallel_heap_identity(&self) -> HeapIdentity {
        self.owner().parallel_heap_identity()
    }

    fn parallel_epoch_retention(&self) -> Result<FacadeEpochRetention, FacadeMarkExclusionError> {
        self.owner().parallel_epoch_retention()
    }

    fn symbol_root_designators(&self) -> SymbolRootDesignators {
        self.owner().symbol_root_designators()
    }

    fn check_symbol_root_designators(
        &self,
        roots: &SymbolRootDesignators,
    ) -> Result<(), RegistrationError> {
        self.owner().check_symbol_root_designators(roots)
    }

    fn collect_parallel_world_roots(
        &self,
        stopped: &mut AllStopped<'_>,
        out: &mut Vec<TaggedValue>,
    ) -> Result<Vec<*mut *mut MarkerObj>, RendezvousError> {
        self.owner().collect_parallel_world_roots(stopped, out)
    }

    fn into_retained_owner(self: Box<Self>) -> Box<dyn Any + Send> {
        self
    }

    fn adopt_current_thread(&self) {
        self.owner().adopt_current_thread()
    }

    fn heap_root_designator(&self) -> HeapRootDesignator {
        self.owner().heap_root_designator()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) enum OwnerRecoveryError {
    Foreign,
    Shared,
    Poisoned,
}

enum RecoveryState {
    Foreign {
        _retainer: RetainedEraOwner,
        _proxy: ManuallyDrop<Box<dyn Any + Send>>,
    },
    Shared {
        _retainer: RetainedEraOwner,
    },
    Poisoned {
        _owner: ManuallyDrop<WorldLoan>,
    },
}

#[must_use]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct RetainedOwnerRecoveryFailure {
    state: RecoveryState,
    _local: PhantomData<Rc<()>>,
}

impl RetainedOwnerRecoveryFailure {
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn reason(&self) -> OwnerRecoveryError {
        match &self.state {
            RecoveryState::Foreign { .. } => OwnerRecoveryError::Foreign,
            RecoveryState::Shared { .. } => OwnerRecoveryError::Shared,
            RecoveryState::Poisoned { .. } => OwnerRecoveryError::Poisoned,
        }
    }
}

impl Drop for RetainedOwnerRecoveryFailure {
    fn drop(&mut self) {
        // Keep every retained owner. Taking a field would drop it.
        match &self.state {
            RecoveryState::Foreign { _retainer, _proxy } => {
                let _retained = (_retainer, _proxy);
            }
            RecoveryState::Shared { _retainer } => {
                let _retained = _retainer;
            }
            RecoveryState::Poisoned { _owner } => {
                let _retained = _owner;
            }
        }
    }
}

impl std::fmt::Debug for RetainedOwnerRecoveryFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.reason().fmt(formatter)
    }
}

impl RetainedEraOwner {
    /// Arm the original owner before any era constructor can run.
    #[cold]
    #[cfg_attr(
        not(any(test, feature = "test-support")),
        expect(
            dead_code,
            reason = "E2 L4 dormant retained era owner; no production caller enters it"
        )
    )]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn new(owner: WorldLoan) -> (Self, WorldLoan) {
        let owner = ManuallyDrop::new(owner);
        let storage = Arc::new(Mutex::new(owner));
        let proxy = OwnerProxy {
            storage: Arc::clone(&storage),
            _not_sync: PhantomData,
        };
        (
            Self {
                storage: ManuallyDrop::new(storage),
                _local: PhantomData,
            },
            WorldLoan::new(proxy),
        )
    }

    /// The collector and era frames have returned. There is no second original
    /// owner and no recovery from a retained failure.
    #[cold]
    #[cfg_attr(
        not(any(test, feature = "test-support")),
        expect(
            dead_code,
            reason = "E2 L4 dormant retained era recovery; no production caller enters it"
        )
    )]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn recover(
        self,
        proxy: WorldLoan,
    ) -> Result<WorldLoan, RetainedOwnerRecoveryFailure> {
        let retained = Box::new(proxy).into_retained_owner();
        let proxy = match retained.downcast::<OwnerProxy>() {
            Ok(proxy) if Arc::ptr_eq(&proxy.storage, &self.storage) => proxy,
            Ok(foreign) => {
                return Err(RetainedOwnerRecoveryFailure {
                    state: RecoveryState::Foreign {
                        _retainer: self,
                        _proxy: ManuallyDrop::new(foreign),
                    },
                    _local: PhantomData,
                });
            }
            Err(foreign) => {
                return Err(RetainedOwnerRecoveryFailure {
                    state: RecoveryState::Foreign {
                        _retainer: self,
                        _proxy: ManuallyDrop::new(foreign),
                    },
                    _local: PhantomData,
                });
            }
        };
        drop(proxy);
        let storage = ManuallyDrop::into_inner(self.storage);
        let mutex = match Arc::try_unwrap(storage) {
            Ok(mutex) => mutex,
            Err(storage) => {
                return Err(RetainedOwnerRecoveryFailure {
                    state: RecoveryState::Shared {
                        _retainer: Self {
                            storage: ManuallyDrop::new(storage),
                            _local: PhantomData,
                        },
                    },
                    _local: PhantomData,
                });
            }
        };
        match mutex.into_inner() {
            Ok(owner) => Ok(ManuallyDrop::into_inner(owner)),
            Err(poisoned) => Err(RetainedOwnerRecoveryFailure {
                state: RecoveryState::Poisoned {
                    _owner: poisoned.into_inner(),
                },
                _local: PhantomData,
            }),
        }
    }
}

static_assertions::assert_not_impl_any!(RetainedEraOwner: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(RetainedOwnerRecoveryFailure: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(OwnerProxy: Send);
static_assertions::assert_not_impl_any!(OwnerProxy: Sync, Clone, Copy);
