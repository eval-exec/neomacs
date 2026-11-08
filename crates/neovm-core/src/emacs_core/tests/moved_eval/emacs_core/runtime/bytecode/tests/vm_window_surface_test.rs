//! VM tests whose fixture is the display window surface, the terminal
//! frame surface, or the minibuffer command surface.
//!
//! `window_cmds`, `crate::window`, `terminal`, and `minibuffer` live
//! above eval. Eval cannot name those crates. Test names and assertions
//! are unchanged. Frame and buffer managers are the public accessors
//! because those fields are `pub(crate)`. The bare harness is core's
//! `new_context`: eval's unit-runtime module stays `cfg(test)`. The
//! selection test keeps `runtime_startup_context`, which is what its
//! bootstrap helper called. The save-window-excursion opcode test is here
//! because it calls the display split trait; `Vm` is public.

use neovm_display::window::{DisplayFrameManagerExt, SplitDirection, SplitPlacement};
use neovm_eval::emacs_core::bytecode::{ByteCodeFunction, Op, StackDepth, Vm};
use neovm_eval::emacs_core::eval::data::minibuffer_entry::MinibufferManagerExt;
use neovm_value::value::{LambdaParams, Value};

use crate::emacs_core::eval::Context;

fn vm_eval_with_init_str(src: &str, init: impl FnOnce(&mut Context)) -> String {
    let mut eval = crate::test_utils::new_context();
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
fn vm_window_chrome_height_builtins_use_last_redisplay_snapshot() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            // GNU gates each height on whether the window WANTS that line
            // (`window.h` WINDOW_{MODE,HEADER,TAB}_LINE_HEIGHT, each
            // `window_wants_*` ? CURRENT_*_HEIGHT : 0), so a recorded height is
            // only reachable once the format variable makes the line present.
            // Both halves are checked here: the default buffer has only a mode
            // line, and enabling the other two exposes the snapshot's heights.
            r#"(list (window-mode-line-height)
                     (window-header-line-height)
                     (window-tab-line-height)
                     (progn
                       (setq header-line-format "h" tab-line-format "t")
                       (list (window-mode-line-height)
                             (window-header-line-height)
                             (window-tab-line-height))))"#,
            |eval| {
                let fid = {
                    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
                    crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(
                        frames, buffers,
                    )
                };
                let wid = eval
                    .frame_manager()
                    .get(fid)
                    .expect("frame")
                    .selected_window;
                neovm_display::test_support::authority::commit_redisplay_cache_for_test(
                    eval.frame_manager_mut().get_mut(fid).expect("frame"),
                    vec![crate::window::WindowDisplaySnapshot {
                        point_rows: None,
                        window_id: wid,
                        mode_line_height: 35,
                        header_line_height: 35,
                        tab_line_height: 34,
                        ..crate::window::WindowDisplaySnapshot::default()
                    }],
                );
            }
        ),
        "OK (35 0 0 (35 35 34))"
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
                let fid = {
                    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
                    crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(
                        frames, buffers,
                    )
                };
                let minibuffer_buffer_id = {
                    let frame = eval.frame_manager().get(fid).expect("selected frame");
                    let minibuffer_wid = frame.minibuffer_window.expect("minibuffer window");
                    frame
                        .find_window(minibuffer_wid)
                        .and_then(|window| window.buffer_id())
                        .expect("minibuffer buffer")
                };
                eval.minibuffers_mut()
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
                let left = eval
                    .frame_manager()
                    .get(fid)
                    .expect("frame")
                    .selected_window;
                let buffer_id = eval.buffer_manager().current_buffer().expect("buffer").id();
                let right = eval
                    .frame_manager_mut()
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
                    .frame_manager_mut()
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
                let w1 = eval
                    .frame_manager()
                    .get(fid)
                    .expect("frame")
                    .selected_window;
                let buffer_id = eval.buffer_manager().current_buffer().expect("buffer").id();
                let _w2 = eval
                    .frame_manager_mut()
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
                       ;; (src/window.c:4986-4989). and `select_window' makes the new
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
                let w1 = eval
                    .frame_manager()
                    .get(fid)
                    .expect("frame")
                    .selected_window;
                let buffer_id = eval.buffer_manager().current_buffer().expect("buffer").id();
                let _w2 = eval
                    .frame_manager_mut()
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
                      (w2 ((lambda (old size side normal) (set-window-new-pixel old (- (if (memq side '(t left right)) (window-pixel-width old) (window-pixel-height old)) size)) (split-window-internal old size side normal)) w1 (/ (window-pixel-width w1) 2) 'right nil))
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
        // GNU 31.0.90 (`-nw` under tmux, same forms): a termcap frame stays
        // visible through `iconify-frame' — `(frame-visible-p f2)' is t and
        // `visible-frame-list' keeps both frames.
        "OK (t 2 t t t 2 2 t 2 t)"
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
                let minibuf_id = eval.buffer_manager_mut().create_buffer(" *Minibuf-1*");
                crate::emacs_core::minibuffer::install_minibuffer_buffer_text(
                    eval.buffer_manager_mut(),
                    minibuf_id,
                    &neovm_value::heap_types::LispString::from_utf8("Prompt: "),
                    Some(&neovm_value::heap_types::LispString::from_utf8("vm-mini")),
                    crate::emacs_core::minibuffer::default_minibuffer_prompt_properties(),
                );
                eval.buffer_manager_mut().set_current(minibuf_id);
                eval.minibuffers_mut()
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
                let minibuf_id = eval.buffer_manager_mut().create_buffer(" *Minibuf-1*");
                crate::emacs_core::minibuffer::install_minibuffer_buffer_text(
                    eval.buffer_manager_mut(),
                    minibuf_id,
                    &neovm_value::heap_types::LispString::from_utf8("Prompt: "),
                    Some(&neovm_value::heap_types::LispString::from_utf8("vm-mini")),
                    crate::emacs_core::minibuffer::default_minibuffer_prompt_properties(),
                );
                eval.buffer_manager_mut().set_current(minibuf_id);
                eval.minibuffers_mut()
                    .read_from_minibuffer(minibuf_id, "Prompt: ", Some("vm-mini"), None)
                    .expect("enter minibuffer");
            },
        ),
        "OK (t minibuffer-quit minibuffer-quit not-an-error)"
    );
}

#[test]
fn vm_save_window_excursion_restores_selected_window() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let buffer_id = eval.buffer_manager_mut().create_buffer("*vm-save-window*");
    eval.buffer_manager_mut().set_current(buffer_id);
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("F1", 960, 640, buffer_id);
    assert!(eval.frame_manager_mut().select_frame(frame_id));
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let other_window = eval
        .frame_manager_mut()
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
    func.max_stack = StackDepth::for_test(2);

    let result = {
        let mut vm = Vm::from_context(&mut eval);
        vm.execute(&func, vec![])
            .expect("save-window-excursion opcode should execute")
    };

    assert_eq!(result, Value::make_window(other_window.0));
    assert_eq!(
        eval.frame_manager()
            .get(frame_id)
            .expect("frame")
            .selected_window,
        selected_window
    );
}
