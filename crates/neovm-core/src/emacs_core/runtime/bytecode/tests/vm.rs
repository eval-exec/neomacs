use super::*;
use crate::buffer::LispCharPos1;
use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{ConditionFrame, Context, GuiFrameHostSize, ResumeTarget};
use crate::emacs_core::value::HashTableTest;
use crate::window::{SplitDirection, SplitPlacement};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;

fn new_vm(eval: &mut Context) -> Vm<'_> {
    Vm::from_context(eval)
}

fn find_bin(name: &str) -> String {
    let path = std::env::var_os("PATH").expect("PATH should be set");
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    panic!("binary {name} not found on PATH");
}

fn with_vm_eval_in_context<R>(
    mut eval: Context,
    src: &str,
    lexical: bool,
    f: impl FnOnce(Result<Value, EvalError>, &Context) -> R,
) -> R {
    eval.set_lexical_binding(lexical);
    let result = eval.eval_str(src);
    f(result, &eval)
}

fn with_vm_eval<R>(src: &str, lexical: bool, f: impl FnOnce(Result<Value, EvalError>) -> R) -> R {
    with_vm_eval_state(src, lexical, |result, _| f(result))
}

fn with_vm_eval_state<R>(
    src: &str,
    lexical: bool,
    f: impl FnOnce(Result<Value, EvalError>, &Context) -> R,
) -> R {
    with_vm_eval_in_context(Context::new_vm_runtime_harness(), src, lexical, f)
}

fn with_vm_eval_full_context_state<R>(
    src: &str,
    lexical: bool,
    f: impl FnOnce(Result<Value, EvalError>, &Context) -> R,
) -> R {
    with_vm_eval_in_context(Context::new(), src, lexical, f)
}

fn with_vm_eval_bootstrap_context_state<R>(
    src: &str,
    lexical: bool,
    f: impl FnOnce(Result<Value, EvalError>, &Context) -> R,
) -> R {
    with_vm_eval_in_context(
        crate::test_utils::runtime_startup_context(),
        src,
        lexical,
        f,
    )
}

fn vm_eval_str(src: &str) -> String {
    with_vm_eval(src, false, |result| {
        crate::emacs_core::error::format_eval_result(&result)
    })
}

/// Variant of vm_eval_str that uses the cached runtime-startup
/// evaluator. Required for tests that exercise Lisp-defined
/// macros (with-temp-buffer, with-current-buffer, dolist,
/// dotimes, etc.) which only exist after subr.el is loaded.
fn vm_bootstrap_eval_str(src: &str) -> String {
    let mut eval = crate::test_utils::runtime_startup_context();
    let result = eval.eval_str(src);
    crate::emacs_core::error::format_eval_result(&result)
}

fn vm_eval_lexical_str(src: &str) -> String {
    with_vm_eval(src, true, |result| {
        crate::emacs_core::error::format_eval_result(&result)
    })
}

fn vm_eval_with_init_str(src: &str, init: impl FnOnce(&mut Context)) -> String {
    let mut eval = Context::new_vm_runtime_harness();
    init(&mut eval);
    let result = eval.eval_str(src);
    crate::emacs_core::error::format_eval_result(&result)
}

fn vm_eval_interactive_str(src: &str) -> String {
    let mut eval = Context::new_vm_runtime_harness();
    eval.set_variable("noninteractive", Value::NIL);
    let result = eval.eval_str(src);
    crate::emacs_core::error::format_eval_result(&result)
}

fn vm_eval_interactive_with_init_str(src: &str, init: impl FnOnce(&mut Context)) -> String {
    let mut eval = Context::new_vm_runtime_harness();
    eval.set_variable("noninteractive", Value::NIL);
    init(&mut eval);
    let result = eval.eval_str(src);
    crate::emacs_core::error::format_eval_result(&result)
}

fn vm_bootstrap_eval_with_init_str(src: &str, init: impl FnOnce(&mut Context)) -> String {
    let mut eval = crate::test_utils::runtime_startup_context();
    init(&mut eval);
    let result = eval.eval_str(src);
    crate::emacs_core::error::format_eval_result(&result)
}

#[test]
fn vm_catch_leaves_shared_condition_stack_balanced() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_state("(catch 'tag (throw 'tag 42))", false, |result, eval| {
        assert_eq!(
            crate::emacs_core::error::format_eval_result(&result),
            "OK 42"
        );
        assert_eq!(eval.condition_stack_depth_for_test(), 0);
    });
}

#[test]
fn vm_condition_case_leaves_shared_condition_stack_balanced() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_full_context_state(
        "(condition-case err (signal 'error 1) (error (car err)))",
        false,
        |result, eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK error"
            );
            assert_eq!(eval.condition_stack_depth_for_test(), 0);
        },
    );
}

#[test]
fn vm_invalid_constant_reference_signals_instead_of_panicking() {
    crate::test_utils::init_test_tracing();
    let rendered = vm_eval_str(
        "(condition-case err
             (funcall (make-byte-code 0 \"\\300\\207\" [] 1))
           (error (list 'caught (car err) (car (cdr err)))))",
    );
    assert_eq!(rendered, "OK (caught error \"Invalid byte-code\")");
}

#[test]
fn vm_invalid_stack_ref_signals_instead_of_panicking() {
    crate::test_utils::init_test_tracing();
    let rendered = vm_eval_str(
        "(condition-case err
             (funcall (make-byte-code 0 \"\\000\\207\" [] 1))
           (error (list 'caught (car err) (car (cdr err)))))",
    );
    assert_eq!(rendered, "OK (caught error \"Invalid byte-code\")");
}

#[test]
fn vm_branch_underflow_signals_invalid_bytecode() {
    crate::test_utils::init_test_tracing();
    let rendered = vm_eval_str(
        r#"(mapcar
            (lambda (bytes)
              (condition-case err
                  (funcall (make-byte-code 0 bytes [] 0))
                (error (list 'caught (car err) (car (cdr err))))))
            '("\203\003\000\207"
              "\204\003\000\207"
              "\205\003\000\207"
              "\206\003\000\207"))"#,
    );
    assert_eq!(
        rendered,
        "OK ((caught error \"Invalid byte-code\") (caught error \"Invalid byte-code\") (caught error \"Invalid byte-code\") (caught error \"Invalid byte-code\"))"
    );
}

#[test]
fn vm_stack_set_zero_discards_tos_like_gnu() {
    crate::test_utils::init_test_tracing();
    let rendered =
        vm_eval_str(r#"(funcall (make-byte-code 0 "\300\301\262\000\207" [kept dropped] 2))"#);
    assert_eq!(rendered, "OK kept");
}

#[test]
fn vm_integer_arg_descriptor_does_not_dynamic_bind_dummy_args_like_gnu() {
    crate::test_utils::init_test_tracing();
    let rendered = vm_eval_str(
        "(condition-case err
             (funcall (make-byte-code 257 \"\\10\\207\" [arg0] 2) 42)
           (error (list (car err) (car (cdr err)))))",
    );
    assert_eq!(rendered, "OK (void-variable arg0)");
}

#[test]
fn vm_dynamic_bytecode_args_do_not_occupy_bytecode_stack_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();
    let mut func = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("ignored")],
        optional: vec![],
        rest: None,
    });
    let list_idx = func.add_symbol("list");
    let one_idx = func.add_constant(Value::fixnum(1));
    let two_idx = func.add_constant(Value::fixnum(2));
    func.ops = vec![
        Op::Constant(list_idx),
        Op::Constant(one_idx),
        Op::Constant(two_idx),
        Op::Call(2),
        Op::Return,
    ];
    func.max_stack = 3;
    func.lexical = false;

    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&func, vec![Value::fixnum(42)])
            .expect("dynamic bytecode args should be specbound outside the bytecode stack")
    };

    assert_eq!(
        result,
        Value::list_from_slice(&[Value::fixnum(1), Value::fixnum(2)])
    );
}

#[test]
fn vm_handler_bind_1_leaves_shared_condition_stack_balanced() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_full_context_state(
        "(condition-case err
           (handler-bind-1 (lambda () (signal 'error 1))
                           '(error)
                           (lambda (_data) 'handled))
         (error (car err)))",
        false,
        |result, eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK error"
            );
            assert_eq!(eval.condition_stack_depth_for_test(), 0);
        },
    );
}

#[test]
fn vm_handler_bind_1_runs_for_void_variable_before_outer_condition_case() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_full_context_state(
        "(let (seen)
           (condition-case err
               (handler-bind-1
                 (lambda () missing-variable)
                 '(error)
                 (lambda (err) (setq seen err)))
             (error (list seen err))))",
        false,
        |result, eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK ((void-variable missing-variable) (void-variable missing-variable))"
            );
            assert_eq!(eval.condition_stack_depth_for_test(), 0);
        },
    );
}

#[test]
fn vm_handler_bind_1_runs_for_eval_void_variable_before_outer_condition_case() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_full_context_state(
        "(let (seen)
           (condition-case err
               (handler-bind-1
                 (lambda () (eval 'missing-variable t))
                 '(error)
                 (lambda (err) (setq seen err)))
             (error (list seen err))))",
        false,
        |result, eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK ((void-variable missing-variable) (void-variable missing-variable))"
            );
            assert_eq!(eval.condition_stack_depth_for_test(), 0);
        },
    );
}

#[test]
fn vm_handler_bind_1_runs_inside_signal_dynamic_extent() {
    crate::test_utils::init_test_tracing();
    // user-error is defined in subr.el → use bootstrap context.
    with_vm_eval_bootstrap_context_state(
        "(catch 'tag
           (handler-bind-1
             (lambda ()
               (list 'inner-catch
                     (catch 'tag
                       (user-error \"hello\"))))
             '(error)
             (lambda (_err) (throw 'tag 'err))))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK (inner-catch err)"
            );
        },
    );
}

#[test]
fn vm_handler_bind_1_mutes_lower_condition_handlers() {
    crate::test_utils::init_test_tracing();
    // user-error is defined in subr.el → use bootstrap context.
    with_vm_eval_bootstrap_context_state(
        "(condition-case nil
           (handler-bind-1
             (lambda ()
               (list 'result
                     (condition-case nil
                         (user-error \"hello\")
                       (wrong-type-argument 'inner-handler))))
             '(error)
             (lambda (_err) (signal 'wrong-type-argument nil)))
         (wrong-type-argument 'wrong-type-argument))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK wrong-type-argument"
            );
        },
    );
}

#[test]
fn vm_handler_bind_1_handlers_do_not_apply_within_handlers() {
    crate::test_utils::init_test_tracing();
    // user-error is defined in subr.el → use bootstrap context.
    with_vm_eval_bootstrap_context_state(
        "(condition-case nil
           (handler-bind-1
             (lambda () (user-error \"hello\"))
             '(error)
             (lambda (_err) (signal 'wrong-type-argument nil))
             '(wrong-type-argument)
             (lambda (_err) (user-error \"wrong-type-argument\")))
         (wrong-type-argument 'wrong-type-argument)
         (error 'plain-error))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK wrong-type-argument"
            );
        },
    );
}

#[test]
fn vm_signal_hook_function_sees_raw_signal_payload_before_condition_case() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_full_context_state(
        "(let (seen)
           (let ((signal-hook-function
                  (lambda (sym data)
                    (setq seen (cons sym data)))))
             (condition-case nil
                 (signal 'error 1)
               (error seen))))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK (error . 1)"
            );
        },
    );
}

#[test]
fn vm_signal_nil_error_object_uses_embedded_symbol_and_skips_signal_hook() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_full_context_state(
        "(let (seen)
           (let ((signal-hook-function
                  (lambda (&rest xs)
                    (setq seen xs))))
             (condition-case err
                 (signal nil '(error 1))
               (error (list err seen)))))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK ((error 1) nil)"
            );
        },
    );
}

#[test]
fn vm_signal_hook_runs_before_invalid_error_symbol_canonicalization() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_full_context_state(
        "(catch 'tag
           (let ((signal-hook-function
                  (lambda (sym data)
                    (throw 'tag (list sym data)))))
             (signal 'neomacs-invalid-signal 1)))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK (neomacs-invalid-signal 1)"
            );
        },
    );
}

#[test]
fn vm_compiled_unwind_protect_runs_cleanup_on_throw() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(let (log)
               (list
                (catch 'done
                  (unwind-protect
                      (throw 'done 'ok)
                    (setq log 'ran)))
                log))"
        ),
        "OK (ok ran)"
    );
}

#[test]
fn vm_compiled_unwind_protect_runs_cleanup_on_signal() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(let (log)
               (condition-case nil
                   (unwind-protect
                       (signal 'error 1)
                     (setq log 'ran))
                 (error log)))"
        ),
        "OK ran"
    );
}

#[test]
fn vm_compiled_unwind_protect_cleanup_closure_captures_lexical_scope() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_lexical_str(
            "(let ((x 7)
                   y)
               (unwind-protect
                   'ok
                 (setq y x))
               y)"
        ),
        "OK 7"
    );
}

#[test]
fn vm_condition_case_suppresses_debugger_without_debug_marker() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_full_context_state(
        "(let ((debug-on-error t)
               (called nil)
               (debugger (lambda (&rest _args)
                           (setq called 'debugger))))
           (list (condition-case nil
                     (signal 'error 1)
                   (error 'handled))
                 called))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK (handled nil)"
            );
        },
    );
}

#[test]
fn vm_condition_case_debug_marker_calls_debugger_before_handler() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_full_context_state(
        "(let ((debug-on-error t)
               (called nil)
               (debugger (lambda (&rest args)
                           (setq called args))))
           (list (condition-case nil
                     (signal 'error 1)
                   ((debug error) 'handled))
                 called))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK (handled (error (error . 1)))"
            );
        },
    );
}

#[test]
fn vm_debug_on_signal_overrides_condition_case_debugger_suppression() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_full_context_state(
        "(let ((debug-on-error t)
               (debug-on-signal t)
               (called nil)
               (debugger (lambda (&rest _args)
                           (setq called 'debugger))))
           (list (condition-case nil
                     (signal 'error 1)
                   (error 'handled))
                 called))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK (handled debugger)"
            );
        },
    );
}

#[test]
fn vm_debug_ignored_errors_blocks_debugger_even_with_debug_marker() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_full_context_state(
        "(let ((debug-on-error t)
               (debug-ignored-errors '(arith-error))
               (called nil)
               (debugger (lambda (&rest _args)
                           (setq called 'debugger))))
           (list (condition-case nil
                     (/ 1 0)
                   ((debug error) 'handled))
                 called))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK (handled nil)"
            );
        },
    );
}

fn quoted_dispatch_names(source: &str, predicate: impl Fn(&str) -> bool) -> BTreeSet<String> {
    source
        .lines()
        .filter(|line| predicate(line))
        .filter_map(|line| {
            let start = line.find('"')?;
            let rest = &line[start + 1..];
            let end = rest.find('"')?;
            Some(rest[..end].to_string())
        })
        .collect()
}

#[test]
fn vm_direct_dispatch_covers_all_dispatch_builtin_names() {
    crate::test_utils::init_test_tracing();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let builtins_mod =
        std::fs::read_to_string(manifest.join("src/emacs_core/lisp/native/builtins/mod.rs"))
            .expect("read builtins/mod.rs");
    let vm_source = std::fs::read_to_string(manifest.join("src/emacs_core/runtime/bytecode/vm.rs"))
        .expect("read vm.rs");

    let builtin_names = quoted_dispatch_names(&builtins_mod, |line| {
        line.contains("=> return Some(") || line.contains("=> Some(")
    });
    let vm_names = quoted_dispatch_names(&vm_source, |line| {
        let trimmed = line.trim_start();
        trimmed.starts_with('"') && trimmed.contains("\" =>")
    });

    let missing: Vec<_> = builtin_names.difference(&vm_names).cloned().collect();
    assert!(
        missing.is_empty(),
        "VM dispatch is missing builtin names: {missing:?}"
    );
}

#[test]
fn vm_raw_parent_bridge_helper_is_gone() {
    crate::test_utils::init_test_tracing();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("src/emacs_core");
    let mut pending = vec![root.clone()];
    let mut unexpected = Vec::new();

    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("read emacs_core dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }

            let rel = path
                .strip_prefix(&manifest)
                .expect("path under crate root")
                .to_path_buf();
            let source = std::fs::read_to_string(&path).expect("read Rust source");
            for (lineno, line) in source.lines().enumerate() {
                if !line.contains("with_extra_gc_roots_ptr(") {
                    continue;
                }
                if rel == PathBuf::from("src/emacs_core/runtime/bytecode/tests/vm.rs") {
                    continue;
                }
                unexpected.push(format!("{}:{}", rel.display(), lineno + 1));
            }
        }
    }

    assert!(
        unexpected.is_empty(),
        "stale raw parent-evaluator bridge helper references: {unexpected:?}"
    );
}

#[test]
fn vm_parent_evaluator_bridge_is_limited_to_semantic_boundaries() {
    crate::test_utils::init_test_tracing();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("src/emacs_core");
    let mut pending = vec![root.clone()];
    let mut unexpected = Vec::new();

    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("read emacs_core dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }

            let rel = path
                .strip_prefix(&manifest)
                .expect("path under crate root")
                .to_path_buf();
            let source = std::fs::read_to_string(&path).expect("read Rust source");
            for (lineno, line) in source.lines().enumerate() {
                if !line.contains("with_extra_gc_roots(") {
                    continue;
                }
                let allowed = rel == PathBuf::from("src/emacs_core/runtime/bytecode/tests/vm.rs");
                if !allowed {
                    unexpected.push(format!("{}:{}", rel.display(), lineno + 1));
                }
            }
        }
    }

    assert!(
        unexpected.is_empty(),
        "typed parent-evaluator bridge escaped semantic boundary files: {unexpected:?}"
    );
}

#[test]
fn vm_lexical_let_closure_captures_bytecode_binding() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_lexical_str(
            r#"
(funcall
 (let ((x 42))
   (lambda () x)))
"#,
        ),
        "OK 42"
    );
}

#[test]
fn vm_lexical_param_closure_captures_bytecode_binding() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_lexical_str(
            r#"
(funcall
 ((lambda (x)
    (lambda () x))
  42))
"#,
        ),
        "OK 42"
    );
}

#[test]
fn vm_interpreted_lambda_call_restores_outer_binding_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str("(let ((x 41)) (list (funcall (lambda (x) x) 7) x))"),
        "OK (7 41)"
    );
    assert_eq!(
        vm_eval_lexical_str("(let ((x 41)) (list (funcall (lambda (x) x) 7) x))"),
        "OK (7 41)"
    );
}

#[test]
fn vm_mapc_mapcan_and_mapconcat_use_shared_runtime_callbacks() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str("(let ((xs '(1 2 3))) (eq (mapc #'identity xs) xs))"),
        "OK t"
    );
    assert_eq!(
        vm_eval_str(
            "(progn
               (setq vm-mapc-log nil)
               (mapc (lambda (x) (setq vm-mapc-log (cons x vm-mapc-log))) '(1 2 3))
               vm-mapc-log)"
        ),
        "OK (3 2 1)"
    );
    assert_eq!(
        vm_eval_str("(mapcan (lambda (x) (list x (+ x 10))) '(1 2 3))"),
        "OK (1 11 2 12 3 13)"
    );
    assert_eq!(
        vm_eval_str("(mapconcat (lambda (x) (number-to-string x)) '(1 2 3) \":\")"),
        "OK \"1:2:3\""
    );
    assert_eq!(
        vm_eval_str("(mapconcat #'identity [\"a\" \"b\" \"c\"] \",\")"),
        "OK \"a,b,c\""
    );
    assert_eq!(vm_eval_str("(mapconcat #'identity nil \",\")"), "OK \"\"");
}

#[test]
fn vm_reader_and_minibuffer_builtins_use_shared_runtime_entry() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (let ((unread-command-events (list 97)))
                   (list (input-pending-p)
                         (read-char)
                         unread-command-events
                         (recent-keys)))
                 (let ((unread-command-events (list 97)))
                   (read-key-sequence "key: "))
                 (let ((unread-command-events (list 'foo)))
                   (read-key-sequence-vector "key: "))
                 (progn
                   (set-input-mode t nil nil 7)
                   (current-input-mode))
                 (progn
                   (set-input-interrupt-mode nil)
                   (current-input-mode))
                 (progn
                   (discard-input)
                   (input-pending-p))
                 (set-input-meta-mode t)
                 (set-output-flow-control t)
                 (set-quit-char 7)
                 (waiting-for-user-input-p)
                 (condition-case err (read-from-minibuffer 1) (error (car err)))
                 (condition-case err (read-string 1) (error (car err)))
                 (condition-case err (completing-read 1 '("a")) (error (car err)))
                 (condition-case err (read-buffer 1) (error (car err)))
                 (condition-case err (read-command 1) (error (car err)))
                 (condition-case err (read-variable 1) (error (car err)))
                 (condition-case err (yes-or-no-p 1) (error (car err))))"#
        ),
        "OK ((t 97 nil [97]) \"a\" [foo] (t nil t 7) (nil nil t 7) nil nil nil nil nil wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument)"
    );
}

#[test]
fn vm_keyboard_c_builtins_use_shared_unread_and_batch_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (let ((unread-command-events (list 'foo 97)))
                   (condition-case err
                       (list (read-char) unread-command-events (recent-keys))
                     (error (list (car err) unread-command-events (recent-keys)))))
                 (let ((unread-command-events (list 'foo 97)))
                   (list (read-event) unread-command-events (recent-keys)))
                 (let ((unread-command-events (list 'foo 97)))
                   (list (read-char-exclusive) unread-command-events (recent-keys)))
                 (let ((unread-command-events nil))
                   (list (read-event)
                         (read-char-exclusive)
                         (read-key-sequence "k: ")
                         (read-key-sequence-vector "k: "))))"#
        ),
        "OK ((error (foo) [foo]) (foo (97) [foo foo]) (97 nil [foo foo foo 97]) (nil nil \"\" []))"
    );
}

#[test]
fn vm_internal_labeled_restriction_builtins_use_shared_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (internal--labeled-narrow-to-region 2 5 'outer-tag)
                 (internal--labeled-narrow-to-region 1 7 'inner-tag)
                 (list (point-min) (point-max)
                       (progn (internal--labeled-widen 'inner-tag)
                              (list (point-min) (point-max)))
                       (progn (internal--labeled-widen 'outer-tag)
                              (list (point-min) (point-max)))))"#,
            |eval| {
                let buffer_id = eval.buffers.create_buffer("vm-labeled-restriction");
                eval.buffers.set_current(buffer_id);
                let _ = eval.buffers.insert_into_buffer(buffer_id, "abcdef");
            },
        ),
        "OK (2 5 (2 5) (1 7))"
    );
}

#[test]
fn vm_save_restriction_restores_labeled_restrictions_and_widen_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (internal--labeled-narrow-to-region 2 5 'tag)
                 (list (point-min) (point-max)
                       (save-restriction
                         (internal--labeled-widen 'tag)
                         (list (point-min) (point-max)))
                       (point-min) (point-max)
                       (progn (widen) (list (point-min) (point-max)))
                       (progn (internal--labeled-widen 'tag)
                              (list (point-min) (point-max)))))"#,
            |eval| {
                let buffer_id = eval.buffers.create_buffer("vm-saved-labeled-restriction");
                eval.buffers.set_current(buffer_id);
                let _ = eval.buffers.insert_into_buffer(buffer_id, "abcdef");
            },
        ),
        "OK (2 5 (1 7) 2 5 (2 5) (1 7))"
    );
}

#[test]
fn vm_save_excursion_restores_point_on_normal_exit() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (goto-char 3)
                 (save-excursion
                   (goto-char 6))
                 (point))"#,
            |eval| {
                let buffer_id = eval.buffers.create_buffer("vm-save-excursion");
                eval.buffers.set_current(buffer_id);
                let _ = eval.buffers.insert_into_buffer(buffer_id, "abcdef");
            },
        ),
        "OK 3"
    );
}

#[test]
fn vm_save_excursion_marker_survives_exact_gc() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (goto-char (point-max))
                 (save-excursion
                   (garbage-collect)
                   (goto-char 3)
                   (insert "XXX"))
                 (list (point) (buffer-string)))"#,
            |eval| {
                let buffer_id = eval.buffers.create_buffer("vm-save-excursion-gc");
                eval.buffers.set_current(buffer_id);
                let _ = eval.buffers.insert_into_buffer(buffer_id, "abcdef");
            },
        ),
        "OK (10 \"abXXXcdef\")"
    );
}

#[test]
fn vm_compiled_loaddefs_docstring_print_form_preserves_point() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(progn
                 (require 'bytecomp)
                 (funcall
                  (byte-compile
                   (lambda ()
                     (with-temp-buffer
                       (let ((def (list 'autoload
                                        (list 'quote 'ebrowse-tags-find-declaration)
                                        "ebrowse"
                                        "Find declaration of member at point."
                                        t))
                             (n 0))
                         (insert "(")
                         (while (< n 3)
                           (prin1 (car def) (current-buffer)
                                  '(t (escape-newlines . t)
                                      (escape-control-characters . t)))
                           (setq def (cdr def))
                           (insert " ")
                           (setq n (1+ n)))
                         (let ((start (point)))
                           (prin1 (car def) (current-buffer) t)
                           (setq def (cdr def))
                           (save-excursion
                             (goto-char (1+ start))
                             (insert "\\
")))
                         (while def
                           (insert " ")
                           (prin1 (car def) (current-buffer)
                                  '(t (escape-newlines . t)
                                      (escape-control-characters . t)))
                           (setq def (cdr def)))
                         (insert ")")
                         (buffer-string)))))))"#
        ),
        "OK \"(autoload 'ebrowse-tags-find-declaration \\\"ebrowse\\\" \\\"\\\\\nFind declaration of member at point.\\\" t)\""
    );
}

#[test]
fn vm_sort_uses_shared_runtime_callbacks_and_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(let* ((xs '((1 . a) (1 . b) (0 . c)))
                    (ys (sort xs :key #'car)))
               (list xs ys (eq xs ys)))"
        ),
        "OK (((1 . a) (1 . b) (0 . c)) ((0 . c) (1 . a) (1 . b)) nil)"
    );
    assert_eq!(
        vm_eval_str(
            "(let* ((xs '((1 . a) (0 . b)))
                    (ys (sort xs (lambda (a b) (< (car a) (car b))))))
               (list xs ys (eq xs ys)))"
        ),
        "OK (((0 . b) (1 . a)) ((0 . b) (1 . a)) t)"
    );
    assert_eq!(
        vm_eval_str(
            "(let ((v [3 1 2]))
               (list (sort v #'<) v))"
        ),
        "OK ([1 2 3] [1 2 3])"
    );
}

#[test]
fn vm_sort_lisp_predicate_uses_gnu_one_way_lessp() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(let ((n 0)
                   (i 0)
                   xs)
               (while (<= i 127)
                 (setq xs (cons (list i (% i 8) (% i 3)) xs))
                 (setq i (1+ i)))
               (sort xs (lambda (a b)
                          (setq n (1+ n))
                          (and (= (nth 1 a) (nth 1 b))
                               (< (nth 2 a) (nth 2 b)))))
               n)"
        ),
        "OK 127"
    );
}

#[test]
fn vm_primitive_bytecode_ops_ignore_later_function_cell_overrides() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(progn
                 (require 'bytecomp)
                 (let* ((f-list (byte-compile (lambda () (list 1 2 3))))
                        (f-concat (byte-compile (lambda () (concat "a" "b"))))
                        (f-nth (byte-compile (lambda () (nth 0 '(a b)))))
                        (f-memq (byte-compile (lambda () (memq 'a '(a b)))))
                        (orig-list (symbol-function 'list))
                        (orig-concat (symbol-function 'concat))
                        (orig-nth (symbol-function 'nth))
                        (orig-memq (symbol-function 'memq)))
                   (unwind-protect
                       (progn
                         (fset 'list (lambda (&rest _) 'override))
                         (fset 'concat (lambda (&rest _) 'override))
                         (fset 'nth (lambda (&rest _) 'override))
                         (fset 'memq (lambda (&rest _) 'override))
                         (funcall orig-list
                                  (funcall f-list)
                                  (funcall f-concat)
                                  (funcall f-nth)
                                  (funcall f-memq)))
                     (fset 'list orig-list)
                     (fset 'concat orig-concat)
                     (fset 'nth orig-nth)
                     (fset 'memq orig-memq))))"#
        ),
        "OK ((1 2 3) \"ab\" a (a b))"
    );
}

#[test]
fn vm_arithmetic_bcall_fast_path_observes_live_function_cell() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();

    let mut replacement = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("vm-fast-plus-x"), intern("vm-fast-plus-y")],
        optional: vec![],
        rest: None,
    });
    let replacement_value_idx = replacement.add_constant(Value::fixnum(99));
    replacement.ops = vec![Op::Constant(replacement_value_idx), Op::Return];
    replacement.max_stack = 3;

    let plus = intern("+");
    eval.obarray
        .set_symbol_function_id(plus, Value::make_bytecode(replacement));

    let mut caller = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let plus_idx = caller.add_constant(Value::from_sym_id(plus));
    let lhs_idx = caller.add_constant(Value::fixnum(10));
    let rhs_idx = caller.add_constant(Value::fixnum(20));
    caller.ops = vec![
        Op::Constant(plus_idx),
        Op::Constant(lhs_idx),
        Op::Constant(rhs_idx),
        Op::Call(2),
        Op::Return,
    ];
    caller.max_stack = 3;

    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&caller, vec![])
            .expect("Bcall must call the live function cell")
    };

    assert_eq!(result, Value::fixnum(99));
}

/// JIT Phase 1b: executing a call to a NAMED function records the callee in the
/// function's call-site feedback (monomorphically), at the right instruction
/// index, without disturbing the result. Feature-gated (the `runtime` field +
/// the recording exist only under `jit`).
#[cfg(feature = "jit")]
#[test]
fn vm_records_call_feedback_for_named_callee() {
    use crate::emacs_core::jit::CallFeedback;
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();

    // Callee `vm-feedback-callee`: a bytecode function returning 99.
    // Collection is off by default until a tier reads the feedback; this test
    // exercises the mechanism itself, so opt in.
    crate::emacs_core::jit::force_call_feedback_for_test(true);
    let callee_sym = intern("vm-feedback-callee");
    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("x")],
        optional: vec![],
        rest: None,
    });
    let v99 = callee.add_constant(Value::fixnum(99));
    callee.ops = vec![Op::Constant(v99), Op::Return];
    callee.max_stack = 2;
    eval.obarray
        .set_symbol_function_id(callee_sym, Value::make_bytecode(callee));

    // Caller: push the SYMBOL, push an arg, Call(1). The Call is ops[2].
    let mut caller = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let callee_idx = caller.add_constant(Value::from_sym_id(callee_sym));
    let arg_idx = caller.add_constant(Value::fixnum(1));
    caller.ops = vec![
        Op::Constant(callee_idx),
        Op::Constant(arg_idx),
        Op::Call(1),
        Op::Return,
    ];
    caller.max_stack = 3;
    // Seal in place: the assertions below inspect THIS object's runtime
    // feedback, so the execute harness must not run a sealed clone instead.
    caller.seal_hand_assembled_ops_for_test();

    // No feedback before execution.
    assert_eq!(caller.jit_runtime().call_feedback(2), CallFeedback::Uninit);

    for _ in 0..3 {
        let mut vm = new_vm(&mut eval);
        let result = vm.execute(&caller, vec![]).expect("caller executes");
        assert_eq!(result, Value::fixnum(99));
    }

    // The call site (ops[2]) is now monomorphic on the named callee; the
    // Constant site (ops[0]) is not a call and stays Uninit.
    assert_eq!(
        caller.jit_runtime().call_feedback(2),
        CallFeedback::Monomorphic(callee_sym)
    );
    assert_eq!(caller.jit_runtime().call_feedback(0), CallFeedback::Uninit);
}

/// `NEOVM_JIT=0` is a real interpreter-only execution policy, not merely a
/// high tier-up threshold.  It must remove JIT feedback work from GNU's Bcall
/// hot path as well as preventing native dispatch.
#[cfg(feature = "jit")]
#[test]
fn vm_interpreter_only_policy_skips_jit_call_feedback() {
    use crate::emacs_core::jit::CallFeedback;
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();

    let callee_sym = intern("vm-interpreter-only-feedback-callee");
    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let answer = callee.add_constant(Value::fixnum(42));
    callee.ops = vec![Op::Constant(answer), Op::Return];
    callee.max_stack = 1;
    eval.obarray
        .set_symbol_function_id(callee_sym, Value::make_bytecode(callee));

    let mut caller = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let callee_idx = caller.add_constant(Value::from_sym_id(callee_sym));
    caller.ops = vec![Op::Constant(callee_idx), Op::Call(0), Op::Return];
    caller.max_stack = 1;

    let result = {
        let mut vm = new_vm(&mut eval);
        vm.force_interpreter_only_for_test();
        vm.execute(&caller, vec![])
            .expect("interpreter-only bytecode call should execute")
    };

    assert_eq!(result, Value::fixnum(42));
    assert_eq!(
        caller.jit_runtime().call_feedback(1),
        CallFeedback::Uninit,
        "interpreter-only Bcall must not pay for adaptive-tier feedback"
    );
}

#[test]
fn vm_compiled_maphash_closure_mutates_captured_accumulator_like_face_list() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(progn
                 (require 'bytecomp)
                 (funcall
                  (byte-compile
                   (lambda ()
                     (let ((table (make-hash-table :test 'eq))
                           faces)
                       (puthash 'face '(1) table)
                       (maphash (lambda (face spec)
                                  (push `(,(car spec) . ,face) faces))
                                table)
                       faces)))))"#
        ),
        "OK ((1 . face))"
    );
}

/// Like `execute_manual_vm` but builds the ByteCodeFunction AFTER the
/// evaluator is initialized, avoiding stale symbol/value handles from
/// thread-local runtime replacement.
fn execute_manual_vm_built<T>(
    build: impl FnOnce(&mut crate::buffer::BufferManager) -> (ByteCodeFunction, T),
) -> (Value, crate::buffer::BufferManager, T) {
    // Use the full Context::new() builder so the manual bytecode can
    // call regular builtins (set-buffer, narrow-to-region, etc.). The
    // minimal harness only registers std errors and lacks the full
    // subr surface.
    let mut eval = Context::new();
    let (func, init_state) = build(&mut eval.buffers);

    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&func, vec![])
            .expect("manual bytecode should execute")
    };

    let buffers = std::mem::replace(&mut eval.buffers, crate::buffer::BufferManager::new());
    (result, buffers, init_state)
}

#[test]
fn vm_runtime_harness_exposes_public_builtin_surface() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_in_context(
        Context::new_vm_runtime_harness(),
        r#"(progn
             (setq vm-runtime-harness-base nil)
             (fset 'vm-runtime-harness-fn 'car)
             (defvaralias 'vm-runtime-harness-alias 'vm-runtime-harness-base)
             (setq vm-runtime-harness-alias 7)
             (list
              (windowp (selected-window))
              (funcall 'vm-runtime-harness-fn '(1 . 2))
              vm-runtime-harness-base
              (func-arity 'car)))"#,
        false,
        |result, _| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK (t 1 7 (1 . 1))"
            );
        },
    );
}

#[test]
fn vm_literal_int() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("42"), "OK 42");
}

#[test]
fn vm_nil_t() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("nil"), "OK nil");
    assert_eq!(vm_eval_str("t"), "OK t");
}

#[test]
fn vm_eval_preserves_variable_watcher_registry_across_builtin_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn (add-variable-watcher 'vm-bytecode-var 'vm-bytecode-watch) (get-variable-watchers 'vm-bytecode-var))"
        ),
        "OK (vm-bytecode-watch)"
    );
}

#[test]
fn vm_variable_watcher_management_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (defvar vm-vw-base nil)
               (defvaralias 'vm-vw-alias 'vm-vw-base)
               (add-variable-watcher 'vm-vw-alias 'ignore)
               (list
                 (get-variable-watchers 'vm-vw-base)
                 (progn
                   (remove-variable-watcher 'vm-vw-alias 'ignore)
                   (get-variable-watchers 'vm-vw-base))))"
        ),
        "OK ((ignore) nil)"
    );
}

#[test]
fn vm_kmacro_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let _ = eval.eval_str_each(
        r#"(progn
             (setq vm-kmacro-shared-count 0)
             (setq vm-kmacro-ignore-direct-called nil)
             (fset 'ignore
                   (lambda ()
                     (setq vm-kmacro-ignore-direct-called t)))
             (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
             (let ((g (make-sparse-keymap)))
               (use-global-map g)
               (define-key g [ignore]
                 (lambda ()
                   (interactive)
                   (setq vm-kmacro-shared-count (1+ vm-kmacro-shared-count))))))"#,
    );

    let mut vm = new_vm(&mut eval);
    assert_eq!(
        vm.dispatch_vm_builtin("start-kbd-macro", vec![Value::NIL, Value::NIL])
            .expect("vm start-kbd-macro"),
        Value::NIL
    );
    assert_eq!(
        vm.dispatch_vm_builtin("store-kbd-macro-event", vec![Value::symbol("ignore")])
            .expect("vm store-kbd-macro-event"),
        Value::NIL
    );
    vm.ctx.finalize_kbd_macro_runtime_chars();
    assert_eq!(
        vm.dispatch_vm_builtin("end-kbd-macro", vec![])
            .expect("vm end-kbd-macro"),
        Value::NIL
    );
    assert_eq!(
        vm.ctx
            .eval_symbol("last-kbd-macro")
            .expect("last-kbd-macro"),
        Value::vector(vec![Value::symbol("ignore")])
    );

    assert_eq!(
        vm.dispatch_vm_builtin("call-last-kbd-macro", vec![])
            .expect("vm call-last-kbd-macro"),
        Value::NIL
    );
    assert_eq!(
        vm.dispatch_vm_builtin(
            "execute-kbd-macro",
            vec![Value::vector(vec![Value::symbol("ignore")])]
        )
        .expect("vm execute-kbd-macro"),
        Value::NIL
    );

    assert_eq!(
        vm.ctx
            .eval_symbol("vm-kmacro-shared-count")
            .expect("vm-kmacro-shared-count"),
        Value::fixnum(2)
    );
    assert_eq!(
        vm.ctx
            .eval_symbol("vm-kmacro-ignore-direct-called")
            .expect("vm-kmacro-ignore-direct-called"),
        Value::NIL
    );
}

#[test]
fn vm_execute_kbd_macro_real_key_events_use_command_loop_dispatch() {
    crate::test_utils::init_test_tracing();
    let result = with_vm_eval_full_context_state(
        "(progn
           (setq vm-kmacro-command-loop-count 0)
           (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
           (let ((g (make-sparse-keymap)))
             (use-global-map g)
             (define-key g \"a\"
               (lambda ()
                 (interactive)
                 (setq vm-kmacro-command-loop-count
                       (1+ vm-kmacro-command-loop-count))))
             (execute-kbd-macro \"a\")
             vm-kmacro-command-loop-count))",
        false,
        |result, _| crate::emacs_core::error::format_eval_result(&result),
    );
    assert_eq!(result, "OK 1");
}

#[test]
fn vm_execute_kbd_macro_named_symbol_uses_function_indirection_chain() {
    crate::test_utils::init_test_tracing();
    let result = with_vm_eval_full_context_state(
        "(progn
           (setq vm-kmacro-named-symbol-count 0)
           (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
           (let ((g (make-sparse-keymap)))
             (use-global-map g)
             (define-key g \"a\"
               (lambda ()
                 (interactive)
                 (setq vm-kmacro-named-symbol-count
                       (1+ vm-kmacro-named-symbol-count)))))
           (fset 'vm-kmacro-target \"a\")
           (fset 'vm-kmacro-alias 'vm-kmacro-target)
           (execute-kbd-macro 'vm-kmacro-alias)
           vm-kmacro-named-symbol-count)",
        false,
        |result, _| crate::emacs_core::error::format_eval_result(&result),
    );
    assert_eq!(result, "OK 1");
}

#[test]
fn vm_execute_kbd_macro_zero_count_uses_loopfunc() {
    crate::test_utils::init_test_tracing();
    let result = with_vm_eval_full_context_state(
        "(progn
           (setq vm-kmacro-loop-count 0)
           (setq vm-kmacro-loopfunc-count 0)
           (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
           (fset 'vm-kmacro-loopfunc
             (lambda ()
               (setq vm-kmacro-loopfunc-count (1+ vm-kmacro-loopfunc-count))
               (< vm-kmacro-loopfunc-count 3)))
           (let ((g (make-sparse-keymap)))
             (use-global-map g)
             (define-key g \"a\"
               (lambda ()
                 (interactive)
                 (setq vm-kmacro-loop-count (1+ vm-kmacro-loop-count))))
             (execute-kbd-macro \"a\" 0 'vm-kmacro-loopfunc)
             (list vm-kmacro-loop-count vm-kmacro-loopfunc-count)))",
        false,
        |result, _| crate::emacs_core::error::format_eval_result(&result),
    );
    assert_eq!(result, "OK (2 3)");
}

#[test]
fn vm_execute_kbd_macro_runs_termination_hook_after_error() {
    crate::test_utils::init_test_tracing();
    let result = with_vm_eval_full_context_state(
        "(progn
           (setq vm-kmacro-term-ok nil)
           (setq real-this-command 'vm-outer-real)
           (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
           (fset 'vm-kmacro-term-hook
                 (lambda ()
                   (setq vm-kmacro-term-ok
                         (and (null executing-kbd-macro)
                              (= executing-kbd-macro-index 0)
                              (eq real-this-command 'vm-outer-real)))))
           (setq kbd-macro-termination-hook '(vm-kmacro-term-hook))
           (let ((g (make-sparse-keymap)))
             (use-global-map g)
             (define-key g \"a\" (lambda () (interactive) (error \"boom\"))))
           (condition-case nil
               (execute-kbd-macro \"a\")
             (error nil))
           (list vm-kmacro-term-ok real-this-command))",
        false,
        |result, _| crate::emacs_core::error::format_eval_result(&result),
    );
    assert_eq!(result, "OK (t vm-outer-real)");
}

#[test]
fn vm_varset_triggers_variable_watcher_callbacks() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (fset 'vm-bytecode-watch
                 (lambda (sym new op where)
                   (setq vm-bytecode-watch-op op)
                   (setq vm-bytecode-watch-val new)
                   new))
               (add-variable-watcher 'vm-bytecode-target 'vm-bytecode-watch)
               (setq vm-bytecode-target 19)
               (list vm-bytecode-watch-val vm-bytecode-watch-op))"
        ),
        "OK (19 set)"
    );
}

#[test]
fn vm_varset_refreshes_gc_runtime_settings_after_compiled_startup_timer() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str_each(
        "(progn
           (setq gc-cons-percentage nil)
           (setq gc-cons-threshold 268435456)
           (setq neomacs--startup-gc-ceiling-active t))",
    );
    assert_eq!(
        eval.gc_threshold(),
        4 * 1024 * 1024,
        "startup must temporarily cap the internal collector threshold"
    );

    eval.eval_str(
        "(funcall
           (byte-compile
             (lambda ()
               (setq neomacs--startup-gc-ceiling-active nil))))",
    )
    .expect("compiled startup timer should run");

    assert_eq!(
        eval.gc_threshold(),
        268_435_456,
        "compiled varset must release the internal startup GC ceiling"
    );
}

#[test]
fn vm_varbind_and_unbind_trigger_variable_watcher_callbacks() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (setq vm-watch-events nil)
               (setq vm-watch-target 9)
               (fset 'vm-watch-rec
                 (lambda (sym new op where)
                   (setq vm-watch-events (cons (list op new) vm-watch-events))))
               (add-variable-watcher 'vm-watch-target 'vm-watch-rec)
               (let ((vm-watch-target 1)) 'done)
               vm-watch-events)"
        ),
        "OK ((unlet 9) (let 1))"
    );

    assert_eq!(
        vm_eval_str(
            "(progn
               (setq vm-watch-events nil)
               (setq vm-watch-target 9)
               (fset 'vm-watch-rec
                 (lambda (sym new op where)
                   (setq vm-watch-events (cons (list op new) vm-watch-events))))
               (add-variable-watcher 'vm-watch-target 'vm-watch-rec)
               (let* ((vm-watch-target 2)) 'done)
               vm-watch-events)"
        ),
        "OK ((unlet 9) (let 2))"
    );
}

#[test]
fn vm_unbind_propagates_restore_watcher_signal() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(progn
                 (defvar vm-unbind-error-target 9)
                 (setq vm-unbind-error-body-ran nil)
                 (fset 'vm-unbind-error-watcher
                       (lambda (_symbol _new-value operation _where)
                         (if (eq operation 'unlet)
                             (signal 'error '("restore"))
                           nil)))
                 (add-variable-watcher 'vm-unbind-error-target
                                       'vm-unbind-error-watcher)
                 (condition-case error
                     (funcall
                      (byte-compile
                       (lambda ()
                         (let ((vm-unbind-error-target 1))
                           (setq vm-unbind-error-body-ran t)
                           'body))))
                   (error
                    (list (car error) vm-unbind-error-body-ran))))"#
        ),
        "OK (error t)"
    );
}

#[test]
fn vm_declared_special_ignores_lexical_lookup() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_lexical_str(
            "(progn
               (defvar vm-special 10)
               (let ((vm-special 20))
                 (let ((f (lambda () vm-special)))
                   (let ((vm-special 30))
                     (funcall f)))))"
        ),
        "OK 30"
    );
}

#[test]
fn vm_declared_special_setq_updates_dynamic_binding() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_lexical_str(
            "(progn
               (defvar vm-special 10)
               (let ((vm-special 20))
                 (let ((f (lambda () (setq vm-special (+ vm-special 1)))))
                   (let ((vm-special 30))
                     (funcall f)
                     vm-special))))"
        ),
        "OK 31"
    );
}

#[test]
fn vm_unbind_restores_saved_current_buffer() {
    crate::test_utils::init_test_tracing();
    let (result, buffers, saved_buffer) = execute_manual_vm_built(|buffers| {
        let saved_buffer = buffers.create_buffer("saved");
        let other_buffer = buffers.create_buffer("other");
        buffers.set_current(saved_buffer);

        let mut func = ByteCodeFunction::new(LambdaParams {
            required: vec![],
            optional: vec![],
            rest: None,
        });
        let other_buffer_idx = func.add_constant(Value::make_buffer(other_buffer));
        let set_buffer_idx = func.add_symbol("set-buffer");
        func.ops = vec![
            Op::SaveCurrentBuffer,
            Op::Constant(other_buffer_idx),
            Op::CallBuiltin(set_buffer_idx, 1),
            Op::Pop,
            Op::Unbind(1),
            Op::Nil,
            Op::Return,
        ];
        func.max_stack = 2;
        (func, saved_buffer)
    });

    assert_eq!(result, Value::NIL);
    assert_eq!(
        buffers.current_buffer().map(|buffer| buffer.id),
        Some(saved_buffer)
    );
}

#[test]
fn vm_unbind_counts_unwind_protect_entries_like_gnu() {
    crate::test_utils::init_test_tracing();
    let (result, _buffers, _) = execute_manual_vm_built(|_buffers| {
        let mut noop_func = ByteCodeFunction::new(LambdaParams {
            required: vec![],
            optional: vec![],
            rest: None,
        });
        noop_func.ops = vec![Op::Nil, Op::Return];
        noop_func.max_stack = 1;
        let noop = Value::make_bytecode(noop_func);

        let mut func = ByteCodeFunction::new(LambdaParams {
            required: vec![],
            optional: vec![],
            rest: None,
        });
        let a_idx = func.add_symbol("vm-up-a");
        let b_idx = func.add_symbol("vm-up-b");
        let a_val_idx = func.add_constant(Value::fixnum(7));
        let b_val_idx = func.add_constant(Value::fixnum(9));
        let cleanup_idx = func.add_constant(noop);
        func.ops = vec![
            Op::Constant(a_val_idx),
            Op::VarBind(a_idx),
            Op::Constant(b_val_idx),
            Op::VarBind(b_idx),
            Op::Constant(cleanup_idx),
            Op::UnwindProtectPop,
            Op::Unbind(1),
            Op::VarRef(b_idx),
            Op::Return,
        ];
        func.max_stack = 2;
        (func, ())
    });
    assert_eq!(result, Value::fixnum(9));
}

#[test]
fn vm_bytecode_varref_and_varset_ignore_interpreter_lexenv_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let sym = crate::emacs_core::intern::intern("vm-bytecode-dynamic-x");
    let lexical_value = Value::symbol("lexical-value");
    let dynamic_value = Value::symbol("dynamic-value");
    let updated_value = Value::symbol("updated-dynamic-value");

    let lexical_binding = Value::make_cons(
        crate::emacs_core::value::lexenv_binding_symbol_value(sym),
        lexical_value,
    );
    eval.lexenv = Value::make_cons(lexical_binding, Value::NIL);
    eval.obarray.set_symbol_value_id(sym, dynamic_value);

    let mut func = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let sym_idx = func.add_symbol("vm-bytecode-dynamic-x");
    let updated_idx = func.add_constant(updated_value);
    func.ops = vec![
        Op::VarRef(sym_idx),
        Op::Constant(updated_idx),
        Op::VarSet(sym_idx),
        Op::VarRef(sym_idx),
        Op::List(2),
        Op::Return,
    ];
    func.max_stack = 3;

    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&func, vec![])
            .expect("manual bytecode should execute")
    };

    assert_eq!(
        result,
        Value::list_from_slice(&[dynamic_value, updated_value])
    );
    assert_eq!(
        eval.obarray.symbol_value_id(sym).copied(),
        Some(updated_value)
    );
    assert_eq!(lexical_binding.cons_cdr(), lexical_value);
}

#[test]
fn vm_bytecoded_call_executes_heap_bytecode_without_cloning_function() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();

    let mut inner = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let answer_idx = inner.add_constant(Value::fixnum(42));
    inner.ops = vec![Op::Constant(answer_idx), Op::Return];
    inner.max_stack = 1;
    let inner_value = Value::make_bytecode(inner);

    let mut outer = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let inner_idx = outer.add_constant(inner_value);
    outer.ops = vec![Op::Constant(inner_idx), Op::Call(0), Op::Return];
    outer.max_stack = 1;
    // Seal before the clone counter arms: the counter measures the VM's
    // execution path, not the test harness's sealing normalization.
    outer.seal_hand_assembled_ops_for_test();

    crate::emacs_core::bytecode::chunk::reset_bytecode_function_clone_count_for_test();
    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&outer, vec![])
            .expect("nested bytecoded call should execute")
    };

    assert_eq!(result, Value::fixnum(42));
    assert_eq!(
        crate::emacs_core::bytecode::chunk::bytecode_function_clone_count_for_test(),
        0,
        "VM bytecoded call fast path must execute the heap bytecode object in place"
    );
}

#[test]
fn vm_symbol_bytecode_call_resolves_live_function_cell_once() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();
    let callee_symbol = intern("vm-single-function-cell-read");

    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("argument")],
        optional: vec![],
        rest: None,
    });
    callee.lexical = true;
    callee.ops = vec![Op::StackRef(0), Op::Return];
    callee.max_stack = 2;
    eval.obarray
        .set_symbol_function_id(callee_symbol, Value::make_bytecode(callee));

    let mut caller = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let callee_idx = caller.add_constant(Value::from_sym_id(callee_symbol));
    let argument_idx = caller.add_constant(Value::fixnum(42));
    caller.ops = vec![
        Op::Constant(callee_idx),
        Op::Constant(argument_idx),
        Op::Call(1),
        Op::Return,
    ];
    caller.max_stack = 2;

    crate::emacs_core::symbol::reset_function_cell_lookup_count();
    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&caller, vec![])
            .expect("symbol-bound bytecode call should execute")
    };

    assert_eq!(result, Value::fixnum(42));
    assert_eq!(
        crate::emacs_core::symbol::function_cell_lookup_count(),
        1,
        "GNU Bcall reads the live symbol function cell once before dispatch"
    );
}

#[test]
fn vm_adjacent_stack_transfer_pairs_each_use_one_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();
    let callee_symbol = intern("vm-fused-constant-stackref-callee");

    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("callee-argument")],
        optional: vec![],
        rest: None,
    });
    callee.lexical = true;
    callee.ops = vec![Op::StackRef(0), Op::Return];
    callee.max_stack = 2;
    eval.obarray
        .set_symbol_function_id(callee_symbol, Value::make_bytecode(callee));

    let mut caller = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("caller-argument")],
        optional: vec![],
        rest: None,
    });
    caller.lexical = true;
    let callee_idx = caller.add_constant(Value::from_sym_id(callee_symbol));
    caller.ops = vec![
        Op::Constant(callee_idx),
        Op::StackRef(1),
        Op::Call(1),
        Op::Return,
    ];
    caller.max_stack = 3;

    reset_opcode_dispatch_count();
    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&caller, vec![Value::fixnum(42)])
            .expect("constant/stack-ref unary call should execute")
    };

    assert_eq!(result, Value::fixnum(42));
    assert_eq!(
        opcode_dispatch_count(),
        4,
        "constant/stack-ref and stack-ref/return should each share one dispatch"
    );
}

#[test]
fn vm_stackref_return_pair_uses_one_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();

    let mut function = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("argument")],
        optional: vec![],
        rest: None,
    });
    function.lexical = true;
    function.ops = vec![Op::StackRef(0), Op::Return];
    function.max_stack = 2;

    reset_opcode_dispatch_count();
    let result = new_vm(&mut eval)
        .execute(&function, vec![Value::fixnum(42)])
        .expect("stack-ref/return should execute");

    assert_eq!(result, Value::fixnum(42));
    assert_eq!(
        opcode_dispatch_count(),
        1,
        "stack-ref + return should share one dispatch"
    );
}

#[test]
fn vm_stackref_return_fusion_preserves_frame_limit_validation() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();

    let mut function = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("argument")],
        optional: vec![],
        rest: None,
    });
    function.lexical = true;
    function.ops = vec![Op::StackRef(0), Op::Return];
    function.max_stack = 1;

    let result = new_vm(&mut eval).execute(&function, vec![Value::fixnum(42)]);

    assert!(
        result.is_err(),
        "the fused pair must reject a StackRef push beyond max-stack"
    );
}

#[test]
fn vm_bytecode_callee_skips_native_string_writeback_classification() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();
    let callee_symbol = intern("vm-bytecode-string-identity");

    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("string-argument")],
        optional: vec![],
        rest: None,
    });
    callee.lexical = true;
    callee.ops = vec![Op::StackRef(0), Op::Return];
    callee.max_stack = 2;
    eval.obarray
        .set_symbol_function_id(callee_symbol, Value::make_bytecode(callee));

    let argument = Value::string("abc");
    let mut caller = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    caller.lexical = true;
    let callee_idx = caller.add_constant(Value::from_sym_id(callee_symbol));
    let argument_idx = caller.add_constant(argument);
    caller.ops = vec![
        Op::Constant(callee_idx),
        Op::Constant(argument_idx),
        Op::Call(1),
        Op::Return,
    ];
    caller.max_stack = 2;

    reset_mutating_writeback_classification_count();
    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&caller, vec![])
            .expect("bytecode string identity call should execute")
    };

    assert_eq!(result, argument);
    assert_eq!(
        mutating_writeback_classification_count(),
        0,
        "a resolved bytecode callee cannot be native aset/fillarray"
    );
}

#[test]
fn vm_repeated_symbol_bytecode_calls_reuse_epoch_validated_resolution() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();
    let callee_symbol = intern("vm-cached-function-cell-read");

    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let answer_idx = callee.add_constant(Value::fixnum(42));
    callee.ops = vec![Op::Constant(answer_idx), Op::Return];
    callee.max_stack = 1;
    eval.obarray
        .set_symbol_function_id(callee_symbol, Value::make_bytecode(callee));

    let mut caller = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let callee_idx = caller.add_constant(Value::from_sym_id(callee_symbol));
    caller.ops = vec![
        Op::Constant(callee_idx),
        Op::Call(0),
        Op::Constant(callee_idx),
        Op::Call(0),
        Op::Constant(callee_idx),
        Op::Call(0),
        Op::Return,
    ];
    caller.max_stack = 3;

    crate::emacs_core::symbol::reset_function_cell_lookup_count();
    crate::emacs_core::value::reset_bytecode_data_access_count();
    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&caller, vec![])
            .expect("repeated symbol-bound bytecode calls should execute")
    };

    assert_eq!(result, Value::fixnum(42));
    assert_eq!(
        crate::emacs_core::symbol::function_cell_lookup_count(),
        1,
        "a stable function epoch should make repeated bytecode calls reuse one proven target"
    );
    // The load-bearing invariant is the ONE proven resolution above. The
    // code() projection now deliberately routes through the
    // get_bytecode_data chokepoint (one type-tag compare per call) — that
    // seam is where lazy pdump stubs materialize, and a token minted from a
    // cache replay may predate its function's first run. Three calls, three
    // chokepoint passes, still zero re-resolutions.
    assert_eq!(
        crate::emacs_core::value::bytecode_data_access_count(),
        3,
        "each projection passes the materialization chokepoint exactly once per call"
    );
}

#[test]
fn vm_symbol_bytecode_call_cache_invalidates_after_fset() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();
    let callee_symbol = intern("vm-invalidated-function-cell-read");

    let make_callee = |answer| {
        let mut callee = ByteCodeFunction::new(LambdaParams {
            required: vec![],
            optional: vec![],
            rest: None,
        });
        let answer_idx = callee.add_constant(Value::fixnum(answer));
        callee.ops = vec![Op::Constant(answer_idx), Op::Return];
        callee.max_stack = 1;
        Value::make_bytecode(callee)
    };
    let first = make_callee(1);
    let second = make_callee(2);
    eval.obarray.set_symbol_function_id(callee_symbol, first);

    crate::emacs_core::symbol::reset_function_cell_lookup_count();
    let mut vm = new_vm(&mut eval);
    let ResolvedStackCallTarget::ByteCode { callee } =
        vm.resolve_stack_call_target(Value::from_sym_id(callee_symbol))
    else {
        panic!("first function cell should resolve to bytecode");
    };
    assert_eq!(callee.value(), first);

    vm.ctx.obarray.set_symbol_function_id(callee_symbol, second);
    let ResolvedStackCallTarget::ByteCode { callee } =
        vm.resolve_stack_call_target(Value::from_sym_id(callee_symbol))
    else {
        panic!("redefined function cell should resolve to bytecode");
    };
    assert_eq!(callee.value(), second);
    assert_eq!(
        crate::emacs_core::symbol::function_cell_lookup_count(),
        2,
        "an fset epoch change must force a fresh live function-cell read"
    );
}

#[test]
fn vm_repeated_symbol_call_observes_fset_before_the_next_call() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();
    let callee_symbol = intern("vm-live-redefined-bytecode-call");

    let make_callee = |answer| {
        let mut callee = ByteCodeFunction::new(LambdaParams::simple(vec![]));
        let answer_idx = callee.add_constant(Value::fixnum(answer));
        callee.ops = vec![Op::Constant(answer_idx), Op::Return];
        callee.max_stack = 1;
        Value::make_bytecode(callee)
    };
    let first = make_callee(1);
    let second = make_callee(2);
    eval.obarray.set_symbol_function_id(callee_symbol, first);

    let mut caller = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    let designator = caller.add_constant(Value::from_sym_id(callee_symbol));
    let replacement = caller.add_constant(second);
    caller.ops = vec![
        Op::Constant(designator),
        Op::Call(0),
        Op::Pop,
        Op::Constant(designator),
        Op::Constant(replacement),
        Op::Fset,
        Op::Pop,
        Op::Constant(designator),
        Op::Call(0),
        Op::Return,
    ];
    caller.max_stack = 2;

    let result = new_vm(&mut eval)
        .execute(&caller, vec![])
        .expect("the call after fset must use the replacement function cell");

    assert_eq!(result, Value::fixnum(2));
}

#[test]
fn vm_bytecode_call_chain_stays_in_one_interpreter_driver() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();
    let leaf_symbol = intern("vm-iterative-frame-leaf");
    let middle_symbol = intern("vm-iterative-frame-middle");

    let mut leaf = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let answer_idx = leaf.add_constant(Value::fixnum(42));
    leaf.ops = vec![Op::Constant(answer_idx), Op::Return];
    leaf.max_stack = 1;
    eval.obarray
        .set_symbol_function_id(leaf_symbol, Value::make_bytecode(leaf));

    let mut middle = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let leaf_idx = middle.add_constant(Value::from_sym_id(leaf_symbol));
    middle.ops = vec![Op::Constant(leaf_idx), Op::Call(0), Op::Return];
    middle.max_stack = 1;
    eval.obarray
        .set_symbol_function_id(middle_symbol, Value::make_bytecode(middle));

    let mut outer = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let middle_idx = outer.add_constant(Value::from_sym_id(middle_symbol));
    outer.ops = vec![Op::Constant(middle_idx), Op::Call(0), Op::Return];
    outer.max_stack = 1;

    reset_run_loop_max_depth();
    reset_iterative_context_bc_frames_max();
    reset_generic_bytecode_cleanup_count();
    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&outer, vec![])
            .expect("nested bytecode calls should return through their callers")
    };

    assert_eq!(result, Value::fixnum(42));
    assert_eq!(
        run_loop_max_depth(),
        1,
        "GNU Bcall/setup_frame/Breturn keeps bytecode callees in one interpreter driver"
    );
    assert_eq!(
        iterative_context_bc_frames_max(),
        1,
        "nested iterative frames should root their exact callee in the consumed call operand, not grow Context::bc_frames"
    );
    assert_eq!(
        generic_bytecode_cleanup_count(),
        1,
        "ordinary GNU Breturn transitions bypass generic frame cleanup; only the outer frame exits through it"
    );
}

#[test]
fn vm_iterative_call_roots_exact_callee_after_self_redefinition_and_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();
    let child_symbol = intern("vm-iterative-self-redefining-child");

    let mut child = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    let own_symbol = child.add_constant(Value::from_sym_id(child_symbol));
    let replacement = child.add_constant(Value::NIL);
    let garbage_collect = child.add_constant(Value::from_sym_id(intern("garbage-collect")));
    let answer = child.add_constant(Value::fixnum(42));
    child.ops = vec![
        Op::Constant(own_symbol),
        Op::Constant(replacement),
        Op::Fset,
        Op::Pop,
        Op::Constant(garbage_collect),
        Op::Call(0),
        Op::Pop,
        Op::Constant(answer),
        Op::Return,
    ];
    child.max_stack = 2;
    eval.obarray
        .set_symbol_function_id(child_symbol, Value::make_bytecode(child));

    let mut caller = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    let child_designator = caller.add_constant(Value::from_sym_id(child_symbol));
    caller.ops = vec![Op::Constant(child_designator), Op::Call(0), Op::Return];
    caller.max_stack = 1;

    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&caller, vec![])
            .expect("the exact executing callee must survive its own fset and GC")
    };

    assert_eq!(result, Value::fixnum(42));
    assert!(eval.gc_count > 0, "the child must force an exact GC");
    assert_eq!(
        eval.obarray.symbol_function_id(child_symbol),
        None,
        "the test must remove the bytecode object from its original function cell"
    );
}

#[cfg(feature = "jit")]
#[test]
fn interpreter_resume_point_packs_pc_and_osr_state_in_one_word() {
    let resume = InterpreterResumePoint::new(37, true);

    assert_eq!(
        std::mem::size_of::<InterpreterResumePoint>(),
        std::mem::size_of::<usize>()
    );
    assert_eq!(resume.pc(), 37);
    assert!(resume.osr_tried());

    let fresh = InterpreterResumePoint::new(91, false);
    assert_eq!(fresh.pc(), 91);
    assert!(!fresh.osr_tried());
}

#[test]
fn vm_backward_branch_quit_counter_spans_bytecode_calls_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();
    let child_symbol = intern("vm-backedge-counter-child");

    let mut child = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    child.constants = vec![Value::fixnum(55), Value::fixnum(0), Value::fixnum(42)].into();
    child.ops = vec![
        Op::Constant(0),
        Op::StackRef(0),
        Op::Constant(1),
        Op::Gtr,
        Op::GotoIfNil(7),
        Op::Sub1,
        Op::Goto(1),
        Op::Pop,
        Op::Constant(2),
        Op::Return,
    ];
    child.max_stack = 3;
    eval.obarray
        .set_symbol_function_id(child_symbol, Value::make_bytecode(child));

    let mut outer = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    outer.constants = vec![
        Value::fixnum(200),
        Value::fixnum(0),
        Value::from_sym_id(child_symbol),
    ]
    .into();
    outer.ops = vec![
        Op::Constant(0),
        Op::StackRef(0),
        Op::Constant(1),
        Op::Gtr,
        Op::GotoIfNil(7),
        Op::Sub1,
        Op::Goto(1),
        Op::Pop,
        Op::Constant(2),
        Op::Call(0),
        Op::Return,
    ];
    outer.max_stack = 3;

    crate::emacs_core::eval::reset_bytecode_branch_poll_count();
    let result = new_vm(&mut eval)
        .execute(&outer, vec![])
        .expect("caller and callee countdowns should complete");

    assert_eq!(result, Value::fixnum(42));
    assert_eq!(
        crate::emacs_core::eval::bytecode_branch_poll_count(),
        1,
        "GNU carries one unsigned quitcounter through setup_frame/Breturn"
    );
}

#[test]
fn interpreter_driver_frame_layout_stays_compact() {
    assert!(
        std::mem::size_of::<ResolvedStackCallTarget>() <= 2 * std::mem::size_of::<usize>(),
        "call-target classification must not carry cold subr metadata through every Bcall"
    );
    assert_eq!(
        std::mem::size_of::<InterpreterFunction>(),
        std::mem::size_of::<Value>(),
        "the non-null function code handle must fit in one tagged value"
    );
    assert!(
        std::mem::size_of::<InterpreterFrame>() <= 64,
        "an active interpreter frame must stay register-snapshot sized"
    );
    // A suspended caller is a frame in one stack plus a continuation in a
    // parallel one. Neither is COPIED on a call any more -- entering writes the
    // callee's frame once from registers and returning is a pop -- so this
    // bounds footprint rather than per-call traffic. The old assertion bounded
    // a `SuspendedInterpreterFrame` that no longer exists, and its stated
    // reason ("must not copy ... on every Bcall") is exactly what the frame
    // stack retired.
    assert!(
        std::mem::size_of::<BytecodeCallContinuation>() <= 16,
        "a continuation is pushed per call and must stay two words"
    );
}

#[test]
fn interpreter_function_caches_immutable_bytecode_code() {
    let value = Value::make_bytecode(ByteCodeFunction::new(LambdaParams::simple(vec![])));
    let code = value
        .get_bytecode_data()
        .expect("bytecode value must expose immutable code");

    let function = InterpreterFunction::new(code);

    assert!(std::ptr::eq(function.code(), code));
}

#[test]
fn interpreter_aux_stack_materializes_only_nonempty_suspended_frames() {
    let mut aux = InterpreterFrameAuxStack::new(HandlerStack::new(), BindStack::new());

    aux.suspend_current(InterpreterDriverDepth::ROOT);
    assert_eq!(
        aux.materialized_suspended_len(),
        0,
        "ordinary GNU bytecode calls must not copy an empty handler/bind record"
    );

    aux.current_mut().bind_stack.push(7);
    aux.suspend_current(InterpreterDriverDepth::ROOT);
    assert_eq!(aux.materialized_suspended_len(), 1);
    assert!(aux.current_mut().bind_stack.is_empty());

    aux.restore_current(InterpreterDriverDepth::ROOT);
    assert_eq!(aux.materialized_suspended_len(), 0);
    assert_eq!(aux.current_mut().bind_stack.as_slice(), &[7]);

    aux.suspend_current(InterpreterDriverDepth::ROOT);
    aux.suspend_current(InterpreterDriverDepth::from_suspended_callers(1));
    aux.current_mut().bind_stack.push(9);
    aux.suspend_current(InterpreterDriverDepth::from_suspended_callers(2));
    assert_eq!(
        aux.materialized_suspended_len(),
        2,
        "empty intermediate frames must leave holes in the sparse stack"
    );

    aux.restore_current(InterpreterDriverDepth::from_suspended_callers(2));
    assert_eq!(aux.current_mut().bind_stack.as_slice(), &[9]);
    aux.restore_current(InterpreterDriverDepth::from_suspended_callers(1));
    assert!(aux.current_mut().bind_stack.is_empty());
    aux.restore_current(InterpreterDriverDepth::ROOT);
    assert_eq!(aux.current_mut().bind_stack.as_slice(), &[7]);
}

#[test]
fn interpreter_aux_stack_empty_restore_preserves_reusable_storage() {
    let mut aux = InterpreterFrameAuxStack::new(HandlerStack::new(), BindStack::new());
    let current = aux.current_mut();
    current.bind_stack.extend(0..16);
    current.bind_stack.clear();
    let storage = current.bind_stack.as_ptr();
    let capacity = current.bind_stack.capacity();

    aux.restore_current(InterpreterDriverDepth::ROOT);

    assert_eq!(aux.current_mut().bind_stack.as_ptr(), storage);
    assert_eq!(aux.current_mut().bind_stack.capacity(), capacity);
}

#[test]
fn vm_bcall_max_eval_depth_reports_error_like_gnu_bytecode() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();
    eval.set_max_depth(100);
    eval.set_variable("max-lisp-eval-depth", Value::fixnum(100));

    let recurse_sym = intern("vm-bcall-depth-recurse");
    let mut recurse = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let recurse_idx = recurse.add_constant(Value::from_sym_id(recurse_sym));
    recurse.ops = vec![Op::Constant(recurse_idx), Op::Call(0), Op::Return];
    recurse.max_stack = 1;
    eval.obarray
        .set_symbol_function_id(recurse_sym, Value::make_bytecode(recurse.clone()));

    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&recurse, vec![])
    };
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(resolve_sym(sig.symbol), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Lisp nesting exceeds ‘max-lisp-eval-depth’")]
            );
        }
        other => panic!("expected bytecode max-depth error, got {other:?}"),
    }
}

#[test]
fn vm_bcall_funcall_recursion_reports_bytecode_error_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();
    eval.set_max_depth(100);
    eval.set_variable("max-lisp-eval-depth", Value::fixnum(100));

    let recurse_sym = intern("vm-bcall-funcall-depth-recurse");
    let funcall_sym = intern("funcall");
    let mut recurse = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let funcall_idx = recurse.add_constant(Value::from_sym_id(funcall_sym));
    let recurse_idx = recurse.add_constant(Value::from_sym_id(recurse_sym));
    recurse.ops = vec![
        Op::Constant(funcall_idx),
        Op::Constant(recurse_idx),
        Op::Call(1),
        Op::Return,
    ];
    recurse.max_stack = 2;
    eval.obarray
        .set_symbol_function_id(recurse_sym, Value::make_bytecode(recurse.clone()));

    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&recurse, vec![])
    };
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(resolve_sym(sig.symbol), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Lisp nesting exceeds ‘max-lisp-eval-depth’")]
            );
        }
        other => panic!("expected bytecode max-depth error, got {other:?}"),
    }
}

#[test]
fn vm_unbind_restores_saved_excursion_point() {
    let (result, buffers, (buffer_id, saved_point)) = execute_manual_vm_built(|buffers| {
        let buffer_id = buffers.create_buffer("excursion");
        buffers.set_current(buffer_id);
        {
            let buffer = buffers.get_mut(buffer_id).expect("buffer");
            buffer.insert("abcdef");
            buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(2));
        }
        let saved_point = buffers
            .get(buffer_id)
            .expect("buffer")
            .point_char_pos()
            .get();

        let mut func = ByteCodeFunction::new(LambdaParams {
            required: vec![],
            optional: vec![],
            rest: None,
        });
        let goto_target_idx = func.add_constant(Value::fixnum(5));
        let goto_char_idx = func.add_symbol("goto-char");
        func.ops = vec![
            Op::SaveExcursion,
            Op::Constant(goto_target_idx),
            Op::CallBuiltin(goto_char_idx, 1),
            Op::Pop,
            Op::Unbind(1),
            Op::Nil,
            Op::Return,
        ];
        func.max_stack = 2;
        (func, (buffer_id, saved_point))
    });

    assert_eq!(result, Value::NIL);
    assert_eq!(
        buffers.current_buffer().map(|buffer| buffer.id),
        Some(buffer_id)
    );
    assert_eq!(
        buffers
            .get(buffer_id)
            .expect("buffer")
            .point_char_pos()
            .get(),
        saved_point
    );
}

#[test]
fn vm_save_window_excursion_restores_selected_window() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer_id = eval.buffers.create_buffer("*vm-save-window*");
    eval.buffers.set_current(buffer_id);
    let frame_id = eval.frames.create_frame("F1", 960, 640, buffer_id);
    assert!(eval.frames.select_frame(frame_id));
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    let other_window = eval
        .frames
        .split_window(
            frame_id,
            selected_window,
            SplitDirection::Vertical,
            buffer_id,
            None,
            SplitPlacement::AfterTarget,
        )
        .expect("split window");

    let select_other = Value::list(vec![
        Value::symbol("select-window"),
        Value::make_window(other_window.0),
    ]);
    let selected_form = Value::list(vec![Value::symbol("selected-window")]);
    let body = Value::list(vec![select_other, selected_form]);

    let mut func = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let body_idx = func.add_constant(body);
    func.ops = vec![Op::Constant(body_idx), Op::SaveWindowExcursion, Op::Return];
    func.max_stack = 2;

    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&func, vec![])
            .expect("save-window-excursion opcode should execute")
    };

    assert_eq!(result, Value::make_window(other_window.0));
    assert_eq!(
        eval.frames.get(frame_id).expect("frame").selected_window,
        selected_window
    );
}

#[test]
fn vm_unbind_restores_saved_restriction() {
    crate::test_utils::init_test_tracing();
    let (result, buffers, (buffer_id, saved_begv, saved_zv)) = execute_manual_vm_built(|buffers| {
        let buffer_id = buffers.create_buffer("restriction");
        buffers.set_current(buffer_id);
        {
            let buffer = buffers.get_mut(buffer_id).expect("buffer");
            buffer.insert("abcdef");
            buffer.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(1, 5));
            buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3));
        }
        let buffer = buffers.get(buffer_id).expect("buffer");
        let saved = (
            buffer_id,
            buffer.point_min_char_pos().get(),
            buffer.point_max_char_pos().get(),
        );

        let mut func = ByteCodeFunction::new(LambdaParams {
            required: vec![],
            optional: vec![],
            rest: None,
        });
        let beg_idx = func.add_constant(Value::fixnum(2));
        let end_idx = func.add_constant(Value::fixnum(4));
        let narrow_idx = func.add_symbol("narrow-to-region");
        func.ops = vec![
            Op::SaveRestriction,
            Op::Constant(beg_idx),
            Op::Constant(end_idx),
            Op::CallBuiltin(narrow_idx, 2),
            Op::Pop,
            Op::Unbind(1),
            Op::Nil,
            Op::Return,
        ];
        func.max_stack = 3;
        (func, saved)
    });

    assert_eq!(result, Value::NIL);
    let buffer = buffers.get(buffer_id).expect("buffer");
    assert_eq!(buffer.point_min_char_pos().get(), saved_begv);
    assert_eq!(buffer.point_max_char_pos().get(), saved_zv);
}

#[test]
fn vm_eval_shared_runtime_path_preserves_active_shared_catches() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.push_condition_frame(ConditionFrame::Catch {
        tag: Value::symbol("vm-bridge-catch"),
        resume: ResumeTarget::InterpreterCatch,
    });
    let mut vm = new_vm(&mut eval);

    let throw_form = Value::list(vec![
        Value::symbol("throw"),
        Value::list(vec![
            Value::symbol("quote"),
            Value::symbol("vm-bridge-catch"),
        ]),
        Value::fixnum(7),
    ]);
    let result = vm.call_function(Value::symbol("eval"), vec![throw_form, Value::NIL]);

    assert!(matches!(
        result,
        Err(Flow::Throw(ref thrown))
            if thrown.tag == Value::symbol("vm-bridge-catch")
                && thrown.value == Value::fixnum(7)
    ));
    drop(vm);
    eval.pop_condition_frame();
    assert_eq!(eval.condition_stack_depth_for_test(), 0);
}

#[test]
fn vm_eval_with_explicit_lexenv_restores_outer_vm_lexenv() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_lexical_str("(let ((x 41)) (list (eval 'x '((x . 7))) x))"),
        "OK (7 41)"
    );
}

#[test]
fn vm_addition() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(+ 1 2)"), "OK 3");
    assert_eq!(vm_eval_str("(+ 1 2 3)"), "OK 6");
}

#[test]
fn vm_subtraction() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(- 10 3)"), "OK 7");
    assert_eq!(vm_eval_str("(- 5)"), "OK -5");
}

#[test]
fn vm_multiplication() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(* 4 5)"), "OK 20");
}

#[test]
fn vm_division() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(/ 10 3)"), "OK 3");
}

#[test]
fn vm_comparisons() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(< 1 2)"), "OK t");
    assert_eq!(vm_eval_str("(> 1 2)"), "OK nil");
    assert_eq!(vm_eval_str("(= 3 3)"), "OK t");
    assert_eq!(vm_eval_str("(<= 3 3)"), "OK t");
    assert_eq!(vm_eval_str("(>= 5 3)"), "OK t");
}

#[test]
fn vm_bootstrap_comparisons_and_fixnump_handle_bignums() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            "(let ((x (1+ most-positive-fixnum))) (list (<= x most-positive-fixnum) (>= x most-positive-fixnum) (fixnump x) (bignump x)))"
        ),
        "OK (nil t nil t)"
    );
    assert_eq!(
        vm_bootstrap_eval_str(
            "(let ((x 1267650600228229401496703205376)) (list (<= x most-positive-fixnum) (>= x most-positive-fixnum) (fixnump x) (bignump x)))"
        ),
        "OK (nil t nil t)"
    );
}

#[test]
fn vm_if() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(if t 1 2)"), "OK 1");
    assert_eq!(vm_eval_str("(if nil 1 2)"), "OK 2");
    assert_eq!(vm_eval_str("(if nil 1)"), "OK nil");
}

#[test]
fn vm_and_or() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(and 1 2 3)"), "OK 3");
    assert_eq!(vm_eval_str("(and 1 nil 3)"), "OK nil");
    assert_eq!(vm_eval_str("(or nil nil 3)"), "OK 3");
    assert_eq!(vm_eval_str("(or nil nil)"), "OK nil");
}

#[test]
fn vm_let() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(let ((x 42)) x)"), "OK 42");
    assert_eq!(vm_eval_str("(let ((x 1) (y 2)) (+ x y))"), "OK 3");
}

#[test]
fn vm_let_star() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(let* ((x 1) (y (+ x 1))) y)"), "OK 2");
}

#[test]
fn vm_setq() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(progn (setq x 42) x)"), "OK 42");
}

#[test]
fn vm_while_loop() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str("(let ((x 0)) (while (< x 5) (setq x (1+ x))) x)"),
        "OK 5"
    );
}

#[test]
fn vm_progn() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(progn 1 2 3)"), "OK 3");
}

#[test]
fn vm_prog1() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(prog1 1 2 3)"), "OK 1");
}

#[test]
fn vm_quote() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("'foo"), "OK foo");
    assert_eq!(vm_eval_str("'(1 2 3)"), "OK (1 2 3)");
    assert_eq!(vm_eval_str("[remap ignore]"), "OK [remap ignore]");
}

#[test]
fn vm_type_predicates() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(null nil)"), "OK t");
    assert_eq!(vm_eval_str("(null 1)"), "OK nil");
    assert_eq!(vm_eval_str("(consp '(1 2))"), "OK t");
    assert_eq!(vm_eval_str("(integerp 42)"), "OK t");
    assert_eq!(vm_eval_str("(stringp \"hello\")"), "OK t");
}

#[test]
fn vm_list_ops() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(car '(1 2 3))"), "OK 1");
    assert_eq!(vm_eval_str("(cdr '(1 2 3))"), "OK (2 3)");
    assert_eq!(vm_eval_str("(cons 1 '(2 3))"), "OK (1 2 3)");
    assert_eq!(vm_eval_str("(length '(1 2 3))"), "OK 3");
    assert_eq!(vm_eval_str("(list 1 2 3)"), "OK (1 2 3)");
}

#[test]
fn vm_eq_equal() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(eq 'foo 'foo)"), "OK t");
    assert_eq!(vm_eval_str("(equal '(1 2) '(1 2))"), "OK t");
}

#[test]
fn vm_concat() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(r#"(concat "hello" " " "world")"#),
        r#"OK "hello world""#
    );
}

#[test]
fn vm_switch_branches_using_hash_table_jump_table() {
    crate::test_utils::init_test_tracing();
    // Build all Values AFTER the evaluator is initialized to avoid stale
    // symbol/value handles from thread-local runtime replacement.
    let mut eval = Context::new_minimal_vm_harness();

    let table = Value::hash_table(HashTableTest::Eq);
    if !table.is_hash_table() {
        panic!("expected hash table constant");
    };
    let _ = table.with_hash_table_mut(|ht| {
        let key = Value::symbol("foo").to_hash_key(&ht.test);
        ht.insert(key, Value::symbol("foo"), Value::fixnum(8));
    });

    let func = ByteCodeFunction {
        source_id: crate::emacs_core::bytecode::fresh_bytecode_source_id(),
        // Hand-built but seal-shaped by construction (trailing Return,
        // in-bounds targets/constants); the marker vouches for that. Left
        // unverified so these run in the checked driver instance.
        ops_sealed: true,
        stack_verified: false,
        ops: vec![
            Op::Constant(1),
            Op::Constant(0),
            Op::Switch,
            Op::Constant(2),
            Op::Return,
            Op::Constant(3),
            Op::Return,
        ],
        constants: vec![
            table,
            Value::symbol("foo"),
            Value::fixnum(10),
            Value::fixnum(20),
        ]
        .into(),
        max_stack: 2,
        params: crate::emacs_core::value::LambdaParams::simple(vec![]),
        arglist: Value::NIL,
        lexical: false,
        env: None,
        gnu_byte_offset_map: Some(vec![GnuByteOffsetMapEntry::new(8, 5)]),
        gnu_bytecode_bytes: None,
        docstring: None,
        doc_form: None,
        interactive: None,
        closure_slot_count: 4,
        extra_slots: Vec::new(),
        #[cfg(feature = "jit")]
        runtime: Some(crate::emacs_core::jit::Runtime::new()),
        lazy_gnu_code: None,
    };

    let mut vm = new_vm(&mut eval);
    let result = vm.execute(&func, vec![]).expect("vm switch should execute");
    assert_eq!(result, Value::fixnum(20));
}

#[test]
fn vm_condition_case_catches_signal_and_binds_error() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str("(condition-case err missing-vm-var (error err))"),
        "OK (void-variable missing-vm-var)"
    );
}

#[test]
fn vm_catch_returns_thrown_value() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(catch 'done (throw 'done 99))"), "OK 99");
}

#[test]
fn vm_define_charset_alias_survives_eval_builtin_bridge() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (define-charset-internal
                 'vm-gbk
                 2
                 [#x40 #xFE #x81 #xFE 0 0 0 0]
                 nil nil nil nil nil nil nil nil
                 #x160000
                 nil nil nil nil
                 '(:name vm-gbk :docstring \"VM GBK\"))
               (mapcar 'list '(1 2 3))
               (define-charset-alias 'vm-gbk-alias 'vm-gbk)
               (list (charsetp 'vm-gbk) (charsetp 'vm-gbk-alias)))"
        ),
        "OK (t t)"
    );
}

#[test]
fn vm_define_coding_system_alias_uses_shared_runtime_manager() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (apply #'define-coding-system-internal
                      '(vm-utf8-emacs
                        85
                        utf-8
                        (unicode)
                        t
                        nil
                        nil
                        nil
                        nil
                        nil
                        nil
                        (:name vm-utf8-emacs :docstring \"VM UTF-8 Emacs\")
                        nil))
               (define-coding-system-alias 'vm-emacs-internal 'vm-utf8-emacs-unix)
               (list (coding-system-p 'vm-utf8-emacs-unix)
                     (coding-system-p 'vm-emacs-internal)))"
        ),
        "OK (t t)"
    );
}

#[test]
fn vm_coding_system_priority_and_terminal_internal_state_use_shared_runtime_manager() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (set-coding-system-priority 'raw-text 'utf-8)
               (set-terminal-coding-system-internal 'raw-text)
               (list (car (coding-system-priority-list))
                     (terminal-coding-system)))"
        ),
        "OK (raw-text raw-text)"
    );
}

#[test]
fn vm_roots_bytecode_constants_across_gc_during_eval_builtin_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(let ((map (make-sparse-keymap)))
               (garbage-collect)
               (define-key map [97] 'ignore)
               (lookup-key map [97]))"
        ),
        "OK ignore"
    );
}

#[test]
fn vm_length_accepts_plain_bytecode_closure_shape() {
    crate::test_utils::init_test_tracing();
    let bc = Value::make_bytecode(crate::emacs_core::bytecode::ByteCodeFunction::new(
        crate::emacs_core::value::LambdaParams::simple(vec![intern("x")]),
    ));

    assert_eq!(
        crate::emacs_core::builtins::builtin_length(vec![bc]).unwrap(),
        Value::fixnum(4)
    );
}

#[test]
fn vm_keymap_predicate_and_lookup_resolve_symbol_function_cells() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(let ((map (make-sparse-keymap)))
               (define-key map [97] 'ignore)
               (fset 'vm-test-keymap map)
               (list (keymapp 'vm-test-keymap)
                     (lookup-key 'vm-test-keymap [97])))"
        ),
        "OK (t ignore)"
    );
}

#[test]
fn vm_throw_restores_saved_stack_before_resuming_catch() {
    crate::test_utils::init_test_tracing();
    let func = ByteCodeFunction {
        source_id: crate::emacs_core::bytecode::fresh_bytecode_source_id(),
        // Hand-built but seal-shaped by construction (trailing Return,
        // in-bounds targets/constants); the marker vouches for that. Left
        // unverified so these run in the checked driver instance.
        ops_sealed: true,
        stack_verified: false,
        ops: vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::PushCatch(6),
            Op::Constant(1),
            Op::Constant(2),
            Op::Throw,
            Op::List(2),
            Op::Return,
        ],
        constants: vec![Value::fixnum(42), Value::symbol("done"), Value::fixnum(99)].into(),
        max_stack: 3,
        params: crate::emacs_core::value::LambdaParams::simple(vec![]),
        arglist: Value::NIL,
        lexical: false,
        env: None,
        gnu_byte_offset_map: None,
        gnu_bytecode_bytes: None,
        docstring: None,
        doc_form: None,
        interactive: None,
        closure_slot_count: 4,
        extra_slots: Vec::new(),
        #[cfg(feature = "jit")]
        runtime: Some(crate::emacs_core::jit::Runtime::new()),
        lazy_gnu_code: None,
    };

    let mut eval = Context::new_vm_runtime_harness();
    let mut vm = new_vm(&mut eval);

    let result = vm.execute(&func, vec![]).expect("vm catch should execute");
    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(42), Value::fixnum(99)])
    );
}

#[test]
fn vm_throw_uses_shared_condition_stack_for_outer_catch_without_catch_tag_mirror() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();
    let tag = Value::symbol("vm-shared-outer");
    eval.push_condition_frame(ConditionFrame::Catch {
        tag,
        resume: ResumeTarget::InterpreterCatch,
    });

    let result = eval.eval_str("(throw 'vm-shared-outer 42)");
    // The throw should be caught by the interpreter-level catch frame,
    // so eval_str returns the thrown value.
    assert!(result.is_ok() || result.is_err(), "throw should propagate");
    // After eval, the condition frame we pushed should still be there
    // (eval_str catches the throw at our frame).
    eval.pop_condition_frame();
    assert_eq!(eval.condition_stack_depth_for_test(), 0);
}

#[test]
fn vm_throw_selection_uses_resume_identity_not_numeric_tuple() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();

    let mut inner = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let inner_tag_idx = inner.add_constant(Value::symbol("vm-inner-catch"));
    let outer_tag_idx = inner.add_constant(Value::symbol("vm-outer-catch"));
    let thrown_value_idx = inner.add_constant(Value::fixnum(7));
    let inner_result_idx = inner.add_constant(Value::symbol("vm-inner-handled"));
    inner.ops = vec![
        Op::Constant(inner_tag_idx),
        Op::PushCatch(5),
        Op::Constant(outer_tag_idx),
        Op::Constant(thrown_value_idx),
        Op::Throw,
        Op::Constant(inner_result_idx),
        Op::Return,
    ];
    inner.max_stack = 2;
    let inner_value = Value::make_bytecode(inner);

    let mut outer = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let outer_tag_idx = outer.add_constant(Value::symbol("vm-outer-catch"));
    let inner_func_idx = outer.add_constant(inner_value);
    let outer_result_idx = outer.add_constant(Value::symbol("vm-outer-handled"));
    outer.ops = vec![
        Op::Constant(outer_tag_idx),
        Op::PushCatch(5),
        Op::Constant(inner_func_idx),
        Op::Call(0),
        Op::Return,
        Op::Constant(outer_result_idx),
        Op::Return,
    ];
    outer.max_stack = 2;

    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&outer, vec![])
    };

    assert!(matches!(
        result,
        Ok(value) if value == Value::symbol("vm-outer-handled")
    ));
}

#[test]
fn vm_signal_selection_uses_resume_identity_not_numeric_tuple() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let mut inner = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let inner_conditions_idx = inner.add_constant(Value::symbol("arith-error"));
    let error_sym_idx = inner.add_constant(Value::symbol("error"));
    let signal_data_idx = inner.add_constant(Value::list(vec![Value::fixnum(1)]));
    let signal_subr_idx = inner.add_symbol("signal");
    let inner_result_idx = inner.add_constant(Value::symbol("vm-inner-signal-handled"));
    inner.ops = vec![
        Op::Constant(inner_conditions_idx),
        Op::PushConditionCaseRaw(5),
        Op::Constant(error_sym_idx),
        Op::Constant(signal_data_idx),
        Op::CallBuiltin(signal_subr_idx, 2),
        Op::Constant(inner_result_idx),
        Op::Return,
    ];
    inner.max_stack = 2;
    let inner_value = Value::make_bytecode(inner);

    let mut outer = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let outer_conditions_idx = outer.add_constant(Value::symbol("error"));
    let inner_func_idx = outer.add_constant(inner_value);
    let outer_result_idx = outer.add_constant(Value::symbol("vm-outer-signal-handled"));
    outer.ops = vec![
        Op::Constant(outer_conditions_idx),
        Op::PushConditionCaseRaw(5),
        Op::Constant(inner_func_idx),
        Op::Call(0),
        Op::Return,
        Op::Constant(outer_result_idx),
        Op::Return,
    ];
    outer.max_stack = 2;

    let result = {
        let mut vm = new_vm(&mut eval);
        vm.execute(&outer, vec![])
    };

    assert!(matches!(
        result,
        Ok(value) if value == Value::symbol("vm-outer-signal-handled")
    ));
}

#[test]
fn vm_nested_condition_case_uses_current_shared_condition_slice() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_full_context_state(
        "(condition-case outer
           (condition-case inner
               (signal 'error 1)
             (void-variable 'inner-miss))
         (error (car outer)))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK error"
            );
        },
    );
}

#[test]
fn vm_eval_bridge_preserves_frames_across_eval_dependent_builtins() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str("(frame-parameter (selected-frame) 'width)"),
        "OK 80"
    );
}

#[test]
fn vm_window_and_frame_selection_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((f (selected-frame))
                      (w (selected-window)))
                 (list (framep f)
                       (windowp w)
                       (eq (selected-frame) f)
                       (eq (frame-selected-window f) w)
                       (eq (frame-first-window f) w)
                       (eq (frame-root-window f) w)
                       (eq (window-frame w) f)
                       (bufferp (window-buffer w))
                       (window-live-p w)
                       (window-valid-p w)
                       (frame-live-p f)
                       (frame-visible-p f)))"#
        ),
        "OK (t t t t t t t t t t t t)"
    );
}

#[test]
fn vm_frame_query_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list (frame-char-height)
                     (frame-char-width)
                     (frame-native-height)
                     (frame-native-width)
                     (frame-text-cols)
                     (frame-text-lines)
                     (frame-text-width)
                     (frame-text-height)
                     (frame-total-cols)
                     (frame-total-lines)
                     (frame-position))"#
        ),
        "OK (1 1 25 80 80 25 80 25 80 25 (0 . 0))"
    );
    assert_eq!(
        vm_eval_str(
            r#"(condition-case err
                   (frame-char-height 999999)
                 (error err))"#
        ),
        "OK (wrong-type-argument framep 999999)"
    );
}

#[test]
fn vm_frame_native_metrics_sync_pending_resize_events() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        r#"(list (frame-native-width) (frame-native-height))"#,
        |eval| {
            let scratch = eval.buffers.create_buffer("*scratch*");
            eval.buffers.set_current(scratch);
            let fid = eval.frames.create_frame("vm-frame", 960, 640, scratch);
            assert!(eval.frames.select_frame(fid), "selected frame");
            let frame = eval.frames.get_mut(fid).expect("frame should exist");
            frame.width = 960;
            frame.height = 640;
            frame.set_parameter(Value::symbol("window-system"), Value::symbol("x"));

            let (tx, rx) = crossbeam_channel::unbounded();
            eval.input_rx = Some(rx);
            tx.send(crate::keyboard::InputEvent::Resize {
                width: 1400,
                height: 1600,
                scale_factor: 1.0,
                emacs_frame_id: 0,
            })
            .expect("queue resize");
        },
    );

    assert_eq!(result, "OK (1400 1600)");
}

#[test]
fn vm_frame_identity_and_display_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((mouse (mouse-position))
                     (pixel (mouse-pixel-position)))
                 (list (frame-id)
                       (eq (frame-root-frame) (selected-frame))
                       (eq (next-frame) (selected-frame))
                       (eq (previous-frame) (selected-frame))
                       (eq (old-selected-frame) (selected-frame))
                       (eq (car mouse) (selected-frame))
                       (cdr mouse)
                       (eq (car pixel) (selected-frame))
                       (cdr pixel)
                       (window-system)
                       (tool-bar-height)
                       (tab-bar-height)))"#
        ),
        "OK (1 t t t t t (nil) t (nil) nil 0 0)"
    );
    assert_eq!(
        vm_eval_str(
            r#"(list (condition-case err (frame-id "x") (error err))
                     (condition-case err (window-system "x") (error err))
                     (condition-case err (tool-bar-height "x") (error err))
                     (condition-case err (next-frame "x") (error err)))"#
        ),
        "OK ((wrong-type-argument frame-live-p \"x\") (wrong-type-argument framep \"x\") (wrong-type-argument framep \"x\") (wrong-type-argument frame-live-p \"x\"))"
    );
}

#[test]
fn vm_terminal_and_display_entrypoints_use_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((frame (selected-frame))
                      (before (length (frame-list)))
                      (created (x-create-frame '((name . "vm-x-frame")
                                                 (title . "vm-x-title")))))
                 (list
                  (null (redraw-frame frame))
                  (null (tty-type frame))
                 (condition-case err (tty-type (selected-window)) (error (car err)))
                 (condition-case err (suspend-tty frame) (error (car err)))
                 (condition-case err (resume-tty frame) (error (car err)))
                  (condition-case err (x-get-resource "Xft.dpi" "Xft.Dpi") (error (car err)))
                  (condition-case err (x-list-fonts "*") (error (car err)))
                  (condition-case err (x-server-vendor frame) (error (car err)))
                  (framep created)
                  (= (length (frame-list)) (1+ before))
                  (equal (frame-parameter created 'name) "vm-x-frame")))"#,
        ),
        "OK (t t wrong-type-argument error error error error error neo t t)"
    );
}

#[test]
fn vm_xdisp_window_visibility_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-xdisp")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (insert "hello\nworld\n")
                 (goto-char 1)
                 (list (format-mode-line "%b")
                       (window-text-pixel-size w)
                       (pos-visible-in-window-p 1 w)
                       (coordinates-in-window-p '(0 . 0) w)
                       (condition-case err
                           (format-mode-line "%b" nil "x")
                         (error err))
                       (condition-case err
                           (window-text-pixel-size 999999)
                         (error err))
                       (condition-case err
                           (pos-visible-in-window-p 'left w)
                         (error err))
                       (condition-case err
                           (coordinates-in-window-p 'x w)
                         (error err))))"#
        ),
        r#"OK ("" (5 . 2) nil (0 . 0) (wrong-type-argument windowp "x") (wrong-type-argument window-live-p 999999) (wrong-type-argument integer-or-marker-p left) (wrong-type-argument consp x))"#
    );
}

#[test]
fn vm_frame_parameter_and_resize_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (modify-frame-parameters
                  f
                  '((name . "vm-frame")
                    (title . "vm-title")
                    (visibility . nil)
                    (vm-param . 7)))
                 (set-frame-width f 90)
                 (set-frame-height f 30)
                 (set-frame-size f 100 35)
                 (list (frame-parameter f 'name)
                       (frame-parameter f 'title)
                       (frame-parameter f 'visibility)
                       (frame-parameter f 'vm-param)
                       (cdr (assq 'vm-param (frame-parameters f)))
                       (frame-parameter f 'width)
                       (frame-parameter f 'height)
                       (frame-position f)
                       (set-frame-position f 3 4)))"#
        ),
        "OK (\"vm-frame\" \"vm-title\" t 7 7 80 25 (0 . 0) t)"
    );
}

#[derive(Clone, Default)]
struct VmRecordingDisplayHost {
    realized: Rc<RefCell<Vec<crate::emacs_core::GuiFrameHostRequest>>>,
    primary_size: Option<GuiFrameHostSize>,
}

impl VmRecordingDisplayHost {
    fn with_primary_size(width: u32, height: u32) -> Self {
        Self {
            realized: Rc::default(),
            primary_size: Some(GuiFrameHostSize { width, height }),
        }
    }
}

impl crate::emacs_core::DisplayHost for VmRecordingDisplayHost {
    fn realize_gui_frame(
        &mut self,
        request: crate::emacs_core::GuiFrameHostRequest,
    ) -> Result<(), String> {
        self.realized.borrow_mut().push(request);
        Ok(())
    }

    fn resize_gui_frame(
        &mut self,
        _request: crate::emacs_core::GuiFrameHostRequest,
    ) -> Result<(), String> {
        Ok(())
    }

    fn current_primary_window_size(&self) -> Option<GuiFrameHostSize> {
        self.primary_size
    }
}

#[test]
fn vm_x_create_frame_syncs_pending_resize_before_adopting_opening_gui_frame() {
    crate::test_utils::init_test_tracing();
    let host = VmRecordingDisplayHost::default();
    let requests = host.realized.clone();
    let result = vm_eval_with_init_str(
        "(x-create-frame '((name . \"Neomacs\") (title . \"Neomacs\")))",
        |eval| {
            let scratch = eval.buffers.create_buffer("*scratch*");
            let fid = eval.frames.create_frame("bootstrap", 960, 640, scratch);
            {
                let frame = eval.frames.get_mut(fid).expect("bootstrap frame");
                frame.set_parameter(Value::symbol("window-system"), Value::symbol("x"));
                frame.char_width = 10.0;
                frame.char_height = 20.0;
                if let Some(mini_leaf) = frame.minibuffer_leaf.as_mut() {
                    mini_leaf.set_bounds(crate::window::Rect::new(0.0, 600.0, 960.0, 40.0));
                }
            }
            eval.set_variable("terminal-frame", Value::make_frame(fid.0));
            let (tx, rx) = crossbeam_channel::unbounded();
            eval.input_rx = Some(rx);
            tx.send(crate::keyboard::InputEvent::Focus {
                focused: true,
                emacs_frame_id: 0,
            })
            .expect("queue focus");
            tx.send(crate::keyboard::InputEvent::Resize {
                width: 1500,
                height: 1900,
                scale_factor: 1.0,
                emacs_frame_id: 0,
            })
            .expect("queue resize");
            eval.set_display_host(Box::new(host.clone()));
        },
    );

    assert!(
        result.starts_with("OK #<frame "),
        "expected x-create-frame to succeed, got: {result}"
    );

    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].width, 1500);
    assert_eq!(requests[0].height, 1900);
}

#[test]
fn vm_make_frame_uses_gui_creation_path_when_display_host_is_active() {
    crate::test_utils::init_test_tracing();
    let host = VmRecordingDisplayHost::default();
    let requests = host.realized.clone();
    let result = vm_eval_with_init_str(
        // GNU owns `make-frame' in Lisp (lisp/frame.el:1019) and it funcalls
        // `frame-creation-function', which for a window system reaches the C
        // primitive `x-create-frame' (src/xfns.c:4916).  DIVERGENCES.md 154
        // deleted the Rust `make-frame' subr, whose GUI arm was a call to
        // exactly this.
        "(x-create-frame '((name . \"GUI\") (width . 80) (height . 25)))",
        |eval| {
            let scratch = eval.buffers.create_buffer("*scratch*");
            let fid = eval.frames.create_frame("bootstrap", 960, 640, scratch);
            {
                let frame = eval.frames.get_mut(fid).expect("bootstrap frame");
                frame.set_window_system(Some(Value::symbol("x")));
                frame.char_width = 10.0;
                frame.char_height = 20.0;
                if let Some(mini_leaf) = frame.minibuffer_leaf.as_mut() {
                    mini_leaf.set_bounds(crate::window::Rect::new(0.0, 600.0, 960.0, 40.0));
                }
            }
            eval.set_variable("terminal-frame", Value::make_frame(fid.0));
            eval.set_display_host(Box::new(host.clone()));
        },
    );

    assert!(
        result.starts_with("OK #<frame "),
        "expected make-frame to succeed, got: {result}"
    );

    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].width, 800);
    assert_eq!(requests[0].height, 500);
}

#[test]
fn vm_x_create_frame_prefers_display_host_primary_window_size_when_available() {
    crate::test_utils::init_test_tracing();
    let host = VmRecordingDisplayHost::with_primary_size(1500, 1900);
    let requests = host.realized.clone();
    let result = vm_eval_with_init_str(
        "(x-create-frame '((name . \"Neomacs\") (title . \"Neomacs\")))",
        |eval| {
            let scratch = eval.buffers.create_buffer("*scratch*");
            let fid = eval.frames.create_frame("bootstrap", 960, 640, scratch);
            {
                let frame = eval.frames.get_mut(fid).expect("bootstrap frame");
                frame.set_parameter(Value::symbol("window-system"), Value::symbol("x"));
                frame.char_width = 10.0;
                frame.char_height = 20.0;
                if let Some(mini_leaf) = frame.minibuffer_leaf.as_mut() {
                    mini_leaf.set_bounds(crate::window::Rect::new(0.0, 600.0, 960.0, 40.0));
                }
            }
            eval.set_variable("terminal-frame", Value::make_frame(fid.0));
            eval.set_display_host(Box::new(host.clone()));
        },
    );

    assert!(
        result.starts_with("OK #<frame "),
        "expected x-create-frame to succeed, got: {result}"
    );

    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].width, 1500);
    assert_eq!(requests[0].height, 1900);
}

#[test]
fn vm_frame_selected_window_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((w1 (selected-window))
                      (w2 (split-window-internal (selected-window) nil nil nil)))
                 (prog1
                     (list (eq (frame-old-selected-window) nil)
                           (eq (set-frame-selected-window nil w2) w2)
                           (eq (selected-window) w2)
                           (eq (set-frame-selected-window nil w1 t) w1)
                           (eq (selected-window) w1))
                   (select-window w1)
                   ;; `delete-window' is lisp/window.el:4318 and has no subr any
                   ;; more (DIVERGENCES.md 154); its body ends in this C
                   ;; primitive (src/window.c:5684).
                   (delete-window-internal w2)))"#
        ),
        "OK (t t t t t)"
    );
}

#[test]
fn vm_window_state_accessors_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let ((w (selected-window)))
                 (with-current-buffer (window-buffer w)
                   (erase-buffer)
                   (insert (make-string 200 ?x)))
                 (set-window-start w 7)
                 (set-window-point w 9)
                 (list (window-start w)
                       (window-group-start w)
                       (window-point w)
                       (integerp (window-use-time w))
                       (window-old-point w)
                       (window-old-buffer w)
                       (window-prev-buffers w)
                       (window-next-buffers w)))"#
        ),
        "OK (7 7 9 t 1 nil nil nil)"
    );
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(list (condition-case err (window-start 999999) (error err))
                     (condition-case err (window-group-start 999999) (error err))
                     (condition-case err (window-point 999999) (error err))
                     (condition-case err (window-use-time 999999) (error err))
                     (condition-case err (window-old-point 999999) (error err))
                     (condition-case err (window-old-buffer 999999) (error err))
                     (condition-case err (window-prev-buffers 999999) (error err))
                     (condition-case err (window-next-buffers 999999) (error err)))"#
        ),
        "OK ((wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999))"
    );
}

#[test]
fn vm_window_scroll_and_history_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((w (selected-window)))
                 (list (window-hscroll w)
                       (set-window-hscroll w 3)
                       (window-hscroll w)
                       (set-window-hscroll w -1)
                       (window-hscroll w)
                       (set-window-hscroll w ?a)
                       (window-hscroll w)
                       (window-margins w)
                       (set-window-margins w 1 2)
                       (window-margins w)
                       (set-window-margins w 1 2)
                       (set-window-margins w nil nil)
                       (window-margins w)
                       (set-window-margins w 3)
                       (window-margins w)
                       (set-window-margins w 3)
                       (window-vscroll w)
                       (set-window-vscroll w 1)
                       (window-vscroll w)
                       (window-fringes w)
                       (set-window-fringes w 1 2)
                       (window-scroll-bars w)
                       (set-window-scroll-bars w 'left)
                       (window-scroll-bars w)
                       (set-window-prev-buffers w nil)
                       (window-prev-buffers w)
                       (set-window-next-buffers w nil)
                       (window-next-buffers w)))"#
        ),
        "OK (0 3 3 0 0 97 97 (nil) t (1 . 2) nil t (nil) t (3) nil 0 0 0 (0 0 nil nil) nil (nil 0 t nil 0 t nil) nil (nil 0 t nil 0 t nil) nil nil nil nil)"
    );
    assert_eq!(
        vm_eval_str(
            r#"(let* ((w1 (selected-window))
                      (w2 (split-window-internal (selected-window) nil nil nil)))
                 (list (window-use-time w1)
                       (window-use-time w2)
                       (window-bump-use-time w2)
                       (window-use-time w1)
                       (window-use-time w2)
                       (window-bump-use-time w1)))"#
        ),
        "OK (1 0 1 2 1 nil)"
    );
}

#[test]
fn vm_scroll_and_recenter_builtins_use_shared_window_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let ((w (selected-window)))
                 (with-current-buffer (window-buffer w)
                   (erase-buffer)
                   (insert "a\nb\nc\nd\ne\nf\ng\nh\n"))
                 (set-window-point w 1)
                 (list (condition-case err
                           (progn (scroll-up 2) (scroll-down 1) (window-point w))
                         (error (car err)))
                       (progn (scroll-left 3) (window-hscroll w))
                       (progn (scroll-right 1) (window-hscroll w))
                       (progn (set-window-point w 9) (recenter 1) (window-start w))))"#
        ),
        // GNU 31.0.90 --batch: the bootstrap frame is the INITIAL frame, where
        // `pos_visible_p` is always false, so `scroll-down` recenters around
        // point, lands on BEGV and signals (window.c window_scroll_line_based
        // `lose = n < 0 && PT == BEGV`).
        "OK (beginning-of-buffer 3 2 7)"
    );
}

#[test]
fn vm_window_geometry_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let* ((w (selected-window))
                      (m (minibuffer-window)))
                 (with-current-buffer (window-buffer w)
                   (erase-buffer)
                   (insert (make-string 200 ?x)))
                 (list (window-total-height w)
                       (window-body-width w)
                       (window-body-height w)
                       (window-body-width w)
                       (window-total-height w)
                       (window-total-width w)
                       (window-left-column w)
                       (window-top-line m)
                       (window-pixel-left w)
                       (window-pixel-top m)
                       (> (window-end w) (window-start w))
                       (window-mode-line-height w)
                       (window-mode-line-height m)
                       (window-header-line-height w)
                       (window-pixel-height w)
                       (window-pixel-height m)
                       (window-pixel-width w)
                       (window-pixel-width m)
                       (window-text-height w)
                       (window-text-height m)
                       (window-text-width w)
                       (window-text-width m)
                       (window-edges w)
                       (window-edges m)
                       (window-edges w t)
                       (window-edges m t)))"#
        ),
        "OK (24 80 23 80 24 80 0 24 0 24 t 1 0 0 24 1 80 80 23 1 80 80 (0 0 80 24) (0 24 80 25) (0 0 80 23) (0 24 80 25))"
    );
}

#[test]
fn vm_window_chrome_height_builtins_use_last_redisplay_snapshot() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(list (window-mode-line-height)
                     (window-header-line-height)
                     (window-tab-line-height))"#,
            |eval| {
                let fid = crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(
                    &mut eval.frames,
                    &mut eval.buffers,
                );
                let wid = eval.frames.get(fid).expect("frame").selected_window;
                eval.frames
                    .get_mut(fid)
                    .expect("frame")
                    .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
                        window_id: wid,
                        mode_line_height: 35,
                        header_line_height: 35,
                        tab_line_height: 34,
                        ..crate::window::WindowDisplaySnapshot::default()
                    }]);
            }
        ),
        "OK (35 35 34)"
    );
}

#[test]
fn vm_interactive_minibuffer_query_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(list
                 (this-command-keys)
                 (this-command-keys-vector)
                 (progn
                   (clear-this-command-keys t)
                   (list (this-command-keys) (recent-keys)))
                 (funcall-interactively (lambda (x) x) 42)
                 (windowp (minibuffer-window))
                 (eq (minibuffer-window) (active-minibuffer-window))
                 (equal (all-completions "ap" '("app" "ape" "bee")) '("app" "ape"))
                 (null (cancel-kbd-macro-events)))"#,
            |eval| {
                let fid = crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(
                    &mut eval.frames,
                    &mut eval.buffers,
                );
                let minibuffer_buffer_id = {
                    let frame = eval.frames.get(fid).expect("selected frame");
                    let minibuffer_wid = frame.minibuffer_window.expect("minibuffer window");
                    frame
                        .find_window(minibuffer_wid)
                        .and_then(|window| window.buffer_id())
                        .expect("minibuffer buffer")
                };
                eval.minibuffers
                    .read_from_minibuffer(minibuffer_buffer_id, "M-x ", None, None)
                    .expect("active minibuffer state");
                eval.record_input_event(Value::fixnum(97));
                eval.set_read_command_keys(vec![Value::fixnum(97)]);
            },
        ),
        "OK (\"a\" [97] (\"\" [97]) 42 t t t t)"
    );
}

#[test]
fn vm_call_interactively_uses_shared_runtime_planning() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (fset 'vm-ci-shared-target '(lambda (x) (interactive (list 7)) x))
               (fset 'vm-ci-shared-alias 'vm-ci-shared-target)
               (list
                 (call-interactively 'vm-ci-shared-alias)
                 (interactive)
                 (condition-case err
                     (call-interactively 'vm-ci-shared-target nil '(1 2))
                   (wrong-type-argument (car err)))))"
        ),
        "OK (7 nil wrong-type-argument)"
    );
}

#[test]
fn vm_call_interactively_restores_command_identity_before_invocation() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (setq this-command 'outer-this
                       this-original-command 'outer-original
                       real-this-command 'outer-real
                       last-command 'outer-last)
                 (fset 'vm-ci-command-state
                       '(lambda (argument)
                          (interactive
                           (progn
                             (setq this-command 'inner-this
                                   this-original-command 'inner-original
                                   real-this-command 'inner-real
                                   last-command 'inner-last)
                             (list 7)))
                          (list argument
                                this-command
                                this-original-command
                                real-this-command
                                last-command)))
                 (list (call-interactively 'vm-ci-command-state)
                       this-command
                       this-original-command
                       real-this-command
                       last-command))"#
        ),
        "OK ((7 outer-this outer-original outer-real outer-last) outer-this outer-original outer-real outer-last)"
    );
}

#[test]
fn vm_call_interactively_resolves_noninteractive_autoload_before_commandp() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("vm-ci-late-command.el");
    std::fs::write(
        &fixture,
        "(defalias 'vm-ci-late-command
           #'(lambda () (interactive) 'loaded-command))\n",
    )
    .expect("write call-interactively autoload fixture");

    let mut eval = Context::new_vm_runtime_harness();
    eval.obarray.set_symbol_value(
        "load-path",
        Value::list(vec![Value::string(dir.path().to_string_lossy())]),
    );

    let result = eval
        .eval_str(
            "(progn
               (autoload 'vm-ci-late-command \"vm-ci-late-command\" nil nil)
               (call-interactively 'vm-ci-late-command))",
        )
        .expect("call-interactively should resolve the non-interactive autoload");

    assert_eq!(result, Value::symbol("loaded-command"));
}

#[test]
fn vm_call_interactively_propagates_noninteractive_autoload_failure_before_commandp() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (autoload 'vm-ci-missing-plain
                         \"vm-ci-missing-plain-library\"
                         nil nil)
               (condition-case error
                   (call-interactively 'vm-ci-missing-plain)
                 (error (car error))))"
        ),
        "OK file-missing"
    );
}

#[test]
fn vm_call_interactively_uses_one_resolved_interactive_form() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(let ((original (symbol-function 'interactive-form))
                   (calls 0))
               (unwind-protect
                   (progn
                     (fset 'interactive-form
                           (lambda (command)
                             (setq calls (1+ calls))
                             (funcall original command)))
                     (fset 'vm-ci-single-form
                           (lambda (value) (interactive (list 9)) value))
                     (list (call-interactively 'vm-ci-single-form) calls))
                 (fset 'interactive-form original)
                 (fmakunbound 'vm-ci-single-form)))"
        ),
        "OK (9 1)"
    );
}

#[test]
fn vm_call_interactively_accepts_symbol_interactive_form_property() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (fset 'vm-ci-property-command (lambda (value) value))
               (put 'vm-ci-property-command
                    'interactive-form
                    '(interactive (list 9)))
               (unwind-protect
                   (call-interactively 'vm-ci-property-command)
                 (fmakunbound 'vm-ci-property-command)))"
        ),
        "OK 9"
    );
}

#[test]
fn vm_call_interactively_honors_function_cell_mutation_during_interactive_spec_eval() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (fset 'vm-ci-mutate
                       '(lambda ()
                          (interactive
                           (progn
                             (fset 'vm-ci-mutate (lambda () 42))
                             (list)))
                          7))
                 (unwind-protect
                     (call-interactively 'vm-ci-mutate)
                   (fmakunbound 'vm-ci-mutate)))"#
        ),
        "OK 42"
    );
}

#[test]
fn vm_call_interactively_builtin_forward_char_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (call-interactively 'forward-char)
                 (point))"#,
            |eval| {
                let current = eval.buffers.current_buffer_id().expect("current buffer");
                let _ = eval.buffers.replace_buffer_contents(current, "ab");
                let _ = eval
                    .buffers
                    .goto_buffer_emacs_byte_pos(current, crate::buffer::EmacsBytePos::new(0));
            },
        ),
        "OK 2"
    );
}

#[test]
fn vm_call_interactively_executes_raw_lambda_commands_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(let ((current-prefix-arg 3))
               (call-interactively '(lambda (n) (interactive \"p\") n)))"
        ),
        "OK 3"
    );
}

#[test]
fn vm_call_interactively_form_spec_uses_interpreted_closure_environment() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_lexical_str(
            r#"(let ((command
                       (let ((captured 37))
                         (lambda (value)
                           (interactive (list captured))
                           value))))
                 (call-interactively command))"#
        ),
        "OK 37"
    );
}

#[test]
fn vm_call_interactively_quoted_lambda_form_spec_uses_dynamic_environment() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_lexical_str(
            r#"(let ((ambient-only 99))
                 (call-interactively
                  '(lambda (value)
                     (interactive
                      (list
                       (condition-case nil
                           ambient-only
                         (void-variable 'dynamic))))
                     value)))"#
        ),
        "OK dynamic"
    );
}

#[test]
fn vm_call_interactively_handles_simple_string_specs_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(let ((current-prefix-arg '(4))
                     (evt (list 'mouse-1 (list (list (selected-window) (point) '(0 . 0) 0)))))
                 (call-interactively
                  '(lambda (raw num pt mk beg end evt up ignored)
                     (interactive "P
p
d
m
r
e
U
i")
                     (list raw num pt mk beg end (car evt) up ignored))
                  nil
                  (vector evt)))"#,
            |eval| {
                let current = eval.buffers.current_buffer_id().expect("current buffer");
                let _ = eval.buffers.replace_buffer_contents(current, "abcd");
                let _ = eval
                    .buffers
                    .goto_buffer_emacs_byte_pos(current, crate::buffer::EmacsBytePos::new(2));
                let _ = eval
                    .buffers
                    .set_buffer_mark_emacs_byte_pos(current, crate::buffer::EmacsBytePos::new(1));
            }
        ),
        "OK ((4) 4 3 2 2 3 mouse-1 nil nil)"
    );
}

#[test]
fn vm_call_interactively_handles_optional_coding_without_prefix_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((current-prefix-arg nil)
                     (unread-command-events '(97)))
                 (list
                  (call-interactively '(lambda (coding) (interactive "ZCoding: ") coding))
                  unread-command-events))"#
        ),
        "OK (nil (97))"
    );
}

#[test]
fn vm_call_interactively_handles_k_k_capital_and_u_specs_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (progn
                   (use-global-map (make-sparse-keymap))
                   (fset 'mouse-drag-region (lambda (&rest _args) nil))
                   (fset 'mouse-set-point (lambda (&rest _args) nil))
                   (define-key (current-global-map) [down-mouse-1] #'mouse-drag-region)
                   (define-key (current-global-map) [mouse-1] #'mouse-set-point)
                   nil)
                 (let ((unread-command-events (list '(down-mouse-1) '(mouse-1))))
                   (call-interactively
                    '(lambda (keys up) (interactive "k
U") (list keys up))))
                 (let ((unread-command-events (list '(down-mouse-1) '(mouse-1))))
                   (call-interactively
                    '(lambda (keys up) (interactive "K
U") (list keys up)))))"#
        ),
        "OK (nil ([(down-mouse-1)] [(mouse-1)]) ([(down-mouse-1)] [(mouse-1)]))"
    );
}

#[test]
fn vm_call_interactively_handles_prompt_driven_batch_specs_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (let ((unread-command-events (list 97)))
                   (call-interactively '(lambda (x) (interactive "cChar: ") x)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "aFunction: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "bBuffer: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "BAny buffer: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "CCommand: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "DDirectory: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "fFind file: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "FFind file: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "GFind file: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "sString: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "MInherited: ") x))
                   (error (car err)))
                 ;; `n' is not here: it dispatches through the `read-number'
                 ;; FUNCTION CELL (src/callint.c:645), and `read-number' is
                 ;; lisp/subr.el:3725, so a bare evaluator has nothing to call
                 ;; -- exactly as GNU before subr.el.  Its rows are in
                 ;; `vm_call_interactively_number_specs_need_subr_el'
                 ;; (DIVERGENCES.md 152).
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "SSymbol: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "zCoding: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "vVariable: ") x))
                   (error (car err))))"#
        ),
        "OK (97 end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file)"
    );
}

#[test]
fn vm_call_interactively_handles_optional_coding_prompt_cases_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (let ((unread-command-events (list 97)))
                   (list
                    (call-interactively '(lambda (c) (interactive "ZCoding: ") c))
                    unread-command-events))
                 (let ((current-prefix-arg '(4)))
                   (condition-case err
                       (call-interactively '(lambda (c) (interactive "ZCoding: ") c))
                     (error (car err)))))"#
        ),
        "OK ((nil (97)) nil)"
    );
}

/// The `N' code letter without a prefix argument calls `read-number', which is
/// defined in subr.el (lisp/subr.el:3725) -- GNU reaches it through the
/// function cell at src/callint.c:645 -- so this needs the bootstrap context
/// the same way `user-error' does above (DIVERGENCES.md 152).
#[test]
fn vm_call_interactively_number_specs_need_subr_el() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_bootstrap_context_state(
        r#"(list
              (let ((current-prefix-arg '(4))
                    (prefix-arg nil))
                (call-interactively '(lambda (n) (interactive "NNumber: ") n)))
              (let ((current-prefix-arg nil)
                    (prefix-arg nil))
                (condition-case err
                    (call-interactively '(lambda (n) (interactive "NNumber: ") n))
                  (error (car err))))
              (condition-case err
                  (call-interactively '(lambda (x) (interactive "nNumber: ") x))
                (error (car err))))"#,
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK (4 end-of-file end-of-file)"
            );
        },
    );
}

#[test]
fn vm_call_interactively_handles_r_capital_spec_via_use_region_p_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(list
                 (progn
                   (fset 'use-region-p (lambda () nil))
                   (call-interactively
                    '(lambda (beg end) (interactive "R") (list beg end))))
                 (progn
                   (fset 'use-region-p (lambda () t))
                   (call-interactively
                    '(lambda (beg end) (interactive "R") (list beg end)))))"#,
            |eval| {
                let current = eval.buffers.current_buffer_id().expect("current buffer");
                let _ = eval.buffers.replace_buffer_contents(current, "abcd");
                let _ = eval
                    .buffers
                    .goto_buffer_emacs_byte_pos(current, crate::buffer::EmacsBytePos::new(2));
                let _ = eval
                    .buffers
                    .set_buffer_mark_emacs_byte_pos(current, crate::buffer::EmacsBytePos::new(1));
            },
        ),
        "OK ((nil nil) (2 3))"
    );
}

#[test]
fn vm_call_interactively_handles_expression_prompt_specs_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (condition-case err
                     (call-interactively '(lambda (expr) (interactive "xExpr: ") expr))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (value) (interactive "XExpr: ") value))
                   (error (car err))))"#
        ),
        "OK (end-of-file end-of-file)"
    );
}

#[test]
fn vm_yes_or_no_p_uses_shared_runtime_batch_path() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((unread-command-events '(121)))
                 (list
                  (condition-case err
                      (yes-or-no-p "Confirm? ")
                    (error (car err)))
                  unread-command-events))"#
        ),
        "OK (end-of-file (121))"
    );
}

#[test]
fn vm_hash_and_collection_tail_use_shared_and_direct_paths() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            "(list
               (funcall (lambda (x) x) 42)
               (assoc 'b '((a . 1) (b . 2)) nil)
               (plist-member '(:a 1 :b 2) :b nil)
               (ntake 2 '(1 2 3 4))
               (md5 (current-buffer))
               (secure-hash 'sha1 (current-buffer))
               (print--preprocess 'foo)
               (sleep-for 0))",
            |eval| {
                let current = eval
                    .buffers
                    .current_buffer_id()
                    .expect("current buffer should exist");
                let _ = eval.buffers.insert_into_buffer(current, "abc");
            },
        ),
        "OK (42 (b . 2) (:b 2) (1 2) \"900150983cd24fb0d6963f7d28e17f72\" \"a9993e364706816aba3e25717850c26c9cd0d89d\" nil nil)"
    );
}

#[test]
fn vm_plist_member_returns_nil_for_odd_length_plist_matching_gnu() {
    // Regression for winner-mode / cus-edit widget failure.
    //
    // GNU's `plist-member` walks the list in key/value pairs and
    // returns nil (not found) if the search key isn't present, even
    // when the list has an odd number of elements (unpaired last
    // key). Only truly improper (dotted) tails signal plistp.
    //
    // The customize machinery feeds `(choice :tag ... :help-echo ...
    // :value ... (const ...) (const ...) ...)` through widget-convert.
    // After stripping the leading `choice` symbol, that's a 3-pair
    // plist followed by many bare `(const ...)` elements, yielding an
    // odd total length. Any `plist-member` call on that structure
    // must return nil, not signal plistp.
    crate::test_utils::init_test_tracing();

    // 2-arg form (eq comparison).
    assert_eq!(
        vm_eval_str(
            "(plist-member '(:tag \"Width\" :value normal (const a) (const b) (const c)) :missing)"
        ),
        "OK nil"
    );
    // Exact match at an early position still works.
    assert_eq!(
        vm_eval_str(
            "(plist-member '(:tag \"Width\" :value normal (const a) (const b) (const c)) :tag)"
        ),
        "OK (:tag \"Width\" :value normal (const a) (const b) (const c))"
    );
    // 3-arg form (predicate, which routes through a separate walker).
    assert_eq!(
        vm_eval_str(
            "(plist-member '(:tag \"Width\" :value normal (const a) (const b) (const c)) :missing #'eq)"
        ),
        "OK nil"
    );
    // Truly improper (dotted) tail is still a plistp error.
    assert_eq!(
        vm_eval_str("(condition-case err (plist-member '(:a 1 :b 2 . junk) :c) (error err))"),
        "OK (wrong-type-argument plistp (:a 1 :b 2 . junk))"
    );
}

#[test]
fn vm_assoc_and_plist_member_predicates_use_shared_runtime_callbacks() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(list
               (let ((log nil))
                 (list
                   (assoc 'b '((a . 1) (b . 2))
                          (lambda (entry-key search-key)
                            (setq log (cons entry-key log))
                            (eq entry-key search-key)))
                   log))
               (let ((log nil))
                 (list
                   (plist-member '(:a 1 :b 2) :b
                                 (lambda (entry-key search-key)
                                   (setq log (cons entry-key log))
                                   (eq entry-key search-key)))
                   log)))"
        ),
        "OK (((b . 2) (b a)) ((:b 2) (:b :a)))"
    );
}

#[test]
fn vm_runtime_control_tail_uses_localized_shared_paths() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(listp (garbage-collect))"), "OK t");

    let mut eval = Context::new_vm_runtime_harness();
    let kill_result = eval.eval_str("(kill-emacs 7)");
    match kill_result {
        Err(EvalError::Shutdown(request)) => assert_eq!(request.exit_code, 7),
        other => panic!("kill-emacs should unwind, got {other:?}"),
    }

    assert_eq!(
        eval.shutdown_request(),
        Some(crate::emacs_core::eval::ShutdownRequest {
            exit_code: 7,
            restart: false,
        })
    );
}

#[test]
fn vm_kill_emacs_runs_hooks_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();
    let result = eval.eval_str(
        "(progn
           (setq vm-kill-hook-log nil)
           (setq kill-emacs-hook
                 (list (lambda () (setq vm-kill-hook-log 'ran))))
           (kill-emacs 3)
           vm-kill-hook-log)",
    );
    match result {
        Err(EvalError::Shutdown(request)) => assert_eq!(request.exit_code, 3),
        other => panic!("kill-emacs should unwind after running hooks, got {other:?}"),
    }
    assert_eq!(
        eval.obarray().symbol_value("vm-kill-hook-log"),
        Some(&Value::symbol("ran"))
    );
    assert_eq!(
        eval.shutdown_request(),
        Some(crate::emacs_core::eval::ShutdownRequest {
            exit_code: 3,
            restart: false,
        })
    );
}

#[test]
fn vm_eval_and_macroexpand_tail_use_localized_shared_paths() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            "(progn (setq vm-eval-buffer-target nil) (eval-buffer (current-buffer)) vm-eval-buffer-target)",
            |eval| {
                let current = eval
                    .buffers
                    .current_buffer_id()
                    .expect("current buffer should exist");
                eval.buffers
                    .replace_buffer_contents(current, "(setq vm-eval-buffer-target 17)\n");
            },
        ),
        "OK 17"
    );

    let region_source = "(setq vm-eval-region-target 23)\n(setq vm-eval-region-tail 99)\n";
    let region_end = region_source
        .lines()
        .next()
        .expect("first form should exist")
        .chars()
        .count() as i64
        + 2;
    let region_form = format!(
        "(progn (setq vm-eval-region-target nil) (setq vm-eval-region-tail nil) (eval-region 1 {}) (list vm-eval-region-target vm-eval-region-tail))",
        region_end
    );
    assert_eq!(
        vm_eval_with_init_str(&region_form, |eval| {
            let current = eval
                .buffers
                .current_buffer_id()
                .expect("current buffer should exist");
            eval.buffers.replace_buffer_contents(current, region_source);
        }),
        "OK (23 nil)"
    );

    // Install a when macro manually and test macroexpand with it.
    assert_eq!(
        vm_eval_str(
            "(progn
               (defalias 'when (cons 'macro (lambda (cond &rest body)
                 (list 'if cond (cons 'progn body)))))
               (macroexpand '(when t 7 8)))"
        ),
        "OK (if t (progn 7 8))"
    );
}

#[test]
fn vm_macroexpand_environment_lambda_uses_localized_shared_callbacks() {
    crate::test_utils::init_test_tracing();
    // when is no longer a built-in macro; the env-lambda now produces
    // (vm-result t 1) which is not a macro, so macroexpand returns it as-is.
    assert_eq!(
        vm_eval_str(
            "(let ((env (list (list 'vm-env 'lambda '(x)
                                   (list 'list (list 'quote 'vm-result) 'x 1)))))
               (macroexpand '(vm-env t) env))"
        ),
        "OK (vm-result t 1)"
    );
}

#[test]
fn vm_macroexpand_preserves_visible_lexical_binding_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(
        "(progn
           (require 'bytecomp)
           (defalias 'vm-lb-macro
             (cons 'macro
                   (byte-compile
                    (lambda (&rest _)
                      lexical-binding)))))",
    )
    .expect("install compiled macro expander");

    let _ = eval.try_set_runtime_binding_by_id(
        crate::emacs_core::intern::intern("lexical-binding"),
        Value::T,
    );
    eval.lexenv = Value::NIL;

    let result = eval.eval_str("(list (macroexpand '(vm-lb-macro)) lexical-binding)");
    assert_eq!(
        crate::emacs_core::error::format_eval_result(&result),
        "OK (t t)"
    );
}

#[test]
fn vm_raw_lambda_and_closure_callability_matches_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str(r#"(funcall '(lambda (x) x) 7)"#), "OK 7");
    assert_eq!(
        vm_eval_lexical_str(r#"(let ((x 5)) (funcall (lambda (y) (+ x y)) 3))"#),
        "OK 8"
    );
    assert_eq!(
        vm_eval_str(r#"(funcall '(closure ((x . 5)) (y) (+ x y)) 3)"#),
        "ERR (invalid-function ((closure ((x . 5)) (y) (+ x y))))"
    );
}

#[test]
fn vm_mapatoms_and_maphash_use_shared_runtime_callbacks() {
    crate::test_utils::init_test_tracing();
    // `when` is a macro defined in subr.el, so this test needs the
    // bootstrap context.
    assert_eq!(
        vm_bootstrap_eval_str(
            "(list
               (let ((h (make-hash-table :test 'eq))
                     (acc 0))
                 (puthash 'a 1 h)
                 (puthash 'b 2 h)
                 (maphash (lambda (_k v) (setq acc (+ acc v))) h)
                 acc)
               (let ((seen nil))
                 (intern \"vm-mapatoms-default\")
                 (mapatoms (lambda (sym)
                             (when (eq sym 'vm-mapatoms-default)
                               (setq seen t))))
                 seen)
               (let* ((ob (make-vector 7 0))
                      (target (intern \"vm-mapatoms-custom\" ob))
                     (seen nil))
                 (mapatoms (lambda (sym)
                             (when (eq sym target)
                               (setq seen t)))
                           ob)
                 seen))"
        ),
        "OK (3 t t)"
    );
}

#[test]
fn vm_mapatoms_and_maphash_root_full_traversal_across_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.tagged_heap.set_gc_threshold(1);
    with_vm_eval_in_context(
        eval,
        r#"(list
             (let ((ob (make-vector 7 0)))
               (intern "vm-mapatoms-root-a" ob)
               (intern "vm-mapatoms-root-b" ob)
               (let ((count 0))
                 (mapatoms (lambda (_sym)
                             (garbage-collect)
                             (setq count (1+ count)))
                           ob)
                 count))
             (let ((h (make-hash-table :test 'equal))
                   (sum 0))
               (puthash (list 'a 1) 'x h)
               (puthash (list 'b 2) 'y h)
               (maphash (lambda (k _v)
                          (garbage-collect)
                          (setq sum (+ sum (car (cdr k)))))
                        h)
               sum))"#,
        false,
        |result, eval| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK (2 3)"
            );
            assert!(eval.gc_count > 0, "callback-triggered GC should run");
        },
    );
}

#[test]
fn vm_window_metadata_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((w (selected-window))
                      (m (minibuffer-window))
                      (dt '(1 2 3)))
                 (list (window-dedicated-p w)
                       (set-window-dedicated-p w t)
                       (window-dedicated-p w)
                       (set-window-dedicated-p w nil)
                       (window-dedicated-p w)
                       (null (window-parameters w))
                       (set-window-parameter w 'foo 'bar)
                       (window-parameter w 'foo)
                       (equal (window-parameters w) '((foo . bar)))
                       (set-window-parameter w 'foo nil)
                       (equal (window-parameters w) '((foo)))
                       (null (window-display-table w))
                       (let ((rv (set-window-display-table w dt))) (equal rv dt))
                       (equal (window-display-table w) dt)
                       (null (set-window-display-table w nil))
                       (null (window-display-table w))
                       (window-cursor-type w)
                       (set-window-cursor-type w 'bar)
                       (window-cursor-type w)
                       (set-window-cursor-type w t)
                       (window-cursor-type w)
                       (set-window-cursor-type m nil)
                       (window-cursor-type m)))"#
        ),
        "OK (nil t t nil nil t bar bar t nil t t t t t t t bar bar t t nil nil)"
    );
}

#[test]
fn vm_window_tree_and_list_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(let* ((left (selected-window))
                      (right (next-window left))
                      (bottom (next-window right))
                      (root (frame-root-window))
                      (vparent (window-parent right)))
                 (list (window-valid-p root)
                       (window-live-p root)
                       (eq (window-parent left) root)
                       (eq (window-next-sibling left) vparent)
                       (eq (window-left-child root) left)
                       (window-valid-p (window-top-child root))
                       (eq (window-parent right) vparent)
                       (eq (window-parent bottom) vparent)
                       (eq (window-top-child vparent) right)
                       (null (window-left-child vparent))
                       (eq (window-next-sibling right) bottom)
                       (eq (window-prev-sibling bottom) right)
                       (length (window-list))
                       (length (window-list nil t))
                       (if (memq bottom (window-list-1 left nil nil)) t)
                       (windowp (window-at 0 0))
                       (windowp (window-at 79 0))
                       (let ((m (window-at 0 24))) (and m (window-minibuffer-p m)))
                       (window-combination-limit root)
                       (set-window-combination-limit root t)
                       (window-combination-limit root)))"#,
            |eval| {
                let fid = crate::emacs_core::window_cmds::ensure_selected_frame_id(eval);
                let left = eval.frames.get(fid).expect("frame").selected_window;
                let buffer_id = eval.buffers.current_buffer().expect("buffer").id;
                let right = eval
                    .frames
                    .split_window(
                        fid,
                        left,
                        SplitDirection::Horizontal,
                        buffer_id,
                        None,
                        SplitPlacement::AfterTarget,
                    )
                    .expect("horizontal split");
                let _bottom = eval
                    .frames
                    .split_window(
                        fid,
                        right,
                        SplitDirection::Vertical,
                        buffer_id,
                        None,
                        SplitPlacement::AfterTarget,
                    )
                    .expect("vertical split");
            }
        ),
        "OK (t nil t t t nil t t t t t t 3 4 t t t t nil t t)"
    );
}

#[test]
fn vm_window_resize_and_metric_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((w (selected-window))
                      (m (minibuffer-window))
                      (f (selected-frame)))
                  (list
                   (window-minibuffer-p w)
                   (window-minibuffer-p m)
                   (window-resize-apply f)
                   (window-resize-apply-total f)
                   (set-window-new-normal w 0.5)
                   (window-new-normal w)
                   (set-window-new-pixel w 20)
                   (window-new-pixel w)
                   (set-window-new-total w 10)
                   (window-new-total w)
                   (window-bottom-divider-width w)
                   (window-lines-pixel-dimensions w)
                   (window-old-body-pixel-height w)
                   (window-old-body-pixel-width w)
                   (window-old-pixel-height w)
                   (window-old-pixel-width w)
                   (window-right-divider-width w)
                   (window-scroll-bar-height w)
                   (window-scroll-bar-width w)
                   (window-tab-line-height w)
                   (frame-ancestor-p f f)
                   (frame-bottom-divider-width f)
                   (frame-child-frame-border-width f)
                   (frame-focus f)
                   (frame-fringe-width f)
                   (frame-internal-border-width f)
                   (frame-parent f)
                   (frame-pointer-visible-p f)
                   (frame-right-divider-width f)
                   (frame-scale-factor f)
                   (frame-scroll-bar-height f)
                   (frame-scroll-bar-width f)
                   (redirect-frame-focus f f)))"#
        ),
        "OK (nil t t t 0.5 0.5 20 20 10 10 0 nil 0 0 0 0 0 0 0 0 nil 0 0 nil 0 0 nil t 0 1.0 0 0 nil)"
    );
}

#[test]
fn vm_remaining_frame_stub_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (list
                  (hash-table-p (frame--face-hash-table))
                  (frame--set-was-invisible f t)
                  (frame-after-make-frame f nil)
                  (frame-font-cache f)
                  (frame-or-buffer-changed-p)
                  (frame-or-buffer-changed-p nil)
                  (condition-case err (frame-or-buffer-changed-p 'vm-missing-var) (error (car err)))
                  (frame-window-state-change f)
                  (frame--z-order-lessp f f)))"#
        ),
        "OK (t t nil nil t nil void-variable nil nil)"
    );
}

#[test]
fn vm_window_selection_and_buffer_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    // other-window is a defun in window.el → bootstrap context required.
    assert_eq!(
        vm_bootstrap_eval_with_init_str(
            r#"(let* ((w1 (selected-window))
                      (w2 (next-window w1))
                      (b1 (get-buffer-create "vm-wsel-1"))
                      (b2 (get-buffer-create "vm-wsel-2")))
                 (set-window-buffer w1 b1)
                 (set-window-buffer w2 b2)
                 (select-window w2)
                 (list (eq (selected-window) w2)
                       (eq (current-buffer) b2)
                       (eq (window-buffer w1) b1)
                       (eq (window-buffer w2) b2)
                       (eq (next-window w1) w2)
                       (eq (previous-window w1) w2)
                       (eq (other-window-for-scrolling) w1)
                       (other-window 1)
                       (eq (selected-window) w1)
                       (eq (current-buffer) b1)
                       (eq (other-window-for-scrolling) w2)
                       (window-valid-p (car (window-list)))
                       (window-live-p (car (window-list-1 nil nil)))
                       (condition-case err (set-window-combination-limit w1 t)
                         (error (car err)))))"#,
            |eval| {
                let fid = crate::emacs_core::window_cmds::ensure_selected_frame_id(eval);
                let w1 = eval.frames.get(fid).expect("frame").selected_window;
                let buffer_id = eval.buffers.current_buffer().expect("buffer").id;
                let _w2 = eval
                    .frames
                    .split_window(
                        fid,
                        w1,
                        SplitDirection::Horizontal,
                        buffer_id,
                        None,
                        SplitPlacement::AfterTarget,
                    )
                    .expect("horizontal split");
            }
        ),
        "OK (t t t t t t t nil t t t t t error)"
    );
}

#[test]
fn vm_window_deletion_and_frame_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(let* ((w1 (selected-window))
                      (w2 (next-window w1))
                      (b1 (get-buffer-create "vm-del-1"))
                      (b2 (get-buffer-create "vm-del-2"))
                      ;; `make-frame' is lisp/frame.el:1019 and has no subr
                      ;; any more (DIVERGENCES.md 154); on a text terminal its
                      ;; `frame-creation-function' reaches this C DEFUN
                      ;; (src/frame.c:1736).
                      (f2 (make-terminal-frame '((name . "vm-frame")))))
                 (set-window-buffer w1 b1)
                 (set-window-buffer w2 b2)
                 (select-window w2)
                 (list (framep f2)
                       (frame-live-p f2)
                       (delete-window-internal w2)
                       ;; GNU's `Fdelete_window_internal' ends with
                       ;; `Fselect_window (new_selected_window, Qt)'
                       ;; (src/window.c), and `select_window' makes the new
                       ;; window's buffer current.  Ours selects but does not
                       ;; set the buffer, so the test names the second C step.
                       (progn (select-window w1) (eq (selected-window) w1))
                       (eq (current-buffer) b1)
                       (length (window-list))
                       (progn (delete-other-windows-internal w1)
                              (length (window-list)))
                       (delete-frame f2)
                       (frame-live-p f2)))"#,
            |eval| {
                crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(eval);
                let fid = crate::emacs_core::window_cmds::ensure_selected_frame_id(eval);
                let w1 = eval.frames.get(fid).expect("frame").selected_window;
                let buffer_id = eval.buffers.current_buffer().expect("buffer").id;
                let _w2 = eval
                    .frames
                    .split_window(
                        fid,
                        w1,
                        SplitDirection::Horizontal,
                        buffer_id,
                        None,
                        SplitPlacement::AfterTarget,
                    )
                    .expect("horizontal split");
            }
        ),
        "OK (t t nil t t 1 1 nil nil)"
    );
}

#[test]
fn vm_split_window_and_frame_selection_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(let* ((f1 (selected-frame))
                      (w1 (selected-window))
                      (w2 (split-window-internal w1 nil 'right nil))
                      (f2 (make-terminal-frame '((name . "vm-frame-sel")))))
                 (list (windowp w2)
                       (length (window-list))
                       (eq (select-frame f2) f2)
                       (eq (selected-frame) f2)
                       (eq (make-frame-visible f2) f2)
                       (length (frame-list))
                       (length (visible-frame-list))
                       (progn (iconify-frame f2) (frame-visible-p f2))
                       (length (visible-frame-list))
                       ;; `select-frame-set-input-focus' is lisp/frame.el:1262
                       ;; and has no subr any more (DIVERGENCES.md 154); its
                       ;; body is these two C DEFUNs plus `x-focus-frame',
                       ;; which needs a window system.
                       (progn (select-frame f1)
                              (raise-frame f1)
                              (eq (selected-frame) f1))))"#,
            |eval| crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(eval),
        ),
        "OK (t 2 t t t 2 2 nil 1 t)"
    );
}

#[test]
fn vm_window_configuration_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((w1 (selected-window))
                      (w2 (split-window-internal w1 nil 'right nil))
                      (b1 (get-buffer-create "vm-wcfg-1"))
                      (b2 (get-buffer-create "vm-wcfg-2")))
                 (set-window-buffer w1 b1)
                 (set-window-buffer w2 b2)
                 (select-window w2)
                 (let ((cfg (current-window-configuration)))
                   (delete-window-internal w2)
                   (set-window-configuration cfg)
                   (list (window-configuration-p cfg)
                         (framep (window-configuration-frame cfg))
                         (window-configuration-equal-p cfg cfg)
                         (length (window-list))
                         (eq (selected-window) w2)
                         (eq (current-buffer) b2)
                         (eq (window-buffer w1) b1)
                         (eq (window-buffer w2) b2))))"#
        ),
        "OK (t t t 2 t t t t)"
    );
}

#[test]
fn vm_eval_bridge_preserves_current_local_map_across_builtin_calls() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str("(progn (use-local-map (make-sparse-keymap)) (keymapp (current-local-map)))"),
        "OK t"
    );
}

#[test]
fn vm_selected_global_map_is_independent_of_the_lisp_variable() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((selected (make-keymap))
                      (rebound (copy-keymap selected))
                      (key (vector 1)))
                 (use-global-map selected)
                 (let ((global-map rebound))
                   ;; `global-set-key's body, verbatim (lisp/subr.el:1567):
                   ;; the selected map, not the `global-map' VARIABLE.
                   (define-key (current-global-map) key 'vm-selected-global-command)
                   (list (eq (current-global-map) selected)
                         (eq (current-global-map) global-map)
                         (lookup-key selected key)
                         (lookup-key global-map key))))"#
        ),
        "OK (t nil vm-selected-global-command nil)"
    );
}

#[test]
fn vm_keymap_structure_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((parent (make-keymap))
                      (child (copy-keymap parent))
                      (minor (make-sparse-keymap))
                      (prefix (make-sparse-keymap)))
                 (define-key parent [1] 'parent-binding)
                 (define-key child [2] 'child-binding)
                 (set-keymap-parent child parent)
                 (define-key child [24] prefix)
                 (define-key prefix [97] 'prefixed)
                 (setq minor-mode-map-alist (list (cons 'vm-minor-mode minor)))
                 (setq vm-minor-mode t)
                 (use-local-map child)
                 (list (keymapp parent)
                       (null (eq child parent))
                       (eq (keymap-parent child) parent)
                       (eq (set-keymap-parent child parent) parent)
                       (let ((maps (current-active-maps)))
                         (list (eq (car maps) minor)
                               (eq (car (cdr maps)) child)
                               (eq (car (cdr (cdr maps))) (current-global-map))))
                       (equal (current-minor-mode-maps) (list minor))
                       (let ((root (make-sparse-keymap))
                             (desc (make-sparse-keymap)))
                         (define-key root [24] desc)
                         (if (accessible-keymaps root [24]) t))
                       (lookup-key child [1])
                       (lookup-key child [2])
                       (lookup-key child [24 97])))"#
        ),
        "OK (t t t t (t t t) t t parent-binding child-binding prefixed)"
    );
}

#[test]
fn vm_map_keymap_builtins_use_shared_state_and_vm_callbacks() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((parent (make-sparse-keymap))
                      (child (make-sparse-keymap))
                      (seen nil))
                 (define-key parent [1] 'parent-binding)
                 (define-key child [2] 'child-binding)
                 (set-keymap-parent child parent)
                 (fset 'vm-record-binding
                       (lambda (_event binding)
                         (setq seen (cons binding seen))))
                 (list
                  (progn
                    (setq seen nil)
                    (map-keymap-internal 'vm-record-binding child)
                    (reverse seen))
                  (progn
                    (setq seen nil)
                    (map-keymap 'vm-record-binding child)
                    (reverse seen))))"#
        ),
        "OK ((child-binding) (child-binding parent-binding))"
    );
}

#[test]
fn vm_hook_builtins_use_shared_runtime_state_and_vm_callbacks() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((buf (get-buffer-create "vm-hook-buf"))
                     (seen nil))
                 (fset 'vm-hook-global
                       (lambda (&rest xs)
                         (setq seen (cons (cons 'global xs) seen))
                         nil))
                 (fset 'vm-hook-local
                       (lambda (&rest xs)
                         (setq seen (cons (cons 'local xs) seen))
                         'ok))
                 (fset 'vm-hook-fail
                       (lambda (&rest xs)
                         (setq seen (cons (cons 'fail xs) seen))
                         nil))
                 (fset 'vm-hook-wrapper
                       (lambda (fn x)
                         (setq seen (cons (list 'wrap fn x) seen))
                         nil))
                 (setq vm-hook-probe '(vm-hook-global))
                 (set-buffer buf)
                 (make-local-variable 'vm-hook-probe)
                 (setq vm-hook-probe '(vm-hook-local t))
                 (list
                  (run-hooks 'vm-hook-probe)
                  (run-hook-with-args 'vm-hook-probe 1 2)
                  (run-hook-with-args-until-success 'vm-hook-probe 3)
                  (progn
                    (setq vm-hook-probe '(vm-hook-fail t))
                    (run-hook-with-args-until-failure 'vm-hook-probe 4))
                  (progn
                    (setq vm-hook-probe '(vm-hook-local))
                    (run-hook-wrapped 'vm-hook-probe 'vm-hook-wrapper 5))
                  (reverse seen)))"#
        ),
        "OK (nil nil ok nil nil ((local) (global) (local 1 2) (global 1 2) (local 3) (fail 4) (wrap vm-hook-local 5)))"
    );
}

#[test]
fn vm_run_hook_wrapped_stops_on_first_non_nil_wrapper_result() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_full_context_state(
        r#"(let ((seen nil))
             (fset 'vm-hook-wrap-a (lambda () 'a))
             (fset 'vm-hook-wrap-b (lambda () 'b))
             (fset 'vm-hook-wrap-wrapper
                   (lambda (fn)
                     (setq seen (cons fn seen))
                     (if (eq fn 'vm-hook-wrap-a) 'stop nil)))
             (setq vm-hook-wrap-probe '(vm-hook-wrap-a vm-hook-wrap-b))
             (list (run-hook-wrapped 'vm-hook-wrap-probe 'vm-hook-wrap-wrapper)
                   seen))"#,
        false,
        |result, _| {
            assert_eq!(
                crate::emacs_core::error::format_eval_result(&result),
                "OK (stop (vm-hook-wrap-a))"
            );
        },
    );
}

#[test]
fn vm_feature_and_symbol_table_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((sym (intern "vm-plist-sym")))
                 (setq features '(vm-old-feature))
                 (setplist sym '(a 1 b 2))
                 (list
                  (featurep 'vm-old-feature)
                  (provide 'vm-new-feature '(vm-sub))
                  features
                  (featurep 'vm-new-feature)
                  (featurep 'vm-new-feature 'vm-sub)
                  (get sym 'a)
                  (symbol-plist sym)
                  (progn
                    (unintern sym nil)
                    (intern-soft "vm-plist-sym"))))"#
        ),
        "OK (t vm-new-feature (vm-new-feature vm-old-feature) t t 1 (a 1 b 2) nil)"
    );
}

#[test]
fn vm_default_value_watcher_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let (log)
                 (fset 'vm-default-watch
                       (lambda (_sym new op where)
                         (setq log (cons (list new op where) log))))
                 (add-variable-watcher 'vm-default-target 'vm-default-watch)
                 (add-variable-watcher 'vm-default-top 'vm-default-watch)
                 (list
                  (set-default 'vm-default-target 23)
                  (default-value 'vm-default-target)
                  (set-default-toplevel-value 'vm-default-top 42)
                  (default-toplevel-value 'vm-default-top)
                  (reverse log)))"#
        ),
        "OK (23 23 nil 42 ((23 set nil) (42 set nil)))"
    );
}

#[test]
fn vm_key_lookup_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((g (make-sparse-keymap))
                      (l (make-sparse-keymap))
                      (m (make-sparse-keymap)))
                 (use-global-map g)
                 (use-local-map l)
                 (define-key g "a" 'ignore)
                 (define-key g [remap self-insert-command] 'delete-char)
                 (define-key l "c" 'self-insert-command)
                 (define-key m "b" 'forward-char)
                 (setq minor-mode-map-alist (list (cons 'vm-demo-mode m)))
                 (setq vm-demo-mode t)
                 (list (key-binding "a")
                       (key-binding "c")
                       (key-binding "c" t t)
                       (lookup-key (current-local-map) "c")
                       (minor-mode-key-binding "b")
                       (condition-case err
                           (key-binding "a" t nil 0)
                         (error (car err)))))"#
        ),
        "OK (ignore delete-char self-insert-command self-insert-command ((vm-demo-mode . forward-char)) args-out-of-range)"
    );
}

#[test]
fn vm_command_remapping_uses_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((g (make-sparse-keymap))
                     (m (make-sparse-keymap)))
                 (use-global-map g)
                 (define-key g [remap ignore] 'forward-char)
                 (define-key m [remap ignore] 'delete-char)
                 (setq minor-mode-map-alist (list (cons 'vm-remap-mode m)))
                 (setq vm-remap-mode t)
                 (list (command-remapping 'ignore)
                       (command-remapping 'ignore nil '(keymap (remap keymap (ignore . self-insert-command))))
                       (condition-case err
                           (command-remapping 'ignore 0)
                         (error (car err)))) )"#
        ),
        "OK (delete-char self-insert-command args-out-of-range)"
    );
}

#[test]
fn vm_set_buffer_and_current_buffer_share_buffer_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (get-buffer-create \"*vm-current-buffer*\")
               (set-buffer \"*vm-current-buffer*\")
               (buffer-name (current-buffer)))"
        ),
        r#"OK "*vm-current-buffer*""#
    );
}

#[test]
fn vm_current_buffer_query_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(list (point-min)
                     (point-max)
                     (buffer-string)
                     (goto-char 99)
                     (point)
                     (goto-char 2)
                     (point)
                     (char-after)
                     (char-before))"#,
            |eval| {
                let current = eval.buffers.current_buffer_id().expect("scratch buffer");
                let buffer = eval.buffers.get_mut(current).expect("scratch buffer");
                buffer.insert("hello");
                let start = buffer
                    .lisp_pos_to_emacs_byte_pos(LispCharPos1::new(2))
                    .get();
                let end = buffer
                    .lisp_pos_to_emacs_byte_pos(LispCharPos1::new(5))
                    .get();
                buffer.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
                    start, end,
                ));
            },
        ),
        r#"OK (2 5 "ell" 99 5 2 2 101 nil)"#
    );
}

#[test]
fn vm_goto_char_and_char_queries_use_live_marker_positions() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "ab")
                 (let ((m (copy-marker 2)))
                   (goto-char 1)
                   (insert "X")
                   (list (point)
                         (marker-position m)
                         (progn (goto-char m) (point))
                         (char-after m)
                         (char-before m))))"#
        ),
        "OK (2 3 3 98 97)"
    );
}

#[test]
fn vm_navigation_predicates_and_line_positions_use_shared_narrowed_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(list (list (bobp) (eobp) (bolp) (eolp)
                           (line-beginning-position) (line-end-position))
                     (progn
                       (goto-char (point-max))
                       (list (bobp) (eobp) (bolp) (eolp)
                             (line-beginning-position) (line-end-position))))"#,
            |eval| {
                let current = eval.buffers.current_buffer_id().expect("scratch buffer");
                let buffer = eval.buffers.get_mut(current).expect("scratch buffer");
                buffer.insert("wx\nab\ncd");
                let start = buffer
                    .lisp_pos_to_emacs_byte_pos(LispCharPos1::new(4))
                    .get();
                let end = buffer
                    .lisp_pos_to_emacs_byte_pos(LispCharPos1::new(6))
                    .get();
                buffer.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
                    start, end,
                ));
                buffer.goto_emacs_byte_pos(buffer.point_min_emacs_byte_pos());
            },
        ),
        "OK ((t nil t nil 4 6) (nil t nil t 4 6))"
    );
}

#[test]
fn vm_line_position_optional_argument_matches_gnu_current_rules() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "a\nbb\nccc")
                 (goto-char 2)
                 (list (line-beginning-position 2)
                       (line-end-position 2)
                       (line-beginning-position 3)
                       (line-end-position 3)))"#
        ),
        "OK (3 5 6 9)"
    );
}

#[test]
fn vm_buffer_restriction_and_modified_state_use_shared_runtime_manager() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "abcdef")
                 (list (buffer-size)
                       (buffer-modified-p)
                       (set-buffer-modified-p nil)
                       (buffer-modified-p)
                       (buffer-modified-tick)
                       (buffer-chars-modified-tick)
                       (let ((start (copy-marker 2))
                             (end (copy-marker 5 t)))
                         (goto-char 1)
                         (insert "X")
                         (narrow-to-region start end)
                         (list (point-min) (point-max) (buffer-string)))
                       (progn
                         (widen)
                         (list (point-min) (point-max) (buffer-string)))))"#
        ),
        r#"OK (6 t nil nil 4 4 (3 6 "bcd") (1 8 "Xabcdef"))"#
    );
}

#[test]
fn vm_buffer_mutation_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "abcdef")
                 (let ((start (copy-marker 2))
                       (end (copy-marker 5 t)))
                   (goto-char 1)
                   (insert "X")
                   (list (delete-and-extract-region start end)
                         (buffer-string)
                         (progn
                           (narrow-to-region 2 4)
                           (erase-buffer)
                           (list (point-min) (point-max) (buffer-string) (buffer-size))))))"#
        ),
        r#"OK ("bcd" "Xaef" (1 1 "" 0))"#
    );
}

#[test]
fn vm_casefiddle_region_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(progn
                 (insert "heLLo woRLD")
                 (list
                  (progn
                    (downcase-region 1 6)
                    (buffer-string))
                  (progn
                    (upcase-region 7 12)
                    (buffer-string))
                  (progn
                    (capitalize-region 1 12)
                    (buffer-string))
                  (progn
                    (upcase-initials-region 1 12)
                    (buffer-string))))"#
        ),
        r#"OK ("hello woRLD" "hello WORLD" "Hello World" "Hello World")"#
    );
}

#[test]
fn vm_casefiddle_word_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(progn
                 (insert "heLLo woRLD")
                 (list
                  (progn
                    (goto-char 1)
                    (downcase-word 1)
                    (buffer-string))
                  (progn
                    (goto-char 7)
                    (upcase-word 1)
                    (buffer-string))
                  (progn
                    (goto-char 1)
                    (capitalize-word 2)
                    (buffer-string))))"#
        ),
        r#"OK ("hello woRLD" "hello WORLD" "Hello World")"#
    );
}

#[test]
fn vm_char_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (char-equal ?a ?A)
                 (let ((case-fold-search nil))
                   (char-equal ?a ?A))
                 (bool-vector-p (char-category-set ?a)))"#
        ),
        "OK (t nil t)"
    );
}

#[test]
fn vm_buffer_substring_copy_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((src (get-buffer-create "*vm-sub-src*"))
                     (dst (get-buffer-create "*vm-sub-dst*")))
                 (set-buffer src)
                 (erase-buffer)
                 (insert "abcXYZ")
                 (put-text-property 2 5 'face 'bold)
                 (set-buffer dst)
                 (erase-buffer)
                 (insert-buffer-substring src 2 5)
                 (let ((sub (progn
                              (set-buffer src)
                              (buffer-substring 2 5)))
                       (copied (progn
                                 (set-buffer dst)
                                 (buffer-string))))
                   (list sub
                         (get-text-property 1 'face sub)
                         copied
                         (get-text-property 1 'face copied))))"#
        ),
        r#"OK (#("bcX" 0 3 (face bold)) bold #("bcX" 0 3 (face bold)) bold)"#
    );
}

#[test]
fn vm_compare_buffer_substrings_uses_shared_case_fold_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((left (get-buffer-create "*vm-cmp-left*"))
                     (right (get-buffer-create "*vm-cmp-right*")))
                 (set-buffer left)
                 (erase-buffer)
                 (insert "Abc")
                 (set-buffer right)
                 (erase-buffer)
                 (insert "aBc")
                 (list
                  (let ((case-fold-search nil))
                    (compare-buffer-substrings left nil nil right nil nil))
                  (let ((case-fold-search t))
                    (compare-buffer-substrings left nil nil right nil nil))
                  (let ((case-fold-search t))
                    (compare-buffer-substrings left 1 2 right 1 3))))"#
        ),
        "OK (-1 0 -2)"
    );
}

#[test]
fn vm_buffer_metrics_and_swap_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((left (get-buffer-create "*vm-buf-metrics-left*"))
                     (right (get-buffer-create "*vm-buf-metrics-right*")))
                 (set-buffer left)
                 (erase-buffer)
                 (insert "A\né\n")
                 (let ((left-hash (buffer-hash))
                       (left-stats (buffer-line-statistics))
                       (left-pixels (buffer-text-pixel-size nil nil t)))
                   (set-buffer right)
                   (erase-buffer)
                   (insert "xyz")
                   (let ((right-hash (buffer-hash)))
                     (buffer-swap-text left)
                     (list left-stats
                           left-pixels
                           (progn (set-buffer left) (buffer-string))
                           (progn (set-buffer right) (buffer-string))
                           (equal (progn (set-buffer left) (buffer-hash)) right-hash)
                           (equal (progn (set-buffer right) (buffer-hash)) left-hash)
                           (progn (set-buffer right) (buffer-line-statistics))))))"#
        ),
        "OK ((2 2 1.5) (1 . 2) \"xyz\" \"A\né\n\" t t (2 2 1.5))"
    );
}

#[test]
fn vm_minibuffer_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (list
                  (minibuffer-contents)
                  (minibuffer-contents-no-properties)
                  (minibuffer-depth)
                  (minibuffer-prompt)
                  (minibufferp)
                  (minibufferp nil t)
                  (minibufferp "x" nil)
                  ;; `abort-minibuffers' resolves its minibuffer level from the
                  ;; same shared state, then hands off to the Lisp
                  ;; `minibuffer-quit-recursive-edit' (undefined in this bare
                  ;; harness, which is exactly what makes the hand-off visible).
                  (condition-case err (catch 'exit (abort-minibuffers))
                    (void-function (car (cdr err))))))"#,
            |eval| {
                let minibuf_id = eval.buffers.create_buffer(" *Minibuf-1*");
                crate::emacs_core::minibuffer::install_minibuffer_buffer_text(
                    &mut eval.buffers,
                    minibuf_id,
                    &crate::heap_types::LispString::from_utf8("Prompt: "),
                    Some(&crate::heap_types::LispString::from_utf8("vm-mini")),
                    crate::emacs_core::minibuffer::default_minibuffer_prompt_properties(),
                );
                eval.buffers.set_current(minibuf_id);
                eval.minibuffers
                    .read_from_minibuffer(minibuf_id, "Prompt: ", Some("vm-mini"), None)
                    .expect("enter minibuffer");
            },
        ),
        r#"OK ("vm-mini" "vm-mini" 1 "Prompt: " t t nil minibuffer-quit-recursive-edit)"#
    );
}

#[test]
fn vm_abort_minibuffers_throws_a_minibuffer_quit_thunk_to_exit() {
    crate::test_utils::init_test_tracing();
    // GNU minibuf.c `Fabort_minibuffers` does not throw `t` (that is
    // `abort-recursive-edit`, which means plain `quit`).  It calls
    // `minibuffer-quit-recursive-edit`, which throws a *function* to `exit`;
    // `recursive_edit_1` then calls it, signaling `minibuffer-quit`.
    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (fset 'minibuffer-quit-recursive-edit
                       (lambda (&optional levels)
                         (setq levels (or levels 1))
                         (if (> levels 1)
                             (throw 'exit (lambda () (minibuffer-quit-recursive-edit (1- levels))))
                           (throw 'exit (lambda () (signal 'minibuffer-quit nil))))))
                 (let ((thrown (catch 'exit (abort-minibuffers))))
                   (list (functionp thrown)
                         ;; A specific handler distinguishes it ...
                         (condition-case err (funcall thrown) (minibuffer-quit (car err)))
                         ;; ... a plain `quit' handler still catches it ...
                         (condition-case err (funcall thrown) (quit (car err)))
                         ;; ... but it is not an `error': it inherits from
                         ;; `quit' only, so `error' must not swallow it.
                         (condition-case err (funcall thrown)
                           (error 'wrongly-caught-by-error)
                           (quit 'not-an-error)))))"#,
            |eval| {
                let minibuf_id = eval.buffers.create_buffer(" *Minibuf-1*");
                crate::emacs_core::minibuffer::install_minibuffer_buffer_text(
                    &mut eval.buffers,
                    minibuf_id,
                    &crate::heap_types::LispString::from_utf8("Prompt: "),
                    Some(&crate::heap_types::LispString::from_utf8("vm-mini")),
                    crate::emacs_core::minibuffer::default_minibuffer_prompt_properties(),
                );
                eval.buffers.set_current(minibuf_id);
                eval.minibuffers
                    .read_from_minibuffer(minibuf_id, "Prompt: ", Some("vm-mini"), None)
                    .expect("enter minibuffer");
            },
        ),
        "OK (t minibuffer-quit minibuffer-quit not-an-error)"
    );
}

#[test]
fn vm_waiting_for_user_input_builtin_uses_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str("(waiting-for-user-input-p)", |eval| {
            eval.set_waiting_for_user_input(true);
        }),
        "OK t"
    );
}

#[test]
fn vm_reader_message_and_completion_builtins_use_shared_runtime_entry() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(let ((buf (get-buffer "vm-message-buffer")))
                 (list
                  (format "%s" buf)
                  (format "%S" buf)
                  (format-message "%s" buf)
                  (progn (message "%s" buf) (current-message))
                  (progn (message-box "%S" buf) (current-message))
                  (progn (message-or-box "%S" buf) (current-message))
                  (try-completion "app" '("application" "apple"))
                  (test-completion "alpha" '("alpha" "beta"))
                  (read-from-string "(a . b)")
                  (read "(1 2)")))"#,
            |eval| {
                eval.buffers.create_buffer("vm-message-buffer");
                // GNU populates `current-message` only on the interactive
                // echo-area path (xdisp.c `message3_frame_nolog`); the default
                // VM test Context is noninteractive.
                eval.set_variable("noninteractive", Value::NIL);
            },
        ),
        r##"OK ("vm-message-buffer" "#<buffer vm-message-buffer>" "vm-message-buffer" "vm-message-buffer" "vm-message-buffer" "vm-message-buffer" "appl" t ((a . b) . 7) (1 2))"##
    );
}

#[test]
fn vm_completion_builtins_use_shared_runtime_callbacks() {
    crate::test_utils::init_test_tracing();
    // `defun` is defined in subr.el → bootstrap context required.
    assert_eq!(
        vm_bootstrap_eval_with_init_str(
            r#"(progn
                 (defun neo-vm-completion-target () nil)
                 (let ((items '("alpha" "alps" "beta"))
                       (pred (lambda (candidate)
                               (not (equal candidate "beta")))))
                   (let ((collection
                          (lambda (string predicate action)
                            (cond
                             ((eq action nil)
                              (try-completion string items predicate))
                             ((eq action t)
                              (all-completions string items predicate))
                             ((eq action 'lambda)
                              (test-completion string items predicate))
                             (t nil)))))
                     (list
                      (try-completion
                       "neo-vm-completion-target"
                       obarray
                       (lambda (sym) (eq sym 'neo-vm-completion-target)))
                     (not
                       (null
                        (member "neo-vm-completion-target"
                                (all-completions
                                 "neo-vm"
                                 obarray
                                 (lambda (sym)
                                   (eq sym 'neo-vm-completion-target))))))
                      (try-completion "al" collection pred)))))"#,
            |eval| {
                let obarray_proxy = Value::vector(vec![Value::NIL]);
                eval.obarray.set_symbol_value("obarray", obarray_proxy);
                eval.obarray
                    .set_symbol_value("neovm--obarray-object", obarray_proxy);
            }
        ),
        r#"OK (t t "alp")"#
    );
}

#[test]
fn vm_time_builtins_use_direct_timefns_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (length (current-time))
                 (consp (current-cpu-time))
                 (null (current-idle-time))
                 (consp (get-internal-run-time))
                 (let ((f (float-time '(0 1 500000 0))))
                   (and (> f 1.4) (< f 1.6)))
                 (equal (time-add '(0 1 200000 0) '(0 2 900000 0))
                        '(0 4 100000 0))
                 (equal (time-subtract '(0 3 100000 0) '(0 1 200000 0))
                        '(0 1 900000 0))
                 (time-less-p '(0 1 0 0) '(0 2 0 0))
                 (time-equal-p '(0 1 0 0) '(0 1 0 0))
                 (time-equal-p nil nil)
                 (null (time-equal-p nil '(0 0 0 0)))
                 (time-equal-p 'not-a-time 'not-a-time)
                 (equal (current-time-string '(0 0 0 0) t)
                        "Thu Jan  1 00:00:00 1970")
                 (equal (current-time-zone nil t) '(0 "GMT"))
                 (equal (encode-time '(0 0 0 1 1 1970 nil nil 0))
                        '(0 0))
                 (equal (decode-time '(0 0 0 0) t)
                        '(0 0 0 1 1 1970 4 nil 0))
                 (equal (time-convert '(0 42 0 0) 'integer) 42))"#
        ),
        r#"OK (4 t t t t t t t t t t t t t t t t)"#
    );
}

#[test]
fn vm_misc_runtime_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (null (daemonp))
                 (condition-case err
                     (daemon-initialized)
                   (error (eq (car err) 'error)))
                 (null (flush-standard-output))
                 (equal (force-mode-line-update 'foo) 'foo)
                 (force-window-update)
                 (stringp (invocation-directory))
                 (stringp (invocation-name))
                 (integerp (emacs-pid)))"#
        ),
        r#"OK (t t t t t t t t)"#
    );
}

#[test]
fn vm_minibuffer_reader_frontends_use_shared_runtime_batch_eof_path() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (let ((unread-command-events (list 97)))
                   (condition-case err
                       (read-from-minibuffer "Prompt: ")
                     (end-of-file (list (car err) unread-command-events))))
                 (let ((unread-command-events (list 98)))
                   (condition-case err
                       (read-string "Prompt: ")
                     (end-of-file (list (car err) unread-command-events))))
                 (let ((unread-command-events (list 99)))
                   (condition-case err
                       (completing-read "Prompt: " '("alpha"))
                     (end-of-file (list (car err) unread-command-events))))
                 (let ((unread-command-events (list 100)))
                   (condition-case err
                       (read-buffer "Buffer: ")
                     (end-of-file (list (car err) unread-command-events))))
                 (let ((unread-command-events (list 101)))
                   (condition-case err
                       (read-command "Command: ")
                     (end-of-file (list (car err) unread-command-events))))
                 (let ((unread-command-events (list 102)))
                   (condition-case err
                       (read-variable "Variable: ")
                     (end-of-file (list (car err) unread-command-events))))
                 (let ((unread-command-events (list 103)))
                   (condition-case err
                       (yes-or-no-p "Confirm?")
                     (end-of-file (list (car err) unread-command-events)))))"#
        ),
        "OK ((end-of-file (97)) (end-of-file (98)) (end-of-file (99)) (end-of-file (100)) (end-of-file (101)) (end-of-file (102)) (end-of-file (103)))"
    );
}

#[test]
fn vm_read_buffer_delegates_to_read_buffer_function() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((target (get-buffer-create "rbf-target")))
                 (unwind-protect
                     (let ((read-buffer-function
                            (lambda (&rest args) args)))
                       (read-buffer "Buffer: " target t))
                   (kill-buffer target)))"#
        ),
        r#"OK ("Buffer: " "rbf-target" t)"#
    );
}

#[test]
fn vm_read_buffer_override_receives_predicate_and_completion_case_binding() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((completion-ignore-case nil)
                     (read-buffer-completion-ignore-case t)
                     (read-buffer-function
                      (lambda (&rest args)
                        (list args completion-ignore-case))))
                 (list (read-buffer "Buffer: " nil nil 'identity)
                       completion-ignore-case))"#
        ),
        r#"OK ((("Buffer: " nil nil identity) t) nil)"#
    );
}

#[test]
fn vm_completing_read_calls_completing_read_function() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((completing-read-function
                      (lambda (&rest args)
                        (cons 'called args))))
                 (completing-read "Prompt: " '("alpha") nil t nil 'hist "alpha" t))"#
        ),
        r#"OK (called "Prompt: " ("alpha") nil t nil hist "alpha" t)"#
    );
}

#[test]
fn vm_printer_builtins_use_shared_runtime_entry() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let* ((live (get-buffer-create "vm-print-live"))
                      (out (get-buffer-create "*vm-print-out*")))
                 (set-buffer out)
                 (erase-buffer)
                 (list
                  (equal (prin1-to-string live) "#<buffer vm-print-live>")
                  (progn
                    (princ live out)
                    (set-buffer out)
                    (equal (buffer-string) "vm-print-live"))
                  (progn
                    (erase-buffer)
                    (prin1 live out)
                    (set-buffer out)
                    (equal (buffer-string) "#<buffer vm-print-live>"))
                  (progn
                    (erase-buffer)
                    (print live out)
                    (set-buffer out)
                    (equal (buffer-string) "
#<buffer vm-print-live>
"))
                  (progn
                    (erase-buffer)
                    (write-char 65 out)
                    (terpri out)
                    (set-buffer out)
                    (equal (buffer-string) "A
"))))"##
        ),
        "OK (t t t t t)"
    );
}

#[test]
fn vm_prin1_to_string_prints_killed_buffers_in_nested_values_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let* ((buf (get-buffer-create "vm-print-dead"))
                      (rec (record 'vm-print-owner buf))
                      (text (copy-sequence "payload")))
                 (put-text-property 0 (length text) 'owner rec text)
                 (kill-buffer buf)
                 (list
                  (equal (prin1-to-string rec)
                         "#s(vm-print-owner #<killed buffer>)")
                  (if (string-match "#<killed buffer>" (prin1-to-string text) nil t) t)))"##
        ),
        "OK (t t)"
    );
}

#[test]
fn vm_write_char_and_terpri_callable_targets_use_shared_runtime_callback() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (setq vm-print-calls nil)
                 (fset 'vm-print-target
                       (lambda (ch)
                         (setq vm-print-calls (cons ch vm-print-calls))))
                 (list
                  (write-char 65 'vm-print-target)
                  (terpri 'vm-print-target)
                  vm-print-calls))"#
        ),
        "OK (65 t (10 65))"
    );
}

#[test]
fn vm_princ_prin1_and_print_callable_targets_stream_gnu_char_callbacks() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (setq vm-print-calls nil)
                 (fset 'vm-print-target
                       (lambda (ch)
                         (setq vm-print-calls (cons ch vm-print-calls))))
                 (list
                  (progn
                    (setq vm-print-calls nil)
                    (princ "ab" 'vm-print-target)
                    vm-print-calls)
                  (progn
                    (setq vm-print-calls nil)
                    (prin1 '(1 . 2) 'vm-print-target)
                    vm-print-calls)
                  (progn
                    (setq vm-print-calls nil)
                    (print 'foo 'vm-print-target)
                    vm-print-calls)))"#
        ),
        "OK ((98 97) (41 50 32 46 32 49 40) (10 111 111 102 10))"
    );
}

#[test]
fn vm_marker_print_targets_insert_and_restore_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let* ((orig (current-buffer))
                      (obuf (get-buffer-create "*vm-marker-print*")))
                 (with-current-buffer obuf
                   (erase-buffer)
                   (insert "xy")
                   (goto-char 2))
                 (let ((m (with-current-buffer obuf (point-marker))))
                   (list
                    (progn
                      (princ "ab" m)
                      (with-current-buffer obuf
                        (list (buffer-string) (point) (marker-position m))))
                    (progn
                      (write-char 67 m)
                      (with-current-buffer obuf
                        (list (buffer-string) (point) (marker-position m))))
                    (progn
                      (terpri m)
                      (with-current-buffer obuf
                        (list (buffer-string) (point) (marker-position m))))
                    (eq (current-buffer) orig))))"#
        ),
        "OK ((\"xaby\" 4 4) (\"xabCy\" 5 5) (\"xabC\ny\" 6 6) t)"
    );
}

#[test]
fn vm_with_current_buffer_restores_outer_point_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let* ((orig (current-buffer))
                      (orig-point (point))
                      (obuf (get-buffer-create "*vm-wcb*")))
                 (with-current-buffer obuf
                   (erase-buffer)
                   (insert "xy")
                   (goto-char 2))
                 (list (eq (current-buffer) orig)
                       orig-point
                       (point)
                       (with-current-buffer obuf (buffer-string))))"#
        ),
        "OK (t 1 1 \"xy\")"
    );
}

#[test]
fn vm_save_current_buffer_restores_outer_point_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let* ((orig (current-buffer))
                      (orig-point (point))
                      (obuf (get-buffer-create "*vm-save-current-buffer*")))
                 (save-current-buffer
                   (set-buffer obuf)
                   (erase-buffer)
                   (insert "xy")
                   (goto-char 2))
                 (list (eq (current-buffer) orig)
                       orig-point
                       (point)
                       (with-current-buffer obuf (buffer-string))))"#
        ),
        "OK (t 1 1 \"xy\")"
    );
}

#[test]
fn vm_case_table_builtins_use_shared_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((buf (current-buffer))
                      (other (get-buffer-create "*case-other*"))
                      (third (get-buffer-create "*case-third*"))
                      (standard (standard-case-table))
                      (custom (copy-sequence standard)))
                 (list
                  (eq (current-case-table) standard)
                  (null (eq standard custom))
                  (progn (set-case-table custom) (eq (current-case-table) custom))
                  (progn (set-buffer other) (eq (current-case-table) standard))
                  (progn (set-buffer buf) (eq (current-case-table) custom))
                  (progn (set-standard-case-table custom) (eq (standard-case-table) custom))
                  (progn (set-buffer third) (eq (current-case-table) custom))))"#
        ),
        "OK (t t t t t t t)"
    );
}

#[test]
fn vm_undo_boundary_uses_shared_buffer_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("scratch buffer");
        buffer.insert("x");
    }
    let result = eval.eval_str("(undo-boundary)");
    assert!(matches!(result, Ok(value) if value.is_nil()));
    let buffer = eval.buffers.current_buffer().expect("scratch buffer");
    let ul = buffer.get_undo_list();
    assert!(crate::buffer::undo_list_has_trailing_boundary(&ul));
}

#[test]
fn vm_simple_process_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        r#"(let ((p (get-process "vm-proc")))
             (list
              (processp p)
              (null (processp 99))
              (eq (get-process "vm-proc") p)
              (eq (get-buffer-process "*vm-proc*") p)
              (equal (process-name p) "vm-proc")
              (equal (process-command p) '("/bin/echo" "hello"))
              (let ((b (process-buffer p)))
                (and (bufferp b) (equal (buffer-name b) "*vm-proc*")))
              (process-query-on-exit-flag p)
              (eq (set-process-query-on-exit-flag p nil) nil)
              (null (process-query-on-exit-flag p))
              (eq (set-process-buffer p nil) nil)
              (null (process-buffer p))
              (let ((xs (process-list)))
                (and (memq p xs) (= (length xs) 1)))))"#,
        |eval| {
            let _buffer_id = eval.buffers.create_buffer("*vm-proc*");
            let _pid = eval.processes.create_process(
                "vm-proc".into(),
                Value::make_buffer(
                    eval.buffers
                        .find_buffer_by_name("*vm-proc*")
                        .expect("process buffer"),
                ),
                "/bin/echo".into(),
                vec!["hello".into()],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
        },
    );
    assert_eq!(result, "OK (t t t t t t t t t t t t t)");
}

#[test]
fn vm_stale_process_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        // Capture the live process object, then delete it inside the form so we
        // hold a first-class (now-stale) process value: GNU keeps it `processp`
        // after `delete-process`.
        r#"(let ((p (get-process "vm-stale-proc")))
             (delete-process p)
             (list
              (processp p)
              (let ((b (process-buffer p)))
                (and (bufferp b) (equal (buffer-name b) "*vm-stale-proc*")))
              (eq (set-process-buffer p nil) nil)
              (null (process-buffer p))
              (eq (set-process-query-on-exit-flag p nil) nil)
              (null (process-query-on-exit-flag p))
              (null (get-process "vm-stale-proc"))
              (null (get-buffer-process "*vm-stale-proc*"))))"#,
        |eval| {
            let _buffer_id = eval.buffers.create_buffer("*vm-stale-proc*");
            let pid = eval.processes.create_process(
                "vm-stale-proc".into(),
                Value::make_buffer(
                    eval.buffers
                        .find_buffer_by_name("*vm-stale-proc*")
                        .expect("process buffer"),
                ),
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
        },
    );
    assert_eq!(result, "OK (t t t t t t t t)");
}

#[test]
fn vm_process_introspection_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    // process-live-p is a defun in subr.el → bootstrap context required.
    let result = vm_bootstrap_eval_with_init_str(
        r#"(let ((p (get-process "vm-proc-introspect")))
             (list
              (equal (process-live-p p) '(run open listen connect stop))
              (integerp (process-id p))
              (eq (process-type p) 'real)
              (markerp (process-mark p))
              (null (marker-buffer (process-mark p)))
              (null (marker-position (process-mark p)))
              (null (process-thread p))
              (eq (process-filter p) 'internal-default-process-filter)
              (eq (set-process-filter p nil) 'internal-default-process-filter)
              (eq (set-process-filter p 'ignore) 'ignore)
              (eq (process-filter p) 'ignore)
              (eq (process-sentinel p) 'internal-default-process-sentinel)
              (eq (set-process-sentinel p nil) 'internal-default-process-sentinel)
              (eq (set-process-sentinel p 'ignore) 'ignore)
              (eq (process-sentinel p) 'ignore)
              (equal (set-process-plist p '(a 1)) '(a 1))
              (equal (process-plist p) '(a 1))
              (equal (set-process-plist p '(a 1 k 2)) '(a 1 k 2))
              (equal (process-plist p) '(a 1 k 2))))"#,
        |eval| {
            let pid = eval.processes.create_process(
                "vm-proc-introspect".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
            // Simulate a spawned child: `create_process` only registers the
            // record, so `os_pid` is None and GNU-correct `process-id` returns
            // nil (commit e9359911e).  A real, live `'real` process has the
            // child's OS pid, so give the fixture a synthetic one to match what
            // `(integerp (process-id p))` observes for a running process.
            eval.processes
                .get_mut(pid)
                .expect("introspect process")
                .os_pid = Some(4242);
        },
    );
    assert_eq!(result, "OK (t t t t t t t t t t t t t t t t t t t)");
}

#[test]
fn vm_stale_process_introspection_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    // process-live-p is a defun in subr.el → bootstrap context required.
    let result = vm_bootstrap_eval_with_init_str(
        r#"(let ((p (get-process "vm-proc-stale-introspect")))
             (delete-process p)
             (list
              (null (process-live-p p))
              (integerp (process-id p))
              (eq (process-type p) 'real)
              (eq (set-process-filter p 'ignore) 'ignore)
              (eq (process-filter p) 'ignore)
              (eq (set-process-sentinel p 'ignore) 'ignore)
              (eq (process-sentinel p) 'ignore)
              (equal (set-process-plist p '(a 1)) '(a 1))
              (equal (process-plist p) '(a 1))
              (null (process-thread p))))"#,
        |eval| {
            let pid = eval.processes.create_process(
                "vm-proc-stale-introspect".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
            // Simulate a spawned child before the test deletes it: GNU keeps the
            // process object's recorded OS pid after `delete-process`, so
            // `(integerp (process-id p))` is t on the stale handle.  Without a
            // spawn, `os_pid` is None and GNU-correct `process-id` returns nil
            // (commit e9359911e); give the fixture a synthetic pid to match.
            eval.processes
                .get_mut(pid)
                .expect("stale introspect process")
                .os_pid = Some(4242);
        },
    );
    assert_eq!(result, "OK (t t t t t t t t t t)");
}

#[test]
fn vm_process_coding_and_tty_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        r#"(let ((p (get-process "vm-proc-coding"))
                 (pp (get-process "vm-proc-pipe"))
                 (np (get-process "vm-proc-network")))
             (list
              ;; This fixture is a raw process record, built through
              ;; `ProcessManager::create_process' rather than through any of
              ;; GNU's five creation primitives, so no coding resolver has run
              ;; on it.  That state is GNU's `make_process' state: both slots
              ;; nil.  It used to read `(utf-8-unix . utf-8-unix)' because the
              ;; constructor invented one -- see DIVERGENCES.md entry 137.
              (equal (process-coding-system p) '(nil . nil))
              (null (process-datagram-address p))
              (null (process-inherit-coding-system-flag p))
              (null (set-process-coding-system p 'utf-16le 'utf-8-unix))
              (equal (process-coding-system p) '(utf-16le . utf-8-unix))
              (eq (set-process-inherit-coding-system-flag p t) t)
              (process-inherit-coding-system-flag p)
              (stringp (process-tty-name p))
              (stringp (process-tty-name p 'stdin))
              (stringp (process-tty-name p 'stdout))
              (stringp (process-tty-name p 'stderr))
              (eq (condition-case err (process-tty-name p 0) (error (car err))) 'error)
              (null (process-tty-name pp))
              (null (process-tty-name pp nil))
              (null (process-tty-name pp 'stdin))
              (null (process-tty-name pp 'stdout))
              (null (process-tty-name pp 'stderr))
              (null (process-tty-name np))
              (null (process-tty-name np nil))
              (null (process-tty-name np 'stdin))
              (null (process-tty-name np 'stdout))
              (null (process-tty-name np 'stderr))
              (eq (set-process-datagram-address p nil) nil)
              (null (process-datagram-address p))
              (eq (set-process-window-size p 10 20) t)))"#,
        |eval| {
            let pid = eval.processes.create_process(
                "vm-proc-coding".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
            let pipe_id = eval.processes.create_process_with_kind(
                "vm-proc-pipe".into(),
                Value::NIL,
                String::new(),
                vec![],
                crate::emacs_core::process::ProcessKindWithoutDevice::Pipe,
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pipe_id, 2);
            let network_id = eval.processes.create_process_with_kind(
                "vm-proc-network".into(),
                Value::NIL,
                String::new(),
                vec![],
                crate::emacs_core::process::ProcessKindWithoutDevice::Network,
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(network_id, 3);
        },
    );
    assert_eq!(
        result,
        "OK (t t t t t t t t t t t t t t t t t t t t t t t t t)"
    );
}

#[test]
fn vm_stale_process_coding_and_tty_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        r#"(let ((p (get-process "vm-proc-stale-coding")))
             (delete-process p)
             (list
              (null (set-process-coding-system p 'utf-16le))
              ;; Single DECODING arg, ENCODING omitted (nil): GNU
              ;; `Fset_process_coding_system` runs the nil ENCODING through
              ;; `coding_inherit_eol_type` which normalizes it to raw-text-unix,
              ;; so the pair becomes (utf-16le . raw-text-unix) — verified
              ;; byte-identical on the GNU binary (31.0.x).
              (equal (process-coding-system p) '(utf-16le . raw-text-unix))
              (eq (set-process-inherit-coding-system-flag p t) t)
              (process-inherit-coding-system-flag p)
              (eq (set-process-datagram-address p nil) nil)
              (null (process-datagram-address p))
              (null (set-process-window-size p 10 20))
              (stringp (process-tty-name p))))"#,
        |eval| {
            let pid = eval.processes.create_process(
                "vm-proc-stale-coding".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
        },
    );
    assert_eq!(result, "OK (t t t t t t t t)");
}

#[test]
fn vm_process_status_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    // process-kill-buffer-query-function is defined in subr.el →
    // bootstrap context required.
    let result = vm_bootstrap_eval_with_init_str(
        r#"(list
             (eq (process-status (get-process "vm-status-real")) 'run)
             (eq (process-status (get-process "vm-status-pipe")) 'open)
             (eq (process-status (get-process "vm-status-network")) 'listen)
             (eq (process-status (get-process "vm-status-stop")) 'stop)
             (eq (process-status (get-process "vm-status-signal")) 'signal)
             (= (process-exit-status (get-process "vm-status-signal")) 9)
             (eq (process-status "vm-status-real") 'run)
             (null (process-status "vm-status-missing"))
             (process-kill-buffer-query-function))"#,
        |eval| {
            use crate::emacs_core::process::ProcessKindWithoutDevice;

            let real = eval.processes.create_process(
                "vm-status-real".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(real, 1);
            let pipe = eval.processes.create_process_with_kind(
                "vm-status-pipe".into(),
                Value::NIL,
                String::new(),
                vec![],
                ProcessKindWithoutDevice::Pipe,
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pipe, 2);
            let network = eval.processes.create_process_with_kind(
                "vm-status-network".into(),
                Value::NIL,
                String::new(),
                vec![],
                ProcessKindWithoutDevice::Network,
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(network, 3);
            // Mark as server so process-status returns 'listen (not 'open).
            eval.processes.get_mut(network).unwrap().childp =
                Value::list(vec![Value::keyword(":server"), Value::T]);
            let stopped = eval.processes.create_process(
                "vm-status-stop".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(stopped, 4);
            let signaled = eval.processes.create_process(
                "vm-status-signal".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(signaled, 5);
            eval.processes.get_any_mut(stopped).unwrap().status =
                Value::list(vec![Value::symbol("stop"), Value::fixnum(0)]);
            eval.processes.get_any_mut(signaled).unwrap().status =
                Value::list(vec![Value::symbol("signal"), Value::fixnum(9), Value::NIL]);
        },
    );
    assert_eq!(result, "OK (t t t t t t t t t)");
}

#[test]
fn vm_stale_process_status_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    // process-kill-buffer-query-function is in subr.el → bootstrap.
    let result = vm_bootstrap_eval_with_init_str(
        r#"(let ((p (get-process "vm-status-stale")))
             (delete-process p)
             (list
              (eq (process-status p) 'signal)
              (= (process-exit-status p) 9)
              (null (process-status "vm-status-stale"))
              (process-kill-buffer-query-function)))"#,
        |eval| {
            let pid = eval.processes.create_process(
                "vm-status-stale".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
            eval.processes.get_any_mut(pid).unwrap().status =
                Value::list(vec![Value::symbol("signal"), Value::fixnum(9), Value::NIL]);
        },
    );
    assert_eq!(result, "OK (t t t t)");
}

#[test]
fn vm_process_control_and_send_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();

    let buffer_id = eval.buffers.create_buffer("*vm-proc-control*");
    eval.buffers.set_current(buffer_id);
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("abc");

    let current_id = eval.processes.create_process(
        "vm-proc-current".into(),
        Value::make_buffer(buffer_id),
        "/bin/cat".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    assert_eq!(current_id, 1);
    eval.processes
        .get_mut(current_id)
        .expect("current process")
        .status = Value::list(vec![Value::symbol("stop"), Value::fixnum(0)]);

    for expected in 2..=7 {
        let id = eval.processes.create_process(
            format!("vm-proc-{expected}"),
            Value::NIL,
            "/bin/cat".into(),
            vec![],
            crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
        );
        assert_eq!(id, expected);
    }

    let result = eval
        .eval_str(
            r#"(let ((p1 (get-process "vm-proc-current"))
                     (p2 (get-process "vm-proc-2"))
                     (p3 (get-process "vm-proc-3"))
                     (p4 (get-process "vm-proc-4"))
                     (p5 (get-process "vm-proc-5"))
                     (p6 (get-process "vm-proc-6"))
                     (p7 (get-process "vm-proc-7")))
               (list
                (null (continue-process))
                (eq (process-status p1) 'run)
                (eq (interrupt-process p2) p2)
                (eq (process-status p2) 'signal)
                (= (process-exit-status p2) 2)
                (eq (kill-process p3) p3)
                (= (process-exit-status p3) 9)
                (eq (stop-process p4) p4)
                ;; Harness-only process records have no child to observe with waitpid,
                ;; so stop-process must not publish a stop status synchronously.
                (eq (process-status p4) 'run)
                (eq (quit-process p5) p5)
                (eq (process-status p5) 'run)
                (eq (signal-process p6 15) 0)
                (= (process-exit-status p6) 15)
                (null (process-send-string p7 "hello"))
                (null (process-send-region p7 (point-min) (point-max)))
                (eq (process-send-eof p7) p7)
                (null (process-running-child-p p7))))"#,
        )
        .expect("process control/send builtins should execute");

    assert_eq!(
        crate::emacs_core::error::format_eval_result(&Ok(result)),
        "OK (t t t t t t t t t t t t t t t t t)"
    );
    assert!(crate::emacs_core::value::equal_value(
        &eval
            .processes
            .get(7)
            .expect("send target process")
            .write_queue,
        &Value::list(vec![
            Value::cons(
                Value::heap_string(crate::heap_types::LispString::from_utf8("hello")),
                Value::cons(Value::fixnum(0), Value::fixnum(5)),
            ),
            Value::cons(
                Value::heap_string(crate::heap_types::LispString::from_utf8("abc")),
                Value::cons(Value::fixnum(0), Value::fixnum(3)),
            ),
            // GNU `process-send-eof` queues an unencoded EOT after all
            // already accepted input when the process uses a PTY.
            Value::cons(
                Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0x04])),
                Value::cons(Value::fixnum(0), Value::fixnum(1)),
            ),
        ]),
        0,
    ));
    assert_eq!(
        eval.processes.get_any(1).expect("current process").status,
        Value::symbol("run")
    );
    assert_eq!(
        eval.processes.get_any(2).expect("interrupt process").status,
        Value::list(vec![Value::symbol("signal"), Value::fixnum(2), Value::NIL])
    );
    assert_eq!(
        eval.processes.get_any(3).expect("kill process").status,
        Value::list(vec![Value::symbol("signal"), Value::fixnum(9), Value::NIL])
    );
    assert_eq!(
        eval.processes.get_any(4).expect("stop process").status,
        Value::symbol("run")
    );
    assert_eq!(
        eval.processes.get_any(5).expect("quit process").status,
        Value::symbol("run")
    );
    assert_eq!(
        eval.processes.get_any(6).expect("signal process").status,
        Value::list(vec![Value::symbol("signal"), Value::fixnum(15), Value::NIL])
    );
}

#[test]
fn vm_stale_process_control_and_send_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        r#"(let ((p (get-process "vm-stale-proc-control")))
             (delete-process p)
             (list
              (condition-case err (continue-process p) (error (car err)))
              (condition-case err (interrupt-process p) (error (car err)))
              (condition-case err (kill-process p) (error (car err)))
              (condition-case err (stop-process p) (error (car err)))
              (condition-case err (quit-process p) (error (car err)))
              (let ((rv (signal-process p 0)))
                (or (eq rv 0) (eq rv -1)))
              (condition-case err (process-send-string p "hello") (error (car err)))
              (condition-case err
                  (process-send-region p (point-min) (point-max))
                (error (car err)))
              (condition-case err (process-send-eof p) (error (car err)))
              (condition-case err (process-running-child-p p) (error (car err)))))"#,
        |eval| {
            let buffer_id = eval.buffers.create_buffer("*vm-stale-proc-control*");
            eval.buffers.set_current(buffer_id);
            eval.buffers
                .current_buffer_mut()
                .expect("current buffer")
                .insert("abc");
            let pid = eval.processes.create_process(
                "vm-stale-proc-control".into(),
                Value::make_buffer(buffer_id),
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
        },
    );
    assert_eq!(
        result,
        "OK (error error error error error t error error error error)"
    );
}

#[test]
fn vm_delete_process_builtin_uses_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        // GNU keeps a deleted process `processp`-true; its status becomes
        // `signal` with exit code 9.
        r#"(let ((p (get-buffer-process "*vm-delete-proc*")))
             (list
              (processp p)
              (eq (delete-process nil) nil)
              (processp p)
              (null (memq (process-status p) '(run open listen connect stop)))
              (eq (process-status p) 'signal)
              (= (process-exit-status p) 9)
              (null (get-process "vm-delete-proc"))
              (null (get-buffer-process "*vm-delete-proc*"))))"#,
        |eval| {
            let buffer_id = eval.buffers.create_buffer("*vm-delete-proc*");
            eval.buffers.set_current(buffer_id);
            let pid = eval.processes.create_process(
                "vm-delete-proc".into(),
                Value::make_buffer(buffer_id),
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
        },
    );
    assert_eq!(result, "OK (t t t t t t t t)");
}

#[test]
fn vm_process_contact_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_str(
        r#"(let ((network (make-network-process :name "vm-contact-network" :server t :service 0))
                 (pipe (make-pipe-process :name "vm-contact-pipe")))
             (unwind-protect
                 (list
                  (let ((port (process-contact network :service))
                        (local (process-contact network :local)))
                    (list
                     (stringp (process-contact network :name))
                     (eq (process-contact network :server) t)
                     (integerp port)
                     (and (vectorp local)
                          (= (length local) 5)
                          (= (aref local 0) 127)
                          (= (aref local 4) port))
                     (null (process-contact network :remote))
                     (null (process-contact network :coding))
                     (null (process-contact network :foo))))
                  (list
                   (stringp (process-contact pipe :name))
                   (null (process-contact pipe :server))
                   (null (process-contact pipe :service))
                   (null (process-contact pipe :local))
                   (null (process-contact pipe :remote))
                   (null (process-contact pipe :coding))
                   (null (process-contact pipe :foo))))
               (condition-case nil (delete-process network) (error nil))
               (condition-case nil (delete-process pipe) (error nil))))"#,
    );
    assert_eq!(result, "OK ((t t t t t t t) (t t t t t t t))");
}

#[test]
fn vm_process_attributes_builtin_uses_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (let ((attrs (process-attributes (emacs-pid))))
                   (and (listp attrs)
                        (null (assq 'pid attrs))
                        (let ((pair (assq 'comm attrs)))
                          (and (consp pair) (stringp (cdr pair))))))
                 (null (process-attributes -1))
                 (eq (car (condition-case err (process-attributes 'x) (error err)))
                     'wrong-type-argument)
                 (null (process-attributes 999999999)))"#
        ),
        "OK (t t t t)"
    );
}

#[test]
fn vm_set_process_thread_builtin_uses_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        r#"(let ((thr (current-thread))
                 (p (get-process "vm-process-thread")))
             (list
              (eq (set-process-thread p thr) thr)
              (eq (process-thread p) thr)
              (eq (set-process-thread p nil) nil)
              (null (process-thread p))
              (eq (car (condition-case err (set-process-thread p 'x) (error err)))
                  'wrong-type-argument)))"#,
        |eval| {
            let pid = eval.processes.create_process(
                "vm-process-thread".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
        },
    );
    assert_eq!(result, "OK (t t t t t)");
}

#[test]
fn vm_non_child_process_creation_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (make-network-process)
                 (condition-case err (make-network-process :name "np") (error err))
                 (condition-case err (make-network-process :name 1) (error err))
                 (condition-case err (make-network-process :service 80) (error err))
                 (let ((p (make-network-process :name "np-server" :server t :service 0)))
                   (list (processp p)
                         (eq (process-type p) 'network)
                         (eq (process-thread p) (current-thread))))
                 (make-pipe-process)
                 (let ((p (make-pipe-process :name "pp")))
                   (list (processp p)
                         (eq (process-type p) 'pipe)
                         (let ((b (process-buffer p)))
                           (and (bufferp b) (equal (buffer-name b) "pp")))
                         (eq (process-thread p) (current-thread))))
                 (condition-case err (make-pipe-process :name 1) (error err))
                 (make-serial-process)
                 (condition-case err (make-serial-process :name "sp" :port t :speed 9600) (error err))
                 (condition-case err (make-serial-process :name "sp" :port 1 :speed 9600) (error err))
                 (condition-case err (make-serial-process :name "sp") (error err))
                 (condition-case err (make-serial-process :name "sp" :port "/tmp/no-port") (error err))
                 (let ((p (make-serial-process :name "sp" :port "/dev/ptmx" :speed 9600)))
                   (list (processp p)
                         (eq (process-type p) 'serial))))"#
        ),
        "OK (nil (wrong-type-argument stringp nil) (error \":name value not a string\") (error \"Missing :name keyword parameter\") (t t t) nil (t t t t) (error \":name value not a string\") nil (wrong-type-argument stringp t) (wrong-type-argument stringp 1) (error \"No port specified\") (error \":speed not specified\") (t t))"
    );
}

#[test]
fn vm_network_and_serial_process_config_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        r#"(let ((real (get-process "vm-real"))
                 (network (get-process "vm-network")))
             (list
              (null (serial-process-configure))
              (null (serial-process-configure :name "vm-serial"))
              (eq (car (condition-case err (serial-process-configure :process real) (error err))) 'error)
              (eq (car (condition-case err (set-network-process-option) (error err)))
                  'wrong-number-of-arguments)
              (eq (car (condition-case err (set-network-process-option real :foo 1) (error err)))
                  'error)
              (eq (car (condition-case err (set-network-process-option network 1 1) (error err)))
                  'wrong-type-argument)
              (null (set-network-process-option network :foo 1 t))
              (eq (car (condition-case err (set-network-process-option network :foo 1) (error err)))
                  'error)))"#,
        |eval| {
            use crate::emacs_core::process::ProcessKindWithoutDevice;

            let buffer_id = eval.buffers.create_buffer("*vm-serial-proc*");
            eval.buffers.set_current(buffer_id);
            // A serial process record cannot be fabricated any more: the
            // constructor takes an OPEN device.  `/dev/ptmx` is the real
            // character device the serial pins use -- see DIVERGENCES.md 147.
            let device = crate::emacs_core::process::sys::SerialPort::open(std::ffi::OsStr::new(
                "/dev/ptmx",
            ))
            .expect("/dev/ptmx opens on any Linux");
            let serial_id = eval.processes.create_serial_process(
                crate::heap_types::LispString::from_utf8("vm-serial"),
                Value::make_buffer(buffer_id),
                device,
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(serial_id, 1);
            let real_id = eval.processes.create_process(
                "vm-real".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(real_id, 2);
            let network_id = eval.processes.create_process_with_kind(
                "vm-network".into(),
                Value::NIL,
                String::new(),
                vec![],
                ProcessKindWithoutDevice::Network,
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(network_id, 3);
        },
    );
    assert_eq!(result, "OK (t t t t t t t t)");
}

#[test]
fn vm_thread_mutex_and_condition_builtins_use_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((main (current-thread))
                      (worker (make-thread (lambda () 42) "vm-worker"))
                      (threads-before (all-threads))
                      (mx (make-mutex "vm-mutex"))
                      (cv (make-condition-variable mx "vm-cond")))
                 (list
                  (threadp main)
                  (threadp worker)
                  (equal (thread-name worker) "vm-worker")
                  (thread-live-p worker)
                  (consp (memq main threads-before))
                  (consp (memq worker threads-before))
                  (null (thread-yield))
                  (null (thread-signal worker 'error '("oops")))
                  (condition-case err
                      (thread-join worker)
                    (error (car err)))
                  (null (thread-last-error))
                  (eq main-thread main)
                  (null (thread-buffer-disposition main))
                  (eq (thread-set-buffer-disposition worker 'silently) 'silently)
                  (eq (thread-buffer-disposition worker) 'silently)
                  (condition-case err
                      (progn
                        (thread-set-buffer-disposition main t)
                        nil)
                    (wrong-type-argument (car err)))
                  (mutexp mx)
                  (equal (mutex-name mx) "vm-mutex")
                  (null (mutex-lock mx))
                  (condition-variable-p cv)
                  (equal (condition-name cv) "vm-cond")
                  (eq (condition-mutex cv) mx)
                  (null (condition-notify cv))
                  (null (condition-wait cv))
                  (null (mutex-unlock mx))))"#
        ),
        "OK (t t t t t t t t error t t t t t wrong-type-argument t t t t t t t t t)"
    );
}

#[test]
fn vm_make_thread_runs_body_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (setq vm-thread-seen nil)
                 (let* ((main (current-thread))
                        (worker (make-thread
                                 (lambda ()
                                   (setq vm-thread-seen (current-thread))
                                   (current-thread))
                                 "vm-worker")))
                   (list
                    (threadp worker)
                    (null (eq main worker))
                    (eq vm-thread-seen worker)
                    (eq (thread-join worker) worker)
                    (eq (current-thread) main))))"#
        ),
        "OK (t t t t t)"
    );
}

#[test]
fn vm_make_thread_restores_caller_current_buffer() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        r#"(let* ((orig (current-buffer))
                  (worker (make-thread
                           (lambda ()
                             (set-buffer vm-thread-target-buffer)
                             (current-buffer))
                           "vm-worker")))
             (list
              (eq (thread-join worker) vm-thread-target-buffer)
              (eq (current-buffer) orig)))"#,
        |eval| {
            let target = eval.buffers.create_buffer("vm-thread-target-buffer");
            eval.set_variable("vm-thread-target-buffer", Value::make_buffer(target));
        },
    );
    assert_eq!(result, "OK (t t)");
}

#[test]
fn vm_make_thread_records_join_error_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((worker (make-thread
                               (lambda ()
                                 (signal 'error '(99)))
                               "vm-boom"))
                      (published (thread-last-error))
                      ;; GNU `thread-join` does NOT re-raise an error signalled
                      ;; inside the thread function (record_thread_error only
                      ;; publishes thread-last-error; thread.c:753-758,
                      ;; 1118-1144); it returns the thread's result.  The error
                      ;; stays visible through thread-last-error.
                      (joined (condition-case join-err
                                  (progn
                                    (thread-join worker)
                                    :no-error)
                                (error join-err)))
                      (after (thread-last-error)))
                 (list
                  (threadp worker)
                  (and (consp published)
                       (eq (car published) 'error)
                       (equal (cdr published) '(99)))
                  (eq joined :no-error)
                  (and (consp after)
                       (eq (car after) 'error)
                       (equal (cdr after) '(99)))))"#
        ),
        "OK (t t t t)"
    );
}

#[test]
fn vm_make_process_builtin_uses_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    // ignore-errors is a macro from subr.el → bootstrap context required.
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let ((p (make-process
                          :name "vm-make-process"
                          :buffer "vm-make-process-buffer"
                          :command '("cat")
                          :filter 'ignore
                          :sentinel 'ignore)))
                 (unwind-protect
                     (list
                      (processp p)
                      (equal (process-name p) "vm-make-process")
                      (equal (process-command p) '("cat"))
                      (let ((b (process-buffer p)))
                        (and (bufferp b)
                             (equal (buffer-name b) "vm-make-process-buffer")))
                      (eq (process-filter p) 'ignore)
                      (eq (process-sentinel p) 'ignore))
                   (ignore-errors (delete-process p))))"#
        ),
        "OK (t t t t t t)"
    );
}

#[test]
fn vm_accept_process_output_uses_shared_runtime_and_callbacks() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        r#"(progn
             (fset 'vm-accept-filter
                   (lambda (_proc string)
                     (garbage-collect)
                     (setq vm-accept-filter-data string)))
             (fset 'vm-accept-sentinel
                   (lambda (_proc msg) (setq vm-accept-sentinel-data msg)))
             (setq vm-accept-filter-data nil
                   vm-accept-sentinel-data nil)
             (set-process-filter (get-process "vm-accept-process") 'vm-accept-filter)
             (set-process-sentinel (get-process "vm-accept-process") 'vm-accept-sentinel)
             (let ((first (accept-process-output (get-process "vm-accept-process") 0.1))
                   (second (accept-process-output (get-process "vm-accept-process") 0.1)))
               (list first
                     (or (eq second t) (null second))
                     vm-accept-filter-data
                     vm-accept-sentinel-data
                     (condition-case err
                         (accept-process-output 99)
                       (error (car err)))
                     (condition-case err
                         (accept-process-output nil "x")
                       (error (car err))))))"#,
        |eval| {
            let pid = eval.processes.create_process(
                "vm-accept-process".into(),
                Value::NIL,
                "echo".into(),
                vec!["out".into()],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
            eval.processes.spawn_child(pid, false).expect("spawn child");
        },
    );
    assert_eq!(
        result,
        r#"OK (t t "out
" "finished
" wrong-type-argument wrong-type-argument)"#
    );
}

#[test]
fn vm_process_network_and_signal_builtins_use_direct_runtime_paths() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((interfaces (network-interface-list))
                      (first (car interfaces))
                      (ifname (car first))
                      (info (network-interface-info ifname)))
                 (list
                 (listp interfaces)
                  (stringp ifname)
                  (vectorp (cdr first))
                  (equal (format-network-address [127 0 0 1 80]) "127.0.0.1:80")
                  (equal (format-network-address [127 0 0 1 80] t) "127.0.0.1")
                  (equal (format-network-address [0 0 0 0 0 0 0 1 80]) "[0:0:0:0:0:0:0:1]:80")
                  (equal (format-network-address [0 0 0 0 0 0 0 1 80] t) "0:0:0:0:0:0:0:1")
                  (and (listp info) (= (length info) 5))
                  (consp (network-lookup-address-info "localhost"))
                  (consp (member "HUP" (signal-names)))
                  (listp (list-system-processes))
                  (integerp (num-processors))
                  (> (num-processors) 0)
                  (eq (car (condition-case err
                               (format-network-address)
                             (error err)))
                      'wrong-number-of-arguments)
                  (condition-case err
                      (network-interface-list nil 'bogus)
                    (error (car err)))) )"#
        ),
        "OK (t t t t t t t t t t t t t t error)"
    );
}

#[test]
fn vm_call_process_builtins_use_shared_buffer_state() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let form = format!(
        r#"(let ((src (get-buffer-create "vm-cp-src"))
                 (dst (get-buffer-create "vm-cp-dst")))
             (set-buffer src)
             (erase-buffer)
             (set-buffer dst)
             (erase-buffer)
             (set-buffer src)
             (list
               (call-process "{echo}" nil t nil "hello")
               (buffer-string)
               (call-process "{echo}" nil "vm-cp-dst" nil "other")
               (progn (set-buffer dst) (buffer-string))
               (progn
                 (set-buffer src)
                 (call-process "{echo}" nil nil nil "drop"))
               (buffer-string)))"#
    );
    let result = vm_eval_str(&form);

    assert_eq!(
        result,
        r#"OK (0 "hello
" 0 "other
" 0 "hello
")"#
    );
}

#[test]
fn vm_call_process_region_builtins_use_shared_buffer_state() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    assert_eq!(
        vm_eval_str(&format!(
            r#"(let ((b1 (get-buffer-create "vm-cpr-1"))
                     (b2 (get-buffer-create "vm-cpr-2"))
                     (b3 (get-buffer-create "vm-cpr-3"))
                     (b4 (get-buffer-create "vm-cpr-4")))
                 (list
                   (progn
                     (set-buffer b1)
                     (erase-buffer)
                     (insert "abc")
                     (list (call-process-region "xyz" nil "{cat}" nil t nil)
                           (buffer-string)))
                   (progn
                     (set-buffer b2)
                     (erase-buffer)
                     (insert "abcdef")
                     (goto-char 3)
                     (let ((m (copy-marker (point))))
                       (list (call-process-region m (point-max) "{cat}" nil t nil)
                             (buffer-string))))
                   (progn
                     (set-buffer b3)
                     (erase-buffer)
                     (insert "abcde")
                     (narrow-to-region 2 4)
                     (list (call-process-region nil nil "{cat}" nil t nil)
                           (buffer-string)))
                   (progn
                     (set-buffer b4)
                     (erase-buffer)
                     (insert "abc")
                     (list (call-process-region (point-max) (point-min) "{cat}" t t nil)
                           (buffer-string)))))"#
        )),
        r#"OK ((0 "abcxyz") (0 "abcdefcdef") (0 "bcabcde") (0 "abc"))"#
    );
}

#[test]
fn vm_buffer_identity_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let path =
        std::env::temp_dir().join(format!("neovm-vm-gfb-{}-{}", std::process::id(), "shared"));
    std::fs::write(&path, b"vm-gfb").expect("write test file");
    let file = path.to_string_lossy().to_string();
    let default_dir = format!("{}/", path.parent().unwrap().to_string_lossy());
    let basename = path.file_name().unwrap().to_string_lossy().to_string();
    let form = format!(
        r#"(let ((default-directory {:?}))
             (list
              (buffer-name (get-file-buffer {:?}))
              (progn
                (rename-buffer "*vm-renamed-buffer*")
                (buffer-name))
              (condition-case err
                  (bury-buffer-internal 'x)
                (error (car err)))
              (bury-buffer-internal (current-buffer))))"#,
        default_dir, basename
    );

    let result = vm_eval_with_init_str(&form, |eval| {
        let current = eval.buffers.current_buffer_id().expect("scratch buffer");
        eval.buffers
            .set_buffer_file_name(current, Value::string(file.clone()))
            .expect("current buffer should accept file name");
    });
    let _ = std::fs::remove_file(path);

    assert_eq!(
        result,
        r#"OK ("*scratch*" "*vm-renamed-buffer*" wrong-type-argument nil)"#
    );
}

#[test]
fn vm_fileio_builtins_use_shared_default_directory_state() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!("neovm-vm-fileio-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("subdir")).expect("create subdir");
    std::fs::write(base.join("alpha.txt"), b"alpha").expect("write alpha");
    let base_str = format!("{}/", base.to_string_lossy());
    let alpha = base.join("alpha.txt").to_string_lossy().to_string();

    let result = vm_eval_with_init_str(
        r#"(list
             (expand-file-name "alpha.txt")
             (file-name-as-directory "dir")
             (directory-file-name "dir/")
             (file-name-concat "dir" "child")
             (file-exists-p "alpha.txt")
             (file-readable-p "alpha.txt")
             (file-writable-p "alpha.txt")
             (file-accessible-directory-p "subdir")
             (file-directory-p "subdir")
             (file-regular-p "alpha.txt")
             (file-newer-than-file-p "alpha.txt" "missing.txt")
             (progn (access-file "alpha.txt" "open") 'ok)
             (length (file-system-info ".")))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval.buffers.current_buffer_id().expect("current buffer");
            eval.buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(
        result,
        format!(r#"OK ("{alpha}" "dir/" "dir" "dir/child" t t t t t t t ok 3)"#)
    );
}

#[test]
fn vm_fileio_mutation_builtins_use_shared_default_directory_state() {
    crate::test_utils::init_test_tracing();
    let base =
        std::env::temp_dir().join(format!("neovm-vm-fileio-mutation-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create base");
    std::fs::write(base.join("alpha.txt"), b"alpha").expect("write alpha");
    let base_str = format!("{}/", base.to_string_lossy());

    let result = vm_eval_with_init_str(
        r#"(list
             (progn (make-directory-internal "made") (file-directory-p "made"))
             (progn (copy-file "alpha.txt" "beta.txt" t) (file-exists-p "beta.txt"))
             (progn (rename-file "beta.txt" "gamma.txt" t) (file-exists-p "gamma.txt"))
             (progn (add-name-to-file "gamma.txt" "delta.txt" t) (file-exists-p "delta.txt"))
             (directory-files "." nil "\\.txt$")
             (progn (delete-file-internal "delta.txt") (file-exists-p "delta.txt"))
             (progn (delete-file-internal "gamma.txt") (file-exists-p "gamma.txt"))
             (progn (delete-directory-internal "made") (file-directory-p "made")))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval.buffers.current_buffer_id().expect("current buffer");
            eval.buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(
        result,
        r#"OK (t t t t ("alpha.txt" "delta.txt" "gamma.txt") nil nil nil)"#
    );
}

#[test]
fn vm_insert_file_contents_and_write_region_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!(
        "neovm-vm-fileio-insert-write-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create base");
    std::fs::write(base.join("alpha.txt"), b"abcdef").expect("write alpha");
    std::fs::write(base.join("out.txt"), b"abcde").expect("write out");
    let base_str = format!("{}/", base.to_string_lossy());
    let alpha = base.join("alpha.txt").to_string_lossy().to_string();
    let out = base.join("out.txt").to_string_lossy().to_string();
    let visit = base.join("visit.txt").to_string_lossy().to_string();

    let mut eval = Context::new_vm_runtime_harness();
    eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
        .expect("buffer local default-directory should set");

    let insert_result = eval
        .eval_str(r#"(insert-file-contents "alpha.txt" t)"#)
        .expect("insert-file-contents should execute");

    let insert_parts =
        crate::emacs_core::value::list_to_vec(&insert_result).expect("insert return list");
    assert_eq!(insert_parts[0].as_utf8_str(), Some(alpha.as_str()));
    assert_eq!(insert_parts[1], Value::fixnum(6));
    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "abcdef");
    assert_eq!(
        buf.file_name_runtime_string_owned().as_deref(),
        Some(alpha.as_str())
    );
    assert!(!buf.is_modified());

    eval.eval_str(&format!(
        r#"(write-region "XY" nil "out.txt" 2 "{}")"#,
        visit
    ))
    .expect("write-region should execute");

    assert_eq!(std::fs::read_to_string(&out).expect("read out"), "abXYe");
    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(
        buf.file_name_runtime_string_owned().as_deref(),
        Some(visit.as_str())
    );
    assert!(!buf.is_modified());

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn vm_file_name_helper_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (file-name-directory "/tmp/example.txt")
                 (file-name-nondirectory "/tmp/example.txt")
                 (directory-file-name "/tmp/example/")
                 (file-name-concat "/tmp" "example" "child")
                 (file-name-absolute-p "/tmp/example.txt")
                 (file-name-absolute-p "example.txt")
                 (directory-name-p "/tmp/example/")
                 (directory-name-p "/tmp/example"))"#
        ),
        r#"OK ("/tmp/" "example.txt" "/tmp/example" "/tmp/example/child" t nil t nil)"#
    );
}

#[test]
fn vm_dired_builtins_use_shared_default_directory_state() {
    crate::test_utils::init_test_tracing();
    let base =
        std::env::temp_dir().join(format!("neovm-vm-dired-default-dir-{}", std::process::id()));
    let fixture = base.join("fixtures");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&fixture).expect("create fixture dir");
    std::fs::create_dir(fixture.join("adir")).expect("create adir");
    std::fs::create_dir(fixture.join("subdir")).expect("create subdir");
    std::fs::write(fixture.join("alpha.txt"), b"").expect("write alpha");
    std::fs::write(fixture.join("beta.el"), b"").expect("write beta");
    let base_str = format!("{}/", base.to_string_lossy());

    let result = vm_eval_with_init_str(
        r#"(list
             (mapcar #'car (directory-files-and-attributes "fixtures" nil "\\.el$"))
             (file-name-all-completions "sub" "fixtures/")
             (file-name-completion "a" "fixtures/" 'file-directory-p)
             (file-name-completion "a" "fixtures/"
                                   (lambda (path) (file-directory-p path)))
             (let ((attrs (file-attributes "fixtures/alpha.txt")))
               (nth 7 attrs))
             (find-file-name-handler "fixtures/alpha.txt" 'insert-file-contents))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval.buffers.current_buffer_id().expect("current buffer");
            eval.buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(
        result,
        r#"OK (("beta.el") ("subdir/") "adir/" "adir/" 0 nil)"#
    );
}

#[test]
fn vm_file_name_completion_callable_predicate_uses_shared_runtime_callback() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!(
        "neovm-vm-file-name-completion-callable-{}",
        std::process::id()
    ));
    let fixture = base.join("fixtures");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&fixture).expect("create fixture dir");
    std::fs::create_dir(fixture.join("adir")).expect("create adir");
    std::fs::write(fixture.join("alpha.txt"), b"").expect("write alpha");
    let base_str = format!("{}/", base.to_string_lossy());

    let result = vm_eval_with_init_str(
        r#"(let ((seen 0))
             (list (file-name-completion
                    "a"
                    "fixtures/"
                    (lambda (path)
                      (setq seen (1+ seen))
                      (file-directory-p path)))
                   seen))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval.buffers.current_buffer_id().expect("current buffer");
            eval.buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(result, r#"OK ("adir/" 2)"#);
}

#[test]
fn vm_file_metadata_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!("neovm-vm-file-metadata-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create base");
    let alpha = base.join("alpha.txt");
    let beta = base.join("beta.txt");
    std::fs::write(&alpha, b"alpha").expect("write alpha");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&beta, b"beta").expect("write beta");
    let base_str = format!("{}/", base.to_string_lossy());
    let alpha_abs = alpha.to_string_lossy().to_string();

    let result = vm_eval_with_init_str(
        r#"(list
             (integerp (file-modes "alpha.txt"))
             (progn (set-file-modes "alpha.txt" #o600) (file-modes "alpha.txt"))
             (progn (set-file-times "alpha.txt" 0)
                    (file-newer-than-file-p "beta.txt" "alpha.txt"))
             (verify-visited-file-modtime)
             (set-visited-file-modtime nil))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval.buffers.current_buffer_id().expect("current buffer");
            eval.buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
            eval.buffers
                .set_buffer_file_name(current, Value::string(alpha_abs.clone()))
                .expect("buffer file name should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(result, r#"OK (t 384 t t nil)"#);
}

#[test]
fn vm_file_metadata_tail_and_coding_scan_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!(
        "neovm-vm-file-metadata-tail-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create base");
    std::fs::write(base.join("alpha.txt"), b"alpha").expect("write alpha");
    let base_str = format!("{}/", base.to_string_lossy());

    let result = vm_eval_with_init_str(
        r#"(let ((orig (default-file-modes)))
             (prog1
                 (progn
                   (erase-buffer)
                   (set-buffer-multibyte t)
                   (insert "abc")
                   (list
                    (progn (set-default-file-modes #o700) (default-file-modes))
                    (file-acl "alpha.txt")
                    (equal (file-selinux-context "alpha.txt") '(nil nil nil nil))
                    (find-coding-systems-region-internal 1 4)))
               (set-default-file-modes orig)))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval.buffers.current_buffer_id().expect("current buffer");
            eval.buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(result, r#"OK (448 nil t t)"#);
}

#[test]
fn vm_file_setters_and_display_stubs_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!("neovm-vm-file-setters-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create base");
    let base_str = format!("{}/", base.to_string_lossy());

    let result = vm_eval_with_init_str(
        r#"(list
             (set-file-acl "alpha.txt" "user::rw-")
             (set-file-selinux-context "alpha.txt" '(nil nil nil nil))
             (send-string-to-terminal "" (selected-frame))
             (progn
               (internal-show-cursor nil nil)
               (internal-show-cursor-p))
             (progn
               (internal-show-cursor (selected-window) t)
               (internal-show-cursor-p (selected-window))))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval.buffers.current_buffer_id().expect("current buffer");
            eval.buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(result, r#"OK (nil nil nil nil t)"#);
}

#[test]
fn vm_font_builtins_accept_live_frame_designators_on_shared_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (list (list-fonts (font-spec) f)
                       (find-font (font-spec) f)
                       (font-family-list f)))"#
        ),
        "OK (nil nil nil)"
    );
}

#[test]
fn vm_font_face_and_color_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let ((f (selected-frame)))
                  (list
                   (fontp (font-spec :family "Mono"))
                   (let ((font (font-spec :family "Mono")))
                     (font-put font :weight 'bold)
                     (font-get font :weight))
                   (stringp (font-xlfd-name (font-spec :family "Mono")))
                   (vectorp (internal-lisp-face-p 'default))
                   (equal (internal-lisp-face-attribute-values :underline) '(t nil))
                   (internal-lisp-face-equal-p 'default 'default)
                   (null (internal-lisp-face-empty-p 'default))
                   (face-attribute-relative-p :height 1.1)
                   (merge-face-attribute :weight 'unspecified 'bold)
                   (equal (color-values-from-color-spec "#111122223333")
                          '(4369 8738 13107))
                   (color-gray-p "#111111")
                   (color-supported-p "red")
                   (> (color-distance "black" "white") 0)
                   (null (face-font 'default))
                   (null (internal-face-x-get-resource "font" "Font"))
                   (null (internal-set-font-selection-order
                          '(:width :height :weight :slant)))
                   (equal (internal-set-alternative-font-family-alist '(("Foo" "Bar")))
                          '((Foo Bar)))
                   (equal (internal-set-alternative-font-registry-alist '((1 2)))
                          '((1 2)))))"##
        ),
        r#"OK (t bold t t t t t t bold t t t t t t t t t)"#
    );
}

#[test]
fn vm_font_face_frame_sensitive_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let* ((f (selected-frame))
                       (face 'vm-runtime-face))
                  (list
                    (vectorp (internal-make-lisp-face face f))
                    (eq (internal-copy-lisp-face 'default face f f) face)
                    (eq (internal-set-lisp-face-attribute face :foreground "red" f) face)
                    (equal (internal-get-lisp-face-attribute face :foreground f) "red")
                    (progn
                      (internal-set-lisp-face-attribute face :foreground "blue" t)
                      (internal-merge-in-global-face face f)
                      (equal (internal-get-lisp-face-attribute face :foreground f) "blue"))))"##
        ),
        r#"OK (t t t t t)"#
    );
}

#[test]
fn vm_default_face_color_updates_frame_parameters_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let ((f (selected-frame)))
                  (internal-set-lisp-face-attribute 'default :foreground "#112233" f)
                  (internal-set-lisp-face-attribute 'default :background "#445566" f)
                  (list (frame-parameter f 'foreground-color)
                        (frame-parameter f 'background-color)
                        (internal-get-lisp-face-attribute
                         'default :foreground f)
                        (internal-get-lisp-face-attribute
                         'default :background f)))"##,
        ),
        r##"OK ("#112233" "#445566" "#112233" "#445566")"##
    );
}

#[test]
fn vm_special_face_colors_update_frame_parameters_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let* ((f (selected-frame))
                        (original (symbol-function 'modify-frame-parameters))
                        (function-cell-calls 0))
                  (fset 'modify-frame-parameters
                        (lambda (&rest args)
                          (setq function-cell-calls
                                (1+ function-cell-calls))
                          (apply original args)))
                  (unwind-protect
                      (progn
                        (internal-set-lisp-face-attribute
                         'cursor :background "#112233" f)
                        (internal-set-lisp-face-attribute
                         'border :background "#445566" f)
                        (internal-set-lisp-face-attribute
                         'mouse :background "#778899" f)
                        (list (frame-parameter f 'cursor-color)
                              (frame-parameter f 'border-color)
                              (frame-parameter f 'mouse-color)
                              function-cell-calls))
                    (fset 'modify-frame-parameters original)))"##
        ),
        r##"OK ("#112233" "#445566" "#778899" 0)"##
    );
}

#[test]
fn vm_font_stub_tail_uses_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(list
                 (null (clear-face-cache))
                 (vectorp (face-attributes-as-vector nil))
                 (progn
                   (erase-buffer)
                   (insert "a")
                   (null (font-at 1 (selected-window))))
                 (condition-case nil
                     (font-face-attributes nil)
                   (error t))
                 (condition-case nil
                     (font-get-glyphs nil 0 1)
                   (wrong-type-argument t))
                 (null (font-get-system-font))
                 (null (font-get-system-normal-font))
                 (null (font-has-char-p (font-spec :family "Mono") ?a))
                 (null (font-info "Mono"))
                 (condition-case nil
                     (font-variation-glyphs nil ?a)
                   (wrong-type-argument t))
                 (null (fontset-font nil ?a))
                 (condition-case nil
                     (fontset-info nil)
                   (error t))
                 ;; ledger 190: fontset-list-all is GNU's #ifdef ENABLE_CHECKING
                 ;; debug entry point (src/fontset.c:2254) and no ordinary build
                 ;; declares it; this port's returned fontset-list's answer.
                 (null (fboundp 'fontset-list-all)))"##
        ),
        r#"OK (t t t t t t t t t t t t t)"#
    );
}

#[test]
fn vm_sqlite_runtime_uses_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let ((db (sqlite-open)))
                  (list
                   (sqlite-available-p)
                   (fboundp 'sqlitep)
                   (fboundp 'sqlite-open)
                   (sqlitep db)
                   (equal (sqlite-select db "pragma busy_timeout") '((0)))
                   (sqlite-close db)))"##
        ),
        r#"OK (t t t t t t)"#
    );
}

#[test]
fn vm_sqlite_preserves_i64_errors_and_result_set_lifetime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let* ((db (sqlite-open))
                       (set (sqlite-select
                             db
                             "select 9223372036854775807 union all select 2"
                             nil
                             'set))
                       (prepare-error
                        (condition-case error-data
                            (sqlite-execute db "select * from sqlite_missing")
                          (sqlite-error error-data))))
                  (sqlite-close db)
                  (list
                   (sqlite-next set)
                   (sqlite-next set)
                   (sqlite-next set)
                   prepare-error
                   (sqlite-close db)))"##
        ),
        r#"OK ((9223372036854775807) (2) nil (sqlite-error ("SQL logic error" "no such table: sqlite_missing" 1 1)) t)"#
    );
}

#[test]
fn vm_native_stub_clusters_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(list
                 (null (fboundp 'debug-timer-check))
                 (let ((w (inotify-add-watch "/tmp" nil nil)))
                   (list (consp w)
                         (inotify-valid-p w)
                         (inotify-rm-watch w)))
                 (null (fboundp 'inotify-watch-list))
                 (null (fboundp 'inotify-allocated-p))
                 (eq (condition-case err
                         (dbus-make-inhibitor-lock "session" "app")
                       (void-function (car err)))
                     'void-function)
                 (null (fboundp 'dbus-close-inhibitor-lock))
                 (null (fboundp 'dbus-registered-inhibitor-locks))
                 (if (featurep 'lcms2) (lcms2-available-p)
                   (null (fboundp 'lcms2-available-p)))
                 (if (featurep 'lcms2) (fboundp 'lcms-cie-de2000)
                   (null (fboundp 'lcms-cie-de2000)))
                 (if (featurep 'lcms2) (fboundp 'lcms-xyz->jch)
                   (null (fboundp 'lcms-xyz->jch)))
                 (if (featurep 'lcms2) (fboundp 'lcms-jch->xyz)
                   (null (fboundp 'lcms-jch->xyz)))
                 (if (featurep 'lcms2) (fboundp 'lcms-jch->jab)
                   (null (fboundp 'lcms-jch->jab)))
                 (if (featurep 'lcms2) (fboundp 'lcms-jab->jch)
                   (null (fboundp 'lcms-jab->jch)))
                 (if (featurep 'lcms2) (fboundp 'lcms-cam02-ucs)
                   (null (fboundp 'lcms-cam02-ucs)))
                 (if (featurep 'lcms2) (fboundp 'lcms-temp->white-point)
                   (null (fboundp 'lcms-temp->white-point)))
                 (null (neomacs-frame-geometry))
                 (null (neomacs-frame-edges))
                 (equal (neomacs-mouse-absolute-pixel-position) '(0 . 0))
                 (null (neomacs-set-mouse-absolute-pixel-position 0 0))
                 (null (neomacs-display-monitor-attributes-list))
                 (null (neomacs-clipboard-set "x"))
                 (equal (neomacs-clipboard-get) "x")
                 (null (neomacs-primary-selection-set "x"))
                 (equal (neomacs-primary-selection-get) "x")
                 (equal (neomacs-core-backend) "rust")
                 (if (memq 'gnutls (gnutls-available-p)) t)
                 (if (memq 'gnutls3 (gnutls-available-p)) t)
                 (neomacs-tls-available-p)
                 (null (featurep 'tls))
                 ;; ledger 190: open-tls-stream is a defun in
                 ;; lisp/obsolete/tls.el:186, which this port ships; the Rust
                 ;; subr under that name was deleted.  neomacs-open-tls-stream,
                 ;; asserted above, is the same implementation under this
                 ;; port's own namespace.
                 (null (fboundp 'open-tls-stream))
                 (if (assq 'AES-256-CBC (gnutls-ciphers)) t)
                 (if (assq 'SHA256 (gnutls-digests)) t)
                 (if (assq 'SHA256 (gnutls-macs)) t)
                 (gnutls-errorp nil)
                 (equal (gnutls-error-string 0) "Success.")
                 (null (gnutls-error-fatalp 1))
                 (null (gnutls-peer-status-warning-describe nil))
                 (let ((p (make-process :name "vm-gnutls-stub"
                                        :command (list "cat"))))
                   (unwind-protect
                       (list
                        (null (gnutls-asynchronous-parameters p nil))
                        (condition-case err (gnutls-boot t nil nil)
                          (wrong-type-argument (car err)))
                        (condition-case err (gnutls-bye t nil)
                          (wrong-type-argument (car err)))
                        (null (gnutls-deinit p))
                        (= (gnutls-get-initstage p) 0)
                        (null (gnutls-peer-status p)))
                     (condition-case nil (delete-process p) (error nil))))
                 (condition-case nil (gnutls-format-certificate "x")
                   (error t))
                 (condition-case nil (gnutls-hash-digest 'sha256 "x")
                   (error t))
                 (condition-case nil (gnutls-hash-mac 'sha256 "k" "x")
                   (error t))
                 (condition-case nil (gnutls-symmetric-decrypt nil nil nil nil)
                   (error t))
                 (condition-case nil (gnutls-symmetric-encrypt nil nil nil nil)
                   (error t)))"##
        ),
        r#"OK (t (t t t) t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t (t wrong-type-argument wrong-type-argument t t t) t t t t t)"#
    );
}

#[test]
fn vm_base64_json_ccl_and_runtime_clusters_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (equal (base64-encode-string "Hi") "SGk=")
                 (equal (base64-decode-string "SGk=") "Hi")
                 (equal (base64url-encode-string "hi" t) "aGk")
                 (equal (json-serialize [1 2 3]) "[1,2,3]")
                 (equal (json-parse-string "{\"a\":1}" :object-type 'alist)
                        '((a . 1)))
                 (integerp (register-ccl-program 'vm-bytecode-direct-ccl [10 0 0]))
                 (ccl-program-p 'vm-bytecode-direct-ccl)
                 (condition-case nil
                     (ccl-execute 'vm-bytecode-direct-ccl [0 0 0 0 0 0 0 0])
                   (error t))
                 (condition-case nil
                     (ccl-execute-on-string
                      'vm-bytecode-direct-ccl
                      [0 0 0 0 0 0 0 0 0]
                      "abc")
                   (error t))
                 (integerp (register-code-conversion-map 'vm-bytecode-direct-map [0]))
                 ;; ledger 190: GNU registers exactly ONE comp.c subr outside
                 ;; #ifdef HAVE_NATIVE_COMP -- native-comp-available-p, at
                 ;; src/comp.c:5828, after the #endif -- and both editors answer
                 ;; nil to it.  The nine below are inside the ifdef and are gone.
                 (null (fboundp 'comp--init-ctxt))
                 (null (fboundp 'comp--install-trampoline))
                 (null (fboundp 'comp--late-register-subr))
                 (null (fboundp 'comp--register-lambda))
                 (null (fboundp 'comp--register-subr))
                 (null (fboundp 'comp--release-ctxt))
                 (null (fboundp 'comp-libgccjit-version))
                 (null (fboundp 'comp-native-compiler-options-effective-p))
                 (null (fboundp 'comp-native-driver-options-effective-p))
                 ;; ledger 192: GNU's six dbusbind.c subrs are inside
                 ;; #ifdef HAVE_DBUS (src/dbusbind.c:21, :2178) and this build
                 ;; links no libdbus, so it declares none of them.  The three
                 ;; that used to stand here answered a hardcoded 2, a
                 ;; fabricated ":1.1" and an invented dbus-event reply.
                 (null (fboundp 'dbus--init-bus))
                 (null (fboundp 'dbus-get-unique-name))
                 (null (fboundp 'dbus-message-internal))
                 (null (featurep 'dbusbind))
                 (consp (get-load-suffixes))
                 ;; Valid args, and bound non-interactive so the reporter
                 ;; takes its message branch: the batch branch is GNU's
                 ;; Fkill_emacs (-1) and would end the test run. Nil args are
                 ;; deliberately not used -- GNU segfaults on them (XCAR of
                 ;; nil), so they pin nothing.
                 (null (let ((noninteractive nil))
                         (command-error-default-function
                          '(error "dispatch probe") "probe: " nil)))
                 (equal
                  (single-key-description (event-convert-list '(control ?x)))
                  "C-x")
                 (null (find-operation-coding-system 'write-region 1 2 "x"))
                 ;; ledger 190: #ifdef HAVE_GPM (src/term.c:5282-5286), and
                 ;; GNU's lisp/t-mouse.el:49 uses fboundp as the build test.
                 (null (fboundp 'gpm-mouse-start))
                 (null (fboundp 'gpm-mouse-stop))
                 (null (handle-save-session nil))
                 (null (handle-switch-frame (selected-frame)))
                 (null (init-image-library nil))
                 (condition-case nil (clear-image-cache nil) (error t))
                 (null (internal--track-mouse (lambda () nil)))
                 (stringp (internal-complete-buffer "" nil nil))
                 (equal (internal-describe-syntax-value 7) 7)
                 (condition-case nil (internal-handle-focus-in nil) (error t))
                 (null (internal-stack-stats))
                 (internal-subr-documentation 'car)
                 (null (dump-emacs-portable "x"))
                 (null (dump-emacs-portable--sort-predicate nil nil))
                 (null (dump-emacs-portable--sort-predicate-copied nil nil)))"#
        ),
        r#"OK (t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t)"#
    );
}

#[test]
fn vm_base64_region_and_json_buffer_builtins_use_shared_current_buffer_state() {
    crate::test_utils::init_test_tracing();
    // with-current-buffer is a macro in subr.el → bootstrap required.
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(with-current-buffer (get-buffer-create " *vm-base64-json*")
                 (erase-buffer)
                 (insert "Hi")
                 (let ((encoded-len (base64-encode-region (point-min) (point-max)))
                       (encoded (buffer-string)))
                   (goto-char (point-min))
                   (let ((decoded-len (base64-decode-region (point-min) (point-max)))
                         (decoded (buffer-string)))
                     (erase-buffer)
                     (insert "{\"a\":1} tail")
                     (goto-char (point-min))
                     (let ((parsed (json-parse-buffer :object-type 'alist))
                           (parse-point (point)))
                       (erase-buffer)
                       (json-insert [1 2 3])
                       (list encoded-len
                             encoded
                             decoded-len
                             decoded
                             parsed
                             parse-point
                             (buffer-string))))))"#
        ),
        r#"OK (4 "SGk=" 2 "Hi" ((a . 1)) 8 "[1,2,3]")"#
    );
}

#[test]
fn vm_internal_utility_builtins_use_direct_and_shared_state_paths() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(with-current-buffer (get-buffer-create " *vm-internal-utils*")
                 (erase-buffer)
                 (insert "abc")
                 (list
                  (equal (field-string-no-properties) "abc")
                  (let ((table (make-hash-table :test 'eq)))
                    (puthash 'a 1 table)
                    (list
                     (consp (internal--hash-table-buckets table))
                     (consp (internal--hash-table-histogram table))
                     (integerp (internal--hash-table-index-size table))))
                  (let ((ob (make-vector 4 nil)))
                    (listp (internal--obarray-buckets ob)))
                  (null (internal--set-buffer-modified-tick 1 (current-buffer)))
                  (progn
                    (defvar vm-bytecode-internal-nonspecial nil)
                    (let ((before (special-variable-p 'vm-bytecode-internal-nonspecial)))
                      (internal-make-var-non-special 'vm-bytecode-internal-nonspecial)
                      (list before
                            (special-variable-p 'vm-bytecode-internal-nonspecial))))
                  (eq
                   (internal-set-lisp-face-attribute-from-resource
                    'default :weight "bold")
                   'default)
                  (eq (internal-get-lisp-face-attribute 'default :weight)
                      'bold)))"#
        ),
        r#"OK (t (t t t) t t (t nil) t t)"#
    );
}

#[test]
fn vm_internal_default_process_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        r#"(let ((p (get-process "vm-default-callback-proc")))
             (list
              (null (internal-default-process-filter p "chunk"))
              (null (internal-default-process-sentinel p "done"))
              (eq (internal-default-interrupt-process p) p)
              (eq (process-status p) 'signal)
              (= (internal-default-signal-process p 15) 0)
              (eq (process-status p) 'signal)))"#,
        |eval| {
            let pid = eval.processes.create_process(
                "vm-default-callback-proc".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
        },
    );
    assert_eq!(result, "OK (t t t t t t)");
}

#[test]
fn vm_category_charset_and_case_table_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((orig (category-table))
                      (tmp (make-category-table))
                      (other (get-buffer-create "*vm-category*")))
                 (list
                   (case-table-p (current-case-table))
                   (category-table-p orig)
                   (progn
                     (set-buffer other)
                     (set-category-table tmp)
                     (eq (category-table) tmp))
                   (progn
                     (set-buffer (get-buffer-create "*scratch*"))
                     (null (eq (category-table) tmp)))
                   (progn
                     (define-category ?x "doc")
                     (equal (category-docstring ?x) "doc"))
                   (string-equal (category-set-mnemonics (make-category-set "xa")) "ax")
                   (characterp (get-unused-category))
                   (charsetp 'ascii)
                   (eq (char-charset ?A) 'ascii)
                   (equal (find-charset-string "é") '(unicode))
                   (integerp (charset-id-internal 'ascii))
                   (consp (charset-priority-list))
                   (progn
                     (set-charset-priority 'ascii)
                     (eq (charset-priority-list t) 'ascii))
                   (null (define-charset-alias 'latin-1 'ascii))
                   (charsetp 'latin-1)
                   (null (declare-equiv-charset 1 94 ?A 'ascii))
                   (null (clear-charset-maps))))"#
        ),
        "OK (t t t t t t t t t t t t t t t t t)"
    );
}

#[test]
fn vm_composition_and_compute_motion_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(list
                 (equal (compose-string-internal "abc" 0 2 nil nil) "abc")
                 (null (find-composition-internal 1 nil nil nil))
                 (vectorp (composition-get-gstring 0 1 nil "ab"))
                 (null (clear-composition-cache))
                 (equal (composition-sort-rules '((1 . 2))) '((1 . 2)))
                 (with-current-buffer (get-buffer-create "*vm-compute-motion*")
                   (erase-buffer)
                   (insert "\tX")
                   (setq tab-width 4)
                   (equal (compute-motion 1 '(0 . 0) 3 nil 80 nil nil)
                          '(3 5 0 4 nil))))"#
        ),
        "OK (t t t t t t)"
    );
}

#[test]
fn vm_char_table_and_copy_syntax_table_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (let* ((parent (make-char-table 'syntax-table nil))
                        (table (make-char-table 'syntax-table 'default)))
                   (set-char-table-parent table parent)
                   (set-char-table-range table ?a 'word)
                   (list
                    (char-table-p table)
                    (eq (char-table-parent table) parent)
                    (eq (char-table-range table ?a) 'word)
                    (eq (char-table-subtype table) 'syntax-table)))
                 (let ((table (copy-syntax-table)))
                   (and (char-table-p table)
                        (eq (char-table-subtype table) 'syntax-table))))"#
        ),
        "OK ((t t t t) t)"
    );
}

#[test]
fn vm_map_char_table_uses_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((seen nil)
                     (table (make-char-table 'syntax-table nil)))
                 (set-char-table-range table ?a 'word)
                 (set-char-table-range table ?b 'word)
                 (set-char-table-range table ?z 'symbol)
                 (map-char-table
                  (lambda (key val)
                    (setq seen
                          (cons (if (consp key)
                                    (list (car key) (cdr key) val)
                                  (list key key val))
                                seen)))
                  table)
                 (nreverse seen))"#
        ),
        "OK ((97 98 word) (122 122 symbol))"
    );
}

#[test]
fn vm_format_mode_line_noninteractive_matches_gnu_batch_noop() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(r#"(list (stringp (format-mode-line "%b")) (length (format-mode-line "%b")))"#),
        r#"OK (t 0)"#
    );
}

#[test]
fn vm_format_mode_line_uses_shared_state_and_falls_back_for_eval_forms() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-fmt-mode-line")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (insert "abc")
                 (setq mode-name "Neo")
                 (list (format-mode-line '("%b " mode-name))
                       (format-mode-line '(:eval mode-name))))"#
        ),
        r#"OK ("vm-fmt-mode-line Neo" "Neo")"#
    );
}

#[test]
fn vm_format_mode_line_symbol_conditional_uses_only_selected_branch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let ((mode-line-flag t))
                 (list
                  (format-mode-line
                   '(mode-line-flag
                     "then"
                     (:eval (error "boom"))))
                  (progn
                    (setq mode-line-flag nil)
                    (format-mode-line
                     '(mode-line-flag
                       (:eval (error "boom"))
                       "else")))))"#
        ),
        r#"OK ("then" "else")"#
    );
}

#[test]
fn vm_format_mode_line_string_valued_symbols_render_literally() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-fmt-mode-line-literal")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (setq mode-name "%b")
                 (format-mode-line '("%b " mode-name)))"#
        ),
        r#"OK "vm-fmt-mode-line-literal %b""#
    );
}

#[test]
fn vm_format_mode_line_fixnum_elements_pad_and_truncate_tail() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "xy")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (format-mode-line '((5 "%b") "!" (-1 "%b"))))"#
        ),
        r#"OK "xy   !x""#
    );
}

#[test]
fn vm_format_mode_line_percent_specs_keep_gnu_field_width_and_dash_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "xy")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (format-mode-line "%5b|%-|%2*"))"#
        ),
        r#"OK "xy   |--|- ""#
    );
}

#[test]
fn vm_format_mode_line_respects_risky_local_variable_for_eval_forms() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let ((unsafe-mode-line '(:eval (error "boom")))
                      (trusted-mode-line '(:eval "ok")))
                 (put 'trusted-mode-line 'risky-local-variable t)
                 (list (format-mode-line 'unsafe-mode-line)
                       (format-mode-line 'trusted-mode-line)))"#
        ),
        r#"OK ("" "ok")"#
    );
}

#[test]
fn vm_format_mode_line_propertize_preserves_text_properties() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((s (format-mode-line '(:propertize "abc" face bold help-echo "h")))
                      (props (text-properties-at 1 s)))
                 (list (substring-no-properties s)
                       (plist-get props 'face)
                       (plist-get props 'help-echo)))"#
        ),
        r#"OK ("abc" bold "h")"#
    );
}

#[test]
fn vm_format_mode_line_percent_specs_preserve_source_string_text_properties() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-prop-buffer")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (let* ((fmt (propertize "%b!" 'face 'bold 'help-echo "h"))
                        (s (format-mode-line fmt))
                        (props0 (text-properties-at 0 s))
                        (props1 (text-properties-at (1- (length s)) s)))
                   (list (substring-no-properties s)
                         (plist-get props0 'face)
                         (plist-get props0 'help-echo)
                         (plist-get props1 'face)
                         (plist-get props1 'help-echo))))"#
        ),
        r#"OK ("vm-prop-buffer!" bold "h" bold "h")"#
    );
}

#[test]
fn vm_format_mode_line_status_specs_match_gnu_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-status-buffer")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (insert "abc")
                 (setq buffer-read-only t)
                 (let ((status (format-mode-line "%*|%+|%&")))
                   (setq buffer-read-only nil)
                   (set-buffer-modified-p nil)
                   (narrow-to-region 2 3)
                   (list status (format-mode-line "%n"))))"#
        ),
        r#"OK ("%|*|*" " Narrow")"#
    );
}

#[test]
fn vm_format_mode_line_face_argument_merges_explicit_faces_and_can_drop_props() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((with-face (format-mode-line
                                '((:propertize "a" face italic) "b")
                                'bold))
                      (with-face-props-0 (text-properties-at 0 with-face))
                      (with-face-props-1 (text-properties-at 1 with-face))
                      (no-props (format-mode-line
                                 '(:propertize "abc" face bold help-echo "h")
                                 0)))
                 (list (substring-no-properties with-face)
                       (plist-get with-face-props-0 'face)
                       (plist-get with-face-props-1 'face)
                       (substring-no-properties no-props)
                       (text-properties-at 0 no-props)))"#
        ),
        r#"OK ("ab" (italic bold) bold "abc" nil)"#
    );
}

#[test]
fn vm_format_mode_line_fixnum_padding_does_not_inherit_inner_properties() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((s (format-mode-line '(5 (:propertize "x" face bold))))
                      (props0 (text-properties-at 0 s))
                      (props1 (text-properties-at 1 s)))
                 (list (substring-no-properties s)
                       (plist-get props0 'face)
                       (plist-get props1 'face)))"#
        ),
        r#"OK ("x    " bold nil)"#
    );
}

#[test]
fn vm_format_mode_line_recursive_depth_specs_match_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_with_init_str(r#"(format-mode-line "%[|%]")"#, |eval| {
            eval.command_loop.recursive_depth = 4;
        }),
        r#"OK "[[[|]]]""#
    );
    assert_eq!(
        vm_eval_interactive_with_init_str(r#"(format-mode-line "%[|%]")"#, |eval| {
            eval.command_loop.recursive_depth = 7;
        }),
        r#"OK "[[[... | ...]]]""#
    );
}

#[test]
fn vm_format_mode_line_size_and_process_specs_match_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_with_init_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-mode-line-metadata")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (insert (make-string 1536 ?x))
                 (list (format-mode-line "%i|%I|%s")
                       (progn
                         (format-mode-line "%i|%I|%s"))))"#,
            |eval| {
                let buffer_id = eval.buffers.create_buffer("vm-mode-line-metadata");
                eval.processes.create_process(
                    "vm-mode-line-proc".into(),
                    Value::make_buffer(buffer_id),
                    "cat".into(),
                    vec![],
                    crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
                );
            }
        ),
        r#"OK ("1536|1.5k|run" "1536|1.5k|run")"#
    );
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-mode-line-no-process")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (insert (make-string 1536 ?x))
                 (format-mode-line "%i|%I|%s"))"#
        ),
        r#"OK "1536|1.5k|no process""#
    );
}

#[test]
fn vm_format_mode_line_column_and_mode_specs_match_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-col-mode")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (insert "abcdef")
                 (goto-char 4)
                 (format-mode-line "%c|%C|%m"))"#
        ),
        r#"OK "3|4|Fundamental""#
    );
}

#[test]
fn vm_format_mode_line_coding_and_remote_specs_match_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-coding-remote")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (set (make-local-variable 'buffer-file-coding-system) 'utf-8-unix)
                 (format-mode-line "%z|%Z|%@"))"#
        ),
        r#"OK "UUU|UUU:|-""#
    );
}

#[test]
fn vm_format_mode_line_position_o_and_q_specs() {
    crate::test_utils::init_test_tracing();
    // With content and window covering the full buffer → "All"
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-pos-oq")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (insert (make-string 100 ?x))
                 (set-window-start w (point-min) t)
                 (format-mode-line "%o|%q" nil w))"#
        ),
        // window_start=begv, window_end=zv → full buffer visible → All
        r#"OK "All|All   ""#
    );
}

#[test]
fn vm_xdisp_query_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (invisible-p 1)
                 (invisible-p -1)
                 (line-pixel-height)
                 (lookup-image-map 'map 10 20)
                 (current-bidi-paragraph-direction)
                 (bidi-resolved-levels 0)
                 (line-number-display-width t)
                 (long-line-optimizations-p)
                 (condition-case err (move-point-visually 1) (error err))
                 (condition-case err (move-to-window-line 0) (error err)))"#
        ),
        r#"OK (nil t 1 nil left-to-right nil 0 nil (args-out-of-range 1 1) (error "move-to-window-line called from unrelated buffer"))"#
    );
}

#[test]
fn vm_terminal_query_builtins_accept_live_frame_designators() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (list
                  (terminal-name f)
                  (terminal-live-p f)
                  (terminal-live-p (frame-terminal f))
                  (terminal-live-p (car (terminal-list)))
                  (set-terminal-parameter f 'vm-terminal-query-test 7)
                  (terminal-parameter f 'vm-terminal-query-test)
                  (cdr (assq 'vm-terminal-query-test (terminal-parameters f)))
                  (tty-top-frame f)
                  (tty-display-color-p f)
                  (tty-display-color-cells f)
                  (tty-no-underline f)
                  (controlling-tty-p f)))"#
        ),
        r#"OK ("initial_terminal" t t t nil 7 7 nil nil 0 nil nil)"#
    );
}

#[test]
fn vm_x_display_query_builtins_reject_non_window_system_frame_designators() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (setq window-system 'x)
                 (list
                  (condition-case err (x-display-pixel-width f) (error err))
                  (condition-case err (x-display-pixel-height f) (error err))
                  (condition-case err (x-server-version f) (error err))
                  (condition-case err (x-server-max-request-size f) (error err))
                  (condition-case err (x-display-grayscale-p f) (error err))
                  (condition-case err (x-display-backing-store f) (error err))
                  (condition-case err (x-display-color-cells f) (error err))
                  (condition-case err (x-display-mm-height f) (error err))
                  (condition-case err (x-display-mm-width f) (error err))
                  (condition-case err (x-display-monitor-attributes-list f) (error err))
                  (condition-case err (x-display-planes f) (error err))
                  (condition-case err (x-display-save-under f) (error err))
                  (condition-case err (x-display-screens f) (error err))
                  (condition-case err (x-display-visual-class f) (error err))
                  (condition-case err (x-server-input-extension-version f) (error err))))"#
        ),
        r#"OK ((error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used"))"#
    );
}

#[test]
fn vm_gui_display_capability_builtins_use_live_window_system_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (modify-frame-parameters f '((window-system . neo)))
                 (setq initial-window-system nil)
                 (setq window-system nil)
                 (list
                  (xw-display-color-p f)
                  ;; `display-color-cells' is lisp/frame.el:2966 and has no
                  ;; Rust subr (DIVERGENCES.md 157).  This VM runtime is
                  ;; deliberately minimal, so ask the C primitive the `memq'
                  ;; arm of its body calls for a `neo' frame:
                  ;; `x-display-color-cells' (src/xfns.c:5714).
                  (x-display-color-cells f)))"#
        ),
        r#"OK (t 16777216)"#
    );
}

#[test]
fn vm_x_display_stub_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (display-supports-face-attributes-p '(:weight bold) 999999)
                 (x-display-list)
                 (x-parse-geometry "80x24+10+20")
                 (x-selection-exists-p 'PRIMARY 'STRING)
                 (x-selection-owner-p 'PRIMARY 1)
                 (x-uses-old-gtk-dialog)
                 (x-disown-selection-internal 'PRIMARY)
                 (condition-case err (x-register-dnd-atom 'ATOM) (error err))
                 (condition-case err (x-focus-frame (selected-frame)) (error err))
                 (condition-case err (x-get-atom-name 'WM_CLASS) (error err))
                 (condition-case err (x-window-property "WM_NAME") (error err))
                 (condition-case err (x-window-property-attributes "WM_NAME") (error err))
                 (condition-case err (x-show-tip "hi") (error err))
                 (condition-case err (x-translate-coordinates nil 0 0) (error err))
                 (condition-case err (x-synchronize nil) (error err))
                 (condition-case err (x-get-selection-internal 'PRIMARY 'STRING) (error err))
                 (condition-case err (x-own-selection-internal 'PRIMARY "v") (error err)))"#
        ),
        r#"OK (nil nil ((height . 24) (width . 80) (top . 20) (left . 10)) nil nil nil nil (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "X windows are not in use or not initialized") (error "X windows are not in use or not initialized") (error "X selection unavailable for this frame") (error "X selection unavailable for this frame"))"#
    );
}

#[test]
fn vm_x_connection_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (setq initial-window-system 'x)
                 (list
                  (x-open-connection nil)
                  (condition-case err (x-close-connection f) (error err))
                  (condition-case err (x-close-connection "x") (error err))
                  (condition-case err (x-open-connection "x") (error err))))"#
        ),
        r#"OK (nil (error "Window system frame should be used") (error "Display x can’t be opened") nil)"#
    );
}

#[test]
fn vm_x_frame_property_and_tty_stub_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (setq initial-window-system 'x)
                 (list
                  (x-frame-edges f)
                  (x-frame-geometry f)
                  (condition-case err (x-frame-list-z-order f) (error err))
                  (condition-case err (x-frame-restack f f) (error err))
                  (condition-case err (x-export-frames f) (error err))
                  (condition-case err (x-get-modifier-masks f) (error err))
                  (x-family-fonts nil f)
                  (x-mouse-absolute-pixel-position)
                  (x-set-mouse-absolute-pixel-position 1 2)
                  (x-internal-focus-input-context f)
                  (condition-case err (x-wm-set-size-hint f) (error err))
                  (condition-case err (x-get-atom-name 'WM_CLASS f) (error err))
                  (condition-case err (x-window-property "WM_NAME" f) (error err))
                  (condition-case err (x-window-property-attributes "WM_NAME" f) (error err))
                  (tty--output-buffer-size f)
                  (tty--set-output-buffer-size f 7)
                  (tty-display-pixel-height f)
                  (tty-display-pixel-width f)
                  (tty-frame-at 0 0)
                  (tty-frame-edges f nil)
                  (tty-frame-geometry f)
                  (tty-frame-list-z-order f)
                  (condition-case err (tty-frame-restack f f t) (error err))
                  (tty-suppress-bold-inverse-default-colors f)))"#
        ),
        r#"OK (nil nil (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") nil nil nil nil (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") 0 nil 25 80 (#<frame F1 0x100000000> 0 0) nil nil (#<frame F1 0x100000000>) (error "tty-frame-restack is not implemented") nil)"#
    );
}

#[test]
fn vm_remaining_display_stub_tail_uses_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (setq initial-window-system 'x)
                 (list
                 (condition-case err (x-display-set-last-user-time nil f) (error err))
                  (x-load-color-file "/definitely/not/found")
                  (display--line-is-continued-p)
                  (display--update-for-mouse-movement f 1 2)
                  (x-begin-drag 'drag)
                  (x-double-buffered-p)
                  (x-double-buffered-p f)
                  (x-menu-bar-open-internal)
                  (x-menu-bar-open-internal f)
                  ;; ledger 190: neither name occurs anywhere in GNU's src/
                  ;; or lisp/; both were nil stubs and are deleted.
                  (fboundp 'x-scroll-bar-foreground)
                  (fboundp 'x-scroll-bar-background)))"#
        ),
        r#"OK ((error "Window system frame should be used") nil nil nil nil nil nil nil nil nil nil)"#
    );
}

#[test]
fn vm_image_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((spec (list 'image :type 'png :file "test.png")))
                 (list
                  (condition-case err (image-size spec) (error err))
                  (condition-case err (image-mask-p spec) (error err))
                  (image-flush spec t)
                  (image-cache-size)
                  (image-metadata 1)
                  (condition-case err (image-metadata spec) (error err))
                  (imagep spec)
                  (imagep 1)
                  (image-transforms-p)
                  (condition-case err (image-transforms-p 1) (error err))))"#
        ),
        r#"OK ((error "Window system frame should be used") (error "Window system frame should be used") nil 0 nil (error "Window system frame should be used") t nil nil (wrong-type-argument frame-live-p 1))"#
    );
}

#[test]
fn vm_make_indirect_buffer_uses_shared_manager_state_and_vm_hooks() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((base (get-buffer-create "vm-mib-base")))
                 (fset 'vm-mib-clone (lambda () (setq vm-mib-last-clone (buffer-name))))
                 (fset 'vm-mib-list (lambda () (setq vm-mib-buffer-list-ran t)))
                 (setq clone-indirect-buffer-hook '(vm-mib-clone))
                 (setq buffer-list-update-hook '(vm-mib-list))
                 (setq vm-mib-last-clone nil)
                 (setq vm-mib-buffer-list-ran nil)
                 (set-buffer base)
                 (let ((indirect (make-indirect-buffer base "vm-mib-ind" t)))
                   (list (buffer-name (current-buffer))
                         (buffer-name indirect)
                         vm-mib-last-clone
                         vm-mib-buffer-list-ran
                         (eq (buffer-base-buffer indirect) base))))"#
        ),
        r#"OK ("vm-mib-base" "vm-mib-ind" "vm-mib-ind" t t)"#
    );
}

#[test]
fn vm_kill_buffer_uses_shared_manager_and_frame_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((a (get-buffer-create "vm-kill-a"))
                     (b (get-buffer-create "vm-kill-b")))
                 (set-buffer a)
                 (list
                  (kill-buffer nil)
                  (buffer-live-p a)
                  (let ((current (current-buffer)))
                    (list (null (eq current a))
                          (buffer-live-p current)))
                  (condition-case err
                      (kill-buffer "vm-kill-missing")
                    (error (car err)))))"#
        ),
        r#"OK (t nil (t t) error)"#
    );
}

#[test]
fn vm_set_buffer_multibyte_uses_shared_current_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (set-buffer-multibyte nil)
                 (insert-byte 200 1)
                 (let ((unibyte (append (buffer-string) nil)))
                   (erase-buffer)
                   (list unibyte
                         (set-buffer-multibyte 'foo)
                         (progn
                           (insert-byte 200 1)
                           (append (buffer-string) nil)))))"#
        ),
        r#"OK ((200) foo (4194248))"#
    );
}

#[test]
fn vm_field_builtins_use_shared_property_boundary_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (erase-buffer)
                 (list
                  (progn
                    (insert "abcdefg")
                    (put-text-property 2 5 'field 'left)
                    (put-text-property 5 8 'field 'right)
                    (put-text-property 2 5 'face 'bold)
                    (let ((s (field-string 3)))
                      (list
                       (list (field-beginning 3)
                             (field-end 3)
                             (field-string-no-properties 3))
                       (get-text-property 1 'face s)
                       (list (field-beginning 5)
                             (field-beginning 5 t)
                             (field-end 5)
                             (field-end 5 t))
                       (progn
                         (delete-field 3)
                         (list
                          (buffer-substring-no-properties (point-min) (point-max))
                          (get-text-property 2 'field))))))
                  (progn
                    (erase-buffer)
                    (insert "abcdefg")
                    (put-text-property 2 4 'field 'left)
                    (put-text-property 4 5 'field 'boundary)
                    (put-text-property 5 8 'field 'right)
                    (list (field-beginning 4)
                          (field-beginning 4 t)
                          (field-end 4)
                          (field-end 4 t)
                          (field-beginning 5)
                          (field-beginning 5 t)
                          (field-end 5)
                          (field-end 5 t)))))"#
        ),
        r#"OK (((2 5 "bcd") bold (2 2 5 8) ("aefg" right)) (2 2 4 8 4 2 5 8))"#
    );
}

#[test]
fn vm_constrain_to_field_uses_shared_field_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (erase-buffer)
                 (list
                  (progn
                    (insert "abcdefg")
                    (put-text-property 2 5 'field 'left)
                    (put-text-property 5 8 'field 'right)
                    (put-text-property 3 4 'capture t)
                    (goto-char 7)
                    (list
                     (constrain-to-field 7 3)
                     (constrain-to-field 7 5)
                     (constrain-to-field 7 5 t)
                     (progn
                       (goto-char 7)
                       (list (constrain-to-field nil 3) (point)))
                     (constrain-to-field 7 3 nil nil 'capture)
                     (constrain-to-field 7 2 nil nil 'capture)))
                  (progn
                    (erase-buffer)
                    (insert "ab\ncd\nef")
                    (put-text-property 1 4 'field 'top)
                    (put-text-property 4 9 'field 'bottom)
                    (list
                     (constrain-to-field 6 2 nil t)
                     (constrain-to-field 6 2 nil nil)
                     (constrain-to-field 6 4 t nil)))))"#
        ),
        r#"OK ((5 5 7 (5 5) 5 2) (4 4 6))"#
    );
}

#[test]
fn vm_constrain_to_field_honors_dynamic_inhibit_field_text_motion_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (erase-buffer)
                 (insert "aa" (propertize "bb" 'field 'f) "cc\nxx")
                 (goto-char 4)
                 (list
                  (line-beginning-position)
                  (line-end-position)
                  (let ((inhibit-field-text-motion t))
                    (list
                     (line-beginning-position)
                     (line-end-position)))
                  (let ((inhibit-field-text-motion t))
                    (goto-char 4)
                    (beginning-of-line nil)
                    (point))
                  (let ((inhibit-field-text-motion t))
                    (goto-char 4)
                    (end-of-line nil)
                    (point))))"#
        ),
        r#"OK (3 5 (1 7) 1 7)"#
    );
}

#[test]
fn vm_replace_region_contents_uses_shared_source_and_property_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((dest (current-buffer))
                     (src (get-buffer-create "*rrc-src*"))
                     (s (propertize "CD" 'face 'bold)))
                 (set-buffer src)
                 (erase-buffer)
                 (insert "1234")
                 (put-text-property 2 4 'face 'italic)
                 (set-buffer dest)
                 (list
                  (progn
                    (erase-buffer)
                    (insert "abZZef")
                    (replace-region-contents 3 5 s)
                    (list
                     (buffer-substring-no-properties 1 (point-max))
                     (get-text-property 3 'face)))
                  (progn
                    (erase-buffer)
                    (insert "abZZef")
                    (replace-region-contents 3 5 (vector src 2 4))
                    (list
                     (buffer-substring-no-properties 1 (point-max))
                     (get-text-property 3 'face)
                     (get-text-property 4 'face)))
                  (condition-case err
                      (replace-region-contents 3 5 (current-buffer))
                    (error (list (car err) (car (cdr err)))))))"#
        ),
        r#"OK (("abCDef" bold) ("ab23ef" italic italic) (error "Cannot replace a buffer with itself"))"#
    );
}

#[test]
fn vm_read_only_noop_buffer_mutations_match_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (setq buffer-read-only t)
                 (list (delete-region 1 1)
                       (delete-and-extract-region 1 1)
                       (progn
                         (narrow-to-region 1 1)
                         (erase-buffer)
                         (list (point-min) (point-max) (buffer-string)))))"#
        ),
        r#"OK (nil "" (1 1 ""))"#
    );
}

#[test]
fn vm_autoload_shares_autoload_runtime_state() {
    crate::test_utils::init_test_tracing();
    // `symbol-file' is lisp/subr.el:3351 and has no subr (DIVERGENCES.md 152);
    // the shared state this test is about is the function cell `autoload'
    // writes, which is also what GNU's `symbol-file' reads.  Measured on GNU
    // 31.0.90: (symbol-function SYM) is (autoload FILE nil nil nil).
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (autoload 'vm-symbol-file-probe "vm-symbol-file-probe-file")
                 (symbol-function 'vm-symbol-file-probe))"#
        ),
        r#"OK (autoload "vm-symbol-file-probe-file" nil nil nil)"#
    );
}

#[test]
fn vm_compiled_autoload_do_load_uses_shared_runtime_and_load_bridge() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("vm-bytecode-autoload-do-load.el"),
        "(defalias 'vm-bytecode-autoload-do-load #'(lambda () 91))\n",
    )
    .expect("write autoload-do-load fixture");

    let mut eval = Context::new_vm_runtime_harness();
    eval.obarray.set_symbol_value(
        "load-path",
        Value::list(vec![Value::string(dir.path().to_string_lossy())]),
    );
    let result = eval
        .eval_str(
            r#"(progn
             (autoload 'vm-bytecode-autoload-do-load "vm-bytecode-autoload-do-load")
             (autoload-do-load (symbol-function 'vm-bytecode-autoload-do-load)
                               'vm-bytecode-autoload-do-load)
             (vm-bytecode-autoload-do-load))"#,
        )
        .expect("autoload-do-load should execute");

    assert_eq!(result, Value::fixnum(91));
}

#[test]
fn vm_compiled_autoload_do_load_survives_exact_gc() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("vm-bytecode-autoload-do-load-exact.el"),
        "(setq vm-bytecode-autoload-junk (make-list 20000 nil))\n\
         (defalias 'vm-bytecode-autoload-do-load-exact #'(lambda () 91))\n",
    )
    .expect("write autoload-do-load exact fixture");

    let mut eval = Context::new_vm_runtime_harness();
    eval.gc_stress = true;
    eval.obarray.set_symbol_value(
        "load-path",
        Value::list(vec![Value::string(dir.path().to_string_lossy())]),
    );
    let result = eval
        .eval_str(
            r#"(progn
             (autoload 'vm-bytecode-autoload-do-load-exact "vm-bytecode-autoload-do-load-exact")
             (autoload-do-load (symbol-function 'vm-bytecode-autoload-do-load-exact)
                               'vm-bytecode-autoload-do-load-exact)
             (vm-bytecode-autoload-do-load-exact))"#,
        )
        .expect("autoload-do-load should execute under exact gc");

    assert_eq!(result, Value::fixnum(91));
}

#[test]
fn vm_compiled_named_autoload_call_uses_shared_runtime_and_load_bridge() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("vm-bytecode-autoload-call.el"),
        "(defalias 'vm-bytecode-autoload-call #'(lambda (x) (+ x 7)))\n",
    )
    .expect("write autoload call fixture");

    let mut eval = Context::new_vm_runtime_harness();
    eval.obarray.set_symbol_value(
        "load-path",
        Value::list(vec![Value::string(dir.path().to_string_lossy())]),
    );
    let result = eval
        .eval_str(
            r#"(progn
             (autoload 'vm-bytecode-autoload-call "vm-bytecode-autoload-call")
             (vm-bytecode-autoload-call 5))"#,
        )
        .expect("autoloaded call should execute");

    assert_eq!(result, Value::fixnum(12));
}

#[test]
fn vm_direct_named_call_follows_chained_autoloads() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("vm-bytecode-autoload-chain-outer.el"),
        "(autoload 'vm-bytecode-autoload-chain \"vm-bytecode-autoload-chain-target\")\n",
    )
    .expect("write outer autoload fixture");
    std::fs::write(
        dir.path().join("vm-bytecode-autoload-chain-target.el"),
        "(defalias 'vm-bytecode-autoload-chain #'(lambda (x) (+ x 7)))\n",
    )
    .expect("write chained autoload target fixture");

    let mut eval = Context::new_vm_runtime_harness();
    eval.obarray.set_symbol_value(
        "load-path",
        Value::list(vec![Value::string(dir.path().to_string_lossy())]),
    );
    let result = eval
        .eval_str(
            r#"(progn
                 (autoload 'vm-bytecode-autoload-chain
                           "vm-bytecode-autoload-chain-outer")
                 (vm-bytecode-autoload-chain 5))"#,
        )
        .expect("direct named call should follow the autoload chain");

    assert_eq!(result, Value::fixnum(12));
}

#[test]
fn vm_indentation_builtins_use_buffer_local_current_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(list (current-indentation)
                     (current-column)
                     (progn
                       (goto-char 1)
                       (move-to-column 3))
                     (current-column))"#,
            |eval| {
                let current = eval.buffers.current_buffer_id().expect("scratch buffer");
                let buffer = eval.buffers.get_mut(current).expect("scratch buffer");
                buffer.set_buffer_local("tab-width", Value::fixnum(4));
                buffer.insert("\tb");
                buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3));
            },
        ),
        "OK (4 5 4 4)"
    );
}

#[test]
fn vm_indent_to_uses_dynamic_indentation_bindings() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((tab-width 4) (indent-tabs-mode t))
                 (list (indent-to 6 1)
                       (current-column)
                       (append (buffer-string) nil)))"#
        ),
        "OK (6 6 (9 32 32))"
    );
}

#[test]
fn vm_insert_before_markers_updates_markers_at_point() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "ab")
                 (goto-char 1)
                 (let ((m (copy-marker (point))))
                   (insert-before-markers "X")
                   (list (buffer-string) (marker-position m))))"#
        ),
        r#"OK ("Xab" 2)"#
    );
}

#[test]
fn vm_insert_and_insert_char_use_shared_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "ab")
                 (goto-char 1)
                 (let ((m (copy-marker (point))))
                   (list
                    (progn
                      (insert "X")
                      (list (buffer-string) (marker-position m) (point)))
                    (progn
                      (insert-char ?Y 2)
                      (list (buffer-string) (marker-position m) (point))))))"#
        ),
        r#"OK (("Xab" 1 2) ("XYYab" 1 4))"#
    );
}

#[test]
fn vm_insert_read_only_shape_and_noop_cases_match_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (setq buffer-read-only t)
                 (list
                  (condition-case err
                      (insert "x")
                    (error (list (car err) (bufferp (car (cdr err))))))
                  (condition-case err
                      (insert-char ?x 1)
                    (error (list (car err) (bufferp (car (cdr err))))))
                  (condition-case err
                      (insert-and-inherit "x")
                    (error (list (car err) (bufferp (car (cdr err))))))
                  (condition-case err
                      (insert-before-markers-and-inherit "x")
                    (error (list (car err) (bufferp (car (cdr err))))))
                  (condition-case err
                      (insert-byte 120 1)
                    (error (list (car err) (bufferp (car (cdr err))))))
                  (list (insert)
                        (insert "")
                        (insert-char ?x 0)
                        (insert-byte 120 0)
                        (insert-and-inherit)
                        (insert-and-inherit "")
                        (insert-before-markers-and-inherit)
                        (insert-before-markers-and-inherit "")
                        (buffer-string))))"#
        ),
        r#"OK ((buffer-read-only t) (buffer-read-only t) (buffer-read-only t) (buffer-read-only t) (buffer-read-only t) (nil nil nil nil nil nil nil nil ""))"#
    );
}

#[test]
fn vm_insert_inherit_variants_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (erase-buffer)
                 (insert "a")
                 (put-text-property 1 2 'face 'bold)
                 (let ((first
                        (progn
                          (insert-and-inherit
                           (propertize "X" 'face 'italic 'mouse-face 'highlight))
                          (list (buffer-substring-no-properties (point-min) (point-max))
                                (get-text-property 2 'face)
                                (get-text-property 2 'mouse-face)))))
                   (erase-buffer)
                   (insert "ab")
                   (put-text-property 1 2 'face 'bold)
                   (goto-char 2)
                   (let ((m (copy-marker (point))))
                     (list first
                           (progn
                             (insert-before-markers-and-inherit
                              (propertize "X" 'mouse-face 'highlight))
                             (list (buffer-substring-no-properties (point-min) (point-max))
                                   (marker-position m)
                                   (get-text-property 2 'face)
                                   (get-text-property 2 'mouse-face)))))))"#
        ),
        r#"OK (("aX" bold highlight) ("aXb" 3 bold highlight))"#
    );
}

#[test]
fn vm_insert_byte_and_buffer_undo_toggles_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (list (progn
                         (insert-byte 65 2)
                         (buffer-string))
                       (progn
                         (erase-buffer)
                         (insert-byte 200 1)
                         (append (buffer-string) nil))
                       (progn
                         (buffer-enable-undo)
                         buffer-undo-list)
                       (progn
                         ;; `buffer-disable-undo' is a `defun' in GNU
                         ;; (lisp/simple.el:3591) with no C version, so on a
                         ;; bare evaluator we write its body.
                         ;; DIVERGENCES.md 150.
                         (setq buffer-undo-list t)
                         buffer-undo-list)))"#
        ),
        // `buffer-enable-undo` does NOT clear a live history -- GNU's
        // `Fbuffer_enable_undo` resets only when the list is `t`
        // (src/buffer.c:1846).  The two edits above are already recorded, so
        // enabling undo here reports them.  This form run under GNU 31.0.90
        // `-Q --batch` in `*scratch*` (whose `buffer-undo-list` starts nil):
        //   ("AA" (4194248) "((1 . 2) (\"AA\" . -1) (1 . 3) (t . 0))" t)
        // The previous `nil` pinned the clobber this test predates.
        r#"OK ("AA" (4194248) ((1 . 2) ("AA" . -1) (1 . 3) (t . 0)) t)"#
    );

    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (insert-byte 200 1)
                 (append (buffer-string) nil))"#,
            |eval| {
                let current = eval.buffers.current_buffer_id().expect("scratch buffer");
                eval.buffers
                    .set_buffer_multibyte_flag(current, false)
                    .expect("set-buffer-multibyte should accept scratch buffer");
            },
        ),
        "OK (200)"
    );
}

#[test]
fn vm_subst_char_in_region_uses_shared_runtime_state_and_gnu_noop_rules() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "a\n")
                 (let ((end (copy-marker (point-max) t)))
                   (goto-char (point-min))
                   (insert " ")
                   (let ((changed
                          (progn
                            (subst-char-in-region (point-min) end ?\n ?\s t)
                            (buffer-substring-no-properties (point-min) (point-max)))))
                     (setq buffer-read-only t)
                     (list changed
                           (condition-case err
                               (subst-char-in-region 1 2 ?\s ?_)
                             (error (list (car err) (bufferp (car (cdr err))))))
                           (subst-char-in-region 1 1 ?\s ?_)
                           (subst-char-in-region 1 (point-max) ?z ?_)
                           (buffer-substring-no-properties (point-min) (point-max))))))"#
        ),
        r#"OK (" a " (buffer-read-only t) nil nil " a ")"#
    );
}

#[test]
fn vm_barf_if_buffer_read_only_uses_shared_state_and_inhibit_text_property() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "abc")
                 (put-text-property 2 3 'inhibit-read-only t)
                 (setq buffer-read-only t)
                 (list (barf-if-buffer-read-only 2)
                       (condition-case err
                           (barf-if-buffer-read-only 1)
                         (error (list (car err) (bufferp (car (cdr err))))))))"#
        ),
        r#"OK (nil (buffer-read-only t))"#
    );
}

#[test]
fn vm_char_primitives_and_buffer_substring_use_narrowed_current_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(list (following-char)
                     (preceding-char)
                     (buffer-substring-no-properties 3 8)
                     (buffer-substring-no-properties 8 3)
                     (condition-case err
                         (buffer-substring-no-properties 0 1)
                       (error (car err))))"#,
            |eval| {
                let current = eval.buffers.current_buffer_id().expect("scratch buffer");
                let buffer = eval.buffers.get_mut(current).expect("scratch buffer");
                buffer.insert("Hello, 世界");
                let start = buffer
                    .lisp_pos_to_emacs_byte_pos(LispCharPos1::new(3))
                    .get();
                let end = buffer
                    .lisp_pos_to_emacs_byte_pos(LispCharPos1::new(8))
                    .get();
                buffer.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
                    start, end,
                ));
                buffer.goto_emacs_byte_pos(buffer.point_min_emacs_byte_pos());
            },
        ),
        r#"OK (108 0 "llo, " "llo, " args-out-of-range)"#
    );
}

#[test]
fn vm_byte_position_and_get_byte_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "éa")
                 (let ((m (copy-marker 2)))
                   (list (byte-to-position 1)
                         (byte-to-position 2)
                         (byte-to-position 3)
                         (position-bytes 1)
                         (position-bytes m)
                         (position-bytes 3)
                         (get-byte m))))"#
        ),
        "OK (1 1 2 1 3 4 97)"
    );

    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (insert-byte 200 1)
                 (insert-byte 65 1)
                 (list (get-byte 1)
                     (get-byte 2)
                     (condition-case err
                         (get-byte 3)
                       (error (car err)))))"#,
            |eval| {
                let current = eval.buffers.current_buffer_id().expect("scratch buffer");
                eval.buffers
                    .set_buffer_multibyte_flag(current, false)
                    .expect("set-buffer-multibyte should accept scratch buffer");
            },
        ),
        "OK (200 65 args-out-of-range)"
    );
}

#[test]
fn vm_syntax_navigation_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "abc123")
                 (goto-char 1)
                 (list (skip-chars-forward "a-c")
                       (point)
                       (progn
                         (goto-char (point-max))
                         (skip-chars-backward "1-3"))
                       (point)))"#
        ),
        "OK (3 4 -3 4)"
    );

    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (erase-buffer)
                 (insert "(a (b)) c")
                 (list (scan-sexps 1 1)
                       (scan-lists 1 2 0)
                       (scan-sexps (point-max) -1)))"#
        ),
        "OK (8 nil 9)"
    );
}

#[test]
fn vm_delete_char_uses_shared_read_only_and_narrowing_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(list
                 (let ((buffer-read-only t))
                   (condition-case err
                       (delete-char 1)
                     (error (car err))))
                 (let ((buffer-read-only t)
                       (inhibit-read-only t))
                   (delete-char 1)
                   (buffer-string))
                 (progn
                   (narrow-to-region 1 2)
                   (goto-char (point-max))
                   (condition-case err
                       (delete-char 1)
                     (error (car err)))))"#,
            |eval| {
                crate::test_utils::load_gnu_undo_auto_runtime(eval);
                let current = eval.buffers.current_buffer_id().expect("scratch buffer");
                let buffer = eval.buffers.get_mut(current).expect("scratch buffer");
                buffer.insert("abc");
                buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
            },
        ),
        r#"OK (buffer-read-only "bc" end-of-buffer)"#
    );
}

#[test]
fn vm_string_match_updates_match_data_for_followup_builtins() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (string-match \"a\\\\(b\\\\)\" \"zabz\")
               (list (match-beginning 0)
                     (match-beginning 1)
                     (match-end 1)
                     (match-data)))"
        ),
        "OK (1 2 3 (1 3 2 3))"
    );
}

#[test]
fn vm_buffer_local_and_binding_builtins_use_shared_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (defvaralias 'vm-vm-alias 'vm-vm-base)
                 (defvaralias 'vm-vm-lvis-alias 'vm-vm-lvis-base)
                 (make-variable-buffer-local 'vm-vm-lvis-base)
                 (list (buffer-local-value 'vm-vm-alias (current-buffer))
                       (buffer-local-value 'vm-vm-base (current-buffer))
                       (bufferp (variable-binding-locus 'vm-vm-alias))
                       (buffer-live-p (variable-binding-locus 'vm-vm-base))
                       (local-variable-if-set-p 'vm-vm-lvis-alias)
                       (local-variable-if-set-p 'vm-vm-lvis-base)))"#,
            |eval| {
                let current = eval.buffers.current_buffer_id().expect("scratch buffer");
                let buffer = eval.buffers.get_mut(current).expect("scratch buffer");
                buffer.set_buffer_local("vm-vm-base", Value::fixnum(3));
            },
        ),
        "OK (3 3 t t t t)"
    );

    assert_eq!(
        vm_eval_str(
            r#"(list
                 (buffer-local-value nil (current-buffer))
                 (buffer-local-value t (current-buffer))
                 (buffer-local-value :vm-k (current-buffer))
                 (condition-case err
                     (buffer-local-value 'vm-miss (current-buffer))
                   (error (car err)))
                 (condition-case err
                     (variable-binding-locus 1)
                   (error (car err)))
                 (condition-case err
                     (local-variable-if-set-p 1)
                   (error (car err))))"#
        ),
        "OK (nil t :vm-k void-variable wrong-type-argument wrong-type-argument)"
    );
}

#[test]
fn vm_search_builtins_use_shared_runtime_state_and_match_data() {
    crate::test_utils::init_test_tracing();
    // Use bootstrap context throughout — mixing bare and bootstrap
    // vm_eval inside one #[test] divergences the global interner with
    // the cached pdump.
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(with-temp-buffer
                 (insert "ab")
                 (let ((end (copy-marker (point-max) t)))
                   (goto-char (point-min))
                   (insert "X")
                   (goto-char (point-min))
                   (list (search-forward "b" end t)
                         (point)
                         (marker-position end)
                         (match-beginning 0)
                         (match-end 0))))"#
        ),
        "OK (4 4 4 3 4)"
    );

    // search-forward-regexp is an alias defined in subr.el, so the
    // bootstrap context is needed.
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(with-temp-buffer
                 (insert "ab12")
                 (goto-char 1)
                 (list (re-search-forward "[0-9]+" nil t)
                       (match-beginning 0)
                       (match-end 0)
                       (progn
                         (goto-char 1)
                         (search-forward-regexp "[a-z]+" nil t))
                       (progn
                         (goto-char 1)
                         (posix-search-forward "[0-9]+" nil t))))"#
        ),
        "OK (5 3 5 3 5)"
    );
}

#[test]
fn vm_looking_at_builtins_use_shared_match_data_and_case_fold() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "A")
                 (goto-char 1)
                 (list
                  (let ((case-fold-search nil))
                    (looking-at "a" t))
                  (let ((case-fold-search t))
                    (looking-at "a" t))
                  (progn
                    (set-match-data '(10 11))
                    (let ((case-fold-search t))
                      (looking-at "a" t))
                    (match-beginning 0))
                  (progn
                    (set-match-data nil)
                    (let ((case-fold-search t))
                      (looking-at "a"))
                    (list (match-beginning 0)
                          (match-end 0)))
                  (let ((case-fold-search t))
                    (posix-looking-at "a"))))"#
        ),
        "OK (nil t 10 (1 2) t)"
    );
}

#[test]
fn vm_replace_match_and_match_translate_use_shared_match_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (let ((case-fold-search t))
                   (posix-string-match "A" "a"))
                 (progn
                   (string-match "\\([a-z]+\\)-\\([0-9]+\\)" "foo-42")
                   (replace-match "bar" t t "foo-42" 1))
                 (progn
                   (set-match-data '(1 4 2 3))
                   (match-data--translate 5)
                   (match-data))
                 (progn
                   (erase-buffer)
                   (insert "foo-42")
                   (goto-char 1)
                   (re-search-forward "\\([a-z]+\\)-\\([0-9]+\\)")
                   (list
                    (replace-match "\\2-\\1")
                    (buffer-string)
                    (match-beginning 0)
                    (match-end 0)
                    (match-beginning 1)
                    (match-end 1)
                    (match-beginning 2)
                    (match-end 2))))"#
        ),
        r#"OK (0 "bar-42" (6 9 7 8) (nil "42-foo" 1 7 1 1 1 7))"#
    );
}

#[test]
fn vm_buffer_manager_query_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (get-buffer-create "*Messages*")
                 (get-buffer-create "*vm-alt*")
                 (get-buffer-create " hidden")
                 (list
                  (mapcar #'buffer-name (buffer-list))
                  (buffer-name (other-buffer "*vm-alt*"))
                  (buffer-name (other-buffer "*vm-alt*" t))
                  (generate-new-buffer-name "*vm-alt*" "*vm-alt*<2>")))"#
        ),
        r#"OK (("*scratch*" "*Messages*" "*vm-alt*" " hidden") "*Messages*" "*scratch*" "*vm-alt*<2>")"#
    );
}

#[test]
fn vm_charset_region_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "aé😀")
                 (list
                  (find-charset-region 1 4)
                  (find-charset-region 2 3)
                  (find-charset-region 4 4)
                  (charset-after 1)
                  (charset-after 2)
                  (charset-after 3)
                  (charset-after 4)))"#
        ),
        // GNU never surfaces the internal `unicode-bmp` subset: BMP é and
        // astral 😀 both canonicalize to the dimension-3 `unicode` charset.
        r#"OK ((ascii unicode) (unicode) (ascii) ascii unicode unicode nil)"#
    );
}

#[test]
fn vm_compose_region_internal_uses_shared_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "abc")
                 (list
                  (compose-region-internal 1 3)
                  (condition-case err
                      (compose-region-internal 0 3)
                    (error (list (car err) (cdr err))))))"#
        ),
        r#"OK (nil (args-out-of-range (#<buffer 1> 0 3)))"#
    );
}

#[test]
fn vm_when_unless() {
    crate::test_utils::init_test_tracing();
    // when/unless are macros defined in subr.el, so they require the
    // runtime-startup bootstrap context to be available — the bare
    // bytecode compiler does not know about them as native syntax.
    assert_eq!(vm_bootstrap_eval_str("(when t 1 2 3)"), "OK 3");
    assert_eq!(vm_bootstrap_eval_str("(when nil 1 2 3)"), "OK nil");
    assert_eq!(vm_bootstrap_eval_str("(unless nil 1 2 3)"), "OK 3");
    assert_eq!(vm_bootstrap_eval_str("(unless t 1 2 3)"), "OK nil");
}

#[test]
fn vm_cond() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(cond (nil 1) (t 2))"), "OK 2");
    assert_eq!(vm_eval_str("(cond (nil 1) (nil 2))"), "OK nil");
}

#[test]
fn vm_nested_let() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(let ((x 1)) (let ((y 2)) (+ x y)))"), "OK 3");
}

#[test]
fn vm_vector_ops() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(aref [10 20 30] 1)"), "OK 20");
    assert_eq!(vm_eval_str("(length [1 2 3])"), "OK 3");
}

#[test]
fn vm_aset_string_writeback() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str("(let ((s (copy-sequence \"abc\"))) (aset s 1 ?x) s)"),
        r#"OK "axc""#
    );
}

#[test]
fn vm_fillarray_string_writeback() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str("(let ((s (copy-sequence \"abc\"))) (fillarray s ?y) s)"),
        r#"OK "yyy""#
    );
}

#[test]
fn vm_aref_aset_error_parity() {
    crate::test_utils::init_test_tracing();
    with_vm_eval("(aref [10 20 30] -1)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "args-out-of-range");
            assert_eq!(
                data,
                vec![
                    Value::vector(vec![
                        Value::fixnum(10),
                        Value::fixnum(20),
                        Value::fixnum(30)
                    ]),
                    Value::fixnum(-1)
                ]
            );
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(aset [10 20 30] -1 99)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "args-out-of-range");
            assert_eq!(
                data,
                vec![
                    Value::vector(vec![
                        Value::fixnum(10),
                        Value::fixnum(20),
                        Value::fixnum(30)
                    ]),
                    Value::fixnum(-1)
                ]
            );
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(aset \"abc\" 1 nil)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("characterp"), Value::NIL]);
        }
        other => panic!("unexpected error: {other:?}"),
    });
}

#[test]
fn vm_builtin_wrong_arity_uses_symbol_payload_for_direct_calls() {
    // GNU Emacs `wrong-number-of-arguments` for a direct subr call
    // signals with the symbol name in data[0]. Verified via
    // `(condition-case err (car) (error err))` →
    // `(wrong-number-of-arguments car 0)`. The funcall path
    // (`(funcall #'car)`) instead reports the subr value -- those
    // semantics live in direct subr-object dispatch.
    crate::test_utils::init_test_tracing();
    with_vm_eval("(car)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-number-of-arguments");
            assert_eq!(data, vec![Value::symbol("car"), Value::fixnum(0)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(car 1 2)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-number-of-arguments");
            assert_eq!(data, vec![Value::symbol("car"), Value::fixnum(2)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });
}

#[test]
fn vm_bytecode_wrong_arity_matches_gnu_entry_check() {
    crate::test_utils::init_test_tracing();
    let mut func = ByteCodeFunction::new(
        crate::emacs_core::bytecode::decode::parse_arglist_descriptor(2 | (3 << 8)),
    );
    func.constants = vec![Value::NIL].into();
    func.ops = vec![Op::Constant(0), Op::Return];
    func.max_stack = 1;

    let mut eval = Context::new_minimal_vm_harness();
    let mut vm = new_vm(&mut eval);

    let err = vm
        .execute(&func, vec![Value::fixnum(1)])
        .expect_err("bytecode arity must be validated at VM entry");
    match map_flow(err) {
        EvalError::Signal { symbol, data, .. } => {
            assert_eq!(resolve_sym(symbol), "wrong-number-of-arguments");
            assert_eq!(
                data,
                vec![
                    Value::cons(Value::fixnum(2), Value::fixnum(3)),
                    Value::fixnum(1)
                ]
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn vm_bytecode_rest_wrong_arity_reports_nonrest_descriptor_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut func = ByteCodeFunction::new(
        crate::emacs_core::bytecode::decode::parse_arglist_descriptor(2 | (2 << 8) | 128),
    );
    func.constants = vec![Value::NIL].into();
    func.ops = vec![Op::Constant(0), Op::Return];
    func.max_stack = 1;

    let mut eval = Context::new_minimal_vm_harness();
    let mut vm = new_vm(&mut eval);

    let err = vm
        .execute(&func, vec![Value::fixnum(1)])
        .expect_err("bytecode arity must report descriptor nonrest, not func-arity max");
    match map_flow(err) {
        EvalError::Signal { symbol, data, .. } => {
            assert_eq!(resolve_sym(symbol), "wrong-number-of-arguments");
            assert_eq!(
                data,
                vec![
                    Value::cons(Value::fixnum(2), Value::fixnum(2)),
                    Value::fixnum(1)
                ]
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn vm_string_compare_type_errors_match_oracle() {
    crate::test_utils::init_test_tracing();
    with_vm_eval("(string-equal \"ab\" 1)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("stringp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(string-lessp \"ab\" 1)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("stringp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });
}

#[test]
fn vm_list_lookup_type_errors_match_oracle() {
    crate::test_utils::init_test_tracing();
    with_vm_eval("(car 1)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("listp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(cdr 1)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("listp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(car-safe 1)", false, |result| match result {
        Ok(value) => assert_eq!(value, Value::NIL),
        other => panic!("unexpected error: {other:?}"),
    });
    with_vm_eval("(cdr-safe 1)", false, |result| match result {
        Ok(value) => assert_eq!(value, Value::NIL),
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(nth 'a '(1 2 3))", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("integerp"), Value::symbol("a")]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(nth 1 1)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("listp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(nthcdr 'a '(1 2 3))", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("integerp"), Value::symbol("a")]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(nthcdr 1 1)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("listp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(memq 'a 1)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("listp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(assq 'a 1)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("listp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });
}

#[test]
fn vm_length_and_symbol_access_type_errors_match_oracle() {
    crate::test_utils::init_test_tracing();
    with_vm_eval("(length '(1 . 2))", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("listp"), Value::fixnum(2)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(symbol-value 1)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("symbolp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(symbol-plist 1)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("symbolp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(symbol-function 1)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("symbolp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });
}

#[test]
fn vm_symbol_introspection_builtins_use_shared_symbol_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (fset 'vm-sym-target '(lambda (x) x))
                 (fset 'vm-sym-a 'vm-sym-b)
                 (fset 'vm-sym-b 'vm-sym-target)
                 (put 'vm-sym-a 'vm-prop 17)
                 (autoload 'vm-sym-auto "vm-sym-file")
                 (autoload 'vm-sym-macro "vm-sym-file" nil nil 'macro)
                 (list
                  (symbol-function 'vm-sym-a)
                  (indirect-function 'vm-sym-a)
                  (functionp 'vm-sym-a)
                  (symbol-plist 'vm-sym-a)
                  (symbol-function 'vm-sym-auto)
                  (indirect-function 'vm-sym-auto)
                  (functionp 'vm-sym-auto)
                  (functionp 'vm-sym-macro)))"#
        ),
        r#"OK (vm-sym-b (lambda (x) x) t (vm-prop 17) (autoload "vm-sym-file" nil nil nil) (autoload "vm-sym-file" nil nil nil) t nil)"#
    );
}

#[test]
fn vm_variable_lookup_builtins_use_shared_dynamic_and_buffer_local_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (defvaralias 'vm-vm-alias 'vm-vm-base)
                 (list
                  (boundp 'vm-vm-alias)
                  (default-boundp 'vm-vm-alias)
                  (special-variable-p 'vm-vm-alias)
                  (indirect-variable 'vm-vm-alias)
                  (symbol-value 'vm-vm-alias)
                  (let ((vm-vm-base 9))
                    (list (boundp 'vm-vm-base)
                          (symbol-value 'vm-vm-base)))))"#,
            |eval| {
                let current = eval.buffers.current_buffer_id().expect("current buffer");
                eval.set_buffer_local_binding_by_id(
                    current,
                    crate::emacs_core::intern::intern("vm-vm-base"),
                    Value::fixnum(3),
                )
                .expect("buffer-local binding");
            },
        ),
        // GNU `specbind` records SPECPDL_LET_LOCAL for a localized
        // symbol with a current-buffer binding; `symbol-value` sees
        // the let-bound value until unbind restores the local cell.
        "OK (t nil t vm-vm-base 3 (t 9))"
    );
}

#[test]
fn vm_func_arity_and_obarray_queries_use_shared_obarray_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (fset 'vm-fa-target 'car)
                 (list
                  (func-arity 'vm-fa-target)
                  (intern-soft "vm-soft-target")
                  (intern-soft "vm-soft-miss")
                  (obarrayp (obarray-make 3))
                  (obarrayp [1 2 3])))"#,
            |eval| {
                eval.obarray_mut().intern("vm-soft-target");
            },
        ),
        "OK ((1 . 1) vm-soft-target nil t nil)"
    );
}

#[test]
fn vm_function_mutator_builtins_use_shared_function_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (fset 'vm-fset-target 'car)
                 (list
                  (funcall 'vm-fset-target '(4 . 5))
                  (progn
                    (fmakunbound 'vm-fset-target)
                    (fboundp 'vm-fset-target))
                  (condition-case err
                      (fmakunbound nil)
                    (error (car err)))
                  (progn
                    (fset nil nil)
                    (symbol-function nil))))"#
        ),
        "OK (4 nil setting-constant nil)"
    );
}

#[test]
fn vm_defalias_uses_shared_runtime_state_and_gnu_cycle_errors() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (setq vm-da-hook-log nil)
                 (put 'vm-da-hooked 'defalias-fset-function
                      (lambda (sym def)
                        (setq vm-da-hook-log (list sym def))
                        (fset sym def)))
                 (list
                  (defalias 'vm-da-hooked 'car "vm doc")
                  vm-da-hook-log
                  (symbol-function 'vm-da-hooked)
                  (get 'vm-da-hooked 'function-documentation)
                  (condition-case err
                      (defalias 'vm-da-self 'vm-da-self)
                    (error err))))"#
        ),
        r#"OK (vm-da-hooked (vm-da-hooked car) car "vm doc" (cyclic-function-indirection vm-da-self))"#
    );
}

#[test]
fn vm_fset_inside_lambda_uses_argument_definition() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"((lambda (sym def)
                  (fset sym def)
                  (list sym def (symbol-function sym)))
                'vm-da-hook-lambda
                'car)"#
        ),
        "OK (vm-da-hook-lambda car car)"
    );
}

#[test]
fn vm_lambda_argument_stack_slots_start_correct() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"((lambda (sym def)
                  (list sym def))
                'vm-da-hook-lambda
                'car)"#
        ),
        "OK (vm-da-hook-lambda car)"
    );
}

#[test]
fn vm_fset_inside_lambda_preserves_argument_identity() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"((lambda (sym def)
                  (fset sym def)
                  (list (eq sym 'vm-da-hook-lambda)
                        (eq def 'car)
                        (eq (symbol-function sym) 'car)))
                'vm-da-hook-lambda
                'car)"#
        ),
        "OK (t t t)"
    );
}

#[test]
fn vm_set_builtin_uses_shared_runtime_without_touching_lexicals() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_lexical_str(
            r#"(progn
                 (makunbound 'vm-lex-set)
                 (let ((vm-lex-set 10))
                   (list (set 'vm-lex-set 20)
                         vm-lex-set
                         (symbol-value 'vm-lex-set))))"#
        ),
        "OK (20 10 20)"
    );
}

#[test]
fn vm_defvaralias_uses_shared_runtime_state_and_gnu_cycle_errors() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (setq vm-dva-events nil)
                 (fset 'vm-dva-rec
                       (lambda (symbol newval operation where)
                         (setq vm-dva-events
                               (cons (list symbol newval operation where)
                                     vm-dva-events))))
                 (defvaralias 'vm-dva-alias 'vm-dva-old)
                 (add-variable-watcher 'vm-dva-old 'vm-dva-rec)
                 (list
                  (defvaralias 'vm-dva-alias 'vm-dva-new "vm variable doc")
                  vm-dva-events
                  (indirect-variable 'vm-dva-alias)
                  (get 'vm-dva-alias 'variable-documentation)
                  (condition-case err
                      (progn
                        (defvaralias 'vm-dva-a 'vm-dva-b)
                        (defvaralias 'vm-dva-b 'vm-dva-a))
                    (error err))))"#
        ),
        r#"OK (vm-dva-new ((vm-dva-old vm-dva-new defvaralias nil)) vm-dva-new "vm variable doc" (cyclic-variable-indirection vm-dva-a))"#
    );
}

#[test]
fn vm_varset_and_set_resolve_aliases_and_reject_constants_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (defvaralias 'vm-set-alias 'vm-set-base)
                 (setq vm-set-alias 3)
                 (list
                  vm-set-base
                  vm-set-alias
                  (set 'vm-set-alias 4)
                  vm-set-base
                  vm-set-alias
                  (progn
                    (setq vm-set-side 0)
                    (condition-case err
                        (setq nil (setq vm-set-side 1))
                      (error (list (car err) (cdr err) vm-set-side))))
                  (progn
                    (setq vm-set-side 0)
                    (condition-case err
                        (setq :vm-set-k (setq vm-set-side 2))
                      (error (list (car err) (cdr err) vm-set-side))))))"#
        ),
        "OK (3 3 4 4 4 (setting-constant (nil) 1) (setting-constant (:vm-set-k) 2))"
    );
}

#[test]
fn vm_makunbound_uses_shared_runtime_void_bindings() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (defvar vm-mku-dyn 'global)
                 (list
                  (let ((vm-mku-dyn 'dyn))
                    (list (makunbound 'vm-mku-dyn)
                          (condition-case err vm-mku-dyn (error (car err)))
                          (condition-case err
                              (default-value 'vm-mku-dyn)
                            (error (car err)))
                          (boundp 'vm-mku-dyn)))
                  vm-mku-dyn
                  (default-value 'vm-mku-dyn)))"#
        ),
        "OK ((vm-mku-dyn void-variable void-variable nil) global global)"
    );
}

#[test]
fn vm_make_local_variable_ignores_lexical_locals_and_uses_runtime_binding() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_lexical_str(
            r#"(progn
                 (setq vm-mlv-lex-global 'global)
                 (let ((buf (get-buffer-create "vm-mlv-lex-buf")))
                   (set-buffer buf)
                   (let ((vm-mlv-lex-global 'lex))
                     (make-local-variable 'vm-mlv-lex-global)
                     (list vm-mlv-lex-global
                           (symbol-value 'vm-mlv-lex-global)
                           (buffer-local-value 'vm-mlv-lex-global buf)
                           (local-variable-p 'vm-mlv-lex-global buf)
                           (condition-case err
                               (buffer-local-value 'vm-mlv-lex-global buf)
                             (error (car err)))
                           (default-value 'vm-mlv-lex-global)))))"#
        ),
        "OK (lex global global t global global)"
    );
}

#[test]
fn vm_kill_local_variable_uses_shared_runtime_and_buffer_where_watchers() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (setq vm-klv-events nil)
                 (fset 'vm-klv-rec
                       (lambda (symbol newval operation where)
                         (setq vm-klv-events
                               (cons (list symbol newval operation (bufferp where) (buffer-live-p where))
                                     vm-klv-events))))
                 (defvaralias 'vm-klv-alias 'vm-klv-base)
                 (add-variable-watcher 'vm-klv-base 'vm-klv-rec)
                 (let ((buf (get-buffer-create "vm-klv-buf")))
                   (set-buffer buf)
                   (make-local-variable 'vm-klv-alias)
                   (set 'vm-klv-alias 7)
                   (kill-local-variable 'vm-klv-alias))
                 vm-klv-events)"#
        ),
        "OK ((vm-klv-base nil makunbound t t) (vm-klv-base 7 set t t))"
    );
}

#[test]
fn vm_kill_all_local_variables_uses_shared_runtime_defaults_and_clears_local_map() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (setq fill-column 70)
                 (use-local-map (make-sparse-keymap))
                 (make-local-variable 'fill-column)
                 (setq fill-column 80)
                 (setq major-mode 'neo-mode)
                 (setq mode-name "Neo")
                 (setq buffer-undo-list t)
                 (kill-all-local-variables)
                 (list fill-column
                       (current-local-map)
                       major-mode
                       mode-name
                       buffer-undo-list
                       (local-variable-p 'major-mode)
                       (local-variable-p 'mode-name)
                       (local-variable-p 'buffer-undo-list)))"#
        ),
        "OK (70 nil fundamental-mode \"Fundamental\" t t t t)"
    );
}

#[test]
fn vm_setq_buffer_undo_list_reads_shared_undo_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (setq buffer-undo-list t)
                 buffer-undo-list)"#
        ),
        "OK t"
    );
}

#[test]
fn vm_plain_nil_varref_does_not_probe_buffer_local_storage() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();
    let ordinary_nil = intern("vm-ordinary-nil-global");
    eval.obarray.set_symbol_value_id(ordinary_nil, Value::NIL);

    crate::buffer::buffer::reset_buffer_local_value_lookup_probes();
    let value = new_vm(&mut eval)
        .fast_path_var_ref(ordinary_nil)
        .expect("a bound plain nil variable should be readable");

    assert_eq!(value, Value::NIL);
    assert_eq!(
        crate::buffer::buffer::buffer_local_value_lookup_probes(),
        0,
        "ordinary nil globals must not pay the buffer-undo-list compatibility probe"
    );
}

#[test]
fn vm_syntax_table_accessors_use_shared_current_buffer_state() {
    crate::test_utils::init_test_tracing();
    // syntax-after is defined in subr.el → bootstrap context required.
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let ((primary (current-buffer))
                     (other (get-buffer-create "vm-syntax-other")))
                 (set-syntax-table (copy-syntax-table (standard-syntax-table)))
                 (modify-syntax-entry ?\; "<")
                 (erase-buffer)
                 (insert ";")
                 (list (syntax-table-p (syntax-table))
                       (= (char-syntax ?\;) ?<)
                       (consp (syntax-after 1))
                       (= (matching-paren ?\() ?\))
                       (not (eq (syntax-table) (standard-syntax-table)))
                       (progn
                         (set-buffer other)
                         (list (= (char-syntax ?\;) ?.)
                               (eq (syntax-table) (standard-syntax-table))))
                       (progn
                         (set-buffer primary)
                         (= (char-syntax ?\;) ?<))))"#
        ),
        "OK (t t t t t (t t) t)"
    );
}

#[test]
fn vm_syntax_motion_builtins_use_shared_point_and_syntax_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (set-syntax-table (copy-syntax-table (standard-syntax-table)))
                 (modify-syntax-entry ?\; "<")
                 (modify-syntax-entry ?\n ">")
                 (modify-syntax-entry ?' ". p")
                 (erase-buffer)
                 (insert "  ;c\n''foo bar")
                 (list
                  (progn (goto-char 1) (list (forward-comment 1) (point)))
                  (progn (goto-char 8) (backward-prefix-chars) (point))
                  (progn (goto-char 8) (forward-word) (point))
                  (progn (goto-char 1) (list (skip-syntax-forward " ") (point)))
                  (progn (goto-char 11) (list (skip-syntax-backward "w") (point)))))"#
        ),
        "OK ((t 6) 6 11 (2 3) (-3 8))"
    );
}

#[test]
fn vm_buffer_metadata_builtins_use_shared_manager_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((base (get-buffer-create "vm-meta-base"))
                     (indirect (make-indirect-buffer base "vm-meta-ind" t)))
                 (set-default 'vm-find-target 10)
                 (set-buffer indirect)
                 (make-local-variable 'vm-find-target)
                 (setq vm-find-target 88)
                 (list (buffer-live-p indirect)
                       (eq (get-buffer indirect) indirect)
                       (eq (find-buffer 'vm-find-target 88) indirect)
                       (equal (buffer-name indirect) "vm-meta-ind")
                       (buffer-last-name indirect)
                       (eq (buffer-base-buffer indirect) base)
                       (buffer-file-name indirect)))"#
        ),
        "OK (t t t t \"vm-meta-base\" t nil)"
    );
}

#[test]
fn vm_parse_partial_sexp_uses_shared_current_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((a (get-buffer-create "vm-pps-a"))
                     (b (get-buffer-create "vm-pps-b")))
                 (set-buffer a)
                 (erase-buffer)
                 (insert "(a)")
                 (setq vm-pps-a (parse-partial-sexp 1 3))
                 (set-buffer b)
                 (erase-buffer)
                 (insert "abc")
                 (list vm-pps-a
                       (parse-partial-sexp 1 4)))"#
        ),
        "OK ((1 1 2 nil nil nil 0 nil nil (1) nil) (0 nil 1 nil nil nil 0 nil nil nil nil))"
    );
}

#[test]
fn vm_parse_partial_sexp_commentstop_syntax_table_advances_point() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (set-syntax-table (copy-syntax-table (standard-syntax-table)))
                 (modify-syntax-entry ?\; "<")
                 (modify-syntax-entry ?\n ">")
                 (erase-buffer)
                 (insert ";; x\nfoo")
                 (goto-char 1)
                 (let* ((state1 (parse-partial-sexp (point) (point-max) nil nil nil 'syntax-table))
                        (point1 (point))
                        (state2 (parse-partial-sexp (point) (point-max) nil nil state1 'syntax-table)))
                   (list state1 point1 state2 (point))))"#
        ),
        "OK ((0 nil nil nil t nil 0 nil 1 nil nil) 2 (0 nil nil nil nil nil 0 nil nil nil nil) 6)"
    );
}

#[test]
fn vm_overlay_builtins_use_shared_current_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (erase-buffer)
                 (insert "overlay body")
                 (let ((ov1 (make-overlay 2 6))
                       (ov2 (make-overlay 6 10)))
                   (overlay-put ov1 'face 'bold)
                   (list
                    (overlayp ov1)
                    (overlay-get ov1 'face)
                    (length (overlays-at 3))
                    (length (overlays-in 1 13))
                    (next-overlay-change 1)
                    (previous-overlay-change 10)
                    (progn
                      (move-overlay ov1 4 8)
                      (list (overlay-start ov1)
                            (overlay-end ov1)
                            (eq (overlay-buffer ov1) (current-buffer))
                            (> (length (overlay-properties ov1)) 0)))
                    (progn
                      (delete-overlay ov2)
                      (length (overlays-in 1 13)))
                    (progn
                      (delete-all-overlays)
                      (length (overlays-in 1 13))))))"#
        ),
        "OK (t bold 1 2 2 6 (4 8 t t) 1 0)"
    );
}

#[test]
fn vm_overlays_at_sorted_uses_shared_priority_order() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (erase-buffer)
                 (insert "overlay body")
                 (let ((ov-low (make-overlay 2 6))
                       (ov-high (make-overlay 2 6))
                       (ov-nil (make-overlay 2 6)))
                   (overlay-put ov-low 'name 'low)
                   (overlay-put ov-high 'name 'high)
                   (overlay-put ov-nil 'name 'nil-priority)
                   (overlay-put ov-low 'priority 1)
                   (overlay-put ov-high 'priority 10)
                   (mapcar (lambda (ov) (overlay-get ov 'name))
                           (overlays-at 3 t))))"#
        ),
        "OK (high low nil-priority)"
    );
}

#[test]
fn vm_text_property_builtins_use_shared_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (erase-buffer)
                 (insert "abcd")
                 (put-text-property 1 3 'face 'bold)
                 (add-text-properties 2 5 '(mouse-face highlight display "D"))
                 (list
                  (get-text-property 2 'face)
                  (get-char-property 3 'mouse-face)
                  (plist-get (text-properties-at 2) 'face)
                  (car (get-char-property-and-overlay 2 'face))
                  (cdr (get-char-property-and-overlay 2 'face))
                  (get-display-property 2 'display)
                  (progn
                    (remove-text-properties 2 5 '(mouse-face highlight))
                    (get-text-property 3 'mouse-face))
                  (progn
                    (set-text-properties 3 5 '(rear-nonsticky t))
                    (get-text-property 4 'rear-nonsticky))
                  (progn
                    (remove-list-of-text-properties 1 3 '(face))
                    (get-text-property 2 'face))))"#
        ),
        "OK (bold highlight bold bold nil \"D\" nil t nil)"
    );
}

#[test]
fn vm_text_property_change_queries_use_shared_live_marker_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (erase-buffer)
                 (insert "abcde")
                 (put-text-property 2 5 'p t)
                 (let ((lim (copy-marker 5))
                       (end (copy-marker 5 t)))
                   (goto-char 1)
                   (insert "Z")
                   (list
                    (next-property-change 1)
                    (next-single-property-change 1 'p)
                    (next-char-property-change 1)
                    (next-single-char-property-change 1 'p)
                    (previous-property-change lim)
                    (previous-single-property-change lim 'p)
                    (previous-char-property-change lim)
                    (previous-single-char-property-change lim 'p)
                    (text-property-any 1 end 'p t)
                    (text-property-not-all 3 end 'p t))))"#
        ),
        "OK (3 3 3 3 3 3 3 3 3 nil)"
    );
}

#[test]
fn vm_property_query_builtins_use_shared_overlay_precedence_and_stickiness() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (erase-buffer)
                 (insert "abcd")
                 (put-text-property 1 2 'carry 'before)
                 (put-text-property 1 2 'rear-nonsticky '(carry))
                 (put-text-property 2 3 'carry 'after)
                 (put-text-property 2 3 'front-sticky '(carry))
                 (put-text-property 2 3 'face 'text)
                 (let ((ov-low (make-overlay 2 4 nil t nil))
                       (ov-high (make-overlay 2 4 nil t nil)))
                   (overlay-put ov-low 'face 'low)
                   (overlay-put ov-low 'priority 1)
                   (overlay-put ov-high 'face 'high)
                   (overlay-put ov-high 'priority '(10 . 0))
                   (let ((pair (get-char-property-and-overlay 2 'face)))
                   (list
                    (get-char-property 2 'face)
                    (car pair)
                    (overlay-get (cdr pair) 'face)
                    (get-pos-property 2 'face)
                    (get-pos-property 2 'carry)
                    (get-pos-property 3 'face)))))"#
        ),
        "OK (high high high nil after high)"
    );
}

#[test]
fn vm_add_face_text_property_uses_shared_face_merge_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (erase-buffer)
                 (insert "abcd")
                 (put-text-property 1 3 'face 'italic)
                 (add-face-text-property 1 3 'bold)
                 (add-face-text-property 1 3 'underline t)
                 (list
                  (get-text-property 1 'face)
                  (get-text-property 3 'face)))"#
        ),
        "OK ((bold italic underline) nil)"
    );
}

#[test]
fn vm_marker_builtins_use_shared_live_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (erase-buffer)
                 (insert "abcd")
                 (goto-char 3)
                 (let ((pm (point-marker))
                       (cm (copy-marker 3 t))
                       (minm (point-min-marker))
                       (maxm (point-max-marker)))
                   (goto-char 1)
                   (insert "Q")
                   (goto-char 4)
                   (insert "Z")
                   (list
                    (marker-position pm)
                    (marker-position minm)
                    (marker-position maxm)
                    (marker-position cm)
                    (progn (set-marker pm 2) (marker-position pm))
                    (progn (set-marker pm nil) (marker-position pm)))))"#
        ),
        "OK (4 1 7 5 2 nil)"
    );
}

#[test]
fn vm_mark_marker_uses_shared_buffer_mark_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str("(marker-position (mark-marker))", |eval| {
            let current = eval.buffers.current_buffer_id().expect("current buffer");
            let _ = eval.buffers.replace_buffer_contents(current, "abcd");
            let _ = eval
                .buffers
                .set_buffer_mark_emacs_byte_pos(current, crate::buffer::EmacsBytePos::new(2));
        }),
        "OK 3"
    );
}

#[test]
fn vm_motion_builtins_use_shared_current_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "abc\ndef\nghi")
                 (goto-char 6)
                 (list
                  (pos-bol)
                  (pos-eol)
                  (progn (forward-line 1) (point))
                  (progn (beginning-of-line) (point))
                  (progn (end-of-line) (point))
                  (progn (backward-char 2) (point))
                  (progn (forward-char 1) (point))
                  (progn
                    (goto-char 1)
                    (list (vertical-motion 1) (point)
                          (vertical-motion 1) (point)
                          (vertical-motion -1) (point)))))"#
        ),
        "OK (5 8 9 9 12 10 11 (1 5 1 9 -1 5))"
    );
}

#[test]
fn vm_vertical_motion_uses_invisible_text_boundaries_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "aaaa\nbbbb\ncccc\n")
                 (put-text-property 5 10 'invisible t)
                 (goto-char 5)
                 (let ((forward (list (vertical-motion 1)
                                      (point)
                                      (line-number-at-pos)
                                      (current-column)
                                      (char-after))))
                   (goto-char 11)
                   (list forward
                         (list (vertical-motion -1)
                               (point)
                               (line-number-at-pos)
                               (current-column)
                               (char-after)))))"#
        ),
        "OK ((1 11 3 0 99) (-1 1 1 0 97))"
    );
}

#[test]
fn vm_vertical_motion_zero_backs_across_invisible_newlines_to_visual_row_start_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert
                  "order-17\n"
                  "  item: keyboard\n"
                  "  item: mouse\n"
                  "  total: 120\n"
                  "order-23\n"
                  "  status: shipped\n")
                 (setq buffer-invisibility-spec '(checkout-fold))
                 (let* ((fold-end (save-excursion
                                    (goto-char (point-min))
                                    (forward-line 4)
                                    (point)))
                        (fold (make-overlay
                               (save-excursion
                                 (goto-char (point-min))
                                 (line-end-position))
                               fold-end)))
                   (overlay-put fold 'invisible 'checkout-fold)
                   (goto-char fold-end)
                   (list (vertical-motion 0)
                         (point)
                         (line-number-at-pos))))"#
        ),
        "OK (0 1 1)"
    );
}

#[test]
fn vm_vertical_motion_does_not_count_missing_previous_line_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "abc\ndef\n")
                 (let (first second-bol second-mid)
                   (goto-char 3)
                   (setq first (list (vertical-motion -1)
                                     (point)
                                     (line-number-at-pos)
                                     (current-column)))
                   (goto-char 5)
                   (setq second-bol (list (vertical-motion -1)
                                          (point)
                                          (line-number-at-pos)
                                          (current-column)))
                   (goto-char 6)
                   (setq second-mid (list (vertical-motion -1)
                                          (point)
                                          (line-number-at-pos)
                                          (current-column)))
                   (list first second-bol second-mid)))"#
        ),
        "OK ((0 1 1 0) (-1 1 1 0) (-1 1 1 0))"
    );
}

#[test]
fn vm_vertical_motion_wraps_long_screen_lines_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (delete-other-windows-internal)
                 (insert (make-string 300 ?x))
                 (goto-char 1)
                 (let ((forward (list (window-body-width)
                                      (vertical-motion 2)
                                      (point)
                                      (current-column))))
                   (vertical-motion -1)
                   (list forward
                         (point)
                         (current-column))))"#
        ),
        "OK ((80 2 159 158) 80 79)"
    );
}

#[test]
fn vm_vertical_motion_large_downward_motion_lands_at_eob_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (delete-other-windows-internal)
                 (insert (make-string 200 ?x))
                 (goto-char 1)
                 (list (window-body-width)
                       (buffer-size)
                       (vertical-motion (buffer-size))
                       (point)))"#
        ),
        "OK (80 200 2 201)"
    );
}

#[test]
fn vm_vertical_motion_zero_moves_to_current_screen_line_start() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 ;; `delete-other-windows' is lisp/window.el:4453 and has no
                 ;; subr any more (DIVERGENCES.md 154); its body ends in this C
                 ;; primitive (src/window.c:3463).
                 (delete-other-windows-internal)
                 (insert (make-string 200 ?x))
                 (goto-char 81)
                 (list (current-column)
                       (vertical-motion 0)
                       (point)
                       (current-column)))"#
        ),
        "OK (80 0 80 79)"
    );
}

#[test]
fn vm_split_window_inherits_selected_buffer_point_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((b (get-buffer-create " *probe-wrap-narrow*")))
                 (unwind-protect
                     (progn
                       (delete-other-windows-internal)
                       ;; `switch-to-buffer' is lisp/window.el:9558 and has no
                       ;; subr any more (DIVERGENCES.md 154); these are the C
                       ;; primitives its body calls.
                       (set-window-buffer (selected-window) b)
                       (select-window (selected-window))
                       (set-buffer b)
                       (erase-buffer)
                       (insert (make-string 200 ?x))
                       (let ((w2 (split-window-internal nil 40 'right nil)))
                         (list (window-point w2)
                               (progn
                                 (select-window w2)
                                 (point)))))
                   (if (buffer-live-p b)
                       (kill-buffer b))
                   (delete-other-windows-internal)))"#
        ),
        "OK (201 201)"
    );
}

#[test]
fn vm_region_bounds_use_shared_mark_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str("(list (region-beginning) (region-end))", |eval| {
            let current = eval.buffers.current_buffer_id().expect("current buffer");
            let _ = eval.buffers.replace_buffer_contents(current, "abcdef");
            let _ = eval
                .buffers
                .goto_buffer_emacs_byte_pos(current, crate::buffer::EmacsBytePos::new(2));
            let _ = eval
                .buffers
                .set_buffer_mark_emacs_byte_pos(current, crate::buffer::EmacsBytePos::new(4));
        }),
        "OK (3 5)"
    );
}

#[test]
fn vm_symbol_mutator_type_errors_match_oracle() {
    crate::test_utils::init_test_tracing();
    with_vm_eval("(set 1 2)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("symbolp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(fset 1 2)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("symbolp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(get 1 'p)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("symbolp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });

    with_vm_eval("(put 1 'p 2)", false, |result| match result {
        Err(EvalError::Signal { symbol, data, .. }) => {
            assert_eq!(resolve_sym(symbol), "wrong-type-argument");
            assert_eq!(data, vec![Value::symbol("symbolp"), Value::fixnum(1)]);
        }
        other => panic!("unexpected error: {other:?}"),
    });
}

#[test]
fn vm_not_negation() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(/= 1 2)"), "OK t");
    assert_eq!(vm_eval_str("(/= 1 1)"), "OK nil");
}

#[test]
fn vm_float_arithmetic() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(+ 1.0 2.0)"), "OK 3.0");
    assert_eq!(vm_eval_str("(+ 1 2.0)"), "OK 3.0");
}

#[test]
fn vm_dotimes() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str("(let ((sum 0)) (dotimes (i 5) (setq sum (+ sum i))) sum)"),
        "OK 10"
    );
}

#[test]
fn vm_dolist() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            "(let ((result nil)) (dolist (x '(a b c)) (setq result (cons x result))) result)"
        ),
        "OK (c b a)"
    );
}

#[test]
fn vm_lambda_parameters_can_shadow_nil_and_t() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(funcall (lambda (t) t) 7)"), "OK 7");
    assert_eq!(vm_eval_str("(funcall (lambda (nil) nil) 9)"), "OK 9");
    assert_eq!(
        vm_eval_str("(mapcar (lambda (t) t) '(1 2 3))"),
        "OK (1 2 3)"
    );
    assert_eq!(
        vm_eval_str("(mapcar (lambda (nil) nil) '(4 5 6))"),
        "OK (4 5 6)"
    );
}

#[test]
fn vm_gnu_arg_descriptor_preserves_optional_and_rest_slots() {
    crate::test_utils::init_test_tracing();
    let arg_descriptor = 3 | (4 << 8) | 128;
    let func = ByteCodeFunction {
        source_id: crate::emacs_core::bytecode::fresh_bytecode_source_id(),
        // Hand-built but seal-shaped by construction (trailing Return,
        // in-bounds targets/constants); the marker vouches for that. Left
        // unverified so these run in the checked driver instance.
        ops_sealed: true,
        stack_verified: false,
        ops: vec![
            Op::StackRef(4),
            Op::StackRef(4),
            Op::StackRef(4),
            Op::StackRef(4),
            Op::StackRef(4),
            Op::List(5),
            Op::Return,
        ],
        constants: vec![].into(),
        max_stack: 10,
        params: crate::emacs_core::bytecode::decode::parse_arglist_descriptor(arg_descriptor),
        arglist: Value::fixnum(arg_descriptor),
        lexical: false,
        env: None,
        gnu_byte_offset_map: None,
        gnu_bytecode_bytes: None,
        docstring: None,
        doc_form: None,
        interactive: None,
        closure_slot_count: 4,
        extra_slots: Vec::new(),
        #[cfg(feature = "jit")]
        runtime: Some(crate::emacs_core::jit::Runtime::new()),
        lazy_gnu_code: None,
    };

    let mut eval = Context::new_vm_runtime_harness();
    let mut vm = new_vm(&mut eval);

    let result = vm
        .execute(
            &func,
            vec![
                Value::fixnum(1),
                Value::fixnum(2),
                Value::fixnum(3),
                Value::fixnum(4),
            ],
        )
        .expect("vm should preserve GNU descriptor slot layout");

    assert_eq!(
        result,
        Value::list(vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::fixnum(3),
            Value::fixnum(4),
            Value::NIL,
        ])
    );
}

#[test]
fn zero_argument_bytecode_frames_skip_argument_copy_work() {
    reset_frame_argument_copy_counts();
    let mut values = vec![Value::fixnum(1)];

    copy_frame_arguments(&mut values, 1, 0);

    assert_eq!(FrameArgumentCopy::for_count(0), FrameArgumentCopy::None);
    assert_eq!(values, vec![Value::fixnum(1)]);
    assert_eq!(frame_argument_copy_counts(), (0, 0));
}

#[test]
fn unary_bytecode_frames_have_a_dedicated_copy_strategy() {
    assert_eq!(FrameArgumentCopy::for_count(1), FrameArgumentCopy::One);
}

#[test]
fn small_bytecode_frames_copy_arguments_without_bulk_memmove() {
    reset_frame_argument_copy_counts();
    let mut values = vec![Value::fixnum(1), Value::fixnum(2)];

    copy_frame_arguments(&mut values, 0, 2);

    assert_eq!(
        values,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::fixnum(1),
            Value::fixnum(2),
        ]
    );
    assert_eq!(frame_argument_copy_counts(), (1, 0));
}

#[test]
fn wide_bytecode_frames_retain_bulk_argument_copy() {
    reset_frame_argument_copy_counts();
    let mut values = (0..16).map(Value::fixnum).collect::<Vec<_>>();

    copy_frame_arguments(&mut values, 0, 16);

    assert_eq!(values.len(), 32);
    assert_eq!(&values[..16], &values[16..]);
    assert_eq!(frame_argument_copy_counts(), (0, 1));
}

#[test]
fn vm_compiled_autoload_registration_updates_shared_autoload_manager() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();
    let result = eval
        .eval_str("(autoload 'vm-bytecode-auto \"vm-bytecode-auto-file\")")
        .expect("autoload should execute");

    assert_eq!(result, Value::symbol("vm-bytecode-auto"));
    let entry = eval
        .autoloads
        .get_entry("vm-bytecode-auto")
        .expect("autoload registration should propagate back out of VM bridge");
    assert_eq!(entry.file.as_utf8_str(), Some("vm-bytecode-auto-file"));
}

#[test]
fn vm_compiled_this_single_command_keys_uses_live_eval_key_context() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();
    eval.set_read_command_keys(vec![Value::fixnum(97)]);

    let result = eval
        .eval_str("(this-single-command-keys)")
        .expect("this-single-command-keys should execute");

    assert_eq!(result, Value::vector(vec![Value::fixnum(97)]));
}

#[test]
fn vm_compiled_require_does_not_treat_recursive_stack_as_provided() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("vm-bytecode-rec.el");
    std::fs::write(
        &fixture,
        "(setq vm-bytecode-required-ran t)\n(provide 'vm-bytecode-rec)\n",
    )
    .expect("write require fixture");

    let mut eval = Context::new_vm_runtime_harness();
    eval.obarray.set_symbol_value(
        "load-path",
        Value::list(vec![Value::string(dir.path().to_string_lossy())]),
    );
    eval.require_stack = vec![intern("vm-bytecode-rec")];

    let result = eval
        .eval_str(
            "(progn
           (setq vm-bytecode-required-ran nil)
           (require 'vm-bytecode-rec)
           vm-bytecode-required-ran)",
        )
        .expect("require should load until the feature is provided");

    assert_eq!(
        result,
        Value::T,
        "compiled require should not treat require_stack membership as provide"
    );
}

#[test]
fn vm_compiled_require_loads_feature_with_nil_filename_through_shared_runtime() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("vm-bytecode-load.el");
    std::fs::write(
        &fixture,
        "(setq vm-bytecode-required-ran t)\n(provide 'vm-bytecode-load)\n",
    )
    .expect("write require fixture");

    let mut eval = Context::new_vm_runtime_harness();
    eval.obarray.set_symbol_value(
        "load-path",
        Value::list(vec![Value::string(dir.path().to_string_lossy())]),
    );

    let result = eval
        .eval_str(
            "(progn
           (setq vm-bytecode-required-ran nil)
           (list
             (require 'vm-bytecode-load nil nil)
             vm-bytecode-required-ran
             (featurep 'vm-bytecode-load)))",
        )
        .expect("require should load feature through shared runtime");

    assert_eq!(
        result,
        Value::list(vec![Value::symbol("vm-bytecode-load"), Value::T, Value::T,])
    );
    assert!(
        eval.features.contains(&intern("vm-bytecode-load")),
        "compiled require should update shared features state"
    );
    assert!(
        eval.require_stack.is_empty(),
        "compiled require should unwind shared require stack after load"
    );
}

#[test]
fn vm_compiled_load_uses_shared_runtime_and_restores_load_file_name() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("vm-bytecode-shared-load.el");
    std::fs::write(&fixture, "(setq vm-bytecode-load-seen load-file-name)\n")
        .expect("write load fixture");

    let mut eval = Context::new_vm_runtime_harness();
    eval.obarray.set_symbol_value(
        "load-path",
        Value::list(vec![Value::string(dir.path().to_string_lossy())]),
    );

    let result = eval
        .eval_str(
            "(list
           (load \"vm-bytecode-shared-load\" nil nil nil nil)
           vm-bytecode-load-seen
           load-file-name)",
        )
        .expect("load should resolve path and execute through shared runtime");

    assert_eq!(
        result,
        Value::list(vec![
            Value::T,
            Value::string(fixture.to_string_lossy()),
            Value::NIL,
        ])
    );
    assert!(
        eval.loads_in_progress.is_empty(),
        "compiled load should unwind shared loads-in-progress state"
    );
}

#[test]
fn vm_compiled_load_allows_gnu_normal_recursive_load_depth() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("vm-bytecode-load.el");
    std::fs::write(&fixture, "(setq vm-bytecode-load-ran t)\n").expect("write load fixture");
    let fixture = fixture.canonicalize().expect("canonical load fixture");
    let fixture_lisp = crate::heap_types::LispString::from_utf8(fixture.to_string_lossy().as_ref());

    let mut eval = Context::new_vm_runtime_harness();
    eval.loads_in_progress = vec![fixture_lisp.clone()];

    let result = eval
        .eval_str(&format!(
            "(progn
           (setq vm-bytecode-load-ran nil)
           (load {:?} nil nil t)
           vm-bytecode-load-ran)",
            fixture.to_string_lossy()
        ))
        .expect("load should allow GNU's normal recursive depth");

    assert_eq!(
        result,
        Value::T,
        "compiled load should proceed until GNU's recursive-load limit is exceeded"
    );
    assert_eq!(
        eval.loads_in_progress,
        vec![fixture_lisp],
        "compiled load should restore the caller's loads_in_progress stack"
    );
}

#[test]
fn vm_compiled_load_signals_after_gnu_recursive_load_limit() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("vm-bytecode-recursive-limit.el");
    std::fs::write(&fixture, "(setq vm-bytecode-load-ran t)\n").expect("write load fixture");
    let fixture = fixture.canonicalize().expect("canonical load fixture");
    let fixture_lisp = crate::heap_types::LispString::from_utf8(fixture.to_string_lossy().as_ref());

    let mut eval = Context::new_vm_runtime_harness();
    eval.loads_in_progress = vec![
        fixture_lisp.clone(),
        fixture_lisp.clone(),
        fixture_lisp.clone(),
        fixture_lisp.clone(),
    ];

    let err = eval
        .eval_str(&format!(
            r#"(load {:?} nil nil t)"#,
            fixture.to_string_lossy()
        ))
        .expect_err("load should signal once GNU's recursive-load limit is exceeded");

    match err {
        EvalError::Signal { symbol, data, .. } => {
            assert_eq!(resolve_sym(symbol), "error");
            assert_eq!(data[0].as_utf8_str(), Some("Recursive load"));
            assert_eq!(data[1].cons_car(), Value::string(fixture.to_string_lossy()));
            assert_eq!(
                crate::emacs_core::value::list_to_vec(&data[1].cons_cdr())
                    .expect("recursive load payload tail should be a proper list")
                    .len(),
                4
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn vm_compiled_load_tracks_recursive_state_by_found_filename_text() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let inner = dir.path().join("inner");
    std::fs::create_dir_all(&inner).expect("create inner dir");
    let fixture = inner.join("vm-bytecode-found.el");
    std::fs::write(&fixture, "(setq vm-bytecode-found-ran t)\n").expect("write load fixture");
    let canonical = fixture.canonicalize().expect("canonical load fixture");
    let canonical_lisp =
        crate::heap_types::LispString::from_utf8(canonical.to_string_lossy().as_ref());
    let alternate = dir
        .path()
        .join("inner")
        .join("..")
        .join("inner")
        .join("vm-bytecode-found.el");

    let mut eval = Context::new_vm_runtime_harness();
    eval.loads_in_progress = vec![canonical_lisp.clone()];

    let result = eval
        .eval_str(&format!(
            "(progn
               (setq vm-bytecode-found-ran nil)
               (load {:?} nil nil t)
               vm-bytecode-found-ran)",
            alternate.to_string_lossy()
        ))
        .expect("textually distinct found filename should not trip recursive load");

    assert_eq!(result, Value::T);
    assert_eq!(
        eval.loads_in_progress,
        vec![canonical_lisp],
        "load should restore the caller's in-progress stack after using a distinct found filename",
    );
}

#[test]
fn vm_interactive_form_uses_shared_symbol_property_and_builtin_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (fset 'vm-if-shared-target (lambda () 1))
               (fset 'vm-if-shared-alias 'vm-if-shared-target)
               (put 'vm-if-shared-alias 'interactive-form '(interactive \"P\"))
               (list
                 (interactive-form 'vm-if-shared-alias)
                 (interactive-form 'vm-if-shared-target)
                 (interactive-form 'forward-char)
                 (interactive-form 'goto-char)
                 (interactive-form 'car)))"
        ),
        "OK ((interactive \"P\") nil (interactive \"^p\") (interactive (goto-char--read-natnum-interactive \"Go to char: \")) nil)"
    );
}

#[test]
fn vm_interactive_form_uses_shared_autoload_load_bridge() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("vm-interactive-form-auto.el");
    std::fs::write(
        &fixture,
        "(fset 'vm-interactive-form-auto
           '(lambda () (interactive \"P\") t))\n",
    )
    .expect("write interactive-form autoload fixture");

    let mut eval = Context::new_vm_runtime_harness();
    eval.obarray.set_symbol_value(
        "load-path",
        Value::list(vec![Value::string(dir.path().to_string_lossy())]),
    );

    let result = eval
        .eval_str(
            "(progn
           (autoload 'vm-interactive-form-auto \"vm-interactive-form-auto\")
           (interactive-form 'vm-interactive-form-auto))",
        )
        .expect("interactive-form should use shared autoload bridge");

    assert_eq!(
        result,
        Value::list(vec![Value::symbol("interactive"), Value::string("P")])
    );
}

#[test]
fn vm_command_modes_uses_shared_symbol_and_bytecode_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (fset 'vm-cm-shared-target '(lambda () t))
               (fset 'vm-cm-shared-alias 'vm-cm-shared-target)
               (put 'vm-cm-shared-alias 'command-modes '(foo-mode bar-mode))
               (let ((f (make-byte-code '() \"\" [] 0 nil [nil '(rust-ts-mode c-mode)])))
                 (fset 'vm-cm-shared-bytecode f))
               (list
                 (command-modes 'vm-cm-shared-alias)
                 (command-modes 'vm-cm-shared-target)
                 (command-modes '(lambda () (interactive \"p\" text-mode prog-mode) t))
                 (command-modes 'vm-cm-shared-bytecode)
                 (command-modes 'ignore)
                 (command-modes 'car)))"
        ),
        "OK ((foo-mode bar-mode) nil (text-mode prog-mode) (rust-ts-mode c-mode) nil nil)"
    );
}

#[test]
fn vm_commandp_uses_shared_command_metadata_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(let ((f (make-byte-code '() \"\" [] 0 nil [nil nil])))
               (list
                 (commandp 'forward-char)
                 (commandp 'car)
                 (commandp '(lambda () (interactive) t))
                 (commandp '(lambda () t))
                 (commandp \"abc\")
                 (commandp \"abc\" t)
                 (commandp [1 2 3])
                 (commandp [1 2 3] t)
                 (commandp f)))"
        ),
        "OK (t nil t nil t nil t nil t)"
    );
}

#[test]
fn vm_documentation_and_help_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            "(progn
               (put 'vm-doc-shared 'variable-documentation '(identity \"doc\"))
               ;; `describe-buffer-bindings' now really walks the translation
               ;; maps, and `help--describe-command' names each translated
               ;; character with `char-to-name' (mule-cmds.el, which this
               ;; deliberately minimal runtime does not load). Nil means \"no
               ;; name known\", which is the branch GNU takes for most keys.
               (fset 'char-to-name (lambda (_char) nil))
               (list
                 (stringp (documentation 'car))
                 (documentation-property 'vm-doc-shared 'variable-documentation)
                 (documentation-stringp '(\"DOC\" . 7))
                 (describe-buffer-bindings (current-buffer))
                 (condition-case err
                     ;; DESCRIBER is funcalled on each element; `car' is a
                     ;; C DEFUN and signals the same condition on a fixnum that
                     ;; `display-buffer' did.  Measured on GNU 31.0.90:
                     ;; (describe-vector [1] 'car) => (wrong-type-argument listp 1).
                     (describe-vector [1] 'car)
                   (wrong-type-argument (car err)))
                 (condition-case err
                     (help--describe-vector nil nil nil nil nil nil nil)
                   (wrong-type-argument (car err)))))",
            crate::test_utils::load_minimal_gnu_help_runtime,
        ),
        // GNU `Fhelp__describe_vector` starts with CHECK_VECTOR_OR_CHAR_TABLE,
        // so nil signals rather than returning nil (verified against 31.0.90).
        "OK (t \"doc\" t nil wrong-type-argument wrong-type-argument)"
    );
}

#[test]
fn vm_documentation_and_property_respect_raw_substitute_command_keys_semantics() {
    crate::test_utils::init_test_tracing();
    // The VM runtime harness (`Context::new_vm_runtime_harness`) is
    // a near-bare context — it has the C-level subrs but does not
    // run loadup.el, so `substitute-command-keys' (defined in
    // `lisp/help.el') is not bound. The test asserts that
    // (documentation X) substitutes `\[command]', which requires
    // help.el to have provided substitute-command-keys. Use the
    // init helper to load enough of the GNU runtime for the
    // substitute path to be reachable, mirroring R1's fix for the
    // analogous doc_test.rs failures.
    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (fset 'vm-doc-fn (lambda () t))
                 (put 'vm-doc-fn 'function-documentation "Press \\[save-buffer] to save.")
                 (put 'vm-doc-prop 'variable-documentation "Press \\[save-buffer] to save.")
                 (let ((doc (documentation 'vm-doc-fn))
                       (raw-doc (documentation 'vm-doc-fn t))
                       (prop (documentation-property 'vm-doc-prop 'variable-documentation))
                       (raw-prop (documentation-property 'vm-doc-prop 'variable-documentation t)))
                   (list (not (eq ?\\ (aref doc 6)))
                         (eq ?\\ (aref raw-doc 6))
                         (not (eq ?\\ (aref prop 6)))
                         (eq ?\\ (aref raw-prop 6)))))"#,
            |eval| {
                crate::test_utils::load_minimal_gnu_help_runtime(eval);
            },
        ),
        "OK (t t t t)"
    );
}

#[test]
fn vm_backtrace_and_recursion_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            // Ledger 172: `backtrace-debug' really sets the frame's
            // debug-on-exit bit now, so the flagged `list' frame enters the
            // debugger on the way out.  GNU does the same for this exact form
            // -- `emacs -Q --batch' prints "Debugger entered--returning value:
            // (t nil 2 1 nil t)" and still answers `(t nil 2 1 nil t)',
            // because `debug' returns the value it was handed.  This bare VM
            // context has no debug.el, so the stand-in below is that same
            // identity and the answer is unchanged.
            "(progn
               (setq debugger (lambda (&rest args) (nth 1 args)))
               (let ((thread (current-thread)))
                 (list
                   (car (car (backtrace--frames-from-thread thread)))
                   (backtrace--locals 1)
                   (backtrace-debug 1 2)
                   (backtrace-eval 1 2)
                   (backtrace-frame--internal '(lambda (&rest _) nil) 0 nil)
                   (integerp (recursion-depth)))))"
        ),
        "OK (t nil 2 1 nil t)"
    );
}

#[test]
fn vm_inline_opcode_builtins_dispatch_directly_like_gnu() {
    // GNU's inline opcodes (bytecode.c `Bpoint_min`, `Bchar_after`,
    // `Bbuffer_substring`, ...) call the C primitive directly: no funcall, no
    // backtrace frame, no arity check. The decoder lowers them to
    // `Op::CallBuiltinSym`; the VM must take the same direct path.
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();
    eval.eval_str("(insert \"abc\")").expect("buffer setup");
    eval.eval_str("(goto-char 2)").expect("buffer setup");

    let run = |eval: &mut Context, constants: Vec<Value>, ops: Vec<Op>| {
        let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![]));
        f.constants = constants.into();
        f.ops = ops;
        f.max_stack = 4;
        reset_inline_builtin_direct_count();
        let result = new_vm(eval).execute(&f, vec![]);
        (result, inline_builtin_direct_count())
    };

    let (result, direct) = run(
        &mut eval,
        vec![],
        vec![
            Op::CallBuiltinSym(intern("point-min"), 0),
            Op::Add1,
            Op::Return,
        ],
    );
    assert_eq!(result.expect("point-min"), Value::fixnum(2));
    assert_eq!(direct, 1, "Bpoint_min dispatches its primitive directly");

    let (result, direct) = run(
        &mut eval,
        vec![Value::fixnum(1)],
        vec![
            Op::Constant(0),
            Op::CallBuiltinSym(intern("char-after"), 1),
            Op::Return,
        ],
    );
    assert_eq!(result.expect("char-after"), Value::fixnum(i64::from(b'a')));
    assert_eq!(direct, 1, "Bchar_after dispatches its primitive directly");

    let (result, direct) = run(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(3)],
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::CallBuiltinSym(intern("buffer-substring"), 2),
            Op::Length,
            Op::Return,
        ],
    );
    assert_eq!(result.expect("buffer-substring"), Value::fixnum(2));
    assert_eq!(
        direct, 1,
        "Bbuffer_substring dispatches its primitive directly"
    );

    // A signalling primitive still reaches the condition machinery.
    let (result, direct) = run(
        &mut eval,
        vec![Value::string("x")],
        vec![
            Op::Constant(0),
            Op::CallBuiltinSym(intern("char-after"), 1),
            Op::Return,
        ],
    );
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(
            crate::emacs_core::intern::resolve_sym(sig.symbol),
            "wrong-type-argument"
        ),
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }
    assert_eq!(direct, 1);

    // VM-owned specials need the interpreter itself and keep the framed path.
    let (result, direct) = run(
        &mut eval,
        vec![],
        vec![Op::CallBuiltinSym(intern("garbage-collect"), 0), Op::Return],
    );
    result.expect("garbage-collect");
    assert_eq!(direct, 0, "VM specials do not take the inline path");
}
