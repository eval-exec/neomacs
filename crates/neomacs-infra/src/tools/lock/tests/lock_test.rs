use super::ToolLockCatalog;

#[test]
fn tools_lock_rejects_unsorted_rows() {
    let error = ToolLockCatalog::parse(
        "name\tversion\tsource\tsha256\n\
         zed\t1.0\tsystem\t\n\
         abi\t2.0\tsystem\t\n",
    )
    .expect_err("unsorted rows must be rejected");
    assert!(error.contains("sorted by name"), "{error}");
}

#[test]
fn tools_lock_rejects_missing_header_and_tolerates_empty_sha() {
    let error =
        ToolLockCatalog::parse("git\t2.51.2\n").expect_err("missing header must be rejected");
    assert!(error.contains("header row"), "{error}");

    let catalog = ToolLockCatalog::parse("name\tversion\tsource\tsha256\ngit\t2.51.2\tsystem\t\n")
        .expect("empty sha256 cell is a version-only pin");
    assert!(catalog.entry("git").is_some());
}

#[test]
fn tools_lock_accepts_a_nix_source_and_rejects_an_unknown_one() {
    let catalog = ToolLockCatalog::parse(
        "name\tversion\tsource\tsha256\n\
         java\t21.0.5\tnix:melpa-tools\t\n",
    )
    .expect("a nix attribute is a source the lock understands");
    let entry = catalog.entry("java").expect("java row");
    assert_eq!(entry.source, "nix:melpa-tools");

    let error = ToolLockCatalog::parse(
        "name\tversion\tsource\tsha256\n\
         java\t21.0.5\tpacman\t\n",
    )
    .expect_err("a source no strategy implements must be rejected");
    assert!(error.contains("unknown source"), "{error}");
}
