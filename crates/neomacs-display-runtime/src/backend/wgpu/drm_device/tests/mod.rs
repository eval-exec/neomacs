use super::*;

#[cfg(unix)]
use std::os::unix::fs::symlink;

#[cfg(unix)]
fn fake_render_node_roots() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let scratch = tempfile::tempdir().expect("create DRM resolver scratch directory");
    let sysfs = scratch.path().join("sys-dev-char");
    let devices = scratch.path().join("devices");
    let dev_dri = scratch.path().join("dev-dri");
    fs::create_dir_all(&sysfs).expect("create fake sysfs root");
    fs::create_dir_all(&devices).expect("create fake device root");
    fs::create_dir_all(&dev_dri).expect("create fake dev/dri root");
    (scratch, sysfs, dev_dri)
}

#[cfg(unix)]
fn install_fake_sysfs_link(root: &Path, major: u32, minor: u32, name: &str) {
    let device = root
        .parent()
        .expect("fake sysfs root has parent")
        .join("devices")
        .join(name);
    fs::create_dir_all(&device).expect("create fake sysfs device");
    symlink(device, root.join(format!("{major}:{minor}"))).expect("link fake sysfs device");
}

#[cfg(unix)]
#[test]
fn exact_render_node_resolver_rejects_missing_and_non_character_nodes() {
    let (_scratch, sysfs, dev_dri) = fake_render_node_roots();
    install_fake_sysfs_link(&sysfs, 1, 3, "renderD128");
    assert_eq!(
        render_node_from_device_number_in(1, 3, &sysfs, &dev_dri),
        None
    );

    fs::write(dev_dri.join("renderD128"), b"not a device").expect("write regular file");
    assert_eq!(
        render_node_from_device_number_in(1, 3, &sysfs, &dev_dri),
        None
    );
}

#[cfg(unix)]
#[test]
fn exact_render_node_resolver_checks_the_candidate_device_number() {
    let (_scratch, sysfs, dev_dri) = fake_render_node_roots();
    install_fake_sysfs_link(&sysfs, 1, 3, "renderD128");
    install_fake_sysfs_link(&sysfs, 1, 5, "renderD128");
    symlink("/dev/null", dev_dri.join("renderD128")).expect("link character device");

    assert_eq!(
        render_node_from_device_number_in(1, 5, &sysfs, &dev_dri),
        None
    );
    assert_eq!(
        render_node_from_device_number_in(1, 3, &sysfs, &dev_dri),
        Some(dev_dri.join("renderD128"))
    );
}
