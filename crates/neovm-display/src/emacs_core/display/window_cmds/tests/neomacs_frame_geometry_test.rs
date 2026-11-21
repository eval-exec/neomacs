//! `neomacs-frame-geometry` / `neomacs-frame-edges` on a GUI frame.
//!
//! GNU's `frame-geometry` doc and `Fx_frame_geometry` (src/xfns.c:6778-6958)
//! define three nested rectangles: `outer` includes the window-manager
//! decorations and the external menu/tool bars, `native` excludes those bars,
//! and `inner` additionally excludes the internal border and the tab bar.
//! Neomacs draws the bars inside its own window, so these primitives derive
//! the same rectangles from the stored physical size and realized bar heights.

use neovm_value::value::Value;

use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::print::print_value;

use crate::window::DisplayFrameManagerExt;

fn gui_frame_context() -> (Context, neovm_value::display_vocab::FrameId) {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let buffer = ctx.edit.buffers.create_buffer("*frame-geometry*");
    ctx.edit.buffers.set_current(buffer);
    let frame_id = ctx.frame_manager_mut().create_frame("F1", 832, 692, buffer);
    let frame = ctx.frame_manager_mut().get_mut(frame_id).expect("frame");
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.displays_chrome = true;
    frame
        .parameters
        .insert(Value::symbol("internal-border-width"), Value::fixnum(2));
    // Realized geometry after the parameter write: 17 + 34 external bars, a
    // 20 px tab bar inside the native frame, and a 2 px internal border.
    frame.menu_bar_height = 17;
    frame.tool_bar_height = 34;
    frame.tab_bar_height = 20;
    (ctx, frame_id)
}

fn answer(ctx: &mut Context, form: &str) -> String {
    print_value(&ctx.eval_str(form).expect("frame geometry form"))
}

#[test]
fn frame_edges_nest_outer_native_and_inner_like_gnu() {
    let (mut ctx, frame_id) = gui_frame_context();
    let frame = ctx.frame_manager().get(frame_id).expect("frame");
    assert_eq!(
        (
            frame.menu_bar_height,
            frame.tool_bar_height,
            frame.tab_bar_height
        ),
        (17, 34, 20),
        "setup keeps the realized bars"
    );
    // 692 = 51 external + 20 tab + 2*2 border + 617 text area.
    assert_eq!(
        answer(&mut ctx, "(neomacs-frame-edges nil 'outer-edges)"),
        "(0 0 832 692)"
    );
    // Native edges start below the external menu/tool band but keep the tab
    // bar and the internal border.
    assert_eq!(
        answer(&mut ctx, "(neomacs-frame-edges nil 'native-edges)"),
        "(0 51 832 692)"
    );
    assert_eq!(
        answer(&mut ctx, "(neomacs-frame-edges)"),
        "(0 51 832 692)",
        "an absent TYPE means native edges, like Fx_frame_edges"
    );
    // Inner edges skip the band, the internal border and the tab bar.
    assert_eq!(
        answer(&mut ctx, "(neomacs-frame-edges nil 'inner-edges)"),
        "(2 73 830 690)"
    );
}

#[test]
fn frame_geometry_counts_the_external_bar_band_without_moving_it() {
    let (mut ctx, _frame_id) = gui_frame_context();
    // The alist is GNU's: outer-size includes the bars, and menu-bar-size /
    // tool-bar-size are the frame's realized heights.
    assert_eq!(
        answer(&mut ctx, "(assq 'outer-size (neomacs-frame-geometry))"),
        "(outer-size 832 . 692)"
    );
    assert_eq!(
        answer(&mut ctx, "(assq 'menu-bar-size (neomacs-frame-geometry))"),
        "(menu-bar-size 832 . 17)"
    );
    assert_eq!(
        answer(&mut ctx, "(assq 'tool-bar-size (neomacs-frame-geometry))"),
        "(tool-bar-size 832 . 34)"
    );
    assert_eq!(
        answer(&mut ctx, "(assq 'tab-bar-size (neomacs-frame-geometry))"),
        "(tab-bar-size 828 . 20)"
    );
    assert_eq!(
        answer(
            &mut ctx,
            "(assq 'internal-border-width (neomacs-frame-geometry))"
        ),
        "(internal-border-width . 2)"
    );
    assert_eq!(
        answer(
            &mut ctx,
            "(list (assq 'menu-bar-external (neomacs-frame-geometry))
                   (assq 'tool-bar-external (neomacs-frame-geometry))
                   (assq 'tool-bar-position (neomacs-frame-geometry)))"
        ),
        "((menu-bar-external . t) (tool-bar-external . t) (tool-bar-position . top))"
    );
    // The native frame is the physical frame without the external band, so it
    // keeps counting exactly that band as a difference.
    assert_eq!(
        answer(&mut ctx, "(frame-native-height)"),
        "641",
        "692 physical - 51 external bars; the tab bar and border stay inside"
    );
}

#[test]
fn frame_geometry_and_edges_are_nil_without_a_window_system() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let buffer = ctx.edit.buffers.create_buffer("*frame-geometry-tty*");
    ctx.edit.buffers.set_current(buffer);
    ctx.frame_manager_mut().create_frame("F1", 80, 25, buffer);
    assert!(ctx.eval_str("(neomacs-frame-geometry)").unwrap().is_nil());
    assert!(
        ctx.eval_str("(neomacs-frame-edges nil 'outer-edges)")
            .unwrap()
            .is_nil()
    );
}
