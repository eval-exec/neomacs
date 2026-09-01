//! Complex combo batch 68 — file IO with temp files (read/write/append), file
//! attribute access, file-name ops, directory listing, coding systems on
//! file read/write, file-local-variables, and file locks.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx68_write_read_roundtrip_text_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (24 t \"Hello café 世界 😀\\n\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((path (make-temp-file "neo-cx68-utf8"))
       (data "Hello café 世界 😀\n"))
  (with-temp-buffer
    (insert data)
    (write-region (point-min) (point-max) path nil 'silent))
  (let* ((attrs (file-attributes path))
         (size (file-attribute-size attrs))
         (content (with-temp-buffer
                    (insert-file-contents path)
                    (buffer-string))))
    (delete-file path)
    (list size (string= content data) content (file-exists-p path))))
"##,
        expect,
    );
}

#[test]
fn div_cx68_write_then_append_then_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"first part\\nsecond part\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((path (make-temp-file "neo-cx68-app")))
  (delete-file path)
  (with-temp-buffer
    (insert "first part\n")
    (write-region (point-min) (point-max) path nil 'silent))
  (with-temp-buffer
    (insert "second part\n")
    (write-region (point-min) (point-max) path 'append 'silent))
  (let ((content (with-temp-buffer
                   (insert-file-contents path)
                   (buffer-string))))
    (delete-file path)
    content))
"##,
        expect,
    );
}

#[test]
fn div_cx68_file_name_operations_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"file.txt\" \"/path/to/\" \"file\" \"/path/to/file.txt\" \"gz\" \"file.tar\" \"file.txt\" \"foo/bar\" \"/home/user/foo/bar\" \"/home/user\" \"/home/user/\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (file-name-nondirectory "/path/to/file.txt")
 (file-name-directory "/path/to/file.txt")
 (file-name-base "/path/to/file.txt")
 (file-name-sans-versions "/path/to/file.txt~")
 (file-name-extension "file.tar.gz")
 (file-name-sans-extension "file.tar.gz")
 (file-name-with-extension "file" "txt")
 (file-relative-name "/home/user/foo/bar" "/home/user")
 (expand-file-name "foo/bar" "/home/user")
 (directory-file-name "/home/user/")
 (file-name-as-directory "/home/user"))
"##,
        expect,
    );
}

#[test]
fn div_cx68_directory_files_with_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"alpha.txt\" \"beta.dat\") (\"alpha.txt\" \"beta.dat\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-cx68-dir" t))
       (f1 (expand-file-name "alpha.txt" dir))
       (f2 (expand-file-name "beta.dat" dir)))
  (with-temp-buffer
    (insert "alpha content")
    (write-region (point-min) (point-max) f1 nil 'silent))
  (with-temp-buffer
    (insert "beta content")
    (write-region (point-min) (point-max) f2 nil 'silent))
  (let ((names (sort (directory-files dir nil "[a-z]+\\.[a-z]+$") #'string<))
        (full (sort (mapcar (lambda (n) (file-name-nondirectory n))
                            (directory-files dir t "[a-z]+\\.[a-z]+$")) #'string<)))
    (delete-directory dir t)
    (list names full (file-exists-p dir))))
"##,
        expect,
    );
}

#[test]
fn div_cx68_file_attributes_full_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function file-attribute-file-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((path (make-temp-file "neo-cx68-attrs")))
  (with-temp-buffer
    (insert "0123456789")
    (write-region (point-min) (point-max) path nil 'silent))
  (let* ((a (file-attributes path)))
    (delete-file path)
    (list (file-attribute-size a)
          (file-attribute-modes a)
          (file-attribute-type a)
          (file-attribute-link-number a)
          (car (file-attribute-modification-time a))
          (car (file-attribute-access-time a))
          (file-attribute-file-system a))))
"##,
        expect,
    );
}

#[test]
fn div_cx68_file_coding_roundtrip_latin1_unibyte_charset_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((path (make-temp-file "neo-cx68-latin1"))
       (data (decode-coding-string (unibyte-string #xe9 #xe0 #xfc) 'latin-1-unix)))
  (unwind-protect
      (progn
        (with-temp-buffer
          (insert data)
          (let ((coding-system-for-write 'latin-1-unix))
            (write-region (point-min) (point-max) path nil 'silent)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (let ((coding-system-for-read 'no-conversion))
            (insert-file-contents path))
          (let ((s (buffer-string)))
            (list (length s) (string-bytes s)
                  (text-properties-at 0 s)))))
    (ignore-errors (delete-file path))))
"##,
        expect,
    );
}

#[test]
fn div_cx68_expand_file_name_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"/home/user/foo\" \"/var/foo\" \"[ORACLE-HOME]/foo\" \"/home/user/foo\" \"/home/user/foo\" \"/absolute/path\" \"/home/user/file with spaces\" \"/home/user\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((default-directory "/home/user/"))
  (list
   (expand-file-name "foo")
   (expand-file-name "foo" "/var/")
   (expand-file-name "~/foo")
   (expand-file-name "../foo" "/home/user/proj/")
   (expand-file-name "./foo" "/home/user/")
   (expand-file-name "/absolute/path")
   (expand-file-name "file with spaces")
   (expand-file-name "" "/home/user/")))
"##,
        expect,
    );
}

#[test]
fn div_cx68_make_temp_file_with_suffix_and_dir() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t \"tmp\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((dir (make-temp-file "neo-cx68-base" t))
           (f (make-temp-file (expand-file-name "inner" dir) nil ".tmp")))
      (let ((created (file-exists-p f))
            (f-base (file-name-nondirectory f))
            (f-ext (file-name-extension f)))
        (delete-directory dir t)
        (list created
              (string-prefix-p "inner" f-base)
              (string-suffix-p ".tmp" f-base)
              f-ext
              (file-exists-p dir))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx68_insert_file_contents_partial_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"56789ABCDE\" 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((path (make-temp-file "neo-cx68-partial")))
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
    );
}

#[test]
fn div_cx68_file_modes_get_set_and_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((path (make-temp-file "neo-cx68-modes")))
  (let ((initial-modes (file-modes path)))
    (set-file-modes path #o755)
    (let ((rwx (file-modes path)))
      (set-file-modes path #o644)
      (let ((rw- (file-modes path)))
        (delete-file path)
        (list initial-modes rwx rw- (file-modes-symbolic-to-number "rwxr-xr-x")))))
"##,
        expect,
    );
}

#[test]
fn div_cx68_temp_file_read_write_buffer_local_undo_marker_overlay_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((path (make-temp-file "neo-cx68-mega"))
       (buf (find-file-noselect path)))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "header line\n")
    (insert "second line\n")
    (insert "third line\n")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 14))
          (ov (make-overlay 8 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 1 25)
      (save-buffer))
  (let ((content (with-temp-buffer
                   (insert-file-contents path)
                   (buffer-string))))
    (with-current-buffer buf
      (set-buffer-modified-p nil)
      (kill-buffer buf))
    (delete-file path)
    (list content
          (length content)
          (file-exists-p path))))
"##,
        expect,
    );
}
