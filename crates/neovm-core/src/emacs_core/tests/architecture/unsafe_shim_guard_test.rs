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
//! (c) No safe `extern "C"` fn whose body rebuilds a heap value from one of
//!     its parameters with `from_bits`, `Value::from_bits`, or
//!     `TaggedValue::from_bits`, including through a local copied from that
//!     parameter. A word is the same unsound contract as a raw pointer: safe
//!     code can pass garbage bits. A no-argument shim is not this shape.

use super::architecture_production_source_roots::production_source_files;

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
