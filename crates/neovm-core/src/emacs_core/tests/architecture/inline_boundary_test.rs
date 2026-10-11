//! Pin the verified subset of protected inline boundaries without changing
//! rustc's earlier inlining decisions.
//!
//! `Vm::run_loop`'s machine code moved three times in two days although its
//! own source did not change (2026-10-05/06, fat LTO): -5 instructions on
//! one branch, +185 instructions and frame 0x638 -> 0x648 on a second, -151
//! instructions and frame 0x638 -> 0x668 on a third. Helpers in other files
//! grew or shrank and LLVM's cost model flipped its inlining decision for
//! them; every flip moved VM-tier benchmark rows by 1.5-5%. Adding inline
//! hints from that census alone also changed rustc 1.96's MIR and pre-LTO
//! decisions. Only hints that survived the pre-LTO comparison are pinned.
//!
//! `protected_inline_boundary.list` (next to this file) classifies every
//! workspace function that the protected functions reach, as observed in the
//! DWARF inline tree of the reference fat-LTO shipping build:
//!
//! * `always` / `never`: existing or verified neutral attributes, required
//!   to remain `#[inline(always)]` / `#[inline(never)]`.
//! * `exempt`: a real function whose hint cannot be pinned without changing
//!   codegen, or which left no code at the protected site. Its reason records
//!   the exception; its original attribute is left alone.
//! * `generated`: an explicitly identified macro-generated function, whose
//!   macro invocation must still exist.
//!
//! The fourth column names the protected functions that reach the entry;
//! `root` marks a protected function itself, and `inline` independently marks
//! a body in the protected inline tree. Roots and `inline` entries are scanned
//! for calls to same-file helpers (`self.f(..)`, `Self::f(..)`, `Type::f(..)` of the
//! enclosing type, and free functions of the file): each one must be listed,
//! so a new helper on a protected path cannot arrive unclassified.
//!
//! This is the source half of the guard. The binary half,
//! `tmp/v10x/protected-codegen-check.sh`, pins the instruction count, frame,
//! callee set and inline tree of every protected function in the fat build;
//! regenerate this list from that census whenever the table is re-pinned.
//! Extending the attribute subset also requires comparing all protected
//! pre-LTO bodies and inline trees, plus the level-3 callee-attribute check.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const LIST: &str = include_str!("protected_inline_boundary.list");

const ROOTS: &[(&str, &str)] = &[
    ("emacs_core/runtime/bytecode/vm.rs", "Vm::run_loop"),
    ("emacs_core/runtime/eval/apply.rs", "Context::apply1"),
    (
        "emacs_core/runtime/eval/apply.rs",
        "Context::funcall_general_untraced",
    ),
    ("emacs_core/runtime/jit/cache.rs", "run_armed_leaf"),
    (
        "emacs_core/lisp/native/builtins/higher_order.rs",
        "builtin_mapcar_2",
    ),
    (
        "emacs_core/lisp/native/builtins/higher_order.rs",
        "builtin_mapc_2",
    ),
    (
        "emacs_core/lisp/native/builtins/higher_order.rs",
        "mapcar1_with_callee",
    ),
    (
        "emacs_core/lisp/native/builtins/higher_order.rs",
        "MapCallee::call",
    ),
];

/// What the list pins for one function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Boundary {
    Always,
    Never,
    Exempt,
    Generated,
}

impl Boundary {
    fn parse(word: &str) -> Option<Self> {
        match word {
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            "exempt" => Some(Self::Exempt),
            "generated" => Some(Self::Generated),
            _ => None,
        }
    }

    /// The attribute an `always`/`never` entry must carry.
    fn required_attribute(self) -> Option<&'static str> {
        match self {
            Self::Always => Some("#[inline(always)]"),
            Self::Never => Some("#[inline(never)]"),
            Self::Exempt | Self::Generated => None,
        }
    }
}

#[derive(Debug)]
struct Entry {
    boundary: Boundary,
    /// Relative to `crates/neovm-core/src`.
    file: String,
    /// `name`, `Type::name` or `<Type as Trait>::name`, as written in source.
    item: String,
    root: bool,
    inline_body: bool,
    line: usize,
}

fn parse_entries(list: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for (index, raw) in list.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim_end();
        if line.trim().is_empty() {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').map(str::trim).collect();
        assert!(
            columns.len() == 4 && columns.iter().all(|column| !column.is_empty()),
            "protected_inline_boundary.list:{}: expected `kind<TAB>file<TAB>item<TAB>reach`, got {raw:?}",
            index + 1
        );
        let boundary = Boundary::parse(columns[0]).unwrap_or_else(|| {
            panic!(
                "protected_inline_boundary.list:{}: unknown kind {:?} (always, never, exempt or generated)",
                index + 1,
                columns[0]
            )
        });
        out.push(Entry {
            boundary,
            file: columns[1].to_owned(),
            item: columns[2].to_owned(),
            root: columns[3].split(',').any(|reach| reach == "root"),
            inline_body: columns[3].split(',').any(|reach| reach == "inline"),
            line: index + 1,
        });
    }
    out
}

fn validate_entries(entries: &[Entry]) {
    let mut listed = BTreeSet::new();
    for entry in entries {
        assert!(
            listed.insert((entry.file.as_str(), entry.item.as_str())),
            "protected_inline_boundary.list:{}: duplicate entry for {} {}",
            entry.line,
            entry.file,
            entry.item
        );
        assert!(
            entry.boundary != Boundary::Always || entry.root || entry.inline_body,
            "protected_inline_boundary.list:{}: an always entry needs the independent inline marker",
            entry.line
        );
    }
    let roots: BTreeSet<_> = entries
        .iter()
        .filter(|entry| entry.root)
        .map(|entry| (entry.file.as_str(), entry.item.as_str()))
        .collect();
    assert_eq!(
        roots,
        ROOTS.iter().copied().collect(),
        "protected_inline_boundary.list must contain exactly the protected roots"
    );
}

fn entries() -> Vec<Entry> {
    let entries = parse_entries(LIST);
    validate_entries(&entries);
    entries
}

fn read_source(file: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(file);
    source_lines(
        &std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display())),
    )
}

/// Blank comments and literals while preserving line numbers and indentation.
/// The remaining text is enough for the source contract; it is not a Rust AST.
fn source_lines(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut code = bytes.to_vec();
    let mut at = 0;
    while at < bytes.len() {
        let rest = &text[at..];
        let mut end = None;
        if rest.starts_with("//") {
            end = Some(rest.find('\n').map_or(bytes.len(), |n| at + n));
        } else if rest.starts_with("/*") {
            let mut next = at + 2;
            let mut depth = 1;
            while next < bytes.len() && depth != 0 {
                if bytes[next..].starts_with(b"/*") {
                    depth += 1;
                    next += 2;
                } else if bytes[next..].starts_with(b"*/") {
                    depth -= 1;
                    next += 2;
                } else {
                    next += 1;
                }
            }
            end = Some(next);
        } else {
            let raw_prefix = if rest.starts_with("br") || rest.starts_with("cr") {
                2
            } else if rest.starts_with('r') {
                1
            } else {
                0
            };
            if raw_prefix != 0 && (at == 0 || !is_ident_char(bytes[at - 1] as char)) {
                let hashes = bytes[at + raw_prefix..]
                    .iter()
                    .take_while(|&&b| b == b'#')
                    .count();
                if bytes.get(at + raw_prefix + hashes) == Some(&b'"') {
                    let open = at + raw_prefix + hashes + 1;
                    let close = format!("\"{}", "#".repeat(hashes));
                    end = Some(
                        text[open..]
                            .find(&close)
                            .map_or(bytes.len(), |n| open + n + close.len()),
                    );
                }
            }
            if end.is_none() && bytes[at] == b'"' {
                let mut next = at + 1;
                while next < bytes.len() && bytes[next] != b'"' {
                    next += if bytes[next] == b'\\' { 2 } else { 1 };
                }
                end = Some((next + 1).min(bytes.len()));
            } else if end.is_none() && bytes[at] == b'\'' {
                // A lifetime is left intact; a character or escape is blanked.
                let next = at + 1;
                if bytes.get(next) == Some(&b'\\') {
                    let mut close = next;
                    while close < bytes.len() && bytes[close] != b'\'' {
                        close += if bytes[close] == b'\\' { 2 } else { 1 };
                    }
                    if bytes.get(close) == Some(&b'\'') {
                        end = Some(close + 1);
                    }
                } else if let Some(ch) = text[next..].chars().next() {
                    let close = next + ch.len_utf8();
                    if bytes.get(close) == Some(&b'\'') {
                        end = Some(close + 1);
                    }
                }
            }
        }
        if let Some(end) = end {
            for byte in &mut code[at..end] {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
            at = end;
        } else {
            at += rest.chars().next().expect("at is inside source").len_utf8();
        }
    }
    String::from_utf8(code)
        .expect("masking whole literals and comments preserves UTF-8")
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Currently the only generated function in the census comes from this
/// macro's first argument. Match its invocation, never a comment or reference.
fn generated_item_exists(lines: &[String], item: &str) -> bool {
    let code = lines.join("\n");
    code.match_indices("cached_symbol_id").any(|(at, name)| {
        if at > 0 && code[..at].chars().next_back().is_some_and(is_ident_char) {
            return false;
        }
        let Some(rest) = code[at + name.len()..].trim_start().strip_prefix('!') else {
            return false;
        };
        let Some(rest) = rest.trim_start().strip_prefix('(') else {
            return false;
        };
        rest.trim_start()
            .strip_prefix(item)
            .is_some_and(|tail| tail.trim_start().starts_with(','))
    })
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The function name declared on `line`, if it holds a `fn name(` or `fn name<`.
fn declared_fn(line: &str) -> Option<&str> {
    let mut rest = line;
    while let Some(at) = rest.find("fn ") {
        let before = &rest[..at];
        let name_start = &rest[at + 3..];
        rest = name_start;
        if before.chars().next_back().is_some_and(is_ident_char) {
            continue;
        }
        let name_start = name_start.trim_start();
        let name_len = name_start
            .find(|c: char| !is_ident_char(c))
            .unwrap_or(name_start.len());
        let (name, tail) = name_start.split_at(name_len);
        if !name.is_empty()
            && !name.starts_with(|c: char| c.is_ascii_digit())
            && tail.trim_start().starts_with(['(', '<'])
        {
            return Some(name);
        }
    }
    None
}

/// Drop a leading `<...>` generic parameter list.
fn strip_leading_generics(text: &str) -> &str {
    let text = text.trim_start();
    if !text.starts_with('<') {
        return text;
    }
    let mut depth = 0;
    for (index, c) in text.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return text[index + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    text
}

/// `&'a mut crate::x::Type<T>` -> `&Type`: references kept, lifetimes,
/// `mut`, the path prefix and generic arguments dropped.
fn type_name(text: &str) -> String {
    let mut text = text.trim();
    let mut refs = String::new();
    while let Some(rest) = text.strip_prefix('&') {
        refs.push('&');
        text = rest.trim_start();
        if let Some(rest) = text.strip_prefix('\'') {
            text = rest
                .trim_start_matches(|c: char| is_ident_char(c))
                .trim_start();
        }
        if let Some(rest) = text.strip_prefix("mut ") {
            text = rest.trim_start();
        }
    }
    let text = text.strip_prefix("dyn ").unwrap_or(text);
    let mut depth = 0;
    let mut plain = String::new();
    for c in text.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            _ if depth == 0 => plain.push(c),
            _ => {}
        }
    }
    let last = plain.trim().rsplit("::").next().unwrap_or("").trim();
    format!("{refs}{last}")
}

/// The block enclosing line `index`: an `impl`, a `trait`, or file level.
#[derive(Debug, PartialEq, Eq)]
enum Enclosing {
    File,
    Impl { ty: String, r#trait: Option<String> },
    Trait(String),
}

fn enclosing(lines: &[String], index: usize) -> Enclosing {
    let depth = indent(&lines[index]);
    if depth == 0 {
        return Enclosing::File;
    }
    let mut at = index;
    let header = loop {
        if at == 0 {
            return Enclosing::File;
        }
        at -= 1;
        let line = &lines[at];
        let trimmed = line.trim();
        if trimmed.is_empty()
            || indent(line) >= depth
            || trimmed.starts_with("//")
            || trimmed.starts_with("#[")
            || trimmed.starts_with('}')
        {
            continue;
        }
        // A multi-line impl header ends in a bare `{` or a `where` clause.
        if trimmed == "{" || trimmed.starts_with("where") || trimmed.ends_with(',') {
            continue;
        }
        break at;
    };
    let line = lines[header].trim_start();
    let line = line.strip_prefix("pub(crate) ").unwrap_or(line);
    let line = line.strip_prefix("pub ").unwrap_or(line);
    let line = line.strip_prefix("unsafe ").unwrap_or(line);
    if let Some(rest) = line.strip_prefix("trait ") {
        let name: String = rest.chars().take_while(|&c| is_ident_char(c)).collect();
        return Enclosing::Trait(name);
    }
    let Some(rest) = line.strip_prefix("impl") else {
        return Enclosing::File;
    };
    if rest.starts_with(|c: char| is_ident_char(c)) {
        return Enclosing::File;
    }
    let mut text = rest.to_owned();
    let mut next = header + 1;
    while !text.contains('{') && next < lines.len() {
        text.push(' ');
        text.push_str(lines[next].trim());
        next += 1;
    }
    let text = text.split('{').next().unwrap_or("");
    let text = strip_leading_generics(text);
    let text = text.split(" where").next().unwrap_or(text);
    match text.split_once(" for ") {
        Some((r#trait, ty)) => Enclosing::Impl {
            ty: type_name(ty),
            r#trait: Some(type_name(r#trait)),
        },
        None => Enclosing::Impl {
            ty: type_name(text),
            r#trait: None,
        },
    }
}

/// The list spelling of the function declared on line `index`.
fn item_at(lines: &[String], index: usize) -> Option<String> {
    let name = declared_fn(&lines[index])?;
    Some(match enclosing(lines, index) {
        Enclosing::File => name.to_owned(),
        Enclosing::Impl { ty, r#trait: None } => format!("{ty}::{name}"),
        Enclosing::Impl {
            ty,
            r#trait: Some(r#trait),
        } => format!("<{ty} as {trait}>::{name}"),
        Enclosing::Trait(r#trait) => format!("trait {trait}::{name}"),
    })
}

/// Line indices of every function the item names (cfg alternatives give
/// several).
fn find_item(lines: &[String], item: &str) -> Vec<usize> {
    let name = item.rsplit("::").next().unwrap_or(item);
    (0..lines.len())
        .filter(|&index| {
            declared_fn(&lines[index]) == Some(name)
                && item_at(lines, index).as_deref() == Some(item)
        })
        .collect()
}

/// Attributes in the attribute/comment block directly above
/// line `index`.
fn item_attributes(lines: &[String], index: usize) -> Vec<String> {
    let mut found = Vec::new();
    for line in lines[..index].iter().rev() {
        let trimmed = line.trim();
        let attribute_or_comment = trimmed.starts_with('#') || trimmed.starts_with("//");
        if !trimmed.is_empty()
            && (!attribute_or_comment
                && (trimmed.ends_with('}') || trimmed.ends_with(';') || trimmed.ends_with('{')))
        {
            break;
        }
        if trimmed.starts_with("#[") {
            found.push(trimmed.to_owned());
        }
    }
    found
}

/// The body of a function in the comment/literal-free source.
fn body(lines: &[String], index: usize) -> String {
    let text = lines[index..].join("\n");
    let mut depth = 0usize;
    let mut start = None;
    for (at, byte) in text.bytes().enumerate() {
        match byte {
            b'{' => {
                depth += 1;
                start.get_or_insert(at);
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0
                    && let Some(start) = start
                {
                    return text[start..=at].to_owned();
                }
            }
            b';' if start.is_none() => return String::new(),
            _ => {}
        }
    }
    String::new()
}

/// Same-file helpers the function on line `index` calls, spelled as list items.
fn same_file_callees(lines: &[String], index: usize, item: &str) -> BTreeSet<String> {
    let code = body(lines, index);
    let own_type = match enclosing(lines, index) {
        Enclosing::Impl { ty, .. } => Some(ty),
        Enclosing::File | Enclosing::Trait(_) => None,
    };
    let bytes = code.as_bytes();
    let mut candidates = BTreeSet::new();
    let mut at = 0;
    while at < bytes.len() {
        if !is_ident_char(bytes[at] as char) || (at > 0 && is_ident_char(bytes[at - 1] as char)) {
            at += 1;
            continue;
        }
        let end = at
            + code[at..]
                .find(|c: char| !is_ident_char(c))
                .unwrap_or(code.len() - at);
        let word = &code[at..end];
        let after = code[end..].trim_start();
        // Optional turbofish before the argument list.
        let after = match after.strip_prefix("::") {
            Some(rest) if rest.starts_with('<') => strip_leading_generics(rest),
            None => after,
            Some(_) => after,
        };
        if after.starts_with('(') && word.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        {
            let before = code[..at].trim_end();
            if let Some(receiver) = before.strip_suffix('.') {
                if receiver.trim_end().ends_with("self")
                    && let Some(ty) = &own_type
                {
                    candidates.insert(format!("{ty}::{word}"));
                }
            } else if let Some(path) = before.strip_suffix("::") {
                let qualifier: String = path
                    .chars()
                    .rev()
                    .take_while(|&c| is_ident_char(c))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                match (qualifier.as_str(), &own_type) {
                    ("Self", Some(ty)) => {
                        candidates.insert(format!("{ty}::{word}"));
                    }
                    (q, _) if q.starts_with(|c: char| c.is_ascii_uppercase()) => {
                        candidates.insert(format!("{q}::{word}"));
                    }
                    _ => {}
                }
            } else if !before.ends_with("fn") {
                candidates.insert(word.to_owned());
            }
        }
        at = end;
    }
    candidates.remove(item);
    candidates
        .into_iter()
        .filter(|candidate| !find_item(lines, candidate).is_empty())
        .collect()
}

#[test]
fn protected_inline_boundary_attributes_match_the_pinned_list() {
    let mut sources: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut problems = Vec::new();
    for entry in entries() {
        let lines = sources
            .entry(entry.file.clone())
            .or_insert_with(|| read_source(&entry.file));
        if entry.boundary == Boundary::Generated {
            if !generated_item_exists(lines, &entry.item) {
                problems.push(format!(
                    "list line {}: generated {} {} has no recognized macro invocation",
                    entry.line, entry.file, entry.item
                ));
            }
            continue;
        }
        let found = find_item(lines, &entry.item);
        if found.is_empty() {
            problems.push(format!(
                "list line {}: {} {} not found (renamed, moved or deleted?): re-derive the list from \
                 the protected-codegen-check.sh census",
                entry.line, entry.file, entry.item
            ));
            continue;
        }
        let Some(required) = entry.boundary.required_attribute() else {
            continue;
        };
        for index in found {
            let attributes: Vec<_> = item_attributes(lines, index)
                .into_iter()
                .filter(|attribute| attribute.starts_with("#[inline"))
                .collect();
            if attributes == [required] {
                continue;
            }
            problems.push(format!(
                "{}:{} `{}` carries {:?} but the protected inline boundary pins `{}` \
                 (list line {}); extending this subset requires a protected pre-LTO comparison",
                entry.file,
                index + 1,
                entry.item,
                attributes,
                required,
                entry.line
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "protected inline boundary violations:\n  {}",
        problems.join("\n  ")
    );
}

#[test]
fn protected_functions_call_only_classified_same_file_helpers() {
    let entries = entries();
    let listed: BTreeSet<(&str, &str)> = entries
        .iter()
        .map(|entry| (entry.file.as_str(), entry.item.as_str()))
        .collect();
    let mut sources: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut unclassified: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for entry in entries
        .iter()
        .filter(|entry| entry.root || entry.inline_body)
    {
        let lines = sources
            .entry(entry.file.clone())
            .or_insert_with(|| read_source(&entry.file));
        for index in find_item(lines, &entry.item) {
            for callee in same_file_callees(lines, index, &entry.item) {
                if !listed.contains(&(entry.file.as_str(), callee.as_str())) {
                    unclassified
                        .entry((entry.file.clone(), callee))
                        .or_default()
                        .insert(entry.item.clone());
                }
            }
        }
    }
    let report: Vec<String> = unclassified
        .iter()
        .map(|((file, callee), callers)| format!("{file}: {callee} (called from {callers:?})"))
        .collect();
    assert!(
        report.is_empty(),
        "helpers reached from a protected function but missing from \
         protected_inline_boundary.list -- classify it from the protected census; require a hint \
         only after its protected pre-LTO comparison is neutral, otherwise record an exemption:\n  {}",
        report.join("\n  ")
    );
}

#[test]
fn saved_labeled_restrictions_keep_the_cold_drop_boundary() {
    use crate::buffer::{LabeledRestriction, SavedLabeledRestrictions, SavedRestrictionState};

    static_assertions::assert_eq_size!(SavedLabeledRestrictions, Option<Vec<LabeledRestriction>>);
    static_assertions::assert_eq_align!(SavedLabeledRestrictions, Option<Vec<LabeledRestriction>>);

    // The saved state's field must retain the wrapper, even if an unused
    // wrapper and its Drop implementation remain elsewhere in the source.
    let _: fn(&SavedRestrictionState) -> &SavedLabeledRestrictions =
        |saved| &saved.labeled_restrictions;

    let lines = read_source("buffer/buffer.rs");
    let found = find_item(&lines, "<SavedLabeledRestrictions as Drop>::drop");
    assert_eq!(
        found.len(),
        1,
        "the saved restriction wrapper needs one Drop"
    );
    let attributes = item_attributes(&lines, found[0]);
    for required in ["#[cold]", "#[inline(never)]"] {
        assert!(
            attributes.iter().any(|attribute| attribute == required),
            "SavedLabeledRestrictions::drop must carry {required}, got {attributes:?}"
        );
    }
}

#[test]
fn inline_boundary_list_rejects_duplicate_and_missing_roots() {
    let roots = ROOTS
        .iter()
        .map(|(file, item)| format!("exempt\t{file}\t{item}\troot"))
        .collect::<Vec<_>>()
        .join("\n");
    validate_entries(&parse_entries(&roots));

    let duplicate = format!("{roots}\n{}", roots.lines().next().unwrap());
    assert!(std::panic::catch_unwind(|| validate_entries(&parse_entries(&duplicate))).is_err());
    assert!(std::panic::catch_unwind(|| validate_entries(&parse_entries(""))).is_err());
    let missing = roots.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert!(std::panic::catch_unwind(|| validate_entries(&parse_entries(&missing))).is_err());
    let extra = format!("{roots}\nexempt\tx.rs\tOther::run\troot");
    assert!(std::panic::catch_unwind(|| validate_entries(&parse_entries(&extra))).is_err());

    let with_inline = format!("{roots}\nexempt\tx.rs\tHelper::call\tinline,run_loop");
    let entries = parse_entries(&with_inline);
    validate_entries(&entries);
    assert!(entries.last().unwrap().inline_body);
    assert_eq!(entries.last().unwrap().boundary, Boundary::Exempt);
}

#[test]
fn inline_boundary_source_scan_ignores_comments_and_literals() {
    let lines = source_lines(
        r####"
        /* fn absent() {} /* nested comment */ */
        const TEXT: &str = r###"fn absent() {}"###;
        impl Example {
            #[inline(always)]
            // The attribute block may include a comment.
            fn run<'a>(&'a self) {
                self.real::<Option<Vec<u8>>>();
                let _ = "self.absent()";
                let _ = br#"self.absent()"#;
                let _ = '\u{7b}';
                let _ = 'é';
                /* self.absent(); /* nested */ */
            }
            fn real<T>(&self) {}
            fn absent(&self) {}
        }
        "####,
    );
    assert!(find_item(&lines, "absent").is_empty());
    let run = find_item(&lines, "Example::run");
    assert_eq!(run.len(), 1);
    assert_eq!(item_attributes(&lines, run[0]), ["#[inline(always)]"]);
    assert_eq!(
        same_file_callees(&lines, run[0], "Example::run"),
        BTreeSet::from(["Example::real".to_owned()])
    );

    let generated = source_lines("cached_symbol_id! ( lambda_symbol, \"lambda\" );");
    assert!(generated_item_exists(&generated, "lambda_symbol"));
    assert!(!generated_item_exists(&generated, "lambda"));
    let comments = source_lines(
        "// cached_symbol_id!(lambda_symbol, \"lambda\");\n\
         const TEXT: &str = \"cached_symbol_id!(lambda_symbol, x)\";",
    );
    assert!(!generated_item_exists(&comments, "lambda_symbol"));
}
