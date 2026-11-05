//! B3 TEST-SUPPORT-VIS guard (lead ruling 02:1xZ, peer signature 02:2xZ V1-V6).
//!
//! The A' mechanism keeps every widened item's *production* visibility
//! unchanged and exposes it only under the crate's `test-support` feature
//! (`#[cfg_attr(feature = "test-support", visibility::make(pub))]`, or a
//! production/cfg-split declaration pair for struct fields and file modules
//! where the attribute cannot be applied). Each provider crate records those
//! items in `TEST-SUPPORT-VIS-<CRATE>.tsv` beside this guard, with its own
//! `.ceiling` ratchet.
//!
//! This guard is the class's B3 half:
//!
//! * every ledger has the signed six-column shape and its rows never exceed
//!   the ceiling;
//! * every row names an existing defining file in the named crate, a known
//!   kind, and a declaration that the file actually contains;
//! * V1 disjointness, fail-closed: a row may not name an item that a
//!   compile-fail fixture/golden names (trybuild families under
//!   `crates/neovm-core/tests`, plus every `*.stderr` golden there), nor a
//!   SEAL-producer / PC2-installer / authority row of the promotion ledgers;
//! * every `visibility::make(pub)` declaration in an extracted crate must be
//!   listed by a row (the A' spelling; the documented cfg-split field/module
//!   form cannot be discovered textually and is covered by the row checks
//!   instead).
//!
//! The guard is a no-op while no `TEST-SUPPORT-VIS-*` ledger exists, so the
//! class can land before the provider batches. Production surface is never
//! read or widened here.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::architecture_plist_head_seam_guard_test::code_only;
use super::architecture_production_source_roots::workspace_root;

const ARCHITECTURE_DIR: &str = "crates/neovm-core/src/emacs_core/tests/architecture";
const VIS_PREFIX: &str = "TEST-SUPPORT-VIS-";
const HEADER: &str = "crate\titem\tkind\tfile\ttests\treason";
const VIS_CRATES: &[&str] = &[
    "neovm-value",
    "neovm-edit",
    "neovm-text",
    "neovm-eval",
    "neovm-display",
    "neovm-system",
    "neovm-display-identity",
];
const KINDS: &[&str] = &[
    "fn",
    "method",
    "struct",
    "enum",
    "union",
    "trait",
    "type",
    "const",
    "const fn",
    "unsafe fn",
    "static",
    "mod",
    "field",
];

/// Trybuild fixture families under `crates/neovm-core/tests`. Every `*.stderr`
/// golden below `tests/` is added on top, so a later suite joins automatically.
const FIXTURE_DIRS: &[&str] = &[
    "thread_safety_ui",
    "thread_safety_pass",
    "host_effect_ui",
    "host_effect_pass",
    "pentry_ui",
    "pentry_pass",
    "p15_threading_ui",
    "p15_threading_pass",
    "p15_threading_support",
    "p2ht_ui",
    "p2ht_pass",
    "p1rv_ui",
    "p1rv_pass",
    "p0s3_ui",
    "p0s3_pass",
    "intern_shared_ui",
    "intern_shared_pass",
    "chartable_write_ui",
    "mint_seal_ui",
    "split_t2_ui",
    "split_t2_pass",
];

struct VisRow {
    crate_dir: String,
    item: String,
    kind: String,
    file: String,
    /// One or more test/source files that need the widened item. The signed
    /// format writes several entries as extra tab-separated columns before
    /// the reason, so every field between `file` and `reason` is one path.
    tests: Vec<String>,
}

#[derive(Default)]
struct FixtureNames {
    words: BTreeSet<String>,
    qualified: BTreeSet<String>,
    use_leaves: BTreeSet<String>,
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn read_dir_sorted(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))? {
        entries.push(entry.map_err(|error| error.to_string())?.path());
    }
    entries.sort();
    Ok(entries)
}

fn walk_files(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !path.is_dir() {
        return Ok(());
    }
    for entry in read_dir_sorted(path)? {
        if entry.is_dir() {
            walk_files(&entry, out)?;
        } else {
            out.push(entry);
        }
    }
    Ok(())
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Identifiers plus `A::b::C` chains plus `use` leaves.
fn collect_fixture_names(source: &str, names: &mut FixtureNames) {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if !is_ident_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_ident_byte(bytes[index]) {
            index += 1;
        }
        let first = &source[start..index];
        names.words.insert(first.to_owned());
        let mut chain = first.to_owned();
        let mut end = index;
        while bytes.get(end) == Some(&b':') && bytes.get(end + 1) == Some(&b':') {
            let mut next = end + 2;
            if next >= bytes.len() || !is_ident_start(bytes[next]) {
                break;
            }
            let piece_start = next;
            while next < bytes.len() && is_ident_byte(bytes[next]) {
                next += 1;
            }
            chain.push_str("::");
            chain.push_str(&source[piece_start..next]);
            names.words.insert(source[piece_start..next].to_owned());
            end = next;
        }
        if chain.contains("::") {
            names.qualified.insert(chain);
        }
        index = end.max(index);
    }
    // `use ...;` leaves are module names a fixture names directly.
    for line in source.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("use ") {
            continue;
        }
        let Some(body) = trimmed.strip_prefix("use ") else {
            continue;
        };
        let body = body.split(';').next().unwrap_or(body);
        for part in body.split(',') {
            let leaf = part
                .trim()
                .rsplit("::")
                .next()
                .unwrap_or_default()
                .split(" as ")
                .next()
                .unwrap_or_default()
                .trim();
            if !leaf.is_empty() && leaf != "*" && leaf != "{" && leaf != "}" {
                names.use_leaves.insert(leaf.to_owned());
            }
        }
    }
}

fn fixture_names(workspace: &Path) -> Result<FixtureNames, String> {
    let tests = workspace.join("crates/neovm-core/tests");
    let mut names = FixtureNames::default();
    let mut rust_files = Vec::new();
    for dir in FIXTURE_DIRS {
        walk_files(&tests.join(dir), &mut rust_files)?;
    }
    for path in rust_files {
        if path.extension().is_some_and(|extension| extension == "rs") {
            collect_fixture_names(&code_only(&read(&path)?), &mut names);
        }
    }
    let mut all_files = Vec::new();
    walk_files(&tests, &mut all_files)?;
    for path in all_files {
        if path
            .extension()
            .is_some_and(|extension| extension == "stderr")
        {
            collect_fixture_names(&read(&path)?, &mut names);
        }
    }
    Ok(names)
}

/// Whether any qualified suffix of `item` (length >= 2) appears as a chain in
/// the fixtures, or a type-shaped leaf appears as a bare word.
fn names_in_fixtures(item: &str, kind: &str, fixtures: &FixtureNames) -> Option<String> {
    let segments: Vec<&str> = item.split("::").collect();
    let leaf = *segments.last()?;
    match kind {
        "struct" | "enum" | "union" | "trait" | "type" => {
            if fixtures.words.contains(leaf) || fixtures.use_leaves.contains(leaf) {
                return Some(format!("type name `{leaf}` appears in a fixture/golden"));
            }
        }
        "mod" => {
            if fixtures.use_leaves.contains(leaf) {
                return Some(format!("module `{leaf}` is imported by a fixture"));
            }
        }
        _ => {}
    }
    for suffix_len in 2..=segments.len().min(4) {
        let suffix = segments[segments.len() - suffix_len..].join("::");
        for chain in &fixtures.qualified {
            if chain == &suffix || chain.ends_with(&format!("::{suffix}")) {
                return Some(format!(
                    "qualified name `{suffix}` appears in a fixture/golden as `{chain}`"
                ));
            }
        }
    }
    None
}

/// SEAL-producer, PC2-installer and authority rows from every promotion
/// ledger. These are the items V1 forbids the VIS class from widening.
fn authority_items(workspace: &Path) -> Result<BTreeSet<String>, String> {
    let mut items = BTreeSet::new();
    for path in read_dir_sorted(&workspace.join(ARCHITECTURE_DIR))? {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("PROMOTION-LEDGER") || !name.ends_with(".tsv") {
            continue;
        }
        for line in read(&path)?.lines() {
            if line.starts_with("item\t") || line.trim().is_empty() {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() != 6 {
                continue;
            }
            let kind = fields[1];
            let reason = fields[5];
            let is_authority = kind == "pc2"
                || kind == "seal"
                || reason.contains("SEAL producer")
                || reason.contains("seal mint")
                || reason.contains("SEAL MINT")
                || reason.contains("PC2 installer")
                || reason.contains("authority row")
                || reason.contains("AUTHORITY");
            if is_authority {
                items.insert(fields[0].to_owned());
            }
        }
    }
    Ok(items)
}

fn names_authority_item(item: &str, authority: &BTreeSet<String>) -> Option<String> {
    let segments: Vec<&str> = item.split("::").collect();
    for candidate in authority {
        let candidate_segments: Vec<&str> = candidate.split("::").collect();
        if item == candidate {
            return Some(format!("authority/SEAL/PC2 row `{candidate}`"));
        }
        // A VIS row for a type/field that the authority row names directly.
        let candidate_leaf = candidate_segments.last().copied().unwrap_or(candidate);
        let item_leaf = segments.last().copied().unwrap_or(item);
        let type_like = candidate_leaf
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_uppercase());
        if type_like && segments.contains(&candidate_leaf) {
            return Some(format!("authority/SEAL/PC2 row `{candidate}`"));
        }
        if candidate_segments.len() >= 2 {
            for suffix_len in 2..=segments.len().min(candidate_segments.len()) {
                let suffix = segments[segments.len() - suffix_len..].join("::");
                if candidate == &suffix
                    || candidate.ends_with(&format!("::{suffix}"))
                    || suffix.ends_with(&format!("::{candidate}"))
                {
                    return Some(format!("authority/SEAL/PC2 row `{candidate}`"));
                }
            }
        } else if candidate_leaf == item_leaf {
            return Some(format!("authority/SEAL/PC2 row `{candidate}`"));
        }
    }
    None
}

fn parse_ledger(path: &Path, text: &str) -> Result<Vec<VisRow>, String> {
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Err(format!(
            "{}: exact six-column header required: {HEADER}",
            path.display()
        ));
    }
    let mut rows = Vec::new();
    let mut identities = BTreeSet::new();
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 6 || fields.iter().any(|field| field.trim().is_empty()) {
            return Err(format!(
                "{}: invalid VIS row at line {}",
                path.display(),
                index + 2
            ));
        }
        let row = VisRow {
            crate_dir: fields[0].to_owned(),
            item: fields[1].to_owned(),
            kind: fields[2].to_owned(),
            file: fields[3].to_owned(),
            tests: fields[4..fields.len() - 1]
                .iter()
                .map(|test| (*test).to_owned())
                .collect(),
        };
        if !identities.insert((
            row.crate_dir.clone(),
            row.item.clone(),
            row.kind.clone(),
            row.file.clone(),
        )) {
            return Err(format!(
                "{}: duplicate VIS identity {} {} {} {}",
                path.display(),
                row.crate_dir,
                row.kind,
                row.item,
                row.file
            ));
        }
        rows.push(row);
    }
    Ok(rows)
}

fn check_row(
    workspace: &Path,
    row: &VisRow,
    fixtures: &FixtureNames,
    authority: &BTreeSet<String>,
) -> Result<(), String> {
    let what = format!("{} {} {}", row.crate_dir, row.kind, row.item);
    if !VIS_CRATES.contains(&row.crate_dir.as_str()) {
        return Err(format!("{what}: unknown test-support crate"));
    }
    if !KINDS.contains(&row.kind.as_str()) {
        return Err(format!("{what}: unknown kind"));
    }
    if Path::new(&row.file).is_absolute()
        || Path::new(&row.file)
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(format!("{what}: defining file must be workspace-relative"));
    }
    if !row
        .file
        .starts_with(&format!("crates/{}/src/", row.crate_dir))
    {
        return Err(format!(
            "{what}: defining file must live in the row's crate"
        ));
    }
    let definition = workspace.join(&row.file);
    if !definition.is_file() {
        return Err(format!(
            "{what}: defining file is missing: {}",
            definition.display()
        ));
    }
    let source = read(&definition)?;
    let segments: Vec<&str> = row.item.split("::").collect();
    let leaf = segments.last().copied().unwrap_or_default();
    if !source.contains(leaf) {
        return Err(format!(
            "{what}: `{leaf}` does not occur in the defining file"
        ));
    }
    // Type-qualified rows (fields, methods, associated functions) must name
    // a type that the defining file actually declares. Lowercase segments are
    // module path components and need not occur textually in their own file.
    for segment in &segments[..segments.len().saturating_sub(1)] {
        let type_like = segment
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_uppercase());
        if type_like && !source.contains(segment) {
            return Err(format!(
                "{what}: type `{segment}` does not occur in the defining file"
            ));
        }
    }
    for test in &row.tests {
        if Path::new(test.as_str()).is_absolute() || test.contains("..") {
            return Err(format!("{what}: test path must be workspace-relative"));
        }
        if !workspace.join(test.as_str()).is_file() {
            return Err(format!("{what}: named test file is missing: {test}"));
        }
    }
    if let Some(why) = names_in_fixtures(&row.item, &row.kind, fixtures) {
        return Err(format!("{what}: V1 forbids widening this item: {why}"));
    }
    if let Some(why) = names_authority_item(&row.item, authority) {
        return Err(format!("{what}: V1 forbids widening this item: {why}"));
    }
    Ok(())
}

/// Every `visibility::make(pub)` site in an extracted crate, as
/// `(workspace-relative file, item name)`.
fn visibility_make_sites(workspace: &Path) -> Result<Vec<(String, String)>, String> {
    let mut sites = Vec::new();
    for crate_dir in VIS_CRATES {
        let src = workspace.join("crates").join(crate_dir).join("src");
        let mut files = Vec::new();
        walk_files(&src, &mut files)?;
        for path in files {
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let text = read(&path)?;
            if !text.contains("visibility::make(pub)") {
                continue;
            }
            let relative = path
                .strip_prefix(workspace)
                .map_err(|_| format!("{} is outside the workspace", path.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            let lines: Vec<&str> = text.lines().collect();
            for (index, line) in lines.iter().enumerate() {
                if !line.contains("visibility::make(pub)") {
                    continue;
                }
                let mut found = None;
                for next in lines.iter().skip(index + 1).take(6) {
                    if let Some(name) = declaration_name(next) {
                        found = Some(name);
                        break;
                    }
                }
                let name = found.ok_or_else(|| {
                    format!(
                        "{relative}:{}: visibility::make(pub) without a declaration to name",
                        index + 1
                    )
                })?;
                sites.push((relative.clone(), name));
            }
        }
    }
    Ok(sites)
}

fn declaration_name(line: &str) -> Option<String> {
    let mut rest = line.trim_start();
    // Skip visibility prefixes (`pub`, `pub(crate)`, `pub(in path)`, ...).
    if let Some(after_pub) = rest.strip_prefix("pub ") {
        rest = after_pub.trim_start();
    } else if let Some(after_pub) = rest.strip_prefix("pub(") {
        let close = after_pub.find(')')?;
        rest = after_pub[close + 1..].trim_start();
    }
    // Skip item qualifiers in any order: `const`, `async`, `unsafe`, `default`.
    loop {
        let mut advanced = false;
        for qualifier in ["const ", "async ", "unsafe ", "default "] {
            if let Some(tail) = rest.strip_prefix(qualifier) {
                rest = tail.trim_start();
                advanced = true;
            }
        }
        if !advanced {
            break;
        }
    }
    for keyword in [
        "fn ", "struct ", "enum ", "union ", "trait ", "type ", "static ", "mod ",
    ] {
        if let Some(tail) = rest.strip_prefix(keyword) {
            let name: String = tail
                .trim_start()
                .chars()
                .take_while(|character| character.is_alphanumeric() || *character == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

fn check_visibility_coverage(workspace: &Path, ledgers: &[VisRow]) -> Result<(), String> {
    let mut listed: BTreeSet<(String, String)> = BTreeSet::new();
    for row in ledgers {
        let leaf = row.item.rsplit("::").next().unwrap_or(&row.item);
        listed.insert((row.file.clone(), leaf.to_owned()));
    }
    for (file, item) in visibility_make_sites(workspace)? {
        if !listed.contains(&(file.clone(), item.clone())) {
            return Err(format!(
                "{file}: `{item}` carries visibility::make(pub) but has no TEST-SUPPORT-VIS row"
            ));
        }
    }
    Ok(())
}

fn validate(workspace: &Path) -> Result<(), String> {
    let architecture = workspace.join(ARCHITECTURE_DIR);
    let mut ledger_paths = Vec::new();
    for path in read_dir_sorted(&architecture)? {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with(VIS_PREFIX) && name.ends_with(".tsv") {
            ledger_paths.push(path);
        }
    }
    if ledger_paths.is_empty() {
        return Ok(());
    }
    let fixtures = fixture_names(workspace)?;
    let authority = authority_items(workspace)?;
    let mut all_rows = Vec::new();
    for path in ledger_paths {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| format!("{}: bad ledger name", path.display()))?;
        let ceiling_path = architecture.join(format!("{stem}.ceiling"));
        let ceiling_text = read(&ceiling_path)?;
        let ceiling: usize = ceiling_text.trim().parse().map_err(|_| {
            format!(
                "{}: expected one nonnegative integer",
                ceiling_path.display()
            )
        })?;
        let rows = parse_ledger(&path, &read(&path)?)?;
        if rows.len() > ceiling {
            return Err(format!(
                "{}: {} rows exceed the TEST-SUPPORT-VIS ceiling {ceiling}",
                path.display(),
                rows.len()
            ));
        }
        for row in &rows {
            check_row(workspace, row, &fixtures, &authority)?;
        }
        all_rows.extend(rows);
    }
    check_visibility_coverage(workspace, &all_rows)
}

#[test]
fn test_support_vis_ledgers_obey_the_signed_class() {
    let workspace = workspace_root();
    validate(&workspace).unwrap_or_else(|error| panic!("TEST-SUPPORT-VIS: {error}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(sources: &[&str]) -> FixtureNames {
        let mut names = FixtureNames::default();
        for source in sources {
            collect_fixture_names(&code_only(source), &mut names);
        }
        names
    }

    #[test]
    fn v1_rejects_type_names_and_qualified_chains() {
        let fixtures = names(&[
            "fn f(pin: &InFlightRootPin) { let _ = InFlightRootPin::registry; }",
            "use neovm_core::error::SignalData;",
        ]);
        assert!(names_in_fixtures("error::InFlightRootPin", "struct", &fixtures).is_some());
        assert!(
            names_in_fixtures("error::InFlightRootPin::registry", "field", &fixtures).is_some()
        );
        assert!(names_in_fixtures("error::SignalData", "struct", &fixtures).is_some());
    }

    #[test]
    fn v1_ignores_bare_generic_field_and_method_words() {
        let fixtures = names(&["fn f(v: &mut Vec<u8>) { v.push(1); let x = v.len(); }"]);
        assert!(names_in_fixtures("error::ThrowData::new", "fn", &fixtures).is_none());
        assert!(
            names_in_fixtures(
                "heap_types::ordered::OrderedShiftTree::len",
                "fn",
                &fixtures
            )
            .is_none()
        );
        assert!(
            names_in_fixtures("error::InFlightRootPin::registry", "field", &fixtures).is_none()
        );
    }

    #[test]
    fn v1_ignores_comments() {
        let fixtures = names(&["// InFlightRootPin::registry is private\nfn f() {}"]);
        assert!(
            names_in_fixtures("error::InFlightRootPin::registry", "field", &fixtures).is_none()
        );
    }

    #[test]
    fn authority_matching_uses_type_segments_and_qualified_suffixes() {
        let authority = BTreeSet::from([
            "ColdWorldLease::from_erased".to_owned(),
            "WorldSlot".to_owned(),
            "EvalHooksInstalled".to_owned(),
        ]);
        assert!(names_authority_item("error::ColdWorldLease::from_erased", &authority).is_some());
        assert!(names_authority_item("world::WorldSlot", &authority).is_some());
        assert!(names_authority_item("hooks::EvalHooksInstalled", &authority).is_some());
        assert!(names_authority_item("error::ThrowData::new", &authority).is_none());
    }
}
