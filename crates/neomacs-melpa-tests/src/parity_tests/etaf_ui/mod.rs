//! Behavioural parity for the pinned `etaf-ui` component library.
//!
//! Probes cover the deterministic helpers the package documents as public:
//! color mixing/contrast/legibility, QR painting, icon rows, and the native
//! editor value/replace contract.  Mounted component interaction, floating
//! surfaces and the gallery belong to the GUI lane.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)

(defun neomacs-etaf-ui-catch (form)
  "Evaluate FORM, returning its value or (ERROR . DATA)."
  (condition-case err
      (funcall form)
    (error (list 'ERROR (car err) (cdr err)))))
"####;

fn etaf_ui_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-ui", "0.3.3"), "etaf-ui.el")
        .expect("prepare pinned etaf-ui source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_ui_batch() {
    let cases = vec![
        // Color mixing, WCAG contrast and legibility are pure functions.
        ParityBatchCase::value(
            "color_math",
            r####"
(list (etaf-ui-color-mix "#000000" "#ffffff" 0.5)
      (etaf-ui-color-mix "#336699" "#ff0000" 0.25)
      (etaf-ui-color-mix "#abcdef" "#abcdef" 0)
      (etaf-ui-color-contrast "#000000" "#ffffff")
      (etaf-ui-color-contrast "#777777" "#ffffff")
      (etaf-ui-color-legible "#777777" "#ffffff")
      (etaf-ui-color-legible "#ffff00" "#ffffff"))
"####,
            expect![[
                r##"OK ("#808080" "#664C73" "#ABCDEF" 21.0 4.478089453577214 "#6D6D6D" "#707000")"##
            ]],
        ),
        // QR painting: module rows, quiet zone, compact block glyphs and the
        // documented type error.
        ParityBatchCase::value(
            "qr_painting",
            r####"
(list (split-string (etaf-ui-qr-paint '("111" "101" "111")) "\n")
      (etaf-ui-qr-paint '("1" "11") 0)
      (nth 1 (split-string (etaf-ui-qr-paint '("11" "10") 2) "\n"))
      (neomacs-etaf-ui-catch (lambda () (etaf-ui-qr-paint '("1" 2)))))
"####,
            expect![[
                r##"OK ((#("           " 0 11 (face #1=(:foreground "#000000" :background "#FFFFFF"))) #("           " 0 11 (face #1#)) #("    █▀█    " 0 11 (face #1#)) #("    ▀▀▀    " 0 11 (face #1#)) #("           " 0 11 (face #1#)) #("           " 0 11 (face #1#))) #("█▄" 0 2 (face #1#)) #("  █▀  " 0 6 (face #1#)) (ERROR wrong-type-argument (list-of-strings ("1" 2))))"##
            ]],
        ),
        // Icon rows are propertized strings; inspect text and face/display
        // properties without depending on a font.
        ParityBatchCase::value(
            "icon_lines",
            r####"
(let ((lines (neomacs-etaf-ui-catch
              (lambda () (etaf-ui-icon-lines "check")))))
  (if (and (listp lines) (eq (car lines) 'ERROR))
      lines
    (list (mapcar #'substring-no-properties lines)
          (mapcar (lambda (line)
                    (list (length line)
                          (get-text-property 0 'face line)
                          (get-text-property 0 'display line)))
                  lines))))
"####,
            expect![[r#"OK (("✓") ((1 nil nil)))"#]],
        ),
        // The native editor exposes a property-free value and a replace that
        // updates it.
        ParityBatchCase::value(
            "editor_value_and_replace",
            r####"
(with-temp-buffer
  (etaf-ui-editor-mode 1)
  (insert "hello")
  (let ((before (etaf-ui-editor-value (current-buffer)))
        (replaced (etaf-ui-editor-replace (current-buffer) "world")))
    (list before
          (etaf-ui-editor-value (current-buffer))
          replaced
          (buffer-substring-no-properties (point-min) (point-max)))))
"####,
            expect![[r#"OK ("hello" "world" t "world")"#]],
        ),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("etaf-ui parity batch");
    assert_oracle_batch_cases(etaf_ui_oracle(), test_name, "etaf-ui parity", &cases);
}
