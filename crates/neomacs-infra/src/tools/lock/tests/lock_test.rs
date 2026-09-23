use super::ToolLockCatalog;

#[test]
fn tools_lock_rejects_unsorted_rows() {
    let error = ToolLockCatalog::parse(
        "name\tversion\tsha256\n\
         zed\t1.0\t\n\
         abi\t2.0\t\n",
    )
    .expect_err("unsorted rows must be rejected");
    assert!(error.contains("sorted by name"), "{error}");
}

#[test]
fn tools_lock_rejects_missing_header_and_tolerates_empty_sha() {
    let error =
        ToolLockCatalog::parse("git\t2.51.2\n").expect_err("missing header must be rejected");
    assert!(error.contains("header row"), "{error}");

    let catalog = ToolLockCatalog::parse("name\tversion\tsha256\ngit\t2.51.2\t\n")
        .expect("empty sha256 cell is a version-only pin");
    assert!(catalog.entry("git").is_some());
}
