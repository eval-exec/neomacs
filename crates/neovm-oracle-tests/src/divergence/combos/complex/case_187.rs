//! Complex combo batch 187 — `buffer` / `indirect-buffer` / `buffer-name`
//! / `rename-buffer` / `generate-new-buffer-name` / `buffer-list`
//! reordering via `bury-buffer` / `unbury-buffer`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx187_generate_new_buffer_name_unique() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"neo-cx187-buf<3>\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create "neo-cx187-buf"))
      (buf-b (get-buffer-create "neo-cx187-buf<2>")))
  (let ((next (generate-new-buffer-name "neo-cx187-buf")))
    (kill-buffer buf-a)
    (kill-buffer buf-b)
    next))
"##,
        expect,
    );
}

#[test]
fn div_cx187_rename_buffer_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"neo-cx187-renamed\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx187-orig*")))
  (with-current-buffer buf
    (rename-buffer "neo-cx187-renamed"))
  (let ((name (buffer-name buf)))
    (prog1 name
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx187_indirect_buffer_shares_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"shared text\" \"shared text\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((base (get-buffer-create " *neo-cx187-base*"))
       (ind (make-indirect-buffer base " *neo-cx187-ind*")))
  (with-current-buffer base
    (insert "shared text"))
  (let ((base-str (with-current-buffer base (buffer-string)))
        (ind-str (with-current-buffer ind (buffer-string)))
        (base-eq-ind (eq (buffer-base-buffer ind) base)))
    (prog1 (list base-str ind-str base-eq-ind
                 (string= base-str ind-str))
      (kill-buffer ind)
      (kill-buffer base))))
"##,
        expect,
    );
}

#[test]
fn div_cx187_bury_buffer_reorders_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-position)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx187-bury-a*"))
      (buf-b (get-buffer-create " *neo-cx187-bury-b*")))
  (let ((list-before (list (memq buf-a (buffer-list))
                           (memq buf-b (buffer-list))))
        (pos-a-before (cl-position buf-a (buffer-list))))
    (bury-buffer buf-a)
    (let ((pos-a-after (cl-position buf-a (buffer-list))))
      (kill-buffer buf-a)
      (kill-buffer buf-b)
      (list pos-a-before pos-a-after
            (> pos-a-after pos-a-before)))))
"##,
        expect,
    );
}

#[test]
fn div_cx187_buffer_list_predicate_and_other_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx187-other-a*"))
      (buf-b (get-buffer-create " *neo-cx187-other-b*")))
  (set-window-buffer (selected-window) buf-a)
  (let ((other (other-buffer buf-a)))
    (prog1 (list (bufferp other)
                 (buffer-live-p other)
                 (not (eq other buf-a)))
      (kill-buffer buf-a)
      (kill-buffer buf-b))))
"##,
        expect,
    );
}

#[test]
fn div_cx187_buffer_modified_p_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx187-mod*")))
  (with-current-buffer buf
    (erase-buffer)
    (insert "content"))
  (let ((mod-after-insert (buffer-modified-p buf)))
    (with-current-buffer buf
      (set-buffer-modified-p nil))
    (let ((mod-after-clear (buffer-modified-p buf)))
      (prog1 (list mod-after-insert mod-after-clear)
        (kill-buffer buf)))))
"##,
        expect,
    );
}

#[test]
fn div_cx187_buffer_file_name_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx187-file*")))
  (list (buffer-file-name buf)
        (buffer-file-name)
        (null (buffer-file-name buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx187_kill_buffer_removes_from_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((#<killed buffer>) t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx187-kill*")))
  (let ((in-list-before (memq buf (buffer-list))))
    (kill-buffer buf)
    (let ((in-list-after (memq buf (buffer-list))))
      (list in-list-before (null in-list-after) (buffer-live-p buf)))))
"##,
        expect,
    );
}

#[test]
fn div_cx187_get_buffer_create_unique() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \" *neo-cx187-gc*\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-1 (get-buffer-create " *neo-cx187-gc*"))
      (buf-2 (get-buffer-create " *neo-cx187-gc*")))
  (prog1 (list (eq buf-1 buf-2)
               (buffer-name buf-1))
    (kill-buffer buf-1)))
"##,
        expect,
    );
}

#[test]
fn div_cx187_buffer_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((base (get-buffer-create " *neo-cx187-mega-base*"))
       (ind (make-indirect-buffer base " *neo-cx187-mega-ind*")))
  (with-current-buffer base
    (buffer-enable-undo)
    (insert "Indirect buffer mega test content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (with-current-buffer ind (buffer-string))
                         (eq (buffer-base-buffer ind) base)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (kill-buffer ind)
        (kill-buffer base)
        (list state (buffer-live-p base) (buffer-live-p ind))))))
"##,
        expect,
    );
}
