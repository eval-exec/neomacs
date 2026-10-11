//! Call-local storage for captured native predicates. Heap state is published
//! before every boundary that can enter Lisp or collect. No shared cache or
//! heap backing reference crosses that boundary; each mutator owns its state.
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::wildcard_enum_match_arm)]
use super::sort::{
    SortReadStorage, SortRootGuard, SortStorage, TemporaryRootRetention, TemporaryStorage,
    ValueRooting, gnu_style_sort_items,
};
use super::{Flow, SortDirection, SortItem, SortPredicate, SortRuntime, Value};
use std::marker::PhantomData;
use std::ops::Range;

/// A saved length in this mutator's private arena, never a specpdl scope.
#[derive(Debug)]
#[must_use = "restore the buffered sort's roots at the end of their scope"]
struct BufferedRootScope {
    len: usize,
    owner: PhantomData<*const ()>,
}

/// An index in this activation's private arena, never a specpdl root slot.
#[derive(Clone, Copy, Debug)]
struct BufferedRootSlot {
    index: usize,
    owner: PhantomData<*const ()>,
}

/// Remaining roots in one buffered merge. The empty default owns no roots.
#[derive(Debug, Default)]
struct BufferedRootBatch {
    slots: Range<usize>,
    owner: PhantomData<*const ()>,
}

static_assertions::assert_type_ne_all!(BufferedRootScope, super::SortRootScope);
static_assertions::assert_type_ne_all!(BufferedRootSlot, super::SortRootSlot);
static_assertions::assert_not_impl_any!(BufferedRootScope: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(BufferedRootSlot: Send, Sync);
static_assertions::assert_not_impl_any!(BufferedRootBatch: Send, Sync);
const _: () = assert!(std::mem::size_of::<BufferedRootScope>() == std::mem::size_of::<usize>());
const _: () = assert!(std::mem::size_of::<BufferedRootSlot>() == std::mem::size_of::<usize>());
const _: () =
    assert!(std::mem::size_of::<BufferedRootBatch>() == std::mem::size_of::<Range<usize>>());

/// Whether private permutation moves still need publication to the live vector.
/// This is activation-local state, with no mutator references or shared cache.
#[derive(Clone, Copy, Debug)]
enum VectorPublication {
    Current,
    Pending,
}
const _: () = assert!(std::mem::size_of::<VectorPublication>() == std::mem::size_of::<bool>());

#[derive(Debug)]
struct State {
    vector: Value,
    items: Vec<SortItem>,
    roots: Vec<Value>,
    publication: VectorPublication,
    _owner: PhantomData<*const ()>,
}
impl State {
    fn publish(&mut self) {
        match self.publication {
            VectorPublication::Current => return,
            VectorPublication::Pending => {}
        }
        self.vector
            .with_vector_data_mut(|values| {
                for (value, item) in values.iter_mut().zip(&self.items) {
                    *value = item.value;
                }
            })
            .unwrap();
        self.publication = VectorPublication::Current;
    }
    fn reload(&mut self) {
        for (item, value) in self
            .items
            .iter_mut()
            .zip(self.vector.as_vector_data().unwrap())
        {
            *item = SortItem {
                value: *value,
                key: *value,
            };
        }
        self.publication = VectorPublication::Current;
    }
}
/// The raw pointer refers only to this activation's Rust state. Storage and
/// runtime take disjoint, short accesses, ending before any runtime call.
#[derive(Debug)]
struct Storage {
    state: *mut State,
    stack_roots: Vec<BufferedRootSlot>,
    temporary_storage: TemporaryStorage,
}
impl SortReadStorage for Storage {
    #[inline]
    fn len(&self) -> usize {
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe { (*self.state).items.len() }
    }
    #[inline]
    fn item(&self, index: usize) -> SortItem {
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe { (&(*self.state).items)[index] }
    }
    fn snapshot(&self, range: Range<usize>) -> Vec<SortItem> {
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe { (&(*self.state).items)[range].to_vec() }
    }
}
impl SortStorage<BufferedRootSlot> for Storage {
    #[inline]
    fn put(&mut self, index: usize, item: SortItem) {
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe {
            (&mut (*self.state).items)[index] = item;
            (*self.state).publication = VectorPublication::Pending;
        }
    }
    #[inline]
    fn value_rooting(&self) -> ValueRooting {
        ValueRooting::Required
    }
    #[inline]
    fn root_insertion_key(
        &self,
        runtime: &mut impl SortRuntime<RootSlot = BufferedRootSlot>,
        pivot: SortItem,
    ) {
        runtime.root_sort_slot(pivot.key);
    }
    #[inline]
    fn has_merge_cleanup(&self) -> bool {
        self.temporary_storage == TemporaryStorage::Heap
    }
    fn retain_stack_temporary(
        &mut self,
        runtime: &mut impl SortRuntime<RootSlot = BufferedRootSlot>,
        source: &[SortItem],
    ) -> TemporaryRootRetention {
        // GNU sort.c:489-509,604-635: persistent 256-slot stack scratch, then
        // heap scratch with unwind relocation cleanup after its first use.
        if self.temporary_storage == TemporaryStorage::Heap || source.len() > 256 {
            self.temporary_storage = TemporaryStorage::Heap;
            return TemporaryRootRetention::Merge;
        }
        for (index, item) in source.iter().enumerate() {
            if let Some(slot) = self.stack_roots.get(index) {
                runtime.set_sort_slot(slot, item.value);
            } else {
                self.stack_roots.push(runtime.root_sort_slot(item.value));
            }
        }
        TemporaryRootRetention::WholeSort
    }
    fn reverse(&mut self, range: Range<usize>) {
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe {
            (&mut (*self.state).items)[range].reverse();
            (*self.state).publication = VectorPublication::Pending;
        }
    }
    fn copy_within(&mut self, range: Range<usize>, dest: usize) {
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe {
            (*self.state).items.copy_within(range, dest);
            (*self.state).publication = VectorPublication::Pending;
        }
    }
    fn copy_from(&mut self, dest: usize, source: &[SortItem]) {
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe {
            (&mut (*self.state).items)[dest..dest + source.len()].copy_from_slice(source);
            (*self.state).publication = VectorPublication::Pending;
        }
    }
}
#[derive(Debug)]
struct Runtime<'a, R: SortRuntime> {
    inner: &'a mut R,
    state: *mut State,
}
impl<R: SortRuntime> super::sort_runtime_sealed::Sealed for Runtime<'_, R> {}

impl<R: SortRuntime> Runtime<'_, R> {
    /// # Safety
    /// `state` must point to this runtime's live sorting activation. Storage
    /// and runtime accesses must be exclusive and end before calling `inner`.
    #[cold]
    #[inline(never)]
    unsafe fn publish_roots(inner: &mut R, state: *mut State) {
        // No access to state remains borrowed across a runtime call.
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        let len = unsafe { (*state).roots.len() };
        for index in 0..len {
            // SAFETY: state belongs to the live sorting activation. Storage and
            // runtime accesses are disjoint and end before any callback or collection.
            let value = unsafe { (&(*state).roots)[index] };
            inner.root_sort_value(value);
        }
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe {
            (*state).publish();
        }
    }
}
impl<R: SortRuntime> Runtime<'_, R> {
    #[cold]
    #[inline(never)]
    fn finish_published_error(&mut self, call: super::NativeSortCall) -> Result<Value, Flow> {
        // The native frame remains live while observers see the permutation.
        // SAFETY: this runtime owns the live activation's state pointer; no
        // storage or runtime access remains borrowed during publication.
        unsafe { Self::publish_roots(self.inner, self.state) };
        let result = self.inner.finish_native_sort_call(call);
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe {
            (*self.state).reload();
        }
        result
    }

    #[cold]
    #[inline(never)]
    fn call_published<T>(
        &mut self,
        call: impl FnOnce(&mut R) -> Result<T, Flow>,
    ) -> Result<T, Flow> {
        let mut roots = SortRootGuard::new(self.inner, ValueRooting::Required);
        let runtime = roots.runtime();
        // SAFETY: this runtime owns the live activation's state pointer; no
        // storage or runtime access remains borrowed during publication.
        unsafe { Self::publish_roots(runtime, self.state) };
        let result = call(runtime);
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe {
            (*self.state).reload();
        }
        result
    }
}
impl<R: SortRuntime> SortRuntime for Runtime<'_, R> {
    type RootScope = BufferedRootScope;
    type RootSlot = BufferedRootSlot;
    type RootBatch = BufferedRootBatch;
    #[inline]
    fn root_sort_value(&mut self, value: Value) {
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe {
            (*self.state).roots.push(value);
        }
    }
    #[inline]
    fn save_sort_roots(&self) -> Self::RootScope {
        BufferedRootScope {
            // SAFETY: state belongs to the live sorting activation. Storage and
            // runtime accesses are disjoint and end before any callback or collection.
            len: unsafe { (*self.state).roots.len() },
            owner: PhantomData,
        }
    }
    #[inline]
    fn restore_sort_roots(&mut self, scope: Self::RootScope) {
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe {
            (*self.state).roots.truncate(scope.len);
        }
    }
    #[inline]
    fn root_sort_slot(&mut self, value: Value) -> Self::RootSlot {
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        let index = unsafe { (*self.state).roots.len() };
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe {
            (*self.state).roots.push(value);
        }
        BufferedRootSlot {
            index,
            owner: PhantomData,
        }
    }
    #[inline]
    fn clear_sort_slot(&mut self, slot: &Self::RootSlot) {
        self.set_sort_slot(slot, Value::NIL);
    }
    #[inline]
    fn set_sort_slot(&mut self, slot: &Self::RootSlot, value: Value) {
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe {
            (&mut (*self.state).roots)[slot.index] = value;
        }
    }
    #[inline]
    fn root_sort_batch(&mut self, values: impl Iterator<Item = Value>) -> Self::RootBatch {
        // Indices remain stable if this activation's Rust root Vec reallocates.
        // No collector or Lisp callback can run during this local extension.
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        let roots = unsafe { &mut (*self.state).roots };
        let start = roots.len();
        roots.extend(values);
        BufferedRootBatch {
            slots: start..roots.len(),
            owner: PhantomData,
        }
    }
    #[inline]
    fn clear_sort_batch(&mut self, batch: &Self::RootBatch, range: Range<usize>) {
        let slots = &batch.slots;
        debug_assert!(range.end <= slots.len());
        // SAFETY: state belongs to the live sorting activation. Storage and
        // runtime accesses are disjoint and end before any callback or collection.
        unsafe {
            (&mut (*self.state).roots)[slots.start + range.start..slots.start + range.end]
                .fill(Value::NIL);
        }
    }
    fn call_sort_function1(&mut self, function: Value, arg: Value) -> Result<Value, Flow> {
        self.call_published(move |runtime| runtime.call_sort_function1(function, arg))
    }
    fn call_sort_function2(
        &mut self,
        function: Value,
        left: Value,
        right: Value,
    ) -> Result<Value, Flow> {
        self.call_published(move |runtime| runtime.call_sort_function2(function, left, right))
    }
    fn compare_sort_keys(
        &mut self,
        left: &Value,
        right: &Value,
    ) -> Result<std::cmp::Ordering, Flow> {
        // Copy handles before entering the cold publication helper: references
        // to these keys belong only to that helper's owned closure.
        let (left, right) = (*left, *right);
        self.call_published(move |runtime| runtime.compare_sort_keys(&left, &right))
    }
    // Keep the guarded native entry and finish at this private sort site.
    #[inline(always)]
    fn call_sort_predicate(
        &mut self,
        predicate: SortPredicate,
        left: Value,
        right: Value,
    ) -> Result<Value, Flow> {
        match self.inner.begin_native_sort_call(predicate, left, right) {
            Some(call) if call.result.is_ok() => self.inner.finish_native_sort_call(call),
            Some(call) => self.finish_published_error(call),
            None => self
                .call_published(move |runtime| runtime.call_sort_predicate(predicate, left, right)),
        }
    }
}
pub(super) fn sort_native_vector(
    runtime: &mut impl SortRuntime,
    vector: Value,
    predicate: SortPredicate,
    reverse: SortDirection,
) -> Result<(), Flow> {
    let mut state = State {
        vector,
        items: vector
            .as_vector_data()
            .unwrap()
            .iter()
            .map(|value| SortItem {
                value: *value,
                key: *value,
            })
            .collect(),
        roots: Vec::new(),
        publication: VectorPublication::Current,
        _owner: PhantomData,
    };
    let ptr = &mut state as *mut State;
    let mut storage = Storage {
        state: ptr,
        stack_roots: Vec::new(),
        temporary_storage: TemporaryStorage::Stack,
    };
    if reverse.is_descending() {
        storage.reverse(0..storage.len());
    }
    let result = gnu_style_sort_items(
        &mut Runtime {
            inner: runtime,
            state: ptr,
        },
        &mut storage,
        predicate,
    );
    if result.is_ok() && reverse.is_descending() {
        storage.reverse(0..storage.len());
    }
    state.publish();
    result
}
