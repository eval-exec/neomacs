use expect_test::expect;

use super::ParityBatchCase;

/// What an alabaster theme actually changes on the display this suite runs on
/// -- which is nothing, and that is the finding.
///
/// All 491 of the package's face specs are written `((,c ...))' where `c' is
/// `((class color) (min-colors 256))', and none of them carries a `(t ...)'
/// fallback.  A batch frame reports visual class `static-gray', so the
/// `(class color)' half of that clause cannot match however many colours the
/// display claims, and every themed face resolves to `unspecified'.
///
/// This workflow states that on the record: the display facts, the gate's own
/// answer via `face-spec-set-match-display', the resolved appearance of a
/// representative face with the theme off and on, and the count of themed
/// faces whose resolved appearance the theme changes.  Pinning the reason
/// beside the result is what the theme notes ask for when a package's
/// behaviour is gated on a capability the batch editor lacks.
///
/// It is worth having because the rest of this package's suite reads as though
/// the opposite were true.  Thirteen tests in `rendering.rs' and eleven in
/// `lifecycle.rs' call `face-attribute' after enabling a theme, with names like
/// "resolves titles todos links blocks and metadata faces" -- and every colour
/// they record is `"unspecified-fg"' or `"unspecified-bg"'.  They are green,
/// they are stable, and they assert that nothing resolved.
///
/// The package prelude compounds it.  `TRUE_COLOR_PRELUDE' redefines
/// `display-color-cells' to return 16777216, which reads as "the display
/// problem has been handled".  It has not: the clause fails on `(class color)',
/// not on the colour count, so `face-spec-set-match-display' returns nil with
/// the fake in place exactly as it does without it.  A fake that does not work
/// is worse than no fake, because it stops the next reader looking.
fn no_themed_face_resolves_on_this_display_and_the_gate_says_why() -> ParityBatchCase {
    ParityBatchCase::value(
        "no_themed_face_resolves_on_this_display_and_the_gate_says_why",
        r##"(let* ((sample '(default font-lock-string-face font-lock-keyword-face
                 region mode-line))
       (resolved (lambda ()
                   (mapcar (lambda (face)
                             (list face
                                   (face-attribute face :foreground nil t)
                                   (face-attribute face :background nil t)))
                           sample)))
       (before (funcall resolved)))
  (load-theme 'alabaster-themes-dark t)
  (let ((after (funcall resolved)))
    (list :display (list :color-cells (display-color-cells)
                         :visual-class (display-visual-class)
                         :graphic (display-graphic-p))
          :gate (list :clause '((class color) (min-colors 256))
                      :matches (face-spec-set-match-display
                                '((class color) (min-colors 256)) nil)
                      :colour-count-alone-matches
                      (face-spec-set-match-display '((min-colors 256)) nil)
                      :class-alone-matches
                      (face-spec-set-match-display '((class color)) nil))
          :theme-enabled (and (memq 'alabaster-themes-dark custom-enabled-themes) t)
          :registered-face-count
          (length (seq-filter (lambda (s) (eq (car s) 'theme-face))
                              (get 'alabaster-themes-dark 'theme-settings)))
          :before before
          :after after
          :faces-whose-appearance-changed
          (seq-remove #'null
                      (seq-mapn (lambda (b a) (unless (equal b a) (car b)))
                                before after)))))"##,
        expect![[
            r#"OK (:display (:color-cells 16777216 :visual-class static-gray :graphic nil) :gate (:clause ((class color) (min-colors 256)) :matches nil :colour-count-alone-matches t :class-alone-matches nil) :theme-enabled t :registered-face-count 501 :before ((default "unspecified-fg" "unspecified-bg") (font-lock-string-face unspecified unspecified) (font-lock-keyword-face unspecified unspecified) (region unspecified unspecified) (mode-line unspecified unspecified)) :after ((default "unspecified-fg" "unspecified-bg") (font-lock-string-face unspecified unspecified) (font-lock-keyword-face unspecified unspecified) (region unspecified unspecified) (mode-line unspecified unspecified)) :faces-whose-appearance-changed nil)"#
        ]],
    )
}

fn the_registered_specs_carry_the_real_colours_the_resolved_reads_cannot_see() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_registered_specs_carry_the_real_colours_the_resolved_reads_cannot_see",
        r##"(progn
  (load-theme 'alabaster-themes-dark t)
  (let ((spec (lambda (face)
                (let (found)
                  (dolist (setting (get 'alabaster-themes-dark 'theme-settings))
                    (when (and (eq (car setting) 'theme-face)
                               (eq (nth 1 setting) face)
                               (not found))
                      (setq found (copy-tree (nth 3 setting)))))
                  found))))
    (list :specs (mapcar (lambda (face) (list face (funcall spec face)))
                         '(default font-lock-string-face font-lock-keyword-face
                           org-level-1 diff-added))
          :every-spec-uses-the-same-clause
          (let ((clauses (mapcar (lambda (face) (car (car (funcall spec face))))
                                 '(default font-lock-string-face
                                   font-lock-keyword-face org-level-1
                                   diff-added))))
            (list :clauses (delete-dups (copy-tree clauses))
                  :count (length clauses))))))"##,
        expect![[
            r##"OK (:specs ((default ((((class color) (min-colors 256)) :background "#0E1415" :foreground "#CECECE"))) (font-lock-string-face ((((class color) (min-colors 256)) :foreground "#95CB82"))) (font-lock-keyword-face ((((class color) (min-colors 256)) :foreground "#CECECE"))) (org-level-1 ((((class color) (min-colors 256)) :inherit bold :height unspecified :weight unspecified :foreground "#8AB1F0"))) (diff-added ((((class color) (min-colors 256)) :background "#1f3a1f" :foreground "#95CB82")))) :every-spec-uses-the-same-clause (:clauses (((class color) (min-colors 256))) :count 5))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        no_themed_face_resolves_on_this_display_and_the_gate_says_why(),
        the_registered_specs_carry_the_real_colours_the_resolved_reads_cannot_see(),
    ]
}
