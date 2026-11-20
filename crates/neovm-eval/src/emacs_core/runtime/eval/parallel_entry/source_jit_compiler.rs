//! Feature-static compiler-scope closure after the actual backend receipt.

use super::source_jit_backend::SourceJitBackend;

#[cfg(feature = "jit")]
pub(crate) use crate::emacs_core::jit::source_suspension::{
    SourceCompilerClosed as SourceJitCompiler, SourceScopeRefusal as SourceJitCompilerError,
};
#[cfg(not(feature = "jit"))]
#[derive(Debug)]
pub(crate) struct SourceJitCompiler(std::marker::PhantomData<std::rc::Rc<()>>);
#[cfg(not(feature = "jit"))]
pub(crate) type SourceJitCompilerError = std::convert::Infallible;

/// # Safety
///
/// The backend is the actual preceding receipt in the sole inaccessible
/// source chain. All execution and collector or store frames have returned.
/// No source execution or GC follows until the complete publication or
/// return protocol.
#[cold]
pub(crate) unsafe fn close(
    backend: &SourceJitBackend,
) -> Result<SourceJitCompiler, SourceJitCompilerError> {
    #[cfg(feature = "jit")]
    {
        // SAFETY: the enclosing source-boundary contract. The actual backend
        // is borrowed from its concrete owning source chain, never synthesized.
        unsafe { SourceJitCompiler::close(backend) }
    }
    #[cfg(not(feature = "jit"))]
    {
        let _ = backend;
        Ok(SourceJitCompiler(std::marker::PhantomData))
    }
}

static_assertions::assert_not_impl_any!(SourceJitCompiler: Send, Sync, Clone, Copy);
