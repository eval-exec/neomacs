//! Feature-static source cache effect. This is not complete JIT or TLS
//! withdrawal and supplies no nmjit or heap transport authority.

use super::super::Context;

#[cfg(feature = "jit")]
pub(crate) use crate::emacs_core::jit::cache::source_detachment::{
    DetachedSourceCache as SourceJitCache, SourceCacheError as SourceJitCacheError,
};
#[cfg(not(feature = "jit"))]
#[derive(Debug)]
pub(crate) struct SourceJitCache(std::marker::PhantomData<std::rc::Rc<()>>);
#[cfg(not(feature = "jit"))]
pub(crate) type SourceJitCacheError = std::convert::Infallible;

/// # Safety
///
/// The actual sole source has no active native, VM, or compile frame and is
/// inaccessible until its complete return protocol. Its final roots remain
/// retained and cannot be collected before publication or explicit tracing.
#[cold]
pub(crate) unsafe fn withdraw(source: &mut Context) -> Result<SourceJitCache, SourceJitCacheError> {
    #[cfg(feature = "jit")]
    {
        use crate::emacs_core::jit::cache::source_detachment::SourceCacheOwner;
        // SAFETY: the enclosing source-boundary contract above. Slot,
        // identity, and pending checks stay in the one real constructor.
        unsafe { SourceJitCache::withdraw(SourceCacheOwner::for_context(source)) }
    }
    #[cfg(not(feature = "jit"))]
    {
        let _ = source;
        Ok(SourceJitCache(std::marker::PhantomData))
    }
}

static_assertions::assert_not_impl_any!(SourceJitCache: Send, Sync, Clone, Copy);
