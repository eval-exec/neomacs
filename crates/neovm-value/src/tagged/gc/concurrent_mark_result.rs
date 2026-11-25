/// Owned worker result, sent only after the last heap/snapshot read.
/// Headers remain live and non-moving until the coordinator joins the worker;
/// addresses are never dereferenced after the old/young sweep starts.
///
/// Sealed leaf (value-gc C2, G1): fields are private so sibling modules in
/// `tagged::gc` cannot construct or mutate a `ConcurrentMarkResult` in safe
/// code. This file is `include!`d in `gc_thread.rs` so the defining type path
/// remains `gc::gc_thread::ConcurrentMarkResult` (preserving existing
/// `!Sync` trybuild goldens); inside `gc_thread.rs` itself only the worker
/// handoff (`from_parts`) and test-only `empty_for_heap` construct it.
/// Construction requires `unsafe fn from_parts` tagged with the originating
/// heap's [`HeapIdentity`], which [`MarkExitSlot::take_for_finish`] verifies
/// before handing any deferred [`MarkWord`] to the mutator.
#[derive(Debug)]
pub(super) struct ConcurrentMarkResult {
    heap: super::HeapIdentity,
    /// Marker-owned work that the stopped mutators trace after receiving this result.
    /// Only this worker appends; the completion channel transfers ownership.
    deferred: Vec<super::MarkWord>,
    promo: Vec<usize>,
    symbols: Vec<super::SymId>,
}

impl ConcurrentMarkResult {
    /// Assemble a completed worker result bound to `heap`.
    ///
    /// # Safety
    /// Every [`MarkWord`] in `deferred` and every header address in `promo` must
    /// belong to `heap`, have been captured from live objects during an active
    /// concurrent mark cycle on `heap`, and remain retained until `heap`'s
    /// coordinator joins this result.
    pub(super) unsafe fn from_parts(
        heap: super::HeapIdentity,
        deferred: Vec<super::MarkWord>,
        promo: Vec<usize>,
        symbols: Vec<super::SymId>,
    ) -> Self {
        Self {
            heap,
            deferred,
            promo,
            symbols,
        }
    }

    /// Verify that this result belongs to `expected_heap` and unpack its logs.
    /// Fails closed with `Err(actual_heap)` on mismatch so a mis-routed worker
    /// result can never materialize foreign `MarkWord`s onto another heap.
    pub(super) fn into_parts_for_heap(
        self,
        expected_heap: super::HeapIdentity,
    ) -> Result<(Vec<super::MarkWord>, Vec<usize>, Vec<super::SymId>), super::HeapIdentity> {
        if self.heap == expected_heap {
            Ok((self.deferred, self.promo, self.symbols))
        } else {
            Err(self.heap)
        }
    }
}
