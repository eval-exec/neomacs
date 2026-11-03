//! C3: the leaf depends only on serde, never on display-protocol or a
//! `neovm-*`/`neomacs-*` crate.

/// Dependency keys from `[dependencies]` and `[build-dependencies]`.
fn dependency_names(manifest: &str) -> Vec<String> {
    let mut in_dependencies = false;
    let mut names = Vec::new();
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_dependencies = trimmed == "[dependencies]" || trimmed == "[build-dependencies]";
            continue;
        }
        if !in_dependencies || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let name = trimmed
            .split(['.', ' ', '='])
            .next()
            .unwrap_or_default()
            .to_owned();
        if !name.is_empty() {
            names.push(name);
        }
    }
    names
}

#[test]
fn leaf_depends_only_on_serde() {
    let manifest = include_str!("../Cargo.toml");
    let names = dependency_names(manifest);
    assert_eq!(
        names,
        ["serde"],
        "neovm-display-identity may depend only on serde; found {names:?}"
    );
    for name in names {
        assert!(
            !name.starts_with("neovm-") && !name.starts_with("neomacs-"),
            "the leaf must not depend on a runtime/protocol crate: {name}"
        );
    }
}
