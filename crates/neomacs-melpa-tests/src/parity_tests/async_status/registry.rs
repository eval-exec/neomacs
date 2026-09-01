use super::ParityBatchCase;
use expect_test::expect;

fn installed_descriptor_matches_the_exact_pin_commit_and_dependencies() -> ParityBatchCase {
    ParityBatchCase::value(
        "installed_descriptor_matches_the_exact_pin_commit_and_dependencies",
        r##"(let* ((description
        (cadr (assq 'async-status package-alist)))
       (requirements
        (mapcar
         (lambda (requirement)
           (list
            (car requirement)
            (package-version-join (cadr requirement))))
         (package-desc-reqs description))))
  (list
   (package-desc-name description)
   (package-version-join
    (package-desc-version description))
   (package-desc-summary description)
   requirements
   (package-desc-extras description)))"##,
        expect![[
            r#"OK (async-status "20230821.204" "A package for indicator support." ((emacs "28.1") (svg-lib "0.2.7") (posframe "1.4.2")) ((:maintainers ("Jason Kim" . "sukbeom.kim@gmail.com")) (:authors ("Jason Kim" . "sukbeom.kim@gmail.com")) (:keywords "tools" "async") (:revdesc . "d2f5becc9850") (:commit . "d2f5becc9850c26aa71fb581f9fc389eac740f52") (:url . "https://github.com/seokbeomkim/async-status")))"#
        ]],
    )
}

fn packaged_source_has_the_verified_melpa_payload_content_hash() -> ParityBatchCase {
    ParityBatchCase::value(
        "packaged_source_has_the_verified_melpa_payload_content_hash",
        r##"(let* ((description
        (cadr (assq 'async-status package-alist)))
       (source
        (expand-file-name
         "async-status.el"
         (package-desc-dir description))))
  (list
   (file-name-nondirectory source)
   (with-temp-buffer
     (set-buffer-multibyte nil)
     (insert-file-contents-literally source)
     (secure-hash 'sha256 (current-buffer)))
   (file-attribute-size
    (file-attributes source))))"##,
        expect![
            "OK (\"async-status.el\" \"ec9fe3f25a13458e26d206f12e61590ab2598fb067f9775db1ce26bcbd40df9b\" 8659)"
        ],
    )
}

fn complete_declared_function_surface_is_bound_after_loading() -> ParityBatchCase {
    ParityBatchCase::value(
        "complete_declared_function_surface_is_bound_after_loading",
        r##"(mapcar
 (lambda (symbol)
   (list symbol
         (fboundp symbol)
         (and
          (documentation symbol)
          t)
         (copy-tree
          (help-function-arglist symbol t))))
 '(async-status--get-absolute-path-by-id
   async-status-req-id
   async-status-clean-up
   async-status--get-msg-val
   async-status-safely-set-msg-val
   async-status-set-msg-val
   async-status-show
   async-status-hide
   async-status--print-truncated-string
   async-status--redraw-item
   async-status--refresh-status-bar
   async-status--update-items
   async-status--find-item-by-msgid
   async-status--remove-item
   async-status-add-item-to-bar
   async-status-remove-item-from-bar))"##,
        expect![[
            r#"OK ((async-status--get-absolute-path-by-id t t (id)) (async-status-req-id t t (name)) (async-status-clean-up t t (id)) (async-status--get-msg-val t t (id)) (async-status-safely-set-msg-val t t (id val &optional threshold)) (async-status-set-msg-val t t (id val)) (async-status-show t t nil) (async-status-hide t t (&optional force)) (async-status--print-truncated-string t t (str max-length)) (async-status--redraw-item t t (item)) (async-status--refresh-status-bar t t nil) (async-status--update-items t t (event)) (async-status--find-item-by-msgid t t (id)) (async-status--remove-item t t (item)) (async-status-add-item-to-bar t t (id &optional label)) (async-status-remove-item-from-bar t t (id)))"#
        ]],
    )
}

fn generated_item_struct_surface_is_complete_and_callable() -> ParityBatchCase {
    ParityBatchCase::value(
        "generated_item_struct_surface_is_complete_and_callable",
        r##"(mapcar
 (lambda (symbol)
   (list
    symbol
    (fboundp symbol)
    (copy-tree
     (help-function-arglist symbol t))))
 '(async-status--item-p
   make-async-status--item
   copy-async-status--item
   async-status--item-msg-id
   async-status--item-fs-watcher-id
   async-status--item-file-path
   async-status--item-progress
   async-status--item-label))"##,
        expect![[
            r#"OK ((async-status--item-p t (x)) (make-async-status--item t (&rest --cl-rest--)) (copy-async-status--item t (arg)) (async-status--item-msg-id t (x)) (async-status--item-fs-watcher-id t (x)) (async-status--item-file-path t (x)) (async-status--item-progress t (x)) (async-status--item-label t (x)))"#
        ]],
    )
}

fn variables_custom_types_defaults_and_group_metadata_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "variables_custom_types_defaults_and_group_metadata_match",
        r##"(list
 async-status--file-prefix
 async-status-indicator-width
 async-status-progress-bar-width
 async-status-progress-bar-height
 async-status--shown-items
 (get 'async-status-indicator-width 'custom-type)
 (get 'async-status-progress-bar-width 'custom-type)
 (get 'async-status-progress-bar-height 'custom-type)
 (get 'async-status-indicator-width 'custom-group)
 (get 'async-status 'custom-group)
 (get 'async-status 'group-documentation)
 (get 'async-status 'custom-links))"##,
        expect![[
            r#"OK ("async-status-" 462 150.0 20.0 nil integer float float nil ((async-status-indicator-width custom-variable) (async-status-progress-bar-width custom-variable) (async-status-progress-bar-height custom-variable)) "An indicator to display the status of Emacs processes." ((url-link "https://github.com/seokbeomKim/async-status")))"#
        ]],
    )
}

fn item_constructor_accessors_copy_and_mutation_have_value_semantics() -> ParityBatchCase {
    ParityBatchCase::value(
        "item_constructor_accessors_copy_and_mutation_have_value_semantics",
        r##"(let* ((item
        (make-async-status--item
         :msg-id "job-a"
         :fs-watcher-id '(watch . 7)
         :file-path "/sandbox/job-a"
         :progress 0.25
         :label "Compile"))
       (copy (copy-async-status--item item)))
  (setf
   (async-status--item-progress copy) 0.75
   (async-status--item-label copy) "Link")
  (list
   (async-status--item-p item)
   (async-status--item-p copy)
   (eq item copy)
   (equal item copy)
   (list
    (async-status--item-msg-id item)
    (equal
     (async-status--item-fs-watcher-id item)
     '(watch . 7))
    (file-name-nondirectory
     (async-status--item-file-path item))
    (async-status--item-progress item)
    (async-status--item-label item))
   (list
    (async-status--item-msg-id copy)
    (equal
     (async-status--item-fs-watcher-id copy)
     '(watch . 7))
    (file-name-nondirectory
     (async-status--item-file-path copy))
    (async-status--item-progress copy)
    (async-status--item-label copy))))"##,
        expect![[
            r#"OK (t t nil nil ("job-a" t "job-a" 0.25 "Compile") ("job-a" t "job-a" 0.75 "Link"))"#
        ]],
    )
}

fn item_constructor_accepts_partial_and_reordered_keyword_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "item_constructor_accepts_partial_and_reordered_keyword_arguments",
        r##"(let ((empty (make-async-status--item))
      (partial
       (make-async-status--item
        :label "Only label"
        :msg-id "partial")))
  (list
   (async-status-test-item-summary empty)
   (async-status-test-item-summary partial)
   (async-status--item-p '(async-status--item nil nil nil nil nil))
   (async-status--item-p [async-status--item nil nil nil nil nil])
   (async-status--item-p nil)))"##,
        expect!["OK ((nil nil nil nil nil) (\"partial\" nil nil nil \"Only label\") nil nil nil)"],
    )
}

fn repeated_source_loads_preserve_customized_values_and_existing_defvar_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "repeated_source_loads_preserve_customized_values_and_existing_defvar_state",
        r##"(let* ((source (getenv "NEOMACS_PACKAGE_SOURCE"))
       (old-width async-status-indicator-width)
       (old-progress-width async-status-progress-bar-width))
  (setq
   async-status-indicator-width 77
   async-status-progress-bar-width 91.5
   async-status--shown-items
   (list
    (make-async-status--item :msg-id "kept")))
  (load source nil t t)
  (load source nil t t)
  (prog1
      (list
       (featurep 'async-status)
       async-status-indicator-width
       async-status-progress-bar-width
       (mapcar #'async-status--item-msg-id
               async-status--shown-items))
    (setq
     async-status-indicator-width old-width
     async-status-progress-bar-width old-progress-width
     async-status--shown-items nil)))"##,
        expect!["OK (t 77 91.5 (\"kept\"))"],
    )
}

fn generated_autoload_file_registers_only_its_feature_without_eager_runtime_loading()
-> ParityBatchCase {
    ParityBatchCase::value(
        "generated_autoload_file_registers_only_its_feature_without_eager_runtime_loading",
        r##"(let ((history
       (assoc
        (getenv "NEOMACS_PACKAGE_SOURCE")
        load-history)))
  (list
   (featurep 'async-status-autoloads)
   (featurep 'async-status)
   (fboundp 'async-status-req-id)
   (fboundp 'async-status-add-item-to-bar)
   (file-name-nondirectory (car history))
   (cdr history)))"##,
        expect![[
            r#"OK (t nil nil nil "async-status-autoloads.el" ((provide . async-status-autoloads)))"#
        ]],
    )
}

pub(super) fn registry_async_status_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        installed_descriptor_matches_the_exact_pin_commit_and_dependencies(),
        packaged_source_has_the_verified_melpa_payload_content_hash(),
        complete_declared_function_surface_is_bound_after_loading(),
        generated_item_struct_surface_is_complete_and_callable(),
        variables_custom_types_defaults_and_group_metadata_match(),
        item_constructor_accessors_copy_and_mutation_have_value_semantics(),
        item_constructor_accepts_partial_and_reordered_keyword_arguments(),
        repeated_source_loads_preserve_customized_values_and_existing_defvar_state(),
    ]
}

pub(super) fn registry_async_status_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![generated_autoload_file_registers_only_its_feature_without_eager_runtime_loading()]
}
