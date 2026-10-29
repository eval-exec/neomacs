//! Explicit architecture-gate/xtask inventory of parallel-entry declarations.
//!
//! This does not generate or modify serial dispatch code. Unknown registration
//! syntax fails the architecture gate; installed-catalog tests also check the
//! active configuration against this all-configuration source inventory.

use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Expr, Fields, Item, Token};

#[path = "../src/emacs_core/lisp/native/subr/effect_schema.rs"]
mod schema;
use schema::{Authority, CatalogKind, Classification, SchemaEntry, SchemaError};

#[derive(Clone, Copy)]
enum DeclarationMark {
    Unmarked,
    World,
}

#[derive(Clone, Copy)]
enum ScanContext {
    Production,
    PureTemplate,
}

struct Record {
    kind: CatalogKind,
    name: String,
    source: String,
    classification: Classification,
    target: Option<String>,
}

struct Variant {
    name: String,
    pattern: String,
}

struct Scanner<'records> {
    records: &'records mut Vec<Record>,
    source: String,
    mark: DeclarationMark,
    pure_template: DeclarationMark,
    context: ScanContext,
    template_constructors: usize,
    evaluator_templates: usize,
    errors: Vec<String>,
}

impl Scanner<'_> {
    fn record(&mut self, kind: CatalogKind, name: String, mark: DeclarationMark) {
        let classification = match mark {
            DeclarationMark::Unmarked => Classification::Unclassified,
            DeclarationMark::World => Classification::Classified(Authority::World),
        };
        self.records.push(Record {
            kind,
            name,
            source: self.source.clone(),
            classification,
            target: None,
        });
    }

    fn expressions(&mut self, tokens: TokenStream, mark: DeclarationMark) {
        match Punctuated::<Expr, Token![,]>::parse_terminated.parse2(tokens) {
            Ok(expressions) => {
                let previous = std::mem::replace(&mut self.mark, mark);
                for expression in &expressions {
                    self.visit_expr(expression);
                }
                self.mark = previous;
            }
            Err(error) => self.errors.push(format!(
                "{}: cannot parse classified subr declarations: {error}",
                self.source
            )),
        }
    }

    fn cfg_select(&mut self, tokens: TokenStream) {
        // cfg_select's branch bodies are ordinary Rust items. Parse every
        // branch: this is deliberately the all-configuration inventory.
        let mut branches = 0;
        for token in tokens {
            if let TokenTree::Group(group) = token
                && group.delimiter() == Delimiter::Brace
            {
                branches += 1;
                match syn::parse2::<syn::File>(group.stream()) {
                    Ok(file) => self.visit_file(&file),
                    Err(error) => self.errors.push(format!(
                        "{}: cannot parse cfg_select branch: {error}",
                        self.source
                    )),
                }
            }
        }
        if branches == 0 {
            self.errors
                .push(format!("{}: cfg_select has no branch bodies", self.source));
        }
    }

    fn pure_template(&mut self, tokens: TokenStream) {
        let expansions = match macro_expansions(tokens) {
            Ok(expansions) => expansions,
            Err(error) => {
                self.errors.push(format!("{}: {error}", self.source));
                return;
            }
        };
        for expansion in expansions {
            let block = match syn::parse2::<syn::Block>(expansion) {
                Ok(block) => block,
                Err(error) => {
                    self.errors.push(format!(
                        "{}: cannot parse register_pure_subr expansion: {error}",
                        self.source
                    ));
                    return;
                }
            };
            let mut template_records = Vec::new();
            let mut scanner = Scanner {
                records: &mut template_records,
                source: self.source.clone(),
                mark: DeclarationMark::Unmarked,
                pure_template: DeclarationMark::Unmarked,
                context: ScanContext::PureTemplate,
                template_constructors: 0,
                evaluator_templates: 0,
                errors: Vec::new(),
            };
            scanner.visit_block(&block);
            if scanner.template_constructors != 1 {
                scanner.errors.push(format!(
                    "{}: register_pure_subr expansion must contain exactly one explicitly World-classified SubrSpec::new constructor",
                    self.source
                ));
            }
            if !scanner.errors.is_empty() {
                self.errors.extend(scanner.errors);
                return;
            }
        }
        self.pure_template = DeclarationMark::World;
    }
}

fn replace_metavariables(tokens: TokenStream) -> Result<TokenStream, String> {
    let mut result = TokenStream::new();
    let mut tokens = tokens.into_iter();
    while let Some(token) = tokens.next() {
        match token {
            TokenTree::Punct(punct) if punct.as_char() == '$' => match tokens.next() {
                Some(TokenTree::Ident(name)) => result.extend([TokenTree::Ident(
                    proc_macro2::Ident::new(&format!("__pentry_meta_{name}"), name.span()),
                )]),
                _ => return Err("unsupported repetition in register_pure_subr expansion".into()),
            },
            TokenTree::Group(group) => {
                let mut replaced = proc_macro2::Group::new(
                    group.delimiter(),
                    replace_metavariables(group.stream())?,
                );
                replaced.set_span(group.span());
                result.extend([TokenTree::Group(replaced)]);
            }
            token => result.extend([token]),
        }
    }
    Ok(result)
}

fn macro_expansions(tokens: TokenStream) -> Result<Vec<TokenStream>, String> {
    let mut tokens = tokens.into_iter().peekable();
    let mut expansions = Vec::new();
    while let Some(token) = tokens.next() {
        if let TokenTree::Punct(punct) = token
            && punct.as_char() == '='
        {
            if !matches!(tokens.next(), Some(TokenTree::Punct(punct)) if punct.as_char() == '>') {
                return Err("invalid register_pure_subr macro rule".into());
            }
            let Some(TokenTree::Group(group)) = tokens.next() else {
                return Err("register_pure_subr expansion must be a block".into());
            };
            if group.delimiter() != Delimiter::Brace {
                return Err("register_pure_subr expansion must be a block".into());
            }
            expansions.push(replace_metavariables(TokenStream::from(TokenTree::Group(
                group,
            )))?);
        }
    }
    if expansions.is_empty() {
        return Err("register_pure_subr has no expansion rules".into());
    }
    Ok(expansions)
}

fn last_ident(path: &syn::Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn path_pair(path: &syn::Path) -> Option<(String, String)> {
    let mut segments = path.segments.iter().rev();
    let method = segments.next()?.ident.to_string();
    let owner = segments.next()?.ident.to_string();
    Some((owner, method))
}

fn literal_name(expression: &Expr) -> Option<String> {
    if let Expr::Lit(literal) = expression
        && let syn::Lit::Str(name) = &literal.lit
    {
        return Some(name.value());
    }
    None
}

fn contains_ident(tokens: TokenStream, expected: &str) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => ident == expected,
        TokenTree::Group(group) => contains_ident(group.stream(), expected),
        TokenTree::Punct(_) | TokenTree::Literal(_) => false,
    })
}

fn test_only(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && matches!(&attribute.meta, syn::Meta::List(list) if list.tokens.to_string() == "test")
    })
}

fn item_attributes(item: &Item) -> &[syn::Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        // syn's non-exhaustive syntax enum permits future Rust item kinds.
        _ => &[],
    }
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        if !test_only(item_attributes(item)) {
            visit::visit_item(self, item);
        }
    }

    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if item
            .ident
            .as_ref()
            .is_some_and(|name| name == "register_pure_subr")
        {
            self.pure_template(item.mac.tokens.clone());
            return;
        }
        visit::visit_item_macro(self, item);
    }

    fn visit_macro(&mut self, declaration: &'ast syn::Macro) {
        match last_ident(&declaration.path).as_deref() {
            Some("world_subr") => {
                self.expressions(declaration.tokens.clone(), DeclarationMark::World);
            }
            Some("define_subrs") => self.expressions(declaration.tokens.clone(), self.mark),
            Some("cfg_select") => {
                if [
                    "SubrSpec",
                    "define_subrs",
                    "register_pure_subr",
                    "WorldSubr",
                    "EvaluatorSubr",
                    "world_registry",
                ]
                .iter()
                .any(|name| contains_ident(declaration.tokens.clone(), name))
                {
                    self.cfg_select(declaration.tokens.clone());
                }
            }
            Some("register_pure_subr") => {
                match Punctuated::<Expr, Token![,]>::parse_terminated
                    .parse2(declaration.tokens.clone())
                {
                    Ok(arguments) => match arguments.iter().nth(1).and_then(literal_name) {
                        Some(name) => self.record(CatalogKind::Primitive, name, self.pure_template),
                        None => self.errors.push(format!(
                            "{}: register_pure_subr needs a literal Lisp name",
                            self.source
                        )),
                    },
                    Err(error) => self.errors.push(format!(
                        "{}: cannot parse register_pure_subr invocation: {error}",
                        self.source
                    )),
                }
            }
            _ => {
                if contains_ident(declaration.tokens.clone(), "SubrSpec") {
                    self.errors.push(format!(
                        "{}: unknown registration macro {} contains SubrSpec",
                        self.source,
                        last_ident(&declaration.path).unwrap_or_default()
                    ));
                }
            }
        }
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Expr::Path(function) = call.func.as_ref()
            && let Some((owner, method)) = path_pair(&function.path)
        {
            if owner == "WorldSubr" && method == "new" {
                let previous = std::mem::replace(&mut self.mark, DeclarationMark::World);
                for argument in &call.args {
                    self.visit_expr(argument);
                }
                self.mark = previous;
                return;
            }
            if owner == "SubrSpec" {
                if !matches!(
                    method.as_str(),
                    "new"
                        | "fixed0"
                        | "fixed1"
                        | "fixed2"
                        | "fixed3"
                        | "fixed4"
                        | "fixed5"
                        | "fixed6"
                        | "fixed7"
                        | "fixed8"
                        | "evaluator"
                ) {
                    self.errors.push(format!(
                        "{}: unsupported SubrSpec constructor: {method}",
                        self.source
                    ));
                    return;
                }
                if matches!(self.context, ScanContext::PureTemplate) {
                    if method == "new"
                        && call.args.first().and_then(literal_name).is_none()
                        && matches!(self.mark, DeclarationMark::World)
                    {
                        self.template_constructors += 1;
                    } else {
                        self.errors.push(format!(
                            "{}: register_pure_subr template lacks an explicitly World-classified name-parameter constructor",
                            self.source
                        ));
                    }
                    visit::visit_expr_call(self, call);
                    return;
                }
                match call.args.first().and_then(literal_name) {
                    Some(name) => self.record(CatalogKind::Primitive, name, self.mark),
                    None if method == "evaluator"
                        && self.source == "src/emacs_core/runtime/eval/subrs.rs" =>
                    {
                        match self.mark {
                            DeclarationMark::World => self.evaluator_templates += 1,
                            DeclarationMark::Unmarked => self.errors.push(format!(
                                "{}: evaluator template lacks explicit WorldSubr classification",
                                self.source
                            )),
                        }
                    }
                    None => self.errors.push(format!(
                        "{}: unsupported nonliteral SubrSpec::{method} registration",
                        self.source
                    )),
                }
            }
            if owner == "EvaluatorSubr" && matches!(method.as_str(), "special" | "callable") {
                match call.args.first().and_then(literal_name) {
                    Some(name) => self.record(CatalogKind::Primitive, name, DeclarationMark::World),
                    None => self.errors.push(format!(
                        "{}: evaluator declaration needs a literal Lisp name",
                        self.source
                    )),
                }
            }
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_struct(&mut self, expression: &'ast syn::ExprStruct) {
        if last_ident(&expression.path).as_deref() == Some("SubrSpec") {
            self.errors.push(format!(
                "{}: unsupported SubrSpec struct literal; use an explicitly classified constructor",
                self.source
            ));
            return;
        }
        visit::visit_expr_struct(self, expression);
    }
}

fn source_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "tests") {
                source_files(&path, files)?;
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if name != "tests.rs" && !name.ends_with("_test.rs") && !name.ends_with("_tests.rs") {
                files.push(path);
            }
        }
    }
    Ok(())
}

fn enum_variants(path: &Path, expected: &str) -> Result<(Vec<Variant>, DeclarationMark), String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let file = syn::parse_file(&source).map_err(|error| format!("{}: {error}", path.display()))?;
    let declaration = file
        .items
        .iter()
        .find_map(|item| match item {
            Item::Enum(item) if item.ident == expected => Some(item),
            _ => None,
        })
        .ok_or_else(|| format!("{}: missing enum {expected}", path.display()))?;
    let mut mark = DeclarationMark::Unmarked;
    for item in &file.items {
        if let Item::Macro(item) = item
            && last_ident(&item.mac.path).as_deref() == Some("world_registry")
        {
            let registry = syn::parse2::<syn::Ident>(item.mac.tokens.clone()).map_err(|error| {
                format!("{}: invalid world_registry marker: {error}", path.display())
            })?;
            if registry == expected {
                match mark {
                    DeclarationMark::Unmarked => mark = DeclarationMark::World,
                    DeclarationMark::World => {
                        return Err(format!(
                            "{}: duplicate authority classification for registry {expected}",
                            path.display()
                        ));
                    }
                }
            }
        }
    }
    let variants = declaration
        .variants
        .iter()
        .map(|variant| {
            let name = variant.ident.to_string();
            let pattern = match &variant.fields {
                Fields::Unit => name.clone(),
                Fields::Unnamed(_) => format!("{name}(..)"),
                Fields::Named(_) => format!("{name} {{ .. }}"),
            };
            Variant { name, pattern }
        })
        .collect();
    Ok((variants, mark))
}

fn kind_name(kind: CatalogKind) -> &'static str {
    match kind {
        CatalogKind::Primitive => "Primitive",
        CatalogKind::Opcode => "Opcode",
        CatalogKind::Leaf => "Leaf",
        CatalogKind::Alias => "Alias",
    }
}

fn authority_name(classification: Classification) -> Result<&'static str, String> {
    match classification {
        Classification::Unclassified => {
            Err("primitive inventory: missing authority classification".into())
        }
        Classification::Classified(Authority::Scalar) => Ok("Scalar"),
        Classification::Classified(Authority::Sealed) => Ok("Sealed"),
        Classification::Classified(Authority::World) => Ok("World"),
    }
}

// Only top-level literal-name defalias forms are part of this shipped source
// registry. Nonliteral target expressions stay World-only, with no claimed
// symbol target; runtime alias resolution remains authoritative.
fn alias_records(source: &str, records: &mut Vec<Record>) -> Result<(), String> {
    for line in source.lines() {
        let Some(tail) = line.strip_prefix("(defalias '") else {
            continue;
        };
        let name_end = tail
            .find(char::is_whitespace)
            .ok_or_else(|| "lisp/subr.el: cannot parse literal defalias declaration".to_string())?;
        let name = &tail[..name_end];
        let definition = tail[name_end..].trim_start();
        let quoted = definition
            .strip_prefix("#'")
            .or_else(|| definition.strip_prefix('\''));
        let target = quoted.map(|definition| {
            definition
                .split(|character: char| character.is_whitespace() || character == ')')
                .next()
                .unwrap_or_default()
                .to_string()
        });
        if name.is_empty() || target.as_ref().is_some_and(String::is_empty) {
            return Err("lisp/subr.el: empty literal defalias name or target".into());
        }
        records.push(Record {
            kind: CatalogKind::Alias,
            name: name.into(),
            source: "lisp/subr.el".into(),
            classification: Classification::Classified(Authority::World),
            target,
        });
    }
    Ok(())
}

pub fn generate(manifest: &Path, project_root: &Path, out: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    source_files(&manifest.join("src"), &mut files)?;
    files.sort();
    let mut records = Vec::new();
    for path in files {
        let source =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let file =
            syn::parse_file(&source).map_err(|error| format!("{}: {error}", path.display()))?;
        let relative = path
            .strip_prefix(manifest)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let mut scanner = Scanner {
            records: &mut records,
            source: relative,
            mark: DeclarationMark::Unmarked,
            pure_template: DeclarationMark::Unmarked,
            context: ScanContext::Production,
            template_constructors: 0,
            evaluator_templates: 0,
            errors: Vec::new(),
        };
        scanner.visit_file(&file);
        if scanner.source == "src/emacs_core/runtime/eval/subrs.rs"
            && scanner.evaluator_templates != 2
        {
            scanner.errors.push(format!(
                "{}: expected exactly two explicitly World-classified evaluator templates, found {}",
                scanner.source, scanner.evaluator_templates
            ));
        }
        if !scanner.errors.is_empty() {
            return Err(scanner.errors.join("\n"));
        }
    }
    let opcode_source = "src/emacs_core/runtime/bytecode/opcode.rs";
    let tier_i_source = "src/emacs_core/runtime/eval/tier_i/compile.rs";
    let leaf_source = "src/emacs_core/lisp/native/subr/leaf.rs";
    let (opcodes, opcode_mark) = enum_variants(&manifest.join(opcode_source), "Op")?;
    let (tier_i_opcodes, tier_i_mark) = enum_variants(&manifest.join(tier_i_source), "Op")?;
    let (leaves, leaf_mark) = enum_variants(&manifest.join(leaf_source), "LeafId")?;
    for (kind, source, variants, mark) in [
        (CatalogKind::Opcode, opcode_source, &opcodes, opcode_mark),
        (CatalogKind::Leaf, leaf_source, &leaves, leaf_mark),
    ] {
        let classification = match mark {
            DeclarationMark::Unmarked => Classification::Unclassified,
            DeclarationMark::World => Classification::Classified(Authority::World),
        };
        for variant in variants {
            records.push(Record {
                kind,
                name: variant.name.clone(),
                source: source.into(),
                classification,
                target: None,
            });
        }
    }
    let tier_i_classification = match tier_i_mark {
        DeclarationMark::Unmarked => Classification::Unclassified,
        DeclarationMark::World => Classification::Classified(Authority::World),
    };
    for variant in &tier_i_opcodes {
        records.push(Record {
            kind: CatalogKind::Opcode,
            name: format!("TierI::{}", variant.name),
            source: tier_i_source.into(),
            classification: tier_i_classification,
            target: None,
        });
    }
    let aliases_path = project_root.join("lisp/subr.el");
    let aliases_source = fs::read_to_string(&aliases_path)
        .map_err(|error| format!("{}: {error}", aliases_path.display()))?;
    alias_records(&aliases_source, &mut records)?;

    let schema_entries = records
        .iter()
        .map(|record| SchemaEntry {
            kind: record.kind,
            name: &record.name,
            classification: record.classification,
        })
        .collect::<Vec<_>>();
    if let Err(error) = schema::validate(&schema_entries) {
        let detail = match error {
            SchemaError::UnclassifiedPrimitive => records
                .iter()
                .find(|record| record.classification == Classification::Unclassified)
                .map(|record| format!("{} ({})", record.name, record.source))
                .unwrap_or_default(),
            SchemaError::DuplicateRegistration => {
                let mut detail = String::new();
                for (index, record) in records.iter().enumerate() {
                    if records[..index]
                        .iter()
                        .any(|earlier| earlier.kind == record.kind && earlier.name == record.name)
                    {
                        detail = format!("{} ({})", record.name, record.source);
                        break;
                    }
                }
                detail
            }
        };
        let message = match error {
            SchemaError::UnclassifiedPrimitive => {
                "primitive inventory: missing authority classification"
            }
            SchemaError::DuplicateRegistration => "primitive inventory: duplicate registration",
        };
        return Err(format!("{message}: {detail}"));
    }

    let mut generated = String::from("// Generated by build_support/pentry_inventory.rs.\n");
    generated.push_str("#[allow(dead_code)] // E1 cold inventory has no production consumer.\n");
    generated.push_str("pub(super) static INVENTORY: &[InventoryEntry] = &[\n");
    let mut json_entries = Vec::new();
    for record in &records {
        let kind = kind_name(record.kind);
        let authority = authority_name(record.classification)?;
        let target = match &record.target {
            Some(target) => format!("Some({target:?})"),
            None => "None".into(),
        };
        generated.push_str(&format!(
            "    InventoryEntry {{ kind: CatalogKind::{kind}, name: {:?}, source: {:?}, authority: Authority::{authority}, target: {target} }},\n",
            record.name, record.source
        ));
        json_entries.push(serde_json::json!({
            "kind": kind,
            "name": record.name,
            "source": record.source,
            "authority": authority,
            "target": record.target,
        }));
    }
    generated.push_str("];\n");
    for (table, function, argument, variants) in [
        (
            "OPCODES",
            "opcode_authority",
            "crate::emacs_core::bytecode::Op",
            &opcodes,
        ),
        (
            "LEAVES",
            "leaf_authority",
            "crate::emacs_core::subr::leaf::LeafId",
            &leaves,
        ),
    ] {
        generated.push_str("#[allow(dead_code)] // E1 cold registry has no production consumer.\n");
        generated.push_str(&format!(
            "pub(super) static {table}: &[(&str, Authority)] = &[\n"
        ));
        for variant in variants {
            generated.push_str(&format!("    ({:?}, Authority::World),\n", variant.name));
        }
        generated.push_str("];\n");
        let reference = if table == "OPCODES" { "&" } else { "" };
        generated.push_str(
            "#[allow(dead_code)] // E1 exhaustive projection is not installed in dispatch.\n",
        );
        generated.push_str(&format!("pub(super) fn {function}(entry: {reference}{argument}) -> Authority {{\n    match entry {{\n"));
        for variant in variants {
            generated.push_str(&format!(
                "        {argument}::{} => Authority::World,\n",
                variant.pattern
            ));
        }
        generated.push_str("    }\n}\n");
    }
    // Tier-I's implementation enum remains private. Its complete source enum
    // contributes cold labels, never an exposed type or execution entrypoint.
    generated
        .push_str("#[allow(dead_code)] // E1 cold Tier-I registry has no production consumer.\n");
    generated.push_str("pub(super) static TIER_I_OPCODES: &[(&str, Authority)] = &[\n");
    for variant in &tier_i_opcodes {
        generated.push_str(&format!(
            "    ({:?}, Authority::World),\n",
            format!("TierI::{}", variant.name)
        ));
    }
    generated.push_str("];\n");
    let json = serde_json::json!({
        "schema_version": 1,
        "scope": "all source configurations; installed coverage is checked separately",
        "entries": json_entries,
    });
    fs::write(out.join("pentry_inventory.rs"), generated)
        .map_err(|error| format!("write generated Rust inventory: {error}"))?;
    fs::write(
        out.join("pentry-inventory.json"),
        serde_json::to_vec_pretty(&json)
            .map_err(|error| format!("encode inventory JSON: {error}"))?,
    )
    .map_err(|error| format!("write inventory JSON: {error}"))?;
    Ok(())
}
