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
//!   `name  layer  cfg` — one row per canonical bootstrap variable in binder
//!   order; the layer slices are the four canonical registrations
//!   (fileio 24, process 9, dbusbind 9, xfaces 9).
//!
//! Regenerate with `cargo xtask upper-snapshots` (writes) or
//! `cargo xtask upper-snapshots --check` (fails on drift). Eval's cfg(test)
//! snapshot-consistency test includes this module and asserts the TSVs, the
//! generated source and the compiled registration table agree row for row.

// This file is compiled both by xtask and by eval's cfg(test) drift test;
// only parsing and rendering are used. The host-cfg predicate lives in
// neovm-eval's host_cfg module so every consumer shares one implementation.
#![allow(dead_code)]

use std::path::Path;

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

/// One canonical bootstrap-variable row, in binder order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapVarRow {
    pub name: String,
    pub layer: String,
    pub cfg: String,
}

fn parse_cfg(attr: &str) -> Result<String, String> {
    let compact: String = attr.split_whitespace().collect();
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

/// Derive the canonical bootstrap-variable rows from the generated source.
pub fn parse_bootstrap_vars(source: &str) -> Result<Vec<BootstrapVarRow>, String> {
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut in_array = false;
    let mut pending_cfg: Option<String> = None;
    for line in source.lines() {
        if !in_array {
            if line
                .trim_start()
                .starts_with("static UPPER_BOOTSTRAP_VAR_NAMES")
            {
                in_array = true;
            }
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "];" {
            continue;
        }
        if trimmed.starts_with("#[cfg(") {
            let cfg = parse_cfg(trimmed)?;
            if cfg != "dbus" {
                return Err(format!("unexpected bootstrap cfg {cfg:?}: {line}"));
            }
            pending_cfg = Some(cfg);
            continue;
        }
        let name = trimmed
            .strip_prefix('"')
            .and_then(|name| name.strip_suffix("\","))
            .ok_or_else(|| format!("unexpected bootstrap array line: {line}"))?;
        entries.push((
            name.to_owned(),
            pending_cfg.take().unwrap_or_else(|| "always".to_owned()),
        ));
    }
    // The four canonical registrations the binder calls, in order.
    const LAYERS: [(&str, usize); 4] = [
        ("fileio", 24),
        ("process", 9),
        ("dbusbind", 9),
        ("xfaces", 9),
    ];
    let expected: usize = LAYERS.iter().map(|(_, count)| *count).sum();
    if entries.len() != expected {
        return Err(format!(
            "expected {expected} bootstrap names, parsed {}",
            entries.len()
        ));
    }
    let mut rows = Vec::with_capacity(expected);
    let mut index = 0;
    for (layer, count) in LAYERS {
        for _ in 0..count {
            let (name, cfg) = entries[index].clone();
            let gated = cfg == "dbus";
            if gated != (layer == "dbusbind") {
                return Err(format!("{layer} entry {name} has unexpected cfg {cfg:?}"));
            }
            rows.push(BootstrapVarRow {
                name,
                layer: layer.to_owned(),
                cfg,
            });
            index += 1;
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
    let mut out = String::from("name\tlayer\tcfg\n");
    for row in rows {
        out.push_str(&format!("{}\t{}\t{}\n", row.name, row.layer, row.cfg));
    }
    out
}

/// Read the two generated sources and write (or, with `check`, compare) the
/// two checked-in snapshot files.
pub fn generate(repo_root: &Path, check: bool) -> Result<(), String> {
    let read = |relative: &str| {
        std::fs::read_to_string(repo_root.join(relative))
            .map_err(|error| format!("read {relative}: {error}"))
    };
    let subrs = parse_upper_subrs(&read(SUBRS_SOURCE)?)?;
    let bootstrap = parse_bootstrap_vars(&read(BOOTSTRAP_SOURCE)?)?;
    let snapshots = [
        (SUBRS_SNAPSHOT, render_upper_subrs(&subrs)),
        (BOOTSTRAP_SNAPSHOT, render_bootstrap_vars(&bootstrap)),
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
