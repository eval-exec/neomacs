//! Divergence tests: file operations deep, file attributes, directory ops.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_file_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 12 t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((attrs (file-attributes ".")))
  (list (consp attrs)
        (length attrs)
        (file-directory-p ".")
        (file-symlink-p ".")
        (integerp (nth 7 attrs))))"#,
        expect,
    );
}

#[test]
fn divergence_file_mtime_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((tmp (make-temp-file "neovm-mtime-"))
          (attrs (file-attributes tmp)))
  (unwind-protect
      (list (consp (nth 5 attrs))
            (integerp (nth 7 attrs))
            (= (nth 7 attrs) 0)
            (file-writable-p tmp)
            (file-readable-p tmp))
    (delete-file tmp)))"#,
        expect,
    );
}

#[test]
fn divergence_expand_file_name_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"[ORACLE-PROJECT-ROOT]/crates/neovm-oracle-tests/foo/baz\" \"[ORACLE-HOME]/test\" \"[ORACLE-PROJECT-ROOT]/crates/neovm-oracle-tests/test\" \"/absolute/path\" t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (expand-file-name "foo/bar/../baz")
  (expand-file-name "~/test")
  (expand-file-name "./test")
  (expand-file-name "/absolute/path")
  (file-name-absolute-p "/foo")
  (file-name-absolute-p "foo"))"#,
        expect,
    );
}

#[test]
fn divergence_file_name_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"/a/b/\" \"c.txt\" \"el\" \"gz\" \"test\" \"test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (file-name-directory "/a/b/c.txt")
  (file-name-nondirectory "/a/b/c.txt")
  (file-name-extension "test.el")
  (file-name-extension "test.tar.gz")
  (file-name-sans-extension "test.el")
  (file-name-base "test.el"))"#,
        expect,
    );
}

#[test]
fn divergence_file_copy_rename() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t \"content\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((src (make-temp-file "neovm-copy-src-"))
        dst)
  (unwind-protect
      (progn
        (write-region "content" nil src nil 'silent)
        (setq dst (make-temp-file "neovm-copy-dst-"))
        (copy-file src dst t)
        (list (file-exists-p src)
              (file-exists-p dst)
              (with-temp-buffer
                (insert-file-contents dst)
                (buffer-string))))
    (when (file-exists-p src) (delete-file src))
    (when (file-exists-p dst) (delete-file dst))))"#,
        expect,
    );
}

#[test]
fn divergence_make_directory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t (\".\" \"..\") 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((dir (make-temp-file "neovm-mkdir-" t)))
  (unwind-protect
      (list (file-directory-p dir)
            (file-exists-p dir)
            (directory-files dir)
            (length (directory-files dir)))
    (delete-directory dir t)))"#,
        expect,
    );
}

#[test]
fn divergence_path_separators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable directory-sep-char)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (stringp path-separator)
  (string= path-separator ":")
  (stringp directory-sep-char)
  (= directory-sep-char ?/))"#,
        expect,
    );
}

#[test]
fn divergence_file_executable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 420 t 493 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((path (make-temp-file "neovm-executable-")))
  (unwind-protect
      (progn
        (set-file-modes path #o644)
        (let ((before (file-executable-p path))
              (before-modes (file-modes path)))
          (set-file-modes path #o755)
          (let ((after-modes (file-modes path)))
            (list before
                  before-modes
                  (file-executable-p path)
                  after-modes
                  (integerp after-modes)))))
    (delete-file path)))"#,
        expect,
    );
}

#[test]
fn divergence_write_read_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Hello World\" 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((tmp (make-temp-file "neovm-rw-")))
  (unwind-protect
      (progn
        (write-region "Hello World" nil tmp nil 'silent)
        (list (with-temp-buffer
                (insert-file-contents tmp)
                (buffer-string))
              (file-attribute-size (file-attributes tmp))))
    (delete-file tmp)))"#,
        expect,
    );
}

#[test]
fn divergence_insert_file_contents_partial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"DEFG\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((tmp (make-temp-file "neovm-partial-")))
  (unwind-protect
      (progn
        (write-region "ABCDEFGHIJ" nil tmp nil 'silent)
        (with-temp-buffer
          (insert-file-contents tmp nil 3 7)
          (buffer-string)))
    (delete-file tmp)))"#,
        expect,
    );
}
