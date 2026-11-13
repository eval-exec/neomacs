//! Oracle twins of the thread-signal regressions R5–R8.
//!
//! Each form is the one pinned GNU Emacs 31.1 (`a360712c9d27`) printed.
//! The expectations are those prints (probe3 for R5, R7, and R8; probe5 for
//! the sleep-bounded R6), not Neomacs output. `setq`/`cons` keep the forms
//! free of `subr.el` `push`.
//! The harness eval is lexical; a free `setq` is still the global binding
//! both threads see.

use crate::common;

use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_thread_signal_keeps_only_the_last() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU: ((join-err error ("last")) nil nil ((caught error ("last"))))
    common::assert_oracle_parity_expect(
        r##"
(progn
  (setq gnu-ts-log nil)
  (setq gnu-ts-w
        (make-thread
         (lambda ()
           (condition-case err
               (progn (thread-yield) 'ran)
             (arith-error
              (setq gnu-ts-log (cons (list 'caught (car err) (cdr err)) gnu-ts-log))
              'arith)
             (error
              (setq gnu-ts-log (cons (list 'caught (car err) (cdr err)) gnu-ts-log))
              'other)))))
  (thread-signal gnu-ts-w 'arith-error '("first"))
  (thread-signal gnu-ts-w 'error '("last"))
  (list
   (condition-case err
       (thread-join gnu-ts-w)
     (error (list 'join-err (car err) (cdr err))))
   (thread-live-p gnu-ts-w)
   (thread-last-error)
   gnu-ts-log))
"##,
        expect_test::expect![[
            r#""OK ((join-err error (\"last\")) nil nil ((caught error (\"last\"))))""#
        ]],
    );
}

#[test]
fn oracle_thread_signal_of_an_exited_thread_stays_dead() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU, after the worker had exited (probe5, sleep-bounded wait):
    // (nil nil nil nil (join-err arith-error ("bad")) nil nil)
    common::assert_oracle_parity_expect(
        r##"
(progn
  (setq gnu-ts-w (make-thread (lambda () 42)))
  (setq gnu-r6-deadline (+ (float-time) 0.5))
  (while (and (thread-live-p gnu-ts-w) (< (float-time) gnu-r6-deadline))
    (thread-yield))
  (setq gnu-r6-live1 (thread-live-p gnu-ts-w))
  (setq gnu-r6-le1 (thread-last-error))
  (thread-signal gnu-ts-w 'arith-error '("bad"))
  (list gnu-r6-live1
        (thread-live-p gnu-ts-w)
        gnu-r6-le1
        (thread-last-error)
        (condition-case err
            (list :ok (thread-join gnu-ts-w))
          (error (list 'join-err (car err) (cdr err))))
        (thread-last-error)
        (thread-live-p gnu-ts-w)))
"##,
        expect_test::expect![[
            r#""OK (nil nil nil nil (join-err arith-error (\"bad\")) nil nil)""#
        ]],
    );
}

#[test]
fn oracle_thread_signal_second_join_returns_the_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU: ((join-err arith-error ("bad")) (:ok caught) nil nil
    //       ((caught arith-error ("bad"))))
    common::assert_oracle_parity_expect(
        r##"
(progn
  (setq gnu-ts-log nil)
  (setq gnu-ts-w
        (make-thread
         (lambda ()
           (condition-case err
               (progn (thread-yield) 'ran)
             (arith-error
              (setq gnu-ts-log (cons (list 'caught (car err) (cdr err)) gnu-ts-log))
              'caught)))))
  (thread-signal gnu-ts-w 'arith-error '("bad"))
  (setq gnu-r7-j1
        (condition-case err
            (list :ok (thread-join gnu-ts-w))
          (error (list 'join-err (car err) (cdr err)))))
  (setq gnu-r7-j2
        (condition-case err
            (list :ok (thread-join gnu-ts-w))
          (error (list 'join-err (car err) (cdr err)))))
  (list gnu-r7-j1 gnu-r7-j2 (thread-live-p gnu-ts-w) (thread-last-error) gnu-ts-log))
"##,
        expect_test::expect![[
            r#""OK ((join-err arith-error (\"bad\")) (:ok caught) nil nil ((caught arith-error (\"bad\"))))""#
        ]],
    );
}

#[test]
fn oracle_returned_unjoined_worker_is_dead() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU 31.1 a360712c9d27:
    // ((t t) nil nil 42 nil)
    // thread.c:76 is m_specpdl != NULL; run_thread clears it at :822-823
    // and unlinks at :853-859, both before join.
    common::assert_oracle_parity_expect(
        r##"
(let* ((w (make-thread (lambda () 42)))
       (before (list (thread-live-p w) (and (memq w (all-threads)) t))))
  (sleep-for 0.2)
  (list before
        (thread-live-p w)
        (and (memq w (all-threads)) t)
        (thread-join w)
        (thread-live-p w)))
"##,
        expect_test::expect![[r#""OK ((t t) nil nil 42 nil)""#]],
    );
}

#[test]
fn oracle_thread_signal_before_handler_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU: ((join-err arith-error ("bad")) nil (arith-error "bad") nil)
    // The yield reacquires before the user's condition-case, so the signal
    // escapes (thread.c:805 is before handlerlist_sentinel at :810-815;
    // post_acquire raises at :141-154).
    common::assert_oracle_parity_expect(
        r##"
(progn
  (setq gnu-ts-log nil)
  (setq gnu-ts-w
        (make-thread
         (lambda ()
           (thread-yield)
           (condition-case err
               'body
             (arith-error
              (setq gnu-ts-log (cons (list 'caught (car err) (cdr err)) gnu-ts-log))
              'caught)))))
  (thread-signal gnu-ts-w 'arith-error '("bad"))
  (list
   (condition-case err
       (thread-join gnu-ts-w)
     (error (list 'join-err (car err) (cdr err))))
   (thread-live-p gnu-ts-w)
   (thread-last-error)
   gnu-ts-log))
"##,
        expect_test::expect![[
            r#""OK ((join-err arith-error (\"bad\")) nil (arith-error \"bad\") nil)""#
        ]],
    );
}
