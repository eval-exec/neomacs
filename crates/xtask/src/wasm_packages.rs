//! Locked, build-time acquisition of the browser landing page's Lisp packages.
//!
//! Git object archives, not working trees, are the input. Third-party sources
//! stay in the ignored build cache and in an independently addressed bundle.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;
use std::process::Command;

use flate2::{Compression, GzBuilder};
use serde::Deserialize;
use tar::{Builder, Header};

use super::portable_assets::sha256_file;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize)]
struct Lock {
    package: Vec<Package>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    url: String,
    revision: String,
}

pub(super) fn package(repo: &Path, output: &Path) -> Result<()> {
    let lock_bytes = fs::read(repo.join("crates/neomacs-wasm/packages.lock.toml"))?;
    let lock: Lock = toml::from_str(std::str::from_utf8(&lock_bytes)?)?;
    let mut files = BTreeMap::from([("etc/wasm-packages.lock.toml".to_owned(), lock_bytes)]);
    for package in lock.package {
        if package.name.is_empty()
            || !package
                .name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'-')
            || package.revision.len() != 40
            || !package.revision.bytes().all(|b| b.is_ascii_hexdigit())
            || !package.url.starts_with("https://github.com/")
        {
            return Err("invalid browser package lock entry".into());
        }
        let cache = repo
            .join("target/wasm-package-sources")
            .join(&package.name)
            .join(&package.revision);
        fs::create_dir_all(&cache)?;
        git(&cache, &["init", "--bare", "--quiet"])?;
        let present = Command::new("git")
            .current_dir(&cache)
            .args([
                "cat-file",
                "-e",
                &format!("{}^{{commit}}", package.revision),
            ])
            .output()?
            .status
            .success();
        if !present {
            println!(
                "+ fetching browser package {} @ {}",
                package.name, package.revision
            );
            git(
                &cache,
                &[
                    "fetch",
                    "--depth=1",
                    "--no-tags",
                    &package.url,
                    &package.revision,
                ],
            )?;
        }
        let archive = git(&cache, &["archive", "--format=tar", &package.revision])?;
        for entry in tar::Archive::new(Cursor::new(archive)).entries()? {
            let mut entry = entry?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let path = entry.path()?.into_owned();
            if !path
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
            {
                return Err("package archive path must be relative".into());
            }
            let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !matches!(extension, "el" | "png" | "svg")
                && !name.starts_with("LICENSE")
                && !name.starts_with("COPYING")
            {
                continue;
            }
            let target = format!(
                "lisp/neomacs-wasm-packages/{}/{}",
                package.name,
                path.to_string_lossy()
            );
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            if files.insert(target, bytes).is_some() {
                return Err("duplicate package path".into());
            }
        }
    }
    let archive_path = output.join("packages.bundle");
    write_archive(&archive_path, files)?;
    let digest = sha256_file(&archive_path)?;
    fs::write(
        output.join("packages.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema": 1, "sha256": digest,
        }))?,
    )?;
    Ok(())
}

fn git(directory: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .current_dir(directory)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "package git {} failed: {}",
            args[0],
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(output.stdout)
}

fn write_archive(path: &Path, files: BTreeMap<String, Vec<u8>>) -> Result<()> {
    let encoder = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(fs::File::create(path)?, Compression::new(6));
    let mut archive = Builder::new(encoder);
    for (name, bytes) in files {
        let mut header = Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_cksum();
        archive.append_data(&mut header, name, Cursor::new(bytes))?;
    }
    archive.into_inner()?.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_archive_is_deterministic_and_retains_sources_and_licenses() {
        let directory = tempfile::tempdir().unwrap();
        let files = BTreeMap::from([
            (
                "lisp/neomacs-wasm-packages/example/example.el".into(),
                b"(provide 'example)".to_vec(),
            ),
            ("etc/wasm-packages.lock.toml".into(), b"locked".to_vec()),
        ]);
        let first = directory.path().join("first.bundle");
        let second = directory.path().join("second.bundle");
        write_archive(&first, files.clone()).unwrap();
        write_archive(&second, files).unwrap();
        assert_eq!(fs::read(first).unwrap(), fs::read(second).unwrap());
    }
}
