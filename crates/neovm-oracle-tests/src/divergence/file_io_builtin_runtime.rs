//! File-I/O builtin parity: insert-file-contents (partial beg/end, replace,
//! return count), write-region (append, string source, beg/end), make-temp-name
//! uniqueness, directory-files (+match), file-attributes of a dir, file-modes,
//! file-name directory/nondirectory/as-directory + abbreviate-file-name.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn directory_files() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"a.txt\" \"b.txt\") 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((d (make-temp-file "neo-dir-" t)))
  (unwind-protect (progn
    (with-temp-file (expand-file-name "a.txt" d) (insert "x"))
    (with-temp-file (expand-file-name "b.txt" d) (insert "y"))
    (list (sort (directory-files d nil "\\.txt$") #'string<)
          (length (directory-files d))))
    (delete-directory d t)))"##,
        expect,
    );
}

#[test]
fn file_attrs_dir() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((d (make-temp-file "neo-fad-" t)))
  (unwind-protect (let ((a (file-attributes d)))
    (list (eq (car a) t) (file-directory-p d) (file-exists-p d)))
    (delete-directory d t)))"##,
        expect,
    );
}

#[test]
fn file_modes_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (420 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "neo-fm-")))
  (unwind-protect (progn (set-file-modes f #o644)
    (list (file-modes f) (file-readable-p f) (file-writable-p f)))
    (delete-file f)))"##,
        expect,
    );
}

#[test]
fn file_name_dir_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"/a/b/\" \"\" \"/a/b/\" \"/a/b\" \"~/x\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (file-name-directory "/a/b/") (file-name-nondirectory "/a/b/")
        (file-name-as-directory "/a/b") (directory-file-name "/a/b/")
        (abbreviate-file-name (expand-file-name "~/x")))"##,
        expect,
    );
}

#[test]
fn insert_file_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "neo-ifc-")))
  (unwind-protect (progn (with-temp-file f (insert "hello"))
    (with-temp-buffer (let ((r (insert-file-contents f))) (list (nth 1 r) (buffer-string)))))
    (delete-file f)))"##,
        expect,
    );
}

#[test]
fn insert_file_partial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"2345\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "neo-ifp-")))
  (unwind-protect (progn (with-temp-file f (insert "0123456789"))
    (with-temp-buffer (insert-file-contents f nil 2 6) (buffer-string)))
    (delete-file f)))"##,
        expect,
    );
}

#[test]
fn insert_file_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"new content\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "neo-ifr-")))
  (unwind-protect (progn (with-temp-file f (insert "new content"))
    (with-temp-buffer (insert "old") (insert-file-contents f nil nil nil t) (buffer-string)))
    (delete-file f)))"##,
        expect,
    );
}

#[test]
fn make_temp_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((n1 (make-temp-name "/tmp/neo-")) (n2 (make-temp-name "/tmp/neo-")))
  (list (stringp n1) (string-prefix-p "/tmp/neo-" n1) (not (string= n1 n2))))"##,
        expect,
    );
}

#[test]
fn write_region_append() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"line1\\nline2\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "neo-wra-")))
  (unwind-protect (progn
    (write-region "line1\n" nil f)
    (write-region "line2\n" nil f t)
    (with-temp-buffer (insert-file-contents f) (buffer-string)))
    (delete-file f)))"##,
        expect,
    );
}

#[test]
fn write_region_string_beg_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "neo-wrs-")))
  (unwind-protect (progn (write-region "abcdefgh" 2 5 f)
    (with-temp-buffer (insert-file-contents f) (buffer-string)))
    (delete-file f)))"##,
        expect,
    );
}
