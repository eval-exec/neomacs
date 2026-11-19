//! Eval orders disposable text-cache retirement before the final source
//! census. Each text crate owns its TLS. Value does not depend on text.

use neovm_value::heap_registry::text_retirement::TextCacheRetirement;
use neovm_value::tagged::gc::HeapIdentity;

#[cold]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "E2 L3 dormant text-cache retirement order; no production caller enters it"
    )
)]
pub(crate) fn discard_disposable_text_caches(heap: HeapIdentity) -> TextCacheRetirement {
    let _ = neovm_text::emacs_core::casetab::retirement::retire_casetab_for_heap(heap);
    let _ = neovm_text::emacs_core::syntax::retirement::retire_syntax_for_heap(heap);
    let _ = neovm_text::emacs_core::category::retirement::retire_category_for_heap(heap);
    let _ = neovm_text::emacs_core::regex::retirement::retire_regex_for_heap(heap);
    neovm_value::string_pos_cache::retire_string_pos_cache_for_heap(heap)
}

#[cfg(test)]
#[path = "tests/text_retirement_test.rs"]
mod tests;
