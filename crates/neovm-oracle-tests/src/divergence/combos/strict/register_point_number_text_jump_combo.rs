//! Strict combo oracle probes, batch 180: registers. point-to-register (marker
//! storage), number registers, string registers, copy-to-register (text
//! extraction), set-register update, jump-to-register point restoration, and
//! marker position retrieval (avoids printing raw marker objects).
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_register_point_number_string_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "hello world")
  (point-to-register ?a)
  (set-register ?n 42)
  (set-register ?s "saved text")
  (copy-to-register ?r 1 6)
  (let ((m (get-register ?a)))
    (list (marker-position m)
          (markerp m)
          (get-register ?n)
          (get-register ?s)
          (get-register ?r)
          (progn (set-register ?n 99) (get-register ?n))
          (progn (jump-to-register ?a) (point))
          (progn (set-register ?s "changed") (get-register ?s)))))
"##;
    let expect =
        expect_test::expect![[r#""OK (12 t 42 \"saved text\" \"hello\" 99 12 \"changed\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_register_increment_window_config_rect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "0123456789ABCDEFGHIJ")
  (set-register ?r 5)
  (set-register ?x "extra")
  (list (register-same-line ?r)
        (get-register ?r)
        (progn (increment-register 3 ?r) (get-register ?r))
        (progn (increment-register -2 ?r) (get-register ?r))
        (get-register ?x)
        (copy-to-register ?t 3 7)
        (get-register ?t)
        (length (get-register ?t))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function register-same-line)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_register_kbd_insert_register_named() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (set-register ?i "inserted text")
  (insert-register ?i)
  (list (buffer-string)
        (point)
        (get-register ?i)
        (set-register (make-char 'greek-iso8859-7 97) "greek-key")
        (get-register (make-char 'greek-iso8859-7 97))))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"inserted text\" 1 \"inserted text\" \"greek-key\" \"greek-key\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
