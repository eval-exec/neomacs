//! Store-chain slow exits classify the live owner tag without reading heap
//! contents or allocating. A barrier and a wrong-cons signal are semantic
//! events, even when the same physical store repeatedly leaves native code.

use super::*;
use crate::emacs_core::jit::{cache, compile};
use crate::emacs_core::value::LambdaParams;

fn assert_store_semantic(store: Op) {
    compile::force_profit_gate_for_test(false);
    compile::force_deopt_for_test(false);
    force_reopt_for_test(Some(ReoptKnobs {
        site_limit: 1,
        ..ReoptKnobs::stress()
    }));
    let cell = Value::cons(Value::make_int(0), Value::NIL);
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::Constant(0), Op::Constant(1), store, Op::Return];
    f.constants = vec![cell, Value::make_int(1)].into();
    f.max_stack = 4;
    f.seal_hand_assembled_ops_for_test();
    let id = cache::compile_and_cache_jit_leaf(&f, None).expect("store body compiles");
    let pointer = cache::compiled_leaf_ptr_for_test(id).expect("cached store body");
    // The cold hooks allocate no Lisp objects and do not reach a safepoint.
    // Retirement would retain the leaf's allocation, too.
    let leaf = unsafe { &*pointer };
    for (owner, cause) in [
        (cell, DeoptCause::ColdFlagged),
        (Value::make_int(9), DeoptCause::TypeError),
        (Value::NIL, DeoptCause::TypeError),
    ] {
        let stack = [owner, Value::make_int(1)];
        for origin in [
            LeafOrigin::Entry,
            LeafOrigin::Osr {
                header_pc: 2,
                snapshot: &stack,
            },
        ] {
            assert_eq!(
                classify(
                    std::ptr::null(),
                    f.executable_ops(),
                    leaf,
                    origin,
                    2,
                    &stack
                ),
                cause,
            );
            for _ in 0..3 {
                leaf.obs.note_deopt_at(2);
                assert_eq!(
                    note_deopt(
                        std::ptr::null(),
                        &f,
                        leaf,
                        origin,
                        DeoptEvent::Precise {
                            pc: 2,
                            stack: &stack,
                            cause: None
                        }
                    ),
                    ReoptVerdict::Kept,
                    "{cause:?} must not retire speculation at the one-event limit",
                );
                assert_eq!(leaf.obs.reopt_deopt_count_at(2), 0);
                assert!(!leaf.retired.get());
            }
        }
    }
    assert_eq!(leaf.obs.deopt_count_at(2), 18);
    assert_eq!(f.jit_runtime().deopt_history(2), 18);
    assert_eq!(f.jit_runtime().reopt_count(), 0);
    assert_eq!(
        cell.cons_car(),
        Value::make_int(0),
        "classification never writes"
    );
    force_reopt_for_test(None);
}

#[test]
fn setcar_chain_barriers_and_wrong_cons_errors_do_not_retire_speculation() {
    assert_store_semantic(Op::Setcar);
}

#[test]
fn setcdr_chain_barriers_and_wrong_cons_errors_do_not_retire_speculation() {
    assert_store_semantic(Op::Setcdr);
}
