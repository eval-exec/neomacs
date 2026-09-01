//! Strict combo oracle probes, batch 130: variable watcher with buffer-local
//! dynamics, narrowing + undo + text-property + marker combo, save-restriction
//! + save-excursion with process output, and cl-loop with destructuring +
//! hash-table + multiple accumulators.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_u4_variable_watcher_buffer_local_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (defvar probe-vw-var 0)
  (let ((watcher (lambda (sym new op where)
                   (push (list op
                               (and (bufferp where) (buffer-name where)))
                         log))))
    (add-variable-watcher 'probe-vw-var watcher)
    (unwind-protect
        (progn
          (setq probe-vw-var 1)
          (setq-default probe-vw-var 2)
          (with-temp-buffer
            (setq-local probe-vw-var 3)
            (kill-local-variable 'probe-vw-var))
          (set 'probe-vw-var 4)
          (makunbound 'probe-vw-var)
          (defvar probe-vw-var 5))
      (remove-variable-watcher 'probe-vw-var watcher))
    (list (nreverse log)
          probe-vw-var
          (default-value 'probe-vw-var))))
"##;
    let expect = expect_test::expect![[
        r#""OK (((set nil) (set nil) (set \" *temp*\") (makunbound \" *temp*\") (set nil) (makunbound nil) (set nil)) 5 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u4_narrow_undo_textprop_marker_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (generate-new-buffer " *probe-nutm*"))
      (m (make-marker)))
  (unwind-protect
      (with-current-buffer b
        (buffer-enable-undo)
        (insert "AAAABBBBCCCCDDDD")
        (add-text-properties 1 5 '(face bold))
        (set-marker m 8)
        (undo-boundary)
        (narrow-to-region 3 13)
        (goto-char 6)
        (delete-region 5 8)
        (push (list 'after-delete (marker-position m) (buffer-string)) nil)
        (undo-boundary)
        (widen)
        (push (list 'after-widen (marker-position m) (buffer-string)) nil)
        (list (marker-position m)
              (marker-buffer m)
              (buffer-string)
              (get-text-property 1 'face)
              (get-text-property 5 'face)
              (buffer-substring 1 5)))
    (kill-buffer b)))
"##;
    let expect = expect_test::expect![[r#""ERR (setting-constant nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u4_cl_loop_destructure_hash_accumulator_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((h (make-hash-table :test 'equal)))
  (puthash "a" 1 h)
  (puthash "b" 2 h)
  (puthash "c" 3 h)
  (puthash "d" 4 h)
  (list (cl-loop for k being the hash-keys of h using (hash-values v)
                 sum v)
        (cl-loop for k being the hash-keys of h using (hash-values v)
                 when (> v 2) collect (cons k v))
        (cl-loop for k being the hash-keys of h
                 count (eq (aref k 0) ?a))
        (cl-loop for (a . b) in '((1 . 2) (3 . 4) (5 . 6))
                 sum (* a b))
        (cl-loop for x in '(1 2 3 4 5)
                 for y = (* x x)
                 sum y into total
                 maximize y into max-v
                 finally (return (list total max-v)))
        (cl-loop for i from 0 to 10
                 for c across "ABCDEFGHIJ"
                 when (cl-evenp i)
                 collect c into evens
                 else collect c into odds
                 finally (return (list evens odds)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u4_save_excursion_restriction_with_insert_and_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "ORIGINAL TEXT HERE")
  (let ((point-before (point))
        (undo-count (length buffer-undo-list)))
    (save-excursion
      (save-restriction
        (narrow-to-region 1 10)
        (goto-char 5)
        (insert "INSERTED")
        (undo-boundary)
        (delete-region 1 3)))
    (list (buffer-string)
          (point)
          (= (point) point-before)
          (> (length buffer-undo-list) undo-count)
          (buffer-substring 1 10))))
"##;
    let expect =
        expect_test::expect![[r#""OK (\"IGINSERTEDINAL TEXT HERE\" 25 nil t \"IGINSERTE\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u4_face_remapping_and_text_scale_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-frc*")))
  (unwind-protect
      (with-current-buffer b
        (let ((c1 (face-remap-add-relative 'default :height 1.2))
              (c2 (face-remap-add-relative 'default :weight 'bold)))
          (text-scale-set 1)
          (let ((result (list text-scale-mode-amount
                              (length face-remapping-alist)))
            (face-remap-remove-relative c1)
            (face-remap-remove-relative c2)
            (text-scale-set 0)
            (append result
                    (list text-scale-mode-amount
                          (length face-remapping-alist)))))
    (kill-buffer b))))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
