//! X7's recursive form: a hot leaf entered through its own spec slots
//! upgrades, unlinks the self pointers, and serves the remaining recursion.

use crate::emacs_core::bytecode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::bg::{BgMode, force_mode_for_test};
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::compile::{
    Tier2Knob, Tier2PolicyKnob, force_deopt_for_test, force_profit_gate_for_test,
    force_tier2_for_test, force_tier2_policy_for_test,
};
use crate::emacs_core::jit::feedback::{FeedbackMode, force_feedback_mode_for_test};
use crate::emacs_core::jit::inline::force_inline_for_test;
use crate::emacs_core::jit::stats::{ObserveOverride, force_observe_for_test};
use crate::emacs_core::jit::tier2::{T2Origin, T2Upgrade};
use crate::emacs_core::value::Value;

#[test]
fn tier2_recursive_spec_reached_leaf_serves_over_ninety_percent() {
    const WINDOW: u32 = 20;
    let mut knob = Tier2Knob::from_env(|_| None);
    knob.on = true;
    knob.window = WINDOW;
    force_tier2_for_test(Some(knob));
    force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        stable: 1,
        attempts: 4,
        budget_pct: 0,
        floor_ms: 5,
        max_reopt: 3,
    }));
    force_mode_for_test(Some(BgMode::Sync));
    force_feedback_mode_for_test(Some(FeedbackMode::Use));
    force_inline_for_test(Some(false));
    force_profit_gate_for_test(false);
    force_deopt_for_test(false);
    force_observe_for_test(ObserveOverride {
        entry_count: true,
        ..Default::default()
    });
    let mut ctx = Context::new();
    let name = intern("neovm--t2-x7-self-fib");
    // (lambda (n) (if (< n 2) n
    //               (+ (neovm--t2-x7-self-fib (1- n))
    //                  (neovm--t2-x7-self-fib (- n 2)))))
    let f = function(
        vec![
            Op::StackRef(0),
            Op::Constant(0),
            Op::Lss,
            Op::GotoIfNil(5),
            Op::Return,
            Op::Constant(1),
            Op::StackRef(1),
            Op::Sub1,
            Op::Call(1),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Constant(0),
            Op::Sub,
            Op::Call(1),
            Op::Add,
            Op::Return,
        ],
        vec![Value::make_int(2), Value::from_sym_id(name)],
        1,
    );
    f.jit_runtime().set_hot_for_test();
    let value = Value::make_bytecode(f);
    crate::emacs_core::eval::push_scratch_gc_root(value);
    ctx.obarray.set_symbol_function_id(name, value);
    let f = value.get_bytecode_data().expect("recursive byte-code");
    assert_eq!(
        cache::try_run_compiled(&mut ctx, f, value, &[Value::make_int(3)]).unwrap(),
        Some(Value::make_int(2).bits())
    );
    let id = f.jit_runtime().compiled_id().expect("compiled");
    let old = cache::compiled_leaf_ptr_for_test(id).expect("T1");
    // SAFETY: this mutator's current/fallback/retired leaf roots retain
    // both leaves until cache clear. No clear occurs while inspected.
    let old = unsafe { &*old };
    assert_eq!(old.obs.t2.origin, T2Origin::Profiling);
    assert!(
        old.bytecode_spec_slots()
            .any(|slot| std::ptr::eq(slot.leaf_ptr(), old)),
        "the T1 was reached through its own spec slot"
    );
    // GNU 31.1 oracle: fib(18) is 2584 (tmp/t21-fib-gnu.txt).
    assert_eq!(
        cache::try_run_compiled(&mut ctx, f, value, &[Value::make_int(18)]).unwrap(),
        Some(Value::make_int(2584).bits())
    );
    let upgraded = cache::compiled_leaf_ptr_for_test(id).expect("upgrade");
    let upgraded = unsafe { &*upgraded };
    assert!(!std::ptr::eq(upgraded, old));
    assert_eq!(
        upgraded.obs.t2.origin,
        T2Origin::Upgrade(T2Upgrade::Feedback)
    );
    let (rows, _) = cache::leaf_report_rows();
    let total: u64 = rows
        .iter()
        .filter(|row| row.id == id)
        .map(|row| row.obs.entries)
        .sum();
    let served = upgraded.obs.entries.get();
    assert!(total > 8_000, "the recursive spec sites ran hot");
    assert!(
        served * 10 >= (total - u64::from(WINDOW)) * 9,
        "T2 served {served} of {} entries after the window",
        total - u64::from(WINDOW)
    );
    for leaf in [old, upgraded] {
        for slot in leaf.bytecode_spec_slots() {
            assert!(
                !std::ptr::eq(slot.leaf_ptr(), old),
                "no current or retained self slot still calls T1"
            );
        }
    }
    assert!(cache::revert_t2_to_t1(f, upgraded));
    assert!(std::ptr::eq(
        cache::compiled_leaf_ptr_for_test(id).expect("reverted T1"),
        old,
    ));
    for leaf in [old, upgraded] {
        assert!(
            leaf.bytecode_spec_slots()
                .all(|slot| !std::ptr::eq(slot.leaf_ptr(), upgraded)),
            "revert unlinks T2 from the detached fallback's self slots",
        );
    }
    force_tier2_policy_for_test(None);
    force_tier2_for_test(None);
    force_mode_for_test(None);
    force_feedback_mode_for_test(None);
    force_inline_for_test(None);
}
