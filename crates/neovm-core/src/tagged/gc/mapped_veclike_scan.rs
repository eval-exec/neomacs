//! Stopped-start backing capture for the first concurrent pdump scan.
//!
//! A marker must not reread `LispValueVec`'s storage enum while the mutator
//! promotes its mapped backing. Capture the span before publication instead.

use std::cell::Cell;
use std::marker::PhantomData;
use std::ptr::NonNull;

use super::scan_contract::SingleMutatorWorld;
use super::{MarkWord, TaggedHeap, knobs};
use crate::tagged::header::{
    CHAR_TABLE_TOP_SLOTS, CharTableObj, RecordObj, SubCharTableObj, VecLikeHeader, VecLikeType,
    VectorObj, VectorScanEntry, load_value_atomic,
};
use crate::tagged::value::TaggedValue;

/// Immutable descriptor metadata captured while this heap's writer is stopped.
#[derive(Debug)]
struct MappedVeclikeScanEntry {
    heap_identity: SnapshotHeapIdentity,
    payload: MappedVeclikePayload,
}

/// An admitted heap identity, distinct from an object or backing address.
/// It is compared only; the marker never dereferences this heap pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SnapshotHeapIdentity(NonNull<TaggedHeap>);

impl From<&SingleMutatorWorld<'_>> for SnapshotHeapIdentity {
    fn from(world: &SingleMutatorWorld<'_>) -> Self {
        Self(NonNull::from(world.heap()))
    }
}
static_assertions::assert_not_impl_any!(SnapshotHeapIdentity: Send, Sync);

#[derive(Debug)]
enum MappedVeclikePayload {
    Backing(VectorScanEntry),
    CharTable {
        fixed: CharTableFixedSlots,
        extras: VectorScanEntry,
    },
    /// Mutable owned records have no backing retirement hook. Unported kinds
    /// (including lazy bytecode stubs) also remain termination-only.
    Deferred(MarkWord),
}

/// Fixed-layout slot provenance, never a borrow of the mutable storage enum.
#[derive(Debug)]
struct CharTableFixedSlots {
    object: *const CharTableObj,
}

static_assertions::assert_not_impl_any!(MappedVeclikeScanEntry: Send, Sync);
static_assertions::assert_not_impl_any!(MappedVeclikePayload: Send, Sync);
static_assertions::assert_not_impl_any!(CharTableFixedSlots: Send, Sync);

/// The one marker's first-partition scan descriptors.
///
/// This transparent wrapper preserves the former `Vec<usize>` field layout in
/// `TaggedHeap` and `ConcurrentMarkJob`. Heap identities live in cold entries;
/// an empty snapshot owns no raw backing to decode.
#[repr(transparent)]
pub(crate) struct MappedVeclikeScanSnapshot {
    entries: Vec<MappedVeclikeScanEntry>,
    _exclusive_reader: PhantomData<Cell<()>>,
}

// SAFETY: only stopped-start admission can construct this private payload.
// Mapped spans stay mapped through join or heap abandonment. Owned vector
// spans retire before replacement when their scan mode permits capture;
// owned char-table/sub-table spans are fixed-length and only atomically
// overwritten. Mutable owned record spans are not captured. The one marker
// decodes slots with Acquire, and never rereads a live LispValueVec enum.
unsafe impl Send for MappedVeclikeScanSnapshot {}
static_assertions::assert_impl_all!(MappedVeclikeScanSnapshot: Send, std::fmt::Debug);
static_assertions::assert_not_impl_any!(MappedVeclikeScanSnapshot: Sync, Clone);
static_assertions::assert_eq_size!(MappedVeclikeScanSnapshot, Vec<usize>);
static_assertions::assert_eq_size!(Option<MappedVeclikeScanSnapshot>, Option<Vec<usize>>);
const _: () = {
    assert!(
        std::mem::align_of::<MappedVeclikeScanSnapshot>() == std::mem::align_of::<Vec<usize>>()
    );
    assert!(
        std::mem::align_of::<Option<MappedVeclikeScanSnapshot>>()
            == std::mem::align_of::<Option<Vec<usize>>>()
    );
    assert!(std::mem::offset_of!(MappedVeclikeScanSnapshot, entries) == 0);
};

impl std::fmt::Debug for MappedVeclikeScanSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MappedVeclikeScanSnapshot")
            .field("entries", &self.entries)
            .finish()
    }
}

/// A decoded child is confined to the collector invoking `scan`.
pub(super) enum MappedVeclikeScanItem {
    Child(TaggedValue),
    Deferred(MarkWord),
}

static_assertions::assert_not_impl_any!(MappedVeclikeScanItem: Send, Sync);

impl MappedVeclikeScanSnapshot {
    /// Capture each live header's subtype and backing while its writer is stopped.
    ///
    /// # Safety
    /// Every header is live and belongs to `world`'s heap. The production mapped
    /// registry retains these headers and mapped spans until the marker joins
    /// or the heap is abandoned. Owned vector spans follow that heap's existing
    /// retirement protocol; owned char-table/sub-table backings cannot resize
    /// or be replaced during the scan and their slot writes are atomic with
    /// pointee publication. No Lisp callback, GC safepoint or writer may run
    /// during capture. Test fixtures must provide the same storage lifetime.
    pub(crate) unsafe fn capture(
        world: &SingleMutatorWorld<'_>,
        headers: impl IntoIterator<Item = *mut VecLikeHeader>,
    ) -> Self {
        let owned_vector_scan = world.heap().vec_scan != knobs::VecScanMode::Defer;
        let entries = headers
            .into_iter()
            .map(|header| {
                // SAFETY: capture's provenance contract admits this aligned,
                // initialized same-heap header while every writer is stopped.
                let type_tag = unsafe { (*header).type_tag };
                // SAFETY: tagging a retained veclike header only constructs
                // collector work; no live payload is borrowed on the worker.
                let owner = MarkWord::of(unsafe { TaggedValue::from_veclike_ptr(header) });
                let payload = match type_tag {
                    VecLikeType::Vector => {
                        // SAFETY: the stopped header's immutable subtype proves
                        // VectorObj layout; the borrowed enum cannot race here.
                        let data = unsafe { &(*(header as *const VectorObj)).data };
                        let backing = data.scan_entry();
                        if !backing.is_mapped() && !owned_vector_scan {
                            // Defer mode also disables the vector retirement
                            // hook; it cannot lend an owned backing to a reader.
                            MappedVeclikePayload::Deferred(owner)
                        } else {
                            MappedVeclikePayload::Backing(backing)
                        }
                    }
                    VecLikeType::Record | VecLikeType::WindowConfiguration => {
                        // SAFETY: these stopped subtypes both use RecordObj's
                        // layout. A mapped backing remains immutable on promote.
                        let data = unsafe { &(*(header as *const RecordObj)).data };
                        let backing = data.scan_entry();
                        if !backing.is_mapped() {
                            // RecordBulk may replace or grow this Vec without
                            // retirement. Termination traces its current data;
                            // the existing SATB barrier preserves old children.
                            MappedVeclikePayload::Deferred(owner)
                        } else {
                            MappedVeclikePayload::Backing(backing)
                        }
                    }
                    VecLikeType::SubCharTable => {
                        // SAFETY: the stopped subtype proves SubCharTableObj
                        // layout. Fixed-length writes retain this captured span.
                        let contents = unsafe { &(*(header as *const SubCharTableObj)).contents };
                        MappedVeclikePayload::Backing(contents.scan_entry())
                    }
                    VecLikeType::CharTable => {
                        let object = header as *const CharTableObj;
                        let fixed = CharTableFixedSlots { object };
                        // SAFETY: exclusive stopped-start capture prevents an
                        // enum promotion while this span descriptor is read.
                        let extras = unsafe { (*object).extras.scan_entry() };
                        MappedVeclikePayload::CharTable { fixed, extras }
                    }
                    // These unported kinds remain termination-only. Naming
                    // every variant makes new kinds require a scan decision.
                    // ByteCode's lazy pdump materialization replaces plain
                    // payload metadata and remains mutator-only.
                    VecLikeType::Bignum
                    | VecLikeType::Marker
                    | VecLikeType::Overlay
                    | VecLikeType::Finalizer
                    | VecLikeType::SymbolWithPos
                    | VecLikeType::UserPtr
                    | VecLikeType::Process
                    | VecLikeType::Frame
                    | VecLikeType::Window
                    | VecLikeType::BoolVector
                    | VecLikeType::Buffer
                    | VecLikeType::HashTable
                    | VecLikeType::Obarray
                    | VecLikeType::Terminal
                    | VecLikeType::Subr
                    | VecLikeType::Xwidget
                    | VecLikeType::XwidgetView
                    | VecLikeType::Thread
                    | VecLikeType::Mutex
                    | VecLikeType::CondVar
                    | VecLikeType::ModuleFunction
                    | VecLikeType::Sqlite
                    | VecLikeType::Lambda
                    | VecLikeType::Font
                    | VecLikeType::Macro
                    | VecLikeType::ByteCode
                    | VecLikeType::Timer
                    | VecLikeType::SurfaceHandle
                    | VecLikeType::VideoHandle => MappedVeclikePayload::Deferred(owner),
                };
                MappedVeclikeScanEntry {
                    heap_identity: SnapshotHeapIdentity::from(world),
                    payload,
                }
            })
            .collect();
        Self {
            entries,
            _exclusive_reader: PhantomData,
        }
    }

    /// Scan captured spans, never a live storage enum or growable Vec header.
    ///
    /// # Safety
    /// This is the admitted cycle's owning marker, before its explicit finish,
    /// or a stopped fixture owner providing the same storage contracts.
    /// Capture's backing retention and atomic slot publication contracts still
    /// hold, including retired vectors and the whole image on abandonment.
    pub(super) unsafe fn scan(&self, mut visit: impl FnMut(MappedVeclikeScanItem)) {
        let heap_identity = self.entries.first().map(|entry| entry.heap_identity);
        for entry in &self.entries {
            debug_assert_eq!(Some(entry.heap_identity), heap_identity);
            match &entry.payload {
                MappedVeclikePayload::Backing(backing) => {
                    // SAFETY: capture's retained initialized span contract
                    // remains in force until this marker's last read.
                    unsafe {
                        backing.scan_values(|value| visit(MappedVeclikeScanItem::Child(value)))
                    };
                }
                MappedVeclikePayload::CharTable { fixed, extras } => {
                    // SAFETY: capture proved this retained CharTableObj's
                    // layout. addr_of! computes only fixed field addresses;
                    // it never reads or borrows the mutable storage enum.
                    let fields = unsafe {
                        [
                            std::ptr::addr_of!((*fixed.object).defalt),
                            std::ptr::addr_of!((*fixed.object).parent),
                            std::ptr::addr_of!((*fixed.object).purpose),
                            std::ptr::addr_of!((*fixed.object).ascii),
                        ]
                    };
                    for slot in fields {
                        // SAFETY: each captured fixed field remains initialized
                        // and aligned; overlapping writes use atomic publication.
                        let value = load_value_atomic(unsafe { &*slot });
                        visit(MappedVeclikeScanItem::Child(value));
                    }
                    // SAFETY: the retained object has this fixed-size inline
                    // array; computing its address does not borrow the owner.
                    let contents = unsafe {
                        std::ptr::addr_of!((*fixed.object).contents).cast::<TaggedValue>()
                    };
                    for index in 0..CHAR_TABLE_TOP_SLOTS {
                        // SAFETY: capture recorded this fixed-size inline array;
                        // index stays in bounds and atomic writes retain it.
                        let slot = unsafe { &*contents.add(index) };
                        visit(MappedVeclikeScanItem::Child(load_value_atomic(slot)));
                    }
                    // SAFETY: extras have the same retained span and atomic
                    // publication contract as sub-table contents.
                    unsafe {
                        extras.scan_values(|value| visit(MappedVeclikeScanItem::Child(value)))
                    };
                }
                MappedVeclikePayload::Deferred(owner) => {
                    visit(MappedVeclikeScanItem::Deferred(*owner));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tagged::gc::TaggedHeap;
    use crate::tagged::header::LispValueVec;

    /// # Safety
    /// Headers are same-heap fixture objects; one stopped owner retains all
    /// storage until the direct scan and excludes aliases and callbacks.
    unsafe fn capture_fixture(
        heap: &mut TaggedHeap,
        headers: impl IntoIterator<Item = *const VecLikeHeader>,
    ) -> MappedVeclikeScanSnapshot {
        // SAFETY: these fixtures retain one stopped owner with no installed
        // TLS aliases, callbacks or safepoints. Every supplied header belongs
        // to this heap and its backings remain alive until the direct scan.
        let world = unsafe { SingleMutatorWorld::from_heap(heap) };
        // SAFETY: the fixture owns the supplied objects and preserves the
        // captured spans through the direct scan; no other writer exists.
        unsafe {
            MappedVeclikeScanSnapshot::capture(
                &world,
                headers.into_iter().map(|header| header.cast_mut()),
            )
        }
    }

    /// # Safety
    /// The fixture's stopped owner retains the captured image/owned/retired
    /// backings through this call; it has not invalidated a captured span.
    unsafe fn scan_fixture(
        snapshot: &MappedVeclikeScanSnapshot,
    ) -> (Vec<TaggedValue>, Vec<TaggedValue>) {
        let mut children = Vec::new();
        let mut deferred = Vec::new();
        // SAFETY: callers retain their stopped heap, image fixture and retired
        // buffers until this scan returns. No concurrent writer or GC runs.
        unsafe {
            snapshot.scan(|item| match item {
                MappedVeclikeScanItem::Child(value) => children.push(value),
                MappedVeclikeScanItem::Deferred(owner) => deferred.push(owner.value()),
            })
        };
        (children, deferred)
    }

    #[test]
    fn owned_records_defer_before_bulk_backing_replacement() {
        let mut heap = TaggedHeap::new();
        let record = heap.alloc_record(vec![TaggedValue::fixnum(1)]);
        let configuration = heap.alloc_window_configuration(vec![TaggedValue::fixnum(2)]);
        let headers = [
            record.as_veclike_ptr().unwrap(),
            configuration.as_veclike_ptr().unwrap(),
        ];
        // SAFETY: both headers were allocated on this fixture's stopped heap;
        // capture defers their owned payloads and their owners remain live.
        let snapshot = unsafe { capture_fixture(&mut heap, headers) };
        for header in headers {
            // SAFETY: both fixture subtypes use RecordObj. Only this stopped
            // owner writes, and capture deferred rather than borrowing these
            // replaceable owned buffers. Free the old buffers deliberately.
            unsafe {
                (*(header as *mut RecordObj)).data =
                    LispValueVec::owned(vec![TaggedValue::fixnum(3); 128]);
            }
        }
        // SAFETY: no backing was captured; both deferred owners remain live.
        let (children, deferred) = unsafe { scan_fixture(&snapshot) };
        assert!(children.is_empty());
        assert_eq!(deferred, vec![record, configuration]);
    }

    #[test]
    fn mapped_record_capture_survives_enum_promotion_and_bulk_replacement() {
        let image = vec![TaggedValue::fixnum(11), TaggedValue::fixnum(12)];
        let mut heap = TaggedHeap::new();
        let record = heap.alloc_record(Vec::new());
        let header = record.as_veclike_ptr().unwrap();
        // SAFETY: this stopped fixture retains `image` after the heap and
        // snapshot are destroyed. The record is not yet published to a reader.
        unsafe {
            (*(header as *mut RecordObj)).data = LispValueVec::mapped(image.as_ptr(), image.len());
        }
        // SAFETY: this same-heap owner and its immutable image remain live
        // through the direct scan, with no callback or concurrent writer.
        let snapshot = unsafe { capture_fixture(&mut heap, [header]) };
        // SAFETY: exclusive fixture mutation copies away from the captured
        // immutable image before replacing the new owned backing. Neither
        // operation can invalidate the snapshot's original mapped span.
        unsafe {
            let data = &mut (*(header as *mut RecordObj)).data;
            data.ensure_owned()[0] = TaggedValue::fixnum(99);
            *data = LispValueVec::owned(vec![TaggedValue::fixnum(100); 128]);
        }
        // SAFETY: promotion/replacement left the captured image untouched.
        let (children, deferred) = unsafe { scan_fixture(&snapshot) };
        assert_eq!(children, image);
        assert!(deferred.is_empty());
    }

    #[test]
    fn owned_vector_capture_uses_retired_buffer_after_bulk_replacement() {
        let mut heap = TaggedHeap::new();
        heap.vec_scan = knobs::VecScanMode::Snapshot;
        let vector = heap.alloc_vector(vec![TaggedValue::fixnum(21)]);
        let header = vector.as_veclike_ptr().unwrap();
        // SAFETY: the same-heap vector remains live; the enabled retire hook
        // preserves its captured backing before the fixture replacement.
        let snapshot = unsafe { capture_fixture(&mut heap, [header]) };
        // The same hook used before VectorBulk writes retains the captured
        // buffer. This direct fixture has no active worker or TLS mutator.
        heap.concurrent_clone_on_write_vector(vector);
        assert_eq!(heap.retired_vector_buffers.len(), 1);
        // SAFETY: this stopped owner replaces only the hook's fresh clone;
        // the captured old buffer remains in retired_vector_buffers.
        unsafe {
            (*(header as *mut VectorObj)).data =
                LispValueVec::owned(vec![TaggedValue::fixnum(22); 128]);
        }
        // SAFETY: the captured buffer remains in the stopped heap's retire list.
        let (children, deferred) = unsafe { scan_fixture(&snapshot) };
        assert_eq!(children, vec![TaggedValue::fixnum(21)]);
        assert!(deferred.is_empty());
    }

    #[test]
    fn owned_vector_defers_when_scan_mode_disables_retirement() {
        let mut heap = TaggedHeap::new();
        heap.vec_scan = knobs::VecScanMode::Defer;
        let vector = heap.alloc_vector(vec![TaggedValue::fixnum(31)]);
        let header = vector.as_veclike_ptr().unwrap();
        // SAFETY: the same-heap vector remains live; disabled retirement makes
        // capture defer this owner rather than lend its replaceable buffer.
        let snapshot = unsafe { capture_fixture(&mut heap, [header]) };
        // SAFETY: this stopped owner can free the old buffer because capture
        // deferred the whole object when its retirement hook was disabled.
        unsafe {
            (*(header as *mut VectorObj)).data =
                LispValueVec::owned(vec![TaggedValue::fixnum(32); 128]);
        }
        // SAFETY: no backing was captured; the deferred owner remains live.
        let (children, deferred) = unsafe { scan_fixture(&snapshot) };
        assert!(children.is_empty());
        assert_eq!(deferred, vec![vector]);
    }
}
