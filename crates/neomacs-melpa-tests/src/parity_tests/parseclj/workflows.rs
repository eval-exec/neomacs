use expect_test::expect;

use super::ParityBatchCase;

fn parse_clojure_builds_ast_for_vector_and_map() -> ParityBatchCase {
    ParityBatchCase::value(
        "parse_clojure_builds_ast_for_vector_and_map",
        r####"
(let* ((ast (parseclj-parse-clojure "[1 :kw \"s\"]"))
       (root-children (parseclj-ast-children ast))
       (vector-node (car root-children))
       (items (parseclj-ast-children vector-node)))
  (list :root-type (parseclj-ast-node-type ast)
        :vector-type (parseclj-ast-node-type vector-node)
        :item-types (mapcar #'parseclj-ast-node-type items)
        :item-values (mapcar #'parseclj-ast-value items)
        :branch-p (and (parseclj-ast-branch-node-p vector-node) t)))
"####,
        expect![[
            r#"OK (:root-type :root :vector-type :vector :item-types (:number :keyword :string) :item-values (1 :kw "s") :branch-p t)"#
        ]],
    )
}

fn lex_next_classifies_tokens_in_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "lex_next_classifies_tokens_in_order",
        r####"
(with-temp-buffer
  (insert "{:a 1, :b [true nil]}")
  (goto-char (point-min))
  (let (tokens)
    (while (not (parseclj-lex-at-eof-p))
      (let ((tok (parseclj-lex-next)))
        (when (not (eq (parseclj-lex-token-type tok) :whitespace))
          (push (list (parseclj-lex-token-type tok)
                      (parseclj-lex-token-form tok))
                tokens))))
    (nreverse tokens)))
"####,
        expect![[
            r#"OK ((:lbrace "{") (:keyword ":a") (:number "1") (:keyword ":b") (:lbracket "[") (:true "true") (:nil "nil") (:rbracket "]") (:rbrace "}"))"#
        ]],
    )
}

fn unparse_round_trip_preserves_source_shape() -> ParityBatchCase {
    ParityBatchCase::value(
        "unparse_round_trip_preserves_source_shape",
        r####"
(let* ((src "(defn demo [x] (+ x 1))")
       (ast (parseclj-parse-clojure src))
       (out (parseclj-unparse-clojure-to-string ast)))
  (list :out out
        :same (and (string= src out) t)
        :root-type (parseclj-ast-node-type ast)))
"####,
        expect![[r#"OK (:out "(defn demo [x] (+ x 1))" :same t :root-type :root)"#]],
    )
}

fn number_and_keyword_leaf_values_decode() -> ParityBatchCase {
    ParityBatchCase::value(
        "number_and_keyword_leaf_values_decode",
        r####"
(with-temp-buffer
  (insert "42 :hello 3.5")
  (goto-char (point-min))
  (let* ((n (parseclj-lex-next))
         (_ws (parseclj-lex-next))
         (k (parseclj-lex-next))
         (_ws2 (parseclj-lex-next))
         (f (parseclj-lex-next)))
    (list :number-type (parseclj-lex-token-type n)
          :number-value (parseclj-lex--leaf-token-value n)
          :keyword-type (parseclj-lex-token-type k)
          :keyword-form (parseclj-lex-token-form k)
          :float-value (parseclj-lex--leaf-token-value f))))
"####,
        expect![[
            r#"OK (:number-type :number :number-value 42 :keyword-type :keyword :keyword-form ":hello" :float-value 3.5)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        parse_clojure_builds_ast_for_vector_and_map(),
        lex_next_classifies_tokens_in_order(),
        unparse_round_trip_preserves_source_shape(),
        number_and_keyword_leaf_values_decode(),
    ]
}
