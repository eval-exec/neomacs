//! Signal-time depth reserves follow the admitted owner's binding channel.

use crate::emacs_core::eval::{Context, ThreadingMode};
use neovm_value::value::Value;

#[test]
fn green_signal_hook_reads_projected_integer_depth_after_admission() {
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    let result = context.eval_str(
        "(progn
           (setq green-depth-hook-ran nil)
           (thread-join (make-thread (lambda () nil)))
           (setq signal-hook-function
                 (lambda (_kind _data) (setq green-depth-hook-ran t)))
           (condition-case nil (car 7) (error nil))
           (setq signal-hook-function nil)
           green-depth-hook-ran)",
    );
    context.green_teardown();
    assert_eq!(result.unwrap(), Value::T);
    assert!(context.specpdl.is_empty());
}

fn reserve_binding(mut context: Context, localized: bool) {
    context.set_lexical_binding(true);
    context
        .eval_str(
            "(progn
           (setq green-depth-writes nil)
           (defalias 'green-depth-watcher
             (lambda (_symbol _value operation _where)
               (setq green-depth-writes (cons operation green-depth-writes))))
           (setq debugger
             (lambda (&rest _args)
               (setq green-depth-seen
                 (list max-lisp-eval-depth lisp-eval-depth-reserve))
               (thread-join (make-thread (lambda ()
                 (list max-lisp-eval-depth lisp-eval-depth-reserve)))))))",
        )
        .unwrap();
    if localized {
        context
            .eval_str(
                "(progn
               (set (make-local-variable 'max-lisp-eval-depth) 400)
               (set (make-local-variable 'lisp-eval-depth-reserve) 80))",
            )
            .unwrap();
    } else {
        context.set_variable("max-lisp-eval-depth", Value::fixnum(400));
        context.set_variable("lisp-eval-depth-reserve", Value::fixnum(80));
    }
    context
        .eval_str("(thread-join (make-thread (lambda () nil)))")
        .unwrap();
    context
        .eval_str(
            "(progn
           (add-variable-watcher 'max-lisp-eval-depth #'green-depth-watcher)
           (add-variable-watcher 'lisp-eval-depth-reserve #'green-depth-watcher))",
        )
        .unwrap();
    let saved = context.specpdl.len();
    context
        .specbind_uncached(
            neovm_value::intern::intern("max-lisp-eval-depth"),
            Value::fixnum(100),
        )
        .unwrap();
    context
        .specbind_uncached(
            neovm_value::intern::intern("lisp-eval-depth-reserve"),
            Value::fixnum(200),
        )
        .unwrap();
    context.set_variable("green-depth-writes", Value::NIL);
    context.depth = 100;
    let result = context.call_debugger(vec![Value::symbol("error"), Value::NIL]);
    context.depth = 0;
    let seen = context
        .default_value_only_by_name("green-depth-seen")
        .unwrap();
    let restored_limit = context
        .variable_value_in_current_buffer(neovm_value::intern::intern("max-lisp-eval-depth"))
        .unwrap();
    let restored_reserve = context
        .variable_value_in_current_buffer(neovm_value::intern::intern("lisp-eval-depth-reserve"))
        .unwrap();
    let writes = context
        .default_value_only_by_name("green-depth-writes")
        .unwrap();
    let outcome = result.unwrap();
    assert_eq!(crate::emacs_core::print::print_value(&seen), "(300 0)");
    // A peer has no binding on the borrowing owner's channel.
    assert_eq!(crate::emacs_core::print::print_value(&outcome), "(400 80)");
    assert_eq!(restored_limit, Value::fixnum(100));
    assert_eq!(restored_reserve, Value::fixnum(200));
    assert_eq!(
        writes,
        Value::NIL,
        "temporary native reserve writes bypass watchers"
    );
    context.unbind_to_result(saved).unwrap();
    context.green_teardown();
    assert_eq!(context.max_depth, 400);
    assert!(context.specpdl.is_empty());
}

#[derive(Clone, Copy)]
enum CanonicalDepthStorage {
    Forwarded,
    LocalizedDefault,
    LocalizedLocal,
}

fn reserve_canonical(mut context: Context, storage: CanonicalDepthStorage, switch_buffer: bool) {
    use crate::emacs_core::eval::owner_bindings::access_channel;
    use neovm_value::intern::intern;

    context
        .eval_str(
            "(progn
               (setq green-depth-source (get-buffer-create \" *green-depth-source*\")
                     green-depth-target (get-buffer-create \" *green-depth-target*\"))
               (set-buffer green-depth-source)
               (setq max-lisp-eval-depth 100 lisp-eval-depth-reserve 200))",
        )
        .unwrap();
    match storage {
        CanonicalDepthStorage::Forwarded => {}
        CanonicalDepthStorage::LocalizedDefault => {
            context
                .eval_str(
                    "(progn
                       (set-buffer green-depth-target)
                       (set (make-local-variable 'max-lisp-eval-depth) 150)
                       (set (make-local-variable 'lisp-eval-depth-reserve) 40)
                       (set-buffer green-depth-source))",
                )
                .unwrap();
        }
        CanonicalDepthStorage::LocalizedLocal => {
            context
                .eval_str(
                    "(progn
                       (set-default 'max-lisp-eval-depth 400)
                       (set-default 'lisp-eval-depth-reserve 80)
                       (set (make-local-variable 'max-lisp-eval-depth) 100)
                       (set (make-local-variable 'lisp-eval-depth-reserve) 200)
                       (set-buffer green-depth-target)
                       (set (make-local-variable 'max-lisp-eval-depth) 150)
                       (set (make-local-variable 'lisp-eval-depth-reserve) 40)
                       (set-buffer green-depth-source))",
                )
                .unwrap();
        }
    }
    context
        .eval_str(
            "(progn
               (thread-join (make-thread (lambda () nil)))
               (setq green-depth-writes nil)
               (defalias 'green-depth-watcher
                 (lambda (_symbol _value operation _where)
                   (setq green-depth-writes (cons operation green-depth-writes))))
               (add-variable-watcher 'max-lisp-eval-depth #'green-depth-watcher)
               (add-variable-watcher 'lisp-eval-depth-reserve #'green-depth-watcher)
               (setq debugger
                 (lambda (&rest _args)
                   (setq green-depth-seen
                     (list max-lisp-eval-depth lisp-eval-depth-reserve))
                   (if green-depth-switch-buffer (set-buffer green-depth-target))
                   (setq green-depth-selected
                     (list max-lisp-eval-depth lisp-eval-depth-reserve)))))",
        )
        .unwrap();
    context.set_variable("green-depth-switch-buffer", Value::bool(switch_buffer));
    let ids = [
        intern("max-lisp-eval-depth"),
        intern("lisp-eval-depth-reserve"),
    ];
    // These calls must reach canonical storage, rather than either owner-bound
    // branch already exercised by reserve_binding. A Lisp-void slot is distinct
    // from an owner having no binding on this channel.
    let assert_canonical = |context: &Context| {
        for id in ids {
            let projection = context.obarray.thread_projection_of(id).unwrap();
            let channel = access_channel(&context.buffers, id, projection);
            assert!(
                context
                    .owner
                    .slab()
                    .unwrap()
                    .slot(projection.index(), channel)
                    .value()
                    .is_none()
            );
        }
    };
    assert_canonical(&context);
    context.depth = 100;
    let result = context.call_debugger(vec![Value::symbol("error"), Value::NIL]);
    context.depth = 0;
    result.unwrap();
    assert_canonical(&context);
    assert_eq!(
        crate::emacs_core::print::print_value(
            &context
                .default_value_only_by_name("green-depth-seen")
                .unwrap()
        ),
        "(300 0)"
    );
    let expected_reserve = if switch_buffer { 90 } else { 200 };
    for (id, expected) in ids.into_iter().zip([100, expected_reserve]) {
        assert_eq!(
            context.variable_value_in_current_buffer(id),
            Some(Value::fixnum(expected))
        );
        // Localized storage retains both its native scalar and canonical cons.
        assert_eq!(
            context
                .obarray
                .thread_projection_of(id)
                .unwrap()
                .logical_forwarder()
                .unwrap()
                .as_int_fwd()
                .unwrap()
                .get_i64(),
            expected
        );
    }
    assert_eq!(context.max_depth, 100);
    assert_eq!(
        context.default_value_only_by_name("green-depth-writes"),
        Some(Value::NIL),
        "canonical temporary reserve writes bypass watchers"
    );
    if switch_buffer {
        assert_eq!(
            crate::emacs_core::print::print_value(
                &context
                    .eval_str(
                        "(list green-depth-selected
                               (eq (current-buffer) green-depth-target)
                               (buffer-local-value 'max-lisp-eval-depth green-depth-source)
                               (buffer-local-value 'lisp-eval-depth-reserve green-depth-source)
                               (default-value 'max-lisp-eval-depth)
                               (default-value 'lisp-eval-depth-reserve))"
                    )
                    .unwrap()
            ),
            "((150 40) t 300 0 400 80)"
        );
    } else if matches!(storage, CanonicalDepthStorage::LocalizedDefault) {
        assert_eq!(
            context
                .eval_str(
                    "(or (local-variable-p 'max-lisp-eval-depth)
                         (local-variable-p 'lisp-eval-depth-reserve))"
                )
                .unwrap(),
            Value::NIL,
            "native reserve writes do not create a local binding"
        );
    }
    context.green_teardown();
    assert!(context.specpdl.is_empty());
}

#[test]
fn green_signal_reserve_borrows_owner_default_binding_without_watchers() {
    // Native table bootstrap must finish for every Context before the first
    // admission captures those tables for a World.
    let [bound, canonical]: [Context; 2] =
        std::array::from_fn(|_| Context::new_with_threading(ThreadingMode::Green));
    reserve_binding(bound, false);
    reserve_canonical(canonical, CanonicalDepthStorage::Forwarded, false);
}

#[test]
fn green_signal_reserve_borrows_owner_local_binding_without_watchers() {
    let [bound, default, local, switched]: [Context; 4] =
        std::array::from_fn(|_| Context::new_with_threading(ThreadingMode::Green));
    reserve_binding(bound, true);
    reserve_canonical(default, CanonicalDepthStorage::LocalizedDefault, false);
    reserve_canonical(local, CanonicalDepthStorage::LocalizedLocal, false);
    reserve_canonical(switched, CanonicalDepthStorage::LocalizedLocal, true);
}
