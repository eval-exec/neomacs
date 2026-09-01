use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, POWERLINE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const POWERLINE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const POWERLINE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'powerline)

(defvar powerline-test-process " cargo-check")

(defun powerline-test-property-runs (string property)
  (let ((position 0)
        runs)
    (while (< position (length string))
      (let* ((value
              (get-text-property position property string))
             (next
              (or
               (next-single-property-change
                position property string)
               (length string))))
        (when value
          (push
           (list position next (copy-tree value))
           runs))
        (setq position next)))
    (nreverse runs)))

(defun powerline-test-text-summary (string)
  (list
   :text (substring-no-properties string)
   :width (string-width string)
   :faces (powerline-test-property-runs string 'face)
   :mouse-faces
   (powerline-test-property-runs string 'mouse-face)
   :help
   (powerline-test-property-runs string 'help-echo)
   :display
   (powerline-test-property-runs string 'display)))

(defun powerline-test-key-binding (string event)
  (let ((position 0)
        map)
    (while (and (< position (length string))
                (not (keymapp map)))
      (setq map
            (get-text-property
             position 'local-map string)
            position
            (or
             (next-single-property-change
              position 'local-map string)
             (length string))))
    (and (keymapp map)
         (lookup-key map event))))
"##;

fn powerline_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(POWERLINE_MELPA_PIN, "powerline.el")
        .expect("prepare pinned powerline source below ./tmp")
        .with_prelude(POWERLINE_TEST_PRELUDE)
        .with_timeout(POWERLINE_TEST_TIMEOUT)
}

fn deployment_status_line_preserves_face_runs_renders_images_and_measures_columns()
-> ParityBatchCase {
    let elisp_form = r##"
(cl-letf
    (((symbol-function 'format-mode-line)
      (lambda (value &rest _)
        (if (stringp value)
            value
          (format "%s" value))))
     ((symbol-function 'image-size)
      (lambda (_image &optional _pixels _frame)
        '(4 . 1))))
  (let* ((build
          (propertize
           "BUILD" 'face 'bold
           'help-echo "Open build log"))
         (left
          (powerline-raw build 'status-face 'r))
         (separator
          '(image :type xpm
                  :face separator-face
                  :name deploy-arrow))
         (right
          (powerline-raw "任务" 'active-face 'l))
         (values (list left separator right nil))
         (rendered (powerline-render values)))
    (list
     :left (powerline-test-text-summary left)
     :rendered (powerline-test-text-summary rendered)
     :measured-columns (powerline-width values)
     :source
     (powerline-test-text-summary build))))
"##;
    let expect = expect![[
        r####"OK (:left (:text "BUILD " :width 6 :faces ((0 5 (bold status-face)) (5 6 (status-face))) :mouse-faces nil :help ((0 5 "Open build log")) :display nil) :rendered (:text "BUILD   任务" :width 12 :faces ((0 5 (bold status-face)) (5 6 (status-face)) (6 7 separator-face) (7 10 (active-face))) :mouse-faces nil :help ((0 5 "Open build log")) :display ((6 7 (image :type xpm :face separator-face :name deploy-arrow)))) :measured-columns 15 :source (:text "BUILD" :width 5 :faces ((0 5 bold)) :mouse-faces nil :help ((0 5 "Open build log")) :display nil))"####
    ]];
    ParityBatchCase::value(
        "deployment_status_line_preserves_face_runs_renders_images_and_measures_columns",
        elisp_form,
        expect,
    )
}

fn operational_segments_report_modes_narrowing_encoding_process_vc_and_click_actions()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert (make-string 100 ?x))
  (narrow-to-region 11 31)
  (setq mode-name "Release"
        minor-mode-alist
        '((lint-mode " Lint")
          (sync-mode " Sync"))
        mode-line-process 'powerline-test-process
        powerline-test-process " cargo-check"
        buffer-file-coding-system 'utf-8-unix
        buffer-file-name "/workspace/release.rs"
        vc-mode " Git:main"
        powerline-narrowed-indicator "Scoped"
        powerline-buffer-size-suffix t)
  (let ((forced 0))
    (cl-letf
        (((symbol-function 'format-mode-line)
          (lambda (value &rest _)
            (cond
             ((equal value minor-mode-alist) "Lint Sync")
             ((equal value '(vc-mode vc-mode)) " Git:main")
             ((stringp value) value)
             ((symbolp value)
              (format "%s" (symbol-value value)))
             (t (format "%s" value)))))
         ((symbol-function 'force-mode-line-update)
          (lambda (&rest _)
            (setq forced (1+ forced)))))
      (let* ((major
              (powerline-major-mode 'major-face 'l))
             (minor
              (powerline-minor-modes 'minor-face 'r))
             (narrow
              (powerline-narrow 'scope-face 'l))
             (encoding
              (powerline-encoding 'encoding-face))
             (process
              (powerline-process 'process-face 'l))
             (vc
              (let ((window-system nil))
                (powerline-vc 'vc-face 'r)))
             (size-before
              (powerline-buffer-size 'size-face 'l))
             (size-action
              (powerline-test-key-binding
               size-before
               [mode-line mouse-1])))
        (funcall size-action)
        (let ((size-after
               (powerline-buffer-size 'size-face 'l)))
          (list
           :segments
           (mapcar
            #'powerline-test-text-summary
            (list major minor narrow encoding
                  process vc size-before size-after))
           :major-help
           (get-text-property 1 'help-echo major)
           :major-mouse-2
           (powerline-test-key-binding
            major [mode-line mouse-2])
           :narrow-mouse-1
           (powerline-test-key-binding
            narrow [mode-line mouse-1])
           :size-format-toggle
           (list
            (substring-no-properties size-before)
            (substring-no-properties size-after)
            powerline-buffer-size-suffix
            forced)
           :combined
           (powerline-concat
            (substring-no-properties major)
            (substring-no-properties narrow)
            (substring-no-properties vc))))))))
"##;
    let expect = expect![[
        r####"OK (:segments ((:text " Release" :width 8 :faces ((0 8 (major-face))) :mouse-faces ((1 8 mode-line-highlight)) :help ((1 8 "Major mode\nmouse-1: Display major mode menu\nmouse-2: Show help for major mode\nmouse-3: Toggle minor modes")) :display nil) (:text "Lint Sync " :width 10 :faces ((0 4 (minor-face)) (4 5 (minor-face minor-face)) (5 10 (minor-face))) :mouse-faces ((0 4 mode-line-highlight) (5 9 mode-line-highlight)) :help ((0 4 "Minor mode\n mouse-1: Display minor mode menu\n mouse-2: Show help for minor mode\n mouse-3: Toggle minor modes") (5 9 "Minor mode\n mouse-1: Display minor mode menu\n mouse-2: Show help for minor mode\n mouse-3: Toggle minor modes")) :display nil) (:text " Scoped" :width 7 :faces ((0 7 (scope-face))) :mouse-faces ((1 7 mode-line-highlight)) :help ((1 7 "mouse-1: Remove narrowing from the current buffer")) :display nil) (:text "unix" :width 4 :faces ((0 4 (encoding-face))) :mouse-faces nil :help nil :display nil) (:text "  cargo-check" :width 13 :faces ((0 13 (process-face))) :mouse-faces nil :help nil :display nil) (:text "  Git:main " :width 12 :faces ((0 12 (vc-face))) :mouse-faces nil :help nil :display nil) (:text " %I" :width 3 :faces ((0 3 (size-face))) :mouse-faces ((1 3 mode-line-highlight)) :help nil :display nil) (:text " %i" :width 3 :faces ((0 3 (size-face))) :mouse-faces ((1 3 mode-line-highlight)) :help nil :display nil)) :major-help "Major mode\nmouse-1: Display major mode menu\nmouse-2: Show help for major mode\nmouse-3: Toggle minor modes" :major-mouse-2 describe-mode :narrow-mouse-1 mode-line-widen :size-format-toggle (" %I" " %i" nil 1) :combined "  Release  Scoped   Git:main  ")"####
    ]];
    ParityBatchCase::value(
        "operational_segments_report_modes_narrowing_encoding_process_vc_and_click_actions",
        elisp_form,
        expect,
    )
}

fn frame_local_memoization_reuses_truthy_results_and_cache_lifecycle_is_explicit() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((cache nil)
      (truthy-calls 0)
      (nil-calls 0)
      frame-updates
      deleted)
  (cl-letf
      (((symbol-function 'frame-parameter)
        (lambda (_frame parameter)
          (and (eq parameter 'powerline-cache)
               cache)))
       ((symbol-function 'modify-frame-parameters)
        (lambda (frame parameters)
          (setq cache
                (cdr (assq 'powerline-cache parameters)))
          (push
           (list frame
                 (hash-table-p cache)
                 (hash-table-count cache))
           frame-updates)))
       ((symbol-function 'set-frame-parameter)
        (lambda (frame parameter value)
          (when (eq parameter 'powerline-cache)
            (setq cache value)
            (push (list frame value) deleted)))))
    (let ((truthy
           (pl/memoize-wrap-frame-local
            (lambda (environment)
              (setq truthy-calls (1+ truthy-calls))
              (list :environment environment
                    :sequence truthy-calls))))
          (falsey
           (pl/memoize-wrap-frame-local
            (lambda (_environment)
              (setq nil-calls (1+ nil-calls))
              nil))))
      (let ((first (funcall truthy "staging"))
            (second (funcall truthy "staging"))
            (other (funcall truthy "production")))
        (funcall falsey "staging")
        (funcall falsey "staging")
        (let ((before-delete
               (and cache (hash-table-count cache))))
          (powerline-delete-cache 'release-frame)
          (list
           :results
           (list
            (copy-tree first)
            (copy-tree second)
            (copy-tree other))
           :same-object (eq first second)
           :calls
           (list :truthy truthy-calls
                 :nil nil-calls)
           :cache-before-delete before-delete
           :frame-updates (nreverse frame-updates)
           :deleted (nreverse deleted)
           :cache-after-delete cache))))))
"##;
    let expect = expect![[
        r####"OK (:results ((:environment "staging" :sequence 1) (:environment "staging" :sequence 1) (:environment "production" :sequence 2)) :same-object t :calls (:truthy 2 :nil 2) :cache-before-delete 3 :frame-updates ((nil t 0)) :deleted ((release-frame nil)) :cache-after-delete nil)"####
    ]];
    ParityBatchCase::value(
        "frame_local_memoization_reuses_truthy_results_and_cache_lifecycle_is_explicit",
        elisp_form,
        expect,
    )
}

fn alignment_spacers_scale_reservations_and_terminal_separator_selection() -> ParityBatchCase {
    let elisp_form = r##"
(cl-letf
    (((symbol-function 'get-scroll-bar-mode)
      (lambda () 'right))
     ((symbol-function 'frame-char-height)
      (lambda (&optional _frame) 19)))
  (let* ((window-system t)
         (powerline-default-separator 'wave)
         (powerline-text-scale-factor 1.5)
         (powerline-height nil)
         (right (powerline-fill 'right-face 12))
         (default-reserve (powerline-fill 'default-face nil))
         (center (powerline-fill-center 'center-face 10))
         (gui-separator (powerline-current-separator))
         (gui-height (pl/separator-height))
         (terminal-separator
          (let ((window-system nil))
            (powerline-current-separator)))
         (fixed-height
          (let ((powerline-height 27))
            (pl/separator-height))))
    (list
     :right (powerline-test-text-summary right)
     :default-reserve
     (powerline-test-text-summary default-reserve)
     :center (powerline-test-text-summary center)
     :separator
     (list :gui gui-separator
           :terminal terminal-separator)
     :height
     (list :gui gui-height :fixed fixed-height))))
"##;
    let expect = expect![[
        r####"OK (:right (:text " " :width 1 :faces ((0 1 right-face)) :mouse-faces nil :help nil :display ((0 1 ((space :align-to (- (+ right right-fringe right-margin) 15.0)))))) :default-reserve (:text " " :width 1 :faces ((0 1 default-face)) :mouse-faces nil :help nil :display ((0 1 ((space :align-to (- (+ right right-fringe right-margin) 27.0)))))) :center (:text " " :width 1 :faces ((0 1 center-face)) :mouse-faces nil :help nil :display ((0 1 ((space :align-to (- (+ center (0.5 . right-margin)) 15.0 (0.5 . left-margin))))))) :separator (:gui wave :terminal utf-8) :height (:gui 19 :fixed 27))"####
    ]];
    ParityBatchCase::value(
        "alignment_spacers_scale_reservations_and_terminal_separator_selection",
        elisp_form,
        expect,
    )
}

fn graphical_separators_generate_directional_xpm_once_and_utf8_fallback_carries_colors()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((window-system t)
      (cache nil)
      created)
  (cl-letf
      (((symbol-function 'frame-parameter)
        (lambda (_frame parameter)
          (and (eq parameter 'powerline-cache)
               cache)))
       ((symbol-function 'modify-frame-parameters)
        (lambda (_frame parameters)
          (setq cache
                (cdr (assq 'powerline-cache parameters)))))
       ((symbol-function 'pl/background-color)
        (lambda (face)
          (pcase face
            ('source-face "#112233")
            ('target-face "#ddeeff")
            (_ nil))))
       ((symbol-function 'pl/hex-color)
        (lambda (color) color))
       ((symbol-function 'pl/interpolate)
        (lambda (_left _right) "#778899"))
       ((symbol-function 'create-image)
        (lambda (data type data-p &rest properties)
          (let ((image
                 (list 'image
                       :id (1+ (length created))
                       :type type
                       :data-p data-p
                       :data data
                       :properties properties)))
            (push image created)
            image))))
    (pl/reset-cache)
    (let* ((left
            (powerline-arrow-left
             'source-face 'target-face 5))
           (left-again
            (powerline-arrow-left
             'source-face 'target-face 5))
           (right
            (powerline-arrow-right
             'source-face 'target-face 5))
           (utf8
            (powerline-utf-8-left
             'source-face 'target-face)))
      (list
       :rows
       (list
        (pl/row-pattern 3 7 2)
        (pl/row-pattern 20 4 3)
        (pl/reverse-pattern
         '((0 1 2) (2 1 0))))
       :images (reverse created)
       :memoized
       (list
        :same-object (eq left left-again)
        :create-count (length created)
        :cache-size (hash-table-count cache)
        :directions
        (list
         (plist-get (cdr left) :id)
         (plist-get (cdr right) :id)))
       :utf8
       (list
        :character (string-to-char utf8)
        :text (substring-no-properties utf8)
        :face (get-text-property 0 'face utf8))))))
"##;
    let expect = expect![[
        r####"OK (:rows ((0 0 0 2 2 1 1) (0 0 0 0) ((2 1 0) (0 1 2))) :images ((image :id 1 :type xpm :data-p t :data "/* XPM */ static char * arrow_left[] = { \"2 5 3 1\", \"0 c #112233\", \"1 c #ddeeff\", \"2 c #778899\",\"11\",\"01\",\"00\",\"01\",\"11\",};" :properties (:ascent center :scale 1 :face target-face)) (image :id 2 :type xpm :data-p t :data "/* XPM */ static char * arrow_right[] = { \"2 5 3 1\", \"0 c #ddeeff\", \"1 c #112233\", \"2 c #778899\",\"11\",\"10\",\"00\",\"10\",\"11\",};" :properties (:ascent center :scale 1 :face source-face))) :memoized (:same-object t :create-count 2 :cache-size 2 :directions (1 2)) :utf8 (:character 57520 :text "" :face ((:foreground "#112233" :background "#ddeeff" :inverse-video nil))))"####
    ]];
    ParityBatchCase::value(
        "graphical_separators_generate_directional_xpm_once_and_utf8_fallback_carries_colors",
        elisp_form,
        expect,
    )
}

fn scroll_hud_uses_widened_buffer_ranges_character_pixels_and_memoized_percentages()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert (make-string 1000 ?x))
  (narrow-to-region 201 401)
  (let ((cache nil)
        (window-range '(251 . 351))
        generated)
    (cl-letf
        (((symbol-function 'frame-parameter)
          (lambda (_frame parameter)
            (and (eq parameter 'powerline-cache)
                 cache)))
         ((symbol-function 'modify-frame-parameters)
          (lambda (_frame parameters)
            (setq cache
                  (cdr (assq 'powerline-cache parameters)))))
         ((symbol-function 'window-start)
          (lambda (&optional _window)
            (car window-range)))
         ((symbol-function 'window-end)
          (lambda (&optional _window _update)
            (cdr window-range)))
         ((symbol-function 'frame-char-height)
          (lambda (&optional _frame) 9))
         ((symbol-function 'frame-char-width)
          (lambda (&optional _frame) 7))
         ((symbol-function 'face-background)
          (lambda (face &optional _frame _inherit)
            (if (eq face 'viewport-face)
                "#33aa66"
              "#202020")))
         ((symbol-function 'pl/make-xpm)
          (lambda (name color1 color2 data)
            (let ((result
                   (list
                    :name name
                    :colors (list color1 color2)
                    :dimensions
                    (cons (length (car data))
                          (length data))
                    :rows
                    (mapcar #'car data))))
              (push result generated)
              result))))
      (pl/reset-cache)
      (let ((first
             (powerline-hud
              'viewport-face 'track-face 2))
            (same
             (powerline-hud
              'viewport-face 'track-face 2)))
        (setq window-range '(701 . 901))
        (let ((later
               (powerline-hud
                'viewport-face 'track-face 2)))
          (list
           :first (copy-tree first)
           :same-object (eq first same)
           :later (copy-tree later)
           :generator-calls
           (mapcar #'copy-tree
                   (nreverse generated))
           :cache-size (hash-table-count cache)
           :restriction
           (list (point-min) (point-max)
                 (buffer-size))))))))
"##;
    let expect = expect![[
        r####"OK (:first (:name "percent" :colors ("#33aa66" "#202020") :dimensions (14 . 9) :rows (0 0 1 1 0 0 0 0 0)) :same-object t :later (:name "percent" :colors ("#33aa66" "#202020") :dimensions (14 . 9) :rows (0 0 0 0 0 0 1 1 0)) :generator-calls ((:name "percent" :colors ("#33aa66" "#202020") :dimensions (14 . 9) :rows (0 0 1 1 0 0 0 0 0)) (:name "percent" :colors ("#33aa66" "#202020") :dimensions (14 . 9) :rows (0 0 0 0 0 0 1 1 0))) :cache-size 2 :restriction (201 401 1000))"####
    ]];
    ParityBatchCase::value(
        "scroll_hud_uses_widened_buffer_ranges_character_pixels_and_memoized_percentages",
        elisp_form,
        expect,
    )
}

fn window_selection_minibuffer_stack_and_mouse_handlers_follow_interactive_lifecycle()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((selected 'window-a)
      (frame-selected 'window-a)
      (minibuffer-active nil)
      (minibuffer-origin 'editor-window)
      (powerline-selected-window nil)
      (pl/minibuffer-selected-window-list nil)
      forced
      actions)
  (cl-letf
      (((symbol-function 'selected-window)
        (lambda () selected))
       ((symbol-function 'frame-selected-window)
        (lambda (&optional _frame)
          frame-selected))
       ((symbol-function 'minibuffer-window-active-p)
        (lambda (_window) minibuffer-active))
       ((symbol-function 'minibuffer-selected-window)
        (lambda () minibuffer-origin))
       ((symbol-function 'force-mode-line-update)
        (lambda (&optional all)
          (push all forced)))
       ((symbol-function 'minor-mode-menu-from-indicator)
        (lambda (indicator)
          (push (list 'menu indicator) actions)))
       ((symbol-function 'describe-minor-mode-from-indicator)
        (lambda (indicator)
          (push (list 'help indicator) actions))))
    (powerline-set-selected-window)
    (let ((initial
           (powerline-selected-window-active)))
      (setq selected 'window-b
            frame-selected 'window-b
            minibuffer-active t)
      (powerline-set-selected-window)
      (let ((during-minibuffer
             (list powerline-selected-window
                   (powerline-selected-window-active))))
        (setq minibuffer-origin 'prompt-parent)
        (pl/minibuffer-setup)
        (setq minibuffer-origin 'recursive-parent)
        (pl/minibuffer-setup)
        (let ((stack-top
               (pl/minibuffer-selected-window)))
          (pl/minibuffer-exit)
          (let ((stack-after-exit
                 (pl/minibuffer-selected-window)))
            (pl/minibuffer-exit)
            (funcall
             (eval
              (powerline-mouse
               'minor 'menu " Lint")
              t)
             'event)
            (funcall
             (eval
              (powerline-mouse
               'minor 'help " Sync")
              t)
             'event)
            (let ((ignored
                   (funcall
                    (eval
                     (powerline-mouse
                      'major 'menu "Release")
                     t)
                    'event)))
              (powerline-unset-selected-window)
              (list
               :active-initially initial
               :during-minibuffer
               during-minibuffer
               :stack
               (list stack-top
                     stack-after-exit
                     (pl/minibuffer-selected-window))
               :mouse-actions (nreverse actions)
               :ignored-mouse ignored
               :forced (nreverse forced)
               :after-unset
               powerline-selected-window))))))))
"##;
    let expect = expect![[
        r####"OK (:active-initially t :during-minibuffer (window-a nil) :stack (recursive-parent prompt-parent nil) :mouse-actions ((menu " Lint") (help " Sync")) :ignored-mouse nil :forced (nil nil) :after-unset nil)"####
    ]];
    ParityBatchCase::value(
        "window_selection_minibuffer_stack_and_mouse_handlers_follow_interactive_lifecycle",
        elisp_form,
        expect,
    )
}

fn nano_theme_installs_a_live_centered_mode_line_and_revert_restores_the_saved_default()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((original (default-value 'mode-line-format))
      (pl/default-mode-line '("ORIGINAL-MODE-LINE")))
  (unwind-protect
      (with-temp-buffer
        (rename-buffer "release-dashboard" t)
        (insert "changed")
        (set-buffer-modified-p t)
        (cl-letf
            (((symbol-function
               'powerline-selected-window-active)
              (lambda () t))
             ((symbol-function 'powerline-raw)
              (lambda (value &optional face pad)
                (let ((text
                       (cond
                        ((equal value "%b")
                         (buffer-name))
                        ((and
                          (stringp value)
                          (string-prefix-p
                           "GNU Emacs " value))
                         "GNU Emacs VERSION")
                        (t value))))
                  (format
                   "<raw:%s:%s:%s>"
                   text face pad))))
             ((symbol-function 'powerline-width)
              (lambda (values)
                (apply
                 #'+
                 (mapcar #'string-width values))))
             ((symbol-function 'powerline-render)
              (lambda (values)
                (mapconcat #'identity values "")))
             ((symbol-function 'powerline-fill-center)
              (lambda (face reserve)
                (format
                 "<center:%s:%s>" face reserve)))
             ((symbol-function 'powerline-fill)
              (lambda (face reserve)
                (format
                 "<fill:%s:%s>" face reserve))))
          (powerline-nano-theme)
          (let* ((installed
                  (default-value 'mode-line-format))
                 (evaluation
                  (cadr (cadr installed)))
                 (rendered (eval evaluation t)))
            (powerline-revert)
            (list
             :installed-prefix (car installed)
             :installed-evaluator
             (car (cadr installed))
             :rendered rendered
             :restored
             (default-value 'mode-line-format)))))
    (setq-default mode-line-format original)))
"##;
    let expect = expect![[
        r####"OK (:installed-prefix "%e" :installed-evaluator :eval :rendered "<raw:GNU Emacs VERSION:powerline-active0:l><center:powerline-active0:22.5><raw:release-dashboard:powerline-active0:nil><fill:powerline-active0:60><raw:Modified:powerline-active0:r><fill:powerline-active0:0>" :restored ("ORIGINAL-MODE-LINE"))"####
    ]];
    ParityBatchCase::value(
        "nano_theme_installs_a_live_centered_mode_line_and_revert_restores_the_saved_default",
        elisp_form,
        expect,
    )
}

#[test]
fn powerline_package_batch() {
    let cases = vec![
        deployment_status_line_preserves_face_runs_renders_images_and_measures_columns(),
        operational_segments_report_modes_narrowing_encoding_process_vc_and_click_actions(),
        frame_local_memoization_reuses_truthy_results_and_cache_lifecycle_is_explicit(),
        alignment_spacers_scale_reservations_and_terminal_separator_selection(),
        graphical_separators_generate_directional_xpm_once_and_utf8_fallback_carries_colors(),
        scroll_hud_uses_widened_buffer_ranges_character_pixels_and_memoized_percentages(),
        window_selection_minibuffer_stack_and_mouse_handlers_follow_interactive_lifecycle(),
        nano_theme_installs_a_live_centered_mode_line_and_revert_restores_the_saved_default(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed powerline parity test");
    assert_oracle_batch_cases(powerline_oracle(), test_name, "powerline_parity", &cases);
}
