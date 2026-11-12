use super::*;

#[test]
fn legacy_registry_substrings_select_the_first_charset_entry() {
    for pattern in ["iso8859", "8859", "ISO8859", ""] {
        assert_eq!(
            registry_hint(pattern).unwrap().map(|hint| hint.name),
            Some("iso8859-1"),
            "{pattern}"
        );
    }
}

#[test]
fn legacy_registry_patterns_keep_emacs_regexp_syntax_and_table_order() {
    for (pattern, name) in [
        ("iso8859-[12]", "iso8859-1"),
        ("^iso8859-2$", "iso8859-2"),
        ("^iso8859-2$\\|^iso8859-1$", "iso8859-1"),
        ("gb2312.1980", "gb2312.1980-0"),
    ] {
        assert_eq!(
            registry_hint(pattern).unwrap().map(|hint| hint.name),
            Some(name),
            "{pattern}"
        );
    }
    assert!(
        registry_hint("iso8859.2").unwrap().is_none(),
        "the period remains literal"
    );
}

#[test]
fn legacy_registry_patterns_do_not_trim_whitespace() {
    assert!(registry_hint(" iso8859-1 ").unwrap().is_none());
}

#[test]
fn legacy_registry_language_and_coverage_share_the_selected_entry() {
    assert_eq!(registry_language("jisx0208").unwrap(), Some("ja"));
    assert_eq!(
        registry_query_chars(Some("^iso8859-2$\\|^iso8859-1$"), 'x').unwrap(),
        vec![0xa0, 0xa1, 0xb4, 0xbc, 0xd0]
    );
}

#[test]
fn legacy_registry_unicode_shortcuts_keep_their_existing_exact_listing_identity() {
    for name in ["ascii-0", "iso10646-1", "unicode-bmp"] {
        assert!(registry_supported_for_listing(name).unwrap());
    }
    for name in ["ASCII-0", "ISO10646-1", "Unicode-BMP"] {
        assert!(!registry_supported_for_listing(name).unwrap());
    }
}

#[test]
fn legacy_registry_lookup_works_on_a_detached_native_thread() {
    let name = std::thread::spawn(|| registry_hint("JISX0208*").unwrap().map(|hint| hint.name))
        .join()
        .unwrap();
    assert_eq!(name, Some("jisx0208.1983-0"));
}

#[test]
fn legacy_registry_compile_failure_is_not_an_unmatched_or_unconstrained_policy() {
    assert!(matches!(registry_hint("["),
        Err(EmacsRegexpError::Compile(ref message)) if message == "Unmatched [ or [^"));
    let spec = StoredFontSpec {
        registry: Some(neovm_core::emacs_core::intern::intern("[")),
        family: None,
        lang: None,
        weight: None,
        slant: None,
        width: None,
        definition: None,
    };
    assert!(GnuFontPolicy::constraints_for_entity(&spec).is_err());
    assert!(GnuFontPolicy::constraints_for_character(&spec, 'a').is_err());
}
