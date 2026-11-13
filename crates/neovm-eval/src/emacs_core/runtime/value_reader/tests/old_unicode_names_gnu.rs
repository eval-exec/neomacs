//! Historical-name expectations refreshed solely from GNU Emacs 31.1.
#[test]
fn rea05_all_historical_names_match_gnu_in_both_literal_domains() {
    super::escapes_gnu::check(
        "old-unicode-names",
        include_str!("old-unicode-names.el"),
        include_str!("escapes-old-unicode-names.expect"),
    );
}

#[test]
fn rea05_named_literal_rejects_non_ascii_before_case_folding() {
    super::escapes_gnu::check(
        "unicode-name-ascii-boundary",
        include_str!("unicode-name-ascii-boundary.el"),
        include_str!("escapes-unicode-name-ascii-boundary.expect"),
    );
}
