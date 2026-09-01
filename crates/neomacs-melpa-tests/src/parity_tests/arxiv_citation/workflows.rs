use expect_test::expect;

use super::ParityBatchCase;

fn gui_selection_resolves_an_unpublished_paper_and_updates_real_bibliographies() -> ParityBatchCase
{
    ParityBatchCase::value(
        "gui_selection_resolves_an_unpublished_paper_and_updates_real_bibliographies",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (team-bib (expand-file-name "team.bib" root))
       (paper-bib (expand-file-name "paper.bib" root))
       (selected-url "https://arxiv.org/abs/2501.01234")
       (arxiv-citation-bibtex-files (list team-bib paper-bib))
       network-calls selection-calls response-buffers)
  (with-temp-file team-bib
    (insert "% Shared research bibliography\n"))
  (with-temp-file paper-bib
    (insert "@Book{existing,\n title = {Existing Result},\n}\n"))
  (cl-letf
      (((symbol-function 'gui-get-primary-selection)
        (lambda ()
          (push 'primary selection-calls)
          selected-url))
       ((symbol-function 'gui-get-selection)
        (lambda (selection)
          (push (list 'selection selection) selection-calls)
          "https://arxiv.org/abs/9999.99999"))
       ((symbol-function 'url-retrieve-synchronously)
        (lambda (url &rest arguments)
          (push (cons url arguments) network-calls)
          (let ((buffer (generate-new-buffer " *arxiv-citation-response*")))
            (push buffer response-buffers)
            (with-current-buffer buffer
              (cond
               ((string-prefix-p "https://zbmath.org/?q=arXiv:" url)
                (insert
                 "HTTP/1.1 200 OK\nContent-Type: text/html\n\n"
                 "<html ><head><title>zbMATH search</title>"
                 "<script>No documents matched</script></head><body></body></html>"))
               ((string-prefix-p "http://export.arxiv.org/api/query?id_list=" url)
                (insert
                 "HTTP/1.1 200 OK\nContent-Type: application/atom+xml\n\n"
                 "<?xml version=\"1.0\" encoding=\"UTF-8\"?>"
                 "<feed><entry>"
                 "<title>Operational Semantics for AI: A Reproducible Study</title>"
                 "<author><name>Ada Lovelace</name></author>"
                 "<author><name>Grace Brewster Murray Hopper</name></author>"
                 "<published>2025-01-17T12:00:00Z</published>"
                 "<category term=\"cs.PL\"/><category term=\"cs.AI\"/>"
                 "</entry></feed>"))
               (t
                (error "Unexpected network request: %S" url))))
            buffer))))
    (unwind-protect
        (let ((result (call-interactively #'arxiv-citation-gui)))
          (list
           result
           (commandp 'arxiv-citation-gui)
           (nreverse selection-calls)
           (nreverse network-calls)
           (mapcar
            (lambda (file)
              (with-temp-buffer
                (insert-file-contents file)
                (buffer-string)))
            (list team-bib paper-bib))))
      (mapc
       (lambda (buffer)
         (when (buffer-live-p buffer)
           (kill-buffer buffer)))
       response-buffers)
      (dolist (file (list team-bib paper-bib))
        (when (file-exists-p file)
          (delete-file file))))))"##,
        expect![[
            r#"OK (nil t (primary (selection CLIPBOARD)) (("https://zbmath.org/?q=arXiv:2501.01234" t t) ("http://export.arxiv.org/api/query?id_list=2501.01234" t t)) ("% Shared research bibliography\n\n@Article{lovelace25:operat-seman-ai,\n author        = {Lovelace, Ada and Hopper, GraceBrewsterMurray},\n journal       = {arXiv e-prints},\n title         = {{O}perational {S}emantics for {A}{I}: {A} {R}eproducible {S}tudy},\n year          = {2025},\n eprint        = {2501.01234},\n eprintclass   = {cs.PL},\n eprinttype    = {arXiv},\n keywords      = {cs.PL, cs.AI},\n}\n" "@Book{existing,\n title = {Existing Result},\n}\n\n@Article{lovelace25:operat-seman-ai,\n author        = {Lovelace, Ada and Hopper, GraceBrewsterMurray},\n journal       = {arXiv e-prints},\n title         = {{O}perational {S}emantics for {A}{I}: {A} {R}eproducible {S}tudy},\n year          = {2025},\n eprint        = {2501.01234},\n eprintclass   = {cs.PL},\n eprinttype    = {arXiv},\n keywords      = {cs.PL, cs.AI},\n}\n"))"#
        ]],
    )
}

fn clipboard_zbmath_result_replaces_the_remote_key_and_appends_aligned_bibtex() -> ParityBatchCase {
    ParityBatchCase::value(
        "clipboard_zbmath_result_replaces_the_remote_key_and_appends_aligned_bibtex",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (bibliography (expand-file-name "published.bib" root))
       (arxiv-citation-bibtex-files (list bibliography))
       network-calls response-buffers)
  (with-temp-file bibliography
    (insert "% Published papers\n"))
  (cl-letf
      (((symbol-function 'gui-get-primary-selection)
        (lambda () "selected notes, not a URL"))
       ((symbol-function 'gui-get-selection)
        (lambda (selection)
          (unless (eq selection 'CLIPBOARD)
            (error "Unexpected GUI selection: %S" selection))
          "https://zbmath.org/?q=an:145668001"))
       ((symbol-function 'url-retrieve-synchronously)
        (lambda (url &rest arguments)
          (push (cons url arguments) network-calls)
          (let ((buffer (generate-new-buffer " *zbmath-response*")))
            (push buffer response-buffers)
            (with-current-buffer buffer
              (cond
               ((equal url "https://zbmath.org/?q=an:145668001")
                (insert
                 "HTTP/1.1 200 OK\nContent-Type: text/html\n\n"
                 "<html ><head><title>zbMATH result</title>"
                 "<script>Document Zbl 1456.68001</script></head><body></body></html>"))
               ((equal url "https://zbmath.org/bibtex/1456.68001.bib")
                (insert
                 "HTTP/1.1 200 OK\nContent-Type: text/plain\n\n"
                 "@Article{REMOTE-KEY,\n"
                 "author = {Lovelace, Ada and Turing, Alan},\n"
                 "title = {Executable Semantics for Editors},\n"
                 "journal = {Journal of Reproducible Systems},\n"
                 "year = {2024},\n"
                 "volume = {17},\n"
                 "pages = {101--129},\n"
                 "}\n"))
               (t
                (error "Unexpected network request: %S" url))))
            buffer))))
    (unwind-protect
        (list
         (call-interactively #'arxiv-citation-gui)
         (nreverse network-calls)
         (with-temp-buffer
           (insert-file-contents bibliography)
           (buffer-string)))
      (mapc
       (lambda (buffer)
         (when (buffer-live-p buffer)
           (kill-buffer buffer)))
       response-buffers)
      (when (file-exists-p bibliography)
        (delete-file bibliography)))))"##,
        expect![[
            r#"OK (nil (("https://zbmath.org/?q=an:145668001" t t) ("https://zbmath.org/bibtex/1456.68001.bib" t t)) "% Published papers\n\n@Article{lovelace24:execut-seman-editor,\nauthor       = {Lovelace, Ada and Turing, Alan},\ntitle        = {Executable Semantics for Editors},\njournal      = {Journal of Reproducible Systems},\nyear         = {2024},\nvolume       = {17},\npages        = {101--129},\n}\n\n")"#
        ]],
    )
}

fn elfeed_downloads_a_real_pdf_file_with_the_documented_name_and_opens_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "elfeed_downloads_a_real_pdf_file_with_the_documented_name_and_opens_it",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (library (file-name-as-directory (expand-file-name "library" root)))
       (expected-file
        (expand-file-name
         "lovelace-turing_practical-proof-assistants.pdf"
         library))
       (pdf-bytes "%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\n%%EOF\n")
       (arxiv-citation-library library)
       (arxiv-citation-max-authors 2)
       (arxiv-citation-overwrite-file t)
       network-calls boundary-calls response-buffers)
  (make-directory library t)
  (with-temp-file expected-file
    (insert "stale download"))
  (provide 'elfeed)
  (setq elfeed-show-entry '(:id "feed-entry-42"))
  (cl-letf
      (((symbol-function 'elfeed-entry-link)
        (lambda (entry)
          (push (list 'entry-link entry) boundary-calls)
          "https://arxiv.org/abs/2502.42424"))
       ((symbol-function 'url-retrieve-synchronously)
        (lambda (url &rest arguments)
          (push (cons url arguments) network-calls)
          (let ((buffer (generate-new-buffer " *arxiv-download-details*")))
            (push buffer response-buffers)
            (with-current-buffer buffer
              (insert
               "HTTP/1.1 200 OK\nContent-Type: application/atom+xml\n\n"
               "<?xml version=\"1.0\" encoding=\"UTF-8\"?>"
               "<feed><entry>"
               "<title>Practical Proof Assistants: Systems at Scale</title>"
               "<author><name>Ada Lovelace</name></author>"
               "<author><name>Alan M. Turing</name></author>"
               "<author><name>Grace Hopper</name></author>"
               "<published>2025-02-28T09:30:00Z</published>"
               "<category term=\"cs.LO\"/><category term=\"cs.PL\"/>"
               "</entry></feed>"))
            buffer)))
       ((symbol-function 'url-copy-file)
        (lambda (url file overwrite)
          (push (list 'copy url file overwrite) boundary-calls)
          (with-temp-buffer
            (set-buffer-multibyte nil)
            (insert (string-as-unibyte pdf-bytes))
            (let ((coding-system-for-write 'no-conversion))
              (write-region (point-min) (point-max) file nil 'silent)))
          nil))
       ((symbol-function 'arxiv-citation-test-viewer)
        (lambda (file)
          (push
           (list
            'open
            file
            (with-temp-buffer
              (set-buffer-multibyte nil)
              (insert-file-contents-literally file)
              (list
               (buffer-size)
               (secure-hash 'sha256 (current-buffer))
               (buffer-string))))
           boundary-calls)
          'viewed)))
    (let ((arxiv-citation-open-pdf-function
           #'arxiv-citation-test-viewer))
      (unwind-protect
          (list
           (call-interactively #'arxiv-citation-elfeed)
           (nreverse network-calls)
           (nreverse boundary-calls)
           (file-exists-p expected-file)
           (with-temp-buffer
             (set-buffer-multibyte nil)
             (insert-file-contents-literally expected-file)
             (buffer-string)))
        (mapc
         (lambda (buffer)
           (when (buffer-live-p buffer)
             (kill-buffer buffer)))
         response-buffers)
        (when (file-directory-p library)
          (delete-directory library t))
        (makunbound 'elfeed-show-entry)))))"##,
        expect![[
            r#"OK (viewed (("http://export.arxiv.org/api/query?id_list=2502.42424" t t)) ((entry-link (:id "feed-entry-42")) (copy "https://arxiv.org/pdf/2502.42424.pdf" "[ORACLE-SANDBOX]/library//lovelace-turing_practical-proof-assistants.pdf" t) (open "[ORACLE-SANDBOX]/library/lovelace-turing_practical-proof-assistants.pdf" (51 "904636248025ad20fb9c6bd8b700179a2a42edb5df3636e926c7e09055ee3f75" "%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\n%%EOF\n"))) t "%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\n%%EOF\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        gui_selection_resolves_an_unpublished_paper_and_updates_real_bibliographies(),
        clipboard_zbmath_result_replaces_the_remote_key_and_appends_aligned_bibtex(),
        elfeed_downloads_a_real_pdf_file_with_the_documented_name_and_opens_it(),
    ]
}
