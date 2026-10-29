//! Value-layer proof that an exclusive collector loan has GEN1 enabled.
//!
//! The capability owns only the collector loan. Runtime scheduling/backing
//! authorities are checked separately by their owning layer after this proof.
use super::QuiescentCollector;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gen1AdmissionError {
    GenerationalRequired,
}

#[must_use = "recover or drop the returned exclusive collector loan"]
pub(crate) struct Gen1AdmissionFailure<'heap> {
    pub(crate) reason: Gen1AdmissionError,
    pub(crate) collector: QuiescentCollector<'heap>,
}
impl std::fmt::Debug for Gen1AdmissionFailure<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Gen1AdmissionFailure")
            .field(&self.reason)
            .finish()
    }
}

/// Owning the actual exclusive loan prevents mode changes for this lifetime.
#[must_use = "consume this capability into checked production admission"]
pub struct Gen1Admission<'heap> {
    collector: QuiescentCollector<'heap>,
}
impl std::fmt::Debug for Gen1Admission<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gen1Admission")
            .field("heap", &self.collector.heap_identity())
            .finish_non_exhaustive()
    }
}
impl<'heap> Gen1Admission<'heap> {
    #[cold]
    #[inline(never)]
    pub(crate) fn check(
        mut collector: QuiescentCollector<'heap>,
    ) -> Result<Self, Gen1AdmissionFailure<'heap>> {
        // Read the retained heap's actual mode exactly once at construction.
        if !collector.registration_heap_mut().generational_enabled() {
            return Err(Gen1AdmissionFailure {
                reason: Gen1AdmissionError::GenerationalRequired,
                collector,
            });
        }
        Ok(Self { collector })
    }

    pub(in crate::tagged::gc) fn into_collector(self) -> QuiescentCollector<'heap> {
        self.collector
    }
}

static_assertions::assert_not_impl_any!(Gen1Admission<'static>: Send, Sync, Clone, Copy);
