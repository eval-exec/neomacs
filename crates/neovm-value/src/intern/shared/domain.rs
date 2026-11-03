//! Process-wide intern-view admission, independent of any one World's stop.
//! No operation waits, locks a registry, or takes a publication ledger here.
//! World WRITER -> registry -> ledger remains the publication lock order;
//! a writer may only try grace and defer, never wait for readers under WRITER.
use super::sync::{AtomicU64, Ordering};
use std::marker::PhantomData;
use std::rc::Rc;

const GRACE: u64 = 1 << 63;
const DRAIN: u64 = 1 << 62;
const COUNT_MASK: u64 = (1 << 47) - 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadRefusal {
    Busy,
    Draining,
    Saturated,
    ReservedState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ForeignDomain;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DrainRefusal {
    AlreadyDraining,
    ReservedState,
}

pub(crate) struct InternReadDomain {
    word: AtomicU64,
}
impl InternReadDomain {
    pub(crate) fn new() -> Self {
        Self {
            word: AtomicU64::new(0),
        }
    }

    /// One load and at most one CAS. A lost race returns Busy without a loan.
    /// Acquisition is per outer native slice, never per metadata probe.
    pub(crate) fn try_scope(&self) -> Result<InternReadScope<'_>, ReadRefusal> {
        let current = self.word.load(Ordering::Acquire);
        if current & GRACE != 0 {
            return Err(ReadRefusal::Busy);
        }
        if current & DRAIN != 0 {
            return Err(ReadRefusal::Draining);
        }
        if current & !COUNT_MASK != 0 {
            return Err(ReadRefusal::ReservedState);
        }
        if current == COUNT_MASK {
            return Err(ReadRefusal::Saturated);
        }
        self.word
            .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ReadRefusal::Busy)?;
        Ok(InternReadScope {
            domain: self,
            confined: PhantomData,
        })
    }

    /// Only the process-wide zero-reader CAS mints grace. A World stop token
    /// is deliberately not accepted. No spin/retry/wait can hold the registry
    /// against a reader that needs that registry on its slow path.
    pub(crate) fn try_grace(&self) -> Result<InternGrace<'_>, ReadRefusal> {
        self.word
            .compare_exchange(0, GRACE, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ReadRefusal::Busy)?;
        Ok(InternGrace {
            domain: self,
            confined: PhantomData,
        })
    }

    /// Request outside WRITER, registry and ledger locks. Admission closes
    /// immediately; existing scopes keep their ordinary Release exit. Retain
    /// this receipt until a later publication boundary, without waiting.
    pub(crate) fn request_drain(&self) -> Result<InternDrain<'_>, DrainRefusal> {
        if self.word.load(Ordering::Acquire) & !(COUNT_MASK | GRACE | DRAIN) != 0 {
            return Err(DrainRefusal::ReservedState);
        }
        // Unlike a CAS against the current count, this bit-set cannot lose to
        // reader churn. An in-progress grace preserves DRAIN when it exits.
        let before = self.word.fetch_or(DRAIN, Ordering::AcqRel);
        if before & DRAIN != 0 {
            return Err(DrainRefusal::AlreadyDraining);
        }
        Ok(InternDrain {
            domain: Some(self),
            confined: PhantomData,
        })
    }

    #[cfg(test)]
    pub(crate) fn seeded_for_test(word: u64) -> Self {
        Self {
            word: AtomicU64::new(word),
        }
    }
}

/// A counted loan of the process read domain. Drop it before stop ack, park,
/// blocking/foreign reentry or owner teardown; a view cannot survive Drop.
#[must_use]
pub(crate) struct InternReadScope<'domain> {
    domain: &'domain InternReadDomain,
    confined: PhantomData<Rc<()>>,
}
impl InternReadScope<'_> {
    /// This borrows an already owned snapshot, not an untracked atomic/raw
    /// pointer. P9.1b must add retained-owner publication before such a load.
    pub(crate) fn borrow<'scope, T>(&'scope self, snapshot: &'scope T) -> InternView<'scope, T> {
        InternView {
            snapshot,
            scope: PhantomData,
        }
    }
}
impl Drop for InternReadScope<'_> {
    fn drop(&mut self) {
        let before = self.domain.word.fetch_sub(1, Ordering::Release);
        debug_assert!(before & COUNT_MASK > 0);
        debug_assert_eq!(before & !(COUNT_MASK | DRAIN), 0);
    }
}

/// No copied pointer may outlive its particular scope or move to a carrier.
pub(crate) struct InternView<'scope, T> {
    snapshot: &'scope T,
    scope: PhantomData<&'scope InternReadScope<'scope>>,
}
impl<T> InternView<'_, T> {
    pub(crate) fn get(&self) -> &T {
        self.snapshot
    }
}

/// Sole ownership of a drain request. A second writer cannot cancel it or
/// acquire its grace. Dropping/cancelling restores admission; successful grace
/// consumes the request, so an old receipt cannot cancel a subsequent drain.
#[must_use]
pub(crate) struct InternDrain<'domain> {
    domain: Option<&'domain InternReadDomain>,
    confined: PhantomData<Rc<()>>,
}
impl<'domain> InternDrain<'domain> {
    /// One CAS, never a wait. Failure returns the same owned request for a
    /// later boundary. Success transfers exclusivity to the grace receipt.
    pub(crate) fn try_grace(mut self) -> Result<InternGrace<'domain>, Self> {
        let domain = self.domain.expect("live drain receipt");
        if domain
            .word
            .compare_exchange(DRAIN, GRACE, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(self);
        }
        self.domain.take();
        Ok(InternGrace {
            domain,
            confined: PhantomData,
        })
    }

    pub(crate) fn cancel_drain(self) {
        drop(self);
    }
}
impl Drop for InternDrain<'_> {
    fn drop(&mut self) {
        if let Some(domain) = self.domain.take() {
            let before = domain.word.fetch_and(!DRAIN, Ordering::Release);
            debug_assert_eq!(before & !(COUNT_MASK | GRACE), DRAIN);
        }
    }
}

/// Exclusive, zero-reader process-domain receipt. The later retained ledger
/// must verify its own domain before using this receipt to reclaim anything.
#[must_use]
pub(crate) struct InternGrace<'domain> {
    domain: &'domain InternReadDomain,
    confined: PhantomData<Rc<()>>,
}
impl InternGrace<'_> {
    pub(crate) fn verify_domain(&self, owner: &InternReadDomain) -> Result<(), ForeignDomain> {
        if std::ptr::eq(self.domain, owner) {
            Ok(())
        } else {
            Err(ForeignDomain)
        }
    }
}
impl Drop for InternGrace<'_> {
    fn drop(&mut self) {
        // A drain may have been requested while grace was held. Preserve its
        // independently owned admission barrier for the later boundary.
        let before = self.domain.word.fetch_and(!GRACE, Ordering::Release);
        debug_assert_eq!(before & !DRAIN, GRACE);
    }
}

static_assertions::assert_impl_all!(InternReadDomain: Send, Sync);
static_assertions::assert_not_impl_any!(InternReadScope<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(InternGrace<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(InternView<'static, u64>: Send, Sync, Clone, Copy);

static_assertions::assert_not_impl_any!(InternDrain<'static>: Send, Sync, Clone, Copy);
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
const _: () = {
    assert!(std::mem::size_of::<InternReadDomain>() == 8);
    assert!(std::mem::align_of::<InternReadDomain>() == 8);
};
