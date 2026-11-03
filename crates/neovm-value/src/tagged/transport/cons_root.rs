//! Sealed rooted inputs and immediate root installation for parallel conses.
//!
//! This bridge never installs a heap in TLS or borrows a whole TaggedHeap.
//! Its unique run borrow keeps the canonical run obligation active until the
//! initialized cell is installed in its canonical rooted destination home.

use super::SharedRoot;
use crate::tagged::gc::{HeapIdentity, RegistrationError, RunAccess};
use crate::tagged::value::TaggedValue;
use std::marker::PhantomData;

mod sealed {
    use super::*;

    // A caller cannot construct the seal. Implementations cannot be supplied
    // outside this crate, nor can a caller extract or install unrooted words.
    pub trait Slot {
        fn validate(
            &self,
            run: &RunAccess<'_, '_>,
            seal: ConsRootSeal,
        ) -> Result<(), RegistrationError>;
        fn word(&self, run: &RunAccess<'_, '_>, seal: ConsRootSeal) -> usize;
        fn install(&mut self, run: &mut RunAccess<'_, '_>, value: TaggedValue, seal: ConsRootSeal);
    }
}
pub(crate) use sealed::Slot as RootSlotOps;

/// A sealed rooted operand or result home for the single-cons primitive.
/// Implemented by shared transport roots and owner-local census slots.
pub trait RootedSlot: sealed::Slot {}

/// Only this bridge can construct inputs from borrowed rooted homes. The
/// operation keeps those homes alive through allocation and result installation.
pub(crate) struct RootedConsInputs<'roots> {
    heap: HeapIdentity,
    car: usize,
    cdr: usize,
    _roots: PhantomData<&'roots ()>,
}

impl RootedConsInputs<'_> {
    pub(crate) fn heap_identity(&self) -> HeapIdentity {
        self.heap
    }

    /// Opaque copies only: the caller still needs matching live run/catalog
    /// authority before interpreting either word as a Lisp object.
    pub(crate) fn words(&self) -> (usize, usize) {
        (self.car, self.cdr)
    }
}

/// Consuming initialized storage into raw root parts requires this seal.
/// Its private field and absent constructors keep that operation confined to
/// the immediate registration sequence below, even within the crate.
pub struct ConsRootSeal {
    _private: (),
}

static_assertions::assert_not_impl_any!(RootedConsInputs<'static>: Clone, Copy);
static_assertions::assert_not_impl_any!(ConsRootSeal: Clone, Copy);

#[inline]
fn rooted_inputs<'roots, C: RootedSlot, D: RootedSlot>(
    run: &RunAccess<'_, '_>,
    car: &'roots C,
    cdr: &'roots D,
) -> RootedConsInputs<'roots> {
    RootedConsInputs {
        heap: run.heap_identity(),
        car: car.word(run, ConsRootSeal { _private: () }),
        cdr: cdr.word(run, ConsRootSeal { _private: () }),
        _roots: PhantomData,
    }
}

/// Validate every source and destination before reserving any cell. The
/// unique run keeps its census resident and prevents a safe point throughout
/// initialization and the infallible destination store.
#[inline]
pub(crate) fn cons_into<C: RootedSlot, D: RootedSlot, R: RootedSlot>(
    run: &mut RunAccess<'_, '_>,
    car: &C,
    cdr: &D,
    result: &mut R,
) -> Result<(), RegistrationError> {
    car.validate(run, ConsRootSeal { _private: () })?;
    cdr.validate(run, ConsRootSeal { _private: () })?;
    result.validate(run, ConsRootSeal { _private: () })?;
    let inputs = rooted_inputs(run, car, cdr);
    let initialized = run.allocate_rooted_cons(&inputs)?;
    let (_, value) = initialized.into_root_parts(ConsRootSeal { _private: () });
    // Nothing between initialization and this store may fail, invoke Lisp,
    // resize the local bank, poll, or release the run. SharedRoot alone pays
    // for transport registration; the local implementation is one word store.
    result.install(run, value, ConsRootSeal { _private: () });
    Ok(())
}

impl RootSlotOps for SharedRoot {
    #[inline]
    fn validate(&self, run: &RunAccess<'_, '_>, _: ConsRootSeal) -> Result<(), RegistrationError> {
        if self.heap_identity() == run.heap_identity() {
            Ok(())
        } else {
            Err(RegistrationError::WrongHeap)
        }
    }
    #[inline]
    fn word(&self, _: &RunAccess<'_, '_>, _: ConsRootSeal) -> usize {
        SharedRoot::word(self)
    }
    fn install(&mut self, run: &mut RunAccess<'_, '_>, value: TaggedValue, _: ConsRootSeal) {
        *self = Self::in_heap(run.heap_identity(), value);
    }
}
impl RootedSlot for SharedRoot {}

impl<'owner, 'era> RunAccess<'owner, 'era> {
    /// Initialize one preallocated owner census slot from a rooted same-heap
    /// source. The logical slot budget is fixed at owner registration.
    #[cold]
    #[inline(never)]
    pub fn root_slot(
        &mut self,
        source: &SharedRoot,
    ) -> Result<crate::tagged::gc::OwnerRootSlot<'era>, RegistrationError> {
        source.validate(self, ConsRootSeal { _private: () })?;
        self.admit_root_slot(SharedRoot::word(source), ConsRootSeal { _private: () })
    }
}

impl SharedRoot {
    #[cfg(test)]
    pub(crate) fn cons_word_for_test(&self) -> usize {
        self.word()
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn cons_from_run(
        run: &mut RunAccess<'_, '_>,
        car: &Self,
        cdr: &Self,
    ) -> Result<Self, RegistrationError> {
        // NIL is an untraced immediate: this placeholder performs no root-table
        // registration, lock or allocation, including on operand refusal.
        let mut result = Self {
            heap: run.heap_identity(),
            transport: super::Transport::Untraced(TaggedValue::NIL.bits()),
        };
        cons_into(run, car, cdr, &mut result)?;
        Ok(result)
    }
}
