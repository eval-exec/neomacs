use expect_test::expect;

use super::ParityBatchCase;

fn compile_splits_components_and_applies_matching_styles() -> ParityBatchCase {
    ParityBatchCase::value(
        "compile_splits_components_and_applies_matching_styles",
        r####"
(let* ((orderless-matching-styles '(orderless-literal orderless-regexp))
       (orderless-style-dispatchers nil)
       (orderless-smart-case t)
       (default (orderless-compile "foo bar"))
       (flex (let ((orderless-matching-styles '(orderless-flex)))
               (orderless-compile "fb")))
       (initialism (let ((orderless-matching-styles '(orderless-initialism)))
                     (orderless-compile "fb")))
       (literal (orderless-literal "a+b"))
       (regexp (orderless-regexp "a+b")))
  (list :default default
        :flex flex
        :initialism initialism
        :literal literal
        :regexp regexp))
"####,
        expect![[
            r#"OK (:default (nil "foo" "bar") :flex (nil "\\(f\\)[^b]*\\(b\\)") :initialism (nil "\\(\\<f\\).*\\(\\<b\\)") :literal "a\\+b" :regexp "a+b")"#
        ]],
    )
}

fn filter_keeps_candidates_matching_every_component_in_any_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "filter_keeps_candidates_matching_every_component_in_any_order",
        r####"
(let* ((orderless-matching-styles '(orderless-literal))
       (orderless-style-dispatchers nil)
       (table '("release-train" "train-release" "deploy" "release" "train")))
  (list :both (orderless-filter "rel train" table)
        :one (orderless-filter "deploy" table)
        :none (orderless-filter "missing" table)
        :reversed (orderless-filter "train rel" table)))
"####,
        expect![[
            r#"OK (:both ("release-train" "train-release") :one ("deploy") :none nil :reversed ("release-train" "train-release"))"#
        ]],
    )
}

fn try_completion_and_all_completions_use_orderless_filtering() -> ParityBatchCase {
    ParityBatchCase::value(
        "try_completion_and_all_completions_use_orderless_filtering",
        r####"
(let* ((orderless-matching-styles '(orderless-literal))
       (orderless-style-dispatchers nil)
       (orderless-expand-substring nil)
       (table '("foo-bar" "foo-baz" "frob" "other"))
       (all (orderless-all-completions "f b" table nil nil))
       ;; `orderless-all-completions' returns an improper list ending in the
       ;; base-size integer from the completion style protocol.
       (candidates
        (cl-loop for rest on all
                 while (consp (cdr rest))
                 collect (substring-no-properties (car rest)) into xs
                 finally return
                 (if (consp rest)
                     (nconc xs (list (substring-no-properties (car rest))))
                   xs)))
       (base (and (consp all) (cdr (last all))))
       (try (orderless-try-completion "f b" table nil 3))
       (unique (orderless-try-completion "fro" table nil 3)))
  (list :candidates candidates
        :base base
        :try try
        :unique unique))
"####,
        expect![[
            r#"OK (:candidates ("foo-bar" "foo-baz" "frob") :base 0 :try ("f b" . 3) :unique ("frob" . 4))"#
        ]],
    )
}

fn highlight_matches_applies_faces_to_each_component() -> ParityBatchCase {
    ParityBatchCase::value(
        "highlight_matches_applies_faces_to_each_component",
        r####"
(let* ((orderless-matching-styles '(orderless-literal))
       (orderless-style-dispatchers nil)
       (compiled (cdr (orderless-compile "foo bar")))
       (highlighted
        (car (orderless-highlight-matches compiled '("xx-foo-yy-bar-zz")))))
  (list :plain (substring-no-properties highlighted)
        :faces (neomacs-orderless-test-faces highlighted)
        :face-vars orderless-match-faces))
"####,
        expect![[
            r#"OK (:plain "xx-foo-yy-bar-zz" :faces (("xx-" nil) ("foo" orderless-match-face-0) ("-yy-" nil) ("bar" orderless-match-face-1) ("-zz" nil)) :face-vars [orderless-match-face-0 orderless-match-face-1 orderless-match-face-2 orderless-match-face-3])"#
        ]],
    )
}

fn affix_dispatch_selects_literal_and_without_literal_styles() -> ParityBatchCase {
    ParityBatchCase::value(
        "affix_dispatch_selects_literal_and_without_literal_styles",
        r####"
(let* ((orderless-style-dispatchers (list #'orderless-affix-dispatch))
       (orderless-matching-styles '(orderless-regexp))
       (table '("release" "train" "release-train" "debug"))
       (=pattern (orderless-compile "=rel"))
       (!pattern (orderless-compile "!train"))
       (filtered-bang (orderless-filter "!train" table)))
  (list :literal-dispatch =pattern
        :without-dispatch !pattern
        :without-filter filtered-bang))
"####,
        expect![[
            r#"OK (:literal-dispatch (nil "rel") :without-dispatch (#[(str) ((not (orderless--match-p pred regexp str))) ((regexp . "train") (pred))]) :without-filter ("release" "debug"))"#
        ]],
    )
}

fn smart_case_toggles_ignore_case_from_component_case() -> ParityBatchCase {
    ParityBatchCase::value(
        "smart_case_toggles_ignore_case_from_component_case",
        r####"
(let* ((orderless-matching-styles '(orderless-literal))
       (orderless-style-dispatchers nil)
       (orderless-smart-case t)
       (lower (orderless-compile "foo"))
       (upper (orderless-compile "Foo"))
       (table '("Foo" "foo" "FOO" "bar"))
       (lower-hits (orderless-filter "foo" table))
       (upper-hits (orderless-filter "Foo" table)))
  (list :lower lower
        :upper upper
        :lower-hits lower-hits
        :upper-hits upper-hits))
"####,
        expect![[
            r#"OK (:lower (nil "foo") :upper (nil "Foo") :lower-hits ("Foo" "foo" "FOO") :upper-hits ("Foo"))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        compile_splits_components_and_applies_matching_styles(),
        filter_keeps_candidates_matching_every_component_in_any_order(),
        try_completion_and_all_completions_use_orderless_filtering(),
        highlight_matches_applies_faces_to_each_component(),
        affix_dispatch_selects_literal_and_without_literal_styles(),
        smart_case_toggles_ignore_case_from_component_case(),
    ]
}
