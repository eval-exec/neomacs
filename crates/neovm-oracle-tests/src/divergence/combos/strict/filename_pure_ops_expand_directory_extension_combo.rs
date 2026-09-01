//! Strict combo oracle probes, batch 154: pure file-name string operations
//! (no filesystem). file-name-directory/nondirectory/sans-extension/extension
//! incl dotfiles and multi-dot, directory-file-name vs file-name-as-directory
//! round-trips, expand-file-name with default-directory + relative + .. + ~,
//! file-name-absolute-p / quoted-p, and convert-standard-filename.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_filename_decompose_extension_dotfiles() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (file-name-directory "/a/b/c.txt")
      (file-name-nondirectory "/a/b/c.txt")
      (file-name-sans-extension "/a/b/c.tar.gz")
      (file-name-extension "/a/b/c.tar.gz")
      (file-name-sans-extension "/a/b/c.txt")
      (file-name-extension "/a/b/.bashrc")
      (file-name-sans-extension "/a/b/.bashrc")
      (file-name-extension "/a/b/Makefile")
      (file-name-extension "/a/b/")
      (file-name-extension "/a/b/tar.gz")
      (file-name-nondirectory "/a/b/")
      (file-name-nondirectory ""))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"/a/b/\" \"c.txt\" \"/a/b/c.tar\" \"gz\" \"/a/b/c\" nil \"/a/b/.bashrc\" nil nil \"gz\" \"\" \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_directory_filename_as_directory_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (directory-file-name "/a/b/c/")
      (directory-file-name "/a/b/c")
      (directory-file-name "/")
      (file-name-as-directory "/a/b/c")
      (file-name-as-directory "/")
      (directory-file-name (file-name-as-directory "/x/y"))
      (file-name-as-directory (directory-file-name "/x/y/")))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"/a/b/c\" \"/a/b/c\" \"/\" \"/a/b/c/\" \"/\" \"/x/y\" \"/x/y/\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_expand_filename_relative_parent_abs_home() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((default-directory "/home/user/projects/app/")
      (home (concat (getenv "HOME") "/")))
  (list (expand-file-name "d/e/../f")
        (expand-file-name "../x" "/a/b/c")
        (expand-file-name "../../y" "/a/b/c/d")
        (expand-file-name "/abs/path")
        (expand-file-name "./local")
        (expand-file-name "sub/./deep/../final")
        (file-name-absolute-p "/abs")
        (file-name-absolute-p "rel")
        (file-name-absolute-p "~/home")
        (file-name-absolute-p "/ssh:host:/path")
        (file-name-quoted-p "/:/escaped/path")
        (file-name-quoted-p "/normal/path")
        (convert-standard-filename "/a/b/c.txt")))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"/home/user/projects/app/d/f\" \"/a/b/x\" \"/a/b/y\" \"/abs/path\" \"/home/user/projects/app/local\" \"/home/user/projects/app/sub/final\" t nil t t t nil \"/a/b/c.txt\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
