use super::LockedElpaCatalog;

#[test]
fn elpa_lock_rejects_unsorted_rows() {
    let error = LockedElpaCatalog::parse(
        "package\tversion\tcommit\n\
         zeta\t1.0\taaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n\
         alpha\t2.0\tbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n",
    )
    .expect_err("unsorted rows must be rejected");
    assert!(error.contains("sorted by package name"), "{error}");
}

#[test]
fn elpa_lock_rejects_missing_header_and_empty_cells() {
    let error =
        LockedElpaCatalog::parse("demo\t1.0\tcafe\n").expect_err("missing header must be rejected");
    assert!(error.contains("header row"), "{error}");

    let error = LockedElpaCatalog::parse("package\tversion\tcommit\ndemo\t\tcafe\n")
        .expect_err("empty version cell must be rejected");
    assert!(error.contains("empty cell"), "{error}");
}
