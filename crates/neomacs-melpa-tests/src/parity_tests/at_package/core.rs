use expect_test::expect;

use super::ParityBatchCase;

fn at_root_object_core_methods_features_and_help_binding_match_the_pin() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_root_object_core_methods_features_and_help_binding_match_the_pin",
        r##"(list
              (@p @)
              (aref @ 0)
              (plist-get
               (aref @ 1)
               :proto)
              (@! @ :keys)
              (mapcar
               (lambda (property)
                 (functionp
                  (@ @ property)))
               '(:set :get :init
                 :new :is :keys))
              (featurep '@)
              (featurep '@-mixins)
              (lookup-key
               global-map
               (kbd "C-h @")))"##,
        expect![[
            r#"OK (t @ nil (:proto :set :get :init :new :is :keys) (t t t t t t) t t describe-@)"#
        ]],
    )
}

fn at_predicate_and_extend_cover_root_default_multiple_prototypes_and_properties() -> ParityBatchCase
{
    ParityBatchCase::value(
        "at_predicate_and_extend_cover_root_default_multiple_prototypes_and_properties",
        r##"(let* ((left
                      (@extend :side 'left))
                     (right
                      (@extend :side 'right))
                     (child
                      (@extend
                       left right
                       :name "child"
                       :nil-value nil)))
               (list
                (mapcar
                 #'@p
                 (list @ left child
                       [not-an-at-object]
                       [@ nil]
                       '(list) nil))
                (eq
                 (car
                  (plist-get
                   (aref left 1)
                   :proto))
                 @)
                (mapcar
                 (lambda (object)
                   (cond
                    ((eq object left)
                     'left)
                    ((eq object right)
                     'right)
                    (t 'other)))
                 (plist-get
                  (aref child 1)
                  :proto))
                (@ child :name)
                (@ child :nil-value)
                (@ child :side)))"##,
        expect!["OK ((t t t nil t nil nil) t (left right) \"child\" nil left)"],
    )
}

fn at_predicate_on_an_empty_vector_signals_the_exact_slot_error() -> ParityBatchCase {
    ParityBatchCase::signal(
        "at_predicate_on_an_empty_vector_signals_the_exact_slot_error",
        r##"(@p [])"##,
        expect!["ERR (args-out-of-range [] 0)"],
    )
}

fn at_precedence_flattens_diamond_inheritance_and_removes_first_duplicate() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_precedence_flattens_diamond_inheritance_and_removes_first_duplicate",
        r##"(let* ((root
                      (@extend :id 'root))
                     (left
                      (@extend root :id 'left))
                     (right
                      (@extend root :id 'right))
                     (top
                      (@extend left right)))
               (mapcar
                (lambda (object)
                  (cond
                   ((eq object left)
                    'left)
                   ((eq object right)
                    'right)
                   ((eq object root)
                    'root)
                   ((eq object @) '@)
                   (t 'unknown)))
                (@precedence top)))"##,
        expect!["OK (left right root @)"],
    )
}

fn at_instance_checks_cover_identity_ancestors_unrelated_and_non_objects() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_instance_checks_cover_identity_ancestors_unrelated_and_non_objects",
        r##"(let* ((parent (@extend))
                     (child (@extend parent))
                     (unrelated (@extend)))
               (list
                (@is child child)
                (@is child parent)
                (@is child @)
                (@is parent child)
                (@is child unrelated)
                (@is t @)
                (@is @ t)
                (@! child :is parent)
                (@! parent :is child)))"##,
        expect!["OK (t t t nil nil nil nil t nil)"],
    )
}

fn at_internal_queue_preserves_fifo_head_and_empty_reset_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_internal_queue_preserves_fifo_head_and_empty_reset_contract",
        r##"(let ((queue
                    (@--queue-create)))
               (list
                (@--queue-head queue)
                (@--queue-enqueue
                 queue 'first)
                (copy-sequence
                 (@--queue-head queue))
                (@--queue-enqueue
                 queue 'second)
                (copy-sequence
                 (@--queue-head queue))
                (@--queue-dequeue queue)
                (copy-sequence
                 (@--queue-head queue))
                (@--queue-dequeue queue)
                (@--queue-head queue)
                queue))"##,
        expect![[
            r#"OK (nil first (first) second (first second) first (second) second nil (nil))"#
        ]],
    )
}

fn at_lookup_uses_breadth_first_inheritance_and_counts_super_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_lookup_uses_breadth_first_inheritance_and_counts_super_matches",
        r##"(let* ((root
                      (@extend :name 'root))
                     (left
                      (@extend root :name 'left))
                     (right
                      (@extend root :name 'right))
                     (top
                      (@extend left right
                               :name 'top))
                     (right-only
                      (@extend
                       (@extend root)
                       right)))
               (list
                (@ top :name)
                (@ top :name :super 1)
                (@ top :name :super 2)
                (@ top :name :super 3)
                (@ right-only :name)))"##,
        expect!["OK (top left right root right)"],
    )
}

fn at_lookup_distinguishes_implicit_error_explicit_nil_and_non_nil_defaults() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_lookup_distinguishes_implicit_error_explicit_nil_and_non_nil_defaults",
        r##"(let ((object (@extend)))
               (list
                (@ object :missing
                   :default nil)
                (@ object :missing
                   :default 'fallback)
                (@ object :missing
                   :super 10
                   :default 'past-end)))"##,
        expect!["OK (nil fallback past-end)"],
    )
}

fn at_lookup_without_property_or_default_signals_exact_dynamic_getter_error() -> ParityBatchCase {
    ParityBatchCase::signal(
        "at_lookup_without_property_or_default_signals_exact_dynamic_getter_error",
        r##"(@ (@extend) :missing)"##,
        expect![[r#"ERR (error "Property unbound: :missing")"#]],
    )
}

fn at_setf_assigns_only_the_immediate_object_and_returns_the_new_value() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_setf_assigns_only_the_immediate_object_and_returns_the_new_value",
        r##"(let* ((parent
                      (@extend :value 'parent))
                     (child (@extend parent)))
               (list
                (@ child :value)
                (setf
                 (@ child :value)
                 'child)
                (@ child :value)
                (@ parent :value)
                (@! child :keys)
                (@! parent :keys)))"##,
        expect![[r#"OK (parent child child parent (:proto :value) (:proto :value))"#]],
    )
}

fn at_method_calls_and_super_method_dsl_chain_through_each_matching_prototype() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_method_calls_and_super_method_dsl_chain_through_each_matching_prototype",
        r##"(let* ((a (@extend))
                     (b (@extend a))
                     (c (@extend b)))
               (def@ a :chain (value)
                 (list 'a value))
               (def@ b :chain (value)
                 (cons 'b
                       (@^:chain value)))
               (def@ c :chain (value)
                 (cons 'c
                       (@^:chain value)))
               (list
                (@! c :chain 7)
                (@--super! c :chain 8)
                (with-@@ c
                  (@^:chain 9))))"##,
        expect!["OK ((c b a 7) (b a 8) (b a 9))"],
    )
}

fn at_property_super_dsl_reads_each_next_matching_value() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_property_super_dsl_reads_each_next_matching_value",
        r##"(let* ((a
                      (@extend :value 'a))
                     (b
                      (@extend a :value 'b))
                     (c
                      (@extend b :value 'c)))
               (list
                (with-@@ c @:value)
                (with-@@ c @^:value)
                (@ c :value :super 2)))"##,
        expect!["OK (c b a)"],
    )
}

fn at_new_calls_initializer_and_core_keys_and_is_methods_observe_the_child() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_new_calls_initializer_and_core_keys_and_is_methods_observe_the_child",
        r##"(let ((rectangle
                    (@extend
                     :width nil
                     :height nil)))
               (def@ rectangle
                   :init (width height)
                 (setf
                  @:width width
                  @:height height))
               (def@ rectangle :area ()
                 (* @:width @:height))
               (let ((instance
                      (@! rectangle
                          :new 6 7)))
                 (list
                  (@! instance :area)
                  (@ instance :width)
                  (@ instance :height)
                  (@! instance :is
                      rectangle)
                  (@! instance :is @)
                  (@! instance :keys)
                  (@! rectangle :keys))))"##,
        expect![[r#"OK (42 6 7 t t (:proto :width :height) (:proto :width :height :init :area))"#]],
    )
}

fn at_dynamic_getter_receives_missing_property_but_explicit_default_bypasses_it() -> ParityBatchCase
{
    ParityBatchCase::value(
        "at_dynamic_getter_receives_missing_property_but_explicit_default_bypasses_it",
        r##"(let ((object
                    (@extend
                     :prefix "got")))
               (def@ object :get (property)
                 (list
                  @:prefix property))
               (list
                (@ object :missing)
                (@ object :other
                   :default 'explicit)))"##,
        expect!["OK ((\"got\" :missing) explicit)"],
    )
}

fn at_walk_replace_and_with_object_preserve_quote_and_expand_property_positions() -> ParityBatchCase
{
    ParityBatchCase::value(
        "at_walk_replace_and_with_object_preserve_quote_and_expand_property_positions",
        r##"(list
              (@--walk
               '(setf @:name 10)
               '(quote)
               #'@--replace)
              (@--walk
               '(setf '@:name 10)
               '(quote)
               #'@--replace)
              (@--walk
               '(@:method @:argument)
               '(quote)
               #'@--replace)
              (macroexpand-1
               '(with-@@ object
                  (list @:value
                        (@:method 1)
                        @^:parent)))
              (with-@@
                  (@extend :value 'ok)
                @:value))"##,
        expect![[
            r#"OK ((setf (@ @@ :name) 10) (setf '@:name 10) (@! @@ :method (@ @@ :argument)) (let ((@@ object)) (list (@ @@ :value) (@! @@ :method 1) (@--super @@ :parent))) ok)"#
        ]],
    )
}

fn at_definer_returns_property_preserves_docstring_and_binds_self_before_arguments()
-> ParityBatchCase {
    ParityBatchCase::value(
        "at_definer_returns_property_preserves_docstring_and_binds_self_before_arguments",
        r##"(let ((object
                    (@extend :base 10)))
               (list
                (def@ object :sum (left
                                   &optional
                                   (right 2))
                  "Add values to the base."
                  (+ @:base left right))
                (@! object :sum 3)
                (@! object :sum 3 4)
                (documentation
                 (@ object :sum))
                (help-function-arglist
                 (@ object :sum)
                 t)))"##,
        expect![[
            r#"OK (:sum 15 17 "Add values to the base.\n\n(fn @@ LEFT &optional (RIGHT 2))" (@@ left &rest --cl-rest--))"#
        ]],
    )
}

pub(super) fn core_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        at_root_object_core_methods_features_and_help_binding_match_the_pin(),
        at_predicate_and_extend_cover_root_default_multiple_prototypes_and_properties(),
        at_predicate_on_an_empty_vector_signals_the_exact_slot_error(),
        at_precedence_flattens_diamond_inheritance_and_removes_first_duplicate(),
        at_instance_checks_cover_identity_ancestors_unrelated_and_non_objects(),
        at_internal_queue_preserves_fifo_head_and_empty_reset_contract(),
        at_lookup_uses_breadth_first_inheritance_and_counts_super_matches(),
        at_lookup_distinguishes_implicit_error_explicit_nil_and_non_nil_defaults(),
        at_lookup_without_property_or_default_signals_exact_dynamic_getter_error(),
        at_setf_assigns_only_the_immediate_object_and_returns_the_new_value(),
        at_method_calls_and_super_method_dsl_chain_through_each_matching_prototype(),
        at_property_super_dsl_reads_each_next_matching_value(),
        at_new_calls_initializer_and_core_keys_and_is_methods_observe_the_child(),
        at_dynamic_getter_receives_missing_property_but_explicit_default_bypasses_it(),
        at_walk_replace_and_with_object_preserve_quote_and_expand_property_positions(),
        at_definer_returns_property_preserves_docstring_and_binds_self_before_arguments(),
    ]
}
