//! Divergence tests: profiling, benchmarking, memory info deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_profiler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'profiler-start)
  (fboundp 'profiler-stop)
  (fboundp 'profiler-report)
  (featurep 'profiler))"#,
        expect,
    );
}

#[test]
fn divergence_elp_profiling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'elp-instrument-function)
  (fboundp 'elp-instrument-package)
  (fboundp 'elp-results)
  (featurep 'elp))"#,
        expect,
    );
}

#[test]
fn divergence_benchmark() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'benchmark-run)
  (fboundp 'benchmark-run-compiled)
  (fboundp 'benchmark)
  (featurep 'benchmark))"#,
        expect,
    );
}

#[test]
fn divergence_memory_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'memory-use-counts)
  (fboundp 'memory-limit)
  (fboundp 'garbage-collect)
  (listp (garbage-collect)))"#,
        expect,
    );
}

#[test]
fn divergence_gc_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'gc-cons-threshold)
  (boundp 'gc-cons-percentage)
  (integerp gc-cons-threshold)
  (numberp gc-cons-percentage)
  (fboundp 'gcs-done))"#,
        expect,
    );
}

#[test]
fn divergence_memory_limits() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (integerp gc-cons-threshold)
  (> gc-cons-threshold 0)
  (<= gc-cons-percentage 1.0)
  (>= gc-cons-percentage 0.0)
  (boundp 'gc-max-freed-per-gc))"#,
        expect,
    );
}

#[test]
fn divergence_pure_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'purecopy)
  (boundp 'pure-bytes-used)
  (integerp pure-bytes-used))"#,
        expect,
    );
}

#[test]
fn divergence_data_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'buffer-has-markers-at)
  (fboundp 'object-intervals)
  (integerp (buffer-size))) "#,
        expect,
    );
}

#[test]
fn divergence_list_internals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'length)
  (fboundp 'safe-length)
  (fboundp 'list-length)
  (= (length '(1 2 3)) 3)
  (= (safe-length '(1 2 3)) 3)) "#,
        expect,
    );
}

#[test]
fn divergence_bytecode_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'byte-code-meter)
  (fboundp 'internal-interpreter-environment)
  (fboundp 'byte-code)) "#,
        expect,
    );
}
