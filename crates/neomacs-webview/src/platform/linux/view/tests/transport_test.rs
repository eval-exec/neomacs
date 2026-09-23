use super::{WebViewFrameTransport, WpeFrameTransport};

#[test]
fn automatic_transport_without_consumer_capabilities_uses_pixels() {
    assert_eq!(
        WpeFrameTransport::resolve(WebViewFrameTransport::Auto, Default::default()),
        WpeFrameTransport::SoftwarePixels,
        "being able to export a DMA-BUF does not prove the renderer can import it"
    );
}

#[test]
fn dma_buf_preference_cannot_override_missing_consumer_support() {
    assert_eq!(
        WpeFrameTransport::resolve(WebViewFrameTransport::DmaBuf, Default::default()),
        WpeFrameTransport::SoftwarePixels
    );
}

#[test]
fn automatic_transport_retains_exact_consumer_formats_and_honors_pixels() {
    use neomacs_display_protocol::{DmaBufFormat, DmaBufImportFormats};
    let formats = DmaBufImportFormats::new([DmaBufFormat {
        fourcc: 0x34325258,
        modifier: 0,
    }]);
    assert_eq!(
        WpeFrameTransport::resolve(WebViewFrameTransport::Auto, formats.clone()),
        WpeFrameTransport::DmaBuf(formats.clone())
    );
    assert_eq!(
        WpeFrameTransport::resolve(WebViewFrameTransport::SoftwarePixels, formats),
        WpeFrameTransport::SoftwarePixels
    );
}
