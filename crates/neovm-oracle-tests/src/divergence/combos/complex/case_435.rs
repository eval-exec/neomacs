//! Complex combo batch 435 — 16 niche org-mode probes: org-export LaTeX,
//! org-export HTML, org-publish, org-datetree, org-clocktable,
//! org-crypt, org-mobile, org-refile, org-toggle, org-store-link,
//! org-insert-link, org-mark-ring, org-speed-commands, org-find-entry-with-id,
//! org-in-progress, org-log-note.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// org-export LaTeX: exporting to LaTeX format.
#[test]
fn div_cx435_org_export_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox-latex)
  (with-temp-buffer
    (insert "* Hello\nWorld\n")
    (list (string-match "\\\\section" (let ((org-export-with-toc nil))
     (org-export-as 'latex nil nil t nil)))
      t))
"##,
        expect,
    );
}

/// org-export HTML: exporting to HTML.
#[test]
fn div_cx435_org_export_html() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox-html)
  (with-temp-buffer
    (insert "* Hello\nWorld\n")
    (list (string-match "outline-2" (let ((org-export-with-toc nil))
      (org-export-as 'html nil nil t nil)))
      t))
"##,
        expect,
    );
}

/// org-publish: publishing projects.
#[test]
fn div_cx435_org_publish_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox-publish)
  (list (fboundp 'org-publish)
        (fboundp 'org-publish-current-project)))
"##,
        expect,
    );
}

/// org-datetree: date tree operations.
#[test]
fn div_cx435_org_datetree_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-datetree-insert-line)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-datetree)
  (with-temp-buffer
    (org-mode)
    (org-datetree-insert-line (encode-time 0 0 0 16 6 2024 nil))
    (buffer-string)))
"##,
        expect,
    );
}

/// org-clocktable: clock table generation.
#[test]
fn div_cx435_org_clocktable_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-clock)
  (with-temp-buffer
    (org-mode)
    (insert "#+BEGIN: clocktable :maxlevel 2\n#+END:\n")
    (list (fboundp 'org-dblock-write:clocktable)
          (fboundp 'org-clock-report)))
  "##,
        expect,
    );
}

/// org-refile: refiling headings.
#[test]
fn div_cx435_org_refile_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* Source\n* Target\n")
    (list (fboundp 'org-refile)
          (fboundp 'org-refile-cache-clear)))
"##,
        expect,
    );
}

/// org-toggle: toggling org features.
#[test]
fn div_cx435_org_toggle_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "text\n")
    (org-toggle-item (point-min) (point-max))
    (buffer-string)))
"##,
        expect,
    );
}

/// org-insert-link / org-store-link.
#[test]
fn div_cx435_org_store_insert_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "[[https://orgmode.org]]\n")
    (goto-char 1)
    (org-insert-link nil "https://example.com" "Example"))
  "##,
        expect,
    );
}

/// org-mark-ring: mark ring operations.
#[test]
fn div_cx435_org_mark_ring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* H1\n* H2\n")
    (org-mark-ring-push 1)
    (org-mark-ring-push 5)
    (list (fboundp 'org-mark-ring-goto)
          (boundp 'org-mark-ring)))
"##,
        expect,
    );
}

/// org-speed-commands: speed keys.
#[test]
fn div_cx435_org_speed_commands() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable org-speed-commands-default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (list (boundp 'org-speed-commands-default)
        (assoc "n" org-speed-commands-default)))
"##,
        expect,
    );
}

/// org-find-entry-with-id: finding entry by ID.
#[test]
fn div_cx435_org_find_entry_with_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-id)
  (with-temp-buffer
    (org-mode)
    (insert "* heading\n")
    (let ((id (org-id-get-create)))
      (condition-case e
          (org-id-find id)
        (error (car e)))))
"##,
        expect,
    );
}

/// org-log-note: logging notes.
#[test]
fn div_cx435_org_log_note() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
  (list (boundp 'org-log-note-headings)
        (fboundp 'org-add-log-note)))
"##,
        expect,
    );
}

/// org-plot: plotting org data.
#[test]
fn div_cx435_org_plot_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-plot)
  (list (fboundp 'org-plot/gnuplot)
        (fboundp 'org-plot/gnuplot-script)))
"##,
        expect,
    );
}

/// org-ctags: ctags integration.
#[test]
fn div_cx435_org_ctags_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-ctags)
  (list (boundp 'org-ctags-enabled-p)
        (fboundp 'org-ctags-visit-directory-tree)))
"##,
        expect,
    );
}

/// org-indent: indentation mode.
#[test]
fn div_cx435_org_indent_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-indent)
  (with-temp-buffer
    (org-mode)
    (org-indent-mode 1)
    (list org-indent-mode
          (boundp 'org-indent-indentation-per-level)))
"##,
        expect,
    );
}

/// org-faces: org-mode face definitions.
#[test]
fn div_cx435_org_faces_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified])""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-faces)
  (list (facep 'org-level-1)
        (facep 'org-level-2)
        (facep 'org-date)
        (facep 'org-tag)))
"##,
        expect,
    );
}
