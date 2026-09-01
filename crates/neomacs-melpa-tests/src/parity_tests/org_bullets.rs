use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ORG_BULLETS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'org-bullets)

(defun neomacs-org-bullets-test-heading-state ()
  "Describe the current Org heading's text and rendered bullet properties."
  (save-excursion
    (beginning-of-line)
    (unless (looking-at "^\\(\\*+\\) \\(.*\\)$")
      (error "Point is not on an Org heading"))
    (let* ((start (match-beginning 1))
           (end (match-end 1))
           (level (- end start))
           (bullet (1- end)))
      (list
       :raw (match-string-no-properties 0)
       :level level
       :composition (get-text-property bullet 'composition)
       :bullet-face (get-text-property bullet 'face)
       :leading-faces
       (let (faces)
         (dotimes (offset (max 0 (1- level)) (nreverse faces))
           (push (get-text-property (+ start offset) 'face) faces)))
       :keymap-span
       (let (mapped)
         (dotimes (offset (1+ level) (nreverse mapped))
           (push (eq (get-text-property (+ start offset) 'keymap)
                     org-bullets-bullet-map)
                 mapped)))))))

(defun neomacs-org-bullets-test-all-headings ()
  "Describe every heading in the current buffer in document order."
  (save-excursion
    (goto-char (point-min))
    (let (states)
      (while (re-search-forward "^\\*+ " nil t)
        (push (neomacs-org-bullets-test-heading-state) states))
      (nreverse states))))

(defun neomacs-org-bullets-test-fontify (text)
  "Create and fully fontify an Org Bullets buffer containing TEXT."
  (insert text)
  (org-mode)
  (org-bullets-mode 1)
  (font-lock-ensure (point-min) (point-max)))
"####;

fn default_bullets_cycle_across_a_real_project_outline() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (neomacs-org-bullets-test-fontify
   "* Plan\n** Build\n*** Test\n**** Release\n***** Observe\n")
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :headings (neomacs-org-bullets-test-all-headings)
        :mode org-bullets-mode
        :modified (buffer-modified-p)))
"####;
    let expected = expect![[
        r#"OK (:text "* Plan\n** Build\n*** Test\n**** Release\n***** Observe\n" :headings ((:raw "* Plan" :level 1 :composition ((1 . 9673)) :bullet-face org-level-1 :leading-faces nil :keymap-span (t t)) (:raw "** Build" :level 2 :composition ((1 . 9675)) :bullet-face org-level-2 :leading-faces (org-hide) :keymap-span (t t t)) (:raw "*** Test" :level 3 :composition ((1 . 10040)) :bullet-face org-level-3 :leading-faces (org-hide org-hide) :keymap-span (t t t t)) (:raw "**** Release" :level 4 :composition ((1 . 10047)) :bullet-face org-level-4 :leading-faces (org-hide org-hide org-hide) :keymap-span (t t t t t)) (:raw "***** Observe" :level 5 :composition ((1 . 9673)) :bullet-face org-level-5 :leading-faces (org-hide org-hide org-hide org-hide) :keymap-span (t t t t t t))) :mode t :modified t)"#
    ]];
    ParityBatchCase::value(
        "default_bullets_cycle_across_a_real_project_outline",
        elisp_form,
        expected,
    )
}

fn odd_level_outline_cycles_a_custom_bullet_palette_by_logical_depth() -> ParityBatchCase {
    let elisp_form = r####"
(let ((org-odd-levels-only t)
      (org-bullets-bullet-list '("A" "B" "C")))
  (with-temp-buffer
    (neomacs-org-bullets-test-fontify
     "* Epic\n*** Story\n***** Task\n******* Check\n")
    (list :logical-characters
          (mapcar #'org-bullets-level-char '(1 3 5 7 9))
          :headings (neomacs-org-bullets-test-all-headings))))
"####;
    let expected = expect![[
        r#"OK (:logical-characters (65 66 67 65 66) :headings ((:raw "* Epic" :level 1 :composition ((1 . 65)) :bullet-face org-level-1 :leading-faces nil :keymap-span (t t)) (:raw "*** Story" :level 3 :composition ((1 . 66)) :bullet-face org-level-2 :leading-faces (org-hide org-hide) :keymap-span (t t t t)) (:raw "***** Task" :level 5 :composition ((1 . 67)) :bullet-face org-level-3 :leading-faces (org-hide org-hide org-hide org-hide) :keymap-span (t t t t t t)) (:raw "******* Check" :level 7 :composition ((1 . 65)) :bullet-face org-level-4 :leading-faces (org-hide org-hide org-hide org-hide org-hide org-hide) :keymap-span (t t t t t t t t))))"#
    ]];
    ParityBatchCase::value(
        "odd_level_outline_cycles_a_custom_bullet_palette_by_logical_depth",
        elisp_form,
        expected,
    )
}

fn demoting_and_promoting_a_subtree_updates_its_rendered_depth() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (neomacs-org-bullets-test-fontify
   "* Release\n** Backend\n*** Deploy\n** Frontend\n")
  (goto-char (point-min))
  (re-search-forward "^\\*\\* Backend")
  (beginning-of-line)
  (let ((before (neomacs-org-bullets-test-all-headings)))
    (org-demote-subtree)
    (font-lock-ensure (point-min) (point-max))
    (let ((demoted (neomacs-org-bullets-test-all-headings)))
      (org-promote-subtree)
      (font-lock-ensure (point-min) (point-max))
      (list :before before
            :demoted demoted
            :restored (neomacs-org-bullets-test-all-headings)
            :text (buffer-substring-no-properties (point-min) (point-max))
            :modified (buffer-modified-p)))))
"####;
    let expected = expect![[
        r#"OK (:before ((:raw "* Release" :level 1 :composition ((1 . 9673)) :bullet-face org-level-1 :leading-faces nil :keymap-span (t t)) (:raw "** Backend" :level 2 :composition ((1 . 9675)) :bullet-face org-level-2 :leading-faces (org-hide) :keymap-span (t t t)) (:raw "*** Deploy" :level 3 :composition ((1 . 10040)) :bullet-face org-level-3 :leading-faces (org-hide org-hide) :keymap-span (t t t t)) (:raw "** Frontend" :level 2 :composition ((1 . 9675)) :bullet-face org-level-2 :leading-faces (org-hide) :keymap-span (t t t))) :demoted ((:raw "* Release" :level 1 :composition ((1 . 9673)) :bullet-face org-level-1 :leading-faces nil :keymap-span (t t)) (:raw "*** Backend" :level 3 :composition ((1 . 10040)) :bullet-face org-level-3 :leading-faces (org-hide org-hide) :keymap-span (t t t t)) (:raw "**** Deploy" :level 4 :composition ((1 . 10047)) :bullet-face org-level-4 :leading-faces (org-hide org-hide org-hide) :keymap-span (t t t t t)) (:raw "** Frontend" :level 2 :composition ((1 . 9675)) :bullet-face org-level-2 :leading-faces (org-hide) :keymap-span (t t t))) :restored ((:raw "* Release" :level 1 :composition ((1 . 9673)) :bullet-face org-level-1 :leading-faces nil :keymap-span (t t)) (:raw "** Backend" :level 2 :composition ((1 . 9675)) :bullet-face org-level-2 :leading-faces (org-hide) :keymap-span (t t t)) (:raw "*** Deploy" :level 3 :composition ((1 . 10040)) :bullet-face org-level-3 :leading-faces (org-hide org-hide) :keymap-span (t t t t)) (:raw "** Frontend" :level 2 :composition ((1 . 9675)) :bullet-face org-level-2 :leading-faces (org-hide) :keymap-span (t t t))) :text "* Release\n** Backend\n*** Deploy\n** Frontend\n" :modified t)"#
    ]];
    ParityBatchCase::value(
        "demoting_and_promoting_a_subtree_updates_its_rendered_depth",
        elisp_form,
        expected,
    )
}

fn disabling_and_reenabling_removes_and_rebuilds_display_state() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (neomacs-org-bullets-test-fontify "* Build\n*** Ship\n")
  (let ((enabled (neomacs-org-bullets-test-all-headings)))
    (org-bullets-mode -1)
    (font-lock-ensure (point-min) (point-max))
    (let ((disabled (neomacs-org-bullets-test-all-headings)))
      (goto-char (point-max))
      (insert "** Verify\n")
      (font-lock-ensure (point-min) (point-max))
      (let ((edited-while-disabled (neomacs-org-bullets-test-all-headings)))
        (org-bullets-mode 1)
        (font-lock-ensure (point-min) (point-max))
        (org-bullets-mode 1)
        (font-lock-ensure (point-min) (point-max))
        (list :enabled enabled
              :disabled disabled
              :edited-while-disabled edited-while-disabled
              :reenabled (neomacs-org-bullets-test-all-headings)
              :mode org-bullets-mode
              :keyword-count
              (cl-count (car org-bullets--keywords)
                        font-lock-keywords
                        :test #'equal))))))
"####;
    let expected = expect![[
        r#"OK (:enabled ((:raw "* Build" :level 1 :composition ((1 . 9673)) :bullet-face org-level-1 :leading-faces nil :keymap-span (t t)) (:raw "*** Ship" :level 3 :composition ((1 . 10040)) :bullet-face org-level-3 :leading-faces (org-hide org-hide) :keymap-span (t t t t))) :disabled ((:raw "* Build" :level 1 :composition nil :bullet-face org-level-1 :leading-faces nil :keymap-span (nil nil)) (:raw "*** Ship" :level 3 :composition nil :bullet-face org-level-3 :leading-faces (org-level-3 org-level-3) :keymap-span (nil nil nil nil))) :edited-while-disabled ((:raw "* Build" :level 1 :composition nil :bullet-face org-level-1 :leading-faces nil :keymap-span (nil nil)) (:raw "*** Ship" :level 3 :composition nil :bullet-face org-level-3 :leading-faces (org-level-3 org-level-3) :keymap-span (nil nil nil nil)) (:raw "** Verify" :level 2 :composition nil :bullet-face org-level-2 :leading-faces (org-level-2) :keymap-span (nil nil nil))) :reenabled ((:raw "* Build" :level 1 :composition ((1 . 9673)) :bullet-face org-level-1 :leading-faces nil :keymap-span (t t)) (:raw "*** Ship" :level 3 :composition ((1 . 10040)) :bullet-face org-level-3 :leading-faces (org-hide org-hide) :keymap-span (t t t t)) (:raw "** Verify" :level 2 :composition ((1 . 9675)) :bullet-face org-level-2 :leading-faces (org-hide) :keymap-span (t t t))) :mode t :keyword-count 1)"#
    ]];
    ParityBatchCase::value(
        "disabling_and_reenabling_removes_and_rebuilds_display_state",
        elisp_form,
        expected,
    )
}

fn org_mode_hook_enables_rendering_per_buffer_without_affecting_plain_text() -> ParityBatchCase {
    let elisp_form = r####"
(let ((org-mode-hook (cons #'org-bullets-mode org-mode-hook)))
  (list
   :project
   (with-temp-buffer
     (insert "* Project\n** Build\n")
     (org-mode)
     (font-lock-ensure (point-min) (point-max))
     (list :mode org-bullets-mode
           :headings (neomacs-org-bullets-test-all-headings)))
   :plain-text
   (with-temp-buffer
     (insert "* Not an Org heading\n")
     (text-mode)
     (font-lock-ensure (point-min) (point-max))
     (list :mode (bound-and-true-p org-bullets-mode)
           :composition (get-text-property (point-min) 'composition)
           :keymap (get-text-property (point-min) 'keymap)))
   :runbook
   (with-temp-buffer
     (insert "* Runbook\n**** Recovery\n")
     (org-mode)
     (font-lock-ensure (point-min) (point-max))
     (list :mode org-bullets-mode
           :headings (neomacs-org-bullets-test-all-headings)))))
"####;
    let expected = expect![[
        r#"OK (:project (:mode t :headings ((:raw "* Project" :level 1 :composition ((1 . 9673)) :bullet-face org-level-1 :leading-faces nil :keymap-span (t t)) (:raw "** Build" :level 2 :composition ((1 . 9675)) :bullet-face org-level-2 :leading-faces (org-hide) :keymap-span (t t t)))) :plain-text (:mode nil :composition nil :keymap nil) :runbook (:mode t :headings ((:raw "* Runbook" :level 1 :composition ((1 . 9673)) :bullet-face org-level-1 :leading-faces nil :keymap-span (t t)) (:raw "**** Recovery" :level 4 :composition ((1 . 10047)) :bullet-face org-level-4 :leading-faces (org-hide org-hide org-hide) :keymap-span (t t t t t)))))"#
    ]];
    ParityBatchCase::value(
        "org_mode_hook_enables_rendering_per_buffer_without_affecting_plain_text",
        elisp_form,
        expected,
    )
}

fn inline_tasks_custom_faces_and_click_map_cover_both_boundary_stars() -> ParityBatchCase {
    let elisp_form = r####"
(require 'org-inlinetask)
(let ((org-bullets-bullet-list '("◆"))
      (org-bullets-face-name 'font-lock-warning-face)
      (org-bullets-bullet-map (make-sparse-keymap)))
  (define-key org-bullets-bullet-map [mouse-1] #'org-cycle)
  (with-temp-buffer
    (neomacs-org-bullets-test-fontify
     "* Project\n*************** Inline deployment\n")
    (goto-char (point-min))
    (re-search-forward "^\\*\\{15\\} ")
    (let* ((end (1- (point)))
           (last-star (1- end))
           (penultimate-star (1- last-star)))
      (list
       :headings (neomacs-org-bullets-test-all-headings)
       :inline-last-composition
       (get-text-property last-star 'composition)
       :inline-penultimate-composition
       (get-text-property penultimate-star 'composition)
       :inline-face-range
       (list (get-text-property penultimate-star 'face)
             (get-text-property last-star 'face))
       :mouse-command
       (lookup-key (get-text-property last-star 'keymap) [mouse-1])))))
"####;
    let expected = expect![[
        r#"OK (:headings ((:raw "* Project" :level 1 :composition ((1 . 9670)) :bullet-face font-lock-warning-face :leading-faces nil :keymap-span (t t)) (:raw "*************** Inline deployment" :level 15 :composition #1=((1 . 9670)) :bullet-face org-inlinetask :leading-faces (org-hide org-hide org-hide org-hide org-hide org-hide org-hide org-hide org-hide org-hide org-hide org-hide org-hide org-inlinetask) :keymap-span (t t t t t t t t t t t t t t t t))) :inline-last-composition #1# :inline-penultimate-composition ((1 . 9670)) :inline-face-range (org-inlinetask org-inlinetask) :mouse-command org-cycle)"#
    ]];
    ParityBatchCase::value(
        "inline_tasks_custom_faces_and_click_map_cover_both_boundary_stars",
        elisp_form,
        expected,
    )
}

fn org_bullets_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORG_BULLETS_MELPA_PIN, "org-bullets.el")
        .expect("prepare pinned Org Bullets source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn org_bullets_practical_workflows_batch() {
    let cases = vec![
        default_bullets_cycle_across_a_real_project_outline(),
        odd_level_outline_cycles_a_custom_bullet_palette_by_logical_depth(),
        demoting_and_promoting_a_subtree_updates_its_rendered_depth(),
        disabling_and_reenabling_removes_and_rebuilds_display_state(),
        org_mode_hook_enables_rendering_per_buffer_without_affecting_plain_text(),
        inline_tasks_custom_faces_and_click_map_cover_both_boundary_stars(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("org-bullets parity batch");
    assert_oracle_batch_cases(
        org_bullets_oracle(),
        test_name,
        "org-bullets parity",
        &cases,
    );
}
