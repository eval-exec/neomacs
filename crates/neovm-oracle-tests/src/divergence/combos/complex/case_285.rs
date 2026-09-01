//! Complex combo batch 285 — `file-name-handler-alist` / `tramp` method
//! dispatch / `file-remote-p` / `file-local-name` / `expand-file-name`
//! with remote paths.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx285_file_name_handler_alist_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'file-name-handler-alist)
      (consp file-name-handler-alist)
      (> (length file-name-handler-alist) 0))
"##,
        expect,
    )
}

#[test]
fn div_cx285_file_remote_p_local_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (file-remote-p "/local/path/file.txt")
      (file-remote-p "/home/user/doc.txt")
      (file-remote-p "/tmp/test")
      (file-remote-p "./relative.txt"))
"##,
        expect,
    )
}

#[test]
fn div_cx285_file_remote_p_remote_methods() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"ssh\" \"host\" \"user\" \"/remote/file\" \"sudo\" \"localhost\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (file-remote-p "/ssh:host:" 'method)
      (file-remote-p "/ssh:host:" 'host)
      (file-remote-p "/ssh:user@host:/path" 'user)
      (file-remote-p "/scp:host:/remote/file" 'localname)
      (file-remote-p "/sudo::/etc/passwd" 'method)
      (file-remote-p "/ssh:localhost:" 'host))
"##,
        expect,
    )
}

#[test]
fn div_cx285_expand_file_name_remote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"/ssh:host:/home/user/file.txt\" \"[ORACLE-HOME]/file.txt\" \"/home/user/file.txt\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (expand-file-name "file.txt" "/ssh:host:/home/user")
      (expand-file-name "~/file.txt")
      (expand-file-name "../file.txt" "/home/user/proj/"))
"##,
        expect,
    )
}

#[test]
fn div_cx285_file_local_name_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"/local/path/file.txt\" \"/remote/path/file.txt\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (file-local-name "/local/path/file.txt")
      (file-local-name "/ssh:host:/remote/path/file.txt"))
"##,
        expect,
    )
}

#[test]
fn div_cx285_directory_file_name_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"/home/user\" \"/home/user/\" \"/tmp\" \"/tmp/\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (directory-file-name "/home/user/")
      (file-name-as-directory "/home/user")
      (directory-file-name "/tmp")
      (file-name-as-directory "/tmp/"))
"##,
        expect,
    )
}

#[test]
fn div_cx285_file_name_decomposition_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"/home/user/file.txt\" \"/home/user/\" \"file.txt\" \"file\" \"txt\" \"file\") (\"/path/to/archive.tar.gz\" \"/path/to/\" \"archive.tar.gz\" \"archive.tar\" \"gz\" \"archive.tar\") (\"simple\" nil \"simple\" \"simple\" nil \"simple\") (\"/dir/noext\" \"/dir/\" \"noext\" \"noext\" nil \"noext\") (\"/trailing/\" \"/trailing/\" \"\" \"\" nil \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (path)
          (list path
                (file-name-directory path)
                (file-name-nondirectory path)
                (file-name-base path)
                (file-name-extension path)
                (file-name-sans-extension (file-name-nondirectory path))))
        '("/home/user/file.txt"
          "/path/to/archive.tar.gz"
          "simple"
          "/dir/noext"
          "/trailing/"))
"##,
        expect,
    )
}

#[test]
fn div_cx285_file_relative_name_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"foo/bar\" \"../user/foo\" \"c\" \"..\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (file-relative-name "/home/user/foo/bar" "/home/user")
      (file-relative-name "/home/user/foo" "/home/other")
      (file-relative-name "/a/b/c" "/a/b")
      (file-relative-name "/a/b" "/a/b/c"))
"##,
        expect,
    )
}

#[test]
fn div_cx285_split_join_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"\" \"home\" \"user\" \"path\" \"file.txt\") \"home/user/file.txt\" (\"\" \"home\" \"user\" \"file.txt\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (split-string "/home/user/path/file.txt" "/")
      (mapconcat #'identity '("home" "user" "file.txt") "/")
      (file-name-split "/home/user/file.txt"))
"##,
        expect,
    )
}

#[test]
fn div_cx285_file_name_ops_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((paths '("/home/user/alpha.txt" "/var/log/beta.log" "/tmp/gamma")))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (mapconcat #'identity paths "\n"))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 20)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 30)
      (let ((state (list (mapcar #'file-name-nondirectory paths)
                         (mapcar #'file-name-extension paths)
                         (mapcar #'expand-file-name paths)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
