//! Green alias regressions mirrored from `oracle_green_threads`.
//!
//! GNU Emacs 31.1 supplied these expected values through the refreshed oracle
//! family. Every handoff uses a mutex/condition handshake and a final join.
//! Each test creates one World; nextest runs each case in its own process.

use super::{eval, green};

#[test]
fn nested_aliases_merge_with_existing_target_bindings() {
    let mut ev = green();
    // The bare Context has the primitive null but no Lisp definition of not.
    eval(&mut ev, "(fset 'not 'null)");
    assert_eq!(
        eval(
            &mut ev,
            r##"(eval '
(progn
(defvar g3e-source 'source-global)
(defvar g3e-target 'target-global)
(defvaralias 'g3e-link 'g3e-target)
(let* ((mutex (make-mutex)) (cv (make-condition-variable mutex))
       (ready nil) (go nil)
       (worker nil))
  (mutex-lock mutex)
  (setq worker
        (make-thread
         (lambda ()
           (list
            (let ((g3e-target 'target-outer))
              (list
               (let ((g3e-source 'source-outer))
                 (list
                  (let ((g3e-target 'target-inner))
                    (list
                     (let ((g3e-source 'source-inner))
                       (mutex-lock mutex)
                       (setq ready t)
                       (condition-notify cv)
                       (while (not go) (condition-wait cv))
                       (mutex-unlock mutex)
                       (list g3e-source g3e-target))
                     (list g3e-source g3e-target)))
                  (list g3e-source g3e-target)))
               (list g3e-source g3e-target)))
            (list g3e-source g3e-target)))))
  (while (not ready) (condition-wait cv))
  (defvaralias 'g3e-source 'g3e-link)
  (setq go t)
  (condition-notify cv)
  (mutex-unlock mutex)
  (list (thread-join worker) g3e-source g3e-target (indirect-variable 'g3e-source)))
)
 t)"##,
        ),
        r##"((((((source-inner source-inner) (target-inner target-inner)) (source-outer source-outer)) (target-outer target-outer)) (target-global target-global)) target-global target-global g3e-target)"##,
    );
}

#[test]
fn an_alias_to_a_void_target_materializes_its_owner_slot() {
    let mut ev = green();
    // The bare Context has the primitive null but no Lisp definition of not.
    eval(&mut ev, "(fset 'not 'null)");
    assert_eq!(
        eval(
            &mut ev,
            r##"(eval '
(progn
  (defvar g3e-source)
  (defvar g3e-target)
  (let* ((mutex (make-mutex))
         (condition (make-condition-variable mutex))
         (ready nil) (go nil) (worker nil))
    (mutex-lock mutex)
    (setq worker
          (make-thread
           (lambda ()
             (let ((g3e-source 'source-bound))
               (mutex-lock mutex)
               (setq ready t)
               (condition-notify condition)
               (while (not go) (condition-wait condition))
               (mutex-unlock mutex)
               (list g3e-source g3e-target)))))
    (while (not ready) (condition-wait condition))
    (defvaralias 'g3e-source 'g3e-target)
    (let ((before (boundp 'g3e-target)))
      (setq go t)
      (condition-notify condition)
      (mutex-unlock mutex)
      (list before (thread-join worker) (boundp 'g3e-source) (boundp 'g3e-target)))))
 t)"##,
        ),
        r##"(nil (source-bound source-bound) nil nil)"##,
    );
}

#[test]
fn an_alias_binding_follows_the_default_of_a_localized_target() {
    let mut ev = green();
    // The bare Context has the primitive null but no Lisp definition of not.
    eval(&mut ev, "(fset 'not 'null)");
    assert_eq!(
        eval(
            &mut ev,
            r##"(eval '
(progn
  (defvar g3e-source 'source-global)
  (defvar g3e-target 'target-global)
  (progn
    (set-buffer (get-buffer-create "g3e-localized-unit"))
    (set (make-local-variable 'g3e-target) 'target-local)
  (let* ((mutex (make-mutex))
         (condition (make-condition-variable mutex))
         (ready nil) (go nil) (worker nil))
    (mutex-lock mutex)
    (setq worker
          (make-thread
           (lambda ()
             (let ((g3e-source 'source-bound))
               (mutex-lock mutex)
               (setq ready t)
               (condition-notify condition)
               (while (not go) (condition-wait condition))
               (mutex-unlock mutex)
               (list g3e-source g3e-target (default-toplevel-value 'g3e-source) (default-toplevel-value 'g3e-target))))))
    (while (not ready) (condition-wait condition))
    (let ((aliased (defvaralias 'g3e-source 'g3e-target)))
      (setq go t)
      (condition-notify condition)
      (mutex-unlock mutex)
      (list aliased (thread-join worker) g3e-source g3e-target)))))
 t)"##,
        ),
        r##"(g3e-target (target-local target-local target-global source-bound) target-local target-local)"##,
    );
}

#[test]
fn an_alias_to_a_constant_signals_when_its_owner_resumes() {
    let mut ev = green();
    // The bare Context has the primitive null but no Lisp definition of not.
    eval(&mut ev, "(fset 'not 'null)");
    assert_eq!(
        eval(
            &mut ev,
            r##"(eval '
(progn
(progn
  (defvar g3e-source 'source-global)

  (let* ((mutex (make-mutex))
         (condition (make-condition-variable mutex))
         (ready nil) (go nil) (worker nil))
    (mutex-lock mutex)
    (setq worker
          (make-thread
           (lambda ()
             (let ((g3e-source 'source-bound))
               (mutex-lock mutex)
               (setq ready t)
               (condition-notify condition)
               (while (not go) (condition-wait condition))
               (mutex-unlock mutex)
               (list g3e-source t (default-toplevel-value 'g3e-source) (default-toplevel-value 't))))))
    (while (not ready) (condition-wait condition))
    (let ((aliased (defvaralias 'g3e-source 't)))
      (setq go t)
      (condition-notify condition)
      (mutex-unlock mutex)
      (list aliased (thread-join worker) (thread-last-error t) g3e-source t))))

)
 t)"##,
        ),
        r##"(t nil (setting-constant g3e-source) t t)"##,
    );
}

#[test]
fn a_constant_alias_signals_again_when_its_binding_pops() {
    let mut ev = green();
    // The bare Context has the primitive null but no Lisp definition of not.
    eval(&mut ev, "(fset 'not 'null)");
    assert_eq!(
        eval(
            &mut ev,
            r##"(eval '
(progn
(defvar g3e-source 'source-global)
(let* ((mutex (make-mutex)) (cv (make-condition-variable mutex))
       (ready nil) (go nil) (worker nil))
  (mutex-lock mutex)
  (setq worker
        (make-thread
         (lambda ()
           (let ((g3e-source 'source-bound))
             (mutex-lock mutex)
             (setq ready t)
             (condition-notify cv)
             (condition-case e
                 (progn
                   (while (not go) (condition-wait cv))
                   (list 'no-error (mutex-unlock mutex)))
               (error
                (list 'caught e
                      (condition-case e2 (mutex-unlock mutex) (error e2))
                      g3e-source)))))))
  (while (not ready) (condition-wait cv))
  (defvaralias 'g3e-source 't)
  (setq go t)
  (condition-notify cv)
  (mutex-unlock mutex)
  (list (thread-join worker) (thread-last-error t) g3e-source))
)
 t)"##,
        ),
        r##"(nil (setting-constant g3e-source) t)"##,
    );
}

#[test]
fn a_constant_alias_retakes_the_condition_mutex_before_signalling() {
    let mut ev = green();
    // The bare Context has the primitive null but no Lisp definition of not.
    eval(&mut ev, "(fset 'not 'null)");
    assert_eq!(
        eval(
            &mut ev,
            r##"(eval '
(progn
(defvar g3e-source 'source-global)
(let* ((mutex (make-mutex)) (cv (make-condition-variable mutex))
       (ready nil) (go nil) (caught nil) (worker nil))
  (mutex-lock mutex)
  (setq worker
        (make-thread
         (lambda ()
           (condition-case e
               (let ((g3e-source 'source-bound))
                 (mutex-lock mutex)
                 (setq ready t)
                 (condition-notify cv)
                 (while (not go) (condition-wait cv))
                 (mutex-unlock mutex)
                 'no-error)
             (error (setq caught e)
                    (list 'caught e (condition-case e2 (mutex-unlock mutex) (error e2))))))))
  (while (not ready) (condition-wait cv))
  (defvaralias 'g3e-source 't)
  (setq go t)
  (condition-notify cv)
  (mutex-unlock mutex)
  (list (thread-join worker) caught (thread-last-error t) g3e-source))
)
 t)"##,
        ),
        r##"((caught (setting-constant g3e-source) nil) (setting-constant g3e-source) nil t)"##,
    );
}

#[test]
fn forwarded_integer_alias_accepts_a_valid_binding() {
    let mut ev = green();
    eval(&mut ev, "(fset 'not 'null)");
    assert_eq!(
        eval(
            &mut ev,
            r##"(eval '
(progn
 (defvar ac-source 'source-global)
 (let* ((m (make-mutex)) (cv (make-condition-variable m))
        (ready nil) (go nil) (worker nil)
        (old gc-cons-threshold))
   (setq gc-cons-threshold 800000)
   (mutex-lock m)
   (setq worker
         (make-thread
          (lambda ()
            (let ((ac-source 987654))
              (mutex-lock m)
              (setq ready t) (condition-notify cv)
              (while (not go) (condition-wait cv))
              (mutex-unlock m)
              (list ac-source gc-cons-threshold)))))
   (while (not ready) (condition-wait cv))
   (defvaralias 'ac-source 'gc-cons-threshold)
   (setq go t) (condition-notify cv) (mutex-unlock m)
   (prog1 (list (thread-join worker) gc-cons-threshold (thread-last-error t))
   (setq gc-cons-threshold old))))
 t)"##
        ),
        r##"((987654 987654) 800000 nil)"##
    );
}

#[test]
fn forwarded_boolean_alias_normalizes_a_truthy_binding() {
    let mut ev = green();
    eval(&mut ev, "(fset 'not 'null)");
    assert_eq!(
        eval(
            &mut ev,
            r##"(eval '
(progn
 (defvar ac-source 'source-global)
 (let* ((m (make-mutex)) (cv (make-condition-variable m))
        (ready nil) (go nil) (worker nil)
        (old auto-save-no-message))
   (setq auto-save-no-message nil)
   (mutex-lock m)
   (setq worker
         (make-thread
          (lambda ()
            (let ((ac-source 'truthy))
              (mutex-lock m)
              (setq ready t) (condition-notify cv)
              (while (not go) (condition-wait cv))
              (mutex-unlock m)
              (list ac-source auto-save-no-message)))))
   (while (not ready) (condition-wait cv))
   (defvaralias 'ac-source 'auto-save-no-message)
   (setq go t) (condition-notify cv) (mutex-unlock m)
   (prog1 (list (thread-join worker) auto-save-no-message (thread-last-error t))
   (setq auto-save-no-message old))))
 t)"##
        ),
        r##"((t t) nil nil)"##
    );
}

#[test]
fn forwarded_alias_reports_its_native_type_error_on_resume() {
    let mut ev = green();
    eval(&mut ev, "(fset 'not 'null)");
    assert_eq!(
        eval(
            &mut ev,
            r##"(eval '
(progn
 (defvar ac-source 'source-global)
 (let* ((m (make-mutex)) (cv (make-condition-variable m))
        (ready nil) (go nil) (worker nil)
        (old gc-cons-threshold))
   (setq gc-cons-threshold 800000)
   (mutex-lock m)
   (setq worker
         (make-thread
          (lambda ()
            (let ((ac-source 'not-an-integer))
              (mutex-lock m)
              (setq ready t) (condition-notify cv)
              (while (not go) (condition-wait cv))
              (mutex-unlock m)
              (list ac-source gc-cons-threshold)))))
   (while (not ready) (condition-wait cv))
   (defvaralias 'ac-source 'gc-cons-threshold)
   (setq go t) (condition-notify cv) (mutex-unlock m)
   (prog1 (list (thread-join worker) gc-cons-threshold (thread-last-error t))
   (setq gc-cons-threshold old))))
 t)"##
        ),
        r##"(nil 800000 (wrong-type-argument integerp not-an-integer))"##
    );
}

#[test]
fn an_alias_retargeted_by_its_running_owner_keeps_gnu_saved_values() {
    let mut ev = green();
    eval(&mut ev, "(fset 'not 'null)");
    assert_eq!(
        eval(
            &mut ev,
            r##"(eval '
(progn
 (defvar ac-source 'source-global)
 (defvar ac-first 'first-global)
 (defvar ac-second 'second-global)
 (let* ((m (make-mutex)) (cv (make-condition-variable m))
        (ready nil) (go nil) (worker nil))
   (mutex-lock m)
   (setq worker
         (make-thread
          (lambda ()
            (let ((ac-source 'bound))
              (mutex-lock m)
              (setq ready t) (condition-notify cv)
              (while (not go) (condition-wait cv))
              (mutex-unlock m)
              (let ((before (list ac-source ac-first ac-second)))
                (defvaralias 'ac-first 'ac-second)
                (list before (list ac-source ac-first ac-second)
                      (thread-join (make-thread (lambda () (list ac-source ac-first ac-second))))
                      (list ac-source ac-first ac-second)))))))
   (while (not ready) (condition-wait cv))
   (defvaralias 'ac-source 'ac-first)
   (setq go t) (condition-notify cv) (mutex-unlock m)
   (list (thread-join worker) ac-source ac-first ac-second)))
 t)"##
        ),
        r##"(((bound bound second-global) (second-global second-global second-global) (first-global first-global first-global) (second-global second-global second-global)) first-global first-global first-global)"##
    );
}

#[test]
fn a_caught_constant_alias_error_allows_repeated_yields_without_a_peer() {
    let mut ev = green();
    eval(&mut ev, "(fset 'not 'null)");
    assert_eq!(
        eval(
            &mut ev,
            r##"(eval '
(progn
 (defvar ac-source 'source-global)
 (let* ((m (make-mutex)) (cv (make-condition-variable m))
        (ready nil) (go nil) (log nil) (worker nil))
   (mutex-lock m)
   (setq worker
         (make-thread
          (lambda ()
            (condition-case outer
                (let ((ac-source 'bound))
                  (mutex-lock m)
                  (setq ready t) (condition-notify cv)
                  (condition-case first
                      (progn (while (not go) (condition-wait cv)) (setq log (cons 'no-first log)))
                    (error (setq log (cons (list 'first first) log))))
                  (mutex-unlock m)
                  (condition-case second
                      (let ((remaining 3))
                        (while (> remaining 0)
                          (setq log (cons (list 'yielded (thread-yield)) log))
                          (setq remaining (1- remaining))))
                    (error (setq log (cons (list 'second second) log))))
                  (setq log (cons 'after-second log)))
              (error (setq log (cons (list 'outer outer) log))))
            (nreverse log))))
   (while (not ready) (condition-wait cv))
   (defvaralias 'ac-source 't)
   (setq go t) (condition-notify cv) (mutex-unlock m)
   (list (thread-join worker) (thread-last-error t))))
 t)"##
        ),
        r##"(((first (setting-constant ac-source)) (yielded nil) (yielded nil) (yielded nil) after-second (outer (setting-constant ac-source))) nil)"##
    );
}

#[test]
fn a_caught_constant_alias_failure_repeats_across_real_handoffs() {
    let mut ev = green();
    eval(&mut ev, "(fset 'not 'null)");
    assert_eq!(
        eval(
            &mut ev,
            r##"(eval '
(progn
 (defvar ac-source 'source-global)
 (progn
   (defvar ac-m) (defvar ac-cv) (defvar ac-sm) (defvar ac-scv)
   (defvar ac-scout-ready) (defvar ac-scout-go) (defvar ac-scout)
   (defvar ac-ready) (defvar ac-go) (defvar ac-log) (defvar ac-worker)
   (setq ac-m (make-mutex) ac-cv (make-condition-variable ac-m)
         ac-sm (make-mutex) ac-scv (make-condition-variable ac-sm)
         ac-scout-ready nil ac-scout-go nil ac-scout nil
         ac-ready nil ac-go nil ac-log nil ac-worker nil)
   (mutex-lock ac-sm)
   (setq ac-scout (make-thread
                (lambda ()
                  (condition-case caught
                      (progn
                        (mutex-lock ac-sm)
                        (setq ac-scout-ready t) (condition-notify ac-scv)
                        (while (not ac-scout-go) (condition-wait ac-scv))
                        (mutex-unlock ac-sm)
                        'scout-normal)
                    (error (list 'ac-scout caught
                                 (condition-case unlock (mutex-unlock ac-sm) (error unlock))))))))
   (while (not ac-scout-ready) (condition-wait ac-scv))
   (mutex-unlock ac-sm)
   (mutex-lock ac-m)
   (setq ac-worker
         (make-thread
          (lambda ()
            (condition-case outer
                (let ((ac-source 'bound))
                  (mutex-lock ac-m)
                  (setq ac-ready t) (condition-notify ac-cv)
                  (condition-case first
                      (progn (while (not ac-go) (condition-wait ac-cv)) (setq ac-log (cons 'no-first ac-log)))
                    (error (setq ac-log (cons (list 'first first) ac-log))))
                  (mutex-unlock ac-m)
                  (mutex-lock ac-sm) (setq ac-scout-go t) (condition-notify ac-scv) (mutex-unlock ac-sm)
                  (condition-case second
                      (progn (while (thread-live-p ac-scout) (thread-yield))
                             (setq ac-log (cons 'yielded ac-log)))
                    (error (setq ac-log (cons (list 'second second) ac-log))))
                  (setq ac-log (cons 'after-second ac-log)))
              (error (setq ac-log (cons (list 'outer outer) ac-log))))
            (list (nreverse ac-log) (thread-join ac-scout)))))
   (while (not ac-ready) (condition-wait ac-cv))
   (defvaralias 'ac-source 't)
   (setq ac-go t) (condition-notify ac-cv) (mutex-unlock ac-m)
   (list (thread-join ac-worker) (thread-last-error t))))
 t)"##
        ),
        r##"((((first (setting-constant ac-source)) (second (setting-constant ac-source)) after-second (outer (setting-constant ac-source))) (ac-scout (setting-constant ac-source) nil)) nil)"##
    );
}
