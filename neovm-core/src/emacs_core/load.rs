//! File loading and module system (require/provide/load).

use super::builtins::collections::builtin_make_hash_table;
use super::error::{EvalError, Flow, map_flow, signal};
use super::intern::{intern, resolve_sym};
use super::keymap::is_list_keymap;
use super::value::{Value, ValueKind, VecLikeType, list_to_vec};
use super::value_reader::ReadSymbolShorthands;
use crate::emacs_core::error::LispCondition;
use crate::heap_types::LispString;
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs;
#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use strum::{EnumString, IntoStaticStr};

fn load_string_text(value: &Value) -> Option<String> {
    // Used for error display, loaddefs file-name filtering and symbol-name
    // collection — all ASCII/Unicode, for which to_utf8_lossy is exact.
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
}

fn load_display_string(value: &LispString) -> String {
    crate::emacs_core::emacs_char::to_utf8_lossy(value.as_bytes())
}

pub(crate) fn cannot_open_load_file_signal(file: &LispString) -> Flow {
    signal(
        LispCondition::FileMissing,
        vec![
            Value::string("Cannot open load file"),
            Value::string("No such file or directory"),
            Value::heap_string(file.clone()),
        ],
    )
}

fn load_name_equal(left: &LispString, right: &LispString) -> bool {
    crate::emacs_core::value::equal_value(
        &Value::heap_string(left.clone()),
        &Value::heap_string(right.clone()),
        0,
    )
}

#[cfg(not(unix))]
fn load_runtime_string(value: &LispString) -> String {
    // Issue #131: only used for tilde-expanding a load path into a Rust string;
    // a lossy UTF-8 rendering is correct here and avoids the storage-string
    // sentinel scheme.
    crate::emacs_core::emacs_char::to_utf8_lossy(value.as_bytes())
}

fn load_path_lisp_string(path: &Path) -> LispString {
    super::fileio::path_to_lisp_file_name(path)
}

fn load_path_buf(value: &LispString) -> PathBuf {
    super::fileio::lisp_file_name_to_path_buf(value)
}

fn load_found_effective(found: &LispString) -> LispString {
    // GNU's compute_found_effective only diverges from FOUND for native-elisp
    // loads. NeoVM doesn't model that path yet, so keep the split in the API
    // now and return FOUND unchanged.
    found.clone()
}

fn load_hist_file_name(
    eval: &super::eval::Context,
    requested: &LispString,
    found: &LispString,
) -> LispString {
    let found_effective = load_found_effective(found);
    if !eval
        .obarray()
        .symbol_value("purify-flag")
        .is_some_and(|value| value.is_truthy())
    {
        return found_effective;
    }

    let found_path = load_path_buf(&found_effective);
    let Some(file_name) = found_path.file_name() else {
        return found_effective;
    };
    let requested_path = load_path_buf(requested);
    let directory = requested_path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty());

    match directory {
        Some(dir) => load_path_lisp_string(&dir.join(file_name)),
        None => super::fileio::path_to_lisp_file_name(Path::new(file_name)),
    }
}

/// EOL-aware coding-system name for an Emacs-extended UTF-8 source file.
fn source_emacs_coding(bytes: &[u8]) -> &'static str {
    match detect_source_eol(bytes) {
        SourceEol::Unix => "utf-8-emacs-unix",
        SourceEol::Dos => "utf-8-emacs-dos",
        SourceEol::Mac => "utf-8-emacs-mac",
    }
}

/// Decode Emacs-extended UTF-8 source straight to a faithful `LispString`
/// (Emacs internal bytes) — issue #131. Non-Unicode source character literals
/// (e.g. `?\xF6\xA0\x87\x8A` -> 0x1A01CA) keep their real codes as extended
/// Emacs bytes instead of the in-Unicode storage sentinels, and the reader's
/// LispString source mode reads them directly. No storage-string round-trip.
pub(crate) fn decode_emacs_utf8_source_lisp(bytes: &[u8]) -> LispString {
    crate::encoding::decode_bytes_to_lisp_string(bytes, source_emacs_coding(bytes))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceEol {
    Unix,
    Dos,
    Mac,
}

fn detect_source_eol(bytes: &[u8]) -> SourceEol {
    let mut saw_lf = false;
    let mut saw_crlf = false;
    let mut saw_lone_cr = false;
    let mut idx = 0;
    while idx < bytes.len() {
        match bytes[idx] {
            b'\n' => saw_lf = true,
            b'\r' => {
                if bytes.get(idx + 1) == Some(&b'\n') {
                    saw_crlf = true;
                    idx += 1;
                } else {
                    saw_lone_cr = true;
                }
            }
            _ => {}
        }
        idx += 1;
    }
    if saw_lf {
        SourceEol::Unix
    } else if saw_crlf {
        SourceEol::Dos
    } else if saw_lone_cr {
        SourceEol::Mac
    } else {
        SourceEol::Unix
    }
}

pub(crate) fn decode_emacs_utf8(bytes: &[u8]) -> String {
    fn push_extended_char_or_escape(out: &mut String, code: u32) {
        if out.ends_with('?') {
            // Replace the extended char with `\x<HEX>` escape so the
            // parser reads it as an integer code point.
            out.push_str(&format!("\\x{:X}", code));
        } else {
            // Outside character literal context, use replacement char.
            out.push('\u{FFFD}');
        }
    }

    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // ASCII byte — fast path.
        if b < 0x80 {
            out.push(b as char);
            i += 1;
            continue;
        }
        // Valid 2-byte UTF-8 (C2-DF).
        if b >= 0xC2 && b <= 0xDF && i + 1 < bytes.len() && (bytes[i + 1] & 0xC0) == 0x80 {
            if let Some(s) = std::str::from_utf8(&bytes[i..i + 2]).ok() {
                out.push_str(s);
                i += 2;
                continue;
            }
        }
        // Valid 3-byte UTF-8 (E0-EF).
        if b >= 0xE0
            && b <= 0xEF
            && i + 2 < bytes.len()
            && (bytes[i + 1] & 0xC0) == 0x80
            && (bytes[i + 2] & 0xC0) == 0x80
        {
            if let Some(s) = std::str::from_utf8(&bytes[i..i + 3]).ok() {
                out.push_str(s);
                i += 3;
                continue;
            }
        }
        // Valid standard 4-byte UTF-8 (F0-F4, code point <= 10FFFF).
        if b >= 0xF0
            && b <= 0xF4
            && i + 3 < bytes.len()
            && (bytes[i + 1] & 0xC0) == 0x80
            && (bytes[i + 2] & 0xC0) == 0x80
            && (bytes[i + 3] & 0xC0) == 0x80
        {
            if let Some(s) = std::str::from_utf8(&bytes[i..i + 4]).ok() {
                out.push_str(s);
                i += 4;
                continue;
            }
        }
        // Extended 4-byte (F5-F7): Emacs-internal code point > U+10FFFF.
        if b >= 0xF5
            && b <= 0xF7
            && i + 3 < bytes.len()
            && (bytes[i + 1] & 0xC0) == 0x80
            && (bytes[i + 2] & 0xC0) == 0x80
            && (bytes[i + 3] & 0xC0) == 0x80
        {
            let code = ((b as u32 & 0x07) << 18)
                | ((bytes[i + 1] as u32 & 0x3F) << 12)
                | ((bytes[i + 2] as u32 & 0x3F) << 6)
                | (bytes[i + 3] as u32 & 0x3F);
            push_extended_char_or_escape(&mut out, code);
            i += 4;
            continue;
        }
        // Extended 5-byte (F8-FB): still accepted by Emacs's internal UTF-8.
        if b >= 0xF8
            && b <= 0xFB
            && i + 4 < bytes.len()
            && (bytes[i + 1] & 0xC0) == 0x80
            && (bytes[i + 2] & 0xC0) == 0x80
            && (bytes[i + 3] & 0xC0) == 0x80
            && (bytes[i + 4] & 0xC0) == 0x80
        {
            let code = ((b as u32 & 0x03) << 24)
                | ((bytes[i + 1] as u32 & 0x3F) << 18)
                | ((bytes[i + 2] as u32 & 0x3F) << 12)
                | ((bytes[i + 3] as u32 & 0x3F) << 6)
                | (bytes[i + 4] as u32 & 0x3F);
            push_extended_char_or_escape(&mut out, code);
            i += 5;
            continue;
        }
        // Extended 6-byte (FC-FD): highest Emacs internal codes.
        if b >= 0xFC
            && b <= 0xFD
            && i + 5 < bytes.len()
            && (bytes[i + 1] & 0xC0) == 0x80
            && (bytes[i + 2] & 0xC0) == 0x80
            && (bytes[i + 3] & 0xC0) == 0x80
            && (bytes[i + 4] & 0xC0) == 0x80
            && (bytes[i + 5] & 0xC0) == 0x80
        {
            let code = ((b as u32 & 0x01) << 30)
                | ((bytes[i + 1] as u32 & 0x3F) << 24)
                | ((bytes[i + 2] as u32 & 0x3F) << 18)
                | ((bytes[i + 3] as u32 & 0x3F) << 12)
                | ((bytes[i + 4] as u32 & 0x3F) << 6)
                | (bytes[i + 5] as u32 & 0x3F);
            push_extended_char_or_escape(&mut out, code);
            i += 6;
            continue;
        }
        // Invalid byte: keep it available to the Lisp reader as a raw byte.
        out.push(char::from_u32(0xE000 + b as u32).expect("byte8 private-use marker"));
        i += 1;
    }
    out
}

/// Format a Value for human-readable error messages, resolving SymIds and heap-backed values.
fn format_value_for_error(v: &Value) -> String {
    match v.kind() {
        ValueKind::Symbol(sid) => super::intern::resolve_sym(sid).to_string(),
        ValueKind::String => format!("\"{}\"", load_string_text(v).expect("checked string")),
        ValueKind::Fixnum(n) => format!("{}", n),
        ValueKind::Nil => "nil".to_string(),
        ValueKind::T => "t".to_string(),
        ValueKind::Cons => {
            let car = v.cons_car();
            let cdr = v.cons_cdr();
            let car_s = format_value_for_error(&car);
            let cdr_s = format_value_for_error(&cdr);
            if cdr == Value::NIL {
                format!("({})", car_s)
            } else {
                format!("({} . {})", car_s, cdr_s)
            }
        }
        _other => format!("{:?}", v),
    }
}

fn format_eval_error_in_state(eval: &super::eval::Context, err: &EvalError) -> String {
    match err {
        EvalError::Signal {
            symbol,
            data,
            raw_data,
        } => {
            let payload = if let Some(raw) = raw_data {
                crate::emacs_core::error::print_value_in_state(eval, raw)
            } else if data.is_empty() {
                "nil".to_string()
            } else {
                crate::emacs_core::error::print_value_in_state(eval, &Value::list(data.clone()))
            };
            format!("({} {})", resolve_sym(*symbol), payload)
        }
        EvalError::UncaughtThrow { tag, value } => format!(
            "(throw {} {})",
            crate::emacs_core::error::print_value_in_state(eval, tag),
            crate::emacs_core::error::print_value_in_state(eval, value),
        ),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum LoadControlSignal {
    KillEmacs,
}

impl LoadControlSignal {
    fn from_symbol_id(symbol: super::intern::SymId) -> Option<Self> {
        resolve_sym(symbol).parse().ok()
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

fn is_kill_emacs_signal(err: &EvalError) -> bool {
    matches!(
        err,
        EvalError::Signal { symbol, .. } if LoadControlSignal::from_symbol_id(*symbol)
            == Some(LoadControlSignal::KillEmacs)
    )
}

fn should_log_load_form_error(eval: &super::eval::Context, err: &EvalError) -> bool {
    match err {
        EvalError::Signal { .. } => !is_kill_emacs_signal(err),
        EvalError::UncaughtThrow { tag, .. } => !eval.has_active_catch(tag),
    }
}

fn format_load_form_error(err: &EvalError) -> String {
    match err {
        EvalError::Signal {
            symbol,
            data,
            raw_data,
        } => {
            let payload = if let Some(raw) = raw_data {
                format_value_for_error(raw)
            } else if data.is_empty() {
                "nil".to_string()
            } else {
                let data_strs: Vec<String> = data.iter().map(format_value_for_error).collect();
                format!("({})", data_strs.join(" "))
            };
            format!("({} {})", resolve_sym(*symbol), payload)
        }
        other => format!("{other:?}"),
    }
}

fn log_streaming_load_form_error(
    eval: &super::eval::Context,
    file_name: &str,
    form_idx: usize,
    preview: String,
    err: &EvalError,
) {
    tracing::error!(
        "  !! {} FORM[{}] FAILED: {} => {}",
        file_name,
        form_idx,
        preview,
        format_load_form_error(err),
    );

    let bt_frames: Vec<_> = eval
        .specpdl
        .iter()
        .rev()
        .filter_map(|entry| match entry {
            super::eval::SpecBinding::Backtrace { function, args, .. } => Some((function, args)),
            _ => None,
        })
        .collect();
    if !bt_frames.is_empty() {
        // Bounds that keep the logged backtrace readable.
        const MAX_BACKTRACE_FRAMES: usize = 20; // frames before the rest is summarized
        const MAX_ARGS_PER_FRAME: usize = 4; // args shown per frame
        const MAX_ARG_CHARS: usize = 40; // chars per arg before truncation
        const ELLIPSIS: &str = "...";
        tracing::error!("  Lisp backtrace:");
        for (j, (function, frame_args)) in bt_frames.iter().enumerate() {
            let func_name = super::print::print_value(function);
            let args = eval.backtrace_args_values(frame_args);
            let args_str = args
                .iter()
                .take(MAX_ARGS_PER_FRAME)
                .map(|a| {
                    // A longer arg is shown as its first (MAX_ARG_CHARS - ELLIPSIS) chars
                    // plus ELLIPSIS, so the displayed width never exceeds MAX_ARG_CHARS.
                    // Count and truncate by CHARACTERS, never bytes: a printed arg can
                    // contain multi-byte UTF-8 (e.g. a bytecode/raw-byte string prints
                    // with `�`), so a byte slice would panic ("not a char boundary")
                    // when the cut lands inside a multi-byte char.
                    let s = super::print::print_value(a);
                    if s.chars().count() > MAX_ARG_CHARS {
                        let kept: String = s.chars().take(MAX_ARG_CHARS - ELLIPSIS.len()).collect();
                        format!("{kept}{ELLIPSIS}")
                    } else {
                        s
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            let ellipsis = if args.len() > MAX_ARGS_PER_FRAME {
                " ..."
            } else {
                ""
            };
            tracing::error!("    {j}: ({func_name} {args_str}{ellipsis})");
            if j >= MAX_BACKTRACE_FRAMES {
                tracing::error!("    ... ({} more frames)", bt_frames.len() - j - 1);
                break;
            }
        }
    }
}

const COMPILED_ELISP_FORM_PREVIEW: &str = "<compiled .elc form elided>";

fn is_compiled_elisp_path(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "elc")
}

fn load_form_log_preview(path: &Path, make_preview: impl FnOnce() -> String) -> String {
    if is_compiled_elisp_path(path) {
        COMPILED_ELISP_FORM_PREVIEW.to_string()
    } else {
        make_preview()
    }
}

fn read_error_for_load(path: &Path, e: super::value_reader::ReadError) -> EvalError {
    match e.kind {
        super::value_reader::ReadErrorKind::EndOfFile => EvalError::Signal {
            symbol: intern("end-of-file"),
            data: vec![],
            raw_data: None,
        },
        super::value_reader::ReadErrorKind::Error => EvalError::Signal {
            symbol: intern("error"),
            data: vec![Value::string(e.message)],
            raw_data: None,
        },
        super::value_reader::ReadErrorKind::InvalidReadSyntax => EvalError::Signal {
            symbol: intern("error"),
            data: vec![Value::string(format!(
                "Read error in {}: {} at position {}",
                path.display(),
                e.message,
                e.position
            ))],
            raw_data: None,
        },
        super::value_reader::ReadErrorKind::Signal => EvalError::Signal {
            symbol: intern(e.signal_symbol.as_deref().unwrap_or("error")),
            data: e.signal_data,
            raw_data: None,
        },
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const GENERATED_LOADDEFS_MARKER: &str = "Generated by the `loaddefs-generate' function.";
const TRANSIENT_RUNTIME_FEATURES: &[&str] = &[
    "cl-lib", "cl-macs", "cl-seq", "cl-extra", "gv", "icons", "pcase", "rx",
];

fn clear_transient_runtime_features(eval: &mut super::eval::Context) {
    for feature in TRANSIENT_RUNTIME_FEATURES {
        eval.remove_feature(feature);
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn eval_generated_form_args(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> Result<Vec<Value>, EvalError> {
    args.iter()
        .map(|value| eval_runtime_form(eval, *value))
        .collect()
}

fn eval_runtime_form(eval: &mut super::eval::Context, form: Value) -> Result<Value, EvalError> {
    eval.eval_sub(form).map_err(map_flow)
}

fn cached_form_requires_eager_replay(form: Value) -> bool {
    form.is_cons()
        && form
            .cons_car()
            .as_symbol_name()
            .is_some_and(|name| matches!(name, "eval-and-compile" | "eval-when-compile"))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn generated_defalias(eval: &mut super::eval::Context, args: &[Value]) -> Result<Value, EvalError> {
    if !(2..=3).contains(&args.len()) {
        return Err(EvalError::Signal {
            symbol: intern("wrong-number-of-arguments"),
            data: vec![Value::symbol("defalias"), Value::fixnum(args.len() as i64)],
            raw_data: None,
        });
    }
    let values = eval_generated_form_args(eval, args)?;
    let result = eval
        .defalias_value(values[0], values[1])
        .map_err(map_flow)?;
    if let Some(doc) = values.get(2).copied().filter(|value| !value.is_nil()) {
        super::builtins::builtin_put(
            eval,
            vec![values[0], Value::symbol("function-documentation"), doc],
        )
        .map_err(map_flow)?;
    }
    Ok(result)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn try_eval_generated_loaddefs_form(
    eval: &mut super::eval::Context,
    form: Value,
) -> Result<Option<Value>, EvalError> {
    let Some(items) = list_to_vec(&form) else {
        return Ok(None);
    };
    let Some(head_sym) = items.first().and_then(|v| v.as_symbol_name()) else {
        return Ok(None);
    };
    let tail = &items[1..];
    // Keep this table limited to core primitive replay.  GNU Lisp-owned
    // helpers from loaddefs (e.g. custom/obsolete metadata helpers) should
    // run through the already-loaded GNU Lisp runtime instead.
    match head_sym {
        "progn" => {
            let mut last = Value::NIL;
            for item in tail {
                last = eval_generated_loaddefs_form(eval, *item)?;
            }
            Ok(Some(last))
        }
        "autoload" => {
            let values = eval_generated_form_args(eval, tail)?;
            Ok(Some(
                super::autoload::builtin_autoload(eval, values).map_err(map_flow)?,
            ))
        }
        "put" | "function-put" => {
            let values = eval_generated_form_args(eval, tail)?;
            Ok(Some(
                super::builtins::builtin_put(eval, values).map_err(map_flow)?,
            ))
        }
        "defalias" => Ok(Some(generated_defalias(eval, tail)?)),
        "defvaralias" => {
            let values = eval_generated_form_args(eval, tail)?;
            Ok(Some(
                super::builtins::builtin_defvaralias(eval, values).map_err(map_flow)?,
            ))
        }
        _ => Ok(None),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn eval_generated_loaddefs_form(
    eval: &mut super::eval::Context,
    form: Value,
) -> Result<Value, EvalError> {
    if let Some(value) = try_eval_generated_loaddefs_form(eval, form)? {
        return Ok(value);
    }
    eval_runtime_form(eval, form)
}

fn has_load_suffix(name: &LispString) -> bool {
    let bytes = name.as_bytes();
    bytes.ends_with(b".el")
        || bytes.ends_with(b".elc")
        || bytes.ends_with(std::env::consts::DLL_SUFFIX.as_bytes())
}

fn append_load_suffix(base: &Path, suffix: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        let mut bytes = base.as_os_str().as_bytes().to_vec();
        bytes.extend_from_slice(suffix);
        PathBuf::from(std::ffi::OsString::from_vec(bytes))
    }

    #[cfg(not(unix))]
    {
        let suffix = std::str::from_utf8(suffix).expect("ASCII suffix");
        PathBuf::from(format!("{}{}", base.to_string_lossy(), suffix))
    }
}

fn source_suffixed_path(base: &Path) -> PathBuf {
    append_load_suffix(base, b".el")
}

fn compiled_suffixed_path(base: &Path) -> PathBuf {
    append_load_suffix(base, b".elc")
}

fn module_suffixed_path(base: &Path) -> PathBuf {
    append_load_suffix(base, std::env::consts::DLL_SUFFIX.as_bytes())
}

fn unsupported_compiled_suffixed_paths(base: &Path) -> [PathBuf; 1] {
    [append_load_suffix(base, b".elc.gz")]
}

fn is_module_path(path: &Path) -> bool {
    path.as_os_str()
        .as_encoded_bytes()
        .ends_with(std::env::consts::DLL_SUFFIX.as_bytes())
}

/// GNU Emacs tries dynamic modules before .elc and .el when modules are
/// supported.  NeoVM matches this by default.
/// Set NEOVM_PREFER_EL=1 to prefer .el source (for debugging).
fn prefer_el_only() -> bool {
    std::env::var("NEOVM_PREFER_EL").is_ok()
}

fn candidate_mtime(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

fn pick_suffixed(base: &Path, prefer_newer: bool) -> Option<PathBuf> {
    let module = module_suffixed_path(base);
    let el = source_suffixed_path(base);
    let elc = compiled_suffixed_path(base);
    let skip_elc = prefer_el_only();

    if prefer_newer && !skip_elc {
        let mut candidates = Vec::new();
        if module.exists() {
            candidates.push(module.clone());
        }
        if elc.exists() {
            candidates.push(elc.clone());
        }
        if el.exists() {
            candidates.push(el.clone());
        }
        return candidates
            .into_iter()
            .filter_map(|path| candidate_mtime(&path).map(|mtime| (mtime, path)))
            .max_by_key(|(mtime, _)| *mtime)
            .map(|(_, path)| path);
    }

    // GNU default with module support: try the module suffix first, then
    // .elc, then .el.
    if module.exists() {
        return Some(module);
    }
    if !skip_elc && elc.exists() {
        return Some(elc);
    }
    if el.exists() {
        return Some(el);
    }

    None
}

fn find_for_base(
    base: &Path,
    original_name: &LispString,
    no_suffix: bool,
    must_suffix: bool,
    prefer_newer: bool,
) -> Option<PathBuf> {
    if no_suffix || has_load_suffix(original_name) {
        if base.is_file() {
            return Some(base.to_path_buf());
        }
        return None;
    }

    if let Some(suffixed) = pick_suffixed(base, prefer_newer) {
        return Some(suffixed);
    }

    if !must_suffix && base.is_file() {
        return Some(base.to_path_buf());
    }

    // Surface unsupported compressed compiled artifacts explicitly.
    for compiled in unsupported_compiled_suffixed_paths(base) {
        if compiled.exists() {
            return Some(compiled);
        }
    }

    None
}

fn expand_tilde_path_buf(path: &LispString) -> PathBuf {
    #[cfg(unix)]
    {
        let bytes = path.as_bytes();
        if bytes == b"~" {
            if let Some(home) = std::env::var_os("HOME") {
                return PathBuf::from(home);
            }
        } else if bytes.starts_with(b"~/") {
            if let Some(home) = std::env::var_os("HOME") {
                let mut expanded = PathBuf::from(home);
                expanded.push(std::ffi::OsString::from_vec(bytes[2..].to_vec()));
                return expanded;
            }
        }

        return load_path_buf(path);
    }

    #[cfg(not(unix))]
    {
        PathBuf::from(expand_tilde(&load_runtime_string(path)))
    }
}

/// Search for a file in the load path.
#[tracing::instrument(level = "debug", ret)]
pub fn find_file_in_load_path(name: &str, load_path: &[LispString]) -> Option<PathBuf> {
    find_file_in_load_path_with_flags(name, load_path, false, false, false)
}

/// Search for a file in load-path with `load` optional suffix flags.
///
/// Behavior follows Emacs:
/// - `no_suffix`: load only the exact filename.
/// - `must_suffix`: require a suffixed file when FILE has no suffix.
/// - `prefer_newer`: ignore suffix order and choose the newest suffixed file.
/// - default: search each load-path directory in order, trying `.elc` before
///   `.el`, then bare names when suffixless loading is allowed.
pub fn find_file_in_load_path_with_flags(
    name: &str,
    load_path: &[LispString],
    no_suffix: bool,
    must_suffix: bool,
    prefer_newer: bool,
) -> Option<PathBuf> {
    let name = LispString::from_utf8(name);
    find_lisp_file_in_load_path_with_flags(&name, load_path, no_suffix, must_suffix, prefer_newer)
        .map(|found| load_path_buf(&found))
}

fn find_lisp_file_in_load_path_with_flags(
    name: &LispString,
    load_path: &[LispString],
    no_suffix: bool,
    must_suffix: bool,
    prefer_newer: bool,
) -> Option<LispString> {
    let path = expand_tilde_path_buf(name);
    if path.is_absolute() {
        return find_for_base(&path, name, no_suffix, must_suffix, prefer_newer)
            .map(|found| load_path_lisp_string(&found));
    }

    // Emacs searches load-path directory-by-directory; suffix preference
    // is evaluated within each directory.
    for dir in load_path {
        let full = expand_tilde_path_buf(dir).join(load_path_buf(name));
        if let Some(found) = find_for_base(&full, name, no_suffix, must_suffix, prefer_newer) {
            return Some(load_path_lisp_string(&found));
        }
    }

    None
}

/// Extract `load-path` from the evaluator's obarray as Lisp strings.
pub fn get_load_path(obarray: &super::symbol::Obarray) -> Vec<LispString> {
    let default_directory = obarray
        .symbol_value("default-directory")
        .and_then(|v| {
            v.is_string()
                .then(|| v.as_lisp_string().expect("checked string").clone())
        })
        .unwrap_or_else(|| LispString::from_unibyte(b".".to_vec()));

    let val = obarray
        .symbol_value("load-path")
        .cloned()
        .unwrap_or(Value::NIL);
    super::value::list_to_vec(&val)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| match v {
            v if v.is_nil() => Some(default_directory.clone()),
            _ if v.is_string() => v.as_lisp_string().cloned(),
            _ => None,
        })
        .collect()
}

pub(crate) enum LoadPlan {
    Return(Value),
    Load {
        requested: LispString,
        found: LispString,
    },
}

pub(crate) fn plan_load_in_state(
    obarray: &super::symbol::Obarray,
    file: Value,
    noerror: Option<Value>,
    nosuffix: Option<Value>,
    must_suffix: Option<Value>,
) -> Result<LoadPlan, Flow> {
    let file = match file.kind() {
        ValueKind::String => file.as_lisp_string().expect("checked string").clone(),
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), file],
            ));
        }
    };
    let file = super::fileio::substitute_in_file_name_lisp(&file);
    let noerror = noerror.is_some_and(|v| v.is_truthy());
    let nosuffix = nosuffix.is_some_and(|v| v.is_truthy());
    let must_suffix = must_suffix.is_some_and(|v| v.is_truthy());
    let prefer_newer = obarray
        .symbol_value("load-prefer-newer")
        .is_some_and(|v| v.is_truthy());

    let load_path = get_load_path(obarray);
    match find_lisp_file_in_load_path_with_flags(
        &file,
        &load_path,
        nosuffix,
        must_suffix,
        prefer_newer,
    ) {
        Some(found) => Ok(LoadPlan::Load {
            requested: file,
            found,
        }),
        None => {
            if noerror {
                Ok(LoadPlan::Return(Value::NIL))
            } else {
                Err(cannot_open_load_file_signal(&file))
            }
        }
    }
}

pub(crate) fn resolve_autoload_load_path_in_state(
    obarray: &super::symbol::Obarray,
    file: &LispString,
) -> Result<PathBuf, Flow> {
    match plan_load_in_state(
        obarray,
        Value::heap_string(file.clone()),
        None,
        None,
        Some(Value::T),
    )? {
        LoadPlan::Load { found, .. } => Ok(load_path_buf(&found)),
        LoadPlan::Return(_) => unreachable!("autoload load planning used noerror=nil"),
    }
}

pub(crate) fn builtin_load_in_vm_runtime(
    shared: &mut super::eval::Context,
    args: &[Value],
) -> Result<Value, Flow> {
    if args.is_empty() {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("load"), Value::fixnum(0)],
        ));
    }

    match plan_load_in_state(
        &shared.obarray,
        args[0],
        args.get(1).copied(),
        args.get(3).copied(),
        args.get(4).copied(),
    )? {
        LoadPlan::Return(value) => Ok(value),
        LoadPlan::Load { requested, found } => {
            let extra_roots = args.to_vec();
            let noerror = args.get(1).is_some_and(|v| v.is_truthy());
            let nomessage = args.get(2).is_some_and(|v| v.is_truthy());
            let path = load_path_buf(&found);
            let root_scope = shared.save_specpdl_roots();
            for root in &extra_roots {
                shared.push_specpdl_root(*root);
            }
            let result = load_file_with_requested_and_found_flags(
                shared, &path, &requested, &found, noerror, nomessage,
            )
            .map_err(|e| match e {
                EvalError::Signal {
                    symbol,
                    data,
                    raw_data,
                } => Flow::Signal(Box::new(crate::emacs_core::error::SignalData {
                    symbol,
                    data,
                    raw_data,
                    suppress_signal_hook: false,
                    selected_resume: None,
                    search_complete: false,
                })),
                EvalError::UncaughtThrow { tag, value } => Flow::Throw { tag, value },
            });
            shared.restore_specpdl_roots(root_scope);
            result
        }
    }
}

pub(crate) const BOOTSTRAP_LOAD_PATH_SUBDIRS: &[&str] = &[
    "",
    "vc",
    "use-package",
    "url",
    "textmodes",
    "progmodes",
    "play",
    "org",
    "nxml",
    "net",
    "mh-e",
    "mail",
    "leim",
    "language",
    "international",
    "image",
    "gnus",
    "eshell",
    "erc",
    "emulation",
    "emacs-lisp",
    "cedet",
    "calendar",
    "calc",
    "obsolete",
];

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn strip_utf8_bom(source: &str) -> &str {
    source.strip_prefix('\u{feff}').unwrap_or(source)
}

fn strip_utf8_bom_bytes(source: &[u8]) -> &[u8] {
    source.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(source)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn strip_reader_prefix(source: &str) -> (&str, bool) {
    let without_bom = strip_utf8_bom(source);
    if !without_bom.starts_with("#!") {
        return (without_bom, false);
    }

    match without_bom.find('\n') {
        Some(index) => (&without_bom[index + 1..], false),
        None => ("", true),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn lexical_binding_enabled_in_file_local_cookie_line(line: &str) -> bool {
    matches!(
        lexical_binding_cookie_in_file_local_cookie_line(line),
        LexicalBindingCookie::Lexical
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LexicalBindingCookie {
    None,
    Dynamic,
    Lexical,
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn lexical_binding_cookie_in_file_local_cookie_line(line: &str) -> LexicalBindingCookie {
    let Some(start) = line.find("-*-") else {
        return LexicalBindingCookie::None;
    };
    let rest = &line[start + 3..];
    let Some(end_rel) = rest.find("-*-") else {
        return LexicalBindingCookie::None;
    };
    let cookie = &rest[..end_rel];

    for entry in cookie.split(';') {
        let Some((name, value)) = entry.split_once(':') else {
            continue;
        };
        if name.trim() == "lexical-binding" {
            return if value.trim() == "t" {
                LexicalBindingCookie::Lexical
            } else {
                LexicalBindingCookie::Dynamic
            };
        }
    }
    LexicalBindingCookie::None
}

fn trim_cookie_ascii(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| *byte != b' ' && *byte != b'\t')
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| *byte != b' ' && *byte != b'\t')
        .map(|index| index + 1)
        .unwrap_or(start);
    &bytes[start..end]
}

fn lexical_binding_cookie_in_file_local_cookie_line_bytes(line: &[u8]) -> LexicalBindingCookie {
    let Some(start) = line.windows(3).position(|window| window == b"-*-") else {
        return LexicalBindingCookie::None;
    };
    let rest = &line[start + 3..];
    let Some(end_rel) = rest.windows(3).position(|window| window == b"-*-") else {
        return LexicalBindingCookie::None;
    };
    let cookie = &rest[..end_rel];

    for entry in cookie.split(|byte| *byte == b';') {
        let Some(colon) = entry.iter().position(|byte| *byte == b':') else {
            continue;
        };
        let name = trim_cookie_ascii(&entry[..colon]);
        let value = trim_cookie_ascii(&entry[colon + 1..]);
        if name == b"lexical-binding" {
            return if value == b"t" {
                LexicalBindingCookie::Lexical
            } else {
                LexicalBindingCookie::Dynamic
            };
        }
    }
    LexicalBindingCookie::None
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn lexical_binding_cookie_for_source(source: &str) -> LexicalBindingCookie {
    let mut lines = strip_utf8_bom(source).lines();
    let first_line = lines.next();
    if let Some(cookie) = first_line.map(lexical_binding_cookie_in_file_local_cookie_line)
        && cookie != LexicalBindingCookie::None
    {
        return cookie;
    }

    if first_line.is_some_and(|line| line.starts_with("#!")) {
        return lines
            .next()
            .map(lexical_binding_cookie_in_file_local_cookie_line)
            .unwrap_or(LexicalBindingCookie::None);
    }

    LexicalBindingCookie::None
}

pub(crate) fn lexical_binding_cookie_for_lisp_source(source: &LispString) -> LexicalBindingCookie {
    let mut lines = strip_utf8_bom_bytes(source.as_bytes()).split(|byte| *byte == b'\n');
    let Some(first_line) = lines.next() else {
        return LexicalBindingCookie::None;
    };

    if first_line.starts_with(b"#!") {
        return lines
            .next()
            .map(lexical_binding_cookie_in_file_local_cookie_line_bytes)
            .unwrap_or(LexicalBindingCookie::None);
    }

    lexical_binding_cookie_in_file_local_cookie_line_bytes(first_line)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn lexical_binding_enabled_for_source(source: &str) -> bool {
    matches!(
        lexical_binding_cookie_for_source(source),
        LexicalBindingCookie::Lexical
    )
}

fn default_toplevel_lexical_binding(eval: &super::eval::Context) -> bool {
    crate::emacs_core::eval::default_toplevel_value_in_state(
        &eval.obarray,
        eval.specpdl.as_slice(),
        Some(&eval.buffers.buffer_defaults),
        intern("lexical-binding"),
    )
    .is_some_and(|value| value.is_truthy())
}

fn lexical_binding_from_cookie(
    eval: &mut super::eval::Context,
    cookie: LexicalBindingCookie,
    from: Option<Value>,
) -> Result<bool, EvalError> {
    match cookie {
        LexicalBindingCookie::Lexical => Ok(true),
        LexicalBindingCookie::Dynamic => Ok(false),
        LexicalBindingCookie::None => {
            let default = default_toplevel_lexical_binding(eval);
            let Some(from) = from else {
                return Ok(default);
            };
            let hook = eval
                .visible_variable_value_or_nil("internal--get-default-lexical-binding-function");
            if hook.is_nil() {
                return Ok(default);
            }

            let roots = eval.save_specpdl_roots();
            eval.push_specpdl_root(hook);
            eval.push_specpdl_root(from);
            let result = eval.apply1(hook, from).map_err(map_flow);
            eval.restore_specpdl_roots(roots);
            result.map(|value| value.is_truthy())
        }
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn source_lexical_binding_for_load(
    eval: &mut super::eval::Context,
    source: &str,
    from: Option<Value>,
) -> Result<bool, EvalError> {
    lexical_binding_from_cookie(eval, lexical_binding_cookie_for_source(source), from)
}

pub(crate) fn source_lexical_binding_for_lisp_source(
    eval: &mut super::eval::Context,
    source: &LispString,
    from: Option<Value>,
) -> Result<bool, EvalError> {
    lexical_binding_from_cookie(eval, lexical_binding_cookie_for_lisp_source(source), from)
}

fn strip_local_variables_comment_prefix(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix(";;")?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

fn source_read_symbol_shorthands_text(source: &str) -> Option<String> {
    let mut local_variables_seen = false;
    let mut collecting = false;
    let mut value = String::new();

    for raw_line in source.lines() {
        let Some(line) = strip_local_variables_comment_prefix(raw_line) else {
            continue;
        };
        let trimmed = line.trim();
        if trimmed == "Local Variables:" {
            local_variables_seen = true;
            collecting = false;
            value.clear();
            continue;
        }
        if !local_variables_seen {
            continue;
        }
        if trimmed == "End:" {
            break;
        }
        if collecting {
            value.push_str(line);
            value.push('\n');
            continue;
        }
        if let Some(rest) = line.trim_start().strip_prefix("read-symbol-shorthands:") {
            collecting = true;
            value.push_str(rest.trim_start());
            value.push('\n');
        }
    }

    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn read_symbol_shorthands_value_text(
    text: &str,
    source_multibyte: bool,
    obarray: &super::symbol::Obarray,
) -> Result<Option<ReadSymbolShorthands>, EvalError> {
    let Some((value, _)) =
        super::value_reader::read_one_with_source_multibyte(text, source_multibyte, 0, obarray)
            .map_err(|err| EvalError::Signal {
                symbol: intern("invalid-read-syntax"),
                data: vec![Value::string(err.message)],
                raw_data: None,
            })?
    else {
        return Ok(None);
    };
    Ok(ReadSymbolShorthands::from_lisp_value(value))
}

fn is_unsupported_compiled_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    // .elc is now supported; only block compressed .elc.gz
    name.ends_with(".elc.gz")
}

/// Check if eager macro expansion is available.
///
/// GNU `readevalloop` only checks whether
/// `internal-macroexpand-for-load` is fbound, and skips eager expansion for
/// `.elc` files.  The Lisp helper itself handles cycles and expansion
/// failures by returning the original form.
#[tracing::instrument(level = "debug", skip(eval))]
pub(crate) fn get_eager_macroexpand_fn(eval: &super::eval::Context) -> Option<Value> {
    // Respect the Elisp `macroexp--pending-eager-loads` variable.
    // When it starts with `skip`, eager expansion is suppressed (mirrors
    // the check in `internal-macroexpand-for-load` in macroexp.el).
    if let Some(val) = eval.obarray().symbol_value("macroexp--pending-eager-loads") {
        if val.is_cons() {
            if val.cons_car().is_symbol_named("skip") {
                return None;
            }
        }
    }
    let f = eval
        .obarray()
        .symbol_function("internal-macroexpand-for-load")?;
    // Guard: if the function cell was set to nil (e.g. via fset), treat as unavailable
    if f.is_nil() {
        return None;
    }
    // GNU `readevalloop` keeps the symbol Qinternal_macroexpand_for_load and
    // resolves the current function cell on each call. Mirror that here
    // instead of caching the callable object itself across the whole load.
    Some(Value::symbol("internal-macroexpand-for-load"))
}

/// Port of real Emacs's `readevalloop_eager_expand_eval` from lread.c.
///
/// Algorithm (matching real Emacs lread.c lines 2013-2032):
/// 1. `val = macroexpand(val, nil)` — one-level expand, mutating `val`
/// 2. If `val` is `(progn ...)`, recurse into each subform
/// 3. Otherwise `eval(macroexpand(val, t))` — full expand the ALREADY
///    one-level-expanded `val`, then eval
///
/// This ensures all macros (including `pcase` inside function bodies) are
/// expanded at load time, preventing combinatorial re-expansion at runtime.
///
/// **Cycle/failure recovery**: NeoVM loads .el source files, not .elc
/// compiled files. This means eager expansion encounters circular require
/// chains (e.g. cl-lib ↔ cl-generic ↔ seq) that real Emacs avoids because
/// .elc files don't need eager expansion. When expansion fails (cycle
/// detection, missing macros, etc.), we fall back to evaluating the form
/// without eager expansion — matching the behavior of loading .elc files.
#[tracing::instrument(level = "debug", skip(eval, form_value, macroexpand_fn, sink))]
pub(crate) fn eager_expand_toplevel_forms(
    eval: &mut super::eval::Context,
    form_value: Value,
    macroexpand_fn: Value,
    sink: &mut impl FnMut(&mut super::eval::Context, Value, Value, bool) -> Result<Value, EvalError>,
) -> Result<Value, EvalError> {
    let original_form = form_value;
    let mutation_epoch_before = eval.macro_expansion_mutation_epoch();
    // Step 1: one-level expand — val = (internal-macroexpand-for-load val nil)
    // Note: real Emacs mutates `val` here; we shadow it.
    let step1_start = std::time::Instant::now();
    let step1_roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(form_value);
    eval.push_specpdl_root(macroexpand_fn);
    // `internal-macroexpand-for-load` is an internal loader helper.
    // Its failures are handled here and its frames are not part of the
    // user-facing loaded form surface, so avoid paying full backtrace
    // bookkeeping on every eager expansion call.
    let val = eval.apply2(macroexpand_fn, form_value, Value::NIL).ok();
    eval.restore_specpdl_roots(step1_roots);
    eval.note_eager_macro_perf_step1(step1_start.elapsed());
    let val = match val {
        Some(v) => v,
        None => {
            // Eager expansion failed (cycle detection, missing macro, etc.).
            // Fall back to evaluating the original form without expansion.
            // This matches .elc behavior where forms are already compiled.
            tracing::debug!("eager_expand step1 failed, falling back to plain eval");
            let roots = eval.save_specpdl_roots();
            eval.push_specpdl_root(form_value);
            let result = sink(eval, original_form, form_value, false);
            eval.restore_specpdl_roots(roots);
            return result;
        }
    };

    // Step 2: if result is (progn ...), recurse into subforms.
    // Root `val` during iteration: the recursive `eager_expand_eval`
    // call triggers evaluation + GC, which could free val's cons cells.
    if val.is_cons() {
        let car = val.cons_car();
        let cdr = val.cons_cdr();
        if car.is_symbol_named("progn") {
            let roots = eval.save_specpdl_roots();
            eval.push_specpdl_root(val);
            let result = (|| -> Result<Value, EvalError> {
                let mut result = Value::NIL;
                let mut tail = cdr;
                while tail.is_cons() {
                    let sub_form = tail.cons_car();
                    tail = tail.cons_cdr();
                    result = eager_expand_toplevel_forms(eval, sub_form, macroexpand_fn, sink)?;
                }
                Ok(result)
            })();
            eval.restore_specpdl_roots(roots);
            return result;
        }
    }

    // Step 3+4: deep expand then eval —
    // GNU lread.c:2030: val = eval_sub(calln(Qmacroexpand, val, Qt));
    // where Qmacroexpand = Qinternal_macroexpand_for_load (set at line 2184).
    // Calling internal-macroexpand-for-load(val, t) with full-p=t triggers
    // macroexpand--all-toplevel (deep/recursive expansion via macroexpand-all).
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(val);
    eval.push_specpdl_root(macroexpand_fn);
    eval.push_specpdl_root(original_form);
    let t3 = std::time::Instant::now();
    // Call internal-macroexpand-for-load(val, t) — full-p=t means deep expand
    let expanded = match eval.apply2(macroexpand_fn, val, Value::T) {
        Ok(v) => v,
        Err(e) => {
            // Full expansion failed; use the one-level-expanded form.
            let form_str = super::print::print_value(&val);
            let form_preview: String = form_str.chars().take(200).collect();
            tracing::debug!("eager_expand step3 failed: {e:?} form={form_preview}");
            val
        }
    };
    let d3 = t3.elapsed();
    eval.note_eager_macro_perf_step3(d3);
    if eval.macro_perf_enabled() && d3.as_millis() > 200 {
        let head = if val.is_cons() {
            val.cons_car().as_symbol_name().unwrap_or("<non-symbol>")
        } else {
            "<atom>"
        };
        let form_str = super::print::print_value(&val);
        let form_preview: String = form_str.chars().take(200).collect();
        tracing::warn!(
            "eager_expand step3 (full-expand) took {d3:.2?} head={head} form={form_preview}"
        );
    }
    let requires_eager_replay = eval.macro_expansion_mutation_epoch() != mutation_epoch_before
        || cached_form_requires_eager_replay(original_form)
        || cached_form_requires_eager_replay(val)
        || cached_form_requires_eager_replay(expanded);
    eval.push_specpdl_root(expanded);
    let result = sink(eval, original_form, expanded, requires_eager_replay);
    eval.restore_specpdl_roots(roots);
    result
}

#[tracing::instrument(level = "debug", skip(eval, form_value, macroexpand_fn))]
pub(crate) fn eager_expand_eval(
    eval: &mut super::eval::Context,
    form_value: Value,
    macroexpand_fn: Value,
) -> Result<Value, EvalError> {
    eager_expand_toplevel_forms(
        eval,
        form_value,
        macroexpand_fn,
        &mut |ctx, _original, expanded, _requires_eager_replay| {
            let roots = ctx.save_specpdl_roots();
            ctx.push_specpdl_root(expanded);
            let t4 = std::time::Instant::now();
            let value = ctx.eval_value(&expanded).map_err(map_flow);
            let d4 = t4.elapsed();
            ctx.note_eager_macro_perf_step4(d4);
            if ctx.macro_perf_enabled() && d4.as_millis() > 200 {
                tracing::warn!("eager_expand step4 (eval) took {d4:.2?}");
            }
            ctx.restore_specpdl_roots(roots);
            value
        },
    )
}

/// Shared context save/restore for file loading.
///
/// Saves and restores: lexical-binding, lexenv, load-file-name,
/// load-true-file-name, current-load-list, temp roots.
/// Sets lexical-binding from the file cookie and load-bound filename metadata.
/// The `body` closure runs with the new context and its result is returned
/// after context restoration.
fn with_load_context<F>(
    eval: &mut super::eval::Context,
    hist_file_name: &LispString,
    found: &LispString,
    lexical_binding: bool,
    body: F,
) -> Result<Value, EvalError>
where
    F: FnOnce(&mut super::eval::Context) -> Result<Value, EvalError>,
{
    let old_macro_cache_disabled = eval.macro_cache_disabled;

    let specpdl_count = eval.specpdl.len();
    // GNU Fload first specbinds `lexical-binding' to nil, then assigns the
    // file's cookie/default value before entering readevalloop. The specpdl
    // binding is what preserves a caller's dynamic `(let ((lexical-binding
    // nil)) ...)' across autoload-triggered loads.
    eval.specbind(intern("lexical-binding"), Value::NIL);
    eval.set_runtime_binding_by_id(intern("lexical-binding"), Value::bool_val(lexical_binding));

    // Mirrors GNU readevalloop (lread.c:2220-2222):
    //   specbind(Qinternal_interpreter_environment,
    //            NILP(lex_bound) ? Qnil : list1(Qt));
    // Use the specpdl for lexenv save/restore, matching GNU exactly.
    // This ensures all modifications to self.lexenv during file loading
    // are properly unwound by unbind_to, even if individual let forms leak.
    {
        use super::eval::SpecBinding;
        eval.specpdl.push(SpecBinding::LexicalEnv {
            old_lexenv: eval.lexenv,
        });
    }
    if lexical_binding {
        eval.lexenv = Value::list(vec![Value::T]);
    } else {
        eval.lexenv = Value::NIL;
    }

    let roots = eval.save_specpdl_roots();

    let load_file_value = Value::heap_string(hist_file_name.clone());
    eval.push_specpdl_root(load_file_value);
    let load_true_file_value = Value::heap_string(found.clone());
    eval.push_specpdl_root(load_true_file_value);
    let current_load_list = Value::cons(load_file_value, Value::NIL);
    eval.push_specpdl_root(current_load_list);
    // GNU Fload specbinds these (`lread.c`) so assignments inside the
    // loaded file affect only the dynamic load context and unwind at load
    // exit. This matters during pdump: the dumped top-level value must be the
    // pre-load default, not the dynamic loadup.el filename.
    eval.specbind(intern("load-file-name"), load_file_value);
    eval.specbind(intern("load-true-file-name"), load_true_file_value);
    eval.specbind(intern("current-load-list"), current_load_list);
    // GNU eager load walks the current function cells directly and does not
    // keep a separate runtime macro-expansion cache. Disable the NeoVM cache
    // across file loads so exact GC does not retain or traverse stale
    // load-local macroexpansion trees. Drop-guarded (house RAII pattern) so
    // a panic contained at a module/JIT boundary inside the load restores
    // the previous state too — an imperative restore would be skipped and
    // latch the cache off for the rest of the session.
    struct MacroCacheDisableGuard<'a> {
        eval: &'a mut super::eval::Context,
        old: bool,
    }
    impl Drop for MacroCacheDisableGuard<'_> {
        fn drop(&mut self) {
            self.eval.macro_cache_disabled = self.old;
        }
    }
    let cache_guard = MacroCacheDisableGuard {
        eval: &mut *eval,
        old: old_macro_cache_disabled,
    };
    cache_guard.eval.macro_cache_disabled = true;

    let result = body(&mut *cache_guard.eval);

    drop(cache_guard);

    // Restore lexenv via specpdl unbind_to, matching GNU's
    // readevalloop cleanup. This pops the LexicalEnv entry we
    // pushed above, along with lexical-binding/load-file-name/
    // load-true-file-name/current-load-list dynamic bindings,
    // restoring their pre-load values.
    eval.unbind_to(specpdl_count);
    eval.restore_specpdl_roots(roots);

    result
}

/// GNU-style streaming read-eval loop using the Value-native reader.
///
/// Reads one form at a time from `content`, optionally macro-expands it via
/// `macroexpand_fn`, evaluates it, then advances to the next form. No
/// parse-all-first, no compilation cache, no macro expansion cache.
///
/// This matches the structure of `readevalloop` in GNU Emacs `lread.c`.
fn streaming_readevalloop(
    eval: &mut super::eval::Context,
    path: &Path,
    hist_file_name: &LispString,
    content: &str,
    source_multibyte: bool,
    shorthands: Option<&ReadSymbolShorthands>,
    macroexpand_fn: Option<Value>,
) -> Result<Value, EvalError> {
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let load_specpdl_base = eval.specpdl.len();

    let mut pos = 0;
    let mut form_idx = 0;

    loop {
        debug_assert_eq!(
            eval.specpdl.len(),
            load_specpdl_base,
            "streaming_readevalloop leaked specpdl entries before {file_name} form {form_idx}",
        );
        let read_result = super::value_reader::read_one_with_source_multibyte_and_shorthands(
            content,
            source_multibyte,
            pos,
            &eval.obarray,
            shorthands,
        )
        .map_err(|e| read_error_for_load(path, e))?;

        let Some((form, next_pos)) = read_result else {
            break; // EOF
        };
        eval.obarray_mut().materialize_read_symbols(form);

        // Log a preview of the form source text.
        let form_start = pos;
        pos = next_pos;

        if tracing::enabled!(tracing::Level::DEBUG) {
            let preview = load_form_log_preview(path, || {
                content[form_start..next_pos].chars().take(160).collect()
            });
            tracing::debug!(
                "{} FORM[{}/streaming]: {}",
                file_name,
                form_idx,
                preview.replace('\n', " ")
            );
        }

        // Root the form value so it survives any GC triggered during
        // macro-expansion or evaluation.
        let eval_roots = eval.save_specpdl_roots();
        eval.push_specpdl_root(form);
        let eval_result = if let Some(mexp) = macroexpand_fn {
            eval.push_specpdl_root(mexp);
            // GNU-style eager expand: one level, recurse for progn,
            // full expand + eval.
            streaming_readevalloop_eager_expand_eval(eval, form, mexp)
        } else {
            eval.eval_sub(form).map_err(map_flow)
        };
        eval.restore_specpdl_roots(eval_roots);

        // Report real load errors with human-readable detail. GNU loadup
        // deliberately exits through `kill-emacs` after dumping, so keep that
        // nonreturn path out of failure diagnostics.
        if let Err(ref e) = eval_result
            && should_log_load_form_error(eval, e)
        {
            let preview = load_form_log_preview(path, || {
                content[form_start..next_pos].chars().take(120).collect()
            });
            log_streaming_load_form_error(eval, &file_name, form_idx, preview, e);
        }
        eval_result?;

        debug_assert_eq!(
            eval.specpdl.len(),
            load_specpdl_base,
            "streaming_readevalloop leaked specpdl entries after {file_name} form {form_idx}",
        );
        form_idx += 1;
    }

    // GNU `readevalloop` builds load-history before unwinding the load
    // context so `current-load-list` still names the file being loaded.
    build_load_history(eval, hist_file_name, true);
    Ok(Value::T)
}

fn streaming_readevalloop_lisp_source(
    eval: &mut super::eval::Context,
    path: &Path,
    hist_file_name: &LispString,
    content: &LispString,
    shorthands: Option<&ReadSymbolShorthands>,
    macroexpand_fn: Option<Value>,
) -> Result<Value, EvalError> {
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let read_source = super::value_reader::LispReadSource::new(content);
    let load_specpdl_base = eval.specpdl.len();

    let mut pos = 0;
    let mut form_idx = 0;

    loop {
        debug_assert_eq!(
            eval.specpdl.len(),
            load_specpdl_base,
            "streaming_readevalloop_lisp_source leaked specpdl entries before {file_name} form {form_idx}",
        );
        let read_result = read_source
            .read_one_with_shorthands(pos, &eval.obarray, shorthands)
            .map_err(|e| read_error_for_load(path, e))?;

        let Some((form, next_pos)) = read_result else {
            break;
        };
        eval.obarray_mut().materialize_read_symbols(form);

        let form_start = pos;
        pos = next_pos;

        if tracing::enabled!(tracing::Level::DEBUG) {
            let preview = load_form_log_preview(path, || {
                read_source
                    .storage_slice_range(form_start, next_pos)
                    .chars()
                    .take(160)
                    .collect()
            });
            tracing::debug!(
                "{} FORM[{}]: {}",
                file_name,
                form_idx,
                preview.replace('\n', " ")
            );
        }

        let eval_roots = eval.save_specpdl_roots();
        eval.push_specpdl_root(form);
        let eval_result = if let Some(mexp) = macroexpand_fn {
            eval.push_specpdl_root(mexp);
            streaming_readevalloop_eager_expand_eval(eval, form, mexp)
        } else {
            eval.eval_sub(form).map_err(map_flow)
        };
        eval.restore_specpdl_roots(eval_roots);

        if let Err(ref e) = eval_result
            && should_log_load_form_error(eval, e)
        {
            let preview = load_form_log_preview(path, || {
                read_source
                    .storage_slice_range(form_start, next_pos)
                    .chars()
                    .take(120)
                    .collect()
            });
            log_streaming_load_form_error(eval, &file_name, form_idx, preview, e);
        }
        eval_result?;

        debug_assert_eq!(
            eval.specpdl.len(),
            load_specpdl_base,
            "streaming_readevalloop_lisp_source leaked specpdl entries after {file_name} form {form_idx}",
        );
        form_idx += 1;
    }

    build_load_history(eval, hist_file_name, true);
    Ok(Value::T)
}

/// GNU-style eager macro expansion during streaming load.
///
/// Matches `readevalloop_eager_expand_eval` in lread.c:
/// 1. One-level macroexpand via `internal-macroexpand-for-load(form, nil)`
/// 2. If result is `(progn ...)`, iterate subforms (recurse for each)
/// 3. Otherwise, full macroexpand via `internal-macroexpand-for-load(form, t)`
///    then eval the result.
fn streaming_readevalloop_eager_expand_eval(
    eval: &mut super::eval::Context,
    form: Value,
    macroexpand: Value,
) -> Result<Value, EvalError> {
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(form);
    eval.push_specpdl_root(macroexpand);

    // Step 1: one-level expand (full_p = nil)
    let step1_start = std::time::Instant::now();
    let expanded = match eval.apply2(macroexpand, form, Value::NIL) {
        Ok(v) => v,
        Err(_) => {
            // Expansion failed (cycle detection, missing macro, etc.).
            // Fall back to evaluating the original form without expansion,
            // matching .elc behavior.
            eval.note_eager_macro_perf_step1(step1_start.elapsed());
            tracing::debug!("streaming eager_expand step1 failed, falling back to plain eval");
            let result = eval.eval_sub(form).map_err(map_flow);
            eval.restore_specpdl_roots(roots);
            return result;
        }
    };
    eval.note_eager_macro_perf_step1(step1_start.elapsed());

    // Root the expanded form so it survives GC during progn iteration.
    let expanded_roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(expanded);
    let result = streaming_readevalloop_eager_expand_eval_inner(eval, expanded, macroexpand);
    eval.restore_specpdl_roots(expanded_roots);
    eval.restore_specpdl_roots(roots);
    result
}

/// Inner helper for eager expansion: handles progn unwinding and full expansion.
fn streaming_readevalloop_eager_expand_eval_inner(
    eval: &mut super::eval::Context,
    expanded: Value,
    macroexpand: Value,
) -> Result<Value, EvalError> {
    // Step 2: if (progn ...), iterate subforms
    if expanded.is_cons() && expanded.cons_car().is_symbol_named("progn") {
        let mut cursor = expanded.cons_cdr();
        let mut last_val = Value::NIL;
        while cursor.is_cons() {
            let subform = cursor.cons_car();
            cursor = cursor.cons_cdr();
            // Root cursor across recursive expansion+eval (it's a cons tail
            // that could be collected if we don't protect it).
            let roots = eval.save_specpdl_roots();
            eval.push_specpdl_root(cursor);
            let result = streaming_readevalloop_eager_expand_eval(eval, subform, macroexpand);
            eval.restore_specpdl_roots(roots);
            last_val = result?;
        }
        return Ok(last_val);
    }

    // Step 3: full expand (full_p = t), then eval
    let step3_start = std::time::Instant::now();
    let fully_expanded = match eval.apply2(macroexpand, expanded, Value::T) {
        Ok(v) => v,
        Err(_) => {
            // Full expansion failed; use the one-level-expanded form.
            tracing::debug!("streaming eager_expand step3 failed, using one-level expansion");
            expanded
        }
    };
    eval.note_eager_macro_perf_step3(step3_start.elapsed());

    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(fully_expanded);
    let step4_start = std::time::Instant::now();
    let result = eval.eval_sub(fully_expanded).map_err(map_flow);
    eval.note_eager_macro_perf_step4(step4_start.elapsed());
    eval.restore_specpdl_roots(roots);
    result
}

/// Load and evaluate a file. Returns the last result.
pub fn load_file(eval: &mut super::eval::Context, path: &Path) -> Result<Value, EvalError> {
    load_file_with_flags(eval, path, false, false)
}

/// Load and evaluate a file with the caller-visible `load` flags.
pub fn load_file_with_flags(
    eval: &mut super::eval::Context,
    path: &Path,
    noerror: bool,
    nomessage: bool,
) -> Result<Value, EvalError> {
    let expanded = expand_tilde(&path.to_string_lossy());
    let path = std::path::Path::new(&expanded);
    tracing::info!("load {}", path.display());
    let requested = load_path_lisp_string(path);
    load_file_with_requested_and_found_flags(eval, path, &requested, &requested, noerror, nomessage)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn load_file_with_found_flags(
    eval: &mut super::eval::Context,
    path: &Path,
    found: &LispString,
    noerror: bool,
    nomessage: bool,
) -> Result<Value, EvalError> {
    load_file_with_requested_and_found_flags(eval, path, found, found, noerror, nomessage)
}

pub(crate) fn load_file_with_requested_and_found_flags(
    eval: &mut super::eval::Context,
    path: &Path,
    requested: &LispString,
    found: &LispString,
    noerror: bool,
    nomessage: bool,
) -> Result<Value, EvalError> {
    if is_unsupported_compiled_path(path) {
        return Err(EvalError::Signal {
            symbol: intern("error"),
            data: vec![Value::string(format!(
                "Loading compressed compiled Elisp artifacts (.elc.gz) is unsupported in neomacs: {}",
                path.display()
            ))],
            raw_data: None,
        });
    }

    // GNU Emacs only signals `Recursive load` once the same found filename is
    // already present four times in `Vloads_in_progress`, i.e. on the fifth
    // attempt. Matching that behavior matters because Lisp depends on the
    // textual `found` identity, not on canonicalized host paths.
    let load_count = eval
        .loads_in_progress
        .iter()
        .filter(|p| load_name_equal(p, found))
        .count();
    if load_count > 3 {
        let found_value = Value::heap_string(found.clone());
        let in_progress = Value::list(
            eval.loads_in_progress
                .iter()
                .cloned()
                .map(Value::heap_string)
                .collect(),
        );
        return Err(EvalError::Signal {
            symbol: intern("error"),
            data: vec![
                Value::string("Recursive load"),
                Value::cons(found_value, in_progress),
            ],
            raw_data: None,
        });
    }
    // Both pieces of load bookkeeping ride the specpdl, mirroring GNU
    // lread.c Fload (`record_unwind_protect (record_load_unwind, ...)` +
    // `specbind (Qload_in_progress, Qt)`), so every unwind restores them:
    // the `unbind_to` below on both normal and `Err(Flow)` exits, and the
    // panic-containment boundary unwinds when a contained panic skips this
    // frame entirely. Imperative restore code here would be skipped by such
    // a panic, wedging `load-in-progress` at t and leaking a spurious
    // "Recursive load" entry.
    let spec_entry = eval.specpdl.len();
    eval.specpdl
        .push(super::eval::SpecBinding::LoadsInProgress {
            len: eval.loads_in_progress.len(),
        });
    eval.loads_in_progress.push(found.clone());
    eval.specbind(intern("load-in-progress"), Value::T);

    let result = stacker::maybe_grow(128 * 1024, 2 * 1024 * 1024, || {
        load_file_body(eval, path, requested, found, noerror, nomessage)
    });

    eval.unbind_to(spec_entry);
    result
}

fn load_file_body(
    eval: &mut super::eval::Context,
    path: &Path,
    requested: &LispString,
    found: &LispString,
    noerror: bool,
    nomessage: bool,
) -> Result<Value, EvalError> {
    let is_elc = path.extension().and_then(|e| e.to_str()) == Some("elc");
    let hist_file_name = load_hist_file_name(eval, requested, found);

    if is_module_path(path) {
        return crate::emacs_core::dynamic_module::load_module(eval, path.to_path_buf())
            .map_err(crate::emacs_core::error::map_flow);
    }

    if !is_elc
        && let load_source_file_function =
            eval.visible_variable_value_or_nil("load-source-file-function")
        && !load_source_file_function.is_nil()
    {
        let full_name = Value::heap_string(found.clone());
        return eval
            .apply(
                load_source_file_function,
                vec![
                    full_name,
                    Value::heap_string(hist_file_name.clone()),
                    Value::bool_val(noerror),
                    Value::bool_val(nomessage),
                ],
            )
            .map_err(crate::emacs_core::error::map_flow);
    }

    // Read raw bytes and decode (with Emacs-extended UTF-8 for .el,
    // or header-skipping for .elc).
    let raw_bytes = std::fs::read(path).map_err(|e| EvalError::Signal {
        symbol: intern("file-error"),
        data: vec![Value::string(format!(
            "Cannot read file: {}: {}",
            path.display(),
            e
        ))],
        raw_data: None,
    })?;

    // For .elc: skip the ;ELC magic header and detect lexical-binding from raw bytes.
    // For .el: decode Emacs-extended UTF-8.
    // GNU lread.c readevalloop order: specbind lexenv [with_load_context] ->
    // readevalloop -> unbind_to -> do-after-load-evaluation.
    //
    // .elc reads via the &str reader. .el decodes straight to a faithful
    // Emacs-bytes LispString and reads via the LispString reader, so source
    // characters — including non-Unicode literals — keep their real codes and
    // never round-trip through the in-Unicode storage-string form (issue #131).
    let result = if is_elc {
        let content = skip_elc_header(&raw_bytes);
        let lexical_binding = elc_has_lexical_binding(&raw_bytes);
        with_load_context(eval, &hist_file_name, found, lexical_binding, |eval| {
            streaming_readevalloop(eval, path, &hist_file_name, &content, false, None, None)
        })
    } else {
        // GNU `Fload` (`src/lread.c`) lets the coding system swallow a leading
        // UTF-8 BOM (U+FEFF); NeoVM's reader does not, so strip it from the raw
        // bytes before decoding (otherwise the reader reads it as a one-character
        // symbol and signals `void-variable`).
        let src_bytes = raw_bytes
            .strip_prefix(&[0xEF, 0xBB, 0xBF])
            .unwrap_or(raw_bytes.as_slice());
        let content = decode_emacs_utf8_source_lisp(src_bytes);
        let lexical_binding = source_lexical_binding_for_lisp_source(
            eval,
            &content,
            Some(Value::heap_string(found.clone())),
        )?;
        let shorthands = match source_read_symbol_shorthands_text(
            &crate::emacs_core::emacs_char::to_utf8_lossy(content.as_bytes()),
        ) {
            Some(text) => {
                read_symbol_shorthands_value_text(&text, content.is_multibyte(), &eval.obarray)?
            }
            None => None,
        };
        with_load_context(eval, &hist_file_name, found, lexical_binding, |eval| {
            let macroexpand_fn = get_eager_macroexpand_fn(eval);
            streaming_readevalloop_lisp_source(
                eval,
                path,
                &hist_file_name,
                &content,
                shorthands.as_ref(),
                macroexpand_fn,
            )
        })
    };
    // GNU lread.c:1533-1541: `build_load_history` runs inside
    // `readevalloop`, before `unbind_to`, while the after-load hooks
    // run after the load context is unwound. Keep the latter order so
    // callbacks see the caller's restored lexenv.
    if result.is_ok() {
        run_after_load_evaluation(eval, &hist_file_name);
    }

    result
}

pub(crate) fn eval_lisp_source_file_in_context(
    eval: &mut super::eval::Context,
    found: &LispString,
    content: &LispString,
) -> Result<Value, EvalError> {
    let macroexpand_fn = get_eager_macroexpand_fn(eval);
    let path = load_path_buf(found);
    // Issue #131: this text is scanned only for the ASCII `Local Variables:` /
    // `read-symbol-shorthands:` header; a lossy UTF-8 rendering keeps real
    // Unicode (incl. PUA) while dropping the buggy storage-string sentinels.
    // The actual forms are evaluated from the original `content` LispString via
    // the byte-faithful reader below.
    let source_text = crate::emacs_core::emacs_char::to_utf8_lossy(content.as_bytes());
    let shorthands_text = source_read_symbol_shorthands_text(&source_text);
    let shorthands = match shorthands_text {
        Some(text) => {
            read_symbol_shorthands_value_text(&text, content.is_multibyte(), &eval.obarray)?
        }
        None => None,
    };
    streaming_readevalloop_lisp_source(
        eval,
        &path,
        found,
        content,
        shorthands.as_ref(),
        macroexpand_fn,
    )
}

/// Skip the `;ELC` magic header in a byte-compiled Elisp file.
/// Returns the remaining content as a string.
fn skip_elc_header(raw_bytes: &[u8]) -> String {
    // .elc files start with ";ELC" magic bytes (0x3B 0x45 0x4C 0x43)
    // followed by version bytes (typically 0x1C 0x00 0x00 0x00 for Emacs 28+).
    // Then comment lines starting with ";;".
    //
    // We need to skip all bytes up to the first non-comment line.
    //
    // GNU Emacs `.elc` files mix ASCII source (defvar, defun, etc.) with
    // unibyte bytecode strings inside `#[...]` byte-code-function literals.
    // The bytecode strings contain raw bytes 0x00-0xFF where bytes >= 0x80
    // are NOT valid UTF-8 starts (e.g., 0xC0 0x87 = `constant 0; return`).
    //
    // We CANNOT use `decode_emacs_utf8` here because it replaces non-UTF-8
    // bytes with U+FFFD or escapes, corrupting the bytecode.  Instead, use
    // Latin-1 encoding: each raw byte 0-255 becomes the Unicode code point
    // with the same value, encoded as UTF-8 in the resulting Rust String.
    // This preserves all 256 byte values losslessly, and `string_value_to_bytes`
    // (which truncates each char to u8) recovers the original bytes exactly.
    let content: String = raw_bytes.iter().map(|&b| b as char).collect();
    let mut start = 0;

    // Skip bytes until we find the first line that doesn't start with ';' or
    // is not a special header byte. The magic is ";ELC" + 4 version bytes.
    let bytes = content.as_bytes();

    // First, skip the 8-byte magic header if present
    if bytes.starts_with(b";ELC") && bytes.len() >= 8 {
        start = 8;
        // Skip any additional non-printable/non-newline header bytes
        while start < bytes.len() && bytes[start] != b'\n' && bytes[start] != b';' {
            start += 1;
        }
    }

    // Now skip comment lines
    while start < bytes.len() {
        if bytes[start] == b'\n' {
            start += 1;
            continue;
        }
        if bytes[start] == b';' {
            // Skip to end of line
            while start < bytes.len() && bytes[start] != b'\n' {
                start += 1;
            }
            continue;
        }
        break;
    }

    content[start..].to_string()
}

/// Check if an .elc file has lexical-binding enabled in its header.
fn elc_has_lexical_binding(raw_bytes: &[u8]) -> bool {
    // Look for "lexical-binding: t" in the first few lines (header area)
    let preview = std::str::from_utf8(&raw_bytes[..raw_bytes.len().min(1024)]).unwrap_or("");
    preview.contains("lexical-binding: t")
}

fn build_load_history(eval: &mut super::eval::Context, filename: &LispString, entire: bool) {
    let path_str = load_display_string(filename);
    tracing::debug!("build_load_history: {}", path_str);
    let roots = eval.save_specpdl_roots();
    let current_load_list = eval.visible_variable_value_or_nil("current-load-list");
    eval.push_specpdl_root(current_load_list);
    let history = eval
        .obarray()
        .symbol_value("load-history")
        .cloned()
        .unwrap_or(Value::NIL);
    eval.push_specpdl_root(history);
    let filtered_history = if entire {
        filter_load_history_without_filename(eval, history, filename)
    } else {
        history
    };
    eval.push_specpdl_root(filtered_history);
    let entry = reverse_copy_rooted_list(eval, current_load_list);
    eval.push_specpdl_root(entry);
    let updated_history = if entire {
        Value::cons(entry, filtered_history)
    } else {
        filtered_history
    };
    eval.push_specpdl_root(updated_history);
    eval.set_variable("load-history", updated_history);
    if entire {
        eval.set_variable("current-load-list", Value::T);
    }
    eval.restore_specpdl_roots(roots);
}

fn reverse_copy_rooted_list(eval: &mut super::eval::Context, list: Value) -> Value {
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(list);
    let mut tail = list;
    let mut reversed = Value::NIL;
    while tail.is_cons() {
        let iter_roots = eval.save_specpdl_roots();
        eval.push_specpdl_root(reversed);
        reversed = Value::cons(tail.cons_car(), reversed);
        eval.restore_specpdl_roots(iter_roots);
        tail = tail.cons_cdr();
    }
    eval.restore_specpdl_roots(roots);
    reversed
}

fn filter_load_history_without_filename(
    eval: &mut super::eval::Context,
    history: Value,
    filename: &LispString,
) -> Value {
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(history);
    let mut tail = history;
    let mut filtered_reversed = Value::NIL;
    while tail.is_cons() {
        let existing = tail.cons_car();
        let keep = if existing.is_cons() {
            existing
                .cons_car()
                .as_lisp_string()
                .is_none_or(|loaded| !load_name_equal(loaded, filename))
        } else {
            true
        };
        if keep {
            let iter_roots = eval.save_specpdl_roots();
            eval.push_specpdl_root(filtered_reversed);
            filtered_reversed = Value::cons(existing, filtered_reversed);
            eval.restore_specpdl_roots(iter_roots);
        }
        tail = tail.cons_cdr();
    }
    eval.push_specpdl_root(filtered_reversed);
    let filtered = reverse_copy_rooted_list(eval, filtered_reversed);
    eval.restore_specpdl_roots(roots);
    filtered
}

fn run_after_load_evaluation(eval: &mut super::eval::Context, path_lisp: &LispString) {
    let path_str = load_display_string(path_lisp);
    let roots = eval.save_specpdl_roots();
    // GNU Emacs lread.c:1540-1541: after loading a file, call
    // (do-after-load-evaluation FILENAME) to run eval-after-load hooks.
    let dale_id = super::intern::intern("do-after-load-evaluation");
    let is_fboundp = eval
        .obarray()
        .symbol_function_id(dale_id)
        .is_some_and(|f| !f.is_nil());
    if is_fboundp {
        let abs_path = Value::heap_string(path_lisp.clone());
        eval.push_specpdl_root(abs_path);
        if let Err(e) = eval.apply1(Value::symbol(dale_id), abs_path) {
            let err_msg = match &e {
                super::error::Flow::Signal(sig) => {
                    let sym = super::intern::resolve_sym(sig.symbol);
                    let data: Vec<String> =
                        sig.data.iter().map(|v| format_value_for_error(v)).collect();
                    format!("({} {})", sym, data.join(" "))
                }
                other => format!("{other:?}"),
            };
            tracing::warn!(
                "do-after-load-evaluation error for {}: {}",
                path_str,
                err_msg
            );
        }
    }
    eval.restore_specpdl_roots(roots);
}

/// Register bootstrap variables owned by the file-loading subsystem.
pub fn register_bootstrap_vars(obarray: &mut super::symbol::Obarray) {
    obarray.set_symbol_value("after-load-alist", Value::NIL);
    obarray.make_special("after-load-alist");
    obarray.set_symbol_value("macroexp--dynvars", Value::NIL);
    obarray.make_special("macroexp--dynvars");
}

/// Create an Context with the full Emacs bootstrap loaded (like GNU
/// Emacs's dumped state).  Mirrors the loadup.el boot sequence.
fn normalized_bootstrap_features(extra_features: &[&str]) -> Vec<String> {
    let mut features = extra_features
        .iter()
        .map(|feature| (*feature).to_string())
        .filter(|feature| !feature.is_empty())
        .collect::<Vec<_>>();
    features.sort_unstable();
    features.dedup();
    features
}

// Bump when bootstrap image semantics change in ways an older dump cannot
// represent correctly. V16 invalidates older caches because category-table
// ownership moved from a parallel manager into dumped Lisp objects. V18
// refreshes caches after the GNU-matching `cl-lib` runtime surface fix. V19
// stops serializing dynamic load context bindings as top-level values. V20
// marks keyboard-translate-table special like GNU's DEFVAR_KBOARD.
// V21 keeps Neomacs' GUI terminal library out of the dumped batch surface.
// V22 stops redirecting bootstrap `loaddefs` loads to `ldefs-boot` when the
// real generated loaddefs file exists, matching GNU loadup.el's fallback path.
// V23 stops advertising GNU X/GTK startup features for Neomacs' `neo` backend.
const BOOTSTRAP_IMAGE_SCHEMA_VERSION: u32 = 23;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadupDumpMode {
    Pbootstrap,
    Pdump,
}

impl LoadupDumpMode {
    pub const fn as_gnu_string(self) -> &'static str {
        match self {
            Self::Pbootstrap => "pbootstrap",
            Self::Pdump => "pdump",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadupStartupSurface {
    pub command_line_args: Vec<String>,
    pub noninteractive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeImageRole {
    Bootstrap,
    Final,
}

impl RuntimeImageRole {
    pub const fn canonical_image_stem(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap-neomacs",
            Self::Final => "neomacs",
        }
    }

    pub const fn image_file_name(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap-neomacs.pdump",
            Self::Final => "neomacs.pdump",
        }
    }

    pub fn fingerprinted_image_file_name(self) -> String {
        format!(
            "{}-{}.pdump",
            self.canonical_image_stem(),
            super::pdump::fingerprint_hex()
        )
    }
}
const RUNTIME_ROOT_ENV: &str = "NEOMACS_RUNTIME_ROOT";
const BOOTSTRAP_CACHE_DIR_ENV: &str = "NEOVM_BOOTSTRAP_CACHE_DIR";

fn compile_time_project_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().expect("project root").to_path_buf()
}

fn is_runtime_root(path: &Path) -> bool {
    path.join("lisp").is_dir() && path.join("etc").is_dir()
}

fn runtime_project_root() -> PathBuf {
    if let Ok(root) = std::env::var(RUNTIME_ROOT_ENV) {
        let path = PathBuf::from(root);
        if is_runtime_root(&path) {
            return path;
        }
        tracing::warn!(
            "{RUNTIME_ROOT_ENV}={} does not contain lisp/ and etc/; falling back",
            path.display()
        );
    }

    let compile_root = compile_time_project_root();
    if is_runtime_root(&compile_root) {
        return compile_root;
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(prefix) = exe.parent().and_then(Path::parent)
    {
        for candidate in [
            prefix.join("share/neomacs"),
            prefix.join("Resources/neomacs"),
        ] {
            if is_runtime_root(&candidate) {
                return candidate;
            }
        }
    }

    panic!(
        "Neomacs runtime root not found. Set {RUNTIME_ROOT_ENV} to a directory containing lisp/ and etc/."
    );
}

/// Directory holding the charset `.map` files (`JISX0201.map`, ...). Resolved at
/// RUNTIME under the install data dir (`<runtime_root>/etc/charsets`) -- the
/// neomacs equivalent of GNU's `charset-map-path`
/// (`(expand-file-name "charsets" data-directory)`), reusing the same
/// `runtime_project_root()` resolver that backs `data-directory` and locates
/// `lisp/`. MUST NOT use `env!("CARGO_MANIFEST_DIR")`: that compile-time path is
/// the build machine's source tree, absent in an installed release, so charset
/// maps would silently fail to load (decode-char -> nil -> "Invalid code(s)").
pub(crate) fn charset_map_directory() -> PathBuf {
    runtime_project_root().join("etc").join("charsets")
}

fn bootstrap_cache_dir(runtime_root: &Path) -> PathBuf {
    if let Ok(dir) = std::env::var(BOOTSTRAP_CACHE_DIR_ENV)
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }

    let compile_root = compile_time_project_root();
    if runtime_root == compile_root {
        return compile_root.join("target");
    }

    if let Ok(dir) = std::env::var("XDG_CACHE_HOME")
        && !dir.is_empty()
    {
        return PathBuf::from(dir).join("neomacs");
    }

    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home).join(".cache/neomacs");
    }

    std::env::temp_dir().join("neomacs")
}

fn should_hash_bootstrap_source_file(path: &Path) -> bool {
    matches!(path.extension().and_then(OsStr::to_str), Some("el" | "elc"))
}

fn collect_bootstrap_source_files(path: &Path, out: &mut Vec<PathBuf>) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };

    if metadata.is_file() {
        if should_hash_bootstrap_source_file(path) {
            out.push(path.to_path_buf());
        }
        return;
    }

    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    let mut children = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect::<Vec<_>>();
    children.sort();
    for child in children {
        collect_bootstrap_source_files(&child, out);
    }
}

fn bootstrap_source_fingerprint(runtime_root: &Path) -> String {
    let mut files = Vec::new();
    collect_bootstrap_source_files(&runtime_root.join("lisp"), &mut files);
    files.sort();

    let mut hasher = Sha256::new();
    hasher.update(b"neomacs-bootstrap-source-fingerprint-v2\0");
    hasher.update(b"rust-executable\0");
    match std::env::current_exe().and_then(|path| {
        let metadata = fs::metadata(&path)?;
        Ok((path, metadata))
    }) {
        Ok((path, metadata)) => {
            hasher.update([1]);
            hasher.update(path.as_os_str().as_encoded_bytes());
            hasher.update([0]);
            hasher.update(metadata.len().to_le_bytes());
            hasher.update([0]);
            if let Ok(modified) = metadata.modified()
                && let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH)
            {
                hasher.update(duration.as_secs().to_le_bytes());
                hasher.update(duration.subsec_nanos().to_le_bytes());
            }
        }
        Err(err) => {
            hasher.update([0]);
            hasher.update(err.to_string().as_bytes());
        }
    }
    hasher.update([0xff]);
    for path in files {
        let rel = path.strip_prefix(runtime_root).unwrap_or(&path);
        hasher.update(rel.as_os_str().as_encoded_bytes());
        hasher.update([0]);
        match fs::read(&path) {
            Ok(bytes) => {
                hasher.update([1]);
                hasher.update(bytes);
            }
            Err(err) => {
                hasher.update([0]);
                hasher.update(err.to_string().as_bytes());
            }
        }
        hasher.update([0xff]);
    }

    let digest = hasher.finalize();
    digest[..16]
        .iter()
        .fold(String::with_capacity(32), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn bootstrap_dump_path(runtime_root: &Path, extra_features: &[&str]) -> PathBuf {
    let features = normalized_bootstrap_features(extra_features);
    let suffix = if features.is_empty() {
        String::new()
    } else {
        format!("-{}", features.join("-"))
    };
    let source_fingerprint = bootstrap_source_fingerprint(runtime_root);
    bootstrap_cache_dir(runtime_root).join(format!(
        "neovm-bootstrap-v{BOOTSTRAP_IMAGE_SCHEMA_VERSION}-{source_fingerprint}{suffix}.pdump"
    ))
}

fn runtime_image_stem_for_executable(executable: &Path, role: RuntimeImageRole) -> String {
    let file_name = executable
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(role.canonical_image_stem());
    file_name
        .strip_suffix(".exe")
        .unwrap_or(file_name)
        .to_string()
}

pub fn runtime_image_path_for_executable(executable: &Path, role: RuntimeImageRole) -> PathBuf {
    executable
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(
            "{}.pdump",
            runtime_image_stem_for_executable(executable, role)
        ))
}

pub fn fingerprinted_runtime_image_path_for_executable(
    executable: &Path,
    role: RuntimeImageRole,
) -> PathBuf {
    executable
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(role.fingerprinted_image_file_name())
}

pub fn default_runtime_image_path(role: RuntimeImageRole) -> PathBuf {
    let executable = std::env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok().or(Some(path)))
        .unwrap_or_else(|| PathBuf::from(role.image_file_name()));
    runtime_image_path_for_executable(&executable, role)
}

fn default_fingerprinted_runtime_image_path(role: RuntimeImageRole) -> PathBuf {
    let executable = std::env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok().or(Some(path)))
        .unwrap_or_else(|| PathBuf::from(role.image_file_name()));
    fingerprinted_runtime_image_path_for_executable(&executable, role)
}

fn runtime_image_candidate_paths_for_executable(
    executable: &Path,
    role: RuntimeImageRole,
) -> Vec<PathBuf> {
    let primary = runtime_image_path_for_executable(executable, role);
    let fingerprinted = fingerprinted_runtime_image_path_for_executable(executable, role);
    if primary == fingerprinted {
        vec![primary]
    } else {
        vec![primary, fingerprinted]
    }
}

fn bootstrap_dump_lock_path(dump_path: &Path) -> PathBuf {
    let file_name = dump_path
        .file_name()
        .expect("bootstrap dump path should have file name");
    let mut lock_name = file_name.to_os_string();
    lock_name.push(".lock");
    dump_path.with_file_name(lock_name)
}

#[derive(Debug)]
enum BootstrapCacheLockError {
    Busy(String),
    Other(String),
}

impl std::fmt::Display for BootstrapCacheLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy(message) | Self::Other(message) => f.write_str(message),
        }
    }
}

fn open_bootstrap_lock_file(lock_path: &Path) -> Result<std::fs::File, String> {
    if let Some(parent) = lock_path.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent).map_err(|err| {
            format!(
                "bootstrap cache lock: failed creating {}: {err}",
                parent.display()
            )
        })?;
    }

    std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|err| {
            format!(
                "bootstrap cache lock: failed opening {}: {err}",
                lock_path.display()
            )
        })
}

struct BootstrapCacheWriteLock {
    file: std::fs::File,
}

impl BootstrapCacheWriteLock {
    fn acquire(lock_path: &Path) -> Result<Self, BootstrapCacheLockError> {
        let file = open_bootstrap_lock_file(lock_path).map_err(BootstrapCacheLockError::Other)?;

        match fs4::FileExt::try_lock(&file) {
            Ok(()) => Ok(Self { file }),
            Err(e) => match e {
                fs4::TryLockError::WouldBlock => Err(BootstrapCacheLockError::Busy(format!(
                    "bootstrap cache lock busy at {}",
                    lock_path.display()
                ))),
                fs4::TryLockError::Error(e) => Err(BootstrapCacheLockError::Other(format!(
                    "bootstrap cache lock: failed locking {}: {}",
                    lock_path.display(),
                    e
                ))),
            },
        }
    }
}

struct BootstrapCacheReadLock {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    file: std::fs::File,
}

impl BootstrapCacheReadLock {
    fn wait(lock_path: &Path) -> Result<Self, String> {
        let file = open_bootstrap_lock_file(lock_path)?;
        file.lock_shared().map_err(|e| {
            format!(
                "bootstrap cache lock: failed waiting on {}: {}",
                lock_path.display(),
                e
            )
        })?;
        Ok(Self { file })
    }
}

impl Drop for BootstrapCacheWriteLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn ensure_startup_compat_variables(eval: &mut super::eval::Context, project_root: &Path) {
    let etc_dir = lisp_directory_name_from_host_path(&project_root.join("etc"));
    let source_dir = lisp_directory_name_from_host_path(project_root);
    let temporary_file_directory = lisp_directory_name_from_host_path(&std::env::temp_dir());
    let path_separator = if cfg!(windows) { ";" } else { ":" };
    let process_environment = {
        #[cfg_attr(not(windows), allow(unused_mut))]
        let mut entries: Vec<(String, String)> = std::env::vars().collect();
        // Mirror GNU `w32.c init_environment`: guarantee HOME is set on Windows,
        // where the OS environment provides APPDATA/USERPROFILE but typically not
        // HOME. Without it `getenv "HOME"` is nil and `~` never expands, so e.g.
        // `directory-files "~"` fails fatally during startup. GNU defaults HOME to
        // the roaming AppData folder (CSIDL_APPDATA == %APPDATA%), else "C:/".
        #[cfg(windows)]
        if !entries.iter().any(|(k, _)| k.eq_ignore_ascii_case("HOME")) {
            let home = std::env::var("APPDATA")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| "C:/".to_string());
            entries.push(("HOME".to_string(), home));
        }
        Value::list(
            entries
                .into_iter()
                .map(|(name, value)| Value::string(format!("{name}={value}")))
                .collect::<Vec<_>>(),
        )
    };
    {
        let obarray = eval.obarray_mut();
        obarray.make_special("initial-environment");
        obarray.make_special("process-environment");
    }
    eval.set_variable("initial-environment", process_environment.clone());
    eval.set_variable("process-environment", process_environment.clone());
    let system_name = super::builtins_extra::builtin_system_name(vec![])
        .unwrap_or_else(|_| Value::string("localhost"));
    let user_full_name = super::builtins_extra::initial_user_full_name_value();
    let user_login_name = super::builtins_extra::builtin_user_login_name(vec![])
        .unwrap_or_else(|_| Value::string("unknown"));
    let user_real_login_name = super::builtins_extra::builtin_user_real_login_name(vec![])
        .unwrap_or_else(|_| Value::string("unknown"));
    let system_configuration = super::builtins_extra::system_configuration_value();
    let system_configuration_options = super::builtins_extra::system_configuration_options_value();
    let system_configuration_features =
        super::builtins_extra::system_configuration_features_value();
    let operating_system_release = super::builtins_extra::operating_system_release_value();
    let defaults = [
        (
            "emacs-copyright",
            Value::string("Copyright (C) 2026 Free Software Foundation, Inc."),
        ),
        ("data-directory", Value::unibyte_string(etc_dir.clone())),
        ("doc-directory", Value::unibyte_string(etc_dir)),
        (
            "source-directory",
            Value::unibyte_string(source_dir.clone()),
        ),
        ("installation-directory", Value::unibyte_string(source_dir)),
        ("exec-directory", Value::NIL),
        // configure-info-directory is initialized to GNU's PATH_INFO default
        // in eval.rs (obarray init); this guarded default only applies when
        // unset, so it stays nil here to avoid clobbering that value.
        ("configure-info-directory", Value::NIL),
        ("charset-map-path", Value::NIL),
        ("path-separator", Value::string(path_separator)),
        ("file-name-coding-system", Value::NIL),
        ("default-file-name-coding-system", Value::NIL),
        ("set-auto-coding-function", Value::NIL),
        ("after-insert-file-functions", Value::NIL),
        ("write-region-annotate-functions", Value::NIL),
        ("write-region-post-annotation-function", Value::NIL),
        ("write-region-annotations-so-far", Value::NIL),
        ("inhibit-file-name-handlers", Value::NIL),
        ("inhibit-file-name-operation", Value::NIL),
        (
            "temporary-file-directory",
            Value::string(temporary_file_directory),
        ),
        ("system-uses-terminfo", Value::T),
        ("create-lockfiles", Value::T),
        ("auto-save-list-file-name", Value::NIL),
        ("auto-save-list-file-prefix", Value::NIL),
        ("auto-save-visited-file-name", Value::NIL),
        ("auto-save-include-big-deletions", Value::NIL),
        ("shared-game-score-directory", Value::NIL),
        ("invocation-name", Value::NIL),
        ("invocation-directory", Value::NIL),
        ("system-messages-locale", Value::NIL),
        ("system-time-locale", Value::NIL),
        ("before-init-time", Value::NIL),
        ("after-init-time", Value::NIL),
        ("system-configuration", system_configuration),
        ("system-configuration-options", system_configuration_options),
        (
            "system-configuration-features",
            system_configuration_features,
        ),
        ("system-name", system_name),
        ("user-full-name", user_full_name),
        ("user-login-name", user_login_name),
        ("user-real-login-name", user_real_login_name),
        ("operating-system-release", operating_system_release),
        ("delayed-warnings-list", Value::NIL),
        ("default-text-properties", Value::NIL),
        ("char-property-alias-alist", Value::NIL),
        ("inhibit-point-motion-hooks", Value::T),
        (
            "text-property-default-nonsticky",
            Value::list(vec![
                Value::cons(Value::symbol("syntax-table"), Value::T),
                Value::cons(Value::symbol("display"), Value::T),
            ]),
        ),
    ];
    for (name, value) in defaults {
        if eval.obarray().symbol_value(name).is_none() {
            eval.set_variable(name, value);
        }
    }
    for name in [
        "data-directory",
        "doc-directory",
        "exec-directory",
        "configure-info-directory",
        "shared-game-score-directory",
        "delayed-warnings-list",
    ] {
        eval.obarray.make_special(name);
    }
    {
        let obarray = eval.obarray_mut();
        // GNU data.c installs these with DEFVAR_LISP, then assigns the
        // fixnum limit and calls make_symbol_constant.  Reassert this here so
        // cached bootstrap dumps from older builds regain the C bootstrap
        // symbol flags after load.
        obarray.set_symbol_value("most-positive-fixnum", Value::fixnum(i64::MAX >> 2));
        obarray.make_special("most-positive-fixnum");
        obarray.set_constant("most-positive-fixnum");
        obarray.set_symbol_value("most-negative-fixnum", Value::fixnum(-(i64::MAX >> 2) - 1));
        obarray.make_special("most-negative-fixnum");
        obarray.set_constant("most-negative-fixnum");
    }
    crate::emacs_core::xfaces::ensure_startup_compat_variables(eval);
}

fn value_symbol_name(value: Value) -> Option<String> {
    if let Some(name) = value.as_symbol_name() {
        return Some(name.to_owned());
    }
    value_quoted_symbol_name(value)
}

fn value_quoted_symbol_name(value: Value) -> Option<String> {
    if let Some(name) = value.as_symbol_name() {
        return Some(name.to_owned());
    }
    // Handle (quote sym) form: a two-element list where the first element is
    // the symbol `quote` and the second is the symbol to extract.
    let items = list_to_vec(&value)?;
    if items.len() == 2 {
        if items[0].is_symbol_named("quote") {
            return items[1].as_symbol_name().map(|s| s.to_owned());
        }
    }
    None
}

fn value_runtime_literal(value: Value) -> Option<Value> {
    // Values from the reader are already runtime values, except (quote X)
    // which evaluates to X (the quoted datum).
    if !value.is_cons() {
        return Some(value);
    }
    // (quote X) -> X
    value_quoted_symbol_name(value).map(|name| Value::symbol(&name))
}

#[derive(Default)]
struct LoaddefsSurfaceState {
    names: std::collections::BTreeSet<String>,
    autoload_args: Vec<Vec<Value>>,
    property_forms: Vec<Value>,
    property_keys: std::collections::BTreeSet<(String, String)>,
    keymap_defvar_forms: Vec<Value>,
    keymap_forms: Vec<LoaddefsKeymapReplayForm>,
    symbol_names: std::collections::BTreeSet<String>,
}

struct LoaddefsKeymapReplayForm {
    target: String,
    form: Value,
}

#[derive(Default)]
struct SourceFileSurfaceState {
    function_names: std::collections::BTreeSet<String>,
    variable_names: std::collections::BTreeSet<String>,
    face_names: std::collections::BTreeSet<String>,
    property_keys: std::collections::BTreeSet<(String, String)>,
    features: std::collections::BTreeSet<String>,
    symbol_names: std::collections::BTreeSet<String>,
}

fn source_surface_insert_property(
    state: &mut SourceFileSurfaceState,
    name: impl Into<String>,
    prop: impl Into<String>,
) {
    state.property_keys.insert((name.into(), prop.into()));
}

fn collect_source_surface(form: Value, state: &mut SourceFileSurfaceState) {
    let Some(items) = list_to_vec(&form) else {
        return;
    };
    let Some(head) = items.first() else {
        return;
    };
    let Some(head_name) = head.as_symbol_name() else {
        return;
    };

    match head_name {
        "progn" | "eval-and-compile" => {
            for item in items.iter().skip(1) {
                collect_source_surface(*item, state);
            }
        }
        "defun" | "defmacro" | "defsubst" | "define-inline" => {
            if let Some(name) = items.get(1).and_then(|v| value_symbol_name(*v)) {
                state.function_names.insert(name);
            }
        }
        "defalias" => {
            if let Some(name) = items.get(1).and_then(|v| value_quoted_symbol_name(*v)) {
                state.function_names.insert(name);
            }
        }
        "defvar" | "defconst" | "defcustom" => {
            if let Some(name) = items.get(1).and_then(|v| value_symbol_name(*v)) {
                state.variable_names.insert(name);
            }
        }
        "defface" => {
            if let Some(name) = items.get(1).and_then(|v| value_symbol_name(*v)) {
                state.variable_names.insert(name.clone());
                state.face_names.insert(name);
            }
        }
        "put" | "function-put" | "define-symbol-prop" => {
            if let Some(name) = items.get(1).and_then(|v| value_quoted_symbol_name(*v))
                && let Some(prop) = items.get(2).and_then(|v| value_symbol_name(*v))
            {
                source_surface_insert_property(state, name, prop);
            }
        }
        "def-edebug-elem-spec" => {
            if let Some(name) = items.get(1).and_then(|v| value_quoted_symbol_name(*v)) {
                source_surface_insert_property(state, name, "edebug-form-spec");
            }
        }
        "provide" => {
            if let Some(feature) = items.get(1).and_then(|v| value_quoted_symbol_name(*v)) {
                state.features.insert(feature);
            }
        }
        "pcase-defmacro" => {
            if let Some(name) = items.get(1).and_then(|v| value_symbol_name(*v)) {
                let macroexpander = format!("{name}--pcase-macroexpander");
                state.function_names.insert(macroexpander.clone());
                source_surface_insert_property(state, &macroexpander, "edebug-form-spec");
                source_surface_insert_property(state, name, "pcase-macroexpander");
            }
        }
        "define-icon" => {
            if let Some(name) = items.get(1).and_then(|v| value_symbol_name(*v)) {
                source_surface_insert_property(state, name, "icon--properties");
            }
        }
        _ => {}
    }
}

fn collect_value_symbol_names(
    value: Value,
    symbol_names: &mut std::collections::BTreeSet<String>,
    seen: &mut std::collections::BTreeSet<usize>,
) {
    match value.kind() {
        ValueKind::Nil => {
            symbol_names.insert("nil".to_string());
        }
        ValueKind::T => {
            symbol_names.insert("t".to_string());
        }
        ValueKind::Symbol(id) => {
            symbol_names.insert(resolve_sym(id).to_string());
        }
        ValueKind::Cons => {
            let key = value.bits();
            if !seen.insert(key) {
                return;
            }
            collect_value_symbol_names(value.cons_car(), symbol_names, seen);
            collect_value_symbol_names(value.cons_cdr(), symbol_names, seen);
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            let key = value.bits();
            if !seen.insert(key) {
                return;
            }
            if let Some(items) = value.as_vector_data() {
                for item in items.iter().copied() {
                    collect_value_symbol_names(item, symbol_names, seen);
                }
            }
        }
        ValueKind::Veclike(VecLikeType::Record | VecLikeType::Lambda | VecLikeType::Macro) => {
            let key = value.bits();
            if !seen.insert(key) {
                return;
            }
            if let Some(slots) = value.as_record_data().or_else(|| value.closure_slots()) {
                for slot in slots.iter().copied() {
                    collect_value_symbol_names(slot, symbol_names, seen);
                }
            }
        }
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let key = value.bits();
            if !seen.insert(key) {
                return;
            }
            if let Some(table) = value.as_hash_table() {
                for key_value in table.key_snapshots.values().copied() {
                    collect_value_symbol_names(key_value, symbol_names, seen);
                }
                for value in table.data.values().copied() {
                    collect_value_symbol_names(value, symbol_names, seen);
                }
            }
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            let key = value.bits();
            if !seen.insert(key) {
                return;
            }
            if let Some(bytecode) = value.get_bytecode_data() {
                collect_value_symbol_names(bytecode.arglist, symbol_names, seen);
                for constant in bytecode.constants.iter().copied() {
                    collect_value_symbol_names(constant, symbol_names, seen);
                }
                if let Some(env) = bytecode.env {
                    collect_value_symbol_names(env, symbol_names, seen);
                }
                if let Some(doc_form) = bytecode.doc_form {
                    collect_value_symbol_names(doc_form, symbol_names, seen);
                }
                if let Some(interactive) = bytecode.interactive {
                    collect_value_symbol_names(interactive, symbol_names, seen);
                }
                for slot in bytecode.extra_slots.iter().copied() {
                    collect_value_symbol_names(slot, symbol_names, seen);
                }
            }
        }
        ValueKind::Veclike(VecLikeType::CharTable) => {
            let key = value.bits();
            if !seen.insert(key) {
                return;
            }
            if let Some(slots) = value.char_table_external_slots() {
                for slot in slots {
                    collect_value_symbol_names(slot, symbol_names, seen);
                }
            }
        }
        ValueKind::Veclike(VecLikeType::SubCharTable) => {
            let key = value.bits();
            if !seen.insert(key) {
                return;
            }
            if let Some(table) = value.as_sub_char_table_obj() {
                for slot in table.contents.iter().copied() {
                    collect_value_symbol_names(slot, symbol_names, seen);
                }
            }
        }
        ValueKind::Veclike(VecLikeType::SymbolWithPos) => {
            if let Some(symbol) = value.as_symbol_with_pos_sym() {
                collect_value_symbol_names(symbol, symbol_names, seen);
            }
        }
        _ => {}
    }
}

fn collect_source_surface_from_paths(
    paths: &[PathBuf],
    error_context: &str,
) -> Result<SourceFileSurfaceState, EvalError> {
    let mut state = SourceFileSurfaceState::default();

    for path in paths {
        let bytes = fs::read(path).map_err(|err| EvalError::Signal {
            symbol: intern("error"),
            data: vec![Value::string(format!(
                "{error_context}: failed reading {}: {err}",
                path.display()
            ))],
            raw_data: None,
        })?;
        let source = decode_emacs_utf8_source_lisp(&bytes);
        let obarray = crate::emacs_core::symbol::Obarray::new();
        let forms = crate::emacs_core::value_reader::read_all_lisp_source(&source, &obarray)
            .map_err(|err| EvalError::Signal {
                symbol: intern("error"),
                data: vec![Value::string(format!(
                    "{error_context}: failed parsing {}: {err}",
                    path.display()
                ))],
                raw_data: None,
            })?;

        for form in forms {
            collect_value_symbol_names(
                form,
                &mut state.symbol_names,
                &mut std::collections::BTreeSet::new(),
            );
            collect_source_surface(form, &mut state);
        }
    }

    Ok(state)
}

fn collect_loaddefs_autoload_args(
    expr: Value,
    allowed_files: Option<&std::collections::BTreeSet<String>>,
    allowed_names: Option<&std::collections::BTreeSet<String>>,
    state: &mut LoaddefsSurfaceState,
) {
    let Some(items) = list_to_vec(&expr) else {
        return;
    };
    let Some(head) = items.first() else {
        return;
    };
    if !head.is_symbol_named("autoload") {
        return;
    }

    let Some(name) = items.get(1).and_then(|v| value_quoted_symbol_name(*v)) else {
        return;
    };
    let Some(file_value) = items.get(2).and_then(|v| value_runtime_literal(*v)) else {
        return;
    };
    let ValueKind::String = file_value.kind() else {
        return;
    };
    let file = load_string_text(&file_value).expect("checked string");
    if let Some(files) = allowed_files
        && !files.contains(&file)
    {
        return;
    };
    if let Some(names) = allowed_names
        && !names.contains(&name)
    {
        return;
    }

    state.names.insert(name.clone());
    collect_value_symbol_names(
        expr,
        &mut state.symbol_names,
        &mut std::collections::BTreeSet::new(),
    );
    let mut args = vec![Value::symbol(&name), file_value];
    for item in items.iter().skip(3).take(3) {
        let Some(value) = value_runtime_literal(*item) else {
            return;
        };
        args.push(value);
    }
    state.autoload_args.push(args);
}

fn collect_loaddefs_property_forms(
    expr: Value,
    names: &std::collections::BTreeSet<String>,
    state: &mut LoaddefsSurfaceState,
) {
    let Some(items) = list_to_vec(&expr) else {
        return;
    };
    let Some(head) = items.first() else {
        return;
    };
    let Some(head_name) = head.as_symbol_name() else {
        return;
    };
    if head_name != "function-put" && head_name != "put" && head_name != "define-symbol-prop" {
        return;
    }
    let Some(name) = items.get(1).and_then(|v| value_quoted_symbol_name(*v)) else {
        return;
    };
    if names.contains(&name) {
        collect_value_symbol_names(
            expr,
            &mut state.symbol_names,
            &mut std::collections::BTreeSet::new(),
        );
        state.property_forms.push(expr);
        if let Some(prop) = items.get(2).and_then(|v| value_symbol_name(*v)) {
            state.property_keys.insert((name, prop));
        }
    }
}

fn loaddefs_keymap_replay_target(expr: Value) -> Option<String> {
    let items = list_to_vec(&expr)?;
    let head = items.first()?;
    match head.as_symbol_name()? {
        "define-key" | "keymap-set" => items.get(1).and_then(|value| value_symbol_name(*value)),
        "if" | "progn" | "unless" | "when" => items
            .iter()
            .skip(1)
            .find_map(|value| loaddefs_keymap_replay_target(*value)),
        _ => None,
    }
}

fn collect_loaddefs_keymap_forms(expr: Value, state: &mut LoaddefsSurfaceState) {
    let Some(target) = loaddefs_keymap_replay_target(expr) else {
        return;
    };
    collect_value_symbol_names(
        expr,
        &mut state.symbol_names,
        &mut std::collections::BTreeSet::new(),
    );
    state
        .keymap_forms
        .push(LoaddefsKeymapReplayForm { target, form: expr });
}

fn collect_loaddefs_keymap_defvar_form(expr: Value, state: &mut LoaddefsSurfaceState) {
    let Some(items) = list_to_vec(&expr) else {
        return;
    };
    let Some(head) = items.first() else {
        return;
    };
    if !head.is_symbol_named("defvar") {
        return;
    }
    let Some(name) = items.get(1).and_then(|value| value_symbol_name(*value)) else {
        return;
    };
    if !name.ends_with("-map") {
        return;
    }
    collect_value_symbol_names(
        expr,
        &mut state.symbol_names,
        &mut std::collections::BTreeSet::new(),
    );
    state.keymap_defvar_forms.push(expr);
}

fn collect_loaddefs_surface_from_paths(
    paths: &[PathBuf],
    allowed_files: Option<&std::collections::BTreeSet<String>>,
    allowed_names: Option<&std::collections::BTreeSet<String>>,
    error_context: &str,
) -> Result<LoaddefsSurfaceState, EvalError> {
    let mut state = LoaddefsSurfaceState::default();

    for path in paths {
        let bytes = fs::read(path).map_err(|err| EvalError::Signal {
            symbol: intern("error"),
            data: vec![Value::string(format!(
                "{error_context}: failed reading {}: {err}",
                path.display()
            ))],
            raw_data: None,
        })?;
        let source = decode_emacs_utf8_source_lisp(&bytes);
        let obarray = crate::emacs_core::symbol::Obarray::new();
        let forms = crate::emacs_core::value_reader::read_all_lisp_source(&source, &obarray)
            .map_err(|err| EvalError::Signal {
                symbol: intern("error"),
                data: vec![Value::string(format!(
                    "{error_context}: failed parsing {}: {err}",
                    path.display()
                ))],
                raw_data: None,
            })?;

        for form in &forms {
            collect_loaddefs_autoload_args(*form, allowed_files, allowed_names, &mut state);
            collect_loaddefs_keymap_defvar_form(*form, &mut state);
            collect_loaddefs_keymap_forms(*form, &mut state);
        }
        let property_names = allowed_names
            .cloned()
            .unwrap_or_else(|| state.names.clone());
        for form in &forms {
            collect_loaddefs_property_forms(*form, &property_names, &mut state);
        }
    }

    Ok(state)
}

fn compile_only_cl_loaddefs_state(project_root: &Path) -> Result<LoaddefsSurfaceState, EvalError> {
    collect_loaddefs_surface_from_paths(
        &[project_root.join("lisp/emacs-lisp/cl-loaddefs.el")],
        None,
        None,
        "bootstrap runtime cleanup",
    )
}

fn runtime_loaddefs_restore_state(project_root: &Path) -> Result<LoaddefsSurfaceState, EvalError> {
    let runtime_files = ["gv", "icons", "pcase", "rx"]
        .into_iter()
        .map(str::to_string)
        .collect::<std::collections::BTreeSet<_>>();
    collect_loaddefs_surface_from_paths(
        &[project_root.join("lisp/ldefs-boot.el")],
        Some(&runtime_files),
        None,
        "bootstrap runtime cleanup",
    )
}

fn loaded_source_paths(eval: &mut super::eval::Context) -> Vec<PathBuf> {
    {
        let history = eval
            .obarray()
            .symbol_value("load-history")
            .cloned()
            .unwrap_or(Value::NIL);
        let mut paths = std::collections::BTreeSet::new();

        for entry in list_to_vec(&history).unwrap_or_default() {
            if !entry.is_cons() {
                continue;
            };
            let Some(path) = entry.cons_car().as_lisp_string() else {
                continue;
            };
            let path = load_path_buf(path);
            if path.extension().is_some_and(|ext| ext == "el") {
                paths.insert(path);
                continue;
            }
            if path.extension().is_some_and(|ext| ext == "elc") {
                let source_path = path.with_extension("el");
                if source_path.exists() {
                    paths.insert(source_path);
                }
            }
        }

        paths.into_iter().collect()
    }
}

fn is_compile_only_loaddefs_provider(path: &Path) -> bool {
    matches!(
        path.file_stem().and_then(|stem| stem.to_str()),
        Some(
            "cl-loaddefs"
                | "cl-preloaded"
                | "cl-lib"
                | "cl-macs"
                | "cl-seq"
                | "cl-extra"
                | "gv"
                | "icons"
        )
    )
}

fn is_generated_loaddefs_provider(path: &Path) -> bool {
    matches!(
        path.file_stem().and_then(|stem| stem.to_str()),
        Some("loaddefs" | "ldefs-boot" | "theme-loaddefs")
    )
}

fn runtime_loaded_source_restore_state(
    eval: &mut super::eval::Context,
    project_root: &Path,
    allowed_names: &std::collections::BTreeSet<String>,
) -> Result<LoaddefsSurfaceState, EvalError> {
    let paths = loaded_source_paths(eval)
        .into_iter()
        .filter(|path| path.starts_with(project_root))
        .filter(|path| !is_compile_only_loaddefs_provider(path))
        .filter(|path| !is_generated_loaddefs_provider(path))
        .collect::<Vec<_>>();
    collect_loaddefs_surface_from_paths(
        &paths,
        None,
        Some(allowed_names),
        "bootstrap runtime cleanup",
    )
}

fn runtime_source_bootstrap_surface_state(
    project_root: &Path,
) -> Result<SourceFileSurfaceState, EvalError> {
    collect_source_surface_from_paths(
        &[
            project_root.join("lisp/emacs-lisp/icons.el"),
            project_root.join("lisp/emacs-lisp/pcase.el"),
            project_root.join("lisp/emacs-lisp/rx.el"),
        ],
        "bootstrap runtime cleanup",
    )
}

fn symbol_has_runtime_surface(eval: &super::eval::Context, id: super::intern::SymId) -> bool {
    id == super::intern::NIL_SYM_ID
        || id == super::intern::T_SYM_ID
        || eval.obarray().is_constant_id(id)
        || eval.obarray().is_special_id(id)
        || eval.obarray().boundp_id(id)
        || eval.obarray().symbol_function_id(id).is_some()
        || !eval.obarray().symbol_plist_id(id).is_nil()
        || eval.coding_systems.contains_runtime_symbol(id)
}

fn collect_runtime_referenced_symbol_names(
    eval: &super::eval::Context,
) -> std::collections::BTreeSet<String> {
    let mut symbol_names = std::collections::BTreeSet::new();
    let mut seen = std::collections::BTreeSet::new();

    for id in eval.obarray().global_member_ids().collect::<Vec<_>>() {
        if let Some(value) = eval.obarray().symbol_value_id(id).copied() {
            collect_value_symbol_names(value, &mut symbol_names, &mut seen);
        }
        if let Some(function) = eval.obarray().symbol_function_id(id) {
            collect_value_symbol_names(function, &mut symbol_names, &mut seen);
        }
        let plist = eval.obarray().symbol_plist_id(id);
        if !plist.is_nil() {
            collect_value_symbol_names(plist, &mut symbol_names, &mut seen);
        }
    }

    symbol_names
}

fn symbol_has_runtime_surface_or_reference(
    eval: &super::eval::Context,
    id: super::intern::SymId,
    referenced_symbol_names: &std::collections::BTreeSet<String>,
) -> bool {
    symbol_has_runtime_surface(eval, id)
        || referenced_symbol_names.contains(super::intern::resolve_sym(id))
}

fn is_gnu_preloaded_syntax_symbol(name: &str) -> bool {
    // GNU's dump keeps reader, lambda-list, pattern, face-spec, cl-generic
    // specializer, frame/action-alist, and some feature-name markers interned
    // even when they have no value/function/plist surface.  Later Lisp compares
    // these symbols by identity, notably funcall_lambda's `&optional' /
    // `&rest' handling, pcase's backquote expander, lread.c's printed
    // hash-table keys, `face-spec-set-match-display',
    // `cl-generic-generalizers', `frame-width'/`frame-height' frame parameter
    // lookup, and `require' of byte-compiled files whose constants were read
    // during loadup.
    name.starts_with('&')
        || matches!(
            name,
            "," | ",@"
                | "`"
                | "."
                | "..."
                | "_"
                | "quote"
                | "function"
                | "lambda"
                | "closure"
                | "hash-table"
                | "data"
                | "test"
                | "size"
                | "purecopy"
                | "weakness"
                | "cl--class"
                | "cl-deftype-satisfies"
                | "head"
                | "app"
                | "pred"
                | "subclass"
                | "eql"
                | "derived-mode"
                | "oclosure"
                | "cl-defmethod"
                | "width"
                | "height"
                | "unsplittable"
                | "preserve-size"
                | "body-function"
                | "bump-use-time"
                | "dedicated"
                | "inhibit-switch-frame"
                | "window-height"
                | "window-width"
                | "window-size"
                | "body-chars"
                | "text-pixels"
                | "reuse"
                | "window"
                | "frame"
                | "same"
                | "other"
                | "tab"
                | "class"
                | "min-colors"
                | "background"
                | "supports"
                | "color"
                | "light"
                | "dark"
                | "graphic"
                | "tty"
                | "x"
                | "w32"
                | "ns"
                | "haiku"
                | "pgtk"
                | "motif"
                | "gtk"
                | "lucid"
                | "x-toolkit"
                | "icons"
                | "gv"
                | "cl-lib"
                | "cl-macs"
                | "ascii"
                | "unicode"
                | "unicode-bmp"
                | "latin-iso8859-1"
                | "iso-8859-1"
                | "emacs"
                | "eight-bit"
                | "ucs"
        )
}

fn is_gnu_preloaded_builtin_type_property(name: &str, prop: &str) -> bool {
    // GNU's dumped image keeps the `cl-preloaded.el` built-in type graph
    // live at runtime.  Some of the same symbols pass through compile-time
    // cleanup paths because cl-lib helpers are transient, but their
    // built-in class metadata is not transient: pcase, cl-generic, and type
    // predicates inspect it directly.
    matches!(prop, "cl--class" | "cl-deftype-satisfies")
        && matches!(
            name,
            "t" | "atom"
                | "tree-sitter-compiled-query"
                | "tree-sitter-node"
                | "tree-sitter-parser"
                | "user-ptr"
                | "font-object"
                | "font-entity"
                | "font-spec"
                | "condvar"
                | "mutex"
                | "thread"
                | "terminal"
                | "hash-table"
                | "frame"
                | "buffer"
                | "window"
                | "process"
                | "finalizer"
                | "window-configuration"
                | "overlay"
                | "number-or-marker"
                | "symbol"
                | "obarray"
                | "native-comp-unit"
                | "sequence"
                | "list"
                | "array"
                | "number"
                | "float"
                | "integer-or-marker"
                | "integer"
                | "marker"
                | "bignum"
                | "fixnum"
                | "boolean"
                | "symbol-with-pos"
                | "vector"
                | "record"
                | "bool-vector"
                | "char-table"
                | "string"
                | "null"
                | "cons"
                | "function"
                | "compiled-function"
                | "closure"
                | "byte-code-function"
                | "subr"
                | "module-function"
                | "interpreted-function"
                | "special-form"
                | "native-comp-function"
                | "primitive-function"
        )
}

fn restore_gnu_stale_preloaded_face_doc_refs(eval: &mut super::eval::Context) {
    // GNU's dumped image can preserve a compiled-doc reference from loadup even
    // when the installed .elc later shifts by a byte or two.  doc.c then returns
    // nil after its one reload attempt because `custom-declare-face' does not
    // redeclare an already-created face.  The current GNU oracle has exactly
    // that state for this preloaded simple.el face: the offset points into the
    // #@ length header, not at the doc body.
    let face = Value::symbol("blink-matching-paren-offscreen");
    let prop = Value::symbol("face-documentation");
    let Ok(current) = super::builtins::builtin_get(eval, vec![face, prop]) else {
        return;
    };
    if !current.is_cons() {
        return;
    }
    let current_file = current.cons_car();
    if current_file
        .as_lisp_string()
        .is_some_and(|name| name.as_bytes() == b"simple.elc")
    {
        return;
    }
    let Some(position) = current.cons_cdr().as_int() else {
        return;
    };
    let stale_ref = Value::cons(
        Value::string("simple.elc"),
        Value::fixnum(position.saturating_sub(2)),
    );
    let _ = super::builtins::builtin_put(eval, vec![face, prop, stale_ref]);
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn apply_ldefs_boot_autoloads_for_names(
    eval: &mut super::eval::Context,
    names: &[&str],
) -> Result<(), EvalError> {
    let project_root = runtime_project_root();
    let ldefs_path = project_root.join("lisp/ldefs-boot.el");
    let bytes = fs::read(&ldefs_path).map_err(|err| EvalError::Signal {
        symbol: intern("error"),
        data: vec![Value::string(format!(
            "ldefs-boot autoload restore: failed reading {}: {err}",
            ldefs_path.display()
        ))],
        raw_data: None,
    })?;
    let source = decode_emacs_utf8_source_lisp(&bytes);
    let forms = crate::emacs_core::value_reader::read_all_lisp_source(&source, &eval.obarray)
        .map_err(|err| EvalError::Signal {
            symbol: intern("error"),
            data: vec![Value::string(format!(
                "ldefs-boot autoload restore: failed parsing {}: {err}",
                ldefs_path.display()
            ))],
            raw_data: None,
        })?;

    // Phase: parsed Lisp forms in `forms` (a Vec<Value>) live on
    // the malloc heap and are NOT reachable via conservative stack
    // scanning. The intervening `eval_generated_loaddefs_form`
    // calls below can trigger GC and would reclaim the cons cells
    // out from under us. Root every form for the duration of the
    // dispatch loop.
    let wanted = names
        .iter()
        .map(|name| (*name).to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let roots = eval.save_specpdl_roots();
    for form in &forms {
        eval.push_specpdl_root(*form);
    }

    let result: Result<(), EvalError> = (|| {
        for form in &forms {
            let Some(items) = list_to_vec(form) else {
                continue;
            };
            let Some(head) = items.first() else {
                continue;
            };
            if head.is_symbol_named("autoload")
                && let Some(name) = items.get(1).and_then(|v| value_quoted_symbol_name(*v))
                && wanted.contains(&name)
            {
                eval_generated_loaddefs_form(eval, *form)?;
            }
        }

        let mut property_forms: Vec<Value> = Vec::new();
        for form in &forms {
            let Some(items) = list_to_vec(form) else {
                continue;
            };
            let Some(head) = items.first() else {
                continue;
            };
            let Some(head_name) = head.as_symbol_name() else {
                continue;
            };
            if head_name != "function-put" && head_name != "put" {
                continue;
            }
            let Some(name) = items.get(1).and_then(|v| value_quoted_symbol_name(*v)) else {
                continue;
            };
            if wanted.contains(&name) {
                property_forms.push(*form);
            }
        }

        for form in &property_forms {
            eval_generated_loaddefs_form(eval, *form)?;
        }
        Ok(())
    })();
    eval.restore_specpdl_roots(roots);
    result?;
    restore_gnu_stale_preloaded_face_doc_refs(eval);

    Ok(())
}

fn normalize_bootstrap_runtime_surface(
    eval: &mut super::eval::Context,
    project_root: &Path,
) -> Result<(), EvalError> {
    let compile_only_state = compile_only_cl_loaddefs_state(project_root).map_err(|e| {
        tracing::error!("compile_only_cl_loaddefs_state failed: {e:?}");
        e
    })?;
    let runtime_loaddefs_state = runtime_loaddefs_restore_state(project_root).map_err(|e| {
        tracing::error!("runtime_loaddefs_restore_state failed: {e:?}");
        e
    })?;
    let runtime_source_state =
        runtime_source_bootstrap_surface_state(project_root).map_err(|e| {
            tracing::error!("runtime_source_bootstrap_surface_state failed: {e:?}");
            e
        })?;
    let runtime_loaded_state =
        runtime_loaded_source_restore_state(eval, project_root, &compile_only_state.names)
            .map_err(|e| {
                tracing::error!("runtime_loaded_source_restore_state failed: {e:?}");
                e
            })?;
    let mut strip_names = compile_only_state.names.clone();
    strip_names.extend(runtime_loaddefs_state.names.iter().cloned());
    strip_names.extend(runtime_loaded_state.names.iter().cloned());

    let mut stripped_features = TRANSIENT_RUNTIME_FEATURES
        .iter()
        .map(|name| (*name).to_string())
        .collect::<std::collections::BTreeSet<_>>();
    stripped_features.extend(runtime_source_state.features.iter().cloned());
    for feature in &stripped_features {
        eval.remove_feature(feature);
    }
    // Keep the transient helper list authoritative even if the parsed source
    // surface misses a provide edge.
    clear_transient_runtime_features(eval);
    // GNU's dumped runtime starts `gensym-counter` at 0.  Source bootstrap
    // expands many macros while loading core Lisp, so NeoVM must explicitly
    // drop that transient expansion count from the runtime surface.
    eval.set_variable("gensym-counter", Value::fixnum(0));

    for (name, prop) in compile_only_state
        .property_keys
        .iter()
        .chain(runtime_loaddefs_state.property_keys.iter())
        .chain(runtime_loaded_state.property_keys.iter())
        .chain(runtime_source_state.property_keys.iter())
    {
        if is_gnu_preloaded_builtin_type_property(name, prop) {
            continue;
        }
        let _ = super::builtins::builtin_put(
            eval,
            vec![Value::symbol(name), Value::symbol(prop), Value::NIL],
        );
    }

    for name in &strip_names {
        eval.obarray_mut().fmakunbound(&name);
        eval.autoloads.remove(name);
        let _ = super::builtins::builtin_put(
            eval,
            vec![
                Value::symbol(name),
                Value::symbol("autoload-macro"),
                Value::NIL,
            ],
        );
    }
    for name in &runtime_source_state.function_names {
        if runtime_loaddefs_state.names.contains(name) {
            continue;
        }
        eval.obarray_mut().fmakunbound(name);
        eval.autoloads.remove(name);
        let _ = super::builtins::builtin_put(
            eval,
            vec![
                Value::symbol(name),
                Value::symbol("autoload-macro"),
                Value::NIL,
            ],
        );
    }
    for name in &runtime_source_state.variable_names {
        eval.obarray_mut().makunbound(name);
    }
    for name in &runtime_source_state.face_names {
        super::font::clear_created_lisp_face(name);
    }
    let referenced_symbol_names = collect_runtime_referenced_symbol_names(eval);
    for name in &runtime_source_state.symbol_names {
        if runtime_loaddefs_state.symbol_names.contains(name)
            || runtime_loaded_state.symbol_names.contains(name)
        {
            continue;
        }
        let Some(id) = super::intern::lookup_interned(name) else {
            continue;
        };
        if is_gnu_preloaded_syntax_symbol(name) {
            continue;
        }
        if !symbol_has_runtime_surface_or_reference(eval, id, &referenced_symbol_names) {
            eval.obarray_mut().unintern_id(id);
        }
    }

    let autoload_entries = eval.autoloads.entries_snapshot();
    for (name, _) in &autoload_entries {
        if strip_names.contains(name) {
            eval.autoloads.remove(name);
            let _ = super::builtins::builtin_put(
                eval,
                vec![
                    Value::symbol(name),
                    Value::symbol("autoload-macro"),
                    Value::NIL,
                ],
            );
        }
    }

    // Phase: protect parsed-form Values across the autoload/eval
    // calls below. The Values stored in `runtime_loaded_state` and
    // `runtime_loaddefs_state` come from
    // `value_reader::read_all` which allocates Lisp cells on the tagged heap.
    // Conservative stack scanning only reaches stack-resident pointers, NOT
    // pointers stored inside Vec<Value> heap allocations, so intervening GCs
    // (triggered by builtin_autoload, builtin_put, etc.) would reclaim the
    // cons cells and leave the Values dangling. Push them all into temp_roots
    // for the duration of the call.
    let roots = eval.save_specpdl_roots();
    for args in runtime_loaded_state
        .autoload_args
        .iter()
        .chain(runtime_loaddefs_state.autoload_args.iter())
    {
        for v in args {
            eval.push_specpdl_root(*v);
        }
    }
    for form in runtime_loaded_state
        .keymap_defvar_forms
        .iter()
        .chain(runtime_loaddefs_state.keymap_defvar_forms.iter())
    {
        eval.push_specpdl_root(*form);
    }
    for form in runtime_loaded_state
        .property_forms
        .iter()
        .chain(runtime_loaddefs_state.property_forms.iter())
    {
        eval.push_specpdl_root(*form);
    }
    for replay in runtime_loaded_state
        .keymap_forms
        .iter()
        .chain(runtime_loaddefs_state.keymap_forms.iter())
    {
        eval.push_specpdl_root(replay.form);
    }

    let result: Result<(), EvalError> = (|| {
        // GNU loadup.el loads window.el before files.el, and the normalized
        // runtime surface for partial checkpoints at or after that stage keeps
        // the later ldefs-boot keymap links. Earlier checkpoints like
        // `bindings` should retain only the bare prefix maps from subr.el.
        let replay_ldefs_boot_keymaps = eval.feature_present("window");

        for args in runtime_loaded_state
            .autoload_args
            .iter()
            .chain(runtime_loaddefs_state.autoload_args.iter())
        {
            super::autoload::builtin_autoload(eval, args.clone()).map_err(map_flow)?;
        }
        if replay_ldefs_boot_keymaps {
            for form in runtime_loaded_state
                .keymap_defvar_forms
                .iter()
                .chain(runtime_loaddefs_state.keymap_defvar_forms.iter())
            {
                eval_runtime_form(eval, *form)?;
            }
        }
        for form in runtime_loaded_state
            .property_forms
            .iter()
            .chain(runtime_loaddefs_state.property_forms.iter())
        {
            eval_runtime_form(eval, *form)?;
        }
        if replay_ldefs_boot_keymaps {
            for replay in runtime_loaded_state
                .keymap_forms
                .iter()
                .chain(runtime_loaddefs_state.keymap_forms.iter())
            {
                let Some(value) = eval.obarray().symbol_value(&replay.target) else {
                    continue;
                };
                if !is_list_keymap(value) {
                    continue;
                }
                eval_runtime_form(eval, replay.form)?;
            }
        }
        Ok(())
    })();
    eval.restore_specpdl_roots(roots);
    result?;
    restore_gnu_stale_preloaded_face_doc_refs(eval);

    Ok(())
}

pub(crate) fn normalize_final_dump_runtime_surface(
    eval: &mut super::eval::Context,
) -> Result<(), EvalError> {
    let project_root = runtime_project_root();
    let runtime_loaddefs_state = runtime_loaddefs_restore_state(&project_root).map_err(|e| {
        tracing::error!("runtime_loaddefs_restore_state failed: {e:?}");
        e
    })?;
    let runtime_source_state =
        runtime_source_bootstrap_surface_state(&project_root).map_err(|e| {
            tracing::error!("runtime_source_bootstrap_surface_state failed: {e:?}");
            e
        })?;

    let mut stripped_features = TRANSIENT_RUNTIME_FEATURES
        .iter()
        .map(|name| (*name).to_string())
        .collect::<std::collections::BTreeSet<_>>();
    stripped_features.extend(runtime_source_state.features.iter().cloned());
    for feature in &stripped_features {
        eval.remove_feature(feature);
    }
    clear_transient_runtime_features(eval);

    for (name, prop) in runtime_loaddefs_state
        .property_keys
        .iter()
        .chain(runtime_source_state.property_keys.iter())
    {
        if is_gnu_preloaded_builtin_type_property(name, prop) {
            continue;
        }
        let _ = super::builtins::builtin_put(
            eval,
            vec![Value::symbol(name), Value::symbol(prop), Value::NIL],
        );
    }

    for name in &runtime_loaddefs_state.names {
        eval.obarray_mut().fmakunbound(name);
        eval.autoloads.remove(name);
        let _ = super::builtins::builtin_put(
            eval,
            vec![
                Value::symbol(name),
                Value::symbol("autoload-macro"),
                Value::NIL,
            ],
        );
    }
    for name in &runtime_source_state.function_names {
        if runtime_loaddefs_state.names.contains(name) {
            continue;
        }
        eval.obarray_mut().fmakunbound(name);
        eval.autoloads.remove(name);
        let _ = super::builtins::builtin_put(
            eval,
            vec![
                Value::symbol(name),
                Value::symbol("autoload-macro"),
                Value::NIL,
            ],
        );
    }
    for name in &runtime_source_state.variable_names {
        eval.obarray_mut().makunbound(name);
    }
    for name in &runtime_source_state.face_names {
        super::font::clear_created_lisp_face(name);
    }

    let autoload_entries = eval.autoloads.entries_snapshot();
    for (name, _) in &autoload_entries {
        if runtime_loaddefs_state.names.contains(name) {
            eval.autoloads.remove(name);
            let _ = super::builtins::builtin_put(
                eval,
                vec![
                    Value::symbol(name),
                    Value::symbol("autoload-macro"),
                    Value::NIL,
                ],
            );
        }
    }

    // GNU's dumper does not keep the source-loaded `icons', `pcase', and `rx'
    // implementations in the final image; loadup keeps only their generated
    // loaddefs surface.  Neomacs' source-surface parser also interns symbols
    // globally, so strip no-surface parser leftovers after removing the source
    // definitions, while preserving symbols referenced by restored loaddefs.
    let referenced_symbol_names = collect_runtime_referenced_symbol_names(eval);
    for name in &runtime_source_state.symbol_names {
        if runtime_loaddefs_state.symbol_names.contains(name) {
            continue;
        }
        let Some(id) = super::intern::lookup_interned(name) else {
            continue;
        };
        if is_gnu_preloaded_syntax_symbol(name) {
            continue;
        }
        if !symbol_has_runtime_surface_or_reference(eval, id, &referenced_symbol_names) {
            eval.obarray_mut().unintern_id(id);
        }
    }

    let roots = eval.save_specpdl_roots();
    for args in &runtime_loaddefs_state.autoload_args {
        for v in args {
            eval.push_specpdl_root(*v);
        }
    }
    for form in &runtime_loaddefs_state.keymap_defvar_forms {
        eval.push_specpdl_root(*form);
    }
    for form in &runtime_loaddefs_state.property_forms {
        eval.push_specpdl_root(*form);
    }
    for replay in &runtime_loaddefs_state.keymap_forms {
        eval.push_specpdl_root(replay.form);
    }

    let result: Result<(), EvalError> = (|| {
        for args in &runtime_loaddefs_state.autoload_args {
            super::autoload::builtin_autoload(eval, args.clone()).map_err(map_flow)?;
        }
        for form in &runtime_loaddefs_state.property_forms {
            eval_runtime_form(eval, *form)?;
        }
        Ok(())
    })();
    eval.restore_specpdl_roots(roots);
    result?;
    restore_gnu_stale_preloaded_face_doc_refs(eval);

    Ok(())
}

fn bootstrap_runtime_window_system_symbol(eval: &mut super::eval::Context) -> Option<Value> {
    if eval.feature_present("neomacs")
        || eval.feature_present(super::display::gui_window_system_symbol())
    {
        Some(Value::symbol(super::display::gui_window_system_symbol()))
    } else if eval.feature_present("x") {
        Some(Value::symbol("x"))
    } else {
        None
    }
}

fn restore_cached_runtime_window_system_surface(eval: &mut super::eval::Context) {
    let Some(window_system) = bootstrap_runtime_window_system_symbol(eval) else {
        return;
    };

    let frame_id = if let Some(frame_id) = eval.frames.selected_frame().map(|frame| frame.id) {
        Some(frame_id)
    } else if let Some(frame_id) = eval.frames.frame_list().into_iter().next() {
        let _ = eval.frames.select_frame(frame_id);
        eval.sync_keyboard_terminal_owner();
        Some(frame_id)
    } else {
        None
    };

    if let Some(frame_id) = frame_id
        && let Some(frame) = eval.frames.get_mut(frame_id)
    {
        frame.set_window_system(Some(window_system));
    }

    eval.set_variable("window-system", window_system);
    eval.set_variable("initial-window-system", window_system);
}

/// Build the `exec-path` directory list from the runtime `PATH` environment
/// variable, mirroring GNU `decode_env_path ("PATH", NULL, false)` (emacs.c).
///
///  - Split on the platform path separator (`SEPCHAR`): `;` on Windows, `:`
///    on Unix. Splitting on a hardcoded `:` corrupts Windows `PATH` entries,
///    whose directories carry a `:` drive letter (e.g.
///    `C:\Program Files\Git\cmd`) and are joined with `;` — so `exec-path`
///    fills with bogus fragments and `executable-find` can never locate git
///    (GitHub issue #126).
///  - Empty elements default to "." (the current directory), as GNU does
///    when its EMPTY argument is false.
///  - Present each directory in GNU's Lisp file-name syntax (`/` separators
///    on Windows), matching its `dostounix_filename` normalization.
pub(crate) fn exec_path_dirs_from_env() -> Vec<String> {
    exec_path_dirs_from_os(std::env::var_os("PATH"))
}

/// Core of [`exec_path_dirs_from_env`], split out so it can be unit-tested
/// with an explicit `PATH` value instead of mutating the process environment.
pub(crate) fn exec_path_dirs_from_os(path: Option<std::ffi::OsString>) -> Vec<String> {
    let Some(path) = path else {
        return Vec::new();
    };
    std::env::split_paths(&path)
        .map(|dir| {
            if dir.as_os_str().is_empty() {
                ".".to_string()
            } else {
                super::fileio::host_path_to_lisp_file_name_string(&dir)
            }
        })
        .collect()
}

fn clear_runtime_loader_state(eval: &mut super::eval::Context) {
    // These stacks only describe in-flight bootstrap loads/requires.
    // Letting them leak into the runtime surface makes later `require`
    // calls falsely look recursive/already-active.
    eval.require_stack.clear();
    eval.loads_in_progress.clear();
}

fn finalize_cached_bootstrap_eval(
    eval: &mut super::eval::Context,
    project_root: &Path,
) -> Result<(), EvalError> {
    // Register all builtins — pdump doesn't preserve live Rust entry-point
    // pointers on heap subr objects, so the callable surface must be rebuilt.
    // GNU Emacs loads the pdump as-is with no cleanup/normalization.
    // We only need to:
    // 1. Re-register builtins (pdump can't preserve Rust function pointers)
    // 2. Re-install BUFFER_OBJFWD forwarders (pdump load leaves the
    //    redirect as Plainval; mirror Context::new_inner here so
    //    default-directory etc. are Forwarded again).
    // 3. Reset thread-local caches
    // 4. Set path variables for the current runtime location
    super::builtins::init_builtins(eval);

    // Re-install BUFFER_OBJFWD forwarders to restore the Forwarded
    // redirect tag on per-buffer variables. `pdump::convert.rs`
    // leaves Forwarded symbols at Plainval/NIL (documented Phase 8
    // gap), so writes via set_variable would otherwise bypass the
    // per-buffer slot entirely. Mirrors the loop in
    // `Context::new_inner`.
    {
        use crate::buffer::buffer::BUFFER_SLOT_INFO;
        use crate::emacs_core::forward::alloc_buffer_objfwd;
        use crate::emacs_core::intern::intern;
        let obarray = eval.obarray_mut();
        for info in BUFFER_SLOT_INFO {
            // Phase 10D holdouts 3/4/5: skip internal-only slots
            // (syntax-table / category-table / case-table) — they
            // live in the BVAR slot block but have no Lisp variable
            // exposure, matching GNU.
            if !info.install_as_forwarder {
                continue;
            }
            let id = intern(info.name);
            let predicate = if info.predicate.is_empty() {
                intern("null")
            } else {
                intern(info.predicate)
            };
            let fwd = alloc_buffer_objfwd(
                info.offset.as_u16(),
                info.local_flags_idx,
                predicate,
                info.default.to_value(),
            );
            obarray.install_buffer_objfwd(id, fwd);
        }
    }
    super::font::restore_created_faces_from_table(&eval.face_table.face_list());
    clear_runtime_loader_state(eval);
    clear_transient_runtime_features(eval);
    ensure_startup_compat_variables(eval, project_root);
    restore_cached_runtime_window_system_surface(eval);
    // GNU subr.el defines this as 0 in the dumped image.  Bootstrap/loading
    // code may have consumed gensyms before pdump serialization; do not expose
    // that transient construction state to runtime Lisp.
    eval.set_variable("gensym-counter", Value::fixnum(0));

    let lisp_dir = project_root.join("lisp");
    eval.set_variable(
        "load-path",
        Value::list(runtime_load_path_entries(&lisp_dir)),
    );

    let etc_dir = project_root.join("etc");
    eval.set_variable(
        "data-directory",
        Value::unibyte_string(lisp_directory_name_from_host_path(&etc_dir)),
    );
    eval.set_variable(
        "doc-directory",
        Value::unibyte_string(lisp_directory_name_from_host_path(&etc_dir)),
    );
    eval.set_variable(
        "source-directory",
        Value::unibyte_string(lisp_directory_name_from_host_path(project_root)),
    );
    eval.set_variable(
        "installation-directory",
        Value::unibyte_string(lisp_directory_name_from_host_path(project_root)),
    );

    // Mirror GNU `init_buffer` (`src/buffer.c:4923`): after loading
    // the dumped image, switch to `*scratch*` and reset its
    // `default-directory` to the runtime cwd captured at startup
    // (GNU `emacs_wd` / our `std::env::current_dir()`). GNU only
    // touches the scratch buffer and the (shared) minibuffer here —
    // every other buffer inherits on creation. Mirror that by
    // setting just the current buffer's slot via `set_variable`,
    // which routes through the FORWARDED dispatch.
    if let Ok(cwd) = std::env::current_dir() {
        eval.set_variable(
            "default-directory",
            Value::unibyte_string(lisp_directory_name_from_host_path(&cwd)),
        );
    }

    // GNU's dumped image reaches `normal-top-level` with
    // `abbreviated-home-dir` unset, so the first startup
    // `abbreviate-file-name` computes the cache from the runtime HOME
    // rather than the build/dump HOME.  Neomacs can compute the cache
    // while constructing its dump; clear only the cache value here and
    // let lisp/files.el repopulate its `home` plist entry.
    eval.set_variable("abbreviated-home-dir", Value::NIL);

    // Mirror GNU `init_callproc` (src/callproc.c:1960-1963,2038-2044):
    // re-initialize exec-path and shell-file-name from the RUNTIME
    // environment so that CI-built release images don't bake in the
    // build machine's $SHELL / $PATH.  The pdump carried build-time
    // values; these must be overwritten after every pdump load.
    //
    // exec-path: list of dirs from runtime $PATH
    // (GNU callproc.c: init_callproc_1 / init_callproc)
    {
        let path_dirs: Vec<Value> = exec_path_dirs_from_env()
            .into_iter()
            .map(Value::unibyte_string)
            .collect();
        eval.set_variable("exec-path", Value::list(path_dirs));
    }

    // exec-directory: directory containing the neomacs binary
    // (GNU callproc.c:1961 — Ffile_name_as_directory of car of exec-path,
    //  then overridden in init_callproc with lib-src dir)
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        eval.set_variable(
            "exec-directory",
            Value::unibyte_string(lisp_directory_name_from_host_path(dir)),
        );
    }

    // shell-file-name: $SHELL from runtime environment, or "/bin/sh"
    // (GNU callproc.c:2038-2044)
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        eval.set_variable("shell-file-name", Value::unibyte_string(shell));
    }

    restore_gnu_stale_preloaded_face_doc_refs(eval);
    eval.clear_top_level_eval_state();

    Ok(())
}

pub(crate) fn bootstrap_load_path_entries(lisp_dir: &Path) -> Vec<Value> {
    let mut load_path_entries = Vec::new();
    for sub in BOOTSTRAP_LOAD_PATH_SUBDIRS {
        let dir = if sub.is_empty() {
            lisp_dir.to_path_buf()
        } else {
            lisp_dir.join(sub)
        };
        if dir.is_dir() {
            load_path_entries.push(Value::string(
                crate::emacs_core::fileio::host_path_to_lisp_file_name_string(&dir),
            ));
        }
    }
    load_path_entries
}

/// Build the runtime `load-path` from `EMACSLOADPATH` and Neomacs' bundled
/// Lisp directories.  As in GNU `init_lread` (`src/lread.c`), an empty path
/// element stands for the entire default load path rather than the current
/// directory.  If there is no empty element, keep the defaults at the end:
/// unlike an installed GNU Emacs, Neomacs currently has no launcher wrapper
/// that appends its versioned Lisp directory to `EMACSLOADPATH`.
fn runtime_load_path_entries(lisp_dir: &Path) -> Vec<Value> {
    runtime_load_path_entries_from_os(lisp_dir, std::env::var_os("EMACSLOADPATH"))
}

/// Testable core of [`runtime_load_path_entries`].
pub(crate) fn runtime_load_path_entries_from_os(
    lisp_dir: &Path,
    emacs_load_path: Option<std::ffi::OsString>,
) -> Vec<Value> {
    let default_load_path = bootstrap_load_path_entries(lisp_dir);
    let Some(emacs_load_path) = emacs_load_path else {
        return default_load_path;
    };

    let mut load_path = Vec::new();
    let mut included_defaults = false;
    for dir in std::env::split_paths(&emacs_load_path) {
        if dir.as_os_str().is_empty() {
            load_path.extend(default_load_path.iter().cloned());
            included_defaults = true;
        } else {
            load_path.push(Value::string(
                crate::emacs_core::fileio::host_path_to_lisp_file_name_string(&dir),
            ));
        }
    }
    if !included_defaults {
        load_path.extend(default_load_path);
    }
    load_path
}

fn lisp_directory_name_from_host_path(path: &Path) -> String {
    let mut name = crate::emacs_core::fileio::host_path_to_lisp_file_name_string(path);
    if !name.ends_with('/') {
        name.push('/');
    }
    name
}

fn eval_startup_forms(eval: &mut super::eval::Context, forms_src: &str) -> Result<(), EvalError> {
    eval.eval_str(forms_src)?;
    Ok(())
}

/// Apply the runtime startup state that GNU Emacs has after the dumped image
/// is loaded and `normal-top-level` begins to run.
///
/// The dumped bootstrap image intentionally stops before normal interactive
/// startup.  Runtime callers that compare against `emacs --batch -Q` still
/// need the early startup buffer initialization that `startup.el` performs for
/// the `*scratch*` buffer.
fn sync_runtime_interpreted_closure_filter(eval: &mut super::eval::Context) {
    let closure_filter_sym = super::intern::intern("internal-make-interpreted-closure-function");
    let cconv_sym = super::intern::intern("cconv-make-interpreted-closure");
    let filter_fn = eval
        .obarray()
        .symbol_value_id(closure_filter_sym)
        .cloned()
        .and_then(|value| {
            if value.as_symbol_id() == Some(cconv_sym) {
                eval.obarray().symbol_function_id(cconv_sym)
            } else {
                None
            }
        });
    eval.set_interpreted_closure_filter_fn(filter_fn);
}

pub fn apply_runtime_startup_state(eval: &mut super::eval::Context) -> Result<(), EvalError> {
    let project_root = runtime_project_root();
    let minibuf_id = eval
        .buffers
        .find_buffer_by_name(" *Minibuf-0*")
        .unwrap_or_else(|| eval.buffers.create_buffer(" *Minibuf-0*"));
    eval.ensure_startup_messages_buffer();
    if let Some(messages_id) = eval.buffers.find_buffer_by_name("*Messages*") {
        eval.buffers
            .note_buffer_order_after(messages_id, minibuf_id);
    }
    let scratch_id = eval
        .buffers
        .find_buffer_by_name("*scratch*")
        .unwrap_or_else(|| eval.buffers.create_buffer("*scratch*"));
    eval.set_current_buffer_unrecorded(scratch_id)
        .map_err(map_flow)?;
    super::window_cmds::seed_batch_startup_frame_in_state(&mut eval.frames, &mut eval.buffers);
    eval_startup_forms(
        eval,
        // GNU `startup.el` abbreviates `default-directory` after loadup, and
        // the initial window displays `*scratch*` through the same metadata
        // path as later `set-window-buffer` calls.
        r#"
          (progn
            (setq default-directory (abbreviate-file-name default-directory))
            (set-window-buffer (selected-window) (current-buffer)))
        "#,
    )?;
    eval_startup_forms(
        eval,
        // Note: the closing paren count must balance the opens.
        // GNU loadup.el invokes `initial-major-mode` on `*scratch*`
        // when it's still in `fundamental-mode`; we replicate that
        // post-loadup hook here.
        r#"
          (if (get-buffer "*scratch*")
              (with-current-buffer "*scratch*"
                (if (eq major-mode 'fundamental-mode)
                    (funcall initial-major-mode))))
        "#,
    )?;

    // GNU's startup path reaches its post-startup surface through compiled
    // early Lisp. NeoVM executes the same files from source, which can
    // transiently reload compile-time helpers such as `cl-lib` and `gv`.
    // Normalize the runtime-visible autoload/feature surface again after
    // those forms run.
    normalize_bootstrap_runtime_surface(eval, &project_root)?;

    sync_runtime_interpreted_closure_filter(eval);
    clear_transient_runtime_features(eval);
    eval.set_variable("max-lisp-eval-depth", Value::fixnum(1600));
    eval.clear_top_level_eval_state();
    // GNU startup evaluates command-line --eval forms with lexical=t
    // (startup.el: command-line-1), and the post-startup *scratch* surface is
    // lexical. Set this after unwinding transient specpdl bindings so the
    // runtime top-level surface persists.
    eval.set_lexical_binding(true);
    // Repair C-level DEFVAR declarations that may come from an older cached
    // pdump. These mirror GNU callint.c, keyboard.c, and minibuf.c startup
    // declarations and must be special under lexical-binding.
    for name in [
        "command-history",
        "command-debug-status",
        "mark-even-if-inactive",
        "current-minibuffer-command",
    ] {
        eval.obarray.make_special(name);
    }
    eval.assign(
        "minibuffer-prompt-properties",
        Value::list(vec![Value::symbol("read-only"), Value::T]),
    );

    Ok(())
}

fn install_bootstrap_x_window_system_vars(
    eval: &mut super::eval::Context,
) -> Result<(), EvalError> {
    let keysym_table = builtin_make_hash_table(vec![
        Value::keyword(":test"),
        Value::symbol("eql"),
        Value::keyword(":size"),
        Value::fixnum(900),
    ])
    .map_err(map_flow)?;
    eval.set_variable("x-keysym-table", keysym_table);
    eval.set_variable("x-toolkit-scroll-bars", Value::symbol("gtk"));
    eval.set_variable("x-selection-timeout", Value::fixnum(0));
    eval.set_variable("x-session-id", Value::NIL);
    eval.set_variable("x-session-previous-id", Value::NIL);
    eval.set_variable("x-lost-selection-functions", Value::NIL);
    eval.set_variable("x-sent-selection-functions", Value::NIL);
    for name in [
        "x-ctrl-keysym",
        "x-alt-keysym",
        "x-hyper-keysym",
        "x-meta-keysym",
        "x-super-keysym",
    ] {
        eval.set_variable(name, Value::NIL);
    }
    Ok(())
}

fn maybe_trace_bootstrap_step(message: impl AsRef<str>) {
    if std::env::var_os("NEOVM_TRACE_BOOTSTRAP_STEPS").is_some() {
        eprintln!("bootstrap-step: {}", message.as_ref());
    }
}

fn maybe_trace_bootstrap_macro_perf(eval: &super::eval::Context) {
    if let Some(summary) = eval.macro_perf_summary() {
        let gc_elapsed = eval
            .obarray()
            .symbol_value("gc-elapsed")
            .and_then(|value| value.as_number_f64())
            .unwrap_or(0.0);
        eprintln!(
            "bootstrap-macro-perf: {summary} | gc=gcs-done:{} elapsed:{:.3}s",
            eval.gc_count, gc_elapsed
        );
    }
}

pub fn create_bootstrap_evaluator() -> Result<super::eval::Context, EvalError> {
    create_bootstrap_evaluator_with_features(&[])
}

fn set_loadup_dump_mode(eval: &mut super::eval::Context, dump_mode: Option<LoadupDumpMode>) {
    match dump_mode {
        Some(mode) => eval.set_variable("dump-mode", Value::string(mode.as_gnu_string())),
        None => eval.set_variable("dump-mode", Value::NIL),
    }
}

fn apply_loadup_startup_surface(
    eval: &mut super::eval::Context,
    startup_surface: &LoadupStartupSurface,
) {
    let argv = startup_surface
        .command_line_args
        .iter()
        .cloned()
        .map(Value::string)
        .collect::<Vec<_>>();
    eval.set_variable("command-line-args", Value::list(argv));
    eval.set_variable("command-line-args-left", Value::NIL);
    eval.set_variable("command-line-processed", Value::NIL);
    eval.set_variable(
        "noninteractive",
        if startup_surface.noninteractive {
            Value::T
        } else {
            Value::NIL
        },
    );
}

pub fn create_bootstrap_evaluator_with_features(
    extra_features: &[&str],
) -> Result<super::eval::Context, EvalError> {
    create_bootstrap_evaluator_with_dump_mode(extra_features, None)
}

pub fn create_bootstrap_evaluator_with_dump_mode(
    extra_features: &[&str],
    dump_mode: Option<LoadupDumpMode>,
) -> Result<super::eval::Context, EvalError> {
    create_bootstrap_evaluator_with_startup_surface(extra_features, dump_mode, None)
}

pub fn create_bootstrap_evaluator_with_startup_surface(
    extra_features: &[&str],
    dump_mode: Option<LoadupDumpMode>,
    startup_surface: Option<&LoadupStartupSurface>,
) -> Result<super::eval::Context, EvalError> {
    // Discover the runtime root (contains lisp/ and etc/).
    let project_root = runtime_project_root();
    let lisp_dir = project_root.join("lisp");
    assert!(
        lisp_dir.is_dir(),
        "lisp/ directory not found at {}",
        lisp_dir.display()
    );
    stacker::maybe_grow(128 * 1024, 2 * 1024 * 1024, || {
        maybe_trace_bootstrap_step("create_bootstrap_evaluator_with_features: enter");
        let mut eval = super::eval::Context::new();
        maybe_trace_bootstrap_step("create_bootstrap_evaluator_with_features: evaluator-new");
        let bootstrap_features = normalized_bootstrap_features(extra_features);
        for feature in &bootstrap_features {
            let _ = eval.provide_value(Value::symbol(&feature), None);
        }
        maybe_trace_bootstrap_step(format!(
            "create_bootstrap_evaluator_with_features: provided-features={bootstrap_features:?}"
        ));
        if bootstrap_features.iter().any(|feature| feature == "x") {
            install_bootstrap_x_window_system_vars(&mut eval)?;
            maybe_trace_bootstrap_step(
                "create_bootstrap_evaluator_with_features: installed-x-window-system-vars",
            );
        }

        // Set up load-path with lisp/ and its subdirectories.
        eval.set_variable(
            "load-path",
            Value::list(bootstrap_load_path_entries(&lisp_dir)),
        );
        let bootstrap_frame_id = super::window_cmds::seed_batch_startup_frame_in_state(
            &mut eval.frames,
            &mut eval.buffers,
        );
        maybe_trace_bootstrap_step(format!(
            "create_bootstrap_evaluator_with_features: seeded-batch-bootstrap-frame={bootstrap_frame_id:?}"
        ));
        if let Some(startup_surface) = startup_surface {
            apply_loadup_startup_surface(&mut eval, startup_surface);
            maybe_trace_bootstrap_step(
                "create_bootstrap_evaluator_with_features: applied-loadup-startup-surface",
            );
        }
        // GNU loadup.el uses a string-valued dump-mode (`pdump` /
        // `pbootstrap`) to decide whether Lisp should call
        // `dump-emacs-portable`. Keep ordinary cached bootstrap on nil, but
        // let explicit temacs-style flows seed the real GNU value here.
        set_loadup_dump_mode(&mut eval, dump_mode);
        eval.set_variable("purify-flag", Value::NIL);
        eval.set_variable("max-lisp-eval-depth", Value::fixnum(1600));
        eval.set_variable("inhibit-load-charset-map", Value::T);
        // data-directory: directory of machine-independent data files (etc/)
        let etc_dir = project_root.join("etc");
        eval.set_variable(
            "data-directory",
            Value::unibyte_string(lisp_directory_name_from_host_path(&etc_dir)),
        );
        eval.set_variable(
            "doc-directory",
            Value::unibyte_string(lisp_directory_name_from_host_path(&etc_dir)),
        );
        // source-directory: top-level source tree
        eval.set_variable(
            "source-directory",
            Value::unibyte_string(lisp_directory_name_from_host_path(&project_root)),
        );
        eval.set_variable(
            "installation-directory",
            Value::unibyte_string(lisp_directory_name_from_host_path(&project_root)),
        );

        // exec-path: list of dirs from PATH env var (C: callproc.c init_callproc_1)
        let path_dirs: Vec<Value> = exec_path_dirs_from_env()
            .into_iter()
            .map(Value::unibyte_string)
            .collect();
        eval.set_variable("exec-path", Value::list(path_dirs));
        eval.set_variable("exec-suffixes", Value::NIL);
        eval.set_variable("exec-directory", Value::NIL);
        eval.obarray.make_special("exec-path");
        eval.obarray.make_special("exec-suffixes");
        eval.obarray.make_special("exec-directory");
        // GNU callproc.c: syms_of_callproc defines these Lisp variables
        // before Lisp files read them as defcustom defaults.  GNU sets
        // `emacsclient-program-name` to "emacsclient" because it ships a
        // matching lib-src/emacsclient.  Neomacs ships `neomacsclient`;
        // advertising the GNU name makes packages pick a host GNU client
        // that can pass version probes but fail against a Neomacs server.
        for (name, program) in [
            ("ctags-program-name", "ctags"),
            ("etags-program-name", "etags"),
            ("hexl-program-name", "hexl"),
            ("emacsclient-program-name", "neomacsclient"),
            ("movemail-program-name", "movemail"),
            ("ebrowse-program-name", "ebrowse"),
            ("rcs2log-program-name", "rcs2log"),
        ] {
            eval.set_variable(name, Value::unibyte_string(program));
            eval.obarray.make_special(name);
        }

        // shell-file-name: GNU callproc.c:2041 — $SHELL or /bin/sh
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        eval.set_variable("shell-file-name", Value::unibyte_string(shell));
        eval.obarray.make_special("shell-file-name");
        // shell-command-switch: GNU simple.el — defaults to "-c"
        eval.set_variable("shell-command-switch", Value::unibyte_string("-c"));

        // menu-bar-final-items: list of menu-bar items to put at end (C: xmenu.c)
        eval.set_variable(
            "menu-bar-final-items",
            Value::list(vec![Value::symbol("help-menu")]),
        );

        // glyphless-char-display: char-table for glyphless character display
        // (C: xdisp.c syms_of_xdisp). First register extra slots, then create.
        {
            let stubs = [
                "(put 'glyphless-char-display 'char-table-extra-slots 1)",
                "(setq glyphless-char-display (make-char-table 'glyphless-char-display nil))",
                "(set-char-table-extra-slot glyphless-char-display 0 'empty-box)",
            ];
            for stub in &stubs {
                let _ = eval.eval_str(stub);
            }
        }

        // Load loadup.el — this does everything GNU's loadup.el does:
        // loads all core .el/.elc files, handles platform conditionals,
        // manages eager expansion, etc.
        let loadup_path = lisp_dir.join("loadup.el");
        tracing::info!("Loading loadup.el from {}", loadup_path.display());
        match load_file(&mut eval, &loadup_path) {
            Ok(_) => tracing::info!("loadup.el completed successfully"),
            Err(e) => {
                if is_kill_emacs_signal(&e) {
                    tracing::info!("loadup.el completed (kill-emacs after dump)");
                } else {
                    let rendered = format_eval_error_in_state(&eval, &e);
                    tracing::error!("loadup.el failed: {rendered}");
                    maybe_trace_bootstrap_step(format!(
                        "create_bootstrap_evaluator_with_features: loadup-failed={rendered}"
                    ));
                    return Err(e);
                }
            }
        }
        maybe_trace_bootstrap_macro_perf(&eval);

        if dump_mode.is_some() && eval.shutdown_request.is_some() {
            return Ok(eval);
        }

        // If loadup.el set a shutdown request (via kill-emacs at the end
        // of the dump flow), clear it so the caller gets a usable evaluator.
        eval.shutdown_request = None;

        tracing::info!("\n=== LOADUP BOOTSTRAP COMPLETE ===");

        // Modern Emacs (27+) defaults to lexical-binding: t for *scratch*
        // and interactive evaluation. Match this for oracle test parity.
        eval.set_lexical_binding(true);
        eval.clear_top_level_eval_state();
        let _ = eval.frames.delete_frame(bootstrap_frame_id);
        clear_runtime_loader_state(&mut eval);

        Ok(eval)
    })
}

/// Create a bootstrap evaluator, using a pdump cache file if available.
///
/// On first call, performs the full bootstrap and saves the result to a
/// `.pdump` file next to the `lisp/` directory. On subsequent calls,
/// loads from the dump file (~10-50ms vs 3-5s bootstrap).
///
/// The dump file is invalidated by the bootstrap image schema version and
/// by a fingerprint of the runtime root's Lisp sources. Set
/// `NEOVM_DISABLE_PDUMP=1` to force fresh bootstrap.
pub fn create_bootstrap_evaluator_cached() -> Result<super::eval::Context, EvalError> {
    create_bootstrap_evaluator_cached_with_features(&[])
}

pub fn create_runtime_startup_evaluator() -> Result<super::eval::Context, EvalError> {
    create_runtime_startup_evaluator_with_features(&[])
}

pub(crate) fn create_runtime_startup_evaluator_at_path(
    extra_features: &[&str],
    dump_path: &Path,
) -> Result<super::eval::Context, EvalError> {
    let mut eval = create_bootstrap_evaluator_cached_at_path(extra_features, dump_path)?;
    apply_runtime_startup_state(&mut eval)?;
    maybe_run_after_pdump_load_hook(&mut eval);

    Ok(eval)
}

pub fn create_runtime_startup_evaluator_with_features(
    extra_features: &[&str],
) -> Result<super::eval::Context, EvalError> {
    let project_root = runtime_project_root();
    let dump_path = bootstrap_dump_path(&project_root, extra_features);
    create_runtime_startup_evaluator_at_path(extra_features, &dump_path)
}

pub fn create_runtime_startup_evaluator_cached() -> Result<super::eval::Context, EvalError> {
    create_runtime_startup_evaluator()
}

pub fn create_runtime_startup_evaluator_cached_with_features(
    extra_features: &[&str],
) -> Result<super::eval::Context, EvalError> {
    create_runtime_startup_evaluator_with_features(extra_features)
}

pub fn load_runtime_image_with_features(
    role: RuntimeImageRole,
    extra_features: &[&str],
    dump_path: Option<&Path>,
) -> Result<super::eval::Context, EvalError> {
    let executable = runtime_image_executable(role, dump_path);
    load_runtime_image_with_features_for_executable(role, extra_features, dump_path, &executable)
}

fn runtime_image_executable(role: RuntimeImageRole, dump_path: Option<&Path>) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok().or(Some(path)))
        .unwrap_or_else(|| {
            if dump_path.is_some() {
                PathBuf::from(role.image_file_name())
            } else {
                default_runtime_image_path(role)
            }
        })
}

/// Whether any runtime image candidate for ROLE exists on disk for the
/// running executable. Lets startup pick a degradation tier (final image,
/// bootstrap image, source bootstrap) before attempting a load, so a
/// merely-absent image never has to be distinguished from a corrupt one
/// after the fact.
pub fn runtime_image_available(role: RuntimeImageRole) -> bool {
    let executable = runtime_image_executable(role, None);
    runtime_image_candidate_paths_for_executable(&executable, role)
        .iter()
        .any(|candidate| candidate.exists())
}

pub(crate) fn load_runtime_image_with_features_for_executable(
    role: RuntimeImageRole,
    extra_features: &[&str],
    dump_path: Option<&Path>,
    executable: &Path,
) -> Result<super::eval::Context, EvalError> {
    use super::pdump;

    let project_root = runtime_project_root();
    let candidates = dump_path
        .map(|path| vec![path.to_path_buf()])
        .unwrap_or_else(|| runtime_image_candidate_paths_for_executable(executable, role));
    let mut eval = {
        let mut last_error = None;
        let mut loaded = None;
        for (index, candidate) in candidates.iter().enumerate() {
            match pdump::load_from_dump(candidate) {
                Ok(eval) => {
                    loaded = Some(eval);
                    break;
                }
                Err(pdump::DumpError::Io(err))
                    if err.kind() == std::io::ErrorKind::NotFound
                        && index + 1 < candidates.len() =>
                {
                    tracing::info!(
                        "pdump: runtime image {} not found, trying next candidate",
                        candidate.display()
                    );
                }
                Err(err) => {
                    last_error = Some(runtime_image_load_error(role, candidate, err));
                    break;
                }
            }
        }
        match (loaded, last_error) {
            (Some(eval), _) => eval,
            (None, Some(err)) => return Err(err),
            (None, None) => {
                return Err(runtime_image_load_error(
                    role,
                    &default_fingerprinted_runtime_image_path(role),
                    pdump::DumpError::Io(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "runtime image not found",
                    )),
                ));
            }
        }
    };

    if !extra_features.is_empty() {
        let bootstrap_features = normalized_bootstrap_features(extra_features);
        for feature in &bootstrap_features {
            let _ = eval.provide_value(Value::symbol(feature), None);
        }
    }

    finalize_cached_bootstrap_eval(&mut eval, &project_root).map_err(|e| {
        tracing::error!("finalize_cached_bootstrap_eval failed: {e:?}");
        e
    })?;

    Ok(eval)
}

fn runtime_image_load_error(
    role: RuntimeImageRole,
    dump_path: &Path,
    err: super::pdump::DumpError,
) -> EvalError {
    let image_kind = match role {
        RuntimeImageRole::Bootstrap => "bootstrap",
        RuntimeImageRole::Final => "final",
    };
    let message = format!(
        "failed to load {image_kind} image {}: {err}",
        dump_path.display()
    );
    tracing::error!("{message}");
    let payload = Value::symbol(intern(&message));
    EvalError::Signal {
        symbol: intern("error"),
        data: vec![payload],
        raw_data: Some(payload),
    }
}

pub fn maybe_run_after_pdump_load_hook(eval: &mut super::eval::Context) -> bool {
    if super::pdump::take_after_pdump_load_hook_pending(eval) {
        super::pdump::runtime::run_after_pdump_load_hook(eval);
        return true;
    }
    false
}

pub fn create_bootstrap_evaluator_cached_with_features(
    extra_features: &[&str],
) -> Result<super::eval::Context, EvalError> {
    let project_root = runtime_project_root();
    let dump_path = bootstrap_dump_path(&project_root, extra_features);
    create_bootstrap_evaluator_cached_at_path(extra_features, &dump_path)
}

pub(crate) fn create_bootstrap_evaluator_cached_at_path(
    extra_features: &[&str],
    dump_path: &Path,
) -> Result<super::eval::Context, EvalError> {
    use super::pdump;

    fn finalize_or_log(
        eval: &mut super::eval::Context,
        project_root: &Path,
        context: &str,
    ) -> Result<(), EvalError> {
        match finalize_cached_bootstrap_eval(eval, project_root) {
            Ok(()) => Ok(()),
            Err(err) => {
                let rendered = format_eval_error_in_state(eval, &err);
                tracing::error!("{context}: {rendered}");
                Err(err)
            }
        }
    }

    fn try_load_dump(
        dump_path: &Path,
        project_root: &Path,
        log_context: &str,
    ) -> Result<Option<super::eval::Context>, EvalError> {
        let start = std::time::Instant::now();
        match pdump::load_from_dump(dump_path) {
            Ok(mut eval) => {
                tracing::info!(
                    "pdump: loaded bootstrap state from {} {} ({:.2?})",
                    dump_path.display(),
                    log_context,
                    start.elapsed()
                );
                finalize_or_log(
                    &mut eval,
                    project_root,
                    "pdump finalize after cached load failed",
                )?;
                Ok(Some(eval))
            }
            Err(err) => {
                tracing::warn!(
                    "pdump: load {} failed ({err})",
                    if log_context.is_empty() {
                        "attempt"
                    } else {
                        log_context
                    }
                );
                Ok(None)
            }
        }
    }

    let project_root = runtime_project_root();
    let lock_path = bootstrap_dump_lock_path(dump_path);
    tracing::info!("pdump: bootstrap cache candidate {}", dump_path.display());

    // Allow disabling pdump via env var
    if std::env::var("NEOVM_DISABLE_PDUMP").unwrap_or_default() == "1" {
        let mut eval = create_bootstrap_evaluator_with_features(extra_features)?;
        finalize_or_log(&mut eval, &project_root, "pdump disabled finalize failed")?;
        return Ok(eval);
    }

    // Try loading from dump first
    if dump_path.exists() {
        if let Some(eval) = try_load_dump(dump_path, &project_root, "on first attempt")? {
            return Ok(eval);
        }
    } else {
        tracing::info!("pdump: bootstrap cache miss at {}", dump_path.display());
    }

    let _write_lock = match BootstrapCacheWriteLock::acquire(&lock_path) {
        Ok(lock) => Some(lock),
        Err(BootstrapCacheLockError::Busy(err)) => {
            tracing::info!("pdump: waiting for bootstrap cache writer ({err})");
            match BootstrapCacheReadLock::wait(&lock_path) {
                Ok(read_lock) => {
                    if dump_path.exists()
                        && let Some(eval) =
                            try_load_dump(dump_path, &project_root, "after waiting for writer")?
                    {
                        return Ok(eval);
                    }

                    drop(read_lock);
                    match BootstrapCacheWriteLock::acquire(&lock_path) {
                        Ok(lock) => Some(lock),
                        Err(err) => {
                            tracing::warn!(
                                "pdump: cache writer handoff unavailable ({err}), bootstrapping without cache"
                            );
                            None
                        }
                    }
                }
                Err(wait_err) => {
                    tracing::warn!(
                        "pdump: cache wait failed ({wait_err}), bootstrapping without cache"
                    );
                    None
                }
            }
        }
        Err(BootstrapCacheLockError::Other(err)) => {
            tracing::warn!("pdump: cache lock unavailable ({err}), bootstrapping without cache");
            None
        }
    };

    if _write_lock.is_none() {
        let mut eval = create_bootstrap_evaluator_with_features(extra_features)?;
        ensure_startup_compat_variables(&mut eval, &project_root);
        finalize_or_log(
            &mut eval,
            &project_root,
            "pdump lockless fallback finalize failed",
        )?;
        return Ok(eval);
    }

    if dump_path.exists() {
        if let Some(eval) = try_load_dump(dump_path, &project_root, "after acquiring write lock")? {
            return Ok(eval);
        }
    }

    // Full bootstrap
    let start = std::time::Instant::now();
    let mut eval = create_bootstrap_evaluator_with_features(extra_features)?;
    ensure_startup_compat_variables(&mut eval, &project_root);
    let bootstrap_time = start.elapsed();

    // Save dump for next time.
    if let Some(parent) = dump_path.parent()
        && !parent.exists()
    {
        let _ = std::fs::create_dir_all(parent);
    }
    let dump_start = std::time::Instant::now();
    match pdump::dump_to_file(&eval, dump_path) {
        Ok(()) => {
            tracing::info!(
                "pdump: saved bootstrap state to {} ({:.2?}, bootstrap took {:.2?})",
                dump_path.display(),
                dump_start.elapsed(),
                bootstrap_time,
            );
            let reload_start = std::time::Instant::now();
            match pdump::load_from_dump(dump_path) {
                Ok(mut loaded) => {
                    finalize_or_log(
                        &mut loaded,
                        &project_root,
                        "pdump fresh reload finalize failed",
                    )?;
                    tracing::info!(
                        "pdump: reloaded freshly written bootstrap state from {} ({:.2?})",
                        dump_path.display(),
                        reload_start.elapsed()
                    );
                    return Ok(loaded);
                }
                Err(e) => {
                    tracing::warn!(
                        "pdump: failed to reload freshly written bootstrap image ({e}), using in-memory bootstrap"
                    );
                }
            }
        }
        Err(e) => {
            tracing::warn!("pdump: failed to save ({e}), will bootstrap again next time");
        }
    }

    finalize_or_log(
        &mut eval,
        &project_root,
        "pdump in-memory fallback finalize failed",
    )?;
    Ok(eval)
}

/// Expand `~/` prefix to the HOME directory, matching GNU Emacs's
/// `Fsubstitute_in_file_name` (lread.c:1155).
pub(crate) fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}{}", home.to_string_lossy(), &path[1..]);
        }
    }
    path.to_string()
}

#[cfg(test)]
#[path = "load_test.rs"]
mod tests;
