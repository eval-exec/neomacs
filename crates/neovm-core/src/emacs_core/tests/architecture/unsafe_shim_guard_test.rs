//! Source guard for the JIT C-ABI shims.
//!
//! Two rules, both source scans over the whole crate so `macro_rules!` bodies
//! (the leaf trampolines are generated there) are covered:
//!
//! (a) No `#[allow(clippy::not_unsafe_ptr_arg_deref)]`, in any combined or
//!     multi-line attribute.
//! (b) No safe `extern "C"` fn — `pub`, `pub(crate)` or private — whose
//!     signature takes a raw pointer (`*mut`/`*const`). The match is purely
//!     signature-based and ignores whether the body dereferences: a safe
//!     caller may always pass garbage, and forwarding the pointer to an
//!     `unsafe` callee inside an unsafe block is exactly the unjustified
//!     contract that makes the shim unsound. There are no exceptions; the
//!     only correct form is `unsafe extern "C" fn` with a `/// # Safety`
//!     section. The ABI, the `#[unsafe(no_mangle)]` symbol and the
//!     address-based JIT symbol table are unchanged by that marker.

use std::path::{Path, PathBuf};

const LINT: &str = "not_unsafe_ptr_arg_deref";

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
/// dropped while every newline is preserved, so the scans match declarations
/// (never docs or test data quoting the forbidden shapes, including this
/// guard's own fixtures) and reported line numbers stay exact. Rust block
/// comments nest.
///
/// One literal is syntax rather than data: the ABI string directly after
/// `extern` (always `"C"`), so it is preserved for the declaration scan.
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
    /// `(characters consumed through the opening quote, hash count)` when a
    /// raw string (`r"..."`, `r#"..."#`, `br#"..."#`) starts at `at`.
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
    /// `'static`/`'a` are lifetimes; `'a'`/`'\n'` are char literals.
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
                    if out.trim_end().ends_with("extern") {
                        out.push(character);
                        index += 1;
                        while index < characters.len() {
                            let literal = characters[index];
                            out.push(literal);
                            index += 1;
                            if literal == '\\' {
                                if let Some(escaped) = characters.get(index) {
                                    out.push(*escaped);
                                    index += 1;
                                }
                                continue;
                            }
                            if literal == '"' {
                                break;
                            }
                        }
                        continue;
                    }
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

/// Find every `allow(...)` attribute whose balanced token list contains
/// `LINT`, in whitespace-stripped source, so attributes spread over several
/// lines and combined lint lists are both caught.
fn allow_attributes_containing_lint(stripped: &str) -> usize {
    let needle = "allow(";
    let mut found = 0;
    let mut from = 0;
    while let Some(offset) = stripped[from..].find(needle) {
        let start = from + offset + needle.len() - 1;
        let mut depth = 0usize;
        let mut close = None;
        for (index, character) in stripped[start..].char_indices() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(start + index);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(close) = close
            && stripped[start..=close].contains(LINT)
        {
            found += 1;
        }
        from = from + offset + needle.len();
    }
    found
}

/// The parameter list starting at the first `(` at or after `from`, with
/// balanced parentheses.
fn parameter_list(source: &str, from: usize) -> &str {
    let Some(open) = source[from..].find('(').map(|offset| from + offset) else {
        return "";
    };
    let mut depth = 0usize;
    for (index, character) in source[open..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &source[open..=open + index];
                }
            }
            _ => {}
        }
    }
    ""
}

/// Safe `extern "C"` declarations whose signature takes a raw pointer.
/// Returns `(name, line)` in source order.
fn safe_pointer_extern_fns(source: &str) -> Vec<(String, usize)> {
    let mut offenders = Vec::new();
    let mut from = 0;
    while let Some(offset) = source[from..].find("extern") {
        let at = from + offset;
        let is_extern_c = source[at..]
            .strip_prefix("extern")
            .is_some_and(|rest| rest.trim_start().starts_with("\"C\""));
        if !is_extern_c {
            from = at + "extern".len();
            continue;
        }
        // `unsafe` is the last token of the declaration prefix only for
        // `[pub[(crate)] ]unsafe extern "C"`; a preceding attribute's own
        // `unsafe` token does not sit at the end of the prefix.
        let before = source[..at].trim_end();
        let unsafe_declaration = before.ends_with("unsafe") && {
            let rest = &before[..before.len() - "unsafe".len()];
            rest.chars()
                .next_back()
                .is_none_or(|character| !(character.is_alphanumeric() || character == '_'))
        };
        if unsafe_declaration {
            from = at + "extern".len();
            continue;
        }
        let after_abi = source[at..]
            .strip_prefix("extern")
            .and_then(|rest| rest.trim_start().strip_prefix("\"C\""))
            .map(str::trim_start);
        let Some(after_abi) = after_abi else {
            from = at + "extern".len();
            continue;
        };
        // A declaration has whitespace between `fn` and its name; `fn(` is a
        // function-pointer *type* annotation, not a shim declaration.
        let Some(after_fn) = after_abi.strip_prefix("fn") else {
            from = at + "extern".len();
            continue;
        };
        if !after_fn.starts_with(char::is_whitespace) {
            from = at + "extern".len();
            continue;
        }
        let name_region = after_fn.trim_start();
        let name_start = source.len() - name_region.len();
        let name = source[name_start..]
            .chars()
            .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | '$'))
            .collect::<String>();
        let params = parameter_list(source, name_start + name.len());
        if params.contains("*mut ") || params.contains("*const ") {
            let line = source[..at].matches('\n').count() + 1;
            offenders.push((name, line));
        }
        from = at + "extern".len();
    }
    offenders
}

#[test]
fn unsafe_shims_carry_no_not_unsafe_ptr_arg_deref_allow() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("src");
    let mut files = Vec::new();
    source_files(&root, &mut files);
    assert!(
        files.len() > 100,
        "the guard must scan the whole crate source tree, saw {} files",
        files.len()
    );
    let mut offenders = Vec::new();
    for path in &files {
        let source = code_only(&std::fs::read_to_string(path).expect("read source file"));
        let relative = path.strip_prefix(&manifest).unwrap_or(path);
        for (line, content) in source.lines().enumerate() {
            if content.contains("#[allow(") && content.contains(LINT) {
                offenders.push(format!(
                    "{}:{}: {}",
                    relative.display(),
                    line + 1,
                    content.trim()
                ));
            }
        }
        let stripped = source
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        let found = allow_attributes_containing_lint(&stripped);
        if found > 0
            && !offenders
                .iter()
                .any(|entry| entry.starts_with(&format!("{}:", relative.display())))
        {
            offenders.push(format!(
                "{}: {found} multi-line or combined allow attribute(s)",
                relative.display()
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "a safe function that dereferences a raw pointer argument is unsound by \
         construction; make the shim `unsafe extern \"C\" fn` with a `/// # Safety` \
         contract instead of allowing the pointer-deref lint:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn unsafe_shims_are_the_only_pointer_taking_extern_c_declarations() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("src");
    let mut files = Vec::new();
    source_files(&root, &mut files);
    assert!(
        files.len() > 100,
        "the guard must scan the whole crate source tree, saw {} files",
        files.len()
    );
    let mut offenders = Vec::new();
    let mut scanned = 0usize;
    for path in &files {
        let source = code_only(&std::fs::read_to_string(path).expect("read source file"));
        let relative = path.strip_prefix(&manifest).unwrap_or(path);
        scanned += source.matches("extern \"C\"").count();
        for (name, line) in safe_pointer_extern_fns(&source) {
            offenders.push(format!("{}:{line}: {name}", relative.display()));
        }
    }
    assert!(
        scanned > 50,
        "the declaration scan must actually see the crate's extern \"C\" items, saw {scanned}"
    );
    assert!(
        offenders.is_empty(),
        "a safe `extern \"C\"` fn taking a raw pointer is callable from safe Rust \
         with any address and cannot uphold its callee's contract; declare it \
         `unsafe extern \"C\" fn` with a `/// # Safety` section:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn unsafe_shim_guard_controls_detect_synthetic_offenders() {
    // Rule (a): single-line, combined and multi-line allow attributes all match.
    assert_eq!(
        allow_attributes_containing_lint("#[allow(clippy::not_unsafe_ptr_arg_deref)]fnf(){}"),
        1
    );
    assert_eq!(
        allow_attributes_containing_lint(
            "#[allow(clippy::too_many_arguments,clippy::not_unsafe_ptr_arg_deref)]fnf(){}"
        ),
        1
    );
    assert_eq!(
        allow_attributes_containing_lint("#[allow(\n clippy::not_unsafe_ptr_arg_deref\n)]"),
        1
    );
    assert_eq!(
        allow_attributes_containing_lint("#[allow(clippy::too_many_arguments)]fnf(){}"),
        0
    );

    // The code-only view keeps declarations and drops comments and string
    // data, so this guard's own fixtures never make it flag itself.
    let code = code_only(
        "// pub extern \"C\" fn doc(ctx: *mut u8) {}\n\
         let _ = \"pub extern \\\"C\\\" fn data(ctx: *mut u8) {}\";\n\
         pub extern \"C\" fn real(ctx: *mut u8) {}",
    );
    assert_eq!(safe_pointer_extern_fns(&code).len(), 1, "{code}");

    // Rule (b): every safe visibility is caught; `unsafe` and pointer-free
    // declarations are not.
    for fixture in [
        "pub extern \"C\" fn sneaky(ctx: *mut u8) -> i64 { 0 }",
        "pub(crate) extern \"C\" fn sneaky(ctx: *const u8) -> i64 { 0 }",
        "extern \"C\" fn sneaky(ctx: *mut u8) -> i64 { 0 }",
        "pub extern \"C\" fn sneaky(\n    ctx: *mut u8,\n    out: *mut i64,\n) -> i64 { 0 }",
        "macro_rules! m { () => { pub extern \"C\" fn $name(ctx: *const Context) {} } }",
    ] {
        assert!(
            !safe_pointer_extern_fns(fixture).is_empty(),
            "control must flag: {fixture}"
        );
    }
    for fixture in [
        "pub unsafe extern \"C\" fn ok(ctx: *mut u8) -> i64 { 0 }",
        "pub(crate) unsafe extern \"C\" fn ok(ctx: *const u8) -> i64 { 0 }",
        "unsafe extern \"C\" fn ok(ctx: *mut u8) -> i64 { 0 }",
        "pub extern \"C\" fn ok(v: i64) -> i64 { v }",
        "#[unsafe(no_mangle)] pub unsafe extern \"C\" fn ok(ctx: *mut u8) -> i64 { 0 }",
    ] {
        assert!(
            safe_pointer_extern_fns(fixture).is_empty(),
            "control must not flag: {fixture}"
        );
    }
}
