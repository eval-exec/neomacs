//! Explicit image-fixture preparation for `cargo xtask image-fixtures`.

use neomacs_infra::image_fixtures::{NYAN_MODE_NAME, NYAN_MODE_SHA256, NYAN_MODE_URL};
use std::process::ExitCode;

fn main() -> ExitCode {
    match neomacs_infra::pinned::pinned_file(NYAN_MODE_NAME, NYAN_MODE_URL, NYAN_MODE_SHA256) {
        Ok(path) => {
            println!("verified {} sha256={NYAN_MODE_SHA256}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("FIXTURE-FETCH: url={NYAN_MODE_URL} sha256={NYAN_MODE_SHA256} error={error}");
            ExitCode::FAILURE
        }
    }
}
