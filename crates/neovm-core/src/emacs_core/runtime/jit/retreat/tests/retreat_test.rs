//! The per-pc retreat word and its lazily allocated table.

use super::*;

/// The bits are distinct, fit the low byte, and leave the count byte alone.
#[test]
fn retreat_bits_are_distinct_and_fit_the_low_byte() {
    let mut seen = 0u16;
    for bit in RetreatBit::ALL {
        let b = bit as u16;
        assert_eq!(b.count_ones(), 1, "{bit:?}");
        assert_eq!(
            b & !SiteRetreat::BITS_MASK,
            0,
            "{bit:?} leaves the low byte"
        );
        assert_eq!(seen & b, 0, "{bit:?} repeats a bit");
        seen |= b;
    }
    let site = SiteRetreat::default();
    for bit in RetreatBit::ALL {
        site.set(bit);
    }
    assert_eq!(site.count(), 0);
    assert_eq!(site.bits().collect::<Vec<_>>(), RetreatBit::ALL);
}

/// Setting one bit sets only that bit, and the word renders it by name.
#[test]
fn a_set_bit_is_the_only_one_on() {
    for bit in RetreatBit::ALL {
        let site = SiteRetreat::default();
        site.set(bit);
        for other in RetreatBit::ALL {
            assert_eq!(site.has(other), other == bit, "{bit:?} vs {other:?}");
        }
        assert!(format!("{site:?}").contains(bit.name()));
    }
}

/// A read never allocates; the first mark sizes the table to the body.
#[test]
fn the_table_allocates_on_the_first_mark_only() {
    let table = SiteRetreatTable::new();
    assert!(!table.has(3, RetreatBit::NoInline));
    assert!(table.get(3).is_none(), "a read allocated the table");
    table
        .site(3, 8)
        .expect("pc 3 of an 8-op body")
        .set(RetreatBit::NoInline);
    assert!(table.has(3, RetreatBit::NoInline));
    assert!(!table.has(2, RetreatBit::NoInline));
    assert!(!table.has(3, RetreatBit::NoHoist));
    assert!(table.site(8, 8).is_none(), "pc past the body");
    assert!(!table.has(100, RetreatBit::NoInline));
}

/// P0.4's no-inline marks keep their meaning through the table.
#[test]
fn no_inline_marks_live_in_the_retreat_table() {
    let rt = crate::emacs_core::jit::RuntimeState::new();
    assert!(!rt.call_site_no_inline(5));
    rt.mark_call_site_no_inline(5, 10);
    assert!(rt.call_site_no_inline(5));
    assert!(!rt.call_site_no_inline(4));
    assert!(!rt.call_site_no_inline(6));
    // A pc past the body is ignored, as the bitset did.
    rt.mark_call_site_no_inline(64, 10);
    assert!(!rt.call_site_no_inline(64));
}

/// The deopt count lives in the high byte: it counts, saturates at 255 and
/// leaves the bits alone (and the bits leave it alone).
#[test]
fn the_deopt_count_counts_saturates_and_keeps_the_bits() {
    let site = SiteRetreat::default();
    site.set(RetreatBit::NoHoist);
    assert_eq!(site.note_deopt(), 1);
    assert_eq!(site.note_deopt(), 2);
    for _ in 0..300 {
        site.note_deopt();
    }
    assert_eq!(site.count(), u8::MAX);
    assert_eq!(site.note_deopt(), u8::MAX, "saturated");
    site.set(RetreatBit::KeepBlock);
    assert_eq!(site.count(), u8::MAX);
    assert_eq!(
        site.bits().collect::<Vec<_>>(),
        [RetreatBit::NoHoist, RetreatBit::KeepBlock]
    );
}

/// A precise deopt counts in its source's history at its pc (P2.1 C2), and
/// the count stays with the source when the deopt retires the leaf: the
/// leaf's own counters go with the leaf, the history does not.
#[cfg(feature = "jit")]
#[test]
fn a_precise_deopt_counts_in_the_history_and_outlives_the_leaf() {
    use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::jit::{ReoptLevel, cache};
    use crate::emacs_core::value::{LambdaParams, Value};
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    // (lambda (x) (+ x 1)), held to the baseline, whose `+` deopts
    // precisely on a float.
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return];
    f.constants = vec![Value::make_int(1)].into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(4);
    f.seal_hand_assembled_ops();
    f.jit_runtime()
        .set_reopt_level_for_test(ReoptLevel::BaselineOnly);
    let f_val = Value::make_bytecode(f.clone());
    let run = |arg: Value| {
        crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[arg]).expect("no signal")
    };
    assert_eq!(run(Value::make_int(41)), Some(Value::make_int(42).bits()));
    let id = f.jit_runtime().compiled_id().expect("compiled");
    assert_eq!(cache::cache_entry_kind_for_test(id), "compiled");
    assert_eq!(f.jit_runtime().deopt_history(2), 0, "nothing deopted yet");
    // The float deopts at the `+` (pc 2); the interpreter finishes the call.
    let _ = run(Value::make_float(1.5));
    assert_eq!(f.jit_runtime().deopt_history(2), 1);
    assert_eq!(f.jit_runtime().deopt_history(0), 0, "only the deopting pc");
    // A float at a fixnum site is conclusive: the leaf was retired, and the
    // history is still the source's.
    assert_ne!(cache::cache_entry_kind_for_test(id), "compiled");
    assert_eq!(f.jit_runtime().deopt_history(2), 1);
}
