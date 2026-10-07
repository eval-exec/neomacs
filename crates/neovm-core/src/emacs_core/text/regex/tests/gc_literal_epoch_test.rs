use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[test]
fn gc_collection_epoch_literal_translation_discards_memo_before_address_reuse() {
    let mut ctx = Context::new();
    // Build the cache fixture without safe-point collections of Rust locals.
    ctx.gc_inhibit_depth += 1;
    let old_case = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
    crate::emacs_core::chartable::builtin_set_char_table_range(
        vec![
            old_case,
            Value::fixnum(b'[' as i64),
            Value::fixnum(b']' as i64),
        ],
        None,
    )
    .unwrap();
    crate::emacs_core::casetab::builtin_set_case_table(&mut ctx, vec![old_case]).unwrap();
    let old_canon =
        crate::emacs_core::casetab::buffer_case_canon_table(ctx.buffers.current_buffer().unwrap())
            .unwrap();
    assert_eq!(
        buffer_search_translation(ctx.buffers.current_buffer().unwrap(), true)
            .unwrap()
            .translate(b'[' as u32),
        b']' as u32
    );
    ctx.eval_str("(set-case-table (standard-case-table))")
        .unwrap();
    ctx.gc_inhibit_depth -= 1;

    let (mut ctx, reused_canon, allocated) = std::thread::Builder::new()
        .name("literal-epoch-collector".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            ctx.setup_thread_locals();
            ctx.gc_collect_exact();
            assert!(!ctx.tagged_heap.owns_heap_value_for_test(old_canon));
            ctx.tagged_heap.set_gc_threshold(usize::MAX);

            // Allocate on the freeing thread so ordinary allocators can reuse
            // its char-table free list. No safe point occurs in this loop.
            // Retain every candidate until after allocation to avoid repeatedly
            // allocating and freeing an unrelated address.
            let mut candidates = Vec::new();
            let mut reused = None;
            for _ in 0..1500 {
                let candidate = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
                candidates.push(candidate);
                if candidate.bits() == old_canon.bits() {
                    reused = Some(candidate);
                    break;
                }
            }
            if let Some(canon) = reused {
                crate::emacs_core::chartable::builtin_set_char_table_range(
                    vec![
                        canon,
                        Value::fixnum(b'[' as i64),
                        Value::fixnum(b'~' as i64),
                    ],
                    None,
                )
                .unwrap();
                let case = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
                crate::emacs_core::chartable::builtin_set_char_table_extra_slot(vec![
                    case,
                    Value::fixnum(1),
                    canon,
                ])
                .unwrap();
                crate::emacs_core::casetab::builtin_set_case_table(&mut ctx, vec![case]).unwrap();
                assert_eq!(
                    crate::emacs_core::casetab::buffer_case_canon_table(
                        ctx.buffers.current_buffer().unwrap()
                    )
                    .unwrap()
                    .bits(),
                    old_canon.bits()
                );
            }
            (ctx, reused, candidates.len())
        })
        .unwrap()
        .join()
        .unwrap();

    let subscriber = tracing_subscriber::fmt()
        .with_test_writer()
        .with_ansi(false)
        .without_time()
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(
            address_reused = reused_canon.is_some(),
            allocated,
            "literal translation collection-epoch fixture"
        );
    });

    ctx.setup_thread_locals();
    // This assertion is independent of the allocator: even a quarantining
    // allocator must discard the stale memo after the foreign collection.
    assert!(
        LITERAL_TRT_CACHE.with(|cache| cache.borrow().is_none()),
        "activation retained the literal translation memo after a foreign collection"
    );
    if let Some(canon) = reused_canon {
        assert!(ctx.tagged_heap.owns_heap_value_for_test(canon));
        assert_eq!(
            buffer_search_translation(ctx.buffers.current_buffer().unwrap(), true)
                .unwrap()
                .translate(b'[' as u32),
            b'~' as u32,
            "recycled case-canon address reused the old literal folding memo"
        );
    }
}
