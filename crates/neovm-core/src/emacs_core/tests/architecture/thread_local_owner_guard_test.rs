//! Architecture guard: no PRODUCTION `thread_local!` may OWN a `Context`,
//! `World` or `TaggedHeap` (peer ruling 11:5xZ G1).
//!
//! `Context::drop` needs live TLS by construction: it clears the thread's
//! `TAGGED_HEAP`, retires registries and joins the heap's collector, whose
//! `Collector::join` emits a tracing event.  A thread-local destructor runs
//! inside TLS destruction, where tracing's own thread-local may already be
//! gone, so an owning fixture dropped there panics with
//! `cannot access a Thread Local Storage value during or after destruction`
//! (std AccessError).  Product code therefore keeps only RAW POINTERS in
//! thread-locals; test fixtures that genuinely need an owning Context use
//! the one typed `ThreadExitLeak` wrapper.
//!
//! The scan covers production sources (files under `crates/*/src`, excluding
//! `/tests/`, `/core_tests/` and `/test_support` test surfaces) and skips
//! statics whose attributes mark them test-only (`test` or the test-only
//! features).  Raw-pointer types are allowed.  The synthetic self-tests
//! below are the mutation RED: a planted owning thread-local is reported.

use std::path::Path;

use super::architecture_production_source_roots::crates_dir as workspace_crates_dir;

/// Types whose ownership may not cross a thread-local destructor.
const OWNING_TYPES: &[&str] = &["Context", "World", "TaggedHeap"];

/// Features that mark a thread-local as test-only, so it is not present in
/// production builds and is not the product invariant's subject.
const TEST_FEATURES: &[&str] = &["test-support", "test-thread-fallback-heap", "fuzzing"];

#[derive(Debug, PartialEq, Eq)]
struct OwnerTls {
    name: String,
    file: String,
}

/// True when any attribute line marks the item test-only.
fn test_only_attributes(attrs: &[String]) -> bool {
    attrs.iter().any(|attr| {
        let trimmed = attr.trim_start();
        if !trimmed.starts_with("#[cfg(") {
            return false;
        }
        if trimmed.contains("test") {
            return true;
        }
        TEST_FEATURES
            .iter()
            .any(|feature| trimmed.contains(&format!("\"{feature}\"")))
    })
}

/// Identifier-boundary search (so `Context` does not match `ContextView`).
fn contains_word(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let at = start + pos;
        let before_ok = at == 0 || !is_ident_byte(bytes[at - 1]);
        let end = at + needle.len();
        let after_ok = end == bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        start = end;
    }
    false
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// A raw-pointer type owns nothing: `*mut Context` / `*const Context` are
/// allowed (the production GC fixtures are exactly this shape).
fn type_owns(type_text: &str) -> bool {
    if type_text.contains("*mut") || type_text.contains("*const") {
        return false;
    }
    OWNING_TYPES
        .iter()
        .any(|owning| contains_word(type_text, owning))
}

/// Collect the contiguous attribute lines immediately before `index`,
/// walking backwards over the line vector.
fn attributes_before(lines: &[&str], index: usize) -> Vec<String> {
    let mut attrs = Vec::new();
    let mut i = index;
    while i > 0 {
        i -= 1;
        let line = lines[i].trim();
        if line.is_empty() {
            // Allow one blank line between attributes and the item.
            continue;
        }
        if line.starts_with("#[") {
            attrs.push(line.to_string());
            continue;
        }
        break;
    }
    attrs
}

/// Extract `static NAME: TYPE` declarations inside one `thread_local!`
/// block, with the attributes that mark them test-only.
fn statics_in_block(block: &str, outer_attrs: &[String]) -> Vec<(String, String, bool)> {
    let lines: Vec<&str> = block.lines().collect();
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("static ") else {
            continue;
        };
        let Some(colon) = rest.find(':') else {
            continue;
        };
        let name = rest[..colon].trim().to_string();
        if name.is_empty() {
            continue;
        }
        // Type text: from the colon to the `=` at zero bracket depth.
        let type_and_more = &rest[colon + 1..];
        let mut depth = 0i32;
        let mut type_text = String::new();
        for ch in type_and_more.chars() {
            match ch {
                '<' | '(' | '[' => depth += 1,
                '>' | ')' | ']' => depth -= 1,
                '=' if depth == 0 => break,
                _ => {}
            }
            type_text.push(ch);
        }
        let mut attrs = outer_attrs.to_vec();
        attrs.extend(attributes_before(&lines, index));
        let test_only = test_only_attributes(&attrs);
        out.push((name, type_text, test_only));
    }
    out
}

/// Scan one source text; returns the offending thread-locals.
fn scan_source(text: &str, file: &str) -> Vec<OwnerTls> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    let mut search = 0usize;
    while let Some(pos) = text[search..].find("thread_local!") {
        let at = search + pos;
        // The number of lines before `at` is this line's 0-based index.
        let line_index = text[..at].lines().count();
        let outer_attrs = attributes_before(&lines, line_index);
        let Some(open_rel) = text[at..].find('{') else {
            break;
        };
        let open = at + open_rel;
        let mut depth = 0i32;
        let mut end = open;
        for (offset, ch) in text[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + offset + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        let block = &text[open..end];
        for (name, type_text, test_only) in statics_in_block(block, &outer_attrs) {
            if test_only {
                continue;
            }
            if type_owns(&type_text) {
                found.push(OwnerTls {
                    name,
                    file: file.to_string(),
                });
            }
        }
        search = end;
    }
    found
}

/// Production sources only: under `crates/*/src`, excluding the test trees.
fn is_production_path(path: &Path) -> bool {
    let text = path.to_string_lossy().replace('\\', "/");
    if !text.contains("/src/") {
        return false;
    }
    !text.contains("/tests/") && !text.contains("/core_tests/") && !text.contains("/test_support")
}

fn visit(dir: &Path, found: &mut Vec<OwnerTls>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit(&path, found);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        if !is_production_path(&path) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        found.extend(scan_source(&text, &path.to_string_lossy()));
    }
}

#[test]
fn no_production_thread_local_owns_a_context_world_or_tagged_heap() {
    let crates = workspace_crates_dir();
    let mut found = Vec::new();
    visit(&crates, &mut found);
    assert!(
        found.is_empty(),
        "production thread_local! statics own a Context/World/TaggedHeap: {found:?}; \
         store raw pointers, or a test fixture in the typed ThreadExitLeak wrapper"
    );
}

#[test]
fn mutation_red_a_planted_owning_thread_local_is_reported() {
    let text = r#"
thread_local! {
    static CTX: std::cell::RefCell<Option<Box<Context>>> = const { std::cell::RefCell::new(None) };
}
"#;
    let found = scan_source(text, "synthetic/owning.rs");
    assert_eq!(found.len(), 1, "the guard must flag the planted owner");
    assert_eq!(found[0].name, "CTX");
}

#[test]
fn mutation_red_a_world_and_a_tagged_heap_are_reported() {
    let text = r#"
thread_local! {
    static WORLD: std::cell::RefCell<World> = const { std::cell::RefCell::new(World::new()) };
    static HEAP: std::cell::RefCell<Option<AliasableBox<TaggedHeap>>> = const { std::cell::RefCell::new(None) };
}
"#;
    let found = scan_source(text, "synthetic/owning2.rs");
    let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"WORLD"), "{names:?}");
    assert!(names.contains(&"HEAP"), "{names:?}");
}

#[test]
fn raw_pointer_thread_locals_are_allowed() {
    let text = r#"
thread_local! {
    static MODULE_CTX: std::cell::Cell<*mut Context> = const { std::cell::Cell::new(std::ptr::null_mut()) };
    static HEAP: std::cell::Cell<*const TaggedHeap> = const { std::cell::Cell::new(std::ptr::null()) };
}
"#;
    assert!(scan_source(text, "synthetic/raw.rs").is_empty());
}

#[test]
fn test_gated_owning_thread_locals_are_allowed() {
    let static_gate = r#"
thread_local! {
    #[cfg(any(test, feature = "test-thread-fallback-heap"))]
    static HEAP: std::cell::RefCell<Option<ThreadExitLeak<AliasableBox<TaggedHeap>>>> =
        const { std::cell::RefCell::new(None) };
}
"#;
    assert!(scan_source(static_gate, "synthetic/gated.rs").is_empty());
    let outer_gate = r#"
#[cfg(test)]
thread_local! {
    static CTX: std::cell::RefCell<Context> = const { std::cell::RefCell::new(Context::new()) };
}
"#;
    assert!(scan_source(outer_gate, "synthetic/gated2.rs").is_empty());
}
