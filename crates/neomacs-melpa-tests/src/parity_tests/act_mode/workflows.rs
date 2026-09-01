use expect_test::expect;

use super::ParityBatchCase;

/// Opening an ACT design is the entire installation story: the package's only
/// side effect on load is its `auto-mode-alist' entry.  Visiting six real file
/// names pins which ones the entry claims -- including the case-folded and
/// backup-suffixed forms Emacs retries, and the near misses it must not claim
/// -- and the resulting buffer pins the mode's identity: derived from
/// `prog-mode', named "act", with the package's font-lock rules installed
/// buffer locally and the generated keymap and syntax table in place.
fn visiting_an_act_file_selects_the_mode_and_its_prog_mode_inheritance() -> ParityBatchCase {
    ParityBatchCase::value(
        "visiting_an_act_file_selects_the_mode_and_its_prog_mode_inheritance",
        r##"(progn
  (let ((observed nil))
    (dolist (name '("design.act" "design.ACT" "design.act.bak" "design.act~"
                    "design.acta" "act"))
      (let ((buffer (actm-test-visit name "export defproc demo () { }\n")))
        (push (list name major-mode) observed)
        (kill-buffer buffer)))
    (actm-test-visit "design.act" actm-test-design)
    (list :routing (nreverse observed)
          :alist (assoc "\\.act\\'" auto-mode-alist)
          :mode major-mode
          :mode-name mode-name
          :parent (get 'act-mode 'derived-mode-parent)
          :derived (and (derived-mode-p 'prog-mode) t)
          :font-lock-defaults font-lock-defaults
          :buffer-local (local-variable-p 'font-lock-defaults)
          :keymap-parent (eq (keymap-parent act-mode-map) prog-mode-map)
          :syntax-table (eq (syntax-table) act-mode-syntax-table))))"##,
        expect![[
            r#"OK (:routing (("design.act" act-mode) ("design.ACT" act-mode) ("design.act.bak" act-mode) ("design.act~" act-mode) ("design.acta" fundamental-mode) ("act" fundamental-mode)) :alist ("\\.act\\'" . act-mode) :mode act-mode :mode-name "act" :parent prog-mode :derived t :font-lock-defaults ((act-fontlock)) :buffer-local t :keymap-parent t :syntax-table t)"#
        ]],
    )
}

fn syntax_highlighting_covers_every_category_of_the_language() -> ParityBatchCase {
    ParityBatchCase::value(
        "syntax_highlighting_covers_every_category_of_the_language",
        r##"(progn
  (actm-test-visit "design.act" actm-test-design)
  (list :runs (actm-test-face-runs)
        :point (point)
        :modified (buffer-modified-p)))"##,
        expect![[
            r#"OK (:runs (("// a two-stage buffer, from the ACT tutorial" . font-lock-comment-face) ("\n") ("import" . font-lock-keyword-face) (" ") ("\"globals.act\"" . font-lock-string-face) (";\n") ("export" . font-lock-keyword-face) (" ") ("defproc" . font-lock-function-name-face) (" buffer (") ("bool" . font-lock-type-face) ("? in; ") ("bool" . font-lock-type-face) ("! out) {\n  ") ("bool" . font-lock-type-face) (" _x;\n  ") ("prs" . font-lock-function-name-face) (" {\n    in => _x-\n    _x => out-\n  }\n}\n") ("deftype" . font-lock-function-name-face) (" ") ("e1of" . font-lock-type-face) ("<3>" . font-lock-constant-face) (" onehot;\n") ("defchan" . font-lock-function-name-face) (" handshake (") ("int" . font-lock-type-face) (" width) { ") ("pint" . font-lock-type-face) (" w = width; }\n")) :point 1 :modified nil)"#
        ]],
    )
    .fresh_process()
}

fn comments_case_and_word_boundaries_keep_keywords_from_leaking() -> ParityBatchCase {
    ParityBatchCase::value(
        "comments_case_and_word_boundaries_keep_keywords_from_leaking",
        r##"(progn
  (actm-test-visit "boundaries.act"
    (concat "// import int defproc inside a comment\n"
            "exported printing myint pint2 int_t \"int\" INT Int\n"
            "int x; e1of<12> y; a<b> z; <3>\n"))
  (list :runs (actm-test-face-runs)
        :faces (actm-test-faces-of "exported" "printing" "myint" "pint2"
                                   "int_t" "INT" "Int")))"##,
        expect![[
            r#"OK (:runs (("// import int defproc inside a comment" . font-lock-comment-face) ("\nexported printing myint pint2 ") ("int" . font-lock-type-face) ("_t ") ("\"int\"" . font-lock-string-face) (" INT Int\n") ("int" . font-lock-type-face) (" x; ") ("e1of" . font-lock-type-face) ("<12>" . font-lock-constant-face) (" y; a<b> z; ") ("<3>" . font-lock-constant-face) ("\n")) :faces (("exported" nil) ("printing" nil) ("myint" nil) ("pint2" nil) ("int_t" font-lock-type-face) ("INT" nil) ("Int" nil)))"#
        ]],
    )
}

fn the_mode_paints_comments_without_declaring_any_comment_syntax() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_mode_paints_comments_without_declaring_any_comment_syntax",
        r##"(progn
  (actm-test-visit "syntax.act" actm-test-design)
  (font-lock-ensure)
  (list :comment-start comment-start
        :comment-end comment-end
        :slash-syntax (string (char-syntax ?/))
        :quote-syntax (string (char-syntax ?\"))
        :in-comment (save-excursion
                      (goto-char (point-min))
                      (search-forward "two-stage")
                      (list (nth 4 (syntax-ppss))
                            (get-text-property (1- (point)) 'face)))
        :in-string (save-excursion
                     (goto-char (point-min))
                     (search-forward "globals")
                     (list (nth 3 (syntax-ppss))
                           (get-text-property (1- (point)) 'face)))
        :parse-sexp-ignore-comments parse-sexp-ignore-comments))"##,
        expect![[
            r#"OK (:comment-start nil :comment-end "" :slash-syntax "_" :quote-syntax "\"" :in-comment (nil font-lock-comment-face) :in-string (34 font-lock-string-face) :parse-sexp-ignore-comments t)"#
        ]],
    )
}

fn tab_falls_back_to_prog_modes_relative_indentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "tab_falls_back_to_prog_modes_relative_indentation",
        r##"(progn
  (actm-test-visit "indent.act"
    "defproc buffer (bool? in) {\nbool _x;\nprs {\nin => _x-\n}\n}\n")
  (let ((before (buffer-substring-no-properties (point-min) (point-max)))
        after-region)
    (indent-region (point-min) (point-max))
    (setq after-region (buffer-substring-no-properties (point-min) (point-max)))
    (goto-char (point-min))
    (forward-line 1)
    (indent-for-tab-command)
    (list :indent-line-function indent-line-function
          :tab-width tab-width
          :indent-region-changed (not (equal before after-region))
          :column (current-column)
          :line (buffer-substring-no-properties (line-beginning-position)
                                                (line-end-position))
          :text (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect![[
            r#"OK (:indent-line-function indent-relative :tab-width 8 :indent-region-changed nil :column 8 :line "\11bool _x;" :text "defproc buffer (bool? in) {\n\11bool _x;\nprs {\nin => _x-\n}\n}\n")"#
        ]],
    )
}

fn appending_a_declaration_highlights_the_new_text_too() -> ParityBatchCase {
    ParityBatchCase::value(
        "appending_a_declaration_highlights_the_new_text_too",
        r##"(progn
  (actm-test-visit "edit.act" "export defproc demo () { }\n")
  (let ((before (actm-test-face-runs)))
    (goto-char (point-max))
    (insert "deftype e2of<4> dual;\n")
    (font-lock-flush)
    (list :before before
          :after (actm-test-face-runs)
          :modified (buffer-modified-p)
          :point (point))))"##,
        expect![[
            r#"OK (:before (("export" . font-lock-keyword-face) (" ") ("defproc" . font-lock-function-name-face) (" demo () { }\n")) :after (("export" . font-lock-keyword-face) (" ") ("defproc" . font-lock-function-name-face) (" demo () { }\n") ("deftype" . font-lock-function-name-face) (" ") ("e2of" . font-lock-type-face) ("<4>" . font-lock-constant-face) (" dual;\n")) :modified t :point 50)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        visiting_an_act_file_selects_the_mode_and_its_prog_mode_inheritance(),
        syntax_highlighting_covers_every_category_of_the_language(),
        comments_case_and_word_boundaries_keep_keywords_from_leaking(),
        the_mode_paints_comments_without_declaring_any_comment_syntax(),
        tab_falls_back_to_prog_modes_relative_indentation(),
        appending_a_declaration_highlights_the_new_text_too(),
    ]
}
