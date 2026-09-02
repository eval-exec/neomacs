//! `num-processors` (GNU `process.c` `Fnum_processors`).
//!
//! GNU defines this DEFUN outside `#ifdef subprocesses`: processor count is a
//! property of the host CPU, not of process support. Both process backends
//! (`process/mod.rs` and `process/portable.rs`) therefore share this file
//! instead of each carrying a copy.

use strum::{EnumString, IntoStaticStr};

use crate::emacs_core::error::{EvalResult, LispCondition, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

/// The optional QUERY argument, mirroring gnulib's `nproc_query`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum NumProcessorsQuery {
    /// `NPROC_ALL`: every configured processor, including unavailable ones.
    All,
    /// `NPROC_CURRENT`: processors available to this process, ignoring
    /// `OMP_NUM_THREADS`.
    Current,
}

impl NumProcessorsQuery {
    fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }
}

/// (num-processors &optional QUERY) -> integer
pub(crate) fn builtin_num_processors(_ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("num-processors"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let query = args.first().and_then(NumProcessorsQuery::from_symbol_value);
    Ok(Value::fixnum(num_processors_count(query) as i64))
}

fn num_processors_count(query: Option<NumProcessorsQuery>) -> u64 {
    match query {
        Some(NumProcessorsQuery::All) => all_processors_count(),
        Some(NumProcessorsQuery::Current) => current_processors_count(),
        None => current_processors_count_overridable(),
    }
}

#[cfg(unix)]
fn current_processors_count_overridable() -> u64 {
    use std::os::unix::ffi::OsStrExt;
    let omp_threads = std::env::var_os("OMP_NUM_THREADS");
    let omp_limit = std::env::var_os("OMP_THREAD_LIMIT");
    current_processors_count_overridable_with_env(
        omp_threads.as_deref().map(OsStrExt::as_bytes),
        omp_limit.as_deref().map(OsStrExt::as_bytes),
        current_processors_count(),
    )
}

#[cfg(not(unix))]
fn current_processors_count_overridable() -> u64 {
    let omp_threads = std::env::var("OMP_NUM_THREADS").ok();
    let omp_limit = std::env::var("OMP_THREAD_LIMIT").ok();
    current_processors_count_overridable_with_env(
        omp_threads.as_deref().map(str::as_bytes),
        omp_limit.as_deref().map(str::as_bytes),
        current_processors_count(),
    )
}

/// gnulib `num_processors (NPROC_CURRENT_OVERRIDABLE)`: `OMP_NUM_THREADS`
/// wins when set, `OMP_THREAD_LIMIT` caps, and zero means "unset".
fn current_processors_count_overridable_with_env(
    omp_threads: Option<&[u8]>,
    omp_limit: Option<&[u8]>,
    current_count: u64,
) -> u64 {
    let omp_threads = omp_threads.and_then(parse_openmp_threads).unwrap_or(0);
    let mut omp_limit = omp_limit.and_then(parse_openmp_threads).unwrap_or(u64::MAX);
    if omp_limit == 0 {
        omp_limit = u64::MAX;
    }

    if omp_threads != 0 {
        return omp_threads.min(omp_limit);
    }

    current_count.min(omp_limit).max(1)
}

/// gnulib `parse_omp_threads`: leading whitespace, an unsigned decimal, then
/// optional whitespace and either end-of-string or a comma-separated tail.
fn parse_openmp_threads(bytes: &[u8]) -> Option<u64> {
    let mut idx = 0;
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    if idx == bytes.len() || !bytes[idx].is_ascii_digit() {
        return None;
    }

    let start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    let value = std::str::from_utf8(&bytes[start..idx])
        .ok()?
        .parse::<u64>()
        .ok()?;

    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }

    if idx == bytes.len() || bytes[idx] == b',' {
        Some(value)
    } else {
        None
    }
}

fn current_processors_count() -> u64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u64)
        .unwrap_or(1)
        .max(1)
}

/// gnulib `num_processors (NPROC_ALL)`: every configured processor, including
/// ones this process may not run on. Only the native host inventory can see
/// offline or affinity-excluded CPUs; without it the honest answer is the
/// available set, which is also gnulib's own fallback.
fn all_processors_count() -> u64 {
    crate::emacs_core::host_info::configured_processor_count()
        .map(std::num::NonZeroU64::get)
        .unwrap_or_else(current_processors_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_symbols_parse_like_gnu() {
        assert_eq!(
            NumProcessorsQuery::from_symbol_value(&Value::symbol("all")),
            Some(NumProcessorsQuery::All)
        );
        assert_eq!(
            NumProcessorsQuery::from_symbol_value(&Value::symbol("current")),
            Some(NumProcessorsQuery::Current)
        );
        assert_eq!(<&'static str>::from(NumProcessorsQuery::All), "all");
        assert_eq!(
            NumProcessorsQuery::from_symbol_value(&Value::symbol("default")),
            None
        );
    }

    #[test]
    fn num_processors_openmp_parser_matches_gnu_rules() {
        assert_eq!(parse_openmp_threads(b"3"), Some(3));
        assert_eq!(parse_openmp_threads(b" 4,8"), Some(4));
        assert_eq!(parse_openmp_threads(b"5 "), Some(5));
        assert_eq!(parse_openmp_threads(b"0"), Some(0));
        assert_eq!(parse_openmp_threads(b""), None);
        assert_eq!(parse_openmp_threads(b"threads=4"), None);
        assert_eq!(parse_openmp_threads(b"4x"), None);

        assert_eq!(
            current_processors_count_overridable_with_env(Some(b"3"), None, 32),
            3
        );
        assert_eq!(
            current_processors_count_overridable_with_env(Some(b"3"), Some(b"2"), 32),
            2
        );
        assert_eq!(
            current_processors_count_overridable_with_env(Some(b" 4,8"), Some(b"0"), 32),
            4
        );
        assert_eq!(
            current_processors_count_overridable_with_env(None, Some(b"1"), 32),
            1
        );
        assert_eq!(
            current_processors_count_overridable_with_env(Some(b"0"), Some(b"5"), 32),
            5
        );
    }

    #[test]
    fn every_query_returns_a_positive_count() {
        let mut ctx = Context::new();
        for args in [
            vec![],
            vec![Value::symbol("all")],
            vec![Value::symbol("current")],
        ] {
            let count = builtin_num_processors(&mut ctx, args)
                .unwrap()
                .as_fixnum()
                .unwrap();
            assert!(count >= 1);
        }
        assert!(
            builtin_num_processors(&mut ctx, vec![Value::NIL, Value::NIL])
                .is_err()
        );
    }
}
