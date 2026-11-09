//! Generator for the H3/H3b upper-layer registration snapshots.
//!
//! The unit runtime's checked-in `upper_subrs.generated.rs` and
//! `upper_bootstrap_vars.generated.rs` are Rust source with `fn` items and
//! cfg attributes; the P2 names+order tests need the same data as pure
//! values. This module derives two checked-in TSVs from those two sources:
//!
//! * `crates/neovm-eval/src/upper_subrs.snapshot.tsv`
//!   `name  min  max  layer  cfg` — one row per upper subr in production
//!   registration order, `max` `-` for unbounded, `layer` from the panicking
//!   fn's marker, `cfg` from the element's cfg attribute.
//! * `crates/neovm-eval/src/upper_bootstrap_vars.snapshot.tsv`
//!   `name  layer  cfg  default` — one row per canonical bootstrap variable in
//!   binder order; the layer slices are the four canonical registrations
//!   (fileio 24, process 9, dbusbind 9, xfaces 9). `default` is the value T4
//!   binds: `marker` for Lisp_Object variables, `int:<n>` for a forwarded
//!   DEFVAR_INT slot. The rows are parsed from the four production binders'
//!   own initializers, not from the generated data, so a typed slot's default
//!   cannot drift from the binder that owns it.
//! * `crates/neovm-eval/src/upper_bootstrap_vars.generated.rs` is rendered
//!   from the same binder parse (name, cfg and the typed default).
//!
//! Regenerate with `cargo xtask upper-snapshots` (writes) or
//! `cargo xtask upper-snapshots --check` (fails on drift). Eval's cfg(test)
//! snapshot-consistency test includes this module and asserts the TSVs, the
//! generated source and the compiled registration table agree row for row.

// This file is compiled both by xtask and by eval's cfg(test) drift test;
// only parsing and rendering are used. The host-cfg vocabulary lives in
// neovm-eval's host_cfg module so every consumer shares one implementation;
// its activity predicate is eval-only (host_cfg_active.rs) and is never
// compiled by xtask, which has no build-script probe.
#![allow(dead_code)]

use std::path::Path;

// The cfg-label vocabulary is shared with eval's unit runtime and the plan
// generator; include the one host_cfg source so xtask cannot drift. Only the
// vocabulary is included: the activity predicate stays in eval.
#[path = "../src/host_cfg.rs"]
mod host_cfg;

pub const SUBRS_SOURCE: &str = "crates/neovm-eval/src/upper_subrs.generated.rs";
pub const BOOTSTRAP_SOURCE: &str = "crates/neovm-eval/src/upper_bootstrap_vars.generated.rs";
pub const SUBRS_SNAPSHOT: &str = "crates/neovm-eval/src/upper_subrs.snapshot.tsv";
pub const BOOTSTRAP_SNAPSHOT: &str = "crates/neovm-eval/src/upper_bootstrap_vars.snapshot.tsv";

/// One upper-layer subr row, in production registration order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpperSubrRow {
    pub name: String,
    pub min: u16,
    pub max: Option<u16>,
    pub layer: String,
    pub cfg: String,
}

/// The value T4 binds for one canonical bootstrap variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootstrapDefault {
    /// Lisp_Object variable: the unit runtime binds its ABSENT-LAYER marker.
    Marker,
    /// Forwarded DEFVAR_INT slot: the production binder's own initial value.
    Integer(i64),
}

impl BootstrapDefault {
    /// The snapshot-column spelling of this default.
    pub fn label(&self) -> String {
        match self {
            Self::Marker => "marker".to_owned(),
            Self::Integer(value) => format!("int:{value}"),
        }
    }
}

/// One canonical bootstrap-variable row, in binder order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapVarRow {
    pub name: String,
    pub layer: String,
    pub cfg: String,
    pub default: BootstrapDefault,
}

/// The four production binders, in canonical registration order, with the
/// pinned number of variables each one binds.
pub const BOOTSTRAP_BINDERS: [(&str, &str, usize); 4] = [
    (
        "fileio",
        "crates/neovm-system/src/emacs_core/system/fileio/mod.rs",
        24,
    ),
    (
        "process",
        "crates/neovm-system/src/emacs_core/system/process/bootstrap_vars.rs",
        9,
    ),
    (
        "dbusbind",
        "crates/neovm-system/src/emacs_core/system/dbusbind/subrs.rs",
        9,
    ),
    (
        "xfaces",
        "crates/neovm-display/src/emacs_core/display/xfaces/mod.rs",
        9,
    ),
];

fn parse_cfg(attr: &str) -> Result<String, String> {
    let compact: String = attr.split_whitespace().collect();
    // The x11-only surface form must be recognized before the generic
    // windows check below, which would otherwise match its substring.
    if compact.contains("not(any(windows,target_os=\"macos\"))") {
        return Ok("x11-only".to_owned());
    }
    // The negated surface form must be recognized before the bare macOS test
    // below, which would otherwise match its substring.
    if compact.contains("not(target_os=\"macos\")") {
        return Ok("not-macos".to_owned());
    }
    if compact.contains("target_os=\"macos\"") {
        return Ok("macos".to_owned());
    }
    if compact.contains("windows") {
        return Ok("windows".to_owned());
    }
    if compact.contains("neomacs_have_dbus") {
        return Ok("dbus".to_owned());
    }
    if compact.contains("feature=\"sqlite\"") {
        return Ok("sqlite".to_owned());
    }
    Err(format!("unknown snapshot cfg attribute: {attr}"))
}

/// Derive the upper-subr rows from the generated registration source.
pub fn parse_upper_subrs(source: &str) -> Result<Vec<UpperSubrRow>, String> {
    // First pass: each panicking fn's marker names its owning layer.
    let mut layers: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut current: Option<String> = None;
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("fn upper_subr_") {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                return Err(format!("malformed upper subr fn: {line}"));
            }
            current = Some(format!("upper_subr_{digits}"));
            continue;
        }
        let Some(fname) = current.clone() else {
            continue;
        };
        const MARKER: &str = "s::absent_layer_panic(\"ABSENT-LAYER ";
        if let Some(start) = line.find(MARKER) {
            let rest = &line[start + MARKER.len()..];
            let layer = rest
                .split_once("::")
                .map(|(layer, _)| layer)
                .ok_or_else(|| format!("{fname}: marker has no layer separator"))?;
            if layers.insert(fname.clone(), layer.to_owned()).is_some() {
                return Err(format!("{fname}: duplicate marker"));
            }
            current = None;
        } else if trimmed == "}" {
            return Err(format!("{fname}: no absent-layer marker"));
        }
    }

    // Second pass: the declaration array, in order, with element cfg attrs.
    let mut rows = Vec::new();
    let mut in_array = false;
    let mut pending_cfg: Option<String> = None;
    for line in source.lines() {
        if !in_array {
            if line.trim_start().starts_with("static UPPER_SUBRS") {
                in_array = true;
            }
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("#[cfg(") {
            pending_cfg = Some(parse_cfg(trimmed)?);
            continue;
        }
        if trimmed == "];" {
            continue;
        }
        let body = trimmed
            .strip_prefix('(')
            .and_then(|body| body.strip_suffix("),"))
            .ok_or_else(|| format!("unexpected upper subr array line: {line}"))?;
        let parts: Vec<&str> = body.split(',').map(str::trim).collect();
        if parts.len() != 4 {
            return Err(format!("malformed upper subr tuple: {line}"));
        }
        let name = parts[0]
            .strip_prefix('"')
            .and_then(|name| name.strip_suffix('"'))
            .ok_or_else(|| format!("malformed upper subr name: {line}"))?;
        let min: u16 = parts[1]
            .parse()
            .map_err(|_| format!("malformed upper subr min: {line}"))?;
        let max = if parts[2] == "None" {
            None
        } else {
            let digits = parts[2]
                .strip_prefix("Some(")
                .and_then(|max| max.strip_suffix(')'))
                .ok_or_else(|| format!("malformed upper subr max: {line}"))?;
            Some(
                digits
                    .parse()
                    .map_err(|_| format!("malformed upper subr max: {line}"))?,
            )
        };
        let layer = layers
            .get(parts[3])
            .ok_or_else(|| format!("{}: no panicking fn in the source", parts[3]))?
            .clone();
        rows.push(UpperSubrRow {
            name: name.to_owned(),
            min,
            max,
            layer,
            cfg: pending_cfg.take().unwrap_or_else(|| "always".to_owned()),
        });
    }
    if rows.is_empty() {
        return Err("no upper subr rows parsed".to_owned());
    }
    if rows.len() != layers.len() {
        return Err(format!(
            "{} array rows but {} panicking fns",
            rows.len(),
            layers.len()
        ));
    }
    Ok(rows)
}

/// The body of the binder function starting at `signature`, braces included.
fn function_body<'a>(source: &'a str, signature: &str) -> Result<&'a str, String> {
    let start = source
        .find(signature)
        .ok_or_else(|| format!("missing `{signature}`"))?;
    let open = source[start..]
        .find('{')
        .map(|offset| start + offset)
        .ok_or_else(|| format!("`{signature}` has no body"))?;
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut index = open;
    let mut line_comment = false;
    let mut block_comment = false;
    let mut string = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }
        if block_comment {
            if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if string {
            if byte == b'\\' {
                index += 2;
                continue;
            }
            if byte == b'"' {
                string = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                line_comment = true;
                index += 2;
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                block_comment = true;
                index += 2;
            }
            b'"' => {
                string = true;
                index += 1;
            }
            b'{' => {
                depth += 1;
                index += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(&source[open..=index]);
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    Err(format!("unbalanced body for `{signature}`"))
}

/// A quoted string literal at the start of `text` (after whitespace), plus
/// the text that follows its closing quote.
fn string_literal(text: &str) -> Result<(String, &str), String> {
    let text = text.trim_start();
    let rest = text.strip_prefix('"').ok_or_else(|| {
        format!(
            "expected a string literal near `{}`",
            &text[..text.len().min(60)]
        )
    })?;
    let end = rest
        .find('"')
        .ok_or_else(|| "unterminated string literal".to_owned())?;
    Ok((rest[..end].to_owned(), &rest[end + 1..]))
}

/// The integer initializer of a `define_int_variable` call, starting just
/// after its name literal.
fn integer_initializer(text: &str) -> Result<i64, String> {
    let rest = text
        .trim_start()
        .strip_prefix(',')
        .ok_or_else(|| "expected `,` before the integer initializer".to_owned())?
        .trim_start();
    let token: String = rest
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '_')
        .collect();
    let digits = token.replace('_', "");
    digits
        .parse()
        .map_err(|_| format!("malformed integer initializer `{token}`"))
}

/// Parse one binder body's variable registrations in source order.
///
/// Recognized calls: `seed_value`, `define_special_variable`,
/// `define_c_hook_variable` (Lisp_Object -> marker), and
/// `define_int_variable` (forwarded typed slot -> its literal default). A
/// `define_bool_variable` is rejected: no current binder binds one, and its
/// byte-boolean class would have to be modeled before a value could be
/// bound correctly.
fn parse_binder_specs(body: &str, layer: &str) -> Result<Vec<BootstrapVarRow>, String> {
    const APIS: [&str; 5] = [
        ".seed_value(",
        ".define_special_variable(",
        ".define_int_variable(",
        ".define_bool_variable(",
        ".define_c_hook_variable(",
    ];
    let mut found: Vec<(usize, BootstrapVarRow)> = Vec::new();
    for api in APIS {
        let mut from = 0;
        while let Some(offset) = body[from..].find(api) {
            let at = from + offset;
            let (name, after_name) = string_literal(&body[at + api.len()..])?;
            let default = if api == ".define_int_variable(" {
                BootstrapDefault::Integer(integer_initializer(after_name)?)
            } else if api == ".define_bool_variable(" {
                return Err(format!(
                    "{layer}: typed boolean variable {name:?} has no modeled byte-boolean class"
                ));
            } else {
                BootstrapDefault::Marker
            };
            found.push((
                at,
                BootstrapVarRow {
                    name,
                    layer: layer.to_owned(),
                    cfg: if layer == "dbusbind" {
                        "dbus".to_owned()
                    } else {
                        "always".to_owned()
                    },
                    default,
                },
            ));
            from = at + api.len();
        }
    }
    found.sort_by_key(|(at, _)| *at);
    Ok(found.into_iter().map(|(_, row)| row).collect())
}

/// Derive every canonical bootstrap variable from the four production
/// binders' own initializers, in registration order.
pub fn parse_bootstrap_binders(repo_root: &Path) -> Result<Vec<BootstrapVarRow>, String> {
    let mut rows = Vec::new();
    for (layer, file, expected) in BOOTSTRAP_BINDERS {
        let source = std::fs::read_to_string(repo_root.join(file))
            .map_err(|error| format!("read {file}: {error}"))?;
        let body = function_body(&source, "fn register_bootstrap_vars(")?;
        let specs = parse_binder_specs(body, layer)?;
        if specs.len() != expected {
            return Err(format!(
                "{layer}: expected {expected} bootstrap variables, parsed {}",
                specs.len()
            ));
        }
        rows.extend(specs);
    }
    let mut seen = std::collections::BTreeSet::new();
    for row in &rows {
        if !seen.insert(row.name.clone()) {
            return Err(format!("duplicate bootstrap variable {:?}", row.name));
        }
    }
    Ok(rows)
}

/// Render the TSV for the given rows (a trailing newline always ends it).
pub fn render_upper_subrs(rows: &[UpperSubrRow]) -> String {
    let mut out = String::from("name\tmin\tmax\tlayer\tcfg\n");
    for row in rows {
        let max = row
            .max
            .map_or_else(|| "-".to_owned(), |max| max.to_string());
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            row.name, row.min, max, row.layer, row.cfg
        ));
    }
    out
}

/// Render the TSV for the given bootstrap rows.
pub fn render_bootstrap_vars(rows: &[BootstrapVarRow]) -> String {
    let mut out = String::from("name\tlayer\tcfg\tdefault\n");
    for row in rows {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            row.name,
            row.layer,
            row.cfg,
            row.default.label()
        ));
    }
    out
}

/// Render the generated Rust source from the same binder-derived rows.
pub fn render_bootstrap_source(rows: &[BootstrapVarRow]) -> Result<String, String> {
    let mut out = String::from(
        "// Generated by the eval unit-runtime bootstrap-variable snapshot generator.\n\
         // Names, order and typed defaults come from the four canonical production\n\
         // binders (fileio, process, dbusbind, xfaces), read from their own\n\
         // initializers. T4 binds exactly these values; only Lisp_Object variables\n\
         // keep the ABSENT-LAYER marker. Do not edit by hand.\n\
         \n\
         // (included inside the cfg(test) unit_runtime module; `s` is the slot module)\n\
         static UPPER_BOOTSTRAP_VARS: &[(&str, s::BootstrapDefault)] = &[\n",
    );
    for row in rows {
        if row.cfg != "always" {
            let attr = match row.cfg.as_str() {
                "dbus" => "neomacs_have_dbus",
                "sqlite" => "feature = \"sqlite\"",
                other => return Err(format!("unrenderable bootstrap cfg {other:?}")),
            };
            out.push_str(&format!("    #[cfg({attr})]\n"));
        }
        let default = match row.default {
            BootstrapDefault::Marker => "s::BootstrapDefault::Marker".to_owned(),
            BootstrapDefault::Integer(value) => {
                format!("s::BootstrapDefault::Integer({value})")
            }
        };
        out.push_str(&format!("    (\"{}\", {}),\n", row.name, default));
    }
    out.push_str("];\n");
    Ok(out)
}

/// Parse the upper sources and the four production binders, then write (or,
/// with `check`, compare) the checked-in snapshots and the generated
/// bootstrap source.
pub fn generate(repo_root: &Path, check: bool) -> Result<(), String> {
    let read = |relative: &str| {
        std::fs::read_to_string(repo_root.join(relative))
            .map_err(|error| format!("read {relative}: {error}"))
    };
    if !host_cfg::HostCfgClass::vocabulary_round_trips() {
        return Err(
            "host cfg vocabulary does not round-trip; fix HostCfgClass::label/from_label \
             in crates/neovm-eval/src/host_cfg.rs"
                .to_owned(),
        );
    }
    let subrs = parse_upper_subrs(&read(SUBRS_SOURCE)?)?;
    let bootstrap = parse_bootstrap_binders(repo_root)?;
    for row in &subrs {
        host_cfg::HostCfgClass::from_label(&row.cfg).ok_or_else(|| {
            format!(
                "upper subr row {:?}: unknown cfg class {:?}; add it to HostCfgClass",
                row.name, row.cfg
            )
        })?;
    }
    for row in &bootstrap {
        host_cfg::HostCfgClass::from_label(&row.cfg).ok_or_else(|| {
            format!(
                "bootstrap snapshot row {:?}: unknown cfg class {:?}; add it to HostCfgClass",
                row.name, row.cfg
            )
        })?;
    }
    let snapshots = [
        (SUBRS_SNAPSHOT, render_upper_subrs(&subrs)),
        (BOOTSTRAP_SNAPSHOT, render_bootstrap_vars(&bootstrap)),
        (BOOTSTRAP_SOURCE, render_bootstrap_source(&bootstrap)?),
    ];
    for (relative, rendered) in snapshots {
        let path = repo_root.join(relative);
        if check {
            let checked_in = std::fs::read_to_string(&path)
                .map_err(|error| format!("read {relative}: {error}"))?;
            if checked_in != rendered {
                return Err(format!(
                    "{relative} is stale; run `cargo xtask upper-snapshots`"
                ));
            }
        } else {
            std::fs::write(&path, rendered)
                .map_err(|error| format!("write {relative}: {error}"))?;
        }
    }
    Ok(())
}
