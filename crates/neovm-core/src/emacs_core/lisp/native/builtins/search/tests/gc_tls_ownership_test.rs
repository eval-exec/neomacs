use super::*;
use crate::emacs_core::eval::Context;

#[test]
fn gc_tls_ownership_regex_reentry_keeps_evicted_compiled_translation_alive() {
    let mut ctx = Context::new();
    ctx.eval_str(
        r#"(progn
            (setq parse-sexp-lookup-properties t
                  syntax-propertize--done -1
                  gc-tls-reentry-ran nil)
            (insert "aa")
            (fset 'internal--syntax-propertize
              (lambda (target)
                (setq gc-tls-reentry-ran t)
                (set-case-table (standard-case-table))
                (let ((n 0))
                  (while (< n 129)
                    (string-match (concat "[a-z]" (number-to-string n)) "a")
                    (setq n (1+ n))))
                (garbage-collect)
                (setq syntax-propertize--done target))))"#,
    )
    .unwrap();
    let custom = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
    crate::emacs_core::casetab::builtin_set_case_table(&mut ctx, vec![custom]).unwrap();
    let canon =
        crate::emacs_core::casetab::buffer_case_canon_table(ctx.buffers.current_buffer().unwrap())
            .unwrap();
    let payload = ctx.eval_str("(make-hash-table)").unwrap();
    crate::emacs_core::chartable::builtin_set_char_table_range(
        vec![canon, Value::fixnum(0x1ffff), payload],
        None,
    )
    .unwrap();
    let (_, lazy_relevant, compiled) = prepare_current_buffer_regexp_syntax_to_reporting_compiled(
        &mut ctx,
        Value::string("[[:word:]]a"),
        true,
        false,
        Some(2),
    )
    .unwrap();
    assert!(lazy_relevant);
    assert!(
        ctx.obarray
            .symbol_value_id_copied(intern("gc-tls-reentry-ran"))
            .is_some_and(|ran| !ran.is_nil())
    );
    assert!(
        ctx.tagged_heap.owns_heap_value_for_test(payload),
        "syntax propertize evicted the active regexp and reclaimed its translation table"
    );
    assert!(compiled.translate.is_some());
}
