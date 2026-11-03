//! Architecture guard for peer ruling A2: `neovm-value/test-support` is
//! enabled by exactly one manifest site, `neovm-core`'s `[dev-dependencies]`.
//!
//! Fail-closed by construction: every workspace manifest is parsed as TOML;
//! a parse error panics.  The guard inspects every dependency table
//! (normal/dev/build, target-specific and the workspace table), resolves
//! renamed packages through `package = "..."`, and treats any enabler outside
//! the single signed site (or a value default feature that enables it) as a
//! failure.  It also requires the single enabler to be present, so deleting
//! it by accident cannot silently turn the test surface off.

use std::path::{Path, PathBuf};

const VALUE: &str = "neovm-value";
const FEATURE: &str = "test-support";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/neovm-core is two levels below the workspace root")
        .to_path_buf()
}

fn manifests(root: &Path) -> Vec<(String, toml::Value)> {
    let mut out = Vec::new();
    let root_manifest = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&root_manifest)
        .unwrap_or_else(|e| panic!("read {}: {e}", root_manifest.display()));
    let doc: toml::Value =
        toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", root_manifest.display()));
    out.push(("Cargo.toml".to_string(), doc));
    let crates = root.join("crates");
    let mut dirs: Vec<_> = std::fs::read_dir(&crates)
        .unwrap_or_else(|e| panic!("read {}: {e}", crates.display()))
        .map(|entry| entry.expect("crate dir entry").path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let manifest = dir.join("Cargo.toml");
        if !manifest.exists() {
            continue;
        }
        let rel = manifest
            .strip_prefix(root)
            .expect("manifest under workspace root")
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()));
        let doc: toml::Value =
            toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", manifest.display()));
        out.push((rel, doc));
    }
    out
}

fn dependency_tables(doc: &toml::Value) -> Vec<(String, &toml::Table)> {
    let mut out = Vec::new();
    let Some(root) = doc.as_table() else {
        return out;
    };
    for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = root.get(key).and_then(toml::Value::as_table) {
            out.push((key.to_string(), table));
        }
    }
    if let Some(workspace) = root.get("workspace").and_then(toml::Value::as_table) {
        if let Some(table) = workspace
            .get("dependencies")
            .and_then(toml::Value::as_table)
        {
            out.push(("workspace.dependencies".to_string(), table));
        }
    }
    if let Some(targets) = root.get("target").and_then(toml::Value::as_table) {
        for (target, value) in targets {
            let Some(target) = value.as_table() else {
                continue;
            };
            for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(table) = target.get(key).and_then(toml::Value::as_table) {
                    out.push((format!("target.{target}.{key}"), table));
                }
            }
        }
    }
    out
}

fn enables_test_support(key: &str, dep: &toml::Value) -> bool {
    let package = dep
        .as_table()
        .and_then(|t| t.get("package"))
        .and_then(toml::Value::as_str)
        .unwrap_or(key);
    if package != VALUE {
        return false;
    }
    match dep {
        // A bare version string cannot enable a non-default feature.
        toml::Value::String(_) => false,
        toml::Value::Table(table) => table
            .get("features")
            .and_then(toml::Value::as_array)
            .is_some_and(|features| {
                features
                    .iter()
                    .filter_map(toml::Value::as_str)
                    .any(|f| f == FEATURE)
            }),
        _ => panic!("unexpected dependency value for {key}: {dep:?}"),
    }
}

#[test]
fn architecture_test_support_is_enabled_only_by_core_dev_dependencies() {
    let root = workspace_root();
    let mut enablers = Vec::new();
    let mut value_default_has_feature = false;
    for (manifest, doc) in manifests(&root) {
        for (table, deps) in dependency_tables(&doc) {
            for (key, dep) in deps {
                if enables_test_support(key, dep) {
                    enablers.push((manifest.clone(), table.clone()));
                }
            }
        }
        if manifest == "crates/neovm-value/Cargo.toml" {
            let features = doc
                .get("features")
                .and_then(toml::Value::as_table)
                .unwrap_or_else(|| panic!("{manifest}: missing [features] table"));
            let default = features
                .get("default")
                .and_then(toml::Value::as_array)
                .unwrap_or_else(|| panic!("{manifest}: missing default feature list"));
            value_default_has_feature = default
                .iter()
                .filter_map(toml::Value::as_str)
                .any(|f| f == FEATURE);
            assert!(
                features.contains_key(FEATURE),
                "{manifest}: the `{FEATURE}` feature must stay declared"
            );
        }
    }
    assert!(
        !value_default_has_feature,
        "neovm-value must not enable `{FEATURE}` through its default features"
    );
    assert_eq!(
        enablers,
        vec![(
            "crates/neovm-core/Cargo.toml".to_string(),
            "dev-dependencies".to_string()
        )],
        "`{VALUE}/{FEATURE}` must be enabled only by neovm-core [dev-dependencies]; found {enablers:?}"
    );
}
