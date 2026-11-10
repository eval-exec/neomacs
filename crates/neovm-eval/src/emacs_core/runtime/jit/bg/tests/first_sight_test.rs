//! First-sight compiles in the background (P2.4 B9): a speculated call
//! site's first call into an uncompiled callee defers the callee's backend;
//! the site takes the strict call path until the leaf is installed. T-F1.

use crate::emacs_core::bytecode::ByteCodeFunctionExt;

use super::*;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::RuntimeStateTieringExt;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::compile::force_deopt_for_test;
use neovm_value::value::Value;

/// A first-sight resolve defers the callee's backend and answers "no leaf"
/// (the strict path); the resolve after the backend finished installs it.
#[test]
fn jit_bg_first_sight_defers_then_installs() {
    force_mode_for_test(Some(BgMode::Sync));
    force_deferred_install_for_test(true);
    hold_publish_for_test(true);
    force_deopt_for_test(false);
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let ctx = &mut ev as *mut Context;
    let f = function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    );
    let before = stats_snapshot().enqueued[JobClass::FirstSight as usize];
    assert!(cache::resolve_compiled_leaf_ptr(ctx, &f).is_none());
    let id = f.jit_runtime().compiled_id().expect("id");
    assert_eq!(cache::cache_entry_kind_for_test(id), "pending");
    assert_eq!(
        stats_snapshot().enqueued[JobClass::FirstSight as usize],
        before + 1
    );
    assert!(
        cache::resolve_compiled_leaf_ptr(ctx, &f).is_none(),
        "still running: the strict path again"
    );
    assert_eq!(publish_held_for_test(), 1);
    assert!(cache::resolve_compiled_leaf_ptr(ctx, &f).is_some());
    assert_eq!(cache::cache_entry_kind_for_test(id), "compiled");
    hold_publish_for_test(false);
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}
