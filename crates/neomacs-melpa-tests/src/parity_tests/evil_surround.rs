use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_SURROUND_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const EVIL_SURROUND_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const EVIL_SURROUND_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'evil-surround)

(defun neomacs-evil-surround-test-position ()
  "Describe the editing cursor without buffer identity or display state."
  (list :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :state (and (boundp 'evil-state) evil-state)))

(defun neomacs-evil-surround-test-properties (begin end)
  "Describe property runs from BEGIN through END."
  (let ((position begin)
        runs)
    (while (< position end)
      (let ((next (next-property-change position nil end)))
        (push (list (- position begin)
                    (- next begin)
                    (text-properties-at position))
              runs)
        (setq position next)))
    (nreverse runs)))

(defun neomacs-evil-surround-test-tag-overlays ()
  "Return outer and inner overlays for the single HTML element at point."
  (save-excursion
    (goto-char (point-min))
    (let ((outer-start (point-min))
          (inner-start (progn (search-forward ">") (point)))
          (inner-end (progn (search-forward "</") (match-beginning 0)))
          (outer-end (progn (search-forward ">") (point))))
      (list (make-overlay outer-start outer-end nil nil t)
            (make-overlay inner-start inner-end nil nil t)))))
"###;

fn evil_surround_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_SURROUND_MELPA_PIN, "evil-surround.el")
        .expect("prepare revision-pinned Evil Surround source below ./tmp")
        .with_prelude(EVIL_SURROUND_TEST_PRELUDE)
        .with_timeout(EVIL_SURROUND_TEST_TIMEOUT)
}

fn characterwise_surround_preserves_properties_markers_and_undo_history() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "Deploy ")
  (let ((begin (point)))
    (insert (propertize "REL-2048" 'release-id 2048 'face 'bold))
    (let* ((end (point))
           (start-marker (copy-marker begin nil))
           (end-marker (copy-marker end t)))
      (setq buffer-undo-list nil)
      (evil-surround-region begin end 'exclusive ?\])
      (let ((edited (buffer-string))
            (edited-point (neomacs-evil-surround-test-position))
            (markers (list (marker-position start-marker)
                           (marker-position end-marker)))
            (properties
             (neomacs-evil-surround-test-properties
              (point-min) (point-max))))
        (setq buffer-undo-list (primitive-undo 2 buffer-undo-list))
        (list :edited edited
              :edited-point edited-point
              :markers markers
              :properties properties
              :restored (buffer-string)
              :restored-point (neomacs-evil-surround-test-position)
              :undo-remaining buffer-undo-list)))))
"###;
    let expected = expect![[
        r#"OK (:edited #("Deploy [REL-2048]" 8 16 (release-id 2048 face bold)) :edited-point (:point 8 :line 1 :column 7 :state nil) :markers (8 18) :properties ((0 8 nil) (8 16 (release-id 2048 face bold)) (16 17 nil)) :restored #("Deploy REL-2048" 7 15 (release-id 2048 face bold)) :restored-point (:point 8 :line 1 :column 7 :state nil) :undo-remaining nil)"#
    ]];
    ParityBatchCase::value(
        "characterwise_surround_preserves_properties_markers_and_undo_history",
        elisp_form,
        expected,
    )
}

fn nested_lisp_change_then_delete_keeps_inner_expression_and_cursor_stable() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(deploy (validate release) (publish release))")
  (goto-char (point-min))
  (search-forward "validate")
  (cl-letf (((symbol-function 'evil-surround-read-char) (lambda () ?\])))
    (evil-surround-change ?\())
  (let ((changed (buffer-string))
        (after-change (neomacs-evil-surround-test-position)))
    (search-forward "release")
    (evil-surround-delete ?\[)
    (list :changed changed
          :after-change after-change
          :deleted (buffer-string)
          :after-delete (neomacs-evil-surround-test-position)
          :deleted-left evil-surround-last-deleted-left)))
"###;
    let expected = expect![[
        r#"OK (:changed "(deploy [validate release] (publish release))" :after-change (:point 9 :line 1 :column 8 :state nil) :deleted "(deploy validate release (publish release))" :after-delete (:point 9 :line 1 :column 8 :state nil) :deleted-left "[")"#
    ]];
    ParityBatchCase::value(
        "nested_lisp_change_then_delete_keeps_inner_expression_and_cursor_stable",
        elisp_form,
        expected,
    )
}

fn linewise_surround_indents_a_multiline_javascript_release_guard() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (js-mode)
  (insert "function deploy(release) {\n"
          "  validate(release);\n"
          "  publish(release);\n"
          "  notify(release);\n"
          "}\n")
  (goto-char (point-min))
  (forward-line 1)
  (let ((begin (point)))
    (forward-line 2)
    (evil-surround-region begin (point) 'line ?\})
    (list :source (buffer-string)
          :position (neomacs-evil-surround-test-position))))
"###;
    let expected = expect![[
        r#"OK (:source "function deploy(release) {\n    {\n\11validate(release);\n\11publish(release);\n    }\n    notify(release);\n}\n" :position (:point 30 :line 2 :column 2 :state nil))"#
    ]];
    ParityBatchCase::value(
        "linewise_surround_indents_a_multiline_javascript_release_guard",
        elisp_form,
        expected,
    )
}

fn blockwise_surround_marks_the_status_column_without_touching_other_columns() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "api      ready\n"
          "worker   retry\n"
          "web      saved\n")
  (setq-local evil-surround-pairs-alist
              (cons '(?| . ("|" . "|")) evil-surround-pairs-alist))
  (goto-char (point-min))
  (search-forward "ready")
  (let ((begin (match-beginning 0)))
    (search-forward "saved")
    (evil-surround-region begin (match-end 0) 'block ?|)
    (list :source (buffer-string)
          :position (neomacs-evil-surround-test-position))))
"###;
    let expected = expect![[
        r#"OK (:source "api      |ready|\nworker   |retry|\nweb      |saved|\n" :position (:point 10 :line 1 :column 9 :state nil))"#
    ]];
    ParityBatchCase::value(
        "blockwise_surround_marks_the_status_column_without_touching_other_columns",
        elisp_form,
        expected,
    )
}

fn html_tag_change_can_keep_then_deliberately_drop_production_attributes() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (html-mode)
  (insert "<section class=\"release\" data-id=\"REL-2048\">Canary ready</section>")
  (goto-char (point-min))
  (search-forward "Canary")
  (pcase-let ((`(,outer ,inner) (neomacs-evil-surround-test-tag-overlays)))
    (unwind-protect
        (cl-letf (((symbol-function 'evil-surround-read-char) (lambda () ?t))
                  ((symbol-function 'evil-surround-read-from-minibuffer)
                   (lambda (&rest _) "article")))
          (evil-surround-change ?t outer inner))
      (delete-overlay outer)
      (delete-overlay inner)))
  (let ((attributes-kept (buffer-string))
        (after-first-change (neomacs-evil-surround-test-position)))
    (goto-char (point-min))
    (pcase-let ((`(,outer ,inner) (neomacs-evil-surround-test-tag-overlays)))
      (unwind-protect
          (cl-letf (((symbol-function 'evil-surround-read-char) (lambda () ?t))
                    ((symbol-function 'evil-surround-read-from-minibuffer)
                     (lambda (&rest _) "aside>")))
            (evil-surround-change ?t outer inner))
        (delete-overlay outer)
        (delete-overlay inner)))
    (list :attributes-kept attributes-kept
          :attributes-dropped (buffer-string)
          :position (neomacs-evil-surround-test-position)
          :deleted-left evil-surround-last-deleted-left)))
"###;
    let expected = expect![[
        r#"OK (:attributes-kept "<article class=\"release\" data-id=\"REL-2048\">Canary ready</article>" :attributes-dropped "<aside>Canary ready</aside>" :position (:point 1 :line 1 :column 0 :state nil) :deleted-left "<article class=\"release\" data-id=\"REL-2048\">")"#
    ]];
    ParityBatchCase::value(
        "html_tag_change_can_keep_then_deliberately_drop_production_attributes",
        elisp_form,
        expected,
    )
}

fn buffer_local_markdown_pair_formats_one_buffer_without_leaking_to_another() -> ParityBatchCase {
    let elisp_form = r###"
(let ((formatted
       (with-temp-buffer
         (text-mode)
         (insert "Release REL-2048 is ready")
         (setq-local evil-surround-pairs-alist
                     (cons '(?~ . ("**" . "**"))
                           evil-surround-pairs-alist))
         (goto-char (point-min))
         (search-forward "REL-2048")
         (evil-surround-region (match-beginning 0) (match-end 0) 'exclusive ?~)
         (list :source (buffer-string)
               :position (neomacs-evil-surround-test-position)
               :pair (evil-surround-pair ?~))))
      (unconfigured
       (with-temp-buffer
         (insert "Release REL-2048 is ready")
         (goto-char (point-min))
         (search-forward "REL-2048")
         (evil-surround-region (match-beginning 0) (match-end 0) 'exclusive ?~)
         (list :source (buffer-string)
               :position (neomacs-evil-surround-test-position)
               :pair (evil-surround-pair ?~)))))
  (list :configured formatted :fresh-buffer unconfigured))
"###;
    let expected = expect![[
        r#"OK (:configured (:source "Release **REL-2048** is ready" :position (:point 9 :line 1 :column 8 :state nil) :pair ("**" . "**")) :fresh-buffer (:source "Release ~REL-2048~ is ready" :position (:point 9 :line 1 :column 8 :state nil) :pair ("~" . "~")))"#
    ]];
    ParityBatchCase::value(
        "buffer_local_markdown_pair_formats_one_buffer_without_leaking_to_another",
        elisp_form,
        expected,
    )
}

fn spaced_delimiter_deletion_trims_only_the_surround_not_inner_whitespace() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(deploy ( release  candidate ) safely)")
  (goto-char (point-min))
  (search-forward "candidate")
  (evil-surround-delete ?\()
  (list :source (buffer-string)
        :position (neomacs-evil-surround-test-position)
        :deleted-left evil-surround-last-deleted-left))
"###;
    let expected = expect![[
        r#"OK (:source "(deploy release  candidate safely)" :position (:point 9 :line 1 :column 8 :state nil) :deleted-left "( ")"#
    ]];
    ParityBatchCase::value(
        "spaced_delimiter_deletion_trims_only_the_surround_not_inner_whitespace",
        elisp_form,
        expected,
    )
}

fn real_evil_keys_change_tags_delete_quotes_and_repeat_word_surrounds() -> ParityBatchCase {
    let elisp_form = r###"
(let ((buffer (generate-new-buffer " *evil-surround-release-note*"))
      snapshots)
  (unwind-protect
      (save-window-excursion
        (switch-to-buffer buffer)
        (insert "\"Hello world!\"")
        (goto-char 2)
        (evil-mode 1)
        (turn-on-evil-surround-mode)
        (evil-normal-state)
        (dolist (keys '("cs\"'" "cs'<q>" "dst" "ysiw]" "W."))
          (execute-kbd-macro keys)
          (push (list keys
                      (buffer-string)
                      (neomacs-evil-surround-test-position))
                snapshots))
        (nreverse snapshots))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (when evil-mode (evil-mode -1)))
      (kill-buffer buffer))))
"###;
    let expected = expect![[
        r#"OK (("cs\"'" "'Hello world!'" (:point 1 :line 1 :column 0 :state normal)) ("cs'<q>" "<q>Hello world!</q>" (:point 1 :line 1 :column 0 :state normal)) ("dst" "Hello world!" (:point 1 :line 1 :column 0 :state normal)) ("ysiw]" "[Hello] world!" (:point 1 :line 1 :column 0 :state normal)) ("W." "[Hello] [world]!" (:point 9 :line 1 :column 8 :state normal)))"#
    ]];
    ParityBatchCase::value(
        "real_evil_keys_change_tags_delete_quotes_and_repeat_word_surrounds",
        elisp_form,
        expected,
    )
    .fresh_process()
}

#[test]
fn evil_surround_package_batch() {
    assert_oracle_batch_cases(
        evil_surround_oracle(),
        "evil-surround-package-batch",
        "evil-surround",
        &[
            characterwise_surround_preserves_properties_markers_and_undo_history(),
            nested_lisp_change_then_delete_keeps_inner_expression_and_cursor_stable(),
            linewise_surround_indents_a_multiline_javascript_release_guard(),
            blockwise_surround_marks_the_status_column_without_touching_other_columns(),
            html_tag_change_can_keep_then_deliberately_drop_production_attributes(),
            buffer_local_markdown_pair_formats_one_buffer_without_leaking_to_another(),
            spaced_delimiter_deletion_trims_only_the_surround_not_inner_whitespace(),
            real_evil_keys_change_tags_delete_quotes_and_repeat_word_surrounds(),
        ],
    );
}
