//! Oracle parity tests for GNU seeded `random` semantics.
//!
//! GNU implements `random` in `src/fns.c`.  A string argument reseeds the
//! process PRNG from the string bytes; subsequent bounded and unbounded calls
//! are deterministic for that seed, so oracle comparison can check exact
//! sequence compatibility instead of only range invariants.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_random_string_seed_exact_sequence_and_error_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; String seeds must reset the generator to an exactly reproducible GNU
 ;; sequence, including the return value of the seeding call itself.
 (let ((first (list (random "neomacs-oracle-seed")
                    (random 1000)
                    (random 1000)
                    (random 1)
                    (random))))
   (list first
         (progn
           (random "neomacs-oracle-seed")
           (list (random 1000)
                 (random 1000)
                 (random 1)
                 (random)))))
 ;; Different string seeds should produce GNU's different deterministic stream.
 (progn
   (random "neomacs-oracle-other-seed")
   (list (random 17) (random 17) (random 17)))
 ;; LIMIT must be positive for fixnums.
 (condition-case err
     (random 0)
   (error (list (car err) (cdr err))))
 (condition-case err
     (random -1)
   (error (list (car err) (cdr err))))
 ;; Non-string, non-integer, non-t arguments fall through to unbounded random.
 (integerp (random nil))
 (integerp (random 'symbol))
 (condition-case err
     (random 10 20)
   (error (list (car err) (cdr err)))))"#;

    let expect = expect_test::expect![[
        r#""OK (((0 897 359 0 0) (897 359 0 0)) (6 14 16) (args-out-of-range (0)) (args-out-of-range (-1)) t t (wrong-number-of-arguments (random 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
