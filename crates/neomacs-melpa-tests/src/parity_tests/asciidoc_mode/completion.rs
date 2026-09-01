use expect_test::expect;

use super::ParityBatchCase;

fn context_completion_returns_exact_bounds_candidates_and_exclusivity_for_real_inputs()
-> ParityBatchCase {
    ParityBatchCase::value(
        "context_completion_returns_exact_bounds_candidates_and_exclusivity_for_real_inputs",
        r##"(cl-labels
    ((probe
      (content)
      (with-temp-buffer
        (insert content)
        (goto-char (point-max))
        (let ((capf (asciidoc--capf)))
          (if (null capf)
              nil
            (let* ((beg (nth 0 capf))
                   (end (nth 1 capf))
                   (table (nth 2 capf))
                   (fragment
                    (buffer-substring-no-properties
                     beg end)))
              (list
               beg end fragment
               (plist-get (nthcdr 3 capf)
                          :exclusive)
               (sort
                (all-completions
                 fragment table)
                #'string<))))))))
  (list
   (probe
    "== My Section\n\n[[explicit]] x\n\nSee <<ex")
   (probe
    "[[explicit]] x\n\nSee xref:ex")
   (probe
    ":custom-attr: value\n\nUse {cu")
   (probe "[source,ru")
   (probe "See <<id,the te")
   (probe "See <<id>> and ")
   (probe "ordinary prose")))"##,
        expect![[
            r#"OK ((38 40 "ex" no ("explicit")) (26 28 "ex" no ("explicit")) (27 29 "cu" no ("custom-attr")) (9 11 "ru" no ("ruby" "rust")) nil nil nil)"#
        ]],
    )
}

fn include_completion_preserves_gnu_behavior_with_a_real_document_and_filesystem_entries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "include_completion_preserves_gnu_behavior_with_a_real_document_and_filesystem_entries",
        r##"(let* ((root
         (expand-file-name
          "asciidoc-mode-completion-contract"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (main
        (expand-file-name "main.adoc" root)))
  (when (file-directory-p root)
    (delete-directory root t))
  (make-directory
   (expand-file-name "guides" root) t)
  (with-temp-file
      (expand-file-name "guide.adoc" root)
    (insert "= Guide\n"))
  (with-temp-file
      (expand-file-name
       "guides/install.adoc" root)
    (insert "= Install\n"))
  (unwind-protect
      (with-temp-buffer
        (setq buffer-file-name main)
        (insert "include::gu")
        (goto-char (point-max))
        (let* ((capf (asciidoc--capf))
               (beg (nth 0 capf))
               (end (nth 1 capf))
               (table (nth 2 capf))
               (fragment
                (buffer-substring-no-properties
                 beg end))
               (candidates
                (sort
                 (all-completions
                  fragment table)
                 #'string<)))
          (list
           fragment
           (mapcar
            (lambda (candidate)
              (file-relative-name
               candidate root))
            candidates)
           (try-completion fragment table)
           (plist-get (nthcdr 3 capf)
                      :exclusive))))
    (delete-directory root t)))"##,
        expect![[r#"OK ("gu" nil nil no)"#]],
    )
}

fn attribute_and_source_language_collections_merge_buffer_local_and_builtin_values_stably()
-> ParityBatchCase {
    ParityBatchCase::value(
        "attribute_and_source_language_collections_merge_buffer_local_and_builtin_values_stably",
        r##"(with-temp-buffer
  (insert
   ":custom-one: first\n"
   ":custom-two: second\n"
   ":custom-one: override\n"
   ":toc: local\n")
  (let ((asciidoc-code-lang-modes
         '(("Practical" . text-mode)
           ("ruby" . ruby-mode)
           ("bash" . sh-mode))))
    (let ((attributes
           (asciidoc--attribute-names))
          (languages
           (asciidoc--source-languages)))
      (list
       (seq-take attributes 8)
       (length attributes)
       (mapcar
        (lambda (name)
          (cons name
                (member name attributes)))
        '("custom-one" "custom-two"
          "toc" "doctitle" "cpp"
          "two-semicolons"))
       (seq-take languages 10)
       (length languages)
       (mapcar
        (lambda (name)
          (cons name
                (member name languages)))
        '("Practical" "ruby" "bash"
          "rust" "emacs-lisp"
          "asciidoc"))))))"##,
        expect![[
            r#"OK (("custom-one" "custom-two" "toc" "doctitle" "author" "authorinitials" "firstname" "lastname") 58 (("custom-one" "custom-one" . #1=("custom-two" . #2=("toc" . #3=("doctitle" "author" "authorinitials" "firstname" "lastname" "email" "revnumber" "revdate" "revremark" "version" "doctype" "backend" "sectnums" "sectanchors" "toclevels" "icons" "imagesdir" "source-highlighter" "experimental" "idprefix" "idseparator" "nofooter" "stem" "tabsize" "leveloffset" "sp" "nbsp" "zwsp" "wj" "apos" "quot" "lsquo" "rsquo" "ldquo" "rdquo" "deg" "plus" "brvbar" "vbar" "amp" "lt" "gt" "startsb" "endsb" "caret" "asterisk" "tilde" "backslash" "backtick" "two-colons" . #5=("two-semicolons" . #4=("cpp" "pp" "blank" "empty")))))) ("custom-two" . #1#) ("toc" . #2#) ("doctitle" . #3#) ("cpp" . #4#) ("two-semicolons" . #5#)) ("Practical" "ruby" "bash" "asciidoc" "c" "clojure" "cpp" "csharp" "css" "diff") 41 (("Practical" "Practical" . #6=("ruby" . #7=("bash" . #10=("asciidoc" "c" "clojure" "cpp" "csharp" "css" "diff" "dockerfile" "elixir" . #9=("emacs-lisp" "erlang" "go" "groovy" "haskell" "html" "java" "javascript" "json" "kotlin" "lisp" "lua" "make" "markdown" "ocaml" "perl" "php" "python" . #8=("rust" "scala" "scheme" "sh" "shell" "sql" "swift" "toml" "typescript" "xml" "yaml")))))) ("ruby" . #6#) ("bash" . #7#) ("rust" . #8#) ("emacs-lisp" . #9#) ("asciidoc" . #10#)))"#
        ]],
    )
    .fresh_process()
}

fn flyspell_predicate_checks_prose_but_skips_links_references_anchors_macros_and_code()
-> ParityBatchCase {
    ParityBatchCase::value(
        "flyspell_predicate_checks_prose_but_skips_links_references_anchors_macros_and_code",
        r##"(with-temp-buffer
  (insert
   "Ordinary proseword remains checkable.\n"
   "Visit https://zzqqzz.example/path.\n"
   "See <<some-section>> and [[anchor-id]].\n"
   "Run `somefn` then kbd:[C-c C-c].\n")
  (asciidoc-mode)
  (font-lock-ensure)
  (prin1-to-string
   (mapcar
    (lambda (needle)
      (goto-char (point-min))
      (search-forward needle)
      (cons needle
            (asciidoc--flyspell-verify)))
    '("proseword"
      "zzqqzz"
      "some-section"
      "anchor-id"
      "somefn"
      "C-c"))))"##,
        expect![[
            r#"OK "((\"proseword\" . t) (\"zzqqzz\") (\"some-section\") (\"anchor-id\") (\"somefn\") (\"C-c\"))""#
        ]],
    )
}

pub(super) fn completion_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        context_completion_returns_exact_bounds_candidates_and_exclusivity_for_real_inputs(),
        include_completion_preserves_gnu_behavior_with_a_real_document_and_filesystem_entries(),
        attribute_and_source_language_collections_merge_buffer_local_and_builtin_values_stably(),
        flyspell_predicate_checks_prose_but_skips_links_references_anchors_macros_and_code(),
    ]
}
