use expect_test::expect;

use super::ParityBatchCase;

/// The documented main workflow: a student mails a zipped submission, the
/// grader triggers the mu4e attachment action, answers the group and week
/// prompts, and abgaben files the attachment below `abgaben-root-folder`,
/// unpacks it with `unzip` and links it from the org outline.
fn capture_submission_files_unpacks_and_links_a_zipped_submission() -> ParityBatchCase {
    ParityBatchCase::value(
        "capture_submission_files_unpacks_and_links_a_zipped_submission",
        r##"(let* ((abgaben-root-folder
        (expand-file-name "Abgaben SS25/" abgaben-test-root))
       (abgaben-org-file
        (abgaben-test-org-file
         "notizen/abgaben.org"
         (concat "* Kurs Notizen\n"
                 "Nicht anfassen.\n"
                 "* Abgaben\n"
                 "** gruppe-di\n"
                 "*** 03\n"
                 "**** [[file:alt.pdf][alt.pdf]] Email: [[mu4e:msgid:alt@uni.example][Alte Abgabe]]\n"
                 "** gruppe-do\n")))
       (abgaben-heading "Abgaben")
       (abgaben-all-groups '("gruppe-di" "gruppe-do"))
       (abgaben--curr-group "gruppe-do")
       (abgaben--curr-week "01")
       (abgaben-test-answers '("gruppe-di" "03"))
       (archive
        (abgaben-test-write-file
         (expand-file-name "mailstore/Übung 03 – Lösung.zip" abgaben-test-root)
         "entry loesung.tex\nentry Lösung.pdf\n"))
       (message-plist
        (list :message-id "CAF-42@uni.example"
              :subject "Übung 03 – Lösung"
              :attachments
              (list (list :index 1 :name "hinweis.txt"
                          :mime-type "text/plain"
                          :source (abgaben-test-write-file
                                   (expand-file-name "mailstore/hinweis.txt"
                                                     abgaben-test-root)
                                   "bitte nicht speichern\n"))
                    (list :index 2 :name "Übung 03 – Lösung.zip"
                          :mime-type "application/zip"
                          :source archive))))
       (buffer nil))
  (abgaben-test-install-unzip)
  (unwind-protect
      (progn
        (abgaben-capture-submission message-plist 2)
        (setq buffer (current-buffer))
        (save-buffer)
        (list
         (abgaben-test-events)
         (abgaben-test-commands)
         (list abgaben--curr-group abgaben--curr-week)
         (list (buffer-name) major-mode (line-number-at-pos)
               (buffer-substring-no-properties (point) (line-end-position))
               (buffer-modified-p))
         (abgaben-test-tree abgaben-root-folder)
         (abgaben-test-contents
          (expand-file-name "gruppe-di/03/Übung 03 – Lösung/Lösung.pdf"
                            abgaben-root-folder))
         (abgaben-test-contents abgaben-org-file)))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))"##,
        expect![[
            r#"OK (((completing-read 8 "Which group? " ("gruppe-di" "gruppe-do") t nil "gruppe-do" "gruppe-di") (completing-read 8 "Which week? " ("01" "02" "03" "04" "05" "06" "07" "08" "09" "10" "11" "12" "13" "14") t nil "01" "03") (get-attach 2 "Übung 03 – Lösung.zip") (save-attachment "Übung 03 – Lösung.zip" "Abgaben SS25/gruppe-di/03/Übung 03 – Lösung.zip")) ("unzip Übung 03 – Lösung.zip -d Übung 03 – Lösung") ("gruppe-di" "03") ("abgaben.org" org-mode 6 "" nil) ("gruppe-di/03/Übung 03 – Lösung.zip" "gruppe-di/03/Übung 03 – Lösung/Lösung.pdf" "gruppe-di/03/Übung 03 – Lösung/loesung.tex") "unpacked Lösung.pdf\n" "* Kurs Notizen\nNicht anfassen.\n* Abgaben\n** gruppe-di\n*** 03\n**** [[file:[ORACLE-SANDBOX]/Abgaben SS25/gruppe-di/03/Übung 03 – Lösung][Übung 03 – Lösung.zip]] Email: [[mu4e:msgid:CAF-42@uni.example][Übung 03 – Lösung]]\n**** [[file:alt.pdf][alt.pdf]] Email: [[mu4e:msgid:alt@uni.example][Alte Abgabe]]\n** gruppe-do\n")"#
        ]],
    )
}

fn capture_submission_creates_the_missing_week_and_unpacks_a_real_tarball() -> ParityBatchCase {
    ParityBatchCase::value(
        "capture_submission_creates_the_missing_week_and_unpacks_a_real_tarball",
        r##"(let* ((abgaben-root-folder
        (expand-file-name "Abgaben SS25/" abgaben-test-root))
       (abgaben-org-file
        (abgaben-test-org-file
         "notizen/abgaben.org"
         (concat "* Abgaben\n"
                 "** gruppe-do\n"
                 "*** 02\n"
                 "**** [[file:frueher][frueher.tar.gz]] Email: [[mu4e:msgid:x@y][Frueher]]\n"
                 "Kommentar zur zweiten Woche.\n")))
       (abgaben-heading "Abgaben")
       (abgaben-all-groups '("gruppe-di" "gruppe-do"))
       (abgaben--curr-group "gruppe-di")
       (abgaben--curr-week "02")
       (abgaben-test-answers '("gruppe-do" "07"))
       (archive
        (abgaben-test-make-tarball
         (expand-file-name "mailstore/Aufgabe 07.tar.gz" abgaben-test-root)
         '(("bericht.tex" . "\\section{Lösung}\n")
           ("daten/messung.csv" . "zeit,wert\n1,42\n"))))
       (message-plist
        (list :attachments
              (list (list :index 1 :name "Aufgabe 07.tar.gz"
                          :mime-type "application/gzip"
                          :source archive))))
       (buffer nil))
  (unwind-protect
      (progn
        (abgaben-capture-submission message-plist 1)
        (setq buffer (current-buffer))
        (save-buffer)
        (list
         (abgaben-test-events)
         (list abgaben--curr-group abgaben--curr-week)
         (abgaben-test-tree abgaben-root-folder)
         (abgaben-test-contents
          (expand-file-name "gruppe-do/07/Aufgabe 07/daten/messung.csv"
                            abgaben-root-folder))
         (abgaben-test-contents
          (expand-file-name "gruppe-do/07/Aufgabe 07/bericht.tex"
                            abgaben-root-folder))
         (abgaben-test-contents abgaben-org-file)))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))"##,
        expect![[
            r#"OK (((completing-read 8 "Which group? " ("gruppe-di" "gruppe-do") t nil "gruppe-di" "gruppe-do") (completing-read 8 "Which week? " ("01" "02" "03" "04" "05" "06" "07" "08" "09" "10" "11" "12" "13" "14") t nil "02" "07") (get-attach 1 "Aufgabe 07.tar.gz") (save-attachment "Aufgabe 07.tar.gz" "Abgaben SS25/gruppe-do/07/Aufgabe 07.tar.gz")) ("gruppe-do" "07") ("gruppe-do/07/Aufgabe 07.tar.gz" "gruppe-do/07/Aufgabe 07/bericht.tex" "gruppe-do/07/Aufgabe 07/daten/messung.csv") "zeit,wert\n1,42\n" "\\section{Lösung}\n" "* Abgaben\n** gruppe-do\n*** 07\n**** [[file:[ORACLE-SANDBOX]/Abgaben SS25/gruppe-do/07/Aufgabe 07][Aufgabe 07.tar.gz]] Email: [[mu4e:msgid:<none>][<none>]]\n*** 02\n**** [[file:frueher][frueher.tar.gz]] Email: [[mu4e:msgid:x@y][Frueher]]\nKommentar zur zweiten Woche.\n")"#
        ]],
    )
    .fresh_process()
}

fn capture_submission_without_the_group_heading_signals_search_failed_after_saving()
-> ParityBatchCase {
    ParityBatchCase::value(
        "capture_submission_without_the_group_heading_signals_search_failed_after_saving",
        r##"(let* ((abgaben-root-folder
        (expand-file-name "Abgaben SS25/" abgaben-test-root))
       (abgaben-org-file
        (abgaben-test-org-file
         "notizen/abgaben.org"
         (concat "* Abgaben\n"
                 "** gruppe-di\n"
                 "*** 05\n")))
       (abgaben-heading "Abgaben")
       (abgaben-all-groups '("gruppe-di" "gruppe-do"))
       (abgaben--curr-group "gruppe-di")
       (abgaben--curr-week "05")
       (abgaben-test-answers '("gruppe-do" "05"))
       (message-plist
        (list :message-id "spät@uni.example"
              :subject "Nachreichung"
              :attachments
              (list (list :index 1 :name "loesung.pdf"
                          :mime-type "application/pdf"
                          :source (abgaben-test-write-file
                                   (expand-file-name "mailstore/loesung.pdf"
                                                     abgaben-test-root)
                                   "%PDF-1.7 nachgereicht\n")))))
       (outcome nil))
  (unwind-protect
      (progn
        (setq outcome
              (condition-case error
                  (progn (abgaben-capture-submission message-plist 1)
                         'unexpectedly-succeeded)
                (error (list (car error) (cdr error)))))
        (list
         outcome
         (abgaben-test-events)
         (list abgaben--curr-group abgaben--curr-week)
         (list (buffer-name) major-mode (buffer-modified-p) (point))
         (abgaben-test-tree abgaben-root-folder)
         (abgaben-test-contents
          (expand-file-name "gruppe-do/05/loesung.pdf" abgaben-root-folder))
         (abgaben-test-contents abgaben-org-file)))
    (let ((buffer (get-file-buffer abgaben-org-file)))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer (set-buffer-modified-p nil))
        (kill-buffer buffer)))))"##,
        expect![[
            r#"OK ((search-failed ("** gruppe-do")) ((completing-read 8 "Which group? " ("gruppe-di" "gruppe-do") t nil "gruppe-di" "gruppe-do") (completing-read 8 "Which week? " ("01" "02" "03" "04" "05" "06" "07" "08" "09" "10" "11" "12" "13" "14") t nil "05" "05") (get-attach 1 "loesung.pdf") (save-attachment "loesung.pdf" "Abgaben SS25/gruppe-do/05/loesung.pdf")) ("gruppe-do" "05") ("abgaben.org" org-mode nil 1) ("gruppe-do/05/loesung.pdf") "%PDF-1.7 nachgereicht\n" "* Abgaben\n** gruppe-di\n*** 05\n")"#
        ]],
    )
    .fresh_process()
}

fn export_pdf_annot_to_org_sorts_annotations_filters_links_and_totals_points() -> ParityBatchCase {
    ParityBatchCase::value(
        "export_pdf_annot_to_org_sorts_annotations_filters_links_and_totals_points",
        r##"(let* ((abgaben-points-re "Aufgabe [0-9.]*: ?\\([0-9.]*\\)/\\([0-9.]*\\)")
       (abgaben-points-heading "Deine Punkte")
       (abgaben-points-overall "Gesamt")
       (pdf-file
        (expand-file-name "Abgaben SS25/gruppe-di/03/Aufgabe 03 [final] Lösung.pdf"
                          abgaben-test-root))
       (abgaben-org-file
        (abgaben-test-org-file
         "notizen/abgaben.org"
         (concat "* Abgaben\n"
                 "** gruppe-di\n"
                 "*** 03\n"
                 "**** [[file:" (org-link-escape pdf-file)
                 "][Lösung]] Email: [[mu4e:msgid:ada@uni.example][Aufgabe 3]]\n"
                 "***** veralteter Export\n"
                 "Alter Text der weg muss.\n"
                 "**** [[file:andere.pdf][Andere]] Email: [[mu4e:msgid:bob@uni.example][Aufgabe 3]]\n")))
       (annotations
        (list
         '((page . 3) (edges 0.10 0.20 0.40 0.24) (type . text) (id . annot-3-0)
           (color . "#ff0000") (label . "Prüfer")
           (contents . "Aufgabe 3: 3/3 – sauber gelöst"))
         '((page . 2) (edges 0.10 0.55 0.40 0.60) (type . highlight)
           (id . annot-2-1) (markup-edges (0.10 0.55 0.40 0.60))
           (contents . "Aufgabe 2: 0.5/2 Beweis unvollständig"))
         '((page . 1) (edges 0.12 0.30 0.44 0.36) (type . text) (id . annot-1-0)
           (contents . "Aufgabe 1: 2.5/3 Randfall fehlt"))
         '((page . 2) (edges 0.10 0.15 0.40 0.20) (type . squiggly)
           (id . annot-2-0) (markup-edges (0.10 0.15 0.40 0.20))
           (contents . "Notation!"))
         '((page . 1) (edges 0.10 0.10 0.30 0.14) (type . link)
           (id . annot-1-link) (contents . "Aufgabe 9: 99/99"))))
       (buffer nil))
  (unwind-protect
      (progn
        (setq buffer (find-file abgaben-org-file))
        (goto-char (point-min))
        (search-forward "[[file:")
        (beginning-of-line)
        (cl-letf (((symbol-function 'pdf-info-getannots)
                   (lambda (&optional pages file-or-buffer)
                     (abgaben-test-record
                      (list 'getannots pages
                            (abgaben-test-relative file-or-buffer)))
                     (copy-tree annotations))))
          (call-interactively 'abgaben-export-pdf-annot-to-org))
        (list
         (abgaben-test-events)
         (list (point) (line-number-at-pos) (buffer-modified-p))
         (abgaben-test-buffer-text)))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))"##,
        expect![[
            r#"OK (((getannots nil "Abgaben SS25/gruppe-di/03/Aufgabe 03 [final] Lösung.pdf")) (31 4 t) "* Abgaben\n** gruppe-di\n*** 03\n**** [[file:[ORACLE-SANDBOX]/Abgaben SS25/gruppe-di/03/Aufgabe 03 \\[final\\] Lösung.pdf][Lösung]] Email: [[mu4e:msgid:ada@uni.example][Aufgabe 3]]\n***** Deine Punkte\nAufgabe 1: 2.5/3\nAufgabe 2: 0.5/2\nAufgabe 3: 3/3\nGesamt: 6.0/8 \n***** annot-1-0\nAufgabe 1: 2.5/3 Randfall fehlt\n***** annot-2-0\nNotation!\n***** annot-2-1\nAufgabe 2: 0.5/2 Beweis unvollständig\n***** annot-3-0\nAufgabe 3: 3/3 – sauber gelöst\n\n**** [[file:andere.pdf][Andere]] Email: [[mu4e:msgid:bob@uni.example][Aufgabe 3]]\n")"#
        ]],
    )
    .fresh_process()
}

fn export_pdf_annot_to_org_uses_the_shipped_defaults_for_an_unscored_submission() -> ParityBatchCase
{
    ParityBatchCase::value(
        "export_pdf_annot_to_org_uses_the_shipped_defaults_for_an_unscored_submission",
        r##"(let* ((pdf-file
        (expand-file-name "Abgaben SS25/gruppe-do/11/Nachreichung.pdf"
                          abgaben-test-root))
       (abgaben-org-file
        (abgaben-test-org-file
         "notizen/abgaben.org"
         (concat "* Abgaben\n"
                 "** gruppe-do\n"
                 "*** 11\n"
                 "**** [[file:" (org-link-escape pdf-file)
                 "][Nachreichung.pdf]] Email: [[mu4e:msgid:cleo@uni.example][Nachreichung]]\n")))
       (annotations
        (list
         '((page . 1) (edges 0.20 0.40 0.60 0.44) (type . text) (id . annot-1-1)
           (contents . "Bitte Notation prüfen (siehe Aufgabe 2)"))
         '((page . 1) (edges 0.20 0.10 0.60 0.14) (type . underline)
           (id . annot-1-0) (markup-edges (0.20 0.10 0.60 0.14)))))
       (buffer nil))
  (unwind-protect
      (progn
        (setq buffer (find-file abgaben-org-file))
        (goto-char (point-min))
        (search-forward "[[file:")
        (beginning-of-line)
        (cl-letf (((symbol-function 'pdf-info-getannots)
                   (lambda (&optional pages file-or-buffer)
                     (abgaben-test-record
                      (list 'getannots pages
                            (abgaben-test-relative file-or-buffer)))
                     (copy-tree annotations))))
          (call-interactively 'abgaben-export-pdf-annot-to-org))
        (save-buffer)
        (list
         (abgaben-test-events)
         (list abgaben-points-heading abgaben-points-overall
               abgaben-points-re abgaben-pdf-tools-org-non-exportable-types)
         (list (point) (buffer-modified-p))
         (abgaben-test-contents abgaben-org-file)))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))"##,
        expect![[
            r#"OK (((getannots nil "Abgaben SS25/gruppe-do/11/Nachreichung.pdf")) ("your points" "overall" "assignment [0-9.]*: ?\\([0-9.]*\\)/\\([0-9.]*\\)" (link)) (31 nil) "* Abgaben\n** gruppe-do\n*** 11\n**** [[file:[ORACLE-SANDBOX]/Abgaben SS25/gruppe-do/11/Nachreichung.pdf][Nachreichung.pdf]] Email: [[mu4e:msgid:cleo@uni.example][Nachreichung]]\n***** your points\noverall: 0/0 \n***** annot-1-0\n***** annot-1-1\nBitte Notation prüfen (siehe Aufgabe 2)\n")"#
        ]],
    )
    .fresh_process()
}

fn prepare_reply_yanks_the_mml_reply_and_opens_the_original_message() -> ParityBatchCase {
    ParityBatchCase::value(
        "prepare_reply_yanks_the_mml_reply_and_opens_the_original_message",
        r##"(let* ((pdf-file
        (expand-file-name "Abgaben SS25/gruppe-di/03/Aufgabe 03 [final] Lösung.pdf"
                          abgaben-test-root))
       (abgaben-org-file
        (abgaben-test-org-file
         "notizen/abgaben.org"
         (concat "* Abgaben\n"
                 "** gruppe-di\n"
                 "*** 03\n"
                 "**** [[file:" (org-link-escape pdf-file)
                 "][Lösung]] Email: [[mu4e:msgid:ada@uni.example][Aufgabe 3]]\n"
                 "***** Deine Punkte\n"
                 "Aufgabe 1: 2.5/3\n"
                 "Gesamt: 2.5/3 \n"
                 "***** annot-1-0\n"
                 "Aufgabe 1: 2.5/3 Randfall fehlt\n"
                 "**** [[file:andere.pdf][Andere]] Email: [[mu4e:msgid:bob@uni.example][Aufgabe 3]]\n")))
       (kill-ring nil)
       (kill-ring-yank-pointer nil)
       (buffer nil))
  (unwind-protect
      (progn
        (setq buffer (find-file abgaben-org-file))
        (goto-char (point-min))
        (search-forward "[[file:")
        (beginning-of-line)
        (let ((start (point)))
          (call-interactively 'abgaben-prepare-reply)
          (list
           (abgaben-test-events)
           (list start (point) (buffer-modified-p))
           (mapcar #'substring-no-properties kill-ring)
           (abgaben-test-buffer-text))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))"##,
        expect![[
            r#"OK (((open-mail "msgid:ada@uni.example")) (31 31 nil) ("***** Deine Punkte\nAufgabe 1: 2.5/3\nGesamt: 2.5/3 \n***** annot-1-0\nAufgabe 1: 2.5/3 Randfall fehlt\n<#part type=\"application/pdf\" filename=\"[ORACLE-SANDBOX]/Abgaben SS25/gruppe-di/03/Aufgabe 03 [final] Lösung.pdf\" disposition=attachment><#/part>" "**** [[file:[ORACLE-SANDBOX]/Abgaben SS25/gruppe-di/03/Aufgabe 03 \\[final\\] Lösung.pdf][Lösung]] Email: [[mu4e:msgid:ada@uni.example][Aufgabe 3]]\n" "**** [[file:[ORACLE-SANDBOX]/Abgaben SS25/gruppe-di/03/Aufgabe 03 \\[final\\] Lösung.pdf][Lösung]] Email: [[mu4e:msgid:ada@uni.example][Aufgabe 3]]\n***** Deine Punkte\nAufgabe 1: 2.5/3\nGesamt: 2.5/3 \n***** annot-1-0\nAufgabe 1: 2.5/3 Randfall fehlt\n") "* Abgaben\n** gruppe-di\n*** 03\n**** [[file:[ORACLE-SANDBOX]/Abgaben SS25/gruppe-di/03/Aufgabe 03 \\[final\\] Lösung.pdf][Lösung]] Email: [[mu4e:msgid:ada@uni.example][Aufgabe 3]]\n***** Deine Punkte\nAufgabe 1: 2.5/3\nGesamt: 2.5/3 \n***** annot-1-0\nAufgabe 1: 2.5/3 Randfall fehlt\n**** [[file:andere.pdf][Andere]] Email: [[mu4e:msgid:bob@uni.example][Aufgabe 3]]\n")"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        capture_submission_files_unpacks_and_links_a_zipped_submission(),
        capture_submission_creates_the_missing_week_and_unpacks_a_real_tarball(),
        capture_submission_without_the_group_heading_signals_search_failed_after_saving(),
        export_pdf_annot_to_org_sorts_annotations_filters_links_and_totals_points(),
        export_pdf_annot_to_org_uses_the_shipped_defaults_for_an_unscored_submission(),
        prepare_reply_yanks_the_mml_reply_and_opens_the_original_message(),
    ]
}
