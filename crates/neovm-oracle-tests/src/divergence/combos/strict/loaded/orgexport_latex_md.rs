//! Strict combo oracle probes, batch 78: org export to LaTeX (ox-latex) and
//! Markdown (ox-md) — each backend has distinct conversion logic.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_p2_org_export_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"% Created 2026-06-15 Mon 12:00\\n% Intended LaTeX compiler: pdflatex\\n\\\\documentclass[11pt]{article}\\n\\n\\\\usepackage[utf8]{inputenc}\\n\\\\usepackage[T1]{fontenc}\\n\\\\usepackage{graphicx}\\n\\\\usepackage{longtable}\\n\\\\usepackage{wrapfig}\\n\\\\usepackage{rotating}\\n\\\\usepackage[normalem]{ulem}\\n\\\\usepackage{amsmath}\\n\\\\usepackage{amssymb}\\n\\\\usepackage{capt-of}\\n\\\\usepackage{hyperref}\\n\\\\date{\\\\today}\\n\\\\title{}\\n\\\\hypersetup{\\n pdfauthor={},\\n pdftitle={},\\n pdfkeywords={},\\n pdfsubject={},\\n pdfcreator={},\\n pdflang={English}}\\n\\\\begin{document}\\n\\n\\\\tableofcontents\\n\\n\\\\section{Heading}\\n\\\\label{sec:orgID}\\nText with \\\\(E=mc^2\\\\) and \\\\href{https://example.com}{link}.\\n\\\\end{document}\"""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_with_load_expect(
        r##"
(replace-regexp-in-string "\\borg[0-9a-f]\\{6,\\}\\b" "orgID"
  (with-temp-buffer
    (org-mode)
    (insert "* Heading\nText with $E=mc^2$ and [[https://example.com][link]].\n")
    (org-export-as 'latex)))
"##,
        &["org/org.el", "org/ox.el", "org/ox-latex.el"],
        expect,
    );
}

#[test]
fn div_p2_org_export_markdown() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"\\n# Table of Contents\\n\\n1.  [Heading](#orgID)\\n\\n\\n<a id=\\\"orgID\\\"></a>\\n\\n# Heading\\n\\nText with [link](https://example.com).\\n\\n-   item one\\n-   item two\\n\\n\"""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(replace-regexp-in-string "\\borg[0-9a-f]\\{6,\\}\\b" "orgID"
  (with-temp-buffer
    (org-mode)
    (insert "* Heading\nText with [[https://example.com][link]].\n- item one\n- item two\n")
    (org-export-as 'md)))
"##,
        &["org/org.el", "org/ox.el", "org/ox-md.el"],
        expect,
    );
}

#[test]
fn div_p2_org_export_latex_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"% Created 2026-06-15 Mon 12:00\\n% Intended LaTeX compiler: pdflatex\\n\\\\documentclass[11pt]{article}\\n\\n\\\\usepackage[utf8]{inputenc}\\n\\\\usepackage[T1]{fontenc}\\n\\\\usepackage{graphicx}\\n\\\\usepackage{longtable}\\n\\\\usepackage{wrapfig}\\n\\\\usepackage{rotating}\\n\\\\usepackage[normalem]{ulem}\\n\\\\usepackage{amsmath}\\n\\\\usepackage{amssymb}\\n\\\\usepackage{capt-of}\\n\\\\usepackage{hyperref}\\n\\\\date{\\\\today}\\n\\\\title{}\\n\\\\hypersetup{\\n pdfauthor={},\\n pdftitle={},\\n pdfkeywords={},\\n pdfsubject={},\\n pdfcreator={},\\n pdflang={English}}\\n\\\\begin{document}\\n\\n\\\\tableofcontents\\n\\n\\\\section{Table}\\n\\\\label{sec:orgID}\\n\\\\begin{center}\\n\\\\begin{tabular}{rr}\\nA & B\\\\\\\\\\n1 & 2\\\\\\\\\\n3 & 4\\\\\\\\\\n\\\\end{tabular}\\n\\\\end{center}\\n\\\\end{document}\"""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_with_load_expect(
        r##"
(replace-regexp-in-string "\\borg[0-9a-f]\\{6,\\}\\b" "orgID"
  (with-temp-buffer
    (org-mode)
    (insert "* Table\n| A | B |\n| 1 | 2 |\n| 3 | 4 |\n")
    (org-export-as 'latex)))
"##,
        &["org/org.el", "org/ox.el", "org/ox-latex.el"],
        expect,
    );
}
