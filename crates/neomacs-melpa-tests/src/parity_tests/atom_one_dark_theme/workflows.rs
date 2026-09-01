//! What the mode-specific face remapping actually changes on screen.
//!
//! This corpus was already converted and is in good shape -- 55/55 in both
//! editors, real fontified buffers, real ansi-color output, a real
//! compilation buffer. One thing it does not establish is that the feature
//! the package exists for has any observable effect, and that gap is not
//! visible from reading the tests, because the assertion that misses it
//! looks exactly like one that would catch it.

use expect_test::expect;

use super::ParityBatchCase;

/// The same HTML buffer with the remapping on and off, side by side.
///
/// `remapping.rs`'s `..._registered_hook_applies_real_html_font_lock_workflow`
/// fontifies a real HTML buffer and pins the `face` property of six tokens.
/// Those six values are **identical with the remapping disabled** -- measured,
/// not assumed, and the first two elements of this snapshot are that
/// measurement. `face-remapping-alist` is buffer-local and consulted by the
/// display engine; it does not change the `face` text property, so a test can
/// fontify a real buffer, read real properties out of it, and still be blind
/// to the entire feature. That test is saved from proving nothing only by the
/// `face-remapping-alist` element sitting in the same returned list.
///
/// What the remap changes is the colour those two faces resolve to. In an
/// HTML buffer `font-lock-function-name-face` is redirected from the theme's
/// blue `#61AFEF` to red `#E06C75`, and `font-lock-variable-name-face` from
/// red `#E06C75` to orange `#D19A66` -- so a tag name and an attribute name
/// swap out of the colours the same faces carry everywhere else in the theme.
///
/// The effective colour is read out of `face-remapping-alist` rather than
/// from the display, because a batch frame has no redisplay to ask and
/// manufacturing one would be inventing the capability. The relative-spec
/// lookup here is what the display engine does with the same alist, and the
/// unremapped `face-attribute` value beside it is what the entry overrides.
fn the_html_remap_changes_the_colour_and_leaves_the_face_property_untouched() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_html_remap_changes_the_colour_and_leaves_the_face_property_untouched",
        r##"(cl-labels
              ((observe
                ()
                (with-temp-buffer
                  (insert
                   "<section class=\"card\">Hello</section>")
                  (html-mode)
                  (font-lock-ensure)
                  (list
                   :face-properties
                   (mapcar
                    (lambda (token)
                      (goto-char (point-min))
                      (search-forward token)
                      (get-text-property
                       (match-beginning 0)
                       'face))
                    '("section" "class"))
                   :remapped
                   (mapcar
                    (lambda (face)
                      (let ((entry
                             (assq face face-remapping-alist)))
                        (list
                         face
                         (face-attribute face :foreground nil t)
                         (plist-get
                          (car-safe (cdr entry))
                          :foreground))))
                    '(font-lock-function-name-face
                      font-lock-variable-name-face))))))
            (unwind-protect
                (progn
                  (enable-theme 'atom-one-dark)
                  (let* ((atom-one-dark-theme-force-faces-for-mode t)
                         (on (observe))
                         (atom-one-dark-theme-force-faces-for-mode nil)
                         (off (observe)))
                    (list
                     :with-remapping on
                     :without-remapping off
                     :face-properties-are-identical
                     (equal (plist-get on :face-properties)
                            (plist-get off :face-properties))
                     :colours-differ
                     (not (equal (plist-get on :remapped)
                                 (plist-get off :remapped))))))
              (when (custom-theme-enabled-p 'atom-one-dark)
                (disable-theme 'atom-one-dark))))"##,
        expect![[
            r##"OK (:with-remapping (:face-properties (font-lock-function-name-face font-lock-variable-name-face) :remapped ((font-lock-function-name-face "#61AFEF" "#E06C75") (font-lock-variable-name-face "#E06C75" "#D19A66"))) :without-remapping (:face-properties (font-lock-function-name-face font-lock-variable-name-face) :remapped ((font-lock-function-name-face "#61AFEF" nil) (font-lock-variable-name-face "#E06C75" nil))) :face-properties-are-identical t :colours-differ t)"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![the_html_remap_changes_the_colour_and_leaves_the_face_property_untouched()]
}
