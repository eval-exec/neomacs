//! Source attribution for a synthetic virtual-frame exit; no native chain
//! producer is enabled by this stage.
use super::*;
use crate::emacs_core::jit::{cache, compile};
use crate::emacs_core::value::LambdaParams;

fn function(ops: Vec<Op>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = vec![Value::make_int(1)].into();
    f.max_stack = 4;
    f.seal_hand_assembled_ops();
    f
}

#[test]
fn chain_feedback_widens_inner_source_and_retires_physical_leaf() {
    compile::force_profit_gate_for_test(false);
    compile::force_deopt_for_test(false);
    force_reopt_for_test(Some(ReoptKnobs::stress()));
    let physical = function(vec![Op::Constant(0), Op::Return]);
    let inner = function(vec![Op::Constant(0), Op::Add1, Op::Return]);
    let physical_id = cache::compile_and_cache_jit_leaf(&physical, None).expect("physical leaf");
    let inner_id = cache::compile_and_cache_jit_leaf(&inner, None).expect("inner leaf");
    let physical_ptr = cache::compiled_leaf_ptr_for_test(physical_id).unwrap();
    let inner_ptr = cache::compiled_leaf_ptr_for_test(inner_id).unwrap();
    // Cached and retired leaves remain allocated throughout this no-safepoint hook.
    let leaf = unsafe { &*physical_ptr };
    assert_eq!(
        note_deopt_chain(
            std::ptr::null(),
            &physical,
            &inner,
            leaf,
            LeafOrigin::Entry,
            0,
            DeoptEvent::Precise {
                pc: 1,
                stack: &[],
                cause: Some(DeoptCause::ArithOperands(NumericFeedback::Float))
            }
        ),
        ReoptVerdict::Invalidated
    );
    assert_eq!(
        inner.jit_runtime().numeric_feedback(1),
        NumericFeedback::Float
    );
    assert_eq!(
        physical.jit_runtime().numeric_feedback(1),
        NumericFeedback::FixnumOnly
    );
    assert_eq!(
        cache::cache_entry_kind_for_test(physical_id),
        "deferred-reopt"
    );
    assert_eq!(cache::compiled_leaf_ptr_for_test(inner_id), Some(inner_ptr));
    assert!(leaf.retired.get());
    assert!(!unsafe { &*inner_ptr }.retired.get());
    force_reopt_for_test(None);
}

#[test]
fn chain_repeated_deopt_uses_physical_guard_pc_for_site_limit() {
    compile::force_profit_gate_for_test(false);
    compile::force_deopt_for_test(false);
    let knobs = ReoptKnobs::stress();
    force_reopt_for_test(Some(knobs));
    let physical = function(vec![Op::Constant(0), Op::Return]);
    let inner = function(vec![Op::Constant(0), Op::Add1, Op::Return]);
    let id = cache::compile_and_cache_jit_leaf(&physical, None).unwrap();
    let ptr = cache::compiled_leaf_ptr_for_test(id).unwrap();
    // No safepoint occurs in the hook; retirement retains the old leaf.
    let leaf = unsafe { &*ptr };
    for count in 1..=knobs.site_limit {
        leaf.obs.note_deopt_at(0);
        let verdict = note_deopt_chain(
            std::ptr::null(),
            &physical,
            &inner,
            leaf,
            LeafOrigin::Entry,
            0,
            DeoptEvent::Precise {
                pc: 1,
                stack: &[],
                cause: Some(DeoptCause::ArithOverflow),
            },
        );
        assert_eq!(
            verdict,
            if count < knobs.site_limit {
                ReoptVerdict::Kept
            } else {
                ReoptVerdict::Invalidated
            }
        );
    }
    assert_eq!(
        inner.jit_runtime().numeric_feedback(1),
        NumericFeedback::Other
    );
    assert_eq!(
        physical.jit_runtime().numeric_feedback(1),
        NumericFeedback::FixnumOnly
    );
    assert!(leaf.retired.get());
    force_reopt_for_test(None);
}

#[test]
fn inline_identity_deopt_retires_the_physical_leaf_without_a_profile_window() {
    compile::force_profit_gate_for_test(false);
    compile::force_deopt_for_test(false);
    force_reopt_for_test(Some(ReoptKnobs {
        site_limit: 3,
        ..ReoptKnobs::stress()
    }));
    let physical = function(vec![Op::Constant(0), Op::Return]);
    let inner = function(vec![Op::Constant(0), Op::Add1, Op::Return]);
    let id = cache::compile_and_cache_jit_leaf(&physical, None).unwrap();
    let ptr = cache::compiled_leaf_ptr_for_test(id).unwrap();
    // The invalidation retains the old allocation and reaches no safepoint.
    let leaf = unsafe { &*ptr };
    leaf.obs.note_deopt_at(0);
    assert_eq!(
        note_deopt_chain(
            std::ptr::null(),
            &physical,
            &inner,
            leaf,
            LeafOrigin::Entry,
            0,
            DeoptEvent::Precise {
                pc: 1,
                stack: &[],
                cause: Some(DeoptCause::InlineIdentity),
            },
        ),
        ReoptVerdict::Invalidated,
        "an identity mismatch is conclusive on its first exit"
    );
    assert_eq!(cache::cache_entry_kind_for_test(id), "none");
    assert_eq!(physical.jit_runtime().reopt_count(), 1);
    assert_eq!(inner.jit_runtime().reopt_count(), 0);
    assert_eq!(
        inner.jit_runtime().numeric_feedback(1),
        NumericFeedback::FixnumOnly
    );
    assert!(leaf.retired.get());
    force_reopt_for_test(None);
}
