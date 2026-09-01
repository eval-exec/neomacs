use std::time::Duration;

use expect_test::expect;

use crate::{
    COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, DOOM_MODELINE_MELPA_PIN, NERD_ICONS_MELPA_PIN,
    SHRINK_PATH_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const DOOM_MODELINE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const DOOM_MODELINE_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'bytecomp)
(require 'doom-modeline)

(defun neomacs-doom-modeline-test-render-segments (segments)
  "Run Doom Modeline's registered SEGMENTS and concatenate their output."
  (mapconcat
   (lambda (segment)
     (cond
      ((stringp segment) segment)
      ((alist-get segment doom-modeline--fn-alist)
       (funcall (alist-get segment doom-modeline--fn-alist)))
      ((alist-get segment doom-modeline--var-alist)
       (symbol-value (alist-get segment doom-modeline--var-alist)))
      (t (error "Unregistered Doom Modeline segment: %S" segment))))
   segments ""))

(defun neomacs-doom-modeline-test-property-runs (string property)
  "Return stable PROPERTY runs from STRING."
  (let ((position 0)
        runs)
    (while (< position (length string))
      (let* ((value (get-text-property position property string))
             (next (or (next-single-property-change
                        position property string)
                       (length string))))
        (when value
          (push (list position next (copy-tree value)) runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-doom-modeline-test-summary (string)
  "Return the visible content and interaction styling of STRING."
  (when string
    (list :text (substring-no-properties string)
          :width (string-width string)
          :faces (neomacs-doom-modeline-test-property-runs string 'face)
          :mouse-faces
          (neomacs-doom-modeline-test-property-runs string 'mouse-face)
          :help (neomacs-doom-modeline-test-property-runs string 'help-echo))))

(defun neomacs-doom-modeline-test-key (string event)
  "Return the first binding for EVENT exposed by STRING."
  (let ((position 0)
        binding)
    (while (and (< position (length string)) (not binding))
      (when-let* ((map (get-text-property position 'local-map string)))
        (setq binding (lookup-key map event)))
      (setq position
            (or (next-single-property-change position 'local-map string)
                (length string))))
    binding))

(defun neomacs-doom-modeline-test-modeline-key (format)
  "Return the Doom Modeline name represented by FORMAT."
  (cl-loop for definition in doom-modeline--modelines
           for name = (car definition)
           when (equal format (list "%e" (doom-modeline name)))
           return name))
"###;

fn doom_modeline_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DOOM_MODELINE_MELPA_PIN, "doom-modeline.el")
        .expect("prepare revision-pinned Doom Modeline source below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare exact Compat dependency below ./tmp")
        .with_melpa_dependency(NERD_ICONS_MELPA_PIN)
        .expect("prepare revision-pinned Nerd Icons dependency below ./tmp")
        .with_melpa_dependency(SHRINK_PATH_MELPA_PIN)
        .expect("prepare revision-pinned Shrink Path dependency below ./tmp")
        .with_prelude(DOOM_MODELINE_TEST_PRELUDE)
        .with_timeout(DOOM_MODELINE_TEST_TIMEOUT)
}

fn project_file_styles_drive_real_buffer_identification_and_actions() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((fixture (expand-file-name "tmp/doom-modeline-file-workflow/"
                                  default-directory))
       (root (expand-file-name "project-omega/" fixture))
       (file (expand-file-name "services/api/release-plan.el" root))
       buffer)
  (unwind-protect
      (progn
        (make-directory (file-name-directory file) t)
        (write-region "(defun deploy-release () :ready)\n" nil file nil 'silent)
        (setq buffer (find-file-noselect file))
        (with-current-buffer buffer
          (setq-local doom-modeline--project-root root)
          (let ((doom-modeline-icon nil)
                (doom-modeline-unicode-fallback nil)
                reports)
            (dolist (style '(auto truncate-with-project relative-to-project
                            file-name-with-project project buffer-name))
              (when (eq style 'buffer-name)
                (rename-buffer "release-plan.el<operations>" t))
              (let ((doom-modeline-buffer-file-name-style style))
                (doom-modeline-update-buffer-file-name)
                (push (list :style style
                            :rendered
                            (neomacs-doom-modeline-test-summary
                             doom-modeline--buffer-file-name))
                      reports)))
            (list
             :styles (nreverse reports)
             :mouse-1
             (neomacs-doom-modeline-test-key
              doom-modeline--buffer-file-name [mode-line mouse-1])
             :mouse-3
             (neomacs-doom-modeline-test-key
              doom-modeline--buffer-file-name [mode-line mouse-3])
             :visited (file-equal-p buffer-file-name file)
             :content (buffer-string)))))
    (when (buffer-live-p buffer) (kill-buffer buffer))
    (when (file-directory-p fixture) (delete-directory fixture t))))
"###;
    let expected = expect![[
        r###"OK (:styles ((:style auto :rendered (:text "project-omega/s/a/release-plan.el" :width 33 :faces ((0 14 doom-modeline-project-dir) (14 18 doom-modeline-buffer-path) (18 33 doom-modeline-buffer-file)) :mouse-faces ((0 33 mode-line-highlight)) :help ((0 33 "[ORACLE-TMPDIR]/doom-modeline-file-workflow/project-omega/services/api/release-plan.el\nmouse-1: Previous buffer\nmouse-3: Next buffer")))) (:style truncate-with-project :rendered (:text "project-omega/s/a/release-plan.el" :width 33 :faces ((0 14 doom-modeline-project-dir) (14 18 doom-modeline-buffer-path) (18 33 doom-modeline-buffer-file)) :mouse-faces ((0 33 mode-line-highlight)) :help ((0 33 "[ORACLE-TMPDIR]/doom-modeline-file-workflow/project-omega/services/api/release-plan.el\nmouse-1: Previous buffer\nmouse-3: Next buffer")))) (:style relative-to-project :rendered (:text "services/api/release-plan.el" :width 28 :faces ((0 13 doom-modeline-buffer-path) (13 28 doom-modeline-buffer-file)) :mouse-faces ((0 28 mode-line-highlight)) :help ((0 28 "[ORACLE-TMPDIR]/doom-modeline-file-workflow/project-omega/services/api/release-plan.el\nmouse-1: Previous buffer\nmouse-3: Next buffer")))) (:style file-name-with-project :rendered (:text "project-omega|release-plan.el" :width 29 :faces ((0 13 doom-modeline-project-dir) (14 29 doom-modeline-buffer-file)) :mouse-faces ((0 29 mode-line-highlight)) :help ((0 29 "[ORACLE-TMPDIR]/doom-modeline-file-workflow/project-omega/services/api/release-plan.el\nmouse-1: Previous buffer\nmouse-3: Next buffer")))) (:style project :rendered (:text "project-omega" :width 13 :faces ((0 13 doom-modeline-project-dir)) :mouse-faces ((0 13 mode-line-highlight)) :help ((0 13 "[ORACLE-TMPDIR]/doom-modeline-file-workflow/project-omega/services/api/release-plan.el\nmouse-1: Previous buffer\nmouse-3: Next buffer")))) (:style buffer-name :rendered (:text "release-plan.el<operations>" :width 27 :faces ((0 27 doom-modeline-buffer-file)) :mouse-faces ((0 27 mode-line-highlight)) :help ((0 27 "[ORACLE-TMPDIR]/doom-modeline-file-workflow/project-omega/services/api/release-plan.el\nrelease-plan.el<operations>\nmouse-1: Previous buffer\nmouse-3: Next buffer"))))) :mouse-1 mode-line-previous-buffer :mouse-3 mode-line-next-buffer :visited t :content #("(defun deploy-release () :ready)\n" 0 33 (fontified nil)))"###
    ]];
    ParityBatchCase::value(
        "project_file_styles_drive_real_buffer_identification_and_actions",
        elisp_form,
        expected,
    )
}

fn saved_modified_narrowed_read_only_and_missing_states_render_in_precedence_order()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((fixture (expand-file-name "tmp/doom-modeline-state-workflow/"
                                  default-directory))
       (file (expand-file-name "release-state.txt" fixture))
       buffer)
  (unwind-protect
      (progn
        (make-directory fixture t)
        (write-region "alpha\nbeta\ngamma\n" nil file nil 'silent)
        (setq buffer (find-file-noselect file))
        (with-current-buffer buffer
          (let ((doom-modeline-icon nil)
                (doom-modeline-unicode-fallback nil)
                (doom-modeline-buffer-file-name-style 'file-name)
                reports)
            (doom-modeline-update-buffer-file-name)
            (cl-labels
                ((capture (state)
                   (let ((icon (doom-modeline-update-buffer-file-state-icon)))
                     (push
                      (list :state state
                            :icon (neomacs-doom-modeline-test-summary icon)
                            :buffer-info
                            (neomacs-doom-modeline-test-summary
                             (doom-modeline-segment--buffer-info)))
                      reports))))
              (capture 'saved)
              (goto-char (point-max))
              (insert "deployment pending\n")
              (capture 'modified)
              (narrow-to-region 2 9)
              (capture 'modified-and-narrowed)
              (setq buffer-read-only t)
              (capture 'read-only-and-narrowed)
              (setq buffer-read-only nil)
              (widen)
              (set-buffer-modified-p nil)
              (delete-file file)
              (capture 'missing)
              (nreverse reports)))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (setq buffer-read-only nil))
      (kill-buffer buffer))
    (when (file-directory-p fixture) (delete-directory fixture t))))
"###;
    let expected = expect![[
        r###"OK ((:state saved :icon (:text "" :width 0 :faces nil :mouse-faces nil :help nil) :buffer-info (:text " release-state.txt" :width 18 :faces ((0 1 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (1 18 doom-modeline-buffer-file)) :mouse-faces ((1 18 mode-line-highlight)) :help ((1 18 "[ORACLE-TMPDIR]/doom-modeline-state-workflow/release-state.txt\nmouse-1: Previous buffer\nmouse-3: Next buffer")))) (:state modified :icon (:text "%1*" :width 3 :faces ((0 3 (:inherit (doom-modeline (:inherit (doom-modeline doom-modeline-warning)) nil)))) :mouse-faces nil :help nil) :buffer-info (:text " %1* release-state.txt" :width 22 :faces ((0 1 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (1 4 (:inherit (doom-modeline (:inherit (doom-modeline doom-modeline-warning)) nil))) (4 5 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (5 22 (:inherit (doom-modeline doom-modeline-buffer-modified)))) :mouse-faces ((5 22 mode-line-highlight)) :help ((5 22 "[ORACLE-TMPDIR]/doom-modeline-state-workflow/release-state.txt\nmouse-1: Previous buffer\nmouse-3: Next buffer")))) (:state modified-and-narrowed :icon (:text "%1*><" :width 5 :faces ((0 3 (:inherit (doom-modeline (:inherit (doom-modeline doom-modeline-warning)) nil))) (3 5 (:inherit (doom-modeline (:inherit (doom-modeline doom-modeline-warning)) nil)))) :mouse-faces nil :help nil) :buffer-info (:text " %1*>< release-state.txt" :width 24 :faces ((0 1 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (1 4 (:inherit (doom-modeline (:inherit (doom-modeline doom-modeline-warning)) nil))) (4 6 (:inherit (doom-modeline (:inherit (doom-modeline doom-modeline-warning)) nil))) (6 7 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (7 24 (:inherit (doom-modeline doom-modeline-buffer-modified)))) :mouse-faces ((7 24 mode-line-highlight)) :help ((7 24 "[ORACLE-TMPDIR]/doom-modeline-state-workflow/release-state.txt\nmouse-1: Previous buffer\nmouse-3: Next buffer")))) (:state read-only-and-narrowed :icon (:text "%1*><" :width 5 :faces ((0 3 (:inherit (doom-modeline (:inherit (doom-modeline doom-modeline-warning)) nil))) (3 5 (:inherit (doom-modeline (:inherit (doom-modeline doom-modeline-warning)) nil)))) :mouse-faces nil :help nil) :buffer-info (:text " %1*>< release-state.txt" :width 24 :faces ((0 1 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (1 4 (:inherit (doom-modeline (:inherit (doom-modeline doom-modeline-warning)) nil))) (4 6 (:inherit (doom-modeline (:inherit (doom-modeline doom-modeline-warning)) nil))) (6 7 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (7 24 (:inherit (doom-modeline doom-modeline-buffer-modified)))) :mouse-faces ((7 24 mode-line-highlight)) :help ((7 24 "[ORACLE-TMPDIR]/doom-modeline-state-workflow/release-state.txt\nmouse-1: Previous buffer\nmouse-3: Next buffer")))) (:state missing :icon (:text "!" :width 1 :faces ((0 1 (:inherit (doom-modeline (:inherit (doom-modeline doom-modeline-urgent)) nil)))) :mouse-faces nil :help nil) :buffer-info (:text " ! release-state.txt" :width 20 :faces ((0 1 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (1 2 (:inherit (doom-modeline (:inherit (doom-modeline doom-modeline-urgent)) nil))) (2 3 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (3 20 doom-modeline-buffer-file)) :mouse-faces ((3 20 mode-line-highlight)) :help ((3 20 "[ORACLE-TMPDIR]/doom-modeline-state-workflow/release-state.txt\nmouse-1: Previous buffer\nmouse-3: Next buffer")))))"###
    ]];
    ParityBatchCase::value(
        "saved_modified_narrowed_read_only_and_missing_states_render_in_precedence_order",
        elisp_form,
        expected,
    )
}

fn selections_report_practical_character_line_rectangle_and_word_metrics() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "deploy alpha beta\nrelease gamma delta\nrollback omega\n")
  (setq doom-modeline-current-window (selected-window))
  (cl-labels
      ((capture (name beginning end rectangle word-count)
         (goto-char end)
         (set-mark beginning)
         (setq mark-active t)
         (let ((rectangle-mark-mode rectangle)
               (doom-modeline-enable-word-count word-count))
           (list :selection name
                 :range (buffer-substring-no-properties beginning end)
                 :segment
                 (neomacs-doom-modeline-test-summary
                  (doom-modeline-segment--selection-info))))))
    (let ((alpha (progn (goto-char (point-min)) (search-forward "alpha")
                        (- (point) 5)))
          (beta-end (progn (goto-char (point-min)) (search-forward "beta")
                           (point)))
          (delta-end (progn (goto-char (point-min)) (search-forward "delta")
                            (point))))
      (list
       (capture 'single-line alpha beta-end nil t)
       (capture 'multi-line alpha delta-end nil t)
       (capture 'rectangle alpha delta-end t nil)
       (capture 'whole-lines (point-min)
                (progn (goto-char (point-min)) (forward-line 2) (point))
                nil t)))))
"###;
    let expected = expect![[
        r###"OK ((:selection single-line :range "alpha beta" :segment (:text " 10C 2W " :width 8 :faces ((0 8 doom-modeline-emphasis)) :mouse-faces nil :help nil)) (:selection multi-line :range "alpha beta\nrelease gamma delta" :segment (:text " 30C 2L 5W " :width 11 :faces ((0 11 doom-modeline-emphasis)) :mouse-faces nil :help nil)) (:selection rectangle :range "alpha beta\nrelease gamma delta" :segment (:text " 30C 2L " :width 8 :faces ((0 8 doom-modeline-emphasis)) :mouse-faces nil :help nil)) (:selection whole-lines :range "deploy alpha beta\nrelease gamma delta\n" :segment (:text " 38C 2L 6W " :width 11 :faces ((0 11 doom-modeline-emphasis)) :mouse-faces nil :help nil)))"###
    ]];
    ParityBatchCase::value(
        "selections_report_practical_character_line_rectangle_and_word_metrics",
        elisp_form,
        expected,
    )
}

fn encoding_major_mode_environment_and_text_scale_form_a_real_editor_status_cluster()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(message \"release Ω\")\n")
  (setq-local doom-modeline-env--version "29.4")
  (setq-local text-scale-mode-amount 2)
  (let ((doom-modeline-icon nil)
        (doom-modeline-env-version t)
        reports)
    (dolist (configuration
             '((utf-8-unix t)
               (utf-8-unix nondefault)
               (utf-8-dos nondefault)
               (iso-latin-1-dos nondefault)))
      (setq buffer-file-coding-system (car configuration))
      (let ((doom-modeline-buffer-encoding (cadr configuration)))
        (push
         (list :coding buffer-file-coding-system
               :policy doom-modeline-buffer-encoding
               :encoding
               (neomacs-doom-modeline-test-summary
                (doom-modeline-segment--buffer-encoding)))
         reports)))
    (list
     :encodings (nreverse reports)
     :major-mode
     (neomacs-doom-modeline-test-summary
      (doom-modeline-segment--major-mode))
     :content (buffer-string)
     :mode major-mode)))
"###;
    let expected = expect![[
        r###"OK (:encodings ((:coding utf-8-unix :policy t :encoding (:text " LF UTF-8 " :width 10 :faces ((0 1 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (1 3 (:inherit (doom-modeline mode-line-active))) (3 4 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (4 9 (:inherit (doom-modeline mode-line-active))) (9 10 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil)))) :mouse-faces ((1 3 doom-modeline-highlight) (4 9 doom-modeline-highlight)) :help ((1 3 "End-of-line style: Unix-style LF\nmouse-1: Cycle") (4 9 mode-line-mule-info-help-echo)))) (:coding utf-8-unix :policy nondefault :encoding nil) (:coding utf-8-dos :policy nondefault :encoding (:text " CRLF " :width 6 :faces ((0 1 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (1 5 (:inherit (doom-modeline mode-line-active))) (5 6 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil)))) :mouse-faces ((1 5 doom-modeline-highlight)) :help ((1 5 "End-of-line style: DOS-style CRLF\nmouse-1: Cycle")))) (:coding iso-latin-1-dos :policy nondefault :encoding (:text " CRLF ISO-LATIN-1 " :width 18 :faces ((0 1 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (1 5 (:inherit (doom-modeline mode-line-active))) (5 6 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (6 17 (:inherit (doom-modeline mode-line-active))) (17 18 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil)))) :mouse-faces ((1 5 doom-modeline-highlight) (6 17 doom-modeline-highlight)) :help ((1 5 "End-of-line style: DOS-style CRLF\nmouse-1: Cycle") (6 17 mode-line-mule-info-help-echo))))) :major-mode (:text "  29.4 (+2) " :width 12 :faces ((0 1 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (1 11 (:inherit (doom-modeline doom-modeline-buffer-major-mode))) (11 12 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil)))) :mouse-faces ((1 6 doom-modeline-highlight)) :help ((1 6 "Major mode\nmouse-1: Display major mode menu\nmouse-2: Show help for major mode\nmouse-3: Toggle minor modes"))) :content "(message \"release Ω\")\n" :mode emacs-lisp-mode)"###
    ]];
    ParityBatchCase::value(
        "encoding_major_mode_environment_and_text_scale_form_a_real_editor_status_cluster",
        elisp_form,
        expected,
    )
}

fn extension_api_rebuilds_a_deployment_modeline_and_its_rendered_output() -> ParityBatchCase {
    let elisp_form = r###"
(defvar neomacs-doom-modeline-test-environment nil)
(defvar neomacs-doom-modeline-test-failed-jobs nil)
(defvar neomacs-doom-modeline-test-owner nil)
(let ((neomacs-doom-modeline-test-environment "staging")
      (neomacs-doom-modeline-test-failed-jobs 2)
      (neomacs-doom-modeline-test-owner " release-team ")
      (doom-modeline-excluded-modelines nil)
      (saved-functions doom-modeline--fn-alist)
      (saved-modelines doom-modeline--modelines))
  ;; Define runtime extensions the same way a user's configuration does.
  (doom-modeline-def-segment neomacs-deployment-status
    (format " %s:%d "
            neomacs-doom-modeline-test-environment
            neomacs-doom-modeline-test-failed-jobs))
  (doom-modeline-def-segment neomacs-release-owner
    neomacs-doom-modeline-test-owner)
  (doom-modeline-def-modeline 'neomacs-release
    '(neomacs-deployment-status "|" buffer-info-simple)
    '(neomacs-release-owner major-mode))
  (unwind-protect
      (let ((mode-name "Release Review")
            (doom-modeline-icon nil)
            (doom-modeline-buffer-state-icon nil)
            (buffer (current-buffer)))
        (rename-buffer "release-dashboard" t)
        (cl-labels
            ((capture ()
               (let ((definition
                      (assq 'neomacs-release doom-modeline--modelines)))
                 (list
                  :definition (copy-tree definition)
                  :left
                  (neomacs-doom-modeline-test-summary
                   (neomacs-doom-modeline-test-render-segments
                    (cadr definition)))
                  :right
                  (neomacs-doom-modeline-test-summary
                   (neomacs-doom-modeline-test-render-segments
                    (caddr definition)))
                  :format (copy-tree (doom-modeline 'neomacs-release))))))
          (let ((initial (capture)))
            (doom-modeline-add-segment
             'selection-info 'neomacs-deployment-status
             :after 'neomacs-release)
            (let ((after-add (capture)))
              (doom-modeline-remove-segment
               'neomacs-release-owner 'neomacs-release)
              (let ((after-remove (capture)))
                (doom-modeline-set-modeline 'neomacs-release)
                (list :initial initial
                      :after-add after-add
                      :after-remove after-remove
                      :installed
                      (equal mode-line-format
                             (list "%e" (doom-modeline 'neomacs-release)))
                      :same-buffer (eq buffer (current-buffer))))))))
    (setq doom-modeline--fn-alist saved-functions
          doom-modeline--modelines saved-modelines)
    (fmakunbound 'doom-modeline-segment--neomacs-deployment-status)
    (fmakunbound 'doom-modeline-segment--neomacs-release-owner)))
"###;
    let expected = expect![[
        r###"OK (:initial (:definition (neomacs-release (neomacs-deployment-status "|" buffer-info-simple) (neomacs-release-owner major-mode)) :left (:text " staging:2 | release-dashboard" :width 30 :faces ((12 13 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (13 30 (:inherit (doom-modeline doom-modeline-buffer-file)))) :mouse-faces ((13 30 doom-modeline-highlight)) :help ((13 30 "Buffer name\nmouse-1: Previous buffer\nmouse-3: Next buffer"))) :right (:text " release-team   " :width 16 :faces ((14 16 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil)))) :mouse-faces nil :help nil) :format (:eval (doom-modeline-format--neomacs-release))) :after-add (:definition (neomacs-release (neomacs-deployment-status selection-info "|" buffer-info-simple) (neomacs-release-owner major-mode)) :left (:text " staging:2 | release-dashboard" :width 30 :faces ((12 13 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (13 30 (:inherit (doom-modeline doom-modeline-buffer-file)))) :mouse-faces ((13 30 doom-modeline-highlight)) :help ((13 30 "Buffer name\nmouse-1: Previous buffer\nmouse-3: Next buffer"))) :right (:text " release-team   " :width 16 :faces ((14 16 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil)))) :mouse-faces nil :help nil) :format (:eval (doom-modeline-format--neomacs-release))) :after-remove (:definition (neomacs-release (neomacs-deployment-status selection-info "|" buffer-info-simple) (major-mode)) :left (:text " staging:2 | release-dashboard" :width 30 :faces ((12 13 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (13 30 (:inherit (doom-modeline doom-modeline-buffer-file)))) :mouse-faces ((13 30 doom-modeline-highlight)) :help ((13 30 "Buffer name\nmouse-1: Previous buffer\nmouse-3: Next buffer"))) :right (:text "  " :width 2 :faces ((0 2 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil)))) :mouse-faces nil :help nil) :format (:eval (doom-modeline-format--neomacs-release))) :installed t :same-buffer t)"###
    ]];
    ParityBatchCase::value(
        "extension_api_rebuilds_a_deployment_modeline_and_its_rendered_output",
        elisp_form,
        expected,
    )
}

fn responsive_width_switches_from_project_path_to_a_compact_operational_view() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (rename-buffer "release-plan.el<ops>" t)
  (setq-local buffer-file-name "/workspace/project-omega/services/release-plan.el")
  (setq-local buffer-file-truename buffer-file-name)
  (setq-local doom-modeline--project-root "/workspace/project-omega/")
  (setq-local doom-modeline--project-name " [project-omega] ")
  (let ((doom-modeline-icon nil)
        (doom-modeline-buffer-state-icon nil)
        (doom-modeline-buffer-file-name-style 'relative-to-project)
        (doom-modeline-window-width-limit 85)
        reports)
    (doom-modeline-update-buffer-file-name)
    (dolist (width '(120 85 60))
      (cl-letf (((symbol-function 'window-total-width)
                 (lambda (&rest _) width)))
        (doom-modeline-window-size-change)
        (push
         (list :window-width width
               :limited doom-modeline--limited-width-p
               :buffer-info
               (neomacs-doom-modeline-test-summary
                (doom-modeline-segment--buffer-info))
               :project
               (neomacs-doom-modeline-test-summary
                (doom-modeline-segment--project-name)))
         reports)))
    (let ((doom-modeline-window-width-limit 0.5))
      (cl-letf (((symbol-function 'window-total-width) (lambda (&rest _) 40))
                ((symbol-function 'frame-width) (lambda (&rest _) 100)))
        (doom-modeline-window-size-change)
        (push (list :window-ratio 0.4
                    :limited doom-modeline--limited-width-p
                    :buffer-info
                    (neomacs-doom-modeline-test-summary
                     (doom-modeline-segment--buffer-info)))
              reports)))
    (nreverse reports)))
"###;
    let expected = expect![[
        r###"OK ((:window-width 120 :limited nil :buffer-info (:text " services/release-plan.el" :width 25 :faces ((0 1 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (1 10 doom-modeline-buffer-path) (10 25 doom-modeline-buffer-file)) :mouse-faces ((1 25 mode-line-highlight)) :help ((1 25 "/workspace/project-omega/services/release-plan.el\nrelease-plan.el<ops>\nmouse-1: Previous buffer\nmouse-3: Next buffer"))) :project nil) (:window-width 85 :limited t :buffer-info (:text " release-plan.el<ops>" :width 21 :faces ((0 1 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (1 21 (:inherit (doom-modeline doom-modeline-buffer-file)))) :mouse-faces ((1 21 doom-modeline-highlight)) :help ((1 21 "Buffer name\nmouse-1: Previous buffer\nmouse-3: Next buffer"))) :project nil) (:window-width 60 :limited t :buffer-info (:text " release-plan.el<ops>" :width 21 :faces ((0 1 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (1 21 (:inherit (doom-modeline doom-modeline-buffer-file)))) :mouse-faces ((1 21 doom-modeline-highlight)) :help ((1 21 "Buffer name\nmouse-1: Previous buffer\nmouse-3: Next buffer"))) :project nil) (:window-ratio 0.4 :limited t :buffer-info (:text " release-plan.el<ops>" :width 21 :faces ((0 1 (:inherit ((:inherit (doom-modeline mode-line-active)) nil nil))) (1 21 (:inherit (doom-modeline doom-modeline-buffer-file)))) :mouse-faces ((1 21 doom-modeline-highlight)) :help ((1 21 "Buffer name\nmouse-1: Previous buffer\nmouse-3: Next buffer")))))"###
    ]];
    ParityBatchCase::value(
        "responsive_width_switches_from_project_path_to_a_compact_operational_view",
        elisp_form,
        expected,
    )
}

fn global_mode_assigns_special_buffers_and_restores_editor_defaults() -> ParityBatchCase {
    let elisp_form = r###"
(let ((plain (generate-new-buffer " *doom-modeline-plain*"))
      (message-buffer (generate-new-buffer " *doom-modeline-message*"))
      (later-message (generate-new-buffer " *doom-modeline-later-message*"))
      (before-default (copy-tree (default-value 'mode-line-format))))
  (unwind-protect
      (progn
        (with-current-buffer message-buffer
          (require 'message)
          (message-mode))
        (doom-modeline-mode 1)
        (with-current-buffer later-message
          (message-mode))
        (let ((enabled
               (list
                :global doom-modeline-mode
                :default
                (neomacs-doom-modeline-test-modeline-key
                 (default-value 'mode-line-format))
                :plain
                (with-current-buffer plain
                  (neomacs-doom-modeline-test-modeline-key mode-line-format))
                :existing-message
                (with-current-buffer message-buffer
                  (neomacs-doom-modeline-test-modeline-key mode-line-format))
                :later-message
                (with-current-buffer later-message
                  (neomacs-doom-modeline-test-modeline-key mode-line-format))
                :major-mode-hook
                (memq #'doom-modeline-auto-set-modeline
                      after-change-major-mode-hook))))
          (doom-modeline-mode -1)
          (list
           :enabled enabled
           :disabled
           (list :global doom-modeline-mode
                 :default-restored
                 (equal before-default (default-value 'mode-line-format))
                 :plain-restored
                 (with-current-buffer plain
                   (equal before-default mode-line-format))
                 :message-restored
                 (with-current-buffer message-buffer
                   (equal before-default mode-line-format))
                 :major-mode-hook
                 (memq #'doom-modeline-auto-set-modeline
                       after-change-major-mode-hook)))))
    (doom-modeline-mode -1)
    (mapc (lambda (buffer)
            (when (buffer-live-p buffer) (kill-buffer buffer)))
          (list plain message-buffer later-message))))
"###;
    let expected = expect![[
        r###"OK (:enabled (:global t :default main :plain main :existing-message message :later-message message :major-mode-hook (doom-modeline-auto-set-modeline doom-modeline-update-buffer-file-icon global-eldoc-mode-enable-in-buffer)) :disabled (:global nil :default-restored nil :plain-restored nil :message-restored nil :major-mode-hook nil))"###
    ]];
    ParityBatchCase::value(
        "global_mode_assigns_special_buffers_and_restores_editor_defaults",
        elisp_form,
        expected,
    )
}

#[test]
fn doom_modeline_package_batch() {
    let cases = vec![
        project_file_styles_drive_real_buffer_identification_and_actions(),
        saved_modified_narrowed_read_only_and_missing_states_render_in_precedence_order(),
        selections_report_practical_character_line_rectangle_and_word_metrics(),
        encoding_major_mode_environment_and_text_scale_form_a_real_editor_status_cluster(),
        extension_api_rebuilds_a_deployment_modeline_and_its_rendered_output(),
        responsive_width_switches_from_project_path_to_a_compact_operational_view(),
        global_mode_assigns_special_buffers_and_restores_editor_defaults(),
    ];
    assert_oracle_batch_cases(
        doom_modeline_oracle(),
        "doom-modeline-package-batch",
        "Doom Modeline",
        &cases,
    );
}
