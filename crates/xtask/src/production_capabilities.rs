use serde::Deserialize;

const WORKSPACE_MANIFEST: &str = include_str!(concat!(env!("CARGO_WORKSPACE_DIR"), "/Cargo.toml"));
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CargoCapability {
    Video,
    Webview,
}

impl CargoCapability {
    pub(crate) const fn feature_name(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Webview => "webview",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProductionVideoBackend {
    None,
    LinkedGstreamer,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct PlatformCapabilities {
    cargo_features: Vec<CargoCapability>,
    video_backend: ProductionVideoBackend,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CapabilityManifest {
    schema_version: u32,
    linux: PlatformCapabilities,
    darwin: PlatformCapabilities,
    windows: PlatformCapabilities,
}

#[derive(Debug, Deserialize)]
struct WorkspaceMetadata {
    #[serde(rename = "neomacs-production-capabilities")]
    production_capabilities: CapabilityManifest,
}

#[derive(Debug, Deserialize)]
struct Workspace {
    metadata: WorkspaceMetadata,
}

#[derive(Debug, Deserialize)]
struct CargoManifest {
    workspace: Workspace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostPlatform {
    Linux,
    Darwin,
    Windows,
}

impl HostPlatform {
    fn current() -> Result<Self, String> {
        if cfg!(target_os = "linux") {
            Ok(Self::Linux)
        } else if cfg!(target_os = "macos") {
            Ok(Self::Darwin)
        } else if cfg!(target_os = "windows") {
            Ok(Self::Windows)
        } else {
            Err(format!(
                "no production capability profile for target OS {}",
                std::env::consts::OS
            ))
        }
    }

    fn select(self, manifest: CapabilityManifest) -> PlatformCapabilities {
        match self {
            Self::Linux => manifest.linux,
            Self::Darwin => manifest.darwin,
            Self::Windows => manifest.windows,
        }
    }
}

/// A validated distribution build policy.
///
/// The workspace manifest is the serialization seam shared with Nix.  Past
/// that seam, callers see enums rather than feature/backend strings, and the
/// constructor rejects combinations that cannot produce a runnable package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProductionCapabilities(PlatformCapabilities);

impl ProductionCapabilities {
    pub(crate) fn for_host() -> Result<Self, String> {
        let cargo: CargoManifest = toml::from_str(WORKSPACE_MANIFEST)
            .map_err(|error| format!("invalid production capability metadata: {error}"))?;
        let manifest = cargo.workspace.metadata.production_capabilities;
        if manifest.schema_version != SUPPORTED_SCHEMA_VERSION {
            return Err(format!(
                "unsupported production capability schema {}; expected {}",
                manifest.schema_version, SUPPORTED_SCHEMA_VERSION
            ));
        }

        let host = HostPlatform::current()?;
        let capabilities = host.select(manifest);
        validate_capabilities(host, &capabilities)?;
        Ok(Self(capabilities))
    }

    pub(crate) fn cargo_features(&self) -> &[CargoCapability] {
        &self.0.cargo_features
    }

    pub(crate) fn cargo_feature_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.cargo_features()
            .iter()
            .copied()
            .map(CargoCapability::feature_name)
    }

    #[cfg(test)]
    pub(crate) const fn video_backend(&self) -> ProductionVideoBackend {
        self.0.video_backend
    }
}

fn validate_capabilities(
    host: HostPlatform,
    capabilities: &PlatformCapabilities,
) -> Result<(), String> {
    let has_video = capabilities
        .cargo_features
        .contains(&CargoCapability::Video);
    let valid_video_product = matches!(
        (host, capabilities.video_backend, has_video),
        (
            HostPlatform::Linux,
            ProductionVideoBackend::LinkedGstreamer,
            true
        ) | (
            HostPlatform::Darwin | HostPlatform::Windows,
            ProductionVideoBackend::None,
            false
        )
    );
    if !valid_video_product {
        return Err(format!(
            "invalid production video product for {host:?}: backend {:?}, video feature {has_video}",
            capabilities.video_backend
        ));
    }

    for (index, capability) in capabilities.cargo_features.iter().enumerate() {
        if capabilities.cargo_features[..index].contains(capability) {
            return Err(format!(
                "duplicate production Cargo capability {capability:?} for {host:?}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CargoCapability, HostPlatform, PlatformCapabilities, ProductionVideoBackend,
        validate_capabilities,
    };

    #[test]
    fn platform_and_video_backend_must_describe_one_real_product() {
        let linux_without_video = PlatformCapabilities {
            cargo_features: Vec::new(),
            video_backend: ProductionVideoBackend::None,
        };
        assert!(validate_capabilities(HostPlatform::Linux, &linux_without_video).is_err());

        let darwin_with_gstreamer = PlatformCapabilities {
            cargo_features: vec![CargoCapability::Video],
            video_backend: ProductionVideoBackend::LinkedGstreamer,
        };
        assert!(validate_capabilities(HostPlatform::Darwin, &darwin_with_gstreamer).is_err());

        let linux_full = PlatformCapabilities {
            cargo_features: vec![CargoCapability::Video],
            video_backend: ProductionVideoBackend::LinkedGstreamer,
        };
        assert_eq!(
            validate_capabilities(HostPlatform::Linux, &linux_full),
            Ok(())
        );
    }
}
