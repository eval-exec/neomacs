use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::PerfError;

pub(crate) fn unix_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub(crate) fn write_json_atomically(path: &Path, value: &impl Serialize) -> Result<(), PerfError> {
    let json = serde_json::to_vec_pretty(value).map_err(PerfError::SerializeArtifact)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, json).map_err(|source| PerfError::WriteArtifact {
        path: temporary.clone(),
        source,
    })?;
    fs::rename(&temporary, path).map_err(|source| PerfError::WriteArtifact {
        path: path.to_path_buf(),
        source,
    })
}
