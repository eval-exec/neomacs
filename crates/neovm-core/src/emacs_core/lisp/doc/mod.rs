//! Documentation and help support builtins.
//!
//! Provides:
//! - `documentation` — retrieve docstring from a function
//! - `documentation-property` — retrieve documentation property
//! - `Snarf-documentation` — install every documentation string the DOC file
//!   has onto the symbol it belongs to, once, from `lisp/loadup.el:448`
//!
//! Those last two are GNU's reader and GNU's writer, and they meet on the
//! symbol's plist and nowhere else (`src/doc.c:418`, `src/doc.c:613`).  The
//! writer runs after the C `DEFVAR`s and after every preloaded Lisp file, and
//! its `Fput` is an overwrite -- so a name that is both a `DEFVAR_*` and a
//! preloaded Lisp `defvar` ends up with the C text.  Ledger 182 is the entry
//! that turned this port around to match; before it, the `etc/DOC` stand-in
//! was consulted lazily and only when the plist was empty, which is a fallback
//! and therefore the opposite order.

use super::error::{EvalResult, Flow, signal};
use super::intern::{intern, resolve_sym};
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::expect_args;
use std::fs::File;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_min_max_args(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), Flow> {
    if args.len() < min || args.len() > max {
        Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol(name), Value::fixnum(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins
// ---------------------------------------------------------------------------

/// `(documentation FUNCTION &optional RAW)` -- return the docstring of FUNCTION.
///
/// Looks up FUNCTION in the obarray's function cell. If the function cell
/// holds a `Lambda` (or `Macro`) with a docstring, returns it as a string.
/// Otherwise returns nil.  Unless RAW is non-nil, string results are passed
/// through `substitute-command-keys`, matching GNU Emacs.
pub(crate) fn builtin_documentation(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let raw = args.get(1).is_some_and(|v| v.is_truthy());
    // `bool try_reload = documentation_dynamic_reload;` (`src/doc.c:353`), read
    // ONCE before the `retry:` label so the retry cannot loop.
    let mut try_reload = documentation_dynamic_reload(eval);
    loop {
        let (plan, lisp_directory) = documentation_plan(eval, &args)?;
        let outcome = execute_documentation_plan(
            plan,
            |execution| match execution {
                DocumentationExecution::Eval(value) => eval.eval_value(&value),
                DocumentationExecution::FunctionDoc(function) => {
                    eval.apply(Value::symbol("function-documentation"), vec![function])
                }
            },
            lisp_directory.as_deref(),
        )?;
        match outcome {
            DocumentationOutcome::Value(value) => {
                return finish_documentation_result(value, raw, |value| {
                    maybe_substitute_command_keys(eval, value)
                });
            }
            DocumentationOutcome::Unresolved(reread) => {
                if !std::mem::take(&mut try_reload) {
                    return Ok(Value::NIL);
                }
                perform_doc_reread(eval, reread)?;
            }
        }
    }
}

enum DocumentationPlan {
    Final(Value),
    Eval(Value),
    FunctionDoc(Value),
    /// The reference on the plist named a record that is not there.  GNU calls
    /// `reread_doc_file` and takes its one `goto retry`
    /// (`src/doc.c:371-377`, `:441-447`).
    Unresolved(DocReread),
}

enum DocumentationExecution {
    Eval(Value),
    FunctionDoc(Value),
}

/// What a `documentation`/`documentation-property` lookup produced, with GNU's
/// two nils kept apart.
///
/// `get_doc_string` answers `Qnil` for a reference whose bytes are not a record
/// header (`src/doc.c:254-260`), and GNU does **not** return that nil: it
/// rereads and retries once.  Every other nil on this path -- an absent
/// property, an `Feval` that produced nil -- is a final answer.  Collapsing the
/// two into `Value::NIL` is what made [`documentation_dynamic_reload`] a
/// variable this port declared and never read.
enum DocumentationOutcome {
    Value(Value),
    Unresolved(DocReread),
}

/// GNU's `reread_doc_file` (`src/doc.c:311-317`), which is one `if` over the
/// SHAPE of the reference that failed to resolve:
///
/// ```c
/// static void reread_doc_file (Lisp_Object file)
/// {
///   if (NILP (file)) Fsnarf_documentation (Vdoc_file_name);
///   else save_match_data_load (file, Qt, Qt, Qt, Qnil);
/// }
/// ```
///
/// Its argument is `Fcar_safe (doc)`, so the branch is decided by the reference
/// and by nothing else: nil for a bare fixnum, which points into `etc/DOC`, and
/// the FILE string for a `(FILE . POS)` cons, which points into a `.elc`.
/// Carrying that as a type rather than as a `Lisp_Object` that is sometimes nil
/// is what makes the two arms exhaustive here.
enum DocReread {
    /// `(FILE . POS)`: `save_match_data_load (file, Qt, Qt, Qt, Qnil)`.
    LoadCompiledFile(String),
    /// A bare fixnum: `Fsnarf_documentation (Vdoc_file_name)`.
    SnarfDocFile,
}

/// `documentation-dynamic-reload` (`src/doc.c:720-733`, default `true`).
///
/// Read through the evaluator rather than off the global cell because GNU's is
/// a `DEFVAR_BOOL` and therefore `let`-bindable, and every sweep that reads
/// documentation binds it to nil first (ledger 182 §4: a doc sweep is a WRITE).
fn documentation_dynamic_reload(eval: &super::eval::Context) -> bool {
    eval.eval_symbol_by_id(intern("documentation-dynamic-reload"))
        .is_ok_and(|value| value.is_truthy())
}

/// GNU's `reread_doc_file`, performed.
///
/// Both arms re-run a writer, which is the point: GNU's retry only helps
/// because the reread installs *fresh* references over the stale ones.
/// Measured in GNU 31.0.90 `-Q --batch`, with `documentation-dynamic-reload`
/// nil as the control:
///
/// ```text
/// (put 'case-fold-search 'variable-documentation 7)
///   reload off -> nil,  plist stays 7
///   reload on  -> "Non-nil if searches and matches should ignore case.",
///                 plist is 556387 again
/// ```
fn perform_doc_reread(eval: &mut super::eval::Context, reread: DocReread) -> Result<(), Flow> {
    match reread {
        DocReread::LoadCompiledFile(file) => {
            // `save_match_data_load (file, Qt, Qt, Qt, Qnil)`: NOERROR and
            // NOMESSAGE and NOSUFFIX all t, MUST-SUFFIX nil -- the name on the
            // plist is already the file's own name, suffix included.
            super::builtins::search::with_preserved_match_data(eval, |eval| {
                eval.apply(
                    Value::symbol("load"),
                    vec![Value::string(file), Value::T, Value::T, Value::T],
                )
            })?;
        }
        DocReread::SnarfDocFile => {
            // `Fsnarf_documentation (Vdoc_file_name)`.  This port has exactly
            // one DOC file and it is `var_docs::DocImage`, so the scan is
            // called directly: `internal-doc-file-name` is deliberately nil
            // here (ledger 182 §10 -- assigning "DOC" sends
            // `help-C-file-name', `lisp/help-fns.el:359-373', to
            // `insert-file-contents-literally' on a file that does not exist),
            // and naming a file that is not on disk in order to reach a scan
            // that never opens one would be a spelling, not a source.
            snarf_variable_documentation(&mut eval.obarray);
        }
    }
    Ok(())
}

fn execute_documentation_plan(
    plan: DocumentationPlan,
    mut execute: impl FnMut(DocumentationExecution) -> EvalResult,
    lisp_directory: Option<&str>,
) -> Result<DocumentationOutcome, Flow> {
    match plan {
        DocumentationPlan::Final(value) => Ok(DocumentationOutcome::Value(value)),
        DocumentationPlan::Eval(value) => {
            execute(DocumentationExecution::Eval(value)).map(DocumentationOutcome::Value)
        }
        DocumentationPlan::Unresolved(reread) => Ok(DocumentationOutcome::Unresolved(reread)),
        DocumentationPlan::FunctionDoc(function) => {
            let doc = execute(DocumentationExecution::FunctionDoc(function))?;
            documentation_result_from_raw_doc(lisp_directory, doc)
        }
    }
}

fn finish_documentation_result(
    value: Value,
    raw: bool,
    mut substitute_command_keys: impl FnMut(Value) -> EvalResult,
) -> EvalResult {
    if raw || !value.is_string() {
        Ok(value)
    } else {
        substitute_command_keys(value)
    }
}

fn maybe_substitute_command_keys(eval: &mut super::eval::Context, value: Value) -> EvalResult {
    if eval
        .obarray()
        .symbol_function_id(intern("substitute-command-keys"))
        .is_none()
    {
        return Ok(value);
    }

    eval.eval_value(&Value::list(vec![
        Value::symbol("substitute-command-keys"),
        value,
    ]))
}

fn documentation_plan(
    eval: &super::eval::Context,
    args: &[Value],
) -> Result<(DocumentationPlan, Option<String>), Flow> {
    expect_min_max_args("documentation", args, 1, 2)?;
    let obarray = eval.obarray();
    let lisp_directory = obarray.symbol_value("lisp-directory").and_then(|v| {
        v.as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
    });

    // GNU doc.c:Fdocumentation calls Fget on the original symbol before
    // looking at the function cell.  Keep that exact object identity so
    // uninterned symbols and symbols-with-pos use the same path as `get`.
    if super::builtins::symbols::symbol_id_checked(&args[0], eval.symbols_with_pos_enabled)
        .is_some()
    {
        let prop_key = Value::symbol("function-documentation");
        if let Some(prop) =
            super::builtins::symbols::symbol_property_get(eval, args[0], prop_key)?.1
            && !prop.is_nil()
        {
            let plan = documentation_plan_from_property_value(lisp_directory.as_deref(), prop)?;
            return Ok((plan, lisp_directory));
        }
    }

    let function =
        resolve_documentation_function_value(obarray, args[0], eval.symbols_with_pos_enabled)?;
    let plan = if obarray
        .symbol_function_id(intern("function-documentation"))
        .is_some()
    {
        DocumentationPlan::FunctionDoc(function)
    } else {
        DocumentationPlan::Final(function_doc_or_error(function)?)
    };
    Ok((plan, lisp_directory))
}

fn documentation_result_from_raw_doc(
    lisp_directory: Option<&str>,
    value: Value,
) -> Result<DocumentationOutcome, Flow> {
    if value == Value::fixnum(0) {
        return Ok(DocumentationOutcome::Value(Value::NIL));
    }

    if let Some((file, position)) = compiled_doc_ref(&value) {
        return Ok(
            match load_compiled_doc_string(lisp_directory, &file, position)? {
                DocStringRead::Resolved(text) => DocumentationOutcome::Value(text),
                DocStringRead::Unresolved => {
                    DocumentationOutcome::Unresolved(DocReread::LoadCompiledFile(file))
                }
            },
        );
    }

    Ok(DocumentationOutcome::Value(value))
}

fn resolve_documentation_function_value(
    obarray: &super::symbol::Obarray,
    function: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    let mut resolved =
        if super::builtins::symbols::symbol_id_checked(&function, symbols_with_pos_enabled)
            .is_some()
        {
            let func = super::builtins::symbols::symbol_function_impl_1_checked(
                obarray,
                function,
                symbols_with_pos_enabled,
            )?;
            if func.is_nil() {
                return Err(signal(LispCondition::VoidFunction, vec![function]));
            }
            func
        } else {
            function
        };

    if let Some(alias_symbol) =
        super::builtins::symbols::symbol_id_checked(&resolved, symbols_with_pos_enabled)
        && let Some(indirect) =
            super::builtins::symbols::resolve_indirect_symbol_by_id_in_obarray_checked(
                obarray,
                alias_symbol,
                symbols_with_pos_enabled,
            )
            .map(|(_, value)| value)
    {
        resolved = indirect;
    }

    Ok(resolved)
}

fn function_doc_or_error(func_val: Value) -> EvalResult {
    if let Some(result) = quoted_lambda_documentation(&func_val) {
        return result;
    }
    if let Some(result) = quoted_macro_invalid_designator(&func_val) {
        return result;
    }

    match func_val.kind() {
        ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::Macro) => {
            Ok(func_val
                .closure_docstring()
                .flatten()
                .map_or(Value::NIL, |doc| Value::heap_string(doc.clone())))
        }
        ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr) => {
            // `SnarfedSubr::of` is `Fsnarf_documentation`'s `Ffboundp` clause
            // (`src/doc.c:617-621`) carried by the type: it is the only key
            // `subr_docs::lookup` accepts, and only a subr `Value` can make
            // one.  Both subr representations reach here, and both are subrs.
            let subr =
                super::subr_docs::SnarfedSubr::of(func_val).expect("matched on a subr Value");
            let doc = super::subr_docs::lookup(&subr).unwrap_or("Built-in function.");
            Ok(Value::string(doc))
        }
        ValueKind::String | ValueKind::Veclike(VecLikeType::Vector) => {
            Ok(Value::string("Keyboard macro."))
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            let bc = func_val.get_bytecode_data().unwrap();
            Ok(bc
                .docstring
                .as_ref()
                .map_or(Value::NIL, |doc| Value::heap_string(doc.clone())))
        }
        _other => Err(signal(LispCondition::InvalidFunction, vec![func_val])),
    }
}

fn quoted_lambda_documentation(function: &Value) -> Option<EvalResult> {
    if !function.is_cons() {
        return None;
    };

    let pair_car = function.cons_car();
    let pair_cdr = function.cons_cdr();
    if pair_car.as_symbol_name() != Some("lambda") {
        return None;
    }

    let mut tail = pair_cdr;

    if !tail.is_cons() {
        return Some(Err(signal(LispCondition::InvalidFunction, vec![*function])));
    };
    let _params_and_body_car = tail.cons_car();
    let params_and_body_cdr = tail.cons_cdr();
    tail = params_and_body_cdr;

    match tail.kind() {
        ValueKind::Nil => Some(Ok(Value::NIL)),
        ValueKind::Cons => {
            let body_car = tail.cons_car();
            let _body_cdr = tail.cons_cdr();
            if body_car.is_string() {
                Some(Ok(body_car))
            } else {
                Some(Ok(Value::NIL))
            }
        }
        _other => Some(Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), tail],
        ))),
    }
}

fn quoted_macro_invalid_designator(function: &Value) -> Option<EvalResult> {
    if !function.is_cons() {
        return None;
    };

    let pair_car = function.cons_car();
    let pair_cdr = function.cons_cdr();
    if pair_car.as_symbol_name() != Some("macro") {
        return None;
    }

    let payload = pair_cdr;
    if payload.is_nil() {
        return Some(Err(signal(LispCondition::VoidFunction, vec![Value::NIL])));
    }

    // GNU extracts the docstring from the function part of (macro . fn),
    // rather than signaling invalid-function.
    Some(function_doc_or_error(payload))
}

fn documentation_plan_from_property_value(
    lisp_directory: Option<&str>,
    value: Value,
) -> Result<DocumentationPlan, Flow> {
    if value.is_string() {
        return Ok(DocumentationPlan::Final(value));
    }

    // `if (BASE_EQ (tem, make_fixnum (0))) tem = Qnil;` (`src/doc.c:433-434`,
    // and `:363-365` on the function side) runs BEFORE the `FIXNUMP` test, so
    // the fixnum `0` -- which `make-docfile` cannot emit and which GNU reserves
    // to mean "there is no doc" -- never reaches `get_doc_string` and never
    // triggers a reread.  It falls into GNU's `Feval (Qnil)`, which is nil.
    if value == Value::fixnum(0) {
        return Ok(DocumentationPlan::Final(Value::NIL));
    }

    if let Some((file, position)) = compiled_doc_ref(&value) {
        return Ok(
            match load_compiled_doc_string(lisp_directory, &file, position)? {
                DocStringRead::Resolved(text) => DocumentationPlan::Final(text),
                DocStringRead::Unresolved => {
                    DocumentationPlan::Unresolved(DocReread::LoadCompiledFile(file))
                }
            },
        );
    }

    // A fixnum that the DOC image did not resolve.  GNU does not answer nil
    // here either: `reread_doc_file (Fcar_safe (doc))` with a nil car is
    // `Fsnarf_documentation (Vdoc_file_name)`, and the retry then reads the
    // position the re-scan just installed.
    if value.is_fixnum() {
        return Ok(DocumentationPlan::Unresolved(DocReread::SnarfDocFile));
    }

    Ok(DocumentationPlan::Eval(value))
}

fn compiled_doc_ref(value: &Value) -> Option<(String, i64)> {
    if !value.is_cons() {
        return None;
    };
    let pair_car = value.cons_car();
    let pair_cdr = value.cons_cdr();
    Some((
        pair_car
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))?,
        pair_cdr.as_int()?,
    ))
}

fn resolve_compiled_doc_path(lisp_directory: Option<&str>, file: &str) -> PathBuf {
    let path = Path::new(file);
    if path.is_absolute() {
        return path.to_path_buf();
    }

    if let Some(dir) = lisp_directory {
        return Path::new(dir).join(path);
    }

    path.to_path_buf()
}

fn compiled_doc_prefix_is_valid(prefix: &[u8]) -> bool {
    if prefix.is_empty() {
        return false;
    }

    let mut test = 1_usize;
    if prefix[prefix.len() - test] == 0x1f {
        return true;
    }
    if prefix[prefix.len() - test] != b' ' {
        return false;
    }
    test += 1;
    while prefix.len() >= test && prefix[prefix.len() - test].is_ascii_digit() {
        test += 1;
    }
    if prefix.len() < test || prefix[prefix.len() - test] != b'@' {
        return false;
    }
    test += 1;
    prefix.len() >= test && prefix[prefix.len() - test] == b'#'
}

fn decode_compiled_doc_bytes(bytes: &[u8]) -> EvalResult {
    let mut out = Vec::with_capacity(bytes.len());
    let mut pos = 0_usize;
    while pos < bytes.len() {
        if bytes[pos] != 0x01 {
            out.push(bytes[pos]);
            pos += 1;
            continue;
        }

        pos += 1;
        let Some(&escaped) = bytes.get(pos) else {
            return Err(signal(
                "error",
                vec![Value::string(
                    "Invalid data in documentation file -- dangling ^A escape",
                )],
            ));
        };
        match escaped {
            0x01 => out.push(0x01),
            b'0' => out.push(0x00),
            b'_' => out.push(0x1f),
            other => {
                return Err(signal(
                    "error",
                    vec![Value::string(format!(
                        "Invalid data in documentation file -- ^A followed by code {:03o}",
                        other
                    ))],
                ));
            }
        }
        pos += 1;
    }

    Ok(Value::string(super::load::decode_emacs_utf8(&out)))
}

/// What `get_doc_string` produced (`src/doc.c:105-306`).
///
/// `Unresolved` is its `return Qnil`, and it has exactly two causes: the record
/// has no terminating `^_`, or the bytes before the position are not a record
/// header (`src/doc.c:254-260`).  A file that cannot be opened is NOT one of
/// them -- GNU answers the sentence `Cannot open doc string file "..."` there,
/// which is a value and stops the retry.
enum DocStringRead {
    Resolved(Value),
    Unresolved,
}

fn load_compiled_doc_string(
    lisp_directory: Option<&str>,
    file: &str,
    position: i64,
) -> Result<DocStringRead, Flow> {
    let position = position.unsigned_abs();
    let resolved = resolve_compiled_doc_path(lisp_directory, file);
    let mut handle = match File::open(&resolved) {
        Ok(file_handle) => file_handle,
        Err(err) if matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            return Ok(DocStringRead::Resolved(Value::string(format!(
                "Cannot open doc string file \"{file}\"\n"
            ))));
        }
        Err(err) => {
            return Err(signal(
                LispCondition::FileError,
                vec![
                    Value::string("Read error on documentation file"),
                    Value::string(format!("{}: {}", resolved.display(), err)),
                ],
            ));
        }
    };

    let prefix_len = usize::try_from(position.min(1024)).unwrap_or(1024);
    let start = position.saturating_sub(prefix_len as u64);
    handle.seek(SeekFrom::Start(start)).map_err(|_| {
        signal(
            "error",
            vec![Value::string(format!(
                "Position {position} out of range in doc string file \"{file}\""
            ))],
        )
    })?;

    let offset = prefix_len;
    let mut buffer = Vec::with_capacity(prefix_len + 8192);
    let mut chunk = [0_u8; 8192];
    let end_index = loop {
        let read = handle.read(&mut chunk).map_err(|err| {
            signal(
                LispCondition::FileError,
                vec![
                    Value::string("Read error on documentation file"),
                    Value::string(format!("{}: {}", resolved.display(), err)),
                ],
            )
        })?;
        if read == 0 {
            break None;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > offset
            && let Some(pos) = buffer[offset..].iter().position(|&byte| byte == 0x1f)
        {
            break Some(offset + pos);
        }
    };

    let Some(end_index) = end_index else {
        return Ok(DocStringRead::Unresolved);
    };

    if offset == 0 || buffer.len() < offset || !compiled_doc_prefix_is_valid(&buffer[..offset]) {
        return Ok(DocStringRead::Unresolved);
    }

    decode_compiled_doc_bytes(&buffer[offset..end_index]).map(DocStringRead::Resolved)
}

fn startup_doc_quote_style_display(doc: &str) -> String {
    let mut out = String::with_capacity(doc.len());
    let mut backtick_open = false;
    let mut escaped_backtick_open = false;
    let mut chars = doc.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek().copied() {
                Some('`') => {
                    chars.next();
                    escaped_backtick_open = true;
                    backtick_open = false;
                    continue;
                }
                Some('\'') if escaped_backtick_open => {
                    chars.next();
                    escaped_backtick_open = false;
                    continue;
                }
                _ => {
                    out.push(ch);
                    continue;
                }
            }
        }

        if escaped_backtick_open {
            if ch == '\'' {
                escaped_backtick_open = false;
            } else {
                out.push(ch);
            }
            continue;
        }

        match ch {
            '`' => {
                if backtick_open {
                    out.push('\u{2019}');
                    backtick_open = false;
                } else {
                    out.push('\u{2018}');
                    backtick_open = true;
                }
            }
            '\'' => {
                out.push('\u{2019}');
                if backtick_open {
                    backtick_open = false;
                }
            }
            _ => out.push(ch),
        }
    }

    out
}

fn startup_doc_quote_style_raw(doc: &str) -> String {
    doc.chars()
        .map(|ch| match ch {
            '\u{2018}' => '`',
            '\u{2019}' => '\'',
            _ => ch,
        })
        .collect()
}

/// `(documentation-property SYMBOL PROP &optional RAW)` -- return the
/// documentation property PROP of SYMBOL.
///
/// Context-aware implementation:
/// - validates SYMBOL as a symbol designator (`symbolp`)
/// - returns nil when PROP is not a symbol (matching Emacs `get`-like behavior)
/// - unresolved integer doc offsets return nil
/// - non-integer values are evaluated as Lisp and returned
/// - unless RAW is non-nil, string results are passed through
///   `substitute-command-keys`
pub(crate) fn builtin_documentation_property(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let raw = args.get(2).is_some_and(|v| v.is_truthy());
    // `bool try_reload = documentation_dynamic_reload;` (`src/doc.c:415`), then
    // `retry:` -- the label sits BEFORE `Fget`, because the whole point of the
    // reread is that it rewrites the plist entry the retry then reads.
    let mut try_reload = documentation_dynamic_reload(eval);
    loop {
        let plan = documentation_property_plan(eval, &args)?;
        let outcome = execute_documentation_plan(
            plan,
            |execution| match execution {
                DocumentationExecution::Eval(value) => eval.eval_value(&value),
                DocumentationExecution::FunctionDoc(_) => unreachable!(),
            },
            None,
        )?;
        match outcome {
            DocumentationOutcome::Value(value) => {
                return finish_documentation_result(value, raw, |value| {
                    maybe_substitute_command_keys(eval, value)
                });
            }
            DocumentationOutcome::Unresolved(reread) => {
                // `try_reload = false; goto retry;` -- once, and only once.
                if !std::mem::take(&mut try_reload) {
                    return Ok(Value::NIL);
                }
                perform_doc_reread(eval, reread)?;
            }
        }
    }
}

fn documentation_property_plan(
    eval: &super::eval::Context,
    args: &[Value],
) -> Result<DocumentationPlan, Flow> {
    expect_min_max_args("documentation-property", args, 2, 3)?;
    let obarray = eval.obarray();
    let lisp_directory = obarray.symbol_value("lisp-directory").and_then(|v| {
        v.as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
    });

    let prop = args[1];
    let (symbol_id, mut property_value) =
        super::builtins::symbols::symbol_property_get(eval, args[0], prop)?;
    let prop_is_variable_documentation = eq_value_swp(
        &prop,
        &Value::symbol("variable-documentation"),
        eval.symbols_with_pos_enabled,
    );

    // GNU doc.c:Fdocumentation_property retries variable aliases only for
    // `variable-documentation' when the direct property lookup returned nil.
    if prop_is_variable_documentation
        && property_value.as_ref().is_none_or(|value| value.is_nil())
        && let Some(indirect) = obarray.indirect_variable_id(symbol_id)
        && indirect != symbol_id
    {
        let plist = obarray.symbol_plist_id(indirect);
        property_value =
            crate::emacs_core::plist::plist_get_swp(plist, &prop, eval.symbols_with_pos_enabled);
    }

    let raw = args.get(2).is_some_and(|v| v.is_truthy());

    // GNU reads the plist and nothing else (`src/doc.c:418`), because by the
    // time anyone asks, `Fsnarf_documentation` has already written every doc
    // `etc/DOC` has onto the symbol it belongs to -- over the top of the Lisp
    // `defvar`'s string where both exist (`lisp/loadup.el:251` then `:476`;
    // ledger 182).  There is no name-keyed second source to fall back to: the
    // fixnum on the plist IS the reference into the DOC image, and
    // `DocImage::text_at` is `get_doc_string`.
    match property_value {
        Some(value) => {
            // `src/doc.c:437-438`: `if (FIXNUMP (tem) ...) tem = get_doc_string
            // (tem, 0);`, for whatever PROP names.  Nil for a fixnum that does
            // not point at a record, which is `src/doc.c:254-260`.
            if value.is_fixnum()
                && let Some(text) =
                    super::var_docs::doc_image().text_at(value.as_int().unwrap_or(0))
            {
                // The grave/curly conversion is applied here because a caller
                // may be in a context where `substitute-command-keys'
                // (lisp/help.el) is not reachable.
                let doc = if raw {
                    startup_doc_quote_style_raw(text)
                } else {
                    startup_doc_quote_style_display(text)
                };
                return Ok(DocumentationPlan::Final(Value::string(doc)));
            }
            documentation_plan_from_property_value(lisp_directory.as_deref(), value)
        }
        _ => Ok(DocumentationPlan::Final(Value::NIL)),
    }
}

/// `Fsnarf_documentation`'s scan (`src/doc.c:566-628`), over the `etc/DOC`
/// stand-in.
///
/// Runs once, from `lisp/loadup.el:448`, which is GNU's `lisp/loadup.el:476`
/// -- **after** the C `DEFVAR`s and after every preloaded Lisp file.  That
/// ordering is the whole point: `Fput` is an overwrite, so a name that is both
/// a C `DEFVAR` and a preloaded Lisp `defvar` ends up with the C text, and
/// `indent-tabs-mode` answers `buffer.c`'s sentence rather than
/// `define-minor-mode`'s (`lisp/simple.el:7639`).
///
/// Three clauses of GNU's are kept and one is not:
///
/// - `oblookup (Vobarray, ...)` **does not intern**, and neither does this:
///   `etc/DOC` names variables no build declares, and creating them would put
///   symbols in the obarray that GNU's does not have.
/// - `!NILP (Fboundp (sym))` is [`var_docs::SnarfedVariable::if_bound_in`],
///   ledger 173's gate, and is the only constructor for the key
///   [`var_docs::lookup`] accepts.
/// - `strncmp (end, "\nSKIP", 5)` is enforced at compile time instead, by the
///   `const` assertion in `var_docs`: a regenerated table carrying a `SKIP`
///   placeholder does not build.
/// - `!NILP (Fmemq (sym, delayed_init))` has nothing to select here.  It is a
///   Lisp-level escape hatch for preloaded `custom-initialize-delay`
///   defcustoms (`lisp/custom.el:142-161`), and ledger 173 measured that no C
///   `DEFVAR` name is on `custom-delayed-init-variables`.
pub(crate) fn snarf_variable_documentation(obarray: &mut super::symbol::Obarray) -> usize {
    let mut installed: Vec<(super::intern::SymId, i64)> = Vec::new();
    for (name, _) in super::var_docs::gnu_table::GNU_VAR_DOCS {
        // GNU's `oblookup': a name this obarray does not have is not a symbol,
        // and `if (SYMBOLP (sym))' skips it (`src/doc.c:600').
        let Some(id) = super::intern::lookup_interned(name) else {
            continue;
        };
        if !obarray.is_global_member(id) {
            continue;
        }
        let Some(doc) = super::var_docs::SnarfedVariable::if_bound_in(obarray, id, name)
            .and_then(super::var_docs::lookup)
        else {
            continue;
        };
        installed.push((id, doc.position()));
    }

    let prop = intern("variable-documentation");
    let count = installed.len();
    for (id, position) in installed {
        // `Fput (sym, Qvariable_documentation, make_fixnum (...))'
        // (`src/doc.c:613`) -- an overwrite, not a default.
        let _ = obarray.put_property_id(id, prop, Value::fixnum(position));
    }
    count
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// `(Snarf-documentation FILENAME)` -- install every documentation string the
/// DOC file has onto the symbol it belongs to.
///
/// For the canonical `"DOC"` name this runs [`snarf_variable_documentation`]
/// over the `etc/DOC` stand-in.  Other names keep GNU's error classes for
/// invalid and missing paths, which is what an on-disk DOC file would give.
fn snarf_doc_path_invalid(filename: &str) -> bool {
    if filename.is_empty() {
        return true;
    }

    let mut segments = filename
        .split('/')
        .filter(|segment| !segment.is_empty())
        .peekable();
    if segments.peek().is_none() {
        return true;
    }

    segments.all(|segment| segment == "." || segment == "..")
}

pub(crate) fn builtin_snarf_documentation(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("Snarf-documentation", &args, 1)?;
    let filename = match args[0].as_utf8_str() {
        Some(name) => name,
        None => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };

    if filename == "DOC" {
        snarf_variable_documentation(&mut eval.obarray);
        return Ok(Value::NIL);
    }

    if filename.starts_with("DOC/") {
        return Err(signal(
            LispCondition::FileError,
            vec![
                Value::string("Read error"),
                Value::string(format!("/usr/share/emacs/etc/{filename}")),
            ],
        ));
    }

    if snarf_doc_path_invalid(filename) {
        return Err(signal(
            "error",
            vec![Value::string("DOC file invalid at position 0")],
        ));
    }

    Err(signal(
        LispCondition::FileMissing,
        vec![
            Value::string("Opening doc string file"),
            Value::string("No such file or directory"),
            Value::string(format!("/usr/share/emacs/etc/{filename}")),
        ],
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
