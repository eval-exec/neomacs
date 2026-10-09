//! A bounded, atomic-only model of the concurrent-mark symbol read protocol.
//! See README.md for the production correspondence and the limits of this model.

use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;

use loom::sync::Arc;
use loom::sync::atomic::{AtomicU8, AtomicU32, AtomicUsize, Ordering, fence};
use loom::thread;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use static_assertions::{assert_impl_all, assert_not_impl_any};

mod sealed {
    pub trait Sealed {}
}

/// Closed protocols make every ordering change in a control explicit.
trait Protocol: sealed::Sealed + Send + Sync + 'static {
    const WRITER_FENCE: Option<Ordering>;
    const READER_FENCE: Option<Ordering>;
    const TAG_STORE: Ordering;
    const TAG_LOAD: Ordering;
}

macro_rules! protocol {
    ($name:ident, $writer:expr, $reader:expr, $store:expr, $load:expr) => {
        #[derive(Clone, Copy, Debug)]
        struct $name;
        impl sealed::Sealed for $name {}
        impl Protocol for $name {
            const WRITER_FENCE: Option<Ordering> = $writer;
            const READER_FENCE: Option<Ordering> = $reader;
            const TAG_STORE: Ordering = $store;
            const TAG_LOAD: Ordering = $load;
        }
        assert_impl_all!($name: Send, Sync, Copy, Clone, fmt::Debug);
    };
}

protocol!(
    Production,
    Some(Ordering::Release),
    Some(Ordering::Acquire),
    Ordering::Relaxed,
    Ordering::Relaxed
);
protocol!(
    OmitWriterFence,
    None,
    Some(Ordering::Acquire),
    Ordering::Relaxed,
    Ordering::Relaxed
);
protocol!(
    OmitReaderFence,
    Some(Ordering::Release),
    None,
    Ordering::Relaxed,
    Ordering::Relaxed
);
protocol!(
    OmitBothFences,
    None,
    None,
    Ordering::Relaxed,
    Ordering::Relaxed
);
// This is deliberately stronger than production, not a substitute for it.
protocol!(
    ReleaseAcquireTagWithoutFences,
    None,
    None,
    Ordering::Release,
    Ordering::Acquire
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MockValue {
    Heap(usize),
    Symbol(usize),
    Immediate(usize),
}

impl MockValue {
    fn bits(self) -> usize {
        match self {
            Self::Heap(id) => (id << 2) | 1,
            Self::Symbol(id) => (id << 2) | 2,
            Self::Immediate(id) => id << 2,
        }
    }

    fn traceable(bits: usize, collection: Collection) -> bool {
        bits & 3 == 1 || (collection == Collection::Major && bits & 3 == 2)
    }

    // Semantic oracle: this uses the typed mock value, not raw tag decoding.
    fn expected_child(self, collection: Collection) -> Option<usize> {
        match self {
            Self::Heap(_) => Some(self.bits()),
            Self::Symbol(_) if collection == Collection::Major => Some(self.bits()),
            Self::Symbol(_) | Self::Immediate(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Collection {
    Minor,
    Major,
}

/// These numbers match GNU symbol_redirect and the production redirect mask.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
enum Redirect {
    Plain = 0,
    Alias = 1,
    Localized = 2,
    Forwarded = 3,
}

assert_impl_all!(Redirect: Send, Sync, Copy, Clone, fmt::Debug, TryFrom<u8>, Into<u8>);
const _: () = {
    assert!(std::mem::size_of::<Redirect>() == 1);
    assert!(std::mem::align_of::<Redirect>() == 1);
};

impl Redirect {
    fn decode(flags: u8) -> Self {
        Self::try_from(flags & REDIRECT_MASK)
            .expect("masked GNU redirect bits contain one of the four declared arms")
    }
}

// Model the actual byte storage, without assigning behavior to unrelated flag
// bits. These fixed high bits stay unchanged; the production tag mask is 0b11.
const REDIRECT_MASK: u8 = 0b11;
const UNCHANGED_FLAG_BITS: u8 = 0x60;

/// Opaque words stand for alias IDs, BLV handles and forward descriptors.
/// Nothing in this model borrows or dereferences payload storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueCell {
    Plain(MockValue),
    Alias(usize),
    Localized(usize),
    Forwarded(usize),
}

impl ValueCell {
    fn raw(self) -> RawCell {
        let (redirect, word) = match self {
            Self::Plain(value) => (Redirect::Plain, value.bits()),
            Self::Alias(id) => (Redirect::Alias, id),
            Self::Localized(handle) => (Redirect::Localized, handle),
            Self::Forwarded(handle) => (Redirect::Forwarded, handle),
        };
        RawCell { redirect, word }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RawCell {
    redirect: Redirect,
    word: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Snapshot {
    cell: RawCell,
    function: usize,
    plist: usize,
}

impl Snapshot {
    fn children(self, collection: Collection) -> Vec<usize> {
        let mut children = Vec::new();
        if self.cell.redirect == Redirect::Plain && MockValue::traceable(self.cell.word, collection)
        {
            children.push(self.cell.word);
        }
        if MockValue::traceable(self.function, collection) {
            children.push(self.function);
        }
        if MockValue::traceable(self.plist, collection) {
            children.push(self.plist);
        }
        children
    }
}

#[derive(Debug)]
struct AtomicSymbol {
    seq: AtomicU32,
    flags: AtomicU8,
    word: AtomicUsize,
    function: AtomicUsize,
    plist: AtomicUsize,
}

assert_impl_all!(AtomicSymbol: Send, Sync, fmt::Debug);

/// The sole writer is affine and cannot be accessed concurrently by reference.
/// Construction hands out exactly one writer and one read endpoint.
#[derive(Debug)]
struct Writer<M: Protocol> {
    symbol: Arc<AtomicSymbol>,
    current: RawCell,
    mode: PhantomData<M>,
    exclusive: PhantomData<Cell<()>>,
}

assert_impl_all!(Writer<Production>: Send, fmt::Debug);
assert_not_impl_any!(Writer<Production>: Sync, Clone, Copy);

#[derive(Debug)]
struct Reader<M: Protocol> {
    symbol: Arc<AtomicSymbol>,
    mode: PhantomData<M>,
}

assert_impl_all!(Reader<Production>: Send, Sync, fmt::Debug);
assert_not_impl_any!(Reader<Production>: Copy, Clone);

fn endpoints<M: Protocol>(initial: ValueCell) -> (Writer<M>, Reader<M>) {
    let current = initial.raw();
    let symbol = Arc::new(AtomicSymbol {
        seq: AtomicU32::new(0),
        flags: AtomicU8::new(UNCHANGED_FLAG_BITS | u8::from(current.redirect)),
        word: AtomicUsize::new(current.word),
        function: AtomicUsize::new(MockValue::Heap(10).bits()),
        plist: AtomicUsize::new(MockValue::Symbol(11).bits()),
    });
    (
        Writer {
            symbol: symbol.clone(),
            current,
            mode: PhantomData,
            exclusive: PhantomData,
        },
        Reader {
            symbol,
            mode: PhantomData,
        },
    )
}

/// Dropping the model window closes it with one atomic increment. It acquires
/// no mutex and invokes no callback. The model checker may abort exploration;
/// this guard introduces no fallible destructor operation of its own.
#[must_use = "a window retains exclusive writer access until it closes"]
#[derive(Debug)]
struct CellWindow<'a, M: Protocol> {
    writer: &'a mut Writer<M>,
}

assert_impl_all!(CellWindow<'static, Production>: Send, fmt::Debug);
assert_not_impl_any!(CellWindow<'static, Production>: Sync, Clone, Copy);

impl<M: Protocol> Writer<M> {
    fn begin(&mut self) -> CellWindow<'_, M> {
        self.symbol.seq.fetch_add(1, Ordering::Relaxed);
        if let Some(ordering) = M::WRITER_FENCE {
            fence(ordering);
        }
        CellWindow { writer: self }
    }

    // These are independent stores, just as set_symbol_function_id_for and
    // set_symbol_plist_id are outside production CellWrite.
    fn set_function(&mut self, value: MockValue) {
        self.symbol.function.store(value.bits(), Ordering::Release);
    }

    fn set_plist(&mut self, value: MockValue) {
        self.symbol.plist.store(value.bits(), Ordering::Release);
    }
}

impl<M: Protocol> CellWindow<'_, M> {
    fn publish(&mut self, value: ValueCell) {
        let next = value.raw();
        // Production always publishes the machine word with Release.
        self.writer.symbol.word.store(next.word, Ordering::Release);
        if next.redirect != self.writer.current.redirect {
            self.writer
                .symbol
                .flags
                .store(UNCHANGED_FLAG_BITS | u8::from(next.redirect), M::TAG_STORE);
        }
        // Same-arm writes model NewTag::Keep: no flags read or write.
        self.writer.current = next;
    }
}

impl<M: Protocol> Drop for CellWindow<'_, M> {
    fn drop(&mut self) {
        self.writer.symbol.seq.fetch_add(1, Ordering::Release);
    }
}

impl<M: Protocol> Reader<M> {
    /// One production read-loop attempt. None means retry; bounding the model
    /// this way makes no claim about scheduling fairness or retry convergence.
    fn try_read(&self) -> Option<Snapshot> {
        let before = self.symbol.seq.load(Ordering::Acquire);
        if before & 1 != 0 {
            return None;
        }
        let redirect = Redirect::decode(self.symbol.flags.load(M::TAG_LOAD));
        let word = self.symbol.word.load(Ordering::Acquire);
        let function = self.symbol.function.load(Ordering::Acquire);
        let plist = self.symbol.plist.load(Ordering::Acquire);
        if let Some(ordering) = M::READER_FENCE {
            fence(ordering);
        }
        if self.symbol.seq.load(Ordering::Relaxed) != before {
            return None;
        }
        Some(Snapshot {
            cell: RawCell { redirect, word },
            function,
            plist,
        })
    }
}

fn validate(snapshot: Snapshot, old: ValueCell, new: ValueCell) {
    assert!(
        snapshot.cell == old.raw() || snapshot.cell == new.raw(),
        "accepted mixed redirect/payload: {snapshot:?}, old={old:?}, new={new:?}"
    );
    assert!(
        [MockValue::Heap(10).bits(), MockValue::Immediate(12).bits()].contains(&snapshot.function),
        "function was not one complete atomic word"
    );
    assert!(
        [MockValue::Symbol(11).bits(), MockValue::Heap(13).bits()].contains(&snapshot.plist),
        "plist was not one complete atomic word"
    );
    for collection in [Collection::Minor, Collection::Major] {
        let mut expected = Vec::new();
        let cell = if snapshot.cell == old.raw() { old } else { new };
        match cell {
            ValueCell::Plain(value) => expected.extend(value.expected_child(collection)),
            ValueCell::Alias(_) | ValueCell::Localized(_) | ValueCell::Forwarded(_) => {}
        }
        let function = if snapshot.function == MockValue::Heap(10).bits() {
            MockValue::Heap(10)
        } else {
            MockValue::Immediate(12)
        };
        let plist = if snapshot.plist == MockValue::Symbol(11).bits() {
            MockValue::Symbol(11)
        } else {
            MockValue::Heap(13)
        };
        expected.extend(function.expected_child(collection));
        expected.extend(plist.expected_child(collection));
        assert_eq!(snapshot.children(collection), expected);
    }
}

fn model<M: Protocol>(initial: ValueCell, next: ValueCell) {
    let mut builder = loom::model::Builder::new();
    builder.max_threads = 2;
    builder.max_branches = 128;
    // Explicitly prevent environment variables from silently selecting a
    // permutation, duration, preemption or checkpoint truncation.
    builder.max_permutations = None;
    builder.max_duration = None;
    builder.preemption_bound = None;
    builder.checkpoint_file = None;
    builder.expect_explicit_explore = false;
    builder.location = true;
    builder.log = false;
    builder.check(move || {
        let (mut writer, reader) = endpoints::<M>(initial);
        let old = initial.raw();
        let new = next.raw();
        let first = reader.try_read().expect("initial state is even");
        assert_eq!(first.cell, old);
        validate(first, initial, next);
        let worker = thread::spawn(move || {
            writer.set_function(MockValue::Immediate(12));
            {
                let mut window = writer.begin();
                window.publish(next);
            }
            writer.set_plist(MockValue::Heap(13));
        });
        if let Some(snapshot) = reader.try_read() {
            validate(snapshot, initial, next);
        }
        worker.join().expect("writer completed");
        let last = reader
            .try_read()
            .expect("joined writer leaves an even state");
        assert_eq!(last.cell, new);
        assert_eq!(last.function, MockValue::Immediate(12).bits());
        assert_eq!(last.plist, MockValue::Heap(13).bits());
        validate(last, initial, next);
    });
}

#[test]
fn production_plain_to_alias() {
    model::<Production>(ValueCell::Plain(MockValue::Heap(1)), ValueCell::Alias(9));
}

#[test]
fn production_alias_to_plain() {
    model::<Production>(ValueCell::Alias(5), ValueCell::Plain(MockValue::Heap(2)));
}

#[test]
fn production_plain_to_localized() {
    model::<Production>(
        ValueCell::Plain(MockValue::Heap(1)),
        ValueCell::Localized(9),
    );
}

#[test]
fn production_plain_to_forwarded() {
    model::<Production>(
        ValueCell::Plain(MockValue::Heap(1)),
        ValueCell::Forwarded(9),
    );
}

#[test]
fn production_forwarded_to_localized() {
    model::<Production>(ValueCell::Forwarded(5), ValueCell::Localized(9));
}

#[test]
fn production_same_arm_updates() {
    for (old, new) in [
        (
            ValueCell::Plain(MockValue::Heap(1)),
            ValueCell::Plain(MockValue::Symbol(2)),
        ),
        (ValueCell::Alias(5), ValueCell::Alias(9)),
        (ValueCell::Localized(5), ValueCell::Localized(9)),
        (ValueCell::Forwarded(5), ValueCell::Forwarded(9)),
    ] {
        model::<Production>(old, new);
    }
}

#[test]
#[should_panic(expected = "accepted mixed redirect/payload")]
fn negative_control_without_writer_fence() {
    model::<OmitWriterFence>(ValueCell::Alias(5), ValueCell::Plain(MockValue::Heap(2)));
}

#[test]
#[should_panic(expected = "accepted mixed redirect/payload")]
fn negative_control_without_reader_fence() {
    model::<OmitReaderFence>(ValueCell::Alias(5), ValueCell::Plain(MockValue::Heap(2)));
}

#[test]
fn same_tag_payload_release_acquire_without_fences() {
    model::<OmitBothFences>(
        ValueCell::Plain(MockValue::Heap(1)),
        ValueCell::Plain(MockValue::Heap(2)),
    );
}

#[test]
fn stronger_tag_release_acquire_experiment_without_fences() {
    model::<ReleaseAcquireTagWithoutFences>(
        ValueCell::Alias(5),
        ValueCell::Plain(MockValue::Heap(2)),
    );
}
