//! Complex combo batch 70 — format / print / read interplay: %S circular,
//! %s vs %S vs %P, read of malformed data, format-spec multi-key,
//! `with-output-to-temp-buffer`, and `pp` indentation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx70_format_specifiers_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Format string ends in middle of format specifier\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (format "%d" 42)
 (format "%i" 42)
 (format "%5d" 42)
 (format "%-5d|" 42)
 (format "%05d" 42)
 (format "%+d" 42)
 (format "%o" 64)
 (format "%x" 255)
 (format "%X" 255)
 (format "%b" 10)
 (format "%c" 65)
 (format "%e" 12345.6789)
 (format "%f" 12345.6789)
 (format "%g" 0.00001)
 (format "%.2f" 3.14159)
 (format "%10.3f" 3.14159)
 (format "%s" "hello")
 (format "%10s|" "hi")
 (format "%-10s|" "hi")
 (format "%S" '(1 "two" 3))
 (format "%%")
 (format "%3$" 1 2 3))
"##,
        expect,
    );
}

#[test]
fn div_cx70_format_positional_arguments_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Not enough arguments for format string\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (format "%2$s %1$s" "world" "hello")
 (format "%1$d + %2$d = %3$d" 2 3 5)
 (format "%s = %2$d (or %d)" "x" 99)
 (format "%3$-10s|" "a" "b" "c"))
"##,
        expect,
    );
}

#[test]
fn div_cx70_print_S_with_various_data_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"42\" \"\\\"hello\\\"\" \"(1 2 3)\" \"[1 2 3]\" \"symbol\" \"65\" \"3.14\" \"#s(hash-table test equal)\" #(\"XXX\" 1 2 (face bold)) #^[nil nil syntax-table #^^[3 0 nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil :word nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] #^^[1 0 #^^[2 0 #^^[3 0 nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil :word nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil])""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (prin1-to-string 42)
 (prin1-to-string "hello")
 (prin1-to-string '(1 2 3))
 (prin1-to-string [1 2 3])
 (prin1-to-string 'symbol)
 (prin1-to-string ?A)
 (prin1-to-string 3.14)
 (prin1-to-string (make-hash-table :test 'equal))
 (let ((s (make-string 3 ?X)))
   (put-text-property 1 2 'face 'bold s)
   s)
 (let ((tbl (make-char-table 'syntax-table nil)))
   (aset tbl ?a :word)
   tbl))
"##,
        expect,
    );
}

#[test]
fn div_cx70_prin1_vs_princ_no_escape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function princ-to-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((str "hello \"world\"")
      (lst '(a "b" c)))
  (list
   (prin1-to-string str)
   (princ-to-string str)
   (prin1-to-string lst)
   (princ-to-string lst)))
"##,
        expect,
    );
}

#[test]
fn div_cx70_print_circle_shared_and_circular() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((shared (list 1 2 3)))
  (setcdr (cddr shared) (cdr shared))
  (list
   (let ((print-circle t)) (prin1-to-string shared))
   (let ((print-gensym t)) (prin1-to-string (gensym)))
   (let ((print-circle t) (print-length 3)) (prin1-to-string '(1 2 3 4 5 6 7 8 9)))
   (let ((print-level 2)) (prin1-to-string '((((deep))))))
"##,
        expect,
    );
}

#[test]
fn div_cx70_read_from_string_position_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((1 2 3) 7) (symbol 6) (\"string\" 8) (123 3) (nil 3) ([1 2 3] 7) (#s(hash-table test eq data (a 1 b 2)) 45) ((1 2 3) 7) ((1 2 . 3) 9) (:err . end-of-file) (:err . invalid-read-syntax))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (condition-case e
              (let ((res (read-from-string s)))
                (list (car res) (cdr res)))
            (error (cons :err (car e)))))
        '("(1 2 3)"
          "symbol"
          "\"string\""
          "123"
          "nil"
          "[1 2 3]"
          "#s(hash-table size 10 test eq data (a 1 b 2))"
          "(1 2 3) trailing"
          "(1 2 . 3)"
          "(incomplete"
          "."))
"##,
        expect,
    );
}

#[test]
fn div_cx70_read_from_string_with_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (café 19990 (\"a α b\") [α β γ] hello世界)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (condition-case e
              (car (read-from-string s))
            (error (cons :err (car e)))))
        '("café"
          "?世"
          "(\"a α b\")"
          "[α β γ]"
          "hello世界"))
"##,
        expect,
    );
}

#[test]
fn div_cx70_with_output_to_temp_buffer_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"line 1\\nline 2\\n(1 2 3)\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((temp-buffer-name "*My Temp Output*"))
  (with-output-to-temp-buffer temp-buffer-name
    (princ "line 1\n")
    (princ "line 2\n")
    (princ '(1 2 3)))
  (let ((content (with-current-buffer temp-buffer-name
                   (prog1 (buffer-string)
                     (kill-buffer)))))
    content))
"##,
        expect,
    );
}

#[test]
fn div_cx70_pp_indentation_of_nested_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"(:config (:option-a \\\"value\\\" :option-b (:nested-a 1 :nested-b 2))\\n\t :option-c (1 2 3))\\n\" 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((nested '(:config (:option-a "value"
                       :option-b (:nested-a 1
                                   :nested-b 2))
                       :option-c (1 2 3))))
  (list (pp-to-string nested)
        (length (split-string (pp-to-string nested) "\n"))))
"##,
        expect,
    );
}

#[test]
fn div_cx70_format_message_vs_message_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"plain text\" \"with ‘quotes’ here\" \"value: 42\" \"val1 ‘a’ val2 ‘b’\" \"with `quotes' here\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (format-message "plain text")
 (format-message "with `quotes' here")
 (format-message "value: %d" 42)
 (format-message "val1 `%s' val2 `%s'" "a" "b")
 (format "with `quotes' here"))
"##,
        expect,
    );
}

#[test]
fn div_cx70_number_to_string_with_base_via_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"42\" \"-42\" \"3.14\" \"ff\" \"0xff\" \"100\" \"0100\" \"1010\" \"-1\" \"000ff\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (number-to-string 42)
 (number-to-string -42)
 (number-to-string 3.14)
 (format "%x" 255)
 (format "%#x" 255)
 (format "%o" 64)
 (format "%#o" 64)
 (format "%b" 10)
 (format "%x" -1)
 (format "%05x" 255))
"##,
        expect,
    );
}

#[test]
fn div_cx70_print_with_special_floats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"1.0e+INF\" \"-1.0e+INF\" \"-0.0e+NaN\" \"inf\" \"inf\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (condition-case e (prin1-to-string (/ 1.0 0.0)) (error (cons :err (car e))))
 (condition-case e (prin1-to-string (/ -1.0 0.0)) (error (cons :err (car e))))
 (condition-case e (prin1-to-string (/ 0.0 0.0)) (error (cons :err (car e))))
 (condition-case e (format "%f" (/ 1.0 0.0)) (error (cons :err (car e))))
 (condition-case e (format "%d" (/ 1.0 0.0)) (error (cons :err (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx70_print_read_roundtrip_with_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"#(\\\"hello world\\\" 0 11 (face bold))\" #(\"hello world\" 0 11 (face bold)) bold bold t)""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (propertize "hello world" 'face 'bold))
       (printed (prin1-to-string s))
       (read-back (car (read-from-string printed))))
  (list printed
        read-back
        (get-text-property 1 'face read-back)
        (get-text-property 6 'face read-back)
        (equal s read-back)))
"##,
        expect,
    );
}
