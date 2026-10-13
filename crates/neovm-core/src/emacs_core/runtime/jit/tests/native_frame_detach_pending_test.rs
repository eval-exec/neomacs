//! Pending callees use indexed bytecode arguments, whose storage recovery
//! retires before the leaked cleanup runs. Their observable values must live.

use super::*;
use crate::emacs_core::jit::{bg, cache, compile};

struct HeldPending;

impl Drop for HeldPending {
    fn drop(&mut self) {
        // Cancel this mutator's jobs before its Context dies, even on failure.
        cache::clear();
        bg::force_mode_for_test(None);
        force_profit_gate_for_test(true);
    }
}

#[test]
fn recovery_retains_owned_and_flagged_indexed_args_until_unbind() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.gc_collect_exact();
    ev.bc_buf.extend([Value::make_int(11), Value::make_int(13)]);
    let _stable = ev.push_backtrace_frame_from_bc_stack(Value::symbol("stable-span"), 0, 2);
    ev.push_backtrace_frame_owned(
        Value::symbol("stable-owned"),
        smallvec::smallvec![
            Value::make_int(17),
            Value::make_int(19),
            Value::make_int(23)
        ],
    );
    let snapshot = ev.module_boundary_snapshot();
    let base = ev.specpdl.len();
    let args_base = ev.backtrace_args_stack_len_for_test();
    ev.push_backtrace_frame_owned(
        Value::symbol("retained-owned"),
        smallvec::smallvec![
            Value::string("owned-heap"),
            Value::cons(Value::make_int(29), Value::make_int(31)),
            Value::vector(vec![Value::make_int(37)])
        ],
    );
    ev.bc_buf.extend([Value::make_int(41), Value::make_int(43)]);
    let _flagged = ev.push_backtrace_frame_from_bc_stack(Value::symbol("flagged-span"), 2, 2);
    assert!(ev.set_backtrace_debug_on_exit(base + 1, true));

    let arguments = |ev: &Context| -> Vec<String> {
        ev.specpdl
            .iter()
            .map(|entry| {
                let SpecBinding::Backtrace { args, .. } = entry else {
                    panic!("fixture uses descriptors, including its flagged arity-two frame")
                };
                let values = ev.backtrace_args_values(args);
                format!(
                    "({})",
                    values
                        .iter()
                        .map(crate::emacs_core::print::print_value)
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            })
            .collect()
    };
    let expected = vec![
        "(11 13)",
        "(17 19 23)",
        "(\"owned-heap\" (29 . 31) [37])",
        "(41 43)",
    ];
    // A match-handler recovery followed by leaf-exit recovery can heal the
    // same snapshot twice; the first retained owned slots must survive again.
    for _ in 0..2 {
        ev.restore_jit_shim_boundary(&snapshot, snapshot.condition_len());
        assert_eq!(arguments(&ev), expected, "values before a later collection");
        assert!(ev.backtrace_frame_wants_debug_on_exit(base + 1));
        ev.gc_collect_exact();
        assert_eq!(arguments(&ev), expected, "owned copies remain GC roots");
    }
    assert!(ev.set_backtrace_debug_on_exit(base + 1, false));
    ev.unbind_to(base);
    assert_eq!(ev.backtrace_args_stack_len_for_test(), args_base);
    assert_eq!(arguments(&ev), &expected[..base]);
    ev.unbind_to(0);
    assert_eq!(ev.backtrace_args_stack_len_for_test(), 0);
}

#[test]
fn held_pending_callee_keeps_cleanup_backtrace_args_after_recovery() {
    crate::test_utils::init_test_tracing();
    // All Lisp state stays on this fresh mutator; only join's unit crosses
    // the thread boundary. Worker holds release on panic as well as success.
    std::thread::spawn(|| {
        let mut ev = Context::new();
        ev.gc_collect_exact();
        let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Legacy);
        bg::force_mode_for_test(Some(bg::BgMode::Threaded));
        force_profit_gate_for_test(false);
        force_deopt_for_test(false);
        let _hold = bg::hold_workers_for_test();
        let _scope = HeldPending;
        for (nargs, argument_form, expected) in [
            (2usize, "(list 41 42)", "((41 42))"),
            (
                2,
                "(list (copy-sequence \"kept-a\") (cons 73 79))",
                "((\"kept-a\" (73 . 79)))",
            ),
            (
                3,
                "(list (copy-sequence \"kept-b\") (cons 83 89) (vector 97))",
                "((\"kept-b\" (83 . 89) [97]))",
            ),
        ] {
            cache::clear();
            let name = format!("neo-detach-pending-{nargs}");
            ev.eval_str(&format!(
                "(progn
                   (setq neo-detach-seen 'unset)
                   (fset 'neo-detach-leaker
                         (function
                          (lambda ()
                            (unwind-protect
                                (neovm--internal-panic \"pending-leak-boom\")
                              (garbage-collect)
                              (setq neo-detach-seen {walk}))))))",
                walk = walk_form(&name)
            ))
            .expect("leaker");
            let callee = Value::make_bytecode(function(
                nargs,
                vec![Op::Constant(0), Op::Call(0), Op::Return],
                vec![Value::symbol("neo-detach-leaker")],
            ));
            ev.obarray.set_symbol_function_id(intern(&name), callee);
            ev.set_variable("neo-detach-obj", callee);
            // The native caller has no heap argument constants: after return,
            // only the retained callee frame may keep these arguments alive.
            let mut ops = vec![Op::Constant(0)];
            ops.extend((0..nargs).map(|_| Op::StackRef(nargs as u16)));
            ops.extend([Op::Call(nargs as u16), Op::Return]);
            let caller = function(nargs, ops, vec![Value::symbol(&name)]);
            let leaf = compile_bytecode_function_with(&caller, Some(&ev.obarray))
                .expect("caller compiles synchronously");
            let arguments = ev.eval_str(argument_form).expect("fresh arguments");
            let words: Vec<Value> = crate::emacs_core::value::list_to_vec(&arguments)
                .expect("argument list")
                .iter()
                .copied()
                .collect();
            let spec0 = ev.specpdl.len();
            let args0 = ev.backtrace_args_stack_len_for_test();
            let ctx_ptr = &mut ev as *mut Context as *mut u8;
            assert_eq!(leaf.call(ctx_ptr, &words), NativeRun::Signal);
            let id = callee
                .get_bytecode_data()
                .expect("callee")
                .jit_runtime()
                .compiled_id()
                .expect("callee compile requested");
            assert_eq!(
                cache::cache_entry_kind_for_test(id),
                "pending",
                "held worker guarantees the indexed bytecode fallback"
            );
            let flow = take_pending_flow().expect("contained panic");
            assert!(panic_message(&flow).contains("pending-leak-boom"));
            let collections_before_cleanup = ev.gc_count;
            ev.unbind_to(spec0);
            assert!(
                ev.gc_count > collections_before_cleanup,
                "the deferred cleanup must actually collect"
            );
            let seen = ev.eval_str("neo-detach-seen").expect("cleanup witness");
            assert_eq!(
                crate::emacs_core::print::print_value(&seen),
                expected,
                "pending fallback keeps all {nargs} arguments through cleanup GC"
            );
            assert_eq!(
                ev.backtrace_args_stack_len_for_test(),
                args0,
                "deferred unwind releases the retained owned arguments"
            );
        }
    })
    .join()
    .expect("pending cleanup regression mutator");
}
