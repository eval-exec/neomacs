use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[test]
fn gc_tls_ownership_regex_compiled_translation_keeps_its_table_alive() {
    let mut ctx = Context::new();
    let payload = ctx.eval_str("(make-hash-table)").unwrap();
    let table = crate::emacs_core::chartable::make_char_table_value(Value::NIL, payload);
    let pattern = LispString::from_utf8("gc-tls-translation-pattern");
    let compiled = compile_lisp_pattern_with_posix_translation(
        &pattern,
        true,
        false,
        true,
        Some(table),
        &DefaultSyntaxLookup,
    )
    .unwrap();
    assert!(compiled.translate.is_some());
    ctx.gc_collect_exact();
    // A hash-table is boxed: the ownership probe never dereferences its
    // payload and can safely detect that exact GC freed the cached child.
    assert!(
        ctx.tagged_heap.owns_heap_value_for_test(payload),
        "compiled regexp cache failed to root its case translation char-table"
    );
    assert_eq!(
        compiled.translate.as_ref().unwrap().translate(b'A' as u32),
        b'A' as u32
    );
}

#[test]
fn gc_tls_ownership_regex_new_heap_discards_old_compiled_tables() {
    {
        let _first = Context::new();
        let table = crate::emacs_core::chartable::make_char_table_value(Value::NIL, Value::NIL);
        compile_lisp_pattern_with_posix_translation(
            &LispString::from_utf8("gc-tls-old-translation"),
            true,
            false,
            true,
            Some(table),
            &DefaultSyntaxLookup,
        )
        .unwrap();
        assert!(LISP_REGEX_PATTERN_CACHE.with(|cache| !cache.borrow().is_empty()));
    }
    let mut next = Context::new();
    assert!(
        LISP_REGEX_PATTERN_CACHE.with(|cache| cache.borrow().is_empty()),
        "compiled regexp cache retained a translator from a dropped heap"
    );
    next.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_regex_literal_translation_keeps_its_table_alive() {
    let mut ctx = Context::new();
    let custom = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
    crate::emacs_core::chartable::builtin_set_char_table_range(
        vec![
            custom,
            Value::fixnum(b'[' as i64),
            Value::fixnum(b']' as i64),
        ],
        None,
    )
    .unwrap();
    crate::emacs_core::casetab::builtin_set_case_table(&mut ctx, vec![custom]).unwrap();
    let buffer = ctx.buffers.current_buffer().unwrap();
    let table = crate::emacs_core::casetab::buffer_case_canon_table(buffer).unwrap();
    let translation = buffer_search_translation(buffer, true).unwrap();
    let payload = ctx.eval_str("(make-hash-table)").unwrap();
    crate::emacs_core::chartable::builtin_set_char_table_range(
        vec![table, Value::fixnum(0x1ffff), payload],
        None,
    )
    .unwrap();
    ctx.eval_str("(set-case-table (standard-case-table))")
        .unwrap();
    ctx.gc_collect_exact();
    assert!(
        ctx.tagged_heap.owns_heap_value_for_test(payload),
        "literal regexp cache failed to root its case translation char-table"
    );
    assert_eq!(translation.translate(b'[' as u32), b']' as u32);
}

#[derive(Clone, Copy)]
enum SyntaxCacheOwner {
    LispPatterns,
    SearchPatterns,
}

fn assert_cached_syntax_table_lives(owner: SyntaxCacheOwner) {
    let mut ctx = Context::new();
    let table = ctx
        .eval_str("(progn (set-syntax-table (copy-syntax-table)) (syntax-table))")
        .unwrap();
    let payload = ctx.eval_str("(make-hash-table)").unwrap();
    crate::emacs_core::chartable::builtin_set_char_table_range(
        vec![table, Value::fixnum(0x1ffff), payload],
        None,
    )
    .unwrap();
    let syntax = BufferSyntaxLookup {
        syntax_table: crate::emacs_core::syntax::SyntaxTable::for_buffer(
            ctx.buffers.current_buffer().unwrap(),
        ),
        category_table: None,
        word_boundary: Default::default(),
    };
    let pattern = LispString::from_utf8("[[:word:]]");
    match owner {
        SyntaxCacheOwner::LispPatterns => {
            let compiled = compile_lisp_pattern_with_posix_translation(
                &pattern, false, false, true, None, &syntax,
            )
            .unwrap();
            assert!(compiled.used_syntax);
        }
        SyntaxCacheOwner::SearchPatterns => {
            let compiled =
                compile_search_pattern_with_posix(&pattern, false, false, &syntax).unwrap();
            assert!(
                matches!(compiled, CompiledSearchPattern::Emacs(ref pattern) if pattern.used_syntax)
            );
        }
    }
    ctx.eval_str("(set-syntax-table (standard-syntax-table))")
        .unwrap();
    ctx.gc_collect_exact();
    assert!(
        ctx.tagged_heap.owns_heap_value_for_test(payload),
        "compiled syntax cache kept table identity bits while exact GC freed that table"
    );
}

#[test]
fn gc_tls_ownership_regex_lisp_cache_roots_its_syntax_identity() {
    assert_cached_syntax_table_lives(SyntaxCacheOwner::LispPatterns);
}

#[test]
fn gc_tls_ownership_regex_search_cache_roots_its_syntax_identity() {
    assert_cached_syntax_table_lives(SyntaxCacheOwner::SearchPatterns);
}
