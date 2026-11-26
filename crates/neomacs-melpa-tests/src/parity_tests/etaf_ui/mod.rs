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
        // Theme tokens map a small palette onto the catalog's semantic roles
        // and derive omitted roles from surface/text lightness.
        ParityBatchCase::value(
            "theme_tokens",
            r####"
(let ((tokens (etaf-ui-theme-tokens '(:bg "#FFFFFF" :fg "#000000"))))
  (list (plist-get tokens :ui-panel-bg)
        (plist-get tokens :ui-border)
        (plist-get tokens :ui-muted-fg)
        (plist-get tokens :ui-accent)
        (plist-get tokens :ui-success-fg)
        (plist-get tokens :ui-info-fg)
        (condition-case err
            (etaf-ui-theme-tokens '(:fg "#000000"))
          (error (list (car err) (cdr err))))))
"####,
            expect![[
                r##"OK ("#FFFFFF" "#D1D1D1" "#595959" "#000000" "#1A7F37" "#000000" (wrong-type-argument (stringp :bg nil)))"##
            ]],
        ),
        // Icon sets can be registered, replaced and removed.
        ParityBatchCase::value(
            "icon_sets",
            r####"
(let ((spec '(:kind font :family "Monospace" :height 0.5
              :glyphs ((play . "p")))))
  (etaf-ui-icon-set-register 'test-icons spec)
  (let ((registered (and (memq 'test-icons (etaf-ui-icon-sets)) t))
        (names (etaf-ui-icon-names 'test-icons))
        (readback (etaf-ui-icon-set-spec 'test-icons)))
    (etaf-ui-icon-set-register
     'test-icons '(:kind font :family "Monospace" :height 0.5
                   :glyphs ((pause . "P"))))
    (let ((replaced (etaf-ui-icon-names 'test-icons)))
      (etaf-ui-icon-set-remove 'test-icons)
      (list registered names readback replaced
            (etaf-ui-icon-set-spec 'test-icons)
            (etaf-ui-icon-set-remove 'test-icons)))))
"####,
            expect![[
                r#"OK (t (play) (:kind font :family "Monospace" :height 0.5 :glyphs ((play . "p"))) (pause) nil nil)"#
            ]],
        ),
        // The shipped Markdown parser exposes title, outline, block types,
        // footnotes, features, extension predicates and highlighting.
        ParityBatchCase::value(
            "markdown_parse",
            r####"
(let ((document
       (etaf-ui-markdown-parse
        "---\ntitle: Doc\n---\n\n# Intro\n\nText[^a].\n\n## Part\n\n[^a]: note\n")))
  (list (plist-get document :title)
        (plist-get document :outline)
        (mapcar (lambda (block) (plist-get block :type))
                (plist-get document :children))
        (plist-get document :footnotes)
        (plist-get document :features)
        (etaf-ui-markdown-extension-p 'gfm)
        (etaf-ui-markdown-highlight-code "elisp" "(+ 1 2)")))
"####,
            expect![[
                r#"OK ("Doc" ((1 "Intro" "intro") (2 "Part" "part")) (heading paragraph heading) ((:type footnote-def :label "a" :number 1 :children ((:type paragraph :text "note" :children ((:type text :text "note")))))) (:front-matter t :tables t :task-lists t :strikethrough t :autolinks t :footnotes t :alerts t :extensions (definition-list subscript superscript abbreviation math emoji mark icon kbd badge)) nil ((:text "(+ 1 2)" :props nil)))"#
            ]],
        ),
        // SVG helpers emit compact escaped markup and reject invalid scenes.
        ParityBatchCase::value(
            "svg_builders",
            r####"
(list (etaf-ui-svg-rect 1 2 3 4 :rx 2 :fill "#FFF")
      (etaf-ui-svg-circle 0 0 4.5)
      (etaf-ui-svg-line 0 0 10 0 :stroke "#123" :stroke-width 0.85)
      (etaf-ui-svg-path "M0 0 L1 1")
      (etaf-ui-svg-group "<circle r='1'/>" :transform "translate(1,2)")
      (etaf-ui-svg-text 1 2 "车 & <将>")
      (condition-case err
          (etaf-ui-svg-rect 0 0 -1 2)
        (error (list (car err) (cdr err)))))
"####,
            expect![[
                r#"OK ("<rect x='1' y='2' width='3' height='4' rx='2' fill='#FFF'/>" "<circle cx='0' cy='0' r='4.5'/>" "<line x1='0' y1='0' x2='10' y2='0' stroke='#123' stroke-width='0.85'/>" "<path d='M0 0 L1 1'/>" "<g transform='translate(1,2)'><circle r='1'/></g>" "<text x='1' y='2'>车 &amp; &lt;将&gt;</text>" (etaf-ui-svg-error ("Rectangle width must be a positive number, got -1")))"#
            ]],
        ),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("etaf-ui parity batch");
    assert_oracle_batch_cases(etaf_ui_oracle(), test_name, "etaf-ui parity", &cases);
}
