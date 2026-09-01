use expect_test::expect;

use super::ParityBatchCase;

fn at_list_all_includes_only_bound_at_prefixed_objects() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_list_all_includes_only_bound_at_prefixed_objects",
        r##"(unwind-protect
               (progn
                 (set
                  '@neomacs-parity-object
                  (@extend))
                 (set
                  '@neomacs-parity-value
                  42)
                 (set
                  'neomacs-parity-object
                  (@extend))
                 (let ((all
                        (@--list-all)))
                   (list
                    (and
                     (memq
                      '@neomacs-parity-object
                      all)
                     t)
                    (and
                     (memq
                      '@neomacs-parity-value
                      all)
                     t)
                    (and
                     (memq
                      'neomacs-parity-object
                      all)
                     t)
                    (and (memq '@ all) t)
                    (and
                     (memq '@soft-get all)
                     t))))
             (mapc
              (lambda (symbol)
                (when (boundp symbol)
                  (makunbound symbol)))
              '(@neomacs-parity-object
                @neomacs-parity-value
                neomacs-parity-object)))"##,
        expect!["OK (t nil nil t t)"],
    )
}

fn at_describe_delegates_the_resolved_method_to_describe_function() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_describe_delegates_the_resolved_method_to_describe_function",
        r##"(let ((object (@extend)))
               (def@ object :method ()
                 'result)
               (let ((method
                      (@ object :method))
                     observed)
                 (cl-letf
                     (((symbol-function
                        'describe-function)
                       (lambda (function)
                         (setq observed
                               (eq function
                                   method))
                         'described)))
                   (list
                    (describe-@
                     object :method)
                    observed))))"##,
        expect!["OK (described t)"],
    )
}

fn at_describe_interactive_form_filters_prototypes_and_function_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_describe_interactive_form_filters_prototypes_and_function_properties",
        r##"(unwind-protect
               (progn
                 (set
                  '@neomacs-describe-proto
                  (@extend
                   :value 10
                   :method
                   (lambda (_) 'ok)))
                 (let ((answers
                        '("@neomacs-describe-proto"
                          ":method"))
                       events)
                   (cl-letf
                       (((symbol-function
                          'completing-read)
                         (lambda (prompt
                                  collection
                                  predicate
                                  require-match
                                  initial)
                           (push
                            (list
                             prompt
                             (and
                              (member
                               (car answers)
                               collection)
                              t)
                             predicate
                             require-match
                             initial)
                            events)
                           (prog1
                               (car answers)
                             (setq answers
                                   (cdr answers)))))
                        ((symbol-function
                          'describe-function)
                         (lambda (_) 'described)))
                     (list
                      (call-interactively
                       'describe-@)
                      (nreverse events)))))
             (when
                 (boundp
                  '@neomacs-describe-proto)
               (makunbound
                '@neomacs-describe-proto)))"##,
        expect![[
            r#"OK (described (("Describe prototype: " t nil t "@") ("Describe property: " t nil t ":")))"#
        ]],
    )
}

fn at_undefine_all_makunbounds_every_symbol_returned_by_reflection() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_undefine_all_makunbounds_every_symbol_returned_by_reflection",
        r##"(progn
               (set
                '@neomacs-first
                (@extend))
               (set
                '@neomacs-second
                (@extend))
               (cl-letf
                   (((symbol-function
                      '@--list-all)
                     (lambda ()
                       '(@neomacs-first
                         @neomacs-second))))
                 (list
                  (@--undefine-all)
                  (boundp
                   '@neomacs-first)
                  (boundp
                   '@neomacs-second))))"##,
        expect!["OK ((@neomacs-first @neomacs-second) nil nil)"],
    )
}

fn at_byte_compile_all_compiles_only_function_valued_direct_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_byte_compile_all_compiles_only_function_valued_direct_properties",
        r##"(let ((prototype
                    (@extend
                     :first #'car
                     :value 10
                     :second #'cdr))
                   events)
               (set
                '@neomacs-compile
                prototype)
               (unwind-protect
                   (cl-letf
                       (((symbol-function
                          '@--list-all)
                         (lambda ()
                           '(@neomacs-compile)))
                        ((symbol-function
                          'byte-compile)
                         (lambda (function)
                           (push
                            (cond
                             ((eq function
                                  #'car)
                              'car)
                             ((eq function
                                  #'cdr)
                              'cdr)
                             (t 'other))
                            events)
                           'compiled)))
                     (list
                      (@--byte-compile-all)
                      (nreverse events)))
                 (makunbound
                  '@neomacs-compile)))"##,
        expect!["OK (nil (car cdr))"],
    )
}

pub(super) fn reflection_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        at_list_all_includes_only_bound_at_prefixed_objects(),
        at_describe_delegates_the_resolved_method_to_describe_function(),
        at_describe_interactive_form_filters_prototypes_and_function_properties(),
        at_undefine_all_makunbounds_every_symbol_returned_by_reflection(),
        at_byte_compile_all_compiles_only_function_valued_direct_properties(),
    ]
}
