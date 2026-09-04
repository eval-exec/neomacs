use super::*;
use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer, GlyphRowRole};
use crate::core::types::{Color, DisplayWindowId};
use crate::render_thread::terminal_expansion::TerminalExpansion;
use crate::terminal::{
    TerminalDisplayTarget, TerminalFloatPlacement, TerminalGridSize, TerminalId, TerminalView,
};
use crate::thread_comm::ThreadComms;
use std::sync::{Arc, Mutex};

fn make_test_app() -> RenderApp {
    let comms = ThreadComms::new();
    let (_emacs, render) = comms.split();
    RenderApp::new(
        render,
        800,
        600,
        "test".to_owned(),
        Arc::new(crate::render_thread::ImageRenderState::default()),
        Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new())),
        true,
        crate::terminal::new_shared_terminals(),
    )
}

#[cfg(target_os = "linux")]
#[test]
fn terminal_scene_commands_schedule_repositioning_and_final_removal() {
    let mut app = make_test_app();
    let id = TerminalId::new(71).expect("nonzero terminal id");
    let size = TerminalGridSize::new(20, 5).expect("positive terminal grid");
    let view = TerminalView::new(id, size, TerminalDisplayTarget::Floating, Some("/bin/sh"))
        .expect("create real PTY shell");
    app.terminal_manager.terminals.insert(id, view);

    let primary = app
        .frame_windows
        .primary_window_mut()
        .expect("test app has a primary window");
    primary.render.set_current_frame(
        Some(FrameGlyphBuffer::with_size(800.0, 600.0)),
        None,
        Default::default(),
    );
    let retained_terminal_glyph = FrameGlyph::Border {
        window_id: DisplayWindowId::new(0),
        row_role: GlyphRowRole::ModeLine,
        clip_rect: None,
        x: 10.0,
        y: 10.0,
        width: 100.0,
        height: 50.0,
        color: Color::WHITE,
    };
    let _ = primary
        .render
        .replace_terminal_expansion(TerminalExpansion::new(
            vec![retained_terminal_glyph.clone()],
            Default::default(),
        ));
    primary.render.begin_presentable_render();

    app.handle_terminal(TerminalCommand::TerminalSetFloat {
        id,
        placement: TerminalFloatPlacement::new(24.0, 48.0, 0.75).expect("valid terminal placement"),
    });
    assert!(
        app.frame_windows
            .primary_window()
            .expect("test app has a primary window")
            .render
            .compositor
            .dirty,
        "terminal placement changes must schedule their replacement scene"
    );
    app.frame_windows
        .primary_window_mut()
        .expect("test app has a primary window")
        .render
        .begin_presentable_render();

    app.handle_terminal(TerminalCommand::TerminalDestroy { id });

    assert!(
        app.frame_windows
            .primary_window()
            .expect("test app has a primary window")
            .render
            .compositor
            .dirty,
        "terminal removal must request the repaint that clears its retained layer"
    );
    app.prepare_frame_state_for_render();
    let composed = app
        .frame_windows
        .primary_window()
        .expect("test app has a primary window")
        .render
        .current_frame_clone()
        .expect("composed editor frame");
    assert!(!composed.glyphs.contains(&retained_terminal_glyph));
}
