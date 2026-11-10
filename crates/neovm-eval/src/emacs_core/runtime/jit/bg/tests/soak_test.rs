//! The background-compile soak (P2.4 B13): a Lisp program run first on
//! its interpreted definitions, then byte-compiled under `NEOVM_JIT_BG=on`
//! with the stress knob (every worker job 0-2 ms late, every install 1-4
//! probes late), must give the same answer at every iteration: entry
//! tier-ups, first-sight callees and OSR loops go pending and install at
//! random points, while a callee is redefined mid-run and the collector
//! runs.

use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use std::time::Duration;

use super::*;
use crate::emacs_core::jit::compile::force_deopt_for_test;
use crate::emacs_core::print::print_value;
use neovm_value::intern::intern;

/// The soak's collector side, on a bare Context small enough to collect at
/// every allocation (`NEOVM_GC_STRESS=1` makes it so; `gc_stress` here
/// does it either way): functions whose only reference to a constant list
/// is their leaf tier up through the worker while the eval thread keeps
/// collecting; every call, interpreted, pending or native, returns the
/// intact list.
#[test]
fn jit_bg_worker_installs_under_a_collection_per_allocation() {
    use crate::emacs_core::bytecode::Op;
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
    use neovm_value::value::Value;
    force_mode_for_test(Some(BgMode::Threaded));
    force_stress_for_test(Some(true));
    force_deopt_for_test(false);
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let ctx = &mut ev as *mut Context;
    let mut functions = Vec::new();
    for i in 0..6 {
        let list = ev
            .eval_str(&format!(
                "(list 'jit-bg-gc-{i} (make-string {i} ?z) (cons {i} {i}))"
            ))
            .expect("list");
        let symbol = format!("jit-bg-gc-fn-{i}");
        let name = intern(&symbol);
        let f = function(vec![Op::Constant(0), Op::Return], vec![list], 0);
        // Bound, so the heap object (and through it the list) stays alive.
        ev.obarray
            .set_symbol_function_id(name, Value::make_bytecode(f));
        let expected = format!("(jit-bg-gc-{i} \"{}\" ({i} . {i}))", "z".repeat(i));
        functions.push((symbol, name, expected));
    }
    ev.gc_stress = true;
    let collections = ev.tagged_heap.gc_collections();
    for round in 0..40 {
        for (symbol, name, expected) in &functions {
            let bc = ev
                .obarray
                .symbol_function_id(*name)
                .and_then(|f| f.get_bytecode_data())
                .expect("bound");
            // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
            let bits =
                unsafe { crate::emacs_core::jit::try_run_compiled(ctx, bc, Value::NIL, &[]) }
                    .expect("no signal");
            let value = match bits {
                Some(bits) => unsafe {
                    /* SAFETY: the compiled-call Ok word is a real Value's tagged bits */
                    Value::from_bits(bits)
                },
                // Interpreted this call (cold, or pending).
                None => ev
                    .apply(Value::symbol(symbol), vec![])
                    .expect("interpreted"),
            };
            assert_eq!(&print_value(&value), expected, "round {round}");
            ev.eval_str("(make-list 64 (cons 1 2))").expect("churn");
        }
        if round == 5 {
            assert!(quiesce_for_test(Duration::from_secs(60)));
        }
    }
    ev.gc_stress = false;
    assert!(
        ev.tagged_heap.gc_collections() >= collections + 240,
        "a collection at least per call: {} -> {}",
        collections,
        ev.tagged_heap.gc_collections()
    );
    let stats = stats_snapshot();
    assert!(stats.installed[JobClass::Entry as usize] >= 6, "{stats:?}");
    force_stress_for_test(None);
    force_mode_for_test(None);
}
