//! Root-owning heap fixture for tests that alternate direct and TLS access.
//!
//! The owner, every view, and TLS all derive from AliasableBox's single root.
//! A helper that crosses TLS/barriers must borrow TestHeap, not TaggedHeap:
//! keeping a payload loan alive across a root-derived sibling is still invalid.

use super::*;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

/// A consistent no-worker SATB scope for native/Lisp store probes. Automatic
/// collection must remain inhibited; the probe must not force an explicit GC.
/// The Context loan and thread marker keep restoration on the installing owner.
pub(crate) struct SyntheticConcurrentBarrier<'a> {
    context: &'a mut crate::emacs_core::eval::Context,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

static_assertions::assert_not_impl_any!(SyntheticConcurrentBarrier<'static>: Send, Sync);

impl<'a> SyntheticConcurrentBarrier<'a> {
    pub(crate) fn enter(context: &'a mut crate::emacs_core::eval::Context) -> Self {
        assert!(
            context.gc_inhibit_depth > 0,
            "synthetic barriers require GC inhibition"
        );
        assert_eq!(context.tagged_heap.mark_phase(), MarkPhase::Idle);
        let guard = Self {
            context,
            _thread: std::marker::PhantomData,
        };
        guard.context.tagged_heap.mark_in_progress = true;
        guard
            .context
            .tagged_heap
            .set_concurrent_active_for_test(true);
        guard
    }

    pub(crate) fn context(&mut self) -> &mut crate::emacs_core::eval::Context {
        self.context
    }
}

impl Drop for SyntheticConcurrentBarrier<'_> {
    fn drop(&mut self) {
        self.context
            .tagged_heap
            .set_concurrent_active_for_test(false);
        self.context.tagged_heap.mark_in_progress = false;
    }
}

pub(crate) struct TestHeap {
    owner: AliasableBox<TaggedHeap>,
}

impl TestHeap {
    pub(crate) fn new() -> Self {
        Self::from_heap(TaggedHeap::new())
    }

    /// Finish address-independent setup before installing a custom heap.
    pub(crate) fn from_heap(heap: TaggedHeap) -> Self {
        let fixture = Self {
            owner: AliasableBox::new(heap),
        };
        fixture.install();
        fixture
    }

    /// Copy the provenance root without borrowing any heap bytes.
    pub(crate) fn root(&self) -> NonNull<TaggedHeap> {
        self.owner.root()
    }

    /// Refresh TLS from the root after a phase/barrier-window change.
    pub(crate) fn install(&self) {
        set_tagged_heap_root(self.root());
    }

    pub(in crate::tagged::gc) fn with_mutator_gc_for_test<R>(
        &mut self,
        id: mutator_gc::TestMutatorGcId,
        body: impl FnOnce(NonNull<TaggedHeap>) -> R,
    ) -> Result<R, mutator_gc::TestMutatorSwitchError> {
        self.owner.with_mutator_gc_for_test(id, body)
    }
}

impl Deref for TestHeap {
    type Target = TaggedHeap;

    fn deref(&self) -> &TaggedHeap {
        self.owner.get()
    }
}

impl DerefMut for TestHeap {
    fn deref_mut(&mut self) -> &mut TaggedHeap {
        self.owner.get_mut()
    }
}

#[test]
fn heapborrow_r7_fixture_owner_and_tls_share_root() {
    let mut heap = TestHeap::new();
    let first = heap.alloc_cons(TaggedValue::fixnum(61), TaggedValue::NIL);
    let second = TaggedValue::cons(first, TaggedValue::NIL);
    heap.alloc_vector(vec![second]);
    heap.install();
    let third = TaggedValue::cons(second, TaggedValue::NIL);
    assert_eq!(
        third.cons_car().cons_car().cons_car(),
        TaggedValue::fixnum(61)
    );
    assert!(tagged_heap_is_current(&heap));
    drop(heap);
    assert!(!tagged_heap_is_installed());
}

#[test]
fn heapborrow_r7_displaced_fixture_drop_preserves_new_installation() {
    let first = TestHeap::new();
    let second = TestHeap::new();
    drop(first);
    assert!(tagged_heap_is_current(&second));
    drop(second);
    assert!(!tagged_heap_is_installed());
}

#[test]
fn heapborrow_r7_stress_fixtures_do_not_reintroduce_borrow_installers() {
    for (path, source) in [
        (
            "satb_gc_stress_test.rs",
            include_str!("satb_gc_stress_test.rs"),
        ),
        (
            "concurrent_major_worker_test.rs",
            include_str!("concurrent_major_worker_test.rs"),
        ),
        (
            "concurrent_hash_worker_tests.rs",
            include_str!("concurrent_hash_worker_tests.rs"),
        ),
        (
            "concurrent_leaf_claim_tests.rs",
            include_str!("concurrent_leaf_claim_tests.rs"),
        ),
        (
            "chartable_concurrent_write_test.rs",
            include_str!("chartable_concurrent_write_test.rs"),
        ),
    ] {
        assert!(
            !source.contains(concat!("set_tagged", "_heap(")),
            "{path} must install TLS through its TestHeap root"
        );
    }
}

/// Collection can run while Lisp code is tracking dependencies. Traversing
/// its roots must preserve their values without registering Lisp reads.
#[cfg(feature = "jit")]
#[test]
fn heapborrow_r8_collector_scans_do_not_observe_lisp_reads() {
    let mut heap = TestHeap::new();
    heap.set_generational_enabled(false);
    heap.install();
    use crate::tagged::collection_reads::{CompiledJournalMode, force_compiled_journal_for_test};
    struct RestoreMode;
    impl Drop for RestoreMode {
        fn drop(&mut self) {
            force_compiled_journal_for_test(None);
        }
    }
    let _restore = RestoreMode;
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let child = heap.alloc_cons(TaggedValue::fixnum(67), TaggedValue::NIL);
    let root = heap.alloc_cons(child, TaggedValue::NIL);
    let ((), certificate) = crate::tagged::collection_reads::capture(|| {
        heap.begin_stw_collection();
        heap.seed_root(root);
        // Exercise the mutator's termination drain, not collect_exact's
        // worker-thread mark (which has no active Lisp-read capture).
        heap.incremental_drain_all();
    });
    assert!(certificate.is_some(), "collection alone is not a Lisp read");
    assert!(
        !crate::tagged::collection_reads::is_observed(root.bits()),
        "the collector must not mark a root as Lisp-observed"
    );
    assert!(
        !crate::tagged::collection_reads::is_observed(child.bits()),
        "the collector must not mark a child as Lisp-observed"
    );
    heap.complete_collection();
    assert_eq!(root.cons_car().cons_car(), TaggedValue::fixnum(67));
}

/// Cover vectorlike classification and string projections as well as conses.
#[cfg(feature = "jit")]
#[test]
fn heapborrow_r8_vector_and_string_scans_do_not_observe_lisp_reads() {
    use crate::tagged::collection_reads::{CompiledJournalMode, force_compiled_journal_for_test};
    struct RestoreMode;
    impl Drop for RestoreMode {
        fn drop(&mut self) {
            force_compiled_journal_for_test(None);
        }
    }
    let mut heap = TestHeap::new();
    heap.set_generational_enabled(false);
    heap.install();
    let _restore = RestoreMode;
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let text = heap.alloc_string(crate::heap_types::LispString::from_utf8("gc retained"));
    let root = heap.alloc_vector(vec![text]);
    let ((), certificate) = crate::tagged::collection_reads::capture(|| {
        heap.begin_stw_collection();
        heap.seed_root(root);
        heap.incremental_drain_all();
    });
    assert!(certificate.is_some());
    for value in [root, text] {
        assert!(!crate::tagged::collection_reads::is_observed(value.bits()));
    }
    heap.complete_collection();
    assert_eq!(
        root.as_vector_data().unwrap()[0].as_str_owned().unwrap(),
        "gc retained"
    );

    // Real Lisp projections must still create dependencies after GC.
    let (_, reads) = crate::tagged::collection_reads::capture(|| {
        root.as_vector_data().unwrap()[0].as_str_owned().unwrap()
    });
    assert!(reads.is_some());
    assert!(crate::tagged::collection_reads::is_observed(root.bits()));
    assert!(crate::tagged::collection_reads::is_observed(text.bits()));
}
