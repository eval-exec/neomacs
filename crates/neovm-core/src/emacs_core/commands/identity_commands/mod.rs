//! Host identity Lisp entries that stay in the core command layer.
//!
//! Credential reads are a system service and are not part of the EditHost
//! surface, so under R0 these entries stay in core; the editor layer keeps
//! only buffer/text machinery.

use crate::emacs_core::error::{EvalResult, expect_args};
use crate::emacs_core::runtime_identity::{CredentialScope, process_group_id, process_user_id};
use crate::emacs_core::value::Value;

/// `(user-uid)` — return effective user ID.
pub(crate) fn builtin_user_uid(args: Vec<Value>) -> EvalResult {
    expect_args("user-uid", &args, 0)?;
    Ok(Value::fixnum(i64::from(process_user_id(
        CredentialScope::Effective,
    ))))
}

/// `(user-real-uid)` — return real user ID.
pub(crate) fn builtin_user_real_uid(args: Vec<Value>) -> EvalResult {
    expect_args("user-real-uid", &args, 0)?;
    Ok(Value::fixnum(i64::from(process_user_id(
        CredentialScope::Real,
    ))))
}

/// `(group-gid)` — return the effective group ID.
pub(crate) fn builtin_group_gid(args: Vec<Value>) -> EvalResult {
    expect_args("group-gid", &args, 0)?;
    Ok(Value::fixnum(i64::from(process_group_id(
        CredentialScope::Effective,
    ))))
}

/// `(group-real-gid)` — return the real group ID.
pub(crate) fn builtin_group_real_gid(args: Vec<Value>) -> EvalResult {
    expect_args("group-real-gid", &args, 0)?;
    Ok(Value::fixnum(i64::from(process_group_id(
        CredentialScope::Real,
    ))))
}
