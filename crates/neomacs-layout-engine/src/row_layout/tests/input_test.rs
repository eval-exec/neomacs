use super::*;
use crate::display_item::{DisplayLength, DisplayStretch, DisplayStretchWidth, RenderFaceRef};

#[test]
fn owned_text_resolves_inheritance_and_moves_across_threads() {
    fn require_send_sync<T: Send + Sync + 'static>() {}
    require_send_sync::<ResolvedTextInput>();
    let source = SourceSpan::synthetic(4, 0, 3);
    let input = ResolvedTextInput::capture(
        DisplayItem::new(
            source.clone(),
            RenderFaceRef::Inherit,
            DisplayItemKind::TextRun(DisplayTextRun::independent("aλ中")),
        ),
        FaceId::new(7),
    )
    .expect("ordinary text is supported");
    let input = std::thread::spawn(move || input).join().unwrap();
    assert_eq!(input.face, FaceId::new(7));
    assert_eq!(input.span, source);
    assert_eq!(input.run.text.as_ref(), "aλ中");
}

#[test]
fn non_text_input_returns_unchanged_for_synchronous_processing() {
    let item = DisplayItem::new(
        SourceSpan::synthetic(2, 0, 1),
        RenderFaceRef::Inherit,
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Pixels(12.0)),
            height: None,
            ascent: None,
        }),
    );
    let original = item.clone();
    assert_eq!(
        ResolvedTextInput::capture(item, FaceId::new(7)),
        Err(original)
    );
}
