//! Compiler contracts for the proof-2 sealed hash-table authority.
//!
//! The three A7 rows live in `tests/p2ht_ui`. The mutation control copies
//! the branded model, removes one lifetime brand, and the same body then
//! type-checks. That copy is written to a per-run directory under the
//! process temp dir.

#[test]
#[cfg(feature = "contract-fixtures")]
fn p2ht_compile_contracts() {
    use std::fs;
    use std::path::PathBuf;

    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/p2ht_ui/*.rs");
    cases.compile_fail("tests/p2ht_brand/finish_while_leased.rs");
    cases.compile_fail("tests/p2ht_brand/cons_while_writer_live.rs");
    cases.compile_fail("tests/p2ht_brand/cons_while_reader_live.rs");

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let product = fs::read_to_string(
        manifest.join("../neovm-value/src/tagged/gc/mutator_registration/table_authority.rs"),
    )
    .expect("table authority source");
    assert_eq!(
        product.matches("access: &'a mut RunAccess<'_, '_>").count(),
        1,
        "one writer brand"
    );
    assert_eq!(
        product.matches("access: &'a RunAccess<'_, '_>").count(),
        1,
        "one reader brand"
    );

    let brand = fs::read_to_string(manifest.join("tests/p2ht_brand/lease_brand.rs"))
        .expect("lease brand model");
    let mutations = [
        (
            "finish_while_leased",
            "access: &'a mut RunAccess<'_, '_>",
            "access: &mut RunAccess<'_, 'a>",
        ),
        (
            "cons_while_writer_live",
            "access: &'a mut RunAccess<'_, '_>",
            "access: &mut RunAccess<'_, 'a>",
        ),
        (
            "cons_while_reader_live",
            "access: &'a RunAccess<'_, '_>",
            "access: &RunAccess<'_, 'a>",
        ),
    ];
    let output = std::env::temp_dir().join(format!("p2ht-brand-mutations-{}", std::process::id()));
    for (name, before, after) in mutations {
        assert_eq!(
            brand.matches(before).count(),
            1,
            "one model brand for {name}"
        );
        assert!(!before.contains('\n') && !after.contains('\n'));
        let directory = output.join(name);
        fs::create_dir_all(&directory).expect("mutation directory");
        fs::write(
            directory.join("lease_brand.rs"),
            brand.replacen(before, after, 1),
        )
        .expect("mutated brand");
        let body = fs::read_to_string(manifest.join(format!("tests/p2ht_brand/{name}.rs")))
            .expect("model body");
        let path = directory.join(format!("{name}.rs"));
        fs::write(&path, body).expect("mutated body");
        cases.pass(path);
    }
}
