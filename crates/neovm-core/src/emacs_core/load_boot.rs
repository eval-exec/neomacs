//! Core-facade startup/loader construction entry points (stage-4 E3a, Option A).
//!
//! These are the process-level ways to obtain a `Context`: source bootstrap,
//! cached bootstrap, runtime-startup wrapper, pdump image load. They live in
//! core (outside the eval manifest) so the evaluator crate never has to boot
//! its own upper-layer tables. Each entry publishes the one canonical eval
//! hook set through [`super::eval_hook_installers::install_eval_hooks`] and
//! passes the `EvalHooksInstalled` proof to the eval-side implementation,
//! whose constructors require it by value (peer E3a).
//!
//! The heavy policy (dump candidate selection, fallback rules, loadup
//! invocation) stays in eval behind the proof-taking functions in
//! `emacs_core::load` / `emacs_core::pdump`; these facades only boot the
//! hooks and delegate.

use super::error::EvalError;
use super::eval::Context;
use super::eval_hook_installers::install_eval_hooks;
use super::load::{
    BootstrapError, LoadupInvocation, RuntimeImageRole, bootstrap_evaluator_cached,
    bootstrap_evaluator_cached_at_path, bootstrap_evaluator_cached_with_features,
    bootstrap_evaluator_for_loadup, bootstrap_evaluator_with_features, load_runtime_image,
    load_runtime_image_for_executable, runtime_startup_evaluator,
    runtime_startup_evaluator_at_path, runtime_startup_evaluator_cached,
    runtime_startup_evaluator_cached_with_features, runtime_startup_evaluator_with_features,
};
use super::pdump::{DumpError, load_from_dump_with_proof};
use std::path::Path;

/// Create a bootstrap evaluator with no extra features.
pub fn create_bootstrap_evaluator() -> Result<Context, BootstrapError> {
    let proof = install_eval_hooks();
    bootstrap_evaluator_with_features(&[], proof)
}

/// Create a bootstrap evaluator with EXTRA_FEATURES provided.
pub fn create_bootstrap_evaluator_with_features(
    extra_features: &[&str],
) -> Result<Context, BootstrapError> {
    let proof = install_eval_hooks();
    bootstrap_evaluator_with_features(extra_features, proof)
}

/// Create a bootstrap evaluator for the given LOADUP invocation.
pub fn create_bootstrap_evaluator_for_loadup(
    extra_features: &[&str],
    invocation: &LoadupInvocation,
) -> Result<Context, BootstrapError> {
    let proof = install_eval_hooks();
    bootstrap_evaluator_for_loadup(extra_features, invocation, proof)
}

/// Create a bootstrap evaluator, using a pdump cache file if available.
pub fn create_bootstrap_evaluator_cached() -> Result<Context, BootstrapError> {
    let proof = install_eval_hooks();
    bootstrap_evaluator_cached(proof)
}

/// Cached bootstrap with EXTRA_FEATURES provided.
pub fn create_bootstrap_evaluator_cached_with_features(
    extra_features: &[&str],
) -> Result<Context, BootstrapError> {
    let proof = install_eval_hooks();
    bootstrap_evaluator_cached_with_features(extra_features, proof)
}

/// Cached bootstrap at an explicit dump path.
pub fn create_bootstrap_evaluator_cached_at_path(
    extra_features: &[&str],
    dump_path: &Path,
) -> Result<Context, BootstrapError> {
    let proof = install_eval_hooks();
    bootstrap_evaluator_cached_at_path(extra_features, dump_path, proof)
}

/// Create a runtime-startup evaluator (cached bootstrap + startup state).
pub fn create_runtime_startup_evaluator() -> Result<Context, BootstrapError> {
    let proof = install_eval_hooks();
    runtime_startup_evaluator(proof)
}

/// Runtime-startup evaluator with EXTRA_FEATURES provided.
pub fn create_runtime_startup_evaluator_with_features(
    extra_features: &[&str],
) -> Result<Context, BootstrapError> {
    let proof = install_eval_hooks();
    runtime_startup_evaluator_with_features(extra_features, proof)
}

/// Cached runtime-startup evaluator.
pub fn create_runtime_startup_evaluator_cached() -> Result<Context, BootstrapError> {
    let proof = install_eval_hooks();
    runtime_startup_evaluator_cached(proof)
}

/// Cached runtime-startup evaluator with EXTRA_FEATURES provided.
pub fn create_runtime_startup_evaluator_cached_with_features(
    extra_features: &[&str],
) -> Result<Context, BootstrapError> {
    let proof = install_eval_hooks();
    runtime_startup_evaluator_cached_with_features(extra_features, proof)
}

/// Runtime-startup evaluator at an explicit dump path.
pub fn create_runtime_startup_evaluator_at_path(
    extra_features: &[&str],
    dump_path: &Path,
) -> Result<Context, BootstrapError> {
    let proof = install_eval_hooks();
    runtime_startup_evaluator_at_path(extra_features, dump_path, proof)
}

/// Load a runtime image for ROLE through the implementation policy.
pub fn load_runtime_image_with_features(
    role: RuntimeImageRole,
    extra_features: &[&str],
    dump_path: Option<&Path>,
) -> Result<Context, EvalError> {
    let proof = install_eval_hooks();
    load_runtime_image(role, extra_features, dump_path, proof)
}

/// Load a runtime image for ROLE using an explicit EXECUTABLE.
pub fn load_runtime_image_with_features_for_executable(
    role: RuntimeImageRole,
    extra_features: &[&str],
    dump_path: Option<&Path>,
    executable: &Path,
) -> Result<Context, EvalError> {
    let proof = install_eval_hooks();
    load_runtime_image_for_executable(role, extra_features, dump_path, executable, proof)
}

/// Load evaluator state from a pdump file.
pub fn load_from_dump(path: &Path) -> Result<Context, DumpError> {
    let proof = install_eval_hooks();
    load_from_dump_with_proof(path, proof)
}
