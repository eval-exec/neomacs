use super::discard_disposable_text_caches;
use crate::emacs_core::eval::Context;
use neovm_value::heap_registry::text_retirement::{TextCacheRetirement, heap_identity_word};
use neovm_value::value::Value;

#[test]
fn pentry_e2b_text_retire_context_canonical_tables_survive_collection_and_reactivation() {
    let mut source = Context::new_minimal_vm_harness(crate::test_utils::unit_harness());
    let case = neovm_value::chartab::make_char_table_value(Value::NIL, Value::NIL);
    let syntax = neovm_value::chartab::make_char_table_value(Value::NIL, Value::NIL);
    let codes = neovm_value::chartab::make_char_table_value(Value::NIL, Value::NIL);
    let category = neovm_value::chartab::make_char_table_value(Value::NIL, Value::NIL);
    source.cached_standard_case_table = Some(case);
    source.standard_syntax_table = syntax;
    source.syntax_code_objects = codes;
    source.standard_category_table = category;
    source.setup_thread_locals();
    let heap = source.tagged_heap.heap_identity();
    let word = heap_identity_word(heap);
    let canonical = [case, syntax, codes, category];
    assert!(canonical.iter().all(|value| value.is_heap_object()));
    assert_eq!(
        neovm_text::emacs_core::casetab::retirement::retire_casetab_for_heap(heap),
        Ok(TextCacheRetirement::Retired(heap))
    );
    assert_eq!(
        neovm_text::emacs_core::syntax::retirement::retire_syntax_for_heap(heap),
        Ok(TextCacheRetirement::Retired(heap))
    );
    assert_eq!(
        neovm_text::emacs_core::category::retirement::retire_category_for_heap(heap),
        Ok(TextCacheRetirement::Retired(heap))
    );
    let mut roots = Vec::new();
    neovm_text::emacs_core::casetab::collect_casetab_gc_roots(&mut roots, word);
    neovm_text::emacs_core::syntax::collect_syntax_gc_roots(&mut roots, word);
    neovm_text::emacs_core::category::collect_category_gc_roots(&mut roots, word);
    assert!(roots.is_empty(), "no standard table remains in TLS");
    source.gc_collect_exact();
    for value in canonical {
        assert!(
            source.tagged_heap.owns_heap_value_for_test(value),
            "canonical Context root lost after TLS retirement"
        );
    }
    source.setup_thread_locals();
    neovm_text::emacs_core::casetab::collect_casetab_gc_roots(&mut roots, word);
    neovm_text::emacs_core::syntax::collect_syntax_gc_roots(&mut roots, word);
    neovm_text::emacs_core::category::collect_category_gc_roots(&mut roots, word);
    for value in canonical {
        assert!(
            roots.contains(&value),
            "reactivation changed canonical table identity"
        );
    }
    let _ = discard_disposable_text_caches(heap);
}
