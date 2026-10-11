use super::WindowChromeStringState;
use neomacs_display_protocol::GlyphStringId;
use neovm_core::emacs_core::{Context, Value};
use neovm_core::window::{PresentedWindowChromeArea, PresentedWindowChromeString};

fn area_sources(
    evaluator: &Context,
    area: PresentedWindowChromeArea,
    spelling: &str,
) -> Vec<PresentedWindowChromeString> {
    let value = Value::string(spelling);
    // SAFETY: the value was freshly allocated on this installed evaluator's
    // mutator, with no intervening collection. The tests keep its Context
    // alive and seed admitted root leases during their explicit collections.
    let roots = unsafe { evaluator.share_values(&[value]) }.unwrap();
    roots
        .into_iter()
        .map(|root| PresentedWindowChromeString::new(area, GlyphStringId::new(1), root))
        .collect()
}

#[test]
fn reused_chrome_preserves_frozen_storage_and_its_existing_root_lease() {
    let mut evaluator = Box::new(Context::new());
    evaluator.setup_thread_locals();
    let mut building = area_sources(&evaluator, PresentedWindowChromeArea::ModeLine, "mode");
    building.extend(area_sources(
        &evaluator,
        PresentedWindowChromeArea::HeaderLine,
        "header",
    ));
    let frozen = WindowChromeStringState::Building(building).finish(&evaluator);
    let reused = WindowChromeStringState::ReusedFrozen(frozen.clone()).finish(&evaluator);
    assert_eq!(frozen.as_slice().as_ptr(), reused.as_slice().as_ptr());
    assert!(reused[0].object().shares_backing_root(frozen[0].object()));
    drop(frozen);
    evaluator.gc_collect_exact();
    for source in reused.iter() {
        assert!(evaluator.materialize(source.object()).is_ok());
    }
}

#[test]
fn replacing_one_chrome_area_thaws_only_the_new_builder() {
    let mut evaluator = Box::new(Context::new());
    evaluator.setup_thread_locals();
    let mut building = area_sources(&evaluator, PresentedWindowChromeArea::ModeLine, "mode");
    building.extend(area_sources(
        &evaluator,
        PresentedWindowChromeArea::HeaderLine,
        "old header",
    ));
    let frozen = WindowChromeStringState::Building(building).finish(&evaluator);
    let mut changed = WindowChromeStringState::ReusedFrozen(frozen.clone());
    changed.replace_area(
        PresentedWindowChromeArea::HeaderLine,
        area_sources(
            &evaluator,
            PresentedWindowChromeArea::HeaderLine,
            "new header",
        ),
    );
    assert!(matches!(&changed, WindowChromeStringState::Building(_)));
    let changed = changed.finish(&evaluator);
    assert_ne!(frozen.as_slice().as_ptr(), changed.as_slice().as_ptr());
    assert_eq!(changed.len(), 2);
    assert_eq!(changed[0].area(), PresentedWindowChromeArea::ModeLine);
    assert_eq!(changed[1].area(), PresentedWindowChromeArea::HeaderLine);
    assert!(changed[0].object().is_same_object(frozen[0].object()));
    assert!(!changed[1].object().is_same_object(frozen[1].object()));
    assert!(changed[0].object().shares_backing_root(changed[1].object()));
    evaluator.gc_collect_exact();
    for source in frozen.iter().chain(changed.iter()) {
        assert!(evaluator.materialize(source.object()).is_ok());
    }
}
