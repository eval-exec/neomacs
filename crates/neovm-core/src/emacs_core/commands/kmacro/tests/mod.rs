use super::super::intern::intern;
use super::*;
use crate::emacs_core::autoload::is_autoload_value;
use crate::emacs_core::builtins::plain_str_to_lisp_string;
use crate::emacs_core::eval::Context;
use crate::emacs_core::format_eval_result;
use crate::test_utils::{eval_with_ldefs_boot_autoloads, runtime_startup_eval_all};

fn bootstrap_eval_all(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

fn kmacro_string(text: &str) -> crate::heap_types::LispString {
    plain_str_to_lisp_string(text, true)
}

// -----------------------------------------------------------------------
// Kmacro metadata / keyboard runtime tests
// -----------------------------------------------------------------------

#[test]
fn new_manager_defaults() {
    crate::test_utils::init_test_tracing();
    let mgr = KmacroManager::new();
    assert!(mgr.macro_ring.is_empty());
    assert_eq!(mgr.counter, 0);
    assert_eq!(mgr.counter_format, kmacro_string("%d"));
}

#[test]
fn keyboard_runtime_finalize_and_cancel_match_gnu_macro_boundary_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    builtin_start_kbd_macro(&mut eval, vec![]).expect("start");
    assert!(eval.command_loop.keyboard.kboard.defining_kbd_macro);
    assert_eq!(
        eval.eval_symbol("defining-kbd-macro")
            .expect("defining-kbd-macro"),
        Value::T
    );

    builtin_store_kbd_macro_event(&mut eval, vec![Value::char('a')]).expect("store a");
    eval.finalize_kbd_macro_runtime_chars();
    builtin_store_kbd_macro_event(&mut eval, vec![Value::char('b')]).expect("store b");
    crate::emacs_core::builtins::builtin_cancel_kbd_macro_events(&mut eval, vec![])
        .expect("cancel current command events");
    builtin_end_kbd_macro(&mut eval, vec![]).expect("end");

    assert_eq!(
        eval.command_loop.last_kbd_macro(),
        Some([Value::char('a')].as_slice())
    );
    assert_eq!(
        builtin_last_kbd_macro(&mut eval, vec![]).expect("last-kbd-macro"),
        Value::vector(vec![Value::char('a')])
    );
    assert_eq!(
        eval.eval_symbol("last-kbd-macro")
            .expect("last-kbd-macro var"),
        Value::vector(vec![Value::char('a')])
    );
    assert_eq!(
        eval.eval_symbol("defining-kbd-macro")
            .expect("defining-kbd-macro"),
        Value::NIL
    );
}

#[test]
fn start_and_end_kbd_macro_publish_gnu_status_messages() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.assign("noninteractive", Value::NIL);

    builtin_start_kbd_macro(&mut eval, vec![]).expect("start macro");
    assert_eq!(
        eval.current_message_text().as_deref(),
        Some("Defining kbd macro...")
    );

    builtin_end_kbd_macro(&mut eval, vec![]).expect("end macro");
    assert_eq!(
        eval.current_message_text().as_deref(),
        Some("Keyboard macro defined")
    );
}

#[test]
fn macro_ring_pushes_previous_keyboard_runtime_macro() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    builtin_start_kbd_macro(&mut eval, vec![]).expect("start first");
    builtin_store_kbd_macro_event(&mut eval, vec![Value::char('a')]).expect("store a");
    eval.finalize_kbd_macro_runtime_chars();
    builtin_end_kbd_macro(&mut eval, vec![]).expect("end first");
    assert!(eval.kmacro.macro_ring.is_empty());

    builtin_start_kbd_macro(&mut eval, vec![]).expect("start second");
    builtin_store_kbd_macro_event(&mut eval, vec![Value::char('b')]).expect("store b");
    eval.finalize_kbd_macro_runtime_chars();
    builtin_end_kbd_macro(&mut eval, vec![]).expect("end second");
    assert_eq!(eval.kmacro.macro_ring, vec![vec![Value::char('a')]]);

    builtin_start_kbd_macro(&mut eval, vec![]).expect("start third");
    builtin_store_kbd_macro_event(&mut eval, vec![Value::char('c')]).expect("store c");
    eval.finalize_kbd_macro_runtime_chars();
    builtin_end_kbd_macro(&mut eval, vec![]).expect("end third");
    assert_eq!(
        eval.kmacro.macro_ring,
        vec![vec![Value::char('a')], vec![Value::char('b')]]
    );
}

#[test]
fn format_counter_decimal() {
    crate::test_utils::init_test_tracing();
    let mgr = KmacroManager {
        counter: 42,
        counter_format: kmacro_string("%d"),
        ..KmacroManager::new()
    };
    assert_eq!(mgr.format_counter(), "42");
}

#[test]
fn format_counter_hex() {
    crate::test_utils::init_test_tracing();
    let mgr = KmacroManager {
        counter: 255,
        counter_format: kmacro_string("%x"),
        ..KmacroManager::new()
    };
    assert_eq!(mgr.format_counter(), "ff");
}

#[test]
fn format_counter_octal() {
    crate::test_utils::init_test_tracing();
    let mgr = KmacroManager {
        counter: 8,
        counter_format: kmacro_string("%o"),
        ..KmacroManager::new()
    };
    assert_eq!(mgr.format_counter(), "10");
}

#[test]
fn format_counter_with_prefix() {
    crate::test_utils::init_test_tracing();
    let mgr = KmacroManager {
        counter: 7,
        counter_format: kmacro_string("item-%d"),
        ..KmacroManager::new()
    };
    assert_eq!(mgr.format_counter(), "item-7");
}

#[test]
fn format_counter_unknown_format() {
    crate::test_utils::init_test_tracing();
    let mgr = KmacroManager {
        counter: 99,
        counter_format: kmacro_string("???"),
        ..KmacroManager::new()
    };
    // Fallback to plain decimal
    assert_eq!(mgr.format_counter(), "99");
}

// -----------------------------------------------------------------------
// Builtin-level tests
// -----------------------------------------------------------------------

#[test]
fn test_start_and_end_macro() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // Start recording
    let result = builtin_start_kbd_macro(&mut eval, vec![]);
    assert!(result.is_ok());
    assert!(eval.command_loop.keyboard.kboard.defining_kbd_macro);

    // Double-start should error
    let result = builtin_start_kbd_macro(&mut eval, vec![]);
    assert!(result.is_err());

    // Store some events
    let _ = builtin_store_kbd_macro_event(&mut eval, vec![Value::char('h')]);
    let _ = builtin_store_kbd_macro_event(&mut eval, vec![Value::char('i')]);
    eval.finalize_kbd_macro_runtime_chars();

    // End recording
    let result = builtin_end_kbd_macro(&mut eval, vec![]);
    assert!(result.is_ok());
    assert!(!eval.command_loop.keyboard.kboard.defining_kbd_macro);
    assert_eq!(
        eval.command_loop.last_kbd_macro(),
        Some([Value::char('h'), Value::char('i')].as_slice())
    );

    // Double-end should error
    let result = builtin_end_kbd_macro(&mut eval, vec![]);
    assert!(result.is_err());
}

/// `start-kbd-macro`'s contract.
///
/// This used to be spelled `defining-kbd-macro` and to call a second Rust subr
/// of that name.  GNU has no such `DEFUN`: `lisp/help.el:356` is
/// `(fset 'defining-kbd-macro (symbol-function 'start-kbd-macro))`, so in GNU
/// the two names share ONE subr object and `(subr-name (symbol-function
/// 'defining-kbd-macro))` answers "start-kbd-macro".  Ledger 190 deleted the
/// duplicate; the contract it checked is `start-kbd-macro`'s, and is checked
/// here under that name.
#[test]
fn test_start_kbd_macro_builtin_contract() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // Arity contract.  Only the upper bound is checked in the function body:
    // `start-kbd-macro` is registered `(1 . 2)` -- GNU's
    // `DEFUN ("start-kbd-macro", ..., 1, 2, "P", ...)` (`src/macros.c:43`) --
    // and the MIN is enforced by the dispatcher, so a direct call with no
    // arguments reaches the body and records with APPEND nil.  The registered
    // arity itself is asserted in `subr/info_tests.rs`.
    assert!(builtin_start_kbd_macro(&mut eval, vec![Value::NIL, Value::NIL, Value::NIL]).is_err());

    // APPEND with no prior macro should signal wrong-type-argument.
    let append_without_last = builtin_start_kbd_macro(&mut eval, vec![Value::T]);
    assert!(append_without_last.is_err());

    // Fresh recording with APPEND=nil should succeed.
    assert_eq!(
        builtin_start_kbd_macro(&mut eval, vec![Value::NIL]).unwrap(),
        Value::NIL
    );
    assert!(eval.command_loop.keyboard.kboard.defining_kbd_macro);

    // Re-entry while recording should signal `error`.
    let already = builtin_start_kbd_macro(&mut eval, vec![Value::NIL, Value::T]);
    assert!(already.is_err());

    // Finish recording and ensure append path works once a last macro exists.
    let _ = builtin_store_kbd_macro_event(&mut eval, vec![Value::char('a')]);
    eval.finalize_kbd_macro_runtime_chars();
    let _ = builtin_end_kbd_macro(&mut eval, vec![]);
    assert_eq!(
        eval.command_loop.last_kbd_macro(),
        Some([Value::char('a')].as_slice())
    );
    assert_eq!(
        builtin_start_kbd_macro(&mut eval, vec![Value::T, Value::T]).unwrap(),
        Value::NIL
    );
    let _ = builtin_end_kbd_macro(&mut eval, vec![]);
}

#[test]
fn test_start_with_append() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // Record a macro
    let _ = builtin_start_kbd_macro(&mut eval, vec![]);
    let _ = builtin_store_kbd_macro_event(&mut eval, vec![Value::char('a')]);
    eval.finalize_kbd_macro_runtime_chars();
    let _ = builtin_end_kbd_macro(&mut eval, vec![]);

    // Append to it
    let _ = builtin_start_kbd_macro(&mut eval, vec![Value::T, Value::T]);
    assert_eq!(eval.command_loop.keyboard.kboard.kbd_macro_events.len(), 1);
    let _ = builtin_store_kbd_macro_event(&mut eval, vec![Value::char('b')]);
    eval.finalize_kbd_macro_runtime_chars();
    let _ = builtin_end_kbd_macro(&mut eval, vec![]);

    assert_eq!(
        eval.command_loop.last_kbd_macro(),
        Some([Value::char('a'), Value::char('b')].as_slice())
    );
}

#[test]
fn test_start_with_append_reexecutes_last_macro_when_no_exec_is_nil() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let _ = eval.eval_str_each(
        "(progn
           (setq kmacro-append-count 0)
           (setq kmacro-append-ignore-direct-called nil)
           (fset 'command-execute
                 (lambda (cmd &optional _record _keys _special)
                   (funcall cmd)))
           (fset 'ignore
                 (lambda ()
                   (setq kmacro-append-ignore-direct-called t)))
           (let ((g (make-sparse-keymap)))
             (use-global-map g)
             (define-key g [ignore]
               (lambda ()
                 (interactive)
                 (setq kmacro-append-count (1+ kmacro-append-count))))))",
    );

    builtin_start_kbd_macro(&mut eval, vec![]).expect("start");
    builtin_store_kbd_macro_event(&mut eval, vec![Value::symbol("ignore")]).expect("store");
    eval.finalize_kbd_macro_runtime_chars();
    builtin_end_kbd_macro(&mut eval, vec![]).expect("end");

    assert_eq!(
        eval.eval_symbol("kmacro-append-count")
            .expect("kmacro-append-count"),
        Value::fixnum(0)
    );

    builtin_start_kbd_macro(&mut eval, vec![Value::T, Value::NIL]).expect("append");
    assert_eq!(
        eval.eval_symbol("kmacro-append-count")
            .expect("kmacro-append-count"),
        Value::fixnum(1)
    );
    assert_eq!(
        eval.command_loop.keyboard.kboard.kbd_macro_events,
        vec![Value::symbol("ignore")]
    );
    assert_eq!(
        eval.eval_symbol("kmacro-append-ignore-direct-called")
            .expect("kmacro-append-ignore-direct-called"),
        Value::NIL
    );
    builtin_end_kbd_macro(&mut eval, vec![]).expect("end append");
    assert_eq!(
        eval.command_loop.last_kbd_macro(),
        Some([Value::symbol("ignore")].as_slice())
    );
}

#[test]
fn test_start_with_append_real_key_macro_reexecutes_via_command_loop_and_marks_append() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let _ = eval.eval_str_each(
        r#"(progn
             (setq kmacro-append-real-count 0)
             (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
             (let ((g (make-sparse-keymap)))
               (use-global-map g)
               (define-key g "a"
                 (lambda ()
                   (interactive)
                   (setq kmacro-append-real-count (1+ kmacro-append-real-count))))))"#,
    );

    builtin_start_kbd_macro(&mut eval, vec![]).expect("start");
    builtin_store_kbd_macro_event(&mut eval, vec![Value::char('a')]).expect("store a");
    eval.finalize_kbd_macro_runtime_chars();
    builtin_end_kbd_macro(&mut eval, vec![]).expect("end");

    assert_eq!(
        eval.eval_symbol("kmacro-append-real-count")
            .expect("kmacro-append-real-count"),
        Value::fixnum(0)
    );

    builtin_start_kbd_macro(&mut eval, vec![Value::T, Value::NIL]).expect("append");
    assert_eq!(
        eval.eval_symbol("kmacro-append-real-count")
            .expect("kmacro-append-real-count"),
        Value::fixnum(1)
    );
    assert_eq!(
        eval.eval_symbol("defining-kbd-macro")
            .expect("defining-kbd-macro"),
        Value::symbol("append")
    );
    builtin_end_kbd_macro(&mut eval, vec![]).expect("end append");
}

#[test]
fn test_start_with_append_no_exec_skips_reexecution() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let _ = eval.eval_str_each(
        "(progn
           (setq kmacro-no-exec-count 0)
           (fset 'kmacro-no-exec-bump
                 (lambda ()
                   (setq kmacro-no-exec-count (1+ kmacro-no-exec-count)))))",
    );

    builtin_start_kbd_macro(&mut eval, vec![]).expect("start");
    builtin_store_kbd_macro_event(&mut eval, vec![Value::symbol("kmacro-no-exec-bump")])
        .expect("store");
    eval.finalize_kbd_macro_runtime_chars();
    builtin_end_kbd_macro(&mut eval, vec![]).expect("end");

    builtin_start_kbd_macro(&mut eval, vec![Value::T, Value::T]).expect("append");
    assert_eq!(
        eval.eval_symbol("kmacro-no-exec-count")
            .expect("kmacro-no-exec-count"),
        Value::fixnum(0)
    );
    builtin_end_kbd_macro(&mut eval, vec![]).expect("end append");
}

#[test]
fn test_call_last_macro_no_macro() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // No macro defined -- should error
    let result = builtin_call_last_kbd_macro(&mut eval, vec![]);
    assert!(result.is_err());
}

#[test]
fn test_store_event_wrong_args() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // Wrong arg count
    let result = builtin_store_kbd_macro_event(&mut eval, vec![]);
    assert!(result.is_err());
}

#[test]
fn test_execute_kbd_macro_restores_outer_execution_state() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let outer = vec![Value::char('o'), Value::char('u')];
    eval.begin_executing_kbd_macro_runtime(outer.clone());
    eval.command_loop.keyboard.kboard.kbd_macro_index = 1;

    builtin_execute_kbd_macro(&mut eval, vec![Value::vector(vec![])])
        .expect("execute nested macro");

    assert_eq!(
        eval.command_loop
            .keyboard
            .kboard
            .executing_kbd_macro
            .as_deref(),
        Some(outer.as_slice())
    );
    assert_eq!(eval.command_loop.keyboard.kboard.kbd_macro_index, 1);
    assert_eq!(
        eval.eval_symbol("executing-kbd-macro-index")
            .expect("executing-kbd-macro-index"),
        Value::fixnum(1)
    );
}

#[test]
fn test_execute_kbd_macro_real_key_events_use_command_loop_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let results = eval.eval_str_each(
        r#"(progn
             (setq kmacro-command-loop-count 0)
             (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
             (let ((g (make-sparse-keymap)))
               (use-global-map g)
               (define-key g "a"
                 (lambda ()
                   (interactive)
                   (setq kmacro-command-loop-count (1+ kmacro-command-loop-count))))
               (execute-kbd-macro "a")
               kmacro-command-loop-count))"#,
    );
    assert_eq!(results.len(), 1);
    assert_eq!(format_eval_result(&results[0]), "OK 1");
}

#[test]
fn execute_kbd_macro_tail_is_not_pending_input_for_while_no_input_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();

    let result = eval
        .eval_str(
            r#"(progn
                 (setq neo-while-no-input-macro-log nil)
                 (defun neo-while-no-input-macro-command ()
                   (interactive)
                   (push (while-no-input 'ran)
                         neo-while-no-input-macro-log))
                 (let ((map (make-sparse-keymap)))
                   (define-key map "a" #'neo-while-no-input-macro-command)
                   (use-local-map map)
                   (execute-kbd-macro "aaa"))
                 neo-while-no-input-macro-log)"#,
        )
        .expect("keyboard macro should execute");

    assert_eq!(format!("{result}"), "(ran ran ran)");
}

/// GNU clears `last_nonmenu_event` at the top of EVERY key-sequence read
/// (keyboard.c:11038-11054, just after the `replay_sequence:` label), and only
/// then assigns the key it read (keyboard.c:11668-11673).  The sequence read
/// that discovers an exhausted keyboard macro therefore leaves the variable
/// nil, which is what Lisp observes after `execute-kbd-macro` returns.
///
/// Measured on GNU Emacs -Q --batch (`tmp/p97-probe5.el`): a command bound to
/// `C-c C-d` invoked twice by one macro sees `last-nonmenu-event` = 4 (C-d)
/// both times, and the variable is nil once the macro finishes.
///
/// The value matters beyond bookkeeping: `imenu-choose-buffer-index`
/// (lisp/imenu.el:915) decides between the mouse menu and the completing-read
/// prompt with `(listp last-nonmenu-event)`, so a stale integer left behind by
/// a macro turns a silent GNU `imenu` into a minibuffer prompt.
#[test]
fn execute_kbd_macro_leaves_last_nonmenu_event_nil_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();

    let result = eval
        .eval_str(
            r#"(progn
                 (setq neo-last-nonmenu-macro-log nil)
                 (defun neo-last-nonmenu-macro-command ()
                   (interactive)
                   (push last-nonmenu-event neo-last-nonmenu-macro-log))
                 (let ((map (make-sparse-keymap)))
                   (define-key map (kbd "C-c C-d")
                     #'neo-last-nonmenu-macro-command)
                   (use-local-map map)
                   (execute-kbd-macro (kbd "C-c C-d C-c C-d")))
                 (list (nreverse neo-last-nonmenu-macro-log)
                       last-nonmenu-event))"#,
        )
        .expect("keyboard macro should execute");

    assert_eq!(format!("{result}"), "((4 4) nil)");
}

/// GNU ends a keyboard-macro iteration inside `read_key_sequence`, whose
/// `done` path calls `echo_update` before it returns an empty sequence to
/// `command_loop_1`.  Neomacs must cross the same key-reader boundary instead
/// of returning early from the command loop: otherwise a transient sub-read
/// echo such as "Zap to char: x" remains current and suppresses kmacro's repeat
/// hint.
#[test]
fn exhausted_kbd_macro_iteration_finalizes_echo_at_key_reader_boundary() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.begin_executing_kbd_macro_runtime(Vec::new());
    eval.eval_str(
        r##"(progn
              (setq neo-macro-echo-clear-hook-count 0)
              (setq echo-area-clear-hook
                    (list (lambda ()
                            (setq neo-macro-echo-clear-hook-count
                                  (1+ neo-macro-echo-clear-hook-count))))))"##,
    )
    .expect("install macro-boundary echo hook probe");
    eval.set_current_message(Some(crate::heap_types::LispString::from_utf8(
        "Zap to char: x",
    )));

    eval.execute_kbd_macro_iteration_via_command_loop()
        .expect("empty macro iteration should end cleanly");

    assert_eq!(eval.current_message_text(), None);
    assert_eq!(
        eval.eval_symbol("neo-macro-echo-clear-hook-count")
            .expect("echo clear hook count"),
        Value::fixnum(0),
        "GNU echo_update clears through message3_nolog, not echo-area-clear-hook"
    );
}

#[test]
fn execute_kbd_macro_continues_in_recursive_minibuffer_command_loop() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let results = eval.eval_str_each(
        r#"(progn
             (setq kmacro-minibuffer-events nil
                   kmacro-minibuffer-result 'unset)
             (fset 'command-execute
                   (lambda (command &optional _record _keys _special)
                     (funcall command)))
             (fset 'kmacro-capture-next
                   (lambda ()
                     (interactive)
                     (setq kmacro-minibuffer-events
                           (append kmacro-minibuffer-events '(next)))))
             (fset 'kmacro-exit-minibuffer
                   (lambda ()
                     (interactive)
                     (throw 'exit nil)))
             (fset 'kmacro-open-minibuffer
                   (lambda ()
                     (interactive)
                     ;; GNU lisp/net/dbus.el's synchronous polling temporarily
                     ;; hides unread-command-events, reads an event, then
                     ;; restores that event for the next command loop.
                     (let ((event
                            (let (unread-command-events)
                              (read-event nil nil 0.001))))
                       (if event
                           (setq unread-command-events
                                 (nconc unread-command-events (list event)))))
                     (let ((event
                            (let (unread-command-events)
                              (read-event nil nil 0.001))))
                       (if event
                           (setq unread-command-events
                                 (nconc unread-command-events (list event)))))
                     (let ((map (make-sparse-keymap))
                           (minibuffer-setup-hook nil))
                       (define-key map "\C-n" 'kmacro-capture-next)
                       (define-key map "\r" 'kmacro-exit-minibuffer)
                       (setq kmacro-minibuffer-result
                             (read-from-minibuffer "P: " nil map)))))
             (let ((global (make-sparse-keymap)))
               (use-global-map global)
               (define-key global "\C-c:" 'kmacro-open-minibuffer)
               (execute-kbd-macro "\C-c:\C-n\r")
               (list kmacro-minibuffer-events
                     kmacro-minibuffer-result)))"#,
    );

    assert_eq!(results.len(), 1);
    assert_eq!(format_eval_result(&results[0]), r#"OK ((next) "")"#);
}

#[test]
fn test_execute_kbd_macro_publishes_raw_event_before_menu_filter() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let results = eval.eval_str_each(
        r#"(progn
             (setq kmacro-menu-filter-observations nil
                   kmacro-menu-filter-fallthrough-count 0)
             (fset 'command-execute
                   (lambda (command &optional _record _keys _special)
                     (funcall command)))
             (fset 'kmacro-capture-last-input-event
                   (lambda (binding)
                     (setq kmacro-menu-filter-observations
                           (cons (list binding last-input-event)
                                 kmacro-menu-filter-observations))
                     nil))
             (let ((map (make-sparse-keymap))
                   (global (make-sparse-keymap)))
               (use-global-map global)
               (define-key
                global " "
                (lambda ()
                  (setq kmacro-menu-filter-fallthrough-count
                        (1+ kmacro-menu-filter-fallthrough-count))))
               (define-key
                map " "
                '(menu-item "" ignore
                            :filter kmacro-capture-last-input-event))
               (let ((minor-mode-map-alist (list (cons t map)))
                     (recent-before (append (recent-keys) nil)))
                 (execute-kbd-macro " ")
                 (list kmacro-menu-filter-observations
                       kmacro-menu-filter-fallthrough-count
                       last-input-event
                       (equal recent-before
                              (append (recent-keys) nil))))))"#,
    );
    assert_eq!(results.len(), 1);
    assert_eq!(format_eval_result(&results[0]), "OK (((ignore 32)) 1 32 t)");
}

#[test]
fn test_execute_kbd_macro_symbol_events_use_command_loop_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let results = eval.eval_str_each(
        r#"(progn
             (setq kmacro-symbol-event-count 0)
             (setq kmacro-ignore-direct-called nil)
             (fset 'ignore
                   (lambda ()
                     (setq kmacro-ignore-direct-called t)))
             (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
             (let ((g (make-sparse-keymap)))
               (use-global-map g)
               (define-key g [ignore]
                 (lambda ()
                   (interactive)
                   (setq kmacro-symbol-event-count (1+ kmacro-symbol-event-count))))
               (execute-kbd-macro [ignore])
               (list kmacro-symbol-event-count kmacro-ignore-direct-called)))"#,
    );
    assert_eq!(results.len(), 1);
    assert_eq!(format_eval_result(&results[0]), "OK (1 nil)");
}

#[test]
fn test_execute_kbd_macro_named_symbol_uses_function_indirection_chain() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let results = eval.eval_str_each(
        r#"(progn
             (setq kmacro-named-symbol-count 0)
             (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
             (let ((g (make-sparse-keymap)))
               (use-global-map g)
               (define-key g "a"
                 (lambda ()
                   (interactive)
                   (setq kmacro-named-symbol-count (1+ kmacro-named-symbol-count)))))
             (fset 'kmacro-target "a")
             (fset 'kmacro-alias 'kmacro-target)
             (execute-kbd-macro 'kmacro-alias)
             kmacro-named-symbol-count)"#,
    );
    assert_eq!(results.len(), 1);
    assert_eq!(format_eval_result(&results[0]), "OK 1");
}

#[test]
fn test_call_last_kbd_macro_raw_prefix_repeats_real_key_macro() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let _ = eval.eval_str_each(
        r#"(progn
             (setq kmacro-call-last-count 0)
             (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
             (let ((g (make-sparse-keymap)))
               (use-global-map g)
               (define-key g "a"
                 (lambda ()
                   (interactive)
                   (setq kmacro-call-last-count (1+ kmacro-call-last-count))))))"#,
    );

    builtin_start_kbd_macro(&mut eval, vec![]).expect("start");
    builtin_store_kbd_macro_event(&mut eval, vec![Value::char('a')]).expect("store a");
    eval.finalize_kbd_macro_runtime_chars();
    builtin_end_kbd_macro(&mut eval, vec![]).expect("end");

    builtin_call_last_kbd_macro(&mut eval, vec![Value::list(vec![Value::fixnum(4)])])
        .expect("call-last with raw prefix");

    assert_eq!(
        eval.eval_symbol("kmacro-call-last-count")
            .expect("kmacro-call-last-count"),
        Value::fixnum(4)
    );
}

#[test]
fn test_call_last_kbd_macro_symbol_events_use_command_loop_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let _ = eval.eval_str_each(
        r#"(progn
             (setq kmacro-call-last-symbol-count 0)
             (setq kmacro-call-last-ignore-direct-called nil)
             (fset 'ignore
                   (lambda ()
                     (setq kmacro-call-last-ignore-direct-called t)))
             (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
             (let ((g (make-sparse-keymap)))
               (use-global-map g)
               (define-key g [ignore]
                 (lambda ()
                   (interactive)
                   (setq kmacro-call-last-symbol-count
                         (1+ kmacro-call-last-symbol-count))))))"#,
    );

    builtin_start_kbd_macro(&mut eval, vec![]).expect("start");
    builtin_store_kbd_macro_event(&mut eval, vec![Value::symbol("ignore")]).expect("store ignore");
    eval.finalize_kbd_macro_runtime_chars();
    builtin_end_kbd_macro(&mut eval, vec![]).expect("end");
    builtin_call_last_kbd_macro(&mut eval, vec![]).expect("call-last");

    assert_eq!(
        eval.eval_symbol("kmacro-call-last-symbol-count")
            .expect("kmacro-call-last-symbol-count"),
        Value::fixnum(1)
    );
    assert_eq!(
        eval.eval_symbol("kmacro-call-last-ignore-direct-called")
            .expect("kmacro-call-last-ignore-direct-called"),
        Value::NIL
    );
}

#[test]
fn test_execute_kbd_macro_zero_count_uses_loopfunc_for_real_key_macro() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let _ = eval.eval_str_each(
        r#"(progn
             (setq kmacro-loop-count 0)
             (setq kmacro-loopfunc-count 0)
             (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
             (fset 'kmacro-loopfunc
               (lambda ()
                 (setq kmacro-loopfunc-count (1+ kmacro-loopfunc-count))
                 (< kmacro-loopfunc-count 3)))
             (let ((g (make-sparse-keymap)))
               (use-global-map g)
               (define-key g "a"
                 (lambda ()
                   (interactive)
                   (setq kmacro-loop-count (1+ kmacro-loop-count))))))"#,
    );

    builtin_execute_kbd_macro(
        &mut eval,
        vec![
            Value::string("a"),
            Value::fixnum(0),
            Value::symbol("kmacro-loopfunc"),
        ],
    )
    .expect("execute with loopfunc");

    assert_eq!(
        eval.eval_symbol("kmacro-loop-count")
            .expect("kmacro-loop-count"),
        Value::fixnum(2)
    );
    assert_eq!(
        eval.eval_symbol("kmacro-loopfunc-count")
            .expect("kmacro-loopfunc-count"),
        Value::fixnum(3)
    );
}

#[test]
fn test_end_kbd_macro_repeat_executes_remaining_iterations() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    let _ = eval.eval_str_each(
        r#"(progn
             (setq kmacro-end-repeat-count 0)
             (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
             (let ((g (make-sparse-keymap)))
               (use-global-map g)
               (define-key g "a"
                 (lambda ()
                   (interactive)
                   (setq kmacro-end-repeat-count (1+ kmacro-end-repeat-count))))))"#,
    );

    builtin_start_kbd_macro(&mut eval, vec![]).expect("start");
    builtin_store_kbd_macro_event(&mut eval, vec![Value::char('a')]).expect("store a");
    eval.finalize_kbd_macro_runtime_chars();
    builtin_end_kbd_macro(&mut eval, vec![Value::fixnum(3)]).expect("end with repeat");

    assert_eq!(
        eval.eval_symbol("kmacro-end-repeat-count")
            .expect("kmacro-end-repeat-count"),
        Value::fixnum(2)
    );
    assert_eq!(
        eval.command_loop.last_kbd_macro(),
        Some([Value::char('a')].as_slice())
    );
}

#[test]
fn test_execute_kbd_macro_runs_termination_hook_after_restoring_runtime_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let _ = eval.eval_str_each(
        r#"(progn
             (setq kmacro-term-ok nil)
             (setq real-this-command 'outer-real)
             (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
             (fset 'kmacro-term-hook
                   (lambda ()
                     (setq kmacro-term-ok
                           (and (null executing-kbd-macro)
                                (= executing-kbd-macro-index 0)
                                (eq real-this-command 'outer-real)))))
             (setq kbd-macro-termination-hook '(kmacro-term-hook))
             (let ((g (make-sparse-keymap)))
               (use-global-map g)
               (define-key g "a" (lambda () (interactive) 'ok)))
             (execute-kbd-macro "a"))"#,
    );

    assert_eq!(
        eval.eval_symbol("kmacro-term-ok").expect("kmacro-term-ok"),
        Value::T
    );
    assert_eq!(
        eval.eval_symbol("real-this-command")
            .expect("real-this-command"),
        Value::symbol("outer-real")
    );
}

#[test]
fn test_execute_kbd_macro_runs_termination_hook_after_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let _ = eval.eval_str_each(
        r#"(progn
             (setq kmacro-error-term-ok nil)
             (setq real-this-command 'outer-real)
             (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
             (fset 'kmacro-error-term-hook
                   (lambda ()
                     (setq kmacro-error-term-ok
                           (and (null executing-kbd-macro)
                                (= executing-kbd-macro-index 0)
                                (eq real-this-command 'outer-real)))))
             (setq kbd-macro-termination-hook '(kmacro-error-term-hook))
             (let ((g (make-sparse-keymap)))
               (use-global-map g)
               (define-key g "a" (lambda () (interactive) (error "boom"))))
             (condition-case nil
                 (execute-kbd-macro "a")
               (error nil)))"#,
    );

    assert_eq!(
        eval.eval_symbol("kmacro-error-term-ok")
            .expect("kmacro-error-term-ok"),
        Value::T
    );
    assert_eq!(
        eval.eval_symbol("real-this-command")
            .expect("real-this-command"),
        Value::symbol("outer-real")
    );
}

#[test]
fn test_call_last_kbd_macro_preserves_gnu_real_this_command_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let _ = eval.eval_str_each(
        r#"(progn
             (setq kmacro-call-last-term-ok nil)
             (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
             (let ((g (make-sparse-keymap)))
               (use-global-map g)
               (define-key g "a" (lambda () (interactive) 'ok)))
             (start-kbd-macro nil nil)
             (store-kbd-macro-event ?a)
             (end-kbd-macro)
             (setq real-this-command 'outer-real)
             (fset 'kmacro-call-last-term-hook
                   (lambda ()
                     (setq kmacro-call-last-term-ok
                           (and (null executing-kbd-macro)
                                (= executing-kbd-macro-index 0)
                                (equal real-this-command last-kbd-macro)))))
             (setq kbd-macro-termination-hook '(kmacro-call-last-term-hook))
             (call-last-kbd-macro))"#,
    );

    assert_eq!(
        eval.eval_symbol("kmacro-call-last-term-ok")
            .expect("kmacro-call-last-term-ok"),
        Value::T
    );
    assert_eq!(
        eval.eval_symbol("real-this-command")
            .expect("real-this-command"),
        eval.eval_symbol("last-kbd-macro").expect("last-kbd-macro")
    );
}

#[test]
fn test_last_kbd_macro_builtin() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    assert_eq!(
        builtin_last_kbd_macro(&mut eval, vec![]).unwrap(),
        Value::NIL
    );

    eval.command_loop.keyboard.kboard.last_kbd_macro =
        Some(vec![Value::char('x'), Value::char('y')]);
    let value = builtin_last_kbd_macro(&mut eval, vec![]).unwrap();
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = value.as_vector_data().unwrap().clone();
            assert_eq!(*items, vec![Value::char('x'), Value::char('y')]);
        }
        other => panic!("expected vector, got {other:?}"),
    }

    assert!(builtin_last_kbd_macro(&mut eval, vec![Value::NIL]).is_err());
}

#[test]
fn test_kmacro_p_builtin_subset() {
    crate::test_utils::init_test_tracing();
    assert_eq!(builtin_kmacro_p(vec![Value::NIL]).unwrap(), Value::NIL);
    assert_eq!(
        builtin_kmacro_p(vec![Value::vector(vec![])]).unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_kmacro_p(vec![Value::string("abc")]).unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_kmacro_p(vec![Value::fixnum(1)]).unwrap(),
        Value::NIL
    );
    assert!(builtin_kmacro_p(vec![]).is_err());
    assert!(builtin_kmacro_p(vec![Value::NIL, Value::NIL]).is_err());
}

#[test]
fn test_kmacro_builtin_arity_contracts() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    assert_eq!(
        builtin_start_kbd_macro(&mut eval, vec![Value::NIL, Value::NIL]).unwrap(),
        Value::NIL
    );
    assert!(builtin_start_kbd_macro(&mut eval, vec![Value::NIL, Value::NIL]).is_err());
    assert_eq!(
        builtin_end_kbd_macro(&mut eval, vec![]).unwrap(),
        Value::NIL
    );
    assert!(builtin_start_kbd_macro(&mut eval, vec![Value::NIL, Value::NIL, Value::NIL]).is_err());
    assert!(builtin_end_kbd_macro(&mut eval, vec![Value::NIL, Value::NIL, Value::NIL]).is_err());
    assert!(
        builtin_call_last_kbd_macro(&mut eval, vec![Value::NIL, Value::NIL, Value::NIL]).is_err()
    );
    assert!(builtin_execute_kbd_macro(&mut eval, vec![]).is_err());
    assert!(
        builtin_execute_kbd_macro(
            &mut eval,
            vec![Value::NIL, Value::NIL, Value::NIL, Value::NIL]
        )
        .is_err()
    );
}

#[test]
fn test_name_last_kbd_macro() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    // No macro -- should error
    let result = builtin_name_last_kbd_macro(&mut eval, vec![Value::symbol("my-macro")]);
    assert!(result.is_err());

    // Record a macro
    builtin_start_kbd_macro(&mut eval, vec![]).expect("start");
    builtin_store_kbd_macro_event(&mut eval, vec![Value::symbol(intern("forward-char"))])
        .expect("store");
    eval.finalize_kbd_macro_runtime_chars();
    builtin_end_kbd_macro(&mut eval, vec![]).expect("end");

    // Name it
    let result = builtin_name_last_kbd_macro(&mut eval, vec![Value::symbol("my-macro")]);
    assert!(result.is_ok());

    // Check that the symbol has a function binding
    let func = eval.obarray.symbol_function("my-macro");
    assert!(func.is_some());
    match func.unwrap().kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = func.unwrap().as_vector_data().unwrap().clone();
            assert_eq!(items.len(), 1);
        }
        _other => panic!("Expected Vector, got {:?}", func.unwrap()),
    }
}

#[test]
fn test_name_last_kbd_macro_wrong_type() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    let result = builtin_name_last_kbd_macro(&mut eval, vec![Value::fixnum(42)]);
    assert!(result.is_err());
}

#[test]
fn test_kbd_macro_query_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["kbd-macro-query"]);
    let function = eval
        .obarray
        .symbol_function("kbd-macro-query")
        .expect("missing kbd-macro-query startup function cell");
    assert!(is_autoload_value(&function));
}

#[test]
fn test_kbd_macro_query_loads_from_gnu_macros_el() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(list (condition-case err
                     (kbd-macro-query nil)
                   (error (list 'err (car err) (car (cdr err)))))
                 (subrp (symbol-function 'kbd-macro-query)))"#,
    );
    assert_eq!(
        result[0],
        r#"OK ((err user-error "Not defining or executing kbd macro") nil)"#
    );
}

#[test]
fn test_kbd_macro_query_loaded_arity_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(condition-case err
               (kbd-macro-query)
             (error (list 'err (car err))))"#,
    );
    assert_eq!(result[0], r#"OK (err wrong-number-of-arguments)"#);
}

#[test]
fn test_resolve_macro_events_vector() {
    crate::test_utils::init_test_tracing();
    let eval = Context::new();
    let v = Value::vector(vec![Value::char('a'), Value::char('b')]);
    let events = resolve_macro_events(&eval, &v).unwrap();
    assert_eq!(events.len(), 2);
}

#[test]
fn test_resolve_macro_events_string() {
    crate::test_utils::init_test_tracing();
    let eval = Context::new();
    let s = Value::string("hello");
    let events = resolve_macro_events(&eval, &s).unwrap();
    assert_eq!(events.len(), 5);
    match events[0].kind() {
        ValueKind::Fixnum(n) if n == 'h' as i64 => {}
        _other => panic!("Expected Char('h'), got {:?}", events[0]),
    }
}

#[test]
fn test_resolve_macro_events_symbol_function_chain() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.obarray_mut().set_symbol_function(
        "kmacro-target",
        Value::vector(vec![Value::char('x'), Value::char('y')]),
    );
    eval.obarray_mut()
        .set_symbol_function("kmacro-alias", Value::symbol("kmacro-target"));

    let events = resolve_macro_events(&eval, &Value::symbol("kmacro-alias")).unwrap();
    assert_eq!(events.len(), 2);
}

#[test]
fn test_resolve_macro_events_list_errors_like_gnu() {
    crate::test_utils::init_test_tracing();
    let eval = Context::new();
    let list = Value::list(vec![Value::char('x'), Value::char('y')]);
    let result = resolve_macro_events(&eval, &list);
    let Err(Flow::Signal(sig)) = result else {
        panic!("expected signal for list macro");
    };
    assert_eq!(sig.symbol_name(), "error");
    assert_eq!(
        sig.data,
        vec![Value::string("Keyboard macros must be strings or vectors")]
    );
}

#[test]
fn test_resolve_macro_events_wrong_type() {
    crate::test_utils::init_test_tracing();
    let eval = Context::new();
    let result = resolve_macro_events(&eval, &Value::fixnum(42));
    let Err(Flow::Signal(sig)) = result else {
        panic!("expected signal for non-macro value");
    };
    assert_eq!(sig.symbol_name(), "error");
    assert_eq!(
        sig.data,
        vec![Value::string("Keyboard macros must be strings or vectors")]
    );
}

#[test]
fn test_insert_kbd_macro_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["insert-kbd-macro"]);
    let function = eval
        .obarray
        .symbol_function("insert-kbd-macro")
        .expect("missing insert-kbd-macro startup function cell");
    assert!(is_autoload_value(&function));
}

#[test]
fn test_insert_kbd_macro_loads_from_gnu_macros_el() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (fset 'test-macro [97 98])
             (insert-kbd-macro 'test-macro)
             (list (and (string-match-p "defalias" (buffer-string)) t)
                   (and (string-match-p "test-macro" (buffer-string)) t)
                   (subrp (symbol-function 'insert-kbd-macro))))"#,
    );
    assert_eq!(result[0], r#"OK (t t nil)"#);
}

#[test]
fn test_insert_kbd_macro_loaded_arity_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(condition-case err
               (insert-kbd-macro)
             (error (list 'err (car err))))"#,
    );
    assert_eq!(result[0], r#"OK (err wrong-number-of-arguments)"#);
}
