//! Architecture guard for peer R4 (cfg(test) audit refinement): no moved
//! production file may contain a bare `test` cfg predicate except on a test
//! module declaration.
//!
//! The peer's refinement is strictest: every non-module `cfg(test)`,
//! `cfg!(test)`, `cfg(not(test))` and `cfg_attr(test, ..)` in the moved
//! production files is converted to the `test-support` feature form:
//! `any(test, feature = "test-support")`.  This guard allows a bare `test`
//! leaf only when it is a direct child of an `any(..)` that also has a direct
//! `feature = "test-support"` child, so `any(test, feature = "test-support")`
//! and `all(any(test, feature = "test-support"), debug_assertions)` pass while
//! `all(test, feature = "test-support")` and bare `test` fail.
//!
//! Test module declarations are exempt: `#[cfg(test)] mod tests;`,
//! `#[cfg(test)] mod tests { .. }`, and any other cfg attribute attached to a
//! `mod` declaration.  They carry tests, not product behaviour, and move with
//! their tests.
//!
//! Fail-closed: the manifest must list the 182 production files, every row
//! must resolve to exactly one source file, unreadable files and unparseable
//! predicaments that mention `test` are violations, and a syntax-clean file is
//! scanned structurally (comments and literals removed first).
//!
//! This guard is RED until the vmcall lane's conversion commit lands; that is
//! the intended sequencing.  It never takes an allowlist.

use std::path::{Path, PathBuf};

use super::architecture_production_source_roots::production_source_root;

const MANIFEST: &str = include_str!("moved_production_files.tsv");
const EXPECTED_FILES: usize = 183;

#[derive(Debug)]
struct Violation {
    file: String,
    line: usize,
    text: String,
}

#[derive(Debug, PartialEq, Eq)]
enum Pred {
    Any(Vec<Pred>),
    All(Vec<Pred>),
    Not(Box<Pred>),
    Feature(String),
    Ident(String),
    Other,
}

// ---------------------------------------------------------------------------
// Lexing: remove comments, preserve string/char/raw-string literals.
// ---------------------------------------------------------------------------

fn skip_normal_string(bytes: &[u8], mut i: usize) -> usize {
    // i at the opening quote.
    i += 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

fn skip_raw_string(bytes: &[u8], prefix_len: usize) -> Option<usize> {
    // Detected `r`, `br`, `cr` followed by zero or more '#' and a '"'.
    let mut i = prefix_len;
    let mut hashes = 0;
    while i < bytes.len() && bytes[i] == b'#' {
        hashes += 1;
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'"' {
        return None;
    }
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let mut j = i + 1;
            let mut seen = 0;
            while j < bytes.len() && bytes[j] == b'#' && seen < hashes {
                seen += 1;
                j += 1;
            }
            if seen == hashes {
                return Some(j);
            }
        }
        i += 1;
    }
    Some(bytes.len())
}

fn skip_char_or_lifetime(bytes: &[u8], i: usize) -> usize {
    // i at `'`.  A char literal is `'x'`, `'\n'`, `'\u{..}'`; anything else is
    // a lifetime/label and is ordinary punctuation for this scanner.
    let mut j = i + 1;
    if j >= bytes.len() {
        return i + 1;
    }
    if bytes[j] == b'\\' {
        j += 2;
        while j < bytes.len() && bytes[j] != b'\'' && bytes[j] != b'\n' {
            j += 1;
        }
        return if j < bytes.len() && bytes[j] == b'\'' {
            j + 1
        } else {
            i + 1
        };
    }
    // One UTF-8 scalar value: skip continuation bytes, then expect `'`.
    j += 1;
    while j < bytes.len() && (bytes[j] & 0b1100_0000) == 0b1000_0000 {
        j += 1;
    }
    if j < bytes.len() && bytes[j] == b'\'' {
        j + 1
    } else {
        i + 1
    }
}

fn cleaned_code(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let mut depth = 1usize;
            out.push(' ');
            out.push(' ');
            i += 2;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    depth += 1;
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                } else if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    depth -= 1;
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                } else {
                    out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
            continue;
        }
        // Raw strings: r"", r#".."#, br.., cr..
        let raw_prefix = match b {
            b'r' => Some(1),
            b'b' | b'c' if i + 1 < bytes.len() && bytes[i + 1] == b'r' => Some(2),
            _ => None,
        };
        if let Some(plen) = raw_prefix {
            if let Some(end) = skip_raw_string(bytes, i + plen) {
                let literal = &text[i..end];
                out.push_str(literal);
                i = end;
                continue;
            }
        }
        if b == b'"' {
            let end = skip_normal_string(bytes, i);
            out.push_str(&text[i..end]);
            i = end;
            continue;
        }
        if b == b'\'' {
            let end = skip_char_or_lifetime(bytes, i);
            out.push_str(&text[i..end]);
            i = end;
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Structural scan.
// ---------------------------------------------------------------------------

fn line_of(code: &str, idx: usize) -> usize {
    code.as_bytes()[..idx]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

fn skip_literal(bytes: &[u8], i: usize) -> usize {
    let b = bytes[i];
    let raw_prefix = match b {
        b'r' => Some(1),
        b'b' | b'c' if i + 1 < bytes.len() && bytes[i + 1] == b'r' => Some(2),
        _ => None,
    };
    if let Some(plen) = raw_prefix {
        if let Some(end) = skip_raw_string(bytes, i + plen) {
            return end;
        }
    }
    if b == b'"' {
        return skip_normal_string(bytes, i);
    }
    if b == b'\'' {
        return skip_char_or_lifetime(bytes, i);
    }
    i + 1
}

fn bracket_span(bytes: &[u8], start: usize) -> Option<usize> {
    // start at '['; returns the index just past the matching ']'.
    let mut depth = 0i32;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' => {
                i = skip_literal(bytes, i);
                continue;
            }
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn paren_span(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    // start at '('; returns (index past ')', inner start=start+1).
    let mut depth = 0i32;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' => {
                i = skip_literal(bytes, i);
                continue;
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((i + 1, i));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn read_ident(bytes: &[u8], mut i: usize) -> (String, usize) {
    let start = i;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    (String::from_utf8_lossy(&bytes[start..i]).into_owned(), i)
}

/// The keyword of the item an attribute block attaches to, or None when the
/// block is followed by something that is not an item keyword (a statement,
/// expression, macro invocation, ...).  `pub`, `pub(..)` and `unsafe` are
/// skipped before the keyword.
fn item_keyword(code: &str, mut i: usize) -> Option<String> {
    let bytes = code.as_bytes();
    loop {
        i = skip_ws(bytes, i);
        if i >= bytes.len() {
            return None;
        }
        if bytes[i] == b'#' {
            let mut j = i + 1;
            if j < bytes.len() && bytes[j] == b'!' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'[' {
                i = bracket_span(bytes, j)?;
                continue;
            }
            return None;
        }
        let (word, after) = read_ident(bytes, i);
        match word.as_str() {
            "pub" => {
                let mut j = skip_ws(bytes, after);
                if j < bytes.len() && bytes[j] == b'(' {
                    let (end, _) = paren_span(bytes, j)?;
                    j = end;
                }
                i = j;
                continue;
            }
            "" => return None,
            _ => return Some(word),
        }
    }
}

fn attr_predicate(inner: &str) -> Option<String> {
    let t = inner.trim_start();
    let rest = if let Some(r) = t.strip_prefix("cfg_attr") {
        r
    } else if let Some(r) = t.strip_prefix("cfg") {
        r
    } else {
        return None;
    };
    let rest = rest.trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let bytes = rest.as_bytes();
    let (_, close) = paren_span(bytes, 0)?;
    let body = &rest[1..close];
    if t.starts_with("cfg_attr") {
        // First top-level comma separates the predicate.
        let mut depth = 0i32;
        let b = body.as_bytes();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'"' | b'\'' => {
                    i = skip_literal(b, i);
                    continue;
                }
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b',' if depth == 0 => return Some(body[..i].to_string()),
                _ => {}
            }
            i += 1;
        }
        Some(body.to_string())
    } else {
        Some(body.to_string())
    }
}

// ---------------------------------------------------------------------------
// cfg predicate parsing.
// ---------------------------------------------------------------------------

struct Parser<'a> {
    bytes: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            bytes: s.as_bytes(),
            i: 0,
        }
    }

    fn ws(&mut self) {
        while self.i < self.bytes.len() && self.bytes[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn expr(&mut self) -> Option<Pred> {
        self.ws();
        if self.i >= self.bytes.len() {
            return None;
        }
        if self.bytes[self.i] == b'"' {
            let end = skip_normal_string(self.bytes, self.i);
            let value = String::from_utf8_lossy(&self.bytes[self.i + 1..end - 1]).into_owned();
            self.i = end;
            return Some(Pred::Feature(value));
        }
        if self.bytes[self.i] == b'\'' {
            let end = skip_char_or_lifetime(self.bytes, self.i);
            self.i = end;
            return Some(Pred::Other);
        }
        if self.bytes[self.i] == b'!' {
            self.i += 1;
            return self.expr();
        }
        if !(self.bytes[self.i].is_ascii_alphabetic() || self.bytes[self.i] == b'_') {
            // Numbers, paths, operators: consume one byte as Other.
            self.i += 1;
            return Some(Pred::Other);
        }
        let (word, after) = read_ident(self.bytes, self.i);
        self.i = after;
        self.ws();
        if self.i < self.bytes.len() && self.bytes[self.i] == b'(' {
            let (_, close) = paren_span(self.bytes, self.i)?;
            let inner = String::from_utf8_lossy(&self.bytes[self.i + 1..close]).into_owned();
            self.i = close + 1;
            let mut sub = Parser::new(&inner);
            let mut children = Vec::new();
            loop {
                sub.ws();
                if sub.i >= sub.bytes.len() {
                    break;
                }
                children.push(sub.expr()?);
                sub.ws();
                if sub.i < sub.bytes.len() && sub.bytes[sub.i] == b',' {
                    sub.i += 1;
                    continue;
                }
                break;
            }
            // Ignore trailing junk in a group rather than accepting it: a
            // group with unconsumed non-whitespace is Other, which is safe
            // only when it contains no bare test.
            sub.ws();
            if sub.i < sub.bytes.len() {
                return Some(Pred::Other);
            }
            return Some(match word.as_str() {
                "any" => Pred::Any(children),
                "all" => Pred::All(children),
                "not" if children.len() == 1 => Pred::Not(Box::new(children.remove(0))),
                _ => Pred::Other,
            });
        }
        self.ws();
        if self.i < self.bytes.len() && self.bytes[self.i] == b'=' {
            self.i += 1;
            self.ws();
            let value = self.expr()?;
            return Some(match (word.as_str(), value) {
                ("feature", Pred::Feature(f)) => Pred::Feature(f),
                _ => Pred::Other,
            });
        }
        Some(Pred::Ident(word))
    }
}

fn bad_pred(pred: &Pred) -> bool {
    match pred {
        Pred::Ident(name) => name == "test",
        Pred::Any(children) => {
            let has_feature = children
                .iter()
                .any(|c| matches!(c, Pred::Feature(f) if f == "test-support"));
            for child in children {
                if matches!(child, Pred::Ident(name) if name == "test") {
                    if !has_feature {
                        return true;
                    }
                } else if bad_pred(child) {
                    return true;
                }
            }
            false
        }
        Pred::All(children) => children.iter().any(bad_pred),
        Pred::Not(inner) => bad_pred(inner),
        Pred::Feature(_) | Pred::Other => false,
    }
}

fn contains_bare_test_textually(pred: &str) -> bool {
    // Fallback for unparseable predicates: strip string literals, then look
    // for the identifier `test`.
    let bytes = pred.as_bytes();
    let mut stripped = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            i = skip_literal(bytes, i);
            stripped.push('S');
            continue;
        }
        stripped.push(bytes[i] as char);
        i += 1;
    }
    let b = stripped.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            let (word, after) = read_ident(b, i);
            if word == "test" {
                return true;
            }
            i = after;
        } else {
            i += 1;
        }
    }
    false
}

fn predicate_is_bad(pred: &str) -> bool {
    let bytes = pred.as_bytes();
    let mut parser = Parser::new(pred);
    match parser.expr() {
        Some(parsed) => {
            parser.ws();
            if parser.i >= bytes.len() {
                bad_pred(&parsed)
            } else {
                contains_bare_test_textually(pred)
            }
        }
        None => contains_bare_test_textually(pred),
    }
}

// ---------------------------------------------------------------------------
// File scan.
// ---------------------------------------------------------------------------

fn scan_file(name: &str, text: &str, out: &mut Vec<Violation>) {
    let code = cleaned_code(text);
    let bytes = code.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"'
            || b == b'\''
            || b == b'r'
            || ((b == b'b' || b == b'c') && i + 1 < bytes.len() && bytes[i + 1] == b'r')
        {
            // Literals are opaque: `"#[cfg(test)]"` inside a string is data.
            i = skip_literal(bytes, i);
            continue;
        }
        if b == b'#' {
            let mut j = i + 1;
            let mut is_inner = false;
            if j < bytes.len() && bytes[j] == b'!' {
                is_inner = true;
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'[' {
                let Some(end) = bracket_span(bytes, j) else {
                    out.push(Violation {
                        file: name.to_string(),
                        line: line_of(&code, i),
                        text: "unterminated attribute".to_string(),
                    });
                    return;
                };
                let inner = &code[j + 1..end - 1];
                if let Some(pred) = attr_predicate(inner) {
                    // Inner attributes gate the enclosing module, not the next
                    // item, so they are never a test-module declaration.
                    let exempt = !is_inner
                        && item_keyword(&code, end)
                            .map(|k| k == "mod")
                            .unwrap_or(false);
                    if !exempt && predicate_is_bad(&pred) {
                        out.push(Violation {
                            file: name.to_string(),
                            line: line_of(&code, i),
                            text: format!("#[{}]", inner.trim()),
                        });
                    }
                }
                i = end;
                continue;
            }
        }
        // cfg!(...) macro expression.
        if bytes[i..].starts_with(b"cfg") {
            let before_ok =
                i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            let mut j = i + 3;
            if before_ok {
                j = skip_ws(bytes, j);
                if j < bytes.len() && bytes[j] == b'!' {
                    j = skip_ws(bytes, j + 1);
                    if j < bytes.len() && bytes[j] == b'(' {
                        if let Some((end, close)) = paren_span(bytes, j) {
                            let pred = &code[j + 1..close];
                            if predicate_is_bad(pred) {
                                out.push(Violation {
                                    file: name.to_string(),
                                    line: line_of(&code, i),
                                    text: format!("cfg!({})", pred.trim()),
                                });
                            }
                            i = end;
                            continue;
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

// ---------------------------------------------------------------------------
// Test.
// ---------------------------------------------------------------------------

fn resolve_source(core_src: &Path, value_src: &Path, old: &str, dest: &str) -> PathBuf {
    let pre = core_src.join(old);
    let post = value_src.join(dest);
    match (pre.exists(), post.exists()) {
        (true, false) => pre,
        (false, true) => post,
        (true, true) => panic!(
            "moved production file exists in both trees: {} and {}",
            pre.display(),
            post.display()
        ),
        (false, false) => {
            panic!("moved production file missing in both trees: {old} / {dest}")
        }
    }
}

fn scan_str(src: &str) -> Vec<Violation> {
    let mut out = Vec::new();
    scan_file("synthetic", src, &mut out);
    out
}

#[test]
fn architecture_cfg_test_guard_self_test() {
    let good = [
        "#[cfg(test)]\nmod tests;\n",
        "#[cfg(test)]\n#[path = \"t.rs\"]\npub(crate) mod tests;\n",
        "#[cfg(test)]\nmod tests { fn t() {} }\n",
        "#[cfg(any(test, feature = \"test-support\"))]\nfn f() {}\n",
        "#[cfg(not(any(test, feature = \"test-support\")))]\nfn f() {}\n",
        "#[cfg(all(any(test, feature = \"test-support\"), debug_assertions))]\nfn f() {}\n",
        "fn f() { if cfg!(any(test, feature = \"test-support\")) {} }\n",
        "// #[cfg(test)] fn f() {}\nfn g() {}\n",
        "fn f() { let s = \"#[cfg(test)]\"; }\n",
        "/* #[cfg(test)] fn f() {} */\nfn g() {}\n",
        "macro_rules! m { () => { #[cfg(test)] mod inner; } }\n",
        "fn f() { let s = cfg!(feature = \"x\"); }\n",
    ];
    for src in good {
        let found = scan_str(src);
        assert!(found.is_empty(), "false positive for {src:?}: {found:?}");
    }
    let bad = [
        "#[cfg(test)]\nfn f() {}\n",
        "#[cfg(test)]\nstruct S;\n",
        "#[cfg(not(test))]\nfn f() {}\n",
        "#[cfg(all(test, debug_assertions))]\nfn f() {}\n",
        "#[cfg(all(test, feature = \"test-support\"))]\nfn f() {}\n",
        "#[cfg_attr(test, allow(dead_code))]\nfn f() {}\n",
        "#[cfg_attr(not(test), allow(dead_code))]\nfn f() {}\n",
        "fn f() { if cfg!(test) {} }\n",
        "#[cfg(test)]\nuse std::fmt;\n",
        "#[cfg(test)]\nlet x = 1;\n",
        "#![cfg(test)]\nmod tests;\n",
        "#[cfg(all(\n    test,\n    debug_assertions,\n))]\nfn f() {}\n",
        "fn f() { #[cfg(test)] { } }\n",
        "#[cfg(all(test, any(feature = \"test-support\", feature = \"x\")))]\nfn f() {}\n",
    ];
    for src in bad {
        let found = scan_str(src);
        assert!(
            !found.is_empty(),
            "false negative (guard would pass) for {src:?}"
        );
    }
}

#[test]
fn architecture_moved_production_has_no_bare_cfg_test() {
    let core_src = production_source_root("neovm-core").src;
    let value_src = production_source_root("neovm-value").src;
    let mut rows = 0usize;
    let mut violations = Vec::new();
    for line in MANIFEST.lines() {
        if line.starts_with('#') || line.starts_with("old_rel") || line.is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let (Some(old), Some(dest)) = (parts.next(), parts.next()) else {
            panic!("bad moved-production manifest row: {line:?}");
        };
        rows += 1;
        let path = resolve_source(&core_src, &value_src, old, dest);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        scan_file(old, &text, &mut violations);
    }
    assert_eq!(
        rows, EXPECTED_FILES,
        "moved-production manifest must list {EXPECTED_FILES} files"
    );
    if !violations.is_empty() {
        let mut msg = format!(
            "{} moved production files still contain a bare `cfg(test)` outside a \
             test-module declaration (peer R4 conversion is vmcall's; this guard \
             goes green when it lands). {} violations:\n",
            rows,
            violations.len()
        );
        for v in violations.iter().take(80) {
            msg.push_str(&format!("  {}:{}: {}\n", v.file, v.line, v.text));
        }
        if violations.len() > 80 {
            msg.push_str(&format!("  ... {} more\n", violations.len() - 80));
        }
        panic!("{msg}");
    }
}
