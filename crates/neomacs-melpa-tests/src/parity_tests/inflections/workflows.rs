use expect_test::expect;

use super::ParityBatchCase;

fn pluralize_regular_and_irregular_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "pluralize_regular_and_irregular_forms",
        r####"
(list :cat (inflection-pluralize-string "cat")
      :box (inflection-pluralize-string "box")
      :person (inflection-pluralize-string "person")
      :child (inflection-pluralize-string "child")
      :status (inflection-pluralize-string "status"))
"####,
        expect![[
            r#"OK (:cat "cats" :box "boxes" :person "people" :child "children" :status "statuses")"#
        ]],
    )
}

fn singularize_regular_and_irregular_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "singularize_regular_and_irregular_forms",
        r####"
(list :cats (inflection-singularize-string "cats")
      :boxes (inflection-singularize-string "boxes")
      :people (inflection-singularize-string "people")
      :children (inflection-singularize-string "children")
      :statuses (inflection-singularize-string "statuses"))
"####,
        expect![[
            r#"OK (:cats "cat" :boxes "box" :people "person" :children "child" :statuses "status")"#
        ]],
    )
}

fn uncountable_and_non_string_inputs() -> ParityBatchCase {
    ParityBatchCase::value(
        "uncountable_and_non_string_inputs",
        r####"
(list :fish-plural (inflection-pluralize-string "fish")
      :fish-singular (inflection-singularize-string "fish")
      :sheep (inflection-pluralize-string "sheep")
      :nil-plural (inflection-pluralize-string nil)
      :nil-singular (inflection-singularize-string 42))
"####,
        expect![[
            r#"OK (:fish-plural "fish" :fish-singular "fish" :sheep "sheep" :nil-plural nil :nil-singular nil)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        pluralize_regular_and_irregular_forms(),
        singularize_regular_and_irregular_forms(),
        uncountable_and_non_string_inputs(),
    ]
}
