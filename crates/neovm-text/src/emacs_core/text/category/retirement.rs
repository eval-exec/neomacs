//! Matching-heap canonical table cache retirement. Reactivation uses the
//! returning Context, never a saved unrooted cache entry.

use super::{STANDARD_CATEGORY_TABLE_HEAP, STANDARD_CATEGORY_TABLE_OBJECT};
use neovm_value::heap_registry::text_retirement::{
    PreparedValueCacheRetirement, TextCacheRetireError, TextCacheRetirement, TextCacheSlot,
};
use neovm_value::tagged::gc::HeapIdentity;

#[cold]
#[doc(hidden)]
pub fn retire_category_for_heap(
    heap: HeapIdentity,
) -> Result<TextCacheRetirement, TextCacheRetireError> {
    STANDARD_CATEGORY_TABLE_OBJECT.with(|value| {
        STANDARD_CATEGORY_TABLE_HEAP.with(|owner| {
            PreparedValueCacheRetirement::prepare(value, owner, heap, TextCacheSlot::CategoryTable)
                .map(PreparedValueCacheRetirement::commit)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::super::{
        STANDARD_CATEGORY_TABLE_HEAP, STANDARD_CATEGORY_TABLE_OBJECT, collect_category_gc_roots,
    };
    use super::*;
    use neovm_value::value::Value;
    use std::cell::Cell;

    fn heap(word: usize) -> HeapIdentity {
        HeapIdentity::from_legacy_word(word).expect("non-zero heap word")
    }

    fn install(word: usize, table: Value) {
        STANDARD_CATEGORY_TABLE_OBJECT.with(|slot| *slot.borrow_mut() = Some(table));
        STANDARD_CATEGORY_TABLE_HEAP.with(|owner| owner.set(word));
    }

    #[test]
    fn pentry_e2b_text_retire_category_matching_root_is_removed() {
        install(11, Value::fixnum(47));
        assert_eq!(
            retire_category_for_heap(heap(11)),
            Ok(TextCacheRetirement::Retired(heap(11)))
        );
        assert!(STANDARD_CATEGORY_TABLE_OBJECT.with(|slot| slot.borrow().is_none()));
        assert_eq!(STANDARD_CATEGORY_TABLE_HEAP.with(Cell::get), 0);
        let mut roots = Vec::new();
        collect_category_gc_roots(&mut roots, 11);
        assert!(roots.is_empty());
    }

    #[test]
    fn pentry_e2b_text_retire_category_preserves_foreign_even_with_live_view() {
        install(22, Value::fixnum(9));
        STANDARD_CATEGORY_TABLE_OBJECT.with(|slot| {
            let view = slot.borrow_mut();
            assert_eq!(
                retire_category_for_heap(heap(11)),
                Ok(TextCacheRetirement::AlreadyInactive(heap(11)))
            );
            assert_eq!(*view, Some(Value::fixnum(9)));
        });
        assert_eq!(STANDARD_CATEGORY_TABLE_HEAP.with(Cell::get), 22);
    }

    #[test]
    fn pentry_e2b_text_retire_category_reader_refuses_without_change() {
        install(11, Value::fixnum(47));
        STANDARD_CATEGORY_TABLE_OBJECT.with(|slot| {
            let view = slot.borrow();
            assert_eq!(
                retire_category_for_heap(heap(11)),
                Err(TextCacheRetireError::Borrowed(TextCacheSlot::CategoryTable))
            );
            assert_eq!(*view, Some(Value::fixnum(47)));
        });
        assert_eq!(STANDARD_CATEGORY_TABLE_HEAP.with(Cell::get), 11);
    }

    #[test]
    fn pentry_e2b_text_retire_category_writer_refuses_without_change() {
        install(11, Value::fixnum(47));
        STANDARD_CATEGORY_TABLE_OBJECT.with(|slot| {
            let view = slot.borrow_mut();
            assert_eq!(
                retire_category_for_heap(heap(11)),
                Err(TextCacheRetireError::Borrowed(TextCacheSlot::CategoryTable))
            );
            assert_eq!(*view, Some(Value::fixnum(47)));
        });
        assert_eq!(STANDARD_CATEGORY_TABLE_HEAP.with(Cell::get), 11);
    }
}
