//! Fail-closed classification of the descriptor methods the source grammar uses.
//! Discovering a consuming builder does not grant it permission: every method
//! in the real `SubrSpec` impl must match an explicit classified signature.
use std::collections::BTreeSet;
use std::path::Path;
use syn::visit::{self, Visit};

const SOURCE: &str = "crates/neovm-eval/src/emacs_core/lisp/native/subr/mod.rs";

// These five entries are the complete metadata-builder classification. Keep
// constructors and observers separate, even where they also consume `self`.
const BUILDERS: &[&str] = &[
    "interactive(mut self, spec: BuiltinInteractiveSpec) -> Self",
    "requires_eval_state(mut self) -> Self",
    "placeholder(mut self, placeholder: NoEvalPlaceholder) -> Self",
    "disabled_command(mut self) -> Self",
    "leaf(mut self, leaf: &'static leaf::LeafSpec) -> Self",
];
const CONSTRUCTORS: &[&str] = &[
    "new(name: &'static str, function: NativeFn, arity: SubrArity) -> Self",
    "fixed0(name: &'static str, function: SubrFn0) -> Self",
    "fixed1(name: &'static str, function: SubrFn1, minimum: FixedMin1) -> Self",
    "fixed2(name: &'static str, function: SubrFn2, minimum: FixedMin2) -> Self",
    "fixed3(name: &'static str, function: SubrFn3, minimum: FixedMin3) -> Self",
    "fixed4(name: &'static str, function: SubrFn4, minimum: FixedMin4) -> Self",
    "fixed5(name: &'static str, function: SubrFn5, minimum: FixedMin5) -> Self",
    "fixed6(name: &'static str, function: SubrFn6, minimum: FixedMin6) -> Self",
    "native(name: &'static str, function: SubrFn, arity: SubrArity) -> Self",
    "evaluator(name: &'static str, arity: SubrArity, dispatch_kind: SubrDispatchKind) -> Self",
];
const OBSERVERS: &[&str] = &[
    "name(self) -> &'static str",
    "function(self) -> Option<SubrFn>",
    "arity(self) -> SubrArity",
    "dispatch_kind(self) -> SubrDispatchKind",
    "interactive_spec(self) -> Option<BuiltinInteractiveSpec>",
    "no_eval_policy(self) -> NoEvalPolicy",
    "command_default(self) -> CommandDefault",
    "leaf_spec(self) -> Option<&'static leaf::LeafSpec>",
];

fn fail(message: impl AsRef<str>) -> String {
    format!("SUBR_METADATA_INVENTORY: {}", message.as_ref())
}

pub(super) fn validate_classification(root: &Path) -> Result<(), String> {
    let source = std::fs::read_to_string(root.join(SOURCE))
        .map_err(|error| fail(format!("read {SOURCE}: {error}")))?;
    validate_source(&source)
}

// This is a signature projection, not an alias resolver. Only the concrete
// path/reference/Option types present in the signed inventory are supported.
fn type_key(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(ty) if ty.attrs.is_empty() && ty.qself.is_none() => {
            if ty.path.leading_colon.is_some() {
                return None;
            }
            let mut segments = Vec::new();
            for segment in &ty.path.segments {
                let mut key = segment.ident.to_string();
                match &segment.arguments {
                    syn::PathArguments::None => {}
                    syn::PathArguments::AngleBracketed(arguments)
                        if arguments.colon2_token.is_none() =>
                    {
                        let args = arguments
                            .args
                            .iter()
                            .map(|arg| match arg {
                                syn::GenericArgument::Type(ty) => type_key(ty),
                                _ => None,
                            })
                            .collect::<Option<Vec<_>>>()?;
                        key.push('<');
                        key.push_str(&args.join(","));
                        key.push('>');
                    }
                    _ => return None,
                }
                segments.push(key);
            }
            Some(segments.join("::"))
        }
        syn::Type::Reference(ty) if ty.attrs.is_empty() => Some(format!(
            "&{}{}{}",
            ty.lifetime
                .as_ref()
                .map(|lifetime| format!("'{} ", lifetime.ident))
                .unwrap_or_default(),
            if ty.mutability.is_some() { "mut " } else { "" },
            type_key(&ty.elem)?
        )),
        _ => None,
    }
}

fn signature_key(method: &syn::ImplItemFn) -> Option<String> {
    let sig = &method.sig;
    if !matches!(method.vis, syn::Visibility::Public(_))
        || method.modifiers.defaultness.is_some()
        || method.attrs.iter().any(|attr| !attr.path().is_ident("doc"))
        || sig.constness.is_none()
        || sig.asyncness.is_some()
        || !matches!(sig.safety, syn::Safety::Default)
        || sig.abi.is_some()
        || !sig.generics.params.is_empty()
        || sig.generics.where_clause.is_some()
        || sig.variadic.is_some()
    {
        return None;
    }
    let mut inputs = Vec::new();
    for arg in &sig.inputs {
        inputs.push(match arg {
            syn::FnArg::Receiver(receiver)
                if receiver.attrs.is_empty()
                    && matches!(receiver.kind, syn::ReceiverKind::Value) =>
            {
                if receiver.mutability.is_some() {
                    "mut self"
                } else {
                    "self"
                }
                .to_owned()
            }
            syn::FnArg::Typed(arg) if arg.attrs.is_empty() => {
                let syn::Pat::Ident(pattern) = arg.pat.as_ref() else {
                    return None;
                };
                if !pattern.attrs.is_empty()
                    || pattern.by_ref.is_some()
                    || pattern.mutability.is_some()
                    || pattern.subpat.is_some()
                {
                    return None;
                }
                format!("{}: {}", pattern.ident, type_key(&arg.ty)?)
            }
            _ => return None,
        });
    }
    let syn::ReturnType::Type(_, output) = &sig.output else {
        return None;
    };
    let output = type_key(output)?;
    // Both spellings denote this exact inherent impl's descriptor type.
    let output = if output == "Self" {
        "SubrSpec"
    } else {
        &output
    };
    Some(format!("{}({}) -> {output}", sig.ident, inputs.join(", ")))
}

#[derive(Default)]
struct Inventory {
    methods: BTreeSet<String>,
    error: Option<String>,
    impls: usize,
    module_depth: usize,
}

impl<'ast> Visit<'ast> for Inventory {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.module_depth += 1;
        visit::visit_item_mod(self, item);
        self.module_depth -= 1;
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        // Inspect every source cfg branch. In particular, a cfg(test) method
        // cannot evade classification by being inactive on the current host.
        let named_spec = matches!(item.self_ty.as_ref(), syn::Type::Path(ty)
            if ty.path.segments.last().is_some_and(|part| part.ident == "SubrSpec"));
        if item.trait_.is_none() && named_spec {
            self.impls += 1;
            let canonical_owner = matches!(item.self_ty.as_ref(), syn::Type::Path(ty)
                if ty.attrs.is_empty() && ty.qself.is_none() && ty.path.is_ident("SubrSpec"));
            if !canonical_owner
                || self.module_depth != 0
                || item.unsafety.is_some()
                || item.modifiers.defaultness.is_some()
                || item.modifiers.polarity.is_some()
                || !item.generics.params.is_empty()
                || item.generics.where_clause.is_some()
                || item.attrs.iter().any(|attr| !attr.path().is_ident("doc"))
            {
                self.error = Some(fail(
                    "SubrSpec impl owner/modifiers/cfg require classification",
                ));
            }
            for member in &item.items {
                let syn::ImplItem::Fn(method) = member else {
                    self.error = Some(fail(
                        "non-method SubrSpec impl item requires classification",
                    ));
                    continue;
                };
                let Some(key) = signature_key(method) else {
                    self.error = Some(fail(format!(
                        "SubrSpec::{} signature/cfg drift",
                        method.sig.ident
                    )));
                    continue;
                };
                if !self.methods.insert(key.clone()) {
                    self.error = Some(fail(format!("duplicate SubrSpec method {key}")));
                }
            }
        }
        // Other owners, including WorldSubr::serial, are not builders, but
        // nested source items still need to be visited without cfg filtering.
        visit::visit_item_impl(self, item);
    }
}

fn validate_source(source: &str) -> Result<(), String> {
    let file = syn::parse_file(source).map_err(|error| fail(format!("parse {SOURCE}: {error}")))?;
    if file.attrs.iter().any(|attr| !attr.path().is_ident("doc")) {
        return Err(fail(
            "SubrSpec source-file attributes/cfg require classification",
        ));
    }
    let mut inventory = Inventory::default();
    inventory.visit_file(&file);
    if let Some(error) = inventory.error {
        return Err(error);
    }
    if inventory.impls != 1 {
        return Err(fail("expected one classified inherent SubrSpec impl"));
    }
    let expected = BUILDERS
        .iter()
        .chain(CONSTRUCTORS)
        .chain(OBSERVERS)
        .map(|signature| signature.replace("-> Self", "-> SubrSpec"))
        .collect::<BTreeSet<_>>();
    if inventory.methods != expected {
        let added = inventory.methods.difference(&expected).collect::<Vec<_>>();
        let missing = expected.difference(&inventory.methods).collect::<Vec<_>>();
        return Err(fail(format!(
            "unclassified/drifted methods {added:?}; missing {missing:?}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = include_str!("../src/emacs_core/lisp/native/subr/mod.rs");

    #[test]
    fn actual_subr_spec_methods_have_complete_classification() {
        validate_source(REAL).unwrap();
        assert_eq!(
            (BUILDERS.len(), CONSTRUCTORS.len(), OBSERVERS.len()),
            (5, 10, 8)
        );
    }

    #[test]
    fn new_const_consuming_builder_requires_classification_even_under_cfg() {
        for gate in ["", "#[cfg(test)]", "#[cfg(windows)]", "#[cfg(any())]"] {
            let changed = REAL.replace(
                "impl SubrSpec {",
                &format!(
                    "impl SubrSpec {{ {gate} pub const fn unclassified(mut self) -> Self {{ self }}"
                ),
            );
            assert!(validate_source(&changed).is_err(), "accepted gate {gate}");
        }
    }

    #[test]
    fn builder_const_receiver_and_argument_drift_are_rejected() {
        for (before, after) in [
            (
                "pub const fn requires_eval_state(mut self)",
                "pub fn requires_eval_state(mut self)",
            ),
            (
                "requires_eval_state(mut self)",
                "requires_eval_state(&mut self)",
            ),
            ("requires_eval_state(mut self)", "requires_eval_state(self)"),
            (
                "requires_eval_state(mut self)",
                "requires_eval_state(mut self, extra: bool)",
            ),
            ("spec: BuiltinInteractiveSpec", "spec: OtherInteractiveSpec"),
            ("leaf: &'static leaf::LeafSpec", "leaf: &leaf::LeafSpec"),
            (
                "placeholder: NoEvalPlaceholder",
                "placeholder: Option<NoEvalPlaceholder>",
            ),
            ("pub const fn interactive(", "pub const fn interactive<T>("),
            (
                "pub const fn disabled_command",
                "#[cfg(test)] pub const fn disabled_command",
            ),
        ] {
            assert!(REAL.contains(before), "stale mutation {before}");
            assert!(
                validate_source(&REAL.replace(before, after)).is_err(),
                "accepted {after}"
            );
        }
    }

    #[test]
    fn other_owners_are_excluded_and_explicit_descriptor_return_is_supported() {
        let changed = REAL.replace(
            "requires_eval_state(mut self) -> Self",
            "requires_eval_state(mut self) -> SubrSpec",
        );
        validate_source(&format!(
            "{changed}\nimpl Other {{ pub const fn unclassified(self) -> Self {{ self }} }}"
        ))
        .unwrap();
    }

    #[test]
    fn file_and_impl_cfg_cannot_hide_the_inventory() {
        assert!(validate_source(&format!("#![cfg(test)]\n{REAL}")).is_err());
        assert!(
            validate_source(&REAL.replace("impl SubrSpec {", "#[cfg(test)] impl SubrSpec {"))
                .is_err()
        );
    }
}
