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
    if all_rows.iter().any(|row| row.kind == "mod") {
        expanded_mod_rows_violation(workspace, &all_rows, &auth_rows(workspace))?;
    }
    Ok(())
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

struct AuthRow {
    crate_name: String,
    item: String,
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

/// The two signed fence definition files and their fence module path,
/// relative to the crate root (the eval authority module is inline).
fn auth_fence_file(crate_name: &str) -> Option<(&'static str, &'static [&'static str])> {
    match crate_name {
        "neovm-value" => Some((
            "crates/neovm-value/src/test_support/authority.rs",
            &["test_support", "authority"],
        )),
        "neovm-eval" => Some((
            "crates/neovm-eval/src/test_support.rs",
            &["test_support", "authority"],
        )),
        "neovm-edit" => Some((
            "crates/neovm-edit/src/test_support/authority.rs",
            &["test_support", "authority"],
        )),
        // The display fence is its signed test_support::authority module
        // (peer 19:36Z follow-up): the three DisplayFrameExt test fixtures
        // route through delegates there.
        "neovm-display" => Some((
            "crates/neovm-display/src/test_support/authority.rs",
            &["test_support", "authority"],
        )),
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
fn auth_declares_public(file: &Path, leaf: &str, impl_owner: Option<&str>) -> Result<bool, String> {
    let text = read(file)?;
    let ast = syn::parse_file(&text)
        .map_err(|error| format!("AUTHORITY_PATHS {}: {error}", file.display()))?;
    fn items_declare(items: &[syn::Item], leaf: &str, impl_owner: Option<&str>) -> bool {
        items.iter().any(|item| {
            let public = match item {
                syn::Item::Fn(item) => {
                    item.sig.ident == leaf && matches!(item.vis, syn::Visibility::Public(_))
                }
                syn::Item::Type(item) => {
                    item.ident == leaf && matches!(item.vis, syn::Visibility::Public(_))
                }
                syn::Item::Struct(item) => {
                    item.ident == leaf && matches!(item.vis, syn::Visibility::Public(_))
                }
                syn::Item::Enum(item) => {
                    item.ident == leaf && matches!(item.vis, syn::Visibility::Public(_))
                }
                // Trait methods carry no `pub` keyword; a method of a public
                // trait is a public path (the display fence's shape).
                syn::Item::Trait(item) => {
                    matches!(item.vis, syn::Visibility::Public(_))
                        && item.items.iter().any(|member| {
                            matches!(member, syn::TraitItem::Fn(function) if function.sig.ident == leaf)
                        })
                }
                syn::Item::Const(item) => {
                    item.ident == leaf && matches!(item.vis, syn::Visibility::Public(_))
                }
                syn::Item::Static(item) => {
                    item.ident == leaf && matches!(item.vis, syn::Visibility::Public(_))
                }
                syn::Item::Use(item) => {
                    let mut leaves = Vec::new();
                    auth_use_leaves(&item.tree, &mut Vec::new(), &mut leaves);
                    matches!(item.vis, syn::Visibility::Public(_))
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
                            .is_some_and(|(_, inner)| items_declare(inner, leaf, impl_owner))
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
    Ok(items_declare(&ast.items, leaf, impl_owner))
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
        .map(|row| {
            let (file, module) = auth_fence_file(&row.crate_name).unwrap_or_else(|| {
                panic!("AUTHORITY_PATHS unknown fence crate {}", row.crate_name)
            });
            (
                row.crate_name.clone(),
                (
                    module.iter().map(|segment| segment.to_string()).collect(),
                    workspace.join(file),
                ),
            )
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
        let (fence_file, _) = auth_fence_file(&crate_name)
            .unwrap_or_else(|| panic!("AUTHORITY_PATHS unknown fence crate {crate_name}"));
        let lib = workspace
            .join("crates")
            .join(&crate_name)
            .join("src")
            .join("lib.rs");
        let mut map = BTreeMap::new();
        auth_collect_modules(&lib, Vec::new(), true, &mut map)?;
        let fence = workspace
            .join(fence_file)
            .canonicalize()
            .map_err(|error| format!("AUTHORITY_PATHS {fence_file}: {error}"))?;
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
            let (_, module) = auth_fence_file(&crate_name).unwrap();
            let module: Vec<String> = module.iter().map(|segment| segment.to_string()).collect();
            let in_fence = rest.len() > module.len() && rest[..module.len()] == module[..];
            if in_fence {
                let fence_public = map
                    .iter()
                    .find(|(_, (segments, _))| *segments == module)
                    .map(|(_, (_, public))| *public)
                    .or_else(|| map.get(&fence).map(|(_, public)| *public))
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
                if !auth_declares_public(&fence, leaf, fence_owner)?
                    && !auth_alias_method_is_fenced(
                        &workspace,
                        &fence,
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
                let Some((module_file, module_public)) = auth_module_entry(&map, module_segments)
                else {
                    return Err(format!(
                        "AUTHORITY_PATHS {expected}: no source module for {}",
                        module_segments.join("::")
                    ));
                };
                if !module_public {
                    return Err(format!(
                        "AUTHORITY_PATHS {expected}: the module chain is not publicly reachable"
                    ));
                }
                if !auth_declares_public(module_file, owner, None)? {
                    return Err(format!(
                        "AUTHORITY_PATHS {expected}: {} does not publicly expose {owner:?}",
                        module_file.display()
                    ));
                }
                if method_row
                    && !auth_file_pub_inherent_method(
                        &workspace.join(&row.defining_file),
                        &[owner.to_string()],
                        leaf,
                    )?
                {
                    return Err(format!(
                        "AUTHORITY_PATHS {expected}: the defining file does not expose {owner}::{leaf} as a pub inherent method"
                    ));
                }
                if auth_declares_public(&fence, leaf, None)?
                    || (method_row && auth_declares_public(&fence, owner, None)?)
                {
                    return Err(format!(
                        "AUTHORITY_PATHS {expected}: the authority fence also exposes the item, so the signed path is not exclusive"
                    ));
                }
            }
            let defining = workspace
                .join(&row.defining_file)
                .canonicalize()
                .map_err(|error| format!("AUTHORITY_PATHS {}: {error}", row.defining_file))?;
            if defining != fence {
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
