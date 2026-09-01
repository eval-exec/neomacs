use expect_test::expect;

use super::ParityBatchCase;

fn opening_dashboard_renders_and_opens_an_actionable_bookmark() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-dashboard-test-with-workspace
    "dashboard-render" "*dashboard-parity-render*"
  (let* ((alpha (expand-file-name "notes/alpha.el" root))
         (beta (expand-file-name "notes/beta.el" root))
         (plan (expand-file-name "plans/release.org" root))
         (dashboard-startupify-list
          '(dashboard-insert-banner-title
            dashboard-insert-newline
            dashboard-insert-init-info
            dashboard-insert-items))
         (dashboard-banner-logo-title "Engineering Workspace")
         (dashboard-init-info "3 projects indexed; build is green")
         (dashboard-items '((recents . 2) (bookmarks . 2)))
         (recentf-list (list alpha beta))
         (bookmark-alist
          `(("Release plan" (filename . ,plan) (position . 1)))))
    (make-directory (file-name-directory alpha) t)
    (make-directory (file-name-directory plan) t)
    (with-temp-file alpha (insert "(message \"alpha\")\n"))
    (with-temp-file beta (insert "(message \"beta\")\n"))
    (with-temp-file plan
      (insert "* Release plan\n"
              "** Ship Neomacs\n"
              "- [X] parity gate\n"))
    (dashboard-open)
    (let (dashboard-state bookmark-buffer)
      (with-current-buffer dashboard-buffer-name
        (goto-char (point-min))
        (search-forward "alpha.el")
        (let* ((alpha-position (match-beginning 0))
               (alpha-properties
                (list :button (and (get-char-property alpha-position 'button) t)
                      :face (get-char-property alpha-position 'face)
                      :mouse-face (get-char-property alpha-position 'mouse-face)
                      :path (file-relative-name
                             (get-char-property alpha-position 'dashboard-path)
                             root))))
          (goto-char (point-min))
          (search-forward "Release plan")
          (let ((bookmark-position (match-beginning 0)))
            (setq dashboard-state
                  (list
                   :selected
                   (eq (current-buffer) (window-buffer (selected-window)))
                   :mode major-mode
                   :mode-name mode-name
                   :read-only buffer-read-only
                   :undo-disabled (eq buffer-undo-list t)
                   :truncate-lines truncate-lines
                   :revert-function revert-buffer-function
                   :keys (list (key-binding (kbd "RET"))
                               (key-binding (kbd "TAB"))
                               (key-binding (kbd "<delete>")))
                   :text (buffer-substring-no-properties
                          (point-min) (point-max))
                   :alpha alpha-properties
                   :bookmark
                   (list :button
                         (and (get-char-property bookmark-position 'button) t)
                         :face (get-char-property bookmark-position 'face)
                         :name (get-char-property
                                bookmark-position 'dashboard-bookmarks-name)
                         :path (file-relative-name
                                (get-char-property
                                 bookmark-position 'dashboard-path)
                                root))))
            (beginning-of-line)
            (dashboard-return)
            (setq bookmark-buffer (window-buffer (selected-window))))))
      (with-current-buffer bookmark-buffer
        (list :dashboard dashboard-state
              :activated-bookmark
              (list :selected
                    (eq bookmark-buffer (window-buffer (selected-window)))
                    :file (file-relative-name buffer-file-name root)
                    :mode major-mode
                    :point (point)
                    :text (buffer-substring-no-properties
                           (point-min) (point-max))))))))
"####;
    let expected = expect![[
        r#"OK (:dashboard (:selected t :mode dashboard-mode :mode-name "Dashboard" :read-only t :undo-disabled t :truncate-lines t :revert-function dashboard-refresh-buffer :keys (dashboard-return widget-forward dashboard-remove-item-under) :text "Engineering Workspace\n\n3 projects indexed; build is green\n\nRecent Files:\n    alpha.el\n    beta.el\n\nBookmarks:\n    Release plan\n\n" :alpha (:button t :face dashboard-items-face :mouse-face (highlight) :path "notes/alpha.el") :bookmark (:button t :face dashboard-items-face :name "Release plan" :path "plans/release.org")) :activated-bookmark (:selected t :file "plans/release.org" :mode org-mode :point 1 :text "* Release plan\n** Ship Neomacs\n- [X] parity gate\n"))"#
    ]];
    ParityBatchCase::value(
        "opening_dashboard_renders_and_opens_an_actionable_bookmark",
        elisp_form,
        expected,
    )
}

fn return_on_a_recent_file_opens_the_selected_document() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-dashboard-test-with-workspace
    "dashboard-open-recent" "*dashboard-parity-open-recent*"
  (let* ((document (expand-file-name "notes/incident-review.org" root))
         (dashboard-startupify-list '(dashboard-insert-items))
         (dashboard-items '((recents . 5)))
         (recentf-list (list document)))
    (make-directory (file-name-directory document) t)
    (with-temp-file document
      (insert "* Incident review\n"
              "- Owner: Platform\n"
              "- Status: resolved\n"))
    (dashboard-open)
    (let (visited-buffer)
      (with-current-buffer dashboard-buffer-name
        (goto-char (point-min))
        (search-forward "incident-review.org")
        (beginning-of-line)
        (dashboard-return)
        (setq visited-buffer (window-buffer (selected-window))))
      (with-current-buffer visited-buffer
        (list :selected-document
              (eq visited-buffer (window-buffer (selected-window)))
              :visited-file (file-relative-name buffer-file-name root)
              :mode major-mode
              :point (point)
              :text (buffer-substring-no-properties (point-min) (point-max))
              :dashboard-still-live
              (and (buffer-live-p (get-buffer dashboard-buffer-name)) t))))))
"####;
    let expected = expect![[
        r#"OK (:selected-document t :visited-file "notes/incident-review.org" :mode org-mode :point 1 :text "* Incident review\n- Owner: Platform\n- Status: resolved\n" :dashboard-still-live t)"#
    ]];
    ParityBatchCase::value(
        "return_on_a_recent_file_opens_the_selected_document",
        elisp_form,
        expected,
    )
}

fn refreshing_dashboard_replaces_stale_workspace_data_in_place() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-dashboard-test-with-workspace
    "dashboard-refresh" "*dashboard-parity-refresh*"
  (let* ((old-file (expand-file-name "old-task.el" root))
         (new-file (expand-file-name "new-task.el" root))
         (dashboard-startupify-list '(dashboard-insert-items))
         (dashboard-items '((recents . 3)))
         (recentf-list (list old-file)))
    (with-temp-file old-file (insert "old\n"))
    (with-temp-file new-file (insert "new\n"))
    (dashboard-open)
    (let* ((dashboard-buffer (get-buffer dashboard-buffer-name))
           (before
            (with-current-buffer dashboard-buffer
              (buffer-substring-no-properties (point-min) (point-max)))))
      (setq recentf-list (list new-file old-file))
      (dashboard-refresh-buffer)
      (with-current-buffer dashboard-buffer-name
        (list :same-buffer (eq dashboard-buffer (current-buffer))
              :selected (eq dashboard-buffer (window-buffer (selected-window)))
              :before before
              :after (buffer-substring-no-properties (point-min) (point-max))
              :new-item-path
              (progn
                (goto-char (point-min))
                (search-forward "new-task.el")
                (file-relative-name
                 (get-char-property (match-beginning 0) 'dashboard-path)
                 root))
              :mode major-mode
              :read-only buffer-read-only)))))
"####;
    let expected = expect![[
        r#"OK (:same-buffer t :selected t :before "\n\nRecent Files:\n    old-task.el\n\n" :after "\n\nRecent Files:\n    new-task.el\n    old-task.el\n\n" :new-item-path "new-task.el" :mode dashboard-mode :read-only t)"#
    ]];
    ParityBatchCase::value(
        "refreshing_dashboard_replaces_stale_workspace_data_in_place",
        elisp_form,
        expected,
    )
}

fn deleting_a_recent_item_updates_rendering_and_persisted_history() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-dashboard-test-with-workspace
    "dashboard-remove" "*dashboard-parity-remove*"
  (let* ((obsolete (expand-file-name "obsolete.el" root))
         (active (expand-file-name "active.el" root))
         (dashboard-startupify-list '(dashboard-insert-items))
         (dashboard-items '((recents . 5)))
         (recentf-list (list obsolete active)))
    (with-temp-file obsolete (insert "obsolete\n"))
    (with-temp-file active (insert "active\n"))
    (dashboard-open)
    (with-current-buffer dashboard-buffer-name
      (goto-char (point-min))
      (search-forward "obsolete.el")
      (beginning-of-line)
      (dashboard-remove-item-under)
      (let ((persisted-recent-files
             (let ((recentf-list nil))
               (load recentf-save-file nil t t)
               (mapcar (lambda (file) (file-relative-name file root))
                       recentf-list))))
        (list :recent-files
              (mapcar (lambda (file) (file-relative-name file root))
                      recentf-list)
              :persisted-file-readable (and (file-readable-p recentf-save-file) t)
              :persisted-recent-files persisted-recent-files
              :text (buffer-substring-no-properties (point-min) (point-max))
              :obsolete-visible
              (and (save-excursion
                     (goto-char (point-min))
                     (search-forward "obsolete.el" nil t))
                   t)
              :active-visible
              (and (save-excursion
                     (goto-char (point-min))
                     (search-forward "active.el" nil t))
                   t)
              :line (line-number-at-pos)
              :mode major-mode
              :read-only buffer-read-only)))))
"####;
    let expected = expect![[
        r#"OK (:recent-files ("active.el") :persisted-file-readable t :persisted-recent-files ("active.el") :text "\n\nRecent Files:\n    active.el\n\n" :obsolete-visible nil :active-visible t :line 4 :mode dashboard-mode :read-only t)"#
    ]];
    ParityBatchCase::value(
        "deleting_a_recent_item_updates_rendering_and_persisted_history",
        elisp_form,
        expected,
    )
}

fn empty_sources_render_clear_empty_states_without_dead_buttons() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-dashboard-test-with-workspace
    "dashboard-empty" "*dashboard-parity-empty*"
  (let ((dashboard-startupify-list
         '(dashboard-insert-banner-title dashboard-insert-items))
        (dashboard-banner-logo-title "New workspace")
        (dashboard-items '((recents . 5) (bookmarks . 5))))
    (dashboard-open)
    (with-current-buffer dashboard-buffer-name
      (let ((position (point-min))
            (buttons 0))
        (while (< position (point-max))
          (when (get-char-property position 'button)
            (setq buttons (1+ buttons)))
          (setq position (next-single-property-change
                          position 'button nil (point-max))))
        (list :text (buffer-substring-no-properties (point-min) (point-max))
              :empty-markers (how-many "--- No items ---"
                                       (point-min) (point-max))
              :buttons buttons
              :mode major-mode
              :read-only buffer-read-only)))))
"####;
    let expected = expect![[
        r#"OK (:text "New workspace\n\n\nRecent Files:\n    --- No items ---\n\nBookmarks:\n    --- No items ---\n\n" :empty-markers 2 :buttons 0 :mode dashboard-mode :read-only t)"#
    ]];
    ParityBatchCase::value(
        "empty_sources_render_clear_empty_states_without_dead_buttons",
        elisp_form,
        expected,
    )
}

fn startup_hooks_render_select_and_announce_the_dashboard() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-dashboard-test-with-workspace
    "dashboard-startup" "*dashboard-parity-startup*"
  (let ((dashboard-startupify-list
         '(dashboard-insert-banner-title
           dashboard-insert-newline
           dashboard-insert-init-info))
        (dashboard-banner-logo-title "Engineering start")
        (dashboard-init-info "Bootstrap complete")
        (dashboard-items nil)
        (command-line-args '("neomacs"))
        (window-size-change-functions nil)
        (window-setup-hook nil)
        (after-init-hook nil)
        (emacs-startup-hook nil)
        events)
    (let ((dashboard-before-initialize-hook
           (list (lambda () (push 'before events))))
          (dashboard-after-initialize-hook
           (list (lambda () (push 'after events)))))
      (dashboard-setup-startup-hook)
      (let ((installed
             (list
              :resize
              (and (memq 'dashboard-resize-on-hook
                         (flatten-tree window-size-change-functions))
                   t)
              :window-setup
              (and (memq 'dashboard-resize-on-hook window-setup-hook) t)
              :after-init
              (and (memq 'dashboard-insert-startupify-lists after-init-hook) t)
              :startup
              (and (memq 'dashboard-initialize emacs-startup-hook) t))))
        (run-hooks 'after-init-hook)
        (let ((rendered-before-selection
               (with-current-buffer dashboard-buffer-name
                 (list :text
                       (buffer-substring-no-properties (point-min) (point-max))
                       :mode major-mode
                       :read-only buffer-read-only
                       :selected
                       (eq (current-buffer)
                           (window-buffer (selected-window)))))))
          (run-hooks 'emacs-startup-hook)
          (with-current-buffer dashboard-buffer-name
            (list :installed installed
                  :rendered-before-selection rendered-before-selection
                  :events (nreverse events)
                  :selected
                  (eq (current-buffer) (window-buffer (selected-window)))
                  :point (point)
                  :text (buffer-substring-no-properties
                         (point-min) (point-max))
                  :mode major-mode
                  :read-only buffer-read-only)))))))
"####;
    let expected = expect![[
        r#"OK (:installed (:resize t :window-setup t :after-init t :startup t) :rendered-before-selection (:text "Engineering start\n\nBootstrap complete" :mode dashboard-mode :read-only t :selected nil) :events (before after) :selected t :point 1 :text "Engineering start\n\nBootstrap complete" :mode dashboard-mode :read-only t)"#
    ]];
    ParityBatchCase::value(
        "startup_hooks_render_select_and_announce_the_dashboard",
        elisp_form,
        expected,
    )
}

fn invalid_init_info_fails_while_opening_the_public_dashboard() -> ParityBatchCase {
    let elisp_form = r####"
(let ((dashboard-buffer-name "*dashboard-parity-invalid-init*")
      (dashboard-startupify-list '(dashboard-insert-init-info))
      (dashboard-init-info '(not-a-string-or-function))
      (dashboard-items nil)
      (dashboard-center-content nil)
      (dashboard-vertically-center-content nil)
      (inhibit-startup-screen inhibit-startup-screen))
  (dashboard-open))
"####;
    let expected = expect![[
        r#"ERR (user-error "Unknown init info type (cons): (not-a-string-or-function)")"#
    ]];
    ParityBatchCase::signal(
        "invalid_init_info_fails_while_opening_the_public_dashboard",
        elisp_form,
        expected,
    )
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opening_dashboard_renders_and_opens_an_actionable_bookmark(),
        return_on_a_recent_file_opens_the_selected_document(),
        refreshing_dashboard_replaces_stale_workspace_data_in_place(),
        deleting_a_recent_item_updates_rendering_and_persisted_history(),
        empty_sources_render_clear_empty_states_without_dead_buttons(),
        startup_hooks_render_select_and_announce_the_dashboard(),
        invalid_init_info_fails_while_opening_the_public_dashboard(),
    ]
}
