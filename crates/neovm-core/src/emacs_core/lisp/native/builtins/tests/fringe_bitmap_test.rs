//! Unit tests for the fringe-bitmap registry and `define-fringe-bitmap`.

use super::*;
use crate::emacs_core::Context;
use crate::emacs_core::value::Value;

/// `magit-fringe-bitmap>` collapsed-arrow rows (width 8). Stored MSB-aligned, so
/// `#b01100000` (= 0x60) becomes `0x6000`: columns 1 and 2 set.
const MAGIT_ARROW_GT: [u32; 8] = [
    0b01100000, 0b00110000, 0b00011000, 0b00001100, 0b00011000, 0b00110000, 0b01100000, 0b00000000,
];

#[test]
fn parse_bits_rows_is_msb_aligned_width_8() {
    let rows = parse_bits_rows(&MAGIT_ARROW_GT, 8);
    assert_eq!(rows.len(), 8);
    // 0x60 << (16 - 8) = 0x6000. Leftmost column (bit 15) is clear, columns 1,2 set.
    assert_eq!(rows[0], 0x6000);
    // Renderer reads column b as (bits >> (15 - b)) & 1.
    assert_eq!((rows[0] >> 15) & 1, 0, "column 0 clear");
    assert_eq!((rows[0] >> 14) & 1, 1, "column 1 set");
    assert_eq!((rows[0] >> 13) & 1, 1, "column 2 set");
    assert_eq!((rows[0] >> 12) & 1, 0, "column 3 clear");
}

#[test]
fn parse_bits_rows_uses_only_width_low_bits() {
    // Width 4: only the low 4 bits matter; high bits are masked off.
    let rows = parse_bits_rows(&[0b1111_1010], 4);
    // mask 0b1111 -> 0b1010, shifted up by 16-4 = 12 -> 0xA000.
    assert_eq!(rows[0], 0xA000);
    assert_eq!((rows[0] >> 15) & 1, 1, "col 0");
    assert_eq!((rows[0] >> 14) & 1, 0, "col 1");
    assert_eq!((rows[0] >> 13) & 1, 1, "col 2");
    assert_eq!((rows[0] >> 12) & 1, 0, "col 3");
}

#[test]
fn parse_bits_rows_width_16_keeps_all_bits() {
    let rows = parse_bits_rows(&[0xC003], 16);
    assert_eq!(rows[0], 0xC003);
}

#[test]
fn fit_rows_to_height_centers_when_taller() {
    let (rows, h) = fit_rows_to_height(vec![0x6000, 0x3000], Some(6));
    assert_eq!(h, 6);
    // 4 extra rows: fill1 = 2, fill2 = 2.
    assert_eq!(rows, vec![0, 0, 0x6000, 0x3000, 0, 0]);
}

#[test]
fn fit_rows_to_height_defaults_to_natural_length() {
    let (rows, h) = fit_rows_to_height(vec![0x6000, 0x3000, 0x1800], None);
    assert_eq!(h, 3);
    assert_eq!(rows.len(), 3);
}

fn define_via_eval(eval: &mut Context, form: &str) -> Value {
    eval.eval_str(form).expect("define-fringe-bitmap eval")
}

fn fringe_index(eval: &mut Context, symbol: &str) -> u32 {
    eval.eval_str(&format!("(get '{symbol} 'fringe)"))
        .expect("get fringe prop")
        .as_fixnum()
        .expect("index") as u32
}

#[test]
fn define_fringe_bitmap_stores_bits_and_returns_symbol() {
    let mut eval = Context::new();
    let result = define_via_eval(
        &mut eval,
        "(define-fringe-bitmap 'magit-fringe-bitmap> [#b01100000 #b00110000 #b00011000 \
         #b00001100 #b00011000 #b00110000 #b01100000 #b00000000])",
    );
    assert_eq!(result, Value::symbol("magit-fringe-bitmap>"));

    // The `'fringe` property stores the registry index.
    let prop = eval
        .eval_str("(get 'magit-fringe-bitmap> 'fringe)")
        .expect("get fringe prop");
    let index = prop.as_fixnum().expect("fringe property is an integer");
    // User bitmaps start at index 25.
    assert!(index >= 25, "user index {index} should be >= 25");

    // The registry has the bits, MSB-aligned, with default width 8, height 8.
    let bitmap = eval
        .fringe_bitmaps
        .get_by_index(index as u32)
        .expect("registry entry by index");
    assert_eq!(bitmap.width, 8);
    assert_eq!(bitmap.height, 8);
    assert_eq!(bitmap.bits.len(), 8);
    assert_eq!(bitmap.bits[0], 0x6000);
    assert_eq!(bitmap.period, 0);
}

#[test]
fn define_fringe_bitmap_string_bits_parse_msb_first() {
    let mut eval = Context::new();
    // A unibyte string row "\140" == 0x60; same as the vector form above.
    define_via_eval(
        &mut eval,
        "(define-fringe-bitmap 'test-str-bitmap \"\\140\\060\" nil 8)",
    );
    let index = eval
        .eval_str("(get 'test-str-bitmap 'fringe)")
        .expect("get fringe prop")
        .as_fixnum()
        .expect("index") as u32;
    let bitmap = eval.fringe_bitmaps.get_by_index(index).expect("entry");
    assert_eq!(bitmap.width, 8);
    assert_eq!(bitmap.bits[0], 0x6000, "0x60 -> MSB-aligned 0x6000");
    assert_eq!(bitmap.bits[1], 0x3000, "0x30 -> MSB-aligned 0x3000");
}

#[test]
fn define_fringe_bitmap_redefine_keeps_index() {
    let mut eval = Context::new();
    define_via_eval(
        &mut eval,
        "(define-fringe-bitmap 'redef-bitmap [#b10000000])",
    );
    let first = eval
        .eval_str("(get 'redef-bitmap 'fringe)")
        .expect("first index")
        .as_fixnum()
        .expect("first index");
    define_via_eval(
        &mut eval,
        "(define-fringe-bitmap 'redef-bitmap [#b11000000 #b11000000])",
    );
    let second = eval
        .eval_str("(get 'redef-bitmap 'fringe)")
        .expect("second index")
        .as_fixnum()
        .expect("second index");
    assert_eq!(first, second, "redefining keeps the same index");
    let bitmap = eval
        .fringe_bitmaps
        .get_by_index(second as u32)
        .expect("entry");
    assert_eq!(bitmap.bits.len(), 2);
}

#[test]
fn define_fringe_bitmap_align_top_and_bottom_parse() {
    let mut eval = Context::new();
    define_via_eval(
        &mut eval,
        "(define-fringe-bitmap 'top-bitmap [#b10000000] nil 8 'top)",
    );
    let top = fringe_index(&mut eval, "top-bitmap");
    assert_eq!(
        eval.fringe_bitmaps.get_by_index(top).expect("top").align,
        FringeBitmapAlign::Top
    );

    define_via_eval(
        &mut eval,
        "(define-fringe-bitmap 'bottom-bitmap [#b10000000] nil 8 'bottom)",
    );
    let bottom = fringe_index(&mut eval, "bottom-bitmap");
    assert_eq!(
        eval.fringe_bitmaps
            .get_by_index(bottom)
            .expect("bottom")
            .align,
        FringeBitmapAlign::Bottom
    );
}

#[test]
fn define_fringe_bitmap_periodic_align_sets_period() {
    let mut eval = Context::new();
    define_via_eval(
        &mut eval,
        "(define-fringe-bitmap 'periodic-bitmap [#b10101010 #b01010101] nil 8 '(top t))",
    );
    let index = fringe_index(&mut eval, "periodic-bitmap");
    let bitmap = eval.fringe_bitmaps.get_by_index(index).expect("entry");
    assert_eq!(bitmap.period, 2, "period == natural row count");
    assert_eq!(bitmap.height, 255, "periodic height forced to 255");
    assert_eq!(bitmap.bits.len(), 255);
}

#[test]
fn destroy_fringe_bitmap_removes_entry() {
    let mut eval = Context::new();
    define_via_eval(&mut eval, "(define-fringe-bitmap 'doomed [#b10000000])");
    let index = fringe_index(&mut eval, "doomed");
    assert!(eval.fringe_bitmaps.get_by_index(index).is_some());
    eval.eval_str("(destroy-fringe-bitmap 'doomed)")
        .expect("destroy");
    assert!(
        eval.fringe_bitmaps.get_by_index(index).is_none(),
        "destroyed bitmap removed from registry"
    );
    let prop = eval
        .eval_str("(get 'doomed 'fringe)")
        .expect("get prop after destroy");
    assert!(prop.is_nil(), "fringe property cleared");
}

// ---------------------------------------------------------------------------
// Stage 1: the 24 GNU standard built-in bitmaps are seeded into the registry.
// ---------------------------------------------------------------------------

/// `index_of` a standard bitmap symbol matches its fringe.c index.
#[test]
fn standard_bitmaps_index_of_matches_fringe_c() {
    let eval = Context::new();
    let idx = |name: &str| {
        let sym = crate::emacs_core::intern::intern(name);
        eval.fringe_bitmaps.index_of(sym)
    };
    // fringe.c standard_bitmaps[] order (slot 0 = NO_FRINGE_BITMAP).
    assert_eq!(idx("question-mark"), Some(1));
    assert_eq!(idx("right-arrow"), Some(4));
    assert_eq!(idx("up-arrow"), Some(5));
    assert_eq!(idx("large-circle"), Some(9));
    assert_eq!(idx("left-bracket"), Some(16));
    assert_eq!(idx("vertical-bar"), Some(22));
    assert_eq!(idx("horizontal-bar"), Some(23));
    assert_eq!(
        idx("empty-line"),
        Some(24),
        "empty-line is fringe.c index 24"
    );
}

/// The reverse map resolves each standard index back to its bitmap, and the
/// `'fringe` plist property is set to the same index (mirrors fringe.el's loop).
#[test]
fn standard_bitmaps_fringe_property_and_reverse_map() {
    let mut eval = Context::new();
    // `(get 'right-arrow 'fringe)` == 4.
    let prop = eval
        .eval_str("(get 'right-arrow 'fringe)")
        .expect("get fringe prop");
    assert_eq!(prop.as_fixnum(), Some(4));
    let empty = eval
        .eval_str("(get 'empty-line 'fringe)")
        .expect("get fringe prop");
    assert_eq!(empty.as_fixnum(), Some(24));
    // index -> bitmap reverse lookup works for a standard bitmap.
    assert!(
        eval.fringe_bitmaps.get_by_index(4).is_some(),
        "right-arrow resolvable by index"
    );
}

/// right-arrow bits/dims match fringe.c (MSB-aligned), align center, period 0.
#[test]
fn standard_right_arrow_bits_match_fringe_c() {
    let eval = Context::new();
    let bitmap = eval
        .fringe_bitmaps
        .get_by_index(4)
        .expect("right-arrow registered");
    assert_eq!(bitmap.width, 8);
    assert_eq!(bitmap.height, 8);
    assert_eq!(bitmap.period, 0);
    assert_eq!(bitmap.align, FringeBitmapAlign::Center);
    // fringe.c right_arrow_bits = {0x18,0x0c,0x06,0x3f,0x3f,0x06,0x0c,0x18},
    // MSB-aligned by << (16-8) = << 8.
    let expected: Vec<u16> = [0x18u16, 0x0c, 0x06, 0x3f, 0x3f, 0x06, 0x0c, 0x18]
        .iter()
        .map(|r| r << 8)
        .collect();
    assert_eq!(bitmap.bits, expected);
}

/// up-arrow aligns TOP; down-arrow aligns BOTTOM (fringe.c).
#[test]
fn standard_arrow_alignments_match_fringe_c() {
    let eval = Context::new();
    assert_eq!(
        eval.fringe_bitmaps.get_by_index(5).expect("up-arrow").align,
        FringeBitmapAlign::Top
    );
    assert_eq!(
        eval.fringe_bitmaps
            .get_by_index(6)
            .expect("down-arrow")
            .align,
        FringeBitmapAlign::Bottom
    );
}

/// empty-line is PERIODIC (period 3), height 72, with the repeating 0x3c tile.
#[test]
fn standard_empty_line_is_periodic() {
    let eval = Context::new();
    let bitmap = eval
        .fringe_bitmaps
        .get_by_index(24)
        .expect("empty-line registered");
    assert_eq!(bitmap.period, 3, "empty-line period is 3");
    assert_eq!(bitmap.height, 72);
    assert_eq!(bitmap.bits.len(), 72);
    assert_eq!(bitmap.align, FringeBitmapAlign::Top);
    // 3-row tile: blank, 0x3c (-> 0x3c00 MSB-aligned), blank.
    assert_eq!(bitmap.bits[0], 0x0000);
    assert_eq!(bitmap.bits[1], 0x3c00);
    assert_eq!(bitmap.bits[2], 0x0000);
    // Tile repeats.
    assert_eq!(bitmap.bits[4], 0x3c00);
}

/// User bitmaps stay ABOVE the standard range: the first `define-fringe-bitmap`
/// gets index 25, never colliding with a seeded standard bitmap.
#[test]
fn user_bitmaps_stay_above_standard_range() {
    let mut eval = Context::new();
    eval.eval_str("(define-fringe-bitmap 'vm-above-std [#b10000000])")
        .expect("define");
    let index = eval
        .eval_str("(get 'vm-above-std 'fringe)")
        .expect("get fringe prop")
        .as_fixnum()
        .expect("index");
    assert_eq!(
        index, 25,
        "first user bitmap is index 25 (above standard 1..24)"
    );
}

/// Redefining a STANDARD bitmap symbol keeps its standard index (GNU replaces
/// the existing definition in place).
#[test]
fn redefining_standard_bitmap_keeps_standard_index() {
    let mut eval = Context::new();
    eval.eval_str("(define-fringe-bitmap 'right-arrow [#b11000000 #b11000000])")
        .expect("redefine right-arrow");
    let index = eval
        .eval_str("(get 'right-arrow 'fringe)")
        .expect("get fringe prop")
        .as_fixnum()
        .expect("index");
    assert_eq!(
        index, 4,
        "right-arrow keeps fringe.c index 4 after redefine"
    );
    let bitmap = eval.fringe_bitmaps.get_by_index(4).expect("entry");
    assert_eq!(bitmap.bits.len(), 2, "geometry replaced in place");
}

// ---------------------------------------------------------------------------
// Stage 2: `fringe-bitmaps` is bound so fringe.el's standard-bitmap seeding +
// indicator/cursor-alist defaults install. The full-lisp bootstrap path is
// exercised by `tests/fringe_standard_indicators.rs`; here we verify the gating
// `fringe-bitmaps` binding exists and that fringe.el's exact `setq-default`
// forms install the GNU defaults whose symbols resolve to registered indices.
// ---------------------------------------------------------------------------

/// `fringe-bitmaps` is bound (nil) in a fresh Context, so fringe.el's
/// `(boundp 'fringe-bitmaps)`-guarded seeding/alist block runs at load time.
/// This is the gate for Stage 2: without it, fringe.el installs neither the
/// standard-bitmap `'fringe` indices nor the indicator/cursor-alist defaults.
#[test]
fn fringe_bitmaps_variable_is_bound() {
    let mut eval = Context::new();
    let bound = eval.eval_str("(boundp 'fringe-bitmaps)").expect("boundp");
    assert!(
        bound.is_truthy(),
        "fringe-bitmaps must be bound (GNU binds it nil) so fringe.el's seeding runs"
    );
}

/// Every physical bitmap symbol that the GNU default `fringe-indicator-alist` /
/// `fringe-cursor-alist` reference (fringe.el ~65-84) resolves to a registered
/// standard bitmap index. (The alist VALUES are installed by fringe.el in the
/// full-lisp runtime — see `tests/fringe_standard_indicators.rs`; here we just
/// confirm the referenced bitmaps all exist so those alists are not dangling.)
#[test]
fn indicator_and_cursor_alist_referenced_bitmaps_are_registered() {
    let eval = Context::new();
    for sym in [
        // fringe-indicator-alist physical bitmaps:
        "left-arrow",
        "right-arrow",
        "left-curly-arrow",
        "right-curly-arrow",
        "right-triangle",
        "up-arrow",
        "down-arrow",
        "top-left-angle",
        "top-right-angle",
        "bottom-left-angle",
        "bottom-right-angle",
        "left-bracket",
        "right-bracket",
        "empty-line",
        "question-mark",
        // fringe-cursor-alist physical bitmaps:
        "filled-rectangle",
        "hollow-rectangle",
        "vertical-bar",
        "horizontal-bar",
        "hollow-square",
    ] {
        let s = crate::emacs_core::intern::intern(sym);
        assert!(
            eval.fringe_bitmaps.index_of(s).is_some(),
            "{sym} referenced by the default fringe alists must be a registered bitmap"
        );
    }
}

#[test]
fn set_fringe_bitmap_face_records_override() {
    let mut eval = Context::new();
    define_via_eval(&mut eval, "(define-fringe-bitmap 'faced [#b10000000])");
    let index = fringe_index(&mut eval, "faced");
    eval.eval_str("(set-fringe-bitmap-face 'faced 'magit-section-heading)")
        .expect("set-fringe-bitmap-face");
    let bitmap = eval.fringe_bitmaps.get_by_index(index).expect("entry");
    assert_eq!(bitmap.face.as_deref(), Some("magit-section-heading"));

    // A subsequent geometry-only redefinition preserves the face override.
    define_via_eval(
        &mut eval,
        "(define-fringe-bitmap 'faced [#b11000000 #b11000000])",
    );
    let bitmap = eval.fringe_bitmaps.get_by_index(index).expect("entry");
    assert_eq!(
        bitmap.face.as_deref(),
        Some("magit-section-heading"),
        "redefining geometry keeps the set-fringe-bitmap-face override"
    );
}
