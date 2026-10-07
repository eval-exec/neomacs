use super::*;
use crate::emacs_core::format_eval_result;

fn entries(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[cfg(unix)]
fn sh() -> &'static str {
    ["/bin/sh", "/usr/bin/sh", "/run/current-system/sw/bin/sh"]
        .into_iter()
        .find(|path| Path::new(path).exists())
        .expect("a POSIX shell for the child-process probe")
}

/// Run a real child through `call-process` and return what it printed.
#[cfg(unix)]
fn child_output(eval: &mut Context, script: &str) -> String {
    let form = format!(
        r#"(progn
             (set-buffer (get-buffer-create " *environment-probe*"))
             (erase-buffer)
             (call-process "{sh}" nil t nil "-c" {script:?})
             (buffer-string))"#,
        sh = sh(),
    );
    format_eval_result(&eval.eval_str(&form))
}

/// Print each named variable as `NAME=<value>` when set and `NAME!` when
/// unset, so an empty value stays distinguishable from a missing one.
#[cfg(unix)]
fn report_script(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| format!(r#"if [ -n "${{{name}+x}}" ]; then printf '%s ' "{name}=${name}"; else printf '%s ' "{name}!"; fi;"#))
        .collect()
}

/// The environment a launcher wrapper such as the Nix package's hands the
/// editor: its own library path, runtime root, driver and plugin settings,
/// plus the record of what the user's environment held before the wrapper
/// changed it.
fn wrapped_launch() -> Vec<(String, String)> {
    entries(&[
        ("HOME", "/home/user"),
        ("LD_LIBRARY_PATH", "/nix/store/runtime/lib:/home/user/lib"),
        ("RUST_LOG", "info"),
        ("NEOMACS_RUNTIME_ROOT", "/nix/store/neomacs/share/neomacs"),
        ("VK_DRIVER_FILES", "/nix/store/mesa/icd.json"),
        (
            "GST_PLUGIN_SYSTEM_PATH_1_0",
            "/nix/store/gst/lib/gstreamer-1.0",
        ),
        (
            "NEOMACS_WRAPPER_ENV",
            "LD_LIBRARY_PATH:RUST_LOG:NEOMACS_RUNTIME_ROOT:VK_DRIVER_FILES:GST_PLUGIN_SYSTEM_PATH_1_0",
        ),
        ("NEOMACS_WRAPPER_ORIGINAL_LD_LIBRARY_PATH", "/home/user/lib"),
        ("NEOMACS_WRAPPER_ORIGINAL_GST_PLUGIN_SYSTEM_PATH_1_0", ""),
    ])
}

#[cfg(unix)]
const REPORTED: &[&str] = &[
    "HOME",
    "LD_LIBRARY_PATH",
    "RUST_LOG",
    "NEOMACS_RUNTIME_ROOT",
    "VK_DRIVER_FILES",
    "GST_PLUGIN_SYSTEM_PATH_1_0",
    "NEOMACS_WRAPPER_ENV",
    "NEOMACS_WRAPPER_ORIGINAL_LD_LIBRARY_PATH",
];

#[cfg(unix)]
#[test]
fn child_process_inherits_the_environment_from_before_the_launcher_wrapper() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_environment_snapshot(&mut eval, wrapped_launch());

    assert_eq!(
        child_output(&mut eval, &report_script(REPORTED)),
        concat!(
            r#"OK "HOME=/home/user LD_LIBRARY_PATH=/home/user/lib RUST_LOG! "#,
            r#"NEOMACS_RUNTIME_ROOT! VK_DRIVER_FILES! GST_PLUGIN_SYSTEM_PATH_1_0= "#,
            r#"NEOMACS_WRAPPER_ENV! NEOMACS_WRAPPER_ORIGINAL_LD_LIBRARY_PATH! ""#,
        ),
        "a subprocess must see the user's environment, not the wrapper's private \
         library path, runtime root, drivers or bookkeeping"
    );
}

#[test]
fn lisp_startup_environment_is_the_environment_from_before_the_launcher_wrapper() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_environment_snapshot(&mut eval, wrapped_launch());

    let result = eval.eval_str(
        r#"(list (getenv-internal "LD_LIBRARY_PATH")
                 (getenv-internal "RUST_LOG")
                 (getenv-internal "NEOMACS_RUNTIME_ROOT")
                 (getenv-internal "GST_PLUGIN_SYSTEM_PATH_1_0")
                 (getenv-internal "NEOMACS_WRAPPER_ENV")
                 (getenv-internal "LD_LIBRARY_PATH" initial-environment)
                 (getenv-internal "NEOMACS_WRAPPER_ENV" initial-environment))"#,
    );
    assert_eq!(
        format_eval_result(&result),
        r#"OK ("/home/user/lib" nil nil "" nil "/home/user/lib" nil)"#,
        "process-environment and initial-environment describe the user's \
         environment, as GNU documents for initial-environment"
    );
}

#[cfg(unix)]
#[test]
fn unwrapped_launch_environment_is_inherited_unchanged() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_environment_snapshot(
        &mut eval,
        entries(&[
            ("HOME", "/home/user"),
            ("LD_LIBRARY_PATH", "/opt/user/lib"),
            ("RUST_LOG", "debug"),
            ("NEOMACS_RUNTIME_ROOT", "/src/neomacs"),
        ]),
    );

    assert_eq!(
        child_output(
            &mut eval,
            &report_script(&[
                "HOME",
                "LD_LIBRARY_PATH",
                "RUST_LOG",
                "NEOMACS_RUNTIME_ROOT"
            ]),
        ),
        r#"OK "HOME=/home/user LD_LIBRARY_PATH=/opt/user/lib RUST_LOG=debug NEOMACS_RUNTIME_ROOT=/src/neomacs ""#,
        "without a launcher record every inherited variable is the user's own"
    );
}

#[cfg(unix)]
#[test]
fn launcher_record_restores_removed_variables_and_hides_stray_bookkeeping() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_environment_snapshot(
        &mut eval,
        entries(&[
            ("HOME", "/home/user"),
            ("NEOMACS_WRAPPER_ENV", "PYTHONPATH"),
            ("NEOMACS_WRAPPER_ORIGINAL_PYTHONPATH", "/home/user/python"),
            ("NEOMACS_WRAPPER_ORIGINAL_UNLISTED", "stale"),
        ]),
    );

    assert_eq!(
        child_output(
            &mut eval,
            &report_script(&[
                "HOME",
                "PYTHONPATH",
                "UNLISTED",
                "NEOMACS_WRAPPER_ORIGINAL_UNLISTED"
            ]),
        ),
        r#"OK "HOME=/home/user PYTHONPATH=/home/user/python UNLISTED! NEOMACS_WRAPPER_ORIGINAL_UNLISTED! ""#,
        "a variable the wrapper unset comes back, and bookkeeping never reaches a child"
    );
}
