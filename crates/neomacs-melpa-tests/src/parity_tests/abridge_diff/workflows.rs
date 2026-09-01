use expect_test::expect;

use super::ParityBatchCase;

fn abridge_diff_mode_abridges_a_refined_unified_diff_hunk_in_diff_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "abridge_diff_mode_abridges_a_refined_unified_diff_hunk_in_diff_mode",
        r#"
    ;; A user diffs two revisions of a paragraph-per-line LaTeX paper, opens the
    ;; real unified diff in `diff-mode', switches abridge-diff on and refines the
    ;; hunk.  Every changed line must keep its start plus a small window around
    ;; the refined words, with the rest elided behind an ellipsis.
    (let ((text (abridge-diff-test-unified-diff
                 "paper" "paper.tex"
                 abridge-diff-test-paper-old abridge-diff-test-paper-new)))
      (unwind-protect
          (abridge-diff-test-with-buffer text
            (diff-mode)
            (abridge-diff-mode 1)
            (abridge-diff-enable-hiding)
            (diff-hunk-next)
            (diff-refine-hunk)
            (list major-mode
                  abridge-diff-mode
                  abridge-diff-hiding
                  buffer-invisibility-spec
                  (point)
                  (equal (buffer-substring-no-properties (point-min) (point-max)) text)
                  (abridge-diff-test-rendered)
                  (abridge-diff-test-hidden)
                  (abridge-diff-test-refined-overlays 'diff-mode 'fine)))
        (abridge-diff-mode -1)))
"#,
        expect![[
            r#"OK (diff-mode t t ((abridge-diff-invisible . t) t) 33 t ("--- a/paper.tex" "+++ b/paper.tex" "@@ -1,6 +1,6 @@" " \\section{Résultats}" "-The naïve estimator converges slowly..." "+The refined estimator converges slowly..." " Every measurement was repeated three times." "-The Grüneisen parameter γ was held fixed at 1.85 for the whole..." "+The Grüneisen parameter γ was held fixed at 2.10 for the whole..." " We thank the anonymous reviewers for their comments." "-Figure 3 shows the same data on a logarithmic axis, together with the analytic prediction of Eq. (7) and the two bootstrap confidence bands computed from ten thousand resamples of the raw counts." "+Figure 3 shows the same data on a logarithmic axis, together with the analytic prediction of Eq. (7) and the two bootstrap confidence bands computed from ten thousand resamples of the raw counts and their weights." "") ((107 291 abridge-diff-invisible " whenever the sampling density is uneven, and the resulting curves stay noisy near the boundary of the domain, which makes any comparison with the reference solution hard to interpret.") (331 515 abridge-diff-invisible " whenever the sampling density is uneven, and the resulting curves stay noisy near the boundary of the domain, which makes any comparison with the reference solution hard to interpret.") (624 742 abridge-diff-invisible " sweep, and the residuals were accumulated over the full temperature range from 4 K up to 300 K without any smoothing.") (806 924 abridge-diff-invisible " sweep, and the residuals were accumulated over the full temperature range from 4 K up to 300 K without any smoothing.")) ((70 292 nil t "-The naïve estimator converges slowly whenever the sampling density is uneven, and the resulting curves stay noisy near the boundary of the domain, which makes any comparison with the reference solution hard to interpret.\n") (75 80 diff-refine-removed nil "naïve") (292 516 nil t "+The refined estimator converges slowly whenever the sampling density is uneven, and the resulting curves stay noisy near the boundary of the domain, which makes any comparison with the reference solution hard to interpret.\n") (297 304 diff-refine-added nil "refined") (561 743 nil t "-The Grüneisen parameter γ was held fixed at 1.85 for the whole sweep, and the residuals were accumulated over the full temperature range from 4 K up to 300 K without any smoothing.\n") (606 607 diff-refine-removed nil "1") (608 610 diff-refine-removed nil "85") (743 925 nil t "+The Grüneisen parameter γ was held fixed at 2.10 for the whole sweep, and the residuals were accumulated over the full temperature range from 4 K up to 300 K without any smoothing.\n") (788 789 diff-refine-added nil "2") (790 792 diff-refine-added nil "10") (979 1176 nil t "-Figure 3 shows the same data on a logarithmic axis, together with the analytic prediction of Eq. (7) and the two bootstrap confidence bands computed from ten thousand resamples of the raw counts.\n") (1174 1175 nil nil ".") (1176 1391 nil t "+Figure 3 shows the same data on a logarithmic axis, together with the analytic prediction of Eq. (7) and the two bootstrap confidence bands computed from ten thousand resamples of the raw counts and their weights.\n") (1372 1389 diff-refine-added nil "and their weights")))"#
        ]],
    )
}

fn abridge_diff_toggle_hiding_switches_a_refined_hunk_between_abridged_and_full_text()
-> ParityBatchCase {
    ParityBatchCase::value(
        "abridge_diff_toggle_hiding_switches_a_refined_hunk_between_abridged_and_full_text",
        r#"
    ;; `abridge-diff-toggle-hiding' is the documented way to see the full hunk
    ;; again.  Toggling must only change the invisibility spec: the abridged text
    ;; is still in the buffer and comes back unchanged on the second toggle.
    (let ((text (abridge-diff-test-unified-diff
                 "toggle" "paper.tex"
                 abridge-diff-test-paper-old abridge-diff-test-paper-new))
          messages)
      (unwind-protect
          (abridge-diff-test-with-buffer text
            (diff-mode)
            (abridge-diff-mode 1)
            (abridge-diff-enable-hiding)
            (diff-hunk-next)
            (diff-refine-hunk)
            (cl-letf (((symbol-function 'message)
                       (lambda (format-string &rest arguments)
                         (push (apply #'format format-string arguments) messages)
                         nil)))
              (let ((abridged (abridge-diff-test-rendered)))
                (abridge-diff-toggle-hiding)
                (let ((full (abridge-diff-test-rendered))
                      (full-hiding abridge-diff-hiding)
                      (full-spec (copy-tree buffer-invisibility-spec)))
                  (abridge-diff-toggle-hiding)
                  (list abridged
                        full
                        (equal full (split-string
                                     (buffer-substring-no-properties
                                      (point-min) (point-max))
                                     "\n"))
                        full-hiding
                        full-spec
                        abridge-diff-hiding
                        (copy-tree buffer-invisibility-spec)
                        (equal abridged (abridge-diff-test-rendered))
                        (length (abridge-diff-test-hidden))
                        (nreverse messages))))))
        (abridge-diff-mode -1)))
"#,
        expect![[
            r#"OK (("--- a/paper.tex" "+++ b/paper.tex" "@@ -1,6 +1,6 @@" " \\section{Résultats}" "-The naïve estimator converges slowly..." "+The refined estimator converges slowly..." " Every measurement was repeated three times." "-The Grüneisen parameter γ was held fixed at 1.85 for the whole..." "+The Grüneisen parameter γ was held fixed at 2.10 for the whole..." " We thank the anonymous reviewers for their comments." "-Figure 3 shows the same data on a logarithmic axis, together with the analytic prediction of Eq. (7) and the two bootstrap confidence bands computed from ten thousand resamples of the raw counts." "+Figure 3 shows the same data on a logarithmic axis, together with the analytic prediction of Eq. (7) and the two bootstrap confidence bands computed from ten thousand resamples of the raw counts and their weights." "") ("--- a/paper.tex" "+++ b/paper.tex" "@@ -1,6 +1,6 @@" " \\section{Résultats}" "-The naïve estimator converges slowly whenever the sampling density is uneven, and the resulting curves stay noisy near the boundary of the domain, which makes any comparison with the reference solution hard to interpret." "+The refined estimator converges slowly whenever the sampling density is uneven, and the resulting curves stay noisy near the boundary of the domain, which makes any comparison with the reference solution hard to interpret." " Every measurement was repeated three times." "-The Grüneisen parameter γ was held fixed at 1.85 for the whole sweep, and the residuals were accumulated over the full temperature range from 4 K up to 300 K without any smoothing." "+The Grüneisen parameter γ was held fixed at 2.10 for the whole sweep, and the residuals were accumulated over the full temperature range from 4 K up to 300 K without any smoothing." " We thank the anonymous reviewers for their comments." "-Figure 3 shows the same data on a logarithmic axis, together with the analytic prediction of Eq. (7) and the two bootstrap confidence bands computed from ten thousand resamples of the raw counts." "+Figure 3 shows the same data on a logarithmic axis, together with the analytic prediction of Eq. (7) and the two bootstrap confidence bands computed from ten thousand resamples of the raw counts and their weights." "") t nil (t) t ((abridge-diff-invisible . t) t) t 4 ("Diff Abridging Off" "Diff Abridging On"))"#
        ]],
    )
}

fn abridge_diff_word_budget_settings_change_how_much_context_survives() -> ParityBatchCase {
    ParityBatchCase::value(
        "abridge_diff_word_budget_settings_change_how_much_context_survives",
        r#"
    ;; The customization group is the package's second documented feature.  The
    ;; same changelog hunk is refined four times with different budgets; each
    ;; setting must move the ellipsis to a different word.
    (let ((text (abridge-diff-test-unified-diff
                 "budget" "CHANGELOG"
                 abridge-diff-test-changelog-old abridge-diff-test-changelog-new)))
      (unwind-protect
          (cl-flet ((render (word-buffer invisible-min)
                      (let ((abridge-diff-word-buffer word-buffer)
                            (abridge-diff-invisible-min invisible-min))
                        (abridge-diff-test-with-buffer text
                          (diff-mode)
                          (abridge-diff-enable-hiding)
                          (diff-hunk-next)
                          (diff-refine-hunk)
                          (list word-buffer
                                invisible-min
                                (nthcdr 3 (abridge-diff-test-rendered))
                                (mapcar (lambda (run) (nth 3 run))
                                        (abridge-diff-test-hidden)))))))
            (abridge-diff-mode 1)
            (list (list abridge-diff-word-buffer
                        abridge-diff-invisible-min
                        abridge-diff-first-words-preserve
                        abridge-diff-no-change-line-words)
                  (render 3 5)
                  (render 0 1)
                  (render 8 5)
                  (render 3 400)))
        (abridge-diff-mode -1)))
"#,
        expect![[
            r#"OK ((3 5 4 12) (3 5 ("-Fixed a crash in the exporter..." "+Fixed a hang in the exporter..." "") (" when the document contained nested tables with merged cells inside a footnote." " when the document contained nested tables with merged cells inside a footnote.")) (0 1 ("-Fixed a crash..." "+Fixed a hang..." "") (" in the exporter when the document contained nested tables with merged cells inside a footnote." " in the exporter when the document contained nested tables with merged cells inside a footnote.")) (8 5 ("-Fixed a crash in the exporter when the document contained nested..." "+Fixed a hang in the exporter when the document contained nested..." "") (" tables with merged cells inside a footnote." " tables with merged cells inside a footnote.")) (3 400 ("-Fixed a crash in the exporter when the document contained nested tables with merged cells inside a footnote." "+Fixed a hang in the exporter when the document contained nested tables with merged cells inside a footnote." "") nil))"#
        ]],
    )
}

fn abridge_diff_abridges_smerge_conflict_refinement_with_the_no_change_word_budget()
-> ParityBatchCase {
    ParityBatchCase::value(
        "abridge_diff_abridges_smerge_conflict_refinement_with_the_no_change_word_budget",
        r#"
    ;; abridge-diff advises `smerge-refine-regions', so it also fires when the
    ;; user refines a real merge conflict with `smerge-refine'.  smerge tags its
    ;; overlays with `smerge'/`refine' rather than `diff-mode'/`fine', so no
    ;; refined region is protected and each conflict line keeps exactly
    ;; `abridge-diff-no-change-line-words' words.
    (let* ((path (abridge-diff-test-write "merge/deploy-checklist.txt"
                                          abridge-diff-test-conflict))
           (vc-handled-backends nil)
           (buffer (find-file-noselect path)))
      (unwind-protect
          (with-current-buffer buffer
            (abridge-diff-mode 1)
            (smerge-mode 1)
            (abridge-diff-enable-hiding)
            (goto-char (point-min))
            (smerge-next)
            (set-buffer-modified-p nil)
            (smerge-refine)
            (list (file-name-nondirectory (buffer-file-name))
                  major-mode
                  smerge-mode
                  (point)
                  (buffer-modified-p)
                  abridge-diff-no-change-line-words
                  (abridge-diff-test-rendered)
                  (mapcar (lambda (run) (list (nth 0 run) (nth 1 run) (nth 3 run)))
                          (abridge-diff-test-hidden))
                  (abridge-diff-test-refined-overlays 'diff-mode 'fine)
                  (mapcar (lambda (overlay)
                            (list (nth 0 overlay) (nth 1 overlay)
                                  (nth 2 overlay) (nth 3 overlay)))
                          (abridge-diff-test-refined-overlays 'smerge 'refine))))
        (abridge-diff-mode -1)
        (with-current-buffer buffer (set-buffer-modified-p nil))
        (kill-buffer buffer)))
"#,
        expect![[
            r##"OK ("deploy-checklist.txt" text-mode t 24 nil 12 ("# Deployment checklist" "<<<<<<< HEAD" "Run the migration script against the staging database before touching production, and..." "Notify the on-call engineer in the release channel." "=======" "Run the migration script against the staging database before touching production, and..." "Notify the on-call engineer in the release channel." ">>>>>>> feature/rollout" "Archive the build artifacts." "") ((122 183 " keep the maintenance banner up until the smoke tests finish.") (329 393 " keep the maintenance banner up until every smoke test finishes.")) nil ((37 236 nil t) (160 163 smerge-refined-removed nil) (170 175 smerge-refined-removed nil) (176 182 smerge-refined-removed nil) (244 446 nil t) (367 372 smerge-refined-added nil) (379 383 smerge-refined-added nil) (384 392 smerge-refined-added nil)))"##
        ]],
    )
}

fn abridge_diff_mode_is_the_switch_that_installs_and_removes_the_abridging_advice()
-> ParityBatchCase {
    ParityBatchCase::value(
        "abridge_diff_mode_is_the_switch_that_installs_and_removes_the_abridging_advice",
        r#"
    ;; Turning the global mode off must restore plain diff-mode: refinement still
    ;; happens, but nothing is hidden and the advice is gone again.
    (let ((text (abridge-diff-test-unified-diff
                 "switch" "CHANGELOG"
                 abridge-diff-test-changelog-old abridge-diff-test-changelog-new)))
      (unwind-protect
          (cl-flet ((refine-once ()
                      (abridge-diff-test-with-buffer text
                        (diff-mode)
                        (abridge-diff-enable-hiding)
                        (diff-hunk-next)
                        (diff-refine-hunk)
                        (list abridge-diff-mode
                              (and (advice-member-p #'abridge-diff-abridge
                                                    #'smerge-refine-regions)
                                   t)
                              (length (abridge-diff-test-refined-overlays
                                       'diff-mode 'fine))
                              (mapcar (lambda (run) (nth 3 run))
                                      (abridge-diff-test-hidden))
                              (equal (abridge-diff-test-rendered)
                                     (split-string
                                      (buffer-substring-no-properties
                                       (point-min) (point-max))
                                      "\n"))))))
            (list (refine-once)
                  (progn (abridge-diff-mode 1) (refine-once))
                  (progn (abridge-diff-mode -1) (refine-once))))
        (abridge-diff-mode -1)
        (advice-remove #'smerge-refine-regions #'abridge-diff-abridge)))
"#,
        expect![[
            r#"OK ((nil nil 4 nil t) (t t 4 (" when the document contained nested tables with merged cells inside a footnote." " when the document contained nested tables with merged cells inside a footnote.") nil) (nil nil 4 nil t))"#
        ]],
    )
}

fn abridge_diff_font_lock_refinement_abridges_changed_hunks_and_leaves_additions_whole()
-> ParityBatchCase {
    ParityBatchCase::value(
        "abridge_diff_font_lock_refinement_abridges_changed_hunks_and_leaves_additions_whole",
        r#"
    ;; `diff-refine' defaults to `font-lock', which is why the README promises
    ;; abridging "immediately" without any command.  Fontifying a two-hunk diff
    ;; must abridge the modified hunk and leave the pure-insertion hunk, including
    ;; its Greek and Japanese text, completely visible.
    (let ((text (abridge-diff-test-unified-diff
                 "notes" "notes.md"
                 abridge-diff-test-notes-old abridge-diff-test-notes-new)))
      (unwind-protect
          (abridge-diff-test-with-buffer text
            (diff-mode)
            (abridge-diff-mode 1)
            (abridge-diff-enable-hiding)
            (font-lock-mode 1)
            (font-lock-ensure)
            (list diff-refine
                  (abridge-diff-test-rendered)
                  (mapcar (lambda (run) (list (nth 0 run) (nth 1 run) (nth 3 run)))
                          (abridge-diff-test-hidden))
                  (mapcar (lambda (overlay)
                            (list (nth 0 overlay) (nth 1 overlay)
                                  (nth 2 overlay) (nth 3 overlay)))
                          (abridge-diff-test-refined-overlays 'diff-mode 'fine))))
        (abridge-diff-mode -1)))
"#,
        expect![[
            r#"OK (font-lock ("--- a/notes.md" "+++ b/notes.md" "@@ -1,5 +1,5 @@" " # Release notes" "-The installer now verifies the checksum of..." "+The installer now validates the checksum of..." " " " ## Compatibility" " The minimum supported version is unchanged." "@@ -9,3 +9,4 @@" " " " ## Credits" " Thanks to everyone who filed a report." "+Special thanks to the translators of the Ελληνικά and 日本語 catalogues." "") ((107 245 " every downloaded artifact before it is unpacked, and it refuses to continue whenever the signature does not match the published manifest.") (290 428 " every downloaded artifact before it is unpacked, and it refuses to continue whenever the signature does not match the published manifest.")) ((31 494 nil nil) (64 246 nil t) (83 91 diff-refine-removed nil) (246 429 nil t) (265 274 diff-refine-added nil) (494 635 nil nil)))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        abridge_diff_mode_abridges_a_refined_unified_diff_hunk_in_diff_mode(),
        abridge_diff_toggle_hiding_switches_a_refined_hunk_between_abridged_and_full_text(),
        abridge_diff_word_budget_settings_change_how_much_context_survives(),
        abridge_diff_abridges_smerge_conflict_refinement_with_the_no_change_word_budget(),
        abridge_diff_mode_is_the_switch_that_installs_and_removes_the_abridging_advice(),
        abridge_diff_font_lock_refinement_abridges_changed_hunks_and_leaves_additions_whole(),
    ]
}
