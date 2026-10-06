use super::super::*;
use neomacs_app::font_queries::conversion::{
    core_font_otf_capability, core_opened_font_from_selection, font_otf_capability_for_file,
};

#[test]
fn selected_font_capability_reuse_preserves_the_complete_opened_font() {
    // Exercise native selection and the exact projection used by face-font,
    // without a window constructor, renderer or event loop.
    let mut metrics = FontMetricsService::new();
    let selected = metrics
        .select_font_for_char('M', "Monospace", 400, false, 16.0)
        .expect("native monospace selection");
    let expected = core_opened_font_from_selection(selected, font_otf_capability_for_file);
    for _ in 0..30 {
        let selected = metrics
            .select_font_for_char('M', "Monospace", 400, false, 16.0)
            .expect("same native selection");
        let cached = core_opened_font_from_selection(selected, |file, face_index| {
            metrics
                .otf_capability_for_file(file, face_index)
                .map(core_font_otf_capability)
        });
        assert_eq!(
            cached, expected,
            "names, metrics, identity and both OTF tables stay unchanged"
        );
    }
    metrics.clear_caches();
    let selected = metrics
        .select_font_for_char('M', "Monospace", 400, false, 16.0)
        .expect("selection after native invalidation");
    let reopened = core_opened_font_from_selection(selected, |file, face_index| {
        metrics
            .otf_capability_for_file(file, face_index)
            .map(core_font_otf_capability)
    });
    assert_eq!(reopened, expected);
}
