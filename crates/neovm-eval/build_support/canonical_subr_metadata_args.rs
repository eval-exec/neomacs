//! Closed, source-only metadata arguments for immutable SubrSpec homes.
//! This is not a Rust evaluator or a general import/type resolver.

use std::collections::BTreeMap;
use std::path::Path;

const LIB: &str = "crates/neovm-eval/src/lib.rs";
const CORE: &str = "crates/neovm-eval/src/emacs_core/mod.rs";
const EVAL: &str = "crates/neovm-eval/src/emacs_core/runtime/eval/mod.rs";
const INTERACTIVE: &str = "crates/neovm-eval/src/emacs_core/runtime/eval/interactive_registry.rs";
const SUBR: &str = "crates/neovm-eval/src/emacs_core/lisp/native/subr/mod.rs";
const LEAF: &str = "crates/neovm-eval/src/emacs_core/lisp/native/subr/leaf.rs";
const BUILTINS: &str = "crates/neovm-eval/src/emacs_core/lisp/native/builtins/mod.rs";
const LEAVES: &str = "crates/neovm-eval/src/emacs_core/lisp/native/builtins/leaves/mod.rs";
const FLAT: &str = "crates/neovm-eval/src/emacs_core/lisp/native/builtins/";

type Args = syn::punctuated::Punctuated<syn::Expr, syn::Token![,]>;

fn failure(message: impl std::fmt::Display) -> String {
    format!("static descriptor metadata: {message}")
}

fn path(path: &syn::Path) -> Result<Vec<String>, String> {
    if path.leading_colon.is_some()
        || path.segments.is_empty()
        || path
            .segments
            .iter()
            .any(|part| !matches!(part.arguments, syn::PathArguments::None))
    {
        return Err(failure("only plain, nongeneric paths are supported"));
    }
    Ok(path
        .segments
        .iter()
        .map(|part| part.ident.to_string())
        .collect())
}

fn expression_path(expr: &syn::Expr) -> Result<Vec<String>, String> {
    let syn::Expr::Path(expr) = expr else {
        return Err(failure("expected one plain item path"));
    };
    if !expr.attrs.is_empty() || expr.qself.is_some() {
        return Err(failure(
            "qualified/attributed item expressions are unsupported",
        ));
    }
    path(&expr.path)
}

fn conditional(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg") || a.path().is_ident("cfg_attr") || a.path().is_ident("test")
    })
}

fn ordinary_attrs(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().all(|a| {
        [
            "doc", "allow", "deny", "warn", "forbid", "inline", "cold", "derive",
        ]
        .iter()
        .any(|name| a.path().is_ident(name))
    })
}

// The two existing leaf-owner files have this exact lint-only attribute.
// It never selects a definition. No other cfg_attr is interpreted here.
fn leaf_lint(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("cfg_attr") {
        return false;
    }
    let Ok(meta) = attr.parse_args_with(
        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
    ) else {
        return false;
    };
    let parts = meta.iter().collect::<Vec<_>>();
    let [syn::Meta::List(condition), syn::Meta::List(lint)] = parts.as_slice() else {
        return false;
    };
    if !condition.path.is_ident("not") || !lint.path.is_ident("allow") {
        return false;
    }
    let Ok(syn::Meta::NameValue(feature)) = syn::parse2::<syn::Meta>(condition.tokens.clone())
    else {
        return false;
    };
    let syn::Expr::Lit(value) = feature.value else {
        return false;
    };
    let syn::Lit::Str(value) = value.lit else {
        return false;
    };
    feature.path.is_ident("feature")
        && value.value() == "jit"
        && syn::parse2::<syn::Ident>(lint.tokens.clone()).is_ok_and(|ident| ident == "dead_code")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ty {
    Interactive,
    Placeholder,
    Leaf,
    Str,
}
impl Ty {
    fn owner(self) -> (&'static str, &'static str) {
        match self {
            Self::Interactive => (INTERACTIVE, "BuiltinInteractiveSpec"),
            Self::Placeholder => (SUBR, "NoEvalPlaceholder"),
            Self::Leaf => (LEAF, "LeafSpec"),
            Self::Str => ("", "str"),
        }
    }
    fn canonical(self) -> Vec<String> {
        let parts: &[&str] = match self {
            Self::Interactive => &[
                "crate",
                "emacs_core",
                "eval",
                "interactive_registry",
                "BuiltinInteractiveSpec",
            ],
            Self::Placeholder => &["crate", "emacs_core", "subr", "NoEvalPlaceholder"],
            Self::Leaf => &["crate", "emacs_core", "subr", "leaf", "LeafSpec"],
            Self::Str => &["str"],
        };
        parts.iter().map(|s| (*s).to_owned()).collect()
    }
}

struct Resolver<'a> {
    root: &'a Path,
    home: &'a str,
    files: BTreeMap<String, syn::File>,
}
impl<'a> Resolver<'a> {
    fn new(root: &'a Path, home: &'a str, ast: &syn::File) -> Self {
        Self {
            root,
            home,
            files: BTreeMap::from([(home.to_owned(), ast.clone())]),
        }
    }
    fn file(&mut self, file: &str) -> Result<syn::File, String> {
        if !self.files.contains_key(file) {
            let source = std::fs::read_to_string(self.root.join(file))
                .map_err(|e| failure(format!("{file}: {e}")))?;
            let ast = syn::parse_file(&source).map_err(|e| failure(format!("{file}: {e}")))?;
            self.files.insert(file.to_owned(), ast);
        }
        let ast = self.files[file].clone();
        for attr in &ast.attrs {
            if !ordinary_attrs(std::slice::from_ref(attr))
                && !((file == LEAF || file == LEAVES) && leaf_lint(attr))
            {
                return Err(failure(format!("unsupported file attribute in {file}")));
            }
        }
        Ok(ast)
    }
    fn module(
        &mut self,
        parent: &str,
        name: &str,
        expected_path: Option<&str>,
    ) -> Result<(), String> {
        let ast = self.file(parent)?;
        let found: Vec<_> = ast
            .items
            .iter()
            .filter_map(|item| match item {
                syn::Item::Mod(m) if m.ident == name => Some(m),
                _ => None,
            })
            .collect();
        let [module] = found.as_slice() else {
            return Err(failure(format!(
                "ambiguous/missing module {parent}::{name}"
            )));
        };
        if module.content.is_some()
            || module.unsafety.is_some()
            || conditional(&module.attrs)
            || Self::named_type(&ast, name) != 1
            || !Self::binding_paths(&ast, name)?.is_empty()
        {
            return Err(failure("conditional/inline/unsafe metadata owner module"));
        }
        let mut paths = Vec::new();
        for attr in &module.attrs {
            if attr.path().is_ident("path") {
                let syn::Meta::NameValue(meta) = &attr.meta else {
                    return Err(failure("malformed module path"));
                };
                let syn::Expr::Lit(value) = &meta.value else {
                    return Err(failure("nonliteral module path"));
                };
                let syn::Lit::Str(value) = &value.lit else {
                    return Err(failure("nonstring module path"));
                };
                paths.push(value.value());
            } else if !ordinary_attrs(std::slice::from_ref(attr)) {
                return Err(failure("unsupported owner module attribute"));
            }
        }
        if paths
            != expected_path
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        {
            return Err(failure(format!(
                "unexpected owner module path {parent}::{name}"
            )));
        }
        Ok(())
    }
    fn owners(&mut self, file: &str) -> Result<(), String> {
        self.module(LIB, "emacs_core", None)?;
        if file == INTERACTIVE {
            self.module(CORE, "eval", Some("runtime/eval/mod.rs"))?;
            self.module(EVAL, "interactive_registry", None)?;
        } else if file == SUBR || file == LEAF {
            self.module(CORE, "subr", Some("lisp/native/subr/mod.rs"))?;
            if file == LEAF {
                self.module(SUBR, "leaf", None)?;
            }
        } else if file == BUILTINS || file == LEAVES || file.starts_with(FLAT) {
            self.module(CORE, "builtins", Some("lisp/native/builtins/mod.rs"))?;
            if file == LEAVES {
                self.module(BUILTINS, "leaves", None)?;
            } else if file != BUILTINS {
                let name = file
                    .strip_prefix(FLAT)
                    .and_then(|s| s.strip_suffix(".rs"))
                    .filter(|s| !s.contains('/'))
                    .ok_or_else(|| failure("unsupported metadata module"))?;
                self.module(BUILTINS, name, None)?;
            }
        } else {
            return Err(failure("metadata owner outside closed Eval modules"));
        }
        self.file(file)?;
        Ok(())
    }
    fn locate(&mut self, current: &str, p: &[String]) -> Result<(String, String), String> {
        let result = match p {
            [name] => (current.to_owned(), name.clone()),
            [s, name] if s == "self" => (current.to_owned(), name.clone()),
            [c, e, b, name] if c == "crate" && e == "emacs_core" && b == "builtins" => {
                (BUILTINS.to_owned(), name.clone())
            }
            [c, e, b, m, name] if c == "crate" && e == "emacs_core" && b == "builtins" => {
                let file = if m == "leaves" {
                    LEAVES.to_owned()
                } else {
                    format!("{FLAT}{m}.rs")
                };
                (file, name.clone())
            }
            [c, e, s, name] if c == "crate" && e == "emacs_core" && s == "subr" => {
                (SUBR.to_owned(), name.clone())
            }
            [c, e, s, l, name]
                if c == "crate" && e == "emacs_core" && s == "subr" && l == "leaf" =>
            {
                (LEAF.to_owned(), name.clone())
            }
            [c, e, v, i, name]
                if c == "crate"
                    && e == "emacs_core"
                    && v == "eval"
                    && i == "interactive_registry" =>
            {
                (INTERACTIVE.to_owned(), name.clone())
            }
            _ => return Err(failure("path is not a direct known Eval item")),
        };
        self.owners(&result.0)?;
        Ok(result)
    }
    fn binding_paths(ast: &syn::File, name: &str) -> Result<Vec<Vec<String>>, String> {
        fn walk(
            tree: &syn::UseTree,
            prefix: &mut Vec<String>,
            name: &str,
            out: &mut Vec<Vec<String>>,
        ) -> Result<(), String> {
            match tree {
                syn::UseTree::Path(p) => {
                    prefix.push(p.ident.to_string());
                    walk(&p.tree, prefix, name, out)?;
                    prefix.pop();
                }
                syn::UseTree::Name(n) if n.ident == name => {
                    let mut p = prefix.clone();
                    p.push(n.ident.to_string());
                    out.push(p);
                }
                syn::UseTree::Rename(n) if n.rename == name => {
                    return Err(failure("renamed metadata binding"));
                }
                syn::UseTree::Group(g) => {
                    for item in &g.items {
                        walk(item, prefix, name, out)?;
                    }
                }
                _ => {}
            }
            Ok(())
        }
        let mut out = Vec::new();
        for item in &ast.items {
            if let syn::Item::Use(u) = item {
                let before = out.len();
                walk(&u.tree, &mut Vec::new(), name, &mut out)?;
                if out.len() != before && (u.leading_colon.is_some() || conditional(&u.attrs)) {
                    return Err(failure("conditional metadata import"));
                }
            }
        }
        Ok(out)
    }
    fn named_type(ast: &syn::File, name: &str) -> usize {
        ast.items
            .iter()
            .filter(|i| match i {
                syn::Item::Enum(x) => x.ident == name,
                syn::Item::Struct(x) => x.ident == name,
                syn::Item::Type(x) => x.ident == name,
                syn::Item::Union(x) => x.ident == name,
                syn::Item::Mod(x) => x.ident == name,
                syn::Item::ExternCrate(x) => x.rename.as_ref().map_or(&x.ident, |(_, n)| n) == name,
                _ => false,
            })
            .count()
    }
    fn type_path(&mut self, current: &str, p: &[String], expected: Ty) -> Result<(), String> {
        let canonical = expected.canonical();
        if p == canonical {
            self.check_type_owner(expected)?;
            return Ok(());
        }
        let [name] = p else {
            return Err(failure("metadata type path is not its actual owner"));
        };
        let ast = self.file(current)?;
        let (owner, actual_name) = expected.owner();
        if current == owner && name == actual_name && Self::named_type(&ast, name) == 1 {
            self.check_type_owner(expected)?;
            return Ok(());
        }
        if Self::named_type(&ast, name) != 0 {
            return Err(failure("local/type-alias metadata type shadow"));
        }
        let bindings = Self::binding_paths(&ast, name)?;
        if bindings.len() != 1 || bindings[0] != canonical {
            return Err(failure("unknown or aliased metadata type"));
        }
        self.check_type_owner(expected)
    }
    fn value_return(&mut self, file: &str, ty: &syn::Type) -> Result<(), String> {
        let syn::Type::Path(p) = ty else {
            return Err(failure("function must return Value"));
        };
        if p.qself.is_some() || !p.attrs.is_empty() {
            return Err(failure("qualified/attributed Value type"));
        }
        let p = path(&p.path)?;
        if p == ["neovm_value", "value", "Value"] {
            self.external_value_root(file)?;
            return Ok(());
        }
        if p != ["Value"] {
            return Err(failure("function must return the known Value type"));
        }
        let ast = self.file(file)?;
        if Self::named_type(&ast, "Value") != 0 {
            return Err(failure("shadow/alias Value type"));
        }
        let imports = Self::binding_paths(&ast, "Value")?;
        if imports.len() == 1 && imports[0] == ["neovm_value", "value", "Value"] {
            self.external_value_root(file)?;
            return Ok(());
        }
        if !imports.is_empty() {
            return Err(failure("unknown Value import"));
        }
        self.value_facade(file, &ast)
    }
    fn external_value_root(&mut self, file: &str) -> Result<(), String> {
        for file in [file, LIB] {
            let ast = self.file(file)?;
            if Self::named_type(&ast, "neovm_value") != 0
                || !Self::binding_paths(&ast, "neovm_value")?.is_empty()
            {
                return Err(failure("shadowed external Value owner"));
            }
        }
        Ok(())
    }
    fn value_facade(&mut self, file: &str, ast: &syn::File) -> Result<(), String> {
        fn globs(ast: &syn::File) -> Result<Vec<Vec<String>>, String> {
            fn walk(t: &syn::UseTree, p: &mut Vec<String>, out: &mut Vec<Vec<String>>) -> bool {
                match t {
                    syn::UseTree::Path(x) => {
                        p.push(x.ident.to_string());
                        let yes = walk(&x.tree, p, out);
                        p.pop();
                        yes
                    }
                    syn::UseTree::Glob(_) => {
                        out.push(p.clone());
                        true
                    }
                    syn::UseTree::Group(x) => {
                        let mut found = false;
                        for t in &x.items {
                            found |= walk(t, p, out);
                        }
                        found
                    }
                    _ => false,
                }
            }
            let mut out = Vec::new();
            for item in &ast.items {
                if let syn::Item::Use(u) = item {
                    if walk(&u.tree, &mut Vec::new(), &mut out)
                        && (u.leading_colon.is_some() || conditional(&u.attrs))
                    {
                        return Err(failure("conditional/absolute Value facade glob"));
                    }
                }
            }
            Ok(out)
        }
        let actual = globs(ast)?;
        let expected: Vec<Vec<String>> = match file {
            INTERACTIVE => vec![vec!["super".to_owned()]],
            EVAL => vec![
                vec!["neovm_value".to_owned(), "error".to_owned()],
                vec!["neovm_value".to_owned(), "value".to_owned()],
            ],
            BUILTINS => [
                "neovm_value::value",
                "arithmetic",
                "buffer_text_backend",
                "collections",
                "cons_list",
                "from_value",
                "misc_pure",
                "strings",
                "types",
                "file_notify",
                "higher_order",
                "hooks",
                "keymaps",
                "maphash",
                "misc_eval",
                "neovm_edit::emacs_core::buffer",
                "search",
                "stubs",
                "symbols",
                "treesit",
            ]
            .iter()
            .map(|s| s.split("::").map(str::to_owned).collect())
            .collect(),
            _ => {
                return Err(failure(
                    "bare Value needs an explicit canonical import in this owner",
                ));
            }
        };
        if actual != expected {
            return Err(failure("fixed Value facade glob inventory changed"));
        }
        self.external_value_root(file)?;
        if file == INTERACTIVE {
            let parent = self.file(EVAL)?;
            if Self::named_type(&parent, "Value") != 0
                || !Self::binding_paths(&parent, "Value")?.is_empty()
            {
                return Err(failure("shadowed Eval Value facade"));
            }
            self.value_facade(EVAL, &parent)?;
        }
        Ok(())
    }
    fn check_type_owner(&mut self, expected: Ty) -> Result<(), String> {
        let (file, name) = expected.owner();
        self.owners(file)?;
        let ast = self.file(file)?;
        let declarations: Vec<_> = ast
            .items
            .iter()
            .filter(|i| match i {
                syn::Item::Enum(x) => x.ident == name,
                syn::Item::Struct(x) => x.ident == name,
                syn::Item::Type(x) => x.ident == name,
                syn::Item::Union(x) => x.ident == name,
                _ => false,
            })
            .collect();
        if declarations.len() != 1 {
            return Err(failure("metadata type definition is not unique"));
        }
        if expected == Ty::Leaf {
            let syn::Item::Struct(s) = declarations[0] else {
                return Err(failure("LeafSpec must be its actual struct"));
            };
            if conditional(&s.attrs)
                || !ordinary_attrs(&s.attrs)
                || !s.generics.params.is_empty()
                || s.generics.where_clause.is_some()
            {
                return Err(failure("conditional/generic LeafSpec"));
            }
            return Ok(());
        }
        let syn::Item::Enum(e) = declarations[0] else {
            return Err(failure("metadata constructor owner is not an enum"));
        };
        if conditional(&e.attrs)
            || !ordinary_attrs(&e.attrs)
            || !e.generics.params.is_empty()
            || e.generics.where_clause.is_some()
        {
            return Err(failure("conditional/generic metadata enum"));
        }
        let expected_names: &[&str] = if expected == Ty::Interactive {
            &["String", "Form"]
        } else {
            &["Nil", "FixnumZero", "WindowLineHeight"]
        };
        if e.variants.len() != expected_names.len() {
            return Err(failure("metadata enum variant inventory changed"));
        }
        for (variant, name) in e.variants.iter().zip(expected_names) {
            if variant.ident != *name
                || !ordinary_attrs(&variant.attrs)
                || conditional(&variant.attrs)
                || variant.discriminant.is_some()
            {
                return Err(failure("metadata enum variant changed/gated"));
            }
            if expected == Ty::Placeholder {
                if !matches!(variant.fields, syn::Fields::Unit) {
                    return Err(failure("placeholder must be a unit variant"));
                }
            } else {
                let syn::Fields::Unnamed(fields) = &variant.fields else {
                    return Err(failure("interactive variant must be a tuple"));
                };
                if fields.unnamed.len() != 1
                    || conditional(&fields.unnamed[0].attrs)
                    || !ordinary_attrs(&fields.unnamed[0].attrs)
                {
                    return Err(failure("interactive tuple payload changed"));
                }
                if *name == "String" {
                    self.declared_type(file, &fields.unnamed[0].ty, Ty::Str)?;
                } else {
                    let syn::Type::FnPtr(f) = &fields.unnamed[0].ty else {
                        return Err(failure("Form must store a plain fn pointer"));
                    };
                    if !f.attrs.is_empty()
                        || f.lifetimes.is_some()
                        || f.unsafety.is_some()
                        || f.abi.is_some()
                        || !f.inputs.is_empty()
                        || f.variadic.is_some()
                    {
                        return Err(failure("Form fn-pointer signature changed"));
                    }
                    let syn::ReturnType::Type(_, ty) = &f.output else {
                        return Err(failure("Form must return Value"));
                    };
                    self.value_return(file, ty)?;
                }
            }
        }
        Ok(())
    }
    fn declared_type(&mut self, file: &str, ty: &syn::Type, expected: Ty) -> Result<(), String> {
        if expected == Ty::Str {
            let syn::Type::Reference(r) = ty else {
                return Err(failure("string constant must be &str"));
            };
            if !r.attrs.is_empty()
                || r.mutability.is_some()
                || r.lifetime.as_ref().is_some_and(|l| l.ident != "static")
            {
                return Err(failure("string reference must be shared static"));
            }
            let syn::Type::Path(p) = r.elem.as_ref() else {
                return Err(failure("string constant must be &str"));
            };
            let ast = self.file(file)?;
            if !p.attrs.is_empty()
                || p.qself.is_some()
                || !p.path.is_ident("str")
                || Self::named_type(&ast, "str") != 0
                || !Self::binding_paths(&ast, "str")?.is_empty()
            {
                return Err(failure("not the primitive str type"));
            }
            return Ok(());
        }
        let syn::Type::Path(p) = ty else {
            return Err(failure("metadata item type mismatch"));
        };
        if !p.attrs.is_empty() || p.qself.is_some() {
            return Err(failure("qualified/attributed metadata item type"));
        }
        self.type_path(file, &path(&p.path)?, expected)
    }
    fn item(&mut self, current: &str, p: &[String]) -> Result<(String, syn::Item), String> {
        let (file, name) = self.locate(current, p)?;
        let ast = self.file(&file)?;
        let items: Vec<_> = ast
            .items
            .iter()
            .filter(|i| match i {
                syn::Item::Const(x) => x.ident == name,
                syn::Item::Static(x) => x.ident == name,
                syn::Item::Fn(x) => x.sig.ident == name,
                syn::Item::Type(x) => x.ident == name,
                syn::Item::Struct(x) => x.ident == name,
                syn::Item::Enum(x) => x.ident == name,
                syn::Item::Mod(x) => x.ident == name,
                _ => false,
            })
            .collect();
        if items.len() != 1 || !Self::binding_paths(&ast, &name)?.is_empty() {
            return Err(failure("item path missing, shadowed or aliased"));
        }
        Ok((file, items[0].clone()))
    }
    fn form(&mut self, current: &str, expr: &syn::Expr) -> Result<(), String> {
        let p = expression_path(expr)?;
        let (file, item) = self.item(current, &p)?;
        let syn::Item::Fn(f) = item else {
            return Err(failure(
                "Form requires the actual plain fn item, not a holder",
            ));
        };
        let s = &f.sig;
        if conditional(&f.attrs)
            || !ordinary_attrs(&f.attrs)
            || f.modifiers.defaultness.is_some()
            || s.constness.is_some()
            || s.asyncness.is_some()
            || !matches!(s.safety, syn::Safety::Default)
            || s.abi.is_some()
            || !s.generics.params.is_empty()
            || s.generics.where_clause.is_some()
            || !s.inputs.is_empty()
            || s.variadic.is_some()
        {
            return Err(failure(
                "Form target is not a plain unconditional fn() -> Value",
            ));
        }
        let syn::ReturnType::Type(_, ty) = &s.output else {
            return Err(failure("Form target must return Value"));
        };
        self.value_return(&file, ty)
    }
    fn leaf(&mut self, current: &str, expr: &syn::Expr) -> Result<(), String> {
        let syn::Expr::Reference(r) = expr else {
            return Err(failure("leaf requires &STATIC"));
        };
        if !r.attrs.is_empty() || r.mutability.is_some() {
            return Err(failure("leaf requires a plain shared reference"));
        }
        let p = expression_path(&r.expr)?;
        let (file, item) = self.item(current, &p)?;
        let syn::Item::Static(s) = item else {
            return Err(failure(
                "leaf requires immutable STATIC, never promoted CONST",
            ));
        };
        if !matches!(s.mutability, syn::StaticMutability::None)
            || conditional(&s.attrs)
            || !ordinary_attrs(&s.attrs)
        {
            return Err(failure("leaf static is mutable/conditional/unsupported"));
        }
        self.declared_type(&file, &s.ty, Ty::Leaf)?;
        // PEER5750: this actual LeafSpec STATIC's initializer is opaque.
        Ok(())
    }
    fn value(
        &mut self,
        current: &str,
        expr: &syn::Expr,
        expected: Ty,
        initializer: bool,
    ) -> Result<(), String> {
        if expected == Ty::Str {
            if let syn::Expr::Lit(l) = expr {
                return if l.attrs.is_empty() && matches!(l.lit, syn::Lit::Str(_)) {
                    Ok(())
                } else {
                    Err(failure("String payload must be a string literal/data item"))
                };
            }
        } else {
            let candidate = match expr {
                syn::Expr::Path(p) if p.attrs.is_empty() && p.qself.is_none() => {
                    Some((path(&p.path)?, None))
                }
                syn::Expr::Call(c) if c.attrs.is_empty() => {
                    Some((expression_path(&c.func)?, Some(&c.args)))
                }
                _ => None,
            };
            if let Some((p, args)) = candidate {
                if p.len() >= 2 {
                    let (variant, type_path) = p
                        .split_last()
                        .ok_or_else(|| failure("empty variant path"))?;
                    if self.type_path(current, type_path, expected).is_ok() {
                        if expected == Ty::Placeholder
                            && args.is_none()
                            && ["Nil", "FixnumZero", "WindowLineHeight"].contains(&variant.as_str())
                        {
                            return Ok(());
                        }
                        if expected == Ty::Interactive {
                            let args = args.ok_or_else(|| {
                                failure("interactive tuple variant needs one payload")
                            })?;
                            if args.len() != 1 {
                                return Err(failure(
                                    "interactive tuple variant needs exactly one payload",
                                ));
                            }
                            return match variant.as_str() {
                                "String" => self.value(current, &args[0], Ty::Str, false),
                                "Form" => self.form(current, &args[0]),
                                _ => Err(failure("unknown actual interactive variant")),
                            };
                        }
                        return Err(failure("unit/tuple metadata variant shape mismatch"));
                    }
                }
            }
        }
        if initializer {
            return Err(failure(
                "metadata initializer must be direct literal/enum data, not alias/factory",
            ));
        }
        let p = expression_path(expr)?;
        let (file, item) = self.item(current, &p)?;
        let (attrs, ty, expr) = match &item {
            syn::Item::Const(c)
                if c.generics.params.is_empty() && c.generics.where_clause.is_none() =>
            {
                (&c.attrs, c.ty.as_ref(), c.expr.as_ref())
            }
            syn::Item::Static(s) if matches!(s.mutability, syn::StaticMutability::None) => {
                (&s.attrs, s.ty.as_ref(), s.expr.as_ref())
            }
            _ => {
                return Err(failure(
                    "metadata data path must name an actual const/immutable static",
                ));
            }
        };
        if conditional(attrs) || !ordinary_attrs(attrs) {
            return Err(failure("conditional metadata data item"));
        }
        self.declared_type(&file, ty, expected)?;
        self.value(&file, expr, expected, true)
    }
}

pub(super) fn validate(
    root: &Path,
    home_file: &str,
    home_ast: &syn::File,
    builder: &str,
    args: &Args,
) -> Result<(), String> {
    let arity = match builder {
        "requires_eval_state" | "disabled_command" => 0,
        "interactive" | "placeholder" | "leaf" => 1,
        _ => return Err(failure("unknown metadata builder")),
    };
    if args.len() != arity {
        return Err(failure(format!("{builder} requires {arity} argument(s)")));
    }
    if arity == 0 {
        return Ok(());
    }
    let mut resolver = Resolver::new(root, home_file, home_ast);
    let home = resolver.home.to_owned();
    match builder {
        "interactive" => resolver.value(&home, &args[0], Ty::Interactive, false),
        "placeholder" => resolver.value(&home, &args[0], Ty::Placeholder, false),
        "leaf" => resolver.leaf(&home, &args[0]),
        _ => Err(failure("unknown metadata builder")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse::Parser;

    const HOME: &str = "crates/neovm-eval/src/emacs_core/lisp/native/builtins/misc_pure.rs";
    const ENUM: &str = "crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec";
    const PLACEHOLDER: &str = "crate::emacs_core::subr::NoEvalPlaceholder";
    const STATIC_LEAF: &str = "crate::emacs_core::builtins::leaves::REAL_LEAF";

    struct Fixture(tempfile::TempDir);
    impl Fixture {
        fn new() -> Self {
            let f = Self(tempfile::tempdir().unwrap());
            f.write(LIB, "pub mod emacs_core;");
            f.write(
                CORE,
                r#"
                #[path="runtime/eval/mod.rs"] pub mod eval;
                #[path="lisp/native/subr/mod.rs"] pub mod subr;
                #[path="lisp/native/builtins/mod.rs"] pub mod builtins;
            "#,
            );
            f.write(EVAL, "pub mod interactive_registry;");
            f.write(
                INTERACTIVE,
                r#"
                use neovm_value::value::Value;
                pub enum BuiltinInteractiveSpec { String(&'static str), Form(fn() -> Value) }
                pub const FORWARD: BuiltinInteractiveSpec = BuiltinInteractiveSpec::String("^p");
                pub static WIDEN: BuiltinInteractiveSpec = BuiltinInteractiveSpec::String("");
            "#,
            );
            f.write(
                SUBR,
                r#"
                pub(crate) mod leaf;
                pub enum NoEvalPlaceholder { Nil, FixnumZero, WindowLineHeight }
            "#,
            );
            f.write(LEAF, "pub struct LeafSpec;");
            f.write(BUILTINS, "pub mod misc_pure; pub mod leaves;");
            f.write(
                LEAVES,
                r#"
                #![cfg_attr(not(feature="jit"), allow(dead_code))]
                use crate::emacs_core::subr::leaf::LeafSpec;
                pub static REAL_LEAF: LeafSpec = LeafSpec::new(opaque_factory());
            "#,
            );
            f.write(
                HOME,
                r#"
                use crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec;
                fn form() -> neovm_value::value::Value { opaque_form_body() }
            "#,
            );
            f
        }
        fn write(&self, path: &str, source: &str) {
            let path = self.0.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }
        fn check(&self, builder: &str, arguments: &str) -> Result<(), String> {
            let ast = syn::parse_file(&std::fs::read_to_string(self.0.path().join(HOME)).unwrap())
                .unwrap();
            let args = Args::parse_terminated.parse_str(arguments).unwrap();
            validate(self.0.path(), HOME, &ast, builder, &args)
        }
    }

    #[test]
    fn metadata_args_accept_actual_enum_data_constants_and_trailing_commas() {
        let f = Fixture::new();
        for input in [
            format!("{ENUM}::String(\"^p\")"),
            format!("{ENUM}::String(\"^p\",),"),
            "crate::emacs_core::eval::interactive_registry::FORWARD,".into(),
            "crate::emacs_core::eval::interactive_registry::WIDEN".into(),
            "BuiltinInteractiveSpec::String(\"x\")".into(),
        ] {
            f.check("interactive", &input).unwrap();
        }
        for variant in ["Nil", "FixnumZero", "WindowLineHeight"] {
            f.check("placeholder", &format!("{PLACEHOLDER}::{variant},"))
                .unwrap();
        }
        f.check("requires_eval_state", "").unwrap();
        f.check("disabled_command", "").unwrap();
    }

    #[test]
    fn metadata_args_accept_only_plain_form_function_items_and_opaque_static_leaves() {
        let f = Fixture::new();
        f.check(
            "interactive",
            &format!("{ENUM}::Form(crate::emacs_core::builtins::misc_pure::form,)"),
        )
        .unwrap();
        f.check("interactive", &format!("{ENUM}::Form(form)"))
            .unwrap();
        // The LeafSpec::new/opaque_factory calls are deliberately not evaluated
        // or recursively admitted as argument grammar: only this static is opaque.
        f.check("leaf", &format!("&{STATIC_LEAF},")).unwrap();
    }

    #[test]
    fn metadata_args_reject_calls_factories_macros_and_fake_enum_constructors() {
        let f = Fixture::new();
        for input in [
            "CapitalizedHelper(\"x\")".to_owned(),
            format!("{ENUM}::String(factory())"),
            format!("{ENUM}::String({{ \"x\" }})"),
            format!("{ENUM}::String(concat!(\"x\"))"),
            format!("{ENUM}::String::<u8>(\"x\")"),
            format!("{ENUM}::Unknown(\"x\")"),
            format!("{ENUM}::String(\"x\",\"y\")"),
            format!("{ENUM}::String(1)"),
            "OtherProvider".into(),
            "unknown_function".into(),
        ] {
            assert!(f.check("interactive", &input).is_err(), "{input}");
        }
        for input in [
            format!("{PLACEHOLDER}::Nil()"),
            format!("{PLACEHOLDER}::Unknown"),
            "true".into(),
        ] {
            assert!(f.check("placeholder", &input).is_err(), "{input}");
        }
        assert!(f.check("requires_eval_state", "1").is_err());
        assert!(f.check("interactive", "").is_err());
        assert!(f.check("unknown_builder", "").is_err());
    }

    #[test]
    fn metadata_args_reject_form_holders_closures_casts_calls_and_signature_changes() {
        for declaration in [
            "const holder: fn() -> neovm_value::value::Value = form;",
            "static holder: fn() -> neovm_value::value::Value = form;",
            "fn holder(x:u8) -> neovm_value::value::Value { opaque() }",
            "fn holder<T>() -> neovm_value::value::Value { opaque() }",
            "const fn holder() -> neovm_value::value::Value { opaque() }",
            "async fn holder() -> neovm_value::value::Value { opaque() }",
            "unsafe fn holder() -> neovm_value::value::Value { opaque() }",
            "fn holder() -> OtherValue { opaque() }",
            "#[cfg(any())] fn holder() -> neovm_value::value::Value { opaque() }",
            "use crate::emacs_core::builtins::misc_pure::form as holder;",
        ] {
            let f = Fixture::new();
            f.write(
                HOME,
                &format!("fn form() -> neovm_value::value::Value {{ opaque() }} {declaration}"),
            );
            assert!(
                f.check("interactive", &format!("{ENUM}::Form(holder)"))
                    .is_err(),
                "{declaration}"
            );
        }
        let f = Fixture::new();
        for payload in [
            "form()",
            "|| opaque()",
            "form as fn()->neovm_value::value::Value",
            "make_form!()",
            "(form)",
        ] {
            assert!(
                f.check("interactive", &format!("{ENUM}::Form({payload})"))
                    .is_err(),
                "{payload}"
            );
        }
    }

    #[test]
    fn metadata_args_reject_leaf_constants_mutability_wrong_types_cfg_and_holders() {
        for declaration in [
            "pub const REAL_LEAF: LeafSpec = opaque();",
            "pub static mut REAL_LEAF: LeafSpec = opaque();",
            "pub static REAL_LEAF: OtherType = opaque();",
            "#[cfg(any())] pub static REAL_LEAF: LeafSpec = opaque();",
            "#[cfg_attr(feature=\"x\",cfg(any()))] pub static REAL_LEAF: LeafSpec = opaque();",
            "pub static REAL_LEAF: &'static LeafSpec = opaque();",
            "pub static REAL_LEAF: LeafSpec = opaque(); pub static REAL_LEAF: LeafSpec = opaque();",
        ] {
            let f = Fixture::new();
            f.write(
                LEAVES,
                &format!("use crate::emacs_core::subr::leaf::LeafSpec; {declaration}"),
            );
            assert!(
                f.check("leaf", &format!("&{STATIC_LEAF}")).is_err(),
                "{declaration}"
            );
        }
        let f = Fixture::new();
        for expression in [
            STATIC_LEAF.to_owned(),
            format!("&mut {STATIC_LEAF}"),
            format!("&({STATIC_LEAF})"),
            "&OtherProvider".into(),
        ] {
            assert!(f.check("leaf", &expression).is_err(), "{expression}");
        }
    }

    #[test]
    fn metadata_args_reject_data_aliases_factories_wrong_types_and_conditionals() {
        for declaration in [
            "const ARG: BuiltinInteractiveSpec = factory();",
            "const ARG: BuiltinInteractiveSpec = OTHER;",
            "const ARG: BuiltinInteractiveSpec = ARG;",
            "const ARG: SubrSpec = other_provider();",
            "static mut ARG: BuiltinInteractiveSpec = BuiltinInteractiveSpec::String(\"x\");",
            "#[cfg(any())] const ARG: BuiltinInteractiveSpec = BuiltinInteractiveSpec::String(\"x\");",
            "const ARG: BuiltinInteractiveSpec = BuiltinInteractiveSpec::String(CODE); const CODE:&str = factory();",
            "use crate::emacs_core::eval::interactive_registry::FORWARD as ARG;",
        ] {
            let f = Fixture::new();
            f.write(HOME,&format!("use crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec; {declaration}"));
            assert!(f.check("interactive", "ARG").is_err(), "{declaration}");
        }
    }

    #[test]
    fn metadata_args_reject_enum_type_shadows_renames_and_changed_actual_definitions() {
        for declaration in [
            "enum BuiltinInteractiveSpec { String(&'static str) }",
            "struct BuiltinInteractiveSpec;",
            "type BuiltinInteractiveSpec = Real;",
            "use crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec as Alias;",
        ] {
            let f = Fixture::new();
            f.write(HOME, declaration);
            let input = if declaration.contains(" as Alias") {
                "Alias::String(\"x\")"
            } else {
                "BuiltinInteractiveSpec::String(\"x\")"
            };
            assert!(f.check("interactive", input).is_err(), "{declaration}");
        }
        for source in [
            "pub fn BuiltinInteractiveSpec() {}",
            "pub enum BuiltinInteractiveSpec { String(&'static str), Form(fn()->Value), Other }",
            "#[cfg(any())] pub enum BuiltinInteractiveSpec { String(&'static str), Form(fn()->Value) }",
            "pub enum BuiltinInteractiveSpec { String(u8), Form(fn()->Value) }",
        ] {
            let f = Fixture::new();
            f.write(INTERACTIVE, source);
            assert!(
                f.check("interactive", &format!("{ENUM}::String(\"x\")"))
                    .is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn metadata_args_reject_conditional_or_redirected_owner_modules() {
        for source in [
            "#[cfg(any())] pub mod leaves; pub mod misc_pure;",
            "#[path=\"other.rs\"] pub mod leaves; pub mod misc_pure;",
            "pub mod leaves; pub mod leaves; pub mod misc_pure;",
        ] {
            let f = Fixture::new();
            f.write(BUILTINS, source);
            assert!(
                f.check("leaf", &format!("&{STATIC_LEAF}")).is_err(),
                "{source}"
            );
        }
        let f = Fixture::new();
        f.write(EVAL, "#[cfg(any())] pub mod interactive_registry;");
        assert!(
            f.check("interactive", &format!("{ENUM}::String(\"x\")"))
                .is_err()
        );
    }

    #[test]
    fn metadata_args_require_known_value_and_primitive_string_type_bindings() {
        for declaration in [
            "fn form() -> Value { opaque() }",
            "type Value = Other; fn form() -> Value { opaque() }",
            "use other::Value; fn form() -> Value { opaque() }",
            "use neovm_value::value::Value as Alias; fn form() -> Alias { opaque() }",
            "fn form() -> crate::emacs_core::value::Value { opaque() }",
        ] {
            let f = Fixture::new();
            f.write(HOME, declaration);
            assert!(
                f.check("interactive", &format!("{ENUM}::Form(form)"))
                    .is_err(),
                "{declaration}"
            );
        }
        let f = Fixture::new();
        f.write(
            HOME,
            "use neovm_value::value::Value; fn form() -> Value { opaque() }",
        );
        f.check("interactive", &format!("{ENUM}::Form(form)"))
            .unwrap();
        for import in [
            "use other::str;",
            "use other::Thing as str;",
            "type str = Other;",
        ] {
            let f = Fixture::new();
            f.write(HOME, &format!("{import} const CODE:&str=\"x\";"));
            assert!(
                f.check("interactive", &format!("{ENUM}::String(CODE)"))
                    .is_err(),
                "{import}"
            );
        }
    }

    #[test]
    fn metadata_args_resolve_real_repository_form_constants_and_static_leaf() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join("Cargo.toml").is_file() && p.join(INTERACTIVE).is_file())
            .expect("workspace root for source-only metadata smoke");
        let ast = syn::parse_file(&std::fs::read_to_string(root.join(HOME)).unwrap()).unwrap();
        for (builder, expr) in [
            (
                "interactive",
                format!("{ENUM}::Form(crate::emacs_core::builtins::goto_char_interactive_spec)"),
            ),
            (
                "interactive",
                "crate::emacs_core::eval::interactive_registry::FORWARD_CHAR_INTERACTIVE_SPEC"
                    .into(),
            ),
            (
                "interactive",
                "crate::emacs_core::eval::interactive_registry::WIDEN_INTERACTIVE_SPEC".into(),
            ),
            (
                "leaf",
                "&crate::emacs_core::builtins::leaves::GET_CHAR_PROPERTY".into(),
            ),
        ] {
            let args = Args::parse_terminated.parse_str(&expr).unwrap();
            validate(root, HOME, &ast, builder, &args).unwrap_or_else(|e| panic!("{expr}: {e}"));
        }
    }
}
