//! E2a source-only compiler evidence for the generative World-loan lifetime.

#[test]
fn pentry_lifetime_compile_contracts() {
    use std::fs;
    use std::path::PathBuf;

    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/view_outlives_guard.rs");
    cases.pass("tests/ui/pass/normal_guard.rs");
    cases.pass("tests/ui/pass/nested_guard.rs");

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(manifest.join("tests/support/lifetime_contract.rs"))
        .expect("source-only World-loan lifetime model");
    let before = "body: impl for<'g> FnOnce(&mut SerialContext<'g>, &BatonWitness<'g>) -> R,";
    let after = "body: impl FnOnce(&mut SerialContext<'stop>, &BatonWitness<'stop>) -> R,";
    assert_eq!(source.matches(before).count(), 1, "one HRTB lifetime bound");
    assert!(!before.contains('\n') && !after.contains('\n'));

    // This mutation removes only the generative guard-lifetime bound. The
    // World borrow still backs the actual reference in View. The unchanged
    // escaping fixture must now compile, proving which bound rejects it.
    let output = manifest.join("../../tmp/pentry/e2a-lifetime-mutation");
    fs::create_dir_all(&output).expect("isolated lifetime mutation directory");
    fs::write(
        output.join("lifetime_contract.rs"),
        source.replacen(before, after, 1),
    )
    .expect("one-line bound-removal copy of the same model source");
    let fixture = fs::read_to_string(manifest.join("tests/ui/view_outlives_guard.rs"))
        .expect("view escape negative body");
    let fixture = fixture.replace("../support/lifetime_contract.rs", "lifetime_contract.rs");
    let mutation = output.join("view_outlives_guard_mutation.rs");
    fs::write(&mutation, fixture).expect("unchanged negative body with isolated source path");
    cases.pass(mutation);
}
