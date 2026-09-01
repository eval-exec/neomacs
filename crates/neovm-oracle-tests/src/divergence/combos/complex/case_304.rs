//! Complex combo batch 304 — `file` I/O deep: `write-region` with
//! append/replace, `insert-file-contents` with partial range,
//! `copy-file`/`rename-file`/`make-directory`/`set-file-modes`/
//! `file-symlink-p`/`file-truename` with temp files.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx304_write_region_append_multiple_times() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"firstsecondthird\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((path (make-temp-file "neo-cx304-app")))
  (delete-file path)
  (dolist (data '("first" "second" "third"))
    (with-temp-buffer
      (insert data)
      (write-region (point-min) (point-max) path 'append 'silent)))
  (let ((content (with-temp-buffer
                   (insert-file-contents path)
                   (buffer-string))))
    (delete-file path)
    content))
"##,
        expect,
    )
}

#[test]
fn div_cx304_insert_file_contents_with_partial_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"56789ABCDE\" 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((path (make-temp-file "neo-cx304-part")))
  (with-temp-buffer
    (insert "0123456789ABCDEFGHIJ")
    (write-region (point-min) (point-max) path nil 'silent))
  (let ((content (with-temp-buffer
                   (insert-file-contents path nil 5 15)
                   (buffer-string))))
    (delete-file path)
    (list content (length content))))
"##,
        expect,
    )
}

#[test]
fn div_cx304_copy_file_with_permissions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (493 493 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((src (make-temp-file "neo-cx304-cp-src"))
       (dst (concat src "-copy")))
  (with-temp-buffer
    (insert "copy test content")
    (write-region (point-min) (point-max) src nil 'silent))
  (set-file-modes src #o755)
  (copy-file src dst)
  (let ((src-modes (file-modes src))
        (dst-modes (file-modes dst))
        (src-content (with-temp-buffer (insert-file-contents src) (buffer-string)))
        (dst-content (with-temp-buffer (insert-file-contents dst) (buffer-string))))
    (delete-file src)
    (delete-file dst)
    (list src-modes dst-modes (= src-modes dst-modes)
          (string= src-content dst-content))))
"##,
        expect,
    )
}

#[test]
fn div_cx304_rename_file_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t \"rename test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((src (make-temp-file "neo-cx304-rn-src"))
       (dst (concat src "-renamed")))
  (with-temp-buffer
    (insert "rename test")
    (write-region (point-min) (point-max) src nil 'silent))
  (rename-file src dst)
  (let ((src-exists (file-exists-p src))
        (dst-exists (file-exists-p dst))
        (dst-content (with-temp-buffer (insert-file-contents dst) (buffer-string))))
    (delete-file dst)
    (list src-exists dst-exists dst-content)))
"##,
        expect,
    )
}

#[test]
fn div_cx304_make_directory_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((base (make-temp-file "neo-cx304-mkdir" t))
       (deep (expand-file-name "a/b/c/d" base)))
  (delete-directory base t)
  (make-directory deep t)
  (let ((created (file-directory-p deep)))
    (delete-directory base t)
    created))
"##,
        expect,
    )
}

#[test]
fn div_cx304_set_file_modes_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((path (make-temp-file "neo-cx304-modes")))
  (let ((initial (file-modes path)))
    (set-file-modes path #o755)
    (let ((rwx (file-modes path)))
      (set-file-modes path #o644)
      (let ((rw- (file-modes path)))
        (delete-file path)
        (list initial rwx rw-))))
"##,
        expect,
    )
}

#[test]
fn div_cx304_file_symlink_p_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"real.txt\" nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((dir (make-temp-file "neo-cx304-sym" t))
           (real (expand-file-name "real.txt" dir))
           (link (expand-file-name "link.txt" dir)))
      (unwind-protect
          (progn
            (write-region "content" nil real nil 'silent)
            (let ((default-directory (file-name-as-directory dir)))
              (make-symbolic-link "real.txt" "link.txt"))
            (let ((link-target (file-symlink-p link))
                  (real-target (file-symlink-p real))
                  (true-link (file-truename link))
                  (true-real (file-truename real)))
              (list link-target real-target
                    (string= true-link true-real))))
        (when (file-exists-p dir) (delete-directory dir t))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx304_directory_files_recursively_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 4 (\"a.txt\" \"c.txt\" \"d.txt\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((root (make-temp-file "neo-cx304-rec" t))
       (sub (expand-file-name "sub" root)))
  (make-directory sub t)
  (dolist (f '("a.txt" "b.dat" "c.txt"))
    (write-region "x" nil (expand-file-name f root) nil 'silent))
  (write-region "x" nil (expand-file-name "d.txt" sub) nil 'silent)
  (let ((all-txt (sort (directory-files-recursively root "\\.txt$") #'string<))
        (all-files (sort (directory-files-recursively root ".") #'string<)))
    (delete-directory root t)
    (list (length all-txt) (length all-files)
          (mapcar #'file-name-nondirectory all-txt))))
"##,
        expect,
    )
}

#[test]
fn div_cx304_file_attributes_full_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 \"-rw-------\" nil 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((path (make-temp-file "neo-cx304-attr"))
       (fixed-time (seconds-to-time 100)))
  (with-temp-buffer
    (insert "0123456789")
    (write-region (point-min) (point-max) path nil 'silent))
  (set-file-times path fixed-time)
  (let ((a (file-attributes path)))
    (delete-file path)
    (list (file-attribute-size a)
          (file-attribute-modes a)
          (file-attribute-type a)
          (file-attribute-link-number a)
          (time-equal-p (file-attribute-modification-time a) fixed-time))))
"##,
        expect,
    )
}

#[test]
fn div_cx304_file_io_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((path (make-temp-file "neo-cx304-mega"))
       (data "File IO mega café 世界 test"))
  (with-temp-buffer
    (insert data)
    (write-region (point-min) (point-max) path nil 'silent))
  (let ((buf (find-file-noselect path)))
    (with-current-buffer buf
      (buffer-enable-undo)
      (put-text-property 1 5 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (let ((state (list (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (widen)
          (set-buffer-modified-p nil)
          (kill-buffer buf)
          (delete-file path)
          (list state)))))
"##,
        expect,
    )
}
