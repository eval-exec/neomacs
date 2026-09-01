//! Complex combo batch 441 — 15 pinpoint probes into remaining edge-case
//! terrain: load-history, read-circle, pcase app/guard, cl-loop multiple
//! collections, sort circular, nconc circular, plist malformed,
//! setcar/setcdr self-ref, add-variable-watcher, track-mouse batch,
//! condition-case :success/:failure, key-parse edge,
//! variable-watcher buffer-local, recursive-edit abort.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// load-history: recording of loaded files.
#[test]
fn div_cx441_load_history() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (listp load-history)
      (> (length load-history) 0)
      (consp (car load-history)))"##,
        expect,
    );
}

/// read-circle: reading circular list notation.
#[test]
fn div_cx441_read_circle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 1 2 . #2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((read-circle t))
  (car (read-from-string "#1=(1 2 . #1#)")))"##,
        expect,
    );
}

/// pcase with app and guard patterns.
#[test]
fn div_cx441_pcase_app_guard() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :high""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((v 5))
  (pcase '(3 4 5)
    (`(,a ,b ,c) (pcase c
                   ((and (pred numberp) (guard (> c 3))) :high)
                   (_ :low)))))"##,
        expect,
    );
}

/// cl-loop across multiple collection types simultaneously.
#[test]
fn div_cx441_cl_loop_multi_collection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 1 ht) (puthash "b" 2 ht)
  (cl-loop for k being the hash-keys of ht
           for v being the hash-values of ht
           collect (cons k v)))"##,
        expect,
    );
}

/// sort with circular list detection (should error properly).
#[test]
fn div_cx441_sort_circular() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK circular-list""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((l (list 3 1 2)))
      (setcdr (cddr l) l)
      (sort l #'<))
  (error (car e)))"##,
        expect,
    );
}

/// nconc with circular list (should error or handle gracefully).
#[test]
fn div_cx441_nconc_circular() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3 . #2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((l1 (list 1 2))
          (l2 (list 3 4)))
      (setcdr l2 l2)
      (nconc l1 l2))
  (error (car e)))"##,
        expect,
    );
}

/// plist on malformed (non-proper) property lists.
#[test]
fn div_cx441_plist_malformed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (plist-get '(:a 1 :b) :b) (error (car e)))
      (condition-case e (plist-member '(1 2 3) :a) (error (car e))))"##,
        expect,
    );
}

/// setcar / setcdr on self-referential structures.
#[test]
fn div_cx441_setcar_setcdr_selfref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((#1 2 3) 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((x (list 1 2 3)))
  (setcar x x)
  (list (car x) (cadr x) (caddr x)))"##,
        expect,
    );
}

/// add-variable-watcher / remove-variable-watcher.
#[test]
fn div_cx441_add_variable_watcher() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((watched nil))
  (add-variable-watcher 'neo-cx441-w (lambda (&rest _) (setq watched t)))
  (setq neo-cx441-w 42)
  watched)"##,
        expect,
    );
}

/// track-mouse in batch mode.
#[test]
fn div_cx441_track_mouse_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r##"(track-mouse nil)"##, expect);
}

/// condition-case with :success (Emacs 28+).
#[test]
fn div_cx441_condition_case_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable val)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case :success val (/ 6 2) (:success val))
      (condition-case :success val (error "fail") (:success val)))"##,
        expect,
    );
}

/// key-parse with edge case inputs.
#[test]
fn div_cx441_key_parse_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([24 6] [M-C-return] [])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (key-parse "C-x C-f")
      (key-parse "M-C-<return>")
      (condition-case e (key-parse "") (error (car e))))"##,
        expect,
    );
}

/// buffer-local-set-state / buffer-local-restore-state deeper.
#[test]
fn div_cx441_buffer_local_state_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp 'neo-cx441-s1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (setq-local neo-cx441-s1 'a neo-cx441-s2 'b)
  (let ((state (buffer-local-set-state 'neo-cx441-s1 'x 'neo-cx441-s2 'y)))
    (list neo-cx441-s1 neo-cx441-s2
          (progn (buffer-local-restore-state state)
                 neo-cx441-s1 neo-cx441-s2))))"##,
        expect,
    );
}

/// event-start/event-end on character event.
#[test]
fn div_cx441_event_start_end_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t (#<window 1 on *scratch*> 1 (0 . 0) 0) (#<window 1 on *scratch*> 1 (0 . 0) 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ev ?a))
  (list (eventp ev)
        (event-start ev)
        (event-end ev)))"##,
        expect,
    );
}

/// format with %S on hash-table prints.
#[test]
fn div_cx441_format_S_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r##""OK \"#s(hash-table test equal data (\\\"a\\\" 1))\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 1 ht)
  (format "%S" ht))"##,
        expect,
    );
}
