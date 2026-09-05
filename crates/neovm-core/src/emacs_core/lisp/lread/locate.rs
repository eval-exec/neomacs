//! GNU file lookup: search order, suffixes, predicates, and filename handlers.

use super::expect_lisp_string;
use crate::emacs_core::error::{
    EvalResult, Flow, LispCondition, expect_max_args, expect_min_args, signal,
};
use crate::emacs_core::value::*;
use crate::heap_types::LispString;

/// `(locate-file FILENAME PATH &optional SUFFIXES PREDICATE)`
///
/// Search PATH for FILENAME with each suffix in SUFFIXES.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_locate_file(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("locate-file", &args, 2)?;
    expect_max_args("locate-file", &args, 4)?;
    let filename = expect_lisp_string(&args[0])?;
    let path = parse_path_argument(&args[1])?;
    let suffixes = if args.len() > 2 {
        parse_suffixes_argument(&args[2])?
    } else {
        Vec::new()
    };
    let predicate = match args.get(3).copied() {
        Some(predicate) => Some(normalize_locate_file_public_predicate(eval, predicate)?),
        None => None,
    };
    Ok(
        match locate_file_with_path_and_suffixes(
            eval,
            &filename,
            &path,
            &suffixes,
            predicate.as_ref(),
        )? {
            Some(found) => Value::heap_string(found),
            None => Value::NIL,
        },
    )
}

/// `(locate-file-internal FILENAME PATH SUFFIXES &optional PREDICATE)`
///
/// Internal variant of `locate-file`; currently uses the same lookup behavior.
pub(crate) fn builtin_locate_file_internal(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("locate-file-internal", &args, 2)?;
    expect_max_args("locate-file-internal", &args, 4)?;
    let filename = expect_lisp_string(&args[0])?;
    let path = parse_path_argument(&args[1])?;
    // GNU Emacs: SUFFIXES is optional (nil when omitted)
    let suffixes = if args.len() > 2 {
        parse_suffixes_argument(&args[2])?
    } else {
        Vec::new()
    };
    Ok(
        match locate_file_with_path_and_suffixes(eval, &filename, &path, &suffixes, args.get(3))? {
            Some(found) => Value::heap_string(found),
            None => Value::NIL,
        },
    )
}

fn expect_list(value: &Value) -> Result<Vec<Value>, Flow> {
    list_to_vec(value).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *value],
        )
    })
}

fn parse_path_argument(value: &Value) -> Result<Vec<LispString>, Flow> {
    let mut path = Vec::new();
    let Some(entries) = list_to_vec(value) else {
        return Ok(path);
    };
    for entry in entries {
        match entry.kind() {
            ValueKind::Nil => path.push(LispString::from_unibyte(b".".to_vec())),
            ValueKind::String => path.push(entry.as_lisp_string().expect("checked string").clone()),
            _ => {}
        }
    }
    Ok(path)
}

fn parse_suffixes_argument(value: &Value) -> Result<Vec<LispString>, Flow> {
    let mut suffixes = Vec::new();
    for entry in expect_list(value)? {
        match entry.kind() {
            ValueKind::Nil => suffixes.push(LispString::from_unibyte(Vec::new())),
            ValueKind::String => {
                suffixes.push(entry.as_lisp_string().expect("checked string").clone())
            }
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), entry],
                ));
            }
        }
    }
    Ok(suffixes)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn normalize_locate_file_public_predicate(
    eval: &mut crate::emacs_core::eval::Context,
    predicate: Value,
) -> Result<Value, Flow> {
    if predicate.is_nil() {
        return Ok(predicate);
    }

    let functionp = crate::emacs_core::builtins::builtin_functionp_1(eval, predicate)?.is_truthy();
    if matches!(predicate.kind(), ValueKind::Symbol(_)) && !functionp {
        return Ok(access_mask_from_predicate_symbols(&[predicate]));
    }
    if predicate.is_cons()
        && !functionp
        && let Some(items) = list_to_vec(&predicate)
    {
        return Ok(access_mask_from_predicate_symbols(&items));
    }
    Ok(predicate)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn access_mask_from_predicate_symbols(items: &[Value]) -> Value {
    let mut mask = 0;
    for item in items {
        if eq_value(item, &Value::symbol("executable")) {
            mask |= 1;
        }
        if eq_value(item, &Value::symbol("writable")) {
            mask |= 2;
        }
        if eq_value(item, &Value::symbol("readable")) {
            mask |= 4;
        }
    }
    Value::fixnum(mask)
}

fn locate_file_with_path_and_suffixes(
    eval: &mut crate::emacs_core::eval::Context,
    filename: &LispString,
    path: &[LispString],
    suffixes: &[LispString],
    predicate: Option<&Value>,
) -> Result<Option<LispString>, Flow> {
    let effective_suffixes: Vec<LispString> = if suffixes.is_empty() {
        vec![LispString::from_unibyte(Vec::new())]
    } else {
        suffixes.to_vec()
    };

    let absolute = matches!(filename.as_bytes().first(), Some(b'/') | Some(b'~'));
    if absolute || path.is_empty() {
        let expanded = locate_file_expand_name(eval, filename, None)?;
        for suffix in &effective_suffixes {
            let candidate_lisp = append_lisp_file_name_suffix(&expanded, suffix);
            if candidate_matches_openp(eval, predicate, &candidate_lisp)? {
                return Ok(Some(candidate_lisp));
            }
        }
        return Ok(None);
    }

    for dir in path {
        let base = locate_file_expand_name(eval, filename, Some(dir))?;
        for suffix in &effective_suffixes {
            let candidate_lisp = append_lisp_file_name_suffix(&base, suffix);
            if candidate_matches_openp(eval, predicate, &candidate_lisp)? {
                return Ok(Some(candidate_lisp));
            }
        }
    }

    Ok(None)
}

fn locate_file_expand_name(
    eval: &mut crate::emacs_core::eval::Context,
    name: &LispString,
    default_dir: Option<&LispString>,
) -> Result<LispString, Flow> {
    let mut args = vec![Value::heap_string(name.clone())];
    if let Some(dir) = default_dir {
        args.push(Value::heap_string(dir.clone()));
    }
    let expanded = crate::emacs_core::fileio::builtin_expand_file_name(eval, args)?;
    Ok(expanded
        .as_lisp_string()
        .expect("expand-file-name should return a string")
        .clone())
}

fn append_lisp_file_name_suffix(base: &LispString, suffix: &LispString) -> LispString {
    let mut bytes = base.as_bytes().to_vec();
    bytes.extend_from_slice(suffix.as_bytes());
    if base.is_multibyte() || suffix.is_multibyte() {
        LispString::from_emacs_bytes(bytes)
    } else {
        LispString::from_unibyte(bytes)
    }
}

fn candidate_matches_openp(
    eval: &mut crate::emacs_core::eval::Context,
    predicate: Option<&Value>,
    candidate: &LispString,
) -> Result<bool, Flow> {
    let Some(predicate) = predicate else {
        return readable_non_directory_candidate(eval, candidate);
    };
    if predicate.is_nil() {
        return readable_non_directory_candidate(eval, candidate);
    }
    if predicate.is_t() {
        return readable_non_directory_candidate(eval, candidate);
    }

    if let Some(mask) = predicate.as_fixnum()
        && mask >= 0
    {
        return Ok(integer_access_predicate_matches(eval, candidate, mask));
    }

    let result = eval.funcall_general(*predicate, vec![Value::heap_string(candidate.clone())])?;
    if result.is_nil() {
        return Ok(false);
    }
    if eq_value(&result, &Value::symbol("dir-ok")) {
        return Ok(true);
    }
    Ok(crate::emacs_core::fileio::builtin_file_directory_p(
        eval,
        vec![Value::heap_string(candidate.clone())],
    )?
    .is_nil())
}

fn readable_non_directory_candidate(
    eval: &mut crate::emacs_core::eval::Context,
    candidate: &LispString,
) -> Result<bool, Flow> {
    use crate::emacs_core::fileio::{self, AccessMode, FileEntryKind};

    // GNU openp checks for a file-exists-p handler, then delegates readability
    // to file-readable-p. The ordinary path must use the same namespace as
    // Lisp file primitives, including immutable bundled runtime resources.
    let handler = fileio::find_file_name_handler_lisp_for_eval(
        eval,
        candidate,
        Value::symbol("file-exists-p"),
    );
    if !handler.is_nil() {
        return Ok(!fileio::builtin_file_readable_p(
            eval,
            vec![Value::heap_string(candidate.clone())],
        )?
        .is_nil());
    }
    let path = crate::emacs_core::fileio::lisp_file_name_to_path_buf(candidate);
    let filesystem = eval.editor_file_system();
    Ok(filesystem.access(&path, AccessMode::Read)
        && filesystem
            .metadata(&path, true)
            .is_ok_and(|metadata| metadata.kind != FileEntryKind::Directory))
}

fn integer_access_predicate_matches(
    eval: &crate::emacs_core::eval::Context,
    candidate: &LispString,
    mask: i64,
) -> bool {
    use crate::emacs_core::fileio::{AccessMode, AccessPermissions, FileEntryKind};
    let Some(permissions) = AccessPermissions::from_posix_mask(mask) else {
        return false;
    };
    let path = crate::emacs_core::fileio::lisp_file_name_to_path_buf(candidate);
    let filesystem = eval.editor_file_system();
    filesystem
        .metadata(&path, true)
        .is_ok_and(|metadata| metadata.kind != FileEntryKind::Directory)
        && filesystem.access(&path, AccessMode::Existing(permissions))
}
