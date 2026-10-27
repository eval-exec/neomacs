//! Source-level guard coverage for every embedded oracle form.
//!
//! `OracleSandbox::new` validates a form before any mode dispatch, so every
//! test that reaches an `assert_oracle_*` entry point is checked in snapshot,
//! verify, live and refresh runs alike.  The gap that let 50 malformed forms
//! through to CI was *execution* coverage: the forms lived in the crate's lib
//! target, which a `--test oracle` qualification never runs, so no mode ever
//! saw them.
//!
//! This test closes that gap from the other side: it walks the module graph
//! reachable from `src/lib.rs` and `tests/*.rs`, extracts every string literal
//! passed to an `assert_oracle_*` call (directly or through the `form`
//! variable the call consumes), and runs the reader guard on it.  It needs no
//! Emacs, no Neomacs binary, and no test execution, so a malformed form is
//! caught even in an ignored, env-gated, or never-run test.  A new test cannot
//! bypass it: the sweep discovers the literal from the source text.
//!
//! Only literals are checked here.  Forms assembled at run time (`format!`,
//! `concat!`) cannot be validated without executing their test; those are
//! covered by the full-package snapshot and verify qualification.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::common::oracle_sandbox::validate_form_source;

/// A string literal found in Rust source.
struct Literal {
    /// Byte offset of the opening delimiter (`r`, `b`, or `"`).
    start: usize,
    /// Byte range of the literal's decoded content in the source.
    content_start: usize,
    content_end: usize,
    /// Number of `#`s in a raw string; 0 for a plain string.
    hashes: usize,
}

/// Byte-level scanner for Rust source.
///
/// It is deliberately small: it only has to find string literals, comments,
/// and the code positions in between.  `code[i]` is false inside comments and
/// string/char literals.
struct Lexer<'a> {
    src: &'a [u8],
    code: Vec<bool>,
    literals: Vec<Literal>,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer {
            src: src.as_bytes(),
            code: vec![true; src.len()],
            literals: Vec::new(),
        }
    }

    fn mark(&mut self, start: usize, end: usize) {
        for slot in &mut self.code[start..end] {
            *slot = false;
        }
    }

    fn run(mut self) -> (Vec<bool>, Vec<Literal>) {
        let n = self.src.len();
        let mut i = 0;
        while i < n {
            match self.src[i] {
                b'/' if i + 1 < n && self.src[i + 1] == b'/' => {
                    let start = i;
                    while i < n && self.src[i] != b'\n' {
                        i += 1;
                    }
                    self.mark(start, i);
                }
                b'/' if i + 1 < n && self.src[i + 1] == b'*' => {
                    let start = i;
                    let mut depth = 1;
                    i += 2;
                    while i < n && depth > 0 {
                        if self.src[i] == b'/' && i + 1 < n && self.src[i + 1] == b'*' {
                            depth += 1;
                            i += 2;
                        } else if self.src[i] == b'*' && i + 1 < n && self.src[i + 1] == b'/' {
                            depth -= 1;
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    self.mark(start, i);
                }
                b'\'' => {
                    // A char literal or a lifetime.  Lifetimes (`'a`) have no
                    // closing quote on the next one or two bytes.
                    let is_char = i + 1 < n
                        && (self.src[i + 1] == b'\\' || (i + 2 < n && self.src[i + 2] == b'\''));
                    if is_char {
                        let start = i;
                        i += 1;
                        while i < n && self.src[i] != b'\'' {
                            if self.src[i] == b'\\' {
                                i += 1;
                            }
                            i += 1;
                        }
                        i = (i + 1).min(n);
                        self.mark(start, i);
                    } else {
                        i += 1;
                    }
                }
                b'"' => {
                    let start = i;
                    i += 1;
                    while i < n {
                        match self.src[i] {
                            b'\\' => i += 2,
                            b'"' => {
                                i += 1;
                                break;
                            }
                            _ => i += 1,
                        }
                    }
                    let literal = Literal {
                        start,
                        content_start: start + 1,
                        content_end: (i - 1).min(n),
                        hashes: 0,
                    };
                    self.literals.push(literal);
                    self.mark(start, i.min(n));
                }
                b'r' if self.raw_opener(i).is_some() => {
                    let (quote, hashes) = self.raw_opener(i).expect("checked");
                    let start = i;
                    let content_start = quote + 1;
                    let mut j = content_start;
                    let mut end = n;
                    while j < n {
                        if self.src[j] == b'"' && self.hashes_at(j + 1) >= hashes {
                            end = j + 1 + hashes;
                            break;
                        }
                        j += 1;
                    }
                    let literal = Literal {
                        start,
                        content_start,
                        content_end: j.min(n),
                        hashes,
                    };
                    self.literals.push(literal);
                    self.mark(start, end);
                    i = end;
                }
                _ => i += 1,
            }
        }
        (self.code, self.literals)
    }

    /// Offset of the opening quote when a raw string starts at `i`.
    fn raw_opener(&self, i: usize) -> Option<(usize, usize)> {
        let n = self.src.len();
        if i >= n || self.src[i] != b'r' {
            return None;
        }
        let mut j = i + 1;
        while j < n && self.src[j] == b'#' {
            j += 1;
        }
        if j < n && self.src[j] == b'"' {
            Some((j, j - i - 1))
        } else {
            None
        }
    }

    fn hashes_at(&self, mut i: usize) -> usize {
        let mut k = 0;
        while i < self.src.len() && self.src[i] == b'#' {
            k += 1;
            i += 1;
        }
        k
    }
}

/// Decode a plain Rust string literal body (without surrounding quotes).
fn decode_plain(body: &str) -> String {
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c != b'\\' {
            out.push(c as char);
            i += 1;
            continue;
        }
        i += 1;
        if i >= bytes.len() {
            break;
        }
        match bytes[i] {
            b'\n' => {
                // Continuation: drop the newline and the next line's indent.
                i += 1;
                while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                    i += 1;
                }
            }
            b'\r' if i + 1 < bytes.len() && bytes[i + 1] == b'\n' => {
                i += 2;
                while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                    i += 1;
                }
            }
            b'n' => {
                out.push('\n');
                i += 1;
            }
            b't' => {
                out.push('\t');
                i += 1;
            }
            b'r' => {
                out.push('\r');
                i += 1;
            }
            b'0' => {
                out.push('\0');
                i += 1;
            }
            b'\\' => {
                out.push('\\');
                i += 1;
            }
            b'"' => {
                out.push('"');
                i += 1;
            }
            b'\'' => {
                out.push('\'');
                i += 1;
            }
            b'x' if i + 2 < bytes.len() => {
                let hex = &body[i + 1..i + 3];
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v as char);
                }
                i += 3;
            }
            b'u' if i + 1 < bytes.len() && bytes[i + 1] == b'{' => {
                if let Some(close) = body[i + 2..].find('}') {
                    if let Ok(v) = u32::from_str_radix(&body[i + 2..i + 2 + close], 16) {
                        if let Some(ch) = char::from_u32(v) {
                            out.push(ch);
                        }
                    }
                    i += 2 + close + 1;
                } else {
                    i += 1;
                }
            }
            other => {
                out.push('\\');
                out.push(other as char);
                i += 1;
            }
        }
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Read the identifier starting at `i`, returning it and the next offset.
fn ident_at(src: &[u8], mut i: usize) -> (String, usize) {
    let start = i;
    while i < src.len() && is_ident_byte(src[i]) {
        i += 1;
    }
    (String::from_utf8_lossy(&src[start..i]).into_owned(), i)
}

fn skip_ws(src: &[u8], mut i: usize) -> usize {
    while i < src.len() && (src[i] as char).is_whitespace() {
        i += 1;
    }
    i
}

/// Whether the first non-space character opens a top-level Lisp form.
fn looks_like_form(content: &str) -> bool {
    let s = content.trim_start();
    s.starts_with('(') || s.starts_with(';')
}

fn line_of(src: &[u8], offset: usize) -> usize {
    1 + src[..offset.min(src.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// Collect the string literals that are used as oracle form sources.
fn form_candidates(src: &str) -> Vec<(usize, String)> {
    let bytes = src.as_bytes();
    let (code, literals) = Lexer::new(src).run();

    // Literal start offsets, for direct-argument detection.
    let mut literal_at: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (idx, lit) in literals.iter().enumerate() {
        literal_at.insert(lit.start, idx);
    }

    // First-argument identifiers of every assert_oracle_* call.
    let mut first_arg_idents: HashSet<String> = HashSet::new();
    let mut direct: HashSet<usize> = HashSet::new();
    let needle = b"assert_oracle_";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if code[i] && bytes[i..].starts_with(needle) {
            let mut j = i + needle.len();
            // The identifier continues while it is lowercase letters/underscore.
            while j < bytes.len()
                && (bytes[j].is_ascii_lowercase() || bytes[j] == b'_' || bytes[j].is_ascii_digit())
            {
                j += 1;
            }
            j = skip_ws(bytes, j);
            if j < bytes.len() && bytes[j] == b'(' {
                j = skip_ws(bytes, j + 1);
                if j < bytes.len() && bytes[j] == b'&' {
                    j = skip_ws(bytes, j + 1);
                }
                if let Some(&idx) = literal_at.get(&j) {
                    direct.insert(idx);
                } else if j < bytes.len() && is_ident_byte(bytes[j]) {
                    let (name, _) = ident_at(bytes, j);
                    first_arg_idents.insert(name);
                }
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }

    // Literals bound as `let <name> = <literal>` where <name> is consumed as
    // the form argument of an assert_oracle_* call.
    let mut candidates = Vec::new();
    for (idx, lit) in literals.iter().enumerate() {
        let content = if lit.hashes == 0 {
            decode_plain(&src[lit.content_start..lit.content_end])
        } else {
            src[lit.content_start..lit.content_end].to_string()
        };
        if !looks_like_form(&content) {
            continue;
        }
        let is_candidate = if direct.contains(&idx) {
            true
        } else {
            // Walk back over whitespace to `=`, then the bound name.
            let mut k = lit.start;
            while k > 0 && (bytes[k - 1] as char).is_whitespace() {
                k -= 1;
            }
            if k > 0 && bytes[k - 1] == b'=' && code[k - 2.min(k - 1)] {
                let mut n = k - 1;
                while n > 0 && (bytes[n - 1] as char).is_whitespace() {
                    n -= 1;
                }
                let mut m = n;
                while m > 0 && is_ident_byte(bytes[m - 1]) {
                    m -= 1;
                }
                let name = String::from_utf8_lossy(&bytes[m..n]).into_owned();
                let mut p = m;
                while p > 0 && (bytes[p - 1] as char).is_whitespace() {
                    p -= 1;
                }
                let is_let = p >= 3 && &bytes[p - 3..p] == b"let" && {
                    let before = p - 3;
                    before == 0 || !is_ident_byte(bytes[before - 1])
                };
                is_let && !name.is_empty() && first_arg_idents.contains(&name)
            } else {
                false
            }
        };
        if is_candidate {
            candidates.push((line_of(bytes, lit.start), content));
        }
    }
    candidates
}

/// Resolve a `mod NAME;` declaration to a file.
fn resolve_module(containing: &Path, name: &str) -> Option<PathBuf> {
    let dir = containing.parent()?;
    let stem = containing.file_stem()?.to_string_lossy().into_owned();
    let child_dir = if stem == "lib" || stem == "mod" || stem == "main" {
        dir.to_path_buf()
    } else {
        dir.join(&stem)
    };
    for candidate in [
        child_dir.join(format!("{name}.rs")),
        child_dir.join(name).join("mod.rs"),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Every `mod NAME;` and `#[path = "..."] mod NAME;` child of one file.
fn child_modules(path: &Path, src: &str) -> Vec<PathBuf> {
    let bytes = src.as_bytes();
    let (code, _) = Lexer::new(src).run();
    let mut out = Vec::new();
    // #[path = "..."] mod name;
    let path_needle = b"#[path";
    let mut i = 0;
    while i + path_needle.len() <= bytes.len() {
        if code[i] && bytes[i..].starts_with(path_needle) {
            if let Some(eq) = src[i..].find('=') {
                let after = i + eq + 1;
                let j = skip_ws(bytes, after);
                if j < bytes.len() && bytes[j] == b'"' {
                    let start = j + 1;
                    if let Some(close) = src[start..].find('"') {
                        let rel = &src[start..start + close];
                        let target = path.parent().unwrap_or(Path::new(".")).join(rel);
                        out.push(target);
                    }
                }
            }
        }
        i += 1;
    }
    // Plain `mod name;` declarations.
    let mut i = 0;
    while i < bytes.len() {
        if code[i] && bytes[i..].starts_with(b"mod") {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let mut j = i + 3;
            let after_ok = j >= bytes.len() || !is_ident_byte(bytes[j]);
            if before_ok && after_ok {
                j = skip_ws(bytes, j);
                if j < bytes.len() && is_ident_byte(bytes[j]) {
                    let (name, k) = ident_at(bytes, j);
                    let k = skip_ws(bytes, k);
                    if k < bytes.len() && bytes[k] == b';' && code[k] {
                        if let Some(p) = resolve_module(path, &name) {
                            out.push(p);
                        }
                    }
                }
            }
        }
        i += 1;
    }
    out
}

fn collect_reachable(root: &Path, seen: &mut HashSet<PathBuf>, out: &mut Vec<PathBuf>) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let path = path.canonicalize().unwrap_or(path);
        if !seen.insert(path.clone()) {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        out.push(path.clone());
        stack.extend(child_modules(&path, &src));
    }
}

#[test]
fn every_compiled_oracle_form_source_is_complete() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_root = manifest.join("src/lib.rs");
    assert!(lib_root.is_file(), "missing {}", lib_root.display());

    let mut roots = vec![lib_root];
    let tests_dir = manifest.join("tests");
    let mut integration: Vec<PathBuf> = std::fs::read_dir(&tests_dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", tests_dir.display()))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    integration.sort();
    roots.extend(integration);

    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for root in &roots {
        collect_reachable(root, &mut seen, &mut files);
    }

    let mut checked = 0usize;
    let mut failures = Vec::new();
    for path in &files {
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        for (line, content) in form_candidates(&src) {
            checked += 1;
            if let Err(err) = validate_form_source(&content) {
                let rel = path.strip_prefix(manifest).unwrap_or(path);
                failures.push(format!("{}:{line}: {err}", rel.display()));
            }
        }
    }

    // A parser regression must not turn this test green by finding nothing.
    assert!(
        files.len() > 3000,
        "module graph too small: {} files (parser regression?)",
        files.len()
    );
    assert!(
        checked > 30_000,
        "only {checked} embedded forms found (parser regression?)"
    );
    assert!(
        failures.is_empty(),
        "{} malformed oracle form source(s) found without running Emacs:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The extractor must keep finding planted forms: one malformed literal is
/// rejected, a complete one is accepted, and a direct literal argument is
/// discovered as well as a `form` binding.
#[test]
fn sweep_detects_a_planted_malformed_form() {
    let bad = r##"
#[test]
fn bad() {
    let form = r#"(list 1"#;
    assert_oracle_parity_expect(form, expect);
}
"##;
    let found = form_candidates(bad);
    assert_eq!(found.len(), 1, "expected one form candidate, got {found:?}");
    assert!(
        validate_form_source(&found[0].1).is_err(),
        "the planted malformed form must be rejected"
    );

    let good = r##"
#[test]
fn good() {
    let form = r#"(list 1 2)"#;
    assert_oracle_parity_expect(form, expect);
    assert_oracle_parity_expect(r#"(list 3)"#, expect);
}
"##;
    let found = form_candidates(good);
    assert_eq!(
        found.len(),
        2,
        "expected two form candidates, got {found:?}"
    );
    for (_, content) in &found {
        assert!(
            validate_form_source(content).is_ok(),
            "complete form rejected: {content:?}"
        );
    }
}

/// The reader guard runs at `OracleSandbox::new`, before any `OracleMode`
/// dispatch, so it cannot be skipped by a snapshot/verify/live/refresh gate.
#[test]
fn oracle_sandbox_rejects_malformed_forms_without_any_mode() {
    for form in ["(list 1", "(list 1))", "(progn (list 1)"] {
        match crate::common::oracle_sandbox::OracleSandbox::new(form, &[], Path::new(".")) {
            Err(err) => assert!(
                err.contains("oracle form source rejected"),
                "unexpected error for {form:?}: {err}"
            ),
            Ok(_) => panic!("the guard must reject an incomplete form: {form:?}"),
        }
    }
}
