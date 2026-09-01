//! file-attributes field-format coverage (currently faithful).
//!
//! file-attributes returns an 11/12-element list; these probes check the field
//! types and values against GNU on a real temp file: link-type, nlinks,
//! uid/gid (string vs number), the three timestamps, size, mode-string,
//! inode/device. Plus file-attribute-modes and the file-* predicates.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

fn _u() {}

#[test]
fn div_fa_attributes_full_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    _u();
    let expect = expect_test::expect![[
        r#""OK (:nil :num :num :num :cons :cons :cons :num :str :other :num :num)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-fa-")))
  (write-region "hello" nil f nil 0)
  (unwind-protect
      (mapcar (lambda (e) (cond ((null e) :nil) ((stringp e) :str)
                                ((numberp e) :num) ((consp e) :cons) (t :other)))
              (file-attributes f))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_fa_uid_gid_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-fau-")))
  (unwind-protect
      (let ((a (file-attributes f)))
        (list (stringp (nth 2 a)) (stringp (nth 3 a))
              (numberp (nth 2 a)) (numberp (nth 3 a))))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_fa_mode_string_and_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 \"-rw-------\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-fam-")))
  (write-region "hello" nil f nil 0)
  (unwind-protect
      (let ((a (file-attributes f)))
        (list (nth 7 a) (nth 8 a)))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_fa_mod_time_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 4 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-fat-")))
  (write-region "x" nil f nil 0)
  (unwind-protect
      (list (length (nth 4 (file-attributes f)))
            (length (nth 5 (file-attributes f)))
            (length (nth 6 (file-attributes f))))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_fa_attribute_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"-rw-------\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-faam-")))
  (write-region "x" nil f nil 0)
  (unwind-protect (file-attribute-modes (file-attributes f))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_fa_directory_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"drwx------\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((d (make-temp-file "neo-fad-" t)))
  (unwind-protect
      (list (car (file-attributes d))
            (file-attribute-modes (file-attributes d)))
    (ignore-errors (delete-directory d))))
"##,
        expect,
    );
}

#[test]
fn div_fa_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-fap-")))
  (unwind-protect
      (list (file-regular-p f) (file-directory-p f)
            (file-symlink-p f) (file-exists-p f))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_fa_file_modes_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (384 493)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-famr-")))
  (unwind-protect
      (let ((m (file-modes f)))
        (set-file-modes f #o755)
        (list m (file-modes f)))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}
