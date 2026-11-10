//! Bounded cfg policy shared by the TLS scanner and explicit gate checks.

/// These are the only feature names this interim policy treats as test-only.
/// Keep the scanner and explicit gate checks on this shared table.
pub(super) const TEST_ONLY_FEATURES: &[&str] = &["test-support", "redisplay-test-policy"];

/// Recognize only the bounded inner-cfg shapes reviewed for the first member.
/// Unrecognized or malformed predicates stay visible to the TLS scanner.
pub(super) fn interim_cfg_is_test_only(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::List(list) => {
            if !list.path.is_ident("all") && !list.path.is_ident("any") {
                return false;
            }
            let Ok(arms) = list.parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            ) else {
                return false;
            };
            if list.path.is_ident("all") {
                matches!(arms.first(), Some(syn::Meta::Path(path)) if path.is_ident("test"))
            } else {
                arms.iter().all(allowed_any_arm)
            }
        }
        syn::Meta::NameValue(_) => false,
    }
}

fn allowed_any_arm(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::NameValue(value) if value.path.is_ident("feature") => {
            let syn::Expr::Lit(expression) = &value.value else {
                return false;
            };
            let syn::Lit::Str(name) = &expression.lit else {
                return false;
            };
            // LitStr decoding preserves internal whitespace and identifier
            // boundaries; never compact or prefix-match the literal value.
            TEST_ONLY_FEATURES.contains(&name.value().as_str())
        }
        syn::Meta::NameValue(_) | syn::Meta::List(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::interim_cfg_is_test_only;

    fn check(source: &str, expected: bool) {
        let meta = syn::parse_str::<syn::Meta>(source).expect("cfg fixture Meta");
        assert_eq!(interim_cfg_is_test_only(&meta), expected, "{source}");
    }

    #[test]
    fn exact_test_and_allowlisted_any_gates_are_test_only() {
        for source in [
            "test",
            "any(test)",
            "any(test, feature = \"test-support\")",
            "any(feature = \"test-support\")",
            "any(feature = \"redisplay-test-policy\", test)",
            "any(test, feature = \"test-support\", feature = \"redisplay-test-policy\")",
        ] {
            check(source, true);
        }
    }

    #[test]
    fn whitespace_between_tokens_does_not_change_the_policy() {
        for source in [
            " test ",
            "any ( test , feature = \"test-support\" , )",
            "any(\n feature = \"redisplay-test-policy\",\n test\n)",
            "all ( test , feature = \"x\" , )",
        ] {
            check(source, true);
        }
    }

    #[test]
    fn feature_values_are_exact_decoded_literals() {
        check("any(test, feature = r#\"test-support\"#)", true);
        for source in [
            "any(test, feature = \"test - support\")",
            "any(test, feature = \"test-support \")",
            "any(test, feature = \" test-support\")",
            "any(test, feature = \"test-support-extra\")",
            "any(test, feature = \"redisplay-test-policy-extra\")",
            "any(test, feature = \"redisplay - test-policy\")",
            "any(test, feature = concat!(\"test-\", \"support\"))",
            "any(test, feature = true)",
            "any(test, other_feature = \"test-support\")",
        ] {
            check(source, false);
        }
    }

    #[test]
    fn production_and_unknown_any_arms_stay_scanned() {
        for source in [
            "any(test, debug_assertions)",
            "any(test, feature = \"vm-profile\")",
            "any(test, feature = \"fuzzing\")",
            "any(test, feature = \"x\")",
            "any(test, neomacs_have_dbus)",
            "any(testing, feature = \"test-support\")",
            "any(test_support)",
            "any(test::nested)",
            "any(test, all(test))",
            "any(test = true)",
        ] {
            check(source, false);
        }
    }

    #[test]
    fn all_requires_an_exact_first_test_arm() {
        for source in [
            "all(test)",
            "all(test, feature = \"x\")",
            "all(test, debug_assertions, not(feature = \"x\"))",
            "all(test, custom::predicate(value))",
        ] {
            check(source, true);
        }
        for source in [
            "all(debug_assertions, test)",
            "all(feature = \"test-support\", test)",
            "all(testing, feature = \"x\")",
            "all(test::nested)",
            "all(test = true)",
        ] {
            check(source, false);
        }
    }

    #[test]
    fn unknown_operators_and_malformed_arguments_stay_scanned() {
        for source in [
            "debug_assertions",
            "neomacs_have_dbus",
            "testing",
            "feature = \"test-support\"",
            "not(test)",
            "cfg_attr(test, feature = \"test-support\")",
            "cfg(test)",
            "custom::any(test)",
            "any(test, \"not a Meta\")",
            "all(test,,,)",
        ] {
            check(source, false);
        }
    }

    #[test]
    fn empty_cfg_lists_follow_the_bounded_rules() {
        check("all()", false);
        // any() is false in every configuration; it emits no TLS item.
        check("any()", true);
    }
}
