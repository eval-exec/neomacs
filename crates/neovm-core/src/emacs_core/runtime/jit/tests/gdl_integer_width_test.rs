//! GNU bignum.c:92-100 rejects freshly computed oversized integers; GNU
//! eval.c:1965-1975 runs signal-hook-function before selecting a handler.
//! This test seeds numeric feedback so it exercises native generic arithmetic,
//! independently of tier-up thresholds or the inline/fuser environment.
use super::*;
use crate::emacs_core::error::FlowKind;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::value::LambdaParams;

#[test]
fn gdl_integer_width_native_signal_roots_operands_through_collecting_hook() {
    crate::test_utils::init_test_tracing();
    let _backend = opt_mode_scope_for_test(OptMode::Legacy);
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("gdl-width-left"), intern("gdl-width-right")],
        optional: Vec::new(),
        rest: None,
    });
    function.lexical = true;
    // Consume the entry operands themselves, leaving no residual stack that
    // could accidentally keep them alive for the register-operand root guard.
    function.ops = vec![Op::Mul, Op::Return];
    function.max_stack = 2;
    function
        .jit_runtime()
        .record_numeric(0, function.ops.len(), NumericFeedback::Other);
    let leaf = compile_bytecode_function(&function).expect("generic multiplication compiles");
    let mut eval = Context::new();
    eval.eval_str(
        "(setq integer-width 128
               gdl-width-hook-count 0
               signal-hook-function
               (lambda (symbol _data)
                 (if (eq symbol 'overflow-error)
                     (progn
                       (setq gdl-width-hook-count (1+ gdl-width-hook-count))
                       (garbage-collect)
                       (let ((n 0))
                         (while (< n 64) (expt 3 60) (setq n (1+ n))))))
                 nil))",
    )
    .expect("install collecting overflow hook");
    eval.bc_buf.push(Value::symbol("gdl-width-root-sentinel"));
    let saved_stack = eval.bc_buf.clone();

    for invocation in 1..=3 {
        let operands = eval
            .eval_str("(cons (expt 2 100) (1+ (expt 2 99)))")
            .expect("machine-width-floor operands");
        let args = [operands.cons_car(), operands.cons_cdr()];
        let expected = args.map(|value| crate::emacs_core::print::print_value(&value));
        let ctx_ptr = &mut eval as *mut Context as *mut u8;
        assert_eq!(
            leaf.call(ctx_ptr, &args),
            NativeRun::Signal,
            "must return native STATUS_SIGNAL rather than deopt or a value"
        );
        let flow = take_pending_flow().expect("native signal is stashed");
        let FlowKind::Signal(signal) = flow.into_kind() else {
            panic!("expected overflow-error signal");
        };
        assert_eq!(signal.symbol, intern("overflow-error"));
        assert!(signal.data.is_empty());
        assert_eq!(
            eval.obarray.symbol_value_copied("gdl-width-hook-count"),
            Some(Value::fixnum(invocation)),
            "the native error dispatched its collecting hook exactly once"
        );
        assert_eq!(
            eval.bc_buf, saved_stack,
            "temporary arithmetic roots unwind"
        );
        for (value, expected) in args.into_iter().zip(expected) {
            assert!(
                eval.tagged_heap.owns_heap_value_for_test(value),
                "operand remains allocated after the hook's exact collection"
            );
            // Check ownership before reading the payload: a missing root is a
            // deterministic assertion failure, never a read of freed storage.
            assert_eq!(crate::emacs_core::print::print_value(&value), expected);
        }
    }
}
