//! Divergence tests: file-name + directory + path + expand combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_file_name_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"/home/user/\" t \"file.txt\" t \"test\" t \"test\" t \"el\" t nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (file-name-directory "/home/user/file.txt")
        (string= (file-name-directory "/home/user/file.txt") "/home/user/")
        (file-name-nondirectory "/home/user/file.txt")
        (string= (file-name-nondirectory "/home/user/file.txt") "file.txt")
        (file-name-sans-extension "test.el")
        (string= (file-name-sans-extension "test.el") "test")
        (file-name-sans-extension "test.elc")
        (string= (file-name-sans-extension "test.elc") "test")
        (file-name-extension "test.el")
        (string= (file-name-extension "test.el") "el")
        (file-name-extension "test")
        (null (file-name-extension "test")))) "#,
        expect,
    );
}

#[test]
fn divergence_expand_file_name_relative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((abs (expand-file-name "foo/bar.txt" "/home/user"))
        (abs2 (expand-file-name "../bar.txt" "/home/user/docs"))
        (abs3 (expand-file-name "./test.el" "/tmp")))
    (list (string= abs "/home/user/foo/bar.txt")
          (string= abs2 "/home/user/bar.txt")
          (string= abs3 "/tmp/test.el")
          (> (length abs) 0)
          (file-name-absolute-p abs)
          (file-name-absolute-p abs2)
          (not (file-name-absolute-p "relative/path"))))) "#,
        expect,
    );
}

#[test]
fn divergence_directory_file_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"/home/user\" t \"/home/user\" t \"/home/user/\" t \"/home/user/\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (directory-file-name "/home/user/")
        (string= (directory-file-name "/home/user/") "/home/user")
        (directory-file-name "/home/user")
        (string= (directory-file-name "/home/user") "/home/user")
        (file-name-as-directory "/home/user")
        (string= (file-name-as-directory "/home/user") "/home/user/")
        (file-name-as-directory "/home/user/")
        (string= (file-name-as-directory "/home/user/") "/home/user/"))) "#,
        expect,
    );
}

#[test]
fn divergence_file_name_concat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"/a/b/file.txt\" t \"/a/b/file.txt\" t \"/tmp/sub/file.txt\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (concat (file-name-directory "/a/b/") "file.txt")
        (string= (concat (file-name-directory "/a/b/") "file.txt") "/a/b/file.txt")
        (expand-file-name "file.txt" "/a/b/")
        (string= (expand-file-name "file.txt" "/a/b/") "/a/b/file.txt")
        (file-name-concat "/tmp" "sub" "file.txt")
        (string= (file-name-concat "/tmp" "sub" "file.txt") "/tmp/sub/file.txt"))) "#,
        expect,
    );
}

#[test]
fn divergence_path_split_components() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"home\" \"user\" \"docs\" \"file.txt\") t t \"file.txt\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((split (split-string "/home/user/docs/file.txt" "/" t)))
    (list split
          (equal split '("home" "user" "docs" "file.txt"))
          (= (length split) 4)
          (car (last split))
          (string= (car (last split)) "file.txt")))) "#,
        expect,
    );
}

#[test]
fn divergence_file_truename_tilde() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 0 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((expanded (expand-file-name "~")))
    (list (file-name-absolute-p expanded)
          (> (length expanded) 1)
          (string-match "^/" expanded)
          (= (string-match "^/" expanded) 0)
          (not (string= expanded "~"))))) "#,
        expect,
    );
}

#[test]
fn divergence_file_attributes_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Hermetic: stats a fixture directory + file created inside the
    // harness-controlled shared tempdir, not live /tmp, whose nlink/size
    // drift with machine state (27/660 at the 2026-07-14 bless, 129/3300
    // three days later).  Locks GNU src/dired.c `file_attributes` (single
    // AT_SYMLINK_NOFOLLOW fstatat) semantics: car is t for a directory and
    // nil for a regular file, st_nlink is 1 for a fresh regular file,
    // st_size is the byte length, and the modes string comes from
    // filemodestring — pinned via set-file-modes (src/fileio.c
    // Fset_file_modes) so the process umask cannot leak in.  Directory
    // nlink/size stay type-asserted only (filesystem-dependent).
    let expect = expect_test::expect![[r#""OK (t t t t nil 1 27 t \"-rw-r--r--\")""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let* ((base (file-name-as-directory
              (or (getenv "NEOVM_ORACLE_TEST_TMPDIR") temporary-file-directory)))
       (dir (expand-file-name "attr-fixture" base))
       (file (expand-file-name "data.bin" dir)))
  (make-directory dir t)
  (write-region "0123456789abcdefghijklmnopq" nil file nil 'silent)
  (set-file-modes file #o644)
  (let ((dattrs (file-attributes dir))
        (fattrs (file-attributes file)))
    (list (car dattrs)
          (eq (car dattrs) t)
          (integerp (nth 1 dattrs))
          (integerp (nth 7 dattrs))
          (car fattrs)
          (nth 1 fattrs)
          (nth 7 fattrs)
          (integerp (nth 7 fattrs))
          (nth 8 fattrs)))) "#,
        expect,
    );
}

#[test]
fn divergence_make_temp_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((tmp (make-temp-file "test-div-")))
    (unwind-protect
        (list (file-exists-p tmp)
              (file-regular-p tmp)
              (> (length tmp) 0)
              (not (null
                    (string-match-p "test-div-" (file-name-nondirectory tmp))))
              (file-writable-p tmp)
              (= (nth 7 (file-attributes tmp)) 0))
      (delete-file tmp)))) "#,
        expect,
    );
}

#[test]
fn divergence_make_directory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((dir (make-temp-name
              (expand-file-name "test-dir-div-" temporary-file-directory))))
    (unwind-protect
        (progn
          (make-directory dir)
          (list (file-directory-p dir)
                (file-exists-p dir)
                (> (length dir) 0)))
      (delete-directory dir)))) "#,
        expect,
    );
}

#[test]
fn divergence_file_size_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((src (make-temp-file "test-src-"))
        (content "Hello, World!"))
    (unwind-protect
        (progn
          (write-region content nil src nil 'silent)
          (let ((dst (make-temp-file "test-dst-")))
            (unwind-protect
                (progn
                  (copy-file src dst t)
                  (list (file-exists-p src)
                        (file-exists-p dst)
                        (= (nth 7 (file-attributes src))
                           (nth 7 (file-attributes dst)))
                        (> (nth 7 (file-attributes src)) 0)))
              (when (file-exists-p dst) (delete-file dst)))))
      (delete-file src)))) "#,
        expect,
    );
}
