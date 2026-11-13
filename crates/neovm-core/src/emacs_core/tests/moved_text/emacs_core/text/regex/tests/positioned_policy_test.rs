use super::*;

use neovm_edit::buffer::{
    Buffer, BufferId, CharPos0, CharRange, EmacsBytePos, EmacsByteRange, LispCharPos1,
};
use neovm_text::emacs_core::casefiddle::apply_replace_match_case;
use neovm_text::emacs_core::casefiddle::{
    apply_replace_match_case_lisp, apply_replace_match_case_lisp_with,
};
use neovm_text::emacs_core::syntax::SyntaxClass;
use neovm_value::emacs_char::char_to_byte_pos;
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicUsize, Ordering};

use neovm_eval::emacs_core::eval::Context;
use neovm_text::emacs_core::regex::*;
use neovm_text::emacs_core::regex_emacs;
use neovm_text::emacs_core::regex_emacs::DefaultSyntaxLookup;
use neovm_text::emacs_core::regex_emacs::SyntaxCacheKey;
use neovm_text::emacs_core::regex_emacs::SyntaxLookup;
use neovm_value::heap_types::LispString;
use neovm_value::tagged::value::SymbolMode;
use std::rc::Rc;

#[test]
fn positioned_compiled_regex_reuses_its_policy_and_keeps_distinct_match_results() {
    let mut context = crate::test_utils::new_context();
    // SAFETY: test-only owner-thread heap setup through the non-swappable mutable accessor contract; the view is used in place for allocation/policy and is never swapped or moved across Worlds.
    let purpose = unsafe { context.tagged_heap_mut() }.alloc_symbol_with_pos(
        neovm_value::value::Value::symbol("char-code-property-table"),
        neovm_value::value::Value::fixnum(0),
    );
    let table = neovm_text::emacs_core::chartable::make_char_table_with_extra_slots(
        purpose,
        neovm_value::value::Value::NIL,
        5,
    );
    // The compressed leaf translates U+0100 to character 2 only when the
    // positioned purpose is honored (GNU chartab.c UNIPROP_TABLE_P).
    let compressed =
        neovm_value::value::Value::heap_string(LispString::from_unibyte(vec![2, 2, 0xC2, 0x83, 1]));
    let mut depth2_contents = vec![neovm_value::value::Value::NIL; 32];
    depth2_contents[2] = compressed;
    let depth2 = neovm_value::value::Value::make_sub_char_table(2, 0, depth2_contents);
    let mut depth1_contents = vec![neovm_value::value::Value::NIL; 16];
    depth1_contents[0] = depth2;
    let depth1 = neovm_value::value::Value::make_sub_char_table(1, 0, depth1_contents);
    table
        .with_char_table_mut(|obj| obj.set_contents(0, depth1))
        .unwrap();

    let pattern = LispString::from_utf8("\u{100}");
    let compile = |mode| {
        compile_lisp_pattern_with_posix_translation(
            &pattern,
            true,
            false,
            true,
            Some(table),
            &PolicySyntax(mode),
        )
        .unwrap()
    };
    let bare = compile(SymbolMode::Bare);
    let positioned = compile(SymbolMode::Positioned);
    assert!(!Rc::ptr_eq(&bare, &positioned));
    assert!(Rc::ptr_eq(&positioned, &compile(SymbolMode::Positioned)));
    assert!(Rc::ptr_eq(&bare, &compile(SymbolMode::Bare)));

    let target = b"\x02";
    assert!(
        regex_emacs::re_match(
            &bare,
            target,
            0,
            target.len(),
            &PolicySyntax(SymbolMode::Bare),
            0
        )
        .is_none()
    );
    assert!(
        regex_emacs::re_match(
            &positioned,
            target,
            0,
            target.len(),
            &PolicySyntax(SymbolMode::Positioned),
            0
        )
        .is_some()
    );
}

#[test]
fn literal_translation_cache_reuses_only_the_requested_symbol_policy() {
    let mut context = crate::test_utils::new_context();
    context
        .eval_str("(set-case-table (standard-case-table))")
        .unwrap();
    let buffer = context.edit.buffers.current_buffer().unwrap();
    let bare = buffer_search_translation_with_symbols(buffer, true, SymbolMode::Bare).unwrap();
    assert!(Rc::ptr_eq(
        &bare,
        &buffer_search_translation_with_symbols(buffer, true, SymbolMode::Bare).unwrap()
    ));
    let positioned =
        buffer_search_translation_with_symbols(buffer, true, SymbolMode::Positioned).unwrap();
    assert!(!Rc::ptr_eq(&bare, &positioned));
    assert_eq!(positioned.symbol_mode(), SymbolMode::Positioned);
    assert!(Rc::ptr_eq(
        &positioned,
        &buffer_search_translation_with_symbols(buffer, true, SymbolMode::Positioned).unwrap()
    ));
    let restored = buffer_search_translation_with_symbols(buffer, true, SymbolMode::Bare).unwrap();
    assert_eq!(restored.symbol_mode(), SymbolMode::Bare);
    assert!(!Rc::ptr_eq(&positioned, &restored));
}

struct PolicySyntax(SymbolMode);

impl SyntaxLookup for PolicySyntax {
    fn symbol_mode(&self) -> SymbolMode {
        self.0
    }
    fn char_syntax(&self, c: char) -> neovm_text::emacs_core::syntax::SyntaxClass {
        DefaultSyntaxLookup.char_syntax(c)
    }
    fn char_has_category(&self, c: char, cat: u8) -> bool {
        DefaultSyntaxLookup.char_has_category(c, cat)
    }
    fn cache_key(&self) -> SyntaxCacheKey {
        SyntaxCacheKey::Standard
    }
}
