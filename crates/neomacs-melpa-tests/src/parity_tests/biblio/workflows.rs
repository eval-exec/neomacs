use expect_test::expect;

use super::ParityBatchCase;

/// The lookup flow: the mocked HTTP transport serves the recorded
/// crossref payload and the package parses and renders it into the
/// results buffer.
fn the_lookup_renders_the_recorded_results() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_lookup_renders_the_recorded_results",
        r####"(let ((source-buffer (current-buffer)))
  (setq biblio-synchronous t)
  (cl-letf (((symbol-function 'url-retrieve-synchronously)
             #'biblio--test-http-mock))
    (let ((results (biblio-lookup #'biblio-crossref-backend
                                  "compiler design")))
      (list :source (biblio--test-source-state)
            :results-buffer (buffer-name results)
            :results-text
            (with-current-buffer results
              (buffer-substring-no-properties (point-min) (point-max)))
            :entry-titles
            (with-current-buffer results
              (let (titles)
                (save-excursion
                  (goto-char (point-min))
                  (while (not (eobp))
                    (when-let ((meta (get-text-property
                                      (point) 'biblio-metadata)))
                      (push (biblio-alist-get 'title meta) titles))
                    (forward-line 1)))
                (nreverse titles)))))))"####,
        expect![[
            r#"OK (:source (:upstream-tree "2e5baf3f77b588608f57b10a590ae213b645faf0" :feature t :version "20250812.1408") :results-buffer "*CrossRef search*" :results-text "CrossRef search results for ‘compiler design’\n> Design for Test\n  (no authors)\n  In: Advanced ASIC Chip Synthesis Using Synopsys® Design Compiler™ Physical Compiler™ and PrimeTime®\n  Type: book-chapter\n  Publisher: Kluwer Academic Publishers\n  URL: https://doi.org/10.1007/0-306-47507-3_8\n\n> Asic Design Methodology\n  (no authors)\n  In: Advanced ASIC Chip Synthesis Using Synopsys® Design Compiler™ Physical Compiler™ and PrimeTime®\n  Type: book-chapter\n  Publisher: Kluwer Academic Publishers\n  URL: https://doi.org/10.1007/0-306-47507-3_1\n\n> Advanced ASIC Chip Synthesis Using Synopsys® Design Compiler™ Physical Compiler™ and PrimeTime® [2002]\n  (no authors)\n  Type: book\n  Publisher: Kluwer Academic Publishers\n  URL: https://doi.org/10.1007/b117024\n\n" :entry-titles ("Design for Test" "Design for Test" "Design for Test" "Design for Test" "Design for Test" "Design for Test" "Design for Test" "Asic Design Methodology" "Asic Design Methodology" "Asic Design Methodology" "Asic Design Methodology" "Asic Design Methodology" "Asic Design Methodology" "Asic Design Methodology" "Advanced ASIC Chip Synthesis Using Synopsys® Design Compiler™ Physical Compiler™ and PrimeTime®" "Advanced ASIC Chip Synthesis Using Synopsys® Design Compiler™ Physical Compiler™ and PrimeTime®" "Advanced ASIC Chip Synthesis Using Synopsys® Design Compiler™ Physical Compiler™ and PrimeTime®" "Advanced ASIC Chip Synthesis Using Synopsys® Design Compiler™ Physical Compiler™ and PrimeTime®" "Advanced ASIC Chip Synthesis Using Synopsys® Design Compiler™ Physical Compiler™ and PrimeTime®" "Advanced ASIC Chip Synthesis Using Synopsys® Design Compiler™ Physical Compiler™ and PrimeTime®"))"#
        ]],
    )
}

/// The crossref backend builds the documented query URL.
fn the_backend_builds_the_documented_url() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_backend_builds_the_documented_url",
        r####"(let ((plain (biblio-crossref-backend 'url "compiler design"))
      (spaced (biblio-crossref-backend 'url "  a  b  ")))
  (list :plain plain
        :spaced spaced
        :prompt (biblio-crossref-backend 'prompt)))"####,
        expect![[
            r#"OK (:plain "https://api.crossref.org/works?query=compiler%20design" :spaced "https://api.crossref.org/works?query=%20%20a%20%20b%20%20" :prompt "CrossRef query: ")"#
        ]],
    )
}

/// The selection-insert flow: the entry at point is converted to
/// BibTeX and inserted into the source buffer.
fn the_selection_inserts_the_bibtex_into_the_source_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_selection_inserts_the_bibtex_into_the_source_buffer",
        r####"(let ((source-buffer (current-buffer)))
  (setq biblio-synchronous t)
  (cl-letf (((symbol-function 'url-retrieve-synchronously)
             #'biblio--test-http-mock))
    (biblio-lookup #'biblio-crossref-backend "compiler design"))
  (with-current-buffer "*CrossRef search*"
    (biblio--selection-first)
    (biblio--test-with-ui-capture
     (biblio--selection-insert)
     (list :source-text
           (with-current-buffer source-buffer
             (buffer-substring-no-properties (point-min) (point-max)))))))"####,
        expect![[
            r#"OK (:source-text "@InBook{1,\n  booktitle    = {Advanced ASIC Chip Synthesis Using Synopsys® Design\n                  Compiler™ Physical Compiler™ and PrimeTime®},\n  publisher    = {Kluwer Academic Publishers},\n  isbn\11       = 0792376447,\n  pages\11       = {151–173},\n  doi\11       = {10.1007/0-306-47507-3_8},\n  url\11       = {http://dx.doi.org/10.1007/0-306-47507-3_8}\n}\n\n")"#
        ]],
    )
}

/// The DOI and string helpers.
fn the_doi_and_string_helpers_normalize_their_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_doi_and_string_helpers_normalize_their_forms",
        r####"(list :strip (biblio-strip "  padded  ")
        :strip-nil (biblio-strip nil)
        :doi-plain (biblio-cleanup-doi "10.1007/0-306-47507-3_8")
        :doi-url (biblio-cleanup-doi "https://doi.org/10.1007/x")
        :doi-dx (biblio-cleanup-doi "https://dx.doi.org/10.1007/y")
        :join (biblio-join ", " "a" "" "b")
        :join-empty (biblio-join ", " ""))"####,
        expect![[
            r#"OK (:strip "padded" :strip-nil nil :doi-plain "10.1007/0-306-47507-3_8" :doi-url "10.1007/x" :doi-dx "10.1007/y" :join "a, b" :join-empty "")"#
        ]],
    )
}

/// The results buffer's metadata-at-point entry rejects positions
/// without an entry.
fn the_metadata_at_point_requires_an_entry() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_metadata_at_point_requires_an_entry",
        r####"(let ((caught nil))
  (with-temp-buffer
    (condition-case err
        (biblio--selection-metadata-at-point)
      (error (setq caught (list (car err) (cadr err)))))
    (list :error caught)))"####,
        expect![[r#"OK (:error (user-error "No entry at point"))"#]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_lookup_renders_the_recorded_results(),
        the_backend_builds_the_documented_url(),
        the_selection_inserts_the_bibtex_into_the_source_buffer(),
        the_doi_and_string_helpers_normalize_their_forms(),
        the_metadata_at_point_requires_an_entry(),
    ]
}
