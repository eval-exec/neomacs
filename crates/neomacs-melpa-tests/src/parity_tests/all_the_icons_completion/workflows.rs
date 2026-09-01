use expect_test::expect;

use super::ParityBatchCase;

/// Installation is the whole mode: switching it on advises
/// `completion-metadata-get', so a table that offers no affixation function of
/// its own suddenly has one, and switching it off takes the advice away and the
/// table is back to offering nothing.  The mode is global, and it does not
/// disturb the other metadata properties a completion UI reads.
fn enabling_the_mode_installs_the_affixation_advice_and_disabling_removes_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_the_mode_installs_the_affixation_advice_and_disabling_removes_it",
        r##"(unwind-protect
    (let ((table (aic-test-table '("src/" "notes.org") '((category . file)))))
      (let ((before (list :advised (aic-test-advised)
                          :mode (bound-and-true-p all-the-icons-completion-mode)
                          :affixations (aic-test-affixations table '("src/"))
                          :metadata (aic-test-metadata-passthrough table))))
        (all-the-icons-completion-mode 1)
        (let ((enabled (list :advised (aic-test-advised)
                             :mode all-the-icons-completion-mode
                             :global (get 'all-the-icons-completion-mode 'globalized-minor-mode)
                             :affixations (aic-test-affixations table '("src/"))
                             :metadata (aic-test-metadata-passthrough table))))
          (all-the-icons-completion-mode 0)
          (list :before before
                :enabled enabled
                :disabled (list :advised (aic-test-advised)
                                :mode all-the-icons-completion-mode
                                :affixations (aic-test-affixations table '("src/"))
                                :metadata (aic-test-metadata-passthrough table))))))
  (aic-test-cleanup))"##,
        expect![[
            r#"OK (:before (:advised nil :mode nil :affixations no-affixation-function :metadata (:category file :cycle-sort nil :annotation nil)) :enabled (:advised t :mode t :global nil :affixations (("src/" (61462 32) ("github-octicons" all-the-icons-completion-dir-face) "")) :metadata (:category file :cycle-sort nil :annotation nil)) :disabled (:advised nil :mode nil :affixations no-affixation-function :metadata (:category file :cycle-sort nil :annotation nil)))"#
        ]],
    )
}

fn file_candidates_get_a_directory_a_file_or_the_fallback_icon() -> ParityBatchCase {
    ParityBatchCase::value(
        "file_candidates_get_a_directory_a_file_or_the_fallback_icon",
        r##"(unwind-protect
    (progn
      (all-the-icons-completion-mode 1)
      (let ((candidates '("src/" "notes.org" "script.py" "Makefile" "unknown.zzz")))
        (list :category 'file
              :affixations (aic-test-affixations
                            (aic-test-table candidates '((category . file)))
                            candidates)
              :directory-face-defined (and (facep 'all-the-icons-completion-dir-face) t))))
  (aic-test-cleanup))"##,
        expect![[
            r#"OK (:category file :affixations (("src/" (61462 32) ("github-octicons" all-the-icons-completion-dir-face) "") ("notes.org" (59671 32) ("file-icons" all-the-icons-lgreen) "") ("script.py" (59688 32) ("all-the-icons" all-the-icons-dblue) "") ("Makefile" (59001 32) ("file-icons" all-the-icons-dorange) "") ("unknown.zzz" (61462 32) ("FontAwesome" all-the-icons-dsilver) "")) :directory-face-defined t)"#
        ]],
    )
}

fn project_file_and_buffer_categories_use_their_own_icon_lookups() -> ParityBatchCase {
    ParityBatchCase::value(
        "project_file_and_buffer_categories_use_their_own_icon_lookups",
        r##"(unwind-protect
    (progn
      (all-the-icons-completion-mode 1)
      (let ((files '("notes.org" "script.py" "src/"))
            (elisp (generate-new-buffer "aic-probe-code.el"))
            (plain (generate-new-buffer "aic-probe-notes")))
        (unwind-protect
            (progn
              (with-current-buffer elisp (emacs-lisp-mode))
              (let ((buffers (list (buffer-name elisp) (buffer-name plain))))
                (list :file (aic-test-affixations
                             (aic-test-table files '((category . file))) files)
                      :project-file (aic-test-affixations
                                     (aic-test-table files '((category . project-file)))
                                     files)
                      :buffer-modes (list (buffer-local-value 'major-mode elisp)
                                          (buffer-local-value 'major-mode plain))
                      :buffer (aic-test-affixations
                               (aic-test-table buffers '((category . buffer)))
                               buffers))))
          (kill-buffer elisp)
          (kill-buffer plain))))
  (aic-test-cleanup))"##,
        expect![[
            r#"OK (:file (("notes.org" (59671 32) ("file-icons" all-the-icons-lgreen) "") ("script.py" (59688 32) ("all-the-icons" all-the-icons-dblue) "") ("src/" (61462 32) ("github-octicons" all-the-icons-completion-dir-face) "")) :project-file (("notes.org" (59671 32) ("file-icons" all-the-icons-lgreen) "") ("script.py" (59688 32) ("all-the-icons" all-the-icons-dblue) "") ("src/" (61462 32) ("github-octicons" all-the-icons-completion-dir-face) "")) :buffer-modes (emacs-lisp-mode fundamental-mode) :buffer (("aic-probe-code.el" (59686 32) ("file-icons" all-the-icons-purple) "") ("aic-probe-notes" (59686 32) ("file-icons" all-the-icons-dsilver) "")))"#
        ]],
    )
}

fn an_existing_affixation_or_annotation_function_is_kept_and_prefixed() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_existing_affixation_or_annotation_function_is_kept_and_prefixed",
        r##"(unwind-protect
    (progn
      (all-the-icons-completion-mode 1)
      (let ((candidates '("notes.org" "script.py")))
        (list
         :own-affixation
         (aic-test-affixations
          (aic-test-table candidates
                          (list '(category . file)
                                (cons 'affixation-function
                                      (lambda (cands)
                                        (mapcar (lambda (cand)
                                                  (list cand "> " " [file]"))
                                                cands)))))
          candidates)
         :own-annotation
         (aic-test-affixations
          (aic-test-table candidates
                          (list '(category . file)
                                (cons 'annotation-function
                                      (lambda (cand)
                                        (concat "  (" (upcase cand) ")")))))
          candidates)
         :annotation-without-category
         (aic-test-affixations
          (aic-test-table candidates
                          (list (cons 'annotation-function
                                      (lambda (cand) (concat "  " cand)))))
          candidates))))
  (aic-test-cleanup))"##,
        expect![[
            r#"OK (:own-affixation (("notes.org" (59671 32 62 32) ("file-icons" all-the-icons-lgreen) " [file]") ("script.py" (59688 32 62 32) ("all-the-icons" all-the-icons-dblue) " [file]")) :own-annotation (("notes.org" (59671 32) ("file-icons" all-the-icons-lgreen) "  (NOTES.ORG)") ("script.py" (59688 32) ("all-the-icons" all-the-icons-dblue) "  (SCRIPT.PY)")) :annotation-without-category (("notes.org" nil nil "  notes.org") ("script.py" nil nil "  script.py")))"#
        ]],
    )
}

fn multi_category_and_bookmark_candidates_resolve_their_own_category() -> ParityBatchCase {
    ParityBatchCase::value(
        "multi_category_and_bookmark_candidates_resolve_their_own_category",
        r##"(unwind-protect
    (progn
      (all-the-icons-completion-mode 1)
      (require 'bookmark)
      (setq bookmark-alist nil)
      (bookmark-store "project-code" '((filename . "/home/user/project/main.py")) nil)
      (bookmark-store "a-place" '((handler . ignore)) nil)
      (let ((scratch (generate-new-buffer "aic-probe-scratch")))
        (unwind-protect
            (let ((multi (list (propertize "notes.org"
                                           'multi-category '(file . "notes.org"))
                               (propertize (buffer-name scratch)
                                           'multi-category
                                           (cons 'buffer (buffer-name scratch)))))
                  (bookmarks '("project-code" "a-place")))
              (list :multi-category (aic-test-affixations
                                     (aic-test-table multi '((category . multi-category)))
                                     multi)
                    :bookmark-names (bookmark-all-names)
                    :bookmark (aic-test-affixations
                               (aic-test-table bookmarks '((category . bookmark)))
                               bookmarks)))
          (kill-buffer scratch))))
  (aic-test-cleanup))"##,
        expect![[
            r#"OK (:multi-category (("notes.org" (59671 32) ("file-icons" all-the-icons-lgreen) "") ("aic-probe-scratch" (59686 32) ("file-icons" all-the-icons-dsilver) "")) :bookmark-names ("a-place" "project-code") :bookmark (("project-code" (59688 32) ("all-the-icons" all-the-icons-dblue) "") ("a-place" (61563 32) ("github-octicons" all-the-icons-completion-dir-face) "")))"#
        ]],
    )
}

fn unknown_categories_are_empty_and_completion_itself_is_never_changed() -> ParityBatchCase {
    ParityBatchCase::value(
        "unknown_categories_are_empty_and_completion_itself_is_never_changed",
        r##"(unwind-protect
    (let* ((candidates '("src/" "script.py" "scratch.txt"))
           (typed-table (aic-test-table candidates '((category . file))))
           (unknown-table (aic-test-table candidates '((category . nonesuch))))
           (bare-table (aic-test-table candidates)))
      (let ((off (list :try (try-completion "sc" typed-table)
                       :all (all-completions "s" typed-table)
                       :test (test-completion "script.py" typed-table)
                       :unknown (aic-test-affixations unknown-table candidates)
                       :bare (aic-test-affixations bare-table candidates))))
        (all-the-icons-completion-mode 1)
        (let ((on (list :try (try-completion "sc" typed-table)
                        :all (all-completions "s" typed-table)
                        :test (test-completion "script.py" typed-table)
                        :unknown (aic-test-affixations unknown-table candidates)
                        :bare (aic-test-affixations bare-table candidates))))
          (list :off off
                :on on
                :completion-unchanged
                (list (equal (plist-get off :try) (plist-get on :try))
                      (equal (plist-get off :all) (plist-get on :all))
                      (equal (plist-get off :test) (plist-get on :test)))))))
  (aic-test-cleanup))"##,
        expect![[
            r#"OK (:off (:try "scr" :all ("src/" "script.py" "scratch.txt") :test t :unknown no-affixation-function :bare no-affixation-function) :on (:try "scr" :all ("src/" "script.py" "scratch.txt") :test t :unknown (("src/" nil nil "") ("script.py" nil nil "") ("scratch.txt" nil nil "")) :bare no-affixation-function) :completion-unchanged (t t t))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        enabling_the_mode_installs_the_affixation_advice_and_disabling_removes_it(),
        file_candidates_get_a_directory_a_file_or_the_fallback_icon(),
        project_file_and_buffer_categories_use_their_own_icon_lookups(),
        an_existing_affixation_or_annotation_function_is_kept_and_prefixed(),
        multi_category_and_bookmark_candidates_resolve_their_own_category(),
        unknown_categories_are_empty_and_completion_itself_is_never_changed(),
    ]
}
