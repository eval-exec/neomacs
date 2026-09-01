//! Build-time application identity shared with the Linux desktop entry.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("build output directory"));
    generate_application_identity(&crate_dir, &out_dir);

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        let stubs = crate_dir.join(".mingw-stubs");
        if stubs.exists() {
            println!("cargo:rustc-link-search=native={}", stubs.display());
        }
    }
}

fn generate_application_identity(crate_dir: &Path, out_dir: &Path) {
    let desktop_entry = crate_dir.join("assets/neomacs.desktop");
    println!("cargo:rerun-if-changed={}", desktop_entry.display());

    let desktop_file_id = desktop_entry
        .file_name()
        .and_then(|name| name.to_str())
        .expect("desktop entry must have a UTF-8 filename");
    let app_id = desktop_entry
        .file_stem()
        .and_then(|name| name.to_str())
        .expect("desktop entry must have a UTF-8 stem");
    let contents = fs::read_to_string(&desktop_entry).expect("read canonical desktop entry");
    let icon_name = desktop_entry_value(&contents, "Icon");
    let startup_wm_class = desktop_entry_value(&contents, "StartupWMClass");
    assert_eq!(
        startup_wm_class, app_id,
        "StartupWMClass must match the desktop file stem/application ID"
    );

    let generated = format!(
        "pub(crate) const GENERATED_APP_ID: &str = {app_id:?};\n\
         pub(crate) const GENERATED_DESKTOP_FILE_ID: &str = {desktop_file_id:?};\n\
         pub(crate) const GENERATED_ICON_NAME: &str = {icon_name:?};\n"
    );
    fs::write(out_dir.join("application_identity.rs"), generated)
        .expect("write generated application identity");
}

fn desktop_entry_value<'a>(contents: &'a str, key: &str) -> &'a str {
    let prefix = format!("{key}=");
    let mut matches = contents
        .lines()
        .filter_map(|line| line.strip_prefix(&prefix));
    let value = matches
        .next()
        .unwrap_or_else(|| panic!("canonical desktop entry must define {key}"));
    assert!(
        matches.next().is_none(),
        "canonical desktop entry must define {key} exactly once"
    );
    assert!(!value.is_empty(), "canonical desktop entry has empty {key}");
    value
}
