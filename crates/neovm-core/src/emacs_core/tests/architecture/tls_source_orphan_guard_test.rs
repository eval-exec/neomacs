//! A source-keyed classification must still name a literal TLS declaration
//! and either an exact scanner key or an explicitly validated fixture gate.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;

fn normalized_ident(ident: &proc_macro2::Ident) -> String {
    let spelling = ident.to_string();
    spelling.strip_prefix("r#").unwrap_or(&spelling).to_owned()
}

#[derive(Default)]
struct LiteralTlsNames(BTreeSet<String>, Option<String>);

impl LiteralTlsNames {
    fn declaration(&mut self, tokens: proc_macro2::TokenStream) {
        let Ok(body) = syn::parse2::<syn::File>(tokens) else {
            return;
        };
        // A TLS body is a list of static declarations. Do not recover names
        // from an unrecognized macro grammar or a substring of its tokens.
        if body
            .items
            .iter()
            .all(|item| matches!(item, syn::Item::Static(_)))
        {
            for item in body.items {
                if let syn::Item::Static(item) = item {
                    self.0.insert(normalized_ident(&item.ident));
                }
            }
        }
    }

    fn template(&mut self, tokens: proc_macro2::TokenStream) -> Result<(), String> {
        use proc_macro2::TokenTree;
        let tokens: Vec<_> = tokens.into_iter().collect();
        let mut at = 0;
        while at < tokens.len() {
            if matches!(tokens.get(at), Some(TokenTree::Punct(hash)) if hash.as_char() == '#') {
                let group_at = at
                    + 1
                    + usize::from(matches!(
                        tokens.get(at + 1), Some(TokenTree::Punct(bang)) if bang.as_char() == '!'
                    ));
                if matches!(tokens.get(group_at), Some(TokenTree::Group(_))) {
                    at = group_at + 1;
                    continue;
                }
            }
            if let [TokenTree::Ident(kind), TokenTree::Punct(bang), ..] = &tokens[at..]
                && normalized_ident(kind) == "macro_rules"
                && bang.as_char() == '!'
            {
                let (body_at, consumed) = match &tokens[at + 2..] {
                    [TokenTree::Ident(_), TokenTree::Group(_), ..] => (at + 3, 4),
                    [
                        TokenTree::Punct(dollar),
                        TokenTree::Ident(_),
                        TokenTree::Group(_),
                        ..,
                    ] if dollar.as_char() == '$' => (at + 4, 5),
                    _ => return Err("unmodelled macro_rules declaration name".into()),
                };
                let TokenTree::Group(body) = &tokens[body_at] else {
                    unreachable!("the declaration name patterns require a body group")
                };
                self.macro_rules(body.stream())?;
                at += consumed;
                continue;
            }
            if let (
                Some(TokenTree::Ident(name)),
                Some(TokenTree::Punct(bang)),
                Some(TokenTree::Group(body)),
            ) = (tokens.get(at), tokens.get(at + 1), tokens.get(at + 2))
            {
                if bang.as_char() == '!' {
                    if normalized_ident(name) == "thread_local" {
                        self.declaration(body.stream());
                    }
                    // Opaque invocations such as stringify! are not source
                    // declarations, even inside a macro_rules transcriber.
                    at += 3;
                    continue;
                }
            }
            if let TokenTree::Group(group) = &tokens[at] {
                self.template(group.stream())?;
            }
            at += 1;
        }
        Ok(())
    }

    fn macro_rules(&mut self, tokens: proc_macro2::TokenStream) -> Result<(), String> {
        use proc_macro2::TokenTree;
        let tokens: Vec<_> = tokens.into_iter().collect();
        // Inspect only arm transcribers. A literal thread_local! pattern in
        // an arm matcher does not declare a static. Unknown grammar cannot
        // silently turn matcher tokens into declaration evidence.
        let mut arm = 0;
        while arm < tokens.len() {
            let [
                TokenTree::Group(_),
                TokenTree::Punct(eq),
                TokenTree::Punct(gt),
                TokenTree::Group(body),
                ..,
            ] = &tokens[arm..]
            else {
                return Err("unmodelled macro_rules arm".into());
            };
            if eq.as_char() != '='
                || eq.spacing() != proc_macro2::Spacing::Joint
                || gt.as_char() != '>'
            {
                return Err("unmodelled macro_rules arm arrow".into());
            }
            self.template(body.stream())?;
            arm += 4;
            if arm < tokens.len() {
                if !matches!(&tokens[arm], TokenTree::Punct(separator) if separator.as_char() == ';')
                {
                    return Err("unmodelled macro_rules arm separator".into());
                }
                arm += 1;
            }
        }
        Ok(())
    }
}

impl<'ast> Visit<'ast> for LiteralTlsNames {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if mac
            .path
            .segments
            .last()
            .is_some_and(|part| normalized_ident(&part.ident) == "thread_local")
        {
            self.declaration(mac.tokens.clone());
        } else if mac.path.segments.len() == 1
            && mac
                .path
                .segments
                .first()
                .is_some_and(|part| normalized_ident(&part.ident) == "macro_rules")
        {
            if let Err(error) = self.macro_rules(mac.tokens.clone()) {
                self.1.get_or_insert(error);
            }
        }
    }
}

fn literal_tls_names(source: &str) -> Result<BTreeSet<String>, String> {
    let syntax = syn::parse_file(source).map_err(|error| error.to_string())?;
    let mut names = LiteralTlsNames::default();
    names.visit_file(&syntax);
    match names.1 {
        Some(error) => Err(error),
        None => Ok(names.0),
    }
}

fn source_orphan_violations(
    rows: &[(&str, &str, TlsClass)],
    scanned: &[(String, String)],
    enclosing: &BTreeSet<(String, String)>,
    mut source_names: impl FnMut(&str) -> Result<BTreeSet<String>, String>,
) -> Vec<String> {
    let scanned: BTreeSet<_> = scanned.iter().cloned().collect();
    let mut files = BTreeMap::new();
    let mut violations = Vec::new();
    for &(file, name, _) in rows {
        if file == BINARY_CENSUS {
            continue;
        }
        let names = files.entry(file).or_insert_with(|| source_names(file));
        match names {
            Ok(names) if names.contains(name) => {}
            Ok(_) => {
                violations.push(format!(
                    "{file}::{name}: no literal TLS static in its source file"
                ));
                continue;
            }
            Err(error) => {
                violations.push(format!(
                    "{file}::{name}: cannot establish source declaration: {error}"
                ));
                continue;
            }
        }
        let key = (name.to_owned(), file.to_owned());
        if !scanned.contains(&key) && !enclosing.contains(&key) {
            violations.push(format!(
                "{file}::{name}: no exact scanner key or validated enclosing fixture gate"
            ));
        }
    }
    violations
}

fn validated_enclosing_keys(root: &Path) -> BTreeSet<(String, String)> {
    let mut enclosing = BTreeSet::new();
    for gate in ENCLOSING_TEST_GATES {
        resolve_enclosing_test_gate(root, gate)
            .unwrap_or_else(|error| panic!("invalid enclosing gate {}: {error}", gate.module));
        let file = gate
            .resolved_files
            .last()
            .expect("validated route has an owner file");
        enclosing.extend(
            gate.names
                .iter()
                .map(|name| ((*name).to_owned(), (*file).to_owned())),
        );
    }
    enclosing
}

#[test]
fn source_keyed_tls_rows_have_live_source_and_scan_or_gate_proof() {
    let root = workspace_crates_dir();
    let enclosing = validated_enclosing_keys(&root);
    let violations = source_orphan_violations(
        TLS_CLASSIFICATION,
        &scanned_tls_names(),
        &enclosing,
        |file| {
            let source =
                std::fs::read_to_string(root.join(file)).map_err(|error| error.to_string())?;
            literal_tls_names(&source)
        },
    );
    assert!(
        violations.is_empty(),
        "orphaned source TLS classifications: {violations:#?}"
    );
}

#[test]
fn validated_fixture_gate_covers_its_exact_source_key_without_a_scan_hit() {
    let root = workspace_crates_dir();
    let rows = [(
        "neovm-value/src/tls_scope.rs",
        "SCALAR",
        TlsClass::SourceOnly,
    )];
    let enclosing = validated_enclosing_keys(&root);
    let violations = source_orphan_violations(&rows, &[], &enclosing, |file| {
        literal_tls_names(&std::fs::read_to_string(root.join(file)).unwrap())
    });
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn planted_source_orphan_and_wrong_file_key_are_rejected() {
    let rows = [
        ("owner.rs", "LIVE", TlsClass::MustReset),
        ("owner.rs", "REMOVED", TlsClass::MustReset),
        ("moved.rs", "LIVE", TlsClass::MustReset),
    ];
    let scanned = vec![("LIVE".into(), "owner.rs".into())];
    let violations = source_orphan_violations(&rows, &scanned, &BTreeSet::new(), |file| {
        literal_tls_names(if file == "owner.rs" {
            "thread_local! { static LIVE: Cell<u8> = Cell::new(0); }"
        } else {
            ""
        })
    });
    assert_eq!(violations.len(), 2, "{violations:?}");
    assert!(violations[0].contains("owner.rs::REMOVED"));
    assert!(violations[1].contains("moved.rs::LIVE"));
}

#[test]
fn scanner_hits_in_comments_strings_and_opaque_macros_are_not_declarations() {
    let source = r#"
        // thread_local! { static COMMENT: Cell<u8> = Cell::new(0); }
        const EXAMPLE: &str = "thread_local! { static STRING: Cell<u8> = Cell::new(0); }";
        stringify!(thread_local! { static OPAQUE: Cell<u8> = Cell::new(0); });
    "#;
    let mut scanned = Vec::new();
    scan_tls_source(source, "owner.rs", &mut scanned);
    assert!(scanned.iter().any(|(name, _)| name == "COMMENT"));
    let rows = [
        ("owner.rs", "COMMENT", TlsClass::SourceOnly),
        ("owner.rs", "STRING", TlsClass::SourceOnly),
        ("owner.rs", "OPAQUE", TlsClass::SourceOnly),
    ];
    let violations = source_orphan_violations(&rows, &scanned, &BTreeSet::new(), |_| {
        literal_tls_names(source)
    });
    assert_eq!(violations.len(), 3, "{violations:?}");
}

#[test]
fn literal_tls_proof_includes_local_statics_and_macro_rules_transcribers() {
    let names = literal_tls_names(
        r#"
        fn fixture() { std::thread_local! { static LOCAL: Cell<u8> = Cell::new(0); } }
        fn raw_fixture() { std::r#thread_local! { static r#RAW_LOCAL: () = (); } }
        macro_rules! raw_maker {
            ($name:ident) => {
                r#macro_rules! $name {
                    (thread_local! { static RAW_GENERATED_MATCHER: () = (); }) => {
                        r#thread_local! { static r#RAW_GENERATED_TEMPLATE: () = (); }
                    };
                }
            };
        }
        macro_rules! declare {
            (thread_local! { static MATCHER: () = (); }) => {
                fn generated() {
                    thread_local! { static TEMPLATE: Cell<u8> = Cell::new(0); }
                    stringify!(thread_local! { static OPAQUE: () = (); });
                }
                #[example(thread_local! { static ATTRIBUTE: () = (); })]
                fn attributed() {}
                macro_rules! nested {
                    (thread_local! { static NESTED_MATCHER: () = (); }) => {};
                }
                macro_rules! $name {
                    (thread_local! { static GENERATED_MATCHER: () = (); }) => {
                        thread_local! { static GENERATED_TEMPLATE: () = (); }
                    };
                }
            };
        }
    "#,
    )
    .unwrap();
    assert_eq!(
        names,
        BTreeSet::from([
            "LOCAL".to_owned(),
            "TEMPLATE".to_owned(),
            "GENERATED_TEMPLATE".to_owned(),
            "RAW_LOCAL".to_owned(),
            "RAW_GENERATED_TEMPLATE".to_owned()
        ])
    );
    for source in [
        "macro_rules! broken { unmodelled }",
        "macro_rules! broken { () -> {} }",
        "macro_rules! broken { () = > {} }",
        "macro_rules! broken { () => {}, }",
        "macro_rules! outer { () => { macro_rules! #unknown { () => {}; } }; }",
    ] {
        assert!(literal_tls_names(source).is_err(), "{source}");
    }
}

#[test]
fn only_an_explicit_validated_gate_can_cover_a_missing_scanner_key() {
    let rows = [("owner.rs", "GATED", TlsClass::SourceOnly)];
    let source = "#[cfg(test)] thread_local! { static GATED: Cell<u8> = Cell::new(0); }";
    let read = |_: &str| literal_tls_names(source);
    let violations = source_orphan_violations(&rows, &[], &BTreeSet::new(), read);
    assert_eq!(
        violations.len(),
        1,
        "direct item gates do not implicitly authorize a stale row"
    );
    let explicit_gate = BTreeSet::from([("GATED".to_owned(), "owner.rs".to_owned())]);
    assert!(source_orphan_violations(&rows, &[], &explicit_gate, read).is_empty());
    let wrong_gate = BTreeSet::from([("GATED".to_owned(), "other.rs".to_owned())]);
    assert_eq!(
        source_orphan_violations(&rows, &[], &wrong_gate, read).len(),
        1
    );
}
