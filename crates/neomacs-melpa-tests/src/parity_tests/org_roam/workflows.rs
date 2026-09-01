use expect_test::expect;

use super::ParityBatchCase;

fn database_sync_builds_a_queryable_graph_from_real_org_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "database_sync_builds_a_queryable_graph_from_real_org_files",
        r##"
(neomacs-org-roam-test-with-kb
  (list
   :files
   (sort (mapcar (lambda (file) (file-relative-name file root))
                 (org-roam-list-files))
         #'string<)
   :counts
   (list :files (caar (org-roam-db-query
                       [:select (funcall count) :from files]))
         :nodes (caar (org-roam-db-query
                       [:select (funcall count) :from nodes]))
         :links (caar (org-roam-db-query
                       [:select (funcall count) :from links]))
         :aliases (caar (org-roam-db-query
                         [:select (funcall count) :from aliases]))
         :tags (caar (org-roam-db-query
                      [:select (funcall count) :from tags]))
         :refs (caar (org-roam-db-query
                      [:select (funcall count) :from refs])))
   :node-list
   (let ((nodes (org-roam-node-list)))
     (list :count (length nodes)
           :unique-ids
           (sort (seq-uniq (mapcar #'org-roam-node-id nodes))
                 #'string<)))
   :db-exists (file-exists-p org-roam-db-location)))
"##,
        expect![[
            r#"OK (:files ("alpha.org" "beta.org" "notes/gamma.org") :counts (:files 3 :nodes 4 :links 3 :aliases 3 :tags 5 :refs 1) :node-list (:count 7 :unique-ids ("alpha-id" "beta-id" "gamma-id" "milestone-id")) :db-exists t)"#
        ]],
    )
}

fn node_lookup_returns_file_heading_alias_ref_and_planning_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "node_lookup_returns_file_heading_alias_ref_and_planning_metadata",
        r##"
(neomacs-org-roam-test-with-kb
  (list
   :by-id
   (neomacs-org-roam-test-node-state
    (org-roam-node-from-id "alpha-id") root)
   :by-title
   (neomacs-org-roam-test-node-state
    (org-roam-node-from-title-or-alias "Beta") root)
   :by-alias
   (neomacs-org-roam-test-node-state
    (org-roam-node-from-title-or-alias "first note" t) root)
   :by-ref
   (neomacs-org-roam-test-node-state
    (org-roam-node-from-ref "https://example.test/alpha") root)
   :heading
   (neomacs-org-roam-test-node-state
    (org-roam-node-from-id "milestone-id") root)
   :missing
   (list (org-roam-node-from-id "missing")
         (org-roam-node-from-title-or-alias "missing")
         (org-roam-node-from-ref "https://example.test/missing"))))
"##,
        expect![[
            r#"OK (:by-id (:id "alpha-id" :title "Alpha λ" :file "alpha.org" :level 0 :todo nil :priority nil :scheduled nil :deadline nil :olp nil :tags ("project" "unicode") :aliases ("First Note" "Origin") :refs ("//example.test/alpha")) :by-title (:id "beta-id" :title "Beta" :file "beta.org" :level 0 :todo nil :priority nil :scheduled nil :deadline nil :olp nil :tags ("project") :aliases nil :refs nil) :by-alias (:id "alpha-id" :title "Alpha λ" :file "alpha.org" :level 0 :todo nil :priority nil :scheduled nil :deadline nil :olp nil :tags ("project" "unicode") :aliases ("First Note" "Origin") :refs ("//example.test/alpha")) :by-ref (:id "alpha-id" :title "Alpha λ" :file "alpha.org" :level 0 :todo nil :priority nil :scheduled nil :deadline nil :olp nil :tags ("project" "unicode") :aliases ("First Note" "Origin") :refs ("//example.test/alpha")) :heading (:id "milestone-id" :title "Milestone" :file "beta.org" :level 1 :todo "TODO" :priority nil :scheduled "2026-08-10T00:00:00" :deadline "2026-08-12T00:00:00" :olp nil :tags ("project") :aliases ("Checkpoint") :refs nil) :missing (nil nil nil))"#
        ]],
    )
}

fn backlinks_recover_all_source_nodes_and_exact_link_positions() -> ParityBatchCase {
    ParityBatchCase::value(
        "backlinks_recover_all_source_nodes_and_exact_link_positions",
        r##"
(neomacs-org-roam-test-with-kb
  (let* ((alpha (org-roam-node-from-id "alpha-id"))
         (backlinks (org-roam-backlinks-get alpha)))
    (list
     :target (org-roam-node-title alpha)
     :backlinks
     (mapcar
      (lambda (backlink)
        (list :source
              (neomacs-org-roam-test-node-state
               (org-roam-backlink-source-node backlink) root)
              :point (org-roam-backlink-point backlink)
              :properties (org-roam-backlink-properties backlink)))
      (sort backlinks
            (lambda (left right)
              (string<
               (org-roam-node-title
                (org-roam-backlink-source-node left))
               (org-roam-node-title
                (org-roam-backlink-source-node right))))))
     :unique-count
     (length (org-roam-backlinks-get alpha :unique t)))))
"##,
        expect![[
            r#"OK (:target "Alpha λ" :backlinks ((:source (:id "gamma-id" :title "Gamma" :file "notes/gamma.org" :level 0 :todo nil :priority nil :scheduled nil :deadline nil :olp nil :tags ("archive") :aliases nil :refs nil) :point 89 :properties (:outline nil)) (:source (:id "milestone-id" :title "Milestone" :file "beta.org" :level 1 :todo "TODO" :priority nil :scheduled "2026-08-10T00:00:00" :deadline "2026-08-12T00:00:00" :olp nil :tags ("project") :aliases ("Checkpoint") :refs nil) :point 236 :properties (:outline ("Milestone")))) :unique-count 2)"#
        ]],
    )
}

fn public_node_editors_update_plain_text_and_database_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_node_editors_update_plain_text_and_database_metadata",
        r##"
(neomacs-org-roam-test-with-kb
  (let* ((file (expand-file-name "alpha.org" root))
         (buffer (find-file-noselect file)))
    (unwind-protect
        (with-current-buffer buffer
          (org-mode)
          (goto-char (point-min))
          (org-roam-alias-add "Primary")
          (org-roam-alias-remove "Origin")
          (org-roam-tag-add '("active" "project"))
          (org-roam-tag-remove '("unicode"))
          (org-roam-ref-add "https://docs.example.test/alpha")
          (org-roam-ref-remove "https://example.test/alpha")
          (save-buffer)
          (org-roam-db-update-file file)
          (let ((node (org-roam-node-from-id "alpha-id")))
            (list
             :text (buffer-substring-no-properties
                    (point-min) (point-max))
             :node (neomacs-org-roam-test-node-state node root)
             :old-ref (org-roam-node-from-ref
                       "https://example.test/alpha")
             :new-ref
             (org-roam-node-id
              (org-roam-node-from-ref
               "https://docs.example.test/alpha")))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer (set-buffer-modified-p nil))
        (kill-buffer buffer)))))
"##,
        expect![[
            r#"OK (:text ":PROPERTIES:\n:ID: alpha-id\n:ROAM_ALIASES: Primary \"First Note\"\n:ROAM_REFS: https://docs.example.test/alpha\n:END:\n#+title: Alpha λ\n#+filetags: :active:project:\n\nAlpha body links to [[id:beta-id][Beta]].\n" :node (:id "alpha-id" :title "Alpha λ" :file "alpha.org" :level 0 :todo nil :priority nil :scheduled nil :deadline nil :olp nil :tags ("active" "project") :aliases ("First Note" "Primary") :refs ("//docs.example.test/alpha")) :old-ref nil :new-ref "alpha-id")"#
        ]],
    )
}

fn roam_links_replace_with_ids_and_completion_exposes_titles_and_aliases() -> ParityBatchCase {
    ParityBatchCase::value(
        "roam_links_replace_with_ids_and_completion_exposes_titles_and_aliases",
        r##"
(neomacs-org-roam-test-with-kb
  (with-temp-buffer
    (insert "See [[roam:First Note][the origin]] and [[roam:Beta]].\n")
    (org-mode)
    (org-roam-link-replace-all)
    (goto-char (point-max))
    (insert "\n[[roam:Chec]]")
    (search-backward "Chec")
    (goto-char (+ (point) 4))
    (let* ((capf (org-roam-complete-link-at-point))
           (start (nth 0 capf))
           (end (nth 1 capf))
           (table (nth 2 capf)))
      (list
       :text (buffer-substring-no-properties
              (point-min) (line-end-position 1))
       :bounds (list start end)
       :prefix (buffer-substring-no-properties start end)
       :matches (all-completions
                 (buffer-substring-no-properties start end)
                 table)
       :titles
       (sort (org-roam--get-titles) #'string<)))))
"##,
        expect![[
            r#"OK (:text "See [[id:alpha-id][the origin]] and [[id:beta-id][Beta]].\n\n[[roam:Chec]]" :bounds (67 71) :prefix "Chec" :matches ("Checkpoint") :titles ("Alpha λ" "Beta" "Checkpoint" "First Note" "Gamma" "Milestone" "Origin"))"#
        ]],
    )
}

fn dedicated_backlink_buffer_renders_real_sources_and_previews() -> ParityBatchCase {
    ParityBatchCase::value(
        "dedicated_backlink_buffer_renders_real_sources_and_previews",
        r##"
(neomacs-org-roam-test-with-kb
  (let* ((node (org-roam-node-from-id "alpha-id"))
         (org-roam-mode-sections
          '((org-roam-backlinks-section :unique t)))
         (org-roam-preview-function
          (lambda () (buffer-substring-no-properties
                      (line-beginning-position)
                      (line-end-position))))
         buffer)
    (unwind-protect
        (save-window-excursion
          (org-roam-buffer-display-dedicated node)
          (setq buffer
                (get-buffer
                 (org-roam-buffer--dedicated-name node)))
          (with-current-buffer buffer
            (list :mode major-mode
                  :dedicated (and (org-roam-buffer-dedicated-p) t)
                  :name (buffer-name)
                  :header (substring-no-properties
                           (format-mode-line header-line-format))
                  :text (buffer-substring-no-properties
                         (point-min) (point-max))
                  :current-node
                  (org-roam-node-id
                   org-roam-buffer-current-node))))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))
"##,
        expect![[r#"OK (:mode org-roam-mode :dedicated t :name "*org-roam: Alpha λ<alpha.org>*" :header "" :text "Backlinks (2)\nGamma (Top)\nGamma references [[id:alpha-id][Alpha λ]].\n\nMilestone (Milestone)\nMilestone links to [[id:alpha-id][Alpha λ]].\n\n\n" :current-node "alpha-id")"#]],
    )
    .fresh_process()
}

fn autosync_mode_registers_hooks_and_tracks_saved_note_changes() -> ParityBatchCase {
    ParityBatchCase::value(
        "autosync_mode_registers_hooks_and_tracks_saved_note_changes",
        r##"
(neomacs-org-roam-test-with-kb
  (let ((file (expand-file-name "beta.org" root))
        before enabled updated disabled)
    (unwind-protect
        (progn
          (setq before
                (org-roam-node-title
                 (org-roam-node-from-id "beta-id")))
          (org-roam-db-autosync-mode 1)
          (setq enabled
                (list :mode org-roam-db-autosync-mode
                      :find-hook
                      (and (memq #'org-roam-db-autosync--setup-file-h
                                 find-file-hook) t)
                      :rename-advice
                      (and (advice-member-p
                            #'org-roam-db-autosync--rename-file-a
                            'rename-file) t)
                      :delete-advice
                      (and (advice-member-p
                            #'org-roam-db-autosync--delete-file-a
                            'delete-file) t)))
          (let ((buffer (find-file-noselect file)))
            (unwind-protect
                (with-current-buffer buffer
                  (goto-char (point-min))
                  (re-search-forward "^#\\+title: Beta$")
                  (replace-match "#+title: Beta Updated")
                  (save-buffer))
              (when (buffer-live-p buffer)
                (with-current-buffer buffer
                  (set-buffer-modified-p nil))
                (kill-buffer buffer))))
          (setq updated
                (org-roam-node-title
                 (org-roam-node-from-id "beta-id")))
          (org-roam-db-autosync-mode -1)
          (setq disabled
                (list :mode org-roam-db-autosync-mode
                      :find-hook
                      (and (memq #'org-roam-db-autosync--setup-file-h
                                 find-file-hook) t)
                      :rename-advice
                      (and (advice-member-p
                            #'org-roam-db-autosync--rename-file-a
                            'rename-file) t)
                      :delete-advice
                      (and (advice-member-p
                            #'org-roam-db-autosync--delete-file-a
                            'delete-file) t)))
          (list :before before
                :enabled enabled
                :updated updated
                :disabled disabled))
      (when org-roam-db-autosync-mode
        (org-roam-db-autosync-mode -1)))))
"##,
        expect![[r#"OK (:before "Beta" :enabled (:mode t :find-hook t :rename-advice t :delete-advice t) :updated "Beta Updated" :disabled (:mode nil :find-hook nil :rename-advice nil :delete-advice nil))"#]],
    )
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        database_sync_builds_a_queryable_graph_from_real_org_files(),
        node_lookup_returns_file_heading_alias_ref_and_planning_metadata(),
        backlinks_recover_all_source_nodes_and_exact_link_positions(),
        public_node_editors_update_plain_text_and_database_metadata(),
        roam_links_replace_with_ids_and_completion_exposes_titles_and_aliases(),
        dedicated_backlink_buffer_renders_real_sources_and_previews(),
        autosync_mode_registers_hooks_and_tracks_saved_note_changes(),
    ]
}
