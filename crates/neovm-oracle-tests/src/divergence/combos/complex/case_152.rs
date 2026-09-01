//! Complex combo batch 152 — `dired` / `dired-x` / `dired+` operations:
//! listing, marking, deletion, compression, recursive operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx152_dired_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'dired)
      (list (fboundp 'dired)
            (fboundp 'dired-jump)
            (boundp 'dired-listing-switches)
            (boundp 'dired-dwim-target)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx152_dired_listing_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"alpha.txt\" \"beta.dat\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-cx152-dired" t))
       (f1 (expand-file-name "alpha.txt" dir))
       (f2 (expand-file-name "beta.dat" dir)))
  (write-region "alpha content" nil f1 nil 'silent)
  (write-region "beta content" nil f2 nil 'silent)
  (let ((entries (directory-files dir nil "^[^.]")))
    (delete-directory dir t)
    (sort entries #'string<)))
"##,
        expect,
    );
}

#[test]
fn div_cx152_dired_x_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'dired-x)
      (list (fboundp 'dired-omit-mode)
            (boundp 'dired-omit-files)
            (boundp 'dired-guess-shell-alist-user)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx152_directory_files_full_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a.txt\" \"b.txt\" \"sub\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-cx152-full" t))
       (sub (expand-file-name "sub" dir)))
  (make-directory sub t)
  (write-region "x" nil (expand-file-name "a.txt" dir) nil 'silent)
  (write-region "x" nil (expand-file-name "b.txt" dir) nil 'silent)
  (write-region "x" nil (expand-file-name "c.txt" sub) nil 'silent)
  (let ((entries (directory-files dir t "^[^.]")))
    (delete-directory dir t)
    (sort (mapcar #'file-name-nondirectory entries) #'string<)))
"##,
        expect,
    );
}

#[test]
fn div_cx152_dired_recursive_compress() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (20 (\"file.txt\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((dir (make-temp-file "neo-cx152-comp" t))
           (f (expand-file-name "file.txt" dir)))
      (write-region "compressible content" nil f nil 'silent)
      (let ((before-size (file-attribute-size (file-attributes f))))
        (condition-case err
            (dired-compress-file f)
          (error :err))
        (let ((after-files (directory-files dir nil "^[^.]")))
          (delete-directory dir t)
          (list before-size after-files))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx152_dired_mode_buffer_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((dir (make-temp-file "neo-cx152-mode" t))
           (buf-name (format "%s" dir)))
      (write-region "x" nil (expand-file-name "f.txt" dir) nil 'silent)
      (let ((buf (dired-noselect dir)))
        (with-current-buffer buf
          (prog1 (list (eq major-mode 'dired-mode)
                       (buffer-string))
            (kill-buffer buf))))
      (delete-directory dir t))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx152_dired_filename_quoting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"file with spaces.txt\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-cx152-quote" t))
       (weird-name "file with spaces.txt")
       (weird-path (expand-file-name weird-name dir)))
  (write-region "x" nil weird-path nil 'silent)
  (let* ((entries (directory-files dir nil))
         (full-entries (directory-files dir t))
         (full-hit (car (member weird-path full-entries))))
    (delete-directory dir t)
    (list (not (null (member weird-name entries)))
          (and full-hit (file-name-nondirectory full-hit))
          (and full-hit (file-name-absolute-p full-hit)))))
"##,
        expect,
    );
}

#[test]
fn div_cx152_dired_sort_by_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'dired-sort-toggle)
          (boundp 'dired-actual-switches)
          (boundp 'dired-directory)
          (boundp 'dired-use-ls-dired))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx152_dired_create_directory() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((parent (make-temp-file "neo-cx152-mkdir" t))
       (child (expand-file-name "child-dir" parent)))
  (make-directory child t)
  (let ((created (file-directory-p child)))
    (delete-directory parent t)
    created))
"##,
        expect,
    );
}

#[test]
fn div_cx152_dired_writable_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-cx152-write" t))
       (f (expand-file-name "f.txt" dir)))
  (write-region "x" nil f nil 'silent)
  (let ((writable (file-writable-p f))
        (read-only (progn (set-file-modes f #o444) (file-writable-p f))))
    (set-file-modes f #o644)
    (delete-directory dir t)
    (list writable (not read-only))))
"##,
        expect,
    );
}

#[test]
fn div_cx152_dired_recursive_find() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 (\"a.txt\" \"b.txt\" \"c.txt\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((root (make-temp-file "neo-cx152-find" t))
       (sub1 (expand-file-name "sub1" root))
       (sub2 (expand-file-name "sub2" sub1)))
  (make-directory sub2 t)
  (write-region "x" nil (expand-file-name "a.txt" root) nil 'silent)
  (write-region "x" nil (expand-file-name "b.txt" sub1) nil 'silent)
  (write-region "x" nil (expand-file-name "c.txt" sub2) nil 'silent)
  (let ((all (sort (directory-files-recursively root "\\.txt$")
                   #'string<))
        (names (sort (mapcar #'file-name-nondirectory
                             (directory-files-recursively root "\\.txt$"))
                     #'string<)))
    (delete-directory root t)
    (list (length all) names)))
"##,
        expect,
    );
}

#[test]
fn div_cx152_dired_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-cx152-mega" t))
       (f1 (expand-file-name "alpha.txt" dir))
       (f2 (expand-file-name "beta.dat" dir)))
  (write-region "alpha content" nil f1 nil 'silent)
  (write-region "beta content" nil f2 nil 'silent)
  (let ((entries (directory-files dir nil "^[^.]")))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert (mapconcat #'identity entries "\n"))
      (put-text-property 1 5 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (let ((state (list entries
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (delete-directory dir t)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}
