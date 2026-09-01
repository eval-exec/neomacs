//! File-I/O deep coverage (thin area: ~2 prior files).
//!
//! write-region append/coding, insert-file-contents offsets, file-name-* edge
//! cases, file-name-completion, file-newer-than-file-p, predicates, copy-file
//! overwrite, symlinks (make-symbolic-link / file-symlink-p / file-truename),
//! directory-files-and-attributes, file-size, file-name-version, set-file-modes.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_fid_write_region_append() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"abcdef\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-fid-app-")))
  (write-region "abc" nil f nil 0)
  (write-region "def" nil f 'append 0)
  (prog1 (with-temp-buffer (insert-file-contents f) (buffer-string))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_fid_write_region_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"café\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-fid-cod-")))
  (let ((coding-system-for-write 'utf-8-unix)) (write-region "café" nil f nil 0))
  (prog1 (with-temp-buffer
           (let ((coding-system-for-read 'utf-8-unix)) (insert-file-contents f))
           (buffer-string))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_fid_insert_file_contents_offsets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"bcd\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-fid-off-")))
  (write-region "abcdef" nil f nil 0)
  (prog1 (with-temp-buffer (insert-file-contents f nil 1 4) (buffer-string))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_fid_file_name_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"c.txt\" \"/a/b/\" \"c.txt\" \"gz\" \".gz\" \"c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (file-name-nondirectory "/a/b/c.txt")
      (file-name-directory "/a/b/c.txt")
      (file-name-sans-versions "c.txt~")
      (file-name-extension "c.tar.gz")
      (file-name-extension "c.tar.gz" t)
      (file-name-base "/a/b/c.txt"))
"##,
        expect,
    );
}

#[test]
fn div_fid_expand_file_name_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"/a/b/x\" \"/a/b/x\" \"/a/b/x\" \"/a/b\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (expand-file-name "x" "/a/b")
      (expand-file-name "../x" "/a/b/c")
      (expand-file-name "x" "/a/b/")
      (expand-file-name "" "/a/b"))
"##,
        expect,
    );
}

#[test]
fn div_fid_substitute_in_file_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"/h/x\" \"/b\" \"~/c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((process-environment (cons "HOME=/h" process-environment)))
  (list (substitute-in-file-name "$HOME/x")
        (substitute-in-file-name "/a//b")
        (substitute-in-file-name "/a/~/c")))
"##,
        expect,
    );
}

#[test]
fn div_fid_file_name_completion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"ap\" (\"apple\" \"apricot\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((dir (make-temp-file "neo-fid-comp-" t)))
  (write-region "" nil (expand-file-name "apple" dir) nil 0)
  (write-region "" nil (expand-file-name "apricot" dir) nil 0)
  (prog1 (list (file-name-completion "ap" dir)
               (sort (file-name-all-completions "ap" dir) 'string<))
    (ignore-errors (delete-directory dir t))))
"##,
        expect,
    );
}

#[test]
fn div_fid_file_newer_than_file_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (make-temp-file "neo-fid-a-")) (b (make-temp-file "neo-fid-b-")))
  (set-file-times a 100)
  (set-file-times b 100)
  (prog1 (list (eq (file-newer-than-file-p a b) nil)
               (eq (file-newer-than-file-p b a) nil))
    (ignore-errors (delete-file a)) (ignore-errors (delete-file b))))
"##,
        expect,
    );
}

#[test]
fn div_fid_file_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-fid-p-")))
  (write-region "x" nil f nil 0)
  (prog1 (list (file-exists-p f) (file-readable-p f) (file-writable-p f)
               (file-regular-p f) (file-symlink-p f) (file-directory-p f))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_fid_copy_file_overwrite() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"data\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (make-temp-file "neo-fid-ca-")) (b (make-temp-file "neo-fid-cb-")))
  (write-region "data" nil a nil 0)
  (copy-file a b t)
  (prog1 (with-temp-buffer (insert-file-contents b) (buffer-string))
    (ignore-errors (delete-file a)) (ignore-errors (delete-file b))))
"##,
        expect,
    );
}

#[test]
fn div_fid_symlink_truename() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((dir (make-temp-file "neo-fid-sl-" t)))
  (let ((target (expand-file-name "target" dir))
        (link (expand-file-name "link" dir)))
    (write-region "" nil target nil 0)
    (make-symbolic-link target link)
    (prog1 (list (if (file-symlink-p link) t nil)
                 (eq (file-truename link) (file-truename target)))
      (ignore-errors (delete-directory dir t)))))
"##,
        expect,
    );
}

#[test]
fn div_fid_directory_files_and_attributes_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((dir (make-temp-file "neo-fid-dfa-" t)))
  (write-region "" nil (expand-file-name "x" dir) nil 0)
  (write-region "" nil (expand-file-name "y" dir) nil 0)
  (prog1 (length (directory-files-and-attributes dir nil "^[^.]"))
    (ignore-errors (delete-directory dir t))))
"##,
        expect,
    );
}

#[test]
fn div_fid_file_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function file-size)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-fid-sz-")))
  (write-region "hello" nil f nil 0)
  (prog1 (file-size f) (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_fid_set_file_modes_explicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 420""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-fid-sm-")))
  (set-file-modes f #o644)
  (prog1 (file-modes f) (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_fid_file_name_version() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"x.txt~\" void-function void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (file-name-sans-versions "x.txt~" 2)
      (condition-case e (file-name-version "x.txt.~3~") (error (car e)))
      (condition-case e (file-name-version "plain") (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_fid_temp_file_dir_and_make_nearby() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-nearby-temp-file "neo-fid-mnt-")))
  (prog1 (list (stringp (temporary-file-directory))
               (file-exists-p f)
               (string-prefix-p "neo-fid-mnt-" (file-name-nondirectory f)))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}
