use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, WEB_COMPLETION_DATA_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const WEB_COMPLETION_DATA_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const WEB_COMPLETION_DATA_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defun web-completion-data-parity-resolve-source (entry)
  (let ((location (cdr entry)))
    (if (symbolp location) (symbol-value location) location)))

(defun web-completion-data-parity-read (path)
  (when (file-exists-p path)
    (with-temp-buffer
      (let ((coding-system-for-read 'utf-8))
        (insert-file-contents path))
      (buffer-string))))

(defun web-completion-data-parity-lines (path)
  (let ((contents (web-completion-data-parity-read path)))
    (and contents (split-string contents "\n" t))))

(defun web-completion-data-parity-file (source relative)
  (expand-file-name relative source))

(defun web-completion-data-parity-source-lines (source relative)
  (web-completion-data-parity-lines
   (web-completion-data-parity-file source relative)))

(defun web-completion-data-parity-entry (line framework)
  "Decode LINE using company-web's public data convention."
  (if (string-match "\\(.*?\\) \\(.*\\)" line)
      (list :name (match-string 1 line)
            :framework framework
            :doc (replace-regexp-in-string
                  "\\\\n" "\n" (match-string 2 line)))
    (list :name line :framework framework :doc nil)))

(defun web-completion-data-parity-entries (source relative framework)
  (mapcar (lambda (line)
            (web-completion-data-parity-entry line framework))
          (or (web-completion-data-parity-source-lines source relative)
              nil)))

(defun web-completion-data-parity-names (entries)
  (mapcar (lambda (entry) (plist-get entry :name)) entries))

(defun web-completion-data-parity-base-source ()
  (web-completion-data-parity-resolve-source
   (assoc "html" web-completion-data-sources)))

(defun web-completion-data-parity-source-entries (relative)
  "Read RELATIVE across registered sources in registry order."
  (let (entries)
    (dolist (source web-completion-data-sources entries)
      (let* ((framework (car source))
             (directory
              (web-completion-data-parity-resolve-source source))
             (path (and directory
                        (web-completion-data-parity-file
                         directory relative))))
        (when (and path (file-exists-p path))
          (setq entries
                (append entries
                        (web-completion-data-parity-entries
                         directory relative framework))))))))

(defun web-completion-data-parity-attributes (source tag framework)
  (append
   (web-completion-data-parity-entries
    source (format "html-attributes-list/%s" tag) framework)
   (web-completion-data-parity-entries
    source "html-attributes-list/global" (concat framework ", G"))))

(defun web-completion-data-parity-doc-summary (text)
  (and text
       (list :characters (length text)
             :lines (length (split-string text "\n"))
             :first-line (car (split-string text "\n"))
             :last-nonempty
             (car (last (split-string text "\n" t))))))

(defun web-completion-data-parity-prefix ()
  (buffer-substring-no-properties
   (save-excursion
     (skip-chars-backward "[:alnum:]_-" (line-beginning-position))
     (point))
   (point)))

(defun web-completion-data-parity-complete-prefix (replacement)
  (let ((end (point)))
    (skip-chars-backward "[:alnum:]_-" (line-beginning-position))
    (delete-region (point) end)
    (insert replacement)))

(defun web-completion-data-parity-test-root ()
  (let ((root
         (file-name-as-directory
          (expand-file-name
           "web-completion-data-extension"
           (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-directory-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun web-completion-data-parity-write (root relative contents)
  (let ((path (expand-file-name relative root)))
    (make-directory (file-name-directory path) t)
    (with-temp-file path
      (insert contents))
    path))
"####;

fn web_completion_data_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(WEB_COMPLETION_DATA_MELPA_PIN, "web-completion-data.el")
        .expect("prepare pinned web-completion-data source below ./tmp")
        .with_prelude(WEB_COMPLETION_DATA_TEST_PRELUDE)
        .with_timeout(WEB_COMPLETION_DATA_TEST_TIMEOUT)
}

fn installed_source_registry_resolves_the_complete_data_layout() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((entry (assoc "html" web-completion-data-sources))
       (source (web-completion-data-parity-resolve-source entry))
       (directories
        (sort
         (cl-remove-if-not
          (lambda (name)
            (file-directory-p (expand-file-name name source)))
          (directory-files source nil "\\`[^.]"))
         #'string<)))
  (list
   :registry web-completion-data-sources
   :location-kind (if (symbolp (cdr entry)) 'symbol 'string)
   :resolved-to-declared-source
   (equal source web-completion-data-html-source-dir)
   :source-exists (file-directory-p source)
   :directories directories
   :tag-list-exists
   (file-regular-p (expand-file-name "html-tag-list" source))
   :artifact-counts
   (mapcar
    (lambda (directory)
      (cons directory
            (length
             (directory-files
              (expand-file-name directory source)
              nil "\\`[^.]"))))
    directories)))
"####;
    let expect = expect![[
        r####"OK (:registry (("html" . web-completion-data-html-source-dir)) :location-kind symbol :resolved-to-declared-source t :source-exists t :directories ("html-attributes-complete" "html-attributes-list" "html-attributes-short-docs" "html-tag-short-docs") :tag-list-exists t :artifact-counts (("html-attributes-complete" . 61) ("html-attributes-list" . 70) ("html-attributes-short-docs" . 162) ("html-tag-short-docs" . 138)))"####
    ]];
    ParityBatchCase::value(
        "installed_source_registry_resolves_the_complete_data_layout",
        elisp_form,
        expect,
    )
}

fn author_completes_a_main_element_and_opens_its_documentation() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((source (web-completion-data-parity-base-source))
       (tag-lines
        (web-completion-data-parity-source-lines source "html-tag-list"))
       (tags
        (web-completion-data-parity-names
         (mapcar (lambda (line)
                   (web-completion-data-parity-entry line "html"))
                 tag-lines)))
       (doc
        (web-completion-data-parity-read
         (expand-file-name "html-tag-short-docs/main" source))))
  (with-temp-buffer
    (html-mode)
    (insert "<!doctype html>\n<html>\n<body>\n  <ma")
    (let* ((prefix (web-completion-data-parity-prefix))
           (candidates (all-completions prefix tags)))
      (web-completion-data-parity-complete-prefix "main")
      (insert ">Dashboard</main>\n</body>\n</html>\n")
      (list :prefix prefix
            :candidates candidates
            :tag-count (length tags)
            :first-and-last (list (car tags) (car (last tags)))
            :completed-document (buffer-string)
            :documentation
            (web-completion-data-parity-doc-summary doc)))))
"####;
    let expect = expect![[
        r####"OK (:prefix "ma" :candidates ("main" "map" "mark" "marquee") :tag-count 147 :first-and-last ("a" "xmp") :completed-document "<!doctype html>\n<html>\n<body>\n  <main>Dashboard</main>\n</body>\n</html>\n" :documentation (:characters 1050 :lines 21 :first-line "The HTML <main> element represents the main content of  the <body> of a document or application. The main content area consists of content that is directly related to, or expands upon the central topic of a document or the central functionality of an application. This content should be unique to the document, excluding any content that is repeated across a set of documents such as sidebars, navigation links, copyright information, site logos, and search forms (unless, of course, the document's main function is as a search form)." :last-nonempty "HTMLElement"))"####
    ]];
    ParityBatchCase::value(
        "author_completes_a_main_element_and_opens_its_documentation",
        elisp_form,
        expect,
    )
}

fn form_author_completes_input_attributes_with_inline_and_long_docs() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((source (web-completion-data-parity-base-source))
       (attributes
        (web-completion-data-parity-attributes source "input" "html"))
       (names (web-completion-data-parity-names attributes))
       (long-doc
        (web-completion-data-parity-read
         (expand-file-name
          "html-attributes-short-docs/input-autocomplete" source))))
  (with-temp-buffer
    (html-mode)
    (insert "<form action=\"/checkout\">\n  <input type=\"email\" au")
    (let* ((prefix (web-completion-data-parity-prefix))
           (candidates (all-completions prefix names))
           (autosave (cl-find "autosave" attributes
                              :key (lambda (entry)
                                     (plist-get entry :name))
                              :test #'string=)))
      (web-completion-data-parity-complete-prefix "autocomplete")
      (insert "=\"email\" required>\n</form>\n")
      (list
       :prefix prefix
       :candidates candidates
       :attribute-count (length attributes)
       :duplicates
       (cl-loop for name in (delete-dups (copy-sequence names))
                for count = (cl-count name names :test #'string=)
                when (> count 1)
                collect (cons name count))
       :autosave-inline-doc
       (plist-get autosave :doc)
       :autocomplete-long-doc
       (web-completion-data-parity-doc-summary long-doc)
       :completed-form (buffer-string)))))
"####;
    let expect = expect![[
        r####"OK (:prefix "au" :candidates ("autocapitalize" "autocomplete" "autocorrect" "autofocus" "autosave") :attribute-count 92 :duplicates (("spellcheck" . 2)) :autosave-inline-doc "autosave [HTML5]\n\nThis attribute should be defined as a unique value. If the value of the type attribute is search, previous search term values will persist in the dropdown across page load." :autocomplete-long-doc (:characters 1455 :lines 10 :first-line "autocomplete [HTML5]" :last-nonempty "The autocomplete attribute also controls whether Firefox will, unlike other browsers, persist the dynamic disabled state and (if applicable) dynamic checkedness of an <input> across page loads. The persistence feature is enabled by default. Setting the value of the autocomplete attribute to off disables this feature; this works even when the autocomplete attribute would normally not apply to the <input> by virtue of its type. See bug 654072.") :completed-form "<form action=\"/checkout\">\n  <input type=\"email\" autocomplete=\"email\" required>\n</form>\n")"####
    ]];
    ParityBatchCase::value(
        "form_author_completes_input_attributes_with_inline_and_long_docs",
        elisp_form,
        expect,
    )
}

fn author_completes_input_types_and_link_targets_with_value_docs() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((source (web-completion-data-parity-base-source))
       (input-types
        (web-completion-data-parity-entries
         source "html-attributes-complete/input-type" "html"))
       (targets
        (web-completion-data-parity-entries
         source "html-attributes-complete/a-target" "html")))
  (with-temp-buffer
    (html-mode)
    (insert "<input name=\"appointment\" type=\"d")
    (let* ((type-prefix (web-completion-data-parity-prefix))
           (type-candidates
            (all-completions
             type-prefix
             (web-completion-data-parity-names input-types))))
      (web-completion-data-parity-complete-prefix "datetime-local")
      (insert "\">\n<a href=\"/receipt\" target=\"_")
      (let* ((target-prefix (web-completion-data-parity-prefix))
             (target-candidates
              (all-completions
               target-prefix
               (web-completion-data-parity-names targets))))
        (web-completion-data-parity-complete-prefix "_blank")
        (insert "\">Receipt</a>\n")
        (list
         :type-prefix type-prefix
         :type-candidates type-candidates
         :type-docs
         (mapcar
          (lambda (name)
            (let ((entry
                   (cl-find name input-types
                            :key (lambda (candidate)
                                   (plist-get candidate :name))
                            :test #'string=)))
              (list name (plist-get entry :doc))))
          type-candidates)
         :target-prefix target-prefix
         :target-candidates target-candidates
         :target-docs
         (mapcar
          (lambda (entry)
            (list (plist-get entry :name)
                  (plist-get entry :doc)))
          targets)
         :completed-fragment (buffer-string))))))
"####;
    let expect = expect![[
        r####"OK (:type-prefix "d" :type-candidates ("date" "datetime" "datetime-local") :type-docs (("date" "[HTML5]\nA date (year, month, day) with no time zone.\nA date control") ("datetime" "[HTML5]\nDefines a date and time control (year, month, day, hour, minute, second, and fraction of a second, based on UTC time zone)") ("datetime-local" "[HTML5]\nDefines a date and time control (year, month, day, hour, minute, second, and fraction of a second (no time zone)")) :target-prefix "_" :target-candidates ("_blank" "_parent" "_self" "_top") :target-docs (("_blank" "Load in a new window") ("_parent" "Load in the parent frameset") ("_self" "Load in the same frame as it was clicked") ("_top" "Load in the full body of the window")) :completed-fragment "<input name=\"appointment\" type=\"datetime-local\">\n<a href=\"/receipt\" target=\"_blank\">Receipt</a>\n")"####
    ]];
    ParityBatchCase::value(
        "author_completes_input_types_and_link_targets_with_value_docs",
        elisp_form,
        expect,
    )
}

fn buffer_local_framework_source_layers_project_candidates_over_html() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (web-completion-data-parity-test-root))
       (base-sources (copy-tree web-completion-data-sources)))
  (web-completion-data-parity-write
   root "html-tag-list"
   "app-shell Project application shell\nmodal-panel Modal with title and actions\n")
  (web-completion-data-parity-write
   root "html-attributes-list/modal-panel"
   "open Whether the modal starts open\ntitle Accessible heading\n")
  (web-completion-data-parity-write
   root "html-attributes-list/global"
   "data-test Stable test selector\n")
  (web-completion-data-parity-write
   root "html-attributes-complete/modal-panel-open"
   "false Keep the modal closed\ntrue Show the modal immediately\n")
  (with-temp-buffer
    (html-mode)
    (setq-local web-completion-data-sources
                (cons (cons "Project" root)
                      web-completion-data-sources))
    (insert "<m")
    (let* ((prefix (web-completion-data-parity-prefix))
           (tags
            (web-completion-data-parity-source-entries "html-tag-list"))
           (candidates
            (all-completions
             prefix (web-completion-data-parity-names tags)))
           (project-tag
            (cl-find "modal-panel" tags
                     :key (lambda (entry) (plist-get entry :name))
                     :test #'string=))
           (project-attributes
            (web-completion-data-parity-attributes
             root "modal-panel" "Project"))
           (project-values
            (web-completion-data-parity-entries
             root "html-attributes-complete/modal-panel-open" "Project")))
      (web-completion-data-parity-complete-prefix "modal-panel")
      (insert " open=\"true\" title=\"Cart\"></modal-panel>")
      (list
       :sources
       (mapcar (lambda (entry)
                 (list (car entry)
                       (if (symbolp (cdr entry)) 'symbol 'string)))
               web-completion-data-sources)
       :prefix prefix
       :candidates candidates
       :project-tag project-tag
       :project-attributes project-attributes
       :project-values project-values
       :completed-component (buffer-string)
       :buffer-local (local-variable-p 'web-completion-data-sources)
       :global-unchanged
       (equal base-sources
              (default-value 'web-completion-data-sources))))))
"####;
    let expect = expect![[
        r####"OK (:sources (("Project" string) ("html" symbol)) :prefix "m" :candidates ("modal-panel" "main" "map" "mark" "marquee" "menu" "menuitem" "meta" "meter" "multicol") :project-tag (:name "modal-panel" :framework "Project" :doc "Modal with title and actions") :project-attributes ((:name "open" :framework "Project" :doc "Whether the modal starts open") (:name "title" :framework "Project" :doc "Accessible heading") (:name "data-test" :framework "Project, G" :doc "Stable test selector")) :project-values ((:name "false" :framework "Project" :doc "Keep the modal closed") (:name "true" :framework "Project" :doc "Show the modal immediately")) :completed-component "<modal-panel open=\"true\" title=\"Cart\"></modal-panel>" :buffer-local t :global-unchanged t)"####
    ]];
    ParityBatchCase::value(
        "buffer_local_framework_source_layers_project_candidates_over_html",
        elisp_form,
        expect,
    )
}

fn shipped_indexes_form_a_consistent_completion_dataset() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((source (web-completion-data-parity-base-source))
       (tags
        (web-completion-data-parity-source-lines source "html-tag-list"))
       (tag-docs
        (directory-files
         (expand-file-name "html-tag-short-docs" source)
         nil "\\`[^.]"))
       (attribute-tags
        (directory-files
         (expand-file-name "html-attributes-list" source)
         nil "\\`[^.]"))
       (value-files
        (directory-files
         (expand-file-name "html-attributes-complete" source)
         nil "\\`[^.]"))
       (missing-tag-docs
        (cl-remove-if
         (lambda (tag) (member tag tag-docs)) tags))
       (orphan-tag-docs
        (cl-remove-if
         (lambda (doc) (member doc tags)) tag-docs))
       (attribute-tags-outside-index
        (cl-remove-if
         (lambda (tag)
           (or (string= tag "global") (member tag tags)))
         attribute-tags))
       malformed-values)
  (dolist (file value-files)
    (dolist (line
             (web-completion-data-parity-source-lines
              source (format "html-attributes-complete/%s" file)))
      (when (string=
             ""
             (plist-get
              (web-completion-data-parity-entry line "html") :name))
        (push (list file line) malformed-values))))
  (list :tag-count (length tags)
        :tag-doc-count (length tag-docs)
        :attribute-tag-count (length attribute-tags)
        :attribute-value-file-count (length value-files)
        :missing-tag-docs missing-tag-docs
        :orphan-tag-docs orphan-tag-docs
        :attribute-tags-outside-index attribute-tags-outside-index
        :malformed-value-rows (nreverse malformed-values)))
"####;
    let expect = expect![[
        r####"OK (:tag-count 147 :tag-doc-count 138 :attribute-tag-count 70 :attribute-value-file-count 61 :missing-tag-docs ("command" "href" "image" "multicol" "nextid" "noembed" "portfolio" "rb" "svg") :orphan-tag-docs nil :attribute-tags-outside-index ("insindex") :malformed-value-rows nil)"####
    ]];
    ParityBatchCase::value(
        "shipped_indexes_form_a_consistent_completion_dataset",
        elisp_form,
        expect,
    )
}

#[test]
fn web_completion_data_package_batch() {
    let cases = vec![
        installed_source_registry_resolves_the_complete_data_layout(),
        author_completes_a_main_element_and_opens_its_documentation(),
        form_author_completes_input_attributes_with_inline_and_long_docs(),
        author_completes_input_types_and_link_targets_with_value_docs(),
        buffer_local_framework_source_layers_project_candidates_over_html(),
        shipped_indexes_form_a_consistent_completion_dataset(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("unnamed web-completion-data parity test");
    assert_oracle_batch_cases(
        web_completion_data_oracle(),
        test_name,
        "web_completion_data_parity",
        &cases,
    );
}
