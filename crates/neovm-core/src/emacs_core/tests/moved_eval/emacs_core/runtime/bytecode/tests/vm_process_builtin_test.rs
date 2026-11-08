//! Process-builtin VM tests re-homed out of neovm-eval.
//!
//! These tests name the system process surface. Eval cannot depend on that
//! crate. Test names and assertions are unchanged. The bare harness is
//! core's `new_context`: eval's unit-runtime module must stay `cfg(test)`
//! (the T5 absent-layer scan), so a dependency build cannot see it.
//! The process-manager field is `pub(crate)`; the tests call `processes_mut`.
//! Buffer state is the public `edit.buffers` field.

use neovm_value::value::Value;

use crate::emacs_core::Context;
use crate::emacs_core::process::{ProcessCodingSystemsExt, SystemProcessExt};

fn vm_eval_str(src: &str) -> String {
    let mut eval = crate::test_utils::new_context();
    eval.set_lexical_binding(false);
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

fn vm_eval_with_init_str(src: &str, init: impl FnOnce(&mut Context)) -> String {
    let mut eval = crate::test_utils::new_context();
    init(&mut eval);
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

fn vm_eval_interactive_str(src: &str) -> String {
    let mut eval = crate::test_utils::new_context();
    eval.set_variable("noninteractive", Value::NIL);
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

fn vm_eval_interactive_with_init_str(src: &str, init: impl FnOnce(&mut Context)) -> String {
    let mut eval = crate::test_utils::new_context();
    eval.set_variable("noninteractive", Value::NIL);
    init(&mut eval);
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

fn vm_bootstrap_eval_with_init_str(src: &str, init: impl FnOnce(&mut Context)) -> String {
    let mut eval = crate::test_utils::runtime_startup_context();
    init(&mut eval);
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
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
            let _buffer_id = eval.edit.buffers.create_buffer("*vm-proc*");
            let buffer = eval
                .edit
                .buffers
                .find_buffer_by_name("*vm-proc*")
                .expect("process buffer");
            let _pid = eval.processes_mut().create_process(
                "vm-proc".into(),
                Value::make_buffer(buffer),
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
            let _buffer_id = eval.edit.buffers.create_buffer("*vm-stale-proc*");
            let buffer = eval
                .edit
                .buffers
                .find_buffer_by_name("*vm-stale-proc*")
                .expect("process buffer");
            let pid = eval.processes_mut().create_process(
                "vm-stale-proc".into(),
                Value::make_buffer(buffer),
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
            let pid = eval.processes_mut().create_process(
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
            eval.processes_mut()
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
            let pid = eval.processes_mut().create_process(
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
            eval.processes_mut()
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
            let pid = eval.processes_mut().create_process(
                "vm-proc-coding".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
            let pipe_id = eval.processes_mut().create_process_with_kind(
                "vm-proc-pipe".into(),
                Value::NIL,
                String::new(),
                vec![],
                crate::emacs_core::process::ProcessKindWithoutDevice::Pipe,
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pipe_id, 2);
            let network_id = eval.processes_mut().create_process_with_kind(
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
            let pid = eval.processes_mut().create_process(
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

            let real = eval.processes_mut().create_process(
                "vm-status-real".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(real, 1);
            let pipe = eval.processes_mut().create_process_with_kind(
                "vm-status-pipe".into(),
                Value::NIL,
                String::new(),
                vec![],
                ProcessKindWithoutDevice::Pipe,
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pipe, 2);
            let network = eval.processes_mut().create_process_with_kind(
                "vm-status-network".into(),
                Value::NIL,
                String::new(),
                vec![],
                ProcessKindWithoutDevice::Network,
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(network, 3);
            // Mark as server so process-status returns 'listen (not 'open).
            eval.processes_mut().get_mut(network).unwrap().childp =
                Value::list(vec![Value::keyword(":server"), Value::T]);
            let stopped = eval.processes_mut().create_process(
                "vm-status-stop".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(stopped, 4);
            let signaled = eval.processes_mut().create_process(
                "vm-status-signal".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(signaled, 5);
            eval.processes_mut().get_any_mut(stopped).unwrap().status =
                Value::list(vec![Value::symbol("stop"), Value::fixnum(0)]);
            eval.processes_mut().get_any_mut(signaled).unwrap().status =
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
            let pid = eval.processes_mut().create_process(
                "vm-status-stale".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
            eval.processes_mut().get_any_mut(pid).unwrap().status =
                Value::list(vec![Value::symbol("signal"), Value::fixnum(9), Value::NIL]);
        },
    );
    assert_eq!(result, "OK (t t t t)");
}

#[test]
fn vm_process_control_and_send_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();

    let buffer_id = eval.edit.buffers.create_buffer("*vm-proc-control*");
    eval.edit.buffers.set_current(buffer_id);
    eval.edit
        .buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("abc");

    let current_id = eval.processes_mut().create_process(
        "vm-proc-current".into(),
        Value::make_buffer(buffer_id),
        "/bin/cat".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    assert_eq!(current_id, 1);
    eval.processes_mut()
        .get_mut(current_id)
        .expect("current process")
        .status = Value::list(vec![Value::symbol("stop"), Value::fixnum(0)]);

    for expected in 2..=7 {
        let id = eval.processes_mut().create_process(
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
               ;; Synthetic records start before coding setup; the send target
               ;; carries bytes, so initialize it through the public setter.
               (set-process-coding-system p7 'binary 'binary)
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
        .unwrap_or_else(|err| {
            panic!(
                "process control/send builtins should execute: {}",
                crate::emacs_core::format_eval_result(&Err(err))
            )
        });

    assert_eq!(
        crate::emacs_core::format_eval_result(&Ok(result)),
        "OK (t t t t t t t t t t t t t t t t t)"
    );
    assert!(neovm_value::value::equal_value(
        &eval
            .processes_mut()
            .get(7)
            .expect("send target process")
            .write_queue,
        &Value::list(vec![
            Value::cons(
                Value::heap_string(neovm_value::heap_types::LispString::from_utf8("hello")),
                Value::cons(Value::fixnum(0), Value::fixnum(5)),
            ),
            Value::cons(
                Value::heap_string(neovm_value::heap_types::LispString::from_utf8("abc")),
                Value::cons(Value::fixnum(0), Value::fixnum(3)),
            ),
            // GNU `process-send-eof` queues an unencoded EOT after all
            // already accepted input when the process uses a PTY.
            Value::cons(
                Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
                    0x04
                ])),
                Value::cons(Value::fixnum(0), Value::fixnum(1)),
            ),
        ]),
        0,
    ));
    assert_eq!(
        eval.processes_mut()
            .get_any(1)
            .expect("current process")
            .status,
        Value::symbol("run")
    );
    assert_eq!(
        eval.processes_mut()
            .get_any(2)
            .expect("interrupt process")
            .status,
        Value::list(vec![Value::symbol("signal"), Value::fixnum(2), Value::NIL])
    );
    assert_eq!(
        eval.processes_mut()
            .get_any(3)
            .expect("kill process")
            .status,
        Value::list(vec![Value::symbol("signal"), Value::fixnum(9), Value::NIL])
    );
    assert_eq!(
        eval.processes_mut()
            .get_any(4)
            .expect("stop process")
            .status,
        Value::symbol("run")
    );
    assert_eq!(
        eval.processes_mut()
            .get_any(5)
            .expect("quit process")
            .status,
        Value::symbol("run")
    );
    assert_eq!(
        eval.processes_mut()
            .get_any(6)
            .expect("signal process")
            .status,
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
            let buffer_id = eval.edit.buffers.create_buffer("*vm-stale-proc-control*");
            eval.edit.buffers.set_current(buffer_id);
            eval.edit
                .buffers
                .current_buffer_mut()
                .expect("current buffer")
                .insert("abc");
            let pid = eval.processes_mut().create_process(
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
            let buffer_id = eval.edit.buffers.create_buffer("*vm-delete-proc*");
            eval.edit.buffers.set_current(buffer_id);
            let pid = eval.processes_mut().create_process(
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
            let pid = eval.processes_mut().create_process(
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
    // GNU's serial_configure reads the device with tcgetattr first and reports
    // its failure (src/sysdep.c:3164-3166).  Linux initialises a pty master
    // when it is opened, so /dev/ptmx configures there; Darwin initialises the
    // tty only when its slave is first opened (measured by this tree's
    // SerialTestPty fixture), so the same port signals
    // (file-error "Failed tcgetattr" ...) under GNU and NeoVM alike on Darwin.
    let serial_pty_probe = if cfg!(target_os = "macos") {
        r#"(condition-case err
                     (let ((p (make-serial-process :name "sp" :port "/dev/ptmx" :speed 9600)))
                       (list (processp p)
                             (eq (process-type p) 'serial)))
                   (error err))"#
    } else {
        r#"(let ((p (make-serial-process :name "sp" :port "/dev/ptmx" :speed 9600)))
                     (list (processp p)
                           (eq (process-type p) 'serial)))"#
    };
    let serial_pty_row = if cfg!(target_os = "macos") {
        // Darwin reports ENOTTY from tcgetattr on a pty master whose slave
        // has never been opened.  GNU passes the host's strerror text through
        // (report_file_errno, src/fileio.c:286-300), so build the row from the
        // same host call rather than pinning one libc's words.
        format!(
            "(file-error \"Failed tcgetattr\" \"{}\")",
            crate::emacs_core::errno::emacs_strerror(libc::ENOTTY)
        )
    } else {
        "(t t)".to_owned()
    };
    assert_eq!(
        vm_eval_str(&format!(
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
                 {serial_pty_probe})"#
        )),
        format!(
            "OK (nil (wrong-type-argument stringp nil) (error \":name value not a string\") (error \"Missing :name keyword parameter\") (t t t) nil (t t t t) (error \":name value not a string\") nil (wrong-type-argument stringp t) (wrong-type-argument stringp 1) (error \"No port specified\") (error \":speed not specified\") {serial_pty_row})"
        )
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

            let buffer_id = eval.edit.buffers.create_buffer("*vm-serial-proc*");
            eval.edit.buffers.set_current(buffer_id);
            // A serial process record cannot be fabricated any more: the
            // constructor takes an OPEN device.  `/dev/ptmx` is the real
            // character device the serial pins use -- see DIVERGENCES.md 147.
            let device = crate::emacs_core::process::sys::open_serial_port_for_test(
                std::ffi::OsStr::new("/dev/ptmx"),
            )
            .expect("/dev/ptmx opens on any Linux");
            let serial_id = eval.processes_mut().create_serial_process(
                neovm_value::heap_types::LispString::from_utf8("vm-serial"),
                Value::make_buffer(buffer_id),
                device,
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(serial_id, 1);
            let real_id = eval.processes_mut().create_process(
                "vm-real".into(),
                Value::NIL,
                "/bin/cat".into(),
                vec![],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(real_id, 2);
            let network_id = eval.processes_mut().create_process_with_kind(
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
            let pid = eval.processes_mut().create_process(
                "vm-accept-process".into(),
                Value::NIL,
                "echo".into(),
                vec!["out".into()],
                crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
            );
            assert_eq!(pid, 1);
            eval.processes_mut()
                .spawn_child(pid, false)
                .expect("spawn child");
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
            let pid = eval.processes_mut().create_process(
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
                 (list (format-mode-line "%i|%I|%s" 0)
                       (progn
                         (format-mode-line "%i|%I|%s" 0))))"#,
            |eval| {
                let buffer_id = eval.edit.buffers.create_buffer("vm-mode-line-metadata");
                eval.processes_mut().create_process(
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
                 (format-mode-line "%i|%I|%s" 0))"#
        ),
        r#"OK "1536|1.5k|no process""#
    );
}
