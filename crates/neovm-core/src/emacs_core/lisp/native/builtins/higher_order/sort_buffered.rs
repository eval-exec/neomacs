//! Call-local storage for captured native predicates. Heap state is published
//! before every boundary that can enter Lisp or collect. No shared cache or
//! heap backing reference crosses that boundary; each mutator owns its state.
use super::sort::{SortStorage, gnu_style_sort_items};
use super::{
    Flow, SortItem, SortPredicate, SortRootBatch, SortRootScope, SortRootSlot, SortRuntime, Value,
};
use std::ops::Range;

struct State {
    vector: Value,
    items: Vec<SortItem>,
    roots: Vec<Value>,
    dirty: bool,
}
impl State {
    fn publish(&mut self) {
        if self.dirty {
            self.vector
                .with_vector_data_mut(|values| {
                    for (value, item) in values.iter_mut().zip(&self.items) {
                        *value = item.value;
                    }
                })
                .unwrap();
            self.dirty = false;
        }
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
        self.dirty = false;
    }
}
/// The raw pointer refers only to this activation's Rust state. Storage and
/// runtime take disjoint, short accesses, ending before any runtime call.
struct Storage {
    state: *mut State,
    stack_roots: Vec<SortRootSlot>,
    heap_temporary: bool,
}
impl SortStorage for Storage {
    #[inline]
    fn len(&self) -> usize {
        unsafe { (*self.state).items.len() }
    }
    #[inline]
    fn item(&self, index: usize) -> SortItem {
        unsafe { (&(*self.state).items)[index] }
    }
    #[inline]
    fn put(&mut self, index: usize, item: SortItem) {
        unsafe {
            (&mut (*self.state).items)[index] = item;
            (*self.state).dirty = true;
        }
    }
    #[inline]
    fn needs_value_roots(&self) -> bool {
        true
    }
    #[inline]
    fn root_insertion_key(&self, runtime: &mut impl SortRuntime, pivot: SortItem) {
        runtime.root_sort_slot(pivot.key);
    }
    #[inline]
    fn has_merge_cleanup(&self) -> bool {
        self.heap_temporary
    }
    fn retain_stack_temporary(
        &mut self,
        runtime: &mut impl SortRuntime,
        source: &[SortItem],
    ) -> bool {
        // GNU sort.c:489-509,604-635: persistent 256-slot stack scratch, then
        // heap scratch with unwind relocation cleanup after its first use.
        if self.heap_temporary || source.len() > 256 {
            self.heap_temporary = true;
            return false;
        }
        for (index, item) in source.iter().enumerate() {
            if let Some(slot) = self.stack_roots.get(index) {
                runtime.set_sort_slot(slot, item.value);
            } else {
                self.stack_roots.push(runtime.root_sort_slot(item.value));
            }
        }
        true
    }
    fn reverse(&mut self, range: Range<usize>) {
        unsafe {
            (&mut (*self.state).items)[range].reverse();
            (*self.state).dirty = true;
        }
    }
    fn copy_within(&mut self, range: Range<usize>, dest: usize) {
        unsafe {
            (*self.state).items.copy_within(range, dest);
            (*self.state).dirty = true;
        }
    }
    fn copy_from(&mut self, dest: usize, source: &[SortItem]) {
        unsafe {
            (&mut (*self.state).items)[dest..dest + source.len()].copy_from_slice(source);
            (*self.state).dirty = true;
        }
    }
    fn snapshot(&self, range: Range<usize>) -> Vec<SortItem> {
        unsafe { (&(*self.state).items)[range].to_vec() }
    }
}
struct Runtime<'a, R: SortRuntime> {
    inner: &'a mut R,
    state: *mut State,
}
impl<R: SortRuntime> Runtime<'_, R> {
    #[cold]
    #[inline(never)]
    fn publish_roots(&mut self) {
        // No access to state remains borrowed across a runtime call.
        let len = unsafe { (*self.state).roots.len() };
        for index in 0..len {
            let value = unsafe { (&(*self.state).roots)[index] };
            self.inner.root_sort_value(value);
        }
        unsafe {
            (*self.state).publish();
        }
    }
}
impl<R: SortRuntime> Runtime<'_, R> {
    #[cold]
    #[inline(never)]
    fn finish_published_error(&mut self, call: super::NativeSortCall) -> Result<Value, Flow> {
        // The native frame remains live while observers see the permutation.
        self.publish_roots();
        let result = self.inner.finish_native_sort_call(call);
        unsafe {
            (*self.state).reload();
        }
        result
    }

    #[cold]
    #[inline(never)]
    fn call_published_predicate(
        &mut self,
        predicate: SortPredicate,
        left: Value,
        right: Value,
    ) -> Result<Value, Flow> {
        let scope = self.inner.save_sort_roots();
        self.publish_roots();
        let result = self.inner.call_sort_predicate(predicate, left, right);
        unsafe {
            (*self.state).reload();
        }
        self.inner.restore_sort_roots(scope);
        result
    }
}
impl<R: SortRuntime> SortRuntime for Runtime<'_, R> {
    #[inline]
    fn root_sort_value(&mut self, value: Value) {
        unsafe {
            (*self.state).roots.push(value);
        }
    }
    #[inline]
    fn save_sort_roots(&self) -> SortRootScope {
        SortRootScope::Buffered(unsafe { (*self.state).roots.len() })
    }
    #[inline]
    fn restore_sort_roots(&mut self, scope: SortRootScope) {
        let SortRootScope::Buffered(len) = scope else {
            unreachable!()
        };
        unsafe {
            (*self.state).roots.truncate(len);
        }
    }
    #[inline]
    fn root_sort_slot(&mut self, value: Value) -> SortRootSlot {
        let index = unsafe { (*self.state).roots.len() };
        unsafe {
            (*self.state).roots.push(value);
        }
        SortRootSlot::Buffered(index)
    }
    #[inline]
    fn clear_sort_slot(&mut self, slot: &SortRootSlot) {
        self.set_sort_slot(slot, Value::NIL);
    }
    #[inline]
    fn set_sort_slot(&mut self, slot: &SortRootSlot, value: Value) {
        let SortRootSlot::Buffered(index) = slot else {
            unreachable!()
        };
        unsafe {
            (&mut (*self.state).roots)[*index] = value;
        }
    }
    #[inline]
    fn root_sort_batch(&mut self, items: &[SortItem]) -> SortRootBatch {
        // Indices remain stable if this activation's Rust root Vec reallocates.
        // No collector or Lisp callback can run during this local extension.
        let roots = unsafe { &mut (*self.state).roots };
        let start = roots.len();
        roots.extend(items.iter().map(|item| item.value));
        SortRootBatch::Buffered(start..roots.len())
    }
    #[inline]
    fn clear_sort_batch(&mut self, batch: &SortRootBatch, range: Range<usize>) {
        let SortRootBatch::Buffered(slots) = batch else {
            unreachable!("buffered sort roots use activation-local index ranges")
        };
        debug_assert!(range.end <= slots.len());
        unsafe {
            (&mut (*self.state).roots)[slots.start + range.start..slots.start + range.end]
                .fill(Value::NIL);
        }
    }
    fn call_sort_function1(&mut self, _: Value, _: Value) -> Result<Value, Flow> {
        unreachable!()
    }
    fn call_sort_function2(&mut self, _: Value, _: Value, _: Value) -> Result<Value, Flow> {
        unreachable!()
    }
    fn compare_sort_keys(&mut self, _: &Value, _: &Value) -> Result<std::cmp::Ordering, Flow> {
        unreachable!()
    }
    #[inline]
    fn call_sort_predicate(
        &mut self,
        predicate: SortPredicate,
        left: Value,
        right: Value,
    ) -> Result<Value, Flow> {
        match self.inner.begin_native_sort_call(predicate, left, right) {
            Some(call) if call.result.is_ok() => self.inner.finish_native_sort_call(call),
            Some(call) => self.finish_published_error(call),
            None => self.call_published_predicate(predicate, left, right),
        }
    }
}
pub(super) fn sort_native_vector(
    runtime: &mut impl SortRuntime,
    vector: Value,
    predicate: SortPredicate,
    reverse: bool,
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
        dirty: false,
    };
    let ptr = &mut state as *mut State;
    let mut storage = Storage {
        state: ptr,
        stack_roots: Vec::new(),
        heap_temporary: false,
    };
    if reverse {
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
    if result.is_ok() && reverse {
        storage.reverse(0..storage.len());
    }
    state.publish();
    result
}
