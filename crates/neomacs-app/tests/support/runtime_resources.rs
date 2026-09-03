use std::io::Cursor;
use std::path::Path;

use flate2::{Compression, GzBuilder};
use neomacs_app::runtime_resources::{MountedRuntimeResources, RuntimeResourceBundle};
use sha2::{Digest, Sha256};
use tar::{Builder, Header};

pub(crate) fn content_id(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn runtime_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let encoder = GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), Compression::fast());
    let mut archive = Builder::new(encoder);
    for (path, contents) in entries {
        let mut header = Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_cksum();
        archive
            .append_data(&mut header, path, Cursor::new(*contents))
            .unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap()
}

pub(crate) fn mounted_runtime_resources(entries: &[(&str, &[u8])]) -> MountedRuntimeResources {
    let archive = runtime_archive(entries);
    let id = content_id(&archive);
    let bundle = RuntimeResourceBundle::from_assets(&archive, id.as_bytes())
        .expect("pair browser runtime bundle assets");
    MountedRuntimeResources::from_bundle(Path::new("/neomacs"), bundle)
        .expect("mount browser runtime bundle")
}
