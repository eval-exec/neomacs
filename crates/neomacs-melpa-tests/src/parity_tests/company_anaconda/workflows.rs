use expect_test::expect;

use super::ParityBatchCase;

fn prefix_detection_covers_attributes_imports_spaces_numbers_comments_and_disabled_mode()
-> ParityBatchCase {
    ParityBatchCase::value(
        "prefix_detection_covers_attributes_imports_spaces_numbers_comments_and_disabled_mode",
        r####"
(let (results)
  (dolist (fixture
           '(("first.dis" t)
             ("from inventory import Wid" t)
             ("import inven" t)
             ("value = " t)
             ("123" t)
             ("0x1f" t)
             ("123." t)
             ("1.1." t)
             ("# first.dis" t)
             ("first.dis" nil)))
    (with-temp-buffer
      (python-mode)
      (insert (car fixture))
      (let ((anaconda-mode (cadr fixture)))
        (push (list (car fixture) anaconda-mode
                    (company-anaconda-prefix))
              results))))
  (nreverse results))
"####,
        expect![[
            r##"OK (("first.dis" t "first.dis") ("from inventory import Wid" t ("Wid" . t)) ("import inven" t ("inven" . t)) ("value = " t stop) ("123" t nil) ("0x1f" t nil) ("123." t nil) ("1.1." t ("1.1." . t)) ("# first.dis" t nil) ("first.dis" nil nil))"##
        ]],
    )
}

fn async_candidates_preserve_prefix_and_attach_complete_server_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_candidates_preserve_prefix_and_attach_complete_server_metadata",
        r####"
(with-temp-buffer
  (python-mode)
  (insert "first.dis")
  (let ((anaconda-mode t))
    (neomacs-company-anaconda-test-reset)
    (cl-letf (((symbol-function 'anaconda-mode-call)
               #'neomacs-company-anaconda-test-rpc))
      (let* ((prefix (company-anaconda-prefix))
             (async (company-anaconda 'candidates prefix))
             candidates)
        (funcall (cdr async) (lambda (value) (setq candidates value)))
        (while (null candidates) (accept-process-output nil 0.01))
        (list :prefix prefix
              :async (car async)
              :candidates
              (mapcar
               (lambda (candidate)
                 (list :text (substring-no-properties candidate)
                       :struct (append (get-text-property 0 'struct candidate) nil)
                       :annotation (company-anaconda 'annotation candidate)
                       :meta (company-anaconda 'meta candidate)
                       :location (company-anaconda 'location candidate)))
               candidates)
              :calls (nreverse neomacs-company-anaconda-test-calls))))))
"####,
        expect![[
            r#"OK (:prefix "first.dis" :async :async :candidates ((:text "first.discounted" :struct ("discounted" "function: inventory.Widget.discounted" "discounted(percent)\n\nReturn the discounted price." "/workspace/inventory.py" 11) :annotation "<function: inventory.Widget.discounted>" :meta "discounted(percent)" :location ("/workspace/inventory.py" . 11)) (:text "first.display_name" :struct ("display_name" "function: inventory.Widget.display_name" "display_name()\n\nReturn the visible name." "/workspace/inventory.py" 15) :annotation "<function: inventory.Widget.display_name>" :meta "display_name()" :location ("/workspace/inventory.py" . 15)) (:text "first.duplicate" :struct ("duplicate" "function: inventory.Widget.duplicate" "duplicate()\n\nReturn an independent copy." "/workspace/inventory.py" 19) :annotation "<function: inventory.Widget.duplicate>" :meta "duplicate()" :location ("/workspace/inventory.py" . 19))) :calls (("company_complete" "first.dis" 1 9)))"#
        ]],
    )
}

fn real_company_session_displays_async_candidates_and_inserts_selected_completion()
-> ParityBatchCase {
    ParityBatchCase::value(
        "real_company_session_displays_async_candidates_and_inserts_selected_completion",
        r####"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (python-mode)
    (insert "first.dis")
    (setq-local anaconda-mode t
                company-backends '(company-anaconda)
                company-frontends '(company-pseudo-tooltip-frontend)
                company-idle-delay nil)
    (neomacs-company-anaconda-test-reset)
    (cl-letf (((symbol-function 'anaconda-mode-call)
               #'neomacs-company-anaconda-test-rpc))
      (company-mode 1)
      (company-begin-backend 'company-anaconda)
      (while (null company-candidates) (accept-process-output nil 0.01))
      (let ((opened
             (list :prefix company-prefix
                   :candidates (neomacs-company-anaconda-test-plain-candidates)
                   :selection company-selection
                   :annotation (company-call-backend
                                'annotation (car company-candidates))
                   :meta (company-call-backend 'meta (car company-candidates))
                   :location (company-call-backend 'location (car company-candidates))
                   :tooltip (and (company-tooltip-visible-p) t))))
        (company-select-next 2)
        (let ((chosen (nth company-selection company-candidates)))
          (company-complete-selection)
          (list :opened opened :chosen (substring-no-properties chosen)
                :buffer (buffer-string) :point (point)
                :active (and company-candidates t)
                :calls (nreverse neomacs-company-anaconda-test-calls)))))))
"####,
        expect![[
            r#"OK (:opened (:prefix "first.dis" :candidates ("first.discounted" "first.display_name" "first.duplicate") :selection 0 :annotation "<function: inventory.Widget.discounted>" :meta "discounted(percent)" :location ("/workspace/inventory.py" . 11) :tooltip nil) :chosen "first.duplicate" :buffer "first.duplicate" :point 16 :active nil :calls (("company_complete" "first.dis" 1 9) ("company_complete" "first.duplicate" 1 15)))"#
        ]],
    )
}

fn annotations_are_customizable_and_blank_docs_or_missing_paths_return_nil() -> ParityBatchCase {
    ParityBatchCase::value(
        "annotations_are_customizable_and_blank_docs_or_missing_paths_return_nil",
        r####"
(let* ((full (neomacs-company-anaconda-test-candidate
              "Widget" "class: inventory.Widget"
              "Widget(name)\n\nA catalogue item." "/workspace/inventory.py" 4))
       (blank (neomacs-company-anaconda-test-candidate
               "empty" "statement" "   " "" nil))
       (company-anaconda-annotation-function
        (lambda (candidate)
          (let ((description (aref (get-text-property 0 'struct candidate) 1)))
            (format "<%s>" (substring description 0 1))))))
  (list
   :full (list :annotation (company-anaconda 'annotation full)
               :meta (company-anaconda 'meta full)
               :doc (with-current-buffer (company-anaconda 'doc-buffer full)
                      (list :mode major-mode
                            :read-only buffer-read-only
                            :text (buffer-substring-no-properties
                                   (point-min) (point-max))))
               :location (company-anaconda 'location full))
   :blank (list :annotation (company-anaconda 'annotation blank)
                :meta (company-anaconda 'meta blank)
                :doc (company-anaconda 'doc-buffer blank)
                :location (company-anaconda 'location blank))))
"####,
        expect![[
            r#"OK (:full (:annotation "<c>" :meta "Widget(name)" :doc (:mode fundamental-mode :read-only t :text "\nWidget(name)\n\nA catalogue item.\n\n") :location ("/workspace/inventory.py" . 4)) :blank (:annotation "<s>" :meta "   " :doc (:buffer "*Anaconda*") :location nil))"#
        ]],
    )
}

fn backend_policy_reports_case_sorting_and_empty_async_results_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "backend_policy_reports_case_sorting_and_empty_async_results_exactly",
        r####"
(with-temp-buffer
  (python-mode)
  (insert "unknown.att")
  (let ((anaconda-mode t)
        (company-anaconda-case-insensitive nil))
    (neomacs-company-anaconda-test-reset)
    (setq neomacs-company-anaconda-test-results [])
    (cl-letf (((symbol-function 'anaconda-mode-call)
               #'neomacs-company-anaconda-test-rpc))
      (let* ((prefix (company-anaconda 'prefix))
             (async (company-anaconda 'candidates prefix))
             (called nil))
        (funcall (cdr async) (lambda (value) (setq called (list :value value))))
        (while (null called) (accept-process-output nil 0.01))
        (list :prefix prefix :result called
              :ignore-case (company-anaconda 'ignore-case)
              :sorted (company-anaconda 'sorted)
              :interactive-command (commandp 'company-anaconda)
              :calls (nreverse neomacs-company-anaconda-test-calls))))))
"####,
        expect![[
            r#"OK (:prefix "unknown.att" :result (:value nil) :ignore-case nil :sorted t :interactive-command t :calls (("company_complete" "unknown.att" 1 11)))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        prefix_detection_covers_attributes_imports_spaces_numbers_comments_and_disabled_mode(),
        async_candidates_preserve_prefix_and_attach_complete_server_metadata(),
        real_company_session_displays_async_candidates_and_inserts_selected_completion(),
        annotations_are_customizable_and_blank_docs_or_missing_paths_return_nil(),
        backend_policy_reports_case_sorting_and_empty_async_results_exactly(),
    ]
}
