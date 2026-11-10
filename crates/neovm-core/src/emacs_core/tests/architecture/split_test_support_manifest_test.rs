//! A2: a provider's test surface is enabled only from above, by a package
//! `[dev-dependencies]` edge or by the enabling crate's own `test-support`
//! feature forwarding to a lower provider's `test-support`. Enabling through
//! a normal or build dependency is refused for every crate; the normal
//! workspace dependencies (including core's) carry no test-support feature.
//! This is test-side tooling: all Cargo targets/features are inspected, not
//! merely the dependencies selected by this test process's configuration.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use syn::{Expr, Item, Lit, Meta};
use toml::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ErrorCode {
    FeatureForwardEscape,
    FeatureMissing,
    ForeignEnabler,
    LibRead,
    ManifestParse,
    ManifestRead,
    ManifestShape,
    Metadata,
    MetadataParse,
    ModuleCycle,
    ModuleInclude,
    ModuleMacro,
    ModuleMissingOrAmbiguous,
    ModulePath,
    ModuleRead,
    NonDevEnabler,
    PackageIdentity,
    RootManifest,
    SourceParse,
    StateData,
    SurfaceMissingOrDuplicate,
    SurfaceState,
    TestSupportUngated,
    UnknownCfg,
    ValueLibMissing,
    ValueLibMissingOrDuplicate,
    ValuePackageMissingOrDuplicate,
    WorkspaceDependenciesMissing,
    WorkspaceDependencyMissing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GuardError {
    code: ErrorCode,
    detail: String,
}

impl GuardError {
    fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    fn is(&self, code: ErrorCode) -> bool {
        self.code == code
    }
}

impl std::fmt::Display for GuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

type GuardResult<T> = Result<T, GuardError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SurfaceState {
    Absent,
    Present,
}

impl SurfaceState {
    fn parse(text: &str) -> GuardResult<Self> {
        match text {
            "crate\tstate\nneovm-value\tabsent\n" => Ok(Self::Absent),
            "crate\tstate\nneovm-value\tpresent\n" => Ok(Self::Present),
            _ => Err(GuardError::new(
                ErrorCode::StateData,
                "A2_STATE_DATA: expected exactly one neovm-value state row",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DependencyKind {
    Normal,
    Dev,
    Build,
}

impl DependencyKind {
    const TABLES: [(&str, Self); 3] = [
        ("dependencies", Self::Normal),
        ("dev-dependencies", Self::Dev),
        ("build-dependencies", Self::Build),
    ];
}

#[derive(Debug)]
struct Manifest {
    package: String,
    path: PathBuf,
    data: Value,
}

impl Manifest {
    fn parse(package: &str, path: PathBuf, source: &str) -> GuardResult<Self> {
        let data: Value = toml::from_str(source).map_err(|error| {
            GuardError::new(
                ErrorCode::ManifestParse,
                format!("A2_MANIFEST_PARSE {}: {error}", path.display()),
            )
        })?;
        let actual = data
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(Value::as_str);
        if actual != Some(package) {
            return Err(GuardError::new(
                ErrorCode::PackageIdentity,
                format!(
                    "A2_PACKAGE_IDENTITY {}: {actual:?} != {package}",
                    path.display()
                ),
            ));
        }
        Ok(Self {
            package: package.into(),
            path,
            data,
        })
    }
}

fn strings(value: Option<&Value>, site: &str) -> GuardResult<Vec<String>> {
    match value {
        None => Ok(Vec::new()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value.as_str().map(str::to_owned).ok_or_else(|| {
                    GuardError::new(
                        ErrorCode::ManifestShape,
                        format!("A2_MANIFEST_SHAPE {site}: expected feature string"),
                    )
                })
            })
            .collect(),
        Some(_) => Err(GuardError::new(
            ErrorCode::ManifestShape,
            format!("A2_MANIFEST_SHAPE {site}: expected feature array"),
        )),
    }
}

#[derive(Debug)]
struct Dependency {
    package: String,
    features: Vec<String>,
    defaults: bool,
}

fn dependency(
    alias: &str,
    raw: &Value,
    inherited: &toml::map::Map<String, Value>,
) -> GuardResult<Dependency> {
    let table = match raw {
        Value::String(_) => None,
        Value::Table(table) => Some(table),
        _ => {
            return Err(GuardError::new(
                ErrorCode::ManifestShape,
                format!("A2_MANIFEST_SHAPE {alias}: invalid dependency spec"),
            ));
        }
    };
    let workspace = match table.and_then(|table| table.get("workspace")) {
        None | Some(Value::Boolean(false)) => false,
        Some(Value::Boolean(true)) => true,
        Some(_) => {
            return Err(GuardError::new(
                ErrorCode::ManifestShape,
                format!("A2_MANIFEST_SHAPE {alias}: workspace is not boolean"),
            ));
        }
    };
    let mut result = if workspace {
        let parent = inherited.get(alias).ok_or_else(|| {
            GuardError::new(
                ErrorCode::WorkspaceDependencyMissing,
                format!("A2_WORKSPACE_DEPENDENCY_MISSING {alias}"),
            )
        })?;
        let mut value = dependency(alias, parent, &toml::map::Map::new())?;
        value
            .features
            .extend(strings(table.and_then(|t| t.get("features")), alias)?);
        value
    } else {
        let package = match table.and_then(|t| t.get("package")) {
            None => alias,
            Some(Value::String(name)) => name,
            Some(_) => {
                return Err(GuardError::new(
                    ErrorCode::ManifestShape,
                    format!("A2_MANIFEST_SHAPE {alias}: package is not string"),
                ));
            }
        };
        Dependency {
            package: package.into(),
            features: strings(table.and_then(|t| t.get("features")), alias)?,
            defaults: true,
        }
    };
    if let Some(value) = table.and_then(|t| t.get("default-features")) {
        result.defaults = value.as_bool().ok_or_else(|| {
            GuardError::new(
                ErrorCode::ManifestShape,
                format!("A2_MANIFEST_SHAPE {alias}: default-features is not boolean"),
            )
        })?;
    }
    Ok(result)
}

fn features(manifest: &Manifest) -> GuardResult<BTreeMap<String, Vec<String>>> {
    match manifest.data.get("features") {
        None => Ok(BTreeMap::new()),
        Some(Value::Table(table)) => table
            .iter()
            .map(|(name, value)| Ok((name.clone(), strings(Some(value), name)?)))
            .collect(),
        Some(_) => Err(GuardError::new(
            ErrorCode::ManifestShape,
            format!(
                "A2_MANIFEST_SHAPE {}: features is not table",
                manifest.package
            ),
        )),
    }
}

fn reaches_surface(
    feature: &str,
    definitions: &BTreeMap<String, Vec<String>>,
    visited: &mut BTreeSet<String>,
) -> GuardResult<bool> {
    if feature == "test-support" {
        return Ok(true);
    }
    if !visited.insert(feature.into()) {
        return Ok(false);
    }
    let members = definitions.get(feature).ok_or_else(|| {
        GuardError::new(
            ErrorCode::FeatureMissing,
            format!("A2_FEATURE_MISSING {feature}"),
        )
    })?;
    for member in members {
        if !member.contains('/')
            && !member.starts_with("dep:")
            && definitions.contains_key(member)
            && reaches_surface(member, definitions, visited)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DependencyScope {
    Package,
    Target(String),
}

struct DependencyTable<'a> {
    scope: DependencyScope,
    kind: DependencyKind,
    entries: &'a toml::map::Map<String, Value>,
}

fn dependency_tables(manifest: &Manifest) -> GuardResult<Vec<DependencyTable<'_>>> {
    let root = manifest.data.as_table().ok_or_else(|| {
        GuardError::new(
            ErrorCode::ManifestShape,
            "A2_MANIFEST_SHAPE: manifest is not table",
        )
    })?;
    let mut scopes = vec![(DependencyScope::Package, root)];
    if let Some(targets) = root.get("target") {
        let targets = targets.as_table().ok_or_else(|| {
            GuardError::new(
                ErrorCode::ManifestShape,
                "A2_MANIFEST_SHAPE: target is not table",
            )
        })?;
        for (target, value) in targets {
            scopes.push((
                DependencyScope::Target(target.clone()),
                value.as_table().ok_or_else(|| {
                    GuardError::new(
                        ErrorCode::ManifestShape,
                        format!("A2_MANIFEST_SHAPE: target {target} is not table"),
                    )
                })?,
            ));
        }
    }
    let mut result = Vec::new();
    for (scope, declarations) in scopes {
        for (name, kind) in DependencyKind::TABLES {
            if let Some(value) = declarations.get(name) {
                result.push(DependencyTable {
                    scope: scope.clone(),
                    kind,
                    entries: value.as_table().ok_or_else(|| {
                        GuardError::new(
                            ErrorCode::ManifestShape,
                            format!("A2_MANIFEST_SHAPE: {name} is not table"),
                        )
                    })?,
                });
            }
        }
    }
    Ok(result)
}

/// Every workspace crate that declares a `test-support` feature is a
/// provider. The feature name is the class-wide identity: a crate may not
/// dev-enable or forward a provider's surface unless it sits above that
/// provider.
fn provider_features(
    manifests: &[Manifest],
) -> GuardResult<BTreeMap<String, BTreeMap<String, Vec<String>>>> {
    let mut providers = BTreeMap::new();
    for manifest in manifests {
        let definitions = features(manifest)?;
        if definitions.contains_key("test-support") {
            providers.insert(manifest.package.clone(), definitions);
        }
    }
    Ok(providers)
}

/// Production (normal) dependency edges only. Dev and build edges are
/// excluded: a test-only consumer is not "above" its provider.
fn normal_dependency_graph(
    manifests: &[Manifest],
    inherited: &toml::map::Map<String, Value>,
) -> GuardResult<BTreeMap<String, BTreeSet<String>>> {
    let mut graph = BTreeMap::new();
    for manifest in manifests {
        let mut packages = BTreeSet::new();
        for table in dependency_tables(manifest)? {
            if table.kind != DependencyKind::Normal {
                continue;
            }
            for (alias, raw) in table.entries {
                packages.insert(dependency(alias, raw, inherited)?.package);
            }
        }
        graph.insert(manifest.package.clone(), packages);
    }
    Ok(graph)
}

/// Whether `package` reaches `provider` through normal dependencies.
fn is_above(graph: &BTreeMap<String, BTreeSet<String>>, package: &str, provider: &str) -> bool {
    let mut pending = vec![package.to_owned()];
    let mut visited = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        let Some(dependencies) = graph.get(&current) else {
            continue;
        };
        for dependency in dependencies {
            if dependency == provider {
                return true;
            }
            if graph.contains_key(dependency) {
                pending.push(dependency.clone());
            }
        }
    }
    false
}

fn check_manifests(
    manifests: &[Manifest],
    inherited: &toml::map::Map<String, Value>,
    state: SurfaceState,
) -> GuardResult<()> {
    let values: Vec<_> = manifests
        .iter()
        .filter(|p| p.package == "neovm-value")
        .collect();
    let [value] = values.as_slice() else {
        return Err(GuardError::new(
            ErrorCode::ValuePackageMissingOrDuplicate,
            "A2_VALUE_PACKAGE_MISSING_OR_DUPLICATE",
        ));
    };
    let value_features = features(value)?;
    let declared = value_features.contains_key("test-support");
    if declared != (state == SurfaceState::Present) {
        return Err(GuardError::new(
            ErrorCode::SurfaceState,
            format!("A2_SURFACE_STATE: {state:?}, feature declared={declared}"),
        ));
    }
    let providers = provider_features(manifests)?;
    // No provider feature other than `test-support` may reach that provider's
    // own test surface. This covers `default` and every product feature.
    // `test-thread-fallback-heap` is the one test-only exception: it implies
    // `test-support` so `thread_exit_leak`'s existing cfg arm compiles. A
    // product feature that reaches the surface through that implication is
    // still refused below, because this skip names only the fallback feature.
    for (provider, definitions) in &providers {
        for name in definitions
            .keys()
            .filter(|name| name.as_str() != "test-support")
        {
            if provider == "neovm-value" && name == "test-thread-fallback-heap" {
                continue;
            }
            if reaches_surface(name, definitions, &mut BTreeSet::new())? {
                return Err(GuardError::new(
                    ErrorCode::FeatureForwardEscape,
                    format!("A2_FEATURE_FORWARD_ESCAPE {provider}/{name} -> test-support"),
                ));
            }
        }
    }
    // Preserve Prep E's original prohibition on enabling a provider's surface
    // from workspace.dependencies, including an unused spec. It is not an
    // owner-scoped dev edge and must not become a latent enabling route.
    for (alias, raw) in inherited {
        let dep = dependency(alias, raw, &toml::map::Map::new())?;
        let Some(definitions) = providers.get(&dep.package) else {
            continue;
        };
        for feature in &dep.features {
            if reaches_surface(feature, definitions, &mut BTreeSet::new())? {
                return Err(GuardError::new(
                    ErrorCode::NonDevEnabler,
                    format!(
                        "A2_NON_DEV_ENABLER workspace.dependencies alias={alias} -> {}",
                        dep.package
                    ),
                ));
            }
        }
    }
    let graph = normal_dependency_graph(manifests, inherited)?;
    for manifest in manifests {
        let above_provider = |provider: &str| is_above(&graph, &manifest.package, provider);
        let mut aliases: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut normal_aliases: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for table in dependency_tables(manifest)? {
            let kind = table.kind;
            for (alias, raw) in table.entries {
                let dep = dependency(alias, raw, inherited)?;
                let Some(definitions) = providers.get(&dep.package) else {
                    continue;
                };
                aliases
                    .entry(dep.package.clone())
                    .or_default()
                    .insert(alias.clone());
                if kind == DependencyKind::Normal {
                    normal_aliases
                        .entry(dep.package.clone())
                        .or_default()
                        .insert(alias.clone());
                }
                let direct = dep.features.iter().any(|f| f == "test-support");
                let indirect = dep
                    .features
                    .iter()
                    .filter(|f| f.as_str() != "test-support")
                    .map(|f| reaches_surface(f, definitions, &mut BTreeSet::new()))
                    .collect::<GuardResult<Vec<_>>>()?
                    .into_iter()
                    .any(|v| v);
                let default = dep.defaults
                    && definitions.contains_key("default")
                    && reaches_surface("default", definitions, &mut BTreeSet::new())?;
                if !(direct || indirect || default) {
                    continue;
                }
                if kind != DependencyKind::Dev || table.scope != DependencyScope::Package {
                    return Err(GuardError::new(
                        ErrorCode::NonDevEnabler,
                        format!(
                            "A2_NON_DEV_ENABLER {} {:?} {kind:?} alias={alias} -> {} at {}",
                            manifest.package,
                            table.scope,
                            dep.package,
                            manifest.path.display()
                        ),
                    ));
                }
                if !above_provider(&dep.package) {
                    return Err(GuardError::new(
                        ErrorCode::ForeignEnabler,
                        format!(
                            "A2_FOREIGN_ENABLER {} is not above {} ({kind:?} alias={alias}) at {}",
                            manifest.package,
                            dep.package,
                            manifest.path.display()
                        ),
                    ));
                }
            }
        }
        for (feature, members) in features(manifest)? {
            for member in members {
                let Some((alias, requested)) = member.split_once('/') else {
                    continue;
                };
                let alias = alias.trim_end_matches('?');
                for (provider, definitions) in &providers {
                    if !aliases
                        .get(provider)
                        .is_some_and(|names| names.contains(alias))
                    {
                        continue;
                    }
                    if !reaches_surface(requested, definitions, &mut BTreeSet::new())? {
                        continue;
                    }
                    let downward = feature == "test-support"
                        && normal_aliases
                            .get(provider)
                            .is_some_and(|names| names.contains(alias))
                        && above_provider(provider);
                    if !downward {
                        return Err(GuardError::new(
                            ErrorCode::FeatureForwardEscape,
                            format!(
                                "A2_FEATURE_FORWARD_ESCAPE {}/{feature}: {member}",
                                manifest.package
                            ),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Truth {
    No,
    Yes,
    Unknown,
}

fn truth(meta: &Meta, surface: bool) -> GuardResult<Truth> {
    match meta {
        Meta::NameValue(value) if value.path.is_ident("feature") => {
            let Expr::Lit(expr) = &value.value else {
                return Err(GuardError::new(
                    ErrorCode::UnknownCfg,
                    "A2_UNKNOWN_CFG: nonliteral feature",
                ));
            };
            let Lit::Str(name) = &expr.lit else {
                return Err(GuardError::new(
                    ErrorCode::UnknownCfg,
                    "A2_UNKNOWN_CFG: nonstring feature",
                ));
            };
            Ok(if name.value() == "test-support" {
                if surface { Truth::Yes } else { Truth::No }
            } else {
                Truth::Unknown
            })
        }
        Meta::Path(_) | Meta::NameValue(_) => Ok(Truth::Unknown),
        Meta::List(list) => {
            let values = list
                .parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )
                .map_err(|e| {
                    GuardError::new(ErrorCode::UnknownCfg, format!("A2_UNKNOWN_CFG: {e}"))
                })?;
            let values = values
                .iter()
                .map(|m| truth(m, surface))
                .collect::<GuardResult<Vec<_>>>()?;
            if list.path.is_ident("all") {
                Ok(if values.contains(&Truth::No) {
                    Truth::No
                } else if values.contains(&Truth::Unknown) {
                    Truth::Unknown
                } else {
                    Truth::Yes
                })
            } else if list.path.is_ident("any") {
                Ok(if values.contains(&Truth::Yes) {
                    Truth::Yes
                } else if values.contains(&Truth::Unknown) {
                    Truth::Unknown
                } else {
                    Truth::No
                })
            } else if list.path.is_ident("not") && values.len() == 1 {
                Ok(match values[0] {
                    Truth::No => Truth::Yes,
                    Truth::Yes => Truth::No,
                    Truth::Unknown => Truth::Unknown,
                })
            } else {
                Err(GuardError::new(
                    ErrorCode::UnknownCfg,
                    "A2_UNKNOWN_CFG: unknown predicate or arity",
                ))
            }
        }
    }
}

fn check_surface_declaration(source: &str, state: SurfaceState) -> GuardResult<()> {
    let file = syn::parse_file(source)
        .map_err(|e| GuardError::new(ErrorCode::SourceParse, format!("A2_SOURCE_PARSE: {e}")))?;
    let mut count = 0;
    for item in &file.items {
        let Item::Mod(module) = item else {
            continue;
        };
        if module.ident != "test_support" {
            continue;
        }
        count += 1;
        let predicates = module
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("cfg"))
            .map(|a| {
                a.parse_args::<Meta>().map_err(|e| {
                    GuardError::new(ErrorCode::UnknownCfg, format!("A2_UNKNOWN_CFG: {e}"))
                })
            })
            .collect::<GuardResult<Vec<_>>>()?;
        let combined = |surface| -> GuardResult<Truth> {
            let values = predicates
                .iter()
                .map(|p| truth(p, surface))
                .collect::<GuardResult<Vec<_>>>()?;
            Ok(if values.contains(&Truth::No) {
                Truth::No
            } else if values.contains(&Truth::Unknown) {
                Truth::Unknown
            } else {
                Truth::Yes
            })
        };
        if combined(false)? != Truth::No || combined(true)? != Truth::Yes {
            return Err(GuardError::new(
                ErrorCode::TestSupportUngated,
                "A2_TEST_SUPPORT_UNGATED: module must exist exactly under feature=test-support",
            ));
        }
        if module.attrs.iter().any(|a| a.path().is_ident("cfg_attr")) {
            return Err(GuardError::new(
                ErrorCode::UnknownCfg,
                "A2_UNKNOWN_CFG: test_support cfg_attr requires explicit review",
            ));
        }
    }
    match (state, count) {
        (SurfaceState::Absent, 0) | (SurfaceState::Present, 1) => Ok(()),
        _ => Err(GuardError::new(
            ErrorCode::SurfaceMissingOrDuplicate,
            format!("A2_SURFACE_MISSING_OR_DUPLICATE {state:?}: {count}"),
        )),
    }
}

/// Validate every declared value module even when this host's cfg disables it.
/// The surface itself has a crate-root identity; nested lookalikes cannot stand
/// in for it. No missing module is silently skipped.
fn check_module_tree(root: &Path, state: SurfaceState) -> GuardResult<()> {
    fn items(
        items: &[Item],
        directory: &Path,
        path_directory: &Path,
        seen: &mut BTreeSet<PathBuf>,
    ) -> GuardResult<()> {
        for item in items {
            match item {
                Item::Mod(module) => {
                    let mut explicit = None;
                    for attr in &module.attrs {
                        if attr.path().is_ident("path") {
                            let Meta::NameValue(meta) = &attr.meta else {
                                return Err(GuardError::new(
                                    ErrorCode::ModulePath,
                                    "A2_MODULE_PATH: nonliteral path",
                                ));
                            };
                            let Expr::Lit(value) = &meta.value else {
                                return Err(GuardError::new(
                                    ErrorCode::ModulePath,
                                    "A2_MODULE_PATH: nonliteral path",
                                ));
                            };
                            let Lit::Str(value) = &value.lit else {
                                return Err(GuardError::new(
                                    ErrorCode::ModulePath,
                                    "A2_MODULE_PATH: nonstring path",
                                ));
                            };
                            if explicit
                                .replace(path_directory.join(value.value()))
                                .is_some()
                            {
                                return Err(GuardError::new(
                                    ErrorCode::ModulePath,
                                    "A2_MODULE_PATH: duplicate path",
                                ));
                            }
                        }
                        if attr.path().is_ident("cfg_attr") {
                            // Choosing a PATH from just this host's cfg would
                            // leave another configuration unscanned. Other
                            // conditional attributes cannot change which file
                            // the module loads (visibility::make(pub), allow,
                            // doc, ...), and this walk visits the declared
                            // module regardless of cfg, so they stay accepted.
                            let args = attr
                                .parse_args_with(
                                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                                )
                                .map_err(|e| {
                                    GuardError::new(
                                        ErrorCode::ModulePath,
                                        format!("A2_MODULE_PATH: cfg_attr: {e}"),
                                    )
                                })?;
                            let mut metas = args.into_iter();
                            if metas.next().is_none() {
                                return Err(GuardError::new(
                                    ErrorCode::ModulePath,
                                    "A2_MODULE_PATH: cfg_attr without a predicate",
                                ));
                            }
                            if metas.any(|meta| {
                                meta.path().is_ident("path") || meta.path().is_ident("cfg_attr")
                            }) {
                                return Err(GuardError::new(
                                    ErrorCode::ModulePath,
                                    format!(
                                        "A2_MODULE_PATH: conditional module attributes require explicit resolution: {}",
                                        module.ident
                                    ),
                                ));
                            }
                        }
                    }
                    if let Some((_, children)) = &module.content {
                        let child = directory.join(module.ident.to_string());
                        items_walk(children, &child, seen)?;
                    } else {
                        let path = if let Some(path) = explicit {
                            path
                        } else {
                            let plain = directory.join(format!("{}.rs", module.ident));
                            let nested = directory.join(module.ident.to_string()).join("mod.rs");
                            match (plain.is_file(), nested.is_file()) {
                                (true, false) => plain,
                                (false, true) => nested,
                                _ => {
                                    return Err(GuardError::new(
                                        ErrorCode::ModuleMissingOrAmbiguous,
                                        format!(
                                            "A2_MODULE_MISSING_OR_AMBIGUOUS: {} under {}",
                                            module.ident,
                                            directory.display()
                                        ),
                                    ));
                                }
                            }
                        };
                        file(&path, seen)?;
                    }
                }
                Item::Macro(item) if item.mac.path.is_ident("include") => {
                    let literal: syn::LitStr =
                        syn::parse2(item.mac.tokens.clone()).map_err(|e| {
                            GuardError::new(
                                ErrorCode::ModuleInclude,
                                format!("A2_MODULE_INCLUDE: {e}"),
                            )
                        })?;
                    file(&path_directory.join(literal.value()), seen)?;
                }
                Item::Macro(item) => {
                    fn mentions_surface(tokens: proc_macro2::TokenStream) -> bool {
                        tokens.into_iter().any(|token| match token {
                            proc_macro2::TokenTree::Ident(name) => name == "test_support",
                            proc_macro2::TokenTree::Group(group) => {
                                mentions_surface(group.stream())
                            }
                            _ => false,
                        })
                    }
                    if mentions_surface(item.mac.tokens.clone()) {
                        return Err(GuardError::new(
                            ErrorCode::ModuleMacro,
                            "A2_MODULE_MACRO: generated test_support declaration requires explicit resolution",
                        ));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn items_walk(
        children: &[Item],
        directory: &Path,
        seen: &mut BTreeSet<PathBuf>,
    ) -> GuardResult<()> {
        items(children, directory, directory, seen)
    }
    fn file(path: &Path, seen: &mut BTreeSet<PathBuf>) -> GuardResult<()> {
        let canonical = path.canonicalize().map_err(|e| {
            GuardError::new(
                ErrorCode::ModuleRead,
                format!("A2_MODULE_READ {}: {e}", path.display()),
            )
        })?;
        if !seen.insert(canonical.clone()) {
            return Err(GuardError::new(
                ErrorCode::ModuleCycle,
                format!("A2_MODULE_CYCLE: {}", path.display()),
            ));
        }
        let source = std::fs::read_to_string(path).map_err(|e| {
            GuardError::new(
                ErrorCode::ModuleRead,
                format!("A2_MODULE_READ {}: {e}", path.display()),
            )
        })?;
        let syntax = syn::parse_file(&source).map_err(|e| {
            GuardError::new(
                ErrorCode::SourceParse,
                format!("A2_SOURCE_PARSE {}: {e}", path.display()),
            )
        })?;
        let parent = path
            .parent()
            .ok_or_else(|| GuardError::new(ErrorCode::ModulePath, "A2_MODULE_PATH: no parent"))?;
        let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
            GuardError::new(ErrorCode::ModulePath, "A2_MODULE_PATH: non-UTF8 source")
        })?;
        let directory = match stem {
            "lib" | "main" | "mod" => parent.to_owned(),
            _ => parent.join(stem),
        };
        items(&syntax.items, &directory, parent, seen)?;
        seen.remove(&canonical);
        Ok(())
    }
    let source = std::fs::read_to_string(root)
        .map_err(|e| GuardError::new(ErrorCode::LibRead, format!("A2_LIB_READ: {e}")))?;
    check_surface_declaration(&source, state)?;
    file(root, &mut BTreeSet::new())
}

fn workspace_inputs() -> GuardResult<(Vec<Manifest>, toml::map::Map<String, Value>, PathBuf)> {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .map_err(|e| GuardError::new(ErrorCode::Metadata, format!("A2_METADATA: {e}")))?;
    if !output.status.success() {
        return Err(GuardError::new(
            ErrorCode::Metadata,
            format!("A2_METADATA: {}", String::from_utf8_lossy(&output.stderr)),
        ));
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| {
        GuardError::new(ErrorCode::MetadataParse, format!("A2_METADATA_PARSE: {e}"))
    })?;
    let members = metadata["workspace_members"].as_array().ok_or_else(|| {
        GuardError::new(
            ErrorCode::Metadata,
            "A2_METADATA: missing workspace_members",
        )
    })?;
    let packages = metadata["packages"]
        .as_array()
        .ok_or_else(|| GuardError::new(ErrorCode::Metadata, "A2_METADATA: missing packages"))?;
    let root = metadata["workspace_root"]
        .as_str()
        .ok_or_else(|| GuardError::new(ErrorCode::Metadata, "A2_METADATA: missing root"))?;
    let source = std::fs::read_to_string(Path::new(root).join("Cargo.toml"))
        .map_err(|e| GuardError::new(ErrorCode::RootManifest, format!("A2_ROOT_MANIFEST: {e}")))?;
    let root: Value = toml::from_str(&source)
        .map_err(|e| GuardError::new(ErrorCode::RootManifest, format!("A2_ROOT_MANIFEST: {e}")))?;
    let inherited = root
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(Value::as_table)
        .ok_or_else(|| {
            GuardError::new(
                ErrorCode::WorkspaceDependenciesMissing,
                "A2_WORKSPACE_DEPENDENCIES_MISSING",
            )
        })?
        .clone();
    let mut manifests = Vec::new();
    let mut found = BTreeSet::new();
    let mut value_source = None;
    for package in packages {
        if !members.contains(&package["id"]) {
            continue;
        }
        let name = package["name"].as_str().ok_or_else(|| {
            GuardError::new(ErrorCode::Metadata, "A2_METADATA: missing package name")
        })?;
        let path = PathBuf::from(package["manifest_path"].as_str().ok_or_else(|| {
            GuardError::new(ErrorCode::Metadata, "A2_METADATA: missing manifest path")
        })?);
        let source = std::fs::read_to_string(&path).map_err(|e| {
            GuardError::new(
                ErrorCode::ManifestRead,
                format!("A2_MANIFEST_READ {}: {e}", path.display()),
            )
        })?;
        manifests.push(Manifest::parse(name, path, &source)?);
        if !found.insert(package["id"].to_string()) {
            return Err(GuardError::new(
                ErrorCode::Metadata,
                "A2_METADATA: duplicate package",
            ));
        }
        if name == "neovm-value" {
            let targets = package["targets"]
                .as_array()
                .ok_or_else(|| GuardError::new(ErrorCode::Metadata, "A2_METADATA: no targets"))?;
            let libs: Vec<_> = targets
                .iter()
                .filter(|t| {
                    t["kind"]
                        .as_array()
                        .is_some_and(|k| k.iter().any(|v| v == "lib"))
                })
                .collect();
            let [lib] = libs.as_slice() else {
                return Err(GuardError::new(
                    ErrorCode::ValueLibMissingOrDuplicate,
                    "A2_VALUE_LIB_MISSING_OR_DUPLICATE",
                ));
            };
            let path = lib["src_path"]
                .as_str()
                .ok_or_else(|| GuardError::new(ErrorCode::Metadata, "A2_METADATA: no lib root"))?;
            value_source = Some(PathBuf::from(path));
        }
    }
    if found.len() != members.len() {
        return Err(GuardError::new(
            ErrorCode::Metadata,
            "A2_METADATA: workspace member not scanned",
        ));
    }
    Ok((
        manifests,
        inherited,
        value_source
            .ok_or_else(|| GuardError::new(ErrorCode::ValueLibMissing, "A2_VALUE_LIB_MISSING"))?,
    ))
}

#[test]
fn architecture_test_support_is_enabled_only_from_above_the_provider() {
    let state =
        SurfaceState::parse(include_str!("test_support_surface_state.tsv")).expect("A2 state data");
    let (manifests, inherited, source) = workspace_inputs().expect("complete workspace metadata");
    check_manifests(&manifests, &inherited, state).expect("A2 feature manifest fence");
    check_module_tree(&source, state).expect("A2 module existence fence");
}

fn fixture(core_edges: &str, extra: &str) -> Vec<Manifest> {
    [
        ("neovm-value", "[features]\ntest-support=[]\n".to_owned()),
        ("neovm-core", core_edges.to_owned()),
        ("consumer", extra.to_owned()),
    ]
    .into_iter()
    .map(|(name, tail)| {
        Manifest::parse(
            name,
            PathBuf::from(format!("{name}/Cargo.toml")),
            &format!("[package]\nname='{name}'\n{tail}"),
        )
        .expect("fixture TOML")
    })
    .collect()
}

/// The normal edge that puts a crate above neovm-value. Every real provider
/// crate has one; a dev-only consumer does not.
const NORMAL: &str = "[dependencies]\nlower={package='neovm-value'}\n";
const DEV: &str = "[dev-dependencies]\nlower={package='neovm-value',features=['test-support']}\n";
const GATED: &str = "#[cfg(feature=\"test-support\")] pub mod test_support {}";

#[test]
fn architecture_a2_normal_dependency_mutation_red_then_restored_green() {
    let inherited = toml::map::Map::new();
    let good = fixture(&format!("{NORMAL}{DEV}"), "");
    assert_eq!(
        check_manifests(&good, &inherited, SurfaceState::Present),
        Ok(())
    );
    let bad = fixture(
        "[dependencies]\nlower={package='neovm-value',features=['test-support']}\n",
        "",
    );
    let error = check_manifests(&bad, &inherited, SurfaceState::Present)
        .expect_err("normal enablement must be refused");
    assert!(error.is(ErrorCode::NonDevEnabler), "{error}");
    eprintln!("MUTATION RED: {error}");
    assert_eq!(
        check_manifests(&good, &inherited, SurfaceState::Present),
        Ok(())
    );
    eprintln!("RESTORED GREEN: A2 normal dependency");
}

#[test]
fn architecture_a2_module_gate_mutation_red_then_restored_green() {
    assert_eq!(
        check_surface_declaration(GATED, SurfaceState::Present),
        Ok(())
    );
    let error = check_surface_declaration("pub mod test_support {}", SurfaceState::Present)
        .expect_err("removing the feature gate must be refused");
    assert!(error.is(ErrorCode::TestSupportUngated), "{error}");
    eprintln!("MUTATION RED: {error}");
    assert_eq!(
        check_surface_declaration(GATED, SurfaceState::Present),
        Ok(())
    );
    eprintln!("RESTORED GREEN: A2 module gate");
}

#[test]
fn architecture_a2_rejects_renamed_target_and_foreign_enablers() {
    let inherited = toml::map::Map::new();
    let target = fixture(
        &format!(
            "{NORMAL}{DEV}[target.'cfg(unix)'.dependencies]\nv={{package='neovm-value',features=['test-support']}}\n"
        ),
        "",
    );
    assert!(
        check_manifests(&target, &inherited, SurfaceState::Present)
            .expect_err("target normal dependency")
            .is(ErrorCode::NonDevEnabler)
    );
    let foreign = fixture(
        &format!("{NORMAL}{DEV}"),
        "[dev-dependencies]\nv={package='neovm-value',features=['test-support']}\n",
    );
    assert!(
        check_manifests(&foreign, &inherited, SurfaceState::Present)
            .expect_err("foreign dev dependency")
            .is(ErrorCode::ForeignEnabler)
    );
}

#[test]
fn architecture_a2_rejects_forwarding_inheritance_and_state_regression() {
    let inherited: Value =
        toml::from_str("lower={package='neovm-value',features=['test-support']}").expect("fixture");
    let inherited = inherited.as_table().expect("table");
    let bad = fixture("[dependencies]\nlower.workspace=true\n", "");
    assert!(
        check_manifests(&bad, inherited, SurfaceState::Present)
            .expect_err("workspace inheritance")
            .is(ErrorCode::NonDevEnabler)
    );
    let forward = fixture(
        &format!("{NORMAL}{DEV}[features]\nmy-tests=['lower?/test-support']\n"),
        "",
    );
    assert!(
        check_manifests(&forward, &toml::map::Map::new(), SurfaceState::Present)
            .expect_err("feature forward")
            .is(ErrorCode::FeatureForwardEscape)
    );
    let good = fixture(&format!("{NORMAL}{DEV}"), "");
    assert!(
        check_manifests(&good, &toml::map::Map::new(), SurfaceState::Absent)
            .expect_err("surface phase cannot regress")
            .is(ErrorCode::SurfaceState)
    );
    assert!(
        check_surface_declaration("", SurfaceState::Present)
            .expect_err("surface cannot vanish")
            .is(ErrorCode::SurfaceMissingOrDuplicate)
    );
}

#[test]
fn architecture_a2_fails_closed_on_unknown_or_missing_inputs() {
    assert!(SurfaceState::parse("").is_err());
    assert!(SurfaceState::parse("crate\tstate\nneovm-value\tmaybe\n").is_err());
    assert!(Manifest::parse("broken", PathBuf::from("broken/Cargo.toml"), "[").is_err());
    assert!(check_manifests(&[], &toml::map::Map::new(), SurfaceState::Absent).is_err());
    assert!(check_surface_declaration("not rust {{{", SurfaceState::Absent).is_err());
    assert!(
        check_surface_declaration(
            "#[cfg(unknown(feature=\"test-support\"))] pub mod test_support {}",
            SurfaceState::Present
        )
        .expect_err("unknown cfg syntax")
        .is(ErrorCode::UnknownCfg)
    );
    assert!(
        check_surface_declaration(
            "#[cfg_attr(feature=\"test-support\", cfg(any()))] pub mod test_support {}",
            SurfaceState::Present
        )
        .is_err()
    );
    assert!(
        check_surface_declaration(&format!("{GATED}\n{GATED}"), SurfaceState::Present).is_err()
    );
}

#[test]
fn architecture_a2_unknown_module_and_missing_source_are_failures() {
    let tree = tempfile::tempdir().expect("fixture directory");
    let root = tree.path().join("lib.rs");
    std::fs::write(&root, "pub mod unknown;").expect("fixture root");
    assert!(
        check_module_tree(&root, SurfaceState::Absent)
            .expect_err("unknown module must fail")
            .is(ErrorCode::ModuleMissingOrAmbiguous)
    );
    std::fs::write(tree.path().join("unknown.rs"), "not rust {{{").expect("fixture source");
    assert!(
        check_module_tree(&root, SurfaceState::Absent)
            .expect_err("unparseable module must fail")
            .is(ErrorCode::SourceParse)
    );
    std::fs::remove_file(&root).expect("remove fixture root");
    assert!(
        check_module_tree(&root, SurfaceState::Absent)
            .expect_err("missing lib must fail")
            .is(ErrorCode::LibRead)
    );
}

#[test]
fn architecture_a2_visibility_cfg_attr_is_scanned_but_conditional_path_is_refused() {
    let tree = tempfile::tempdir().expect("fixture directory");
    let root = tree.path().join("lib.rs");
    std::fs::write(&root, "mod holder;\n").expect("fixture root");
    // A nested test_support module may be conditionally WIDENED: the walk
    // still scans its content, and cfg_attr cannot change its source file.
    std::fs::write(
        tree.path().join("holder.rs"),
        "#[cfg(any(test, feature = \"test-support\"))]\n#[cfg_attr(feature = \"test-support\", visibility::make(pub))]\nmod test_support { pub fn fixture() {} }\n",
    )
    .expect("visibility fixture");
    check_module_tree(&root, SurfaceState::Absent)
        .expect("conditional-visibility module must be scanned");
    // A conditional PATH leaves another configuration unscanned: fail closed.
    std::fs::write(
        tree.path().join("holder.rs"),
        "#[cfg_attr(unix, path = \"other.rs\")]\nmod swapped;\n",
    )
    .expect("path fixture");
    std::fs::write(tree.path().join("other.rs"), "pub fn x() {}\n").expect("path target");
    assert!(
        check_module_tree(&root, SurfaceState::Absent)
            .expect_err("conditional module path must fail closed")
            .is(ErrorCode::ModulePath)
    );
}

#[test]
fn architecture_a2_preserves_workspace_and_default_feature_refusals() {
    let inherited: Value =
        toml::from_str("lower={package='neovm-value',features=['test-support']}").expect("fixture");
    let inherited = inherited.as_table().expect("table");
    let normal = fixture(&format!("{NORMAL}{DEV}"), "");
    assert!(
        check_manifests(&normal, inherited, SurfaceState::Present)
            .expect_err("even unused workspace enabler must be refused")
            .is(ErrorCode::NonDevEnabler)
    );
    let mut default = fixture(DEV, "");
    default[0]
        .data
        .get_mut("features")
        .and_then(toml::Value::as_table_mut)
        .expect("fixture feature table")
        .insert(
            "default".into(),
            toml::Value::Array(vec![toml::Value::String("test-support".into())]),
        );
    assert!(
        check_manifests(&default, &toml::map::Map::new(), SurfaceState::Present)
            .expect_err("value default cannot enable its test surface")
            .is(ErrorCode::FeatureForwardEscape)
    );
}

#[test]
fn architecture_a2_target_dev_edges_are_never_sanctioned() {
    let target_only = fixture(
        &format!(
            "{NORMAL}[target.'cfg(unix)'.dev-dependencies]\nv={{package='neovm-value',features=['test-support']}}\n"
        ),
        "",
    );
    assert!(
        check_manifests(&target_only, &toml::map::Map::new(), SurfaceState::Present)
            .expect_err("a target dev edge is not the signed package dev edge")
            .is(ErrorCode::NonDevEnabler)
    );
}

#[test]
fn architecture_a2_accepts_the_fallback_heap_implication_only() {
    let inherited = toml::map::Map::new();
    let mut allowed = fixture(&format!("{NORMAL}{DEV}"), "");
    allowed[0]
        .data
        .get_mut("features")
        .and_then(Value::as_table_mut)
        .expect("fixture feature table")
        .insert(
            "test-thread-fallback-heap".into(),
            Value::Array(vec![Value::String("test-support".into())]),
        );
    assert_eq!(
        check_manifests(&allowed, &inherited, SurfaceState::Present),
        Ok(())
    );
    let mut product = fixture(&format!("{NORMAL}{DEV}"), "");
    product[0]
        .data
        .get_mut("features")
        .and_then(Value::as_table_mut)
        .expect("fixture feature table")
        .insert(
            "jit".into(),
            Value::Array(vec![Value::String("test-thread-fallback-heap".into())]),
        );
    product[0]
        .data
        .get_mut("features")
        .and_then(Value::as_table_mut)
        .expect("fixture feature table")
        .insert(
            "test-thread-fallback-heap".into(),
            Value::Array(vec![Value::String("test-support".into())]),
        );
    assert!(
        check_manifests(&product, &inherited, SurfaceState::Present)
            .expect_err("a product feature must not reach the surface through the fallback")
            .is(ErrorCode::FeatureForwardEscape)
    );
}

/// The fallback implication opens a route to `test-support`. A normal
/// dependency and a workspace dependency that request
/// `test-thread-fallback-heap` are still non-dev enablers.
#[test]
fn architecture_a2_refuses_normal_and_workspace_fallback_heap_enablers() {
    let imply = |mut manifests: Vec<Manifest>| {
        manifests[0]
            .data
            .get_mut("features")
            .and_then(Value::as_table_mut)
            .expect("fixture feature table")
            .insert(
                "test-thread-fallback-heap".into(),
                Value::Array(vec![Value::String("test-support".into())]),
            );
        manifests
    };
    // neovm-value = { features = ["test-thread-fallback-heap"] }
    let normal = imply(fixture(
        &format!("{NORMAL}{DEV}"),
        "[dependencies]\nneovm-value={features=['test-thread-fallback-heap']}\n",
    ));
    let error = check_manifests(&normal, &toml::map::Map::new(), SurfaceState::Present)
        .expect_err("a normal dependency must not enable the fallback heap");
    assert!(error.is(ErrorCode::NonDevEnabler), "{error}");
    assert!(
        error.detail.contains("Normal") && error.detail.contains("alias=neovm-value"),
        "{error}"
    );

    // The same edge in [workspace.dependencies].
    let inherited: Value =
        toml::from_str("neovm-value={features=['test-thread-fallback-heap']}").expect("fixture");
    let inherited = inherited.as_table().expect("table");
    let workspace = imply(fixture(&format!("{NORMAL}{DEV}"), ""));
    let error = check_manifests(&workspace, inherited, SurfaceState::Present)
        .expect_err("workspace.dependencies must not enable the fallback heap");
    assert!(error.is(ErrorCode::NonDevEnabler), "{error}");
    assert!(
        error.detail.contains("workspace.dependencies")
            && error.detail.contains("alias=neovm-value"),
        "{error}"
    );
}

#[test]
fn architecture_a2_accepts_dev_edges_and_forwards_from_above() {
    let inherited = toml::map::Map::new();
    let core = fixture(&format!("{NORMAL}{DEV}"), "");
    assert_eq!(
        check_manifests(&core, &inherited, SurfaceState::Present),
        Ok(())
    );
    // A non-core crate that sits above the provider may dev-enable it.
    let enabled = fixture(
        NORMAL,
        "[dependencies]\nlower={package='neovm-value'}\n\
         [dev-dependencies]\nlower={package='neovm-value',features=['test-support']}\n",
    );
    assert_eq!(
        check_manifests(&enabled, &inherited, SurfaceState::Present),
        Ok(())
    );
    // ... and may forward the provider's surface from its own test-support.
    let forwarded = fixture(
        NORMAL,
        "[dependencies]\nlower={package='neovm-value'}\n\
         [features]\ntest-support=['lower/test-support']\n",
    );
    assert_eq!(
        check_manifests(&forwarded, &inherited, SurfaceState::Present),
        Ok(())
    );
}

#[test]
fn architecture_a2_rules_apply_to_every_provider() {
    let provider = |consumer: &str| -> Vec<Manifest> {
        [
            ("neovm-value", "[features]\ntest-support=[]\n"),
            (
                "neovm-text",
                "[features]\ntest-support=['neovm-value/test-support']\n\
                 [dependencies]\nvalue={package='neovm-value'}\n",
            ),
            (
                "neovm-core",
                "[dependencies]\nvalue={package='neovm-value'}\ntext={package='neovm-text'}\n",
            ),
            ("consumer", consumer),
        ]
        .into_iter()
        .map(|(name, tail)| {
            Manifest::parse(
                name,
                PathBuf::from(format!("{name}/Cargo.toml")),
                &format!("[package]\nname='{name}'\n{tail}"),
            )
            .expect("fixture TOML")
        })
        .collect()
    };
    let inherited = toml::map::Map::new();
    // A normal/build dependency may not enable a higher provider's surface,
    // even from a crate that is above it.
    for enabling in [
        "[dependencies]\ntext={package='neovm-text',features=['test-support']}\n",
        "[build-dependencies]\ntext={package='neovm-text',features=['test-support']}\n",
    ] {
        let error = check_manifests(&provider(enabling), &inherited, SurfaceState::Present)
            .expect_err("only a package dev edge may enable a provider surface");
        assert!(error.is(ErrorCode::NonDevEnabler), "{error}");
    }
    // The same edge is allowed as a dev dependency from above...
    let dev = provider(
        "[dependencies]\ntext={package='neovm-text'}\n\
         [dev-dependencies]\ntext={package='neovm-text',features=['test-support']}\n",
    );
    assert_eq!(
        check_manifests(&dev, &inherited, SurfaceState::Present),
        Ok(())
    );
    // ... but a dev-only consumer is still below the provider it enables.
    let foreign =
        provider("[dev-dependencies]\ntext={package='neovm-text',features=['test-support']}\n");
    let error = check_manifests(&foreign, &inherited, SurfaceState::Present)
        .expect_err("a dev-only consumer is not above the provider");
    assert!(error.is(ErrorCode::ForeignEnabler), "{error}");
}
