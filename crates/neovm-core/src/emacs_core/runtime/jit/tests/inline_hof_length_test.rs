//! The bounded, context-free HOF preflight keeps its exact decline contract.

use super::{HOF_LIST_PREFLIGHT_MAX, neovm_jit_hof_length};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

fn preflight(sequence: Value) -> i64 {
    neovm_jit_hof_length(sequence.bits() as i64)
}

fn tail_at(mut sequence: Value, offset: usize) -> Value {
    for _ in 0..offset {
        sequence = sequence.cons_cdr();
    }
    sequence
}

#[test]
fn inline_hof_length_brent_preserves_proper_lengths_at_teleport_and_limit_boundaries() {
    let mut ctx = Context::new();
    ctx.push_vm_root_frame();
    let total = HOF_LIST_PREFLIGHT_MAX as usize + 1;
    let sequence = Value::list((0..total).map(|n| Value::make_int(n as i64)).collect());
    ctx.push_vm_frame_root(sequence);
    let counts = Value::memory_use_counts_snapshot();
    for len in [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 13, 14, 15, 16, 29, 30, 31, 32, 32_766, 32_767, 32_768, 65_534,
        65_535,
    ] {
        assert_eq!(
            preflight(tail_at(sequence, total - len)),
            len as i64,
            "length {len}"
        );
    }
    assert_eq!(preflight(sequence), -1, "65,536 cells decline");
    assert_eq!(Value::memory_use_counts_snapshot(), counts);
    ctx.pop_vm_root_frame();
}

#[test]
fn inline_hof_length_brent_declines_dotted_and_lasso_lists() {
    let mut ctx = Context::new();
    ctx.push_vm_root_frame();
    let dotted = Value::cons(Value::make_int(1), Value::make_int(2));
    ctx.push_vm_frame_root(dotted);
    let mut lassos = Vec::new();
    for prefix in [0, 1, 2, 7] {
        for period in [1, 2, 3, 8, 17] {
            let total = prefix + period;
            let sequence = Value::list((0..total).map(|n| Value::make_int(n as i64)).collect());
            ctx.push_vm_frame_root(sequence);
            tail_at(sequence, total - 1).set_cdr(tail_at(sequence, prefix));
            lassos.push((prefix, period, sequence));
        }
    }
    let counts = Value::memory_use_counts_snapshot();
    assert_eq!(preflight(Value::NIL), 0);
    assert_eq!(preflight(Value::make_int(2)), -1);
    assert_eq!(preflight(dotted), -1);
    for (prefix, period, sequence) in lassos {
        assert_eq!(preflight(sequence), -1, "prefix {prefix}, cycle {period}");
    }
    assert_eq!(Value::memory_use_counts_snapshot(), counts);
    ctx.pop_vm_root_frame();
}

#[test]
fn inline_hof_length_brent_reads_mutated_cdrs_without_cached_results() {
    let mut ctx = Context::new();
    ctx.push_vm_root_frame();
    let sequence = Value::list((0..6).map(Value::make_int).collect());
    ctx.push_vm_frame_root(sequence);
    let second = sequence.cons_cdr();
    ctx.push_vm_frame_root(second);
    let last = tail_at(sequence, 5);
    let counts = Value::memory_use_counts_snapshot();
    assert_eq!(preflight(sequence), 6);
    last.set_cdr(tail_at(sequence, 2));
    assert_eq!(preflight(sequence), -1, "cycle introduced after validation");
    last.set_cdr(Value::NIL);
    assert_eq!(preflight(sequence), 6);
    sequence.set_cdr(Value::NIL);
    assert_eq!(preflight(sequence), 1, "shortened list");
    sequence.set_cdr(Value::make_int(4));
    assert_eq!(preflight(sequence), -1, "dotted list");
    sequence.set_cdr(second);
    assert_eq!(preflight(sequence), 6, "repaired list");
    assert_eq!(Value::memory_use_counts_snapshot(), counts);
    ctx.pop_vm_root_frame();
}
