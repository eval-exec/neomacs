use super::*;
use crate::emacs_core::eval::Context;

#[test]
fn gc_tls_ownership_regex_reentry_keeps_evicted_compiled_translation_alive() {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
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
    let mode = ctx.symbol_mode();
    // Same install as `(set-case-table)`: refuse a non-table, ensure the
    // standard table, fill derived slots, then write the buffer slot.
    (|| -> neovm_value::error::EvalResult {
        if !neovm_text::emacs_core::casetab::is_case_table_with_symbols(&custom, mode) {
            return Err(neovm_value::error::signal(
                neovm_value::error::LispCondition::WrongTypeArgument,
                vec![Value::symbol("case-table-p"), custom],
            ));
        }
        // SAFETY: the standard-table ensure only reads and stores a variable value.
        let _ = unsafe {
            ctx.obarray_view_mut().with_legacy_value_write(|obarray| {
                neovm_text::emacs_core::casetab::ensure_standard_case_table_object_in_state_with_symbols(
                    obarray, mode,
                )
            })
        }?;
        neovm_text::emacs_core::casetab::ensure_case_table_derived_slots_with_symbols(
            custom, mode,
        )?;
        neovm_text::emacs_core::casetab::set_current_case_table_for_buffer_in_state(
            &mut ctx.edit.buffers,
            custom,
        )?;
        Ok(custom)
    })()
    .unwrap();
    let canon = neovm_text::emacs_core::casetab::buffer_case_canon_table(
        ctx.edit.buffers.current_buffer().unwrap(),
    )
    .unwrap();
    let payload = ctx.eval_str("(make-hash-table)").unwrap();
    neovm_text::emacs_core::chartable::builtin_set_char_table_range(
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
