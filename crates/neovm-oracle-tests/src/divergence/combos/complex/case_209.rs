//! Complex combo batch 209 — `thread` / `mutex` / `condition-variable`
//! and dynamic-library availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx209_thread_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'make-thread)
          (fboundp 'thread-join)
          (fboundp 'thread-signal)
          (fboundp 'current-thread)
          (fboundp 'all-threads)
          (boundp 'main-thread))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx209_mutex_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'make-mutex)
          (fboundp 'mutex-lock)
          (fboundp 'mutex-unlock)
          (fboundp 'with-mutex)
          (fboundp 'mutex-owner))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx209_condition_variable_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'make-condition-variable)
          (fboundp 'condition-wait)
          (fboundp 'condition-notify)
          (fboundp 'condition-broadcast))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx209_current_thread_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ct (current-thread)))
      (list (threadp ct)
            (eq ct main-thread)
            (consp (all-threads))
            (>= (length (all-threads)) 1)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx209_mutex_lock_unlock_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((m (make-mutex :name "neo-cx209")))
      (mutex-lock m)
      (let ((owner-locked (mutex-owner m)))
        (mutex-unlock m)
        (let ((owner-unlocked (mutex-owner m)))
          (list (eq owner-locked (current-thread))
                (null owner-unlocked)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx209_with_mutex_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((m (make-mutex :name "neo-cx209-wm"))
          (result nil))
      (with-mutex m
        (push :inside result))
      (list (null (mutex-owner m))
            (nreverse result)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx209_dynamic_library_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'dll-load)
          (fboundp 'dll-unload)
          (fboundp 'dll-loaded-p)
          (boundp 'dynamic-library-alist))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx209_thread_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ct (current-thread))
          (mt main-thread))
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Thread mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (threadp ct) (eq ct mt)
                             (>= (length (all-threads)) 1)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
