use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_schedule_deadline_priority_property_mutation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"<2026-05-27 Wed 09:30>\" \"<2026-05-28 Thu>\" \"1:15\" 2000 \"* TODO [#A] Task\\nDEADLINE: <2026-05-28 Thu> SCHEDULED: <2026-05-27 Wed 09:30>\\n:PROPERTIES:\\n:Effort:   1:15\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-log-reschedule nil)
          (org-log-redeadline nil))
      (org-mode)
      (insert "* TODO Task\n")
      (goto-char (point-min))
      (org-schedule nil "2026-05-27 Wed 09:30")
      (org-deadline nil "2026-05-28 Thu")
      (org-set-property "Effort" "1:15")
      (org-priority ?A)
      (list (org-entry-get nil "SCHEDULED")
            (org-entry-get nil "DEADLINE")
            (org-entry-get nil "Effort")
            (org-get-priority (thing-at-point 'line t))
            (buffer-substring-no-properties (point-min) (point-max))))))"#,
        expect,
    );
}

#[test]
fn org_clock_in_out_drawer_logbook_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (0 \"* TODO Task\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:30] =>  1:30\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r#"(progn
  (require 'org)
  (require 'org-clock)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Task\n")
    (goto-char (point-min))
    (let ((org-clock-into-drawer t)
          (org-clock-out-remove-zero-time-clocks nil))
      (org-clock-in nil (encode-time 0 0 9 27 5 2026))
      (org-clock-out nil t (encode-time 0 30 10 27 5 2026))
      (list org-clock-total-time
            (buffer-substring-no-properties (point-min) (point-max))))))"#,
        expect,
    );
}

#[test]
fn org_promote_demote_subtree_startup_odd_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"#+STARTUP: odd\\n* A\\n* B\\n** C\\n\" \"#+STARTUP: odd\\n* A\\n** B\\n*** C\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "#+STARTUP: odd\n")
    (insert "* A\n** B\n*** C\n")
    (goto-char (point-min))
    (search-forward "B")
    (beginning-of-line)
    (org-promote-subtree)
    (let ((after-promote
           (buffer-substring-no-properties (point-min) (point-max))))
      (org-demote-subtree)
      (list after-promote
            (buffer-substring-no-properties (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_list_indent_outdent_repair_lisp_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"- one\\n  - two\\n  - child\\n- three\\n\" \"- one\\n- two\\n  - child\\n- three\\n\" (unordered (\"one\") (\"two\" (unordered (\"child\"))) (\"three\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (org-mode)
    (insert "- one\n- two\n  - child\n- three\n")
    (goto-char (point-min))
    (search-forward "two")
    (beginning-of-line)
    (org-indent-item)
    (let ((after-indent
           (buffer-substring-no-properties (point-min) (point-max))))
      (org-outdent-item)
      (org-list-repair)
      (list after-indent
            (buffer-substring-no-properties (point-min) (point-max))
            (org-list-to-lisp)))))"#,
        expect,
    );
}

#[test]
fn org_texinfo_export_markup_list_table_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t t t \"@node Intro\\n@chapter Intro\\n\\nText with @strong{bold}@comma{} @samp{code}@comma{} and @uref{https://example.org, link}.\\n@itemize\\n@item\\nitem one\\n@item\\nitem two\\n@end itemize\\n@multitable {a} {a}\\n@item A\\n@tab B\\n@item 1\\n@tab 2\\n@end multitable\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-texinfo)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Manual\n")
    (insert "#+AUTHOR: Ada\n")
    (insert "* Intro\n")
    (insert "Text with *bold*, =code=, and [[https://example.org][link]].\n")
    (insert "- item one\n- item two\n")
    (insert "| A | B |\n| 1 | 2 |\n")
    (let* ((org-export-with-toc nil)
           (texi (org-export-as 'texinfo nil nil t nil)))
      (list (not (null (string-match-p "@node Intro" texi)))
            (not (null (string-match-p "@chapter Intro" texi)))
            (not (null (string-match-p "@strong{bold}" texi)))
            (not (null (string-match-p "@samp{code}" texi)))
            (not (null (string-match-p "@uref{https://example.org, link}" texi)))
            (not (null (string-match-p "@itemize" texi)))
            (not (null (string-match-p "@multitable" texi)))
            texi))))"##,
        expect,
    );
}

#[test]
fn org_beamer_export_frame_list_alert_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t \"\\\\section{Section}\\n\\\\label{sec:org-id}\\n\\\\begin{frame}[label={sec:org-id}]{Frame}\\n\\\\begin{itemize}\\n\\\\item item one\\n\\\\item item two\\n\\\\end{itemize}\\nA paragraph with \\\\alert{bold}.\\n\\\\end{frame}\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-beamer)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Slides\n")
    (insert "#+OPTIONS: H:2 toc:nil\n")
    (insert "* Section\n")
    (insert "** Frame\n")
    (insert "- item one\n- item two\n")
    (insert "#+ATTR_BEAMER: :overlay <2->\n")
    (insert "A paragraph with *bold*.\n")
    (let* ((org-export-with-toc nil)
           (latex (org-export-as 'beamer nil nil t nil))
           (normalized
            (replace-regexp-in-string
             "sec:org[[:alnum:]]+"
             "sec:org-id"
             latex)))
      (list (not (null (string-match-p "\\\\section" latex)))
            (not (null (string-match-p "\\\\begin{frame}" latex)))
            (not (null (string-match-p "{Frame}" latex)))
            (not (null (string-match-p "\\\\begin{itemize}" latex)))
            (not (null (string-match-p "\\\\alert{bold}" latex)))
            normalized))))"##,
        expect,
    );
}

#[test]
fn org_icalendar_export_todo_schedule_deadline_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t t \"BEGIN:VEVENT\\nDTSTAMP:<STAMP>\\nUID:TS1-<uid>\\nDTSTART;VALUE=DATE:20260528\\nDTEND;VALUE=DATE:20260529\\nSUMMARY:Event\\nDESCRIPTION:DEADLINE: <2026-05-28 Thu>\\nCATEGORIES:???\\nEND:VEVENT\\nBEGIN:VTODO\\nUID:TODO-<uid>\\nDTSTAMP:<STAMP>\\nDTSTART:20260527T090000\\nSUMMARY:Event\\nDESCRIPTION:DEADLINE: <2026-05-28 Thu>\\nCATEGORIES:???\\nSEQUENCE:1\\nPRIORITY:5\\nSTATUS:NEEDS-ACTION\\nEND:VTODO\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-icalendar)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Cal Probe\n")
    (insert "* TODO Event\n")
    (insert "SCHEDULED: <2026-05-27 Wed 09:00-10:00>\n")
    (insert "DEADLINE: <2026-05-28 Thu>\n")
    (let* ((org-icalendar-include-todo t)
           (ical (org-export-as 'icalendar nil nil t nil))
           (normalized
            (replace-regexp-in-string
             "DTSTAMP:[0-9TZ]+"
             "DTSTAMP:<stamp>"
             (replace-regexp-in-string
              "UID:\\(TS1\\|TODO\\)-[^\n]+"
              "UID:\\1-<uid>"
              ical))))
      (list (not (null (string-match-p "BEGIN:VEVENT" ical)))
            (not (null (string-match-p "BEGIN:VTODO" ical)))
            (not (null (string-match-p "SUMMARY:Event" ical)))
            (not (null (string-match-p "DTSTART:20260527T090000" ical)))
            (not (null (string-match-p "DTSTART;VALUE=DATE:20260528" ical)))
             (not (null (string-match-p "STATUS:NEEDS-ACTION" ical)))
             normalized))))"##,
        expect,
    );
}

#[test]
fn org_export_region_emphasis_link_footnote_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 37 50)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Export Region\n\n")
    (insert "* Section\n")
    (insert "Text with *bold*, /italic/, =code=, ~verbatim~, _underline_.\n\n")
    (insert "Link: [[https://example.org/path?q=1][Example]].\n\n")
    (insert "Footnote[fn:1] and inline[fn:2:inline note].\n\n")
    (insert "| A | B |\n|---+---|\n| 1 | 2 |\n\n")
    (insert "#+begin_quote\nQuoted *text*.\n#+end_quote\n\n")
    (insert "[fn:1] Definition with =code=.\n")
    (let* ((org-export-with-toc nil)
           (html (org-export-as 'html nil nil t nil)))
      (list (mapcar (lambda (re)
                      (not (null (string-match-p re html))))
                    '("<b>bold</b>"
                      "<i>italic</i>"
                      "<code>code</code>"
                      "<pre>verbatim</pre>"
                      "<u>underline</u>"
                      "href=\"https://example.org/path?q=1\""
                      "Example"
                      "tabular"
                      "<blockquote>"
                      "footnote"))
            (mapcar (lambda (tag)
                      (let ((c 0) (s 0))
                        (while (string-match (concat "<" tag) html s)
                          (setq s (match-end 0) c (1+ c)))
                        c))
                    '("b" "i" "code" "pre" "u" "td" "blockquote"))
            (replace-regexp-in-string
             "sec:org[[:alnum:]-]+" "sec:org-id"
             (replace-regexp-in-string "org[[:alnum:]-]\\{8,\\}" "orgHASH"
                                       html)))))))"##,
        expect,
    );
}
