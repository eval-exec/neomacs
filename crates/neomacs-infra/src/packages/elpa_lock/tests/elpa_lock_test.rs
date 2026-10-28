use super::LockedElpaCatalog;

const FIRST_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SECOND_SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// The parser borrows the embedded manifest for the process lifetime; these
/// tests build manifests at runtime, so hand it an equivalent one.
fn leaked(manifest: String) -> &'static str {
    Box::leak(manifest.into_boxed_str())
}

#[test]
fn elpa_lock_rejects_unsorted_rows() {
    let error = LockedElpaCatalog::parse(leaked(format!(
        "package\tversion\tcommit\tsha256\n\
         zeta\t1.0\tcccccccccccccccccccccccccccccccccccccccc\t{FIRST_SHA}\n\
         alpha\t2.0\tdddddddddddddddddddddddddddddddddddddddd\t{SECOND_SHA}\n"
    )))
    .expect_err("unsorted rows must be rejected");
    assert!(error.contains("sorted by package name"), "{error}");
}

#[test]
fn elpa_lock_rejects_missing_header_and_empty_cells() {
    let error = LockedElpaCatalog::parse("demo\t1.0\tcafe\tdeadbeef\n")
        .expect_err("missing header must be rejected");
    assert!(error.contains("header row"), "{error}");

    let error = LockedElpaCatalog::parse(leaked(format!(
        "package\tversion\tcommit\tsha256\ndemo\t\tcccccccccccccccccccccccccccccccccccccccc\t{FIRST_SHA}\n"
    )))
    .expect_err("empty version cell must be rejected");
    assert!(error.contains("empty cell"), "{error}");

    let error = LockedElpaCatalog::parse(
        "package\tversion\tcommit\tsha256\ndemo\t1.0\tcccccccccccccccccccccccccccccccccccccccc\tdeadbeef\n",
    )
    .expect_err("short checksum must be rejected");
    assert!(error.contains("lowercase hex sha256"), "{error}");
}

#[test]
fn elpa_lock_accepts_a_well_formed_row() {
    let catalog = LockedElpaCatalog::parse(leaked(format!(
        "package\tversion\tcommit\tsha256\ndemo\t1.0\tcccccccccccccccccccccccccccccccccccccccc\t{FIRST_SHA}\n"
    )))
    .expect("well-formed manifest");
    let row = catalog.source("demo", "1.0").expect("row");
    assert_eq!(row.commit, "cccccccccccccccccccccccccccccccccccccccc");
    assert_eq!(row.sha256, FIRST_SHA);
}
