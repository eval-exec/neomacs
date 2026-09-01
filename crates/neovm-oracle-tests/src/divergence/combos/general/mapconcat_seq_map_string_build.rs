//! Deep combo: mapconcat + seq-map + string building + format composition.
//! Tests string building patterns with mapping and joining operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_mapconcat_basic_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"a,b,c\" \"1-2-3-4-5\" \"[hello] [world]\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (mapconcat 'identity '(\"a\" \"b\" \"c\") \",\")\n\
         (mapconcat 'number-to-string '(1 2 3 4 5) \"-\")\n\
         (mapconcat (lambda (x) (format \"[%s]\" x))\n\
         '(\"hello\" \"world\") \" \")))",
        expect,
    );
}

#[test]
fn deficiency_string_join_with_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"name=Alice&age=30&city=NYC\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((items '((name . \"Alice\") (age . \"30\") (city . \"NYC\"))))\n\
         (mapconcat (lambda (pair)\n\
         (format \"%s=%s\" (car pair) (cdr pair)))\n\
         items \"&\")))",
        expect,
    );
}

#[test]
fn deficiency_mapconcat_with_index_via_number_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-pairlis)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((items '(\"alpha\" \"beta\" \"gamma\" \"delta\")))\n\
         (mapconcat (lambda (pair)\n\
         (format \"%d. %s\" (car pair) (cdr pair)))\n\
         (cl-pairlis (number-sequence 1 4) items)\n\
         \"\\n\")))",
        expect,
    );
}

#[test]
fn deficiency_seq_map_into_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"0102030405\" \"1 10 11 100 101\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((nums '(1 2 3 4 5)))\n\
         (list (mapconcat (lambda (n) (format \"%02x\" n)) nums \"\")\n\
         (mapconcat (lambda (n) (format \"%b\" n)) nums \" \"))))",
        expect,
    );
}

#[test]
fn deficiency_build_csv_from_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"Alice,30,NYC\\nBob,25,LA\\nCarol,35,SF\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((rows '((\"Alice\" 30 \"NYC\")\n\
         (\"Bob\" 25 \"LA\")\n\
         (\"Carol\" 35 \"SF\"))))\n\
         (mapconcat (lambda (row)\n\
         (mapconcat (lambda (cell)\n\
         (if (stringp cell) cell (number-to-string cell)))\n\
         row \",\"))\n\
         rows \"\\n\")))",
        expect,
    );
}

#[test]
fn deficiency_mapconcat_empty_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"\" \"single\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (mapconcat 'identity nil \",\")\n\
         (mapconcat 'identity '(\"single\") \",\")))",
        expect,
    );
}

#[test]
fn deficiency_with_output_to_string_build() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"Header\\nLine 1\\nLine 2\\nLine 3\\nLine 4\\nLine 5\\nFooter\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (with-output-to-string\n\
         (princ \"Header\\n\")\n\
         (dotimes (i 5)\n\
         (princ (format \"Line %d\\n\" (1+ i))))\n\
         (princ \"Footer\")))",
        expect,
    );
}

#[test]
fn deficiency_string_build_with_propertize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"bold and italic\" 0 4 (face bold) 9 15 (face italic)) bold italic 15)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((parts (list (propertize \"bold\" 'face 'bold)\n\
         \" and \"\n\
         (propertize \"italic\" 'face 'italic))))\n\
         (let ((combined (apply 'concat parts)))\n\
         (list combined\n\
         (get-text-property 0 'face combined)\n\
         (get-text-property 9 'face combined)\n\
         (length combined)))))",
        expect,
    );
}

#[test]
fn deficiency_format_table_with_padding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK \"Alice       95\\nBob         87\\nCharlie     92\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((data '((\"Alice\" 95) (\"Bob\" 87) (\"Charlie\" 92))))\n\
         (mapconcat (lambda (row)\n\
         (format \"%-10s %3d\"\n\
         (nth 0 row) (nth 1 row)))\n\
         data \"\\n\")))",
        expect,
    );
}

#[test]
fn deficiency_build_html_like_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"<ul>\\n  <li>Apple</li>\\n  <li>Banana</li>\\n  <li>Cherry</li>\\n</ul>\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((items '(\"Apple\" \"Banana\" \"Cherry\")))\n\
         (concat \"<ul>\\n\"\n\
         (mapconcat (lambda (item)\n\
         (format \"  <li>%s</li>\" item))\n\
         items \"\\n\")\n\
         \"\\n</ul>\")))",
        expect,
    );
}
