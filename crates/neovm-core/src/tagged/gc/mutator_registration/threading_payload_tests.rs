//! Narrow stopped-collector test access, with no dependency on World code.
//! Real-World integration proofs are registered by the runtime thread suite.

use super::{AllStopped, RendezvousError};
use crate::tagged::header::VecLikeHeader;
use crate::tagged::value::TaggedValue;

#[cfg(test)]
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

#[cfg(test)]
impl AllStopped<'_> {
    /// Acknowledge the no-collection result after the GEN0 minor refusal.
    /// The callback neither inspects the heap nor changes liveness.
    pub(crate) fn acknowledge_refused_minor_for_threading_test(
        &mut self,
    ) -> Result<(), RendezvousError> {
        self.trace(|_, _| Ok(()))
    }
}
