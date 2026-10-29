//! Compile-time production-entry contracts and bound-removal controls.
//!
//! Mutation controls include an isolated copy of the real capability source
//! with exactly one bound changed. An accidental unrelated compiler error
//! therefore cannot bless a negative.

#[test]
#[cfg(feature = "contract-fixtures")]
fn pentry_compile_contracts() {
    use std::fs;
    use std::path::PathBuf;

    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/pentry_ui/*.rs");
    cases.pass("tests/pentry_pass/*.rs");

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let capability = fs::read_to_string(
        manifest.join("src/emacs_core/runtime/eval/parallel_entry/capability.rs"),
    )
    .expect("real production-entry capability source");
    let bounds = fs::read_to_string(manifest.join("tests/pentry_ui/support/thread_bounds.rs"))
        .expect("source-local Send/Sync test bounds");
    let output = manifest.join("../../tmp/pentry/contract-mutations");

    enum Source {
        Capability,
        ThreadBounds,
    }
    let mutations = [
        (
            "wrong_heap",
            Source::Capability,
            "pub fn require_heap(&self, _: &HeapIdentity<'heap>) {}",
            "pub fn require_heap(&self, _: &HeapIdentity<'_>) {}",
        ),
        (
            "wrong_owner",
            Source::Capability,
            "pub fn require_owner(&self, _: &OwnerIdentity<'owner>) {}",
            "pub fn require_owner(&self, _: &OwnerIdentity<'_>) {}",
        ),
        (
            "stale_shape",
            Source::Capability,
            "pub fn require_shape(&self, _: &BindingShape<'shape>) {}",
            "pub fn require_shape(&self, _: &BindingShape<'_>) {}",
        ),
        (
            "witness_escape",
            Source::Capability,
            "body: impl for<'guard> FnOnce(BatonWitness<'guard, 'heap, 'owner>) -> R,",
            "body: impl FnOnce(BatonWitness<'static, 'heap, 'owner>) -> R,",
        ),
        (
            "run_plus_serial",
            Source::Capability,
            "pub fn parallel(&mut self) -> ParallelExecution<'_, 'heap, 'owner, 'shape> {",
            "pub fn parallel(&mut self) -> ParallelExecution<'static, 'heap, 'owner, 'shape> {",
        ),
        (
            "native_plus_heap",
            Source::Capability,
            "pub fn native(&mut self) -> NativeExecution<'_, 'heap, 'owner> {",
            "pub fn native(&mut self) -> NativeExecution<'static, 'heap, 'owner> {",
        ),
        (
            "illegal_send",
            Source::ThreadBounds,
            "impl<T: Send> ThreadTransfer<T> {",
            "impl<T> ThreadTransfer<T> {",
        ),
        (
            "illegal_sync",
            Source::ThreadBounds,
            "impl<T: Sync> ThreadSharing<T> {",
            "impl<T> ThreadSharing<T> {",
        ),
    ];
    for (name, source, before, after) in mutations {
        let directory = output.join(name);
        fs::create_dir_all(&directory).expect("isolated mutation directory");
        let mutate = |text: &str| {
            assert_eq!(text.matches(before).count(), 1, "one bound for {name}");
            assert!(!before.contains('\n') && !after.contains('\n'));
            text.replacen(before, after, 1)
        };
        let (capability_copy, bounds_copy) = match source {
            Source::Capability => (mutate(&capability), bounds.clone()),
            Source::ThreadBounds => (capability.clone(), mutate(&bounds)),
        };
        fs::write(directory.join("capability.rs"), capability_copy)
            .expect("isolated capability copy");
        fs::write(directory.join("thread_bounds.rs"), bounds_copy)
            .expect("isolated fixture bound copy");
        let fixture = fs::read_to_string(manifest.join(format!("tests/pentry_ui/{name}.rs")))
            .expect("negative fixture body");
        let fixture = fixture
            .replace(
                "../../src/emacs_core/runtime/eval/parallel_entry/capability.rs",
                "capability.rs",
            )
            .replace("support/thread_bounds.rs", "thread_bounds.rs");
        let path = directory.join(format!("{name}_mutation.rs"));
        fs::write(&path, fixture).expect("unchanged negative body with isolated module paths");
        cases.pass(path);
    }
}
