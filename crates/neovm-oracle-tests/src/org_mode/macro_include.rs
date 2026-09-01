use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_include_keyword_expands_file_content_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK \"#+TITLE: Main\\n#+MACRO: incmacro Included $1\\n* Included\\nBody {{{incmacro(value)}}}\\n* Local\\nBody\\n\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let* ((root (make-temp-file "org-include" t))
         (inc (expand-file-name "inc.org" root)))
    (unwind-protect
        (progn
          (with-temp-file inc
            (insert "#+MACRO: incmacro Included $1\n")
            (insert "* Included\n")
            (insert "Body {{{incmacro(value)}}}\n"))
          (with-temp-buffer
            (org-mode)
            (insert "#+TITLE: Main\n")
            (insert "#+INCLUDE: \"" inc "\"\n")
            (insert "* Local\nBody\n")
            (goto-char (point-min))
            (org-export-expand-include-keyword nil root nil nil nil)
            (buffer-substring-no-properties (point-min) (point-max))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_macro_escape_extract_replace_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"x\\\\,y,z\" (\"x,y\" \"z\") nil \"#+MACRO: count (eval (number-to-string (1+ (string-to-number $1))))\\n#+MACRO: wrap [$1|$2]\\nValue (eval (number-to-string (1+ (string-to-number 4)))); [a|b]; escaped [x,y|z].\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-macro)
  (with-temp-buffer
    (org-mode)
    (insert "#+MACRO: count (eval (number-to-string (1+ (string-to-number $1))))\n")
    (insert "#+MACRO: wrap [$1|$2]\n")
    (insert "Value {{{count(4)}}}; {{{wrap(a,b)}}}; escaped {{{wrap(x\\,y,z)}}}.\n")
    (let ((templates (org-macro--collect-macros)))
      (list (org-macro-escape-arguments "x,y" "z")
            (org-macro-extract-arguments "x\\,y,z")
            (org-macro-expand "wrap(a,b)" templates)
            (progn
              (org-macro-replace-all templates)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_macro_html_export_markup_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t \"<div id=\\\"outline-container-org-id\\\" class=\\\"outline-2\\\">\\n<h2 id=\\\"org-id\\\"><span class=\\\"section-number-2\\\">1.</span> H</h2>\\n<div class=\\\"outline-text-2\\\" id=\\\"text-1\\\">\\n<p>\\n<i>text</i>\\n</p>\\n</div>\\n</div>\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: X\n")
    (insert "#+MACRO: emph /$1/\n")
    (insert "* H\n{{{emph(text)}}}\n")
    (let* ((org-export-with-toc nil)
           (html (org-export-as 'html nil nil t nil)))
      (list (not (null (string-match-p "<i>text</i>" html)))
            (replace-regexp-in-string
             "org[[:alnum:]]+"
             "org-id"
             html)))))"##,
        expect,
    );
}

#[test]
fn org_include_location_only_contents_footnotes_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 33 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let* ((root (make-temp-file "org-include-location" t))
         (inc (expand-file-name "chapters.org" root)))
    (unwind-protect
        (progn
          (with-temp-file inc
            (insert "#+TITLE: Included\n")
            (insert "* Prelude\nSkip me.\n")
            (insert "* Target\n")
            (insert "SCHEDULED: <2026-05-27 Wed>\n")
            (insert ":PROPERTIES:\n:CUSTOM_ID: target\n:END:\n")
            (insert "First body [fn:local].\n")
            (insert "** Child\nChild body.\n")
            (insert "[fn:local] Included footnote.\n"))
          (with-temp-buffer
            (org-mode)
            (insert "#+TITLE: Main\n")
            (insert "* Parent\n")
            (insert "#+INCLUDE: \"" inc "::* Target\" :only-contents t :minlevel 3\n")
            (insert "* After\n")
            (goto-char (point-min))
            (org-export-expand-include-keyword nil root nil nil nil)
            (let ((tree (org-element-parse-buffer)))
              (list (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :level h)
                              (org-element-property :raw-value h))))
                    (org-element-map tree 'footnote-definition
                      (lambda (f) (org-element-property :label f)))
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_include_literal_blocks_lines_parse_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 28 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let* ((root (make-temp-file "org-include-literal" t))
         (src (expand-file-name "snippet.el" root))
         (txt (expand-file-name "notes.txt" root)))
    (unwind-protect
        (progn
          (with-temp-file src
            (insert ";; one\n(message \"two\")\n(message \"three\")\n;; four\n"))
          (with-temp-file txt
            (insert "alpha\nbeta <tag>\ngamma\n"))
          (with-temp-buffer
            (org-mode)
            (insert "#+INCLUDE: \"" src "\" src emacs-lisp :lines \"2-3\" -n\n")
            (insert "#+INCLUDE: \"" txt "\" example :lines \"1-2\"\n")
            (goto-char (point-min))
            (let ((parsed-src (org-export-parse-include-value
                               (concat "\"" src "\" src emacs-lisp :lines \"2-3\" -n")
                               root))
                  (parsed-example (org-export-parse-include-value
                                   (concat "\"" txt "\" example :lines \"1-2\"")
                                   root)))
              (org-export-expand-include-keyword nil root nil nil nil)
              (list parsed-src
                    parsed-example
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_macro_counter_nested_replacement_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (error \"Undefined Org macro: counter; aborting\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-macro)
  (with-temp-buffer
    (org-mode)
    (insert "#+MACRO: wrap <<$1>>\n")
    (insert "#+MACRO: pair $1={{{$2}}}\n")
    (insert "A {{{counter(seq)}}}; B {{{counter(seq,+3)}}}; ")
    (insert "C {{{counter(seq)}}}; D {{{counter(seq,-1)}}}; ")
    (insert "E {{{wrap(text)}}}; F {{{pair(label,wrap(value))}}}.\n")
    (let ((templates (org-macro--collect-macros)))
      (list (mapcar #'car templates)
            (org-macro-expand "wrap(text)" templates)
            (org-macro-expand "pair(label,wrap(value))" templates)
            (progn
              (org-macro-replace-all templates)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_include_nested_macro_footnote_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"Definition not found for footnote outer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (require 'org-macro)
  (let* ((root (make-temp-file "org-include-nested" t))
         (sub (expand-file-name "sub" root))
         (inner (expand-file-name "inner.org" sub))
         (outer (expand-file-name "outer.org" root)))
    (unwind-protect
        (progn
          (make-directory sub)
          (with-temp-file inner
            (insert "#+MACRO: inner /Inner $1/\n")
            (insert "* Inner Head\n")
            (insert "Inner body {{{inner(value)}}} [fn:inner].\n")
            (insert "[fn:inner] Inner footnote.\n"))
          (with-temp-file outer
            (insert "#+MACRO: outer *Outer $1*\n")
            (insert "* Outer Head\n")
            (insert "Outer body {{{outer(value)}}}.\n")
            (insert "#+INCLUDE: \"sub/inner.org\" :minlevel 2\n")
            (insert "[fn:outer] Outer footnote.\n"))
          (with-temp-buffer
            (org-mode)
            (insert "#+TITLE: Main\n")
            (insert "#+MACRO: main =Main $1=\n")
            (insert "* Main Head\n")
            (insert "Main body {{{main(value)}}} [fn:outer].\n")
            (insert "#+INCLUDE: \"" outer "\" :minlevel 2\n")
            (goto-char (point-min))
            (org-export-expand-include-keyword nil root nil nil nil)
            (let* ((expanded (buffer-substring-no-properties
                              (point-min) (point-max)))
                   (templates (org-macro--collect-macros))
                   (macro-output (progn
                                   (org-macro-replace-all templates)
                                   (buffer-substring-no-properties
                                    (point-min) (point-max))))
                   (tree (org-element-parse-buffer))
                   (html (replace-regexp-in-string
                          "org[[:alnum:]]+"
                          "org-id"
                          (org-export-as 'html nil nil t nil))))
              (list (mapcar #'car templates)
                    (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :level h)
                              (org-element-property :raw-value h))))
                    (org-element-map tree 'footnote-definition
                      (lambda (f) (org-element-property :label f)))
                    expanded
                    macro-output
                    (not (null (string-match-p "<b>Outer value</b>" html)))
                    (not (null (string-match-p "<i>Inner value</i>" html)))
                    (not (null (string-match-p "footnotes" html)))
                    html)))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_macro_builtin_property_date_env_include_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (user-error \"Unable to read file \\\"/$ORG_ORACLE_INCLUDE_ROOT/env-include.org\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (require 'org-macro)
  (let* ((root (make-temp-file "org-macro-env" t))
         (inc (expand-file-name "env-include.org" root))
         (loop-a (expand-file-name "loop-a.org" root))
         (loop-b (expand-file-name "loop-b.org" root))
         (old-env (getenv "ORG_ORACLE_INCLUDE_ROOT")))
    (unwind-protect
        (progn
          (setenv "ORG_ORACLE_INCLUDE_ROOT" root)
          (with-temp-file inc
            (insert "* Env Head\n")
            (insert ":PROPERTIES:\n:Owner: EnvOwner\n:END:\n")
            (insert "Env body {{{property(Owner,* Env Head)}}}.\n"))
          (with-temp-file loop-a
            (insert "#+INCLUDE: \"loop-b.org\"\n"))
          (with-temp-file loop-b
            (insert "#+INCLUDE: \"loop-a.org\"\n"))
          (with-temp-buffer
            (org-mode)
            (insert "#+TITLE: Main Title\n")
            (insert "#+AUTHOR: Ada\n")
            (insert "#+AUTHOR: Bea\n")
            (insert "#+DATE: <2026-05-27 Wed>\n")
            (insert "#+MACRO: kw (eval (org-macro--find-keyword-value $1 t))\n")
            (insert "#+MACRO: prop {{{property($1,$2)}}}\n")
            (insert "* Target\n")
            (insert ":PROPERTIES:\n:Owner: LocalOwner\n:Effort: 1:30\n:END:\n")
            (insert "Date {{{date(%Y-%m-%d)}}}; ")
            (insert "authors {{{kw(AUTHOR)}}}; ")
            (insert "owner {{{prop(Owner,* Target)}}}; ")
            (insert "missing {{{property(Missing,* Target)}}}.\n")
            (insert "#+INCLUDE: \"$ORG_ORACLE_INCLUDE_ROOT/env-include.org\" :minlevel 2\n")
            (let ((before (buffer-substring-no-properties
                           (point-min) (point-max)))
                  expanded recursive-error templates macro-output tree)
              (goto-char (point-min))
              (org-export-expand-include-keyword nil "/" nil nil t)
              (setq expanded
                    (buffer-substring-no-properties
                     (point-min) (point-max)))
              (setq templates (org-macro--collect-macros))
              (org-macro-replace-all templates)
              (setq macro-output
                    (buffer-substring-no-properties
                     (point-min) (point-max)))
              (setq tree (org-element-parse-buffer))
              (with-temp-buffer
                (org-mode)
                (insert "#+INCLUDE: \"loop-a.org\"\n")
                (goto-char (point-min))
                (setq recursive-error
                      (condition-case err
                          (progn
                            (org-export-expand-include-keyword
                             nil root nil nil nil)
                            nil)
                        (error
                         (cons
                          (car err)
                          (mapcar
                           (lambda (value)
                             (if (stringp value)
                                 (replace-regexp-in-string
                                  (regexp-quote root) "<root>" value)
                               value))
                           (cdr err))))))))
              (list before
                    expanded
                    (mapcar #'car templates)
                    macro-output
                    (org-macro--find-keyword-value "AUTHOR" t)
                    (org-macro--find-date)
                    (org-macro--get-property "Owner" "* Target")
                    (org-macro--get-property "Effort" "* Target")
                    recursive-error
                    (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :level h)
                              (org-element-property :raw-value h))))
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))
      (if old-env
          (setenv "ORG_ORACLE_INCLUDE_ROOT" old-env)
        (setenv "ORG_ORACLE_INCLUDE_ROOT" nil))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_include_export_environment_reference_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 73 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (require 'org-macro)
  (let* ((root (make-temp-file "org-include-env-ref" t))
         (inc (expand-file-name "chapter.org" root)))
    (unwind-protect
        (progn
          (with-temp-file inc
            (insert "#+PROPERTY: Project_ALL Alpha Beta\n")
            (insert "#+MACRO: chapter Chapter-$1\n")
            (insert "* Included :inc:\n")
            (insert ":PROPERTIES:\n:CUSTOM_ID: included\n:Project: Alpha\n:END:\n")
            (insert "#+NAME: inc-table\n")
            (insert "#+CAPTION: Included table {{{chapter(table)}}}\n")
            (insert "| Key | Value |\n|-----+-------|\n| A   | 1     |\n")
            (insert "See <<inc-target>> and [[#main][main]].\n")
            (insert "<<inc-target>>\n"))
          (with-temp-buffer
            (org-mode)
            (insert "#+TITLE: Main\n")
            (insert "#+AUTHOR: Ada\n")
            (insert "#+OPTIONS: toc:nil tags:t\n")
            (insert "#+PROPERTY: Project_ALL Alpha Beta Gamma\n")
            (insert "#+MACRO: local Local-$1\n")
            (insert "* Main :root:\n")
            (insert ":PROPERTIES:\n:CUSTOM_ID: main\n:Project: Gamma\n:END:\n")
            (insert "#+NAME: main-table\n")
            (insert "#+CAPTION: Main table {{{local(table)}}}\n")
            (insert "| Key | Value |\n|-----+-------|\n| M   | 9     |\n")
            (insert "#+INCLUDE: \"" inc "\" :minlevel 2\n")
            (goto-char (point-min))
            (org-export-expand-include-keyword nil root nil nil nil)
            (let* ((expanded (buffer-substring-no-properties
                              (point-min) (point-max)))
                   (templates (org-macro--collect-macros))
                   (_ (org-macro-replace-all templates))
                   (tree (org-element-parse-buffer))
                   (info (org-export-get-environment 'html nil nil))
                   (headlines
                    (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :level h)
                              (org-element-property :raw-value h)
                              (org-export-get-reference h info)
                              (org-export-get-tags h info)
                              (org-export-get-node-property
                               "Project" h t)
                              (org-export-get-category h info)))))
                   (tables
                    (org-element-map tree 'table
                      (lambda (table)
                        (list (org-element-property :name table)
                              (org-export-get-reference table info)
                              (org-export-get-caption table)
                              (org-export-get-ordinal table info)))))
                   (targets
                    (org-element-map tree 'target
                      (lambda (target)
                        (list (org-element-property :value target)
                              (org-export-get-reference target info))))))
              (list expanded
                    (mapcar #'car templates)
                    headlines
                    tables
                    targets
                    (plist-get info :title)
                    (plist-get info :author)
                    (plist-get info :with-toc)
                     (replace-regexp-in-string
                      "org[[:alnum:]]+"
                      "org-id"
                      (org-export-as 'html nil nil t nil)))))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_macro_expand_nested_arg_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"#+MACRO: greet Hello, $1!\\n#+MACRO: wrap /$1/\\n#+MACRO: twice $1 and $1\\n#+MACRO: concat $1$2\\n#+MACRO: nested {{{wrap($1)}}} plus $2\\n\\n* Section\\nGreet: {{{greet(World)}}}\\nWrap: {{{wrap(important)}}}\\nTwice: {{{twice(repeated)}}}\\nConcat: {{{concat(foo,bar)}}}\\nNested: {{{nested(bold,extra)}}}\\n\" \"#+MACRO: greet Hello, $1!\\n#+MACRO: wrap /$1/\\n#+MACRO: twice $1 and $1\\n#+MACRO: concat $1$2\\n#+MACRO: nested {{{wrap($1)}}} plus $2\\n\\n* Section\\nGreet: Hello, World!\\nWrap: /important/\\nTwice: repeated and repeated\\nConcat: foobar\\nNested: /bold/ plus extra\\n\" (\"author\" \"concat\" \"date\" \"email\" \"greet\" \"nested\" \"title\" \"twice\" \"wrap\") ((\"nested\" \"{{{wrap($1)}}} plus $2\") (\"concat\" \"$1$2\") (\"twice\" \"$1 and $1\") (\"wrap\" \"/$1/\") (\"greet\" \"Hello, $1!\") (\"author\" nil) (\"email\" nil) (\"title\" nil) (\"date\" nil)) (375 395))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-macro)
  (with-temp-buffer
    (org-mode)
    (insert "#+MACRO: greet Hello, $1!\n")
    (insert "#+MACRO: wrap /$1/\n")
    (insert "#+MACRO: twice $1 and $1\n")
    (insert "#+MACRO: concat $1$2\n")
    (insert "#+MACRO: nested {{{wrap($1)}}} plus $2\n\n")
    (insert "* Section\n")
    (insert "Greet: {{{greet(World)}}}\n")
    (insert "Wrap: {{{wrap(important)}}}\n")
    (insert "Twice: {{{twice(repeated)}}}\n")
    (insert "Concat: {{{concat(foo,bar)}}}\n")
    (insert "Nested: {{{nested(bold,extra)}}}\n")
    (let ((before (buffer-substring-no-properties
                   (point-min) (point-max))))
      ;; Collect and replace
      (let ((macros (org-macro--collect-macros)))
        (org-macro-replace-all macros)
        (let ((after (buffer-substring-no-properties
                      (point-min) (point-max)))
              (macro-names (sort (mapcar #'car macros) #'string<))
              (macro-vals (mapcar (lambda (m)
                                    (list (car m) (cdr m)))
                                  macros)))
          ;; Export after expansion
          (let* ((html (org-export-as 'html nil nil t nil))
                 (has-greet (string-match-p "Hello, World!" html))
                 (has-italic (string-match-p "<i>" html)))
            (list before
                  after
                  macro-names
                  macro-vals
                  (list has-greet has-italic))))))))"##,
        expect,
    );
}

#[test]
fn org_macro_chained_nested_expansion_divergence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"#+MACRO: greet Hello, $1!\\n#+MACRO: wrap /$1/\\n#+MACRO: twice $1 and $1\\n\\n* Section\\nChained: {{{twice({{{greet(A)}}})}}}\\n\" \"#+MACRO: greet Hello, $1!\\n#+MACRO: wrap /$1/\\n#+MACRO: twice $1 and $1\\n\\n* Section\\nChained: Hello, A and {{{greet(A!\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-macro)
  (with-temp-buffer
    (org-mode)
    (insert "#+MACRO: greet Hello, $1!\n")
    (insert "#+MACRO: wrap /$1/\n")
    (insert "#+MACRO: twice $1 and $1\n\n")
    (insert "* Section\n")
    (insert "Chained: {{{twice({{{greet(A)}}})}}}\n")
    (let ((before (buffer-substring-no-properties
                   (point-min) (point-max))))
      (let ((macros (org-macro--collect-macros)))
        (org-macro-replace-all macros)
        (let ((after (buffer-substring-no-properties
                      (point-min) (point-max))))
          (list before after))))))"##,
        expect,
    );
}

#[test]
fn org_include_file_lines_blocks_export_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 55 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox-html)
  (let* ((root (make-temp-file "org-include-deep" t))
         (src-file (expand-file-name "src.org" root))
         (code-file (expand-file-name "code.el" root)))
    (unwind-protect
        (progn
          (with-temp-file src-file
            (insert "#+TITLE: Include Test\n\n")
            (insert "* Section\n")
            (insert "#+INCLUDE: \"" code-file "\" src emacs-lisp\n\n")
            (insert "After include.\n"))
          (with-temp-file code-file
            (insert "(defun hello ()\n  (message \"hello\"))\n"))
          (with-current-buffer (find-file-noselect src-file)
            (org-mode)
            ;; Preview includes
            (let* ((before (buffer-substring-no-properties
                            (point-min) (point-max)))
                   (tree-before (org-element-parse-buffer))
                   (types-before
                    (mapcar #'org-element-type
                            (org-element-map tree-before t #'identity))))
              ;; Execute include
              (org-export-expand-include-keyword)
                (let* ((norm (lambda (s)
                              (replace-regexp-in-string
                               "org[[:alnum:]]\\{7,9\\}" "orgHASH"
                              (replace-regexp-in-string
                               (regexp-quote root) "<root>" s))))
                       (after (funcall norm
                              (buffer-substring-no-properties
                               (point-min) (point-max))))
                       (tree-after (org-element-parse-buffer))
                       (types-after
                        (mapcar #'org-element-type
                                (org-element-map tree-after t #'identity)))
                       (src-blocks
                        (org-element-map tree-after 'src-block
                          (lambda (sb)
                            (list (org-element-property :language sb)
                                  (org-element-property :value sb)))))
                       (html (funcall norm
                              (org-export-as 'html nil nil t nil)))
                       (has-code (string-match-p "defun" html)))
                  (kill-buffer)
                  (list (funcall norm before)
                        types-before
                        after
                        types-after
                        src-blocks
                        has-code
                        html))))))
      (delete-directory root t))))"##,
        expect,
    );
}
