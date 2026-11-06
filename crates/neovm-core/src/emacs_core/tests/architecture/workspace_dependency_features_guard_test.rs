//! Cargo feature hygiene (LEAD 03:5xZ, peer suggestion 03:5xZ): no crate
//! may depend on a sibling `neovm-*` crate with default features enabled.
//! Every intra-workspace `neovm-*` dependency — normal, dev and build,
//! including the `target.*` tables — must disable default features:
//! `default-features = false`.
//!
//! Why: the product graph is checked and built with
//! `--no-default-features`. An edge that leaves a sibling's defaults on
//! re-enables that sibling's `default` features (`jit`, `sqlite`) through
//! feature unification, so `--no-default-features` stops being a no-jit
//! graph. The measured leak at the split base was core->display,
//! display->eval and system->eval; this guard keeps the whole class from
//! coming back.
//!
//! Manifest semantics encoded here:
//! - a plain version edge (`neovm-x = "0.0.20"`) enables defaults: RED;
//! - an inline/table edge without `default-features = false`: RED;
//! - `default-features = true`: RED;
//! - `workspace = true` may take the setting from
//!   `[workspace.dependencies]`; a local `default-features = false` wins,
//!   and a workspace entry that does not disable defaults: RED;
//! - only dependencies whose name is a workspace member and starts with
//!   `neovm-` are checked; a non-neovm or non-member dependency is not
//!   this class (ordinary cargo defaults rules apply).
//!
//! The mutation test builds a synthetic workspace and exercises every RED
//! form and the GREEN forms, including a dev-dependency edge and a
//! workspace-inherited edge.

use std::collections::BTreeSet;
use std::path::Path;

use super::architecture_production_source_roots::workspace_root;

const DEP_SECTIONS: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

fn toml_manifest(path: &Path) -> Result<toml::Value, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    toml::from_str::<toml::Value>(&text).map_err(|error| format!("{}: {error}", path.display()))
}

/// Cargo accepts both the hyphen and the underscore spelling.
fn bool_key(table: &toml::Table, name: &str) -> Option<bool> {
    table
        .get(name)
        .or_else(|| table.get(&name.replace('-', "_")))
        .and_then(toml::Value::as_bool)
}

fn dependency_tables(manifest: &toml::Value) -> Vec<(String, toml::Table)> {
    let mut tables = Vec::new();
    let Some(root) = manifest.as_table() else {
        return tables;
    };
    for section in DEP_SECTIONS {
        if let Some(table) = root.get(section).and_then(toml::Value::as_table) {
            tables.push((section.to_owned(), table.clone()));
        }
    }
    if let Some(targets) = root.get("target").and_then(toml::Value::as_table) {
        for (target, value) in targets {
            let Some(target_table) = value.as_table() else {
                continue;
            };
            for section in DEP_SECTIONS {
                if let Some(table) = target_table.get(section).and_then(toml::Value::as_table) {
                    tables.push((format!("target.{target}.{section}"), table.clone()));
                }
            }
        }
    }
    tables
}

/// Ok when the edge explicitly disables the sibling's default features.
fn edge_disables_defaults(
    name: &str,
    spec: &toml::Value,
    workspace_deps: &toml::Table,
) -> Result<(), String> {
    let table = match spec {
        toml::Value::String(version) => {
            return Err(format!(
                "plain version edge `{version}` enables default features"
            ));
        }
        toml::Value::Table(table) => table,
        other => return Err(format!("unsupported dependency form {other:?}")),
    };
    match bool_key(table, "default-features") {
        Some(false) => return Ok(()),
        Some(true) => return Err("`default-features = true`".to_owned()),
        None => {}
    }
    if bool_key(table, "workspace") == Some(true) {
        let entry = workspace_deps
            .get(name)
            .and_then(toml::Value::as_table)
            .ok_or_else(|| {
                "`workspace = true` without a [workspace.dependencies] entry".to_owned()
            })?;
        return match bool_key(entry, "default-features") {
            Some(false) => Ok(()),
            Some(true) => {
                Err("its [workspace.dependencies] entry sets `default-features = true`".to_owned())
            }
            None => Err(
                "its [workspace.dependencies] entry does not disable default features".to_owned(),
            ),
        };
    }
    Err("missing `default-features = false`".to_owned())
}

fn workspace_dependency_default_feature_violation(workspace: &Path) -> Result<(), String> {
    let root_manifest = toml_manifest(&workspace.join("Cargo.toml"))?;
    let Some(root) = root_manifest.as_table() else {
        return Err("workspace Cargo.toml is not a table".into());
    };
    let Some(workspace_table) = root.get("workspace").and_then(toml::Value::as_table) else {
        return Err("workspace Cargo.toml has no [workspace] table".into());
    };
    let Some(members) = workspace_table
        .get("members")
        .and_then(toml::Value::as_array)
    else {
        return Err("workspace Cargo.toml has no [workspace] members".into());
    };
    let workspace_deps = workspace_table
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .cloned()
        .unwrap_or_default();
    // Map every member package name to its manifest so only intra-workspace
    // `neovm-*` edges are checked (cargo would otherwise treat the name as a
    // registry package).
    let mut member_names = BTreeSet::new();
    let mut manifests = Vec::new();
    for member in members {
        let Some(relative) = member.as_str() else {
            return Err("a workspace member is not a string".into());
        };
        let manifest_path = workspace.join(relative).join("Cargo.toml");
        let manifest = toml_manifest(&manifest_path)?;
        let Some(name) = manifest
            .as_table()
            .and_then(|table| table.get("package"))
            .and_then(toml::Value::as_table)
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
        else {
            return Err(format!("{}: no package.name", manifest_path.display()));
        };
        member_names.insert(name.to_owned());
        manifests.push((name.to_owned(), manifest_path, manifest));
    }
    let mut violations = Vec::new();
    for (crate_name, manifest_path, manifest) in &manifests {
        for (section, table) in dependency_tables(manifest) {
            for (dependency, spec) in &table {
                if !dependency.starts_with("neovm-") || !member_names.contains(dependency) {
                    continue;
                }
                if let Err(reason) = edge_disables_defaults(dependency, spec, &workspace_deps) {
                    violations.push(format!(
                        "{} [{section}] {crate_name} -> {dependency}: {reason}",
                        manifest_path.display()
                    ));
                }
            }
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        violations.sort();
        Err(format!(
            "intra-workspace neovm-* dependencies must set `default-features = false`:\n  {}",
            violations.join("\n  ")
        ))
    }
}

#[test]
fn workspace_dependencies_disable_default_features() {
    workspace_dependency_default_feature_violation(&workspace_root())
        .unwrap_or_else(|error| panic!("WORKSPACE-DEP-DEFAULTS: {error}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write_workspace(root: &Path, workspace_edge: &str) {
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                r#"
[workspace]
members = ["a", "b"]
resolver = "2"

[workspace.dependencies]
neovm-b = {{ path = "b"{workspace_edge} }}
"#
            ),
        )
        .expect("root manifest");
        std::fs::create_dir_all(root.join("a")).expect("a");
        std::fs::create_dir_all(root.join("b")).expect("b");
        std::fs::write(
            root.join("b/Cargo.toml"),
            r#"
[package]
name = "neovm-b"
version = "0.0.0"
edition = "2024"
"#,
        )
        .expect("b manifest");
    }

    fn write_a(root: &Path, section: &str, edge: &str) {
        std::fs::write(
            root.join("a/Cargo.toml"),
            format!(
                r#"
[package]
name = "neovm-a"
version = "0.0.0"
edition = "2024"

[{section}]
{edge}
"#
            ),
        )
        .expect("a manifest");
    }

    #[test]
    fn mutation_forms_are_literal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        write_workspace(root, ", default-features = false");

        // GREEN: explicit false.
        write_a(
            root,
            "dependencies",
            r#"neovm-b = { path = "../b", default-features = false }"#,
        );
        workspace_dependency_default_feature_violation(root).expect("explicit false");

        // GREEN: workspace-inherited, workspace entry disables defaults.
        write_a(root, "dependencies", "neovm-b.workspace = true");
        workspace_dependency_default_feature_violation(root).expect("workspace-inherited false");

        // RED: the mutation case — an intra-workspace edge without the
        // setting.
        write_a(root, "dependencies", r#"neovm-b = { path = "../b" }"#);
        let error =
            workspace_dependency_default_feature_violation(root).expect_err("missing false");
        assert!(error.contains("neovm-b"), "{error}");
        assert!(error.contains("default-features = false"), "{error}");

        // RED: explicit true.
        write_a(
            root,
            "dependencies",
            r#"neovm-b = { path = "../b", default-features = true }"#,
        );
        assert!(workspace_dependency_default_feature_violation(root).is_err());

        // RED: a dev-dependency edge is the same class.
        write_a(root, "dev-dependencies", r#"neovm-b = { path = "../b" }"#);
        assert!(workspace_dependency_default_feature_violation(root).is_err());

        // RED: a workspace entry that does not disable defaults.
        write_workspace(root, "");
        write_a(root, "dependencies", "neovm-b.workspace = true");
        assert!(workspace_dependency_default_feature_violation(root).is_err());

        // RED: a target-specific dependency table is the same class.
        std::fs::write(
            root.join("a/Cargo.toml"),
            r#"
[package]
name = "neovm-a"
version = "0.0.0"
edition = "2024"

[target.'cfg(unix)'.dependencies]
neovm-b = { path = "../b" }
"#,
        )
        .expect("target edge");
        assert!(workspace_dependency_default_feature_violation(root).is_err());

        // GREEN: a non-workspace / non-neovm dependency is not this class.
        write_a(
            root,
            "dependencies",
            r#"serde = { version = "1", default-features = true }"#,
        );
        workspace_dependency_default_feature_violation(root).expect("non-neovm edge");
    }
}
