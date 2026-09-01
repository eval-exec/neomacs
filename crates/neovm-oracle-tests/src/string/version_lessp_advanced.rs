//! Advanced oracle parity tests for `string-version-lessp`.
//!
//! Tests version number comparisons with multiple dots, alpha vs numeric sorting,
//! pre-release suffixes, leading zeros, edge cases (empty strings, identical strings,
//! very long version numbers), and combined with sort for version list ordering.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Multi-dot version numbers and deep hierarchy
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_version_lessp_multi_dot_versions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
      ;; Standard semver-style comparisons
      (push (string-version-lessp "1.0.0" "1.0.1") results)
      (push (string-version-lessp "1.0.1" "1.0.0") results)
      (push (string-version-lessp "1.0.0" "1.1.0") results)
      (push (string-version-lessp "1.1.0" "1.0.0") results)
      (push (string-version-lessp "1.0.0" "2.0.0") results)
      (push (string-version-lessp "2.0.0" "1.0.0") results)

      ;; Deep version hierarchies
      (push (string-version-lessp "1.2.3.4" "1.2.3.5") results)
      (push (string-version-lessp "1.2.3.4.5" "1.2.3.4.6") results)
      (push (string-version-lessp "1.2.3.4.5.6" "1.2.3.4.5.6") results)

      ;; Different depths: shorter vs longer
      (push (string-version-lessp "1.2" "1.2.0") results)
      (push (string-version-lessp "1.2.0" "1.2") results)
      (push (string-version-lessp "1.2" "1.2.1") results)
      (push (string-version-lessp "1.2.1" "1.2") results)

      ;; Major version dominates
      (push (string-version-lessp "2.0.0" "10.0.0") results)
      (push (string-version-lessp "9.99.99" "10.0.0") results)

      (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil t nil t t nil t nil t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alpha vs numeric sorting and mixed segments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_version_lessp_alpha_numeric_mixing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
      ;; Pure alphabetic comparison
      (push (string-version-lessp "abc" "abd") results)
      (push (string-version-lessp "abd" "abc") results)

      ;; Numeric segments sorted numerically, not lexicographically
      (push (string-version-lessp "file2" "file10") results)
      (push (string-version-lessp "file10" "file2") results)

      ;; Mixed: alpha prefix then number
      (push (string-version-lessp "lib1.2" "lib1.10") results)
      (push (string-version-lessp "lib10" "lib2") results)

      ;; Alpha segments between numeric
      (push (string-version-lessp "1a2" "1a10") results)
      (push (string-version-lessp "1b2" "1a10") results)

      ;; Case sensitivity (uppercase < lowercase in ASCII)
      (push (string-version-lessp "A" "a") results)
      (push (string-version-lessp "a" "A") results)
      (push (string-version-lessp "Version1" "version1") results)

      ;; Numeric-only vs alpha-prefixed
      (push (string-version-lessp "1" "a") results)
      (push (string-version-lessp "a" "1") results)

      ;; Dot-separated mixed
      (push (string-version-lessp "v1.alpha.3" "v1.beta.2") results)
      (push (string-version-lessp "v1.beta.2" "v1.alpha.3") results)

      (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil t nil t nil t nil t t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Leading zeros and numeric edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_version_lessp_leading_zeros() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
      ;; Leading zeros: "007" vs "7"
      (push (string-version-lessp "007" "7") results)
      (push (string-version-lessp "7" "007") results)

      ;; Leading zeros in version components
      (push (string-version-lessp "1.02.3" "1.2.3") results)
      (push (string-version-lessp "1.2.3" "1.02.3") results)

      ;; All zeros
      (push (string-version-lessp "0.0.0" "0.0.1") results)
      (push (string-version-lessp "0.0.0" "0.0.0") results)

      ;; Large numbers
      (push (string-version-lessp "1.999" "1.1000") results)
      (push (string-version-lessp "1.1000" "1.999") results)

      ;; Very large version numbers
      (push (string-version-lessp "99.99.99" "100.0.0") results)
      (push (string-version-lessp "100.0.0" "99.99.99") results)

      ;; Single digit comparisons
      (push (string-version-lessp "0" "1") results)
      (push (string-version-lessp "9" "10") results)
      (push (string-version-lessp "10" "9") results)

      ;; Zero vs non-zero
      (push (string-version-lessp "0" "0") results)
      (push (string-version-lessp "00" "0") results)
      (push (string-version-lessp "0" "00") results)

      (nreverse results))"#;
    let expect =
        expect_test::expect![[r#""OK (nil nil nil nil t nil t nil t nil t t nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases: empty strings, identical, prefix relations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_version_lessp_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
      ;; Empty strings
      (push (string-version-lessp "" "") results)
      (push (string-version-lessp "" "1") results)
      (push (string-version-lessp "1" "") results)
      (push (string-version-lessp "" "a") results)
      (push (string-version-lessp "a" "") results)

      ;; Identical strings
      (push (string-version-lessp "1.0.0" "1.0.0") results)
      (push (string-version-lessp "abc" "abc") results)
      (push (string-version-lessp "v2.3.beta" "v2.3.beta") results)

      ;; Prefix relations
      (push (string-version-lessp "1.2" "1.2.3") results)
      (push (string-version-lessp "1.2.3" "1.2") results)
      (push (string-version-lessp "abc" "abcdef") results)
      (push (string-version-lessp "abcdef" "abc") results)

      ;; Single character
      (push (string-version-lessp "a" "b") results)
      (push (string-version-lessp "z" "a") results)
      (push (string-version-lessp "1" "2") results)

      ;; Strings with hyphens
      (push (string-version-lessp "1.0-alpha" "1.0-beta") results)
      (push (string-version-lessp "1.0-beta" "1.0-alpha") results)

      ;; Strings with underscores
      (push (string-version-lessp "foo_1" "foo_2") results)
      (push (string-version-lessp "foo_10" "foo_2") results)

      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (nil t nil t nil nil nil nil t nil t nil t nil t t nil t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sorting version lists using string-version-lessp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_version_lessp_sort_versions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((versions '("2.0.1" "1.10.0" "1.9.3" "1.2.0" "10.0.0"
                       "0.1.0" "1.0.0" "2.0.0" "1.10.1" "1.9.10")))
      ;; Sort using string-version-lessp
      (let ((sorted (sort (copy-sequence versions) #'string-version-lessp)))
        ;; Verify sorted order: each element <= next
        (let ((ordered t)
              (tail sorted))
          (while (cdr tail)
            (when (string-version-lessp (cadr tail) (car tail))
              (setq ordered nil))
            (setq tail (cdr tail)))
          ;; Also sort file names with numeric parts
          (let ((files '("file20.txt" "file3.txt" "file1.txt" "file10.txt" "file2.txt")))
            (let ((sorted-files (sort (copy-sequence files) #'string-version-lessp)))
              ;; Sort package versions with pre-release tags
              (let ((pkgs '("emacs-28.1" "emacs-27.2" "emacs-29.1"
                            "emacs-28.2" "emacs-27.1")))
                (let ((sorted-pkgs (sort (copy-sequence pkgs) #'string-version-lessp)))
                  (list :versions sorted
                        :ordered ordered
                        :files sorted-files
                        :packages sorted-pkgs))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (:versions (\"0.1.0\" \"1.0.0\" \"1.2.0\" \"1.9.3\" \"1.9.10\" \"1.10.0\" \"1.10.1\" \"2.0.0\" \"2.0.1\" \"10.0.0\") :ordered t :files (\"file1.txt\" \"file2.txt\" \"file3.txt\" \"file10.txt\" \"file20.txt\") :packages (\"emacs-27.1\" \"emacs-27.2\" \"emacs-28.1\" \"emacs-28.2\" \"emacs-29.1\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Comprehensive pairwise transitivity check
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_version_lessp_transitivity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify transitivity: if a < b and b < c, then a < c
    let form = r#"(let ((versions '("0.1" "0.2" "0.10" "1.0" "1.1" "1.9" "1.10" "2.0"))
          (transitive-ok t))
      (let ((n (length versions))
            (i 0))
        (while (< i n)
          (let ((j 0))
            (while (< j n)
              (let ((k 0))
                (while (< k n)
                  (let ((a (nth i versions))
                        (b (nth j versions))
                        (c (nth k versions)))
                    (when (and (string-version-lessp a b)
                               (string-version-lessp b c))
                      (unless (string-version-lessp a c)
                        (setq transitive-ok nil))))
                  (setq k (1+ k))))
              (setq j (1+ j))))
          (setq i (1+ i))))
      ;; Also verify irreflexivity
      (let ((irreflexive-ok t))
        (dolist (v versions)
          (when (string-version-lessp v v)
            (setq irreflexive-ok nil)))
        ;; Verify asymmetry: not (a < b and b < a)
        (let ((asymmetric-ok t))
          (dolist (a versions)
            (dolist (b versions)
              (when (and (string-version-lessp a b)
                         (string-version-lessp b a))
                (setq asymmetric-ok nil))))
          (list :transitive transitive-ok
                :irreflexive irreflexive-ok
                :asymmetric asymmetric-ok))))"#;
    let expect = expect_test::expect![[r#""OK (:transitive t :irreflexive t :asymmetric t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbol arguments and pre-release suffix patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_version_lessp_symbols_and_suffixes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
      ;; Symbols as arguments (Emacs auto-converts via symbol-name)
      (push (string-version-lessp 'v1 'v2) results)
      (push (string-version-lessp 'v10 'v2) results)
      (push (string-version-lessp 'pkg1 'pkg1) results)

      ;; Pre-release style suffixes
      (push (string-version-lessp "1.0alpha" "1.0beta") results)
      (push (string-version-lessp "1.0beta" "1.0rc") results)
      (push (string-version-lessp "1.0rc" "1.0") results)

      ;; Numeric suffixes after text
      (push (string-version-lessp "1.0alpha1" "1.0alpha2") results)
      (push (string-version-lessp "1.0alpha2" "1.0alpha10") results)

      ;; Snapshot/date-based versions
      (push (string-version-lessp "20230101" "20230201") results)
      (push (string-version-lessp "20230201" "20230101") results)

      ;; Mixed separators: dots vs hyphens vs underscores
      (push (string-version-lessp "1-2-3" "1-2-4") results)
      (push (string-version-lessp "1_2_3" "1_2_4") results)
      (push (string-version-lessp "1.2-3" "1.2-4") results)

      (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil t t nil t t t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
