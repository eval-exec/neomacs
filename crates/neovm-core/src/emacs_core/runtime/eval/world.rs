//! Stable shared storage, with one permanently confined Context owner.
//!
//! Storage extraction grants no scheduling or cross-thread access. Every owning
//! view carries its own cold allocation lease; hot borrowing reads only its
//! leading manager pointer. No mutable aggregate World reference reaches Lisp.

use super::*;
use crate::buffer::buffer::{BufferSelection, BufferStorage, OwnedBufferStorageParts};
use crate::emacs_core::defvar_bool::ByteBooleanVars;
use crate::emacs_core::symbol::CanonicalAccess;
use crate::emacs_core::symbol::{
    CanonicalDispatchCells, ConcealOutcome, DebuggerDescriptor, FunctionEpoch, FunctionEpochBump,
    LispVariableLocality, MakeAliasError, NotPlain, OBARRAY_DISPATCH_CELLS_OFFSET,
    RevealedProjection, SetInternalAlist, SetInternalBind,
};
use crate::heap_types::LispString;
use std::cell::{Cell, UnsafeCell};
use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::sync::Arc;

mod era;
pub use era::WorldEra;
pub(crate) use era::prepare_keeper_backing;
#[cfg(test)]
pub(crate) use era::{WorldProofEra, WorldProofEraFinishFailure};
mod green_owner;
mod slot;
mod sole_owner;
#[cfg(test)]
pub(crate) use green_owner::OwnedProofProjection;
#[allow(unused_imports)]
pub(crate) use green_owner::{
    Baton, FrozenLegacyRoots, OwnedParallelError, OwnedParallelProjection,
};
pub(crate) use slot::WorldSlot;
pub(crate) use sole_owner::{
    PublishedSoleOwnerDriver, SoleOwnerDriver, SoleOwnerRegistration, SoleOwnerVm,
};

/// Allocations only: no view or lease points back into this owner.
struct World {
    sole_context: sole_owner::SoleContextConstruction,
    owned_roots: Box<UnsafeCell<crate::emacs_core::owned_roots::OwnedRootRegistry>>,
    /// The heap's single provenance root (see
    /// [`crate::tagged::gc::AliasableBox`]): the owning accessors, the
    /// `Context`'s heap view and the TLS slot all derive from this pointer.
    tagged_heap: crate::tagged::gc::AliasableBox<crate::tagged::gc::TaggedHeap>,
    obarray: Box<UnsafeCell<Obarray>>,
    features: Box<UnsafeCell<Vec<SymId>>>,
    watchers: Box<UnsafeCell<VariableWatcherList>>,
    active_variable_watchers: Box<UnsafeCell<HashSet<SymId>>>,
    autoloads: Box<UnsafeCell<AutoloadManager>>,
    treesit: Box<UnsafeCell<super::super::treesit::TreeSitterManager>>,
    frames: Box<UnsafeCell<FrameManager>>,
    coding_systems: Box<UnsafeCell<CodingSystemManager>>,
    macro_perf_stats: Box<UnsafeCell<MacroPerfStats>>,
    buffers: Box<UnsafeCell<BufferStorage>>,
    /// GNU-global state every owner shares (round 3w).
    global: Box<UnsafeCell<super::global_state::GlobalState>>,
    /// Every owner's slab of dynamic bindings (Phase 0 step 2), for
    /// allocation and the collector's root phase.
    owner_slabs: std::cell::RefCell<super::owner_bindings::OwnerSlabRegistry>,
    /// Owner-local variable slot assignments (Phase 0 step 2): the
    /// descriptors admission and later materialization issue. Its stable
    /// address (inside the shared World allocation) is what a projection
    /// scope names (`symbol::ProjectionScope`).
    thread_projections: std::cell::RefCell<crate::emacs_core::forward::ThreadProjectionTable>,
    /// How this World runs Lisp threads, fixed at construction.
    threading: Cell<super::admission::ThreadingMode>,
    /// The Green runtime (round 3d), once started.
    green: std::cell::RefCell<Option<Box<super::green::GreenRuntime>>>,
    /// The OS thread holding the World's Baton, in a Green World
    /// (`green_owner`'s safety argument; checked in debug builds).
    baton_holder: Cell<Option<std::thread::ThreadId>>,
}

impl Drop for World {
    fn drop(&mut self) {
        self.finish_green_registry();
        // The obarray's projection scope names this World's table; it goes
        // with the World, on the World's own thread.
        let generation = self.obarray.get_mut().generation();
        crate::emacs_core::symbol::leave_projection_scope(generation);
    }
}

impl fmt::Debug for World {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("World")
            .field("storage", &"sealed manager allocations")
            .finish_non_exhaustive()
    }
}
static_assertions::assert_not_impl_any!(World: Send, Sync);

/// Allocation lifetime, never mutator authority or Lisp-object reachability.
/// Duplication and destruction occur only at cold ownership boundaries.
pub(crate) struct WorldLease(Arc<World>);
static_assertions::assert_not_impl_any!(WorldLease: Send, Sync, Clone);
static_assertions::assert_eq_size!(WorldLease, Arc<World>);

#[cfg(test)]
impl Context {
    /// Observe allocation lifetime without extending it or exposing World data.
    pub(in crate::emacs_core::eval) fn world_lifetime_probe_for_test(
        &self,
    ) -> impl Fn() -> bool + use<> {
        let world = Arc::downgrade(&self.obarray.lease.0);
        move || world.upgrade().is_some()
    }
}

impl fmt::Debug for WorldLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorldLease")
            .field("owners", &Arc::strong_count(&self.0))
            .finish()
    }
}

impl WorldLease {
    #[cold]
    pub(crate) fn duplicate_for_view(&self) -> Self {
        Self(Arc::clone(&self.0))
    }

    #[cold]
    fn into_exclusive(self) -> Result<ExclusiveWorldFinish, OutstandingWorldViews> {
        match Arc::try_unwrap(self.0) {
            Ok(world) => Ok(ExclusiveWorldFinish(world)),
            Err(world) => {
                // try_unwrap failed, so at least one other owning view exists.
                // Confinement forbids concurrent retirement; the fallback is
                // defensive metadata, never an access or reclamation permit.
                let count = NonZeroUsize::new(Arc::strong_count(&world).saturating_sub(1))
                    .unwrap_or(NonZeroUsize::MIN);
                Err(OutstandingWorldViews(count))
            }
        }
    }
}

/// A unique manager view. Moving the view also moves allocation lifetime.
#[repr(C)]
pub(crate) struct WorldView<T> {
    ptr: NonNull<T>,
    lease: WorldLease,
}
static_assertions::assert_not_impl_any!(WorldView<Obarray>: Send, Sync, Copy, Clone);
const _: () = assert!(std::mem::offset_of!(WorldView<Obarray>, ptr) == 0);

impl<T> fmt::Debug for WorldView<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorldView")
            .field("address", &self.ptr)
            .finish_non_exhaustive()
    }
}
impl<T> Deref for WorldView<T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        // SAFETY: the closed constructor creates exactly one mutable view of
        // this cell. Its strong lease retains allocation lifetime; this shared
        // borrow excludes a mutable borrow through the same owning view.
        unsafe { self.ptr.as_ref() }
    }
}
impl<T> DerefMut for WorldView<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: this unique view is exclusively borrowed. World exposes no
        // independent manager accessor and no safe second-view constructor.
        unsafe { self.ptr.as_mut() }
    }
}
impl WorldView<crate::tagged::gc::TaggedHeap> {
    /// The heap's provenance root.
    ///
    /// Raw-pointer users that keep a heap pointer across other heap accesses
    /// must start here, not at a `&mut *view` reborrow: a reference-derived
    /// pointer is a child tag that the next sibling reborrow freezes or pops
    /// (Tree/Stacked Borrows respectively).
    pub(crate) fn heap_root(&self) -> NonNull<crate::tagged::gc::TaggedHeap> {
        self.ptr
    }

    /// Install this view's heap in the calling thread's allocation slot.
    ///
    /// The slot is set from the view's provenance root, which is the same
    /// root every access through the view derives from, so later `&mut`
    /// reborrows through the view cannot invalidate the installed pointer.
    pub(crate) fn install_tagged_heap(&self) {
        crate::tagged::gc::set_tagged_heap_root(self.ptr);
    }
}

impl<T> AsRef<T> for WorldView<T> {
    #[inline(always)]
    fn as_ref(&self) -> &T {
        self
    }
}
impl<T> AsMut<T> for WorldView<T> {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut T {
        self
    }
}

impl<'a, T> IntoIterator for &'a WorldView<T>
where
    &'a T: IntoIterator,
{
    type Item = <&'a T as IntoIterator>::Item;
    type IntoIter = <&'a T as IntoIterator>::IntoIter;
    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.deref().into_iter()
    }
}

/// Shared-only root bookkeeping; its existing RefCell owns short write phases.
#[repr(C)]
pub(crate) struct OwnedRootRegistryView {
    ptr: NonNull<crate::emacs_core::owned_roots::OwnedRootRegistry>,
    lease: WorldLease,
}
static_assertions::assert_not_impl_any!(OwnedRootRegistryView: Send, Sync, Copy, Clone);
const _: () = assert!(std::mem::offset_of!(OwnedRootRegistryView, ptr) == 0);
impl fmt::Debug for OwnedRootRegistryView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnedRootRegistryView")
            .field("address", &self.ptr)
            .finish_non_exhaustive()
    }
}
impl Deref for OwnedRootRegistryView {
    type Target = crate::emacs_core::owned_roots::OwnedRootRegistry;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        // SAFETY: the lease retains this registry; no mutable registry view is
        // exposed. Its own RefCell protects the existing bookkeeping phase.
        unsafe { self.ptr.as_ref() }
    }
}

/// Owner-private admission words, never an independent World observer.
///
/// The epoch and stable debugger descriptor are copied only after a canonical
/// writer's manager loan has ended. The Boolean behind the descriptor remains
/// live: a collector or callback may change it before the next admission.
#[repr(C)]
#[derive(Debug)]
struct AdmissionHeader {
    epoch: Cell<FunctionEpoch>,
    debugger: Cell<DebuggerDescriptor>,
    thread_confined: PhantomData<*const ()>,
}
static_assertions::assert_not_impl_any!(AdmissionHeader: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(AdmissionHeader: fmt::Debug);
static_assertions::assert_eq_size!(Cell<FunctionEpoch>, u64);
static_assertions::assert_eq_size!(Cell<DebuggerDescriptor>, usize);
const _: () = {
    assert!(std::mem::offset_of!(AdmissionHeader, epoch) == 0);
    assert!(std::mem::offset_of!(AdmissionHeader, debugger) == std::mem::size_of::<u64>());
    assert!(
        std::mem::size_of::<AdmissionHeader>()
            == (std::mem::size_of::<u64>() + std::mem::size_of::<usize>())
                .next_multiple_of(std::mem::align_of::<AdmissionHeader>())
    );
};

impl AdmissionHeader {
    #[inline(always)]
    fn new(canonical: &CanonicalDispatchCells) -> Self {
        Self {
            epoch: Cell::new(canonical.admission_epoch()),
            debugger: Cell::new(canonical.admission_debugger()),
            thread_confined: PhantomData,
        }
    }

    /// The ONE retirement hook. It never resolves, allocates, asserts, calls
    /// Lisp, increments a clock, takes a lock, or waits for marker completion.
    ///
    /// An owner's own debugger cell stays: it names the owner's binding of
    /// `debug-on-next-call`, which no canonical write changes, and only the
    /// owner's unbind gives the canonical descriptor back
    /// ([`ObarrayView::point_debugger`]).
    #[inline(always)]
    fn retire_current_admission(&self, canonical: &CanonicalDispatchCells) {
        self.epoch.set(canonical.admission_epoch());
        if !self.debugger.get().is_owner() {
            self.debugger.set(canonical.admission_debugger());
        }
    }

    /// The retirement of a function-cell write: only the function clock
    /// moves. Such a write never changes the canonical debugger descriptor
    /// (resolution and installation do, in full-dispatch phases), so the
    /// header's debugger -- canonical, or this owner's own cell -- stays.
    #[inline(always)]
    fn retire_function_epoch(&self, canonical: &CanonicalDispatchCells) {
        self.epoch.set(canonical.admission_epoch());
    }

    #[inline(always)]
    fn function_epoch(&self) -> u64 {
        self.epoch.get().into()
    }

    #[inline(always)]
    fn debug_on_next_call_armed_fast(&self) -> bool {
        self.debugger.get().armed()
    }
}

/// Manager, canonical provenance, owner-private header and allocation lease
/// move together. No independently extracted header or mutable manager exists.
#[repr(C)]
pub(crate) struct ObarrayView {
    ptr: NonNull<Obarray>,
    dispatch: NonNull<CanonicalDispatchCells>,
    admission: AdmissionHeader,
    lease: WorldLease,
}
static_assertions::assert_not_impl_any!(ObarrayView: Send, Sync, Copy, Clone);
static_assertions::assert_impl_all!(ObarrayView: fmt::Debug);
const _: () = {
    assert!(std::mem::offset_of!(ObarrayView, ptr) == 0);
    assert!(std::mem::offset_of!(ObarrayView, dispatch) == std::mem::size_of::<usize>());
    assert!(std::mem::offset_of!(ObarrayView, admission) == 2 * std::mem::size_of::<usize>());
};
impl fmt::Debug for ObarrayView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObarrayView")
            .field("manager", &self.ptr)
            .field("canonical", &self.dispatch)
            .field("admission", &self.admission)
            .finish_non_exhaustive()
    }
}
impl Deref for ObarrayView {
    type Target = Obarray;
    #[inline(always)]
    fn deref(&self) -> &Obarray {
        // SAFETY: this sole owning view retains the stable manager allocation.
        // Shared readers exclude its closed mutation phases. Canonical lazy
        // resolution requires a mutable manager and cannot bypass retirement.
        unsafe { self.ptr.as_ref() }
    }
}

/// A read-complete descriptor publication on exactly its current owner.
///
/// Discovery finishes before construction, so a panic during discovery cannot
/// leave a canonical write or retirement obligation. The private receiver loan
/// excludes owner replacement, another writer or a callback before consuming
/// publication. This capability never transports a cached Boolean.
#[must_use]
struct PreparedDebuggerPublication<'owner> {
    owner: &'owner mut ObarrayView,
    resolved: Option<&'static crate::emacs_core::forward::LispBoolFwd>,
}
static_assertions::assert_not_impl_any!(PreparedDebuggerPublication<'static>: Send, Sync, Copy, Clone);
impl fmt::Debug for PreparedDebuggerPublication<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedDebuggerPublication")
            .field("owner", &std::ptr::from_ref(self.owner))
            .field("descriptor", &self.resolved.map(std::ptr::from_ref))
            .finish()
    }
}
impl PreparedDebuggerPublication<'_> {
    /// Only the no-fail pointer installation is inside the ONE retirement
    /// phase. There is no lookup, allocation, assertion or callback here.
    #[inline(always)]
    fn publish(self) -> Option<&'static crate::emacs_core::forward::LispBoolFwd> {
        let Self { owner, resolved } = self;
        owner.with_dispatch_write(|manager| manager.publish_debug_on_next_call_descriptor(resolved))
    }
}

/// A private phase-local retirement obligation. The destination is derived
/// from the CURRENT receiver, never retained by canonical World storage.
#[must_use]
struct AdmissionRetirement<'phase> {
    source: NonNull<CanonicalDispatchCells>,
    destination: NonNull<AdmissionHeader>,
    scope: RetirementScope,
    owner: PhantomData<&'phase mut ObarrayView>,
}

/// What a canonical-changing phase can have changed, so what its
/// retirement republishes. Fixed per call site, so it folds away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RetirementScope {
    /// Anything: the function clock and the debugger descriptor.
    Dispatch,
    /// A function cell: the function clock alone.
    FunctionCell,
}
static_assertions::assert_not_impl_any!(AdmissionRetirement<'static>: Send, Sync, Clone, Copy);
impl fmt::Debug for AdmissionRetirement<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdmissionRetirement")
            .field("source", &self.source)
            .field("destination", &self.destination)
            .field("scope", &self.scope)
            .finish()
    }
}
impl<'phase> AdmissionRetirement<'phase> {
    #[inline(always)]
    fn new(view: &'phase mut ObarrayView, scope: RetirementScope) -> Self {
        Self {
            source: view.dispatch,
            destination: NonNull::from(&mut view.admission),
            scope,
            owner: PhantomData,
        }
    }
}
impl Drop for AdmissionRetirement<'_> {
    #[inline(always)]
    fn drop(&mut self) {
        // SAFETY: the enclosing exclusive phase retains both the current
        // receiver and its bundled allocation lease. Its nonescaping manager
        // loan has ended (also when unwinding) before this guard retires. The
        // source derives from the finalized UnsafeCell root, not a temporary
        // manager reference. No callback observes this phase-local guard.
        unsafe {
            let destination = self.destination.as_ref();
            match self.scope {
                RetirementScope::Dispatch => {
                    destination.retire_current_admission(self.source.as_ref());
                }
                RetirementScope::FunctionCell => {
                    destination.retire_function_epoch(self.source.as_ref());
                }
            }
        }
    }
}

/// Safe method-only mutable access to an owning Obarray view.
///
/// There is no mutable Deref, raw manager/interior accessor, pending Drop
/// obligation, or independently movable header. Every canonical-changing
/// method publishes before returning, including Rust unwind; forgetting this
/// adapter cannot skip retirement. Data-only methods expose only their audited
/// operation. The private borrow prevents moving/swapping the owning view.
#[must_use]
pub struct ObarrayWriteAccess<'ctx> {
    view: &'ctx mut ObarrayView,
}
static_assertions::assert_not_impl_any!(ObarrayWriteAccess<'static>: Send, Sync, Copy, Clone);
static_assertions::assert_impl_all!(ObarrayWriteAccess<'static>: fmt::Debug);
static_assertions::assert_eq_size!(ObarrayWriteAccess<'static>, usize);
impl fmt::Debug for ObarrayWriteAccess<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObarrayWriteAccess")
            .field("view", &self.view)
            .finish()
    }
}
impl Deref for ObarrayWriteAccess<'_> {
    type Target = Obarray;
    #[inline(always)]
    fn deref(&self) -> &Obarray {
        self.view
    }
}
impl<'ctx> ObarrayWriteAccess<'ctx> {
    #[inline(always)]
    pub(crate) fn new(view: &'ctx mut ObarrayView) -> Self {
        Self { view }
    }
}

impl ObarrayView {
    /// Issue a pointer-free phase registration from the closed World policy.
    ///
    /// The existing lease keeps allocations alive; its Arc count does not
    /// establish sole Context ownership. The private construction marker does.
    /// This grants no second manager view and performs no lease operation.
    #[inline(always)]
    pub(crate) fn sole_owner_registration(&self) -> SoleOwnerRegistration {
        self.lease.0.sole_context.registration()
    }

    #[cfg(feature = "jit")]
    pub(crate) const ADMISSION_HEADER_OFFSET: usize = std::mem::offset_of!(Self, admission);
    #[cfg(feature = "jit")]
    pub(crate) const ADMISSION_EPOCH_OFFSET: usize =
        Self::ADMISSION_HEADER_OFFSET + std::mem::offset_of!(AdmissionHeader, epoch);
    #[cfg(feature = "jit")]
    pub(crate) const ADMISSION_DEBUGGER_OFFSET: usize =
        Self::ADMISSION_HEADER_OFFSET + std::mem::offset_of!(AdmissionHeader, debugger);

    #[cold]
    pub(super) fn closing_lease(&self) -> WorldLease {
        self.lease.duplicate_for_view()
    }

    /// Refresh only this original owner's current receiver after cold setup.
    /// This grants no second owner or scheduling authority. If a later stage
    /// admits a second owner, its admission and ORIGINAL returning continuation
    /// must refresh through their own current receiver before reading the header.
    /// Neither the scheduler nor World may retain a destination or refresh a
    /// parked Context beneath its live protector; that protocol is not built here.
    #[cold]
    pub(super) fn refresh_admission(&mut self) {
        // SAFETY: no manager loan overlaps this exclusive receiver borrow;
        // canonical source and current destination are kept by one view/lease.
        self.admission
            .retire_current_admission(unsafe { self.dispatch.as_ref() });
    }

    /// Name OWNER_CELL as this owner's `debug-on-next-call` while the owner
    /// binds it, or the canonical descriptor again (`None`).
    #[cold]
    pub(crate) fn point_debugger(
        &mut self,
        owner_cell: Option<&'static crate::emacs_core::forward::LispBoolFwd>,
    ) {
        let descriptor = match owner_cell {
            Some(cell) => DebuggerDescriptor::owner(cell),
            // SAFETY: as in `refresh_admission`.
            None => unsafe { self.dispatch.as_ref() }.admission_debugger(),
        };
        self.admission.debugger.set(descriptor);
    }

    /// Test the actual descriptor identity, not just two equal Boolean values.
    #[cfg(test)]
    pub(crate) fn admission_matches_canonical(&self) -> bool {
        // SAFETY: this shared borrow excludes both manager writers and header
        // retirement; the allocation-root source is kept by this same lease.
        let canonical = unsafe { self.dispatch.as_ref() };
        self.admission.epoch.get() == canonical.admission_epoch()
            && self.admission.debugger.get() == canonical.admission_debugger()
    }

    /// Canonical-changing proxies alone use this private, noncalling phase.
    #[inline(always)]
    fn with_dispatch_write<R>(
        &mut self,
        write: impl for<'manager> FnOnce(&'manager mut Obarray) -> R,
    ) -> R {
        self.with_retiring_write(RetirementScope::Dispatch, write)
    }

    /// [`Self::with_dispatch_write`] for a function-cell write, which moves
    /// only the function clock ([`AdmissionHeader::retire_function_epoch`]).
    #[inline(always)]
    fn with_function_write<R>(
        &mut self,
        write: impl for<'manager> FnOnce(&'manager mut Obarray) -> R,
    ) -> R {
        self.with_retiring_write(RetirementScope::FunctionCell, write)
    }

    #[inline(always)]
    fn with_retiring_write<R>(
        &mut self,
        scope: RetirementScope,
        write: impl for<'manager> FnOnce(&'manager mut Obarray) -> R,
    ) -> R {
        let mut manager = self.ptr;
        let retirement = AdmissionRetirement::new(self, scope);
        // SAFETY: this sole view is exclusively borrowed; the closure's manager
        // loan cannot escape, call Lisp or overlap an admission reader. Every
        // private caller uses canonical producers, preserving manager storage.
        let result = write(unsafe { manager.as_mut() });
        drop(retirement);
        result
    }

    /// Compatibility quarantine for existing helpers requiring a whole manager.
    /// Safe public callers instead use ObarrayWriteAccess's closed operations.
    ///
    /// # Safety
    /// The phase must not call Lisp or admit native/JIT execution. It must
    /// preserve this admitted manager's storage/generation: no replacement,
    /// swap, or independently retained raw/interior mutation authority. Function
    /// writes must use canonical Obarray producers, never raw LispSymbol fields;
    /// descriptor writes must use the canonical installation/resolution hooks.
    /// No manager, canonical or header reader may overlap the mutable phase.
    #[inline(always)]
    pub(crate) unsafe fn with_legacy_storage_write<R>(
        &mut self,
        write: impl for<'manager> FnOnce(&'manager mut Obarray) -> R,
    ) -> R {
        self.with_dispatch_write(write)
    }

    /// Data-only compatibility quarantine, with NO admission publication.
    ///
    /// # Safety
    /// The helper must neither call Lisp nor mutate function cells, function
    /// stamps/floors/clock, the canonical debugger descriptor, manager storage
    /// identity or generation. It may perform audited value/property/BLV-cache
    /// operations and live forwarded Boolean stores. No raw or interior mutable
    /// authority may escape or overlap another manager/header phase. A new
    /// helper needs its own data-only audit; the stronger contract is not
    /// interchangeable with with_legacy_storage_write's publishing effect.
    #[inline(always)]
    pub(crate) unsafe fn with_legacy_value_write<R>(
        &mut self,
        write: impl for<'manager> FnOnce(&'manager mut Obarray) -> R,
    ) -> R {
        // SAFETY: this sole view's exclusive borrow covers the nonescaping
        // manager phase; the caller supplies the audited data-only effect.
        write(unsafe { self.ptr.as_mut() })
    }

    #[inline(always)]
    pub(crate) fn function_epoch(&self) -> u64 {
        self.admission.function_epoch()
    }

    #[inline(always)]
    pub(crate) fn debug_on_next_call_armed_fast(&self) -> bool {
        self.admission.debug_on_next_call_armed_fast()
    }

    #[inline(always)]
    pub(crate) fn debug_on_next_call_bool_fwd_cached(
        &self,
    ) -> Option<&'static crate::emacs_core::forward::LispBoolFwd> {
        self.admission.debugger.get().resolved()
    }

    #[inline(always)]
    pub(crate) fn debug_on_next_call_bool_fwd(
        &mut self,
        id: SymId,
    ) -> Option<&'static crate::emacs_core::forward::LispBoolFwd> {
        // Discovery is a pure manager read. Its loan ends and any possible
        // panic precedes the publication capability and retirement guard.
        let resolved = self.discover_debug_on_next_call_descriptor(id);
        PreparedDebuggerPublication {
            owner: self,
            resolved,
        }
        .publish()
    }

    /// Ensure storage without exposing mutable LispSymbol function/flag fields.
    #[inline(always)]
    pub fn ensure_symbol_id(&mut self, id: SymId) {
        // SAFETY: the exclusive view grants this short nonpublishing phase.
        // Only membership/storage changes; no mutable interior leaves it.
        let _ = unsafe { self.ptr.as_mut().ensure_symbol_id(id) };
    }

    /// Intern membership without exposing a mutable LispSymbol.
    #[inline(always)]
    pub fn get_or_intern(&mut self, name: &str) {
        // SAFETY: this same sealed membership-only phase returns no interior.
        let _ = unsafe { self.ptr.as_mut().get_or_intern(name) };
    }

    /// Localize storage, preserving the stable forwarded descriptor identity.
    /// The manager's raw BLV designator is deliberately not returned.
    #[inline(always)]
    pub fn make_symbol_localized(&mut self, id: SymId, default: Value) {
        // SAFETY: this short exclusive phase preserves canonical dispatch;
        // neither the newly allocated nor existing mutable BLV escapes.
        let _ = unsafe { self.ptr.as_mut().make_symbol_localized(id, default) };
    }

    #[inline(always)]
    pub fn replace_symbol_plist_id<I>(&mut self, symbol: SymId, entries: I)
    where
        I: IntoIterator<Item = (SymId, Value)>,
    {
        // SAFETY: the private exclusive phase changes only the property list;
        // no manager/interior reference escapes the iterator-driven operation.
        unsafe { self.ptr.as_mut().replace_symbol_plist_id(symbol, entries) }
    }
}

impl ObarrayWriteAccess<'_> {
    /// Read the current owner's coherent function clock without a World hop.
    #[inline(always)]
    pub fn function_epoch(&self) -> u64 {
        self.view.function_epoch()
    }
    #[cfg(test)]
    #[inline(always)]
    pub(crate) fn invalidate_all_function_bindings(&mut self, why: FunctionEpochBump) {
        self.view.invalidate_all_function_bindings(why);
    }
    #[inline(always)]
    pub fn ensure_symbol_id(&mut self, id: SymId) {
        self.view.ensure_symbol_id(id);
    }
    #[inline(always)]
    pub fn get_or_intern(&mut self, name: &str) {
        self.view.get_or_intern(name);
    }
    #[inline(always)]
    pub fn make_symbol_localized(&mut self, id: SymId, default: Value) {
        self.view.make_symbol_localized(id, default);
    }
    #[inline(always)]
    pub fn replace_symbol_plist_id<I>(&mut self, symbol: SymId, entries: I)
    where
        I: IntoIterator<Item = (SymId, Value)>,
    {
        self.view.replace_symbol_plist_id(symbol, entries);
    }
}

// An explicit sealed list, not a generic safe mutable-manager escape. Adding a
// new Obarray writer does not automatically expose it on an owning view. The
// compiler rejects its use until its data/dispatch effect is classified here.
macro_rules! obarray_write_operation {
    (data, $view:expr, $name:ident, $($arg:ident),*) => {{
        // SAFETY: the sole view's exclusive borrow covers this audited
        // data-only operation. No manager/interior reference escapes and no
        // canonical function clock or debugger descriptor can change.
        unsafe { $view.ptr.as_mut().$name($($arg),*) }
    }};
    (dispatch, $view:expr, $name:ident, $($arg:ident),*) => {
        $view.with_dispatch_write(|manager| manager.$name($($arg),*))
    };
    (function, $view:expr, $name:ident, $($arg:ident),*) => {
        $view.with_function_write(|manager| manager.$name($($arg),*))
    };
}
// Effect and access are independent compile-time domains. Internal-only
// operations stay on the confined View; only needed methods reach the public
// adapter. Neither axis introduces a run-time mode check or fallback arm.
macro_rules! obarray_method_proxies {
    ($phase:ident; view_only; $( $(#[$attr:meta])* $visibility:vis fn $name:ident(
        $($arg:ident: $ty:ty),* $(,)?
    ) $(-> $result:ty)?; )*) => {
        impl ObarrayView {
            $(
                $(#[$attr])*
                #[inline(always)]
                $visibility fn $name(&mut self, $($arg: $ty),*) $(-> $result)? {
                    obarray_write_operation!($phase, self, $name, $($arg),*)
                }
            )*
        }
    };
    ($phase:ident; public_access; $( $(#[$attr:meta])* $visibility:vis fn $name:ident(
        $($arg:ident: $ty:ty),* $(,)?
    ) $(-> $result:ty)?; )*) => {
        obarray_method_proxies! {
            $phase; view_only;
            $(
                $(#[$attr])*
                $visibility fn $name($($arg: $ty),*) $(-> $result)?;
            )*
        }
        impl ObarrayWriteAccess<'_> {
            $(
                $(#[$attr])*
                #[inline(always)]
                $visibility fn $name(&mut self, $($arg: $ty),*) $(-> $result)? {
                    self.view.$name($($arg),*)
                }
            )*
        }
    };
}

obarray_method_proxies! {
    data; public_access;
    pub fn intern(name: &str) -> String;
    pub fn intern_lisp_string(name: &LispString) -> SymId;
    pub fn intern_lisp_value(name_value: crate::tagged::value::TaggedValue) -> SymId;
    pub fn ensure_interned_global_id(id: SymId);
    pub(crate) fn materialize_read_symbols(value: Value);
    #[cfg(test)]
    pub fn set_symbol_value(name: &str, value: Value);
    pub fn seed_value(name: &str, value: Value);
    pub fn set_canonical_value_by_name(access: CanonicalAccess,
            name: &str,
            value: Value,
        );
    pub fn define_lisp_variable(name: &str,
            value: Value,
            locality: LispVariableLocality,
        );
    #[cfg(test)]
    pub fn set_symbol_value_id(id: SymId, value: Value);
    pub fn set_canonical_value(access: CanonicalAccess,
            id: SymId,
            value: Value,
        );
    pub fn set_blv_local_if_set(id: SymId, local_if_set: bool);
    pub fn reset_blv_cache_to_default(id: SymId);
    pub fn reset_blv_cache_for_buffer(id: SymId, buffer: Value);
    pub fn find_symbol_value_in_buffer(id: SymId,
            _current_buffer_id: Option<crate::buffer::BufferId>,
            current_buffer_value: Value,
            local_var_alist: Value,
            current_buffer_slots: Option<&[Value]>,
            current_buffer_local_flags: u64,
            buffer_defaults: Option<&[Value]>,
        ) -> Option<Value>;
    #[cfg(test)]
    pub fn set_internal(id: SymId, value: Value);
    pub fn for_each_value_cell_mut(f: impl FnMut(&mut Value));
    #[cfg(test)]
    pub fn makunbound(name: &str);
    #[cfg(test)]
    pub fn makunbound_id(id: SymId);
    pub fn makunbound_canonical_by_name(access: CanonicalAccess,
            name: &str,
        );
    pub fn makunbound_canonical(access: CanonicalAccess, id: SymId);
    pub(crate) fn set_canonical_value_routed(id: SymId,
            value: Value,
        ) -> crate::emacs_core::symbol::CanonicalWrite;
    pub(crate) fn makunbound_canonical_routed(id: SymId) -> crate::emacs_core::symbol::CanonicalWrite;
    pub fn put_property(name: &str, prop: &str, value: Value) -> Result<(), Flow>;
    pub fn put_property_id(symbol: SymId,
            prop: SymId,
            value: Value,
        ) -> Result<(), Flow>;
    pub fn set_symbol_plist_id(symbol: SymId, plist: Value);
    pub fn make_special(name: &str);
    pub fn define_special_variable(name: &str, value: Value);
    pub fn define_c_hook_variable(name: &str);
    pub fn make_special_id(id: SymId);
    pub fn make_non_special(name: &str);
    pub fn make_non_special_id(id: SymId);
    pub fn set_constant(name: &str);
    pub fn set_constant_id(id: SymId);
    pub fn make_buffer_local(name: &str, local_if_set: bool);
    pub fn make_alias(id: SymId, target: SymId) -> Result<(), MakeAliasError>;
    pub fn delete_variable_alias_id(id: SymId);
    pub fn make_variable_alias(new_alias: SymId,
            base: SymId,
        ) -> Result<(), MakeAliasError>;
}

obarray_method_proxies! {
    data; view_only;
    pub(crate) fn note_watchers_changed(id: SymId, has_watchers: bool);
    pub(crate) fn swap_plain_untrapped_value_id(id: SymId,
            value: Value,
        ) -> Option<Value>;
    pub(crate) fn set_plain_untrapped_value_id(id: SymId, value: Value) -> bool;
    pub(crate) fn mark_runtime_projected_id(id: SymId);
    pub(crate) fn reveal_projection(id: SymId) -> Option<RevealedProjection>;
    pub(crate) fn owned_thread_projection(id: SymId)
        -> Option<&'static crate::emacs_core::forward::ThreadProjectionDescriptor>;
    pub(crate) fn conceal_projection(revealed: RevealedProjection) -> ConcealOutcome;
    pub(crate) fn store_plain_value_id(id: SymId, value: Value) -> Result<(), NotPlain>;
    pub(crate) fn set_internal_localized(sym_id: SymId,
            value: Value,
            target_buf: Value,
            target_alist: Value,
            bindflag: SetInternalBind,
            let_shadows: bool,
        ) -> SetInternalAlist;
    pub(crate) fn set_internal_localized_with(sym_id: SymId,
            value: Value,
            target_buf: Value,
            target_alist: Value,
            bindflag: SetInternalBind,
            let_shadows: impl FnOnce() -> bool,
        ) -> SetInternalAlist;
}

obarray_method_proxies! {
    dispatch; public_access;
    pub fn install_buffer_objfwd(id: SymId,
            fwd: &'static crate::emacs_core::forward::LispBufferObjFwd,
        );
    pub fn install_boolfwd(id: SymId,
            fwd: &'static crate::emacs_core::forward::LispBoolFwd,
        );
    pub fn define_bool_variable(name: &str,
            initial: bool,
            byte_boolean_vars: ByteBooleanVars,
        );
    pub fn reattach_localized_forwarder(id: SymId,
            kind: crate::emacs_core::symbol::LocalizedForwarderKind,
        );
    pub fn install_intfwd(id: SymId,
            fwd: &'static crate::emacs_core::forward::LispIntFwd,
        );
    pub fn install_objfwd(id: SymId,
            fwd: &'static crate::emacs_core::forward::LispObjFwd,
        );
    pub fn install_kboard_objfwd(id: SymId,
            fwd: &'static crate::emacs_core::forward::LispKboardObjFwd,
        );
    pub fn define_int_variable(name: &str, initial: i64);
    pub fn unintern_name(name: &str) -> bool;
    pub fn unintern_lisp_string(name: &LispString) -> bool;
    pub fn unintern_id(id: SymId) -> bool;
    #[cfg(test)]
    pub(crate) fn set_function_epoch_for_test(epoch: u64);
}

obarray_method_proxies! {
    dispatch; view_only;
    pub(crate) fn invalidate_all_function_bindings(why: FunctionEpochBump);
}

// Function-cell writes: they move the function clock and never the
// canonical debugger descriptor.
obarray_method_proxies! {
    function; public_access;
    pub fn set_symbol_function(name: &str, function: Value);
    pub fn set_symbol_function_id(id: SymId, function: Value);
    pub(crate) fn set_symbol_function_id_for(id: SymId,
            function: Value,
            why: FunctionEpochBump,
        );
    pub fn fmakunbound(name: &str);
    pub fn fmakunbound_id(id: SymId);
    pub fn clear_function_silent(name: &str);
    pub fn clear_function_silent_id(id: SymId);
}

/// Pure Context-bound owner-header read; no manager reference spans Lisp.
#[must_use]
pub(crate) struct DispatchReadProjection<'ctx> {
    ptr: NonNull<AdmissionHeader>,
    owner: PhantomData<&'ctx Context>,
}
static_assertions::assert_not_impl_any!(DispatchReadProjection<'static>: Send, Sync, Copy, Clone);
static_assertions::assert_eq_size!(DispatchReadProjection<'static>, usize);
impl fmt::Debug for DispatchReadProjection<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DispatchReadProjection")
            .field("address", &self.ptr)
            .finish()
    }
}
impl DispatchReadProjection<'_> {
    #[inline(always)]
    pub(crate) fn function_epoch(&self) -> u64 {
        // SAFETY: this pointer derives from the CURRENT Context receiver;
        // its borrow excludes writer retirement and retains the bundled view.
        unsafe { self.ptr.as_ref().function_epoch() }
    }
    #[inline(always)]
    pub(crate) fn debug_on_next_call_armed_fast(&self) -> bool {
        // SAFETY: the same current borrowed header contains a stable
        // descriptor; its live forwarded Boolean is reread, never mirrored.
        unsafe { self.ptr.as_ref().debug_on_next_call_armed_fast() }
    }
}

/// Existing exclusive clock read, tied to the whole mutable Context phase.
#[must_use]
#[cfg(feature = "jit")]
pub(crate) struct DispatchExclusiveProjection<'ctx> {
    ptr: NonNull<AdmissionHeader>,
    owner: PhantomData<&'ctx mut Context>,
}
#[cfg(feature = "jit")]
static_assertions::assert_not_impl_any!(DispatchExclusiveProjection<'static>: Send, Sync, Copy, Clone);
#[cfg(feature = "jit")]
static_assertions::assert_eq_size!(DispatchExclusiveProjection<'static>, usize);
#[cfg(feature = "jit")]
impl fmt::Debug for DispatchExclusiveProjection<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DispatchExclusiveProjection")
            .field("address", &self.ptr)
            .finish()
    }
}
#[cfg(feature = "jit")]
impl DispatchExclusiveProjection<'_> {
    #[inline(always)]
    pub(crate) fn function_epoch(&mut self) -> u64 {
        // SAFETY: the originating whole-Context mutable borrow excludes every
        // other header or writer phase. No second World facade exists.
        unsafe { self.ptr.as_ref().function_epoch() }
    }
    #[inline(always)]
    pub(crate) fn debug_on_next_call_armed_fast(&self) -> bool {
        // SAFETY: this pointer names the current exclusive owner's header.
        unsafe { self.ptr.as_ref().debug_on_next_call_armed_fast() }
    }
}
impl Context {
    #[inline(always)]
    pub(crate) fn dispatch_projection(&self) -> DispatchReadProjection<'_> {
        DispatchReadProjection {
            ptr: NonNull::from(&self.obarray.admission),
            owner: PhantomData,
        }
    }
    #[inline(always)]
    #[cfg(feature = "jit")]
    pub(crate) fn dispatch_projection_exclusive(&mut self) -> DispatchExclusiveProjection<'_> {
        DispatchExclusiveProjection {
            ptr: NonNull::from(&mut self.obarray.admission),
            owner: PhantomData,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0} World storage views remain after Context retirement")]
pub struct OutstandingWorldViews(NonZeroUsize);
static_assertions::assert_impl_all!(OutstandingWorldViews: Send,Sync);

#[derive(Debug, thiserror::Error)]
pub enum ContextShutdownError {
    #[error("GC marker completion failed: {0}")]
    MarkerFinish(#[from] crate::tagged::gc::MarkFinishError),
    #[error("World storage is still leased: {0}")]
    OutstandingViews(#[from] OutstandingWorldViews),
}
static_assertions::assert_impl_all!(ContextShutdownError: Send,Sync);

#[must_use]
struct ExclusiveWorldFinish(World);
static_assertions::assert_not_impl_any!(ExclusiveWorldFinish: Send,Sync);
impl fmt::Debug for ExclusiveWorldFinish {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExclusiveWorldFinish")
            .finish_non_exhaustive()
    }
}
impl ExclusiveWorldFinish {
    #[cold]
    fn finish(mut self) -> Result<(), ContextShutdownError> {
        // SAFETY: consuming uniqueness proof means no Context or extracted
        // owning view survives. Marker completion and TLS retirement already
        // happened; only noncalling destruction follows explicit reclamation.
        unsafe {
            self.0.tagged_heap.get_mut().shutdown_owned_resources()?;
        }
        Ok(())
    }
}

#[inline]
fn cell_ptr<T>(cell: &UnsafeCell<T>) -> NonNull<T> {
    // SAFETY: UnsafeCell::get points into this live cell allocation and cannot
    // be null. Derive from the cell authority, never a temporary T reference.
    unsafe { NonNull::new_unchecked(cell.get()) }
}

/// Closed factory input; managers are finalized before any view is published.
pub(super) struct WorldStorageBuilder {
    pub(super) owned_roots: crate::emacs_core::owned_roots::OwnedRootRegistry,
    pub(super) tagged_heap: crate::tagged::gc::AliasableBox<crate::tagged::gc::TaggedHeap>,
    pub(super) obarray: Obarray,
    pub(super) features: Vec<SymId>,
    pub(super) watchers: VariableWatcherList,
    pub(super) active_variable_watchers: HashSet<SymId>,
    pub(super) autoloads: AutoloadManager,
    pub(super) treesit: super::super::treesit::TreeSitterManager,
    pub(super) frames: FrameManager,
    pub(super) coding_systems: CodingSystemManager,
    pub(super) macro_perf_stats: MacroPerfStats,
    pub(super) buffers: OwnedBufferStorageParts,
    pub(super) global: super::global_state::GlobalState,
}

static_assertions::assert_not_impl_any!(WorldStorageBuilder: Send, Sync, Clone);
impl fmt::Debug for WorldStorageBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorldStorageBuilder")
            .field("storage", &"unpublished manager allocations")
            .finish_non_exhaustive()
    }
}

pub(super) struct ReadyWorld {
    pub(super) owned_roots: OwnedRootRegistryView,
    pub(super) tagged_heap: WorldView<crate::tagged::gc::TaggedHeap>,
    pub(super) obarray: ObarrayView,
    pub(super) features: WorldView<Vec<SymId>>,
    pub(super) watchers: WorldView<VariableWatcherList>,
    pub(super) active_variable_watchers: WorldView<HashSet<SymId>>,
    pub(super) autoloads: WorldView<AutoloadManager>,
    pub(super) treesit: WorldView<super::super::treesit::TreeSitterManager>,
    pub(super) frames: WorldView<FrameManager>,
    pub(super) coding_systems: WorldView<CodingSystemManager>,
    pub(super) macro_perf_stats: WorldView<MacroPerfStats>,
    pub(super) buffers: BufferManager,
    pub(super) global: super::global_state::GlobalSlots,
}

static_assertions::assert_not_impl_any!(ReadyWorld: Send, Sync, Clone);
impl fmt::Debug for ReadyWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadyWorld")
            .field("storage", &"unique leased views")
            .finish_non_exhaustive()
    }
}

impl WorldStorageBuilder {
    #[cold]
    pub(super) fn build(self) -> ReadyWorld {
        let (buffers, selection) = self.buffers.into_parts();
        // The owner already holds the allocation's single provenance root, so
        // moving it into the World is a plain move: the address and the root
        // tag survive it. The TLS installation below re-derives from that
        // root, never from a `&mut` borrow of the heap.
        let heap = self.tagged_heap;
        let lease = WorldLease(Arc::new(World {
            sole_context: sole_owner::SoleContextConstruction::new(),
            owned_roots: Box::new(UnsafeCell::new(self.owned_roots)),
            tagged_heap: heap,
            obarray: Box::new(UnsafeCell::new(self.obarray)),
            features: Box::new(UnsafeCell::new(self.features)),
            watchers: Box::new(UnsafeCell::new(self.watchers)),
            active_variable_watchers: Box::new(UnsafeCell::new(self.active_variable_watchers)),
            autoloads: Box::new(UnsafeCell::new(self.autoloads)),
            treesit: Box::new(UnsafeCell::new(self.treesit)),
            frames: Box::new(UnsafeCell::new(self.frames)),
            coding_systems: Box::new(UnsafeCell::new(self.coding_systems)),
            macro_perf_stats: Box::new(UnsafeCell::new(self.macro_perf_stats)),
            buffers,
            global: Box::new(UnsafeCell::new(self.global)),
            owner_slabs: Default::default(),
            thread_projections: Default::default(),
            threading: Cell::new(super::admission::ThreadingMode::from_env()),
            green: Default::default(),
            baton_holder: Cell::new(None),
        }));
        // Reinstall the upstream TLS slot from the finalized heap's root
        // before any allocation-capable Context setup resumes. The root (not
        // a `&mut` borrow) is what makes the slot survive every sibling
        // access the owner later takes.
        crate::tagged::gc::set_tagged_heap_root(lease.0.tagged_heap.root());
        Self::publish_views(&lease, selection)
    }

    /// The unique set of manager views one owner Context holds over the
    /// World LEASE names, with SELECTION as its current buffer.
    #[cold]
    fn publish_views(lease: &WorldLease, selection: BufferSelection) -> ReadyWorld {
        let ptr = cell_ptr(&lease.0.obarray);
        // SAFETY: final stable UnsafeCell allocation root, not a temporary
        // manager reference. Embedded repr(C) canonical pair has this const
        // offset; whole-manager phases finish before subsequent raw reads.
        let dispatch = unsafe {
            NonNull::new_unchecked(
                ptr.as_ptr()
                    .cast::<u8>()
                    .add(OBARRAY_DISPATCH_CELLS_OFFSET)
                    .cast::<CanonicalDispatchCells>(),
            )
        };
        ReadyWorld {
            owned_roots: OwnedRootRegistryView {
                ptr: cell_ptr(&lease.0.owned_roots),
                lease: lease.duplicate_for_view(),
            },
            tagged_heap: WorldView {
                // The heap's provenance root, not a fresh `UnsafeCell::get`
                // (there is no cell around the heap any more): this copy and
                // the TLS slot must name the same tag.
                ptr: lease.0.tagged_heap.root(),
                lease: lease.duplicate_for_view(),
            },
            obarray: ObarrayView {
                ptr,
                dispatch,
                // SAFETY: this finalized canonical root has no manager loan;
                // both constructors install a valid non-null sentinel/cell.
                admission: AdmissionHeader::new(unsafe { dispatch.as_ref() }),
                lease: lease.duplicate_for_view(),
            },
            features: WorldView {
                ptr: cell_ptr(&lease.0.features),
                lease: lease.duplicate_for_view(),
            },
            watchers: WorldView {
                ptr: cell_ptr(&lease.0.watchers),
                lease: lease.duplicate_for_view(),
            },
            active_variable_watchers: WorldView {
                ptr: cell_ptr(&lease.0.active_variable_watchers),
                lease: lease.duplicate_for_view(),
            },
            autoloads: WorldView {
                ptr: cell_ptr(&lease.0.autoloads),
                lease: lease.duplicate_for_view(),
            },
            treesit: WorldView {
                ptr: cell_ptr(&lease.0.treesit),
                lease: lease.duplicate_for_view(),
            },
            frames: WorldView {
                ptr: cell_ptr(&lease.0.frames),
                lease: lease.duplicate_for_view(),
            },
            coding_systems: WorldView {
                ptr: cell_ptr(&lease.0.coding_systems),
                lease: lease.duplicate_for_view(),
            },
            macro_perf_stats: WorldView {
                ptr: cell_ptr(&lease.0.macro_perf_stats),
                lease: lease.duplicate_for_view(),
            },
            // SAFETY: the World's stable cell, kept alive by this owner's
            // leases; World state is touched by one owner at a time.
            global: unsafe { super::global_state::GlobalSlots::over(cell_ptr(&lease.0.global)) },
            // SAFETY: World owns the exact stable cell and this is its sole
            // selection/view; the supplied lease is for that same World.
            buffers: unsafe {
                BufferManager::from_world_parts(
                    cell_ptr(&lease.0.buffers),
                    selection,
                    lease.duplicate_for_view(),
                )
            },
        }
    }
}

/// Owner slabs (Phase 0 step 2): the World allocates and traces them.
impl Context {
    /// Allocate this Context's owner slab in its World's registry.
    #[cold]
    #[inline(never)]
    pub(super) fn register_owner_slab(
        &mut self,
    ) -> std::ptr::NonNull<super::owner_bindings::OwnerSlab> {
        let heap = self.tagged_heap.identity();
        self.obarray.lease.0.owner_slabs.borrow_mut().register(heap)
    }

    /// Visit every owner slab's values -- each owner's bindings are roots,
    /// whether or not that owner is running -- and the canonical plain word
    /// of every projected variable, which the World's table roots.
    pub(super) fn trace_owner_slabs(&self, heap_id: usize, visit: &mut dyn FnMut(Value)) {
        let world = &self.obarray.lease.0;
        world.owner_slabs.borrow().trace_roots(heap_id, visit);
        world.thread_projections.borrow().trace_roots(visit);
        if let Some(runtime) = world.green.borrow().as_ref() {
            runtime.trace_roots(visit);
        }
    }
}

/// Threading mode and admission state (Phase 0 step 2).
impl Context {
    /// How this Context's World runs Lisp threads.
    #[inline]
    pub(crate) fn threading_mode(&self) -> super::admission::ThreadingMode {
        self.obarray.lease.0.threading.get()
    }

    /// Fix the World's threading mode. Only construction calls this, before
    /// any Lisp runs (`Context::new_with_threading`).
    #[cold]
    pub(super) fn set_threading_mode_at_construction(
        &mut self,
        mode: super::admission::ThreadingMode,
    ) {
        debug_assert!(
            !self.threads_admitted(),
            "the mode is fixed before admission"
        );
        self.obarray.lease.0.threading.set(mode);
    }

    /// Whether thread admission has run in this World.
    #[inline]
    pub(crate) fn threads_admitted(&self) -> bool {
        self.tagged_heap.threads_admitted()
    }

    /// Make room for admission to project OWNED variables onto descriptors
    /// of their own, plus the VOID one: arena pages and the issued list. A
    /// refusal changes nothing a reader sees.
    pub(super) fn reserve_thread_projections(&mut self, owned: usize) -> Result<(), ()> {
        self.obarray
            .lease
            .0
            .thread_projections
            .borrow_mut()
            .try_reserve(owned)
            .map_err(|_| ())
    }

    /// The index the ORDINAL-th next projection this World issues will get.
    pub(super) fn upcoming_thread_projection(
        &self,
        ordinal: usize,
    ) -> crate::emacs_core::forward::TlsIndex {
        self.obarray
            .lease
            .0
            .thread_projections
            .borrow()
            .upcoming_index(ordinal)
    }

    /// Admission's projection of CELLS, each as its plan says, in one
    /// canonical-changing phase. Allocates nothing once
    /// [`Self::reserve_thread_projections`] covered them.
    pub(super) fn project_admitted_cells(
        &mut self,
        cells: &[(SymId, crate::emacs_core::symbol::AdmissionPlanCell)],
    ) {
        let world = Arc::clone(&self.obarray.lease.0);
        let mut table = world.thread_projections.borrow_mut();
        self.obarray.with_dispatch_write(|obarray| {
            for &(id, plan) in cells {
                obarray.project_admitted_cell(id, plan, &mut table);
            }
        });
    }

    /// Enter this World's projection scope, so its obarray births symbols
    /// on VOID. Allocates nothing once
    /// `reserve_projection_scope` and the table's reservation ran.
    pub(super) fn enter_world_projection_scope(&self) {
        let world = &self.obarray.lease.0;
        let void = world.thread_projections.borrow_mut().void_descriptor();
        // SAFETY: the table is this World's, which owns the obarray of this
        // generation and leaves the scope when it drops (`World::drop`).
        unsafe {
            crate::emacs_core::symbol::enter_projection_scope(
                self.obarray.generation(),
                &world.thread_projections,
                void,
            );
        }
    }
}

/// Thread-projection hooks for tests (Phase 0 step 2): no production path
/// projects a variable yet.
#[cfg(test)]
impl Context {
    /// Project `id`'s variable through this World's table
    /// (`Obarray::project_thread_variable_for_test`, a redirect change and
    /// so a canonical-changing phase).
    pub(crate) fn project_thread_variable_for_test(
        &mut self,
        id: SymId,
    ) -> Option<&'static crate::emacs_core::forward::ThreadProjectionDescriptor> {
        // `buffer-undo-list` keeps its storage in the buffer's shared undo
        // state, not a cell; its bindings swap with their owner instead
        // (`owner_mirrors`).
        if id == super::buffer_undo_list_symbol() {
            return None;
        }
        // A Context that can see a projected cell holds an owner slab: the
        // owner-aware variable API's dormant answer relies on it.
        self.ensure_owner_slab();
        let world = Arc::clone(&self.obarray.lease.0);
        let mut table = world.thread_projections.borrow_mut();
        self.obarray
            .with_dispatch_write(|obarray| obarray.project_thread_variable_for_test(id, &mut table))
    }

    /// The projection CELL's header heads, resolved by the World's table.
    pub(crate) fn thread_projection_for_test(
        &self,
        id: SymId,
    ) -> Option<&'static crate::emacs_core::forward::ThreadProjectionDescriptor> {
        let fwd = self.obarray.get_by_id(id)?.forwarded_descriptor()?;
        self.obarray
            .lease
            .0
            .thread_projections
            .borrow()
            .projection(fwd)
    }

    /// How many projections this World's table has issued.
    pub(crate) fn thread_projection_count_for_test(&self) -> usize {
        self.obarray.lease.0.thread_projections.borrow().len()
    }
}

impl Context {
    #[cold]
    pub(super) fn shutdown_world(mut self: Box<Self>) -> Result<(), ContextShutdownError> {
        self.finish_concurrent_mark()?;
        let closing = self.obarray.closing_lease();
        // Context Drop retracts owner TLS/registries before its views retire.
        drop(self);
        closing.into_exclusive()?.finish()
    }
}

#[cfg(test)]
#[path = "tests/world_storage_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/sole_owner_phase_test.rs"]
mod sole_owner_tests;
