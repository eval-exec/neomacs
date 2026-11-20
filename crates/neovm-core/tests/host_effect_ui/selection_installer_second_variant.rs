//! The hot installer has one variant. A second arm is a different type.
#[path = "../../../neovm-eval/src/emacs_core/lisp/native/builtins/selection_installer.rs"]
mod installer;

fn main() {
    let _second = installer::SelectionInstaller::OwnedRoutedWorker;
}
