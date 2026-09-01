//! Practical parity for lsp-ivy's workspace-symbol Ivy UI.
//!
//! These cases report the public no-workspace error, search planted LSP
//! symbols through Ivy, filter by extra query words, jump to a definition,
//! and remove a session folder.

use std::time::Duration;

use expect_test::expect;

use crate::{
    CachedMelpaOracle, DASH_MELPA_PIN, IVY_MELPA_PIN, LSP_IVY_MELPA_PIN, LSP_MODE_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'ivy)
(require 'lsp-mode)
(require 'lsp-ivy)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst li458-test-tree
  "37a4a751aca9387c96b2da27559b47809a65e69a")
(defconst li458-test-manifest
  '(("lsp-ivy-pkg.el" . "4146604e5fc6a96e9bbf300da65948189a089fa829fb6dc06df0980d1bef1f53")
    ("lsp-ivy.el" . "867e392deb56cc832649b3fdeeb8b9c47185cb22c00260648218aeabf082c4bc")))

(defvar li458-test-log nil)
(defvar li458-test-requests nil)

(defun li458-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun li458-test-source-state ()
  (let* ((located (locate-library "lsp-ivy.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (li458-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/lsp-ivy.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car li458-test-manifest)))
      (error "Unexpected installed lsp-ivy payload: %S"
             (or manifest files)))
    (dolist (entry li458-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (li458-test-sha file) expected))
          (error "Unexpected installed lsp-ivy source: %S"
                 (cons entry manifest)))))
    (list :tree li458-test-tree
          :manifest manifest
          :feature (featurep 'lsp-ivy)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'lsp-ivy package-alist)))))))

(defun li458-test-write (path text)
  (make-directory (file-name-directory path) t)
  (write-region text nil path nil 'silent)
  path)

(defun li458-test-symbol (name kind container path line character)
  (lsp-make-symbol-information
   :name name
   :kind kind
   :container-name? container
   :location
   (lsp-make-location
    :uri (lsp--path-to-uri path)
    :range (lsp-make-range
            :start (lsp-make-position :line line :character character)
            :end (lsp-make-position :line line :character character)))))

(defun li458-test-plain (cands)
  (mapcar #'substring-no-properties cands))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LSP_IVY_MELPA_PIN, "lsp-ivy.el")
        .expect("prepare pinned lsp-ivy source below ./tmp")
        .with_melpa_dependency(LSP_MODE_MELPA_PIN)
        .expect("prepare pinned lsp-mode dependency below ./tmp")
        .with_melpa_dependency(IVY_MELPA_PIN)
        .expect("prepare pinned ivy dependency below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn no_workspace_reports_the_public_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "no_workspace_reports_the_public_error",
        r####"
(let ((lsp--cur-workspace nil)
      (lsp--buffer-workspaces nil))
  (condition-case err
      (lsp-ivy-workspace-symbol nil)
    (error (list :source (li458-test-source-state)
                 :signal (car err)
                 :message (error-message-string err)))))
"####,
        expect![[
            r#"OK (:source (:tree "37a4a751aca9387c96b2da27559b47809a65e69a" :manifest (("lsp-ivy-pkg.el" . "4146604e5fc6a96e9bbf300da65948189a089fa829fb6dc06df0980d1bef1f53") ("lsp-ivy.el" . "867e392deb56cc832649b3fdeeb8b9c47185cb22c00260648218aeabf082c4bc")) :feature t :version "20260507.1752") :signal user-error :message "No LSP workspace active")"#
        ]],
    )
}

fn workspace_symbol_search_jumps_to_the_selected_definition() -> ParityBatchCase {
    ParityBatchCase::value(
        "workspace_symbol_search_jumps_to_the_selected_definition",
        r####"
(save-window-excursion
  (let* ((root (file-name-as-directory
                (expand-file-name "lsp-ivy-ws"
                                  (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (service (expand-file-name "src/service.el" root))
         (workspace (make-lsp--workspace :root root :client (make-lsp--client)))
         (lsp--cur-workspace workspace)
         (lsp--buffer-workspaces (list workspace))
         (symbols
          (list
           (li458-test-symbol "PromoteRelease" 12 "ReleaseService" service 2 9)
           (li458-test-symbol "CreateRelease" 12 "ReleaseController"
                              (expand-file-name "src/controller.el" root) 1 9)))
         (li458-test-requests nil)
         ivy-prompt ivy-initial chosen)
    (unwind-protect
        (progn
          (when (file-exists-p root) (delete-directory root t))
          (li458-test-write service
                            ";; service\n\n(defun PromoteRelease (release)\n  release)\n")
          (li458-test-write (expand-file-name "src/controller.el" root)
                            ";; controller\n(defun CreateRelease (x) x)\n")
          (cl-letf (((symbol-function 'lsp-workspace-root)
                     (lambda (&rest _) root))
                    ((symbol-function 'lsp-request-while-no-input)
                     (lambda (method params)
                       (push (list :method method :params params)
                             li458-test-requests)
                       symbols))
                    ((symbol-function 'ivy-read)
                     (lambda (prompt collection &rest args)
                       (setq ivy-prompt prompt
                             ivy-initial (plist-get args :initial-input))
                       (let ((cands (funcall collection "Promote"))
                             (action (plist-get args :action)))
                         (setq chosen (li458-test-plain cands))
                         (funcall action (car cands))
                         (car cands)))))
            (with-current-buffer (find-file-noselect service)
              (lsp-ivy-workspace-symbol nil)
              (list :source (li458-test-source-state)
                    :prompt ivy-prompt
                    :initial ivy-initial
                    :candidates chosen
                    :requests
                    (mapcar
                     (lambda (req)
                       (let ((params (plist-get req :params)))
                         (list :method (plist-get req :method)
                               :query (or (plist-get params :query)
                                          (and (hash-table-p params)
                                               (or (gethash "query" params)
                                                   (gethash :query params)))))))
                     (nreverse li458-test-requests))
                    :file (file-relative-name (buffer-file-name) root)
                    :line (line-number-at-pos)
                    :text (buffer-substring-no-properties
                           (line-beginning-position)
                           (line-end-position))))))
      (when (file-exists-p root)
        (delete-directory root t)))))
"####,
        expect![[
            r#"OK (:source (:tree "37a4a751aca9387c96b2da27559b47809a65e69a" :manifest (("lsp-ivy-pkg.el" . "4146604e5fc6a96e9bbf300da65948189a089fa829fb6dc06df0980d1bef1f53") ("lsp-ivy.el" . "867e392deb56cc832649b3fdeeb8b9c47185cb22c00260648218aeabf082c4bc")) :feature t :version "20260507.1752") :prompt "Workspace symbol: " :initial nil :candidates ("[Func] PromoteRelease.ReleaseService · src/service.el" "[Func] CreateRelease.ReleaseController · src/controller.el") :requests ((:method "workspace/symbol" :query "Promote")) :file "src/service.el" :line 3 :text "(defun PromoteRelease (release)")"#
        ]],
    )
}

fn extra_query_words_filter_candidates_and_kinds_can_be_hidden() -> ParityBatchCase {
    ParityBatchCase::value(
        "extra_query_words_filter_candidates_and_kinds_can_be_hidden",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "lsp-ivy-filter"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (service (expand-file-name "src/service.el" root))
       (workspace (make-lsp--workspace :root root :client (make-lsp--client)))
       (lsp--cur-workspace workspace)
       (lsp--buffer-workspaces (list workspace))
       (symbols
        (list
         (li458-test-symbol "PromoteRelease" 12 "ReleaseService" service 2 9)
         (li458-test-symbol "PromoteFlag" 13 "ReleaseService" service 8 4)
         (li458-test-symbol "caféHelper" 12 "Utils"
                            (expand-file-name "src/utils.el" root) 0 6)))
       filtered hidden)
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory (file-name-directory service) t)
        (write-region "x" nil service nil 'silent)
        (cl-letf (((symbol-function 'lsp-workspace-root)
                   (lambda (&rest _) root))
                  ((symbol-function 'lsp-request-while-no-input)
                   (lambda (&rest _) symbols))
                  ((symbol-function 'ivy-read)
                   (lambda (_prompt collection &rest _)
                     (setq filtered
                           (li458-test-plain (funcall collection "Promote Service")))
                     (let ((lsp-ivy-filter-symbol-kind '(13)))
                       (setq hidden
                             (li458-test-plain (funcall collection "Promote"))))
                     nil)))
          (lsp-ivy--workspace-symbol (list workspace) "Workspace symbol: " nil)
          (list :source (li458-test-source-state)
                :filtered filtered
                :hidden hidden)))
    (when (file-exists-p root)
      (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "37a4a751aca9387c96b2da27559b47809a65e69a" :manifest (("lsp-ivy-pkg.el" . "4146604e5fc6a96e9bbf300da65948189a089fa829fb6dc06df0980d1bef1f53") ("lsp-ivy.el" . "867e392deb56cc832649b3fdeeb8b9c47185cb22c00260648218aeabf082c4bc")) :feature t :version "20260507.1752") :filtered ("[Func] PromoteRelease.ReleaseService · src/service.el" "[Var ] PromoteFlag.ReleaseService · src/service.el") :hidden ("[Func] PromoteRelease.ReleaseService · src/service.el" "[Func] caféHelper.Utils · src/utils.el"))"#
        ]],
    )
}

fn folders_remove_asks_ivy_and_forwards_the_chosen_folder() -> ParityBatchCase {
    ParityBatchCase::value(
        "folders_remove_asks_ivy_and_forwards_the_chosen_folder",
        r####"
(let ((removed nil)
      (folders '("/proj/alpha" "/proj/café"))
      prompt cands)
  (cl-letf (((symbol-function 'lsp-session)
             (lambda () (make-lsp-session :folders folders)))
            ((symbol-function 'lsp-find-session-folder)
             (lambda (_session _dir) "/proj/alpha"))
            ((symbol-function 'lsp-workspace-folders-remove)
             (lambda (folder) (push folder removed)))
            ((symbol-function 'ivy--kill-current-candidate)
             (lambda () t))
            ((symbol-function 'ivy-read)
             (lambda (p collection &rest args)
               (setq prompt p
                     cands collection)
               (funcall (plist-get args :action) "/proj/café")
               "/proj/café")))
    (lsp-ivy-workspace-folders-remove)
    (list :source (li458-test-source-state)
          :prompt prompt
          :cands cands
          :removed (nreverse removed))))
"####,
        expect![[
            r#"OK (:source (:tree "37a4a751aca9387c96b2da27559b47809a65e69a" :manifest (("lsp-ivy-pkg.el" . "4146604e5fc6a96e9bbf300da65948189a089fa829fb6dc06df0980d1bef1f53") ("lsp-ivy.el" . "867e392deb56cc832649b3fdeeb8b9c47185cb22c00260648218aeabf082c4bc")) :feature t :version "20260507.1752") :prompt "Select workspace folder to remove: " :cands ("/proj/alpha" "/proj/café") :removed ("/proj/café"))"#
        ]],
    )
}

#[test]
fn lsp_ivy_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        no_workspace_reports_the_public_error(),
        workspace_symbol_search_jumps_to_the_selected_definition(),
        extra_query_words_filter_candidates_and_kinds_can_be_hidden(),
        folders_remove_asks_ivy_and_forwards_the_chosen_folder(),
    ];
    assert_oracle_batch_cases(oracle(), "lsp-ivy-rank458", "lsp_ivy_parity", &cases);
}
