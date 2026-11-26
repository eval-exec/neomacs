//! Feature-static backend effect, with the actual preceding cache receipt.

use super::source_jit_cache::SourceJitCache;

#[cfg(feature = "jit")]
pub(crate) use crate::emacs_core::jit::compile::shared::source_detachment::{
    DetachedSourceBackend as SourceJitBackend, SourceBackendError as SourceJitBackendError,
};
#[cfg(not(feature = "jit"))]
#[derive(Debug)]
pub(crate) struct SourceJitBackend(std::marker::PhantomData<std::rc::Rc<()>>);
#[cfg(not(feature = "jit"))]
pub(crate) type SourceJitBackendError = std::convert::Infallible;

/// # Safety
///
/// The cache belongs to the actual inaccessible sole source. All execution
/// and collector or store frames have returned, and its root image and backing
/// remain retained without GC until the complete publication or return protocol.
#[cold]
pub(crate) unsafe fn withdraw(
    cache: &SourceJitCache,
) -> Result<SourceJitBackend, SourceJitBackendError> {
    #[cfg(feature = "jit")]
    {
        // SAFETY: the enclosing source-boundary contract. The actual cache is
        // borrowed from its concrete owning source chain, never synthesized.
        unsafe { SourceJitBackend::withdraw(cache) }
    }
    #[cfg(not(feature = "jit"))]
    {
        let _ = cache;
        Ok(SourceJitBackend(std::marker::PhantomData))
    }
}

static_assertions::assert_not_impl_any!(SourceJitBackend: Send, Sync, Clone, Copy);
