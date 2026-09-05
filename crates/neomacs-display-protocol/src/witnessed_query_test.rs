//! Guards the rule that a hit test cannot be built from raw coordinates.
//!
//! `PresentedHitQuery` takes a `PresentationFramePoint`, which only a
//! projection or a `PresentMapping` produces, so a query cannot silently name
//! the wrong pixel while panes are in motion. That guarantee only holds while
//! this crate publishes no *other* way to ask about a point. It did: for a
//! while `PresentedPointerMap::hit_test(x, y)` sat beside the query, answering
//! the same question from two `f32`s with no witness at all, and a call site
//! reaching for it would have compiled and been wrong only during a morph.
//!
//! A validating constructor can be bypassed and a missing one cannot, so the
//! rule is enforced on the shape of the public surface rather than inside it:
//! no published method may answer a question *about* the receiver from a bare
//! `(x: f32, y: f32)` pair. Constructors that take a position (`add_char`,
//! `GeometryPoint::from_px`, and the rest) are not queries — they are how a
//! position enters the vocabulary in the first place — and are excluded by
//! taking `&mut self` or no receiver.

use std::path::{Path, PathBuf};

/// Every published `&self` method whose parameters name a raw coordinate pair,
/// as `file::method` -> why it is not a hit test in disguise.
///
/// Empty on purpose. An entry here is a claim that a caller cannot use the
/// method to resolve a pointer against a presentation; if that is hard to
/// argue, the method should take a witnessed point instead.
fn allowlist() -> &'static [(&'static str, &'static str)] {
    &[]
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn production_sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                out.push(path);
            }
        }
    }
    let root = crate_root().join("src");
    let mut found = Vec::new();
    walk(&root, &mut found);
    found.sort();
    found
        .iter()
        .filter(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            !name.ends_with("_test.rs") && name != "tests.rs"
        })
        .map(|path| {
            let relative = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            (relative, std::fs::read_to_string(path).unwrap_or_default())
        })
        .collect()
}

/// The parameter list of the `pub fn` whose `(` is at `open`.
fn parameter_list(source: &str, open: usize) -> &str {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    for index in open..bytes.len() {
        match bytes[index] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return &source[open..=index];
                }
            }
            _ => {}
        }
    }
    &source[open..]
}

fn raw_coordinate_queries() -> Vec<String> {
    let mut found = Vec::new();
    for (file, source) in production_sources() {
        let mut search = 0usize;
        while let Some(offset) = source[search..].find("pub fn ") {
            let start = search + offset + "pub fn ".len();
            search = start;
            let Some(name_end) = source[start..].find('(') else {
                break;
            };
            let name = source[start..start + name_end].trim();
            if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name.is_empty() {
                continue;
            }
            let parameters = parameter_list(&source, start + name_end);
            let takes_raw_pair = parameters.contains("x: f32") && parameters.contains("y: f32");
            let borrows_receiver =
                parameters.contains("&self") && !parameters.contains("&mut self");
            if takes_raw_pair && borrows_receiver {
                found.push(format!("{file}::{name}"));
            }
        }
    }
    found
}

#[test]
fn no_published_method_resolves_a_point_from_coordinates_that_never_met_a_projection() {
    // If this fails, the crate offers a second way to ask where a pointer is —
    // one that takes the pointer's position at face value. Every such answer
    // agrees with the pixels only while the panes are settled, and disagrees
    // for the whole length of a `split-window` morph, which is exactly the bug
    // `PresentedHitQuery`'s missing raw constructor exists to make impossible.
    let mut unexplained: Vec<String> = raw_coordinate_queries();
    unexplained.retain(|found| !allowlist().iter().any(|(allowed, _)| allowed == found));

    assert!(
        unexplained.is_empty(),
        "these take a raw coordinate pair instead of a witnessed point: {unexplained:?}. \
         Take a `PresentationFramePoint` (or a `PresentedHitQuery`), or add an entry to \
         this test's allowlist saying why the method cannot resolve a pointer.",
    );
}

#[test]
fn the_guard_can_see_a_raw_coordinate_query_when_one_exists() {
    // Without this the guard above is indistinguishable from a scanner that
    // matches nothing at all, and would keep passing after a refactor renamed
    // `pub fn` or changed how parameter lists are written.
    let source = "impl T {\n    pub fn probe(&self, x: f32, y: f32) -> bool { true }\n}\n";
    let open = source.find('(').expect("the fixture has a parameter list");
    let parameters = parameter_list(source, open);

    assert!(parameters.contains("x: f32") && parameters.contains("y: f32"));
    assert!(parameters.contains("&self") && !parameters.contains("&mut self"));
}
