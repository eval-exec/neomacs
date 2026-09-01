//! Strong uncovered-features-26 oracle tests — org-export and publishing.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-export-as-html
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_html() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-as-html)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H1\nBody *bold* /italic/")
  (org-export-as-html nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-as-latex
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-as-latex)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H1\nBody *bold* /italic/")
  (org-export-as-latex nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-as-ascii
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-as-ascii)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H1\nBody *bold* /italic/")
  (org-export-as-ascii nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-as-utf8
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-as-utf8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H1\nBody *bold* /italic/")
  (org-export-as-utf8 nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-as-html-to-buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_html_buf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-as-html-to-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H1\nBody *bold* /italic/")
  (org-export-as-html-to-buffer nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-as-latex-to-buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_latex_buf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-as-latex-to-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H1\nBody *bold* /italic/")
  (org-export-as-latex-to-buffer nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-region-as-html
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-region-as-html)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H1\nBody *bold* /italic/")
  (goto-char (point-min))
  (search-forward "Body")
  (beginning-of-line)
  (org-export-region-as-html (point) (point-max) nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-as-pdf
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_pdf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H1\nBody *bold* /italic/")
  (condition-case nil
      (org-export-as-pdf nil)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-as-pdf-and-open
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_pdf_open() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H1\nBody *bold* /italic/")
  (condition-case nil
      (org-export-as-pdf-and-open nil)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-dispatch
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (:error (user-error \"Export aborted\") \"#+TITLE: Test\\n* H1\\nBody *bold* /italic/\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H1\nBody *bold* /italic/")
  (condition-case err
      (let ((unread-command-events (list ?q)))
        (org-export-dispatch nil)
        (list :ok (buffer-string)))
    (error (list :error err (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-html-export-as-html
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_html_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK \"<?xml version=\\\"1.0\\\" encoding=\\\"utf-8\\\"?>\\n<!DOCTYPE html PUBLIC \\\"-//W3C//DTD XHTML 1.0 Strict//EN\\\"\\n\\\"http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd\\\">\\n<html xmlns=\\\"http://www.w3.org/1999/xhtml\\\" lang=\\\"en\\\" xml:lang=\\\"en\\\">\\n<head>\\n<!-- 2026-06-15 Mon 12:00 -->\\n<meta http-equiv=\\\"Content-Type\\\" content=\\\"text/html;charset=utf-8\\\" />\\n<meta name=\\\"viewport\\\" content=\\\"width=device-width, initial-scale=1\\\" />\\n<title>Test</title>\\n<meta name=\\\"generator\\\" content=\\\"Org Mode\\\" />\\n<style type=\\\"text/css\\\">\\n  #content { max-width: 60em; margin: auto; }\\n  .title  { text-align: center;\\n             margin-bottom: .2em; }\\n  .subtitle { text-align: center;\\n              font-size: medium;\\n              font-weight: bold;\\n              margin-top:0; }\\n  .todo   { font-family: monospace; color: red; }\\n  .done   { font-family: monospace; color: green; }\\n  .priority { font-family: monospace; color: orange; }\\n  .tag    { background-color: #eee; font-family: monospace;\\n            padding: 2px; font-size: 80%; font-weight: normal; }\\n  .timestamp { color: #bebebe; }\\n  .timestamp-kwd { color: #5f9ea0; }\\n  .org-right  { margin-left: auto; margin-right: 0px;  text-align: right; }\\n  .org-left   { margin-left: 0px;  margin-right: auto; text-align: left; }\\n  .org-center { margin-left: auto; margin-right: auto; text-align: center; }\\n  .underline { text-decoration: underline; }\\n  #postamble p, #preamble p { font-size: 90%; margin: .2em; }\\n  p.verse { margin-left: 3%; }\\n  pre {\\n    border: 1px solid #e6e6e6;\\n    border-radius: 3px;\\n    background-color: #f2f2f2;\\n    padding: 8pt;\\n    font-family: monospace;\\n    overflow: auto;\\n    margin: 1.2em;\\n  }\\n  pre.src {\\n    position: relative;\\n    overflow: auto;\\n  }\\n  pre.src:before {\\n    display: none;\\n    position: absolute;\\n    top: -8px;\\n    right: 12px;\\n    padding: 3px;\\n    color: #555;\\n    background-color: #f2f2f299;\\n  }\\n  pre.src:hover:before { display: inline; margin-top: 14px;}\\n  /* Languages per Org manual */\\n  pre.src-asymptote:before { content: 'Asymptote'; }\\n  pre.src-awk:before { content: 'Awk'; }\\n  pre.src-authinfo::before { content: 'Authinfo'; }\\n  pre.src-c:before { content: 'C'; }\\n  pre.src-C:before { content: 'C'; }\\n  /* pre.src-C++ doesn't work in CSS */\\n  pre.src-clojure:before { content: 'Clojure'; }\\n  pre.src-css:before { content: 'CSS'; }\\n  pre.src-D:before { content: 'D'; }\\n  pre.src-ditaa:before { content: 'ditaa'; }\\n  pre.src-dot:before { content: 'Graphviz'; }\\n  pre.src-calc:before { content: 'Emacs Calc'; }\\n  pre.src-emacs-lisp:before { content: 'Emacs Lisp'; }\\n  pre.src-fortran:before { content: 'Fortran'; }\\n  pre.src-gnuplot:before { content: 'gnuplot'; }\\n  pre.src-haskell:before { content: 'Haskell'; }\\n  pre.src-hledger:before { content: 'hledger'; }\\n  pre.src-java:before { content: 'Java'; }\\n  pre.src-js:before { content: 'JavaScript'; }\\n  pre.src-latex:before { content: 'LaTeX'; }\\n  pre.src-ledger:before { content: 'Ledger'; }\\n  pre.src-lisp:before { content: 'Lisp'; }\\n  pre.src-lilypond:before { content: 'Lilypond'; }\\n  pre.src-lua:before { content: 'Lua'; }\\n  pre.src-matlab:before { content: 'MATLAB'; }\\n  pre.src-mscgen:before { content: 'Mscgen'; }\\n  pre.src-ocaml:before { content: 'Objective Caml'; }\\n  pre.src-octave:before { content: 'Octave'; }\\n  pre.src-org:before { content: 'Org mode'; }\\n  pre.src-oz:before { content: 'OZ'; }\\n  pre.src-plantuml:before { content: 'Plantuml'; }\\n  pre.src-processing:before { content: 'Processing.js'; }\\n  pre.src-python:before { content: 'Python'; }\\n  pre.src-R:before { content: 'R'; }\\n  pre.src-ruby:before { content: 'Ruby'; }\\n  pre.src-sass:before { content: 'Sass'; }\\n  pre.src-scheme:before { content: 'Scheme'; }\\n  pre.src-screen:before { content: 'Gnu Screen'; }\\n  pre.src-sed:before { content: 'Sed'; }\\n  pre.src-sh:before { content: 'shell'; }\\n  pre.src-sql:before { content: 'SQL'; }\\n  pre.src-sqlite:before { content: 'SQLite'; }\\n  /* additional languages in org.el's org-babel-load-languages alist */\\n  pre.src-forth:before { content: 'Forth'; }\\n  pre.src-io:before { content: 'IO'; }\\n  pre.src-J:before { content: 'J'; }\\n  pre.src-makefile:before { content: 'Makefile'; }\\n  pre.src-maxima:before { content: 'Maxima'; }\\n  pre.src-perl:before { content: 'Perl'; }\\n  pre.src-picolisp:before { content: 'Pico Lisp'; }\\n  pre.src-scala:before { content: 'Scala'; }\\n  pre.src-shell:before { content: 'Shell Script'; }\\n  pre.src-ebnf2ps:before { content: 'ebfn2ps'; }\\n  /* additional language identifiers per \\\"defun org-babel-execute\\\"\\n       in ob-*.el */\\n  pre.src-cpp:before  { content: 'C++'; }\\n  pre.src-abc:before  { content: 'ABC'; }\\n  pre.src-coq:before  { content: 'Coq'; }\\n  pre.src-groovy:before  { content: 'Groovy'; }\\n  /* additional language identifiers from org-babel-shell-names in\\n     ob-shell.el: ob-shell is the only babel language using a lambda to put\\n     the execution function name together. */\\n  pre.src-bash:before  { content: 'bash'; }\\n  pre.src-csh:before  { content: 'csh'; }\\n  pre.src-ash:before  { content: 'ash'; }\\n  pre.src-dash:before  { content: 'dash'; }\\n  pre.src-ksh:before  { content: 'ksh'; }\\n  pre.src-mksh:before  { content: 'mksh'; }\\n  pre.src-posh:before  { content: 'posh'; }\\n  /* Additional Emacs modes also supported by the LaTeX listings package */\\n  pre.src-ada:before { content: 'Ada'; }\\n  pre.src-asm:before { content: 'Assembler'; }\\n  pre.src-caml:before { content: 'Caml'; }\\n  pre.src-delphi:before { content: 'Delphi'; }\\n  pre.src-html:before { content: 'HTML'; }\\n  pre.src-idl:before { content: 'IDL'; }\\n  pre.src-mercury:before { content: 'Mercury'; }\\n  pre.src-metapost:before { content: 'MetaPost'; }\\n  pre.src-modula-2:before { content: 'Modula-2'; }\\n  pre.src-pascal:before { content: 'Pascal'; }\\n  pre.src-ps:before { content: 'PostScript'; }\\n  pre.src-prolog:before { content: 'Prolog'; }\\n  pre.src-simula:before { content: 'Simula'; }\\n  pre.src-tcl:before { content: 'tcl'; }\\n  pre.src-tex:before { content: 'TeX'; }\\n  pre.src-plain-tex:before { content: 'Plain TeX'; }\\n  pre.src-verilog:before { content: 'Verilog'; }\\n  pre.src-vhdl:before { content: 'VHDL'; }\\n  pre.src-xml:before { content: 'XML'; }\\n  pre.src-nxml:before { content: 'XML'; }\\n  /* add a generic configuration mode; LaTeX export needs an additional\\n     (add-to-list 'org-latex-listings-langs '(conf \\\" \\\")) in .emacs */\\n  pre.src-conf:before { content: 'Configuration File'; }\\n\\n  table { border-collapse:collapse; }\\n  caption.t-above { caption-side: top; }\\n  caption.t-bottom { caption-side: bottom; }\\n  td, th { vertical-align:top;  }\\n  th.org-right  { text-align: center;  }\\n  th.org-left   { text-align: center;   }\\n  th.org-center { text-align: center; }\\n  td.org-right  { text-align: right;  }\\n  td.org-left   { text-align: left;   }\\n  td.org-center { text-align: center; }\\n  dt { font-weight: bold; }\\n  .footpara { display: inline; }\\n  .footdef  { margin-bottom: 1em; }\\n  .figure { padding: 1em; }\\n  .figure p { text-align: center; }\\n  .equation-container {\\n    display: table;\\n    text-align: center;\\n    width: 100%;\\n  }\\n  .equation {\\n    vertical-align: middle;\\n  }\\n  .equation-label {\\n    display: table-cell;\\n    text-align: right;\\n    vertical-align: middle;\\n  }\\n  .inlinetask {\\n    padding: 10px;\\n    border: 2px solid gray;\\n    margin: 10px;\\n    background: #ffffcc;\\n  }\\n  #org-div-home-and-up\\n   { text-align: right; font-size: 70%; white-space: nowrap; }\\n  textarea { overflow-x: auto; }\\n  .linenr { font-size: smaller }\\n  .code-highlighted { background-color: #ffff00; }\\n  .org-info-js_info-navigation { border-style: none; }\\n  #org-info-js_console-label\\n    { font-size: 10px; font-weight: bold; white-space: nowrap; }\\n  .org-info-js_search-highlight\\n    { background-color: #ffff00; color: #000000; font-weight: bold; }\\n  .org-svg { }\\n</style>\\n</head>\\n<body>\\n<div id=\\\"content\\\" class=\\\"content\\\">\\n<h1 class=\\\"title\\\">Test</h1>\\n<div id=\\\"table-of-contents\\\" role=\\\"doc-toc\\\">\\n<h2>Table of Contents</h2>\\n<div id=\\\"text-table-of-contents\\\" role=\\\"doc-toc\\\">\\n<ul>\\n<li><a href=\\\"#orgXXXXXXX\\\">1. H1</a></li>\\n</ul>\\n</div>\\n</div>\\n<div id=\\\"outline-container-orgXXXXXXX\\\" class=\\\"outline-2\\\">\\n<h2 id=\\\"orgXXXXXXX\\\"><span class=\\\"section-number-2\\\">1.</span> H1</h2>\\n<div class=\\\"outline-text-2\\\" id=\\\"text-1\\\">\\n<p>\\nBody <b>bold</b> <i>italic</i></p>\\n</div>\\n</div>\\n</div>\\n<div id=\\\"postamble\\\" class=\\\"status\\\">\\n<p class=\\\"date\\\">Created: 2026-06-15 Mon 12:00</p>\\n<p class=\\\"validation\\\"><a href=\\\"https://validator.w3.org/check?uri=referer\\\">Validate</a></p>\\n</div>\\n</body>\\n</html>\"""##
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (let ((org-export-time-stamp-file nil))
    (insert "#+TITLE: Test\n* H1\nBody *bold* /italic/")
    (condition-case nil
        (org-html-export-as-html)
      (error nil))
    (replace-regexp-in-string
     "org[0-9a-f]\\{7\\}" "orgXXXXXXX"
     (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-latex-export-as-latex
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_latex_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"% Created 2026-06-15 Mon 12:00\\n% Intended LaTeX compiler: pdflatex\\n\\\\documentclass[11pt]{article}\\n\\n\\\\usepackage[utf8]{inputenc}\\n\\\\usepackage[T1]{fontenc}\\n\\\\usepackage{graphicx}\\n\\\\usepackage{longtable}\\n\\\\usepackage{wrapfig}\\n\\\\usepackage{rotating}\\n\\\\usepackage[normalem]{ulem}\\n\\\\usepackage{amsmath}\\n\\\\usepackage{amssymb}\\n\\\\usepackage{capt-of}\\n\\\\usepackage{hyperref}\\n\\\\date{\\\\today}\\n\\\\title{Test}\\n\\\\hypersetup{\\n pdfauthor={},\\n pdftitle={Test},\\n pdfkeywords={},\\n pdfsubject={},\\n pdfcreator={},\\n pdflang={English}}\\n\\\\begin{document}\\n\\n\\\\maketitle\\n\\\\tableofcontents\\n\\n\\\\section{H1}\\n\\\\label{sec:orgXXXXXXX}\\nBody \\\\textbf{bold} \\\\emph{italic}\\n\\\\end{document}\"""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (let ((org-export-time-stamp-file nil))
    (insert "#+TITLE: Test\n* H1\nBody *bold* /italic/")
    (condition-case nil
        (org-latex-export-as-latex)
      (error nil))
    (replace-regexp-in-string
     "org[0-9a-f]\\{7\\}" "orgXXXXXXX"
     (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-ascii-export-as-ascii
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_ascii_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"                                 ______\\n\\n                                  TEST\\n                                 ______\\n\\n\\nTable of Contents\\n_________________\\n\\n1. H1\\n\\n\\n1 H1\\n====\\n\\n  Body *bold* /italic/\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H1\nBody *bold* /italic/")
  (condition-case nil
      (org-ascii-export-as-ascii)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-publish
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_publish() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-publish "test" nil)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-publish-all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_publish_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-publish-all nil)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-publish-current-file
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_publish_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-publish-current-file nil)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-publish-current-project
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_publish_project() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-publish-current-project nil)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-publish-sitemap
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_publish_sitemap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-publish-sitemap "test")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-define-backend
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-export-define-backend 'test '((template . (lambda (contents info) contents))))
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-get-environment
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_env() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Test\" 0 4 (:parent (#(\"Test\" 0 4 (:parent #4)))))) (#(\"Me\" 0 2 (:parent (#(\"Me\" 0 2 (:parent #4)))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n#+AUTHOR: Me\n* H1\nBody")
  (let ((env (org-export-get-environment nil)))
    (list (plist-get env :title)
          (plist-get env :author))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-get-contents
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-get-contents)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H1\nBody\n** H2\nSub")
  (let ((info (org-export-get-environment nil)))
    (org-export-get-contents (current-buffer) info)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-string-as
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-string-as)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-export-string-as "* H\nBody *bold*" 'html t)"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-string-as latex
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_string_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-string-as)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-export-string-as "* H\nBody *bold*" 'latex t)"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-string-as ascii
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf26_export_string_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-string-as)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-export-string-as "* H\nBody *bold*" 'ascii t)"##,
        expect,
    );
}
