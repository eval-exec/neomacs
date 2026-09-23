use super::*;

#[test]
fn rejects_collaborators_from_different_windows_com_type_families() {
    let metadata = metadata_with_windows_versions("0.62.2", "0.54.0");

    assert!(matches!(
        validate_windows_com_family(&metadata),
        Err(CoherenceViolation::SplitWindowsComFamily {
            wgpu_hal: PackageId(wgpu),
            gpu_allocator: PackageId(allocator),
        }) if wgpu.ends_with("windows@0.62.2") && allocator.ends_with("windows@0.54.0")
    ));
}

#[test]
fn accepts_collaborators_from_one_windows_com_type_family() {
    let metadata = metadata_with_windows_versions("0.62.2", "0.62.2");

    assert_eq!(validate_windows_com_family(&metadata), Ok(()));
}

fn metadata_with_windows_versions(
    wgpu_windows_version: &str,
    allocator_windows_version: &str,
) -> CargoMetadata {
    let wgpu_hal = package_id("wgpu-hal", "30.0.1");
    let gpu_allocator = package_id("gpu-allocator", "0.28.0");
    let wgpu_windows = package_id("windows", wgpu_windows_version);
    let allocator_windows = package_id("windows", allocator_windows_version);
    let mut packages = vec![
        CargoPackage {
            id: wgpu_hal.clone(),
            name: "wgpu-hal".into(),
        },
        CargoPackage {
            id: gpu_allocator.clone(),
            name: "gpu-allocator".into(),
        },
    ];
    for id in [&wgpu_windows, &allocator_windows] {
        if !packages.iter().any(|package| package.id == *id) {
            packages.push(CargoPackage {
                id: id.clone(),
                name: "windows".into(),
            });
        }
    }
    CargoMetadata {
        packages,
        resolve: CargoResolve {
            nodes: vec![
                CargoNode {
                    id: wgpu_hal,
                    deps: vec![
                        CargoNodeDependency {
                            name: "gpu_allocator".into(),
                            pkg: gpu_allocator.clone(),
                        },
                        CargoNodeDependency {
                            name: "windows".into(),
                            pkg: wgpu_windows,
                        },
                    ],
                },
                CargoNode {
                    id: gpu_allocator,
                    deps: vec![CargoNodeDependency {
                        name: "windows".into(),
                        pkg: allocator_windows,
                    }],
                },
            ],
        },
    }
}

fn package_id(name: &str, version: &str) -> PackageId {
    PackageId(format!(
        "registry+https://github.com/rust-lang/crates.io-index#{name}@{version}"
    ))
}
