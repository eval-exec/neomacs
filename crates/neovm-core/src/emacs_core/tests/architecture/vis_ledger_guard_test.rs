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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};

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

/// The original lexical corpus. It remains the fallback for a fixture that
/// does not parse: macro-generated or otherwise unresolvable names must stay
/// fail-closed, so those files keep the broad matcher.
#[derive(Default)]
struct FixtureNames {
    words: BTreeSet<String>,
    qualified: BTreeSet<String>,
    use_leaves: BTreeSet<String>,
}

/// One fixture or compiler golden's naming context. Resolution is per file:
/// a bare word must resolve to the item *there*, never through an import in
/// another fixture.
struct FixtureScope {
    label: String,
    /// `use path::Name [as Alias];` bindings: local name -> the named path.
    imports: Vec<(String, Vec<String>)>,
    /// `use module::*;` module paths.
    globs: Vec<Vec<String>>,
    /// Type-namespace names declared in this file (items and generic type
    /// parameters). A bare word with one of these names resolves locally.
    bindings: BTreeSet<String>,
    /// Reference-position identifiers, flagged when the occurrence sits in a
    /// generic bracket of a compiler golden.
    words: Vec<(String, bool)>,
    /// Qualified `A::B::C` chains, as written.
    chains: Vec<Vec<String>>,
    /// Compiler goldens render library generic parameters as bare words
    /// (`SubrObj::<Entry>`); such an occurrence does not reference an item
    /// named `Entry`.
    generic_exempt: bool,
    /// An unparseable file keeps the lexical matcher, fail-closed.
    broad: Option<FixtureNames>,
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

#[derive(Default)]
struct UseCollector {
    imports: Vec<(String, Vec<String>)>,
    globs: Vec<Vec<String>>,
}

fn collect_use_tree(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    imports: &mut Vec<(String, Vec<String>)>,
    globs: &mut Vec<Vec<String>>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_use_tree(&path.tree, prefix, imports, globs);
            prefix.pop();
        }
        syn::UseTree::Name(name) => {
            let local = name.ident.to_string();
            if local == "self" {
                if let Some(last) = prefix.last() {
                    imports.push((last.clone(), prefix.clone()));
                }
            } else {
                let mut full = prefix.clone();
                full.push(local.clone());
                imports.push((local, full));
            }
        }
        syn::UseTree::Rename(rename) => {
            let mut full = prefix.clone();
            full.push(rename.ident.to_string());
            imports.push((rename.rename.to_string(), full));
        }
        syn::UseTree::Glob(_) => {
            if !prefix.is_empty() {
                globs.push(prefix.clone());
            }
        }
        syn::UseTree::Group(group) => {
            for item in &group.items {
                collect_use_tree(item, prefix, imports, globs);
            }
        }
    }
}

impl<'ast> Visit<'ast> for UseCollector {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        collect_use_tree(
            &item.tree,
            &mut Vec::new(),
            &mut self.imports,
            &mut self.globs,
        );
    }
}

#[derive(Default)]
struct BindingCollector {
    names: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for BindingCollector {
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        self.names.insert(item.ident.to_string());
        visit::visit_item_struct(self, item);
    }
    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        self.names.insert(item.ident.to_string());
        visit::visit_item_enum(self, item);
    }
    fn visit_item_union(&mut self, item: &'ast syn::ItemUnion) {
        self.names.insert(item.ident.to_string());
        visit::visit_item_union(self, item);
    }
    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        self.names.insert(item.ident.to_string());
        visit::visit_item_trait(self, item);
    }
    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        self.names.insert(item.ident.to_string());
        visit::visit_item_type(self, item);
    }
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.names.insert(item.ident.to_string());
        visit::visit_item_mod(self, item);
    }
    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        self.names.insert(item.ident.to_string());
        visit::visit_item_extern_crate(self, item);
    }
    fn visit_generics(&mut self, generics: &'ast syn::Generics) {
        for parameter in &generics.params {
            if let syn::GenericParam::Type(parameter) = parameter {
                self.names.insert(parameter.ident.to_string());
            }
        }
        visit::visit_generics(self, generics);
    }
}

/// Reference-position identifiers and qualified chains. A path component
/// (`A::B`), a field/method name (`value.entry`) or an import prefix is not a
/// standalone reference. The flag is true when the identifier sits inside a
/// generic bracket; the matcher trusts that flag only for compiler goldens.
fn lexical_names(source: &str) -> (Vec<(String, bool)>, Vec<Vec<String>>) {
    fn generic_bracket(source: &str, at: usize) -> bool {
        let bytes = source.as_bytes();
        let mut index = at + 1;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        if bytes.get(index) == Some(&b'\'') {
            return true;
        }
        if !bytes.get(index).is_some_and(|byte| is_ident_start(*byte)) {
            return false;
        }
        while bytes.get(index).is_some_and(|byte| is_ident_byte(*byte)) {
            index += 1;
        }
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        matches!(bytes.get(index), Some(b':' | b',' | b'>' | b'<' | b'='))
    }

    let bytes = source.as_bytes();
    let mut words = Vec::new();
    let mut chains = Vec::new();
    let mut depth = 0usize;
    let mut previous: Option<u8> = None;
    let mut before_previous: Option<u8> = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'<' {
            if generic_bracket(source, index) {
                depth += 1;
            }
            before_previous = previous;
            previous = Some(byte);
            index += 1;
            continue;
        }
        if byte == b'>' {
            if previous != Some(b'-') && depth > 0 {
                depth -= 1;
            }
            before_previous = previous;
            previous = Some(byte);
            index += 1;
            continue;
        }
        if !is_ident_start(byte) {
            if !byte.is_ascii_whitespace() {
                before_previous = previous;
                previous = Some(byte);
            }
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_ident_byte(bytes[index]) {
            index += 1;
        }
        let root = &source[start..index];
        // `A::B` components and `value.entry` names are not standalone
        // references; a single `:` (a type annotation) is not path syntax.
        let component =
            (previous == Some(b':') && before_previous == Some(b':')) || previous == Some(b'.');
        let mut chain = vec![root.to_owned()];
        let mut end = index;
        while bytes.get(end) == Some(&b':') && bytes.get(end + 1) == Some(&b':') {
            let mut next = end + 2;
            while bytes
                .get(next)
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                next += 1;
            }
            if next >= bytes.len() || !is_ident_start(bytes[next]) {
                break;
            }
            let piece_start = next;
            while next < bytes.len() && is_ident_byte(bytes[next]) {
                next += 1;
            }
            chain.push(source[piece_start..next].to_owned());
            end = next;
        }
        if !component {
            words.push((root.to_owned(), depth > 0));
        }
        if chain.len() > 1 {
            chains.push(chain);
        }
        index = end.max(index);
        before_previous = previous;
        previous = bytes.get(index.saturating_sub(1)).copied();
    }
    (words, chains)
}

fn fixture_scope(label: String, source: &str, rust: bool, generic_exempt: bool) -> FixtureScope {
    let code = if rust {
        code_only(source)
    } else {
        source.to_owned()
    };
    let (words, chains) = lexical_names(&code);
    let mut scope = FixtureScope {
        label,
        imports: Vec::new(),
        globs: Vec::new(),
        bindings: BTreeSet::new(),
        words,
        chains,
        generic_exempt,
        broad: None,
    };
    if rust {
        match syn::parse_file(&code) {
            Ok(file) => {
                let mut uses = UseCollector::default();
                uses.visit_file(&file);
                scope.imports = uses.imports;
                scope.globs = uses.globs;
                let mut bindings = BindingCollector::default();
                bindings.visit_file(&file);
                scope.bindings = bindings.names;
            }
            Err(_) => {
                let mut broad = FixtureNames::default();
                collect_fixture_names(&code, &mut broad);
                scope.broad = Some(broad);
            }
        }
    }
    scope
}

fn fixture_scopes(workspace: &Path) -> Result<Vec<FixtureScope>, String> {
    let tests = workspace.join("crates/neovm-core/tests");
    let mut rust_files = Vec::new();
    for dir in FIXTURE_DIRS {
        walk_files(&tests.join(dir), &mut rust_files)?;
    }
    let mut scopes = Vec::new();
    let mut fixture_index: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for path in rust_files {
        if !path.extension().is_some_and(|extension| extension == "rs") {
            continue;
        }
        let source = read(&path)?;
        let label = path
            .strip_prefix(workspace)
            .unwrap_or(&path)
            .display()
            .to_string();
        fixture_index.insert(path, scopes.len());
        scopes.push(fixture_scope(label, &source, true, false));
    }
    let mut all_files = Vec::new();
    walk_files(&tests, &mut all_files)?;
    for path in all_files {
        if !path
            .extension()
            .is_some_and(|extension| extension == "stderr")
        {
            continue;
        }
        let source = read(&path)?;
        let label = path
            .strip_prefix(workspace)
            .unwrap_or(&path)
            .display()
            .to_string();
        let mut scope = fixture_scope(label, &source, false, true);
        let sibling = path.with_extension("rs");
        if let Some(index) = fixture_index.get(&sibling) {
            let paired = &scopes[*index];
            scope.imports.clone_from(&paired.imports);
            scope.globs.clone_from(&paired.globs);
            scope.bindings.clone_from(&paired.bindings);
        }
        scopes.push(scope);
    }
    Ok(scopes)
}

fn path_ends_with(path: &[String], item: &[&str]) -> bool {
    path.len() >= item.len()
        && path[path.len() - item.len()..]
            .iter()
            .zip(item)
            .all(|(word, expected)| word == expected)
}

fn item_suffix_matches(chain: &[String], item: &[&str]) -> bool {
    (2..=item.len().min(4)).any(|suffix_len| {
        chain.len() >= suffix_len
            && chain[chain.len() - suffix_len..]
                .iter()
                .zip(&item[item.len() - suffix_len..])
                .all(|(word, expected)| word == expected)
    })
}

fn resolve_chain(chain: &[String], imports: &[(String, Vec<String>)]) -> Vec<String> {
    let Some(root) = chain.first() else {
        return Vec::new();
    };
    let Some((_, path)) = imports.iter().find(|(local, _)| local == root) else {
        return chain.to_vec();
    };
    let mut resolved = path.clone();
    resolved.extend_from_slice(&chain[1..]);
    resolved
}

fn glob_covers_item_module(glob: &[String], item: &[&str]) -> bool {
    let parent = &item[..item.len().saturating_sub(1)];
    !glob.is_empty() && !parent.is_empty() && path_ends_with(glob, parent)
}

/// Whether a reference-position `word` can name `item` in `scope`.
fn word_names_item(scope: &FixtureScope, word: &str, item: &[&str]) -> bool {
    let explicit: Vec<&Vec<String>> = scope
        .imports
        .iter()
        .filter(|(local, _)| local == word)
        .map(|(_, path)| path)
        .collect();
    match explicit.as_slice() {
        // A single local import resolves the bare name, here or elsewhere.
        [path] => path_ends_with(path, item),
        // Two imports of one name are already ambiguous.
        [_, _, ..] => true,
        [] => {
            if scope
                .globs
                .iter()
                .any(|glob| glob_covers_item_module(glob, item))
            {
                // A glob of the item's module could supply the name.
                return true;
            }
            if scope.bindings.contains(word) {
                return false;
            }
            // No import and no local binding: the name is unresolvable here.
            // The class stays fail-closed.
            true
        }
    }
}

/// The lexical fallback for a file that does not parse.
fn lexical_hit(item: &str, kind: &str, names: &FixtureNames) -> Option<String> {
    let segments: Vec<&str> = item.split("::").collect();
    let leaf = *segments.last()?;
    match kind {
        "struct" | "enum" | "union" | "trait" | "type" => {
            if names.words.contains(leaf) || names.use_leaves.contains(leaf) {
                return Some(format!("type name `{leaf}` appears in the file"));
            }
        }
        "mod" => {
            if names.use_leaves.contains(leaf) {
                return Some(format!("module `{leaf}` is imported by the file"));
            }
        }
        _ => {}
    }
    for suffix_len in 2..=segments.len().min(4) {
        let suffix = segments[segments.len() - suffix_len..].join("::");
        for chain in &names.qualified {
            if chain == &suffix || chain.ends_with(&format!("::{suffix}")) {
                return Some(format!("qualified name `{suffix}` appears as `{chain}`"));
            }
        }
    }
    None
}

/// Whether any fixture or golden resolves a name to `item`. A type name is
/// matched only through resolution in its own file: an explicit `use` of the
/// item, a glob of the item's module, a qualified path, or a bare word that
/// resolves to the item there. Unparseable files keep the lexical fallback,
/// fail-closed.
fn names_in_fixtures(item: &str, kind: &str, fixtures: &[FixtureScope]) -> Option<String> {
    let segments: Vec<&str> = item.split("::").collect();
    let leaf = *segments.last()?;
    for scope in fixtures {
        if let Some(broad) = &scope.broad {
            if let Some(why) = lexical_hit(item, kind, broad) {
                return Some(format!("{}: {why}", scope.label));
            }
            continue;
        }
        for (_, path) in &scope.imports {
            if path_ends_with(path, &segments) {
                return Some(format!(
                    "{}: `use {}` names the item",
                    scope.label,
                    path.join("::")
                ));
            }
        }
        if matches!(kind, "struct" | "enum" | "union" | "trait" | "type") {
            for (word, generic) in &scope.words {
                if word != leaf {
                    continue;
                }
                if scope.generic_exempt && *generic {
                    continue;
                }
                if word_names_item(scope, word, &segments) {
                    return Some(format!(
                        "{}: bare `{word}` resolves to the item",
                        scope.label
                    ));
                }
            }
        }
        if kind != "mod" {
            for chain in &scope.chains {
                let resolved = resolve_chain(chain, &scope.imports);
                if item_suffix_matches(&resolved, &segments) {
                    return Some(format!(
                        "{}: qualified name `{}` resolves to the item",
                        scope.label,
                        resolved.join("::")
                    ));
                }
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
    fixtures: &[FixtureScope],
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
    let fixtures = fixture_scopes(workspace)?;
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

    fn scopes(sources: &[&str]) -> Vec<FixtureScope> {
        sources
            .iter()
            .enumerate()
            .map(|(index, source)| fixture_scope(format!("fixture{index}.rs"), source, true, false))
            .collect()
    }

    #[test]
    fn v1_rejects_type_names_and_qualified_chains() {
        let fixtures = scopes(&[
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
        let fixtures = scopes(&["fn f(v: &mut Vec<u8>) { v.push(1); let x = v.len(); }"]);
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
        let fixtures = scopes(&["// InFlightRootPin::registry is private\nfn f() {}"]);
        assert!(
            names_in_fixtures("error::InFlightRootPin::registry", "field", &fixtures).is_none()
        );
    }

    #[test]
    fn v1_planted_import_turns_the_check_red_then_restores_green() {
        let planted =
            scopes(&["use neovm_text::emacs_core::syntax::parse_loop::Entry;\nfn f(_: Entry) {}"]);
        let error = names_in_fixtures("emacs_core::syntax::parse_loop::Entry", "struct", &planted)
            .expect("a planted import of the item must be refused");
        eprintln!("MUTATION RED: {error}");
        let restored = scopes(&["enum Entry { A }\nfn f(_: Entry) {}"]);
        assert!(
            names_in_fixtures("emacs_core::syntax::parse_loop::Entry", "struct", &restored)
                .is_none()
        );
        eprintln!("RESTORED GREEN: local Entry resolves locally");
    }

    #[test]
    fn v1_resolves_bare_names_and_globs_in_their_own_file() {
        // A glob of the item's module is ambiguous and stays fail-closed.
        let glob =
            scopes(&["use neovm_text::emacs_core::syntax::parse_loop::*;\nfn f(_: Entry) {}"]);
        assert!(
            names_in_fixtures("emacs_core::syntax::parse_loop::Entry", "struct", &glob).is_some()
        );
        // An aliased module chain resolves to the item.
        let alias = scopes(&[
            "use neovm_text::emacs_core::syntax::parse_loop as pl;\nfn f(_: pl::Entry) {}",
        ]);
        assert!(
            names_in_fixtures("emacs_core::syntax::parse_loop::Entry", "struct", &alias).is_some()
        );
        // Another item's qualified path with the same leaf is not this item.
        let other = scopes(&["fn f() { let _ = CanonicalArm::Plain; }"]);
        assert!(
            names_in_fixtures("emacs_core::syntax::parse_loop::Plain", "struct", &other).is_none()
        );
        // A local import that resolves elsewhere is not this item.
        let elsewhere = scopes(&["use other::Entry;\nfn f(_: Entry) {}"]);
        assert!(
            names_in_fixtures(
                "emacs_core::syntax::parse_loop::Entry",
                "struct",
                &elsewhere
            )
            .is_none()
        );
    }

    #[test]
    fn v1_golden_generic_parameters_are_not_item_references() {
        let golden = vec![fixture_scope(
            "golden.stderr".to_owned(),
            "note: required by a bound in `SubrObj`\n\
             | pub struct SubrObj<Entry: Copy + 'static> {\n\
             error: call to `SubrObj::<Entry>::new_unchecked` is unsafe",
            false,
            true,
        )];
        assert!(
            names_in_fixtures("emacs_core::syntax::parse_loop::Entry", "struct", &golden).is_none()
        );
    }

    #[test]
    fn v1_unparseable_files_keep_the_lexical_fallback() {
        let broken = scopes(&["not rust {{{ InFlightRootPin {"]);
        assert!(names_in_fixtures("error::InFlightRootPin", "struct", &broken).is_some());
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
