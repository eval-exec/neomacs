use wgpu::{CompositeAlphaMode, SurfaceCapabilities, TextureFormat};

pub(super) fn preferred_format(capabilities: &SurfaceCapabilities) -> Option<TextureFormat> {
    capabilities
        .formats
        .iter()
        .copied()
        .find(TextureFormat::is_srgb)
        .or_else(|| capabilities.formats.first().copied())
}

pub(super) fn preferred_alpha_mode(
    capabilities: &SurfaceCapabilities,
) -> Option<CompositeAlphaMode> {
    capabilities
        .alpha_modes
        .contains(&CompositeAlphaMode::PreMultiplied)
        .then_some(CompositeAlphaMode::PreMultiplied)
        .or_else(|| capabilities.alpha_modes.first().copied())
}

#[cfg(test)]
mod tests {
    use super::{preferred_alpha_mode, preferred_format};
    use wgpu::{
        CompositeAlphaMode, PresentMode, SurfaceCapabilities, TextureFormat, TextureUsages,
    };

    fn capabilities(
        formats: Vec<TextureFormat>,
        alpha_modes: Vec<CompositeAlphaMode>,
    ) -> SurfaceCapabilities {
        SurfaceCapabilities {
            formats,
            format_capabilities: Vec::new(),
            present_modes: vec![PresentMode::Fifo],
            alpha_modes,
            usages: TextureUsages::RENDER_ATTACHMENT,
        }
    }

    #[test]
    fn chooses_srgb_over_an_earlier_linear_format() {
        let capabilities = capabilities(
            vec![TextureFormat::Bgra8Unorm, TextureFormat::Rgba8UnormSrgb],
            vec![CompositeAlphaMode::Opaque],
        );

        assert_eq!(
            preferred_format(&capabilities),
            Some(TextureFormat::Rgba8UnormSrgb)
        );
    }

    #[test]
    fn falls_back_to_the_first_available_format() {
        let capabilities = capabilities(
            vec![TextureFormat::Rgba16Float],
            vec![CompositeAlphaMode::Opaque],
        );

        assert_eq!(
            preferred_format(&capabilities),
            Some(TextureFormat::Rgba16Float)
        );
    }

    #[test]
    fn prefers_premultiplied_alpha_when_supported() {
        let capabilities = capabilities(
            vec![TextureFormat::Bgra8UnormSrgb],
            vec![
                CompositeAlphaMode::Opaque,
                CompositeAlphaMode::PreMultiplied,
            ],
        );

        assert_eq!(
            preferred_alpha_mode(&capabilities),
            Some(CompositeAlphaMode::PreMultiplied)
        );
    }
}
