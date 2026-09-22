;;; color-spec-invalid-hex.el --- Invalid hex color regression tests -*- lexical-binding: t; -*-

;; Run with GNU Emacs or Neomacs:
;;   --batch -Q -l test/neomacs/color-spec-invalid-hex.el
;;   -f ert-run-tests-batch-and-exit

(require 'ert)

(ert-deftest neomacs-color-spec-rejects-non-hex-bytes ()
  ;; These strings hit each byte-slicing branch of the hex parser.  Invalid
  ;; color input must return nil, including when a slice would split UTF-8.
  ;; Include two-, three-, and four-byte characters at slicing boundaries.
  (dolist (spec '("#あ" "#éa" "#中" "#中文" "#한" "#😀ab"
                  "#あい" "#日abc"
                  "#123あ123456" "#1234567あ12"
                  "#12+345" "#+00100000000"))
    (should-not (color-values-from-color-spec spec))))

(ert-deftest neomacs-color-spec-keeps-valid-hex-values ()
  (dolist (case '(("#f05" . (65535 0 21845))
                  ("#1fb0C5" . (7967 45232 50629))
                  ("#1f83b0ADC5e2" . (8067 45229 50658))))
    (should (equal (color-values-from-color-spec (car case)) (cdr case)))))

(provide 'color-spec-invalid-hex)
;;; color-spec-invalid-hex.el ends here
