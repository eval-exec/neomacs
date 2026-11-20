//! Reference-free admission into one borrowed production era.
//!
//! Value does not name the evaluator keeper. The callback receives the
//! original loan, the decoded census, and the exclusive GEN1 admission. It
//! admits and finishes that era, then returns the same loan. This module
//! keeps the phase, the publication, and the opaque census. A typed refusal
//! or an unwind retains that backing and poisons the phase. No production
//! caller enters it.

use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;

use crate::tagged::gc::{
    EraWorldLoan, FrozenFunctionRoots, FrozenLegacyRoots, Gen1Admission, TaggedValue, WorldLoan,
};
use crate::tagged::transport::RootBatchLease;

use super::{
    Backing, HeapRootDesignator, HeapTransferCompletion, OpaqueRootWord, PreparedHeapTransfer,
    RetainedTransferFailure, ReturnControl, TransferError, TransferPhase,
};

/// Why a scoped production era refused. The backing stays with the failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum ScopedEraError {
    Transfer(TransferError),
    Admission(crate::tagged::gc::RegistrationError),
    Finish(crate::tagged::gc::RegistrationError),
}

enum FailureState {
    Entry { transfer: RetainedTransferFailure },
    Local { scope: ManuallyDrop<EraScope> },
}

/// Owned refusal. No execution, recovery, or transport accessor.
#[must_use]
#[doc(hidden)]
pub struct ScopedEraFailure {
    reason: ScopedEraError,
    state: FailureState,
}

impl ScopedEraFailure {
    #[doc(hidden)]
    pub fn reason(&self) -> ScopedEraError {
        self.reason
    }

    #[doc(hidden)]
    pub fn is_poisoned(&self) -> bool {
        match &self.state {
            FailureState::Entry { transfer } => transfer.is_poisoned(),
            FailureState::Local { scope } => scope.control.phase.load() == TransferPhase::Poisoned,
        }
    }
}

impl Drop for ScopedEraFailure {
    fn drop(&mut self) {
        // Name the retained state. Taking it would free the backing this
        // refusal is required to keep.
        match &self.state {
            FailureState::Entry { transfer } => {
                let _retained = transfer;
            }
            FailureState::Local { scope } => {
                let _retained = scope;
            }
        }
    }
}

impl std::fmt::Debug for ScopedEraFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("ScopedEraFailure")
            .field(&self.reason)
            .finish()
    }
}

/// There is deliberately no [`PreparedHeapTransfer`] while the callback's era
/// owns the keeper. Publication and the opaque census survive every refusal.
struct EraScope {
    root: NonNull<super::super::super::TaggedHeap>,
    control: Arc<ReturnControl>,
    publication: Option<ManuallyDrop<RootBatchLease>>,
    words: Option<ManuallyDrop<Box<[OpaqueRootWord]>>>,
    functions: FrozenFunctionRoots,
    owner: Option<ManuallyDrop<WorldLoan>>,
    armed: bool,
    /// Load-bearing: the loan and the publication are `Send`, and this scope
    /// must not be.
    _local: PhantomData<Rc<()>>,
}

impl EraScope {
    fn new(mut transfer: PreparedHeapTransfer) -> Self {
        let Backing {
            owner,
            publication,
            words,
            functions,
        } = transfer.backing.take().expect("affine prepared backing");
        let root = transfer.root.as_non_null();
        let control = Arc::clone(&transfer.control);
        // The emptied transfer has no backing, so its Drop does not poison.
        drop(transfer);
        Self {
            root,
            control,
            publication: Some(ManuallyDrop::new(publication)),
            words: Some(ManuallyDrop::new(words)),
            functions,
            owner: Some(ManuallyDrop::new(owner)),
            armed: true,
            _local: PhantomData,
        }
    }

    fn abandon(&self) {
        self.control.poison();
        // A forgotten permit can still name this heap. Comparing addresses
        // is safe here; reconstructing a shared heap reference is not.
        super::super::super::clear_tagged_heap_root_if_installed(self.root);
    }

    fn refuse(self, reason: ScopedEraError) -> ScopedEraFailure {
        self.abandon();
        ScopedEraFailure {
            reason,
            state: FailureState::Local {
                scope: ManuallyDrop::new(self),
            },
        }
    }

    /// The callback's era and collector arguments end before this returns.
    fn run<R: HeapTransferCompletion>(
        &mut self,
        body: impl for<'heap> FnOnce(
            Gen1Admission<'heap>,
            WorldLoan,
            FrozenLegacyRoots,
        )
            -> Result<(WorldLoan, R), (ScopedEraError, Option<WorldLoan>)>,
    ) -> Result<R, ScopedEraError> {
        {
            let owner = self.owner.as_ref().expect("affine era owner").deref();
            EraWorldLoan::adopt_current_thread(owner);
            if !EraWorldLoan::held_here(owner) {
                return Err(ScopedEraError::Transfer(TransferError::WrongThread));
            }
        }
        let words = self.words.as_ref().expect("retained opaque census");
        let mut decoded = Vec::new();
        decoded
            .try_reserve_exact(words.len())
            .map_err(|_| ScopedEraError::Transfer(TransferError::Capacity))?;
        // SAFETY: each word was stored from `TaggedValue::bits` while this
        // scope's publication pinned that census. Decoding is this keeper path.
        decoded.extend(
            words
                .iter()
                .map(|word| unsafe { TaggedValue::from_bits(word.0) }),
        );
        let heap = self.control.identity.heap;
        let functions = self.functions;
        // SAFETY: the publication above still pins every managed word. The
        // census is the one captured for this retained root.
        let roots = unsafe { FrozenLegacyRoots::new(heap, decoded, functions) };
        let root = self.root;
        let owner = ManuallyDrop::into_inner(self.owner.take().expect("affine era owner"));
        // SAFETY: this scope holds the original root, and no collector loan
        // is outstanding. The callback drops that loan before returning.
        let outcome = unsafe {
            super::with_retained_collector(root, move |collector| {
                let admission = match Gen1Admission::check(collector) {
                    Ok(admission) => admission,
                    Err(failure) => {
                        let (_reason, collector) = failure.into_parts();
                        drop(collector);
                        return Err((
                            ScopedEraError::Transfer(TransferError::GenerationalRequired),
                            Some(owner),
                        ));
                    }
                };
                body(admission, owner, roots)
            })
        };
        match outcome {
            Ok((owner, result)) => {
                self.owner = Some(ManuallyDrop::new(owner));
                Ok(result)
            }
            Err((reason, owner)) => {
                if let Some(owner) = owner {
                    self.owner = Some(ManuallyDrop::new(owner));
                }
                Err(reason)
            }
        }
    }

    fn complete<R: HeapTransferCompletion>(
        mut self,
        result: R,
    ) -> Result<(PreparedHeapTransfer, R), ScopedEraFailure> {
        let foreign = super::super::super::TAGGED_HEAP.with(|slot| {
            let installed = slot.get();
            !installed.is_null() && installed != self.root.as_ptr()
        });
        if foreign {
            return Err(self.refuse(ScopedEraError::Transfer(TransferError::InstalledTls)));
        }
        if let Err(reason) = super::retire_transfer_tls(self.root) {
            return Err(self.refuse(ScopedEraError::Transfer(reason)));
        }
        let owner = ManuallyDrop::into_inner(self.owner.take().expect("returned era owner"));
        let publication =
            ManuallyDrop::into_inner(self.publication.take().expect("retained publication"));
        let words = ManuallyDrop::into_inner(self.words.take().expect("retained census"));
        // SAFETY: `root` is the same provenance designator this transfer
        // already held. The returned loan still retains that allocation.
        let transfer = PreparedHeapTransfer {
            root: unsafe { HeapRootDesignator::from_original(self.root) },
            control: Arc::clone(&self.control),
            backing: Some(Backing {
                owner,
                publication,
                words,
                functions: self.functions,
            }),
        };
        // A failed transition reports through the rebuilt transfer. Leaving
        // this scope armed would abandon that same control a second time.
        self.armed = false;
        if let Err(reason) = transfer
            .control
            .transition(TransferPhase::Local, TransferPhase::Prepared)
        {
            return Err(entry_failure(transfer, reason));
        }
        Ok((transfer, result))
    }
}

impl Drop for EraScope {
    fn drop(&mut self) {
        if self.armed {
            self.abandon();
        }
        // `ManuallyDrop` fields retain the publication, census, and owner.
    }
}

fn entry_failure(transfer: PreparedHeapTransfer, reason: TransferError) -> ScopedEraFailure {
    ScopedEraFailure {
        reason: ScopedEraError::Transfer(reason),
        state: FailureState::Entry {
            transfer: transfer.refuse(reason),
        },
    }
}

impl PreparedHeapTransfer {
    /// Run one borrowed production era against this reference-free right.
    ///
    /// The callback admits and finishes the era. It returns the original loan
    /// when it still owns that loan. A finish refusal returns `None`: the
    /// era's own drop retains the keeper. This rebuilds a `Send` right only
    /// after that callback, including its era argument, has returned.
    #[cold]
    #[doc(hidden)]
    pub fn with_scoped_owner<R: HeapTransferCompletion>(
        self,
        body: impl for<'heap> FnOnce(
            Gen1Admission<'heap>,
            WorldLoan,
            FrozenLegacyRoots,
        )
            -> Result<(WorldLoan, R), (ScopedEraError, Option<WorldLoan>)>,
    ) -> Result<(Self, R), ScopedEraFailure> {
        if super::super::super::tagged_heap_is_installed() {
            return Err(entry_failure(self, TransferError::InstalledTls));
        }
        if let Err(reason) = self
            .control
            .transition(TransferPhase::Prepared, TransferPhase::Local)
        {
            return Err(entry_failure(self, reason));
        }
        let mut scope = EraScope::new(self);
        match scope.run(body) {
            Ok(result) => scope.complete(result),
            Err(reason) => Err(scope.refuse(reason)),
        }
    }
}

static_assertions::assert_not_impl_any!(EraScope: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ScopedEraFailure: Send, Sync, Clone, Copy);
