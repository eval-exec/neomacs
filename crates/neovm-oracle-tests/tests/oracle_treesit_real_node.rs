//! Real pinned JSON nodes retain their intrinsic type and parser identity.
use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;
use neomacs_melpa_test_support::{EmacsRuntime, GNU31_TREE_SITTER_GRAMMARS, elisp_string};

#[test]
fn real_json_node_keeps_type_and_parser_through_gc_and_deletion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let case = GNU31_TREE_SITTER_GRAMMARS
        .iter()
        .copied()
        .find(|case| case.language == "json")
        .expect("the pinned grammar catalog includes JSON");
    // This uses the same attested GNU resolver and pinned grammar preparation
    // as treesit_grammar_loading.rs. An unavailable grammar is a failure, not
    // a skipped positive case.
    let grammar = case
        .prepare(&EmacsRuntime::gnu_emacs())
        .expect("prepare the real pinned JSON grammar with the checked oracle");
    let grammar_path = elisp_string(&grammar.to_string_lossy());
    let sample = elisp_string(case.sample);
    let form = format!(
        r#"(progn
  (require 'treesit)
  (setq treesit-extra-load-path (list {grammar_path}))
  (with-temp-buffer
    (insert {sample})
    (let* ((parser (treesit-parser-create 'json))
           (node (treesit-parser-root-node parser))
           (observe (lambda ()
                      (list (treesit-node-p node)
                            (eq (treesit-node-parser node) parser)
                            (recordp node)
                            (type-of node))))
           (initial (list (funcall observe)
                          (treesit-node-type node)
                          (treesit-node-start node)
                          (treesit-node-end node))))
      (garbage-collect)
      (let ((after-gc (funcall observe)))
        (goto-char (point-max))
        (insert " ")
        (let ((after-edit (funcall observe)))
          (treesit-parser-delete parser)
          (garbage-collect)
          (let ((fake (record 'treesit-node 1 parser)))
            (list initial after-gc after-edit (funcall observe)
                  (list (recordp fake) (treesit-node-p fake)
                        (condition-case err
                            (list 'value (eq (treesit-node-parser fake) parser))
                          (error (list (car err) (cadr err)
                                       (length (cdr err))
                                       (eq (caddr err) fake))))))))))))"#
    );
    // Compare the error's predicate, exact data arity and offending-object
    // identity without serializing the native parser inside the fake record.
    // Placeholder only: GNU refresh must generate the expectation.
    common::assert_oracle_parity_expect(&form, expect_test::expect![[""]]);
}
