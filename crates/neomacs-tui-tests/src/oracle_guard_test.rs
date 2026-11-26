//! Fail-closed regression guard enforcing that `crates/neomacs-tui-tests` never
//! spawns GNU Emacs via a bare `"emacs"` PATH lookup or the unattested
//! `NEOMACS_GNU_EMACS_BINARY` environment variable (#845).
//!
//! Every GNU Emacs invocation in `crates/neomacs-tui-tests` must resolve
//! through [`crate::oracle::gnu_oracle_program`] (directly or via
//! [`crate::TuiSession::gnu_emacs*`]). This guard scans every `.rs` file under
//! `crates/neomacs-tui-tests/` with a fail-closed Rust string-literal lexer so
//! bypasses via renamed imports, helper functions, `const`s, `macro_rules!`,
//! raw strings, escape sequences, `concat!`, or out-of-tree `#[path]` modules
//! are caught regardless of AST nesting.

use std::path::{Component, Path, PathBuf};

const SELF_RELATIVE_PATH: &str = "crates/neomacs-tui-tests/src/oracle_guard_test.rs";

const FORBIDDEN_LITERALS: &[(&str, &str)] = &[
    (
        "emacs",
        "bare \"emacs\" literal bypasses oracle::gnu_oracle_program() attestation",
    ),
    (
        "NEOMACS_GNU_EMACS_BINARY",
        "legacy unattested env var bypasses oracle::gnu_oracle_program() attestation",
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Finding {
    relative_path: String,
    line: usize,
    reason: String,
}

impl Finding {
    fn render(&self) -> String {
        format!("{}:{}: {}", self.relative_path, self.line, self.reason)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Ident(String),
    StringLit(String),
    Punct(char),
}

#[derive(Debug, Clone)]
struct Token {
    line: usize,
    kind: TokenKind,
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Fail-closed Rust lexer extracting identifiers, punctuation, and fully
/// decoded string literals (normal, byte, C-string, and raw strings) while
/// skipping line comments, nested block comments, and char/lifetime tokens.
fn lex_rust(source: &str) -> Result<Vec<Token>, String> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut tokens = Vec::new();

    while i < len {
        let b = bytes[i];
        if b == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Line comment.
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            i += 2;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Nested block comment.
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
            let start_line = line;
            i += 2;
            let mut depth = 1usize;
            while i < len && depth > 0 {
                if bytes[i] == b'\n' {
                    line += 1;
                    i += 1;
                } else if bytes[i] == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if depth != 0 {
                return Err(format!(
                    "unterminated block comment starting at line {start_line}"
                ));
            }
            continue;
        }

        // Raw string literal: r"...", r#"..."#, br#"..."#, cr#"..."#.
        if let Some((prefix_len, hashes)) = detect_raw_string(bytes, i) {
            let start_line = line;
            i += prefix_len + hashes + 1; // skip prefix, leading '#', and opening '"'
            let content_start = i;
            let mut found_end = None;
            while i < len {
                if bytes[i] == b'\n' {
                    line += 1;
                    i += 1;
                    continue;
                }
                if bytes[i] == b'"'
                    && i + hashes < len
                    && bytes[i + 1..=i + hashes].iter().all(|c| *c == b'#')
                {
                    found_end = Some(i);
                    i += 1 + hashes;
                    break;
                }
                i += 1;
            }
            let Some(content_end) = found_end else {
                return Err(format!(
                    "unterminated raw string literal starting at line {start_line}"
                ));
            };
            let decoded = String::from_utf8_lossy(&bytes[content_start..content_end]).into_owned();
            tokens.push(Token {
                line: start_line,
                kind: TokenKind::StringLit(decoded),
            });
            continue;
        }

        // Byte char literal `b'...'` or byte/c string literal `b"..."` / `c"..."`.
        if (b == b'b' || b == b'c') && i + 1 < len && bytes[i + 1] == b'"' {
            let start_line = line;
            let (decoded, next_i, next_line) = lex_quoted_string(source, i + 1, line)?;
            i = next_i;
            line = next_line;
            tokens.push(Token {
                line: start_line,
                kind: TokenKind::StringLit(decoded),
            });
            continue;
        }
        if b == b'b' && i + 1 < len && bytes[i + 1] == b'\'' {
            let (next_i, next_line) = skip_char_literal(source, i + 1, line)?;
            i = next_i;
            line = next_line;
            continue;
        }

        // Normal string literal `"..."`.
        if b == b'"' {
            let start_line = line;
            let (decoded, next_i, next_line) = lex_quoted_string(source, i, line)?;
            i = next_i;
            line = next_line;
            tokens.push(Token {
                line: start_line,
                kind: TokenKind::StringLit(decoded),
            });
            continue;
        }

        // Char literal vs lifetime.
        if b == b'\'' {
            if let Some((next_i, next_line)) = try_skip_char_literal(source, i, line)? {
                i = next_i;
                line = next_line;
            } else {
                i += 1;
            }
            continue;
        }

        // Identifier (including raw identifier `r#ident`).
        if b == b'r' && i + 2 < len && bytes[i + 1] == b'#' && is_ident_start(bytes[i + 2]) {
            let start_line = line;
            i += 2;
            let start = i;
            while i < len && is_ident_continue(bytes[i]) {
                i += 1;
            }
            tokens.push(Token {
                line: start_line,
                kind: TokenKind::Ident(source[start..i].to_owned()),
            });
            continue;
        }
        if is_ident_start(b) {
            let start_line = line;
            let start = i;
            while i < len && is_ident_continue(bytes[i]) {
                i += 1;
            }
            tokens.push(Token {
                line: start_line,
                kind: TokenKind::Ident(source[start..i].to_owned()),
            });
            continue;
        }

        if b.is_ascii_punctuation() {
            tokens.push(Token {
                line,
                kind: TokenKind::Punct(b as char),
            });
            i += 1;
            continue;
        }

        // Advance one UTF-8 scalar value.
        let ch = source[i..]
            .chars()
            .next()
            .ok_or_else(|| "invalid UTF-8".to_string())?;
        i += ch.len_utf8();
    }

    Ok(tokens)
}

fn detect_raw_string(bytes: &[u8], i: usize) -> Option<(usize, usize)> {
    let prefix_len = if bytes[i] == b'r' {
        1
    } else if (bytes[i] == b'b' || bytes[i] == b'c') && i + 1 < bytes.len() && bytes[i + 1] == b'r'
    {
        2
    } else {
        return None;
    };
    let mut j = i + prefix_len;
    let mut hashes = 0usize;
    while j < bytes.len() && bytes[j] == b'#' {
        hashes += 1;
        j += 1;
    }
    if j < bytes.len() && bytes[j] == b'"' {
        Some((prefix_len, hashes))
    } else {
        None
    }
}

fn try_skip_char_literal(
    source: &str,
    quote_idx: usize,
    line: usize,
) -> Result<Option<(usize, usize)>, String> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    if quote_idx + 1 >= len {
        return Ok(None);
    }
    if bytes[quote_idx + 1] == b'\\' {
        let (next_i, next_line) = skip_char_literal(source, quote_idx, line)?;
        return Ok(Some((next_i, next_line)));
    }
    let ch = source[quote_idx + 1..]
        .chars()
        .next()
        .ok_or_else(|| format!("unterminated char literal at line {line}"))?;
    let after = quote_idx + 1 + ch.len_utf8();
    if after < len && bytes[after] == b'\'' {
        let next_line = if ch == '\n' { line + 1 } else { line };
        Ok(Some((after + 1, next_line)))
    } else {
        // Lifetime token such as `'static` or `'pump`.
        Ok(None)
    }
}

fn skip_char_literal(
    source: &str,
    quote_idx: usize,
    line: usize,
) -> Result<(usize, usize), String> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = quote_idx + 1;
    if i >= len {
        return Err(format!("unterminated char literal at line {line}"));
    }
    if bytes[i] == b'\\' {
        let (_, next_i, next_line) = decode_escape(source, i, line)?;
        i = next_i;
        if i >= len || bytes[i] != b'\'' {
            return Err(format!("unterminated escaped char literal at line {line}"));
        }
        Ok((i + 1, next_line))
    } else {
        let ch = source[i..]
            .chars()
            .next()
            .ok_or_else(|| format!("unterminated char literal at line {line}"))?;
        let after = i + ch.len_utf8();
        if after >= len || bytes[after] != b'\'' {
            return Err(format!("unterminated char literal at line {line}"));
        }
        Ok((after + 1, if ch == '\n' { line + 1 } else { line }))
    }
}

fn lex_quoted_string(
    source: &str,
    quote_idx: usize,
    mut line: usize,
) -> Result<(String, usize, usize), String> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let start_line = line;
    let mut i = quote_idx + 1;
    let mut out = String::new();
    while i < len {
        let b = bytes[i];
        if b == b'"' {
            return Ok((out, i + 1, line));
        }
        if b == b'\\' {
            let (decoded, next_i, next_line) = decode_escape(source, i, line)?;
            if let Some(ch) = decoded {
                out.push(ch);
            }
            i = next_i;
            line = next_line;
            continue;
        }
        if b == b'\n' {
            line += 1;
        }
        let ch = source[i..]
            .chars()
            .next()
            .ok_or_else(|| format!("invalid UTF-8 in string literal at line {start_line}"))?;
        out.push(ch);
        i += ch.len_utf8();
    }
    Err(format!(
        "unterminated string literal starting at line {start_line}"
    ))
}

fn decode_escape(
    source: &str,
    backslash_idx: usize,
    mut line: usize,
) -> Result<(Option<char>, usize, usize), String> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = backslash_idx + 1;
    if i >= len {
        return Err(format!("unterminated escape sequence at line {line}"));
    }
    match bytes[i] {
        b'\\' => Ok((Some('\\'), i + 1, line)),
        b'"' => Ok((Some('"'), i + 1, line)),
        b'\'' => Ok((Some('\''), i + 1, line)),
        b'n' => Ok((Some('\n'), i + 1, line)),
        b'r' => Ok((Some('\r'), i + 1, line)),
        b't' => Ok((Some('\t'), i + 1, line)),
        b'0' => Ok((Some('\0'), i + 1, line)),
        b'\n' | b'\r' => {
            if bytes[i] == b'\r' && i + 1 < len && bytes[i + 1] == b'\n' {
                i += 2;
            } else {
                i += 1;
            }
            line += 1;
            while i < len && matches!(bytes[i], b' ' | b'\t') {
                i += 1;
            }
            Ok((None, i, line))
        }
        b'x' => {
            if i + 2 >= len {
                return Err(format!("truncated \\x escape at line {line}"));
            }
            let hex = &source[i + 1..i + 3];
            let value = u8::from_str_radix(hex, 16)
                .map_err(|_| format!("invalid \\x escape {hex:?} at line {line}"))?;
            Ok((Some(value as char), i + 3, line))
        }
        b'u' => {
            if i + 1 >= len || bytes[i + 1] != b'{' {
                return Err(format!("malformed \\u escape at line {line}"));
            }
            let close = source[i + 2..]
                .find('}')
                .map(|off| i + 2 + off)
                .ok_or_else(|| format!("unterminated \\u{{...}} escape at line {line}"))?;
            let hex: String = source[i + 2..close].chars().filter(|c| *c != '_').collect();
            let code = u32::from_str_radix(&hex, 16)
                .map_err(|_| format!("invalid \\u escape {hex:?} at line {line}"))?;
            let ch = char::from_u32(code)
                .ok_or_else(|| format!("invalid Unicode scalar \\u{{{hex}}} at line {line}"))?;
            Ok((Some(ch), close + 1, line))
        }
        other => Err(format!(
            "unsupported escape \\{} at line {line}",
            other as char
        )),
    }
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    normalized
}

fn check_forbidden_literal(
    relative_path: &str,
    line: usize,
    value: &str,
    findings: &mut Vec<Finding>,
) {
    for (needle, reason) in FORBIDDEN_LITERALS {
        if value == *needle {
            findings.push(Finding {
                relative_path: relative_path.to_owned(),
                line,
                reason: (*reason).to_owned(),
            });
        }
    }
}

fn scan_source(
    crate_root: &Path,
    file_path: &Path,
    relative_path: &str,
    source: &str,
) -> Result<Vec<Finding>, String> {
    let tokens = lex_rust(source).map_err(|err| format!("{relative_path}: {err}"))?;
    let mut findings = Vec::new();
    let crate_root_norm = normalize_lexical(crate_root);
    let file_dir = file_path.parent().unwrap_or(crate_root);

    let mut idx = 0usize;
    let mut attr_bracket_depth = 0usize;
    while idx < tokens.len() {
        if tokens[idx].kind == TokenKind::Punct('#')
            && idx + 1 < tokens.len()
            && tokens[idx + 1].kind == TokenKind::Punct('[')
        {
            attr_bracket_depth += 1;
            idx += 2;
            continue;
        }
        if attr_bracket_depth > 0 {
            match tokens[idx].kind {
                TokenKind::Punct('[') => attr_bracket_depth += 1,
                TokenKind::Punct(']') => attr_bracket_depth -= 1,
                _ => {}
            }
        }
        match &tokens[idx].kind {
            TokenKind::StringLit(value) => {
                check_forbidden_literal(relative_path, tokens[idx].line, value, &mut findings);
            }
            // Catch `concat!("em", "acs")` and similar literal concatenation.
            TokenKind::Ident(ident)
                if ident == "concat"
                    && idx + 2 < tokens.len()
                    && tokens[idx + 1].kind == TokenKind::Punct('!')
                    && matches!(tokens[idx + 2].kind, TokenKind::Punct('(' | '[' | '{')) =>
            {
                let open = match tokens[idx + 2].kind {
                    TokenKind::Punct('(') => ')',
                    TokenKind::Punct('[') => ']',
                    TokenKind::Punct('{') => '}',
                    _ => unreachable!(),
                };
                let start_line = tokens[idx].line;
                let mut j = idx + 3;
                let mut combined = String::new();
                let mut only_strings = true;
                while j < tokens.len() && tokens[j].kind != TokenKind::Punct(open) {
                    match &tokens[j].kind {
                        TokenKind::StringLit(part) => combined.push_str(part),
                        TokenKind::Punct(',') => {}
                        _ => only_strings = false,
                    }
                    j += 1;
                }
                if only_strings && !combined.is_empty() {
                    check_forbidden_literal(relative_path, start_line, &combined, &mut findings);
                }
            }
            // Fail closed on `#[path = "..."]` or `#[cfg_attr(..., path = "...")]`
            // pointing outside `crates/neomacs-tui-tests/`.
            TokenKind::Ident(ident)
                if attr_bracket_depth > 0
                    && ident == "path"
                    && idx + 2 < tokens.len()
                    && tokens[idx + 1].kind == TokenKind::Punct('=') =>
            {
                if let TokenKind::StringLit(target) = &tokens[idx + 2].kind {
                    let resolved = normalize_lexical(&file_dir.join(target));
                    if !resolved.starts_with(&crate_root_norm) {
                        findings.push(Finding {
                            relative_path: relative_path.to_owned(),
                            line: tokens[idx].line,
                            reason: format!(
                                "#[path = {target:?}] escapes scanned crate root {}",
                                crate_root_norm.display()
                            ),
                        });
                    }
                }
            }
            // Fail closed on `include!("...")` pointing outside `crates/neomacs-tui-tests/`.
            TokenKind::Ident(ident)
                if ident == "include"
                    && idx + 3 < tokens.len()
                    && tokens[idx + 1].kind == TokenKind::Punct('!')
                    && matches!(tokens[idx + 2].kind, TokenKind::Punct('(' | '[' | '{')) =>
            {
                if let TokenKind::StringLit(target) = &tokens[idx + 3].kind {
                    let resolved = normalize_lexical(&file_dir.join(target));
                    if !resolved.starts_with(&crate_root_norm) {
                        findings.push(Finding {
                            relative_path: relative_path.to_owned(),
                            line: tokens[idx].line,
                            reason: format!(
                                "include!({target:?}) escapes scanned crate root {}",
                                crate_root_norm.display()
                            ),
                        });
                    }
                }
            }
            _ => {}
        }
        idx += 1;
    }

    Ok(findings)
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to scan directory {}: {error}", dir.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|error| panic!("failed to read entry in {}: {error}", dir.display()))
            .path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if matches!(name, "target" | ".git" | "tmp" | "node_modules") {
                continue;
            }
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn tui_tests_never_use_bare_emacs_or_unattested_oracle_env() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = crate_root
        .ancestors()
        .nth(2)
        .expect("crate lives at <repo>/crates/neomacs-tui-tests");

    let mut files = Vec::new();
    for subdir in ["src", "tests"] {
        collect_rs_files(&crate_root.join(subdir), &mut files);
    }
    files.sort();
    assert!(
        !files.is_empty(),
        "oracle guard found zero Rust files under {}",
        crate_root.display()
    );

    let mut violations = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(repo_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if relative == SELF_RELATIVE_PATH {
            continue;
        }
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("oracle guard failed to read {relative}: {error}"));
        let findings = scan_source(crate_root, &path, &relative, &source)
            .unwrap_or_else(|error| panic!("oracle guard failed closed on {error}"));
        violations.extend(findings);
    }

    assert!(
        violations.is_empty(),
        "found {} forbidden bare-\"emacs\" / unattested-oracle reference(s) in crates/neomacs-tui-tests \
         (every GNU Emacs invocation must use neomacs_tui_tests::oracle::gnu_oracle_program()):\n{}",
        violations.len(),
        violations
            .iter()
            .map(Finding::render)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn oracle_guard_rejects_every_bypass_shape_and_fails_closed() {
    let crate_root = Path::new("/repo/crates/neomacs-tui-tests");
    let sample_file = Path::new("/repo/crates/neomacs-tui-tests/tests/sample.rs");

    let forbidden_cases = [
        ("direct PathBuf::from", r#"let p = PathBuf::from("emacs");"#),
        ("direct Command::new", r#"let c = Command::new("emacs");"#),
        (
            "direct TuiLaunch::new",
            r#"let l = TuiLaunch::new("emacs");"#,
        ),
        ("OsStr::new", r#"let o = std::ffi::OsStr::new("emacs");"#),
        ("string .into()", r#"let p: PathBuf = "emacs".into();"#),
        (
            "renamed import",
            r#"use std::path::PathBuf as P; let p = P::from("emacs");"#,
        ),
        ("const binding", r#"const BIN: &str = "emacs";"#),
        (
            "associated const",
            r#"struct S; impl S { const BIN: &'static str = "emacs"; }"#,
        ),
        (
            "free function",
            r#"fn gnu_bin() -> &'static str { "emacs" }"#,
        ),
        (
            "inside macro_rules!",
            r#"macro_rules! mk { () => { Command::new("emacs") }; }"#,
        ),
        ("inside macro invocation", r#"vec!["emacs"]"#),
        ("raw string", r####"let p = r#"emacs"#;"####),
        ("byte string", r#"let p = b"emacs";"#),
        ("escaped string", r#"let p = "\x65macs";"#),
        ("unicode escaped string", r#"let p = "\u{65}macs";"#),
        ("concat! bypass", r#"let p = concat!("em", "acs");"#),
        (
            "legacy env var",
            r#"let _ = std::env::var_os("NEOMACS_GNU_EMACS_BINARY");"#,
        ),
        (
            "out-of-root #[path]",
            r#"#[path = "../../other/bypass.rs"] mod bypass;"#,
        ),
        (
            "out-of-root #[cfg_attr(..., path = ...)]",
            r#"#[cfg_attr(unix, path = "../../other/bypass.rs")] mod bypass;"#,
        ),
        (
            "out-of-root include!",
            r#"include!("../../other/bypass.rs");"#,
        ),
    ];

    for (label, code) in forbidden_cases {
        let findings = scan_source(crate_root, sample_file, "tests/sample.rs", code)
            .unwrap_or_else(|err| panic!("{label}: unexpected lex error: {err}"));
        assert!(
            !findings.is_empty(),
            "oracle guard failed to reject bypass shape {label:?}: {code}"
        );
    }

    // Fail-closed checks: unparseable/unterminated constructs must return Err.
    for (label, broken) in [
        ("unterminated string", r#"let s = "emacs"#),
        ("unterminated raw string", r####"let s = r#"emacs""####),
        ("unterminated block comment", r#"/* outer /* inner */"#),
    ] {
        assert!(
            scan_source(crate_root, sample_file, "tests/sample.rs", broken).is_err(),
            "oracle guard must fail closed on {label}"
        );
    }

    // Benign cases must not trigger false positives.
    let benign = r#"
// Comment mentioning "emacs" and NEOMACS_GNU_EMACS_BINARY is ignored.
/* Nested /* "emacs" */ block comment is ignored. */
fn ok<'pump>(x: &'pump str) -> PathBuf {
    let _c = '"';
    let _other = "emacs-lisp";
    let _flag = "--eval=(princ emacs-version)";
    #[path = "helper.rs"]
    mod helper;
    let _ = x;
    neomacs_tui_tests::oracle::gnu_oracle_program()
}
"#;
    let benign_findings = scan_source(crate_root, sample_file, "tests/sample.rs", benign)
        .expect("benign sample must lex cleanly");
    assert!(
        benign_findings.is_empty(),
        "benign sample triggered false positives: {benign_findings:?}"
    );
}
