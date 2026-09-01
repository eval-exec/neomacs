use expect_test::expect;

use super::ParityBatchCase;

fn a_service_dashboard_renders_sorted_records_columns_and_status_links() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_service_dashboard_renders_sorted_records_columns_and_status_links",
        r##"
(save-window-excursion
  (unwind-protect
      (progn
        (neomacs-bui-test-kill-buffers)
        (bui-list-get-display-entries 'neomacs-bui-service)
        (with-current-buffer "*Neomacs Service Dashboard*"
          (list :buffer (buffer-name)
                :mode major-mode
                :read-only buffer-read-only
                :sort tabulated-list-sort-key
                :columns (mapcar #'car (append tabulated-list-format nil))
                :rows (neomacs-bui-test-list-rows)
                :buttons (neomacs-bui-test-buttons))))
    (neomacs-bui-test-kill-buffers)))
"##,
        expect![[
            r##"OK (:buffer "*Neomacs Service Dashboard*" :mode neomacs-bui-service-list-mode :read-only t :sort ("Service") :columns ("Service" "Team" "Pods" "Healthy" "Status URL") :rows ((:id api :mark empty :text "  API Gateway        PLATFORM        3 Yes      https://status.example/api") (:id billing :mark empty :text "  Billing Worker     FINANCE         2 No       https://status.example/billing") (:id search :mark empty :text "  Search Indexer     DATA            1 Yes      https://status.example/search")) :buttons ((:label "https://status.example/api" :type bui-url :help "Browse URL") (:label "https://status.example/billing" :type bui-url :help "Browse URL") (:label "https://status.example/search" :type bui-url :help "Browse URL")))"##
        ]],
    )
}

fn operators_mark_services_sort_by_capacity_and_open_a_combined_detail_view() -> ParityBatchCase {
    ParityBatchCase::value(
        "operators_mark_services_sort_by_capacity_and_open_a_combined_detail_view",
        r##"
(save-window-excursion
  (unwind-protect
      (progn
        (neomacs-bui-test-kill-buffers)
        (bui-list-get-display-entries 'neomacs-bui-service)
        (with-current-buffer "*Neomacs Service Dashboard*"
          (goto-char (point-min))
          (bui-list--mark 'general t 'incident 418)
          (bui-list--mark 'restart t 'incident 418)
          (bui-list-sort 2)
          (let ((ascending (neomacs-bui-test-list-rows))
                (marked (bui-list-get-marked)))
            (bui-list-sort 2)
            (let ((descending (neomacs-bui-test-list-rows)))
              (bui-list-describe 'general 'restart)
              (with-current-buffer "*Neomacs Service Detail*"
                (list :ascending ascending
                      :descending descending
                      :marked marked
                      :detail-ids (mapcar #'bui-entry-id
                                          (bui-current-entries))
                      :detail-text (buffer-substring-no-properties
                                    (point-min) (point-max))
                      :detail-buttons (neomacs-bui-test-buttons)))))))
    (neomacs-bui-test-kill-buffers)))
"##,
        expect![[
            r##"OK (:ascending ((:id search :mark empty :text "  Search Indexer     DATA            1 Yes      https://status.example/search") (:id billing :mark restart :text "R Billing Worker     FINANCE         2 No       https://status.example/billing") (:id api :mark general :text "* API Gateway        PLATFORM        3 Yes      https://status.example/api")) :descending ((:id api :mark general :text "* API Gateway        PLATFORM        3 Yes      https://status.example/api") (:id billing :mark restart :text "R Billing Worker     FINANCE         2 No       https://status.example/billing") (:id search :mark empty :text "  Search Indexer     DATA            1 Yes      https://status.example/search")) :marked ((billing restart incident 418) (api general incident 418)) :detail-ids (api billing) :detail-text "Service           : API Gateway\nTeam              : platform\nPods              : 3\nHealthy           : Yes\nStatus URL        : https://status.example/api\nRunbook Notes     : Routes public traffic and validates access tokens.\nOperations        : Restart\n\n--- next service ---\nService           : Billing Worker\nTeam              : finance\nPods              : 2\nHealthy           : No\nStatus URL        : https://status.example/billing\nRunbook Notes     : Consumes invoice jobs from the durable queue.\nOperations        : Restart\n" :detail-buttons ((:label "https://status.example/api" :type bui-url :help "Browse URL") (:label "Restart" :type bui-action :help "Restart this service") (:label "https://status.example/billing" :type bui-url :help "Browse URL") (:label "Restart" :type bui-action :help "Restart this service")))"##
        ]],
    )
}

fn filtering_and_revert_refetch_live_records_without_losing_the_filter() -> ParityBatchCase {
    ParityBatchCase::value(
        "filtering_and_revert_refetch_live_records_without_losing_the_filter",
        r##"
(save-window-excursion
  (let ((original neomacs-bui-test-services))
    (unwind-protect
        (progn
          (neomacs-bui-test-kill-buffers)
          (bui-list-get-display-entries 'neomacs-bui-service)
          (with-current-buffer "*Neomacs Service Dashboard*"
            (bui-filter-current-entries
             (lambda (entry) (bui-entry-value entry 'healthy)))
            (let ((initial (neomacs-bui-test-list-rows)))
              (setq neomacs-bui-test-services
                    (append
                     (mapcar
                      (lambda (entry)
                        (if (eq (bui-entry-id entry) 'api)
                            (cons '(replicas . 5)
                                  (assq-delete-all 'replicas
                                                   (copy-tree entry)))
                          entry))
                      neomacs-bui-test-services)
                     '(((id . worker)
                        (name . "Queue Worker")
                        (owner . platform)
                        (replicas . 4)
                        (healthy . t)
                        (url . "https://status.example/worker")
                        (notes . "Drains background jobs.")))))
              (revert-buffer nil t)
              (let ((after-revert (neomacs-bui-test-list-rows))
                    (active-filter-count
                     (length bui-active-filter-predicates)))
                (bui-disable-filters)
                (list :initial initial
                      :after-revert after-revert
                      :active-filter-count active-filter-count
                      :after-disable (neomacs-bui-test-list-rows))))))
      (setq neomacs-bui-test-services original)
      (neomacs-bui-test-kill-buffers))))
"##,
        expect![[
            r##"OK (:initial ((:id api :mark empty :text "  API Gateway        PLATFORM        3 Yes      https://status.example/api") (:id search :mark empty :text "  Search Indexer     DATA            1 Yes      https://status.example/search")) :after-revert ((:id api :mark empty :text "  API Gateway        PLATFORM        5 Yes      https://status.example/api") (:id worker :mark empty :text "  Queue Worker       PLATFORM        4 Yes      https://status.example/worker") (:id search :mark empty :text "  Search Indexer     DATA            1 Yes      https://status.example/search")) :active-filter-count 1 :after-disable ((:id api :mark empty :text "  API Gateway        PLATFORM        5 Yes      https://status.example/api") (:id billing :mark empty :text "  Billing Worker     FINANCE         2 No       https://status.example/billing") (:id worker :mark empty :text "  Queue Worker       PLATFORM        4 Yes      https://status.example/worker") (:id search :mark empty :text "  Search Indexer     DATA            1 Yes      https://status.example/search")))"##
        ]],
    )
}

fn detail_actions_and_back_forward_history_follow_the_user_journey() -> ParityBatchCase {
    ParityBatchCase::value(
        "detail_actions_and_back_forward_history_follow_the_user_journey",
        r##"
(save-window-excursion
  (unwind-protect
      (progn
        (neomacs-bui-test-kill-buffers)
        (setq neomacs-bui-test-action-log nil)
        (bui-get-display-entries 'neomacs-bui-service 'info '(id api))
        (with-current-buffer "*Neomacs Service Detail*"
          (let ((api-text (buffer-substring-no-properties
                           (point-min) (point-max))))
            (bui-get-display-entries-current
             'neomacs-bui-service 'info '(id billing))
            (let* ((billing-text (buffer-substring-no-properties
                                  (point-min) (point-max)))
                   (restart (save-excursion
                              (goto-char (point-min))
                              (let (button found)
                                (while (and (not found)
                                            (setq button
                                                  (forward-button 1 nil nil t)))
                                  (when (equal (button-label button) "Restart")
                                    (setq found button)))
                                found))))
              (button-activate restart)
              (bui-button-copy-label (button-start restart))
              (let ((copied (current-kill 0 t))
                    (billing-buttons (neomacs-bui-test-buttons))
                    (back-depth (length bui-history-back-stack)))
                (bui-history-back)
                (let ((back-text (buffer-substring-no-properties
                                  (point-min) (point-max)))
                      (back-buttons (neomacs-bui-test-buttons)))
                  (bui-history-forward)
                  (list :api-text api-text
                        :billing-text billing-text
                        :billing-buttons billing-buttons
                        :back-depth back-depth
                        :action-log neomacs-bui-test-action-log
                        :copied copied
                        :back-text back-text
                        :back-buttons back-buttons
                        :forward-service
                        (bui-entry-id (car (bui-current-entries))))))))))
    (neomacs-bui-test-kill-buffers)))
"##,
        expect![[
            r##"OK (:api-text "Service           : API Gateway\nTeam              : platform\nPods              : 3\nHealthy           : Yes\nStatus URL        : https://status.example/api\nRunbook Notes     : Routes public traffic and validates access tokens.\nOperations        : Restart\n" :billing-text "Service           : Billing Worker\nTeam              : finance\nPods              : 2\nHealthy           : No\nStatus URL        : https://status.example/billing\nRunbook Notes     : Consumes invoice jobs from the durable queue.\nOperations        : Restart\n\n[back]\n" :billing-buttons ((:label "https://status.example/billing" :type bui-url :help "Browse URL") (:label "Restart" :type bui-action :help "Restart this service") (:label "[back]" :type bui-history :help "Go back to the previous info")) :back-depth 1 :action-log ((:action restart :service billing :label "Restart")) :copied "Restart" :back-text "Service           : API Gateway\nTeam              : platform\nPods              : 3\nHealthy           : Yes\nStatus URL        : https://status.example/api\nRunbook Notes     : Routes public traffic and validates access tokens.\nOperations        : Restart\n\n[forward]\n" :back-buttons ((:label "https://status.example/api" :type bui-url :help "Browse URL") (:label "Restart" :type bui-action :help "Restart this service") (:label "[forward]" :type bui-history :help "Go forward to the next info")) :forward-service billing)"##
        ]],
    )
}

fn an_empty_dashboard_rejects_entry_actions_at_the_command_boundary() -> ParityBatchCase {
    ParityBatchCase::signal(
        "an_empty_dashboard_rejects_entry_actions_at_the_command_boundary",
        r##"
(with-temp-buffer
  (neomacs-bui-service-list-mode)
  (bui-list-current-id))
"##,
        expect![[r##"ERR (user-error "No entry here")"##]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        a_service_dashboard_renders_sorted_records_columns_and_status_links(),
        operators_mark_services_sort_by_capacity_and_open_a_combined_detail_view(),
        filtering_and_revert_refetch_live_records_without_losing_the_filter(),
        detail_actions_and_back_forward_history_follow_the_user_journey(),
        an_empty_dashboard_rejects_entry_actions_at_the_command_boundary(),
    ]
}
