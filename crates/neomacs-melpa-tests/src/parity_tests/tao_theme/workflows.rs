use expect_test::expect;

use super::ParityBatchCase;

/// The golden-ratio greyscale generator: 32 exact values from the pure
/// phi arithmetic.
fn the_golden_scale_is_the_exact_gradient() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_golden_scale_is_the_exact_gradient",
        r####"(let ((scale (tao-theme-golden-scale)))
  (list :source (tao--test-source-state)
        :length (length scale)
        :first (seq-take scale 4)
        :last (seq-take (nreverse (copy-sequence scale)) 4)
        :ascending (equal scale (sort (copy-sequence scale) #'<))))"####,
        expect![[
            r#"OK (:source (:upstream-tree "f7bed42b2c6c5e892f7f3bc83da6abc5a3ca7725" :feature t :version "20250717.347") :length 32 :first (0 0 0 0) :last (255 255 255 255) :ascending t)"#
        ]],
    )
}

/// The yin (dark) and yang (light) palettes are the inverted gradients
/// with the named color slots.
fn the_yin_and_yang_palettes_are_the_named_gradients() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_yin_and_yang_palettes_are_the_named_gradients",
        r####"(let* ((yin (tao-theme-yin-palette))
      (yang (tao-theme-yang-palette)))
  (list :yin-names (mapcar #'car yin)
        :yin-first (seq-take yin 3)
        :yang-first (seq-take yang 3)
        :yin-background (cdr (assoc "color-4" yin))
        :yin-foreground (cdr (assoc "color-10" yin))
        :yang-background (cdr (assoc "color-4" yang))
        :yin-equals-yang-reversed
        (equal (mapcar #'cdr yin)
               (nreverse (mapcar #'cdr yang)))))"####,
        expect![[
            r##"OK (:yin-names ("color-1" "color-2" "color-3" "color-4" "color-5" "color-6" "color-7" "color-8" "color-9" "color-10" "color-11" "color-12" "color-13" "color-14" "color-15") :yin-first (("color-1" . "#050505") ("color-2" . "#090909") ("color-3" . "#0E0E0E")) :yang-first (("color-1" . "#FCFCFC") ("color-2" . "#FAFAFA") ("color-3" . "#F6F6F6")) :yin-background "#171717" :yin-foreground "#DADADA" :yang-background "#F1F1F1" :yin-equals-yang-reversed t)"##
        ]],
    )
}

/// The sepia option shifts the palette through the documented depth and
/// saturation parameters.
fn the_sepia_option_shifts_the_palette() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_sepia_option_shifts_the_palette",
        r####"(let* ((plain (tao-theme-yin-palette))
      (sepia (let ((tao-theme-use-sepia t))
               (tao-theme-yin-palette)))
      (deep (let ((tao-theme-use-sepia t)
                  (tao-theme-sepia-depth 20))
              (tao-theme-yin-palette))))
  (list :plain-mid (cdr (assoc "color-7" plain))
        :sepia-mid (cdr (assoc "color-7" sepia))
        :deep-mid (cdr (assoc "color-7" deep))
        :sepia-differs (not (equal plain sepia))
        :depth-differs (not (equal sepia deep))))"####,
        expect![[
            r##"OK (:plain-mid "#616161" :sepia-mid "#737063" :deep-mid "#857F63" :sepia-differs t :depth-differs t)"##
        ]],
    )
}

/// Loading the yin theme applies the palette to the built-in faces.
fn the_yin_theme_applies_the_palette_to_the_faces() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_yin_theme_applies_the_palette_to_the_faces",
        r####"(unwind-protect
    (progn
      (load-theme 'tao-yin t)
      (tao--test-faces))
  (disable-theme 'tao-yin))"####,
        expect![[
            r##"OK (:default-fg "#DADADA" :default-bg "#171717" :link-fg "#F6F6F6" :show-paren-match "#FAFAFA" :font-lock-keyword "#F1F1F1" :font-lock-string "#9E9E9E" :font-lock-comment "#9E9E9E" :mode-line-fg unspecified :mode-line-bg unspecified)"##
        ]],
    )
}

/// Loading the yang theme flips the palette to the light variant.
fn the_yang_theme_flips_the_palette() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_yang_theme_flips_the_palette",
        r####"(unwind-protect
    (progn
      (load-theme 'tao-yang t)
      (tao--test-faces))
  (disable-theme 'tao-yang))"####,
        expect![[
            r##"OK (:default-fg "#3C3C3C" :default-bg "#F1F1F1" :link-fg "#0E0E0E" :show-paren-match "#090909" :font-lock-keyword "#171717" :font-lock-string "#9E9E9E" :font-lock-comment "#9E9E9E" :mode-line-fg unspecified :mode-line-bg unspecified)"##
        ]],
    )
}

/// The UI helpers: the height scaling and the boxed property face.
fn the_ui_helpers_scale_and_box() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_ui_helpers_scale_and_box",
        r####"(let ((default-height (face-attribute 'default :height)))
  (list :height-10 (tao-theme-height 1.0)
        :height-12 (tao-theme-height 1.2)
        :boxed (tao-boxed "#ff0000")))"####,
        expect![[
            r##"OK (:height-10 1.0 :height-12 1.0 :boxed (:color "#ff0000" :line-width -1))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_golden_scale_is_the_exact_gradient(),
        the_yin_and_yang_palettes_are_the_named_gradients(),
        the_sepia_option_shifts_the_palette(),
        the_yin_theme_applies_the_palette_to_the_faces(),
        the_yang_theme_flips_the_palette(),
        the_ui_helpers_scale_and_box(),
    ]
}
