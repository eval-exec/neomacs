use super::*;

#[test]
fn media_foundation_defaults_to_limited_bt709() {
    assert_eq!(
        media_foundation_colorimetry(MediaFoundationColorMetadata::default()),
        VideoColorimetry::BT709_LIMITED
    );
}

#[test]
fn media_foundation_maps_hdr10_metadata() {
    assert_eq!(
        media_foundation_colorimetry(MediaFoundationColorMetadata {
            primaries: Some(MFVideoPrimaries_BT2020.0 as u32),
            transfer: Some(MFVideoTransFunc_2084.0 as u32),
            matrix: Some(MFVideoTransferMatrix_BT2020_10.0 as u32),
            range: Some(MFNominalRange_0_255.0 as u32),
            chroma_siting: Some(MFVideoChromaSubsampling_DV_PAL.0 as u32),
        }),
        VideoColorimetry {
            primaries: VideoColorPrimaries::Bt2020,
            transfer: VideoTransferCharacteristic::Pq,
            matrix: VideoMatrixCoefficients::Bt2020NonConstantLuminance,
            range: VideoColorRange::Full,
            chroma_location: VideoChromaLocation::TopLeft,
        }
    );
}

#[test]
fn output_format_keeps_native_and_packed_types_consistent() {
    assert_eq!(
        WindowsOutputFormat::Nv12.frame(),
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)
    );
    assert_eq!(WindowsOutputFormat::Nv12.wgpu(), wgpu::TextureFormat::NV12);
    assert_eq!(
        WindowsOutputFormat::Nv12.candidates(),
        [WindowsOutputFormat::Nv12, WindowsOutputFormat::Bgra8]
    );
    assert_eq!(
        WindowsOutputFormat::Nv12.fallback_after_rejection(),
        Some(WindowsOutputFormat::Bgra8)
    );
    assert_eq!(WindowsOutputFormat::Bgra8.fallback_after_rejection(), None);
    assert_eq!(
        WindowsOutputFormat::Bgra8.frame(),
        VideoFrameFormat::Packed(PackedVideoFormat::Bgra8)
    );
    assert_eq!(
        WindowsOutputFormat::Bgra8.media_engine_dxgi(),
        DXGI_FORMAT_B8G8R8A8_UNORM
    );
    assert_eq!(
        WindowsOutputFormat::Bgra8.resource_dxgi(),
        DXGI_FORMAT_B8G8R8A8_TYPELESS
    );
    assert_eq!(
        WindowsOutputFormat::Bgra8.wgpu(),
        wgpu::TextureFormat::Bgra8UnormSrgb
    );
    assert_eq!(
        WindowsOutputFormat::Nv12.completed_import(),
        CompletedFrameImport::GpuBlit {
            reported_bytes: None
        }
    );
}

#[test]
fn format_change_event_invalidates_cached_stream_metadata() {
    assert_eq!(
        media_engine_event_flag(MF_MEDIA_ENGINE_EVENT_FORMATCHANGE.0 as u32),
        EVENT_FORMAT_CHANGED
    );
}
