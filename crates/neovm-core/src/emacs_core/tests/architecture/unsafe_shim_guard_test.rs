//! Source guard for the JIT C-ABI shims and safe-fn raw-pointer/word contracts (G4).
//!
//! Five rules, covering both macro-generated `extern "C"` shims and every
//! production function across the workspace:
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
//! (c) No safe `extern "C"` fn whose body rebuilds a heap value from one of
//!     its parameters with `from_bits`, `Value::from_bits`, or
//!     `TaggedValue::from_bits`, including through a local copied from that
//!     parameter. A word is the same unsound contract as a raw pointer: safe
//!     code can pass garbage bits. A no-argument shim is not this shape.
//! (d) Class guard (G4): across every workspace production crate, no safe `fn`
//!     taking a raw pointer parameter (`*const`/`*mut`, `NonNull<T>`, or
//!     `emacs_value`) — or casting an integer word parameter (`usize`/`u64`/
//!     `i64`) to a pointer — may dereference that pointer (or a value derived
//!     from it via locals, assignments, control flow, patterns, struct fields,
//!     calls, or macro invocations) or pass it to an unsafe operation/call
//!     inside an `unsafe` block. `macro_rules!` bodies are scanned as items
//!     too. Pure address/identity checks (`==`, `!=`, `is_null`, `cast`,
//!     `addr`, `ptr::eq`) remain allowed.
//! (e) Class guard (G4): across every workspace production crate, no safe `fn`
//!     may pass an integer word parameter (`usize`/`u64`/`i64`, or a value
//!     derived from it) directly to `from_bits` / `Value::from_bits` /
//!     `TaggedValue::from_bits`, or cast it to a raw pointer for an unsafe op.
//! (f) Class guard (G4, sys B3/B4): across every workspace production crate,
//!     no safe `fn` (outside foreign `extern { ... }` import blocks) may take
//!     a parameter whose type names a raw C FFI handle (`TSNode`, `TSTree`,
//!     `TSQuery`, `TSQueryCursor`, `sqlite3_stmt`, `sqlite3`, `sqlite3_value`),
//!     whether by value, reference, raw pointer, `Option`, or `NonNull`.
//!     Either accept a validated/typed owner or declare `unsafe fn` with a
//!     `/// # Safety` contract. Limitation: this matches by handle type name
//!     in parameter types only; it does not inspect structs embedding a handle
//!     field or lifetime-erasing storage of a borrowed wrapper (`into_raw`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use proc_macro2::{Delimiter, Group, Ident, Spacing, Span, TokenStream, TokenTree};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use super::architecture_production_source_roots::{crates_dir, production_source_files};

const LINT: &str = "not_unsafe_ptr_arg_deref";

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

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn ident_at(text: &str, at: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    if at > 0 && is_ident_byte(bytes[at - 1]) {
        return None;
    }
    let mut end = at;
    while end < bytes.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }
    if end == at {
        None
    } else {
        Some(&text[at..end])
    }
}

fn contains_ident(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    let needle = name.as_bytes();
    if needle.is_empty() || needle.len() > bytes.len() {
        return false;
    }
    let mut index = 0;
    while index + needle.len() <= bytes.len() {
        if &bytes[index..index + needle.len()] == needle {
            let before = index == 0 || !is_ident_byte(bytes[index - 1]);
            let after = index + needle.len();
            let after = after == bytes.len() || !is_ident_byte(bytes[after]);
            if before && after {
                return true;
            }
        }
        index += 1;
    }
    false
}

/// The binding name of one parameter, ignoring `self`.
fn binding_name(param: &str) -> Option<String> {
    let mut rest = param.trim();
    if rest.is_empty() {
        return None;
    }
    if let Some(stripped) = rest.strip_prefix('&') {
        rest = stripped.trim_start();
    }
    if let Some(stripped) = rest.strip_prefix("mut") {
        if stripped
            .chars()
            .next()
            .is_none_or(|character| !(character.is_alphanumeric() || character == '_'))
        {
            rest = stripped.trim_start();
        }
    }
    if rest == "self" || rest.starts_with("self:") || rest.starts_with("self ") {
        return None;
    }
    let name: String = rest
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect();
    if name.is_empty() || name == "self" {
        None
    } else {
        Some(name)
    }
}

fn parameter_names(params: &str) -> Vec<String> {
    let inner = params
        .trim()
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or("");
    let mut names = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (index, character) in inner.char_indices() {
        match character {
            '(' | '<' | '[' => depth += 1,
            ')' | '>' | ']' => depth -= 1,
            ',' if depth == 0 => {
                if let Some(name) = binding_name(&inner[start..index]) {
                    names.push(name);
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    if let Some(name) = binding_name(&inner[start..]) {
        names.push(name);
    }
    names
}

fn function_body(source: &str, after_params: usize) -> &str {
    let Some(relative) = source[after_params..].find('{') else {
        return "";
    };
    let open = after_params + relative;
    let mut depth = 0i32;
    for (index, character) in source[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[open + 1..open + index];
                }
            }
            _ => {}
        }
    }
    ""
}

/// `let` bindings in source order: `(name, initializer, end offset)`.
fn let_bindings(body: &str) -> Vec<(String, String, usize)> {
    let mut bindings = Vec::new();
    let mut index = 0usize;
    while index < body.len() {
        let Some(name_at) = ident_at(body, index) else {
            index += 1;
            continue;
        };
        if name_at != "let" {
            index += name_at.len();
            continue;
        }
        let mut cursor = index + name_at.len();
        while body
            .as_bytes()
            .get(cursor)
            .is_some_and(u8::is_ascii_whitespace)
        {
            cursor += 1;
        }
        if ident_at(body, cursor) == Some("mut") {
            cursor += "mut".len();
            while body
                .as_bytes()
                .get(cursor)
                .is_some_and(u8::is_ascii_whitespace)
            {
                cursor += 1;
            }
        }
        let Some(bound) = ident_at(body, cursor) else {
            index += name_at.len();
            continue;
        };
        cursor += bound.len();
        while body
            .as_bytes()
            .get(cursor)
            .is_some_and(u8::is_ascii_whitespace)
        {
            cursor += 1;
        }
        if body.as_bytes().get(cursor) != Some(&b'=') {
            index += name_at.len();
            continue;
        }
        cursor += 1;
        let expr_start = cursor;
        let mut depth = 0i32;
        while cursor < body.len() {
            match body.as_bytes()[cursor] {
                b'(' | b'{' | b'[' => depth += 1,
                b')' | b'}' | b']' if depth > 0 => depth -= 1,
                b';' if depth == 0 => break,
                _ => {}
            }
            cursor += 1;
        }
        let expr = body[expr_start..cursor].to_string();
        bindings.push((bound.to_string(), expr, cursor));
        index = cursor;
    }
    bindings
}

fn tainted_at(params: &[String], lets: &[(String, String, usize)], at: usize) -> Vec<String> {
    let mut tainted = params.to_vec();
    for (name, expr, end) in lets {
        if *end > at {
            break;
        }
        if tainted.iter().any(|source| contains_ident(expr, source)) {
            tainted.push(name.clone());
        }
    }
    tainted
}

/// Safe `extern "C"` fns that rebuild a value from a parameter word.
/// Returns `(name, line)` in source order.
fn safe_word_extern_fns(source: &str) -> Vec<(String, usize)> {
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
        if params.is_empty() {
            from = at + "extern".len();
            continue;
        }
        let names = parameter_names(params);
        let body_at = name_start + name.len() + params.len();
        let body = function_body(source, body_at);
        let lets = let_bindings(body);
        let mut index = 0usize;
        while index < body.len() {
            let Some(token) = ident_at(body, index) else {
                index += 1;
                continue;
            };
            if token == "from_bits" {
                let after = index + token.len();
                let args = parameter_list(body, after);
                if !args.is_empty() {
                    let tainted = tainted_at(&names, &lets, index);
                    if tainted.iter().any(|name| contains_ident(args, name)) {
                        let line = source[..at].matches('\n').count() + 1;
                        offenders.push((name.clone(), line));
                        break;
                    }
                }
            }
            index += token.len();
        }
        from = at + "extern".len();
    }
    offenders
}

#[test]
fn unsafe_shims_carry_no_not_unsafe_ptr_arg_deref_allow() {
    let files = production_source_files();
    assert!(
        files.len() > 100,
        "the guard must scan every production source tree, saw {} files",
        files.len()
    );
    let mut offenders = Vec::new();
    for file in &files {
        let source = code_only(&std::fs::read_to_string(&file.path).expect("read source file"));
        let relative = file.workspace_relative();
        for (line, content) in source.lines().enumerate() {
            if content.contains("#[allow(") && content.contains(LINT) {
                offenders.push(format!("{relative}:{}: {}", line + 1, content.trim()));
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
                .any(|entry| entry.starts_with(&format!("{relative}:")))
        {
            offenders.push(format!(
                "{relative}: {found} multi-line or combined allow attribute(s)"
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
    let files = production_source_files();
    assert!(
        files.len() > 100,
        "the guard must scan every production source tree, saw {} files",
        files.len()
    );
    let mut offenders = Vec::new();
    let mut scanned = 0usize;
    for file in &files {
        let source = code_only(&std::fs::read_to_string(&file.path).expect("read source file"));
        let relative = file.workspace_relative();
        scanned += source.matches("extern \"C\"").count();
        for (name, line) in safe_pointer_extern_fns(&source) {
            offenders.push(format!("{relative}:{line}: {name}"));
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

    // Rule (c): a parameter word rebuilt with from_bits is the same hole,
    // including a local copied from that parameter. A no-argument shim, an
    // already-unsafe shim, and from_bits of a non-parameter are not.
    for fixture in [
        "pub extern \"C\" fn sneaky(v: i64) -> i64 { unsafe { Value::from_bits(v as usize).bits() as i64 } }",
        "pub(crate) extern \"C\" fn sneaky(bits: usize) { let _ = unsafe { TaggedValue::from_bits(bits) }; }",
        "extern \"C\" fn sneaky(owner_bits: i64) -> i8 { let bits = owner_bits as usize; let _ = unsafe { from_bits(bits) }; 0 }",
        "pub extern \"C\" fn sneaky(tag: i64, value: i64) { let _ = unsafe { Value::from_bits(value as usize) }; }",
    ] {
        assert!(
            !safe_word_extern_fns(fixture).is_empty(),
            "control must flag: {fixture}"
        );
    }
    for fixture in [
        "pub extern \"C\" fn neovm_jit_switch_stale() {}",
        "pub unsafe extern \"C\" fn ok(v: i64) { let _ = Value::from_bits(v as usize); }",
        "pub extern \"C\" fn ok(v: i64) -> i64 { v }",
        "pub extern \"C\" fn ok(v: i64) -> i64 { let _ = Value::from_bits(0); v }",
        "pub extern \"C\" fn ok(saved: i64) { restore(saved as usize); }",
    ] {
        assert!(
            safe_word_extern_fns(fixture).is_empty(),
            "control must not flag: {fixture}"
        );
    }
}

#[test]
fn unsafe_shims_do_not_rebuild_values_from_safe_words() {
    let files = production_source_files();
    assert!(
        files.len() > 100,
        "the guard must scan every production source tree, saw {} files",
        files.len()
    );
    let mut offenders = Vec::new();
    for file in &files {
        let source = code_only(&std::fs::read_to_string(&file.path).expect("read source file"));
        let relative = file.workspace_relative();
        for (name, line) in safe_word_extern_fns(&source) {
            offenders.push(format!("{relative}:{line}: {name}"));
        }
    }
    assert!(
        offenders.is_empty(),
        "a safe `extern \"C\"` fn that rebuilds a Value from a parameter word \
         is callable from safe Rust with garbage bits; declare it \
         `unsafe extern \"C\" fn` with a `/// # Safety` section:\n{}",
        offenders.join("\n")
    );
}

/// `neovm_jit_gc_restore` truncates the scratch-root stack to a caller-supplied
/// depth. Safe Rust can pass a smaller depth while an outer frame still needs
/// those roots; a later collection then frees values generated code holds.
#[test]
fn gc_restore_shim_is_an_unsafe_extern() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../neovm-eval/src/emacs_core/runtime/jit/compile/shims.rs");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    assert!(
        src.contains("unsafe extern \"C\" fn neovm_jit_gc_restore"),
        "neovm_jit_gc_restore must be `unsafe extern \"C\"`: it drops scratch roots from an untrusted depth"
    );
    let start = src
        .find("fn neovm_jit_gc_restore")
        .expect("gc_restore declaration");
    let doc = src[..start]
        .lines()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        doc.contains("# Safety"),
        "neovm_jit_gc_restore needs a Safety contract for the saved depth, immediately above the function; saw:\n{doc}"
    );
}

// ---------------------------------------------------------------------------
// G4 class guard (rules d & e): workspace-wide syn scan over production fns
// ---------------------------------------------------------------------------

const EXCLUDED_WORKSPACE_CRATES: &[&str] = &[
    "neomacs-bench",
    "neomacs-melpa-tests",
    "neomacs-xtask",
    "neovm-oracle-tests",
    "xtask",
];

const SAFE_PTR_METHODS: &[&str] = &[
    "addr",
    "align_offset",
    "as_ptr",
    "cast",
    "cast_const",
    "cast_mut",
    "cmp",
    "eq",
    "expose_provenance",
    "is_aligned",
    "is_aligned_to",
    "is_null",
    "ne",
    "with_addr",
];

const NON_PTR_RETURN_METHODS: &[&str] = &[
    "addr",
    "align_offset",
    "cmp",
    "eq",
    "expose_provenance",
    "is_aligned",
    "is_aligned_to",
    "is_empty",
    "is_err",
    "is_none",
    "is_null",
    "is_ok",
    "is_some",
    "len",
    "ne",
];

fn workspace_production_source_files() -> Vec<(String, PathBuf)> {
    fn walk(crate_name: &str, src_root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut sorted: Vec<_> = entries.filter_map(Result::ok).collect();
        sorted.sort_by_key(|entry| entry.path());
        for entry in sorted {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if matches!(name, "tests" | "test_support" | "restricted_lisp") {
                    continue;
                }
                walk(crate_name, src_root, &path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.starts_with("test_")
                    || name.ends_with("_test.rs")
                    || name.ends_with("_tests.rs")
                    || name == "tests.rs"
                    || name == "test_support.rs"
                    || name == "test_utils.rs"
                    || name == "restricted_lisp.rs"
                {
                    continue;
                }
                let rel = path
                    .strip_prefix(src_root)
                    .expect("file under crate src")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((format!("{crate_name}/src/{rel}"), path));
            }
        }
    }

    let root = crates_dir();
    let mut crate_dirs: Vec<_> = std::fs::read_dir(&root)
        .expect("read workspace crates/")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .collect();
    crate_dirs.sort_by_key(|entry| entry.path());

    let mut files = Vec::new();
    for entry in crate_dirs {
        let crate_name = entry
            .file_name()
            .into_string()
            .expect("valid utf-8 crate dir");
        if EXCLUDED_WORKSPACE_CRATES.contains(&crate_name.as_str()) {
            continue;
        }
        let src = entry.path().join("src");
        if src.is_dir() {
            walk(&crate_name, &src, &src, &mut files);
        }
    }
    files
}

fn meta_is_cfg_test(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::List(list) if list.path.is_ident("all") => list
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .is_ok_and(|nested| nested.iter().any(meta_is_cfg_test)),
        syn::Meta::List(list) if list.path.is_ident("any") => list
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .is_ok_and(|nested| !nested.is_empty() && nested.iter().all(meta_is_cfg_test)),
        _ => false,
    }
}

fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("cfg") {
            return false;
        }
        attr.parse_args::<syn::Meta>()
            .is_ok_and(|meta| meta_is_cfg_test(&meta))
    })
}

const RAW_FFI_HANDLE_TYPES: &[&str] = &[
    "TSNode",
    "TSTree",
    "TSQuery",
    "TSQueryCursor",
    "sqlite3_stmt",
    "sqlite3",
    "sqlite3_value",
];

fn is_raw_ffi_handle_type(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Group(group) => is_raw_ffi_handle_type(&group.elem),
        syn::Type::Paren(paren) => is_raw_ffi_handle_type(&paren.elem),
        syn::Type::Reference(reference) => is_raw_ffi_handle_type(&reference.elem),
        syn::Type::Ptr(ptr) => is_raw_ffi_handle_type(&ptr.elem),
        syn::Type::Path(path) => path.path.segments.last().is_some_and(|seg| {
            RAW_FFI_HANDLE_TYPES
                .iter()
                .any(|handle| seg.ident == *handle)
                || match &seg.arguments {
                    syn::PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| {
                        matches!(
                            arg,
                            syn::GenericArgument::Type(inner) if is_raw_ffi_handle_type(inner)
                        )
                    }),
                    _ => false,
                }
        }),
        _ => false,
    }
}

fn is_raw_ptr_type(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Ptr(_) => true,
        syn::Type::Group(group) => is_raw_ptr_type(&group.elem),
        syn::Type::Paren(paren) => is_raw_ptr_type(&paren.elem),
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|seg| seg.ident == "emacs_value" || seg.ident == "NonNull"),
        _ => false,
    }
}

fn is_word_int_type(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Group(group) => is_word_int_type(&group.elem),
        syn::Type::Paren(paren) => is_word_int_type(&paren.elem),
        syn::Type::Path(path) => {
            path.path.segments.last().is_some_and(|seg| {
                matches!(seg.ident.to_string().as_str(), "usize" | "u64" | "i64")
            })
        }
        _ => false,
    }
}

fn collect_pat_idents(pat: &syn::Pat, out: &mut Vec<String>) {
    match pat {
        syn::Pat::Ident(ident) => {
            let name = ident.ident.to_string();
            if name != "self" && !name.starts_with('_') {
                out.push(name);
            }
            if let Some((_, sub)) = &ident.subpat {
                collect_pat_idents(sub, out);
            }
        }
        syn::Pat::Tuple(tuple) => {
            for elem in &tuple.elems {
                collect_pat_idents(elem, out);
            }
        }
        syn::Pat::TupleStruct(ts) => {
            for elem in &ts.elems {
                collect_pat_idents(elem, out);
            }
        }
        syn::Pat::Struct(st) => {
            for field in &st.fields {
                collect_pat_idents(&field.pat, out);
            }
        }
        syn::Pat::Slice(slice) => {
            for elem in &slice.elems {
                collect_pat_idents(elem, out);
            }
        }
        syn::Pat::Or(or) => {
            for case in &or.cases {
                collect_pat_idents(case, out);
            }
        }
        syn::Pat::Reference(r) => collect_pat_idents(&r.pat, out),
        syn::Pat::Type(t) => collect_pat_idents(&t.pat, out),
        syn::Pat::Paren(p) => collect_pat_idents(&p.pat, out),
        _ => {}
    }
}

fn collect_assign_target_idents(expr: &syn::Expr, out: &mut Vec<String>) {
    match expr {
        syn::Expr::Path(path) => {
            if let Some(ident) = path.path.get_ident() {
                let name = ident.to_string();
                if name != "self" && !name.starts_with('_') {
                    out.push(name);
                }
            }
        }
        syn::Expr::Field(field) => collect_assign_target_idents(&field.base, out),
        syn::Expr::Paren(p) => collect_assign_target_idents(&p.expr, out),
        syn::Expr::Group(g) => collect_assign_target_idents(&g.expr, out),
        syn::Expr::Tuple(t) => {
            for elem in &t.elems {
                collect_assign_target_idents(elem, out);
            }
        }
        _ => {}
    }
}

fn call_last_two_segments(func: &syn::Expr) -> Option<(Option<String>, String)> {
    let syn::Expr::Path(path) = func else {
        return None;
    };
    let segs: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let last = segs.last()?.clone();
    let prev = if segs.len() >= 2 {
        Some(segs[segs.len() - 2].clone())
    } else {
        None
    };
    Some((prev, last))
}

fn is_word_to_ptr_constructor(func: &syn::Expr) -> bool {
    let Some((_, last)) = call_last_two_segments(func) else {
        return false;
    };
    matches!(
        last.as_str(),
        "with_exposed_provenance"
            | "with_exposed_provenance_mut"
            | "from_exposed_addr"
            | "from_exposed_addr_mut"
            | "without_provenance"
            | "without_provenance_mut"
    )
}

fn is_ptr_accepting_inferred_cast_call(func: &syn::Expr) -> bool {
    let Some((prev, last)) = call_last_two_segments(func) else {
        return false;
    };
    if prev.as_deref() == Some("NonNull") && matches!(last.as_str(), "new" | "new_unchecked") {
        return true;
    }
    last.starts_with("from_") && last.ends_with("_ptr")
}

/// Whether `expr` is derived from a word parameter (`usize`/`u64`/`i64`).
fn expr_is_word_derived(
    expr: &syn::Expr,
    ptr_tainted: &BTreeSet<String>,
    word_tainted: &BTreeSet<String>,
) -> bool {
    match expr {
        syn::Expr::Path(path) => path
            .path
            .get_ident()
            .is_some_and(|ident| word_tainted.contains(&ident.to_string())),
        syn::Expr::Cast(cast) => {
            !is_raw_ptr_type(&cast.ty)
                && (expr_is_word_derived(&cast.expr, ptr_tainted, word_tainted)
                    || expr_is_ptr_derived(&cast.expr, ptr_tainted, word_tainted))
        }
        syn::Expr::Paren(paren) => expr_is_word_derived(&paren.expr, ptr_tainted, word_tainted),
        syn::Expr::Group(group) => expr_is_word_derived(&group.expr, ptr_tainted, word_tainted),
        syn::Expr::Unary(u) => expr_is_word_derived(&u.expr, ptr_tainted, word_tainted),
        syn::Expr::Try(t) => expr_is_word_derived(&t.expr, ptr_tainted, word_tainted),
        syn::Expr::Field(f) => expr_is_word_derived(&f.base, ptr_tainted, word_tainted),
        syn::Expr::Struct(s) => {
            s.fields
                .iter()
                .any(|f| expr_is_word_derived(&f.expr, ptr_tainted, word_tainted))
                || s.rest
                    .as_deref()
                    .is_some_and(|r| expr_is_word_derived(r, ptr_tainted, word_tainted))
        }
        syn::Expr::Tuple(t) => t
            .elems
            .iter()
            .any(|e| expr_is_word_derived(e, ptr_tainted, word_tainted)),
        syn::Expr::Block(b) => block_tail_expr(&b.block)
            .is_some_and(|e| expr_is_word_derived(e, ptr_tainted, word_tainted)),
        syn::Expr::If(i) => {
            block_tail_expr(&i.then_branch)
                .is_some_and(|e| expr_is_word_derived(e, ptr_tainted, word_tainted))
                || i.else_branch
                    .as_ref()
                    .is_some_and(|(_, e)| expr_is_word_derived(e, ptr_tainted, word_tainted))
        }
        syn::Expr::Match(m) => m
            .arms
            .iter()
            .any(|a| expr_is_word_derived(&a.body, ptr_tainted, word_tainted)),
        syn::Expr::MethodCall(call) => {
            let method = call.method.to_string();
            if matches!(method.as_str(), "addr" | "expose_provenance")
                && expr_is_ptr_derived(&call.receiver, ptr_tainted, word_tainted)
            {
                return true;
            }
            !NON_PTR_RETURN_METHODS.contains(&method.as_str())
                && expr_is_word_derived(&call.receiver, ptr_tainted, word_tainted)
        }
        _ => false,
    }
}

/// Whether `expr` is derived from a raw pointer parameter (including `NonNull`),
/// or from a word parameter cast/converted to a raw pointer.
fn expr_is_ptr_derived(
    expr: &syn::Expr,
    ptr_tainted: &BTreeSet<String>,
    word_tainted: &BTreeSet<String>,
) -> bool {
    match expr {
        syn::Expr::Path(path) => path
            .path
            .get_ident()
            .is_some_and(|ident| ptr_tainted.contains(&ident.to_string())),
        syn::Expr::Cast(cast) => {
            if is_raw_ptr_type(&cast.ty) {
                expr_is_ptr_derived(&cast.expr, ptr_tainted, word_tainted)
                    || expr_is_word_derived(&cast.expr, ptr_tainted, word_tainted)
            } else {
                expr_is_ptr_derived(&cast.expr, ptr_tainted, word_tainted)
            }
        }
        syn::Expr::Paren(paren) => expr_is_ptr_derived(&paren.expr, ptr_tainted, word_tainted),
        syn::Expr::Group(group) => expr_is_ptr_derived(&group.expr, ptr_tainted, word_tainted),
        syn::Expr::Reference(r) => expr_is_ptr_derived(&r.expr, ptr_tainted, word_tainted),
        syn::Expr::Unary(u) => expr_is_ptr_derived(&u.expr, ptr_tainted, word_tainted),
        syn::Expr::Try(t) => expr_is_ptr_derived(&t.expr, ptr_tainted, word_tainted),
        syn::Expr::Field(f) => expr_is_ptr_derived(&f.base, ptr_tainted, word_tainted),
        syn::Expr::Struct(s) => {
            s.fields
                .iter()
                .any(|f| expr_is_ptr_derived(&f.expr, ptr_tainted, word_tainted))
                || s.rest
                    .as_deref()
                    .is_some_and(|r| expr_is_ptr_derived(r, ptr_tainted, word_tainted))
        }
        syn::Expr::Tuple(t) => t
            .elems
            .iter()
            .any(|e| expr_is_ptr_derived(e, ptr_tainted, word_tainted)),
        syn::Expr::Block(b) => block_tail_expr(&b.block)
            .is_some_and(|e| expr_is_ptr_derived(e, ptr_tainted, word_tainted)),
        syn::Expr::If(i) => {
            block_tail_expr(&i.then_branch)
                .is_some_and(|e| expr_is_ptr_derived(e, ptr_tainted, word_tainted))
                || i.else_branch
                    .as_ref()
                    .is_some_and(|(_, e)| expr_is_ptr_derived(e, ptr_tainted, word_tainted))
        }
        syn::Expr::Match(m) => m
            .arms
            .iter()
            .any(|a| expr_is_ptr_derived(&a.body, ptr_tainted, word_tainted)),
        syn::Expr::MethodCall(call) => {
            let method = call.method.to_string();
            !NON_PTR_RETURN_METHODS.contains(&method.as_str())
                && expr_is_ptr_derived(&call.receiver, ptr_tainted, word_tainted)
        }
        syn::Expr::Call(call) => {
            if is_word_to_ptr_constructor(&call.func) {
                return call.args.iter().any(|arg| {
                    expr_is_word_derived(arg, ptr_tainted, word_tainted)
                        || expr_is_ptr_derived(arg, ptr_tainted, word_tainted)
                });
            }
            if is_safe_ptr_free_call(&call.func) {
                return false;
            }
            call.args
                .iter()
                .any(|arg| expr_is_ptr_derived(arg, ptr_tainted, word_tainted))
        }
        _ => false,
    }
}

/// Whether `expr` is a word-tainted expression cast via `as _` (e.g.
/// `Value::from_cons_ptr(word as _)` or `NonNull::new_unchecked(word as _)`).
fn expr_is_word_inferred_cast(
    expr: &syn::Expr,
    ptr_tainted: &BTreeSet<String>,
    word_tainted: &BTreeSet<String>,
) -> bool {
    match expr {
        syn::Expr::Cast(cast) => {
            matches!(&*cast.ty, syn::Type::Infer(_))
                && expr_is_word_derived(&cast.expr, ptr_tainted, word_tainted)
        }
        syn::Expr::Paren(p) => expr_is_word_inferred_cast(&p.expr, ptr_tainted, word_tainted),
        syn::Expr::Group(g) => expr_is_word_inferred_cast(&g.expr, ptr_tainted, word_tainted),
        _ => false,
    }
}

fn block_tail_expr(block: &syn::Block) -> Option<&syn::Expr> {
    match block.stmts.last()? {
        syn::Stmt::Expr(expr, None) => Some(expr),
        _ => None,
    }
}

/// Whether `expr` mentions any pointer-tainted identifier or word-to-pointer
/// cast anywhere inside a dereference operand.
fn expr_mentions_ptr_or_word_cast(
    expr: &syn::Expr,
    ptr_tainted: &BTreeSet<String>,
    word_tainted: &BTreeSet<String>,
) -> bool {
    struct Finder<'a> {
        ptr_tainted: &'a BTreeSet<String>,
        word_tainted: &'a BTreeSet<String>,
        found: bool,
    }
    impl<'ast> Visit<'ast> for Finder<'_> {
        fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
            if let Some(ident) = path.path.get_ident()
                && self.ptr_tainted.contains(&ident.to_string())
            {
                self.found = true;
            }
            visit::visit_expr_path(self, path);
        }

        fn visit_expr_cast(&mut self, cast: &'ast syn::ExprCast) {
            if (is_raw_ptr_type(&cast.ty) || matches!(&*cast.ty, syn::Type::Infer(_)))
                && expr_is_word_derived(&cast.expr, self.ptr_tainted, self.word_tainted)
            {
                self.found = true;
            }
            visit::visit_expr_cast(self, cast);
        }

        fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
            if is_word_to_ptr_constructor(&call.func)
                && call.args.iter().any(|arg| {
                    expr_is_word_derived(arg, self.ptr_tainted, self.word_tainted)
                        || expr_is_ptr_derived(arg, self.ptr_tainted, self.word_tainted)
                })
            {
                self.found = true;
            }
            visit::visit_expr_call(self, call);
        }
    }
    let mut finder = Finder {
        ptr_tainted,
        word_tainted,
        found: false,
    };
    finder.visit_expr(expr);
    finder.found
}

fn is_safe_ptr_free_call(func: &syn::Expr) -> bool {
    let syn::Expr::Path(path) = func else {
        return false;
    };
    let segs: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    matches!(
        segs.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice(),
        ["ptr", "eq"]
            | ["std", "ptr", "eq"]
            | ["core", "ptr", "eq"]
            | ["ptr", "addr_eq"]
            | ["std", "ptr", "addr_eq"]
            | ["core", "ptr", "addr_eq"]
    )
}

fn is_value_from_bits_call(func: &syn::Expr) -> bool {
    let syn::Expr::Path(path) = func else {
        return false;
    };
    let segs: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let Some(last) = segs.last() else {
        return false;
    };
    if last != "from_bits" {
        return false;
    }
    match segs.len() {
        1 => true,
        n => matches!(segs[n - 2].as_str(), "Value" | "TaggedValue"),
    }
}

struct FnBodyScanner {
    ptr_tainted: BTreeSet<String>,
    word_tainted: BTreeSet<String>,
    in_unsafe: usize,
    ptr_offense: Option<usize>,
    word_offense: Option<usize>,
}

impl FnBodyScanner {
    fn new(ptr_params: Vec<String>, word_params: Vec<String>) -> Self {
        Self {
            ptr_tainted: ptr_params.into_iter().collect(),
            word_tainted: word_params.into_iter().collect(),
            in_unsafe: 0,
            ptr_offense: None,
            word_offense: None,
        }
    }

    fn taint_idents_from_expr(&mut self, expr: &syn::Expr, idents: &[String]) {
        if expr_is_ptr_derived(expr, &self.ptr_tainted, &self.word_tainted) {
            for name in idents {
                self.ptr_tainted.insert(name.clone());
            }
        }
        if expr_is_word_derived(expr, &self.ptr_tainted, &self.word_tainted) {
            for name in idents {
                self.word_tainted.insert(name.clone());
            }
        }
    }

    fn scan_macro_tokens(&mut self, tokens: TokenStream) {
        if let Ok(block) = syn::parse2::<syn::Block>(tokens.clone()) {
            self.visit_block(&block);
            return;
        }
        if let Ok(exprs) = syn::parse::Parser::parse2(
            syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
            tokens.clone(),
        ) {
            for expr in &exprs {
                self.visit_expr(expr);
            }
            return;
        }
        // Fallback for custom macro grammars such as `jit_shim_contain!(detach args, ctx, SENTINEL, { ... })`:
        // recursively inspect nested groups as blocks/expr lists, and also track
        // raw `unsafe { ... }` blocks when a group contains non-standard syntax.
        let trees: Vec<TokenTree> = tokens.into_iter().collect();
        let mut i = 0;
        while i < trees.len() {
            if let TokenTree::Ident(ident) = &trees[i]
                && ident == "unsafe"
                && let Some(TokenTree::Group(group)) = trees.get(i + 1)
                && group.delimiter() == Delimiter::Brace
            {
                self.in_unsafe += 1;
                self.scan_macro_tokens(group.stream());
                self.in_unsafe -= 1;
                i += 2;
                continue;
            }
            if let TokenTree::Group(group) = &trees[i] {
                if group.delimiter() == Delimiter::Brace {
                    let braced = TokenStream::from(TokenTree::Group(group.clone()));
                    if let Ok(block) = syn::parse2::<syn::Block>(braced) {
                        self.visit_block(&block);
                        i += 1;
                        continue;
                    }
                }
                self.scan_macro_tokens(group.stream());
            }
            i += 1;
        }
    }
}

impl<'ast> Visit<'ast> for FnBodyScanner {
    // Nested `fn` items have their own safety boundary and are visited by the
    // outer file visitor directly.
    fn visit_item_fn(&mut self, _node: &'ast syn::ItemFn) {}

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let Some(init) = &local.init {
            self.visit_expr(&init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr(diverge);
            }
            let mut bound = Vec::new();
            collect_pat_idents(&local.pat, &mut bound);
            self.taint_idents_from_expr(&init.expr, &bound);
        }
    }

    fn visit_expr_assign(&mut self, node: &'ast syn::ExprAssign) {
        self.visit_expr(&node.right);
        let mut targets = Vec::new();
        collect_assign_target_idents(&node.left, &mut targets);
        self.taint_idents_from_expr(&node.right, &targets);
        self.visit_expr(&node.left);
    }

    fn visit_expr_let(&mut self, node: &'ast syn::ExprLet) {
        self.visit_expr(&node.expr);
        let mut bound = Vec::new();
        collect_pat_idents(&node.pat, &mut bound);
        self.taint_idents_from_expr(&node.expr, &bound);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.visit_expr(&node.expr);
        let mut bound = Vec::new();
        collect_pat_idents(&node.pat, &mut bound);
        self.taint_idents_from_expr(&node.expr, &bound);
        self.visit_block(&node.body);
    }

    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        self.visit_expr(&node.expr);
        let expr_ptr = expr_is_ptr_derived(&node.expr, &self.ptr_tainted, &self.word_tainted);
        let expr_word = expr_is_word_derived(&node.expr, &self.ptr_tainted, &self.word_tainted);
        for arm in &node.arms {
            if expr_ptr || expr_word {
                let mut bound = Vec::new();
                collect_pat_idents(&arm.pat, &mut bound);
                if expr_ptr {
                    for name in &bound {
                        self.ptr_tainted.insert(name.clone());
                    }
                }
                if expr_word {
                    for name in &bound {
                        self.word_tainted.insert(name.clone());
                    }
                }
            }
            self.visit_pat(&arm.pat);
            self.visit_expr(&arm.body);
        }
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.scan_macro_tokens(mac.tokens.clone());
    }

    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        self.in_unsafe += 1;
        visit::visit_expr_unsafe(self, node);
        self.in_unsafe -= 1;
    }

    fn visit_expr_unary(&mut self, node: &'ast syn::ExprUnary) {
        if self.in_unsafe > 0
            && matches!(node.op, syn::UnOp::Deref(_))
            && expr_mentions_ptr_or_word_cast(&node.expr, &self.ptr_tainted, &self.word_tainted)
            && self.ptr_offense.is_none()
        {
            self.ptr_offense = Some(node.op.span().start().line);
        }
        visit::visit_expr_unary(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if self.in_unsafe > 0 && self.ptr_offense.is_none() {
            let method = node.method.to_string();
            let recv_tainted =
                expr_is_ptr_derived(&node.receiver, &self.ptr_tainted, &self.word_tainted)
                    || expr_is_word_inferred_cast(
                        &node.receiver,
                        &self.ptr_tainted,
                        &self.word_tainted,
                    );
            let arg_tainted = node
                .args
                .iter()
                .any(|arg| expr_is_ptr_derived(arg, &self.ptr_tainted, &self.word_tainted));
            if (recv_tainted && !SAFE_PTR_METHODS.contains(&method.as_str()))
                || (arg_tainted && !matches!(method.as_str(), "eq" | "ne" | "cmp"))
            {
                self.ptr_offense = Some(node.method.span().start().line);
            }
        }
        visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if self.in_unsafe > 0
            && self.ptr_offense.is_none()
            && !is_safe_ptr_free_call(&node.func)
            && node.args.iter().any(|arg| {
                expr_is_ptr_derived(arg, &self.ptr_tainted, &self.word_tainted)
                    || (is_ptr_accepting_inferred_cast_call(&node.func)
                        && expr_is_word_inferred_cast(arg, &self.ptr_tainted, &self.word_tainted))
            })
        {
            self.ptr_offense = Some(node.paren_token.span.open().start().line);
        }
        if self.word_offense.is_none()
            && is_value_from_bits_call(&node.func)
            && node
                .args
                .iter()
                .any(|arg| expr_is_word_derived(arg, &self.ptr_tainted, &self.word_tainted))
        {
            self.word_offense = Some(node.paren_token.span.open().start().line);
        }
        visit::visit_expr_call(self, node);
    }
}

/// Replace `$ident` macro metavariables with synthetic valid identifiers
/// (`__macro_<ident>`) preserving the original span so `syn` can parse `fn`
/// definitions embedded inside `macro_rules!` bodies with exact line numbers.
fn normalize_macro_body_tokens(tokens: TokenStream) -> TokenStream {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    let mut out = Vec::with_capacity(trees.len());
    let mut i = 0;
    while i < trees.len() {
        if let TokenTree::Punct(p) = &trees[i]
            && p.as_char() == '$'
        {
            if let Some(TokenTree::Ident(ident)) = trees.get(i + 1) {
                let synth = Ident::new(&format!("__macro_{ident}"), ident.span());
                out.push(TokenTree::Ident(synth));
                i += 2;
                continue;
            }
            if let Some(TokenTree::Group(group)) = trees.get(i + 1)
                && group.delimiter() == Delimiter::Parenthesis
            {
                // Flatten `$( ... )sep*` repetition to a single instance so
                // inner `fn` items or statements can still be parsed.
                out.extend(normalize_macro_body_tokens(group.stream()));
                i += 2;
                if let Some(TokenTree::Punct(sep)) = trees.get(i)
                    && !matches!(sep.as_char(), '*' | '+' | '?')
                {
                    i += 1;
                }
                if let Some(TokenTree::Punct(rep)) = trees.get(i)
                    && matches!(rep.as_char(), '*' | '+' | '?')
                {
                    i += 1;
                }
                continue;
            }
        }
        match &trees[i] {
            TokenTree::Group(group) => {
                let mut rebuilt = Group::new(
                    group.delimiter(),
                    normalize_macro_body_tokens(group.stream()),
                );
                rebuilt.set_span(group.span());
                out.push(TokenTree::Group(rebuilt));
            }
            other => out.push(other.clone()),
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// Extract `fn` items from a `macro_rules!` definition by scanning each
/// `(...) => { ... }` rule arm's body.
fn extract_macro_rules_fns(tokens: TokenStream) -> Vec<syn::ItemFn> {
    let mut fns = Vec::new();
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    let mut i = 0;
    while i + 2 < trees.len() {
        let is_fat_arrow = matches!(
            (&trees[i], &trees[i + 1]),
            (TokenTree::Punct(eq), TokenTree::Punct(gt))
                if eq.as_char() == '=' && eq.spacing() == Spacing::Joint && gt.as_char() == '>'
        );
        if is_fat_arrow && let Some(TokenTree::Group(body)) = trees.get(i + 2) {
            let normalized = normalize_macro_body_tokens(body.stream());
            if let Ok(file) = syn::parse2::<syn::File>(normalized.clone()) {
                struct MacroFnCollector<'a>(&'a mut Vec<syn::ItemFn>);
                impl<'ast> Visit<'ast> for MacroFnCollector<'_> {
                    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
                        self.0.push(node.clone());
                        visit::visit_item_fn(self, node);
                    }
                    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
                        self.0.push(syn::ItemFn {
                            attrs: node.attrs.clone(),
                            vis: node.vis.clone(),
                            modifiers: node.modifiers.clone(),
                            sig: node.sig.clone(),
                            block: Box::new(node.block.clone()),
                        });
                        visit::visit_impl_item_fn(self, node);
                    }
                }
                MacroFnCollector(&mut fns).visit_file(&file);
            } else {
                // Fallback: extract individual `fn ... { ... }` slices from
                // the normalized body token stream if the arm mixes statements
                // or fragments with item definitions.
                let body_trees: Vec<TokenTree> = normalized.into_iter().collect();
                let mut k = 0;
                while k < body_trees.len() {
                    if let TokenTree::Ident(ident) = &body_trees[k]
                        && ident == "fn"
                    {
                        // Walk backward over leading modifiers/attributes (`#[...]`,
                        // `pub(...)`, `const`, `async`, `unsafe`, `extern "..."`).
                        let mut start = k;
                        while start > 0 {
                            match &body_trees[start - 1] {
                                TokenTree::Ident(prev)
                                    if matches!(
                                        prev.to_string().as_str(),
                                        "pub" | "const" | "async" | "unsafe" | "extern"
                                    ) =>
                                {
                                    start -= 1;
                                }
                                TokenTree::Literal(_)
                                    if start >= 2
                                        && matches!(&body_trees[start - 2], TokenTree::Ident(e) if e == "extern") =>
                                {
                                    start -= 2;
                                }
                                TokenTree::Group(g)
                                    if g.delimiter() == Delimiter::Parenthesis
                                        && start >= 2
                                        && matches!(&body_trees[start - 2], TokenTree::Ident(p) if p == "pub") =>
                                {
                                    start -= 2;
                                }
                                TokenTree::Group(g)
                                    if g.delimiter() == Delimiter::Bracket
                                        && start >= 2
                                        && matches!(&body_trees[start - 2], TokenTree::Punct(p) if p.as_char() == '#') =>
                                {
                                    start -= 2;
                                }
                                _ => break,
                            }
                        }
                        // Walk forward to the function's body `{ ... }`.
                        let mut end = k + 1;
                        while end < body_trees.len() {
                            if let TokenTree::Group(g) = &body_trees[end]
                                && g.delimiter() == Delimiter::Brace
                            {
                                end += 1;
                                break;
                            }
                            if let TokenTree::Punct(p) = &body_trees[end]
                                && p.as_char() == ';'
                            {
                                end = k + 1;
                                break;
                            }
                            end += 1;
                        }
                        if end > k + 1 {
                            let slice: TokenStream =
                                body_trees[start..end].iter().cloned().collect();
                            if let Ok(item_fn) = syn::parse2::<syn::ItemFn>(slice) {
                                fns.push(item_fn);
                                k = end;
                                continue;
                            }
                        }
                    }
                    k += 1;
                }
            }
            i += 3;
            continue;
        }
        i += 1;
    }
    let _ = Span::call_site();
    fns
}

#[derive(Default)]
struct ClassGuardScan {
    file: String,
    fn_count: usize,
    ptr_offenders: Vec<String>,
    word_offenders: Vec<String>,
    ffi_handle_offenders: Vec<String>,
}

impl ClassGuardScan {
    fn check_fn(&mut self, attrs: &[syn::Attribute], sig: &syn::Signature, block: &syn::Block) {
        if has_cfg_test(attrs) || matches!(sig.safety, syn::Safety::Unsafe(_)) {
            return;
        }
        self.fn_count += 1;
        let mut ptr_params = Vec::new();
        let mut word_params = Vec::new();
        let mut has_raw_ffi_handle = false;
        for arg in &sig.inputs {
            let syn::FnArg::Typed(typed) = arg else {
                continue;
            };
            if is_raw_ffi_handle_type(&typed.ty) {
                has_raw_ffi_handle = true;
            }
            if is_raw_ptr_type(&typed.ty) {
                collect_pat_idents(&typed.pat, &mut ptr_params);
            } else if is_word_int_type(&typed.ty) {
                collect_pat_idents(&typed.pat, &mut word_params);
            }
        }
        let fn_line = sig.ident.span().start().line;
        if has_raw_ffi_handle {
            self.ffi_handle_offenders.push(format!(
                "{}:{fn_line}: fn {} (raw C FFI handle parameter)",
                self.file, sig.ident
            ));
        }
        if ptr_params.is_empty() && word_params.is_empty() {
            return;
        }
        let mut scanner = FnBodyScanner::new(ptr_params, word_params);
        scanner.visit_block(block);
        if let Some(use_line) = scanner.ptr_offense {
            self.ptr_offenders.push(format!(
                "{}:{fn_line}: fn {} (unsafe ptr use at line {use_line})",
                self.file, sig.ident
            ));
        }
        if let Some(use_line) = scanner.word_offense {
            self.word_offenders.push(format!(
                "{}:{fn_line}: fn {} (from_bits on word param at line {use_line})",
                self.file, sig.ident
            ));
        }
    }
}

impl<'ast> Visit<'ast> for ClassGuardScan {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        visit::visit_item_mod(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        visit::visit_item_impl(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.check_fn(&node.attrs, &node.sig, &node.block);
        visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.check_fn(&node.attrs, &node.sig, &node.block);
        visit::visit_impl_item_fn(self, node);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        if let Some(block) = &node.default {
            self.check_fn(&node.attrs, &node.sig, block);
        }
        visit::visit_trait_item_fn(self, node);
    }

    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        if node.mac.path.is_ident("macro_rules") {
            for item_fn in extract_macro_rules_fns(node.mac.tokens.clone()) {
                self.check_fn(&item_fn.attrs, &item_fn.sig, &item_fn.block);
            }
        }
    }
}

struct ClassGuardReport {
    ptr_offenders: Vec<String>,
    word_offenders: Vec<String>,
    ffi_handle_offenders: Vec<String>,
    fn_count: usize,
}

fn scan_class_guard_source(file: &str, source: &str) -> ClassGuardReport {
    let parsed = syn::parse_file(source).unwrap_or_else(|error| panic!("parse {file}: {error}"));
    let mut scan = ClassGuardScan {
        file: file.to_owned(),
        ..ClassGuardScan::default()
    };
    scan.visit_file(&parsed);
    ClassGuardReport {
        ptr_offenders: scan.ptr_offenders,
        word_offenders: scan.word_offenders,
        ffi_handle_offenders: scan.ffi_handle_offenders,
        fn_count: scan.fn_count,
    }
}

#[test]
fn class_guard_controls_detect_safe_ptr_deref_and_word_from_bits() {
    // Synthetic offenders MUST be caught.
    for (label, fixture, expect_ptr, expect_word) in [
        (
            "deref raw ptr param",
            "fn bad(p: *const u8) -> u8 { unsafe { *p } }",
            true,
            false,
        ),
        (
            "pass raw ptr param to unsafe call",
            "fn bad(p: *mut u8) { unsafe { ffi(p); } }",
            true,
            false,
        ),
        (
            "call as_ref on raw ptr param",
            "fn bad(p: *mut u8) -> Option<&'static u8> { unsafe { p.as_ref() } }",
            true,
            false,
        ),
        (
            "tainted local from raw ptr param",
            "fn bad(p: *mut u8) -> u32 { let q = p as *const u32; unsafe { *q } }",
            true,
            false,
        ),
        (
            "emacs_value typedef deref",
            "fn bad(v: emacs_value) -> usize { unsafe { (*v).v } }",
            true,
            false,
        ),
        (
            "from_bits on word param",
            "fn bad(bits: u64) -> Value { unsafe { Value::from_bits(bits as usize) } }",
            false,
            true,
        ),
        (
            "from_bits via tainted local",
            "fn bad(raw: i64) -> Value { let w = raw as usize; unsafe { TaggedValue::from_bits(w) } }",
            false,
            true,
        ),
        // B1: macro argument deref (jit_shim_contain! shape)
        (
            "deref inside macro argument block",
            "fn bad(ctx: *mut u8) -> i64 { jit_shim_contain!(ctx, 0, { let c = unsafe { &*(ctx as *const Context) }; c.x }) }",
            true,
            false,
        ),
        (
            "deref inside detach macro argument block",
            "fn bad(ctx: *mut u8, args: *const i64) -> i64 { jit_shim_contain!(detach args, ctx, 0, { unsafe { *args } }) }",
            true,
            false,
        ),
        // B1: safe fn generated inside macro_rules! body (leaf_abi $rest shape)
        (
            "safe fn inside macro_rules! body",
            "macro_rules! def_rest { ($rest:ident, $spec:path, $body:path) => { #[cold] fn $rest(ctx: *const Context) -> i64 { unsafe { leaf_contained(ctx, &$spec, |c| $body(c)) } } }; }",
            true,
            false,
        ),
        // B2: word-as-pointer casts (FwdRef::from_address / mark_run_at / from_cons_ptr shapes)
        (
            "usize word cast to raw ptr and passed to NonNull::new_unchecked",
            "fn bad(address: usize) -> FwdRef { FwdRef(unsafe { std::ptr::NonNull::new_unchecked(address as *mut LispFwd) }) }",
            true,
            false,
        ),
        (
            "usize word cast to raw ptr and dereferenced",
            "fn bad(base: usize) -> u8 { let p = base as *const u8; unsafe { *p } }",
            true,
            false,
        ),
        (
            "usize word cast via as _ to Value::from_cons_ptr",
            "fn bad(word: usize) -> Value { unsafe { Value::from_cons_ptr(word as _) } }",
            true,
            false,
        ),
        // B3: NonNull<T> parameter deref / FFI pass
        (
            "NonNull param as_ref",
            "fn bad(root: NonNull<TaggedHeap>) -> usize { unsafe { root.as_ref().len() } }",
            true,
            false,
        ),
        (
            "NonNull param as_ptr passed to unsafe ffi",
            "fn bad(display: NonNull<c_void>) { unsafe { from_foreign(display.as_ptr().cast()) } }",
            true,
            false,
        ),
        // B4: extended taint propagation
        (
            "taint via assignment",
            "fn bad(p: *const u8) -> u8 { let mut q = core::ptr::null(); q = p; unsafe { *q } }",
            true,
            false,
        ),
        (
            "taint via if expression",
            "fn bad(p: *const u8, flag: bool) -> u8 { let q = if flag { p } else { core::ptr::null() }; unsafe { *q } }",
            true,
            false,
        ),
        (
            "taint via match expression",
            "fn bad(p: *const u8, n: usize) -> u8 { let q = match n { 0 => p, _ => core::ptr::null() }; unsafe { *q } }",
            true,
            false,
        ),
        (
            "taint via if let pattern",
            "fn bad(p: *mut u8) -> u8 { if let Some(q) = NonNull::new(p) { return unsafe { *q.as_ref() }; } 0 }",
            true,
            false,
        ),
        (
            "taint via struct field",
            "fn bad(p: *const u8) -> u8 { let w = Wrap { ptr: p }; unsafe { *w.ptr } }",
            true,
            false,
        ),
        (
            "taint via call result NonNull::new(p)?",
            "fn bad(p: *mut u8) -> Option<&'static u8> { let q = NonNull::new(p)?; Some(unsafe { q.as_ref() }) }",
            true,
            false,
        ),
        // N1: cfg(any(target_os = "macos", test)) is production on macOS, MUST NOT be skipped
        (
            "cfg(any(target_os, test)) is still production",
            "#[cfg(any(target_os = \"macos\", test))] fn bad(p: *const u8) -> u8 { unsafe { *p } }",
            true,
            false,
        ),
    ] {
        let report = scan_class_guard_source("fixture.rs", fixture);
        assert_eq!(
            !report.ptr_offenders.is_empty(),
            expect_ptr,
            "ptr control mismatch for {label}: {:?}",
            report.ptr_offenders
        );
        assert_eq!(
            !report.word_offenders.is_empty(),
            expect_word,
            "word control mismatch for {label}: {:?}",
            report.word_offenders
        );
    }

    // Rule (f): safe fns taking raw C FFI handle types (by value, pointer,
    // Option, NonNull, or ref) must be caught.
    for (label, fixture) in [
        (
            "safe pub fn taking ffi::TSNode",
            "impl Q { pub fn matches(&self, node: ffi::TSNode) {} }",
        ),
        (
            "safe fn taking tree_sitter::ffi::TSNode",
            "fn node_on_clone<'tree>(tree: &'tree WalkClone, raw: tree_sitter::ffi::TSNode) {}",
        ),
        (
            "safe fn taking *mut ffi::sqlite3_stmt",
            "fn bind_values(connection: &Connection, stmt: *mut ffi::sqlite3_stmt) {}",
        ),
        (
            "safe fn taking Option<NonNull<ffi::TSQuery>>",
            "fn bad(q: Option<NonNull<ffi::TSQuery>>) {}",
        ),
        ("safe fn taking *mut sqlite3", "fn bad(db: *mut sqlite3) {}"),
    ] {
        let report = scan_class_guard_source("fixture.rs", fixture);
        assert!(
            !report.ffi_handle_offenders.is_empty(),
            "rule (f) control must flag {label}"
        );
    }

    // Valid shapes MUST NOT be flagged.
    for (label, fixture) in [
        (
            "unsafe fn dereferencing raw ptr",
            "unsafe fn ok(p: *const u8) -> u8 { unsafe { *p } }",
        ),
        (
            "unsafe fn calling from_bits",
            "unsafe fn ok(bits: usize) -> Value { unsafe { Value::from_bits(bits) } }",
        ),
        (
            "unsafe fn inside macro_rules! body",
            "macro_rules! def_ok { ($rest:ident) => { unsafe fn $rest(ctx: *const Context) -> i64 { unsafe { leaf_contained(ctx) } } }; }",
        ),
        (
            "safe identity comparison inside unsafe chain walk",
            "impl Chain { fn ok(&self, target: *mut Node) -> bool { unsafe { let mut c = self.head; while !c.is_null() { if c == target { return true; } c = (*c).next; } false } } }",
        ),
        (
            "safe null check only",
            "fn ok(p: *const u8) -> bool { p.is_null() }",
        ),
        (
            "from_bits on table slot indexed by usize param",
            "fn ok(&self, idx: usize) -> Value { unsafe { Value::from_bits(self.table[idx]) } }",
        ),
        (
            "f64::from_bits",
            "fn ok(bits: u64) -> f64 { f64::from_bits(bits) }",
        ),
        (
            "usize length passed with as _ to unsafe syscall",
            "fn ok(&self, len: usize) { unsafe { libc::write(1, self.ptr, len as _); } }",
        ),
        (
            "address of local holding word param passed to ffi",
            "fn ok(epoch: i64) { let mut t = epoch; unsafe { libc::localtime_r(&mut t as *mut _); } }",
        ),
        (
            "cfg(test) is skipped",
            "#[cfg(test)] fn ok(p: *const u8) -> u8 { unsafe { *p } }",
        ),
        (
            "cfg(all(feature, test)) is skipped",
            "#[cfg(all(feature = \"jit\", test))] fn ok(p: *const u8) -> u8 { unsafe { *p } }",
        ),
        (
            "unsafe fn taking raw TSNode",
            "impl Q { pub unsafe fn matches(&self, node: ffi::TSNode) {} }",
        ),
        (
            "unsafe fn taking *mut ffi::sqlite3_stmt",
            "unsafe fn row_to_value(stmt: *mut ffi::sqlite3_stmt) {}",
        ),
        (
            "safe fn taking typed &PreparedStatement owner",
            "fn bind_values(connection: &Connection, statement: &PreparedStatement) {}",
        ),
        (
            "safe fn whose parameter is merely named sqlite3",
            "fn helper(sqlite3: &Connection) {}",
        ),
        (
            "foreign extern C import block",
            "extern \"C\" { fn sqlite3_reset(stmt: *mut sqlite3_stmt) -> i32; }",
        ),
    ] {
        let report = scan_class_guard_source("fixture.rs", fixture);
        assert!(
            report.ptr_offenders.is_empty()
                && report.word_offenders.is_empty()
                && report.ffi_handle_offenders.is_empty(),
            "valid control flagged for {label}: ptrs={:?} words={:?} ffi={:?}",
            report.ptr_offenders,
            report.word_offenders,
            report.ffi_handle_offenders
        );
    }
}

#[test]
fn safe_fns_do_not_deref_raw_ptr_or_word_params() {
    let files = workspace_production_source_files();
    assert!(
        files.len() > 200,
        "G4 class guard must scan every workspace production source file, saw {}",
        files.len()
    );
    let mut ptr_offenders = Vec::new();
    let mut word_offenders = Vec::new();
    let mut ffi_handle_offenders = Vec::new();
    let mut fn_count = 0usize;
    for (relative, path) in &files {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("read {relative}: {error}"));
        let report = scan_class_guard_source(relative, &source);
        fn_count += report.fn_count;
        ptr_offenders.extend(report.ptr_offenders);
        word_offenders.extend(report.word_offenders);
        ffi_handle_offenders.extend(report.ffi_handle_offenders);
    }
    assert!(
        fn_count > 1000,
        "G4 class guard scanned too few production fns ({fn_count})"
    );
    assert!(
        ptr_offenders.is_empty() && word_offenders.is_empty() && ffi_handle_offenders.is_empty(),
        "G4 class guard: a safe fn must not dereference a raw pointer parameter, \
         rebuild a Value from a word parameter via from_bits, or take a raw C FFI \
         handle parameter; mark it `unsafe fn` with a `# Safety` section or take \
         a validated/typed owner.\n\
         raw-ptr offenders ({}):\n{}\n\
         word-param from_bits offenders ({}):\n{}\n\
         raw-FFI-handle offenders ({}):\n{}",
        ptr_offenders.len(),
        ptr_offenders.join("\n"),
        word_offenders.len(),
        word_offenders.join("\n"),
        ffi_handle_offenders.len(),
        ffi_handle_offenders.join("\n"),
    );
}
