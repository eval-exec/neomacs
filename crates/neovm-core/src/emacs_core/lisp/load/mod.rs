//! File loading and module system (require/provide/load).

use super::builtins::collections::builtin_make_hash_table;
use super::error::{EvalError, Flow, map_flow, signal};
use super::intern::{format_symbol_name_for_diagnostic, intern, resolve_sym};
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
use std::time::SystemTime;

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
    left.schars() == right.schars()
        && left.sbytes() == right.sbytes()
        && left.as_bytes() == right.as_bytes()
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

pub(crate) fn load_path_buf(value: &LispString) -> PathBuf {
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

/// Decode Emacs-extended UTF-8 source straight to a faithful `LispString`
/// (Emacs internal bytes) — issue #131. Non-Unicode source character literals
/// (e.g. `?\xF6\xA0\x87\x8A` -> 0x1A01CA) keep their real codes as extended
/// Emacs bytes instead of the in-Unicode storage sentinels, and the reader's
/// LispString source mode reads them directly. No storage-string round-trip.
///
/// `utf-8-emacs` leaves its end-of-line type UNDECIDED, so the shared decoder
/// detects it -- GNU's `decode_eol` (src/coding.c:6783-6806), the same function
/// `insert-file-contents`, `decode-coding-string` and subprocess output all go
/// through.  This used to be a third, private copy of that detector
/// (`detect_source_eol` / `source_emacs_coding`, deleted with DIVERGENCES.md
/// entry 139): it always resolved the eol itself and handed the decoder a
/// concrete `-unix`/`-dos`/`-mac` name, so a change to the shared rule could not
/// reach `load`.  The two answered identically on every input, the
/// stray-^M-in-a-DOS-file case included, which is why deleting it is a
/// deduplication and not a behaviour change; the equivalence is pinned by
/// `load_source_eol_detection_matches_the_shared_decoder`.
pub(crate) fn decode_emacs_utf8_source_lisp(
    bytes: &[u8],
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> LispString {
    crate::encoding::decode_bytes_to_lisp_string(bytes, "utf-8-emacs", eol_conversion)
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
        if (0xC2..=0xDF).contains(&b)
            && i + 1 < bytes.len()
            && (bytes[i + 1] & 0xC0) == 0x80
            && let Ok(s) = std::str::from_utf8(&bytes[i..i + 2])
        {
            out.push_str(s);
            i += 2;
            continue;
        }
        // Valid 3-byte UTF-8 (E0-EF).
        if (0xE0..=0xEF).contains(&b)
            && i + 2 < bytes.len()
            && (bytes[i + 1] & 0xC0) == 0x80
            && (bytes[i + 2] & 0xC0) == 0x80
            && let Ok(s) = std::str::from_utf8(&bytes[i..i + 3])
        {
            out.push_str(s);
            i += 3;
            continue;
        }
        // Valid standard 4-byte UTF-8 (F0-F4, code point <= 10FFFF).
        if (0xF0..=0xF4).contains(&b)
            && i + 3 < bytes.len()
            && (bytes[i + 1] & 0xC0) == 0x80
            && (bytes[i + 2] & 0xC0) == 0x80
            && (bytes[i + 3] & 0xC0) == 0x80
            && let Ok(s) = std::str::from_utf8(&bytes[i..i + 4])
        {
            out.push_str(s);
            i += 4;
            continue;
        }
        // Extended 4-byte (F5-F7): Emacs-internal code point > U+10FFFF.
        if (0xF5..=0xF7).contains(&b)
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
        if (0xF8..=0xFB).contains(&b)
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
        if (0xFC..=0xFD).contains(&b)
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

fn format_value_for_error(v: &Value) -> String {
    match v.kind() {
        ValueKind::Symbol(sid) => format_symbol_name_for_diagnostic(sid),
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
            ..
        } => {
            let payload = if let Some(raw) = raw_data {
                crate::emacs_core::error::print_value_in_state(eval, raw)
            } else if data.is_empty() {
                "nil".to_string()
            } else {
                crate::emacs_core::error::print_value_in_state(eval, &Value::list(data.clone()))
            };
            format!(
                "({} {})",
                format_symbol_name_for_diagnostic(*symbol),
                payload
            )
        }
        EvalError::UncaughtThrow { tag, value, .. } => format!(
            "(throw {} {})",
            crate::emacs_core::error::print_value_in_state(eval, tag),
            crate::emacs_core::error::print_value_in_state(eval, value),
        ),
        EvalError::Shutdown(request) => format!("(kill-emacs {})", request.exit_code),
    }
}

fn should_log_load_form_error(eval: &super::eval::Context, err: &EvalError) -> bool {
    match err {
        EvalError::Signal { .. } => true,
        // A shutdown is the normal end of a dump run, not a load failure.
        EvalError::Shutdown(_) => false,
        EvalError::UncaughtThrow { tag, .. } => !eval.has_active_catch(tag),
    }
}

fn format_load_form_error(err: &EvalError) -> String {
    match err {
        EvalError::Signal {
            symbol,
            data,
            raw_data,
            ..
        } => {
            let payload = if let Some(raw) = raw_data {
                format_value_for_error(raw)
            } else if data.is_empty() {
                "nil".to_string()
            } else {
                let data_strs: Vec<String> = data.iter().map(format_value_for_error).collect();
                format!("({})", data_strs.join(" "))
            };
            format!(
                "({} {})",
                format_symbol_name_for_diagnostic(*symbol),
                payload
            )
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

fn read_error_for_load(
    path: &Path,
    source: super::reader::ReadSourceObject,
    e: super::value_reader::ReadError,
) -> EvalError {
    match e.kind {
        // GNU `end_of_file_error` takes the datum from the STREAM
        // (`src/lread.c:2121-2132`); the readevalloop that noticed the
        // truncation has no say in it.
        super::value_reader::ReadErrorKind::EndOfFile => EvalError::signal(
            intern("end-of-file"),
            source.error_datum().into_iter().collect(),
            None,
        ),
        super::value_reader::ReadErrorKind::Error => {
            EvalError::signal(intern("error"), vec![Value::string(e.message)], None)
        }
        super::value_reader::ReadErrorKind::InvalidReadSyntax => EvalError::signal(
            intern("error"),
            vec![Value::string(format!(
                "Read error in {}: {} at position {}",
                path.display(),
                e.message,
                e.position
            ))],
            None,
        ),
        super::value_reader::ReadErrorKind::Signal => EvalError::signal(
            intern(e.signal_symbol.as_deref().unwrap_or("error")),
            e.signal_data,
            None,
        ),
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
        return Err(EvalError::signal(
            intern("wrong-number-of-arguments"),
            vec![Value::symbol("defalias"), Value::fixnum(args.len() as i64)],
            None,
        ));
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

#[cfg(test)]
fn source_suffixed_path(base: &Path) -> PathBuf {
    append_load_suffix(base, b".el")
}

#[cfg(test)]
fn compiled_suffixed_path(base: &Path) -> PathBuf {
    append_load_suffix(base, b".elc")
}

fn unsupported_compiled_suffixed_paths(base: &Path) -> [PathBuf; 1] {
    [append_load_suffix(base, b".elc.gz")]
}

/// GNU `load`'s `is_module` test: the primary module suffix OR the platform's
/// secondary one (`.so` on darwin), src/lread.c. Testing only the primary made a
/// `vterm-module.so` found on macOS get read as Lisp instead of dlopen'd
/// (neomacs#193).
fn is_module_path(path: &Path) -> bool {
    super::lread::path_has_module_suffix_for_os(&path.to_string_lossy(), std::env::consts::OS)
}

/// GNU Emacs tries dynamic modules before .elc and .el when modules are
/// supported.  NeoVM matches this by default.
/// Set NEOVM_PREFER_EL=1 to prefer .el source (for debugging).
fn prefer_el_only() -> bool {
    std::env::var("NEOVM_PREFER_EL").is_ok()
}

#[derive(Clone, Copy)]
struct LoadFileAccess<'a> {
    runtime_resources: Option<&'a dyn super::fileio::RuntimeResourceStore>,
}

impl<'a> LoadFileAccess<'a> {
    const fn native() -> Self {
        Self {
            runtime_resources: None,
        }
    }

    const fn with_runtime_resources(
        runtime_resources: Option<&'a dyn super::fileio::RuntimeResourceStore>,
    ) -> Self {
        Self { runtime_resources }
    }

    fn mounted_contents(self, path: &Path) -> Option<&'a [u8]> {
        self.runtime_resources?.file_contents(path)
    }

    fn is_file(self, path: &Path) -> bool {
        self.mounted_contents(path).is_some() || path.is_file()
    }

    fn modified(self, path: &Path) -> Option<std::time::SystemTime> {
        if self.mounted_contents(path).is_some() {
            // Packaged runtime archives are deterministic and carry no live
            // source mtimes. Treat every mounted entry as the same epoch so
            // GNU's suffix order breaks `.elc`/`.el` ties.
            return Some(std::time::UNIX_EPOCH);
        }
        fs::metadata(path).ok()?.modified().ok()
    }

    fn read(self, path: &Path) -> std::io::Result<Vec<u8>> {
        self.mounted_contents(path)
            .map(<[u8]>::to_vec)
            .map(Ok)
            .unwrap_or_else(|| fs::read(path))
    }
}

/// One validated snapshot of GNU's live load-suffix variables.
///
/// Both Lisp-visible `get-load-suffixes` and file resolution consume this
/// plan, so validation, cross-product ordering, and representation handling
/// cannot drift between the two paths.
pub(crate) struct LoadSuffixPlan {
    required: Vec<Vec<u8>>,
    representations: Vec<Vec<u8>>,
}

impl LoadSuffixPlan {
    pub(crate) fn from_obarray(obarray: &super::symbol::Obarray) -> Result<Self, Flow> {
        let suffixes = strict_load_suffix_list(
            obarray.symbol_value("load-suffixes"),
            "load-suffixes",
            Some(default_load_suffixes()),
        )?;
        let representations = strict_load_suffix_list(
            obarray.symbol_value("load-file-rep-suffixes"),
            "load-file-rep-suffixes",
            Some(vec![Vec::new()]),
        )?;
        // GNU only reads `jka-compr-load-suffixes` while considering a
        // non-empty representation of a dynamic module.  Keep the raw Lisp
        // value so irrelevant members (and even an otherwise-invalid value)
        // do not affect ordinary Elisp suffixes.
        let compressed_representations = obarray
            .symbol_value("jka-compr-load-suffixes")
            .copied()
            .unwrap_or(Value::NIL);

        let mut required = Vec::with_capacity(suffixes.len() * representations.len());
        for suffix in &suffixes {
            for representation in &representations {
                // GNU Fget_load_suffixes does not try compressed dynamic
                // modules when the representation comes from jka-compr.
                if !representation.is_empty()
                    && suffix.ends_with(std::env::consts::DLL_SUFFIX.as_bytes())
                    && super::builtins::builtin_member(vec![
                        Value::heap_string(LispString::from_unibyte(representation.clone())),
                        compressed_representations,
                    ])?
                    .is_truthy()
                {
                    continue;
                }
                let mut combined = suffix.clone();
                combined.extend_from_slice(representation);
                required.push(combined);
            }
        }

        Ok(Self {
            required,
            representations,
        })
    }

    pub(crate) fn required_values(&self) -> Vec<Value> {
        self.required
            .iter()
            .cloned()
            .map(|bytes| Value::heap_string(LispString::from_unibyte(bytes)))
            .collect()
    }

    fn search_suffixes(&self, include_representation_fallbacks: bool) -> Vec<Vec<u8>> {
        let mut suffixes = self.required.clone();
        if include_representation_fallbacks {
            suffixes.extend(self.representations.iter().cloned());
        }
        suffixes
    }
}

fn strict_load_suffix_list(
    value: Option<&Value>,
    name: &str,
    unbound_default: Option<Vec<Vec<u8>>>,
) -> Result<Vec<Vec<u8>>, Flow> {
    let Some(value) = value else {
        return Ok(unbound_default.unwrap_or_default());
    };
    let Some(items) = super::value::list_to_vec(value) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *value],
        ));
    };
    items
        .into_iter()
        .map(|item| {
            item.as_lisp_string()
                .map(|suffix| suffix.as_bytes().to_vec())
                .ok_or_else(|| {
                    signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("stringp"), item],
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()
        .inspect_err(|_flow| {
            tracing::debug!(variable = name, "invalid live load suffix list");
        })
}

/// The suffixes a load-path search appends, for one platform.
///
/// This is the same list the Lisp `load-suffixes` carries (see
/// `lread::load_suffixes_startup_values_for_os`), and it MUST stay that list:
/// `locate-file` and `load` consult the Lisp variable while `require` searches
/// through here, so a platform difference between the two silently breaks
/// `require` only. That is exactly how neomacs#193 survived its first fix --
/// `std::env::consts::DLL_SUFFIX` names darwin's PRIMARY suffix (`.dylib`) and
/// knows nothing of its secondary `.so`, so `(require 'vterm-module)` could not
/// find `vterm-module.so` even though `locate-file` and `load` both could.
///
/// Takes the OS by name so every platform's list is testable from any host.
pub(crate) fn default_load_suffixes_for_os(os: &str) -> Vec<Vec<u8>> {
    super::lread::load_suffixes_startup_values_for_os(os)
        .into_iter()
        .map(|suffix| suffix.as_bytes().to_vec())
        .collect()
}

fn default_load_suffixes() -> Vec<Vec<u8>> {
    default_load_suffixes_for_os(std::env::consts::OS)
}

fn pick_suffixed(
    access: LoadFileAccess<'_>,
    base: &Path,
    prefer_newer: bool,
    suffixes: &[Vec<u8>],
) -> Option<PathBuf> {
    let skip_elc = prefer_el_only();
    // Compressed candidates are not loadable (no jka-compr); they are
    // excluded here and surfaced by the explicit unsupported-artifact check
    // in `find_for_base`, preserving the pre-existing error behavior.
    let candidates = suffixes
        .iter()
        .filter(|suffix| !suffix.ends_with(b".gz"))
        .filter(|suffix| !(skip_elc && suffix.ends_with(b".elc")))
        .map(|suffix| append_load_suffix(base, suffix))
        .filter(|path| access.is_file(path));

    if prefer_newer {
        // GNU `openp` replaces its saved candidate only when the next one is
        // STRICTLY newer -- `if (timespec_cmp (mtime, save_mtime) <= 0)
        // emacs_close (fd);` (`src/lread.c:1991`) -- so an exact mtime tie
        // keeps the EARLIER suffix, which is `.elc` before `.el`.
        //
        // `Iterator::max_by_key` documents the opposite: "If several elements
        // are equally maximum, the last element is returned."  That silently
        // inverted the tie towards source.  It was unreachable in practice
        // while nothing turned `load-prefer-newer' on; ledger 202 turns it on
        // for every image build, where a tie is one coarse filesystem
        // timestamp away.
        return candidates
            .filter_map(|path| access.modified(&path).map(|mtime| (mtime, path)))
            .reduce(|best, next| if next.0 > best.0 { next } else { best })
            .map(|(_, path)| path);
    }
    candidates.into_iter().next()
}

fn find_for_base(
    access: LoadFileAccess<'_>,
    base: &Path,
    no_suffix: bool,
    prefer_newer: bool,
    suffixes: &[Vec<u8>],
) -> Option<PathBuf> {
    if no_suffix {
        if access.is_file(base) {
            return Some(base.to_path_buf());
        }
        return None;
    }

    if let Some(suffixed) = pick_suffixed(access, base, prefer_newer, suffixes) {
        return Some(suffixed);
    }

    // Surface unsupported compressed compiled artifacts explicitly.
    unsupported_compiled_suffixed_paths(base)
        .into_iter()
        .find(|compiled| access.is_file(compiled))
}

fn expand_tilde_path_buf(path: &LispString) -> PathBuf {
    #[cfg(unix)]
    {
        let bytes = path.as_bytes();
        if bytes == b"~" {
            if let Some(home) = std::env::var_os("HOME") {
                return PathBuf::from(home);
            }
        } else if bytes.starts_with(b"~/")
            && let Some(home) = std::env::var_os("HOME")
        {
            let mut expanded = PathBuf::from(home);
            expanded.push(std::ffi::OsString::from_vec(bytes[2..].to_vec()));
            return expanded;
        }

        load_path_buf(path)
    }

    #[cfg(not(unix))]
    {
        PathBuf::from(expand_tilde(&load_runtime_string(path)))
    }
}

/// Which candidates a load-path search may accept — GNU `Fload`'s NOSUFFIX and
/// MUST-SUFFIX arguments, which are mutually exclusive and so belong in one
/// value rather than two booleans (`false, false` at a call site said nothing,
/// and `require` silently got the wrong one).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadSuffixRequirement {
    /// GNU NOSUFFIX = t: exactly the name given, no suffix appended.
    ExactNameOnly,
    /// GNU MUST-SUFFIX = t: only suffixed candidates, so an extensionless file
    /// can never match — what `Frequire` passes when it was given no FILENAME
    /// (src/fns.c), and what keeps a `bin/org-capture` shell script from
    /// shadowing `org-capture.el` (neomacs#…, Doom).
    SuffixRequired,
    /// GNU's default: each directory tries the suffixes in order and then the
    /// bare name.
    BareNameAllowed,
}

impl LoadSuffixRequirement {
    /// GNU `Frequire`: MUST-SUFFIX is t exactly when no FILENAME was supplied.
    pub fn for_require(filename_given: bool) -> Self {
        if filename_given {
            Self::BareNameAllowed
        } else {
            Self::SuffixRequired
        }
    }

    fn candidates(
        self,
        obarray: &super::symbol::Obarray,
        file: &LispString,
    ) -> Result<LoadCandidates, Flow> {
        if self == Self::ExactNameOnly {
            // GNU Fload does not even inspect the suffix variables when
            // NOSUFFIX is non-nil.  Besides avoiding needless work, keeping
            // this as a distinct typed state preserves GNU's error ordering
            // when a dynamically bound suffix variable is malformed.
            return Ok(LoadCandidates::ExactName);
        }

        let suffix_required = self == Self::SuffixRequired && effective_must_suffix(file, true);
        let suffixes = LoadSuffixPlan::from_obarray(obarray)?.search_suffixes(!suffix_required);
        Ok(LoadCandidates::AppendSuffixes(suffixes))
    }
}

/// Fully resolved candidate policy for one load-path lookup.
///
/// `ExactName` is intentionally not represented by an empty suffix list:
/// exact loads bypass live suffix-variable validation, whereas an ordinary
/// suffix search can legitimately produce an empty list.
enum LoadCandidates {
    ExactName,
    AppendSuffixes(Vec<Vec<u8>>),
}

/// Search for a file in the load path.
#[tracing::instrument(level = "debug", ret)]
pub fn find_file_in_load_path(name: &str, load_path: &[LispString]) -> Option<PathBuf> {
    find_file_in_load_path_with_requirement(
        name,
        load_path,
        LoadSuffixRequirement::BareNameAllowed,
        false,
    )
}

/// Search `load-path` for NAME under an explicit suffix policy.
pub fn find_file_in_load_path_with_requirement(
    name: &str,
    load_path: &[LispString],
    requirement: LoadSuffixRequirement,
    prefer_newer: bool,
) -> Option<PathBuf> {
    find_file_in_load_path_with_flags(
        name,
        load_path,
        matches!(requirement, LoadSuffixRequirement::ExactNameOnly),
        matches!(requirement, LoadSuffixRequirement::SuffixRequired),
        prefer_newer,
    )
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
    let must_suffix = effective_must_suffix(&name, must_suffix);
    let mut suffixes = default_load_suffixes();
    if !must_suffix {
        suffixes.push(Vec::new());
    }
    find_lisp_file_in_load_path_with_flags(
        LoadFileAccess::native(),
        &name,
        load_path,
        no_suffix,
        prefer_newer,
        &suffixes,
    )
    .map(|found| load_path_buf(&found))
}

/// Resolve FILE against the evaluator's live load state.
///
/// This is the single runtime seam used by both `load` and `require`.
/// Keeping suffix variables, representation suffixes, `load-prefer-newer`,
/// and the exact-vs-suffixed policy together prevents callers from silently
/// falling back to the process-startup defaults.
pub(crate) fn resolve_load_path_file_with_resources(
    obarray: &super::symbol::Obarray,
    buf: Option<&crate::buffer::Buffer>,
    file: &LispString,
    requirement: LoadSuffixRequirement,
    runtime_resources: Option<&dyn super::fileio::RuntimeResourceStore>,
) -> Result<Option<LispString>, Flow> {
    let candidates = requirement.candidates(obarray, file)?;
    let (exact_name_only, suffixes) = match candidates {
        LoadCandidates::ExactName => (true, Vec::new()),
        LoadCandidates::AppendSuffixes(suffixes) => (false, suffixes),
    };
    let prefer_newer = obarray
        .symbol_value("load-prefer-newer")
        .is_some_and(|value| value.is_truthy());
    let load_path = get_load_path(obarray, buf);

    Ok(find_lisp_file_in_load_path_with_flags(
        LoadFileAccess::with_runtime_resources(runtime_resources),
        file,
        &load_path,
        exact_name_only,
        prefer_newer,
        &suffixes,
    ))
}

fn find_lisp_file_in_load_path_with_flags(
    access: LoadFileAccess<'_>,
    name: &LispString,
    load_path: &[LispString],
    no_suffix: bool,
    prefer_newer: bool,
    suffixes: &[Vec<u8>],
) -> Option<LispString> {
    let path = expand_tilde_path_buf(name);
    if path.is_absolute() {
        return find_for_base(access, &path, no_suffix, prefer_newer, suffixes)
            .map(|found| load_path_lisp_string(&found));
    }

    // Emacs searches load-path directory-by-directory; suffix preference
    // is evaluated within each directory.
    for dir in load_path {
        let full = expand_tilde_path_buf(dir).join(load_path_buf(name));
        if let Some(found) = find_for_base(access, &full, no_suffix, prefer_newer, suffixes) {
            return Some(load_path_lisp_string(&found));
        }
    }

    None
}

/// GNU's MUST-SUFFIX means that an otherwise bare FILE must acquire a load
/// suffix.  It is deliberately relaxed for an already-suffixed FILE and for
/// any FILE containing a directory component, where the exact name remains a
/// permitted representation candidate.
fn effective_must_suffix(file: &LispString, requested: bool) -> bool {
    if !requested || has_load_suffix(file) {
        return false;
    }

    load_path_buf(file)
        .parent()
        .is_none_or(|parent| parent.as_os_str().is_empty())
}

/// Extract `load-path` from the evaluator's obarray as Lisp strings, with a
/// `nil` element resolved the way GNU resolves it.
///
/// GNU's `openp` (`src/lread.c:1806-1815`) expands each candidate against the
/// path element and then, if the result is still not absolute, against
/// `BVAR (current_buffer, directory)`; a `nil` element reaches that second step
/// via `Fexpand_file_name (str, Qnil)`, whose `NILP (default_directory)` arm is
/// the same buffer slot (`src/fileio.c:1082-1084`).
///
/// `default-directory` is `DEFVAR_PER_BUFFER` (`src/buffer.c:5392`) -- GNU has
/// **no** global for it -- so reading it from the obarray had no correct case
/// at all: this port installs the name as a `LispFwdType::BufferObj` forwarder
/// whose buffer-less `load()` is `None` by construction, so the read silently
/// produced the `"."` fallback and resolved against the process cwd instead of
/// the buffer. Ledger 196; ledger 191 found the site and sized it.
pub fn get_load_path(
    obarray: &super::symbol::Obarray,
    buf: Option<&crate::buffer::Buffer>,
) -> Vec<LispString> {
    let default_directory = obarray
        .value_in_buffer(buf, "default-directory")
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

#[cfg(test)]
pub(crate) fn plan_load_in_state(
    obarray: &super::symbol::Obarray,
    buf: Option<&crate::buffer::Buffer>,
    file: Value,
    noerror: Option<Value>,
    nosuffix: Option<Value>,
    must_suffix: Option<Value>,
) -> Result<LoadPlan, Flow> {
    plan_load_with_resources(obarray, buf, file, noerror, nosuffix, must_suffix, None)
}

pub(crate) fn plan_load_in_context(
    evaluator: &super::eval::Context,
    file: Value,
    noerror: Option<Value>,
    nosuffix: Option<Value>,
    must_suffix: Option<Value>,
) -> Result<LoadPlan, Flow> {
    plan_load_with_resources(
        &evaluator.obarray,
        evaluator.buffers.current_buffer(),
        file,
        noerror,
        nosuffix,
        must_suffix,
        evaluator.runtime_resource_store(),
    )
}

fn plan_load_with_resources(
    obarray: &super::symbol::Obarray,
    buf: Option<&crate::buffer::Buffer>,
    file: Value,
    noerror: Option<Value>,
    nosuffix: Option<Value>,
    must_suffix: Option<Value>,
    runtime_resources: Option<&dyn super::fileio::RuntimeResourceStore>,
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
    let requirement = if nosuffix.is_some_and(|value| value.is_truthy()) {
        LoadSuffixRequirement::ExactNameOnly
    } else if must_suffix.is_some_and(|value| value.is_truthy()) {
        LoadSuffixRequirement::SuffixRequired
    } else {
        LoadSuffixRequirement::BareNameAllowed
    };

    match resolve_load_path_file_with_resources(
        obarray,
        buf,
        &file,
        requirement,
        runtime_resources,
    )? {
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

#[cfg(test)]
pub(crate) fn resolve_autoload_load_path_in_state(
    obarray: &super::symbol::Obarray,
    buf: Option<&crate::buffer::Buffer>,
    file: &LispString,
) -> Result<PathBuf, Flow> {
    match plan_load_in_state(
        obarray,
        buf,
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

    match plan_load_in_context(
        shared,
        args[0],
        args.get(1).copied(),
        args.get(3).copied(),
        args.get(4).copied(),
    )? {
        LoadPlan::Return(value) => Ok(value),
        LoadPlan::Load { requested, found } => {
            let extra_roots = args.to_vec();
            let options = LoadOptions::from_lisp_flags(
                args.get(1).is_some_and(|v| v.is_truthy()),
                args.get(2).is_some_and(|v| v.is_truthy()),
            );
            let path = load_path_buf(&found);
            let root_scope = shared.save_specpdl_roots();
            for root in &extra_roots {
                shared.push_specpdl_root(*root);
            }
            let result = load_file_with_requested_and_found_options(
                shared, &path, &requested, &found, options,
            )
            .map_err(crate::emacs_core::error::flow_from_eval_error);
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
            .map_err(|err| {
                EvalError::signal(
                    intern("invalid-read-syntax"),
                    vec![Value::string(err.message)],
                    None,
                )
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
    if let Some(val) = eval.obarray().symbol_value("macroexp--pending-eager-loads")
        && val.is_cons()
        && val.cons_car().is_symbol_named("skip")
    {
        return None;
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
    let specpdl_count = eval.specpdl.len();
    let roots = eval.save_specpdl_roots();
    let result = (|| -> Result<Value, EvalError> {
        // GNU Fload first specbinds `lexical-binding' to nil, then assigns the
        // file's cookie/default value before entering readevalloop. The
        // binding preserves a caller's dynamic value across nested loads.
        eval.try_specbind_or_unwind_to(specpdl_count, intern("lexical-binding"), Value::NIL)
            .map_err(map_flow)?;
        eval.try_set_runtime_binding_by_id(
            intern("lexical-binding"),
            Value::bool_val(lexical_binding),
        )
        .map_err(map_flow)?;

        // Mirrors GNU readevalloop's internal-interpreter-environment
        // specbinding. Keep lexenv restoration on the same specpdl scope.
        {
            use super::eval::SpecBinding;
            eval.specpdl.push(SpecBinding::LexicalEnv {
                old_lexenv: eval.lexenv,
            });
        }
        eval.lexenv = if lexical_binding {
            Value::list(vec![Value::T])
        } else {
            Value::NIL
        };

        let load_file_value = Value::heap_string(hist_file_name.clone());
        eval.push_specpdl_root(load_file_value);
        let load_true_file_value = Value::heap_string(found.clone());
        eval.push_specpdl_root(load_true_file_value);
        let current_load_list = Value::cons(load_file_value, Value::NIL);
        eval.push_specpdl_root(current_load_list);
        // GNU Fload specbinds these (`lread.c`) so assignments inside the
        // loaded file affect only the dynamic load context.
        for (symbol, value) in [
            (intern("load-file-name"), load_file_value),
            (intern("load-true-file-name"), load_true_file_value),
            (intern("current-load-list"), current_load_list),
        ] {
            eval.try_specbind_or_unwind_to(specpdl_count, symbol, value)
                .map_err(map_flow)?;
        }
        body(eval)
    })();

    // Restore lexenv via specpdl unbind_to, matching GNU's
    // readevalloop cleanup. This pops the LexicalEnv entry we
    // pushed above, along with lexical-binding/load-file-name/
    // load-true-file-name/current-load-list dynamic bindings,
    // restoring their pre-load values.
    let result = eval
        .unbind_to_with_result(
            specpdl_count,
            result.map_err(crate::emacs_core::error::flow_from_eval_error),
        )
        .map_err(map_flow);
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
    source: super::reader::ReadSourceObject,
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
        let read_result = super::value_reader::read_one_from_encoded_file_bytes(
            content,
            pos,
            &eval.obarray,
            shorthands,
        )
        .map_err(|e| read_error_for_load(path, source, e))?;

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
    source: super::reader::ReadSourceObject,
    shorthands: Option<&ReadSymbolShorthands>,
    macroexpand_fn: Option<Value>,
) -> Result<Value, EvalError> {
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let read_source = super::value_reader::LispReadSource::new(content);

    // Bind `standard-input` to the shared load-read cursor so `(read)` inside a
    // loaded form reads the *next* top-level form from this same source and the
    // loop resumes after it — GNU `readevalloop`'s `specbind (Qstandard_input,
    // readcharfun)` (lread.c).  The cursor reads a heap copy of `content`; its
    // bytes are identical, so the loop's reads of `content` and `(read)`'s
    // reads of the copy advance one shared byte offset (`cursor.pos`).
    let setup_specpdl_base = eval.specpdl.len();
    let content_value = Value::heap_string(content.clone());
    eval.push_specpdl_root(content_value);
    // `(read)` inside a loaded form reads the SAME stream, so GNU raises its
    // end-of-file through the same `end_of_file_error (source)`.
    let eof_source = source.error_datum();
    if let Some(eof_source) = eof_source {
        eval.push_specpdl_root(eof_source);
    }
    eval.try_specbind_or_unwind_to(
        setup_specpdl_base,
        intern("standard-input"),
        eval.load_read_stream_token.as_lisp_value(),
    )
    .map_err(map_flow)?;
    eval.load_read_cursors.push(super::eval::LoadReadCursor {
        source: content_value,
        eof_source,
        pos: 0,
        shorthands: shorthands.cloned(),
    });

    let load_specpdl_base = eval.specpdl.len();

    let loop_result: Result<(), EvalError> = (|| {
        let mut form_idx = 0;
        loop {
            debug_assert_eq!(
                eval.specpdl.len(),
                load_specpdl_base,
                "streaming_readevalloop_lisp_source leaked specpdl entries before {file_name} form {form_idx}",
            );
            // Read at the shared cursor: a `(read)` in the previous form may have
            // advanced it past forms the loop must now skip.
            let pos = eval
                .load_read_cursors
                .last()
                .expect("load-read cursor present during readevalloop")
                .pos;
            // GNU `readevalloop` reads every top-level form through
            // `load-read-function` when it is non-nil:
            //     else if (! NILP (Vload_read_function))
            //       val = calln (Vload_read_function, readcharfun);   (lread.c:2317)
            // Edebug installs itself exactly there (`add-function :around
            // load-read-function #'edebug--read`) to instrument definitions AS
            // they are read, so a loader that never calls the hook makes
            // `edebug-all-defs` silently do nothing.
            //
            // The hook must be the SOLE reader of the form: reading internally
            // first and then calling it re-reads the same text and desynchronizes
            // the shared cursor. Keeping the internal reader while the hook is
            // still the default `read` symbol is observably identical (calling the
            // builtin on this stream yields this form) and leaves the bootstrap
            // path off the Lisp call route.
            // GNU `readevalloop` skips whitespace and comments and breaks on EOF
            // BEFORE invoking the reader, so the read function is called exactly
            // once per form (lread.c). Probing here keeps that call count: the
            // probe never consumes -- the cursor advances only from the reader
            // chosen below.
            let probe = read_source
                .read_one_with_shorthands(pos, &eval.obarray, shorthands)
                .map_err(|e| read_error_for_load(path, source, e))?;
            let Some((probed_form, probed_next)) = probe else {
                break;
            };
            // Then read the form through `load-read-function` when it is non-nil:
            //     else if (! NILP (Vload_read_function))
            //       val = calln (Vload_read_function, readcharfun);   (lread.c:2317)
            // Edebug installs itself exactly there (`add-function :around
            // load-read-function #'edebug--read`) to instrument definitions AS they
            // are read, so a loader that never calls the hook leaves
            // `edebug-all-defs` silently doing nothing. The hook reads from the
            // shared cursor, so ITS advance is the next position -- the probe's is
            // discarded. Staying on the internal reader while the hook is still the
            // default `read` symbol is observably identical and keeps the bootstrap
            // path off the Lisp call route.
            let read_hook = eval
                .obarray
                .symbol_value("load-read-function")
                .copied()
                .filter(|hook| !hook.is_nil() && !hook.is_symbol_named("read"));
            let (form, next_pos) = match read_hook {
                Some(hook) => {
                    let hooked = eval
                        .funcall_general(hook, vec![eval.load_read_stream_token.as_lisp_value()])
                        .map_err(map_flow)?;
                    let advanced = eval
                        .load_read_cursors
                        .last()
                        .map(|cursor| cursor.pos)
                        .unwrap_or(probed_next);
                    (hooked, advanced)
                }
                None => (probed_form, probed_next),
            };
            eval.obarray_mut().materialize_read_symbols(form);

            let form_start = pos;
            if let Some(cursor) = eval.load_read_cursors.last_mut() {
                cursor.pos = next_pos;
            }

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
        Ok(())
    })();

    // Unwind the load-read cursor and the `standard-input` binding + source
    // root regardless of how the loop exited (break, form error, read error).
    eval.load_read_cursors.pop();
    let loop_result = eval
        .unbind_to_with_result(
            setup_specpdl_base,
            loop_result
                .map(|()| Value::NIL)
                .map_err(crate::emacs_core::error::flow_from_eval_error),
        )
        .map(|_| ())
        .map_err(map_flow);

    loop_result?;

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

/// Whether a missing load target signals or returns nil.
///
/// GNU exposes this as `load`'s NOERROR flag.  Give it a domain name instead
/// of passing a positional boolean through the loader: it does not suppress
/// evaluation errors, and confusing it with NOMESSAGE changes user-visible
/// behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MissingFilePolicy {
    Signal,
    ReturnNil,
}

impl MissingFilePolicy {
    pub(crate) const fn from_noerror(noerror: bool) -> Self {
        if noerror {
            Self::ReturnNil
        } else {
            Self::Signal
        }
    }

    const fn as_noerror(self) -> bool {
        matches!(self, Self::ReturnNil)
    }
}

/// Whether a load reports its start and completion.
///
/// Explicit `(load ...)` calls normally report progress.  Autoload and
/// `require` are implicit dependency loads and GNU always suppresses those
/// messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LoadMessagePolicy {
    Report,
    Suppress,
}

impl LoadMessagePolicy {
    const fn from_nomessage(nomessage: bool) -> Self {
        if nomessage {
            Self::Suppress
        } else {
            Self::Report
        }
    }

    const fn as_nomessage(self) -> bool {
        matches!(self, Self::Suppress)
    }
}

/// Caller-selected GNU `load` policy.
///
/// Keeping the two independent axes in a typed value prevents implicit
/// dependency loads from silently inheriting the verbose explicit-load
/// default, which was the source of the Advent-mode divergence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LoadOptions {
    missing_file: MissingFilePolicy,
    messages: LoadMessagePolicy,
}

impl LoadOptions {
    pub(crate) const EXPLICIT: Self = Self {
        missing_file: MissingFilePolicy::Signal,
        messages: LoadMessagePolicy::Report,
    };

    pub(crate) const fn implicit_dependency(missing_file: MissingFilePolicy) -> Self {
        Self {
            missing_file,
            messages: LoadMessagePolicy::Suppress,
        }
    }

    pub(crate) const fn from_lisp_flags(noerror: bool, nomessage: bool) -> Self {
        Self {
            missing_file: MissingFilePolicy::from_noerror(noerror),
            messages: LoadMessagePolicy::from_nomessage(nomessage),
        }
    }
}

/// Load and evaluate a file. Returns the last result.
pub fn load_file(eval: &mut super::eval::Context, path: &Path) -> Result<Value, EvalError> {
    load_file_with_options(eval, path, LoadOptions::EXPLICIT)
}

/// Load and evaluate a file with an explicit caller policy.
pub(crate) fn load_file_with_options(
    eval: &mut super::eval::Context,
    path: &Path,
    options: LoadOptions,
) -> Result<Value, EvalError> {
    let expanded = expand_tilde(&path.to_string_lossy());
    let path = std::path::Path::new(&expanded);
    tracing::info!("load {}", path.display());
    let requested = load_path_lisp_string(path);
    load_file_with_requested_and_found_options(eval, path, &requested, &requested, options)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn load_file_with_found_options(
    eval: &mut super::eval::Context,
    path: &Path,
    found: &LispString,
    options: LoadOptions,
) -> Result<Value, EvalError> {
    load_file_with_requested_and_found_options(eval, path, found, found, options)
}

pub(crate) fn load_file_with_requested_and_found_options(
    eval: &mut super::eval::Context,
    path: &Path,
    requested: &LispString,
    found: &LispString,
    options: LoadOptions,
) -> Result<Value, EvalError> {
    if is_unsupported_compiled_path(path) {
        return Err(EvalError::signal(
            intern("error"),
            vec![Value::string(format!(
                "Loading compressed compiled Elisp artifacts (.elc.gz) is unsupported in neomacs: {}",
                path.display()
            ))],
            None,
        ));
    }

    let user_init_file = intern("user-init-file");
    if eval
        .visible_runtime_variable_value_by_id(user_init_file)
        .map_err(map_flow)?
        == Some(Value::T)
    {
        eval.try_set_runtime_binding_by_id(user_init_file, Value::heap_string(found.clone()))
            .map_err(map_flow)?;
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
        return Err(EvalError::signal(
            intern("error"),
            vec![
                Value::string("Recursive load"),
                Value::cons(found_value, in_progress),
            ],
            None,
        ));
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
    eval.try_specbind_or_unwind_to(spec_entry, intern("load-in-progress"), Value::T)
        .map_err(map_flow)?;

    let result = super::stack_growth::maybe_grow(128 * 1024, 2 * 1024 * 1024, || {
        load_file_body(eval, path, requested, found, options)
    });

    eval.unbind_to_with_result(
        spec_entry,
        result.map_err(crate::emacs_core::error::flow_from_eval_error),
    )
    .map_err(map_flow)
}

/// Whether a `.elc` still implements the `.el` it was compiled from.
///
/// This exists because `is_elc: bool` could not hold the answer.  A bool says
/// "this is bytecode"; it has no room for "...and its source is newer", so the
/// question had nowhere to live and this port never asked it -- while GNU asks
/// it in `Fload` itself (`src/lread.c:1368-1398`) and messages the answer.
/// Making the compiled case CARRY its verdict means no caller can reach the
/// bytecode branch without having been handed the reason it may be wrong.
///
/// GNU's `openp` computes the same comparison one frame earlier and throws it
/// away, which is what the FIXME at `src/lread.c:1367` regrets.  Ledger 202.
#[derive(Clone, Debug, Eq, PartialEq)]
enum CompiledFreshness {
    /// No `.el` sibling, or the `.elc` is at least as new as it: what runs is
    /// what the source says.
    Current,
    /// The `.el` is STRICTLY newer than the `.elc` about to be read.  What
    /// runs is not what the source says, and a test asserting on it is
    /// reporting a build fault as a code fault.
    SourceNewer {
        source: PathBuf,
        source_mtime: SystemTime,
        compiled_mtime: SystemTime,
    },
}

impl CompiledFreshness {
    /// Stat the `.el` beside PATH the way GNU does, by replacing the trailing
    /// `c` (`src/lread.c:1366,1374`).
    fn of_compiled(path: &Path, access: LoadFileAccess<'_>) -> Self {
        let source = path.with_extension("el");
        let (Some(compiled_mtime), Some(source_mtime)) =
            (access.modified(path), access.modified(&source))
        else {
            // GNU only warns when BOTH stats succeed (`result == 0` twice).
            return Self::Current;
        };
        if source_mtime <= compiled_mtime {
            return Self::Current;
        }
        // GNU suppresses the message for bootstrap "compile-first" `.elc`
        // whose timestamps are set to the epoch (`src/lread.c:1387-1390`,
        // bug#58224).
        if compiled_mtime == std::time::UNIX_EPOCH {
            return Self::Current;
        }
        Self::SourceNewer {
            source,
            source_mtime,
            compiled_mtime,
        }
    }
}

/// One `.elc` under a Lisp tree that no longer implements its `.el`.
///
/// Carries both mtimes because "the mtimes" is the entire diagnosis a reader
/// needs, and a caller handed only a path re-derives them or, far more often,
/// does not.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StaleBytecode {
    pub(crate) compiled: PathBuf,
    pub(crate) source: PathBuf,
    pub(crate) compiled_mtime: SystemTime,
    pub(crate) source_mtime: SystemTime,
}

/// Every `.elc` under ROOT whose `.el` is strictly newer.
///
/// Built from the very files and stats [`bootstrap_source_fingerprint`]
/// already collects for its memo key: `collect_bootstrap_source_files` walks
/// exactly the `.el`/`.elc` set, and `bootstrap_source_stats` already reads
/// each one's mtime.  Those rows have existed all along; until ledger 202
/// nothing asked them this question, which is ledger 173's law in a new place
/// -- except that here the rows were written and the predicate was missing.
pub(crate) fn stale_lisp_bytecode(root: &Path) -> Vec<StaleBytecode> {
    let mut files = Vec::new();
    collect_bootstrap_source_files(root, &mut files);
    files.sort();
    let mtimes = bootstrap_source_stats(&files)
        .into_iter()
        .map(|stat| {
            let mtime = SystemTime::UNIX_EPOCH
                + std::time::Duration::new(stat.modified_secs, stat.modified_nanos);
            (stat.path, mtime)
        })
        .collect::<std::collections::HashMap<_, _>>();

    let mut stale = files
        .iter()
        .filter(|path| path.extension().and_then(OsStr::to_str) == Some("elc"))
        .filter_map(|compiled| {
            let source = compiled.with_extension("el");
            let compiled_mtime = *mtimes.get(compiled)?;
            let source_mtime = *mtimes.get(&source)?;
            (source_mtime > compiled_mtime).then(|| StaleBytecode {
                compiled: compiled.clone(),
                source,
                compiled_mtime,
                source_mtime,
            })
        })
        .collect::<Vec<_>>();
    stale.sort_by(|a, b| a.compiled.cmp(&b.compiled));
    stale
}

/// What this process does about bytecode older than its source.
///
/// The two arms are a type rather than a boolean because they answer to two
/// different owners and neither may be reached by accident:
///
/// * A **user's** editor must not refuse to start over one stale `.elc`.  GNU
///   warns and carries on (`src/lread.c:1379`), and so does this.
/// * A **test** asserting on compiled behaviour must never read bytecode that
///   does not implement the checked-out source.  `cargo xtask fresh-build`
///   opens by deleting every generated `.elc` and recompiling, so tests run
///   through it cannot see one; a bare `cargo nextest run` compiles nothing
///   and reads whatever is on disk.  That asymmetry between the two paths is
///   the defect, and refusing is how the second path notices what the first
///   prevents.
///
/// # Which arm a process gets, and why it is not `cfg!(test)`
///
/// Ledger 202 chose the arm with `cfg!(test)`, which Rust sets **only for the
/// crate being compiled as a test**.  `neovm-core` compiled as an ordinary
/// dependency of another crate's test binary therefore saw `false`, so the
/// refusal was live for `neovm-core`'s own 482 in-process tests and dark for
/// the 62 in `neomacs-bin` and the 13 in `neomacs-layout-engine`.  Reproduced
/// in ledger 206 on one deliberately staled tree: `neovm-core`'s
/// `the_gui_terminal_layer_adds_documentation_and_never_rewrites_it` refused in
/// 2.0 s naming the file and both mtimes, while `neomacs-bin`'s
/// `bootstrap_gui_frame_uses_gnu_cursor_and_pointer_color_defaults` passed in
/// 9.4 s, silently, off the same stale tree.
///
/// The proxy was wrong in kind, not in reach: `cfg!(test)` is a fact about a
/// **compilation unit** and the question is about a **process**.  So the
/// default is inverted.  [`Self::for_this_process`] refuses unless this process
/// has said it is a shipped editor, and only `neomacs`'s `main` says so, via
/// [`announce_shipped_editor_process`].  A test binary in any crate -- one
/// written next year, in a crate that does not exist yet -- is covered by
/// construction, because there is nothing for it to opt into.
///
/// Sniffing `NEXTEST` was rejected in ledger 202 and stays rejected: the oracle,
/// TUI and MELPA harnesses spawn `target/release/neomacs` as a **child**, which
/// would inherit the variable and make the shipped editor refuse to start.  An
/// in-process announcement is not inherited by anything.
///
/// # What GNU's scope actually is
///
/// GNU has no sweep.  Its two defences are both per-file and both unconditional:
/// `openp` picks the newer of `foo.el`/`foo.elc` when `load-prefer-newer` is on
/// (`src/lread.c:1988-1998`), and `Fload` messages *"Source file `%s' newer than
/// byte-compiled file"* when it is off (`src/lread.c:1368-1398`).  Neither asks
/// who is running.  What makes GNU's *tree* trustworthy is `make`:
/// `lisp/Makefile.in`'s `%.elc: %.el` rule means a stale `.elc` cannot survive a
/// build, and GNU's test suite depends on that build.
///
/// This port has no `make`, and `cargo nextest run` compiles no Lisp at all.
/// The sweep is therefore the port's stand-in for GNU's Makefile, not for
/// anything in `lread.c` -- which is exactly why the shipped editor must be the
/// one exception (it gets GNU's `Fload` warning, which this port also has) and
/// every harness must be the rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaleBytecodePolicy {
    /// GNU `Fload`: name the file and load it anyway.
    Warn,
    /// Refuse to build an image at all, naming every stale file and its
    /// mtimes.
    Refuse,
}

/// Set to any non-empty value to downgrade [`StaleBytecodePolicy::Refuse`] to
/// [`StaleBytecodePolicy::Warn`] inside the test harness.
///
/// Exists so that a deliberate stale-artifact reproduction -- the one that
/// found this -- can still be run.
pub const ALLOW_STALE_BYTECODE_ENV: &str = "NEOVM_ALLOW_STALE_BYTECODE";

/// Whether this process is a shipped editor.
///
/// A fact about the PROCESS, which is what the question was all along.  It
/// starts false, so anything that has not spoken up is treated as a harness.
static SHIPPED_EDITOR_PROCESS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// **Announce that this process is a shipped editor**, so it warns about stale
/// bytecode the way GNU does instead of refusing to start.
///
/// There is exactly one caller and there must only ever be one: `neomacs`'s
/// `main`, as its first statement.  `crates/neomacs/src/bin/mock-display.rs` and
/// `neomacsclient.rs` do not call it because neither builds an image; the
/// `bootstrap-neomacs` and `neomacs-temacs` role images are byte copies of the
/// `neomacs` binary (`xtask` `copy_executable_role_images`), so they run this
/// same `main` and are covered -- which they must be, since `fresh-build`
/// drives them across a tree whose `.elc` are mid-recompile and therefore
/// transiently stale.
///
/// `stale_bytecode_test::only_the_shipped_editors_main_announces_itself` scans
/// the workspace and fails on a second caller in any crate.
///
/// Ledger 206.
pub fn announce_shipped_editor_process() {
    SHIPPED_EDITOR_PROCESS.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Undo [`announce_shipped_editor_process`], so the test that checks it flips
/// the flag cannot leave the flag flipped for a test that shares its process.
///
/// `cargo nextest run` gives every test its own process and this would not be
/// needed; the project mandates nextest, and this exists so that a `cargo test`
/// run is not silently order-dependent anyway.
#[cfg(test)]
pub(crate) fn withdraw_shipped_editor_announcement() {
    SHIPPED_EDITOR_PROCESS.store(false, std::sync::atomic::Ordering::Relaxed);
}

impl StaleBytecodePolicy {
    /// What THIS PROCESS does about a stale tree.
    ///
    /// `Refuse` unless [`announce_shipped_editor_process`] has been called.
    /// The default is the strict one deliberately: a crate that forgets to
    /// declare anything is a harness, and a harness reading stale bytecode is
    /// the bug this whole family is about.
    pub fn for_this_process() -> Self {
        Self::for_announcement(SHIPPED_EDITOR_PROCESS.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// The decision itself, without the global read, so it can be checked in
    /// both directions without a test mutating process state.
    pub(crate) fn for_announcement(shipped_editor: bool) -> Self {
        if shipped_editor {
            Self::for_user_runtime()
        } else {
            Self::for_test_harness()
        }
    }

    /// What an in-process test bootstrap does: refuse, unless a deliberate
    /// reproduction has asked for the old behaviour.
    pub(crate) fn for_test_harness() -> Self {
        match std::env::var_os(ALLOW_STALE_BYTECODE_ENV) {
            Some(value) if !value.is_empty() => Self::Warn,
            _ => Self::Refuse,
        }
    }

    /// What a shipped editor does: exactly what GNU does.
    pub(crate) fn for_user_runtime() -> Self {
        Self::Warn
    }

    /// The refusal text for STALE, or `None` when this policy never refuses or
    /// there is nothing to refuse over.
    pub(crate) fn report(self, stale: &[StaleBytecode]) -> Option<String> {
        if self == Self::Warn || stale.is_empty() {
            return None;
        }
        let mut report = format!(
            "{} byte-compiled Lisp file{} under lisp/ {} older than the source \
             {} compiled from, so this image would run bytecode that does not \
             implement the checked-out tree.\n\
             Generated .elc files are gitignored: they do not travel with a \
             pull, a merge or a fresh worktree, and `load' prefers a .elc over \
             a newer .el.\n\
             Fix: `cargo xtask fresh-build --release' (it deletes every \
             generated .elc first), or byte-compile the files below.\n\
             Set {ALLOW_STALE_BYTECODE_ENV}=1 to run against them anyway.\n",
            stale.len(),
            if stale.len() == 1 { "" } else { "s" },
            if stale.len() == 1 { "is" } else { "are" },
            if stale.len() == 1 {
                "it was"
            } else {
                "they were"
            },
        );
        for entry in stale.iter().take(STALE_BYTECODE_REPORT_LIMIT) {
            report.push_str(&format!(
                "  {} (compiled {}) is older than {} (modified {})\n",
                entry.compiled.display(),
                format_stale_mtime(entry.compiled_mtime),
                entry.source.display(),
                format_stale_mtime(entry.source_mtime),
            ));
        }
        if stale.len() > STALE_BYTECODE_REPORT_LIMIT {
            report.push_str(&format!(
                "  ... and {} more\n",
                stale.len() - STALE_BYTECODE_REPORT_LIMIT
            ));
        }
        Some(report)
    }
}

/// How many stale files the refusal names before summarising.
///
/// A peer session's tree had 33; naming them all is the point, and a tree with
/// hundreds is a tree nobody needs a full listing of.
const STALE_BYTECODE_REPORT_LIMIT: usize = 40;

/// Seconds since the epoch: enough to compare two files by eye, and free of
/// any timezone or locale dependence that would make the message itself vary.
fn format_stale_mtime(time: SystemTime) -> String {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(since) => format!("{}s", since.as_secs()),
        Err(_) => "before the epoch".to_string(),
    }
}

fn load_file_body(
    eval: &mut super::eval::Context,
    path: &Path,
    requested: &LispString,
    found: &LispString,
    options: LoadOptions,
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
                    Value::bool_val(options.missing_file.as_noerror()),
                    Value::bool_val(options.messages.as_nomessage()),
                ],
            )
            .map_err(crate::emacs_core::error::map_flow);
    }

    // Read raw bytes and decode (with Emacs-extended UTF-8 for .el,
    // or header-skipping for .elc).
    let raw_bytes = LoadFileAccess::with_runtime_resources(eval.runtime_resource_store())
        .read(path)
        .map_err(|e| {
            EvalError::signal(
                intern("file-error"),
                vec![Value::string(format!(
                    "Cannot read file: {}: {}",
                    path.display(),
                    e
                ))],
                None,
            )
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
        // GNU `Fload`, src/lread.c:1368-1398: having opened a `.elc`, stat the
        // `.el` beside it and say so when the source is newer.  GNU skips the
        // comparison when `load-prefer-newer' is on, because then `openp'
        // already chose by mtime and a `.elc` in hand IS the newer file.
        //
        // Nothing in this port emitted this message before ledger 202, so a
        // load that ran superseded bytecode ran it silently -- and four
        // separate sessions read the resulting behaviour difference as a code
        // defect instead of a build fault.
        if !eval
            .visible_variable_value_or_nil("load-prefer-newer")
            .is_truthy()
            && let CompiledFreshness::SourceNewer { source, .. } = CompiledFreshness::of_compiled(
                path,
                LoadFileAccess::with_runtime_resources(eval.runtime_resource_store()),
            )
        {
            let stale_message = format!(
                "Source file `{}' newer than byte-compiled file; using older file",
                source.display()
            );
            let _ = super::builtins::dispatch_builtin(
                eval,
                "message",
                vec![Value::string(stale_message)],
            );
        }
        let content = skip_elc_header(&raw_bytes);
        let lexical_binding = elc_has_lexical_binding(&raw_bytes);
        with_load_context(eval, &hist_file_name, found, lexical_binding, |eval| {
            // GNU `Fload` reads a `.elc` from the file itself, so its reader
            // errors are the `from_file_p` arm: the datum is the
            // `load-true-file-name` this context just bound.
            let source =
                super::reader::ReadSourceObject::LoadFile(Value::heap_string(found.clone()));
            streaming_readevalloop(eval, path, &hist_file_name, &content, source, None, None)
        })
    } else {
        // GNU `Fload` (`src/lread.c`) lets the coding system swallow a leading
        // UTF-8 BOM (U+FEFF); NeoVM's reader does not, so strip it from the raw
        // bytes before decoding (otherwise the reader reads it as a one-character
        // symbol and signals `void-variable`).
        let src_bytes = raw_bytes
            .strip_prefix(&[0xEF, 0xBB, 0xBF])
            .unwrap_or(raw_bytes.as_slice());
        let content = decode_emacs_utf8_source_lisp(src_bytes, eval.eol_conversion());
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
            // Reached only when `load-source-file-function` is nil, i.e. when
            // `Fload` really does read the source file itself: GNU's
            // `from_file_p` arm.
            let source =
                super::reader::ReadSourceObject::LoadFile(Value::heap_string(found.clone()));
            streaming_readevalloop_lisp_source(
                eval,
                path,
                &hist_file_name,
                &content,
                source,
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

/// Run a readevalloop over a source file's text on behalf of a caller that
/// already owns the stream — `eval-buffer` during a `load`.  `source` is that
/// stream, and it decides the datum of every reader error GNU raises through
/// `end_of_file_error` (`src/lread.c:2121-2132`).
pub(crate) fn eval_lisp_source_file_in_context(
    eval: &mut super::eval::Context,
    found: &LispString,
    content: &LispString,
    source: super::reader::ReadSourceObject,
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
        source,
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
                    let sym = format_symbol_name_for_diagnostic(sig.symbol);
                    let data: Vec<String> = sig.data.iter().map(format_value_for_error).collect();
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
// 24: hash tables dump as insertion-ordered (key, value, snapshot) triples.
// 29: pdump v13 — relocation words baked for a planned map base. Pre-bake
// and post-bake binaries must not share cache filenames: dev/test builds
// share a placeholder fingerprint and would ping-pong-overwrite each other's
// images otherwise.
// 30: pdump v14 — BytecodeExtras relayout (object-relative gnu/const spans
// for lazy bytecode stubs).
// 31: pdump v15 — lazy-stub ByteCodeFunction bytes baked into bytecode
// struct spans at dump time (the loader writes nothing there); stub layout
// witness added to the image header.
const BOOTSTRAP_IMAGE_SCHEMA_VERSION: u32 = 31;

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

/// The only two legal reasons to evaluate `loadup.el`.
///
/// A preload-only evaluation deliberately has no command-line surface:
/// `Context` seeds `command-line-processed` to t, so loadup's final
/// `(eval top-level t)` cannot become a user session.  A dump invocation owns
/// its dump mode and build argv together and is always noninteractive.  This
/// closed shape prevents callers from constructing the invalid combination
/// that caused issue #316: no dump mode plus a user-session argv.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadupInvocation {
    PreloadOnly,
    Dump(LoadupDumpInvocation),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadupDumpInvocation {
    mode: LoadupDumpMode,
    command_line_args: Vec<String>,
}

impl LoadupDumpInvocation {
    pub fn new(mode: LoadupDumpMode, command_line_args: Vec<String>) -> Self {
        Self {
            mode,
            command_line_args,
        }
    }

    pub const fn mode(&self) -> LoadupDumpMode {
        self.mode
    }

    pub fn command_line_args(&self) -> &[String] {
        &self.command_line_args
    }
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
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
}

fn is_runtime_root(path: &Path) -> bool {
    path.join("lisp").is_dir() && path.join("etc").is_dir()
}

/// Runtime-root candidates for a running executable, nearest first.
///
/// Ports GNU's dual layout support:
///
/// * The dynamic half is `init_cmdargs` (src/emacs.c): starting from the
///   invocation directory -- following symlinks, which
///   `current_exe().canonicalize()` does for us in one step -- Emacs checks
///   the executable's own directory and then its parent for the tree
///   signature, and records the hit as `installation-directory`;
///   `load_path_default` (src/lread.c) then resets the load-path to
///   `installation-directory/lisp`. GNU's signature is `lib-src` + `etc`;
///   ours is `lisp` + `etc` ([`is_runtime_root`]). This covers the release
///   tarball's flat layout (executable beside `lisp`/`etc`) and the
///   direct-sibling versioned shape (`<root>/bin/neomacs` with
///   `<root>/lisp`), reached through the `~/.local/bin/neomacs` symlink.
///   install.sh stages the versioned tree as `<ver>/bin` +
///   `<ver>/share/neomacs` so that resolver generations before this
///   walk-up existed (v0.0.15 and earlier, which only probe
///   `<grandparent>/share/neomacs`) locate it too.
/// * The configured half is GNU's compile-time `PATH_LOADSEARCH`
///   (`<prefix>/share/emacs/<version>/lisp`); we probe instead of baking a
///   prefix at build time: `<grandparent>/share/neomacs` (deb/rpm/AppImage:
///   `<prefix>/bin/neomacs` + `<prefix>/share/neomacs`) and
///   `<grandparent>/Resources/neomacs` (macOS app bundle:
///   `Contents/MacOS/neomacs` + `Contents/Resources/neomacs`).
fn runtime_root_candidates(exe: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::with_capacity(4);
    if let Some(dir) = exe.parent() {
        candidates.push(dir.to_path_buf());
        if let Some(parent) = dir.parent() {
            candidates.push(parent.to_path_buf());
            candidates.push(parent.join("share/neomacs"));
            candidates.push(parent.join("Resources/neomacs"));
        }
    }
    candidates
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
        && let Ok(resolved) = exe.canonicalize()
    {
        for candidate in runtime_root_candidates(&resolved) {
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

/// `<runtime-root>/etc/images` -- the `data-directory/images` entry of GNU's
/// image file search path (`image_find_image_fd` -> `openp`), backing relative
/// image `:file` resolution. Resolved at RUNTIME via `runtime_project_root()`,
/// never `env!("CARGO_MANIFEST_DIR")`, for the same release-correctness reason
/// as [`charset_map_directory`].
pub fn image_data_directory() -> PathBuf {
    runtime_project_root().join("etc").join("images")
}

/// Delete all but the newest `keep` bootstrap cache images in
/// `dump_path`'s directory (by mtime; the about-to-be-written path is
/// exempt). Best-effort: any error just leaves files behind.
fn prune_bootstrap_cache_generations(dump_path: &Path, keep: usize) {
    let Some(dir) = dump_path.parent() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut images: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            if !(name.starts_with("neovm-bootstrap-") && name.ends_with(".pdump")) {
                return None;
            }
            if path == dump_path {
                return None;
            }
            Some((entry.metadata().ok()?.modified().ok()?, path))
        })
        .collect();
    if images.len() <= keep {
        return;
    }
    images.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in images.into_iter().skip(keep) {
        let _ = std::fs::remove_file(&path);
    }
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

/// How many `(stat key -> content fingerprint)` pairs the memo keeps.
///
/// Each entry is one distinct state of `lisp/`, so a handful covers branch
/// switching without letting the file grow without bound.
const BOOTSTRAP_FINGERPRINT_MEMO_ENTRIES: usize = 16;

const BOOTSTRAP_FINGERPRINT_MEMO_FILE: &str = "neovm-bootstrap-fingerprint-memo-v1";

/// How far a source's mtime must precede a memo entry before that entry is
/// trusted.
///
/// Covers filesystems whose timestamps are coarse (one second is still common)
/// plus room for a write landing either side of the record, so a same-length
/// edit cannot slip through unseen. See
/// [`bootstrap_fingerprint_memo_lookup`].
const BOOTSTRAP_FINGERPRINT_MEMO_RACE_MARGIN: u128 = 2_000_000_000;

/// The cheap facts about one bootstrap source file that decide whether its
/// contents still need to be read.
///
/// Collecting these costs one `stat` per file; reading and hashing the
/// contents costs three orders of magnitude more, so the fingerprint consults
/// the stat facts first and only falls back to the contents when they move.
#[derive(Clone, Debug, PartialEq, Eq)]
struct BootstrapSourceStat {
    path: PathBuf,
    len: u64,
    modified_secs: u64,
    modified_nanos: u32,
}

/// Counts the calls that actually read and hash every source file, so tests can
/// prove the memo is what answers a repeat call rather than inferring it from
/// timing.
#[cfg(test)]
pub(crate) static BOOTSTRAP_CONTENT_FINGERPRINT_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

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

/// The content fingerprint that names this tree's bootstrap image.
///
/// The value is content-addressed: every `.el`/`.elc` file under `lisp/` is
/// read and hashed. That makes it exact but expensive -- the tree is ~3300
/// files and ~130 MB, which costs well over a second in a debug build -- so
/// callers go through [`bootstrap_source_fingerprint`], which memoizes this
/// result against the far cheaper stat key.
fn bootstrap_source_fingerprint(runtime_root: &Path) -> String {
    let mut files = Vec::new();
    collect_bootstrap_source_files(&runtime_root.join("lisp"), &mut files);
    files.sort();

    let stats = bootstrap_source_stats(&files);
    let stat_key = bootstrap_source_stat_key(runtime_root, &stats);
    let newest = bootstrap_source_newest_mtime(&stats);
    let memo_path = bootstrap_cache_dir(runtime_root).join(BOOTSTRAP_FINGERPRINT_MEMO_FILE);
    if let Some(fingerprint) = bootstrap_fingerprint_memo_lookup(&memo_path, &stat_key, newest) {
        return fingerprint;
    }

    let fingerprint = bootstrap_content_fingerprint(runtime_root, &files);
    bootstrap_fingerprint_memo_store(&memo_path, &stat_key, &fingerprint);
    fingerprint
}

/// The most recent modification time across the collected sources, in
/// nanoseconds since the epoch.
fn bootstrap_source_newest_mtime(stats: &[BootstrapSourceStat]) -> u128 {
    stats
        .iter()
        .map(|stat| {
            u128::from(stat.modified_secs) * 1_000_000_000 + u128::from(stat.modified_nanos)
        })
        .max()
        .unwrap_or(0)
}

fn system_time_nanos(time: std::time::SystemTime) -> u128 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

/// Read the cheap stat facts for each already-collected source file.
///
/// A file whose metadata cannot be read contributes zeroed facts, which simply
/// keeps it indistinguishable from a missing file in the stat key; the content
/// hash remains the authority on what the tree actually contains.
fn bootstrap_source_stats(files: &[PathBuf]) -> Vec<BootstrapSourceStat> {
    files
        .iter()
        .map(|path| {
            let (len, modified_secs, modified_nanos) = match fs::metadata(path) {
                Ok(metadata) => {
                    let modified = metadata
                        .modified()
                        .ok()
                        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok());
                    (
                        metadata.len(),
                        modified.map_or(0, |d| d.as_secs()),
                        modified.map_or(0, |d| d.subsec_nanos()),
                    )
                }
                Err(_) => (0, 0, 0),
            };
            BootstrapSourceStat {
                path: path.clone(),
                len,
                modified_secs,
                modified_nanos,
            }
        })
        .collect()
}

/// Hash the identity the memo is keyed on: this executable plus every source
/// file's path, length and modification time.
///
/// This reads no file contents, so it costs one `stat` per file rather than a
/// full pass over the tree. Two trees that agree on every one of these facts
/// are taken to have the same contents -- the same assumption the executable
/// leg of the content fingerprint already makes, and the one every build system
/// relies on. A file edited in place without moving its length or mtime would
/// defeat it, which is why the memo caches a content hash rather than replacing
/// it: the stored value stays exactly what a full content pass would produce.
fn bootstrap_source_stat_key(runtime_root: &Path, stats: &[BootstrapSourceStat]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"neomacs-bootstrap-stat-key-v1\0");
    hash_bootstrap_executable_identity(&mut hasher);
    hasher.update([0xff]);
    for stat in stats {
        let rel = stat.path.strip_prefix(runtime_root).unwrap_or(&stat.path);
        hasher.update(rel.as_os_str().as_encoded_bytes());
        hasher.update([0]);
        hasher.update(stat.len.to_le_bytes());
        hasher.update(stat.modified_secs.to_le_bytes());
        hasher.update(stat.modified_nanos.to_le_bytes());
        hasher.update([0xff]);
    }
    hex_digest_prefix(hasher)
}

/// Fold the running executable's path, size and mtime into a fingerprint.
///
/// Shared by the content fingerprint and the stat key so a rebuilt binary
/// invalidates both together.
fn hash_bootstrap_executable_identity(hasher: &mut Sha256) {
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
}

fn hex_digest_prefix(hasher: Sha256) -> String {
    let digest = hasher.finalize();
    digest[..16]
        .iter()
        .fold(String::with_capacity(32), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Look up a previously computed content fingerprint for this exact stat key.
///
/// An entry is only trusted when every source it describes is comfortably older
/// than the moment it was recorded. An edit that keeps a file's length and
/// lands in the same timestamp tick as the previous write moves nothing in the
/// stat key, so the memo would otherwise answer for the wrong tree -- two
/// same-length writes in quick succession are enough to do it. This is the
/// racily-clean problem Git solves the same way: a record cannot vouch for a
/// file that was written around the time the record was taken, because a
/// further write in that same tick would be invisible. Anything modified within
/// [`BOOTSTRAP_FINGERPRINT_MEMO_RACE_MARGIN`] of the record is therefore
/// refused, and the caller falls back to hashing contents. Sources in a real
/// checkout are minutes or hours old, so the fast path is unaffected; the cost
/// lands only on someone who edits Lisp and runs within the margin.
///
/// Any unreadable or malformed memo is treated as a miss: the memo is a cache,
/// never a source of truth, so a bad one costs time and nothing else.
fn bootstrap_fingerprint_memo_lookup(
    memo_path: &Path,
    stat_key: &str,
    newest_source_nanos: u128,
) -> Option<String> {
    let contents = fs::read_to_string(memo_path).ok()?;
    contents.lines().find_map(|line| {
        let mut fields = line.split('\t');
        let key = fields.next()?;
        let recorded = fields.next()?.parse::<u128>().ok()?;
        let fingerprint = fields.next()?;
        let settled =
            newest_source_nanos.saturating_add(BOOTSTRAP_FINGERPRINT_MEMO_RACE_MARGIN) <= recorded;
        (key == stat_key && settled).then(|| fingerprint.to_string())
    })
}

/// Record `stat_key -> fingerprint`, keeping the newest entries first.
///
/// The write is atomic (write a sibling temporary, then rename) because many
/// test processes race here; a loser simply overwrites with its own equally
/// valid view, and a reader only ever sees a complete file.
fn bootstrap_fingerprint_memo_store(memo_path: &Path, stat_key: &str, fingerprint: &str) {
    let recorded = system_time_nanos(std::time::SystemTime::now());
    let mut lines = vec![format!("{stat_key}\t{recorded}\t{fingerprint}")];
    if let Ok(existing) = fs::read_to_string(memo_path) {
        for line in existing.lines() {
            if line
                .split_once('\t')
                .is_some_and(|(key, _)| key != stat_key)
            {
                lines.push(line.to_string());
            }
        }
    }
    lines.truncate(BOOTSTRAP_FINGERPRINT_MEMO_ENTRIES);
    let mut body = lines.join("\n");
    body.push('\n');

    let Some(parent) = memo_path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(mut temp) = tempfile::NamedTempFile::new_in(parent) else {
        return;
    };
    use std::io::Write as _;
    if temp.write_all(body.as_bytes()).is_err() || temp.flush().is_err() {
        return;
    }
    let _ = temp.persist(memo_path);
}

fn bootstrap_content_fingerprint(runtime_root: &Path, files: &[PathBuf]) -> String {
    #[cfg(test)]
    BOOTSTRAP_CONTENT_FINGERPRINT_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let mut hasher = Sha256::new();
    hasher.update(b"neomacs-bootstrap-source-fingerprint-v2\0");
    hash_bootstrap_executable_identity(&mut hasher);
    hasher.update([0xff]);
    for path in files {
        let rel = path.strip_prefix(runtime_root).unwrap_or(path);
        hasher.update(rel.as_os_str().as_encoded_bytes());
        hasher.update([0]);
        match fs::read(path) {
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

    hex_digest_prefix(hasher)
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

/// The dump-image search, in GNU's order (`load_pdump`, `src/emacs.c:935-1120`).
///
/// GNU walks four rungs and this port walks the same four, with one extra
/// that is GNU's third rung evaluated against the uninstalled `PATH_EXEC`:
///
/// 1. `<executable>.pdmp` beside the binary, same basename with any `.exe`
///    stripped (`src/emacs.c:1024-1041`).
/// 2. the fingerprinted image beside the binary -- GNU's rung 3 in the
///    uninstalled case, where `PATH_EXEC` *is* the executable's directory.
///    Listed separately because [`super::path_exec`] only reports that
///    directory when no installed archlib exists.
/// 3. `PATH_EXEC/neomacs-FINGERPRINT.pdump` (`src/emacs.c:1055-1077`).  GNU
///    hardcodes the product name here "so that the Emacs binary still works
///    if the user copies and renames it", and
///    `RuntimeImageRole::fingerprinted_image_file_name` does the same.
/// 4. `PATH_EXEC/<basename argv0>.pdump` (`src/emacs.c:1096-1120`), so a
///    renamed executable and its renamed dump can share one archlib.
///
/// Note which rung is *absent*: `EMACSPATH` never enters here.  GNU reads the
/// compile-time `PATH_EXEC` constant directly in `load_pdump`
/// (`src/emacs.c:984`) and only consults `EMACSPATH` later, in
/// `init_callproc_1` (`src/callproc.c:1960`), because the dump has to be
/// found before any Lisp -- including any Lisp that could observe the
/// environment -- exists.  Keeping the environment out of the dump search is
/// deliberate parity, not an omission.
///
/// Duplicates are dropped while preserving order, so the uninstalled case
/// (where `PATH_EXEC` is the executable's own directory) still yields the two
/// rungs it always had.
fn runtime_image_candidate_paths_for_executable(
    executable: &Path,
    role: RuntimeImageRole,
) -> Vec<PathBuf> {
    let path_exec = super::path_exec::path_exec_for_executable(executable);
    let candidates = [
        runtime_image_path_for_executable(executable, role),
        fingerprinted_runtime_image_path_for_executable(executable, role),
        path_exec.dir().join(role.fingerprinted_image_file_name()),
        path_exec.dir().join(format!(
            "{}.pdump",
            runtime_image_stem_for_executable(executable, role)
        )),
    ];
    let mut ordered: Vec<PathBuf> = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if !ordered.contains(&candidate) {
            ordered.push(candidate);
        }
    }
    ordered
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

        #[cfg(not(target_family = "wasm"))]
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
        #[cfg(target_family = "wasm")]
        Ok(Self { file })
    }
}

struct BootstrapCacheReadLock {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    file: std::fs::File,
}

impl BootstrapCacheReadLock {
    fn wait(lock_path: &Path) -> Result<Self, String> {
        let file = open_bootstrap_lock_file(lock_path)?;
        #[cfg(not(target_family = "wasm"))]
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
        #[cfg(not(target_family = "wasm"))]
        let _ = self.file.unlock();
    }
}

fn ensure_startup_compat_variables(eval: &mut super::eval::Context, project_root: &Path) {
    let etc_dir = lisp_directory_name_from_host_path(&project_root.join("etc"));
    let source_dir = lisp_directory_name_from_host_path(project_root);
    let temporary_file_directory = lisp_directory_name_from_host_path(&std::env::temp_dir());
    let path_separator = if cfg!(windows) { ";" } else { ":" };
    super::runtime_identity::install(eval);
    let system_configuration = super::builtins_extra::system_configuration_value();
    let system_configuration_options = super::builtins_extra::system_configuration_options_value();
    let system_configuration_features =
        super::builtins_extra::system_configuration_features_value();
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
        ("delayed-warnings-list", Value::NIL),
        ("default-text-properties", Value::NIL),
        ("char-property-alias-alist", Value::NIL),
        ("inhibit-point-motion-hooks", Value::T),
        (
            "text-property-default-nonsticky",
            crate::emacs_core::textprop::default_text_property_nonsticky_alist(),
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
    if items.len() == 2 && items[0].is_symbol_named("quote") {
        return items[1].as_symbol_name().map(|s| s.to_owned());
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
                for key_value in table.key_snapshots().copied() {
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
        let bytes = fs::read(path).map_err(|err| {
            EvalError::signal(
                intern("error"),
                vec![Value::string(format!(
                    "{error_context}: failed reading {}: {err}",
                    path.display()
                ))],
                None,
            )
        })?;
        // Not GNU's `load`: this scans fixed files out of our own `lisp/`
        // tree into a THROWAWAY obarray to work out what the bootstrap must
        // clean up.  It is not a conversion any Lisp binding can be in effect
        // for, and it must read the same source whatever the session holds, so
        // it names its answer instead of asking.
        let source = decode_emacs_utf8_source_lisp(
            &bytes,
            crate::emacs_core::coding::EolConversion::Enabled,
        );
        let obarray = crate::emacs_core::symbol::Obarray::new();
        let forms = crate::emacs_core::value_reader::read_all_lisp_source(&source, &obarray)
            .map_err(|err| {
                EvalError::signal(
                    intern("error"),
                    vec![Value::string(format!(
                        "{error_context}: failed parsing {}: {err}",
                        path.display()
                    ))],
                    None,
                )
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
        let bytes = fs::read(path).map_err(|err| {
            EvalError::signal(
                intern("error"),
                vec![Value::string(format!(
                    "{error_context}: failed reading {}: {err}",
                    path.display()
                ))],
                None,
            )
        })?;
        // Not GNU's `load`: this scans fixed files out of our own `lisp/`
        // tree into a THROWAWAY obarray to work out what the bootstrap must
        // clean up.  It is not a conversion any Lisp binding can be in effect
        // for, and it must read the same source whatever the session holds, so
        // it names its answer instead of asking.
        let source = decode_emacs_utf8_source_lisp(
            &bytes,
            crate::emacs_core::coding::EolConversion::Enabled,
        );
        let obarray = crate::emacs_core::symbol::Obarray::new();
        let forms = crate::emacs_core::value_reader::read_all_lisp_source(&source, &obarray)
            .map_err(|err| {
                EvalError::signal(
                    intern("error"),
                    vec![Value::string(format!(
                        "{error_context}: failed parsing {}: {err}",
                        path.display()
                    ))],
                    None,
                )
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
            let entry_car = entry.cons_car();
            let Some(path) = eval.lisp_string(entry_car) else {
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
    let bytes = fs::read(&ldefs_path).map_err(|err| {
        EvalError::signal(
            intern("error"),
            vec![Value::string(format!(
                "ldefs-boot autoload restore: failed reading {}: {err}",
                ldefs_path.display()
            ))],
            None,
        )
    })?;
    let source = decode_emacs_utf8_source_lisp(&bytes, eval.eol_conversion());
    let forms = crate::emacs_core::value_reader::read_all_lisp_source(&source, &eval.obarray)
        .map_err(|err| {
            EvalError::signal(
                intern("error"),
                vec![Value::string(format!(
                    "ldefs-boot autoload restore: failed parsing {}: {err}",
                    ldefs_path.display()
                ))],
                None,
            )
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
        // The transient load's `defvar` also wrote a `variable-documentation`
        // (FILE . POS) plist entry that this unbind left behind - seven rx.el
        // variables in the dumped image were documented-but-unbound, the
        // exact signature the snarf-documentation last-writer oracle checks.
        // GNU's image has no trace of a file it never loaded.
        let _ = super::builtins::builtin_put(
            eval,
            vec![
                Value::symbol(name),
                Value::symbol("variable-documentation"),
                Value::NIL,
            ],
        );
    }
    for name in &runtime_source_state.face_names {
        super::xfaces::clear_created_lisp_face(name);
        // Keep the canonical existence store (face--new-frame-defaults) in sync
        // with the created-face set so internal-lisp-face-p stops reporting the
        // unloaded face (its fast path reads the table, not the created-set).
        super::xfaces::remove_face_new_frame_defaults_entry(eval, name);
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
        // The transient load's `defvar` also wrote a `variable-documentation`
        // (FILE . POS) plist entry that this unbind left behind - seven rx.el
        // variables in the dumped image were documented-but-unbound, the
        // exact signature the snarf-documentation last-writer oracle checks.
        // GNU's image has no trace of a file it never loaded.
        let _ = super::builtins::builtin_put(
            eval,
            vec![
                Value::symbol(name),
                Value::symbol("variable-documentation"),
                Value::NIL,
            ],
        );
    }
    for name in &runtime_source_state.face_names {
        super::xfaces::clear_created_lisp_face(name);
        // Keep the canonical existence store (face--new-frame-defaults) in sync
        // with the created-face set so internal-lisp-face-p stops reporting the
        // unloaded face (its fast path reads the table, not the created-set).
        super::xfaces::remove_face_new_frame_defaults_entry(eval, name);
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

/// GNU's `init_lread` reset (src/lread.c:5522-5528), plus the two Rust-side
/// stacks that stand in for its one non-Lisp line.
///
/// GNU writes its dump from inside `-l loadup` -- `lisp/loadup.el` calls
/// `dump-emacs-portable` while loadup.el is still being loaded -- so the image
/// is captured with `load-in-progress` at t and `load-file-name` naming
/// loadup.el.  `init_lread` is called from `main` on every startup, dumped
/// image or not (src/emacs.c:2220), and resets that state before any Lisp runs.
///
/// The Lisp rows live in `post_image_init::PostImageInit::Lread`, alongside
/// the other 39 post-image `init_*` call sites; ledger 177 moved them there so
/// that this reset stopped being the only one anybody had written down.
/// `apply_post_image_init` applies them again later in
/// `finalize_cached_bootstrap_eval`; the assignment is idempotent, and doing it
/// here as well keeps the loader state consistent for everything that runs in
/// between.
fn clear_runtime_loader_state(eval: &mut super::eval::Context) {
    // GNU's `Vloads_in_progress = Qnil` (src/lread.c:5528) is a STATIC C
    // variable and not a Lisp variable (src/lread.c:237), so the stack IS its
    // counterpart.  These stacks only describe in-flight bootstrap
    // loads/requires; letting them leak into the runtime surface makes later
    // `require` calls falsely look recursive/already-active.
    eval.require_stack.clear();
    eval.loads_in_progress.clear();
    // The Lisp half of the same fact.  Clearing only the stacks left
    // `load-in-progress` wedged at t for the whole session, which ordinary
    // packages read: `f.el`'s `f-this-file' answers `load-file-name' whenever
    // `load-in-progress' is non-nil, so at top level it returned nil here
    // where GNU returns `(buffer-file-name)'.
    for row in super::post_image_init::PostImageInit::Lread.constants() {
        eval.set_variable(row.name, row.value.value());
    }
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
            let fwd = alloc_buffer_objfwd(
                info.offset.as_u16(),
                info.local_flags_idx,
                info.predicate,
                info.default.to_value(),
            );
            obarray.install_buffer_objfwd(id, fwd);
        }
    }
    super::xfaces::restore_created_faces_from_table(&eval.face_table.face_list());
    clear_runtime_loader_state(eval);
    clear_transient_runtime_features(eval);
    super::environment::install_host_environment_snapshot(eval);
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

    // GNU's post-image `init_*` sequence: every Lisp-visible fact the
    // stretch of `main` below `load_pdump` (src/emacs.c:1436) establishes,
    // walked in `main`'s order.  This is where exec-path, exec-directory,
    // shell-file-name, charset-map-path and font-log are re-derived from the
    // RUNTIME environment, so a CI-built release image cannot bake in the
    // build machine's $PATH or $SHELL, and where the constant resets
    // (`load-in-progress', `gcs-done', `quit-flag', the kboard block, ...)
    // are re-applied.  See emacs_core::post_image_init for the table and the
    // GNU citation behind every row.
    super::post_image_init::apply_post_image_init(eval);

    restore_gnu_stale_preloaded_face_doc_refs(eval);
    // Some GNU C-level variables, notably `data-directory`, become bound only
    // after the initial `Context` bootstrap pass. Re-run the generated DEFVAR
    // adoption at the runtime boundary so late-bound variables receive both
    // their forwarding storage and their declared-special flag. The pass is
    // idempotent for symbols restored intact from a pdump.
    super::defvar_object::adopt(eval.obarray_mut());
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

/// Restore GNU's FINAL-image interpreted-closure environment filter.
///
/// GNU lisp/loadup.el:387-392 sets `internal-make-interpreted-closure-function'
/// to `cconv-make-interpreted-closure' right after loading cconv, guarded by
/// `(compiled-function-p (symbol-function 'cconv-fv))`.  The guard splits
/// GNU's two image flavors: bootstrap-emacs (interpreted cconv -- the filter
/// stays nil because running it interpreted over the whole build would be
/// "excruciatingly slow") and the final emacs (compiled cconv -- the filter
/// is installed; loadup's comment states the setting itself "should be safe
/// ... unconditionally").  Every shipped GNU image, including the oracle
/// Emacs, therefore runs with the filter on: src/eval.c Ffunction (lines
/// 617-623) routes interpreted-closure creation through
/// lisp/emacs-lisp/cconv.el cconv-make-interpreted-closure, which trims the
/// captured environment to the lambda's free variables (cconv-fv).
///
/// A Neomacs tree dumped without byte-compiled Lisp evaluates the loadup
/// guard to nil, so without this repair even the FINAL image would stay in
/// GNU's bootstrap flavor and closures would capture the whole lexical
/// environment -- observably diverging from any real GNU (e.g. gv-get setter
/// closures printing extra env pairs).  Callers apply this only to
/// final-image surfaces: the release binary's Final runtime image and the
/// `apply_runtime_startup_state` "GNU -Q equivalent" test surface.  The
/// Bootstrap image keeps the nil filter exactly like GNU's bootstrap-emacs,
/// so build tooling (loaddefs scrape, unidata generation) is not slowed by
/// interpreted cconv.
fn restore_final_image_interpreted_closure_filter(eval: &mut super::eval::Context) {
    let filter_sym = super::intern::intern("internal-make-interpreted-closure-function");
    let cconv_sym = super::intern::intern("cconv-make-interpreted-closure");
    let filter_is_nil = eval
        .obarray()
        .symbol_value_id(filter_sym)
        .is_none_or(|value| value.is_nil());
    let cconv_bound = eval
        .obarray()
        .symbol_function_id(cconv_sym)
        .is_some_and(|function| !function.is_nil());
    if filter_is_nil && cconv_bound {
        eval.set_variable(
            "internal-make-interpreted-closure-function",
            Value::symbol(cconv_sym),
        );
    }
}

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

/// Apply the runtime startup state that GNU Emacs has after the dumped image
/// is loaded and `normal-top-level` begins to run.
///
/// The dumped bootstrap image intentionally stops before normal interactive
/// startup.  Runtime callers that compare against `emacs --batch -Q` still
/// need the early startup buffer initialization that `startup.el` performs for
/// the `*scratch*` buffer.
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
    // GNU's `init_display' (src/dispnew.c:7413-7422) is the first thing after
    // the pdump is loaded that touches the initial frame's faces:
    //
    //     void init_display (void)
    //     { if (noninteractive) { if (dumped_with_pdumper_p ()) init_faces_initial (); }
    //       else init_display_interactive (); }
    //
    // and `init_faces_initial' (src/dispnew.c:7178) sets the tty default
    // fg/bg pixels and then `call0 (Qtty_set_up_initial_frame_faces)'.  That
    // Lisp (lisp/faces.el:2409) is `(frame-set-background-mode frame t)' plus
    // `(face-set-after-frame-default frame)', and `frame-set-background-mode'
    // (lisp/frame.el:1526) is what COMPUTES `background-mode' and
    // `display-type' -- which is why they are absent for the whole of loadup
    // and why GNU's `faces.el' load never reaches `display-color-cells'.
    //
    // This is that call, at that point.  It must not move earlier: seeding the
    // two parameters in Rust before loadup is DIVERGENCES.md 157's bug.
    eval_startup_forms(
        eval,
        r#"
          (if (fboundp 'tty-set-up-initial-frame-faces)
              (tty-set-up-initial-frame-faces))
        "#,
    )?;
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

    // This surface emulates a GNU -Q FINAL image for runtime consumers, so
    // the interpreted-closure env filter belongs here even when the cached
    // bootstrap image (correctly) left it nil.
    restore_final_image_interpreted_closure_filter(eval);
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

/// The refusal, decided once per process.
///
/// Once, because the sweep walks the whole Lisp tree and both bootstrap entry
/// points have to consult it -- the cached one included, since a pdump built
/// while the escape hatch was set would otherwise let a later run HIT that
/// cache and skip the check entirely.
static STALE_BYTECODE_REFUSAL: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// Unless this process is a shipped editor, refuse to build an image from a
/// stale Lisp tree.
///
/// The whole defect in one sentence: `cargo xtask fresh-build` opens by
/// DELETING every generated `.elc` and recompiling, while `cargo nextest run`
/// compiles nothing and reads whatever is on disk -- so the same tree gives
/// two different answers and only one of them is checked.  This makes the
/// unchecked path notice.
///
/// The verdict is decided once but re-raised on every call: a build that was
/// refused must stay refused however many times it is attempted.
///
/// It is a no-op in a shipped editor, which warns per file at load time the
/// way GNU does (`src/lread.c:1379`) and starts anyway.  Ledger 202 decided
/// that with `cfg!(test)`, which is a fact about a compilation unit and left
/// the refusal dark in every crate that merely links this one; ledger 206
/// made it [`StaleBytecodePolicy::for_this_process`], which is a fact about
/// the process.
///
/// Ledgers 202, 206.
fn refuse_stale_lisp_bytecode(lisp_dir: &Path) {
    let refusal = STALE_BYTECODE_REFUSAL.get_or_init(|| {
        let policy = StaleBytecodePolicy::for_this_process();
        // Decide the policy BEFORE sweeping: `report` discards its argument
        // under `Warn`, but Rust would have evaluated the sweep to build it,
        // so a warning build would still have paid for a walk it cannot use.
        if policy == StaleBytecodePolicy::Warn {
            return None;
        }
        policy.report(&stale_lisp_bytecode(lisp_dir))
    });
    if let Some(report) = refusal {
        panic!("{report}");
    }
}

/// The variables `lisp/loadup.el:110-116` sets while building the image.
///
/// Both are set to `t` there, in ONE `(if dump-mode (progn ...))`:
///
/// ```elisp
/// (if dump-mode
///     (progn
///       ;; To reduce the size of dumped Emacs, we avoid making huge char-tables.
///       (setq inhibit-load-charset-map t)
///       ;; --eval gets handled too late.
///       (defvar load--prefer-newer load-prefer-newer)
///       (setq load-prefer-newer t)))
/// ```
///
/// They are listed together, and driven from this one list, so that the next
/// person to hoist one cannot leave the other behind -- which is exactly what
/// happened: `inhibit-load-charset-map` was hoisted into Rust and
/// `load-prefer-newer` was left behind the dead conditional, where it silently
/// let stale bytecode into every image this port has built.
pub(crate) const LOADUP_DUMP_BRANCH_SEEDED_VARIABLES: &[&str] =
    &["inhibit-load-charset-map", "load-prefer-newer"];

/// Seed the statements `lisp/loadup.el` runs only under `dump-mode`.
///
/// Preload-only construction runs loadup with `dump-mode' nil, while an
/// explicit [`LoadupInvocation::Dump`] follows GNU's string-valued dump path.
/// The preload-only path still needs the pre-dump loading policy without
/// asking Lisp to write an image, so these branch effects are seeded here.
///
/// `load-prefer-newer` is the load-bearing one (GNU Bug#17629): with it on,
/// `openp` chooses the newer of `foo.el`/`foo.elc`, so a `.elc` that no longer
/// implements its `.el` cannot enter the image at all.  Without it, this
/// port's image was built from whatever bytecode happened to be on disk.
///
/// `load--prefer-newer` is seeded too, and deliberately: `loadup.el:492-496`
/// restores the user-visible value from it and is guarded by `boundp` ALONE,
/// not by `dump-mode`.  Seeding the temporary lets GNU's own Lisp perform the
/// restore -- including `(put 'load-prefer-newer 'standard-value ...)` and the
/// `makunbound` -- instead of a Rust copy of it, and lands the image on GNU's
/// measured answer: `emacs -Q --batch` reports `load-prefer-newer` nil,
/// `standard-value` nil, `load--prefer-newer` unbound.
pub(crate) fn seed_loadup_dump_branch_state(eval: &mut super::eval::Context) {
    // loadup.el:115 saves the pre-dump value before :116 overwrites it.
    let previous = eval
        .obarray()
        .symbol_value("load-prefer-newer")
        .copied()
        .unwrap_or(Value::NIL);
    eval.set_variable("load--prefer-newer", previous);
    for name in LOADUP_DUMP_BRANCH_SEEDED_VARIABLES {
        eval.set_variable(name, Value::T);
    }
}

fn apply_loadup_invocation(eval: &mut super::eval::Context, invocation: &LoadupInvocation) {
    let LoadupInvocation::Dump(dump) = invocation else {
        // Context's bootstrap defaults are already the preload-only contract:
        // command-line-processed=t and no user argv to process.  In
        // particular, do not manufacture a command-line surface here; the
        // absence is what prevents loadup's top-level tail from starting a
        // disposable user session.
        eval.set_variable("dump-mode", Value::NIL);
        return;
    };

    let argv = dump
        .command_line_args()
        .iter()
        .cloned()
        .map(Value::string)
        .collect::<Vec<_>>();
    eval.set_variable("command-line-args", Value::list(argv));
    eval.set_variable("command-line-args-left", Value::NIL);
    eval.set_variable("command-line-processed", Value::NIL);
    eval.set_variable("noninteractive", Value::T);
    eval.set_variable("dump-mode", Value::string(dump.mode().as_gnu_string()));
}

pub fn create_bootstrap_evaluator_with_features(
    extra_features: &[&str],
) -> Result<super::eval::Context, EvalError> {
    create_bootstrap_evaluator_for_loadup(extra_features, &LoadupInvocation::PreloadOnly)
}

pub fn create_bootstrap_evaluator_for_loadup(
    extra_features: &[&str],
    invocation: &LoadupInvocation,
) -> Result<super::eval::Context, EvalError> {
    // Discover the runtime root (contains lisp/ and etc/).
    let project_root = runtime_project_root();
    let lisp_dir = project_root.join("lisp");
    assert!(
        lisp_dir.is_dir(),
        "lisp/ directory not found at {}",
        lisp_dir.display()
    );
    refuse_stale_lisp_bytecode(&lisp_dir);
    super::stack_growth::maybe_grow(128 * 1024, 2 * 1024 * 1024, || {
        maybe_trace_bootstrap_step("create_bootstrap_evaluator_with_features: enter");
        let mut eval = super::eval::Context::new();
        maybe_trace_bootstrap_step("create_bootstrap_evaluator_with_features: evaluator-new");
        // Match GNU's initialization order: Lisp startup must inherit the host
        // environment before loadup.el can inspect HOME or any other variable.
        super::environment::install_host_environment_snapshot(&mut eval);
        let bootstrap_features = normalized_bootstrap_features(extra_features);
        for feature in &bootstrap_features {
            let _ = eval.provide_value(Value::symbol(feature), None);
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
        apply_loadup_invocation(&mut eval, invocation);
        maybe_trace_bootstrap_step(
            "create_bootstrap_evaluator_with_features: applied-loadup-invocation",
        );
        eval.set_variable("purify-flag", Value::NIL);
        eval.set_variable("max-lisp-eval-depth", Value::fixnum(1600));
        // PreloadOnly needs loadup.el:110-116's loading policy without taking
        // the dump branch; Dump repeats the same assignments idempotently in
        // Lisp.  Keep BOTH statements together: `inhibit-load-charset-map`
        // used to be seeded alone, while its sibling `load-prefer-newer` is
        // what keeps bytecode older than source out of an image (Bug#17629).
        seed_loadup_dump_branch_state(&mut eval);
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

        // GNU callproc.c plus w32.c: inherited $SHELL, /bin/sh on POSIX,
        // and the private cmdproxy.exe on Windows. Bootstrap and post-image
        // startup share the same typed platform policy.
        super::shell_file_name::install(&mut eval);
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
                if matches!(e, EvalError::Shutdown(_)) {
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

        if matches!(invocation, LoadupInvocation::Dump(_)) && eval.shutdown_request.is_some() {
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
    if let Err(err) = apply_runtime_startup_state(&mut eval) {
        // Render while the evaluator heap is still alive: the caller's
        // cleanup purges the heap, leaving the signal payload unprintable.
        let rendered = format_eval_error_in_state(&mut eval, &err);
        tracing::error!("runtime startup after bootstrap failed: {rendered}");
        return Err(err);
    }
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

    finalize_restored_runtime_image(&mut eval, role, extra_features)?;

    Ok(eval)
}

/// Rebuild the live Rust/host surface of one deserialized runtime image.
///
/// Native pdump files and target-independent portable snapshots share this
/// transition. Deserialization restores Lisp-owned state but cannot itself
/// preserve Rust function pointers, live host environment facts, or loader
/// transients. Product adapters must call this exactly once before attaching
/// the evaluator to an editor session.
pub fn finalize_restored_runtime_image(
    eval: &mut super::eval::Context,
    role: RuntimeImageRole,
    extra_features: &[&str],
) -> Result<(), EvalError> {
    let runtime_root = runtime_project_root();
    finalize_restored_runtime_image_at_root(eval, role, extra_features, &runtime_root)
}

/// Rebuild a deserialized image using an embedding host's explicit runtime
/// resource root.
///
/// Sandboxed native applications cannot participate in executable-relative
/// discovery, and browser embeddings have only a virtual path namespace. This
/// entry point keeps that host fact explicit while preserving the same GNU
/// post-image initialization sequence as desktop startup.
pub fn finalize_restored_runtime_image_at_root(
    eval: &mut super::eval::Context,
    role: RuntimeImageRole,
    extra_features: &[&str],
    runtime_root: &Path,
) -> Result<(), EvalError> {
    if !extra_features.is_empty() {
        let bootstrap_features = normalized_bootstrap_features(extra_features);
        for feature in &bootstrap_features {
            let _ = eval.provide_value(Value::symbol(feature), None);
        }
    }

    activate_runtime_evaluator_at_root(eval, runtime_root, role)?;
    Ok(())
}

/// Cross the post-preload boundary for an evaluator that will be used as a
/// runtime image of `role`.
///
/// This is deliberately shared by pdump loading and the source fallback: the
/// origin of an evaluator must not decide its Lisp-visible runtime contract.
/// [`RuntimeImageRole`] makes the Bootstrap/Final distinction exhaustive, so
/// adding another image role cannot silently inherit the wrong surface.
pub fn activate_runtime_evaluator(
    eval: &mut super::eval::Context,
    role: RuntimeImageRole,
) -> Result<(), EvalError> {
    let project_root = runtime_project_root();
    activate_runtime_evaluator_at_root(eval, &project_root, role)
}

fn activate_runtime_evaluator_at_root(
    eval: &mut super::eval::Context,
    project_root: &Path,
    role: RuntimeImageRole,
) -> Result<(), EvalError> {
    finalize_cached_bootstrap_eval(eval, project_root).map_err(|error| {
        tracing::error!("runtime evaluator activation failed: {error:?}");
        error
    })?;

    match role {
        // GNU bootstrap-emacs keeps the preload construction surface: build
        // tooling still needs the larger interpreted evaluator allowance and
        // the unfiltered interpreted-closure environment.
        RuntimeImageRole::Bootstrap => {}
        // GNU's shipped Emacs starts from `syms_of_eval`'s 1600 limit
        // (`src/eval.c:4405-4413`).  Source loadup raises it to 4200 only while
        // bootstrapping (`lisp/loadup.el:102-106`).  Use `set_variable`, not a
        // host-only cache setter, so the DEFVAR_INT cell and evaluator cache
        // remain one fact.
        RuntimeImageRole::Final => {
            eval.set_variable("max-lisp-eval-depth", Value::fixnum(1600));
            restore_final_image_interpreted_closure_filter(eval);
        }
    }

    Ok(())
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
    EvalError::signal(intern("error"), vec![payload], Some(payload))
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
    // Before the dump is even tried: a pdump written while
    // NEOVM_ALLOW_STALE_BYTECODE was set is named by the same content
    // fingerprint as the stale tree that produced it, so a later run without
    // the escape hatch would HIT that cache and never reach the uncached
    // bootstrap's check.
    refuse_stale_lisp_bytecode(&project_root.join("lisp"));
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

    if dump_path.exists()
        && let Some(eval) = try_load_dump(dump_path, &project_root, "after acquiring write lock")?
    {
        return Ok(eval);
    }

    // Full bootstrap
    let start = std::time::Instant::now();
    let mut eval = create_bootstrap_evaluator_with_features(extra_features)?;
    ensure_startup_compat_variables(&mut eval, &project_root);
    let bootstrap_time = start.elapsed();

    // Prune stale fingerprint generations before writing a new one: the
    // cache is keyed by source fingerprint, so every elisp change strands the
    // previous ~21MB image forever (observed: 617 files / 13GB). Keep the
    // most recent few; anything evicted is regenerable by construction.
    prune_bootstrap_cache_generations(dump_path, 8);
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
    if path.starts_with("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return format!("{}{}", home.to_string_lossy(), &path[1..]);
    }
    path.to_string()
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
