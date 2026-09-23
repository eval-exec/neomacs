use super::*;
use neovm_core::emacs_core::Context;
use std::collections::HashMap;

/// Construct a realistic test context: 800x600 window, 10px char
/// width, 20px line height, 8px left fringe, 0 margins, 0 scroll
/// bar, fringes inside margins.
fn test_ctx() -> PixelCalcContext {
    PixelCalcContext {
        frame_column_width: 10.0,
        frame_line_height: 20.0,
        frame_res_x: 96.0,
        frame_res_y: 96.0,
        face_font_height: 20.0,
        face_font_width: 10.0,
        // Text area runs from x=8 (after left fringe) to x=792 (before
        // right fringe). width = 784.
        text_area_left: 8.0,
        text_area_right: 792.0,
        text_area_width: 784.0,
        // Left margin: x=0, width=0. Right margin: x=792, width=0.
        left_margin_left: 0.0,
        left_margin_width: 0.0,
        right_margin_left: 792.0,
        right_margin_width: 0.0,
        left_fringe_width: 8.0,
        right_fringe_width: 8.0,
        fringes_outside_margins: false,
        scroll_bar_width: 0.0,
        scroll_bar_on_left: false,
        line_number_pixel_width: 0.0,
        symbol_values: HashMap::new(),
        image_sizes: PixelCalcImageSizes::default(),
    }
}

/// Parse an Elisp expression via a fresh Context.
fn parse(src: &str) -> (Context, Value) {
    let mut ctx = Context::new();
    let v = ctx
        .eval_str(&format!("(quote {})", src))
        .expect("parse should succeed");
    (ctx, v)
}

#[test]
fn pixel_calc_domains_match_gnu_symbols() {
    assert_eq!(
        PixelCalcUnit::from_symbol_name("in"),
        Some(PixelCalcUnit::Inch)
    );
    assert_eq!(
        PixelCalcUnit::from_symbol_name("mm"),
        Some(PixelCalcUnit::Mm)
    );
    assert_eq!(
        PixelCalcUnit::from_symbol_name("cm"),
        Some(PixelCalcUnit::Cm)
    );
    assert_eq!(PixelCalcUnit::from_symbol_name("pt"), None);
    assert_eq!(<&'static str>::from(PixelCalcUnit::Inch), "in");

    assert_eq!(
        PixelCalcSymbol::from_symbol_name("left-fringe"),
        Some(PixelCalcSymbol::LeftFringe)
    );
    assert_eq!(
        PixelCalcSymbol::from_symbol_name("scroll-bar"),
        Some(PixelCalcSymbol::ScrollBar)
    );
    assert_eq!(PixelCalcSymbol::from_symbol_name("image"), None);
    assert_eq!(
        <&'static str>::from(PixelCalcSymbol::RightMargin),
        "right-margin"
    );

    assert_eq!(
        PixelCalcConsHead::from_symbol_name("image"),
        Some(PixelCalcConsHead::Image)
    );
    assert_eq!(
        PixelCalcConsHead::from_symbol_name("+"),
        Some(PixelCalcConsHead::Plus)
    );
    assert_eq!(
        PixelCalcConsHead::from_symbol_name("-"),
        Some(PixelCalcConsHead::Minus)
    );
    assert_eq!(PixelCalcConsHead::from_symbol_name("plus"), None);
    assert_eq!(<&'static str>::from(PixelCalcConsHead::Xwidget), "xwidget");
}

// ------------------------------------------------------------------
// Nil
// ------------------------------------------------------------------
#[test]
fn nil_returns_zero() {
    let ctx = test_ctx();
    let r = calc_pixel_width_or_height(&ctx, &Value::NIL, true, None);
    assert_eq!(r, Some(0.0));
}

// ------------------------------------------------------------------
// Numbers
// ------------------------------------------------------------------
#[test]
fn fixnum_width_scales_by_column_width() {
    let ctx = test_ctx();
    let v = Value::fixnum(3);
    // 3 × frame_column_width = 30
    assert_eq!(calc_pixel_width_or_height(&ctx, &v, true, None), Some(30.0));
}

#[test]
fn fixnum_height_scales_by_line_height() {
    let ctx = test_ctx();
    let v = Value::fixnum(3);
    // 3 × frame_line_height = 60
    assert_eq!(
        calc_pixel_width_or_height(&ctx, &v, false, None),
        Some(60.0)
    );
}

#[test]
fn float_width_scales_by_column_width() {
    // Value::make_float requires a tagged heap on the thread;
    // install one via a fresh Context.
    let (_keep, v) = parse("1.5");
    let ctx = test_ctx();
    // 1.5 × 10 = 15
    assert_eq!(calc_pixel_width_or_height(&ctx, &v, true, None), Some(15.0));
}

// ------------------------------------------------------------------
// Symbols: height / width / text
// ------------------------------------------------------------------
#[test]
fn height_symbol_returns_face_font_height() {
    let ctx = test_ctx();
    let v = Value::symbol("height");
    assert_eq!(calc_pixel_width_or_height(&ctx, &v, true, None), Some(20.0));
}

#[test]
fn width_symbol_returns_face_font_width() {
    let ctx = test_ctx();
    let v = Value::symbol("width");
    assert_eq!(calc_pixel_width_or_height(&ctx, &v, true, None), Some(10.0));
}

#[test]
fn text_symbol_returns_text_area_width_minus_lnum() {
    let ctx = test_ctx();
    let v = Value::symbol("text");
    // text_area_width (784) - lnum_pixel_width (0) = 784
    assert_eq!(
        calc_pixel_width_or_height(&ctx, &v, true, None),
        Some(784.0)
    );
}

// ------------------------------------------------------------------
// Symbols: units (in/mm/cm)
// ------------------------------------------------------------------
#[test]
fn inch_unit_returns_dpi_pixels() {
    let ctx = test_ctx();
    let v = Value::symbol("in");
    // frame_res_x / 1.0 = 96
    assert_eq!(calc_pixel_width_or_height(&ctx, &v, true, None), Some(96.0));
}

#[test]
fn cm_unit_returns_dpi_over_2_54() {
    let ctx = test_ctx();
    let v = Value::symbol("cm");
    assert!(
        (calc_pixel_width_or_height(&ctx, &v, true, None).unwrap() - (96.0 / 2.54)).abs() < 0.01
    );
}

// ------------------------------------------------------------------
// Symbols: align-to position (right/left/center)
// ------------------------------------------------------------------
#[test]
fn right_symbol_in_align_to_mode_returns_text_right() {
    let ctx = test_ctx();
    let v = Value::symbol("right");
    let mut align = -1_i32;
    let px = calc_pixel_width_or_height(&ctx, &v, true, Some(&mut align));
    // First-eval symbols write the position to *align_to and return 0.
    assert_eq!(px, Some(0.0));
    assert_eq!(align, ctx.text_area_right as i32);
}

#[test]
fn left_symbol_in_align_to_mode_returns_text_left_plus_lnum() {
    let ctx = test_ctx();
    let v = Value::symbol("left");
    let mut align = -1_i32;
    let px = calc_pixel_width_or_height(&ctx, &v, true, Some(&mut align));
    assert_eq!(px, Some(0.0));
    assert_eq!(
        align,
        (ctx.text_area_left + ctx.line_number_pixel_width) as i32
    );
}

#[test]
fn center_symbol_in_align_to_mode_returns_midpoint() {
    let ctx = test_ctx();
    let v = Value::symbol("center");
    let mut align = -1_i32;
    let px = calc_pixel_width_or_height(&ctx, &v, true, Some(&mut align));
    assert_eq!(px, Some(0.0));
    // text_area_left (8) + 0 + text_area_width/2 (392) = 400
    assert_eq!(align, 400);
}

// ------------------------------------------------------------------
// Symbols: width mode (non-first-time) treats same symbols as widths
// ------------------------------------------------------------------
#[test]
fn right_fringe_symbol_in_width_mode_returns_fringe_width() {
    let ctx = test_ctx();
    let v = Value::symbol("right-fringe");
    // In width mode (align_to = None), right-fringe returns the fringe
    // WIDTH, not its position.
    assert_eq!(calc_pixel_width_or_height(&ctx, &v, true, None), Some(8.0));
}

#[test]
fn left_margin_symbol_in_width_mode_returns_margin_width() {
    let ctx = test_ctx();
    let v = Value::symbol("left-margin");
    assert_eq!(calc_pixel_width_or_height(&ctx, &v, true, None), Some(0.0));
}

#[test]
fn unknown_symbol_falls_through_to_buffer_local_value_like_gnu() {
    let mut ctx = test_ctx();
    ctx.symbol_values
        .insert("header-line-indent-width".to_string(), Value::fixnum(0));
    let (_keep, v) = parse("(+ header-line-indent-width 1)");
    let mut align = -1_i32;
    assert_eq!(
        calc_pixel_width_or_height(&ctx, &v, true, Some(&mut align)),
        Some(10.0)
    );
}

// ------------------------------------------------------------------
// Cons: (+ E...) and (- E...)
// ------------------------------------------------------------------
#[test]
fn plus_of_two_numbers_in_width_mode() {
    let ctx = test_ctx();
    // (+ 3 4) = 3 columns + 4 columns = (3 + 4) * 10 = 70
    let (_keep, v) = parse("(+ 3 4)");
    assert_eq!(calc_pixel_width_or_height(&ctx, &v, true, None), Some(70.0));
}

#[test]
fn minus_of_two_numbers_in_width_mode() {
    let ctx = test_ctx();
    // (- 5 3) = 5 cols - 3 cols = (5 - 3) * 10 = 20
    let (_keep, v) = parse("(- 5 3)");
    assert_eq!(calc_pixel_width_or_height(&ctx, &v, true, None), Some(20.0));
}

#[test]
fn minus_of_single_number_matches_gnu_double_negate() {
    let ctx = test_ctx();
    // GNU edge case: `(- 5)` with a SINGLE argument double-negates
    // because of how xdisp.c:30286 sets pixels=-px on the first
    // iteration AND then applies `pixels = -pixels` at the end
    // for the minus head. For the single-arg case this cancels
    // out and returns +50 (not -50 as one might expect from Elisp
    // arithmetic). The edge case is irrelevant in practice — every
    // `:align-to`/`:width` form we have seen uses multi-arg minus
    // like `(- right N)`. We match GNU exactly.
    let (_keep, v) = parse("(- 5)");
    assert_eq!(calc_pixel_width_or_height(&ctx, &v, true, None), Some(50.0));
}

// ------------------------------------------------------------------
// Cons: (NUM) — absolute pixel count
// ------------------------------------------------------------------
#[test]
fn paren_num_returns_literal_pixels() {
    let ctx = test_ctx();
    // (200) = 200 pixels literal, no column scaling
    let (_keep, v) = parse("(200)");
    assert_eq!(
        calc_pixel_width_or_height(&ctx, &v, true, None),
        Some(200.0)
    );
}

// ------------------------------------------------------------------
// Cons: (NUM . UNIT)
// ------------------------------------------------------------------
#[test]
fn num_dot_width_unit_scales_by_face_font_width() {
    let ctx = test_ctx();
    // (2 . width) = 2 * face_font_width = 2 * 10 = 20
    let (_keep, v) = parse("(2 . width)");
    assert_eq!(calc_pixel_width_or_height(&ctx, &v, true, None), Some(20.0));
}

// ------------------------------------------------------------------
// Doom-modeline form: (- right (200))
// ------------------------------------------------------------------
#[test]
fn doom_modeline_right_minus_paren_literal() {
    let ctx = test_ctx();
    // `right` resolves to text_area_right (792) on first eval, then
    // inside the recursive minus, `(200)` returns 200 pixels literal.
    // Result: align_to = 792 - 200 = 592.
    //
    // The caller's alignment sentinel starts at -1; after the call,
    // we expect it to have been updated along the way.
    let (_keep, v) = parse("(- right (200))");
    let mut align = -1_i32;
    let r = calc_pixel_width_or_height(&ctx, &v, true, Some(&mut align));
    // The outer result is the computed offset from the align_to
    // base. For doom-modeline this is what the caller adds to
    // current_x to reach the target position. With our context
    // (right=792, literal=200):
    //   first iter: px = 0 (right wrote 792 to align_to, returned 0)
    //   second iter: px = 200 (literal), subtracted: 0 + (-200)... wait.
    //
    // Hmm. Let me work through this more carefully against GNU:
    //   cdr = (right (200))
    //   iter 1 (right): px = 0, align_to becomes 792
    //     first=true → pixels = -0 = 0 (minus head)
    //   iter 2 ((200)): px = 200 (no align_to offset since align_to >= 0)
    //     not first → pixels += 200 → pixels = 200
    //   end of loop: minus ⇒ pixels = -200
    //
    // Hmm. That gives -200, not 792-200=592.
    //
    // GNU's stretch-glyph producer uses the RESOLVED align_to value
    // as the target x, and the returned `res` as extra stretch.
    // So target_x = align_to (792) and extra = res (-200)?
    //
    // Re-reading xdisp.c:32585:
    //   int x = it->current_x + ...;
    //   int x0 = x;
    //   ...
    //   if (it->glyph_row == NULL || !it->glyph_row->mode_line_p)
    //     align_to = (align_to < 0 ? 0 : align_to);
    //   else if (align_to < 0)
    //     align_to = x;
    //   ...
    //   if (align_to < 0)
    //     align_to = x + (int)tem;
    //   ...
    //   width = max (0, align_to - x);
    //
    // So the convention is: align_to ends up as an absolute pixel
    // position (the target x). The width of the stretch is
    // max(0, align_to - current_x). The `res` return value is used
    // only when align_to < 0 (width mode, no base).
    //
    // For `(- right (200))`:
    //   - align_to starts at -1
    //   - iter 1: `right` sets align_to = 792, returns res = 0
    //   - iter 2: `(200)` returns res = 200, align_to stays 792
    //     (it's no longer < 0)
    //   - plus/minus accumulator: first=0 (negated), then +200 → 200
    //   - minus negation at end: -200
    //
    // So the function returns -200 with align_to = 792. The caller
    // then computes target_x = align_to + res = 792 + (-200) = 592.
    //
    // That matches doom's intent.
    //
    // So the test should verify: align_to becomes 792 (updated by
    // `right`), and the returned pixels is -200.
    assert_eq!(align, 792);
    assert_eq!(r, Some(-200.0));
    // The caller resolves the final target position:
    let target_x = align + (r.unwrap() as i32);
    assert_eq!(target_x, 592);
}

// ------------------------------------------------------------------
// Regression: fixnum :align-to behaves like the old parser
// ------------------------------------------------------------------
#[test]
fn fixnum_align_to_in_align_mode_writes_column_offset() {
    let ctx = test_ctx();
    // A bare fixnum in align-to mode: n * column_width + lnum_pixel_width.
    // With lnum_pixel_width = 0 and column_width = 10:
    //   `5` in align-to mode → 5 * 10 = 50 pixels
    // The old parser at engine.rs:527 did: content_x + n * char_w.
    // Our helper returns the pixels portion; the caller is
    // responsible for adding content_x (or using align_to as target).
    let v = Value::fixnum(5);
    let mut align = -1_i32;
    assert_eq!(
        calc_pixel_width_or_height(&ctx, &v, true, Some(&mut align)),
        Some(50.0)
    );
}

/// Issue #204 — `(- center (0.5 . IMAGE-SPEC))` is how Dashboard and
/// `image-dired` centre a banner.
///
/// GNU evaluates `(image …)` to the image's own pixel width (xdisp.c:30506,
/// via `lookup_image`) and `(NUM . EXPR)` to NUM × EXPR (xdisp.c:30551), so a
/// 400px-wide image resolves to `centre − 200`. Answering the image operand
/// with a fixed placeholder mis-centred every such banner.
#[test]
fn image_operand_uses_resolved_image_width() {
    let mut ctx = test_ctx();
    let (_c, prop) = parse(r#"(- center (0.5 . (image :type png :file "x.png")))"#);
    let (_c2, image) = parse(r#"(image :type png :file "x.png")"#);
    ctx.image_sizes.insert(image, 400.0, 300.0);

    let mut align_to: i32 = -1;
    let pixels = calc_pixel_width_or_height(&ctx, &prop, true, Some(&mut align_to));

    // `center` = text_area_left (8) + text_area_width/2 (392) = 400.
    assert_eq!(align_to, 400);
    assert_eq!(
        pixels,
        Some(-200.0),
        "half of the resolved 400px image width"
    );
}

/// GNU guards the image arm with `FRAME_WINDOW_P` (xdisp.c:30506): on a
/// terminal frame no arm matches an `(image …)` head, so the whole expression
/// fails and `:align-to` goes unapplied. With no resolved size we must do the
/// same rather than invent a width.
#[test]
fn image_operand_without_resolved_size_fails_like_gnu_on_tty() {
    let ctx = test_ctx();
    let (_c, prop) = parse(r#"(- center (0.5 . (image :type png :file "x.png")))"#);

    let mut align_to: i32 = -1;
    assert_eq!(
        calc_pixel_width_or_height(&ctx, &prop, true, Some(&mut align_to)),
        None
    );
}
