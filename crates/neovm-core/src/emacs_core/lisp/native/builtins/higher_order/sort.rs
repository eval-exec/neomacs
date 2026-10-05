//! GNU TimSort storage and merging. Vector sorts read and write the live
//! Lisp vector, retaining no slice borrow across Lisp callbacks.
use super::{Flow, SortItem, SortPredicate, SortRuntime, Value};
use std::ops::Range;

#[cfg(test)]
#[path = "../tests/gd_e_sort_stack_temporary.rs"]
mod gd_e_sort_stack_temporary_tests;

/// Storage belongs to one mutator invocation. Shared heap stores go through
/// the runtime barriers; no pointer to vector backing survives a Lisp call.
pub(super) trait SortStorage {
    fn len(&self) -> usize;
    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn item(&self, index: usize) -> SortItem;
    fn put(&mut self, index: usize, item: SortItem);
    #[inline]
    fn insertion_pivot(&self, _: usize, pivot: SortItem) -> SortItem {
        pivot
    }
    #[inline]
    fn needs_value_roots(&self) -> bool {
        false
    }
    #[inline]
    fn root_insertion_key(&self, _: &mut impl SortRuntime, _: SortItem) {}
    /// Retain a copy made into GNU's persistent stack temporary array.
    /// False means the temporary values need the merge's remaining-value roots.
    #[inline]
    fn retain_stack_temporary(&mut self, _: &mut impl SortRuntime, _: &[SortItem]) -> bool {
        false
    }
    /// Scratch list storage is discarded on failure. Live vector storage
    /// restores relocated values only when GNU registered unwind cleanup.
    #[inline]
    fn has_merge_cleanup(&self) -> bool {
        true
    }
    fn reverse(&mut self, range: Range<usize>);
    fn copy_within(&mut self, range: Range<usize>, dest: usize);
    fn copy_from(&mut self, dest: usize, source: &[SortItem]);
    #[inline]
    fn snapshot(&self, range: Range<usize>) -> Vec<SortItem> {
        range.map(|index| self.item(index)).collect()
    }
}
impl SortStorage for [SortItem] {
    #[inline]
    fn len(&self) -> usize {
        self.len()
    }
    #[inline]
    fn item(&self, index: usize) -> SortItem {
        self[index]
    }
    #[inline]
    fn put(&mut self, index: usize, item: SortItem) {
        self[index] = item;
    }
    #[inline]
    fn reverse(&mut self, range: Range<usize>) {
        self[range].reverse();
    }
    #[inline]
    fn copy_within(&mut self, range: Range<usize>, dest: usize) {
        self.copy_within(range, dest);
    }
    #[inline]
    fn copy_from(&mut self, dest: usize, source: &[SortItem]) {
        self[dest..dest + source.len()].copy_from_slice(source);
    }
    #[inline]
    fn snapshot(&self, range: Range<usize>) -> Vec<SortItem> {
        self[range].to_vec()
    }
}
/// A call-local borrowed view; it retains no heap borrow or shared cache.
struct SortRange<'a, S: SortStorage + ?Sized> {
    storage: &'a S,
    range: Range<usize>,
}
impl<'a, S: SortStorage + ?Sized> SortRange<'a, S> {
    #[inline]
    fn new(storage: &'a S, range: Range<usize>) -> Self {
        Self { storage, range }
    }
}
impl<S: SortStorage + ?Sized> SortStorage for SortRange<'_, S> {
    #[inline]
    fn len(&self) -> usize {
        self.range.len()
    }
    #[inline]
    fn item(&self, index: usize) -> SortItem {
        self.storage.item(self.range.start + index)
    }
    fn put(&mut self, _: usize, _: SortItem) {
        unreachable!()
    }
    fn reverse(&mut self, _: Range<usize>) {
        unreachable!()
    }
    fn copy_within(&mut self, _: Range<usize>, _: usize) {
        unreachable!()
    }
    fn copy_from(&mut self, _: usize, _: &[SortItem]) {
        unreachable!()
    }
}

/// Keys, when supplied, are private to the sorting invocation and rooted by
/// its existing root scope. Values always reside in the Lisp vector.
pub(super) struct VectorSortStorage {
    pub(super) vector: Value,
    pub(super) keys: Option<Vec<Value>>,
    // GNU sort.c:175-182 keeps this array in the sort's stack frame. Its
    // previously written values remain visible to conservative C-stack GC,
    // including after a later merge switches to heap temporary storage.
    stack_temporary_roots: Vec<crate::emacs_core::eval::SpecpdlRootSlot>,
    heap_temporary_allocated: bool,
}
impl VectorSortStorage {
    pub(super) fn new(vector: Value) -> Self {
        Self {
            vector,
            keys: None,
            stack_temporary_roots: Vec::new(),
            heap_temporary_allocated: false,
        }
    }
}
impl SortStorage for VectorSortStorage {
    #[inline]
    fn len(&self) -> usize {
        self.vector.as_vector_data().unwrap().len()
    }
    #[inline]
    fn item(&self, index: usize) -> SortItem {
        let value = self.vector.as_vector_data().unwrap()[index];
        SortItem {
            value,
            key: self.keys.as_ref().map_or(value, |keys| keys[index]),
        }
    }
    #[inline]
    fn insertion_pivot(&self, index: usize, pivot: SortItem) -> SortItem {
        if self.keys.is_some() {
            SortItem {
                value: self.item(index).value,
                ..pivot
            }
        } else {
            pivot
        }
    }
    #[inline]
    fn needs_value_roots(&self) -> bool {
        true
    }
    #[inline]
    fn has_merge_cleanup(&self) -> bool {
        // merge_init registers cleanup for allocated keys (length >= 128),
        // or merge_getmem registers it upon the first heap temporary allocation
        // (GNU sort.c:1109-1121,525-526,613-619). Small stack-only merges leave
        // their completed writes visible when a comparison signals.
        self.heap_temporary_allocated || (self.keys.is_some() && self.len() >= 128)
    }
    #[inline]
    fn root_insertion_key(&self, runtime: &mut impl SortRuntime, pivot: SortItem) {
        // Keyed insertion retains only its key (already rooted with all keys),
        // then reads its live value after the comparisons (GNU sort.c:234,253).
        if self.keys.is_none() {
            runtime.root_sort_slot(pivot.key);
        }
    }
    fn retain_stack_temporary(
        &mut self,
        runtime: &mut impl SortRuntime,
        source: &[SortItem],
    ) -> bool {
        // GNU merge_init reserves 256 unkeyed slots, or separate key/value
        // areas with min(ceil(length / 2), 128) slots (sort.c:489-509).
        let capacity = if self.keys.is_some() {
            self.len().div_ceil(2).min(128)
        } else {
            256
        };
        // merge_getmem never returns to temparray after its first allocation,
        // even when a subsequent merge would fit there (sort.c:604-635).
        if self.heap_temporary_allocated || source.len() > capacity {
            self.heap_temporary_allocated = true;
            return false;
        }
        // sortslice_memcpy overwrites only the new prefix (sort.c:651,776).
        // Reuse its roots and preserve the untouched tail from earlier merges;
        // consumed stack slots are not cleared as heap relocation slots are.
        for (index, item) in source.iter().enumerate() {
            if let Some(slot) = self.stack_temporary_roots.get(index) {
                runtime.set_sort_slot(slot, item.value);
            } else {
                self.stack_temporary_roots
                    .push(runtime.root_sort_slot(item.value));
            }
        }
        true
    }
    #[inline]
    fn put(&mut self, index: usize, item: SortItem) {
        assert!(self.vector.set_vector_slot(index, item.value));
        if let Some(keys) = &mut self.keys {
            keys[index] = item.key;
        }
    }
    fn reverse(&mut self, range: Range<usize>) {
        self.vector
            .with_vector_data_mut(|values| values[range.clone()].reverse())
            .unwrap();
        if let Some(keys) = &mut self.keys {
            keys[range].reverse();
        }
    }
    fn copy_within(&mut self, range: Range<usize>, dest: usize) {
        self.vector
            .with_vector_data_mut(|values| values.copy_within(range.clone(), dest))
            .unwrap();
        if let Some(keys) = &mut self.keys {
            keys.copy_within(range, dest);
        }
    }
    fn copy_from(&mut self, dest: usize, source: &[SortItem]) {
        self.vector
            .with_vector_data_mut(|values| {
                for (slot, item) in values[dest..dest + source.len()].iter_mut().zip(source) {
                    *slot = item.value;
                }
            })
            .unwrap();
        if let Some(keys) = &mut self.keys {
            for (slot, item) in keys[dest..dest + source.len()].iter_mut().zip(source) {
                *slot = item.key;
            }
        }
    }
}

#[derive(Clone, Copy)]
struct PendingRun {
    base: usize,
    len: usize,
    power: i32,
}

const GALLOP_WIN_MIN: usize = 7;

pub(super) fn gnu_style_sort_items(
    runtime: &mut impl SortRuntime,
    items: &mut (impl SortStorage + ?Sized),
    lessp_fn: SortPredicate,
) -> Result<(), Flow> {
    let len = items.len();
    if len < 2 {
        return Ok(());
    }

    let minrun = merge_compute_minrun(len);
    let mut pending: Vec<PendingRun> = Vec::new();
    let mut min_gallop = GALLOP_WIN_MIN;
    let mut base = 0;
    let mut remaining = len;

    while remaining > 0 {
        let (mut run_len, descending) = count_run(runtime, items, base, len, lessp_fn)?;
        if descending {
            items.reverse(base..base + run_len);
        }
        if run_len < minrun {
            let force = remaining.min(minrun);
            binarysort(runtime, items, base, base + force, base + run_len, lessp_fn)?;
            run_len = force;
        }

        found_new_run(
            runtime,
            items,
            &mut pending,
            run_len,
            len,
            lessp_fn,
            &mut min_gallop,
        )?;
        pending.push(PendingRun {
            base,
            len: run_len,
            power: 0,
        });

        base += run_len;
        remaining -= run_len;
    }

    merge_force_collapse(runtime, items, &mut pending, lessp_fn, &mut min_gallop)
}

fn sort_item_less(
    runtime: &mut impl SortRuntime,
    left: SortItem,
    right: SortItem,
    lessp_fn: SortPredicate,
) -> Result<bool, Flow> {
    if matches!(lessp_fn, SortPredicate::ValueLt) {
        return Ok(matches!(
            runtime.compare_sort_keys(&left.key, &right.key)?,
            std::cmp::Ordering::Less
        ));
    }

    Ok(runtime
        .call_sort_predicate(lessp_fn, left.key, right.key)?
        .is_truthy())
}

fn binarysort(
    runtime: &mut impl SortRuntime,
    items: &mut (impl SortStorage + ?Sized),
    lo: usize,
    hi: usize,
    mut start: usize,
    lessp_fn: SortPredicate,
) -> Result<(), Flow> {
    if lo == start {
        start += 1;
    }
    while start < hi {
        let pivot = items.item(start);
        let roots = items.needs_value_roots().then(|| runtime.save_sort_roots());
        items.root_insertion_key(runtime, pivot);
        let result = (|| {
            let mut left = lo;
            let mut right = start;
            while left < right {
                let mid = left + ((right - left) >> 1);
                if sort_item_less(runtime, pivot, items.item(mid), lessp_fn)? {
                    right = mid;
                } else {
                    left = mid + 1;
                }
            }
            let pivot = items.insertion_pivot(start, pivot);
            items.copy_within(left..start, left + 1);
            items.put(left, pivot);
            Ok(())
        })();
        if let Some(roots) = roots {
            runtime.restore_sort_roots(roots);
        }
        result?;
        start += 1;
    }
    Ok(())
}

fn count_run(
    runtime: &mut impl SortRuntime,
    items: &(impl SortStorage + ?Sized),
    lo: usize,
    hi: usize,
    lessp_fn: SortPredicate,
) -> Result<(usize, bool), Flow> {
    debug_assert!(lo < hi);
    if lo + 1 == hi {
        return Ok((1, false));
    }

    let mut run_len = 2;
    if sort_item_less(runtime, items.item(lo + 1), items.item(lo), lessp_fn)? {
        while lo + run_len < hi
            && sort_item_less(
                runtime,
                items.item(lo + run_len),
                items.item(lo + run_len - 1),
                lessp_fn,
            )?
        {
            run_len += 1;
        }
        Ok((run_len, true))
    } else {
        while lo + run_len < hi
            && !sort_item_less(
                runtime,
                items.item(lo + run_len),
                items.item(lo + run_len - 1),
                lessp_fn,
            )?
        {
            run_len += 1;
        }
        Ok((run_len, false))
    }
}

fn merge_compute_minrun(mut n: usize) -> usize {
    let mut r = 0;
    while n >= 64 {
        r |= n & 1;
        n >>= 1;
    }
    n + r
}

fn powerloop(s1: usize, n1: usize, n2: usize, n: usize) -> i32 {
    debug_assert!(n1 > 0 && n2 > 0);
    debug_assert!(s1 + n1 + n2 <= n);

    let mut a = 2 * s1 + n1;
    let mut b = a + n1 + n2;
    let mut result = 0;
    loop {
        result += 1;
        if a >= n {
            a -= n;
            b -= n;
        } else if b >= n {
            break;
        }
        a <<= 1;
        b <<= 1;
    }
    result
}

fn found_new_run(
    runtime: &mut impl SortRuntime,
    items: &mut (impl SortStorage + ?Sized),
    pending: &mut Vec<PendingRun>,
    new_len: usize,
    total_len: usize,
    lessp_fn: SortPredicate,
    min_gallop: &mut usize,
) -> Result<(), Flow> {
    if pending.is_empty() {
        return Ok(());
    }

    let prev = *pending.last().expect("pending run");
    let power = powerloop(prev.base, prev.len, new_len, total_len);
    while pending.len() > 1 && pending[pending.len() - 2].power > power {
        let index = pending.len() - 2;
        merge_at(runtime, items, pending, index, lessp_fn, min_gallop)?;
    }
    let last = pending.len() - 1;
    pending[last].power = power;
    Ok(())
}

fn merge_force_collapse(
    runtime: &mut impl SortRuntime,
    items: &mut (impl SortStorage + ?Sized),
    pending: &mut Vec<PendingRun>,
    lessp_fn: SortPredicate,
    min_gallop: &mut usize,
) -> Result<(), Flow> {
    while pending.len() > 1 {
        let mut index = pending.len() - 2;
        if index > 0 && pending[index - 1].len < pending[index + 1].len {
            index -= 1;
        }
        merge_at(runtime, items, pending, index, lessp_fn, min_gallop)?;
    }
    Ok(())
}

fn merge_at(
    runtime: &mut impl SortRuntime,
    items: &mut (impl SortStorage + ?Sized),
    pending: &mut Vec<PendingRun>,
    index: usize,
    lessp_fn: SortPredicate,
    min_gallop: &mut usize,
) -> Result<(), Flow> {
    let left = pending[index];
    let right = pending[index + 1];
    debug_assert_eq!(left.base + left.len, right.base);

    merge_runs(
        runtime, items, left.base, left.len, right.len, lessp_fn, min_gallop,
    )?;
    pending[index].len = left.len + right.len;
    pending.remove(index + 1);
    Ok(())
}

fn gallop_left(
    runtime: &mut impl SortRuntime,
    key: SortItem,
    items: &(impl SortStorage + ?Sized),
    hint: usize,
    lessp_fn: SortPredicate,
    retain_key: bool,
) -> Result<usize, Flow> {
    // GNU keeps the gallop key in a C local across comparator calls. A
    // callback can remove its original vector slot and collect in that call.
    let roots = retain_key.then(|| runtime.save_sort_roots());
    if retain_key {
        runtime.root_sort_slot(key.key);
    }
    let result = (|| {
        debug_assert!(!items.is_empty());
        debug_assert!(hint < items.len());

        let n = items.len() as isize;
        let hint = hint as isize;
        let mut last_offset = 0isize;
        let mut offset = 1isize;

        if sort_item_less(runtime, items.item(hint as usize), key, lessp_fn)? {
            let max_offset = n - hint;
            while offset < max_offset {
                if sort_item_less(runtime, items.item((hint + offset) as usize), key, lessp_fn)? {
                    last_offset = offset;
                    offset = (offset << 1) + 1;
                } else {
                    break;
                }
            }
            if offset > max_offset {
                offset = max_offset;
            }
            last_offset += hint;
            offset += hint;
        } else {
            let max_offset = hint + 1;
            while offset < max_offset {
                if sort_item_less(runtime, items.item((hint - offset) as usize), key, lessp_fn)? {
                    break;
                }
                last_offset = offset;
                offset = (offset << 1) + 1;
            }
            if offset > max_offset {
                offset = max_offset;
            }
            let k = last_offset;
            last_offset = hint - offset;
            offset = hint - k;
        }

        last_offset += 1;
        while last_offset < offset {
            let mid = last_offset + ((offset - last_offset) >> 1);
            if sort_item_less(runtime, items.item(mid as usize), key, lessp_fn)? {
                last_offset = mid + 1;
            } else {
                offset = mid;
            }
        }
        Ok(offset as usize)
    })();
    if let Some(roots) = roots {
        runtime.restore_sort_roots(roots);
    }
    result
}

fn gallop_right(
    runtime: &mut impl SortRuntime,
    key: SortItem,
    items: &(impl SortStorage + ?Sized),
    hint: usize,
    lessp_fn: SortPredicate,
    retain_key: bool,
) -> Result<usize, Flow> {
    // GNU keeps the gallop key in a C local across comparator calls. A
    // callback can remove its original vector slot and collect in that call.
    let roots = retain_key.then(|| runtime.save_sort_roots());
    if retain_key {
        runtime.root_sort_slot(key.key);
    }
    let result = (|| {
        debug_assert!(!items.is_empty());
        debug_assert!(hint < items.len());

        let n = items.len() as isize;
        let hint = hint as isize;
        let mut last_offset = 0isize;
        let mut offset = 1isize;

        if sort_item_less(runtime, key, items.item(hint as usize), lessp_fn)? {
            let max_offset = hint + 1;
            while offset < max_offset {
                if sort_item_less(runtime, key, items.item((hint - offset) as usize), lessp_fn)? {
                    last_offset = offset;
                    offset = (offset << 1) + 1;
                } else {
                    break;
                }
            }
            if offset > max_offset {
                offset = max_offset;
            }
            let k = last_offset;
            last_offset = hint - offset;
            offset = hint - k;
        } else {
            let max_offset = n - hint;
            while offset < max_offset {
                if sort_item_less(runtime, key, items.item((hint + offset) as usize), lessp_fn)? {
                    break;
                }
                last_offset = offset;
                offset = (offset << 1) + 1;
            }
            if offset > max_offset {
                offset = max_offset;
            }
            last_offset += hint;
            offset += hint;
        }

        last_offset += 1;
        while last_offset < offset {
            let mid = last_offset + ((offset - last_offset) >> 1);
            if sort_item_less(runtime, key, items.item(mid as usize), lessp_fn)? {
                offset = mid;
            } else {
                last_offset = mid + 1;
            }
        }
        Ok(offset as usize)
    })();
    if let Some(roots) = roots {
        runtime.restore_sort_roots(roots);
    }
    result
}

fn merge_runs(
    runtime: &mut impl SortRuntime,
    items: &mut (impl SortStorage + ?Sized),
    base: usize,
    left_len: usize,
    right_len: usize,
    lessp_fn: SortPredicate,
    min_gallop: &mut usize,
) -> Result<(), Flow> {
    let mut left_base = base;
    let mut left_len = left_len;
    let right_base = base + left_len;
    let mut right_len = right_len;

    let skipped = gallop_right(
        runtime,
        items.item(right_base),
        &SortRange::new(items, left_base..left_base + left_len),
        0,
        lessp_fn,
        items.needs_value_roots(),
    )?;
    left_base += skipped;
    left_len -= skipped;
    if left_len == 0 {
        return Ok(());
    }

    right_len = gallop_left(
        runtime,
        items.item(left_base + left_len - 1),
        &SortRange::new(items, right_base..right_base + right_len),
        right_len - 1,
        lessp_fn,
        items.needs_value_roots(),
    )?;
    if right_len == 0 {
        return Ok(());
    }

    if left_len <= right_len {
        merge_lo(
            runtime, items, left_base, left_len, right_base, right_len, lessp_fn, min_gallop,
        )
    } else {
        merge_hi(
            runtime, items, left_base, left_len, right_base, right_len, lessp_fn, min_gallop,
        )
    }
}

#[allow(clippy::too_many_arguments)] // TimSort merge state follows the reference algorithm directly
fn merge_lo(
    runtime: &mut impl SortRuntime,
    items: &mut (impl SortStorage + ?Sized),
    left_base: usize,
    mut left_len: usize,
    right_base: usize,
    mut right_len: usize,
    lessp_fn: SortPredicate,
    min_gallop: &mut usize,
) -> Result<(), Flow> {
    let left = items.snapshot(left_base..left_base + left_len);
    // Stack temporary roots belong to the whole sort; heap roots follow the
    // remaining relocation range marked by GNU merge_markmem (sort.c:538-543).
    let stack_temporary = items.retain_stack_temporary(runtime, &left);
    let scoped_value_roots = items.needs_value_roots() && !stack_temporary;
    let root_scope = scoped_value_roots.then(|| runtime.save_sort_roots());
    let temp_roots: Vec<_> = if scoped_value_roots {
        left.iter()
            .map(|item| runtime.root_sort_slot(item.value))
            .collect()
    } else {
        Vec::new()
    };
    let mut left_index = 0;
    let mut right_index = right_base;
    let mut dest = left_base;
    let result = (|| {
        items.put(dest, items.item(right_index));
        dest += 1;
        right_index += 1;
        right_len -= 1;
        if right_len == 0 {
            items.copy_from(dest, &left[left_index..left_index + left_len]);
            return Ok(());
        }
        if left_len == 1 {
            if right_len > 0 {
                items.copy_within(right_index..right_index + right_len, dest);
                dest += right_len;
            }
            items.put(dest, left[left_index]);
            return Ok(());
        }

        let mut threshold = *min_gallop;
        loop {
            let mut acount = 0;
            let mut bcount = 0;

            loop {
                if sort_item_less(runtime, items.item(right_index), left[left_index], lessp_fn)? {
                    items.put(dest, items.item(right_index));
                    dest += 1;
                    right_index += 1;
                    right_len -= 1;
                    bcount += 1;
                    acount = 0;
                    if right_len == 0 {
                        if left_len > 0 {
                            items.copy_from(dest, &left[left_index..left_index + left_len]);
                        }
                        return Ok(());
                    }
                    if bcount >= threshold {
                        break;
                    }
                } else {
                    items.put(dest, left[left_index]);
                    dest += 1;
                    if scoped_value_roots {
                        runtime.clear_sort_slot(&temp_roots[left_index]);
                    }
                    left_index += 1;
                    left_len -= 1;
                    acount += 1;
                    bcount = 0;
                    if left_len == 1 {
                        if right_len > 0 {
                            items.copy_within(right_index..right_index + right_len, dest);
                            dest += right_len;
                        }
                        items.put(dest, left[left_index]);
                        return Ok(());
                    }
                    if acount >= threshold {
                        break;
                    }
                }
            }

            threshold += 1;
            loop {
                if threshold > 1 {
                    threshold -= 1;
                }
                *min_gallop = threshold;

                let k = gallop_right(
                    runtime,
                    items.item(right_index),
                    &left[left_index..left_index + left_len],
                    0,
                    lessp_fn,
                    items.needs_value_roots(),
                )?;
                acount = k;
                if k != 0 {
                    items.copy_from(dest, &left[left_index..left_index + k]);
                    dest += k;
                    if scoped_value_roots {
                        for slot in &temp_roots[left_index..left_index + k] {
                            runtime.clear_sort_slot(slot);
                        }
                    }
                    left_index += k;
                    left_len -= k;
                    if left_len == 1 {
                        if right_len > 0 {
                            items.copy_within(right_index..right_index + right_len, dest);
                            dest += right_len;
                        }
                        items.put(dest, left[left_index]);
                        return Ok(());
                    }
                    if left_len == 0 {
                        return Ok(());
                    }
                }

                items.put(dest, items.item(right_index));
                dest += 1;
                right_index += 1;
                right_len -= 1;
                if right_len == 0 {
                    if left_len > 0 {
                        items.copy_from(dest, &left[left_index..left_index + left_len]);
                    }
                    return Ok(());
                }

                let k = gallop_left(
                    runtime,
                    left[left_index],
                    &SortRange::new(items, right_index..right_index + right_len),
                    0,
                    lessp_fn,
                    items.needs_value_roots(),
                )?;
                bcount = k;
                if k != 0 {
                    items.copy_within(right_index..right_index + k, dest);
                    dest += k;
                    right_index += k;
                    right_len -= k;
                    if right_len == 0 {
                        if left_len > 0 {
                            items.copy_from(dest, &left[left_index..left_index + left_len]);
                        }
                        return Ok(());
                    }
                }

                items.put(dest, left[left_index]);
                dest += 1;
                if scoped_value_roots {
                    runtime.clear_sort_slot(&temp_roots[left_index]);
                }
                left_index += 1;
                left_len -= 1;
                if left_len == 1 {
                    if right_len > 0 {
                        items.copy_within(right_index..right_index + right_len, dest);
                        dest += right_len;
                    }
                    items.put(dest, left[left_index]);
                    return Ok(());
                }

                if acount < GALLOP_WIN_MIN && bcount < GALLOP_WIN_MIN {
                    break;
                }
            }

            threshold += 1;
            *min_gallop = threshold;
        }
    })();
    if result.is_err() && items.has_merge_cleanup() {
        items.copy_from(dest, &left[left_index..left_index + left_len]);
    }
    if let Some(scope) = root_scope {
        runtime.restore_sort_roots(scope);
    }
    result
}

#[allow(clippy::too_many_arguments)] // TimSort merge state follows the reference algorithm directly
fn merge_hi(
    runtime: &mut impl SortRuntime,
    items: &mut (impl SortStorage + ?Sized),
    left_base: usize,
    mut left_len: usize,
    right_base: usize,
    mut right_len: usize,
    lessp_fn: SortPredicate,
    min_gallop: &mut usize,
) -> Result<(), Flow> {
    let right = items.snapshot(right_base..right_base + right_len);
    let stack_temporary = items.retain_stack_temporary(runtime, &right);
    let scoped_value_roots = items.needs_value_roots() && !stack_temporary;
    let root_scope = scoped_value_roots.then(|| runtime.save_sort_roots());
    let temp_roots: Vec<_> = if scoped_value_roots {
        right
            .iter()
            .map(|item| runtime.root_sort_slot(item.value))
            .collect()
    } else {
        Vec::new()
    };
    let mut dest = (right_base + right_len - 1) as isize;
    let mut left_index = (left_base + left_len - 1) as isize;
    let mut right_index = (right_len - 1) as isize;
    let result = (|| {
        items.put(dest as usize, items.item(left_index as usize));
        dest -= 1;
        left_index -= 1;
        left_len -= 1;
        if left_len == 0 {
            items.copy_from(left_base, &right[..right_len]);
            return Ok(());
        }
        if right_len == 1 {
            let dest_end = dest as usize;
            let dest_start = dest_end + 1 - left_len;
            let src_end = left_index as usize;
            let src_start = src_end + 1 - left_len;
            items.copy_within(src_start..src_start + left_len, dest_start);
            items.put(dest_start - 1, right[right_index as usize]);
            return Ok(());
        }

        let mut threshold = *min_gallop;
        loop {
            let mut acount = 0;
            let mut bcount = 0;

            loop {
                if sort_item_less(
                    runtime,
                    right[right_index as usize],
                    items.item(left_index as usize),
                    lessp_fn,
                )? {
                    items.put(dest as usize, items.item(left_index as usize));
                    dest -= 1;
                    left_index -= 1;
                    left_len -= 1;
                    acount += 1;
                    bcount = 0;
                    if left_len == 0 {
                        if right_len > 0 {
                            let dest_end = dest as usize;
                            let dest_start = dest_end + 1 - right_len;
                            let right_start = right_index as usize + 1 - right_len;
                            items.copy_from(
                                dest_start,
                                &right[right_start..right_start + right_len],
                            );
                        }
                        return Ok(());
                    }
                    if acount >= threshold {
                        break;
                    }
                } else {
                    items.put(dest as usize, right[right_index as usize]);
                    dest -= 1;
                    right_index -= 1;
                    right_len -= 1;
                    if scoped_value_roots {
                        runtime.clear_sort_slot(&temp_roots[right_len]);
                    }
                    bcount += 1;
                    acount = 0;
                    if right_len == 1 {
                        let dest_end = dest as usize;
                        let dest_start = dest_end + 1 - left_len;
                        let src_end = left_index as usize;
                        let src_start = src_end + 1 - left_len;
                        items.copy_within(src_start..src_start + left_len, dest_start);
                        items.put(dest_start - 1, right[right_index as usize]);
                        return Ok(());
                    }
                    if bcount >= threshold {
                        break;
                    }
                }
            }

            threshold += 1;
            loop {
                if threshold > 1 {
                    threshold -= 1;
                }
                *min_gallop = threshold;

                let k = left_len
                    - gallop_right(
                        runtime,
                        right[right_index as usize],
                        &SortRange::new(items, left_base..left_base + left_len),
                        left_len - 1,
                        lessp_fn,
                        items.needs_value_roots(),
                    )?;
                acount = k;
                if k != 0 {
                    let dest_start = dest as usize + 1 - k;
                    let src_start = left_index as usize + 1 - k;
                    items.copy_within(src_start..src_start + k, dest_start);
                    dest -= k as isize;
                    left_index -= k as isize;
                    left_len -= k;
                    if left_len == 0 {
                        if right_len > 0 {
                            let dest_end = dest as usize;
                            let dest_start = dest_end + 1 - right_len;
                            let right_start = right_index as usize + 1 - right_len;
                            items.copy_from(
                                dest_start,
                                &right[right_start..right_start + right_len],
                            );
                        }
                        return Ok(());
                    }
                }

                items.put(dest as usize, right[right_index as usize]);
                dest -= 1;
                right_index -= 1;
                right_len -= 1;
                if scoped_value_roots {
                    runtime.clear_sort_slot(&temp_roots[right_len]);
                }
                if right_len == 1 {
                    let dest_end = dest as usize;
                    let dest_start = dest_end + 1 - left_len;
                    let src_end = left_index as usize;
                    let src_start = src_end + 1 - left_len;
                    items.copy_within(src_start..src_start + left_len, dest_start);
                    items.put(dest_start - 1, right[right_index as usize]);
                    return Ok(());
                }

                let k = right_len
                    - gallop_left(
                        runtime,
                        items.item(left_index as usize),
                        &right[..right_len],
                        right_len - 1,
                        lessp_fn,
                        items.needs_value_roots(),
                    )?;
                bcount = k;
                if k != 0 {
                    let dest_start = dest as usize + 1 - k;
                    let right_start = right_index as usize + 1 - k;
                    items.copy_from(dest_start, &right[right_start..right_start + k]);
                    dest -= k as isize;
                    right_index -= k as isize;
                    if scoped_value_roots {
                        for slot in &temp_roots[right_len - k..right_len] {
                            runtime.clear_sort_slot(slot);
                        }
                    }
                    right_len -= k;
                    if right_len == 1 {
                        let dest_end = dest as usize;
                        let dest_start = dest_end + 1 - left_len;
                        let src_end = left_index as usize;
                        let src_start = src_end + 1 - left_len;
                        items.copy_within(src_start..src_start + left_len, dest_start);
                        items.put(dest_start - 1, right[right_index as usize]);
                        return Ok(());
                    }
                    if right_len == 0 {
                        return Ok(());
                    }
                }

                items.put(dest as usize, items.item(left_index as usize));
                dest -= 1;
                left_index -= 1;
                left_len -= 1;
                if left_len == 0 {
                    if right_len > 0 {
                        let dest_end = dest as usize;
                        let dest_start = dest_end + 1 - right_len;
                        let right_start = right_index as usize + 1 - right_len;
                        items.copy_from(dest_start, &right[right_start..right_start + right_len]);
                    }
                    return Ok(());
                }

                if acount < GALLOP_WIN_MIN && bcount < GALLOP_WIN_MIN {
                    break;
                }
            }

            threshold += 1;
            *min_gallop = threshold;
        }
    })();
    if result.is_err() && right_len > 0 && items.has_merge_cleanup() {
        items.copy_from(dest as usize + 1 - right_len, &right[..right_len]);
    }
    if let Some(scope) = root_scope {
        runtime.restore_sort_roots(scope);
    }
    result
}
