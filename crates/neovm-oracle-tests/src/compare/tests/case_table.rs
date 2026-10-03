//! GNU compare-strings fetches multibyte characters and uses buffer-local
//! character upcase rules. Refresh every expectation from GNU, never by hand.
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_compare_case_table_with_case_table_and_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (let ((table (copy-case-table (standard-case-table))))
    (set-case-syntax-pair ?X ?a table)
    (let ((before (compare-strings "a" nil nil "X" nil nil t)))
      (list before
            (with-case-table table
              (list (compare-strings "a" nil nil "X" nil nil t)
                    (compare-strings "a" nil nil "A" nil nil t)
                    (compare-strings "aq" nil nil "XQ" nil nil t)
                    (compare-strings "aq" nil nil "XQ" nil nil nil)))
            (compare-strings "a" nil nil "X" nil nil t)))))
"#;
    let expect = expect_test::expect![[r#""OK (-1 (t 1 t 1) -1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_case_table_turkish_dotless_and_dotted_i() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (let ((table (copy-case-table (standard-case-table))))
    (set-case-syntax-pair ?I ?ı table)
    (set-case-syntax-pair ?İ ?i table)
    (with-case-table table
      (mapcar (lambda (pair)
                (list pair
                      (compare-strings (car pair) nil nil (cadr pair) nil nil t)
                      (compare-strings (cadr pair) nil nil (car pair) nil nil t)
                      (compare-strings (car pair) nil nil (cadr pair) nil nil)))
              '(("ı" "I") ("i" "İ") ("i" "I") ("ı" "İ")
                ("head-ıx" "HEAD-Ix") ("head-ix" "HEAD-Ix"))))))
"#;
    let expect = expect_test::expect![[
        r#""OK (((\"ı\" \"I\") t t 1) ((\"i\" \"İ\") t t -1) ((\"i\" \"I\") 1 -1 1) ((\"ı\" \"İ\") -1 1 1) ((\"head-ıx\" \"HEAD-Ix\") t t 1) ((\"head-ix\" \"HEAD-Ix\") 6 -6 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_case_table_unibyte_buffer_standard_and_custom_mapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (multibyte '(t nil))
    (with-temp-buffer
      (set-buffer-multibyte multibyte)
      (let ((table (copy-case-table (standard-case-table))))
        (set-case-syntax-pair ?Ā ?a table)
        (push
         (list multibyte
               (list (compare-strings "é" nil nil "É" nil nil t)
                     (compare-strings "à" nil nil "À" nil nil t)
                     (compare-strings "Жq" nil nil "жQ" nil nil t))
               (with-case-table table
                 (list (compare-strings "a" nil nil "Ā" nil nil t)
                       (compare-strings "a" nil nil "A" nil nil t)
                       (compare-strings "pa" nil nil "pĀ" nil nil t))))
         answers))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[r#""OK ((t (t t t) (t 1 t)) (nil (1 1 t) (-1 -1 -2)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_case_table_mixed_unibyte_multibyte_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((u (unibyte-string ?a #xe9 ?q))
      (raw (string-make-multibyte (unibyte-string ?A #xe9 ?Q)))
      (unicode "AÉQ")
      answers)
  (dolist (multibyte '(t nil))
    (with-temp-buffer
      (set-buffer-multibyte multibyte)
      (push (list multibyte (multibyte-string-p u) (multibyte-string-p raw)
                  (compare-strings u nil nil raw nil nil t)
                  (compare-strings raw nil nil u nil nil t)
                  (compare-strings u 1 2 raw 1 2 t)
                  (compare-strings u nil nil unicode nil nil t)
                  (compare-strings unicode nil nil u nil nil t)
                  (compare-strings "aéq" nil nil unicode nil nil t)
                  (compare-strings u nil nil raw nil nil nil)) answers)))
  (nreverse answers))
"#;
    let expect =
        expect_test::expect![[r#""OK ((t nil t t t t 2 -2 t 1) (nil nil t t t t 2 -2 2 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_case_table_far_start_end_ranges_and_position_sign() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (let ((table (copy-case-table (standard-case-table)))
        (s (concat (make-string 64 ?ж) "ıxq" (make-string 64 ?あ))))
    (set-case-syntax-pair ?I ?ı table)
    (with-case-table table
      (let (answers)
        (dotimes (_ 3)
          (push (list (compare-strings s 64 67 "IXQ" 0 nil t)
                      (compare-strings "IXQ" 0 nil s 64 67 t)
                      (compare-strings s -67 -64 "IXQ" 0 99 t)
                      (compare-strings s 64 67 "IXR" 0 nil t)
                      (compare-strings "IXR" 0 nil s 64 67 t)
                      (compare-strings s 64 67 "Ix" 0 nil t)
                      (compare-strings "Ix" 0 nil s 64 67 t)
                      (compare-strings s 65 65 "" 0 0 t)
                      (compare-strings s 64 67 "IXQ" 0 nil)) answers))
        (nreverse answers)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t t t -3 3 3 -3 t 1) (t t t -3 3 3 -3 t 1) (t t t -3 3 3 -3 t 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_case_table_string_wrappers_share_current_buffer_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (multibyte '(t nil))
    (with-temp-buffer
      (set-buffer-multibyte multibyte)
      (let ((table (copy-case-table (standard-case-table))))
        (set-case-syntax-pair ?I ?ı table)
        (set-case-syntax-pair ?İ ?i table)
        (with-case-table table
          (push (list multibyte
                      (string-equal-ignore-case "ıq" "IQ")
                      (string-equal-ignore-case "iq" "IQ")
                      (string-equal-ignore-case "é" "É")
                      (string-prefix-p "ıq" "IQ-rest" t)
                      (string-prefix-p "iq" "IQ-rest" t)
                      (string-prefix-p "ıq" "IQ-rest")
                      (string-suffix-p "ıq" "rest-IQ" t)
                      (string-suffix-p "iq" "rest-IQ" t)
                      (string-suffix-p "ıq" "rest-IQ")
                      (string-prefix-p "é" "Éx" t)
                      (string-suffix-p "é" "xÉ" t)) answers)))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t t nil t t nil nil t nil nil t t) (nil t nil nil t nil nil t nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_case_table_assoc_string_shares_current_buffer_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (multibyte '(t nil))
    (with-temp-buffer
      (set-buffer-multibyte multibyte)
      (let ((table (copy-case-table (standard-case-table))))
        (set-case-syntax-pair ?I ?ı table)
        (set-case-syntax-pair ?İ ?i table)
        (with-case-table table
          (push (list multibyte
                      (assoc-string "ıq" '(("IQ" . first) ("İQ" . second)) t)
                      (assoc-string "iq" '(("IQ" . first) ("İQ" . second)) t)
                      (assoc-string "iq" '(IQ İQ) t)
                      (assoc-string "é" '(("É" . upper) ("é" . lower)) t)
                      (assoc-string "ıq" '(("IQ" . first)) nil)
                      (assoc-string (unibyte-string #xe9)
                                    (list (cons (string-make-multibyte (unibyte-string #xe9))
                                                'raw)) t)) answers)))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t (\"IQ\" . first) (\"İQ\" . second) İQ (\"É\" . upper) nil (\"\\351\" . raw)) (nil (\"IQ\" . first) nil nil (\"é\" . lower) nil (\"\\351\" . raw)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_case_table_completion_callers_turkish_and_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (multibyte '(t nil))
    (dolist (ignore '(nil t))
      (with-temp-buffer
        (set-buffer-multibyte multibyte)
        (let ((table (copy-case-table (standard-case-table)))
              (completion-ignore-case ignore)
              (collection '("Icarus" "Icon" "İcat" "İcone" "Éclair" "École" "école")))
          (set-case-syntax-pair ?I ?ı table)
          (set-case-syntax-pair ?İ ?i table)
          (with-case-table table
            (push (list multibyte ignore
                        (try-completion "ı" collection)
                        (all-completions "ı" collection)
                        (test-completion "ıcon" collection)
                        (try-completion "i" collection)
                        (all-completions "i" collection)
                        (test-completion "icat" collection)
                        (try-completion "é" collection)
                        (all-completions "é" collection)
                        (test-completion "éclair" collection)) answers))))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t nil nil nil nil nil nil nil \"école\" (\"école\") nil) (t t \"Ic\" (\"Icarus\" \"Icon\") t \"İc\" (\"İcat\" \"İcone\") t \"éc\" (\"Éclair\" \"École\" \"école\") t) (nil nil nil nil nil nil nil nil \"école\" (\"école\") nil) (nil t \"Ic\" (\"Icarus\" \"Icon\") t nil nil nil \"école\" (\"école\") nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_case_table_completion_predicate_changes_active_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (answers)
  (dolist (operation '(try-completion all-completions test-completion))
    (with-temp-buffer
      (let ((table (copy-case-table (standard-case-table)))
            (completion-ignore-case t) (seen nil))
        (set-case-syntax-pair ?X ?a table)
        (let ((result
               (funcall operation "a" '("a" "Xray" "ALPS")
                        (lambda (candidate)
                          (push candidate seen)
                          (set-case-table table)
                          (if (eq operation 'test-completion)
                              (not (equal candidate "a")) t)))))
          (push (list operation result (nreverse seen)
                      (compare-strings "a" nil nil "X" nil nil t)) answers)))))
  (nreverse answers))
"#;
    let expect = expect_test::expect![[
        r#""OK ((try-completion \"a\" (\"a\" \"Xray\") t) (all-completions (\"a\" \"Xray\") (\"a\" \"Xray\") t) (test-completion nil (\"a\") t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_case_table_file_name_completion_and_ignored_suffix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((dir (make-temp-file
            (expand-file-name "gp-case-table-" (getenv "NEOVM_ORACLE_TEST_TMPDIR")) t)))
  (unwind-protect
      (progn
        (dolist (name '("Icarus" "Icon" "İcat" "İcone" "Éclair" "École" "école"
                        "keep.A" "keep.x"))
          (with-temp-file (expand-file-name name dir) (insert "x")))
        (let (answers)
          (dolist (multibyte '(t nil))
            (dolist (ignore '(nil t))
              (with-temp-buffer
                (set-buffer-multibyte multibyte)
                (let ((table (copy-case-table (standard-case-table)))
                      (completion-ignore-case ignore))
                  (set-case-syntax-pair ?I ?ı table)
                  (set-case-syntax-pair ?İ ?i table)
                  (set-case-syntax-pair ?X ?a table)
                  (push
                   (list multibyte ignore
                         (list (file-name-completion "é" dir)
                               (sort (file-name-all-completions "é" dir) #'string<))
                         (with-case-table table
                           (list (file-name-completion "ı" dir)
                                 (sort (file-name-all-completions "ı" dir) #'string<)
                                 (file-name-completion "i" dir)
                                 (sort (file-name-all-completions "i" dir) #'string<)
                                 (let ((completion-ignored-extensions '(".a")))
                                   (list (file-name-completion "keep." dir)
                                         (sort (file-name-all-completions "keep." dir)
                                               #'string<))))))
                   answers)))))
          (nreverse answers)))
    (delete-directory dir t)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t nil (\"école\" (\"école\")) (nil nil nil nil (\"keep.\" (\"keep.A\" \"keep.x\")))) (t t (\"éc\" (\"Éclair\" \"École\" \"école\")) (\"Ic\" (\"Icarus\" \"Icon\") \"İc\" (\"İcat\" \"İcone\") (\"keep.A\" (\"keep.A\" \"keep.x\")))) (nil nil (\"école\" (\"école\")) (nil nil nil nil (\"keep.\" (\"keep.A\" \"keep.x\")))) (nil t (\"école\" (\"école\")) (\"Ic\" (\"Icarus\" \"Icon\") nil nil (\"keep.A\" (\"keep.A\" \"keep.x\")))))""#
    ]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(form, expect);
}

#[test]
fn oracle_compare_case_table_file_name_completion_encoded_width_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((dir (make-temp-file
            (expand-file-name "gp-case-width-" (getenv "NEOVM_ORACLE_TEST_TMPDIR")) t)))
  (unwind-protect
      (progn
        (dolist (group '(("short" "I") ("long" "Ir" "Is")
                         ("mixed" "I" "Ir" "Is") ("reverse" "ı")))
          (let ((subdir (expand-file-name (car group) dir)))
            (make-directory subdir)
            (dolist (name (cdr group))
              (with-temp-file (expand-file-name name subdir) (insert "x")))))
        (let (answers)
          (dolist (multibyte '(t nil))
            (with-temp-buffer
              (set-buffer-multibyte multibyte)
              (let ((table (copy-case-table (standard-case-table)))
                    (completion-ignore-case t))
                (set-case-syntax-pair ?I ?ı table)
                (with-case-table table
                  (push
                   (list multibyte
                         (mapcar
                          (lambda (group)
                            (let ((subdir (expand-file-name group dir)))
                              (list group (file-name-completion "ı" subdir)
                                    (sort (file-name-all-completions "ı" subdir)
                                          #'string<))))
                          '("short" "long" "mixed"))
                         (let ((subdir (expand-file-name "reverse" dir)))
                           (list (file-name-completion "I" subdir)
                                 (sort (file-name-all-completions "I" subdir) #'string<)))
                         (let ((subdir (expand-file-name "short" dir)))
                           (list (file-name-completion (unibyte-string ?I) subdir)
                                 (file-name-completion
                                  (string-make-multibyte (unibyte-string ?I)) subdir))))
                   answers)))))
          (nreverse answers)))
    (delete-directory dir t)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t ((\"short\" nil nil) (\"long\" \"I\" (\"Ir\" \"Is\")) (\"mixed\" \"I\" (\"Ir\" \"Is\"))) (\"ı\" (\"ı\")) (t t)) (nil ((\"short\" nil nil) (\"long\" \"I\" (\"Ir\" \"Is\")) (\"mixed\" \"I\" (\"Ir\" \"Is\"))) (\"ı\" (\"ı\")) (t t)))""#
    ]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(form, expect);
}
