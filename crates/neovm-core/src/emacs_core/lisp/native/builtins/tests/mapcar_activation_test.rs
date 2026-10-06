//! Mapping activation specialization preserves the existing callback protocol.

use super::{MapCallee, builtin_mapc_2, builtin_mapcan, builtin_mapcar_2, builtin_mapconcat};
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::error::{EvalResult, FlowKind, FlowResultExt};
use crate::emacs_core::eval::{Context, SpecBinding};
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::cache;
use crate::emacs_core::print::print_value;
use crate::emacs_core::subr::{FixedMin1, SubrSpec};
use crate::emacs_core::value::{LambdaParams, Value};
use crate::tagged::collection_reads::capture;
use crate::tagged::mutate::LispCollectionRevision;

#[derive(Clone, Copy, Debug)]
enum Callback {
    NativeSymbol,
    NativeObject,
    Interpreted,
    ByteCode,
    ByteCodeSymbol,
}

const CALLBACKS: [Callback; 5] = [
    Callback::NativeSymbol,
    Callback::NativeObject,
    Callback::Interpreted,
    Callback::ByteCode,
    Callback::ByteCodeSymbol,
];

fn context() -> Context {
    crate::test_utils::init_test_tracing();
    Context::new()
}

fn identity(_: &mut Context, item: Value) -> EvalResult {
    Ok(item)
}

fn bytecode(target: Option<Value>) -> Value {
    let mut code = ByteCodeFunction::new(LambdaParams::simple(vec![intern("mapact-item")]));
    code.lexical = true;
    code.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    code.ops = match target {
        Some(target) => {
            code.constants = vec![target].into();
            vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return]
        }
        None => vec![Op::StackRef(0), Op::Return],
    };
    code.jit_runtime().set_hot_for_test();
    Value::make_bytecode(code)
}

fn callback(ctx: &mut Context, kind: Callback) -> Value {
    match kind {
        Callback::NativeSymbol | Callback::NativeObject => {
            ctx.register_subr(SubrSpec::fixed1(
                "mapact-identity",
                identity,
                FixedMin1::One,
            ));
            if matches!(kind, Callback::NativeSymbol) {
                Value::symbol("mapact-identity")
            } else {
                ctx.obarray().symbol_function("mapact-identity").unwrap()
            }
        }
        Callback::Interpreted => ctx.eval_str("(lambda (x) x)").unwrap(),
        Callback::ByteCode => bytecode(None),
        Callback::ByteCodeSymbol => {
            let function = bytecode(None);
            ctx.obarray_mut()
                .set_symbol_function("mapact-aliased-bytecode", function);
            Value::symbol("mapact-aliased-bytecode")
        }
    }
}

fn sequence(vector: bool, items: Vec<Value>) -> Value {
    if vector {
        Value::vector(items)
    } else {
        Value::list(items)
    }
}

#[test]
fn mapcar_activation_mapcar_preserves_all_callback_kinds_for_lists_and_vectors() {
    for kind in CALLBACKS {
        let mut ctx = context();
        let roots = ctx.save_vm_roots();
        let function = callback(&mut ctx, kind);
        ctx.push_vm_frame_root(function);
        for vector in [false, true] {
            let input = sequence(vector, vec![Value::fixnum(11), Value::fixnum(22)]);
            ctx.push_vm_frame_root(input);
            let before = (ctx.depth, ctx.specpdl.len());
            for _ in 0..3 {
                let output = builtin_mapcar_2(&mut ctx, function, input).unwrap();
                assert_eq!(print_value(&output), "(11 22)", "{kind:?}, vector={vector}");
                assert_eq!((ctx.depth, ctx.specpdl.len()), before);
            }
        }
        ctx.restore_vm_roots(roots);
    }
}

#[test]
fn mapcar_activation_mapc_preserves_input_identity_for_every_callback_kind() {
    for kind in CALLBACKS {
        let mut ctx = context();
        let roots = ctx.save_vm_roots();
        let function = callback(&mut ctx, kind);
        ctx.push_vm_frame_root(function);
        for vector in [false, true] {
            let input = sequence(vector, vec![Value::fixnum(11), Value::fixnum(22)]);
            ctx.push_vm_frame_root(input);
            assert_eq!(builtin_mapc_2(&mut ctx, function, input).unwrap(), input);
        }
        ctx.restore_vm_roots(roots);
    }
}

#[test]
fn mapcar_activation_mapcan_retains_generic_callback_and_nconc_semantics() {
    for kind in CALLBACKS {
        let mut ctx = context();
        let roots = ctx.save_vm_roots();
        let function = callback(&mut ctx, kind);
        ctx.push_vm_frame_root(function);
        for vector in [false, true] {
            let first = Value::list(vec![Value::fixnum(11)]);
            let second = Value::list(vec![Value::fixnum(22)]);
            let input = sequence(vector, vec![first, second]);
            ctx.push_vm_frame_root(input);
            let output = builtin_mapcan(&mut ctx, vec![function, input]).unwrap();
            assert_eq!(output, first, "nconc reuses the first callback result");
            assert_eq!(print_value(&output), "(11 22)", "{kind:?}, vector={vector}");
        }
        ctx.restore_vm_roots(roots);
    }
}

#[test]
fn mapcar_activation_mapconcat_preserves_all_callback_kinds_and_separator() {
    for kind in CALLBACKS {
        let mut ctx = context();
        let roots = ctx.save_vm_roots();
        let function = callback(&mut ctx, kind);
        ctx.push_vm_frame_root(function);
        for vector in [false, true] {
            let input = sequence(vector, vec![Value::string("one"), Value::string("two")]);
            ctx.push_vm_frame_root(input);
            let output =
                builtin_mapconcat(&mut ctx, vec![function, input, Value::string("-")]).unwrap();
            assert_eq!(
                output.as_utf8_str(),
                Some("one-two"),
                "{kind:?}, vector={vector}"
            );
        }
        ctx.restore_vm_roots(roots);
    }
}

#[test]
fn mapcar_activation_copied_bytecode_uses_existing_entry_with_jit_disabled() {
    struct RestoreJit(bool);
    impl Drop for RestoreJit {
        fn drop(&mut self) {
            crate::emacs_core::jit::force_jit_off_for_test(self.0);
        }
    }
    let _restore = RestoreJit(crate::emacs_core::jit::jit_forced_off_for_test());
    // A scoped thread-local control, never a mutable process environment knob.
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut ctx = context();
    let roots = ctx.save_vm_roots();
    let function = bytecode(None);
    function
        .get_bytecode_data()
        .unwrap()
        .jit_runtime()
        .set_heat_for_test(0);
    ctx.push_vm_frame_root(function);
    for vector in [false, true] {
        let input = sequence(vector, vec![Value::fixnum(11), Value::fixnum(22)]);
        ctx.push_vm_frame_root(input);
        assert_eq!(
            print_value(&builtin_mapcar_2(&mut ctx, function, input).unwrap()),
            "(11 22)"
        );
        assert_eq!(builtin_mapc_2(&mut ctx, function, input).unwrap(), input);
        let strings = sequence(vector, vec![Value::string("one"), Value::string("two")]);
        ctx.push_vm_frame_root(strings);
        assert_eq!(
            builtin_mapconcat(&mut ctx, vec![function, strings, Value::string("-")])
                .unwrap()
                .as_utf8_str(),
            Some("one-two")
        );
        let lists = sequence(
            vector,
            vec![
                Value::list(vec![Value::fixnum(11)]),
                Value::list(vec![Value::fixnum(22)]),
            ],
        );
        ctx.push_vm_frame_root(lists);
        assert_eq!(
            print_value(&builtin_mapcan(&mut ctx, vec![function, lists]).unwrap()),
            "(11 22)"
        );
    }
    let runtime = function.get_bytecode_data().unwrap().jit_runtime();
    assert_eq!(
        runtime.heat(),
        0,
        "the unchanged interpreter-only entry performs no tier heat bookkeeping"
    );
    assert!(runtime.armed_leaf_slot(cache::leaf_slot_epoch()).is_none());
    ctx.restore_vm_roots(roots);
}

#[test]
fn mapcar_activation_nested_bytecode_and_interpreted_callbacks_keep_owner_frames() {
    let mut ctx = context();
    ctx.eval_str("(fset 'mapact-nested (lambda (x) (mapcar (lambda (y) (+ y 1)) (list x x))))")
        .unwrap();
    let roots = ctx.save_vm_roots();
    let function = bytecode(Some(Value::symbol("mapact-nested")));
    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    ctx.push_vm_frame_root(function);
    ctx.push_vm_frame_root(input);
    let before = (ctx.depth, ctx.specpdl.len());
    for _ in 0..3 {
        let output = builtin_mapcar_2(&mut ctx, function, input).unwrap();
        assert_eq!(print_value(&output), "((2 2) (3 3))");
        assert_eq!((ctx.depth, ctx.specpdl.len()), before);
    }
    ctx.restore_vm_roots(roots);
}

#[test]
fn mapcar_activation_outer_observation_retains_bytecode_read_certificate() {
    let mut ctx = context();
    let roots = ctx.save_vm_roots();
    let function = bytecode(None);
    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    ctx.push_vm_frame_root(function);
    ctx.push_vm_frame_root(input);
    let (result, reads) = capture(|| {
        assert!(matches!(
            MapCallee::resolve(&mut ctx, function),
            MapCallee::Generic(_)
        ));
        builtin_mapcar_2(&mut ctx, function, input)
    });
    assert_eq!(print_value(&result.unwrap()), "(1 2)");
    let reads = reads.expect("observed mapping retains its read certificate");
    assert!(reads.unchanged());
    LispCollectionRevision::changed(function);
    assert!(
        !reads.unchanged(),
        "observed callback metadata stays in the certificate"
    );
    assert!(!crate::tagged::collection_reads::is_active());
    ctx.restore_vm_roots(roots);
}

#[test]
fn mapcar_activation_bytecode_redefinition_is_seen_by_the_next_element() {
    let mut ctx = context();
    ctx.eval_str("(progn (fset 'mapact-target (lambda (x) x)) (fset 'mapact-redefine (lambda (x) (let ((answer (funcall 'mapact-target x))) (fset 'mapact-target (lambda (x) (+ x 100))) answer))))").unwrap();
    let roots = ctx.save_vm_roots();
    let function = bytecode(Some(Value::symbol("mapact-redefine")));
    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    ctx.push_vm_frame_root(function);
    ctx.push_vm_frame_root(input);
    assert_eq!(
        print_value(&builtin_mapcar_2(&mut ctx, function, input).unwrap()),
        "(1 102 103)"
    );
    ctx.restore_vm_roots(roots);
}

#[test]
fn mapcar_activation_redefinition_of_mapping_callback_symbol_takes_effect_next_element() {
    let mut ctx = context();
    ctx.eval_str(
        "(fset 'mapact-redefine-own-symbol
               (lambda (x)
                 (fset 'mapact-redefine-own-symbol (lambda (y) (+ y 100)))
                 x))",
    )
    .expect("mapping callback that replaces its own function cell");
    let roots = ctx.save_vm_roots();
    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    ctx.push_vm_frame_root(input);
    let before = (ctx.depth, ctx.specpdl.len());
    assert_eq!(
        print_value(
            &builtin_mapcar_2(&mut ctx, Value::symbol("mapact-redefine-own-symbol"), input)
                .unwrap()
        ),
        "(1 102 103)",
        "the mapping designator is reread; only a direct bytecode identity is copied"
    );
    assert_eq!((ctx.depth, ctx.specpdl.len()), before);
    ctx.restore_vm_roots(roots);
}

fn replacement(_: &mut Context, item: Value) -> EvalResult {
    Ok(Value::fixnum(item.as_fixnum().unwrap() + 100))
}

fn redefine_native(ctx: &mut Context, item: Value) -> EvalResult {
    ctx.register_subr(SubrSpec::fixed1(
        "mapact-redefining-native",
        replacement,
        FixedMin1::One,
    ));
    Ok(item)
}

#[test]
fn mapcar_activation_checked_native_cache_still_invalidates_next_element() {
    let mut ctx = context();
    ctx.register_subr(SubrSpec::fixed1(
        "mapact-redefining-native",
        redefine_native,
        FixedMin1::One,
    ));
    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    assert_eq!(
        print_value(
            &builtin_mapcar_2(&mut ctx, Value::symbol("mapact-redefining-native"), input).unwrap()
        ),
        "(1 102 103)"
    );
}

#[test]
fn mapcar_activation_checked_native_metadata_reads_stay_activation_bounded() {
    use crate::emacs_core::eval::{
        global_subr_lookup_count, native_callback_cache_enabled, reset_global_subr_lookup_count,
    };
    let mut ctx = context();
    ctx.register_subr(SubrSpec::fixed1(
        "mapact-cache-floor",
        identity,
        FixedMin1::One,
    ));
    let roots = ctx.save_vm_roots();
    let short = Value::list(vec![Value::fixnum(0)]);
    let long = Value::list((0..64).map(Value::fixnum).collect());
    ctx.push_vm_frame_root(short);
    ctx.push_vm_frame_root(long);
    let mut counts = Vec::new();
    for input in [short, long] {
        reset_global_subr_lookup_count();
        let result =
            builtin_mapcar_2(&mut ctx, Value::symbol("mapact-cache-floor"), input).unwrap();
        counts.push(global_subr_lookup_count());
        assert_eq!(print_value(&result), print_value(&input));
    }
    if native_callback_cache_enabled() {
        assert_eq!(
            counts[0], counts[1],
            "metadata lookup work must not grow with callback count"
        );
    }
    // No environment is rewritten: the cache's off control remains reachable.
    ctx.restore_vm_roots(roots);
}

#[test]
fn mapcar_activation_debugger_runs_before_bytecode_callback() {
    let mut ctx = context();
    ctx.eval_str("(setq mapact-debug-calls nil debugger (lambda (&rest args) (if (eq (car args) 'exit) (car (cdr args)) (setq mapact-debug-calls (cons args mapact-debug-calls)) nil)))").unwrap();
    let roots = ctx.save_vm_roots();
    let function = bytecode(None);
    let input = Value::list(vec![Value::fixnum(11), Value::fixnum(22)]);
    ctx.push_vm_frame_root(function);
    ctx.push_vm_frame_root(input);
    ctx.assign("debug-on-next-call", Value::T);
    assert_eq!(
        print_value(&builtin_mapcar_2(&mut ctx, function, input).unwrap()),
        "(11 22)"
    );
    assert_eq!(
        print_value(
            &ctx.obarray()
                .symbol_value_copied("mapact-debug-calls")
                .unwrap()
        ),
        "((lambda))"
    );
    ctx.restore_vm_roots(roots);
}

#[test]
fn mapcar_activation_debug_on_entry_preserves_backtrace_and_next_element_redefinition() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.eval_str("(require 'debug)")
        .expect("load the actual GNU debug-on-entry implementation");
    ctx.register_subr(SubrSpec::fixed1(
        "mapact-debug-entry",
        identity,
        FixedMin1::One,
    ));
    let roots = ctx.save_vm_roots();
    let function = bytecode(Some(Value::symbol("mapact-debug-entry")));
    ctx.push_vm_frame_root(function);
    // Warm the existing inner named-call entry before advice changes its cell.
    assert_eq!(
        ctx.apply1_bytecode_unobserved(function, Value::fixnum(7))
            .unwrap(),
        Value::fixnum(7)
    );
    ctx.eval_str(
        "(setq mapact-entry-calls nil mapact-entry-args nil
               debugger
               (lambda (&rest args)
                 (setq mapact-entry-calls (cons args mapact-entry-calls))
                 (mapbacktrace
                   (lambda (_evaluated function arguments _flags)
                     (if (eq function 'mapact-debug-entry)
                         (setq mapact-entry-args arguments))))
                 (fset 'mapact-debug-entry (lambda (x) (+ x 100)))
                 nil))",
    )
    .expect("a debugger that observes the live frame and redefines its function");
    ctx.eval_str("(debug-on-entry 'mapact-debug-entry)")
        .expect("install actual debug-on-entry advice");
    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    ctx.push_vm_frame_root(input);
    let before = (ctx.depth, ctx.specpdl.len());
    assert_eq!(
        print_value(&builtin_mapcar_2(&mut ctx, function, input).unwrap()),
        "(1 102 103)",
        "advice continues its copied original; the next element sees fset"
    );
    assert_eq!((ctx.depth, ctx.specpdl.len()), before);
    let calls = ctx
        .obarray()
        .symbol_value_copied("mapact-entry-calls")
        .unwrap();
    assert_eq!(
        calls.cons_cdr(),
        Value::NIL,
        "fset cancels entry advice for later elements"
    );
    assert_eq!(calls.cons_car().cons_car(), Value::symbol("debug"));
    assert_eq!(
        print_value(
            &ctx.obarray()
                .symbol_value_copied("mapact-entry-args")
                .unwrap()
        ),
        "(1)",
        "the debugger sees the current callback's argument"
    );
    ctx.restore_vm_roots(roots);
}

fn collect_with_backtrace(ctx: &mut Context, item: Value) -> EvalResult {
    assert!(ctx.specpdl.iter().any(|binding| matches!(binding, SpecBinding::Backtrace1 { function, arg, .. } if function.is_bytecode() && *arg == item)), "bytecode callback frame must retain its argument before a nested native call");
    ctx.gc_collect_exact();
    assert!(
        item.as_utf8_str().is_some(),
        "the callback argument survives collection"
    );
    Ok(item)
}

#[test]
fn mapcar_activation_forced_gc_keeps_bytecode_backtrace_and_result_roots() {
    let mut ctx = context();
    ctx.register_subr(SubrSpec::fixed1(
        "mapact-collect",
        collect_with_backtrace,
        FixedMin1::One,
    ));
    let roots = ctx.save_vm_roots();
    let function = bytecode(Some(Value::symbol("mapact-collect")));
    let input = Value::list(vec![Value::string("first"), Value::string("second")]);
    ctx.push_vm_frame_root(function);
    ctx.push_vm_frame_root(input);
    let before = (ctx.depth, ctx.specpdl.len());
    let output = builtin_mapcar_2(&mut ctx, function, input).unwrap();
    assert_eq!(print_value(&output), "(\"first\" \"second\")");
    assert_eq!((ctx.depth, ctx.specpdl.len()), before);
    ctx.restore_vm_roots(roots);
}

fn collect_then_allocate_fresh_result(ctx: &mut Context, item: Value) -> EvalResult {
    // Only the mapping sink can retain an earlier return value: the source
    // sequence contains fixnums, and this callback stores no results elsewhere.
    ctx.gc_collect_exact();
    Ok(Value::string(format!(
        "fresh-{}",
        item.as_fixnum().unwrap()
    )))
}

#[test]
fn mapcar_activation_gc_preserves_independent_fresh_completed_results() {
    let mut ctx = context();
    ctx.register_subr(SubrSpec::fixed1(
        "mapact-fresh-result",
        collect_then_allocate_fresh_result,
        FixedMin1::One,
    ));
    let roots = ctx.save_vm_roots();
    let function = bytecode(Some(Value::symbol("mapact-fresh-result")));
    let input = Value::list(vec![Value::fixnum(0), Value::fixnum(1), Value::fixnum(2)]);
    ctx.push_vm_frame_root(function);
    ctx.push_vm_frame_root(input);
    let collections = ctx.gc_count;
    let before = (ctx.depth, ctx.specpdl.len());
    let result = builtin_mapcar_2(&mut ctx, function, input).unwrap();
    ctx.push_vm_frame_root(result);
    assert!(ctx.gc_count > collections);
    assert_eq!(
        print_value(&result),
        "(\"fresh-0\" \"fresh-1\" \"fresh-2\")"
    );
    ctx.gc_collect_exact();
    assert_eq!(
        print_value(&result),
        "(\"fresh-0\" \"fresh-1\" \"fresh-2\")"
    );
    // mapconcat uses the distinct Collect sink rather than mapcar's RootSlots.
    let concatenated =
        builtin_mapconcat(&mut ctx, vec![function, input, Value::string("|")]).unwrap();
    assert_eq!(concatenated.as_utf8_str(), Some("fresh-0|fresh-1|fresh-2"));
    assert_eq!((ctx.depth, ctx.specpdl.len()), before);
    ctx.restore_vm_roots(roots);
}

#[test]
fn mapcar_activation_signal_and_throw_restore_callback_frontiers() {
    let mut ctx = context();
    for (helper, throws) in [
        ("(lambda (x) (signal 'error (list x)))", false),
        ("(lambda (x) (throw 'mapact-tag x))", true),
    ] {
        let target = ctx.eval_str(helper).unwrap();
        let roots = ctx.save_vm_roots();
        ctx.push_vm_frame_root(target);
        let function = bytecode(Some(target));
        let input = Value::list(vec![Value::string("rooted argument")]);
        ctx.push_vm_frame_root(function);
        ctx.push_vm_frame_root(input);
        let before = (ctx.depth, ctx.specpdl.len());
        if throws {
            // A direct uncaught throw may be validated as no-catch. Exercise
            // the actual nonlocal exit through an outer interpreter catch.
            ctx.assign("mapact-throw-function", function);
            ctx.assign("mapact-throw-input", input);
            let answer = ctx
                .eval_str("(catch 'mapact-tag (mapcar mapact-throw-function mapact-throw-input))")
                .expect("the outer catch receives the inner bytecode callback's throw");
            assert_eq!(answer.as_utf8_str(), Some("rooted argument"));
        } else {
            let flow = builtin_mapcar_2(&mut ctx, function, input)
                .kinded()
                .unwrap_err();
            assert!(matches!(flow, FlowKind::Signal(_)));
        }
        assert_eq!((ctx.depth, ctx.specpdl.len()), before);
        assert_eq!(input.cons_car().as_utf8_str(), Some("rooted argument"));
        ctx.restore_vm_roots(roots);
    }
}

#[test]
fn mapcar_activation_reads_list_cdr_after_callback_mutation() {
    let mut ctx = context();
    ctx.eval_str("(fset 'mapact-shorten (lambda (x) (if (= x 1) (setcdr mapact-sequence nil)) x))")
        .unwrap();
    let roots = ctx.save_vm_roots();
    let function = bytecode(Some(Value::symbol("mapact-shorten")));
    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    ctx.push_vm_frame_root(function);
    ctx.push_vm_frame_root(input);
    ctx.assign("mapact-sequence", input);
    assert_eq!(
        print_value(&builtin_mapcar_2(&mut ctx, function, input).unwrap()),
        "(1)"
    );
    ctx.restore_vm_roots(roots);
}

#[test]
fn mapcar_activation_retier_and_cache_retirement_use_the_existing_entry() {
    let mut ctx = context();
    let roots = ctx.save_vm_roots();
    let function = bytecode(None);
    let input = Value::list(vec![Value::fixnum(11), Value::fixnum(22)]);
    ctx.push_vm_frame_root(function);
    ctx.push_vm_frame_root(input);
    assert_eq!(
        print_value(&builtin_mapcar_2(&mut ctx, function, input).unwrap()),
        "(11 22)"
    );
    let code = function.get_bytecode_data().unwrap();
    if crate::emacs_core::jit::jit_runtime_enabled() {
        assert!(
            code.jit_runtime()
                .armed_leaf_slot(cache::leaf_slot_epoch())
                .is_some(),
            "the warmed callback must exercise an armed leaf"
        );
    }
    if crate::emacs_core::jit::jit_runtime_enabled()
        && let Some(at) = crate::emacs_core::jit::retier_heat()
    {
        // The existing stack-call probe bumps at the crossing and declines;
        // its fallback dispatcher also accounts for the call. Compare with
        // that unchanged entry instead of assuming one heat unit per item.
        let control = bytecode(None);
        ctx.push_vm_frame_root(control);
        for item in [11, 22] {
            assert_eq!(
                ctx.apply1_bytecode_unobserved(control, Value::fixnum(item))
                    .unwrap(),
                Value::fixnum(item)
            );
        }
        let control_code = control.get_bytecode_data().unwrap();
        assert!(
            control_code
                .jit_runtime()
                .armed_leaf_slot(cache::leaf_slot_epoch())
                .is_some(),
            "the control must use the same warmed admission shape"
        );
        let source_id = code.jit_runtime().compiled_id().unwrap();
        let control_id = control_code.jit_runtime().compiled_id().unwrap();
        assert_ne!(
            source_id, control_id,
            "the control is an independent source"
        );
        let initial_allocator = cache::compiled_regalloc_for_test(source_id);
        assert!(initial_allocator.is_some());
        assert_eq!(
            initial_allocator,
            cache::compiled_regalloc_for_test(control_id)
        );
        assert!(matches!(
            MapCallee::resolve(&mut ctx, function),
            MapCallee::UnobservedByteCode(copied) if copied == function
        ));
        let epoch_before = cache::leaf_slot_epoch();
        assert!(code.jit_runtime().armed_leaf_slot(epoch_before).is_some());
        code.jit_runtime().set_heat_for_test(at.saturating_sub(1));
        assert_eq!(
            print_value(&builtin_mapcar_2(&mut ctx, function, input).unwrap()),
            "(11 22)"
        );
        let epoch_after = cache::leaf_slot_epoch();
        let control_was_stale = control_code
            .jit_runtime()
            .armed_leaf_slot(epoch_after)
            .is_none();
        assert_eq!(control_was_stale, epoch_after != epoch_before);
        if control_was_stale {
            // Retiring the mapped leaf invalidates every armed slot through
            // the global epoch. Reacquire the control below the crossing;
            // otherwise its first trial call would skip the stack probe.
            let rearm_heat = crate::emacs_core::jit::hot_threshold();
            assert!(rearm_heat.saturating_add(1) < at);
            control_code.jit_runtime().set_heat_for_test(rearm_heat);
            assert_eq!(
                ctx.apply1_bytecode_unobserved(control, Value::fixnum(11))
                    .unwrap(),
                Value::fixnum(11)
            );
        }
        assert_eq!(
            initial_allocator,
            cache::compiled_regalloc_for_test(control_id)
        );
        assert!(
            control_code
                .jit_runtime()
                .armed_leaf_slot(cache::leaf_slot_epoch())
                .is_some(),
            "the control must be armed immediately before its own crossing"
        );
        control_code
            .jit_runtime()
            .set_heat_for_test(at.saturating_sub(1));
        for item in [11, 22] {
            assert_eq!(
                ctx.apply1_bytecode_unobserved(control, Value::fixnum(item))
                    .unwrap(),
                Value::fixnum(item)
            );
        }
        assert_eq!(code.jit_runtime().heat(), control_code.jit_runtime().heat());
        tracing::info!(
            epoch_before,
            epoch_after,
            control_was_stale,
            ?initial_allocator,
            mapped_heat = code.jit_runtime().heat(),
            control_heat = control_code.jit_runtime().heat(),
            "retier admission control matched after current-epoch reacquisition"
        );
    }
    cache::clear();
    assert_eq!(
        print_value(&builtin_mapcar_2(&mut ctx, function, input).unwrap()),
        "(11 22)"
    );
    ctx.restore_vm_roots(roots);
}

#[test]
fn mapcar_activation_optional_and_rest_callbacks_keep_parameter_marshaling() {
    let mut ctx = context();
    let roots = ctx.save_vm_roots();
    for (required, optional, rest, expected) in [
        (true, true, true, "((11 nil nil) (22 nil nil))"),
        (false, false, true, "(((11)) ((22)))"),
    ] {
        let params = LambdaParams {
            required: if required {
                vec![intern("mapact-required")]
            } else {
                vec![]
            },
            optional: if optional {
                vec![intern("mapact-optional")]
            } else {
                vec![]
            },
            rest: rest.then(|| intern("mapact-rest")),
        };
        let slots =
            params.required.len() + params.optional.len() + usize::from(params.rest.is_some());
        let mut code = ByteCodeFunction::new(params);
        code.lexical = true;
        code.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
        code.ops = (0..slots)
            .map(|_| Op::StackRef((slots - 1) as u16))
            .collect();
        code.ops.extend([Op::List(slots as u16), Op::Return]);
        code.jit_runtime().set_hot_for_test();
        let function = Value::make_bytecode(code);
        let input = Value::list(vec![Value::fixnum(11), Value::fixnum(22)]);
        ctx.push_vm_frame_root(function);
        ctx.push_vm_frame_root(input);
        for _ in 0..3 {
            assert_eq!(
                print_value(&builtin_mapcar_2(&mut ctx, function, input).unwrap()),
                expected
            );
        }
    }
    ctx.restore_vm_roots(roots);
}

fn capture_inside_callback(ctx: &mut Context, item: Value) -> EvalResult {
    let (answer, reads) = capture(|| {
        let function = bytecode(None);
        let input = Value::list(vec![item]);
        builtin_mapcar_2(ctx, function, input)
    });
    assert!(reads.expect("inner observation scope").unchanged());
    assert!(!crate::tagged::collection_reads::is_active());
    Ok(answer?.cons_car())
}

#[test]
fn mapcar_activation_inner_observation_retires_before_the_next_callback() {
    let mut ctx = context();
    ctx.register_subr(SubrSpec::fixed1(
        "mapact-capture",
        capture_inside_callback,
        FixedMin1::One,
    ));
    let roots = ctx.save_vm_roots();
    let function = bytecode(Some(Value::symbol("mapact-capture")));
    let input = Value::list(vec![Value::fixnum(11), Value::fixnum(22)]);
    ctx.push_vm_frame_root(function);
    ctx.push_vm_frame_root(input);
    assert_eq!(
        print_value(&builtin_mapcar_2(&mut ctx, function, input).unwrap()),
        "(11 22)"
    );
    assert!(!crate::tagged::collection_reads::is_active());
    ctx.restore_vm_roots(roots);
}

#[test]
fn mapcar_activation_wrong_arity_is_deferred_until_a_nonempty_sequence() {
    let mut ctx = context();
    let mut code = ByteCodeFunction::new(LambdaParams::simple(vec![
        intern("mapact-first"),
        intern("mapact-second"),
    ]));
    code.lexical = true;
    code.ops = vec![Op::StackRef(1), Op::Return];
    code.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    let function = Value::make_bytecode(code);
    let roots = ctx.save_vm_roots();
    ctx.push_vm_frame_root(function);
    let before = (ctx.depth, ctx.specpdl.len());
    assert_eq!(
        builtin_mapcar_2(&mut ctx, function, Value::NIL).unwrap(),
        Value::NIL
    );
    assert!(matches!(
        builtin_mapcar_2(&mut ctx, function, Value::list(vec![Value::NIL])).kinded(),
        Err(FlowKind::Signal(_))
    ));
    assert_eq!((ctx.depth, ctx.specpdl.len()), before);
    ctx.restore_vm_roots(roots);
}
