//! Per-char *case-fold lower->upper* matrix (Greek + Cyrillic).
//!
//! Confirmed root cause: Neomacs case-fold-search handles 2-byte UTF-8 chars
//! in the lower lead-byte range (CE, D0) but MISSES the upper range (CF, D1).
//! So Greek pi-omega (CF) and Cyrillic р-я (D1) fail to case-fold-match their
//! uppercase; Greek alpha-omicron (CE) and Cyrillic а-п (D0) work.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cf_cp_945() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 945) (string 913)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_946() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 946) (string 914)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_947() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 947) (string 915)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_948() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 948) (string 916)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_949() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 949) (string 917)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_950() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 950) (string 918)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_951() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 951) (string 919)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_952() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 952) (string 920)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_953() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 953) (string 921)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_954() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 954) (string 922)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_955() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 955) (string 923)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_956() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 956) (string 924)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_957() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 957) (string 925)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_958() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 958) (string 926)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_959() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 959) (string 927)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_960() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 960) (string 928)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_961() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 961) (string 929)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_963() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 963) (string 931)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_964() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 964) (string 932)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_965() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 965) (string 933)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_966() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 966) (string 934)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_967() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 967) (string 935)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_968() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 968) (string 936)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_969() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 969) (string 937)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1072() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1072) (string 1040)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1073() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1073) (string 1041)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1074() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1074) (string 1042)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1075() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1075) (string 1043)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1076() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1076) (string 1044)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1077() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1077) (string 1045)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1078() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1078) (string 1046)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1079() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1079) (string 1047)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1080() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1080) (string 1048)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1081() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1081) (string 1049)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1082() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1082) (string 1050)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1083() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1083) (string 1051)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1084() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1084) (string 1052)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1085() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1085) (string 1053)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1086() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1086) (string 1054)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1087() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1087) (string 1055)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1088() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1088) (string 1056)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1089() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1089) (string 1057)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1090() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1090) (string 1058)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1091() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1091) (string 1059)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1092() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1092) (string 1060)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1093() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1093) (string 1061)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1094() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1094) (string 1062)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1095() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1095) (string 1063)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1096() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1096) (string 1064)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1097() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1097) (string 1065)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1098() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1098) (string 1066)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1099() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1099) (string 1067)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1100() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1100) (string 1068)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1101() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1101) (string 1069)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1102() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1102) (string 1070)) 1 0))",
        expect,
    );
}

#[test]
fn div_cf_cp_1103() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((case-fold-search t)) (if (string-match (string 1103) (string 1071)) 1 0))",
        expect,
    );
}
