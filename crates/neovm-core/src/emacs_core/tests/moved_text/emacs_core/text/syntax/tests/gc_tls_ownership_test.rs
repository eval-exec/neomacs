use super::*;

use LispSyntaxObjectReuse::{CanonicalBare, Fresh};
use neovm_edit::buffer::buffer::BUFFER_SLOT_SYNTAX_TABLE;
use neovm_edit::buffer::{
    Buffer, BufferManager, CharLen, CharPos0, EmacsByteLen, EmacsBytePos, LispCharPos1,
    TextPropertyTable,
};
use neovm_edit::emacs_core::edit_host::EditVariableRead;
use neovm_edit::emacs_core::textprop::CharPropertyResolver;
use neovm_text::emacs_core::regex_emacs::BoundarySyntaxMode;
use neovm_text::emacs_core::regex_emacs::SyntaxAssertion;
use neovm_text::emacs_core::syntax::parse_loop::{
    Entry, Plain, SafePositionRecorder, ScanEnd, ScanFinish, run_parse_loop,
};
use neovm_value::emacs_char::EmacsChar;
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::heap_types::{GnuStringCharPos, LispString, StringCharacterCountKind};
use neovm_value::symbol::CanonicalAccess;
use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::sync::OnceLock;
use strum::{EnumString, IntoStaticStr};

use crate::emacs_core::eval::Context;
use neovm_value::value::Value;

use neovm_text::emacs_core::syntax::*;

#[test]
fn gc_tls_ownership_syntax_new_heap_reset_drops_old_values() {
    {
        let mut first = crate::test_utils::new_context();
        let value = populate(&mut first);
        assert!(roots(&first).iter().any(|root| root.bits() == value.bits()));
    }
    let mut next = crate::test_utils::new_context();
    assert!(
        roots(&next)
            .iter()
            .all(|value| !value.is_heap_object()
                || next.tagged_heap().owns_heap_value_for_test(*value)),
        "new-heap reset retained a value from the dropped heap"
    );
    next.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_syntax_excludes_another_live_heap() {
    let mut first = crate::test_utils::new_context();
    let mut second = crate::test_utils::new_context();
    let value = populate(&mut second);
    assert!(
        roots(&second)
            .iter()
            .any(|root| root.bits() == value.bits())
    );
    assert!(
        roots(&first)
            .iter()
            .all(|value| !value.is_heap_object()
                || first.tagged_heap().owns_heap_value_for_test(*value)),
        "GC roots contain a value belonging to another live Context"
    );
    first.gc_collect_exact();
    second.setup_thread_locals();
    second.gc_collect_exact();
    assert!(second.tagged_heap().owns_heap_value_for_test(value));
}

#[test]
fn gc_tls_ownership_syntax_weak_ascii_keys_invalidate_before_heap_reuse() {
    let old_tick = {
        let mut first = crate::test_utils::new_context();
        let chartable = first.eval_str("(progn (set-syntax-table (copy-syntax-table)) (modify-syntax-entry ?a \".\") (syntax-table))").unwrap();
        let table = neovm_text::test_support::authority::syntax_table(chartable);
        assert_eq!(
            flat_ascii_syntax_entry(&table, b'a').class,
            SyntaxClass::Punctuation
        );
        assert_eq!(
            flat_ascii_entries_for_table(&table)[b'a' as usize].class(),
            SyntaxClass::Punctuation
        );
        first.gc_collect_exact();
        neovm_text::emacs_core::chartable::char_table_write_tick()
    };
    let mut next = crate::test_utils::new_context();
    assert_ne!(
        neovm_text::emacs_core::chartable::char_table_write_tick(),
        old_tick,
        "new char-tables must invalidate weak identity caches before address reuse"
    );
    let table = SyntaxTable::new_standard();
    assert_eq!(
        flat_ascii_syntax_entry(&table, b'a').class,
        SyntaxClass::Word
    );
    assert_eq!(
        flat_ascii_entries_for_table(&table)[b'a' as usize].class(),
        SyntaxClass::Word
    );
    next.gc_collect_exact();
}

fn populate(_ctx: &mut Context) -> Value {
    let table = ensure_standard_syntax_table_object().unwrap();
    ensure_syntax_code_objects();
    table
}

fn roots(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_syntax_gc_roots(&mut roots, ctx.tagged_heap().tagged_heap_identity());
    roots
}
