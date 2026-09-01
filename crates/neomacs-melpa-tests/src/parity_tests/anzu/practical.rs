use expect_test::expect;

use super::ParityBatchCase;

fn incremental_search_reports_each_repeated_match_and_restores_the_mode_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "incremental_search_reports_each_repeated_match_and_restores_the_mode_line",
        r##"(let ((buffer
         (generate-new-buffer " *anzu-incremental-search-workflow*"))
        result)
    (unwind-protect
        (progn
          (switch-to-buffer buffer)
          (insert "alpha beta alpha ALPHA\nalpha omega")
          (goto-char (point-min))
          (let ((initial-mode-line (copy-tree mode-line-format)))
            (anzu-mode 1)
            (isearch-mode t nil nil nil)
            (isearch-process-search-string "alpha" "alpha")
            (let ((first
                   (list
                    :point (point)
                    :line (line-number-at-pos)
                    :text
                    (buffer-substring-no-properties
                     (- (point) (length isearch-string))
                     (point))
                    :indicator
                    (substring-no-properties
                     (or
                      (eval
                       (cadr (car mode-line-format))
                       t)
                      "")))))
              (isearch-repeat 'forward)
              (let ((second
                     (list
                      :point (point)
                      :line (line-number-at-pos)
                      :text
                      (buffer-substring-no-properties
                       (- (point) (length isearch-string))
                       (point))
                      :indicator
                      (substring-no-properties
                       (or
                        (eval
                         (cadr (car mode-line-format))
                         t)
                        ""))))
                    (during-mode-line (car mode-line-format)))
                (isearch-done)
                (setq result
                      (list
                       :first first
                       :second second
                       :during-mode-line during-mode-line
                       :after-search
                       (list
                        :isearch isearch-mode
                        :anzu anzu-mode
                        :mode-line-restored
                        (equal mode-line-format initial-mode-line))))
                (anzu-mode -1)
                (setq result
                      (append
                       result
                       (list
                        :after-disable
                        (list
                         :anzu anzu-mode
                         :mode-line-restored
                         (equal mode-line-format
                                initial-mode-line)))))))))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))
    result)"##,
        expect![[
            r#"OK (:first (:point 6 :line 1 :text "alpha" :indicator "(1/4)") :second (:point 17 :line 1 :text "alpha" :indicator "(2/4)") :during-mode-line (:eval (anzu--update-mode-line)) :after-search (:isearch nil :anzu t :mode-line-restored t) :after-disable (:anzu nil :mode-line-restored t))"#
        ]],
    )
}

fn replace_at_cursor_renames_a_symbol_only_inside_the_current_defun() -> ParityBatchCase {
    ParityBatchCase::value(
        "replace_at_cursor_renames_a_symbol_only_inside_the_current_defun",
        r##"(let ((buffer
         (generate-new-buffer " *anzu-scoped-rename-workflow*"))
        result)
    (unwind-protect
        (progn
          (switch-to-buffer buffer)
          (emacs-lisp-mode)
          (insert
           "(defun deploy (target)\n"
           "  (let ((status target))\n"
           "    (message \"%s -> %s\" target status)))\n\n"
           "(setq target 'staging)\n")
          (goto-char (point-min))
          (search-forward "target")
          (backward-char 2)
          (let ((before
                 (list
                  :point (point)
                  :symbol
                  (substring-no-properties
                   (thing-at-point 'symbol))))
                (initial-mode-line (copy-tree mode-line-format))
                (anzu-replace-at-cursor-thing 'defun)
                (anzu-replace-to-string-separator " ⇒ ")
                (query-replace-history nil)
                prompts)
            (cl-letf (((symbol-function 'read-from-minibuffer)
                       (lambda (prompt &rest _arguments)
                         (push prompt prompts)
                         "environment")))
              (anzu-replace-at-cursor-thing))
            (setq result
                  (list
                   :before before
                   :after
                   (list
                    :point (point)
                    :symbol
                    (substring-no-properties
                     (thing-at-point 'symbol)))
                   :buffer (buffer-string)
                   :prompts (nreverse prompts)
                   :history query-replace-history
                   :mark (mark t)
                   :mode-line-restored
                   (equal mode-line-format initial-mode-line)
                   :stale-anzu-overlays
                   (cl-count-if
                    (lambda (overlay)
                      (overlay-get overlay 'anzu-overlay))
                    (overlays-in
                     (point-min) (point-max)))))))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))
    result)"##,
        expect![[
            r#"OK (:before (:point 20 :symbol "target") :after (:point 16 :symbol "environment") :buffer "(defun deploy (environment)\n  (let ((status environment))\n    (message \"%s -> %s\" environment status)))\n\n(setq target 'staging)\n" :prompts ("Query replace regexp \\_<target\\_> with: ") :history ("environment") :mark 1 :mode-line-restored t :stale-anzu-overlays 0)"#
        ]],
    )
}

fn regexp_isearch_flows_into_selective_capture_group_replacement() -> ParityBatchCase {
    ParityBatchCase::value(
        "regexp_isearch_flows_into_selective_capture_group_replacement",
        r##"(let ((buffer
         (generate-new-buffer " *anzu-isearch-replace-workflow*"))
        result)
    (unwind-protect
        (progn
          (switch-to-buffer buffer)
          (insert
           "INFO user=alice id=17\n"
           "WARN user=bob id=23\n"
           "INFO user=carol id=42\n"
           "INFO user=dave id=99\n")
          (goto-char (point-min))
          (let ((initial-mode-line (copy-tree mode-line-format))
                (answers '("OK id=\\2 owner=\\1"))
                (decisions '(?y ?n ?!))
                (search-regexp
                 "^INFO user=\\([[:alpha:]]+\\) id=\\([[:digit:]]+\\)$")
                (query-replace-history nil)
                (anzu--query-defaults nil)
                input-prompts
                decision-prompts)
            (anzu-mode 1)
            (isearch-mode t t nil nil)
            (isearch-process-search-string
             search-regexp search-regexp)
            (let ((search-state
                   (list
                    :point (point)
                    :line (line-number-at-pos)
                    :text
                    (buffer-substring-no-properties
                     (line-beginning-position)
                     (line-end-position))
                    :indicator
                    (substring-no-properties
                     (or
                      (eval
                       (cadr (car mode-line-format))
                       t)
                      "")))))
              (cl-letf (((symbol-function 'read-from-minibuffer)
                         (lambda (prompt &rest _arguments)
                           (push prompt input-prompts)
                           (pop answers)))
                        ((symbol-function 'read-key)
                         (lambda (prompt &rest _arguments)
                           (push
                            (substring-no-properties prompt)
                            decision-prompts)
                           (pop decisions))))
                (anzu-isearch-query-replace-regexp 1))
              (setq result
                    (list
                     :search search-state
                     :buffer (buffer-string)
                     :point (point)
                     :mark (mark t)
                     :input-prompts (nreverse input-prompts)
                     :decision-prompts
                     (nreverse decision-prompts)
                     :input-answers-left answers
                     :decisions-left decisions
                     :history query-replace-history
                     :isearch isearch-mode
                     :anzu anzu-mode
                     :mode-line-restored
                     (equal mode-line-format initial-mode-line)
                     :stale-anzu-overlays
                     (cl-count-if
                      (lambda (overlay)
                        (overlay-get overlay 'anzu-overlay))
                      (overlays-in
                       (point-min) (point-max)))))
              (anzu-mode -1))))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))
    result)"##,
        expect![[
            r#"OK (:search (:point 22 :line 1 :text "INFO user=alice id=17" :indicator "(1/3)") :buffer "OK id=17 owner=alice\nWARN user=bob id=23\nINFO user=carol id=42\nOK id=99 owner=dave\n" :point 83 :mark 1 :input-prompts ("Query replace regexp ^INFO user=\\([[:alpha:]]+\\) id=\\([[:digit:]]+\\)$ with: ") :decision-prompts ("Query replacing regexp ^INFO user=\\([[:alpha:]]+\\) id=\\([[:digit:]]+\\)$ with OK id=17 owner=alice: (? for help) " "Query replacing regexp ^INFO user=\\([[:alpha:]]+\\) id=\\([[:digit:]]+\\)$ with OK id=42 owner=carol: (? for help) " "Query replacing regexp ^INFO user=\\([[:alpha:]]+\\) id=\\([[:digit:]]+\\)$ with OK id=99 owner=dave: (? for help) ") :input-answers-left nil :decisions-left nil :history ("OK id=\\2 owner=\\1" "^INFO user=\\([[:alpha:]]+\\) id=\\([[:digit:]]+\\)$") :isearch nil :anzu t :mode-line-restored t :stale-anzu-overlays 0)"#
        ]],
    )
}

fn global_mode_covers_existing_and_future_buffers_then_stops_cleanly() -> ParityBatchCase {
    ParityBatchCase::value(
        "global_mode_covers_existing_and_future_buffers_then_stops_cleanly",
        r##"(let ((existing
         (generate-new-buffer " *anzu-global-existing*"))
        future
        disabled-future
        result)
    (unwind-protect
        (progn
          (global-anzu-mode -1)
          (with-current-buffer existing
            (text-mode)
            (insert "release candidate release notes"))
          (global-anzu-mode 1)
          (setq future
                (generate-new-buffer " *anzu-global-future*"))
          (switch-to-buffer future)
          (emacs-lisp-mode)
          (insert
           "(defconst release-channel 'stable)\n"
           "(message \"release=%s\" release-channel)\n")
          (goto-char (point-min))
          (let ((existing-enabled
                 (with-current-buffer existing anzu-mode))
                (future-enabled anzu-mode))
            (isearch-mode t nil nil nil)
            (isearch-process-search-string
             "release" "release")
            (let ((search
                   (list
                    :point (point)
                    :line (line-number-at-pos)
                    :text
                    (buffer-substring-no-properties
                     (- (point) (length isearch-string))
                     (point))
                    :indicator
                    (substring-no-properties
                     (or
                      (eval
                       (cadr (car mode-line-format))
                       t)
                      "")))))
              (isearch-done)
              (global-anzu-mode -1)
              (setq disabled-future
                    (generate-new-buffer
                     " *anzu-global-disabled-future*"))
              (with-current-buffer disabled-future
                (text-mode))
              (setq result
                    (list
                     :enabled
                     (list
                      :existing existing-enabled
                      :future future-enabled)
                     :future-buffer-search search
                     :disabled
                     (list
                      :existing
                      (with-current-buffer existing anzu-mode)
                      :future
                      (with-current-buffer future anzu-mode)
                      :new-buffer
                      (with-current-buffer
                          disabled-future
                        anzu-mode))
                     :global global-anzu-mode)))))
      (global-anzu-mode -1)
      (dolist (buffer
               (list existing future disabled-future))
        (when (buffer-live-p buffer)
          (kill-buffer buffer))))
    result)"##,
        expect![[
            r#"OK (:enabled (:existing t :future t) :future-buffer-search (:point 18 :line 1 :text "release" :indicator "(1/3)") :disabled (:existing nil :future nil :new-buffer nil) :global nil)"#
        ]],
    )
}

fn search_threshold_and_no_match_face_track_a_refined_log_query() -> ParityBatchCase {
    ParityBatchCase::value(
        "search_threshold_and_no_match_face_track_a_refined_log_query",
        r##"(let ((buffer
         (generate-new-buffer " *anzu-threshold-search-workflow*"))
        result)
    (unwind-protect
        (progn
          (switch-to-buffer buffer)
          (insert
           "ERROR api timeout\n"
           "info cache warm\n"
           "error db timeout\n"
           "Error queue timeout\n"
           "error worker timeout\n")
          (goto-char (point-min))
          (let ((anzu-search-threshold 3))
            (anzu-mode 1)
            (isearch-mode t nil nil nil)
            (setq isearch-case-fold-search t)
            (isearch-process-search-string "error" "error")
            (let* ((first-indicator
                    (eval
                     (cadr (car mode-line-format))
                     t))
                   (first
                    (list
                     :point (point)
                     :line (line-number-at-pos)
                     :indicator
                     (substring-no-properties first-indicator)
                     :face
                     (get-text-property
                      0 'face first-indicator))))
              (isearch-repeat 'forward)
              (let* ((second-indicator
                      (eval
                       (cadr (car mode-line-format))
                       t))
                     (second
                      (list
                       :point (point)
                       :line (line-number-at-pos)
                       :indicator
                       (substring-no-properties
                        second-indicator)
                       :face
                       (get-text-property
                        0 'face second-indicator))))
                (isearch-del-char
                 (length isearch-string))
                (isearch-process-search-string
                 "missing" "missing")
                (let ((missing-indicator
                       (eval
                        (cadr (car mode-line-format))
                        t)))
                  (setq result
                        (list
                         :first first
                         :second second
                         :refined-query
                         (list
                          :success isearch-success
                          :indicator
                          (substring-no-properties
                           missing-indicator)
                          :face
                          (get-text-property
                           0 'face
                           missing-indicator))))
                  (isearch-done)
                  (anzu-mode -1))))))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))
    result)"##,
        expect![[
            r#"OK (:first (:point 6 :line 1 :indicator "(1/3+)" :face anzu-mode-line) :second (:point 40 :line 3 :indicator "(2/3+)" :face anzu-mode-line) :refined-query (:success nil :indicator "(0/0)" :face anzu-mode-line-no-match))"#
        ]],
    )
}

pub(super) fn practical_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        incremental_search_reports_each_repeated_match_and_restores_the_mode_line(),
        replace_at_cursor_renames_a_symbol_only_inside_the_current_defun(),
        regexp_isearch_flows_into_selective_capture_group_replacement(),
        global_mode_covers_existing_and_future_buffers_then_stops_cleanly(),
        search_threshold_and_no_match_face_track_a_refined_log_query(),
    ]
}
