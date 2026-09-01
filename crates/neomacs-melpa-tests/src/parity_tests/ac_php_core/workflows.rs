use expect_test::expect;

use super::ParityBatchCase;

/// Indexing, which everything else depends on.
///
/// `ac-php-remake-tags` resolves the project root, writes the configuration
/// file when it finds none -- the fixture deliberately does not write one, so
/// what is asserted is the package's own -- runs the indexer, and leaves an
/// index where the package looks for it.  The loaded result is then asserted
/// in full: the classes, the inheritance edges read from `extends`, the
/// namespaced function, and the file list the index's positions refer to.
fn indexing_a_project_writes_its_configuration_and_loads_every_class() -> ParityBatchCase {
    ParityBatchCase::value(
        "indexing_a_project_writes_its_configuration_and_loads_every_class",
        r##"
(let ((root (ac-php-test-make-project))
      (program (ac-php-test-install-php)))
  (ac-php-test-in-php-buffer
   "src/Service/Cart.php"
   (call-interactively 'ac-php-remake-tags)
   (let* ((finished (ac-php-test-wait-for-index))
          (tags-data (ac-php-get-tags-data))
          (cache (expand-file-name "cache" ac-php-test-root)))
     (list :indexer-finished finished
           :calls (mapcar (lambda (call)
                            (mapcar #'file-name-nondirectory call))
                          (ac-php-test-php-calls))
           :config (ac-php-test-read
                    (expand-file-name ".ac-php-conf.json" ac-php-test-project))
           :index-files (sort (mapcar #'file-name-nondirectory
                                      (directory-files-recursively cache ""))
                              #'string<)
           :progress ac-php-phptags-index-progress
           :classes (let (keys)
                      (maphash (lambda (key _value) (push key keys))
                               (ac-php-g--class-map tags-data))
                      (sort keys #'string<))
           :inheritance (let (edges)
                          (maphash (lambda (key value)
                                     (push (cons key (append value nil)) edges))
                                   (ac-php-g--inherit-map tags-data))
                          (sort edges (lambda (a b) (string< (car a) (car b)))))
           :functions (let (keys)
                        (maphash (lambda (key _value) (push key keys))
                                 (ac-php-g--function-map tags-data))
                        (sort keys #'string<))
           :indexed-files (mapcar (lambda (file) (file-relative-name file root))
                                  (append (ac-php-g--file-list tags-data) nil))))))
"##,
        expect![[
            r##"OK (:indexer-finished t :calls (("phpctags" ".ac-php-conf.json" "cache" "--rebuild=no" "--realpath_flag=yes")) :config "{\n  \"use-cscope\": null,\n  \"tag-dir\": null,\n  \"filter\": {\n    \"php-file-ext-list\": [\n      \"php\"\n    ],\n    \"php-path-list\": [\n      \".\"\n    ],\n    \"ignore-ruleset\": [\n      \"# like .gitignore file \",\n      \"/vendor/**/[tT]ests/**/*.php\",\n      \"/vendor/**/[Ee]xamples/**/*.php\",\n      \"/vendor/composer/*.php\",\n      \"/vendor/*.php\",\n      \"# not need php_codesniffer\",\n      \"/vendor/squizlabs/php_codesniffer/**/*.php\",\n      \"#  -- end -- \"\n    ]\n  }\n}" :index-files ("tags-vendor.el" "tags.el") :progress 83 :classes ("\\Shop\\Model\\Product" "\\Shop\\Service\\BaseCart" "\\Shop\\Service\\Cart") :inheritance (("\\Shop\\Service\\Cart" "\\Shop\\Service\\BaseCart")) :functions ("\\Shop\\Model\\Product" "\\Shop\\Model\\Product(" "\\Shop\\Model\\formatMoney(" "\\Shop\\Service\\BaseCart" "\\Shop\\Service\\BaseCart(" "\\Shop\\Service\\Cart" "\\Shop\\Service\\Cart(") :indexed-files ("src/Model/Product.php" "src/Service/BaseCart.php" "src/Service/Cart.php"))"##
        ]],
    )
}

fn the_project_root_is_found_by_any_marker_and_the_command_fails_without_one() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_project_root_is_found_by_any_marker_and_the_command_fails_without_one",
        r##"
(let ((base (expand-file-name "markers" ac-php-test-root))
      (program (ac-php-test-install-php)))
  (mapcar
   (lambda (case)
     (let* ((name (car case))
            (marker (cdr case))
            (directory (expand-file-name name base))
            (file (expand-file-name "src/deep/Thing.php" directory)))
       (make-directory (file-name-directory file) t)
       (ac-php-test-write file "<?php\nclass Thing {}\n")
       (when marker
         (ac-php-test-write (expand-file-name marker directory) ""))
       (let ((buffer (find-file-noselect file))
             (ac-php-gen-tags-flag nil)
             ;; The stand-in appends to one log across the whole form, so a
             ;; case reports only the invocations it added.
             (before (let ((calls (ac-php-test-php-calls)))
                       (if (listp calls) (length calls) 0))))
         (unwind-protect
             (with-current-buffer buffer
               (php-mode)
               (let ((outcome (ac-php-test-outcome
                               (progn (call-interactively 'ac-php-remake-tags) t))))
                 (ac-php-test-wait-for-index)
                 (list name
                       :outcome outcome
                       :indexed-root
                       (let* ((calls (ac-php-test-php-calls))
                              (added (nthcdr before (if (listp calls) calls nil))))
                         (mapcar (lambda (call)
                                   (let ((config (nth 1 call)))
                                     (and (stringp config)
                                          (string-match "markers/\\([^/]+\\)/" config)
                                          (match-string 1 config))))
                                 added)))))
           (kill-buffer buffer)))))
   '(("projectile" . ".projectile")
     ("conf" . ".ac-php-conf.json")
     ("composer" . "vendor/autoload.php")
     ("nothing" . nil))))
"##,
        expect![[
            r#"OK (("projectile" :outcome (:ok t) :indexed-root ("projectile")) ("conf" :outcome (:ok t) :indexed-root ("conf")) ("composer" :outcome (:ok t) :indexed-root ("composer")) ("nothing" :outcome (:error wrong-type-argument (stringp nil)) :indexed-root nil))"#
        ]],
    )
}

fn jumping_to_a_definition_needs_xref_and_then_the_location_stack_returns() -> ParityBatchCase {
    ParityBatchCase::value(
        "jumping_to_a_definition_needs_xref_and_then_the_location_stack_returns",
        r##"
(let ((root (ac-php-test-make-project))
      (program (ac-php-test-install-php)))
  (ac-php-test-in-php-buffer
   "src/Service/Cart.php"
   (call-interactively 'ac-php-remake-tags)
   (ac-php-test-wait-for-index)
   (cl-flet ((at-the-use-of-product ()
               (goto-char (point-min))
               (search-forward "new Product")
               (backward-char 3)))
     (at-the-use-of-product)
     (let ((without-xref (ac-php-test-outcome
                          (progn (call-interactively 'ac-php-find-symbol-at-point) t))))
       (require 'xref)
       (at-the-use-of-product)
       (let ((started (list (file-name-nondirectory (buffer-file-name))
                            (line-number-at-pos))))
         (call-interactively 'ac-php-find-symbol-at-point)
         (let ((arrived (list (file-name-nondirectory (buffer-file-name))
                              (line-number-at-pos)
                              (buffer-substring-no-properties
                               (line-beginning-position) (line-end-position)))))
           (call-interactively 'ac-php-location-stack-back)
           (list :without-xref without-xref
                 :started started
                 :arrived arrived
                 :returned (list (file-name-nondirectory (buffer-file-name))
                                 (line-number-at-pos))
                 :stack-depth (length ac-php-location-stack))))))))
"##,
        expect![[
            r#"OK (:without-xref (:error void-variable (find-tag-marker-ring)) :started ("Cart.php" 10) :arrived ("Product.php" 7 "class Product") :returned ("Cart.php" 10) :stack-depth 1)"#
        ]],
    )
}

fn the_type_at_point_is_resolved_from_the_buffer_and_the_index() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_type_at_point_is_resolved_from_the_buffer_and_the_index",
        r##"
(let ((root (ac-php-test-make-project))
      (program (ac-php-test-install-php)))
  (ac-php-test-in-php-buffer
   "src/Service/Cart.php"
   (call-interactively 'ac-php-remake-tags)
   (ac-php-test-wait-for-index)
   (mapcar
    (lambda (text)
      (goto-char (point-min))
      (search-forward "return $product;")
      (beginning-of-line)
      (let ((start (point)) resolved)
        (insert text)
        (setq resolved (ac-php-get-class-at-point (ac-php-get-tags-data)))
        (delete-region start (point))
        (list text (if (stringp resolved) (substring-no-properties resolved) resolved))))
    '("$this->" "$product->" "Product::" "self::" "parent::"))))
"##,
        expect![[
            r#"OK (("$this->" "\\Shop\\Service\\Cart.") ("$product->" "\\Shop\\Model\\Product.") ("Product::" "\\Shop\\Model\\Product.") ("self::" "\\Shop\\Service\\Cart.") ("parent::" "\\Shop\\Service\\Cart.__parent__."))"#
        ]],
    )
}

fn a_missing_vendor_index_turns_every_lookup_into_a_type_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_missing_vendor_index_turns_every_lookup_into_a_type_error",
        r##"
(let ((root (ac-php-test-make-project))
      (program (ac-php-test-install-php)))
  (ac-php-test-in-php-buffer
   "src/Service/Cart.php"
   (call-interactively 'ac-php-remake-tags)
   (ac-php-test-wait-for-index)
   (let* ((cache (expand-file-name "cache" ac-php-test-root))
          (directory (car (directory-files cache t "^tags-")))
          (tags-file (expand-file-name "tags.el" directory))
          (vendor-file (expand-file-name "tags-vendor.el" directory)))
     (list :both-present (list (file-exists-p tags-file) (file-exists-p vendor-file))
           :loads-with-vendor (and (ac-php-get-tags-data) t)
           :after-deleting-the-vendor-index
           (progn
             (delete-file vendor-file)
             (setq ac-php-tag-last-data-list nil)
             (list :load-data
                   (ac-php-test-outcome
                    (and (ac-php-load-data tags-file vendor-file
                                           (directory-file-name ac-php-test-project))
                         t))
                   :tags-data-returns (ac-php-get-tags-data)
                   :class-at-point
                   (ac-php-test-outcome
                    (ac-php-get-class-at-point (ac-php-get-tags-data)))))))))
"##,
        expect![
            "OK (:both-present (t t) :loads-with-vendor t :after-deleting-the-vendor-index (:load-data (:error wrong-type-argument (hash-table-p nil)) :tags-data-returns ac-php-phptags-index-process-filter :class-at-point (:ok nil)))"
        ],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        indexing_a_project_writes_its_configuration_and_loads_every_class(),
        the_project_root_is_found_by_any_marker_and_the_command_fails_without_one(),
        jumping_to_a_definition_needs_xref_and_then_the_location_stack_returns(),
        the_type_at_point_is_resolved_from_the_buffer_and_the_index(),
        a_missing_vendor_index_turns_every_lookup_into_a_type_error(),
    ]
}
