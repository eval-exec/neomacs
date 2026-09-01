use expect_test::expect;

use super::ParityBatchCase;

/// The package's headline workflow.  A user reads a manual in `Info-mode',
/// runs `ace-link-setup-default' so that "o" is bound, presses "o" to label the
/// two menu items and then the label key of the entry to open.  The second half
/// lands in a node holding a single cross reference, where avy jumps without
/// asking for a key at all.
///
fn ace_link_info_labels_visible_references_and_follows_the_chosen_one() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_link_info_labels_visible_references_and_follows_the_chosen_one",
        r##"(ace-link-test-session
 (ace-link-setup-default)
 (ace-link-test-open-manual)
 (let ((top (list :node Info-current-node
                  :key (lookup-key Info-mode-map "o")
                  :style (assq 'ace-link-info avy-styles-alist)
                  :where (ace-link-test-where))))
   (execute-kbd-macro (kbd "o s"))
   (let ((followed (list :node Info-current-node
                         :where (ace-link-test-where)
                         :text (buffer-substring-no-properties
                                (point-min) (point-max)))))
     (execute-kbd-macro (kbd "o"))
     (list :top top
           :followed followed
           :single-candidate (list :node Info-current-node
                                   :where (ace-link-test-where))
           :keys (ace-link-test-pressed)))))"##,
        expect![[
            r#"OK (:top (:node "Top" :key ace-link-info :style (ace-link-info . at) :where (:buffer "*info*" :window-buffer "*info*" :mode Info-mode :point 58 :line 2 :column 0 :line-text "")) :followed (:node "Advanced" :where (:buffer "*info*" :window-buffer "*info*" :mode Info-mode :point 61 :line 2 :column 0 :line-text "") :text "File: sandbox.info,  Node: Advanced,  Prev: Basics,  Up: Top\n\n2 Advanced\n==========\n\nBack to *note Basics::.\n") :single-candidate (:node "Basics" :where (:buffer "*info*" :window-buffer "*info*" :mode Info-mode :point 73 :line 2 :column 0 :line-text "")) :keys (("s" ((8 2 "a" "Basics::      How to begin.") (9 2 "s" "Advanced::    Deeper water.")))))"#
        ]],
    )
}

fn ace_link_info_aborts_on_c_g_and_reports_an_unknown_label_key() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_link_info_aborts_on_c_g_and_reports_an_unknown_label_key",
        r##"(ace-link-test-session
 (ace-link-setup-default)
 (ace-link-test-open-manual)
 (let ((before (ace-link-test-where)))
   (execute-kbd-macro (kbd "o C-g"))
   (let ((aborted (list :node Info-current-node
                        :where (ace-link-test-where)
                        :labels (ace-link-test-labels))))
     (execute-kbd-macro (kbd "o z C-g"))
     (list :before before
           :aborted aborted
           :after-unknown-key (list :node Info-current-node
                                    :where (ace-link-test-where)
                                    :labels (ace-link-test-labels))
           :keys (ace-link-test-pressed)))))"##,
        expect![[
            r#"OK (:before (:buffer "*info*" :window-buffer "*info*" :mode Info-mode :point 58 :line 2 :column 0 :line-text "") :aborted (:node "Top" :where (:buffer "*info*" :window-buffer "*info*" :mode Info-mode :point 58 :line 2 :column 0 :line-text "") :labels nil) :after-unknown-key (:node "Top" :where (:buffer "*info*" :window-buffer "*info*" :mode Info-mode :point 58 :line 2 :column 0 :line-text "") :labels nil) :keys (("C-g" ((8 2 "a" "Basics::      How to begin.") (9 2 "s" "Advanced::    Deeper water."))) ("z" ((8 2 "a" "Basics::      How to begin.") (9 2 "s" "Advanced::    Deeper water."))) ("C-g" ((8 2 "a" "Basics::      How to begin.") (9 2 "s" "Advanced::    Deeper water.")))))"#
        ]],
    )
}

fn ace_link_help_follows_the_source_button_to_the_defining_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_link_help_follows_the_source_button_to_the_defining_file",
        r##"(ace-link-test-session
 (ace-link-setup-default)
 (load (ace-link-test-write
        "lib/notes-util.el"
        ";;; notes-util.el --- helpers  -*- lexical-binding: t; -*-\n(defun notes-util-render (entry)\n  \"Render ENTRY for the notebook.\nSee `notes-util-format' and `describe-function' for details.\"\n  entry)\n(defun notes-util-format (entry) entry)\n(provide 'notes-util)\n")
       nil t t)
 (describe-function 'notes-util-render)
 (with-current-buffer "*Help*"
   (set-window-buffer (selected-window) (current-buffer))
   (goto-char (point-min))
   (let ((help (list :mode major-mode
                     :key (lookup-key help-mode-map "o")
                     :style (assq 'ace-link-help avy-styles-alist)
                     :text (buffer-substring-no-properties
                            (point-min) (point-max)))))
     (execute-kbd-macro (kbd "o s"))
     (list :help help
           :keys (ace-link-test-pressed)
           :where (ace-link-test-where)
           :help-text (with-current-buffer "*Help*"
                        (buffer-substring-no-properties
                         (point-min) (point-max)))))))"##,
        expect![[r#"OK (:help (:mode help-mode :key ace-link-help :style (ace-link-help . post) :text "notes-util-render is an interpreted-function in\n‘[ORACLE-SANDBOX]/lib/notes-util.el’.\n\n(notes-util-render ENTRY)\n\nRender ENTRY for the notebook.\nSee ‘notes-util-format’ and ‘describe-function’ for details.\n") :keys (("s" ((1 24 "ai" "interpreted-function in") (2 1 "s/" "[ORACLE-SANDBOX]/lib/notes-util.el’.") (7 5 "dn" "notes-util-format’ and ‘describe-function’ for details.") (7 30 "fd" "describe-function’ for details.")))) :where (:buffer "notes-util.el" :window-buffer "notes-util.el" :mode emacs-lisp-mode :point 59 :line 2 :column 0 :line-text "(defun notes-util-render (entry)") :help-text "notes-util-render is an interpreted-function in\n‘[ORACLE-SANDBOX]/lib/notes-util.el’.\n\n(notes-util-render ENTRY)\n\nRender ENTRY for the notebook.\nSee ‘notes-util-format’ and ‘describe-function’ for details.\n")"#]],
    )
    .fresh_process()
}

fn ace_link_org_offers_every_visible_link_type_and_follows_each_kind() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_link_org_offers_every_visible_link_type_and_follows_each_kind",
        r##"(ace-link-test-session
 (ace-link-setup-default)
 (ace-link-test-capture-browsers)
 (ace-link-test-write "notes/appendix.org"
                      "#+title: Appendix\n* Appendix top\nThe appendix body.\n")
 (let ((buffer (find-file-noselect
                (ace-link-test-write
                 "notes/plan.org"
                 (concat "#+title: Plan\n\n* Overview\nRead the [[https://example.org/manual][project manual]] and\n"
                         "the [[file:appendix.org][appendix]].  Jump to [[*Milestones][milestones]].\n"
                         "A bare link: <https://example.org/bare>\n\n"
                         "* Milestones\nSee [[https://example.org/tracker][tracker]].\n\n"
                         "* Archive\nHidden [[https://example.org/hidden][hidden link]].\n")))))
   (set-buffer buffer)
   (set-window-buffer (selected-window) buffer)
   (define-key org-mode-map (kbd "M-o") #'ace-link-org)
   (goto-char (point-max))
   (re-search-backward "^\\* Archive" nil t)
   (execute-kbd-macro (kbd "TAB"))
   (let ((folded (list :mode major-mode
                       :style (assq 'ace-link-org avy-styles-alist)
                       :archive-invisible (get-char-property (- (point-max) 5)
                                                            'invisible))))
     (goto-char (point-min))
     (execute-kbd-macro (kbd "M-o d"))
     (let ((internal (ace-link-test-where)))
       (goto-char (point-min))
       (execute-kbd-macro (kbd "M-o a"))
       (let ((remote (list :where (ace-link-test-where)
                           :browsed (ace-link-test-browsed))))
         (goto-char (point-min))
         (execute-kbd-macro (kbd "M-o s"))
         (list :folded folded
               :internal internal
               :remote remote
               :file (ace-link-test-where)
               :keys (ace-link-test-pressed)
               :browsed (ace-link-test-browsed)))))))"##,
        expect![[
            r##"OK (:folded (:mode org-mode :style (ace-link-org . pre) :archive-invisible org-fold-outline) :internal (:buffer "plan.org" :window-buffer "plan.org" :mode org-mode :point 202 :line 8 :column 0 :line-text "* Milestones") :remote (:where (:buffer "plan.org" :window-buffer "plan.org" :mode org-mode :point 35 :line 4 :column 9 :line-text "Read the [[https://example.org/manual][project manual]] and") :browsed (#1=(browse "https://example.org/manual"))) :file (:buffer "appendix.org" :window-buffer "appendix.org" :mode org-mode :point 0 :line 1 :column 0 :line-text "#+title: Appendix") :keys (("d" ((4 9 "a[" "[[https://example.org/manual][project manual]] and") (5 4 "s[" "[[file:appendix.org][appendix]].  Jump to [[*Milestones][milestones]].") (5 47 "d[" "[[*Milestones][milestones]].") (6 13 "f<" "<https://example.org/bare>") (9 4 "g[" "[[https://example.org/tracker][tracker]].") (12 7 "h[" "[[https://example.org/hidden][hidden link]]."))) ("a" ((4 9 "a[" "[[https://example.org/manual][project manual]] and") (5 4 "s[" "[[file:appendix.org][appendix]].  Jump to [[*Milestones][milestones]].") (5 47 "d[" "[[*Milestones][milestones]].") (6 13 "f<" "<https://example.org/bare>") (9 4 "g[" "[[https://example.org/tracker][tracker]].") (12 7 "h[" "[[https://example.org/hidden][hidden link]]."))) ("s" ((4 9 "a[" "[[https://example.org/manual][project manual]] and") (5 4 "s[" "[[file:appendix.org][appendix]].  Jump to [[*Milestones][milestones]].") (5 47 "d[" "[[*Milestones][milestones]].") (6 13 "f<" "<https://example.org/bare>") (9 4 "g[" "[[https://example.org/tracker][tracker]].") (12 7 "h[" "[[https://example.org/hidden][hidden link]].")))) :browsed (#1#))"##
        ]],
    )
}

fn ace_link_compilation_jumps_from_a_real_compile_run_to_the_error_site() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_link_compilation_jumps_from_a_real_compile_run_to_the_error_site",
        r##"(ace-link-test-session
 (ace-link-setup-default)
 (ace-link-test-write "src/parser.c" "int main(void) {\n  return 0;\n}\n")
 (ace-link-test-write "src/lexer.c" "/* lexer */\nint lex(void) { return 1; }\n")
 (let* ((default-directory (file-name-as-directory (ace-link-test-path "")))
        (buffer (compilation-start
                 "printf 'src/parser.c:2:3: warning: unused value\\nsrc/lexer.c:2:17: error: bad token\\n'"
                 nil
                 (lambda (&rest _) "*compile-fixture*"))))
   (ace-link-test-await-compilation buffer)
   (with-current-buffer buffer
     (set-window-buffer (selected-window) (current-buffer))
     (goto-char (point-min))
     (let ((compilation (list :mode major-mode
                              :running (and (get-buffer-process (current-buffer)) t)
                              :key (lookup-key compilation-mode-map "o")
                              :style (assq 'ace-link-compilation avy-styles-alist)
                              :dispatch (assq 'ace-link-compilation
                                              ace-link-major-mode-actions))))
       (execute-kbd-macro (kbd "o s"))
       (list :compilation compilation
             :keys (ace-link-test-pressed)
             :where (ace-link-test-where)
             :file-text (buffer-substring-no-properties
                         (point-min) (point-max)))))))"##,
        expect![[r#"OK (:compilation (:mode compilation-mode :running nil :key ace-link-compilation :style (ace-link-compilation . post) :dispatch (ace-link-compilation compilation-mode grep-mode)) :keys (("s" ((5 0 "as" "src/parser.c:2:3: warning: unused value") (6 0 "ss" "src/lexer.c:2:17: error: bad token")))) :where (:buffer "lexer.c" :window-buffer "lexer.c" :mode c-mode :point 28 :line 2 :column 16 :line-text "int lex(void) { return 1; }") :file-text "/* lexer */\nint lex(void) { return 1; }\n")"#]],
    )
    .fresh_process()
}

fn ace_link_eww_follows_a_local_page_and_a_prefix_opens_the_external_browser() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_link_eww_follows_a_local_page_and_a_prefix_opens_the_external_browser",
        r##"(ace-link-test-session
 (require 'eww)
 (ace-link-test-capture-browsers)
 (ace-link-setup-default)
 (ace-link-test-write
  "site/index.html"
  "<html><head><title>Field notes</title></head><body>\n<h1>Field notes</h1>\n<p>Start with the <a href=\"intro.html\">introduction</a>, then read the\n<a href=\"api.html\">API reference</a>.</p>\n<p>External: <a href=\"https://example.org/upstream\">upstream</a>.</p>\n</body></html>\n")
 (ace-link-test-write
  "site/intro.html"
  "<html><head><title>Introduction</title></head><body>\n<h1>Introduction</h1>\n<p>Back to the <a href=\"index.html\">index</a>.</p>\n</body></html>\n")
 (ace-link-test-write
  "site/api.html"
  "<html><head><title>API</title></head><body><h1>API</h1><p>Nothing here yet.</p></body></html>\n")
 (eww-open-file (ace-link-test-path "site/index.html"))
 (with-current-buffer "*eww*"
   (set-window-buffer (selected-window) (current-buffer))
   (let ((index (list :mode major-mode
                      :title (plist-get eww-data :title)
                      :key (lookup-key eww-mode-map "o")
                      :style (assq 'ace-link-eww avy-styles-alist)
                      :text (buffer-substring-no-properties
                             (point-min) (point-max)))))
     (execute-kbd-macro (kbd "o a"))
     (let ((followed (list :title (plist-get eww-data :title)
                           :url (plist-get eww-data :url)
                           :where (ace-link-test-where))))
       (execute-kbd-macro (kbd "l"))
       (execute-kbd-macro (kbd "C-u o d"))
       (list :index index
             :followed followed
             :external (list :title (plist-get eww-data :title)
                             :where (ace-link-test-where))
             :browsed (ace-link-test-browsed)
             :keys (ace-link-test-pressed))))))"##,
        expect![[r#"OK (:index (:mode eww-mode :title "Field notes" :key ace-link-eww :style (ace-link-eww . post) :text "Field\nnotes\n\n\nStart\nwith\nthe\nintroduction,\nthen\nread\nthe\nAPI\nreference.\n\n\nExternal:\nupstream.\n") :followed (:title "Introduction" :url "file://[ORACLE-SANDBOX]/site/intro.html" :where (:buffer "*eww*" :window-buffer "*eww*" :mode eww-mode :point 0 :line 1 :column 0 :line-text "Introduction")) :external (:title "Field notes" :where (:buffer "*eww*" :window-buffer "*eww*" :mode eww-mode :point 84 :line 17 :column 0 :line-text "upstream.")) :browsed ((browse-external "https://example.org/upstream")) :keys (("a" ((8 0 "ai" "introduction,") (12 0 "sA" "API") (17 0 "du" "upstream."))) ("d" ((8 0 "ai" "introduction,") (12 0 "sA" "API") (17 0 "du" "upstream.")))))"#]],
    )
    .fresh_process()
}

fn ace_link_dispatches_on_major_mode_and_falls_back_when_unsupported() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_link_dispatches_on_major_mode_and_falls_back_when_unsupported",
        r##"(ace-link-test-session
 (ace-link-test-write "src/parser.c" "int main(void) {\n  return 0;\n}\n")
 (global-set-key (kbd "M-o") #'ace-link)
 (global-set-key (kbd "C-c o") #'ace-link-compilation)
 (let ((buffer (generate-new-buffer "*shell-output*")))
   (set-window-buffer (selected-window) buffer)
   (set-buffer buffer)
   (setq default-directory (file-name-as-directory (ace-link-test-path "")))
   (let ((plain (list :mode major-mode
                      :unsupported (condition-case failure
                                       (execute-kbd-macro (kbd "M-o"))
                                     (error failure))
                      :fallback (let ((ace-link-fallback-function
                                       (lambda ()
                                         (list 'fallback major-mode (point)))))
                                  (ace-link)))))
     (insert "$ make\nsrc/parser.c:2:3: warning: unused value\nsrc/parser.c:3:1: error: stray brace\n")
     (compilation-shell-minor-mode 1)
     (font-lock-ensure)
     (goto-char (point-min))
     (let ((minor (list :minor-mode compilation-shell-minor-mode
                        :table ace-link-minor-mode-actions
                        :dispatch (condition-case failure
                                      (execute-kbd-macro (kbd "M-o s"))
                                    (error failure)))))
       (goto-char (point-min))
       (execute-kbd-macro (kbd "C-c o s"))
       (list :plain plain
             :minor minor
             :keys (ace-link-test-pressed)
             :where (ace-link-test-where))))))"##,
        expect![[r#"OK (:plain (:mode fundamental-mode :unsupported (error "fundamental-mode isn’t supported") :fallback (fallback fundamental-mode 1)) :minor (:minor-mode t :table ((ace-link-compilation compilation-shell-minor-mode)) :dispatch (error "fundamental-mode isn’t supported")) :keys (("s" ((2 0 "a" "src/parser.c:2:3: warning: unused value") (3 0 "s" "src/parser.c:3:1: error: stray brace")))) :where (:buffer "parser.c" :window-buffer "parser.c" :mode c-mode :point 29 :line 3 :column 0 :line-text "}"))"#]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ace_link_info_labels_visible_references_and_follows_the_chosen_one(),
        ace_link_info_aborts_on_c_g_and_reports_an_unknown_label_key(),
        ace_link_help_follows_the_source_button_to_the_defining_file(),
        ace_link_org_offers_every_visible_link_type_and_follows_each_kind(),
        ace_link_compilation_jumps_from_a_real_compile_run_to_the_error_site(),
        ace_link_eww_follows_a_local_page_and_a_prefix_opens_the_external_browser(),
        ace_link_dispatches_on_major_mode_and_falls_back_when_unsupported(),
    ]
}
