//! Complex combo batch 395 — `file` I/O ultimate: write-region append,
//! insert-file-contents partial range, copy-file with permissions,
//! rename-file, make-directory recursive, file-symlink-p, file-truename,
//! directory-files-recursively, file-attributes full query.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx395_write_region_append_multiple_times() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"firstsecondthird\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((path (make-temp-file "neo-cx395-app")))
  (delete-file path)
  (dolist (data '("first" "second" "third"))
    (with-temp-buffer
      (insert data)
      (write-region (point-min) (point-max) path 'append 'silent)))
  (let ((content (with-temp-buffer (insert-file-contents path) (buffer-string))))
    (delete-file path)
    content))
"##,
        expect,
    )
}

#[test]
fn div_cx395_insert_file_contents_partial_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"56789ABCDE\" 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((path (make-temp-file "neo-cx395-part")))
  (with-temp-buffer
    (insert "0123456789ABCDEFGHIJ")
    (write-region (point-min) (point-max) path nil 'silent))
  (let ((content (with-temp-buffer (insert-file-contents path nil 5 15) (buffer-string))))
    (delete-file path)
    (list content (length content))))
"##,
        expect,
    )
}

#[test]
fn div_cx395_copy_file_with_permissions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (493 493 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((src (make-temp-file "neo-cx395-cp-src"))
       (dst (concat src "-copy")))
  (with-temp-buffer (insert "copy test") (write-region (point-min) (point-max) src nil 'silent))
  (set-file-modes src #o755)
  (copy-file src dst)
  (let ((src-modes (file-modes src)) (dst-modes (file-modes dst)))
    (delete-file src)
    (delete-file dst)
    (list src-modes dst-modes (= src-modes dst-modes))))
"##,
        expect,
    )
}

#[test]
fn div_cx395_rename_file_and_make_directory() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((src (make-temp-file "neo-cx395-rn-src"))
       (dst (concat src "-renamed")))
  (with-temp-buffer (insert "rename test") (write-region (point-min) (point-max) src nil 'silent))
  (rename-file src dst)
  (let ((result (list (file-exists-p src) (file-exists-p dst))))
    (delete-file dst)
    (let* ((base (make-temp-file "neo-cx395-mkdir" t))
           (deep (expand-file-name "a/b/c/d" base)))
      (delete-directory base t)
      (make-directory deep t)
      (let ((created (file-directory-p deep)))
        (delete-directory base t)
        (append result (list created)))))
"##,
        expect,
    )
}

#[test]
fn div_cx395_file_symlink_and_truename() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"real.txt\" nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((dir (make-temp-file "neo-cx395-sym" t))
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
              (list link-target real-target (string= true-link true-real))))
        (when (file-exists-p dir) (delete-directory dir t))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx395_directory_files_recursively() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 4 (\"a.txt\" \"c.txt\" \"d.txt\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((root (make-temp-file "neo-cx395-rec" t))
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
fn div_cx395_file_attributes_full_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 \"-rw-------\" nil 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((path (make-temp-file "neo-cx395-attr")))
  (with-temp-buffer (insert "0123456789") (write-region (point-min) (point-max) path nil 'silent))
  (let ((a (file-attributes path)))
    (delete-file path)
    (list (file-attribute-size a)
          (file-attribute-modes a)
          (file-attribute-type a)
          (file-attribute-link-number a))))
"##,
        expect,
    )
}

#[test]
fn div_cx395_file_name_decomposition_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"/home/user/file.txt\" \"/home/user/\" \"file.txt\" \"txt\" \"file\") (\"/path/to/archive.tar.gz\" \"/path/to/\" \"archive.tar.gz\" \"gz\" \"archive.tar\") (\"simple\" nil \"simple\" nil \"simple\") (\"/dir/noext\" \"/dir/\" \"noext\" nil \"noext\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (path)
          (list path
                (file-name-directory path)
                (file-name-nondirectory path)
                (file-name-extension path)
                (file-name-sans-extension (file-name-nondirectory path))))
        '("/home/user/file.txt"
          "/path/to/archive.tar.gz"
          "simple"
          "/dir/noext"))
"##,
        expect,
    )
}

#[test]
fn div_cx395_set_file_modes_and_times() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-cx395-modes" t))
       (f1 (expand-file-name "f1" dir))
       (f2 (expand-file-name "f2" dir)))
  (write-region "1" nil f1 nil 'silent)
  (write-region "2" nil f2 nil 'silent)
  (set-file-modes f1 #o755)
  (set-file-modes f2 #o644)
  (set-file-times f1 '(100 0 0 0))
  (set-file-times f2 '(200 0 0 0))
  (let ((newer (file-newer-than-file-p f2 f1)))
    (delete-directory dir t)
    newer))
"##,
        expect,
    )
}

#[test]
fn div_cx395_file_io_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((path (make-temp-file "neo-cx395-mega"))
       (data "File IO mega café 世界 test"))
  (delete-file path)
  (with-temp-buffer (insert data) (write-region (point-min) (point-max) path nil 'silent))
  (let ((buf (find-file-noselect path)))
    (with-current-buffer buf
      (buffer-enable-undo)
      (put-text-property 1 5 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 12)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 14)
        (let ((state (list (file-attribute-size (file-attributes path))
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (widen()
          (set-buffer-modified-p nil)
          (kill-buffer buf)
          (delete-file path)
          (list state)))))
"##,
        expect,
    )
}
