use super::{
    InvalidPresentationTarget, NativeVideoBuildProfile, NativeVideoFrameRate,
    NativeVideoMediaMetadata, NativeVideoPresentationTarget,
};
use crate::Frontend;

#[test]
fn presentation_target_preserves_non_zero_gui_dimensions() {
    let target = NativeVideoPresentationTarget::from_frontend(Frontend::Gui {
        width: 1920,
        height: 1080,
    })
    .expect("valid GUI presentation target");

    assert_eq!(target.width(), 1920);
    assert_eq!(target.height(), 1080);
}

#[test]
fn only_known_optimized_neomacs_profiles_are_accepted() {
    let release = "Neomacs 0.0.16\nBuild: release for x86_64-unknown-linux-gnu with rustc 1.96.1";
    let debug = "Neomacs 0.0.16\nBuild: debug for x86_64-unknown-linux-gnu with rustc 1.96.1";

    assert_eq!(
        NativeVideoBuildProfile::from_version(release),
        Ok(NativeVideoBuildProfile::Release)
    );
    assert!(NativeVideoBuildProfile::from_version(debug).is_err());
    assert!(NativeVideoBuildProfile::from_version("Neomacs 0.0.16").is_err());
}

#[test]
fn media_contract_accepts_both_sixty_and_ntsc_sixty() {
    let metadata = |frame_rate| NativeVideoMediaMetadata {
        width_pixels: 3840,
        height_pixels: 2160,
        frame_rate,
        codec_caps: "video/x-h264".to_owned(),
    };

    assert!(
        metadata(NativeVideoFrameRate {
            numerator: 60,
            denominator: 1,
        })
        .validate_4k60()
        .is_ok()
    );
    assert!(
        metadata(NativeVideoFrameRate {
            numerator: 60_000,
            denominator: 1_001,
        })
        .validate_4k60()
        .is_ok()
    );
    assert!(
        metadata(NativeVideoFrameRate {
            numerator: 30,
            denominator: 1,
        })
        .validate_4k60()
        .is_err()
    );
    assert!(
        metadata(NativeVideoFrameRate {
            numerator: 120,
            denominator: 1,
        })
        .validate_4k60()
        .is_err()
    );
    let mut low_resolution = metadata(NativeVideoFrameRate {
        numerator: 60,
        denominator: 1,
    });
    low_resolution.width_pixels = 1920;
    assert!(low_resolution.validate_4k60().is_err());
}

#[test]
fn presentation_target_rejects_an_unrepresentable_size() {
    assert_eq!(
        NativeVideoPresentationTarget::from_frontend(Frontend::Gui {
            width: 0,
            height: 1080,
        }),
        Err(InvalidPresentationTarget::ZeroWidth)
    );
    assert_eq!(
        NativeVideoPresentationTarget::from_frontend(Frontend::Gui {
            width: 1920,
            height: 0,
        }),
        Err(InvalidPresentationTarget::ZeroHeight)
    );
    assert_eq!(
        NativeVideoPresentationTarget::from_frontend(Frontend::Batch),
        Err(InvalidPresentationTarget::NotGui)
    );
}

#[test]
fn persisted_media_identity_rejects_unknown_fields() {
    let json = r#"{
        "width_pixels": 3840,
        "height_pixels": 2160,
        "frame_rate": { "numerator": 60, "denominator": 1 },
        "codec_caps": "video/x-h264",
        "future_semantic_field": true
    }"#;

    let error = serde_json::from_str::<NativeVideoMediaMetadata>(json)
        .expect_err("unknown identity fields must fail closed");
    assert!(error.to_string().contains("unknown field"));
}
