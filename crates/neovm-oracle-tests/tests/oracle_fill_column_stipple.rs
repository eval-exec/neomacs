//! Issue #600's configuration must preserve the bitmap and the space character.
use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn fill_column_indicator_preserves_author_stipple_configuration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(progn
             (require 'display-fill-column-indicator)
             (setq-default display-fill-column-indicator-character ?\s)
             (let* ((w (window-font-width))
                    (stipple `(,w 1 ,(apply #'unibyte-string
                                           (append (make-list (1- (/ (+ w 7) 8)) ?\0)
                                                   '(1))))))
               (set-face-attribute 'fill-column-indicator nil :stipple stipple)
               (display-fill-column-indicator-mode 1)
               (let ((actual (face-attribute 'fill-column-indicator :stipple nil)))
                 (list (= (car actual) w)
                       (cadr actual)
                       (string-to-list (caddr actual))
                       display-fill-column-indicator-character
                       display-fill-column-indicator))))"#,
        expect_test::expect![[r#""OK (t 1 (1) 32 t)""#]],
    );
}
