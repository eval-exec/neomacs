//! Retire both matching syntax roots only after all matching slots are loaned.
//! A foreign component is preserved, even if it has a live local view.

use super::{
    STANDARD_SYNTAX_TABLE_HEAP, STANDARD_SYNTAX_TABLE_OBJECT, SYNTAX_CODE_OBJECTS,
    SYNTAX_CODE_OBJECTS_HEAP,
};
use neovm_value::heap_registry::text_retirement::{
    PreparedValueCacheRetirement, TextCacheRetireError, TextCacheRetirement, TextCacheSlot,
};
use neovm_value::tagged::gc::HeapIdentity;

#[cold]
#[doc(hidden)]
pub fn retire_syntax_for_heap(
    heap: HeapIdentity,
) -> Result<TextCacheRetirement, TextCacheRetireError> {
    STANDARD_SYNTAX_TABLE_OBJECT.with(|table| {
        STANDARD_SYNTAX_TABLE_HEAP.with(|table_owner| {
            SYNTAX_CODE_OBJECTS.with(|codes| {
                SYNTAX_CODE_OBJECTS_HEAP.with(|codes_owner| {
                    let table = PreparedValueCacheRetirement::prepare(
                        table,
                        table_owner,
                        heap,
                        TextCacheSlot::SyntaxTable,
                    )?;
                    let codes = PreparedValueCacheRetirement::prepare(
                        codes,
                        codes_owner,
                        heap,
                        TextCacheSlot::SyntaxCodes,
                    )?;
                    let table = table.commit();
                    let codes = codes.commit();
                    Ok(match table {
                        TextCacheRetirement::Retired(_) => table,
                        TextCacheRetirement::AlreadyInactive(_) => codes,
                    })
                })
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::super::{
        STANDARD_SYNTAX_TABLE_HEAP, STANDARD_SYNTAX_TABLE_OBJECT, SYNTAX_CODE_OBJECTS,
        SYNTAX_CODE_OBJECTS_HEAP, collect_syntax_gc_roots,
    };
    use super::*;
    use neovm_value::value::Value;
    use std::cell::Cell;

    fn heap(word: usize) -> HeapIdentity {
        HeapIdentity::from_legacy_word(word).expect("non-zero heap word")
    }

    fn install(table_word: usize, codes_word: usize) {
        STANDARD_SYNTAX_TABLE_OBJECT.with(|slot| *slot.borrow_mut() = Some(Value::fixnum(47)));
        STANDARD_SYNTAX_TABLE_HEAP.with(|owner| owner.set(table_word));
        SYNTAX_CODE_OBJECTS.with(|slot| *slot.borrow_mut() = Some(Value::fixnum(48)));
        SYNTAX_CODE_OBJECTS_HEAP.with(|owner| owner.set(codes_word));
    }

    #[test]
    fn pentry_e2b_text_retire_syntax_matching_roots_are_removed_together() {
        install(11, 11);
        assert_eq!(
            retire_syntax_for_heap(heap(11)),
            Ok(TextCacheRetirement::Retired(heap(11)))
        );
        assert!(STANDARD_SYNTAX_TABLE_OBJECT.with(|slot| slot.borrow().is_none()));
        assert!(SYNTAX_CODE_OBJECTS.with(|slot| slot.borrow().is_none()));
        assert_eq!(STANDARD_SYNTAX_TABLE_HEAP.with(Cell::get), 0);
        assert_eq!(SYNTAX_CODE_OBJECTS_HEAP.with(Cell::get), 0);
        let mut roots = Vec::new();
        collect_syntax_gc_roots(&mut roots, 11);
        assert!(roots.is_empty());
    }

    #[test]
    fn pentry_e2b_text_retire_syntax_codes_loan_does_not_commit_table() {
        install(11, 11);
        SYNTAX_CODE_OBJECTS.with(|codes| {
            let view = codes.borrow();
            assert_eq!(
                retire_syntax_for_heap(heap(11)),
                Err(TextCacheRetireError::Borrowed(TextCacheSlot::SyntaxCodes))
            );
            assert_eq!(*view, Some(Value::fixnum(48)));
        });
        assert_eq!(
            STANDARD_SYNTAX_TABLE_OBJECT.with(|slot| *slot.borrow()),
            Some(Value::fixnum(47))
        );
        assert_eq!(STANDARD_SYNTAX_TABLE_HEAP.with(Cell::get), 11);
        assert_eq!(SYNTAX_CODE_OBJECTS_HEAP.with(Cell::get), 11);
    }

    #[test]
    fn pentry_e2b_text_retire_syntax_preserves_foreign_table() {
        install(22, 11);
        assert_eq!(
            retire_syntax_for_heap(heap(11)),
            Ok(TextCacheRetirement::Retired(heap(11)))
        );
        assert_eq!(
            STANDARD_SYNTAX_TABLE_OBJECT.with(|slot| *slot.borrow()),
            Some(Value::fixnum(47))
        );
        assert_eq!(STANDARD_SYNTAX_TABLE_HEAP.with(Cell::get), 22);
        assert!(SYNTAX_CODE_OBJECTS.with(|slot| slot.borrow().is_none()));
        assert_eq!(SYNTAX_CODE_OBJECTS_HEAP.with(Cell::get), 0);
    }
}
