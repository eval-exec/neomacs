use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, POS_TIP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const POS_TIP_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const POS_TIP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'tooltip)
(require 'pos-tip)

(defun pos-tip-test-property-runs (string property)
  (let ((position 0)
        runs)
    (while (< position (length string))
      (let* ((value (get-text-property position property string))
             (next
              (or
               (next-single-property-change
                position property string)
               (length string))))
        (when value
          (push (list position next value) runs))
        (setq position next)))
    (nreverse runs)))

(defun pos-tip-test-row-metrics (rows)
  (mapcar
   (lambda (row)
     (list
      (substring-no-properties row)
      :columns (string-width row)
      :faces (pos-tip-test-property-runs row 'face)))
   rows))
"##;

fn pos_tip_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(POS_TIP_MELPA_PIN, "pos-tip.el")
        .expect("prepare pinned pos-tip source below ./tmp")
        .with_prelude(POS_TIP_TEST_PRELUDE)
        .with_timeout(POS_TIP_TEST_TIMEOUT)
}

fn release_notes_are_split_filled_and_truncated_by_display_columns() -> ParityBatchCase {
    let elisp_form = r##"
(let ((pos-tip-tab-width 4)
      (release
       "Deploy\torders after validation. 状态正常，准备发布。")
      (paragraphs
       "First paragraph wraps for an operator.\n\nSecond stays separate."))
  (let ((split (pos-tip-split-string release 14 2 nil nil 4))
        (filled (pos-tip-fill-string paragraphs 16 1 'left nil 5))
        (truncated
         (pos-tip-truncate-string
          "deploy-production-service\n状态检查完成\nrollback-ready\nignored"
          12 3)))
    (list
     :split (pos-tip-test-row-metrics split)
     :filled
     (pos-tip-test-row-metrics
      (split-string filled "\n" nil))
     :truncated
     (pos-tip-test-row-metrics
      (split-string truncated "\n" nil)))))
"##;
    let expect = expect![[
        r####"OK (:split (("  Deploy  orde" :columns 14 :faces nil) ("  rs after val" :columns 14 :faces nil) ("  idation. 状" :columns 13 :faces nil) ("  态正常，准备" :columns 14 :faces nil)) :filled ((" First paragraph" :columns 16 :faces ((0 1 default))) (" wraps for an" :columns 13 :faces ((0 1 default))) (" operator." :columns 10 :faces ((0 1 default))) ("" :columns 0 :faces nil) (" Second stays" :columns 13 :faces ((0 1 default)))) :truncated (("deploy-produ" :columns 12 :faces nil) ("状态检查完成" :columns 12 :faces nil) ("rollback-rea" :columns 12 :faces nil)))"####
    ]];
    ParityBatchCase::value(
        "release_notes_are_split_filled_and_truncated_by_display_columns",
        elisp_form,
        expect,
    )
}

fn tooltip_dimensions_account_for_tabs_wide_text_borders_and_line_spacing() -> ParityBatchCase {
    let elisp_form = r##"
(let ((pos-tip-border-width 2)
      (pos-tip-internal-border-width 3))
  (cl-letf (((symbol-function 'default-value)
             (lambda (symbol)
               (and (eq symbol 'line-spacing) 3)))
            ((symbol-function 'frame-parameter)
             (lambda (_frame parameter)
               (and (eq parameter 'line-spacing) 99)))
            ((symbol-function 'frame-char-height)
             (lambda (&optional _frame) 20)))
    (let ((integer-spacing
           (pos-tip-tooltip-height 3 20 'release-frame)))
      (cl-letf (((symbol-function 'default-value)
                 (lambda (_symbol) nil))
                ((symbol-function 'frame-parameter)
                 (lambda (_frame parameter)
                   (and (eq parameter 'line-spacing) 0.25))))
        (let ((fractional-spacing
               (pos-tip-tooltip-height 3 20 'release-frame)))
          (cl-letf (((symbol-function 'frame-parameter)
                     (lambda (_frame _parameter) nil)))
            (list
             :strings
             (mapcar
              (lambda (text)
                (list text
                      :size (pos-tip-string-width-height text)))
              '("ab\tc\n状态\n" "single line" ""))
             :pixel-widths
             (list
              (pos-tip-tooltip-width 0 8)
              (pos-tip-tooltip-width 12 8)
              (pos-tip-tooltip-width 7 11))
             :pixel-heights
             (list
              :integer integer-spacing
              :fractional fractional-spacing
              :none
              (pos-tip-tooltip-height
               3 20 'release-frame)))))))))
"##;
    let expect = expect![[
        r####"OK (:strings (("ab\11c\n状态\n" :size (9 . 2)) ("single line" :size (11 . 1)) ("" :size (0 . 1))) :pixel-widths (10 106 87) :pixel-heights (:integer 79 :fractional 85 :none 70))"####
    ]];
    ParityBatchCase::value(
        "tooltip_dimensions_account_for_tabs_wide_text_borders_and_line_spacing",
        elisp_form,
        expect,
    )
}

fn release_tooltip_colors_follow_face_pair_customization_and_fallback_precedence() -> ParityBatchCase
{
    let elisp_form = r##"
(cl-letf (((symbol-function 'facep)
           (lambda (value)
             (eq value 'release-tip-face)))
          ((symbol-function 'face-attribute)
           (lambda (face attribute &rest _)
             (cond
              ((and (eq face 'release-tip-face)
                    (eq attribute :foreground))
               "gold")
              ((and (eq face 'release-tip-face)
                    (eq attribute :background))
               "navy")
              (t 'unspecified))))
          ((symbol-function 'face-foreground)
           (lambda (_face) "fallback-fg"))
          ((symbol-function 'face-background)
           (lambda (_face) "fallback-bg")))
  (let ((pos-tip-foreground-color nil)
        (pos-tip-background-color nil))
    (list
     :face
     (cons
      (pos-tip-compute-foreground-color 'release-tip-face)
      (pos-tip-compute-background-color 'release-tip-face))
     :pair
     (cons
      (pos-tip-compute-foreground-color '("ivory" . "maroon"))
      (pos-tip-compute-background-color '("ivory" . "maroon")))
     :custom
     (let ((pos-tip-foreground-color "custom-fg")
           (pos-tip-background-color "custom-bg"))
       (cons
        (pos-tip-compute-foreground-color nil)
        (pos-tip-compute-background-color nil)))
     :fallback
     (cons
      (pos-tip-compute-foreground-color nil)
      (pos-tip-compute-background-color nil)))))
"##;
    let expect = expect![[
        r####"OK (:face ("gold" . "navy") :pair ("ivory" . "maroon") :custom ("custom-fg" . "custom-bg") :fallback ("fallback-fg" . "fallback-bg"))"####
    ]];
    ParityBatchCase::value(
        "release_tooltip_colors_follow_face_pair_customization_and_fallback_precedence",
        elisp_form,
        expect,
    )
}

fn frame_geometry_normalizes_chrome_and_uses_absolute_coordinates_when_needed() -> ParityBatchCase {
    let elisp_form = r##"
(cl-letf
    (((symbol-function 'frame-parameter)
      (lambda (frame parameter)
        (cdr
         (assq
          parameter
          (cdr
           (assq
            frame
            '((primary
               (left . 420) (top . 180)
               (menu-bar-lines . 1)
               (tool-bar-lines . 0))
              (peer
               (left . 120) (top . 80)
               (menu-bar-lines . 1)
               (tool-bar-lines . 0))
              (toolbar-peer
               (left . 900) (top . 700)
               (menu-bar-lines . 1)
               (tool-bar-lines . 1))
              (different-chrome
               (left . 900) (top . 700)
               (menu-bar-lines . 0)
               (tool-bar-lines . 1))))))))))
  (list
   :natural-number-bits
   (mapcar
    (lambda (value)
      (list value
            (pos-tip-normalize-natnum value)
            (pos-tip-normalize-natnum value 2)))
    '(-1 0 1 5 2.5 text))
   :terminal-window-system
   (pos-tip-window-system)
   :invalid-frame
   (condition-case error-data
       (pos-tip-window-system 'not-a-frame)
     (error error-data))
   :same-chrome
   (pos-tip-frame-relative-position 'primary 'peer)
   :different-chrome
   (pos-tip-frame-relative-position
    'primary 'different-chrome nil
    '(450 . 210) '(920 . 760))
   :w32-ignores-toolbar-difference
   (pos-tip-frame-relative-position
    'primary 'toolbar-peer t)))
"##;
    let expect = expect![[
        r####"OK (:natural-number-bits ((-1 0 0) (0 0 0) (1 1 4) (5 1 4) (2.5 0 0) (text 0 0)) :terminal-window-system nil :invalid-frame (wrong-type-argument framep not-a-frame) :same-chrome (300 . 100) :different-chrome (-470 . -550) :w32-ignores-toolbar-difference (-480 . -520))"####
    ]];
    ParityBatchCase::value(
        "frame_geometry_normalizes_chrome_and_uses_absolute_coordinates_when_needed",
        elisp_form,
        expect,
    )
}

fn pixel_position_places_clamps_and_flips_tooltips_around_visible_text() -> ParityBatchCase {
    let elisp_form = r##"
(cl-letf
    (((symbol-function 'window-frame)
      (lambda (_window) 'release-frame))
     ((symbol-function 'pos-tip-window-system)
      (lambda (&optional _frame) 'x))
     ((symbol-function 'posn-at-point)
      (lambda (&rest _) 'release-position))
     ((symbol-function 'posn-actual-col-row)
      (lambda (_position) '(4 . 2)))
     ((symbol-function 'window-line-height)
      (lambda (&rest _) '(16 0 40 0)))
     ((symbol-function 'posn-x-y)
      (lambda (_position) '(30 . 40)))
     ((symbol-function 'window-inside-pixel-edges)
      (lambda (_window) '(10 20 310 220)))
     ((symbol-function 'window-pixel-edges)
      (lambda (_window) '(5 7 305 207)))
     ((symbol-function 'frame-pixel-width)
      (lambda (&optional _frame) 320))
     ((symbol-function 'frame-pixel-height)
      (lambda (&optional _frame) 200))
     ((symbol-function 'x-display-pixel-width)
      (lambda (&optional _frame) 1000))
     ((symbol-function 'x-display-pixel-height)
      (lambda (&optional _frame) 700)))
  (let (results)
    (dolist
        (spec
         '((:name below
            :width 100 :height 50
            :coordinates relative :dx 5 :dy nil)
           (:name clamped-above
            :width 300 :height 160
            :coordinates relative :dx 100 :dy nil)
           (:name explicit-dy-above
            :width 50 :height 180
            :coordinates relative :dx 0 :dy 25)
           (:name absolute
            :width 80 :height 100
            :coordinates (100 . 50) :dx -10 :dy nil)))
      (let ((position
             (pos-tip-compute-pixel-position
              42 'release-window
              (plist-get spec :width)
              (plist-get spec :height)
              (plist-get spec :coordinates)
              (plist-get spec :dx)
              (plist-get spec :dy))))
        (push
         (list
          :name (plist-get spec :name)
          :position position
          :upper pos-tip-upperside-p)
         results)))
    (nreverse results)))
"##;
    let expect = expect![[
        r####"OK ((:name below :position (45 . 63) :upper nil) (:name clamped-above :position (20 . 0) :upper t) (:name explicit-dy-above :position (40 . 20) :upper t) (:name absolute :position (130 . 113) :upper nil))"####
    ]];
    ParityBatchCase::value(
        "pixel_position_places_clamps_and_flips_tooltips_around_visible_text",
        elisp_form,
        expect,
    )
}

fn low_level_show_dispatches_geometry_appearance_mouse_avoidance_and_timeout() -> ParityBatchCase {
    let elisp_form = r##"
(let (shown avoided cancelled)
  (cl-letf
      (((symbol-function 'pos-tip-window-system)
        (lambda (&optional _frame) 'x))
       ((symbol-function 'pos-tip-compute-pixel-position)
        (lambda (&rest _) '(120 . 80)))
       ((symbol-function 'frame-parameter)
        (lambda (_frame parameter)
          (cdr
           (assq parameter
                 '((font . "Test Mono")
                   (line-spacing . 2))))))
       ((symbol-function 'frame-char-width)
        (lambda (&optional _frame) 8))
       ((symbol-function 'frame-char-height)
        (lambda (&optional _frame) 16))
       ((symbol-function 'mouse-pixel-position)
        (lambda ()
          (cons (selected-frame) '(10 . 20))))
       ((symbol-function 'pos-tip-avoid-mouse)
        (lambda (left right top bottom &optional frame)
          (setq avoided
                (list
                 left right top bottom
                 :selected-frame
                 (eq frame (selected-frame))))
          (cons frame '(400 . 300))))
       ((symbol-function 'x-show-tip)
        (lambda (string frame parameters timeout dx dy)
          (setq shown
                (list
                 :text (substring-no-properties string)
                 :faces
                 (pos-tip-test-property-runs string 'face)
                 :selected-frame
                 (eq frame (selected-frame))
                 :parameters parameters
                 :timeout timeout
                 :dx dx
                 :dy dy
                 :maximum x-max-tooltip-size))
          'shown))
       ((symbol-function 'pos-tip-cancel-timer)
        (lambda ()
          (setq cancelled t))))
    (let ((pos-tip-border-width 2)
          (pos-tip-internal-border-width 3)
          (window (selected-window))
          (text
           (propertize
            "Deploy staging"
            'face 'font-lock-function-name-face)))
      (list
       :position
       (pos-tip-show-no-propertize
        text '("ivory" . "navy")
        42 window 0 90 54 'relative 7 9)
       :shown shown
       :avoided avoided
       :cancelled cancelled))))
"##;
    let expect = expect![[
        r####"OK (:position (120 . 80) :shown (:text "Deploy staging" :faces ((0 14 font-lock-function-name-face)) :selected-frame t :parameters ((border-width . 2) (internal-border-width . 3) (font . "Test Mono") (line-spacing . 2) (foreground-color . "ivory") (background-color . "navy")) :timeout nil :dx -280 :dy -220 :maximum (11 . 3)) :avoided (120 210 80 134 :selected-frame t) :cancelled t)"####
    ]];
    ParityBatchCase::value(
        "low_level_show_dispatches_geometry_appearance_mouse_avoidance_and_timeout",
        elisp_form,
        expect,
    )
}

fn high_level_show_prepares_real_tooltip_text_for_fill_truncate_and_passthrough() -> ParityBatchCase
{
    let elisp_form = r##"
(let (calls)
  (cl-letf
      (((symbol-function 'pos-tip-x-display-width)
        (lambda (&optional _frame) 20))
       ((symbol-function 'pos-tip-x-display-height)
        (lambda (&optional _frame) 3))
       ((symbol-function 'frame-char-width)
        (lambda (&optional _frame) 8))
       ((symbol-function 'frame-char-height)
        (lambda (&optional _frame) 16))
       ((symbol-function 'face-attribute)
        (lambda (_face attribute &rest _)
          (if (eq attribute :font)
              "Test Mono"
            'unspecified)))
       ((symbol-function 'default-value)
        (lambda (_symbol) nil))
       ((symbol-function 'frame-parameter)
        (lambda (_frame _parameter) nil))
       ((symbol-function 'pos-tip-show-no-propertize)
        (lambda
          (string tip-color pos window timeout
                  pixel-width pixel-height
                  frame-coordinates dx dy)
          (push
           (list
            :text (substring-no-properties string)
            :face (get-text-property 0 'face string)
            :color tip-color
            :position pos
            :same-window (eq window (selected-window))
            :timeout timeout
            :pixels (cons pixel-width pixel-height)
            :coordinates frame-coordinates
            :offsets (list dx dy))
           calls)
          (cons 10 20))))
    (let ((pos-tip-border-width 1)
          (pos-tip-internal-border-width 2))
      (list
       :returns
       (list
        (pos-tip-show
         "Deploy production after every validation gate."
         '("ivory" . "navy")
         12 (selected-window) 9 12 'relative 3 4)
        (pos-tip-show
         "deploy-production-service\n状态检查完成\nrollback-ready\nfourth"
         '("white" . "red")
         7 (selected-window) 5 nil 'relative nil nil)
        (pos-tip-show
         "Ready"
         '("black" . "gold")
         2 (selected-window) nil nil 'relative 0 0))
       :calls (nreverse calls)))))
"##;
    let expect = expect![[
        r####"OK (:returns ((10 . 20) (10 . 20) (10 . 20)) :calls ((:text "Deploy\nproduction\nafter every" :face (:font "Test Mono" :foreground "ivory" :background "navy") :color ("ivory" . "navy") :position 12 :same-window t :timeout 9 :pixels (94 . 54) :coordinates relative :offsets (3 4)) (:text "deploy-production-se\n状态检查完成\nrollback-ready" :face (:font "Test Mono" :foreground "white" :background "red") :color ("white" . "red") :position 7 :same-window t :timeout 5 :pixels (166 . 54) :coordinates relative :offsets (nil nil)) (:text "Ready" :face (:font "Test Mono" :foreground "black" :background "gold") :color ("black" . "gold") :position 2 :same-window t :timeout nil :pixels (46 . 22) :coordinates relative :offsets (0 0))))"####
    ]];
    ParityBatchCase::value(
        "high_level_show_prepares_real_tooltip_text_for_fill_truncate_and_passthrough",
        elisp_form,
        expect,
    )
}

#[test]
fn pos_tip_package_batch() {
    let cases = vec![
        release_notes_are_split_filled_and_truncated_by_display_columns(),
        tooltip_dimensions_account_for_tabs_wide_text_borders_and_line_spacing(),
        release_tooltip_colors_follow_face_pair_customization_and_fallback_precedence(),
        frame_geometry_normalizes_chrome_and_uses_absolute_coordinates_when_needed(),
        pixel_position_places_clamps_and_flips_tooltips_around_visible_text(),
        low_level_show_dispatches_geometry_appearance_mouse_avoidance_and_timeout(),
        high_level_show_prepares_real_tooltip_text_for_fill_truncate_and_passthrough(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed pos-tip parity test");
    assert_oracle_batch_cases(pos_tip_oracle(), test_name, "pos_tip_parity", &cases);
}
