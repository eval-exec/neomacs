use super::*;
use neomacs_display_protocol::ImageCacheUsage;

#[test]
fn shared_image_state_publishes_exact_renderer_cache_usage() {
    let state = ImageRenderState::default();

    state.publish_cache_usage(ImageCacheUsage::new(17, 23));

    assert_eq!(state.cached_size_bytes(), 40);
}
