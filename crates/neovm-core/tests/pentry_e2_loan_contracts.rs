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
    for fixture in target
        .fixtures(
            "tests/pentry_e2_loan_ui",
            "tests/pentry_e2_loan_ui/arm64_macos",
        )
        .into_iter()
        .filter(|fixture| {
            // Transfer fixtures have their own runner so a bless of those
            // stderrs cannot rewrite the loan goldens.
            fixture
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| !name.starts_with("transfer_"))
        })
    {
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

#[test]
#[cfg(feature = "contract-fixtures")]
fn pentry_e2_loan_transfer_compile_contracts() {
    use std::fs;
    use std::path::PathBuf;

    let cases = trybuild::TestCases::new();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fail = [
        "transfer_heap_root_from_original_requires_unsafe.rs",
        "transfer_heap_root_not_clone.rs",
        "transfer_heap_root_not_copy.rs",
        "transfer_heap_root_not_send.rs",
        "transfer_heap_root_not_sync.rs",
        "transfer_prepared_prepare_detached_requires_unsafe.rs",
        "transfer_prepared_not_clone.rs",
        "transfer_prepared_not_copy.rs",
        "transfer_prepared_not_sync.rs",
        "transfer_pending_has_no_constructor.rs",
        "transfer_pending_not_clone.rs",
        "transfer_pending_not_copy.rs",
        "transfer_pending_not_send.rs",
        "transfer_pending_not_sync.rs",
        "transfer_returned_has_no_constructor.rs",
        "transfer_returned_not_clone.rs",
        "transfer_returned_not_copy.rs",
        "transfer_returned_not_sync.rs",
        "transfer_restored_has_no_constructor.rs",
        "transfer_restored_into_source_parts_requires_unsafe.rs",
        "transfer_restored_not_clone.rs",
        "transfer_restored_not_copy.rs",
        "transfer_restored_not_send.rs",
        "transfer_restored_not_sync.rs",
    ];
    for name in fail {
        cases.compile_fail(manifest.join("tests/pentry_e2_loan_ui").join(name));
    }
    cases.pass("tests/pentry_e2_loan_pass/transfer_prepared_is_send.rs");
    cases.pass("tests/pentry_e2_loan_pass/transfer_returned_is_send.rs");

    let bounds =
        fs::read_to_string(manifest.join("tests/pentry_e2_loan_ui/support/loan_bounds.rs"))
            .expect("loan bound helper");
    let product = fs::read_to_string(
        manifest.join("../neovm-value/src/tagged/gc/facade_mark/heap_transfer.rs"),
    )
    .expect("heap transfer");
    assert_eq!(
        product.matches("pub unsafe fn new(").count(),
        0,
        "heap transfer must not add a `new` constructor"
    );
    let output = manifest.join("../../tmp/pentry/e2-transfer-mutations");
    let bound_mutations = [
        (
            "transfer_heap_root_not_clone",
            "impl<T: Clone> RequireClone<T> {",
            "impl<T> RequireClone<T> {",
        ),
        (
            "transfer_heap_root_not_copy",
            "impl<T: Copy> RequireCopy<T> {",
            "impl<T> RequireCopy<T> {",
        ),
        (
            "transfer_heap_root_not_send",
            "impl<T: Send> RequireSend<T> {",
            "impl<T> RequireSend<T> {",
        ),
        (
            "transfer_heap_root_not_sync",
            "impl<T: Sync> RequireSync<T> {",
            "impl<T> RequireSync<T> {",
        ),
        (
            "transfer_prepared_not_clone",
            "impl<T: Clone> RequireClone<T> {",
            "impl<T> RequireClone<T> {",
        ),
        (
            "transfer_prepared_not_copy",
            "impl<T: Copy> RequireCopy<T> {",
            "impl<T> RequireCopy<T> {",
        ),
        (
            "transfer_prepared_not_sync",
            "impl<T: Sync> RequireSync<T> {",
            "impl<T> RequireSync<T> {",
        ),
        (
            "transfer_pending_not_clone",
            "impl<T: Clone> RequireClone<T> {",
            "impl<T> RequireClone<T> {",
        ),
        (
            "transfer_pending_not_copy",
            "impl<T: Copy> RequireCopy<T> {",
            "impl<T> RequireCopy<T> {",
        ),
        (
            "transfer_pending_not_send",
            "impl<T: Send> RequireSend<T> {",
            "impl<T> RequireSend<T> {",
        ),
        (
            "transfer_pending_not_sync",
            "impl<T: Sync> RequireSync<T> {",
            "impl<T> RequireSync<T> {",
        ),
        (
            "transfer_returned_not_clone",
            "impl<T: Clone> RequireClone<T> {",
            "impl<T> RequireClone<T> {",
        ),
        (
            "transfer_returned_not_copy",
            "impl<T: Copy> RequireCopy<T> {",
            "impl<T> RequireCopy<T> {",
        ),
        (
            "transfer_returned_not_sync",
            "impl<T: Sync> RequireSync<T> {",
            "impl<T> RequireSync<T> {",
        ),
        (
            "transfer_restored_not_clone",
            "impl<T: Clone> RequireClone<T> {",
            "impl<T> RequireClone<T> {",
        ),
        (
            "transfer_restored_not_copy",
            "impl<T: Copy> RequireCopy<T> {",
            "impl<T> RequireCopy<T> {",
        ),
        (
            "transfer_restored_not_send",
            "impl<T: Send> RequireSend<T> {",
            "impl<T> RequireSend<T> {",
        ),
        (
            "transfer_restored_not_sync",
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

    let constructor_mutations = [
        (
            "transfer_heap_root_from_original_requires_unsafe",
            "pub unsafe fn from_original(",
            "struct HeapRootDesignator(*mut ());\n\
             impl HeapRootDesignator {\n    \
             pub fn from_original(root: *mut ()) -> Self { Self(root) }\n}\n\
             fn main() { let _root = HeapRootDesignator::from_original(std::ptr::null_mut()); }\n",
        ),
        (
            "transfer_prepared_prepare_detached_requires_unsafe",
            "pub unsafe fn prepare_detached<'heap>(",
            "struct PreparedHeapTransfer;\n\
             impl PreparedHeapTransfer {\n    \
             pub fn prepare_detached() -> Self { Self }\n}\n\
             fn main() { let _transfer = PreparedHeapTransfer::prepare_detached(); }\n",
        ),
        (
            "transfer_restored_into_source_parts_requires_unsafe",
            "pub unsafe fn into_source_parts(",
            "struct RestoredHeapBacking;\n\
             impl RestoredHeapBacking {\n    \
             pub fn into_source_parts(self) -> Self { self }\n}\n\
             fn main() { let _parts = RestoredHeapBacking.into_source_parts(); }\n",
        ),
        (
            "transfer_pending_has_no_constructor",
            "pub struct PendingHeapReturn {",
            "struct PendingHeapReturn;\n\
             impl PendingHeapReturn {\n    \
             pub fn new() -> Self { Self }\n}\n\
             fn main() { let _pending = PendingHeapReturn::new(); }\n",
        ),
        (
            "transfer_returned_has_no_constructor",
            "pub struct ReturnedHeapTransfer {",
            "struct ReturnedHeapTransfer;\n\
             impl ReturnedHeapTransfer {\n    \
             pub fn new() -> Self { Self }\n}\n\
             fn main() { let _returned = ReturnedHeapTransfer::new(); }\n",
        ),
        (
            "transfer_restored_has_no_constructor",
            "pub struct RestoredHeapBacking {",
            "struct RestoredHeapBacking;\n\
             impl RestoredHeapBacking {\n    \
             pub fn new() -> Self { Self }\n}\n\
             fn main() { let _restored = RestoredHeapBacking::new(); }\n",
        ),
    ];
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
