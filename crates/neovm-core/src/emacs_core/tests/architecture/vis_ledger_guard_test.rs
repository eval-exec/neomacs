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
    "macro",
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

/// Evaluate a cfg predicate with the two knobs this guard varies: whether
/// `test` is enabled and whether `feature = "test-support"` is enabled. Only
/// a predicate that is definitively false marks an arm out of scope for the
/// chosen environment.
fn cfg_truth_with(meta: &syn::Meta, test: bool, test_support: bool) -> CfgTruth {
    match meta {
        syn::Meta::Path(path) if path.is_ident("test") => {
            if test {
                CfgTruth::True
            } else {
                CfgTruth::False
            }
        }
        syn::Meta::NameValue(value) if value.path.is_ident("feature") => {
            if let syn::Expr::Lit(expr) = &value.value {
                if let syn::Lit::Str(name) = &expr.lit {
                    if name.value() == "test-support" {
                        return if test_support {
                            CfgTruth::True
                        } else {
                            CfgTruth::False
                        };
                    }
                }
            }
            CfgTruth::Unknown
        }
        syn::Meta::List(list) if list.path.is_ident("not") => {
            match list.parse_args::<syn::Meta>() {
                Ok(inner) => match cfg_truth_with(&inner, test, test_support) {
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
                        match cfg_truth_with(meta, test, test_support) {
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

/// `test = false`, `feature = "test-support" = false`: only a predicate that
/// is definitively false marks a test-only arm.
fn cfg_truth(meta: &syn::Meta) -> CfgTruth {
    cfg_truth_with(meta, false, false)
}

/// The test-support build (`test = false`, `feature = "test-support" = true`),
/// the only build in which a cfg-split `pub mod` arm exists. Used by the
/// mod-row expansion to decide which nested items are actually compiled there.
fn cfg_truth_test_support(meta: &syn::Meta) -> CfgTruth {
    cfg_truth_with(meta, false, true)
}

fn expansion_gated(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("test") {
            return true;
        }
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Meta>()
                .map(|meta| cfg_truth_test_support(&meta) == CfgTruth::False)
                .unwrap_or(false)
    })
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
        } else if segments.len() == 1 && candidate_leaf == item_leaf {
            // A BARE promotion-ledger item name (the ledger carries its
            // module in a separate column) only collides with a bare VIS
            // item. A qualified VIS row names a specific module path, and
            // same-leaf functions in other modules are unrelated items (the
            // FIRST_ID alias lesson): the qualified suffix rules above are
            // the actual overlap test.
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

/// A `macro` row names the CRATE-ROOT export of a `#[macro_export]` macro
/// (lead-integ 01:3xZ condition): `#[macro_export]` exports the macro at the
/// crate root, so a module-prefixed item such as
/// `emacs_core::eval::eval_absent_layer_slots` is not a real path and must be
/// red. The defining file must also actually contain a `macro_rules! <leaf>`
/// item carrying its own `#[macro_export]` attribute. Both facts are read
/// from the parsed item, not from the file text: a mention of `#[macro_export]`
/// or of the macro name in a comment or a string literal does not satisfy the
/// row, and a different exported macro in the same file does not cover a
/// non-exported one.
fn macro_row_violation(row: &VisRow, source: &str) -> Result<(), String> {
    let leaf = row.item.rsplit("::").next().unwrap_or(&row.item);
    if row.item.contains("::") {
        return Err(format!(
            "{} {} macro: a macro row must name the crate-root export (no module prefix; #[macro_export] exports at the root), got `{}`",
            row.crate_dir, row.item, row.item
        ));
    }
    let parsed = syn::parse_file(source)
        .map_err(|error| format!("{} {leaf} macro: {error}", row.crate_dir))?;
    let definition = parsed.items.iter().find_map(|item| match item {
        syn::Item::Macro(item_macro)
            if item_macro.ident.as_ref().is_some_and(|ident| ident == leaf) =>
        {
            Some(item_macro)
        }
        _ => None,
    });
    let Some(definition) = definition else {
        return Err(format!(
            "{} {leaf} macro: the defining file has no `macro_rules! {leaf}` item",
            row.crate_dir
        ));
    };
    if !definition
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("macro_export"))
    {
        return Err(format!(
            "{} {leaf} macro: `macro_rules! {leaf}` has no #[macro_export] of its own; a module-scoped macro is not a crate-root export",
            row.crate_dir
        ));
    }
    Ok(())
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
    // A self-file mod row names the module's own source file (the spelling
    // accepted for the expansion): the file IS the module, so the textual
    // leaf check does not apply.
    let self_file_mod = row.kind == "mod"
        && Path::new(&row.file)
            .file_name()
            .and_then(|file| file.to_str())
            .is_some_and(|file| file == format!("{leaf}.rs"));
    if !source.contains(leaf) && !self_file_mod {
        return Err(format!(
            "{what}: `{leaf}` does not occur in the defining file"
        ));
    }
    if row.kind == "macro" {
        macro_row_violation(row, &source)?;
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

/// FEATURE-ONLY PUB SURFACE (peer 00:26:50Z follow-up; the third widening
/// shape).
///
/// A plain `pub` item whose cfg keeps it out of the production build AND out
/// of the crate's own `cfg(test)` build exists only in the cross-crate
/// `test-support` build, e.g.
/// `#[cfg(feature = "test-support")] pub struct RcCensus`. None of the other
/// arms can see it: there is no `visibility::make(pub)` attribute, there is
/// no production cfg-split twin, and TS-COVERAGE only scans the
/// `test_support` trees. Such an item is cross-crate test-support surface by
/// construction, so the forward check requires it to be named by a row (a
/// TEST-SUPPORT-VIS row at its defining file, or an AUTHORITY fence row)
/// exactly like the other widening sites.
///
/// `cfg(any(test, feature = "test-support"))` items are deliberately NOT
/// this arm: they also exist in the crate's own test build. The wider class
/// of test-gated `pub` items reachable under the feature is reported
/// separately; this arm is the shape that can only ever be cross-crate API.
fn feature_only_pub_sites(workspace: &Path) -> Result<Vec<(String, String)>, String> {
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
            collect_feature_only_pub_sites(&parsed.items, &relative, &mut sites);
        }
    }
    sites.sort();
    sites.dedup();
    Ok(sites)
}

fn collect_feature_only_pub_sites(
    items: &[syn::Item],
    file: &str,
    out: &mut Vec<(String, String)>,
) {
    for item in items {
        let (vis, attrs, name) = match item {
            syn::Item::Fn(item) => (&item.vis, &item.attrs, item.sig.ident.to_string()),
            syn::Item::Struct(item) => (&item.vis, &item.attrs, item.ident.to_string()),
            syn::Item::Enum(item) => (&item.vis, &item.attrs, item.ident.to_string()),
            syn::Item::Union(item) => (&item.vis, &item.attrs, item.ident.to_string()),
            syn::Item::Trait(item) => (&item.vis, &item.attrs, item.ident.to_string()),
            syn::Item::Type(item) => (&item.vis, &item.attrs, item.ident.to_string()),
            syn::Item::Const(item) => (&item.vis, &item.attrs, item.ident.to_string()),
            syn::Item::Static(item) => (&item.vis, &item.attrs, item.ident.to_string()),
            syn::Item::Mod(module) => {
                if let Some((_, inner)) = &module.content {
                    collect_feature_only_pub_sites(inner, file, out);
                }
                continue;
            }
            _ => continue,
        };
        if matches!(vis, syn::Visibility::Public(_)) && feature_only_gated(attrs) {
            out.push((file.to_owned(), name));
        }
    }
}

/// True when a cfg set makes an item present in the cross-crate build
/// (`test = false`, `feature = "test-support"`) and absent in the crate's own
/// test build (`test = true`, the feature off). Unknown predicates never
/// match; the arm stays fail-closed for the shapes it can decide.
fn feature_only_gated(attrs: &[syn::Attribute]) -> bool {
    let cfgs: Vec<syn::Meta> = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cfg"))
        .filter_map(|attr| attr.parse_args::<syn::Meta>().ok())
        .collect();
    if cfgs.is_empty() {
        return false;
    }
    let present_cross = cfgs
        .iter()
        .all(|meta| cfg_truth_with(meta, false, true) == CfgTruth::True);
    let absent_own_test = cfgs
        .iter()
        .any(|meta| cfg_truth_with(meta, true, false) == CfgTruth::False);
    present_cross && absent_own_test
}

/// H1a/T5 guard for the EVAL-UNIT-RUNTIME AbsentLayer refusal blocks
/// (peer 22:54:30Z H1a; peer 00:15:30Z T1-T5).
///
/// The refusal BUILDING BLOCKS (the shared unlinked helper, the marker, and
/// the `eval_absent_layer_slots!` macro) may be shared from eval under
/// test-support because they mint nothing. A canonical-set binary must never
/// contain a unit set, so no source under `neovm-core`, `neovm-system` or the
/// `neomacs` binary crate may reference any of them. In the provider crates
/// the macro may be invoked only inside a crate's own `cfg(test)`
/// `unit_runtime` module (never feature test-support, never any other
/// module); there is at most ONE macro definition, gated for test-support.
/// The macro_rules body itself is exempt from the reference scan (it is the
/// building block's definition, not a use).
fn unit_runtime_absent_layer_violation(workspace: &Path) -> Result<(), String> {
    const CANONICAL_ROOTS: &[&str] = &["neovm-core", "neovm-system", "neomacs"];
    let mut roots: BTreeMap<String, bool> = BTreeMap::new();
    for crate_name in VIS_CRATES {
        roots.insert(format!("crates/{crate_name}/src"), false);
    }
    for crate_name in CANONICAL_ROOTS {
        roots.insert(format!("crates/{crate_name}/src"), true);
    }
    let mut definitions = 0usize;
    let mut violations = Vec::new();
    for (root, canonical) in roots {
        let mut files = Vec::new();
        walk_files(&workspace.join(&root), &mut files)?;
        for path in files {
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let relative = path
                .strip_prefix(workspace)
                .map_err(|_| format!("{} is outside the workspace", path.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            // The architecture guards name the marker and the macro literally
            // to enforce this rule; a guard's own token literals are not a
            // compilation reference to the building block.
            if relative.contains("/tests/architecture/") {
                continue;
            }
            let source = read(&path)?;
            let parsed =
                syn::parse_file(&source).map_err(|error| format!("{relative}: {error}"))?;
            let mut visitor = AbsentLayerVisitor {
                file: &relative,
                canonical,
                unit_runtime_depth: 0,
                definitions: &mut definitions,
                violations: &mut violations,
            };
            visitor.visit_file(&parsed);
        }
    }
    if definitions > 1 {
        return Err(format!(
            "UNIT-RUNTIME: {definitions} {ABSENT_LAYER_MACRO}! definitions; exactly one is allowed (T5 V1)"
        ));
    }
    if let Some(violation) = violations.into_iter().next() {
        return Err(violation);
    }
    Ok(())
}

const ABSENT_LAYER_MARKER: &str = "ABSENT-LAYER";
const ABSENT_LAYER_HELPER: &str = "absent_layer_unlinked";
const ABSENT_LAYER_MACRO: &str = "eval_absent_layer_slots";
const UNIT_RUNTIME_MODULE: &str = "unit_runtime";

struct AbsentLayerVisitor<'a> {
    file: &'a str,
    canonical: bool,
    unit_runtime_depth: usize,
    definitions: &'a mut usize,
    violations: &'a mut Vec<String>,
}

impl AbsentLayerVisitor<'_> {
    fn reference(&mut self, what: &str) {
        if self.canonical {
            self.violations.push(format!(
                "{}: a canonical crate references {what}; an AbsentLayer building block must never reach a canonical-set binary (H1a)",
                self.file
            ));
        }
    }
}

impl<'ast> Visit<'ast> for AbsentLayerVisitor<'_> {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let is_unit_runtime = node.ident == UNIT_RUNTIME_MODULE && cfg_test_only(&node.attrs);
        if is_unit_runtime {
            self.unit_runtime_depth += 1;
        }
        visit::visit_item_mod(self, node);
        if is_unit_runtime {
            self.unit_runtime_depth -= 1;
        }
    }

    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if node.mac.path.is_ident("macro_rules") {
            if node
                .ident
                .as_ref()
                .is_some_and(|ident| ident == ABSENT_LAYER_MACRO)
            {
                *self.definitions += 1;
                if self.canonical {
                    self.violations.push(format!(
                        "{}: the {ABSENT_LAYER_MACRO}! definition must live in a provider crate, never a canonical one (H1a)",
                        self.file
                    ));
                } else if !gated_for_test_support(&node.attrs) {
                    self.violations.push(format!(
                        "{}: the {ABSENT_LAYER_MACRO}! definition must be gated cfg(any(test, feature = \"test-support\")) (T5)",
                        self.file
                    ));
                }
            }
            // The macro_rules body is the building block itself, not a use.
            return;
        }
        visit::visit_item_macro(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let is_slot_macro = node
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == ABSENT_LAYER_MACRO);
        if is_slot_macro {
            if self.canonical {
                self.violations.push(format!(
                    "{}: a canonical crate invokes {ABSENT_LAYER_MACRO}!; a canonical-set binary must never contain a unit set (H1a)",
                    self.file
                ));
            } else if self.unit_runtime_depth == 0 {
                self.violations.push(format!(
                    "{}: {ABSENT_LAYER_MACRO}! is invoked outside a cfg(test) unit_runtime module (T5)",
                    self.file
                ));
            }
        }
        visit::visit_macro(self, node);
    }

    fn visit_path(&mut self, node: &'ast syn::Path) {
        if node
            .segments
            .last()
            .is_some_and(|segment| segment.ident == ABSENT_LAYER_HELPER)
        {
            self.reference("the shared absent_layer_unlinked helper");
        }
        visit::visit_path(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let mut leaves = Vec::new();
        auth_use_leaves(&node.tree, &mut Vec::new(), &mut leaves);
        for (path, alias, glob) in leaves {
            if glob {
                continue;
            }
            let named = |name: &str| {
                alias.as_deref() == Some(name) || path.last().is_some_and(|segment| segment == name)
            };
            if named(ABSENT_LAYER_HELPER) {
                self.reference("the shared absent_layer_unlinked helper");
            }
            if named(ABSENT_LAYER_MACRO) {
                self.reference("the eval_absent_layer_slots! macro");
            }
        }
        visit::visit_item_use(self, node);
    }

    fn visit_lit_str(&mut self, node: &'ast syn::LitStr) {
        if node.value().contains(ABSENT_LAYER_MARKER) {
            self.reference("the ABSENT-LAYER marker");
        }
        visit::visit_lit_str(self, node);
    }
}

/// True for a module that exists in the crate's own `cfg(test)` build but is
/// absent from the cross-crate test-support build: the H1/T5 `cfg(test)`
/// only shape (never `cfg(any(test, feature = "test-support"))`).
fn cfg_test_only(attrs: &[syn::Attribute]) -> bool {
    let cfgs: Vec<syn::Meta> = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cfg"))
        .filter_map(|attr| attr.parse_args::<syn::Meta>().ok())
        .collect();
    if cfgs.is_empty() {
        return false;
    }
    let present_own_test = cfgs
        .iter()
        .all(|meta| cfg_truth_with(meta, true, false) == CfgTruth::True);
    let absent_cross = cfgs
        .iter()
        .any(|meta| cfg_truth_with(meta, false, true) == CfgTruth::False);
    present_own_test && absent_cross
}

/// The T5 gate for the macro definition: absent in production, present in the
/// cross-crate test-support build.
fn gated_for_test_support(attrs: &[syn::Attribute]) -> bool {
    let cfgs: Vec<syn::Meta> = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cfg"))
        .filter_map(|attr| attr.parse_args::<syn::Meta>().ok())
        .collect();
    if cfgs.is_empty() {
        return false;
    }
    let present_cross = cfgs
        .iter()
        .all(|meta| cfg_truth_with(meta, false, true) == CfgTruth::True);
    present_cross && test_gated(attrs)
}

const SLOT_MACRO_NAME: &str = "eval_absent_layer_slots";
const ABSENT_LAYER_HIT_REFUSED: &str = "Refused";
const ABSENT_LAYER_HIT_UNLINKED: &str = "Unlinked";
const ABSENT_LAYER_HIT_TAKER: &str = "take_absent_layer_hits_for_test";

/// H2/T1/T2/T3/T4/H4 guard for the EVAL-UNIT-RUNTIME slot table (peer
/// 22:54:30Z H2/H2a, peer 00:15:30Z T1-T4).
///
/// The signed table lives as one macro_rules body: `eval_absent_layer_slots!`
/// expands a fixed `absent_layer_slots` module of 152 slot functions (135
/// marker PANICs, 12 absent-by-construction UNLINKED slots — nine unit no-ops
/// plus three valued T7 slots: the signal self-pipe and the terminal
/// print-handle return `None`, and the case-table sync returns
/// `CASE_TABLES_SYNC_OK` — 2 structural
/// ASSERT-EMPTY checks, 2 T2 per-thread handle slots, 1 T3 marker slot). This
/// guard parses that body and proves, statically:
/// - every slot body is exactly a marker panic, an exactly-one-call unlinked
///   no-op, a recorded unlinked call followed by the honest absent value (a
///   plain path such as `None` or the const `CASE_TABLES_SYNC_OK`), a single
///   marker-carrying assert-empty check, or the signed
///   handle/marker wrapper shape (a bare empty/default body is red);
/// - the 12 UNLINKED names, markers and honest values, the 2 ASSERT-EMPTY
///   names and markers, and the 2+1 handle/marker slots are exactly the
///   signed ones; every marker is unique and carries the `ABSENT-LAYER `
///   prefix;
/// - the shared helpers have the signed shapes: record_hit writes the
///   env-named append-only log and the TLS hit list with the typed hit class
///   (H4/C2: every log line and TLS entry is (kind, marker)), the panic
///   helper records a Refused hit before it aborts, the unlinked helper
///   records an Unlinked hit and is exactly one recorded
///   no-op, the assert-empty helper panics unless the upper state is empty,
///   the two handles are lazily created per thread through OnceCell
///   get_or_init (T2), the fontset alias is an uninterned ABSENT-LAYER marker
///   symbol and never NIL (T3), and the bootstrap binder binds each upper
///   name to an uninterned ABSENT-LAYER marker (T4);
/// - T1: eval's dependency sections contain no system/display/core edge, so
///   the six absent-by-construction upper states cannot exist in its test
///   binary.
/// The file is skipped until the EVAL-UNIT-RUNTIME spike lands; from then on
/// this is the P2 slot-table gate. Any slot-kind change must come back to the
/// peer (T7), so the counts and the special maps are pinned.
fn unit_runtime_slot_violation(workspace: &Path) -> Result<(), String> {
    let file = workspace.join("crates/neovm-eval/src/emacs_core/runtime/eval/absent_layer.rs");
    if !file.is_file() {
        return Ok(());
    }
    let source = read(&file)?;
    if !source.contains("ABSENT-LAYER upper-variable::") {
        return Err(
            "ABSENT-LAYER T4: the bootstrap marker name template is not the signed one".into(),
        );
    }
    let items = absent_layer_module_items(&source)?;
    let summary = classify_absent_layer_slots(&items)?;
    validate_absent_layer_slot_summary(&summary)?;
    validate_absent_layer_helpers(&items)?;
    let cargo = read(&workspace.join("crates/neovm-eval/Cargo.toml"))?;
    validate_eval_has_no_upper_dependency(&cargo)?;
    Ok(())
}

/// Extract the `absent_layer_slots` module items from the macro_rules body.
fn absent_layer_module_items(source: &str) -> Result<Vec<syn::Item>, String> {
    let parsed = syn::parse_file(source).map_err(|error| format!("absent-layer macro: {error}"))?;
    let mut tokens = None;
    for item in &parsed.items {
        if let syn::Item::Macro(item_macro) = item {
            if item_macro
                .ident
                .as_ref()
                .is_some_and(|ident| ident == SLOT_MACRO_NAME)
            {
                tokens = Some(item_macro.mac.tokens.clone());
                break;
            }
        }
    }
    let tokens =
        tokens.ok_or_else(|| format!("the single {SLOT_MACRO_NAME}! definition was not found"))?;
    let mut iter = tokens.into_iter().peekable();
    let mut body = None;
    while let Some(token) = iter.next() {
        if let proc_macro2::TokenTree::Punct(punct) = &token {
            if punct.as_char() == '=' {
                if let Some(proc_macro2::TokenTree::Punct(next)) = iter.peek() {
                    if next.as_char() == '>' {
                        iter.next();
                        while let Some(next) = iter.next() {
                            if let proc_macro2::TokenTree::Group(group) = next {
                                body = Some(rewrite_dollar_crate(group.stream()));
                                break;
                            }
                        }
                        break;
                    }
                }
            }
        }
    }
    let body =
        body.ok_or_else(|| format!("{SLOT_MACRO_NAME}! has no macro_rules arm body to inspect"))?;
    let expansion: syn::File =
        syn::parse2(body).map_err(|error| format!("{SLOT_MACRO_NAME}! expansion body: {error}"))?;
    for item in expansion.items {
        if let syn::Item::Mod(module) = item {
            if module.ident == "absent_layer_slots" {
                return module.content.map(|(_, items)| items).ok_or_else(|| {
                    "the absent_layer_slots module is declared without a body".into()
                });
            }
        }
    }
    Err("the macro expansion has no absent_layer_slots module".into())
}

/// `$crate` is macro metasyntax; rewrite it to a plain `crate` path so the
/// fixed expansion body parses as ordinary Rust.
fn rewrite_dollar_crate(tokens: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let mut out = proc_macro2::TokenStream::new();
    let mut iter = tokens.into_iter().peekable();
    while let Some(token) = iter.next() {
        match token {
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '$' => {
                if let Some(proc_macro2::TokenTree::Ident(ident)) = iter.peek() {
                    if ident == "crate" {
                        let ident = ident.clone();
                        iter.next();
                        out.extend([proc_macro2::TokenTree::Ident(ident)]);
                        continue;
                    }
                }
                out.extend([proc_macro2::TokenTree::Punct(punct)]);
            }
            proc_macro2::TokenTree::Group(group) => {
                let inner = rewrite_dollar_crate(group.stream());
                out.extend([proc_macro2::TokenTree::Group(proc_macro2::Group::new(
                    group.delimiter(),
                    inner,
                ))]);
            }
            other => out.extend([other]),
        }
    }
    out
}

#[derive(Debug, Default)]
struct AbsentLayerSlotSummary {
    panic: Vec<(String, String)>,
    /// (slot, marker, honest absent value). `None` is the unit no-op shape
    /// (the recorded call is the whole body); `Some(name)` is a
    /// construction-reached slot that returns its honest absent value, e.g.
    /// the signal self-pipe's `None` (T7).
    unlinked: Vec<(String, String, Option<String>)>,
    assert_empty: Vec<(String, String)>,
    handle: Vec<(String, String, String)>,
    marker: Vec<(String, String)>,
}

const ABSENT_LAYER_HELPERS: &[&str] = &[
    "record_hit",
    "take_absent_layer_hits_for_test",
    "absent_layer_unlinked",
    "absent_layer_panic",
    "absent_layer_assert_empty",
    "absent_face_registry_handle",
    "absent_terminal_registry_handle",
    "absent_fontset_alias_marker",
    "bind_absent_bootstrap_vars",
];

fn absent_layer_call(stmt: &syn::Stmt) -> Option<(String, Option<String>)> {
    let syn::Stmt::Expr(syn::Expr::Call(call), _) = stmt else {
        return None;
    };
    let syn::Expr::Path(path) = &*call.func else {
        return None;
    };
    let name = path.path.segments.last()?.ident.to_string();
    let literal = |argument: Option<&syn::Expr>| -> Option<String> {
        let syn::Expr::Lit(literal) = argument? else {
            return None;
        };
        let syn::Lit::Str(value) = &literal.lit else {
            return None;
        };
        Some(value.value())
    };
    if name == "record_hit" {
        // C2 typed hit record: `record_hit(AbsentLayerHit::<Kind>, marker)`.
        // The class is part of the call shape, so the pre-C2 untyped spelling
        // `record_hit(marker)` is not a recognized hit record.
        let mut arguments = call.args.iter();
        let kind = match arguments.next()? {
            syn::Expr::Path(path) => path.path.segments.last()?.ident.to_string(),
            _ => return None,
        };
        return Some((format!("record_hit::{kind}"), literal(arguments.next())));
    }
    Some((name, literal(call.args.first())))
}

/// The honest absent value a construction-reached UNLINKED slot returns
/// after its recorded call: a plain path expression such as `None` (T7).
/// Anything else (a call, a literal, a block) is not the signed shape.
fn unlinked_honest_value(stmt: &syn::Stmt) -> Option<String> {
    let syn::Stmt::Expr(syn::Expr::Path(path), _) = stmt else {
        return None;
    };
    Some(path.path.segments.last()?.ident.to_string())
}

fn classify_absent_layer_slots(items: &[syn::Item]) -> Result<AbsentLayerSlotSummary, String> {
    let mut summary = AbsentLayerSlotSummary::default();
    for item in items {
        let syn::Item::Fn(function) = item else {
            continue;
        };
        let name = function.sig.ident.to_string();
        if ABSENT_LAYER_HELPERS.contains(&name.as_str()) {
            continue;
        }
        let stmts = &function.block.stmts;
        let calls: Vec<(String, Option<String>)> =
            stmts.iter().filter_map(absent_layer_call).collect();
        let panics: Vec<&(String, Option<String>)> = calls
            .iter()
            .filter(|(call, _)| call == "absent_layer_panic")
            .collect();
        let unlinked: Vec<&(String, Option<String>)> = calls
            .iter()
            .filter(|(call, _)| call == "absent_layer_unlinked")
            .collect();
        let asserts: Vec<&(String, Option<String>)> = calls
            .iter()
            .filter(|(call, _)| call == "absent_layer_assert_empty")
            .collect();
        if panics.len() == 1 && stmts.len() == 1 {
            let marker = panics[0]
                .1
                .clone()
                .ok_or_else(|| format!("{name}: the panic marker is not a string literal"))?;
            summary.panic.push((name, marker));
            continue;
        }
        if unlinked.len() == 1 && stmts.len() == 1 {
            let marker = unlinked[0]
                .1
                .clone()
                .ok_or_else(|| format!("{name}: the unlinked marker is not a string literal"))?;
            summary.unlinked.push((name, marker, None));
            continue;
        }
        // T7: a construction-reached slot records the Unlinked hit and then
        // returns its honest absent value (the signal self-pipe's `None`).
        if unlinked.len() == 1
            && stmts.len() == 2
            && absent_layer_call(&stmts[0]).is_some_and(|(call, _)| call == "absent_layer_unlinked")
        {
            if let Some(value) = unlinked_honest_value(&stmts[1]) {
                let marker = unlinked[0].1.clone().ok_or_else(|| {
                    format!("{name}: the unlinked marker is not a string literal")
                })?;
                summary.unlinked.push((name, marker, Some(value)));
                continue;
            }
        }
        if asserts.len() == 1 && panics.is_empty() && unlinked.is_empty() {
            let marker = asserts[0].1.clone().ok_or_else(|| {
                format!("{name}: the assert-empty marker is not a string literal")
            })?;
            summary.assert_empty.push((name, marker));
            continue;
        }
        if stmts.len() == 2 {
            let first = absent_layer_call(&stmts[0]);
            let second = absent_layer_call(&stmts[1]);
            if let (Some((first_name, Some(marker))), Some((second_name, None))) = (first, second) {
                if first_name == format!("record_hit::{ABSENT_LAYER_HIT_UNLINKED}") {
                    if second_name == "absent_face_registry_handle"
                        || second_name == "absent_terminal_registry_handle"
                    {
                        summary.handle.push((name, marker, second_name));
                        continue;
                    }
                    if second_name == "absent_fontset_alias_marker" {
                        summary.marker.push((name, marker));
                        continue;
                    }
                }
            }
        }
        return Err(format!(
            "ABSENT-LAYER slot `{name}` has no signed body shape (a bare empty/default body is a silent default)"
        ));
    }
    Ok(summary)
}

fn validate_absent_layer_slot_summary(summary: &AbsentLayerSlotSummary) -> Result<(), String> {
    if summary.panic.len() != 135
        || summary.unlinked.len() != 12
        || summary.assert_empty.len() != 2
        || summary.handle.len() != 2
        || summary.marker.len() != 1
    {
        return Err(format!(
            "ABSENT-LAYER slot table drifted: {} PANIC, {} UNLINKED, {} ASSERT-EMPTY, {} HANDLE, {} MARKER (pinned 135/12/2/2/1)",
            summary.panic.len(),
            summary.unlinked.len(),
            summary.assert_empty.len(),
            summary.handle.len(),
            summary.marker.len()
        ));
    }
    let unlinked: BTreeMap<&str, (&str, Option<&str>)> = summary
        .unlinked
        .iter()
        .map(|(name, marker, value)| (name.as_str(), (marker.as_str(), value.as_deref())))
        .collect();
    // The signed T1 unit no-ops plus the T7 slots reached by construction and
    // by runtime paths: the eval construction census (terminal/fontset/xfaces
    // registry hooks) are no-ops, the signal self-pipe and the terminal
    // print-handle return their honest absent value None, and the case-table
    // sync returns its honest absent value CASE_TABLES_SYNC_OK.
    let expected_unlinked = [
        (
            "dbusbind_reset_thread_locals",
            "ABSENT-LAYER system::dbusbind.reset_thread_locals",
            None,
        ),
        (
            "boot_clear_font_cache_state",
            "ABSENT-LAYER display::boot.clear_font_cache_state",
            None,
        ),
        (
            "terminal_reset_thread_locals",
            "ABSENT-LAYER display::terminal.reset_thread_locals",
            None,
        ),
        (
            "terminal_reset_handle",
            "ABSENT-LAYER display::terminal.reset_handle",
            None,
        ),
        ("timefns_reset", "ABSENT-LAYER system::timefns.reset", None),
        ("sqlite_reset", "ABSENT-LAYER system::sqlite.reset", None),
        (
            "terminal_install_registry_handle",
            "ABSENT-LAYER display::terminal.install_registry_handle",
            None,
        ),
        ("fontset_reset", "ABSENT-LAYER display::fontset.reset", None),
        (
            "xfaces_install_registry_handle",
            "ABSENT-LAYER display::xfaces.install_registry_handle",
            None,
        ),
        (
            "input_signal_wake_pipe_read_fd",
            "ABSENT-LAYER system::input.signal_wake_pipe_read_fd",
            Some("None"),
        ),
        (
            "terminal_print_handle",
            "ABSENT-LAYER display::terminal.print_handle",
            Some("None"),
        ),
        (
            "case_tables_sync",
            "ABSENT-LAYER core::case_tables.sync",
            Some("CASE_TABLES_SYNC_OK"),
        ),
    ];
    for (name, marker, value) in expected_unlinked {
        if unlinked.get(name) != Some(&(marker, value)) {
            return Err(format!(
                "ABSENT-LAYER: UNLINKED slot `{name}` must call absent_layer_unlinked(\"{marker}\") and be the signed shape (unit call-only or honest value {value:?}) (T1/T7)"
            ));
        }
    }
    let assert_empty: BTreeMap<&str, &str> = summary
        .assert_empty
        .iter()
        .map(|(name, marker)| (name.as_str(), marker.as_str()))
        .collect();
    let expected_assert_empty = [
        (
            "dynamic_module_collect_gc_roots",
            "ABSENT-LAYER system::dynamic_module.collect_gc_roots",
        ),
        (
            "absent_sqlite_installer",
            "ABSENT-LAYER core::sqlite.install_finalizer_table",
        ),
    ];
    for (name, marker) in expected_assert_empty {
        if assert_empty.get(name) != Some(&marker) {
            return Err(format!(
                "ABSENT-LAYER: ASSERT-EMPTY slot `{name}` must assert with marker \"{marker}\" (H2a)"
            ));
        }
    }
    let handles: BTreeMap<&str, (&str, &str)> = summary
        .handle
        .iter()
        .map(|(name, marker, helper)| (name.as_str(), (marker.as_str(), helper.as_str())))
        .collect();
    let expected_handles = [
        (
            "terminal_current_registry_handle",
            "ABSENT-LAYER display::terminal.current_registry_handle",
            "absent_terminal_registry_handle",
        ),
        (
            "xfaces_current_registry_handle",
            "ABSENT-LAYER display::xfaces.current_registry_handle",
            "absent_face_registry_handle",
        ),
    ];
    for (name, marker, helper) in expected_handles {
        if handles.get(name) != Some(&(marker, helper)) {
            return Err(format!(
                "ABSENT-LAYER: T2 handle slot `{name}` must record \"{marker}\" and return `{helper}()`"
            ));
        }
    }
    if summary.marker.len() != 1 {
        return Err("ABSENT-LAYER: exactly one T3 marker slot is signed".into());
    }
    let (marker_name, marker_text) = &summary.marker[0];
    if marker_name != "fontset_alias_alist_startup_value"
        || marker_text != "ABSENT-LAYER display::fontset.alias_alist_startup_value"
    {
        return Err(format!(
            "ABSENT-LAYER: the T3 slot must be fontset_alias_alist_startup_value with the signed marker, got `{marker_name}` \"{marker_text}\""
        ));
    }
    let mut seen = BTreeSet::new();
    let mut markers: Vec<&str> = Vec::new();
    markers.extend(summary.panic.iter().map(|(_, marker)| marker.as_str()));
    markers.extend(
        summary
            .unlinked
            .iter()
            .map(|(_, marker, _)| marker.as_str()),
    );
    markers.extend(
        summary
            .assert_empty
            .iter()
            .map(|(_, marker)| marker.as_str()),
    );
    markers.extend(summary.handle.iter().map(|(_, marker, _)| marker.as_str()));
    markers.extend(summary.marker.iter().map(|(_, marker)| marker.as_str()));
    for marker in markers {
        if !marker.starts_with("ABSENT-LAYER ") || !marker.contains("::") {
            return Err(format!(
                "ABSENT-LAYER: marker \"{marker}\" must be \"ABSENT-LAYER <layer>::<slot>\""
            ));
        }
        if !seen.insert(marker.clone()) {
            return Err(format!("ABSENT-LAYER: duplicate slot marker \"{marker}\""));
        }
    }
    Ok(())
}

#[derive(Default)]
struct AbsentLayerHelperFacts {
    calls: BTreeSet<String>,
    methods: BTreeSet<String>,
    macros: BTreeSet<String>,
    strings: Vec<String>,
    paths: BTreeSet<String>,
    has_thread_local: bool,
    top_level_calls: Vec<String>,
    top_level_macros: Vec<String>,
    /// Token text of every expression macro. `writeln!`/`format!` arguments
    /// are not Rust AST nodes, so a method called only inside one (the C2
    /// `kind.label()` in the log line) is visible only here.
    macro_texts: Vec<String>,
}

impl<'ast> Visit<'ast> for AbsentLayerHelperFacts {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = &*node.func {
            if let Some(segment) = path.path.segments.last() {
                self.calls.insert(segment.ident.to_string());
            }
        }
        visit::visit_expr_call(self, node);
    }
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.methods.insert(node.method.to_string());
        visit::visit_expr_method_call(self, node);
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if let Some(segment) = node.path.segments.last() {
            self.macros.insert(segment.ident.to_string());
        }
        visit::visit_macro(self, node);
    }
    fn visit_expr_macro(&mut self, node: &'ast syn::ExprMacro) {
        self.macro_texts.push(node.mac.tokens.to_string());
        visit::visit_expr_macro(self, node);
    }
    fn visit_lit_str(&mut self, node: &'ast syn::LitStr) {
        self.strings.push(node.value());
        visit::visit_lit_str(self, node);
    }
    fn visit_path(&mut self, node: &'ast syn::Path) {
        if let Some(segment) = node.segments.last() {
            self.paths.insert(segment.ident.to_string());
        }
        visit::visit_path(self, node);
    }
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if node.mac.path.is_ident("thread_local") {
            self.has_thread_local = true;
        }
        visit::visit_item_macro(self, node);
    }
}

fn absent_layer_helper_facts(function: &syn::ItemFn) -> AbsentLayerHelperFacts {
    let mut facts = AbsentLayerHelperFacts {
        top_level_calls: function
            .block
            .stmts
            .iter()
            .filter_map(|stmt| absent_layer_call(stmt).map(|(name, _)| name))
            .collect(),
        ..AbsentLayerHelperFacts::default()
    };
    for stmt in &function.block.stmts {
        if let syn::Stmt::Expr(syn::Expr::Macro(expr_macro), _) = stmt {
            if let Some(segment) = expr_macro.mac.path.segments.last() {
                facts.top_level_macros.push(segment.ident.to_string());
            }
        }
    }
    facts.visit_item_fn(function);
    facts
}

fn absent_layer_helper<'a>(items: &'a [syn::Item], name: &str) -> Result<&'a syn::ItemFn, String> {
    for item in items {
        if let syn::Item::Fn(function) = item {
            if function.sig.ident == name {
                return Ok(function);
            }
        }
    }
    Err(format!(
        "ABSENT-LAYER: the shared helper `{name}` is missing"
    ))
}

fn validate_record_hit(function: &syn::ItemFn) -> Result<(), String> {
    // C2: the hit record is typed: exactly `(kind: AbsentLayerHit, marker)`.
    let mut inputs = function.sig.inputs.iter();
    let typed_kind = matches!(
        inputs.next(),
        Some(syn::FnArg::Typed(argument))
            if matches!(
                &*argument.ty,
                syn::Type::Path(path)
                    if path
                        .path
                        .segments
                        .last()
                        .is_some_and(|segment| segment.ident == "AbsentLayerHit")
            )
    );
    if !typed_kind {
        return Err(
            "ABSENT-LAYER H4/C2: record_hit must take the typed hit class first (kind: AbsentLayerHit)"
                .into(),
        );
    }
    if !matches!(inputs.next(), Some(syn::FnArg::Typed(_))) || inputs.next().is_some() {
        return Err(
            "ABSENT-LAYER H4/C2: record_hit must take exactly the hit class and the marker".into(),
        );
    }
    let facts = absent_layer_helper_facts(function);
    if !facts.calls.contains("var_os") {
        return Err(
            "ABSENT-LAYER H4: record_hit must read the NEOVM_ABSENT_LAYER_LOG env var".into(),
        );
    }
    for name in ["get_or_init", "create", "append", "open", "lock", "push"] {
        if !facts.methods.contains(name) {
            return Err(format!(
                "ABSENT-LAYER H4: record_hit must use the `{name}` step of the out-of-band record"
            ));
        }
    }
    if !facts.macros.contains("writeln") {
        return Err(
            "ABSENT-LAYER H4: record_hit must append the marker to the env-named log".into(),
        );
    }
    if !facts
        .strings
        .iter()
        .any(|value| value == "NEOVM_ABSENT_LAYER_LOG")
    {
        return Err("ABSENT-LAYER H4: the hit log env var must be NEOVM_ABSENT_LAYER_LOG".into());
    }
    if !facts.paths.contains("HITS") {
        return Err("ABSENT-LAYER H4: record_hit must also push to the process hit list".into());
    }
    // C2: the logged line carries the class label and the TLS entry is the
    // (kind, marker) pair, not a bare marker.
    if !facts.macro_texts.iter().any(|text| text.contains("label")) {
        return Err(
            "ABSENT-LAYER H4/C2: the logged hit line must carry the hit-class label".into(),
        );
    }
    if !facts
        .macro_texts
        .iter()
        .any(|text| text.contains("{}: {marker}"))
    {
        return Err(
            "ABSENT-LAYER H4/C2: the log line must be written as `{label}: {marker}`".into(),
        );
    }
    struct HitPairFacts {
        pair: bool,
    }
    impl<'ast> Visit<'ast> for HitPairFacts {
        fn visit_expr_tuple(&mut self, node: &'ast syn::ExprTuple) {
            if node.elems.len() == 2
                && matches!(&node.elems[0], syn::Expr::Path(path) if path.path.is_ident("kind"))
                && matches!(&node.elems[1], syn::Expr::Path(path) if path.path.is_ident("marker"))
            {
                self.pair = true;
            }
            visit::visit_expr_tuple(self, node);
        }
    }
    let mut pairs = HitPairFacts { pair: false };
    pairs.visit_item_fn(function);
    if !pairs.pair {
        return Err(
            "ABSENT-LAYER H4/C2: the process hit list must record the (kind, marker) pair".into(),
        );
    }
    Ok(())
}

fn validate_unlinked_helper(function: &syn::ItemFn) -> Result<(), String> {
    let expected = format!("record_hit::{ABSENT_LAYER_HIT_UNLINKED}");
    if function.block.stmts.len() != 1 {
        return Err(
            "ABSENT-LAYER T1/C2: absent_layer_unlinked must be exactly one Unlinked hit record"
                .into(),
        );
    }
    let call = absent_layer_call(&function.block.stmts[0]);
    if !matches!(call, Some((ref name, _)) if *name == expected) {
        return Err(
            "ABSENT-LAYER T1/C2: absent_layer_unlinked must record an Unlinked hit (record_hit(AbsentLayerHit::Unlinked, marker))"
                .into(),
        );
    }
    Ok(())
}

fn validate_panic_helper(function: &syn::ItemFn) -> Result<(), String> {
    let stmts = &function.block.stmts;
    if stmts.len() != 3 {
        return Err("ABSENT-LAYER H4: absent_layer_panic must record, print and panic".into());
    }
    let expected = format!("record_hit::{ABSENT_LAYER_HIT_REFUSED}");
    let first = absent_layer_call(&stmts[0]);
    if !matches!(first, Some((ref name, _)) if *name == expected) {
        return Err(
            "ABSENT-LAYER H4/C2: the panic path must record a Refused hit BEFORE it aborts".into(),
        );
    }
    let facts = absent_layer_helper_facts(function);
    if !facts.macros.contains("eprintln") || !facts.macros.contains("panic") {
        return Err("ABSENT-LAYER H2a: the panic path must print the marker and panic".into());
    }
    Ok(())
}

fn validate_assert_empty_helper(function: &syn::ItemFn) -> Result<(), String> {
    let stmts = &function.block.stmts;
    let Some(syn::Stmt::Expr(syn::Expr::If(if_expr), _)) = stmts.first() else {
        return Err(
            "ABSENT-LAYER H2a: absent_layer_assert_empty must assert the upper state is empty before returning"
                .into(),
        );
    };
    let syn::Expr::Unary(unary) = &*if_expr.cond else {
        return Err("ABSENT-LAYER H2a: the assert-empty condition must be `!empty`".into());
    };
    if !matches!(&*unary.expr, syn::Expr::Path(path) if path.path.is_ident("empty")) {
        return Err("ABSENT-LAYER H2a: the assert-empty condition must be `!empty`".into());
    }
    let mut facts = AbsentLayerHelperFacts::default();
    facts.visit_block(&if_expr.then_branch);
    if !facts.calls.contains("absent_layer_panic") {
        return Err("ABSENT-LAYER H2a: a non-empty upper state must panic with the marker".into());
    }
    Ok(())
}

fn validate_handle_helper(function: &syn::ItemFn) -> Result<(), String> {
    let facts = absent_layer_helper_facts(function);
    if !(facts.has_thread_local || facts.macros.contains("thread_local"))
        || !facts.methods.contains("get_or_init")
    {
        return Err(
            "ABSENT-LAYER T2: a per-thread handle must be lazily created through a thread-local OnceCell get_or_init"
                .into(),
        );
    }
    if !facts.methods.contains("with") || !facts.methods.contains("clone") {
        return Err(
            "ABSENT-LAYER T2: the handle helper must return the SAME cached per-thread handle (with + clone)"
                .into(),
        );
    }
    Ok(())
}

fn validate_alias_marker_helper(function: &syn::ItemFn) -> Result<(), String> {
    let facts = absent_layer_helper_facts(function);
    if !facts
        .calls
        .contains("make_uninterned_symbol_with_name_value")
    {
        return Err(
            "ABSENT-LAYER T3: the fontset alias must be an uninterned marker symbol".into(),
        );
    }
    if !facts
        .strings
        .iter()
        .any(|value| value == "ABSENT-LAYER display::fontset.alias_alist_startup_value")
    {
        return Err("ABSENT-LAYER T3: the fontset alias marker text is not the signed one".into());
    }
    if facts.paths.contains("NIL") || facts.strings.iter().any(|value| value == "NIL") {
        return Err("ABSENT-LAYER T3: the fontset alias slot must never return nil".into());
    }
    Ok(())
}

fn validate_bootstrap_binder(function: &syn::ItemFn) -> Result<(), String> {
    let facts = absent_layer_helper_facts(function);
    if !facts
        .calls
        .contains("make_uninterned_symbol_with_name_value")
    {
        return Err(
            "ABSENT-LAYER T4: each upper bootstrap variable must bind to an uninterned marker"
                .into(),
        );
    }
    if !facts.macros.contains("format") {
        return Err(
            "ABSENT-LAYER T4: bootstrap markers must be named ABSENT-LAYER upper-variable::<name>"
                .into(),
        );
    }
    for method in ["make_special", "set_symbol_value"] {
        if !facts.methods.contains(method) {
            return Err(format!(
                "ABSENT-LAYER T4: the binder must call `{method}` so boundp/special-variable shapes match"
            ));
        }
    }
    Ok(())
}

fn validate_absent_layer_hit_enum(items: &[syn::Item]) -> Result<(), String> {
    let hit_enum = items
        .iter()
        .find_map(|item| match item {
            syn::Item::Enum(value) if value.ident == "AbsentLayerHit" => Some(value),
            _ => None,
        })
        .ok_or_else(|| {
            "ABSENT-LAYER H4/C2: the slot module must define AbsentLayerHit".to_string()
        })?;
    let variants: Vec<String> = hit_enum
        .variants
        .iter()
        .map(|variant| variant.ident.to_string())
        .collect();
    if variants != [ABSENT_LAYER_HIT_REFUSED, ABSENT_LAYER_HIT_UNLINKED] {
        return Err(format!(
            "ABSENT-LAYER H4/C2: AbsentLayerHit variants must be exactly Refused and Unlinked (got {variants:?})"
        ));
    }
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    for item in items {
        let syn::Item::Impl(block) = item else {
            continue;
        };
        let owner = match &*block.self_ty {
            syn::Type::Path(path) => path
                .path
                .segments
                .last()
                .map(|segment| segment.ident.to_string()),
            _ => None,
        };
        if owner.as_deref() != Some("AbsentLayerHit") {
            continue;
        }
        for member in &block.items {
            let syn::ImplItem::Fn(function) = member else {
                continue;
            };
            if function.sig.ident != "label" {
                continue;
            }
            for stmt in &function.block.stmts {
                let syn::Stmt::Expr(syn::Expr::Match(match_expr), _) = stmt else {
                    continue;
                };
                for arm in &match_expr.arms {
                    let syn::Pat::Path(pattern) = &arm.pat else {
                        continue;
                    };
                    let Some(variant) = pattern
                        .path
                        .segments
                        .last()
                        .map(|segment| segment.ident.to_string())
                    else {
                        continue;
                    };
                    let syn::Expr::Lit(literal) = &*arm.body else {
                        continue;
                    };
                    let syn::Lit::Str(label) = &literal.lit else {
                        continue;
                    };
                    labels.insert(variant, label.value());
                }
            }
        }
    }
    if labels.get(ABSENT_LAYER_HIT_REFUSED).map(String::as_str) != Some("refused")
        || labels.get(ABSENT_LAYER_HIT_UNLINKED).map(String::as_str) != Some("unlinked")
    {
        return Err(format!(
            "ABSENT-LAYER H4/C2: AbsentLayerHit::label must map Refused->\"refused\" and Unlinked->\"unlinked\" (got {labels:?})"
        ));
    }
    Ok(())
}

fn validate_take_hits_helper(function: &syn::ItemFn) -> Result<(), String> {
    let syn::ReturnType::Type(_, ty) = &function.sig.output else {
        return Err(
            "ABSENT-LAYER H4/C2: take_absent_layer_hits_for_test must return the drained hit list"
                .into(),
        );
    };
    let syn::Type::Path(path) = &**ty else {
        return Err(
            "ABSENT-LAYER H4/C2: take_absent_layer_hits_for_test must return Vec<(AbsentLayerHit, &'static str)>"
                .into(),
        );
    };
    let Some(syn::PathSegment { ident, arguments }) = path.path.segments.last() else {
        return Err("ABSENT-LAYER H4/C2: the drained hit list type is missing".into());
    };
    if ident != "Vec" {
        return Err(
            "ABSENT-LAYER H4/C2: take_absent_layer_hits_for_test must return Vec<(AbsentLayerHit, &'static str)>"
                .into(),
        );
    }
    let syn::PathArguments::AngleBracketed(generic) = arguments else {
        return Err("ABSENT-LAYER H4/C2: the drained hit list must carry its element type".into());
    };
    let element_is_pair = generic.args.first().is_some_and(|argument| {
        matches!(
            argument,
            syn::GenericArgument::Type(syn::Type::Tuple(tuple))
                if tuple.elems.len() == 2
                    && matches!(
                        &tuple.elems[0],
                        syn::Type::Path(path)
                            if path
                                .path
                                .segments
                                .last()
                                .is_some_and(|segment| segment.ident == "AbsentLayerHit")
                    )
        )
    });
    if !element_is_pair {
        return Err(
            "ABSENT-LAYER H4/C2: the drained hit list must be Vec<(AbsentLayerHit, &'static str)>"
                .into(),
        );
    }
    let facts = absent_layer_helper_facts(function);
    if !facts.paths.contains("HITS")
        || !facts.calls.contains("take")
        || !facts.methods.contains("with")
        || !facts.methods.contains("borrow_mut")
    {
        return Err(
            "ABSENT-LAYER H4/C2: take_absent_layer_hits_for_test must drain the TLS hit list with std::mem::take"
                .into(),
        );
    }
    Ok(())
}

fn validate_absent_layer_helpers(items: &[syn::Item]) -> Result<(), String> {
    validate_absent_layer_hit_enum(items)?;
    validate_take_hits_helper(absent_layer_helper(items, ABSENT_LAYER_HIT_TAKER)?)?;
    validate_record_hit(absent_layer_helper(items, "record_hit")?)?;
    validate_unlinked_helper(absent_layer_helper(items, "absent_layer_unlinked")?)?;
    validate_panic_helper(absent_layer_helper(items, "absent_layer_panic")?)?;
    validate_assert_empty_helper(absent_layer_helper(items, "absent_layer_assert_empty")?)?;
    validate_handle_helper(absent_layer_helper(items, "absent_face_registry_handle")?)?;
    validate_handle_helper(absent_layer_helper(
        items,
        "absent_terminal_registry_handle",
    )?)?;
    validate_alias_marker_helper(absent_layer_helper(items, "absent_fontset_alias_marker")?)?;
    validate_bootstrap_binder(absent_layer_helper(items, "bind_absent_bootstrap_vars")?)?;
    Ok(())
}

/// T1 second half: the six absent-by-construction slots name the system and
/// display layers, so eval's dependency sections must not link either crate
/// (nor core). A dependency edge would let the upper state exist in eval's
/// test binary, contradicting the body shape.
fn validate_eval_has_no_upper_dependency(cargo: &str) -> Result<(), String> {
    let forbidden = ["neovm-system", "neovm-display", "neovm-core"];
    let mut section = String::new();
    for line in cargo.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            section = trimmed.trim_matches(&['[', ']'][..]).to_string();
            for name in forbidden {
                if section == format!("dependencies.{name}")
                    || section == format!("dev-dependencies.{name}")
                    || section == format!("build-dependencies.{name}")
                {
                    return Err(format!(
                        "ABSENT-LAYER T1: eval has a `{section}` dependency edge; the absent-by-construction upper state would exist in its test binary"
                    ));
                }
            }
            continue;
        }
        if !(section == "dependencies"
            || section == "dev-dependencies"
            || section == "build-dependencies")
        {
            continue;
        }
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        let name = trimmed.split('=').next().unwrap_or("").trim();
        for forbidden_name in forbidden {
            if name == forbidden_name {
                return Err(format!(
                    "ABSENT-LAYER T1: eval's [{section}] contains `{name}`; the absent-by-construction upper state would exist in its test binary"
                ));
            }
        }
    }
    Ok(())
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

/// CS-1 identity drops the arm-selecting cfg attrs and every doc attribute.
/// Docs carry no semantics and legitimately differ between arms: R-a puts
/// `#[doc(hidden)]` on the test-support arm only, and the plain-pub/B3 scans
/// judge each arm's doc(hidden) separately. Requiring identical docs would
/// only force MORE duplication, which CS-1 exists to discourage (peer
/// amendment 07:28Z).
fn normalize_arm_attrs(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|attr| !attr.path().is_ident("cfg") && !attr.path().is_ident("doc"));
}

/// CS-1 arm identity: drop the arm-selecting cfg attrs, every doc attribute
/// and every visibility token, keeping derives, repr, field order/names/types
/// and all other attrs intact for an exact AST comparison.
fn normalize_cfg_split_arm(item: &mut syn::Item) {
    match item {
        syn::Item::Struct(item) => {
            normalize_arm_attrs(&mut item.attrs);
            item.vis = syn::Visibility::Inherited;
            for field in &mut item.fields {
                normalize_arm_attrs(&mut field.attrs);
                field.vis = syn::Visibility::Inherited;
            }
        }
        syn::Item::Union(item) => {
            normalize_arm_attrs(&mut item.attrs);
            item.vis = syn::Visibility::Inherited;
            for field in &mut item.fields.named {
                normalize_arm_attrs(&mut field.attrs);
                field.vis = syn::Visibility::Inherited;
            }
        }
        syn::Item::Enum(item) => {
            normalize_arm_attrs(&mut item.attrs);
            item.vis = syn::Visibility::Inherited;
            for variant in &mut item.variants {
                normalize_arm_attrs(&mut variant.attrs);
                for field in &mut variant.fields {
                    normalize_arm_attrs(&mut field.attrs);
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
            // A tuple field has no name a TEST-SUPPORT-VIS field row could
            // spell; the struct's item row covers the widening and the arm
            // identity keeps the tuple shape fixed (BaseTableView and
            // SearchRegisters are the signed tuple pairs).
            continue;
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
                // The scan is textual; a comment that MENTIONS the attribute
                // is not a widening site (jitcall's fence comment carries the
                // literal text and was misread as one).
                let trimmed = line.trim_start();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with('*')
                {
                    continue;
                }
                let code = line.split("//").next().unwrap_or("");
                if !code.contains("visibility::make(pub)") {
                    continue;
                }
                let name = declaration_after(&lines, index).ok_or_else(|| {
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

/// Name the declaration a `visibility::make(pub)` attribute applies to. A
/// single-line re-export names its first imported item (matching the old
/// text scan); a multi-line `use module::{` group names its first local
/// name, not whatever declaration follows the group.
fn declaration_after(lines: &[&str], attr_index: usize) -> Option<String> {
    let mut use_group = false;
    for next in lines.iter().skip(attr_index + 1).take(10) {
        if let Some(name) = declaration_name(next) {
            return Some(name);
        }
        let trimmed = next.trim_start();
        if use_group {
            if trimmed.starts_with('}') {
                continue;
            }
            if let Some(name) = declaration_ident(trimmed) {
                return Some(name);
            }
        }
        if trimmed.ends_with('{') && (trimmed.starts_with("use ") || trimmed.contains(" use ")) {
            use_group = true;
        }
    }
    None
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
        // An ABI string follows `extern`: `extern "C" fn ...`.
        if let Some(tail) = rest.strip_prefix('"')
            && let Some(close) = tail.find('"')
        {
            rest = tail[close + 1..].trim_start();
            advanced = true;
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
    // A `visibility::make(pub)` on a re-export names the imported item.
    if let Some(tail) = rest.strip_prefix("use ") {
        let tree = tail.trim().trim_end_matches(';').trim();
        let last = tree.rsplit("::").next().unwrap_or(tree).trim();
        let last = last.trim_matches(|c| c == '{' || c == '}' || c == ' ');
        if last.is_empty() || last == "*" {
            return None;
        }
        return declaration_ident(last);
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
    // P1-HOOKS-2: a fence-routed A' item (one with its signed AUTHORITY row
    // at the defining file) is test-support surface by the AUTHORITY ledger;
    // VIS V1 forbids a second VIS row for the same item. The AUTHORITY row is
    // therefore the coverage for its `visibility::make(pub)` site.
    let authority_path = workspace.join(ARCHITECTURE_DIR).join("AUTHORITY-ITEMS.tsv");
    if authority_path.is_file() {
        for row in auth_rows(workspace) {
            listed.insert((row.defining_file, auth_leaf(&row.item).to_owned()));
        }
    }
    let mut sites = visibility_make_sites(workspace)?;
    sites.extend(cfg_split_widening_sites(workspace)?);
    sites.extend(feature_only_pub_sites(workspace)?);
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

/// TS-COVERAGE (peer 22:14:00Z pre-P2 condition, (a)): the reverse half of
/// the ledger class, generalized to every provider crate's whole
/// `src/test_support{.rs,/**}` tree. Every PUBLIC item in that tree must have
/// exactly one row:
/// - an AUTHORITY row whose `fence_path` is `<crate>::<module>::<leaf>` (the
///   signed fence route), or
/// - a TEST-SUPPORT-VIS row whose defining file is the item's file (the
///   signed read route).
/// An item may never have both (V1) and never more than one. `use`
/// re-exports and inline modules are not items in this arm (the forward
/// visibility-coverage check owns their widening sites); impl methods are not
/// top-level items either.
fn test_support_reverse_coverage_violation(
    workspace: &Path,
    vis_rows: &[VisRow],
    auth: &[AuthRow],
) -> Result<(), String> {
    #[derive(Clone)]
    struct TreeItem {
        crate_dir: String,
        file: String,
        module: Vec<String>,
        kind: &'static str,
        leaf: String,
    }

    fn collect(
        items: &[syn::Item],
        crate_dir: &str,
        file: &str,
        module: &mut Vec<String>,
        out: &mut Vec<TreeItem>,
    ) {
        let is_public = |vis: &syn::Visibility| matches!(vis, syn::Visibility::Public(_));
        for item in items {
            let leaf_kind = match item {
                syn::Item::Fn(item) if is_public(&item.vis) => {
                    Some(("fn", item.sig.ident.to_string()))
                }
                syn::Item::Struct(item) if is_public(&item.vis) => {
                    Some(("struct", item.ident.to_string()))
                }
                syn::Item::Enum(item) if is_public(&item.vis) => {
                    Some(("enum", item.ident.to_string()))
                }
                syn::Item::Union(item) if is_public(&item.vis) => {
                    Some(("union", item.ident.to_string()))
                }
                syn::Item::Trait(item) if is_public(&item.vis) => {
                    Some(("trait", item.ident.to_string()))
                }
                syn::Item::Type(item) if is_public(&item.vis) => {
                    Some(("type", item.ident.to_string()))
                }
                syn::Item::Const(item) if is_public(&item.vis) => {
                    Some(("const", item.ident.to_string()))
                }
                syn::Item::Static(item) if is_public(&item.vis) => {
                    Some(("static", item.ident.to_string()))
                }
                syn::Item::Mod(item) if is_public(&item.vis) => {
                    if let Some((_, inner)) = &item.content {
                        module.push(item.ident.to_string());
                        collect(inner, crate_dir, file, module, out);
                        module.pop();
                    }
                    None
                }
                _ => None,
            };
            if let Some((kind, leaf)) = leaf_kind {
                out.push(TreeItem {
                    crate_dir: crate_dir.to_owned(),
                    file: file.to_owned(),
                    module: module.clone(),
                    kind,
                    leaf,
                });
            }
        }
    }

    let mut items = Vec::new();
    for crate_dir in VIS_CRATES {
        let src = workspace.join("crates").join(crate_dir).join("src");
        let mut files = Vec::new();
        let plain = src.join("test_support.rs");
        if plain.is_file() {
            files.push(plain);
        }
        let nested = src.join("test_support");
        if nested.is_dir() {
            walk_files(&nested, &mut files)?;
        }
        files.sort();
        for path in files {
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let relative = path
                .strip_prefix(workspace)
                .map_err(|_| format!("{} is outside the workspace", path.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            let after_src = relative
                .split_once("/src/")
                .map(|(_, rest)| rest)
                .ok_or_else(|| format!("{relative}: not under src/"))?;
            let mut segments: Vec<String> = after_src
                .trim_end_matches(".rs")
                .split('/')
                .map(str::to_string)
                .collect();
            if segments.last().map(String::as_str) == Some("mod") {
                segments.pop();
            }
            let parsed =
                syn::parse_file(&read(&path)?).map_err(|error| format!("{relative}: {error}"))?;
            collect(
                &parsed.items,
                crate_dir,
                &relative,
                &mut segments,
                &mut items,
            );
        }
    }
    for item in &items {
        let vis_hits: Vec<&VisRow> = vis_rows
            .iter()
            .filter(|row| {
                row.crate_dir == item.crate_dir
                    && row.file == item.file
                    && row.item.rsplit("::").next() == Some(item.leaf.as_str())
            })
            .collect();
        let root = item.crate_dir.replace('-', "_");
        let expected_fence = format!("{root}::{}::{}", item.module.join("::"), item.leaf);
        let auth_hits: Vec<&AuthRow> = auth
            .iter()
            .filter(|row| row.crate_name == item.crate_dir && auth_expected(row) == expected_fence)
            .collect();
        if vis_hits.is_empty() && auth_hits.is_empty() {
            return Err(format!(
                "TS-COVERAGE {}: pub {} `{}` has no AUTHORITY fence row and no TEST-SUPPORT-VIS row",
                item.file, item.kind, item.leaf
            ));
        }
        if !vis_hits.is_empty() && !auth_hits.is_empty() {
            return Err(format!(
                "TS-COVERAGE {}: pub {} `{}` has BOTH an AUTHORITY fence row and a TEST-SUPPORT-VIS row (V1)",
                item.file, item.kind, item.leaf
            ));
        }
        let total = vis_hits.len() + auth_hits.len();
        if total > 1 {
            return Err(format!(
                "TS-COVERAGE {}: pub {} `{}` has {total} rows (exactly one required)",
                item.file, item.kind, item.leaf
            ));
        }
    }
    Ok(())
}

/// Resolve a full module path written in `from` (a glob target), keeping
/// every segment: `crate`/`neovm_value` roots, `self` and leading `super`.
fn expansion_resolve_module(from: &[String], path: &[String]) -> Option<Vec<String>> {
    let mut base;
    let mut index = 0;
    match path.first()?.as_str() {
        "crate" | "neovm_value" => {
            base = Vec::new();
            index = 1;
        }
        "self" => {
            base = from.to_vec();
            index = 1;
        }
        "super" => {
            base = from.to_vec();
            while index < path.len() && path[index] == "super" {
                base.pop()?;
                index += 1;
            }
        }
        _ => base = from.to_vec(),
    }
    base.extend(path[index..].iter().cloned());
    Some(base)
}

#[derive(Default)]
struct ModExports {
    own: BTreeSet<String>,
    globs: Vec<Vec<String>>,
    /// Peer 15:0xZ alias case: `pub use module::Item as Alias` makes `Item`
    /// root-public under a different leaf, even though the exported name set
    /// only carries `Alias`. Record `(origin module, item)` so the mod-row
    /// expansion can accept an already-public item that is re-exported under
    /// an alias.
    origins: Vec<(Vec<String>, String)>,
}

/// Resolve a module declaration's file and the directory its own child
/// modules live in. An explicit `#[path]` is relative to the declaring file's
/// directory and its children resolve beside the referenced file; otherwise
/// the standard `name.rs` / `name/mod.rs` lookup under the module's child
/// directory applies.
fn expansion_module_target(
    module: &syn::ItemMod,
    dir: &Path,
    file_dir: &Path,
) -> Option<(PathBuf, PathBuf)> {
    for attr in &module.attrs {
        if attr.path().is_ident("path") {
            if let syn::Meta::NameValue(value) = &attr.meta {
                if let syn::Expr::Lit(expr) = &value.value {
                    if let syn::Lit::Str(text) = &expr.lit {
                        let file = file_dir.join(text.value());
                        let child_dir = file.parent().unwrap_or(dir).to_path_buf();
                        return Some((file, child_dir));
                    }
                }
            }
        }
    }
    let name = module.ident.to_string();
    let file = auth_module_file(dir, &name)?;
    Some((file, dir.join(name)))
}

/// The production root-public surface of one crate: exported names plus the
/// `(origin module, item)` pairs behind those names. Peer 15:0xZ: an item
/// re-exported as `pub use module::Item as Alias` is root-public even though
/// the exported NAME set carries only `Alias`, so alias-aware coverage matches
/// the origin pair against the widened module instead of the leaf name.
#[derive(Default)]
struct ProductionSurface {
    names: BTreeSet<String>,
    origins: BTreeSet<(Vec<String>, String)>,
    /// Source file of every module (inline modules share their parent file),
    /// so a re-export can be resolved to the file that defines the item.
    module_files: BTreeMap<Vec<String>, String>,
    /// Every module's re-export origins (private modules included), used to
    /// follow same-name `pub use` chains to the defining module.
    reexports: BTreeMap<Vec<String>, Vec<(Vec<String>, String)>>,
}

/// LEAD 12:3xZ: a VIS `mod` row is a blanket widening -- every pub item the
/// module makes reachable becomes test-support surface. Build the crate's
/// production-public surface (declared pub items, pub re-export leaves and
/// glob contents, and pub inherent methods on public types, through module
/// chains whose declarations are all `pub`), so a mod row can require each
/// newly reachable item to be root-public already or rowed.
fn production_public_surface(
    workspace: &Path,
    crate_dir: &str,
) -> Result<ProductionSurface, String> {
    #[allow(clippy::too_many_arguments)]
    fn build(
        items: &[syn::Item],
        segments: Vec<String>,
        dir: &Path,
        file_dir: &Path,
        is_public: bool,
        workspace: &Path,
        module_file: &str,
        modules: &mut BTreeMap<Vec<String>, ModExports>,
        module_files: &mut BTreeMap<Vec<String>, String>,
        reexports: &mut BTreeMap<Vec<String>, Vec<(Vec<String>, String)>>,
        public_modules: &mut Vec<Vec<String>>,
        visited: &mut BTreeSet<PathBuf>,
    ) -> Result<(), String> {
        let mut exports = ModExports::default();
        let mut nested = Vec::new();
        let mut inline = Vec::new();
        for item in items {
            match item {
                syn::Item::Mod(module) => {
                    if test_gated(&module.attrs) {
                        continue;
                    }
                    let child_public =
                        is_public && matches!(module.vis, syn::Visibility::Public(_));
                    let mut child_segments = segments.clone();
                    child_segments.push(module.ident.to_string());
                    if let Some((_, inner)) = &module.content {
                        let inline_dir = dir.join(module.ident.to_string());
                        inline.push((inner, child_segments, inline_dir, child_public));
                    } else if let Some((file, child_dir)) =
                        expansion_module_target(module, dir, file_dir)
                    {
                        nested.push((file, child_dir, child_segments, child_public));
                    }
                }
                syn::Item::Use(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                    let mut leaves = Vec::new();
                    auth_use_leaves(&item.tree, &mut Vec::new(), &mut leaves);
                    for (path, alias, glob) in leaves {
                        if glob {
                            exports.globs.push(path);
                        } else {
                            // The exported NAME may be an alias; the ORIGIN
                            // (module path + item) is what the alias-aware
                            // coverage check matches against a widened module.
                            if let Some((name, module)) = path.split_last()
                                && let Some(resolved) = expansion_resolve_module(&segments, module)
                            {
                                exports.origins.push((resolved, name.clone()));
                            }
                            if let Some(name) = alias.or_else(|| path.last().cloned()) {
                                exports.own.insert(name);
                            }
                        }
                    }
                }
                syn::Item::Fn(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                    exports.own.insert(item.sig.ident.to_string());
                }
                syn::Item::Struct(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                    exports.own.insert(item.ident.to_string());
                }
                syn::Item::Enum(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                    exports.own.insert(item.ident.to_string());
                }
                syn::Item::Union(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                    exports.own.insert(item.ident.to_string());
                }
                syn::Item::Trait(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                    exports.own.insert(item.ident.to_string());
                }
                syn::Item::Type(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                    exports.own.insert(item.ident.to_string());
                }
                syn::Item::Const(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                    exports.own.insert(item.ident.to_string());
                }
                syn::Item::Static(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                    exports.own.insert(item.ident.to_string());
                }
                syn::Item::Impl(block) if block.trait_.is_none() => {
                    let owner = match &*block.self_ty {
                        syn::Type::Path(path) => path
                            .path
                            .segments
                            .last()
                            .map(|segment| segment.ident.to_string()),
                        _ => None,
                    };
                    if let Some(owner) = owner {
                        for member in &block.items {
                            if let syn::ImplItem::Fn(function) = member {
                                if matches!(function.vis, syn::Visibility::Public(_)) {
                                    exports
                                        .own
                                        .insert(format!("{owner}::{}", function.sig.ident));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if is_public {
            public_modules.push(segments.clone());
        }
        module_files.insert(segments.clone(), module_file.to_owned());
        reexports.insert(segments.clone(), exports.origins.clone());
        modules.insert(segments, exports);
        for (items, child_segments, inline_dir, child_public) in inline {
            build(
                items,
                child_segments,
                &inline_dir,
                &inline_dir,
                child_public,
                workspace,
                module_file,
                modules,
                module_files,
                reexports,
                public_modules,
                visited,
            )?;
        }
        for (file, child_dir, child_segments, child_public) in nested {
            let canonical = file
                .canonicalize()
                .map_err(|error| format!("{}: {error}", file.display()))?;
            if !visited.insert(canonical.clone()) {
                continue;
            }
            let text = read(&canonical)?;
            let ast = syn::parse_file(&text)
                .map_err(|error| format!("{}: {error}", canonical.display()))?;
            let child_file_dir = canonical.parent().unwrap_or(&child_dir);
            let child_relative = canonical
                .strip_prefix(workspace)
                .map_err(|_| format!("{} is outside the workspace", canonical.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            build(
                &ast.items,
                child_segments,
                &child_dir,
                child_file_dir,
                child_public,
                workspace,
                &child_relative,
                modules,
                module_files,
                reexports,
                public_modules,
                visited,
            )?;
        }
        Ok(())
    }
    fn exports_of(
        segments: &[String],
        modules: &BTreeMap<Vec<String>, ModExports>,
        memo: &mut BTreeMap<Vec<String>, BTreeSet<String>>,
        stack: &mut BTreeSet<Vec<String>>,
    ) -> BTreeSet<String> {
        if let Some(names) = memo.get(segments) {
            return names.clone();
        }
        let Some(data) = modules.get(segments) else {
            return BTreeSet::new();
        };
        if !stack.insert(segments.to_vec()) {
            return BTreeSet::new();
        }
        let mut names = data.own.clone();
        for glob in &data.globs {
            if let Some(target) = expansion_resolve_module(segments, glob) {
                names.extend(exports_of(&target, modules, memo, stack));
            }
        }
        stack.remove(segments);
        memo.insert(segments.to_vec(), names.clone());
        names
    }
    /// Origin pairs of the names a public module re-exports, including glob
    /// chains, mirroring `exports_of`.
    fn origins_of(
        segments: &[String],
        modules: &BTreeMap<Vec<String>, ModExports>,
        memo: &mut BTreeMap<Vec<String>, BTreeSet<(Vec<String>, String)>>,
        stack: &mut BTreeSet<Vec<String>>,
    ) -> BTreeSet<(Vec<String>, String)> {
        if let Some(origins) = memo.get(segments) {
            return origins.clone();
        }
        let Some(data) = modules.get(segments) else {
            return BTreeSet::new();
        };
        if !stack.insert(segments.to_vec()) {
            return BTreeSet::new();
        }
        let mut origins: BTreeSet<(Vec<String>, String)> = data.origins.iter().cloned().collect();
        for glob in &data.globs {
            if let Some(target) = expansion_resolve_module(segments, glob) {
                origins.extend(origins_of(&target, modules, memo, stack));
            }
        }
        stack.remove(segments);
        memo.insert(segments.to_vec(), origins.clone());
        origins
    }
    let src = workspace.join("crates").join(crate_dir).join("src");
    let mut modules = BTreeMap::new();
    let mut module_files = BTreeMap::new();
    let mut reexports = BTreeMap::new();
    let mut public_modules = Vec::new();
    let mut visited = BTreeSet::new();
    for root in ["lib.rs", "main.rs"] {
        let path = src.join(root);
        if !path.is_file() {
            continue;
        }
        let canonical = path
            .canonicalize()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if !visited.insert(canonical.clone()) {
            continue;
        }
        let relative = canonical
            .strip_prefix(workspace)
            .map_err(|_| format!("{} is outside the workspace", canonical.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        let text = read(&canonical)?;
        let ast =
            syn::parse_file(&text).map_err(|error| format!("{}: {error}", canonical.display()))?;
        let dir = auth_module_dir(&canonical);
        let file_dir = canonical.parent().unwrap_or(&dir);
        build(
            &ast.items,
            Vec::new(),
            &dir,
            file_dir,
            true,
            workspace,
            &relative,
            &mut modules,
            &mut module_files,
            &mut reexports,
            &mut public_modules,
            &mut visited,
        )?;
    }
    let mut memo = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut origins_memo = BTreeMap::new();
    let mut origins = BTreeSet::new();
    for segments in &public_modules {
        names.extend(exports_of(
            segments,
            &modules,
            &mut memo,
            &mut BTreeSet::new(),
        ));
        origins.extend(origins_of(
            segments,
            &modules,
            &mut origins_memo,
            &mut BTreeSet::new(),
        ));
    }
    Ok(ProductionSurface {
        names,
        origins,
        module_files,
        reexports,
    })
}

/// Follow `pub use` chains to the file that actually defines `item` in
/// `module` (same-name re-export routes such as FunctionOpKind's
/// `pub use functions::FunctionOpKind;`).
fn resolved_definition_file(
    module: &[String],
    item: &str,
    surface: &ProductionSurface,
    visited: &mut BTreeSet<(Vec<String>, String)>,
) -> Option<String> {
    let key = (module.to_vec(), item.to_owned());
    if !visited.insert(key) {
        return None;
    }
    if let Some(list) = surface.reexports.get(module) {
        for (next, next_item) in list {
            if next_item == item
                && let Some(file) = resolved_definition_file(next, item, surface, visited)
            {
                return Some(file);
            }
        }
    }
    surface.module_files.get(module).cloned()
}

/// A re-export inside a widened module is acceptable when the item it
/// re-exports is already production-public (under any alias) or has its own
/// VIS/AUTHORITY row at its defining file; the route adds a spelling, not an
/// item.
fn use_leaf_covered(
    source_item: &str,
    origin_module: &[String],
    surface: &ProductionSurface,
    vis_rows: &[VisRow],
    authority: &[AuthRow],
) -> bool {
    if surface
        .origins
        .contains(&(origin_module.to_vec(), source_item.to_owned()))
    {
        return true;
    }
    let Some(file) =
        resolved_definition_file(origin_module, source_item, surface, &mut BTreeSet::new())
    else {
        return false;
    };
    let vis_hit = vis_rows.iter().any(|row| {
        row.kind != "mod"
            && row.file.as_str() == file.as_str()
            && (row.item == source_item || row.item.ends_with(&format!("::{source_item}")))
    });
    if vis_hit {
        return true;
    }
    authority.iter().any(|row| {
        row.defining_file.as_str() == file.as_str()
            && (row.item == source_item || row.item.ends_with(&format!("::{source_item}")))
    })
}

#[allow(clippy::too_many_arguments)]
fn expansion_covered(
    identity: &str,
    owner: Option<&str>,
    relative: &str,
    segments: &[String],
    surface: &ProductionSurface,
    declared_types: &BTreeSet<String>,
    vis_rows: &[VisRow],
    authority: &[AuthRow],
) -> bool {
    if surface.names.contains(identity) {
        return true;
    }
    // Peer 15:0xZ: an item re-exported production-public under an alias is
    // already public surface; the widened module path does not expose a new
    // item. Match the origin pair, not the exported leaf name.
    if surface
        .origins
        .contains(&(segments.to_vec(), identity.to_owned()))
    {
        return true;
    }
    if let Some(owner) = owner {
        if surface.names.contains(owner) || declared_types.contains(owner) {
            return true;
        }
    }
    let vis_hit = vis_rows.iter().any(|row| {
        row.kind != "mod"
            && row.file == relative
            && (row.item == identity || row.item.ends_with(&format!("::{identity}")))
    });
    if vis_hit {
        return true;
    }
    authority.iter().any(|row| {
        row.defining_file == relative
            && (row.item == identity || row.item.ends_with(&format!("::{identity}")))
    })
}

#[allow(clippy::too_many_arguments)]
fn expand_module_items(
    items: &[syn::Item],
    segments: &[String],
    dir: &Path,
    relative: &str,
    workspace: &Path,
    surface: &ProductionSurface,
    vis_rows: &[VisRow],
    authority: &[AuthRow],
    declared: &mut BTreeSet<String>,
    pending_methods: &mut Vec<(String, String, String)>,
    missing: &mut Vec<String>,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    for item in items {
        match item {
            syn::Item::Mod(module) => {
                if expansion_gated(&module.attrs)
                    || !matches!(module.vis, syn::Visibility::Public(_))
                {
                    continue;
                }
                if let Some((_, inner)) = &module.content {
                    let inline_dir = dir.join(module.ident.to_string());
                    let mut child_segments = segments.to_vec();
                    child_segments.push(module.ident.to_string());
                    expand_module_items(
                        inner,
                        &child_segments,
                        &inline_dir,
                        relative,
                        workspace,
                        surface,
                        vis_rows,
                        authority,
                        declared,
                        pending_methods,
                        missing,
                        visited,
                    )?;
                } else if let Some(target) = auth_module_file(dir, &module.ident.to_string()) {
                    let canonical = target
                        .canonicalize()
                        .map_err(|error| format!("{}: {error}", target.display()))?;
                    if !visited.insert(canonical.clone()) {
                        continue;
                    }
                    let text = read(&canonical)?;
                    let ast = syn::parse_file(&text)
                        .map_err(|error| format!("{}: {error}", canonical.display()))?;
                    let child_relative = canonical
                        .strip_prefix(workspace)
                        .map_err(|_| format!("{} is outside the workspace", canonical.display()))?
                        .to_string_lossy()
                        .replace('\\', "/");
                    let child_dir = auth_module_dir(&canonical);
                    let mut child_segments = segments.to_vec();
                    child_segments.push(module.ident.to_string());
                    expand_module_items(
                        &ast.items,
                        &child_segments,
                        &child_dir,
                        &child_relative,
                        workspace,
                        surface,
                        vis_rows,
                        authority,
                        declared,
                        pending_methods,
                        missing,
                        visited,
                    )?;
                }
            }
            syn::Item::Fn(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                let identity = item.sig.ident.to_string();
                if !expansion_covered(
                    &identity, None, relative, segments, surface, declared, vis_rows, authority,
                ) {
                    missing.push(format!("{relative}:{identity}"));
                }
            }
            syn::Item::Struct(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                let identity = item.ident.to_string();
                declared.insert(identity.clone());
                if !expansion_covered(
                    &identity, None, relative, segments, surface, declared, vis_rows, authority,
                ) {
                    missing.push(format!("{relative}:{identity}"));
                }
            }
            syn::Item::Enum(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                let identity = item.ident.to_string();
                declared.insert(identity.clone());
                if !expansion_covered(
                    &identity, None, relative, segments, surface, declared, vis_rows, authority,
                ) {
                    missing.push(format!("{relative}:{identity}"));
                }
            }
            syn::Item::Union(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                let identity = item.ident.to_string();
                declared.insert(identity.clone());
                if !expansion_covered(
                    &identity, None, relative, segments, surface, declared, vis_rows, authority,
                ) {
                    missing.push(format!("{relative}:{identity}"));
                }
            }
            syn::Item::Trait(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                let identity = item.ident.to_string();
                declared.insert(identity.clone());
                if !expansion_covered(
                    &identity, None, relative, segments, surface, declared, vis_rows, authority,
                ) {
                    missing.push(format!("{relative}:{identity}"));
                }
            }
            syn::Item::Type(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                let identity = item.ident.to_string();
                declared.insert(identity.clone());
                if !expansion_covered(
                    &identity, None, relative, segments, surface, declared, vis_rows, authority,
                ) {
                    missing.push(format!("{relative}:{identity}"));
                }
            }
            syn::Item::Const(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                let identity = item.ident.to_string();
                if !expansion_covered(
                    &identity, None, relative, segments, surface, declared, vis_rows, authority,
                ) {
                    missing.push(format!("{relative}:{identity}"));
                }
            }
            syn::Item::Static(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                let identity = item.ident.to_string();
                if !expansion_covered(
                    &identity, None, relative, segments, surface, declared, vis_rows, authority,
                ) {
                    missing.push(format!("{relative}:{identity}"));
                }
            }
            syn::Item::Impl(block) if block.trait_.is_none() => {
                let owner = match &*block.self_ty {
                    syn::Type::Path(path) => path
                        .path
                        .segments
                        .last()
                        .map(|segment| segment.ident.to_string()),
                    _ => None,
                };
                if let Some(owner) = owner {
                    for member in &block.items {
                        if let syn::ImplItem::Fn(function) = member {
                            if matches!(function.vis, syn::Visibility::Public(_)) {
                                pending_methods.push((
                                    format!("{owner}::{}", function.sig.ident),
                                    owner.clone(),
                                    relative.to_string(),
                                ));
                            }
                        }
                    }
                }
            }
            syn::Item::Use(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                let mut leaves = Vec::new();
                auth_use_leaves(&item.tree, &mut Vec::new(), &mut leaves);
                for (path, alias, glob) in leaves {
                    if glob {
                        missing.push(format!(
                            "{relative}: pub use glob {}::* requires review",
                            path.join("::")
                        ));
                        continue;
                    }
                    let Some(identity) = alias.or_else(|| path.last().cloned()) else {
                        continue;
                    };
                    // A re-export route whose SOURCE item is already public or
                    // rowed adds a spelling, not an item: resolve the origin
                    // through same-name `pub use` chains and accept it.
                    if let Some((source_item, module_path)) = path.split_last()
                        && let Some(origin_module) = expansion_resolve_module(segments, module_path)
                        && use_leaf_covered(
                            source_item,
                            &origin_module,
                            surface,
                            vis_rows,
                            authority,
                        )
                    {
                        continue;
                    }
                    if !expansion_covered(
                        &identity, None, relative, segments, surface, declared, vis_rows, authority,
                    ) {
                        missing.push(format!("{relative}:{identity}"));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// LEAD 12:3xZ: expand every VIS `mod` row to the pub items it makes reachable
/// and require each to be root-public already or covered by its own VIS or
/// AUTHORITY row. A blanket mod widening with an unrowed new pub item fails
/// closed.
fn expanded_mod_rows_violation(
    workspace: &Path,
    rows: &[VisRow],
    authority: &[AuthRow],
) -> Result<(), String> {
    // Canonicalize the root ONCE, at guard entry.  The expansion canonicalizes
    // every source path and then derives the workspace-relative name with
    // `strip_prefix(workspace)`; a root that arrives as a symlink (macOS
    // `/tmp` and `/var` are links to `/private/...`) makes every strip fail
    // with "is outside the workspace" and reds the mod-row self-tests on
    // arm64 while Linux stays green.  The outside-workspace check is
    // unchanged: a file genuinely outside the canonical root still fails.
    let canonical_workspace = workspace
        .canonicalize()
        .map_err(|error| format!("{}: {error}", workspace.display()))?;
    let workspace = canonical_workspace.as_path();
    let mut public_cache: BTreeMap<String, ProductionSurface> = BTreeMap::new();
    let mut failures = Vec::new();
    for row in rows.iter().filter(|row| row.kind == "mod") {
        if !public_cache.contains_key(&row.crate_dir) {
            let surface = production_public_surface(workspace, &row.crate_dir)?;
            public_cache.insert(row.crate_dir.clone(), surface);
        }
        let surface = public_cache
            .get(&row.crate_dir)
            .expect("production surface inserted above");
        let segments: Vec<String> = row.item.split("::").map(str::to_owned).collect();
        let declaring = workspace.join(&row.file);
        let dir = auth_module_dir(&declaring);
        let name = row.item.rsplit("::").next().unwrap_or(&row.item);
        // A file-module row may name the module's own source file: for a
        // file module the defining file and the module are the same file.
        // The historical spelling names the parent that declares the module;
        // both must expand the same pub surface.
        let self_file = declaring
            .file_name()
            .and_then(|file| file.to_str())
            .is_some_and(|file| file == format!("{name}.rs"));
        let declaring_text = read(&declaring)?;
        let declaring_ast = syn::parse_file(&declaring_text)
            .map_err(|error| format!("{}: {error}", declaring.display()))?;
        let declarations: Vec<&syn::ItemMod> = declaring_ast
            .items
            .iter()
            .filter_map(|item| match item {
                syn::Item::Mod(module) if module.ident == name => Some(module),
                _ => None,
            })
            .collect();
        // A cfg-split module has one declaration per arm; expand the arm
        // that is actually compiled in the test-support build (the widened
        // one), not the production arm that is definitively false there.
        let declaration = declarations
            .iter()
            .copied()
            .find(|module| !expansion_gated(&module.attrs));
        if declaration.is_none() && !declarations.is_empty() {
            // Every arm is test-build-only (e.g. `#[cfg(test)] mod tests`):
            // inert in the test-support build, no expansion.
            continue;
        }
        let mut declared = BTreeSet::new();
        let mut pending_methods = Vec::new();
        let mut missing = Vec::new();
        let mut visited = BTreeSet::new();
        let mut target: Option<(PathBuf, PathBuf)> = None;
        if self_file {
            target = Some((declaring.clone(), auth_module_dir(&declaring)));
        }
        if target.is_none() {
            match declaration {
                Some(module) if module.content.is_some() => {
                    // An inline module row (`#[cfg_attr(feature =
                    // "test-support", visibility::make(pub))] #[cfg(test)] mod
                    // tests { .. }`). A module not compiled in the test-support
                    // build is inert and needs no expansion.
                    if expansion_gated(&module.attrs) {
                        continue;
                    }
                    let (_, inner) = module.content.as_ref().expect("checked above");
                    let inline_dir = dir.join(name);
                    expand_module_items(
                        inner,
                        &segments,
                        &inline_dir,
                        &row.file,
                        workspace,
                        surface,
                        rows,
                        authority,
                        &mut declared,
                        &mut pending_methods,
                        &mut missing,
                        &mut visited,
                    )?;
                }
                Some(module) => {
                    if expansion_gated(&module.attrs) {
                        continue;
                    }
                    let declaring_dir = declaring.parent().unwrap_or(&dir);
                    match expansion_module_target(module, &dir, declaring_dir) {
                        Some(found) => target = Some(found),
                        None => {
                            failures.push(format!(
                            "mod row {} does not resolve to a module under {} (the kind must name the widened item's actual kind)",
                            row.item,
                            dir.display()
                        ));
                            continue;
                        }
                    }
                }
                None => match auth_module_file(&dir, name) {
                    Some(file) => target = Some((file, dir.join(name))),
                    None => {
                        failures.push(format!(
                        "mod row {} does not resolve to a module under {} (the kind must name the widened item's actual kind)",
                        row.item,
                        dir.display()
                    ));
                        continue;
                    }
                },
            }
        }
        if let Some((module_file, child_dir)) = target {
            let text = read(&module_file)?;
            let ast = syn::parse_file(&text)
                .map_err(|error| format!("{}: {error}", module_file.display()))?;
            let canonical = module_file
                .canonicalize()
                .map_err(|error| format!("{}: {error}", module_file.display()))?;
            let relative = canonical
                .strip_prefix(workspace)
                .map_err(|_| format!("{} is outside the workspace", module_file.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            visited.insert(canonical);
            expand_module_items(
                &ast.items,
                &segments,
                &child_dir,
                &relative,
                workspace,
                surface,
                rows,
                authority,
                &mut declared,
                &mut pending_methods,
                &mut missing,
                &mut visited,
            )?;
        }
        for (identity, owner, relative) in pending_methods {
            // A pub method on a type that is neither public on a production
            // path nor declared pub in the widened subtree is not publicly
            // reachable: the widening cannot expose it by name.
            if !surface.names.contains(&owner) && !declared.contains(&owner) {
                continue;
            }
            if !expansion_covered(
                &identity,
                Some(&owner),
                &relative,
                &segments,
                surface,
                &declared,
                rows,
                authority,
            ) {
                missing.push(format!("{relative}:{identity}"));
            }
        }
        if !missing.is_empty() {
            failures.push(format!(
                "mod row {} is a blanket widening; each pub item it makes reachable needs a root-public path or its own VIS/AUTHORITY row: {}",
                row.item,
                missing.join(", ")
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "TEST-SUPPORT-VIS: {} mod-row expansion failure(s): {}",
            failures.len(),
            failures.join(" | ")
        ))
    }
}

// ---------------------------------------------------------------------------
// ROW -> ITEM VISIBILITY (display re-home r2, peer #981 R2).
//
// Forward coverage proves that every widening site has a row. The reverse
// direction is the peer's stale-row finding: `mode_line_numeric_padding::
// force_for_test` was a pub(crate) TLS writer with a VIS row and NO widening,
// and the old guard accepted it because it only checked that the leaf occurred
// in the file. This arm parses the row's defining file and requires the named
// item to be cross-crate visible in the test-support build:
//   * a plain `pub` declaration (production-public surface), or
//   * a `visibility::make(pub)` attribute (possibly under cfg_attr), or
//   * the pub arm of a cfg-split item/field pair.
// A `pub(crate)`/private item with no such route is red: it is not
// test-support surface and cannot be named cross-crate at all.
//
// Scope: this arm is enabled for all six provider ledgers
// (VIS_ROW_VISIBILITY_CRATES).  The resolver rules cover the declaration
// shapes those ledgers need: impl consts (both the top-level item visitor and
// the impl-item visitor run for kind const), macro bodies for static/use/type
// rows (a visibility marker plus the kind's leaf inside the flattened token
// stream), self-file module rows, and use-tree routes for any kind.
// ---------------------------------------------------------------------------

const VIS_ROW_VISIBILITY_CRATES: &[&str] = &[
    "neovm-value",
    "neovm-edit",
    "neovm-text",
    "neovm-eval",
    "neovm-display",
    "neovm-system",
];

fn meta_makes_pub(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::List(list) => {
            list.path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "make")
                && list.tokens.to_string().trim() == "pub"
        }
        _ => false,
    }
}

fn attr_makes_public(attr: &syn::Attribute) -> bool {
    if meta_makes_pub(&attr.meta) {
        return true;
    }
    if !attr.path().is_ident("cfg_attr") {
        return false;
    }
    let Ok(args) = attr.parse_args_with(
        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
    ) else {
        return false;
    };
    args.iter().skip(1).any(meta_makes_pub)
}

fn decl_cross_crate_visible(vis: &syn::Visibility, attrs: &[syn::Attribute]) -> bool {
    matches!(vis, syn::Visibility::Public(_)) || attrs.iter().any(attr_makes_public)
}

fn named_fields_visible<'a>(
    fields: impl Iterator<Item = &'a syn::Field>,
    leaf: &str,
) -> Option<bool> {
    let mut found = false;
    for field in fields {
        if field.ident.as_ref().is_some_and(|ident| ident == leaf) {
            found = true;
            if decl_cross_crate_visible(&field.vis, &field.attrs) {
                return Some(true);
            }
        }
    }
    found.then_some(false)
}

fn fields_visible_in_item(item: &syn::Item, leaf: &str) -> Option<bool> {
    match item {
        syn::Item::Struct(item) => match &item.fields {
            syn::Fields::Named(named) => named_fields_visible(named.named.iter(), leaf),
            _ => None,
        },
        syn::Item::Union(item) => named_fields_visible(item.fields.named.iter(), leaf),
        _ => None,
    }
}

/// Whether the source declares `item` (qualified path spelling) and whether
/// the declaration is cross-crate visible in the test-support build.
/// `Ok(None)` means no matching declaration was found (other checks own that
/// case); `Ok(Some(false))` is the stale-row condition this arm rejects.
/// Flatten a macro token stream (recursing into groups) into identifier,
/// literal and punctuation texts.
fn token_texts(tokens: proc_macro2::TokenStream, out: &mut Vec<String>) {
    for token in tokens {
        match token {
            proc_macro2::TokenTree::Group(group) => token_texts(group.stream(), out),
            proc_macro2::TokenTree::Ident(ident) => out.push(ident.to_string()),
            proc_macro2::TokenTree::Literal(literal) => out.push(literal.to_string()),
            proc_macro2::TokenTree::Punct(punct) => out.push(punct.to_string()),
        }
    }
}

/// A macro-generated item is a cross-crate route when the macro body both
/// carries an explicit visibility marker (`pub`, or `visibility`+`make`) and
/// declares the kind's leaf.  `thread_local!` statics carry
/// `#[cfg_attr(..., visibility::make(pub))] static NAME` inside the body;
/// glob imports (`use m::*`) never name a leaf and stay fail-closed.
fn macro_body_decl_visible(tokens: &[String], leaf: &str, kind: &str) -> bool {
    let has_visibility = tokens.iter().any(|token| token == "pub")
        || (tokens.iter().any(|token| token == "visibility")
            && tokens.iter().any(|token| token == "make"));
    if !has_visibility {
        return false;
    }
    let declaration_keywords: &[&str] = match kind {
        "static" => &["static"],
        "type" => &["type", "struct", "enum", "union"],
        "use" => &["use"],
        _ => return false,
    };
    for i in 0..tokens.len() {
        if !declaration_keywords.contains(&tokens[i].as_str()) {
            continue;
        }
        if kind == "use" {
            let mut j = i + 1;
            while j < tokens.len() && tokens[j] != ";" {
                if tokens[j] == leaf {
                    return true;
                }
                j += 1;
            }
        } else if tokens.get(i + 1).map(String::as_str) == Some(leaf) {
            return true;
        }
    }
    false
}

fn visible_decl_in_source(source: &str, item: &str, kind: &str) -> Result<Option<bool>, String> {
    let parsed = syn::parse_file(source).map_err(|error| format!("row defining file: {error}"))?;
    let segments: Vec<&str> = item.split("::").collect();
    let leaf = segments.last().copied().unwrap_or(item);
    let parent = segments
        .get(segments.len().saturating_sub(2))
        .copied()
        .unwrap_or(leaf);

    fn visit_use_tree(tree: &syn::UseTree, leaf: &str) -> bool {
        match tree {
            // A path segment names its module: `pub use helpers::*` makes the
            // `helpers` module's items reachable and names `helpers` itself.
            // The glob still names no item (only the path segment matches).
            syn::UseTree::Path(path) => path.ident == leaf || visit_use_tree(&path.tree, leaf),
            syn::UseTree::Name(name) => name.ident == leaf,
            syn::UseTree::Rename(rename) => rename.rename == leaf || rename.ident == leaf,
            syn::UseTree::Glob(_) => false,
            syn::UseTree::Group(group) => group.items.iter().any(|t| visit_use_tree(t, leaf)),
        }
    }

    fn visit_impl_methods(
        items: &[syn::Item],
        leaf: &str,
        parent: &str,
        kind: &str,
        found: &mut Option<bool>,
    ) {
        for item in items {
            match item {
                syn::Item::Mod(module) => {
                    if let Some((_, inner)) = &module.content {
                        visit_impl_methods(inner, leaf, parent, kind, found);
                    }
                }
                syn::Item::Impl(item_impl) => {
                    let owner = match &*item_impl.self_ty {
                        syn::Type::Path(path) => path
                            .path
                            .segments
                            .last()
                            .map(|segment| segment.ident.to_string()),
                        _ => None,
                    };
                    for impl_item in &item_impl.items {
                        match impl_item {
                            syn::ImplItem::Fn(method) if method.sig.ident == leaf => {
                                let visible = decl_cross_crate_visible(&method.vis, &method.attrs);
                                if owner.as_deref() == Some(parent) {
                                    // The row's own type names the declaration.
                                    *found = Some(visible);
                                } else if found.is_none() {
                                    *found = Some(visible);
                                } else if visible && *found == Some(false) {
                                    *found = Some(true);
                                }
                            }
                            syn::ImplItem::Const(constant)
                                if kind == "const" && constant.ident == leaf =>
                            {
                                let visible =
                                    decl_cross_crate_visible(&constant.vis, &constant.attrs);
                                *found = Some(found.unwrap_or(false) || visible);
                            }
                            _ => {}
                        }
                    }
                }
                syn::Item::Trait(item_trait) => {
                    // A trait method has no visibility of its own: it is part
                    // of the trait's surface, and the trait declaration is the
                    // item a `trait` row checks.
                    for trait_item in &item_trait.items {
                        if let syn::TraitItem::Fn(method) = trait_item {
                            if method.sig.ident == leaf {
                                *found = Some(true);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn visit_items(
        items: &[syn::Item],
        leaf: &str,
        parent: &str,
        kind: &str,
        found: &mut Option<bool>,
    ) {
        for item in items {
            match item {
                syn::Item::Mod(module) => {
                    if module.ident == leaf && kind == "mod" {
                        *found = Some(
                            found.unwrap_or(false)
                                || decl_cross_crate_visible(&module.vis, &module.attrs),
                        );
                    }
                    if let Some((_, inner)) = &module.content {
                        visit_items(inner, leaf, parent, kind, found);
                    }
                }
                syn::Item::Fn(function) if kind.ends_with("fn") && kind != "method" => {
                    if function.sig.ident == leaf {
                        *found = Some(
                            found.unwrap_or(false)
                                || decl_cross_crate_visible(&function.vis, &function.attrs),
                        );
                    }
                }
                syn::Item::Struct(data) if kind == "struct" && data.ident == leaf => {
                    *found = Some(
                        found.unwrap_or(false) || decl_cross_crate_visible(&data.vis, &data.attrs),
                    );
                }
                syn::Item::Enum(data) if kind == "enum" && data.ident == leaf => {
                    *found = Some(
                        found.unwrap_or(false) || decl_cross_crate_visible(&data.vis, &data.attrs),
                    );
                }
                syn::Item::Union(data) if kind == "union" && data.ident == leaf => {
                    *found = Some(
                        found.unwrap_or(false) || decl_cross_crate_visible(&data.vis, &data.attrs),
                    );
                }
                syn::Item::Trait(data) if kind == "trait" && data.ident == leaf => {
                    *found = Some(
                        found.unwrap_or(false) || decl_cross_crate_visible(&data.vis, &data.attrs),
                    );
                }
                syn::Item::Type(data) if kind == "type" && data.ident == leaf => {
                    *found = Some(
                        found.unwrap_or(false) || decl_cross_crate_visible(&data.vis, &data.attrs),
                    );
                }
                syn::Item::Const(data) if kind == "const" && data.ident == leaf => {
                    *found = Some(
                        found.unwrap_or(false) || decl_cross_crate_visible(&data.vis, &data.attrs),
                    );
                }
                syn::Item::Static(data) if kind == "static" && data.ident == leaf => {
                    *found = Some(
                        found.unwrap_or(false) || decl_cross_crate_visible(&data.vis, &data.attrs),
                    );
                }
                syn::Item::Use(item_use) if visit_use_tree(&item_use.tree, leaf) => {
                    *found = Some(
                        found.unwrap_or(false)
                            || decl_cross_crate_visible(&item_use.vis, &item_use.attrs),
                    );
                }
                syn::Item::Macro(item_macro) if matches!(kind, "static" | "use" | "type") => {
                    let mut tokens = Vec::new();
                    token_texts(item_macro.mac.tokens.clone(), &mut tokens);
                    if macro_body_decl_visible(&tokens, leaf, kind) {
                        *found = Some(true);
                    }
                }
                syn::Item::Macro(item_macro)
                    if kind == "macro"
                        && item_macro.ident.as_ref().is_some_and(|ident| ident == leaf) =>
                {
                    let exported = item_macro
                        .attrs
                        .iter()
                        .any(|attr| attr.path().is_ident("macro_export"));
                    *found = Some(found.unwrap_or(false) || exported);
                }
                _ => {}
            }
        }
    }

    let mut found = None;
    if kind == "field" {
        fn visit_field_structs(
            items: &[syn::Item],
            parent: &str,
            leaf: &str,
            found: &mut Option<bool>,
        ) {
            for item in items {
                match item {
                    syn::Item::Mod(module) => {
                        if let Some((_, inner)) = &module.content {
                            visit_field_structs(inner, parent, leaf, found);
                        }
                    }
                    syn::Item::Struct(data) if data.ident == parent => {
                        if let Some(visible) = fields_visible_in_item(item, leaf) {
                            *found = Some(found.unwrap_or(false) || visible);
                        }
                    }
                    syn::Item::Union(data) if data.ident == parent => {
                        if let Some(visible) = fields_visible_in_item(item, leaf) {
                            *found = Some(found.unwrap_or(false) || visible);
                        }
                    }
                    _ => {}
                }
            }
        }
        visit_field_structs(&parsed.items, parent, leaf, &mut found);
    } else if kind == "method" {
        visit_impl_methods(&parsed.items, leaf, parent, kind, &mut found);
    } else if kind == "const" {
        // An associated const may be reached through its impl block, not only
        // as a top-level item; both visitors must run for kind const.
        visit_items(&parsed.items, leaf, parent, kind, &mut found);
        visit_impl_methods(&parsed.items, leaf, parent, kind, &mut found);
    } else {
        visit_items(&parsed.items, leaf, parent, kind, &mut found);
    }
    Ok(found)
}

fn vis_row_visibility_violation(row: &VisRow, source: &str) -> Result<(), String> {
    // A self-file module row names its own source file (`<leaf>.rs` or
    // `<leaf>/mod.rs`); the module's visibility lives in its parent
    // declaration, not in this file's items.
    if row.kind == "mod" {
        let leaf = row.item.rsplit("::").next().unwrap_or(&row.item);
        if row.file.ends_with(&format!("/{leaf}.rs"))
            || row.file.ends_with(&format!("/{leaf}/mod.rs"))
        {
            return Ok(());
        }
    }
    let what = format!("{} {} {}", row.crate_dir, row.kind, row.item);
    match visible_decl_in_source(source, &row.item, &row.kind)? {
        Some(false) => Err(format!(
            "{what}: the defining file declares this item without a cross-crate route in the test-support build (no plain pub, no visibility::make(pub), no cfg-split pub arm); remove the VIS row or widen the item through the signed route (peer #981 R2)"
        )),
        _ => Ok(()),
    }
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
    check_visibility_coverage(workspace, &all_rows)?;
    for row in &all_rows {
        if !VIS_ROW_VISIBILITY_CRATES.contains(&row.crate_dir.as_str()) {
            continue;
        }
        let source = read(&workspace.join(&row.file))?;
        vis_row_visibility_violation(row, &source)?;
    }
    let auth = auth_rows(workspace);
    let vis_entries: Vec<VisEntry> = all_rows
        .iter()
        .map(|row| VisEntry {
            crate_name: row.crate_dir.clone(),
            item: row.item.clone(),
            file: row.file.clone(),
        })
        .collect();
    vis_authority_disjoint_violation(workspace, &vis_entries, &auth)?;
    let (cfg_pub_allowlist, cfg_pub_ceiling) = load_authority_cfg_pub_allowlist(workspace)?;
    authority_cfg_pub_violation(workspace, &auth, &cfg_pub_allowlist, cfg_pub_ceiling)?;
    duplicated_test_support_widening(workspace)?;
    vis_free_fn_reset_violation(workspace, &all_rows)?;
    test_support_reverse_coverage_violation(workspace, &all_rows, &auth)?;
    unit_runtime_absent_layer_violation(workspace)?;
    if all_rows.iter().any(|row| row.kind == "mod") {
        expanded_mod_rows_violation(workspace, &all_rows, &auth_rows(workspace))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// LEAD 10:1xZ / 10:3xZ: VIS ∩ AUTHORITY disjointness and the cfg-gated
// plain-pub AUTHORITY ratchet.
//
// Disjointness: an item that has an AUTHORITY row must have exactly ONE
// cross-crate route. A TEST-SUPPORT-VIS row (or an A-prime widening) for the
// same item is therefore red. Rows carry two conventions: the original path
// (`tagged::x::dup`) or the fence spelling
// (`test_support::authority::dup`). The original is resolved through the
// fence body (the delegate's forwarded call / alias target), so both
// conventions land on the same identity.
//
// Ratchet: every original-path AUTHORITY row is classified as
//   (a) production-public (plain `pub` without a test-support cfg), which
//       stays untouched; or
//   (b) a cfg(any(test, feature = "test-support")) item spelled plain `pub`,
//       which exists only as cross-crate test-support API.
// The (b) set is frozen in AUTHORITY-CFG-PUB-ALLOWLIST.tsv with an exact
// ceiling; it may only shrink. New (b) items are red.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct VisEntry {
    crate_name: String,
    item: String,
    file: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CfgPubRow {
    crate_name: String,
    item: String,
    defining_file: String,
}

fn authority_is_fence_delegate(row: &AuthRow) -> bool {
    row.fence_path.contains("::test_support::authority")
}

fn syn_path_text(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// The first call target / alias target in a delegate body, used to resolve
/// the original item behind a fence spelling.
fn syn_first_forward(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Call(call) => match call.func.as_ref() {
            syn::Expr::Path(path) => Some(syn_path_text(&path.path)),
            _ => None,
        },
        syn::Expr::MethodCall(call) => syn_first_forward(&call.receiver),
        syn::Expr::Paren(paren) => syn_first_forward(&paren.expr),
        syn::Expr::Block(block) => block.block.stmts.iter().find_map(|stmt| match stmt {
            syn::Stmt::Expr(expr, _) => syn_first_forward(expr),
            syn::Stmt::Local(local) => local
                .init
                .as_ref()
                .and_then(|init| syn_first_forward(&init.expr)),
            _ => None,
        }),
        syn::Expr::Reference(reference) => syn_first_forward(&reference.expr),
        _ => None,
    }
}

fn syn_fn_forward(item: &syn::ItemFn) -> Option<String> {
    item.block.stmts.iter().find_map(|stmt| match stmt {
        syn::Stmt::Expr(expr, _) => syn_first_forward(expr),
        _ => None,
    })
}

fn syn_type_forward(item: &syn::ItemType) -> Option<String> {
    match item.ty.as_ref() {
        syn::Type::Path(path) => Some(syn_path_text(&path.path)),
        _ => None,
    }
}

/// Resolve an AUTHORITY row to its crate-relative original item path.
/// `None` means the fence body could not be resolved; the disjointness arm is
/// then fail-closed and matches by crate + leaf.
fn resolve_authority_original(workspace: &Path, row: &AuthRow) -> Option<String> {
    if !authority_is_fence_delegate(row) {
        return Some(row.item.clone());
    }
    let AuthFence::Module { file, .. } = auth_fence_file(&row.crate_name)? else {
        return Some(row.item.clone());
    };
    if row.defining_file != file {
        // Original-path convention: the row names the defining file and may
        // spell a type-relative method (`Type::method`) while fence_path
        // spells the delegate. The item path is the original identity.
        return Some(row.item.clone());
    }
    let source = std::fs::read_to_string(workspace.join(file)).ok()?;
    let parsed = syn::parse_file(&source).ok()?;
    let leaf = auth_leaf(&row.item);
    fn walk_items(items: &[syn::Item], leaf: &str) -> Option<String> {
        for item in items {
            match item {
                syn::Item::Fn(item_fn) if item_fn.sig.ident == leaf => {
                    if let Some(path) = syn_fn_forward(item_fn) {
                        return Some(path);
                    }
                }
                syn::Item::Type(item_type) if item_type.ident == leaf => {
                    if let Some(path) = syn_type_forward(item_type) {
                        return Some(path);
                    }
                }
                syn::Item::Mod(item_mod) => {
                    if let Some((_, items)) = &item_mod.content {
                        if let Some(path) = walk_items(items, leaf) {
                            return Some(path);
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk_items(&parsed.items, leaf)
}

fn path_last_two(path: &str) -> String {
    let segments: Vec<&str> = path
        .split("::")
        .filter(|segment| !segment.is_empty())
        .collect();
    let start = segments.len().saturating_sub(2);
    segments[start..].join("::")
}

/// LEAD 11:2xZ disjointness key: (crate, OWNER TYPE for methods/fields, leaf).
///
/// A bare leaf is insufficient: two owners can define the same method or field
/// name in one file (the historical `ProofEra::registry` vs
/// `InFlightRootPin::registry` pair). `Named` owners must agree; a free
/// function (owner `Free`) can never be the same item as a method or field.
/// When the owner cannot be resolved (macro-generated or unparseable source)
/// the arm keeps the previous file/path fallback, fail-closed.
#[derive(Clone, Debug, PartialEq, Eq)]
enum OwnerKey {
    Free,
    Named(String),
    Unknown,
}

fn item_owner_from_path(item: &str) -> Option<String> {
    let segments: Vec<&str> = item.split("::").filter(|s| !s.is_empty()).collect();
    if segments.len() < 2 {
        return None;
    }
    let candidate = segments[segments.len() - 2];
    let starts_upper = candidate
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase());
    starts_upper.then(|| candidate.to_owned())
}

/// Resolve the owner type of `leaf` in `file`: a method/free-fn/field owner.
fn item_owner_from_file(workspace: &Path, file: &str, leaf: &str) -> OwnerKey {
    let Ok(source) = std::fs::read_to_string(workspace.join(file)) else {
        return OwnerKey::Unknown;
    };
    let Ok(parsed) = syn::parse_file(&source) else {
        return OwnerKey::Unknown;
    };
    let mut free = false;
    let mut owners = BTreeSet::new();
    fn walk(items: &[syn::Item], leaf: &str, free: &mut bool, owners: &mut BTreeSet<String>) {
        for item in items {
            match item {
                syn::Item::Fn(item_fn) if item_fn.sig.ident == leaf => *free = true,
                syn::Item::Impl(item_impl) => {
                    let owner = match item_impl.self_ty.as_ref() {
                        syn::Type::Path(path) => path
                            .path
                            .segments
                            .last()
                            .map(|segment| segment.ident.to_string()),
                        _ => None,
                    };
                    for impl_item in &item_impl.items {
                        if let syn::ImplItem::Fn(impl_fn) = impl_item {
                            if impl_fn.sig.ident == leaf {
                                if let Some(owner) = &owner {
                                    owners.insert(owner.clone());
                                }
                            }
                        }
                    }
                }
                syn::Item::Struct(item_struct) => {
                    for field in &item_struct.fields {
                        if field.ident.as_ref().is_some_and(|ident| ident == leaf) {
                            owners.insert(item_struct.ident.to_string());
                        }
                    }
                }
                syn::Item::Mod(item_mod) => {
                    if let Some((_, inner)) = &item_mod.content {
                        walk(inner, leaf, free, owners);
                    }
                }
                _ => {}
            }
        }
    }
    walk(&parsed.items, leaf, &mut free, &mut owners);
    if free {
        return OwnerKey::Free;
    }
    if owners.len() == 1 {
        return OwnerKey::Named(owners.into_iter().next().expect("one owner"));
    }
    OwnerKey::Unknown
}

fn item_owner(workspace: &Path, item: &str, file: &str, leaf: &str) -> OwnerKey {
    if let Some(owner) = item_owner_from_path(item) {
        return OwnerKey::Named(owner);
    }
    item_owner_from_file(workspace, file, leaf)
}

fn owners_are_distinct(auth: &OwnerKey, vis: &OwnerKey) -> bool {
    match (auth, vis) {
        (OwnerKey::Named(a), OwnerKey::Named(b)) => a != b,
        (OwnerKey::Free, OwnerKey::Named(_)) | (OwnerKey::Named(_), OwnerKey::Free) => true,
        _ => false,
    }
}

fn authority_vis_pair(
    workspace: &Path,
    row: &AuthRow,
    original: Option<&str>,
    vis: &VisEntry,
) -> bool {
    if row.crate_name != vis.crate_name {
        return false;
    }
    let auth_leaf = auth_leaf(&row.item);
    let vis_leaf = vis.item.rsplit("::").next().unwrap_or(&vis.item);
    if auth_leaf != vis_leaf {
        return false;
    }
    let auth_item = original.unwrap_or(&row.item);
    let auth_owner = item_owner(workspace, auth_item, &row.defining_file, auth_leaf);
    let vis_owner = item_owner(workspace, &vis.item, &vis.file, vis_leaf);
    if owners_are_distinct(&auth_owner, &vis_owner) {
        return false;
    }
    if row.defining_file == vis.file {
        return true;
    }
    match original {
        Some(original) => {
            let original = original.strip_prefix("crate::").unwrap_or(original);
            let vis_item = vis.item.strip_prefix("crate::").unwrap_or(&vis.item);
            original == vis_item || path_last_two(original) == path_last_two(vis_item)
        }
        // Unresolvable fence body: fail closed on crate + leaf (+owner key).
        None => true,
    }
}

fn vis_authority_disjoint_violation(
    workspace: &Path,
    vis_entries: &[VisEntry],
    auth: &[AuthRow],
) -> Result<(), String> {
    let originals: Vec<Option<String>> = auth
        .iter()
        .map(|row| resolve_authority_original(workspace, row))
        .collect();
    for (row, original) in auth.iter().zip(originals.iter()) {
        if let Some(vis) = vis_entries
            .iter()
            .find(|vis| authority_vis_pair(workspace, row, original.as_deref(), vis))
        {
            return Err(format!(
                "{} `{}` has an AUTHORITY row ({}) and a TEST-SUPPORT-VIS row ({} {}): one item must have exactly one cross-crate route",
                row.crate_name, row.item, row.fence_path, vis.file, vis.item
            ));
        }
    }
    // A lone A-prime on an AUTHORITY item is the signed WorldSlot pattern
    // (AUTHORITY-ITEMS rows 64 SubrEntry and 224 set_tagged_heap): the item
    // lives in a crate-private module, so the attribute creates no public
    // reach; `authority_items_have_exactly_the_fence_public_path` proves the
    // fence is the only public route. The disjointness failure is a SECOND
    // route: a TEST-SUPPORT-VIS row (or widened field) naming the same item.
    Ok(())
}

fn cfg_attribute_mentions_test_support(attrs: &[syn::Attribute]) -> bool {
    fn tokens_contain_test_support(stream: proc_macro2::TokenStream) -> bool {
        stream.into_iter().any(|token| match token {
            proc_macro2::TokenTree::Ident(ident) => ident.to_string().contains("test-support"),
            proc_macro2::TokenTree::Literal(literal) => {
                literal.to_string().contains("test-support")
            }
            proc_macro2::TokenTree::Group(group) => tokens_contain_test_support(group.stream()),
            proc_macro2::TokenTree::Punct(_) => false,
        })
    }
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && match &attr.meta {
                syn::Meta::List(list) => tokens_contain_test_support(list.tokens.clone()),
                _ => false,
            }
    })
}

fn plain_pub(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

fn syn_item_is_cfg_pub(vis: &syn::Visibility, attrs: &[syn::Attribute]) -> bool {
    plain_pub(vis) && cfg_attribute_mentions_test_support(attrs)
}

/// Locate an original-path AUTHORITY item and classify it as the cfg-gated
/// plain-pub form (b) or not. `Err` means the item could not be located:
/// fail-closed, because an unclassifiable row would let (b) items grow.
fn classify_original_path_item(source: &str, item_path: &str) -> Result<bool, String> {
    let segments: Vec<&str> = item_path.split("::").filter(|s| !s.is_empty()).collect();
    let leaf = *segments
        .last()
        .ok_or_else(|| format!("empty AUTHORITY item path `{item_path}`"))?;
    let owner = if segments.len() >= 2 {
        Some(segments[segments.len() - 2])
    } else {
        None
    };
    fn walk_items(items: &[syn::Item], leaf: &str, owner: Option<&str>) -> Option<bool> {
        for item in items {
            match item {
                syn::Item::Fn(item_fn) if item_fn.sig.ident == leaf => {
                    return Some(syn_item_is_cfg_pub(&item_fn.vis, &item_fn.attrs));
                }
                syn::Item::Const(item_const) if item_const.ident == leaf => {
                    return Some(syn_item_is_cfg_pub(&item_const.vis, &item_const.attrs));
                }
                syn::Item::Static(item_static) if item_static.ident == leaf => {
                    return Some(syn_item_is_cfg_pub(&item_static.vis, &item_static.attrs));
                }
                syn::Item::Type(item_type) if item_type.ident == leaf => {
                    return Some(syn_item_is_cfg_pub(&item_type.vis, &item_type.attrs));
                }
                syn::Item::Trait(item_trait) if item_trait.ident == leaf => {
                    return Some(syn_item_is_cfg_pub(&item_trait.vis, &item_trait.attrs));
                }
                syn::Item::Struct(item_struct) if item_struct.ident == leaf => {
                    return Some(syn_item_is_cfg_pub(&item_struct.vis, &item_struct.attrs));
                }
                syn::Item::Enum(item_enum) if item_enum.ident == leaf => {
                    return Some(syn_item_is_cfg_pub(&item_enum.vis, &item_enum.attrs));
                }
                syn::Item::Union(item_union) if item_union.ident == leaf => {
                    return Some(syn_item_is_cfg_pub(&item_union.vis, &item_union.attrs));
                }
                syn::Item::Struct(item_struct)
                    if owner == Some(item_struct.ident.to_string().as_str()) =>
                {
                    for field in &item_struct.fields {
                        if let Some(ident) = &field.ident {
                            if ident == leaf {
                                return Some(syn_item_is_cfg_pub(&field.vis, &field.attrs));
                            }
                        }
                    }
                }
                syn::Item::Impl(item_impl) => {
                    if let Some(owner) = owner {
                        let self_leaf = match item_impl.self_ty.as_ref() {
                            syn::Type::Path(path) => path
                                .path
                                .segments
                                .last()
                                .map(|segment| segment.ident.to_string()),
                            _ => None,
                        };
                        if self_leaf.as_deref() == Some(owner) {
                            for impl_item in &item_impl.items {
                                if let syn::ImplItem::Fn(impl_fn) = impl_item {
                                    if impl_fn.sig.ident == leaf {
                                        return Some(syn_item_is_cfg_pub(
                                            &impl_fn.vis,
                                            &impl_fn.attrs,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                syn::Item::Mod(item_mod) => {
                    if let Some((_, items)) = &item_mod.content {
                        if let Some(found) = walk_items(items, leaf, owner) {
                            return Some(found);
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk_items(
        &syn::parse_file(source)
            .map_err(|error| format!("parse: {error}"))?
            .items,
        leaf,
        owner,
    )
    .ok_or_else(|| format!("AUTHORITY item `{item_path}` not found in its defining file"))
}

fn authority_cfg_pub_sites(
    workspace: &Path,
    auth: &[AuthRow],
) -> Result<BTreeSet<CfgPubRow>, String> {
    let mut sites = BTreeSet::new();
    for row in auth {
        if authority_is_fence_delegate(row) {
            continue;
        }
        let path = workspace.join(&row.defining_file);
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if classify_original_path_item(&source, &row.item)? {
            sites.insert(CfgPubRow {
                crate_name: row.crate_name.clone(),
                item: row.item.clone(),
                defining_file: row.defining_file.clone(),
            });
        }
    }
    Ok(sites)
}

fn load_authority_cfg_pub_allowlist(
    workspace: &Path,
) -> Result<(BTreeSet<CfgPubRow>, usize), String> {
    let architecture = workspace.join(ARCHITECTURE_DIR);
    let file = architecture.join("AUTHORITY-CFG-PUB-ALLOWLIST.tsv");
    let ceiling_file = architecture.join("AUTHORITY-CFG-PUB-ALLOWLIST.ceiling");
    let mut allowlist = BTreeSet::new();
    if file.is_file() {
        for (index, line) in read(&file)?.lines().enumerate().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() != 3 {
                return Err(format!(
                    "{}:{}: expected 3 columns (crate, item, defining_file)",
                    file.display(),
                    index + 1
                ));
            }
            allowlist.insert(CfgPubRow {
                crate_name: fields[0].to_owned(),
                item: fields[1].to_owned(),
                defining_file: fields[2].to_owned(),
            });
        }
    }
    let ceiling: usize = read(&ceiling_file)?.trim().parse().map_err(|_| {
        format!(
            "{}: expected one nonnegative integer",
            ceiling_file.display()
        )
    })?;
    if allowlist.len() > ceiling {
        return Err(format!(
            "{}: {} allowlist rows exceed the ceiling {ceiling}",
            file.display(),
            allowlist.len()
        ));
    }
    Ok((allowlist, ceiling))
}

fn authority_cfg_pub_violation(
    workspace: &Path,
    auth: &[AuthRow],
    allowlist: &BTreeSet<CfgPubRow>,
    ceiling: usize,
) -> Result<(), String> {
    let computed = authority_cfg_pub_sites(workspace, auth)?;
    for row in computed.difference(allowlist) {
        return Err(format!(
            "{} `{}` ({}) is a NEW cfg-gated plain-pub AUTHORITY item; the frozen allowlist may only shrink",
            row.crate_name, row.item, row.defining_file
        ));
    }
    for row in allowlist.difference(&computed) {
        return Err(format!(
            "{} `{}` ({}) no longer classifies as a cfg-gated plain-pub AUTHORITY item; shrink AUTHORITY-CFG-PUB-ALLOWLIST.tsv",
            row.crate_name, row.item, row.defining_file
        ));
    }
    if allowlist.len() > ceiling {
        return Err(format!(
            "AUTHORITY-CFG-PUB-ALLOWLIST.ceiling {ceiling}: allowlist has {} rows",
            allowlist.len()
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Duplicated test-support widening attributes (LEAD 10:5xZ item 1).
//
// One item must carry at most one visibility::make(pub) cfg_attr. Two
// attributes on the same item are a codemod artifact, not a stronger
// widening, and the one-row-per-item VIS ledger cannot see the duplicate. The
// arm counts per ITEM (attributes may be separated by doc/inline attributes),
// not per source line.
// ---------------------------------------------------------------------------

fn attr_is_make_pub_widening(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("cfg_attr") {
        return false;
    }
    fn flatten(stream: proc_macro2::TokenStream, idents: &mut Vec<String>) {
        for token in stream {
            match token {
                proc_macro2::TokenTree::Ident(ident) => idents.push(ident.to_string()),
                proc_macro2::TokenTree::Group(group) => flatten(group.stream(), idents),
                _ => {}
            }
        }
    }
    let syn::Meta::List(list) = &attr.meta else {
        return false;
    };
    let mut idents = Vec::new();
    flatten(list.tokens.clone(), &mut idents);
    idents
        .windows(3)
        .any(|window| window == ["visibility", "make", "pub"])
}

fn check_duplicate_widening_attrs(attrs: &[syn::Attribute], label: &str, out: &mut Vec<String>) {
    let count = attrs
        .iter()
        .filter(|attr| attr_is_make_pub_widening(attr))
        .count();
    if count > 1 {
        out.push(format!("{label}: {count} duplicated make(pub) attributes"));
    }
}

fn scan_items_for_duplicate_widening(items: &[syn::Item], file: &Path, out: &mut Vec<String>) {
    fn fields(fields: &syn::Fields, owner: &str, file: &Path, out: &mut Vec<String>) {
        for (index, field) in fields.iter().enumerate() {
            let name = field
                .ident
                .as_ref()
                .map(|ident| ident.to_string())
                .unwrap_or_else(|| format!("#{index}"));
            check_duplicate_widening_attrs(
                &field.attrs,
                &format!("{}: {owner}::{name}", file.display()),
                out,
            );
        }
    }
    fn visit(items: &[syn::Item], file: &Path, out: &mut Vec<String>) {
        for item in items {
            match item {
                syn::Item::Mod(item_mod) => {
                    check_duplicate_widening_attrs(
                        &item_mod.attrs,
                        &format!("{}: mod {}", file.display(), item_mod.ident),
                        out,
                    );
                    if let Some((_, inner)) = &item_mod.content {
                        visit(inner, file, out);
                    }
                }
                syn::Item::Struct(item_struct) => {
                    check_duplicate_widening_attrs(
                        &item_struct.attrs,
                        &format!("{}: struct {}", file.display(), item_struct.ident),
                        out,
                    );
                    fields(
                        &item_struct.fields,
                        &item_struct.ident.to_string(),
                        file,
                        out,
                    );
                }
                syn::Item::Enum(item_enum) => {
                    check_duplicate_widening_attrs(
                        &item_enum.attrs,
                        &format!("{}: enum {}", file.display(), item_enum.ident),
                        out,
                    );
                    for variant in &item_enum.variants {
                        check_duplicate_widening_attrs(
                            &variant.attrs,
                            &format!("{}: {}::{}", file.display(), item_enum.ident, variant.ident),
                            out,
                        );
                        fields(
                            &variant.fields,
                            &format!("{}::{}", item_enum.ident, variant.ident),
                            file,
                            out,
                        );
                    }
                }
                syn::Item::Union(item_union) => {
                    check_duplicate_widening_attrs(
                        &item_union.attrs,
                        &format!("{}: union {}", file.display(), item_union.ident),
                        out,
                    );
                    fields(
                        &syn::Fields::Named(item_union.fields.clone()),
                        &item_union.ident.to_string(),
                        file,
                        out,
                    );
                }
                syn::Item::Impl(item_impl) => {
                    let owner = match item_impl.self_ty.as_ref() {
                        syn::Type::Path(path) => path
                            .path
                            .segments
                            .last()
                            .map(|segment| segment.ident.to_string())
                            .unwrap_or_else(|| "_".to_owned()),
                        _ => "_".to_owned(),
                    };
                    check_duplicate_widening_attrs(
                        &item_impl.attrs,
                        &format!("{}: impl {owner}", file.display()),
                        out,
                    );
                    for impl_item in &item_impl.items {
                        match impl_item {
                            syn::ImplItem::Fn(impl_fn) => check_duplicate_widening_attrs(
                                &impl_fn.attrs,
                                &format!("{}: {owner}::{}", file.display(), impl_fn.sig.ident),
                                out,
                            ),
                            syn::ImplItem::Const(item_const) => check_duplicate_widening_attrs(
                                &item_const.attrs,
                                &format!("{}: {owner}::{}", file.display(), item_const.ident),
                                out,
                            ),
                            syn::ImplItem::Type(item_type) => check_duplicate_widening_attrs(
                                &item_type.attrs,
                                &format!("{}: {owner}::{}", file.display(), item_type.ident),
                                out,
                            ),
                            _ => {}
                        }
                    }
                }
                syn::Item::Trait(item_trait) => {
                    check_duplicate_widening_attrs(
                        &item_trait.attrs,
                        &format!("{}: trait {}", file.display(), item_trait.ident),
                        out,
                    );
                    for trait_item in &item_trait.items {
                        match trait_item {
                            syn::TraitItem::Fn(trait_fn) => check_duplicate_widening_attrs(
                                &trait_fn.attrs,
                                &format!(
                                    "{}: {}::{}",
                                    file.display(),
                                    item_trait.ident,
                                    trait_fn.sig.ident
                                ),
                                out,
                            ),
                            syn::TraitItem::Const(item_const) => check_duplicate_widening_attrs(
                                &item_const.attrs,
                                &format!(
                                    "{}: {}::{}",
                                    file.display(),
                                    item_trait.ident,
                                    item_const.ident
                                ),
                                out,
                            ),
                            syn::TraitItem::Type(item_type) => check_duplicate_widening_attrs(
                                &item_type.attrs,
                                &format!(
                                    "{}: {}::{}",
                                    file.display(),
                                    item_trait.ident,
                                    item_type.ident
                                ),
                                out,
                            ),
                            _ => {}
                        }
                    }
                }
                syn::Item::Fn(item_fn) => check_duplicate_widening_attrs(
                    &item_fn.attrs,
                    &format!("{}: fn {}", file.display(), item_fn.sig.ident),
                    out,
                ),
                syn::Item::Const(item_const) => check_duplicate_widening_attrs(
                    &item_const.attrs,
                    &format!("{}: const {}", file.display(), item_const.ident),
                    out,
                ),
                syn::Item::Static(item_static) => check_duplicate_widening_attrs(
                    &item_static.attrs,
                    &format!("{}: static {}", file.display(), item_static.ident),
                    out,
                ),
                syn::Item::Type(item_type) => check_duplicate_widening_attrs(
                    &item_type.attrs,
                    &format!("{}: type {}", file.display(), item_type.ident),
                    out,
                ),
                syn::Item::Use(item_use) => check_duplicate_widening_attrs(
                    &item_use.attrs,
                    &format!("{}: use", file.display()),
                    out,
                ),
                syn::Item::TraitAlias(item_trait_alias) => check_duplicate_widening_attrs(
                    &item_trait_alias.attrs,
                    &format!("{}: trait alias {}", file.display(), item_trait_alias.ident),
                    out,
                ),
                syn::Item::ExternCrate(item_extern) => check_duplicate_widening_attrs(
                    &item_extern.attrs,
                    &format!("{}: extern crate {}", file.display(), item_extern.ident),
                    out,
                ),
                syn::Item::ForeignMod(foreign) => {
                    check_duplicate_widening_attrs(
                        &foreign.attrs,
                        &format!("{}: foreign mod", file.display()),
                        out,
                    );
                    for foreign_item in &foreign.items {
                        if let syn::ForeignItem::Fn(foreign_fn) = foreign_item {
                            check_duplicate_widening_attrs(
                                &foreign_fn.attrs,
                                &format!("{}: extern fn {}", file.display(), foreign_fn.sig.ident),
                                out,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
    visit(items, file, out);
}

fn duplicated_test_support_widening(workspace: &Path) -> Result<(), String> {
    let mut directories = vec![workspace.join("crates")];
    let mut violations = Vec::new();
    while let Some(directory) = directories.pop() {
        if !directory.is_dir() {
            continue;
        }
        for path in read_dir_sorted(&directory)? {
            if path.is_dir() {
                directories.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            let source = read(&path)?;
            if !source.contains("visibility::make(pub)") {
                continue;
            }
            // Compile-fail fixtures may be invalid Rust on purpose; they never
            // carry the widening attribute, so skipping them is not a
            // coverage hole for this arm.
            if let Ok(parsed) = syn::parse_file(&source) {
                scan_items_for_duplicate_widening(&parsed.items, &path, &mut violations);
            }
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "TEST-SUPPORT-VIS: duplicated visibility::make(pub) attribute(s) on one item; each item needs exactly one (LEAD 10:5xZ): {}",
            violations.join(" | ")
        ))
    }
}

// ---------------------------------------------------------------------------
// Free-function TLS/static counter resets are AUTHORITY-class (LEAD 11:2xZ
// set 2). LEAD 18:1xZ retired the frozen allowlist: every historical case is
// now an owning-crate test_support::authority fence delegate with an
// AUTHORITY row, so ANY VIS row in this class is red. Owned-instance resets
// (methods on a type the test owns, and free functions that only touch an
// owned argument) remain VIS. A VIS row for a free-function reset/store of
// process or thread static state must route the write through the owning
// crate's test_support::authority fence with an AUTHORITY row instead.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FreeFnResetRow {
    crate_name: String,
    item: String,
    defining_file: String,
}

fn static_like_ident(ident: &str) -> bool {
    let mut chars = ident.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && ident
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn static_write_method(method: &str) -> bool {
    matches!(
        method,
        "set"
            | "store"
            | "swap"
            | "replace"
            | "take"
            | "fetch_add"
            | "fetch_sub"
            | "fetch_and"
            | "fetch_or"
            | "fetch_xor"
    )
}

fn expr_tail_ident(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        syn::Expr::Paren(paren) => expr_tail_ident(&paren.expr),
        syn::Expr::Reference(reference) => expr_tail_ident(&reference.expr),
        _ => None,
    }
}

fn expr_contains_write(expr: &syn::Expr) -> bool {
    struct Finder {
        found: bool,
    }
    impl<'ast> syn::visit::Visit<'ast> for Finder {
        fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
            let method = call.method.to_string();
            if static_write_method(&method) || method == "borrow_mut" {
                self.found = true;
            }
            syn::visit::visit_expr_method_call(self, call);
        }
    }
    let mut finder = Finder { found: false };
    syn::visit::Visit::visit_expr(&mut finder, expr);
    finder.found
}

#[derive(Default)]
struct StaticUse {
    any_access: bool,
    direct_write: bool,
    closure_write: bool,
}

impl<'ast> syn::visit::Visit<'ast> for StaticUse {
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if expr_tail_ident(&call.receiver).is_some_and(|ident| static_like_ident(&ident)) {
            self.any_access = true;
            let method = call.method.to_string();
            if static_write_method(&method) {
                self.direct_write = true;
            }
            if method == "with" && call.args.iter().any(expr_contains_write) {
                self.closure_write = true;
            }
        }
        syn::visit::visit_expr_method_call(self, call);
    }
}

enum FnSite {
    Free(Box<syn::ItemFn>),
    Method,
}

fn find_free_fn_site(source: &str, leaf: &str) -> Option<FnSite> {
    let parsed = syn::parse_file(source).ok()?;
    let mut free: Option<Box<syn::ItemFn>> = None;
    let mut methods = 0usize;
    fn walk(
        items: &[syn::Item],
        leaf: &str,
        free: &mut Option<Box<syn::ItemFn>>,
        methods: &mut usize,
    ) {
        for item in items {
            match item {
                syn::Item::Fn(item_fn) if item_fn.sig.ident == leaf => {
                    if free.is_none() {
                        *free = Some(Box::new(item_fn.clone()));
                    }
                }
                syn::Item::Impl(item_impl) => {
                    for impl_item in &item_impl.items {
                        if let syn::ImplItem::Fn(impl_fn) = impl_item {
                            if impl_fn.sig.ident == leaf {
                                *methods += 1;
                            }
                        }
                    }
                }
                syn::Item::Trait(item_trait) => {
                    for trait_item in &item_trait.items {
                        if let syn::TraitItem::Fn(trait_fn) = trait_item {
                            if trait_fn.sig.ident == leaf {
                                *methods += 1;
                            }
                        }
                    }
                }
                syn::Item::Mod(item_mod) => {
                    if let Some((_, inner)) = &item_mod.content {
                        walk(inner, leaf, free, methods);
                    }
                }
                _ => {}
            }
        }
    }
    walk(&parsed.items, leaf, &mut free, &mut methods);
    if let Some(free) = free {
        Some(FnSite::Free(free))
    } else if methods > 0 {
        Some(FnSite::Method)
    } else {
        None
    }
}

fn free_fn_reset_candidates(
    workspace: &Path,
    rows: &[VisRow],
) -> Result<BTreeSet<FreeFnResetRow>, String> {
    let mut candidates = BTreeSet::new();
    for row in rows {
        if row.kind != "fn" {
            continue;
        }
        let leaf = row.item.rsplit("::").next().unwrap_or(&row.item);
        let reset_named = leaf.starts_with("reset_") || leaf.starts_with("clear_");
        let reset_or_store_named =
            reset_named || leaf.starts_with("store_") || leaf.starts_with("set_");
        let path = workspace.join(&row.file);
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(site) = find_free_fn_site(&source, leaf) else {
            continue;
        };
        let FnSite::Free(item_fn) = site else {
            // A method mutates an owned instance: the VIS class is correct.
            continue;
        };
        let mut static_use = StaticUse::default();
        syn::visit::Visit::visit_block(&mut static_use, &item_fn.block);
        // The ratified class is a free function whose NAME denotes a reset or
        // store of process/thread static state. Ordinary helpers that read or
        // scope a TLS cache (with_*, record_*, compile_*, ...) are not rows of
        // this class and stay outside the ratchet.
        let candidate = reset_or_store_named
            && (static_use.direct_write
                || static_use.closure_write
                || (reset_named && static_use.any_access));
        if candidate {
            candidates.insert(FreeFnResetRow {
                crate_name: row.crate_dir.clone(),
                item: row.item.clone(),
                defining_file: row.file.clone(),
            });
        }
    }
    Ok(candidates)
}

fn vis_free_fn_reset_violation(workspace: &Path, rows: &[VisRow]) -> Result<(), String> {
    let computed = free_fn_reset_candidates(workspace, rows)?;
    if !computed.is_empty() {
        let offending: Vec<String> = computed
            .iter()
            .map(|row| format!("{} `{}` ({})", row.crate_name, row.item, row.defining_file))
            .collect();
        return Err(format!(
            "TEST-SUPPORT-VIS: free-function TLS/static counter reset/store VIS row(s); route the write through the owning crate's test_support::authority fence and add an AUTHORITY row (LEAD 11:2xZ; LEAD 18:1xZ retired the allowlist, so every case is red): {}",
            offending.join(" | ")
        ));
    }
    Ok(())
}

#[test]
fn test_support_vis_ledgers_obey_the_signed_class() {
    let workspace = workspace_root();
    validate(&workspace).unwrap_or_else(|error| panic!("TEST-SUPPORT-VIS: {error}"));
}

/// Peer #1005 structural guard, part 1: no VIS ledger row may name a field.
/// A field row means the field was widened for the test-support build; the
/// field-privacy rule makes every provider field `pub(crate)` in every arm,
/// with reads through getters (VIS METHOD rows) and writes through AUTHORITY
/// fences.  Zero is the ratchet: any new field row is red.
#[test]
fn vis_ledgers_have_no_field_rows() {
    let workspace = workspace_root();
    let architecture = workspace.join(ARCHITECTURE_DIR);
    let mut offenders = Vec::new();
    for path in read_dir_sorted(&architecture).expect("read architecture dir") {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(VIS_PREFIX) || !name.ends_with(".tsv") {
            continue;
        }
        let text = read(&path).expect("read VIS ledger");
        for (index, line) in text.lines().enumerate() {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() > 2 && fields[2] == "field" {
                offenders.push(format!("{name}:{}: {}", index + 1, fields[1]));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "TEST-SUPPORT-VIS field rows are retired: fields stay pub(crate) in \
         every arm, reads use getters (VIS METHOD rows) and writes use \
         AUTHORITY fences.  Field rows found:\n{}",
        offenders.join("\n")
    );
}

/// The visibility a test-support cfg arm gives one named field.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FieldVisibilityArm {
    /// Inline-module path inside the file, so same-named structs in different
    /// inline modules never collide.
    module: Vec<String>,
    struct_name: String,
    field: String,
    public: bool,
    /// `true` for the test-support arm, `false` for its complement.
    positive: bool,
}

struct FieldVisibilityCollector {
    module: Vec<String>,
    arms: Vec<FieldVisibilityArm>,
}

/// The polarity of the test-support cfg on these attributes, if any: `Some(true)`
/// for `cfg(...feature = "test-support"...)`, `Some(false)` for the `not(...)`
/// complement.  A plain `cfg(test)` arm is not a cross-crate widening and is
/// ignored (the feature, not cfg(test), is what a consumer build enables).
fn test_support_cfg_polarity(attrs: &[syn::Attribute]) -> Option<bool> {
    fn mentions(stream: proc_macro2::TokenStream) -> bool {
        stream.into_iter().any(|token| match token {
            proc_macro2::TokenTree::Ident(ident) => ident.to_string() == "test_support",
            proc_macro2::TokenTree::Literal(literal) => {
                literal.to_string().contains("test-support")
            }
            proc_macro2::TokenTree::Group(group) => mentions(group.stream()),
            proc_macro2::TokenTree::Punct(_) => false,
        })
    }
    fn negated(stream: &proc_macro2::TokenStream) -> bool {
        matches!(
            stream.clone().into_iter().next(),
            Some(proc_macro2::TokenTree::Ident(ident)) if ident == "not"
        )
    }
    for attr in attrs {
        if !attr.path().is_ident("cfg") {
            continue;
        }
        let syn::Meta::List(list) = &attr.meta else {
            continue;
        };
        if mentions(list.tokens.clone()) {
            return Some(!negated(&list.tokens));
        }
    }
    None
}

impl<'ast> Visit<'ast> for FieldVisibilityCollector {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.module.push(item.ident.to_string());
        visit::visit_item_mod(self, item);
        self.module.pop();
    }

    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        let struct_polarity = test_support_cfg_polarity(&item.attrs);
        for field in &item.fields {
            let Some(ident) = &field.ident else {
                continue;
            };
            let Some(positive) = test_support_cfg_polarity(&field.attrs).or(struct_polarity) else {
                continue;
            };
            self.arms.push(FieldVisibilityArm {
                module: self.module.clone(),
                struct_name: item.ident.to_string(),
                field: ident.to_string(),
                public: matches!(field.vis, syn::Visibility::Public(_)),
                positive,
            });
        }
        visit::visit_item_struct(self, item);
    }
}

/// Peer #1005 structural guard, part 2: a field's visibility may not change
/// between the test-support arm and its production complement.  Covers both
/// field-level cfg pairs and whole-struct cfg arms; ignores cfg(test)-only
/// arms, which no cross-crate build enables.
fn field_visibility_bifurcations(source: &str) -> Result<Vec<String>, String> {
    let file = syn::parse_file(source).map_err(|error| format!("does not parse: {error}"))?;
    let mut collector = FieldVisibilityCollector {
        module: Vec::new(),
        arms: Vec::new(),
    };
    collector.visit_file(&file);
    let mut groups: BTreeMap<(Vec<String>, String, String), BTreeSet<(bool, bool)>> =
        BTreeMap::new();
    for arm in collector.arms {
        groups
            .entry((arm.module, arm.struct_name, arm.field))
            .or_default()
            .insert((arm.public, arm.positive));
    }
    let mut violations = Vec::new();
    for ((module, struct_name, field), arms) in groups {
        let positive: BTreeSet<bool> = arms.iter().filter(|(_, p)| *p).map(|(v, _)| *v).collect();
        let negative: BTreeSet<bool> = arms.iter().filter(|(_, p)| !*p).map(|(v, _)| *v).collect();
        if positive.is_empty() || negative.is_empty() {
            continue;
        }
        if positive != negative {
            let module = if module.is_empty() {
                String::new()
            } else {
                format!("{}::", module.join("::"))
            };
            violations.push(format!(
                "{module}{struct_name}::{field}: test-support arm visibility \
                 {positive:?} differs from the production arm {negative:?}"
            ));
        }
    }
    Ok(violations)
}

/// Peer #1005 structural guard over the provider crates: every field is
/// `pub(crate)` in every arm, so the test-support build never widens one.
#[test]
fn test_support_field_visibility_never_bifurcates() {
    let workspace = workspace_root();
    let mut violations = Vec::new();
    for crate_dir in VIS_CRATES {
        let src = workspace.join("crates").join(crate_dir).join("src");
        let mut files = Vec::new();
        walk_files(&src, &mut files).expect("walk provider sources");
        for path in files {
            if path.extension().is_none_or(|extension| extension != "rs") || is_test_path(&path) {
                continue;
            }
            let source = read(&path).expect("read provider source");
            let relative = path
                .strip_prefix(&workspace)
                .expect("provider source under the workspace")
                .to_string_lossy()
                .replace('\\', "/");
            for violation in field_visibility_bifurcations(&source)
                .unwrap_or_else(|error| panic!("{relative}: {error}"))
            {
                violations.push(format!("{relative}: {violation}"));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "test-support field visibility must match the production arm; \
         narrow the field to pub(crate) and route reads through a getter / \
         writes through an AUTHORITY fence:\n{}",
        violations.join("\n")
    );
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
    fn authority_fence_inherent_method_owner_pin_mutations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let value_src = dir.path().join("crates/neovm-value/src");
        let core_src = dir.path().join("crates/neovm-core/src");
        std::fs::create_dir_all(value_src.join("test_support")).expect("value src");
        std::fs::create_dir_all(&core_src).expect("core src");
        std::fs::write(value_src.join("lib.rs"), "pub mod test_support;\n").expect("lib");
        std::fs::write(value_src.join("test_support.rs"), "pub mod authority;\n").expect("support");
        std::fs::write(core_src.join("lib.rs"), "").expect("core lib");
        let rows = vec![AuthRow {
            crate_name: "neovm-value".to_owned(),
            item: "Owner::method".to_owned(),
            kind: "method".to_owned(),
            defining_file: "crates/neovm-value/src/test_support/authority.rs".to_owned(),
            fence_path: "neovm_value::test_support::authority::Owner::method".to_owned(),
        }];
        // Case 1: a pub method on the named type is the signed public path.
        std::fs::write(
            value_src.join("test_support/authority.rs"),
            "pub struct Owner;\nimpl Owner {\n    pub fn method(&self) {}\n}\n",
        )
        .expect("named type");
        authority_public_paths_violation(dir.path(), &rows).expect("pub method on the named type");
        // Case 2: the method absent is red.
        std::fs::write(
            value_src.join("test_support/authority.rs"),
            "pub struct Owner;\n",
        )
        .expect("absent");
        let error = authority_public_paths_violation(dir.path(), &rows).expect_err("absent method");
        assert!(error.contains("does not declare"), "{error}");
        // Case 3: a private method is red.
        std::fs::write(
            value_src.join("test_support/authority.rs"),
            "pub struct Owner;\nimpl Owner {\n    fn method(&self) {}\n}\n",
        )
        .expect("private");
        let error =
            authority_public_paths_violation(dir.path(), &rows).expect_err("private method");
        assert!(error.contains("does not declare"), "{error}");
        // Case 4: the same-name pub method only on a DIFFERENT type is red.
        std::fs::write(
            value_src.join("test_support/authority.rs"),
            "pub struct Owner;\npub struct Other;\nimpl Other {\n    pub fn method(&self) {}\n}\n",
        )
        .expect("different type");
        let error =
            authority_public_paths_violation(dir.path(), &rows).expect_err("different type");
        assert!(error.contains("does not declare"), "{error}");
        // Case 5: a FREE pub fn that merely shares the method's name does not
        // declare `Owner::method` (peer 00:36:00Z follow-up; the leaf-alone
        // arms are disabled for owner-pinned rows).
        std::fs::write(
            value_src.join("test_support/authority.rs"),
            "pub struct Owner;\npub fn method() {}\n",
        )
        .expect("free fn");
        let error = authority_public_paths_violation(dir.path(), &rows).expect_err("free fn");
        assert!(error.contains("does not declare"), "{error}");
    }

    #[test]
    fn test_support_reverse_coverage_mutation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("crates/neovm-edit/src/test_support.rs");
        std::fs::create_dir_all(file.parent().unwrap()).expect("src");
        std::fs::write(&file, "pub mod authority {\n    pub fn unrowed() {}\n}\n").expect("fence");
        let error = test_support_reverse_coverage_violation(dir.path(), &[], &[])
            .expect_err("unrowed pub fn");
        assert!(error.contains("no AUTHORITY"), "{error}");
        let vis_rows = vec![VisRow {
            crate_dir: "neovm-edit".to_owned(),
            item: "test_support::authority::unrowed".to_owned(),
            kind: "fn".to_owned(),
            file: "crates/neovm-edit/src/test_support.rs".to_owned(),
            tests: Vec::new(),
        }];
        test_support_reverse_coverage_violation(dir.path(), &vis_rows, &[]).expect("rowed read");
        let auth = vec![AuthRow {
            crate_name: "neovm-edit".to_owned(),
            item: "unrowed".to_owned(),
            kind: "fn".to_owned(),
            defining_file: "crates/neovm-edit/src/test_support.rs".to_owned(),
            fence_path: "neovm_edit::test_support::authority::unrowed".to_owned(),
        }];
        let error = test_support_reverse_coverage_violation(dir.path(), &vis_rows, &auth)
            .expect_err("both routes");
        assert!(error.contains("BOTH"), "{error}");
        test_support_reverse_coverage_violation(dir.path(), &[], &auth).expect("rowed fence");
    }

    #[test]
    fn feature_only_pub_surface_mutation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("crates/neovm-edit/src");
        std::fs::create_dir_all(&src).expect("src");
        let lib = src.join("lib.rs");
        let relative = "crates/neovm-edit/src/lib.rs".to_owned();
        // A plain-pub item that exists only in the cross-crate test-support
        // build is a widening site: absent in production and absent in the
        // crate's own cfg(test) build.
        std::fs::write(
            &lib,
            "#[cfg(feature = \"test-support\")]\npub struct Surface;\n",
        )
        .expect("feature-only");
        assert_eq!(
            feature_only_pub_sites(dir.path()).expect("sites"),
            vec![(relative.clone(), "Surface".to_owned())]
        );
        // The crate's own test build also sees a cfg(any(test, feature))
        // item, so it is not this shape (the wider test-gated class is
        // reported separately).
        std::fs::write(
            &lib,
            "#[cfg(any(test, feature = \"test-support\"))]\npub struct Shared;\n",
        )
        .expect("shared");
        assert!(
            feature_only_pub_sites(dir.path())
                .expect("sites")
                .is_empty()
        );
        // cfg(test) alone is absent from the cross-crate build.
        std::fs::write(&lib, "#[cfg(test)]\npub struct OwnTestOnly;\n").expect("test-only");
        assert!(
            feature_only_pub_sites(dir.path())
                .expect("sites")
                .is_empty()
        );
        // A production item is not test-support surface.
        std::fs::write(&lib, "pub struct Production;\n").expect("production");
        assert!(
            feature_only_pub_sites(dir.path())
                .expect("sites")
                .is_empty()
        );
        // Only a plain `pub` is cross-crate surface.
        std::fs::write(
            &lib,
            "#[cfg(feature = \"test-support\")]\npub(crate) struct Restricted;\n",
        )
        .expect("restricted");
        assert!(
            feature_only_pub_sites(dir.path())
                .expect("sites")
                .is_empty()
        );
        // An inline module is walked; the item inside it is the site.
        std::fs::write(
            &lib,
            "pub mod nested {\n    #[cfg(feature = \"test-support\")]\n    pub enum Inner { A }\n}\n",
        )
        .expect("inline module");
        assert_eq!(
            feature_only_pub_sites(dir.path()).expect("sites"),
            vec![(relative, "Inner".to_owned())]
        );
    }

    #[test]
    fn macro_row_mutations() {
        let source = "#[macro_export]\nmacro_rules! eval_absent_layer_slots { () => {} }\n";
        let row = |item: &str| VisRow {
            crate_dir: "neovm-eval".to_owned(),
            item: item.to_owned(),
            kind: "macro".to_owned(),
            file: "crates/neovm-eval/src/emacs_core/runtime/eval/absent_layer.rs".to_owned(),
            tests: Vec::new(),
        };
        macro_row_violation(&row("eval_absent_layer_slots"), source).expect("crate-root macro row");
        let error = macro_row_violation(&row("emacs_core::eval::eval_absent_layer_slots"), source)
            .expect_err("module-prefixed macro row");
        assert!(error.contains("crate-root"), "{error}");
        let no_export = "macro_rules! eval_absent_layer_slots { () => {} }\n";
        let error = macro_row_violation(&row("eval_absent_layer_slots"), no_export)
            .expect_err("macro without #[macro_export]");
        assert!(error.contains("#[macro_export]"), "{error}");
        let other_macro = "#[macro_export]\nmacro_rules! other { () => {} }\n";
        let error = macro_row_violation(&row("eval_absent_layer_slots"), other_macro)
            .expect_err("file lacks the named macro");
        assert!(error.contains("macro_rules!"), "{error}");
        // Peer 01:5xZ fifth case: the checks read the parsed item, so a
        // mention in a comment or a string must not satisfy the row.
        let comment_mention = "// #[macro_export] macro_rules! eval_absent_layer_slots\nmacro_rules! eval_absent_layer_slots { () => {} }\n";
        let error = macro_row_violation(&row("eval_absent_layer_slots"), comment_mention)
            .expect_err("comment mention");
        assert!(error.contains("no #[macro_export] of its own"), "{error}");
        let string_mention = "pub const S: &str = \"#[macro_export] macro_rules! eval_absent_layer_slots\";\nmacro_rules! eval_absent_layer_slots { () => {} }\n";
        let error = macro_row_violation(&row("eval_absent_layer_slots"), string_mention)
            .expect_err("string mention");
        assert!(error.contains("no #[macro_export] of its own"), "{error}");
        // A different exported macro in the same file does not cover a
        // non-exported one.
        let exported_other = "#[macro_export]\nmacro_rules! other { () => {} }\nmacro_rules! eval_absent_layer_slots { () => {} }\n";
        let error = macro_row_violation(&row("eval_absent_layer_slots"), exported_other)
            .expect_err("another exported macro in the file");
        assert!(error.contains("no #[macro_export] of its own"), "{error}");
    }

    #[test]
    fn unit_runtime_absent_layer_reference_mutations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let write = |relative: &str, text: &str| {
            let path = dir.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).expect("parent");
            std::fs::write(path, text).expect("write");
        };
        let check = || unit_runtime_absent_layer_violation(dir.path());
        // A canonical crate may not reference the helper, the marker or the
        // macro, even from a cfg(test) unit_runtime module.
        write(
            "crates/neovm-core/src/lib.rs",
            "pub fn f() { absent_layer_unlinked(\"x\"); }\n",
        );
        let error = check().expect_err("helper in core");
        assert!(error.contains("absent_layer_unlinked"), "{error}");
        write(
            "crates/neovm-core/src/lib.rs",
            "pub fn f() -> &'static str { \"ABSENT-LAYER gc::roots\" }\n",
        );
        let error = check().expect_err("marker in core");
        assert!(error.contains("ABSENT-LAYER"), "{error}");
        write(
            "crates/neovm-core/src/lib.rs",
            "// ABSENT-LAYER in a comment is not a reference\npub fn f() {}\n",
        );
        check().expect("comment is not a reference");
        write(
            "crates/neovm-core/src/lib.rs",
            "#[cfg(test)]\nmod unit_runtime {\n    pub fn build() { eval_absent_layer_slots! {} }\n}\n",
        );
        let error = check().expect_err("macro in core");
        assert!(error.contains("unit set"), "{error}");
        write("crates/neovm-core/src/lib.rs", "");
        // Provider crates: the macro is invoked only inside a cfg(test)
        // unit_runtime module, never a feature-gated one.
        write(
            "crates/neovm-eval/src/lib.rs",
            "#[cfg(test)]\nmod unit_runtime {\n    fn build() { eval_absent_layer_slots! {} }\n}\n",
        );
        check().expect("cfg(test) unit_runtime invocation");
        write(
            "crates/neovm-eval/src/lib.rs",
            "fn build() { eval_absent_layer_slots! {} }\n",
        );
        let error = check().expect_err("outside unit_runtime");
        assert!(
            error.contains("outside a cfg(test) unit_runtime module"),
            "{error}"
        );
        write(
            "crates/neovm-eval/src/lib.rs",
            "#[cfg(any(test, feature = \"test-support\"))]\nmod unit_runtime {\n    fn build() { eval_absent_layer_slots! {} }\n}\n",
        );
        let error = check().expect_err("feature-gated unit_runtime");
        assert!(
            error.contains("outside a cfg(test) unit_runtime module"),
            "{error}"
        );
        // One definition, gated for test-support.
        write(
            "crates/neovm-eval/src/lib.rs",
            "#[cfg(any(test, feature = \"test-support\"))]\nmacro_rules! eval_absent_layer_slots { () => {}; }\n#[cfg(test)]\nmod unit_runtime { fn build() { eval_absent_layer_slots! {} } }\n",
        );
        check().expect("definition and invocation");
        write(
            "crates/neovm-eval/src/lib.rs",
            "#[cfg(any(test, feature = \"test-support\"))]\nmacro_rules! eval_absent_layer_slots { () => {}; }\n#[cfg(any(test, feature = \"test-support\"))]\nmacro_rules! eval_absent_layer_slots { () => {}; }\n",
        );
        let error = check().expect_err("two definitions");
        assert!(error.contains("exactly one"), "{error}");
        write(
            "crates/neovm-eval/src/lib.rs",
            "macro_rules! eval_absent_layer_slots { () => {}; }\n",
        );
        let error = check().expect_err("ungated definition");
        assert!(error.contains("gated"), "{error}");
    }

    #[test]
    fn unit_runtime_slot_table_matches_the_signed_shapes() {
        unit_runtime_slot_violation(&workspace_root()).expect("the AbsentLayer slot table");
    }

    #[test]
    fn unit_runtime_slot_mutations() {
        let synthetic = |slots: &str| {
            format!(
                "macro_rules! eval_absent_layer_slots {{ () => {{ pub(crate) mod absent_layer_slots {{ {slots} }} }} }}"
            )
        };
        let valid_slots = r#"
            fn panic_slot() -> ! { absent_layer_panic("ABSENT-LAYER a::panic") }
            fn unlinked_slot() { absent_layer_unlinked("ABSENT-LAYER a::unlinked"); }
            fn assert_slot(roots: &mut Vec<u8>) {
                let before = roots.len();
                absent_layer_assert_empty("ABSENT-LAYER a::assert", roots.len() == before);
            }
            fn handle_slot() { record_hit(AbsentLayerHit::Unlinked, "ABSENT-LAYER a::handle"); absent_face_registry_handle() }
            fn marker_slot() { record_hit(AbsentLayerHit::Unlinked, "ABSENT-LAYER a::marker"); absent_fontset_alias_marker() }
        "#;
        let items = absent_layer_module_items(&synthetic(valid_slots)).expect("valid macro");
        let summary = classify_absent_layer_slots(&items).expect("valid slot shapes");
        assert_eq!(summary.panic.len(), 1);
        assert_eq!(summary.unlinked.len(), 1);
        assert_eq!(summary.assert_empty.len(), 1);
        assert_eq!(summary.handle.len(), 1);
        assert_eq!(summary.marker.len(), 1);
        // A defaulting slot body (bare return, no marker) goes red.
        let defaulting = synthetic("fn panic_slot() -> Value { Value::NIL }");
        let items = absent_layer_module_items(&defaulting).expect("macro parses");
        let error = classify_absent_layer_slots(&items).expect_err("defaulting slot");
        assert!(error.contains("silent default"), "{error}");
        // An ASSERT-EMPTY body with the assert removed goes red.
        let bare_assert = synthetic("fn assert_slot() { }");
        let items = absent_layer_module_items(&bare_assert).expect("macro parses");
        let error = classify_absent_layer_slots(&items).expect_err("bare assert-empty");
        assert!(error.contains("silent default"), "{error}");
        // An UNLINKED body with an extra statement is not the signed one call.
        let extra_unlinked = synthetic(
            "fn unlinked_slot() { let _ = 1; absent_layer_unlinked(\"ABSENT-LAYER a::u\"); }",
        );
        let items = absent_layer_module_items(&extra_unlinked).expect("macro parses");
        let error = classify_absent_layer_slots(&items).expect_err("extra unlinked statement");
        assert!(error.contains("silent default"), "{error}");
        // T7: a construction-reached UNLINKED slot records the hit and then
        // returns its honest absent value; the value is part of the facts.
        let valued_unlinked = synthetic(
            "fn unlinked_none() -> Option<i32> { absent_layer_unlinked(\"ABSENT-LAYER a::u\"); None }",
        );
        let items = absent_layer_module_items(&valued_unlinked).expect("macro parses");
        let summary = classify_absent_layer_slots(&items).expect("valued unlinked slot");
        assert_eq!(summary.unlinked.len(), 1);
        assert_eq!(summary.unlinked[0].2.as_deref(), Some("None"));
        // A trailing value that is not a plain path (here a `Some(0)` call)
        // is not the signed honest-value shape.
        let valued_call = synthetic(
            "fn unlinked_none() -> Option<i32> { absent_layer_unlinked(\"ABSENT-LAYER a::u\"); Some(0) }",
        );
        let items = absent_layer_module_items(&valued_call).expect("macro parses");
        let error = classify_absent_layer_slots(&items).expect_err("valued call");
        assert!(error.contains("silent default"), "{error}");
        // The recorded call must come first; value-then-call is not the shape.
        let value_first = synthetic(
            "fn unlinked_none() -> Option<i32> { None; absent_layer_unlinked(\"ABSENT-LAYER a::u\") }",
        );
        let items = absent_layer_module_items(&value_first).expect("macro parses");
        let error = classify_absent_layer_slots(&items).expect_err("value before call");
        assert!(error.contains("silent default"), "{error}");
        // C2: the direct T2/T3 sites must record the Unlinked class; a
        // Refused class on an absent-by-construction slot is red.
        let wrong_kind = synthetic(
            "fn handle_slot() { record_hit(AbsentLayerHit::Refused, \"ABSENT-LAYER a::handle\"); absent_face_registry_handle() }",
        );
        let items = absent_layer_module_items(&wrong_kind).expect("macro parses");
        let error = classify_absent_layer_slots(&items).expect_err("wrong direct-site kind");
        assert!(error.contains("silent default"), "{error}");

        let function = |source: &str| syn::parse_str::<syn::ItemFn>(source).expect("helper fn");
        // H4/C2: the out-of-band record is typed and writes the env-named
        // append-only log plus the (kind, marker) TLS entry.
        validate_record_hit(&function(
            r#"fn record_hit(kind: AbsentLayerHit, marker: &'static str) {
                let enabled = std::env::var_os("NEOVM_ABSENT_LAYER_LOG");
                if let Some(path) = enabled {
                    let log = HIT_LOG.get_or_init(|| {
                        let file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok();
                        Mutex::new(file)
                    });
                    if let Ok(mut guard) = log.lock() {
                        if let Some(file) = guard.as_mut() {
                            let _ = writeln!(file, "{}: {marker}", kind.label());
                        }
                    }
                }
                HITS.with(|hits| hits.borrow_mut().push((kind, marker)));
            }"#,
        ))
        .expect("valid typed record_hit");
        let no_tls = function(
            r#"fn record_hit(kind: AbsentLayerHit, marker: &'static str) {
                let enabled = std::env::var_os("NEOVM_ABSENT_LAYER_LOG");
                if let Some(path) = enabled {
                    let log = HIT_LOG.get_or_init(|| {
                        let file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok();
                        Mutex::new(file)
                    });
                    if let Ok(mut guard) = log.lock() {
                        if let Some(file) = guard.as_mut() {
                            let _ = writeln!(file, "{}: {marker}", kind.label());
                        }
                    }
                }
            }"#,
        );
        assert!(
            validate_record_hit(&no_tls).is_err(),
            "missing TLS hit record"
        );
        let no_log = function(
            r#"fn record_hit(kind: AbsentLayerHit, marker: &'static str) {
                HITS.with(|hits| hits.borrow_mut().push((kind, marker)));
            }"#,
        );
        assert!(
            validate_record_hit(&no_log).is_err(),
            "missing append-only log"
        );
        // C2 missing-kind: the pre-C2 untyped spelling is not a hit record.
        let untyped = function(
            r#"fn record_hit(marker: &'static str) {
                let enabled = std::env::var_os("NEOVM_ABSENT_LAYER_LOG");
                HITS.with(|hits| hits.borrow_mut().push(marker));
            }"#,
        );
        assert!(
            validate_record_hit(&untyped).is_err(),
            "an untyped record_hit must be red"
        );
        // C2 missing label: a typed signature without the class label is red.
        let no_label = function(
            r#"fn record_hit(kind: AbsentLayerHit, marker: &'static str) {
                let enabled = std::env::var_os("NEOVM_ABSENT_LAYER_LOG");
                if let Some(path) = enabled {
                    let log = HIT_LOG.get_or_init(|| {
                        let file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok();
                        Mutex::new(file)
                    });
                    if let Ok(mut guard) = log.lock() {
                        if let Some(file) = guard.as_mut() {
                            let _ = writeln!(file, "{marker}");
                        }
                    }
                }
                HITS.with(|hits| hits.borrow_mut().push((kind, marker)));
            }"#,
        );
        assert!(
            validate_record_hit(&no_label).is_err(),
            "an unlabelled hit line must be red"
        );
        // C2: the Unlinked helper records the Unlinked class.
        validate_unlinked_helper(&function(
            "fn absent_layer_unlinked(marker: &'static str) { record_hit(AbsentLayerHit::Unlinked, marker); }",
        ))
        .expect("valid unlinked helper");
        assert!(
            validate_unlinked_helper(&function(
                "fn absent_layer_unlinked(marker: &'static str) { let _ = marker; record_hit(AbsentLayerHit::Unlinked, marker); }",
            ))
            .is_err(),
            "an unlinked body with an extra statement must be red"
        );
        assert!(
            validate_unlinked_helper(&function(
                "fn absent_layer_unlinked(marker: &'static str) { record_hit(marker); }",
            ))
            .is_err(),
            "the pre-C2 untyped unlinked record must be red"
        );
        assert!(
            validate_unlinked_helper(&function(
                "fn absent_layer_unlinked(marker: &'static str) { record_hit(AbsentLayerHit::Refused, marker); }",
            ))
            .is_err(),
            "an unlinked no-op recording a Refused hit must be red"
        );
        // H4/C2: the panic path records a Refused hit BEFORE it aborts.
        validate_panic_helper(&function(
            r#"fn absent_layer_panic(marker: &'static str) -> ! {
                record_hit(AbsentLayerHit::Refused, marker);
                eprintln!("{marker}");
                panic!("{marker}")
            }"#,
        ))
        .expect("valid panic helper");
        assert!(
            validate_panic_helper(&function(
                r#"fn absent_layer_panic(marker: &'static str) -> ! {
                    eprintln!("{marker}");
                    record_hit(AbsentLayerHit::Refused, marker);
                    panic!("{marker}")
                }"#,
            ))
            .is_err(),
            "a panic path that records after printing must be red"
        );
        assert!(
            validate_panic_helper(&function(
                r#"fn absent_layer_panic(marker: &'static str) -> ! {
                    record_hit(AbsentLayerHit::Unlinked, marker);
                    eprintln!("{marker}");
                    panic!("{marker}")
                }"#,
            ))
            .is_err(),
            "a panic path recording an Unlinked hit must be red"
        );
        // C2: the hit enum, its label spelling and the drain accessor are pinned.
        let hit_items =
            |source: &str| absent_layer_module_items(&synthetic(source)).expect("macro parses");
        validate_absent_layer_hit_enum(&hit_items(
            "pub(crate) enum AbsentLayerHit { Refused, Unlinked } impl AbsentLayerHit { fn label(self) -> &'static str { match self { Self::Refused => \"refused\", Self::Unlinked => \"unlinked\" } } }",
        ))
        .expect("valid hit enum");
        assert!(
            validate_absent_layer_hit_enum(&hit_items(
                "pub(crate) enum AbsentLayerHit { Refused, Absent } impl AbsentLayerHit { fn label(self) -> &'static str { match self { Self::Refused => \"refused\", Self::Absent => \"absent\" } } }",
            ))
            .is_err(),
            "a third hit class must be red"
        );
        validate_take_hits_helper(&function(
            "pub(crate) fn take_absent_layer_hits_for_test() -> Vec<(AbsentLayerHit, &'static str)> { HITS.with(|hits| std::mem::take(&mut *hits.borrow_mut())) }",
        ))
        .expect("valid hit-list drain");
        assert!(
            validate_take_hits_helper(&function(
                "pub(crate) fn take_absent_layer_hits_for_test() -> Vec<(AbsentLayerHit, &'static str)> { HITS.with(|hits| hits.borrow().clone()) }",
            ))
            .is_err(),
            "a non-draining hit accessor must be red"
        );
        assert!(
            validate_take_hits_helper(&function(
                "pub(crate) fn take_absent_layer_hits_for_test() -> Vec<&'static str> { HITS.with(|hits| std::mem::take(&mut *hits.borrow_mut())) }",
            ))
            .is_err(),
            "an untyped hit-list return must be red"
        );
        // H2a: assert-empty proves emptiness before returning.
        validate_assert_empty_helper(&function(
            "fn absent_layer_assert_empty(marker: &'static str, empty: bool) { if !empty { absent_layer_panic(marker); } }",
        ))
        .expect("valid assert-empty helper");
        assert!(
            validate_assert_empty_helper(&function(
                "fn absent_layer_assert_empty(marker: &'static str, empty: bool) { }",
            ))
            .is_err(),
            "an assert-empty helper without the assert must be red"
        );
        // T2: a per-thread handle is lazily created and reused.
        validate_handle_helper(&function(
            r#"fn absent_face_registry_handle() -> Handle {
                thread_local! {
                    static HANDLE: std::cell::OnceCell<Handle> = const { std::cell::OnceCell::new() };
                }
                HANDLE.with(|cell| cell.get_or_init(|| Handle::new()).clone())
            }"#,
        ))
        .expect("valid handle helper");
        assert!(
            validate_handle_helper(&function(
                "fn absent_face_registry_handle() -> Handle { Handle::new() }",
            ))
            .is_err(),
            "a fresh handle per call must be red"
        );
        // T3: an uninterned marker, never nil.
        validate_alias_marker_helper(&function(
            r#"fn absent_fontset_alias_marker() -> Value {
                let name = Value::string("ABSENT-LAYER display::fontset.alias_alist_startup_value");
                Value::from_sym_id(neovm_value::intern::make_uninterned_symbol_with_name_value(name))
            }"#,
        ))
        .expect("valid alias marker helper");
        assert!(
            validate_alias_marker_helper(&function(
                "fn absent_fontset_alias_marker() -> Value { Value::NIL }",
            ))
            .is_err(),
            "nil for the fontset alias must be red"
        );
        // T4: bootstrap names bind to uninterned markers and stay special.
        validate_bootstrap_binder(&function(
            r#"fn bind_absent_bootstrap_vars(obarray: &mut Obarray, names: &[&'static str]) {
                for name in names {
                    let marker_name = Value::string(format!("ABSENT-LAYER upper-variable::{name}"));
                    let marker = Value::from_sym_id(make_uninterned_symbol_with_name_value(marker_name));
                    obarray.make_special(name);
                    obarray.set_symbol_value(name, marker);
                }
            }"#,
        ))
        .expect("valid bootstrap binder");
        assert!(
            validate_bootstrap_binder(&function(
                r#"fn bind_absent_bootstrap_vars(obarray: &mut Obarray, names: &[&'static str]) {
                    for name in names {
                        let marker_name = Value::string(format!("ABSENT-LAYER upper-variable::{name}"));
                        let marker = Value::from_sym_id(make_uninterned_symbol_with_name_value(marker_name));
                        obarray.set_symbol_value(name, marker);
                    }
                }"#,
            ))
            .is_err(),
            "a binder that skips make_special must be red"
        );
        // T1 metadata: an upper dependency edge in eval is red.
        validate_eval_has_no_upper_dependency(
            "[dependencies]\nneovm-value = { path = \"../neovm-value\" }\n[dev-dependencies]\nneovm-text = { path = \"../neovm-text\" }\n",
        )
        .expect("no upper edge");
        let error = validate_eval_has_no_upper_dependency(
            "[dev-dependencies.neovm-display]\npath = \"../neovm-display\"\n",
        )
        .expect_err("upper dependency edge");
        assert!(error.contains("dependency edge"), "{error}");
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
        // An ABI string follows `extern`: `extern "C" fn ...`.
        assert_eq!(
            declaration_name("pub extern \"C\" fn aset_multibyte() {}"),
            Some("aset_multibyte".to_owned())
        );
        // A widened `use` re-export names the imported item. The leaf is the
        // leading identifier of the final path segment, so a braced list names
        // its first item and a glob names nothing.
        assert_eq!(
            declaration_name(
                "pub(crate) use child_status::{UnrecordedStatusRead, UpdateStatusSite};"
            ),
            Some("UnrecordedStatusRead".to_owned())
        );
        assert_eq!(declaration_name("pub(crate) use helpers::*;"), None);
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

    /// P1-HOOKS-2: a fence-routed A' item has its AUTHORITY row at the
    /// defining file and MUST NOT get a second VIS row (V1). That AUTHORITY
    /// row is the coverage for its `visibility::make(pub)` site; a row at a
    /// different file does not cover it.
    #[test]
    fn authority_row_covers_a_fence_widening_site() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("crates/neovm-value/src");
        std::fs::create_dir_all(&src).expect("src");
        std::fs::write(
            src.join("fence_item.rs"),
            r#"
#[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
pub(crate) struct FenceOnly;
"#,
        )
        .expect("fence item");
        // Without the AUTHORITY row the widening site is unrowed.
        let error = check_visibility_coverage(dir.path(), &[]).expect_err("unrowed fence item");
        assert!(error.contains("FenceOnly"), "{error}");
        eprintln!("MUTATION RED: {error}");
        let arch = dir
            .path()
            .join("crates/neovm-core/src/emacs_core/tests/architecture");
        std::fs::create_dir_all(&arch).expect("arch");
        std::fs::write(
            arch.join("AUTHORITY-ITEMS.tsv"),
            "crate\titem\tkind\tdefining_file\tfence_path\ttests\treason\tsafety\n\
             neovm-value\tFenceOnly\tstruct\tcrates/neovm-value/src/fence_item.rs\tneovm_value::test_support::authority::FenceOnly\tcrates/neovm-core/core_tests/moved.rs\tP1-HOOKS-2 fence-routed A' item; the fence is the only public path\tsafe\n",
        )
        .expect("authority ledger");
        assert!(check_visibility_coverage(dir.path(), &[]).is_ok());
        // A row at a different defining file is not coverage for this site.
        std::fs::write(
            arch.join("AUTHORITY-ITEMS.tsv"),
            "crate\titem\tkind\tdefining_file\tfence_path\ttests\treason\tsafety\n\
             neovm-value\tFenceOnly\tstruct\tcrates/neovm-value/src/other.rs\tneovm_value::test_support::authority::FenceOnly\tcrates/neovm-core/core_tests/moved.rs\tmisplaced row\tsafe\n",
        )
        .expect("authority ledger");
        assert!(check_visibility_coverage(dir.path(), &[]).is_err());
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

    #[test]
    fn cfg_split_arm_identity_ignores_doc_only_differences() {
        // Peer amendment 07:28Z: docs are not identity. `#[doc(hidden)]` is
        // legitimately test-arm-only, and doc comments may differ per arm.
        let mutated = CS1_BASELINE
            .replace(
                "#[cfg(not(feature = \"test-support\"))]\npub(crate) struct Split {",
                "/// production-only docs\n#[cfg(not(feature = \"test-support\"))]\npub(crate) struct Split {",
            )
            .replace(
                "#[cfg(feature = \"test-support\")]\npub struct Split {",
                "#[doc(hidden)]\n#[cfg(feature = \"test-support\")]\npub struct Split {",
            )
            .replace(
                "pub(crate) visible: u8,\n    pub(crate) widened: u8,",
                "/// production field docs\n    pub(crate) visible: u8,\n    pub(crate) widened: u8,",
            );
        assert_ne!(mutated, CS1_BASELINE, "mutation applied");
        let dir = cs1_workspace(&mutated);
        let file = "crates/neovm-value/src/split.rs".to_owned();
        let rows = vec![field_row("neovm-value", "Split::widened", &file)];
        assert!(check_cfg_split_arm_identity(dir.path(), &rows).is_ok());
    }

    #[test]
    fn cfg_split_arm_identity_still_rejects_a_doc_and_derive_difference() {
        let mutated = CS1_BASELINE.replace(
            "#[cfg(feature = \"test-support\")]\npub struct Split {",
            "/// test-only docs\n#[cfg(feature = \"test-support\")]\n#[derive(Clone)]\npub struct Split {",
        );
        assert_ne!(mutated, CS1_BASELINE, "mutation applied");
        let dir = cs1_workspace(&mutated);
        let file = "crates/neovm-value/src/split.rs".to_owned();
        let rows = vec![field_row("neovm-value", "Split::widened", &file)];
        let error = check_cfg_split_arm_identity(dir.path(), &rows).expect_err("doc+derive");
        assert!(error.contains("not token-identical"), "{error}");
        eprintln!("MUTATION RED: {error}");
    }

    #[test]
    fn cfg_split_arm_identity_accepts_a_widened_tuple_field_without_a_field_row() {
        // A tuple field has no name to ledger (BaseTableView/SearchRegisters);
        // the struct item row covers the widening and identity fixes the shape.
        const TUPLE: &str = r#"
#[cfg(not(feature = "test-support"))]
pub(crate) struct Tuple(pub(crate) u8);
#[cfg(feature = "test-support")]
pub struct Tuple(pub u8);
"#;
        let dir = cs1_workspace(TUPLE);
        let file = "crates/neovm-value/src/split.rs".to_owned();
        let row = VisRow {
            crate_dir: "neovm-value".to_owned(),
            item: "Tuple".to_owned(),
            kind: "struct".to_owned(),
            file,
            tests: Vec::new(),
        };
        assert!(check_cfg_split_arm_identity(dir.path(), &[row]).is_ok());
    }

    // ------------------------------------------------------------------
    // EVAL-UNIT-RUNTIME H3b/T4 (peer-signed per-module fallback): each of
    // the four canonical bootstrap-variable binders must bind exactly its
    // snapshot slice, and every bound name must be special. Within-module
    // order stays pinned by the generation source; the cross-crate check is
    // the per-module set + specialness + count.

    const UPPER_BOOTSTRAP_VARS_SNAPSHOT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../neovm-eval/src/upper_bootstrap_vars.snapshot.tsv"
    ));

    /// True when a snapshot cfg class is active for this build.
    ///
    /// The class vocabulary and its activity predicate have one home: eval's
    /// shared `HostCfgClass`, compiled in the crate whose build script ran the
    /// workspace host-cfg probe. A local `cfg!` copy would read the *test*
    /// crate's cfg set instead, which is the extraction drift this replaces
    /// (a harness build compiles the binder with eval's host cfg but the test
    /// crate without it).
    fn snapshot_cfg_active(cfg: &str) -> bool {
        neovm_eval::host_cfg::HostCfgClass::from_label(cfg)
            .unwrap_or_else(|| panic!("unknown snapshot cfg class `{cfg}`"))
            .is_active()
    }

    /// Snapshot rows: name, layer, cfg, production default (T4).
    ///
    /// The default is `marker` for Lisp_Object variables: the binder binds an
    /// uninterned ABSENT-LAYER marker symbol, and its identity belongs to the
    /// binding site, not to production. Forwarded typed variables carry
    /// `int:<n>`, the binder's own production default, because a typed slot
    /// refuses a Lisp_Object write (T4; peer ruling 05:2xZ (3)).
    fn bootstrap_snapshot_rows() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
        let mut rows = Vec::new();
        for (index, line) in UPPER_BOOTSTRAP_VARS_SNAPSHOT.lines().enumerate() {
            if index == 0 || line.trim().is_empty() {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), 4, "bootstrap snapshot row shape: {line:?}");
            rows.push((fields[0], fields[1], fields[2], fields[3]));
        }
        rows
    }

    /// The snapshots are pure data: a stray item would make the by-path
    /// include a declaration surface instead of the generated data.
    fn assert_pure_data_snapshot(label: &str, text: &str) {
        for needle in ["fn ", "use ", "impl "] {
            assert!(
                !text.contains(needle),
                "{label} must be pure data; found `{needle}`"
            );
        }
    }

    /// A heap scope for the H3b bootstrap binders.
    ///
    /// The binders construct tagged values (`Value::string` for the dbus
    /// version strings, hash tables, ...), so each call needs a
    /// thread-local `TaggedHeap`. A core `#[test]` thread has none, and the
    /// binder crates are dependency builds where `cfg(test)` is off, so
    /// their `with_tagged_heap` panics "no TaggedHeap set for this thread"
    /// instead of creating the test fallback. Installing a heap around each
    /// binder keeps the binders on their production path; the `Drop` clears
    /// the TLS slot before the owned heap is freed, including on a panic.
    struct BootstrapBinderHeapScope {
        heap: Box<neovm_value::tagged::gc::TaggedHeap>,
    }

    impl BootstrapBinderHeapScope {
        fn new() -> Self {
            let mut heap = Box::new(neovm_value::tagged::gc::TaggedHeap::new());
            neovm_value::tagged::gc::set_tagged_heap(&mut heap);
            Self { heap }
        }
    }

    impl Drop for BootstrapBinderHeapScope {
        fn drop(&mut self) {
            neovm_value::tagged::gc::clear_tagged_heap_if_installed(&self.heap);
        }
    }

    fn bound_names(obarray: &neovm_value::symbol::Obarray) -> BTreeSet<String> {
        obarray
            .iter_symbols()
            .filter(|(id, _)| obarray.boundp_id(*id))
            .map(|(_, symbol)| neovm_value::intern::resolve_name(symbol.name()).to_owned())
            .collect()
    }

    fn assert_binder_binds_its_snapshot_slice(
        layer: &str,
        bind: impl FnOnce(&mut neovm_value::symbol::Obarray),
    ) {
        let mut obarray = neovm_value::symbol::Obarray::new();
        // `Obarray::new` pre-binds GNU's t and nil; compare the names the
        // binder ADDS, so that pair can neither mask a missing name nor
        // count as an extra one.
        //
        // Run the binder inside its own heap scope; the comparison below is
        // unchanged.
        let _heap_scope = BootstrapBinderHeapScope::new();
        let before = bound_names(&obarray);
        bind(&mut obarray);
        let after = bound_names(&obarray);
        let actual: BTreeSet<String> = after.difference(&before).cloned().collect();
        let expected_rows: Vec<(&'static str, &'static str)> = bootstrap_snapshot_rows()
            .into_iter()
            .filter(|(_, row_layer, cfg, _)| *row_layer == layer && snapshot_cfg_active(cfg))
            .map(|(name, _, _, default)| (name, default))
            .collect();
        let expected: BTreeSet<String> = expected_rows
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect();
        assert_eq!(
            actual, expected,
            "the {layer} bootstrap binder's bound-name set must equal its snapshot slice"
        );
        for name in &actual {
            assert!(
                obarray.is_special(name),
                "the {layer} bootstrap binder must make `{name}` special"
            );
        }
        // T4 typed defaults (peer ruling 05:2xZ (3), LEAD 06:4xZ (b)): a
        // forwarded typed variable's snapshot row carries `int:<n>`, and the
        // real binder must bind exactly that production value. `marker` rows
        // are checked only for boundness and specialness above (their
        // uninterned marker symbol belongs to the binding site).
        for (name, default) in &expected_rows {
            let (name, default) = (*name, *default);
            match default.strip_prefix("int:") {
                Some(raw) => {
                    let want: i64 = raw.parse().unwrap_or_else(|_| {
                        panic!("snapshot `{name}` default `{default}` is not `int:<i64>`")
                    });
                    let got = obarray
                        .canonical_value_by_name(
                            neovm_value::symbol::CanonicalAccess::BOOTSTRAP,
                            name,
                        )
                        .unwrap_or_else(|| panic!("the {layer} binder did not bind `{name}`"));
                    assert_eq!(
                        got,
                        neovm_value::value::Value::fixnum(want),
                        "the {layer} binder must bind `{name}` to its production default {want}"
                    );
                }
                None => assert_eq!(
                    default, "marker",
                    "snapshot `{name}` default must be `marker` or `int:<i64>`, got `{default}`"
                ),
            }
        }
    }

    #[test]
    fn unit_runtime_upper_bootstrap_vars_match_the_snapshot() {
        assert_pure_data_snapshot(
            "upper_bootstrap_vars.snapshot.tsv",
            UPPER_BOOTSTRAP_VARS_SNAPSHOT,
        );
        // The snapshot's module blocks must appear in the canonical binder
        // order. Calling the four binders in this same order (below) pins
        // the module sequence; within-module order comes from the generator.
        let mut blocks: Vec<&str> = Vec::new();
        for (_, layer, _, _) in bootstrap_snapshot_rows() {
            if blocks.last() != Some(&layer) {
                blocks.push(layer);
            }
        }
        assert_eq!(
            blocks,
            ["fileio", "process", "dbusbind", "xfaces"],
            "the bootstrap snapshot's module blocks must be fileio, process, dbusbind, xfaces in canonical order"
        );
        // Coverage closure (H3b r2, LEAD 03:2xZ): the four binder slices
        // must PARTITION the host-active snapshot. A row in a layer that no
        // binder covers would otherwise go unchecked, and the count check
        // rejects a duplicate row that set equality would hide.
        let rows = bootstrap_snapshot_rows();
        for (_, layer, _, _) in &rows {
            assert!(
                ["fileio", "process", "dbusbind", "xfaces"].contains(layer),
                "bootstrap snapshot row in an uncovered layer `{layer}`"
            );
        }
        // T4 (peer ruling 05:2xZ (3)): exactly the four forwarded typed
        // variables carry a production default; every other row is a
        // Lisp_Object marker. Pin the four so the value comparison below
        // cannot go vacuously green if the snapshot loses its `int:` rows.
        // GNU 31.1: process.c read_process_output_max=65536 and
        // process_error_pause_time=1, gnutls.c global_gnutls_log_level=0,
        // xfaces.c face_near_same_color_threshold=30000.
        let typed: Vec<(&str, &str)> = rows
            .iter()
            .filter(|(_, _, cfg, default)| snapshot_cfg_active(cfg) && default.starts_with("int:"))
            .map(|(name, _, _, default)| (*name, *default))
            .collect();
        assert_eq!(
            typed,
            [
                ("read-process-output-max", "int:65536"),
                ("process-error-pause-time", "int:1"),
                ("gnutls-log-level", "int:0"),
                ("face-near-same-color-threshold", "int:30000"),
            ],
            "the four forwarded typed bootstrap vars must carry their production defaults"
        );
        let active: BTreeSet<&str> = rows
            .iter()
            .filter(|(_, _, cfg, _)| snapshot_cfg_active(cfg))
            .map(|(name, _, _, _)| *name)
            .collect();
        let mut sliced: BTreeSet<&str> = BTreeSet::new();
        let mut slice_count = 0usize;
        for layer in ["fileio", "process", "dbusbind", "xfaces"] {
            for (name, row_layer, cfg, _) in &rows {
                if *row_layer == layer && snapshot_cfg_active(cfg) {
                    sliced.insert(*name);
                    slice_count += 1;
                }
            }
        }
        assert_eq!(
            sliced, active,
            "the four binder slices together must equal every host-active snapshot row"
        );
        assert_eq!(
            slice_count,
            active.len(),
            "the binder slices must not overlap or duplicate a snapshot row"
        );
        assert_binder_binds_its_snapshot_slice(
            "fileio",
            neovm_system::emacs_core::fileio::register_bootstrap_vars,
        );
        assert_binder_binds_its_snapshot_slice(
            "process",
            neovm_system::emacs_core::process::register_bootstrap_vars,
        );
        assert_binder_binds_its_snapshot_slice(
            "dbusbind",
            neovm_system::emacs_core::dbusbind::register_bootstrap_vars,
        );
        assert_binder_binds_its_snapshot_slice(
            "xfaces",
            neovm_display::emacs_core::xfaces::register_bootstrap_vars,
        );
    }
}

/// LEAD 12:3xZ: a VIS `mod` row is a blanket widening. The expansion must
/// flag a pub item added to the module without its own VIS/AUTHORITY row,
/// while an item that is already public on a production path is accepted.
#[test]
fn mod_row_expansion_requires_a_row_for_each_new_pub_item() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("crates/neovm-value/src");
    std::fs::create_dir_all(src.join("gc")).expect("src");
    std::fs::write(src.join("lib.rs"), "pub mod gc;\n").expect("lib");
    let gc = "pub use facade::Published;\n\
              #[cfg(not(feature = \"test-support\"))]\nmod facade;\n\
              #[cfg(feature = \"test-support\")]\npub mod facade;\n";
    std::fs::write(src.join("gc.rs"), gc).expect("gc");
    let facade = src.join("gc/facade.rs");
    std::fs::write(
        &facade,
        "pub struct Published;\npub struct Unrowed;\npub fn new_public_fn() {}\nimpl Published {\n    pub fn method(&self) {}\n}\n",
    )
    .expect("facade");
    let mod_row = || VisRow {
        crate_dir: "neovm-value".to_owned(),
        item: "gc::facade".to_owned(),
        kind: "mod".to_owned(),
        file: "crates/neovm-value/src/gc.rs".to_owned(),
        tests: Vec::new(),
    };
    let item_row = |item: &str, kind: &str| VisRow {
        crate_dir: "neovm-value".to_owned(),
        item: item.to_owned(),
        kind: kind.to_owned(),
        file: "crates/neovm-value/src/gc/facade.rs".to_owned(),
        tests: Vec::new(),
    };
    let error =
        expanded_mod_rows_violation(dir.path(), &[mod_row()], &[]).expect_err("unrowed pub items");
    assert!(
        error.contains("Unrowed") && error.contains("new_public_fn"),
        "{error}"
    );
    assert!(!error.contains("Published"), "root-public item: {error}");
    let rows = vec![
        mod_row(),
        item_row("gc::facade::Unrowed", "struct"),
        item_row("gc::facade::new_public_fn", "fn"),
    ];
    assert!(expanded_mod_rows_violation(dir.path(), &rows, &[]).is_ok());
    // A file-module row may name the module's own defining file instead of
    // the parent that declares it; the same pub surface is expanded.
    let self_file_row = VisRow {
        crate_dir: "neovm-value".to_owned(),
        item: "gc::facade".to_owned(),
        kind: "mod".to_owned(),
        file: "crates/neovm-value/src/gc/facade.rs".to_owned(),
        tests: Vec::new(),
    };
    let self_rows = vec![
        self_file_row,
        item_row("gc::facade::Unrowed", "struct"),
        item_row("gc::facade::new_public_fn", "fn"),
    ];
    assert!(expanded_mod_rows_violation(dir.path(), &self_rows, &[]).is_ok());
    let error = expanded_mod_rows_violation(dir.path(), &self_rows[..1], &[])
        .expect_err("a self-file mod row still expands its pub items");
    assert!(
        error.contains("Unrowed") && error.contains("new_public_fn"),
        "{error}"
    );
    // A NEW pub item added to the module goes red without its own row.
    std::fs::write(
        &facade,
        "pub struct Published;\npub struct Unrowed;\npub fn new_public_fn() {}\npub fn blanket_bypass() {}\nimpl Published {\n    pub fn method(&self) {}\n}\n",
    )
    .expect("facade");
    let error = expanded_mod_rows_violation(dir.path(), &rows, &[]).expect_err("new pub item");
    assert!(error.contains("blanket_bypass"), "{error}");
    // An AUTHORITY row also covers an item.
    let authority = vec![AuthRow {
        crate_name: "neovm-value".to_owned(),
        item: "blanket_bypass".to_owned(),
        kind: "fn".to_owned(),
        defining_file: "crates/neovm-value/src/gc/facade.rs".to_owned(),
        fence_path: "neovm_value::test_support::authority::blanket_bypass".to_owned(),
    }];
    assert!(expanded_mod_rows_violation(dir.path(), &rows, &authority).is_ok());
    // A test-gated inline module row is inert: no expansion, no failure.
    let inert = VisRow {
        crate_dir: "neovm-value".to_owned(),
        item: "gc::inert".to_owned(),
        kind: "mod".to_owned(),
        file: "crates/neovm-value/src/gc.rs".to_owned(),
        tests: Vec::new(),
    };
    std::fs::write(
        src.join("gc.rs"),
        format!(
            "{gc}#[cfg(test)]\n#[cfg_attr(feature = \"test-support\", visibility::make(pub))]\nmod inert {{ pub fn test_only() {{}} }}\n"
        ),
    )
    .expect("gc");
    assert!(expanded_mod_rows_violation(dir.path(), &[inert], &[]).is_ok());
}

/// Peer 15:0xZ alias case: `pub use module::Item as Alias` makes `Item`
/// root-public in production, so a mod row widening `module` does not expose
/// a new item and needs no extra row. An unaliased pub item stays red, and an
/// alias confined to a private module is not a root-public path.
#[test]
fn mod_row_expansion_accepts_an_aliased_root_public_item() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("crates/neovm-value/src");
    std::fs::create_dir_all(src.join("intern")).expect("intern dir");
    std::fs::create_dir_all(src.join("hidden")).expect("hidden dir");
    std::fs::write(src.join("lib.rs"), "pub mod intern;\nmod hidden;\n").expect("lib");
    let intern = "pub use well_known::{FIRST_ID as FIRST_WELL_KNOWN_SYM_ID};\n\
                  #[cfg(not(feature = \"test-support\"))]\nmod well_known;\n\
                  #[cfg(feature = \"test-support\")]\npub mod well_known;\n";
    std::fs::write(src.join("intern/mod.rs"), intern).expect("intern mod");
    std::fs::write(
        src.join("intern/well_known.rs"),
        "pub const FIRST_ID: u32 = 3;\npub const UNALIASED: u32 = 4;\npub const HIDDEN_ALIASED: u32 = 5;\n",
    )
    .expect("well_known");
    // Re-exported only inside a private module: not a production-public path.
    std::fs::write(
        src.join("hidden/mod.rs"),
        "pub use crate::intern::well_known::HIDDEN_ALIASED as HIDDEN_ALIAS;\n",
    )
    .expect("hidden mod");
    let mod_row = || VisRow {
        crate_dir: "neovm-value".to_owned(),
        item: "intern::well_known".to_owned(),
        kind: "mod".to_owned(),
        file: "crates/neovm-value/src/intern/mod.rs".to_owned(),
        tests: Vec::new(),
    };
    let item_row = |item: &str| VisRow {
        crate_dir: "neovm-value".to_owned(),
        item: item.to_owned(),
        kind: "const".to_owned(),
        file: "crates/neovm-value/src/intern/well_known.rs".to_owned(),
        tests: Vec::new(),
    };
    let error = expanded_mod_rows_violation(dir.path(), &[mod_row()], &[])
        .expect_err("unaliased and privately aliased items");
    assert!(
        !error.contains("FIRST_ID"),
        "aliased root-public item must be accepted: {error}"
    );
    assert!(error.contains("UNALIASED"), "{error}");
    assert!(error.contains("HIDDEN_ALIASED"), "{error}");
    // Rows for the two unaliased items close the expansion.
    let rows = vec![
        mod_row(),
        item_row("intern::well_known::UNALIASED"),
        item_row("intern::well_known::HIDDEN_ALIASED"),
    ];
    assert!(expanded_mod_rows_violation(dir.path(), &rows, &[]).is_ok());
}

/// ARM64-SPLIT: the root may reach the guard as a symlink.  macOS `/tmp` and
/// `/var` are links to `/private/...`, so `tempfile::tempdir()` hands these
/// self-tests a non-canonical root while every source path is canonicalized;
/// the expansion's `strip_prefix(workspace)` then failed with "is outside the
/// workspace" on arm64 only.  A symlinked root must expand exactly like the
/// canonical one, and must still report an unrowed pub item.
#[cfg(unix)]
#[test]
fn mod_row_expansion_accepts_a_symlinked_root_workspace() {
    let real = tempfile::tempdir().expect("real tempdir");
    let alias_parent = tempfile::tempdir().expect("alias parent");
    let alias = alias_parent.path().join("workspace-alias");
    std::os::unix::fs::symlink(real.path(), &alias).expect("symlink root");
    let src = real.path().join("crates/neovm-value/src");
    std::fs::create_dir_all(src.join("gc")).expect("src");
    std::fs::write(src.join("lib.rs"), "pub mod gc;\n").expect("lib");
    let gc = "pub use facade::Published;\n\
              #[cfg(not(feature = \"test-support\"))]\nmod facade;\n\
              #[cfg(feature = \"test-support\")]\npub mod facade;\n";
    std::fs::write(src.join("gc.rs"), gc).expect("gc");
    std::fs::write(
        src.join("gc/facade.rs"),
        "pub struct Published;\npub struct Unrowed;\npub fn new_public_fn() {}\nimpl Published {\n    pub fn method(&self) {}\n}\n",
    )
    .expect("facade");
    let mod_row = || VisRow {
        crate_dir: "neovm-value".to_owned(),
        item: "gc::facade".to_owned(),
        kind: "mod".to_owned(),
        file: "crates/neovm-value/src/gc.rs".to_owned(),
        tests: Vec::new(),
    };
    let item_row = |item: &str, kind: &str| VisRow {
        crate_dir: "neovm-value".to_owned(),
        item: item.to_owned(),
        kind: kind.to_owned(),
        file: "crates/neovm-value/src/gc/facade.rs".to_owned(),
        tests: Vec::new(),
    };
    let rows = vec![
        mod_row(),
        item_row("gc::facade::Unrowed", "struct"),
        item_row("gc::facade::new_public_fn", "fn"),
    ];
    assert!(
        expanded_mod_rows_violation(&alias, &rows, &[]).is_ok(),
        "a symlinked root must expand like the canonical one"
    );
    let error = expanded_mod_rows_violation(&alias, &[mod_row()], &[])
        .expect_err("an unrowed pub item must stay red through a symlinked root");
    assert!(
        error.contains("Unrowed") && error.contains("new_public_fn"),
        "{error}"
    );
    assert!(expanded_mod_rows_violation(real.path(), &rows, &[]).is_ok());
}

#[test]
fn visibility_make_multiline_use_group_is_named_by_its_first_import() {
    // A multi-line `pub(crate) use module::{ ... }` group must not be
    // attributed to whatever declaration follows the group.
    let lines = [
        "    #[cfg_attr(feature = \"test-support\", visibility::make(pub))]",
        "    pub(crate) use module::{",
        "        FirstTrait, SecondTrait,",
        "    };",
        "    pub(super) mod next_item;",
    ];
    assert_eq!(declaration_after(&lines, 0), Some("FirstTrait".to_owned()));
    // The single-line convention is unchanged: the first imported name.
    let lines = [
        "#[cfg_attr(feature = \"test-support\", visibility::make(pub))]",
        "pub(crate) use module::{First, Second};",
    ];
    assert_eq!(declaration_after(&lines, 0), Some("First".to_owned()));
}

/// A comment that MENTIONS the attribute text is not a widening site; a real
/// attribute on the next declaration still is.
#[test]
fn visibility_make_in_a_comment_is_not_a_widening_site() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("crates/neovm-value/src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::write(
        src.join("commented.rs"),
        "// the chain applies visibility::make(pub) at the fence.\n\
         /* a block comment mentioning visibility::make(pub) */\n\
         /// doc: visibility::make(pub)\n\
         pub(crate) struct NotWidened;\n",
    )
    .expect("commented");
    assert!(visibility_make_sites(dir.path()).expect("sites").is_empty());
    std::fs::write(
        src.join("widened.rs"),
        "#[cfg_attr(feature = \"test-support\", visibility::make(pub))]\n\
         pub(crate) struct Widened;\n",
    )
    .expect("widened");
    let sites = visibility_make_sites(dir.path()).expect("sites");
    assert_eq!(
        sites,
        vec![(
            "crates/neovm-value/src/widened.rs".to_owned(),
            "Widened".to_owned()
        )]
    );
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
                // Peer #828 / lead #809: the original-path rows for the
                // body-classified test-support surface include trait writers
                // and pub-field writers; the KINDS table already carries both.
                "fn" | "unsafe fn"
                    | "method"
                    | "type"
                    | "struct"
                    | "enum"
                    | "const"
                    | "static"
                    | "mod"
                    | "trait"
                    | "field"
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
        // Peer r2 R4: the tests cell is a comma-separated list of REAL
        // workspace paths.  A placeholder or a moved/stale path makes the
        // row's coverage claim unverifiable.
        for cell in tests
            .split(',')
            .map(str::trim)
            .filter(|cell| !cell.is_empty())
        {
            assert!(
                workspace.join(cell).is_file(),
                "AUTHORITY-ITEMS.tsv line {}: tests file missing: {cell}",
                index + 2
            );
        }
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

// ---------------------------------------------------------------------------
// LEAD 10:2xZ: every AUTHORITY item's public path set is EXACTLY {fence_path}.
/// The fence is the sole public route. This check fails on:
///   1. a public `use` (including a glob) outside the fence definition that
///      re-exports the fence item, the fence module, or one of its ancestors;
///   2. an item defined outside the fence whose ORIGINAL path is publicly
///      reachable (every module on the chain is public and the item is pub).
/// Production `src/**` only; test trees and cfg(test)/#[test] items are not
/// public surface. Resolution covers the fence crate name (and `use ... as`
/// aliases of it) plus `crate`/`self`/`super` inside the fence crate itself.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct AuthRow {
    crate_name: String,
    item: String,
    kind: String,
    defining_file: String,
    fence_path: String,
}

fn auth_rows(workspace: &Path) -> Vec<AuthRow> {
    let path = workspace.join(ARCHITECTURE_DIR).join("AUTHORITY-ITEMS.tsv");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate().skip(1) {
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
        rows.push(AuthRow {
            crate_name: fields[0].to_owned(),
            item: fields[1].to_owned(),
            kind: fields[2].to_owned(),
            defining_file: fields[3].to_owned(),
            fence_path: fields[4].to_owned(),
        });
    }
    rows
}

fn auth_leaf(item: &str) -> &str {
    item.rsplit("::").next().unwrap_or(item)
}

fn auth_expected(row: &AuthRow) -> String {
    let leaf = auth_leaf(&row.item);
    if row.fence_path.ends_with(&format!("::{leaf}")) || row.fence_path == leaf {
        row.fence_path.clone()
    } else {
        format!("{}::{leaf}", row.fence_path)
    }
}

/// The AUTHORITY public-path model for one fence crate.
enum AuthFence {
    /// A signed `test_support::authority` module: rows may name it, and the
    /// module is scanned for second routes.
    Module {
        file: &'static str,
        segments: &'static [&'static str],
    },
    /// A crate with no signed fence module. Every AUTHORITY row is an
    /// original-path row (LEAD 02:2xZ / peer 02:19Z): the signed fence_path is
    /// the item's own public module path, so the row is validated against that
    /// module and there is no fence to scan for a second route.
    OriginalPathOnly,
}

/// The signed fence definition file and module path for a fence crate,
/// relative to the crate root (the eval authority module is inline).
fn auth_fence_file(crate_name: &str) -> Option<AuthFence> {
    match crate_name {
        "neovm-value" => Some(AuthFence::Module {
            file: "crates/neovm-value/src/test_support/authority.rs",
            segments: &["test_support", "authority"],
        }),
        "neovm-eval" => Some(AuthFence::Module {
            file: "crates/neovm-eval/src/test_support.rs",
            segments: &["test_support", "authority"],
        }),
        "neovm-edit" => Some(AuthFence::Module {
            file: "crates/neovm-edit/src/test_support/authority.rs",
            segments: &["test_support", "authority"],
        }),
        // The display fence is its signed test_support::authority module
        // (peer 19:36Z follow-up): the three DisplayFrameExt test fixtures
        // route through delegates there.
        "neovm-display" => Some(AuthFence::Module {
            file: "crates/neovm-display/src/test_support/authority.rs",
            segments: &["test_support", "authority"],
        }),
        // The text fence serves the moved regex perf probes (LEAD 18:4xZ):
        // the scoped syntax-fusion TLS switch and the debug match_stats
        // counter reset route through delegates here; originals stay
        // pub(crate).
        "neovm-text" => Some(AuthFence::Module {
            file: "crates/neovm-text/src/test_support/authority.rs",
            segments: &["test_support", "authority"],
        }),
        // LEAD 18:1xZ: the callproc TLS reset now routes through the signed
        // system fence. Existing system rows remain original-path rows and are
        // still validated by the OriginalPath branch; the fence must not
        // declare their leaves as a second route.
        "neovm-system" => Some(AuthFence::Module {
            file: "crates/neovm-system/src/test_support/authority.rs",
            segments: &["test_support", "authority"],
        }),
        _ => None,
    }
}

fn auth_path_attr(attrs: &[syn::Attribute]) -> Option<PathBuf> {
    for attr in attrs {
        if !attr.path().is_ident("path") {
            continue;
        }
        if let syn::Meta::NameValue(value) = &attr.meta {
            if let syn::Expr::Lit(expr) = &value.value {
                if let syn::Lit::Str(text) = &expr.lit {
                    return Some(PathBuf::from(text.value()));
                }
            }
        }
    }
    None
}

fn auth_module_dir(file: &Path) -> PathBuf {
    let parent = file.parent().unwrap_or_else(|| Path::new(""));
    match file.file_name().and_then(|name| name.to_str()) {
        Some("lib.rs") | Some("main.rs") | Some("mod.rs") => parent.to_path_buf(),
        Some(name) => parent.join(name.strip_suffix(".rs").unwrap_or(name)),
        None => parent.to_path_buf(),
    }
}

fn auth_module_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let flat = dir.join(format!("{name}.rs"));
    if flat.is_file() {
        return Some(flat);
    }
    let nested = dir.join(name).join("mod.rs");
    if nested.is_file() {
        return Some(nested);
    }
    None
}

/// File -> (module segments relative to the crate root, every module on the
/// chain is `pub`). Inline modules do not overwrite their file's own entry.
fn auth_collect_modules(
    file: &Path,
    segments: Vec<String>,
    public: bool,
    map: &mut BTreeMap<PathBuf, (Vec<String>, bool)>,
) -> Result<(), String> {
    let canonical = file
        .canonicalize()
        .map_err(|error| format!("{}: {error}", file.display()))?;
    let text = read(&canonical)?;
    let ast = syn::parse_file(&text)
        .map_err(|error| format!("AUTHORITY_PATHS {}: {error}", canonical.display()))?;
    map.entry(canonical.clone())
        .or_insert((segments.clone(), public));
    fn walk(
        items: &[syn::Item],
        file: &Path,
        segments: &[String],
        public: bool,
        map: &mut BTreeMap<PathBuf, (Vec<String>, bool)>,
    ) -> Result<(), String> {
        for item in items {
            let syn::Item::Mod(module) = item else {
                continue;
            };
            // Test-support build view: skip arms that are not compiled with
            // test = false / feature = "test-support", and count a
            // `visibility::make(pub)` widening as a public module.
            if auth_cfg_inactive(&module.attrs) {
                continue;
            }
            let mut child = segments.to_vec();
            child.push(module.ident.to_string());
            let widened = auth_test_support_widened(&module.attrs);
            let child_public =
                public && (matches!(module.vis, syn::Visibility::Public(_)) || widened);
            if let Some((_, inner)) = &module.content {
                walk(inner, file, &child, child_public, map)?;
            } else {
                let dir = auth_module_dir(file);
                let target = match auth_path_attr(&module.attrs) {
                    Some(path) => file.parent().unwrap_or_else(|| Path::new("")).join(path),
                    None => match auth_module_file(&dir, &module.ident.to_string()) {
                        Some(target) => target,
                        None => continue,
                    },
                };
                if target.is_file() {
                    auth_collect_modules(&target, child, child_public, map)?;
                }
            }
        }
        Ok(())
    }
    walk(&ast.items, &canonical, &segments, public, map)?;
    Ok(())
}

fn auth_use_leaves(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    out: &mut Vec<(Vec<String>, Option<String>, bool)>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            auth_use_leaves(&path.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(name) => {
            let mut path = prefix.clone();
            path.push(name.ident.to_string());
            out.push((path, None, false));
        }
        syn::UseTree::Rename(rename) => {
            let mut path = prefix.clone();
            path.push(rename.ident.to_string());
            out.push((path, Some(rename.rename.to_string()), false));
        }
        syn::UseTree::Glob(_) => out.push((prefix.clone(), None, true)),
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                auth_use_leaves(tree, prefix, out);
            }
        }
    }
}

fn auth_test_gated(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("cfg")
                && attr
                    .meta
                    .require_list()
                    .map(|list| {
                        list.tokens.clone().into_iter().any(
                            |token| matches!(token, proc_macro2::TokenTree::Ident(id) if id == "test"),
                        )
                    })
                    .unwrap_or(false))
    })
}

/// True when the declaration is not compiled in the test-support build
/// (test = false, `feature = "test-support"` = true). Peer 15:4xZ: the
/// AUTHORITY-PATHS reachability walk must use the TEST-SUPPORT build, so that
/// a test-support mod-row route is visible as a public path.
fn auth_cfg_inactive(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Meta>()
                .map(|meta| cfg_truth_test_support(&meta) == CfgTruth::False)
                .unwrap_or(false)
    })
}

/// True when `#[cfg_attr(feature = "test-support", visibility::make(pub))]`
/// widens the declaration in the test-support build (a VIS mod-row widening).
fn auth_test_support_widened(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg_attr")
            && attr
                .meta
                .require_list()
                .map(|list| {
                    let text = list.tokens.to_string();
                    text.contains("test-support")
                        && text.contains("visibility")
                        && text.contains("make")
                        && text.contains("pub")
                })
                .unwrap_or(false)
    })
}

/// True when the file (at any inline depth) declares or re-exports `leaf`
/// publicly. The fence file is the one place where the item must be public.
fn auth_items_declare_public(
    items: &[syn::Item],
    leaf: &str,
    impl_owner: Option<&str>,
    trait_self_name: bool,
) -> bool {
    // A row whose item names an owner (`Owner::method`) is a method row:
    // only an inherent `pub fn method` on that owner (the impl arm below)
    // or the separate alias route may declare it. A free fn/struct/const
    // or a `pub use` leaf that merely shares the method's name is NOT the
    // signed path (peer 00:36:00Z follow-up; predates c6314a9304).
    let leaf_arms = impl_owner.is_none();
    items.iter().any(|item| {
            let public = match item {
                syn::Item::Fn(item) => {
                    leaf_arms
                        && item.sig.ident == leaf
                        && matches!(item.vis, syn::Visibility::Public(_))
                }
                syn::Item::Type(item) => {
                    leaf_arms
                        && item.ident == leaf
                        && matches!(item.vis, syn::Visibility::Public(_))
                }
                syn::Item::Struct(item) => {
                    leaf_arms
                        && item.ident == leaf
                        && matches!(item.vis, syn::Visibility::Public(_))
                }
                syn::Item::Enum(item) => {
                    leaf_arms
                        && item.ident == leaf
                        && matches!(item.vis, syn::Visibility::Public(_))
                }
                // Trait methods carry no `pub` keyword; a method of a public
                // trait is a public path (the display fence's shape). The
                // self-name arm is the trait-item original-path shape and is
                // keyed to kind=trait (peer r2 R3): a fn/field/method row
                // must not be satisfied by a same-named trait.
                syn::Item::Trait(item) => {
                    leaf_arms
                        && matches!(item.vis, syn::Visibility::Public(_))
                        && ((trait_self_name && item.ident == leaf)
                            || item.items.iter().any(|member| {
                                matches!(member, syn::TraitItem::Fn(function) if function.sig.ident == leaf)
                            }))
                }
                syn::Item::Const(item) => {
                    leaf_arms
                        && item.ident == leaf
                        && matches!(item.vis, syn::Visibility::Public(_))
                }
                syn::Item::Static(item) => {
                    leaf_arms
                        && item.ident == leaf
                        && matches!(item.vis, syn::Visibility::Public(_))
                }
                syn::Item::Use(item) => {
                    let mut leaves = Vec::new();
                    auth_use_leaves(&item.tree, &mut Vec::new(), &mut leaves);
                    leaf_arms
                        && matches!(item.vis, syn::Visibility::Public(_))
                        && leaves.iter().any(|(path, alias, glob)| {
                            !glob
                                && (alias.as_deref() == Some(leaf)
                                    || path.last().is_some_and(|name| name == leaf))
                        })
                }
                syn::Item::Mod(module) => {
                    matches!(module.vis, syn::Visibility::Public(_))
                        && module
                            .content
                            .as_ref()
                            .is_some_and(|(_, inner)| {
                                auth_items_declare_public(inner, leaf, impl_owner, trait_self_name)
                            })
                }
                // Inherent-impl methods on the fence type are a public path
                // only when the method is `pub` AND the impl's self type is
                // exactly the row item's owner segment (peer 23:14:30Z pin;
                // the vm_test exact-tier witness shape: `pub fn selected`
                // inside `#[cfg(feature = "jit")] impl ExactTierWitness`). A
                // same-name method on any other type is not this row's route,
                // so a fence cannot accidentally publish an unrelated type's
                // method under the signed path.
                syn::Item::Impl(item) => impl_owner.is_some_and(|owner| {
                    let self_ty_owner = match &*item.self_ty {
                        syn::Type::Path(path) => path
                            .path
                            .segments
                            .last()
                            .map(|segment| segment.ident.to_string()),
                        _ => None,
                    };
                    self_ty_owner.as_deref() == Some(owner)
                        && item.items.iter().any(|member| {
                            matches!(member, syn::ImplItem::Fn(function)
                                if function.sig.ident == leaf
                                    && matches!(function.vis, syn::Visibility::Public(_)))
                        })
                }),
                _ => false,
            };
            public
        })
}

fn auth_declares_public(file: &Path, leaf: &str, impl_owner: Option<&str>) -> Result<bool, String> {
    let text = read(file)?;
    let ast = syn::parse_file(&text)
        .map_err(|error| format!("AUTHORITY_PATHS {}: {error}", file.display()))?;
    Ok(auth_items_declare_public(
        &ast.items, leaf, impl_owner, false,
    ))
}

/// The original-path declaration check for a row of `kind`: only a
/// kind=trait row may be satisfied by a public trait's own name (peer r2 R3).
fn auth_declares_public_for_kind(
    file: &Path,
    leaf: &str,
    impl_owner: Option<&str>,
    kind: &str,
) -> Result<bool, String> {
    let text = read(file)?;
    let ast = syn::parse_file(&text)
        .map_err(|error| format!("AUTHORITY_PATHS {}: {error}", file.display()))?;
    Ok(auth_items_declare_public(
        &ast.items,
        leaf,
        impl_owner,
        kind == "trait",
    ))
}

/// Resolve a crate-relative module path for the original-path branch. File
/// modules come from `map`; any remaining segments are inline `mod`
/// declarations inside that file (e.g. `pub mod test_support { ... }` in a
/// production module), which have no separate source file.
fn auth_original_module<'a>(
    map: &'a BTreeMap<PathBuf, (Vec<String>, bool)>,
    segments: &'a [String],
) -> Option<(&'a PathBuf, bool, &'a [String])> {
    for prefix_len in (0..=segments.len()).rev() {
        if let Some((file, public)) = auth_module_entry(map, &segments[..prefix_len]) {
            return Some((file, public, &segments[prefix_len..]));
        }
    }
    None
}

/// `(chain_public, declares_public)` for an inline module tail under `file`:
/// each remaining segment must be a cfg-active public (or test-support
/// widened) inline `mod`, and the innermost module's own items must declare
/// `owner` publicly.
fn auth_inline_module_declares_public(
    file: &Path,
    tail: &[String],
    owner: &str,
    kind: &str,
) -> Result<(bool, bool), String> {
    if tail.is_empty() {
        return Ok((
            true,
            auth_declares_public_for_kind(file, owner, None, kind)?,
        ));
    }
    let text = read(file)?;
    let ast = syn::parse_file(&text)
        .map_err(|error| format!("AUTHORITY_PATHS {}: {error}", file.display()))?;
    fn descend(items: &[syn::Item], tail: &[String], owner: &str, kind: &str) -> (bool, bool) {
        let Some((name, rest)) = tail.split_first() else {
            return (true, false);
        };
        for item in items {
            let syn::Item::Mod(module) = item else {
                continue;
            };
            if module.ident != name || auth_cfg_inactive(&module.attrs) {
                continue;
            }
            let public = matches!(module.vis, syn::Visibility::Public(_))
                || auth_test_support_widened(&module.attrs);
            if !public {
                return (false, false);
            }
            let Some((_, inner)) = &module.content else {
                return (false, false);
            };
            if rest.is_empty() {
                return (
                    true,
                    auth_items_declare_public(inner, owner, None, kind == "trait"),
                );
            }
            return descend(inner, rest, owner, kind);
        }
        (false, false)
    }
    Ok(descend(&ast.items, tail, owner, kind))
}

/// Resolve a Rust path written in `from_module` to (module segments, item
/// leaf). Handles `crate`/`neovm_value` roots, `self` and leading `super`.
fn auth_resolve_path(from_module: &[String], path: &[String]) -> Option<(Vec<String>, String)> {
    let (first, rest) = path.split_first()?;
    let mut base = match first.as_str() {
        "crate" | "neovm_value" => Vec::new(),
        "self" => from_module.to_vec(),
        "super" => {
            let mut base = from_module.to_vec();
            base.pop()?;
            base
        }
        _ => from_module.to_vec(),
    };
    let mut index = 0;
    while index < rest.len() && rest[index] == "super" {
        base.pop()?;
        index += 1;
    }
    let item = rest.last()?.to_string();
    let mut module = base;
    module.extend(rest[..rest.len() - 1].iter().cloned());
    Some((module, item))
}

/// Every crate-local name by which a fence type alias target is known along
/// alias/`pub use` chains, e.g. fence `ProofEra` -> tagged::gc `ProofEra` ->
/// wrappers `ProofEra` / `GenericProofEra` / evaluator `WorldProofEra`.
/// The signed defining file's inherent-impl owner must be one of these names.
fn auth_alias_chain_names(
    workspace: &Path,
    map: &BTreeMap<PathBuf, (Vec<String>, bool)>,
    from_module: &[String],
    ty: &syn::Type,
) -> Result<BTreeSet<String>, String> {
    fn walk(
        workspace: &Path,
        map: &BTreeMap<PathBuf, (Vec<String>, bool)>,
        module: &[String],
        name: &str,
        names: &mut BTreeSet<String>,
        seen: &mut BTreeSet<(String, String)>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 16 {
            return Err(format!(
                "AUTHORITY_PATHS alias target chain is too deep at {name}"
            ));
        }
        if !seen.insert((module.join("::"), name.to_string())) {
            return Ok(());
        }
        names.insert(name.to_string());
        let Some((file, _)) = auth_module_entry(map, module) else {
            return Ok(());
        };
        let text = read(file)?;
        let ast = syn::parse_file(&text)
            .map_err(|error| format!("AUTHORITY_PATHS {}: {error}", file.display()))?;
        for item in &ast.items {
            match item {
                syn::Item::Struct(item) if item.ident == name => return Ok(()),
                syn::Item::Enum(item) if item.ident == name => return Ok(()),
                syn::Item::Union(item) if item.ident == name => return Ok(()),
                syn::Item::Type(item) if item.ident == name => {
                    if let syn::Type::Path(path) = &*item.ty {
                        if path.qself.is_none() {
                            let segments: Vec<String> = path
                                .path
                                .segments
                                .iter()
                                .map(|segment| segment.ident.to_string())
                                .collect();
                            if let Some((target_module, target)) =
                                auth_resolve_path(module, &segments)
                            {
                                walk(
                                    workspace,
                                    map,
                                    &target_module,
                                    &target,
                                    names,
                                    seen,
                                    depth + 1,
                                )?;
                            }
                        }
                    }
                    return Ok(());
                }
                syn::Item::Use(item) => {
                    let mut leaves = Vec::new();
                    auth_use_leaves(&item.tree, &mut Vec::new(), &mut leaves);
                    for (path, alias, glob) in leaves {
                        if glob {
                            continue;
                        }
                        let matches = alias.as_deref() == Some(name)
                            || (alias.is_none() && path.last().is_some_and(|leaf| leaf == name));
                        if !matches {
                            continue;
                        }
                        if let Some((target_module, target)) = auth_resolve_path(module, &path) {
                            walk(
                                workspace,
                                map,
                                &target_module,
                                &target,
                                names,
                                seen,
                                depth + 1,
                            )?;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    let mut names = BTreeSet::new();
    let mut seen = BTreeSet::new();
    if let syn::Type::Path(path) = ty {
        if path.qself.is_none() {
            let segments: Vec<String> = path
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            if let Some((module, name)) = auth_resolve_path(from_module, &segments) {
                walk(workspace, map, &module, &name, &mut names, &mut seen, 0)?;
            }
        }
    }
    Ok(names)
}

/// Method-aware fence rule (gapsurvey 2026-11-05): a signed row whose
/// fence_path traverses a fence-declared type alias names a method, not a
/// literal item in the fence file. Such a row is valid when the fence
/// declares the owner as a PUBLIC type alias AND the signed defining file
/// exposes the method as a `pub` inherent method on the alias's target type
/// name -- resolved through the crate's alias and `pub use` chains, so an
/// alias to an alias to the concrete type still matches. An undeclared or
/// private alias, an unresolvable target name, or a non-public method keeps
/// the row red.
fn auth_alias_method_is_fenced(
    workspace: &Path,
    fence: &Path,
    fence_module: &[String],
    map: &BTreeMap<PathBuf, (Vec<String>, bool)>,
    defining_file: &str,
    item: &str,
) -> Result<bool, String> {
    let Some((owner, method)) = item.rsplit_once("::") else {
        return Ok(false);
    };
    let fence_text = read(fence)?;
    let fence_ast = syn::parse_file(&fence_text)
        .map_err(|error| format!("AUTHORITY_PATHS {}: {error}", fence.display()))?;
    let mut targets = BTreeSet::new();
    let mut routes = 0usize;
    // The owner may be declared by a fence type alias OR re-exported by a
    // fence `pub use` whose target module is private (the REGWAIT helpers).
    // Exactly one fence route may expose it; a second route breaks the signed
    // "exactly this path" property.
    for item in &fence_ast.items {
        match item {
            syn::Item::Type(alias)
                if alias.ident == owner && matches!(alias.vis, syn::Visibility::Public(_)) =>
            {
                routes += 1;
                targets.extend(auth_alias_chain_names(
                    workspace,
                    map,
                    fence_module,
                    &alias.ty,
                )?);
            }
            syn::Item::Use(use_item) if matches!(use_item.vis, syn::Visibility::Public(_)) => {
                let mut leaves = Vec::new();
                auth_use_leaves(&use_item.tree, &mut Vec::new(), &mut leaves);
                for (path, alias, glob) in leaves {
                    if glob {
                        continue;
                    }
                    let exposed = alias.as_deref() == Some(owner)
                        || (alias.is_none() && path.last().is_some_and(|leaf| leaf == owner));
                    if !exposed {
                        continue;
                    }
                    routes += 1;
                    if let Some(target) = path.last() {
                        targets.insert(target.clone());
                    }
                }
            }
            _ => {}
        }
    }
    if routes == 0 {
        return Ok(false);
    }
    if routes > 1 {
        return Err(format!(
            "AUTHORITY_PATHS {owner}::{method}: {routes} fence routes expose the owner; the signed fence path must be unique"
        ));
    }
    if targets.is_empty() {
        return Ok(false);
    }
    let targets: Vec<String> = targets.into_iter().collect();
    let defining = workspace.join(defining_file);
    auth_file_pub_inherent_method(&defining, &targets, method)
}

/// True when `file` declares `pub fn method` in an inherent impl whose
/// self-type is named by `targets` (any alias-chain spelling).
fn auth_file_pub_inherent_method(
    file: &Path,
    targets: &[String],
    method: &str,
) -> Result<bool, String> {
    let text = read(file)?;
    let ast = syn::parse_file(&text)
        .map_err(|error| format!("AUTHORITY_PATHS {}: {error}", file.display()))?;
    fn public_method(items: &[syn::Item], targets: &[String], method: &str) -> bool {
        items.iter().any(|item| match item {
            syn::Item::Impl(block) if block.trait_.is_none() => {
                let owner = match &*block.self_ty {
                    syn::Type::Path(path) => path
                        .path
                        .segments
                        .last()
                        .map(|segment| segment.ident.to_string()),
                    _ => None,
                };
                owner.is_some_and(|owner| targets.contains(&owner))
                    && block.items.iter().any(|item| {
                        matches!(
                            item,
                            syn::ImplItem::Fn(function)
                                if function.sig.ident == method
                                    && matches!(function.vis, syn::Visibility::Public(_))
                        )
                    })
            }
            syn::Item::Mod(module) => module
                .content
                .as_ref()
                .is_some_and(|(_, inner)| public_method(inner, targets, method)),
            _ => false,
        })
    }
    Ok(public_method(&ast.items, targets, method))
}

/// The source file for a crate-relative module path and whether the module
/// chain is fully public.
fn auth_module_entry<'a>(
    map: &'a BTreeMap<PathBuf, (Vec<String>, bool)>,
    segments: &[String],
) -> Option<(&'a PathBuf, bool)> {
    map.iter()
        .find(|(_, (path, _))| path.as_slice() == segments)
        .map(|(file, (_, public))| (file, *public))
}

fn auth_public_use_violation(
    workspace: &Path,
    rows: &[AuthRow],
    fence_maps: &BTreeMap<String, BTreeMap<PathBuf, (Vec<String>, bool)>>,
) -> Result<(), String> {
    let fences: BTreeMap<String, (Vec<String>, PathBuf)> = rows
        .iter()
        .filter_map(|row| {
            match auth_fence_file(&row.crate_name)
                .unwrap_or_else(|| panic!("AUTHORITY_PATHS unknown fence crate {}", row.crate_name))
            {
                AuthFence::Module { file, segments } => Some((
                    row.crate_name.clone(),
                    (
                        segments.iter().map(|segment| segment.to_string()).collect(),
                        workspace.join(file),
                    ),
                )),
                // Original-path-only crates have no fence to protect; the
                // defining-module second-route check is done per row above.
                AuthFence::OriginalPathOnly => None,
            }
        })
        .collect();
    for crate_name in [
        "neovm-value",
        "neovm-edit",
        "neovm-text",
        "neovm-eval",
        "neovm-display",
        "neovm-system",
        "neovm-core",
    ] {
        let src = workspace.join("crates").join(crate_name).join("src");
        if !src.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        walk_files(&src, &mut files)?;
        let map = fence_maps.get(crate_name);
        for path in files {
            if path.extension().is_none_or(|extension| extension != "rs") || is_test_path(&path) {
                continue;
            }
            let canonical = path
                .canonicalize()
                .map_err(|error| format!("AUTHORITY_PATHS {}: {error}", path.display()))?;
            let text = read(&canonical)?;
            let ast = syn::parse_file(&text)
                .map_err(|error| format!("AUTHORITY_PATHS {}: {error}", canonical.display()))?;
            let file_module = map
                .and_then(|map| map.get(&canonical))
                .map(|(segments, _)| segments.clone())
                .unwrap_or_default();
            let mut aliases: BTreeMap<String, String> = BTreeMap::new();
            for item in &ast.items {
                let syn::Item::Use(item) = item else { continue };
                let mut leaves = Vec::new();
                auth_use_leaves(&item.tree, &mut Vec::new(), &mut leaves);
                for (path, alias, glob) in leaves {
                    if glob || alias.is_none() || path.len() != 1 {
                        continue;
                    }
                    let Some(fence) = fences.keys().find(|name| name.replace('-', "_") == path[0])
                    else {
                        continue;
                    };
                    aliases.insert(alias.unwrap(), fence.clone());
                }
            }
            fn scan(
                items: &[syn::Item],
                rows: &[AuthRow],
                crate_name: &str,
                file_module: &[String],
                aliases: &BTreeMap<String, String>,
                fences: &BTreeMap<String, (Vec<String>, PathBuf)>,
                canonical: &Path,
                relative: &str,
                file_reachable: bool,
            ) -> Result<(), String> {
                for item in items {
                    match item {
                        syn::Item::Mod(module) => {
                            if auth_cfg_inactive(&module.attrs) {
                                continue;
                            }
                            if let Some((_, inner)) = &module.content {
                                scan(
                                    inner,
                                    rows,
                                    crate_name,
                                    file_module,
                                    aliases,
                                    fences,
                                    canonical,
                                    relative,
                                    file_reachable,
                                )?;
                            }
                        }
                        syn::Item::Use(item) => {
                            if !matches!(item.vis, syn::Visibility::Public(_))
                                || auth_cfg_inactive(&item.attrs)
                            {
                                continue;
                            }
                            let mut leaves = Vec::new();
                            auth_use_leaves(&item.tree, &mut Vec::new(), &mut leaves);
                            for (path, _alias, glob) in leaves {
                                let Some(first) = path.first() else { continue };
                                let mut fence_name: Option<String> = None;
                                let mut rest: Vec<String> = Vec::new();
                                if let Some(fence) =
                                    fences.keys().find(|name| name.replace('-', "_") == *first)
                                {
                                    fence_name = Some(fence.clone());
                                    rest = path[1..].to_vec();
                                } else if let Some(fence) = aliases.get(first) {
                                    fence_name = Some(fence.clone());
                                    rest = path[1..].to_vec();
                                } else if matches!(first.as_str(), "crate" | "self" | "super") {
                                    let Some(fence) =
                                        fences.keys().find(|name| name.as_str() == crate_name)
                                    else {
                                        continue;
                                    };
                                    let mut base = file_module.to_vec();
                                    let mut index = 0;
                                    while index < path.len()
                                        && matches!(
                                            path[index].as_str(),
                                            "super" | "self" | "crate"
                                        )
                                    {
                                        match path[index].as_str() {
                                            "crate" => base.clear(),
                                            "super" => {
                                                base.pop();
                                            }
                                            _ => {}
                                        }
                                        index += 1;
                                    }
                                    base.extend_from_slice(&path[index..]);
                                    fence_name = Some(fence.clone());
                                    rest = base;
                                }
                                let Some(fence_name) = fence_name else {
                                    continue;
                                };
                                let Some((fence_module, fence_file)) = fences.get(&fence_name)
                                else {
                                    continue;
                                };
                                if canonical == fence_file.as_path() {
                                    continue;
                                }
                                // A named import re-exports exactly `rest` (the
                                // imported module or item); a glob re-exports
                                // the contents of the module `rest`.
                                let exposes_module = fence_module.starts_with(&rest);
                                let exposes_item = !glob
                                    && rest.len() == fence_module.len() + 1
                                    && rest.starts_with(fence_module)
                                    && rest.last().is_some_and(|leaf| {
                                        rows.iter().any(|row| {
                                            row.crate_name == fence_name
                                                && auth_leaf(&row.item) == leaf.as_str()
                                        })
                                    });
                                if exposes_module || exposes_item {
                                    // Peer 15:4xZ: only a route that is
                                    // actually reachable in the test-support
                                    // build is a second path. A re-export in a
                                    // module that stays private in that build
                                    // is not a public route.
                                    if !file_reachable {
                                        continue;
                                    }
                                    return Err(format!(
                                        "B3_AUTHORITY second route: AUTHORITY_PATHS {relative}: public use `{}` adds a route to the {fence_name} fence beyond its signed fence_path",
                                        std::iter::once(first.clone())
                                            .chain(rest.iter().cloned())
                                            .chain(if glob { Some("*".to_owned()) } else { None })
                                            .collect::<Vec<_>>()
                                            .join("::")
                                    ));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(())
            }
            let relative = path
                .strip_prefix(workspace)
                .map_err(|_| format!("{} is outside the workspace", path.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            // Reachability in the TEST-SUPPORT build. When the walk has no
            // entry for the file (crate without AUTHORITY rows), stay
            // fail-closed and treat the route as reachable.
            let file_reachable = map
                .and_then(|map| map.get(&canonical))
                .map(|(_, public)| *public)
                .unwrap_or(true);
            scan(
                &ast.items,
                rows,
                crate_name,
                &file_module,
                &aliases,
                &fences,
                &canonical,
                &relative,
                file_reachable,
            )?;
        }
    }
    Ok(())
}

/// Peer 20:16Z: a method on a PUBLIC trait whose name matches an AUTHORITY
/// row is a second public route to the signed item. Only a method (and trait)
/// gated `#[cfg(test)]` is allowed to carry such a name: `cfg(any(test,
/// feature = "test-support"))` trait methods stay public in the upper
/// test-support build and are rejected.
fn authority_pub_trait_method_violation(workspace: &Path, rows: &[AuthRow]) -> Result<(), String> {
    fn test_only(attrs: &[syn::Attribute]) -> bool {
        attrs.iter().any(|attr| {
            if !attr.path().is_ident("cfg") {
                return false;
            }
            let Ok(list) = attr.meta.require_list() else {
                return false;
            };
            let compact: String = list.tokens.to_string().split_whitespace().collect();
            compact.contains("test")
                && !compact.contains("test-support")
                && !compact.contains("feature")
        })
    }
    // The trait method is a second route only when the row names the SAME
    // trait: collect, per row leaf, the TYPE-like segments of the signed item
    // and fence path (FunctionWriteHooks::prepare_hof_builtin is a different
    // item from ProofEra::prepare_hof_builtin and must not collide by leaf).
    let mut routes: BTreeMap<&str, BTreeMap<&str, BTreeSet<&str>>> = BTreeMap::new();
    for row in rows {
        let leaf = auth_leaf(&row.item);
        let mut owners: BTreeSet<&str> = BTreeSet::new();
        for segment in row
            .item
            .split("::")
            .chain(row.fence_path.split("::"))
            .take_while(|segment| !segment.is_empty())
        {
            if segment
                .chars()
                .next()
                .is_some_and(|first| first.is_ascii_uppercase())
            {
                owners.insert(segment);
            }
        }
        routes
            .entry(row.crate_name.as_str())
            .or_default()
            .entry(leaf)
            .or_default()
            .extend(owners);
    }
    for (crate_name, crate_routes) in routes {
        let src = workspace.join("crates").join(crate_name).join("src");
        if !src.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        walk_files(&src, &mut files)?;
        for path in files {
            if path.extension().is_none_or(|extension| extension != "rs") || is_test_path(&path) {
                continue;
            }
            let canonical = path
                .canonicalize()
                .map_err(|error| format!("AUTHORITY_TRAIT {}: {error}", path.display()))?;
            let text = read(&canonical)?;
            let Ok(ast) = syn::parse_file(&text) else {
                continue;
            };
            for item in &ast.items {
                let syn::Item::Trait(item) = item else {
                    continue;
                };
                if !matches!(item.vis, syn::Visibility::Public(_)) || test_only(&item.attrs) {
                    continue;
                }
                for member in &item.items {
                    let syn::TraitItem::Fn(function) = member else {
                        continue;
                    };
                    if test_only(&function.attrs) {
                        continue;
                    }
                    let leaf = function.sig.ident.to_string();
                    let Some(owners) = crate_routes.get(leaf.as_str()) else {
                        continue;
                    };
                    if !owners.contains(item.ident.to_string().as_str()) {
                        continue;
                    }
                    let relative = canonical
                        .strip_prefix(workspace)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|_| canonical.display().to_string());
                    return Err(format!(
                        "AUTHORITY_PATHS {relative}: pub trait `{}` method `{leaf}` is named like an AUTHORITY row and is not #[cfg(test)]-only; the signed fence path would not be exclusive",
                        item.ident
                    ));
                }
            }
        }
    }
    Ok(())
}

fn authority_public_paths_violation(workspace: &Path, rows: &[AuthRow]) -> Result<(), String> {
    let mut fence_maps: BTreeMap<String, BTreeMap<PathBuf, (Vec<String>, bool)>> = BTreeMap::new();
    for crate_name in rows
        .iter()
        .map(|row| row.crate_name.clone())
        .collect::<BTreeSet<_>>()
    {
        let fence_style = auth_fence_file(&crate_name)
            .unwrap_or_else(|| panic!("AUTHORITY_PATHS unknown fence crate {crate_name}"));
        let lib = workspace
            .join("crates")
            .join(&crate_name)
            .join("src")
            .join("lib.rs");
        let mut map = BTreeMap::new();
        auth_collect_modules(&lib, Vec::new(), true, &mut map)?;
        let (fence, module): (Option<PathBuf>, Vec<String>) = match &fence_style {
            AuthFence::Module { file, segments } => (
                Some(
                    workspace
                        .join(file)
                        .canonicalize()
                        .map_err(|error| format!("AUTHORITY_PATHS {file}: {error}"))?,
                ),
                segments.iter().map(|segment| segment.to_string()).collect(),
            ),
            AuthFence::OriginalPathOnly => (None, Vec::new()),
        };
        for row in rows.iter().filter(|row| row.crate_name == crate_name) {
            let expected = auth_expected(row);
            let leaf = auth_leaf(&row.item);
            let crate_root_name = crate_name.replace('-', "_");
            if !expected.ends_with(&row.item) {
                return Err(format!(
                    "AUTHORITY_PATHS {expected}: row item {:?} is not the signed path tail",
                    row.item
                ));
            }
            let rest: Vec<String> = expected
                .strip_prefix(&format!("{crate_root_name}::"))
                .ok_or_else(|| {
                    format!("AUTHORITY_PATHS {expected}: the path must start at the crate root")
                })?
                .split("::")
                .map(str::to_string)
                .collect();
            let in_fence = match &fence_style {
                AuthFence::Module { .. } => {
                    rest.len() > module.len() && rest[..module.len()] == module[..]
                }
                AuthFence::OriginalPathOnly => false,
            };
            let mut signed_module_file: Option<PathBuf> = None;
            if in_fence {
                let fence = fence
                    .as_ref()
                    .expect("AUTHORITY_PATHS an in-fence row has a fence file");
                let fence_public = map
                    .iter()
                    .find(|(_, (segments, _))| *segments == module)
                    .map(|(_, (_, public))| *public)
                    .or_else(|| map.get(fence).map(|(_, public)| *public))
                    .unwrap_or(false);
                if !fence_public {
                    return Err(format!(
                        "AUTHORITY_PATHS {expected}: the fence module is not publicly reachable"
                    ));
                }
                let fence_owner = {
                    let parts: Vec<&str> = row.item.split("::").collect();
                    parts
                        .len()
                        .checked_sub(2)
                        .and_then(|index| parts.get(index))
                        .copied()
                        .filter(|segment| {
                            segment
                                .chars()
                                .next()
                                .is_some_and(|first| first.is_ascii_uppercase())
                        })
                };
                if !auth_declares_public(fence, leaf, fence_owner)?
                    && !auth_alias_method_is_fenced(
                        &workspace,
                        fence,
                        &module,
                        &map,
                        &row.defining_file,
                        &row.item,
                    )?
                {
                    return Err(format!(
                        "AUTHORITY_PATHS {expected}: the fence does not declare {leaf:?} publicly"
                    ));
                }
            } else {
                // Original-path row: the item is public at its own module path
                // (item-level `#[doc(hidden)] pub`), so the signed path is
                // verified against that module instead of the fence file, and
                // the fence must not expose the item as a second route.
                let item_parts: Vec<&str> = row.item.split("::").collect();
                // A method/associated-function row has a TYPE-like penultimate
                // segment; a lowercase penultimate segment is a module path
                // (e.g. a test-only `module::function` fence).
                let method_row = item_parts.len() >= 2
                    && item_parts[item_parts.len() - 2]
                        .chars()
                        .next()
                        .is_some_and(|first| first.is_ascii_uppercase());
                let owner = if method_row {
                    item_parts[item_parts.len() - 2]
                } else {
                    leaf
                };
                let prefix_len = rest
                    .len()
                    .checked_sub(if method_row { 2 } else { 1 })
                    .ok_or_else(|| {
                        format!("AUTHORITY_PATHS {expected}: the signed path is too short")
                    })?;
                let module_segments = &rest[..prefix_len];
                let Some((module_file, file_public, inline_tail)) =
                    auth_original_module(&map, module_segments)
                else {
                    return Err(format!(
                        "AUTHORITY_PATHS {expected}: no source module for {}",
                        module_segments.join("::")
                    ));
                };
                let (inline_public, mut declares_public) =
                    auth_inline_module_declares_public(module_file, inline_tail, owner, &row.kind)?;
                if !file_public || !inline_public {
                    return Err(format!(
                        "AUTHORITY_PATHS {expected}: the module chain is not publicly reachable"
                    ));
                }
                if !declares_public {
                    return Err(format!(
                        "AUTHORITY_PATHS {expected}: {} does not publicly expose {owner:?}",
                        module_file.display()
                    ));
                }
                signed_module_file = Some(module_file.clone());
                if method_row {
                    // Kind-keyed public route (peer r2 R3): a field row is
                    // satisfied only by a pub named field, every other
                    // method-shaped row only by a pub inherent method.
                    let exposed = if row.kind == "field" {
                        auth_file_pub_field(&workspace.join(&row.defining_file), owner, leaf)?
                    } else {
                        auth_file_pub_inherent_method(
                            &workspace.join(&row.defining_file),
                            &[owner.to_string()],
                            leaf,
                        )?
                    };
                    if !exposed {
                        let shape = if row.kind == "field" {
                            "a pub named field"
                        } else {
                            "a pub inherent method"
                        };
                        return Err(format!(
                            "AUTHORITY_PATHS {expected}: the defining file does not expose {owner}::{leaf} as {shape} (row kind {:?})",
                            row.kind
                        ));
                    }
                }
                if let Some(fence) = &fence {
                    if auth_declares_public(fence, leaf, None)?
                        || (method_row && auth_declares_public(fence, owner, None)?)
                    {
                        return Err(format!(
                            "AUTHORITY_PATHS {expected}: the authority fence also exposes the item, so the signed path is not exclusive"
                        ));
                    }
                }
            }
            let defining = workspace
                .join(&row.defining_file)
                .canonicalize()
                .map_err(|error| format!("AUTHORITY_PATHS {}: {error}", row.defining_file))?;
            let defining_is_signed_module = signed_module_file
                .as_ref()
                .is_some_and(|module_file| defining == *module_file);
            if !defining_is_signed_module && fence.as_ref().is_none_or(|fence| defining != *fence) {
                if let Some((segments, true)) = map.get(&defining) {
                    if auth_declares_public(&defining, leaf, None)? {
                        let mut original = crate_name.replace('-', "_");
                        for segment in segments {
                            original.push_str("::");
                            original.push_str(segment);
                        }
                        original.push_str("::");
                        original.push_str(leaf);
                        // An original-path row's signed path IS the item's
                        // own module path; only a DIFFERENT public path is a
                        // second route.
                        if original != expected {
                            return Err(format!(
                                "AUTHORITY_PATHS {expected}: the defining item is also publicly reachable at {original}"
                            ));
                        }
                    }
                }
            }
        }
        fence_maps.insert(crate_name, map);
    }
    authority_pub_trait_method_violation(workspace, rows)?;
    auth_public_use_violation(workspace, rows, &fence_maps)
}

#[test]
fn authority_rejects_a_pub_trait_method_named_like_a_signed_item() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("crates/neovm-value/src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::write(src.join("lib.rs"), "pub mod window;\n").expect("lib");
    let rows = vec![AuthRow {
        crate_name: "neovm-value".to_owned(),
        item: "window::WindowExt::frobnicate".to_owned(),
        kind: "method".to_owned(),
        defining_file: "crates/neovm-value/src/test_support/authority.rs".to_owned(),
        fence_path: "neovm_value::window::WindowExt::frobnicate".to_owned(),
    }];
    std::fs::write(
        src.join("window.rs"),
        "pub trait WindowExt {\n    fn frobnicate(&self);\n}\n",
    )
    .expect("window");
    let error = authority_pub_trait_method_violation(dir.path(), &rows)
        .expect_err("pub trait method named like a row");
    assert!(
        error.contains("frobnicate") && error.contains("pub trait"),
        "{error}"
    );
    // A #[cfg(test)] method is not a public route.
    std::fs::write(
        src.join("window.rs"),
        "pub trait WindowExt {\n    #[cfg(test)]\n    fn frobnicate(&self);\n}\n",
    )
    .expect("window");
    assert!(authority_pub_trait_method_violation(dir.path(), &rows).is_ok());
    // A cfg(test)-only trait is not public either.
    std::fs::write(
        src.join("window.rs"),
        "#[cfg(test)]\npub trait WindowExt {\n    fn frobnicate(&self);\n}\n",
    )
    .expect("window");
    assert!(authority_pub_trait_method_violation(dir.path(), &rows).is_ok());
    // A test-support-gated trait method is still public in the upper build.
    std::fs::write(
        src.join("window.rs"),
        "pub trait WindowExt {\n    #[cfg(any(test, feature = \"test-support\"))]\n    fn frobnicate(&self);\n}\n",
    )
    .expect("window");
    let error = authority_pub_trait_method_violation(dir.path(), &rows)
        .expect_err("test-support trait method is still public");
    assert!(error.contains("frobnicate"), "{error}");
}

#[test]
fn authority_items_have_exactly_the_fence_public_path() {
    let workspace = workspace_root();
    let rows = auth_rows(&workspace);
    assert!(!rows.is_empty(), "AUTHORITY-ITEMS.tsv has no rows");
    authority_public_paths_violation(&workspace, &rows).unwrap_or_else(|error| panic!("{error}"));
}

#[test]
fn authority_public_paths_mutations() {
    let dir = tempfile::tempdir().expect("tempdir");
    let value_src = dir.path().join("crates/neovm-value/src");
    let core_src = dir.path().join("crates/neovm-core/src");
    std::fs::create_dir_all(value_src.join("test_support")).expect("value src");
    std::fs::create_dir_all(&core_src).expect("core src");
    std::fs::write(
        value_src.join("lib.rs"),
        "pub mod test_support;\npub(crate) mod original;\n",
    )
    .expect("lib");
    std::fs::write(value_src.join("test_support.rs"), "pub mod authority;\n").expect("support");
    std::fs::write(
        value_src.join("test_support/authority.rs"),
        "pub fn mark() { crate::original::mark(); }\n",
    )
    .expect("fence");
    std::fs::write(value_src.join("original.rs"), "pub(crate) fn mark() {}\n").expect("original");
    std::fs::write(core_src.join("lib.rs"), "pub mod route;\n").expect("core lib");
    let rows = vec![AuthRow {
        crate_name: "neovm-value".to_owned(),
        item: "mark".to_owned(),
        kind: "fn".to_owned(),
        defining_file: "crates/neovm-value/src/original.rs".to_owned(),
        fence_path: "neovm_value::test_support::authority::mark".to_owned(),
    }];
    assert!(authority_public_paths_violation(dir.path(), &rows).is_ok());
    let route = core_src.join("route.rs");
    std::fs::write(
        &route,
        "pub use neovm_value::test_support::authority::mark;\n",
    )
    .expect("route");
    let error = authority_public_paths_violation(dir.path(), &rows).expect_err("item route");
    assert!(
        error.contains("AUTHORITY_PATHS") && error.contains("route.rs"),
        "{error}"
    );
    std::fs::write(&route, "pub use neovm_value::test_support::authority::*;\n").expect("route");
    assert!(authority_public_paths_violation(dir.path(), &rows).is_err());
    std::fs::write(&route, "pub use neovm_value::test_support::authority;\n").expect("route");
    assert!(authority_public_paths_violation(dir.path(), &rows).is_err());
    std::fs::write(&route, "pub use neovm_value::value::*;\n").expect("route");
    assert!(authority_public_paths_violation(dir.path(), &rows).is_ok());
    std::fs::remove_file(&route).expect("route");
    std::fs::write(core_src.join("lib.rs"), "").expect("core lib");
    // A same-crate sibling re-export is another public route.
    std::fs::write(
        value_src.join("route.rs"),
        "pub use crate::test_support::authority::mark;\n",
    )
    .expect("value route");
    std::fs::write(
        value_src.join("lib.rs"),
        "pub mod test_support;\npub mod route;\npub(crate) mod original;\n",
    )
    .expect("lib");
    assert!(authority_public_paths_violation(dir.path(), &rows).is_err());
    // The original path becoming public is a violation even without a re-export.
    std::fs::remove_file(value_src.join("route.rs")).expect("route");
    std::fs::write(
        value_src.join("lib.rs"),
        "pub mod test_support;\npub mod original;\n",
    )
    .expect("lib");
    std::fs::write(value_src.join("original.rs"), "pub fn mark() {}\n").expect("original");
    let error = authority_public_paths_violation(dir.path(), &rows).expect_err("original public");
    assert!(error.contains("publicly reachable at"), "{error}");
    std::fs::write(
        value_src.join("lib.rs"),
        "pub mod test_support;\npub(crate) mod original;\n",
    )
    .expect("lib");
    std::fs::write(value_src.join("original.rs"), "pub(crate) fn mark() {}\n").expect("original");
    assert!(authority_public_paths_violation(dir.path(), &rows).is_ok());
}

/// LEAD 02:2xZ / peer 02:19Z: a crate with no signed `test_support::authority`
/// module (neovm-system) validates AUTHORITY rows as ORIGINAL-PATH rows at
/// their own module path. One green fixture plus three mutations pin the arm:
/// the signed module must declare the leaf publicly, its module chain must be
/// publicly reachable in the test-support build view, and the defining module
/// must not be publicly reachable at a DIFFERENT path than the signed one.
#[test]
fn authority_original_path_only_fenceless_mutations() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sys_src = dir.path().join("crates/neovm-system/src");
    std::fs::create_dir_all(sys_src.join("test_support")).expect("system src");
    // neovm-system is a fenced crate (LEAD 18:1xZ added its
    // test_support::authority module), so the original-path rows below are
    // validated in the Module branch; the fence must not expose their leaf.
    std::fs::write(sys_src.join("test_support.rs"), "pub mod authority;\n").expect("support");
    std::fs::write(
        sys_src.join("test_support/authority.rs"),
        "pub fn fence_only_item() {}\n",
    )
    .expect("fence");
    std::fs::write(
        sys_src.join("lib.rs"),
        "pub mod fileio;\npub mod original;\npub mod test_support;\n",
    )
    .expect("lib");
    std::fs::write(sys_src.join("fileio.rs"), "pub fn decode_for_test() {}\n").expect("fileio");
    std::fs::write(sys_src.join("original.rs"), "pub fn decode_for_test() {}\n").expect("original");
    let rows = vec![AuthRow {
        crate_name: "neovm-system".to_owned(),
        item: "fileio::decode_for_test".to_owned(),
        kind: "fn".to_owned(),
        defining_file: "crates/neovm-system/src/fileio.rs".to_owned(),
        fence_path: "neovm_system::fileio".to_owned(),
    }];
    assert!(authority_public_paths_violation(dir.path(), &rows).is_ok());

    // Mutation 1: the signed module does not declare the leaf publicly.
    std::fs::write(sys_src.join("fileio.rs"), "fn decode_for_test() {}\n").expect("fileio");
    let error = authority_public_paths_violation(dir.path(), &rows)
        .expect_err("private leaf in the signed module");
    assert!(error.contains("does not publicly expose"), "{error}");
    std::fs::write(sys_src.join("fileio.rs"), "pub fn decode_for_test() {}\n").expect("fileio");

    // Mutation 2: the module chain is not publicly reachable in the
    // test-support build view.
    std::fs::write(
        sys_src.join("lib.rs"),
        "pub(crate) mod fileio;\npub mod original;\npub mod test_support;\n",
    )
    .expect("lib");
    let error =
        authority_public_paths_violation(dir.path(), &rows).expect_err("private module chain");
    assert!(
        error.contains("module chain is not publicly reachable"),
        "{error}"
    );
    std::fs::write(
        sys_src.join("lib.rs"),
        "pub mod fileio;\npub mod original;\npub mod test_support;\n",
    )
    .expect("lib");

    // Mutation 3: the defining module is publicly reachable at a DIFFERENT
    // path than the signed one (a second route).
    let second_route = vec![AuthRow {
        crate_name: "neovm-system".to_owned(),
        item: "fileio::decode_for_test".to_owned(),
        kind: "fn".to_owned(),
        defining_file: "crates/neovm-system/src/original.rs".to_owned(),
        fence_path: "neovm_system::fileio".to_owned(),
    }];
    let error = authority_public_paths_violation(dir.path(), &second_route)
        .expect_err("defining module at a second path");
    assert!(error.contains("also publicly reachable at"), "{error}");

    // Inline tail: `pub mod test_support { ... }` in a production module has
    // no separate source file; the arm resolves it inside the defining file.
    std::fs::write(
        sys_src.join("fileio.rs"),
        "pub mod test_support { pub fn inline_for_test() {} }\n",
    )
    .expect("fileio");
    let inline_rows = vec![AuthRow {
        crate_name: "neovm-system".to_owned(),
        item: "fileio::test_support::inline_for_test".to_owned(),
        kind: "fn".to_owned(),
        defining_file: "crates/neovm-system/src/fileio.rs".to_owned(),
        fence_path: "neovm_system::fileio::test_support".to_owned(),
    }];
    assert!(authority_public_paths_violation(dir.path(), &inline_rows).is_ok());
    std::fs::write(
        sys_src.join("fileio.rs"),
        "pub(crate) mod test_support { pub fn inline_for_test() {} }\n",
    )
    .expect("fileio");
    let error = authority_public_paths_violation(dir.path(), &inline_rows)
        .expect_err("private inline module chain");
    assert!(
        error.contains("module chain is not publicly reachable"),
        "{error}"
    );
}

/// Peer 15:4xZ pre-P2 condition: a test-support mod-row route must not become
/// a second public path to an AUTHORITY (fence-only) item. AUTHORITY-PATHS
/// computes reachability in the test-support build (test = false, feature
/// test-support = true), so a `visibility::make(pub)` widened module's
/// re-export of the fence item is a second route and goes red with
/// B3_AUTHORITY; the same re-export in a module that stays private in that
/// build is not a public route.
#[test]
fn authority_mod_row_route_is_a_second_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let value_src = dir.path().join("crates/neovm-value/src");
    std::fs::create_dir_all(value_src.join("test_support")).expect("value src");
    std::fs::write(
        value_src.join("lib.rs"),
        "pub mod test_support;\npub(crate) mod original;\npub(crate) mod widened;\n",
    )
    .expect("lib");
    std::fs::write(value_src.join("test_support.rs"), "pub mod authority;\n").expect("support");
    std::fs::write(
        value_src.join("test_support/authority.rs"),
        "pub fn mark() { crate::original::mark(); }\n",
    )
    .expect("fence");
    std::fs::write(value_src.join("original.rs"), "pub(crate) fn mark() {}\n").expect("original");
    let widened = value_src.join("widened.rs");
    std::fs::write(&widened, "pub use crate::test_support::authority::mark;\n").expect("widened");
    let rows = vec![AuthRow {
        crate_name: "neovm-value".to_owned(),
        item: "mark".to_owned(),
        kind: "fn".to_owned(),
        defining_file: "crates/neovm-value/src/original.rs".to_owned(),
        fence_path: "neovm_value::test_support::authority::mark".to_owned(),
    }];
    // The module is private in every build: the re-export is not a public
    // route, so the fence keeps its single signed path.
    assert!(authority_public_paths_violation(dir.path(), &rows).is_ok());
    // A VIS mod-row widening makes the module public in the TEST-SUPPORT
    // build; the same re-export is then a second public path to the
    // fence-only item and must go red.
    std::fs::write(
        value_src.join("lib.rs"),
        "#[cfg_attr(feature = \"test-support\", visibility::make(pub))]\n\
         pub(crate) mod widened;\npub mod test_support;\npub(crate) mod original;\n",
    )
    .expect("lib");
    let error = authority_public_paths_violation(dir.path(), &rows).expect_err("second route");
    assert!(
        error.contains("B3_AUTHORITY") && error.contains("second route"),
        "{error}"
    );
    // Removing the route closes it again even with the widening in place.
    std::fs::write(&widened, "").expect("widened");
    assert!(authority_public_paths_violation(dir.path(), &rows).is_ok());
}

#[test]
fn authority_alias_method_rows_are_method_aware() {
    // A signed row may name a method reached through a fence-declared public
    // type alias (`authority::Owner::method`): the fence declares the alias,
    // and the method is a pub inherent method of the aliased type in the
    // row's signed defining file.
    let dir = tempfile::tempdir().expect("tempdir");
    let value_src = dir.path().join("crates/neovm-value/src");
    let core_src = dir.path().join("crates/neovm-core/src");
    std::fs::create_dir_all(value_src.join("test_support")).expect("value src");
    std::fs::create_dir_all(&core_src).expect("core src");
    std::fs::write(
        value_src.join("lib.rs"),
        "pub mod test_support;\npub(crate) mod original;\n",
    )
    .expect("lib");
    std::fs::write(value_src.join("test_support.rs"), "pub mod authority;\n").expect("support");
    std::fs::write(
        value_src.join("test_support/authority.rs"),
        "pub type Owner<'a> = crate::original::Owner<'a>;\n",
    )
    .expect("fence");
    std::fs::write(
        value_src.join("original.rs"),
        "pub struct Owner<'a>(&'a ());\nimpl<'a> Owner<'a> {\n    pub fn method(&self) {}\n}\n",
    )
    .expect("original");
    std::fs::write(core_src.join("lib.rs"), "").expect("core lib");
    let rows = vec![AuthRow {
        crate_name: "neovm-value".to_owned(),
        item: "Owner::method".to_owned(),
        kind: "method".to_owned(),
        defining_file: "crates/neovm-value/src/original.rs".to_owned(),
        fence_path: "neovm_value::test_support::authority::Owner::method".to_owned(),
    }];
    assert!(authority_public_paths_violation(dir.path(), &rows).is_ok());
    // A private method is not a public path.
    std::fs::write(
        value_src.join("original.rs"),
        "pub struct Owner<'a>(&'a ());\nimpl<'a> Owner<'a> {\n    fn method(&self) {}\n}\n",
    )
    .expect("private method");
    let error = authority_public_paths_violation(dir.path(), &rows).expect_err("private method");
    assert!(error.contains("does not declare"), "{error}");
    // An undeclared alias leaves the leaf undeclared in the fence.
    std::fs::write(
        value_src.join("test_support/authority.rs"),
        "pub type Other<'a> = crate::original::Owner<'a>;\n",
    )
    .expect("undeclared alias");
    assert!(authority_public_paths_violation(dir.path(), &rows).is_err());
    // A private alias is not a public path either.
    std::fs::write(
        value_src.join("test_support/authority.rs"),
        "type Owner<'a> = crate::original::Owner<'a>;\n",
    )
    .expect("private alias");
    assert!(authority_public_paths_violation(dir.path(), &rows).is_err());
    // An alias whose target name has no such public method is red even when
    // the defining file has one on a different type name.
    std::fs::write(
        value_src.join("test_support/authority.rs"),
        "pub type Owner<'a> = crate::original::Absent<'a>;\n",
    )
    .expect("wrong target");
    std::fs::write(
        value_src.join("original.rs"),
        "pub struct Owner<'a>(&'a ());\nimpl<'a> Owner<'a> {\n    pub fn method(&self) {}\n}\n",
    )
    .expect("original");
    assert!(authority_public_paths_violation(dir.path(), &rows).is_err());
    // Original-path method rows (item-level `#[doc(hidden)] pub` at the
    // defining module path): the module chain must be public, the module must
    // publicly expose the owner, and the defining file must expose the pub
    // inherent method. The fence must stay a non-route.
    std::fs::write(
        value_src.join("lib.rs"),
        "pub mod tagged;\npub mod test_support;\npub(crate) mod original;\n",
    )
    .expect("lib");
    std::fs::create_dir_all(value_src.join("tagged")).expect("tagged dir");
    std::fs::write(value_src.join("tagged.rs"), "pub mod gc;\n").expect("tagged");
    std::fs::write(
        value_src.join("tagged/gc.rs"),
        "pub use crate::original::Owner;\n",
    )
    .expect("gc");
    std::fs::write(value_src.join("test_support.rs"), "pub mod authority;\n").expect("support");
    std::fs::write(value_src.join("test_support/authority.rs"), "").expect("fence");
    std::fs::write(
        value_src.join("original.rs"),
        "pub struct Owner;\nimpl Owner {\n    pub fn method(&self) {}\n}\n",
    )
    .expect("original");
    let original_rows = vec![AuthRow {
        crate_name: "neovm-value".to_owned(),
        item: "Owner::method".to_owned(),
        kind: "method".to_owned(),
        defining_file: "crates/neovm-value/src/original.rs".to_owned(),
        fence_path: "neovm_value::tagged::gc::Owner::method".to_owned(),
    }];
    assert!(authority_public_paths_violation(dir.path(), &original_rows).is_ok());
    // A non-public method is not a public path.
    std::fs::write(
        value_src.join("original.rs"),
        "pub struct Owner;\nimpl Owner {\n    fn method(&self) {}\n}\n",
    )
    .expect("private method");
    assert!(authority_public_paths_violation(dir.path(), &original_rows).is_err());
    // The module must publicly expose the owner, not merely the private file.
    std::fs::write(
        value_src.join("original.rs"),
        "pub struct Owner;\nimpl Owner {\n    pub fn method(&self) {}\n}\n",
    )
    .expect("original");
    std::fs::write(value_src.join("tagged/gc.rs"), "").expect("gc");
    assert!(authority_public_paths_violation(dir.path(), &original_rows).is_err());
    // The module chain itself must be public.
    std::fs::write(
        value_src.join("tagged/gc.rs"),
        "pub use crate::original::Owner;\n",
    )
    .expect("gc");
    std::fs::write(
        value_src.join("lib.rs"),
        "pub(crate) mod tagged;\npub mod test_support;\npub(crate) mod original;\n",
    )
    .expect("lib");
    assert!(authority_public_paths_violation(dir.path(), &original_rows).is_err());
    // The authority fence must not expose the item as a second route.
    std::fs::write(
        value_src.join("lib.rs"),
        "pub mod tagged;\npub mod test_support;\npub(crate) mod original;\n",
    )
    .expect("lib");
    std::fs::write(
        value_src.join("test_support/authority.rs"),
        "pub use crate::original::Owner;\n",
    )
    .expect("fence route");
    assert!(authority_public_paths_violation(dir.path(), &original_rows).is_err());
}

/// Peer r2 R3: the two new original-path arms are keyed by the row's KIND.
/// A field row is satisfied only by a pub named field, and the trait
/// self-name arm only satisfies a kind=trait row.  Green controls plus the
/// requested red fixtures: a private field, a tuple field, a non-pub owner,
/// a private trait, and both kind mismatches.
#[test]
fn authority_kind_keyed_route_mutations() {
    let dir = tempfile::tempdir().expect("tempdir");
    let value_src = dir.path().join("crates/neovm-value/src");
    let core_src = dir.path().join("crates/neovm-core/src");
    std::fs::create_dir_all(value_src.join("test_support")).expect("value src");
    std::fs::create_dir_all(&core_src).expect("core src");
    std::fs::write(
        value_src.join("lib.rs"),
        "pub mod test_support;\npub mod types;\npub mod impls;\npub mod traits;\npub(crate) mod hidden;\n",
    )
    .expect("lib");
    std::fs::write(value_src.join("test_support.rs"), "pub mod authority;\n").expect("support");
    std::fs::write(value_src.join("test_support/authority.rs"), "").expect("fence");
    std::fs::write(
        value_src.join("types.rs"),
        "pub struct Holder {\n    pub field: u8,\n    secret: u8,\n}\n\
         impl Holder {\n    pub fn method(&self) {}\n}\n\
         pub struct TupleHolder(pub u8);\n\
         pub(crate) struct CrateOwner;\n",
    )
    .expect("types");
    std::fs::write(value_src.join("hidden.rs"), "pub struct HiddenOwner;\n").expect("hidden");
    std::fs::write(
        value_src.join("impls.rs"),
        "use crate::hidden::HiddenOwner;\n\
         use crate::types::CrateOwner;\n\
         impl HiddenOwner {\n    pub fn hidden_method(&self) {}\n}\n\
         impl CrateOwner {\n    pub fn crate_method(&self) {}\n}\n",
    )
    .expect("impls");
    std::fs::write(
        value_src.join("traits.rs"),
        "pub trait PublicTrait {}\ntrait PrivateTrait {}\n",
    )
    .expect("traits");
    std::fs::write(core_src.join("lib.rs"), "").expect("core lib");
    let row = |item: &str, kind: &str, defining: &str, fence: &str| AuthRow {
        crate_name: "neovm-value".to_owned(),
        item: item.to_owned(),
        kind: kind.to_owned(),
        defining_file: defining.to_owned(),
        fence_path: fence.to_owned(),
    };
    let check =
        |row: &AuthRow| authority_public_paths_violation(dir.path(), std::slice::from_ref(row));

    // Green controls: a pub named field for kind=field, a pub trait's own
    // name for kind=trait.
    let field = row(
        "types::Holder::field",
        "field",
        "crates/neovm-value/src/types.rs",
        "neovm_value::types::Holder::field",
    );
    assert!(check(&field).is_ok());
    let trait_row = row(
        "traits::PublicTrait",
        "trait",
        "crates/neovm-value/src/traits.rs",
        "neovm_value::traits::PublicTrait",
    );
    assert!(check(&trait_row).is_ok());

    // A private field is not a field route.
    let private_field = row(
        "types::Holder::secret",
        "field",
        "crates/neovm-value/src/types.rs",
        "neovm_value::types::Holder::secret",
    );
    let error = check(&private_field).expect_err("private field");
    assert!(error.contains("pub named field"), "{error}");
    // A tuple field has no own item path.
    let tuple_field = row(
        "types::TupleHolder::0",
        "field",
        "crates/neovm-value/src/types.rs",
        "neovm_value::types::TupleHolder::0",
    );
    let error = check(&tuple_field).expect_err("tuple field");
    assert!(error.contains("pub named field"), "{error}");
    // An owner declared pub only inside a private module is not public.
    let hidden_owner = row(
        "impls::HiddenOwner::hidden_method",
        "method",
        "crates/neovm-value/src/impls.rs",
        "neovm_value::impls::HiddenOwner::hidden_method",
    );
    let error = check(&hidden_owner).expect_err("private owner module");
    assert!(error.contains("does not publicly expose"), "{error}");
    // A pub(crate) owner is not public.
    let crate_owner = row(
        "impls::CrateOwner::crate_method",
        "method",
        "crates/neovm-value/src/impls.rs",
        "neovm_value::impls::CrateOwner::crate_method",
    );
    let error = check(&crate_owner).expect_err("pub(crate) owner");
    assert!(error.contains("does not publicly expose"), "{error}");
    // A private trait is not a public trait route.
    let private_trait = row(
        "traits::PrivateTrait",
        "trait",
        "crates/neovm-value/src/traits.rs",
        "neovm_value::traits::PrivateTrait",
    );
    let error = check(&private_trait).expect_err("private trait");
    assert!(error.contains("does not publicly expose"), "{error}");
    // Kind mismatch: a kind=field row cannot ride a pub inherent method.
    let field_on_method = row(
        "types::Holder::method",
        "field",
        "crates/neovm-value/src/types.rs",
        "neovm_value::types::Holder::method",
    );
    let error = check(&field_on_method).expect_err("field row on a method");
    assert!(error.contains("pub named field"), "{error}");
    // Kind mismatch: a method-shaped row of any other kind cannot ride a pub
    // field.
    let method_on_field = row(
        "types::Holder::field",
        "method",
        "crates/neovm-value/src/types.rs",
        "neovm_value::types::Holder::field",
    );
    let error = check(&method_on_field).expect_err("method row on a field");
    assert!(error.contains("pub inherent method"), "{error}");
    // Kind mismatch: the trait self-name arm is off for a kind=method row.
    let method_on_trait = row(
        "traits::PublicTrait",
        "method",
        "crates/neovm-value/src/traits.rs",
        "neovm_value::traits::PublicTrait",
    );
    let error = check(&method_on_trait).expect_err("method row on a trait name");
    assert!(error.contains("does not publicly expose"), "{error}");
}

/// Peer r2 C2 re-cut (lead #977): a cross-file owner row is signed at the
/// owner's PUBLIC module (its own public module or a `pub use` route), and
/// the guard must NOT fall back to a name match elsewhere in the crate.
/// Red fixtures: a privately-imported owner, and a same-named pub type
/// elsewhere.
#[test]
fn authority_rejects_name_only_owner_resolution() {
    let dir = tempfile::tempdir().expect("tempdir");
    let value_src = dir.path().join("crates/neovm-value/src");
    let core_src = dir.path().join("crates/neovm-core/src");
    std::fs::create_dir_all(value_src.join("test_support")).expect("value src");
    std::fs::create_dir_all(&core_src).expect("core src");
    std::fs::write(
        value_src.join("lib.rs"),
        "pub mod test_support;\npub mod api;\npub mod impls;\npub mod other;\npub(crate) mod hidden;\n",
    )
    .expect("lib");
    std::fs::write(value_src.join("test_support.rs"), "pub mod authority;\n").expect("support");
    std::fs::write(value_src.join("test_support/authority.rs"), "").expect("fence");
    // The owner types live in a private module; `api` re-exports one.
    std::fs::write(
        value_src.join("hidden.rs"),
        "pub struct Owner;\npub struct AliasName;\n",
    )
    .expect("hidden");
    std::fs::write(value_src.join("api.rs"), "pub use crate::hidden::Owner;\n").expect("api");
    // The impl file imports both owners privately.
    std::fs::write(
        value_src.join("impls.rs"),
        "use crate::hidden::{AliasName, Owner};\n\
         impl Owner {\n    pub fn method(&self) {}\n}\n\
         impl AliasName {\n    pub fn alias_method(&self) {}\n}\n",
    )
    .expect("impls");
    // An unrelated public type with the same name as a private import.
    std::fs::write(value_src.join("other.rs"), "pub struct AliasName;\n").expect("other");
    std::fs::write(core_src.join("lib.rs"), "").expect("core lib");
    let row = |item: &str, defining: &str, fence: &str| AuthRow {
        crate_name: "neovm-value".to_owned(),
        item: item.to_owned(),
        kind: "method".to_owned(),
        defining_file: defining.to_owned(),
        fence_path: fence.to_owned(),
    };
    let check =
        |row: &AuthRow| authority_public_paths_violation(dir.path(), std::slice::from_ref(row));

    // Green: the signed module is the public re-export; the impl (and its
    // private import) live in another file.
    let exported = row(
        "api::Owner::method",
        "crates/neovm-value/src/impls.rs",
        "neovm_value::api::Owner::method",
    );
    assert!(check(&exported).is_ok());

    // Red: signing the impl module's own path cannot borrow the owner's
    // public declaration elsewhere: a privately-imported owner is not a
    // public route.
    let private_import = row(
        "impls::Owner::method",
        "crates/neovm-value/src/impls.rs",
        "neovm_value::impls::Owner::method",
    );
    let error = check(&private_import).expect_err("privately-imported owner");
    assert!(error.contains("does not publicly expose"), "{error}");

    // Red: a same-named pub type elsewhere must not satisfy the row (the
    // rejected name-anywhere fallback would have accepted it).
    let same_name_elsewhere = row(
        "impls::AliasName::alias_method",
        "crates/neovm-value/src/impls.rs",
        "neovm_value::impls::AliasName::alias_method",
    );
    let error = check(&same_name_elsewhere).expect_err("same-named pub type elsewhere");
    assert!(error.contains("does not publicly expose"), "{error}");
}

#[test]
fn authority_pub_use_method_rows_are_method_aware() {
    // A fence `pub use` of a type from a private module exposes the type at
    // the fence; a signed method row on that type resolves through the
    // re-export to the pub inherent method in the row's defining file
    // (REGWAIT Deadline::contention).
    let dir = tempfile::tempdir().expect("tempdir");
    let value_src = dir.path().join("crates/neovm-value/src");
    let core_src = dir.path().join("crates/neovm-core/src");
    std::fs::create_dir_all(value_src.join("test_support")).expect("value src");
    std::fs::create_dir_all(&core_src).expect("core src");
    std::fs::write(
        value_src.join("lib.rs"),
        "pub mod test_support;\npub(crate) mod original;\n",
    )
    .expect("lib");
    std::fs::write(value_src.join("test_support.rs"), "pub mod authority;\n").expect("support");
    std::fs::write(
        value_src.join("test_support/authority.rs"),
        "pub use crate::original::Owner;\n",
    )
    .expect("fence");
    std::fs::write(
        value_src.join("original.rs"),
        "pub struct Owner;\nimpl Owner {\n    pub fn method(&self) {}\n}\n",
    )
    .expect("original");
    std::fs::write(core_src.join("lib.rs"), "").expect("core lib");
    let rows = vec![AuthRow {
        crate_name: "neovm-value".to_owned(),
        item: "Owner::method".to_owned(),
        kind: "method".to_owned(),
        defining_file: "crates/neovm-value/src/original.rs".to_owned(),
        fence_path: "neovm_value::test_support::authority::Owner::method".to_owned(),
    }];
    assert!(authority_public_paths_violation(dir.path(), &rows).is_ok());
    // A non-public method is not a public path.
    std::fs::write(
        value_src.join("original.rs"),
        "pub struct Owner;\nimpl Owner {\n    fn method(&self) {}\n}\n",
    )
    .expect("private method");
    assert!(authority_public_paths_violation(dir.path(), &rows).is_err());
    // The fence must re-export the owner publicly.
    std::fs::write(
        value_src.join("test_support/authority.rs"),
        "pub(crate) use crate::original::Owner;\n",
    )
    .expect("private use");
    assert!(authority_public_paths_violation(dir.path(), &rows).is_err());
    std::fs::write(value_src.join("test_support/authority.rs"), "").expect("fence");
    assert!(authority_public_paths_violation(dir.path(), &rows).is_err());
    // A second fence route to the same owner breaks the unique signed path.
    std::fs::write(
        value_src.join("original.rs"),
        "pub struct Owner;\nimpl Owner {\n    pub fn method(&self) {}\n}\n",
    )
    .expect("original");
    std::fs::write(
        value_src.join("test_support/authority.rs"),
        "pub use crate::original::Owner;\npub type Owner = crate::original::Owner;\n",
    )
    .expect("second route");
    let error = authority_public_paths_violation(dir.path(), &rows).expect_err("second route");
    assert!(error.contains("fence routes"), "{error}");
}

#[cfg(test)]
mod disjointness_ratchet_tests {
    use super::*;

    fn auth_row(crate_name: &str, item: &str, defining_file: &str, fence_path: &str) -> AuthRow {
        // These fixtures only exercise route/disjointness identity, so the
        // kind is the path-shape default (the kind-keyed checks have their
        // own fixtures).
        let kind = match item.rsplit("::").nth(1) {
            Some(owner)
                if owner
                    .chars()
                    .next()
                    .is_some_and(|first| first.is_ascii_uppercase()) =>
            {
                "method"
            }
            _ => "fn",
        };
        AuthRow {
            crate_name: crate_name.to_owned(),
            item: item.to_owned(),
            kind: kind.to_owned(),
            defining_file: defining_file.to_owned(),
            fence_path: fence_path.to_owned(),
        }
    }

    fn vis_entry(crate_name: &str, item: &str, file: &str) -> VisEntry {
        VisEntry {
            crate_name: crate_name.to_owned(),
            item: item.to_owned(),
            file: file.to_owned(),
        }
    }

    #[test]
    fn vis_authority_disjointness_catches_both_row_conventions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let value_src = dir.path().join("crates/neovm-value/src");
        std::fs::create_dir_all(value_src.join("test_support")).expect("src");
        std::fs::write(value_src.join("lib.rs"), "pub mod test_support;\n").expect("lib");
        std::fs::write(value_src.join("test_support.rs"), "pub mod authority;\n").expect("support");
        std::fs::write(
            value_src.join("test_support/authority.rs"),
            "pub fn dup() { crate::tagged::x::dup(); }\n",
        )
        .expect("fence");
        // Convention 1: original-path row; the VIS row names the same item.
        let original = auth_row(
            "neovm-value",
            "tagged::x::dup",
            "crates/neovm-value/src/tagged.rs",
            "neovm_value::tagged::x::dup",
        );
        let same = vec![vis_entry(
            "neovm-value",
            "tagged::x::dup",
            "crates/neovm-value/src/tagged.rs",
        )];
        vis_authority_disjoint_violation(dir.path(), &same, &[original.clone()])
            .expect_err("original-path pair is red");
        vis_authority_disjoint_violation(dir.path(), &[], &[original])
            .expect("no VIS row is green");
        // Convention 2: fence-spelling row resolved through the delegate body.
        let fenced = auth_row(
            "neovm-value",
            "test_support::authority::dup",
            "crates/neovm-value/src/test_support/authority.rs",
            "neovm_value::test_support::authority::dup",
        );
        let resolved = vec![vis_entry(
            "neovm-value",
            "tagged::x::dup",
            "crates/neovm-value/src/tagged.rs",
        )];
        let error = vis_authority_disjoint_violation(dir.path(), &resolved, &[fenced.clone()])
            .expect_err("fence pair is red");
        assert!(error.contains("exactly one cross-crate route"), "{error}");
        let other = vec![vis_entry(
            "neovm-value",
            "tagged::y::dup",
            "crates/neovm-value/src/tagged.rs",
        )];
        vis_authority_disjoint_violation(dir.path(), &other, &[fenced])
            .expect("a different item is green");
    }

    #[test]
    fn authority_cfg_pub_ratchet_mutations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("crates/neovm-value/src");
        std::fs::create_dir_all(&src).expect("src");
        let file = src.join("lib.rs");
        let row = auth_row(
            "neovm-value",
            "flag_for_test",
            "crates/neovm-value/src/lib.rs",
            "neovm_value::flag_for_test",
        );
        let allow = |items: &[(&str, &str, &str)]| -> BTreeSet<CfgPubRow> {
            items
                .iter()
                .map(|(crate_name, item, defining_file)| CfgPubRow {
                    crate_name: (*crate_name).to_owned(),
                    item: (*item).to_owned(),
                    defining_file: (*defining_file).to_owned(),
                })
                .collect()
        };
        // (a) production-public: green with an empty allowlist.
        std::fs::write(&file, "pub fn flag_for_test() {}\n").expect("pub");
        authority_cfg_pub_violation(dir.path(), &[row.clone()], &allow(&[]), 0)
            .expect("production-public is exempt");
        // (b) new cfg-gated plain pub: red; allowlisted: green.
        std::fs::write(
            &file,
            "#[cfg(any(test, feature = \"test-support\"))]\npub fn flag_for_test() {}\n",
        )
        .expect("cfg pub");
        let error = authority_cfg_pub_violation(dir.path(), &[row.clone()], &allow(&[]), 0)
            .expect_err("new (b) item is red");
        assert!(error.contains("NEW cfg-gated plain-pub"), "{error}");
        let frozen = allow(&[(
            "neovm-value",
            "flag_for_test",
            "crates/neovm-value/src/lib.rs",
        )]);
        authority_cfg_pub_violation(dir.path(), &[row.clone()], &frozen, 1)
            .expect("allowlisted (b) is green");
        // pub(crate) cfg-gated is not the plain-pub class.
        std::fs::write(
            &file,
            "#[cfg(any(test, feature = \"test-support\"))]\npub(crate) fn flag_for_test() {}\n",
        )
        .expect("cfg crate pub");
        authority_cfg_pub_violation(dir.path(), &[row.clone()], &allow(&[]), 0)
            .expect("pub(crate) is exempt");
        // Removing the item leaves a stale allowlist row: red.
        std::fs::write(&file, "pub fn other() {}\n").expect("absent");
        let error = authority_cfg_pub_violation(dir.path(), &[row.clone()], &frozen, 1)
            .expect_err("missing item is red");
        assert!(
            error.contains("not found") || error.contains("no longer classifies"),
            "{error}"
        );
        // The frozen ceiling may only shrink.
        std::fs::write(
            &file,
            "#[cfg(any(test, feature = \"test-support\"))]\npub fn flag_for_test() {}\n",
        )
        .expect("cfg pub again");
        let ceiling_error = authority_cfg_pub_violation(dir.path(), &[row], &frozen, 0)
            .expect_err("ceiling shrink");
        assert!(ceiling_error.contains("allowlist"), "{ceiling_error}");
    }

    #[test]
    fn duplicated_make_pub_attribute_mutations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("crates/neovm-value/src");
        std::fs::create_dir_all(&src).expect("src");
        let file = src.join("lib.rs");
        let attr = "#[cfg_attr(feature = \"test-support\", visibility::make(pub))]";
        std::fs::write(&file, format!("{attr}\npub(crate) fn single() {{}}\n")).expect("single");
        duplicated_test_support_widening(dir.path()).expect("one attribute is green");
        std::fs::write(
            &file,
            format!("{attr}\n#[inline]\n{attr}\npub(crate) fn duplicated() {{}}\n"),
        )
        .expect("duplicated");
        let error = duplicated_test_support_widening(dir.path()).expect_err("duplicate is red");
        assert!(error.contains("duplicated"), "{error}");
        // The arm counts per item even when doc attributes sit between the two
        // widenings, and it also covers fields.
        std::fs::write(
            &file,
            format!("pub(crate) struct S {{\n    {attr}\n    {attr}\n    pub(crate) f: u8,\n}}\n"),
        )
        .expect("field duplicated");
        assert!(duplicated_test_support_widening(dir.path()).is_err());
        // The signed cfg-split twin shape has exactly one attribute per arm.
        std::fs::write(
            &file,
            format!(
                "pub(crate) struct S {{\n    #[cfg(feature = \"test-support\")]\n    {attr}\n    pub f: u8,\n    #[cfg(not(feature = \"test-support\"))]\n    pub(crate) f: u8,\n}}\n"
            ),
        )
        .expect("cfg split");
        duplicated_test_support_widening(dir.path()).expect("cfg-split twins are one each");
    }

    #[test]
    fn vis_free_fn_reset_ratchet_mutations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let value_src = dir.path().join("crates/neovm-value/src");
        std::fs::create_dir_all(&value_src).expect("src");
        let file = value_src.join("lib.rs");
        let row = |item: &str| VisRow {
            crate_dir: "neovm-value".to_owned(),
            item: item.to_owned(),
            kind: "fn".to_owned(),
            file: "crates/neovm-value/src/lib.rs".to_owned(),
            tests: vec!["crates/neovm-core/tests/t.rs".to_owned()],
        };
        // A free-fn TLS counter reset is red: the allowlist is retired, so
        // there is no frozen-green state.
        std::fs::write(
            &file,
            "thread_local! { static COUNTER: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }\npub(crate) fn reset_counter() { COUNTER.with(|count| count.set(0)); }\n",
        )
        .expect("reset");
        let error = vis_free_fn_reset_violation(dir.path(), &[row("reset_counter")])
            .expect_err("free-fn TLS reset is red");
        assert!(error.contains("free-function TLS/static"), "{error}");
        // A method reset mutates an owned instance: it stays VIS.
        std::fs::write(
            &file,
            "pub(crate) struct Owned { count: std::cell::Cell<usize> }\nimpl Owned { pub(crate) fn reset_count(&self) { self.count.set(0); } }\n",
        )
        .expect("method");
        vis_free_fn_reset_violation(dir.path(), &[row("reset_count")])
            .expect("owned-instance reset stays VIS");
        // A free function that only resets its owned argument stays VIS.
        std::fs::write(
            &file,
            "pub(crate) fn reset_view(view: &mut Vec<usize>) { view.clear(); }\n",
        )
        .expect("argument reset");
        vis_free_fn_reset_violation(dir.path(), &[row("reset_view")])
            .expect("argument-owned reset stays VIS");
        // A read-only TLS helper whose name does not denote a reset/store is
        // outside the ratified class (it cannot zero a counter).
        std::fs::write(
            &file,
            "thread_local! { static COUNTER: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }\npub(crate) fn read_counter() -> usize { COUNTER.with(|count| count.get()) }\n",
        )
        .expect("read helper");
        vis_free_fn_reset_violation(dir.path(), &[row("read_counter")])
            .expect("read-only TLS helper stays outside the ratchet");
        // A free-fn static STORE without the reset naming convention is red.
        std::fs::write(
            &file,
            "static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);\npub(crate) fn store_counter() { COUNTER.store(0, std::sync::atomic::Ordering::Relaxed); }\n",
        )
        .expect("store");
        let error = vis_free_fn_reset_violation(dir.path(), &[row("store_counter")])
            .expect_err("static store is red");
        assert!(error.contains("free-function TLS/static"), "{error}");
    }

    #[test]
    fn vis_authority_owner_key_and_row_333_mutations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let value_src = dir.path().join("crates/neovm-value/src");
        std::fs::create_dir_all(&value_src).expect("src");
        let file = value_src.join("original.rs");
        std::fs::write(
            &file,
            "pub struct Alpha;\npub struct Beta;\nimpl Alpha { pub(crate) fn registry(&self) {} }\nimpl Beta { pub(crate) fn registry(&self) {} }\n",
        )
        .expect("owners");
        let auth = auth_row(
            "neovm-value",
            "Alpha::registry",
            "crates/neovm-value/src/original.rs",
            "neovm_value::Alpha::registry",
        );
        // LEAD 11:2xZ key: the same leaf on a DIFFERENT owner is green even in
        // the same defining file (the historical registry case).
        vis_authority_disjoint_violation(
            dir.path(),
            &[vis_entry(
                "neovm-value",
                "Beta::registry",
                "crates/neovm-value/src/original.rs",
            )],
            &[auth.clone()],
        )
        .expect("a different owner with the same leaf is green");
        // The same owner and leaf is the second route: red.
        let error = vis_authority_disjoint_violation(
            dir.path(),
            &[vis_entry(
                "neovm-value",
                "Alpha::registry",
                "crates/neovm-value/src/original.rs",
            )],
            &[auth],
        )
        .expect_err("same owner is red");
        assert!(error.contains("exactly one cross-crate route"), "{error}");
        // (i) the historical row-333 pair: the free-function clone-counter
        // reset that main e8fe979a7a routed through value's authority fence.
        let chunk = value_src.join("chunk.rs");
        std::fs::write(
            &chunk,
            "thread_local! { static CLONES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }\npub(crate) fn reset_bytecode_function_clone_count_for_test() { CLONES.with(|count| count.set(0)); }\n",
        )
        .expect("chunk");
        let clone_auth = auth_row(
            "neovm-value",
            "reset_bytecode_function_clone_count_for_test",
            "crates/neovm-value/src/chunk.rs",
            "neovm_value::test_support::authority::reset_bytecode_function_clone_count_for_test",
        );
        let clone_vis = vis_entry(
            "neovm-value",
            "tagged::bytecode::chunk::reset_bytecode_function_clone_count_for_test",
            "crates/neovm-value/src/chunk.rs",
        );
        let error = vis_authority_disjoint_violation(dir.path(), &[clone_vis], &[clone_auth])
            .expect_err("re-added row 333 is red");
        assert!(error.contains("exactly one cross-crate route"), "{error}");
    }

    fn vis_row_for(item: &str, kind: &str) -> VisRow {
        VisRow {
            crate_dir: "neovm-display".to_owned(),
            item: item.to_owned(),
            kind: kind.to_owned(),
            file: "crates/neovm-display/src/emacs_core/display/xdisp/mod.rs".to_owned(),
            tests: vec![
                "crates/neovm-core/src/emacs_core/tests/moved_display/xdisp_upper_layer_test.rs"
                    .to_owned(),
            ],
        }
    }

    /// Peer #981 R2 red fixture: the historical stale row shape
    /// (`mode_line_numeric_padding::force_for_test`) was a pub(crate) item with
    /// a VIS row and no widening. The row-to-item arm must reject it.
    #[test]
    fn vis_row_visibility_arm_rejects_an_unwidened_pub_crate_item() {
        let row = vis_row_for("emacs_core::xdisp::stale_helper", "fn");
        let source = "pub(crate) fn stale_helper() {}\n";
        assert_eq!(
            visible_decl_in_source(source, &row.item, &row.kind).unwrap(),
            Some(false)
        );
        let error =
            vis_row_visibility_violation(&row, source).expect_err("stale VIS row must be red");
        assert!(error.contains("without a cross-crate route"), "{error}");
    }

    #[test]
    fn vis_row_visibility_arm_accepts_plain_pub_and_visibility_make() {
        let row = vis_row_for("emacs_core::xdisp::live_helper", "fn");
        assert!(vis_row_visibility_violation(&row, "pub fn live_helper() {}\n").is_ok());
        let widened = "#[cfg_attr(any(test, feature = \"test-support\"), visibility::make(pub))]\npub(crate) fn live_helper() {}\n";
        assert!(vis_row_visibility_violation(&row, widened).is_ok());
        let feature_only =
            "#[cfg(all(not(test), feature = \"test-support\"))]\npub fn live_helper() {}\n";
        assert!(vis_row_visibility_violation(&row, feature_only).is_ok());
    }

    #[test]
    fn vis_row_visibility_arm_reads_cfg_split_fields() {
        let row = vis_row_for("emacs_core::xdisp::Thing::field", "field");
        let split = "struct Thing {\n#[cfg(any(test, feature = \"test-support\"))]\npub field: u8,\n#[cfg(not(any(test, feature = \"test-support\")))]\nfield: u8,\n}\n";
        assert!(vis_row_visibility_violation(&row, split).is_ok());
        let private = "struct Thing {\nfield: u8,\n}\n";
        assert!(vis_row_visibility_violation(&row, private).is_err());
    }

    #[test]
    fn vis_row_visibility_arm_reads_pub_crate_use_with_widening() {
        let row = vis_row_for("emacs_core::display::xdisp::SUBRS", "use");
        let widened = "#[cfg_attr(any(test, feature = \"test-support\"), visibility::make(pub))]\npub(crate) use self::subrs::SUBRS;\n";
        assert!(vis_row_visibility_violation(&row, widened).is_ok());
        let plain = "pub(crate) use self::subrs::SUBRS;\n";
        assert!(vis_row_visibility_violation(&row, plain).is_err());
    }
}

/// Peer #828 / lead #809: a pub named field of the owner struct/union in the
/// defining file is a public route for the field row (the field-row shape of
/// an original-path AUTHORITY row).  Named fields only: tuple fields have no
/// own item path.
fn auth_file_pub_field(file: &Path, owner: &str, field: &str) -> Result<bool, String> {
    let text = read(file)?;
    let ast = syn::parse_file(&text)
        .map_err(|error| format!("AUTHORITY_PATHS {}: {error}", file.display()))?;
    fn declares(items: &[syn::Item], owner: &str, field: &str) -> bool {
        items.iter().any(|item| match item {
            syn::Item::Struct(item) => {
                item.ident == owner
                    && matches!(item.vis, syn::Visibility::Public(_))
                    && match &item.fields {
                        syn::Fields::Named(named) => named.named.iter().any(|f| {
                            f.ident.as_ref().is_some_and(|i| i == field)
                                && matches!(f.vis, syn::Visibility::Public(_))
                        }),
                        _ => false,
                    }
            }
            syn::Item::Union(item) => {
                item.ident == owner
                    && matches!(item.vis, syn::Visibility::Public(_))
                    && item.fields.named.iter().any(|f| {
                        f.ident.as_ref().is_some_and(|i| i == field)
                            && matches!(f.vis, syn::Visibility::Public(_))
                    })
            }
            syn::Item::Mod(module) => module
                .content
                .as_ref()
                .is_some_and(|(_, inner)| declares(inner, owner, field)),
            _ => false,
        })
    }
    Ok(declares(&ast.items, owner, field))
}
#[cfg(test)]
mod structural_field_guard_tests {
    use super::*;

    #[test]
    fn field_visibility_bifurcation_plant_reds_then_narrows_green() {
        // Whole-struct arms: the test-support field is pub, production
        // pub(crate); only `f` differs, so exactly one violation.
        let split = r#"
            #[cfg(feature = "test-support")]
            pub(crate) struct S { pub f: u8, pub(crate) g: u8 }
            #[cfg(not(feature = "test-support"))]
            pub(crate) struct S { pub(crate) f: u8, pub(crate) g: u8 }
        "#;
        let found = field_visibility_bifurcations(split).expect("synthetic parses");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("S::f"), "{found:?}");

        // Narrowing the test-support arm to pub(crate) is green.
        let narrowed = r#"
            #[cfg(any(test, feature = "test-support"))]
            pub(crate) struct S { pub(crate) f: u8 }
            #[cfg(not(any(test, feature = "test-support")))]
            pub(crate) struct S { pub(crate) f: u8 }
        "#;
        assert!(
            field_visibility_bifurcations(narrowed)
                .expect("synthetic parses")
                .is_empty()
        );

        // Field-level arms are the same rule.
        let field_split = r#"
            pub(crate) struct S {
                #[cfg(feature = "test-support")] pub f: u8,
                #[cfg(not(feature = "test-support"))] pub(crate) f: u8,
            }
        "#;
        assert_eq!(
            field_visibility_bifurcations(field_split)
                .expect("synthetic parses")
                .len(),
            1
        );

        // cfg(test)-only arms are not cross-crate widenings.
        let test_only = r#"
            #[cfg(test)] pub(crate) struct S { pub f: u8 }
            #[cfg(not(test))] pub(crate) struct S { pub(crate) f: u8 }
        "#;
        assert!(
            field_visibility_bifurcations(test_only)
                .expect("synthetic parses")
                .is_empty()
        );

        // Same-named structs in different inline modules never collide.
        let modules = r#"
            mod a { #[cfg(feature = "test-support")] pub(crate) struct S { pub f: u8 } }
            mod b { #[cfg(not(feature = "test-support"))] pub(crate) struct S { pub(crate) f: u8 } }
        "#;
        assert!(
            field_visibility_bifurcations(modules)
                .expect("synthetic parses")
                .is_empty()
        );
    }
}
