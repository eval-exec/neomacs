use expect_test::expect;

use super::ParityBatchCase;

/// Recording contacts through `addressbook-bookmark-set'.  The prompts show
/// the shape of the interview - name once, then group, mail, phone and web
/// asked repeatedly until answered empty, then the postal fields - and the
/// resulting bookmark records carry every field the package defines, tagged
/// with its own "addressbook" type and jump handler, so `addressbook-alist-only'
/// can tell them apart from ordinary bookmarks.  Nothing is written to disk
/// yet: only the modification counter moves.
fn creating_contacts_records_addressbook_bookmarks() -> ParityBatchCase {
    ParityBatchCase::value(
        "creating_contacts_records_addressbook_bookmarks",
        r##"(let ((bookmark-default-file (ab-test-path "book/contacts.bmk"))
      (bookmark-alist nil)
      (bookmark-save-flag nil)
      (bookmark-alist-modification-count 0))
  (make-directory (ab-test-path "book") t)
  (let ((first (ab-test-with-answers (copy-sequence ab-test-zoe)
                 (addressbook-bookmark-set)
                 (list (mapcar #'car bookmark-alist)
                       (ab-test-record-text "Zoë Müller")
                       (mapcar #'car (ab-test-asked))))))
    (ab-test-with-answers (copy-sequence ab-test-ann)
      (addressbook-bookmark-set))
    (list first
          (mapcar #'car bookmark-alist)
          (ab-test-record-text "Ann Smith")
          (mapcar #'car (addressbook-alist-only))
          (addressbook-bookmark-p "Zoë Müller")
          bookmark-alist-modification-count
          (file-exists-p (ab-test-path "book/contacts.bmk")))))"##,
        expect![[
            r#"OK ((("Zoë Müller") "(\"Zoë Müller\" (city . \"Köln\") (country . \"Deutschland\") (email . \"zoe@example.org, z.mueller@example.net\") (group . \"Freunde\") (handler . addressbook-bookmark-jump) (image . \"\") (last-modified <TIME>) (location . \"Addressbook entry\") (note . \"Grüße aus Köln\") (phone . \"+49 221 4711\") (position . 0) (state . \"NRW\") (street . \"Hauptstraße 7\") (type . \"addressbook\") (web . \"https://zoë.example\") (zipcode . \"50667\"))" ("Name: " "Group: " "Group: " "Mail: " "Mail: " "Mail: " "Phone: " "Phone: " "Web: " "Web: " "Street: " "City: " "State: " "Zipcode: " "Country: " "Note: " "Image path: " "`Zoë Müller' Recorded. Add a new contact? ")) ("Ann Smith" "Zoë Müller") "(\"Ann Smith\" (city . \"Springfield\") (country . \"USA\") (email . \"ann@example.com\") (group . \"Work\") (handler . addressbook-bookmark-jump) (image . \"\") (last-modified <TIME>) (location . \"Addressbook entry\") (note . \"\") (phone . \"\") (position . 0) (state . \"IL\") (street . \"12 Main Street\") (type . \"addressbook\") (web . \"\") (zipcode . \"62704\"))" ("Ann Smith" "Zoë Müller") t 2 nil)"#
        ]],
    )
}

fn the_bookmark_file_round_trips_a_non_ascii_contact_and_filename() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_bookmark_file_round_trips_a_non_ascii_contact_and_filename",
        r##"(let* ((directory (ab-test-path "carnet d'adresses"))
       (file (expand-file-name "répertoire.bmk" directory))
       (bookmark-default-file file)
       (bookmark-alist nil)
       (bookmark-save-flag nil)
       (bookmark-alist-modification-count 0))
  (make-directory directory t)
  (ab-test-with-answers (copy-sequence ab-test-zoe) (addressbook-bookmark-set))
  (ab-test-with-answers (copy-sequence ab-test-ann) (addressbook-bookmark-set))
  (bookmark-save)
  (let ((saved (ab-test-record-text "Zoë Müller"))
        (contents (ab-test-file-contents file))
        (bytes (ab-test-file-bytes file)))
    (setq bookmark-alist nil)
    (bookmark-load file t t)
    (list (list (file-exists-p file)
                (directory-files directory)
                (mapcar #'multibyte-string-p (directory-files directory))
                bookmark-file-coding-system)
          (ab-test-normalize contents)
          (list (and (string-search "?" bytes) t)
                (append (string-to-list
                         (let ((start (string-search "Zo" bytes)))
                           (substring bytes start (+ start 14))))
                        nil))
          (mapcar #'car bookmark-alist)
          (equal saved (ab-test-record-text "Zoë Müller"))
          (ab-test-record-text "Zoë Müller"))))"##,
        expect![[
            r#"OK ((t ("." ".." "répertoire.bmk") (nil nil t) utf-8-emacs-unix) ";;;; Emacs Bookmark Format Version 1;;;; -*- coding: utf-8-emacs; mode: lisp-data -*-\n;;; This format is meant to be slightly human-readable;\n;;; nevertheless, you probably don't want to edit it.\n;;; -*- End Of Bookmark File Format Version Stamp -*-\n((\"Ann Smith\"\n  (position . 0)\n  (last-modified <TIME>)\n  (type . \"addressbook\")\n  (location . \"Addressbook entry\")\n  (image . \"\")\n  (email . \"ann@example.com\")\n  (phone . \"\")\n  (web . \"\")\n  (street . \"12 Main Street\")\n  (city . \"Springfield\")\n  (state . \"IL\")\n  (zipcode . \"62704\")\n  (country . \"USA\")\n  (note . \"\")\n  (group . \"Work\")\n  (handler . addressbook-bookmark-jump))\n(\"Zoë Müller\"\n (position . 0)\n (last-modified <TIME>)\n (type . \"addressbook\")\n (location . \"Addressbook entry\")\n (image . \"\")\n (email . \"zoe@example.org, z.mueller@example.net\")\n (phone . \"+49 221 4711\")\n (web . \"https://zoë.example\")\n (street . \"Hauptstraße 7\")\n (city . \"Köln\")\n (state . \"NRW\")\n (zipcode . \"50667\")\n (country . \"Deutschland\")\n (note . \"Grüße aus Köln\")\n (group . \"Freunde\")\n (handler . addressbook-bookmark-jump))\n)\n" (nil (90 111 195 171 32 77 195 188 108 108 101 114 34 10)) ("Ann Smith" "Zoë Müller") t "(\"Zoë Müller\" (city . \"Köln\") (country . \"Deutschland\") (email . \"zoe@example.org, z.mueller@example.net\") (group . \"Freunde\") (handler . addressbook-bookmark-jump) (image . \"\") (last-modified <TIME>) (location . \"Addressbook entry\") (note . \"Grüße aus Köln\") (phone . \"+49 221 4711\") (position . 0) (state . \"NRW\") (street . \"Hauptstraße 7\") (type . \"addressbook\") (web . \"https://zoë.example\") (zipcode . \"50667\"))")"#
        ]],
    )
}

fn a_latin_1_bookmark_file_keeps_its_accented_contact() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_latin_1_bookmark_file_keeps_its_accented_contact",
        r##"(let* ((directory (ab-test-path "carnet"))
       (file (expand-file-name "adressen.bmk" directory))
       (bookmark-default-file file)
       (bookmark-alist nil)
       (bookmark-save-flag nil)
       (bookmark-file-coding-system 'latin-1)
       (bookmark-alist-modification-count 0))
  (make-directory directory t)
  (ab-test-with-answers (copy-sequence ab-test-zoe) (addressbook-bookmark-set))
  (bookmark-save)
  (let* ((bytes (ab-test-file-bytes file))
         (saved (ab-test-record-text "Zoë Müller")))
    (setq bookmark-alist nil)
    (bookmark-load file t t)
    (list (and (string-search "?" bytes) t)
          (append (string-to-list (substring bytes (or (string-search "Zo" bytes) 0)
                                             (+ 12 (or (string-search "Zo" bytes) 0))))
                  nil)
          bookmark-file-coding-system
          (mapcar #'car bookmark-alist)
          (equal saved (ab-test-record-text (caar bookmark-alist)))
          (assoc-default 'city (car bookmark-alist)))))"##,
        expect![[
            r#"OK (nil (90 111 235 32 77 252 108 108 101 114 34 10) iso-latin-1-unix ("Zoë Müller") t "Köln")"#
        ]],
    )
    .fresh_process()
}

fn the_addressbook_buffer_renders_every_recorded_field() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_addressbook_buffer_renders_every_recorded_field",
        r##"(let ((bookmark-default-file (ab-test-path "book/contacts.bmk"))
      (bookmark-alist nil)
      (bookmark-save-flag nil)
      (bookmark-alist-modification-count 0)
      (user-login-name "melpa-test"))
  (make-directory (ab-test-path "book") t)
  (ab-test-with-answers (copy-sequence ab-test-zoe) (addressbook-bookmark-set))
  (ab-test-with-answers (copy-sequence ab-test-ann) (addressbook-bookmark-set))
  (unwind-protect
      (progn
        (addressbook-jump (list "Zoë Müller" "Ann Smith"))
        (with-current-buffer addressbook-buffer-name
          (list (buffer-name)
                major-mode
                buffer-read-only
                (buffer-substring-no-properties (point-min) (point-max))
                (list (get-text-property (point-min) 'face)
                      (save-excursion (goto-char (point-min))
                                      (search-forward "Name:")
                                      (list (get-text-property (- (point) 1) 'name)
                                            (get-text-property (- (point) 1) 'face))))
                (point)
                (car (addressbook-get-contact-data)))))
    (when (get-buffer addressbook-buffer-name)
      (kill-buffer addressbook-buffer-name))))"##,
        expect![[
            r#"OK ("*addressbook*" addressbook-mode t "Addressbook Melpa-Test\n\n---------------------------------------------\nName:    Zoë Müller\nGroup:   Freunde\nMail:    zoe@example.org, z.mueller@example.net\nPhone:   +49 221 4711\nWeb:     https://zoë.example\nStreet:  Hauptstraße 7\nCity:    Köln\nState:   NRW\nZipcode: 50667\nCountry: Deutschland\nNote:    Grüße aus Köln\n---------------------------------------------\nName:    Ann Smith\nGroup:   Work\nMail:    ann@example.com\nStreet:  12 Main Street\nCity:    Springfield\nState:   IL\nZipcode: 62704\nCountry: USA\n---------------------------------------------\n" (((:foreground "green" :underline t)) ("Zoë Müller" ((:underline t)))) 1 "Zoë Müller")"#
        ]],
    )
}

fn editing_a_contact_offers_its_values_and_rewrites_the_entry() -> ParityBatchCase {
    ParityBatchCase::value(
        "editing_a_contact_offers_its_values_and_rewrites_the_entry",
        r##"(let ((bookmark-default-file (ab-test-path "book/contacts.bmk"))
      (bookmark-alist nil)
      (bookmark-save-flag nil)
      (bookmark-alist-modification-count 0)
      (user-login-name "melpa-test"))
  (make-directory (ab-test-path "book") t)
  (ab-test-with-answers (copy-sequence ab-test-zoe) (addressbook-bookmark-set))
  (ab-test-with-answers (copy-sequence ab-test-ann) (addressbook-bookmark-set))
  (unwind-protect
      (progn
        (addressbook-jump (list "Zoë Müller" "Ann Smith"))
        (with-current-buffer addressbook-buffer-name
          (set-window-buffer (selected-window) (current-buffer))
          (goto-char (point-min))
          (search-forward "Name:    Zoë")
          (let ((edited (ab-test-with-answers (copy-sequence ab-test-zoe-edit)
                          (execute-kbd-macro (kbd "e"))
                          (ab-test-asked))))
            (list (key-binding (kbd "e"))
                  edited
                  (ab-test-record-text "Zoë Müller")
                  (buffer-substring-no-properties (point-min) (point-max))
                  bookmark-alist-modification-count))))
    (when (get-buffer addressbook-buffer-name)
      (kill-buffer addressbook-buffer-name))))"##,
        expect![[
            r#"OK (addressbook-edit (("Name: " . "Zoë Müller") ("Group: " . "Freunde") ("Mail: " . "zoe@example.org, z.mueller@example.net") ("Phone: " . "+49 221 4711") ("Web: " . "https://zoë.example") ("Street: " . "Hauptstraße 7") ("City: " . "Köln") ("State: " . "NRW") ("Zipcode: " . "50667") ("Country: " . "Deutschland") ("Note: " . "Grüße aus Köln") ("Image path: " . "") ("Save changes? " . :y-or-n-p)) "(\"Zoë Müller\" (city . \"München\") (country . \"Deutschland\") (email . \"zoe@example.org\") (group . \"Freunde\") (handler . addressbook-bookmark-jump) (image . \"\") (last-modified <TIME>) (location . \"Addressbook entry\") (note . \"Umgezogen\") (phone . \"+49 221 4711\") (position . 0) (state . \"Bayern\") (street . \"Sendlinger Straße 1\") (type . \"addressbook\") (web . \"https://zoë.example\") (zipcode . \"80331\"))" "Addressbook Melpa-Test\n\n---------------------------------------------\nName:    Zoë Müller\nGroup:   Freunde\nMail:    zoe@example.org\nPhone:   +49 221 4711\nWeb:     https://zoë.example\nStreet:  Sendlinger Straße 1\nCity:    München\nState:   Bayern\nZipcode: 80331\nCountry: Deutschland\nNote:    Umgezogen\n---------------------------------------------\nName:    Ann Smith\nGroup:   Work\nMail:    ann@example.com\nStreet:  12 Main Street\nCity:    Springfield\nState:   IL\nZipcode: 62704\nCountry: USA\n---------------------------------------------\n" 3)"#
        ]],
    )
}

fn deleting_a_contact_leaves_the_buffer_refresh_broken() -> ParityBatchCase {
    ParityBatchCase::value(
        "deleting_a_contact_leaves_the_buffer_refresh_broken",
        r##"(let ((bookmark-default-file (ab-test-path "book/contacts.bmk"))
      (bookmark-alist nil)
      (bookmark-save-flag nil)
      (bookmark-alist-modification-count 0)
      (user-login-name "melpa-test"))
  (make-directory (ab-test-path "book") t)
  (ab-test-with-answers (copy-sequence ab-test-zoe) (addressbook-bookmark-set))
  (ab-test-with-answers (copy-sequence ab-test-ann) (addressbook-bookmark-set))
  (unwind-protect
      (progn
        (addressbook-jump (list "Zoë Müller" "Ann Smith"))
        (bookmark-delete "Ann Smith")
        (bookmark-save)
        (let ((after-delete (list (mapcar #'car bookmark-alist)
                                  (mapcar #'car (addressbook-alist-only))
                                  (assoc "Ann Smith" bookmark-alist)
                                  (and (string-search "Ann Smith"
                                                      (ab-test-file-contents
                                                       (ab-test-path "book/contacts.bmk")))
                                       t))))
          (setq bookmark-alist nil)
          (bookmark-load (ab-test-path "book/contacts.bmk") t t)
          (let ((reloaded (mapcar #'car bookmark-alist)))
            (with-current-buffer addressbook-buffer-name
              (set-window-buffer (selected-window) (current-buffer))
              (goto-char (point-min))
              (list after-delete
                    reloaded
                    (condition-case error
                        (progn (execute-kbd-macro (kbd "g")) :reverted)
                      (error error))
                    (buffer-substring-no-properties (point-min) (point-max)))))))
    (when (get-buffer addressbook-buffer-name)
      (kill-buffer addressbook-buffer-name))))"##,
        expect![[
            r#"OK ((("Zoë Müller") ("Zoë Müller") nil nil) ("Zoë Müller") (wrong-type-argument stringp nil) "Name:    Zoë Müller\nGroup:   Freunde\nMail:    zoe@example.org, z.mueller@example.net\nPhone:   +49 221 4711\nWeb:     https://zoë.example\nStreet:  Hauptstraße 7\nCity:    Köln\nState:   NRW\nZipcode: 50667\nCountry: Deutschland\nNote:    Grüße aus Köln\n---------------------------------------------\n")"#
        ]],
    )
    .fresh_process()
}

fn mail_completion_offers_the_recorded_addresses() -> ParityBatchCase {
    ParityBatchCase::value(
        "mail_completion_offers_the_recorded_addresses",
        r##"(let ((bookmark-default-file (ab-test-path "book/contacts.bmk"))
      (bookmark-alist nil)
      (bookmark-save-flag nil)
      (bookmark-alist-modification-count 0))
  (make-directory (ab-test-path "book") t)
  (ab-test-with-answers (copy-sequence ab-test-zoe) (addressbook-bookmark-set))
  (ab-test-with-answers (copy-sequence ab-test-ann) (addressbook-bookmark-set))
  (addressbook-turn-on-mail-completion)
  (let ((buffer (generate-new-buffer "*addressbook-mail*")))
    (unwind-protect
        (with-current-buffer buffer
          (set-window-buffer (selected-window) buffer)
          (message-mode)
          (insert "To: \nCc: \nSubject: \n--text follows this line--\n")
          (goto-char (point-min))
          (end-of-line)
          (let ((before-binding (key-binding (kbd "TAB"))))
          (insert "Ann")
          (let ((candidates (nth 2 (addressbook-message-complete))))
            (execute-kbd-macro (kbd "TAB"))
            (let ((unique (list (buffer-substring-no-properties
                                 (line-beginning-position) (line-end-position))
                                (point))))
              (forward-line 1)
              (end-of-line)
              (insert "Zo")
              (execute-kbd-macro (kbd "TAB"))
              (execute-kbd-macro (kbd "TAB"))
              (list (mapcar #'car message-completion-alist)
                    (list before-binding (key-binding (kbd "TAB")))
                    candidates
                    unique
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position))
                    (and (get-buffer "*Completions*")
                         (with-current-buffer "*Completions*"
                           (buffer-substring-no-properties (point-min) (point-max)))))))))
      (kill-buffer buffer)
      (when (get-buffer "*Completions*") (kill-buffer "*Completions*")))))"##,
        expect![[
            r#"OK (("^\\(Newsgroups\\|Followup-To\\|Posted-To\\|Gcc\\):" "^\\(Newsgroups\\|Followup-To\\|Posted-To\\|Gcc\\):" "^\\(Resent-\\)?\\(To\\|B?Cc\\):" "^\\(Reply-To\\|From\\|Mail-Followup-To\\|Mail-Copies-To\\):" "^\\(Disposition-Notification-To\\|Return-Receipt-To\\):") (message-tab completion-at-point) (#("Ann Smith              ann@example.com" 23 38 (face font-lock-doc-face)) #("Zoë Müller             zoe@example.org" 23 38 (face font-lock-doc-face)) #("Zoë Müller             z.mueller@example.net" 23 44 (face font-lock-doc-face))) ("To: ann@example.com" 20) "Cc: Zoë Müller             z" "Type M-RET on a completion to select it.\nType M-<down> or M-<up> to move point between completions.\n\n2 possible completions:\nZoë Müller             z.mueller@example.net\nZoë Müller             zoe@example.org")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        creating_contacts_records_addressbook_bookmarks(),
        the_bookmark_file_round_trips_a_non_ascii_contact_and_filename(),
        a_latin_1_bookmark_file_keeps_its_accented_contact(),
        the_addressbook_buffer_renders_every_recorded_field(),
        editing_a_contact_offers_its_values_and_rewrites_the_entry(),
        deleting_a_contact_leaves_the_buffer_refresh_broken(),
        mail_completion_offers_the_recorded_addresses(),
    ]
}
