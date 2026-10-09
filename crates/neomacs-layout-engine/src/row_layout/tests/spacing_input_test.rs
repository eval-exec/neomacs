use super::*;
use neovm_core::emacs_core::{Context, Value};

#[test]
fn every_lisp_spacing_operand_is_rejected_intact() {
    let _eval = Context::new();
    let expression = Value::list(vec![Value::symbol("+"), Value::fixnum(2), Value::fixnum(3)]);
    let literal = DisplayStretchWidth::Length(DisplayLength::Pixels(16.0));
    for stretch in [
        DisplayStretch {
            width: DisplayStretchWidth::AlignTo(expression),
            height: None,
            ascent: None,
        },
        DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Expr(expression)),
            height: None,
            ascent: None,
        },
        DisplayStretch {
            width: literal.clone(),
            height: Some(DisplayLength::Expr(expression)),
            ascent: None,
        },
        DisplayStretch {
            width: literal,
            height: None,
            ascent: Some(DisplayLength::Expr(expression)),
        },
    ] {
        let item = DisplayItem::new(
            SourceSpan::synthetic(1, 0, 1),
            RenderFaceRef::Inherit,
            DisplayItemKind::Stretch(stretch),
        );
        let original = item.clone();
        assert_eq!(
            ResolvedSpacingInput::capture(item, FaceId::new(2)),
            Err(original)
        );
    }
}
