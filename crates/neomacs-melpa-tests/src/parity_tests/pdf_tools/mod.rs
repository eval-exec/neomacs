use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PDF_TOOLS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PDF_TOOLS_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PDF_TOOLS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'pdf-tools)
(require 'pdf-cache)
(require 'pdf-isearch)
(require 'pdf-links)
(require 'pdf-outline)
(require 'pdf-occur)
(require 'pdf-virtual)
(cl-letf (((symbol-function 'pdf-info-features)
           (lambda () '(markup-annotations))))
  (require 'pdf-annot))
"##;

fn pdf_tools_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PDF_TOOLS_MELPA_PIN, "pdf-tools.el")
        .expect("prepare pinned PDF Tools source below ./tmp")
        .with_prelude(PDF_TOOLS_TEST_PRELUDE)
        .with_timeout(PDF_TOOLS_TEST_TIMEOUT)
}

fn document_geometry_converts_page_regions_to_pixels_and_crop_coordinates() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((point-regions '((72 72 288 108)
                        (300 400 500 460)))
       (pixel-regions
        (pdf-util-scale-to point-regions '(612 . 792) '(1224 . 1584) #'round))
       (padded-regions (pdf-util-translate pixel-regions '(8 . 12)))
       (crop '(0.10 0.20 0.80 0.90))
       (page-selection '(0.17 0.27 0.45 0.55))
       (crop-relative
        (pdf-util-edges-transform crop page-selection t))
       (restored
        (pdf-util-edges-transform crop crop-relative)))
  (list
   :pixels pixel-regions
   :padded padded-regions
   :union (apply #'pdf-util-edges-union pixel-regions)
   :overlap (pdf-util-edges-intersection
             '(100 100 400 300) '(250 50 500 180))
   :overlap-area
   (pdf-util-edges-intersection-area
    '(100 100 400 300) '(250 50 500 180))
   :crop-relative crop-relative
   :roundtrip restored
   :inside (pdf-util-edges-inside-p crop '(0.45 . 0.55))
   :outside (pdf-util-edges-inside-p crop '(0.05 . 0.55))))
"##;
    let expect = expect![[
        r####"OK (:pixels ((144 144 576 216) (600 800 1000 920)) :padded ((152 156 584 228) (608 812 1008 932)) :union (144 144 1000 920) :overlap (250 100 400 180) :overlap-area 12000 :crop-relative (0.1 0.10000000000000002 0.49999999999999994 0.5000000000000001) :roundtrip (0.17 0.27 0.44999999999999996 0.55) :inside t :outside nil)"####
    ]];
    ParityBatchCase::value(
        "document_geometry_converts_page_regions_to_pixels_and_crop_coordinates",
        elisp_form,
        expect,
    )
}

fn search_queries_quote_pcre_and_preserve_word_and_hyphenation_intent() -> ParityBatchCase {
    let elisp_form = r##"
(let ((queries
       '("state-of-the-art"
         "C++ guide"
         "(draft)"
         "  punctuation!  "
         "")))
  (list
   :quoted
   (mapcar #'pdf-util-pcre-quote
           '("invoice.total" "a+b?(c)" "[section]" "path\\name"))
   :strict
   (mapcar (lambda (query)
             (pdf-isearch-word-search-regexp query nil "-­"))
           queries)
   :lax
   (mapcar (lambda (query)
             (pdf-isearch-word-search-regexp query t nil))
           queries)))
"##;
    let expect = expect![[
        r####"OK (:quoted ("invoice\\.total" "a\\+b\\?\\(c\\)" "\\[section\\]" "path\\\\name") :strict ("\\bs(?:[\\-­]\\n)?t(?:[\\-­]\\n)?a(?:[\\-­]\\n)?t(?:[\\-­]\\n)?e\\W+o(?:[\\-­]\\n)?f\\W+t(?:[\\-­]\\n)?h(?:[\\-­]\\n)?e\\W+a(?:[\\-­]\\n)?r(?:[\\-­]\\n)?t\\b" "\\bC\\W+g(?:[\\-­]\\n)?u(?:[\\-­]\\n)?i(?:[\\-­]\\n)?d(?:[\\-­]\\n)?e\\b" "\\W+d(?:[\\-­]\\n)?r(?:[\\-­]\\n)?a(?:[\\-­]\\n)?f(?:[\\-­]\\n)?t\\W+" "\\W+p(?:[\\-­]\\n)?u(?:[\\-­]\\n)?n(?:[\\-­]\\n)?c(?:[\\-­]\\n)?t(?:[\\-­]\\n)?u(?:[\\-­]\\n)?a(?:[\\-­]\\n)?t(?:[\\-­]\\n)?i(?:[\\-­]\\n)?o(?:[\\-­]\\n)?n\\W+" "") :lax ("state\\W+of\\W+the\\W+art" "C\\W+guide" "\\W+draft\\W+" "\\W+punctuation\\W+" ""))"####
    ]];
    ParityBatchCase::value(
        "search_queries_quote_pcre_and_preserve_word_and_hyphenation_intent",
        elisp_form,
        expect,
    )
}

fn outline_and_link_workflow_builds_nested_navigation_and_user_facing_actions() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((outline
        '(((depth . 1) (title . "Guide") (type . goto-dest) (page . 1))
          ((depth . 2) (title . "Install") (type . goto-dest) (page . 3))
          ((depth . 2) (title . "API") (type . goto-dest) (page . 8))
          ((depth . 1) (title . "Appendix") (type . goto-dest) (page . 12))))
       (labels '("i" "ii" "1" "2" "3" "4" "5" "6" "7" "8" "9" "A"))
       (tree (pdf-outline-treeify-outline-list outline)))
  (list
   :tree (copy-tree tree)
   :imenu (pdf-outline-imenu-create-index-tree-1 tree labels)
   :actions
   (mapcar
    #'pdf-links-action-to-string
    '(((type . goto-dest) (page . 8) (title . "API"))
      ((type . goto-dest) (page . 0) (title . "Broken bookmark"))
      ((type . uri) (uri . "https://example.org/spec") (title . "Specification"))
      ((type . uri) (uri . "") (title . "Empty"))
      ((type . goto-remote) (filename . "missing.pdf") (page . 4)
       (title . "Companion"))
      ((type . launch) (title . "Unsupported"))))))
"##;
    let expect = expect![[
        r####"OK (:tree ((((depth . 1) (title . "Guide") (type . goto-dest) (page . 1)) ((depth . 2) (title . "Install") (type . goto-dest) (page . 3)) ((depth . 2) (title . "API") (type . goto-dest) (page . 8))) ((depth . 1) (title . "Appendix") (type . goto-dest) (page . 12))) :imenu (("Guide" ("Guide (i)" 0 pdf-outline-imenu-activate-link ((depth . 1) (title . "Guide") (type . goto-dest) (page . 1))) ("Install (1)" 0 pdf-outline-imenu-activate-link ((depth . 2) (title . "Install") (type . goto-dest) (page . 3))) ("API (6)" 0 pdf-outline-imenu-activate-link ((depth . 2) (title . "API") (type . goto-dest) (page . 8)))) ("Appendix (A)" 0 pdf-outline-imenu-activate-link ((depth . 1) (title . "Appendix") (type . goto-dest) (page . 12)))) :actions ("Goto page 8 (API)" "Destination not found (Broken bookmark)" "Link to uri 'https://example.org/spec' (Specification)" "Link to empty uri (Empty)" "Link to nonexistent file 'missing.pdf' (Companion)" "Unrecognized link type: launch (Unsupported)"))"####
    ]];
    ParityBatchCase::value(
        "outline_and_link_workflow_builds_nested_navigation_and_user_facing_actions",
        elisp_form,
        expect,
    )
}

fn occurrence_search_normalizes_documents_and_splits_large_page_ranges_into_batches()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (draft-buffer (generate-new-buffer " *pdf-draft*")))
  (unwind-protect
      (progn
        (with-current-buffer draft-buffer
          (setq buffer-file-name (expand-file-name "reports/draft.pdf" root)))
        (cl-letf (((symbol-function 'pdf-info-number-of-pages)
                   (lambda (document)
                     (if (equal document "appendix.pdf") 53 37))))
          (let* ((normalized
                  (pdf-occur-normalize-documents
                   (list
                    (cons "zeta.pdf" '(2 . 9))
                    (cons draft-buffer '(4 . 11))
                    (cons "alpha.pdf" nil))))
                 (portable
                  (mapcar
                   (lambda (document)
                     (cons
                      (if (bufferp (car document))
                          (buffer-name (car document))
                        (file-relative-name (car document) root))
                      (cdr document)))
                   normalized))
                 (batches
                  (pdf-occur-create-batches
                   (list
                    (cons "manual.pdf" '(1 . 37))
                    (cons "appendix.pdf" '(20 . 0)))
                   16)))
            (list :normalized portable
                  :batches batches
                  :abbreviations
                  (mapcar #'pdf-occur-abbrev-document
                          (list "/docs/manual.pdf" "/docs/" draft-buffer))))))
    (kill-buffer draft-buffer)))
"##;
    let expect = expect![[
        r####"OK (:normalized (("alpha.pdf") ("reports/draft.pdf" 4 . 11) ("zeta.pdf" 2 . 9)) :batches (("manual.pdf" (1 . 16)) ("manual.pdf" (17 . 32)) ("manual.pdf" (33 . 37)) ("appendix.pdf" (20 . 35)) ("appendix.pdf" (36 . 51)) ("appendix.pdf" (52 . 53))) :abbreviations ("manual.pdf" "/docs/" " *pdf-draft*"))"####
    ]];
    ParityBatchCase::value(
        "occurrence_search_normalizes_documents_and_splits_large_page_ranges_into_batches",
        elisp_form,
        expect,
    )
}

fn virtual_document_page_specs_preserve_ranges_regions_and_open_ended_documents() -> ParityBatchCase
{
    let elisp_form = r##"
(cl-letf (((symbol-function 'pdf-info-number-of-pages)
           (lambda (&optional _filename) 42)))
  (let ((range
         (make-pdf-virtual-range
          :filename "manual.pdf"
          :first 4
          :last 11
          :region '(0.1 0.2 0.9 0.8)
          :index-start 7)))
    (list
     :normalized
     (mapcar
      #'pdf-virtual-pagespec-normalize
      '(7
        (3 . 9)
        ((3 . 9) 0 0 1 1)
        (5 0.10 0.20 0.80 0.90)
        (nil 0.25 0.25 0.75 0.75)))
     :range
     (list :length (pdf-virtual-range-length range)
           :filename (pdf-virtual-range-filename range)
           :first (pdf-virtual-range-first range)
           :last (pdf-virtual-range-last range)
           :region (pdf-virtual-range-region range)
           :index-start (pdf-virtual-range-index-start range)))))
"##;
    let expect = expect![[
        r####"OK (:normalized (((7 . 7)) ((3 . 9)) ((3 . 9)) ((5 . 5) 0.1 0.2 0.8 0.9) ((1 . 42) 0.25 0.25 0.75 0.75)) :range (:length 8 :filename "manual.pdf" :first 4 :last 11 :region (0.1 0.2 0.9 0.8) :index-start 7))"####
    ]];
    ParityBatchCase::value(
        "virtual_document_page_specs_preserve_ranges_regions_and_open_ended_documents",
        elisp_form,
        expect,
    )
}

fn annotation_review_merges_defaults_classifies_fields_and_orders_page_entries() -> ParityBatchCase
{
    let elisp_form = r##"
(with-temp-buffer
  (let* ((buffer (current-buffer))
         (first
          (list (cons 'buffer buffer) '(id . first) '(type . highlight) '(page . 2)
                '(edges . (0.60 0.20 0.80 0.30))
                '(markup-edges . ((0.60 0.20 0.80 0.30)))))
         (second
          (list (cons 'buffer buffer) '(id . second) '(type . text) '(page . 1)
                '(edges . (0.20 0.70 0.25 0.78))))
         (third
          (list (cons 'buffer buffer) '(id . third) '(type . underline) '(page . 2)
                '(edges . (0.10 0.20 0.40 0.25))
                '(markup-edges . ((0.10 0.20 0.40 0.25)))))
         (merged
          (pdf-annot-merge-alists
           '((color . "gold") (opacity . 0.8) (author . "reviewer"))
           '((color . "blue") (subject . "TODO") (opacity . 1.0)))))
    (list
     :merged merged
     :classification
     (mapcar
      (lambda (annotation)
        (list (pdf-annot-get-id annotation)
              :text (pdf-annot-text-annotation-p annotation)
              :markup (pdf-annot-markup-annotation-p annotation)
              :display (pdf-annot-get-display-edges annotation)
              :modifiable
              (mapcar
               (lambda (property)
                 (list
                  property
                  (and
                   (pdf-annot-property-modifiable-p annotation property)
                   t)))
               '(color opacity icon label subject))))
      (list first second third))
     :reading-order
     (mapcar #'pdf-annot-get-id
             (sort (list first second third)
                   #'pdf-annot-compare-annotations)))))
"##;
    let expect = expect![[
        r####"OK (:merged ((color . "gold") (opacity . 0.8) (author . "reviewer") (subject . "TODO")) :classification ((first :text nil :markup t :display ((0.6 0.2 0.8 0.3)) :modifiable ((color t) (opacity t) (icon nil) (label t) (subject nil))) (second :text t :markup t :display ((0.2 0.7 0.25 0.78)) :modifiable ((color t) (opacity t) (icon t) (label t) (subject nil))) (third :text nil :markup t :display ((0.1 0.2 0.4 0.25)) :modifiable ((color t) (opacity t) (icon nil) (label t) (subject nil)))) :reading-order (second third first))"####
    ]];
    ParityBatchCase::value(
        "annotation_review_merges_defaults_classifies_fields_and_orders_page_entries",
        elisp_form,
        expect,
    )
}

fn extracted_text_alignment_reconciles_pdf_words_with_source_transcripts() -> ParityBatchCase {
    let elisp_form = r##"
(let ((pdf-words '("The" "quick" "brown" "fox" "jumps"))
      (source-words '("The" "quick" "fox" "jumps" "today")))
  (list
   :whole (pdf-util-seq-alignment pdf-words source-words)
   :prefix
   (pdf-util-seq-alignment
    '("Chapter" "One") '("Chapter" "One" "Introduction") nil 'prefix)
   :suffix
   (pdf-util-seq-alignment
    '("final" "result") '("draft" "final" "result") nil 'suffix)
   :case-insensitive
   (pdf-util-seq-alignment
    '("PDF" "Tools" "Review")
    '("pdf" "tool" "review")
    (lambda (left right)
      (if (string-equal-ignore-case left right) 3 -2)))))
"##;
    let expect = expect![[
        r####"OK (:whole (2 ("The" . "The") ("quick" . "quick") ("brown") ("fox" . "fox") ("jumps" . "jumps") (nil . "today")) :prefix (2 ("Chapter" . "Chapter") ("One" . "One") (nil . "Introduction")) :suffix (2 (nil . "draft") ("final" . "final") ("result" . "result")) :case-insensitive (4 ("PDF" . "pdf") (nil . "tool") ("Tools") ("Review" . "review")))"####
    ]];
    ParityBatchCase::value(
        "extracted_text_alignment_reconciles_pdf_words_with_source_transcripts",
        elisp_form,
        expect,
    )
}

fn document_and_image_caches_distinguish_nil_hits_refresh_lru_and_evict_old_entries()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((pdf-cache-image-limit 3)
        (pdf-cache--data nil)
        (pdf-cache--image-cache nil))
    (pdf-cache--data-put 'outline nil)
    (pdf-cache--data-put 'text "page-1-v1" 1)
    (pdf-cache--data-put 'text "page-1-v2" 1)
    (pdf-cache--data-put 'text "page-2" 2)
    (let ((nil-hit (pdf-cache--data-get 'outline))
          (page-one (pdf-cache--data-get 'text 1))
          (missing (pdf-cache--data-get 'missing 1)))
      (pdf-cache-clear-data-of-pages 2)
      (pdf-cache-put-image 1 100 "page-1-small" 'clean)
      (pdf-cache-put-image 1 200 "page-1-large" 'clean)
      (pdf-cache-put-image 2 180 "page-2" 'clean)
      (let ((lookup (pdf-cache-lookup-image 1 150 nil 'clean))
            (recent (pdf-cache-get-image 1 150 nil 'clean)))
        (pdf-cache-put-image 3 220 "page-3" 'clean)
        (list
         :data
         (list :nil-hit nil-hit
               :page-one page-one
               :missing missing
               :page-two-after-clear (pdf-cache--data-get 'text 2))
         :images
         (list :lookup lookup
               :recent recent
               :evicted-small (pdf-cache-lookup-image 1 90 150 'clean)
               :wrong-hash (pdf-cache-lookup-image 1 150 nil 'dirty)
               :cache (mapcar #'copy-tree pdf-cache--image-cache)))))))
"##;
    let expect = expect![[
        r####"OK (:data (:nil-hit (t) :page-one (t . "page-1-v2") :missing (nil) :page-two-after-clear (nil)) :images (:lookup "page-1-large" :recent "page-1-large" :evicted-small nil :wrong-hash nil :cache ((3 220 "page-3" clean) (1 200 "page-1-large" clean) (2 180 "page-2" clean))))"####
    ]];
    ParityBatchCase::value(
        "document_and_image_caches_distinguish_nil_hits_refresh_lru_and_evict_old_entries",
        elisp_form,
        expect,
    )
}

#[test]
fn pdf_tools_package_batch() {
    let cases = vec![
        document_geometry_converts_page_regions_to_pixels_and_crop_coordinates(),
        search_queries_quote_pcre_and_preserve_word_and_hyphenation_intent(),
        outline_and_link_workflow_builds_nested_navigation_and_user_facing_actions(),
        occurrence_search_normalizes_documents_and_splits_large_page_ranges_into_batches(),
        virtual_document_page_specs_preserve_ranges_regions_and_open_ended_documents(),
        annotation_review_merges_defaults_classifies_fields_and_orders_page_entries(),
        extracted_text_alignment_reconciles_pdf_words_with_source_transcripts(),
        document_and_image_caches_distinguish_nil_hits_refresh_lru_and_evict_old_entries(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed PDF Tools parity test");
    assert_oracle_batch_cases(pdf_tools_oracle(), test_name, "pdf_tools_parity", &cases);
}
