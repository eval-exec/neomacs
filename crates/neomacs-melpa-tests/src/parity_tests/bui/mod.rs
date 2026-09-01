use std::time::Duration;

use crate::{BUI_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const BUI_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const BUI_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'bui)

(defvar neomacs-bui-test-services
  '(((id . api)
     (name . "API Gateway")
     (owner . platform)
     (replicas . 3)
     (healthy . t)
     (url . "https://status.example/api")
     (notes . "Routes public traffic and validates access tokens."))
    ((id . billing)
     (name . "Billing Worker")
     (owner . finance)
     (replicas . 2)
     (healthy)
     (url . "https://status.example/billing")
     (notes . "Consumes invoice jobs from the durable queue."))
    ((id . search)
     (name . "Search Indexer")
     (owner . data)
     (replicas . 1)
     (healthy . t)
     (url . "https://status.example/search")
     (notes . "Builds the customer-facing document index."))))

(defvar neomacs-bui-test-action-log nil)

(defun neomacs-bui-test-get-entries (&optional search-type &rest values)
  "Return deterministic service entries selected by SEARCH-TYPE and VALUES."
  (pcase search-type
    ((pred null) neomacs-bui-test-services)
    ('all neomacs-bui-test-services)
    ('id (bui-entries-by-ids neomacs-bui-test-services values))
    (_ (error "Unknown service search: %S" search-type))))

(defun neomacs-bui-test-list-owner (owner &optional _entry)
  "Render OWNER as an upper-case dashboard team label."
  (upcase (symbol-name owner)))

(defun neomacs-bui-test-record-action (button)
  "Record the operational action represented by BUTTON."
  (push (list :action (button-get button 'operation)
              :service (button-get button 'service)
              :label (button-label button))
        neomacs-bui-test-action-log))

(defun neomacs-bui-test-info-insert-operations (entry)
  "Insert operational controls for service ENTRY."
  (bui-info-insert-title-format "Operations")
  (bui-insert-action-button
   "Restart" #'neomacs-bui-test-record-action "Restart this service"
   'operation 'restart
   'service (bui-entry-id entry))
  (bui-newline))

(defun neomacs-bui-test-describe (&rest ids)
  "Display detail records for service IDS."
  (bui-get-display-entries 'neomacs-bui-service 'info (cons 'id ids)))

(defun neomacs-bui-test-list-rows ()
  "Return stable IDs, marks, and visible text from the dashboard."
  (save-excursion
    (goto-char (point-min))
    (let (rows)
      (while (not (eobp))
        (push (list :id (bui-list-current-id)
                    :mark (bui-list-current-mark-name)
                    :text (buffer-substring-no-properties
                           (line-beginning-position) (line-end-position)))
              rows)
        (forward-line 1))
      (nreverse rows))))

(defun neomacs-bui-test-buttons ()
  "Return stable public properties for every button in the current buffer."
  (save-excursion
    (goto-char (point-min))
    (let (buttons button)
      (while (setq button (forward-button 1 nil nil t))
        (push (list :label (button-label button)
                    :type (button-type button)
                    :help (button-get button 'help-echo))
              buttons))
      (nreverse buttons))))

(defun neomacs-bui-test-kill-buffers ()
  "Remove deterministic BUI fixture buffers between probes."
  (dolist (name '("*Neomacs Service Dashboard*"
                  "*Neomacs Service Detail*"))
    (when-let* ((buffer (get-buffer name)))
      (kill-buffer buffer))))

(bui-define-entry-type neomacs-bui-service
  :titles '((name . "Service")
            (owner . "Team")
            (replicas . "Pods")
            (healthy . "Healthy")
            (url . "Status URL")
            (notes . "Runbook Notes"))
  :boolean-params '(healthy)
  :get-entries-function #'neomacs-bui-test-get-entries)

(bui-define-interface neomacs-bui-service list
  :buffer-name "*Neomacs Service Dashboard*"
  :get-entries-function #'neomacs-bui-test-get-entries
  :describe-function #'neomacs-bui-test-describe
  :format '((name nil 18 t)
            (owner neomacs-bui-test-list-owner 10 t)
            (replicas nil 6 bui-list-sort-numerically-2 :right-align t)
            (healthy nil 8 t)
            (url bui-list-get-url 34 t))
  :marks '((restart . ?R))
  :sort-key '(name))

(bui-define-interface neomacs-bui-service info
  :buffer-name "*Neomacs Service Detail*"
  :get-entries-function #'neomacs-bui-test-get-entries
  :delimiter "\n--- next service ---\n"
  :fill nil
  :format '((name format (simple mode-line-buffer-id))
            (owner format (simple))
            (replicas format (simple))
            (healthy format (simple))
            (url format (simple bui-url))
            (notes format (format))
            neomacs-bui-test-info-insert-operations))
"##;

fn bui_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(BUI_MELPA_PIN, "bui.el")
        .expect("prepare pinned BUI source below ./tmp")
        .with_prelude(BUI_TEST_PRELUDE)
        .with_timeout(BUI_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed BUI parity test").into()
}

pub(crate) fn assert_bui_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(bui_oracle(), &name, "bui_parity", cases);
}

#[test]
fn bui_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_bui_batch(&cases);
}
