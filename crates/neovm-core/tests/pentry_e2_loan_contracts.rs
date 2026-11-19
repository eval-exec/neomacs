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
                .is_some_and(|name| !name.starts_with("transfer_") && !name.starts_with("receipt_"))
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
        "transfer_observations_detached_has_no_constructor.rs",
        "transfer_observations_detached_not_clone.rs",
        "transfer_observations_detached_not_copy.rs",
        "transfer_observations_detached_not_send.rs",
        "transfer_observations_detached_not_sync.rs",
        "transfer_observation_guard_has_no_constructor.rs",
        "transfer_observation_guard_install_requires_unsafe.rs",
        "transfer_observation_guard_not_clone.rs",
        "transfer_observation_guard_not_copy.rs",
        "transfer_observation_guard_not_send.rs",
        "transfer_observation_guard_not_sync.rs",
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
        (
            "transfer_observations_detached_not_clone",
            "impl<T: Clone> RequireClone<T> {",
            "impl<T> RequireClone<T> {",
        ),
        (
            "transfer_observations_detached_not_copy",
            "impl<T: Copy> RequireCopy<T> {",
            "impl<T> RequireCopy<T> {",
        ),
        (
            "transfer_observations_detached_not_send",
            "impl<T: Send> RequireSend<T> {",
            "impl<T> RequireSend<T> {",
        ),
        (
            "transfer_observations_detached_not_sync",
            "impl<T: Sync> RequireSync<T> {",
            "impl<T> RequireSync<T> {",
        ),
        (
            "transfer_observation_guard_not_clone",
            "impl<T: Clone> RequireClone<T> {",
            "impl<T> RequireClone<T> {",
        ),
        (
            "transfer_observation_guard_not_copy",
            "impl<T: Copy> RequireCopy<T> {",
            "impl<T> RequireCopy<T> {",
        ),
        (
            "transfer_observation_guard_not_send",
            "impl<T: Send> RequireSend<T> {",
            "impl<T> RequireSend<T> {",
        ),
        (
            "transfer_observation_guard_not_sync",
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

    // The observation receipt and the placeholder guard live outside
    // heap_transfer.rs. Their needles are checked against the file that
    // defines the type.
    let detachment = fs::read_to_string(
        manifest.join("../neovm-value/src/tagged/collection_reads/store_state/detachment.rs"),
    )
    .expect("observation detachment");
    let reads = fs::read_to_string(manifest.join("../neovm-value/src/tagged/collection_reads.rs"))
        .expect("collection reads");
    let observation_constructors = [
        (
            "transfer_observations_detached_has_no_constructor",
            "pub struct SourceObservationsDetached {",
            detachment.as_str(),
            "struct SourceObservationsDetached;\n\
             impl SourceObservationsDetached {\n    \
             pub fn new() -> Self { Self }\n}\n\
             fn main() { let _receipt = SourceObservationsDetached::new(); }\n",
        ),
        (
            "transfer_observation_guard_has_no_constructor",
            "pub struct ObservationPlaceholderGuard {",
            reads.as_str(),
            "struct ObservationPlaceholderGuard;\n\
             impl ObservationPlaceholderGuard {\n    \
             pub fn new() -> Self { Self }\n}\n\
             fn main() { let _guard = ObservationPlaceholderGuard::new(); }\n",
        ),
        (
            "transfer_observation_guard_install_requires_unsafe",
            "pub unsafe fn install()",
            reads.as_str(),
            "struct ObservationPlaceholderGuard;\n\
             impl ObservationPlaceholderGuard {\n    \
             pub fn install() -> Self { Self }\n}\n\
             fn main() { let _guard = ObservationPlaceholderGuard::install(); }\n",
        ),
    ];
    for (name, before, source, mutated_body) in observation_constructors {
        assert!(
            source.contains(before),
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
fn pentry_e2_receipt_compile_contracts() {
    use std::fs;
    use std::path::PathBuf;

    let cases = trybuild::TestCases::new();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fail = [
        "receipt_detached_registry_detach_requires_unsafe.rs",
        "receipt_detached_registry_has_no_constructor.rs",
        "receipt_detached_registry_not_clone.rs",
        "receipt_detached_registry_not_copy.rs",
        "receipt_detached_registry_not_send.rs",
        "receipt_detached_registry_not_sync.rs",
        "receipt_detach_failure_has_no_constructor.rs",
        "receipt_restore_failure_has_no_constructor.rs",
        "receipt_quit_detach_requires_unsafe.rs",
        "receipt_quit_activation_has_no_constructor.rs",
        "receipt_quit_activation_not_clone.rs",
        "receipt_quit_activation_not_copy.rs",
        "receipt_quit_activation_not_send.rs",
        "receipt_quit_activation_not_sync.rs",
        "receipt_quit_restore_failure_has_no_constructor.rs",
        "receipt_prepared_cache_has_no_constructor.rs",
        "receipt_prepared_cache_not_clone.rs",
        "receipt_prepared_cache_not_copy.rs",
        "receipt_prepared_cache_not_send.rs",
        "receipt_prepared_cache_not_sync.rs",
        "receipt_integer_width_is_private.rs",
        "receipt_in_flight_is_private.rs",
    ];
    for name in fail {
        cases.compile_fail(manifest.join("tests/pentry_e2_loan_ui").join(name));
    }

    let bounds =
        fs::read_to_string(manifest.join("tests/pentry_e2_loan_ui/support/loan_bounds.rs"))
            .expect("loan bound helper");
    let detachment =
        fs::read_to_string(manifest.join("../neovm-value/src/heap_registry/detachment.rs"))
            .expect("registry detachment");
    let quit = fs::read_to_string(manifest.join("../neovm-value/src/quit/quit_detach.rs"))
        .expect("quit detachment");
    let retirement =
        fs::read_to_string(manifest.join("../neovm-value/src/heap_registry/text_retirement.rs"))
            .expect("text retirement");
    let integer_width = fs::read_to_string(
        manifest.join("../neovm-eval/src/emacs_core/lisp/native/builtins/arithmetic/activation.rs"),
    )
    .expect("integer-width activation");
    let in_flight =
        fs::read_to_string(manifest.join("../neovm-value/src/error/registry_detach.rs"))
            .expect("in-flight detachment");
    assert_eq!(
        detachment.matches("pub unsafe fn detach(").count(),
        1,
        "generic detach stays one unsafe constructor"
    );
    assert_eq!(
        quit.matches("pub unsafe fn detach_quit_activation(")
            .count(),
        1,
        "quit detach stays one unsafe constructor"
    );
    let output = manifest.join("../../tmp/pentry-r2/e2-receipt-mutations");
    let bound_mutations = [
        (
            "receipt_detached_registry_not_clone",
            "impl<T: Clone> RequireClone<T> {",
            "impl<T> RequireClone<T> {",
        ),
        (
            "receipt_detached_registry_not_copy",
            "impl<T: Copy> RequireCopy<T> {",
            "impl<T> RequireCopy<T> {",
        ),
        (
            "receipt_detached_registry_not_send",
            "impl<T: Send> RequireSend<T> {",
            "impl<T> RequireSend<T> {",
        ),
        (
            "receipt_detached_registry_not_sync",
            "impl<T: Sync> RequireSync<T> {",
            "impl<T> RequireSync<T> {",
        ),
        (
            "receipt_quit_activation_not_clone",
            "impl<T: Clone> RequireClone<T> {",
            "impl<T> RequireClone<T> {",
        ),
        (
            "receipt_quit_activation_not_copy",
            "impl<T: Copy> RequireCopy<T> {",
            "impl<T> RequireCopy<T> {",
        ),
        (
            "receipt_quit_activation_not_send",
            "impl<T: Send> RequireSend<T> {",
            "impl<T> RequireSend<T> {",
        ),
        (
            "receipt_quit_activation_not_sync",
            "impl<T: Sync> RequireSync<T> {",
            "impl<T> RequireSync<T> {",
        ),
        (
            "receipt_prepared_cache_not_clone",
            "impl<T: Clone> RequireClone<T> {",
            "impl<T> RequireClone<T> {",
        ),
        (
            "receipt_prepared_cache_not_copy",
            "impl<T: Copy> RequireCopy<T> {",
            "impl<T> RequireCopy<T> {",
        ),
        (
            "receipt_prepared_cache_not_send",
            "impl<T: Send> RequireSend<T> {",
            "impl<T> RequireSend<T> {",
        ),
        (
            "receipt_prepared_cache_not_sync",
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
            &detachment,
            "receipt_detached_registry_detach_requires_unsafe",
            "pub unsafe fn detach(",
            "struct HeapRegistrySlot;\n\
             struct DetachedRegistry;\n\
             impl HeapRegistrySlot {\n    \
             pub fn detach(&self) -> DetachedRegistry { DetachedRegistry }\n}\n\
             fn main() { let _receipt = HeapRegistrySlot.detach(); }\n",
        ),
        (
            &detachment,
            "receipt_detached_registry_has_no_constructor",
            "pub struct DetachedRegistry<T> {",
            "struct DetachedRegistry;\n\
             impl DetachedRegistry {\n    \
             pub fn new() -> Self { Self }\n}\n\
             fn main() { let _receipt = DetachedRegistry::new(); }\n",
        ),
        (
            &detachment,
            "receipt_detach_failure_has_no_constructor",
            "pub struct RegistryDetachFailure<T> {",
            "struct RegistryDetachFailure;\n\
             impl RegistryDetachFailure {\n    \
             pub fn new() -> Self { Self }\n}\n\
             fn main() { let _failure = RegistryDetachFailure::new(); }\n",
        ),
        (
            &detachment,
            "receipt_restore_failure_has_no_constructor",
            "pub struct RegistryRestoreFailure<T> {",
            "struct RegistryRestoreFailure;\n\
             impl RegistryRestoreFailure {\n    \
             pub fn new() -> Self { Self }\n}\n\
             fn main() { let _failure = RegistryRestoreFailure::new(); }\n",
        ),
        (
            &quit,
            "receipt_quit_detach_requires_unsafe",
            "pub unsafe fn detach_quit_activation(",
            "struct QuitRequest;\n\
             struct DetachedQuitActivation;\n\
             fn detach_quit_activation(_expected: &QuitRequest) -> DetachedQuitActivation {\n    \
             DetachedQuitActivation\n}\n\
             fn main() {\n    \
             let request = QuitRequest;\n    \
             let _receipt = detach_quit_activation(&request);\n}\n",
        ),
        (
            &quit,
            "receipt_quit_activation_has_no_constructor",
            "pub struct DetachedQuitActivation {",
            "struct DetachedQuitActivation;\n\
             impl DetachedQuitActivation {\n    \
             pub fn new() -> Self { Self }\n}\n\
             fn main() { let _receipt = DetachedQuitActivation::new(); }\n",
        ),
        (
            &quit,
            "receipt_quit_restore_failure_has_no_constructor",
            "pub struct QuitRestoreFailure {",
            "struct QuitRestoreFailure;\n\
             impl QuitRestoreFailure {\n    \
             pub fn new() -> Self { Self }\n}\n\
             fn main() { let _failure = QuitRestoreFailure::new(); }\n",
        ),
        (
            &retirement,
            "receipt_prepared_cache_has_no_constructor",
            "pub struct PreparedValueCacheRetirement<'a> {",
            "struct PreparedValueCacheRetirement;\n\
             impl PreparedValueCacheRetirement {\n    \
             pub fn new() -> Self { Self }\n}\n\
             fn main() { let _prepared = PreparedValueCacheRetirement::new(); }\n",
        ),
        (
            &integer_width,
            "receipt_integer_width_is_private",
            "pub(crate) struct DetachedIntegerWidthActivation {",
            "pub struct DetachedIntegerWidthActivation;\n\
             fn main() { let _receipt = DetachedIntegerWidthActivation; }\n",
        ),
        (
            &in_flight,
            "receipt_in_flight_is_private",
            "pub(crate) struct DetachedInFlightRegistry {",
            "pub struct DetachedInFlightRegistry;\n\
             fn main() { let _receipt = DetachedInFlightRegistry; }\n",
        ),
    ];
    for (product, name, before, mutated_body) in constructor_mutations {
        assert!(
            product.contains(before),
            "constructor source for {name} must contain {before}"
        );
        assert!(!before.contains('\n'));
        assert!(
            !mutated_body.contains("unsafe"),
            "mutation must drop the seal for {name}"
        );
        let directory = output.join(name);
        fs::create_dir_all(&directory).expect("isolated constructor mutation directory");
        let path = directory.join(format!("{name}_mutation.rs"));
        fs::write(&path, mutated_body).expect("seal dropped on a local stub");
        cases.pass(path);
    }
}
