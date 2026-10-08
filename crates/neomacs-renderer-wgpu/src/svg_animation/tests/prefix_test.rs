use super::*;

#[test]
fn delayed_repeating_animation_preserves_initial_base_pixels() {
    let source = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="red" opacity="0.2"><animate attributeName="opacity" from="0.8" to="1" begin="1s" dur="1s" repeatCount="indefinite"/></rect></svg>"#;
    let sampled = sampler::sample(
        source,
        ImageColorContext::default(),
        &crate::svg::SvgResourceContext::Isolated,
        ImageAnimationPolicy::enabled(Some(2)),
    )
    .expect("sampled delayed animation");
    let base = crate::svg::decode(
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="red" opacity="0.2"/></svg>"#,
        Default::default(), Default::default(), Default::default(), Default::default(),
        crate::svg::SvgResourceContext::Isolated,
    ).unwrap();
    assert_eq!(
        sampled.frames[0].rgba, base.rgba,
        "introduction starts before activation"
    );
}

fn resolve_frame(
    cache: &crate::image_sequence::ImageSequenceCache,
    source: &[u8],
    index: u64,
    fps: u32,
) -> (Vec<u8>, neomacs_display_protocol::ImageEmbeddedMetadata) {
    match cache.resolve_svg(
        neomacs_display_protocol::ImageSequenceId::new(1).unwrap(),
        source,
        neomacs_display_protocol::ImageFrameIndex::new(index),
        ImageColorContext::default(),
        &crate::svg::SvgResourceContext::Isolated,
        ImageAnimationPolicy::enabled(Some(fps)),
    ) {
        crate::image_sequence::ImageSequenceResolution::Frame(frame) => frame.into_parts(),
        _ => panic!("materialized frame {index}"),
    }
}

#[test]
fn finite_intro_effect_is_visible_once_before_the_repeatable_tail() {
    let source = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="black"><set attributeName="fill" to="red" dur="1s"/><animate attributeName="opacity" from="1" to="0.8" dur="1s" repeatCount="indefinite"/></rect></svg>"#;
    let cache = crate::image_sequence::ImageSequenceCache::new();
    let (first, metadata) = resolve_frame(&cache, source, 0, 2);
    assert_eq!(metadata.frame_count(), Some(4));
    assert_eq!(metadata.loop_start(), Some(2));
    let (tail, tail_metadata) = resolve_frame(&cache, source, 2, 2);
    assert_eq!(tail_metadata.loop_start(), Some(2));
    assert_ne!(
        first, tail,
        "finite red effect appears before restoring black"
    );
    assert!(first.chunks_exact(4).any(|pixel| pixel[0] != 0));
    assert!(tail.chunks_exact(4).all(|pixel| pixel[0] == 0));
}

#[test]
fn prefix_and_loop_share_the_cap_without_stretching_either_duration() {
    let source = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="red"><animate attributeName="opacity" from="0.2" to="1" begin="3s" dur="2s" repeatCount="indefinite"/></rect></svg>"#;
    let sampled = sampler::sample(
        source,
        ImageColorContext::default(),
        &crate::svg::SvgResourceContext::Isolated,
        ImageAnimationPolicy::enabled(Some(100)),
    )
    .unwrap();
    assert_eq!(sampled.frames.len(), 256);
    let start = sampled.loop_start.unwrap().get() as usize;
    let prefix_seconds: f64 = sampled.frames[..start]
        .iter()
        .map(|frame| frame.delay.seconds().unwrap())
        .sum();
    let loop_seconds: f64 = sampled.frames[start..]
        .iter()
        .map(|frame| frame.delay.seconds().unwrap())
        .sum();
    assert!((prefix_seconds - 3.0).abs() < 1e-9);
    assert!((loop_seconds - 2.0).abs() < 1e-9);
    assert_ne!(sampled.frames[0].delay, sampled.frames[start].delay);
    let cache = crate::image_sequence::ImageSequenceCache::new();
    let (_, metadata) = resolve_frame(&cache, source, 0, 100);
    assert_eq!(metadata.loop_start(), Some(start as u32));
    assert_eq!(metadata.intro_delay(), Some(sampled.frames[0].delay));
    assert_eq!(metadata.loop_delay(), Some(sampled.frames[start].delay));
    let (_, loop_metadata) = resolve_frame(&cache, source, start as u64, 100);
    assert_eq!(
        loop_metadata.frame_delay(),
        Some(sampled.frames[start].delay)
    );
}
