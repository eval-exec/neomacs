use expect_test::expect;

use super::ParityBatchCase;

fn turning_amsreftex_on_takes_over_reftexs_citation_machinery_and_off_leaves_one_override()
-> ParityBatchCase {
    ParityBatchCase::value(
        "turning_amsreftex_on_takes_over_reftexs_citation_machinery_and_off_leaves_one_override",
        r##"
(let ((registered
       (list :auto-mode (cdr (assoc "\\.ltb\\'" auto-mode-alist))
             :extensions (assoc "ltb" reftex-file-extensions)
             :external-finder (assoc "ltb" reftex-external-file-finders)
             :path-variables reftex-ltbpath-environment-variables))
      (before (list :amsreftex-p amsreftex-p
                    :advice (amsref-test-advice)
                    :latex-font-lock
                    (cdr (assq 'latex-mode font-lock-keywords-alist)))))
  (amsreftex-turn-on)
  (let ((on (list :amsreftex-p amsreftex-p
                  :advice (amsref-test-advice)
                  :latex-font-lock
                  (amsref-test-plain
                   (cdr (assq 'latex-mode font-lock-keywords-alist))))))
    (amsreftex-turn-off)
    (list
     :registered-when-loaded registered
     :before-turn-on before
     :after-turn-on on
     :after-turn-off
     (list :amsreftex-p amsreftex-p
           :advice (amsref-test-advice)
           :latex-font-lock
           (cdr (assq 'latex-mode font-lock-keywords-alist)))
     ;; The advice left behind is not inert: `amsreftex-end-of-bib-entry'
     ;; walks three brace groups of a \bib record where vanilla RefTeX's
     ;; `forward-list 1' stops after the citation key.
     :end-of-bib-entry-with-amsreftex-off
     (with-temp-buffer
       (insert "\\bib{noether1918}{article}{\n  author={Noether, Emmy},\n}\nafter\n")
       (goto-char (point-min))
       (let ((end (reftex-end-of-bib-entry nil)))
         (list :end end
               :covers (buffer-substring-no-properties (point-min) end))))
     :turning-off-again
     (condition-case failure (amsreftex-turn-off)
       (error (amsref-test-plain failure))))))
"##,
        expect![[
            r#"OK (:registered-when-loaded (:auto-mode latex-mode :extensions ("ltb" ".ltb") :external-finder ("ltb" . "kpsewhich %f.ltb") :path-variables ("TEXINPUTS")) :before-turn-on (:amsreftex-p nil :advice nil :latex-font-lock nil) :after-turn-on (:amsreftex-p t :advice ((reftex-locate-bibliography-files amsreftex-advise-reftex-locate-bibliography-files) (reftex-parse-bibtex-entry amsreftex-advise-reftex-parse-bibtex-entry) (reftex-get-crossref-alist amsreftex-advise-reftex-get-crossref-alist) (reftex-extract-bib-entries amsreftex-advise-reftex-extract-bib-entries) (reftex-extract-bib-entries-from-thebibliography amsreftex-advise-reftex-extract-bib-entries-from-thebibliography) (reftex-pop-to-bibtex-entry amsreftex-advise-reftex-pop-to-bibtex-entry) (reftex-echo-cite amsreftex-set-last-arg-to-nil) (reftex-parse-from-file amsreftex-parse-from-file) (reftex-bibtex-selection-callback amsreftex-database-selection-callback) (reftex-end-of-bib-entry amsreftex-end-of-bib-entry)) :latex-font-lock (((("^[ \11]*\\(\\\\bib[*]?\\){\\(\\(?:\\w\\|\\s_\\)+\\)}{\\(\\w+\\)}{" (1 font-lock-keyword-face) (2 font-lock-type-face) (3 font-lock-function-name-face)) ("^[ \11]*\\(\\(?:\\w\\|-\\)+\\)[ \11\n\15]*=[ \11\n\15]*{" (1 font-lock-variable-name-face)))))) :after-turn-off (:amsreftex-p nil :advice ((reftex-end-of-bib-entry amsreftex-end-of-bib-entry)) :latex-font-lock nil) :end-of-bib-entry-with-amsreftex-off (:end 56 :covers "\\bib{noether1918}{article}{\n  author={Noether, Emmy},\n}") :turning-off-again (user-error "Amsreftex is not turned on!"))"#
        ]],
    )
}

fn an_amsrefs_document_is_recognised_and_each_named_database_is_located_or_dropped()
-> ParityBatchCase {
    ParityBatchCase::value(
        "an_amsrefs_document_is_recognised_and_each_named_database_is_located_or_dropped",
        r##"
(progn
  (amsreftex-turn-on)
  (amsref-test-write
   "recognition/mathbib.ltb"
   "\\begin{biblist}
\\bib{noether1918}{article}{
  author={Noether, Emmy},
  title={Invariante Variationsprobleme},
  date={1918},
}
\\end{biblist}
")
  (amsref-test-write
   "recognition/elsewhere/faraway.ltb"
   "\\begin{biblist}
\\bib{maxwell1865}{article}{
  author={Maxwell, James Clerk},
  title={A dynamical theory of the electromagnetic field},
  date={1865},
}
\\end{biblist}
")
  (setenv "TEXINPUTS" (amsref-test-path "recognition/elsewhere"))
  (amsref-test-open
   (amsref-test-write
    "recognition/master.tex"
    "\\documentclass{article}
\\usepackage{amsrefs}
\\begin{document}
\\section{Symmetry}
\\label{sec:symmetry}
Conservation laws follow from symmetry \\cite{noether1918}.
\\bibselect{mathbib, faraway, nosuchbase}
\\end{document}
"))
  (let ((amsrefs (list :docstruct (amsref-test-docstruct)
                       :bib-or-thebib (reftex-bib-or-thebib)
                       :bibfiles (reftex-get-bibfile-list)
                       :using-amsrefs-p
                       (and (save-excursion (amsreftex-using-amsrefs-p)) t))))
    ;; `reftex-reset-mode' clears the cached search path of every file type
    ;; RefTeX itself knows about, but amsreftex registers `reftex-ltb-path'
    ;; without teaching the reset about it, so a changed TEXINPUTS never
    ;; reaches a .ltb lookup in a running editor.
    (amsref-test-write
   "recognition/later/later.ltb"
   "\\begin{biblist}
\\bib{gauss1827}{article}{
  author={Gauss, Carl Friedrich},
  title={Disquisitiones generales circa superficies curvas},
  date={1827},
}
\\end{biblist}
")
    (setenv "TEXINPUTS" (amsref-test-path "recognition/later"))
    (reftex-reset-mode)
    (amsref-test-open
     (amsref-test-write
      "recognition/second.tex"
      "\\documentclass{article}
\\usepackage{amsrefs}
\\begin{document}
Curvature \\cite{gauss1827}.
\\bibselect{later}
\\end{document}
"))
    (list :amsrefs-document amsrefs
          :after-reset
          (list :tex-path-status (get 'reftex-tex-path 'status)
                :bib-path-status (get 'reftex-bib-path 'status)
                :ltb-path-status (get 'reftex-ltb-path 'status)
                :ltb-path (amsref-test-plain reftex-ltb-path)
                :bib (amsref-test-plain
                      (assq 'bib (symbol-value reftex-docstruct-symbol)))))))
"##,
        expect![[
            r#"OK (:amsrefs-document (:docstruct ((xr nil "\\\\\\\\\\\\") (index-tags) (is-multi nil) (bibview-cache) (master-dir . "[ORACLE-SANDBOX]/recognition/") (label-numbers) (bof "[ORACLE-SANDBOX]/recognition/master.tex") (toc "toc" "    1 Symmetry" "[ORACLE-SANDBOX]/recognition/master.tex" (:marker 63 "master.tex") 2 "1" "\\section{Symmetry}" 63) ("sec:symmetry" "s" "Conservation laws follow from symmetry \\cite{noether1918}. \\bibselect{mathbib, faraway, nosuchbase} " "[ORACLE-SANDBOX]/recognition/master.tex" nil) (database . "amsrefs") (bib "[ORACLE-SANDBOX]/recognition/mathbib.ltb" "[ORACLE-SANDBOX]/recognition/elsewhere/faraway.ltb") (eof "[ORACLE-SANDBOX]/recognition/master.tex")) :bib-or-thebib bib :bibfiles ("[ORACLE-SANDBOX]/recognition/mathbib.ltb" "[ORACLE-SANDBOX]/recognition/elsewhere/faraway.ltb") :using-amsrefs-p t) :after-reset (:tex-path-status nil :bib-path-status nil :ltb-path-status split :ltb-path ("[ORACLE-SANDBOX]/recognition/elsewhere/") :bib nil))"#
        ]],
    )
}

fn citing_from_an_amsrefs_database_offers_the_matches_and_inserts_the_selected_key()
-> ParityBatchCase {
    ParityBatchCase::value(
        "citing_from_an_amsrefs_database_offers_the_matches_and_inserts_the_selected_key",
        r##"
(progn
  (amsreftex-turn-on)
  (amsref-test-write
   "citing/mathbib.ltb"
   "\\begin{biblist}

\\bib{noether1918}{article}{
  author={Noether, Emmy},
  title={Invariante Variationsprobleme},
  journal={Nachr. Ges. Wiss. G\\\"ottingen},
  date={1918},
  pages={235\\ndash 257},
}

\\bib{atiyah1966}{article}{
  author={Atiyah, Michael},
  author={Segal, Graeme},
  title={The index of elliptic operators},
  journal={Topology},
  volume={4},
  date={1966-11},
  pages={531\\ndash 545},
}

\\end{biblist}
")
  (amsref-test-open
   (amsref-test-write
    "citing/master.tex"
    "\\documentclass{article}
\\usepackage{amsrefs}
\\begin{document}
Conservation laws follow from symmetry .
Index theory rests on elliptic operators .
\\bibselect{mathbib}
\\end{document}
"))
  (goto-char (point-min))
  (search-forward "from symmetry ")
  ;; Every capture happens while its own selection is still the recorded
  ;; one; `amsref-test-cite' clears the record each time it is called.
  (let* ((one-match
          (list :signalled (amsref-test-cite "Atiyah")
                :displays (length amsref-test-selections)
                :selection (amsref-test-selection 0 :text)
                :faces (amsref-test-selection 0 :faces)
                :offered-keys (mapcar #'car (amsref-test-selection 0 :entries))
                :buffer (buffer-substring-no-properties
                         (point-min) (point-max))
                :point (point))))
    (goto-char (point-min))
    (search-forward "elliptic operators ")
    (let ((every-entry
           (list :signalled (amsref-test-cite "=" ?n)
                 :selection (amsref-test-selection 0 :text)
                 :offered-keys
                 (mapcar #'car (amsref-test-selection 0 :entries)))))
      (list :citing-one-match one-match
            :citing-the-second-of-two every-entry
            :buffer (buffer-substring-no-properties (point-min) (point-max))
            :point (point)
            :cite-format reftex-cite-format
            :select-buffer-killed (null (get-buffer "*RefTeX Select*"))))))
"##,
        expect![[
            r#"OK (:citing-one-match (:signalled nil :displays 1 :selection "atiyah1966\n     Atiyah, Segal                  1966 Topology 4, 531-545\n     The index of elliptic operators\n\n" :faces ((font-lock-constant-face . "atiyah1966") (font-lock-keyword-face . "Atiyah, Segal                 ") (font-lock-comment-face . "1966") (font-lock-comment-face . "Topology 4, 531-545") (font-lock-function-name-face . "The index of elliptic operators")) :offered-keys ("atiyah1966") :buffer "\\documentclass{article}\n\\usepackage{amsrefs}\n\\begin{document}\nConservation laws follow from symmetry \\cite{atiyah1966}.\nIndex theory rests on elliptic operators .\n\\bibselect{mathbib}\n\\end{document}\n" :point 119) :citing-the-second-of-two (:signalled nil :selection "atiyah1966\n     Atiyah, Segal                  1966 Topology 4, 531-545\n     The index of elliptic operators\n\nnoether1918\n     Noether                        1918 Nachr. Ges. Wiss. G\\\"ottingen , 235-257\n     Invariante Variationsprobleme\n\n" :offered-keys ("atiyah1966" "noether1918")) :buffer "\\documentclass{article}\n\\usepackage{amsrefs}\n\\begin{document}\nConservation laws follow from symmetry \\cite{atiyah1966}.\nIndex theory rests on elliptic operators \\cite{noether1918}.\n\\bibselect{mathbib}\n\\end{document}\n" :point 180 :cite-format default :select-buffer-killed t)"#
        ]],
    )
}

fn the_selection_carries_every_amsrefs_field_translated_into_the_bibtex_names_reftex_reads()
-> ParityBatchCase {
    ParityBatchCase::value(
        "the_selection_carries_every_amsrefs_field_translated_into_the_bibtex_names_reftex_reads",
        r##"
(progn
  (amsreftex-turn-on)
  (amsref-test-write
   "fields/mathbib.ltb"
   "\\begin{biblist}

\\bib{noether1918}{article}{
  author={Noether, Emmy},
  title={Invariante Variationsprobleme},
  journal={Nachr. Ges. Wiss. G\\\"ottingen},
  date={1918},
  pages={235\\ndash 257},
}

\\bib{atiyah1966}{article}{
  author={Atiyah, Michael},
  author={Segal, Graeme},
  title={The index of elliptic operators},
  date={1966-11},
  pages={531\\ndash 545},
  book={
    title={Global Analysis},
    editor={Spencer, Donald},
    publisher={Univ. Tokyo Press},
    date={1966},
  },
}

\\bib{curie1903}{thesis}{
  author={Curie, Marie},
  title={Recherches sur les substances radioactives},
  type={Doctoral dissertation},
  organization={Universit\\'e de Paris},
  date={1903},
}

\\bib{devlin2011}{thesis}{
  author={Devlin, Sam},
  title={Kernel methods for a small corpus},
  type={M.Sc. thesis},
  organization={University of Elsewhere},
  date={2011},
}

\\bib{weil1948}{article}{
  author={Weil, Andr\\'e},
  title={Sur les courbes alg\\'ebriques},
  xref={proc1948},
  pages={7\\ndash 11},
}

\\bib{bourbaki}{book}{
  editor={Bourbaki, Nicolas},
  title={\\'El\\'ements de math\\'ematique},
}

\\bib*{proc1948}{collection}{
  title={Actes du Congr\\`es},
  editor={Cartan, Henri},
  publisher={Hermann},
  date={1948},
}

\\end{biblist}
")
  (amsref-test-open
   (amsref-test-write
    "fields/master.tex"
    "\\documentclass{article}
\\usepackage{amsrefs}
\\begin{document}
A survey .
\\bibselect{mathbib}
\\end{document}
"))
  (goto-char (point-min))
  (search-forward "A survey ")
  (let* ((signalled (amsref-test-cite "=" ?q))
         (selection (amsref-test-selection 0 :text))
         (entries (amsref-test-selection 0 :entries))
         (orders
          (mapcar
           (lambda (setting)
             (setq reftex-sort-bibtex-matches setting)
             (amsref-test-cite "=" ?q)
             (cons setting
                   (mapcar #'car (amsref-test-selection 0 :entries))))
           '(nil author year reverse-year))))
    (list :quitting-the-selection signalled
          :selection selection
          :entries entries
          :sort-orders orders
          :buffer-unchanged
          (buffer-substring-no-properties (point-min) (point-max)))))
"##,
        expect![[
            r#"OK (:quitting-the-selection (error "Quit") :selection "devlin2011\n     Devlin                         2011 Master: University of Elsewhere\n     Kernel methods for a small corpus\n\natiyah1966\n     Atiyah, Segal                  1966 in: Global Analysis\n     The index of elliptic operators\n\nnoether1918\n     Noether                        1918 Nachr. Ges. Wiss. G\\\"ottingen , 235-257\n     Invariante Variationsprobleme\n\ncurie1903\n     Curie                          1903 PhD: Universit\\'e de Paris\n     Recherches sur les substances radioactives\n\nbourbaki\n     Bourbaki                        book ()\n     \\'El\\'ements de math\\'ematique\n\nweil1948\n     Weil                             , 7-11\n     Sur les courbes alg\\'ebriques\n\n" :entries (("devlin2011" ("&formatted" . "devlin2011\n     Devlin                         2011 Master: University of Elsewhere\n     Kernel methods for a small corpus\n\n") ("&entry" . "\\bib{devlin2011}{thesis}{\n  author={Devlin, Sam},\n  title={Kernel methods for a small corpus},\n  type={M.Sc. thesis},\n  organization={University of Elsewhere},\n  date={2011},\n}") ("school" . "University of Elsewhere") ("&type" . "mastersthesis") ("&key" . "devlin2011") ("title" . "Kernel methods for a small corpus") ("type" . "M.Sc. thesis") ("organization" . "University of Elsewhere") ("year" . "2011") ("author" . "Devlin, Sam")) ("atiyah1966" ("&formatted" . "atiyah1966\n     Atiyah, Segal                  1966 in: Global Analysis\n     The index of elliptic operators\n\n") ("&entry" . "\\bib{atiyah1966}{article}{\n  author={Atiyah, Michael},\n  author={Segal, Graeme},\n  title={The index of elliptic operators},\n  date={1966-11},\n  pages={531\\ndash 545},\n  book={\n    title={Global Analysis},\n    editor={Spencer, Donald},\n    publisher={Univ. Tokyo Press},\n    date={1966},\n  },\n}") ("&type" . "incollection") ("&key" . "atiyah1966") ("title" . "The index of elliptic operators") ("year" . "1966") ("month" . "11") ("pages" . "531-545") ("booktitle" . "Global Analysis") ("author" . "Atiyah, Michael and Segal, Graeme")) ("noether1918" ("&formatted" . "noether1918\n     Noether                        1918 Nachr. Ges. Wiss. G\\\"ottingen , 235-257\n     Invariante Variationsprobleme\n\n") ("&entry" . "\\bib{noether1918}{article}{\n  author={Noether, Emmy},\n  title={Invariante Variationsprobleme},\n  journal={Nachr. Ges. Wiss. G\\\"ottingen},\n  date={1918},\n  pages={235\\ndash 257},\n}") ("&type" . "article") ("&key" . "noether1918") ("title" . "Invariante Variationsprobleme") ("journal" . "Nachr. Ges. Wiss. G\\\"ottingen") ("year" . "1918") ("pages" . "235-257") ("author" . "Noether, Emmy")) ("curie1903" ("&formatted" . "curie1903\n     Curie                          1903 PhD: Universit\\'e de Paris\n     Recherches sur les substances radioactives\n\n") ("&entry" . "\\bib{curie1903}{thesis}{\n  author={Curie, Marie},\n  title={Recherches sur les substances radioactives},\n  type={Doctoral dissertation},\n  organization={Universit\\'e de Paris},\n  date={1903},\n}") ("school" . "Universit\\'e de Paris") ("&type" . "phdthesis") ("&key" . "curie1903") ("title" . "Recherches sur les substances radioactives") ("type" . "Doctoral dissertation") ("organization" . "Universit\\'e de Paris") ("year" . "1903") ("author" . "Curie, Marie")) ("bourbaki" ("&formatted" . "bourbaki\n     Bourbaki                        book ()\n     \\'El\\'ements de math\\'ematique\n\n") ("&entry" . "\\bib{bourbaki}{book}{\n  editor={Bourbaki, Nicolas},\n  title={\\'El\\'ements de math\\'ematique},\n}") ("&type" . "book") ("&key" . "bourbaki") ("title" . "\\'El\\'ements de math\\'ematique") ("editor" . "Bourbaki, Nicolas")) ("weil1948" ("&formatted" . "weil1948\n     Weil                             , 7-11\n     Sur les courbes alg\\'ebriques\n\n") ("&entry" . "\\bib{weil1948}{article}{\n  author={Weil, Andr\\'e},\n  title={Sur les courbes alg\\'ebriques},\n  xref={proc1948},\n  pages={7\\ndash 11},\n}") ("&type" . "article") ("&key" . "weil1948") ("title" . "Sur les courbes alg\\'ebriques") ("xref" . "proc1948") ("pages" . "7-11") ("author" . "Weil, Andr\\'e") ("booktitle" . "Actes du Congr\\`es") ("book-publisher" . "Hermann") ("book-year" . "1948") ("book-editor" . "Cartan, Henri"))) :sort-orders ((nil "bourbaki" "weil1948" "devlin2011" "curie1903" "atiyah1966" "noether1918") (author "atiyah1966" "bourbaki" "curie1903" "devlin2011" "noether1918" "weil1948") (year "bourbaki" "weil1948" "curie1903" "noether1918" "atiyah1966" "devlin2011") (reverse-year "devlin2011" "atiyah1966" "noether1918" "curie1903" "bourbaki" "weil1948")) :buffer-unchanged "\\documentclass{article}\n\\usepackage{amsrefs}\n\\begin{document}\nA survey .\n\\bibselect{mathbib}\n\\end{document}\n")"#
        ]],
    )
}

fn viewing_the_crossref_of_a_citation_pops_to_the_amsrefs_record_and_echoes_it() -> ParityBatchCase
{
    ParityBatchCase::value(
        "viewing_the_crossref_of_a_citation_pops_to_the_amsrefs_record_and_echoes_it",
        r##"
(progn
  (amsreftex-turn-on)
  (amsref-test-write
   "crossref/mathbib.ltb"
   "\\begin{biblist}

\\bib{noether1918}{article}{
  author={Noether, Emmy},
  title={Invariante Variationsprobleme},
  journal={Nachr. Ges. Wiss. G\\\"ottingen},
  date={1918},
  pages={235\\ndash 257},
}

\\bib{atiyah1966}{article}{
  author={Atiyah, Michael},
  author={Segal, Graeme},
  title={The index of elliptic operators},
  journal={Topology},
  volume={4},
  date={1966-11},
  pages={531\\ndash 545},
}

\\end{biblist}
")
  (amsref-test-open
   (amsref-test-write
    "crossref/master.tex"
    "\\documentclass{article}
\\usepackage{amsrefs}
\\begin{document}
Conservation laws \\cite{atiyah1966} and \\cite{nosuchkey}.
\\bibselect{mathbib}
\\end{document}
"))
  (goto-char (point-min))
  (search-forward "atiyah")
  (execute-kbd-macro (kbd "C-c &"))
  (let ((popped
         (list :windows (mapcar (lambda (window)
                                  (buffer-name (window-buffer window)))
                                (window-list))
               :selected-buffer (buffer-name)
               :point (point)
               :database
               (with-current-buffer (get-buffer "mathbib.ltb")
                 (list :point (point)
                       :line (buffer-substring-no-properties
                              (line-beginning-position) (line-end-position))
                       :overlays (amsref-test-overlays))))))
    ;; The automatic display path: `reftex-echo-cite' summarises the record
    ;; in the echo area and caches the summary in the document structure.
    (reftex-view-crossref nil 'echo)
    (let ((echoed (amsref-test-plain
                   (assq 'bibview-cache
                         (symbol-value reftex-docstruct-symbol)))))
      (goto-char (point-min))
      (search-forward "nosuchkey")
      (backward-char 3)
      (list :popped-to-record popped
            :echo-cache echoed
            :missing-key
            (condition-case failure (reftex-view-crossref)
              (error (amsref-test-plain failure)))
            :document-unchanged
            (buffer-substring-no-properties (point-min) (point-max))))))
"##,
        expect![[
            r#"OK (:popped-to-record (:windows ("master.tex" "mathbib.ltb") :selected-buffer "master.tex" :point 93 :database (:point 199 :line "\\bib{atiyah1966}{article}{" :overlays ((199 225 highlight)))) :echo-cache (bibview-cache ("atiyah1966" . "Atiyah & Segal 1966, index elliptic operators, Topology 4:531")) :missing-key (error "No amsrefs entry with citation key nosuchkey") :document-unchanged "\\documentclass{article}\n\\usepackage{amsrefs}\n\\begin{document}\nConservation laws \\cite{atiyah1966} and \\cite{nosuchkey}.\n\\bibselect{mathbib}\n\\end{document}\n")"#
        ]],
    )
    .fresh_process()
}

fn a_bibtex_document_still_gets_vanilla_reftex_behaviour_while_amsreftex_is_on() -> ParityBatchCase
{
    ParityBatchCase::value(
        "a_bibtex_document_still_gets_vanilla_reftex_behaviour_while_amsreftex_is_on",
        r##"
(progn
  (amsreftex-turn-on)
  (amsref-test-write
   "bibtex/refs.bib"
   "@article{noether1918,
  author = {Noether, Emmy},
  title = {Invariante Variationsprobleme},
  journal = {Nachr. Ges. Wiss. G\\\"ottingen},
  year = {1918},
  pages = {235--257}
}

@book{bourbaki1970,
  author = {Bourbaki, Nicolas},
  title = {Th\\'eorie des ensembles},
  publisher = {Hermann},
  year = {1970}
}
")
  (amsref-test-open
   (amsref-test-write
    "bibtex/master.tex"
    "\\documentclass{article}
\\begin{document}
\\section{Classical}
Symmetry .
\\bibliography{refs}
\\end{document}
"))
  (goto-char (point-min))
  (search-forward "Symmetry ")
  (let ((signalled (amsref-test-cite "Noether")))
    (list :amsreftex-p amsreftex-p
          :database-cell (assq 'database (symbol-value reftex-docstruct-symbol))
          :bib-or-thebib (reftex-bib-or-thebib)
          :bibfiles (reftex-get-bibfile-list)
          :using-amsrefs-p (save-excursion (amsreftex-using-amsrefs-p))
          :signalled signalled
          :selection (amsref-test-selection 0 :text)
          ;; The BibTeX parser leaves `235--257' alone; amsreftex's rewrites
          ;; `\ndash ' to a hyphen, so the field proves which one ran.
          :entry (car (amsref-test-selection 0 :entries))
          :buffer (buffer-substring-no-properties (point-min) (point-max)))))
"##,
        expect![[
            r#"OK (:amsreftex-p t :database-cell nil :bib-or-thebib bib :bibfiles ("[ORACLE-SANDBOX]/bibtex/refs.bib") :using-amsrefs-p nil :signalled nil :selection "noether1918\n     Noether                        1918 Nachr. Ges. Wiss. G\\\"ottingen , 235--257\n     Invariante Variationsprobleme\n\n" :entry ("noether1918" ("&formatted" . "noether1918\n     Noether                        1918 Nachr. Ges. Wiss. G\\\"ottingen , 235--257\n     Invariante Variationsprobleme\n\n") ("&entry" . "@article{noether1918,\n  author = {Noether, Emmy},\n  title = {Invariante Variationsprobleme},\n  journal = {Nachr. Ges. Wiss. G\\\"ottingen},\n  year = {1918},\n  pages = {235--257}\n}") ("pages" . "235--257") ("year" . "1918") ("journal" . "Nachr. Ges. Wiss. G\\\"ottingen") ("title" . "Invariante Variationsprobleme") ("author" . "Noether, Emmy") ("&type" . "article") ("&key" . "noether1918")) :buffer "\\documentclass{article}\n\\begin{document}\n\\section{Classical}\nSymmetry \\cite{noether1918}.\n\\bibliography{refs}\n\\end{document}\n")"#
        ]],
    )
}

fn sorting_a_bibliography_orders_the_records_by_the_configured_fields_and_name_parts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "sorting_a_bibliography_orders_the_records_by_the_configured_fields_and_name_parts",
        r##"
(progn
  (amsreftex-turn-on)
  (let ((database
         "\\begin{biblist}

\\bib{segal1968}{article}{
  author={Segal, Graeme},
  title={Classifying spaces},
  date={1968},
}

\\bib{atiyah1961}{article}{
  author={Atiyah, Michael},
  title={Zeta functions},
  date={1961},
}

\\bib{atiyah1966}{article}{
  author={Atiyah, Michael},
  title={Equivariant K-theory},
  date={1966},
}

\\bib{atiyahsegal1969}{article}{
  author={Atiyah, Michael},
  author={Segal, Graeme},
  title={Exponential isomorphisms},
  date={1969},
}

\\bib{orsted1990}{article}{
  author={\\O rsted, Bent},
  title={A note on spherical functions},
  date={1990},
}

\\end{biblist}
")
        (sorted
         (lambda (name)
           (find-file (amsref-test-path name))
           (goto-char (point-min))
           (search-forward "Classifying")
           (amsreftex-sort-bibliography)
           (list :mode major-mode
                 :text (buffer-substring-no-properties
                        (point-min) (point-max))))))
    (amsref-test-write "sorting/by-author-year.ltb" database)
    (amsref-test-write "sorting/by-author-title.ltb" database)
    (amsref-test-write "sorting/by-first-name.ltb" database)
    (amsref-test-write
     "sorting/mixed.tex"
     (concat "\\documentclass{article}
\\usepackage{amsrefs}
\\begin{document}

\\bib{outside1900}{article}{
  author={Zeta, Zoe},
  title={Outside the biblist},
  date={1900},
}

" database "
\\end{document}
"))
    (list
     :sort-fields amsreftex-sort-fields
     :sort-name-parts amsreftex-sort-name-parts
     :by-author-then-year (funcall sorted "sorting/by-author-year.ltb")
     :by-author-then-title
     (let ((amsreftex-sort-fields '("author" "title")))
       (funcall sorted "sorting/by-author-title.ltb"))
     :by-first-then-last-name
     (let ((amsreftex-sort-name-parts '(first last)))
       (funcall sorted "sorting/by-first-name.ltb"))
     :only-the-biblist-is-sorted
     (progn
       (find-file (amsref-test-path "sorting/mixed.tex"))
       (goto-char (point-min))
       (search-forward "Classifying")
       (amsreftex-sort-bibliography)
       (buffer-substring-no-properties (point-min) (point-max))))))
"##,
        expect![[
            r#"OK (:sort-fields ("author" "year") :sort-name-parts (last initial) :by-author-then-year (:mode latex-mode :text "\\begin{biblist}\n\n\\bib{atiyah1961}{article}{\n  author={Atiyah, Michael},\n  title={Zeta functions},\n  date={1961},\n}\n\n\\bib{atiyah1966}{article}{\n  author={Atiyah, Michael},\n  title={Equivariant K-theory},\n  date={1966},\n}\n\n\\bib{atiyahsegal1969}{article}{\n  author={Atiyah, Michael},\n  author={Segal, Graeme},\n  title={Exponential isomorphisms},\n  date={1969},\n}\n\n\\bib{orsted1990}{article}{\n  author={\\O rsted, Bent},\n  title={A note on spherical functions},\n  date={1990},\n}\n\n\\bib{segal1968}{article}{\n  author={Segal, Graeme},\n  title={Classifying spaces},\n  date={1968},\n}\n\n\\end{biblist}\n") :by-author-then-title (:mode latex-mode :text "\\begin{biblist}\n\n\\bib{atiyah1966}{article}{\n  author={Atiyah, Michael},\n  title={Equivariant K-theory},\n  date={1966},\n}\n\n\\bib{atiyah1961}{article}{\n  author={Atiyah, Michael},\n  title={Zeta functions},\n  date={1961},\n}\n\n\\bib{atiyahsegal1969}{article}{\n  author={Atiyah, Michael},\n  author={Segal, Graeme},\n  title={Exponential isomorphisms},\n  date={1969},\n}\n\n\\bib{orsted1990}{article}{\n  author={\\O rsted, Bent},\n  title={A note on spherical functions},\n  date={1990},\n}\n\n\\bib{segal1968}{article}{\n  author={Segal, Graeme},\n  title={Classifying spaces},\n  date={1968},\n}\n\n\\end{biblist}\n") :by-first-then-last-name (:mode latex-mode :text "\\begin{biblist}\n\n\\bib{orsted1990}{article}{\n  author={\\O rsted, Bent},\n  title={A note on spherical functions},\n  date={1990},\n}\n\n\\bib{segal1968}{article}{\n  author={Segal, Graeme},\n  title={Classifying spaces},\n  date={1968},\n}\n\n\\bib{atiyah1961}{article}{\n  author={Atiyah, Michael},\n  title={Zeta functions},\n  date={1961},\n}\n\n\\bib{atiyah1966}{article}{\n  author={Atiyah, Michael},\n  title={Equivariant K-theory},\n  date={1966},\n}\n\n\\bib{atiyahsegal1969}{article}{\n  author={Atiyah, Michael},\n  author={Segal, Graeme},\n  title={Exponential isomorphisms},\n  date={1969},\n}\n\n\\end{biblist}\n") :only-the-biblist-is-sorted "\\documentclass{article}\n\\usepackage{amsrefs}\n\\begin{document}\n\n\\bib{outside1900}{article}{\n  author={Zeta, Zoe},\n  title={Outside the biblist},\n  date={1900},\n}\n\n\\begin{biblist}\n\n\\bib{atiyah1961}{article}{\n  author={Atiyah, Michael},\n  title={Zeta functions},\n  date={1961},\n}\n\n\\bib{atiyah1966}{article}{\n  author={Atiyah, Michael},\n  title={Equivariant K-theory},\n  date={1966},\n}\n\n\\bib{atiyahsegal1969}{article}{\n  author={Atiyah, Michael},\n  author={Segal, Graeme},\n  title={Exponential isomorphisms},\n  date={1969},\n}\n\n\\bib{orsted1990}{article}{\n  author={\\O rsted, Bent},\n  title={A note on spherical functions},\n  date={1990},\n}\n\n\\bib{segal1968}{article}{\n  author={Segal, Graeme},\n  title={Classifying spaces},\n  date={1968},\n}\n\n\\end{biblist}\n\n\\end{document}\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        turning_amsreftex_on_takes_over_reftexs_citation_machinery_and_off_leaves_one_override(),
        an_amsrefs_document_is_recognised_and_each_named_database_is_located_or_dropped(),
        citing_from_an_amsrefs_database_offers_the_matches_and_inserts_the_selected_key(),
        the_selection_carries_every_amsrefs_field_translated_into_the_bibtex_names_reftex_reads(),
        viewing_the_crossref_of_a_citation_pops_to_the_amsrefs_record_and_echoes_it(),
        a_bibtex_document_still_gets_vanilla_reftex_behaviour_while_amsreftex_is_on(),
        sorting_a_bibliography_orders_the_records_by_the_configured_fields_and_name_parts(),
    ]
}
