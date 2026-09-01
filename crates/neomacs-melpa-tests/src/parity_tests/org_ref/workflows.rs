use expect_test::expect;

use super::ParityBatchCase;

fn activation_and_real_document_fontification() -> ParityBatchCase {
    ParityBatchCase::value(
        "activation-and-real-document-fontification",
        r####"(org-ref362-test-run
 "activation-document"
 (lambda (world)
   (org-ref362-test-write-main
    world
    (concat
     "#+title: Activation 界\n#+include: \"chapter λ.org\"\n\n"
     "* Local\n:PROPERTIES:\n:CUSTOM_ID: local-target\n:END:\n\n"
     "Valid [[cite:&ada2024deterministic]] missing [[cite:&missing-key]].\n"
     "Refs ref:local-target and ref:missing-target.\n"
     "Duplicate label:dup and label:dup.\n"
     "bibliography:references.bib\n"))
   (let* ((main (org-ref362-test-visit (plist-get world :main) #'org-mode))
          (chapter (org-ref362-test-visit (plist-get world :chapter) #'org-mode))
          (bib (org-ref362-test-visit (plist-get world :bib) #'bibtex-mode)))
     (switch-to-buffer main)
     (font-lock-ensure)
     (list
        :provenance
        (let ((source (symbol-file 'org-ref-insert-link 'defun)))
          (list :file (file-name-nondirectory source)
                :source-only (string-suffix-p ".el" source)
                :features (mapcar #'featurep
                                  '(org-ref org-ref-core org-ref-export citeproc))))
        :public
        (list :insert org-ref-insert-link-function
              :cite org-ref-insert-cite-function
              :label org-ref-insert-label-function
              :ref org-ref-insert-ref-function
              :key (key-binding (kbd "C-c ]"))
              :menu-hook-count (cl-count #'org-ref-org-menu org-mode-hook)
              :routes
              (mapcar (lambda (type)
                        (list type
                              (functionp (org-link-get-parameter type :follow))
                              (functionp (org-link-get-parameter type :export))))
                      '("cite" "ref" "label" "bibliography")))
        :paths (mapcar (lambda (file)
                         (file-relative-name file (plist-get world :root)))
                       (list (buffer-file-name main)
                             (buffer-file-name chapter)
                             (buffer-file-name bib)))
        :links (mapcar #'org-ref362-test-link-state
                       '("[[cite:&ada2024deterministic]]"
                         "[[cite:&missing-key]]" "ref:local-target"
                         "ref:missing-target" "label:dup"
                         "bibliography:references.bib"))
        :bib-font
        (with-current-buffer bib
          (font-lock-ensure)
          (mapcar
           (lambda (needle)
             (save-excursion
               (goto-char (point-min))
               (search-forward needle)
               (let ((position (match-beginning 0)))
                 (list needle (line-number-at-pos position)
                       (- position (line-beginning-position))
                       (get-text-property position 'face)
                       (get-text-property position 'font-lock-face)))))
           '("@article" "ada2024deterministic" "Lovelace" "Café")))
        :text (buffer-substring-no-properties (point-min) (point-max))
        :modified (buffer-modified-p)))))"####,
        expect![[
            r##"OK (:result (:provenance (:file "org-ref-core.el" :source-only t :features (t t t t)) :public (:insert org-ref-insert-link :cite org-ref-insert-cite-link :label org-ref-insert-label-link :ref org-ref-insert-ref-link :key org-ref-insert-link :menu-hook-count 1 :routes (("cite" t t) ("ref" t t) ("label" nil t) ("bibliography" t t))) :paths ("paper 界.org" "chapter λ.org" "references.bib") :links ((:needle "[[cite:&ada2024deterministic]]" :begin 110 :end 141 :type "cite" :path "&ada2024deterministic" :face-runs ((0 1 org-ref-cite-face nil nil org-ref-cite-tooltip) (1 2 org-ref-cite-face nil nil org-ref-cite-tooltip) (2 7 org-ref-cite-face nil nil org-ref-cite-tooltip) (7 8 org-ref-cite-&-face nil "ada2024deterministic" org-ref-cite-tooltip) (8 27 org-ref-cite-face nil "ada2024deterministic" org-ref-cite-tooltip) (27 28 org-ref-cite-face nil "ada2024deterministic" org-ref-cite-tooltip) (28 29 org-ref-cite-face nil nil org-ref-cite-tooltip) (29 30 org-ref-cite-face nil nil org-ref-cite-tooltip) (30 31 nil nil nil nil))) (:needle "[[cite:&missing-key]]" :begin 149 :end 170 :type "cite" :path "&missing-key" :face-runs ((0 1 org-ref-cite-face nil nil org-ref-cite-tooltip) (1 2 org-ref-cite-face nil nil org-ref-cite-tooltip) (2 7 org-ref-cite-face nil nil org-ref-cite-tooltip) (7 18 font-lock-warning-face nil "missing-key" "Key not found") (18 19 font-lock-warning-face nil "missing-key" "Key not found") (19 20 org-ref-cite-face nil nil org-ref-cite-tooltip) (20 21 org-ref-cite-face nil nil org-ref-cite-tooltip))) (:needle "ref:local-target" :begin 177 :end 194 :type "ref" :path "local-target" :face-runs ((0 4 org-ref-ref-face nil nil org-ref-ref-help-echo) (4 15 org-ref-ref-face nil nil org-ref-ref-help-echo) (15 16 org-ref-ref-face nil nil org-ref-ref-help-echo) (16 17 nil nil nil nil))) (:needle "ref:missing-target" :begin 198 :end 216 :type "ref" :path "missing-target" :face-runs ((0 4 org-ref-ref-face nil nil org-ref-ref-help-echo) (4 17 font-lock-warning-face nil nil "Label not found") (17 18 font-lock-warning-face nil nil "Label not found"))) (:needle "label:dup" :begin 228 :end 238 :type "label" :path "dup" :face-runs ((0 8 org-ref-label-face nil nil "A label") (8 9 org-ref-label-face nil nil "A label") (9 10 nil nil nil nil))) (:needle "bibliography:references.bib" :begin 253 :end 280 :type "bibliography" :path "references.bib" :face-runs ((0 13 org-link nil nil "Bibliography link") (13 26 org-link nil nil "File exists at references.bib") (26 27 org-link nil nil "File exists at references.bib")))) :bib-font (("@article" 1 0 font-lock-function-name-face nil) ("ada2024deterministic" 1 9 font-lock-constant-face nil) ("Lovelace" 2 12 nil nil) ("Café" 3 25 nil nil)) :text "#+title: Activation 界\n#+include: \"chapter λ.org\"\n\n* Local\n:PROPERTIES:\n:CUSTOM_ID: local-target\n:END:\n\nValid [[cite:&ada2024deterministic]] missing [[cite:&missing-key]].\nRefs ref:local-target and ref:missing-target.\nDuplicate label:dup and label:dup.\nbibliography:references.bib\n" :modified nil) :cleanup clean)"##
        ]],
    )
}

fn documented_prefix_dispatch_and_real_completion_insertion() -> ParityBatchCase {
    ParityBatchCase::value(
        "documented-prefix-dispatch-and-real-completion-insertion",
        r####"(org-ref362-test-run
 "prefix-completion"
 (lambda (world)
   (let ((buffer (org-ref362-test-visit (plist-get world :main) #'org-mode))
         citation boundary recovery local-preparation local-drive
         local-reference included-drive included-reference
         label-first label-duplicate watch)
     (switch-to-buffer buffer)

     (org-ref362-test-reset-org-buffer "")
     (setq citation
           (list :drive
                 (org-ref362-test-drive-completion
                  "C-c ] G a m m a TAB RET")
                 :state (org-ref362-test-document-state)))
     (setq watch (org-ref362-test-own-current-watches))

     (org-ref362-test-reset-org-buffer "")
     (setq boundary
           (list :drive
                 (org-ref362-test-drive-completion "C-c ] 2 0 2 0 RET")
                 :state (org-ref362-test-document-state)))
     (org-ref362-test-run-command-loop "C-_")
     (setq recovery
           (list :after-undo (org-ref362-test-document-state)
                 :drive (org-ref362-test-drive-completion
                         "C-c ] G a m m a TAB RET")
                 :state (org-ref362-test-document-state)))

     (org-ref362-test-reset-org-buffer
      (concat "#+include: \"chapter λ.org\"\n\n"
              "\\begin{equation}\nE = h \\nu\n"
              "\\label{eq:local}\n\\end{equation}\n\n"))
     ;; Label discovery is file based.  Save through the real command loop so
     ;; `eq:local' is an actual completion candidate, not merely free input
     ;; accepted by the command's deliberate no-require-match boundary.
     (set-buffer-modified-p t)
     (setq local-preparation
           (list :save (org-ref362-test-run-command-loop "C-x C-s")
                 :state (org-ref362-test-document-state)))
     (goto-char (point-max))
     (setq local-drive
           (org-ref362-test-drive-completion
            "C-u C-c ] e q : l o c a l RET"))
     (unless (member "eq:local"
                     (plist-get (car (plist-get local-drive :reads))
                                :collection))
       (error "Saved local equation was absent from completion: %S"
              local-drive))
     (setq local-reference
           (list :preparation local-preparation
                 :drive local-drive
                 :state (org-ref362-test-document-state)))
     (insert "\n")
     (setq included-drive
           (org-ref362-test-drive-completion
            "C-u C-c ] e q : e n e r g y RET"))
     (unless (member "eq:energy"
                     (plist-get (car (plist-get included-drive :reads))
                                :collection))
       (error "Included equation was absent from completion: %S"
              included-drive))
     (setq included-reference
           (list :drive included-drive
                 :state (org-ref362-test-document-state)))

     (org-ref362-test-reset-org-buffer "")
     (setq label-first
           (list :drive
                 (org-ref362-test-drive-completion
                  "C-u C-u C-c ] n e w - 界 - l a b e l RET")
                 :state (org-ref362-test-document-state)))
     (setq label-first
           (append label-first
                   (list :save (org-ref362-test-run-command-loop "C-x C-s")
                         :saved-state (org-ref362-test-document-state))))
     (insert "\n")
     (setq org-ref362-test-warning-events nil)
     (advice-add 'display-warning :around #'org-ref362-test-warning-observer)
     (unwind-protect
         (setq label-duplicate
               (list :drive
                     (org-ref362-test-drive-completion
                      "C-u C-u C-c ] n e w - 界 - l a b e l RET")
                     :warnings (nreverse org-ref362-test-warning-events)
                     :state (org-ref362-test-document-state)))
       (advice-remove 'display-warning #'org-ref362-test-warning-observer))
     (list :citation citation :watch watch
           :no-require-match boundary :recovery recovery
           :local-reference local-reference
           :included-reference included-reference :label-first label-first
           :label-duplicate label-duplicate))))"####,
        expect![[
            r##"OK (:result (:citation (:drive (:loop (:point 20 :mark nil :active nil :unread nil :minibuffer-depth 0) :reads ((:prompt "org-ref BibTeX entries: " :collection ("Plain                        No External Locator                2019 misc   " "Gamma                        Structured Tools                   2020 book   " "Alpha, Beta                  Deterministic Widgets in Practice  2024 article" "Lovelace, Lei                Deterministic Café Workflow        2024 article") :require-match nil :initial nil :history nil :default nil :input "Gamma                        Structured Tools                   2020 book   " :selected "Gamma                        Structured Tools                   2020 book   " :history-after ("Gamma                        Structured Tools                   2020 book   ")))) :state (:text "[[cite:&gamma2020]]" :point 20 :mark nil :active nil :modified t :undo :present)) :watch (:bibliography ("references.bib") :count 1 :valid (t)) :no-require-match (:drive (:loop (:point 14 :mark nil :active nil :unread nil :minibuffer-depth 0) :reads ((:prompt "org-ref BibTeX entries: " :collection ("Plain                        No External Locator                2019 misc   " "Gamma                        Structured Tools                   2020 book   " "Alpha, Beta                  Deterministic Widgets in Practice  2024 article" "Lovelace, Lei                Deterministic Café Workflow        2024 article") :require-match nil :initial nil :history nil :default nil :input "2020" :selected "2020" :history-after ("2020" "Gamma                        Structured Tools                   2020 book   ")))) :state (:text "[[cite:&nil]]" :point 14 :mark nil :active nil :modified t :undo :present)) :recovery (:after-undo (:text "" :point 1 :mark nil :active nil :modified nil :undo :present) :drive (:loop (:point 20 :mark nil :active nil :unread nil :minibuffer-depth 0) :reads ((:prompt "org-ref BibTeX entries: " :collection ("Plain                        No External Locator                2019 misc   " "Gamma                        Structured Tools                   2020 book   " "Alpha, Beta                  Deterministic Widgets in Practice  2024 article" "Lovelace, Lei                Deterministic Café Workflow        2024 article") :require-match nil :initial nil :history nil :default nil :input "Gamma                        Structured Tools                   2020 book   " :selected "Gamma                        Structured Tools                   2020 book   " :history-after ("Gamma                        Structured Tools                   2020 book   " "2020" "Gamma                        Structured Tools                   2020 book   ")))) :state (:text "[[cite:&gamma2020]]" :point 20 :mark nil :active nil :modified t :undo :present)) :local-reference (:preparation (:save (:point 1 :mark nil :active nil :unread nil :minibuffer-depth 0) :state (:text "#+include: \"chapter λ.org\"\n\n\\begin{equation}\nE = h \\nu\n\\label{eq:local}\n\\end{equation}\n\n" :point 1 :mark nil :active nil :modified nil :undo :empty)) :drive (:loop (:point 103 :mark nil :active nil :unread nil :minibuffer-depth 0) :reads ((:prompt "Label: " :collection ("eq:local" "included-target" "eq:energy" "custom-λ" "table-界") :require-match nil :initial nil :history nil :default nil :input "eq:local" :selected "eq:local" :history-after ("eq:local" "Gamma                        Structured Tools                   2020 book   " "2020" "Gamma                        Structured Tools                   2020 book   ")))) :state (:text "#+include: \"chapter λ.org\"\n\n\\begin{equation}\nE = h \\nu\n\\label{eq:local}\n\\end{equation}\n\neqref:eq:local" :point 103 :mark nil :active nil :modified t :undo :present)) :included-reference (:drive (:loop (:point 117 :mark nil :active nil :unread nil :minibuffer-depth 0) :reads ((:prompt "Label: " :collection ("eq:local" "included-target" "eq:energy" "custom-λ" "table-界") :require-match nil :initial nil :history nil :default nil :input "eq:energy" :selected "eq:energy" :history-after ("eq:energy" "eq:local" "Gamma                        Structured Tools                   2020 book   " "2020" "Gamma                        Structured Tools                   2020 book   ")))) :state (:text "#+include: \"chapter λ.org\"\n\n\\begin{equation}\nE = h \\nu\n\\label{eq:local}\n\\end{equation}\n\neqref:eq:local\nref:eq:energy" :point 117 :mark nil :active nil :modified t :undo :present)) :label-first (:drive (:loop (:point 18 :mark nil :active nil :unread nil :minibuffer-depth 0) :reads ((:prompt "Label: " :collection ("eq:local") :require-match nil :initial nil :history nil :default nil :input "new-界-label" :selected "new-界-label" :history-after ("new-界-label" "eq:energy" "eq:local" "Gamma                        Structured Tools                   2020 book   " "2020" "Gamma                        Structured Tools                   2020 book   ")))) :state (:text "label:new-界-label" :point 18 :mark nil :active nil :modified t :undo :present) :save (:point 18 :mark nil :active nil :unread nil :minibuffer-depth 0) :saved-state (:text "label:new-界-label\n" :point 18 :mark nil :active nil :modified nil :undo :present)) :label-duplicate (:drive (:loop (:point 36 :mark nil :active nil :unread nil :minibuffer-depth 0) :reads ((:prompt "Label: " :collection ("new-界-label") :require-match nil :initial nil :history nil :default nil :input "new-界-label" :selected "new-界-label" :history-after ("new-界-label" "eq:energy" "eq:local" "Gamma                        Structured Tools                   2020 book   " "2020" "Gamma                        Structured Tools                   2020 book   ")))) :warnings ((:type emacs :message "Inserting duplicate label" :level nil :buffer-name nil)) :state (:text "label:new-界-label\nlabel:new-界-label\n" :point 36 :mark nil :active nil :modified t :undo :present))) :cleanup clean)"##
        ]],
    )
}

fn citation_edit_sort_and_bibtex_navigation() -> ParityBatchCase {
    ParityBatchCase::value(
        "citation-edit-sort-and-bibtex-navigation",
        r####"(org-ref362-test-run
 "citation-edit-navigation"
 (lambda (world)
   (org-ref362-test-write-main
    world
    "Unrelated Ω before.\n\nbibliography:references.bib\n\nUnrelated Ω after.\n")
   (let ((buffer (org-ref362-test-visit (plist-get world :main) #'org-mode))
         initial common local next previous shifted-right shifted-left sorted
         changed opened returned
         bib-move org-return)
     (switch-to-buffer buffer)
     (goto-char (point-min))
     (forward-line 1)
     (org-ref-insert-cite-keys '("ada2024deterministic" "gamma2020"))
     (font-lock-ensure)
     (setq initial (org-ref362-test-document-state))

     (goto-char (point-min))
     (search-forward "[[cite:")
     (backward-char)
     (org-ref362-test-run-command-loop
      "C-u M-x o r g - r e f - e d i t - p r e - p o s t - n o t e s RET C o m p a r e RET f o r SPC d e t a i l s RET")
     (font-lock-ensure)
     (setq common (org-ref362-test-document-state))

     (goto-char (point-min))
     (search-forward "gamma2020")
     (goto-char (match-beginning 0))
     (org-ref362-test-run-command-loop
      "M-x o r g - r e f - e d i t - p r e - p o s t - n o t e s RET s e e RET p . SPC 2 RET")
     (font-lock-ensure)
     (setq local (org-ref362-test-document-state))

     (goto-char (point-min))
     (search-forward "ada2024deterministic")
     (goto-char (match-beginning 0))
     (org-ref362-test-run-command-loop "C-<right>")
     (setq next (list (org-ref-get-bibtex-key-under-cursor) (point)))
     (org-ref362-test-run-command-loop "C-<left>")
     (setq previous (list (org-ref-get-bibtex-key-under-cursor) (point)))
     (org-ref362-test-run-command-loop "S-<right>")
     (font-lock-ensure)
     (setq shifted-right (org-ref362-test-document-state))
     (goto-char (point-min))
     (search-forward "ada2024deterministic")
     (goto-char (match-beginning 0))
     (org-ref362-test-run-command-loop "S-<left>")
     (font-lock-ensure)
     (setq shifted-left (org-ref362-test-document-state))
     (org-ref362-test-run-command-loop "S-<up>")
     (font-lock-ensure)
     (setq sorted (org-ref362-test-document-state))
     (org-ref362-test-run-command-loop
      "M-x o r g - r e f - c h a n g e - c i t e - t y p e RET c i t e p RET")
     (font-lock-ensure)
     (setq changed (org-ref362-test-document-state))

     (goto-char (point-min))
     (search-forward "gamma2020")
     (goto-char (match-beginning 0))
     (org-ref362-test-run-command-loop "M-.")
     (setq opened (list :entry (org-ref362-test-bibtex-entry-state)
                        :xref (org-ref362-test-xref-state)
                        :org (org-ref362-test-org-ring-state)))
     (setq bib-move
           (let ((origin (org-ref362-test-bibtex-entry-state)))
             (org-ref362-test-run-command-loop
              "M-x b i b t e x - n e x t - e n t r y RET")
             (let ((next-entry (org-ref362-test-bibtex-entry-state)))
               (org-ref362-test-run-command-loop
                "M-x b i b t e x - p r e v i o u s - e n t r y RET")
               (list :origin origin :next next-entry
                     :returned (org-ref362-test-bibtex-entry-state)))))
     (org-ref362-test-run-command-loop "M-,")
     (setq returned (list :point (org-ref362-test-point-state)
                          :xref (org-ref362-test-xref-state)
                          :org (org-ref362-test-org-ring-state)))
     (org-ref362-test-run-command-loop "C-c &")
     (setq org-return (list :point (org-ref362-test-point-state)
                            :org (org-ref362-test-org-ring-state)))
     (list :initial initial :common common :local local
           :next next :previous previous :sorted sorted
           :shifted-right shifted-right :shifted-left shifted-left
           :changed changed :opened opened :bib-movement bib-move
           :xref-return returned :org-return org-return
           :final (buffer-substring-no-properties (point-min) (point-max))))))"####,
        expect![[
            r#"OK (:result (:initial (:text "Unrelated Ω before.\n[[cite:&ada2024deterministic;&gamma2020]]\nbibliography:references.bib\n\nUnrelated Ω after.\n" :point 62 :mark nil :active nil :modified t :undo :present) :common (:text "Unrelated Ω before.\n[[cite:Compare;&ada2024deterministic;&gamma2020;for details]]\nbibliography:references.bib\n\nUnrelated Ω after.\n" :point 34 :mark nil :active nil :modified t :undo :present) :local (:text "Unrelated Ω before.\n[[cite:Compare;&ada2024deterministic;see &gamma2020 p. 2;for details]]\nbibliography:references.bib\n\nUnrelated Ω after.\n" :point 63 :mark nil :active nil :modified t :undo :present) :next ("gamma2020" 58) :previous ("ada2024deterministic" 36) :sorted (:text "Unrelated Ω before.\n[[cite:Compare;see &gamma2020 p. 2;&ada2024deterministic;for details]]\nbibliography:references.bib\n\nUnrelated Ω after.\n" :point 37 :mark nil :active nil :modified t :undo :present) :shifted-right (:text "Unrelated Ω before.\n[[cite:Compare;see &gamma2020 p. 2;&ada2024deterministic;for details]]\nbibliography:references.bib\n\nUnrelated Ω after.\n" :point 57 :mark nil :active nil :modified t :undo :present) :shifted-left (:text "Unrelated Ω before.\n[[cite:Compare;&ada2024deterministic;see &gamma2020 p. 2;for details]]\nbibliography:references.bib\n\nUnrelated Ω after.\n" :point 37 :mark nil :active nil :modified t :undo :present) :changed (:text "Unrelated Ω before.\n[[citep:Compare;see &gamma2020 p. 2;&ada2024deterministic;for details]]\nbibliography:references.bib\n\nUnrelated Ω after.\n" :point 37 :mark nil :active nil :modified t :undo :present) :opened (:entry (:file "references.bib" :mode bibtex-mode :line 31 :point 726 :column 0 :context "}" :narrowed nil :selected t :key "gamma2020" :type "book" :author "{Gamma, Grace}" :title "{Structured Tools}" :year "{2020}") :xref (:back (("paper 界.org" 42)) :forward nil) :org (("paper 界.org" 42))) :bib-movement (:origin (:file "references.bib" :mode bibtex-mode :line 31 :point 726 :column 0 :context "}" :narrowed nil :selected t :key "gamma2020" :type "book" :author "{Gamma, Grace}" :title "{Structured Tools}" :year "{2020}") :next (:file "references.bib" :mode bibtex-mode :line 37 :point 820 :column 0 :context "}" :narrowed nil :selected t :key "plain2019" :type "misc" :author "{Plain, Pat}" :title "{No External Locator}" :year "{2019}") :returned (:file "references.bib" :mode bibtex-mode :line 31 :point 726 :column 0 :context "}" :narrowed nil :selected t :key "gamma2020" :type "book" :author "{Gamma, Grace}" :title "{Structured Tools}" :year "{2020}")) :xref-return (:point (:file "paper 界.org" :mode org-mode :line 2 :point 42 :column 19 :context "[[citep:Compare;see &gamma2020 p. 2;&ada2024deterministic;for details]]" :narrowed nil :selected t) :xref (:back nil :forward (("references.bib" 552))) :org (("paper 界.org" 42))) :org-return (:point (:file "paper 界.org" :mode org-mode :line 2 :point 42 :column 19 :context "[[citep:Compare;see &gamma2020 p. 2;&ada2024deterministic;for details]]" :narrowed nil :selected t) :org (("paper 界.org" 42))) :final "Unrelated Ω before.\n[[citep:Compare;see &gamma2020 p. 2;&ada2024deterministic;for details]]\nbibliography:references.bib\n\nUnrelated Ω after.\n") :cleanup clean)"#
        ]],
    )
}

fn local_and_included_reference_navigation() -> ParityBatchCase {
    ParityBatchCase::value(
        "local-and-included-reference-navigation",
        r####"(org-ref362-test-run
 "reference-navigation"
 (lambda (world)
   (org-ref362-test-write-main
    world
    (concat
     "#+title: Reference Navigation\n#+include: \"chapter λ.org\"\n\n"
     "* Local\n:PROPERTIES:\n:CUSTOM_ID: local-target\n:END:\n\n"
     "Local ref:local-target. Included ref:included-target. "
     "Equation eqref:eq:energy. Custom ref:custom-λ. Missing ref:recovered-界.\n"
     "bibliography:references.bib\n"))
   (let ((main (org-ref362-test-visit (plist-get world :main) #'org-mode))
         included included-back local local-returns equation equation-back
         custom custom-back missing missing-back cache-insertion recovery)
     (switch-to-buffer main)
     (font-lock-ensure)

     (goto-char (point-min))
     (search-forward "ref:included-target")
     (goto-char (match-beginning 0))
     (setq included
           (list :origin (org-ref362-test-point-state)
                 :drive (org-ref362-test-drive-with-messages "C-c C-o")
                 :destination (org-ref362-test-point-state)
                 :ring (org-ref362-test-org-ring-state)))
     (org-ref362-test-run-command-loop "C-c &")
     (setq included-back (list :point (org-ref362-test-point-state)
                               :ring (org-ref362-test-org-ring-state)))

     (goto-char (point-min))
     (search-forward "ref:local-target")
     (goto-char (match-beginning 0))
     (narrow-to-region (line-beginning-position) (line-end-position))
     (setq local
           (list :origin (org-ref362-test-point-state)
                 :drive (org-ref362-test-drive-with-messages "C-c C-o")
                 :destination (org-ref362-test-point-state)
                 :ring (org-ref362-test-org-ring-state)))
     (setq local-returns
           (org-ref362-test-drive-mark-returns "C-c & C-c &"))

     (goto-char (point-min))
     (search-forward "eqref:eq:energy")
     (goto-char (match-beginning 0))
     (setq equation (progn (org-ref362-test-run-command-loop "C-c C-o")
                           (org-ref362-test-point-state)))
     (org-ref362-test-run-command-loop "C-c &")
     (setq equation-back (org-ref362-test-point-state))

     (goto-char (point-min))
     (search-forward "ref:custom-λ")
     (goto-char (match-beginning 0))
     (setq custom (progn (org-ref362-test-run-command-loop "C-c C-o")
                         (org-ref362-test-point-state)))
     (org-ref362-test-run-command-loop "C-c &")
     (setq custom-back (org-ref362-test-point-state))

     (goto-char (point-min))
     (search-forward "ref:recovered-界")
     (goto-char (match-beginning 0))
     (let ((before (list (org-ref362-test-point-state)
                         (org-ref362-test-org-ring-state))))
       (setq missing
             (list :before before
                   :drive (org-ref362-test-drive-with-messages "C-c C-o")
                   :after (list (org-ref362-test-point-state)
                                (org-ref362-test-org-ring-state)))))
     (org-ref362-test-run-command-loop "C-c &")
     (setq missing-back (list :point (org-ref362-test-point-state)
                              :ring (org-ref362-test-org-ring-state)))
     (with-current-buffer (org-ref362-test-visit
                           (plist-get world :chapter) #'org-mode)
       (goto-char (point-max))
       (insert "\n#+name: recovered-界\nRecovered body.\n")
       (save-buffer))
     (switch-to-buffer main)
     (goto-char (point-max))
     (insert "\nCache insertion: ")
     (setq cache-insertion
           (list :drive
                 (org-ref362-test-drive-completion
                  "C-u C-c ] r e c o v e r e d - 界 RET")
                 :state (org-ref362-test-document-state)))
     (goto-char (point-min))
     (search-forward "ref:recovered-界")
     (goto-char (match-beginning 0))
     (setq recovery
           (list :drive (org-ref362-test-drive-with-messages "C-c C-o")
                 :destination (org-ref362-test-point-state)
                 :ring (org-ref362-test-org-ring-state)))
     (org-ref362-test-run-command-loop "C-c &")
     (list :included included :included-back included-back
           :local local :local-returns local-returns
           :equation equation :equation-back equation-back
           :custom custom :custom-back custom-back
           :missing missing :missing-back missing-back
           :cache-insertion cache-insertion :recovery recovery
           :final (org-ref362-test-point-state)))))"####,
        expect![[
            r##"OK (:result (:included (:origin (:file "paper 界.org" :mode org-mode :line 9 :point 145 :column 33 :context "Local ref:local-target. Included ref:included-target. Equation eqref:eq:energy. Custom ref:custom-λ. Missing ref:recovered-界." :narrowed nil :selected t) :drive (:loop (:point 38 :mark nil :active nil :unread nil :minibuffer-depth 0) :messages ("" "Position saved to mark ring, go back with ‘C-c &’." "Go back with (org-mark-ring-goto) C-c &." "Label ’included-target’ not found in current file or included files")) :destination (:file "chapter λ.org" :mode org-mode :line 3 :point 38 :column 8 :context "#+name: included-target" :narrowed nil :selected t) :ring (("paper 界.org" 145))) :included-back (:point (:file "paper 界.org" :mode org-mode :line 9 :point 145 :column 33 :context "Local ref:local-target. Included ref:included-target. Equation eqref:eq:energy. Custom ref:custom-λ. Missing ref:recovered-界." :narrowed nil :selected t) :ring (("paper 界.org" 145))) :local (:origin (:file "paper 界.org" :mode org-mode :line 1 :point 118 :column 6 :context "Local ref:local-target. Included ref:included-target. Equation eqref:eq:energy. Custom ref:custom-λ. Missing ref:recovered-界." :narrowed t :selected t) :drive (:loop (:point 92 :mark 104 :active t :unread nil :minibuffer-depth 0) :messages ("" "Position saved to mark ring, go back with ‘C-c &’." "Position saved to mark ring, go back with ‘C-c &’." "Go back with (org-mark-ring-goto) C-c &.")) :destination (:file "paper 界.org" :mode org-mode :line 6 :point 92 :column 12 :context ":CUSTOM_ID: local-target" :narrowed nil :selected t) :ring (("paper 界.org" 104) ("paper 界.org" 118) ("paper 界.org" 145))) :local-returns (:loop (:point 118 :mark 104 :active t :unread nil :minibuffer-depth 0) :returns ((:value nil :point (:file "paper 界.org" :mode org-mode :line 6 :point 104 :column 24 :context ":CUSTOM_ID: local-target" :narrowed nil :selected t) :ring (("paper 界.org" 104) ("paper 界.org" 118) ("paper 界.org" 145))) (:value nil :point (:file "paper 界.org" :mode org-mode :line 9 :point 118 :column 6 :context "Local ref:local-target. Included ref:included-target. Equation eqref:eq:energy. Custom ref:custom-λ. Missing ref:recovered-界." :narrowed nil :selected t) :ring (("paper 界.org" 104) ("paper 界.org" 118) ("paper 界.org" 145))))) :equation (:file "chapter λ.org" :mode org-mode :line 8 :point 110 :column 7 :context "\\label{eq:energy}" :narrowed nil :selected t) :equation-back (:file "paper 界.org" :mode org-mode :line 9 :point 175 :column 63 :context "Local ref:local-target. Included ref:included-target. Equation eqref:eq:energy. Custom ref:custom-λ. Missing ref:recovered-界." :narrowed nil :selected t) :custom (:file "chapter λ.org" :mode org-mode :line 13 :point 180 :column 12 :context ":CUSTOM_ID: custom-λ" :narrowed nil :selected t) :custom-back (:file "paper 界.org" :mode org-mode :line 9 :point 199 :column 87 :context "Local ref:local-target. Included ref:included-target. Equation eqref:eq:energy. Custom ref:custom-λ. Missing ref:recovered-界." :narrowed nil :selected t) :missing (:before ((:file "paper 界.org" :mode org-mode :line 9 :point 221 :column 109 :context "Local ref:local-target. Included ref:included-target. Equation eqref:eq:energy. Custom ref:custom-λ. Missing ref:recovered-界." :narrowed nil :selected t) (("paper 界.org" 199) ("paper 界.org" 175) ("paper 界.org" 104) ("paper 界.org" 118))) :drive (:loop (:point 104 :mark 221 :active t :unread nil :minibuffer-depth 0) :messages ("" "Position saved to mark ring, go back with ‘C-c &’." "Label ’recovered-界’ not found in current file or included files")) :after ((:file "paper 界.org" :mode org-mode :line 6 :point 104 :column 24 :context ":CUSTOM_ID: local-target" :narrowed nil :selected t) (("paper 界.org" 221) ("paper 界.org" 199) ("paper 界.org" 175) ("paper 界.org" 104)))) :missing-back (:point (:file "paper 界.org" :mode org-mode :line 9 :point 221 :column 109 :context "Local ref:local-target. Included ref:included-target. Equation eqref:eq:energy. Custom ref:custom-λ. Missing ref:recovered-界." :narrowed nil :selected t) :ring (("paper 界.org" 221) ("paper 界.org" 199) ("paper 界.org" 175) ("paper 界.org" 104))) :cache-insertion (:drive (:loop (:point 299 :mark 221 :active t :unread nil :minibuffer-depth 0) :reads ((:prompt "Label: " :collection ("local-target" "included-target" "eq:energy" "custom-λ" "table-界" "recovered-界") :require-match nil :initial nil :history nil :default nil :input "recovered-界" :selected "recovered-界" :history-after ("recovered-界")))) :state (:text "#+title: Reference Navigation\n#+include: \"chapter λ.org\"\n\n* Local\n:PROPERTIES:\n:CUSTOM_ID: local-target\n:END:\n\nLocal ref:local-target. Included ref:included-target. Equation eqref:eq:energy. Custom ref:custom-λ. Missing ref:recovered-界.\nbibliography:references.bib\n\nCache insertion: ref:recovered-界" :point 299 :mark 221 :active t :modified t :undo :present)) :recovery (:drive (:loop (:point 269 :mark nil :active nil :unread nil :minibuffer-depth 0) :messages ("" "Position saved to mark ring, go back with ‘C-c &’." "Go back with (org-mark-ring-goto) C-c &." "Label ’recovered-界’ not found in current file or included files")) :destination (:file "chapter λ.org" :mode org-mode :line 21 :point 269 :column 8 :context "#+name: recovered-界" :narrowed nil :selected t) :ring (("paper 界.org" 221) ("paper 界.org" 221) ("paper 界.org" 199) ("paper 界.org" 175))) :final (:file "paper 界.org" :mode org-mode :line 9 :point 221 :column 109 :context "Local ref:local-target. Included ref:included-target. Equation eqref:eq:energy. Custom ref:custom-λ. Missing ref:recovered-界." :narrowed nil :selected t)) :cleanup clean)"##
        ]],
    )
}

fn citation_browser_boundary_and_failure_recovery() -> ParityBatchCase {
    ParityBatchCase::value(
        "citation-browser-boundary-and-failure-recovery",
        r####"(org-ref362-test-run
 "browser-boundary"
 (lambda (world)
   (org-ref362-test-write-main
    world
    (concat "[[cite:&ada2024deterministic]]\n"
            "[[cite:&gamma2020]]\n[[cite:&plain2019]]\n"
            "bibliography:references.bib\n"))
   (let ((buffer (org-ref362-test-visit (plist-get world :main) #'org-mode))
         explicit fallback neither failed retry)
     (switch-to-buffer buffer)
     (font-lock-ensure)
     (let ((browse-url-browser-function #'org-ref362-test-browser-recorder))
       (cl-labels
           ((at (key)
              (goto-char (point-min))
              (search-forward key)
              (goto-char (match-beginning 0))))
         (setq org-ref362-test-browser-calls nil)
         (at "ada2024deterministic")
         (setq explicit
               (list :value (org-ref-open-url-at-point)
                     :calls (org-ref362-test-browser-state)))
         (setq org-ref362-test-browser-calls nil)
         (at "gamma2020")
         (setq fallback
               (list :value (org-ref-open-url-at-point)
                     :calls (org-ref362-test-browser-state)))
         (setq org-ref362-test-browser-calls nil)
         (at "plain2019")
         (setq neither
               (list :value (org-ref-open-url-at-point)
                     :calls (org-ref362-test-browser-state)))
         (setq org-ref362-test-browser-calls nil
               org-ref362-test-browser-error
               "controlled browser failure Ω")
         (at "ada2024deterministic")
         (let ((before (org-ref362-test-document-state)))
           (setq failed
                 (condition-case condition
                     (list :value (org-ref-open-url-at-point))
                   (t (list :condition
                            (org-ref362-test-condition-state condition)
                            :calls (org-ref362-test-browser-state)
                            :unchanged (equal before
                                              (org-ref362-test-document-state)))))))
         (setq org-ref362-test-browser-calls nil
               org-ref362-test-browser-error nil)
         (at "ada2024deterministic")
         (setq retry
               (list :value (org-ref-open-url-at-point)
                     :calls (org-ref362-test-browser-state)))))
     (list :explicit explicit :fallback fallback :neither neither
           :failure failed :retry retry
           :source (org-ref362-test-document-state)))))"####,
        expect![[
            r#"OK (:result (:explicit (:value nil :calls ((:url "https://example.invalid/explicit?x=1" :new-window nil))) :fallback (:value nil :calls ((:url "http://dx.doi.org/10.1000/fallback" :new-window nil))) :neither (:value nil :calls nil) :failure (:condition (:symbol error :data ("controlled browser failure Ω") :message "controlled browser failure Ω") :calls ((:url "https://example.invalid/explicit?x=1" :new-window nil)) :unchanged t) :retry (:value nil :calls ((:url "https://example.invalid/explicit?x=1" :new-window nil))) :source (:text "[[cite:&ada2024deterministic]]\n[[cite:&gamma2020]]\n[[cite:&plain2019]]\nbibliography:references.bib\n" :point 9 :mark nil :active nil :modified nil :undo :empty)) :cleanup clean)"#
        ]],
    )
}

fn real_latex_and_csl_export_failure_recovery() -> ParityBatchCase {
    ParityBatchCase::value(
        "real-latex-and-csl-export-failure-recovery",
        r####"(org-ref362-test-run
 "real-export"
 (lambda (world)
   (org-ref362-test-write-main
    world
    (concat
     "#+title: Valid Export Ω\n#+include: \"chapter λ.org\"\n"
     "#+latex_header: \\makeglossaries\n#+latex_header: \\makeindex\n\n"
     "* Local\n:PROPERTIES:\n:CUSTOM_ID: local-target\n:END:\n\n"
     "See [[citep:Compare &gamma2020 chap. 2;&alpha2024 pp. 11-12;for details]].\n"
     "References ref:local-target and ref:included-target; label:export-label.\n"
     "Terms gls:widget and acrfull:rsp with [[index:Ω widget]].\n\n"
     "#+name: glossary\n| label | name   | description |\n"
     "| widget | Widget 界 | Deterministic tool |\n\n"
     "#+name: acronyms\n| key | abbreviation | full form |\n"
     "| rsp | RSP | reproducible system proof |\n\n"
     "bibliography:references.bib\n"))
   (let* ((source (org-ref362-test-visit (plist-get world :main) #'org-mode))
          source-before source-after-latex source-after-csl latex csl
          export-before-failure source-before-failure missing-style
          export-after-failure source-after-failure retry-style)
     (switch-to-buffer source)
     (font-lock-ensure)
     (setq source-before (org-ref362-test-document-state))
     (setq latex (org-export-as 'latex nil nil t))
     (setq source-after-latex
           (with-current-buffer source (org-ref362-test-document-state)))

     (switch-to-buffer source)
     (org-ref-export-as-org nil nil nil nil nil)
     (setq csl (org-ref362-test-output-state "*org-ref ORG Export*"))
     (setq source-after-csl
           (with-current-buffer source (org-ref362-test-document-state)))

     (switch-to-buffer source)
     (setq export-before-failure
           (org-ref362-test-output-state "*org-ref ORG Export*")
           source-before-failure (org-ref362-test-document-state)
           org-ref-footnote-counter 37)
     (let ((org-ref-csl-default-style "missing-style-界.csl"))
       (setq missing-style
             (condition-case condition
                 (list :value (org-ref-export-as-org nil nil nil nil nil))
               (t (list :condition
                        (org-ref362-test-condition-state condition)
                        :footnote-counter org-ref-footnote-counter)))))
     (setq export-after-failure
           (org-ref362-test-output-state "*org-ref ORG Export*")
           source-after-failure
           (with-current-buffer source (org-ref362-test-document-state)))

     (switch-to-buffer source)
     (setq org-ref-csl-default-style "chicago-author-date-16th-edition.csl")
     (org-ref-export-as-org nil nil nil nil nil)
     (setq retry-style
           (org-ref362-test-output-state "*org-ref ORG Export*"))
     (list
      :latex (list :cite
                   (and (string-match
                         "\\\\citep\\[Compare\\]\\[for details\\]{gamma2020,alpha2024}"
                         latex)
                        (match-string 0 latex))
                   :ref (and (string-match "\\\\ref{local-target}" latex)
                             (match-string 0 latex))
                   :bibliography
                   (and (string-match "\\\\bibliography{references}" latex)
                        (match-string 0 latex))
                   :glossary
                   (and (string-match "\\\\gls{widget}" latex)
                        (match-string 0 latex))
                   :acronym
                   (and (string-match "\\\\acrfull{rsp}" latex)
                        (match-string 0 latex))
                   :index
                   (and (string-match "\\\\index{Ω widget}" latex)
                        (match-string 0 latex))
                   :text latex)
      :source source-before
      :source-after-latex (equal source-before source-after-latex)
      :csl csl
      :source-after-csl (equal source-before source-after-csl)
      :missing-style missing-style
      :pre-failure
      (list :source-equal (equal source-before source-before-failure)
            :output-equal (equal csl export-before-failure))
      :failure-source-unchanged
      (equal source-before-failure source-after-failure)
      :failure-output-unchanged
      (equal export-before-failure export-after-failure)
      :style-recovery (equal csl retry-style)
      :recovery-source
      (with-current-buffer source
        (equal source-before (org-ref362-test-document-state)))))))"####,
        expect![[
            r##"OK (:result (:latex (:cite "\\citep[Compare][for details]{gamma2020,alpha2024}" :ref "\\ref{local-target}" :bibliography "\\bibliography{references}" :glossary "\\gls{widget}" :acronym "\\acrfull{rsp}" :index "\\index{Ω widget}" :text "Included target body.\n\n\\begin{equation}\nE = mc^2\n\\label{eq:energy}\n\\end{equation}\n\\section{Included custom}\n\\label{custom-λ}\nCustom target body.\n\n\\begin{table}[htbp]\n\\label{table-界}\n\\centering\n\\begin{tabular}{rl}\nn & value\\\\\n1 & λ\\\\\n\\end{tabular}\n\\end{table}\n\\section{Local}\n\\label{local-target}\nSee \\citep[Compare][for details]{gamma2020,alpha2024}.\nReferences \\ref{local-target} and \\ref{included-target}; \\label{export-label}.\nTerms \\gls{widget} and \\acrfull{rsp} with \\index{Ω widget}.\n\n\\begin{table}[htbp]\n\\label{glossary}\n\\centering\n\\begin{tabular}{lll}\nlabel & name & description\\\\\nwidget & Widget 界 & Deterministic tool\\\\\n\\end{tabular}\n\\end{table}\n\n\\begin{table}[htbp]\n\\label{acronyms}\n\\centering\n\\begin{tabular}{lll}\nkey & abbreviation & full form\\\\\nrsp & RSP & reproducible system proof\\\\\n\\end{tabular}\n\\end{table}\n\n\\bibliography{references}\n") :source (:text "#+title: Valid Export Ω\n#+include: \"chapter λ.org\"\n#+latex_header: \\makeglossaries\n#+latex_header: \\makeindex\n\n* Local\n:PROPERTIES:\n:CUSTOM_ID: local-target\n:END:\n\nSee [[citep:Compare &gamma2020 chap. 2;&alpha2024 pp. 11-12;for details]].\nReferences ref:local-target and ref:included-target; label:export-label.\nTerms gls:widget and acrfull:rsp with [[index:Ω widget]].\n\n#+name: glossary\n| label | name   | description |\n| widget | Widget 界 | Deterministic tool |\n\n#+name: acronyms\n| key | abbreviation | full form |\n| rsp | RSP | reproducible system proof |\n\nbibliography:references.bib\n" :point 1 :mark nil :active nil :modified nil :undo :empty) :source-after-latex t :csl (:name "*org-ref ORG Export*" :mode org-mode :text "#+title: Valid Export Ω Included λ Chapter\n#+name: included-target\nIncluded target body.\n\n\\begin{equation}\nE = mc^2\n\\label{eq:energy}\n\\end{equation}\n* Included custom\nCustom target body.\n\n#+name: table-界\n| n | value |\n| 1 | λ     |\n#+latex_header: \\makeglossaries\n#+latex_header: \\makeindex\n* Local\nSee (Compare [[citeproc_bib_item_2][Gamma 2020, 2]]; [[citeproc_bib_item_1][Alpha and Beta 2024, 11–12]] for details).\nReferences ref:local-target and ref:included-target; label:export-label.\nTerms widget and RSP with [[index:Ω widget]].\n\n#+name: glossary\n| label  | name      | description        |\n| widget | Widget 界 | Deterministic tool |\n\n#+name: acronyms\n| key | abbreviation | full form                 |\n| rsp | RSP          | reproducible system proof |\n\n<<citeproc_bib_item_1>>Alpha, Ada, and Bob Beta. 2024. “Deterministic Widgets in Practice.” /Journal of Reproducible Examples/ 7 (2): 11–19. doi:[[https://doi.org/10.1000/alpha][10.1000/alpha]].\n\n<<citeproc_bib_item_2>>Gamma, Grace. 2020. /Structured Tools/. Test City: Example Press. doi:[[https://doi.org/10.1000/fallback][10.1000/fallback]].\n" :point 1109 :modified t :read-only nil :narrowed nil :min 1 :max 1109) :source-after-csl t :missing-style (:condition (:symbol error :data ("missing-style-界.csl not found") :message "missing-style-界.csl not found") :footnote-counter 0) :pre-failure (:source-equal t :output-equal t) :failure-source-unchanged t :failure-output-unchanged t :style-recovery t :recovery-source t) :cleanup clean)"##
        ]],
    )
}

fn public_analysis_click_fix_and_recovery() -> ParityBatchCase {
    ParityBatchCase::value(
        "public-analysis-click-fix-and-recovery",
        r####"(org-ref362-test-run
 "analysis-fix"
 (lambda (world)
   (org-ref362-test-write-main
    world
    (concat
     "#+title: Analysis Ω\n#+include: \"chapter λ.org\"\n\n"
     "* Local\n:PROPERTIES:\n:CUSTOM_ID: local-target\n:END:\n\n"
     "Valid [[cite:&gamma2020]], missing [[cite:&missing-key]].\n"
     "Valid ref:local-target, bad ref:missing-target.\n"
     "Duplicate label:dup and label:dup.\n"
     "Missing file [[file:missing-界.txt]].\n"
     "bibliography:references.bib\n"))
   (let* ((source (org-ref362-test-visit (plist-get world :main) #'org-mode))
          source-before file-failure partial-report file-recovery-drive
          report-before followed recovery-drive report-after)
     (switch-to-buffer source)
     (font-lock-ensure)
     (setq source-before (org-ref362-test-document-state))
     (setq file-failure
           (condition-case condition
               (list :value
                     (org-ref362-test-run-command-loop
                      "M-x o r g - r e f RET"))
             (t (list :condition
                      (org-ref362-test-condition-state condition)))))
     (setq partial-report (org-ref362-test-report-state))
     (setq file-failure
           (append file-failure
                   (list :source-unchanged
                         (equal source-before
                                (with-current-buffer source
                                  (org-ref362-test-document-state)))
                         :report partial-report)))

     ;; Repair only the missing-file boundary.  The public rerun must now
     ;; produce the citation/ref/duplicate report that the pinned dotted-pair
     ;; bug prevented it from completing above.
     (org-ref362-test-write (expand-file-name "missing-界.txt"
                                              (plist-get world :root))
                            "owned\n")
     (switch-to-buffer source)
     (setq file-recovery-drive
           (org-ref362-test-run-command-loop "M-x o r g - r e f RET"))
     (setq report-before (org-ref362-test-report-state))
     (let ((report (get-buffer "*org-ref*")))
       (select-window (get-buffer-window report))
       (with-current-buffer report
         (goto-char (point-min))
         (search-forward "* Bad citations")
         (search-forward "[[elisp:")
         (goto-char (match-beginning 0)))
       (setq org-ref362-test-confirmation-events nil)
       (let ((org-link-elisp-confirm-function
              #'org-ref362-test-confirm-elisp-link))
         (setq followed
               (list :drive (org-ref362-test-run-command-loop "C-c C-o")
                     :confirmation
                     (nreverse
                      (copy-sequence org-ref362-test-confirmation-events))
                     :destination (org-ref362-test-point-state)))))

     (switch-to-buffer source)
     (goto-char (point-min))
     (while (search-forward "missing-key" nil t)
       (replace-match "alpha2024" t t))
     (goto-char (point-min))
     (while (search-forward "missing-target" nil t)
       (replace-match "local-target" t t))
     (goto-char (point-min))
     (search-forward "Duplicate label:dup and label:dup.")
     (replace-match "Single label:dup." t t)
     (save-buffer)
     (setq recovery-drive
           (org-ref362-test-run-command-loop "M-x o r g - r e f RET"))
     (setq report-after (org-ref362-test-report-state))
     (list :source-before source-before :file-failure file-failure
           :file-recovery-drive file-recovery-drive
           :report-before report-before :followed followed
           :recovery-drive recovery-drive :report-after report-after
           :fixed-source
           (with-current-buffer source (org-ref362-test-document-state))))))"####,
        expect![[
            r##"OK (:result (:source-before (:text "#+title: Analysis Ω\n#+include: \"chapter λ.org\"\n\n* Local\n:PROPERTIES:\n:CUSTOM_ID: local-target\n:END:\n\nValid [[cite:&gamma2020]], missing [[cite:&missing-key]].\nValid ref:local-target, bad ref:missing-target.\nDuplicate label:dup and label:dup.\nMissing file [[file:missing-界.txt]].\nbibliography:references.bib\n" :point 1 :mark nil :active nil :modified nil :undo :empty) :file-failure (:condition (:symbol wrong-type-argument :data (listp (:marker "paper 界.org" 256)) :message "Wrong type argument: listp, #<marker at 256 in paper 界.org>") :source-unchanged t :report (:mode org-mode :read-only nil :headings ("Bad citations" "Bad ref links" "Multiply defined label links" "Bad files") :owned-source-title t :bad-citations "* Bad citations\n- [[elisp:(progn (switch-to-buffer \"paper 界.org\") (goto-char 156)(org-show-entry))][missing-key]]\n\n" :bad-refs "* Bad ref links\n- [[elisp:(progn (switch-to-buffer \"paper 界.org\") (goto-char 188)(org-show-entry))][missing-target]]\n\n" :bad-labels "* Multiply defined label links\n- [[elisp:(progn (switch-to-buffer \"paper 界.org\") (goto-char 228)(org-show-entry))][dup]]\n\n" :bad-files "* Bad files\n")) :file-recovery-drive (:point 1 :mark nil :active nil :unread nil :minibuffer-depth 0) :report-before (:mode org-mode :read-only t :headings ("Bad citations" "Bad ref links" "Multiply defined label links" "Unreferenced label links" "Bibliography" "Miscellaneous" "LaTeX setup" "Warnings" "Utilities") :owned-source-title t :bad-citations "* Bad citations\n- [[elisp:(progn (switch-to-buffer \"paper 界.org\") (goto-char 156)(org-show-entry))][missing-key]]\n\n" :bad-refs "* Bad ref links\n- [[elisp:(progn (switch-to-buffer \"paper 界.org\") (goto-char 188)(org-show-entry))][missing-target]]\n\n" :bad-labels "* Multiply defined label links\n- [[elisp:(progn (switch-to-buffer \"paper 界.org\") (goto-char 228)(org-show-entry))][dup]]\n\n" :bad-files nil) :followed (:drive (:point 156 :mark nil :active nil :unread nil :minibuffer-depth 0) :confirmation ("Execute (progn (switch-to-buffer \"paper 界.org\") (goto-char 156)(org-show-entry)) as Elisp? ") :destination (:file "paper 界.org" :mode org-mode :line 9 :point 156 :column 48 :context "Valid [[cite:&gamma2020]], missing [[cite:&missing-key]]." :narrowed nil :selected t)) :recovery-drive (:point 1 :mark nil :active nil :unread nil :minibuffer-depth 0) :report-after (:mode org-mode :read-only t :headings ("Unreferenced label links" "Bibliography" "Miscellaneous" "LaTeX setup" "Warnings" "Utilities") :owned-source-title t :bad-citations nil :bad-refs nil :bad-labels nil :bad-files nil) :fixed-source (:text "#+title: Analysis Ω\n#+include: \"chapter λ.org\"\n\n* Local\n:PROPERTIES:\n:CUSTOM_ID: local-target\n:END:\n\nValid [[cite:&gamma2020]], missing [[cite:&alpha2024]].\nValid ref:local-target, bad ref:local-target.\nSingle label:dup.\nMissing file [[file:missing-界.txt]].\nbibliography:references.bib\n" :point 221 :mark nil :active nil :modified nil :undo :present)) :cleanup clean)"##
        ]],
    )
}

pub(super) fn org_ref_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        activation_and_real_document_fontification(),
        documented_prefix_dispatch_and_real_completion_insertion(),
        citation_edit_sort_and_bibtex_navigation(),
        local_and_included_reference_navigation(),
        citation_browser_boundary_and_failure_recovery(),
        real_latex_and_csl_export_failure_recovery(),
        public_analysis_click_fix_and_recovery(),
    ]
}
