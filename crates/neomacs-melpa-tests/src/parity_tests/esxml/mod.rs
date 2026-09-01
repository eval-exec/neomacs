//! Practical parity for esxml's public XML generation and query commands.
//!
//! These cases render a realistic catalog from esxml and sxml, parse a local
//! document, query it with CSS selectors, and recover after invalid forms
//! and unimplemented selectors.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ESXML_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'esxml)
(require 'esxml-query)
(set-window-configuration (current-window-configuration))

(defconst ex428-test-tree
  "dbd22a4cd32bf6cae3f94d9a1d1bdee8c84d539b")
(defconst ex428-test-manifest
  '(("esxml-pkg.el" . "70d8fd1ce6e0be6c6c9ae8d179e9bea32110b6fc2b46b13bdd69731520e7d854")
    ("esxml-query.el" . "fe11593b07b694449b1de6b2ce68356528c9cf31475e42247a3889f15971753c")
    ("esxml.el" . "517961c766213d879c3d4cb0178c9bb296c46e460f20847bd2589b471c985281")))

(defvar ex428-test-case-index 0)
(defvar ex428-test-root nil)
(defvar ex428-test-root-owned nil)

(defun ex428-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun ex428-test-source-state ()
  (let* ((located (locate-library "esxml.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (ex428-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/esxml.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car ex428-test-manifest)))
      (error "Unexpected installed esxml payload: %S" (or manifest files)))
    (dolist (entry ex428-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (ex428-test-sha file) expected))
          (error "Unexpected installed esxml source: %S"
                 (cons entry manifest)))))
    (list :tree ex428-test-tree
          :manifest manifest
          :feature (list (featurep 'esxml) (featurep 'esxml-query))
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'esxml package-alist)))))))

(defun ex428-test-condition (thunk)
  (condition-case condition
      (list :returned (funcall thunk))
    (error
     (list :error (car condition)
           :data (mapcar (lambda (item)
                           (if (stringp item)
                               (copy-sequence item)
                             (copy-tree item)))
                         (cdr condition))
           :message (copy-sequence (error-message-string condition))))))

(defun ex428-test-forbid-external (operation &rest arguments)
  (error "Unexpected esxml external boundary: %S %S" operation arguments))

(defun ex428-test-write (relative contents)
  (let ((file (expand-file-name relative ex428-test-root)))
    (unless (and ex428-test-root-owned
                 (file-in-directory-p file ex428-test-root))
      (error "Refusing esxml write outside owned root: %s" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file file (insert contents)))
    file))

(defun ex428-test-file-bytes (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (buffer-string)))

(defun ex428-test-node (node)
  (cond
   ((stringp node) (copy-sequence node))
   ((esxml-branch-p node)
    (list :tag (esxml-node-tag node)
          :attrs (copy-tree (esxml-node-attributes node))
          :children (mapcar #'ex428-test-node (esxml-node-children node))))
   (t (copy-tree node))))

(defun ex428-test-catalog ()
  '(html ((lang . "en-US"))
     (head ()
       (meta ((charset . "utf-8")))
       (link ((rel . "self") (href . "/catalog")))
       (title () "Café 界"))
     (body ()
       (form ((id . "search") (action . "/find") (method . "get"))
         (input ((type . "search") (name . "q") (value . "café"))))
       (table ()
         (thead ()
           (tr ((class . "row") (id . "heading"))
             (th ((class . "col")) "Key")
             (th ((class . "col")) "Value")))
         (tbody ()
           (tr ((class . "row even"))
             (td ((class . "col key")) "Café")
             (td ((class . "col value")) "1"))
           (tr ((class . "row odd") (id . "item-界"))
             (td ((class . "col key")) "界")
             (td ((class . "col value")) "2")))))))

(defun ex428-test-run (body)
  (let* ((index (cl-incf ex428-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "esxml-%d" index) sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (source-before (ex428-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (ex428-test-root root)
         (ex428-test-root-owned nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute esxml sandbox root"))
              (when (file-exists-p root)
                (error "esxml sandbox root exists: %S" root))
              (make-directory root)
              (setq ex428-test-root-owned t
                    enable-local-variables nil
                    debug-on-error nil
                    print-circle nil
                    default-directory root)
              (cl-letf (((symbol-function 'call-process)
                         (lambda (&rest args)
                           (apply #'ex428-test-forbid-external
                                  'call-process args)))
                        ((symbol-function 'call-process-region)
                         (lambda (&rest args)
                           (apply #'ex428-test-forbid-external
                                  'call-process-region args)))
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (apply #'ex428-test-forbid-external
                                  'make-process args)))
                        ((symbol-function 'process-file)
                         (lambda (&rest args)
                           (apply #'ex428-test-forbid-external
                                  'process-file args)))
                        ((symbol-function 'start-file-process)
                         (lambda (&rest args)
                           (apply #'ex428-test-forbid-external
                                  'start-file-process args)))
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (apply #'ex428-test-forbid-external
                                  'start-process args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (apply #'ex428-test-forbid-external
                                  'url-retrieve args)))
                        ((symbol-function 'url-retrieve-synchronously)
                         (lambda (&rest args)
                           (apply #'ex428-test-forbid-external
                                  'url-retrieve-synchronously args))))
                (setq result (funcall body root)))
              (setq source-after (ex428-test-source-state))
              (unless (equal source-before source-after)
                (error "esxml source changed")))
          (error (setq body-error
                       (list (car condition)
                             (copy-tree (cdr condition))))))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error (push (list label (car condition)
                                  (copy-tree (cdr condition)))
                            cleanup-errors)))))
        (setq enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before
              default-directory directory-before)
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda ()
                       (when (buffer-live-p buffer)
                         (with-current-buffer buffer
                           (set-buffer-modified-p nil))
                         (kill-buffer buffer))))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window
                 (lambda () (set-window-configuration window-before)))
        (when (window-live-p selected-window-before)
          (attempt 'selected
                   (lambda () (select-window selected-window-before))))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer
                   (lambda () (set-buffer buffer-before))))
        (when ex428-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (when body-error
      (error "esxml body failed: %S" body-error))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers (mapcar #'buffer-name
                                      (seq-remove
                                       (lambda (buffer)
                                         (memq buffer buffers-before))
                                       (buffer-list)))
                 :new-processes (length
                                 (seq-remove
                                  (lambda (process)
                                    (memq process processes-before))
                                  (process-list)))
                 :new-timers (length
                              (seq-remove
                               (lambda (timer)
                                 (memq timer timers-before))
                               (append timer-list timer-idle-list)))
                 :new-frames (length
                              (seq-remove
                               (lambda (frame)
                                 (memq frame frames-before))
                               (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored (eq (selected-window)
                                      selected-window-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "esxml cleanup failed: %S" (list result cleanup))
        (list :source source-before
              :result result
              :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ESXML_MELPA_PIN, "esxml.el")
        .expect("prepare pinned esxml source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn renders_a_catalog_page_from_esxml_and_sxml() -> ParityBatchCase {
    ParityBatchCase::value(
        "renders_a_catalog_page_from_esxml_and_sxml",
        r####"
(ex428-test-run
 (lambda (root)
   (let* ((page
           `(html ((lang . "en-US"))
              (head ()
                (title () "Café 界")
                (meta ((charset . "utf-8"))))
              (body ()
                (comment nil "ops ledger")
                (h1 () "Café catalog")
                (p () "Visit <script> is escaped")
                (p () (raw-string "<em>trusted</em>"))
                (img ((src . "/café.png") (alt . "界")))
                (a ((href . "/item?q=café&x=1")) "open"))))
         (sxml
          `(html (@ (lang "en-US"))
             (head
               (title "Café 界")
               (meta (@ (charset "utf-8"))))
             (body
               (*COMMENT* "ops ledger")
               (h1 "Café catalog")
               (p "Visit <script> is escaped")
               (p (*RAW-STRING* "<em>trusted</em>"))
               (img (@ (src "/café.png") (alt "界")))
               (a (@ (href "/item?q=café&x=1")) "open"))))
         (compact (esxml-to-xml page))
         (pretty (pp-esxml-to-xml page))
         (from-sxml (sxml-to-xml sxml))
         (as-esxml (sxml-to-esxml sxml))
         (file (ex428-test-write "catalog.html" compact)))
     (list :compact (copy-sequence compact)
           :pretty (copy-sequence pretty)
           :from-sxml (copy-sequence from-sxml)
           :sxml-round-trip (equal compact from-sxml)
           :sxml-esxml (copy-tree as-esxml)
           :validated (ex428-test-condition
                       (lambda () (esxml-validate-form page)))
           :file (copy-sequence (ex428-test-file-bytes file))
           :self-close (copy-sequence (esxml-to-xml '(br ())))
           :string-escape (copy-sequence (esxml-to-xml "<br>"))))))
"####,
        expect![[
            r#"OK (:source (:tree "dbd22a4cd32bf6cae3f94d9a1d1bdee8c84d539b" :manifest (("esxml-pkg.el" . "70d8fd1ce6e0be6c6c9ae8d179e9bea32110b6fc2b46b13bdd69731520e7d854") ("esxml-query.el" . "fe11593b07b694449b1de6b2ce68356528c9cf31475e42247a3889f15971753c") ("esxml.el" . "517961c766213d879c3d4cb0178c9bb296c46e460f20847bd2589b471c985281")) :feature (t t) :version "20260329.1617") :result (:compact "<html lang=\"en-US\"><head><title>Café 界</title><meta charset=\"utf-8\"/></head><body><!-- ops ledger --><h1>Café catalog</h1><p>Visit &lt;script&gt; is escaped</p><p><em>trusted</em></p><img src=\"/café.png\" alt=\"界\"/><a href=\"/item?q=café&amp;x=1\">open</a></body></html>" :pretty "<html lang=\"en-US\">\n  <head>\n    <title>Café 界</title>\n    <meta charset=\"utf-8\"/>\n  </head>\n  <body>\n    <!-- ops ledger -->\n    <h1>Café catalog</h1>\n    <p>Visit <script> is escaped</p>\n    <p>\n      <em>trusted</em>\n    </p>\n    <img src=\"/café.png\" alt=\"界\"/>\n    <a href=\"/item?q=café&amp;x=1\">open</a>\n  </body>\n</html>" :from-sxml "<html lang=\"en-US\"><head><title>Café 界</title><meta charset=\"utf-8\"/></head><body><!-- ops ledger --><h1>Café catalog</h1><p>Visit &lt;script&gt; is escaped</p><p><em>trusted</em></p><img src=\"/café.png\" alt=\"界\"/><a href=\"/item?q=café&amp;x=1\">open</a></body></html>" :sxml-round-trip t :sxml-esxml (html ((lang . "en-US")) (head nil (title nil "Café 界") (meta ((charset . "utf-8")))) (body nil (comment "ops ledger") (h1 nil "Café catalog") (p nil "Visit <script> is escaped") (p nil (raw-string "<em>trusted</em>")) (img ((src . "/café.png") (alt . "界"))) (a ((href . "/item?q=café&x=1")) "open"))) :validated (:error wrong-type-argument :data (attrs "<em>trusted</em>" attrs) :message "Wrong type argument: attrs, \"<em>trusted</em>\", attrs") :file "<html lang=\"en-US\"><head><title>Café 界</title><meta charset=\"utf-8\"/></head><body><!-- ops ledger --><h1>Café catalog</h1><p>Visit &lt;script&gt; is escaped</p><p><em>trusted</em></p><img src=\"/café.png\" alt=\"界\"/><a href=\"/item?q=café&amp;x=1\">open</a></body></html>" :self-close "<br/>" :string-escape "&lt;br&gt;") :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn queries_a_local_catalog_with_css_selectors() -> ParityBatchCase {
    ParityBatchCase::value(
        "queries_a_local_catalog_with_css_selectors",
        r####"
(ex428-test-run
 (lambda (root)
   (let* ((tree (ex428-test-catalog))
          (xml "<note><to>café</to><from>界</from></note>")
          (note (ex428-test-write "note.xml" (concat xml "\n")))
          (parsed (xml-to-esxml xml))
          (escaped (esxml-query-css-escape "item-界")))
     (list
      :wildcard (esxml-node-tag (esxml-query "*" tree))
      :title (car (esxml-node-children (esxml-query "title" tree)))
      :heading-key
      (car (esxml-node-children (esxml-query "table thead th" tree)))
      :child-cell
      (car (esxml-node-children (esxml-query "tbody>tr>td" tree)))
      :heading (esxml-node-tag (esxml-query "#heading" tree))
      :odd-value
      (car (esxml-node-children (esxml-query ".row.odd .value" tree)))
      :self-link (esxml-node-attribute 'href (esxml-query "[rel=self]" tree))
      :lang (esxml-node-attribute 'lang (esxml-query "[lang|=en]" tree))
      :even-row (and (esxml-query "[class~=even]" tree) t)
      :missing (esxml-query "foo, bar" tree)
      :comma-order
      (mapcar #'esxml-node-tag (esxml-query-all "tbody, thead" tree))
      :all-values
      (mapcar (lambda (node) (car (esxml-node-children node)))
              (esxml-query-all "td.value, td.key" tree))
      :forms (mapcar #'esxml-node-tag (esxml-get-forms tree))
      :by-id (esxml-node-tag
              (car (esxml-get-by-key tree 'id "heading")))
      :tags (mapcar #'esxml-node-tag (esxml-get-tags tree '(title form)))
      :escaped escaped
      :unicode-id
      (esxml-node-attribute
       'id (car (esxml-get-by-key tree 'id "item-界")))
      :parsed (ex428-test-node parsed)
      :parsed-to (car (esxml-node-children (esxml-query "to" parsed)))
      :note-on-disk (copy-sequence (ex428-test-file-bytes note))))))
"####,
        expect![[
            r#"OK (:source (:tree "dbd22a4cd32bf6cae3f94d9a1d1bdee8c84d539b" :manifest (("esxml-pkg.el" . "70d8fd1ce6e0be6c6c9ae8d179e9bea32110b6fc2b46b13bdd69731520e7d854") ("esxml-query.el" . "fe11593b07b694449b1de6b2ce68356528c9cf31475e42247a3889f15971753c") ("esxml.el" . "517961c766213d879c3d4cb0178c9bb296c46e460f20847bd2589b471c985281")) :feature (t t) :version "20260329.1617") :result (:wildcard html :title "Café 界" :heading-key "Key" :child-cell "Café" :heading tr :odd-value "2" :self-link "/catalog" :lang "en-US" :even-row t :missing nil :comma-order (thead tbody) :all-values ("Café" "1" "界" "2") :forms (form) :by-id tr :tags (title form) :escaped "item-界" :unicode-id "item-界" :parsed (:tag note :attrs nil :children ((:tag to :attrs nil :children ("café")) (:tag from :attrs nil :children ("界")))) :parsed-to "café" :note-on-disk "<note><to>café</to><from>界</from></note>\n") :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn parses_compound_selectors_and_escapes_identifiers() -> ParityBatchCase {
    ParityBatchCase::value(
        "parses_compound_selectors_and_escapes_identifiers",
        r####"
(ex428-test-run
 (lambda (_root)
   (list
    :star (copy-tree (esxml-parse-css-selector "*"))
    :compound (copy-tree (esxml-parse-css-selector "tr#heading.row.even"))
    :child (copy-tree (esxml-parse-css-selector "tbody>tr>td"))
    :attrs (copy-tree (esxml-parse-css-selector "[class^=row][charset$='8']"))
    :comma (copy-tree (esxml-parse-css-selector "thead, tbody"))
    :escaped-tag (copy-tree (esxml-parse-css-selector "foo\\.bar"))
    :hex (copy-tree (esxml-parse-css-selector "foo\\0030 bar"))
    :escape-null (esxml-query-css-escape "\0")
    :escape-dot (esxml-query-css-escape "foo.bar")
    :escape-space (esxml-query-css-escape "foo bar")
    :escape-leading-digit (esxml-query-css-escape "-1"))))
"####,
        expect![[
            r#"OK (:source (:tree "dbd22a4cd32bf6cae3f94d9a1d1bdee8c84d539b" :manifest (("esxml-pkg.el" . "70d8fd1ce6e0be6c6c9ae8d179e9bea32110b6fc2b46b13bdd69731520e7d854") ("esxml-query.el" . "fe11593b07b694449b1de6b2ce68356528c9cf31475e42247a3889f15971753c") ("esxml.el" . "517961c766213d879c3d4cb0178c9bb296c46e460f20847bd2589b471c985281")) :feature (t t) :version "20260329.1617") :result (:star ((((wildcard)))) :compound ((((tag . tr) (id . "heading") (class . "row") (class . "even")))) :child ((((tag . tbody)) ((combinator . child)) ((tag . tr)) ((combinator . child)) ((tag . td)))) :attrs ((((attribute (name . "class") (prefix-match . "row")) (attribute (name . "charset") (suffix-match . "8"))))) :comma ((((tag . thead))) (((tag . tbody)))) :escaped-tag ((((tag . foo.bar)))) :hex ((((tag . foo0bar)))) :escape-null "�" :escape-dot "foo\\.bar" :escape-space "foo\\ bar" :escape-leading-digit "-\\31 ") :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn invalid_forms_and_selectors_signal_then_recover() -> ParityBatchCase {
    ParityBatchCase::value(
        "invalid_forms_and_selectors_signal_then_recover",
        r####"
(ex428-test-run
 (lambda (_root)
   (let* ((short (ex428-test-condition
                  (lambda () (esxml-validate-form '(p)))))
          (silent (ex428-test-condition
                   (lambda () (esxml-to-xml '(p)))))
          (bad-attr (ex428-test-condition
                     (lambda () (esxml-to-xml '(p ((href . 1)) "x")))))
          (raw (ex428-test-condition
                (lambda () (esxml-to-xml '(raw-string 1)))))
          (empty (ex428-test-condition
                  (lambda () (esxml-parse-css-selector ""))))
          (two-ids (ex428-test-condition
                    (lambda () (esxml-parse-css-selector "foo#bar#baz"))))
          (trailing (ex428-test-condition
                     (lambda () (esxml-parse-css-selector "foo,"))))
          (period (ex428-test-condition
                   (lambda () (esxml-parse-css-selector ". .bar"))))
          (sibling (ex428-test-condition
                    (lambda ()
                      (esxml-query "tr+td" (ex428-test-catalog)))))
          (pseudo (ex428-test-condition
                   (lambda ()
                     (esxml-query ":hover" (ex428-test-catalog)))))
          (bare-cjk (ex428-test-condition
                     (lambda ()
                       (esxml-query "#item-界" (ex428-test-catalog)))))
          recovered)
     (setq recovered
           (list :xml (copy-sequence
                       (esxml-to-xml '(p ((id . "ok")) "café")))
                 :query (car (esxml-node-children
                              (esxml-query "p#ok"
                                           '(body ()
                                              (p ((id . "ok")) "café")))))))
     (list :short short
           :silent silent
           :bad-attr bad-attr
           :raw raw
           :empty empty
           :two-ids two-ids
           :trailing trailing
           :period period
           :sibling sibling
           :pseudo pseudo
           :bare-cjk bare-cjk
           :recovered recovered))))
"####,
        expect![[
            r#"OK (:source (:tree "dbd22a4cd32bf6cae3f94d9a1d1bdee8c84d539b" :manifest (("esxml-pkg.el" . "70d8fd1ce6e0be6c6c9ae8d179e9bea32110b6fc2b46b13bdd69731520e7d854") ("esxml-query.el" . "fe11593b07b694449b1de6b2ce68356528c9cf31475e42247a3889f15971753c") ("esxml.el" . "517961c766213d879c3d4cb0178c9bb296c46e460f20847bd2589b471c985281")) :feature (t t) :version "20260329.1617") :result (:short (:error error :data ("(p) is too short to be a valid esxml expression") :message "(p) is too short to be a valid esxml expression") :silent (:returned nil) :bad-attr (:error wrong-type-argument :data (attrs ((href . 1)) attrs) :message "Wrong type argument: attrs, ((href . 1)), attrs") :raw (:error wrong-type-argument :data (attrs 1 attrs) :message "Wrong type argument: attrs, 1, attrs") :empty (:error error :data ("Expected at least one selector") :message "Expected at least one selector") :two-ids (:error error :data ("Only one id selector allowed per compound") :message "Only one id selector allowed per compound") :trailing (:error error :data ("Expected selector after comma") :message "Expected selector after comma") :period (:error error :data ("Expected identifier after period") :message "Expected identifier after period") :sibling (:error error :data ("Unimplemented combinator ((combinator . direct-sibling))") :message "Unimplemented combinator ((combinator . direct-sibling))") :pseudo (:error error :data ("Unimplemented attribute type: pseudo-class") :message "Unimplemented attribute type: pseudo-class") :bare-cjk (:error error :data ("Invalid token detected: 界") :message "Invalid token detected: 界") :recovered (:xml "<p id=\"ok\">café</p>" :query "café")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn esxml_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        renders_a_catalog_page_from_esxml_and_sxml(),
        queries_a_local_catalog_with_css_selectors(),
        parses_compound_selectors_and_escapes_identifiers(),
        invalid_forms_and_selectors_signal_then_recover(),
    ];
    assert_oracle_batch_cases(oracle(), "esxml-rank428", "esxml_parity", &cases);
}
