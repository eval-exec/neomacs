//! Compile-fail contracts for E5.1's serial turn (E5.1-TYPES §4).
//!
//! Every negative path-includes the real `serial_turn.rs`. Where one line
//! decides a contract, its control copies the real source with exactly that
//! line changed, and the same fixture body then type-checks. The move-only
//! negatives (a turn used after `end` or `park`, `park` inside `with_world`)
//! and the private dormant Context have no one-line control: they rest on
//! `self` consumption and field privacy, which rustc reports directly.

#[test]
#[cfg(feature = "contract-fixtures")]
fn e51_turn_compile_contracts() {
    use std::fs;
    use std::path::PathBuf;

    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/e51_turn_ui/*.rs");

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let turn = fs::read_to_string(
        manifest.join("../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/serial_turn.rs"),
    )
    .expect("real serial-turn source");
    let bounds = fs::read_to_string(manifest.join("tests/e51_turn_ui/support/thread_bounds.rs"))
        .expect("source-local Send test bound");
    let output = manifest.join("../../tmp/e51/contract-mutations");

    enum Source {
        Turn,
        ThreadBounds,
    }
    let mutations = [
        (
            "world_view_escapes",
            Source::Turn,
            "pub(crate) fn with_world<R>(&mut self, body: impl for<'g> FnOnce(&'g mut C) -> R) -> R {",
            "pub(crate) fn with_world<'v, R>(&'v mut self, body: impl FnOnce(&'v mut C) -> R) -> R {",
        ),
        (
            "serial_turn_not_send",
            Source::ThreadBounds,
            "impl<T: Send> ThreadTransfer<T> {",
            "impl<T> ThreadTransfer<T> {",
        ),
        (
            "owner_serial_state_not_send",
            Source::ThreadBounds,
            "impl<T: Send> ThreadTransfer<T> {",
            "impl<T> ThreadTransfer<T> {",
        ),
    ];
    for (name, source, before, after) in mutations {
        let directory = output.join(name);
        fs::create_dir_all(&directory).expect("isolated mutation directory");
        let mutate = |text: &str| {
            assert_eq!(text.matches(before).count(), 1, "one line for {name}");
            assert!(!before.contains('\n') && !after.contains('\n'));
            text.replacen(before, after, 1)
        };
        let (turn_copy, bounds_copy) = match source {
            Source::Turn => (mutate(&turn), bounds.clone()),
            Source::ThreadBounds => (turn.clone(), mutate(&bounds)),
        };
        fs::write(directory.join("serial_turn.rs"), turn_copy).expect("isolated turn copy");
        fs::write(directory.join("thread_bounds.rs"), bounds_copy)
            .expect("isolated fixture bound copy");
        let fixture = fs::read_to_string(manifest.join(format!("tests/e51_turn_ui/{name}.rs")))
            .expect("negative fixture body");
        let fixture = fixture
            .replace(
                "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/serial_turn.rs",
                "serial_turn.rs",
            )
            .replace("support/thread_bounds.rs", "thread_bounds.rs");
        let path = directory.join(format!("{name}_mutation.rs"));
        fs::write(&path, fixture).expect("unchanged negative body with isolated module paths");
        cases.pass(path);
    }
}
