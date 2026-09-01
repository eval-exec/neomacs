//! Oracle parity tests for GNU `button.el` text and overlay button semantics.
//!
//! GNU implements button types, inherited properties, text-property buttons,
//! overlay buttons, and boundary lookup in `lisp/button.el`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_button_type_inheritance_and_text_button_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'button)
  (define-button-type 'neomacs-oracle-base-button
    'help-echo "base-help"
    'oracle-prop 'base)
  (define-button-type 'neomacs-oracle-child-button
    :supertype 'neomacs-oracle-base-button
    'oracle-prop 'child
    'child-only t)
  (with-temp-buffer
    (insert "abcdef")
    (let ((button (make-text-button 2 5 :type 'neomacs-oracle-child-button
                                    'local-prop "local")))
      (list button
            (markerp (button-at 3))
            (button-start (button-at 3))
            (button-end (button-at 3))
            (button-get (button-at 3) 'type)
            (button-get (button-at 3) 'oracle-prop)
            (button-get (button-at 3) 'child-only)
            (button-get (button-at 3) 'help-echo)
            (button-get (button-at 3) 'local-prop)
            (button-at 1)
            (button-at 5)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (2 t 2 5 neomacs-oracle-child-button child t \"base-help\" \"local\" nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_insert_text_button_next_previous_and_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'button)
  (with-temp-buffer
    (insert "pre ")
    (insert-text-button "one" 'tag 1)
    (insert " gap ")
    (insert-text-button "two" 'tag 2)
    (let ((first (button-at 5))
          (second (button-at 13)))
      (button-put first 'tag 'changed)
      (button-put second 'extra "yes")
      (list (buffer-string)
            (button-start first)
            (button-end first)
            (button-get first 'tag)
            (button-start second)
            (button-end second)
            (button-get second 'tag)
            (button-get second 'extra)
            (button-get (next-button (point-min)) 'tag)
            (button-get (next-button (button-end first)) 'tag)
            (button-get (previous-button (point-max)) 'tag)
            (button-at (point-min))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"pre one gap two\" 4 7 (tag changed category default-button button (t)) 12 15 (extra \"yes\" tag 2 category default-button button (t))) 5 8 changed 13 16 2 \"yes\" changed 2 2 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_make_text_button_string_and_category_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'button)
  (let ((s (make-text-button "label" nil 'tag 'string-button)))
    (list s
          (get-text-property 1 'tag s)
          (get-text-property 1 'type s)
          (get-text-property 1 'button s)
          (condition-case err
              (make-text-button "bad" nil 'category 'x)
            (error (list (car err) (cadr err))))
          (condition-case err
              (with-temp-buffer
                (insert "abc")
                (let ((b (make-text-button 1 2)))
                  (button-put b 'category 'x)))
            (error (list (car err) (cadr err)))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"label\" 0 5 (tag string-button category default-button button (t))) string-button button (t) (error \"Button ‘category’ property may not be set directly\") (error \"Button ‘category’ property may not be set directly\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_overlay_buttons_and_activation_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'button)
  (let ((log nil))
    (with-temp-buffer
      (insert "0123456789")
      (let ((b (make-button 3 7
                            'button-data 'payload
                            'action (lambda (x) (setq log (cons (list 'action x) log)))
                            'mouse-action (lambda (x) (setq log (cons (list 'mouse x) log))))))
        (button-activate b)
        (button-activate b t)
        (list (overlayp b)
              (eq (button-at 4) b)
              (button-start b)
              (button-end b)
              (button-label b)
              (button-get b 'type)
              (nreverse log))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t 3 7 \"2345\" button ((action payload) (mouse payload)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
