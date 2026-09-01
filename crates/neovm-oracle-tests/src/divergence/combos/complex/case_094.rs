//! Complex combo batch 94 — file watchers & directory recursion & find /
//! locate-dominating-file, file-name-as-directory, expand-file-name with
//! complex relative paths, file-truename with symlinks.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx94_file_truename_simple_no_symlinks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"simple.txt\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-cx94-tn" t))
       (f (expand-file-name "simple.txt" dir)))
  (unwind-protect
      (progn
        (with-temp-buffer
          (insert "content")
          (write-region (point-min) (point-max) f nil 'silent))
        (let ((true (file-truename f)))
          (list (file-name-nondirectory true)
                (string= true f)
                (file-name-absolute-p true))))
    (when (file-exists-p dir) (delete-directory dir t))))
"##,
        expect,
    );
}

#[test]
fn div_cx94_file_symlink_resolution_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"real.txt\" \"real.txt\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((dir (make-temp-file "neo-cx94-sym" t))
           (real (expand-file-name "real.txt" dir))
           (link (expand-file-name "link.txt" dir)))
      (unwind-protect
          (progn
            (with-temp-buffer
              (insert "real")
              (write-region (point-min) (point-max) real nil 'silent))
            (let ((default-directory (file-name-as-directory dir)))
              (make-symbolic-link "real.txt" "link.txt"))
            (let ((true-of-link (file-truename link))
                  (true-of-real (file-truename real)))
              (list (file-name-nondirectory true-of-link)
                    (file-name-nondirectory true-of-real)
                    (file-name-absolute-p true-of-link)
                    (string= true-of-link true-of-real))))
        (when (file-exists-p dir) (delete-directory dir t))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx94_directory_files_recursively_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4 (\"a1.txt\" \"a2.txt\" \"b1.txt\" \"g1.txt\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((root (make-temp-file "neo-cx94-rec" t))
       (sub-a (expand-file-name "alpha" root))
       (sub-b (expand-file-name "beta" root))
       (sub-sub (expand-file-name "gamma" sub-a)))
  (make-directory sub-a t)
  (make-directory sub-b t)
  (make-directory sub-sub t)
  (dolist (f '("a1.txt" "a2.txt"))
    (write-region "x" nil (expand-file-name f sub-a) nil 'silent))
  (write-region "x" nil (expand-file-name "g1.txt" sub-sub) nil 'silent)
  (write-region "x" nil (expand-file-name "b1.txt" sub-b) nil 'silent)
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
fn div_cx94_locate_dominating_file_finds_in_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((root (make-temp-file "neo-cx94-dom" t))
       (marker (expand-file-name "MARKER" root))
       (sub (expand-file-name "sub" root))
       (subsub (expand-file-name "deep" sub)))
  (write-region "x" nil marker nil 'silent)
  (make-directory subsub t)
  (let ((located (locate-dominating-file subsub "MARKER")))
    (prog1 (list (and located (file-equal-p located root))
                 (and located (file-name-absolute-p located))
                 (and located (string-suffix-p "/" located))
                 (and located (file-exists-p (expand-file-name "MARKER" located))))
      (delete-directory root t))))
"##,
        expect,
    );
}

#[test]
fn div_cx94_file_name_concat_chains() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"/home/user/doc/file.txt\" \"/home/user\" \"rel/path/to/file\" \"/trailing/path\" \"/trailing-slash/\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (file-name-concat "/home" "user" "doc" "file.txt")
      (file-name-concat "/home" "user")
      (file-name-concat "rel" "path" "to" "file")
      (file-name-concat "/trailing/" "path")
      (file-name-concat "/trailing-slash/" ""))
"##,
        expect,
    );
}

#[test]
fn div_cx94_expand_file_name_with_dots_in_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"/home/user/foo\" \"/home/foo\" \"/foo\" \"/home/foo\" \"/home/user/foo/baz\" \"/home/user/foo/.bar\" \"/home/user/foo/bar./baz\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((default-directory "/home/user/"))
  (list
   (expand-file-name "./foo")
   (expand-file-name "../foo")
   (expand-file-name "../../foo")
   (expand-file-name "./../foo")
   (expand-file-name "./foo/./bar/../baz")
   (expand-file-name "foo/.bar")
   (expand-file-name "foo/bar./baz")))
"##,
        expect,
    );
}

#[test]
fn div_cx94_directory_files_match_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"alpha.txt\" \"beta.txt\") (\"alpha.txt\" \"beta.txt\" \"delta.log\" \"gamma.dat\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-cx94-pred" t))
       (fns '("alpha.txt" "beta.txt" "gamma.dat" "delta.log")))
  (dolist (f fns)
    (write-region "x" nil (expand-file-name f dir) nil 'silent))
  (let ((txt (sort (directory-files dir t "\\.txt$") #'string<))
        (all (sort (directory-files dir nil "^[^.].+$") #'string<)))
    (delete-directory dir t)
    (list (mapcar #'file-name-nondirectory txt)
          all)))
"##,
        expect,
    );
}

#[test]
fn div_cx94_file_attributes_with_symlink() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"real.dat\" \"real.dat\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((dir (make-temp-file "neo-cx94-attr" t))
           (real (expand-file-name "real.dat" dir))
           (link (expand-file-name "link.dat" dir)))
      (unwind-protect
          (progn
            (write-region "real-content" nil real nil 'silent)
            (let ((default-directory (file-name-as-directory dir)))
              (make-symbolic-link "real.dat" "link.dat"))
            (let ((attr-real (file-attributes real))
                  (attr-link (file-attributes link))
                  (attr-link-no-follow (file-attributes link t)))
              (list (file-attribute-type attr-real)
                    (file-attribute-type attr-link)
                    (file-attribute-type attr-link-no-follow))))
        (when (file-exists-p dir) (delete-directory dir t))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx94_set_file_times_and_check_file_newer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-cx94-times" t))
       (f1 (expand-file-name "f1" dir))
       (f2 (expand-file-name "f2" dir)))
  (write-region "1" nil f1 nil 'silent)
  (write-region "2" nil f2 nil 'silent)
  (set-file-times f1 '(100 0 0 0))
  (set-file-times f2 '(200 0 0 0))
  (let ((newer (file-newer-than-file-p f2 f1))
        (older (file-newer-than-file-p f1 f2)))
    (delete-directory dir t)
    (list newer older)))
"##,
        expect,
    );
}

#[test]
fn div_cx94_copy_file_recursive_directory() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((src (make-temp-file "neo-cx94-cps" t))
       (dst (make-temp-file "neo-cx94-cpd" t))
       (f (expand-file-name "inner.txt" src)))
  (delete-directory dst t)
  (write-region "content" nil f nil 'silent)
  (copy-directory src dst)
  (let ((copied-f (expand-file-name (file-name-nondirectory src)
                                     (expand-file-name "inner.txt" dst))))
    (let ((dst-listing (directory-files dst t "^[^.].+$")))
      (delete-directory src t)
      (delete-directory dst t)
      (list (> (length dst-listing) 0)
            (file-exists-p (expand-file-name (file-name-nondirectory src) dst))))))
"##,
        expect,
    );
}

#[test]
fn div_cx94_directory_empty_p_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((empty (make-temp-file "neo-cx94-empty" t))
       (nonempty (make-temp-file "neo-cx94-non" t))
       (f (expand-file-name "file.txt" nonempty)))
  (write-region "x" nil f nil 'silent)
  (let ((e1 (directory-empty-p empty))
        (e2 (directory-empty-p nonempty)))
    (delete-directory empty t)
    (delete-directory nonempty t)
    (list e1 e2)))
"##,
        expect,
    );
}

#[test]
fn div_cx94_directory_watcher_with_marker_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Changes to be undone are outside visible portion of buffer\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-cx94-mega" t))
       (f1 (expand-file-name "alpha.txt" dir))
       (f2 (expand-file-name "beta.txt" dir)))
  (write-region "alpha" nil f1 nil 'silent)
  (write-region "beta" nil f2 nil 'silent)
  (let ((initial (sort (directory-files dir nil "^[^.]") #'string<)))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert (mapconcat #'identity initial "\n"))
      (put-text-property 1 5 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 3 12)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 1 15)
        (let ((state (list (buffer-string)
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
