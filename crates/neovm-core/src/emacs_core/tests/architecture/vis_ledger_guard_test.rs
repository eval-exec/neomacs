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
    "use",
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

/// A type name that can flow into a field access, as written in the fixture.
#[derive(Clone, Debug)]
enum TypeCandidate {
    /// A type path, for example `Context` or `crate::eval::Context`.
    Path(Vec<String>),
    /// The base expression's type cannot be determined; the field arm treats
    /// this as ambiguous and fail-closed.
    Unknown,
}

/// The privacy-class view of a compiler golden: only the text inside error
/// blocks whose class visibility can cause (E0603/E0624/E0451/E0616/E0423, or
/// a private / not-publicly-re-exported note). Auto-trait E0277 obligation
/// notes are excluded: visibility cannot change auto traits (peer 07:4xZ).
struct GoldenPrivacy {
    words: Vec<(String, bool)>,
    chains: Vec<Vec<String>>,
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
    /// Named-field reads (`.field`): field name -> candidate base types. The
    /// base type is resolved as far as the fixture's own annotations and
    /// initializers allow; anything else stays `Unknown` (fail-closed).
    field_reads: Vec<(String, Vec<TypeCandidate>)>,
    /// Type paths used in struct literals (`S { .. }`) and struct patterns
    /// (`let S { .. }`).
    struct_uses: Vec<Vec<String>>,
    /// Textual `.field` mentions (method calls excluded). Compiler goldens and
    /// unparseable files use this fail-closed view.
    field_mentions: BTreeSet<String>,
    /// Compiler goldens render library generic parameters as bare words
    /// (`SubrObj::<Entry>`); such an occurrence does not reference an item
    /// named `Entry`.
    generic_exempt: bool,
    /// A compiler golden's privacy-class block view, used by ITEM rows.
    golden_privacy: Option<GoldenPrivacy>,
    /// An unparseable file keeps the lexical matcher, fail-closed.
    broad: Option<FixtureNames>,
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn is_test_path(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str().to_str() == Some("tests"))
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "tests.rs" || name.ends_with("_test.rs"))
}

#[derive(Clone, Copy, PartialEq)]
enum CfgTruth {
    True,
    False,
    Unknown,
}

/// `test = false`, `feature = "test-support" = false`: only a predicate that
/// is definitively false marks a test-only arm.
fn cfg_truth(meta: &syn::Meta) -> CfgTruth {
    match meta {
        syn::Meta::Path(path) if path.is_ident("test") => CfgTruth::False,
        syn::Meta::NameValue(value) if value.path.is_ident("feature") => {
            if let syn::Expr::Lit(expr) = &value.value {
                if let syn::Lit::Str(name) = &expr.lit {
                    if name.value() == "test-support" {
                        return CfgTruth::False;
                    }
                }
            }
            CfgTruth::Unknown
        }
        syn::Meta::List(list) if list.path.is_ident("not") => {
            match list.parse_args::<syn::Meta>() {
                Ok(inner) => match cfg_truth(&inner) {
                    CfgTruth::True => CfgTruth::False,
                    CfgTruth::False => CfgTruth::True,
                    CfgTruth::Unknown => CfgTruth::Unknown,
                },
                Err(_) => CfgTruth::Unknown,
            }
        }
        syn::Meta::List(list) if list.path.is_ident("any") || list.path.is_ident("all") => {
            let any = list.path.is_ident("any");
            match list.parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            ) {
                Ok(metas) => {
                    let mut unknown = false;
                    for meta in &metas {
                        match cfg_truth(meta) {
                            CfgTruth::True if any => return CfgTruth::True,
                            CfgTruth::False if !any => return CfgTruth::False,
                            CfgTruth::Unknown => unknown = true,
                            _ => {}
                        }
                    }
                    if unknown {
                        CfgTruth::Unknown
                    } else if any {
                        CfgTruth::False
                    } else {
                        CfgTruth::True
                    }
                }
                Err(_) => CfgTruth::Unknown,
            }
        }
        _ => CfgTruth::Unknown,
    }
}

fn test_gated(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Meta>()
                .map(|meta| cfg_truth(&meta) == CfgTruth::False)
                .unwrap_or(false)
    })
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

fn path_segments(path: &syn::Path) -> Vec<String> {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect()
}

/// The `.field` mentions in a source text. `.name(` is a method call, not a
/// field read; Rust fixtures have already had comments and literals blanked.
fn field_mentions(source: &str) -> BTreeSet<String> {
    let bytes = source.as_bytes();
    let mut mentions = BTreeSet::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'.'
            || !bytes
                .get(index + 1)
                .is_some_and(|byte| is_ident_start(*byte))
        {
            index += 1;
            continue;
        }
        let start = index + 1;
        let mut end = start;
        while end < bytes.len() && is_ident_byte(bytes[end]) {
            end += 1;
        }
        let mut after = end;
        while bytes
            .get(after)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            after += 1;
        }
        if bytes.get(after) != Some(&b'(') {
            mentions.insert(source[start..end].to_owned());
        }
        index = end;
    }
    mentions
}

/// Resolve the type of a field-access base as far as local annotations and
/// initializers allow. Field reads in fixtures are privacy probes: a base
/// whose type cannot be determined stays ambiguous (fail-closed).
#[derive(Default)]
struct FieldCollector {
    types: BTreeMap<String, Vec<TypeCandidate>>,
    self_type: Option<Vec<String>>,
    field_reads: Vec<(String, Vec<TypeCandidate>)>,
    struct_uses: Vec<Vec<String>>,
}

impl FieldCollector {
    fn add_type(&mut self, name: String, candidates: &[TypeCandidate]) {
        let entry = self.types.entry(name).or_default();
        if candidates.is_empty() {
            entry.push(TypeCandidate::Unknown);
        } else {
            entry.extend(candidates.iter().cloned());
        }
    }

    fn bind_pattern(&mut self, pattern: &syn::Pat, candidates: &[TypeCandidate]) {
        match pattern {
            syn::Pat::Ident(_) => {
                for name in pattern_idents(pattern) {
                    self.add_type(name, candidates);
                }
            }
            syn::Pat::Type(typed) => {
                let candidates = self.type_candidates(&typed.ty);
                self.bind_pattern(&typed.pat, &candidates);
            }
            _ => {
                // A compound pattern binds fields/elements whose own types are
                // not modelled; keep them ambiguous.
                for name in pattern_idents(pattern) {
                    self.add_type(name, &[TypeCandidate::Unknown]);
                }
            }
        }
    }

    fn type_candidates(&self, ty: &syn::Type) -> Vec<TypeCandidate> {
        match ty {
            syn::Type::Path(path) => {
                let segments = path_segments(&path.path);
                if segments.len() == 1 && segments[0] == "Self" {
                    return match &self.self_type {
                        Some(ty) => vec![TypeCandidate::Path(ty.clone())],
                        None => vec![TypeCandidate::Unknown],
                    };
                }
                vec![TypeCandidate::Path(segments)]
            }
            syn::Type::Reference(reference) => self.type_candidates(&reference.elem),
            syn::Type::Paren(paren) => self.type_candidates(&paren.elem),
            syn::Type::Group(group) => self.type_candidates(&group.elem),
            _ => vec![TypeCandidate::Unknown],
        }
    }

    fn expr_candidates(&self, expression: &syn::Expr) -> Vec<TypeCandidate> {
        match expression {
            syn::Expr::Path(path) => {
                let segments = path_segments(&path.path);
                if segments.len() == 1 {
                    if segments[0] == "self" || segments[0] == "Self" {
                        return match &self.self_type {
                            Some(ty) => vec![TypeCandidate::Path(ty.clone())],
                            None => vec![TypeCandidate::Unknown],
                        };
                    }
                    self.types
                        .get(&segments[0])
                        .cloned()
                        .unwrap_or_else(|| vec![TypeCandidate::Unknown])
                } else {
                    // A multi-segment path in expression position is a const
                    // or associated item; its type is not modelled.
                    vec![TypeCandidate::Unknown]
                }
            }
            syn::Expr::Struct(expression) => {
                vec![TypeCandidate::Path(path_segments(&expression.path))]
            }
            syn::Expr::Reference(reference) => self.expr_candidates(&reference.expr),
            syn::Expr::Paren(paren) => self.expr_candidates(&paren.expr),
            syn::Expr::Group(group) => self.expr_candidates(&group.expr),
            syn::Expr::Cast(cast) => self.type_candidates(&cast.ty),
            syn::Expr::Try(try_block) => self.expr_candidates(&try_block.expr),
            syn::Expr::Await(await_expression) => self.expr_candidates(&await_expression.base),
            syn::Expr::Call(call) => {
                // `S::default()` / `S::new(..)` name the type in their
                // prefix; a free function's return type is unknown.
                if let syn::Expr::Path(path) = call.func.as_ref() {
                    let segments = path_segments(&path.path);
                    if segments.len() >= 2 {
                        return vec![TypeCandidate::Path(segments[..segments.len() - 1].to_vec())];
                    }
                }
                vec![TypeCandidate::Unknown]
            }
            _ => vec![TypeCandidate::Unknown],
        }
    }
}

fn pattern_idents(pattern: &syn::Pat) -> Vec<String> {
    #[derive(Default)]
    struct Idents(Vec<String>);
    impl<'ast> Visit<'ast> for Idents {
        fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
            self.0.push(pattern.ident.to_string());
            visit::visit_pat_ident(self, pattern);
        }
    }
    let mut idents = Idents::default();
    idents.visit_pat(pattern);
    idents.0
}

impl<'ast> Visit<'ast> for FieldCollector {
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let previous = self.self_type.clone();
        if let syn::Type::Path(path) = &*item.self_ty {
            self.self_type = Some(path_segments(&path.path));
        }
        visit::visit_item_impl(self, item);
        self.self_type = previous;
    }
    fn visit_fn_arg(&mut self, argument: &'ast syn::FnArg) {
        if let syn::FnArg::Typed(typed) = argument {
            let candidates = self.type_candidates(&typed.ty);
            self.bind_pattern(&typed.pat, &candidates);
        }
        visit::visit_fn_arg(self, argument);
    }
    fn visit_pat_type(&mut self, pattern: &'ast syn::PatType) {
        let candidates = self.type_candidates(&pattern.ty);
        self.bind_pattern(&pattern.pat, &candidates);
        visit::visit_pat_type(self, pattern);
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        let candidates = match &local.pat {
            syn::Pat::Type(typed) => self.type_candidates(&typed.ty),
            _ => match &local.init {
                Some(init) => self.expr_candidates(&init.expr),
                None => vec![TypeCandidate::Unknown],
            },
        };
        self.bind_pattern(&local.pat, &candidates);
        visit::visit_local(self, local);
    }
    fn visit_expr_field(&mut self, field: &'ast syn::ExprField) {
        if let syn::Member::Named(name) = &field.member {
            let candidates = self.expr_candidates(&field.base);
            self.field_reads.push((name.to_string(), candidates));
        }
        visit::visit_expr_field(self, field);
    }
    fn visit_expr_struct(&mut self, expression: &'ast syn::ExprStruct) {
        self.struct_uses.push(path_segments(&expression.path));
        visit::visit_expr_struct(self, expression);
    }
    fn visit_pat_struct(&mut self, pattern: &'ast syn::PatStruct) {
        self.struct_uses.push(path_segments(&pattern.path));
        visit::visit_pat_struct(self, pattern);
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
        field_reads: Vec::new(),
        struct_uses: Vec::new(),
        field_mentions: field_mentions(&code),
        generic_exempt,
        golden_privacy: None,
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
                let mut fields = FieldCollector::default();
                fields.visit_file(&file);
                scope.field_reads = fields.field_reads;
                scope.struct_uses = fields.struct_uses;
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

/// The text of a golden's privacy-class error blocks only. A golden
/// occurrence is a V1 hit for an ITEM row only here: visibility cannot change
/// auto-trait impls, so an E0277 Send/Sync obligation note is not a hit.
fn privacy_error_blocks(source: &str) -> String {
    fn privacy_line(line: &str) -> bool {
        ["E0603", "E0624", "E0451", "E0616", "E0423"]
            .iter()
            .any(|code| line.contains(code))
            || line.contains("private")
            || line.contains("not publicly re-exported")
    }
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let is_header = trimmed.starts_with("error[")
            || trimmed.starts_with("error:")
            || trimmed.starts_with("warning[")
            || trimmed.starts_with("warning:");
        if is_header && !current.is_empty() {
            blocks.push(std::mem::take(&mut current));
        }
        current.push(line);
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    let mut text = String::new();
    for block in blocks {
        if block.iter().any(|line| privacy_line(line)) {
            for line in block {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    text
}

/// A compiler golden's scope: the full text still feeds the field arm, while
/// `golden_privacy` is the only view ITEM rows may match.
fn golden_scope(label: String, source: &str) -> FixtureScope {
    let mut scope = fixture_scope(label, source, false, true);
    let (words, chains) = lexical_names(&privacy_error_blocks(source));
    scope.golden_privacy = Some(GoldenPrivacy { words, chains });
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
        let mut scope = golden_scope(label, &source);
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

/// Whether a type path written in a fixture names the row's struct, using the
/// same per-file resolution as item rows.
fn type_path_names_struct(scope: &FixtureScope, path: &[String], parent: &[&str]) -> bool {
    if path.is_empty() {
        return true;
    }
    if path.len() == 1 {
        return word_names_item(scope, &path[0], parent);
    }
    path_ends_with(path, parent) || item_suffix_matches(path, parent)
}

/// The named fields declared for `struct_name` in a row's defining file, one
/// set per declaration (cfg arms deduplicate by name). A missing or unnamed
/// struct is an error: a field row must name a struct whose field set is
/// decidable.
struct DeclaredStructs<'a> {
    name: &'a str,
    fields: Vec<BTreeSet<String>>,
}

impl<'ast> Visit<'ast> for DeclaredStructs<'_> {
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        if item.ident == self.name {
            let mut names = BTreeSet::new();
            let mut unnamed = false;
            for field in &item.fields {
                match &field.ident {
                    Some(ident) => {
                        names.insert(ident.to_string());
                    }
                    None => unnamed = true,
                }
            }
            if !unnamed && !names.is_empty() {
                self.fields.push(names);
            }
        }
        visit::visit_item_struct(self, item);
    }
}

fn declared_struct_fields(
    source: &str,
    struct_name: &str,
) -> Result<Vec<BTreeSet<String>>, String> {
    let file = syn::parse_file(source)
        .map_err(|error| format!("field row defining file does not parse: {error}"))?;
    let mut structs = DeclaredStructs {
        name: struct_name,
        fields: Vec::new(),
    };
    structs.visit_file(&file);
    if structs.fields.is_empty() {
        return Err(format!(
            "field row names no named-field struct `{struct_name}` in its defining file"
        ));
    }
    Ok(structs.fields)
}

/// The field arm's struct identity: the row's crate, the defining file from the
/// ledger row, and the struct name. The parent-path spelling is excluded on
/// purpose: rows written by different lanes may spell the same struct
/// differently (`Context` vs `emacs_core::eval::Context`), and rule (b) must
/// see every widened field of the struct rather than one spelling's subset.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FieldStructKey {
    crate_dir: String,
    file: String,
    struct_name: String,
}

/// Collect the widened-field set per struct identity across every ledger.
/// Mixing parent-path spellings for one identity is an error: silently
/// splitting the set would make rule (b) fail open.
fn widened_field_sets(
    ledgers: &[Vec<VisRow>],
) -> Result<BTreeMap<FieldStructKey, BTreeSet<String>>, String> {
    let mut widened: BTreeMap<FieldStructKey, BTreeSet<String>> = BTreeMap::new();
    let mut spellings: BTreeMap<FieldStructKey, BTreeSet<String>> = BTreeMap::new();
    for rows in ledgers {
        for row in rows {
            if row.kind != "field" {
                continue;
            }
            let Some((parent, field)) = row.item.rsplit_once("::") else {
                return Err(format!(
                    "{} field {}: item has no parent type",
                    row.crate_dir, row.item
                ));
            };
            let struct_name = parent.rsplit("::").next().unwrap_or(parent);
            let key = FieldStructKey {
                crate_dir: row.crate_dir.clone(),
                file: row.file.clone(),
                struct_name: struct_name.to_owned(),
            };
            spellings
                .entry(key.clone())
                .or_default()
                .insert(parent.to_owned());
            widened.entry(key).or_default().insert(field.to_owned());
        }
    }
    for (key, spellings) in &spellings {
        if spellings.len() > 1 {
            return Err(format!(
                "{} field rows for struct `{}` in {} mix parent spellings: {}; \
                 use one canonical spelling",
                key.crate_dir,
                key.struct_name,
                key.file,
                spellings.iter().cloned().collect::<Vec<_>>().join(" / ")
            ));
        }
    }
    Ok(widened)
}

/// The field arm of V1 (peer ruling 04:2xZ). Field visibility cannot change a
/// struct's trait impls, so merely naming the type is not a hit. A field row
/// `S::f` is a hit iff a fixture
/// (a) reads `.f` on a base that can resolve to `S` (ambiguous -> hit), or
/// (b) constructs/destructures `S` while every field of `S` is widened.
fn field_hit(
    fixtures: &[FixtureScope],
    parent: &[&str],
    field: &str,
    declared: &[BTreeSet<String>],
    widened: &BTreeSet<String>,
) -> Option<String> {
    let parent_item = parent.join("::");
    let fully_widened = declared
        .iter()
        .any(|fields| !fields.is_empty() && fields.is_subset(widened));
    for scope in fixtures {
        if let Some(broad) = &scope.broad {
            if scope.field_mentions.contains(field)
                && lexical_hit(&parent_item, "struct", broad).is_some()
            {
                return Some(format!(
                    "{}: `.{}` mentions the row's struct in an unparseable file",
                    scope.label, field
                ));
            }
            continue;
        }
        if scope.generic_exempt {
            // A compiler golden cannot resolve a base type from its text.
            // Require both the `.field` mention and a resolved reference to
            // the struct.
            if scope.field_mentions.contains(field)
                && names_in_fixtures_scoped(
                    &parent_item,
                    "struct",
                    std::slice::from_ref(scope),
                    false,
                )
                .is_some()
            {
                return Some(format!(
                    "{}: `.{}` on the row's struct in a compiler golden",
                    scope.label, field
                ));
            }
            continue;
        }
        let mut observed = false;
        for (read, candidates) in &scope.field_reads {
            if read != field {
                continue;
            }
            observed = true;
            for candidate in candidates {
                match candidate {
                    TypeCandidate::Path(path) => {
                        if type_path_names_struct(scope, path, parent) {
                            return Some(format!(
                                "{}: `.{field}` read on a base that resolves to the row's struct",
                                scope.label
                            ));
                        }
                    }
                    TypeCandidate::Unknown => {
                        return Some(format!(
                            "{}: `.{field}` read on an unresolved base (fail-closed)",
                            scope.label
                        ));
                    }
                }
            }
        }
        if !observed && scope.field_mentions.contains(field) {
            // Not an analyzable field read: a macro token stream or attribute.
            return Some(format!(
                "{}: `.{}` text cannot be analyzed (fail-closed)",
                scope.label, field
            ));
        }
        if fully_widened {
            for path in &scope.struct_uses {
                if type_path_names_struct(scope, path, parent) {
                    return Some(format!(
                        "{}: constructs or destructures the row's struct while every field is widened",
                        scope.label
                    ));
                }
            }
        }
    }
    None
}

/// Decide a field row against the fixture corpus. The caller supplies the
/// row's defining source, already read by the row checks.
fn field_row_fixture_hit(
    row: &VisRow,
    source: &str,
    fixtures: &[FixtureScope],
    widened: &BTreeMap<FieldStructKey, BTreeSet<String>>,
) -> Result<Option<String>, String> {
    let Some((parent_item, field)) = row.item.rsplit_once("::") else {
        return Err(format!(
            "{} field {}: item has no parent type",
            row.crate_dir, row.item
        ));
    };
    let parent: Vec<&str> = parent_item.split("::").collect();
    let struct_name = parent
        .last()
        .copied()
        .ok_or_else(|| format!("{} field {}: empty parent type", row.crate_dir, row.item))?;
    let declared = declared_struct_fields(source, struct_name)?;
    let key = FieldStructKey {
        crate_dir: row.crate_dir.clone(),
        file: row.file.clone(),
        struct_name: struct_name.to_owned(),
    };
    let widened = widened.get(&key).cloned().unwrap_or_default();
    Ok(field_hit(fixtures, &parent, field, &declared, &widened))
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
    names_in_fixtures_scoped(item, kind, fixtures, true)
}

fn names_in_fixtures_scoped(
    item: &str,
    kind: &str,
    fixtures: &[FixtureScope],
    golden_privacy: bool,
) -> Option<String> {
    // Field rows use the separate field arm (peer ruling 04:2xZ): naming the
    // type is not a hit, so the item-path rule does not apply to them.
    if kind == "field" {
        return None;
    }
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
        // A golden's ITEM rows may match only its privacy-class blocks; the
        // fixture SOURCE and field-row paths keep the full view (07:4xZ).
        let (words, chains) = match (golden_privacy, scope.golden_privacy.as_ref()) {
            (true, Some(privacy)) => (&privacy.words, &privacy.chains),
            _ => (&scope.words, &scope.chains),
        };
        if matches!(kind, "struct" | "enum" | "union" | "trait" | "type") {
            for (word, generic) in words {
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
            for chain in chains {
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
    widened: &BTreeMap<FieldStructKey, BTreeSet<String>>,
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
    let fixture_hit = if row.kind == "field" {
        field_row_fixture_hit(row, &source, fixtures, widened)?
    } else {
        names_in_fixtures(&row.item, &row.kind, fixtures)
    };
    if let Some(why) = fixture_hit {
        return Err(format!("{what}: V1 forbids widening this item: {why}"));
    }
    if let Some(why) = names_authority_item(&row.item, authority) {
        return Err(format!("{what}: V1 forbids widening this item: {why}"));
    }
    Ok(())
}

/// Every `visibility::make(pub)` site in an extracted crate, as
/// `(workspace-relative file, item name)`.
/// Widenings the textual `visibility::make` scan cannot see: sibling cfg-split
/// declarations of one item or one re-export where a test-support arm is
/// public and a production arm is not (peer 07:4xZ; the ThreadingMode bypass).
fn cfg_split_widening_sites(workspace: &Path) -> Result<Vec<(String, String)>, String> {
    let mut sites = Vec::new();
    for crate_dir in VIS_CRATES {
        let src = workspace.join("crates").join(crate_dir).join("src");
        let mut files = Vec::new();
        walk_files(&src, &mut files)?;
        for path in files {
            if path.extension().is_none_or(|extension| extension != "rs") || is_test_path(&path) {
                continue;
            }
            let relative = path
                .strip_prefix(workspace)
                .map_err(|_| format!("{} is outside the workspace", path.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            let source = read(&path)?;
            let parsed =
                syn::parse_file(&source).map_err(|error| format!("{relative}: {error}"))?;
            collect_cfg_split_sites(&parsed.items, &relative, &mut sites);
        }
    }
    sites.sort();
    sites.dedup();
    Ok(sites)
}

fn record_cfg_split(
    declarations: &mut BTreeMap<(String, String), Vec<(bool, bool)>>,
    kind: &str,
    name: &str,
    vis: &syn::Visibility,
    attrs: &[syn::Attribute],
) {
    declarations
        .entry((kind.to_owned(), name.to_owned()))
        .or_default()
        .push((matches!(vis, syn::Visibility::Public(_)), test_gated(attrs)));
}

fn use_local_names(tree: &syn::UseTree) -> Vec<String> {
    fn expand(tree: &syn::UseTree, out: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(path) => expand(&path.tree, out),
            syn::UseTree::Name(name) => out.push(name.ident.to_string()),
            syn::UseTree::Rename(rename) => out.push(rename.rename.to_string()),
            syn::UseTree::Glob(_) => {}
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    expand(item, out);
                }
            }
        }
    }
    let mut names = Vec::new();
    expand(tree, &mut names);
    names
}

fn collect_cfg_split_sites(items: &[syn::Item], file: &str, out: &mut Vec<(String, String)>) {
    let mut declarations: BTreeMap<(String, String), Vec<(bool, bool)>> = BTreeMap::new();
    for item in items {
        match item {
            syn::Item::Mod(module) => {
                // Module visibility uses the documented cfg-split module form;
                // its rows are checked separately (RB-2 shell module) and the
                // ruling's item inventory is the item/re-export class.
                if let Some((_, inner)) = &module.content {
                    collect_cfg_split_sites(inner, file, out);
                }
            }
            syn::Item::Struct(item) => record_cfg_split(
                &mut declarations,
                "struct",
                &item.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            syn::Item::Enum(item) => record_cfg_split(
                &mut declarations,
                "enum",
                &item.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            syn::Item::Union(item) => record_cfg_split(
                &mut declarations,
                "union",
                &item.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            syn::Item::Trait(item) => record_cfg_split(
                &mut declarations,
                "trait",
                &item.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            syn::Item::Type(item) => record_cfg_split(
                &mut declarations,
                "type",
                &item.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            syn::Item::Fn(item) => record_cfg_split(
                &mut declarations,
                "fn",
                &item.sig.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            syn::Item::Const(item) => record_cfg_split(
                &mut declarations,
                "const",
                &item.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            syn::Item::Static(item) => record_cfg_split(
                &mut declarations,
                "static",
                &item.ident.to_string(),
                &item.vis,
                &item.attrs,
            ),
            syn::Item::Use(item) => {
                for name in use_local_names(&item.tree) {
                    record_cfg_split(&mut declarations, "use", &name, &item.vis, &item.attrs);
                }
            }
            _ => {}
        }
    }
    for ((_kind, name), arms) in declarations {
        let gated_public = arms.iter().any(|(public, gated)| *public && *gated);
        let production_private = arms.iter().any(|(public, gated)| !*public && !*gated);
        if gated_public && production_private {
            out.push((file.to_owned(), name));
        }
    }
}

fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Union(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        _ => &[],
    }
}

fn item_ident(item: &syn::Item) -> Option<&syn::Ident> {
    match item {
        syn::Item::Struct(item) => Some(&item.ident),
        syn::Item::Union(item) => Some(&item.ident),
        syn::Item::Enum(item) => Some(&item.ident),
        _ => None,
    }
}

fn normalize_cfg_attrs(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|attr| !attr.path().is_ident("cfg"));
}

/// CS-1 arm identity: drop the arm-selecting cfg attrs and every visibility
/// token, keeping derives, repr, docs, field order/names/types and all other
/// attrs intact for an exact AST comparison.
fn normalize_cfg_split_arm(item: &mut syn::Item) {
    match item {
        syn::Item::Struct(item) => {
            normalize_cfg_attrs(&mut item.attrs);
            item.vis = syn::Visibility::Inherited;
            for field in &mut item.fields {
                normalize_cfg_attrs(&mut field.attrs);
                field.vis = syn::Visibility::Inherited;
            }
        }
        syn::Item::Union(item) => {
            normalize_cfg_attrs(&mut item.attrs);
            item.vis = syn::Visibility::Inherited;
            for field in &mut item.fields.named {
                normalize_cfg_attrs(&mut field.attrs);
                field.vis = syn::Visibility::Inherited;
            }
        }
        syn::Item::Enum(item) => {
            normalize_cfg_attrs(&mut item.attrs);
            item.vis = syn::Visibility::Inherited;
            for variant in &mut item.variants {
                normalize_cfg_attrs(&mut variant.attrs);
                for field in &mut variant.fields {
                    normalize_cfg_attrs(&mut field.attrs);
                    field.vis = syn::Visibility::Inherited;
                }
            }
        }
        _ => {}
    }
}

fn named_field_list(item: &syn::Item) -> Option<Vec<&syn::Field>> {
    match item {
        syn::Item::Struct(item) => Some(item.fields.iter().collect()),
        syn::Item::Union(item) => Some(item.fields.named.iter().collect()),
        _ => None,
    }
}

fn widened_fields(production: &syn::Item, test: &syn::Item) -> Result<Vec<String>, String> {
    let (Some(production), Some(test)) = (named_field_list(production), named_field_list(test))
    else {
        return Ok(Vec::new());
    };
    if production.len() != test.len() {
        return Err("cfg-split arm field counts differ".to_owned());
    }
    let mut widened = Vec::new();
    for (production, test) in production.iter().zip(&test) {
        let production_public = matches!(production.vis, syn::Visibility::Public(_));
        let test_public = matches!(test.vis, syn::Visibility::Public(_));
        if production_public || !test_public {
            continue;
        }
        let Some(name) = &test.ident else {
            return Err("cfg-split arm widens an unnamed field".to_owned());
        };
        widened.push(name.to_string());
    }
    Ok(widened)
}

fn row_names_field(row: &VisRow, struct_name: &str, field: &str) -> bool {
    let segments: Vec<&str> = row.item.split("::").collect();
    segments.len() >= 2
        && segments[segments.len() - 2] == struct_name
        && segments[segments.len() - 1] == field
}

fn check_cfg_split_item_group(
    file: &str,
    name: &str,
    arms: &[&syn::Item],
    ledgers: &[VisRow],
) -> Result<(), String> {
    let tests: Vec<&syn::Item> = arms
        .iter()
        .copied()
        .filter(|item| test_gated(item_attrs(item)))
        .collect();
    let productions: Vec<&syn::Item> = arms
        .iter()
        .copied()
        .filter(|item| !test_gated(item_attrs(item)))
        .collect();
    if tests.is_empty() || productions.is_empty() {
        return Ok(());
    }
    for test in &tests {
        for production in &productions {
            let mut test_arm = (*test).clone();
            let mut production_arm = (*production).clone();
            normalize_cfg_split_arm(&mut test_arm);
            normalize_cfg_split_arm(&mut production_arm);
            if test_arm != production_arm {
                return Err(format!(
                    "{file}: cfg-split arms of `{name}` are not token-identical except visibility"
                ));
            }
        }
    }
    let Some(test) = tests.first() else {
        return Ok(());
    };
    let mut widened = BTreeSet::new();
    for production in &productions {
        widened.extend(widened_fields(production, test)?);
    }
    for field in widened {
        let rows: Vec<&VisRow> = ledgers
            .iter()
            .filter(|row| {
                row.kind == "field" && row.file == file && row_names_field(row, name, &field)
            })
            .collect();
        match rows.len() {
            1 => {}
            0 => {
                return Err(format!(
                    "{file}: cfg-split arm widens field `{name}::{field}` with no TEST-SUPPORT-VIS row"
                ));
            }
            _ => {
                return Err(format!(
                    "{file}: cfg-split field `{name}::{field}` has more than one TEST-SUPPORT-VIS row"
                ));
            }
        }
    }
    Ok(())
}

fn check_cfg_split_items(
    items: &[syn::Item],
    file: &str,
    ledgers: &[VisRow],
) -> Result<(), String> {
    let mut groups: BTreeMap<(String, String), Vec<&syn::Item>> = BTreeMap::new();
    for item in items {
        match item {
            syn::Item::Mod(module) => {
                if let Some((_, inner)) = &module.content {
                    check_cfg_split_items(inner, file, ledgers)?;
                }
            }
            syn::Item::Struct(_) | syn::Item::Union(_) | syn::Item::Enum(_) => {
                let kind = match item {
                    syn::Item::Struct(_) => "struct",
                    syn::Item::Union(_) => "union",
                    _ => "enum",
                };
                if let Some(ident) = item_ident(item) {
                    groups
                        .entry((kind.to_owned(), ident.to_string()))
                        .or_default()
                        .push(item);
                }
            }
            _ => {}
        }
    }
    for ((_kind, name), arms) in groups {
        if arms.len() > 1 {
            check_cfg_split_item_group(file, &name, &arms, ledgers)?;
        }
    }
    Ok(())
}

/// CS-1: every cfg(not(X))/cfg(X) struct/union/enum pair must be
/// token-identical except visibility, and every widened field must have
/// exactly one VIS row (peer finding 06:27Z).
fn check_cfg_split_arm_identity(workspace: &Path, ledgers: &[VisRow]) -> Result<(), String> {
    for crate_dir in VIS_CRATES {
        let src = workspace.join("crates").join(crate_dir).join("src");
        let mut files = Vec::new();
        walk_files(&src, &mut files)?;
        for path in files {
            if path.extension().is_none_or(|extension| extension != "rs") || is_test_path(&path) {
                continue;
            }
            let relative = path
                .strip_prefix(workspace)
                .map_err(|_| format!("{} is outside the workspace", path.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            let source = read(&path)?;
            let parsed =
                syn::parse_file(&source).map_err(|error| format!("{relative}: {error}"))?;
            check_cfg_split_items(&parsed.items, &relative, ledgers)?;
        }
    }
    Ok(())
}

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

fn declaration_ident(text: &str) -> Option<String> {
    let name: String = text
        .trim_start()
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
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
    // `const` is a qualifier only when it introduces a function; otherwise it
    // is the item itself (`const NONE: Self`).
    if let Some(after_const) = rest.strip_prefix("const ") {
        let tail = after_const.trim_start();
        if !tail.starts_with("fn ")
            && !tail.starts_with("unsafe ")
            && !tail.starts_with("async ")
            && !tail.starts_with("extern ")
        {
            return declaration_ident(tail);
        }
    }
    // Skip item qualifiers in any order: `const`, `async`, `unsafe`, `default`,
    // `extern`.
    loop {
        let mut advanced = false;
        for qualifier in ["const ", "async ", "unsafe ", "default ", "extern "] {
            if let Some(tail) = rest.strip_prefix(qualifier) {
                rest = tail.trim_start();
                advanced = true;
            }
        }
        if !advanced {
            break;
        }
    }
    // An `extern "C"` ABI string sits between the qualifier and the item
    // keyword: `unsafe extern "C" fn f(...)`.
    if rest.starts_with('"') {
        if let Some(close) = rest[1..].find('"') {
            rest = rest[close + 2..].trim_start();
        }
    }
    for keyword in [
        "fn ", "struct ", "enum ", "union ", "trait ", "type ", "static ", "mod ",
    ] {
        if let Some(tail) = rest.strip_prefix(keyword) {
            if let Some(name) = declaration_ident(tail) {
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
    let mut sites = visibility_make_sites(workspace)?;
    sites.extend(cfg_split_widening_sites(workspace)?);
    sites.sort();
    sites.dedup();
    for (file, item) in sites {
        if !listed.contains(&(file.clone(), item.clone())) {
            return Err(format!(
                "{file}: `{item}` widens test-support visibility but has no TEST-SUPPORT-VIS row"
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
    let mut ledgers = Vec::new();
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
        ledgers.push(rows);
    }
    // The field arm needs the complete widened-field set per struct across
    // every ledger: construction/destructuring is only a hit when EVERY field
    // of the struct is widened. The map is keyed by (crate, defining file,
    // struct), never by parent-path spelling; mixed spellings are an error.
    let widened = widened_field_sets(&ledgers)?;
    let mut all_rows = Vec::new();
    for rows in ledgers {
        for row in &rows {
            check_row(workspace, row, &fixtures, &authority, &widened)?;
        }
        all_rows.extend(rows);
    }
    check_cfg_split_arm_identity(workspace, &all_rows)?;
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
        // Field rows no longer use the item-path rule (peer ruling 04:2xZ);
        // the field arm decides them from `.field` reads and construction.
        assert!(
            names_in_fixtures("error::InFlightRootPin::registry", "field", &fixtures).is_none()
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
    fn v1_field_read_mutation_red_then_restored_green() {
        let declared = vec![BTreeSet::from(["storage".to_owned()])];
        let widened = BTreeSet::from(["storage".to_owned()]);
        let parent = ["heap_types", "LispString"];
        let read = scopes(&[
            "use neovm_value::heap_types::LispString;\nfn f(s: &LispString) { let _ = s.storage; }",
        ]);
        let error = field_hit(&read, &parent, "storage", &declared, &widened)
            .expect("a planted field read must be refused");
        eprintln!("MUTATION RED: {error}");
        let restored =
            scopes(&["use neovm_value::heap_types::LispString;\nfn f(s: &LispString) {}"]);
        assert!(field_hit(&restored, &parent, "storage", &declared, &widened).is_none());
        eprintln!("RESTORED GREEN: no field read");
        // A read through a different type does not name this struct's field.
        let other =
            scopes(&["struct Other { storage: usize }\nfn f(s: &Other) { let _ = s.storage; }"]);
        assert!(field_hit(&other, &parent, "storage", &declared, &widened).is_none());
    }

    #[test]
    fn v1_field_read_on_an_unresolved_base_fails_closed() {
        let declared = vec![BTreeSet::from(["storage".to_owned()])];
        let widened = BTreeSet::from(["storage".to_owned()]);
        let unresolved = scopes(&["fn f() { let x = make(); let _ = x.storage; }"]);
        assert!(
            field_hit(
                &unresolved,
                &["heap_types", "LispString"],
                "storage",
                &declared,
                &widened
            )
            .is_some()
        );
    }

    #[test]
    fn v1_field_construction_requires_every_field() {
        let parent = ["error", "InFlightRootPin"];
        let declared = vec![BTreeSet::from(["registry".to_owned(), "slot".to_owned()])];
        let every = BTreeSet::from(["registry".to_owned(), "slot".to_owned()]);
        let partial = BTreeSet::from(["registry".to_owned()]);
        let literal = scopes(&["use neovm_value::error::InFlightRootPin;\n\
             fn f() { let _ = InFlightRootPin { registry: todo!(), slot: 0 }; }"]);
        assert!(field_hit(&literal, &parent, "registry", &declared, &every).is_some());
        assert!(field_hit(&literal, &parent, "registry", &declared, &partial).is_none());
        let pattern = scopes(&["use neovm_value::error::InFlightRootPin;\n\
             fn f(p: InFlightRootPin) { let InFlightRootPin { registry, slot } = p; }"]);
        assert!(field_hit(&pattern, &parent, "slot", &declared, &every).is_some());
        // Naming the type without constructing or reading it is not a hit.
        let named =
            scopes(&["use neovm_value::error::InFlightRootPin;\nfn f(_: InFlightRootPin) {}"]);
        assert!(field_hit(&named, &parent, "registry", &declared, &every).is_none());
    }

    #[test]
    fn v1_field_golden_needs_both_the_read_and_the_struct() {
        let declared = vec![BTreeSet::from(["size".to_owned()])];
        let widened = BTreeSet::from(["size".to_owned()]);
        let both = vec![fixture_scope(
            "golden.stderr".to_owned(),
            "error: field `size` of `heap_types::LispString` is private\n  | s.size",
            false,
            true,
        )];
        assert!(
            field_hit(
                &both,
                &["heap_types", "LispString"],
                "size",
                &declared,
                &widened
            )
            .is_some()
        );
        let read_only = vec![fixture_scope(
            "golden.stderr".to_owned(),
            "error: cannot read `.size` here",
            false,
            true,
        )];
        assert!(
            field_hit(
                &read_only,
                &["heap_types", "LispString"],
                "size",
                &declared,
                &widened
            )
            .is_none()
        );
    }

    #[test]
    fn v1_declaration_names_cover_consts_and_const_fns() {
        assert_eq!(
            declaration_name("    pub(crate) const NONE: Self = Self {"),
            Some("NONE".to_owned())
        );
        assert_eq!(
            declaration_name("    #[cfg_attr(feature = \"test-support\", visibility::make(pub))]"),
            None
        );
        assert_eq!(
            declaration_name("pub const fn helper() {}"),
            Some("helper".to_owned())
        );
        assert_eq!(
            declaration_name("pub(crate) fn foo() {}"),
            Some("foo".to_owned())
        );
        assert_eq!(
            declaration_name("pub static COUNTER: usize = 0;"),
            Some("COUNTER".to_owned())
        );
        assert_eq!(
            declaration_name("unsafe extern \"C\" fn module_make_global_ref("),
            Some("module_make_global_ref".to_owned())
        );
        assert_eq!(
            declaration_name("extern \"C\" fn helper() {}"),
            Some("helper".to_owned())
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

    fn field_row(crate_dir: &str, item: &str, file: &str) -> VisRow {
        VisRow {
            crate_dir: crate_dir.to_owned(),
            item: item.to_owned(),
            kind: "field".to_owned(),
            file: file.to_owned(),
            tests: Vec::new(),
        }
    }

    #[test]
    fn field_arm_keys_widened_fields_by_crate_file_and_struct() {
        let file = "crates/neovm-value/src/heap_types.rs";
        let ledgers = vec![vec![
            field_row("neovm-value", "heap_types::LispString::size", file),
            field_row("neovm-value", "heap_types::LispString::data", file),
        ]];
        let widened = widened_field_sets(&ledgers).expect("one spelling");
        let key = FieldStructKey {
            crate_dir: "neovm-value".to_owned(),
            file: file.to_owned(),
            struct_name: "LispString".to_owned(),
        };
        assert_eq!(
            widened.get(&key),
            Some(&BTreeSet::from(["size".to_owned(), "data".to_owned()]))
        );
        // A same-named struct in another file is a different identity, so its
        // fields never satisfy rule (b) for this one.
        let other = "crates/neovm-value/src/other.rs";
        let ledgers = vec![vec![
            field_row("neovm-value", "heap_types::LispString::size", file),
            field_row("neovm-value", "other::LispString::size", other),
        ]];
        assert_eq!(
            widened_field_sets(&ledgers).expect("distinct files").len(),
            2
        );
    }

    #[test]
    fn mixed_parent_spellings_for_one_struct_are_an_error() {
        let file = "crates/neovm-value/src/heap_types.rs";
        let ledgers = vec![
            vec![field_row(
                "neovm-value",
                "heap_types::LispString::size",
                file,
            )],
            vec![field_row("neovm-value", "LispString::data", file)],
        ];
        let error = widened_field_sets(&ledgers).expect_err("mixed spellings must fail closed");
        assert!(error.contains("mix parent spellings"), "{error}");
        assert!(
            error.contains("LispString / heap_types::LispString"),
            "{error}"
        );
        eprintln!("MUTATION RED: {error}");
    }

    #[test]
    fn v1_golden_privacy_block_hits_a_planted_private_item() {
        let golden = vec![golden_scope(
            "golden.stderr".to_owned(),
            "error[E0603]: struct `Entry` is private\n  --> fixture.rs:1:1\n   |\n 1 | use crate::parse_loop::Entry;\n",
        )];
        let hit = names_in_fixtures("emacs_core::syntax::parse_loop::Entry", "struct", &golden);
        assert!(hit.is_some(), "{hit:?}");
        eprintln!("MUTATION RED: {hit:?}");
    }

    #[test]
    fn v1_golden_auto_trait_note_is_not_a_hit() {
        let golden = vec![golden_scope(
            "golden.stderr".to_owned(),
            "error[E0277]: `Cell<eval::admission::ThreadingMode>` cannot be shared between threads safely\n   = note: required for `Context` to implement `Sync`\n",
        )];
        assert!(
            names_in_fixtures("emacs_core::admission::ThreadingMode", "enum", &golden).is_none()
        );
        // The real ThreadingMode goldens carry only E0277 obligation notes.
        let workspace = workspace_root();
        for relative in [
            "crates/neovm-core/tests/thread_safety_ui/flow_word/context_send.stderr",
            "crates/neovm-core/tests/p1rv_ui/era_cannot_be_shared.stderr",
        ] {
            let source = read(&workspace.join(relative)).expect("golden");
            let scope = golden_scope(relative.to_owned(), &source);
            let hit = names_in_fixtures("emacs_core::admission::ThreadingMode", "enum", &[scope]);
            assert!(hit.is_none(), "{relative}: {hit:?}");
        }
    }

    #[test]
    fn v1_fixture_source_use_still_hits() {
        let fixture = vec![fixture_scope(
            "fixture.rs".to_owned(),
            "use neovm_eval::emacs_core::admission::ThreadingMode;\nfn f(_: ThreadingMode) {}\n",
            true,
            false,
        )];
        assert!(
            names_in_fixtures("emacs_core::admission::ThreadingMode", "enum", &fixture).is_some()
        );
    }

    #[test]
    fn cfg_split_item_and_use_widening_sites_are_detected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("crates/neovm-value/src");
        std::fs::create_dir_all(&src).expect("src");
        std::fs::write(
            src.join("split.rs"),
            r#"
#[cfg(not(feature = "test-support"))]
pub(crate) enum Mode {
    A,
}
#[cfg(feature = "test-support")]
pub enum Mode {
    A,
}
#[cfg(not(feature = "test-support"))]
pub(crate) use crate::thing::Thing;
#[cfg(feature = "test-support")]
pub use crate::thing::Thing;
#[cfg(not(feature = "test-support"))]
pub struct Always;
#[cfg(feature = "test-support")]
pub struct Always;
"#,
        )
        .expect("split");
        let sites = cfg_split_widening_sites(dir.path()).expect("sites");
        let names: Vec<&str> = sites.iter().map(|(_, name)| name.as_str()).collect();
        assert!(names.contains(&"Mode"), "{sites:?}");
        assert!(names.contains(&"Thing"), "{sites:?}");
        assert!(!names.contains(&"Always"), "both arms public: {sites:?}");
    }

    #[test]
    fn cfg_split_site_without_a_row_turns_coverage_red() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("crates/neovm-value/src");
        std::fs::create_dir_all(&src).expect("src");
        std::fs::write(
            src.join("split.rs"),
            r#"
#[cfg(not(feature = "test-support"))]
pub(crate) enum Mode {
    A,
}
#[cfg(feature = "test-support")]
pub enum Mode {
    A,
}
"#,
        )
        .expect("split");
        let file = "crates/neovm-value/src/split.rs".to_owned();
        let error = check_visibility_coverage(dir.path(), &[]).expect_err("unrowed split");
        assert!(error.contains("Mode"), "{error}");
        eprintln!("MUTATION RED: {error}");
        let row = VisRow {
            crate_dir: "neovm-value".to_owned(),
            item: "Mode".to_owned(),
            kind: "enum".to_owned(),
            file,
            tests: Vec::new(),
        };
        assert!(check_visibility_coverage(dir.path(), &[row]).is_ok());
    }

    fn cs1_workspace(source: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("crates/neovm-value/src");
        std::fs::create_dir_all(&src).expect("src");
        std::fs::write(src.join("split.rs"), source).expect("split");
        dir
    }

    const CS1_BASELINE: &str = r#"
#[cfg(not(feature = "test-support"))]
pub(crate) struct Split {
    pub(crate) visible: u8,
    pub(crate) widened: u8,
}
#[cfg(feature = "test-support")]
pub struct Split {
    pub(crate) visible: u8,
    pub widened: u8,
}
"#;

    #[test]
    fn cfg_split_arm_identity_accepts_identical_arms_and_ledgered_fields() {
        let dir = cs1_workspace(CS1_BASELINE);
        let file = "crates/neovm-value/src/split.rs".to_owned();
        let rows = vec![field_row("neovm-value", "Split::widened", &file)];
        assert!(check_cfg_split_arm_identity(dir.path(), &rows).is_ok());
    }

    #[test]
    fn cfg_split_arm_identity_rejects_a_field_reorder() {
        let mutated = CS1_BASELINE.replace(
            "    pub(crate) visible: u8,\n    pub widened: u8,",
            "    pub widened: u8,\n    pub(crate) visible: u8,",
        );
        assert_ne!(mutated, CS1_BASELINE, "mutation applied");
        let dir = cs1_workspace(&mutated);
        let file = "crates/neovm-value/src/split.rs".to_owned();
        let rows = vec![field_row("neovm-value", "Split::widened", &file)];
        let error = check_cfg_split_arm_identity(dir.path(), &rows).expect_err("reorder");
        assert!(error.contains("not token-identical"), "{error}");
        eprintln!("MUTATION RED: {error}");
    }

    #[test]
    fn cfg_split_arm_identity_rejects_an_added_derive() {
        let mutated = CS1_BASELINE.replace(
            "#[cfg(feature = \"test-support\")]\npub struct Split {",
            "#[cfg(feature = \"test-support\")]\n#[derive(Clone)]\npub struct Split {",
        );
        assert_ne!(mutated, CS1_BASELINE, "mutation applied");
        let dir = cs1_workspace(&mutated);
        let file = "crates/neovm-value/src/split.rs".to_owned();
        let rows = vec![field_row("neovm-value", "Split::widened", &file)];
        let error = check_cfg_split_arm_identity(dir.path(), &rows).expect_err("derive");
        assert!(error.contains("not token-identical"), "{error}");
        eprintln!("MUTATION RED: {error}");
    }

    #[test]
    fn cfg_split_arm_identity_rejects_an_unledgered_widened_field() {
        let dir = cs1_workspace(CS1_BASELINE);
        let error = check_cfg_split_arm_identity(dir.path(), &[]).expect_err("unledgered");
        assert!(error.contains("no TEST-SUPPORT-VIS row"), "{error}");
        eprintln!("MUTATION RED: {error}");
    }
}

/// B3 AUTHORITY-ITEMS guard (peer ruling 07:3xZ): every row has the signed
/// eight-column shape, its defining file exists, and every public item in the
/// value TA2 fence has exactly one row. Future fence items must be rowed
/// before they are added; the defining-file check is the ledger's existence
/// requirement, while the `tests` column intentionally may name a test file
/// that lands with the same batch.
#[test]
fn authority_items_have_rows_and_existing_defining_files() {
    let workspace = workspace_root();
    let path = workspace.join(ARCHITECTURE_DIR).join("AUTHORITY-ITEMS.tsv");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let mut lines = text.lines();
    let header = lines.next().expect("AUTHORITY-ITEMS header");
    assert_eq!(
        header, "crate\titem\tkind\tdefining_file\tfence_path\ttests\treason\tsafety",
        "AUTHORITY-ITEMS.tsv: signed header"
    );
    let mut rows: BTreeSet<(String, String)> = BTreeSet::new();
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(
            fields.len(),
            8,
            "AUTHORITY-ITEMS.tsv line {} has {} columns",
            index + 2,
            fields.len()
        );
        let (crate_name, item, kind, file, fence_path, tests, reason, safety) = (
            fields[0], fields[1], fields[2], fields[3], fields[4], fields[5], fields[6], fields[7],
        );
        assert!(
            !item.is_empty(),
            "AUTHORITY-ITEMS.tsv line {}: empty item",
            index + 2
        );
        assert!(
            !fence_path.is_empty(),
            "AUTHORITY-ITEMS.tsv line {}: empty fence_path",
            index + 2
        );
        assert!(
            !tests.is_empty(),
            "AUTHORITY-ITEMS.tsv line {}: empty tests",
            index + 2
        );
        assert!(
            !reason.is_empty(),
            "AUTHORITY-ITEMS.tsv line {}: empty reason",
            index + 2
        );
        assert!(
            matches!(
                kind,
                "fn" | "unsafe fn"
                    | "method"
                    | "type"
                    | "struct"
                    | "enum"
                    | "const"
                    | "static"
                    | "mod"
            ),
            "AUTHORITY-ITEMS.tsv line {}: kind {kind:?}",
            index + 2
        );
        assert!(
            safety == "safe" || safety.starts_with("unsafe: "),
            "AUTHORITY-ITEMS.tsv line {}: safety {safety:?}",
            index + 2
        );
        let defining = workspace.join(file);
        assert!(
            defining.is_file(),
            "AUTHORITY-ITEMS.tsv line {}: defining file missing: {file}",
            index + 2
        );
        assert!(
            rows.insert((crate_name.to_owned(), item.to_owned())),
            "AUTHORITY-ITEMS.tsv line {}: duplicate row {crate_name}/{item}",
            index + 2
        );
    }
    assert!(!rows.is_empty(), "AUTHORITY-ITEMS.tsv has no rows");
    let fence_path = workspace.join("crates/neovm-value/src/test_support/authority.rs");
    let fence = std::fs::read_to_string(&fence_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", fence_path.display()));
    let parsed = syn::parse_file(&fence).expect("parse value TA2 fence");
    let mut fence_items = BTreeSet::new();
    for item in &parsed.items {
        let (name, visibility) = match item {
            syn::Item::Fn(item) => (item.sig.ident.to_string(), &item.vis),
            syn::Item::Type(item) => (item.ident.to_string(), &item.vis),
            syn::Item::Struct(item) => (item.ident.to_string(), &item.vis),
            syn::Item::Enum(item) => (item.ident.to_string(), &item.vis),
            syn::Item::Const(item) => (item.ident.to_string(), &item.vis),
            syn::Item::Static(item) => (item.ident.to_string(), &item.vis),
            _ => continue,
        };
        if matches!(visibility, syn::Visibility::Public(_)) {
            fence_items.insert(name);
        }
    }
    for item in fence_items {
        assert!(
            rows.contains(&("neovm-value".to_owned(), item.clone())),
            "value TA2 fence item {item} has no AUTHORITY-ITEMS row"
        );
    }
}
