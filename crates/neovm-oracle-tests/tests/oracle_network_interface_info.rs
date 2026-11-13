//! GNU 31.1 `network_interface_info` (process.c:4499-4501) checks SBYTES
//! against sizeof ifr_name before copying the name. The kernel interprets
//! an embedded NUL only after that check.

#![cfg(unix)]

use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_network_interface_info_counts_bytes_before_nul() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(list
            (condition-case err
                (network-interface-info "abcdefghijklmnop")
              (error err))
            (condition-case err
                (network-interface-info (concat "abc" (string 0) "defghijklmnopq"))
              (error err))
            (equal (network-interface-info (concat "lo" (string 0) "x"))
                   (network-interface-info "lo"))
            (network-interface-info "abcdefghijklmno"))"#,
        expect_test::expect![[
            r#""OK ((error \"Interface name too long\") (error \"Interface name too long\") t nil)""#
        ]],
    );
}

#[test]
fn oracle_network_interface_info_uses_stored_unibyte_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(list
            (condition-case err
                (network-interface-info (concat (make-string 14 ?a) (unibyte-string 233)))
              (error err))
            (condition-case err
                (network-interface-info (concat (make-string 15 ?a) (unibyte-string 233)))
              (error err)))"#,
        expect_test::expect![[r#""OK (nil (error \"Interface name too long\"))""#]],
    );
}
