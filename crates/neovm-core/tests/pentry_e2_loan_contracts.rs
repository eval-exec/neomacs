//! Compile-fail contracts for the dormant era-loan types.
//!
//! Each negative names the product type. The mutation control changes exactly
//! one bound in the helper, or removes `unsafe` from a copy of the constructor
//! line, and the same call then type-checks. E1's `pentry_ui` / `pentry_pass`
//! fixtures are not part of this runner.

#[cfg(feature = "contract-fixtures")]
#[path = "thread_safety_ui/support/fixture_target.rs"]
mod fixture_target;

#[test]
#[cfg(feature = "contract-fixtures")]
fn pentry_e2_loan_compile_contracts() {
    use std::fs;
    use std::path::PathBuf;

    let cases = trybuild::TestCases::new();
    let target = fixture_target::FixtureTarget::current();
    for fixture in target.fixtures(
        "tests/pentry_e2_loan_ui",
        "tests/pentry_e2_loan_ui/arm64_macos",
    ) {
        cases.compile_fail(fixture);
    }
    cases.pass("tests/pentry_e2_loan_pass/world_loan_is_send.rs");

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let bounds =
        fs::read_to_string(manifest.join("tests/pentry_e2_loan_ui/support/loan_bounds.rs"))
            .expect("loan bound helper");
    let product = fs::read_to_string(
        manifest.join("../neovm-value/src/tagged/gc/mutator_registration/world_loan.rs"),
    )
    .expect("real loan seam");
    let output = manifest.join("../../tmp/pentry/e2-loan-mutations");

    let bound_mutations = [
        (
            "world_identity_not_clone",
            "impl<T: Clone> RequireClone<T> {",
            "impl<T> RequireClone<T> {",
        ),
        (
            "world_identity_not_copy",
            "impl<T: Copy> RequireCopy<T> {",
            "impl<T> RequireCopy<T> {",
        ),
        (
            "symbol_root_designators_not_clone",
            "impl<T: Clone> RequireClone<T> {",
            "impl<T> RequireClone<T> {",
        ),
        (
            "symbol_root_designators_not_copy",
            "impl<T: Copy> RequireCopy<T> {",
            "impl<T> RequireCopy<T> {",
        ),
        (
            "world_loan_not_clone",
            "impl<T: Clone> RequireClone<T> {",
            "impl<T> RequireClone<T> {",
        ),
        (
            "world_loan_not_copy",
            "impl<T: Copy> RequireCopy<T> {",
            "impl<T> RequireCopy<T> {",
        ),
        (
            "world_loan_not_sync",
            "impl<T: Sync> RequireSync<T> {",
            "impl<T> RequireSync<T> {",
        ),
    ];
    for (name, before, after) in bound_mutations {
        assert_eq!(bounds.matches(before).count(), 1, "one bound for {name}");
        assert!(!before.contains('\n') && !after.contains('\n'));
        let directory = output.join(name);
        fs::create_dir_all(&directory).expect("isolated mutation directory");
        fs::write(
            directory.join("loan_bounds.rs"),
            bounds.replacen(before, after, 1),
        )
        .expect("mutated bound helper");
        let fixture =
            fs::read_to_string(manifest.join(format!("tests/pentry_e2_loan_ui/{name}.rs")))
                .expect("negative fixture body");
        let fixture = fixture.replace("support/loan_bounds.rs", "loan_bounds.rs");
        let path = directory.join(format!("{name}_mutation.rs"));
        fs::write(&path, fixture).expect("negative body against the mutated bound");
        cases.pass(path);
    }

    // The product constructors are `unsafe fn`. Dropping that word from a
    // one-line copy of each signature makes the same call compile. The
    // negative fixtures above call the real functions and must not.
    let constructor_mutations = [
        (
            "world_identity_new_requires_unsafe",
            "pub unsafe fn new(address: usize) -> Self {",
            "struct WorldIdentity(usize);\n\
             impl WorldIdentity {\n    \
             pub fn new(address: usize) -> Self { Self(address) }\n}\n\
             fn main() { let _identity = WorldIdentity::new(0); }\n",
        ),
        (
            "symbol_root_designators_new_requires_unsafe",
            "world: WorldIdentity,",
            "struct WorldIdentity(usize);\n\
             struct SymbolRootDesignators(WorldIdentity);\n\
             impl WorldIdentity {\n    \
             pub fn new(address: usize) -> Self { Self(address) }\n}\n\
             impl SymbolRootDesignators {\n    \
             pub fn new(world: WorldIdentity) -> Self { Self(world) }\n}\n\
             fn main() {\n    \
             let _roots = SymbolRootDesignators::new(WorldIdentity::new(0));\n}\n",
        ),
    ];
    assert_eq!(
        product
            .matches("pub unsafe fn new(address: usize) -> Self {")
            .count(),
        1,
        "WorldIdentity::new stays a single unsafe constructor"
    );
    assert_eq!(
        product.matches("pub unsafe fn new(").count(),
        2,
        "both identity constructors stay unsafe"
    );
    for (name, before, mutated_body) in constructor_mutations {
        assert!(
            product.contains(before),
            "constructor source for {name} must contain {before}"
        );
        assert!(!before.contains('\n'));
        assert!(
            !mutated_body.contains("unsafe"),
            "mutation must drop unsafe for {name}"
        );
        let directory = output.join(name);
        fs::create_dir_all(&directory).expect("isolated constructor mutation directory");
        let path = directory.join(format!("{name}_mutation.rs"));
        fs::write(&path, mutated_body).expect("safe copy of the constructor call");
        cases.pass(path);
    }
}
