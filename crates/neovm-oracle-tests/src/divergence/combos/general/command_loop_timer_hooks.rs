//! Divergence combo tests: command-loop × timer × hook × prefix-arg ×
//! this-command lifecycle.
//!
//! These tests stress the command-loop variable reset cycle (the fix
//! for GNU keyboard.c:1416-1419 parity) by combining multiple
//! subsystems that read or write `this-command`, `real-this-command`,
//! `this-original-command`, `last-command`, `prefix-arg`,
//! `current-prefix-arg`, `last-prefix-arg`, pre/post-command hooks,
//! and timer callbacks that observe or mutate command-loop state.
//!
//! Goal: surface unknown divergences, not just guard known fixes.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ---------------------------------------------------------------------------
// this-command lifecycle across command-execute chains
// ---------------------------------------------------------------------------

#[test]
fn combo_this_command_chain_through_command_execute() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((before-execute cmd-alpha cmd-alpha cmd-prev) (after-execute cmd-alpha cmd-alpha cmd-prev) (after-last-transfer cmd-alpha cmd-alpha cmd-alpha))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((trace nil))
    (defun tracify (sym)
      (setq trace (cons (list sym
                              (symbol-value 'this-command)
                              (symbol-value 'real-this-command)
                              (symbol-value 'last-command))
                        trace)))
    (setq this-command 'cmd-alpha)
    (setq real-this-command 'cmd-alpha)
    (setq last-command 'cmd-prev)
    (tracify 'before-execute)
    (command-execute 'ignore)
    (tracify 'after-execute)
    (setq last-command this-command)
    (tracify 'after-last-transfer)
    (nreverse trace)))"#,
        expect,
    );
}

#[test]
fn combo_this_command_after_multiple_command_execute() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((results nil))
    (setq this-command nil real-this-command nil last-command nil)
    (command-execute 'ignore)
    (push (list 'step1 this-command real-this-command last-command) results)
    (setq last-command this-command)
    (command-execute 'forward-char)
    (push (list 'step2 this-command real-this-command last-command) results)
    (setq last-command this-command)
    (command-execute 'backward-char)
    (push (list 'step3 this-command real-this-command last-command) results)
    (nreverse results)))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// prefix-arg lifecycle: prefix-arg → current-prefix-arg → last-prefix-arg
// ---------------------------------------------------------------------------

#[test]
fn combo_prefix_arg_transition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((initial nil nil nil) (after-set-prefix (4) nil nil) (after-command-execute nil (4) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((results nil))
    (push (list 'initial
                prefix-arg current-prefix-arg last-prefix-arg) results)
    (setq prefix-arg '(4))
    (push (list 'after-set-prefix
                prefix-arg current-prefix-arg last-prefix-arg) results)
    (command-execute 'ignore)
    (push (list 'after-command-execute
                prefix-arg current-prefix-arg last-prefix-arg) results)
    (nreverse results)))"#,
        expect,
    );
}

#[test]
fn combo_prefix_arg_numeric_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((nil nil 1) (1 1 1) ((4) (4) 4) ((16) (16) 16) ((64) (64) 64) (-1 -1 -1) ((-1) (-1) -1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((results nil))
    (dolist (parg (list nil 1 '(4) '(16) '(64) -1 '(-1)))
      (setq prefix-arg parg)
      (command-execute 'ignore)
      (push (list parg
                  current-prefix-arg
                  (prefix-numeric-value current-prefix-arg))
            results)
      (setq prefix-arg nil))
    (nreverse results)))"#,
        expect,
    );
}

#[test]
fn combo_prefix_arg_survives_until_command_execute() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument commandp capture-prefix)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((captured-prefix nil))
    (defun capture-prefix ()
      (setq captured-prefix current-prefix-arg))
    (setq prefix-arg '(4))
    (command-execute 'capture-prefix)
    (list captured-prefix
          (prefix-numeric-value captured-prefix)
          (eq captured-prefix '(4)))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// pre-command-hook and post-command-hook interaction
// ---------------------------------------------------------------------------

#[test]
fn combo_pre_post_command_hooks_fire_around_command_execute() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((trace nil))
    (add-hook 'pre-command-hook
              (lambda ()
                (push (list 'pre this-command real-this-command) trace))
              nil t)
    (add-hook 'post-command-hook
              (lambda ()
                (push (list 'post this-command real-this-command last-command) trace))
              nil t)
    (setq this-command nil real-this-command nil last-command nil)
    (command-execute 'ignore)
    (remove-hook 'pre-command-hook nil t)
    (remove-hook 'post-command-hook nil t)
    (nreverse trace)))"#,
        expect,
    );
}

#[test]
fn combo_pre_command_hook_sees_old_this_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((pre-snap nil)
        (post-snap nil))
    (setq this-command 'old-cmd real-this-command 'old-cmd last-command nil)
    (add-hook 'pre-command-hook
              (lambda ()
                (setq pre-snap (list this-command real-this-command
                                     current-prefix-arg)))
              nil t)
    (add-hook 'post-command-hook
              (lambda ()
                (setq post-snap (list this-command real-this-command
                                      last-command current-prefix-arg)))
              nil t)
    (command-execute 'ignore)
    (remove-hook 'pre-command-hook nil t)
    (remove-hook 'post-command-hook nil t)
    (list pre-snap post-snap)))"#,
        expect,
    );
}

#[test]
fn combo_post_command_hook_runs_after_this_command_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((observed nil))
    (add-hook 'post-command-hook
              (lambda ()
                (push (list 'post
                            this-command
                            (eq this-command 'ignore))
                      observed))
              nil t)
    (command-execute 'ignore)
    (remove-hook 'post-command-hook nil t)
    (nreverse observed)))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Hooks that modify command-loop variables
// ---------------------------------------------------------------------------

#[test]
fn combo_post_command_hook_modifies_last_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((final nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((snapshots nil))
    (add-hook 'post-command-hook
              (lambda ()
                (push (list 'before-mutate last-command) snapshots)
                (setq last-command 'hook-set)
                (push (list 'after-mutate last-command) snapshots))
              nil t)
    (command-execute 'ignore)
    (remove-hook 'post-command-hook nil t)
    (push (list 'final last-command) snapshots)
    (nreverse snapshots)))"#,
        expect,
    );
}

#[test]
fn combo_pre_command_hook_sets_prefix_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument commandp snap-prefix)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((captured nil))
    (add-hook 'pre-command-hook
              (lambda ()
                (setq prefix-arg '(4)))
              nil t)
    (defun snap-prefix ()
      (setq captured current-prefix-arg))
    (command-execute 'snap-prefix)
    (remove-hook 'pre-command-hook nil t)
    captured))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Timer callbacks reading command-loop state
// ---------------------------------------------------------------------------

#[test]
fn combo_timer_callback_reads_this_command_during_sit_for() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (some-cmd some-cmd nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((snap nil)
        (timer nil))
    (setq this-command 'some-cmd real-this-command 'some-cmd)
    (setq timer (run-with-timer 0.1 nil
                  (lambda ()
                    (setq snap (list this-command real-this-command
                                     last-command current-prefix-arg)))))
    (sit-for 0.3)
    (cancel-timer timer)
    snap))"#,
        expect,
    );
}

#[test]
fn combo_timer_mutates_this_command_during_sit_for() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK timer-set-it""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((timer nil))
    (setq this-command 'before-timer)
    (setq timer (run-with-timer 0.1 nil
                  (lambda ()
                    (setq this-command 'timer-set-it))))
    (sit-for 0.3)
    (cancel-timer timer)
    this-command))"#,
        expect,
    );
}

#[test]
fn combo_idle_timer_reads_this_command_during_sit_for() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((snap nil)
        (timer nil))
    (setq this-command 'idle-test-cmd)
    (setq timer (run-with-idle-timer 0.1 nil
                  (lambda ()
                    (setq snap (list this-command real-this-command
                                     (null this-command))))))
    (sit-for 0.3)
    (cancel-timer timer)
    snap))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Multiple timers interacting with command state
// ---------------------------------------------------------------------------

#[test]
fn combo_multiple_timers_see_mutated_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((t1 initial) (t2 from-t1) (t3 from-t2) (final from-t2))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((trace nil)
        (t1 nil) (t2 nil) (t3 nil))
    (setq this-command 'initial)
    (setq t1 (run-with-timer 0.05 nil
                (lambda ()
                  (push (list 't1 this-command) trace)
                  (setq this-command 'from-t1))))
    (setq t2 (run-with-timer 0.15 nil
                (lambda ()
                  (push (list 't2 this-command) trace)
                  (setq this-command 'from-t2))))
    (setq t3 (run-with-timer 0.25 nil
                (lambda ()
                  (push (list 't3 this-command) trace))))
    (sit-for 0.4)
    (cancel-timer t1)
    (cancel-timer t2)
    (cancel-timer t3)
    (push (list 'final this-command) trace)
    (nreverse trace)))"#,
        expect,
    );
}

#[test]
fn combo_timer_fires_during_accept_process_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"TIMER-RAN\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((fired nil)
        (timer nil)
        (buf (generate-new-buffer " timer-accept")))
    (with-current-buffer buf
      (setq timer (run-with-timer 0.1 nil
                    (lambda ()
                      (setq fired t)
                      (with-current-buffer buf
                        (insert "TIMER-RAN")))))
      (accept-process-output nil 0.3)
      (let ((result (list fired (buffer-string))))
        (cancel-timer timer)
        (kill-buffer buf)
        result))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// command-execute with remapping and this-original-command
// ---------------------------------------------------------------------------

#[test]
fn combo_command_remapping_sets_this_original_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((original-cmd nil)
        (remapped-cmd nil)
        (map (make-sparse-keymap)))
    (define-key map [remap forward-char] 'backward-char)
    (use-global-map map)
    (setq this-original-command nil)
    (setq this-command nil)
    (command-execute 'forward-char)
    (setq original-cmd this-original-command
          remapped-cmd this-command)
    (use-global-map (make-sparse-keymap))
    (list original-cmd remapped-cmd
          (eq original-cmd 'forward-char)
          (eq remapped-cmd 'forward-char))))"#,
        expect,
    );
}

#[test]
fn combo_command_remapping_through_call_interactively() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((observed-this nil)
        (observed-real nil)
        (observed-orig nil)
        (map (make-sparse-keymap)))
    (define-key map [remap ignore] 'forward-char)
    (use-global-map map)
    (setq this-command nil real-this-command nil this-original-command nil)
    (call-interactively 'ignore)
    (setq observed-this this-command
          observed-real real-this-command
          observed-orig this-original-command)
    (use-global-map (make-sparse-keymap))
    (list observed-this observed-real observed-orig
          (command-remapping 'ignore))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// command-execute error handling and state preservation
// ---------------------------------------------------------------------------

#[test]
fn combo_command_execute_error_preserves_partial_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((caught (wrong-type-argument commandp (closure (t) nil (error \"test-error\"))) before-error before-error) (after before-error before-error nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((results nil))
    (setq this-command 'before-error real-this-command 'before-error)
    (condition-case err
        (command-execute
         (lambda () (error "test-error")))
      (error (push (list 'caught err this-command real-this-command) results)))
    (push (list 'after this-command real-this-command last-command) results)
    (nreverse results)))"#,
        expect,
    );
}

#[test]
fn combo_post_command_hook_after_error_in_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((post-fired nil)
        (post-snapshot nil))
    (add-hook 'post-command-hook
              (lambda ()
                (setq post-fired t)
                (setq post-snapshot (list this-command real-this-command)))
              nil t)
    (condition-case nil
        (command-execute
         (lambda () (error "boom")))
      (error nil))
    (remove-hook 'post-command-hook nil t)
    (list post-fired post-snapshot)))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// this-command-keys and this-single-command-keys with command-execute
// ---------------------------------------------------------------------------

#[test]
fn combo_this_command_keys_after_various_invocations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((initial \"\" []) (after-command-execute \"\" []) (after-call-interactively \"\" []) (after-funcall \"\" []))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((results nil))
    (push (list 'initial
                (this-command-keys)
                (this-single-command-keys)) results)
    (command-execute 'ignore)
    (push (list 'after-command-execute
                (this-command-keys)
                (this-single-command-keys)) results)
    (call-interactively 'ignore)
    (push (list 'after-call-interactively
                (this-command-keys)
                (this-single-command-keys)) results)
    (funcall 'ignore)
    (push (list 'after-funcall
                (this-command-keys)
                (this-single-command-keys)) results)
    (nreverse results)))"#,
        expect,
    );
}

#[test]
fn combo_clear_this_command_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((before nil) (after nil))
    (setq before (length (this-command-keys)))
    (clear-this-command-keys t)
    (setq after (length (this-command-keys)))
    (list before after)))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Complex: hooks + timers + this-command + prefix-arg all at once
// ---------------------------------------------------------------------------

#[test]
fn combo_hooks_timers_prefix_arg_this_command_full_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((timer nil nil nil (4)))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((trace nil)
        (timer nil))
    (setq this-command nil real-this-command nil
          last-command nil prefix-arg nil)
    (add-hook 'pre-command-hook
              (lambda ()
                (push (list 'pre
                            this-command real-this-command
                            current-prefix-arg prefix-arg)
                      trace))
              nil t)
    (add-hook 'post-command-hook
              (lambda ()
                (push (list 'post
                            this-command real-this-command
                            last-command current-prefix-arg)
                      trace))
              nil t)
    (setq timer (run-with-timer 0.1 nil
                  (lambda ()
                    (push (list 'timer
                                this-command real-this-command
                                last-command current-prefix-arg)
                          trace))))
    (setq prefix-arg '(4))
    (command-execute 'ignore)
    (sit-for 0.3)
    (cancel-timer timer)
    (remove-hook 'pre-command-hook nil t)
    (remove-hook 'post-command-hook nil t)
    (nreverse trace)))"#,
        expect,
    );
}

#[test]
fn combo_nested_command_execute_through_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument commandp outer-cmd)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((trace nil))
    (defun inner-cmd ()
      (push (list 'inner this-command real-this-command current-prefix-arg) trace))
    (defun outer-cmd ()
      (push (list 'outer-before this-command real-this-command current-prefix-arg) trace)
      (setq this-command 'outer-still)
      (command-execute 'inner-cmd)
      (push (list 'outer-after this-command real-this-command) trace))
    (setq this-command nil real-this-command nil prefix-arg '(4))
    (command-execute 'outer-cmd)
    (push (list 'top-level this-command real-this-command last-command) trace)
    (nreverse trace)))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// last-command and real-last-command across transitions
// ---------------------------------------------------------------------------

#[test]
fn combo_last_command_real_last_command_transition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((results nil))
    (push (list 'initial
                last-command
                (bound-and-true-p real-last-command)) results)
    (setq this-command 'cmd-a real-this-command 'cmd-a)
    (setq last-command this-command)
    (command-execute 'ignore)
    (push (list 'after-step1
                last-command
                this-command
                real-this-command
                (bound-and-true-p real-last-command)) results)
    (setq last-command this-command)
    (command-execute 'forward-char)
    (push (list 'after-step2
                last-command
                this-command
                real-this-command
                (bound-and-true-p real-last-command)) results)
    (nreverse results)))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// called-interactively-p inside hooks and timers
// ---------------------------------------------------------------------------

#[test]
fn combo_called_interactively_in_post_command_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((result nil))
    (add-hook 'post-command-hook
              (lambda ()
                (setq result (list (called-interactively-p)
                                   (called-interactively-p 'any)
                                   (called-interactively-p 'interactive))))
              nil t)
    (command-execute 'ignore)
    (remove-hook 'post-command-hook nil t)
    result))"#,
        expect,
    );
}

#[test]
fn combo_called_interactively_in_timer_callback() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((result nil)
        (timer nil))
    (setq timer (run-with-timer 0.1 nil
                  (lambda ()
                    (setq result (list (called-interactively-p)
                                       (called-interactively-p 'any)
                                       (called-interactively-p 'interactive))))))
    (sit-for 0.3)
    (cancel-timer timer)
    result))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Timer list management: interleaved add/cancel
// ---------------------------------------------------------------------------

#[test]
fn combo_timer_list_management_interleaved() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 2 3 2 t nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((t1 (run-with-timer 10 nil 'ignore))
        (t2 (run-with-timer 10 nil 'ignore))
        (t3 (run-with-timer 10 nil 'ignore))
        (i1 (run-with-idle-timer 10 nil 'ignore))
        (i2 (run-with-idle-timer 10 nil 'ignore)))
    (let ((before-t (length timer-list))
          (before-i (length timer-idle-list)))
      (cancel-timer t2)
      (cancel-timer i1)
      (let ((after-t (length timer-list))
            (after-i (length timer-idle-list))
            (t1-live (not (null (memq t1 timer-list))))
            (t2-live (not (null (memq t2 timer-list))))
            (t3-live (not (null (memq t3 timer-list))))
            (i1-live (not (null (memq i1 timer-idle-list))))
            (i2-live (not (null (memq i2 timer-idle-list)))))
        (cancel-timer t1) (cancel-timer t3) (cancel-timer i2)
        (list before-t after-t
              before-i after-i
              t1-live t2-live t3-live
              i1-live i2-live)))))"#,
        expect,
    );
}

#[test]
fn combo_repeating_timer_cancellation_from_callback() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 (1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((count 0)
        (timer nil)
        (trace nil))
    (setq timer (run-with-timer 0.05 0.05
                  (lambda ()
                    (setq count (1+ count))
                    (push count trace)
                    (when (>= count 3)
                      (cancel-timer timer)))))
    (sit-for 0.5)
    (cancel-timer timer)
    (list count (nreverse trace))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Repeating idle timer with accept-process-output
// ---------------------------------------------------------------------------

#[test]
fn combo_repeating_idle_timer_cancel_from_own_callback() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((count 0)
        (timer nil)
        (trace nil))
    (setq timer (run-with-idle-timer 0.05 t
                  (lambda ()
                    (setq count (1+ count))
                    (push count trace)
                    (when (>= count 3)
                      (cancel-timer timer)))))
    (sit-for 0.5)
    (cancel-timer timer)
    (list count (nreverse trace))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Timer + buffer + command-execute cross-subsystem
// ---------------------------------------------------------------------------

#[test]
fn combo_timer_modifies_buffer_during_command_execution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"INITIAL-TIMER\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-timer-buf"))
        (timer nil)
        (result nil))
    (with-current-buffer buf
      (insert "INITIAL"))
    (setq timer (run-with-timer 0.1 nil
                  (lambda ()
                    (with-current-buffer buf
                      (goto-char (point-max))
                      (insert "-TIMER")))))
    (command-execute 'ignore)
    (sit-for 0.3)
    (cancel-timer timer)
    (with-current-buffer buf
      (setq result (buffer-string)))
    (kill-buffer buf)
    result))"#,
        expect,
    );
}

#[test]
fn combo_post_command_hook_modifies_buffer_and_timer_reads_it() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"START\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-hook-timer"))
        (timer nil)
        (snap nil))
    (with-current-buffer buf
      (insert "START"))
    (add-hook 'post-command-hook
              (lambda ()
                (with-current-buffer buf
                  (goto-char (point-max))
                  (insert "-HOOK")))
              nil t)
    (setq timer (run-with-timer 0.1 nil
                  (lambda ()
                    (with-current-buffer buf
                      (setq snap (buffer-string))))))
    (command-execute 'ignore)
    (sit-for 0.3)
    (cancel-timer timer)
    (remove-hook 'post-command-hook nil t)
    (kill-buffer buf)
    snap))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// echo-keystrokes variable semantics
// ---------------------------------------------------------------------------

#[test]
fn combo_echo_keystrokes_value_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((default (default-value 'echo-keystrokes))
        (local (symbol-value 'echo-keystrokes)))
    (list (number-or-marker-p default)
          (or (null default) (> default 0))
          (number-or-marker-p local)
          (or (null local) (> local 0))
          (eq default local))))"#,
        expect,
    );
}

#[test]
fn combo_echo_keystrokes_help_bound_and_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (boundp 'echo-keystrokes-help)
        (symbol-value 'echo-keystrokes-help)
        (not (null echo-keystrokes-help))))"#,
        expect,
    );
}
