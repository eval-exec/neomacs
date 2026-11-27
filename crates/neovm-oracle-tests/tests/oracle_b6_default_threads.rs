//! b6 qualification harness: the GNU thread rows replayed with an opt-in era open.
//!
//!
//! **The module name is load-bearing for gate selection.** The batch gate selects
//! oracle rows with `test(/thread|e6p1|e4p2|default_threads|signal|.../)` matched
//! against the full test id, so every row here is reached through the module
//! segment `oracle_b6_default_threads` -> the `default_threads` clause, by
//! construction rather than by each row's own name. Rename this module and all 31
//! rows silently stop running - a row no filter reaches looks exactly like a row
//! that passes. Before renaming, re-run the reachability check and update the
//! gate alternation.
//!
//! See `docs/architecture/b6-qualification-plan.md` §2 and §10. The point of
//! this suite is NOT to compare thread answers again — `oracle_default_threads`
//! and `oracle_green_threads` already do that. It is to show that the answers
//! are unchanged **while a parallel keeper era is open and one opt-in owner is
//! parked**, and to refuse to pass unless that was actually the case.
//!
//! Two properties are deliberate and both come from hard experience:
//!
//! 1. **The witness is out of band.** The prelude writes `admitted=<t|nil>`
//!    and `parked=<t|nil>` to the file named by `NEOVM_B6_WITNESS`; the row's
//!    returned value is untouched. A witness folded into the compared payload
//!    would make the parity comparison compare the harness instead of the
//!    answer — and GNU, which has no `neomacs-make-parallel-thread`, would
//!    return a different shape by construction.
//! 2. **A missing witness is a failure, not a skip.** "The arm left no file"
//!    and "the arm never admitted" are the two ways this suite could pass
//!    vacuously, so both panic with the file path in the message.
//!
//! The red half of this harness is provable before the green half exists: run
//! it against the **dormant** binary (admission constant `false`) and the
//! witness says `admitted=nil`, so the row fails. That was done on the dormant
//! tree before the enabled tree could be built, per the rule that a check
//! never observed failing has no meaning.
//!
//! **Scope: the whole thread suite, one named exclusion.** This file replays
//! every row of `oracle_default_threads.rs` with the era open — 31 of its 32
//! rows, named `b6_<original>`, each keeping its original form and its
//! original GNU expectation byte for byte. (The arm grew by #877's regression
//! row after its scope was ratified; it is replayed here so the every-row
//! claim stays true.) The excluded row is
//! `e8s_make_thread_arity_and_main_thread_identity`, which prints
//! `(length (all-threads))`: GNU answers 1, and an admitted owner is a real
//! thread object, so the b6 arm answers 2 by construction. That is not a
//! divergence, and no single form can prove both. The exclusion is covered
//! instead by the witness: every row asserts `threads=2` and `admitted=t`, so
//! an era arm that failed to add its owner — or added two, or admitted
//! nothing — is red on every row, not on one. The Sim and Green control arms
//! keep running the excluded row unchanged in `oracle_default_threads.rs`, so
//! nothing is weakened or deleted.
//!
//! The former second exclusion (`e6p1_a_feature_detected_opt_in_creator...`)
//! is gone: that row forked on `fboundp 'neomacs-make-parallel-thread` and
//! compared the Neomacs-only hint, so with admission on it could never agree
//! with GNU. It was re-expressed as `e6p1_make_thread_matches_gnu_in_the_default_arm`
//! (both editors create through `make-thread`, no Neomacs-only function in the
//! compared list) and rejoins this arm as `b6_e6p1_make_thread_matches_gnu_in_the_default_arm`.
//! The creator's own semantics live in the unit rows instead, including a
//! forced-off fallback row.
//!
//! The arm MUST run with `NEOVM_THREADS=green`: `admit_parallel` refuses with
//! `Unavailable` unless `green_world(ctx)` holds (`green/api.rs:392`), and a
//! `--batch -Q` default run is Sim. This is the configuration S3 will ship, so
//! the b6 arm doubles as evidence for S3 rather than pinning a private config.

use crate::common;

use common::return_if_neovm_enable_oracle_proptest_not_set;

/// Admits one idle opt-in owner, records the witness, and returns nil.
///
/// GNU has no `neomacs-make-parallel-thread`, so `fboundp` makes the whole form
/// a no-op there and the row's value is the bare row form. `NEOVM_B6_ERA` is a
/// second guard so the prelude can never run in a suite that did not ask for it.
const B6_PRELUDE: &str = r#"(if (and (getenv "NEOVM_B6_ERA")
                                     (fboundp 'neomacs-make-parallel-thread))
    (let* ((m (make-mutex))
           (c (make-condition-variable m "b6-idle"))
           (parked nil)
           (owner (neomacs-make-parallel-thread
                   (lambda ()
                     (mutex-lock m)
                     (setq parked t)
                     (condition-wait c))
                   "b6-idle-owner"))
           (deadline (+ (float-time) 5.0)))
      (while (and (not parked) (< (float-time) deadline))
        (thread-yield))
      (let ((path (getenv "NEOVM_B6_WITNESS")))
        (when path
          (with-temp-file path
            (insert (format "admitted=%s parked=%s threads=%s\n"
                            (and (fboundp 'neomacs-thread-parallel-p)
                                 (funcall 'neomacs-thread-parallel-p owner))
                            parked
                            (length (all-threads))))))))
  nil)"#;

/// Replay `form` with an era open and one idle owner, and require the witness.
fn assert_b6_thread_row(form: &str, expected: expect_test::Expect) {
    let dir = tempfile::tempdir().expect("b6 witness tempdir");
    let witness_path = dir
        .path()
        .join("witness.txt")
        .to_string_lossy()
        .into_owned();
    let arm: &[(&str, &str)] = &[
        ("NEOVM_THREADS", "green"),
        ("NEOVM_B6_ERA", "1"),
        ("NEOVM_B6_WITNESS", witness_path.as_str()),
    ];
    let wrapped = format!("(progn {B6_PRELUDE} {form})");

    common::assert_oracle_parity_under_envs_expect(&wrapped, &[arm], expected);

    // `refresh` (and only refresh) returns before the Neomacs arm runs, so there
    // is no witness to demand. Every mode that actually compares an arm — verify,
    // live, and snapshot — must leave one.
    if std::env::var("NEOVM_ORACLE_MODE").as_deref() == Ok("refresh") {
        return;
    }

    let witness = std::fs::read_to_string(&witness_path).unwrap_or_else(|error| {
        panic!(
            "the b6 arm left no witness at {witness_path} ({error}): the admission \
             witness would be vacuous, so this row proves nothing"
        )
    });
    assert!(
        witness.contains("admitted=t"),
        "the b6 arm did not admit an opt-in owner; witness was {witness:?}. \
         On the dormant tree this is the expected RED (admission refused), on the \
         enabled tree it means the era never opened"
    );
    assert!(
        witness.contains("threads=2"),
        "the era arm must add exactly one thread (main + the admitted owner); \
         witness was {witness:?}"
    );
    assert!(
        witness.contains("parked=t"),
        "the idle owner never reached its park; witness was {witness:?}"
    );
}

/// `make-thread`'s GNU-visible shape with an era open and an idle owner parked.
/// The row states its arm: both editors create through `make-thread`, so the
/// compared list carries no Neomacs-only function. (Its predecessor forked on
/// `fboundp 'neomacs-make-parallel-thread` and compared the Neomacs-only hint,
/// which cannot agree once admission is on; the creator's own semantics are
/// covered by the unit rows in opt_in_creator_test.rs.)
#[test]
fn b6_e6p1_make_thread_matches_gnu_in_the_default_arm() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((name (copy-sequence "e6p1"))
       (th (make-thread
            (lambda ()
              (let ((child (make-thread (lambda () 'child))))
                (list 'result (eq (current-thread) main-thread)
                      (thread-join child))))
            name)))
  (list (func-arity #'make-thread) (threadp th) (eq name (thread-name th))
        (and (memq th (all-threads)) t)
        (thread-join th) (thread-live-p th)
        (condition-case err (make-thread (lambda () 1) 42) (error err))))
"##,
        expect_test::expect![[
            r#""OK ((1 . 3) t t t (result nil child) nil (wrong-type-argument stringp 42))""#
        ]],
    );
}

#[test]
fn b6_e8s_make_thread_join_value_name_and_liveness() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((name (copy-sequence "e8s-receipt"))
       (th (make-thread (lambda ()
                          (sleep-for 0.3)
                          (list 'result (eq (current-thread) main-thread)))
                        name)))
  (list (thread-live-p th) (string= name (thread-name th))
        (and (memq th (all-threads)) t)
        (and (memq main-thread (all-threads)) t)
        (thread-join th) (thread-join th)
        (thread-live-p th) (memq th (all-threads))))
"##,
        expect_test::expect![[r#""OK (t t t t (result nil) (result nil) nil nil)""#]],
    );
}

#[test]
fn b6_e8s_thread_name_returns_the_stored_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((name (copy-sequence "e8s-stored"))
       (th (make-thread (lambda () nil) name)))
  (list (eq name (thread-name th)) (eq name (thread-name th))))
"##,
        expect_test::expect![[r#""OK (t t)""#]],
    );
}

#[test]
fn b6_e8s_thread_join_self_and_internal_error_last_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(progn
  (thread-last-error t)
  (let ((th (make-thread (lambda () (error "inside")) "e8s-internal")))
    (list (condition-case e (thread-join (current-thread))
              (error (list 'join-self (car (cdr e)))))
          (thread-join th)
          (thread-last-error)
          (thread-last-error t)
          (thread-last-error))))
"##,
        expect_test::expect![[
            r#""OK ((join-self \"Cannot join current thread\") nil (error \"inside\") (error \"inside\") nil)""#
        ]],
    );
}

#[test]
fn b6_e8s_thread_signal_to_main_is_a_thread_event() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(progn
  (require 'thread)
  (require 'cl-lib)
  (let ((seen nil) (messages nil)
        (old (lookup-key special-event-map [thread-event])))
    (unwind-protect
        (progn
          (define-key special-event-map [thread-event]
            (lambda (event)
              (interactive "e")
              (setq seen (list (eq (car event) 'thread-event)
                               (thread-name (nth 1 event))
                               (nth 2 event) (nth 3 event)))
              (cl-letf (((symbol-function 'message)
                         (lambda (fmt &rest args)
                           (setq messages (list fmt (cdr args))))))
                (thread-handle-event event))))
          (thread-join (make-thread
                        (lambda ()
                          (thread-signal main-thread 'error '("to-main"))
                          'sent)
                        "e8s-sender"))
          ;; read-event dispatches special events; the time bound terminates it.
          (list (read-event nil nil 0.1) seen messages))
      (define-key special-event-map [thread-event] old))))
"##,
        expect_test::expect![[
            r#""OK (nil (t \"e8s-sender\" error (\"to-main\")) (\"Error %s: %S\" ((error (\"to-main\")))))""#
        ]],
    );
}

#[test]
fn b6_e8s_thread_signal_to_worker_pending_at_join_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((m (make-mutex)) (ready nil)
       (th (make-thread (lambda ()
                          (setq ready t)
                          (mutex-lock m)
                          (mutex-unlock m))
                        "e8s-target")))
  (mutex-lock m)
  (while (not ready) (thread-yield))
  ;; No yield between setting the pending signal and join's entry snapshot.
  (list (thread-signal th 'error '("external"))
        (condition-case e (thread-join th) (error (list 'join-raised (car (cdr e)))))
        (mutex-unlock m)
        (condition-case e (thread-join th) (error (list 'join2-raised (car (cdr e)))))
        (thread-last-error t)))
"##,
        expect_test::expect![[
            r#""OK (nil (join-raised \"external\") nil nil (error \"external\"))""#
        ]],
    );
}

#[test]
fn b6_e8s_a_signal_before_a_thread_runs_is_raised_at_its_next_delivery_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((held (make-mutex "e8s-held"))
       (settle (lambda (pred)
                 (let ((deadline (+ (float-time) 5.0)))
                   (while (and (not (funcall pred)) (< (float-time) deadline))
                     (thread-yield)))))
       (newborn (lambda (body)
                  (let* ((log nil)
                         (th (make-thread
                              (lambda ()
                                (condition-case err
                                    (funcall body (lambda (step) (push step log)))
                                  (error (push (list 'caught (car (cdr err))) log)))
                                'done)
                              "e8s-newborn")))
                    (thread-signal th 'error '("early"))
                    (funcall settle (lambda () (not (thread-live-p th))))
                    (list (if (thread-live-p th) 'live 'dead) (nreverse log)))))
       (waiter-log nil)
       (waiter nil)
       (results nil))
  (mutex-lock held)
  (push (funcall newborn
                 (lambda (note)
                   (let ((m (make-mutex)))
                     (mutex-lock m) (funcall note 'locked)
                     (mutex-unlock m) (funcall note 'unlocked)
                     (thread-yield) (funcall note 'yielded))))
        results)
  (push (funcall newborn
                 (lambda (note)
                   (let ((m (make-mutex)))
                     (mutex-lock m) (mutex-lock m) (funcall note 'locked-twice)
                     (mutex-unlock m) (funcall note 'still-held)
                     (mutex-unlock m) (funcall note 'released))))
        results)
  (push (funcall newborn
                 (lambda (note)
                   (let* ((m (make-mutex)) (c (make-condition-variable m)))
                     (mutex-lock m) (funcall note 'locked)
                     (condition-notify c) (funcall note 'notified)
                     (mutex-unlock m) (funcall note 'unlocked))))
        results)
  (push (funcall newborn
                 (lambda (note)
                   (funcall note 'locking)
                   (mutex-lock held) (funcall note 'got-it)))
        results)
  (push (funcall newborn
                 (lambda (note)
                   (let* ((m (make-mutex)) (c (make-condition-variable m)))
                     (mutex-lock m) (funcall note 'locked)
                     (condition-wait c) (funcall note 'woke))))
        results)
  (setq waiter
        (make-thread (lambda ()
                       (condition-case err
                           (progn (mutex-lock held) (mutex-unlock held)
                                  (push 'waiter-got-it waiter-log))
                         (error (push (list 'waiter-caught (car (cdr err)))
                                      waiter-log))))
                     "e8s-waiter"))
  (funcall settle (lambda () (eq (thread--blocker waiter) held)))
  (push (funcall newborn
                 (lambda (note)
                   (funcall note 'joining)
                   (thread-join waiter) (funcall note 'joined)))
        results)
  (push (funcall newborn
                 (lambda (note)
                   (funcall note 'sending)
                   (thread-signal waiter 'error '("to-waiter"))
                   (funcall note 'sent)))
        results)
  (funcall settle (lambda () (not (thread-live-p waiter))))
  (push waiter-log results)
  (mutex-unlock held)
  (let* ((log nil)
         (th (make-thread (lambda () (push 'ran log) 'done) "e8s-plain")))
    (thread-signal th 'error '("early"))
    (funcall settle (lambda () (not (thread-live-p th))))
    (push (list log
                (condition-case err (list 'joined (thread-join th))
                  (error (list 'join-raised (car (cdr err))))))
          results))
  (nreverse results))
"##,
        expect_test::expect![[
            r#""OK ((dead (locked (caught \"early\"))) (dead (locked-twice still-held (caught \"early\"))) (dead (locked (caught \"early\"))) (dead (locking (caught \"early\"))) (dead (locked (caught \"early\"))) (dead (joining (caught \"early\"))) (dead (sending (caught \"early\"))) ((waiter-caught \"to-waiter\")) ((ran) (join-raised \"early\")))""#
        ]],
    );
}

#[test]
fn b6_e8s_a_threads_own_error_leaves_a_pending_signal_for_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((settle (lambda (th)
                 (let ((deadline (+ (float-time) 5.0)))
                   (while (and (thread-live-p th) (< (float-time) deadline))
                     (thread-yield)))))
       (join (lambda (th)
               (condition-case err (list 'joined (thread-join th))
                 (t (list 'join-raised err)))))
       (own (make-thread (lambda () (error "own")) "e8s-own"))
       (own-wta nil)
       (shape nil)
       (results nil))
  (thread-signal own 'error '("early"))
  (funcall settle own)
  (push (list (thread-live-p own) (thread-last-error)
              (funcall join own) (funcall join own))
        results)
  (setq own-wta (make-thread (lambda () (signal 'wrong-type-argument '(own-wta)))
                             "e8s-own-wta"))
  (thread-signal own-wta 'quit '(early-quit))
  (funcall settle own-wta)
  (push (list (thread-last-error t) (funcall join own-wta)) results)
  (setq shape (make-thread (lambda () 'ok) "e8s-shape"))
  (thread-signal shape 'error '(a (b c) 3))
  (funcall settle shape)
  (push (funcall join shape) results)
  (nreverse results))
"##,
        expect_test::expect![[
            r#""OK ((nil (error \"own\") (join-raised (error \"early\")) (join-raised (error \"early\"))) ((wrong-type-argument own-wta) (join-raised (quit early-quit))) (join-raised (error a (b c) 3)))""#
        ]],
    );
}

#[test]
fn b6_e8s_thread_yield_in_main_and_worker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(list (thread-yield)
      (thread-join (make-thread (lambda () (thread-yield) 'after-yield)
                                "e8s-yielder")))
"##,
        expect_test::expect![[r#""OK (nil after-yield)""#]],
    );
}

#[test]
fn b6_e8s_mutex_recursion_and_contention_handoff() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let ((m (make-mutex "e8s-m")))
  (mutex-lock m)
  (let ((th (make-thread (lambda ()
                           (mutex-lock m)
                           (mutex-unlock m)
                           'worker-got)
                         "e8s-contender")))
    ;; Recursive acquisition while the contender waits; it can only take the
    ;; mutex after BOTH levels are released.
    (mutex-lock m)
    (list (mutex-unlock m)
          (mutex-unlock m)
          (thread-join th)
          (condition-case e (mutex-unlock m)
            (error (list 'unowned (car (cdr e))))))))
"##,
        expect_test::expect![[
            r#""OK (nil nil worker-got (unowned \"Cannot unlock mutex owned by another thread\"))""#
        ]],
    );
}

#[test]
fn b6_e8s_condition_notify_all_wakes_every_waiter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((m (make-mutex)) (c (make-condition-variable m)) (ready 0) (go nil)
       (worker (lambda ()
                 (with-mutex m
                   (setq ready (1+ ready))
                   (condition-notify c t)
                   (let ((rounds 0))
                     (while (and (not go) (< rounds 200000))
                       (setq rounds (1+ rounds))
                       (condition-wait c))
                     (list 'woke (> rounds 199000))))))
       (a (make-thread worker "e8s-broadcast-a"))
       (b (make-thread worker "e8s-broadcast-b")))
  (with-mutex m
    (while (< ready 2) (condition-wait c))
    (setq go t)
    (condition-notify c t))
  (list (thread-join a) (thread-join b)))
"##,
        expect_test::expect![[r#""OK ((woke nil) (woke nil))""#]],
    );
}

#[test]
fn b6_e8s_condition_notify_one_wakes_exactly_one_waiter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((m (make-mutex))
       (c (make-condition-variable m))
       (gate (make-condition-variable m))
       (ready 0)
       (go nil)
       (returns 0)
       (worker (lambda ()
                 (with-mutex m
                   (setq ready (1+ ready))
                   (condition-notify gate t)
                   (let ((rounds 0))
                     (while (and (not go) (< rounds 200000))
                       (condition-wait c)
                       ;; Count every return, including a wake that loops
                       ;; because `go` is still nil. Readiness uses `gate`,
                       ;; so this count is wakes of `c` only.
                       (setq returns (1+ returns))
                       (setq rounds (1+ rounds)))))))
       (a (make-thread worker "e8s-one"))
       (b (make-thread worker "e8s-two"))
       (polls 0))
  (with-mutex m
    (while (< ready 2) (condition-wait gate)))
  (with-mutex m (condition-notify c))
  ;; `returns` counts condition-wait returns on `c`. One notify wakes one
  ;; waiter. The poll is bounded so a mode that cannot park still ends.
  (while (and (< returns 1) (< polls 100000))
    (setq polls (1+ polls))
    (thread-yield))
  (let ((after-one returns))
    (with-mutex m (setq go t) (condition-notify c t))
    (list after-one (thread-join a) (thread-join b) returns)))
"##,
        expect_test::expect![[r#""OK (1 nil nil 3)""#]],
    );
}

#[test]
fn b6_e8s_binding_child_does_not_see_parent_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(progn
  (defvar e8s-special 'global)
  (defun e8s-read-special () e8s-special)
  (let ((e8s-special 'parent-let))
    (list e8s-special
          (thread-join
           (make-thread
            (lambda ()
              (list e8s-special (e8s-read-special)
                    (default-value 'e8s-special)))
            "e8s-bind-child"))
          e8s-special)))
"##,
        expect_test::expect![[r#""OK (parent-let (global global global) parent-let)""#]],
    );
}

#[test]
fn b6_e8s_binding_worker_let_and_set_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(progn
  (defvar e8s-special 'global)
  (defun e8s-read-special () e8s-special)
  (let* ((m (make-mutex))
         (c (make-condition-variable m))
         (ready nil)
         (go nil)
         (seen nil)
         (th (make-thread
              (lambda ()
                (let ((e8s-special 'worker-let))
                  (with-mutex m
                    (setq ready t)
                    (condition-notify c t)
                    (let ((rounds 0))
                      (while (and (not go) (< rounds 200000))
                        (setq rounds (1+ rounds))
                        (condition-wait c))))
                  (setq seen (list e8s-special (e8s-read-special)
                                   (default-value 'e8s-special)
                                   go))))
              "e8s-bind-worker")))
    (with-mutex m
      (let ((rounds 0))
        (while (and (not ready) (< rounds 200000))
          (setq rounds (1+ rounds))
          (condition-wait c))))
    (let ((other (list e8s-special (default-value 'e8s-special) ready)))
      (set-default 'e8s-special 'new-global)
      (with-mutex m (setq go t) (condition-notify c t))
      (thread-join th)
      (list other seen (list e8s-special (default-value 'e8s-special))))))
"##,
        expect_test::expect![[
            r#""OK ((global global t) (worker-let worker-let worker-let t) (new-global new-global))""#
        ]],
    );
}

#[test]
fn b6_e8s_binding_buffer_local_across_threads() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(progn
  (defvar e8s-local 'default)
  (let ((b (generate-new-buffer " *e8s-local*")))
    (unwind-protect
        (with-current-buffer b
          (set (make-local-variable 'e8s-local) 'buffer-base)
          (let* ((m (make-mutex))
                 (c (make-condition-variable m))
                 (ready nil)
                 (go nil)
                 (seen nil)
                 (th (make-thread
                      (lambda ()
                        (let ((e8s-local 'buffer-let))
                          (with-mutex m
                            (setq ready t)
                            (condition-notify c t)
                            (let ((rounds 0))
                              (while (and (not go) (< rounds 200000))
                                (setq rounds (1+ rounds))
                                (condition-wait c))))
                          (setq seen (list (eq (current-buffer) b)
                                           e8s-local
                                           (default-value 'e8s-local)
                                           go))))
                      "e8s-bind-buffer")))
            (with-mutex m
              (let ((rounds 0))
                (while (and (not ready) (< rounds 200000))
                  (setq rounds (1+ rounds))
                  (condition-wait c))))
            (let ((other (list e8s-local (default-value 'e8s-local) ready)))
              (setq e8s-local 'buffer-updated)
              (set-default 'e8s-local 'default-updated)
              (with-mutex m (setq go t) (condition-notify c t))
              (thread-join th)
              (list other seen
                    (list e8s-local (default-value 'e8s-local))))))
      (kill-buffer b))))
"##,
        expect_test::expect![[
            r#""OK ((buffer-base default t) (t buffer-let default-updated t) (buffer-updated default-updated))""#
        ]],
    );
}

#[test]
fn b6_p15_condition_wait_signal_reacquires_before_inner_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((m (make-mutex))
       (ready (make-condition-variable m))
       (condition (make-condition-variable m))
       (seen nil)
       (waiting nil)
       (joined nil)
       (worker nil))
  (mutex-lock m)
  (setq worker
    (make-thread
      (lambda ()
        (with-mutex m
          (mutex-lock m)
          (condition-notify ready)
          (condition-case problem
              (condition-wait condition)
            (error
             (setq seen
               (list (car problem)
                     (thread--blocker (current-thread))
                     (mutex-unlock m)
                     (mutex-unlock m)
                     (condition-case nil (mutex-unlock m)
                       (error 'released))))
             (mutex-lock m)))))))
  (condition-wait ready)
  (setq waiting (eq (thread--blocker worker) condition))
  (thread-signal worker 'error '(p15-condition-signal))
  (mutex-unlock m)
  ;; GNU thread.c:1135-1143 snapshots the pending injected error at join entry;
  ;; it may re-raise that error after waiting even though the worker handled it.
  ;; Keep the complete condition/data: a different join error must fail parity.
  (setq joined
    (condition-case join-error
        (list 'returned (thread-join worker))
      (error (list 'raised join-error))))
  (list waiting seen joined))
"##,
        expect_test::expect![[
            r#""OK (t (error nil nil nil released) (raised (error p15-condition-signal)))""#
        ]],
    );
}

#[test]
fn b6_p15_unreferenced_held_mutex_has_no_implicit_root() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let ((held (make-hash-table :test 'eq :weakness 'key))
      (paired (make-hash-table :test 'eq :weakness 'key))
      (kept nil))
  (funcall
    (lambda ()
      (let ((mutex (make-mutex)))
        (mutex-lock mutex)
        (puthash mutex t held))))
  (garbage-collect)
  (garbage-collect)
  (let ((held-count (hash-table-count held)))
    (funcall
      (lambda ()
        (let* ((mutex (make-mutex))
               (condition (make-condition-variable mutex)))
          (puthash mutex t paired)
          (puthash condition t paired)
          (setq kept condition)
          nil)))
    (garbage-collect)
    (garbage-collect)
    (let ((retained-count (hash-table-count paired)))
      (setq kept nil)
      (garbage-collect)
      (garbage-collect)
      (list held-count retained-count (hash-table-count paired)))))
"##,
        expect_test::expect![[r#""OK (0 2 0)""#]],
    );
}

#[test]
fn b6_e8s_condition_wait_interrupted_by_thread_signal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((m (make-mutex)) (c (make-condition-variable m)) (ready nil)
       (th (make-thread
            (lambda ()
              (mutex-lock m) (mutex-lock m)
              (setq ready t)
              (let ((e (condition-case err
                          (let ((rounds 0))
                            (while (< rounds 200000)
                              (setq rounds (1+ rounds))
                              (condition-wait c))
                            'not-signalled)
                        (error err))))
                (mutex-unlock m) (mutex-unlock m)
                (list (if (eq e 'not-signalled) e (car (cdr e))) 'recursion-restored)))
            "e8s-condition-signal")))
  (while (not ready) (thread-yield))
  ;; Taking m proves the target released it (every recursion level) for its wait.
  (mutex-lock m)
  (thread-signal th 'error '("wake-condition"))
  (mutex-unlock m)
  ;; thread-join re-raises the pending signal recorded at its entry, in the
  ;; JOINER, even when the target handles the signal; the second join returns
  ;; the target's own value (GNU thread.c:1131-1151, raise at 146-154).
  (list (condition-case e (thread-join th) (error (list 'join-raised (car (cdr e)))))
        (thread-join th)
        (thread-last-error t)))
"##,
        expect_test::expect![[
            r#""OK ((join-raised \"wake-condition\") (\"wake-condition\" recursion-restored) nil)""#
        ]],
    );
}

/// The `make-thread` arm, stated rather than detected: the row's stimulus is
/// `make-thread` on both editors, so it does not change when admission flips
/// on. The creator's own arm is covered by the E6-P.4 unit rows.
#[test]
fn b6_e6p1_make_thread_takes_signals_mutexes_and_last_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((creator #'make-thread)
       (m (make-mutex "e6p1-m")) (c (make-condition-variable m)) (ready nil)
       (th (funcall creator
                    (lambda ()
                      (mutex-lock m) (mutex-lock m)
                      (setq ready t)
                      (let ((e (condition-case err
                                   (let ((rounds 0))
                                     (while (< rounds 200000)
                                       (setq rounds (1+ rounds))
                                       (condition-wait c))
                                     'not-signalled)
                                 (error err))))
                        (mutex-unlock m) (mutex-unlock m)
                        (list (if (eq e 'not-signalled) e (car (cdr e)))
                              'recursion-restored)))
                    "e6p1-condition-signal")))
  (let ((deadline (+ (float-time) 5.0)))
    (while (and (not ready) (< (float-time) deadline)) (thread-yield)))
  (mutex-lock m)
  (thread-signal th 'error '("wake-opt-in"))
  (mutex-unlock m)
  (let* ((joined (list (condition-case e (thread-join th)
                         (error (list 'join-raised (car (cdr e)))))
                       (thread-join th)))
         (dier (funcall creator (lambda () (signal 'wrong-type-argument '(e6p1-dies))))))
    (thread-join dier)
    (list ready joined (thread-last-error t) (thread-last-error)
          (mutex-name m) (thread-name th))))
"##,
        expect_test::expect![[
            r#""OK (t ((join-raised \"wake-opt-in\") (\"wake-opt-in\" recursion-restored)) (wrong-type-argument e6p1-dies) nil \"e6p1-m\" \"e6p1-condition-signal\")""#
        ]],
    );
}

#[test]
fn b6_e8s_thread_signal_to_a_join_waiter_ends_the_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((m (make-mutex)) (ready nil) (target nil)
       (blocker (make-thread (lambda () (mutex-lock m) (mutex-unlock m))
                             "e8s-blocker")))
  (mutex-lock m)
  (setq target
        (make-thread (lambda ()
                       (setq ready t)
                       (condition-case e (thread-join blocker)
                         (error (car (cdr e)))))
                     "e8s-join-waiter"))
  (while (not ready) (thread-yield))
  (thread-signal target 'error '("wake-join"))
  ;; The signal ends the target's own join of the blocker (the target catches
  ;; it); main's join re-raises the entry snapshot, then returns the target's
  ;; value, and the released blocker finishes normally.
  (list (condition-case e (thread-join target) (error (list 'join-raised (car (cdr e)))))
        (mutex-unlock m)
        (thread-join target)
        (thread-join blocker)))
"##,
        expect_test::expect![[r#""OK ((join-raised \"wake-join\") nil \"wake-join\" nil)""#]],
    );
}

#[test]
fn b6_e8s_blocking_io_writer_thread_releases_main_sleep_for() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let ((ran nil))
  (let ((th (make-thread (lambda () (sleep-for 0.1) (push 'writer-ran ran))
                         "e8s-sleep-writer")))
    ;; The writer thread must run DURING main's sleep for `ran' to be set:
    ;; this is the blocking-I/O Baton release, not a timeout race.
    (sleep-for 0.5)
    (let ((during (car ran)))
      (list during (thread-join th) (car ran)))))
"##,
        expect_test::expect![[r#""OK (writer-ran (writer-ran) writer-ran)""#]],
    );
}

/// Issue #877's regression shape, replayed with the era open: a prior thread
/// join leaves an unread nudge on the shared native pipe, so main's first
/// physical wait returns at once and its second must honor the new writer's
/// earlier select deadline. The row landed in `oracle_default_threads.rs`
/// after this arm's scope was ratified; it is replayed here so the arm keeps
/// its every-row claim.
#[test]
fn b6_e8s_blocking_io_writer_thread_releases_main_sleep_for_after_prior_thread_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let ((prior (make-thread (lambda () 42) "e8s-prior")))
  (thread-join prior)
  (let* ((ran nil)
         (th (make-thread (lambda () (sleep-for 0.1) (push 'writer-ran ran))
                          "e8s-sleep-writer-after-join")))
    (sleep-for 0.5)
    (let ((during (car ran)))
      (list during (thread-join th) (car ran)))))
"##,
        expect_test::expect![[r#""OK (writer-ran (writer-ran) writer-ran)""#]],
    );
}

#[test]
fn b6_e8s_blocking_io_writer_thread_releases_accept_process_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((filter-out nil)
       (proc (make-process :name "e8s-cat" :command '("cat")
                           :noquery t :connection-type 'pipe
                           :filter (lambda (_p s) (push (substring s 0) filter-out))
                           :sentinel (lambda (_p _e) nil)))
       (th (make-thread (lambda ()
                          (sleep-for 0.2)
                          (process-send-string proc "e8s-byte"))
                        "e8s-cat-writer")))
  (unwind-protect
      ;; `cat' echoes what is written to it; main cannot write while blocked
      ;; in this accept, so only the writer thread running DURING the wait can
      ;; produce the echoed output that ends it before the 3s timeout.  (A
      ;; `make-pipe-process' has no loopback in GNU 31.1: even a main-thread
      ;; send never reaches its own filter, so it cannot carry this case.)
      (let ((woken (accept-process-output proc 3.0)))
        (list woken (car filter-out)))
    (thread-join th)
    (delete-process proc)))
"##,
        expect_test::expect![[r#""OK (t \"e8s-byte\")""#]],
    );
}

#[test]
fn b6_e8s_blocking_io_writer_thread_releases_any_process_wait() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((filter-out nil)
       (proc (make-process :name "e8s-any-cat" :command '("cat")
                           :noquery t :connection-type 'pipe
                           :filter (lambda (_p s) (push (substring s 0) filter-out))
                           :sentinel (lambda (_p _e) nil)))
       (th (make-thread (lambda ()
                          (sleep-for 0.2)
                          (process-send-string proc "e8s-any-byte"))
                        "e8s-any-writer")))
  (unwind-protect
      ;; No PROCESS argument: the wait is for any process output, which only
      ;; the writer thread's echoed send can produce while main is blocked.
      (let ((woken (accept-process-output nil 3.0)))
        (list woken (car filter-out)))
    (thread-join th)
    (delete-process proc)))
"##,
        expect_test::expect![[r#""OK (t \"e8s-any-byte\")""#]],
    );
}

#[test]
fn b6_e8s_killed_current_buffer_signals_and_moves_the_thread() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((buffer (get-buffer-create "e8s-killed"))
       (mutex (make-mutex))
       (condition (make-condition-variable mutex))
       (waiting nil)
       (go nil)
       (worker
        (with-current-buffer buffer
          (make-thread
           (lambda ()
             (with-mutex mutex
               (setq waiting t)
               (condition-case err
                   (let ((rounds 0))
                     (while (and (not go) (< rounds 200000))
                       (setq rounds (1+ rounds))
                       (condition-wait condition))
                     'not-signalled)
                 (error (list 'signalled err
                              (eq (current-buffer) buffer)
                              (buffer-live-p (current-buffer)))))))
           "e8s-killed-worker"))))
  (while (not waiting) (thread-yield))
  (let ((killed (kill-buffer buffer)))
    (with-mutex mutex
      (setq go t)
      (condition-notify condition))
    (list killed (buffer-live-p buffer)
          (condition-case e (thread-join worker)
            (error (list 'join-raised (car e))))
          (thread-join worker))))
"##,
        expect_test::expect![[
            r#""OK (t nil (join-raised thread-buffer-killed) (signalled (thread-buffer-killed) nil t))""#
        ]],
    );
}

#[test]
fn b6_e8s_buffer_disposition_t_keeps_the_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((kept (get-buffer-create "e8s-kept"))
       (release nil)
       (parked 0)
       (keeper (with-current-buffer kept
                 (make-thread (lambda ()
                                (setq parked (1+ parked))
                                (while (not release) (thread-yield))
                                (list (buffer-name)
                                      (thread-buffer-disposition
                                       (current-thread))))
                              "e8s-keeper" t))))
  (while (< parked 1) (thread-yield))
  (let ((results (list (kill-buffer kept) (buffer-live-p kept))))
    (setq release t)
    (append results (list (thread-join keeper)))))
"##,
        expect_test::expect![[r#""OK (nil t (\"e8s-kept\" t))""#]],
    );
}

#[test]
fn b6_e8s_buffer_disposition_silently_switches_without_signal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((quiet (get-buffer-create "e8s-quiet"))
       (release nil)
       (parked 0)
       (hider (with-current-buffer quiet
                (make-thread (lambda ()
                               (setq parked (1+ parked))
                               (while (not release) (thread-yield))
                               (list (eq (current-buffer) quiet)
                                     (buffer-live-p (current-buffer))
                                     (thread-buffer-disposition
                                      (current-thread))))
                             "e8s-hider" 'silently))))
  (while (< parked 1) (thread-yield))
  (let ((results (list (kill-buffer quiet) (buffer-live-p quiet))))
    (setq release t)
    (append results (list (thread-join hider)))))
"##,
        expect_test::expect![[r#""OK (t nil (nil t silently))""#]],
    );
}

#[test]
fn b6_e8s_buffer_disposition_custom_signal_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(let* ((buffer (get-buffer-create "e8s-custom"))
       (mutex (make-mutex))
       (condition (make-condition-variable mutex))
       (waiting nil)
       (go nil)
       (worker
        (with-current-buffer buffer
          (make-thread
           (lambda ()
             (with-mutex mutex
               (setq waiting t)
               (condition-case err
                   (let ((rounds 0))
                     (while (and (not go) (< rounds 200000))
                       (setq rounds (1+ rounds))
                       (condition-wait condition))
                     'not-signalled)
                 (error (list 'signalled (car err)
                              (bufferp (current-buffer)))))))
           "e8s-custom-worker" 'e8s-buffer-signal))))
  (while (not waiting) (thread-yield))
  (let ((killed (kill-buffer buffer)))
    (with-mutex mutex
      (setq go t)
      (condition-notify condition))
    (list killed (thread-join worker))))
"##,
        expect_test::expect![[r#""ERR (thread-buffer-killed)""#]],
    );
}

#[test]
fn b6_e4p2_k2_join_after_worker_void_returns_and_last_error_holds_the_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(progn
  (thread-last-error t)
  (let ((th (make-thread (lambda () unbound-var-e4p2)))
        (deadline (+ (float-time) 0.5)))
    (while (and (thread-live-p th) (< (float-time) deadline))
      (thread-yield))
    (if (thread-live-p th)
        'still-live
      (list
       (condition-case err
           (progn (thread-join th) 'joined)
         (error err))
       (thread-last-error)))))
"##,
        expect_test::expect![[r#""OK (joined (void-variable unbound-var-e4p2))""#]],
    );
}

#[test]
fn b6_e4p2_k2_join_while_worker_live_then_void_returns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(progn
  (thread-last-error t)
  (let* ((m (make-mutex))
         (c (make-condition-variable m))
         (phase 'start)
         (th (make-thread
              (lambda ()
                (with-mutex m
                  (setq phase 'blocked)
                  (condition-notify c)
                  (while (eq phase 'blocked)
                    (condition-wait c))
                  unbound-var-e4p2-live))
              "e4p2-live")))
    (with-mutex m
      (while (eq phase 'start) (condition-wait c)))
    (let ((live (thread-live-p th)))
      (with-mutex m
        (setq phase 'go)
        (condition-notify c))
      (list live
            (condition-case err
                (progn (thread-join th) 'joined)
              (error (list 'signaled (car err))))
            (thread-last-error)))))
"##,
        expect_test::expect![[r#""OK (t joined (void-variable unbound-var-e4p2-live))""#]],
    );
}

#[test]
fn b6_e4p2_k2_thread_last_error_cleanup_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r##"
(progn
  (thread-last-error t)
  (let ((th (make-thread (lambda () unbound-var-e4p2-clear)))
        (deadline (+ (float-time) 0.5)))
    (while (and (thread-live-p th) (< (float-time) deadline))
      (thread-yield))
    (if (thread-live-p th)
        'still-live
      (progn
        (thread-join th)
        (list (thread-last-error)
              (thread-last-error t)
              (thread-last-error))))))
"##,
        expect_test::expect![[
            r#""OK ((void-variable unbound-var-e4p2-clear) (void-variable unbound-var-e4p2-clear) nil)""#
        ]],
    );
}

#[test]
fn b6_thread_join_value_with_an_era_open() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_b6_thread_row(
        r#"(let ((th (make-thread (lambda () 42) "b6-joiner")))
             (thread-join th))"#,
        expect_test::expect![[r#""OK 42""#]],
    );
}
