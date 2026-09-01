use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, XTERM_COLOR_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const XTERM_COLOR_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const XTERM_COLOR_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'xterm-color)

(defun neomacs-xterm-color-test-reset ()
  "Reset the buffer-local parser and cache to its documented defaults."
  (setq-local xterm-color-preserve-properties nil
              xterm-color-render t
              xterm-color--current-fg nil
              xterm-color--current-bg nil
              xterm-color--char-list nil
              xterm-color--CSI-list nil
              xterm-color--state :char
              xterm-color--attributes 0
              xterm-color--face-cache nil
              xterm-color--truecolor-face-cache nil))

(defun neomacs-xterm-color-test-state ()
  "Describe the buffer-local stream and cache state."
  (list
   :state xterm-color--state
   :foreground xterm-color--current-fg
   :background xterm-color--current-bg
   :attributes xterm-color--attributes
   :pending-chars (reverse (copy-sequence xterm-color--char-list))
   :pending-csi (reverse (copy-sequence xterm-color--CSI-list))
   :face-cache
   (and xterm-color--face-cache
        (hash-table-count xterm-color--face-cache))
   :truecolor-cache
   (and xterm-color--truecolor-face-cache
        (hash-table-count xterm-color--truecolor-face-cache))))

(defun neomacs-xterm-color-test-string-runs (string)
  "Return every property run in STRING with its exact visible text."
  (let ((position 0)
        (limit (length string))
        runs)
    (while (< position limit)
      (let ((next (next-property-change position string limit)))
        (push
         (list
          (substring-no-properties string position next)
          (text-properties-at position string))
         runs)
        (setq position next)))
    (nreverse runs)))

(defun neomacs-xterm-color-test-buffer-runs ()
  "Return every accessible buffer property run with exact visible text."
  (let ((position (point-min))
        (limit (point-max))
        runs)
    (while (< position limit)
      (let ((next (next-property-change position nil limit)))
        (push
         (list
          (buffer-substring-no-properties position next)
          (text-properties-at position))
         runs)
        (setq position next)))
    (nreverse runs)))

(defun neomacs-xterm-color-test-overlays ()
  "Describe xterm-color overlays in stable buffer order."
  (mapcar
   (lambda (overlay)
     (list
      (overlay-start overlay)
      (overlay-end overlay)
      (overlay-get overlay 'face)
      (overlay-get overlay 'font-lock-face)
      (overlay-get overlay 'xterm-color)
      (overlay-get overlay 'evaporate)
      (buffer-substring-no-properties
       (overlay-start overlay)
       (overlay-end overlay))))
   (sort (overlays-in (point-min) (point-max))
         (lambda (left right)
           (let ((left-start (overlay-start left))
                 (right-start (overlay-start right)))
             (if (= left-start right-start)
                 (< (overlay-end left) (overlay-end right))
               (< left-start right-start)))))))
"##;

fn xterm_color_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(XTERM_COLOR_MELPA_PIN, "xterm-color.el")
        .expect("prepare exact xterm-color source below ./tmp")
        .with_prelude(XTERM_COLOR_TEST_PRELUDE)
        .with_timeout(XTERM_COLOR_TEST_TIMEOUT)
}

fn streamed_build_log_reassembles_split_csi_and_carries_style() -> ParityBatchCase {
    let elisp_form = r##"
(let ((chunks
       '("compile: \e[3"
         "1;1mERROR"
         ": missing crate"
         "\e[39m; retry \e[4munderlined\e[24m done\e[0m\n"))
      outputs first-state final-state)
  (with-temp-buffer
    (neomacs-xterm-color-test-reset)
    (cl-loop
     for chunk in chunks
     for index from 0 do
     (let ((output (xterm-color-filter chunk)))
       (push (substring-no-properties output) outputs)
       (insert output)
       (when (= index 0)
         (setq first-state (neomacs-xterm-color-test-state)))))
    (setq final-state (neomacs-xterm-color-test-state))
    (list
     :chunks (nreverse outputs)
     :first-state first-state
     :runs (neomacs-xterm-color-test-buffer-runs)
     :text (buffer-substring-no-properties (point-min) (point-max))
     :point (point)
     :final-state final-state)))
"##;
    let expect = expect![[
        r##"OK (:chunks ("compile: " "ERROR" ": missing crate" "; retry underlined done\n") :first-state (:state :ansi-csi :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi (51) :face-cache 0 :truecolor-cache 0) :runs (("compile: " nil) ("ERROR: missing crate" (xterm-color t face (:foreground "#EC6261"))) ("; retry " (xterm-color t face nil)) ("underlined" (xterm-color t face (:underline t))) (" done" (xterm-color t face nil)) ("\n" nil)) :text "compile: ERROR: missing crate; retry underlined done\n" :point 54 :final-state (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 3 :truecolor-cache 0))"##
    ]];
    ParityBatchCase::value(
        "streamed_build_log_reassembles_split_csi_and_carries_style",
        elisp_form,
        expect,
    )
}

fn interleaved_process_buffers_keep_parser_color_and_cache_state_independent() -> ParityBatchCase {
    let elisp_form = r##"
(let ((left (generate-new-buffer " *xterm-color-worker-a*"))
      (right (generate-new-buffer " *xterm-color-worker-b*"))
      result)
  (unwind-protect
      (progn
        (dolist (buffer (list left right))
          (with-current-buffer buffer
            (neomacs-xterm-color-test-reset)))
        (with-current-buffer left
          (insert (xterm-color-filter "worker-a: \e[38;5;196mFAIL")))
        (with-current-buffer right
          (insert (xterm-color-filter "worker-b: \e[38;2;12;34;56mRUN")))
        (with-current-buffer left
          (insert (xterm-color-filter " still red\e[0m end")))
        (with-current-buffer right
          (insert (xterm-color-filter "NING\e[0m done")))
        (setq result
              (list
               :left
               (with-current-buffer left
                 (list
                  :runs (neomacs-xterm-color-test-buffer-runs)
                  :text (buffer-substring-no-properties (point-min) (point-max))
                  :state (neomacs-xterm-color-test-state)))
               :right
               (with-current-buffer right
                 (list
                  :runs (neomacs-xterm-color-test-buffer-runs)
                  :text (buffer-substring-no-properties (point-min) (point-max))
                  :state (neomacs-xterm-color-test-state))))))
    (dolist (buffer (list left right))
      (when (buffer-live-p buffer)
        (kill-buffer buffer))))
  result)
"##;
    let expect = expect![[
        r##"OK (:left (:runs (("worker-a: " nil) ("FAIL still red" (xterm-color t face (:foreground "#ff0000"))) (" end" nil)) :text "worker-a: FAIL still red end" :state (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 1 :truecolor-cache 0)) :right (:runs (("worker-b: " nil) ("RUNNING" (xterm-color t face (:foreground "#0c2238"))) (" done" nil)) :text "worker-b: RUNNING done" :state (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 0 :truecolor-cache 1)))"##
    ]];
    ParityBatchCase::value(
        "interleaved_process_buffers_keep_parser_color_and_cache_state_independent",
        elisp_form,
        expect,
    )
}

fn terminal_report_combines_attributes_colors_resets_and_render_policy() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (neomacs-xterm-color-test-reset)
  (let* ((transcript
          (concat
           "\e[3;4;9;51;53;31;44mdecorated"
           "\e[23;24;29;54;55mcolors-only"
           "\e[7mreversed\e[27mnormal"
           "\e[39mbackground\e[49mplain"
           "\e[94mbright-blue\e[0m"
           "\e[34;1mblue-bright\e[39mbright-no-foreground"
           "\e[34mblue-bright-after-39\e[0m"
           "\e[94mAIXTERM-blue\e[34mnormal-blue-after-94\e[0m"
           "\e[101mAIXTERM-red-background\e[0m"
           "\e[38;5;202;48;5;236m256\e[0m"
           "\e[38;2;12;34;56;48;2;210;220;230mtrue\e[m"))
         (rendered (xterm-color-filter transcript))
         hidden visible)
    (setq-local xterm-color-render nil)
    (setq hidden (xterm-color-filter "\e[32;4mhidden-style"))
    (let ((hidden-state (neomacs-xterm-color-test-state)))
      (setq-local xterm-color-render t)
      (setq visible (xterm-color-filter "visible\e[0m"))
      (list
       :rendered
       (list
        :text (substring-no-properties rendered)
        :runs (neomacs-xterm-color-test-string-runs rendered))
       :render-off
       (list
        :text (substring-no-properties hidden)
        :runs (neomacs-xterm-color-test-string-runs hidden)
        :state hidden-state)
       :render-resumed
       (list
        :text (substring-no-properties visible)
        :runs (neomacs-xterm-color-test-string-runs visible))
       :final-state (neomacs-xterm-color-test-state)))))
"##;
    let expect = expect![[
        r##"OK (:rendered (:text "decoratedcolors-onlyreversednormalbackgroundplainbright-blueblue-brightbright-no-foregroundblue-bright-after-39AIXTERM-bluenormal-blue-after-94AIXTERM-red-background256true" :runs (("decorated" (xterm-color t face (:slant italic :underline t :strike-through t :overline t :box t :foreground "#A93F43" :background "#4068A3"))) ("colors-only" (xterm-color t face #1=(:foreground "#A93F43" :background "#4068A3"))) ("reversed" (xterm-color t face (:inverse-video t :foreground "#A93F43" :background "#4068A3"))) ("normal" (xterm-color t face #1#)) ("background" (xterm-color t face (:background "#4068A3"))) ("plain" nil) ("bright-blue" (xterm-color t face #3=(:foreground "#63B4F6"))) ("blue-bright" (xterm-color t face #2=(:foreground "#63B4F6"))) ("bright-no-foreground" (xterm-color t face nil)) ("blue-bright-after-39" (xterm-color t face #2#)) ("AIXTERM-blue" (xterm-color t face #3#)) ("normal-blue-after-94" (xterm-color t face (:foreground "#4068A3"))) ("AIXTERM-red-background" (xterm-color t face (:background "#EC6261"))) ("256" (xterm-color t face (:foreground "#ff5f00" :background "#303030"))) ("true" (xterm-color t face (:foreground "#0c2238" :background "#d2dce6"))))) :render-off (:text "hidden-style" :runs (("hidden-style" nil)) :state (:state :char :foreground 2 :background nil :attributes 4 :pending-chars nil :pending-csi nil :face-cache 10 :truecolor-cache 1)) :render-resumed (:text "visible" :runs (("visible" (xterm-color t face (:underline t :foreground "#59963A"))))) :final-state (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 11 :truecolor-cache 1))"##
    ]];
    ParityBatchCase::value(
        "terminal_report_combines_attributes_colors_resets_and_render_policy",
        elisp_form,
        expect,
    )
}

fn malformed_terminal_protocol_is_swallowed_without_desynchronizing_output() -> ParityBatchCase {
    let elisp_form = r##"
(let ((chunks
       '("prompt>\e]0;deploy dashboard"
         "\e"
         "\\ready "
         "\e]8;;https://example.invalid\aA"
         "\e[38;5;999mB"
         "\e[38;5mC"
         "\e[38;2;1;2mD"
         "\e[38:2:1:2:3mE\e[2J"
         "\e(BF"
         "\e"
         "Z"
         "\e[3"
         "2mgreen"))
      plain-chunks states diagnostics)
  (with-temp-buffer
    (neomacs-xterm-color-test-reset)
    (let ((xterm-color-debug t))
      (cl-letf (((symbol-function 'message)
                 (lambda (format-string &rest arguments)
                   (when format-string
                     (push (apply #'format format-string arguments)
                           diagnostics)))))
        (dolist (chunk chunks)
          (let ((output (xterm-color-filter chunk)))
            (push (substring-no-properties output) plain-chunks)
            (push (neomacs-xterm-color-test-state) states)
            (insert output)))))
    (list
     :plain-chunks (nreverse plain-chunks)
     :runs (neomacs-xterm-color-test-buffer-runs)
     :text (buffer-substring-no-properties (point-min) (point-max))
     :diagnostics (nreverse diagnostics)
     :states (nreverse states))))
"##;
    let expect = expect![[
        r##"OK (:plain-chunks ("prompt>" "" "ready " "A" "B" "C" "D" "E" "F" "" "Z" "" "green") :runs (("prompt>ready ABCDEFZ" nil) ("green" (xterm-color t face (:foreground "#59963A")))) :text "prompt>ready ABCDEFZgreen" :diagnostics ("xterm-color: SGR 38;5;999 exceeds range" "xterm-color: SGR 38;5;nil error, expected 38;5;COLOR" "xterm-color: SGR 38;2;1;2;nil error, expected 38;2;R;G;B" "xterm-color: Invalid SGR attribute 58" "xterm-color: (74 50) CSI not implemented" "xterm-color: 66 SET-CHAR not implemented") :states ((:state :ansi-osc :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 0 :truecolor-cache 0) (:state :ansi-osc-esc :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 0 :truecolor-cache 0) (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 0 :truecolor-cache 0) (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 0 :truecolor-cache 0) (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 0 :truecolor-cache 0) (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 0 :truecolor-cache 0) (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 0 :truecolor-cache 0) (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 0 :truecolor-cache 0) (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 0 :truecolor-cache 0) (:state :ansi-esc :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 0 :truecolor-cache 0) (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 0 :truecolor-cache 0) (:state :ansi-csi :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi (51) :face-cache 0 :truecolor-cache 0) (:state :char :foreground 2 :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 1 :truecolor-cache 0)))"##
    ]];
    ParityBatchCase::value(
        "malformed_terminal_protocol_is_swallowed_without_desynchronizing_output",
        elisp_form,
        expect,
    )
}

fn eshell_properties_and_font_lock_face_are_preserved_by_documented_policy() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((prompt (propertize "λ "
                           'face 'success
                           'field 'prompt
                           'read-only t))
       (directory (propertize "cwd"
                              'face 'shadow
                              'field 'directory))
       (input (concat prompt "\e[33mwarning\e[0m " directory))
       (literal-ansi
        (propertize "\e[32mPROTECTED\e[0m"
                    'field 'literal-terminal-data
                    'read-only t))
       default preserved hazard font-locked)
  (with-temp-buffer
    (neomacs-xterm-color-test-reset)
    (setq default (xterm-color-filter input)))
  (with-temp-buffer
    (neomacs-xterm-color-test-reset)
    (setq-local xterm-color-preserve-properties t)
    (setq preserved (xterm-color-filter input)))
  (with-temp-buffer
    (neomacs-xterm-color-test-reset)
    (setq-local xterm-color-preserve-properties t)
    (setq hazard
          (xterm-color-filter
           (concat "\e[31mred " literal-ansi " still-red\e[0m"))))
  (with-temp-buffer
    (neomacs-xterm-color-test-reset)
    (setq-local font-lock-mode t)
    (setq font-locked
          (xterm-color-filter "\e[35mcompiler-note\e[0m")))
  (list
   :default (neomacs-xterm-color-test-string-runs default)
   :preserved (neomacs-xterm-color-test-string-runs preserved)
   :propertized-ansi-hazard
   (list
    :text (substring-no-properties hazard)
    :runs (neomacs-xterm-color-test-string-runs hazard))
   :font-lock (neomacs-xterm-color-test-string-runs font-locked)))
"##;
    let expect = expect![[
        r##"OK (:default (("λ " nil) ("warning" (xterm-color t face (:foreground "#BE8A2D"))) (" cwd" nil)) :preserved (("λ " (read-only t field prompt face success)) ("warning" (face (:foreground "#BE8A2D") xterm-color t)) (" " nil) ("cwd" (field directory face shadow))) :propertized-ansi-hazard (:text "red \33[32mPROTECTED\33[0m still-red" :runs (("red " (face #1=(:foreground "#A93F43") xterm-color t)) ("\33[32mPROTECTED\33[0m" (read-only t field literal-terminal-data)) (" still-red" (face #1# xterm-color t)))) :font-lock (("compiler-note" (xterm-color t font-lock-face (:foreground "#7F60A7")))))"##
    ]];
    ParityBatchCase::value(
        "eshell_properties_and_font_lock_face_are_preserved_by_documented_policy",
        elisp_form,
        expect,
    )
}

fn colorize_buffer_supports_overlays_and_respects_read_only_choice() -> ParityBatchCase {
    let elisp_form = r##"
(let (overlay-workflow refused accepted)
  (with-temp-buffer
    (neomacs-xterm-color-test-reset)
    (insert "plain \e[31mERROR\e[0m tail")
    (set-buffer-modified-p nil)
    (xterm-color-colorize-buffer t)
    (setq overlay-workflow
          (list
           :text (buffer-substring-no-properties (point-min) (point-max))
           :point (point)
           :modified (buffer-modified-p)
           :text-runs (neomacs-xterm-color-test-buffer-runs)
           :overlays (neomacs-xterm-color-test-overlays))))
  (with-temp-buffer
    (neomacs-xterm-color-test-reset)
    (insert "keep \e[32mSAFE\e[0m")
    (goto-char 6)
    (set-buffer-modified-p nil)
    (setq buffer-read-only t)
    (cl-letf (((symbol-function 'y-or-n-p) (lambda (_prompt) nil)))
      (setq refused (xterm-color-colorize-buffer)))
    (setq refused
          (list
           :return refused
           :text (buffer-substring-no-properties (point-min) (point-max))
           :point (point)
           :read-only buffer-read-only
           :modified (buffer-modified-p)
           :state (neomacs-xterm-color-test-state))))
  (with-temp-buffer
    (neomacs-xterm-color-test-reset)
    (insert "keep \e[32mSAFE\e[0m")
    (goto-char 6)
    (set-buffer-modified-p nil)
    (setq buffer-read-only t)
    (cl-letf (((symbol-function 'y-or-n-p) (lambda (_prompt) t)))
      (xterm-color-colorize-buffer))
    (setq accepted
          (list
           :text (buffer-substring-no-properties (point-min) (point-max))
           :runs (neomacs-xterm-color-test-buffer-runs)
           :point (point)
           :read-only buffer-read-only
           :modified (buffer-modified-p)
           :state (neomacs-xterm-color-test-state))))
  (list :overlay overlay-workflow :refused refused :accepted accepted))
"##;
    let expect = expect![[
        r##"OK (:overlay (:text "plain ERROR tail" :point 1 :modified t :text-runs (("plain ERROR tail" nil)) :overlays ((7 12 (:foreground "#A93F43") nil t t "ERROR"))) :refused (:return nil :text "keep \33[32mSAFE\33[0m" :point 6 :read-only t :modified nil :state (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache nil :truecolor-cache nil)) :accepted (:text "keep SAFE" :runs (("keep " nil) ("SAFE" (xterm-color t face (:foreground "#59963A")))) :point 1 :read-only t :modified t :state (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 1 :truecolor-cache 0)))"##
    ]];
    ParityBatchCase::value(
        "colorize_buffer_supports_overlays_and_respects_read_only_choice",
        elisp_form,
        expect,
    )
}

fn narrowed_log_colorization_preserves_inaccessible_text_and_properties() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (neomacs-xterm-color-test-reset)
  (insert "HEADER\n\e[31mFAIL\e[0m at build step\nFOOTER\n")
  (add-text-properties 1 7 '(neomacs-test-scope header))
  (save-excursion
    (goto-char (point-min))
    (search-forward "FAIL")
    (add-text-properties
     (match-beginning 0)
     (match-end 0)
     '(neomacs-test-scope stale-parser-property)))
  (add-text-properties (- (point-max) 7) (1- (point-max))
                       '(neomacs-test-scope footer))
  (set-buffer-modified-p nil)
  (let ((body-begin (save-excursion
                      (goto-char (point-min))
                      (forward-line 1)
                      (point)))
        (body-end (save-excursion
                    (goto-char (point-min))
                    (forward-line 2)
                    (point)))
        restricted)
    (narrow-to-region body-begin body-end)
    (goto-char (point-max))
    (xterm-color-colorize-buffer)
    (setq restricted
          (list
           :bounds (list (point-min) (point-max))
           :point (point)
           :text (buffer-substring-no-properties (point-min) (point-max))
           :runs (neomacs-xterm-color-test-buffer-runs)))
    (widen)
    (list
     :restricted restricted
     :whole-text (buffer-substring-no-properties (point-min) (point-max))
     :whole-runs (neomacs-xterm-color-test-buffer-runs)
     :point (point)
     :modified (buffer-modified-p)
     :state (neomacs-xterm-color-test-state))))
"##;
    let expect = expect![[
        r##"OK (:restricted (:bounds (8 27) :point 8 :text "FAIL at build step\n" :runs (("FAIL" #1=(xterm-color t face (:foreground "#A93F43"))) (" at build step\n" nil))) :whole-text "HEADER\nFAIL at build step\nFOOTER\n" :whole-runs (("HEADER" (neomacs-test-scope header)) ("\n" nil) ("FAIL" #1#) (" at build step\n" nil) ("FOOTER" (neomacs-test-scope footer)) ("\n" nil)) :point 8 :modified t :state (:state :char :foreground nil :background nil :attributes 0 :pending-chars nil :pending-csi nil :face-cache 1 :truecolor-cache 0))"##
    ]];
    ParityBatchCase::value(
        "narrowed_log_colorization_preserves_inaccessible_text_and_properties",
        elisp_form,
        expect,
    )
}

fn palette_and_bold_policy_changes_require_explicit_cache_clear() -> ParityBatchCase {
    let elisp_form = r##"
(let ((xterm-color-names (copy-sequence xterm-color-names))
      (xterm-color-names-bright (copy-sequence xterm-color-names-bright)))
  (with-temp-buffer
    (neomacs-xterm-color-test-reset)
    (let* ((xterm-color-use-bold-for-bright nil)
           (first (xterm-color-filter "\e[1;31mred\e[0m")))
      (aset xterm-color-names 1 "#010203")
      (aset xterm-color-names-bright 1 "#040506")
      (let* ((xterm-color-use-bold-for-bright t)
             (without-clear
              (xterm-color-filter "\e[1;31mstale\e[0m"))
             (cache-before-clear
              (hash-table-count xterm-color--face-cache)))
        (xterm-color-clear-cache)
        (let ((after-clear
               (xterm-color-filter "\e[1;31mfresh\e[0m")))
          (list
           :first (neomacs-xterm-color-test-string-runs first)
           :without-clear
           (neomacs-xterm-color-test-string-runs without-clear)
           :cache-before-clear cache-before-clear
           :after-clear
           (neomacs-xterm-color-test-string-runs after-clear)
           :cache-after-refill
           (hash-table-count xterm-color--face-cache)))))))
"##;
    let expect = expect![[
        r##"OK (:first (("red" (xterm-color t face #1=(:foreground "#EC6261")))) :without-clear (("stale" (xterm-color t face #1#))) :cache-before-clear 1 :after-clear (("fresh" (xterm-color t face (:weight bold :foreground "#010203")))) :cache-after-refill 1)"##
    ]];
    ParityBatchCase::value(
        "palette_and_bold_policy_changes_require_explicit_cache_clear",
        elisp_form,
        expect,
    )
}

fn public_palette_mapper_covers_ansi_cube_and_grayscale_boundaries() -> ParityBatchCase {
    let elisp_form = r##"
(mapcar
 (lambda (color)
   (cons color (xterm-color-256 color)))
 '(0 7 8 15 16 21 52 196 231 232 255))
"##;
    let expect = expect![[
        r##"OK ((0 . "#192033") (7 . "#7E8A90") (8 . "#666666") (15 . "#D3D2D1") (16 . "#000000") (21 . "#0000ff") (52 . "#5f0000") (196 . "#ff0000") (231 . "#ffffff") (232 . "#080808") (255 . "#eeeeee"))"##
    ]];
    ParityBatchCase::value(
        "public_palette_mapper_covers_ansi_cube_and_grayscale_boundaries",
        elisp_form,
        expect,
    )
}

#[test]
fn xterm_color_package_batch() {
    let cases = vec![
        streamed_build_log_reassembles_split_csi_and_carries_style(),
        interleaved_process_buffers_keep_parser_color_and_cache_state_independent(),
        terminal_report_combines_attributes_colors_resets_and_render_policy(),
        malformed_terminal_protocol_is_swallowed_without_desynchronizing_output(),
        eshell_properties_and_font_lock_face_are_preserved_by_documented_policy(),
        colorize_buffer_supports_overlays_and_respects_read_only_choice(),
        narrowed_log_colorization_preserves_inaccessible_text_and_properties(),
        palette_and_bold_policy_changes_require_explicit_cache_clear(),
        public_palette_mapper_covers_ansi_cube_and_grayscale_boundaries(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed xterm-color parity test");
    assert_oracle_batch_cases(
        xterm_color_oracle(),
        test_name,
        "xterm_color_parity",
        &cases,
    );
}
