//! `BacktraceNative` lifetime on contained-panic paths: a native frame that
//! a contained panic leaves behind as residue must not keep reading its
//! caller's call-args slot after the caller leaf exits. GNU never has the
//! state -- `unwind_to_catch` unbinds before it longjmps (eval.c:1449, :1461)
//! -- so the frame is detached while the slot is still live: exact words for
//! one or two arguments, the function alone for any other count.

use super::*;
use crate::emacs_core::eval::{Context, SpecBinding};
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::LambdaParams;

fn function(nargs: usize, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=nargs).map(|i| SymId(i as u32)).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    f.seal_hand_assembled_ops();
    f
}

/// How the callee is entered and how its panic comes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// First call: the spec shim's slow half arms the site and runs the
    /// callee through `run_leaf_native_to_native`.
    Arming,
    /// Later calls: the armed fast path's raw entry, the exit through
    /// `call_spec_finish`.
    FastRaw,
    /// Later calls to a callee that binds a variable: the fast path's framed
    /// entry, whose own exit heals the callee's residue first.
    FastFramed,
}

/// The message of a contained panic's error signal.
fn panic_message(flow: &Flow) -> String {
    match flow {
        Flow::Signal(sig) => sig
            .data
            .first()
            .and_then(|v| v.as_str_owned())
            .unwrap_or_default(),
        other => format!("{other:?}"),
    }
}

/// Every entry above SPEC0 that records one of CALLEES as its function (a
/// speculated call records the callee object, a generic one the symbol), as
/// printed argument lists (the owned or compact shapes), panicking on a
/// `BacktraceNative` (the shape that reads a slot).
fn residue_frames(ev: &Context, spec0: usize, callees: &[Value]) -> Vec<String> {
    let ours = |function: &Value| callees.contains(function);
    let mut frames = Vec::new();
    for entry in &ev.specpdl[spec0..] {
        match entry {
            SpecBinding::BacktraceNative {
                function, nargs, ..
            } if ours(function) => {
                panic!("a {nargs}-argument native frame still reads its caller's dead slot")
            }
            SpecBinding::Backtrace1 { function, arg, .. } if ours(function) => {
                frames.push(format!("({})", crate::emacs_core::print::print_value(arg)));
            }
            SpecBinding::Backtrace2 {
                function,
                arg0,
                arg1,
            } if ours(function) => frames.push(format!(
                "({} {})",
                crate::emacs_core::print::print_value(arg0),
                crate::emacs_core::print::print_value(arg1)
            )),
            SpecBinding::Backtrace { function, .. } if ours(function) => {
                frames.push("function-only".to_string());
            }
            _ => {}
        }
    }
    frames
}

/// A Lisp backtrace walk collecting the argument lists of the frames whose
/// function is the symbol NAME or the object in `neo-detach-obj`.
fn walk_form(name: &str) -> String {
    format!(
        "(let ((out nil))
           (mapbacktrace
            (function (lambda (_evald f args _flags)
                        (if (or (eq f '{name}) (eq f neo-detach-obj))
                            (setq out (cons args out))))))
           out)"
    )
}

/// A hot compiled caller calls a compiled callee of NARGS arguments through
/// its spec site; the callee's call to a panicking builtin is contained in
/// the callee's own shim. After the contained panic reaches the caller's
/// exit, the caller's frame for the callee is still on the specpdl (the
/// deferred unwind retires it) -- and must no longer read the caller's
/// call-args slot, which is dead. Arity 2 keeps its exact words, 0 and 3
/// keep the function only. A collection and a backtrace walk with the
/// residue in place are then safe.
#[test]
fn contained_panic_detaches_native_frames_before_the_caller_exits() {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    for nargs in [0usize, 2, 3] {
        for framed in [false, true] {
            let mut ev = Context::new();
            ev.gc_collect_exact();
            let ctx_ptr = &mut ev as *mut Context as *mut u8;
            let name = format!("neo-detach-callee-{nargs}-{framed}");
            let callee_sym = Value::symbol(&name);
            // (lambda (a..) (neovm--internal-panic "detach-boom")), or with a
            // dynamic binding around the call so the callee runs framed.
            let (ops, constants) = if framed {
                (
                    vec![
                        Op::Constant(3),
                        Op::VarBind(2),
                        Op::Constant(0),
                        Op::Constant(1),
                        Op::Call(1),
                        Op::Unbind(1),
                        Op::Return,
                    ],
                    vec![
                        Value::symbol("neovm--internal-panic"),
                        Value::string("detach-boom"),
                        Value::symbol("neo-detach-dynvar"),
                        Value::make_int(5),
                    ],
                )
            } else {
                (
                    vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
                    vec![
                        Value::symbol("neovm--internal-panic"),
                        Value::string("detach-boom"),
                    ],
                )
            };
            let callee = Value::make_bytecode(function(nargs, ops, constants));
            ev.obarray.set_symbol_function_id(intern(&name), callee);
            ev.set_variable("neo-detach-obj", callee);
            // (lambda () (CALLEE 41 42 43 ...)): unframed, so its exit heals
            // the boundary but leaves the specpdl residue to the deferred
            // unwind -- the window this test inspects.
            let mut caller_ops = vec![Op::Constant(0)];
            let mut caller_consts = vec![callee_sym];
            for i in 0..nargs {
                caller_ops.push(Op::Constant(1 + i as u16));
                caller_consts.push(Value::make_int(41 + i as i64));
            }
            caller_ops.push(Op::Call(nargs as u16));
            caller_ops.push(Op::Return);
            let caller = function(0, caller_ops, caller_consts);
            let leaf =
                compile_bytecode_function_with(&caller, Some(&ev.obarray)).expect("compiles");
            let routes: &[Route] = if framed {
                &[Route::Arming, Route::FastFramed]
            } else {
                &[Route::Arming, Route::FastRaw]
            };
            for &route in routes {
                let case = format!("{nargs} arguments, {route:?}");
                let spec0 = ev.specpdl.len();
                let fast0 = SPEC_SHIM_FAST_COUNT.load(Ordering::Relaxed);
                assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal, "{case}");
                if route != Route::Arming {
                    assert!(
                        SPEC_SHIM_FAST_COUNT.load(Ordering::Relaxed) > fast0,
                        "{case}: the fast path ran"
                    );
                }
                let frames = residue_frames(&ev, spec0, &[callee_sym, callee]);
                let want = match nargs {
                    2 => "(41 42)",
                    _ => "function-only",
                };
                // A framed callee's own exit heals its residue first, so on
                // the arming route the frame is the balanced top again and
                // pops; the framed fast path leaves it (detached) for the
                // caller's healing, like the raw routes.
                let popped = framed && route == Route::Arming;
                let want_frames: Vec<String> = if popped {
                    Vec::new()
                } else {
                    vec![want.to_string()]
                };
                assert_eq!(frames, want_frames, "{case}");
                // The dispatcher's take allocates the message; collect with
                // the residue in place, then walk it from Lisp.
                let flow = take_pending_flow().expect("the contained panic stashes a flow");
                assert!(
                    panic_message(&flow).contains("detach-boom"),
                    "{case}: {flow:?}"
                );
                ev.gc_collect_exact();
                let seen = ev.eval_str(&walk_form(&name)).expect("backtrace walk");
                let want_seen = match (popped, nargs) {
                    (true, _) => "nil",
                    (false, 2) => "((41 42))",
                    (false, _) => "(nil)",
                };
                assert_eq!(
                    crate::emacs_core::print::print_value(&seen),
                    want_seen,
                    "{case}: the walk reads the detached frame"
                );
                ev.unbind_to(spec0);
                assert_eq!(ev.specpdl.len(), spec0);
            }
        }
    }
}

/// The realistic reader: an `unwind-protect` the panicked extent leaked
/// ABOVE the caller's frame, whose cleanup the enclosing unwind runs after
/// the caller leaf has exited -- with a collection (under GC stress) and a
/// backtrace walk inside the cleanup. The caller's frame for the callee is
/// below that record, so it is still on the specpdl while the cleanup runs.
#[test]
fn a_leaked_cleanup_after_a_contained_panic_reads_only_detached_frames() {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    for nargs in [2usize, 3] {
        let mut ev = Context::new();
        // A first collection outside native code moves the JIT cache onto
        // this Context's heap (the stress collections below run under native
        // leaves, where the cache must not be cleared).
        ev.gc_collect_exact();
        let ctx_ptr = &mut ev as *mut Context as *mut u8;
        let name = format!("neo-detach-leak-callee-{nargs}");
        let callee_sym = Value::symbol(&name);
        ev.eval_str(&format!(
            "(progn
               (setq neo-detach-seen 'unset)
               (fset 'neo-detach-leaker
                     (function
                      (lambda ()
                        (unwind-protect
                            (neovm--internal-panic \"leak-boom\")
                          (garbage-collect)
                          (setq neo-detach-seen {walk}))))))",
            walk = walk_form(&name)
        ))
        .expect("leaker");
        // (lambda (a b [c]) (neo-detach-leaker)): the interpreted leaker's
        // unwind-protect record is pushed above this callee's frame and
        // skipped by the panic.
        let callee = Value::make_bytecode(function(
            nargs,
            vec![Op::Constant(0), Op::Call(0), Op::Return],
            vec![Value::symbol("neo-detach-leaker")],
        ));
        ev.obarray.set_symbol_function_id(intern(&name), callee);
        ev.set_variable("neo-detach-obj", callee);
        let mut caller_ops = vec![Op::Constant(0)];
        let mut caller_consts = vec![callee_sym];
        for i in 0..nargs {
            caller_ops.push(Op::Constant(1 + i as u16));
            caller_consts.push(Value::make_int(41 + i as i64));
        }
        caller_ops.push(Op::Call(nargs as u16));
        caller_ops.push(Op::Return);
        let caller = function(0, caller_ops, caller_consts);
        let leaf = compile_bytecode_function_with(&caller, Some(&ev.obarray)).expect("compiles");
        ev.gc_stress = true;
        for round in 0..2 {
            let case = format!("{nargs} arguments, round {round}");
            let spec0 = ev.specpdl.len();
            assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal, "{case}");
            let flow = take_pending_flow().expect("the contained panic stashes a flow");
            assert!(
                panic_message(&flow).contains("leak-boom"),
                "{case}: {flow:?}"
            );
            // The enclosing frame's unwind: the leaked cleanup runs here.
            ev.unbind_to(spec0);
            let seen = ev.eval_str("neo-detach-seen").expect("witness");
            let want = if nargs == 2 { "((41 42))" } else { "(nil)" };
            assert_eq!(
                crate::emacs_core::print::print_value(&seen),
                want,
                "{case}: the cleanup saw the caller's frame, detached"
            );
            ev.eval_str("(setq neo-detach-seen 'unset)").unwrap();
        }
        ev.gc_stress = false;
    }
}

/// A Rust panic that unwinds through a native pusher, inside a shim wrapped
/// with the `detach` arm, leaves the frame detached from the shim's slot
/// (which dies when the shim's JIT caller exits).
#[test]
fn a_panic_unwinding_through_a_native_pusher_detaches_its_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let func = Value::symbol("neo-detach-unwound");
    fn shim(ctx: *mut u8, func: Value, args_ptr: *const i64, nargs: usize) -> i64 {
        jit_shim_contain!(detach args_ptr, ctx, STATUS_SIGNAL, {
            // SAFETY: the test's live Context.
            let ctx = unsafe { &mut *(ctx as *mut Context) };
            // SAFETY: the test's slot outlives the shim.
            unsafe { ctx.push_backtrace_frame_from_native_args(func, args_ptr, nargs) };
            panic!("unwound-boom");
        })
    }
    for nargs in [0usize, 3, 4] {
        let slot: Vec<i64> = (0..nargs)
            .map(|i| Value::make_int(i as i64 + 1).bits() as i64)
            .collect();
        let spec0 = ev.specpdl.len();
        assert_eq!(shim(ctx_ptr, func, slot.as_ptr(), nargs), STATUS_SIGNAL);
        assert!(shim_panic_pending());
        assert_eq!(
            residue_frames(&ev, spec0, &[func]),
            vec!["function-only".to_string()],
            "{nargs} arguments"
        );
        let _ = take_pending_flow();
        ev.unbind_to(spec0);
    }
}

/// `apply` into a compiled callee spreads its arguments into a buffer of
/// `call_apply_native`'s own frame, so the callee's backtrace frame records
/// a copy (`CalleeFrameArgs::Local`), never a pointer into that buffer.
#[test]
fn apply_native_callee_frame_owns_its_spread_arguments() {
    use crate::emacs_core::bytecode::vm::{APPLY_CALLEE_FRAME_BORROWS, APPLY_NATIVE_CALLS};
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    let mut ev = Context::new();
    for nargs in [2usize, 3, 5] {
        let f = function(nargs, vec![Op::List(nargs as u16), Op::Return], vec![]);
        f.jit_runtime().set_hot_for_test();
        let callee = Value::make_bytecode(f);
        crate::emacs_core::eval::push_scratch_gc_root(callee);
        // (lambda (f l) (apply f 1 l))
        let caller = function(
            2,
            vec![
                Op::Constant(0),
                Op::StackRef(2),
                Op::Constant(1),
                Op::StackRef(3),
                Op::Call(3),
                Op::Return,
            ],
            vec![Value::symbol("apply"), Value::make_int(1)],
        );
        caller.jit_runtime().set_hot_for_test();
        let caller = Value::make_bytecode(caller);
        crate::emacs_core::eval::push_scratch_gc_root(caller);
        let rest: Vec<String> = (2..=nargs).map(|n| n.to_string()).collect();
        let list = ev
            .eval_str(&format!("(list {})", rest.join(" ")))
            .expect("list");
        let want: Vec<String> = (1..=nargs).map(|n| n.to_string()).collect();
        for _ in 0..3 {
            let _ = ev.funcall_general_untraced(caller, vec![callee, list]);
        }
        APPLY_CALLEE_FRAME_BORROWS.with(|c| c.set(None));
        let before = APPLY_NATIVE_CALLS.with(|c| c.get());
        let got = ev
            .funcall_general_untraced(caller, vec![callee, list])
            .expect("apply");
        assert_eq!(
            crate::emacs_core::print::print_value(&got),
            format!("({})", want.join(" ")),
            "{nargs} arguments"
        );
        assert_eq!(
            APPLY_NATIVE_CALLS.with(|c| c.get()) - before,
            1,
            "{nargs} arguments: apply ran natively"
        );
        assert_eq!(
            APPLY_CALLEE_FRAME_BORROWS.with(|c| c.get()),
            Some(false),
            "{nargs} arguments: the callee's frame copies the spread"
        );
    }
}
