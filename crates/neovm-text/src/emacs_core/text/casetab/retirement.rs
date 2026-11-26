//! Matching-heap canonical table cache retirement. Reactivation uses the
//! returning Context, never a saved unrooted cache entry.

use super::{STANDARD_CASE_TABLE_HEAP, STANDARD_CASE_TABLE_OBJECT};
use neovm_value::heap_registry::text_retirement::{
    PreparedValueCacheRetirement, TextCacheRetireError, TextCacheRetirement, TextCacheSlot,
};
use neovm_value::tagged::gc::HeapIdentity;

#[cold]
#[doc(hidden)]
pub fn retire_casetab_for_heap(
    heap: HeapIdentity,
) -> Result<TextCacheRetirement, TextCacheRetireError> {
    STANDARD_CASE_TABLE_OBJECT.with(|value| {
        STANDARD_CASE_TABLE_HEAP.with(|owner| {
            PreparedValueCacheRetirement::prepare(value, owner, heap, TextCacheSlot::CaseTable)
                .map(PreparedValueCacheRetirement::commit)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::super::{
        STANDARD_CASE_TABLE_HEAP, STANDARD_CASE_TABLE_OBJECT, collect_casetab_gc_roots,
    };
    use super::*;
    use neovm_value::value::Value;
    use std::cell::Cell;

    fn heap(word: usize) -> HeapIdentity {
        HeapIdentity::from_legacy_word(word).expect("non-zero heap word")
    }

    fn install(word: usize, table: Value) {
        STANDARD_CASE_TABLE_OBJECT.with(|slot| *slot.borrow_mut() = Some(table));
        STANDARD_CASE_TABLE_HEAP.with(|owner| owner.set(word));
    }

    #[test]
    fn pentry_e2b_text_retire_casetab_matching_root_is_removed() {
        let table = Value::fixnum(47);
        install(11, table);
        assert_eq!(
            retire_casetab_for_heap(heap(11)),
            Ok(TextCacheRetirement::Retired(heap(11)))
        );
        assert!(STANDARD_CASE_TABLE_OBJECT.with(|slot| slot.borrow().is_none()));
        assert_eq!(STANDARD_CASE_TABLE_HEAP.with(Cell::get), 0);
        let mut roots = Vec::new();
        collect_casetab_gc_roots(&mut roots, 11);
        assert!(roots.is_empty());
    }

    #[test]
    fn pentry_e2b_text_retire_casetab_preserves_foreign_even_with_live_view() {
        install(22, Value::fixnum(9));
        STANDARD_CASE_TABLE_OBJECT.with(|slot| {
            let view = slot.borrow_mut();
            assert_eq!(
                retire_casetab_for_heap(heap(11)),
                Ok(TextCacheRetirement::AlreadyInactive(heap(11)))
            );
            assert_eq!(*view, Some(Value::fixnum(9)));
        });
        assert_eq!(STANDARD_CASE_TABLE_HEAP.with(Cell::get), 22);
    }

    #[test]
    fn pentry_e2b_text_retire_casetab_reader_refuses_without_change() {
        install(11, Value::fixnum(47));
        STANDARD_CASE_TABLE_OBJECT.with(|slot| {
            let view = slot.borrow();
            assert_eq!(
                retire_casetab_for_heap(heap(11)),
                Err(TextCacheRetireError::Borrowed(TextCacheSlot::CaseTable))
            );
            assert_eq!(*view, Some(Value::fixnum(47)));
        });
        assert_eq!(STANDARD_CASE_TABLE_HEAP.with(Cell::get), 11);
    }

    #[test]
    fn pentry_e2b_text_retire_casetab_writer_refuses_without_change() {
        install(11, Value::fixnum(47));
        STANDARD_CASE_TABLE_OBJECT.with(|slot| {
            let view = slot.borrow_mut();
            assert_eq!(
                retire_casetab_for_heap(heap(11)),
                Err(TextCacheRetireError::Borrowed(TextCacheSlot::CaseTable))
            );
            assert_eq!(*view, Some(Value::fixnum(47)));
        });
        assert_eq!(STANDARD_CASE_TABLE_HEAP.with(Cell::get), 11);
    }

    #[test]
    fn pentry_e2b_text_retire_casetab_uncommitted_loan_does_not_retire() {
        install(11, Value::fixnum(47));
        STANDARD_CASE_TABLE_OBJECT.with(|slot| {
            STANDARD_CASE_TABLE_HEAP.with(|owner| {
                let prepared = PreparedValueCacheRetirement::prepare(
                    slot,
                    owner,
                    heap(11),
                    TextCacheSlot::CaseTable,
                )
                .unwrap();
                drop(prepared);
                assert_eq!(*slot.borrow(), Some(Value::fixnum(47)));
                assert_eq!(owner.get(), 11);
            })
        });
    }

    #[test]
    fn pentry_e2b_text_retire_casetab_repeat_has_no_restoration_effect() {
        install(11, Value::fixnum(47));
        let retired = retire_casetab_for_heap(heap(11)).unwrap();
        assert_eq!(
            retire_casetab_for_heap(heap(11)),
            Ok(TextCacheRetirement::AlreadyInactive(heap(11)))
        );
        let _ = retired;
        assert!(STANDARD_CASE_TABLE_OBJECT.with(|slot| slot.borrow().is_none()));
        assert_eq!(STANDARD_CASE_TABLE_HEAP.with(Cell::get), 0);
    }
}

/// The composition tests hold this TLS loan. No production callback and no
/// new slot. Retirement must refuse it.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn with_source_retirement_reader_for_test<R>(body: impl FnOnce() -> R) -> R {
    STANDARD_CASE_TABLE_OBJECT.with(|slot| {
        let _reader = slot.borrow();
        body()
    })
}
