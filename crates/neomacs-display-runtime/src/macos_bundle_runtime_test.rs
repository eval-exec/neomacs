use super::MacOsBundleRuntime;
use std::fs;

fn workspace_tempdir() -> tempfile::TempDir {
    let workspace_tmp = std::path::Path::new(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("workspace tmp directory");
    tempfile::Builder::new()
        .prefix("macos-bundle-runtime.")
        .tempdir_in(workspace_tmp)
        .expect("workspace-local temp app root")
}

#[test]
fn projects_packaged_media_paths_from_the_executable() {
    let root = workspace_tempdir();
    let contents = root.path().join("neomacs.app/Contents");
    let executable = contents.join("MacOS/neomacs");

    let runtime = MacOsBundleRuntime::from_executable(&executable).expect("app layout");

    assert_eq!(
        runtime.plugin_system_path,
        contents.join("Resources/gstreamer-1.0")
    );
    assert_eq!(
        runtime.plugin_scanner,
        contents.join("Helpers/gst-plugin-scanner")
    );
    assert_eq!(runtime.gio_modules, contents.join("Resources/gio"));
}

#[test]
fn rejects_an_executable_outside_an_app_bundle() {
    assert!(
        MacOsBundleRuntime::from_executable(std::path::Path::new("target/release/neomacs"))
            .is_none()
    );
}

#[test]
fn requires_plugins_and_scanner_before_activating_private_runtime() {
    let root = workspace_tempdir();
    let contents = root.path().join("neomacs.app/Contents");
    let executable = contents.join("MacOS/neomacs");
    let runtime = MacOsBundleRuntime::from_executable(&executable).expect("app layout");

    assert!(!runtime.media_is_complete());

    fs::create_dir_all(&runtime.plugin_system_path).expect("plugin directory");
    fs::create_dir_all(runtime.plugin_scanner.parent().expect("scanner parent"))
        .expect("helper directory");
    fs::write(&runtime.plugin_scanner, b"scanner").expect("scanner fixture");

    assert!(runtime.media_is_complete());
}
