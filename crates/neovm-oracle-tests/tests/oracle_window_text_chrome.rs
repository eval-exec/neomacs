//! GNU xdisp.c: MODE-LINES includes only chrome the target window displays.
use crate::common;

#[test]
fn window_text_pixel_size_does_not_invent_disabled_chrome() {
    common::assert_oracle_parity_expect(
        r#"(save-window-excursion
             (with-temp-buffer
               (switch-to-buffer (current-buffer))
               (insert "one")
               (setq-local mode-line-format nil
                           header-line-format nil
                           tab-line-format nil)
               (let ((body (cdr (window-text-pixel-size nil nil t))))
                 (list
                  (mapcar
                   (lambda (mode)
                     (= body (cdr (window-text-pixel-size
                                   nil nil t nil (frame-pixel-height) mode))))
                   '(nil t mode-line header-line tab-line))
                  (list (window-mode-line-height)
                        (window-header-line-height)
                        (window-tab-line-height))))))"#,
        expect_test::expect![[r#""OK ((t t t t t) (0 0 0))""#]],
    );
}

#[test]
fn window_text_pixel_size_selects_requested_present_chrome() {
    common::assert_oracle_parity_expect(
        r#"(save-window-excursion
             (with-temp-buffer
               (switch-to-buffer (current-buffer))
               (insert "one")
               (let (results)
                 (dolist (formats '((nil nil nil)
                                    (("m") nil nil)
                                    (nil ("h") nil)
                                    (nil nil ("t"))
                                    (("m") ("h") ("t"))))
                   (setq-local mode-line-format (nth 0 formats)
                               header-line-format (nth 1 formats)
                               tab-line-format (nth 2 formats))
                   (push (mapcar
                          (lambda (mode)
                            (cdr (window-text-pixel-size
                                  nil nil t nil (frame-pixel-height) mode)))
                          '(nil t mode-line header-line tab-line arbitrary))
                         results))
                 (nreverse results))))"#,
        expect_test::expect![[
            r#""OK ((1 1 1 1 1 1) (1 2 2 1 1 1) (1 2 1 2 1 1) (1 2 1 1 2 1) (1 4 2 2 2 1))""#
        ]],
    );
}

#[test]
fn window_text_pixel_size_ignores_uninterned_selector_names() {
    common::assert_oracle_parity_expect(
        r#"(save-window-excursion
             (with-temp-buffer
               (switch-to-buffer (current-buffer))
               (insert "one")
               (setq-local mode-line-format '("m")
                           header-line-format '("h")
                           tab-line-format '("t"))
               (mapcar
                (lambda (mode)
                  (cdr (window-text-pixel-size
                        nil nil t nil (frame-pixel-height) mode)))
                (list nil t 'mode-line 'header-line 'tab-line
                      (make-symbol "t") (make-symbol "mode-line")
                      (make-symbol "header-line") (make-symbol "tab-line")))))"#,
        expect_test::expect![[r#""OK (1 4 2 2 2 1 1 1 1)""#]],
    );
}
