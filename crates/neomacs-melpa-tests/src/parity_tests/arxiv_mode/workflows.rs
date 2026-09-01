use expect_test::expect;

use super::ParityBatchCase;

fn simple_search_renders_results_navigates_to_an_abstract_and_opens_the_paper() -> ParityBatchCase {
    ParityBatchCase::value(
        "simple_search_renders_results_navigates_to_an_abstract_and_opens_the_paper",
        r##"(let* ((arxiv-pop-up-new-frame nil)
       (arxiv-startup-with-abstract-window nil)
       (arxiv-use-variable-pitch nil)
       (arxiv-author-list-maximum 2)
       (arxiv-entries-per-fetch 25)
       (xml
        "<?xml version=\"1.0\"?>
<feed>
 <opensearch:totalResults>2</opensearch:totalResults>
 <opensearch:startIndex>0</opensearch:startIndex>
 <opensearch:itemsPerPage>2</opensearch:itemsPerPage>
 <entry>
  <id>http://arxiv.org/abs/2501.00001</id>
  <updated>2025-01-03T04:05:06Z</updated>
  <published>2025-01-02T03:04:05Z</published>
  <title>Executable Editors in Practice</title>
  <summary>We study $x+y=z$ in a complete editor workflow.</summary>
  <author><name>Ada Lovelace</name></author>
  <author><name>Grace Hopper</name></author>
  <arxiv:comment>18 pages, 4 figures</arxiv:comment>
  <arxiv:journal_ref>Journal of Editor Systems 12</arxiv:journal_ref>
  <category term=\"cs.PL\"/><category term=\"cs.SE\"/>
  <link href=\"http://arxiv.org/pdf/2501.00001\" title=\"pdf\"/>
 </entry>
 <entry>
  <id>http://arxiv.org/abs/2501.00002</id>
  <updated>2025-01-05T06:07:08Z</updated>
  <published>2025-01-04T05:06:07Z</published>
  <title>Deterministic Display Pipelines</title>
  <summary>Rendering buffers reproducibly across implementations.</summary>
  <author><name>Alan Turing</name></author>
  <author><name>Barbara Liskov</name></author>
  <author><name>Edsger Dijkstra</name></author>
  <category term=\"cs.SE\"/><category term=\"cs.PL\"/>
  <link href=\"http://arxiv.org/pdf/2501.00002\" title=\"pdf\"/>
 </entry>
</feed>")
       requests browser-calls response-buffers list-buffer abstract-buffer)
  (cl-letf
      (((symbol-function 'read-string)
        (lambda (prompt &rest arguments)
          (unless
              (equal
               prompt
               "Search all fields (use space to seperate and \"\" to quote): ")
            (error "Unexpected search prompt: %S" prompt))
          (unless (null arguments)
            (error "Unexpected search arguments: %S" arguments))
          "proof assistants \"editor semantics\""))
       ((symbol-function 'url-retrieve-synchronously)
        (lambda (url &rest arguments)
          (push (cons url arguments) requests)
          (let ((buffer (generate-new-buffer " *arxiv-mode-search-response*")))
            (push buffer response-buffers)
            (with-current-buffer buffer
              (insert xml))
            buffer)))
       ((symbol-function 'browse-url)
        (lambda (url &rest arguments)
          (push (cons url arguments) browser-calls)
          'opened)))
    (unwind-protect
        (progn
          (call-interactively #'arxiv-search)
          (setq list-buffer arxiv-buffer)
          (let (initial-state navigated-state abstract-state)
            (with-current-buffer list-buffer
              (setq initial-state
                    (list
                     major-mode
                     (buffer-substring-no-properties (point-min) (point-max))
                     arxiv-query-data-list
                     arxiv-query-info
                     arxiv-query-total-results
                     arxiv-query-results-min
                     arxiv-query-results-max
                     arxiv-current-entry
                     (list
                      (overlay-start arxiv-highlight-overlay)
                      (overlay-end arxiv-highlight-overlay))
                     (mapcar
                      (lambda (key)
                        (key-binding (kbd key)))
                      '("n" "p" "SPC" "RET" "d" "e" "b" "B"))))
              (call-interactively (key-binding (kbd "n")))
              (setq navigated-state
                    (list
                     arxiv-current-entry
                     (line-number-at-pos)
                     (list
                      (overlay-start arxiv-highlight-overlay)
                      (overlay-end arxiv-highlight-overlay))))
              (call-interactively (key-binding (kbd "SPC")))
              (setq abstract-buffer arxiv-abstract-buffer)
              (call-interactively (key-binding (kbd "RET"))))
            (with-current-buffer abstract-buffer
              (setq abstract-state
                    (list
                     major-mode
                     header-line-format
                     (buffer-substring-no-properties (point-min) (point-max))
                     (let ((position (point-min))
                           button buttons)
                       (while (setq button (next-button position))
                         (push
                          (list
                           (button-label button)
                           (button-get button 'help-echo)
                           (button-get button 'follow-link))
                          buttons)
                         (setq position (button-end button)))
                       (nreverse buttons))
                     (window-live-p arxiv-abstract-window)
                     (window-dedicated-p arxiv-abstract-window))))
            (list
             initial-state
             navigated-state
             abstract-state
             (nreverse requests)
             (nreverse browser-calls))))
      (when (window-live-p arxiv-abstract-window)
        (delete-window arxiv-abstract-window))
      (setq arxiv-abstract-window nil)
      (when (buffer-live-p abstract-buffer)
        (kill-buffer abstract-buffer))
      (when (buffer-live-p list-buffer)
        (switch-to-buffer (get-buffer-create "*scratch*"))
        (kill-buffer list-buffer))
      (mapc
       (lambda (buffer)
         (when (buffer-live-p buffer)
           (kill-buffer buffer)))
       response-buffers)
      (setq arxiv-buffer nil
            arxiv-abstract-buffer nil
            arxiv-highlight-overlay nil))))"##,
        expect![[
            r#"OK ((arxiv-mode " Executable Editors in Practice\n Ada Lovelace, Grace Hopper\n 2025-01-02  [cs.PL] [cs.SE] \n\n Deterministic Display Pipelines\n Alan Turing, Barbara Liskov, et al.\n 2025-01-04  [cs.SE] [cs.PL] \n\n" ((all t "proof assistants \"editor semantics\"")) "all:proof assistants \"editor semantics\"" 2 1 2 0 (1 92) (arxiv-next-entry arxiv-prev-entry arxiv-SPC arxiv-open-current-url arxiv-download-pdf arxiv-download-pdf-export-bibtex arxiv-export-bibtex arxiv-export-bibtex-to-buffer)) (1 5 (92 193)) (arxiv-abstract-mode " arXiv:2501.00002" "\nDeterministic Display Pipelines\n\nAlan Turing, Barbara Liskov, Edsger Dijkstra\n\n    Rendering buffers reproducibly across implementations.\n\nComments: N/A\nSubjects: Software Engineering (cs.SE); Programming Languages (cs.PL)\nSubmitted: 2025-01-04 05:06:07 \nUpdated: 2025-01-05 06:07:08 " (("Deterministic Display Pipelines" "Link: http://arxiv.org/abs/2501.00002" t) ("Alan Turing" "Look up author: Alan Turing" t) ("Barbara Liskov" "Look up author: Barbara Liskov" t) ("Edsger Dijkstra" "Look up author: Edsger Dijkstra" t)) t t) (("http://export.arxiv.org/api/query?search_query=all:proof+assistants+%22editor+semantics%22&start=0&max_results=25")) (("http://arxiv.org/abs/2501.00002")))"#
        ]],
    )
}

fn result_key_downloads_the_pdf_and_appends_a_linked_bibtex_entry() -> ParityBatchCase {
    ParityBatchCase::value(
        "result_key_downloads_the_pdf_and_appends_a_linked_bibtex_entry",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (pdf-file (expand-file-name "chosen-paper.pdf" root))
       (bib-file (expand-file-name "library.bib" root))
       (pdf-bytes "%PDF-1.7\narxiv-mode practical fixture\n%%EOF\n")
       (arxiv-pop-up-new-frame nil)
       (arxiv-startup-with-abstract-window nil)
       (arxiv-default-download-folder root)
       (arxiv-default-bibliography bib-file)
       (arxiv-entries-per-fetch 10)
       (xml
        "<?xml version=\"1.0\"?>
<feed>
 <opensearch:totalResults>1</opensearch:totalResults>
 <opensearch:startIndex>0</opensearch:startIndex>
 <opensearch:itemsPerPage>1</opensearch:itemsPerPage>
 <entry>
  <id>http://arxiv.org/abs/2502.42424</id>
  <updated>2025-02-28T10:30:00Z</updated>
  <published>2025-02-27T09:20:00Z</published>
  <title>Practical Proof Assistants</title>
  <summary> A reproducible proof assistant workflow. </summary>
  <author><name>Ada Lovelace</name></author>
  <author><name>Alan M. Turing</name></author>
  <arxiv:doi>10.1000/proof.42424</arxiv:doi>
  <arxiv:journal_ref>Journal of Mechanized Reasoning</arxiv:journal_ref>
  <category term=\"cs.LO\"/><category term=\"cs.PL\"/>
  <link href=\"http://arxiv.org/pdf/2502.42424\" title=\"pdf\"/>
 </entry>
</feed>")
       requests boundary-calls response-buffers list-buffer)
  (with-temp-file bib-file
    (insert "% Team bibliography\n"))
  (cl-letf
      (((symbol-function 'read-string)
        (lambda (&rest _arguments)
          "proof assistant"))
       ((symbol-function 'url-retrieve-synchronously)
        (lambda (url &rest arguments)
          (push (cons url arguments) requests)
          (let ((buffer (generate-new-buffer " *arxiv-mode-export-response*")))
            (push buffer response-buffers)
            (with-current-buffer buffer
              (insert xml))
            buffer)))
       ((symbol-function 'read-file-name)
        (lambda (prompt directory &rest arguments)
          (push (list 'choose prompt directory arguments) boundary-calls)
          (cond
           ((string-prefix-p "save pdf as:" prompt) pdf-file)
           ((string-prefix-p "export to bibliography file:" prompt) bib-file)
           (t (error "Unexpected file prompt: %S" prompt)))))
       ((symbol-function 'url-copy-file)
        (lambda (url file overwrite)
          (push (list 'copy url file overwrite) boundary-calls)
          (with-temp-buffer
            (set-buffer-multibyte nil)
            (insert (string-as-unibyte pdf-bytes))
            (let ((coding-system-for-write 'no-conversion))
              (write-region (point-min) (point-max) file nil 'silent)))
          nil)))
    (unwind-protect
        (progn
          (call-interactively #'arxiv-search)
          (setq list-buffer arxiv-buffer)
          (with-current-buffer list-buffer
            (call-interactively (key-binding (kbd "e"))))
          (list
           (nreverse requests)
           (nreverse boundary-calls)
           (with-temp-buffer
             (set-buffer-multibyte nil)
             (insert-file-contents-literally pdf-file)
             (list
              (buffer-size)
              (secure-hash 'sha256 (current-buffer))
              (buffer-string)))
           (with-temp-buffer
             (insert-file-contents bib-file)
             (buffer-string))))
      (when (buffer-live-p list-buffer)
        (switch-to-buffer (get-buffer-create "*scratch*"))
        (kill-buffer list-buffer))
      (mapc
       (lambda (buffer)
         (when (buffer-live-p buffer)
           (kill-buffer buffer)))
       response-buffers)
      (dolist (file (list pdf-file bib-file))
        (when (file-exists-p file)
          (delete-file file)))
      (setq arxiv-buffer nil
            arxiv-highlight-overlay nil))))"##,
        expect![[
            r#"OK ((("http://export.arxiv.org/api/query?search_query=all:proof+assistant&start=0&max_results=10")) ((choose "save pdf as: " "[ORACLE-SANDBOX]" (nil nil "2502.42424.pdf")) (copy "http://arxiv.org/pdf/2502.42424" "[ORACLE-SANDBOX]/chosen-paper.pdf" 1) (choose "export to bibliography file: " "[ORACLE-SANDBOX]/library.bib" (nil confirm))) (44 "190d95b6d075f54ed65fe902b2fd51da639f6f63e830706cbbdea27fe7f36878" "%PDF-1.7\narxiv-mode practical fixture\n%%EOF\n") "% Team bibliography\n@article{lovelace25:_pract_proof_assis,\ntitle = {Practical Proof Assistants},\nauthor = {Lovelace, Ada and Turing, Alan M.},\nabstract = {A reproducible proof assistant workflow.},\narchivePrefix = {arXiv},\neprint = {2502.42424},\nurl = {http://arxiv.org/abs/2502.42424},\nyear = {2025},\ndoi = {10.1000/proof.42424},\njournal = {Journal of Mechanized Reasoning},\nfile = {:[ORACLE-SANDBOX]/chosen-paper.pdf:pdf}\n}\n")"#
        ]],
    )
}

fn daily_list_sorts_primary_submissions_then_fetches_and_renders_the_next_page() -> ParityBatchCase
{
    ParityBatchCase::value(
        "daily_list_sorts_primary_submissions_then_fetches_and_renders_the_next_page",
        r##"(let* ((arxiv-pop-up-new-frame nil)
       (arxiv-startup-with-abstract-window nil)
       (arxiv-use-variable-pitch nil)
       (arxiv-entries-per-fetch 2)
       (as-of (date-to-time "2024-01-08T21:00:00-05:00"))
       requests response-buffers list-buffer)
  (cl-letf
      (((symbol-function 'url-retrieve-synchronously)
        (lambda (url &rest arguments)
          (push (cons url arguments) requests)
          (let* ((second-page (string-match-p "&start=2&" url))
                 (buffer (generate-new-buffer " *arxiv-mode-daily-response*")))
            (push buffer response-buffers)
            (with-current-buffer buffer
              (insert
               (if second-page
                   "<?xml version=\"1.0\"?>
<feed>
 <opensearch:totalResults>3</opensearch:totalResults>
 <opensearch:startIndex>2</opensearch:startIndex>
 <opensearch:itemsPerPage>1</opensearch:itemsPerPage>
 <entry>
  <id>http://arxiv.org/abs/2401.00003</id>
  <updated>2024-01-08T18:00:00Z</updated>
  <published>2024-01-08T17:00:00Z</published>
  <title>Third Page Result</title>
  <summary>The paginated practical result.</summary>
  <author><name>Grace Hopper</name></author>
  <category term=\"cs.PL\"/>
  <link href=\"http://arxiv.org/pdf/2401.00003\" title=\"pdf\"/>
 </entry>
</feed>"
                 "<?xml version=\"1.0\"?>
<feed>
 <opensearch:totalResults>3</opensearch:totalResults>
 <opensearch:startIndex>0</opensearch:startIndex>
 <opensearch:itemsPerPage>2</opensearch:itemsPerPage>
 <entry>
  <id>http://arxiv.org/abs/2401.00001</id>
  <updated>2024-01-08T16:00:00Z</updated>
  <published>2024-01-08T15:00:00Z</published>
  <title>Cross Listed First</title>
  <summary>A cross-listed result.</summary>
  <author><name>Alan Turing</name></author>
  <category term=\"cs.AI\"/><category term=\"cs.PL\"/>
  <link href=\"http://arxiv.org/pdf/2401.00001\" title=\"pdf\"/>
 </entry>
 <entry>
  <id>http://arxiv.org/abs/2401.00002</id>
  <updated>2024-01-08T17:00:00Z</updated>
  <published>2024-01-08T16:00:00Z</published>
  <title>Primary Category First</title>
  <summary>A primary category result.</summary>
  <author><name>Ada Lovelace</name></author>
  <category term=\"cs.PL\"/><category term=\"cs.SE\"/>
  <link href=\"http://arxiv.org/pdf/2401.00002\" title=\"pdf\"/>
 </entry>
</feed>")))
            buffer))))
    (unwind-protect
        (progn
          (arxiv-read-new "cs.PL" as-of)
          (setq list-buffer arxiv-buffer)
          (let (initial-state paged-state)
            (with-current-buffer list-buffer
              (setq initial-state
                    (list
                     (mapcar
                      (lambda (entry) (alist-get 'title entry))
                      arxiv-entry-list)
                     arxiv-query-data-list
                     arxiv-query-info
                     arxiv-query-results-min
                     arxiv-query-results-max
                     arxiv-query-total-results
                     (buffer-substring-no-properties (point-min) (point-max))))
              (arxiv-next-entry 2)
              (setq paged-state
                    (list
                     arxiv-current-entry
                     (line-number-at-pos)
                     (mapcar
                      (lambda (entry) (alist-get 'title entry))
                      arxiv-entry-list)
                     (buffer-substring-no-properties (point-min) (point-max))
                     (list
                      (overlay-start arxiv-highlight-overlay)
                      (overlay-end arxiv-highlight-overlay)))))
            (list
             initial-state
             paged-state
             (nreverse requests))))
      (when (buffer-live-p list-buffer)
        (switch-to-buffer (get-buffer-create "*scratch*"))
        (kill-buffer list-buffer))
      (mapc
       (lambda (buffer)
         (when (buffer-live-p buffer)
           (kill-buffer buffer)))
       response-buffers)
      (setq arxiv-buffer nil
            arxiv-highlight-overlay nil))))"##,
        expect![[
            r#"OK ((("Primary Category First" "Cross Listed First") ((date-start . "202401051900") (date-end . "202401081900") (category . "cs.PL")) " Showing new submissions in cs.PL from 20240105(Fri) to 20240108(Mon)." 1 2 3 " Primary Category First\n Ada Lovelace\n 2024-01-08  [cs.PL] [cs.SE] \n\n Cross Listed First\n Alan Turing\n 2024-01-08  [cs.AI] [cs.PL] \n\n") (2 9 ("Primary Category First" "Cross Listed First" "Third Page Result") " Primary Category First\n Ada Lovelace\n 2024-01-08  [cs.PL] [cs.SE] \n\n Cross Listed First\n Alan Turing\n 2024-01-08  [cs.AI] [cs.PL] \n\n Third Page Result\n Grace Hopper\n 2024-01-08  [cs.PL] \n\n" (134 190)) (("http://export.arxiv.org/api/query?search_query=submittedDate:[202401051900+TO+202401081900]+AND+cat:cs.PL*&sortBy=submittedDate&sortOrder=ascending&start=0&max_results=2") ("http://export.arxiv.org/api/query?search_query=submittedDate:[202401051900+TO+202401081900]+AND+cat:cs.PL*&sortBy=submittedDate&sortOrder=ascending&start=2&max_results=2")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        simple_search_renders_results_navigates_to_an_abstract_and_opens_the_paper(),
        result_key_downloads_the_pdf_and_appends_a_linked_bibtex_entry(),
        daily_list_sorts_primary_submissions_then_fetches_and_renders_the_next_page(),
    ]
}
