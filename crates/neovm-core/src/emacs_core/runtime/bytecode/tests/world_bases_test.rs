//! The operand cursor retires its World designators at the same escape edge.
//! These fixtures build bytecode only after its owning Context installs TLS.

use super::*;
use crate::buffer::BufferManager;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use crate::emacs_core::subr::SubrSpec;

fn function(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    function.ops = ops;
    function.constants = constants.into();
    function.max_stack = super::super::function_slots::StackDepth::try_from(16usize)
        .expect("valid fixture stack depth");
    function
}

fn execute(context: &mut Context, function: &ByteCodeFunction) -> Value {
    let specpdl_base = context.specpdl.len();
    let condition_base = context.condition_stack_len();
    let stack_base = context.bc_buf.len();
    let result = {
        let mut vm = Vm::from_context(context);
        #[cfg(feature = "jit")]
        vm.force_interpreter_only_for_test();
        vm.execute(function, vec![])
            .expect("the bytecode must resume through the current owner")
    };
    assert_eq!(context.specpdl.len(), specpdl_base);
    assert_eq!(context.condition_stack_len(), condition_base);
    assert_eq!(context.bc_buf.len(), stack_base);
    result
}

fn replace_buffers_and_collect(context: &mut Context) -> EvalResult {
    let mut replacement = BufferManager::new();
    let selected = replacement.create_buffer(" *world-bases-returning-owner*");
    replacement.set_current(selected);
    context.buffers = replacement;
    context
        .obarray
        .set_symbol_value_id(intern("world-bases-callback-value"), Value::fixnum(73));
    context.gc_collect_exact();
    Ok(Value::NIL)
}

fn signal_after_replacing_buffers(context: &mut Context) -> EvalResult {
    replace_buffers_and_collect(context)?;
    Err(signal(
        LispCondition::Error,
        vec![Value::string("world-bases callback signal")],
    ))
}

fn throw_after_replacing_buffers(context: &mut Context) -> EvalResult {
    replace_buffers_and_collect(context)?;
    Err(Flow::throw(
        Value::symbol("world-bases-outer-tag"),
        Value::fixnum(99),
    ))
}

#[test]
fn world_bases_cursor_reacquires_after_publication_gc_and_manager_replacement() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let global = intern("world-bases-published-global");
    context
        .obarray
        .set_symbol_value_id(global, Value::fixnum(11));
    context
        .bc_buf
        .push(Value::list(vec![Value::string("operand-only")]));
    let mut vm = Vm::from_context(&mut context);
    let phase = vm.sole_owner.driver();
    let cursor = RunLoopCursor::acquire(&mut vm, phase);
    assert_eq!(
        vm.ctx.obarray.symbol_value_id_copied(global),
        Some(Value::fixnum(11))
    );
    let published = cursor.publish(&mut vm);
    for index in 0..256 {
        vm.ctx.obarray.set_symbol_value_id(
            intern(&format!("world-bases-growth-{index}")),
            Value::fixnum(index),
        );
    }
    vm.ctx
        .obarray
        .set_symbol_value_id(global, Value::fixnum(22));
    let gc_before = vm.ctx.gc_count;
    replace_buffers_and_collect(vm.ctx).expect("published GC and replacement");
    assert!(vm.ctx.gc_count > gc_before);
    let cursor = published.rejoin(&mut vm);
    assert_eq!(
        vm.ctx.obarray.symbol_value_id_copied(global),
        Some(Value::fixnum(22))
    );
    assert_eq!(print_value(&cursor.operands[0]), "(\"operand-only\")");
    assert_eq!(
        vm.ctx
            .buffers
            .current_buffer()
            .and_then(|buffer| buffer.name.as_str_owned())
            .as_deref(),
        Some(" *world-bases-returning-owner*")
    );
    let _finished = cursor.publish(&mut vm);
    vm.ctx.bc_buf.clear();
}

#[test]
fn world_bases_varref_keeps_plain_forwarded_alias_and_localized_reads() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    context
        .eval_str(
            "(progn
               (setq world-bases-global 11 gc-cons-threshold 12345678)
               (defvaralias 'world-bases-alias 'world-bases-global)
               (setq world-bases-local 33)
               (make-variable-buffer-local 'world-bases-local)
               (setq world-bases-local 44))",
        )
        .expect("prepare plain, forwarded, alias and BLV channels");
    assert!(
        context
            .obarray
            .get_by_id(intern("gc-cons-threshold"))
            .and_then(|symbol| symbol.forwarded_descriptor())
            .is_some(),
        "the fixture must exercise a real forwarded variable"
    );
    let code = function(
        vec![
            Op::VarRef(0),
            Op::VarRef(1),
            Op::VarRef(2),
            Op::VarRef(3),
            Op::List(4),
            Op::Return,
        ],
        vec![
            Value::symbol("world-bases-global"),
            Value::symbol("gc-cons-threshold"),
            Value::symbol("world-bases-alias"),
            Value::symbol("world-bases-local"),
        ],
    );
    assert_eq!(
        print_value(&execute(&mut context, &code)),
        "(11 12345678 11 44)"
    );
}

#[test]
fn world_bases_varset_keeps_plain_alias_forwarded_local_and_watcher_channels() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    context
        .eval_str(
            "(progn
               (setq world-bases-store-plain nil
                     world-bases-store-local nil
                     world-bases-store-watched nil
                     world-bases-store-events nil)
               (defvaralias 'world-bases-store-alias 'world-bases-store-plain)
               (make-local-variable 'world-bases-store-local)
               (fset 'world-bases-store-watch
                 (lambda (_symbol value operation _where)
                   (setq world-bases-store-events
                     (cons (list value operation) world-bases-store-events))
                   (garbage-collect)))
               (add-variable-watcher 'world-bases-store-watched
                                     'world-bases-store-watch))",
        )
        .expect("prepare real VarSet channels");
    let names = [
        "world-bases-store-plain",
        "world-bases-store-alias",
        "gc-cons-threshold",
        "world-bases-store-local",
        "world-bases-store-watched",
    ];
    let mut constants: Vec<Value> = names.iter().map(|name| Value::symbol(name)).collect();
    constants.push(Value::fixnum(12345678));
    let mut ops = Vec::new();
    for index in 0..names.len() {
        ops.push(Op::Constant(5));
        ops.push(Op::VarSet(index as u16));
    }
    for index in 0..names.len() {
        ops.push(Op::VarRef(index as u16));
    }
    ops.extend([Op::List(5), Op::Return]);
    let code = function(ops, constants);
    let gc_before = context.gc_count;
    assert_eq!(
        print_value(&execute(&mut context, &code)),
        "(12345678 12345678 12345678 12345678 12345678)"
    );
    assert!(
        context.gc_count > gc_before,
        "the watched store must publish"
    );
    assert_eq!(
        print_value(
            &context
                .eval_str("world-bases-store-events")
                .expect("watcher events")
        ),
        "((12345678 set))"
    );
}

#[test]
fn world_bases_varset_plain_cell_is_a_canonical_root_after_collection() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let target = intern("world-bases-store-canonical-root");
    context.obarray.set_symbol_value_id(target, Value::NIL);
    let code = function(
        vec![
            Op::Constant(0),
            Op::List(1),
            Op::VarSet(1),
            Op::Constant(2),
            Op::Call(0),
            Op::Pop,
            Op::VarRef(1),
            Op::Return,
        ],
        vec![
            Value::fixnum(73),
            Value::from_sym_id(target),
            Value::symbol("garbage-collect"),
        ],
    );
    // The newly allocated cons is absent from constants and the stack when
    // GC runs; only the canonical symbol-cell store retains it.
    let gc_before = context.gc_count;
    assert_eq!(print_value(&execute(&mut context, &code)), "(73)");
    assert!(context.gc_count > gc_before);
}

#[test]
fn world_bases_bytecode_callback_publishes_new_value_and_function_epoch() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new_vm_runtime_harness();
    let target = intern("world-bases-redefined-target");
    let callback = intern("world-bases-redefining-callback");
    let variable = intern("world-bases-redefined-variable");
    let first = Value::make_bytecode(function(
        vec![Op::Constant(0), Op::Return],
        vec![Value::fixnum(1)],
    ));
    let replacement = Value::make_bytecode(function(
        vec![Op::Constant(0), Op::Return],
        vec![Value::fixnum(2)],
    ));
    context.obarray.set_symbol_function_id(target, first);
    context
        .obarray
        .set_symbol_value_id(variable, Value::fixnum(11));
    let callback_code = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Fset,
            Op::Pop,
            Op::Constant(2),
            Op::VarSet(3),
            Op::Constant(4),
            Op::Call(0),
            Op::Pop,
            Op::Nil,
            Op::Return,
        ],
        vec![
            Value::from_sym_id(target),
            replacement,
            Value::fixnum(22),
            Value::from_sym_id(variable),
            Value::symbol("garbage-collect"),
        ],
    );
    context
        .obarray
        .set_symbol_function_id(callback, Value::make_bytecode(callback_code));
    let caller = function(
        vec![
            Op::Constant(0),
            Op::Call(0),
            Op::Pop,
            Op::Constant(1),
            Op::Call(0),
            Op::Pop,
            Op::Constant(0),
            Op::Call(0),
            Op::VarRef(2),
            Op::List(2),
            Op::Return,
        ],
        vec![
            Value::from_sym_id(target),
            Value::from_sym_id(callback),
            Value::from_sym_id(variable),
        ],
    );
    let gc_before = context.gc_count;
    let epoch_before = context.dispatch_projection().function_epoch();
    assert_eq!(print_value(&execute(&mut context, &caller)), "(2 22)");
    assert!(context.gc_count > gc_before);
    assert!(context.dispatch_projection().function_epoch() > epoch_before);
}

#[test]
fn world_bases_native_callback_rejoins_with_current_buffer_storage_and_selection() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    context.register_subr(SubrSpec::fixed0(
        "world-bases-replace-buffers",
        replace_buffers_and_collect,
    ));
    let caller = function(
        vec![
            Op::Constant(0),
            Op::Call(0),
            Op::Pop,
            Op::VarRef(1),
            Op::CallBuiltinSym(intern("current-buffer"), 0),
            Op::List(2),
            Op::Return,
        ],
        vec![
            Value::symbol("world-bases-replace-buffers"),
            Value::symbol("world-bases-callback-value"),
        ],
    );
    let result = execute(&mut context, &caller);
    assert_eq!(result.cons_car(), Value::fixnum(73));
    assert_eq!(
        result.cons_cdr().cons_car().as_buffer_id(),
        context.buffers.current_buffer_id()
    );
    assert_eq!(
        context
            .buffers
            .current_buffer()
            .and_then(|buffer| buffer.name.as_str_owned())
            .as_deref(),
        Some(" *world-bases-returning-owner*")
    );
}

#[test]
fn world_bases_signal_rejoin_observes_hook_mutation_after_callback_gc() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    context.register_subr(SubrSpec::fixed0(
        "world-bases-signal-callback",
        signal_after_replacing_buffers,
    ));
    context
        .eval_str(
            "(setq signal-hook-function
               (lambda (_signal _data)
                 (setq world-bases-callback-value 55)
                 (garbage-collect)))",
        )
        .expect("install the signal hook");
    let caller = function(
        vec![
            Op::Constant(0),
            Op::PushConditionCaseRaw(6),
            Op::Constant(1),
            Op::Call(0),
            Op::Nil,
            Op::Return,
            Op::Pop,
            Op::VarRef(2),
            Op::CallBuiltinSym(intern("current-buffer"), 0),
            Op::List(2),
            Op::Return,
        ],
        vec![
            Value::symbol("error"),
            Value::symbol("world-bases-signal-callback"),
            Value::symbol("world-bases-callback-value"),
        ],
    );
    let gc_before = context.gc_count;
    let result = execute(&mut context, &caller);
    assert_eq!(result.cons_car(), Value::fixnum(55));
    assert_eq!(
        result.cons_cdr().cons_car().as_buffer_id(),
        context.buffers.current_buffer_id()
    );
    assert!(context.gc_count >= gc_before + 2);
}

#[test]
fn world_bases_throw_rejoin_delivers_value_and_current_owner_reads() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    context.register_subr(SubrSpec::fixed0(
        "world-bases-throw-callback",
        throw_after_replacing_buffers,
    ));
    let caller = function(
        vec![
            Op::Constant(0),
            Op::PushCatch(6),
            Op::Constant(1),
            Op::Call(0),
            Op::Nil,
            Op::Return,
            Op::VarRef(2),
            Op::CallBuiltinSym(intern("current-buffer"), 0),
            Op::List(3),
            Op::Return,
        ],
        vec![
            Value::symbol("world-bases-outer-tag"),
            Value::symbol("world-bases-throw-callback"),
            Value::symbol("world-bases-callback-value"),
        ],
    );
    let gc_before = context.gc_count;
    let result = execute(&mut context, &caller);
    assert_eq!(result.cons_car(), Value::fixnum(99));
    assert_eq!(result.cons_cdr().cons_car(), Value::fixnum(73));
    assert_eq!(
        result.cons_cdr().cons_cdr().cons_car().as_buffer_id(),
        context.buffers.current_buffer_id()
    );
    assert!(context.gc_count > gc_before);
}

#[test]
fn world_bases_varref_reads_nil_undo_and_all_object_forwarders_after_gc() {
    use crate::emacs_core::defvar_bool::ByteBooleanVars;
    use crate::emacs_core::forward::{LispFwdType, alloc_kboard_objfwd, alloc_objfwd};

    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    context.obarray.define_bool_variable(
        "world-bases-forwarded-bool",
        false,
        ByteBooleanVars::ErasedByLreadInit,
    );
    context.obarray.install_objfwd(
        intern("world-bases-forwarded-object"),
        alloc_objfwd(Value::string("object-only")),
    );
    context.obarray.install_kboard_objfwd(
        intern("world-bases-forwarded-keyboard"),
        alloc_kboard_objfwd(Value::string("keyboard-only")),
    );
    context
        .eval_str(
            "(progn
               (setq world-bases-direct-nil nil world-bases-forwarded-bool 7)
               (make-local-variable 'fill-column)
               (setq fill-column 91))",
        )
        .expect("prepare nil, normalized Boolean and per-buffer channels");
    context
        .buffers
        .current_buffer_mut()
        .expect("a selected buffer")
        .set_undo_list(Value::list(vec![Value::string("undo-only")]));
    assert_eq!(
        context
            .obarray
            .get_by_id(intern("buffer-undo-list"))
            .and_then(|symbol| symbol.plain_value()),
        Some(Value::NIL),
        "the dedicated undo read must override a plain nil cell"
    );
    for (name, family) in [
        ("world-bases-forwarded-bool", LispFwdType::Bool),
        ("world-bases-forwarded-object", LispFwdType::Obj),
        ("world-bases-forwarded-keyboard", LispFwdType::KboardObj),
        ("fill-column", LispFwdType::BufferObj),
    ] {
        assert_eq!(
            context
                .obarray
                .get_by_id(intern(name))
                .and_then(|symbol| symbol.forwarded_descriptor())
                .map(|forwarder| forwarder.ty()),
            Some(family),
            "the fixture must exercise the requested family for {name}"
        );
    }

    let constants = vec![
        Value::symbol("garbage-collect"),
        Value::symbol("world-bases-direct-nil"),
        Value::symbol("buffer-undo-list"),
        Value::symbol("world-bases-forwarded-bool"),
        Value::symbol("world-bases-forwarded-object"),
        Value::symbol("world-bases-forwarded-keyboard"),
        Value::symbol("fill-column"),
    ];
    // Exercise the actual opcode admission, not just fast_path_var_ref. Neither
    // nil nor the dedicated undo channel may enter the generic alist probe.
    let nil_and_undo = function(
        vec![Op::VarRef(1), Op::VarRef(2), Op::List(2), Op::Return],
        constants.clone(),
    );
    crate::buffer::buffer::reset_buffer_local_value_lookup_probes();
    assert_eq!(
        print_value(&execute(&mut context, &nil_and_undo)),
        "(nil (\"undo-only\"))"
    );
    assert_eq!(crate::buffer::buffer::buffer_local_value_lookup_probes(), 0);

    // The two object strings have no root in this function's constants. Their
    // descriptor roots and the selected buffer's undo root must survive GC.
    let code = function(
        vec![
            Op::Constant(0),
            Op::Call(0),
            Op::Pop,
            Op::VarRef(1),
            Op::VarRef(2),
            Op::VarRef(3),
            Op::VarRef(4),
            Op::VarRef(5),
            Op::VarRef(6),
            Op::List(6),
            Op::Return,
        ],
        constants,
    );
    let gc_before = context.gc_count;
    assert_eq!(
        print_value(&execute(&mut context, &code)),
        "(nil (\"undo-only\") t \"object-only\" \"keyboard-only\" 91)"
    );
    assert!(context.gc_count > gc_before);
}

#[derive(Clone, Copy, Debug)]
enum VoidVarRefChannel {
    MissingKeyword,
    MissingPlain,
    Plain,
    Alias,
    Localized,
}

#[test]
fn world_bases_varref_preserves_missing_keyword_and_void_channel_errors() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let plain = intern("world-bases-void-plain");
    context.obarray.ensure_symbol_id(plain);
    let alias = intern("world-bases-void-alias");
    context
        .obarray
        .make_variable_alias(alias, plain)
        .expect("alias to a real unbound plain cell");
    let localized = intern("world-bases-void-localized");
    context
        .obarray
        .make_symbol_localized(localized, Value::UNBOUND);

    // Registry identity alone must not materialize these Obarray slots. A
    // keyword specifically distinguishes the new lookup-only fallback: its
    // canonical fallback returns the keyword itself instead of signaling.
    let missing_keyword = intern(":world-bases-missing-keyword");
    let missing_plain = intern("world-bases-missing-plain");
    assert!(crate::emacs_core::intern::is_keyword_id(missing_keyword));
    for channel in [
        VoidVarRefChannel::MissingKeyword,
        VoidVarRefChannel::MissingPlain,
        VoidVarRefChannel::Plain,
        VoidVarRefChannel::Alias,
        VoidVarRefChannel::Localized,
    ] {
        let name = match channel {
            VoidVarRefChannel::MissingKeyword => {
                assert!(context.obarray.get_by_id(missing_keyword).is_none());
                missing_keyword
            }
            VoidVarRefChannel::MissingPlain => {
                assert!(context.obarray.get_by_id(missing_plain).is_none());
                missing_plain
            }
            VoidVarRefChannel::Plain => plain,
            VoidVarRefChannel::Alias => alias,
            VoidVarRefChannel::Localized => localized,
        };
        let code = function(
            vec![Op::VarRef(0), Op::Return],
            vec![Value::from_sym_id(name)],
        );
        let specpdl_base = context.specpdl.len();
        let condition_base = context.condition_stack_len();
        let stack_base = context.bc_buf.len();
        let result = {
            let mut vm = Vm::from_context(&mut context);
            #[cfg(feature = "jit")]
            vm.force_interpreter_only_for_test();
            vm.execute(&code, vec![])
        };
        match result.kinded() {
            Err(FlowKind::Signal(signal)) => {
                assert_eq!(resolve_sym(signal.symbol), "void-variable", "{channel:?}");
                assert_eq!(signal.data, vec![Value::from_sym_id(name)], "{channel:?}");
            }
            Err(FlowKind::Throw(_) | FlowKind::ThreadBlocked(_) | FlowKind::Shutdown(_)) => {
                panic!("{channel:?} must signal void-variable")
            }
            Ok(value) => panic!(
                "{channel:?} returned {} instead of signaling",
                print_value(&value)
            ),
        }
        assert_eq!(context.specpdl.len(), specpdl_base, "{channel:?}");
        assert_eq!(context.condition_stack_len(), condition_base, "{channel:?}");
        assert_eq!(context.bc_buf.len(), stack_base, "{channel:?}");
    }
}
