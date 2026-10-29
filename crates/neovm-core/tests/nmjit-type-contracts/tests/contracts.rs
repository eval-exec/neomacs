//! Same positive and seven negative clients used by the core N1 entry.
//! Isolate compiler work from engine dependencies; sketches grant no authority.
#[test]
fn nmjit_n1_sketch_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass_checked.rs");
    cases.compile_fail("tests/ui/fail_*.rs");
}
