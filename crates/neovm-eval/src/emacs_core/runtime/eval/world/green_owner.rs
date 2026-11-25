//! Green owner construction (Phase 0 step 2, round 3d): further Contexts
//! over one World, one per Lisp thread, each on its own OS thread.
//!
//! # Safety argument
//!
//! The sole-owner policy (`sole_owner`) proves World views safe for ONE
//! Context: each manager cell has exactly one mutable view, owned by that
//! Context. A Green World admits further owners, each with its own full set
//! of views over the same cells, so that proof no longer holds by itself.
//! It is replaced by the [`Baton`]:
//!
//! 1. A Green World has exactly one Baton, issued once
//!    ([`Context::issue_green_baton`]). It is `Send` and neither `Sync` nor
//!    `Clone`, so at any time exactly one OS thread holds it.
//! 2. Only the holder runs Lisp or touches any view of the World: an owner
//!    builds its views only from the Baton it holds
//!    ([`Baton::owner_views`]), and it runs only between receiving the Baton
//!    and handing it on. Every other owner is parked, blocked in its
//!    receive, holding views it does not dereference.
//! 3. Handing the Baton on is a channel send, received by the next holder:
//!    the send/receive pair orders every write the previous holder made
//!    through its views before every access the next holder makes. That is
//!    the only cross-thread edge any World cell ever crosses.
//! 4. The World's `Cell`s and `RefCell`s, its heap and its obarray are thus
//!    accessed by one thread at a time, in a total order, which is what
//!    their `!Sync` contracts require. World and Context stay `!Send`; only
//!    the Baton carries the World's allocation across threads.
//! 5. The Baton also retains the heap's facade epoch, so no concurrent
//!    marker runs while Green owners exist: every collection is synchronous,
//!    on the holder.
//!
//! Debug builds check point 2 at every view creation: the World records its
//! holder's OS thread ([`Baton::enter`]), and [`Baton::owner_views`] panics
//! off that thread.

use super::*;
#[cfg(test)]
use crate::emacs_core::eval::world::WorldProofEraExt;

#[path = "parallel_roots.rs"]
mod parallel_roots;
#[cfg(any(test, feature = "test-support"))]
pub(crate) use parallel_roots::OwnedProofProjection;
pub(crate) use parallel_roots::{FrozenLegacyRoots, OwnedParallelError, OwnedParallelProjection};

impl World {
    /// Finish publications before automatic field destruction reaches the heap.
    /// World exclusivity proves that every former owner has terminated.
    #[cold]
    #[inline(never)]
    pub(crate) fn finish_green_registry(&mut self) {
        let Some(runtime) = self.green.get_mut().as_mut() else {
            return;
        };
        if !runtime.parked.get_mut().is_empty() {
            let heap = self.tagged_heap.get_mut();
            // SAFETY: the final World owns every view exclusively; no owner,
            // callback or collection extent remains. Its last publications
            // still exclude concurrent launch. With no Context left, the
            // transport table contains all remaining external heap roots.
            let collector = unsafe {
                heap.drain_to_quiescent(|heap| {
                    let mut roots = Vec::new();
                    neovm_value::tagged::transport::collect_shared_root_gc_roots(heap, &mut roots);
                    for root in roots {
                        heap.seed_root(root);
                    }
                })
            }
            .expect("the terminated Green World's collector drains completely");
            runtime.finish_terminated_roots(&collector);
        }
        drop(self.green.get_mut().take());
        debug_assert!(
            !self.tagged_heap.get_mut().facade_mark_is_excluded(),
            "Green publications, reservations and Baton epochs finish before the heap"
        );
    }
}

/// The right to run Lisp in a Green World: whoever holds it is the one OS
/// thread that may touch the World. See the module's safety argument.
#[must_use = "the World's Lisp stops while its Baton is dropped"]
pub struct Baton {
    /// Keeps concurrent marking excluded for the Green era (point 5).
    /// Release before `world`: this Baton can own the final World reference.
    _retention: neovm_value::tagged::gc::FacadeEpochRetention,
    world: Arc<World>,
}

// SAFETY: the Baton is the World's single, unshareable token (point 1); the
// World behind it is touched only by the thread holding it, and the channel
// that moves it orders those accesses (points 2-4).
unsafe impl Send for Baton {}
static_assertions::assert_not_impl_any!(Baton: Sync, Clone, Copy);

impl fmt::Debug for Baton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Baton")
            .field("holder", &self.world.baton_holder.get())
            .finish_non_exhaustive()
    }
}

impl Baton {
    /// Cold proof-3 backing admission. Original allocation roots only; no
    /// Context view, heap reference or ordinary mutable manager borrow.
    pub(crate) fn parallel_symbol_backing(&self) -> neovm_value::symbol::parallel::SymbolBacking {
        assert!(self.held_here(), "symbol backing keeper must be held here");
        // SAFETY: era admission has escrowed the serial Context and retains
        // this Baton for every owner loan, including abandoned-era retention.
        unsafe {
            neovm_value::symbol::parallel::SymbolBacking::new(
                std::ptr::NonNull::new(self.world.obarray.get()).expect("World obarray root"),
                std::ptr::NonNull::from(&self.world.thread_projections),
                Arc::as_ptr(&self.world) as usize,
            )
        }
    }
    /// The calling OS thread has received the Baton: it is the holder.
    #[doc(hidden)]
    pub fn enter(&self) {
        self.world
            .baton_holder
            .set(Some(std::thread::current().id()));
    }

    /// Whether the calling OS thread holds the Baton.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn held_here(&self) -> bool {
        self.world.baton_holder.get() == Some(std::thread::current().id())
    }

    /// Identify the retained heap without borrowing its storage. Admission
    /// separately checks `held_here`; metadata alone grants no World access.
    #[cold]
    #[inline(never)]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn parallel_heap_identity(&self) -> neovm_value::tagged::gc::HeapIdentity {
        self._retention.heap_identity()
    }

    /// Cold metadata retention for parallel root publications, with no heap loan.
    #[cold]
    #[inline(never)]
    pub(crate) fn parallel_epoch_retention(
        &self,
    ) -> Result<
        neovm_value::tagged::gc::FacadeEpochRetention,
        neovm_value::tagged::gc::FacadeMarkExclusionError,
    > {
        self._retention.retain()
    }

    /// Derive the inert World allocation handle used inside an open parallel
    /// era to pair with a keeper-lent [`WorldTurn`]. Grants no World access on
    /// its own; see [`EraWorldHandle`].
    #[cold]
    #[inline(never)]
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "L6.7b b4.3 Slice 2 era admission is the caller")
    )]
    pub(crate) fn era_world_handle(
        &self,
    ) -> Result<EraWorldHandle, neovm_value::tagged::gc::FacadeMarkExclusionError> {
        assert!(self.held_here(), "era World handle derived off the Baton");
        Ok(EraWorldHandle {
            _retention: self._retention.retain()?,
            world: Arc::clone(&self.world),
        })
    }

    /// The retained World identity as a copied scalar. Never clones the Arc;
    /// holding the token neither retains nor frees the World.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn world_identity_ptr(&self) -> usize {
        Arc::as_ptr(&self.world) as usize
    }

    /// Test-only adoption of a serially built, retained World. The symbol
    /// fixture builds the World and keeps the sole retention itself; this
    /// constructor never escapes its defining module tree.
    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn from_test_world_parts(
        retention: neovm_value::tagged::gc::FacadeEpochRetention,
        world: Arc<World>,
    ) -> Self {
        Self {
            _retention: retention,
            world,
        }
    }

    /// A further owner's views of the World, starting in CURRENT. Only the
    /// holder may build them (point 2, checked in debug builds).
    pub(crate) fn owner_views(&self, current: Option<BufferId>) -> ReadyWorld {
        debug_assert!(
            self.held_here(),
            "a World view made off the thread holding the World's Baton"
        );
        let lease = WorldLease(Arc::clone(&self.world));
        WorldStorageBuilder::publish_views(&lease, BufferSelection::for_owner(current))
    }

    /// Reserve a parked root publication under this World's held exclusion.
    pub(crate) fn prepare_parked_batch(
        &self,
        heap: &neovm_value::tagged::gc::TaggedHeap,
        capacity: usize,
    ) -> Result<
        neovm_value::tagged::transport::PreparedRootBatch,
        neovm_value::tagged::transport::RootBatchError,
    > {
        debug_assert!(self.held_here(), "root publication prepared off the Baton");
        neovm_value::tagged::transport::PreparedRootBatch::reserve_retained(
            heap,
            self._retention.retain()?,
            capacity,
        )
    }

    /// Retire an exiting worker's final native root after its Context and
    /// carrier TLS have gone. This is intentionally not a payload accessor.
    pub(crate) fn retire_thread_completion(
        &mut self,
        completion: crate::emacs_core::threads::CompletingThread,
    ) -> crate::emacs_core::threads::RetiredCompletion {
        retire_thread_completion_with(&self.world, &self._retention, completion)
    }

    /// The World's Green runtime.
    pub(crate) fn green_runtime(
        &self,
    ) -> std::cell::Ref<'_, Option<Box<super::super::green::GreenRuntime>>> {
        debug_assert!(self.held_here(), "the Green runtime read off the Baton");
        self.world.green.borrow()
    }

    #[cfg(test)]
    pub(crate) fn install_stowed_green_runtime_for_test(
        &self,
        captured: super::super::green::TestGreenBootstrap,
    ) {
        assert!(self.held_here(), "install GreenRuntime on keeper thread");
        let mut slot = self.world.green.borrow_mut();
        assert!(slot.is_none(), "World must not already have a GreenRuntime");
        *slot = Some(super::super::green::GreenRuntime::new_stowed_for_test(
            self.parallel_heap_identity(),
            captured,
        ));
    }

    #[cfg(test)]
    pub(crate) fn uninstall_green_runtime_for_test(&self) {
        assert!(self.held_here(), "uninstall GreenRuntime on keeper thread");
        let runtime = self.world.green.borrow_mut().take();
        if let Some(runtime) = runtime {
            assert!(
                runtime.parked_is_empty_for_test(),
                "all parked root leases must retire before uninstalling GreenRuntime"
            );
            drop(runtime);
        }
    }

    #[cfg(test)]
    pub(crate) fn test_world_handle_for_test(&self) -> TestBatonWorldHandle {
        assert!(self.held_here(), "mint world handle on keeper thread");
        TestBatonWorldHandle {
            world: Arc::clone(&self.world),
            retention: self
                .parallel_epoch_retention()
                .expect("keeper Baton retains facade epoch"),
        }
    }
}

/// Test-only `Send` handle allowing a spawned Baton thread inside a live
/// `KeeperEra` fixture to build a seated `Context` over the era's `World`
/// (serialized against the keeper's turns by the test).
#[cfg(test)]
pub(crate) struct TestBatonWorldHandle {
    world: Arc<World>,
    retention: neovm_value::tagged::gc::FacadeEpochRetention,
}

// SAFETY: test-only fixture handle; callers serialize `spawn_seated_context_for_test`
// while no other thread accesses the World.
#[cfg(test)]
unsafe impl Send for TestBatonWorldHandle {}

#[cfg(test)]
impl TestBatonWorldHandle {
    pub(crate) fn green_runtime_for_test(
        &self,
    ) -> std::cell::Ref<'_, Option<Box<super::super::green::GreenRuntime>>> {
        self.world.green.borrow()
    }

    pub(crate) fn owner_views_for_test(&self, current: Option<BufferId>) -> ReadyWorld {
        let lease = WorldLease(Arc::clone(&self.world));
        WorldStorageBuilder::publish_views(&lease, BufferSelection::for_owner(current))
    }

    pub(crate) fn spawn_seated_context_for_test(
        &self,
        worker_id: u64,
    ) -> (Context, neovm_value::tagged::gc::FacadeEpochRetention) {
        let ctx = Context::new_seated_era_worker_for_test(self, worker_id);
        let retention = self
            .retention
            .retain()
            .expect("retain facade epoch for seated world handle");
        (ctx, retention)
    }

    pub(crate) fn era_world_handle_for_test(&self) -> EraWorldHandle {
        EraWorldHandle {
            _retention: self
                .retention
                .retain()
                .expect("retain facade epoch for EraWorldHandle"),
            world: Arc::clone(&self.world),
        }
    }

    pub(crate) fn green_control_for_test(&self) -> std::sync::Arc<super::super::green::Control> {
        let runtime = self.world.green.borrow();
        let runtime = runtime
            .as_ref()
            .expect("era test installs GreenRuntime before reading Control");
        std::sync::Arc::clone(&runtime.control)
    }

    pub(crate) fn spawn_unseated_serial_context_for_test(&self) -> Context {
        Context::new_unseated_serial_owner_for_test(self)
    }
}

fn retire_thread_completion_with(
    world: &Arc<World>,
    retention: &neovm_value::tagged::gc::FacadeEpochRetention,
    completion: crate::emacs_core::threads::CompletingThread,
) -> crate::emacs_core::threads::RetiredCompletion {
    let held_here = world.baton_holder.get() == Some(std::thread::current().id());
    if !held_here || completion.heap_identity() != retention.heap_identity() {
        // No installed heap remains here, so a mismatched authority is a
        // fail-stop rather than a Lisp error or an attempted TLS restore.
        std::process::abort();
    }
    // SAFETY: the caller holds the unique World authority (either the World's
    // Baton or the keeper's active WorldTurn) on this OS thread, excluding
    // every other World access. The Context was retired, no manager borrow
    // survives, and removal touches only native registry storage: no heap word
    // is decoded, collected, mutated or accessed through TLS. The caller
    // retains the World allocation until after Control::finish transfers
    // ownership.
    unsafe { (&mut (*world.global.get()).threads).retire_thread_completion(completion) }
}

/// Inert World allocation handle retained for an open parallel era.
///
/// Holding an `EraWorldHandle` grants NO access to the World's cells, heap,
/// obarray, or `GreenRuntime`. World access is only unlocked when paired with a
/// keeper-lent [`WorldTurn`](super::super::parallel_entry::world_turn::WorldTurn)
/// of matching heap identity inside [`EraTurnRight`].
#[must_use = "the era World handle retains the World's facade epoch until dropped"]
pub(crate) struct EraWorldHandle {
    /// Keeps concurrent marking excluded while retained.
    /// Release before `world`: matches [`Baton`]'s drop order.
    _retention: neovm_value::tagged::gc::FacadeEpochRetention,
    world: Arc<World>,
}

// SAFETY: `EraWorldHandle` exposes no read or write access to `World` on its
// own; it only carries the allocation pointer and retained facade epoch across
// threads so an active `WorldTurn` (unique, `Send`, cut under the keeper's
// stop) can unlock World access on whichever Baton OS thread holds that turn.
unsafe impl Send for EraWorldHandle {}
static_assertions::assert_not_impl_any!(EraWorldHandle: Sync, Clone, Copy);

impl fmt::Debug for EraWorldHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EraWorldHandle")
            .field("heap", &self._retention.heap_identity())
            .finish_non_exhaustive()
    }
}

impl EraWorldHandle {
    pub(crate) fn heap_identity(&self) -> neovm_value::tagged::gc::HeapIdentity {
        self._retention.heap_identity()
    }

    pub(crate) fn try_clone(
        &self,
    ) -> Result<Self, neovm_value::tagged::gc::FacadeMarkExclusionError> {
        Ok(Self {
            _retention: self._retention.retain()?,
            world: Arc::clone(&self.world),
        })
    }

    pub(crate) fn parallel_epoch_retention(
        &self,
    ) -> Result<
        neovm_value::tagged::gc::FacadeEpochRetention,
        neovm_value::tagged::gc::FacadeMarkExclusionError,
    > {
        self._retention.retain()
    }

    /// Check whether this World's Green carriers are `CarriedHolder::Stowed`
    /// while holding a keeper-lent `WorldTurn` of matching heap identity.
    pub(crate) fn serial_world_is_stowed_under_turn(
        &self,
        turn: &super::super::parallel_entry::world_turn::WorldTurn,
    ) -> bool {
        if turn.identity().heap != self.heap_identity() {
            return false;
        }
        self.world.green.borrow().as_ref().is_some_and(|runtime| {
            runtime.carried_holder.get() == super::super::green::CarriedHolder::Stowed
        })
    }
}

/// RAII guard restoring `World::baton_holder` to the era's escrowing holder
/// (keeper OS thread) before `WorldLend` returns or abandons the turn (HIGH-1:
/// covers both normal return and K7 unwind/drop).
///
/// Invariant (by induction over lends): at every lend, `World::baton_holder`
/// holds `Some(keeper_tid)` for the era's escrowing holder (the thread that
/// will check `Baton::held_here()` at era close or teardown). Era open starts
/// with the escrowed `Baton` naming that holder; every subsequent lend follows
/// either `open_era` or a `give_back`/restore from `KeeperHolderRestore`; and
/// the only writer in between is `TurnRight::enter` on the unique active turn.
///
/// Declared strictly BEFORE `lend` inside [`EraTurnRight`] so field drop order
/// restores `baton_holder` strictly before `WorldLend::drop` sends
/// `KeeperMsg::GroupAbandon` (channel send synchronizes-with keeper recv).
struct KeeperHolderRestore {
    world: Arc<World>,
    keeper_holder: Option<std::thread::ThreadId>,
    armed: bool,
}

// SAFETY: `KeeperHolderRestore` lives solely inside the unique `EraTurnRight`
// that owns the keeper's active `WorldTurn`; it writes `World::baton_holder`
// only while this thread holds exclusive turn authority right before the turn
// leaves back to the keeper.
unsafe impl Send for KeeperHolderRestore {}

impl fmt::Debug for KeeperHolderRestore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeeperHolderRestore")
            .field("keeper_holder", &self.keeper_holder)
            .field("armed", &self.armed)
            .finish_non_exhaustive()
    }
}

impl KeeperHolderRestore {
    fn restore_now(mut self) {
        if self.armed {
            self.world.baton_holder.set(self.keeper_holder);
            self.armed = false;
        }
    }
}

impl Drop for KeeperHolderRestore {
    fn drop(&mut self) {
        if self.armed {
            self.world.baton_holder.set(self.keeper_holder);
            self.armed = false;
        }
    }
}

/// The Baton group's active era turn right (L6.7b r1 §2): pairs the keeper's
/// [`WorldTurn`](super::super::parallel_entry::world_turn::WorldTurn) and armed
/// K7 [`WorldLend`](super::super::parallel_entry::keeper_driver::WorldLend)
/// guard with the checked [`EraWorldHandle`] for the same World heap.
#[derive(Debug)]
#[must_use = "an era turn right holds the keeper's WorldTurn and armed WorldLend guard"]
pub(crate) struct EraTurnRight {
    turn: super::super::parallel_entry::world_turn::WorldTurn,
    /// Declared strictly BEFORE `lend` so unwinding/dropping `EraTurnRight`
    /// restores `World::baton_holder` before `WorldLend::drop` sends
    /// `KeeperMsg::GroupAbandon` (HIGH-1).
    restore_holder: KeeperHolderRestore,
    lend: super::super::parallel_entry::keeper_driver::WorldLend,
    world: EraWorldHandle,
    stowed: Option<super::super::parallel_entry::world_seat::StowedForReturn>,
}

static_assertions::assert_impl_all!(EraTurnRight: Send, fmt::Debug);
static_assertions::assert_not_impl_any!(EraTurnRight: Sync, Clone, Copy);

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "L6.7b b4.3 Slice 2 era interval wiring is the caller"
    )
)]
impl EraTurnRight {
    /// Pair a keeper-lent `WorldTurn` and its `WorldLend` guard with an
    /// `EraWorldHandle` for the same heap (Condition A-3). Returns the inputs
    /// unchanged if the heap identities do not match.
    pub(crate) fn from_parts(
        turn: super::super::parallel_entry::world_turn::WorldTurn,
        lend: super::super::parallel_entry::keeper_driver::WorldLend,
        world: EraWorldHandle,
    ) -> Result<
        Self,
        (
            super::super::parallel_entry::world_turn::WorldTurn,
            super::super::parallel_entry::keeper_driver::WorldLend,
            EraWorldHandle,
        ),
    > {
        if turn.identity().heap != world.heap_identity() {
            return Err((turn, lend, world));
        }
        let keeper_holder = world.world.baton_holder.get();
        debug_assert!(
            keeper_holder.is_some(),
            "R1: at every era lend, World::baton_holder must name the era's escrowing holder"
        );
        let restore_holder = KeeperHolderRestore {
            world: Arc::clone(&world.world),
            keeper_holder,
            armed: true,
        };
        Ok(Self {
            turn,
            restore_holder,
            lend,
            world,
            stowed: None,
        })
    }

    pub(crate) fn turn_identity(
        &self,
    ) -> super::super::parallel_entry::world_turn::WorldTurnIdentity {
        self.turn.identity()
    }

    pub(crate) fn set_stowed(
        &mut self,
        proof: super::super::parallel_entry::world_seat::StowedForReturn,
    ) {
        self.stowed = Some(proof);
    }

    pub(crate) fn clear_stowed(&mut self) {
        self.stowed = None;
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        super::super::parallel_entry::world_turn::WorldTurn,
        super::super::parallel_entry::keeper_driver::WorldLend,
        EraWorldHandle,
    ) {
        self.restore_holder.restore_now();
        (self.turn, self.lend, self.world)
    }

    pub(crate) fn into_parts_with_stowed(
        self,
    ) -> (
        super::super::parallel_entry::world_turn::WorldTurn,
        super::super::parallel_entry::keeper_driver::WorldLend,
        EraWorldHandle,
        Option<super::super::parallel_entry::world_seat::StowedForReturn>,
    ) {
        self.restore_holder.restore_now();
        (self.turn, self.lend, self.world, self.stowed)
    }
}

/// The World turn authority carried by Green threads and scheduler `Wake`
/// messages (L6.7b r1 §2, QD1).
///
/// - Outside a parallel era, every value is [`TurnRight::Solo`]: inline, zero
///   allocation, identical to pre-L6.7b Baton ownership.
/// - Inside an open parallel era, the keeper escrows the `Baton` and every
///   Green turn and wake carries [`TurnRight::Era`], boxed so the `Solo` hot
///   path stays compact.
#[derive(Debug)]
#[must_use = "dropping a TurnRight strands the World or abandons the keeper's WorldTurn"]
pub(crate) enum TurnRight {
    Solo(Baton),
    Era(Box<EraTurnRight>),
}

static_assertions::assert_impl_all!(TurnRight: Send, fmt::Debug);
static_assertions::assert_not_impl_any!(TurnRight: Sync, Clone, Copy);

impl From<Baton> for TurnRight {
    fn from(baton: Baton) -> Self {
        Self::Solo(baton)
    }
}

impl TurnRight {
    #[inline]
    fn world_and_retention(&self) -> (&Arc<World>, &neovm_value::tagged::gc::FacadeEpochRetention) {
        match self {
            Self::Solo(baton) => (&baton.world, &baton._retention),
            Self::Era(era) => (&era.world.world, &era.world._retention),
        }
    }

    #[inline]
    pub(crate) fn is_solo(&self) -> bool {
        matches!(self, Self::Solo(_))
    }

    #[inline]
    pub(crate) fn is_era(&self) -> bool {
        matches!(self, Self::Era(_))
    }

    #[cfg(test)]
    pub(crate) fn era_turn_epoch_for_test(&self) -> Option<std::num::NonZeroU64> {
        match self {
            Self::Solo(_) => None,
            Self::Era(era) => Some(era.turn_identity().epoch),
        }
    }

    /// Cold metadata retention for parallel root publications, with no heap loan.
    #[cold]
    #[inline(never)]
    pub(crate) fn parallel_epoch_retention(
        &self,
    ) -> Result<
        neovm_value::tagged::gc::FacadeEpochRetention,
        neovm_value::tagged::gc::FacadeMarkExclusionError,
    > {
        self.world_and_retention().1.retain()
    }

    pub(crate) fn serial_world_is_stowed(&self) -> bool {
        self.world_and_retention()
            .0
            .green
            .borrow()
            .as_ref()
            .is_some_and(|runtime| {
                runtime.carried_holder.get() == super::super::green::CarriedHolder::Stowed
            })
    }

    pub(crate) fn set_stowed_for_return(
        &mut self,
        proof: super::super::parallel_entry::world_seat::StowedForReturn,
    ) {
        if let Self::Era(era) = self {
            era.set_stowed(proof);
        }
    }

    pub(crate) fn clear_stowed_for_return(&mut self) {
        if let Self::Era(era) = self {
            era.clear_stowed();
        }
    }

    pub(crate) fn from_era(
        turn: super::super::parallel_entry::world_turn::WorldTurn,
        lend: super::super::parallel_entry::keeper_driver::WorldLend,
        world: EraWorldHandle,
    ) -> Result<
        Self,
        (
            super::super::parallel_entry::world_turn::WorldTurn,
            super::super::parallel_entry::keeper_driver::WorldLend,
            EraWorldHandle,
        ),
    > {
        EraTurnRight::from_parts(turn, lend, world).map(|era| Self::Era(Box::new(era)))
    }

    pub(crate) fn into_solo(self) -> Result<Baton, Self> {
        match self {
            Self::Solo(baton) => Ok(baton),
            other @ Self::Era(_) => Err(other),
        }
    }

    pub(crate) fn into_era(self) -> Result<Box<EraTurnRight>, Self> {
        match self {
            Self::Era(era) => Ok(era),
            other @ Self::Solo(_) => Err(other),
        }
    }

    /// The calling OS thread has received this turn right: it is the holder.
    #[inline]
    pub(crate) fn enter(&self) {
        let (world, _) = self.world_and_retention();
        world.baton_holder.set(Some(std::thread::current().id()));
    }

    /// Whether the calling OS thread currently holds this World's turn right.
    #[inline]
    pub(crate) fn held_here(&self) -> bool {
        let (world, _) = self.world_and_retention();
        world.baton_holder.get() == Some(std::thread::current().id())
    }

    /// Identify the retained heap without borrowing its storage.
    #[cold]
    #[inline(never)]
    pub(crate) fn parallel_heap_identity(&self) -> neovm_value::tagged::gc::HeapIdentity {
        let (_, retention) = self.world_and_retention();
        retention.heap_identity()
    }

    /// A further owner's views of the World, starting in CURRENT.
    pub(crate) fn owner_views(&self, current: Option<BufferId>) -> ReadyWorld {
        debug_assert!(
            self.held_here(),
            "a World view made off the thread holding the World's turn right"
        );
        let (world, _) = self.world_and_retention();
        let lease = WorldLease(Arc::clone(world));
        WorldStorageBuilder::publish_views(&lease, BufferSelection::for_owner(current))
    }

    /// Reserve a parked root publication under this World's held exclusion.
    pub(crate) fn prepare_parked_batch(
        &self,
        heap: &neovm_value::tagged::gc::TaggedHeap,
        capacity: usize,
    ) -> Result<
        neovm_value::tagged::transport::PreparedRootBatch,
        neovm_value::tagged::transport::RootBatchError,
    > {
        debug_assert!(
            self.held_here(),
            "root publication prepared off the turn right"
        );
        let (_, retention) = self.world_and_retention();
        neovm_value::tagged::transport::PreparedRootBatch::reserve_retained(
            heap,
            retention.retain()?,
            capacity,
        )
    }

    /// Retire an exiting worker's final native root after its Context and
    /// carrier TLS have gone.
    pub(crate) fn retire_thread_completion(
        &mut self,
        completion: crate::emacs_core::threads::CompletingThread,
    ) -> crate::emacs_core::threads::RetiredCompletion {
        let (world, retention) = self.world_and_retention();
        retire_thread_completion_with(world, retention, completion)
    }

    /// The World's Green runtime.
    pub(crate) fn green_runtime(
        &self,
    ) -> std::cell::Ref<'_, Option<Box<super::super::green::GreenRuntime>>> {
        debug_assert!(
            self.held_here(),
            "the Green runtime read off the turn right"
        );
        let (world, _) = self.world_and_retention();
        world.green.borrow()
    }
}

impl Context {
    /// Build registration fixtures with a real World and its unique keeper.
    /// The higher-ranked closure cannot return an era or collector heap loan.
    #[cfg(test)]
    pub(crate) fn with_test_mutator_era<R>(
        f: impl for<'heap> FnOnce(crate::emacs_core::eval::world::WorldProofEra<'heap>) -> R,
    ) -> R {
        let mut context = Self::new(crate::test_utils::unit_runtime());
        let keeper = context
            .issue_green_baton()
            .expect("fresh fixture World issues its keeper");
        let heap: *mut neovm_value::tagged::gc::TaggedHeap =
            context.tagged_heap.heap_root().as_ptr();
        // SAFETY: this fresh Context is the sole running owner and holds the
        // keeper. No manager loan, callback, safepoint or collection extent
        // survives keeper issuance. Reseeding only enumerates the complete
        // registered-root census; it cannot execute Lisp. The closure below
        // cannot outlive the Context, and admission escrows the actual keeper.
        let collector = unsafe {
            (*heap).drain_to_quiescent(|heap| {
                context.seed_registered_mutator_roots_world_stopped(heap);
            })
        }
        .expect("fresh fixture collector drains completely");
        // Compare only the root address while the exclusive collector loan
        // is retained. Synthetic participants cannot use the serial TLS view.
        neovm_value::tagged::gc::clear_tagged_heap_root_if_installed(
            context.tagged_heap.heap_root(),
        );
        let era = crate::emacs_core::eval::world::WorldProofEra::admit(collector, keeper)
            .unwrap_or_else(|_| panic!("fresh fixture World admits its mutator era"));
        f(era)
    }

    /// Real heap roots prepared by its sole serial owner before the synthetic
    /// parallel era. Participants receive SharedRoot handles, never raw Values.
    #[cfg(test)]
    pub(crate) fn with_test_rooted_mutator_era<R>(
        f: impl for<'heap> FnOnce(
            crate::emacs_core::eval::world::WorldProofEra<'heap>,
            Vec<neovm_value::tagged::transport::SharedRoot>,
        ) -> R,
    ) -> R {
        let mut context = Self::new(crate::test_utils::unit_runtime());
        let mut roots = Vec::new();
        for n in 0..4 {
            let value = context.tagged_heap.alloc_cons(
                neovm_value::tagged::value::TaggedValue::fixnum(n),
                neovm_value::tagged::value::TaggedValue::NIL,
            );
            // SAFETY: sole serial owner; allocation invokes no collection or
            // callback and the source word stays live through root admission.
            roots.push(unsafe {
                neovm_value::tagged::transport::SharedRoot::new(&context.tagged_heap, value)
            });
        }
        let keeper = context.issue_green_baton().expect("rooted fixture keeper");
        let heap: *mut neovm_value::tagged::gc::TaggedHeap =
            context.tagged_heap.heap_root().as_ptr();
        // SAFETY: same complete frozen-World census as with_test_mutator_era.
        // SharedRoot table entries keep every synthetic input live as well.
        let collector = unsafe {
            (*heap).drain_to_quiescent(|heap| {
                context.seed_registered_mutator_roots_world_stopped(heap);
            })
        }
        .expect("rooted fixture collector");
        // Compare only the root address while the exclusive collector loan
        // is retained. Synthetic participants cannot use the serial TLS view.
        neovm_value::tagged::gc::clear_tagged_heap_root_if_installed(
            context.tagged_heap.heap_root(),
        );
        let era = crate::emacs_core::eval::world::WorldProofEra::admit(collector, keeper)
            .unwrap_or_else(|_| panic!("rooted fixture era"));
        f(era, roots)
    }

    /// Issue this Green World's one Baton to the calling thread, the
    /// owner's: the collector is drained and its facade epoch retained for
    /// the Green era, so no concurrent marker runs while Green owners exist.
    #[cold]
    #[inline(never)]
    pub(crate) fn issue_green_baton(
        &mut self,
    ) -> Result<Baton, super::super::green::GreenStartError> {
        let heap: *mut neovm_value::tagged::gc::TaggedHeap = self.tagged_heap.heap_root().as_ptr();
        // SAFETY: the sole running owner, at a safe point, with no manager
        // loan or callback live; the reseed enumerates the registered root
        // census and only adds mark roots (as admission's collector-closed
        // phase does).
        let exclusion = unsafe {
            (*heap)
                .drain_to_quiescent(|heap| {
                    self.seed_registered_mutator_roots_world_stopped(heap);
                })?
                .exclude_concurrent_mark()?
        };
        let retention = exclusion.retain()?;
        drop(exclusion);
        let world = Arc::clone(&self.obarray.lease.0);
        world.baton_holder.set(Some(std::thread::current().id()));
        Ok(Baton {
            world,
            _retention: retention,
        })
    }

    /// This World's Green runtime, if it started.
    pub(crate) fn green_runtime_ref(
        &self,
    ) -> std::cell::Ref<'_, Option<Box<super::super::green::GreenRuntime>>> {
        self.obarray.lease.0.green.borrow()
    }

    /// Install the World's Green runtime (once).
    pub(crate) fn install_green_runtime(&self, runtime: Box<super::super::green::GreenRuntime>) {
        let mut slot = self.obarray.lease.0.green.borrow_mut();
        debug_assert!(slot.is_none(), "a World starts its Green runtime once");
        *slot = Some(runtime);
    }
}
