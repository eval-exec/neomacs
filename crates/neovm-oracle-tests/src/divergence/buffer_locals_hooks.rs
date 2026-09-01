//! Divergence tests: buffer locals deep - permanent locals, hooks, kill-all-locals.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_make_variable_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-perm-bl 0)
  (make-variable-buffer-local 'my-perm-bl)
  (setq my-perm-bl 42)
  (let ((buf (generate-new-buffer " *perm-bl-test*")))
    (with-current-buffer buf
      (list my-perm-bl
            (default-value 'my-perm-bl)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_local_which_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments mapcar 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-blf-test 0)
  (make-local-variable 'my-blf-test)
  (setq my-blf-test 99)
  (list (mapcar (lambda (entry)
                  (if (eq (car-safe entry) 'buffer-display-time)
                      (list 'buffer-display-time
                            (and (consp (cdr entry))
                                 (integerp (cadr entry))
                                 (integerp (caddr entry))
                                 (integerp (cadddr entry)))))
                    entry))
                (buffer-local-variables))
        (assq 'my-blf-test (buffer-local-variables))))"#,
        expect,
    );
}

#[test]
fn divergence_kill_all_local_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 nil 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-kalv-test 0)
  (make-local-variable 'my-kalv-test)
  (setq my-kalv-test 99)
  (kill-all-local-variables)
  (list my-kalv-test
        (local-variable-p 'my-kalv-test)
        (default-value 'my-kalv-test)))"#,
        expect,
    );
}

/// `make-local-variable' prepends to `local_var_alist', so the newest local is
/// the list HEAD. When that head entry is the permanent-local one, GNU's
/// `last'-cursor splice (`src/buffer.c:1168-1225') unlinks the ordinary locals
/// behind it while the head cons stays put -- an interior-only edit. Neomacs
/// used to infer "structure unchanged" from an unchanged head cons and kept a
/// stale symbol -> binding-cons index, so every ordinary local created before
/// a trailing permanent-local kept answering `local-variable-p' and `boundp'
/// with t.
#[test]
fn divergence_kill_all_local_variables_head_permanent_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((nil t) (nil nil t) (nil t nil) (t nil) (nil t nil t 2) (nil t) (nil nil nil nil t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  ;; permanent local created LAST: it is the alist head and survives; the
  ;; ordinary local behind it must not.
  (with-temp-buffer
    (put 'my-kalv-pa 'permanent-local t)
    (set (make-local-variable 'my-kalv-na) 1)
    (set (make-local-variable 'my-kalv-pa) 2)
    (kill-all-local-variables)
    (list (local-variable-p 'my-kalv-na) (local-variable-p 'my-kalv-pa)))
  ;; two ordinary locals behind a head permanent local: both must go.
  (with-temp-buffer
    (put 'my-kalv-pd 'permanent-local t)
    (set (make-local-variable 'my-kalv-nd1) 1)
    (set (make-local-variable 'my-kalv-nd2) 2)
    (set (make-local-variable 'my-kalv-pd) 3)
    (kill-all-local-variables)
    (list (local-variable-p 'my-kalv-nd1)
          (local-variable-p 'my-kalv-nd2)
          (local-variable-p 'my-kalv-pd)))
  ;; permanent local in the middle.
  (with-temp-buffer
    (put 'my-kalv-pb 'permanent-local t)
    (set (make-local-variable 'my-kalv-nb1) 1)
    (set (make-local-variable 'my-kalv-pb) 2)
    (set (make-local-variable 'my-kalv-nb2) 3)
    (kill-all-local-variables)
    (list (local-variable-p 'my-kalv-nb1)
          (local-variable-p 'my-kalv-pb)
          (local-variable-p 'my-kalv-nb2)))
  ;; permanent local created FIRST (alist tail).
  (with-temp-buffer
    (put 'my-kalv-pc 'permanent-local t)
    (set (make-local-variable 'my-kalv-pc) 2)
    (set (make-local-variable 'my-kalv-nc) 1)
    (kill-all-local-variables)
    (list (local-variable-p 'my-kalv-pc) (local-variable-p 'my-kalv-nc)))
  ;; the killed local must stop resolving through the value cache too.
  (with-temp-buffer
    (put 'my-kalv-pf 'permanent-local t)
    (set (make-local-variable 'my-kalv-nf) 1)
    (set (make-local-variable 'my-kalv-pf) 2)
    (kill-all-local-variables)
    (list (local-variable-p 'my-kalv-nf) (local-variable-p 'my-kalv-pf)
          (boundp 'my-kalv-nf) (boundp 'my-kalv-pf) my-kalv-pf))
  ;; a second kill does not launder the first: both keep the same permanent
  ;; head, so nothing invalidates in between.
  (with-temp-buffer
    (put 'my-kalv-pk 'permanent-local t)
    (set (make-local-variable 'my-kalv-nk) 1)
    (set (make-local-variable 'my-kalv-pk) 2)
    (kill-all-local-variables)
    (kill-all-local-variables)
    (list (local-variable-p 'my-kalv-nk) (local-variable-p 'my-kalv-pk)))
  ;; the sharpest form of the defect: two readers of one buffer in one
  ;; expression. buffer-local-variables walks the alist, local-variable-p goes
  ;; through the derived index, and they must never disagree.
  (with-temp-buffer
    (put 'my-kalv-pm 'permanent-local t)
    (set (make-local-variable 'my-kalv-nm1) 1)
    (set (make-local-variable 'my-kalv-nm2) 2)
    (set (make-local-variable 'my-kalv-pm) 3)
    (kill-all-local-variables)
    (list (and (assq 'my-kalv-nm1 (buffer-local-variables)) t)
          (local-variable-p 'my-kalv-nm1)
          (and (assq 'my-kalv-nm2 (buffer-local-variables)) t)
          (local-variable-p 'my-kalv-nm2)
          (and (assq 'my-kalv-pm (buffer-local-variables)) t)
          (local-variable-p 'my-kalv-pm))))"#,
        expect,
    );
}

#[test]
fn divergence_default_value_vssetq_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (20 10 30 30)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-dv-vs 0)
  (setq-default my-dv-vs 10)
  (make-local-variable 'my-dv-vs)
  (setq my-dv-vs 20)
  (list my-dv-vs
        (default-value 'my-dv-vs)
        (setq-default my-dv-vs 30)
        (default-value 'my-dv-vs)))"#,
        expect,
    );
}

#[test]
fn divergence_set_default_to_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-sdn-test 0)
  (setq-default my-sdn-test nil)
  (list (default-value 'my-sdn-test)
        my-sdn-test
        (boundp 'my-sdn-test)))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_local_force() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-blf-var 0)
  (make-variable-buffer-local 'my-blf-var)
  (setq my-blf-var 1)
  (let ((buf (generate-new-buffer " *blf-test*")))
    (with-current-buffer buf
      (setq my-blf-var 2))
    (prog1
        (list (buffer-local-value 'my-blf-var (current-buffer))
              (buffer-local-value 'my-blf-var buf)
              (default-value 'my-blf-var))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn divergence_hook_run_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-hook-result nil)
  (add-hook 'my-test-hook-xyz (lambda () (push 1 my-hook-result)))
  (add-hook 'my-test-hook-xyz (lambda () (push 2 my-hook-result)))
  (run-hooks 'my-test-hook-xyz)
  my-hook-result)"#,
        expect,
    );
}

#[test]
fn divergence_hook_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-hook-remove-result nil)
  (let ((fn (lambda () (push 'a my-hook-remove-result))))
    (add-hook 'my-test-hook-rm fn)
    (remove-hook 'my-test-hook-rm fn)
    (run-hooks 'my-test-hook-rm)
    my-hook-remove-result))"#,
        expect,
    );
}

#[test]
fn divergence_change_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'before-change-functions)
  (boundp 'after-change-functions)
  (boundp 'first-change-hook)
  (listp before-change-functions)
  (listp after-change-functions))"#,
        expect,
    );
}

#[test]
fn divergence_find_file_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'find-file-hook)
  (listp find-file-hook)
  (boundp 'kill-buffer-hook)
  (listp kill-buffer-hook))"#,
        expect,
    );
}
