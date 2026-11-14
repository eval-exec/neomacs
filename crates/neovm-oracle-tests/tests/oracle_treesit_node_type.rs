//! Intrinsic tree-sitter node identity, before any grammar or parser lookup.
//!
//! GNU31.1 treesit.c:Ftreesit_node_parser calls CHECK_TS_NODE. Generic records
//! remain PVEC_RECORD, including records whose type slot says treesit-node.
use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn treesit_node_parser_rejects_forged_record_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Placeholder only: refresh from the attested GNU oracle before verify.
    // An empty-expectation failure is not the regression's RED evidence.
    common::assert_oracle_parity_expect(
        r#"(mapcar
  (lambda (input)
    (list (treesit-node-p input)
          (condition-case err
              (list 'value (treesit-node-parser input))
            (error (list 'error-symbol (car err) 'error-data (cdr err))))))
  (list nil 7 "not-a-node" [treesit-node 11 sentinel]
        (record 'not-a-node)
        (record 'not-a-node 11)
        (record 'not-a-node 11 'sentinel)
        (record 'treesit-node)
        (record 'treesit-node 11 'sentinel)))"#,
        expect_test::expect![[""]],
    );
}
