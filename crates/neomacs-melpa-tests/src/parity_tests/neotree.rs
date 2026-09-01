use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, NEOTREE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'neotree)

(defun neomacs-neotree-test-reset ()
  "Remove NeoTree's global UI state between shared-process parity cases."
  (when neo-global--autorefresh-timer
    (cancel-timer neo-global--autorefresh-timer))
  (setq neo-global--autorefresh-timer nil)
  (when (window-live-p neo-global--window)
    (ignore-errors (delete-window neo-global--window)))
  (when (buffer-live-p neo-global--buffer)
    (kill-buffer neo-global--buffer))
  (setq neo-global--window nil
        neo-global--buffer nil)
  (delete-other-windows))

(defun neomacs-neotree-test-clean-file-buffers (root)
  "Kill unmodified buffers visiting files below ROOT."
  (dolist (buffer (buffer-list))
    (with-current-buffer buffer
      (when (and buffer-file-name
                 (file-in-directory-p buffer-file-name root))
        (set-buffer-modified-p nil)
        (kill-buffer buffer)))))

(defun neomacs-neotree-test-relative (root path)
  "Return PATH relative to ROOT without leaking a random sandbox suffix."
  (cond
   ((null path) nil)
   ((file-equal-p (directory-file-name path) (directory-file-name root)) ".")
   (t (directory-file-name (file-relative-name path root)))))

(defun neomacs-neotree-test-current-node (root)
  "Return the selected NeoTree node relative to ROOT."
  (neomacs-neotree-test-relative
   root (neo-buffer--get-filename-current-line)))

(defun neomacs-neotree-test-snapshot (root)
  "Describe the rendered NeoTree nodes, buttons, and selection below ROOT."
  (with-current-buffer (neo-global--get-buffer)
    (let (entries)
      (save-excursion
        (goto-char (point-min))
        (dotimes (index (length neo-buffer--node-list))
          (let* ((path (aref neo-buffer--node-list index))
                 (button (neo-buffer--get-button-current-line))
                 (line (buffer-substring-no-properties
                        (line-beginning-position) (line-end-position))))
            (when path
              (push
               (list :node (neomacs-neotree-test-relative root path)
                     :line (if (= index 0) "<root>" line)
                     :face (and button (button-get button 'face))
                     :kind (cond
                            ((and button
                                  (eq (button-get button 'keymap)
                                      neotree-dir-button-keymap))
                             'directory)
                            ((and button
                                  (eq (button-get button 'keymap)
                                      neotree-file-button-keymap))
                             'file)
                            (t 'root)))
               entries)))
          (forward-line 1)))
      (list :mode major-mode
            :selected (neomacs-neotree-test-current-node root)
            :entries (nreverse entries)
            :expanded
            (sort
             (mapcar (lambda (path)
                       (neomacs-neotree-test-relative root path))
                     neo-buffer--expanded-node-list)
             #'string<)))))
"####;

fn renders_and_expands_a_real_project_tree_with_node_metadata() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-neotree-test-reset)
  (let* ((root (make-temp-file "neotree-project-" t))
         (src (expand-file-name "src" root))
         (docs (expand-file-name "docs" root))
         (core (expand-file-name "core λ.rs" src))
         (neo-theme 'ascii)
         (neo-show-updir-line nil)
         (neo-banner-message nil)
         (neo-vc-integration nil))
    (unwind-protect
        (progn
          (make-directory src)
          (make-directory docs)
          (with-temp-file core (insert "pub fn ship() {}\n"))
          (with-temp-file (expand-file-name "guide.md" docs)
            (insert "# Release guide\n"))
          (with-temp-file (expand-file-name "README.md" root)
            (insert "Release workspace\n"))
          (neotree-dir root)
          (let ((collapsed (neomacs-neotree-test-snapshot root)))
            (neotree-find src)
            (neotree-enter)
            (neotree-find core)
            (list :collapsed collapsed
                  :expanded (neomacs-neotree-test-snapshot root)
                  :default-directory
                  (neomacs-neotree-test-relative root default-directory))))
      (neomacs-neotree-test-reset)
      (neomacs-neotree-test-clean-file-buffers root)
      (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:collapsed (:mode neotree-mode :selected nil :entries ((:node "." :line "<root>" :face nil :kind root) (:node "docs" :line "+ docs/" :face neo-dir-link-face :kind directory) (:node "src" :line "+ src/" :face neo-dir-link-face :kind directory) (:node "README.md" :line "README.md" :face neo-file-link-face :kind file)) :expanded nil) :expanded (:mode neotree-mode :selected "src/core λ.rs" :entries ((:node "." :line "<root>" :face nil :kind root) (:node "docs" :line "+ docs/" :face neo-dir-link-face :kind directory) (:node "src" :line "- src/" :face neo-dir-link-face :kind directory) (:node "src/core λ.rs" :line "  core λ.rs" :face neo-file-link-face :kind file) (:node "README.md" :line "README.md" :face neo-file-link-face :kind file)) :expanded ("." "." "src" "src")) :default-directory "src")"#
    ]];
    ParityBatchCase::value(
        "renders_and_expands_a_real_project_tree_with_node_metadata",
        elisp_form,
        expected,
    )
}

fn toggles_hidden_build_artifacts_and_sorts_them_last() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-neotree-test-reset)
  (let* ((root (make-temp-file "neotree-hidden-" t))
         (neo-theme 'ascii)
         (neo-show-updir-line nil)
         (neo-show-hidden-files nil)
         (neo-vc-integration nil)
         (neo-filepath-sort-function 'string<))
    (unwind-protect
        (progn
          (make-directory (expand-file-name ".cache" root))
          (make-directory (expand-file-name "src" root))
          (dolist (name '(".env" "build.o" "draft~" "main.rs" "#notes#"))
            (with-temp-file (expand-file-name name root) (insert name)))
          (neotree-dir root)
          (let ((hidden
                 (mapcar (lambda (entry) (plist-get entry :node))
                         (plist-get (neomacs-neotree-test-snapshot root) :entries))))
            (neotree-hidden-file-toggle)
            (let ((shown
                   (mapcar (lambda (entry) (plist-get entry :node))
                           (plist-get (neomacs-neotree-test-snapshot root) :entries))))
              (setq neo-filepath-sort-function 'neo-sort-hidden-last)
              (neotree-refresh)
              (list
               :initial hidden
               :shown shown
               :hidden-last
               (mapcar (lambda (entry) (plist-get entry :node))
                       (plist-get (neomacs-neotree-test-snapshot root) :entries))
               :show-hidden neo-buffer--show-hidden-file-p))))
      (neomacs-neotree-test-reset)
      (delete-directory root t))))
"####;
    let expected = expect![[
        r##"OK (:initial ("." "src" "main.rs") :shown ("." ".cache" "src" "#notes#" ".env" "build.o" "draft~" "main.rs") :hidden-last ("." "src" ".cache" "main.rs" "#notes#" ".env" "build.o" "draft~") :show-hidden t)"##
    ]];
    ParityBatchCase::value(
        "toggles_hidden_build_artifacts_and_sorts_them_last",
        elisp_form,
        expected,
    )
}

fn navigates_nested_nodes_with_documented_keyboard_commands() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-neotree-test-reset)
  (let* ((root (make-temp-file "neotree-navigation-" t))
         (alpha (expand-file-name "alpha" root))
         (beta (expand-file-name "beta" root))
         (child (expand-file-name "task.txt" alpha))
         (neo-theme 'ascii)
         (neo-show-updir-line nil)
         (neo-vc-integration nil))
    (unwind-protect
        (progn
          (make-directory alpha)
          (make-directory beta)
          (with-temp-file child (insert "ship release\n"))
          (with-temp-file (expand-file-name "summary.txt" root) (insert "ready\n"))
          (neotree-dir root)
          (neotree-find alpha)
          (neotree-enter)
          (neotree-find alpha)
          (let ((start (neomacs-neotree-test-current-node root)))
            (neotree-select-down-node)
            (let ((down (neomacs-neotree-test-current-node root)))
              (neotree-select-up-node)
              (let ((up (neomacs-neotree-test-current-node root)))
                (neotree-select-next-sibling-node)
                (let ((next (neomacs-neotree-test-current-node root)))
                  (neotree-select-previous-sibling-node)
                  (let ((previous (neomacs-neotree-test-current-node root)))
                    (neotree-next-line 2)
                    (list :start start :down down :up up :next next
                          :previous previous
                          :two-lines-down
                          (neomacs-neotree-test-current-node root)
                          :default-directory
                          (neomacs-neotree-test-relative root default-directory))))))))
      (neomacs-neotree-test-reset)
      (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:start "alpha" :down "alpha/task.txt" :up "alpha" :next "beta" :previous "alpha" :two-lines-down "beta" :default-directory "beta")"#
    ]];
    ParityBatchCase::value(
        "navigates_nested_nodes_with_documented_keyboard_commands",
        elisp_form,
        expected,
    )
}

fn opens_files_and_quick_looks_without_losing_the_tree_window() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-neotree-test-reset)
  (let* ((root (make-temp-file "neotree-open-" t))
         (release (expand-file-name "release notes.txt" root))
         (checklist (expand-file-name "checklist.txt" root))
         (neo-theme 'ascii)
         (neo-show-updir-line nil)
         (neo-window-width 23)
         (neo-vc-integration nil))
    (unwind-protect
        (progn
          (with-temp-file release (insert "Release Ω\nReady for review.\n"))
          (with-temp-file checklist (insert "package\nsign\npublish\n"))
          (neotree-dir root)
          (neotree-find release)
          (neotree-enter)
          (let ((opened
                 (list :file (file-name-nondirectory buffer-file-name)
                       :mode major-mode
                       :text (buffer-string)
                       :windows (length (window-list))
                       :tree-visible (neo-global--window-exists-p))))
            (neotree-find checklist)
            (neotree-quick-look)
            (list
             :opened opened
             :quick-look
             (list :selected-buffer (buffer-name)
                   :selected-node
                   (neomacs-neotree-test-current-node root)
                   :visited-text
                   (with-current-buffer (find-buffer-visiting checklist)
                     (buffer-string))
                   :windows (length (window-list))
                   :tree-width (window-width neo-global--window)
                   :tree-side (window-parameter neo-global--window 'window-side)))))
      (neomacs-neotree-test-reset)
      (neomacs-neotree-test-clean-file-buffers root)
      (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:opened (:file "release notes.txt" :mode text-mode :text "Release Ω\nReady for review.\n" :windows 2 :tree-visible t) :quick-look (:selected-buffer " *NeoTree*" :selected-node "checklist.txt" :visited-text "package\nsign\npublish\n" :windows 2 :tree-width 23 :tree-side left))"#
    ]];
    ParityBatchCase::value(
        "opens_files_and_quick_looks_without_losing_the_tree_window",
        elisp_form,
        expected,
    )
}

fn creates_renames_copies_and_deletes_real_project_nodes() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-neotree-test-reset)
  (let* ((root (make-temp-file "neotree-mutations-" t))
         (created (expand-file-name "notes/release draft.txt" root))
         (renamed (expand-file-name "notes/release final.txt" root))
         (copied (expand-file-name "release-copy.txt" root))
         (empty-dir (file-name-as-directory (expand-file-name "artifacts" root)))
         (neo-theme 'ascii)
         (neo-show-updir-line nil)
         (neo-create-file-auto-open nil)
         (neo-confirm-create-file 'off-p)
         (neo-confirm-create-directory 'off-p)
         (neo-confirm-delete-file 'off-p)
         (neo-confirm-delete-directory-recursively 'off-p)
         (neo-confirm-kill-buffers-for-files-in-directory 'off-p)
         (neo-vc-integration nil)
         visiting)
    (unwind-protect
        (progn
          (neotree-dir root)
          (neotree-create-node created)
          (with-temp-file created (insert "release Ω is ready\n"))
          (setq visiting (find-file-noselect created))
          (neotree-find created)
          (cl-letf (((symbol-function 'read-file-name)
                     (lambda (&rest _) renamed)))
            (neotree-rename-node))
          (neotree-find renamed)
          (cl-letf (((symbol-function 'read-file-name)
                     (lambda (&rest _) copied)))
            (neotree-copy-node))
          (neotree-create-node empty-dir)
          (neotree-find copied)
          (let ((deleted (file-name-nondirectory (neotree-delete-node))))
            (list
             :deleted deleted
             :visiting-file
             (with-current-buffer visiting
               (file-relative-name buffer-file-name root))
             :visiting-text (with-current-buffer visiting (buffer-string))
             :filesystem
             (sort
              (mapcar
               (lambda (path)
                 (concat (file-relative-name path root)
                         (if (file-directory-p path) "/" "")))
               (directory-files-recursively root "." t))
              #'string<)
             :rendered
             (mapcar (lambda (entry) (plist-get entry :node))
                     (plist-get (neomacs-neotree-test-snapshot root) :entries))
             :copied-exists (file-exists-p copied))))
      (when (buffer-live-p visiting)
        (with-current-buffer visiting (set-buffer-modified-p nil))
        (kill-buffer visiting))
      (neomacs-neotree-test-reset)
      (neomacs-neotree-test-clean-file-buffers root)
      (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:deleted "release-copy.txt" :visiting-file "notes/release final.txt" :visiting-text "release Ω is ready\n" :filesystem ("artifacts/" "notes/" "notes/release final.txt") :rendered ("." "artifacts" "notes" "notes/release final.txt") :copied-exists nil)"#
    ]];
    ParityBatchCase::value(
        "creates_renames_copies_and_deletes_real_project_nodes",
        elisp_form,
        expected,
    )
}

fn preserves_nested_selection_across_refresh_and_root_changes() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-neotree-test-reset)
  (let* ((root (make-temp-file "neotree-roots-" t))
         (app (expand-file-name "apps/store front" root))
         (src (expand-file-name "src" app))
         (main (expand-file-name "main.el" src))
         (neo-theme 'ascii)
         (neo-show-updir-line nil)
         (neo-vc-integration nil))
    (unwind-protect
        (progn
          (make-directory src t)
          (with-temp-file main (insert "(message \"ship\")\n"))
          (neotree-dir root)
          (neotree-find main)
          (let ((before (neomacs-neotree-test-snapshot root)))
            (with-temp-file (expand-file-name "deploy.el" src)
              (insert "(message \"deploy\")\n"))
            (neotree-refresh)
            (let ((refreshed (neomacs-neotree-test-snapshot root)))
              (neotree-find app)
              (neotree-change-root)
              (let ((changed
                     (list :root (neomacs-neotree-test-relative root neo-buffer--start-node)
                           :selected (neomacs-neotree-test-current-node root)
                           :entries
                           (mapcar (lambda (entry) (plist-get entry :node))
                                   (plist-get (neomacs-neotree-test-snapshot root)
                                              :entries)))))
                (goto-char (point-min))
                (neotree-select-up-node)
                (list :before before
                      :refreshed refreshed
                      :changed changed
                      :up-root
                      (neomacs-neotree-test-relative root neo-buffer--start-node)
                      :up-selected
                      (neomacs-neotree-test-current-node root))))))
      (neomacs-neotree-test-reset)
      (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:before (:mode neotree-mode :selected "apps/store front/src/main.el" :entries ((:node "." :line "<root>" :face nil :kind root) (:node "apps" :line "- apps/" :face neo-dir-link-face :kind directory) (:node "apps/store front" :line "  - store front/" :face neo-dir-link-face :kind directory) (:node "apps/store front/src" :line "    - src/" :face neo-dir-link-face :kind directory) (:node "apps/store front/src/main.el" :line "      main.el" :face neo-file-link-face :kind file)) :expanded ("." "apps" "apps/store front" "apps/store front/src")) :refreshed (:mode neotree-mode :selected "apps/store front/src/main.el" :entries ((:node "." :line "<root>" :face nil :kind root) (:node "apps" :line "- apps/" :face neo-dir-link-face :kind directory) (:node "apps/store front" :line "  - store front/" :face neo-dir-link-face :kind directory) (:node "apps/store front/src" :line "    - src/" :face neo-dir-link-face :kind directory) (:node "apps/store front/src/deploy.el" :line "      deploy.el" :face neo-file-link-face :kind file) (:node "apps/store front/src/main.el" :line "      main.el" :face neo-file-link-face :kind file)) :expanded ("." "apps" "apps/store front" "apps/store front/src")) :changed (:root "apps/store front" :selected nil :entries ("apps/store front" "apps/store front/src" "apps/store front/src/deploy.el" "apps/store front/src/main.el")) :up-root "apps" :up-selected nil)"#
    ]];
    ParityBatchCase::value(
        "preserves_nested_selection_across_refresh_and_root_changes",
        elisp_form,
        expected,
    )
}

fn keeps_right_side_window_position_width_and_editor_focus_across_toggles() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-neotree-test-reset)
  (let* ((root (make-temp-file "neotree-window-" t))
         (editor (get-buffer-create "*neotree editor*"))
         (neo-theme 'ascii)
         (neo-show-updir-line nil)
         (neo-window-position 'right)
         (neo-window-width 19)
         (neo-window-fixed-size t)
         (neo-toggle-window-keep-p nil)
         (neo-vc-integration nil))
    (unwind-protect
        (progn
          (switch-to-buffer editor)
          (erase-buffer)
          (insert "Editor remains available\n")
          (with-temp-file (expand-file-name "status.txt" root) (insert "green\n"))
          (neotree-dir root)
          (let ((shown
                 (list :selected (buffer-name)
                       :windows (length (window-list))
                       :width (window-width neo-global--window)
                       :side (window-parameter neo-global--window 'window-side)
                       :slot (window-parameter neo-global--window 'window-slot)
                       :fixed
                       (buffer-local-value 'window-size-fixed neo-global--buffer))))
            (neotree-toggle)
            (let ((hidden
                   (list :selected (buffer-name)
                         :windows (length (window-list))
                         :tree-visible (neo-global--window-exists-p)
                         :tree-buffer-live (buffer-live-p neo-global--buffer))))
              (switch-to-buffer editor)
              (setq neo-toggle-window-keep-p t)
              (neotree-toggle)
              (list :shown shown
                    :hidden hidden
                    :kept
                    (list :selected (buffer-name)
                          :windows (length (window-list))
                          :width (window-width neo-global--window)
                          :side (window-parameter neo-global--window 'window-side)
                          :editor-text (with-current-buffer editor (buffer-string)))))))
      (neomacs-neotree-test-reset)
      (when (buffer-live-p editor) (kill-buffer editor))
      (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:shown (:selected " *NeoTree*" :windows 2 :width 19 :side right :slot 0 :fixed width) :hidden (:selected "*neotree editor*" :windows 1 :tree-visible nil :tree-buffer-live t) :kept (:selected "*neotree editor*" :windows 2 :width 19 :side right :editor-text "Editor remains available\n"))"#
    ]];
    ParityBatchCase::value(
        "keeps_right_side_window_position_width_and_editor_focus_across_toggles",
        elisp_form,
        expected,
    )
}

fn neotree_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(NEOTREE_MELPA_PIN, "neotree.el")
        .expect("prepare pinned NeoTree below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn neotree_practical_workflows_batch() {
    let cases = vec![
        renders_and_expands_a_real_project_tree_with_node_metadata(),
        toggles_hidden_build_artifacts_and_sorts_them_last(),
        navigates_nested_nodes_with_documented_keyboard_commands(),
        opens_files_and_quick_looks_without_losing_the_tree_window(),
        creates_renames_copies_and_deletes_real_project_nodes(),
        preserves_nested_selection_across_refresh_and_root_changes(),
        keeps_right_side_window_position_width_and_editor_focus_across_toggles(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("neotree practical workflow parity batch");
    assert_oracle_batch_cases(neotree_oracle(), test_name, "neotree parity", &cases);
}
