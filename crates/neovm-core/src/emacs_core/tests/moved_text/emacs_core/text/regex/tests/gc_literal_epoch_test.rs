use super::*;
use std::fmt;

use neovm_edit::buffer::{
    Buffer, BufferId, CharPos0, CharRange, EmacsBytePos, EmacsByteRange, LispCharPos1,
};
use neovm_text::emacs_core::casefiddle::apply_replace_match_case;
use neovm_text::emacs_core::casefiddle::{
    apply_replace_match_case_lisp, apply_replace_match_case_lisp_with,
};
use neovm_text::emacs_core::regex_emacs::{
    self, BufferSyntaxLookup, CaseTranslation, CompiledPattern, DefaultSyntaxLookup,
    MatchRegisters, SyntaxCacheKey, SyntaxLookup,
};
use neovm_text::emacs_core::syntax::SyntaxClass;
use neovm_value::emacs_char::char_to_byte_pos;
use neovm_value::heap_types::{GnuStringCharPos, LispString};
use neovm_value::tagged::SymbolMode;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::emacs_core::casetab_entries::builtin_set_case_table;
use crate::emacs_core::eval::Context;

use neovm_text::emacs_core::regex::*;

use neovm_value::value::Value;

use super::gc_collection_epoch_tests::UncoveredRegexCaches;

#[test]
fn gc_collection_epoch_literal_translation_discards_memo_before_address_reuse() {
    let mut ctx = crate::test_utils::new_context();
    // Build the cache fixture without safe-point collections of Rust locals.
    neovm_eval::test_support::authority::enter_gc_inhibit_depth_for_test(&mut ctx);
    let old_case = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
    neovm_text::emacs_core::chartable::builtin_set_char_table_range(
        vec![
            old_case,
            Value::fixnum(b'[' as i64),
            Value::fixnum(b']' as i64),
        ],
        None,
    )
    .unwrap();
    crate::emacs_core::casetab_entries::builtin_set_case_table(&mut ctx, vec![old_case]).unwrap();
    let old_canon = neovm_text::emacs_core::casetab::buffer_case_canon_table(
        ctx.edit.buffers.current_buffer().unwrap(),
    )
    .unwrap();
    assert_eq!(
        buffer_search_translation(ctx.edit.buffers.current_buffer().unwrap(), true)
            .unwrap()
            .translate(b'[' as u32),
        b']' as u32
    );
    ctx.eval_str("(set-case-table (standard-case-table))")
        .unwrap();
    neovm_eval::test_support::authority::leave_gc_inhibit_depth_for_test(&mut ctx);

    let stale = UncoveredRegexCaches::take();
    ctx.gc_collect_exact();
    assert!(!ctx.tagged_heap().owns_heap_value_for_test(old_canon));
    // SAFETY: test-only owner-thread heap setup through the non-swappable mutable accessor contract; the view is used in place for allocation/policy and is never swapped or moved across Worlds.
    unsafe { ctx.tagged_heap_mut() }.set_gc_threshold(usize::MAX);

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
        neovm_text::emacs_core::chartable::builtin_set_char_table_range(
            vec![
                canon,
                Value::fixnum(b'[' as i64),
                Value::fixnum(b'~' as i64),
            ],
            None,
        )
        .unwrap();
        let case = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
        neovm_text::emacs_core::chartable::builtin_set_char_table_extra_slot(vec![
            case,
            Value::fixnum(1),
            canon,
        ])
        .unwrap();
        crate::emacs_core::casetab_entries::builtin_set_case_table(&mut ctx, vec![case]).unwrap();
        assert_eq!(
            neovm_text::emacs_core::casetab::buffer_case_canon_table(
                ctx.edit.buffers.current_buffer().unwrap()
            )
            .unwrap()
            .bits(),
            old_canon.bits()
        );
    }
    let reused_canon = reused;
    let allocated = candidates.len();
    stale.restore();

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
        assert!(ctx.tagged_heap().owns_heap_value_for_test(canon));
        assert_eq!(
            buffer_search_translation(ctx.edit.buffers.current_buffer().unwrap(), true)
                .unwrap()
                .translate(b'[' as u32),
            b'~' as u32,
            "recycled case-canon address reused the old literal folding memo"
        );
    }
}
