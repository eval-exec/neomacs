//! Downloaded, integrity-checked font assets used only by tests.
//!
//! The source archive is pinned by both release URL and SHA-256.  Tests cache
//! the verified files below the workspace's `./tmp` directory so no binary
//! fixtures need to be committed.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use fs4::FileExt;
use sha2::{Digest, Sha256};
use thiserror::Error;

const RELEASE_NAME: &str = "spleen-2.2.0";
const RELEASE_URL: &str =
    "https://github.com/fcambus/spleen/releases/download/2.2.0/spleen-2.2.0.tar.gz";
const RELEASE_SHA256: &str = "ec42925c6b56d2138c862b2f97147c872e472f674bf03423417d827a08d69a89";
const WOFF2_COLLECTION_NAME: &str = "w3c-roundtrip-collection-order-001.woff2";
const WOFF2_COLLECTION_URL: &str = "https://raw.githubusercontent.com/w3c/woff2-compiled-tests/1fd8cd583645618f4df36c65a297479840ad5510/Decoder/Tests/xhtml1/roundtrip-collection-order-001.woff2";
const WOFF2_COLLECTION_SHA256: &str =
    "7a246412785b588a43acf9cddaab0a36674f8581bd328a6107e9761e38713058";
const NOTO_COLOR_EMOJI_NAME: &str = "noto-color-emoji-2.051.ttf";
const NOTO_COLOR_EMOJI_URL: &str = "https://raw.githubusercontent.com/googlefonts/noto-emoji/8998f5dd683424a73e2314a8c1f1e359c19e8742/fonts/NotoColorEmoji.ttf";
const NOTO_COLOR_EMOJI_SHA256: &str =
    "72a635cb3d2f3524c51620cdde406b217204e8a6a06c6a096ff8ed4b5fd6e27b";
const MAX_PINNED_FIXTURE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy)]
struct ArchivedFont {
    name: &'static str,
    sha256: &'static str,
}

const ARCHIVED_FONTS: [ArchivedFont; 5] = [
    ArchivedFont {
        name: "spleen-8x16.bdf",
        sha256: "4a3d97ee61a8c86a7525d8c723cb8a14081f395cd2feb4227ba5e3baf0629bae",
    },
    ArchivedFont {
        name: "spleen-8x16.pcf",
        sha256: "b469833f073927a92ac4ba0f75863a6b04d234d5af7d81a637857697620d5314",
    },
    ArchivedFont {
        name: "spleen-8x16.otb",
        sha256: "73784c46ffb7ff31adcee06923f839e02170fb0132a30323f1be1036b8f1da67",
    },
    ArchivedFont {
        name: "spleen-8x16.woff",
        sha256: "5ca783bd09fec9856fae0e142a1d0fac5542c2508e0c35859b39c42113a503b2",
    },
    ArchivedFont {
        name: "spleen-8x16.woff2",
        sha256: "7062d0818b46b713b08ee78e457a3e78659a2b8d3107f609676cef4e5d38aa2c",
    },
];

static SPLEEN_FIXTURES: OnceLock<SpleenFixtures> = OnceLock::new();
static WOFF2_COLLECTION_FIXTURE: OnceLock<PathBuf> = OnceLock::new();
static NOTO_COLOR_EMOJI_FIXTURE: OnceLock<PathBuf> = OnceLock::new();

/// Paths to the pinned Spleen faces used by the font boundary tests.
#[derive(Clone, Debug)]
pub struct SpleenFixtures {
    root: PathBuf,
}

impl SpleenFixtures {
    #[must_use]
    pub fn bdf(&self) -> PathBuf {
        self.root.join("spleen-8x16.bdf")
    }

    #[must_use]
    pub fn pcf(&self) -> PathBuf {
        self.root.join("spleen-8x16.pcf")
    }

    #[must_use]
    pub fn pcf_gz(&self) -> PathBuf {
        self.root.join("spleen-8x16.pcf.gz")
    }

    #[must_use]
    pub fn otb(&self) -> PathBuf {
        self.root.join("spleen-8x16.otb")
    }

    #[must_use]
    pub fn woff(&self) -> PathBuf {
        self.root.join("spleen-8x16.woff")
    }

    #[must_use]
    pub fn woff2(&self) -> PathBuf {
        self.root.join("spleen-8x16.woff2")
    }
}

/// Download and verify the pinned fixture archive on first use in this test
/// process, then return paths within the workspace-local cache.
///
/// # Panics
///
/// Panics with a detailed message when the network, checksum, archive, or cache
/// operation fails. Tests must not silently skip compatibility coverage.
#[must_use]
pub fn spleen_2_2_0() -> &'static SpleenFixtures {
    SPLEEN_FIXTURES.get_or_init(|| {
        prepare_spleen_fixtures()
            .unwrap_or_else(|error| panic!("failed to prepare pinned Spleen test fonts: {error}"))
    })
}

/// Download and verify the W3C decoder-suite WOFF2 collection used to check
/// that nonzero collection selectors survive decoding to standalone SFNT.
#[must_use]
pub fn woff2_collection() -> &'static Path {
    WOFF2_COLLECTION_FIXTURE
        .get_or_init(|| {
            prepare_pinned_file(
                WOFF2_COLLECTION_NAME,
                WOFF2_COLLECTION_URL,
                WOFF2_COLLECTION_SHA256,
            )
            .unwrap_or_else(|error| {
                panic!("failed to prepare pinned WOFF2 collection fixture: {error}")
            })
        })
        .as_path()
}

/// Download and verify the bitmap-only color SFNT used to check that exact
/// platform selection retains Swash's scalable color-bitmap replay path.
#[must_use]
pub fn noto_color_emoji_2_051() -> &'static Path {
    NOTO_COLOR_EMOJI_FIXTURE
        .get_or_init(|| {
            prepare_pinned_file(
                NOTO_COLOR_EMOJI_NAME,
                NOTO_COLOR_EMOJI_URL,
                NOTO_COLOR_EMOJI_SHA256,
            )
            .unwrap_or_else(|error| {
                panic!("failed to prepare pinned Noto Color Emoji test font: {error}")
            })
        })
        .as_path()
}

#[derive(Debug, Error)]
enum FixtureError {
    #[error("I/O failure at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("download from {url} failed: {message}")]
    Download { url: &'static str, message: String },
    #[error("SHA-256 mismatch for {path}: expected {expected}, got {actual}")]
    Checksum {
        path: PathBuf,
        expected: &'static str,
        actual: String,
    },
    #[error("archive {archive} did not contain {entry}")]
    MissingArchiveEntry { archive: PathBuf, entry: String },
    #[error("invalid archive entry path: {0}")]
    InvalidArchiveEntry(PathBuf),
}

fn prepare_spleen_fixtures() -> Result<SpleenFixtures, FixtureError> {
    let cache_root = workspace_root().join("tmp/font-fixtures");
    create_dir_all(&cache_root)?;

    let lock_path = cache_root.join(format!(".{RELEASE_NAME}.lock"));
    let lock = open_lock(&lock_path)?;
    lock.lock().map_err(|source| FixtureError::Io {
        path: lock_path.clone(),
        source,
    })?;

    let result = prepare_spleen_fixtures_locked(&cache_root);
    FileExt::unlock(&lock).map_err(|source| FixtureError::Io {
        path: lock_path,
        source,
    })?;
    result
}

fn prepare_pinned_file(
    name: &'static str,
    url: &'static str,
    sha256: &'static str,
) -> Result<PathBuf, FixtureError> {
    let cache_root = workspace_root().join("tmp/font-fixtures");
    create_dir_all(&cache_root)?;
    let lock_path = cache_root.join(format!(".{name}.lock"));
    let lock = open_lock(&lock_path)?;
    lock.lock().map_err(|source| FixtureError::Io {
        path: lock_path.clone(),
        source,
    })?;
    let destination = cache_root.join(name);
    let result = ensure_download(&destination, url, sha256).map(|()| destination);
    FileExt::unlock(&lock).map_err(|source| FixtureError::Io {
        path: lock_path,
        source,
    })?;
    result
}

fn prepare_spleen_fixtures_locked(cache_root: &Path) -> Result<SpleenFixtures, FixtureError> {
    let archive_path = cache_root.join(format!("{RELEASE_NAME}.tar.gz"));
    ensure_release_archive(&archive_path)?;

    let fixture_root = cache_root.join(RELEASE_NAME);
    if !all_extracted_files_match(&fixture_root)? {
        extract_verified_fonts(&archive_path, &fixture_root)?;
    }
    ensure_gzipped_pcf(&fixture_root)?;

    Ok(SpleenFixtures { root: fixture_root })
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_WORKSPACE_DIR")).to_path_buf()
}

fn open_lock(path: &Path) -> Result<File, FixtureError> {
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|source| FixtureError::Io {
            path: path.to_path_buf(),
            source,
        })
}

fn create_dir_all(path: &Path) -> Result<(), FixtureError> {
    fs::create_dir_all(path).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn ensure_release_archive(path: &Path) -> Result<(), FixtureError> {
    ensure_download(path, RELEASE_URL, RELEASE_SHA256)
}

fn ensure_download(
    path: &Path,
    url: &'static str,
    expected_sha256: &'static str,
) -> Result<(), FixtureError> {
    if path.exists() && verify_sha256(path, expected_sha256).is_ok() {
        return Ok(());
    }

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| FixtureError::Download {
            url,
            message: error.to_string(),
        })?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_PINNED_FIXTURE_BYTES)
        .read_to_vec()
        .map_err(|error| FixtureError::Download {
            url,
            message: error.to_string(),
        })?;

    let actual = sha256_bytes(&bytes);
    if actual != expected_sha256 {
        return Err(FixtureError::Checksum {
            path: path.to_path_buf(),
            expected: expected_sha256,
            actual,
        });
    }

    let partial = path.with_file_name(format!(
        ".{}.partial",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("font-fixture")
    ));
    write_all(&partial, &bytes)?;
    if path.exists() {
        fs::remove_file(path).map_err(|source| FixtureError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    }
    fs::rename(&partial, path).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    verify_sha256(path, expected_sha256)
}

fn all_extracted_files_match(root: &Path) -> Result<bool, FixtureError> {
    for font in ARCHIVED_FONTS {
        let path = root.join(font.name);
        if !path.exists() || verify_sha256(&path, font.sha256).is_err() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn extract_verified_fonts(archive_path: &Path, root: &Path) -> Result<(), FixtureError> {
    if root.exists() {
        fs::remove_dir_all(root).map_err(|source| FixtureError::Io {
            path: root.to_path_buf(),
            source,
        })?;
    }
    create_dir_all(root)?;

    let archive_file = File::open(archive_path).map_err(|source| FixtureError::Io {
        path: archive_path.to_path_buf(),
        source,
    })?;
    let decoder = GzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(decoder);
    let mut wanted: BTreeMap<&str, &str> = ARCHIVED_FONTS
        .iter()
        .map(|font| (font.name, font.sha256))
        .collect();
    let entries = archive.entries().map_err(|source| FixtureError::Io {
        path: archive_path.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let mut entry = entry.map_err(|source| FixtureError::Io {
            path: archive_path.to_path_buf(),
            source,
        })?;
        let entry_path = entry.path().map_err(|source| FixtureError::Io {
            path: archive_path.to_path_buf(),
            source,
        })?;
        let Some(name) = entry_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if wanted.remove(name).is_none() {
            continue;
        }
        let expected = Path::new(RELEASE_NAME).join(name);
        if entry_path.as_ref() != expected {
            return Err(FixtureError::InvalidArchiveEntry(entry_path.into_owned()));
        }
        let destination = root.join(name);
        let mut output = File::create(&destination).map_err(|source| FixtureError::Io {
            path: destination.clone(),
            source,
        })?;
        io::copy(&mut entry, &mut output).map_err(|source| FixtureError::Io {
            path: destination,
            source,
        })?;
    }

    if let Some((name, _)) = wanted.first_key_value() {
        return Err(FixtureError::MissingArchiveEntry {
            archive: archive_path.to_path_buf(),
            entry: format!("{RELEASE_NAME}/{name}"),
        });
    }
    for font in ARCHIVED_FONTS {
        verify_sha256(&root.join(font.name), font.sha256)?;
    }
    Ok(())
}

fn ensure_gzipped_pcf(root: &Path) -> Result<(), FixtureError> {
    let source_path = root.join("spleen-8x16.pcf");
    let gzip_path = root.join("spleen-8x16.pcf.gz");
    if gzip_path.exists() && gzip_expands_to(&gzip_path, &source_path)? {
        return Ok(());
    }

    let source = File::open(&source_path).map_err(|source| FixtureError::Io {
        path: source_path.clone(),
        source,
    })?;
    let partial = gzip_path.with_extension("gz.partial");
    let output = File::create(&partial).map_err(|source| FixtureError::Io {
        path: partial.clone(),
        source,
    })?;
    let mut encoder = GzEncoder::new(output, Compression::best());
    io::copy(&mut io::BufReader::new(source), &mut encoder).map_err(|source| FixtureError::Io {
        path: partial.clone(),
        source,
    })?;
    encoder.finish().map_err(|source| FixtureError::Io {
        path: partial.clone(),
        source,
    })?;
    if gzip_path.exists() {
        fs::remove_file(&gzip_path).map_err(|source| FixtureError::Io {
            path: gzip_path.clone(),
            source,
        })?;
    }
    fs::rename(&partial, &gzip_path).map_err(|source| FixtureError::Io {
        path: gzip_path.clone(),
        source,
    })?;
    if !gzip_expands_to(&gzip_path, &source_path)? {
        return Err(FixtureError::Checksum {
            path: gzip_path,
            expected: "the verified PCF payload",
            actual: "a different decompressed payload".to_owned(),
        });
    }
    Ok(())
}

fn gzip_expands_to(gzip_path: &Path, source_path: &Path) -> Result<bool, FixtureError> {
    let expected = read_all(source_path)?;
    let compressed = File::open(gzip_path).map_err(|source| FixtureError::Io {
        path: gzip_path.to_path_buf(),
        source,
    })?;
    let mut actual = Vec::new();
    GzDecoder::new(compressed)
        .read_to_end(&mut actual)
        .map_err(|source| FixtureError::Io {
            path: gzip_path.to_path_buf(),
            source,
        })?;
    Ok(actual == expected)
}

fn verify_sha256(path: &Path, expected: &'static str) -> Result<(), FixtureError> {
    let actual = sha256_bytes(&read_all(path)?);
    if actual == expected {
        Ok(())
    } else {
        Err(FixtureError::Checksum {
            path: path.to_path_buf(),
            expected,
            actual,
        })
    }
}

fn read_all(path: &Path) -> Result<Vec<u8>, FixtureError> {
    fs::read(path).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_all(path: &Path, bytes: &[u8]) -> Result<(), FixtureError> {
    let mut file = File::create(path).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    file.write_all(bytes).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    file.sync_all().map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn sha256_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod lib_test;
