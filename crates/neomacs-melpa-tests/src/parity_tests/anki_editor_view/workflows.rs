use expect_test::expect;

use super::ParityBatchCase;

fn protocol_link_searches_real_card_directories_and_reveals_the_matching_subtree() -> ParityBatchCase
{
    ParityBatchCase::value(
        "protocol_link_searches_real_card_directories_and_reveals_the_matching_subtree",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (expand-file-name "anki editor view" sandbox))
         (active (expand-file-name "active decks" root))
         (archive (expand-file-name "archive" root))
         (target (expand-file-name "networking.org" active))
         opened-buffer
         recenter-calls)
    (when (file-directory-p root)
      (delete-directory root t))
    (make-directory active t)
    (make-directory archive t)
    (with-temp-file target
      (insert
       "#+title: Networking cards\n"
       "#+startup: overview\n"
       "* Transport\n"
       "** TCP handshake\n"
       ":PROPERTIES:\n"
       ":ANKI_NOTE_ID: 4242\n"
       ":END:\n"
       "SYN, SYN-ACK, ACK.\n"
       "*** Troubleshooting\n"
       "Packet captures should show all three messages.\n"))
    (with-temp-file (expand-file-name "old.org" archive)
      (insert
       "* Retired card\n"
       ":PROPERTIES:\n"
       ":ANKI_NOTE_ID: 1000\n"
       ":END:\n"))
    (unwind-protect
        (let* ((anki-editor-view-files (list active archive))
               (entry
                (seq-find
                 (lambda (candidate)
                   (equal (plist-get (cdr candidate) :protocol)
                          "anki-editor-view"))
                 org-protocol-protocol-alist))
               (handler (plist-get (cdr entry) :function)))
          (cl-letf
              (((symbol-function 'recenter-top-bottom)
                (lambda (&rest arguments)
                  (push arguments recenter-calls))))
            (funcall handler '(:id "4242" :source "anki-review"))
            (setq opened-buffer (current-buffer))
            (let ((subtree-end
                   (save-excursion
                     (org-end-of-subtree t t))))
              (list
               handler
               (file-relative-name buffer-file-name root)
               (substring-no-properties (org-get-heading t t t t))
               (org-entry-get nil "ANKI_NOTE_ID")
               (buffer-substring-no-properties (point) subtree-end)
               (invisible-p
                (save-excursion
                  (forward-line 1)
                  (point)))
               (nreverse recenter-calls)))))
      (when (buffer-live-p opened-buffer)
        (with-current-buffer opened-buffer
          (set-buffer-modified-p nil))
        (kill-buffer opened-buffer))
      (when (file-directory-p root)
        (delete-directory root t))))"##,
        expect![[
            r#"OK (anki-editor-view--open-anki-note "active decks/networking.org" "TCP handshake" "4242" "** TCP handshake\n:PROPERTIES:\n:ANKI_NOTE_ID: 4242\n:END:\nSYN, SYN-ACK, ACK.\n*** Troubleshooting\nPacket captures should show all three messages.\n" nil (nil))"#
        ]],
    )
}

fn duplicate_note_ids_warn_and_open_the_first_search_result() -> ParityBatchCase {
    ParityBatchCase::value(
        "duplicate_note_ids_warn_and_open_the_first_search_result",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (expand-file-name "anki-editor-view-duplicates" sandbox))
         (current (expand-file-name "current.org" root))
         (archive (expand-file-name "archive.org" root))
         messages
         ripgrep-command
         opened-buffer)
    (when (file-directory-p root)
      (delete-directory root t))
    (make-directory root t)
    (with-temp-file current
      (insert
       "* Current explanation\n"
       ":PROPERTIES:\n"
       ":ANKI_NOTE_ID: 77\n"
       ":END:\n"
       "Use this maintained card.\n"))
    (with-temp-file archive
      (insert
       "* Archived explanation\n"
       ":PROPERTIES:\n"
       ":ANKI_NOTE_ID: 77\n"
       ":END:\n"
       "This duplicate should not win.\n"))
    (unwind-protect
        (let ((anki-editor-view-files (list archive current)))
          (cl-letf
              (((symbol-function 'recenter-top-bottom)
                (lambda (&rest _arguments)))
               ((symbol-function 'shell-command-to-string)
                (lambda (command)
                  (setq ripgrep-command command)
                  (concat current ":3::ANKI_NOTE_ID: 77\n"
                          archive ":3::ANKI_NOTE_ID: 77\n")))
               ((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (let ((rendered (apply #'format format-string arguments)))
                    (when (string-prefix-p "Warning:" rendered)
                      (push rendered messages))
                    rendered))))
            (anki-editor-view--open-anki-note '(:id 77))
            (setq opened-buffer (current-buffer))
            (list
             ripgrep-command
             (file-relative-name buffer-file-name root)
             (substring-no-properties (org-get-heading t t t t))
             (org-entry-get nil "ANKI_NOTE_ID")
             (nreverse messages))))
      (when (buffer-live-p opened-buffer)
        (with-current-buffer opened-buffer
          (set-buffer-modified-p nil))
        (kill-buffer opened-buffer))
      (when (file-directory-p root)
        (delete-directory root t))))"##,
        expect![[
            r#"OK ("rg -n -e \":ANKI_NOTE_ID: 77\" --no-heading  \"[ORACLE-SANDBOX]/anki-editor-view-duplicates/archive.org\" \"[ORACLE-SANDBOX]/anki-editor-view-duplicates/current.org\"" "current.org" "Current explanation" "77" ("Warning: Found more than one (2) location of the Anki Note"))"#
        ]],
    )
}

fn a_missing_anki_note_reports_the_problem_without_replacing_the_users_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_missing_anki_note_reports_the_problem_without_replacing_the_users_buffer",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (expand-file-name "anki-editor-view-missing" sandbox))
         messages)
    (when (file-directory-p root)
      (delete-directory root t))
    (make-directory root t)
    (with-temp-file (expand-file-name "cards.org" root)
      (insert
       "* Existing card\n"
       ":PROPERTIES:\n"
       ":ANKI_NOTE_ID: 123\n"
       ":END:\n"))
    (unwind-protect
        (with-temp-buffer
          (rename-buffer "*Anki review dashboard*" t)
          (insert "Cards due today: 20")
          (let ((anki-editor-view-files (list root))
                (original-buffer (current-buffer)))
            (cl-letf
                (((symbol-function 'message)
                  (lambda (format-string &rest arguments)
                    (let ((rendered (apply #'format format-string arguments)))
                      (push rendered messages)
                      rendered))))
              (list
               (anki-editor-view--open-anki-note '(:id 999999))
               (eq (current-buffer) original-buffer)
               (buffer-string)
               (nreverse messages)))))
      (when (file-directory-p root)
        (delete-directory root t))))"##,
        expect![[r#"OK (nil t "Cards due today: 20" ("Anki note not found."))"#]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        protocol_link_searches_real_card_directories_and_reveals_the_matching_subtree(),
        duplicate_note_ids_warn_and_open_the_first_search_result(),
        a_missing_anki_note_reports_the_problem_without_replacing_the_users_buffer(),
    ]
}
