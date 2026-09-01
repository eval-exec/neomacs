use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_markdown_export_toc_footnote_list_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (t t t t \"\\nTable of Contents\\n=================\\n\\n-   [Alpha](#org-id):tag:\\n    -   [Beta](#org-id)\\n\\n\\nTable of Contents\\n=================\\n\\n-   [Alpha](#org-id):tag:\\n    -   [Beta](#org-id)\\n\\n\\n<a id=\\\"org-id\\\"></a>\\n\\nAlpha     :tag:\\n=====\\n\\nParagraph with **bold**, *italic*, `code`, <sup><a id=\\\"fnr.one\\\" class=\\\"footref\\\" href=\\\"#fn.one\\\" role=\\\"doc-backlink\\\">1</a></sup>, and [site](https://example.org).\\n\\n-   [X] done item\\n-   [ ] open item\\n    1.  nested number\\n    2.  nested second\\n\\n> quoted **text**\\n\\n    (+ 1 2)\\n\\n\\n<a id=\\\"org-id\\\"></a>\\n\\nBeta\\n----\\n\\n<table border=\\\"2\\\" cellspacing=\\\"0\\\" cellpadding=\\\"6\\\" rules=\\\"groups\\\" frame=\\\"hsides\\\">\\n\\n\\n<colgroup>\\n<col  class=\\\"org-left\\\" />\\n\\n<col  class=\\\"org-right\\\" />\\n</colgroup>\\n<thead>\\n<tr>\\n<th scope=\\\"col\\\" class=\\\"org-left\\\">Name</th>\\n<th scope=\\\"col\\\" class=\\\"org-right\\\">Qty</th>\\n</tr>\\n</thead>\\n<tbody>\\n<tr>\\n<td class=\\\"org-left\\\">apple</td>\\n<td class=\\\"org-right\\\">2</td>\\n</tr>\\n</tbody>\\n</table>\\n\\n\\nFootnotes\\n=========\\n\\n<sup><a id=\\\"fn.1\\\" href=\\\"#fnr.1\\\">1</a></sup> Footnote with [GNU](https://gnu.org).\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-md)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Markdown Combo\n")
    (insert "#+OPTIONS: toc:2 num:nil tags:t\n")
    (insert "#+TOC: headlines 2\n")
    (insert "* Alpha :tag:\n")
    (insert "Paragraph with *bold*, /italic/, =code=, [fn:one], and [[https://example.org][site]].\n")
    (insert "- [X] done item\n")
    (insert "- [ ] open item\n")
    (insert "  1. nested number\n")
    (insert "  2. nested second\n\n")
    (insert "#+begin_quote\nquoted *text*\n#+end_quote\n")
    (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
    (insert "** Beta\n")
    (insert "| Name | Qty |\n|-\n| apple | 2 |\n")
    (insert "[fn:one] Footnote with [[https://gnu.org][GNU]].\n")
    (let* ((org-md-headline-style 'mixed)
           (org-export-with-broken-links t)
           (md (replace-regexp-in-string
                "org[[:alnum:]]+"
                "org-id"
                (org-export-as 'md nil nil t nil))))
      (list (not (null (string-match-p "Alpha" md)))
            (not (null (string-match-p "done item" md)))
            (not (null (string-match-p "<table" md)))
            (not (null (string-match-p "<sup>" md)))
            md))))"##,
        expect,
    );
}

#[test]
fn org_ascii_export_drawer_table_clock_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil nil nil t t \"1 TODO Alpha\\n════════════\\n\\n  Text with α, H_2O, x^2, and [Example].\\n  ━━━━━━━━━━━━━━━\\n   Item    Count \\n  ───────────────\\n   apples     12 \\n   pears       3 \\n  ━━━━━━━━━━━━━━━\\n  \t      Centered line\\n        Roses are red\\n          Indented verse\\n\\n\\n[Example] <https://example.org>\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-ascii)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: ASCII Combo\n")
    (insert "#+SUBTITLE: Export Details\n")
    (insert "* TODO Alpha\n")
    (insert "SCHEDULED: <2026-05-27 Wed 09:00-10:00>\n")
    (insert ":LOGBOOK:\n")
    (insert "CLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:15] =>  1:15\n")
    (insert ":END:\n")
    (insert "Text with \\alpha, H_2O, x^2, and [[https://example.org][Example]].\n")
    (insert "| Item | Count |\n|-\n| apples | 12 |\n| pears | 3 |\n")
    (insert "#+begin_center\nCentered line\n#+end_center\n")
    (insert "#+begin_verse\nRoses are red\n  Indented verse\n#+end_verse\n")
    (let* ((org-ascii-text-width 44)
           (org-ascii-charset 'utf-8)
           (org-ascii-links-to-notes t)
           (org-export-with-drawers '("LOGBOOK"))
           (org-export-with-toc nil)
           (text (org-export-as 'ascii nil nil t nil)))
      (list (not (null (string-match-p "ASCII Combo" text)))
            (not (null (string-match-p "SCHEDULED" text)))
            (not (null (string-match-p "CLOCK" text)))
            (not (null (string-match-p "α" text)))
            (not (null (string-match-p "apples" text)))
            text))))"##,
        expect,
    );
}

#[test]
fn org_icalendar_export_deadline_schedule_repeater_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t \"BEGIN:VCALENDAR\\r\\nVERSION:2.0\\r\\nX-WR-CALNAME:cal\\r\\nPRODID:-////Emacs with Org mode//EN\\r\\nX-WR-TIMEZONE:UTC\\r\\nX-WR-CALDESC:Calendar\\r\\nCALSCALE:GREGORIAN\\r\\nBEGIN:VEVENT\\r\\nDTSTAMP:<STAMP>\\r\\nUID:<uid>\\nDTSTART:20260527T090000\\r\\nDTEND:20260527T103000\\r\\nRRULE:FREQ=WEEKLY;INTERVAL=1\\r\\nSUMMARY:S: Timed meeting\\r\\nDESCRIPTION:DEADLINE: <2026-05-28 Thu 17:00> Body text with comma\\\\, semicol\\r\\n on\\\\; and\\\\nnewline marker.\\r\\nCATEGORIES:Work,meet,TODO\\r\\nBEGIN:VALARM\\r\\nACTION:DISPLAY\\r\\nDESCRIPTION:S: Timed meeting\\r\\nTRIGGER:-P0DT0H15M0S\\r\\nEND:VALARM\\r\\nEND:VEVENT\\r\\nBEGIN:VEVENT\\r\\nDTSTAMP:<STAMP>\\r\\nUID:<uid>\\nDTSTART:20260528T170000\\r\\nDTEND:20260528T190000\\r\\nSUMMARY:Timed meeting\\r\\nDESCRIPTION:DEADLINE: <2026-05-28 Thu 17:00> Body text with comma\\\\, semicol\\r\\n on\\\\; and\\\\nnewline marker.\\r\\nCATEGORIES:Work,meet,TODO\\r\\nBEGIN:VALARM\\r\\nACTION:DISPLAY\\r\\nDESCRIPTION:Timed meeting\\r\\nTRIGGER:-P0DT0H15M0S\\r\\nEND:VALARM\\r\\nEND:VEVENT\\r\\nBEGIN:VTODO\\r\\nUID:<uid>\\nDTSTAMP:<STAMP>\\r\\nDTSTART:20260527T090000\\r\\nRRULE:FREQ=WEEKLY;INTERVAL=1\\r\\nSUMMARY:Timed meeting\\r\\nDESCRIPTION:DEADLINE: <2026-05-28 Thu 17:00> Body text with comma\\\\, semicol\\r\\n on\\\\; and\\\\nnewline marker.\\r\\nCATEGORIES:Work,meet,TODO\\r\\nSEQUENCE:1\\r\\nPRIORITY:5\\r\\nSTATUS:NEEDS-ACTION\\r\\nEND:VTODO\\r\\nBEGIN:VEVENT\\r\\nDTSTAMP:<STAMP>\\r\\nUID:<uid>\\nDTSTART;VALUE=DATE:20260529\\r\\nDTEND;VALUE=DATE:20260530\\r\\nSUMMARY:Finished\\r\\nDESCRIPTION:DEADLINE: <2026-05-29 Fri>\\r\\nCATEGORIES:Work,done,DONE\\r\\nEND:VEVENT\\r\\nBEGIN:VTODO\\r\\nUID:<uid>\\nDTSTAMP:<STAMP>\\r\\nSUMMARY:Finished\\r\\nDESCRIPTION:DEADLINE: <2026-05-29 Fri>\\r\\nCATEGORIES:Work,done,DONE\\r\\nSEQUENCE:1\\r\\nPRIORITY:5\\r\\nSTATUS:COMPLETED\\r\\nEND:VTODO\\r\\nBEGIN:VEVENT\\r\\nDTSTAMP:<STAMP>\\r\\nUID:<uid>\\nDTSTART:20260601T130000\\r\\nDTEND:20260601T140000\\r\\nSUMMARY:Event only\\r\\nDESCRIPTION:<2026-06-01 Mon 13:00-14:00>\\r\\nCATEGORIES:Work,event\\r\\nBEGIN:VALARM\\r\\nACTION:DISPLAY\\r\\nDESCRIPTION:Event only\\r\\nTRIGGER:-P0DT0H15M0S\\r\\nEND:VALARM\\r\\nEND:VEVENT\\r\\nEND:VCALENDAR\\r\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'ox-icalendar)
  (let* ((root (make-temp-file "org-ical" t))
         (file (expand-file-name "cal.org" root))
         (org-icalendar-store-UID t)
         (org-icalendar-use-deadline '(event-if-todo todo-due event-if-not-todo))
         (org-icalendar-use-scheduled '(todo-start event-if-todo))
         (org-icalendar-include-todo 'all)
         (org-icalendar-categories '(category local-tags todo-state))
         (org-icalendar-alarm-time 15)
         (org-icalendar-force-alarm t)
         (org-icalendar-timezone "UTC"))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "#+TITLE: Calendar\n")
            (insert "#+CATEGORY: Work\n")
            (insert "* TODO Timed meeting :meet:\n")
            (insert "SCHEDULED: <2026-05-27 Wed 09:00-10:30 +1w>\n")
            (insert "DEADLINE: <2026-05-28 Thu 17:00>\n")
            (insert "Body text with comma, semicolon; and newline marker.\n")
            (insert "* DONE Finished :done:\n")
            (insert "CLOSED: [2026-05-26 Tue 18:00]\n")
            (insert "DEADLINE: <2026-05-29 Fri>\n")
            (insert "* Event only :event:\n")
            (insert "<2026-06-01 Mon 13:00-14:00>\n"))
          (with-current-buffer (find-file-noselect file)
            (org-mode)
            (let* ((ics-file (org-icalendar-export-to-ics nil nil nil))
                   (ics (with-temp-buffer
                          (insert-file-contents ics-file)
                          (buffer-string)))
                   (normalized
                    (replace-regexp-in-string
                     "PRODID:-//[^/\n]+//"
                     "PRODID:-//user//"
                     (replace-regexp-in-string
                      "DTSTAMP:[0-9TZ]+"
                      "DTSTAMP:<stamp>"
                      (replace-regexp-in-string
                       "UID:[^\n]+"
                       "UID:<uid>"
                       ics)))))
              (list (not (null (string-match-p "BEGIN:VCALENDAR" ics)))
                    (not (null (string-match-p "BEGIN:VEVENT" ics)))
                    (not (null (string-match-p "BEGIN:VTODO" ics)))
                    (not (null (string-match-p "RRULE" ics)))
                    (not (null (string-match-p "VALARM" ics)))
                    normalized))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_icalendar_combine_agenda_files_filter_hook_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"combined.ics\") t t t t t t t \"BEGIN:VCALENDAR\\r\\nVERSION:2.0\\r\\nX-WR-CALNAME:Combined Name\\r\\nPRODID:-////Emacs with Org mode//EN\\r\\nX-WR-TIMEZONE:UTC\\r\\nX-WR-CALDESC:Combined Description\\r\\nX-PUBLISHED-TTL:PT2H\\r\\nCALSCALE:GREGORIAN\\r\\nBEGIN:VEVENT\\r\\nDTSTAMP:<STAMP>\\r\\nUID:<uid>\\nDTSTART:20260527T100000\\r\\nDTEND:20260527T120000\\r\\nSUMMARY:S: Task one\\r\\nDESCRIPTION:DEADLINE: <2026-05-28 Thu>\\r\\nCATEGORIES:work,Alpha\\r\\nEND:VEVENT\\r\\nBEGIN:VEVENT\\r\\nDTSTAMP:<STAMP>\\r\\nUID:<uid>\\nDTSTART;VALUE=DATE:20260528\\r\\nDTEND;VALUE=DATE:20260529\\r\\nSUMMARY:Task one\\r\\nDESCRIPTION:DEADLINE: <2026-05-28 Thu>\\r\\nCATEGORIES:work,Alpha\\r\\nEND:VEVENT\\r\\nBEGIN:VTODO\\r\\nUID:<uid>\\nDTSTAMP:<STAMP>\\r\\nDTSTART:20260527T100000\\r\\nSUMMARY:Task one\\r\\nDESCRIPTION:DEADLINE: <2026-05-28 Thu>\\r\\nCATEGORIES:work,Alpha\\r\\nSEQUENCE:1\\r\\nPRIORITY:5\\r\\nSTATUS:NEEDS-ACTION\\r\\nEND:VTODO\\r\\nBEGIN:VEVENT\\r\\nDTSTAMP:<STAMP>\\r\\nUID:<uid>\\nDTSTART:20260601T130000\\r\\nDTEND:20260601T140000\\r\\nSUMMARY:Event two\\r\\nDESCRIPTION:[2026-06-01 Mon 13:00-14:00]\\r\\nCATEGORIES:event,Beta\\r\\nEND:VEVENT\\r\\nBEGIN:VTODO\\r\\nUID:<uid>\\nDTSTAMP:<STAMP>\\r\\nDUE:20260602T090000\\r\\nSUMMARY:Task two\\r\\nCATEGORIES:Beta\\r\\nSEQUENCE:1\\r\\nPRIORITY:5\\r\\nSTATUS:NEEDS-ACTION\\r\\nEND:VTODO\\r\\nEND:VCALENDAR\\r\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-icalendar)
  (let* ((root (make-temp-file "org-ical-combine" t))
         (one (expand-file-name "one.org" root))
         (two (expand-file-name "two.org" root))
         (combined (expand-file-name "combined.ics" root))
         (org-agenda-files (list one two))
         (org-icalendar-combined-agenda-file combined)
         (org-icalendar-combined-name "Combined Name")
         (org-icalendar-combined-description "Combined Description")
         (org-icalendar-ttl "PT2H")
         (org-icalendar-timezone "UTC")
         (org-icalendar-include-todo 'all)
         (org-icalendar-use-scheduled '(todo-start event-if-todo))
         (org-icalendar-use-deadline '(todo-due event-if-not-todo))
         (org-icalendar-with-timestamps t)
         (org-icalendar-exclude-tags '("noexport"))
         (saved nil)
         (org-icalendar-after-save-hook
          (list (lambda (file)
                  (push (file-relative-name file root) saved)))))
    (unwind-protect
        (progn
          (with-temp-file one
            (insert "#+TITLE: One\n#+CATEGORY: Alpha\n")
            (insert "* TODO Task one :work:\n")
            (insert "SCHEDULED: <2026-05-27 Wed 10:00>\n")
            (insert "DEADLINE: <2026-05-28 Thu>\n")
            (insert "* Hidden :noexport:\n<2026-05-30 Sat 12:00>\n"))
          (with-temp-file two
            (insert "#+TITLE: Two\n#+CATEGORY: Beta\n")
            (insert "* Event two :event:\n")
            (insert "[2026-06-01 Mon 13:00-14:00]\n")
            (insert "* TODO Task two\n")
            (insert "DEADLINE: <2026-06-02 Tue 09:00>\n"))
          (org-icalendar-combine-agenda-files nil)
          (let* ((ics (with-temp-buffer
                        (insert-file-contents combined)
                        (buffer-string)))
                 (normalized
                  (replace-regexp-in-string
                   "PRODID:-//[^/\n]+//"
                   "PRODID:-//user//"
                   (replace-regexp-in-string
                    "DTSTAMP:[0-9TZ]+"
                    "DTSTAMP:<stamp>"
                    (replace-regexp-in-string
                     "UID:[^\n]+"
                     "UID:<uid>"
                     ics)))))
            (list (sort saved #'string<)
                  (not (null (string-match-p "X-WR-CALNAME:Combined Name" ics)))
                  (not (null (string-match-p "X-WR-CALDESC:Combined Description" ics)))
                  (not (null (string-match-p "X-PUBLISHED-TTL:PT2H" ics)))
                  (not (null (string-match-p "Task one" ics)))
                  (not (null (string-match-p "Task two" ics)))
                  (not (null (string-match-p "Event two" ics)))
                  (null (string-match-p "Hidden" ics))
                  normalized)))
      (dolist (file (list one two))
        (when (get-file-buffer file) (kill-buffer (get-file-buffer file))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_texinfo_export_menu_definition_table_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t t nil t \"@node First Node\\n@chapter First Node\\n\\nParagraph with @strong{bold}@comma{} @emph{italic}@comma{} @samp{code} and @uref{https://example.org, link}.\\n@table @asis\\n@item Function\\nDescribe function entry\\n@item Variable\\nDescribe variable entry\\n@end table\\n\\n@float Table\\n@multitable {aaaa} {aaaaa}\\n@headitem Name\\n@tab Count\\n@item A\\n@tab 1\\n@item B\\n@tab 2\\n@end multitable\\n@caption{Values}\\n@end float\\n@lisp\\n1  (+ 1 2)\\n@end lisp\\n\\n@menu\\n* Child Node::\\n@end menu\\n\\n@node Child Node\\n@section Child Node\\n\\n@example\\nliteral @@ braces @{ @}\\n@end example\\n\\n@node Second/Node\\n@chapter Second/Node\\n\\nTrailing paragraph with α and H@math{_2O}.\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-texinfo)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Texinfo Combo\n")
    (insert "#+AUTHOR: Ada\n")
    (insert "#+OPTIONS: toc:nil num:t\n")
    (insert "* First Node\n")
    (insert "Paragraph with *bold*, /italic/, =code= and [[https://example.org][link]].\n")
    (insert "- Function :: Describe function entry\n")
    (insert "- Variable :: Describe variable entry\n\n")
    (insert "#+CAPTION: Values\n")
    (insert "| Name | Count |\n|------+-------|\n| A | 1 |\n| B | 2 |\n")
    (insert "#+begin_src emacs-lisp -n\n(+ 1 2)\n#+end_src\n")
    (insert "** Child Node\n")
    (insert "#+begin_example\nliteral @ braces { }\n#+end_example\n")
    (insert "* Second/Node\n")
    (insert "Trailing paragraph with \\alpha and H_2O.\n")
    (let* ((org-export-with-toc nil)
           (texi (org-export-as 'texinfo nil nil t nil))
           (normalized
            (replace-regexp-in-string "org[[:alnum:]]+" "org-id" texi)))
      (list (not (null (string-match-p "@node First Node" texi)))
            (not (null (string-match-p "@menu" texi)))
            (not (null (string-match-p "@table" texi)))
            (not (null (string-match-p "@item Function" texi)))
            (not (null (string-match-p "@multitable" texi)))
            (not (null (string-match-p "@example" texi)))
            (not (null (string-match-p "@code" texi)))
            (not (null (string-match-p "Second/Node" texi)))
            normalized))))"##,
        expect,
    );
}

#[test]
fn org_man_export_sections_lists_tables_refs_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil nil nil nil t t t nil \".SH \\\"NAME\\\"\\n.PP\\nneomacs-test - exercise Org man export\\n.SH \\\"SYNOPSIS\\\"\\n.RS\\n.nf\\n\\\\fCneomacs-test --flag=value FILE\\n\\\\fP\\n.fi\\n.RE\\n.SH \\\"DESCRIPTION\\\"\\n.PP\\nParagraph with \\\\fBbold\\\\fP, \\\\fIitalic\\\\fP, \\\\fIcode\\\\fP, \\\\fCverbatim\\\\fP, -- dash, and https://example.org/path?a=1&b=2 \\\\fBat\\\\fP \\\\fIsite\\\\fP.\\n.TP\\n\\\\fBoption-a\\\\fP\\nfirst option with H\\\\d\\\\s-22O\\\\s+2\\\\u and α\\n.TP\\n\\\\fBoption-b\\\\fP\\nsecond option with custom-id:details \\\\fBat\\\\fP \\\\fIdetails\\\\fP\\n.TS\\n center,box;\\n\\nr l .\\nCode\tMeaning\\n_\\n0\tok\\n2\tfailed\\n.TE\\n.TB \\\"\\\\fRExit status\\\\fP\\\"\\n.SS \\\"Details\\\"\\n.PP\\nTarget paragraph with \\\\fIorg-id\\\\fP and fuzzy:radio-target \\\\fBat\\\\fP \\\\fIradio\\\\fP.\\n.RS\\n.nf\\nliteral .SH should be escaped\\nliteral backslash \\\\e\\\\e\\n\\n.fi\\n.RE\\n.SH \\\"SEE ALSO\\\"\\n.PP\\nfuzzy:man:emacs(1) \\\\fBat\\\\fP \\\\fIemacs(1)\\\\fP and https://gnu.org \\\\fBat\\\\fP \\\\fIGNU\\\\fP.\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-man)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: neomacs-test\n")
    (insert "#+SUBTITLE: Org Man Combo\n")
    (insert "#+AUTHOR: Ada\n")
    (insert "#+DATE: 2026-05-27\n")
    (insert "#+OPTIONS: toc:nil num:nil author:t\n")
    (insert "* NAME\n")
    (insert "neomacs-test - exercise Org man export [fn:one]\n")
    (insert "* SYNOPSIS\n")
    (insert "#+begin_src sh\nneomacs-test --flag=value FILE\n#+end_src\n")
    (insert "* DESCRIPTION\n")
    (insert "Paragraph with *bold*, /italic/, =code=, ~verbatim~, -- dash, and [[https://example.org/path?a=1&b=2][site]].\n")
    (insert "- option-a :: first option with H_2O and \\alpha\n")
    (insert "- option-b :: second option with [[#details][details]]\n\n")
    (insert "#+CAPTION: Exit status\n")
    (insert "| Code | Meaning |\n|------+---------|\n| 0 | ok |\n| 2 | failed |\n")
    (insert "** Details\n")
    (insert "#+NAME: details\n")
    (insert "Target paragraph with <<radio-target>> and [[radio-target][radio]].\n")
    (insert "#+begin_example\nliteral .SH should be escaped\nliteral backslash \\\\\n#+end_example\n")
    (insert "* SEE ALSO\n")
    (insert "[[man:emacs(1)][emacs(1)]] and [[https://gnu.org][GNU]].\n")
    (insert "[fn:one] Footnote text with /markup/ and [[https://example.org/fn][url]].\n")
    (let* ((org-export-with-toc nil)
           (org-export-with-broken-links t)
           (man (org-export-as 'man nil nil t nil))
           (normalized
            (replace-regexp-in-string
             "[ \t]+$" ""
             (replace-regexp-in-string
              "org[[:alnum:]]+"
              "org-id"
              man))))
      (list (not (null (string-match-p "^\\.TH" man)))
            (not (null (string-match-p "^\\.SH NAME" man)))
            (not (null (string-match-p "^\\.SH SYNOPSIS" man)))
            (not (null (string-match-p "^\\.SS Details" man)))
            (not (null (string-match-p "option-a" man)))
            (not (null (string-match-p "Exit status" man)))
            (not (null (string-match-p "emacs(1)" man)))
            (not (null (string-match-p "Footnote" man)))
            normalized))))"##,
        expect,
    );
}

#[test]
fn org_beamer_export_columns_blocks_againframe_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil nil nil t t t t nil t nil t t \"\\\\section{Section One}\\n\\\\label{sec:org-id}\\n\\\\begin{orgframe}[fragile,fragile,label=mainframe]{Main Frame}\\n \\\\frametitle{Explicit Title}\\n\\\\begin{itemize}\\n\\\\item<1-> First item with \\\\alert{bold} text\\n\\\\item<2-> Second item with \\\\href{https://example.org}{link}\\n\\\\end{itemize}\\n\\\\begin{columns}[t]\\n\\\\begin{column}{0.45\\\\columnwidth}\\n\\\\begin{block}<2->{Left Block}\\nLeft body with \\\\texttt{code} and \\\\(\\\\alpha\\\\).\\n\\\\end{block}\\n\\\\end{column}\\n\\\\begin{column}[{Custom Alert}]{0.45\\\\columnwidth}\\n\\\\begin{alertblock}{Right Alert}\\n\\\\begin{verbatim}\\nliteral \\\\begin{frame}\\n\\\\end{verbatim}\\n\\\\end{alertblock}\\n\\\\end{column}\\n\\\\end{columns}\\n\\\\note<2>{Speaker Note\\nNote body with \\\\emph{italic}.}\\n\\\\end{orgframe}\\n\\\\againframe<3>{mainframe}\\n\\\\appendix\\n\\\\begin{frame}[label={sec:org-id},fragile]{Backup Frame}\\nBackup content with H\\\\textsubscript{2O}.\\n\\\\end{frame}\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-beamer)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Beamer Deep Combo\n")
    (insert "#+SUBTITLE: Columns and overlays\n")
    (insert "#+AUTHOR: Ada\n")
    (insert "#+DATE: 2026-05-27\n")
    (insert "#+OPTIONS: H:2 toc:nil num:t\n")
    (insert "#+BEAMER_THEME: Madrid\n")
    (insert "#+BEAMER_COLOR_THEME: seahorse\n")
    (insert "* Section One\n")
    (insert ":PROPERTIES:\n:CUSTOM_ID: sec-one\n:END:\n")
    (insert "** Main Frame\n")
    (insert ":PROPERTIES:\n:CUSTOM_ID: main-frame\n:BEAMER_OPT: fragile,label=mainframe\n:END:\n")
    (insert "#+BEAMER: \\frametitle{Explicit Title}\n")
    (insert "- @@beamer:<1->@@ First item with *bold* text\n")
    (insert "- @@beamer:<2->@@ Second item with [[https://example.org][link]]\n")
    (insert "*** Columns\n")
    (insert ":PROPERTIES:\n:BEAMER_ENV: columns\n:BEAMER_OPT: [t]\n:END:\n")
    (insert "**** Left Block\n")
    (insert ":PROPERTIES:\n:BEAMER_COL: 0.45\n:BEAMER_ENV: block\n:BEAMER_ACT: <2->\n:END:\n")
    (insert "Left body with =code= and \\alpha.\n")
    (insert "**** Right Alert\n")
    (insert ":PROPERTIES:\n:BEAMER_COL: 0.45\n:BEAMER_ENV: alertblock\n:BEAMER_OPT: {Custom Alert}\n:END:\n")
    (insert "#+begin_example\nliteral \\begin{frame}\n#+end_example\n")
    (insert "*** Speaker Note\n")
    (insert ":PROPERTIES:\n:BEAMER_ENV: note\n:BEAMER_ACT: <2>\n:END:\n")
    (insert "Note body with /italic/.\n")
    (insert "** Resume Frame\n")
    (insert ":PROPERTIES:\n:BEAMER_ENV: againframe\n:BEAMER_REF: #main-frame\n:BEAMER_ACT: <3>\n:END:\n")
    (insert "* Appendix\n")
    (insert ":PROPERTIES:\n:BEAMER_ENV: appendix\n:END:\n")
    (insert "** Backup Frame\nBackup content with H_2O.\n")
    (let* ((org-export-with-toc nil)
           (org-beamer-frame-default-options "fragile")
           (latex (org-export-as 'beamer nil nil t nil))
           (normalized
            (replace-regexp-in-string
             "sec:org[[:alnum:]-]+"
             "sec:org-id"
             latex)))
      (list (not (null (string-match-p "\\\\usetheme{Madrid}" latex)))
            (not (null (string-match-p "\\\\usecolortheme{seahorse}" latex)))
            (not (null (string-match-p "\\\\begin{frame}\\[fragile,label=mainframe\\]" latex)))
            (not (null (string-match-p "\\\\frametitle{Explicit Title}" latex)))
            (not (null (string-match-p "\\\\begin{columns}\\[t\\]" latex)))
            (not (null (string-match-p "\\\\begin{column}{0.45\\\\columnwidth}" latex)))
            (not (null (string-match-p "\\\\begin{block}<2->" latex)))
            (not (null (string-match-p "\\\\begin{alertblock}{Custom Alert}" latex)))
            (not (null (string-match-p "\\\\note<2>" latex)))
             (not (null (string-match-p "\\\\againframe<3>{sec-one-main-frame}" latex)))
             (not (null (string-match-p "\\\\appendix" latex)))
             (not (null (string-match-p "\\\\begin{verbatim}" latex)))
             normalized))))"##,
        expect,
    );
}

#[test]
fn org_html_export_detailed_structure_link_image_table_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (user-error \"Org export aborted.  Unable to resolve link: \\\"No match for fuzzy expression: *Target\\\"\\nSee ‘org-export-with-broken-links’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Detailed Export\n")
    (insert "#+AUTHOR: Test Author\n")
    (insert "#+OPTIONS: toc:nil num:nil\n\n")
    (insert "* Section One :tag1:tag2:\n")
    (insert "Paragraph with *bold*, /italic/, _underline_, =code=, and ~verbatim~.\n\n")
    (insert "A [[https://example.org][Example Link]] and a [[file:notes.org::*Target][file link]].\n\n")
    (insert "#+NAME: tbl\n")
    (insert "| Name | Value |\n")
    (insert "|------+-------|\n")
    (insert "| Alpha | 10 |\n")
    (insert "| Beta | 20 |\n\n")
    (insert "#+CAPTION: My Table\n")
    (insert "#+ATTR_HTML: :border 2 :class custom\n\n")
    (insert "#+begin_quote\n")
    (insert "A blockquote with *emphasis*.\n")
    (insert "#+end_quote\n\n")
    (insert "#+begin_src emacs-lisp\n")
    (insert "(+ 1 2)\n")
    (insert "#+end_src\n\n")
    (insert "** Subsection\n")
    (insert ":PROPERTIES:\n:CUSTOM_ID: my-id\n:END:\n")
    (insert "Text with footnote[fn:1].\n\n")
    (insert "[fn:1] Footnote body with =code=.\n")
    (let* ((org-export-with-toc nil)
           (org-export-show-temporary-export-buffer nil)
           (html (org-export-as 'html nil nil t nil))
           (normalized
            (replace-regexp-in-string
             "sec:org[[:alnum:]-]+" "sec:org-id"
             (replace-regexp-in-string
              "org[[:alnum:]-]\\{8,\\}" "orgHASH"
              html))))
      (list
       (replace-regexp-in-string
        "<[^>]+>" ""
        (or (and (string-match "<title>\\([^<]+\\)</title>" html)
                 (match-string 1 html))
            "no-title"))
       (mapcar (lambda (tag)
                 (let ((s 0) (c 0))
                   (while (string-match (concat "<" tag) html s)
                     (setq s (match-end 0) c (1+ c)))
                   c))
               '("h1" "h2" "h3" "blockquote" "pre" "table"))
       (and (string-match "href=\"\\([^\"]+\\)\"" html)
            (match-string 1 html))
       (list (not (null (string-match "<b>bold</b>" html)))
             (not (null (string-match "<i>italic</i>" html)))
             (not (null (string-match "<code>code</code>" html)))
             (not (null (string-match "<pre>" html))))
       (let ((td-count 0) (s 0))
         (while (string-match "<td" html s)
           (setq s (match-end 0) td-count (1+ td-count)))
         td-count)
        normalized)))))"##,
        expect,
    );
}

#[test]
fn org_multi_backend_export_structure_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 59 58)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (require 'ox-latex)
  (require 'ox-ascii)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Multi Backend\n")
    (insert "#+AUTHOR: Tester\n\n")
    (insert "* Section One :tag1:\n")
    (insert "Paragraph with *bold*, /italic/, =code=, and ~verbatim~.\n\n")
    (insert "- [X] Done item\n")
    (insert "- [ ] Todo item\n\n")
    (insert "| Name | Val |\n|------+-----|\n| A | 1 |\n| B | 2 |\n\n")
    (insert "#+begin_quote\nQuoted text.\n#+end_quote\n\n")
    (insert "** Subsection\n:PROPERTIES:\n:CUSTOM_ID: my-id\n:END:\n")
    (insert "See [[https://example.org][Example]].\n\n")
    (insert "[fn:1] Footnote body.\n")
    (let* ((org-export-with-toc nil)
           (html (org-export-as 'html nil nil t nil))
           (latex (org-export-as 'latex nil nil t nil))
           (ascii (let ((org-ascii-charset 'utf-8))
                    (org-export-as 'ascii nil nil t nil))))
      ;; Extract element counts from each backend
      (let ((count-tag (lambda (s re)
                         (let ((c 0) (p 0))
                           (while (string-match re s p)
                             (setq p (match-end 0) c (1+ c)))
                           c))))
        (list
         ;; HTML element counts
         (list (funcall count-tag html "<h[1-3]")
               (funcall count-tag html "<li")
               (funcall count-tag html "<td")
               (funcall count-tag html "<blockquote")
               (funcall count-tag html "<pre")
               (funcall count-tag html "<code")
               (funcall count-tag html "<b>")
               (funcall count-tag html "<i>"))
         ;; LaTeX element counts
         (list (funcall count-tag latex "\\\\section")
               (funcall count-tag latex "\\\\subsection")
               (funcall count-tag latex "\\\\textbf")
               (funcall count-tag latex "\\\\textit")
               (funcall count-tag latex "\\\\texttt")
               (funcall count-tag latex "\\\\begin{itemize}")
               (funcall count-tag latex "\\\\begin{quote}")
               (funcall count-tag latex "tabular"))
         ;; ASCII check patterns
         (list (not (null (string-match-p "Multi Backend" ascii)))
               (not (null (string-match-p "Section One" ascii)))
               (not (null (string-match-p "Example" ascii)))
               (not (null (string-match-p "Footnote" ascii))))
         ;; Full outputs (normalized)
         (replace-regexp-in-string
          "sec:org[[:alnum:]-]+" "sec:org-id"
          (replace-regexp-in-string "org[[:alnum:]-]\\{8,\\}" "orgHASH"
                                    html))
         (replace-regexp-in-string
          "sec:org[[:alnum:]-]+" "sec:org-id" latex)))))))"##,
        expect,
    );
}
