/// Batch 502: set-buffer-multibyte characterization — various raw byte patterns.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx502_multibyte_raw_trailing_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\\310ABC\" 4 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200))
  (insert "ABC")
  (set-buffer-multibyte t)
  (list (buffer-string) (length (buffer-string)) (point-max)))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_raw_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\\310\\311\\312\" 3 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 202))
  (set-buffer-multibyte t)
  (list (buffer-string) (length (buffer-string)) (point-max)))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_ascii_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"ABCDE\" 5 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert "ABCDE")
  (set-buffer-multibyte t)
  (list (buffer-string) (length (buffer-string)) (point-max)))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" 0 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (set-buffer-multibyte t)
  (list (buffer-string) (length (buffer-string)) (point-max)))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_interleaved() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\\310A\\311B\\312C\" 6 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 65 201 66 202 67))
  (set-buffer-multibyte t)
  (list (buffer-string) (length (buffer-string)) (point-max)))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_larger_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\\310\\311\\312\\313\\314ABCDE\" 10 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 202 203 204 65 66 67 68 69))
  (set-buffer-multibyte t)
  (list (buffer-string) (length (buffer-string)) (point-max)))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_marker_preserved() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 #<killed buffer>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 65 66))
  (let ((m (set-marker (make-marker) 2)))
    (set-buffer-multibyte t)
    (list (marker-position m) (marker-buffer m))))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_insert_after_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\310\\311EXTRA\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201))
  (set-buffer-multibyte t)
  (insert "EXTRA")
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_narrow_then_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Changing multibyteness in a narrowed buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65 66 67))
  (narrow-to-region 2 4)
  (set-buffer-multibyte t)
  (list (buffer-string) (point-min) (point-max)))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_overlay_then_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable ov)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65 66))
  (let ((ov (make-overlay 1 3)))
    (overlay-put ov 'face 'bold))
  (set-buffer-multibyte t)
  (list (overlay-start ov) (overlay-end ov) (overlay-live-p ov)))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_property_then_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65 66))
  (put-text-property 1 4 'face 'bold)
  (set-buffer-multibyte t)
  (get-text-property 1 'face))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_delete_then_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\311AB\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65 66))
  (delete-region 1 2)
  (set-buffer-multibyte t)
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_region_then_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\310\\311ABC\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65 66 67))
  (set-buffer-multibyte t)
  (set-buffer-multibyte nil)
  (set-buffer-multibyte t)
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_save_excursion_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\310\\311AB\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65 66))
  (save-excursion
    (set-buffer-multibyte t))
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn div_cx502_multibyte_two_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"�A\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((a (get-buffer-create " *cx502-a*"))
      (b (get-buffer-create " *cx502-b*")))
  (with-current-buffer a
    (set-buffer-multibyte nil)
    (insert (unibyte-string 200 65)))
  (with-current-buffer b
    (set-buffer-multibyte nil)
    (insert (unibyte-string 201 66)))
  (set-buffer-multibyte t)
  (with-current-buffer a (buffer-string)))"##,
        expect,
    );
}
