//! Tests about the crate as a whole rather than one module.

use super::*;

mod process_test;

/// The package-TUI save stage renders `Write file: <home>/.gitignore` on a
/// single 160-column row.  A deep workspace path used to push the sandbox
/// HOME past that bound because the case root lived under the MELPA cache
/// root; the harness now picks a base that fits.
#[test]
fn melpa_sandbox_home_fits_the_package_tui_prompt_row() {
    for label in [
        "helm-gitignore-workflows",
        "helm-gitignore-list-failure",
        "helm-gitignore-generation-failure",
        "corfu",
    ] {
        let base = sandbox_case_base(label);
        let prefix_and_suffix = sanitize_label(label).len() + 1 + 6;
        let worst_home = base.as_os_str().len() + 1 + prefix_and_suffix + "/home".len();
        assert!(
            worst_home <= MAX_SANDBOX_HOME,
            "sandbox base {} for {label:?} leaves a {worst_home}-byte HOME",
            base.display()
        );

        let sandbox = MelpaSandbox::new(label).expect("create melpa sandbox");
        let prompt_width =
            "Write file: ".len() + sandbox.home.as_os_str().len() + "/.gitignore".len();
        assert!(
            prompt_width < 160,
            "label {label:?}: HOME {} needs a {prompt_width}-column prompt row",
            sandbox.home.display()
        );
    }
}
