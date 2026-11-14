//! Generator for the canonical production subr registration plan.
//!
//! `neovm-core`'s `builtins::register_subrs` is the production compatibility
//! chain: it declares the upper-layer subrs directly in source order and calls
//! every extracted batch registrar in the same sequence. The H3 snapshot
//! (`upper_subrs.snapshot.tsv`) carries that data but not that order: its
//! first 996 rows are the direct declarations and its last 119 rows are the
//! batch names appended in batch-call order, while production interleaves the
//! batches with the direct declarations.
//!
//! This module reconstructs the real order and derives two checked-in
//! artifacts:
//!
//! * `crates/neovm-eval/src/canonical_subr_names.tsv`
//!   `name  min  max  layer  cfg` — every name registered by the production
//!   chain, in registration order, including the eval-owned registrars that
//!   are replayed by the unit runtime.
//! * `crates/neovm-eval/src/canonical_subr_step_plan.tsv`
//!   `step  kind  id  layer  first  count  marker` — the same order cut into
//!   executable steps: `eval_call` steps for the eval-owned registrar pieces,
//!   `display_segment` steps for maximal display-layer runs and
//!   `upper_segment` steps for the other upper-layer runs. `first`/`count`
//!   are 1-based positions into the names table, so every row is covered
//!   exactly once. Display run ids are descriptive and stable — the run's
//!   batch keys or its first subr — never positional, so an unrelated plan
//!   edit cannot renumber the display variants. The `eval.event_properties`
//!   step registers no subr; it carries the `init_event_symbol_properties`
//!   marker that production runs between the upper declarations and the
//!   public evaluator batch.
//!
//! Inputs are all derived from source: the core manifest, the batch
//! `define_subrs!` files, eval's `EVALUATOR_SUBRS` table and the generated
//! upper-layer source (parsed with the same parser as the H3 snapshot
//! generator, which also supplies each upper name's layer). The canonical
//! upper subsequence must match the upper source exactly, name for name.
//!
//! The lcms batch is included with cfg `lcms2`. Its eight names are
//! eval-owned and sit at the manifest's `lcms::register_subrs` position; the
//! workspace's single host-cfg probe emits `neomacs_have_lcms2` in every
//! probing crate (eval included), so production compiles and registers the
//! batch on an lcms2 host.
//!
//! Regenerate with `cargo xtask canonical-subr-plan` (writes) or
//! `cargo xtask canonical-subr-plan --check` (fails on drift).

#![allow(dead_code)]

#[path = "upper_snapshots.rs"]
mod upper_snapshots;

use std::collections::BTreeMap;
use std::path::Path;

pub const NAMES_TSV: &str = "crates/neovm-eval/src/canonical_subr_names.tsv";
pub const PLAN_TSV: &str = "crates/neovm-eval/src/canonical_subr_step_plan.tsv";

const CORE_MANIFEST: &str = "crates/neovm-core/src/emacs_core/builtins/subrs.rs";
const UPPER_SOURCE: &str = "crates/neovm-eval/src/upper_subrs.generated.rs";
const EVAL_SUBRS: &str = "crates/neovm-eval/src/emacs_core/runtime/eval/subrs.rs";
const DISPLAY_SUBRS: &str = "crates/neovm-display/src/emacs_core/display/display_subrs/mod.rs";

/// One extracted batch registrar called from the core manifest.
struct BatchDef {
    /// Path captured before `::register_subrs(ctx);` in the manifest.
    module: &'static str,
    /// Stable short name used in descriptive display-run ids.
    key: &'static str,
    /// Source file holding the batch's `define_subrs!` block.
    file: &'static str,
    /// cfg for entries with no per-item attribute of their own.
    base_cfg: &'static str,
    /// Layer for names absent from the upper snapshot (`file_notify` is
    /// eval-owned, so its names never appear there); `None` means every name
    /// must be in the snapshot.
    unlisted_layer: Option<&'static str>,
    /// Step id attached to every row of this batch; only `file_notify` has
    /// one, because its registrar is replayed by the eval unit runtime.
    event_id: Option<&'static str>,
}

const BATCHES: &[BatchDef] = &[
    BatchDef {
        module: "crate::emacs_core::w32::subrs",
        key: "w32",
        file: "crates/neovm-system/src/emacs_core/system/platform/w32/subrs.rs",
        base_cfg: "windows",
        unlisted_layer: None,
        event_id: None,
    },
    BatchDef {
        module: "lcms",
        key: "lcms",
        file: "crates/neovm-eval/src/emacs_core/lisp/native/builtins/lcms/subrs.rs",
        base_cfg: "lcms2",
        unlisted_layer: Some("eval"),
        event_id: Some("eval.lcms"),
    },
    BatchDef {
        module: "crate::emacs_core::composite",
        key: "composite",
        file: "crates/neovm-core/src/emacs_core/composite/subrs.rs",
        base_cfg: "always",
        unlisted_layer: None,
        event_id: None,
    },
    BatchDef {
        module: "crate::emacs_core::neo::terminal",
        key: "terminal",
        file: "crates/neovm-display/src/emacs_core/display/neo/terminal/subrs.rs",
        base_cfg: "always",
        unlisted_layer: None,
        event_id: None,
    },
    BatchDef {
        module: "crate::emacs_core::shader_surface",
        key: "shader",
        file: "crates/neovm-display/src/emacs_core/display/shader_surface/subrs.rs",
        base_cfg: "always",
        unlisted_layer: None,
        event_id: None,
    },
    BatchDef {
        module: "crate::emacs_core::video",
        key: "video",
        file: "crates/neovm-display/src/emacs_core/display/video/subrs.rs",
        base_cfg: "always",
        unlisted_layer: None,
        event_id: None,
    },
    BatchDef {
        module: "crate::emacs_core::xwidget",
        key: "xwidget",
        file: "crates/neovm-display/src/emacs_core/display/xwidget/subrs.rs",
        base_cfg: "always",
        unlisted_layer: None,
        event_id: None,
    },
    BatchDef {
        module: "crate::emacs_core::indent",
        key: "indent",
        file: "crates/neovm-core/src/emacs_core/indent/subrs.rs",
        base_cfg: "always",
        unlisted_layer: None,
        event_id: None,
    },
    BatchDef {
        module: "file_notify",
        key: "file_notify",
        file: "crates/neovm-eval/src/emacs_core/lisp/native/builtins/file_notify/subrs.rs",
        base_cfg: "always",
        unlisted_layer: Some("eval"),
        event_id: Some("eval.file_notify"),
    },
    BatchDef {
        module: "crate::emacs_core::dbusbind",
        key: "dbusbind",
        file: "crates/neovm-system/src/emacs_core/system/dbusbind/subrs.rs",
        base_cfg: "dbus",
        unlisted_layer: None,
        event_id: None,
    },
    BatchDef {
        module: "crate::emacs_core::sqlite",
        key: "sqlite",
        file: "crates/neovm-system/src/emacs_core/system/sqlite/subrs.rs",
        base_cfg: "sqlite",
        unlisted_layer: None,
        event_id: None,
    },
    BatchDef {
        module: "crate::emacs_core::font",
        key: "font",
        file: "crates/neovm-display/src/emacs_core/display/font/subrs.rs",
        base_cfg: "always",
        unlisted_layer: None,
        event_id: None,
    },
    BatchDef {
        module: "crate::emacs_core::neo::effects",
        key: "effects",
        file: "crates/neovm-display/src/emacs_core/display/neo/effects/subrs.rs",
        base_cfg: "always",
        unlisted_layer: None,
        event_id: None,
    },
    BatchDef {
        module: "crate::emacs_core::neo::modifiers",
        key: "modifiers",
        file: "crates/neovm-display/src/emacs_core/display/neo/modifiers/subrs.rs",
        base_cfg: "always",
        unlisted_layer: None,
        event_id: None,
    },
    BatchDef {
        module: "crate::emacs_core::neo::chrome",
        key: "chrome",
        file: "crates/neovm-display/src/emacs_core/display/neo/chrome/subrs.rs",
        base_cfg: "always",
        unlisted_layer: None,
        event_id: None,
    },
];

/// The eval-owned registrar pieces, in the order the core manifest runs them.
const EVAL_EVENT_ORDER: [&str; 7] = [
    "eval.lcms",
    "eval.data",
    "eval.ordinary",
    "eval.fboundp",
    "eval.file_notify",
    "eval.event_properties",
    "eval.public",
];

/// One registered name in canonical order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameRow {
    pub name: String,
    pub min: u16,
    pub max: Option<u16>,
    pub layer: String,
    pub cfg: String,
}

/// One plan step. A zero-count step is the `init_event_symbol_properties`
/// marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanStep {
    pub kind: &'static str,
    pub id: String,
    pub layer: String,
    pub first: usize,
    pub count: usize,
    pub marker: &'static str,
}

#[path = "canonical_subr_metadata_args.rs"]
mod metadata_args;
#[path = "canonical_subr_metadata_inventory.rs"]
mod metadata_inventory;
#[path = "canonical_subr_static_guard.rs"]
mod static_guard;

/// One resolved, immutable descriptor home. This source record also feeds
/// the fail-closed Core/unit single-home guard; it is not a runtime catalog.
#[derive(Clone, Debug)]
struct StaticHome {
    module: String,
    static_ident: String,
    file: String,
    initializer_factory: Option<String>,
    name: String,
    min: u16,
    max: Option<u16>,
    cfg: String,
}

/// A parsed `SubrSpec`/`EvaluatorSubr` entry.
#[derive(Clone)]
struct ParsedSpec {
    name: String,
    min: u16,
    max: Option<u16>,
    cfg: String,
    static_home: Option<StaticHome>,
}

/// Where a registration row came from: the core manifest itself, one of the
/// extracted batch registrars, or an eval-owned registrar piece. Display-run
/// ids are derived from this so they survive unrelated plan edits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Inline,
    Batch(&'static str),
}

enum Item {
    Row {
        row: NameRow,
        event: Option<&'static str>,
        origin: Origin,
    },
    Marker(&'static str),
    /// One production display-step registrar call: the rows that follow
    /// belong to this step, and the step id is the registrar's descriptive
    /// plan id (never positional).
    DisplayStepStart {
        id: String,
    },
}

fn read(repo_root: &Path, relative: &str) -> Result<String, String> {
    std::fs::read_to_string(repo_root.join(relative))
        .map_err(|error| format!("read {relative}: {error}"))
}

/// Byte index of the `{`/`[` that opens the block containing `from`.
fn open_delimiter(text: &str, from: usize, open: u8) -> Result<usize, String> {
    text[from..]
        .find(open as char)
        .map(|offset| from + offset)
        .ok_or_else(|| format!("no `{}` after byte {from}", open as char))
}

/// Index of the matching brace, bracket or parenthesis, skipping strings
/// and comments.
fn matching_delimiter(text: &str, open_index: usize, open: u8, close: u8) -> Result<usize, String> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = open_index;
    let mut block_comment_depth = 0usize;
    while i < bytes.len() {
        let byte = bytes[i];
        if block_comment_depth > 0 {
            if byte == b'/' && bytes.get(i + 1) == Some(&b'*') {
                block_comment_depth += 1;
                i += 2;
                continue;
            }
            if byte == b'*' && bytes.get(i + 1) == Some(&b'/') {
                block_comment_depth -= 1;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if byte == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if byte == b'/' && bytes.get(i + 1) == Some(&b'*') {
            block_comment_depth = 1;
            i += 2;
            continue;
        }
        if byte == b'"' {
            i += 1;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' => i += 2,
                    b'"' => {
                        i += 1;
                        break;
                    }
                    _ => i += 1,
                }
            }
            continue;
        }
        if byte == open {
            depth += 1;
        } else if byte == close {
            depth -= 1;
            if depth == 0 {
                return Ok(i);
            }
        }
        i += 1;
    }
    Err(format!(
        "unbalanced `{}` at byte {open_index}",
        open as char
    ))
}

fn brace_block_after(text: &str, from: usize) -> Result<&str, String> {
    let open = open_delimiter(text, from, b'{')?;
    let close = matching_delimiter(text, open, b'{', b'}')?;
    Ok(&text[open..=close])
}

fn bracket_block_after(text: &str, from: usize) -> Result<&str, String> {
    let open = open_delimiter(text, from, b'[')?;
    let close = matching_delimiter(text, open, b'[', b']')?;
    Ok(&text[open..=close])
}

fn line_of(text: &str, byte: usize) -> usize {
    text[..byte].bytes().filter(|byte| *byte == b'\n').count() + 1
}

/// Parse the cfg label carried by an attribute or cfg_select arm line.
fn cfg_from_directive(line: &str) -> Option<&'static str> {
    // The x11-only surface form must be recognized before the generic
    // windows check below, which would otherwise match its substring.
    if line.contains("not(any(windows, target_os = \"macos\"))") {
        return Some("x11-only");
    }
    // The negated surface form must be recognized before the bare macOS test
    // below, which would otherwise match its substring.
    if line.contains("not(target_os = \"macos\")") {
        return Some("not-macos");
    }
    if line.contains("target_os = \"macos\"") {
        return Some("macos");
    }
    if line.contains("windows") {
        return Some("windows");
    }
    if line.contains("target_os = \"linux\"") {
        return Some("linux");
    }
    if line.contains("neomacs_have_dbus") {
        return Some("dbus");
    }
    if line.contains("feature = \"sqlite\"") || line.contains("\"sqlite\"") {
        return Some("sqlite");
    }
    if line.contains("neomacs_have_lcms2") {
        return Some("lcms2");
    }
    if line.contains("\"vm-profile\"") || line.contains("vm-profile") {
        return Some("vm-profile");
    }
    None
}

/// The cfg attached to the item that starts at `spec_start`, given the byte
/// just after the previous item (`chunk_start`). Attributes and cfg_select
/// arms sit between the previous item's end and this item's constructor, so
/// only that chunk may contribute: scanning further back would leak a
/// preceding item's cfg onto this one.
fn item_cfg(chunk: &str, base_cfg: &str) -> String {
    for line in chunk.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with("#[cfg(") || trimmed.starts_with('(') || trimmed.contains("=>") {
            if let Some(label) = cfg_from_directive(trimmed) {
                return label.to_owned();
            }
        }
    }
    base_cfg.to_owned()
}

/// Parse the `SubrSpec::new(...)`/`SubrSpec::fixedN(...)` starting at `at`.
/// Returns the spec and the byte just past the parsed constructor text, so
/// the next spec's cfg chunk starts after this item rather than inside it.
fn parse_subr_spec_at(
    text: &str,
    at: usize,
    chunk_start: usize,
    base_cfg: &str,
) -> Result<(ParsedSpec, usize), String> {
    let rest = text
        .get(at..)
        .ok_or_else(|| "spec offset out of range".to_owned())?;
    let after = rest
        .strip_prefix("SubrSpec::")
        .ok_or_else(|| "expected SubrSpec::".to_owned())?;
    let kind_len = after
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        .count();
    let kind = &after[..kind_len];
    let after_kind = after[kind_len..]
        .trim_start()
        .strip_prefix('(')
        .ok_or_else(|| format!("malformed SubrSpec::{kind} constructor"))?
        .trim_start();
    let after_quote = after_kind
        .strip_prefix('"')
        .ok_or_else(|| format!("SubrSpec::{kind} has no string literal name"))?;
    let name_len = after_quote
        .find('"')
        .ok_or_else(|| "unterminated subr name".to_owned())?;
    let name = &after_quote[..name_len];
    // Byte just past the parsed constructor text. Every slice derived below
    // is a suffix of `text`, so its start offset is `text.len() - len`.
    let parsed_end;
    let (min, max) = if let Some(digits) = kind.strip_prefix("fixed") {
        // `fixedN(name, function, FixedMinN::Variant)`: the N in the
        // constructor is the maximum arity; the minimum is the enum variant.
        let arity: u16 = digits
            .parse()
            .map_err(|_| format!("malformed fixed arity `{kind}`"))?;
        if arity == 0 {
            parsed_end = text.len() - after_quote[name_len..].len() + 1;
            (0, Some(0))
        } else {
            let search = &after_quote[name_len..];
            let token = format!("FixedMin{digits}::");
            let marker_at = search
                .find(&token)
                .ok_or_else(|| format!("{name}: no {token} minimum"))?;
            let after_marker = &search[marker_at + token.len()..];
            let variant_len = after_marker
                .bytes()
                .take_while(|byte| byte.is_ascii_alphanumeric())
                .count();
            let variant = &after_marker[..variant_len];
            let min = fixed_minimum_value(variant)
                .ok_or_else(|| format!("{name}: unknown FixedMin{digits}::{variant}"))?;
            if min > arity {
                return Err(format!(
                    "{name}: fixed{arity} minimum {min} exceeds the maximum"
                ));
            }
            parsed_end = text.len() - after_marker[variant_len..].len();
            (min, Some(arity))
        }
    } else if kind == "new" {
        let search = &after_quote[name_len..];
        let arity_at = search
            .find("SubrArity::new(")
            .ok_or_else(|| format!("{name}: no SubrArity::new"))?;
        let args = &search[arity_at + "SubrArity::new(".len()..];
        let (min, rest) = parse_u16_prefix(args.trim_start())
            .ok_or_else(|| format!("{name}: malformed SubrArity min"))?;
        let rest = rest
            .trim_start()
            .strip_prefix(',')
            .ok_or_else(|| format!("{name}: missing comma in SubrArity"))?
            .trim_start();
        let (max, after_max) = if let Some(after_none) = rest.strip_prefix("None") {
            (None, after_none)
        } else {
            let inner = rest
                .strip_prefix("Some(")
                .ok_or_else(|| format!("{name}: malformed SubrArity max"))?;
            let (max, after) = parse_u16_prefix(inner)
                .ok_or_else(|| format!("{name}: malformed SubrArity max"))?;
            (Some(max), after)
        };
        parsed_end = text.len() - after_max.len();
        (min, max)
    } else {
        return Err(format!("unknown SubrSpec constructor `{kind}`"));
    };
    Ok((
        ParsedSpec {
            name: name.to_owned(),
            min,
            max,
            cfg: item_cfg(&text[chunk_start..at], base_cfg),
            static_home: None,
        },
        parsed_end,
    ))
}

/// Value of a `FixedMinN` variant (`Zero`..=`Six`); `N` bounds the variant
/// by the constructor, this maps its name to the arity it stands for.
fn fixed_minimum_value(variant: &str) -> Option<u16> {
    Some(match variant {
        "Zero" => 0,
        "One" => 1,
        "Two" => 2,
        "Three" => 3,
        "Four" => 4,
        "Five" => 5,
        "Six" => 6,
        _ => return None,
    })
}

fn parse_u16_prefix(text: &str) -> Option<(u16, &str)> {
    let len = text.bytes().take_while(u8::is_ascii_digit).count();
    if len == 0 {
        return None;
    }
    let value = text[..len].parse().ok()?;
    Some((value, &text[len..]))
}

/// One registration piece inside a display-step registrar: a call to an
/// existing batch registrar, or an inline `SubrSpec` constant.
enum DisplayPiece {
    Batch(&'static BatchDef),
    Inline(ParsedSpec),
}

/// The byte that starts the attribute/doc block immediately above `item_at`.
fn leading_attribute_start(text: &str, item_at: usize) -> usize {
    let mut start = text[..item_at]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    loop {
        if start == 0 {
            return 0;
        }
        let previous_end = start - 1;
        let previous_start = text[..previous_end]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        let previous = text[previous_start..previous_end].trim();
        if previous.starts_with("#[") || previous.starts_with("///") {
            start = previous_start;
        } else {
            return start;
        }
    }
}

/// The inline `const SPEC_...: SubrSpec = ...` declarations of the display
/// step module, by constant name.
fn display_spec_consts(source: &str) -> Result<BTreeMap<String, ParsedSpec>, String> {
    let mut consts = BTreeMap::new();
    for (offset, _) in source.match_indices("const SPEC_") {
        let after = &source[offset + "const ".len()..];
        let name_len = after
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        let name = &after[..name_len];
        let spec_at = source[offset..]
            .find("SubrSpec::")
            .map(|index| offset + index)
            .ok_or_else(|| format!("display spec const {name} has no SubrSpec"))?;
        let chunk_start = leading_attribute_start(source, offset);
        let (spec, _) = parse_subr_spec_at(source, spec_at, chunk_start, "always")
            .map_err(|error| format!("display spec const {name}: {error}"))?;
        if consts.insert(name.to_owned(), spec).is_some() {
            return Err(format!("duplicate display spec const {name}"));
        }
    }
    Ok(consts)
}

/// Parse the display crate's typed step registrars in source order. Each step
/// function registers either batch registrars or inline spec constants; the
/// core manifest calls one such function at each production display position.
fn parse_display_steps(
    repo_root: &Path,
) -> Result<(Vec<String>, BTreeMap<String, Vec<DisplayPiece>>), String> {
    let source = read(repo_root, DISPLAY_SUBRS)?;
    let consts = display_spec_consts(&source)?;
    let mut steps = BTreeMap::new();
    let mut order = Vec::new();
    let mut searched = 0usize;
    while let Some(relative) = source[searched..].find("pub fn register_") {
        let fn_at = searched + relative;
        let after = &source[fn_at + "pub fn register_".len()..];
        let id_len = after
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        let id = &after[..id_len];
        if id.is_empty() {
            return Err("display registrar with an empty step id".to_owned());
        }
        let body = brace_block_after(&source, fn_at)?;
        let mut marks: Vec<(usize, bool)> = Vec::new();
        for (offset, _) in body.match_indices("::register_subrs(ctx);") {
            marks.push((offset, false));
        }
        for (offset, _) in body.match_indices("ctx.register_subr(") {
            marks.push((offset, true));
        }
        marks.sort_unstable();
        let mut pieces = Vec::with_capacity(marks.len());
        for (position, inline) in marks {
            if inline {
                let after_call = &body[position + "ctx.register_subr(".len()..];
                let name_len = after_call
                    .bytes()
                    .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                    .count();
                let const_name = &after_call[..name_len];
                let spec = consts.get(const_name).ok_or_else(|| {
                    format!("display step `{id}` registers unknown const `{const_name}`")
                })?;
                pieces.push(DisplayPiece::Inline(spec.clone()));
            } else {
                let start = body[..position]
                    .rfind(|character: char| {
                        !(character.is_ascii_alphanumeric() || character == '_' || character == ':')
                    })
                    .map(|index| index + 1)
                    .unwrap_or(0);
                let module = &body[start..position];
                let batch = BATCHES
                    .iter()
                    .find(|batch| batch.module == module)
                    .ok_or_else(|| {
                        format!("display step `{id}` calls unknown batch registrar `{module}`")
                    })?;
                pieces.push(DisplayPiece::Batch(batch));
            }
        }
        if pieces.is_empty() {
            return Err(format!("display step `{id}` registers nothing"));
        }
        if body.matches("register_subr").count() != pieces.len() {
            return Err(format!(
                "display step `{id}` has an unmodeled registration form"
            ));
        }
        if steps.insert(id.to_owned(), pieces).is_some() {
            return Err(format!("duplicate display step `{id}`"));
        }
        order.push(id.to_owned());
        searched = fn_at + "pub fn register_".len() + id_len;
    }
    if steps.is_empty() {
        return Err(format!("{DISPLAY_SUBRS}: no display step registrars found"));
    }
    Ok((order, steps))
}

/// Every spec in a `define_subrs!` block (or a whole file when it has none),
/// in source order. Each spec's cfg comes only from the text between the
/// previous spec and this one.
fn specs_in(text: &str, base_cfg: &str) -> Result<Vec<ParsedSpec>, String> {
    let block = if let Some(at) = text.find("define_subrs! {") {
        brace_block_after(text, at)?
    } else {
        text
    };
    let mut specs = Vec::new();
    let mut search_from = 0usize;
    let mut chunk_start = 0usize;
    while let Some(offset) = block[search_from..].find("SubrSpec::") {
        let at = search_from + offset;
        let (spec, end) = parse_subr_spec_at(block, at, chunk_start, base_cfg)?;
        search_from = end.max(at + 1);
        chunk_start = end;
        specs.push(spec);
    }
    if specs.is_empty() {
        return Err("no SubrSpec entries found".to_owned());
    }
    Ok(specs)
}

/// The public evaluator batch (`EVALUATOR_SUBRS`). Its entries carry their
/// arity as the second argument instead of a `SubrSpec`.
fn public_eval_specs(text: &str) -> Result<Vec<ParsedSpec>, String> {
    let at = text
        .find("EVALUATOR_SUBRS: &[EvaluatorSubr] = &[")
        .ok_or_else(|| "EVALUATOR_SUBRS table not found".to_owned())?;
    // The first `[` after the declaration belongs to the `&[EvaluatorSubr]`
    // type; the table's initializer bracket follows the `= &`.
    let initializer = at
        + text[at..]
            .find("= &[")
            .ok_or_else(|| "EVALUATOR_SUBRS initializer not found".to_owned())?
        + "= &".len();
    let block = bracket_block_after(text, initializer)?;
    let mut specs = Vec::new();
    let mut search_from = 0usize;
    while let Some(offset) = block[search_from..].find("EvaluatorSubr::") {
        let at = search_from + offset;
        let after = &block[at + "EvaluatorSubr::".len()..];
        let kind_len = after
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        let kind = &after[..kind_len];
        if kind != "callable" && kind != "special" {
            return Err(format!(
                "unexpected EvaluatorSubr constructor `{kind}` at {}",
                line_of(block, at)
            ));
        }
        let after_kind = after[kind_len..]
            .trim_start()
            .strip_prefix('(')
            .ok_or_else(|| format!("malformed EvaluatorSubr::{kind}"))?
            .trim_start();
        let after_quote = after_kind
            .strip_prefix('"')
            .ok_or_else(|| format!("EvaluatorSubr::{kind} has no name"))?;
        let name_len = after_quote
            .find('"')
            .ok_or_else(|| "unterminated EvaluatorSubr name".to_owned())?;
        let name = &after_quote[..name_len];
        let search = &after_quote[name_len..];
        let arity_at = search
            .find("SubrArity::new(")
            .ok_or_else(|| format!("{name}: no SubrArity::new"))?;
        let args = &search[arity_at + "SubrArity::new(".len()..];
        let (min, rest) = parse_u16_prefix(args.trim_start())
            .ok_or_else(|| format!("{name}: malformed SubrArity min"))?;
        let rest = rest
            .trim_start()
            .strip_prefix(',')
            .ok_or_else(|| format!("{name}: missing comma in SubrArity"))?
            .trim_start();
        let max = if rest.starts_with("None") {
            None
        } else {
            let inner = rest
                .strip_prefix("Some(")
                .ok_or_else(|| format!("{name}: malformed SubrArity max"))?;
            let (max, _) = parse_u16_prefix(inner)
                .ok_or_else(|| format!("{name}: malformed SubrArity max"))?;
            Some(max)
        };
        specs.push(ParsedSpec {
            name: name.to_owned(),
            min,
            max,
            cfg: "always".to_owned(),
            static_home: None,
        });
        search_from = at + "EvaluatorSubr::".len() + 1;
    }
    if specs.is_empty() {
        return Err("EVALUATOR_SUBRS table is empty".to_owned());
    }
    Ok(specs)
}

/// Which evaluator-chain event a `.method(ctx)` call is.
fn eval_event_for(method: &str) -> Option<&'static str> {
    match method {
        ".register_data(ctx)" => Some("eval.data"),
        ".register_ordinary_eval(ctx)" => Some("eval.ordinary"),
        ".register_fboundp(ctx)" => Some("eval.fboundp"),
        ".initialize_event_properties(ctx)" => Some("eval.event_properties"),
        ".register_public_eval(ctx)" => Some("eval.public"),
        _ => None,
    }
}

enum Event {
    Direct { at: usize, spec: ParsedSpec },
    Batch { at: usize, module: String },
    Eval { at: usize, id: &'static str },
    DisplayStep { at: usize, id: String },
}

/// A direct registration is either its existing literal descriptor expression
/// or a canonical immutable-static path. Bound the argument first: never
/// borrow a constructor from the following statement/registration.
fn registration_spec(repo_root: &Path, argument: &str) -> Result<ParsedSpec, String> {
    use syn::parse::Parser;
    let expressions = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
        .parse_str(argument)
        .map_err(|error| format!("malformed descriptor argument: {error}"))?;
    if expressions.len() != 1 {
        return Err("registration requires exactly one descriptor".to_owned());
    }
    // Historical literal registrations may attach these metadata builders.
    // An arbitrary expression containing a literal is not a declaration.
    let mut root = &expressions[0];
    loop {
        match root {
            syn::Expr::MethodCall(method) => {
                if ![
                    "requires_eval_state",
                    "interactive",
                    "placeholder",
                    "disabled_command",
                    "leaf",
                ]
                .iter()
                .any(|name| method.method == *name)
                {
                    return Err("unsupported descriptor metadata builder".to_owned());
                }
                root = &method.receiver;
            }
            syn::Expr::Const(expression) if expression.attrs.is_empty() => {
                let [syn::Stmt::Expr(tail, None)] = expression.block.stmts.as_slice() else {
                    return Err("inline const descriptor requires one tail expression".to_owned());
                };
                root = tail;
            }
            _ => break,
        }
    }
    if let syn::Expr::Macro(expression) = root {
        if !path_is(
            &expression.mac.path,
            &["crate", "emacs_core", "subr", "world_subr"],
        ) {
            return Err("unsupported literal descriptor macro".to_owned());
        }
        let literal: syn::Expr = syn::parse2(expression.mac.tokens.clone())
            .map_err(|error| format!("inline descriptor: {error}"))?;
        return literal_spec(&literal, "inline descriptor");
    }
    static_spec(repo_root, &expressions[0])
}

/// Closed static grammar: one immutable, eval-owned descriptor in an
/// unconditional flat builtin file module. Core and unit copy that actual
/// static. Its initializer may use one same-file private const factory;
/// callable providers are never registration expressions.
fn static_spec(repo_root: &Path, expression: &syn::Expr) -> Result<ParsedSpec, String> {
    let syn::Expr::Path(reference) = expression else {
        return Err("descriptor reference must be one canonical static path".to_owned());
    };
    let path = &reference.path;
    let prefix = ["neovm_eval", "emacs_core", "builtins"];
    if !reference.attrs.is_empty()
        || reference.qself.is_some()
        || path.leading_colon.is_some()
        || path.segments.len() != 5
        || path
            .segments
            .iter()
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        || !path
            .segments
            .iter()
            .take(3)
            .zip(prefix)
            .all(|(segment, name)| segment.ident == name)
    {
        return Err("unsupported canonical static descriptor path".to_owned());
    }
    let module_name = path.segments[3].ident.to_string();
    let static_ident = path.segments[4].ident.to_string();
    let modules = read(
        repo_root,
        "crates/neovm-eval/src/emacs_core/lisp/native/builtins/mod.rs",
    )?;
    let modules = syn::parse_file(&modules).map_err(|error| error.to_string())?;
    if modules
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
    {
        return Err("cfg-gated builtin module file is unsupported".to_owned());
    }
    let declarations: Vec<_> = modules
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Mod(module) if module.ident == module_name => Some(module),
            _ => None,
        })
        .collect();
    if declarations.len() != 1
        || !matches!(declarations[0].vis, syn::Visibility::Public(_))
        || declarations[0].unsafety.is_some()
        || declarations[0].content.is_some()
        || declarations[0]
            .attrs
            .iter()
            .any(|attr| !attr.path().is_ident("doc"))
    {
        return Err("descriptor module must be one unconditional public file module".to_owned());
    }
    let relative =
        format!("crates/neovm-eval/src/emacs_core/lisp/native/builtins/{module_name}.rs");
    let source = read(repo_root, &relative)?;
    let file = syn::parse_file(&source).map_err(|error| format!("{relative}: {error}"))?;
    if file
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
    {
        return Err("cfg-gated descriptor file is unsupported".to_owned());
    }
    let definitions: Vec<_> = file
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Static(item) if item.ident == static_ident => Some(item),
            _ => None,
        })
        .collect();
    if definitions.len() != 1 {
        return Err(format!(
            "{relative}: expected exactly one static {static_ident}"
        ));
    }
    let definition = definitions[0];
    if !matches!(definition.vis, syn::Visibility::Public(_))
        || !matches!(definition.mutability, syn::StaticMutability::None)
        || definition
            .attrs
            .iter()
            .any(|attr| !attr.path().is_ident("doc"))
        || !matches!(definition.ty.as_ref(), syn::Type::Path(ty)
            if ty.qself.is_none() && ty.path.is_ident("SubrSpec"))
    {
        return Err(format!(
            "{relative}: expected unconditional pub immutable static {static_ident}: SubrSpec"
        ));
    }
    let (initializer, initializer_factory) = match definition.expr.as_ref() {
        syn::Expr::Macro(_) | syn::Expr::MethodCall(_) => (definition.expr.as_ref(), None),
        syn::Expr::Call(call) if call.attrs.is_empty() && call.args.is_empty() => {
            let syn::Expr::Path(function) = call.func.as_ref() else {
                return Err("static factory must be a same-file private const function".to_owned());
            };
            if !function.attrs.is_empty()
                || function.qself.is_some()
                || function.path.leading_colon.is_some()
                || function.path.segments.len() != 1
                || !matches!(
                    function.path.segments[0].arguments,
                    syn::PathArguments::None
                )
            {
                return Err("static factory must be one same-file identifier".to_owned());
            }
            let name = function.path.segments[0].ident.to_string();
            let factories: Vec<_> = file
                .items
                .iter()
                .filter_map(|item| match item {
                    syn::Item::Fn(function) if function.sig.ident == name => Some(function),
                    _ => None,
                })
                .collect();
            if factories.len() != 1 {
                return Err(format!(
                    "{relative}: expected exactly one private const factory {name}"
                ));
            }
            let factory = factories[0];
            let signature = &factory.sig;
            if !matches!(factory.vis, syn::Visibility::Inherited)
                || factory.modifiers.defaultness.is_some()
                || signature.constness.is_none()
                || signature.asyncness.is_some()
                || !matches!(signature.safety, syn::Safety::Default)
                || signature.abi.is_some()
                || !signature.generics.params.is_empty()
                || signature.generics.where_clause.is_some()
                || !signature.inputs.is_empty()
                || signature.variadic.is_some()
                || !matches!(&signature.output, syn::ReturnType::Type(_, ty)
                    if matches!(ty.as_ref(), syn::Type::Path(path)
                        if path.qself.is_none() && path.path.is_ident("SubrSpec")))
                || factory
                    .attrs
                    .iter()
                    .any(|attr| !attr.path().is_ident("doc"))
            {
                return Err(format!(
                    "{relative}: factory {name} must be unconditional private const fn () -> SubrSpec"
                ));
            }
            let [syn::Stmt::Expr(tail, None)] = factory.block.stmts.as_slice() else {
                return Err("static factory requires one world literal tail".to_owned());
            };
            (tail, Some(name))
        }
        _ => {
            return Err(
                "static initializer must be one world literal or private const factory call"
                    .to_owned(),
            );
        }
    };
    let literal = initializer_world_literal(repo_root, &relative, &file, initializer)?;
    let mut spec = literal_spec(&literal, &static_ident)?;
    spec.static_home = Some(StaticHome {
        module: module_name,
        static_ident,
        file: relative,
        initializer_factory,
        name: spec.name.clone(),
        min: spec.min,
        max: spec.max,
        cfg: spec.cfg.clone(),
    });
    Ok(spec)
}

/// Metadata builders are permitted only at the single descriptor's source
/// initializer, never on a Core or unit copy of its canonical static.
fn initializer_world_literal(
    root: &Path,
    home_file: &str,
    home_ast: &syn::File,
    expr: &syn::Expr,
) -> Result<syn::Expr, String> {
    let mut receiver = expr;
    while let syn::Expr::MethodCall(method) = receiver {
        let builder = method.method.to_string();
        if !method.attrs.is_empty()
            || method.turbofish.is_some()
            || !matches!(
                builder.as_str(),
                "requires_eval_state" | "interactive" | "placeholder" | "disabled_command" | "leaf"
            )
        {
            return Err("unsupported static descriptor metadata builder".to_owned());
        }
        metadata_args::validate(root, home_file, home_ast, &builder, &method.args)?;
        receiver = &method.receiver;
    }
    let syn::Expr::Macro(world) = receiver else {
        return Err("static initializer requires one world literal".to_owned());
    };
    if !world.attrs.is_empty()
        || !path_is(
            &world.mac.path,
            &["crate", "emacs_core", "subr", "world_subr"],
        )
    {
        return Err("unsupported static descriptor macro".to_owned());
    }
    let literal = syn::parse2(world.mac.tokens.clone())
        .map_err(|error| format!("static descriptor: {error}"))?;
    // Keep the world marker's contents a bare literal constructor. The chain
    // is outside that marker and cannot hide another constructor expression.
    literal_spec(&literal, "static descriptor")?;
    Ok(literal)
}

/// Read the literal descriptor AST itself, never a later textual constructor.
fn literal_spec(expression: &syn::Expr, call: &str) -> Result<ParsedSpec, String> {
    let syn::Expr::Call(constructor) = expression else {
        return Err(format!("{call}: expected one literal SubrSpec constructor"));
    };
    let syn::Expr::Path(path) = constructor.func.as_ref() else {
        return Err(format!("{call}: unsupported descriptor constructor"));
    };
    let segments = &path.path.segments;
    if path.qself.is_some()
        || path.path.leading_colon.is_some()
        || segments.len() != 2
        || segments[0].ident != "SubrSpec"
        || segments
            .iter()
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
    {
        return Err(format!("{call}: unsupported descriptor constructor"));
    }
    let Some(syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(name),
        ..
    })) = constructor.args.first()
    else {
        return Err(format!("{call}: descriptor name must be a literal"));
    };
    let kind = segments[1].ident.to_string();
    let arity_expression = constructor.args.get(2);
    let number = |expression: &syn::Expr| match expression {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(value),
            ..
        }) => value.base10_parse::<u16>().ok(),
        _ => None,
    };
    let arity = || -> Option<(u16, Option<u16>)> {
        if kind == "new" {
            let syn::Expr::Call(arity) = arity_expression? else {
                return None;
            };
            let syn::Expr::Path(path) = arity.func.as_ref() else {
                return None;
            };
            if path.qself.is_some()
                || !path_is(&path.path, &["SubrArity", "new"])
                || arity.args.len() != 2
            {
                return None;
            }
            let min = number(&arity.args[0])?;
            let max = match &arity.args[1] {
                syn::Expr::Path(path) if path.qself.is_none() && path.path.is_ident("None") => None,
                syn::Expr::Call(some) if some.args.len() == 1 => {
                    let syn::Expr::Path(path) = some.func.as_ref() else {
                        return None;
                    };
                    if path.qself.is_some() || !path.path.is_ident("Some") {
                        return None;
                    }
                    Some(number(&some.args[0])?)
                }
                _ => return None,
            };
            Some((min, max))
        } else {
            let max = kind.strip_prefix("fixed")?.parse::<u16>().ok()?;
            if max > 6 {
                return None;
            }
            if max == 0 && constructor.args.len() == 2 {
                return Some((0, Some(0)));
            }
            let syn::Expr::Path(path) = arity_expression? else {
                return None;
            };
            if path.qself.is_some() || path.path.segments.len() != 2 {
                return None;
            }
            let minimum = &path.path.segments;
            if path.path.leading_colon.is_some()
                || minimum
                    .iter()
                    .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
                || minimum[0].ident != format!("FixedMin{max}")
            {
                return None;
            }
            Some((
                fixed_minimum_value(&minimum[1].ident.to_string())?,
                Some(max),
            ))
        }
    };
    let (min, max) = arity().ok_or_else(|| format!("{call}: unsupported literal arity"))?;
    if (kind != "fixed0" && constructor.args.len() != 3) || max.is_some_and(|max| min > max) {
        return Err(format!("{call}: malformed descriptor arity"));
    }
    Ok(ParsedSpec {
        name: name.value(),
        min,
        max,
        cfg: "always".to_owned(),
        static_home: None,
    })
}

fn path_is(path: &syn::Path, expected: &[&str]) -> bool {
    path.leading_colon.is_none()
        && path.segments.len() == expected.len()
        && path.segments.iter().zip(expected).all(|(segment, name)| {
            segment.ident == *name && matches!(segment.arguments, syn::PathArguments::None)
        })
}

/// Scan the core manifest's `register_subrs` body for every registration
/// event, in source order.
fn manifest_events(repo_root: &Path) -> Result<(Vec<Event>, String), String> {
    let source = read(repo_root, CORE_MANIFEST)?;
    let fn_at = source
        .find("pub(crate) fn register_subrs(")
        .ok_or_else(|| "core manifest has no register_subrs".to_owned())?;
    let body = brace_block_after(&source, fn_at)?;
    let body_start = body.as_ptr() as usize - source.as_ptr() as usize;
    let mut events = Vec::new();
    for (offset, _) in body.match_indices("ctx.register_subr(") {
        let position = body_start + offset;
        let open = position + "ctx.register_subr".len();
        let close = matching_delimiter(&source, open, b'(', b')')?;
        let argument = source[open + 1..close].trim();
        let spec = registration_spec(repo_root, argument)
            .map_err(|error| format!("manifest line {}: {error}", line_of(&source, position)))?;
        events.push(Event::Direct { at: position, spec });
    }
    for offset in body.match_indices("::register_subrs(ctx);") {
        let end = body_start + offset.0;
        let start = body[..offset.0]
            .rfind(|character: char| {
                !(character.is_ascii_alphanumeric() || character == '_' || character == ':')
            })
            .map(|index| body_start + index + 1)
            .unwrap_or(body_start);
        events.push(Event::Batch {
            at: end,
            module: source[start..end].to_owned(),
        });
    }
    for (offset, _) in body.match_indices("display_subrs::register_") {
        let at = body_start + offset;
        let after = &body[offset + "display_subrs::register_".len()..];
        let id_len = after
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        let id = &after[..id_len];
        if id.is_empty() || !after[id_len..].trim_start().starts_with('(') {
            return Err(format!(
                "manifest line {}: malformed display_subrs registrar call",
                line_of(&source, at)
            ));
        }
        events.push(Event::DisplayStep {
            at,
            id: id.to_owned(),
        });
    }
    for method in [
        ".register_data(ctx)",
        ".register_ordinary_eval(ctx)",
        ".register_fboundp(ctx)",
        ".initialize_event_properties(ctx)",
        ".register_public_eval(ctx)",
    ] {
        for offset in body.match_indices(method) {
            let at = body_start + offset.0;
            let id =
                eval_event_for(method).ok_or_else(|| format!("unknown eval method {method}"))?;
            events.push(Event::Eval { at, id });
        }
    }
    events.sort_by_key(|event| match event {
        Event::Direct { at, .. }
        | Event::Batch { at, .. }
        | Event::Eval { at, .. }
        | Event::DisplayStep { at, .. } => *at,
    });
    Ok((events, source))
}

fn snapshot_map(repo_root: &Path) -> Result<BTreeMap<String, NameRow>, String> {
    let source = read(repo_root, UPPER_SOURCE)?;
    let rows = upper_snapshots::parse_upper_subrs(&source)?;
    let mut map = BTreeMap::new();
    for row in rows {
        let name = row.name.clone();
        if map
            .insert(
                name.clone(),
                NameRow {
                    name,
                    min: row.min,
                    max: row.max,
                    layer: row.layer,
                    cfg: row.cfg,
                },
            )
            .is_some()
        {
            return Err("duplicate name in the upper source".to_owned());
        }
    }
    Ok(map)
}

fn upper_row(map: &BTreeMap<String, NameRow>, spec: &ParsedSpec) -> Result<NameRow, String> {
    let row = map.get(&spec.name).ok_or_else(|| {
        format!(
            "upper name {:?} missing from the generated source",
            spec.name
        )
    })?;
    if row.min != spec.min || row.max != spec.max {
        return Err(format!(
            "{}: manifest arity {}({:?}) != generated upper arity {}({:?})",
            spec.name, spec.min, spec.max, row.min, row.max
        ));
    }
    Ok(row.clone())
}

/// Build the canonical item sequence.
fn canonical_items(repo_root: &Path) -> Result<Vec<Item>, String> {
    let (events, source) = manifest_events(repo_root)?;
    let homes: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::Direct { spec, .. } => spec.static_home.clone(),
            _ => None,
        })
        .collect();
    metadata_inventory::validate_classification(repo_root)?;
    static_guard::validate_static_subr_sources(repo_root, &homes)?;
    let upper = snapshot_map(repo_root)?;
    let eval_source = read(repo_root, EVAL_SUBRS)?;
    let eval_batch = specs_in(&eval_source, "always")?;
    let public_eval = public_eval_specs(&eval_source)?;
    let data_batch = specs_in(
        &read(
            repo_root,
            "crates/neovm-eval/src/emacs_core/runtime/data/subrs.rs",
        )?,
        "always",
    )?;

    let display_steps = if repo_root.join(DISPLAY_SUBRS).is_file() {
        Some(parse_display_steps(repo_root)?)
    } else {
        None
    };
    let mut declared_display_order: Vec<String> = Vec::new();

    let mut batches: BTreeMap<&'static str, Vec<ParsedSpec>> = BTreeMap::new();
    for batch in BATCHES {
        batches.insert(
            batch.module,
            specs_in(&read(repo_root, batch.file)?, batch.base_cfg)?,
        );
    }

    let mut items = Vec::new();
    for event in events {
        match event {
            Event::Direct { at, spec } => {
                let row = upper_row(&upper, &spec).map_err(|error| {
                    format!(
                        "manifest line {} direct registration: {error}",
                        line_of(&source, at)
                    )
                })?;
                items.push(Item::Row {
                    row,
                    event: None,
                    origin: Origin::Inline,
                });
            }
            Event::Batch { at, module } => {
                let batch = BATCHES
                    .iter()
                    .find(|batch| batch.module == module)
                    .ok_or_else(|| {
                        format!(
                            "manifest line {}: unknown batch registrar `{module}`",
                            line_of(&source, at)
                        )
                    })?;
                push_batch_rows(&mut items, &batches, &upper, batch).map_err(|error| {
                    format!(
                        "manifest line {}: batch `{module}`: {error}",
                        line_of(&source, at)
                    )
                })?;
            }
            Event::DisplayStep { at, id } => {
                let (_, steps) = display_steps.as_ref().ok_or_else(|| {
                    format!(
                        "manifest line {}: display registrar `{id}` but no {DISPLAY_SUBRS}",
                        line_of(&source, at)
                    )
                })?;
                declared_display_order.push(id.clone());
                let pieces = steps.get(&id).ok_or_else(|| {
                    format!(
                        "manifest line {}: display registrar `{id}` not found in {DISPLAY_SUBRS}",
                        line_of(&source, at)
                    )
                })?;
                items.push(Item::DisplayStepStart { id: id.clone() });
                for piece in pieces {
                    match piece {
                        DisplayPiece::Batch(batch) => {
                            push_batch_rows(&mut items, &batches, &upper, batch).map_err(
                                |error| {
                                    format!(
                                        "manifest line {}: display step `{id}`: {error}",
                                        line_of(&source, at)
                                    )
                                },
                            )?;
                        }
                        DisplayPiece::Inline(spec) => items.push(Item::Row {
                            row: upper_row(&upper, spec)?,
                            event: None,
                            origin: Origin::Inline,
                        }),
                    }
                }
            }
            Event::Eval { at, id } => match id {
                "eval.data" => {
                    push_eval_rows(&mut items, &data_batch, id);
                }
                "eval.ordinary" => {
                    push_eval_rows(&mut items, &eval_batch, id);
                }
                "eval.fboundp" => items.push(Item::Row {
                    row: NameRow {
                        name: "fboundp".to_owned(),
                        min: 1,
                        max: Some(1),
                        layer: "eval".to_owned(),
                        cfg: "always".to_owned(),
                    },
                    event: Some(id),
                    origin: Origin::Inline,
                }),
                "eval.event_properties" => {
                    let _ = at;
                    items.push(Item::Marker("init_event_symbol_properties"));
                }
                "eval.public" => {
                    push_eval_rows(&mut items, &public_eval, id);
                }
                other => return Err(format!("unhandled eval event `{other}`")),
            },
        }
    }
    // The display module's registrar functions, the manifest's calls and the
    // generated display segments are one order: a registrar the manifest
    // does not follow, or an extra call with no registrar, is a hard error
    // rather than silently missing rows.
    if let Some((order, _)) = &display_steps
        && declared_display_order != *order
    {
        return Err(format!(
            "manifest display registrars {:?} do not match the display module's registrar order {:?}",
            declared_display_order, order
        ));
    }
    Ok(items)
}

fn push_batch_rows(
    items: &mut Vec<Item>,
    batches: &BTreeMap<&'static str, Vec<ParsedSpec>>,
    upper: &BTreeMap<String, NameRow>,
    batch: &'static BatchDef,
) -> Result<(), String> {
    let module = batch.module;
    let specs = batches
        .get(module)
        .ok_or_else(|| format!("unparsed batch `{module}`"))?;
    for spec in specs {
        let row = match upper.get(&spec.name) {
            Some(row) => {
                if row.min != spec.min || row.max != spec.max {
                    return Err(format!(
                        "{}: batch arity {}({:?}) != generated upper arity {}({:?})",
                        spec.name, spec.min, spec.max, row.min, row.max
                    ));
                }
                row.clone()
            }
            None => {
                let layer = batch.unlisted_layer.ok_or_else(|| {
                    format!(
                        "{module}: {:?} missing from the generated upper source",
                        spec.name
                    )
                })?;
                NameRow {
                    name: spec.name.clone(),
                    min: spec.min,
                    max: spec.max,
                    layer: layer.to_owned(),
                    cfg: spec.cfg.clone(),
                }
            }
        };
        items.push(Item::Row {
            row,
            event: batch.event_id,
            origin: Origin::Batch(batch.key),
        });
    }
    Ok(())
}

fn push_eval_rows(items: &mut Vec<Item>, specs: &[ParsedSpec], event: &'static str) {
    for spec in specs {
        items.push(Item::Row {
            row: NameRow {
                name: spec.name.clone(),
                min: spec.min,
                max: spec.max,
                layer: "eval".to_owned(),
                cfg: spec.cfg.clone(),
            },
            event: Some(event),
            origin: Origin::Inline,
        });
    }
}

/// Make a subr name usable as a Rust identifier fragment: non-alphanumeric
/// runs become one underscore.
fn sanitize_display_component(name: &str) -> String {
    let mut out = String::new();
    let mut underscore = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character);
            underscore = false;
        } else if !underscore {
            out.push('_');
            underscore = true;
        }
    }
    out.trim_matches('_').to_owned()
}

/// Descriptive, stable id for one display run.
///
/// An all-inline run is named after its first subr (`selected-window` ->
/// `selected_window`); a run made of batch calls is named after its batch
/// keys in order (`terminal_shader_video_xwidget`, `font`,
/// `effects_modifiers_chrome`). A mixed run names each maximal inline group
/// after its first subr, prefixed with `inline_`, between the batch keys.
fn display_run_id(origins: &[Origin], rows: &[NameRow]) -> Result<String, String> {
    if rows.is_empty() || origins.len() != rows.len() {
        return Err("empty display run".to_owned());
    }
    if origins.iter().all(|origin| *origin == Origin::Inline) {
        let id = sanitize_display_component(&rows[0].name);
        if id.is_empty() {
            return Err(format!(
                "display run starts with an unusable name {:?}",
                rows[0].name
            ));
        }
        return Ok(id);
    }
    let mut parts: Vec<String> = Vec::new();
    let mut last: Option<Origin> = None;
    for (origin, row) in origins.iter().zip(rows) {
        match origin {
            Origin::Batch(key) => {
                if last != Some(Origin::Batch(key)) {
                    parts.push((*key).to_owned());
                }
            }
            Origin::Inline => {
                if last != Some(Origin::Inline) {
                    let component = sanitize_display_component(&row.name);
                    if component.is_empty() {
                        return Err(format!(
                            "display run has an unusable inline name {:?}",
                            row.name
                        ));
                    }
                    parts.push(format!("inline_{component}"));
                }
            }
        }
        last = Some(*origin);
    }
    let id = parts.join("_");
    if id.is_empty() {
        return Err("display run produced an empty id".to_owned());
    }
    Ok(id)
}

/// Cut the item sequence into plan steps.
fn build_plan(items: &[Item]) -> Result<(Vec<NameRow>, Vec<PlanStep>), String> {
    let mut rows = Vec::new();
    let mut origins: Vec<Origin> = Vec::new();
    let mut steps: Vec<PlanStep> = Vec::new();
    let mut open: Option<PlanStep> = None;
    let mut run_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut eval_seen: Vec<&'static str> = Vec::new();
    let mut pending_display: Option<String> = None;

    let close = |steps: &mut Vec<PlanStep>, open: &mut Option<PlanStep>| {
        if let Some(step) = open.take() {
            steps.push(step);
        }
    };

    for item in items {
        match item {
            Item::Marker(marker) => {
                if *marker != "init_event_symbol_properties" {
                    return Err(format!("unknown marker `{marker}`"));
                }
                close(&mut steps, &mut open);
                steps.push(PlanStep {
                    kind: "eval_call",
                    id: "eval.event_properties".to_owned(),
                    layer: "eval".to_owned(),
                    first: rows.len() + 1,
                    count: 0,
                    marker,
                });
                eval_seen.push("eval.event_properties");
            }
            Item::DisplayStepStart { id } => {
                close(&mut steps, &mut open);
                pending_display = Some(id.clone());
            }
            Item::Row { row, event, origin } => {
                let display_step = if row.layer == "display" {
                    pending_display.take()
                } else {
                    pending_display = None;
                    None
                };
                let mut extends = false;
                if let Some(current) = open.as_ref() {
                    match (event, current.kind) {
                        (Some(id), "eval_call") => {
                            extends = current.id == *id && current.marker.is_empty();
                        }
                        (None, "upper_segment") | (None, "display_segment") => {
                            extends = current.layer == row.layer;
                        }
                        _ => {}
                    }
                }
                if !extends {
                    close(&mut steps, &mut open);
                    let mut next_upper = |layer: &str| {
                        let counter = run_counts.entry(layer.to_owned()).or_insert(0);
                        *counter += 1;
                        format!("{layer}.run{counter:02}")
                    };
                    open = Some(match event {
                        Some(id) => {
                            if eval_seen.contains(id) {
                                return Err(format!("eval event `{id}` occurs twice"));
                            }
                            eval_seen.push(id);
                            PlanStep {
                                kind: "eval_call",
                                id: (*id).to_owned(),
                                layer: "eval".to_owned(),
                                first: rows.len() + 1,
                                count: 0,
                                marker: "",
                            }
                        }
                        None if row.layer == "display" => PlanStep {
                            kind: "display_segment",
                            // A split display step registrar supplies its id;
                            // otherwise it is filled in after the run is
                            // closed, from its origins. Display ids are
                            // descriptive, not positional (peer ruling
                            // 03:35:50Z (b)).
                            id: display_step.unwrap_or_default(),
                            layer: row.layer.clone(),
                            first: rows.len() + 1,
                            count: 0,
                            marker: "",
                        },
                        None => PlanStep {
                            kind: "upper_segment",
                            id: next_upper(&row.layer),
                            layer: row.layer.clone(),
                            first: rows.len() + 1,
                            count: 0,
                            marker: "",
                        },
                    });
                }
                if let Some(current) = open.as_mut() {
                    current.count += 1;
                }
                rows.push(row.clone());
                origins.push(*origin);
            }
        }
    }
    close(&mut steps, &mut open);

    // Display ids name the run by its origin batch(es) or its first subr,
    // never by position: inserting an unrelated plan row must not renumber
    // every later display variant. A split display step registrar already
    // carries its id; the run's origins must derive that same id, so the
    // split's typed registrars and the plan cannot silently diverge.
    let declared_display: Vec<String> = items
        .iter()
        .filter_map(|item| match item {
            Item::DisplayStepStart { id } => Some(id.clone()),
            _ => None,
        })
        .collect();
    let explicit_display: Vec<String> = steps
        .iter()
        .filter(|step| step.kind == "display_segment" && !step.id.is_empty())
        .map(|step| step.id.clone())
        .collect();
    if declared_display != explicit_display {
        return Err(format!(
            "display step ids {:?} do not match the display segments {:?}",
            declared_display, explicit_display
        ));
    }
    for step in steps
        .iter_mut()
        .filter(|step| step.kind == "display_segment")
    {
        let start = step.first - 1;
        let end = start + step.count;
        let id = display_run_id(&origins[start..end], &rows[start..end])?;
        if step.id.is_empty() {
            step.id = id;
        } else if step.id != id {
            return Err(format!(
                "display step id {:?} does not match its registration origin {:?}",
                step.id, id
            ));
        }
    }
    let mut seen_ids = std::collections::BTreeSet::new();
    for step in &steps {
        if !seen_ids.insert(step.id.as_str()) {
            return Err(format!("duplicate plan step id {:?}", step.id));
        }
    }

    // Every row is covered exactly once, in order.
    let mut expected = 1usize;
    for step in &steps {
        if step.first != expected {
            return Err(format!(
                "step {} starts at {} but {} was expected",
                step.id, step.first, expected
            ));
        }
        expected += step.count;
    }
    if expected != rows.len() + 1 {
        return Err(format!(
            "plan covers {} rows but the names table has {}",
            expected - 1,
            rows.len()
        ));
    }

    // The eval-owned pieces occur exactly once each, in production order.
    let expected_eval: Vec<&str> = EVAL_EVENT_ORDER.to_vec();
    if eval_seen != expected_eval {
        return Err(format!(
            "eval steps are {:?}, expected {:?}",
            eval_seen, expected_eval
        ));
    }
    for step in &steps {
        if step.id == "eval.event_properties" && step.marker != "init_event_symbol_properties" {
            return Err("event-properties step lost its marker".to_owned());
        }
    }
    Ok((rows, steps))
}

fn validate_names(names: &[NameRow]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for row in names {
        if !seen.insert(row.name.as_str()) {
            return Err(format!("duplicate canonical name {:?}", row.name));
        }
        if row.max.is_some_and(|max| max < row.min) {
            return Err(format!(
                "{}: max {:?} is below min {}",
                row.name, row.max, row.min
            ));
        }
    }
    Ok(())
}

fn canonical_upper_names(names: &[NameRow]) -> Vec<&str> {
    names
        .iter()
        .filter(|row| row.layer != "eval")
        .map(|row| row.name.as_str())
        .collect()
}

fn snapshot_names(repo_root: &Path) -> Result<Vec<String>, String> {
    let source = read(repo_root, UPPER_SOURCE)?;
    Ok(upper_snapshots::parse_upper_subrs(&source)?
        .into_iter()
        .map(|row| row.name)
        .collect())
}

pub fn render_names(names: &[NameRow]) -> String {
    let mut out = String::from("name\tmin\tmax\tlayer\tcfg\n");
    for row in names {
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

pub fn render_plan(steps: &[PlanStep]) -> String {
    let mut out = String::from("step\tkind\tid\tlayer\tfirst\tcount\tmarker\n");
    for (index, step) in steps.iter().enumerate() {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            index + 1,
            step.kind,
            step.id,
            step.layer,
            step.first,
            step.count,
            if step.marker.is_empty() {
                "-"
            } else {
                step.marker
            }
        ));
    }
    out
}

/// Build the two artifacts from the live sources without touching the tree.
/// This is what the eval cfg(test) plan-consistency test calls.
pub fn rendered(repo_root: &Path) -> Result<(String, String), String> {
    let items = canonical_items(repo_root)?;
    let (names, steps) = build_plan(&items)?;
    validate_names(&names)?;

    let mut canonical_upper = canonical_upper_names(&names);
    let mut snapshot = snapshot_names(repo_root)?;
    canonical_upper.sort_unstable();
    snapshot.sort_unstable();
    if canonical_upper != snapshot {
        return Err(format!(
            "canonical upper names ({} rows) differ from the generated source ({} rows)",
            canonical_upper.len(),
            snapshot.len()
        ));
    }
    Ok((render_names(&names), render_plan(&steps)))
}

/// Build the two artifacts and either write them or (with `check`) compare
/// them against the checked-in copies.
pub fn generate(repo_root: &Path, check: bool) -> Result<(), String> {
    let (names, plan) = rendered(repo_root)?;
    let artifacts = [(NAMES_TSV, names), (PLAN_TSV, plan)];
    for (relative, rendered) in artifacts {
        let path = repo_root.join(relative);
        if check {
            let checked_in = std::fs::read_to_string(&path)
                .map_err(|error| format!("read {relative}: {error}"))?;
            if checked_in != rendered {
                return Err(format!(
                    "{relative} is stale; run `cargo xtask canonical-subr-plan`"
                ));
            }
        } else {
            std::fs::write(&path, rendered)
                .map_err(|error| format!("write {relative}: {error}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod provider_tests {
    use super::*;

    const PROVIDER_CALL: &str = "neovm_eval::emacs_core::builtins::misc_pure::identity_subr_spec()";
    const PROVIDER_FILE: &str =
        "crates/neovm-eval/src/emacs_core/lisp/native/builtins/misc_pure.rs";
    const PROVIDER: &str = r#"
        #[doc(hidden)]
        pub const fn identity_subr_spec() -> SubrSpec {
            crate::emacs_core::subr::world_subr!(SubrSpec::new(
                "identity", NativeFn::ContextVec(identity_entry), SubrArity::new(1, Some(1)),
            ))
        }
    "#;

    const STATIC_REFERENCE: &str = "neovm_eval::emacs_core::builtins::misc_pure::IDENTITY_SUBR";
    const STATIC_FILE: &str = "crates/neovm-eval/src/emacs_core/lisp/native/builtins/misc_pure.rs";
    const STATIC_DEFINITION: &str = r#"
        #[doc(hidden)]
        pub static IDENTITY_SUBR: SubrSpec = crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "identity", NativeFn::ContextVec(identity_entry), SubrArity::new(1, Some(1)),
        ));
    "#;
    const PRIVATE_FACTORY: &str = r#"
        const fn identity_spec() -> SubrSpec {
            crate::emacs_core::subr::world_subr!(SubrSpec::new(
                "identity", NativeFn::ContextVec(identity_entry), SubrArity::new(1, Some(1)),
            ))
        }
        #[doc(hidden)]
        pub static IDENTITY_SUBR: SubrSpec = identity_spec();
    "#;

    fn fixture(manifest: &str, provider: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for (relative, text) in [
            (CORE_MANIFEST, manifest),
            (STATIC_FILE, provider),
            (
                "crates/neovm-eval/src/emacs_core/lisp/native/builtins/mod.rs",
                "#[doc(hidden)] pub mod misc_pure;",
            ),
        ] {
            let path = root.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        root
    }

    #[test]
    fn registration_cannot_borrow_a_later_unregistered_literal() {
        let root = fixture(
            r#"
            pub(crate) fn register_subrs(ctx: &mut Context) {
                ctx.register_subr(unknown_provider());
                let unused = SubrSpec::new("wrong", entry, SubrArity::new(0, None));
            }
        "#,
            "",
        );
        assert!(
            manifest_events(root.path()).is_err(),
            "unknown call must fail closed"
        );
    }

    #[test]
    fn provider_resolution_preserves_manifest_order_and_multiplicity() {
        let manifest = format!(
            r#"
            pub(crate) fn register_subrs(ctx: &mut Context) {{
                ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
                    "before", entry, SubrArity::new(0, None))));
                ctx.register_subr({STATIC_REFERENCE});
                ctx.register_subr({STATIC_REFERENCE});
                ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
                    "after", entry, SubrArity::new(2, Some(3)))));
            }}
        "#
        );
        let root = fixture(&manifest, STATIC_DEFINITION);
        let (events, _) = manifest_events(root.path()).unwrap();
        let resolved: Vec<_> = events
            .into_iter()
            .map(|event| match event {
                Event::Direct { spec, .. } => (spec.name, spec.min, spec.max, spec.cfg),
                _ => panic!("unexpected event"),
            })
            .collect();
        assert_eq!(
            resolved,
            vec![
                ("before".into(), 0, None, "always".into()),
                ("identity".into(), 1, Some(1), "always".into()),
                ("identity".into(), 1, Some(1), "always".into()),
                ("after".into(), 2, Some(3), "always".into()),
            ]
        );
    }

    #[test]
    fn legacy_inline_const_world_descriptor_preserves_leaf_shape() {
        // Exact production get-char-property shape, including its outer
        // const block and the metadata chain on the world-marked fixed ABI.
        let argument = r#"const {
            crate::emacs_core::subr::world_subr!(SubrSpec::fixed3(
                "get-char-property",
                crate::emacs_core::textprop_entries::builtin_get_char_property_3,
                FixedMin3::Two,
            ))
            .leaf(&leaves::GET_CHAR_PROPERTY)
        },"#;
        let spec = registration_spec(Path::new("unused"), argument).unwrap();
        assert_eq!(
            (spec.name.as_str(), spec.min, spec.max, spec.cfg.as_str()),
            ("get-char-property", 2, Some(3), "always")
        );
        for invalid in [
            argument.replace("const {", "{"),
            argument.replace("const {", "const { let side_effect = 0;"),
            argument.replace(
                ".leaf(&leaves::GET_CHAR_PROPERTY)",
                ".leaf(&leaves::GET_CHAR_PROPERTY);",
            ),
            argument.replace(".leaf(&leaves::GET_CHAR_PROPERTY)", ".unknown_metadata()"),
        ] {
            assert!(
                registration_spec(Path::new("unused"), &invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn legacy_inline_metadata_builders_remain_supported() {
        let argument = r#"crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "legacy", entry, SubrArity::new(0, Some(2))
        ))
        .requires_eval_state()
        .interactive(interactive_spec)
        .placeholder(NoEvalPlaceholder::Nil)
        .disabled_command()
        .leaf(&LEAF)"#;
        let spec = registration_spec(Path::new("unused"), argument).unwrap();
        assert_eq!(
            (spec.name.as_str(), spec.min, spec.max),
            ("legacy", 0, Some(2))
        );
        assert!(spec.static_home.is_none());
    }

    #[test]
    fn callable_public_provider_is_rejected_even_with_valid_literal_body() {
        let root = fixture("", PROVIDER);
        assert!(
            registration_spec(
                root.path(),
                "neovm_eval::emacs_core::builtins::misc_pure::identity_subr_spec()"
            )
            .is_err(),
            "callable public descriptor factories must fail closed"
        );
    }

    #[test]
    fn static_initializer_metadata_chains_preserve_literal_projection() {
        for chain in [
            ".requires_eval_state()",
            ".requires_eval_state().interactive(crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(\"p\",))",
            ".placeholder(crate::emacs_core::subr::NoEvalPlaceholder::Nil)",
            ".interactive(crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::Form(crate::emacs_core::builtins::misc_pure::interactive_form))",
            ".leaf(&crate::emacs_core::builtins::leaves::SAMPLE_LEAF)",
        ] {
            let definition =
                STATIC_DEFINITION.replace("        ));", &format!("        )){chain};"));
            let form = "\nfn interactive_form() -> neovm_value::value::Value { opaque_form() }";
            let root = fixture("", &format!("{definition}{form}"));
            for (relative, source) in [
                ("crates/neovm-eval/src/lib.rs", "pub mod emacs_core;"),
                (
                    "crates/neovm-eval/src/emacs_core/mod.rs",
                    r#"#[path="runtime/eval/mod.rs"] pub mod eval; #[path="lisp/native/subr/mod.rs"] pub mod subr; #[path="lisp/native/builtins/mod.rs"] pub mod builtins;"#,
                ),
                (
                    "crates/neovm-eval/src/emacs_core/runtime/eval/mod.rs",
                    "pub mod interactive_registry;",
                ),
                (
                    "crates/neovm-eval/src/emacs_core/runtime/eval/interactive_registry.rs",
                    "use neovm_value::value::Value; pub enum BuiltinInteractiveSpec { String(&'static str), Form(fn() -> Value) }",
                ),
                (
                    "crates/neovm-eval/src/emacs_core/lisp/native/subr/mod.rs",
                    "pub enum NoEvalPlaceholder { Nil, FixnumZero, WindowLineHeight } pub(crate) mod leaf;",
                ),
                (
                    "crates/neovm-eval/src/emacs_core/lisp/native/subr/leaf.rs",
                    "pub struct LeafSpec;",
                ),
                (
                    "crates/neovm-eval/src/emacs_core/lisp/native/builtins/mod.rs",
                    "pub mod misc_pure; pub mod leaves;",
                ),
                (
                    "crates/neovm-eval/src/emacs_core/lisp/native/builtins/leaves/mod.rs",
                    "use crate::emacs_core::subr::leaf::LeafSpec; pub static SAMPLE_LEAF: LeafSpec = LeafSpec::new(opaque_effects());",
                ),
            ] {
                let path = root.path().join(relative);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, source).unwrap();
            }
            let spec = registration_spec(root.path(), &format!("{STATIC_REFERENCE},")).unwrap();
            assert_eq!(
                (spec.name.as_str(), spec.min, spec.max),
                ("identity", 1, Some(1))
            );
            assert!(spec.static_home.unwrap().initializer_factory.is_none());
            let private =
                PRIVATE_FACTORY.replace("            ))", &format!("            )){chain}"));
            std::fs::write(root.path().join(STATIC_FILE), format!("{private}{form}")).unwrap();
            let spec = registration_spec(root.path(), STATIC_REFERENCE).unwrap();
            assert_eq!(
                spec.static_home.unwrap().initializer_factory.as_deref(),
                Some("identity_spec")
            );
        }
    }

    #[test]
    fn static_initializer_metadata_chains_reject_recomputation_and_unknown_shapes() {
        for chain in [
            ".unknown_metadata()",
            ".requires_eval_state(1)",
            ".requires_eval_state::<u8>()",
            ".interactive(build_interactive())",
            ".placeholder(make_placeholder())",
        ] {
            let definition =
                STATIC_DEFINITION.replace("        ));", &format!("        )){chain};"));
            let root = fixture("", &definition);
            assert!(
                registration_spec(root.path(), STATIC_REFERENCE).is_err(),
                "accepted {chain}"
            );
            let private =
                PRIVATE_FACTORY.replace("            ))", &format!("            )){chain}"));
            std::fs::write(root.path().join(STATIC_FILE), private).unwrap();
            assert!(
                registration_spec(root.path(), STATIC_REFERENCE).is_err(),
                "accepted private {chain}"
            );
        }
        for definition in [
            PRIVATE_FACTORY.replace("identity_spec();", "identity_spec().requires_eval_state();"),
            STATIC_DEFINITION.replace(" = crate::", " = #[allow(dead_code)] crate::"),
            STATIC_DEFINITION.replace("        ));", "        ).requires_eval_state());"),
            format!("{STATIC_DEFINITION}\n{STATIC_DEFINITION}"),
        ] {
            let root = fixture("", &definition);
            assert!(
                registration_spec(root.path(), STATIC_REFERENCE).is_err(),
                "accepted {definition}"
            );
        }
    }

    #[test]
    fn immutable_static_records_its_single_home() {
        let root = fixture("", STATIC_DEFINITION);
        let spec = registration_spec(root.path(), STATIC_REFERENCE).unwrap();
        let home = spec.static_home.expect("actual static source home");
        assert_eq!(home.module, "misc_pure");
        assert_eq!(home.static_ident, "IDENTITY_SUBR");
        assert_eq!(home.file, STATIC_FILE);
        assert!(home.initializer_factory.is_none());
        assert_eq!(
            (home.name.as_str(), home.min, home.max, home.cfg.as_str()),
            ("identity", 1, Some(1), "always")
        );
    }

    #[test]
    fn static_initializer_accepts_only_same_file_private_const_factory() {
        let root = fixture("", PRIVATE_FACTORY);
        let spec = registration_spec(root.path(), STATIC_REFERENCE).unwrap();
        assert_eq!(
            (spec.name.as_str(), spec.min, spec.max, spec.cfg.as_str()),
            ("identity", 1, Some(1), "always")
        );
        assert_eq!(
            spec.static_home.unwrap().initializer_factory.as_deref(),
            Some("identity_spec")
        );
        for invalid in [
            PRIVATE_FACTORY.replace("const fn", "pub const fn"),
            PRIVATE_FACTORY.replace("const fn", "pub(crate) const fn"),
            PRIVATE_FACTORY.replace("const fn", "fn"),
            PRIVATE_FACTORY.replace("const fn", "const unsafe fn"),
            PRIVATE_FACTORY.replace("const fn", "#[cfg(unix)] const fn"),
            PRIVATE_FACTORY.replace("const fn", "#[inline] const fn"),
            PRIVATE_FACTORY.replace("identity_spec() ->", "identity_spec<T>() ->"),
            PRIVATE_FACTORY.replace("identity_spec() ->", "identity_spec(x: usize) ->"),
            PRIVATE_FACTORY.replace("-> SubrSpec", "-> Alias"),
            PRIVATE_FACTORY.replace(
                "            crate::",
                "            let side_effect = 0;\n            crate::",
            ),
            PRIVATE_FACTORY.replace("identity_spec();", "self::identity_spec();"),
            PRIVATE_FACTORY.replace("identity_spec();", "identity_spec::<u8>();"),
            PRIVATE_FACTORY.replace("identity_spec();", "identity_spec(1);"),
            PRIVATE_FACTORY.replace("identity_spec();", "elsewhere::identity_spec();"),
            PRIVATE_FACTORY.replace("crate::emacs_core::subr::world_subr!", "alias!"),
            "const fn identity_spec() -> SubrSpec { IDENTITY_SUBR }
             pub static IDENTITY_SUBR: SubrSpec = identity_spec();"
                .to_owned(),
            format!("{PRIVATE_FACTORY}\nconst fn identity_spec() -> SubrSpec {{ IDENTITY_SUBR }}"),
        ] {
            std::fs::write(root.path().join(STATIC_FILE), &invalid).unwrap();
            assert!(
                registration_spec(root.path(), STATIC_REFERENCE).is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn unsupported_provider_calls_and_definitions_fail_closed() {
        let root = fixture("", PROVIDER);
        for call in [
            "alias()",
            "neovm_eval::emacs_core::builtins::misc_pure::identity_subr_spec(1)",
            "neovm_eval::emacs_core::builtins::misc_pure::identity_subr_spec::<u8>()",
            "neovm_eval::emacs_core::builtins::misc_pure::MISSING()",
            "neovm_eval::emacs_core::builtins::misc_pure::identity_subr_spec().requires_eval_state()",
        ] {
            assert!(
                registration_spec(root.path(), call).is_err(),
                "accepted {call}"
            );
        }
        for provider in [
            PROVIDER.replace("pub const fn", "pub fn"),
            PROVIDER.replace("identity_subr_spec()", "identity_subr_spec(x: usize)"),
            PROVIDER.replace("#[doc(hidden)]", "#[cfg(unix)]"),
            PROVIDER.replace("crate::emacs_core::subr::world_subr!", "alias!"),
            PROVIDER.replace("SubrSpec::new(", "{ let side_effect = 0; SubrSpec::new("),
            PROVIDER.replace("Some(1)", "Some(MAX)"),
            PROVIDER.replace("Some(1)", "Some(0)"),
            PROVIDER.replace("\"identity\"", "NAME"),
            format!("{PROVIDER}\n{PROVIDER}"),
            format!("// {}", PROVIDER.replace('\n', "\n// ")),
            PROVIDER.replace(
                "            crate::",
                "            let side_effect = 0;\n            crate::",
            ),
        ] {
            std::fs::write(root.path().join(PROVIDER_FILE), &provider).unwrap();
            assert!(
                registration_spec(root.path(), PROVIDER_CALL).is_err(),
                "accepted {provider}"
            );
        }
        std::fs::write(root.path().join(PROVIDER_FILE), PROVIDER).unwrap();
        for module in [
            "#[cfg(unix)] pub mod misc_pure;",
            "pub use elsewhere as misc_pure;",
            "#[path = \"elsewhere.rs\"] pub mod misc_pure;",
            "pub mod misc_pure {}",
            "pub mod misc_pure; pub mod misc_pure;",
        ] {
            std::fs::write(
                root.path()
                    .join("crates/neovm-eval/src/emacs_core/lisp/native/builtins/mod.rs"),
                module,
            )
            .unwrap();
            assert!(
                registration_spec(root.path(), PROVIDER_CALL).is_err(),
                "accepted {module}"
            );
        }
    }

    #[test]
    fn unsupported_static_references_and_definitions_fail_closed() {
        let root = fixture("", STATIC_DEFINITION);
        for reference in [
            "alias",
            "neovm_eval::emacs_core::builtins::misc_pure::MISSING",
            "neovm_eval::emacs_core::builtins::misc_pure::IDENTITY_SUBR()",
            "neovm_eval::emacs_core::builtins::misc_pure::IDENTITY_SUBR::<u8>",
            "neovm_eval::emacs_core::builtins::misc_pure::<u8>::IDENTITY_SUBR",
            "neovm_eval::emacs_core::builtins::misc_pure::nested::IDENTITY_SUBR",
            "neovm_eval::emacs_core::builtins::misc_pure::IDENTITY_SUBR.requires_eval_state()",
            "(neovm_eval::emacs_core::builtins::misc_pure::IDENTITY_SUBR)",
            "&neovm_eval::emacs_core::builtins::misc_pure::IDENTITY_SUBR",
            "const { neovm_eval::emacs_core::builtins::misc_pure::IDENTITY_SUBR }",
            "::neovm_eval::emacs_core::builtins::misc_pure::IDENTITY_SUBR",
        ] {
            assert!(
                registration_spec(root.path(), reference).is_err(),
                "accepted {reference}"
            );
        }
        for definition in [
            STATIC_DEFINITION.replace("pub static", "pub const"),
            STATIC_DEFINITION.replace("pub static", "static"),
            STATIC_DEFINITION.replace("pub static", "pub(crate) static"),
            STATIC_DEFINITION.replace("pub static", "pub static mut"),
            STATIC_DEFINITION.replace(": SubrSpec", ": Alias"),
            STATIC_DEFINITION.replace("#[doc(hidden)]", "#[cfg(unix)]"),
            STATIC_DEFINITION.replace("crate::emacs_core::subr::world_subr!", "alias!"),
            STATIC_DEFINITION.replace("SubrSpec::new(", "{ let side_effect = 0; SubrSpec::new("),
            STATIC_DEFINITION.replace("Some(1)", "Some(MAX)"),
            STATIC_DEFINITION.replace("Some(1)", "Some(0)"),
            STATIC_DEFINITION.replace("\"identity\"", "NAME"),
            format!("{STATIC_DEFINITION}\n{STATIC_DEFINITION}"),
            format!("// {}", STATIC_DEFINITION.replace('\n', "\n// ")),
            "pub use elsewhere::IDENTITY_SUBR;".to_owned(),
            "pub static IDENTITY_SUBR: SubrSpec = ALIAS;".to_owned(),
            "pub static IDENTITY_SUBR: SubrSpec = { ALIAS };".to_owned(),
            format!("#![cfg(unix)]\n{STATIC_DEFINITION}"),
        ] {
            std::fs::write(root.path().join(STATIC_FILE), &definition).unwrap();
            assert!(
                registration_spec(root.path(), STATIC_REFERENCE).is_err(),
                "accepted {definition}"
            );
        }
        std::fs::write(root.path().join(STATIC_FILE), STATIC_DEFINITION).unwrap();
        for module in [
            "#[cfg(unix)] pub mod misc_pure;",
            "pub use elsewhere as misc_pure;",
            "#[path = \"elsewhere.rs\"] pub mod misc_pure;",
            "pub mod misc_pure {}",
            "pub mod misc_pure; pub mod misc_pure;",
            "pub(crate) mod misc_pure;",
            "#![cfg(unix)] pub mod misc_pure;",
        ] {
            std::fs::write(
                root.path()
                    .join("crates/neovm-eval/src/emacs_core/lisp/native/builtins/mod.rs"),
                module,
            )
            .unwrap();
            assert!(
                registration_spec(root.path(), STATIC_REFERENCE).is_err(),
                "accepted {module}"
            );
        }
    }

    #[test]
    fn core_classification_source_resolves_single_statics_in_original_order() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = read(&root, CORE_MANIFEST).unwrap();
        let (events, _) = manifest_events(&root).unwrap();
        let direct: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Direct { spec, .. } => Some(spec),
                _ => None,
            })
            .collect();
        for (name, provider, before, after, min, max) in [
            (
                "command-modes",
                "COMMAND_MODES_SUBR",
                "get-byte",
                "find-charset-region",
                1,
                Some(1),
            ),
            (
                "commandp",
                "COMMANDP_SUBR",
                "call-interactively",
                "command-remapping",
                1,
                Some(2),
            ),
        ] {
            let call = format!(
                "ctx.register_subr(neovm_eval::emacs_core::builtins::misc_eval::{provider});"
            );
            assert_eq!(
                source.matches(&call).count(),
                1,
                "{name}: source static reference"
            );
            let index = direct.iter().position(|spec| spec.name == name).unwrap();
            assert_eq!(direct.iter().filter(|spec| spec.name == name).count(), 1);
            assert_eq!(direct[index - 1].name, before);
            assert_eq!(direct[index + 1].name, after);
            assert_eq!(
                (
                    direct[index].min,
                    direct[index].max,
                    direct[index].cfg.as_str()
                ),
                (min, max, "always")
            );
        }
    }

    #[test]
    fn core_completion_source_resolves_single_statics_in_original_order() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = read(&root, CORE_MANIFEST).unwrap();
        let (events, _) = manifest_events(&root).unwrap();
        let direct: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Direct { spec, .. } => Some(spec),
                _ => None,
            })
            .collect();
        for (name, provider, before, after) in [
            (
                "try-completion",
                "TRY_COMPLETION_SUBR",
                "read-variable",
                "all-completions",
            ),
            (
                "all-completions",
                "ALL_COMPLETIONS_SUBR",
                "try-completion",
                "test-completion",
            ),
            (
                "test-completion",
                "TEST_COMPLETION_SUBR",
                "all-completions",
                "completion--flex-cost-gotoh",
            ),
        ] {
            let call = format!(
                "ctx.register_subr(neovm_eval::emacs_core::builtins::misc_eval::{provider});"
            );
            assert_eq!(
                source.matches(&call).count(),
                1,
                "{name}: source static reference"
            );
            let index = direct.iter().position(|spec| spec.name == name).unwrap();
            assert_eq!(direct.iter().filter(|spec| spec.name == name).count(), 1);
            assert_eq!(direct[index - 1].name, before);
            assert_eq!(direct[index + 1].name, after);
            assert_eq!(
                (
                    direct[index].min,
                    direct[index].max,
                    direct[index].cfg.as_str()
                ),
                (2, Some(3), "always")
            );
        }
    }

    #[test]
    fn core_identity_event_resolves_the_single_production_descriptor() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = read(&root, CORE_MANIFEST).unwrap();
        assert_eq!(
            source
                .matches(&format!("ctx.register_subr({STATIC_REFERENCE});"))
                .count(),
            1
        );
        let (events, _) = manifest_events(&root).unwrap();
        let direct: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Direct { spec, .. } => Some(spec),
                _ => None,
            })
            .collect();
        let index = direct
            .iter()
            .position(|spec| spec.name == "identity")
            .unwrap();
        assert_eq!(
            direct.iter().filter(|spec| spec.name == "identity").count(),
            1
        );
        assert_eq!(direct[index - 1].name, "string");
        assert_eq!(direct[index + 1].name, "ngettext");
        assert_eq!(
            (
                direct[index].min,
                direct[index].max,
                direct[index].cfg.as_str()
            ),
            (1, Some(1), "always")
        );
    }

    #[test]
    fn core_reverse_event_resolves_the_single_production_descriptor() {
        const REVERSE_PROVIDER_CALL: &str =
            "neovm_eval::emacs_core::builtins::misc_pure::REVERSE_SUBR";
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = read(&root, CORE_MANIFEST).unwrap();
        assert_eq!(
            source
                .matches(&format!("ctx.register_subr({REVERSE_PROVIDER_CALL});"))
                .count(),
            1
        );
        let (events, _) = manifest_events(&root).unwrap();
        let direct: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Direct { spec, .. } => Some(spec),
                _ => None,
            })
            .collect();
        let index = direct
            .iter()
            .position(|spec| spec.name == "reverse")
            .unwrap();
        assert_eq!(
            direct.iter().filter(|spec| spec.name == "reverse").count(),
            1
        );
        assert_eq!(direct[index - 1].name, "cl-type-of");
        assert_eq!(direct[index + 1].name, "copy-alist");
        assert_eq!(
            (
                direct[index].min,
                direct[index].max,
                direct[index].cfg.as_str()
            ),
            (1, Some(1), "always")
        );
    }

    #[test]
    fn core_subrp_event_resolves_the_single_production_descriptor() {
        const SUBRP_PROVIDER_CALL: &str = "neovm_eval::emacs_core::builtins::misc_pure::SUBRP_SUBR";
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = read(&root, CORE_MANIFEST).unwrap();
        assert_eq!(
            source
                .matches(&format!("ctx.register_subr({SUBRP_PROVIDER_CALL});"))
                .count(),
            1
        );
        let (events, _) = manifest_events(&root).unwrap();
        let direct: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Direct { spec, .. } => Some(spec),
                _ => None,
            })
            .collect();
        let index = direct.iter().position(|spec| spec.name == "subrp").unwrap();
        assert_eq!(direct.iter().filter(|spec| spec.name == "subrp").count(), 1);
        assert_eq!(direct[index - 1].name, "buffer-substring-no-properties");
        assert_eq!(direct[index + 1].name, "*");
        assert_eq!(
            (
                direct[index].min,
                direct[index].max,
                direct[index].cfg.as_str()
            ),
            (1, Some(1), "always")
        );
    }
}
