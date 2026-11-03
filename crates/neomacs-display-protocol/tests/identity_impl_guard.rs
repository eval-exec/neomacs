//! C2 and C3 guards for the display identity vocabulary.
//!
//! C2: `neovm-display-identity` owns every inherent/std/serde impl of the
//! moved types. display-protocol keeps only impls of its own traits; today
//! it has none of those for the moved types, so this guard requires zero
//! impl headers naming them anywhere under `src/`. Adding a display-protocol
//! trait impl for a moved type is allowed by the ruling but must update this
//! guard with peer review.
//!
//! C3: display-protocol must not depend on `neovm-value` (the leaf exists so
//! the protocol re-exports the vocabulary without pulling the value graph).

use std::fs;
use std::path::{Path, PathBuf};

const MOVED: [&str; 7] = [
    "VideoId",
    "WebViewId",
    "FontBackendKind",
    "BackendFontSelector",
    "FontVariationCoord",
    "FontVariationSet",
    "ResolvedFontIdentity",
];

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read display-protocol src") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            source_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

/// Yields (line, impl header) for every `impl`/`unsafe impl` item.
fn impl_headers(source: &str) -> Vec<(usize, String)> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut headers = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let head = lines[index].trim_start();
        let is_impl = head.strip_prefix("impl").is_some_and(|rest| {
            rest.is_empty() || rest.starts_with(['<', ' ', '\t', '(', '{', '!'])
        }) || head
            .strip_prefix("unsafe impl")
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['<', ' ', '\t']));
        if is_impl {
            let mut header = head.to_owned();
            let mut end = index;
            while !header.contains('{') && end + 1 < lines.len() && end - index < 8 {
                end += 1;
                header.push(' ');
                header.push_str(lines[end].trim());
            }
            headers.push((index + 1, header));
            index = end;
        }
        index += 1;
    }
    headers
}

#[test]
fn display_protocol_declares_no_impl_of_a_moved_identity_type() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    source_files(&root, &mut files);
    let mut failures = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path).expect("read source");
        for (line, header) in impl_headers(&source) {
            let header = header.split('{').next().unwrap_or(&header);
            let tokens = header
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .collect::<Vec<_>>();
            for name in MOVED {
                if tokens.contains(&name) {
                    failures.push(format!(
                        "{}:{}: display-protocol declares an impl of moved identity type {name}: {}",
                        path.strip_prefix(&root).unwrap_or(&path).display(),
                        line,
                        header
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn dependency_names(manifest: &str) -> Vec<String> {
    let mut in_dependencies = false;
    let mut names = Vec::new();
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_dependencies = trimmed == "[dependencies]"
                || trimmed == "[build-dependencies]"
                || trimmed.starts_with("[target.") && (trimmed.ends_with("dependencies]"));
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
fn display_protocol_does_not_depend_on_the_value_crate() {
    let manifest = include_str!("../Cargo.toml");
    let names = dependency_names(manifest);
    assert!(
        !names.iter().any(|name| name == "neovm-value"),
        "display-protocol must depend on neovm-display-identity, not neovm-value: {names:?}"
    );
}
