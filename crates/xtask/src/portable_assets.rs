//! Deterministic Android/browser runtime asset packaging.

use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use flate2::{Compression, GzBuilder};
use sha2::{Digest, Sha256};
use tar::{Builder, HeaderMode};
use tempfile::NamedTempFile;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const PORTABLE_RUNTIME_IMAGE_ASSET: &str = "neomacs.portable";
// Use an opaque transport suffix: Android's asset packager treats `.gz` as a
// directive to decompress the file and remove its suffix.  The bytes remain a
// deterministic gzip-compressed tar archive on every host.
const RUNTIME_RESOURCE_ARCHIVE_ASSET: &str = "neomacs-runtime.bundle";
const RUNTIME_RESOURCE_ID_ASSET: &str = "neomacs-runtime.sha256";
const REQUIRED_RESOURCE_ROOTS: [&str; 2] = ["lisp", "etc"];
const OPTIONAL_RESOURCE_ROOTS: [&str; 2] = ["leim", "info"];

pub(super) fn run(repo_root: &Path, args: impl IntoIterator<Item = OsString>) -> Result<()> {
    let mut args = args.into_iter();
    let mut portable_image = None;
    let mut output_dir = None;
    let mut runtime_root = repo_root.to_path_buf();
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--portable-runtime-image") => {
                portable_image = Some(resolve_path(
                    repo_root,
                    args.next()
                        .ok_or("--portable-runtime-image requires a path")?,
                ));
            }
            Some("--output-dir") => {
                output_dir = Some(resolve_path(
                    repo_root,
                    args.next().ok_or("--output-dir requires a path")?,
                ));
            }
            Some("--runtime-root") => {
                runtime_root = resolve_path(
                    repo_root,
                    args.next().ok_or("--runtime-root requires a path")?,
                );
            }
            Some("-h" | "--help" | "help") => {
                print_usage();
                return Ok(());
            }
            Some(other) => {
                return Err(format!("unknown package-portable-assets option: {other}").into());
            }
            None => return Err("package-portable-assets arguments must be valid Unicode".into()),
        }
    }

    let portable_image = portable_image.ok_or(
        "package-portable-assets requires --portable-runtime-image PATH; produce it with cargo xtask fresh-build --release --portable-runtime-image PATH",
    )?;
    let output_dir = output_dir.ok_or("package-portable-assets requires --output-dir DIR")?;
    package(&runtime_root, &portable_image, &output_dir)
}

fn package(runtime_root: &Path, portable_image: &Path, output_dir: &Path) -> Result<()> {
    validate_nonempty_file(portable_image, "portable runtime image")?;
    for required in REQUIRED_RESOURCE_ROOTS {
        let path = runtime_root.join(required);
        if !path.is_dir() {
            return Err(format!(
                "runtime root {} does not contain required {required}/ directory",
                runtime_root.display()
            )
            .into());
        }
    }
    fs::create_dir_all(output_dir)?;

    let archive_path = output_dir.join(RUNTIME_RESOURCE_ARCHIVE_ASSET);
    let archive_id = write_runtime_archive(runtime_root, output_dir, &archive_path)?;
    atomic_copy(
        portable_image,
        output_dir,
        &output_dir.join(PORTABLE_RUNTIME_IMAGE_ASSET),
    )?;
    // Publish the resource ID last. Android treats this small file as the
    // selection/commit record and authenticates the archive against it before
    // exposing an extracted tree.
    atomic_write(
        output_dir,
        &output_dir.join(RUNTIME_RESOURCE_ID_ASSET),
        format!("{archive_id}\n").as_bytes(),
    )?;
    sync_directory(output_dir)?;

    println!("+ packaged portable Neomacs assets");
    println!(
        "  image     = {}",
        output_dir.join(PORTABLE_RUNTIME_IMAGE_ASSET).display()
    );
    println!("  resources = {}", archive_path.display());
    println!("  id        = {archive_id}");
    Ok(())
}

fn write_runtime_archive(
    runtime_root: &Path,
    output_dir: &Path,
    destination: &Path,
) -> Result<String> {
    let mut entries = Vec::new();
    for root in REQUIRED_RESOURCE_ROOTS
        .into_iter()
        .chain(OPTIONAL_RESOURCE_ROOTS)
    {
        let source = runtime_root.join(root);
        if source.exists() {
            collect_entries(&source, PathBuf::from(root), &mut entries)?;
        }
    }
    entries.sort_by(|left, right| left.1.cmp(&right.1));

    let mut temporary = NamedTempFile::new_in(output_dir)?;
    {
        let encoder = GzBuilder::new()
            .mtime(0)
            .operating_system(255)
            .write(temporary.as_file_mut(), Compression::new(6));
        let mut archive = Builder::new(encoder);
        archive.mode(HeaderMode::Deterministic);
        archive.follow_symlinks(false);
        for (source, relative) in &entries {
            archive.append_path_with_name(source, relative)?;
        }
        let encoder = archive.into_inner()?;
        encoder.finish()?.sync_all()?;
    }
    let id = sha256_file(temporary.path())?;
    persist(temporary, destination)?;
    Ok(id)
}

fn collect_entries(
    source: &Path,
    relative: PathBuf,
    entries: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "runtime resource bundles do not permit symlinks: {}",
            source.display()
        )
        .into());
    }
    if metadata.is_file() {
        entries.push((source.to_path_buf(), relative));
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(format!(
            "runtime resource bundles support only files and directories: {}",
            source.display()
        )
        .into());
    }

    entries.push((source.to_path_buf(), relative.clone()));
    let mut children = fs::read_dir(source)?.collect::<std::result::Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        collect_entries(&child.path(), relative.join(child.file_name()), entries)?;
    }
    Ok(())
}

fn atomic_copy(source: &Path, directory: &Path, destination: &Path) -> Result<()> {
    if source == destination {
        return Ok(());
    }
    let mut source = File::open(source)?;
    let mut temporary = NamedTempFile::new_in(directory)?;
    io::copy(&mut source, &mut temporary)?;
    temporary.as_file().sync_all()?;
    persist(temporary, destination)
}

fn atomic_write(directory: &Path, destination: &Path, contents: &[u8]) -> Result<()> {
    let mut temporary = NamedTempFile::new_in(directory)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    persist(temporary, destination)
}

fn persist(temporary: NamedTempFile, destination: &Path) -> Result<()> {
    let file = temporary
        .persist(destination)
        .map_err(|error| error.error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(destination, fs::Permissions::from_mode(0o644))?;
    }
    file.sync_all()?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn validate_nonempty_file(path: &Path, description: &str) -> Result<()> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot read {description} {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(format!("{description} is not a nonempty file: {}", path.display()).into());
    }
    Ok(())
}

fn resolve_path(repo_root: &Path, value: OsString) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn print_usage() {
    println!(
        "Usage: cargo xtask package-portable-assets \\\n  --portable-runtime-image PATH --output-dir DIR [--runtime-root DIR]\n\n\
         Stage the portable evaluator image plus a deterministic, authenticated\n\
         lisp/etc/leim/info runtime bundle for Android and neomacs-wasm. Produce\n\
         the input image first with cargo xtask fresh-build --release\n\
         --portable-runtime-image PATH."
    );
}
