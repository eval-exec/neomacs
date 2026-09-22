//! Seal and verify a provisioned package tree.
//!
//! Same invariant as `config_env`'s sealing: once GNU has materialized the
//! package and the cache marked it `ready`, an inventory of every file is
//! recorded, and any later drift (a test writing into the shared tree) is
//! detectable on demand instead of silently poisoning other suites.

use std::path::Path;

use crate::inventory::{self, Drift, Inventory};

const INVENTORY_FILE: &str = "INVENTORY.json";

/// Record the inventory of a freshly prepared package tree.
///
/// Called by [`crate::packages::provision`] after the cache reports `ready`;
/// idempotent, so plain cache hits re-seal cheaply and pick up legitimate
/// manifest bumps.
pub fn seal_provisioned(package: &super::ProvisionedPackage) -> Result<(), String> {
    let walked = package.package_dir();
    let inventory = Inventory::build(walked)?;
    let record = seal_record(package);
    std::fs::write(&record, inventory.to_jsonl()).map_err(|error| {
        format!(
            "failed to write package inventory {}: {error}",
            record.display()
        )
    })
}

/// Deep-verify a provisioned package against its recorded inventory.
pub fn verify_provisioned(
    package: &super::ProvisionedPackage,
) -> Result<ProvisionedSealReport, String> {
    let record = seal_record(package);
    if !record.is_file() {
        return Ok(ProvisionedSealReport::Unsealed);
    }
    let text = std::fs::read_to_string(&record)
        .map_err(|error| format!("failed to read {}: {error}", record.display()))?;
    let parsed = Inventory::parse_jsonl(&text)?;
    Ok(ProvisionedSealReport::Verified(inventory::verify_deep(
        package.package_dir(),
        &parsed,
    )?))
}

/// Whether a provisioned package drifted from its seal.  A missing seal is
/// not drift: packages provisioned before this inventory existed stay usable.
pub fn drifted(report: &ProvisionedSealReport) -> Option<&Drift> {
    match report {
        ProvisionedSealReport::Unsealed => None,
        ProvisionedSealReport::Verified(drift) if drift.is_clean() => None,
        ProvisionedSealReport::Verified(drift) => Some(drift),
    }
}

/// The provenance report for a provisioned package's seal.
#[derive(Clone, Debug)]
pub enum ProvisionedSealReport {
    /// No inventory recorded (provisioned by an older build).
    Unsealed,
    /// The tree was re-hashed and compared against the recorded inventory.
    Verified(Drift),
}

/// The seal record lives beside the cache's own `ready`/`failed` markers,
/// one level above the package directory — the walked tree must never
/// contain the inventory that describes it.
fn seal_record(package: &super::ProvisionedPackage) -> std::path::PathBuf {
    let mut record = package
        .package_dir()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| package.package_dir().to_path_buf());
    record.push(INVENTORY_FILE);
    record
}
