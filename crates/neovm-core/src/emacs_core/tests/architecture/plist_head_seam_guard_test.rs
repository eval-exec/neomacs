//! Source guard for the P10 plist-head write seam.
//!
//! Rule: in production code the symbol plist head atomic is written in
//! exactly one place — `store_plist_head` in
//! `emacs_core/runtime/symbol/cell.rs`.  The runtime `put`/`setplist`
//! primitives, `replace_symbol_plist_id`, construction (`publish_fill`) and
//! pdump reconstruction (`from_dump`) all route through it, so there is no
//! exception list to maintain: construction and pdump bumps before an era is
//! published are harmless because nothing reads the clock yet.
//!
//! The scan runs over the whole crate source, strips comments and string
//! data, skips test-only files and `#[cfg(test)]` items, and requires every
//! remaining `plist.store_atomic(` occurrence to lie inside the seam's body.
//! The in-place writers (`setcar`/`setcdr`/`plist-put` on an existing key/
//! `nconc`) are deliberately not seams: the P10 live walk observes them.

use std::path::{Path, PathBuf};

const STORE: &str = "plist.store_atomic(";
const SEAM: &str = "fn store_plist_head";

fn source_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("read source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

/// Keep code tokens only: comments and string/char/raw-string literals are
/// dropped while every newline is preserved, so the scan matches declarations
/// rather than docs or test data quoting the forbidden shape, and reported
/// line numbers stay exact.  Rust block comments nest.
fn code_only(source: &str) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Normal,
        LineComment,
        BlockComment(usize),
        String,
        Char,
        RawString(usize),
    }
    fn hash_run(characters: &[char], from: usize) -> usize {
        characters[from..]
            .iter()
            .take_while(|character| **character == '#')
            .count()
    }
    fn raw_string_start(characters: &[char], at: usize) -> Option<(usize, usize)> {
        let mut index = at;
        if matches!(characters.get(index), Some('b' | 'c')) {
            index += 1;
        }
        if characters.get(index) != Some(&'r') {
            return None;
        }
        index += 1;
        let hashes = hash_run(characters, index);
        index += hashes;
        if characters.get(index) != Some(&'"') {
            return None;
        }
        Some((index + 1 - at, hashes))
    }
    fn is_lifetime(characters: &[char], at: usize) -> bool {
        let Some(next) = characters.get(at + 1) else {
            return false;
        };
        if !(next.is_ascii_alphabetic() || *next == '_') {
            return false;
        }
        let mut index = at + 1;
        while characters
            .get(index)
            .is_some_and(|character| character.is_alphanumeric() || *character == '_')
        {
            index += 1;
        }
        characters.get(index) != Some(&'\'')
    }
    let characters = source.chars().collect::<Vec<_>>();
    let mut out = String::with_capacity(source.len());
    let mut state = State::Normal;
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        let next = characters.get(index + 1).copied();
        match state {
            State::Normal => {
                if character == '/' && next == Some('/') {
                    state = State::LineComment;
                    index += 2;
                    continue;
                }
                if character == '/' && next == Some('*') {
                    state = State::BlockComment(1);
                    index += 2;
                    continue;
                }
                if let Some((consumed, hashes)) = raw_string_start(&characters, index) {
                    state = State::RawString(hashes);
                    index += consumed;
                    continue;
                }
                if character == '"' {
                    state = State::String;
                    index += 1;
                    continue;
                }
                if character == '\'' {
                    if is_lifetime(&characters, index) {
                        out.push(character);
                        index += 1;
                        continue;
                    }
                    state = State::Char;
                    index += 1;
                    continue;
                }
                out.push(character);
                index += 1;
            }
            State::LineComment => {
                if character == '\n' {
                    out.push('\n');
                    state = State::Normal;
                }
                index += 1;
            }
            State::BlockComment(depth) => {
                if character == '/' && next == Some('*') {
                    state = State::BlockComment(depth + 1);
                    index += 2;
                    continue;
                }
                if character == '*' && next == Some('/') {
                    state = if depth == 1 {
                        State::Normal
                    } else {
                        State::BlockComment(depth - 1)
                    };
                    index += 2;
                    continue;
                }
                if character == '\n' {
                    out.push('\n');
                }
                index += 1;
            }
            State::String | State::Char => {
                if character == '\\' {
                    index += 2;
                    continue;
                }
                if (state == State::String && character == '"')
                    || (state == State::Char && character == '\'')
                {
                    state = State::Normal;
                } else if character == '\n' {
                    out.push('\n');
                }
                index += 1;
            }
            State::RawString(hashes) => {
                if character == '"' && hash_run(&characters, index + 1) >= hashes {
                    state = State::Normal;
                    index += 1 + hashes;
                    continue;
                }
                if character == '\n' {
                    out.push('\n');
                }
                index += 1;
            }
        }
    }
    out
}

/// The index of the `close` byte matching the `open` byte at `open_index`.
fn matching(source: &str, open_index: usize, open: u8, close: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut index = open_index;
    while index < bytes.len() {
        if bytes[index] == open {
            depth += 1;
        } else if bytes[index] == close {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}

/// `test` as a cfg predicate word (string contents are already stripped, so a
/// feature named `...-test-...` cannot match).
fn contains_test_token(attribute: &str) -> bool {
    attribute
        .split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .any(|word| word == "test")
}

/// Byte ranges of items carrying a test-gated `#[cfg(... test ...)]`.
fn test_gated_ranges(code: &str) -> Vec<(usize, usize)> {
    let bytes = code.as_bytes();
    let mut ranges = Vec::new();
    let mut from = 0usize;
    while let Some(offset) = code[from..].find("#[cfg(") {
        let start = from + offset;
        let Some(attribute_end) = matching(code, start + 1, b'[', b']') else {
            break;
        };
        if !contains_test_token(&code[start..=attribute_end]) {
            from = attribute_end + 1;
            continue;
        }
        let mut at = attribute_end + 1;
        loop {
            while at < bytes.len() && bytes[at].is_ascii_whitespace() {
                at += 1;
            }
            if bytes[at..].starts_with(b"#[") {
                let Some(end) = matching(code, at + 1, b'[', b']') else {
                    break;
                };
                at = end + 1;
                continue;
            }
            break;
        }
        let mut end = None;
        let mut index = at;
        while index < bytes.len() {
            match bytes[index] {
                b'{' => {
                    end = matching(code, index, b'{', b'}');
                    break;
                }
                b';' => {
                    end = Some(index);
                    break;
                }
                _ => index += 1,
            }
        }
        match end {
            Some(end) => {
                ranges.push((start, end));
                from = end + 1;
            }
            None => break,
        }
    }
    ranges
}

fn find_all(code: &str, needle: &str) -> Vec<usize> {
    let mut found = Vec::new();
    let mut from = 0usize;
    while let Some(offset) = code[from..].find(needle) {
        let at = from + offset;
        found.push(at);
        from = at + needle.len();
    }
    found
}

/// The seam function's opening and closing brace offsets.
fn seam_body(code: &str) -> Option<(usize, usize)> {
    let start = code.find(SEAM)?;
    let open = code[start..].find('{')? + start;
    let close = matching(code, open, b'{', b'}')?;
    Some((open, close))
}

/// `plist.store_atomic(` occurrences outside test-gated items and outside the
/// seam body.  An empty result means the rule holds for this source text.
fn violations_in(code: &str) -> Vec<usize> {
    let ranges = test_gated_ranges(code);
    let seam = seam_body(code);
    find_all(code, STORE)
        .into_iter()
        .filter(|offset| {
            !ranges
                .iter()
                .any(|(start, end)| *offset >= *start && *offset <= *end)
        })
        .filter(|offset| !seam.is_some_and(|(open, close)| *offset > open && *offset < close))
        .collect()
}

#[test]
fn plist_head_stores_have_exactly_one_production_site() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("src");
    let mut files = Vec::new();
    source_files(&root, &mut files);
    assert!(
        files.len() > 100,
        "the guard must scan the whole crate source tree, saw {} files",
        files.len()
    );
    let mut production_sites = Vec::new();
    let mut seam_seen = false;
    for path in &files {
        let relative = path.strip_prefix(&manifest).unwrap_or(path);
        if relative
            .components()
            .any(|component| component.as_os_str() == "tests")
        {
            continue;
        }
        let source = std::fs::read_to_string(path).expect("read source file");
        let code = code_only(&source);
        if code.contains(SEAM) {
            seam_seen = true;
        }
        let ranges = test_gated_ranges(&code);
        let seam = seam_body(&code);
        for offset in find_all(&code, STORE) {
            if ranges
                .iter()
                .any(|(start, end)| offset >= *start && offset <= *end)
            {
                continue;
            }
            let line = code[..offset].matches('\n').count() + 1;
            production_sites.push(format!("{}:{line}", relative.display()));
            assert!(
                seam.is_some_and(|(open, close)| offset > open && offset < close),
                "a plist head store outside `store_plist_head`: {}:{line}. \
                 Route it through the seam (construction and pdump stores too); \
                 in-place setcar/setcdr/nconc/plist-put writers are not seams.",
                relative.display()
            );
        }
    }
    assert!(
        seam_seen,
        "the production source must define `fn store_plist_head`"
    );
    assert_eq!(
        production_sites.len(),
        1,
        "expected exactly the seam's own store, saw {production_sites:?}"
    );
}

#[test]
fn plist_head_seam_guard_controls_detect_synthetic_bypasses() {
    // The seam itself is the one allowed production site.
    let seam = "pub(crate) fn store_plist_head(sym: &mut LispSymbol, plist: Value) { \
                symbol.plist.store_atomic(plist); }";
    assert!(
        violations_in(seam).is_empty(),
        "the seam fixture must be allowed"
    );

    // Any head store outside it is flagged.
    for fixture in [
        "fn bypass(sym: &mut LispSymbol) { sym.plist.store_atomic(Value::NIL); }",
        "impl Obarray { fn bypass(&mut self) { self.sym().plist.store_atomic(v); } }",
        "fn store_plist_head(sym: &mut LispSymbol) { sym.plist.store_atomic(v); }\n\
         fn bypass(sym: &mut LispSymbol) { sym.plist.store_atomic(v); }",
    ] {
        assert_eq!(
            violations_in(fixture).len(),
            1,
            "control must flag: {fixture}"
        );
    }

    // Test-gated items and stripped comments/strings are not production.
    for fixture in [
        "#[cfg(test)]\nimpl Obarray { fn f(sym: &mut LispSymbol) { sym.plist.store_atomic(v); } }",
        "#[cfg(all(\n    test,\n    feature = \"jit\"\n))]\nmod t { fn f() { s.plist.store_atomic(v); } }",
        "// sym.plist.store_atomic(v)\nconst S: &str = \"sym.plist.store_atomic(v)\";",
    ] {
        assert!(
            violations_in(fixture).is_empty(),
            "control must not flag: {fixture}"
        );
    }
}
