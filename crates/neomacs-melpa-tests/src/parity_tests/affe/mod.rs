use std::time::Duration;

use crate::{AFFE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod async_frontend;
mod autoloads;
mod backend_producer;
mod backend_protocol;
mod backend_search;
mod commands;
mod surface;
mod transport;
mod workflows;

const AFFE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn affe_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AFFE_MELPA_PIN, source_file)
        .expect("prepare pinned affe source and Consult dependency below ./tmp")
        .with_prelude("(require 'cl-lib)")
        .with_timeout(AFFE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed affe parity test").into()
}

/// Multi-probe batch for `assert_affe_autoload_parity` cases (2a).
pub(crate) fn assert_affe_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        affe_oracle("affe-autoloads.el"),
        &name,
        "affe_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_affe_backend_parity` cases (2a).
pub(crate) fn assert_affe_backend_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        affe_oracle("affe-backend.el"),
        &name,
        "affe_backend_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_affe_parity` cases (2a).
pub(crate) fn assert_affe_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(affe_oracle("affe.el"), &name, "affe_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn affe_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [autoloads::autoloads_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_affe_autoload_batch(&cases);
}

#[test]
fn affe_backend_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        backend_producer::backend_producer_public_surface_batch_cases(),
        backend_protocol::backend_protocol_public_surface_batch_cases(),
        backend_search::backend_search_public_surface_batch_cases(),
        surface::surface_affe_backend_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_affe_backend_batch(&cases);
}

#[test]
fn affe_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        async_frontend::async_frontend_public_surface_batch_cases(),
        commands::commands_public_surface_batch_cases(),
        surface::surface_affe_batch_cases(),
        transport::transport_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_affe_batch(&cases);
}

// END generated package batch tests
