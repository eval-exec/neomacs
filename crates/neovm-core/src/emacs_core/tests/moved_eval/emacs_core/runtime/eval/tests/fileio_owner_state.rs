//! Native file-I/O state uses the existing cold owner allocation, not the
//! World's shared state. These rows execute in core with the real hook set.

use crate::emacs_core::eval::{AutoSaveNativeWritePolicy, Context};
use crate::emacs_core::load_boot::load_from_dump;
use crate::emacs_core::pdump::dump_to_file;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};
use neovm_value::error::EvalResult;
use neovm_value::value::{Value, list_to_vec};
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};

fn snapshot(context: &mut Context, arguments: Vec<Value>) -> EvalResult {
    assert!(arguments.is_empty());
    let (kind, mode) = match context.auto_save_native_write_policy() {
        AutoSaveNativeWritePolicy::Ordinary { preserved_mode } => ("ordinary", preserved_mode),
        AutoSaveNativeWritePolicy::Active { creation_mode } => ("active", creation_mode),
    };
    Ok(Value::list(vec![
        Value::fixnum(context.tagged_heap().tagged_heap_identity() as i64),
        Value::symbol(kind),
        Value::fixnum(i64::from(mode)),
    ]))
}

fn activate_worker(context: &mut Context, arguments: Vec<Value>) -> EvalResult {
    assert!(arguments.is_empty());
    assert_eq!(
        context.replace_auto_save_native_write_policy(AutoSaveNativeWritePolicy::Active {
            creation_mode: 0o600,
        }),
        AutoSaveNativeWritePolicy::Ordinary { preserved_mode: 0 },
        "a newly constructed same-World owner must not inherit main's active mode",
    );
    Ok(Value::NIL)
}

#[test]
fn auto_save_native_policy_survives_same_world_green_owner_switch() {
    let mut context = crate::test_utils::new_context();
    context.set_green_threading_mode_at_construction_for_test();
    assert_eq!(
        context.auto_save_native_write_policy(),
        AutoSaveNativeWritePolicy::Ordinary { preserved_mode: 0 },
    );
    for (name, function) in [
        (
            "fileio-owner-snapshot",
            snapshot as fn(&mut Context, Vec<Value>) -> EvalResult,
        ),
        ("fileio-owner-activate-worker", activate_worker),
    ] {
        context.register_subr(SubrSpec::new(
            name,
            NativeFn::ContextVec(function),
            SubrArity::new(0, Some(0)),
        ));
    }
    let heap = context.tagged_heap().tagged_heap_identity();
    context.replace_auto_save_native_write_policy(AutoSaveNativeWritePolicy::Active {
        creation_mode: 0o640,
    });
    let result = context
        .eval_str(
            r#"(progn
             (setq fileio-owner-gate (make-mutex)
                   fileio-owner-shared (list 'initial)
                   fileio-owner-initial nil
                   fileio-owner-before nil
                   fileio-owner-after nil)
             (mutex-lock fileio-owner-gate)
             (setq fileio-owner-worker
               (make-thread
                 (lambda ()
                   (setq fileio-owner-initial (fileio-owner-snapshot))
                   (fileio-owner-activate-worker)
                   (setq fileio-owner-before (fileio-owner-snapshot))
                   (mutex-lock fileio-owner-gate)
                   (setq fileio-owner-after (fileio-owner-snapshot))
                   (setcar fileio-owner-shared 'worker-mutated)
                   (mutex-unlock fileio-owner-gate))))
             (thread-yield)
             (setq fileio-owner-main-parked (fileio-owner-snapshot)
                   fileio-owner-was-blocked
                   (eq (thread--blocker fileio-owner-worker) fileio-owner-gate))
             (mutex-unlock fileio-owner-gate)
             (thread-join fileio-owner-worker)
             (list fileio-owner-initial fileio-owner-before fileio-owner-after
                   fileio-owner-main-parked (fileio-owner-snapshot)
                   fileio-owner-was-blocked (car fileio-owner-shared)))"#,
        )
        .expect("real Green owners must complete the mutex handoff");
    let values = list_to_vec(&result).expect("proper observation list");
    assert_eq!(values.len(), 7);
    for (index, kind, mode) in [
        (0, "ordinary", 0),
        (1, "active", 0o600),
        (2, "active", 0o600),
        (3, "active", 0o640),
        (4, "active", 0o640),
    ] {
        let observed = list_to_vec(&values[index]).expect("executed native snapshot");
        assert_eq!(
            observed,
            vec![
                Value::fixnum(heap as i64),
                Value::symbol(kind),
                Value::fixnum(mode)
            ]
        );
    }
    assert_eq!(
        values[5],
        Value::T,
        "worker actually parked on main's mutex"
    );
    assert_eq!(
        values[6],
        Value::symbol("worker-mutated"),
        "both owners share the real Lisp graph"
    );
    assert_eq!(
        context.auto_save_native_write_policy(),
        AutoSaveNativeWritePolicy::Active {
            creation_mode: 0o640
        },
        "a different owner's execution must not overwrite main's native policy",
    );
}

#[test]
fn auto_save_native_policy_is_reconstructed_as_ordinary_after_dump() {
    let mut context = crate::test_utils::new_context();
    context
        .eval_str("(defvar fileio-owner-dump-marker '(preserved lisp state))")
        .expect("seed actual dump state");
    context.replace_auto_save_native_write_policy(AutoSaveNativeWritePolicy::Active {
        creation_mode: 0o640,
    });
    let directory = tempfile::tempdir().expect("dump directory");
    let path = directory.path().join("fileio-owner.pdump");
    dump_to_file(&context, &path).expect("dump while native policy is active");
    let mut loaded = load_from_dump(&path).expect("reconstruct fresh Context from actual dump");
    assert_eq!(
        loaded.auto_save_native_write_policy(),
        AutoSaveNativeWritePolicy::Ordinary { preserved_mode: 0 },
        "transient active policy and mode bits must not be serialized",
    );
    assert_eq!(
        crate::emacs_core::print::print_value(
            &loaded
                .eval_str("fileio-owner-dump-marker")
                .expect("restored Lisp state"),
        ),
        "(preserved lisp state)",
    );
    assert_eq!(
        loaded.replace_auto_save_native_write_policy(AutoSaveNativeWritePolicy::Active {
            creation_mode: 0o600,
        }),
        AutoSaveNativeWritePolicy::Ordinary { preserved_mode: 0 },
        "the first reconstructed owner slab must start at C-static zero",
    );
    assert_eq!(
        context.auto_save_native_write_policy(),
        AutoSaveNativeWritePolicy::Active {
            creation_mode: 0o640
        },
        "dump reconstruction must not reset the source owner",
    );
}

#[derive(Debug, PartialEq, Eq)]
struct NativeWritePanic(u32);

fn panic_from_write_handler(context: &mut Context, arguments: Vec<Value>) -> EvalResult {
    assert_eq!(
        arguments.len(),
        8,
        "operation plus all seven write-region arguments"
    );
    assert_eq!(arguments[0], Value::symbol("write-region"));
    assert_eq!(
        context.auto_save_native_write_policy(),
        AutoSaveNativeWritePolicy::Active {
            creation_mode: 0o666
        },
        "the real auto-save scope must be active before invoking the handler",
    );
    context.replace_auto_save_native_write_policy(AutoSaveNativeWritePolicy::Active {
        creation_mode: 0o604,
    });
    panic_any(NativeWritePanic(0x604))
}

#[test]
fn auto_save_native_policy_panic_exit_clears_active_and_preserves_current_mode() {
    let mut context = crate::test_utils::new_context();
    context.register_subr(SubrSpec::new(
        "fileio-owner-panic-handler",
        NativeFn::ContextVec(panic_from_write_handler),
        SubrArity::new(1, None),
    ));
    let directory = tempfile::tempdir().expect("handler directory");
    let target = directory.path().join("fileio-owner-panic-target");
    context
        .eval_str(&format!(
            r#"(progn
             (setq auto-save-list-file-name nil create-lockfiles nil)
             (set-buffer (get-buffer-create "fileio-owner-panic-buffer"))
             (insert "native panic must not leak active auto-save policy")
             (setq buffer-file-name nil
                   buffer-auto-save-file-name {:?})
             (put 'fileio-owner-panic-handler 'operations '(write-region))
             (setq file-name-handler-alist
               '(("fileio-owner-panic-target" . fileio-owner-panic-handler))))"#,
            target.to_str().expect("UTF-8 fixture path"),
        ))
        .expect("prepare real target handler");
    let panic = catch_unwind(AssertUnwindSafe(|| context.eval_str("(do-auto-save t t)")))
        .expect_err("the actual handler panic must resume beyond the native policy scope");
    assert_eq!(
        *panic
            .downcast::<NativeWritePanic>()
            .expect("the original typed panic payload"),
        NativeWritePanic(0x604),
    );
    assert_eq!(
        context.auto_save_native_write_policy(),
        AutoSaveNativeWritePolicy::Ordinary {
            preserved_mode: 0o604
        },
        "panic exit clears the flag but preserves the current, not entry, mode bits",
    );
}
