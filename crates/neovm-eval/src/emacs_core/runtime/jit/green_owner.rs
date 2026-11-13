//! Cold Green owner admission. No dormant call-path hook is introduced.
//!
//! Mutable leaves stay in owner-local TLS. Shared source slots carry raw leaf
//! pointers, but every reader checks the existing global leaf epoch. Each
//! baton acquisition invalidates those slots before ANY dispatch. This serial
//! protocol adds no hot owner check and does not certify parallel mutators.

use crate::emacs_core::eval::{Context, SpecBinding};
use neovm_value::intern::SymId;
use neovm_value::value::Value;
use std::marker::PhantomData;
use std::num::{NonZeroU64, NonZeroUsize};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FunctionEpoch(u64);
static_assertions::assert_impl_all!(FunctionEpoch: Send, Sync);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct JitOwnerId(NonZeroU64);
static_assertions::assert_impl_all!(JitOwnerId: Send, Sync);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HeapIdentity(NonZeroUsize);
static_assertions::assert_impl_all!(HeapIdentity: Send, Sync);

#[derive(Clone, Copy, Debug, thiserror::Error)]
pub(crate) enum AdmissionError {
    #[error("green JIT preparation requires a valid owner and heap identity")]
    InvalidIdentity,
    #[error("green JIT restoration requires the original owner and heap")]
    ReturningOwner,
}
static_assertions::assert_impl_all!(AdmissionError: Send, Sync);

/// Captured sources stay on the original continuation, never in a message.
#[derive(Clone, Copy)]
struct NamedSource {
    symbol: SymId,
    function: Value,
    _confined: PhantomData<Rc<()>>,
}
static_assertions::assert_not_impl_any!(NamedSource: Send, Sync);
impl std::fmt::Debug for NamedSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NamedSource")
            .field("symbol", &self.symbol)
            .field("function_bits", &self.function.bits())
            .finish()
    }
}

/// Reusable native scratch, kept only in this owner's boxed Green seat.
/// Every raw-value length is zero outside its original prepare/resume extent.
#[derive(Default)]
pub(crate) struct ParkedJitScratch {
    frames: Vec<Value>,
    pub(crate) called: Vec<SymId>,
    sources: Vec<NamedSource>,
    roots: Vec<Value>,
}
static_assertions::assert_not_impl_any!(ParkedJitScratch: Send, Sync, Clone, Copy);
impl std::fmt::Debug for ParkedJitScratch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Metadata only; never format or decode copied Lisp words.
        formatter
            .debug_struct("ParkedJitScratch")
            .field("frame_capacity", &self.frames.capacity())
            .field("called_capacity", &self.called.capacity())
            .field("source_capacity", &self.sources.capacity())
            .field("root_capacity", &self.roots.capacity())
            .finish()
    }
}
impl ParkedJitScratch {
    #[cfg(test)]
    #[doc(hidden)]
    pub fn storage_for_test(&self) -> [(usize, usize); 4] {
        // Allocation identities only: no method reconstructs native/Lisp pointers.
        [
            (self.frames.as_ptr() as usize, self.frames.capacity()),
            (self.called.as_ptr() as usize, self.called.capacity()),
            (self.sources.as_ptr() as usize, self.sources.capacity()),
            (self.roots.as_ptr() as usize, self.roots.capacity()),
        ]
    }
    #[cfg(test)]
    #[doc(hidden)]
    pub fn empty_for_test(&self) -> bool {
        self.frames.is_empty()
            && self.called.is_empty()
            && self.sources.is_empty()
            && self.roots.is_empty()
    }
}

/// CURRENT-owner preparation copied into the immutable owner/heap publication.
/// On return, captured sources pass to this owner's existing named-function
/// pins BEFORE publication retirement. Repeated parks therefore keep an old
/// definition alive even after another owner changed its canonical cell.
pub(crate) struct ParkedJit {
    owner: JitOwnerId,
    heap: HeapIdentity,
    function_epoch: FunctionEpoch,
    sources: Vec<NamedSource>,
    roots: Vec<Value>,
    _confined: PhantomData<Rc<()>>,
}
static_assertions::assert_not_impl_any!(ParkedJit: Send, Sync);
impl std::fmt::Debug for ParkedJit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Metadata only: never dereference copied heap Values for diagnostics.
        formatter
            .debug_struct("ParkedJit")
            .field("owner", &self.owner)
            .field("heap", &self.heap)
            .field("function_epoch", &self.function_epoch)
            .field("source_count", &self.sources.len())
            .field("root_words", &self.roots.len())
            .finish()
    }
}

impl ParkedJit {
    #[cold]
    #[allow(dead_code, reason = "standalone preparation retains the original API")]
    #[doc(hidden)]
    pub fn prepare(context: &Context) -> Result<Self, AdmissionError> {
        Self::prepare_reusing(context, &mut ParkedJitScratch::default())
    }

    #[cold]
    #[doc(hidden)]
    pub fn prepare_reusing(
        context: &Context,
        scratch: &mut ParkedJitScratch,
    ) -> Result<Self, AdmissionError> {
        debug_assert!(
            scratch.frames.is_empty()
                && scratch.called.is_empty()
                && scratch.sources.is_empty()
                && scratch.roots.is_empty()
        );
        let owner = JitOwnerId(
            NonZeroU64::new(context.context_instance_id())
                .ok_or(AdmissionError::InvalidIdentity)?,
        );
        let heap = HeapIdentity(
            NonZeroUsize::new(context.tagged_heap.tagged_heap_identity())
                .ok_or(AdmissionError::InvalidIdentity)?,
        );
        // A VM-only owner cannot have an executing cached native leaf. Avoid
        // enumerating/pinning source frames in that case. Keep the ordinary
        // parked census, including cached relocation roots, independent of
        // this shortcut: code cached for a future call must remain rooted too.
        // The window/test-depth checks also cover directly invoked test leaves
        // which bypass the production cache dispatch contract.
        if context.jit_root_stack_top == 0
            && super::cache::native_depth() == 0
            && !super::cache::green_has_native_state()
        {
            return Ok(Self {
                owner,
                heap,
                function_epoch: FunctionEpoch(context.obarray.function_epoch()),
                sources: std::mem::take(&mut scratch.sources),
                roots: std::mem::take(&mut scratch.roots),
                _confined: PhantomData,
            });
        }
        scratch
            .frames
            .extend(context.specpdl.iter().filter_map(|entry| match entry {
                SpecBinding::Backtrace { function, .. }
                | SpecBinding::Backtrace1 { function, .. }
                | SpecBinding::Backtrace2 { function, .. }
                | SpecBinding::BacktraceNative { function, .. } => Some(*function),
                SpecBinding::Let { .. }
                | SpecBinding::LetLocal { .. }
                | SpecBinding::LetDefault { .. }
                | SpecBinding::LexicalEnv { .. }
                | SpecBinding::GcRoot { .. }
                | SpecBinding::UnwindProtect { .. }
                | SpecBinding::SaveExcursion { .. }
                | SpecBinding::SaveCurrentBuffer { .. }
                | SpecBinding::SaveRestriction { .. }
                | SpecBinding::LoadsInProgress { .. }
                | SpecBinding::RequireStack { .. }
                | SpecBinding::NativeUnwind { .. }
                | SpecBinding::Nop => None,
            }));
        scratch.called.extend(
            scratch
                .frames
                .iter()
                .filter_map(|function| function.as_symbol_id(context.symbol_mode())),
        );
        scratch
            .called
            .sort_unstable_by_key(|symbol| symbol.to_raw());
        scratch.called.dedup();
        scratch
            .sources
            .extend(scratch.called.iter().filter_map(|&symbol| {
                context
                    .obarray
                    .indirect_function_id(symbol)
                    .map(|function| NamedSource {
                        symbol,
                        function,
                        _confined: PhantomData,
                    })
            }));
        scratch
            .roots
            .extend(scratch.sources.iter().map(|source| source.function));
        scratch.roots.extend(
            scratch
                .frames
                .iter()
                .copied()
                .filter(|function| function.is_bytecode()),
        );
        // The original frames.into_iter() ended immediately after this copy.
        scratch.frames.clear();
        let called = &scratch.called;
        let roots = &mut scratch.roots;
        super::cache::trace_redefined_pins(
            context.tagged_heap.tagged_heap_identity(),
            &|symbol| {
                called
                    .binary_search_by_key(&symbol.to_raw(), |s| s.to_raw())
                    .is_ok()
            },
            &mut |function| roots.push(function),
        );
        // The called-symbol temporary ended at prepare's return originally.
        // Retain allocations only; no stale frame word or symbol list survives.
        scratch.called.clear();
        Ok(Self {
            owner,
            heap,
            function_epoch: FunctionEpoch(context.obarray.function_epoch()),
            sources: std::mem::take(&mut scratch.sources),
            roots: std::mem::take(&mut scratch.roots),
            _confined: PhantomData,
        })
    }
    #[doc(hidden)]
    pub fn roots(&self) -> &[Value] {
        &self.roots
    }
    /// Only the ORIGINAL returning owner invokes this while publication lives.
    /// Retire dependents; never clear executable storage under a native frame.
    #[cold]
    #[allow(dead_code, reason = "standalone restoration retains the original API")]
    #[doc(hidden)]
    pub fn resume(self, context: &Context) -> Result<(), AdmissionError> {
        self.resume_reusing(context, &mut ParkedJitScratch::default())
    }

    #[cold]
    #[doc(hidden)]
    pub fn resume_reusing(
        mut self,
        context: &Context,
        scratch: &mut ParkedJitScratch,
    ) -> Result<(), AdmissionError> {
        if self.owner.0.get() != context.context_instance_id()
            || self.heap.0.get() != context.tagged_heap.tagged_heap_identity()
        {
            return Err(AdmissionError::ReturningOwner);
        }
        begin_owner_turn();
        for source in self.sources.drain(..) {
            super::cache::pin_redefined_function(source.symbol, source.function);
        }
        if self.function_epoch != FunctionEpoch(context.obarray.function_epoch()) {
            super::cache::retire_green_inline_dependents();
        }
        self.return_scratch(scratch);
        Ok(())
    }

    /// Preparation failed before publication or handoff: retain host capacity
    /// at the original drop point without running resume-side cache effects.
    #[doc(hidden)]
    pub fn return_scratch(mut self, scratch: &mut ParkedJitScratch) {
        self.sources.clear();
        self.roots.clear();
        debug_assert!(scratch.sources.is_empty() && scratch.roots.is_empty());
        std::mem::swap(&mut self.sources, &mut scratch.sources);
        std::mem::swap(&mut self.roots, &mut scratch.roots);
    }
}

/// Runs after acquisition, including return from a worker's final exit (which
/// may arm shared slots without yielding). No parked owner is accessed.
#[cold]
pub(crate) fn begin_owner_turn() {
    super::cache::invalidate_green_leaf_slots();
}
