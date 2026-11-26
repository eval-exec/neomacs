//! Discard the matching regexp artifacts and retained table identities. Loan
//! every cache before changing any; destroy artifacts outside all slot guards.

use super::{
    LISP_REGEX_PATTERN_CACHE, LITERAL_TRT_CACHE, REGEX_CACHE_COLLECTION_EPOCH, REGEX_CACHE_HEAP,
    SEARCH_PATTERN_CACHE,
};
use neovm_value::heap_registry::text_retirement::{
    TextCacheRetireError, TextCacheRetirement, TextCacheSlot, heap_identity_word,
};
use neovm_value::tagged::gc::HeapIdentity;
use std::cell::Cell;

#[cold]
#[doc(hidden)]
pub fn retire_regex_for_heap(
    heap: HeapIdentity,
) -> Result<TextCacheRetirement, TextCacheRetireError> {
    if REGEX_CACHE_HEAP.with(Cell::get) != heap_identity_word(heap) {
        return Ok(TextCacheRetirement::AlreadyInactive(heap));
    }
    let retired = SEARCH_PATTERN_CACHE.with(|search| {
        LISP_REGEX_PATTERN_CACHE.with(|lisp| {
            LITERAL_TRT_CACHE.with(|literal| {
                let mut search = search
                    .try_borrow_mut()
                    .map_err(|_| TextCacheRetireError::Borrowed(TextCacheSlot::RegexSearch))?;
                let mut lisp = lisp
                    .try_borrow_mut()
                    .map_err(|_| TextCacheRetireError::Borrowed(TextCacheSlot::RegexLisp))?;
                let mut literal = literal
                    .try_borrow_mut()
                    .map_err(|_| TextCacheRetireError::Borrowed(TextCacheSlot::RegexLiteral))?;
                let retired = (
                    std::mem::take(&mut *search),
                    std::mem::take(&mut *lisp),
                    literal.take(),
                );
                REGEX_CACHE_HEAP.with(|owner| owner.set(0));
                REGEX_CACHE_COLLECTION_EPOCH.with(|epoch| epoch.set(None));
                drop(literal);
                drop(lisp);
                drop(search);
                Ok(retired)
            })
        })
    })?;
    drop(retired);
    Ok(TextCacheRetirement::Retired(heap))
}

#[cfg(test)]
mod tests {
    use super::super::{
        CaseTranslation, DefaultSyntaxLookup, LISP_REGEX_PATTERN_CACHE, LITERAL_TRT_CACHE,
        REGEX_CACHE_COLLECTION_EPOCH, REGEX_CACHE_HEAP, SEARCH_PATTERN_CACHE,
        compile_lisp_pattern_with_posix_translation, compile_search_pattern_with_posix,
    };
    use super::*;
    use neovm_value::heap_types::LispString;
    use neovm_value::value::Value;
    use std::rc::Rc;

    fn counts() -> (usize, usize, usize) {
        (
            SEARCH_PATTERN_CACHE.with(|slot| slot.borrow().len()),
            LISP_REGEX_PATTERN_CACHE.with(|slot| slot.borrow().len()),
            LITERAL_TRT_CACHE.with(|slot| usize::from(slot.borrow().is_some())),
        )
    }

    fn heap() -> HeapIdentity {
        crate::test_utils::init_test_tracing();
        let word = neovm_value::tagged::gc::current_tagged_heap_identity()
            .expect("text tests install a fallback heap");
        HeapIdentity::from_legacy_word(word).expect("fallback heap word")
    }

    fn populate() -> (HeapIdentity, Value) {
        crate::test_utils::init_test_tracing();
        let table = neovm_value::chartab::make_char_table_value(Value::NIL, Value::NIL);
        let pattern = LispString::from_utf8("pentry-cache-retirement");
        compile_lisp_pattern_with_posix_translation(
            &pattern,
            true,
            false,
            true,
            Some(table),
            &DefaultSyntaxLookup,
        )
        .expect("lisp pattern");
        compile_search_pattern_with_posix(&pattern, false, false, &DefaultSyntaxLookup)
            .expect("search pattern");
        LITERAL_TRT_CACHE.with(|slot| {
            *slot.borrow_mut() = Some((
                table.bits(),
                Rc::new(CaseTranslation::from_char_table(table)),
            ));
        });
        let heap = heap();
        REGEX_CACHE_HEAP.with(|owner| owner.set(heap_identity_word(heap)));
        REGEX_CACHE_COLLECTION_EPOCH.with(|epoch| epoch.set(Some(3)));
        assert_eq!(counts(), (1, 1, 1));
        (heap, table)
    }

    #[test]
    fn pentry_e2b_text_retire_regex_discards_all_artifacts_and_roots() {
        let (heap, _) = populate();
        assert_eq!(
            retire_regex_for_heap(heap),
            Ok(TextCacheRetirement::Retired(heap))
        );
        assert_eq!(counts(), (0, 0, 0));
        assert_eq!(REGEX_CACHE_HEAP.with(Cell::get), 0);
        assert_eq!(REGEX_CACHE_COLLECTION_EPOCH.with(Cell::get), None);
    }

    #[test]
    fn pentry_e2b_text_retire_regex_search_loan_refuses_without_changes() {
        let (heap, _) = populate();
        SEARCH_PATTERN_CACHE.with(|slot| {
            let _view = slot.borrow();
            assert_eq!(
                retire_regex_for_heap(heap),
                Err(TextCacheRetireError::Borrowed(TextCacheSlot::RegexSearch))
            );
        });
        assert_eq!(counts(), (1, 1, 1));
        assert_eq!(REGEX_CACHE_HEAP.with(Cell::get), heap_identity_word(heap));
    }

    #[test]
    fn pentry_e2b_text_retire_regex_lisp_loan_does_not_commit_search() {
        let (heap, _) = populate();
        LISP_REGEX_PATTERN_CACHE.with(|slot| {
            let _view = slot.borrow_mut();
            assert_eq!(
                retire_regex_for_heap(heap),
                Err(TextCacheRetireError::Borrowed(TextCacheSlot::RegexLisp))
            );
        });
        assert_eq!(counts(), (1, 1, 1));
        assert_eq!(REGEX_CACHE_HEAP.with(Cell::get), heap_identity_word(heap));
    }

    #[test]
    fn pentry_e2b_text_retire_regex_literal_loan_does_not_commit_other_caches() {
        let (heap, _) = populate();
        LITERAL_TRT_CACHE.with(|slot| {
            let _view = slot.borrow();
            assert_eq!(
                retire_regex_for_heap(heap),
                Err(TextCacheRetireError::Borrowed(TextCacheSlot::RegexLiteral))
            );
        });
        assert_eq!(counts(), (1, 1, 1));
        assert_eq!(REGEX_CACHE_HEAP.with(Cell::get), heap_identity_word(heap));
    }

    #[test]
    fn pentry_e2b_text_retire_regex_foreign_preserved_even_with_live_loan() {
        let (installed, _) = populate();
        let foreign = HeapIdentity::from_legacy_word(heap_identity_word(installed).wrapping_add(1))
            .expect("foreign word");
        LISP_REGEX_PATTERN_CACHE.with(|slot| {
            let _view = slot.borrow_mut();
            assert_eq!(
                retire_regex_for_heap(foreign),
                Ok(TextCacheRetirement::AlreadyInactive(foreign))
            );
        });
        assert_eq!(counts(), (1, 1, 1));
        assert_eq!(
            REGEX_CACHE_HEAP.with(Cell::get),
            heap_identity_word(installed)
        );
    }
}
