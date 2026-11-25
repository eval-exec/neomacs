//! DESIGN §6.9 codec rule. bincode 2.0.1 serializes by position, so the serde
//! attributes it lists as losing data (`src/features/serde/mod.rs:42-53`) must
//! not appear on a type a wire message can reach. The one exception is a
//! symmetric field-level `skip` whose value the receiver rebuilds after decode:
//! both sides skip the same position, and the rebuild restores the value.
//!
//! The type graph is read from this crate's sources and the identity leaf it
//! re-exports, starting from the wire vocabulary roots below. Every identifier
//! that names a type defined here is followed, so the reachable set is an
//! over-approximation: a false edge can only make the rule stricter.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// The seven attributes bincode cannot honour.
const LOSSY: [&str; 7] = [
    "flatten",
    "skip",
    "skip_deserializing",
    "skip_serializing",
    "skip_serializing_if",
    "tag",
    "untagged",
];

/// Types a wire message carries directly: the frame state FrameSync encodes,
/// the input frontier, pointer hits, menus, and the boundary vocabulary.
const WIRE_ROOTS: &[&str] = &[
    "FrameDisplayState",
    "FrontierSnapshot",
    "InputReceipt",
    "InputCancelled",
    "PresentedHit",
    "PopupMenuItem",
    "MenuToken",
    "MenuBarRequestId",
    "FrontendKey",
    "ModifierBits",
    "DisplayTime",
    "PointerTarget",
    "LiveDisplayFrameId",
    "GuiFrameGeometryHints",
    "WindowVisibility",
    "WindowFullscreenMode",
    "MonitorInfo",
    "SystemFonts",
    "SystemFontName",
    "SystemFontRole",
    "GuiResourceQuery",
    "FrameFontSize",
    "SurfaceShaderLanguage",
    "SurfaceUniformInit",
    "SurfaceChannelSource",
    "ImageDataSource",
    "ResolvedImageMetadata",
    "TerminalId",
    "TerminalDisplayTarget",
    "TerminalGridSize",
    "TerminalFloatPlacement",
    "GuiResizeRequestId",
    "FrameVisibility",
];

/// Symmetric `skip` fields whose value the receiver rebuilds after decode,
/// with the rebuild that makes the exception sound.
const REBUILT_AFTER_DECODE: &[(&str, &str, &str)] = &[
    (
        "FrameDisplayState",
        "prepared_scroll_admissions",
        "memo: prepare_scroll_coverage recomputes a missing admission",
    ),
    (
        "GeometryPoint",
        "space",
        "zero-sized PhantomData marker: carries no value",
    ),
    (
        "ImageExtent",
        "space",
        "zero-sized PhantomData marker: carries no value",
    ),
    (
        "PresentedHitIndex",
        "nontext_buckets",
        "OnceLock built by the first matching query (clones start cold)",
    ),
    (
        "PresentedHitIndex",
        "pointer_regions",
        "re-bound from the frame's pointer map by FrameGlyphBuffer::install_presented_hit_index",
    ),
];

/// Types the FrameSync encoder writes field by field, destructured without
/// `..`: "The frame is not sent through FrameDisplayState's derive" (DESIGN
/// §6.9; rows travel as `RowRuns`, §6.10). Their own field attributes never
/// reach the codec, so they are not checked; the types of their fields are
/// still followed, because the encoder may write those through serde. The
/// derives of these types serve JSON snapshots.
const ENCODED_FIELD_BY_FIELD: &[(&str, &str)] = &[
    ("Glyph", "destructured into its row's RowRuns"),
    (
        "GlyphRow",
        "destructured into RowRuns; its process-local appearance is replaced by a content key",
    ),
];

#[derive(Debug)]
struct Item {
    file: PathBuf,
    container_attrs: Vec<String>,
    body: String,
}

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read source dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "tests") {
                source_files(&path, out);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

/// Source text with comments and string literal contents blanked, so braces,
/// keywords and identifiers are only ever code.
fn code_only(source: &str) -> String {
    let chars = source.chars().collect::<Vec<_>>();
    let mut out = String::with_capacity(source.len());
    let mut index = 0;
    while index < chars.len() {
        let (current, next) = (chars[index], chars.get(index + 1).copied());
        if current == '/' && next == Some('/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
        } else if current == '/' && next == Some('*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index += 2;
        } else if current == '"' {
            out.push('"');
            index += 1;
            while index < chars.len() && chars[index] != '"' {
                if chars[index] == '\\' {
                    index += 1;
                }
                index += 1;
            }
            out.push('"');
            index += 1;
        } else if current == '\'' && chars.get(index + 2) == Some(&'\'') {
            // A char literal such as '{' must not count as a brace.
            out.push_str("' '");
            index += 3;
        } else {
            out.push(current);
            index += 1;
        }
    }
    out
}

/// `serde(...)` keys of one attribute block, e.g. `default`, `skip`, `rename`.
fn serde_keys(attribute: &str) -> Vec<String> {
    let Some(start) = attribute.find("serde(") else {
        return Vec::new();
    };
    let inner = &attribute[start + "serde(".len()..attribute.rfind(')').unwrap_or(attribute.len())];
    let mut keys = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for character in inner.chars().chain(std::iter::once(',')) {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let key = current
                    .split(['=', '('])
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
                if !key.is_empty() {
                    keys.push(key);
                }
                current.clear();
                continue;
            }
            _ => {}
        }
        current.push(character);
    }
    keys
}

fn matching(chars: &[char], open_index: usize, open: char, close: char) -> usize {
    let mut depth = 0usize;
    for (offset, character) in chars[open_index..].iter().enumerate() {
        if *character == open {
            depth += 1;
        } else if *character == close {
            depth -= 1;
            if depth == 0 {
                return open_index + offset;
            }
        }
    }
    chars.len() - 1
}

fn is_ident(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

/// Attribute blocks written directly before `item_start`, innermost last.
fn preceding_attributes(chars: &[char], item_start: usize) -> Vec<String> {
    let mut attributes = Vec::new();
    let mut cursor = item_start;
    loop {
        while cursor > 0 && chars[cursor - 1].is_whitespace() {
            cursor -= 1;
        }
        if cursor == 0 || chars[cursor - 1] != ']' {
            return attributes;
        }
        let mut depth = 0usize;
        let mut open = cursor - 1;
        loop {
            match chars[open] {
                ']' => depth += 1,
                '[' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            open -= 1;
        }
        if open == 0 || chars[open - 1] != '#' {
            return attributes;
        }
        attributes.push(chars[open - 1..cursor].iter().collect());
        cursor = open - 1;
    }
}

/// Where an item starts once a visibility such as `pub(crate)` is included.
fn visibility_start(chars: &[char], keyword: usize) -> usize {
    let mut cursor = keyword;
    while cursor > 0 && chars[cursor - 1].is_whitespace() {
        cursor -= 1;
    }
    let mut start = cursor;
    if cursor > 0 && chars[cursor - 1] == ')' {
        let mut depth = 0usize;
        let mut open = cursor - 1;
        loop {
            match chars[open] {
                ')' => depth += 1,
                '(' => {
                    depth -= 1;
                    if depth == 0 || open == 0 {
                        break;
                    }
                }
                _ => {}
            }
            if open == 0 {
                break;
            }
            open -= 1;
        }
        start = open;
    }
    if start >= 3 && chars[start - 3..start] == ['p', 'u', 'b'] {
        start - 3
    } else {
        keyword
    }
}

fn items(file: &Path, source: &str, out: &mut BTreeMap<String, Vec<Item>>) {
    let code = code_only(source);
    let chars = code.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        let at_word = index == 0 || !is_ident(chars[index - 1]);
        let keyword = ["struct", "enum", "type"].into_iter().find(|keyword| {
            at_word
                && keyword
                    .chars()
                    .enumerate()
                    .all(|(offset, expected)| chars.get(index + offset) == Some(&expected))
                && chars
                    .get(index + keyword.len())
                    .is_some_and(|after| after.is_whitespace())
        });
        let Some(keyword) = keyword else {
            index += 1;
            continue;
        };
        let mut cursor = index + keyword.len();
        while chars[cursor].is_whitespace() {
            cursor += 1;
        }
        let name_start = cursor;
        while cursor < chars.len() && is_ident(chars[cursor]) {
            cursor += 1;
        }
        let name = chars[name_start..cursor].iter().collect::<String>();
        let item_start = visibility_start(&chars, index);
        // Skip generics, then take the body.
        let mut body_end = cursor;
        while body_end < chars.len() && !matches!(chars[body_end], '{' | '(' | ';' | '=') {
            if chars[body_end] == '<' {
                body_end = matching(&chars, body_end, '<', '>');
            }
            body_end += 1;
        }
        let body = match chars.get(body_end) {
            Some('{') => {
                let close = matching(&chars, body_end, '{', '}');
                chars[body_end..=close].iter().collect::<String>()
            }
            Some('(') => {
                let close = matching(&chars, body_end, '(', ')');
                chars[body_end..=close].iter().collect::<String>()
            }
            Some('=') => {
                let mut end = body_end;
                while end < chars.len() && chars[end] != ';' {
                    end += 1;
                }
                chars[body_end..end].iter().collect::<String>()
            }
            _ => String::new(),
        };
        if !name.is_empty() {
            out.entry(name).or_default().push(Item {
                file: file.to_owned(),
                container_attrs: preceding_attributes(&chars, item_start),
                body,
            });
        }
        index = cursor;
    }
}

/// `(field or variant, serde keys)` for every attributed member of a body.
fn member_attributes(body: &str) -> Vec<(String, Vec<String>)> {
    let chars = body.chars().collect::<Vec<_>>();
    let mut members = Vec::new();
    let mut index = 0;
    while index + 1 < chars.len() {
        if chars[index] == '#' && chars[index + 1] == '[' {
            let close = matching(&chars, index + 1, '[', ']');
            let attribute = chars[index..=close].iter().collect::<String>();
            let keys = serde_keys(&attribute);
            let mut cursor = close + 1;
            loop {
                while cursor < chars.len() && chars[cursor].is_whitespace() {
                    cursor += 1;
                }
                if chars.get(cursor) == Some(&'#') {
                    cursor = matching(&chars, cursor + 1, '[', ']') + 1;
                } else {
                    break;
                }
            }
            let rest = chars[cursor..].iter().collect::<String>();
            let rest = rest
                .trim_start_matches("pub(crate)")
                .trim_start_matches("pub(super)")
                .trim_start_matches("pub")
                .trim_start();
            let member = rest
                .chars()
                .take_while(|c| is_ident(*c))
                .collect::<String>();
            if !keys.is_empty() {
                members.push((member, keys));
            }
            index = close + 1;
        } else {
            index += 1;
        }
    }
    members
}

fn identifiers(body: &str) -> BTreeSet<String> {
    body.split(|character: char| !is_ident(character))
        .filter(|word| word.starts_with(|c: char| c.is_ascii_uppercase()))
        .map(str::to_owned)
        .collect()
}

#[test]
fn wire_reachable_types_respect_the_bincode_attribute_rule() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    source_files(&manifest.join("src"), &mut files);
    source_files(&manifest.join("../neovm-display-identity/src"), &mut files);
    let mut catalog = BTreeMap::new();
    for file in &files {
        items(
            file,
            &fs::read_to_string(file).expect("read source"),
            &mut catalog,
        );
    }

    let mut failures = Vec::new();
    let mut reachable = BTreeSet::new();
    let mut pending = WIRE_ROOTS
        .iter()
        .map(|root| (*root).to_owned())
        .collect::<Vec<_>>();
    for (encoded, _) in ENCODED_FIELD_BY_FIELD {
        if !catalog.contains_key(*encoded) {
            failures.push(format!(
                "field-by-field type {encoded} is not defined in display-protocol"
            ));
        }
    }
    for root in WIRE_ROOTS {
        if !catalog.contains_key(*root) {
            failures.push(format!(
                "wire root {root} is not defined in display-protocol or the identity leaf"
            ));
        }
    }
    while let Some(name) = pending.pop() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        for item in catalog.get(&name).into_iter().flatten() {
            pending.extend(
                identifiers(&item.body)
                    .into_iter()
                    .filter(|identifier| catalog.contains_key(identifier)),
            );
        }
    }

    let mut allowed_skips = REBUILT_AFTER_DECODE
        .iter()
        .map(|(owner, field, _)| ((*owner).to_owned(), (*field).to_owned()))
        .collect::<BTreeSet<_>>();
    for name in &reachable {
        for item in catalog.get(name).into_iter().flatten() {
            let location = item
                .file
                .strip_prefix(manifest)
                .unwrap_or(&item.file)
                .display();
            for attribute in &item.container_attrs {
                for key in serde_keys(attribute) {
                    if LOSSY.contains(&key.as_str()) {
                        failures.push(format!(
                            "{location}: {name} has container attribute serde({key})"
                        ));
                    }
                }
            }
            if ENCODED_FIELD_BY_FIELD
                .iter()
                .any(|(encoded, _)| encoded == name)
            {
                continue;
            }
            for (member, keys) in member_attributes(&item.body) {
                for key in keys.iter().filter(|key| LOSSY.contains(&key.as_str())) {
                    let symmetric_skip =
                        key == "skip" && allowed_skips.remove(&(name.clone(), member.clone()));
                    if !symmetric_skip {
                        failures.push(format!("{location}: {name}.{member} has serde({key})"));
                    }
                }
            }
        }
    }
    for (owner, field) in allowed_skips {
        failures.push(format!(
            "stale rebuild exception {owner}.{field}: no longer a symmetric skip on a wire-reachable type"
        ));
    }
    assert!(
        failures.is_empty(),
        "{} wire-reachable serde attribute(s) bincode cannot honour (DESIGN §6.9):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
