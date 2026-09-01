//! Oracle parity tests for GNU `subr.el` yank text-property helpers.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_remove_yank_excluded_properties_runs_handlers_then_removes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:remove-yank-excluded-properties runs every configured
    // handler over each run of its property before it removes excluded
    // properties.  A `t' exclusion clears all text properties in the region.
    let form = r#"
(let ((selective
       (with-temp-buffer
         (insert (propertize "ab" 'foo 1 'drop 'gone 'keep 'yes)
                 (propertize "cd" 'foo 2 'drop 'gone 'bar 'z))
         (let ((calls nil)
               (yank-handled-properties
                (list (cons 'foo
                            (lambda (value start end)
                              (push (list value start end) calls)))))
               (yank-excluded-properties '(drop foo)))
           (remove-yank-excluded-properties (point-min) (point-max))
           (list
            (nreverse calls)
            (get-text-property 1 'foo)
            (get-text-property 1 'drop)
            (get-text-property 1 'keep)
            (get-text-property 3 'foo)
            (get-text-property 3 'drop)
            (get-text-property 3 'bar)))))
      (clear-all
       (with-temp-buffer
         (insert (propertize "xy" 'foo 1 'keep 'yes)
                 (propertize "z" 'bar 2))
         (let ((calls nil)
               (yank-handled-properties
                (list (cons 'foo
                            (lambda (value start end)
                              (push (list value start end) calls)))))
               (yank-excluded-properties t))
           (remove-yank-excluded-properties (point-min) (point-max))
           (list
            (nreverse calls)
            (text-properties-at 1)
            (text-properties-at 2)
            (text-properties-at 3))))))
  (list selective clear-all))
"#;
    let expect = expect_test::expect![[r#""ERR (void-variable calls)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_insert_buffer_substring_as_yank_processes_inserted_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((src (generate-new-buffer " *neomacs-oracle-yank-src*")))
  (unwind-protect
      (progn
        (with-current-buffer src
          (insert (propertize "ab" 'foo 1 'drop 'gone 'keep 'yes)
                  (propertize "cd" 'foo 2 'drop 'gone 'bar 'z)
                  (propertize "ef" 'foo 3 'drop 'gone 'tail 'ok)))
        (with-temp-buffer
          (let ((calls nil)
                (yank-handled-properties
                 (list (cons 'foo
                             (lambda (value start end)
                               (push (list value start end) calls)))))
                (yank-excluded-properties '(drop foo)))
            (insert "prefix:")
            (let ((opoint (point)))
              (insert-buffer-substring-as-yank src 3 7)
              (list
               (buffer-string)
               opoint
               (point)
               (nreverse calls)
               (text-properties-at (1+ opoint))
               (get-text-property (1+ opoint) 'drop)
               (get-text-property (1+ opoint) 'foo)
               (get-text-property (1+ opoint) 'bar)
               (get-text-property (- (point) 1) 'tail)))))))
    (kill-buffer src)))
"#;

    let expect = expect_test::expect![[r#""ERR (void-variable calls)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_yank_property_handlers_match_gnu_merge_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((font-lock-case
       (with-temp-buffer
         (insert "abcd")
         (let ((font-lock-defaults nil))
           (yank-handle-font-lock-face-property 'bold 1 3))
         (let ((after-enabled
                (list (get-text-property 1 'face)
                      (get-text-property 3 'face))))
           (let ((font-lock-defaults '(dummy)))
             (yank-handle-font-lock-face-property 'italic 3 5))
           (list after-enabled
                 (get-text-property 1 'face)
                 (get-text-property 3 'face)))))
      (category-case
       (with-temp-buffer
         (setplist 'neomacs-oracle-yank-category
                   '(face category-face help-echo category-help custom category-custom))
         (insert (propertize "ab" 'face 'original-face 'keep 'yes)
                 (propertize "cd" 'help-echo 'original-help))
         (unwind-protect
             (progn
               (yank-handle-category-property 'neomacs-oracle-yank-category
                                              (point-min) (point-max))
               (list
                (text-properties-at 1)
                (text-properties-at 3)))
           (setplist 'neomacs-oracle-yank-category nil)))))
  (list font-lock-case category-case))
"#;

    let expect = expect_test::expect![[
        r#""OK (((bold nil) bold nil) ((keep yes face original-face help-echo category-help custom category-custom) (face category-face help-echo original-help custom category-custom)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
